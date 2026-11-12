//! Source run end to end on a shared, cached `Engine` (the decoded-module
//! cache, scope decision 45) and on an uncached one: parse, compile and
//! run in fuel slices, imports and asyncio included. Neither may panic or
//! overflow the stack, and both must give the same transcript (stdout,
//! each stop, the end). A difference is the cache's doing, unless it can
//! come from addresses (`id`, a default `repr`) or a MemoryError.
//!
//! It is also the step-time oracle (docs/spec.md §5, bounded native
//! steps): a third run with a 1 ms deadline on its start and on every
//! resume, over the same fuel, must park within `MM_FUZZ_STEP_MS` (default
//! 1000) of each deadline. The engine's bound is 50 ms between two
//! safepoints in a release build; this build is instrumented and runs
//! native steps 2 to 15 times slower [M], so the default catches a step
//! several times over the bound, not one just past it (`tests/steps.rs`
//! measures those in release). An overshoot counts only when a rerun
//! repeats it, since a loaded machine stalls a run now and then; 0 turns
//! the check off.
//!
//! The input is the `parse` target's (`parse_input::source`); seed it with
//! programs that import bundled modules, use `except*` and asyncio.
//!
//! ```bash
//! cd fuzz && cargo +nightly fuzz run run_source
//! ```

#![no_main]

#[path = "parse_input.rs"]
mod parse_input;

use micro_monty::{Engine, EngineConfig, Limits, ParkReason, Progress, Resume, Session};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::time::{Duration, Instant};

const SLICE: u64 = 40_000;
const SLICES: usize = 8;

fn config() -> EngineConfig {
    EngineConfig::new().with_limits(Limits::new().with_memory(64 << 20))
}

fn cached() -> &'static Engine {
    static E: OnceLock<Engine> = OnceLock::new();
    E.get_or_init(|| Engine::with_module_cache(config(), micro_monty::DEFAULT_MODULE_CACHE))
}

fn uncached() -> &'static Engine {
    static E: OnceLock<Engine> = OnceLock::new();
    E.get_or_init(|| Engine::with_module_cache(config(), 0))
}

fn transcript(engine: &Engine, src: &str) -> String {
    let code = match engine.compile(src, "fuzz.py") {
        Ok(code) => code,
        Err(e) => return format!("compile {}: {}", e.type_name, e.rendered),
    };
    let out = Arc::new(Mutex::new(String::new()));
    let mut s = Session::new(engine).expect("a new session");
    let sink = Arc::clone(&out);
    s.set_output(Box::new(move |text| {
        sink.lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push_str(text);
    }));
    // The same bytes in both sessions: the default is the OS's randomness,
    // so an unseeded `random` would differ between the two runs.
    let mut n = 0u8;
    s.set_entropy(Some(Box::new(move |b: &mut [u8]| {
        for x in b {
            n = n.wrapping_add(1);
            *x = n;
        }
    })));
    s.set_fuel(Some(SLICE));
    let mut stops = Vec::new();
    let mut progress = s.start(&code);
    for _ in 0..SLICES {
        let next = match &progress {
            // An operation that parked before starting runs on resume
            // whatever fuel is set (`Session::fuel_wanted`); a host that
            // enforces a budget stops there, and so does this one.
            Progress::Parked { .. } if s.fuel_wanted() > SLICE => break,
            Progress::Parked { .. } => {
                s.set_fuel(Some(SLICE));
                Resume::Continue
            }
            _ => break,
        };
        stops.push(format!("{progress:?}"));
        progress = match s.resume(next) {
            Ok(p) => p,
            Err(e) => panic!("resume refused after a park: {e:?}"),
        };
    }
    stops.push(format!("{progress:?}"));
    let text = out.lock().unwrap_or_else(PoisonError::into_inner).clone();
    format!("{text}\n--\n{}", stops.join("\n"))
}

fn step_limit() -> Duration {
    static L: OnceLock<Duration> = OnceLock::new();
    *L.get_or_init(|| {
        let ms = std::env::var("MM_FUZZ_STEP_MS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(1000);
        Duration::from_millis(ms)
    })
}

/// The most a start or resume ran past a 1 ms deadline, over the work the
/// transcripts do (`SLICE * SLICES` fuel), resuming at every deadline.
fn longest_step(engine: &Engine, src: &str) -> Duration {
    let Ok(code) = engine.compile(src, "fuzz.py") else {
        return Duration::ZERO;
    };
    let mut s = Session::new(engine).expect("a new session");
    s.set_output(Box::new(|_| {}));
    s.set_fuel(Some(SLICE * SLICES as u64));
    let tick = Duration::from_millis(1);
    let mut worst = Duration::ZERO;
    for i in 0..10_000 {
        let t = Instant::now();
        s.set_deadline(Some(t + tick));
        let p = if i == 0 {
            s.start(&code)
        } else {
            match s.resume(Resume::Continue) {
                Ok(p) => p,
                Err(_) => break,
            }
        };
        worst = worst.max(t.elapsed().saturating_sub(tick));
        if p != (Progress::Parked { reason: ParkReason::Deadline }) {
            break;
        }
    }
    worst
}

fn check(src: String) {
    let result = std::thread::Builder::new()
        .stack_size(parse_input::STACK)
        .spawn(move || {
            let a = transcript(cached(), &src);
            let b = transcript(uncached(), &src);
            let addresses = ["id(", "hash(", " at 0x", "MemoryError"];
            if a != b
                && !addresses
                    .iter()
                    .any(|m| a.contains(m) || b.contains(m) || src.contains(m))
            {
                panic!(
                    "cached and uncached engines differ for {src:?}\ncached:\n{a}\nuncached:\n{b}"
                );
            }
            let limit = step_limit();
            if !limit.is_zero() && longest_step(uncached(), &src) > limit {
                let again = longest_step(uncached(), &src);
                if again > limit {
                    panic!(
                        "a native step ran {again:?} past a 1 ms deadline (limit {limit:?}, \
                         MM_FUZZ_STEP_MS) for {src:?}"
                    );
                }
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
