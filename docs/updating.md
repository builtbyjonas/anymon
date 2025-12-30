# Updating Anymon

To update Anymon to the latest version, use the built-in update command. This works for prebuilt installations.

## Using the built-in update command

Simply run:

```bash
anymon update
```

This will automatically check for the latest release and update your Anymon binary if a newer version is available.

- On Windows, you can run this in PowerShell or Command Prompt.
- On Unix/macOS, run it in your terminal.

## Notes
- If you installed Anymon via npm, you **should** update using your package manager:
  - `npm i -g anymon@latest`
  - `yarn global add anymon@latest`
  - `pnpm add -g anymon@latest`
  - `bun add -g anymon@latest`

For more details, see the [installation guide](installation.md).
