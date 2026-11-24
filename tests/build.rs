//! Re-emit the Optics native-library rpaths for the root test harnesses.
//!
//! These suites link the presentation stack, whose shared libraries are only
//! resolved at run time. `rustc-link-arg` from a dependency's build script does
//! not cross crate boundaries, so the terminal test binaries must restate the
//! search paths published by the `-sys`/wrapper crates — the same relay
//! `crates/arca/build.rs` performs for the shipped binary. When Optics is
//! linked statically the published lists are empty and nothing is emitted.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    let mut rpath_dirs: Vec<String> = Vec::new();
    for var in [
        "DEP_OPTICS_APP_RS_RPATHS",
        "DEP_OPTICS_WINDOW_RS_RPATHS",
        "DEP_OPTICS_UI_RS_RPATHS",
    ] {
        println!("cargo:rerun-if-env-changed={var}");
        let Ok(rpaths) = std::env::var(var) else {
            continue;
        };
        for dir in rpaths.split(';').filter(|path| !path.is_empty()) {
            if !rpath_dirs.iter().any(|existing| existing == dir) {
                rpath_dirs.push(dir.to_owned());
            }
        }
    }
    if rpath_dirs.is_empty() {
        return;
    }

    // Old-style DT_RPATH so the search also covers transitive deps. GNU-style
    // ELF linkers only; macOS ld64 rejects the flag.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        println!("cargo:rustc-link-arg=-Wl,--disable-new-dtags");
    }
    for dir in rpath_dirs {
        println!("cargo:rustc-link-arg=-Wl,-rpath,{dir}");
    }
}
