# `cargo xtask bench` on Apple Silicon macOS at 5349520 (2026-11-06)

[M] `QJS=… MONTY=… cargo xtask bench --out-dir …` ran 10 timed reps after
one warm-up, from 16:54:28Z to 16:55:15Z. The method is the same as in
`../2026-11-06-mac-m1max-aarch64-macos-c97ce06/`. The run is **not
pinned**: macOS has no `taskset`, and the summary's "unknown CPU, nproc ?"
comes from xtask reading `lscpu`/`nproc`.

- Host: MacBook Pro, Apple M1 Max (8 performance and 2 efficiency cores),
  32 GiB, macOS 27.2, AC power, Rust 1.99.0.
- micro-monty at **5349520**, current main and the final head for
  report 3. It includes c97ce06 (the size pass) and bb34446 (the repr and
  text-limit fixes).
- Other engines: CPython 3.14.7 (uv), QuickJS dquickts (2026-06-04), and
  Monty `pydantic-monty-runtime` 1.4.0.
- Monty still fails `generators` and `exceptions`, so xtask exits 1.
- **Load.** No `cargo` or `rustc` process ran during the run. The owner's
  long-running `plak --resume` session used a steady 35–40% of one core
  throughout, and the load average was about 4. The gate used for the
  c97ce06 run (plak under 20%) never opened, so this run allowed plak up to
  60% of one core.

Geomean over the 12 programs, startup excluded [M]:

| ratio | c97ce06 | 5349520 |
|---|---|---|
| micro-monty / CPython | 0.855 | **0.840** |
| micro-monty / QuickJS | | **1.030** |
| micro-monty / Monty (10 programs) | | **0.477** |
| micro-monty, 5349520 / c97ce06 | | **0.987** |
| CPython, this run / c97ce06 run | | 1.006 |

The engine changes between c97ce06 and 5349520 are small. Callgrind Ir
moved within ±0.02% at bb34446 [S, coordinating thread]. Every program is
within −3% to 0% of the c97ce06 run, so that difference is run-to-run noise
on an unpinned machine [E].

On the Mac, micro-monty beats CPython on 8 of 12 programs. It is slower on
interp (1.79×), generators (1.73×), fmt (1.32×) and sort (1.08×). Startup is 2.5 ms,
against 13.5 ms for CPython.
