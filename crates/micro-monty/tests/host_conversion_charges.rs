//! A host-call argument's conversion reserves its Rust heap from the memory
//! limit before it copies a container's items: a refused conversion never
//! allocates in proportion to the container. Measured as the growth of the
//! process's peak RSS (Linux `/proc`), so it is its own test binary with one
//! test, and nothing else runs alongside.

#![cfg(target_os = "linux")]
#![allow(clippy::expect_used, clippy::panic, reason = "tests")]

use micro_monty::{Engine, EngineConfig, HostModule, HostValue, Limits, Progress, Resume, Session};
use std::sync::{Arc, Mutex};

/// A `kB` field of `/proc/self/status`.
fn status_kb(field: &str) -> usize {
    let status = std::fs::read_to_string("/proc/self/status").expect("proc status");
    status
        .lines()
        .find_map(|l| l.strip_prefix(field))
        .and_then(|v| v.trim().trim_end_matches("kB").trim().parse().ok())
        .expect("status field")
}

const N: usize = 2_000_000;

/// Builds `x`, calls `mark()` (after which the peak RSS is watched) and then `send(x)`,
/// whose conversion the memory limit refuses. Copying the items first would
/// allocate `copied` bytes.
fn refused_conversion_allocates_little(build: &str, memory: u64, copied: usize) {
    let engine = Engine::new(EngineConfig::new().with_limits(Limits::new().with_memory(memory)));
    let mut s = Session::new(&engine).expect("session");
    let out = Arc::new(Mutex::new(String::new()));
    let sink = Arc::clone(&out);
    s.set_output(Box::new(move |t: &str| {
        sink.lock().expect("output lock").push_str(t);
    }));
    s.define(&HostModule::untyped("", &["mark", "send"]))
        .expect("defined");
    let src = format!(
        "x = {build}\nmark()\ntry:\n    send(x)\nexcept MemoryError:\n    print('refused')\n"
    );
    let code = engine.compile(&src, "test.py").expect("compiles");
    let p = s.start(&code);
    let Progress::HostCall { name, .. } = p else {
        panic!("{p:?}")
    };
    assert_eq!(name, "mark");
    // Resets the peak RSS to the current RSS.
    std::fs::write("/proc/self/clear_refs", "5").expect("clear_refs");
    let before = status_kb("VmRSS:");
    let p = s.resume(Resume::Value(HostValue::None)).expect("resumes");
    let growth = status_kb("VmHWM:").saturating_sub(before) * 1024;
    assert_eq!(p, Progress::Complete(None));
    assert_eq!(out.lock().expect("output lock").as_str(), "refused\n");
    assert!(
        growth < copied / 2,
        "{build}: a refused conversion grew the peak RSS by {growth} bytes"
    );
}

#[test]
fn refused_host_conversions_copy_no_items_first() {
    refused_conversion_allocates_little(&format!("[0] * {N}"), 64 << 20, 8 * N);
    refused_conversion_allocates_little(&format!("tuple(range({N}))"), 64 << 20, 8 * N);
    let n = N / 2;
    refused_conversion_allocates_little(&format!("dict.fromkeys(range({n}))"), 80 << 20, 16 * n);
}
