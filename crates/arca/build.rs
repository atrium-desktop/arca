//! Relay the rpaths published by the `iris` crate (`links = "iris_rs"`)
//! and the `transit` crate so the `arca` binary finds
//! libiris/liblens/libflux/libglyph/libtransit in the optics meson
//! build tree at runtime without `LD_LIBRARY_PATH`.

fn main() {
    let mut emitted_dtags = false;
    for var in [
        "DEP_IRIS_RS_RPATHS",
        "DEP_TRANSIT_RPATHS",
        "DEP_TRANSIT_RS_RPATHS",
    ] {
        if let Ok(rpaths) = std::env::var(var) {
            let dirs: Vec<&str> = rpaths.split(';').filter(|s| !s.is_empty()).collect();
            if !dirs.is_empty() {
                if !emitted_dtags {
                    println!("cargo:rustc-link-arg=-Wl,--disable-new-dtags");
                    emitted_dtags = true;
                }
                for dir in &dirs {
                    println!("cargo:rustc-link-arg=-Wl,-rpath,{dir}");
                }
            }
        }
    }
}
