# anymon-core

The Rust core for the Anymon project — an ultra-fast, language-agnostic file
watcher that runs arbitrary commands when files change. This crate provides the
command-line interface, configuration parsing, file-watching logic and task
management used by the `anymon` binary.

## Features

- TOML-based configuration (`Anymon.toml`) with per-task watch patterns.
- Cross-platform spawning: uses native process spawning and falls back to
	`sh -c` / PowerShell when needed.
- Debounce window and ignore patterns.
- Control commands over stdin: `rs`/`restart`, `status`, `quit`.

## Crate layout

- `src/config.rs` — configuration structures and TOML parsing helpers.
- `src/main.rs` — CLI and main runtime (watch loop, task loops, process
	management).

## Usage (crate)

Build and run from the crate directory:

```
cd crates/anymon-core
cargo run --release -- --help
```

When running the binary you will typically either run a single command with
`anymon run "cargo test"` or start the watcher with `anymon watch --config Anymon.toml`.

For full project-level documentation and examples, see the repository's root
README and the `docs/` directory.
