//! End-to-end tests that run the `anymon` binary.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Output, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const TIMEOUT: Duration = Duration::from_secs(20);

fn anymon() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_anymon"));
    cmd.arg("--color").arg("never").env("NO_COLOR", "1");
    cmd
}

struct Project {
    _dir: tempfile::TempDir,
    root: PathBuf,
}

impl Project {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let canonical = dir.path().canonicalize().unwrap();
        // Avoid Windows `\\?\` paths, which are awkward working directories.
        let root = match canonical.to_str().and_then(|p| p.strip_prefix(r"\\?\")) {
            Some(stripped) => PathBuf::from(stripped),
            None => canonical,
        };
        Project { _dir: dir, root }
    }

    fn with_config(config: &str) -> Self {
        let project = Project::new();
        project.write("Anymon.toml", config);
        project
    }

    fn path(&self, rel: &str) -> PathBuf {
        self.root.join(rel)
    }

    fn write(&self, rel: &str, content: &str) {
        let path = self.path(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }

    fn read(&self, rel: &str) -> String {
        std::fs::read_to_string(self.path(rel)).unwrap_or_default()
    }

    fn lines(&self, rel: &str) -> usize {
        self.read(rel)
            .lines()
            .filter(|l| !l.trim().is_empty())
            .count()
    }

    fn run(&self, args: &[&str]) -> Output {
        anymon()
            .args(args)
            .current_dir(&self.root)
            .output()
            .unwrap()
    }

    fn start(&self, args: &[&str]) -> Watcher {
        let mut child = anymon()
            .args(args)
            .current_dir(&self.root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take();
        let output = Arc::new(Mutex::new(String::new()));
        for mut stream in [
            Box::new(child.stdout.take().unwrap()) as Box<dyn Read + Send>,
            Box::new(child.stderr.take().unwrap()),
        ] {
            let output = output.clone();
            std::thread::spawn(move || {
                let mut buf = [0u8; 4096];
                while let Ok(n) = stream.read(&mut buf) {
                    if n == 0 {
                        break;
                    }
                    output
                        .lock()
                        .unwrap()
                        .push_str(&String::from_utf8_lossy(&buf[..n]));
                }
            });
        }
        Watcher {
            child,
            stdin,
            output,
        }
    }
}

/// A running `anymon` watch session; killed on drop.
struct Watcher {
    child: Child,
    stdin: Option<ChildStdin>,
    output: Arc<Mutex<String>>,
}

impl Watcher {
    fn send(&mut self, line: &str) {
        let stdin = self.stdin.as_mut().unwrap();
        writeln!(stdin, "{line}").unwrap();
        stdin.flush().unwrap();
    }

    fn output(&self) -> String {
        self.output.lock().unwrap().clone()
    }

    fn wait_for_output(&self, needle: &str) {
        wait_until(
            &format!("output containing {needle:?}"),
            || self.output().contains(needle),
            || self.output(),
        );
    }

    fn quit(mut self) -> (i32, String) {
        self.send("q");
        let deadline = Instant::now() + TIMEOUT;
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                // Give the reader threads a moment to drain the pipes.
                std::thread::sleep(Duration::from_millis(100));
                return (status.code().unwrap_or(-1), self.output());
            }
            assert!(
                Instant::now() < deadline,
                "anymon did not quit:\n{}",
                self.output()
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

impl Drop for Watcher {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn wait_until(what: &str, mut condition: impl FnMut() -> bool, context: impl Fn() -> String) {
    let deadline = Instant::now() + TIMEOUT;
    while !condition() {
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {what}\n--- output ---\n{}",
            context()
        );
        std::thread::sleep(Duration::from_millis(25));
    }
}

fn wait_for_lines(project: &Project, watcher: &Watcher, file: &str, count: usize) {
    wait_until(
        &format!("{count} line(s) in {file}"),
        || project.lines(file) >= count,
        || {
            format!(
                "{}\n--- {file} ---\n{}",
                watcher.output(),
                project.read(file)
            )
        },
    );
}

/// Make sure a change is not picked up: wait a while and check nothing ran.
fn assert_stays(project: &Project, watcher: &Watcher, file: &str, count: usize) {
    std::thread::sleep(Duration::from_millis(700));
    assert_eq!(
        project.lines(file),
        count,
        "unexpected run\n--- output ---\n{}",
        watcher.output()
    );
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// A command that appends one line to `file` (works with `sh` and `cmd`).
fn append(file: &str, text: &str) -> String {
    format!("echo {text}>> {file}")
}

#[test]
fn prints_version_and_help() {
    let project = Project::new();
    let output = project.run(&["--version"]);
    assert!(output.status.success());
    assert_eq!(
        stdout(&output).trim(),
        format!("anymon {}", env!("CARGO_PKG_VERSION"))
    );

    let output = project.run(&["--help"]);
    assert!(output.status.success());
    let help = stdout(&output);
    for needle in ["Subcommands", "Watch options", "--once", "Examples"] {
        assert!(help.contains(needle), "help lacks {needle}:\n{help}");
    }
}

#[test]
fn prints_shell_completions() {
    let output = Project::new().run(&["completions", "bash"]);
    assert!(output.status.success());
    assert!(stdout(&output).contains("anymon"));
}

#[test]
fn init_creates_a_config_once() {
    let project = Project::new();
    project.write("Cargo.toml", "[package]\nname = \"x\"\n");
    let output = project.run(&["init"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(stdout(&output).contains("Rust project"));
    assert!(project.read("Anymon.toml").contains("cargo run"));

    let output = project.run(&["init"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("already exists"));

    assert!(project.run(&["init", "--force"]).status.success());
    assert!(project.run(&["check"]).status.success());
}

#[test]
fn check_shows_the_resolved_tasks() {
    let project = Project::with_config(
        "[global]\ndebounce = 75\n[[task]]\nname = \"build\"\nwatch = [\"src/**\"]\nrun = \"cargo build\"\n",
    );
    let output = project.run(&["check"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("config OK"), "{text}");
    assert!(text.contains("[build]"), "{text}");
    assert!(text.contains("75ms"), "{text}");
    assert!(text.contains("cargo build"), "{text}");
}

#[test]
fn config_errors_exit_with_code_2() {
    let project = Project::with_config("[[task]]\nrun = \"x\"\nrestrat = true\n");
    for args in [&["check"][..], &["--once"][..], &[][..]] {
        let output = project.run(args);
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        let err = stderr(&output);
        assert!(err.contains("unknown field `restrat`"), "{err}");
        assert!(err.contains("line 3"), "{err}");
    }
}

#[test]
fn missing_config_suggests_init() {
    let project = Project::new();
    let output = project.run(&[]);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("anymon init"));
}

#[test]
fn finds_the_config_in_a_parent_directory() {
    let project = Project::with_config(&format!(
        "[[task]]\nrun = \"{}\"\n",
        append("ran.txt", "ok")
    ));
    std::fs::create_dir_all(project.path("sub/dir")).unwrap();
    let output = anymon()
        .arg("--once")
        .current_dir(project.path("sub/dir"))
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    // Tasks run in the directory of the config file.
    assert_eq!(project.lines("ran.txt"), 1);
}

#[test]
fn once_runs_every_task_and_reports_the_first_failure() {
    let project = Project::with_config(&format!(
        "[[task]]\nname = \"ok\"\nrun = \"{}\"\n[[task]]\nname = \"bad\"\nrun = \"exit 3\"\n",
        append("ok.txt", "done")
    ));
    let output = project.run(&["--once"]);
    assert_eq!(output.status.code(), Some(3), "{}", stderr(&output));
    assert_eq!(project.lines("ok.txt"), 1);
    let err = stderr(&output);
    assert!(err.contains("[bad] failed with exit code 3"), "{err}");

    let output = project.run(&["--once", "-t", "ok"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(project.lines("ok.txt"), 2);

    let output = project.run(&["--once", "-t", "missing"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("unknown task 'missing' (available: ok, bad)"));
}

#[test]
fn runs_ad_hoc_commands() {
    let project = Project::new();
    let output = project.run(&["--once", "--", &append("adhoc.txt", "hi")]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(project.read("adhoc.txt").trim(), "hi");

    let output = project.run(&["--once", "-e", "rs", "--", "exit 4"]);
    assert_eq!(output.status.code(), Some(4));
}

#[test]
fn rejects_misplaced_options() {
    let project = Project::with_config("[[task]]\nrun = \"x\"\n");
    let output = project.run(&["-e", "rs"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("--exts only applies to a command"));

    let output = project.run(&["-t", "x", "--", "make"]);
    assert_eq!(output.status.code(), Some(2));

    let output = project.run(&["init", "--once"]);
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn run_passes_on_the_exit_code() {
    let project = Project::new();
    assert_eq!(project.run(&["run", "exit 5"]).status.code(), Some(5));
    assert_eq!(project.run(&["run", "exit", "6"]).status.code(), Some(6));
    let output = project.run(&["run", &append("run.txt", "quoted")]);
    assert!(output.status.success());
    assert_eq!(project.read("run.txt").trim(), "quoted");
    assert!(!project
        .run(&["run", "anymon-no-such-program-xyz"])
        .status
        .success());
}

#[test]
fn watch_restarts_on_change_and_ignores_the_rest() {
    let project = Project::with_config(&format!(
        r#"
[global]
debounce = 30

[[task]]
name = "build"
watch = ["src/**", "*.md"]
ignore = ["src/*.tmp"]
run = "{}"
"#,
        append("runs.log", "run")
    ));
    project.write(".gitignore", "generated/\n");
    project.write("src/main.txt", "one");
    std::fs::create_dir_all(project.path("generated")).unwrap();

    let mut watcher = project.start(&[]);
    wait_for_lines(&project, &watcher, "runs.log", 1);

    project.write("src/main.txt", "two");
    wait_for_lines(&project, &watcher, "runs.log", 2);

    // New nested directories are picked up.
    project.write("src/deep/er/new.txt", "x");
    wait_for_lines(&project, &watcher, "runs.log", 3);

    // Patterns without a slash match at any depth.
    project.write("docs/guide.md", "x");
    wait_for_lines(&project, &watcher, "runs.log", 4);

    // Ignored, gitignored and unwatched files do not trigger a run.
    project.write("src/scratch.tmp", "x");
    project.write("generated/src/out.md", "x");
    project.write("other/file.txt", "x");
    assert_stays(&project, &watcher, "runs.log", 4);

    watcher.send("rs");
    wait_for_lines(&project, &watcher, "runs.log", 5);

    watcher.send("status");
    watcher.wait_for_output("[build] idle, last run finished");

    let (code, output) = watcher.quit();
    assert_eq!(code, 0, "{output}");
    let changed = Path::new("src").join("main.txt");
    assert!(
        output.contains(&format!("{} changed", changed.display())),
        "{output}"
    );
}

#[test]
fn watch_reloads_the_config() {
    let config = |text: &str| {
        format!(
            "[global]\ndebounce = 30\n[[task]]\nname = \"t\"\nwatch = \"src/**\"\nrun = \"{}\"\n",
            append("runs.log", text)
        )
    };
    let project = Project::with_config(&config("first"));
    std::fs::create_dir_all(project.path("src")).unwrap();

    let watcher = project.start(&[]);
    wait_for_lines(&project, &watcher, "runs.log", 1);

    project.write("Anymon.toml", &config("second"));
    wait_for_lines(&project, &watcher, "runs.log", 2);
    assert!(project.read("runs.log").contains("second"));

    // A broken config keeps the previous one running.
    project.write("Anymon.toml", "[[task]]\nrun = ");
    watcher.wait_for_output("config not reloaded");
    project.write("src/a.txt", "x");
    wait_for_lines(&project, &watcher, "runs.log", 3);

    let (code, _) = watcher.quit();
    assert_eq!(code, 0);
}

#[test]
fn watch_queues_runs_instead_of_restarting() {
    let slow = if cfg!(windows) {
        "ping -n 2 127.0.0.1 > nul"
    } else {
        "sleep 1"
    };
    let project = Project::with_config(&format!(
        "[global]\ndebounce = 20\n[[task]]\nwatch = \"src/**\"\nrestart = false\nrun_on_start = false\nrun = \"{} && {}\"\n",
        slow,
        append("runs.log", "run")
    ));
    std::fs::create_dir_all(project.path("src")).unwrap();

    let watcher = project.start(&[]);
    watcher.wait_for_output("waiting for changes");
    std::thread::sleep(Duration::from_millis(300));

    project.write("src/a.txt", "1");
    std::thread::sleep(Duration::from_millis(300));
    project.write("src/b.txt", "2");
    std::thread::sleep(Duration::from_millis(100));
    project.write("src/c.txt", "3");

    // One run for the first change, exactly one more for the rest.
    wait_for_lines(&project, &watcher, "runs.log", 2);
    assert_stays(&project, &watcher, "runs.log", 2);
    let (code, _) = watcher.quit();
    assert_eq!(code, 0);
}

#[test]
fn watch_polls_when_asked() {
    let project = Project::with_config(&format!(
        "[global]\ndebounce = 20\n[[task]]\nwatch = \"src/**\"\nrun = \"{}\"\n",
        append("runs.log", "run")
    ));
    project.write("src/a.txt", "1");
    let watcher = project.start(&["--poll", "50"]);
    wait_for_lines(&project, &watcher, "runs.log", 1);
    std::thread::sleep(Duration::from_millis(1100));
    project.write("src/a.txt", "22");
    wait_for_lines(&project, &watcher, "runs.log", 2);
    let (code, _) = watcher.quit();
    assert_eq!(code, 0);
}

#[cfg(unix)]
#[test]
fn watch_stops_task_trees_on_sigterm() {
    let project = Project::with_config(
        "[[task]]\nname = \"server\"\nrun = \"sleep 60 & echo $! > child.pid; wait\"\n",
    );
    let watcher = project.start(&[]);
    wait_until(
        "child.pid",
        || !project.read("child.pid").trim().is_empty(),
        || watcher.output(),
    );
    let pid: i32 = project.read("child.pid").trim().parse().unwrap();

    let status = Command::new("kill")
        .args(["-TERM", &watcher.child.id().to_string()])
        .status()
        .unwrap();
    assert!(status.success());
    let mut watcher = watcher;
    let deadline = Instant::now() + TIMEOUT;
    let code = loop {
        if let Some(status) = watcher.child.try_wait().unwrap() {
            break status.code();
        }
        assert!(Instant::now() < deadline, "anymon ignored SIGTERM");
        std::thread::sleep(Duration::from_millis(20));
    };
    assert_eq!(code, Some(0));
    wait_until(
        "the background child to exit",
        || {
            Command::new("kill")
                .args(["-0", &pid.to_string()])
                .stderr(Stdio::null())
                .status()
                .map(|s| !s.success())
                .unwrap_or(true)
        },
        || watcher.output(),
    );
}

#[test]
fn check_accepts_an_explicit_config_path() {
    let project = Project::new();
    project.write(
        "conf/custom.toml",
        "[[task]]\nname = \"x\"\nrun = \"make\"\n",
    );
    let output = project.run(&["check", "--config", "conf/custom.toml"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(stdout(&output).contains("[x]"));

    let output = project.run(&["check", "--config", "nope.toml"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("not found"));
}

#[test]
fn legacy_debug_alias_still_works() {
    let project = Project::with_config("[[task]]\nrun = \"make\"\n");
    assert!(project.run(&["debug"]).status.success());
    assert!(Path::new(env!("CARGO_BIN_EXE_anymon")).exists());
}

#[cfg(unix)]
#[test]
fn changes_during_a_slow_stop_cause_one_restart() {
    // The task ignores SIGTERM, so every stop takes the full kill timeout.
    let project = Project::with_config(
        "[global]\nkill_timeout = 1000\ndebounce = 20\n[[task]]\nwatch = \"src/**\"\n\
         run = \"trap '' TERM; echo started >> starts.log; while true; do sleep 0.05; done\"\n",
    );
    std::fs::create_dir_all(project.path("src")).unwrap();
    let watcher = project.start(&[]);
    wait_for_lines(&project, &watcher, "starts.log", 1);
    std::thread::sleep(Duration::from_millis(200));

    for i in 0..3 {
        project.write("src/a.txt", &i.to_string());
        std::thread::sleep(Duration::from_millis(150));
    }
    wait_for_lines(&project, &watcher, "starts.log", 2);
    std::thread::sleep(Duration::from_millis(1500));
    assert_eq!(project.lines("starts.log"), 2, "{}", watcher.output());
    let (code, _) = watcher.quit();
    assert_eq!(code, 0);
}

#[test]
fn new_directories_respect_ignore_rules() {
    let project = Project::with_config(&format!(
        "[global]\ndebounce = 20\nignore = [\"*.log\"]\n[[task]]\nwatch = \"src/**/*.{{txt,log,bin}}\"\nrun = \"{}\"\n",
        append("runs.log", "run")
    ));
    project.write(".gitignore", "cache/\n");
    std::fs::create_dir_all(project.path("src")).unwrap();
    // Polling registers directories one by one on every platform.
    let watcher = project.start(&["--poll", "50"]);
    wait_for_lines(&project, &watcher, "runs.log", 1);
    std::thread::sleep(Duration::from_millis(1100));

    project.write("src/fresh/debug.log", "x");
    project.write("src/fresh/cache/data.bin", "x");
    assert_stays(&project, &watcher, "runs.log", 1);

    project.write("src/fresh/nested/code.txt", "x");
    wait_for_lines(&project, &watcher, "runs.log", 2);
    let (code, _) = watcher.quit();
    assert_eq!(code, 0);
}
