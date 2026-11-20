//! Relay the rpaths published by the `optics-app` crate
//! (`links = "optics_app_rs"`) so the `arca` binary finds
//! liboptics_app/liboptics_ui/liboptics_window/liboptics_gfx/liboptics_text/
//! liboptics_svg in the optics meson build tree at runtime without
//! `LD_LIBRARY_PATH`.
//!
//! Optics v0.0.82 renames the binding crates to their semantic module names
//! and keeps the application shell above the window and UI layers (Optics
//! ADR-0169 / ADR-0170 / ADR-0172). `optics-app`'s pkg-config probe walks its
//! window/ui/gfx dependencies, so its published rpath list covers the whole
//! native stack.
//!
//! `optics-motion` needs no entry here: Optics implements it as pure Safe
//! Rust, so there is no `liboptics_motion.so` and no `-sys` links metadata
//! (Optics ADR-0158).

fn main() {
    let Ok(rpaths) = std::env::var("DEP_OPTICS_APP_RS_RPATHS") else {
        return;
    };
    let dirs: Vec<&str> = rpaths.split(';').filter(|s| !s.is_empty()).collect();
    if dirs.is_empty() {
        return;
    }
    // Old-style DT_RPATH so the search also covers transitive deps.
    println!("cargo:rustc-link-arg=-Wl,--disable-new-dtags");
    for dir in dirs {
        println!("cargo:rustc-link-arg=-Wl,-rpath,{dir}");
    }
}
