# Usage

This document describes configuration and runtime usage for `anymon`.

CLI

The binary exposes these commands and flags (from `crates/anymon-core/src/main.rs`):

- `anymon run "COMMAND"` — Run a single command once (no shell interpretation
  beyond argument splitting unless fallback is used).
- `anymon watch` — Start watch mode using a TOML config supplied with
  `--config Anymon.toml`.
- `anymon debug` — Print debug information (loaded config, etc.).

Global flags

- `--watch <PATH>` — Override config watch roots.
- `--config <FILE>` — Path to the TOML configuration file.
- `--debounce <MS>` — Debounce window in milliseconds (default 30).
- `--kill-timeout <MS>` — Kill timeout for processes in ms (default 2000).
- `--once` — Run once and exit (global).

Interactive control

When `watch` is running you can type commands on stdin (followed by Enter):

- `rs` or `restart` — restart tasks.
- `status` — print running/stopped status.
- `quit`, `q`, `exit` — request shutdown.

Configuration (Anymon.toml)

The TOML configuration structure is implemented in `src/config.rs`. Example
schema:

```toml
[global]
debounce = 50
ignore = ["target/**", "**/.git/**"]

[[task]]
name = "build"
watch = ["src/**", "Cargo.toml"]
run = "cargo build"
restart = true

[[task]]
name = "tests"
watch = ["tests/**"]
run = "cargo test"
restart = false
```

Field descriptions

- `[global]` section
  - `debounce` (ms): optional debounce window applied to events.
  - `ignore` (array): glob patterns to ignore (relative or absolute).
- `[[task]]` table (can appear multiple times)
  - `name` (string): human-friendly task name.
  - `watch` (array of strings): glob patterns to match file events.
  - `run` (string): command to execute when changes match.
  - `restart` (bool, optional): whether to kill & restart on subsequent
    events (defaults to true).

Globs and roots

Patterns in `watch` and `ignore` are interpreted as globs. The watcher resolves
patterns against the configured root(s):

- `--watch` CLI flag (one or more paths) overrides config roots.
- If `--watch` is omitted and `--config` is specified, the config file's
  directory is used as the root.
- Otherwise the current working directory is used.

Examples

Run a single command:

```bash
anymon run "cargo test"
```

Start watching with a config:

```bash
anymon watch --config example_project/Anymon.toml
```
