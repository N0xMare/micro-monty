# Benchmarks

[M] `cargo xtask bench`: median / min wall-clock ms per process (startup included), after one untimed warm-up run; balanced rotating engine order. Each engine's file records its rep count. Checksums are checked on every run. Programs in `bench/` (docs/spec.md §12). Failed cells have no timing. Only engines measured in this invocation appear.

Run on Intel(R) Xeon(R) Processor @ 2.60GHz, nproc 16, Linux 6.1.189+; 2026-11-04; pinned with taskset -c 2.

| Program | micro-monty 0.0.0 | micro-monty-main 0.0.0 |
|---|---:|---:|
| startup | 3.3 / 3.1 | 3.3 / 3.2 |
| fib | 28.5 / 28.0 | 27.2 / 26.5 |
| loop | 80.9 / 79.6 | 70.4 / 69.1 |
| dict | 147.6 / 142.5 | 123.7 / 118.0 |
| str | 57.6 / 54.6 | 53.9 / 52.6 |
| json | 121.0 / 118.7 | 113.4 / 111.9 |
| closure | 55.4 / 53.6 | 49.2 / 47.7 |
| classes | 152.3 / 150.3 | 144.0 / 143.3 |
| generators | 323.8 / 321.9 | 287.7 / 286.0 |
| exceptions | 108.1 / 106.2 | 103.1 / 102.4 |
| fmt | 232.2 / 226.3 | 227.0 / 223.2 |
| sort | 177.6 / 170.9 | 170.9 / 167.0 |
| interp | 270.3 / 266.9 | 266.4 / 263.9 |
