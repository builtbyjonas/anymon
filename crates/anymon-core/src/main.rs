//! The `anymon` command-line tool.

mod check;
mod cli;
mod init;
mod update;

use std::io::IsTerminal;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use anymon_config::{Config, GlobalConfig, Run, TaskConfig};
use anymon_runner::ui::{self, Verbosity};
use anymon_runner::{paths, Plan, PlanOptions};
use anymon_shell::{CommandLine, Shell, SpawnOptions};
use clap::{CommandFactory, Parser};

use cli::{Cli, ColorChoice, Command, GlobalArgs, WatchArgs};

/// An error together with the exit code it should produce.
struct Failure {
    code: i32,
    error: anyhow::Error,
}

impl Failure {
    /// A problem with the config or the command line (exit code 2).
    fn usage(error: impl Into<anyhow::Error>) -> Self {
        Failure {
            code: 2,
            error: error.into(),
        }
    }
}

impl<E: Into<anyhow::Error>> From<E> for Failure {
    fn from(error: E) -> Self {
        Failure {
            code: 1,
            error: error.into(),
        }
    }
}

type Outcome = std::result::Result<i32, Failure>;

fn main() {
    let cli = Cli::parse();
    configure_output(&cli.global);
    update::cleanup_previous_update();

    let code = match dispatch(cli) {
        Ok(code) => code,
        Err(failure) => {
            ui::error(format!("{:#}", failure.error));
            failure.code
        }
    };
    // Exit right away: a thread blocked on reading stdin must not keep the
    // process alive.
    std::process::exit(code);
}

fn configure_output(global: &GlobalArgs) {
    let color = match global.color {
        ColorChoice::Always => true,
        ColorChoice::Never => false,
        ColorChoice::Auto => {
            let forced = ["CLICOLOR_FORCE", "FORCE_COLOR"]
                .iter()
                .any(|v| std::env::var_os(v).is_some_and(|v| !v.is_empty() && v != "0"));
            let disabled = std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty())
                || std::env::var_os("TERM").is_some_and(|t| t == "dumb");
            forced || (!disabled && std::io::stderr().is_terminal())
        }
    };
    ui::set_color(color);
    ui::set_verbosity(if global.quiet {
        Verbosity::Quiet
    } else if global.verbose {
        Verbosity::Verbose
    } else {
        Verbosity::Normal
    });
}

fn dispatch(cli: Cli) -> Outcome {
    let global = cli.global;
    match cli.command {
        None => watch(cli.watch, &global),
        Some(Command::Watch(args)) => watch(cli.watch.merge(args), &global),
        Some(other) => {
            if !cli.watch.is_empty() {
                return Err(Failure::usage(anyhow!(
                    "watch options must be used with `anymon` or `anymon watch`"
                )));
            }
            match other {
                Command::Watch(_) => unreachable!("handled above"),
                Command::Run { command, shell } => run_command(command, shell),
                Command::Init { force } => init(force),
                Command::Check => check(&global),
                Command::Update { check } => Ok(update::run(check)?),
                Command::Completions { shell } => {
                    clap_complete::generate(
                        shell,
                        &mut Cli::command(),
                        "anymon",
                        &mut std::io::stdout(),
                    );
                    Ok(0)
                }
            }
        }
    }
}

fn runtime() -> Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("failed to start the async runtime")
}

fn plan_options(args: &WatchArgs) -> PlanOptions {
    PlanOptions {
        paths: args.paths.clone(),
        debounce_ms: args.debounce,
        kill_timeout_ms: args.kill_timeout,
        gitignore: args.no_gitignore.then_some(false),
        poll_ms: args.poll,
        ignore: args.ignore.clone(),
        tasks: args.tasks.clone(),
    }
}

fn watch(args: WatchArgs, global: &GlobalArgs) -> Outcome {
    let cwd = std::env::current_dir().context("cannot determine the current directory")?;
    let options = plan_options(&args);

    if !args.command.is_empty() {
        if global.config.is_some() {
            return Err(Failure::usage(anyhow!(
                "--config cannot be combined with a command; put the command in the config instead"
            )));
        }
        if !args.tasks.is_empty() {
            return Err(Failure::usage(anyhow!(
                "--task cannot be combined with a command"
            )));
        }
        let root = paths::canonicalize(&cwd)?;
        let config = ad_hoc_config(&args, &root);
        let plan = Plan::new(&config, &root, None, &options).map_err(Failure::usage)?;
        return execute(plan, args.once, None);
    }

    for (flag, used) in [
        ("--pattern", !args.patterns.is_empty()),
        ("--exts", !args.exts.is_empty()),
        ("--no-restart", args.no_restart),
        ("--shell", args.shell.is_some()),
    ] {
        if used {
            return Err(Failure::usage(anyhow!(
                "{flag} only applies to a command given after `--`, e.g. `anymon {flag} ... -- cargo test`"
            )));
        }
    }

    let config_path = config_path(global, &cwd)?;
    let load = || -> Result<Plan> {
        let config = Config::load(&config_path)?;
        let root = config_path.parent().unwrap_or(&cwd);
        Plan::new(&config, root, Some(&config_path), &options)
    };
    let plan = load().map_err(Failure::usage)?;
    execute(plan, args.once, Some(&load))
}

fn execute(plan: Plan, once: bool, reload: Option<anymon_runner::Reloader<'_>>) -> Outcome {
    let rt = runtime()?;
    if once {
        Ok(rt.block_on(anymon_runner::run_once(plan))?)
    } else {
        rt.block_on(anymon_runner::watch(plan, reload))?;
        Ok(0)
    }
}

/// The config file to use: `--config`, or the closest `Anymon.toml`.
fn config_path(global: &GlobalArgs, cwd: &Path) -> std::result::Result<PathBuf, Failure> {
    match &global.config {
        Some(path) => {
            let path = cwd.join(path);
            if !path.is_file() {
                return Err(Failure::usage(anyhow!(
                    "config file {} not found",
                    path.display()
                )));
            }
            Ok(path)
        }
        None => anymon_config::discover(cwd).ok_or_else(|| {
            Failure::usage(anyhow!(
                "no Anymon.toml found in {} or its parent directories\n\n  \
                 Create one with `anymon init`, or run a command directly:\n    \
                 anymon -e rs -- cargo run",
                cwd.display()
            ))
        }),
    }
}

/// Build a single-task config for `anymon [options] -- COMMAND`.
fn ad_hoc_config(args: &WatchArgs, root: &Path) -> Config {
    let run = match args.command.as_slice() {
        [single] => Run::Command(single.clone()),
        argv => Run::Argv(argv.to_vec()),
    };

    let exts: Vec<String> = args
        .exts
        .iter()
        .map(|e| {
            e.trim()
                .trim_start_matches("*.")
                .trim_start_matches('.')
                .to_string()
        })
        .filter(|e| !e.is_empty())
        .collect();
    let mut globs: Vec<String> = args.patterns.clone();
    globs.extend(exts.iter().map(|ext| format!("*.{ext}")));

    // With --watch, apply the filters inside each watched path so that paths
    // outside the current directory work as expected.
    let watch = if args.paths.is_empty() {
        globs
    } else {
        let mut watch = Vec::new();
        for path in &args.paths {
            let resolved = paths::resolve(path, root);
            let prefix = match resolved.strip_prefix(root) {
                Ok(rel) => rel.to_string_lossy().replace('\\', "/"),
                Err(_) => escape_glob(&resolved.to_string_lossy().replace('\\', "/")),
            };
            if !resolved.is_dir() {
                watch.push(if prefix.is_empty() {
                    "**".into()
                } else {
                    format!("/{prefix}")
                });
                continue;
            }
            let base = if prefix.is_empty() {
                String::new()
            } else {
                format!("{}/", prefix.trim_end_matches('/'))
            };
            let anchor = if base.is_empty() || base.starts_with('/') || base.contains(':') {
                ""
            } else {
                "/"
            };
            if globs.is_empty() {
                watch.push(format!("{anchor}{base}**"));
            }
            for glob in &globs {
                let glob = if glob.contains('/') {
                    glob.clone()
                } else {
                    format!("**/{glob}")
                };
                watch.push(format!("{anchor}{base}{glob}"));
            }
        }
        watch
    };

    Config {
        global: GlobalConfig {
            shell: args.shell.clone(),
            ..GlobalConfig::default()
        },
        tasks: vec![TaskConfig {
            name: None,
            watch,
            ignore: Vec::new(),
            run,
            restart: Some(!args.no_restart),
            run_on_start: Some(true),
            cwd: None,
            env: Default::default(),
            shell: None,
        }],
    }
}

fn escape_glob(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if matches!(c, '*' | '?' | '[' | ']' | '{' | '}') {
            out.push('[');
            out.push(c);
            out.push(']');
        } else {
            out.push(c);
        }
    }
    out
}

fn run_command(command: Vec<String>, shell: Option<String>) -> Outcome {
    let parsed = match command.as_slice() {
        [single] => CommandLine::parse(single),
        argv => CommandLine::from_argv(argv.to_vec()),
    }
    .map_err(|err| Failure::usage(anyhow!("invalid command: {err}")))?;

    let options = SpawnOptions {
        shell: shell.as_deref().map(Shell::from_name).unwrap_or_default(),
        cwd: None,
        env: Vec::new(),
        // The command is in the foreground: it may read stdin and receives
        // Ctrl-C from the terminal directly.
        inherit_stdin: true,
        isolate: false,
    };
    ui::detail(format!("$ {parsed}"));

    let rt = runtime()?;
    let code = rt.block_on(async {
        let mut process = anymon_shell::spawn(&parsed, &options)
            .with_context(|| format!("failed to run `{parsed}`"))?;
        loop {
            tokio::select! {
                status = process.wait() => return Ok::<_, anyhow::Error>(anymon_shell::exit_code(&status?)),
                // The command handles Ctrl-C itself; wait for it to exit.
                _ = tokio::signal::ctrl_c() => {}
            }
        }
    })?;
    Ok(code)
}

fn init(force: bool) -> Outcome {
    let cwd = std::env::current_dir()?;
    if !force {
        if let Some(existing) = anymon_config::CONFIG_FILE_NAMES
            .iter()
            .map(|name| cwd.join(name))
            .find(|path| path.is_file())
        {
            return Err(Failure::usage(anyhow!(
                "{} already exists (use --force to overwrite it)",
                existing.display()
            )));
        }
    }
    let template = init::template_for(&cwd);
    let path = cwd.join("Anymon.toml");
    std::fs::write(&path, &template.content)
        .with_context(|| format!("cannot write {}", path.display()))?;
    println!(
        "Created {} for a {} project.",
        path.display(),
        template.kind
    );
    println!("Review the task, then start watching with `anymon`.");
    Ok(0)
}

fn check(global: &GlobalArgs) -> Outcome {
    let cwd = std::env::current_dir()?;
    let path = config_path(global, &cwd)?;
    let config = Config::load(&path).map_err(Failure::usage)?;
    let root = path.parent().unwrap_or(&cwd);
    let plan =
        Plan::new(&config, root, Some(&path), &PlanOptions::default()).map_err(Failure::usage)?;
    print!("{}", check::describe(&plan));
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(command: &[&str]) -> WatchArgs {
        WatchArgs {
            command: command.iter().map(|s| s.to_string()).collect(),
            ..WatchArgs::default()
        }
    }

    #[test]
    fn ad_hoc_single_argument_is_a_command_line() {
        let cfg = ad_hoc_config(&args(&["cargo test && cargo doc"]), Path::new("/p"));
        assert_eq!(
            cfg.tasks[0].run,
            Run::Command("cargo test && cargo doc".into())
        );
        assert!(cfg.tasks[0].watch.is_empty());
        assert_eq!(cfg.tasks[0].restart, Some(true));
    }

    #[test]
    fn ad_hoc_multiple_arguments_are_an_argv() {
        let mut a = args(&["cargo", "run", "--", "--port", "80"]);
        a.exts = vec!["rs".into(), ".toml".into(), "*.md".into(), " ".into()];
        a.patterns = vec!["assets/**".into()];
        a.no_restart = true;
        let cfg = ad_hoc_config(&a, Path::new("/p"));
        assert!(matches!(&cfg.tasks[0].run, Run::Argv(v) if v.len() == 5));
        assert_eq!(
            cfg.tasks[0].watch,
            vec!["assets/**", "*.rs", "*.toml", "*.md"]
        );
        assert_eq!(cfg.tasks[0].restart, Some(false));
    }

    #[test]
    fn ad_hoc_watch_paths_scope_the_filters() {
        let dir = tempfile::tempdir().unwrap();
        let root = paths::canonicalize(dir.path()).unwrap();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("main.py"), "").unwrap();

        let mut a = args(&["make"]);
        a.paths = vec![root.join("src"), root.join("main.py")];
        a.exts = vec!["rs".into()];
        let cfg = ad_hoc_config(&a, &root);
        assert_eq!(cfg.tasks[0].watch, vec!["/src/**/*.rs", "/main.py"]);

        let mut a = args(&["make"]);
        a.paths = vec![root.clone()];
        let cfg = ad_hoc_config(&a, &root);
        assert_eq!(cfg.tasks[0].watch, vec!["**"]);

        // The generated patterns compile and match what was asked for.
        let mut a = args(&["make"]);
        a.paths = vec![root.join("src")];
        a.exts = vec!["rs".into()];
        let plan = Plan::new(
            &ad_hoc_config(&a, &root),
            &root,
            None,
            &PlanOptions::default(),
        )
        .unwrap();
        assert!(plan.tasks[0].watch.is_match(&root.join("src/deep/lib.rs")));
        assert!(!plan.tasks[0].watch.is_match(&root.join("other/lib.rs")));
    }

    #[test]
    fn escapes_glob_characters() {
        assert_eq!(escape_glob("/a/[b]/c*"), "/a/[[]b[]]/c[*]");
    }
}
