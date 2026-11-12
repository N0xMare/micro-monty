# Benchmarks

[M] `cargo xtask bench`: median / min wall-clock ms per process (startup included), after one untimed warm-up run; balanced rotating engine order. Each engine's file records its rep count. Checksums are checked on every run. Programs in `bench/` (docs/spec.md §12). Failed cells have no timing. Only engines measured in this invocation appear.

Run on unknown CPU, nproc ?, Darwin 27.2.0; 2026-11-06; not pinned (no taskset or fewer than 3 CPUs).

| Program | CPython 3.14.7 | qjs QuickJS version 2026-06-04 | monty-runtime 1.4.0 | micro-monty 0.0.0 |
|---|---:|---:|---:|---:|
| startup | 14.6 / 14.3 | 3.2 / 2.9 | 5.2 / 4.9 | 2.8 / 2.6 |
| fib | 30.2 / 30.0 | 17.0 / 16.7 | 60.0 / 59.7 | 23.2 / 22.9 |
| loop | 174.2 / 163.8 | 44.5 / 44.4 | 116.7 / 115.7 | 49.4 / 49.1 |
| dict | 85.1 / 83.2 | 64.3 / 63.2 | 98.5 / 96.5 | 69.7 / 68.9 |
| str | 34.3 / 33.6 | 23.4 / 23.1 | 35.5 / 35.1 | 29.3 / 29.0 |
| json | 145.1 / 141.0 | 122.2 / 121.8 | 132.9 / 126.5 | 67.4 / 66.8 |
| closure | 72.5 / 71.4 | 30.0 / 29.5 | 117.6 / 117.0 | 35.7 / 35.3 |
| classes | 74.6 / 72.2 | 53.7 / 53.5 | 304.7 / 303.8 | 71.3 / 71.0 |
| generators | 73.3 / 72.8 | 83.7 / 83.2 | failed | 127.8 / 127.3 |
| exceptions | 59.3 / 58.7 | 141.1 / 140.5 | failed | 57.9 / 57.8 |
| fmt | 73.4 / 71.8 | 51.9 / 51.7 | 118.1 / 117.3 | 98.2 / 97.2 |
| sort | 80.0 / 77.6 | 161.5 / 154.1 | 194.9 / 190.4 | 89.8 / 86.1 |
| interp | 77.8 / 76.5 | 116.2 / 115.2 | 276.2 / 275.0 | 140.2 / 139.6 |

Failures on monty-runtime 1.4.0 (not inferred to be unsupported):

- exceptions: `exceptions on monty-runtime 1.4.0: expected `33334 99995 25063333 100000 50000`, got `` (exit status: 1): error:`
- generators: `generators on monty-runtime 1.4.0: expected `67499550000 138450000 299995 100`, got `` (exit status: 1): error:`
