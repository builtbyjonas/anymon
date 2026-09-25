//! The watch loop: receives file events, decides which tasks they concern,
//! debounces them and drives the task actors.

use std::io::{BufRead, IsTerminal};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use notify::event::{EventKind, ModifyKind};
use notify::RecursiveMode;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio::time::Instant;

use crate::ignore::{is_builtin_ignored, GitignoreTree};
use crate::paths::display_relative;
use crate::plan::Plan;
use crate::task::{describe_paths, run_to_completion, TaskActor, TaskCommand, Trigger};
use crate::ui;
use crate::watcher::{FsEvent, FsWatcher};

/// Called when the config file changes; returns the new plan.
pub type Reloader<'a> = &'a dyn Fn() -> Result<Plan>;

/// Maximum number of changed paths remembered per task between runs.
const MAX_PENDING_PATHS: usize = 1000;

/// Some platforms (notably macOS FSEvents) report changes made shortly
/// before a watch was registered. During this window after startup, events
/// for files last modified before the watch started are dropped.
const STARTUP_GRACE: Duration = Duration::from_secs(1);

/// Watch files and run the plan's tasks until the user quits or a signal
/// arrives. When `reload` is given, changes to the config file rebuild the
/// plan and restart all tasks.
pub async fn watch(plan: Plan, reload: Option<Reloader<'_>>) -> Result<()> {
    let mut stdin = StdinReader::spawn();
    let mut signals = Signals::new()?;
    let mut plan = plan;
    loop {
        let session = Session::start(plan, stdin.is_interactive())?;
        match session.run(&mut stdin, &mut signals, reload).await {
            Outcome::Quit => return Ok(()),
            Outcome::Reload(next) => plan = *next,
        }
    }
}

/// Run every task of the plan once, in parallel, and return the exit code
/// of the first task that failed (in config order), or 0.
pub async fn run_once(plan: Plan) -> Result<i32> {
    let plan = Arc::new(plan);
    let mut signals = Signals::new()?;
    let (cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);
    let mut runs = tokio::task::JoinSet::new();
    for index in 0..plan.tasks.len() {
        let plan = plan.clone();
        let cancel = cancel_rx.clone();
        runs.spawn(async move { (index, run_to_completion(plan, index, cancel).await) });
    }

    let mut codes = vec![0; plan.tasks.len()];
    let mut cancelled = false;
    loop {
        tokio::select! {
            joined = runs.join_next() => match joined {
                Some(Ok((index, code))) => codes[index] = code,
                Some(Err(err)) => ui::error(format!("task crashed: {err}")),
                None => break,
            },
            signal = signals.recv(), if !cancelled => {
                ui::info(format!("{signal} received, stopping tasks"));
                cancelled = true;
                let _ = cancel_tx.send(true);
            }
        }
    }
    if cancelled {
        return Ok(130);
    }
    Ok(codes.into_iter().find(|code| *code != 0).unwrap_or(0))
}

enum Outcome {
    Quit,
    Reload(Box<Plan>),
}

#[derive(Default)]
struct Pending {
    deadline: Option<Instant>,
    paths: Vec<PathBuf>,
}

struct TaskHandle {
    commands: mpsc::UnboundedSender<TaskCommand>,
    join: JoinHandle<()>,
}

struct Session {
    plan: Arc<Plan>,
    watcher: FsWatcher,
    events: mpsc::UnboundedReceiver<FsEvent>,
    gitignore: GitignoreTree,
    /// Per task: explicitly watched directories that ignore rules would hide.
    exempt: Vec<Vec<PathBuf>>,
    /// `--watch` paths that ignore rules would hide.
    global_exempt: Vec<PathBuf>,
    tasks: Vec<TaskHandle>,
    pending: Vec<Pending>,
    reload_at: Option<Instant>,
    /// Config file content the current plan was built from.
    config_snapshot: Option<Vec<u8>>,
    /// When the watches were registered.
    watching_since: std::time::SystemTime,
    started: Instant,
}

impl Session {
    fn start(plan: Plan, interactive: bool) -> Result<Session> {
        let plan = Arc::new(plan);
        let (tx, events) = mpsc::unbounded_channel();
        let watcher = FsWatcher::new(tx, plan.poll)
            .context("cannot start the file watcher (`--poll` works without OS file events)")?;
        let mut session = Session {
            watcher,
            events,
            gitignore: GitignoreTree::new(),
            exempt: vec![Vec::new(); plan.tasks.len()],
            global_exempt: Vec::new(),
            tasks: Vec::new(),
            pending: (0..plan.tasks.len()).map(|_| Pending::default()).collect(),
            reload_at: None,
            config_snapshot: plan
                .config_path
                .as_ref()
                .and_then(|p| std::fs::read(p).ok()),
            watching_since: std::time::SystemTime::now(),
            started: Instant::now(),
            plan,
        };
        session.setup_ignores();
        session.setup_watches();
        session.print_banner(interactive);
        session.start_tasks();
        Ok(session)
    }

    fn setup_ignores(&mut self) {
        let plan = self.plan.clone();
        if plan.gitignore {
            self.gitignore.load_ancestors(&plan.root);
            self.gitignore.load_dir(&plan.root);
        }
        for (index, task) in plan.tasks.iter().enumerate() {
            for pattern in &task.watch_patterns {
                if pattern.base != plan.root && self.is_soft_ignored_dir(&pattern.base) {
                    self.exempt[index].push(pattern.base.clone());
                }
            }
        }
        for path in &plan.paths {
            if self.is_soft_ignored_dir(path) {
                self.global_exempt.push(path.clone());
            }
        }
    }

    /// Whether ignore rules that explicit watch patterns may override (built-in
    /// rules and `.gitignore`) hide the directory `dir`.
    fn is_soft_ignored_dir(&mut self, dir: &Path) -> bool {
        if self.plan.gitignore {
            self.gitignore.load_between(&self.plan.root, dir);
        }
        is_builtin_ignored(dir, &self.plan.root)
            || (self.plan.gitignore && self.gitignore.is_ignored(dir, true))
    }

    fn setup_watches(&mut self) {
        let (bases, missing) = self.plan.watch_bases();
        for path in missing {
            ui::warn(format!(
                "not watching {}: it does not exist",
                path.display()
            ));
        }
        let manual = self.watcher.manual_recursion();
        let mut targets = Vec::new();
        for base in &bases {
            if base.recursive {
                if manual || self.plan.gitignore {
                    let (dirs, _) = self.walk(&base.path, false);
                    if manual {
                        targets.extend(dirs.into_iter().map(|d| (d, RecursiveMode::NonRecursive)));
                        continue;
                    }
                }
                targets.push((base.path.clone(), RecursiveMode::Recursive));
            } else {
                if self.plan.gitignore {
                    self.gitignore.load_dir(&base.path);
                }
                targets.push((base.path.clone(), RecursiveMode::NonRecursive));
            }
        }
        let count = self.watcher.watch(targets);
        for base in &bases {
            ui::detail(format!(
                "watching {}{}",
                base.path.display(),
                if base.recursive { " (recursive)" } else { "" }
            ));
        }
        ui::detail(format!(
            "{count} watch{} registered, {} .gitignore file{} loaded",
            if count == 1 { "" } else { "es" },
            self.gitignore.len(),
            if self.gitignore.len() == 1 { "" } else { "s" },
        ));
    }

    /// Walk the directories below `start` that are not ignored, loading their
    /// `.gitignore` files on the way. Optionally also collects files.
    fn walk(&mut self, start: &Path, collect_files: bool) -> (Vec<PathBuf>, Vec<PathBuf>) {
        let mut dirs = Vec::new();
        let mut files = Vec::new();
        let mut stack = vec![start.to_path_buf()];
        while let Some(dir) = stack.pop() {
            if self.plan.gitignore {
                self.gitignore.load_dir(&dir);
            }
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let Ok(file_type) = entry.file_type() else {
                    continue;
                };
                let path = entry.path();
                if file_type.is_dir() {
                    if !self.should_skip_dir(&path) {
                        stack.push(path);
                    }
                } else if collect_files {
                    files.push(path);
                }
            }
            dirs.push(dir);
        }
        (dirs, files)
    }

    fn is_exempt(&self, task: usize, path: &Path) -> bool {
        self.exempt[task]
            .iter()
            .chain(&self.global_exempt)
            .any(|base| path.starts_with(base))
    }

    fn should_skip_dir(&self, dir: &Path) -> bool {
        if self.plan.ignore.is_match(dir) {
            return true;
        }
        let soft = is_builtin_ignored(dir, &self.plan.root)
            || (self.plan.gitignore && self.gitignore.is_ignored(dir, true));
        if !soft {
            return false;
        }
        // Keep directories that lead to, or lie inside, an explicitly watched
        // directory.
        !self
            .exempt
            .iter()
            .flatten()
            .chain(&self.global_exempt)
            .any(|base| dir.starts_with(base) || base.starts_with(dir))
    }

    fn print_banner(&self, interactive: bool) {
        let plan = &self.plan;
        let count = plan.tasks.len();
        let mut line = format!(
            "v{} watching {} ({count} task{})",
            env!("CARGO_PKG_VERSION"),
            display_home(&plan.root),
            if count == 1 { "" } else { "s" }
        );
        if let Some(config) = &plan.config_path {
            line.push_str(&format!(
                ", config {}",
                display_relative(config, &plan.root)
            ));
        }
        ui::info(line);
        if interactive {
            ui::info("type `rs` + Enter to restart, `s` for status, `q` to quit");
        }
    }

    fn start_tasks(&mut self) {
        for index in 0..self.plan.tasks.len() {
            let (tx, rx) = mpsc::unbounded_channel();
            let actor = TaskActor::new(self.plan.clone(), index);
            let join = tokio::spawn(actor.run(rx));
            let task = &self.plan.tasks[index];
            if task.run_on_start {
                let _ = tx.send(TaskCommand::Run(Trigger::Startup));
            } else {
                task.label.info("waiting for changes");
            }
            self.tasks.push(TaskHandle { commands: tx, join });
        }
    }

    async fn run(
        mut self,
        stdin: &mut StdinReader,
        signals: &mut Signals,
        reload: Option<Reloader<'_>>,
    ) -> Outcome {
        loop {
            let deadline = self.next_deadline();
            tokio::select! {
                Some(event) = self.events.recv() => {
                    self.on_event(event);
                    // Drain bursts (e.g. a `git checkout`) in one go.
                    for _ in 0..4096 {
                        match self.events.try_recv() {
                            Ok(event) => self.on_event(event),
                            Err(_) => break,
                        }
                    }
                }
                _ = sleep_until(deadline) => {
                    if let Some(next) = self.flush(reload) {
                        self.shutdown(signals).await;
                        return Outcome::Reload(Box::new(next));
                    }
                }
                line = stdin.recv(), if stdin.is_open() => match line {
                    Some(line) => {
                        if self.on_command(&line) {
                            self.shutdown(signals).await;
                            return Outcome::Quit;
                        }
                    }
                    None => stdin.close(),
                },
                signal = signals.recv() => {
                    ui::info(format!("{signal} received, stopping tasks"));
                    self.shutdown(signals).await;
                    return Outcome::Quit;
                }
            }
        }
    }

    fn next_deadline(&self) -> Option<Instant> {
        self.pending
            .iter()
            .filter_map(|p| p.deadline)
            .chain(self.reload_at)
            .min()
    }

    fn on_event(&mut self, event: FsEvent) {
        let event = match event {
            Ok(event) => event,
            Err(err) => {
                ui::warn(format!("file watcher error: {err}"));
                return;
            }
        };
        if matches!(event.kind, EventKind::Access(_)) {
            return;
        }
        if event.need_rescan() {
            ui::detail("the OS dropped file events; treating everything as changed");
            let root = self.plan.root.clone();
            for index in 0..self.pending.len() {
                self.mark_pending(index, root.clone());
            }
            return;
        }
        let may_create_dir = matches!(
            event.kind,
            EventKind::Create(_) | EventKind::Modify(ModifyKind::Name(_)) | EventKind::Any
        );
        for path in event.paths {
            self.on_path(path, may_create_dir);
        }
    }

    fn on_path(&mut self, path: PathBuf, may_create_dir: bool) {
        let plan = self.plan.clone();
        if plan
            .config_path
            .as_deref()
            .is_some_and(|c| same_file(c, &path))
        {
            ui::detail(format!("config file changed: {}", path.display()));
            self.reload_at = Some(Instant::now() + plan.debounce);
            return;
        }
        if plan.gitignore && path.file_name().is_some_and(|n| n == ".gitignore") {
            if let Some(dir) = path.parent() {
                self.gitignore.load_dir(dir);
            }
        }
        if !plan.is_within_paths(&path) || plan.ignore.is_match(&path) {
            return;
        }

        let metadata = std::fs::metadata(&path).ok();
        let is_dir = metadata.as_ref().is_some_and(|m| m.is_dir());
        if !is_dir && self.started.elapsed() < STARTUP_GRACE {
            let modified = metadata.as_ref().and_then(|m| m.modified().ok());
            if modified.is_some_and(|m| m < self.watching_since) {
                ui::detail(format!("ignoring stale event for {}", path.display()));
                return;
            }
        }
        if is_dir
            && may_create_dir
            && self.watcher.manual_recursion()
            && !self.should_skip_dir(&path)
        {
            // Watch the new directory and pick up files created in it before
            // the watch was in place.
            let (dirs, files) = self.walk(&path, true);
            self.watcher.watch(
                dirs.into_iter()
                    .map(|d| (d, RecursiveMode::NonRecursive))
                    .collect(),
            );
            for file in files {
                self.match_path(file, false);
            }
        }
        self.match_path(path, is_dir);
    }

    fn match_path(&mut self, path: PathBuf, is_dir: bool) {
        let plan = self.plan.clone();
        if !plan.is_within_paths(&path) || plan.ignore.is_match(&path) {
            return;
        }
        let mut soft_ignored: Option<bool> = None;
        for (index, task) in plan.tasks.iter().enumerate() {
            if !task.watch.is_match(&path) || task.ignore.is_match(&path) {
                continue;
            }
            let soft = *soft_ignored.get_or_insert_with(|| {
                is_builtin_ignored(&path, &plan.root)
                    || (plan.gitignore && self.gitignore.is_ignored(&path, is_dir))
            });
            if soft && !self.is_exempt(index, &path) {
                continue;
            }
            self.mark_pending(index, path.clone());
        }
    }

    fn mark_pending(&mut self, index: usize, path: PathBuf) {
        let pending = &mut self.pending[index];
        if pending.paths.len() < MAX_PENDING_PATHS && !pending.paths.contains(&path) {
            pending.paths.push(path);
        }
        pending.deadline = Some(Instant::now() + self.plan.debounce);
    }

    /// Trigger tasks whose debounce window has passed. Returns a new plan if
    /// the config file changed and is valid.
    fn flush(&mut self, reload: Option<Reloader<'_>>) -> Option<Plan> {
        let now = Instant::now();
        if self.reload_at.is_some_and(|at| at <= now) {
            self.reload_at = None;
            let content = self
                .plan
                .config_path
                .as_ref()
                .and_then(|p| std::fs::read(p).ok());
            if content.is_some() && content == self.config_snapshot {
                ui::detail("config file saved without changes");
            } else if let Some(reload) = reload {
                match reload() {
                    Ok(plan) => {
                        ui::info("config changed, restarting");
                        return Some(plan);
                    }
                    Err(err) => ui::warn(format!("config not reloaded: {err:#}")),
                }
            }
        }
        for (index, pending) in self.pending.iter_mut().enumerate() {
            if pending.deadline.is_some_and(|at| at <= now) {
                pending.deadline = None;
                let paths = std::mem::take(&mut pending.paths);
                ui::detail(format!(
                    "{}: {}",
                    self.plan.tasks[index].name,
                    describe_paths(&paths, &self.plan.root)
                ));
                let _ = self.tasks[index]
                    .commands
                    .send(TaskCommand::Run(Trigger::Changed(paths)));
            }
        }
        None
    }

    /// Handle a line typed by the user. Returns `true` to quit.
    fn on_command(&mut self, line: &str) -> bool {
        let mut words = line.split_whitespace();
        let Some(command) = words.next() else {
            return false;
        };
        let names: Vec<&str> = words.collect();
        match command.to_ascii_lowercase().as_str() {
            "rs" | "r" | "restart" => {
                for index in self.select_tasks(&names) {
                    let _ = self.tasks[index]
                        .commands
                        .send(TaskCommand::Run(Trigger::Manual));
                }
            }
            "s" | "status" => {
                for index in self.select_tasks(&names) {
                    let _ = self.tasks[index].commands.send(TaskCommand::Status);
                }
            }
            "q" | "quit" | "exit" => return true,
            "h" | "help" | "?" => {
                ui::info("commands (followed by Enter):");
                ui::info("  rs [task...]      restart all or the named tasks");
                ui::info("  s, status [task]  show what the tasks are doing");
                ui::info("  q, quit           stop all tasks and exit");
            }
            other => ui::warn(format!("unknown command `{other}` (type `help`)")),
        }
        false
    }

    fn select_tasks(&self, names: &[&str]) -> Vec<usize> {
        if names.is_empty() {
            return (0..self.plan.tasks.len()).collect();
        }
        let mut selected = Vec::new();
        for name in names {
            match self.plan.tasks.iter().position(|t| t.name == *name) {
                Some(index) => selected.push(index),
                None => ui::warn(format!("no task named '{name}'")),
            }
        }
        selected
    }

    /// Stop every task, escalating if a second signal arrives.
    async fn shutdown(&mut self, signals: &mut Signals) {
        let mut acks = Vec::new();
        for task in &self.tasks {
            let (tx, rx) = oneshot::channel();
            if task.commands.send(TaskCommand::Stop(tx)).is_ok() {
                acks.push(rx);
            }
        }
        let all_stopped = async {
            for ack in acks {
                let _ = ack.await;
            }
        };
        let limit = self.plan.kill_timeout + Duration::from_secs(3);
        tokio::select! {
            _ = all_stopped => {}
            _ = tokio::time::sleep(limit) => ui::warn("tasks did not stop in time, killing them"),
            signal = signals.recv() => ui::warn(format!("{signal} received again, killing tasks")),
        }
        // Aborting drops the processes, which kills whatever is left.
        for task in self.tasks.drain(..) {
            task.join.abort();
            let _ = task.join.await;
        }
    }
}

async fn sleep_until(deadline: Option<Instant>) {
    match deadline {
        Some(deadline) => tokio::time::sleep_until(deadline).await,
        None => std::future::pending().await,
    }
}

/// Compare paths, ignoring case where the file system usually does.
fn same_file(a: &Path, b: &Path) -> bool {
    if a == b {
        return true;
    }
    if cfg!(any(target_os = "macos", target_os = "windows")) {
        let (a, b) = (a.to_string_lossy(), b.to_string_lossy());
        return a.eq_ignore_ascii_case(&b);
    }
    false
}

fn display_home(path: &Path) -> String {
    if !cfg!(windows) {
        if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
            if let Ok(rel) = path.strip_prefix(&home) {
                if !home.as_os_str().is_empty() && home != Path::new("/") {
                    return Path::new("~").join(rel).display().to_string();
                }
            }
        }
    }
    path.display().to_string()
}

/// Reads commands from stdin on a dedicated thread. A plain thread is used
/// instead of `tokio::io::stdin` because a blocking read cannot be cancelled
/// and would otherwise keep anymon from exiting until Enter is pressed.
struct StdinReader {
    lines: mpsc::UnboundedReceiver<String>,
    open: bool,
    interactive: bool,
}

impl StdinReader {
    fn spawn() -> Self {
        let (tx, lines) = mpsc::unbounded_channel();
        let readable = can_read_stdin();
        if readable {
            let spawned = std::thread::Builder::new()
                .name("anymon-stdin".into())
                .spawn(move || {
                    let stdin = std::io::stdin();
                    let mut line = String::new();
                    loop {
                        line.clear();
                        match stdin.lock().read_line(&mut line) {
                            Ok(0) | Err(_) => break,
                            Ok(_) => {
                                if tx.send(line.trim().to_string()).is_err() {
                                    break;
                                }
                            }
                        }
                    }
                });
            if spawned.is_err() {
                ui::detail("cannot read commands from stdin");
            }
        }
        StdinReader {
            lines,
            open: readable,
            interactive: readable && std::io::stdin().is_terminal(),
        }
    }

    fn is_open(&self) -> bool {
        self.open
    }

    fn is_interactive(&self) -> bool {
        self.interactive
    }

    fn close(&mut self) {
        self.open = false;
    }

    async fn recv(&mut self) -> Option<String> {
        self.lines.recv().await
    }
}

/// Reading from the terminal while in the background would stop anymon
/// (`SIGTTIN`), so stdin is only read in the foreground.
fn can_read_stdin() -> bool {
    #[cfg(unix)]
    unsafe {
        if libc::isatty(libc::STDIN_FILENO) == 1 {
            return libc::tcgetpgrp(libc::STDIN_FILENO) == libc::getpgrp();
        }
    }
    true
}

/// Termination signals: Ctrl-C, and on Unix also `SIGTERM` and `SIGHUP`.
pub(crate) struct Signals {
    #[cfg(unix)]
    interrupt: tokio::signal::unix::Signal,
    #[cfg(unix)]
    terminate: tokio::signal::unix::Signal,
    #[cfg(unix)]
    hangup: tokio::signal::unix::Signal,
    #[cfg(windows)]
    ctrl_c: tokio::signal::windows::CtrlC,
    #[cfg(windows)]
    ctrl_close: tokio::signal::windows::CtrlClose,
}

impl Signals {
    pub(crate) fn new() -> std::io::Result<Self> {
        #[cfg(unix)]
        {
            use tokio::signal::unix::{signal, SignalKind};
            Ok(Signals {
                interrupt: signal(SignalKind::interrupt())?,
                terminate: signal(SignalKind::terminate())?,
                hangup: signal(SignalKind::hangup())?,
            })
        }
        #[cfg(windows)]
        {
            Ok(Signals {
                ctrl_c: tokio::signal::windows::ctrl_c()?,
                ctrl_close: tokio::signal::windows::ctrl_close()?,
            })
        }
        #[cfg(not(any(unix, windows)))]
        {
            Ok(Signals {})
        }
    }

    pub(crate) async fn recv(&mut self) -> &'static str {
        #[cfg(unix)]
        {
            tokio::select! {
                _ = self.interrupt.recv() => "Ctrl-C",
                _ = self.terminate.recv() => "SIGTERM",
                _ = self.hangup.recv() => "SIGHUP",
            }
        }
        #[cfg(windows)]
        {
            tokio::select! {
                _ = self.ctrl_c.recv() => "Ctrl-C",
                _ = self.ctrl_close.recv() => "console close",
            }
        }
        #[cfg(not(any(unix, windows)))]
        {
            std::future::pending().await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_config_paths() {
        assert!(same_file(
            Path::new("/a/Anymon.toml"),
            Path::new("/a/Anymon.toml")
        ));
        assert!(!same_file(
            Path::new("/a/Anymon.toml"),
            Path::new("/b/Anymon.toml")
        ));
        assert_eq!(
            same_file(Path::new("/a/Anymon.toml"), Path::new("/a/anymon.toml")),
            cfg!(any(target_os = "macos", target_os = "windows"))
        );
    }

    #[test]
    fn abbreviates_home() {
        if cfg!(windows) {
            return;
        }
        let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
            return;
        };
        if home == Path::new("/") {
            return;
        }
        assert_eq!(
            display_home(&home.join("proj")),
            format!("~{}proj", std::path::MAIN_SEPARATOR)
        );
        assert_eq!(
            display_home(Path::new("/definitely/elsewhere")),
            "/definitely/elsewhere"
        );
    }
}
