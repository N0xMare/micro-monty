//! Compiles PocketPy 2.2.0's amalgamated pocketpy.c (`$PENG/dl/pocketpy/
//! amalgamated`, from the release tag's amalgamate.py; the release assets
//! are the same two files) with upstream's release flags (-O2, NDEBUG,
//! -ffp-contract=off as its CMakeLists sets), PK_ENABLE_OS=0 and
//! PK_ENABLE_WATCHDOG=1, and shim.c.

fn main() {
    let peng = std::env::var("PENG").unwrap_or_else(|_| "/tmp/peng".into());
    let dir = format!("{peng}/dl/pocketpy/amalgamated");
    cc::Build::new()
        .file(format!("{dir}/pocketpy.c"))
        .file("shim.c")
        .include(&dir)
        .std("c11")
        .opt_level(2)
        .define("NDEBUG", None)
        .define("PK_ENABLE_OS", "0")
        .define("PK_ENABLE_WATCHDOG", "1")
        .flag("-ffp-contract=off")
        .warnings(false)
        .compile("pocketpy");
    println!("cargo:rustc-link-lib=m");
    println!("cargo:rerun-if-changed=shim.c");
}
