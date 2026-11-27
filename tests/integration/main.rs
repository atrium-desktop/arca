//! Root integration suite.
//!
//! One test binary contains every integration module. Cargo compiles and links
//! the suite once instead of producing one executable per former crate-local
//! `tests/*.rs` target. Modules that cross the native boundary are gated by
//! the `native` feature, so `cargo test -p arca-tests --no-default-features`
//! still runs the architecture, engine, XDG, and protocol suites on a host
//! without the Optics toolchain.

#[path = "../common/mod.rs"]
mod common;

mod architecture;
mod chooser_contract;
mod file_operations;
mod navigation;
mod observability;

#[cfg(feature = "native")]
mod damage_and_quiescence;
#[cfg(feature = "native")]
mod interaction_stress;
#[cfg(feature = "native")]
mod ipc;
#[cfg(feature = "native")]
mod observability_ui;
#[cfg(feature = "native")]
mod ui_flows;
#[cfg(feature = "native")]
mod virtualization;
