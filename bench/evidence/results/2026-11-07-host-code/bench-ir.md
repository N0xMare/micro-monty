# Bench Ir, base 8507ad2 against this change [M]

`python3 bench/callgrind.py --engine base="target/base/release/mm run" --engine new="target/release/mm run"`
(thousands of Ir, whole process). Release code at the base `d2912a2` is
the same as `8507ad2`'s (that commit only changes a debug assertion).

| Program | base | new | new / base |
|---|---:|---:|---:|
| startup | 0.9 | 0.9 | 1.01 |
| fib | 321.5 | 321.5 | 1.00 |
| loop | 1,114.9 | 1,114.9 | 1.00 |
| dict | 1,160.9 | 1,161.0 | 1.00 |
| str | 494.2 | 494.2 | 1.00 |
| json | 1,134.9 | 1,135.0 | 1.00 |
| closure | 714.7 | 714.7 | 1.00 |
| classes | 1,382.0 | 1,382.0 | 1.00 |
| generators | 2,380.9 | 2,381.0 | 1.00 |
| exceptions | 992.2 | 992.3 | 1.00 |
| fmt | 1,561.9 | 1,561.9 | 1.00 |
| sort | 1,453.5 | 1,453.5 | 1.00 |
| interp | 2,281.9 | 2,282.0 | 1.00 |

`startup` exactly: 898,848 → 905,966 (+0.79%). In earlier differential
profiles of the same build, the engine's own code accounted for +167 Ir;
the rest is libc reading `/proc/self/maps` at process start under
valgrind, longer for the larger binary (as in decisions 39 and 45).

Accepted by the owner (2026-11-07) as an exception to the ±0.5% bench
rule (HANDOFF scope decision 60).
