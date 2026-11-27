//! Alerts: what the monitors raise and where it goes.
//!
//! Raising an [`Alert`] on an [`AlertLog`]
//!
//! 1. logs it under target `arca::alert` at a level matching its severity,
//! 2. bumps the `alerts.<kind>` counter,
//! 3. keeps it in a bounded ring buffer (persisted into diagnostics), and
//! 4. offers Warning/Critical alerts to the UI as a one-shot status notice.
//!
//! Identical alerts (same kind and summary key) inside [`DEDUP_WINDOW`] are
//! suppressed, so a span that is slow every frame cannot flood the log.

use std::collections::{HashMap, VecDeque};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime};

use serde::{Deserialize, Serialize};

use super::metrics;

/// Alerts kept for diagnostics.
pub const RECENT_CAPACITY: usize = 64;
/// Repeats of the same alert inside this window are dropped.
pub const DEDUP_WINDOW: Duration = Duration::from_secs(30);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Severity {
    Info,
    Warning,
    Critical,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AlertKind {
    /// The UI thread has been inside one frame longer than the stall budget.
    UiStall,
    /// A stalled frame finally completed.
    UiStallRecovered,
    /// Frames are being built far faster than any display refreshes: the
    /// render loop is spinning (e.g. a lost compositor connection).
    FrameStorm,
    /// Many errors were logged in a short window.
    ErrorBurst,
    /// One UI-thread span exceeded its budget.
    SlowOperation,
}

impl AlertKind {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            AlertKind::UiStall => "ui_stall",
            AlertKind::UiStallRecovered => "ui_stall_recovered",
            AlertKind::FrameStorm => "frame_storm",
            AlertKind::ErrorBurst => "error_burst",
            AlertKind::SlowOperation => "slow_operation",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Alert {
    pub kind: AlertKind,
    pub severity: Severity,
    pub summary: String,
    pub at_unix_ms: u64,
}

impl Alert {
    #[must_use]
    pub fn new(kind: AlertKind, severity: Severity, summary: impl Into<String>) -> Alert {
        Alert {
            kind,
            severity,
            summary: summary.into(),
            at_unix_ms: super::unix_ms(SystemTime::now()),
        }
    }

    /// Dedupe key: the summary up to the first digit, so "took 120 ms" and
    /// "took 130 ms" from the same operation count as one alert.
    fn dedup_key(&self) -> (AlertKind, String) {
        let stem: String = self
            .summary
            .chars()
            .take_while(|c| !c.is_ascii_digit())
            .collect();
        (self.kind, stem)
    }
}

#[derive(Default)]
struct Inner {
    recent: VecDeque<Alert>,
    last_seen: HashMap<(AlertKind, String), Instant>,
    notice: Option<Alert>,
}

/// Destination for raised alerts.
#[derive(Default)]
pub struct AlertLog {
    inner: Mutex<Inner>,
}

impl AlertLog {
    #[must_use]
    pub fn new() -> AlertLog {
        AlertLog::default()
    }

    fn with<R>(&self, f: impl FnOnce(&mut Inner) -> R) -> R {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        f(&mut inner)
    }

    /// Raise `alert`. Returns false when it was suppressed as a duplicate.
    pub fn raise(&self, alert: Alert) -> bool {
        let now = Instant::now();
        let key = alert.dedup_key();
        let accepted = self.with(|inner| {
            if let Some(&last) = inner.last_seen.get(&key)
                && now.duration_since(last) < DEDUP_WINDOW
            {
                return false;
            }
            inner.last_seen.insert(key, now);
            if inner.recent.len() == RECENT_CAPACITY {
                inner.recent.pop_front();
            }
            inner.recent.push_back(alert.clone());
            if alert.severity >= Severity::Warning {
                inner.notice = Some(alert.clone());
            }
            true
        });
        if !accepted {
            metrics::counter("alerts.suppressed", 1);
            return false;
        }
        metrics::counter(&format!("alerts.{}", alert.kind.as_str()), 1);
        let level = match alert.severity {
            Severity::Info => log::Level::Info,
            Severity::Warning => log::Level::Warn,
            Severity::Critical => log::Level::Error,
        };
        log::log!(target: "arca::alert", level, "[{}] {}", alert.kind.as_str(), alert.summary);
        true
    }

    /// The retained alerts, oldest first.
    #[must_use]
    pub fn recent(&self) -> Vec<Alert> {
        self.with(|inner| inner.recent.iter().cloned().collect())
    }

    /// The newest Warning/Critical alert not yet shown, consumed on read.
    pub fn take_notice(&self) -> Option<Alert> {
        self.with(|inner| inner.notice.take())
    }
}

/// The process-wide alert log.
pub fn global() -> &'static AlertLog {
    static GLOBAL: OnceLock<AlertLog> = OnceLock::new();
    GLOBAL.get_or_init(AlertLog::new)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raised_alerts_are_retained_and_noticed() {
        let log = AlertLog::new();
        assert!(log.raise(Alert::new(AlertKind::ErrorBurst, Severity::Info, "quiet")));
        assert!(
            log.take_notice().is_none(),
            "info alerts never interrupt the UI"
        );
        assert!(log.raise(Alert::new(AlertKind::UiStall, Severity::Critical, "stuck")));
        assert_eq!(log.take_notice().unwrap().summary, "stuck");
        assert!(log.take_notice().is_none(), "a notice is shown once");
        assert_eq!(log.recent().len(), 2);
    }

    #[test]
    fn duplicates_inside_the_window_are_suppressed() {
        let log = AlertLog::new();
        let slow = |ms: u32| {
            Alert::new(
                AlertKind::SlowOperation,
                Severity::Warning,
                format!("fs.read_dir blocked the UI for {ms} ms"),
            )
        };
        assert!(log.raise(slow(120)));
        assert!(!log.raise(slow(130)), "same operation, different number");
        assert!(log.raise(Alert::new(
            AlertKind::SlowOperation,
            Severity::Warning,
            "preview.load blocked the UI for 140 ms"
        )));
        assert_eq!(log.recent().len(), 2);
    }

    #[test]
    fn the_ring_buffer_is_bounded() {
        let log = AlertLog::new();
        for i in 0..(RECENT_CAPACITY + 10) {
            // Distinct non-digit stems so nothing is deduplicated.
            let stem: String = std::iter::repeat_n('x', i + 1).collect();
            log.raise(Alert::new(AlertKind::ErrorBurst, Severity::Info, stem));
        }
        let recent = log.recent();
        assert_eq!(recent.len(), RECENT_CAPACITY);
        assert_eq!(recent[0].summary.len(), 11, "oldest entries were evicted");
    }

    #[test]
    fn alerts_round_trip_through_json() {
        let alert = Alert::new(AlertKind::FrameStorm, Severity::Critical, "spin");
        let json = serde_json::to_string(&alert).unwrap();
        assert!(json.contains("\"FrameStorm\""), "{json}");
        assert_eq!(serde_json::from_str::<Alert>(&json).unwrap(), alert);
    }
}
