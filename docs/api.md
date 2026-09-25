# Library API

Anymon is mainly a command-line tool, but its crates can be used on their
own. Generate the full API documentation with:

```sh
cargo doc --workspace --no-deps --open
```

## `anymon-config`

The `Anymon.toml` schema.

- `Config`: a parsed config, with `global: GlobalConfig` and
  `tasks: Vec<TaskConfig>`. Use `Config::load(path)` or `"...".parse()`,
  both of which validate. `Config::task_names()` returns the effective task
  names.
- `GlobalConfig`, `TaskConfig`: the `[global]` and `[[task]]` tables.
- `Run`: a task command, either `Run::Command(String)` or
  `Run::Argv(Vec<String>)`.
- `discover(dir)`: find `Anymon.toml`/`anymon.toml`/`.anymon.toml` in `dir`
  or its parents.
- `ConfigError`: I/O, parse (with line and column) and validation errors.

## `anymon-shell`

Running commands and managing process trees.

- `CommandLine::parse(str)`: split a command like a shell would, or keep it as
  a shell script if it uses shell syntax. `CommandLine::from_argv` builds a
  direct command.
- `Shell`: `Sh`, `Cmd`, `PowerShell(..)` or `Other(..)`; `Shell::from_name`
  resolves names such as `bash` or `pwsh`.
- `spawn(&CommandLine, &SpawnOptions) -> Process`: start a command. With
  `isolate: true` (the default) it gets its own process group or job object.
- `Process::terminate(grace)`: stop the whole tree, gracefully first.
  `Process::wait`, `try_wait` and `kill` do what their names say.
  Dropping a `Process` kills what's left of it.
- `describe_exit`, `exit_code`: turn an `ExitStatus` into text or a shell-style
  exit code.
- `run_command(program, args)`: a blocking helper that captures output.

```rust
use anymon_shell::{spawn, CommandLine, SpawnOptions};
use std::time::Duration;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let command = CommandLine::parse("npm run dev")?;
    let mut process = spawn(&command, &SpawnOptions::default())?;
    // ... later: stop the whole process tree
    process.terminate(Duration::from_secs(2)).await?;
    Ok(())
}
```

## `anymon-runner`

Watching and supervision.

- `Plan::new(&config, root, config_path, &PlanOptions)`: validate and
  resolve a config. `PlanOptions` holds the command-line overrides.
- `watch(plan, reload)`: watch and run until the user quits or a signal
  arrives. `reload` (optional) rebuilds the plan when the config file
  changes.
- `run_once(plan)`: run all tasks once and return the exit code.
- `pattern::{Pattern, PathSet}`: the `.gitignore`-style glob matching
  anymon uses.
- `ui`: anymon's colored status output.

These functions need a Tokio runtime with the `process`, `signal`, `time`,
`sync` and `macros` features (a current-thread runtime is enough).

## Stability

The command-line interface and the config format follow semantic versioning
from 1.0.0 on. The Rust APIs of the library crates are provided as they are:
they are versioned together with the binary and may change in minor
releases when the tool needs it.
