//! Allocation and collection timings for the M1 gate (docs/spec.md §14:
//! 1M small allocations in under 50 ms). Run in release:
//!
//! ```bash
//! cargo run --release -p mm-core --example gc_bench
//! ```

use mm_core::{Error, Heap, HeapConfig, Value, types};
use std::time::{Duration, Instant};

const N: u32 = 1_000_000;
const REPS: usize = 7;
const T: u16 = types::FIRST_ENGINE;

fn config() -> HeapConfig {
    HeapConfig {
        memory_limit: 1 << 30,
        ..HeapConfig::default()
    }
}

/// Median and minimum of `REPS` runs of `f`, which returns the time it
/// measured.
fn measure(name: &str, mut f: impl FnMut() -> Result<Duration, Error>) -> Result<(), Error> {
    let mut times = Vec::with_capacity(REPS);
    for _ in 0..REPS {
        times.push(f()?);
    }
    times.sort();
    println!(
        "[M] {name}: median {:.2} ms, min {:.2} ms ({REPS} runs)",
        times[REPS / 2].as_secs_f64() * 1e3,
        times[0].as_secs_f64() * 1e3
    );
    Ok(())
}

/// `N` 3-slot objects, each pointing at the previous one.
fn alloc_chain(heap: &mut Heap) -> Result<Value, Error> {
    let mut prev = Value::NONE;
    for i in 0..N {
        let r = heap.alloc(T, 0, 3, 0)?;
        heap.set(r, 0, prev)?;
        heap.set(r, 1, Value::int(i64::from(i)).unwrap_or(Value::NONE))?;
        prev = Value::from_ref(r, 0);
    }
    Ok(prev)
}

fn main() -> Result<(), Error> {
    alloc_benches()?;
    gc_benches()?;
    intern_bench()
}

fn alloc_benches() -> Result<(), Error> {
    measure("1M 3-slot allocations, fresh heap", || {
        let mut heap = Heap::new(config());
        let t = Instant::now();
        for _ in 0..N {
            std::hint::black_box(heap.alloc(T, 0, 3, 0)?);
        }
        Ok(t.elapsed())
    })?;

    measure(
        "1M 3-slot allocations, reusing free lists after a GC",
        || {
            let mut heap = Heap::new(config());
            // Free every other object so the space comes from the free lists.
            let mut keep = Vec::with_capacity(N as usize);
            for i in 0..2 * N {
                let r = heap.alloc(T, 0, 3, 0)?;
                if i % 2 == 0 {
                    keep.push(Value::from_ref(r, 0));
                }
            }
            heap.collect(&mut |t| t.values(&keep));
            let committed = heap.committed_bytes();
            let t = Instant::now();
            for _ in 0..N {
                std::hint::black_box(heap.alloc(T, 0, 3, 0)?);
            }
            let elapsed = t.elapsed();
            assert_eq!(
                heap.committed_bytes(),
                committed,
                "allocations grew the heap"
            );
            Ok(elapsed)
        },
    )?;

    measure("1M allocations + 2 slot writes each (linked chain)", || {
        let mut heap = Heap::new(config());
        let t = Instant::now();
        std::hint::black_box(alloc_chain(&mut heap)?);
        Ok(t.elapsed())
    })?;

    Ok(())
}

fn gc_benches() -> Result<(), Error> {
    measure("full GC, 1M live objects (chain)", || {
        let mut heap = Heap::new(config());
        let root = alloc_chain(&mut heap)?;
        let t = Instant::now();
        let stats = heap.collect(&mut |t| t.value(root));
        let elapsed = t.elapsed();
        assert_eq!(stats.live_bytes, u64::from(N) * 32);
        Ok(elapsed)
    })?;

    measure(
        "full GC, 1M live objects (wide: one list of 1M refs)",
        || {
            let mut heap = Heap::new(config());
            let list = heap.alloc(T, 0, N, 0)?;
            for i in 0..N {
                let r = heap.alloc(T, 0, 3, 0)?;
                heap.set(list, i, Value::from_ref(r, 0))?;
            }
            let root = Value::from_ref(list, 0);
            let t = Instant::now();
            let stats = heap.collect(&mut |t| t.value(root));
            let elapsed = t.elapsed();
            assert_eq!(stats.live_bytes, (u64::from(N) + 1) * 8 + u64::from(N) * 32);
            Ok(elapsed)
        },
    )?;

    measure("full GC, 1M objects, 1% live (mostly dead heap)", || {
        let mut heap = Heap::new(config());
        let mut keep = Vec::new();
        for i in 0..N {
            let r = heap.alloc(T, 0, 3, 0)?;
            if i % 100 == 0 {
                keep.push(Value::from_ref(r, 0));
            }
        }
        let t = Instant::now();
        let stats = heap.collect(&mut |t| t.values(&keep));
        let elapsed = t.elapsed();
        assert_eq!(stats.live_bytes, u64::from(N / 100) * 32);
        Ok(elapsed)
    })?;

    measure("full GC, 1M objects, all dead", || {
        let mut heap = Heap::new(config());
        for _ in 0..N {
            heap.alloc(T, 0, 3, 0)?;
        }
        let t = Instant::now();
        heap.collect(&mut |_| {});
        Ok(t.elapsed())
    })?;

    Ok(())
}

fn intern_bench() -> Result<(), Error> {
    measure("100k intern lookups of 1k distinct names", || {
        let mut heap = Heap::new(config());
        let names: Vec<String> = (0..1000).map(|i| format!("attribute_{i}")).collect();
        for n in &names {
            heap.intern(n)?;
        }
        let t = Instant::now();
        for i in 0..100_000 {
            std::hint::black_box(heap.intern(&names[i % 1000])?);
        }
        Ok(t.elapsed())
    })
}
