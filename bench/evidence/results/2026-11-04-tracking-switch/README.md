# Per-session tracking switch vs compile-time removal (2026-11-04)

[M] unless labeled. Same orb, toolchain and profile as
`../2026-11-04-tracking-ablation/`. Every run's checksum matched.

## Builds

| Variant | What it is | `mm` |
|---|---|---|
| main | `fec4ff6` | `ec95bd9e…` |
| candidate | always-on line marking | `dec9ebb5…` |
| ablation-a | marking compiled out (stands in for a compile-time feature) | `2d6e0666…` |
| switch-on | candidate + `../../ablation/switch-v1.patch`, default (tracking on) | one binary |
| switch-off | the **same binary** with `MM_DELTA_TRACKING=0` (`env` file) | |

`switch-v1.patch` (diagnostic only; off breaks deltas): a `track: bool` on
each heap space. When it is false, `block_lines`, the allocation path and the
sweep hand out an empty line map, `set_guarded`/`set_raw`-style setters skip
`mark_line`, and `mark_lines` returns on an empty map. `Session::set_delta_tracking`
sets it, and `mm` reads the environment variable once per session. Line bitmaps
are still allocated, so this measures CPU cost only.

A second version moved `mark_lines`' test inline and its loop out of line. It
executed the same instruction count as v1 on loop to within 0.01% (on
1,319,564,849 vs 1,319,559,300; off 1,217,564,966 vs 1,217,559,540), so it
was dropped. The leftover cost is the flag test and its effect on the code
around each write, not the call into `mark_lines`.

## Instructions (callgrind `Ir`, whole `mm run` process; `instructions.md`)

| Program | candidate | compile-time off | switch on | switch off | share of tracking cost left when off |
|---|---:|---:|---:|---:|---:|
| startup | 1.076 | 1.020 | 1.083 | 1.084 | 116% |
| fib | 1.025 | 0.997 | 1.040 | 1.040 | 153% |
| loop | 1.129 | 1.000 | 1.131 | 1.044 | 34% |
| dict | 1.188 | 1.039 | 1.206 | 1.104 | 44% |
| str | 1.065 | 1.001 | 1.073 | 1.015 | 22% |
| json | 1.063 | 1.000 | 1.073 | 1.030 | 47% |
| closure | 1.081 | 1.000 | 1.080 | 1.032 | 40% |
| classes | 1.044 | 1.000 | 1.052 | 1.026 | 59% |
| generators | 1.098 | 0.997 | 1.124 | 1.052 | 54% |
| exceptions | 1.067 | 1.006 | 1.077 | 1.034 | 46% |
| fmt | 1.040 | 1.005 | 1.042 | 1.012 | 20% |
| sort | 1.037 | 1.001 | 1.043 | 1.017 | 44% |
| interp | 1.021 | 1.000 | 1.027 | 1.015 | 73% |

All ratios to main. "Share left" = (switch-off − compile-time off) /
(candidate − compile-time off).

## Time (`measure.py programs`, 3 sessions × 21 rounds, 5 variants rotating; `paired-vs-main.md`)

| Program | main ms | candidate | compile-time off | switch on | switch off |
|---|---:|---:|---:|---:|---:|
| startup | 3.1 | 1.009 | 1.006 | 1.009 | 0.995 |
| fib | 27.0 | 1.056 | 0.983 | 1.082 | 1.083 |
| loop | 69.6 | 1.157 | 0.991 | 1.215 | 1.086 |
| dict | 142.6 | 1.187 | 1.037 | 1.210 | 1.096 |
| str | 55.6 | 1.045 | 0.982 | 1.050 | 0.999 |
| json | 118.2 | 1.046 | 0.989 | 1.047 | 1.007 |
| closure | 48.8 | 1.129 | 0.995 | 1.175 | 1.135 |
| classes | 146.9 | 1.066 | 0.992 | 1.041 | 1.021 |
| generators | 295.4 | 1.113 | 0.991 | 1.105 | 1.045 |
| exceptions | 105.9 | 1.051 | 0.991 | 1.053 | 1.026 |
| fmt | 260.8 | 0.994 | 0.954 | 1.008 | 0.990 |
| sort | 237.0 | 1.047 | 1.068 | 1.074 | 1.039 |
| interp | 275.2 | 1.019 | 0.995 | 1.015 | 1.001 |

Paired per-round medians, 63 pairs each. The 95% CIs are in
`paired-vs-main.md`; sort's are wide (±4%), the rest mostly ±1%.

## Findings

1. **Compile-time removal costs nothing** (Ir within ±0.6% of main on 11/13,
   as in the previous run).
2. **A per-session switch, when off, keeps 20–73% of the tracking
   instructions on 11 programs**, and more than all of them on fib (153%)
   and startup. Time off: loop 1.086, dict 1.096, closure 1.135, fib 1.083.
3. **Switch on is slower than always-on**: up to +2.6 points of Ir
   (generators 1.124 vs 1.098) and +5.8 points of time (loop 1.215 vs 1.157).
4. Fib, which does few heap writes, executes more instructions with the switch
   off (1.040) than the candidate does with marking on (1.025). [E] Adding the
   flag test to setters inlined into the dispatch loop changes how that loop
   compiles, at an instruction cost independent of the flag's value. Not yet
   attributed to specific code; a symbolized build is needed.

Scope: one switch design (a flag on the heap spaces, read at each write).
Untested: designs that keep the check off the hot path, such as a second
dispatch loop or setter set chosen once per session, or flipping the flag at a
point the next snapshot treats as a FULL. The memory cost of line bitmaps
(allocated in all variants here) is not measured.
