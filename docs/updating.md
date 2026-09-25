# Updating

## Built-in updater

```sh
anymon update           # install the latest release
anymon update --check   # only check whether a newer version exists
```

`anymon update`:

1. Looks up the latest release on GitHub.
2. Downloads the archive for the platform the running binary was built for,
   so a static musl build stays a musl build.
3. Verifies the archive's SHA-256 checksum.
4. Replaces the binary in place, atomically. If something fails, the old
   binary stays untouched. On Windows the running `anymon.exe` is moved
   aside and removed on the next start. `anymon-shell` is updated as well
   if it sits next to `anymon`.

If the install directory is not writable (for example `/usr/local/bin`),
run the update with elevated rights, or reinstall with the install script.

If GitHub rate-limits your requests, set `GITHUB_TOKEN` to a personal access
token.

> Upgrading from 0.x: the updater in 0.7.x and earlier could not find the
> release archives. Reinstall once with the install script (see
> [installation.md](installation.md)); `anymon update` works from 1.0.0 on.

## npm

If anymon was installed with npm, `anymon update` detects this and tells you
to update with your package manager instead:

```sh
npm i -g anymon@latest
# or
pnpm add -g anymon@latest
yarn global add anymon@latest
bun add -g anymon@latest
```

For a project dependency, bump the version in `package.json` (e.g.
`npm i -D anymon@latest`).

## Install script

Running the install script again installs the latest version over the
existing one:

```sh
curl -fsSL https://anymon.xyz/install.sh | sh
```
