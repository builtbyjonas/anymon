# Contributing

Thanks for your interest in contributing to anymon! Bug reports, ideas and
pull requests are welcome. Small, focused pull requests are easiest to review.

## Reporting bugs

Please include:

- your OS and how you installed anymon (`anymon --version`),
- your `Anymon.toml` (or the command line you used),
- what you expected and what happened; the output of `anymon -v` often
  shows why an event was or wasn't picked up.

## Making changes

1. Fork the repository and create a branch.
2. Make your change and add tests for new behavior or fixed bugs.
3. Run the checks:

   ```sh
   cargo fmt --all
   cargo clippy --workspace --all-targets -- -D warnings
   cargo test --workspace
   ```

4. Update the documentation in `docs/` and the `README.md` if behavior
   changes, and add an entry to the "Unreleased" section of `CHANGELOG.md`.
5. Open a pull request that explains the motivation and links related issues.

[docs/development.md](docs/development.md) explains the project layout, the
tests and how releases are built.

## Code of Conduct

Please follow the [Code of Conduct](CODE_OF_CONDUCT.md) in issues and pull
requests.
