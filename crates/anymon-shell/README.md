# anymon-shell

Command parsing and process-tree management, as used by
[anymon](https://github.com/builtbyjonas/anymon).

- `CommandLine::parse` splits a command the way a shell would, and keeps
  commands that use shell syntax (pipes, `&&`, variables, ...) as scripts for
  `sh -c`/`cmd /C`.
- `spawn` starts a command in its own process group (Unix) or job object
  (Windows).
- `Process::terminate` stops the whole process tree: `SIGTERM`, then
  `SIGKILL` after a grace period (on Windows the job is terminated). Dropping
  a `Process` kills what's left, so no orphans survive.

```rust
use anymon_shell::{spawn, CommandLine, SpawnOptions};
use std::time::Duration;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let command = CommandLine::parse("npm run dev")?;
    let mut server = spawn(&command, &SpawnOptions::default())?;
    tokio::time::sleep(Duration::from_secs(10)).await;
    server.terminate(Duration::from_secs(2)).await?; // SIGTERM, then SIGKILL
    Ok(())
}
```

The crate also provides the `anymon-shell` binary, which runs a program
directly, without a shell, and exits with its exit code:
`anymon-shell <program> [args...]`.

## License

MIT or Apache-2.0, at your option.
