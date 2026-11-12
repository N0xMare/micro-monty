//! Replays `fuzz/memory_probes/*.py`, each sized from `M` (the limit), at
//! several memory limits, and checks the `memory_bound` target's bounds
//! on each (`memory_meter.rs`). Prints one row per probe and limit: the
//! run heap's high-water over the limit, the compile heap, the outcome.
//!
//! ```bash
//! cargo run --release --manifest-path fuzz/Cargo.toml --features memory-probes --bin memory_probes [-- FILTER]
//! ```
//!
//! Then restore under the same bound: images whose heap needs more than
//! the restoring limit, and every 32-bit field of each engine image's VM
//! state set to the largest table count the reader admits (see
//! `restore_counts`).
//!
//! Exits non-zero if any probe or restore breaks a bound.

#[path = "../../fuzz_targets/memory_meter.rs"]
mod memory_meter;

use micro_monty::{
    Engine, EngineConfig, HostModule, HostParam, HostSignature, HostValue, Limits, ParamKind,
    Progress, Resume, Session, SnapshotError,
};

#[global_allocator]
static METER: memory_meter::Meter = memory_meter::Meter;

const LIMITS: [u64; 3] = [16 << 20, 64 << 20, 256 << 20];

fn main() {
    let filter = std::env::args().nth(1).unwrap_or_default();
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/memory_probes");
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .expect("fuzz/memory_probes")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "py"))
        .filter(|p| p.to_string_lossy().contains(&filter))
        .collect();
    files.sort();
    let mut failed = 0;
    println!(
        "{:<22} {:>6} {:>8} {:>8} {:>9} {:>7} {:>7} {:>7}  outcome",
        "probe", "limit", "run/L", "run MiB", "compile", "image/L", "snap/L", "rest/L"
    );
    for f in &files {
        let name = f.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let src = std::fs::read_to_string(f).expect("probe source");
        for limit in LIMITS {
            let src2 = src.clone();
            let m = std::thread::Builder::new()
                .stack_size(8 << 20)
                .spawn(move || memory_meter::measure(&src2, limit, 200))
                .expect("spawn")
                .join()
                .expect("probe thread");
            let bad = m.violation(limit, src.len());
            let per = |n: usize| n as f64 / limit as f64;
            println!(
                "{name:<22} {:>4}Mi {:>8.2} {:>8.1} {:>8.1}M {:>7.2} {:>7.2} {:>7.2}  {}{}",
                limit >> 20,
                per(m.run),
                m.run as f64 / f64::from(1 << 20),
                m.compile as f64 / f64::from(1 << 20),
                per(m.image),
                per(m.snapshot),
                per(m.restore),
                m.outcome,
                if bad.is_some() { "  OVER" } else { "" }
            );
            if let Some(v) = bad {
                eprintln!("{name} at {} MiB: {v}", limit >> 20);
                failed += 1;
            }
        }
    }
    println!(
        "bound: run ≤ {} × limit + {} MiB; snapshot and restore ≤ limit + image + C; compile ≤ {} B per source byte + {} MiB",
        memory_meter::K,
        memory_meter::C >> 20,
        memory_meter::COMPILE_K,
        memory_meter::COMPILE_C >> 20
    );
    failed += restore_heaps() + restore_counts();
    if failed > 0 {
        eprintln!("{failed} probe or restore runs broke the bound");
        std::process::exit(1);
    }
}

/// The limit restores run under: their bound is 2.5 × 8 MiB + 16 MiB.
const RESTORE_LIMIT: u64 = 8 << 20;

/// Images whose heap is 48 MiB, in small objects or in large ones,
/// restored under an 8 MiB limit: refused, and the segments installed on
/// the way are the limit's, not the image's. One of a few MiB, in several
/// segments of each kind, restores.
fn restore_heaps() -> usize {
    let mut failed = 0;
    let roomy = Engine::new(EngineConfig::new().with_limits(Limits::new().with_memory(64 << 20)));
    let tight =
        Engine::new(EngineConfig::new().with_limits(Limits::new().with_memory(RESTORE_LIMIT)));
    for (name, src, fits) in [
        ("small objects", "xs = [[i] for i in range(1_000_000)]\n", false),
        ("large objects", "xs = [bytes(1 << 20) for _ in range(48)]\n", false),
        (
            "segments of both kinds",
            "xs = [[i] for i in range(30_000)]\nys = [bytes(200_000) for _ in range(3)]\n",
            true,
        ),
    ] {
        let mut s = Session::new(&roomy).expect("session");
        let code = roomy.compile(src, "heap.py").expect("compiles");
        assert_eq!(s.start(&code), Progress::Complete(None), "{name}");
        let image = s.snapshot().expect("snapshot");
        drop(s);
        let (expected, peak) = memory_meter::high_water(|| match tight.restore(&image) {
            Ok(_) => fits,
            Err(e) => !fits && matches!(e, SnapshotError::TooLarge),
        });
        let bound = memory_meter::Measured::run_bound(RESTORE_LIMIT);
        println!(
            "restore of {name} ({} MiB image) under 8 MiB: {:.1} MiB",
            image.len() >> 20,
            peak as f64 / f64::from(1 << 20)
        );
        if !expected || peak > bound {
            eprintln!("restore of {name}: as expected {expected}, heap {peak} > {bound}");
            failed += 1;
        }
    }
    failed
}

/// Sessions under `COUNT_LIMIT`, stopped where their images hold
/// frames, stacks, continuations, shapes, tasks, host futures, host
/// values and bundled code. The last also defines a typed host module,
/// whose image keeps its program's key and recipe: restored without the
/// engine that built that program, it is refused at the host code, so its
/// sweep covers the host modules.
const SESSIONS: [&str; 7] = [
    // Idle, with classes, closures, bundled modules and host constants.
    "import bisect, h\n\
     class P:\n    def __init__(self, x):\n        self.x = x\n\
     def mk(n):\n    return lambda: n\nps = [P(i) for i in range(30)]\nfs = [mk(i) for i in range(5)]\n\
     d = {str(i): (i, b'x' * i, 2 ** (i * 5)) for i in range(20)}\ns = {1, 2, 'three'}\n\
     i = bisect.bisect([1, 3], 2)\nk = h.K\n",
    // Parked inside a generator inside a method.
    "class W:\n    def __init__(self, n):\n        self.n = n\n    def items(self):\n        def gen(k):\n\
     \x20           for i in range(k):\n                try:\n                    yield {'i': i, 's': str(i)}\n\
     \x20               finally:\n                    pass\n        t = 0\n        for d in gen(self.n):\n\
     \x20           t += d['i']\n        return t\nprint(W(10000).items())\n",
    // A host call from inside a sort key, with nested arguments.
    "import h\ndef key(x):\n    return h.ask([x, {'k': (x, 'v')}, b'b', 2 ** 70, 1.5, None, True], n=x)\n\
     print(sorted([3, 1, 2], key=key))\n",
    // Waiting on host futures from tasks.
    "import asyncio, h\nasync def one(i):\n    await asyncio.sleep(0)\n    return await h.slow(i)\n\
     async def main():\n    return await asyncio.gather(*[one(i) for i in range(3)])\nprint(asyncio.run(main()))\n",
    // Parked in recursion through dunders and a with block.
    "class C:\n    def __init__(self, n):\n        self.n = n\n    def __add__(self, o):\n\
     \x20       return C(self.n + o.n) if self.n < 3000 else self\n    def __enter__(self):\n        return self\n\
     \x20   def __exit__(self, *a):\n        return False\nwith C(1) as c:\n    x = c\n    while x.n < 3000:\n\
     \x20       x = x + C(1)\nprint(x.n)\n",
    // Parked in a user iterator under star-unpacking, 40 calls deep.
    "class It:\n    def __init__(self):\n        self.i = 0\n    def __iter__(self):\n        return self\n\
     \x20   def __next__(self):\n        self.i += 1\n        if self.i > 3000:\n            raise StopIteration\n\
     \x20       return self.i\ndef deep(n):\n    if n:\n        return deep(n - 1)\n    a, *b = It()\n\
     \x20   return a, len(b)\nprint(deep(40))\n",
    "import files\n",
];

/// The session with a typed host module.
const TYPED: usize = 6;

/// The engine part of a facade image: what follows the facade's header,
/// without the checksum. It restores, or with `typed` is refused only at
/// the host code (`SESSIONS`).
fn engine_part(image: &[u8], typed: bool) -> &[u8] {
    let body = &image[..image.len() - 8];
    let expected = |r: Result<(), mm_engine::SnapError>| match r {
        Ok(()) => !typed,
        Err(e) => typed && e == mm_engine::SnapError::Corrupt("host code"),
    };
    (44..body.len())
        .map(|o| &body[o..])
        .find(|vm| expected(restore_vm(vm)))
        .expect("an engine image")
}

fn restore_vm(image: &[u8]) -> Result<(), mm_engine::SnapError> {
    mm_engine::Vm::read_snapshot(
        &mut mm_core::Reader::new(image),
        mm_stdlib::MODULES,
        None,
        &mut |_, _, _, _| None,
        COUNT_LIMIT,
    )
    .map(drop)
}

/// A table count is a 32-bit field the reader admits up to its remaining
/// bytes over the size of the table's smallest entry (1 to 36 bytes). One
/// count per entry size bracket: each admitted count asks for more than
/// the bound, given `PADDING`.
const ENTRY_SIZES: [usize; 3] = [8, 14, 36];

/// Bytes after each image, so an admitted count can ask for a table
/// larger than the bound: the restore must charge it before allocating.
const PADDING: usize = 256 << 20;

/// The limit the counts are restored under: large enough that a count the
/// limit's bytes would pay for at one byte per entry asks for more than
/// the bound at the size of an entry (8 bytes or more).
const COUNT_LIMIT: u64 = 32 << 20;

/// Every 32-bit field of each session's VM state (after the heap) set in
/// turn to the largest count the reader admits for each entry size, with
/// `PADDING` bytes after the image; then it and the next field both to a
/// count of three fifths of the bytes the heap leaves, which a table
/// charged at a byte per entry (instead of its entry size) would fit, and
/// which also covers a stack's length and live count together. A count is
/// read before its table's entries, so the restore must refuse it by its
/// charge to the limit, not after allocating the table. The engine image
/// is restored directly (`Vm::read_snapshot`), as the facade does after
/// checking the header and checksum, which would cost a pass over the
/// padding per edit.
fn restore_counts() -> usize {
    let mut failed = 0;
    let engine =
        Engine::new(EngineConfig::new().with_limits(Limits::new().with_memory(COUNT_LIMIT)));
    let bound = memory_meter::Measured::run_bound(COUNT_LIMIT);
    for (k, src) in SESSIONS.iter().enumerate() {
        let mut s = Session::new(&engine).expect("session");
        s.set_output(Box::new(|_| {}));
        s.define(&HostModule {
            constants: vec![
                ("K".into(), HostValue::Int(7)),
                (
                    "rows".into(),
                    HostValue::List(vec![HostValue::Dict(vec![(
                        "row".into(),
                        HostValue::Record {
                            name: "Row".into(),
                            fields: vec![("b".into(), HostValue::Bytes(vec![17; 256]))],
                        },
                    )])]),
                ),
            ],
            ..HostModule::untyped("h", &["ask", "slow"])
        })
        .expect("host module");
        if k == TYPED {
            s.define(&HostModule {
                functions: vec![HostSignature {
                    name: "read".into(),
                    params: vec![HostParam {
                        name: "path".into(),
                        annotation: "str".into(),
                        default: None,
                        kind: ParamKind::PositionalOrKeyword,
                    }],
                    returns: "str".into(),
                    ..HostSignature::default()
                }],
                ..HostModule::untyped("files", &[])
            })
            .expect("typed host module");
        }
        let code = engine.compile(src, "restore.py").expect("compiles");
        s.set_fuel(Some(if matches!(k, 0 | TYPED) {
            u64::MAX
        } else {
            1_500
        }));
        let mut p = s.start(&code);
        while let Progress::HostCall { ref name, .. } = p
            && name == "h.slow"
        {
            p = s.resume(Resume::Future).expect("future");
        }
        let image = s.snapshot().expect("snapshot");
        drop(s);
        let vm = engine_part(&image, k == TYPED);
        let mut r = mm_core::Reader::new(vm);
        let heap = mm_core::Heap::read_snapshot(&mut r, COUNT_LIMIT).expect("heap");
        let state = vm.len() - r.remaining();
        let mid = u32::try_from(heap.available_bytes() * 3 / 5).unwrap_or(u32::MAX);
        drop(heap);
        let mut buf = vm.to_vec();
        buf.resize(vm.len() + PADDING, 0xFF);
        let (mut runs, mut worst) = (0, 0);
        for at in state..vm.len().saturating_sub(4) {
            let old: [u8; 8] = buf[at..at + 8].try_into().expect("8 bytes");
            let rest = buf.len() - at - 4;
            for (n, both) in ENTRY_SIZES
                .map(|size| (u32::try_from(rest / size).unwrap_or(u32::MAX), false))
                .into_iter()
                .chain([(mid, true)])
            {
                buf[at..at + 4].copy_from_slice(&n.to_le_bytes());
                if both {
                    buf[at + 4..at + 8].copy_from_slice(&n.to_le_bytes());
                }
                let (result, peak) = memory_meter::high_water(|| restore_vm(&buf));
                runs += 1;
                worst = worst.max(peak);
                if peak > bound {
                    eprintln!(
                        "session {k}: count {n} at byte {at} of the engine image: heap {peak} > {bound} ({result:?})"
                    );
                    failed += 1;
                }
            }
            buf[at..at + 8].copy_from_slice(&old);
        }
        println!(
            "restore of session {k}: {} bytes of VM state, {runs} counts, at most {:.1} MiB",
            vm.len() - state,
            worst as f64 / f64::from(1 << 20)
        );
    }
    failed
}
