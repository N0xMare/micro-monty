# `cargo xtask bench` on bare-metal x86-64 Linux (2026-11-05)

[M] `QJS=… MONTY=… cargo xtask bench --out-dir bench/results/2026-11-05-host1-ryzen-x86_64-linux`,
10 timed reps after one warm-up, pinned with `taskset -c 2`. The orb results
in `bench/results/*.txt` (2026-11-04, Xeon VM) are left as they were.

- Host "host1": AMD Ryzen 9 8945HS (Zen 4, 8 cores / 16 threads), 28 GiB,
  Ubuntu 24.04.5, Linux 7.0.0-42-generic, bare metal, `performance`
  governor, boost on. Idle during the run (load average < 0.2 before it).
- micro-monty at `origin/main` 06f1fbc (`target/release/mm`; the engine is
  the same at 5568b27). CPython 3.14.7 (uv). QuickJS: dquickts `f9a3efa`,
  `make qjs` (version 2026-06-04), as in docs/history/handoff-0.0.1.md. Monty:
  `pydantic-monty-runtime` 1.4.0 (`monty` CLI).
- Monty fails `generators` and `exceptions` (its parser lacks `yield` and
  class inheritance), as on the orb; the run therefore exits nonzero with
  every other cell retained.

Geomean over the 12 programs (startup excluded) [M]: micro-monty/CPython
1.09 (orb 2026-11-04: 1.11), micro-monty/QuickJS 1.07 (1.08),
micro-monty/Monty 0.39 (0.40). Every engine ran in 0.67–0.69× the orb's time.
