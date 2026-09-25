//! Configuration for anymon.
//!
//! A config file (usually `Anymon.toml`) has an optional `[global]` table and
//! one or more `[[task]]` tables:
//!
//! ```toml
//! [global]
//! debounce = 50
//! ignore = ["dist/**"]
//!
//! [[task]]
//! name = "server"
//! watch = ["src/**", "Cargo.toml"]
//! run = "cargo run"
//! ```
//!
//! Unknown keys are rejected so that typos are reported instead of silently
//! ignored.

use std::collections::{BTreeMap, HashSet};
use std::fmt;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use serde::de::{self, Deserializer, SeqAccess, Visitor};
use serde::Deserialize;

/// File names searched for by [`discover`], in order of preference.
pub const CONFIG_FILE_NAMES: &[&str] = &["Anymon.toml", "anymon.toml", ".anymon.toml"];

/// Default debounce window in milliseconds.
pub const DEFAULT_DEBOUNCE_MS: u64 = 50;

/// Default time in milliseconds a process gets to exit after `SIGTERM`.
pub const DEFAULT_KILL_TIMEOUT_MS: u64 = 2000;

/// A parsed config file.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Settings shared by all tasks.
    #[serde(default)]
    pub global: GlobalConfig,
    /// The `[[task]]` tables.
    #[serde(default, rename = "task")]
    pub tasks: Vec<TaskConfig>,
}

/// The `[global]` table.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GlobalConfig {
    /// Quiet period in milliseconds before a change triggers a run.
    pub debounce: Option<u64>,
    /// Glob patterns ignored by every task.
    #[serde(default, deserialize_with = "string_or_list")]
    pub ignore: Vec<String>,
    /// Skip files ignored by `.gitignore` (default `true`).
    pub gitignore: Option<bool>,
    /// Milliseconds a process gets to exit gracefully before it is killed.
    pub kill_timeout: Option<u64>,
    /// Shell for commands that need one (default `sh` on Unix, `cmd` on Windows).
    pub shell: Option<String>,
    /// Poll for changes every this many milliseconds instead of using native
    /// file-system events (for network drives, Docker volumes, WSL, ...).
    pub poll: Option<u64>,
}

/// A `[[task]]` table.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskConfig {
    /// Name shown in the output. Defaults to the program name.
    pub name: Option<String>,
    /// Glob patterns of files that trigger the task. Empty means everything.
    #[serde(default, deserialize_with = "string_or_list")]
    pub watch: Vec<String>,
    /// Glob patterns excluded for this task.
    #[serde(default, deserialize_with = "string_or_list")]
    pub ignore: Vec<String>,
    /// The command to run.
    pub run: Run,
    /// Restart a still-running process on change (default `true`). When
    /// `false`, a change while the task runs queues exactly one more run.
    pub restart: Option<bool>,
    /// Run the task when anymon starts (default `true`).
    pub run_on_start: Option<bool>,
    /// Working directory, relative to the config file.
    pub cwd: Option<PathBuf>,
    /// Extra environment variables.
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    /// Shell override for this task.
    pub shell: Option<String>,
}

/// A command, either as a string (`"cargo run"`) or as an argument vector
/// (`["cargo", "run"]`, never passed through a shell).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Run {
    /// A command line, parsed like a shell would.
    Command(String),
    /// A program and its arguments, passed on verbatim.
    Argv(Vec<String>),
}

impl Run {
    /// Returns `true` if the command is empty.
    pub fn is_empty(&self) -> bool {
        match self {
            Run::Command(cmd) => cmd.trim().is_empty(),
            Run::Argv(argv) => argv.first().is_none_or(|p| p.trim().is_empty()),
        }
    }

    /// The program name, if it can be determined without a shell.
    pub fn program_name(&self) -> Option<String> {
        let first = match self {
            Run::Command(cmd) => cmd.split_whitespace().next()?.to_string(),
            Run::Argv(argv) => argv.first()?.clone(),
        };
        let name = Path::new(first.trim_matches(['"', '\'']))
            .file_stem()?
            .to_str()?
            .to_string();
        let valid = !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '.'));
        valid.then_some(name)
    }
}

impl fmt::Display for Run {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Run::Command(cmd) => f.write_str(cmd.trim()),
            Run::Argv(argv) => f.write_str(&argv.join(" ")),
        }
    }
}

impl<'de> Deserialize<'de> for Run {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct RunVisitor;

        impl<'de> Visitor<'de> for RunVisitor {
            type Value = Run;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a command string or an array of strings")
            }

            fn visit_str<E: de::Error>(self, v: &str) -> Result<Run, E> {
                Ok(Run::Command(v.to_string()))
            }

            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Run, A::Error> {
                let mut argv = Vec::new();
                while let Some(arg) = seq.next_element::<String>()? {
                    argv.push(arg);
                }
                Ok(Run::Argv(argv))
            }
        }

        deserializer.deserialize_any(RunVisitor)
    }
}

fn string_or_list<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<String>, D::Error> {
    struct ListVisitor;

    impl<'de> Visitor<'de> for ListVisitor {
        type Value = Vec<String>;

        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("a glob pattern or an array of glob patterns")
        }

        fn visit_str<E: de::Error>(self, v: &str) -> Result<Vec<String>, E> {
            Ok(vec![v.to_string()])
        }

        fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Vec<String>, A::Error> {
            let mut list = Vec::new();
            while let Some(item) = seq.next_element::<String>()? {
                list.push(item);
            }
            Ok(list)
        }
    }

    deserializer.deserialize_any(ListVisitor)
}

/// Errors produced while loading or validating a config.
#[derive(Debug)]
pub enum ConfigError {
    /// The file could not be read.
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    /// The file is not valid TOML or does not match the schema.
    Parse {
        path: Option<PathBuf>,
        message: String,
    },
    /// The config parsed but is not usable.
    Invalid {
        path: Option<PathBuf>,
        message: String,
    },
}

impl ConfigError {
    fn with_path(self, path: &Path) -> Self {
        match self {
            ConfigError::Parse { message, .. } => ConfigError::Parse {
                path: Some(path.to_path_buf()),
                message,
            },
            ConfigError::Invalid { message, .. } => ConfigError::Invalid {
                path: Some(path.to_path_buf()),
                message,
            },
            other => other,
        }
    }

    fn invalid(message: impl Into<String>) -> Self {
        ConfigError::Invalid {
            path: None,
            message: message.into(),
        }
    }
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::Io { path, source } => {
                write!(f, "cannot read {}: {source}", path.display())
            }
            ConfigError::Parse { path, message } => {
                match path {
                    Some(path) => write!(f, "invalid config {}", path.display())?,
                    None => f.write_str("invalid config")?,
                }
                write!(f, ": {}", message.trim_end())
            }
            ConfigError::Invalid { path, message } => match path {
                Some(path) => write!(f, "invalid config {}: {message}", path.display()),
                None => write!(f, "invalid config: {message}"),
            },
        }
    }
}

impl std::error::Error for ConfigError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ConfigError::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl FromStr for Config {
    type Err = ConfigError;

    /// Parse and validate a config from TOML text.
    fn from_str(content: &str) -> Result<Self, Self::Err> {
        let config: Config = toml::from_str(content).map_err(|err| ConfigError::Parse {
            path: None,
            message: err.to_string(),
        })?;
        config.validate()?;
        Ok(config)
    }
}

impl Config {
    /// Read, parse and validate a config file.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let path = path.as_ref();
        let content = std::fs::read_to_string(path).map_err(|source| ConfigError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        content
            .parse()
            .map_err(|err: ConfigError| err.with_path(path))
    }

    /// Alias of [`Config::load`], kept for compatibility with anymon 0.x.
    pub fn from_toml(path: &str) -> Result<Self, ConfigError> {
        Self::load(path)
    }

    /// Check the config for mistakes that parsing alone does not catch.
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.tasks.is_empty() {
            return Err(ConfigError::invalid(
                "no tasks defined; add at least one [[task]] table",
            ));
        }
        for pattern in &self.global.ignore {
            if pattern.trim().is_empty() {
                return Err(ConfigError::invalid(
                    "[global] ignore contains an empty pattern",
                ));
            }
        }
        if let Some(shell) = &self.global.shell {
            if shell.trim().is_empty() {
                return Err(ConfigError::invalid("[global] shell is empty"));
            }
        }
        if self.global.poll == Some(0) {
            return Err(ConfigError::invalid(
                "[global] poll must be at least 1 millisecond",
            ));
        }

        let names = self.task_names();
        let mut seen = HashSet::new();
        for (index, (task, name)) in self.tasks.iter().zip(&names).enumerate() {
            let label = match &task.name {
                Some(_) => format!("task '{name}'"),
                None => format!("task #{}", index + 1),
            };
            if let Some(explicit) = &task.name {
                if explicit.trim().is_empty() {
                    return Err(ConfigError::invalid(format!("{label}: name is empty")));
                }
            }
            if !seen.insert(name) {
                return Err(ConfigError::invalid(format!(
                    "duplicate task name '{name}'; give each task a unique name"
                )));
            }
            if task.run.is_empty() {
                return Err(ConfigError::invalid(format!("{label}: `run` is empty")));
            }
            if task
                .watch
                .iter()
                .chain(&task.ignore)
                .any(|p| p.trim().is_empty())
            {
                return Err(ConfigError::invalid(format!(
                    "{label}: glob patterns must not be empty"
                )));
            }
            for key in task.env.keys() {
                if key.is_empty() || key.contains(['=', '\0']) {
                    return Err(ConfigError::invalid(format!(
                        "{label}: invalid environment variable name {key:?}"
                    )));
                }
            }
            if let Some(shell) = &task.shell {
                if shell.trim().is_empty() {
                    return Err(ConfigError::invalid(format!("{label}: shell is empty")));
                }
            }
        }
        Ok(())
    }

    /// The effective names of all tasks, in order: a task's `name`, or else
    /// the program it runs, or else `task-N`. Derived names never collide
    /// with other tasks; they get a numeric suffix (`cargo-2`) instead.
    pub fn task_names(&self) -> Vec<String> {
        let explicit: HashSet<&str> = self
            .tasks
            .iter()
            .filter_map(|t| t.name.as_deref().map(str::trim))
            .collect();
        let mut used = HashSet::new();
        let mut names = Vec::with_capacity(self.tasks.len());
        for (index, task) in self.tasks.iter().enumerate() {
            let name = match &task.name {
                Some(name) => name.trim().to_string(),
                None => {
                    let base = task
                        .run
                        .program_name()
                        .unwrap_or_else(|| format!("task-{}", index + 1));
                    let free = |candidate: &str| {
                        !explicit.contains(candidate) && !used.contains(candidate)
                    };
                    if free(&base) {
                        base
                    } else {
                        (2..)
                            .map(|n| format!("{base}-{n}"))
                            .find(|candidate| free(candidate))
                            .expect("an unused name exists")
                    }
                }
            };
            used.insert(name.clone());
            names.push(name);
        }
        names
    }

    /// Debounce window, falling back to [`DEFAULT_DEBOUNCE_MS`].
    pub fn debounce_ms(&self) -> u64 {
        self.global.debounce.unwrap_or(DEFAULT_DEBOUNCE_MS)
    }

    /// Kill timeout, falling back to [`DEFAULT_KILL_TIMEOUT_MS`].
    pub fn kill_timeout_ms(&self) -> u64 {
        self.global.kill_timeout.unwrap_or(DEFAULT_KILL_TIMEOUT_MS)
    }
}

/// Find a config file in `start` or any of its parent directories.
///
/// File names are compared exactly, so the returned path has the same
/// spelling as the file on disk even on case-insensitive file systems.
pub fn discover(start: impl AsRef<Path>) -> Option<PathBuf> {
    for dir in start.as_ref().ancestors() {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        let names: HashSet<String> = entries
            .filter_map(Result::ok)
            .filter(|e| e.file_type().map(|t| !t.is_dir()).unwrap_or(false))
            .filter_map(|e| e.file_name().into_string().ok())
            .collect();
        if let Some(name) = CONFIG_FILE_NAMES.iter().find(|n| names.contains(**n)) {
            return Some(dir.join(name));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(toml: &str) -> Result<Config, ConfigError> {
        toml.parse()
    }

    #[test]
    fn parses_the_minimal_config() {
        let cfg = parse(
            r#"
            [[task]]
            name = "t"
            watch = ["**/*.rs"]
            run = "echo ok"
            "#,
        )
        .unwrap();
        assert_eq!(cfg.tasks.len(), 1);
        let task = &cfg.tasks[0];
        assert_eq!(task.name.as_deref(), Some("t"));
        assert_eq!(task.watch, vec!["**/*.rs"]);
        assert_eq!(task.run, Run::Command("echo ok".into()));
        assert_eq!(task.restart, None);
        assert_eq!(cfg.global, GlobalConfig::default());
        assert_eq!(cfg.debounce_ms(), DEFAULT_DEBOUNCE_MS);
        assert_eq!(cfg.kill_timeout_ms(), DEFAULT_KILL_TIMEOUT_MS);
    }

    #[test]
    fn parses_every_option() {
        let cfg = parse(
            r#"
            [global]
            debounce = 10
            ignore = ["dist/**", "*.log"]
            gitignore = false
            kill_timeout = 500
            shell = "bash"
            poll = 250

            [[task]]
            name = "server"
            watch = ["src/**"]
            ignore = ["src/generated/**"]
            run = ["cargo", "run", "--", "--port", "8080"]
            restart = false
            run_on_start = false
            cwd = "backend"
            env = { RUST_LOG = "debug", PORT = "8080" }
            shell = "pwsh"
            "#,
        )
        .unwrap();
        assert_eq!(cfg.global.debounce, Some(10));
        assert_eq!(cfg.global.ignore, vec!["dist/**", "*.log"]);
        assert_eq!(cfg.global.gitignore, Some(false));
        assert_eq!(cfg.kill_timeout_ms(), 500);
        assert_eq!(cfg.global.shell.as_deref(), Some("bash"));
        assert_eq!(cfg.global.poll, Some(250));

        let task = &cfg.tasks[0];
        assert_eq!(
            task.run,
            Run::Argv(
                vec!["cargo", "run", "--", "--port", "8080"]
                    .into_iter()
                    .map(String::from)
                    .collect()
            )
        );
        assert_eq!(task.ignore, vec!["src/generated/**"]);
        assert_eq!(task.restart, Some(false));
        assert_eq!(task.run_on_start, Some(false));
        assert_eq!(task.cwd.as_deref(), Some(Path::new("backend")));
        assert_eq!(task.env.get("RUST_LOG").map(String::as_str), Some("debug"));
        assert_eq!(task.shell.as_deref(), Some("pwsh"));
    }

    #[test]
    fn accepts_a_single_pattern_string() {
        let cfg = parse(
            r#"
            [global]
            ignore = "*.log"
            [[task]]
            watch = "src/**"
            run = "make"
            "#,
        )
        .unwrap();
        assert_eq!(cfg.global.ignore, vec!["*.log"]);
        assert_eq!(cfg.tasks[0].watch, vec!["src/**"]);
    }

    #[test]
    fn global_table_may_follow_tasks() {
        let cfg = parse(
            r#"
            [[task]]
            name = "example"
            watch = ["src/**/*.rs"]
            run = "echo 'Hello world from Anymon!'"
            restart = true

            [global]
            debounce = 50
            ignore = ["**/target/**"]
            "#,
        )
        .unwrap();
        assert_eq!(cfg.global.debounce, Some(50));
        assert_eq!(cfg.tasks[0].restart, Some(true));
    }

    #[test]
    fn rejects_unknown_fields_with_location() {
        let err = parse(
            r#"
            [[task]]
            name = "t"
            run = "make"
            restrat = true
            "#,
        )
        .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("unknown field `restrat`"), "{msg}");
        assert!(msg.contains("line 5"), "{msg}");

        let err = parse("[global]\ndebounse = 5\n[[task]]\nrun = \"x\"").unwrap_err();
        assert!(err.to_string().contains("debounse"));

        let err = parse("[globals]\n[[task]]\nrun = \"x\"").unwrap_err();
        assert!(err.to_string().contains("globals"));
    }

    #[test]
    fn rejects_wrong_types() {
        let err = parse("[[task]]\nrun = 5").unwrap_err();
        assert!(
            err.to_string()
                .contains("a command string or an array of strings"),
            "{err}"
        );

        let err = parse("[[task]]\nrun = \"x\"\nwatch = 3").unwrap_err();
        assert!(err.to_string().contains("glob pattern"), "{err}");

        let err = parse("[global]\ndebounce = -1\n[[task]]\nrun = \"x\"").unwrap_err();
        assert!(matches!(err, ConfigError::Parse { .. }));
    }

    #[test]
    fn requires_run() {
        let err = parse("[[task]]\nname = \"t\"").unwrap_err();
        assert!(err.to_string().contains("missing field `run`"), "{err}");
    }

    #[test]
    fn validates_tasks() {
        let cases = [
            ("", "no tasks defined"),
            ("[global]\ndebounce = 1", "no tasks defined"),
            ("[[task]]\nrun = \"  \"", "`run` is empty"),
            ("[[task]]\nrun = []", "`run` is empty"),
            ("[[task]]\nname = \"\"\nrun = \"x\"", "name is empty"),
            (
                "[[task]]\nname = \"a\"\nrun = \"x\"\n[[task]]\nname = \"a\"\nrun = \"y\"",
                "duplicate task name 'a'",
            ),
            ("[[task]]\nrun = \"x\"\nwatch = [\"\"]", "must not be empty"),
            (
                "[[task]]\nrun = \"x\"\nenv = { \"A=B\" = \"1\" }",
                "invalid environment variable",
            ),
            (
                "[global]\nignore = [\" \"]\n[[task]]\nrun = \"x\"",
                "empty pattern",
            ),
            (
                "[global]\nshell = \"\"\n[[task]]\nrun = \"x\"",
                "shell is empty",
            ),
            (
                "[global]\npoll = 0\n[[task]]\nrun = \"x\"",
                "poll must be at least",
            ),
        ];
        for (toml, expected) in cases {
            let err = parse(toml).unwrap_err();
            assert!(
                matches!(err, ConfigError::Invalid { .. }),
                "{toml:?} should be invalid, got {err}"
            );
            assert!(err.to_string().contains(expected), "{toml:?}: {err}");
        }
    }

    #[test]
    fn derives_task_names() {
        let cfg = parse(
            r#"
            [[task]]
            run = "cargo build"
            [[task]]
            run = ["./scripts/lint.sh", "--fix"]
            [[task]]
            run = "cargo test"
            [[task]]
            name = "cargo-3"
            run = "cargo check"
            [[task]]
            run = "cargo doc"
            [[task]]
            run = "$(which make)"
            "#,
        )
        .unwrap();
        assert_eq!(
            cfg.task_names(),
            ["cargo", "lint", "cargo-2", "cargo-3", "cargo-4", "task-6"]
        );
    }

    #[test]
    fn explicit_names_win_over_derived_duplicates() {
        let cfg = parse(
            r#"
            [[task]]
            run = "make"
            [[task]]
            name = "make"
            run = "make test"
            "#,
        )
        .unwrap();
        // The unnamed task yields its name to the explicit one.
        assert_eq!(cfg.task_names(), ["make-2", "make"]);
    }

    #[test]
    fn naming_many_unnamed_tasks_is_fast() {
        let toml = "[[task]]\nrun = \"make\"\n".repeat(200);
        let cfg = parse(&toml).unwrap();
        let names = cfg.task_names();
        assert_eq!(names[0], "make");
        assert_eq!(names[199], "make-200");
    }

    #[test]
    fn run_display_and_program_name() {
        assert_eq!(Run::Command("  go run . ".into()).to_string(), "go run .");
        assert_eq!(Run::Argv(vec!["a".into(), "b".into()]).to_string(), "a b");
        assert_eq!(
            Run::Command("npm run dev".into()).program_name().as_deref(),
            Some("npm")
        );
        assert_eq!(
            Run::Argv(vec!["/usr/bin/python3".into()])
                .program_name()
                .as_deref(),
            Some("python3")
        );
    }

    #[test]
    fn load_reports_the_path() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("Anymon.toml");
        let err = Config::load(&missing).unwrap_err();
        assert!(matches!(err, ConfigError::Io { .. }));
        assert!(err.to_string().contains("Anymon.toml"));

        std::fs::write(&missing, "[[task]]\nrun = 1\n").unwrap();
        let err = Config::load(&missing).unwrap_err();
        assert!(err
            .to_string()
            .starts_with(&format!("invalid config {}", missing.display())));

        std::fs::write(&missing, "[[task]]\nrun = \"make\"\n").unwrap();
        let cfg = Config::from_toml(missing.to_str().unwrap()).unwrap();
        assert_eq!(cfg.tasks.len(), 1);
    }

    #[test]
    fn discovers_config_in_parent_directories() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("a").join("b");
        std::fs::create_dir_all(&nested).unwrap();
        assert!(discover(&nested).is_none_or(|found| !found.starts_with(dir.path())));

        std::fs::write(dir.path().join("anymon.toml"), "").unwrap();
        assert_eq!(discover(&nested), Some(dir.path().join("anymon.toml")));

        std::fs::write(dir.path().join("a").join(".anymon.toml"), "").unwrap();
        assert_eq!(
            discover(&nested),
            Some(dir.path().join("a").join(".anymon.toml"))
        );

        std::fs::write(nested.join("Anymon.toml"), "").unwrap();
        assert_eq!(discover(&nested), Some(nested.join("Anymon.toml")));
    }

    #[test]
    fn discovery_ignores_directories_with_config_names() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("Anymon.toml")).unwrap();
        std::fs::write(dir.path().join(".anymon.toml"), "").unwrap();
        assert_eq!(discover(dir.path()), Some(dir.path().join(".anymon.toml")));
    }
}
