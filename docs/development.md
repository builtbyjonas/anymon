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
4. Commit and push, then publish a GitHub release with the tag `v<version>`
   (for example `v1.0.0`). Mark it as a pre-release for versions such as
   `1.1.0-rc.1`. Draft releases don't start the workflow until they are
   published.

Publishing the release starts the [release workflow](../.github/workflows/release.yml), which:

1. checks that the tag matches the version in `Cargo.toml`,
2. builds and smoke-tests all eight targets,
3. uploads the archives, the `.sha256` files and a combined `SHA256SUMS`
   to the release,
4. publishes the npm packages with provenance: the platform packages first,
   then `anymon`. Pre-releases go to the `next` tag, releases to `latest`.
   Versions that already exist are skipped, so a failed run can be re-run, or
   started manually for an existing tag ("Run workflow").

### Environments

The two publishing jobs (release assets and npm) run in a GitHub
environment: `stable` for releases and `next` for pre-releases. Protection
rules on these environments therefore gate the actual publishing, for example
required reviewers or a rule that only allows `v*` tags. A reviewer approves
once per release, because both jobs start at the same time after the builds.
The build jobs don't use an environment because they have no side effects.
A manual run ("Run workflow") can choose any environment.

All CI jobs run in the `ci` environment.

### npm trusted publishing

The npm packages are published with
[trusted publishing](https://docs.npmjs.com/trusted-publishers): the npm job
exchanges its GitHub OIDC token (`id-token: write`) for a short-lived publish
credential, so no npm token is stored in the repository. npm also attaches
provenance that links every package to the workflow run that built it. The
job always uses the latest npm on the latest Node.js LTS, because trusted
publishing needs npm 11.5.1 or later.

Every package needs a trusted publisher configured on npmjs.com, and npm only
allows that for packages that already exist. Run the setup script once when
setting up the repository, or after adding a new platform package. You need
maintainer rights, account 2FA and npm 11.15 or later.

```sh
npm install -g npm@latest
npm login
.github/scripts/setup-npm-trust.sh
```

The script publishes a placeholder `0.0.0` for packages that don't exist
yet; the next release replaces it as `latest`. It then registers
`builtbyjonas/anymon` → `release.yml` as the trusted publisher of every
package. Pass `--env stable` to accept publishes from that environment only;
pre-releases from `next` would then be rejected. You can also configure
packages by hand: package → Settings → Trusted publisher → GitHub Actions
(organization `builtbyjonas`, repository `anymon`, workflow `release.yml`).

If a package is missing on npm, the release workflow stops before
publishing anything and says which one. After trusted publishing works, you
can delete any old npm automation tokens and the `NPM_TOKEN` secret.

## Website

`www/` is a small Express app deployed on Vercel. It redirects
`anymon.xyz` to the repository and serves `install.sh` and `install.ps1`
from the `main` branch. That makes installer changes live as soon as
they are merged.
