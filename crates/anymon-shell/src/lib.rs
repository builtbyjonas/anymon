//! Command-line parsing and process management for anymon.
//!
//! - [`CommandLine`] parses a command string and decides whether it can run
//!   directly or needs a shell.
//! - [`spawn`] starts a command in its own process group (Unix) or job object
//!   (Windows). The returned [`Process`] can stop the whole process tree,
//!   first gracefully and then by force.
//! - [`run_command`] is a small blocking helper that runs a program and
//!   captures its output.

mod parse;
mod process;
mod shell;

pub use parse::{CommandLine, ParseError};
pub use process::{describe_exit, exit_code, spawn, Process, SpawnOptions};
pub use shell::Shell;

use std::process::Command;

/// Output of [`run_command`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    /// Exit code of the process (or -1 if it was killed by a signal).
    pub status: i32,
    /// Captured stdout as UTF-8 (lossy).
    pub stdout: String,
    /// Captured stderr as UTF-8 (lossy).
    pub stderr: String,
}

/// Run a program directly (no shell) and capture its stdout, stderr and exit code.
pub fn run_command<S: AsRef<str>>(cmd: S, args: &[S]) -> Result<CommandOutput, std::io::Error> {
    let output = Command::new(cmd.as_ref())
        .args(args.iter().map(AsRef::as_ref))
        .output()?;
    Ok(CommandOutput {
        status: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}
