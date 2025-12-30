# Development

This document explains how to build, test, and contribute to the Anymon
project.

## Building

### Build the workspace

```bash
cargo build
```

### Or build in release mode

```bash
cargo build --release
```

## Running tests

### Run the workspace tests

```bash
cargo test --workspace
```

## Formatting and linting

Format the code with `rustfmt` (provided by `rustup component add rustfmt`):

```bash
cargo fmt --all
```

## Run clippy for lint checks:

```bash
rustup component add clippy
cargo clippy --all -- -D warnings
```

## Debugging and logging

Use `cargo run` with the `debug` command to print loaded configuration and
additional runtime info:

```bash
cd crates/anymon-core
cargo run -- debug --config ../example_project/Anymon.toml
```

## Docs generation

Generate API docs and open them in the browser:

```bash
cargo doc --workspace --open
```

## Testing changes locally

Use `example_project` to verify typical workflows quickly:

```bash
cd example_project
cargo run -- --config Anymon.toml watch
```

## Submitting changes

1. Fork and clone the repository.
2. Create a feature branch: `git checkout -b feat/my-change`.
3. Implement changes and add tests where appropriate.
4. Run formatting and tests.
5. Open a pull request with a clear description and reproduction steps.
