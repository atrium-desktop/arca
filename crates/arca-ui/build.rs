//! Relay the rpaths published by the `iris` crate (`links = "iris_rs"`,
//! re-exported from iris-sys) so this crate's test binaries find
//! libiris/liblens/libflux in the optics meson build tree at runtime
//! without `LD_LIBRARY_PATH`.

fn main() {
    let rpaths = std::env::var("DEP_IRIS_RS_RPATHS").unwrap_or_default();
    let dirs: Vec<&str> = rpaths.split(';').filter(|s| !s.is_empty()).collect();
    if !dirs.is_empty() {
        // Old-style DT_RPATH so the search also covers transitive deps.
        println!("cargo:rustc-link-arg=-Wl,--disable-new-dtags");
        for dir in &dirs {
            println!("cargo:rustc-link-arg=-Wl,-rpath,{dir}");
        }
    }
}
