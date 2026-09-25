//! Registration of file-system watches.

use std::path::PathBuf;
use std::time::Duration;

use anyhow::Result;
use notify::{RecursiveMode, Watcher};
use tokio::sync::mpsc::UnboundedSender;

use crate::ui;

/// Events delivered by the OS watcher.
pub(crate) type FsEvent = notify::Result<notify::Event>;

/// Thin wrapper around the platform watcher.
///
/// On macOS and Windows recursive watching is native and cheap. Elsewhere
/// (inotify, kqueue) every directory needs its own watch, so anymon walks the
/// tree itself and skips ignored directories such as `node_modules` or
/// `target`, which keeps the number of watches (and startup time) low.
///
/// With polling, directories are also registered one by one so that ignored
/// directories are never scanned.
pub(crate) struct FsWatcher {
    inner: Box<dyn Watcher + Send>,
    manual_recursion: bool,
    limit_warned: bool,
}

impl FsWatcher {
    pub fn new(tx: UnboundedSender<FsEvent>, poll: Option<Duration>) -> Result<Self> {
        let handler = move |event: FsEvent| {
            let _ = tx.send(event);
        };
        let inner: Box<dyn Watcher + Send> = match poll {
            Some(interval) => Box::new(notify::PollWatcher::new(
                handler,
                notify::Config::default().with_poll_interval(interval),
            )?),
            None => Box::new(notify::recommended_watcher(handler)?),
        };
        Ok(FsWatcher {
            inner,
            manual_recursion: poll.is_some()
                || !cfg!(any(target_os = "macos", target_os = "windows")),
            limit_warned: false,
        })
    }

    /// Whether directories must be registered one by one.
    pub fn manual_recursion(&self) -> bool {
        self.manual_recursion
    }

    /// Register watches. Returns the number of watches that were added.
    pub fn watch(&mut self, targets: Vec<(PathBuf, RecursiveMode)>) -> usize {
        let mut added = 0;
        let mut errors = Vec::new();
        {
            let mut paths = self.inner.paths_mut();
            for (path, mode) in targets {
                match paths.add(&path, mode) {
                    Ok(()) => added += 1,
                    Err(err) => errors.push((path, err)),
                }
            }
            if let Err(err) = paths.commit() {
                ui::warn(format!("failed to start watching: {err}"));
            }
        }
        for (path, err) in errors {
            self.report(path, err);
        }
        added
    }

    fn report(&mut self, path: PathBuf, err: notify::Error) {
        if is_limit_error(&err) {
            if !self.limit_warned {
                self.limit_warned = true;
                let hint = if cfg!(target_os = "linux") {
                    "; raise it with `sudo sysctl fs.inotify.max_user_watches=524288` or ignore large directories"
                } else {
                    "; ignore large directories to watch fewer files"
                };
                ui::warn(format!("the OS limit for file watches was reached{hint}"));
            }
            return;
        }
        match err.kind {
            // The directory disappeared between listing and watching it.
            notify::ErrorKind::PathNotFound => {}
            _ => ui::detail(format!("cannot watch {}: {err}", path.display())),
        }
    }
}

fn is_limit_error(err: &notify::Error) -> bool {
    match &err.kind {
        notify::ErrorKind::MaxFilesWatch => true,
        notify::ErrorKind::Io(io) => {
            // ENOSPC (inotify watches) and EMFILE (kqueue descriptors).
            matches!(io.raw_os_error(), Some(28) | Some(24))
        }
        _ => false,
    }
}
