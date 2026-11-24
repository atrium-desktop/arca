//! End-to-end coverage for the binary's argv surface.
//!
//! Only side-effect-free commands (`--help`, `--version`, unknown-option
//! reporting) run here. Anything that reaches the window lifecycle needs a
//! compositor and belongs in an interactive acceptance run, not CI.

use std::process::Command;

fn arca() -> Command {
    let mut command = Command::new(crate::common::binary("arca"));
    // Keep local commands local: no runtime dir to forward to, no display.
    command
        .env_remove("WAYLAND_DISPLAY")
        .env_remove("XDG_RUNTIME_DIR");
    command
}

#[test]
fn help_is_local_and_documents_every_entry_surface() {
    let output = arca().arg("--help").output().expect("spawn arca --help");
    assert!(output.status.success(), "status: {:?}", output.status);
    let stdout = String::from_utf8(output.stdout).expect("utf-8 help");
    for expected in [
        "Usage:",
        "--chooser-prompt",
        "--choose-file",
        "--choose-files",
        "--choose-dir",
        "--save-file",
        "--new-window",
        "--grid",
        "--list",
        "--miller",
    ] {
        assert!(
            stdout.contains(expected),
            "help must document {expected}, got:\n{stdout}"
        );
    }
    assert!(
        output.stderr.is_empty(),
        "help must not warn: {:?}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn version_reports_the_binary_identity() {
    let output = arca().arg("--version").output().expect("spawn arca -V");
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf-8 version");
    assert!(
        stdout.starts_with("arca "),
        "version line must identify the binary, got {stdout:?}"
    );
    assert!(
        stdout.trim().ends_with(env!("CARGO_PKG_VERSION")),
        "binary and workspace package versions must match, got {stdout:?}"
    );
}

#[test]
fn unknown_options_are_reported_without_failing_help() {
    let output = arca()
        .args(["--help", "--definitely-not-a-flag"])
        .output()
        .expect("spawn arca --help --unknown");
    assert!(output.status.success(), "help still exits 0");
    let stderr = String::from_utf8(output.stderr).expect("utf-8 stderr");
    assert!(
        stderr.contains("unknown option: --definitely-not-a-flag"),
        "unknown options are named on stderr, got {stderr:?}"
    );
    let stdout = String::from_utf8(output.stdout).expect("utf-8 stdout");
    assert!(stdout.contains("Usage:"), "help still prints");
}

#[test]
fn short_help_and_version_flags_are_equivalent() {
    let short = arca().arg("-h").output().expect("spawn -h");
    let long = arca().arg("--help").output().expect("spawn --help");
    assert_eq!(short.stdout, long.stdout);

    let short = arca().arg("-V").output().expect("spawn -V");
    let long = arca().arg("--version").output().expect("spawn --version");
    assert_eq!(short.stdout, long.stdout);
}
