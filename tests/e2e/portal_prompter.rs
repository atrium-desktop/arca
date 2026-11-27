//! End-to-end coverage for the portal chooser process protocol (contract v1).
//!
//! `arca --chooser-prompt` is invoked by `xdg-desktop-portal-atrium` with one
//! JSON request on stdin and must answer with exactly one JSON line on stdout.
//! A malformed or foreign-version request must fail closed *before* opening a
//! window or touching the filesystem; these cases are therefore safe to run on
//! a headless CI host. The successful selection path is covered by headless
//! chooser frame tests and an interactive acceptance run.

use std::io::Write;
use std::process::{Command, Output, Stdio};

use arca_engine::chooser::{
    ARCA_CONTRACT_VERSION, BytePath, ChooserRequestEnvelope, ChooserResponseEnvelope,
    FileChooserMode, FileChooserRequest, FileChooserResponse,
};

fn run_prompter(flag: &str, input: &[u8]) -> Output {
    let mut child = Command::new(crate::common::binary("arca"))
        .arg(flag)
        .env_remove("WAYLAND_DISPLAY")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn arca prompter");
    child
        .stdin
        .take()
        .expect("piped stdin")
        .write_all(input)
        .expect("write request");
    child.wait_with_output().expect("collect prompter output")
}

fn failed_response(output: &Output) -> String {
    assert!(
        !output.status.success(),
        "an invalid request must exit non-zero, got {:?}",
        output.status
    );
    let decoded: ChooserResponseEnvelope =
        serde_json::from_slice(&output.stdout).expect("stdout is one response envelope");
    assert_eq!(decoded.version, ARCA_CONTRACT_VERSION);
    match decoded.response {
        FileChooserResponse::Failed { message } => message,
        other => panic!("expected a failed response, got {other:?}"),
    }
}

fn valid_request(version: u32) -> Vec<u8> {
    let request = FileChooserRequest {
        mode: FileChooserMode::OpenFile,
        app_id: "io.arca.Chooser".into(),
        title: "Open".into(),
        accept_label: None,
        modal: false,
        parent_window: None,
        multiple: false,
        current_folder: Some(BytePath::from("/tmp")),
        current_name: None,
        current_file: None,
        filters: Vec::new(),
        current_filter: None,
        choices: Vec::new(),
        files: Vec::new(),
    };
    request.validate().expect("fixture request is valid");
    serde_json::to_vec(&ChooserRequestEnvelope { version, request })
        .expect("serialize fixture request")
}

#[test]
fn malformed_json_fails_closed_with_one_response_line() {
    let output = run_prompter("--chooser-prompt", b"{not json");
    let message = failed_response(&output);
    assert!(
        message.contains("failed to parse"),
        "the response names the parse failure, got {message:?}"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("invalid chooser request"),
        "the process log explains the refusal"
    );
    assert_eq!(
        output.stdout.iter().filter(|byte| **byte == b'\n').count(),
        1,
        "exactly one response line"
    );
}

#[test]
fn foreign_contract_versions_are_refused_before_prompting() {
    let output = run_prompter(
        "--chooser-prompt",
        &valid_request(ARCA_CONTRACT_VERSION + 1),
    );
    let message = failed_response(&output);
    assert!(
        message.contains("contract version"),
        "the response names the version mismatch, got {message:?}"
    );

    // Retired v6 multi-prompt envelope must fail with a version mismatch.
    let legacy = br#"{"version":6,"prompt":{"kind":"file_chooser","request":{}}}"#;
    let output = run_prompter("--chooser-prompt", legacy);
    let message = failed_response(&output);
    assert!(
        message.contains("contract version 6"),
        "retired v6 must report version mismatch, got {message:?}"
    );
}

#[test]
fn unknown_envelope_fields_are_refused() {
    let raw = format!(
        r#"{{"version":{ARCA_CONTRACT_VERSION},"prompt":{{"kind":"file_chooser","request":{{}}}}}}"#
    );
    let output = run_prompter("--chooser-prompt", raw.as_bytes());
    let message = failed_response(&output);
    assert!(
        message.contains("decode chooser request envelope"),
        "the response names the envelope error, got {message:?}"
    );
}

#[test]
fn the_prompter_alias_uses_the_same_protocol() {
    let canonical = run_prompter("--chooser-prompt", b"{");
    let alias = run_prompter("--prompter", b"{");
    assert_eq!(failed_response(&canonical), failed_response(&alias));
    assert_eq!(canonical.status.code(), alias.status.code());
}
