//! End-to-end coverage for the portal prompter process protocol.
//!
//! `arca --chooser-prompt` is invoked by the portal daemon with one JSON
//! request on stdin and must answer with exactly one JSON line on stdout. A
//! malformed or foreign-version request must fail closed *before* opening a
//! window or touching the filesystem; these cases are therefore safe to run on
//! a headless CI host. The successful selection path is covered by headless
//! chooser frame tests and an interactive acceptance run.

use std::io::Write;
use std::process::{Command, Output, Stdio};

use arca_engine::chooser::{
    BytePath, FileChooserMode, FileChooserRequest, FileChooserResponse, PromptRequest,
    PrompterRequest, PrompterResponse, PROCESS_CONTRACT_VERSION,
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
    let decoded: PrompterResponse =
        serde_json::from_slice(&output.stdout).expect("stdout is one response envelope");
    assert_eq!(decoded.version, PROCESS_CONTRACT_VERSION);
    match decoded.result {
        arca_engine::chooser::PromptResult::FileChooser(FileChooserResponse::Failed {
            message,
        }) => message,
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
    serde_json::to_vec(&PrompterRequest {
        version,
        prompt: PromptRequest::FileChooser(Box::new(request)),
        appearance: None,
    })
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
        String::from_utf8_lossy(&output.stderr).contains("invalid prompter request"),
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
        &valid_request(PROCESS_CONTRACT_VERSION + 1),
    );
    let message = failed_response(&output);
    assert!(
        message.contains("contract version"),
        "the response names the version mismatch, got {message:?}"
    );
}

#[test]
fn a_non_file_chooser_prompt_is_refused() {
    let raw = format!(r#"{{"version":{PROCESS_CONTRACT_VERSION},"prompt":{{"kind":"unknown"}}}}"#);
    let output = run_prompter("--chooser-prompt", raw.as_bytes());
    let message = failed_response(&output);
    assert!(
        message.contains("did not contain a file chooser"),
        "the response names the missing prompt, got {message:?}"
    );
}

#[test]
fn the_prompter_alias_uses_the_same_protocol() {
    let canonical = run_prompter("--chooser-prompt", b"{");
    let alias = run_prompter("--prompter", b"{");
    assert_eq!(failed_response(&canonical), failed_response(&alias));
    assert_eq!(canonical.status.code(), alias.status.code());
}
