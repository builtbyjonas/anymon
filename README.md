# Anymon

Anymon is an ultra‑fast, language‑agnostic file watcher that runs arbitrary
commands when files change. It is intended to be a lightweight developer
productivity tool for running builds, tests, linters, or any script in response
to filesystem events.

## Key points

- Configuration: TOML (Anymon.toml) — see `example_project/Anymon.toml` for an example.
- Cross-platform: Works on Linux, macOS and Windows (uses platform-appropriate
  process spawning when necessary).
- Usage modes: `run` (single run) and `watch` (continuous watching + restart).

## Contents

- `crates/anymon-core/` — Rust core binary and library code.
- `example_project/` — Minimal example and `Anymon.toml` to try the watcher.
- `docs/` — Documentation generated/maintained in this repo.

## Installation

You can install Anymon several ways:

- **From a prebuilt release (recommended)**: Use the convenience installer script which downloads a prebuilt binary for your platform from the GitHub releases for this repository.

  - Unix/macOS (bash/curl):

  ```bash
  curl -sSfL https://anymon.xyz/install.sh | sh
  ```

  - Windows (PowerShell):

  ```powershell
  iex (iwr -UseBasicParsing https://anymon.xyz/install.ps1)
  ```

  The installer script will detect your OS/arch and fetch the matching release asset, unpack it, and place the `anymon` binary into a sensible location.

- **From npm**: A convenience package is available for users who prefer installing via `npm`.

  - Install globally:

  ```bash
  npm i -g anymon
  # or
  yarn global add anymon
  # or
  pnpm add -g anymon
  # or
  bun add -g anymon
  ```

  - Or add per-project as a dev dependency:

  ```bash
  npm i -D anymon
  # or
  yarn add -D anymon
  # or
  pnpm add -D anymon
  # or
  bun add -D anymon
  ```

- **From source**: Build with Rust (requires the Rust toolchain):

```bash
cargo build --release --workspace
```

## Updating

To update Anymon to the latest version, use the built-in update command:

```bash
anymon update
```

This will check for the latest release and update your Anymon binary if a newer version is available.

- On Windows, run this in PowerShell or Command Prompt.
- On Unix/macOS, run it in your terminal.

If you installed Anymon via npm, you **should** update using your package manager:

```bash
npm i -g anymon@latest
# or
yarn global add anymon@latest
# or
pnpm add -g anymon@latest
# or
bun add -g anymon@latest
```

For more details, see `docs/updating.md`.

## Configuration

Configuration is a TOML file (e.g., `Anymon.toml`). The supported schema is
documented in `docs/usage.md` and implemented in `crates/anymon-core/src/config.rs`.

### Generating API docs

To generate Rust API documentation for the workspace:

```bash
cargo doc --workspace --open
```

## Contributing

See `CONTRIBUTING.md` for contribution guidelines and `CODE_OF_CONDUCT.md` for
expected community behavior.

## License

This repository is dual-licensed under the Apache-2.0 and MIT licenses. See `LICENSE-APACHE` and `LICENSE-MIT` for details.
