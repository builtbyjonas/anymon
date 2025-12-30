# API Reference (guide)

The authoritative API is generated from the Rust crate using `cargo doc`.

## Generate docs locally:

```bash
cargo doc --workspace --open
```

## Key public types

- `crates::anymon_core::config::Config` — top-level config with
  `global` and `task` sections.
- `crates::anymon_core::config::TaskConfig` — configuration for an individual
  task: `name`, `watch`, `run`, and `restart`.
- `crates::anymon_core::config::GlobalConfig` — `debounce` and `ignore`.

If you need a programmatic integration with the core library, open the
generated documentation and inspect the public functions in `anymon-core`.
