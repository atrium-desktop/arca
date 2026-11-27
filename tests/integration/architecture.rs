//! Architecture firewalls as executable tests.
//!
//! These used to live only in a shell/CI step. Expressing them in the root
//! integration suite means a contributor gets the same failure from
//! `cargo test` as CI does, with a Rust assertion message, and the rules are
//! versioned beside the code they constrain rather than in workflow YAML.
//!
//! The checks parse manifests as text on purpose: adding a TOML parser to the
//! test harness would add a dependency just to enforce the *absence* of
//! dependencies.

use std::collections::BTreeSet;
use std::path::PathBuf;

fn manifest_path(crate_name: &str) -> PathBuf {
    crate::common::workspace::crate_dir(crate_name).join("Cargo.toml")
}

fn manifest(crate_name: &str) -> String {
    let path = manifest_path(crate_name);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}

/// Dependency keys in one manifest section (`[dependencies]`,
/// `[dev-dependencies]`, `[build-dependencies]`).
fn section_keys(text: &str, section: &str) -> BTreeSet<String> {
    let mut keys = BTreeSet::new();
    let mut inside = false;
    for raw in text.lines() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') {
            inside = line == format!("[{section}]");
            continue;
        }
        if !inside {
            continue;
        }
        if let Some((key, _)) = line.split_once('=') {
            keys.insert(key.trim().to_string());
        }
    }
    keys
}

fn all_dependency_keys(crate_name: &str) -> BTreeSet<String> {
    let text = manifest(crate_name);
    ["dependencies", "dev-dependencies", "build-dependencies"]
        .into_iter()
        .flat_map(|section| section_keys(&text, section))
        .collect()
}

/// Dependencies that end up in a shipped artifact. `dev-dependencies` are
/// deliberately excluded: a crate's unit tests may legitimately use the shared
/// test fixture crate without shipping it to consumers.
fn production_dependency_keys(crate_name: &str) -> BTreeSet<String> {
    let text = manifest(crate_name);
    ["dependencies", "build-dependencies"]
        .into_iter()
        .flat_map(|section| section_keys(&text, section))
        .collect()
}

fn assert_no_dependencies(crate_name: &str, why: &str) {
    let declared = all_dependency_keys(crate_name);
    assert!(
        declared.is_empty(),
        "[INV-ARCH] {crate_name} must declare no dependencies ({why}), found {declared:?}"
    );
}

/// A.1 — `arca-xdg` is the zero-dependency Freedesktop specification layer.
#[test]
fn xdg_layer_is_dependency_free() {
    assert_no_dependencies(
        "arca-xdg",
        "portability guarantee: the Freedesktop layer must build anywhere std does",
    );
}

/// A.1b — the shared test harness stays buildable on a host without the
/// Optics toolchain, so it must not acquire dependencies either.
#[test]
fn test_support_is_dependency_free() {
    assert_no_dependencies(
        "test-support",
        "the native-free CI lane builds it without the Optics stack",
    );
}

/// A.2 — `arca-engine` stays headless: no graphics, windowing, or UI crate.
#[test]
fn engine_layer_never_links_the_graphics_stack() {
    let declared = production_dependency_keys("arca-engine");
    let forbidden = [
        "optics-app",
        "optics-app-sys",
        "optics-ui",
        "optics-ui-sys",
        "optics-window",
        "optics-window-sys",
        "optics-gfx",
        "optics-gfx-sys",
        "optics-text",
        "optics-text-sys",
        "optics-motion",
        "optics",
        "arca-ui",
        "arca",
    ];
    let leaked: Vec<&str> = forbidden
        .into_iter()
        .filter(|crate_name| declared.contains(*crate_name))
        .collect();
    assert!(
        leaked.is_empty(),
        "[INV-ARCH-02] arca-engine must stay headless, but declares {leaked:?}"
    );

    let allowed = BTreeSet::from([
        "arca-xdg".to_string(),
        "image".to_string(),
        // ADR-0021: the logging facade only; the concrete logger is in-tree.
        "log".to_string(),
        "serde".to_string(),
        "serde_json".to_string(),
    ]);
    let unexpected: Vec<&String> = declared.difference(&allowed).collect();
    assert!(
        unexpected.is_empty(),
        "arca-engine gained an unreviewed dependency {unexpected:?}; approve it in an ADR and extend this list"
    );
}

/// A.3 — dependencies flow inward only: outer layers may depend on inner
/// layers, never the reverse, and there are no cycles.
#[test]
fn dependency_direction_is_concentric() {
    let allowed: &[(&str, &[&str])] = &[
        ("arca-xdg", &[]),
        ("arca-engine", &["arca-xdg"]),
        ("arca-ui", &["arca-engine", "arca-xdg"]),
        ("arca", &["arca-engine", "arca-ui", "arca-xdg"]),
    ];
    let workspace_crates: BTreeSet<&str> = allowed.iter().map(|(name, _)| *name).collect();

    for (crate_name, permitted) in allowed {
        let declared = section_keys(&manifest(crate_name), "dependencies");
        let internal: BTreeSet<&str> = declared
            .iter()
            .map(String::as_str)
            .filter(|key| workspace_crates.contains(key))
            .collect();
        let extra: Vec<&&str> = internal
            .iter()
            .filter(|key| !permitted.contains(*key))
            .collect();
        assert!(
            extra.is_empty(),
            "[INV-ARCH-03] {crate_name} may only depend inward on {permitted:?}, but declares {extra:?}"
        );
    }
}

/// A.4 — the production crates never depend on the test-only workspace
/// members, so shipping a binary cannot pull test infrastructure in.
#[test]
fn production_crates_do_not_depend_on_test_infrastructure() {
    for crate_name in ["arca-xdg", "arca-engine", "arca-ui", "arca"] {
        let declared = production_dependency_keys(crate_name);
        for forbidden in ["test-support", "arca-tests"] {
            assert!(
                !declared.contains(forbidden),
                "{crate_name} must not depend on the test-only `{forbidden}` crate"
            );
        }
    }
    for (crate_name, path) in [
        (
            "test-support",
            crate::common::workspace::crate_dir("test-support").join("Cargo.toml"),
        ),
        (
            "arca-tests",
            crate::common::workspace::workspace_path("tests/Cargo.toml"),
        ),
    ] {
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
        assert!(
            text.lines().any(|line| line.trim() == "publish = false"),
            "test-only crate {crate_name} must set `publish = false`"
        );
    }
}

/// A.5 — `arca` is the only public binary. No other crate carries a
/// `src/main.rs`.
#[test]
fn arca_is_the_only_binary() {
    let src = crate::common::workspace_root();
    let mut binaries = Vec::new();
    for entry in std::fs::read_dir(src.join("crates")).expect("read crates/") {
        let entry = entry.expect("crate entry");
        if !entry.file_type().expect("crate file type").is_dir() {
            continue;
        }
        if entry.path().join("src/main.rs").is_file() {
            binaries.push(entry.file_name().to_string_lossy().into_owned());
        }
    }
    binaries.sort();
    assert_eq!(
        binaries,
        ["arca"],
        "[INV-BUILD] `arca` is the sole public executable binary"
    );
}

/// A.6 — the test package remains an explicit workspace member, so
/// `cargo test --workspace` cannot silently skip the external suites.
#[test]
fn root_test_suite_is_a_workspace_member() {
    let root = std::fs::read_to_string(crate::common::workspace_root().join("Cargo.toml"))
        .expect("read workspace manifest");
    assert!(
        root.contains("\"tests\""),
        "the `tests` package must be a workspace member"
    );
    assert!(
        root.contains("\"crates/test-support\""),
        "the shared test-support crate must be a workspace member"
    );
}
