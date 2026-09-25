//! Terminal output of anymon itself.
//!
//! Status messages go to stderr so they never mix with data a task writes to
//! stdout. Output of the tasks themselves is passed through untouched.

use std::fmt::Display;
use std::io::Write;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::Duration;

use colored::{Color, ColoredString, Colorize};

/// How much anymon prints about what it is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Verbosity {
    /// Only errors and failed runs.
    Quiet = 0,
    /// Starts, exits and detected changes.
    Normal = 1,
    /// Additionally every file event and watcher detail.
    Verbose = 2,
}

static VERBOSITY: AtomicU8 = AtomicU8::new(Verbosity::Normal as u8);

/// Set the global verbosity.
pub fn set_verbosity(verbosity: Verbosity) {
    VERBOSITY.store(verbosity as u8, Ordering::Relaxed);
}

/// The global verbosity.
pub fn verbosity() -> Verbosity {
    match VERBOSITY.load(Ordering::Relaxed) {
        0 => Verbosity::Quiet,
        1 => Verbosity::Normal,
        _ => Verbosity::Verbose,
    }
}

/// Enable or disable colored output.
pub fn set_color(enabled: bool) {
    colored::control::set_override(enabled);
}

fn enabled(level: Verbosity) -> bool {
    verbosity() >= level
}

fn emit(line: std::fmt::Arguments<'_>) {
    let mut stderr = std::io::stderr().lock();
    let _ = writeln!(stderr, "{line}");
}

fn prefix() -> ColoredString {
    "[anymon]".cyan().bold()
}

/// A normal status message.
pub fn info(msg: impl Display) {
    if enabled(Verbosity::Normal) {
        emit(format_args!("{} {msg}", prefix()));
    }
}

/// A message only shown with `--verbose`.
pub fn detail(msg: impl Display) {
    if enabled(Verbosity::Verbose) {
        emit(format_args!("{} {}", prefix(), msg.to_string().dimmed()));
    }
}

/// A warning, always shown.
pub fn warn(msg: impl Display) {
    emit(format_args!(
        "{} {} {msg}",
        prefix(),
        "warning:".yellow().bold()
    ));
}

/// A fatal error, always shown.
pub fn error(msg: impl Display) {
    emit(format_args!("{} {msg}", "error:".red().bold()));
}

const TASK_COLORS: [Color; 6] = [
    Color::Blue,
    Color::Magenta,
    Color::Yellow,
    Color::Cyan,
    Color::BrightBlue,
    Color::BrightMagenta,
];

/// The colored `[name]` tag used for messages about a task.
#[derive(Debug, Clone)]
pub struct TaskLabel {
    name: String,
    color: Color,
}

impl TaskLabel {
    /// Create a label; `index` picks the color.
    pub fn new(name: impl Into<String>, index: usize) -> Self {
        TaskLabel {
            name: name.into(),
            color: TASK_COLORS[index % TASK_COLORS.len()],
        }
    }

    /// The task name.
    pub fn name(&self) -> &str {
        &self.name
    }

    fn tag(&self) -> ColoredString {
        format!("[{}]", self.name).color(self.color).bold()
    }

    /// A normal status message about the task.
    pub fn info(&self, msg: impl Display) {
        if enabled(Verbosity::Normal) {
            emit(format_args!("{} {msg}", self.tag()));
        }
    }

    /// The command line that is about to run.
    pub fn command(&self, command: impl Display) {
        if enabled(Verbosity::Normal) {
            emit(format_args!(
                "{} {}",
                self.tag(),
                format!("$ {command}").dimmed()
            ));
        }
    }

    /// A verbose-only message about the task.
    pub fn detail(&self, msg: impl Display) {
        if enabled(Verbosity::Verbose) {
            emit(format_args!("{} {}", self.tag(), msg.to_string().dimmed()));
        }
    }

    /// A successful run.
    pub fn success(&self, msg: impl Display) {
        if enabled(Verbosity::Normal) {
            emit(format_args!("{} {}", self.tag(), msg.to_string().green()));
        }
    }

    /// A failed run, shown even with `--quiet`.
    pub fn failure(&self, msg: impl Display) {
        emit(format_args!("{} {}", self.tag(), msg.to_string().red()));
    }
}

/// Format a duration for humans: `85ms`, `1.24s`, `2m 05s`.
pub fn format_duration(duration: Duration) -> String {
    let millis = duration.as_millis();
    if millis < 1000 {
        format!("{millis}ms")
    } else if millis < 60_000 {
        format!("{:.2}s", duration.as_secs_f64())
    } else {
        let secs = duration.as_secs();
        format!("{}m {:02}s", secs / 60, secs % 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_durations() {
        assert_eq!(format_duration(Duration::from_millis(0)), "0ms");
        assert_eq!(format_duration(Duration::from_millis(85)), "85ms");
        assert_eq!(format_duration(Duration::from_millis(1240)), "1.24s");
        assert_eq!(format_duration(Duration::from_secs(59)), "59.00s");
        assert_eq!(format_duration(Duration::from_secs(125)), "2m 05s");
    }

    #[test]
    fn labels_cycle_through_colors() {
        assert_eq!(TaskLabel::new("a", 0).color, TaskLabel::new("b", 6).color);
        assert_ne!(TaskLabel::new("a", 0).color, TaskLabel::new("b", 1).color);
        assert_eq!(TaskLabel::new("build", 3).name(), "build");
    }
}
