# anymon

An ultra-fast, language-agnostic file watcher that runs anything on change,
like `nodemon` but for every language, with reliable restarts of the whole
process tree.

This package installs the native anymon binary for your platform (Linux
glibc or musl, macOS, Windows; x64 and arm64).

## Install

```sh
npm i -D anymon     # per project
npm i -g anymon     # globally (or pnpm add -g / yarn global add / bun add -g)
```

## Use

```sh
npx anymon -e ts,tsx -- node --import tsx src/server.ts
npx anymon init     # create an Anymon.toml
npx anymon          # run the tasks in Anymon.toml
```

In `package.json`:

```json
{
  "scripts": {
    "dev": "anymon -w src -- node src/index.js"
  }
}
```

While anymon runs, type `rs` + Enter to restart, `s` for status and `q` to
quit.

## Documentation

Everything else, including configuration, patterns and the command-line
options, is documented at
[github.com/builtbyjonas/anymon](https://github.com/builtbyjonas/anymon#readme).

## License

MIT or Apache-2.0, at your option.
