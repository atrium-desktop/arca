//! Relay the rpaths published by `optics-app` (`links = "optics_app_rs"`),
//! `optics-window` (`links = "optics_window_rs"`), and `optics-ui`
//! (`links = "optics_ui_rs"`) so this crate's test binaries find
//! liboptics_app/liboptics_ui/liboptics_window/liboptics_gfx/liboptics_text/
//! liboptics_svg in the optics meson build tree at runtime without
//! `LD_LIBRARY_PATH`.
//!
//! Optics v0.0.82 publishes one crate per semantic module with a matching
//! `links` key (Optics ADR-0169 / ADR-0170). `optics-app` covers the UI host
//! seam and its window/ui/gfx dependencies; the two narrower relays keep the
//! platform and widget layers visible at the call sites that use them.
//!
//! `optics-motion` needs no entry here: Optics implements it as pure Safe
//! Rust, so there is no `liboptics_motion.so` and no `-sys` links metadata
//! (Optics ADR-0158).

fn main() {
    let mut dirs: Vec<String> = Vec::new();
    for var in [
        "DEP_OPTICS_APP_RS_RPATHS",
        "DEP_OPTICS_WINDOW_RS_RPATHS",
        "DEP_OPTICS_UI_RS_RPATHS",
    ] {
        let Ok(rpaths) = std::env::var(var) else {
            continue;
        };
        for dir in rpaths.split(';').filter(|s| !s.is_empty()) {
            if !dirs.iter().any(|existing| existing == dir) {
                dirs.push(dir.to_string());
            }
        }
    }
    if dirs.is_empty() {
        return;
    }
    // Old-style DT_RPATH so the search also covers transitive deps.
    println!("cargo:rustc-link-arg=-Wl,--disable-new-dtags");
    for dir in dirs {
        println!("cargo:rustc-link-arg=-Wl,-rpath,{dir}");
    }
}
