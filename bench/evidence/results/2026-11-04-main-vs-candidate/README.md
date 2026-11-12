# Accepted main vs delta candidate, 13 bench programs (2026-11-04)

[M] unless labeled. Orb: 16 vCPU shared VM, idle (load ~0.6), every process
pinned to CPU 2. No other work ran during timing.

## Builds

Both from `bench/evidence/build_micro.sh` with Rust 1.99.0 and the workspace
release profile (fat LTO, `codegen-units=1`, `panic=abort`):

| Build | Source | `mm` SHA-256 |
|---|---|---|
| main | `fec4ff6` (worktree) | `ec95bd9e7dc963747fbb205f7ef2da3438543f329cd429778e99e47af91f8b8b` |
| candidate | branch `evidence-campaign` at `8be9777` | `dec9ebb5eae20b959b4afbac945c3e46acb827663e8587adb53e5c03b24f1300` |

Probes (`probe`) linked from the same builds: main `aa7cbe2d…`, candidate `f4fb5f48…`.

## Cold process, `cargo xtask bench`

```
MM=<candidate mm> MM_MAIN=<main mm> cargo xtask bench \
  --engines micro-monty,micro-monty-main --reps 21 --out-dir session-N
```

Three independent sessions, run back to back, each with 1 warm-up and 21 timed
rounds in rotating engine order. Every checksum matched; no failures. Ratios
pair the two engines' samples within a round (`bench/evidence/pair_ratios.py`);
the intervals are bootstrap 95% CIs of the median ratio.

| Program | main ms | candidate ms | S1 cand/main [95% CI] | S2 cand/main [95% CI] | S3 cand/main [95% CI] |
|---|---:|---:|---:|---:|---:|
| startup | 3.3 | 3.3 | 1.003 [0.992, 1.015] | 1.003 [0.986, 1.009] | 1.000 [0.977, 1.031] |
| fib | 27.2 | 28.5 | 1.061 [1.052, 1.066] | 1.063 [1.041, 1.065] | 1.047 [1.035, 1.064] |
| loop | 70.4 | 80.9 | 1.149 [1.138, 1.157] | 1.158 [1.146, 1.165] | 1.156 [1.152, 1.192] |
| dict | 123.4 | 147.6 | 1.189 [1.182, 1.210] | 1.189 [1.182, 1.206] | 1.188 [1.166, 1.204] |
| str | 52.8 | 55.5 | 1.055 [1.047, 1.070] | 1.055 [1.048, 1.066] | 1.060 [1.056, 1.076] |
| json | 113.4 | 120.9 | 1.065 [1.059, 1.068] | 1.066 [1.064, 1.069] | 1.065 [1.059, 1.069] |
| closure | 48.3 | 54.3 | 1.120 [1.114, 1.123] | 1.123 [1.116, 1.132] | 1.127 [1.121, 1.142] |
| classes | 144.3 | 152.6 | 1.055 [1.053, 1.073] | 1.058 [1.053, 1.081] | 1.062 [1.054, 1.084] |
| generators | 287.8 | 323.7 | 1.125 [1.121, 1.126] | 1.125 [1.119, 1.129] | 1.124 [1.121, 1.128] |
| exceptions | 103.7 | 109.1 | 1.054 [1.044, 1.067] | 1.044 [1.038, 1.052] | 1.048 [1.044, 1.054] |
| fmt | 232.8 | 235.2 | 1.009 [0.997, 1.016] | 1.014 [1.002, 1.035] | 1.013 [1.007, 1.028] |
| sort | 174.8 | 182.6 | 1.021 [1.018, 1.060] | 1.009 [0.959, 1.028] | 1.034 [1.023, 1.055] |
| interp | 268.1 | 270.3 | 1.009 [0.994, 1.021] | 1.015 [0.975, 1.030] | 1.012 [1.008, 1.021] |

## The earlier warm-loop contradiction

`warm-probe-3M.jsonl`: the in-process probe (`probe warm 3000000 3`), 21
alternating processes per build after one warm-up. It replicates the lost run's
direction: candidate 46.1 ms vs main 48.8 ms per call, paired ratio 0.944
(all 21 pairs 0.884–0.963). The first call is also faster (0.937). It was
**not** a mixed-build artifact.

Same `mm` binaries, same N=3,000,000, 21 alternating runs each:

| Program through `mm run` | main ms | candidate ms | paired ratio |
|---|---:|---:|---:|
| module-level loop (`bench/loop.py`) | 69.7 | 80.4 | 1.156 |
| the probe's function-local loop | 50.3 | 59.8 | 1.192 |

Instructions (callgrind `Ir`, whole process, N=300,000; deterministic):

| Run | main | candidate | ratio |
|---|---:|---:|---:|
| `mm`, module-level loop | 117,453,290 | 132,528,578 | 1.128 |
| `mm`, function-local loop | 77,589,810 | 79,464,224 | 1.024 |
| probe, function-local loop (first + 1 warm call) | 159,981,210 | 157,659,615 | 0.985 |

Interpretation:
- Module-level loop: most of the slowdown is extra executed work (+12.8% Ir,
  +15.6% time). Globals live in a dict, so every iteration writes through the
  candidate's dirty tracking.
- Function-local loop: +2.4% Ir but +19% time in `mm`, and −1.5% Ir / −6% time
  in the probe. Identical source runs faster or slower depending on the final
  link. That is a codegen/layout effect on the dispatch loop (≈99% of
  instructions are in one inlined function), not tracking work. [E] This
  hypothesis is not yet attributed; a symbolized build is needed.
- So main vs candidate time ratios mix two causes: tracking work and codegen
  perturbation. Only a tracking ablation (same source, tracking off) can
  separate them.
