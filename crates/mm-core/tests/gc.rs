//! GC and arena property tests: random operation sequences checked against a
//! model (`support/ops.rs`), deep graphs, steady-state churn, memory limits.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::cast_possible_truncation,
    reason = "tests"
)]

#[path = "support/ops.rs"]
mod ops;

use mm_core::{Error, Heap, HeapConfig, Ref, SEGMENT_BYTES, Value, types};

/// `SplitMix64`: deterministic input for the op interpreter.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    fn bytes(&mut self, n: usize) -> Vec<u8> {
        (0..n).map(|_| self.next() as u8).collect()
    }
}

const T: u16 = types::FIRST_ENGINE;

fn run_seeds(seeds: std::ops::Range<u64>, len: usize) -> ops::Summary {
    let mut total = ops::Summary::default();
    for seed in seeds {
        let data = Rng(seed).bytes(len);
        let s = ops::run(&data);
        total.ops += s.ops;
        total.collections += s.collections;
        total.oom += s.oom;
        total.max_committed = total.max_committed.max(s.max_committed);
    }
    total
}

#[test]
fn random_operation_sequences() {
    let s = run_seeds(0..64, 30_000);
    assert!(s.ops > 150_000, "{s:?}");
    assert!(s.collections > 1_000, "{s:?}");
    assert!(s.oom > 0, "{s:?}");
}

/// Several million operations; run in release:
/// `cargo test --release -p mm-core --test gc -- --ignored --nocapture`.
#[test]
#[ignore = "long; run in release"]
fn random_operation_sequences_long() {
    let start = std::time::Instant::now();
    let s = run_seeds(1000..1800, 80_000);
    println!(
        "[M] op interpreter: {} ops, {} collections, {} OOM refusals, max committed {} bytes, {:.1} s",
        s.ops,
        s.collections,
        s.oom,
        s.max_committed,
        start.elapsed().as_secs_f64()
    );
    assert!(s.ops > 5_000_000);
}

/// Allocates a singly linked chain of `n` objects `[next, payload]`, returns
/// its head.
fn chain(heap: &mut Heap, n: u32) -> Ref {
    let mut link = Value::NONE;
    let mut last = Ref::from_raw(0);
    for i in 0..n {
        last = heap.alloc_init(T, 0, &[link], &[u64::from(i)]).unwrap();
        link = Value::from_ref(last, 0);
    }
    last
}

#[test]
fn deep_chain_marks_without_recursion() {
    const N: u32 = 1_000_000;
    let mut heap = Heap::new(HeapConfig::default());
    let first = chain(&mut heap, N);
    let root = Value::from_ref(first, 0);
    let stats = heap.collect(&mut |t| t.value(root));
    assert_eq!(stats.live_bytes, u64::from(N) * 3 * 8);
    assert_eq!(stats.freed_bytes, 0);
    // Walk the chain: every payload intact.
    let mut cur = Some(first);
    let mut expect = N;
    while let Some(r) = cur {
        expect -= 1;
        assert_eq!(heap.get_raw(r, 1), Ok(u64::from(expect)));
        cur = heap.get(r, 0).unwrap().as_obj();
    }
    assert_eq!(expect, 0);
    let stats = heap.collect(&mut |_| {});
    assert_eq!(stats.live_bytes, 0);
    assert_eq!(stats.freed_bytes, u64::from(N) * 3 * 8);
    assert_eq!(heap.header(first), Err(Error::BadRef));
}

#[test]
fn wide_object_and_cycles() {
    let mut heap = Heap::new(HeapConfig::default());
    let list = heap.alloc(T, 0, 200_000, 0).unwrap();
    for i in 0..200_000u32 {
        let o = heap
            .alloc_init(T, 0, &[Value::from_ref(list, 0)], &[])
            .unwrap();
        heap.set(list, i, Value::from_ref(o, 1)).unwrap();
    }
    let root = Value::from_ref(list, 0);
    let stats = heap.collect(&mut |t| t.value(root));
    assert_eq!(stats.live_bytes, (200_001 + 200_000 * 2) * 8);
    // Cycles through the list die together.
    let stats = heap.collect(&mut |_| {});
    assert_eq!(stats.live_bytes, 0);
    assert_eq!(heap.committed_bytes(), SEGMENT_BYTES + 32 * 1024);
}

#[test]
fn object_prefix_traces_only_the_prefix() {
    let mut heap = Heap::new(HeapConfig::default());
    let a = heap.alloc(T, 0, 0, 1).unwrap();
    let b = heap.alloc(T, 0, 0, 1).unwrap();
    let frames = heap
        .alloc_init(T, 0, &[Value::from_ref(a, 0), Value::from_ref(b, 0)], &[])
        .unwrap();
    heap.collect(&mut |t| t.object_prefix(frames, 1));
    assert!(heap.header(frames).is_ok());
    assert!(heap.header(a).is_ok());
    assert_eq!(heap.header(b), Err(Error::BadRef));
}

/// Steady churn: a fixed-size live set, random sizes, collect whenever the
/// heap asks. Committed memory must plateau.
#[test]
fn churn_reaches_a_steady_state() {
    const ROUNDS: usize = 2_000_000;
    let mut heap = Heap::new(HeapConfig::default());
    let mut rng = Rng(7);
    let mut live = vec![Value::NONE; 20_000];
    let mut peak_first_half = 0;
    let mut peak_second_half = 0;
    for round in 0..ROUNDS {
        let x = rng.next();
        let v = if x.is_multiple_of(7) {
            let b = heap.alloc_bytes_zeroed((x >> 8) as usize % 400).unwrap();
            Value::from_byte_ref(b)
        } else {
            let nvals = (x >> 8) % 6;
            let nraw = (x >> 16) % 30;
            let other = live[(x >> 24) as usize % live.len()];
            let r = heap.alloc(T, 0, nvals as u32 + 1, nraw as u32).unwrap();
            heap.set(r, 0, other).unwrap();
            Value::from_ref(r, 0)
        };
        let slot = (x >> 40) as usize % live.len();
        live[slot] = v;
        if heap.should_collect() {
            let roots = live.clone();
            heap.collect(&mut |t| t.values(&roots));
        }
        let c = heap.committed_bytes();
        if round < ROUNDS / 2 {
            peak_first_half = peak_first_half.max(c);
        } else {
            peak_second_half = peak_second_half.max(c);
        }
    }
    let stats = heap.stats();
    assert!(stats.collections > 10, "{stats:?}");
    assert!(
        peak_second_half <= peak_first_half,
        "committed kept growing: {peak_first_half} then {peak_second_half}"
    );
}

#[test]
fn memory_limit_fails_cleanly_and_recovers() {
    let limit = 4 * (SEGMENT_BYTES + 32 * 1024);
    let mut heap = Heap::new(HeapConfig {
        memory_limit: limit,
        ..HeapConfig::default()
    });
    let mut keep = Vec::new();
    let err = loop {
        match heap.alloc(T, 0, 3, 0) {
            Ok(r) => keep.push(r),
            Err(e) => break e,
        }
    };
    assert_eq!(err, Error::OutOfMemory);
    assert!(heap.committed_bytes() <= limit);
    // Any allocated object can be a root. The limit must leave room for a
    // u32 mark-stack entry per object, even when all roots are handed in
    // before their children are scanned.
    assert!(keep.len() as u64 * (32 + 4) <= limit);
    assert!(!heap.would_fit(SEGMENT_BYTES));
    // The byte space's first segment is small, so a little fits; more than
    // the rest of the budget does not.
    let rest = heap.available_bytes() as usize;
    assert!(!heap.would_fit(rest as u64));
    assert_eq!(heap.alloc_bytes_zeroed(rest), Err(Error::OutOfMemory));
    assert_eq!(
        heap.alloc(T, 0, (SEGMENT_BYTES / 8) as u32, 0),
        Err(Error::OutOfMemory)
    );
    // Every object is intact after the failures.
    for (i, &r) in keep.iter().enumerate().step_by(997) {
        assert_eq!(heap.get(r, 0), Ok(Value::UNDEFINED), "object {i}");
    }
    // Keep half: freed space is reused without new segments.
    let roots: Vec<Value> = keep
        .iter()
        .step_by(2)
        .map(|&r| Value::from_ref(r, 0))
        .collect();
    let stats = heap.collect(&mut |t| t.values(&roots));
    assert_eq!(stats.live_bytes, roots.len() as u64 * 32);
    let committed = heap.committed_bytes();
    for _ in 0..roots.len() {
        heap.alloc(T, 0, 3, 0).unwrap();
    }
    assert_eq!(heap.committed_bytes(), committed);
    assert_eq!(heap.alloc(T, 0, 3, 0), Err(Error::OutOfMemory));
    // Drop everything: a big allocation fits again.
    heap.collect(&mut |_| {});
    assert!(heap.would_fit(2 * SEGMENT_BYTES));
    let big = heap.alloc_bytes_zeroed(2 * SEGMENT_BYTES as usize).unwrap();
    assert_eq!(heap.bytes(big).unwrap().len(), 2 * SEGMENT_BYTES as usize);
    assert!(!heap.would_fit(3 * SEGMENT_BYTES));
}

#[test]
fn should_collect_follows_the_ratio() {
    let mut heap = Heap::new(HeapConfig {
        gc_min_bytes: 1 << 20,
        gc_ratio_percent: 200,
        ..HeapConfig::default()
    });
    assert!(!heap.should_collect());
    // 2 MiB of 32-byte objects is the threshold over a 1 MiB floor.
    let mut keep = Vec::new();
    while !heap.should_collect() {
        keep.push(Value::from_ref(heap.alloc(T, 0, 3, 0).unwrap(), 0));
    }
    assert_eq!(keep.len() as u64 * 32, (2 << 20) + 32);
    let stats = heap.collect(&mut |t| t.values(&keep));
    assert_eq!(stats.allocated_since_gc, 0);
    assert!(!heap.should_collect());
}
