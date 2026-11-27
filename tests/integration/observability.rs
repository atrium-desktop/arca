//! Observability pipeline across crate boundaries (native-free).
//!
//! Unit tests beside each telemetry module pin the building blocks; these
//! tests prove the wiring: real engine operations emit the spans, counters,
//! and log lines the diagnostics report promises, and the watchdog names the
//! operation that stalled the UI thread.

use std::time::Duration;

use arca_engine::AppState;
use arca_engine::config::Config;
use arca_engine::telemetry::{self, Role, alerts, diagnostics, metrics, span, watchdog};

use crate::common::fs::TestDir;
use crate::common::poll;

fn state_in(dir: &TestDir, config: &TestDir) -> AppState {
    AppState::with_config(
        config.join("arca.conf").to_str().unwrap().into(),
        Config::default(),
        dir.str(),
    )
}

fn wait_for_navigation(state: &mut AppState) {
    assert!(poll::wait_until(|| {
        state.poll_navigation();
        !state.is_navigating()
    }));
}

/// A navigation journey produces exactly the APM series `--diagnostics`
/// leads with, and they survive a snapshot round trip.
#[test]
fn engine_operations_feed_the_diagnostics_report() {
    let dir = TestDir::new("obs-report");
    dir.subdir("sub");
    dir.write("a.txt", b"a");
    let config = TestDir::new("obs-report-config");
    let mut state = state_in(&dir, &config);

    assert!(state.begin_navigate(dir.join("sub").to_str().unwrap()));
    wait_for_navigation(&mut state);
    state.refresh();

    let report = diagnostics::capture(Role::Window, std::time::SystemTime::now(), None);
    for series in ["fs.read_dir", "nav.latency", "state.refresh"] {
        assert!(
            report
                .metrics
                .histograms
                .get(series)
                .is_some_and(|h| h.count > 0),
            "missing {series}: {:?}",
            report.metrics.histograms.keys().collect::<Vec<_>>()
        );
    }
    assert!(report.metrics.counters["nav.started"] >= 1);

    let out = TestDir::new("obs-report-out");
    let path = diagnostics::path_for(out.path(), Role::Window);
    diagnostics::write(&path, &report).unwrap();
    let text = diagnostics::render_text(&diagnostics::read(&path).unwrap(), true);
    assert!(
        text.contains("nav.latency") && text.contains("fs.read_dir"),
        "{text}"
    );
}

/// Background transfers are measured and outcome-counted.
#[test]
fn transfer_jobs_record_duration_and_outcome() {
    let dir = TestDir::new("obs-io");
    let source = dir.write("payload.bin", vec![7u8; 64 * 1024]);
    let target = dir.subdir("dest");
    let registry = metrics::global();
    let completed = registry.counter_value("io.jobs_completed");
    let bytes = registry.counter_value("io.bytes");

    let mut engine = arca_engine::AsyncIoEngine::new();
    engine.submit(arca_engine::TransferAction::Copy, vec![source], target);
    let mut done = None;
    assert!(poll::wait_until(|| {
        done = done.take().or_else(|| engine.poll());
        done.is_some()
    }));
    assert!(done.unwrap().error.is_none());
    assert!(registry.counter_value("io.jobs_completed") > completed);
    assert!(registry.counter_value("io.bytes") >= bytes + 64 * 1024);
    assert!(registry.histogram("io.job").is_some_and(|h| h.count() > 0));
}

/// The watchdog's stall alert names the innermost UI-thread span, which is
/// what turns "it froze" into an actionable report.
#[test]
fn a_stalled_ui_thread_is_alerted_with_the_blocking_operation() {
    let monitor = watchdog::Watchdog::start(watchdog::Config {
        stall: Duration::from_millis(80),
        poll: Duration::from_millis(10),
        ..watchdog::Config::default()
    });
    let ui = std::thread::spawn(|| {
        span::mark_ui_thread();
        let _frame = watchdog::FrameGuard::begin();
        let _span = telemetry::span("integration.blocking_operation");
        std::thread::sleep(Duration::from_millis(400));
    });
    let named = |kind| {
        alerts::global().recent().iter().any(|alert| {
            alert.kind == kind && alert.summary.ends_with("in integration.blocking_operation")
        })
    };
    assert!(
        poll::wait_for(Duration::from_secs(5), || named(alerts::AlertKind::UiStall)),
        "alerts: {:?}",
        alerts::global().recent()
    );
    ui.join().unwrap();
    assert!(
        poll::wait_for(Duration::from_secs(5), || named(
            alerts::AlertKind::UiStallRecovered
        )),
        "recovery is reported once the frame completes: {:?}",
        alerts::global().recent()
    );
    drop(monitor);
    assert!(metrics::global().histogram("ui.stall").is_some());
}

/// `telemetry::init` installs the file logger: engine status messages and
/// alerts land in `<state>/logs/arca.log` with level, thread, and target.
#[test]
fn init_routes_engine_logs_into_the_rotating_file() {
    let state_dir = TestDir::new("obs-logs");
    let mut options = telemetry::Options::from_env(Role::Chooser);
    options.state_dir = state_dir.path().to_path_buf();
    options.file_filter = "debug".into();
    options.stderr_filter = "off".into();
    options.watchdog = false;
    let telemetry = telemetry::init(options);
    let Some(log_file) = telemetry.log_file.clone() else {
        panic!("the file sink must open in a writable state dir");
    };
    if !log_file.starts_with(state_dir.path()) {
        // Another test in this process installed the logger first (plain
        // `cargo test` runs tests as threads); nextest runs each test in its
        // own process, where this branch never triggers.
        eprintln!("logger already installed by a sibling test; skipping");
        return;
    }

    let dir = TestDir::new("obs-logs-browse");
    let config = TestDir::new("obs-logs-config");
    let mut state = state_in(&dir, &config);
    state.navigate(dir.join("missing").to_str().unwrap());
    alerts::global().raise(alerts::Alert::new(
        alerts::AlertKind::ErrorBurst,
        alerts::Severity::Warning,
        "integration log routing check",
    ));
    log::logger().flush();

    let text = std::fs::read_to_string(&log_file).unwrap();
    assert!(text.contains("starting: role=chooser"), "{text}");
    assert!(
        text.lines()
            .any(|l| l.contains("INFO ") && l.contains("arca::status: Not a directory")),
        "{text}"
    );
    assert!(
        text.lines()
            .any(|l| l.contains("WARN ") && l.contains("arca::alert: [error_burst] integration")),
        "{text}"
    );
}
