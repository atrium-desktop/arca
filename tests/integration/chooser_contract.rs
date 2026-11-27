//! Integration coverage for the portal chooser wire contract
//! (`arca-engine::chooser`, Atrium ADR-0027 contract v1).
//!
//! The contract is the boundary between `xdg-desktop-portal-atrium` and this
//! process: a version mismatch, a malformed filter, or an answer the request
//! never offered must be refused before any window or filesystem access. These
//! tests exercise the public encoder/decoder exactly as `main` does, with no
//! GUI and no daemon.

use std::path::Path;

use arca_engine::chooser::{
    ARCA_CONTRACT_VERSION, BytePath, Choice, ChooserRequestEnvelope, ChooserResponseEnvelope,
    FileChooserMode, FileChooserRequest, FileChooserResponse, FileFilter, FilterRule,
    FilterRuleKind, glob_match, mime_matches, read_chooser_request, write_chooser_response,
};
use test_support::fs::TestDir;

fn image_filter() -> FileFilter {
    FileFilter::new(
        "Images",
        vec![FilterRule {
            kind: FilterRuleKind::Glob,
            value: "*.png".into(),
        }],
    )
}

fn request(mode: FileChooserMode) -> FileChooserRequest {
    FileChooserRequest {
        mode,
        app_id: "io.arca.Chooser".into(),
        title: "Choose".into(),
        accept_label: Some("_Open".into()),
        modal: false,
        parent_window: None,
        multiple: false,
        current_folder: Some(BytePath::from("/tmp")),
        current_name: None,
        current_file: None,
        filters: vec![image_filter()],
        current_filter: None,
        choices: Vec::new(),
        files: Vec::new(),
    }
}

fn envelope(request: FileChooserRequest) -> ChooserRequestEnvelope {
    ChooserRequestEnvelope {
        version: ARCA_CONTRACT_VERSION,
        request,
    }
}

#[test]
fn request_envelope_roundtrips_and_strips_gtk_mnemonics() {
    let mut req = request(FileChooserMode::OpenFile);
    req.accept_label = Some("__Save _As".into());
    let encoded = serde_json::to_vec(&envelope(req)).expect("serialize request");

    let decoded = read_chooser_request(encoded.as_slice()).expect("decode request");
    assert_eq!(decoded.mode, FileChooserMode::OpenFile);
    assert_eq!(
        decoded.accept_label.as_deref(),
        Some("_Save As"),
        "`__` escapes a literal underscore and `_A` is a mnemonic marker"
    );
}

#[test]
fn unknown_contract_versions_are_refused_before_parsing_fields() {
    let mut value = serde_json::to_value(envelope(request(FileChooserMode::OpenFile)))
        .expect("serialize request");
    value["version"] = serde_json::json!(ARCA_CONTRACT_VERSION + 1);
    let error = read_chooser_request(serde_json::to_vec(&value).unwrap().as_slice())
        .expect_err("a foreign version must not be coerced");
    assert!(error.contains("contract version"), "error was {error:?}");

    // Retired v6 multi-prompt envelope must fail cleanly on version check.
    let legacy = r#"{"version":6,"prompt":{"kind":"file_chooser","request":{}}}"#;
    let error = read_chooser_request(legacy.as_bytes()).expect_err("legacy v6 must be refused");
    assert!(
        error.contains("contract version 6"),
        "error must report version 6, got {error:?}"
    );
}

#[test]
fn malformed_or_unknown_field_requests_fail_closed() {
    let error = read_chooser_request(b"{".as_slice()).expect_err("truncated JSON");
    assert!(error.contains("failed to parse"), "error was {error:?}");

    let raw = format!(
        r#"{{"version":{ARCA_CONTRACT_VERSION},"prompt":{{"kind":"file_chooser","request":{{}}}}}}"#
    );
    let error =
        read_chooser_request(raw.as_bytes()).expect_err("v6 prompt wrapper under v1 must fail");
    assert!(
        error.contains("decode chooser request envelope"),
        "error was {error:?}"
    );
}

#[test]
fn request_validation_rejects_relative_paths_filters_and_choice_drift() {
    let mut relative = request(FileChooserMode::OpenFile);
    relative.current_folder = Some(BytePath::from("not/absolute"));
    assert!(relative.validate().is_err());

    let mut stray_files = request(FileChooserMode::OpenFile);
    stray_files.files = vec![BytePath::from("a.txt")];
    assert!(
        stray_files.validate().is_err(),
        "suggested files are a SaveFiles-only field"
    );

    let mut empty_filter = request(FileChooserMode::OpenFile);
    empty_filter.filters = vec![FileFilter::new("", Vec::new())];
    assert!(empty_filter.validate().is_err());

    let mut duplicate_choice = request(FileChooserMode::OpenFile);
    duplicate_choice.choices = vec![
        Choice {
            id: "same".into(),
            label: "One".into(),
            options: Vec::new(),
            selected: "true".into(),
        },
        Choice {
            id: "same".into(),
            label: "Two".into(),
            options: Vec::new(),
            selected: "false".into(),
        },
    ];
    assert!(duplicate_choice.validate().is_err());

    let mut bad_boolean = request(FileChooserMode::OpenFile);
    bad_boolean.choices = vec![Choice {
        id: "wrap".into(),
        label: "Wrap".into(),
        options: Vec::new(),
        selected: "yes".into(),
    }];
    assert!(bad_boolean.validate().is_err());

    // Empty initial selection is valid per freedesktop FileChooser spec.
    let mut default_choices = request(FileChooserMode::OpenFile);
    default_choices.choices = vec![
        Choice {
            id: "wrap".into(),
            label: "Wrap".into(),
            options: Vec::new(),
            selected: String::new(),
        },
        Choice {
            id: "enc".into(),
            label: "Encoding".into(),
            options: vec![("utf8".into(), "UTF-8".into())],
            selected: String::new(),
        },
    ];
    assert!(default_choices.validate().is_ok());
}

#[test]
fn save_files_requires_and_deduplicates_basenames() {
    let mut save = request(FileChooserMode::SaveFiles);
    assert!(
        save.validate().is_err(),
        "SaveFiles without suggested names is rejected"
    );

    save.files = vec![BytePath::from("a.txt"), BytePath::from("a.txt")];
    save.validate().expect("two basenames are valid");

    let dir = TestDir::new("savefiles");
    dir.write("a.txt", b"existing");
    let paths = save
        .finish_paths(vec![dir.path().to_path_buf()])
        .expect("resolve names");
    assert_eq!(paths.len(), 2);
    assert_eq!(paths[0].file_name().unwrap().to_string_lossy(), "a (1).txt");
    assert_eq!(paths[1].file_name().unwrap().to_string_lossy(), "a (2).txt");
}

#[test]
fn responses_are_validated_against_the_exact_request() {
    let single = request(FileChooserMode::OpenFile);
    let two_paths = FileChooserResponse::Selected {
        paths: vec![BytePath::from("/tmp/a"), BytePath::from("/tmp/b")],
        current_filter: None,
        choices: Vec::new(),
    };
    assert!(
        two_paths.validate_for(&single).is_err(),
        "single selection must return exactly one path"
    );

    let unoffered_filter = FileChooserResponse::Selected {
        paths: vec![BytePath::from("/tmp/a.png")],
        current_filter: Some(FileFilter::new("Text", Vec::new())),
        choices: Vec::new(),
    };
    assert!(unoffered_filter.validate_for(&single).is_err());

    let accepted = FileChooserResponse::Selected {
        paths: vec![BytePath::from("/tmp/a.png")],
        current_filter: Some(image_filter()),
        choices: Vec::new(),
    };
    accepted
        .validate_for(&single)
        .expect("the offered filter is accepted");
    FileChooserResponse::Cancelled
        .validate_for(&single)
        .expect("cancel is always valid");
}

#[test]
fn response_envelope_names_the_chooser_contract_version() {
    let response = FileChooserResponse::Failed {
        message: "boom".into(),
    };
    let mut encoded = Vec::new();
    write_chooser_response(&mut encoded, response).expect("encode response");
    assert!(encoded.ends_with(b"\n"), "one JSON line");
    let decoded: ChooserResponseEnvelope =
        serde_json::from_slice(&encoded).expect("decode response");
    assert_eq!(decoded.version, ARCA_CONTRACT_VERSION);
    match decoded.response {
        FileChooserResponse::Failed { message } => {
            assert_eq!(message, "boom")
        }
        other => panic!("unexpected response {other:?}"),
    }
}

#[test]
fn filter_rules_match_globs_and_mime_subclasses() {
    assert!(glob_match("*.png", "image.png"));
    assert!(glob_match("*.tar.gz", "archive.tar.gz"));
    assert!(!glob_match("*.png", "photo.jpg"));
    assert!(glob_match("file-?.txt", "file-1.txt"));

    assert!(mime_matches("image/*", Path::new("photo.jpg")));
    assert!(mime_matches("text/plain", Path::new("notes.txt")));
    assert!(!mime_matches("video/*", Path::new("photo.png")));

    let filter = image_filter();
    assert!(filter.allows_mime("photo.png", "image/png"));
    assert!(!filter.allows_mime("photo.jpg", "image/jpeg"));
}
