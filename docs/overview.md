# How anymon works

This page explains anymon's architecture and the reasoning behind its
behavior. You don't need it to use anymon, but it helps when debugging or
contributing.

## Crates

| Crate | Role |
| ----- | ---- |
| [`anymon-core`](../crates/anymon-core) | The `anymon` binary: command-line interface, `init`, `check` and the self-updater. |
| [`anymon-runner`](../crates/anymon-runner) | Watching and supervision: turns a config into a plan, registers file watches, filters and debounces events, runs tasks. |
| [`anymon-config`](../crates/anymon-config) | The `Anymon.toml` schema: parsing, validation, discovery. |
| [`anymon-shell`](../crates/anymon-shell) | Command parsing and process-tree management. Also provides the small `anymon-shell` binary. |

## From config to running tasks

1. **Discovery.** `anymon` finds the nearest `Anymon.toml`. Its directory
   becomes the project root.
2. **Planning** (`anymon_runner::Plan`). The config is validated and
   resolved: commands are parsed, glob patterns compiled, working directories
   checked and command-line overrides applied. Mistakes are reported here,
   before anything runs.
3. **Watching.** From the patterns, anymon computes the smallest set of
   directories to watch. `src/**` only needs `src/`, while `*.rs` needs the
   whole project. How it registers them depends on the platform:
   - **macOS (FSEvents) and Windows (ReadDirectoryChangesW)** watch recursively
     natively, so one watch per directory tree is enough.
   - **Linux (inotify), BSDs and polling** need one watch per directory.
     Anymon walks the tree itself and skips ignored directories such as
     `node_modules` and `target`. This keeps it well under the inotify
     watch limit and keeps startup fast. New directories are added as
     they appear, including files created in them before the watch was in
     place.
4. **Filtering.** Each file event passes the ignore rules (user patterns,
   `.gitignore`, built-in rules) and is matched against every task's patterns.
   Access events (a file being read) are dropped, so a task that reads its
   own sources never triggers itself.
5. **Debouncing.** Each task collects matching changes until the debounce
   window has passed without new ones, then triggers a single run. A
   `git checkout` touching 500 files therefore causes one run, not 500.
6. **Supervision.** Every task is an independent actor that owns its
   process. On a trigger it either restarts the process (`restart = true`)
   or queues exactly one follow-up run (`restart = false`).

## Running commands

`anymon_shell::CommandLine::parse` scans the command once:

- Without shell syntax, it's split into words, honoring POSIX quoting, and
  executed directly. That's faster and starts one process fewer than a shell.
- With shell syntax (pipes, redirects, `&&`, variables, globs, ...), the
  unmodified string goes to the shell: `sh -c` on Unix, and
  `cmd /D /S /C` on Windows, like npm.
- If the program of a direct command can't be found, the command is retried
  through the shell. That covers shell builtins, `.cmd` shims on Windows, and
  gives the familiar "command not found" message.

## Process trees

A restart that kills only the direct child isn't enough: `npm run dev` starts
a shell, which starts node, which may start more processes. Anymon therefore
isolates every run:

- **Unix:** the command runs in a new process group. Stopping sends
  `SIGTERM` to the whole group, waits up to `kill_timeout` for it to
  exit, and then sends `SIGKILL` to whatever remains. Processes a command
  left running in the background are found the same way and cleaned up.
- **Windows:** the command runs in a job object created with "kill on close".
  Stopping terminates the job, which ends every process in it. If anymon
  crashes, Windows closes the job and still cleans up.

Because tasks run in their own process group on Unix, Ctrl-C in the terminal
reaches anymon only. Anymon then stops each task gracefully. A second Ctrl-C
kills everything immediately.

## Robustness details

- **Stale events.** macOS can report changes made shortly before a watch
  started. During the first second, anymon ignores events for files last
  modified before watching began.
- **Config reloads** only happen if the file's content changed, and an
  invalid config never replaces a working one.
- **Stdin** is read on a dedicated thread, because a blocking read can't be
  cancelled and would otherwise delay exit until Enter is pressed. When anymon
  runs in the background (`anymon &`), it doesn't read the terminal at all,
  since that would suspend it.
- **Overflow.** If the OS drops events (inotify queue overflow, FSEvents
  rescans), every task is treated as changed rather than silently missing an
  update.
- **Watch limits.** If the OS limit on file watches is reached, anymon keeps
  running and tells you how to raise the limit.

## Performance

- A current-thread async runtime: no thread pool to start or synchronize.
- Globs are compiled once into a `GlobSet` per directory base and matched
  against relative paths.
- `.gitignore` files are parsed once and reloaded only when they change.
- The release binary is built with LTO and stripped, about 4 MB, and uses a
  few MB of memory while idle.
