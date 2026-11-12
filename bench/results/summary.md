# Benchmarks

[M] `cargo xtask bench`: median / min wall-clock ms per process (startup included), after one untimed warm-up run; each engine's file records its rep count. Checksums are checked on every run. Programs in `bench/` (docs/spec.md §12).

Last run on Intel(R) Xeon(R) Processor @ 2.60GHz, nproc 16, Linux 6.1.189+; 2026-11-04; pinned with taskset -c 2. Each engine's file records the machine and date of its own run.

| Program | CPython 3.14.7 | QuickJS version 2026-06-04 | monty-runtime 1.4.0 | micro-monty 0.0.0 |
|---|---:|---:|---:|---:|
| startup | 13.7 / 13.4 | 3.3 / 3.1 | 4.2 / 3.9 | 3.4 / 3.2 |
| fib | 31.0 / 30.6 | 19.1 / 19.0 | 103.8 / 103.0 | 23.1 / 22.2 |
| loop | 170.9 / 144.5 | 66.7 / 66.4 | 245.8 / 242.5 | 59.6 / 58.2 |
| dict | 136.5 / 133.6 | 129.1 / 126.4 | 192.0 / 185.9 | 115.9 / 108.7 |
| str | 42.2 / 40.7 | 34.3 / 33.7 | 73.4 / 72.8 | 46.5 / 45.3 |
| json | 166.7 / 164.0 | 197.7 / 182.5 | 228.1 / 225.1 | 98.2 / 96.3 |
| closure | 74.2 / 68.4 | 37.1 / 36.6 | 179.2 / 160.7 | 39.2 / 36.3 |
| classes | 76.4 / 73.6 | 74.0 / 65.9 | 414.3 / 374.2 | 103.2 / 92.0 |
| generators | 73.9 / 73.3 | 107.7 / 101.2 | unsupported | 223.7 / 220.7 |
| exceptions | 64.2 / 62.9 | 150.3 / 139.1 | unsupported | 80.4 / 80.0 |
| fmt | 87.8 / 86.7 | 79.9 / 76.7 | 172.5 / 167.6 | 185.3 / 168.6 |
| sort | 103.9 / 97.6 | 219.4 / 208.5 | 372.9 / 355.3 | 160.0 / 157.2 |
| interp | 72.9 / 65.0 | 143.4 / 131.7 | 420.3 / 371.6 | 204.1 / 201.9 |

Unsupported on monty-runtime 1.4.0:

- exceptions: `NotImplementedError: The monty syntax parser does not yet support class inheritance and metaclasses`
- generators: `NotImplementedError: The monty syntax parser does not yet support yield expressions`
