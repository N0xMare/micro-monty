//! Compiles shim.c with the flags MicroPython's own build used for the
//! sandbox variant (micropython/build.sh writes them to
//! `$PENG/bin/mpy-cflags.txt`) and links `$PENG/bin/libmicropython.a`.

fn main() {
    let peng = std::env::var("PENG").unwrap_or_else(|_| "/tmp/peng".into());
    let flags = std::fs::read_to_string(format!("{peng}/bin/mpy-cflags.txt"))
        .expect("run micropython/build.sh first");
    let mut b = cc::Build::new();
    b.file("shim.c").cargo_metadata(true);
    for f in flags.split_whitespace() {
        b.flag(f);
    }
    b.warnings(false).compile("mpyshim");
    println!("cargo:rustc-link-search=native={peng}/bin");
    println!("cargo:rustc-link-lib=static=micropython");
    println!("cargo:rustc-link-lib=m");
    println!("cargo:rerun-if-changed=shim.c");
    println!("cargo:rerun-if-changed={peng}/bin/libmicropython.a");
}
