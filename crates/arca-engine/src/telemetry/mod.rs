//! Local-first observability for Arca (ADR-0021).
//!
//! Arca is a desktop application, so every signal stays on the user's machine:
//!
//! - **Log management** ([`logging`]): the `log` facade feeds one in-tree
//!   logger that writes a size-rotated file under
//!   `$XDG_STATE_HOME/arca/logs/` and echoes warnings to stderr.
//! - **APM** ([`mod@span`], [`metrics`]): named spans time operations into
//!   latency histograms (p50/p95/p99/max), with counters and gauges beside
//!   them. UI-thread spans over budget are logged and alerted.
//! - **Monitoring and alerting** ([`watchdog`], [`alerts`]): a watchdog thread
//!   watches the UI frame heartbeat for stalls, runaway render loops, and
//!   error bursts, and raises alerts that are logged, kept in a ring buffer,
//!   and surfaced in the status bar.
//! - **Diagnostics** ([`diagnostics`]): a periodic JSON snapshot of metrics
//!   and recent alerts that `arca --diagnostics` renders for bug reports.
//!
//! Every building block is an ordinary value that tests construct directly;
//! [`init`] only wires the process-wide instances together.

pub mod alerts;
#[cfg(unix)]
pub mod capture;
pub mod diagnostics;
pub mod logging;
pub mod metrics;
pub mod span;
pub mod watchdog;

use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub use alerts::{Alert, AlertKind, Severity};
pub use metrics::{Registry, Snapshot, counter, gauge, observe};
pub use span::{Span, span};

/// Which entry surface this process runs; it names the diagnostics file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// The interactive file manager window.
    Window,
    /// The portal prompter (`--prompter`): stdout carries the protocol.
    Prompter,
    /// A standalone picker (`--choose-file`, …).
    Chooser,
}

impl Role {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Window => "window",
            Role::Prompter => "prompter",
            Role::Chooser => "chooser",
        }
    }
}

/// How [`init`] should wire the process.
#[derive(Clone, Debug)]
pub struct Options {
    pub role: Role,
    /// Root for logs and diagnostics; defaults to [`state_dir`].
    pub state_dir: PathBuf,
    /// File log filter spec (`ARCA_LOG`), e.g. `info,arca_engine::watch=debug`.
    pub file_filter: String,
    /// stderr filter spec (`ARCA_LOG_STDERR`).
    pub stderr_filter: String,
    /// Start the UI watchdog thread.
    pub watchdog: bool,
    /// Save all thread stacks to `<state_dir>/stalls/` on `ui_stall`
    /// (`ARCA_STALL_CAPTURE`; default on in debug builds).
    pub stall_capture: bool,
    /// Periodically write `diagnostics-<role>.json`.
    pub diagnostics: bool,
    pub watchdog_config: watchdog::Config,
}

impl Options {
    /// Defaults for `role`, overridden by the documented environment
    /// variables (`ARCA_LOG`, `ARCA_LOG_STDERR`, `ARCA_STALL_MS`).
    #[must_use]
    pub fn from_env(role: Role) -> Options {
        let mut watchdog_config = watchdog::Config::default();
        if let Some(ms) = std::env::var("ARCA_STALL_MS")
            .ok()
            .and_then(|value| value.trim().parse::<u64>().ok())
            .filter(|&ms| ms > 0)
        {
            watchdog_config.stall = Duration::from_millis(ms);
        }
        Options {
            role,
            stall_capture: cfg!(unix)
                && capture_enabled(std::env::var("ARCA_STALL_CAPTURE").ok().as_deref()),
            state_dir: state_dir(),
            file_filter: std::env::var("ARCA_LOG").unwrap_or_else(|_| "info".into()),
            stderr_filter: std::env::var("ARCA_LOG_STDERR").unwrap_or_else(|_| "warn".into()),
            watchdog: true,
            diagnostics: role == Role::Window,
            watchdog_config,
        }
    }
}

/// `ARCA_STALL_CAPTURE` over the build default (on in debug builds).
fn capture_enabled(value: Option<&str>) -> bool {
    match value.map(|v| v.trim().to_ascii_lowercase()) {
        Some(v) if matches!(v.as_str(), "1" | "on" | "true" | "yes") => true,
        Some(v) if matches!(v.as_str(), "0" | "off" | "false" | "no") => false,
        _ => cfg!(debug_assertions),
    }
}

/// Keeps the background telemetry threads alive; dropping it stops the
/// watchdog and writes a final diagnostics snapshot.
pub struct Telemetry {
    pub log_file: Option<PathBuf>,
    _watchdog: Option<watchdog::Watchdog>,
    _diagnostics: Option<diagnostics::Writer>,
}

/// `$XDG_STATE_HOME/arca` — logs and diagnostics live here.
#[must_use]
pub fn state_dir() -> PathBuf {
    crate::xdg::state_home().join("arca")
}

/// Install the process logger and start the background monitors.
///
/// Safe to call more than once: only the first call installs the logger
/// (the `log` facade accepts one), later calls still start monitors.
pub fn init(options: Options) -> Telemetry {
    let started = SystemTime::now();
    let log_dir = options.state_dir.join("logs");
    let (logger, log_file) = logging::Logger::build(
        logging::Filter::parse(&options.file_filter),
        logging::Filter::parse(&options.stderr_filter),
        Some(&log_dir),
    );
    let installed = logging::install(logger);
    if installed {
        log::info!(
            target: "arca",
            "arca {} starting: role={} pid={} log={}",
            env!("CARGO_PKG_VERSION"),
            options.role.as_str(),
            std::process::id(),
            log_file
                .as_ref()
                .map_or_else(|| "stderr only".to_string(), |p| p.display().to_string()),
        );
    }
    #[cfg(unix)]
    let hook: Option<std::sync::Arc<watchdog::AlertHook>> = options.stall_capture.then(|| {
        let capturer =
            capture::Capturer::new(capture::Config::new(&options.state_dir, options.role));
        let hook: std::sync::Arc<watchdog::AlertHook> =
            std::sync::Arc::new(move |alert: &Alert| {
                let _ = capturer.on_alert(alert);
            });
        hook
    });
    #[cfg(not(unix))]
    let hook = None;
    let watchdog = options
        .watchdog
        .then(|| watchdog::Watchdog::start_with_hook(options.watchdog_config.clone(), hook));
    let diagnostics = options.diagnostics.then(|| {
        diagnostics::Writer::start(
            diagnostics::path_for(&options.state_dir, options.role),
            options.role,
            started,
            log_file.clone(),
            diagnostics::WRITE_INTERVAL,
        )
    });
    Telemetry {
        log_file,
        _watchdog: watchdog,
        _diagnostics: diagnostics,
    }
}

/// Milliseconds since the Unix epoch.
#[must_use]
pub fn unix_ms(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// RFC 3339 UTC timestamp with millisecond precision, e.g.
/// `2026-11-25T01:02:03.456Z`. Civil-from-days (H. Hinnant), no time crate.
#[must_use]
pub fn format_utc_ms(unix_ms: u64) -> String {
    let secs = unix_ms / 1000;
    let millis = unix_ms % 1000;
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let (hour, minute, second) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{millis:03}Z")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utc_formatting_matches_known_instants() {
        assert_eq!(format_utc_ms(0), "1970-01-01T00:00:00.000Z");
        // 2000-02-29 leap day.
        assert_eq!(format_utc_ms(951_782_400_123), "2000-02-29T00:00:00.123Z");
        assert_eq!(format_utc_ms(1_795_568_523_456), "2026-11-25T01:02:03.456Z");
    }

    #[test]
    fn the_stall_capture_switch_overrides_the_build_default() {
        assert_eq!(capture_enabled(None), cfg!(debug_assertions));
        for on in ["1", "on", "TRUE", " yes "] {
            assert!(capture_enabled(Some(on)), "{on}");
        }
        for off in ["0", "off", "False", "no"] {
            assert!(!capture_enabled(Some(off)), "{off}");
        }
        assert_eq!(capture_enabled(Some("maybe")), cfg!(debug_assertions));
    }

    #[test]
    fn roles_name_their_diagnostics_files() {
        assert_eq!(Role::Window.as_str(), "window");
        assert_eq!(Role::Prompter.as_str(), "prompter");
        assert_eq!(Role::Chooser.as_str(), "chooser");
    }
}
