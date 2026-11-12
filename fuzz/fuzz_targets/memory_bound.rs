//! The memory limit bounds the process (docs/spec.md §11): source run in
//! a session with `Limits::memory = 8 MiB` must keep the Rust heap's
//! high-water within `K × limit + C`, and compiling it within
//! `COMPILE_K` bytes per source byte (`memory_meter.rs`). A failure is a
//! native allocation the guest can size that the limit does not admit.
//!
//! The input is the `parse` target's (`parse_input::source`), with
//! `M = 8388608` defined first; seed it with `fuzz/memory_probes/*.py`
//! behind a `#` byte, so each runs as written:
//!
//! ```bash
//! mkdir -p fuzz/corpus/memory_bound
//! for f in fuzz/memory_probes/*.py; do (printf '#'; cat "$f") > "fuzz/corpus/memory_bound/$(basename "$f")"; done
//! cd fuzz && cargo +nightly fuzz run memory_bound -- -rss_limit_mb=4096
//! ```

#![no_main]

#[path = "memory_meter.rs"]
mod memory_meter;
#[path = "parse_input.rs"]
mod parse_input;

#[global_allocator]
static METER: memory_meter::Meter = memory_meter::Meter;

const LIMIT: u64 = 8 << 20;

fn check(src: String) {
    let result = std::thread::Builder::new()
        .stack_size(parse_input::STACK)
        .spawn(move || {
            let m = memory_meter::measure(&src, LIMIT, 8);
            if let Some(v) = m.violation(LIMIT, src.len()) {
                panic!("{v}\nsource: {src:?}");
            }
        })
        .expect("spawn")
        .join();
    if let Err(panic) = result {
        std::panic::resume_unwind(panic);
    }
}

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    if let Some(src) = parse_input::source(data) {
        check(src);
    }
});
