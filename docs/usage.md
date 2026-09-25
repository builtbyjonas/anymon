# Usage

```text
anymon [OPTIONS] [-- <COMMAND>...]
anymon <SUBCOMMAND> [OPTIONS]
```

## Watching

Anymon has two ways to watch.

**With a config file.** Run `anymon` (or `anymon watch`) anywhere in your
project. It finds the closest `Anymon.toml`, starts every task and restarts
them when their files change. See [configuration.md](configuration.md).

**Ad hoc.** Put a command after `--` and anymon runs it on every change, no
config needed:

```sh
anymon -- cargo test                       # any change in the current directory
anymon -e rs,toml -- cargo test            # only .rs and .toml files
anymon -w src -w tests -- npm test         # only changes in src/ and tests/
anymon -p 'templates/**/*.html' -- make    # files matching a pattern
anymon -w main.py -- python3 main.py       # a single file
anymon -- "cargo build && ./target/debug/app"   # one string: shell syntax works
```

When the command is a single argument, anymon parses it like a shell would.
Several arguments are passed on verbatim.

### Watch options

| Option | Description |
| ------ | ----------- |
| `-w, --watch <PATH>` | Only watch these files or directories. Repeatable. |
| `-p, --pattern <GLOB>` | Files that trigger the command (ad hoc only). Repeatable. |
| `-e, --exts <EXT>` | File extensions that trigger the command (ad hoc only), e.g. `-e rs,toml`. |
| `-i, --ignore <GLOB>` | Ignore matching files. Repeatable. |
| `-t, --task <NAME>` | Only run these tasks from the config. Repeatable. |
| `-d, --debounce <MS>` | How long changes must settle before running (default 50). |
| `--kill-timeout <MS>` | How long a process may take to exit after `SIGTERM` (default 2000). |
| `--no-gitignore` | Don't skip files excluded by `.gitignore`. |
| `--no-restart` | Let a running command finish, then run it again (ad hoc only). |
| `--poll [MS]` | Poll for changes (default every 500 ms) instead of using native events. Use this for network drives, Docker volumes and WSL. |
| `--once` | Run once and exit (see below). |
| `--shell <SHELL>` | Shell for a command that needs one (ad hoc only). |

Command-line options override the matching settings in the config file.
Patterns follow the rules described in
[configuration.md](configuration.md#patterns).

### Interactive commands

While anymon is watching, type a command and press Enter:

| Command | Effect |
| ------- | ------ |
| `rs [task...]`, `r`, `restart` | Restart all tasks, or only the named ones. |
| `s [task...]`, `status` | Show whether each task is running, and how its last run ended. |
| `q`, `quit`, `exit` | Stop all tasks and exit. |
| `h`, `help` | List the commands. |

Ctrl-C does the same as `q`. Pressing it twice kills all tasks immediately.

### Running once (CI and scripts)

`--once` runs every task a single time, in parallel, and exits. The exit code
is that of the first task (in config order) that failed, or 0. That makes it
handy for CI and git hooks:

```sh
anymon --once             # all tasks
anymon --once -t lint     # one task
anymon --once -- make     # ad hoc
```

## Subcommands

### `anymon run <COMMAND>...`

Runs a command once in the foreground and exits with its exit code. The
command is parsed exactly like a task's `run`, so this is a quick way to check
how anymon will execute a command.

```sh
anymon run "cargo build && echo done"
anymon run npm test -- --watch=false
```

### `anymon init [--force]`

Creates an `Anymon.toml` in the current directory, tailored to the project it
finds there: Rust (`Cargo.toml`), Node.js (`package.json`; detects npm, pnpm,
yarn and bun, and `dev`/`start` scripts), Go (`go.mod`) or Python. Otherwise
it writes a generic template. It won't overwrite an existing config unless
you pass `--force`.

### `anymon check`

Validates the config file and prints the resolved settings: the project root,
the directories that will be watched, and each task's command (and whether it
runs directly or through a shell), patterns, working directory and restart
behavior. It exits with code 2 if the config is invalid. (`anymon debug` is an
alias kept from 0.x.)

### `anymon update [--check]`

Replaces the anymon binary with the latest release. See
[updating.md](updating.md).

### `anymon completions <SHELL>`

Prints a completion script for `bash`, `zsh`, `fish`, `elvish` or
`powershell`:

```sh
anymon completions bash > ~/.local/share/bash-completion/completions/anymon
anymon completions zsh > "${fpath[1]}/_anymon"
anymon completions fish > ~/.config/fish/completions/anymon.fish
```

## Global options

| Option | Description |
| ------ | ----------- |
| `-c, --config <FILE>` | Use this config file instead of searching for `Anymon.toml`. |
| `--color <WHEN>` | `auto` (default), `always` or `never`. `auto` respects `NO_COLOR`, `CLICOLOR_FORCE` and `FORCE_COLOR`. |
| `-q, --quiet` | Only print errors and failed runs. |
| `-v, --verbose` | Also print file events, watched directories and process IDs. |
| `-h, --help` | Print help. |
| `-V, --version` | Print the version. |

Anymon prints its own messages to stderr. Your commands' output passes through
unchanged.

## Exit codes

| Code | Meaning |
| ---- | ------- |
| `0` | Success, or watching ended normally (`q`, Ctrl-C, `SIGTERM`). |
| `1` | A runtime error. |
| `2` | Invalid usage or config (unknown option, config error, no config found). |
| other | With `--once` and `run`: the exit code of the failed command (128 + signal number if it was killed by a signal). |
