//! Filesystem watcher for active directories.
//!
//! Watches directories for external filesystem mutations (creates, deletes,
//! renames, writes, attribute changes) and notifies the application so that
//! directory listings update in real time.

use std::collections::HashMap;
use std::io;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

pub type WakeFn = Arc<dyn Fn() + Send + Sync + 'static>;

enum WatchCommand {
    SetWatched(Vec<PathBuf>),
    SetWakeFn(Option<WakeFn>),
    Stop,
}

pub struct FsWatcher {
    cmd_tx: Sender<WatchCommand>,
    events_rx: Receiver<PathBuf>,
    #[cfg(target_os = "linux")]
    wakeup_tx: std::os::raw::c_int,
    worker: Option<JoinHandle<()>>,
}

impl FsWatcher {
    /// Start a new filesystem watcher with default settings.
    pub fn new() -> io::Result<Self> {
        Self::with_wake_fn(None)
    }

    /// Start a new filesystem watcher with an optional wake callback.
    pub fn with_wake_fn(wake_fn: Option<WakeFn>) -> io::Result<Self> {
        #[cfg(target_os = "linux")]
        {
            linux_impl::start_watcher(wake_fn)
        }
        #[cfg(not(target_os = "linux"))]
        {
            let (cmd_tx, _cmd_rx) = mpsc::channel();
            let (_events_tx, events_rx) = mpsc::channel();
            Ok(FsWatcher {
                cmd_tx,
                events_rx,
                worker: None,
            })
        }
    }

    /// Update the set of watched directory paths.
    pub fn set_watched_paths(&self, paths: &[PathBuf]) {
        let _ = self.cmd_tx.send(WatchCommand::SetWatched(paths.to_vec()));
        self.kick();
    }

    /// Set or replace the wake callback.
    pub fn set_wake_fn(&self, wake_fn: Option<WakeFn>) {
        let _ = self.cmd_tx.send(WatchCommand::SetWakeFn(wake_fn));
        self.kick();
    }

    /// Drain all changed paths that have settled since the last drain.
    pub fn drain(&self) -> Vec<PathBuf> {
        let mut paths = Vec::new();
        while let Ok(path) = self.events_rx.try_recv() {
            if !paths.contains(&path) {
                paths.push(path);
            }
        }
        paths
    }

    #[cfg(target_os = "linux")]
    fn kick(&self) {
        let byte = [1u8];
        unsafe {
            linux_impl::write(
                self.wakeup_tx,
                byte.as_ptr() as *const std::os::raw::c_void,
                1,
            );
        }
    }

    #[cfg(not(target_os = "linux"))]
    fn kick(&self) {}
}

impl Drop for FsWatcher {
    fn drop(&mut self) {
        let _ = self.cmd_tx.send(WatchCommand::Stop);
        self.kick();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        #[cfg(target_os = "linux")]
        {
            unsafe {
                linux_impl::close(self.wakeup_tx);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Linux inotify implementation
// ---------------------------------------------------------------------------

#[cfg(target_os = "linux")]
mod linux_impl {
    use super::*;
    use std::ffi::CString;
    use std::os::raw::{c_char, c_int, c_void};
    use std::os::unix::ffi::OsStrExt;

    #[repr(C)]
    struct PollFd {
        fd: c_int,
        events: i16,
        revents: i16,
    }

    const POLLIN: i16 = 0x0001;
    const POLLERR: i16 = 0x0008;
    const POLLHUP: i16 = 0x0010;

    const IN_CLOEXEC: c_int = 0x00080000;
    const IN_NONBLOCK: c_int = 0x00000800;

    const IN_MODIFY: u32 = 0x00000002;
    const IN_ATTRIB: u32 = 0x00000004;
    const IN_CLOSE_WRITE: u32 = 0x00000008;
    const IN_MOVED_FROM: u32 = 0x00000040;
    const IN_MOVED_TO: u32 = 0x00000080;
    const IN_CREATE: u32 = 0x00000100;
    const IN_DELETE: u32 = 0x00000200;
    const IN_DELETE_SELF: u32 = 0x00000400;
    const IN_MOVE_SELF: u32 = 0x00000800;
    const IN_IGNORED: u32 = 0x00008000;
    const IN_Q_OVERFLOW: u32 = 0x00004000;

    const WATCH_MASK: u32 = IN_MODIFY
        | IN_ATTRIB
        | IN_CLOSE_WRITE
        | IN_MOVED_FROM
        | IN_MOVED_TO
        | IN_CREATE
        | IN_DELETE
        | IN_DELETE_SELF
        | IN_MOVE_SELF;

    extern "C" {
        fn inotify_init1(flags: c_int) -> c_int;
        fn inotify_add_watch(fd: c_int, pathname: *const c_char, mask: u32) -> c_int;
        fn inotify_rm_watch(fd: c_int, wd: c_int) -> c_int;
        fn poll(fds: *mut PollFd, nfds: usize, timeout: c_int) -> c_int;
        fn pipe(pipefd: *mut c_int) -> c_int;
        pub fn read(fd: c_int, buf: *mut c_void, count: usize) -> isize;
        pub fn write(fd: c_int, buf: *const c_void, count: usize) -> isize;
        pub fn close(fd: c_int) -> c_int;
    }

    struct DirtyEntry {
        earliest: Instant,
        latest: Instant,
    }

    pub fn start_watcher(initial_wake_fn: Option<WakeFn>) -> io::Result<FsWatcher> {
        let inotify_fd = unsafe { inotify_init1(IN_CLOEXEC | IN_NONBLOCK) };
        if inotify_fd < 0 {
            return Err(io::Error::last_os_error());
        }

        let mut pipe_fds = [0 as c_int; 2];
        let rc = unsafe { pipe(pipe_fds.as_mut_ptr()) };
        if rc != 0 {
            unsafe { close(inotify_fd) };
            return Err(io::Error::last_os_error());
        }
        let pipe_read = pipe_fds[0];
        let pipe_write = pipe_fds[1];

        let (cmd_tx, cmd_rx) = mpsc::channel();
        let (events_tx, events_rx) = mpsc::channel();

        let worker = thread::Builder::new()
            .name("arca-fs-watcher".into())
            .spawn(move || {
                run_worker(inotify_fd, pipe_read, cmd_rx, events_tx, initial_wake_fn);
                unsafe {
                    close(inotify_fd);
                    close(pipe_read);
                }
            })
            .inspect_err(|_e| unsafe {
                close(inotify_fd);
                close(pipe_read);
                close(pipe_write);
            })?;

        Ok(FsWatcher {
            cmd_tx,
            events_rx,
            wakeup_tx: pipe_write,
            worker: Some(worker),
        })
    }

    fn run_worker(
        inotify_fd: c_int,
        pipe_read: c_int,
        cmd_rx: Receiver<WatchCommand>,
        events_tx: Sender<PathBuf>,
        mut wake_fn: Option<WakeFn>,
    ) {
        let mut wd_to_path: HashMap<c_int, PathBuf> = HashMap::new();
        let mut path_to_wd: HashMap<PathBuf, c_int> = HashMap::new();
        let mut dirty_paths: HashMap<PathBuf, DirtyEntry> = HashMap::new();
        let mut inotify_buf = [0u8; 8192];
        let mut pipe_buf = [0u8; 128];

        loop {
            // Determine poll timeout based on pending debounced paths
            let timeout_ms = if dirty_paths.is_empty() {
                -1
            } else {
                let now = Instant::now();
                let min_earliest = dirty_paths.values().map(|e| e.earliest).min().unwrap();
                if min_earliest <= now {
                    0
                } else {
                    let diff = min_earliest.duration_since(now).as_millis();
                    (diff as c_int).clamp(1, 1000)
                }
            };

            let mut poll_fds = [
                PollFd {
                    fd: pipe_read,
                    events: POLLIN,
                    revents: 0,
                },
                PollFd {
                    fd: inotify_fd,
                    events: POLLIN,
                    revents: 0,
                },
            ];

            let poll_rc = unsafe { poll(poll_fds.as_mut_ptr(), poll_fds.len(), timeout_ms) };

            // 1. Drain wakeup pipe and process incoming commands
            if poll_rc > 0 && (poll_fds[0].revents & (POLLIN | POLLERR | POLLHUP) != 0) {
                unsafe {
                    read(
                        pipe_read,
                        pipe_buf.as_mut_ptr() as *mut c_void,
                        pipe_buf.len(),
                    );
                }
            }

            let mut stop = false;
            while let Ok(cmd) = cmd_rx.try_recv() {
                match cmd {
                    WatchCommand::SetWatched(new_paths) => {
                        let new_set: HashMap<PathBuf, ()> =
                            new_paths.into_iter().map(|p| (p, ())).collect();

                        // Remove paths no longer in new_set
                        let mut to_remove = Vec::new();
                        for (path, &wd) in &path_to_wd {
                            if !new_set.contains_key(path) {
                                to_remove.push((path.clone(), wd));
                            }
                        }
                        for (path, wd) in to_remove {
                            unsafe {
                                inotify_rm_watch(inotify_fd, wd);
                            }
                            path_to_wd.remove(&path);
                            wd_to_path.remove(&wd);
                            dirty_paths.remove(&path);
                        }

                        // Add new paths not currently watched
                        for path in new_set.keys() {
                            if !path_to_wd.contains_key(path) {
                                if let Ok(c_path) = CString::new(path.as_os_str().as_bytes()) {
                                    let wd = unsafe {
                                        inotify_add_watch(inotify_fd, c_path.as_ptr(), WATCH_MASK)
                                    };
                                    if wd >= 0 {
                                        path_to_wd.insert(path.clone(), wd);
                                        wd_to_path.insert(wd, path.clone());
                                    }
                                }
                            }
                        }
                    }
                    WatchCommand::SetWakeFn(w) => {
                        wake_fn = w;
                    }
                    WatchCommand::Stop => {
                        stop = true;
                    }
                }
            }

            if stop {
                break;
            }

            // 2. Read inotify events
            if poll_rc > 0 && (poll_fds[1].revents & POLLIN != 0) {
                loop {
                    let bytes = unsafe {
                        read(
                            inotify_fd,
                            inotify_buf.as_mut_ptr() as *mut c_void,
                            inotify_buf.len(),
                        )
                    };
                    if bytes <= 0 {
                        break;
                    }

                    let n = bytes as usize;
                    let mut offset = 0;
                    while offset + 16 <= n {
                        let wd =
                            i32::from_ne_bytes(inotify_buf[offset..offset + 4].try_into().unwrap());
                        let mask = u32::from_ne_bytes(
                            inotify_buf[offset + 4..offset + 8].try_into().unwrap(),
                        );
                        let _cookie = u32::from_ne_bytes(
                            inotify_buf[offset + 8..offset + 12].try_into().unwrap(),
                        );
                        let len = u32::from_ne_bytes(
                            inotify_buf[offset + 12..offset + 16].try_into().unwrap(),
                        ) as usize;

                        offset += 16 + len;

                        if mask & IN_Q_OVERFLOW != 0 {
                            // Queue overflow: mark all watched directories dirty
                            let now = Instant::now();
                            for path in path_to_wd.keys() {
                                dirty_paths.insert(
                                    path.clone(),
                                    DirtyEntry {
                                        earliest: now,
                                        latest: now + Duration::from_millis(50),
                                    },
                                );
                            }
                            continue;
                        }

                        if let Some(path) = wd_to_path.get(&wd).cloned() {
                            if mask & IN_IGNORED != 0 {
                                wd_to_path.remove(&wd);
                                path_to_wd.remove(&path);
                            }

                            let now = Instant::now();
                            dirty_paths
                                .entry(path)
                                .and_modify(|entry| {
                                    // Coalesce: push earliest 50ms ahead, capped by latest
                                    entry.earliest =
                                        (now + Duration::from_millis(50)).min(entry.latest);
                                })
                                .or_insert_with(|| DirtyEntry {
                                    earliest: now + Duration::from_millis(50),
                                    latest: now + Duration::from_millis(200),
                                });
                        }
                    }
                }
            }

            // 3. Emit expired dirty paths
            let now = Instant::now();
            let mut ready = Vec::new();
            dirty_paths.retain(|path, entry| {
                if now >= entry.earliest {
                    ready.push(path.clone());
                    false
                } else {
                    true
                }
            });

            if !ready.is_empty() {
                for path in ready {
                    let _ = events_tx.send(path);
                }
                if let Some(wake) = &wake_fn {
                    wake();
                }
            }
        }

        // Cleanup watches before exit
        for (_, wd) in path_to_wd {
            unsafe {
                inotify_rm_watch(inotify_fd, wd);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn watcher_detects_file_creation_and_removal() {
        let dir = std::env::temp_dir().join(format!(
            "arca-watch-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();

        let wake_count = Arc::new(AtomicUsize::new(0));
        let wake_count_clone = Arc::clone(&wake_count);
        let watcher = FsWatcher::with_wake_fn(Some(Arc::new(move || {
            wake_count_clone.fetch_add(1, Ordering::SeqCst);
        })))
        .unwrap();

        watcher.set_watched_paths(std::slice::from_ref(&dir));

        // Give the worker thread a moment to register the watch
        std::thread::sleep(Duration::from_millis(50));

        // Create a file in the watched directory
        let test_file = dir.join("downloaded_file.zip");
        std::fs::write(&test_file, b"test content").unwrap();

        // Wait for debounce period (50ms) + poll tick
        let start = Instant::now();
        let mut drained = Vec::new();
        while start.elapsed() < Duration::from_secs(2) {
            drained = watcher.drain();
            if !drained.is_empty() {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }

        assert!(
            drained.contains(&dir),
            "expected watcher to report directory change"
        );
        assert!(
            wake_count.load(Ordering::SeqCst) > 0,
            "expected wake callback to fire"
        );

        // Remove the file and verify subsequent notification
        std::fs::remove_file(&test_file).unwrap();
        let start = Instant::now();
        let mut drained_after_remove = Vec::new();
        while start.elapsed() < Duration::from_secs(2) {
            drained_after_remove = watcher.drain();
            if !drained_after_remove.is_empty() {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }

        assert!(
            drained_after_remove.contains(&dir),
            "expected watcher to report directory change after file removal"
        );

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
