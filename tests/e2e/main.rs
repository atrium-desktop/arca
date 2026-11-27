//! Root end-to-end suite.
//!
//! These tests spawn the production `arca` binary as a separate process and
//! speak to it through its real entry surfaces: argv/stdout/stderr, the portal
//! prompter JSON-over-stdio protocol, and the single-instance Unix socket.
//!
//! The whole suite is gated on `native`: the binary links the Optics stack, so
//! a host running the native-free lane cannot execute it (and this package does
//! not build the binary itself; the test runner holds Cargo's target lock).
//! Suites that would need a compositor/GPU are deliberately absent; the window
//! lifecycle is covered by headless frame tests instead.

#[path = "../common/mod.rs"]
mod common;

#[cfg(feature = "native")]
mod cli;
#[cfg(feature = "native")]
mod diagnostics;
#[cfg(feature = "native")]
mod ipc_forward;
#[cfg(feature = "native")]
mod portal_prompter;
