//! Supervision of a single task: starting, restarting and stopping its process.

use std::collections::VecDeque;
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::process::ExitStatus;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anymon_shell::{describe_exit, spawn, Process, SpawnOptions};
use tokio::sync::{mpsc, oneshot};

use crate::paths::display_relative;
use crate::plan::{Plan, TaskPlan};
use crate::ui::format_duration;

/// Maximum number of changed paths passed in `ANYMON_CHANGED_PATHS`.
const MAX_CHANGED_PATHS: usize = 100;

/// Why a task (re)starts.
#[derive(Debug, Clone)]
pub(crate) enum Trigger {
    /// anymon just started.
    Startup,
    /// Watched files changed.
    Changed(Vec<PathBuf>),
    /// The user asked for a restart.
    Manual,
}

/// Messages sent to a task actor.
#[derive(Debug)]
pub(crate) enum TaskCommand {
    Run(Trigger),
    Status,
    Stop(oneshot::Sender<()>),
}

/// Options for starting a task's process.
pub(crate) fn spawn_options(task: &TaskPlan, changed: &[PathBuf]) -> SpawnOptions {
    let mut env: Vec<(OsString, OsString)> =
        task.env.iter().map(|(k, v)| (k.into(), v.into())).collect();
    env.push(("ANYMON_TASK".into(), task.name.clone().into()));
    let listed = changed.iter().take(MAX_CHANGED_PATHS);
    let joined = std::env::join_paths(listed).unwrap_or_default();
    env.push(("ANYMON_CHANGED_PATHS".into(), joined));
    SpawnOptions {
        shell: task.shell.clone(),
        cwd: Some(task.cwd.clone()),
        env,
        inherit_stdin: false,
        isolate: true,
    }
}

/// Short summary of how a run ended.
pub(crate) fn exit_summary(status: &ExitStatus, elapsed: Duration) -> String {
    let took = format_duration(elapsed);
    if status.success() {
        format!("finished in {took}")
    } else if status.code().is_some() {
        format!("failed with {} after {took}", describe_exit(status))
    } else {
        format!("was terminated by {} after {took}", describe_exit(status))
    }
}

struct Running {
    process: Process,
    started: Instant,
}

enum LastRun {
    Never,
    SpawnFailed,
    Exited(ExitStatus, Duration),
}

/// The actor that owns a task's process.
pub(crate) struct TaskActor {
    plan: Arc<Plan>,
    index: usize,
    current: Option<Running>,
    /// Process of the previous run, kept to clean up anything it left behind.
    previous: Option<Process>,
    queued: Option<Trigger>,
    last: LastRun,
}

impl TaskActor {
    pub fn new(plan: Arc<Plan>, index: usize) -> Self {
        TaskActor {
            plan,
            index,
            current: None,
            previous: None,
            queued: None,
            last: LastRun::Never,
        }
    }

    fn task(&self) -> &TaskPlan {
        &self.plan.tasks[self.index]
    }

    pub async fn run(mut self, mut commands: mpsc::UnboundedReceiver<TaskCommand>) {
        let mut backlog = VecDeque::new();
        loop {
            let command = match backlog.pop_front() {
                Some(command) => Some(command),
                None => tokio::select! {
                    command = commands.recv() => command,
                    status = wait_for(&mut self.current) => {
                        self.on_exit(status).await;
                        continue;
                    }
                },
            };
            match command {
                Some(TaskCommand::Run(mut trigger)) => {
                    // Triggers that piled up while the task was busy cause
                    // a single run.
                    while let Some(next) = next_run(&mut commands, &mut backlog) {
                        trigger = merge(Some(trigger), next);
                    }
                    if self.on_trigger(trigger).await {
                        // Changes reported while the previous process was
                        // being stopped are already seen by the new one.
                        let mut dropped = 0;
                        while next_run(&mut commands, &mut backlog).is_some() {
                            dropped += 1;
                        }
                        if dropped > 0 {
                            self.task()
                                .label
                                .detail("changes during the restart are covered by the new run");
                        }
                    }
                }
                Some(TaskCommand::Status) => self.print_status(),
                Some(TaskCommand::Stop(ack)) => {
                    self.stop_all().await;
                    let _ = ack.send(());
                    return;
                }
                None => {
                    self.stop_all().await;
                    return;
                }
            }
        }
    }

    /// Handle a trigger. Returns `true` if a process was (re)started.
    async fn on_trigger(&mut self, trigger: Trigger) -> bool {
        let busy = self.current.is_some();
        if busy && !self.task().restart && !matches!(trigger, Trigger::Manual) {
            if self.queued.is_none() {
                self.task()
                    .label
                    .detail("change detected while running; will run again when it finishes");
            }
            self.queued = Some(merge(self.queued.take(), trigger));
            return false;
        }
        self.announce(&trigger, busy);
        self.queued = None;
        self.start(trigger).await;
        busy
    }

    fn announce(&self, trigger: &Trigger, busy: bool) {
        let label = &self.task().label;
        match trigger {
            Trigger::Startup => {}
            Trigger::Manual => label.info(if busy { "restarting" } else { "starting" }),
            Trigger::Changed(paths) => {
                let first = paths
                    .first()
                    .map(|p| display_relative(p, &self.plan.root))
                    .unwrap_or_else(|| "files".to_string());
                let more = match paths.len() {
                    0 | 1 => String::new(),
                    n => format!(" (+{} more)", n - 1),
                };
                let action = if busy { ", restarting" } else { "" };
                label.info(format!("{first}{more} changed{action}"));
            }
        }
    }

    async fn start(&mut self, trigger: Trigger) {
        let grace = self.plan.kill_timeout;
        if let Some(mut running) = self.current.take() {
            let pid = running.process.id();
            self.task().label.detail(format!("stopping process {pid}"));
            if let Ok(status) = running.process.terminate(grace).await {
                self.task().label.detail(format!(
                    "process {pid} stopped ({})",
                    describe_exit(&status)
                ));
            }
        }
        if let Some(mut previous) = self.previous.take() {
            let _ = previous.terminate(grace).await;
        }

        let changed = match &trigger {
            Trigger::Changed(paths) => paths.as_slice(),
            _ => &[],
        };
        let task = self.task();
        task.label.command(&task.display);
        match spawn(&task.command, &spawn_options(task, changed)) {
            Ok(process) => {
                task.label
                    .detail(format!("started process {}", process.id()));
                self.current = Some(Running {
                    process,
                    started: Instant::now(),
                });
            }
            Err(err) => {
                task.label.failure(format!("failed to start: {err}"));
                self.last = LastRun::SpawnFailed;
            }
        }
    }

    async fn on_exit(&mut self, status: io::Result<ExitStatus>) {
        let Some(running) = self.current.take() else {
            return;
        };
        let elapsed = running.started.elapsed();
        let label = &self.plan.tasks[self.index].label;
        match status {
            Ok(status) => {
                let summary = exit_summary(&status, elapsed);
                if status.success() {
                    label.success(summary);
                } else {
                    label.failure(summary);
                }
                self.last = LastRun::Exited(status, elapsed);
            }
            Err(err) => label.failure(format!("lost track of the process: {err}")),
        }
        self.previous = Some(running.process);

        if let Some(trigger) = self.queued.take() {
            self.announce(&trigger, false);
            self.start(trigger).await;
        }
    }

    fn print_status(&self) {
        let label = &self.task().label;
        if let Some(running) = &self.current {
            label.info(format!(
                "running for {} (pid {})",
                format_duration(running.started.elapsed()),
                running.process.id()
            ));
            return;
        }
        match &self.last {
            LastRun::Never => label.info("idle, waiting for changes"),
            LastRun::SpawnFailed => label.info("idle, the last run failed to start"),
            LastRun::Exited(status, elapsed) => {
                label.info(format!("idle, last run {}", exit_summary(status, *elapsed)))
            }
        }
    }

    async fn stop_all(&mut self) {
        let grace = self.plan.kill_timeout;
        if let Some(mut running) = self.current.take() {
            let pid = running.process.id();
            self.task().label.detail(format!("stopping process {pid}"));
            let _ = running.process.terminate(grace).await;
        }
        if let Some(mut previous) = self.previous.take() {
            let _ = previous.terminate(grace).await;
        }
    }
}

/// Take the next queued `Run` trigger, if the next queued command is one.
/// Any other command is kept (in order) in `backlog`.
fn next_run(
    commands: &mut mpsc::UnboundedReceiver<TaskCommand>,
    backlog: &mut VecDeque<TaskCommand>,
) -> Option<Trigger> {
    if !backlog.is_empty() {
        return None;
    }
    match commands.try_recv().ok()? {
        TaskCommand::Run(trigger) => Some(trigger),
        other => {
            backlog.push_back(other);
            None
        }
    }
}

async fn wait_for(current: &mut Option<Running>) -> io::Result<ExitStatus> {
    match current {
        Some(running) => running.process.wait().await,
        None => std::future::pending().await,
    }
}

/// Combine a queued trigger with a new one so no changed path is lost.
fn merge(queued: Option<Trigger>, next: Trigger) -> Trigger {
    match (queued, next) {
        (Some(Trigger::Changed(mut a)), Trigger::Changed(b)) => {
            for path in b {
                if a.len() >= MAX_CHANGED_PATHS * 10 {
                    break;
                }
                if !a.contains(&path) {
                    a.push(path);
                }
            }
            Trigger::Changed(a)
        }
        (_, next) => next,
    }
}

/// Run a task once to completion, stopping it early when `cancel` fires.
/// Returns the exit code to report.
pub(crate) async fn run_to_completion(
    plan: Arc<Plan>,
    index: usize,
    mut cancel: tokio::sync::watch::Receiver<bool>,
) -> i32 {
    let task = &plan.tasks[index];
    task.label.command(&task.display);
    let mut process = match spawn(&task.command, &spawn_options(task, &[])) {
        Ok(process) => process,
        Err(err) => {
            task.label.failure(format!("failed to start: {err}"));
            return 127;
        }
    };
    let started = Instant::now();
    let code = tokio::select! {
        status = process.wait() => match status {
            Ok(status) => {
                let summary = exit_summary(&status, started.elapsed());
                if status.success() {
                    task.label.success(summary);
                } else {
                    task.label.failure(summary);
                }
                anymon_shell::exit_code(&status)
            }
            Err(err) => {
                task.label.failure(format!("lost track of the process: {err}"));
                1
            }
        },
        _ = cancel.changed() => 130,
    };
    // Also stops anything the task left running in the background.
    let _ = process.terminate(plan.kill_timeout).await;
    code
}

/// Display helper used by the session for changed paths.
pub(crate) fn describe_paths(paths: &[PathBuf], root: &Path) -> String {
    paths
        .iter()
        .map(|p| display_relative(p, root))
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merges_queued_changes() {
        let a = PathBuf::from("/p/a");
        let b = PathBuf::from("/p/b");
        let merged = merge(
            Some(Trigger::Changed(vec![a.clone()])),
            Trigger::Changed(vec![a.clone(), b.clone()]),
        );
        match merged {
            Trigger::Changed(paths) => assert_eq!(paths, vec![a, b]),
            other => panic!("unexpected {other:?}"),
        }
        assert!(matches!(merge(None, Trigger::Manual), Trigger::Manual));
        assert!(matches!(
            merge(Some(Trigger::Startup), Trigger::Changed(vec![])),
            Trigger::Changed(_)
        ));
    }

    #[test]
    fn describes_paths_relative_to_root() {
        let root = Path::new("/p");
        let text = describe_paths(&[PathBuf::from("/p/a.rs"), PathBuf::from("/q/b.rs")], root);
        assert_eq!(text, "a.rs, /q/b.rs");
    }
}
