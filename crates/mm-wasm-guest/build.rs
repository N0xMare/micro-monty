//! The guest's shadow stack (linear memory below its data, so an overflow
//! traps instead of overwriting data): 832 KiB instead of Rust's 1 MiB
//! default. Running code keeps its nesting in the arena; only the parser
//! and compiler recurse, bounded by the nesting limits. Their deepest legal
//! input, nested lambda defaults at the 200-level limit, takes 670,176
//! bytes [M], so this leaves 27% (crates/mm-wasm-host/tests/stack.rs checks
//! that the deepest inputs stay under 90%). Every cell, own case, corpus file
//! and bench program takes at most 41 KB [M].

/// Bytes of shadow stack: a multiple of 16 (the wasm stack alignment).
const STACK_BYTES: u32 = 832 << 10;

fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_ARCH").is_ok_and(|a| a == "wasm32") {
        println!("cargo::rustc-link-arg-cdylib=-zstack-size={STACK_BYTES}");
        // Read-only for hosts: its value before the first call is the
        // stack's top, so a test host can measure the deepest use.
        println!("cargo::rustc-link-arg-cdylib=--export=__stack_pointer");
    }
}
