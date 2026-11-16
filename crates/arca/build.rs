//! Relay the rpaths published by the `aperture` crate (`links =
//! "aperture_rs"`) so the `arca` binary finds libaperture/liblens/libiris/
//! libflux/libglyph in the optics meson build tree at runtime without
//! `LD_LIBRARY_PATH`.
//!
//! Optics v0.0.80 moved the application shell from `iris` to `aperture`
//! (Optics ADR-0166); aperture's pkg-config probe walks its lens/flux/iris
//! dependencies, so its published rpath list covers the whole native stack.
//!
//! `transit` needs no entry here: Optics implements it as pure Safe Rust, so
//! there is no `libtransit.so` and no `transit-sys` links metadata (Optics
//! ADR-0158).

fn main() {
    let Ok(rpaths) = std::env::var("DEP_APERTURE_RS_RPATHS") else {
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
