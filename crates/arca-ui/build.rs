//! Relay the rpaths published by `optics-app` (`links = "optics_app_rs"`),
//! `optics-window` (`links = "optics_window_rs"`), and `optics-ui`
//! (`links = "optics_ui_rs"`) so this crate's test binaries find
//! liboptics_app/liboptics_ui/liboptics_window/liboptics_gfx/liboptics_text/
//! liboptics_svg in the optics meson build tree at runtime without
//! `LD_LIBRARY_PATH`.
//!
//! # Link mode is a build input, not a property of this script
//!
//! Arca does not force a link mode here. Upstream `optics-build` supports both
//! and decides per build: by default it links `liboptics_*.so` when the
//! pkg-config probe finds the shared objects and falls back to
//! `liboptics_*.a` on a static-only tree (shared-preferred since Optics
//! v0.0.86). Callers select explicitly with the upstream switches:
//!
//! - `OPTICS_STATIC=1` (or the additive `static` Cargo feature) — require
//!   static; fails when no archive exists.
//! - `OPTICS_DYNAMIC=1` — force shared.
//! - neither — the shared-preferred probe, i.e. whatever the host provides.
//!
//! This script only reacts to the outcome. When the probe chose shared
//! libraries, the wrapper crates publish their directories as `DEP_*_RPATHS`
//! and this script merges them into `-Wl,-rpath` so headless frame tests run
//! without `LD_LIBRARY_PATH`. When the probe chose the archives the lists are
//! empty and no rpath is emitted.
//!
//! Arca's workspace manifest enables the `static` feature by default so
//! canonical and release builds embed `liboptics_*.a` without extra flags,
//! while linked `dev` worktrees set `OPTICS_DYNAMIC=1` in `.cargo/config.toml`
//! for fast shared-library iteration; see
//! docs/adr/0014-optics-link-mode-and-static-release.md and
//! docs/adr/0016-default-static-optics-manifest-and-dev-dynamic-override.md.
//! Release artifacts are verified with
//! `scripts/check-optics-link.sh --expect static`.
//!
//! Optics v0.0.86 publishes one crate per semantic module with a matching
//! `links` key (Optics ADR-0169 / ADR-0170). `optics-app` covers the UI host
//! seam and its window/ui/gfx dependencies; the narrower `optics-window` and
//! `optics-ui` entries keep the platform and widget layers visible at the call
//! sites that use them.
//!
//! `optics-motion` needs no entry here: Optics implements it as pure Safe
//! Rust, so there is no `liboptics_motion.so` and no `-sys` links metadata
//! (Optics ADR-0158).

use std::env;

/// `links` metadata republished by the three Optics wrapper crates this layer
/// depends on.
const DEP_RPATH_VARS: [&str; 3] = [
    "DEP_OPTICS_APP_RS_RPATHS",
    "DEP_OPTICS_WINDOW_RS_RPATHS",
    "DEP_OPTICS_UI_RS_RPATHS",
];

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
