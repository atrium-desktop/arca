//! Workspace paths and built-binary lookup for the external suites.
//!
//! The root test package cannot rely on `CARGO_BIN_EXE_arca`: Cargo only sets
//! that variable for integration tests inside the binary's own package. These
//! helpers resolve the artifact from the target directory instead, and fail
//! with an actionable message when the binary has not been built. A test must
//! never run `cargo build` itself: the test runner already holds Cargo's
//! target-directory lock, so a nested build blocks until the test times out
//! and looks like a product hang.

use std::path::{Path, PathBuf};

/// Absolute path to the workspace root.
pub fn workspace_root() -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    root.canonicalize().unwrap_or(root)
}

/// Absolute path to a crate directory such as `crates/arca-engine`.
pub fn crate_dir(name: &str) -> PathBuf {
    workspace_root().join("crates").join(name)
}

/// Absolute path to a repository file.
pub fn workspace_path(relative: impl AsRef<Path>) -> PathBuf {
    workspace_root().join(relative)
}

/// Directory containing artifacts for the current test profile.
pub fn target_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("ARCA_TARGET_DIR") {
        return PathBuf::from(dir);
    }
    if let Some(dir) = std::env::var_os("CARGO_TARGET_DIR") {
        let dir = PathBuf::from(dir);
        return if dir.is_absolute() {
            dir
        } else {
            workspace_root().join(dir)
        };
    }
    let exe = std::env::current_exe().expect("current test executable path");
    // Cargo layouts seen in the wild:
    //   <target>/<profile>/deps/<test>-<hash>          (classic)
    //   <target>/<profile>/build/<pkg>/<hash>/out/<test>-<hash> (newer)
    // Both put the profile directory one level above `deps`/`build`. Check
    // `deps` first: a package's build output can itself contain a `build`
    // directory, while `deps` is unambiguous.
    for ancestor in exe.ancestors() {
        let name = ancestor.file_name().and_then(|name| name.to_str());
        if (name == Some("deps") || name == Some("build"))
            && let Some(profile) = ancestor.parent()
        {
            return profile.to_path_buf();
        }
    }
    exe.parent()
        .expect("test executable directory")
        .to_path_buf()
}

/// Path to an already-built workspace binary.
pub fn binary(name: &str) -> PathBuf {
    if let Some(path) = env_binary(name) {
        return path;
    }

    let target = target_dir();
    let candidate = target.join(name);
    if candidate.is_file() {
        return candidate;
    }
    let release = target.join("release").join(name);
    if release.is_file() {
        return release;
    }

    let package = package_for_binary(name).unwrap_or("<unknown package>");
    panic!(
        "workspace binary `{name}` is missing from {}.\n\
         The root suites spawn production binaries and never build them: this \
         test already holds Cargo's target-directory lock, so a nested build \
         would deadlock until the test timeout.\n\
         Run `cargo build -p {package} --bin {name}` first, or run the suites \
         through `cargo nextest run`, whose setup script builds every workspace \
         binary once before the tests start.",
        target.display()
    )
}

fn env_binary(name: &str) -> Option<PathBuf> {
    let exact = [
        format!("NEXTEST_BIN_EXE_{name}"),
        format!("CARGO_BIN_EXE_{name}"),
    ];
    let underscored = [
        format!("NEXTEST_BIN_EXE_{}", name.replace('-', "_")),
        format!("CARGO_BIN_EXE_{}", name.replace('-', "_")),
    ];
    exact
        .into_iter()
        .chain(underscored)
        .find_map(|key| std::env::var_os(key).map(PathBuf::from))
        .filter(|path| path.is_file())
}

fn package_for_binary(name: &str) -> Option<&'static str> {
    Some(match name {
        "arca" => "arca",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_root_contains_the_workspace_manifest() {
        assert!(workspace_root().join("Cargo.toml").is_file());
        assert!(crate_dir("arca-engine").join("Cargo.toml").is_file());
    }
}
