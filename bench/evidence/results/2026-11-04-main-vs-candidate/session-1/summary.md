# Benchmarks

[M] `cargo xtask bench`: median / min wall-clock ms per process (startup included), after one untimed warm-up run; balanced rotating engine order. Each engine's file records its rep count. Checksums are checked on every run. Programs in `bench/` (docs/spec.md §12). Failed cells have no timing. Only engines measured in this invocation appear.

Run on Intel(R) Xeon(R) Processor @ 2.60GHz, nproc 16, Linux 6.1.189+; 2026-11-04; pinned with taskset -c 2.

| Program | micro-monty 0.0.0 | micro-monty-main 0.0.0 |
|---|---:|---:|
| startup | 3.4 / 3.3 | 3.4 / 3.3 |
| fib | 29.8 / 28.4 | 28.1 / 26.7 |
| loop | 81.0 / 80.2 | 70.4 / 69.4 |
| dict | 146.5 / 143.4 | 123.2 / 120.2 |
| str | 54.3 / 53.3 | 51.5 / 50.5 |
| json | 120.2 / 118.8 | 112.6 / 111.5 |
| closure | 53.9 / 53.7 | 48.2 / 47.9 |
| classes | 152.6 / 150.6 | 144.3 / 143.4 |
| generators | 323.6 / 322.4 | 287.8 / 286.4 |
| exceptions | 109.7 / 107.6 | 103.7 / 102.4 |
| fmt | 236.5 / 228.0 | 232.8 / 224.9 |
| sort | 182.6 / 173.5 | 174.8 / 168.1 |
| interp | 269.4 / 267.8 | 268.1 / 261.9 |
