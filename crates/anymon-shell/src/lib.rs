//! anymon-shell
//!
//! Small crate exposing utilities to run system commands directly
//! without going through an external interactive shell.

use std::process::Command;

/// Result of running a command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    /// Exit code of the process (or -1 if unavailable).
    pub status: i32,
    /// Captured stdout as UTF-8 (lossy).
    pub stdout: String,
    /// Captured stderr as UTF-8 (lossy).
    pub stderr: String,
}

/// Run a command directly (no shell), capturing stdout/stderr and exit code.
///
/// `cmd` should be the program name or path, and `args` the arguments to pass.
pub fn run_command<S: AsRef<str>>(cmd: S, args: &[S]) -> Result<CommandOutput, std::io::Error> {
    let mut command = Command::new(cmd.as_ref());
    for a in args {
        command.arg(a.as_ref());
    }

    let output = command.output()?;

    let status = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    Ok(CommandOutput {
        status,
        stdout,
        stderr,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_echo() {
        let args: &[&str] = if cfg!(windows) {
            &["/C", "echo", "hello"]
        } else {
            &["-c", "echo hello"]
        };
        // On Unix use /bin/sh to test, but crate users should prefer direct programs.
        if cfg!(windows) {
            let res = run_command("cmd", args).expect("run cmd");
            assert!(res.stdout.to_lowercase().contains("hello"));
        } else {
            let res = run_command("sh", args).expect("run sh");
            assert!(res.stdout.contains("hello"));
        }
    }
}
