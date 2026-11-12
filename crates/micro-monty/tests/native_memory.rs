//! Natives the guest can drive admit their Rust buffers against the memory
//! limit before they fill them: under a 16 MiB limit, none of these grows
//! the process's peak RSS by more than a few times the limit. Each one used
//! to build a native buffer many times the limit before the arena refused
//! its result (or never refused it). Measured as the growth of the
//! process's peak RSS (Linux `/proc`), so it is its own test binary with
//! one test, and nothing else runs alongside. The counting-allocator form
//! of this check, with every probe, is `fuzz/src/bin/memory_probes.rs`.

#![cfg(target_os = "linux")]
#![allow(clippy::expect_used, clippy::panic, reason = "tests")]

use micro_monty::{Engine, EngineConfig, Limits, Progress, Session};

/// A `kB` field of `/proc/self/status`.
fn status_kb(field: &str) -> usize {
    let status = std::fs::read_to_string("/proc/self/status").expect("proc status");
    status
        .lines()
        .find_map(|l| l.strip_prefix(field))
        .and_then(|v| v.trim().trim_end_matches("kB").trim().parse().ok())
        .expect("status field")
}

const LIMIT: u64 = 16 << 20;

/// Runs `src` (with `M`, the limit, defined) and returns how much the peak
/// RSS grew and how the cell ended.
fn peak_growth(src: &str) -> (usize, String) {
    let engine = Engine::new(EngineConfig::new().with_limits(Limits::new().with_memory(LIMIT)));
    let code = engine
        .compile(&format!("M = {LIMIT}\n{src}"), "test.py")
        .expect("compiles");
    // Resets the peak RSS to the current RSS.
    std::fs::write("/proc/self/clear_refs", "5").expect("clear_refs");
    let before = status_kb("VmRSS:");
    let mut s = Session::new(&engine).expect("session");
    let end = match s.start(&code) {
        Progress::Complete(_) => "complete".to_owned(),
        Progress::Error(e) => e.type_name,
        p => panic!("{src}: {p:?}"),
    };
    let growth = status_kb("VmHWM:").saturating_sub(before) * 1024;
    (growth, end)
}

/// Each with the multiple of the limit it reached before at 64 MiB [M];
/// at this limit each grew the peak RSS by 2.6 to 330 times the limit.
const CASES: [&str; 6] = [
    // 91x: a join of one long str subclass item repeated.
    "class S(str): pass\nx = ('x' * (M // 16)).join([S('a')] * 100)",
    // 79x: every match copied all group registers natively.
    "import re\nx = re.compile('x|' + '(a)' * 20000).findall('x' * 40000)",
    // 25x: a struct format item per format byte.
    "import struct\nx = struct.calcsize('x' * (M // 3))",
    // 12x: the pieces' ranges before any piece.
    "x = (',' * (M // 3)).split(',')",
    "x = ('a\\n' * (M // 6)).splitlines()",
    // 3.6x: binary digits are 8 times the int.
    "x = bin(1 << (M * 2))",
];

/// Set in the child process that runs one case.
const CASE: &str = "MM_NATIVE_MEMORY_CASE";

/// The case after [`CASES`]: a 256 KiB cell of one-character statements,
/// compiled by the session. The parser and compiler peak at about 405
/// bytes per source byte (`Engine::compile` grew the peak RSS by 44 MB
/// here \[M\]), so the session refuses it with a `MemoryError` instead.
fn compile_growth() -> (usize, String) {
    let engine = Engine::new(EngineConfig::new().with_limits(Limits::new().with_memory(LIMIT)));
    let mut s = Session::new(&engine).expect("session");
    let src = "1;".repeat(128 << 10);
    std::fs::write("/proc/self/clear_refs", "5").expect("clear_refs");
    let before = status_kb("VmRSS:");
    let end = match s.compile(&src, "cell.py") {
        Ok(_) => "compiled".to_owned(),
        Err(e) => e.type_name,
    };
    let growth = status_kb("VmHWM:").saturating_sub(before) * 1024;
    (growth, end)
}

#[test]
fn guest_sized_natives_stay_within_twice_the_limit() {
    if let Ok(i) = std::env::var(CASE) {
        let i: usize = i.parse().expect("case index");
        let (growth, end) = CASES
            .get(i)
            .map_or_else(compile_growth, |src| peak_growth(src));
        println!("growth {growth} {end}");
        return;
    }
    // One process per case: freed memory stays resident, so a case run
    // after a larger one would grow the peak less.
    let bound = 2 * usize::try_from(LIMIT).expect("a 16 MiB limit");
    let exe = std::env::current_exe().expect("test binary");
    for (i, src) in CASES
        .iter()
        .chain(&["<a 256 KiB cell compiled>"])
        .enumerate()
    {
        let out = std::process::Command::new(&exe)
            .args([
                "--exact",
                "guest_sized_natives_stay_within_twice_the_limit",
                "--nocapture",
            ])
            .env(CASE, i.to_string())
            .output()
            .expect("child test");
        let text = String::from_utf8_lossy(&out.stdout);
        let line = text
            .lines()
            .find_map(|l| l.strip_prefix("growth "))
            .unwrap_or_else(|| panic!("{src}: no result\n{text}"));
        let (growth, end) = line.split_once(' ').expect("growth and end");
        let growth: usize = growth.parse().expect("growth");
        assert!(
            growth < bound,
            "{src}: the peak RSS grew by {growth} bytes ({end}), more than {bound}"
        );
    }
}
