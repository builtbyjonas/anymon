# Anymon

Anymon is an ultra‑fast, language‑agnostic file watcher that runs arbitrary
commands when files change. It is intended to be a lightweight developer
productivity tool for running builds, tests, linters, or any script in response
to filesystem events.

## Installation

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

## Updating

To update Anymon to the latest version:

```bash
npm i -g anymon@latest
# or
yarn global add anymon@latest
# or
pnpm add -g anymon@latest
# or
bun add -g anymon@latest
```

## Configuration

Configuration is a TOML file (e.g., `Anymon.toml`). The supported schema is
documented in `docs/usage.md` and implemented in `crates/anymon-core/src/config.rs`.

## License

This repository is dual-licensed under the MIT License and the Apache License (Version 2.0).
