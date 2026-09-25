# anymon-runner

The watch loop of [anymon](https://github.com/builtbyjonas/anymon): turns a
config into a validated plan, registers file watches for the directories the
patterns need, filters events (`.gitignore`, built-in and user ignores),
debounces them per task and supervises the task processes.

```rust
use anymon_config::Config;
use anymon_runner::{watch, Plan, PlanOptions};
use std::path::Path;

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    let config = Config::load("Anymon.toml")?;
    let plan = Plan::new(&config, Path::new("."), None, &PlanOptions::default())?;
    watch(plan, None).await // runs until `q`, Ctrl-C or SIGTERM
}
```

How it works is described in [docs/overview.md](../../docs/overview.md).

## License

MIT or Apache-2.0, at your option.
