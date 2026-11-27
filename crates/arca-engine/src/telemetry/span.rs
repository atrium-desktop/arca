//! Timed spans: the APM instrumentation primitive.
//!
//! `let _span = telemetry::span("fs.read_dir");` times the enclosing scope
//! into the histogram of the same name. Spans opened on the UI thread (the
//! thread that called [`mark_ui_thread`]) are additionally
//!
//! - published on the thread's watchdog heartbeat as its *current
//!   operation*, which the watchdog names when it reports a stall ("UI unresponsive for 2.1 s in fs.read_dir"), and
//! - held to [`UI_BUDGET`]: an over-budget span is logged and raises a
//!   [`AlertKind::SlowOperation`](super::AlertKind::SlowOperation) alert.
//!
//! Blocking filesystem work on the UI thread is counted per thread by
//! [`note_ui_blocking_io`], so tests can assert that an interaction stays off
//! the disk without racing sibling tests.

use std::cell::Cell;
use std::time::{Duration, Instant};

use super::{alerts, metrics};

/// A UI-thread span slower than this is a visible hitch: log and alert.
pub const UI_BUDGET: Duration = Duration::from_millis(100);
/// A background span slower than this is worth a log line.
pub const BACKGROUND_BUDGET: Duration = Duration::from_secs(5);

thread_local! {
    static IS_UI: Cell<bool> = const { Cell::new(false) };
    static UI_IO: Cell<u64> = const { Cell::new(0) };
    static CURRENT: Cell<Option<&'static str>> = const { Cell::new(None) };
}

/// Declare the calling thread a UI thread (registering its watchdog
/// heartbeat). Idempotent.
pub fn mark_ui_thread() {
    IS_UI.with(|flag| flag.set(true));
    let _ = super::watchdog::heartbeat();
}

/// Whether the calling thread was marked with [`mark_ui_thread`].
#[must_use]
pub fn is_ui_thread() -> bool {
    IS_UI.with(Cell::get)
}

/// The innermost UI span open on the calling thread.
#[must_use]
pub fn current_operation() -> Option<&'static str> {
    CURRENT.with(Cell::get)
}

/// Record the innermost span locally and publish it on this thread's
/// heartbeat, where the watchdog (on its own thread) reads it.
fn set_current(value: Option<&'static str>) {
    CURRENT.with(|current| current.set(value));
    super::watchdog::heartbeat().set_operation(value);
}

/// Count one blocking filesystem call made on the UI thread.
///
/// No-op elsewhere. Also feeds the global `ui.blocking_io` counter.
pub fn note_ui_blocking_io() {
    if is_ui_thread() {
        UI_IO.with(|count| count.set(count.get() + 1));
        metrics::counter("ui.blocking_io", 1);
    }
}

/// Blocking filesystem calls this thread made while marked as the UI thread.
#[must_use]
pub fn ui_blocking_io_count() -> u64 {
    UI_IO.with(Cell::get)
}

/// A running span; records on drop.
#[must_use = "a span measures the scope that holds it"]
pub struct Span {
    name: &'static str,
    start: Instant,
    ui: bool,
    parent: Option<&'static str>,
}

/// Open a span named `name` (dot-separated, e.g. `nav.read_dir`).
pub fn span(name: &'static str) -> Span {
    let ui = is_ui_thread();
    let parent = if ui {
        let parent = current_operation();
        set_current(Some(name));
        parent
    } else {
        None
    };
    Span {
        name,
        start: Instant::now(),
        ui,
        parent,
    }
}

impl Span {
    #[must_use]
    pub fn elapsed(&self) -> Duration {
        self.start.elapsed()
    }
}

impl Drop for Span {
    fn drop(&mut self) {
        let elapsed = self.start.elapsed();
        metrics::observe(self.name, elapsed);
        if self.ui {
            set_current(self.parent);
            if elapsed > UI_BUDGET {
                metrics::counter("ui.slow_operations", 1);
                log::warn!(
                    target: "arca::apm",
                    "slow UI-thread operation {} took {} ms (budget {} ms)",
                    self.name,
                    elapsed.as_millis(),
                    UI_BUDGET.as_millis()
                );
                alerts::global().raise(alerts::Alert::new(
                    alerts::AlertKind::SlowOperation,
                    alerts::Severity::Warning,
                    format!(
                        "{} blocked the UI for {} ms",
                        self.name,
                        elapsed.as_millis()
                    ),
                ));
            }
        } else if elapsed > BACKGROUND_BUDGET {
            log::info!(
                target: "arca::apm",
                "long background operation {} took {} ms",
                self.name,
                elapsed.as_millis()
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spans_record_into_the_named_histogram() {
        let before = metrics::global()
            .histogram("test.span.records")
            .map_or(0, |h| h.count());
        {
            let _span = span("test.span.records");
        }
        let after = metrics::global()
            .histogram("test.span.records")
            .unwrap()
            .count();
        assert_eq!(after, before + 1);
    }

    #[test]
    fn ui_spans_publish_and_restore_the_current_operation() {
        // Run on a dedicated thread: the UI flag is thread-local.
        std::thread::spawn(|| {
            mark_ui_thread();
            let outer = span("test.span.outer");
            assert_eq!(current_operation(), Some("test.span.outer"));
            {
                let _inner = span("test.span.inner");
                assert_eq!(current_operation(), Some("test.span.inner"));
            }
            assert_eq!(current_operation(), Some("test.span.outer"));
            drop(outer);
            assert_eq!(current_operation(), None);
        })
        .join()
        .unwrap();
    }

    #[test]
    fn blocking_io_is_counted_only_on_the_ui_thread() {
        std::thread::spawn(|| {
            note_ui_blocking_io();
            assert_eq!(
                ui_blocking_io_count(),
                0,
                "background threads are free to block"
            );
            mark_ui_thread();
            note_ui_blocking_io();
            note_ui_blocking_io();
            assert_eq!(ui_blocking_io_count(), 2);
        })
        .join()
        .unwrap();
    }

    #[test]
    fn over_budget_ui_span_raises_a_slow_operation_alert() {
        std::thread::spawn(|| {
            mark_ui_thread();
            {
                let _span = span("test.span.slow");
                std::thread::sleep(UI_BUDGET + Duration::from_millis(20));
            }
        })
        .join()
        .unwrap();
        assert!(
            alerts::global()
                .recent()
                .iter()
                .any(|a| a.kind == alerts::AlertKind::SlowOperation
                    && a.summary.starts_with("test.span.slow")),
            "alerts: {:?}",
            alerts::global().recent()
        );
    }
}
