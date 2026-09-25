//! `anymon check`: validate the config and print what anymon will do.

use std::fmt::{Display, Write};

use anymon_runner::paths::display_relative;
use anymon_runner::pattern::Pattern;
use anymon_runner::ui::format_duration;
use anymon_runner::Plan;
use colored::Colorize;

/// Render a human-readable description of a plan.
pub fn describe(plan: &Plan) -> String {
    let mut out = String::new();
    let rel = |path: &std::path::Path| display_relative(path, &plan.root);

    if let Some(config) = &plan.config_path {
        let _ = writeln!(out, "{} {}", "config OK:".green().bold(), config.display());
    }
    let _ = writeln!(out);
    row(&mut out, 2, "root", plan.root.display());
    row(&mut out, 2, "debounce", format_duration(plan.debounce));
    row(
        &mut out,
        2,
        "kill timeout",
        format_duration(plan.kill_timeout),
    );
    row(
        &mut out,
        2,
        "gitignore",
        yes_no(plan.gitignore, "respected", "not used"),
    );
    if let Some(interval) = plan.poll {
        row(
            &mut out,
            2,
            "polling",
            format!("every {}", format_duration(interval)),
        );
    }
    row(&mut out, 2, "ignore", patterns(&plan.ignore_patterns));

    let (bases, missing) = plan.watch_bases();
    let watched = bases
        .iter()
        .map(|b| {
            let suffix = if b.recursive { "" } else { " (files only)" };
            format!("{}{suffix}", rel(&b.path))
        })
        .collect();
    row(&mut out, 2, "watching", list(watched));
    for path in missing {
        row(
            &mut out,
            2,
            "",
            format!("{} {}", "missing:".yellow(), path.display()),
        );
    }

    for task in &plan.tasks {
        let _ = writeln!(out);
        let _ = writeln!(out, "  {}", format!("[{}]", task.name).bold());
        let how = if task.command.is_shell() {
            format!("via {}", task.shell)
        } else {
            "direct".to_string()
        };
        row(
            &mut out,
            4,
            "run",
            format!("{}  {}", task.display, format!("({how})").dimmed()),
        );
        row(&mut out, 4, "watch", patterns(&task.watch_patterns));
        row(&mut out, 4, "ignore", patterns(&task.ignore_patterns));
        row(&mut out, 4, "cwd", rel(&task.cwd));
        if !task.env.is_empty() {
            let env = task.env.iter().map(|(k, v)| format!("{k}={v}")).collect();
            row(&mut out, 4, "env", list(env));
        }
        row(
            &mut out,
            4,
            "on change",
            yes_no(task.restart, "restart", "run again after the current run"),
        );
        row(
            &mut out,
            4,
            "on start",
            yes_no(task.run_on_start, "run", "wait for changes"),
        );
    }
    out
}

fn row(out: &mut String, indent: usize, key: &str, value: impl Display) {
    let width = if indent == 2 { 13 } else { 11 };
    let _ = writeln!(out, "{:indent$}{key:<width$}{value}", "");
}

fn yes_no(value: bool, yes: &'static str, no: &'static str) -> &'static str {
    if value {
        yes
    } else {
        no
    }
}

fn patterns(patterns: &[Pattern]) -> String {
    list(patterns.iter().map(|p| p.source.clone()).collect())
}

fn list(items: Vec<String>) -> String {
    if items.is_empty() {
        "-".to_string()
    } else {
        items.join(", ")
    }
}
