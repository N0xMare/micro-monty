# Benchmarks

[M] `cargo xtask bench`: median / min wall-clock ms per process (startup included), after one untimed warm-up run; balanced rotating engine order. Each engine's file records its rep count. Checksums are checked on every run. Programs in `bench/` (docs/spec.md §12). Failed cells have no timing. Only engines measured in this invocation appear.

Run on AMD Ryzen 9 8945HS w/ Radeon 780M Graphics, nproc 16, Linux 7.0.0-42-generic; 2026-11-06; pinned with taskset -c 2.

| Program | CPython 3.14.7 | qjs QuickJS version 2026-06-04 | monty-runtime 1.4.0 | micro-monty 0.0.0 |
|---|---:|---:|---:|---:|
| startup | 7.1 / 6.9 | 1.3 / 1.1 | 1.7 / 1.6 | 1.1 / 1.0 |
| fib | 19.4 / 19.2 | 13.9 / 13.9 | 68.6 / 67.9 | 15.9 / 15.8 |
| loop | 136.7 / 122.6 | 51.8 / 51.2 | 241.0 / 238.5 | 43.1 / 42.8 |
| dict | 73.4 / 71.3 | 72.9 / 70.9 | 129.0 / 128.0 | 68.4 / 68.1 |
| str | 28.9 / 28.7 | 25.0 / 24.8 | 48.5 / 47.1 | 30.7 / 30.4 |
| json | 110.8 / 110.4 | 129.7 / 128.3 | 163.5 / 162.3 | 67.0 / 66.9 |
| closure | 55.8 / 54.8 | 28.4 / 28.1 | 127.9 / 126.9 | 30.0 / 29.9 |
| classes | 58.6 / 56.0 | 54.0 / 53.0 | 320.6 / 316.0 | 61.8 / 60.8 |
| generators | 56.0 / 55.2 | 74.3 / 73.6 | failed | 120.1 / 119.1 |
| exceptions | 43.4 / 42.4 | 99.0 / 97.7 | failed | 56.2 / 55.7 |
| fmt | 56.6 / 55.9 | 52.2 / 51.3 | 126.8 / 126.1 | 101.0 / 100.2 |
| sort | 61.6 / 60.4 | 126.8 / 126.2 | 210.3 / 208.1 | 76.5 / 75.8 |
| interp | 57.0 / 56.6 | 111.9 / 110.5 | 304.0 / 302.2 | 115.8 / 115.1 |

Failures on monty-runtime 1.4.0 (not inferred to be unsupported):

- exceptions: `exceptions on monty-runtime 1.4.0: expected `33334 99995 25063333 100000 50000`, got `` (exit status: 1): error:`
- generators: `generators on monty-runtime 1.4.0: expected `67499550000 138450000 299995 100`, got `` (exit status: 1): error:`
