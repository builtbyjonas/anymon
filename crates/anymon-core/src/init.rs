//! `anymon init`: create a starter config for the current project.

use std::path::Path;

/// A detected project type and the config generated for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Template {
    /// Human-readable project kind, e.g. `Rust`.
    pub kind: &'static str,
    /// The generated `Anymon.toml`.
    pub content: String,
}

struct Task {
    name: &'static str,
    watch: Vec<&'static str>,
    ignore: Vec<&'static str>,
    run: String,
}

/// Build a config template suited to the project in `dir`.
pub fn template_for(dir: &Path) -> Template {
    let (kind, task) = detect(dir);
    Template {
        kind,
        content: render(&task),
    }
}

fn detect(dir: &Path) -> (&'static str, Task) {
    let has = |name: &str| dir.join(name).exists();

    if has("Cargo.toml") {
        return (
            "Rust",
            Task {
                name: "run",
                watch: vec!["**/*.rs", "**/Cargo.toml"],
                ignore: vec![],
                run: "cargo run".into(),
            },
        );
    }
    if has("package.json") {
        let manager = if has("pnpm-lock.yaml") {
            "pnpm"
        } else if has("yarn.lock") {
            "yarn"
        } else if has("bun.lockb") || has("bun.lock") {
            "bun"
        } else {
            "npm"
        };
        let scripts = std::fs::read_to_string(dir.join("package.json")).unwrap_or_default();
        let has_script = |name: &str| scripts.contains(&format!("\"{name}\""));
        let run = if has_script("dev") {
            format!("{manager} run dev")
        } else if has_script("start") {
            format!("{manager} start")
        } else {
            "node index.js".to_string()
        };
        return (
            "Node.js",
            Task {
                name: "app",
                watch: vec!["**/*.{js,mjs,cjs,ts,mts,cts,jsx,tsx,json}"],
                ignore: vec!["node_modules/**", "dist/**", "build/**"],
                run,
            },
        );
    }
    if has("go.mod") {
        return (
            "Go",
            Task {
                name: "run",
                watch: vec!["**/*.go", "go.mod"],
                ignore: vec![],
                run: "go run .".into(),
            },
        );
    }
    if has("pyproject.toml") || has("requirements.txt") || has("main.py") {
        let python = if cfg!(windows) { "python" } else { "python3" };
        return (
            "Python",
            Task {
                name: "run",
                watch: vec!["**/*.py"],
                ignore: vec!["**/__pycache__/**", ".venv/**", "venv/**"],
                run: format!("{python} main.py"),
            },
        );
    }
    (
        "generic",
        Task {
            name: "hello",
            watch: vec!["**/*"],
            ignore: vec![],
            run: "echo 'Something changed!'".into(),
        },
    )
}

fn toml_list(items: &[&str]) -> String {
    let quoted: Vec<String> = items.iter().map(|i| format!("{i:?}")).collect();
    format!("[{}]", quoted.join(", "))
}

fn render(task: &Task) -> String {
    format!(
        r#"# Anymon configuration.
# Reference: https://github.com/builtbyjonas/anymon/blob/main/docs/configuration.md

[global]
# Milliseconds to wait for changes to settle before running tasks.
debounce = 50
# Glob patterns ignored by every task. Files excluded by .gitignore are
# ignored automatically (set `gitignore = false` to disable that).
ignore = []

[[task]]
name = "{name}"
# Files that trigger this task. Patterns without a "/" match at any depth.
watch = {watch}
# Files this task ignores.
ignore = {ignore}
# The command. Shell syntax such as `&&` or pipes works; use an array like
# ["cargo", "run"] to pass arguments verbatim without any parsing.
run = {run:?}
# Restart the command when files change while it is still running. With
# `false` the current run finishes first and then the task runs again.
restart = true
"#,
        name = task.name,
        watch = toml_list(&task.watch),
        ignore = toml_list(&task.ignore),
        run = task.run,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use anymon_config::{Config, Run};

    fn template_with(files: &[(&str, &str)]) -> Template {
        let dir = tempfile::tempdir().unwrap();
        for (name, content) in files {
            std::fs::write(dir.path().join(name), content).unwrap();
        }
        template_for(dir.path())
    }

    fn parse(template: &Template) -> Config {
        template
            .content
            .parse()
            .expect("template is a valid config")
    }

    #[test]
    fn detects_rust() {
        let template = template_with(&[("Cargo.toml", "")]);
        assert_eq!(template.kind, "Rust");
        let cfg = parse(&template);
        assert_eq!(cfg.tasks[0].run, Run::Command("cargo run".into()));
        assert_eq!(cfg.tasks[0].watch, vec!["**/*.rs", "**/Cargo.toml"]);
    }

    #[test]
    fn detects_node_package_managers_and_scripts() {
        let template = template_with(&[
            ("package.json", r#"{"scripts":{"dev":"vite"}}"#),
            ("pnpm-lock.yaml", ""),
        ]);
        assert_eq!(template.kind, "Node.js");
        assert_eq!(
            parse(&template).tasks[0].run,
            Run::Command("pnpm run dev".into())
        );

        let template = template_with(&[("package.json", r#"{"scripts":{"start":"node ."}}"#)]);
        assert_eq!(
            parse(&template).tasks[0].run,
            Run::Command("npm start".into())
        );

        let template = template_with(&[("package.json", "{}"), ("yarn.lock", "")]);
        assert_eq!(
            parse(&template).tasks[0].run,
            Run::Command("node index.js".into())
        );
    }

    #[test]
    fn detects_go_and_python() {
        assert_eq!(template_with(&[("go.mod", "")]).kind, "Go");
        let template = template_with(&[("main.py", "")]);
        assert_eq!(template.kind, "Python");
        assert!(
            matches!(&parse(&template).tasks[0].run, Run::Command(c) if c.ends_with("main.py"))
        );
    }

    #[test]
    fn falls_back_to_a_generic_template() {
        let template = template_with(&[]);
        assert_eq!(template.kind, "generic");
        let cfg = parse(&template);
        assert_eq!(cfg.global.debounce, Some(50));
        assert_eq!(cfg.tasks[0].restart, Some(true));
    }
}
