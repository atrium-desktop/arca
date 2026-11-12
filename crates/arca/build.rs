//! Relay the rpaths published by the `iris` crate (`links = "iris_rs"`) so the
//! `arca` binary finds libiris/liblens/libflux/libglyph in the optics meson
//! build tree at runtime without `LD_LIBRARY_PATH`.
//!
//! `transit` needs no entry here: Optics v0.0.79 implements it as pure Safe
//! Rust, so there is no `libtransit.so` and no `transit-sys` links metadata
//! (Optics ADR-0158).

fn main() {
    let Ok(rpaths) = std::env::var("DEP_IRIS_RS_RPATHS") else {
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
