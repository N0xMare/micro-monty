# `cargo xtask bench` on Apple Silicon macOS at the final head c97ce06 (2026-11-06)

[M] `QJS=… MONTY=… cargo xtask bench --out-dir bench/results/2026-11-06-mac-m1max-aarch64-macos-c97ce06`,
10 timed reps after one warm-up, run 00:02:18–00:03:06Z. Same method as
`../2026-11-05-mac-m1max-aarch64-macos/` (5568b27). **Not pinned**: macOS
has no `taskset`, and the summary's machine line reads "unknown CPU, nproc ?"
because xtask reads `lscpu`/`nproc`, which macOS lacks.

- Host: MacBook Pro, Apple M1 Max (8 performance + 2 efficiency cores),
  32 GiB, macOS 27.2 (26B123), AC power. Rust 1.99.0.
- micro-monty at c97ce06 (the size pass on top of the speed landing, the
  asyncio primitives and the aware-datetime fast paths), aarch64-apple-darwin.
  CPython 3.14.7 (uv). QuickJS dquickts `f9a3efa`. Monty
  `pydantic-monty-runtime` 1.4.0, which still fails `generators` (no `yield`
  in its parser) and `exceptions`, so xtask exits 1 with every other cell
  recorded.
- **Load**: the run started only after 30 s with no `cargo` or `rustc`
  process, and none ran during it (a 10 s sampler, `contention.log` in
  `bench/evidence/isolation/results/2026-11-06-soak-40-sessions-final/`).
  Load average 10–14 from the owner's desktop apps (a browser, plak).

## Geomeans over the 12 programs (startup excluded) [M]

| | 5568b27 (2026-11-05) | c97ce06 (this run) |
|---|---|---|
| micro-monty / CPython | 0.923 | **0.855** |
| micro-monty / QuickJS | 1.113 | **1.030** |
| micro-monty / Monty (10 programs) | 0.515 | **0.481** |
| micro-monty c97ce06 / 5568b27 | | **0.921** |
| CPython this run / 2026-11-05 (the noise floor) | | 0.993 |

## micro-monty per program, median ms [M]

| program | 5568b27 | c97ce06 | ratio | CPython ratio (same runs) |
|---|---:|---:|---:|---:|
| startup | 2.7 | 2.8 | 1.04 | 1.01 |
| fib | 23.5 | 23.2 | 0.99 | 0.98 |
| loop | 51.2 | 49.4 | 0.97 | 0.96 |
| dict | 67.5 | 69.7 | 1.03 | 1.02 |
| str | 29.2 | 29.3 | 1.00 | 1.01 |
| json | 64.8 | 67.4 | 1.04 | 1.06 |
| closure | 37.5 | 35.7 | 0.95 | 0.97 |
| classes | 92.0 | 71.3 | **0.78** | 0.97 |
| generators | 170.4 | 127.8 | **0.75** | 0.99 |
| exceptions | 58.2 | 57.9 | 0.99 | 0.98 |
| fmt | 119.4 | 98.2 | **0.82** | 0.98 |
| sort | 90.7 | 89.8 | 0.99 | 1.03 |
| interp | 173.2 | 140.2 | **0.81** | 0.98 |

The gains are where the speed landing aimed: the generator fast path, `Ic::New`
for `cls(...)`, and the str/format paths. Ratios within about ±4% match
CPython's movement between the two days and are noise. `closure` has its own
interleaved A/B in `../2026-11-06-closure-ab/`.

The host1 (x86-64 Linux, pinned) run at c97ce06 follows when its fuzz
campaign has finished.
