# Benchmarks

[M] `cargo xtask bench`: median / min wall-clock ms per process (startup included), after one untimed warm-up run; balanced rotating engine order. Each engine's file records its rep count. Checksums are checked on every run. Programs in `bench/` (docs/spec.md §12). Failed cells have no timing. Only engines measured in this invocation appear.

Run on unknown CPU, nproc ?, Darwin 27.2.0; 2026-11-05; not pinned (no taskset or fewer than 3 CPUs).

| Program | CPython 3.14.7 | qjs QuickJS version 2026-06-04 | monty-runtime 1.4.0 | micro-monty 0.0.0 |
|---|---:|---:|---:|---:|
| startup | 14.5 / 13.7 | 3.1 / 2.9 | 5.0 / 4.4 | 2.7 / 2.5 |
| fib | 30.8 / 30.3 | 17.4 / 16.3 | 61.5 / 60.8 | 23.5 / 22.6 |
| loop | 180.7 / 167.5 | 45.7 / 45.2 | 119.4 / 118.4 | 51.2 / 49.7 |
| dict | 83.8 / 80.5 | 62.5 / 61.0 | 97.0 / 96.1 | 67.5 / 66.8 |
| str | 34.1 / 33.4 | 23.8 / 23.7 | 35.7 / 34.8 | 29.2 / 28.6 |
| json | 137.3 / 133.5 | 122.3 / 119.5 | 128.8 / 125.7 | 64.8 / 63.7 |
| closure | 74.9 / 72.9 | 30.4 / 29.8 | 120.0 / 119.3 | 37.5 / 36.9 |
| classes | 76.7 / 72.9 | 55.0 / 54.3 | 311.1 / 309.0 | 92.0 / 91.4 |
| generators | 74.1 / 73.3 | 85.5 / 84.2 | failed | 170.4 / 168.9 |
| exceptions | 60.8 / 59.7 | 143.2 / 142.3 | failed | 58.2 / 57.7 |
| fmt | 74.6 / 73.3 | 52.6 / 52.2 | 120.5 / 119.8 | 119.4 / 117.9 |
| sort | 77.6 / 76.0 | 148.4 / 145.7 | 183.0 / 178.1 | 90.7 / 88.5 |
| interp | 79.1 / 77.8 | 117.8 / 116.3 | 281.6 / 279.4 | 173.2 / 171.5 |

Failures on monty-runtime 1.4.0 (not inferred to be unsupported):

- exceptions: `exceptions on monty-runtime 1.4.0: expected `33334 99995 25063333 100000 50000`, got `` (exit status: 1): error:`
- generators: `generators on monty-runtime 1.4.0: expected `67499550000 138450000 299995 100`, got `` (exit status: 1): error:`
