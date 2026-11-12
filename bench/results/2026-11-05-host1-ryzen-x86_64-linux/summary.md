# Benchmarks

[M] `cargo xtask bench`: median / min wall-clock ms per process (startup included), after one untimed warm-up run; balanced rotating engine order. Each engine's file records its rep count. Checksums are checked on every run. Programs in `bench/` (docs/spec.md §12). Failed cells have no timing. Only engines measured in this invocation appear.

Run on AMD Ryzen 9 8945HS w/ Radeon 780M Graphics, nproc 16, Linux 7.0.0-42-generic; 2026-11-05; pinned with taskset -c 2.

| Program | CPython 3.14.7 | qjs QuickJS version 2026-06-04 | monty-runtime 1.4.0 | micro-monty 0.0.0 |
|---|---:|---:|---:|---:|
| startup | 7.0 / 7.0 | 1.3 / 1.1 | 1.6 / 1.4 | 1.1 / 1.0 |
| fib | 19.4 / 19.1 | 13.9 / 13.8 | 69.2 / 68.6 | 15.2 / 15.1 |
| loop | 136.9 / 127.9 | 52.1 / 50.0 | 210.9 / 209.4 | 44.0 / 43.7 |
| dict | 75.7 / 72.5 | 72.8 / 71.1 | 123.9 / 122.0 | 68.7 / 68.0 |
| str | 28.8 / 28.4 | 24.8 / 24.7 | 48.7 / 48.6 | 30.3 / 30.0 |
| json | 111.0 / 110.0 | 129.4 / 128.2 | 166.7 / 164.7 | 67.0 / 66.8 |
| closure | 56.0 / 54.0 | 28.2 / 27.9 | 125.0 / 123.6 | 29.9 / 29.6 |
| classes | 58.2 / 56.4 | 54.0 / 53.3 | 306.9 / 304.3 | 77.3 / 77.0 |
| generators | 56.2 / 55.2 | 73.7 / 73.4 | failed | 160.2 / 158.7 |
| exceptions | 43.0 / 42.5 | 95.6 / 95.3 | failed | 54.7 / 54.1 |
| fmt | 56.8 / 56.1 | 51.4 / 51.0 | 124.7 / 124.1 | 123.6 / 122.6 |
| sort | 61.9 / 60.9 | 126.6 / 125.8 | 209.0 / 207.1 | 85.1 / 84.2 |
| interp | 56.9 / 56.5 | 110.5 / 110.0 | 302.1 / 299.3 | 143.8 / 142.1 |

Failures on monty-runtime 1.4.0 (not inferred to be unsupported):

- exceptions: `exceptions on monty-runtime 1.4.0: expected `33334 99995 25063333 100000 50000`, got `` (exit status: 1): error:`
- generators: `generators on monty-runtime 1.4.0: expected `67499550000 138450000 299995 100`, got `` (exit status: 1): error:`
