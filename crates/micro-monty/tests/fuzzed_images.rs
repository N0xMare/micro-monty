//! The `snapshot_restore` fuzz target's seeds replayed (docs/spec.md §9.3):
//! restore refuses a malformed image with an error, or runs it, without
//! panicking or allocating beyond the limit. The allocation is measured as
//! the growth of the process's peak RSS (Linux `/proc`) and the panics by
//! the process's panic hook, so it is its own test binary with one test,
//! and nothing else runs alongside. Beside the other snapshot tests, which
//! run sessions on every core, the peak grew 602 MiB during one seed on a
//! 128-thread machine; alone, every seed stays under the bound [M].

#![allow(clippy::expect_used, clippy::panic, reason = "tests")]

use std::sync::Mutex;

#[path = "../../../fuzz/fuzz_targets/snapshot_restore_input.rs"]
mod fuzz_input;

/// The `snapshot_restore` fuzz target's stored crash and out-of-memory
/// seeds, then every byte of each program's VM-state tail near its end,
/// where the frames, continuations, tasks and shapes are, inverted in turn:
/// restore must refuse each with an error or run it without panicking or
/// allocating beyond the limit. With the frame-window or shape-table check
/// weakened, some of the tail edits restored and then panicked. Edits at
/// fixed offsets elsewhere stop reaching their field whenever the compiler,
/// the prelude or the heap layout changes; mm-engine's snapshot tests
/// corrupt the decoded fields instead (references, dict indexes, class size
/// hints and the rest).
#[test]
fn fuzzed_images_restore_to_errors_not_panics() {
    static PANICS: Mutex<Vec<String>> = Mutex::new(Vec::new());
    let dir = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../conformance/breadth/fuzz/artifacts"
    );
    let mut seeds: Vec<(String, Vec<u8>)> = std::fs::read_dir(dir)
        .expect("artifacts")
        .map(|e| e.expect("entry").path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("snapshot_restore-"))
        })
        .map(|p| (p.display().to_string(), std::fs::read(&p).expect("seed")))
        .collect();
    let n_seeds = seeds.len();
    for pick in (0..6).chain([0xFE, 0xF0, 0xF1, 0xF2]) {
        // The image left mid-sweep holds a 20 MB heap, so its tail is
        // inverted over its last 128 bytes only.
        let from = if pick == 0xF1 {
            8192 - 128
        } else {
            8192 - 1536
        };
        for pos in from..8192u16 {
            let [lo, hi] = pos.to_le_bytes();
            let data = vec![0x04, pick, lo, hi, 0xFF, 0x00];
            seeds.push((format!("tail of program {pick}: {data:02x?}"), data));
        }
    }
    assert!(n_seeds >= 9, "seeds: {n_seeds}");
    // The facade contains panics (an unwinding build turns them into
    // errors), so the hook is what sees them, as an aborting fuzz build would.
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        PANICS.lock().expect("panics").push(info.to_string());
        default(info);
    }));
    let mut failed = Vec::new();
    for (name, data) in &seeds {
        // Peak memory, where Linux reports it: the session's limit is
        // 64 MiB, so a seed must not grow it by gigabytes.
        let _ = std::fs::write("/proc/self/clear_refs", "5");
        let before = peak_rss_kb();
        let _ = std::panic::catch_unwind(|| fuzz_input::run(data));
        let mut p = PANICS.lock().expect("panics");
        if !p.is_empty() {
            failed.push(format!("{name}: {}", p.join("; ")));
            p.clear();
        }
        if let (Some(a), Some(b)) = (before, peak_rss_kb())
            && b.saturating_sub(a) > 512 << 10
        {
            failed.push(format!("{name}: peak memory grew {} MiB", (b - a) >> 10));
        }
    }
    drop(std::panic::take_hook());
    assert!(failed.is_empty(), "{failed:#?}");
}

fn peak_rss_kb() -> Option<usize> {
    let s = std::fs::read_to_string("/proc/self/status").ok()?;
    let line = s.lines().find(|l| l.starts_with("VmHWM:"))?;
    line.split_whitespace().nth(1)?.parse().ok()
}
