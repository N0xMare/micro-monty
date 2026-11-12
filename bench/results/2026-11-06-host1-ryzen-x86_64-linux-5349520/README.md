# `cargo xtask bench` on bare-metal x86-64 Linux at the final head (2026-11-06)

[M] `QJS=… MONTY=… cargo xtask bench --out-dir …` ran 10 timed reps after
one warm-up, pinned with `taskset -c 2`. The run was after the 24 h fuzz
campaign and the closure A/B finished, with host1 otherwise idle. The orb
results in `bench/results/*.txt` (2026-11-04, Xeon VM) and the 2026-11-05
host1 run at 06f1fbc are left as they were.

- Host "host1": AMD Ryzen 9 8945HS (Zen 4, 8 cores / 16 threads), 28 GiB,
  Ubuntu 24.04.5, Linux 7.0.0-42-generic, bare metal, `performance`
  governor, boost on.
- micro-monty at **5349520**, main at the time of the run. It includes the
  final-head commits c97ce06 (size pass) and bb34446 (repr/text-limit
  fixes). It was built as `target/release/mm` in a worktree with the
  pinned toolchain, rustc 1.99.0. CPython 3.14.7 (uv). QuickJS: dquickts
  `make qjs` (version 2026-06-04). Monty: `pydantic-monty-runtime` 1.4.0.
  The binaries' SHA-256 hashes are in `manifest.json`.
- Monty fails `generators` and `exceptions`, as before (its parser lacks
  `yield` and class inheritance), so the run exits nonzero with every other
  cell retained.

Geomean over the 12 programs, startup excluded [M]:

| ratio | 06f1fbc (2026-11-05) | 5349520 (this run) |
|---|---|---|
| micro-monty / CPython | 1.090 | **1.008** |
| micro-monty / QuickJS | 1.07 | **0.977** |
| micro-monty / Monty (10 programs) | 0.39 | **0.351** |
| micro-monty, 5349520 / 06f1fbc | | **0.922** |
| CPython, this run / previous run | | 0.998 (the machine's speed did not change) |

Per program, micro-monty 5349520 / 06f1fbc [M]:

- Gains: generators 0.75, classes 0.80, interp 0.81, fmt 0.82 and
  sort 0.90.
- Within ±3%: loop 0.98, dict 1.00, json 1.00, closure 1.00, str 1.01 and
  exceptions 1.03.
- fib 1.05: 15.9 ms against 15.2 ms. The closure A/B shows fib 2% faster
  at 5349520 than at e6d7251, so this change predates e6d7251 [E].

micro-monty is still slower than CPython on generators (2.14×), interp
(2.03×), fmt (1.78×), exceptions (1.29×), sort (1.24×), str (1.06×) and
classes (1.05×). It is faster on loop (0.32×), closure (0.54×), json
(0.60×), fib (0.82×) and dict (0.93×). Startup is 1.1 ms, against 7.1 ms for
CPython.
