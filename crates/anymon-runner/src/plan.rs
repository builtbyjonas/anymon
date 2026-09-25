//! Resolving a [`Config`] into a validated, ready-to-run [`Plan`].

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{bail, Context, Result};
use anymon_config::{Config, Run};
use anymon_shell::{CommandLine, Shell};

use crate::paths;
use crate::pattern::{merge_bases, PathSet, Pattern, PatternKind, WatchBase};
use crate::ui::TaskLabel;

/// Command-line overrides applied on top of a config.
#[derive(Debug, Clone, Default)]
pub struct PlanOptions {
    /// Only watch these files or directories (`--watch`).
    pub paths: Vec<PathBuf>,
    /// Override `[global] debounce`.
    pub debounce_ms: Option<u64>,
    /// Override `[global] kill_timeout`.
    pub kill_timeout_ms: Option<u64>,
    /// Override `[global] gitignore`.
    pub gitignore: Option<bool>,
    /// Override `[global] poll` (milliseconds).
    pub poll_ms: Option<u64>,
    /// Additional ignore patterns.
    pub ignore: Vec<String>,
    /// Only run these tasks (by name).
    pub tasks: Vec<String>,
}

/// Everything needed to watch and run a set of tasks.
#[derive(Debug)]
pub struct Plan {
    /// Project root: patterns and task directories are relative to it.
    pub root: PathBuf,
    /// The config file, if any. It is watched for changes.
    pub config_path: Option<PathBuf>,
    /// Quiet period before a change triggers a run.
    pub debounce: Duration,
    /// Time a process gets to exit gracefully before it is killed.
    pub kill_timeout: Duration,
    /// Whether `.gitignore` files are honoured.
    pub gitignore: bool,
    /// Poll interval, if polling is used instead of native events.
    pub poll: Option<Duration>,
    /// Ignore patterns that apply to all tasks.
    pub ignore_patterns: Vec<Pattern>,
    /// Compiled [`Plan::ignore_patterns`].
    pub ignore: PathSet,
    /// If not empty, only changes below these paths are considered.
    pub paths: Vec<PathBuf>,
    /// The tasks to run.
    pub tasks: Vec<TaskPlan>,
}

/// A single task, ready to run.
#[derive(Debug)]
pub struct TaskPlan {
    /// Unique task name.
    pub name: String,
    /// Colored label for output.
    pub label: TaskLabel,
    /// The parsed command.
    pub command: CommandLine,
    /// The command as shown to the user.
    pub display: String,
    /// Shell for shell scripts and fallbacks.
    pub shell: Shell,
    /// Working directory.
    pub cwd: PathBuf,
    /// Extra environment variables.
    pub env: Vec<(String, String)>,
    /// Restart a running process on change (instead of queueing a run).
    pub restart: bool,
    /// Run when anymon starts.
    pub run_on_start: bool,
    /// Patterns selecting the files that trigger this task.
    pub watch_patterns: Vec<Pattern>,
    /// Patterns excluded for this task.
    pub ignore_patterns: Vec<Pattern>,
    /// Compiled [`TaskPlan::watch_patterns`].
    pub watch: PathSet,
    /// Compiled [`TaskPlan::ignore_patterns`].
    pub ignore: PathSet,
}

impl Plan {
    /// Build a plan from a config. `root` is the directory patterns are
    /// relative to (normally the directory of the config file).
    pub fn new(
        config: &Config,
        root: &Path,
        config_path: Option<&Path>,
        options: &PlanOptions,
    ) -> Result<Plan> {
        config.validate()?;
        let root = paths::canonicalize(root)
            .with_context(|| format!("project directory {} is not accessible", root.display()))?;
        let config_path = config_path.map(|p| paths::resolve(p, &root));

        let mut ignore_sources = config.global.ignore.clone();
        ignore_sources.extend(options.ignore.iter().cloned());
        let ignore_patterns = compile_all(&ignore_sources, &root, PatternKind::Ignore)?;
        let ignore = PathSet::for_ignore(&ignore_patterns)?;

        let mut restrict = Vec::new();
        for path in &options.paths {
            let resolved = paths::resolve(path, &std::env::current_dir()?);
            if !resolved.exists() {
                bail!("watch path {} does not exist", path.display());
            }
            restrict.push(resolved);
        }

        let names = config.task_names();
        for wanted in &options.tasks {
            if !names.iter().any(|n| n == wanted) {
                bail!("unknown task '{wanted}' (available: {})", names.join(", "));
            }
        }

        let global_shell = config.global.shell.as_deref().map(Shell::from_name);
        let mut tasks = Vec::new();
        for (index, (task, name)) in config.tasks.iter().zip(&names).enumerate() {
            if !options.tasks.is_empty() && !options.tasks.contains(name) {
                continue;
            }
            let context = || format!("task '{name}'");
            let command = match &task.run {
                Run::Command(line) => CommandLine::parse(line),
                Run::Argv(argv) => CommandLine::from_argv(argv.iter().cloned()),
            }
            .with_context(|| format!("{}: invalid command", context()))?;
            let display = match &task.run {
                Run::Command(line) => line.trim().to_string(),
                Run::Argv(_) => command.to_string(),
            };

            let watch_sources = if task.watch.is_empty() {
                vec!["**".to_string()]
            } else {
                task.watch.clone()
            };
            let watch_patterns =
                compile_all(&watch_sources, &root, PatternKind::Watch).with_context(context)?;
            let ignore_patterns =
                compile_all(&task.ignore, &root, PatternKind::Ignore).with_context(context)?;

            let cwd = match &task.cwd {
                Some(dir) => {
                    let cwd = paths::resolve(dir, &root);
                    if !cwd.is_dir() {
                        bail!(
                            "{}: working directory {} does not exist",
                            context(),
                            cwd.display()
                        );
                    }
                    cwd
                }
                None => root.clone(),
            };

            tasks.push(TaskPlan {
                name: name.clone(),
                label: TaskLabel::new(name.clone(), index),
                command,
                display,
                shell: task
                    .shell
                    .as_deref()
                    .map(Shell::from_name)
                    .or_else(|| global_shell.clone())
                    .unwrap_or_default(),
                cwd,
                env: task
                    .env
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect(),
                restart: task.restart.unwrap_or(true),
                run_on_start: task.run_on_start.unwrap_or(true),
                watch: PathSet::new(&watch_patterns).with_context(context)?,
                ignore: PathSet::for_ignore(&ignore_patterns).with_context(context)?,
                watch_patterns,
                ignore_patterns,
            });
        }

        Ok(Plan {
            debounce: Duration::from_millis(options.debounce_ms.unwrap_or(config.debounce_ms())),
            kill_timeout: Duration::from_millis(
                options.kill_timeout_ms.unwrap_or(config.kill_timeout_ms()),
            ),
            gitignore: options
                .gitignore
                .or(config.global.gitignore)
                .unwrap_or(true),
            poll: options
                .poll_ms
                .or(config.global.poll)
                .map(|ms| Duration::from_millis(ms.max(1))),
            root,
            config_path,
            ignore_patterns,
            ignore,
            paths: restrict,
            tasks,
        })
    }

    /// The directories that need to be watched, and the requested paths that
    /// do not exist.
    pub fn watch_bases(&self) -> (Vec<WatchBase>, Vec<PathBuf>) {
        let mut bases = Vec::new();
        if self.paths.is_empty() {
            for task in &self.tasks {
                bases.extend(task.watch_patterns.iter().map(Pattern::watch_base));
            }
        } else {
            for path in &self.paths {
                bases.push(if path.is_dir() {
                    WatchBase {
                        path: path.clone(),
                        recursive: true,
                    }
                } else {
                    WatchBase {
                        path: path.parent().unwrap_or(path).to_path_buf(),
                        recursive: false,
                    }
                });
            }
        }
        if let Some(dir) = self.config_path.as_deref().and_then(Path::parent) {
            bases.push(WatchBase {
                path: dir.to_path_buf(),
                recursive: false,
            });
        }
        merge_bases(bases, &self.root)
    }

    /// Returns `true` if `path` is inside one of the `--watch` paths (or no
    /// such restriction was given).
    pub fn is_within_paths(&self, path: &Path) -> bool {
        self.paths.is_empty() || self.paths.iter().any(|p| path.starts_with(p))
    }
}

fn compile_all(sources: &[String], root: &Path, kind: PatternKind) -> Result<Vec<Pattern>> {
    sources
        .iter()
        .map(|source| Pattern::compile(source, root, kind).map_err(Into::into))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(files: &[&str]) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let root = paths::canonicalize(dir.path()).unwrap();
        for file in files {
            let path = root.join(file);
            if file.ends_with('/') {
                std::fs::create_dir_all(&path).unwrap();
            } else {
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(&path, "").unwrap();
            }
        }
        (dir, root)
    }

    fn config(toml: &str) -> Config {
        toml.parse().unwrap()
    }

    #[test]
    fn applies_defaults() {
        let (_dir, root) = project(&["src/main.rs"]);
        let cfg = config("[[task]]\nrun = \"cargo run\"");
        let plan = Plan::new(&cfg, &root, None, &PlanOptions::default()).unwrap();
        assert_eq!(plan.root, root);
        assert_eq!(
            plan.debounce,
            Duration::from_millis(anymon_config::DEFAULT_DEBOUNCE_MS)
        );
        assert_eq!(
            plan.kill_timeout,
            Duration::from_millis(anymon_config::DEFAULT_KILL_TIMEOUT_MS)
        );
        assert!(plan.gitignore);
        assert_eq!(plan.poll, None);
        let task = &plan.tasks[0];
        assert_eq!(task.name, "cargo");
        assert_eq!(task.display, "cargo run");
        assert_eq!(task.cwd, root);
        assert!(task.restart);
        assert!(task.run_on_start);
        assert_eq!(task.shell, Shell::platform_default());
        assert!(task.watch.is_match(&root.join("anything/at/all.txt")));
    }

    #[test]
    fn cli_options_override_config() {
        let (_dir, root) = project(&["src/"]);
        let cfg = config(
            "[global]\ndebounce = 10\nkill_timeout = 20\ngitignore = true\npoll = 5\n[[task]]\nrun = \"x\"",
        );
        let options = PlanOptions {
            debounce_ms: Some(99),
            kill_timeout_ms: Some(98),
            gitignore: Some(false),
            poll_ms: Some(700),
            ignore: vec!["*.tmp".into()],
            ..PlanOptions::default()
        };
        let plan = Plan::new(&cfg, &root, None, &options).unwrap();
        assert_eq!(plan.poll, Some(Duration::from_millis(700)));
        assert_eq!(plan.debounce, Duration::from_millis(99));
        assert_eq!(plan.kill_timeout, Duration::from_millis(98));
        assert!(!plan.gitignore);
        assert!(plan.ignore.is_match(&root.join("src/a.tmp")));
    }

    #[test]
    fn resolves_task_settings() {
        let (_dir, root) = project(&["backend/"]);
        let cfg = config(
            r#"
            [global]
            shell = "bash"
            [[task]]
            name = "api"
            run = ["node", "server.js", "--port", "80 80"]
            cwd = "backend"
            env = { PORT = "8080" }
            restart = false
            run_on_start = false
            [[task]]
            name = "other"
            run = "make && make test"
            shell = "zsh"
            "#,
        );
        let plan = Plan::new(&cfg, &root, None, &PlanOptions::default()).unwrap();
        let api = &plan.tasks[0];
        assert_eq!(api.cwd, root.join("backend"));
        assert_eq!(api.env, vec![("PORT".to_string(), "8080".to_string())]);
        assert!(!api.restart);
        assert!(!api.run_on_start);
        assert_eq!(api.shell, Shell::Other("bash".into()));
        assert!(!api.command.is_shell());
        let expected = if cfg!(windows) {
            "node server.js --port \"80 80\""
        } else {
            "node server.js --port '80 80'"
        };
        assert_eq!(api.display, expected);

        let other = &plan.tasks[1];
        assert!(other.command.is_shell());
        assert_eq!(other.shell, Shell::Other("zsh".into()));
    }

    #[test]
    fn filters_tasks_by_name() {
        let (_dir, root) = project(&[]);
        let cfg =
            config("[[task]]\nname = \"a\"\nrun = \"x\"\n[[task]]\nname = \"b\"\nrun = \"y\"");
        let options = PlanOptions {
            tasks: vec!["b".into()],
            ..PlanOptions::default()
        };
        let plan = Plan::new(&cfg, &root, None, &options).unwrap();
        assert_eq!(plan.tasks.len(), 1);
        assert_eq!(plan.tasks[0].name, "b");

        let options = PlanOptions {
            tasks: vec!["c".into()],
            ..PlanOptions::default()
        };
        let err = Plan::new(&cfg, &root, None, &options).unwrap_err();
        assert_eq!(err.to_string(), "unknown task 'c' (available: a, b)");
    }

    #[test]
    fn reports_invalid_input() {
        let (_dir, root) = project(&[]);
        let cases = [
            (
                "[[task]]\nname = \"t\"\nrun = \"echo 'oops\"",
                "task 't': invalid command",
            ),
            (
                "[[task]]\nname = \"t\"\nrun = \"x\"\nwatch = [\"src/[\"]",
                "task 't'",
            ),
            (
                "[[task]]\nname = \"t\"\nrun = \"x\"\ncwd = \"missing\"",
                "working directory",
            ),
            (
                "[global]\nignore = [\"a/{b\"]\n[[task]]\nrun = \"x\"",
                "invalid pattern `a/{b`",
            ),
        ];
        for (toml, expected) in cases {
            let err = Plan::new(&config(toml), &root, None, &PlanOptions::default()).unwrap_err();
            assert!(format!("{err:#}").contains(expected), "{toml}: {err:#}");
        }

        let options = PlanOptions {
            paths: vec![root.join("nope")],
            ..PlanOptions::default()
        };
        let err = Plan::new(&config("[[task]]\nrun = \"x\""), &root, None, &options).unwrap_err();
        assert!(err.to_string().contains("does not exist"));
    }

    #[test]
    fn computes_watch_bases() {
        let (_dir, root) = project(&["src/", "docs/", "Anymon.toml"]);
        let cfg = config(
            "[[task]]\nname = \"a\"\nrun = \"x\"\nwatch = [\"src/**\", \"/Cargo.toml\"]\n[[task]]\nname = \"b\"\nrun = \"y\"\nwatch = \"docs/*.md\"",
        );
        let plan = Plan::new(
            &cfg,
            &root,
            Some(&root.join("Anymon.toml")),
            &PlanOptions::default(),
        )
        .unwrap();
        let (bases, missing) = plan.watch_bases();
        assert!(missing.is_empty());
        assert_eq!(
            bases,
            vec![
                WatchBase {
                    path: root.clone(),
                    recursive: false
                },
                WatchBase {
                    path: root.join("docs"),
                    recursive: false
                },
                WatchBase {
                    path: root.join("src"),
                    recursive: true
                },
            ]
        );

        let options = PlanOptions {
            paths: vec![root.join("docs")],
            ..PlanOptions::default()
        };
        let plan = Plan::new(&cfg, &root, None, &options).unwrap();
        assert_eq!(
            plan.watch_bases().0,
            vec![WatchBase {
                path: root.join("docs"),
                recursive: true
            }]
        );
        assert!(plan.is_within_paths(&root.join("docs/a.md")));
        assert!(!plan.is_within_paths(&root.join("src/a.rs")));
    }
}
