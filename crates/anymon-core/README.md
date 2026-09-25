# anymon-core

The `anymon` command-line tool: an ultra-fast, language-agnostic file watcher
that runs anything on change.

```sh
cargo install --git https://github.com/builtbyjonas/anymon anymon-core
anymon -e rs -- cargo test
```

This crate contains the command-line interface, `anymon init`,
`anymon check` and the self-updater. The watching and process handling live
in [`anymon-runner`](../anymon-runner) and [`anymon-shell`](../anymon-shell),
the config format in [`anymon-config`](../anymon-config).

See the [main README](../../README.md) and the [documentation](../../docs)
for usage.

## License

MIT or Apache-2.0, at your option.
