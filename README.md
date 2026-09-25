# Anymon

Anymon is an ultra-fast, language-agnostic file watcher that runs anything on
change. Point it at your project and it rebuilds, re-tests or restarts your
server whenever you save a file. It works with any language and runs on Linux, macOS and Windows.

```console
$ anymon -e rs -- cargo run
[anymon] v1.0.0 watching ~/code/api (1 task)
[cargo] $ cargo run
   Compiling api v0.1.0
    Running `target/debug/api`
listening on :8080
[cargo] src/routes.rs changed, restarting
[cargo] $ cargo run
```

## Highlights

- **Just works.** Finds your `Anymon.toml` automatically, respects
  `.gitignore`, and skips `.git`, editor swap files and other noise.
- **Fast.** It's a single native binary of about 4 MB with nearly instant
  startup and no idle CPU use. Commands run directly without a shell when
  they don't need one.
- **Reliable restarts.** Each task runs in its own process group (a job object
  on Windows). Restarting stops the whole process tree, gracefully first
  (`SIGTERM`, then `SIGKILL` after a timeout), so no orphaned servers keep
  your ports busy.
- **Any command.** Quotes, pipes, `&&` and environment variables work as they
  would in your shell.
- **Several tasks at once.** Each task has its own watch patterns, working
  directory and environment, and restart or queue behavior.
- **Interactive.** Type `rs` to restart, `s` for status and `q` to quit.
  Anymon also reloads itself when you edit the config.
- **Runs anywhere.** Prebuilt binaries are available for Linux (glibc and
  static musl), macOS and Windows, on both x86_64 and ARM64.

## Install

```sh
# Linux and macOS
curl -fsSL https://anymon.xyz/install.sh | sh

# Windows (PowerShell)
irm https://anymon.xyz/install.ps1 | iex

# npm, pnpm, yarn or bun (global or as a dev dependency)
npm i -g anymon
```

On Linux the installer uses the static musl build, which runs on any
distribution, including Alpine. See [docs/installation.md](docs/installation.md)
for all options, including manual downloads, pinned versions and building from
source.

## Quick start

Run a command whenever a file changes, no config needed:

```sh
anymon -e rs -- cargo test              # Rust: re-run tests when a .rs file changes
anymon -e py -- python3 main.py         # Python
anymon -w src -- npm run build          # anything in src/
anymon -- "make && ./build/app"         # shell syntax works
```

For anything longer, create a config file:

```sh
anymon init   # writes an Anymon.toml tailored to your project
anymon        # starts watching
```

```toml
# Anymon.toml
[global]
ignore = ["dist/**"]

[[task]]
name = "server"
watch = ["src/**", "Cargo.toml"]
run = "cargo run"

[[task]]
name = "css"
watch = "styles/*.scss"
run = "sass styles/main.scss dist/main.css"
restart = false     # let a running build finish, then run again
```

While anymon runs, type these commands (followed by Enter):

| Command            | Effect                                    |
| ------------------ | ----------------------------------------- |
| `rs [task...]`     | Restart all tasks, or only the named ones |
| `s`, `status`      | Show what every task is doing             |
| `q`, `quit`        | Stop all tasks and exit                   |

## Documentation

- [Installation](docs/installation.md): installers, npm, manual download, static Linux builds
- [Usage](docs/usage.md): the command line, ad-hoc commands, CI usage
- [Configuration](docs/configuration.md): every option in `Anymon.toml`, and how patterns work
- [Updating](docs/updating.md): `anymon update` and package managers
- [How it works](docs/overview.md): architecture and design decisions
- [Development](docs/development.md): building, testing and releasing
- [Changelog](CHANGELOG.md)

## Contributing

Contributions are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md) and the
[Code of Conduct](CODE_OF_CONDUCT.md).

## License

Anymon is dual-licensed under the [MIT License](MIT-LICENSE.md) and the
[Apache License 2.0](APACHE-LICENSE.md); choose whichever you prefer.
