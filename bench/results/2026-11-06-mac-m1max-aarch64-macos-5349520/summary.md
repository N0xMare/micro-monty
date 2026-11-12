# Benchmarks

[M] `cargo xtask bench`: median / min wall-clock ms per process (startup included), after one untimed warm-up run; balanced rotating engine order. Each engine's file records its rep count. Checksums are checked on every run. Programs in `bench/` (docs/spec.md §12). Failed cells have no timing. Only engines measured in this invocation appear.

Run on unknown CPU, nproc ?, Darwin 27.2.0; 2026-11-06; not pinned (no taskset or fewer than 3 CPUs).

| Program | CPython 3.14.7 | qjs QuickJS version 2026-06-04 | monty-runtime 1.4.0 | micro-monty 0.0.0 |
|---|---:|---:|---:|---:|
| startup | 13.5 / 12.7 | 3.1 / 2.6 | 4.8 / 4.0 | 2.5 / 2.2 |
| fib | 29.5 / 28.9 | 16.4 / 15.9 | 58.3 / 57.8 | 22.8 / 22.0 |
| loop | 182.6 / 160.5 | 43.7 / 43.1 | 114.8 / 113.8 | 48.4 / 48.0 |
| dict | 84.8 / 81.5 | 64.1 / 62.0 | 97.7 / 94.9 | 67.9 / 66.1 |
| str | 34.9 / 32.4 | 23.7 / 23.1 | 35.6 / 34.4 | 29.3 / 28.8 |
| json | 154.5 / 142.7 | 121.3 / 119.9 | 132.6 / 123.0 | 67.1 / 66.4 |
| closure | 71.6 / 69.9 | 29.6 / 29.3 | 116.3 / 115.1 | 35.4 / 35.1 |
| classes | 73.2 / 71.9 | 53.0 / 52.7 | 300.3 / 298.3 | 70.6 / 70.0 |
| generators | 72.9 / 71.8 | 82.5 / 81.8 | failed | 125.9 / 125.2 |
| exceptions | 59.2 / 58.0 | 138.9 / 138.4 | failed | 57.4 / 56.7 |
| fmt | 73.0 / 72.4 | 51.5 / 50.9 | 117.1 / 115.9 | 96.7 / 96.2 |
| sort | 80.9 / 75.2 | 155.6 / 149.1 | 205.0 / 190.8 | 87.6 / 82.9 |
| interp | 77.8 / 76.3 | 115.9 / 114.2 | 276.3 / 274.2 | 139.4 / 138.3 |

Failures on monty-runtime 1.4.0 (not inferred to be unsupported):

- exceptions: `exceptions on monty-runtime 1.4.0: expected `33334 99995 25063333 100000 50000`, got `` (exit status: 1): error:`
- generators: `generators on monty-runtime 1.4.0: expected `67499550000 138450000 299995 100`, got `` (exit status: 1): error:`
