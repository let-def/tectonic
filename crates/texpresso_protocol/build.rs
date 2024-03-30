use std::{env, path::PathBuf};

fn main() {
    let manifest_dir: PathBuf = env::var("CARGO_MANIFEST_DIR").unwrap().into();

    let mut main_header_src = manifest_dir;
    main_header_src.push("support");

    let mut build = cc::Build::new();
    build
        .warnings(true)
        .file("support/support.c")
        .include(&main_header_src)
        .compile("libtexpresso_support.a");

    println!("cargo:rerun-if-changed=support/support.c");

    // Cargo exposes this as the environment variable DEP_XXX_INCLUDE, where XXX
    // is the "links" setting in Cargo.toml. This is the key element that allows
    // us to have a network of crates containing both C/C++ and Rust code that
    // all interlink.
    println!("cargo:include={}", main_header_src.display());
}
