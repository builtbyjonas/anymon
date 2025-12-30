# Overview

Anymon is designed to be a small, fast file watcher that can run arbitrary
commands in response to file system changes. The implementation follows the
separation of concerns below:

- Core: `crates/anymon-core` — CLI, configuration parsing, file watching,
  process lifecycle (spawn/kill/restart) and task orchestration.
- Examples: `example_project` — example configuration and usage patterns for
  typical workflows.

## Runtime behavior

1. The binary loads a TOML configuration (if provided) using the structures in
   `src/config.rs`.
2. Each configured task declares watch globs and a command to run.
3. The watcher creates globsets for each task and for the global ignore set.
4. Detected filesystem events are filtered and debounced. Matching tasks are
   signaled to (re)start their configured command.
5. Tasks can be controlled interactively via stdin (status, restart, quit).

## Design goals

- Minimal configuration (TOML), predictable semantics, and simple control
  primitives.
- Platform portability — use native spawning where available while supporting
  shell commands.
- Safe and predictable process lifecycle with a configurable kill timeout.
