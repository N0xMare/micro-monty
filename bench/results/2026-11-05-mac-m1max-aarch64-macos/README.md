# `cargo xtask bench` on Apple Silicon macOS (2026-11-05)

[M] `QJS=… MONTY=… cargo xtask bench --out-dir bench/results/2026-11-05-mac-m1max-aarch64-macos`,
10 timed reps after one warm-up. **Not pinned**: macOS has no `taskset`, and
the summary's machine line reads "unknown CPU, nproc ?" because xtask reads
`lscpu`/`nproc`, which macOS lacks.

- Host: MacBook Pro, Apple M1 Max (8 performance + 2 efficiency cores),
  32 GiB, macOS 27.2 (26B123), AC power, Low Power Mode off. **Not idle**:
  the owner's desktop apps (plak, a browser, terminal) kept the load
  average near 4 throughout; nothing else from this work ran alongside.
- micro-monty at 5568b27 (engine identical to 06f1fbc), aarch64-apple-darwin.
  CPython 3.14.7 (uv). QuickJS dquickts `f9a3efa` (`make qjs`). Monty
  `pydantic-monty-runtime` 1.4.0. Monty fails `generators` and `exceptions`
  as on Linux.

Geomean over the 12 programs (startup excluded) [M]: micro-monty/CPython
0.92, micro-monty/QuickJS 1.11, micro-monty/Monty 0.51. Against host1:
micro-monty 1.11×, QuickJS 1.06×, CPython 1.31× the time.
