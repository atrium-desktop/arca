//! Diagnostics snapshots: the bug-report artifact.
//!
//! The window process rewrites `<state>/diagnostics-window.json` every
//! [`WRITE_INTERVAL`] and once more on exit, atomically (temp file + rename),
//! so a reader never sees a torn file — even when the window is frozen and
//! the snapshot is being read from a second process. `arca --diagnostics`
//! renders it with [`render_text`].

use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

use super::{Role, alerts, metrics};

/// Schema version of [`Report`]; bump on incompatible changes.
pub const SCHEMA: u32 = 1;
/// How often the window refreshes its snapshot.
pub const WRITE_INTERVAL: Duration = Duration::from_secs(10);

/// One diagnostics snapshot.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Report {
    pub schema: u32,
    pub version: String,
    pub role: String,
    pub pid: u32,
    pub started_unix_ms: u64,
    pub written_unix_ms: u64,
    pub log_file: Option<String>,
    pub metrics: metrics::Snapshot,
    pub alerts: Vec<alerts::Alert>,
}

/// `<state_dir>/diagnostics-<role>.json`.
#[must_use]
pub fn path_for(state_dir: &Path, role: Role) -> PathBuf {
    state_dir.join(format!("diagnostics-{}.json", role.as_str()))
}

/// Capture the process-wide metrics and alerts.
#[must_use]
pub fn capture(role: Role, started: SystemTime, log_file: Option<&Path>) -> Report {
    Report {
        schema: SCHEMA,
        version: env!("CARGO_PKG_VERSION").to_string(),
        role: role.as_str().to_string(),
        pid: std::process::id(),
        started_unix_ms: super::unix_ms(started),
        written_unix_ms: super::unix_ms(SystemTime::now()),
        log_file: log_file.map(|p| p.display().to_string()),
        metrics: metrics::global().snapshot(),
        alerts: alerts::global().recent(),
    }
}

/// Write `report` to `path` atomically.
pub fn write(path: &Path, report: &Report) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_vec_pretty(report).map_err(io::Error::other)?;
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(format!(".{}.tmp", std::process::id()));
    let tmp = PathBuf::from(tmp);
    std::fs::write(&tmp, json)?;
    std::fs::rename(&tmp, path)
}

/// Read a snapshot.
pub fn read(path: &Path) -> io::Result<Report> {
    let bytes = std::fs::read(path)?;
    serde_json::from_slice(&bytes).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

/// Whether the process that wrote `report` is still running (Linux `/proc`).
#[must_use]
pub fn writer_alive(report: &Report) -> bool {
    Path::new(&format!("/proc/{}", report.pid)).exists()
}

/// Histograms shown first in the text report, in this order.
const KEY_HISTOGRAMS: [&str; 6] = [
    "ui.frame",
    "nav.latency",
    "fs.read_dir",
    "thumb.decode",
    "io.job",
    "ui.stall",
];

/// Human-readable rendering of `report`.
#[must_use]
pub fn render_text(report: &Report, alive: bool) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    let uptime = report
        .written_unix_ms
        .saturating_sub(report.started_unix_ms)
        / 1000;
    let _ = writeln!(out, "Arca {} diagnostics ({})", report.version, report.role);
    let _ = writeln!(
        out,
        "  pid {} ({}), snapshot {}, uptime {}s",
        report.pid,
        if alive { "running" } else { "exited" },
        super::format_utc_ms(report.written_unix_ms),
        uptime
    );
    if let Some(log) = &report.log_file {
        let _ = writeln!(out, "  log  {log}");
    }

    let _ = writeln!(
        out,
        "\nLatency (ms)          count     p50     p95     p99     max"
    );
    let mut names: Vec<&String> = KEY_HISTOGRAMS
        .iter()
        .filter_map(|name| {
            report
                .metrics
                .histograms
                .get_key_value(*name)
                .map(|(k, _)| k)
        })
        .collect();
    names.extend(
        report
            .metrics
            .histograms
            .keys()
            .filter(|name| !KEY_HISTOGRAMS.contains(&name.as_str())),
    );
    if names.is_empty() {
        let _ = writeln!(out, "  (no samples yet)");
    }
    for name in names {
        let h = &report.metrics.histograms[name];
        let _ = writeln!(
            out,
            "  {name:<18} {:>8} {:>7.1} {:>7.1} {:>7.1} {:>7.1}",
            h.count, h.p50_ms, h.p95_ms, h.p99_ms, h.max_ms
        );
    }

    if !report.metrics.counters.is_empty() || !report.metrics.gauges.is_empty() {
        let _ = writeln!(out, "\nCounters");
        for (name, value) in &report.metrics.counters {
            let _ = writeln!(out, "  {name:<28} {value}");
        }
        for (name, value) in &report.metrics.gauges {
            let _ = writeln!(out, "  {name:<28} {value} (gauge)");
        }
    }

    let _ = writeln!(out, "\nRecent alerts");
    if report.alerts.is_empty() {
        let _ = writeln!(out, "  none");
    }
    for alert in report.alerts.iter().rev().take(20) {
        let _ = writeln!(
            out,
            "  {} {:<8} {:<18} {}",
            super::format_utc_ms(alert.at_unix_ms),
            format!("{:?}", alert.severity).to_lowercase(),
            alert.kind.as_str(),
            alert.summary
        );
    }
    out
}

/// Background snapshot writer; writes a final snapshot on drop.
pub struct Writer {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
    path: PathBuf,
    role: Role,
    started: SystemTime,
    log_file: Option<PathBuf>,
}

impl Writer {
    #[must_use]
    pub fn start(
        path: PathBuf,
        role: Role,
        started: SystemTime,
        log_file: Option<PathBuf>,
        interval: Duration,
    ) -> Writer {
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);
        let thread_path = path.clone();
        let thread_log = log_file.clone();
        let handle = std::thread::Builder::new()
            .name("arca-diagnostics".into())
            .spawn(move || {
                let tick = Duration::from_millis(100).min(interval);
                let mut since_write = interval; // write promptly at startup
                while !thread_stop.load(Ordering::Relaxed) {
                    if since_write >= interval {
                        since_write = Duration::ZERO;
                        let report = capture(role, started, thread_log.as_deref());
                        if let Err(error) = write(&thread_path, &report) {
                            log::warn!(
                                target: "arca::diagnostics",
                                "cannot write {}: {error}",
                                thread_path.display()
                            );
                        }
                    }
                    std::thread::sleep(tick);
                    since_write += tick;
                }
            })
            .ok();
        Writer {
            stop,
            handle,
            path,
            role,
            started,
            log_file,
        }
    }
}

impl Drop for Writer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
        let report = capture(self.role, self.started, self.log_file.as_deref());
        let _ = write(&self.path, &report);
        log::logger().flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_support::fs::TestDir;

    #[test]
    fn reports_round_trip_through_an_atomic_write() {
        let dir = TestDir::new("arca-diag-roundtrip");
        metrics::counter("test.diagnostics.roundtrip", 1);
        let report = capture(
            Role::Window,
            SystemTime::now(),
            Some(Path::new("/tmp/a.log")),
        );
        let path = path_for(dir.path(), Role::Window);
        write(&path, &report).unwrap();
        assert_eq!(path.file_name().unwrap(), "diagnostics-window.json");
        let back = read(&path).unwrap();
        assert_eq!(back, report);
        assert!(
            back.metrics
                .counters
                .contains_key("test.diagnostics.roundtrip")
        );
        assert_eq!(dir.entry_count(), 1, "no temp file is left behind");
        assert!(writer_alive(&back), "this process wrote it");
    }

    #[test]
    fn a_corrupt_snapshot_is_invalid_data() {
        let dir = TestDir::new("arca-diag-corrupt");
        let path = dir.write("diagnostics-window.json", b"{not json");
        assert_eq!(read(&path).unwrap_err().kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn text_rendering_lists_latency_counters_and_alerts() {
        let mut snapshot = metrics::Snapshot::default();
        let registry = metrics::Registry::new();
        registry.record("ui.frame", Duration::from_millis(4));
        registry.record("zz.custom", Duration::from_millis(1));
        registry.add("nav.started", 3);
        snapshot.histograms = registry.snapshot().histograms;
        snapshot.counters = registry.snapshot().counters;
        let report = Report {
            schema: SCHEMA,
            version: "9.9.9".into(),
            role: "window".into(),
            pid: 42,
            started_unix_ms: 0,
            written_unix_ms: 61_000,
            log_file: Some("/x/arca.log".into()),
            metrics: snapshot,
            alerts: vec![alerts::Alert {
                kind: alerts::AlertKind::UiStall,
                severity: alerts::Severity::Critical,
                summary: "UI unresponsive for 900 ms in fs.read_dir".into(),
                at_unix_ms: 60_000,
            }],
        };
        let text = render_text(&report, false);
        assert!(
            text.starts_with("Arca 9.9.9 diagnostics (window)"),
            "{text}"
        );
        assert!(
            text.contains("pid 42 (exited)") && text.contains("uptime 61s"),
            "{text}"
        );
        assert!(text.contains("log  /x/arca.log"));
        let frame = text.find("ui.frame").unwrap();
        let custom = text.find("zz.custom").unwrap();
        assert!(frame < custom, "key histograms come first:\n{text}");
        assert!(text.contains("nav.started") && text.contains(" 3"));
        assert!(
            text.contains("critical") && text.contains("ui_stall"),
            "{text}"
        );
    }

    #[test]
    fn the_writer_snapshots_promptly_and_on_drop() {
        let dir = TestDir::new("arca-diag-writer");
        let path = path_for(dir.path(), Role::Chooser);
        let writer = Writer::start(
            path.clone(),
            Role::Chooser,
            SystemTime::now(),
            None,
            Duration::from_secs(3600),
        );
        assert!(test_support::poll::wait_until(|| path.exists()));
        let first = read(&path).unwrap().written_unix_ms;
        std::thread::sleep(Duration::from_millis(5));
        drop(writer);
        let last = read(&path).unwrap();
        assert_eq!(last.role, "chooser");
        assert!(last.written_unix_ms >= first, "drop wrote a final snapshot");
    }
}
