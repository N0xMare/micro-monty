# Benchmarks

[M] `cargo xtask bench`: median / min wall-clock ms per process (startup included), after one untimed warm-up run; balanced rotating engine order. Each engine's file records its rep count. Checksums are checked on every run. Programs in `bench/` (docs/spec.md §12). Failed cells have no timing. Only engines measured in this invocation appear.

Run on Intel(R) Xeon(R) Processor @ 2.60GHz, nproc 16, Linux 6.1.189+; 2026-11-04; pinned with taskset -c 2.

| Program | micro-monty 0.0.0 | micro-monty-main 0.0.0 |
|---|---:|---:|
| startup | 3.2 / 3.2 | 3.2 / 3.2 |
| fib | 28.3 / 27.9 | 26.9 / 26.4 |
| loop | 80.6 / 79.8 | 69.9 / 69.0 |
| dict | 147.7 / 143.9 | 123.4 / 120.1 |
| str | 55.5 / 54.8 | 52.8 / 51.4 |
| json | 120.9 / 119.7 | 113.4 / 111.9 |
| closure | 54.3 / 53.5 | 48.3 / 47.6 |
| classes | 153.0 / 150.2 | 144.3 / 143.2 |
| generators | 323.7 / 322.1 | 288.3 / 286.1 |
| exceptions | 109.1 / 106.6 | 103.9 / 101.4 |
| fmt | 235.2 / 225.2 | 233.9 / 217.8 |
| sort | 189.9 / 172.2 | 195.2 / 169.2 |
| interp | 308.5 / 268.6 | 314.7 / 262.7 |
