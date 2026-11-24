//! Shared infrastructure for Arca tests.
//!
//! Arca's verification pyramid has three external layers that all need the
//! same vocabulary:
//!
//! 1. crate-local unit tests (`#[cfg(test)]` beside the module they constrain),
//! 2. the root integration suite (`tests/integration/`), and
//! 3. the root end-to-end suite (`tests/e2e/`).
//!
//! This crate holds no assertions. It provides scratch directories,
//! environment isolation, polling, and workspace/binary lookup so the three
//! layers cannot drift into subtly different fixtures. It is a test-only
//! member and depends on nothing outside `std`: the native-free CI lane builds
//! it on hosts without a Wayland/GPU toolchain.

pub mod env;
pub mod fs;
pub mod poll;
pub mod workspace;
