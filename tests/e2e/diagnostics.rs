//! End-to-end coverage for log management and `arca --diagnostics`.
//!
//! Every test points the spawned binary at its own `XDG_STATE_HOME`, so no
//! test reads or writes the developer's real logs, and nothing here needs a
//! compositor: the prompter fails closed on a malformed request before any
//! window exists, which is enough to exercise logger start-up.

use std::io::Write;
use std::process::{Command, Output, Stdio};
use std::time::SystemTime;

use arca_engine::telemetry::{Role, alerts, diagnostics, metrics};

use crate::common::fs::TestDir;

fn arca(state_home: &TestDir) -> Command {
    let mut command = Command::new(crate::common::binary("arca"));
    command
        .env_remove("WAYLAND_DISPLAY")
        .env_remove("XDG_RUNTIME_DIR")
        .env_remove("ARCA_LOG")
        .env_remove("ARCA_LOG_STDERR")
        .env_remove("ARCA_STALL_CAPTURE")
        .env("XDG_STATE_HOME", state_home.path());
    command
}

fn run_with_stdin(mut command: Command, input: &[u8]) -> Output {
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn arca");
    child.stdin.take().unwrap().write_all(input).unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn diagnostics_without_a_snapshot_exits_1_and_creates_nothing() {
    let state = TestDir::new("e2e-diag-empty");
    let output = arca(&state).arg("--diagnostics").output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("no diagnostics recorded yet"), "{stderr}");
    assert!(output.stdout.is_empty());
    assert_eq!(
        state.entry_count(),
        0,
        "a read-only command must not create state"
    );
}

#[test]
fn diagnostics_renders_a_window_snapshot() {
    let state = TestDir::new("e2e-diag-render");
    let registry = metrics::Registry::new();
    registry.record("ui.frame", std::time::Duration::from_millis(3));
    registry.record("nav.latency", std::time::Duration::from_millis(40));
    registry.add("nav.started", 7);
    let mut report = diagnostics::capture(Role::Window, SystemTime::now(), None);
    report.pid = u32::MAX; // never a live pid: the writer reads as exited
    report.metrics = registry.snapshot();
    report.alerts = vec![alerts::Alert::new(
        alerts::AlertKind::UiStallRecovered,
        alerts::Severity::Warning,
        "UI was unresponsive for 1250 ms in fs.read_dir",
    )];
    let path = diagnostics::path_for(&state.path().join("arca"), Role::Window);
    diagnostics::write(&path, &report).unwrap();

    let output = arca(&state).arg("--diagnostics").output().unwrap();
    assert!(output.status.success(), "{:?}", output);
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("diagnostics (window)"), "{stdout}");
    assert!(stdout.contains("(exited)"), "{stdout}");
    assert!(
        stdout.contains("nav.latency") && stdout.contains("nav.started"),
        "{stdout}"
    );
    assert!(
        stdout.contains("ui_stall_recovered") && stdout.contains("1250 ms in fs.read_dir"),
        "{stdout}"
    );
    assert!(
        stdout.contains(&format!("Snapshot: {}", path.display())),
        "{stdout}"
    );
}

/// Stack captures written on `ui_stall` are listed newest last, at most
/// five, with the total; unrelated files in the directory are ignored.
#[test]
fn diagnostics_lists_the_latest_stall_captures() {
    let state = TestDir::new("e2e-diag-captures");
    let report = diagnostics::capture(Role::Window, SystemTime::now(), None);
    diagnostics::write(
        &diagnostics::path_for(&state.path().join("arca"), Role::Window),
        &report,
    )
    .unwrap();
    state.subdir("arca/stalls");
    for minute in 0..7 {
        state.write(
            format!("arca/stalls/stall-2026-11-25T01{minute:02}00.000Z-window-9.txt"),
            b"Arca stall capture",
        );
    }
    state.write("arca/stalls/notes.txt", b"not a capture");

    let output = arca(&state).arg("--diagnostics").output().unwrap();
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Stall captures (7 in "), "{stdout}");
    let listed: Vec<&str> = stdout.lines().filter(|l| l.contains("/stall-")).collect();
    assert_eq!(listed.len(), 5, "{stdout}");
    assert!(
        listed[0].contains("T010200") && listed[4].contains("T010600"),
        "{stdout}"
    );
    assert!(!stdout.contains("notes.txt"), "{stdout}");
}

/// A process that never stalls writes no captures, even with capture on.
#[test]
fn an_unstalled_process_writes_no_stall_captures() {
    let state = TestDir::new("e2e-no-captures");
    let mut command = arca(&state);
    command.arg("--prompter").env("ARCA_STALL_CAPTURE", "1");
    let _ = run_with_stdin(command, b"not a request\n");
    assert!(!state.path().join("arca/stalls").exists());
}

#[test]
fn a_corrupt_snapshot_is_reported_not_rendered() {
    let state = TestDir::new("e2e-diag-corrupt");
    state.subdir("arca");
    state.write("arca/diagnostics-window.json", b"{truncated");
    let output = arca(&state).arg("--diagnostics").output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("cannot read"));
}

/// The prompter's stdout is a protocol channel: even at `ARCA_LOG=trace`,
/// log records go to the file (and stderr), never stdout.
#[test]
fn prompter_logs_to_the_state_dir_and_keeps_stdout_protocol_clean() {
    let state = TestDir::new("e2e-log-prompter");
    let mut command = arca(&state);
    command.arg("--prompter").env("ARCA_LOG", "trace");
    let output = run_with_stdin(command, b"not a request\n");

    assert!(!output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_eq!(
        stdout.lines().count(),
        1,
        "exactly one response line: {stdout:?}"
    );
    serde_json::from_str::<serde_json::Value>(stdout.trim()).expect("stdout is pure JSON");

    let log = std::fs::read_to_string(state.join("arca/logs/arca.log")).expect("log file");
    let first = log.lines().next().unwrap_or_default();
    assert!(
        first.contains(" INFO  [main] arca: arca ") && first.contains("role=prompter"),
        "start-up line: {first:?}"
    );
    assert!(
        !state.join("arca/diagnostics-prompter.json").exists(),
        "short-lived roles do not write diagnostics snapshots"
    );
}

#[test]
fn stderr_echo_follows_arca_log_stderr() {
    let state = TestDir::new("e2e-log-stderr");
    let mut loud = arca(&state);
    loud.args(["--prompter", "--bogus-flag"])
        .env("ARCA_LOG_STDERR", "warn");
    let loud = run_with_stdin(loud, b"{}");
    let loud_err = String::from_utf8_lossy(&loud.stderr);
    assert!(
        loud_err.contains("WARN  [main] arca: unknown option: --bogus-flag"),
        "{loud_err}"
    );

    let mut quiet = arca(&state);
    quiet
        .args(["--prompter", "--bogus-flag"])
        .env("ARCA_LOG_STDERR", "off");
    let quiet = run_with_stdin(quiet, b"{}");
    let quiet_err = String::from_utf8_lossy(&quiet.stderr);
    assert!(
        !quiet_err.contains("WARN  [main]"),
        "stderr echo is off: {quiet_err}"
    );
    // The file sink still has both runs.
    let log = std::fs::read_to_string(state.join("arca/logs/arca.log")).unwrap();
    assert_eq!(
        log.matches("unknown option: --bogus-flag").count(),
        2,
        "{log}"
    );
}
