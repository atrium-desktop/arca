//! Relay the rpaths published by `aperture` (`links = "aperture_rs"`) and
//! `iris` (`links = "iris_rs"`) so this crate's test binaries find
//! libaperture/liblens/libiris/libflux/libglyph in the optics meson build
//! tree at runtime without `LD_LIBRARY_PATH`.
//!
//! Optics v0.0.80 split the application shell into `aperture` (Optics
//! ADR-0166), so both native relaying paths are needed: `aperture` covers the
//! UI host seam and its lens/flux dependencies, while `iris` keeps the
//! windowing libraries.
//!
//! `transit` needs no entry here: Optics implements it as pure Safe Rust, so
//! there is no `libtransit.so` and no `transit-sys` links metadata (Optics
//! ADR-0158).

fn main() {
    let mut dirs: Vec<String> = Vec::new();
    for var in ["DEP_APERTURE_RS_RPATHS", "DEP_IRIS_RS_RPATHS"] {
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
