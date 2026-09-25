# Installation

Anymon is a single binary with no dependencies. Prebuilt binaries are
available for:

| Platform | Targets |
| -------- | ------- |
| Linux, static (musl) | `x86_64-unknown-linux-musl`, `aarch64-unknown-linux-musl` |
| Linux, glibc ≥ 2.17 | `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu` |
| macOS 11+ | `x86_64-apple-darwin`, `aarch64-apple-darwin` |
| Windows 10+ | `x86_64-pc-windows-msvc`, `aarch64-pc-windows-msvc` |

The **musl** builds are fully static. They run on any Linux distribution,
including Alpine, distroless containers and very old systems, so the installer
uses them by default.

## Install script (recommended)

**Linux and macOS**

```sh
curl -fsSL https://anymon.xyz/install.sh | sh
```

**Windows (PowerShell)**

```powershell
irm https://anymon.xyz/install.ps1 | iex
```

The scripts download the right archive for your system from the GitHub
releases, verify its SHA-256 checksum, install `anymon` and add it to your
`PATH`. They never ask questions, so they also work in CI and Dockerfiles.

| Platform | Default install directory |
| -------- | ------------------------- |
| Linux    | `~/.local/share/anymon` (or `$XDG_DATA_HOME/anymon`) |
| macOS    | `~/Library/Application Support/anymon` |
| Windows  | `%LOCALAPPDATA%\anymon` |

Environment variables customize the installation:

| Variable | Effect |
| -------- | ------ |
| `ANYMON_VERSION` | Install a specific version, e.g. `1.0.0`. |
| `ANYMON_INSTALL_DIR` | Install into this directory. |
| `ANYMON_LIBC` | Linux only: `musl` (default) or `gnu`. |
| `ANYMON_NO_MODIFY_PATH` | Set to `1` to leave your shell profile and `PATH` unchanged. |

```sh
curl -fsSL https://anymon.xyz/install.sh | ANYMON_VERSION=1.0.0 ANYMON_INSTALL_DIR=/usr/local/bin sh
```

## npm

Anymon is on npm for projects that already use Node.js. The package contains
no JavaScript runtime code apart from a tiny launcher. npm installs only the
native binary for your platform (glibc or musl builds on Linux are chosen
automatically).

```sh
npm i -g anymon          # or: pnpm add -g anymon / yarn global add anymon / bun add -g anymon
npm i -D anymon          # as a dev dependency, e.g. for package.json scripts
```

Optional dependencies must not be disabled (`--no-optional` /
`--omit=optional`), because the binaries are shipped that way.

## Docker

The static musl binary is a good fit for container images:

```dockerfile
FROM alpine:3
RUN apk add --no-cache curl \
 && curl -fsSL https://anymon.xyz/install.sh | ANYMON_INSTALL_DIR=/usr/local/bin ANYMON_NO_MODIFY_PATH=1 sh
```

In Docker Desktop bind mounts, file events from the host are sometimes not
delivered to the container. If changes are not detected, use `--poll`.

## Manual download

Download the archive for your platform from the
[releases page](https://github.com/builtbyjonas/anymon/releases). Every
archive has a matching `.sha256` file, and `SHA256SUMS` lists all of them.

```sh
target=x86_64-unknown-linux-musl
curl -fsSLO https://github.com/builtbyjonas/anymon/releases/latest/download/anymon-$target.tar.gz
curl -fsSLO https://github.com/builtbyjonas/anymon/releases/latest/download/anymon-$target.tar.gz.sha256
sha256sum -c anymon-$target.tar.gz.sha256
tar -xzf anymon-$target.tar.gz
sudo install anymon-$target/anymon /usr/local/bin/
```

Each archive contains `anymon`, `anymon-shell` (a small helper that runs a
program without a shell, for scripts), the README and the licenses.

## Build from source

You need Rust 1.88 or newer ([rustup.rs](https://rustup.rs)).

```sh
cargo install --git https://github.com/builtbyjonas/anymon anymon-core
```

Or from a clone:

```sh
git clone https://github.com/builtbyjonas/anymon
cd anymon
cargo build --release
./target/release/anymon --version
```

To cross-compile the Linux release binaries yourself, see
[development.md](development.md#release-builds).

## Uninstall

Delete the binary (for example `rm "$(command -v anymon)"`) and remove the
`# anymon` line the installer added to your shell profile. On Windows, delete
`%LOCALAPPDATA%\anymon` and remove it from your user `PATH`. With npm, run
`npm rm -g anymon`.
