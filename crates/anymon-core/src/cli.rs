//! Command-line interface definition.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

const EXAMPLES: &str = "\
Examples:
  anymon                        Run the tasks in Anymon.toml and restart them on change
  anymon -e rs -- cargo run     Restart `cargo run` whenever a .rs file changes
  anymon -w src -- npm test     Re-run `npm test` when something in src/ changes
  anymon --once                 Run every task once and exit (for scripts and CI)
  anymon init                   Create an Anymon.toml for the current project

While watching, type `rs` + Enter to restart, `s` for status and `q` to quit.
Documentation: https://github.com/builtbyjonas/anymon#readme";

#[derive(Parser, Debug)]
#[command(
    name = "anymon",
    version,
    about = "Ultra-fast, language-agnostic file watcher that runs anything on change.",
    after_help = EXAMPLES,
    override_usage = "anymon [OPTIONS] [-- <COMMAND>...]\n       anymon <SUBCOMMAND> [OPTIONS]",
    subcommand_value_name = "SUBCOMMAND",
    subcommand_help_heading = "Subcommands",
    disable_help_subcommand = true,
    max_term_width = 100
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    #[command(flatten)]
    pub watch: WatchArgs,

    #[command(flatten)]
    pub global: GlobalArgs,
}

#[derive(Args, Debug, Clone, Default)]
#[command(next_help_heading = "Global options")]
pub struct GlobalArgs {
    /// Config file [default: Anymon.toml in this or a parent directory]
    #[arg(short, long, global = true, value_name = "FILE")]
    pub config: Option<PathBuf>,

    /// When to use colors
    #[arg(long, global = true, value_name = "WHEN", default_value = "auto")]
    pub color: ColorChoice,

    /// Only print errors and failed runs
    #[arg(short, long, global = true, conflicts_with = "verbose")]
    pub quiet: bool,

    /// Also print file events and other details
    #[arg(short, long, global = true)]
    pub verbose: bool,
}

#[derive(Args, Debug, Clone, Default, PartialEq, Eq)]
#[command(next_help_heading = "Watch options")]
pub struct WatchArgs {
    /// Only watch these files or directories (repeatable)
    #[arg(short = 'w', long = "watch", value_name = "PATH")]
    pub paths: Vec<PathBuf>,

    /// Glob of files that trigger COMMAND (repeatable)
    #[arg(short = 'p', long = "pattern", value_name = "GLOB")]
    pub patterns: Vec<String>,

    /// File extensions that trigger COMMAND, e.g. `-e rs,toml`
    #[arg(short = 'e', long = "exts", value_name = "EXT", value_delimiter = ',')]
    pub exts: Vec<String>,

    /// Ignore files matching this glob (repeatable)
    #[arg(short = 'i', long = "ignore", value_name = "GLOB")]
    pub ignore: Vec<String>,

    /// Only run this task from the config (repeatable)
    #[arg(short = 't', long = "task", value_name = "NAME")]
    pub tasks: Vec<String>,

    /// Milliseconds to wait for changes to settle [default: 50]
    #[arg(short = 'd', long, value_name = "MS")]
    pub debounce: Option<u64>,

    /// Milliseconds a process gets to exit before it is killed [default: 2000]
    #[arg(long, value_name = "MS")]
    pub kill_timeout: Option<u64>,

    /// Do not skip files that .gitignore excludes
    #[arg(long)]
    pub no_gitignore: bool,

    /// Poll for changes every MS milliseconds (for network drives, Docker, WSL)
    #[arg(long, value_name = "MS", num_args = 0..=1, default_missing_value = "500")]
    pub poll: Option<u64>,

    /// Let a running COMMAND finish, then run it again, instead of restarting it
    #[arg(long)]
    pub no_restart: bool,

    /// Run once and exit with the status of the first failed task
    #[arg(long)]
    pub once: bool,

    /// Shell for COMMAND if it needs one [default: sh, or cmd on Windows]
    #[arg(long, value_name = "SHELL")]
    pub shell: Option<String>,

    /// Command to run instead of the tasks from the config
    #[arg(last = true, value_name = "COMMAND", help_heading = "Arguments")]
    pub command: Vec<String>,
}

impl WatchArgs {
    /// Combine options given before a `watch` subcommand with those after it.
    pub fn merge(self, later: WatchArgs) -> WatchArgs {
        fn join<T>(mut a: Vec<T>, b: Vec<T>) -> Vec<T> {
            a.extend(b);
            a
        }
        WatchArgs {
            paths: join(self.paths, later.paths),
            patterns: join(self.patterns, later.patterns),
            exts: join(self.exts, later.exts),
            ignore: join(self.ignore, later.ignore),
            tasks: join(self.tasks, later.tasks),
            debounce: later.debounce.or(self.debounce),
            kill_timeout: later.kill_timeout.or(self.kill_timeout),
            no_gitignore: self.no_gitignore || later.no_gitignore,
            poll: later.poll.or(self.poll),
            no_restart: self.no_restart || later.no_restart,
            once: self.once || later.once,
            shell: later.shell.or(self.shell),
            command: if later.command.is_empty() {
                self.command
            } else {
                later.command
            },
        }
    }

    /// Returns `true` if no watch option was given.
    pub fn is_empty(&self) -> bool {
        *self == WatchArgs::default()
    }
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Watch files and run tasks when they change (default)
    Watch(WatchArgs),

    /// Run a command once and exit with its status
    Run {
        /// Shell for the command if it needs one
        #[arg(long, value_name = "SHELL")]
        shell: Option<String>,

        /// The command, as one string or as separate arguments
        #[arg(
            required = true,
            trailing_var_arg = true,
            allow_hyphen_values = true,
            value_name = "COMMAND"
        )]
        command: Vec<String>,
    },

    /// Create an Anymon.toml for the current project
    Init {
        /// Overwrite an existing config file
        #[arg(long)]
        force: bool,
    },

    /// Validate the config file and show the resolved tasks
    #[command(alias = "debug")]
    Check,

    /// Update anymon to the latest release
    Update {
        /// Only check whether a newer version is available
        #[arg(long)]
        check: bool,
    },

    /// Print a shell completion script
    Completions {
        /// Target shell
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
pub enum ColorChoice {
    #[default]
    Auto,
    Always,
    Never,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    fn parse(args: &[&str]) -> Cli {
        Cli::try_parse_from(std::iter::once("anymon").chain(args.iter().copied())).unwrap()
    }

    #[test]
    fn definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn no_arguments_means_watch() {
        let cli = parse(&[]);
        assert!(cli.command.is_none());
        assert!(cli.watch.is_empty());
    }

    #[test]
    fn parses_ad_hoc_commands() {
        let cli = parse(&[
            "-e",
            "rs,toml",
            "-w",
            "src",
            "--",
            "cargo",
            "run",
            "--release",
        ]);
        assert_eq!(cli.watch.exts, vec!["rs", "toml"]);
        assert_eq!(cli.watch.paths, vec![PathBuf::from("src")]);
        assert_eq!(cli.watch.command, vec!["cargo", "run", "--release"]);

        let cli = parse(&["watch", "--once", "--", "make"]);
        match cli.command {
            Some(Command::Watch(args)) => {
                assert!(args.once);
                assert_eq!(args.command, vec!["make"]);
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn keeps_the_0x_syntax_working() {
        let cli = parse(&["--config", "Anymon.toml", "watch"]);
        assert_eq!(cli.global.config, Some(PathBuf::from("Anymon.toml")));
        assert!(matches!(cli.command, Some(Command::Watch(_))));

        let cli = parse(&["--debounce", "100", "watch", "--kill-timeout", "5"]);
        let Some(Command::Watch(sub)) = cli.command else {
            panic!("expected watch");
        };
        let merged = cli.watch.merge(sub);
        assert_eq!(merged.debounce, Some(100));
        assert_eq!(merged.kill_timeout, Some(5));

        let cli = parse(&["run", "cargo test"]);
        assert!(
            matches!(cli.command, Some(Command::Run { ref command, .. }) if command == &["cargo test"])
        );

        let cli = parse(&["debug"]);
        assert!(matches!(cli.command, Some(Command::Check)));
    }

    #[test]
    fn run_takes_trailing_arguments() {
        let cli = parse(&["run", "cargo", "test", "--all", "-q"]);
        match cli.command {
            Some(Command::Run { command, shell }) => {
                assert_eq!(command, vec!["cargo", "test", "--all", "-q"]);
                assert_eq!(shell, None);
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn global_flags_work_after_subcommands() {
        let cli = parse(&["check", "-c", "x.toml", "--color", "never", "-q"]);
        assert_eq!(cli.global.config, Some(PathBuf::from("x.toml")));
        assert_eq!(cli.global.color, ColorChoice::Never);
        assert!(cli.global.quiet);
    }

    #[test]
    fn poll_interval_is_optional() {
        assert_eq!(parse(&["--poll"]).watch.poll, Some(500));
        assert_eq!(parse(&["--poll", "100"]).watch.poll, Some(100));
        assert_eq!(parse(&["--poll=250", "--", "make"]).watch.poll, Some(250));
        assert_eq!(parse(&[]).watch.poll, None);
    }

    #[test]
    fn rejects_conflicting_verbosity() {
        assert!(Cli::try_parse_from(["anymon", "-q", "-v"]).is_err());
    }

    #[test]
    fn merge_prefers_later_values() {
        let early = WatchArgs {
            debounce: Some(1),
            ignore: vec!["a".into()],
            command: vec!["x".into()],
            ..WatchArgs::default()
        };
        let late = WatchArgs {
            debounce: Some(2),
            ignore: vec!["b".into()],
            once: true,
            ..WatchArgs::default()
        };
        let merged = early.merge(late);
        assert_eq!(merged.debounce, Some(2));
        assert_eq!(merged.ignore, vec!["a", "b"]);
        assert_eq!(merged.command, vec!["x"]);
        assert!(merged.once);
    }
}
