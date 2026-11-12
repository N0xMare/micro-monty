# Instruction counts

[M] callgrind, whole process (`valgrind --tool=callgrind --trace-children=yes`,
total `Collected` Ir), so startup is included; millions of instructions.
2026-11-03 at `4e220da` plus the six added programs. Engines: CPython 3.14.7,
QuickJS 2026-06-04 (dquickts `b51e92c`, `make qjs`), micro-monty's `mm run`
(`target/release/mm`, as `cargo xtask bench` builds it). Every run printed
its `expected.txt` line. Wall-clock medians are in `summary.md`.

| Program | CPython | QuickJS | micro-monty | mm / CPython | mm / QuickJS |
|---|---:|---:|---:|---:|---:|
| startup | 39.4 | 1.5 | 6.7 | 0.17 | 4.4 |
| fib | 328 | 252 | 401 | 1.22 | 1.59 |
| loop | 3,300 | 838 | 1,349 | 0.41 | 1.61 |
| dict | 923 | 865 | 1,340 | 1.45 | 1.55 |
| str | 326 | 360 | 572 | 1.75 | 1.59 |
| json | 1,402 | 1,954 | 1,735 | 1.24 | 0.89 |
| closure | 1,188 | 540 | 919 | 0.77 | 1.70 |
| classes | 1,063 | 975 | 2,342 | 2.20 | 2.40 |
| generators | 1,054 | 1,352 | 4,273 | 4.06 | 3.16 |
| exceptions | 767 | 634 | 2,596 | 3.39 | 4.10 |
| fmt | 947 | 889 | 2,061 | 2.18 | 2.32 |
| sort | 904 | 2,031 | 8,229 | 9.10 | 4.05 |
| interp | 991 | 1,830 | 3,165 | 3.19 | 1.73 |
