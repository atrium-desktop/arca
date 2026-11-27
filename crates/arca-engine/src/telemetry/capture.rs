//! Stall stack capture: when the watchdog raises `ui_stall`, save every
//! thread's backtrace to `<state>/stalls/` while the process is still stuck.
//!
//! A frozen window is only diagnosable while it is frozen, and attaching a
//! debugger by hand is often impossible: Yama `ptrace_scope=1` (the common
//! distribution default) only lets a process be traced by its *ancestors*.
//! The capture inverts that. Arca spawns the debugger itself and grants that
//! one child permission with `prctl(PR_SET_PTRACER, child_pid)`:
//!
//! 1. spawn `sh -c 'read _ && exec <tool> …'`: the helper blocks on stdin, so
//!    it cannot attach before it is allowed to;
//! 2. `prctl(PR_SET_PTRACER, helper_pid)`: exactly that pid may attach;
//! 3. release the helper (newline on stdin); it `exec`s the tool under the
//!    same pid, which attaches, prints all thread stacks, and detaches;
//! 4. revoke the grant (`PR_SET_PTRACER, 0`), bound the wait with a timeout,
//!    prune old captures, log the path.
//!
//! The tool is `gdb` (Rust- and C-aware frames, source lines), falling back to
//! elfutils `eu-stack`. It runs with no user init files and debuginfod
//! disabled, so a capture never runs user scripts or blocks on the network.
//!
//! Policy ([`super::Options::stall_capture`]): on by default in debug builds,
//! opt-in for release builds (`ARCA_STALL_CAPTURE=1`), one capture at a time, at most one per
//! [`Config::min_interval`], and skipped when Arca is already being traced
//! (a developer's debugger owns the process). Linux only; elsewhere the
//! capturer is inert.

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use super::Role;
use super::alerts::{Alert, AlertKind};

/// File-name prefix of a capture.
pub const FILE_PREFIX: &str = "stall-";

/// Capture policy.
#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    /// `<state>/stalls`.
    pub dir: PathBuf,
    pub role: Role,
    /// Captures kept on disk; older ones are deleted.
    pub keep: usize,
    /// Minimum time between captures (one freeze raises one capture).
    pub min_interval: Duration,
    /// Upper bound on one capture; the helper is killed past it.
    pub timeout: Duration,
}

impl Config {
    /// Defaults for `role`, capturing into `<state_dir>/stalls`. Whether to
    /// capture at all is [`super::Options::stall_capture`].
    #[must_use]
    pub fn new(state_dir: &Path, role: Role) -> Config {
        Config {
            dir: dir_for(state_dir),
            role,
            keep: 20,
            min_interval: Duration::from_secs(60),
            timeout: Duration::from_secs(60),
        }
    }
}

/// `<state_dir>/stalls`.
#[must_use]
pub fn dir_for(state_dir: &Path) -> PathBuf {
    state_dir.join("stalls")
}

/// A debugger able to print another process's stacks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Tool {
    Gdb(PathBuf),
    EuStack(PathBuf),
}

impl Tool {
    /// The first of `gdb`, `eu-stack` found on `PATH`.
    #[must_use]
    pub fn find() -> Option<Tool> {
        find_on_path("gdb")
            .map(Tool::Gdb)
            .or_else(|| find_on_path("eu-stack").map(Tool::EuStack))
    }

    fn program(&self) -> &Path {
        match self {
            Tool::Gdb(path) | Tool::EuStack(path) => path,
        }
    }

    /// Arguments that dump every thread of `pid` and detach.
    #[must_use]
    pub fn args(&self, pid: u32) -> Vec<String> {
        match self {
            Tool::Gdb(_) => [
                "-p",
                &pid.to_string(),
                "-batch",
                "-nx",
                "-iex",
                "set debuginfod enabled off",
                "-iex",
                "set auto-load off",
                // Before the attach, or every thread prints "[New LWP …]".
                "-iex",
                "set print thread-events off",
                "-iex",
                "set pagination off",
                "-ex",
                "info threads",
                "-ex",
                "thread apply all bt 64",
            ]
            .map(String::from)
            .to_vec(),
            Tool::EuStack(_) => ["-p", &pid.to_string(), "-s", "-i"]
                .map(String::from)
                .to_vec(),
        }
    }
}

fn find_on_path(name: &str) -> Option<PathBuf> {
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|dir| dir.join(name))
        .find(|path| {
            use std::os::unix::fs::PermissionsExt;
            path.metadata()
                .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        })
}

/// Why a capture did not run.
#[derive(Debug)]
pub enum Skipped {
    /// Another capture is in progress.
    Busy,
    /// The previous capture was less than `min_interval` ago.
    TooSoon,
    /// Not a `ui_stall`.
    NotAStall,
}

/// Runs one capture; replaced in tests.
pub type CaptureFn = dyn Fn(&Config, &str) -> io::Result<PathBuf> + Send + Sync;

/// Turns `ui_stall` alerts into background captures.
pub struct Capturer {
    config: Config,
    busy: Arc<AtomicBool>,
    last: Mutex<Option<Instant>>,
    run: Arc<CaptureFn>,
}

impl Capturer {
    #[must_use]
    pub fn new(config: Config) -> Capturer {
        Capturer::with_runner(config, Arc::new(capture))
    }

    /// A capturer whose capture step is `run` (tests).
    #[must_use]
    pub fn with_runner(config: Config, run: Arc<CaptureFn>) -> Capturer {
        Capturer {
            config,
            busy: Arc::new(AtomicBool::new(false)),
            last: Mutex::new(None),
            run,
        }
    }

    /// React to one raised alert. Returns the capture thread when one was
    /// started; never blocks the caller (the watchdog must keep sampling to
    /// observe the recovery).
    pub fn on_alert(&self, alert: &Alert) -> Result<std::thread::JoinHandle<()>, Skipped> {
        if alert.kind != AlertKind::UiStall {
            return Err(Skipped::NotAStall);
        }
        {
            let mut last = self
                .last
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if last.is_some_and(|at| at.elapsed() < self.config.min_interval) {
                return Err(Skipped::TooSoon);
            }
            if self.busy.swap(true, Ordering::AcqRel) {
                return Err(Skipped::Busy);
            }
            *last = Some(Instant::now());
        }
        let config = self.config.clone();
        let busy = Arc::clone(&self.busy);
        let run = Arc::clone(&self.run);
        let reason = alert.summary.clone();
        std::thread::Builder::new()
            .name("arca-stall-capture".into())
            .spawn(move || {
                match run(&config, &reason) {
                    Ok(path) => log::warn!(
                        target: "arca::capture",
                        "stall stacks captured to {}",
                        path.display()
                    ),
                    Err(error) => {
                        log::warn!(target: "arca::capture", "stall capture failed: {error}");
                    }
                }
                busy.store(false, Ordering::Release);
            })
            .map_err(|error| {
                self.busy.store(false, Ordering::Release);
                log::warn!(target: "arca::capture", "stall capture thread: {error}");
                Skipped::Busy
            })
    }
}

/// Capture every thread's stack of this process into a new file under
/// `config.dir`, then prune to `config.keep` files. Returns the file.
pub fn capture(config: &Config, reason: &str) -> io::Result<PathBuf> {
    let tracer = tracer_pid();
    if tracer.is_some_and(|pid| pid != 0) {
        return Err(io::Error::other(
            "already being traced (a debugger is attached); not capturing",
        ));
    }
    let tool = Tool::find().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "neither gdb nor eu-stack is on PATH; install gdb to capture stall stacks",
        )
    })?;
    std::fs::create_dir_all(&config.dir)?;
    let pid = std::process::id();
    let path = config
        .dir
        .join(file_name(SystemTime::now(), config.role, pid));
    let mut file = std::fs::File::create(&path)?;
    write_header(&mut file, config.role, pid, reason, &tool)?;
    file.flush()?;

    let status = run_tool(&tool, pid, &file, config.timeout);
    let mut file = std::fs::OpenOptions::new().append(true).open(&path)?;
    match &status {
        Ok(()) => {}
        Err(error) => writeln!(file, "\n[capture incomplete: {error}]")?,
    }
    drop(file);
    prune(&config.dir, config.keep)?;
    status.map(|()| path)
}

/// `stall-<UTC>-<role>-<pid>.txt`; names sort chronologically.
#[must_use]
pub fn file_name(at: SystemTime, role: Role, pid: u32) -> String {
    let stamp = super::format_utc_ms(super::unix_ms(at)).replace(':', "");
    format!("{FILE_PREFIX}{stamp}-{}-{pid}.txt", role.as_str())
}

fn write_header(
    out: &mut impl Write,
    role: Role,
    pid: u32,
    reason: &str,
    tool: &Tool,
) -> io::Result<()> {
    writeln!(out, "Arca {} stall capture", env!("CARGO_PKG_VERSION"))?;
    writeln!(
        out,
        "  at      {}",
        super::format_utc_ms(super::unix_ms(SystemTime::now()))
    )?;
    writeln!(out, "  role    {}  pid {pid}", role.as_str())?;
    writeln!(out, "  reason  {reason}")?;
    writeln!(out, "  tool    {}", tool.program().display())?;
    writeln!(
        out,
        "  ptrace_scope {}",
        std::fs::read_to_string("/proc/sys/kernel/yama/ptrace_scope")
            .map_or_else(|_| "n/a".into(), |s| s.trim().to_string())
    )?;
    writeln!(out, "  exe     {}", std::env::current_exe()?.display())?;
    writeln!(out)
}

/// `TracerPid` from `/proc/self/status` (0 = not traced).
fn tracer_pid() -> Option<u32> {
    std::fs::read_to_string("/proc/self/status")
        .ok()?
        .lines()
        .find_map(|line| line.strip_prefix("TracerPid:"))
        .and_then(|value| value.trim().parse().ok())
}

/// Spawn the gated helper, grant it ptrace, release it, and wait.
fn run_tool(tool: &Tool, pid: u32, out: &std::fs::File, timeout: Duration) -> io::Result<()> {
    let mut command = Command::new("/bin/sh");
    command
        .arg("-c")
        // `read` blocks until the grant is in place; `exec` keeps the pid
        // the grant names.
        .arg(r#"read _ && exec "$0" "$@""#)
        .arg(tool.program())
        .args(tool.args(pid))
        .env("DEBUGINFOD_URLS", "")
        .stdin(Stdio::piped())
        .stdout(out.try_clone()?)
        .stderr(out.try_clone()?);
    let mut child = command.spawn()?;
    let granted = ptracer::allow(child.id());
    let released = child
        .stdin
        .take()
        .map_or(Ok(()), |mut stdin| stdin.write_all(b"\n"));
    let waited = wait_bounded(&mut child, timeout);
    ptracer::revoke();
    if let Err(error) = granted {
        // Not fatal: with ptrace_scope=0 no grant is needed.
        log::debug!(target: "arca::capture", "PR_SET_PTRACER: {error}");
    }
    released?;
    let status = waited?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "{} exited with {status}",
            tool.program().display()
        )))
    }
}

fn wait_bounded(child: &mut Child, timeout: Duration) -> io::Result<std::process::ExitStatus> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status);
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                format!("capture exceeded {} s; helper killed", timeout.as_secs()),
            ));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Keep the newest `keep` captures in `dir`.
pub fn prune(dir: &Path, keep: usize) -> io::Result<()> {
    let mut captures = list(dir)?;
    if captures.len() > keep {
        let excess = captures.len() - keep;
        for old in captures.drain(..excess) {
            let _ = std::fs::remove_file(old);
        }
    }
    Ok(())
}

/// Captures in `dir`, oldest first. A missing directory is empty.
pub fn list(dir: &Path) -> io::Result<Vec<PathBuf>> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut captures: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with(FILE_PREFIX) && n.ends_with(".txt"))
        })
        .collect();
    captures.sort();
    Ok(captures)
}

#[cfg(target_os = "linux")]
mod ptracer {
    use std::ffi::{c_int, c_ulong};

    const PR_SET_PTRACER: c_int = 0x5961_6d61;

    unsafe extern "C" {
        fn prctl(option: c_int, ...) -> c_int;
    }

    /// Let exactly `pid` ptrace this process (Yama `ptrace_scope=1`).
    pub fn allow(pid: u32) -> std::io::Result<()> {
        // SAFETY: PR_SET_PTRACER takes one unsigned long; no memory is passed.
        let rc = unsafe { prctl(PR_SET_PTRACER, c_ulong::from(pid)) };
        if rc == 0 {
            Ok(())
        } else {
            Err(std::io::Error::last_os_error())
        }
    }

    /// Withdraw any ptracer grant.
    pub fn revoke() {
        // SAFETY: as above; 0 clears the exception.
        let _ = unsafe { prctl(PR_SET_PTRACER, 0 as c_ulong) };
    }
}

#[cfg(not(target_os = "linux"))]
mod ptracer {
    pub fn allow(_pid: u32) -> std::io::Result<()> {
        Err(std::io::Error::from(std::io::ErrorKind::Unsupported))
    }
    pub fn revoke() {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::telemetry::alerts::Severity;
    use test_support::fs::TestDir;

    fn config(dir: &Path) -> Config {
        Config {
            dir: dir.to_path_buf(),
            role: Role::Window,
            keep: 3,
            min_interval: Duration::from_secs(60),
            timeout: Duration::from_secs(60),
        }
    }

    fn stall() -> Alert {
        Alert::new(
            AlertKind::UiStall,
            Severity::Critical,
            "UI unresponsive for 900 ms",
        )
    }

    #[test]
    fn file_names_sort_chronologically_and_carry_role_and_pid() {
        let early = SystemTime::UNIX_EPOCH + Duration::from_millis(1_795_543_252_612);
        let late = early + Duration::from_secs(3600);
        let a = file_name(early, Role::Window, 42);
        let b = file_name(late, Role::Window, 7);
        assert!(a < b, "{a} < {b}");
        assert!(
            a.starts_with(FILE_PREFIX) && a.ends_with("-window-42.txt"),
            "{a}"
        );
        assert!(!a.contains(':'), "portable file name: {a}");
    }

    #[test]
    fn pruning_keeps_the_newest_captures_only() {
        let dir = TestDir::new("arca-capture-prune");
        for minute in 0..6 {
            std::fs::write(
                dir.path().join(format!(
                    "{FILE_PREFIX}2026-11-25T01{minute:02}00.000Z-window-1.txt"
                )),
                "x",
            )
            .unwrap();
        }
        std::fs::write(dir.path().join("unrelated.txt"), "keep me").unwrap();
        prune(dir.path(), 3).unwrap();
        let left: Vec<String> = list(dir.path())
            .unwrap()
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(left.len(), 3);
        assert!(left[0].contains("T010300"), "{left:?}");
        assert!(dir.path().join("unrelated.txt").exists());
        assert!(list(&dir.path().join("missing")).unwrap().is_empty());
    }

    #[test]
    fn gdb_and_eu_stack_get_non_interactive_offline_arguments() {
        let gdb = Tool::Gdb("/usr/bin/gdb".into()).args(99);
        assert!(gdb.windows(2).any(|w| w == ["-p", "99"]));
        for required in [
            "-batch",
            "-nx",
            "set debuginfod enabled off",
            "thread apply all bt 64",
        ] {
            assert!(gdb.iter().any(|a| a == required), "{required}: {gdb:?}");
        }
        assert_eq!(
            Tool::EuStack("/usr/bin/eu-stack".into()).args(5),
            ["-p", "5", "-s", "-i"]
        );
    }

    fn counting_capturer(config: Config, delay: Duration) -> (Capturer, Arc<Mutex<Vec<String>>>) {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let record = Arc::clone(&seen);
        let capturer = Capturer::with_runner(
            config,
            Arc::new(move |_: &Config, reason: &str| {
                std::thread::sleep(delay);
                record.lock().unwrap().push(reason.to_string());
                Ok(PathBuf::from("/dev/null"))
            }),
        );
        (capturer, seen)
    }

    #[test]
    fn only_stalls_are_captured() {
        let dir = TestDir::new("arca-capture-policy");
        let (capturer, seen) = counting_capturer(config(dir.path()), Duration::ZERO);
        let storm = Alert::new(AlertKind::FrameStorm, Severity::Critical, "spin");
        assert!(matches!(capturer.on_alert(&storm), Err(Skipped::NotAStall)));
        capturer.on_alert(&stall()).unwrap().join().unwrap();
        assert_eq!(
            seen.lock().unwrap().as_slice(),
            ["UI unresponsive for 900 ms"]
        );
    }

    #[test]
    fn captures_are_rate_limited_and_never_overlap() {
        let dir = TestDir::new("arca-capture-rate");
        let quick = Config {
            min_interval: Duration::ZERO,
            ..config(dir.path())
        };
        let (capturer, seen) = counting_capturer(quick, Duration::from_millis(200));
        let first = capturer.on_alert(&stall()).unwrap();
        assert!(matches!(capturer.on_alert(&stall()), Err(Skipped::Busy)));
        first.join().unwrap();
        capturer.on_alert(&stall()).unwrap().join().unwrap();
        assert_eq!(seen.lock().unwrap().len(), 2, "free again once finished");

        let (capturer, _) = counting_capturer(config(dir.path()), Duration::ZERO);
        capturer.on_alert(&stall()).unwrap().join().unwrap();
        assert!(matches!(capturer.on_alert(&stall()), Err(Skipped::TooSoon)));
    }

    /// The real mechanism against this test process: a helper spawned by us,
    /// granted ptrace for its pid only, dumps a thread we park in a named
    /// function. Exercises the `ptrace_scope=1` path end to end. Skips when
    /// no tool is installed or the kernel forbids attaching outright.
    #[test]
    fn a_real_capture_dumps_every_thread_of_this_process() {
        let scope = std::fs::read_to_string("/proc/sys/kernel/yama/ptrace_scope")
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        if Tool::find().is_none() || scope == "2" || scope == "3" || tracer_pid() != Some(0) {
            eprintln!("skipping: no gdb/eu-stack, ptrace_scope={scope}, or already traced");
            return;
        }
        #[inline(never)]
        fn arca_capture_test_parked_here(stop: &AtomicBool) {
            while !stop.load(Ordering::Relaxed) {
                std::thread::park_timeout(Duration::from_millis(20));
            }
        }
        let stop = Arc::new(AtomicBool::new(false));
        let parked = {
            let stop = Arc::clone(&stop);
            std::thread::Builder::new()
                .name("capture-target".into())
                .spawn(move || arca_capture_test_parked_here(&stop))
                .unwrap()
        };
        let dir = TestDir::new("arca-capture-real");
        let result = capture(&config(dir.path()), "test stall");
        stop.store(true, Ordering::Relaxed);
        parked.join().unwrap();
        let path = result.expect("capture succeeds");
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("reason  test stall"), "{text}");
        assert!(
            text.contains("arca_capture_test_parked_here"),
            "the parked thread's frame is in the dump:\n{text}"
        );
        assert_eq!(tracer_pid(), Some(0), "the helper detached");
    }
}
