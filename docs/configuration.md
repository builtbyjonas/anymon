# Configuration

Anymon reads its tasks from a TOML file, usually `Anymon.toml` in the root of
your project. `anymon init` creates one for you.

When started without `--config`, anymon looks for `Anymon.toml`,
`anymon.toml` or `.anymon.toml` in the current directory and then in each
parent directory, so you can start it from anywhere inside your project. The
directory that contains the config file is the **project root**: patterns and
working directories are relative to it, and tasks run there by default.

Anymon checks the config strictly. Unknown keys, wrong types and invalid
patterns are reported with the file, line and column, so typos never go
unnoticed. `anymon check` validates a config and prints what anymon will do
with it.

## Example

```toml
[global]
debounce = 50                  # ms to wait for changes to settle
ignore = ["dist/**", "*.log"]  # ignored by every task

[[task]]
name = "server"
watch = ["src/**", "Cargo.toml"]
run = "cargo run"
env = { RUST_LOG = "debug" }

[[task]]
name = "docs"
watch = "docs/**/*.md"
run = ["mdbook", "build"]
restart = false
run_on_start = false
```

## `[global]`

All keys are optional.

| Key            | Type             | Default | Description |
| -------------- | ---------------- | ------- | ----------- |
| `debounce`     | integer (ms)     | `50`    | How long changes must settle before tasks run. Changes that arrive within this window are handled together, as one run. |
| `ignore`       | pattern or list  | `[]`    | Patterns ignored by every task. |
| `gitignore`    | bool             | `true`  | Skip files excluded by `.gitignore` files. |
| `kill_timeout` | integer (ms)     | `2000`  | How long a process may take to exit after `SIGTERM` before it is killed. |
| `shell`        | string           | `sh` / `cmd` | Shell for commands that need one (see [Commands](#commands)). Examples: `"bash"`, `"zsh"`, `"pwsh"`, `"powershell"`. |
| `poll`         | integer (ms)     | none    | Poll for changes at this interval instead of using native file events. Use it where native events don't arrive: network drives, some Docker volumes, WSL paths under `/mnt`. |

## `[[task]]`

Each `[[task]]` table defines one command. Add as many as you like; they run
in parallel.

| Key            | Type               | Default | Description |
| -------------- | ------------------ | ------- | ----------- |
| `run`          | string or list     | required | The command. See [Commands](#commands). |
| `name`         | string             | program name | Shown in the output and used by `--task` and `rs <name>`. Must be unique. Defaults to the program name (`cargo`, `npm`, ...), with a number added if it's already taken. |
| `watch`        | pattern or list    | everything | Files that trigger the task. |
| `ignore`       | pattern or list    | `[]`    | Files this task ignores (in addition to the global ones). |
| `restart`      | bool               | `true`  | What happens when files change while the command is still running. `true` stops the command and starts it again, which suits servers. `false` lets the current run finish and then runs once more, which suits builds and tests. |
| `run_on_start` | bool               | `true`  | Run the task when anymon starts. With `false` it waits for the first change. |
| `cwd`          | string             | project root | Working directory, relative to the project root. |
| `env`          | table              | `{}`    | Extra environment variables, e.g. `env = { PORT = "8080" }`. |
| `shell`        | string             | global `shell` | Shell override for this task. |

## Patterns

`watch` and `ignore` take glob patterns. They follow the rules you know from
`.gitignore`:

| Pattern            | Matches |
| ------------------ | ------- |
| `*.rs`             | Every `.rs` file, at any depth. A pattern **without a slash** matches file names anywhere. |
| `Makefile`         | Every file named `Makefile`, at any depth. |
| `src/*.rs`         | `.rs` files directly inside `src/`. A pattern **with a slash** is relative to the project root. `*` never crosses a `/`. |
| `src/**/*.rs`      | `.rs` files anywhere below `src/`. `**` matches any number of directories. |
| `src` or `src/`    | Everything inside the `src/` directory. |
| `/Cargo.toml`      | Only the `Cargo.toml` in the project root. A leading `/` anchors the pattern. |
| `*.{js,ts}`        | Files ending in `.js` or `.ts`. |
| `img/[a-c]*.png`   | Character classes. |
| `../shared/**`     | Paths outside the project root work too. |
| `/abs/path/**`     | Absolute paths are allowed if the directory exists. |

On Windows you can write patterns with either `/` or `\`.

Anymon watches only the directories your patterns need. `watch = ["src/**"]`
watches `src/` and nothing else, which keeps anymon fast in large
repositories.

## What is ignored

A change triggers a task only if it matches the task's `watch` patterns and
none of these:

1. The task's `ignore` patterns and the global `ignore` patterns, including
   patterns given with `--ignore`.
2. Files excluded by `.gitignore`. Anymon reads the `.gitignore` files in
   the project, the ones in parent directories up to the repository root, and
   `.git/info/exclude`. It notices when you edit them. Disable this with
   `gitignore = false` or `--no-gitignore`.
3. Built-in rules: version-control directories (`.git`, `.hg`, `.svn`, `.jj`,
   ...), OS files (`.DS_Store`, `Thumbs.db`) and editor temporary files
   (`*.swp`, `*~`, `.#*`, `#*#`, JetBrains `___jb_tmp___`, ...).

Rules 2 and 3 never hide files you asked for explicitly. If a task watches
`dist/**` and `dist/` is in your `.gitignore`, changes in `dist/` still
trigger that task. Other tasks still ignore them.

## Commands

`run` accepts a string or a list:

```toml
run = "cargo run --release"                  # parsed like a shell would
run = "echo 'hello world'"                   # quotes work
run = "cargo build && ./target/debug/app"    # shell syntax works
run = ["python3", "-c", "print('a && b')"]   # list: passed on verbatim, no parsing
```

Anymon picks the fastest way to run a string command:

- **Simple commands** run directly, without a shell. The command is split
  into words the way a shell would split it (quotes and backslash escapes are
  honored).
- **Commands that use shell features** run through a shell. This covers pipes,
  redirects, `&&`, `;`, variables (`$HOME`, `%PATH%`), globs (`*.txt`),
  `VAR=value cmd`, subshells and comments. The shell is `sh` on Linux and
  macOS and `cmd.exe` on Windows (the same as npm scripts), unless you
  set `shell`.
- **Programs that can't be found**, such as shell builtins (`echo` on
  Windows) or `.cmd` shims such as `npm`, are retried through the shell
  automatically.

Commands get these environment variables in addition to anymon's own
environment and the task's `env`:

| Variable               | Value |
| ---------------------- | ----- |
| `ANYMON_TASK`          | The task name. |
| `ANYMON_CHANGED_PATHS` | The changed files that triggered this run (up to 100), separated like `PATH` (`:` on Unix, `;` on Windows). Empty on the first run. |

A command's standard input is not connected to the terminal, because anymon
uses it for interactive commands. Output passes straight through, colors
included.

## Process handling

- Each run of a task gets its own process group on Unix and its own job object
  on Windows, which covers every process the command starts.
- **Stopping** a task sends `SIGTERM` to the whole group, waits up to
  `kill_timeout`, then sends `SIGKILL` to whatever is left. On Windows
  the job object is terminated immediately.
- If a command exits but leaves background processes behind, anymon cleans
  them up before the next run and when it exits.
- Anymon stops all tasks this way when you quit, press Ctrl-C, or it receives
  `SIGTERM` or `SIGHUP`. Pressing Ctrl-C a second time kills everything
  immediately.

## Reloading

Anymon watches its config file. When you save a change, it validates the new
config, stops all tasks and starts again with the new settings. If the new
config is invalid, anymon reports the error and keeps running with the
previous one. Saving without changes does nothing.
