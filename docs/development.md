# Development

## Setup

You need Rust 1.88 or newer. The workspace pins no toolchain; the latest
stable release is what CI uses.

```sh
git clone https://github.com/builtbyjonas/anymon
cd anymon
cargo build
cargo run -- --help
```

## Layout

```text
crates/
  anymon-core/     the `anymon` binary (CLI, init, check, update)
  anymon-runner/   planning, watching, event filtering, task supervision
  anymon-config/   Anymon.toml schema, validation and discovery
  anymon-shell/    command parsing, process groups/job objects, `anymon-shell` binary
docs/              user documentation
example_project/   a tiny project to try anymon on
installers/        install.sh and install.ps1 (served at anymon.xyz)
npm/               the npm launcher package and one package per platform
www/               the anymon.xyz server (redirects and installer delivery)
.github/           CI and release workflows and their scripts
```

[overview.md](overview.md) describes how the pieces fit together.

## Checks

Run these before opening a pull request. CI runs the same checks on Linux,
macOS and Windows.

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

The tests include:

- unit tests next to the code (parsing, patterns, ignore rules, planning,
  version handling, ...),
- process tests in `crates/anymon-shell/tests`, which check graceful
  termination, escalation to `SIGKILL` and that no grandchild process survives,
- end-to-end tests in `crates/anymon-core/tests/cli.rs`, which run the real
  binary: they change files and check the restarts, ignore rules, config
  reloads, queueing, polling, signals and exit codes.

## Trying changes

```sh
cd example_project
cargo run --manifest-path ../Cargo.toml -- -v
```

Edit `example_project/src/main.rs` and watch the task restart. `-v` shows
every file event and the watched directories. `anymon check` prints the
resolved configuration.

## Release builds

The release binaries are built by `.github/scripts/build.sh`:

- macOS and Windows targets are built with plain `cargo build`.
- Linux targets are cross-compiled with
  [cargo-zigbuild](https://github.com/rust-cross/cargo-zigbuild) and
  [Zig](https://ziglang.org) 0.15. The musl builds are fully static, and the
  glibc builds link against glibc 2.17 so they run on old distributions.

To reproduce a Linux build locally (from any OS):

```sh
cargo install cargo-zigbuild      # and install Zig 0.15, e.g. from ziglang.org
rustup target add x86_64-unknown-linux-musl
.github/scripts/build.sh x86_64-unknown-linux-musl
.github/scripts/package.sh x86_64-unknown-linux-musl dist
```

`package.sh` creates `dist/anymon-<target>.tar.gz` (`.zip` on Windows) and
a `.sha256` file next to it.

## Releasing

1. Update `version` in the `[workspace.package]` table of the root
   `Cargo.toml`, and in `example_project/Cargo.toml`.
2. Run `node .github/scripts/update-npm-versions.js` to sync the npm packages.
3. Add the release notes to `CHANGELOG.md`.
4. Commit, then create a GitHub release with the tag `v<version>` (for example
   `v1.0.0`). Mark it as a pre-release for versions such as `1.1.0-rc.1`.

Creating the release starts the [release workflow](../.github/workflows/release.yml), which:

1. checks that the tag matches the version in `Cargo.toml`,
2. builds and smoke-tests all eight targets,
3. uploads the archives, the `.sha256` files and a combined `SHA256SUMS`
   to the release,
4. publishes the npm packages: the platform packages first, then
   `anymon`. Pre-releases go to the `next` tag, releases to `latest`.
   Versions that already exist are skipped, so a failed run can be re-run, or
   started manually for an existing tag ("Run workflow").

Publishing needs an `NPM_TOKEN` secret that can publish the `anymon` package
and the `@anymon/*` scope. The npm job runs in the `stable` environment for
releases and `next` for pre-releases.

## Website

`www/` is a small Express app deployed on Vercel. It redirects
`anymon.xyz` to the repository and serves `install.sh` and `install.ps1`
from the `main` branch. That makes installer changes live as soon as
they are merged.
