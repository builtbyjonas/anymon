use std::path::Path;
use std::time::{Duration, Instant};

use anymon_shell::{describe_exit, exit_code, spawn, CommandLine, SpawnOptions};

fn options_in(dir: &Path) -> SpawnOptions {
    SpawnOptions {
        cwd: Some(dir.to_path_buf()),
        ..SpawnOptions::default()
    }
}

#[cfg(unix)]
async fn wait_for_file(path: &Path, timeout: Duration) -> String {
    let deadline = Instant::now() + timeout;
    loop {
        if let Ok(content) = std::fs::read_to_string(path) {
            if !content.trim().is_empty() {
                return content;
            }
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {}",
            path.display()
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test]
async fn reports_exit_codes() {
    let dir = tempfile::tempdir().unwrap();
    let mut process = spawn(
        &CommandLine::Shell("exit 3".into()),
        &options_in(dir.path()),
    )
    .unwrap();
    let status = process.wait().await.unwrap();
    assert_eq!(status.code(), Some(3));
    assert_eq!(exit_code(&status), 3);
    assert_eq!(describe_exit(&status), "exit code 3");
    assert_eq!(process.status(), Some(status));
}

#[tokio::test]
async fn runs_direct_commands_with_arguments() {
    let dir = tempfile::tempdir().unwrap();
    let cmd = CommandLine::parse("echo 'hello world'").unwrap();
    let mut process = spawn(&cmd, &options_in(dir.path())).unwrap();
    assert!(process.wait().await.unwrap().success());
}

#[tokio::test]
async fn passes_environment_and_working_directory() {
    let dir = tempfile::tempdir().unwrap();
    let script = if cfg!(windows) {
        "echo %ANYMON_TEST_VALUE%> out.txt"
    } else {
        "echo \"$ANYMON_TEST_VALUE\" > out.txt"
    };
    let mut options = options_in(dir.path());
    options
        .env
        .push(("ANYMON_TEST_VALUE".into(), "from-env".into()));
    let mut process = spawn(&CommandLine::Shell(script.into()), &options).unwrap();
    assert!(process.wait().await.unwrap().success());
    let out = std::fs::read_to_string(dir.path().join("out.txt")).unwrap();
    assert_eq!(out.trim(), "from-env");
}

#[tokio::test]
async fn unknown_programs_fall_back_to_the_shell() {
    let dir = tempfile::tempdir().unwrap();
    let cmd = CommandLine::from_argv(["anymon-definitely-not-a-program"]).unwrap();
    let mut process = spawn(&cmd, &options_in(dir.path())).unwrap();
    let status = process.wait().await.unwrap();
    assert!(!status.success());
}

#[tokio::test]
async fn shell_builtins_work_as_direct_commands() {
    let dir = tempfile::tempdir().unwrap();
    // `cd` is a builtin on every platform and has no executable on PATH.
    let cmd = CommandLine::from_argv(["cd", "."]).unwrap();
    let mut process = spawn(&cmd, &options_in(dir.path())).unwrap();
    assert!(process.wait().await.unwrap().success());
}

#[tokio::test]
async fn terminate_stops_long_running_processes() {
    let dir = tempfile::tempdir().unwrap();
    let script = if cfg!(windows) {
        "ping -n 30 127.0.0.1 > nul"
    } else {
        "sleep 30"
    };
    let mut process = spawn(&CommandLine::Shell(script.into()), &options_in(dir.path())).unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(process.try_wait().unwrap().is_none());

    let started = Instant::now();
    let status = process.terminate(Duration::from_secs(5)).await.unwrap();
    assert!(!status.success());
    assert!(
        started.elapsed() < Duration::from_secs(4),
        "terminate took {:?}",
        started.elapsed()
    );
}

#[tokio::test]
async fn terminate_after_exit_returns_the_original_status() {
    let dir = tempfile::tempdir().unwrap();
    let mut process = spawn(
        &CommandLine::Shell("exit 7".into()),
        &options_in(dir.path()),
    )
    .unwrap();
    process.wait().await.unwrap();
    let status = process.terminate(Duration::from_millis(100)).await.unwrap();
    assert_eq!(status.code(), Some(7));
}

#[cfg(unix)]
mod unix {
    use super::*;

    fn is_alive(pid: i32) -> bool {
        unsafe { libc::kill(pid, 0) == 0 }
    }

    async fn wait_until_dead(pid: i32) {
        let deadline = Instant::now() + Duration::from_secs(3);
        while is_alive(pid) {
            assert!(Instant::now() < deadline, "process {pid} is still alive");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    #[tokio::test]
    async fn terminate_kills_grandchildren() {
        let dir = tempfile::tempdir().unwrap();
        let script = "sleep 30 & echo $! > child.pid; wait";
        let mut process =
            spawn(&CommandLine::Shell(script.into()), &options_in(dir.path())).unwrap();
        let pid: i32 = wait_for_file(&dir.path().join("child.pid"), Duration::from_secs(5))
            .await
            .trim()
            .parse()
            .unwrap();
        assert!(is_alive(pid));

        process.terminate(Duration::from_secs(2)).await.unwrap();
        wait_until_dead(pid).await;
    }

    #[tokio::test]
    async fn terminate_is_graceful_first() {
        let dir = tempfile::tempdir().unwrap();
        let script = "trap 'echo stopped > marker; exit 0' TERM; echo ready > ready; while true; do sleep 0.05; done";
        let mut process =
            spawn(&CommandLine::Shell(script.into()), &options_in(dir.path())).unwrap();
        wait_for_file(&dir.path().join("ready"), Duration::from_secs(5)).await;

        let status = process.terminate(Duration::from_secs(5)).await.unwrap();
        assert_eq!(status.code(), Some(0));
        assert_eq!(
            std::fs::read_to_string(dir.path().join("marker"))
                .unwrap()
                .trim(),
            "stopped"
        );
    }

    #[tokio::test]
    async fn terminate_escalates_to_sigkill() {
        let dir = tempfile::tempdir().unwrap();
        let script = "trap '' TERM; echo ready > ready; while true; do sleep 0.05; done";
        let mut process =
            spawn(&CommandLine::Shell(script.into()), &options_in(dir.path())).unwrap();
        wait_for_file(&dir.path().join("ready"), Duration::from_secs(5)).await;

        let started = Instant::now();
        let status = process.terminate(Duration::from_millis(300)).await.unwrap();
        assert!(started.elapsed() < Duration::from_secs(3));
        assert_eq!(describe_exit(&status), "signal 9 (SIGKILL)");
        assert_eq!(exit_code(&status), 137);
    }

    #[tokio::test]
    async fn terminate_cleans_up_leftovers_of_exited_processes() {
        let dir = tempfile::tempdir().unwrap();
        let script = "sleep 30 & echo $! > child.pid";
        let mut process =
            spawn(&CommandLine::Shell(script.into()), &options_in(dir.path())).unwrap();
        assert!(process.wait().await.unwrap().success());
        let pid: i32 = wait_for_file(&dir.path().join("child.pid"), Duration::from_secs(5))
            .await
            .trim()
            .parse()
            .unwrap();
        assert!(is_alive(pid), "background child should outlive the shell");

        process.terminate(Duration::from_secs(2)).await.unwrap();
        wait_until_dead(pid).await;
    }

    #[tokio::test]
    async fn dropping_a_process_kills_its_tree() {
        let dir = tempfile::tempdir().unwrap();
        let script = "sleep 30 & echo $! > child.pid; wait";
        let process = spawn(&CommandLine::Shell(script.into()), &options_in(dir.path())).unwrap();
        let pid: i32 = wait_for_file(&dir.path().join("child.pid"), Duration::from_secs(5))
            .await
            .trim()
            .parse()
            .unwrap();
        drop(process);
        wait_until_dead(pid).await;
    }
}

#[test]
fn run_command_captures_output() {
    let (program, args): (&str, Vec<&str>) = if cfg!(windows) {
        ("cmd", vec!["/C", "echo hello"])
    } else {
        ("sh", vec!["-c", "echo hello; echo oops >&2; exit 4"])
    };
    let out = anymon_shell::run_command(program, &args).unwrap();
    assert!(out.stdout.contains("hello"));
    if !cfg!(windows) {
        assert_eq!(out.stderr.trim(), "oops");
        assert_eq!(out.status, 4);
    }
}
