//! Restore cost per MiB of image (docs/sdk.md, "A snapshot or restore is
//! not a step"; docs/spec.md §9.3), with snapshot timed alongside for scale.
//! Built natively and for wasm32-wasip1 by `run.sh`, which runs it natively,
//! under wasmtime and on V8 and summarizes the passes.
//!
//! Prints one tab-separated row per image: target-independent label, image
//! MiB, best snapshot ms (of 3), best restore ms (of 5), first restore ms
//! (the first in the process for that image: what a fresh worker pays; the
//! first row's also holds V8's first-call warm-up).

use micro_monty::{Engine, EngineConfig, Limits, ParkReason, Progress, Resume, Session};
use std::time::Instant;

const MIB: f64 = (1 << 20) as f64;

fn row(engine: &Engine, session: &mut Session, label: &str) {
    let mut snap = f64::MAX;
    let mut img = Vec::new();
    for _ in 0..3 {
        let t = Instant::now();
        img = session.snapshot().expect("snapshot");
        snap = snap.min(t.elapsed().as_secs_f64() * 1e3);
    }
    let (mut best, mut first) = (f64::MAX, None);
    for _ in 0..5 {
        let t = Instant::now();
        let restored = engine.restore(&img).expect("restore");
        let ms = t.elapsed().as_secs_f64() * 1e3;
        first.get_or_insert(ms);
        best = best.min(ms);
        drop(restored);
    }
    let mib = img.len() as f64 / MIB;
    let first = first.unwrap_or(best);
    println!("{label}\t{mib:.1}\t{snap:.1}\t{best:.1}\t{first:.1}");
}

fn main() {
    // The snapshot cost heaps (sdk.md): n two-int lists, the cell parked on
    // fuel in a loop after building them.
    for n in [600_000u64, 1_200_000, 2_400_000, 3_000_000] {
        let src = format!(
            "xs = [[i, i + 1] for i in range({n})]\ns = 0\nfor i in range(100_000):\n    s += i\n"
        );
        let engine = Engine::new(
            EngineConfig::new().with_limits(Limits::new().with_fuel(Some(n + 1000))),
        );
        let code = engine.compile(&src, "lists.py").expect("compile");
        let mut session = Session::new(&engine).expect("session");
        let p = session.start(&code);
        assert!(matches!(p, Progress::Parked { .. }), "{p:?}");
        row(&engine, &mut session, &format!("lists {n}"));
    }

    // Large blocks: `json.dumps` of 5M ints. Finished, its text is one
    // block; parked mid-dumps, the block grows in place with room reserved
    // past its data, which the image records and restore reserves again
    // without filling it (4676c0a). Every park at 256k-unit fuel slices is
    // restored, so the parks right after a growth step are among them; the
    // row reports the fastest and slowest park per MiB.
    let engine = Engine::new(EngineConfig::default());
    let setup = engine
        .compile("import json\nxs = list(range(5_000_000))", "setup.py")
        .expect("compile");
    let fresh = || {
        let mut s = Session::new(&engine).expect("session");
        assert!(matches!(s.start(&setup), Progress::Complete(_)));
        s
    };
    {
        let mut s = fresh();
        let done = engine.compile("t = json.dumps(xs)", "done.py").expect("compile");
        assert!(matches!(s.start(&done), Progress::Complete(_)));
        let spin = engine
            .compile("k = 0\nfor i in range(100_000):\n    k += i\n", "spin.py")
            .expect("compile");
        s.set_fuel(Some(1000));
        assert!(matches!(s.start(&spin), Progress::Parked { .. }));
        row(&engine, &mut s, "dumps finished");
    }
    let cell = engine.compile("t = json.dumps(xs)", "dumps.py").expect("compile");
    let mut s = fresh();
    s.set_fuel(Some(1 << 18));
    let mut p = s.start(&cell);
    let (mut parks, mut lo, mut hi) = (0, f64::MAX, 0.0f64);
    let (mut lo_mib, mut hi_mib) = (0.0, 0.0);
    while p == (Progress::Parked { reason: ParkReason::Fuel }) {
        let img = s.snapshot().expect("snapshot");
        let mut best = f64::MAX;
        for _ in 0..3 {
            let t = Instant::now();
            let restored = engine.restore(&img).expect("restore");
            best = best.min(t.elapsed().as_secs_f64() * 1e3);
            drop(restored);
        }
        let mib = img.len() as f64 / MIB;
        let per = best / mib;
        if per < lo {
            (lo, lo_mib) = (per, mib);
        }
        if per > hi {
            (hi, hi_mib) = (per, mib);
        }
        parks += 1;
        s.set_fuel(Some((1 << 18).max(s.fuel_wanted())));
        p = s.resume(Resume::Continue).expect("resume");
    }
    assert!(matches!(p, Progress::Complete(_)), "{p:?}");
    println!("dumps parked ({parks} parks)\tlowest {lo:.2} ms/MiB at {lo_mib:.1} MiB, highest {hi:.2} at {hi_mib:.1} MiB");
}
