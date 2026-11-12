# Soak: many sessions on one shared Engine

Not a gate. A long-running check that one facade `Engine` shared across
threads behaves like a fresh, uncached engine for each session.
The shared `Engine` means one decoded-module cache (1c03cff, scope
decision 45). The soak runs sessions parked, snapshotted, restored and moved
between threads, and checks that memory stays flat.

```bash
python3.14 -I bench/evidence/soak/prepare.py /tmp/soak/workload.jsonl     # once per corpus change
cargo build --release --manifest-path bench/evidence/soak/Cargo.toml --target-dir target/soak
target/soak/release/soak /tmp/soak/workload.jsonl /tmp/soak/native --minutes 60 --threads 6

cargo xtask wasm-host      # builds the shipped guest
cargo build --release --manifest-path bench/evidence/soak/wasm/Cargo.toml --target-dir target/soak-wasm
target/soak-wasm/release/soak-wasm target/wasm32-unknown-unknown/shipped/mm_wasm_guest.wasm \
    /tmp/soak/workload.jsonl /tmp/soak/wasm --minutes 60 --threads 3
```

## Workload

- **Codemode solutions.** All 1,793 in `conformance/codemode/solutions`,
  probes included. `prepare.py` runs each one once through the `full`
  mm-codemode runner and records its tool replies. The soak replays those
  replies in order, in-process, with no Python tool host.
- **Own cases.** Every `conformance/own` case, run as `mm run` runs it.
- **Imports.** One cell per bundled module (from `Engine::environment`)
  that imports it and prints its public names. One more cell imports all
  of them.

## Memory limits

Half the jobs run at the default limit (256 MiB). The other half run at
16, 4, 3, 2, 1.5 or 1 MiB (`--limits KIB,..`), so MemoryError hits during
imports as well as during runs.

A lower limit comes from restoring, on the shared engine, an empty session
saved by an engine with that limit. A restored session keeps the smaller
limit, so every session still uses the one shared engine and its cache.

## What a job does

Each job runs in fuel slices: unparked, or about 5, 50, 300 or 1,000 parks.
At random parks, host calls and waits it is saved and restored on the
shared engine. At random parks it goes back to a shared queue, and any
thread may continue it.

## Checks

1. **Same schedule, uncached engine.** The job's schedule is replayed in a
   session on a new uncached engine (`Engine::with_module_cache(_, 0)`):
   the same fuel slices and the same save-and-restore decisions. The
   session is made the same way. The transcript must be identical: stdout,
   stderr, the end and every host call. Any difference is the cache's or
   the threads' doing.
2. **Unparked run.** The item at the same limit on a new uncached engine,
   with no parks, must give the same transcript.
   - Below the default limit, a snapshot collects garbage. That moves
     where a collection happens, which can decide whether a MemoryError
     hits at the edge of the limit.
   - So a difference there that involves a MemoryError is counted
     (`memory_edge`), not reported.

A difference writes `OUT/diff-NNN.json`, with the job's schedule and both
transcripts.

`log.csv` gets one line per interval with these columns:

- jobs, same, differ;
- parks, snapshots and their total size;
- refused snapshots and failed restores;
- migrations, MemoryErrors, memory-edge cases;
- RSS and its high-water mark;
- the cache's bytes and its cap.

## Probes

The options below reproduce a difference single-threaded:

- `--probe ID|file:PATH --level L`: one item, in every way a session can be
  made: new or restored from an image, on a cached or uncached engine,
  parked or unparked.
- `--probe-schedules ID --level L --runs N`: random schedules on a cold
  cached engine, a warm cached one and an uncached one.
- `--probe-replay ID --level L --decisions 0101.. [--slice N]`: one schedule.
- `--only A,B`: restricts the workload to item ids containing A or B.

## soak-wasm

`wasm/` runs the same workload on the mm-wasm guest through
`crates/mm-wasm-host`. There is one runtime, and each session runs in its
own instance, so each has its own module cache.

Saving and restoring goes into a new instance. The reference is an
unparked run in a new instance.

The reference host's clock and entropy are real, so the wasm sessions run
without them. In those sessions, `time.time()` and `os.urandom` raise.
