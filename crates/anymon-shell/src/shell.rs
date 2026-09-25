//! Shells used to run scripts that need shell syntax.

use std::fmt;
use std::path::Path;

/// A shell that can execute a script.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Shell {
    /// POSIX `sh -c` (default on Unix).
    Sh,
    /// `cmd.exe /D /S /C` (default on Windows, same as npm scripts).
    Cmd,
    /// Windows PowerShell or PowerShell 7 (`-NoProfile -NonInteractive -Command`).
    PowerShell(String),
    /// Any other program, invoked as `<program> -c <script>`.
    Other(String),
}

impl Default for Shell {
    fn default() -> Self {
        Shell::platform_default()
    }
}

impl Shell {
    /// The default shell of the current platform.
    pub fn platform_default() -> Self {
        if cfg!(windows) {
            Shell::Cmd
        } else {
            Shell::Sh
        }
    }

    /// Resolve a shell from a user-supplied name or path such as `bash`,
    /// `pwsh`, `cmd` or `/usr/local/bin/fish`.
    pub fn from_name(name: &str) -> Self {
        let stem = Path::new(name)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(name)
            .to_ascii_lowercase();
        match stem.as_str() {
            "sh" if !cfg!(windows) && !name.contains(['/', '\\']) => Shell::Sh,
            "cmd" => Shell::Cmd,
            "powershell" | "pwsh" => Shell::PowerShell(name.to_string()),
            _ => Shell::Other(name.to_string()),
        }
    }

    /// Program name used for display.
    pub fn name(&self) -> &str {
        match self {
            Shell::Sh => "sh",
            Shell::Cmd => "cmd",
            Shell::PowerShell(program) | Shell::Other(program) => program,
        }
    }

    /// Build a command that runs `script` in this shell.
    pub fn command(&self, script: &str) -> tokio::process::Command {
        match self {
            Shell::Sh => {
                let program = if Path::new("/bin/sh").exists() {
                    "/bin/sh"
                } else {
                    "sh"
                };
                let mut cmd = tokio::process::Command::new(program);
                cmd.arg("-c").arg(script);
                cmd
            }
            Shell::Cmd => cmd_command(script),
            Shell::PowerShell(program) => {
                let mut cmd = tokio::process::Command::new(program);
                cmd.args(["-NoProfile", "-NonInteractive", "-Command", script]);
                cmd
            }
            Shell::Other(program) => {
                let mut cmd = tokio::process::Command::new(program);
                cmd.arg("-c").arg(script);
                cmd
            }
        }
    }

    /// Quote an argument vector so this shell reproduces it verbatim.
    pub fn quote_argv(&self, argv: &[String]) -> String {
        argv.iter()
            .map(|arg| self.quote(arg))
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn quote(&self, arg: &str) -> String {
        let safe = !arg.is_empty()
            && arg.chars().all(|c| {
                c.is_ascii_alphanumeric()
                    || matches!(c, '-' | '_' | '.' | '/' | ':' | '=' | ',' | '+' | '@')
                    || (c == '\\' && matches!(self, Shell::Cmd | Shell::PowerShell(_)))
            });
        if safe {
            return arg.to_string();
        }
        match self {
            Shell::Cmd => format!("\"{}\"", arg.replace('"', "\"\"")),
            Shell::PowerShell(_) => format!("'{}'", arg.replace('\'', "''")),
            Shell::Sh | Shell::Other(_) => format!("'{}'", arg.replace('\'', r"'\''")),
        }
    }
}

impl fmt::Display for Shell {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

#[cfg(windows)]
fn cmd_command(script: &str) -> tokio::process::Command {
    let program = std::env::var_os("ComSpec").unwrap_or_else(|| "cmd.exe".into());
    let mut cmd = tokio::process::Command::new(program);
    // `/S` makes cmd strip exactly the outer quotes and run the rest verbatim,
    // which is how npm runs scripts. The script must not be re-quoted by Rust.
    cmd.args(["/D", "/S", "/C"]);
    cmd.raw_arg(format!("\"{script}\""));
    cmd
}

#[cfg(not(windows))]
fn cmd_command(script: &str) -> tokio::process::Command {
    let mut cmd = tokio::process::Command::new("cmd");
    cmd.args(["/C", script]);
    cmd
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_names() {
        assert_eq!(Shell::from_name("cmd"), Shell::Cmd);
        assert_eq!(Shell::from_name("cmd.exe"), Shell::Cmd);
        assert_eq!(
            Shell::from_name("pwsh"),
            Shell::PowerShell("pwsh".to_string())
        );
        assert_eq!(
            Shell::from_name("powershell.exe"),
            Shell::PowerShell("powershell.exe".to_string())
        );
        assert_eq!(Shell::from_name("bash"), Shell::Other("bash".to_string()));
        assert_eq!(
            Shell::from_name("/usr/bin/fish"),
            Shell::Other("/usr/bin/fish".to_string())
        );
        if cfg!(windows) {
            assert_eq!(Shell::from_name("sh"), Shell::Other("sh".to_string()));
        } else {
            assert_eq!(Shell::from_name("sh"), Shell::Sh);
        }
    }

    #[test]
    fn default_matches_platform() {
        if cfg!(windows) {
            assert_eq!(Shell::default(), Shell::Cmd);
        } else {
            assert_eq!(Shell::default(), Shell::Sh);
        }
    }

    #[test]
    fn quotes_arguments_per_shell() {
        let argv = vec![
            "echo".to_string(),
            "it's".to_string(),
            "a b".to_string(),
            "".to_string(),
        ];
        assert_eq!(Shell::Sh.quote_argv(&argv), r"echo 'it'\''s' 'a b' ''");
        assert_eq!(
            Shell::PowerShell("pwsh".into()).quote_argv(&argv),
            "echo 'it''s' 'a b' ''"
        );
        assert_eq!(Shell::Cmd.quote_argv(&argv), "echo \"it's\" \"a b\" \"\"");
    }
}
