//! Spawning commands and managing the lifetime of their process trees.

use std::ffi::OsString;
use std::io;
use std::path::PathBuf;
use std::process::{ExitStatus, Stdio};
use std::time::{Duration, Instant};

use crate::{CommandLine, Shell};

/// Options for [`spawn`].
#[derive(Debug, Clone)]
pub struct SpawnOptions {
    /// Shell used for shell scripts and as a fallback for unknown programs.
    pub shell: Shell,
    /// Working directory of the process.
    pub cwd: Option<PathBuf>,
    /// Extra environment variables.
    pub env: Vec<(OsString, OsString)>,
    /// Connect the process to anymon's stdin instead of `/dev/null`.
    pub inherit_stdin: bool,
    /// Run the process in its own process group (Unix) or job object
    /// (Windows) so that it can be stopped together with all of its children.
    pub isolate: bool,
}

impl Default for SpawnOptions {
    fn default() -> Self {
        SpawnOptions {
            shell: Shell::platform_default(),
            cwd: None,
            env: Vec::new(),
            inherit_stdin: false,
            isolate: true,
        }
    }
}

/// Spawn a command.
///
/// Direct commands that cannot be found (for example shell builtins such as
/// `echo` on Windows, or `.cmd` shims such as `npm`) are retried through the
/// shell, so any command that works in a terminal works here.
pub fn spawn(command: &CommandLine, options: &SpawnOptions) -> io::Result<Process> {
    match command {
        CommandLine::Shell(script) => spawn_command(options.shell.command(script), options),
        CommandLine::Direct { program, args } => {
            let mut cmd = tokio::process::Command::new(program);
            cmd.args(args);
            match spawn_command(cmd, options) {
                Err(err) if should_fall_back(&err, program) => {
                    #[cfg(windows)]
                    if let Some(resolved) = find_windows_executable(program) {
                        let mut cmd = tokio::process::Command::new(resolved);
                        cmd.args(args);
                        if let Ok(process) = spawn_command(cmd, options) {
                            return Ok(process);
                        }
                    }
                    let mut argv = Vec::with_capacity(args.len() + 1);
                    argv.push(program.clone());
                    argv.extend(args.iter().cloned());
                    let script = options.shell.quote_argv(&argv);
                    spawn_command(options.shell.command(&script), options)
                }
                result => result,
            }
        }
    }
}

fn should_fall_back(err: &io::Error, program: &str) -> bool {
    let is_path = program.contains('/') || (cfg!(windows) && program.contains('\\'));
    !is_path
        && matches!(
            err.kind(),
            io::ErrorKind::NotFound | io::ErrorKind::InvalidInput
        )
}

fn spawn_command(mut cmd: tokio::process::Command, options: &SpawnOptions) -> io::Result<Process> {
    if let Some(cwd) = &options.cwd {
        cmd.current_dir(cwd);
    }
    cmd.envs(options.env.iter().map(|(k, v)| (k, v)));
    cmd.stdin(if options.inherit_stdin {
        Stdio::inherit()
    } else {
        Stdio::null()
    });
    cmd.stdout(Stdio::inherit()).stderr(Stdio::inherit());

    #[cfg(unix)]
    if options.isolate {
        cmd.process_group(0);
    }

    let child = cmd.spawn()?;
    let pid = child.id().unwrap_or(0);

    #[cfg(windows)]
    let job = if options.isolate {
        windows::Job::new()
            .and_then(|job| job.assign(&child).map(|()| job))
            .ok()
    } else {
        None
    };

    Ok(Process {
        child,
        pid,
        status: None,
        isolated: options.isolate,
        group_gone: false,
        #[cfg(windows)]
        job,
    })
}

/// A running (or finished) process started by [`spawn`].
///
/// Dropping a `Process` kills whatever is left of its process tree.
#[derive(Debug)]
pub struct Process {
    child: tokio::process::Child,
    pid: u32,
    status: Option<ExitStatus>,
    isolated: bool,
    /// Set once we know no process of the group is left, so that a recycled
    /// process-group id is never signalled.
    group_gone: bool,
    #[cfg(windows)]
    job: Option<windows::Job>,
}

impl Process {
    /// OS process id of the spawned process.
    pub fn id(&self) -> u32 {
        self.pid
    }

    /// Exit status, if the process has exited and was reaped.
    pub fn status(&self) -> Option<ExitStatus> {
        self.status
    }

    /// Wait for the process to exit. Cancel-safe.
    pub async fn wait(&mut self) -> io::Result<ExitStatus> {
        if let Some(status) = self.status {
            return Ok(status);
        }
        let status = self.child.wait().await?;
        self.record_exit(status);
        Ok(status)
    }

    /// Check whether the process has exited without blocking.
    pub fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        if let Some(status) = self.status {
            return Ok(Some(status));
        }
        let status = self.child.try_wait()?;
        if let Some(status) = status {
            self.record_exit(status);
        }
        Ok(status)
    }

    fn record_exit(&mut self, status: ExitStatus) {
        self.status = Some(status);
        if !self.group_alive() {
            self.group_gone = true;
        }
    }

    /// Stop the process and everything it started.
    ///
    /// On Unix the process group receives `SIGTERM`, and whatever is still
    /// running after `grace` receives `SIGKILL`. On Windows the job object is
    /// terminated immediately. Also cleans up leftover children of a process
    /// that already exited.
    pub async fn terminate(&mut self, grace: Duration) -> io::Result<ExitStatus> {
        let started = Instant::now();
        self.signal(Signal::Terminate);

        if self.status.is_none() {
            match tokio::time::timeout(grace, self.child.wait()).await {
                Ok(status) => self.record_exit(status?),
                Err(_) => {
                    self.signal(Signal::Kill);
                    let status = self.child.wait().await?;
                    self.record_exit(status);
                }
            }
        }

        while self.group_alive() {
            if started.elapsed() >= grace {
                self.signal(Signal::Kill);
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        self.group_gone = true;
        Ok(self.status.expect("status recorded above"))
    }

    /// Kill the process tree immediately without waiting.
    pub fn kill(&mut self) {
        self.signal(Signal::Kill);
    }

    fn group_alive(&self) -> bool {
        if !self.isolated || self.group_gone || self.pid == 0 {
            return false;
        }
        #[cfg(unix)]
        {
            // Signal 0 only checks whether any member of the group exists.
            let rc = unsafe { libc::kill(-(self.pid as libc::pid_t), 0) };
            rc == 0 || io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
        }
        #[cfg(not(unix))]
        {
            false
        }
    }

    fn signal(&mut self, signal: Signal) {
        #[cfg(unix)]
        {
            let signo = match signal {
                Signal::Terminate => libc::SIGTERM,
                Signal::Kill => libc::SIGKILL,
            };
            if self.isolated && !self.group_gone && self.pid != 0 {
                unsafe {
                    libc::kill(-(self.pid as libc::pid_t), signo);
                }
            } else if self.status.is_none() && self.pid != 0 {
                unsafe {
                    libc::kill(self.pid as libc::pid_t, signo);
                }
            }
        }
        #[cfg(windows)]
        {
            let _ = signal;
            if let Some(job) = &self.job {
                job.terminate();
            } else if self.status.is_none() {
                let _ = self.child.start_kill();
            }
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = signal;
            if self.status.is_none() {
                let _ = self.child.start_kill();
            }
        }
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        if self.status.is_none() || self.group_alive() {
            self.kill();
        }
    }
}

#[derive(Clone, Copy)]
enum Signal {
    Terminate,
    Kill,
}

/// Human-readable description of an exit status, e.g. `exit code 1` or
/// `signal 15 (SIGTERM)`.
pub fn describe_exit(status: &ExitStatus) -> String {
    if let Some(code) = status.code() {
        return format!("exit code {code}");
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return match signal_name(signal) {
                Some(name) => format!("signal {signal} ({name})"),
                None => format!("signal {signal}"),
            };
        }
    }
    "unknown exit status".to_string()
}

/// Exit code suitable for passing on to the parent process: the process exit
/// code, or `128 + signal` for processes killed by a signal.
pub fn exit_code(status: &ExitStatus) -> i32 {
    if let Some(code) = status.code() {
        return code;
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return 128 + signal;
        }
    }
    1
}

#[cfg(unix)]
fn signal_name(signal: i32) -> Option<&'static str> {
    Some(match signal {
        libc::SIGHUP => "SIGHUP",
        libc::SIGINT => "SIGINT",
        libc::SIGQUIT => "SIGQUIT",
        libc::SIGABRT => "SIGABRT",
        libc::SIGKILL => "SIGKILL",
        libc::SIGSEGV => "SIGSEGV",
        libc::SIGPIPE => "SIGPIPE",
        libc::SIGTERM => "SIGTERM",
        _ => return None,
    })
}

#[cfg(windows)]
fn find_windows_executable(program: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    let pathext = std::env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".to_string());
    let has_extension = std::path::Path::new(program).extension().is_some();
    for dir in std::env::split_paths(&path) {
        if has_extension {
            let candidate = dir.join(program);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
        for ext in pathext.split(';').filter(|e| !e.is_empty()) {
            let candidate = dir.join(format!("{program}{ext}"));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

#[cfg(windows)]
mod windows {
    use std::io;
    use std::mem::{size_of, zeroed};
    use std::ptr::null;

    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, TerminateJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };

    /// A job object that kills all of its processes when terminated or closed.
    #[derive(Debug)]
    pub struct Job(HANDLE);

    // The handle is only used through thread-safe Win32 calls.
    unsafe impl Send for Job {}
    unsafe impl Sync for Job {}

    impl Job {
        pub fn new() -> io::Result<Job> {
            let handle = unsafe { CreateJobObjectW(null(), null()) };
            if handle.is_null() {
                return Err(io::Error::last_os_error());
            }
            let job = Job(handle);
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { zeroed() };
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let ok = unsafe {
                SetInformationJobObject(
                    job.0,
                    JobObjectExtendedLimitInformation,
                    &info as *const _ as *const core::ffi::c_void,
                    size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                )
            };
            if ok == 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(job)
        }

        pub fn assign(&self, child: &tokio::process::Child) -> io::Result<()> {
            let process = child
                .raw_handle()
                .ok_or_else(|| io::Error::other("process already exited"))?;
            let ok = unsafe { AssignProcessToJobObject(self.0, process as HANDLE) };
            if ok == 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        }

        pub fn terminate(&self) {
            unsafe {
                TerminateJobObject(self.0, 1);
            }
        }
    }

    impl Drop for Job {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
}
