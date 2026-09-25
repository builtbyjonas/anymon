//! Parsing of command strings into direct invocations or shell scripts.

use std::fmt;

/// A command to execute, either directly or through a shell.
///
/// Simple commands such as `cargo build --release` or `echo 'hello world'` are
/// split into a program and its arguments and executed directly, which is
/// faster and avoids an extra shell process. Anything that relies on shell
/// features (pipes, redirects, `&&`, variables, globs, ...) is handed to the
/// platform shell unchanged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandLine {
    /// Execute `program` with `args`, without a shell.
    Direct { program: String, args: Vec<String> },
    /// Hand the script to a shell (`sh -c`, `cmd /C`, ...).
    Shell(String),
}

/// Error returned when a command string cannot be parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    /// The command is empty or only whitespace.
    Empty,
    /// A quote was opened but never closed.
    UnterminatedQuote(char),
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::Empty => f.write_str("command is empty"),
            ParseError::UnterminatedQuote(q) => write!(f, "unterminated {q} quote"),
        }
    }
}

impl std::error::Error for ParseError {}

impl CommandLine {
    /// Parse a command string.
    ///
    /// Quoting follows POSIX shell rules: single quotes are literal, double
    /// quotes allow `\"` and `\\` escapes, and (on Unix) a backslash escapes the
    /// next character. On Windows backslashes are literal so paths such as
    /// `C:\tools\app.exe` work unquoted.
    pub fn parse(line: &str) -> Result<Self, ParseError> {
        let line = line.trim();
        if line.is_empty() {
            return Err(ParseError::Empty);
        }
        match scan(line)? {
            Scan::NeedsShell => Ok(CommandLine::Shell(line.to_string())),
            Scan::Words(words) => Self::from_argv(words),
        }
    }

    /// Build a direct invocation from an argument vector (`[program, args...]`).
    pub fn from_argv<I, S>(argv: I) -> Result<Self, ParseError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut iter = argv.into_iter().map(Into::into);
        let program = iter.next().ok_or(ParseError::Empty)?;
        if program.trim().is_empty() {
            return Err(ParseError::Empty);
        }
        Ok(CommandLine::Direct {
            program,
            args: iter.collect(),
        })
    }

    /// Returns `true` if the command runs through a shell.
    pub fn is_shell(&self) -> bool {
        matches!(self, CommandLine::Shell(_))
    }

    /// The program name for direct commands.
    pub fn program(&self) -> Option<&str> {
        match self {
            CommandLine::Direct { program, .. } => Some(program),
            CommandLine::Shell(_) => None,
        }
    }
}

impl fmt::Display for CommandLine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CommandLine::Shell(script) => f.write_str(script),
            CommandLine::Direct { program, args } => {
                f.write_str(&quote_for_display(program))?;
                for arg in args {
                    write!(f, " {}", quote_for_display(arg))?;
                }
                Ok(())
            }
        }
    }
}

fn quote_for_display(word: &str) -> String {
    let plain = !word.is_empty()
        && word
            .chars()
            .all(|c| !c.is_whitespace() && !matches!(c, '\'' | '"' | '\\' | '$' | '`'));
    if plain {
        word.to_string()
    } else if cfg!(windows) {
        format!("\"{}\"", word.replace('"', "\\\""))
    } else {
        format!("'{}'", word.replace('\'', r"'\''"))
    }
}

enum Scan {
    Words(Vec<String>),
    NeedsShell,
}

/// Characters that make a command depend on shell semantics when unquoted.
fn is_shell_meta(c: char, at_word_start: bool) -> bool {
    match c {
        '|' | '&' | ';' | '<' | '>' | '(' | ')' | '$' | '`' | '*' | '?' | '[' | ']' | '{' | '}'
        | '\n' | '\r' => true,
        '~' | '#' => at_word_start,
        '%' | '^' => cfg!(windows),
        _ => false,
    }
}

fn scan(line: &str) -> Result<Scan, ParseError> {
    let unix = !cfg!(windows);
    let mut words = Vec::new();
    let mut current = String::new();
    let mut in_word = false;
    let mut chars = line.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            ' ' | '\t' => {
                if in_word {
                    words.push(std::mem::take(&mut current));
                    in_word = false;
                }
            }
            '\'' => {
                in_word = true;
                loop {
                    match chars.next() {
                        Some('\'') => break,
                        Some(ch) => current.push(ch),
                        None => return Err(ParseError::UnterminatedQuote('\'')),
                    }
                }
            }
            '"' => {
                in_word = true;
                loop {
                    match chars.next() {
                        Some('"') => break,
                        Some('\\') => match chars.peek().copied() {
                            Some(next)
                                if next == '"' || (unix && matches!(next, '\\' | '$' | '`')) =>
                            {
                                current.push(next);
                                chars.next();
                            }
                            _ => current.push('\\'),
                        },
                        Some('$' | '`') if unix => return Ok(Scan::NeedsShell),
                        Some('%') if !unix => return Ok(Scan::NeedsShell),
                        Some(ch) => current.push(ch),
                        None => return Err(ParseError::UnterminatedQuote('"')),
                    }
                }
            }
            '\\' if unix => {
                in_word = true;
                match chars.next() {
                    Some('\n') => return Ok(Scan::NeedsShell),
                    Some(next) => current.push(next),
                    None => current.push('\\'),
                }
            }
            c if is_shell_meta(c, !in_word) => return Ok(Scan::NeedsShell),
            c => {
                in_word = true;
                current.push(c);
            }
        }
    }
    if in_word {
        words.push(current);
    }

    // `NAME=value cmd` assigns environment variables, which only a shell does.
    if unix {
        if let Some((name, _)) = words.first().and_then(|w| w.split_once('=')) {
            let is_identifier = !name.is_empty()
                && !name.starts_with(|c: char| c.is_ascii_digit())
                && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
            if is_identifier {
                return Ok(Scan::NeedsShell);
            }
        }
    }

    if words.is_empty() {
        return Err(ParseError::Empty);
    }
    Ok(Scan::Words(words))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn direct(program: &str, args: &[&str]) -> CommandLine {
        CommandLine::Direct {
            program: program.to_string(),
            args: args.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn shell(script: &str) -> CommandLine {
        CommandLine::Shell(script.to_string())
    }

    #[test]
    fn splits_simple_commands() {
        assert_eq!(
            CommandLine::parse("cargo build --release").unwrap(),
            direct("cargo", &["build", "--release"])
        );
        assert_eq!(
            CommandLine::parse("  go   test\t./... ").unwrap(),
            direct("go", &["test", "./..."])
        );
    }

    #[test]
    fn honours_quotes() {
        assert_eq!(
            CommandLine::parse("echo 'Hello world from Anymon!'").unwrap(),
            direct("echo", &["Hello world from Anymon!"])
        );
        assert_eq!(
            CommandLine::parse(r#"git commit -m "fix: a thing""#).unwrap(),
            direct("git", &["commit", "-m", "fix: a thing"])
        );
        assert_eq!(
            CommandLine::parse(r#"a "x"'y'z"#).unwrap(),
            direct("a", &["xyz"])
        );
        assert_eq!(
            CommandLine::parse(r#"printf """#).unwrap(),
            direct("printf", &[""])
        );
        assert_eq!(
            CommandLine::parse(r#"echo "say \"hi\"""#).unwrap(),
            direct("echo", &[r#"say "hi""#])
        );
    }

    #[test]
    fn quoted_metacharacters_stay_direct() {
        assert_eq!(
            CommandLine::parse("grep 'a|b' file.txt").unwrap(),
            direct("grep", &["a|b", "file.txt"])
        );
        assert_eq!(
            CommandLine::parse(r#"node -e "console.log(1 && 2)""#).unwrap(),
            direct("node", &["-e", "console.log(1 && 2)"])
        );
    }

    #[test]
    fn detects_shell_syntax() {
        for line in [
            "cargo build && cargo test",
            "cat a | grep b",
            "echo hi > out.txt",
            "echo $HOME",
            "ls *.rs",
            "a; b",
            "(cd sub && make)",
            "echo `date`",
            "echo ~",
            "echo hi # comment",
            "sleep 1 &",
            "echo a\nb",
        ] {
            assert_eq!(CommandLine::parse(line).unwrap(), shell(line), "{line}");
        }
    }

    #[test]
    fn trims_shell_scripts() {
        assert_eq!(CommandLine::parse("  a && b  ").unwrap(), shell("a && b"));
    }

    #[test]
    fn tilde_and_hash_inside_words_are_literal() {
        assert_eq!(
            CommandLine::parse("git log HEAD~1 --format=%h#x").unwrap_or_else(|_| shell("")),
            if cfg!(windows) {
                shell("git log HEAD~1 --format=%h#x")
            } else {
                direct("git", &["log", "HEAD~1", "--format=%h#x"])
            }
        );
    }

    #[test]
    fn rejects_empty_commands() {
        assert_eq!(CommandLine::parse(""), Err(ParseError::Empty));
        assert_eq!(CommandLine::parse("   \t "), Err(ParseError::Empty));
        assert_eq!(CommandLine::parse("''"), Err(ParseError::Empty));
        assert_eq!(
            CommandLine::from_argv(Vec::<String>::new()),
            Err(ParseError::Empty)
        );
    }

    #[test]
    fn rejects_unterminated_quotes() {
        assert_eq!(
            CommandLine::parse("echo 'oops"),
            Err(ParseError::UnterminatedQuote('\''))
        );
        assert_eq!(
            CommandLine::parse("echo \"oops"),
            Err(ParseError::UnterminatedQuote('"'))
        );
    }

    #[test]
    fn from_argv_keeps_arguments_verbatim() {
        assert_eq!(
            CommandLine::from_argv(["python", "-c", "print('a && b')"]).unwrap(),
            direct("python", &["-c", "print('a && b')"])
        );
    }

    #[test]
    fn display_round_trips_simple_commands() {
        let cmd = CommandLine::parse("cargo run -- --port 8080").unwrap();
        assert_eq!(cmd.to_string(), "cargo run -- --port 8080");
        let quoted = CommandLine::from_argv(["echo", "hello world"]).unwrap();
        if cfg!(windows) {
            assert_eq!(quoted.to_string(), "echo \"hello world\"");
        } else {
            assert_eq!(quoted.to_string(), "echo 'hello world'");
        }
    }

    #[cfg(unix)]
    #[test]
    fn unix_backslash_escapes() {
        assert_eq!(
            CommandLine::parse(r"echo hello\ world").unwrap(),
            direct("echo", &["hello world"])
        );
        assert_eq!(
            CommandLine::parse(r#"echo "a\\b" "c\d""#).unwrap(),
            direct("echo", &[r"a\b", r"c\d"])
        );
        assert_eq!(
            CommandLine::parse(r#"echo "cost: \$5""#).unwrap(),
            direct("echo", &["cost: $5"])
        );
    }

    #[cfg(unix)]
    #[test]
    fn unix_env_assignment_needs_shell() {
        assert_eq!(
            CommandLine::parse("RUST_LOG=debug cargo run").unwrap(),
            shell("RUST_LOG=debug cargo run")
        );
        assert_eq!(
            CommandLine::parse("cargo run --features=a").unwrap(),
            direct("cargo", &["run", "--features=a"])
        );
    }

    #[cfg(windows)]
    #[test]
    fn windows_paths_keep_backslashes() {
        assert_eq!(
            CommandLine::parse(r"C:\tools\app.exe --out C:\tmp\x").unwrap(),
            direct(r"C:\tools\app.exe", &["--out", r"C:\tmp\x"])
        );
        assert_eq!(
            CommandLine::parse(r#""C:\Program Files\app.exe" run"#).unwrap(),
            direct(r"C:\Program Files\app.exe", &["run"])
        );
        assert_eq!(
            CommandLine::parse("echo %PATH%").unwrap(),
            shell("echo %PATH%")
        );
    }
}
