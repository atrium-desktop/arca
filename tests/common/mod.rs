//! Shared support for the root external test suites.
//!
//! This module contains no test functions. It re-exports the dependency-free
//! `test-support` vocabulary (scratch directories, environment isolation,
//! polling, workspace/binary lookup) so the integration and end-to-end entries
//! cannot drift, and it owns the one harness that needs the Optics stack: a
//! headless `UiApp` frame driver, compiled only with the `native` feature.

#![allow(dead_code, unused_imports)]

pub use test_support::env;
pub use test_support::fs;
pub use test_support::poll;
pub use test_support::workspace;

#[cfg(feature = "native")]
pub mod ui;

/// Shorthand for the shared binary lookup used by end-to-end entry tests.
pub fn binary(name: &str) -> std::path::PathBuf {
    workspace::binary(name)
}

/// Shorthand for the workspace root.
pub fn workspace_root() -> std::path::PathBuf {
    workspace::workspace_root()
}
