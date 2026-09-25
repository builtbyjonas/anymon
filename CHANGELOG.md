# Changelog

All notable changes to anymon are documented in this file. The format is
based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and anymon
follows [Semantic Versioning](https://semver.org/).

## [1.0.0] - 2026-09-25

The first stable release. The watcher, process handling, command line,
installers and release pipeline have been rewritten with a focus on speed,
reliability and "it just works".

### Added

- **Ad-hoc mode:** `anymon -e rs -- cargo run` watches and runs a command
  without a config file. It supports `-w/--watch`, `-p/--pattern`, `-e/--exts`
  and `--no-restart`.
- **Config discovery:** a bare `anymon` finds `Anymon.toml`, `anymon.toml` or
  `.anymon.toml` in the current directory or any parent directory.
- **`.gitignore` support:** ignored files never trigger tasks. Nested
  `.gitignore` files and `.git/info/exclude` are honored and reloaded when
  they change. Disable with `gitignore = false` or `--no-gitignore`.
- **Built-in ignores** for VCS directories, OS files and editor swap files.
- **New task options:** `ignore`, `cwd`, `env`, `shell`, `run_on_start`, and
  `run` as an argument list (`run = ["cargo", "run"]`).
- **New global options:** `kill_timeout`, `gitignore`, `shell` and `poll`.
  `watch`/`ignore` also accept a single string.
- **`restart = false` now queues:** changes during a run trigger exactly one
  follow-up run after the current one finishes. Previously such tasks never
  re-ran.
- **Config hot reload:** editing the config restarts the tasks with the new
  settings. An invalid config is reported and the previous one keeps running.
- **`--once`** runs all tasks once and exits with the first failure's exit code,
  for CI and git hooks. `-t/--task` selects tasks.
- **Interactive commands:** `rs [task...]` restarts specific tasks, `status`
  shows running time or the last exit status, and `help` lists the commands.
- **`--poll [MS]`** for file systems without native events (network drives,
  Docker volumes, WSL).
- **New subcommands:** `anymon init` (project-aware config template),
  `anymon check` (validate and show the resolved config),
  `anymon completions <shell>`, and `anymon update --check`.
- **`ANYMON_TASK` and `ANYMON_CHANGED_PATHS`** environment variables for
  commands.
- **Output options:** `-q/--quiet`, `-v/--verbose` and `--color`.
  `NO_COLOR`, `CLICOLOR_FORCE` and `FORCE_COLOR` are respected.
- **Static Linux builds** (`x86_64-unknown-linux-musl`,
  `aarch64-unknown-linux-musl`) as release archives and npm packages. They
  run on every distribution, including Alpine.
- **SHA-256 checksums** (`*.sha256` and `SHA256SUMS`) for every release
  archive. The installers and `anymon update` verify them.

### Changed

- **Process trees:** tasks run in their own process group (job object on
  Windows). Restarts and shutdown stop the whole tree, with `SIGTERM` first
  and `SIGKILL` after `kill_timeout`. Previously only the direct child was
  killed, immediately, which left servers started by `npm`, `sh -c` and
  similar wrappers running.
- **Command parsing:** quotes are honored (`echo 'Hello world'` works), shell
  syntax runs through the shell, and simple commands run directly without
  one. Unknown programs fall back to the shell.
- **Patterns:** glob patterns follow `.gitignore` rules. `*` no longer
  matches across directories, patterns without `/` match at any depth, and
  a directory name matches its contents. Only the directories the patterns
  need are watched.
- **Default shell on Windows** is now `cmd.exe` (like npm scripts), which
  starts much faster than PowerShell and supports `&&`. Set
  `shell = "powershell"` or `"pwsh"` to keep the old behavior.
- **Task working directory:** tasks run in the directory of the config file
  instead of the directory anymon was started from.
- **Debounce precedence:** command-line options now override the config
  (`--debounce` was previously ignored when the config set a value). The
  default debounce is 50 ms.
- **Output:** anymon's own messages go to stderr with colored per-task
  labels, and exit status and duration are shown after every run.
- **Exit codes:** `0` on a normal exit, `2` for invalid usage or config, and
  the command's exit code for `run` and `--once`.
- **`anymon run`** streams output live, forwards stdin and exits with the
  command's exit code. It accepts the command as separate arguments.
- **Config validation** rejects unknown keys, empty commands, duplicate task
  names and invalid patterns, with line and column information.
- **Linux gnu builds** now require only glibc 2.17 (previously 2.39), and
  release binaries are smaller: about 4 MB instead of 7 MB.
- **The npm launcher** finds the binary in every install layout, including
  global installs. It forwards signals and mirrors the exit status.
- **Installers:** `install.sh` is POSIX sh, so `curl | sh` works on
  Debian/Ubuntu. Both installers download the exact release archive, verify
  checksums, support `ANYMON_VERSION`, `ANYMON_INSTALL_DIR` and
  `ANYMON_NO_MODIFY_PATH`, can replace a running binary, and no longer wait
  for Enter. `install.ps1` no longer closes the PowerShell window on errors.
- **CI and release workflows:** moved from the unmaintained `actions-rs` to
  maintained actions. Linux builds are cross-compiled with cargo-zigbuild.
  Formatting, clippy, tests on three operating systems and the minimum Rust
  version (1.88) are all checked. npm packages are published only after
  every build succeeds, platform packages before the main package.
- `anymon debug` is now `anymon check`; `debug` remains as an alias.

### Fixed

- `anymon update` downloads the correct archive for the running build
  (including musl), extracts the binary instead of writing the archive over
  it, compares versions properly, ignores pre-releases, and replaces the
  binary atomically. It also works on Windows while anymon is running. The
  updater in 0.7.x could not find any release asset; reinstall once with the
  install script to get 1.0.0.
- `--once` had no effect.
- `--watch` paths were joined with patterns in a way that made many
  patterns never match.
- Invalid glob patterns were silently dropped.
- `status` reported exited processes as running.
- Shutting down could hang until Enter was pressed, because stdin was read on
  the async runtime.
- Files that are only read (access events) could trigger restarts.
- The example project broke `cargo` commands, because it was inside the
  workspace without being excluded.

### Library crates

The Rust APIs of `anymon-config`, `anymon-runner` and `anymon-shell` were
redesigned (see [docs/api.md](docs/api.md)). `Config::from_toml` is kept for
compatibility. All crates now share the workspace version and metadata.

## [0.7.2] and earlier

See the [GitHub releases](https://github.com/builtbyjonas/anymon/releases).

[1.0.0]: https://github.com/builtbyjonas/anymon/releases/tag/v1.0.0
[0.7.2]: https://github.com/builtbyjonas/anymon/releases/tag/v0.7.2
