# Benchmarks

[M] `cargo xtask bench`: median / min wall-clock ms per process (startup included), after one untimed warm-up run; balanced rotating engine order. Each engine's file records its rep count. Checksums are checked on every run. Programs in `bench/` (docs/spec.md §12). Failed cells have no timing. Only engines measured in this invocation appear.

Run on AMD Ryzen 9 8945HS w/ Radeon 780M Graphics, nproc 16, Linux 7.0.0-42-generic; 2026-11-07; pinned with taskset -c 2.

| Program | CPython 3.14.7 | qjs QuickJS version 2026-06-04 | monty-runtime 1.4.0 | micro-monty 0.0.0 |
|---|---:|---:|---:|---:|
| startup | 7.4 / 7.2 | 1.4 / 1.2 | 1.8 / 1.6 | 1.1 / 1.1 |
| fib | 19.7 / 19.3 | 14.0 / 13.9 | 71.9 / 70.6 | 16.0 / 15.8 |
| loop | 134.1 / 127.7 | 52.0 / 51.8 | 215.0 / 210.5 | 43.1 / 43.0 |
| dict | 75.8 / 73.7 | 72.7 / 70.7 | 133.3 / 130.2 | 68.4 / 68.0 |
| str | 29.1 / 28.8 | 24.9 / 24.8 | 48.4 / 48.0 | 30.7 / 30.5 |
| json | 111.9 / 110.6 | 129.3 / 128.6 | 164.5 / 164.3 | 67.2 / 66.8 |
| closure | 57.4 / 56.4 | 28.4 / 28.1 | 143.4 / 142.0 | 29.9 / 29.7 |
| classes | 57.8 / 56.1 | 53.8 / 53.5 | 306.9 / 304.6 | 62.0 / 61.7 |
| generators | 56.5 / 55.6 | 74.2 / 73.9 | failed | 118.3 / 117.2 |
| exceptions | 43.4 / 43.2 | 96.6 / 95.8 | failed | 55.2 / 54.8 |
| fmt | 57.4 / 56.4 | 51.6 / 51.3 | 123.3 / 122.7 | 101.0 / 100.0 |
| sort | 62.0 / 61.1 | 126.5 / 125.5 | 212.1 / 208.5 | 76.6 / 76.0 |
| interp | 57.3 / 56.6 | 111.3 / 110.5 | 293.6 / 291.8 | 115.6 / 114.3 |

Failures on monty-runtime 1.4.0 (not inferred to be unsupported):

- exceptions: `exceptions on monty-runtime 1.4.0: expected `33334 99995 25063333 100000 50000`, got `` (exit status: 1): error:`
- generators: `generators on monty-runtime 1.4.0: expected `67499550000 138450000 299995 100`, got `` (exit status: 1): error:`
