//! Relay the rpaths published by the `optics-app` crate
//! (`links = "optics_app_rs"`) so the `arca` binary finds
//! liboptics_app/liboptics_ui/liboptics_window/liboptics_gfx/liboptics_text/
//! liboptics_svg at runtime without `LD_LIBRARY_PATH`.
//!
//! # Link mode is a build input, not a property of this script
//!
//! Arca does not force a link mode here. Upstream `optics-build` supports both
//! and decides per build: by default it links `liboptics_*.so` when the
//! pkg-config probe finds the shared objects and falls back to
//! `liboptics_*.a` on a static-only tree (shared-preferred since Optics
//! v0.0.83). Callers select explicitly with the upstream switches:
//!
//! - `OPTICS_STATIC=1` (or the additive `static` Cargo feature) — require
//!   static; fails when no archive exists.
//! - `OPTICS_DYNAMIC=1` — force shared.
//! - neither — the shared-preferred probe, i.e. whatever the host provides.
//!
//! This script only reacts to the outcome. When the probe chose shared
//! libraries, `optics-app` publishes their directories as `DEP_*_RPATHS` and
//! this script turns them into `-Wl,-rpath` so the binary and its tests run
//! without `LD_LIBRARY_PATH`. When the probe chose the archives the list is
//! empty and no rpath is emitted.
//!
//! Arca's *release* pipeline pins the static mode (`OPTICS_STATIC=1`) and
//! verifies the artifact with `scripts/check-optics-link.sh --expect static`; see
//! docs/adr/0014-optics-link-mode-and-static-release.md. That gate belongs to the
//! release build, so a developer on a shared-only host still gets a working
//! build instead of a hard error.
//!
//! Optics v0.0.83 keeps the application shell above the window and UI layers
//! (Optics ADR-0169 / ADR-0170 / ADR-0172), so `optics-app`'s pkg-config probe
//! walks its window/ui/gfx dependencies and its published rpath list covers
//! the whole native stack. Pre-1.0 SONAMEs carry the full version, so a stale
//! system library cannot satisfy a dynamic load.
//!
//! `optics-motion` needs no entry here: Optics implements it as pure Safe
//! Rust, so there is no `liboptics_motion.so` and no `-sys` links metadata
//! (Optics ADR-0158).

use std::env;

/// `links` metadata published by `optics-app` for every native module it
/// transitively owns.
const DEP_RPATH_VARS: [&str; 1] = ["DEP_OPTICS_APP_RS_RPATHS"];

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    for var in DEP_RPATH_VARS {
        println!("cargo:rerun-if-env-changed={var}");
    }

    let dirs = published_rpaths();
    if dirs.is_empty() {
        // Static Optics: the archives were linked and nothing needs a runpath.
        return;
    }

    // Old-style DT_RPATH so the search also covers transitive deps. GNU-style
    // ELF linkers only; macOS ld64 rejects the flag.
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        println!("cargo:rustc-link-arg=-Wl,--disable-new-dtags");
    }
    for dir in &dirs {
        println!("cargo:rustc-link-arg=-Wl,-rpath,{dir}");
    }
}

/// Rpaths republished by the Optics wrapper crates, deduplicated in order.
fn published_rpaths() -> Vec<String> {
    let mut dirs: Vec<String> = Vec::new();
    for var in DEP_RPATH_VARS {
        let Ok(rpaths) = env::var(var) else {
            continue;
        };
        for dir in rpaths.split(';').filter(|s| !s.is_empty()) {
            if !dirs.iter().any(|existing| existing == dir) {
                dirs.push(dir.to_string());
            }
        }
    }
    dirs
}
