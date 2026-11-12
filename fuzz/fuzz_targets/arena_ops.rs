//! Interprets the input as arena and GC operations and checks every heap
//! invariant against a model (the interpreter is shared with
//! `crates/mm-core/tests/gc.rs`).
//!
//! ```bash
//! cd fuzz && cargo +nightly fuzz run arena_ops
//! ```

#![no_main]

#[path = "../../crates/mm-core/tests/support/ops.rs"]
mod ops;

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    ops::run(data);
});
