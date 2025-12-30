# Installation

## Requirements

- Rust toolchain (stable) with `cargo` and `rustc`. Install via `rustup`:

```
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
# or on Windows use the rustup installer from https://rustup.rs/
```

## Platform notes

- Windows: PowerShell is used as a fallback to spawn shell commands when the
  configured command cannot be spawned directly.
- Linux/macOS: `sh -c` is used as the fallback shell.

## Build from source

Build the entire workspace:

```bash
cargo build --release --workspace
```

Run the `anymon` binary from the crate:

```bash
cd crates/anymon-core
cargo run -- --help
```

Try the example project

```bash
cd example_project
cargo run -- --config Anymon.toml watch
```

## Installing a prebuilt binary

This repository publishes prebuilt releases on GitHub. The recommended way to
install a prebuilt binary is to use the installer script provided as a release
asset which automatically downloads the correct binary for your OS/architecture
from the GitHub releases.

- Unix/macOS (bash/curl):

```bash
curl -sSfL https://anymon.xyz/install.sh | sh
```

- Windows (PowerShell):

```powershell
iex (iwr -UseBasicParsing https://anymon.xyz/install.ps1)
```

The installer script detects the platform and architecture, downloads the
matching release asset (zip/tarball), unpacks it, and installs the `anymon`
binary into a sensible location.

## Installing via npm

Anymon is also available via npm for users who prefer that ecosystem.

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

- Install for a project as a dev dependency:

```bash
npm i -D anymon
# or
yarn add -D anymon
# or
pnpm add -D anymon
# or
bun add -D anymon
```
