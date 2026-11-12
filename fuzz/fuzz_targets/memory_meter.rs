//! The memory-bound check shared by the `memory_bound` fuzz target and the
//! `memory_probes` replay: a counting global allocator gives the process's
//! Rust-heap high-water, and a cell run under `Limits::memory = L` must
//! keep it within `K × L + C` above what it started from. Compiling the
//! source is outside the session's limit and is bounded separately, per
//! source byte.
//!
//! The allocator is test code: it is registered only by these two binaries
//! (the fuzz crate is outside the workspace and is not shipped), and it
//! forwards every call to `System` unchanged.

#![allow(unsafe_code, reason = "GlobalAlloc; test code, see the module docs")]

use micro_monty::{
    Engine, EngineConfig, HostModule, HostValue, Limits, Progress, Resume, Session,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

/// Run heap per byte of the memory limit, and above it, that a cell may
/// reach [M]: the arena itself (1 L) plus one transient native copy of the
/// largest thing it holds, with headroom for allocator rounding.
pub const K: f64 = 2.5;
/// The constant part: the session's fixed tables, the bundled modules a
/// cell decodes, and uncharged working buffers of a few MiB each (the
/// `re` and native-scratch allowances) [M].
pub const C: usize = 16 << 20;
/// Compile heap per source byte [M]: the parser's tree peaks at about 410
/// bytes per byte for one-character statements.
pub const COMPILE_K: usize = 512;
/// The compile constant: the parser's and compiler's fixed tables.
pub const COMPILE_C: usize = 8 << 20;

/// The counting allocator; register it with `#[global_allocator]`.
pub struct Meter;

static CUR: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

fn grow(n: usize) {
    let c = CUR.fetch_add(n, Relaxed) + n;
    PEAK.fetch_max(c, Relaxed);
}

// SAFETY: every method forwards its arguments to `System` unchanged and
// only counts sizes.
unsafe impl GlobalAlloc for Meter {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        // SAFETY: the caller's contract, forwarded.
        let p = unsafe { System.alloc(l) };
        if !p.is_null() {
            grow(l.size());
        }
        p
    }

    unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
        // SAFETY: the caller's contract, forwarded.
        let p = unsafe { System.alloc_zeroed(l) };
        if !p.is_null() {
            grow(l.size());
        }
        p
    }

    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        // SAFETY: the caller's contract, forwarded.
        unsafe { System.dealloc(p, l) };
        CUR.fetch_sub(l.size(), Relaxed);
    }

    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        // SAFETY: the caller's contract, forwarded.
        let q = unsafe { System.realloc(p, l, n) };
        if !q.is_null() {
            if n <= l.size() {
                // Shrinking is in place (glibc, and dlmalloc on wasm).
                CUR.fetch_sub(l.size() - n, Relaxed);
            } else {
                // Growing may move: the copy holds both blocks for a
                // moment, even where this allocator grew in place.
                grow(n);
                CUR.fetch_sub(l.size(), Relaxed);
            }
        }
        q
    }
}

/// Starts a measurement: the heap in use now, from which the peak counts.
fn mark() -> usize {
    let c = CUR.load(Relaxed);
    PEAK.store(c, Relaxed);
    c
}

/// Runs `f` and gives its result and the heap high-water above what was
/// in use when it started.
#[allow(dead_code, reason = "memory_probes' restore checks only")]
pub fn high_water<T>(f: impl FnOnce() -> T) -> (T, usize) {
    let base = mark();
    let out = f();
    (out, PEAK.load(Relaxed).saturating_sub(base))
}

/// What one source did under one limit.
pub struct Measured {
    /// Heap high-water while compiling, above the heap before it.
    pub compile: usize,
    /// Heap high-water from `Session::new` to the end of the cell.
    pub run: usize,
    /// Heap high-water while the session, after the cell, writes its
    /// snapshot (above the heap before `Session::new`, as `run`).
    pub snapshot: usize,
    /// The snapshot's size (0 if it failed).
    pub image: usize,
    /// Heap high-water while that image is restored, the session dropped
    /// and the image held.
    pub restore: usize,
    /// How the cell ended (or the compile error).
    pub outcome: String,
}

impl Measured {
    /// The bound the run's high-water must keep under `limit`.
    pub fn run_bound(limit: u64) -> usize {
        (K * limit as f64) as usize + C
    }

    /// The bound writing or restoring an `image`-byte snapshot must keep:
    /// the arena (at most the limit) and the image once.
    pub fn image_bound(limit: u64, image: usize) -> usize {
        limit as usize + image + C
    }

    /// The bound compiling `source_len` bytes must keep.
    pub fn compile_bound(source_len: usize) -> usize {
        COMPILE_K * source_len + COMPILE_C
    }

    /// `None` if both bounds hold, else what broke.
    pub fn violation(&self, limit: u64, source_len: usize) -> Option<String> {
        if self.run > Self::run_bound(limit) {
            return Some(format!(
                "run heap {} > {K} × {limit} + {C} = {} ({})",
                self.run,
                Self::run_bound(limit),
                self.outcome
            ));
        }
        for (what, peak) in [("snapshot", self.snapshot), ("restore", self.restore)] {
            if peak > Self::image_bound(limit, self.image) {
                return Some(format!(
                    "{what} heap {peak} > {limit} + image {} + {C} ({})",
                    self.image, self.outcome
                ));
            }
        }
        if self.compile > Self::compile_bound(source_len) {
            return Some(format!(
                "compile heap {} > {COMPILE_K} × {source_len} + {COMPILE_C}",
                self.compile
            ));
        }
        None
    }
}

/// Fuel per slice: the cell parks this often, so a long one is cut short
/// after `slices`; host calls return `None`.
const SLICE: u64 = 10_000_000;

/// Compiles `src` with `M` (the limit in bytes) defined first, so a source
/// can size its data from the limit, and runs it in a fresh session with
/// `Limits::memory = limit`, a host module `h` with `sink` and `mark`, and
/// at most `slices` fuel slices.
pub fn measure(src: &str, limit: u64, slices: usize) -> Measured {
    let engine = Engine::new(EngineConfig::new().with_limits(Limits::new().with_memory(limit)));
    let src = format!("M = {limit}\n{src}");
    let base = mark();
    let code = engine.compile(&src, "probe.py");
    let compile = PEAK.load(Relaxed).saturating_sub(base);
    let code = match code {
        Ok(code) => code,
        Err(e) => {
            return Measured {
                compile,
                run: 0,
                snapshot: 0,
                image: 0,
                restore: 0,
                outcome: format!("compile {}: {}", e.type_name, e.message),
            };
        }
    };
    let base = mark();
    let (outcome, run, snapshot, image) = {
        let mut s = match Session::new(&engine) {
            Ok(s) => s,
            Err(e) => panic!("a new session: {e:?}"),
        };
        s.set_output(Box::new(|_| {}));
        if let Err(e) = s.define(&HostModule::untyped("h", &["sink", "mark"])) {
            panic!("define: {e:?}");
        }
        s.set_fuel(Some(SLICE));
        let mut p = s.start(&code);
        for _ in 0..slices {
            let next = match &p {
                Progress::Parked { .. } => {
                    s.set_fuel(Some(SLICE));
                    Resume::Continue
                }
                Progress::HostCall { .. } => Resume::Value(HostValue::None),
                _ => break,
            };
            p = match s.resume(next) {
                Ok(p) => p,
                Err(e) => panic!("resume: {e:?}"),
            };
        }
        let outcome = match p {
            Progress::Complete(_) => "complete".to_owned(),
            Progress::Error(e) => e.type_name,
            other => format!("{other:?}").chars().take(40).collect(),
        };
        let run = PEAK.load(Relaxed).saturating_sub(base);
        mark();
        let image = s.snapshot().unwrap_or_default();
        let snapshot = PEAK.load(Relaxed).saturating_sub(base);
        (outcome, run, snapshot, image)
    };
    mark();
    let restored = (!image.is_empty()).then(|| engine.restore(&image));
    let restore = PEAK.load(Relaxed).saturating_sub(base);
    drop(restored);
    Measured {
        compile,
        run,
        snapshot,
        image: image.len(),
        restore,
        outcome,
    }
}
