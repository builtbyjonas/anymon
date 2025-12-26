use anyhow::Result;
use colored::Colorize;
use globset::GlobSet;
use notify::Event;
use notify::Watcher;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Clone)]
pub struct TaskSpec {
    pub name: String,
    pub run: String,
    pub restart: bool,
    pub globset: GlobSet,
    pub roots: Vec<PathBuf>,
}

pub fn pref() -> String {
    "[anymon]".cyan().bold().to_string()
}

pub fn pref_task(name: &str) -> String {
    let left: colored::ColoredString = "[anymon]".cyan().bold();
    let right: colored::ColoredString = format!("[{}]", name).black().bold();
    format!("{} {}:", left, right)
}

pub fn try_spawn(cmd: &str) -> std::io::Result<tokio::process::Child> {
    use std::path::{Path, PathBuf};

    fn find_executable(name: &str) -> Option<PathBuf> {
        // If name already contains a path separator, treat it as a path and
        // return it if it exists and is a file.
        let p = Path::new(name);
        if p.components().count() > 1 || name.contains(std::path::MAIN_SEPARATOR) {
            if p.exists() {
                return Some(p.to_path_buf());
            }
            return None;
        }

        let paths = std::env::var_os("PATH")?;
        #[cfg(windows)]
        let splits = std::env::split_paths(&paths);
        #[cfg(not(windows))]
        let splits = std::env::split_paths(&paths);

        #[cfg(windows)]
        let pathext = std::env::var_os("PATHEXT").unwrap_or_else(|| ".EXE;.CMD;.BAT;.COM".into());
        #[cfg(windows)]
        let exts: Vec<String> = pathext
            .to_string_lossy()
            .split(';')
            .map(|s| s.to_string())
            .collect();

        for dir in splits {
            #[cfg(windows)]
            {
                for ext in &exts {
                    let candidate = dir.join(format!("{}{}", name, ext));
                    if candidate.exists() {
                        return Some(candidate);
                    }
                }
            }
            #[cfg(not(windows))]
            {
                let candidate = dir.join(name);
                if candidate.exists() {
                    return Some(candidate);
                }
            }
        }
        None
    }

    let mut parts = cmd.split_whitespace();
    if let Some(prog) = parts.next() {
        let args: Vec<&str> = parts.collect();

        // First try to spawn directly (this will search PATH on most OSes),
        // but if it fails with NotFound, try to resolve via PATH/PATHEXT
        // explicitly (handles cases where extension is needed on Windows).
        match tokio::process::Command::new(prog).args(&args).spawn() {
            Ok(child) => return Ok(child),
            Err(e) => {
                if e.kind() != std::io::ErrorKind::NotFound {
                    return Err(e);
                }
            }
        }

        if let Some(found) = find_executable(prog) {
            return tokio::process::Command::new(found).args(&args).spawn();
        }

        // As a last resort, fall back to invoking a platform shell to handle
        // builtins and shell-specific syntax (quotes, pipes, etc.). This is
        // only used when the program cannot be resolved as an executable.
        if cfg!(windows) {
            return tokio::process::Command::new("powershell")
                .args(["-NoProfile", "-Command", cmd])
                .spawn();
        } else {
            return tokio::process::Command::new("sh")
                .arg("-c")
                .arg(cmd)
                .spawn();
        }
    }

    Err(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        "program not found",
    ))
}

pub async fn run_once(cmd: &str, _kill_timeout: u64) -> Result<()> {
    let mut child = match try_spawn(cmd) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{} process spawn failed: {}", pref(), e);
            return Ok(());
        }
    };
    let status = child.wait().await?;
    println!("{} process exited: {}", pref(), status);
    Ok(())
}

pub async fn watch_mode(
    cfg: anymon_config::Config,
    watch: Option<Vec<String>>,
    debounce_ms: u64,
    kill_timeout: u64,
) -> Result<()> {
    use globset::GlobSetBuilder;
    use tokio::sync::{broadcast, Mutex};

    let mut debounce_ms = debounce_ms;
    if let Some(g) = &cfg.global {
        if let Some(d) = g.debounce {
            debounce_ms = d;
        }
    }

    let tasks = cfg.task.unwrap_or_default();
    if tasks.is_empty() {
        eprintln!("{} no tasks defined in config", pref());
        return Ok(());
    }

    // Determine watch roots (either provided, or config file dir, or current dir)
    let roots: Vec<PathBuf> = if let Some(w) = watch.clone() {
        w.into_iter()
            .map(PathBuf::from)
            .map(|p| {
                if p.is_relative() {
                    std::env::current_dir().unwrap().join(p)
                } else {
                    p
                }
            })
            .collect()
    } else {
        vec![std::env::current_dir()?]
    };

    let mut specs = Vec::new();
    for t in tasks.iter() {
        let mut builder = GlobSetBuilder::new();
        for pat in &t.watch {
            if let Ok(g) = globset::Glob::new(pat) {
                let _ = builder.add(g);
            }
            for root in roots.iter() {
                let combined = root.join(pat).to_string_lossy().replace('\\', "/");
                if let Ok(g2) = globset::Glob::new(&combined) {
                    let _ = builder.add(g2);
                }
            }
        }

        let globset = builder
            .build()
            .unwrap_or_else(|_| globset::GlobSet::empty());
        specs.push(TaskSpec {
            name: t.name.clone(),
            run: t.run.clone(),
            restart: t.restart.unwrap_or(true),
            globset,
            roots: roots.clone(),
        });
    }

    // Build ignore globset from global.ignore (resolve against roots too)
    use globset::GlobSetBuilder as GSBuilder;
    let mut ignore_builder = GSBuilder::new();
    if let Some(g) = &cfg.global {
        if let Some(ignore_patterns) = &g.ignore {
            for pat in ignore_patterns.iter() {
                if let Ok(gp) = globset::Glob::new(pat) {
                    let _ = ignore_builder.add(gp);
                }
                for root in roots.iter() {
                    let combined = root.join(pat).to_string_lossy().replace('\\', "/");
                    if let Ok(gp2) = globset::Glob::new(&combined) {
                        let _ = ignore_builder.add(gp2);
                    }
                }
            }
        }
    }
    let ignore_globset = ignore_builder
        .build()
        .unwrap_or_else(|_| globset::GlobSet::empty());

    // Broadcast channel for filesystem events
    let (tx, _rx) = broadcast::channel::<PathBuf>(1024);
    // Broadcast channel for control commands typed on stdin (e.g., rs, restart, quit, status)
    let (ctrl_tx, _ctrl_rx) = tokio::sync::broadcast::channel::<String>(32);
    // oneshot to signal watch_mode shutdown from stdin
    let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    // wrap tx and ignore set in Arcs to move into closure
    let tx_arc = tx.clone();
    let ignore_arc = Arc::new(ignore_globset);

    let mut watcher: notify::RecommendedWatcher =
        notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
            match res {
                Ok(event) => {
                    for path in event.paths {
                        // skip ignored paths early
                        if !ignore_arc.is_empty() && ignore_arc.is_match(&path) {
                            continue;
                        }
                        let _ = tx_arc.send(path);
                    }
                }
                Err(e) => eprintln!("{} watch error: {e}", pref()),
            }
        })?;

    for root in roots.iter() {
        watcher.watch(root, notify::RecursiveMode::Recursive)?;
        println!("{} watching: {}", pref(), root.display());
    }

    // Task state holders
    let mut handles = Vec::new();

    for spec in specs.into_iter() {
        let rx = tx.subscribe();
        let spec = Arc::new(spec);

        // Attempt to start the configured task once at startup. If spawning fails,
        // the task loop will still try to start it on subsequent file changes.
        let initial_child = match try_spawn(&spec.run) {
            Ok(child) => {
                println!("{} starting: {}", pref_task(&spec.name), spec.run);
                Some(child)
            }
            Err(e) => {
                eprintln!("{} failed to spawn: {}", pref_task(&spec.name), e);
                None
            }
        };

        let handle = tokio::spawn(run_task_loop(
            spec.clone(),
            Mutex::new(initial_child),
            rx,
            ctrl_tx.subscribe(),
            debounce_ms,
            kill_timeout,
        ));
        handles.push(handle);
    }

    // Spawn a task to read stdin lines and broadcast control commands. Keep
    // the JoinHandle so we can abort it during shutdown to avoid hanging on
    // blocking stdin reads (notably on Windows).
    let stdin_handle = {
        let ctrl_tx = ctrl_tx.clone();
        let shutdown_tx = shutdown_tx;
        tokio::spawn(async move {
            use tokio::io::{AsyncBufReadExt, BufReader};
            let stdin = BufReader::new(tokio::io::stdin());
            let mut lines = stdin.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let cmd = line.trim().to_lowercase();
                if cmd.is_empty() {
                    continue;
                }
                let _ = ctrl_tx.send(cmd.clone());
                if cmd == "quit" || cmd == "q" || cmd == "exit" {
                    let _ = shutdown_tx.send(());
                    break;
                }
            }
        })
    };

    // Wait for Ctrl-C or stdin-triggered shutdown and then exit
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            println!("{} received Ctrl-C, shutting down", pref());
        }
        _ = &mut shutdown_rx => {
            println!("{} shutdown requested from stdin", pref());
        }
    }

    // Abort stdin reader to ensure it doesn't keep the process alive on some
    // platforms (async stdin reads can block cancellation). Then wait for the
    // task to finish.
    stdin_handle.abort();
    let _ = stdin_handle.await;

    // Note: child processes will be killed by individual task loops on drop if implemented
    for h in handles {
        h.abort();
    }

    Ok(())
}

pub async fn run_task_loop(
    spec: Arc<TaskSpec>,
    child_slot: tokio::sync::Mutex<Option<tokio::process::Child>>,
    mut rx: tokio::sync::broadcast::Receiver<PathBuf>,
    mut ctrl_rx: tokio::sync::broadcast::Receiver<String>,
    debounce_ms: u64,
    kill_timeout: u64,
) {
    use std::time::{Duration, Instant};

    let mut last_event: Instant;
    loop {
        tokio::select! {
            recv = rx.recv() => match recv {
                Ok(path) => {
                    // check globset against path relative to any of the spec roots
                    let mut matched = false;
                    if spec.globset.is_empty() {
                        matched = true;
                    } else {
                        for root in spec.roots.iter() {
                            if let Ok(rel) = path.strip_prefix(root) {
                                if spec.globset.is_match(rel) {
                                    matched = true;
                                    break;
                                }
                            }
                        }
                    }
                    if !matched {
                        continue;
                    }
                    println!("{} change detected: {}", pref_task(&spec.name), path.display());

                    // Wait for a quiet window (debounce). Listen for new events while
                    // sleeping so we can update `last_event` and extend the window.
                    last_event = Instant::now();
                    loop {
                        let sleep = tokio::time::sleep(Duration::from_millis(debounce_ms));
                        tokio::pin!(sleep);
                        tokio::select! {
                            _ = &mut sleep => {
                                if last_event.elapsed() >= Duration::from_millis(debounce_ms) {
                                    break;
                                } else {
                                    continue;
                                }
                            }
                            recv = rx.recv() => match recv {
                                Ok(next_path) => {
                                    // update last_event only for matching paths
                                    let mut matched_next = false;
                                    if spec.globset.is_empty() {
                                        matched_next = true;
                                    } else {
                                        for root in spec.roots.iter() {
                                            if let Ok(rel) = next_path.strip_prefix(root) {
                                                if spec.globset.is_match(rel) {
                                                    matched_next = true;
                                                    break;
                                                }
                                            }
                                        }
                                    }
                                    if matched_next {
                                        last_event = Instant::now();
                                    }
                                    continue;
                                }
                                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                                    eprintln!("{} event lagged by {} messages", pref_task(&spec.name), n);
                                    continue;
                                }
                                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                            }
                        }
                    }

                    // (re)start process
                    if spec.restart {
                        // kill existing if present
                        let mut guard = child_slot.lock().await;
                        if let Some(mut c) = guard.take() {
                            println!("{} stopping existing process...", pref_task(&spec.name));
                            let _ = c.kill().await;
                            // wait for graceful exit with timeout
                            let wait_fut = c.wait();
                            match tokio::time::timeout(Duration::from_millis(kill_timeout), wait_fut).await {
                                Ok(_) => println!("{} stopped", pref_task(&spec.name)),
                                Err(_) => println!("{} kill timeout exceeded", pref_task(&spec.name)),
                            }
                        }

                        println!("{} starting: {}", pref_task(&spec.name), spec.run);
                        match try_spawn(&spec.run) {
                            Ok(child) => {
                                *guard = Some(child);
                            }
                            Err(e) => eprintln!("{} failed to spawn: {}", pref_task(&spec.name), e),
                        }
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    eprintln!("{} event lagged by {} messages", pref_task(&spec.name), n);
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            },
            ctrl = ctrl_rx.recv() => match ctrl {
                Ok(cmd) => {
                    match cmd.as_str() {
                        "rs" | "restart" => {
                            let mut guard = child_slot.lock().await;
                            if let Some(mut c) = guard.take() {
                                println!("{} restarting (stop)...", pref_task(&spec.name));
                                let _ = c.kill().await;
                            }
                            match try_spawn(&spec.run) {
                                Ok(child) => {
                                    *guard = Some(child);
                                    println!("{} restarted", pref_task(&spec.name));
                                }
                                Err(e) => eprintln!("{} restart failed: {}", pref_task(&spec.name), e),
                            }
                        }
                        "status" => {
                            let guard = child_slot.lock().await;
                            if guard.is_some() {
                                println!("{} status: running", pref_task(&spec.name));
                            } else {
                                println!("{} status: stopped", pref_task(&spec.name));
                            }
                        }
                        "quit" | "q" | "exit" => {
                            println!("{} quitting task loop", pref_task(&spec.name));
                            break;
                        }
                        _ => {}
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    eprintln!("{} ctrl channel lagged by {}", pref_task(&spec.name), n);
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    }
}
