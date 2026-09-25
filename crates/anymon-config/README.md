# anymon-config

The configuration format of [anymon](https://github.com/builtbyjonas/anymon):
parsing, validation and discovery of `Anymon.toml`.

```rust
use anymon_config::{discover, Config};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = discover(std::env::current_dir()?).ok_or("no Anymon.toml found")?;
    let config = Config::load(&path)?; // parses and validates
    for name in config.task_names() {
        println!("task {name}");
    }
    Ok(())
}
```

Unknown keys are rejected, and errors include the file, line and column. The
format is documented in [docs/configuration.md](../../docs/configuration.md).

## License

MIT or Apache-2.0, at your option.
