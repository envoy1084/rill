//! The macOS FoundationDB package installs `libfdb_c.dylib` with the
//! install name `@rpath/libfdb_c.dylib`, so binaries built with `fdb` need
//! an rpath to find it outside `cargo run`. `FDB_LIB_DIR` overrides the
//! package's default location.

fn main() {
    println!("cargo:rerun-if-env-changed=FDB_LIB_DIR");
    let fdb = std::env::var_os("CARGO_FEATURE_FDB").is_some();
    let macos = std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos");
    if fdb && macos {
        let dir = std::env::var("FDB_LIB_DIR").unwrap_or_else(|_| "/usr/local/lib".into());
        println!("cargo:rustc-link-arg-bins=-Wl,-rpath,{dir}");
    }
}
