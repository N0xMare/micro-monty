# Tracking-off ablation: where the candidate's execution cost comes from (2026-11-04)

[M] unless labeled. Same orb, toolchain (Rust 1.99.0) and release profile as
`../2026-11-04-main-vs-candidate/`. Every run's checksum matched.

## Builds

All built with `bench/evidence/build_micro.sh`:

| Variant | Source | `mm` SHA-256 |
|---|---|---|
| main | `fec4ff6` | `ec95bd9e…` |
| candidate | `evidence-campaign` at `8be9777` | `dec9ebb5…` |
| ablation-a | candidate + `../../ablation/a-no-line-marking.patch`: `mark_line`/`mark_lines` are no-ops | `2d6e0666…` |
| ablation-b | ablation-a + main's `crates/mm-engine/src/dict.rs` (`../../ablation/b-no-marking-main-dict.patch`) | `1eea8cd4…` |

The ablations are **diagnostic only**: without marking, delta snapshots
silently miss changes. They are not shippable configurations. Everything else
(segment dirty flags, line-bitmap allocation and accounting, snapshot code) is
unchanged, so A isolates the cost of the marking itself. B also removes the
candidate's dict-index rewrite (separate checked `get_raw`/`set_raw` per word
instead of one `raw_words_mut` borrow), which exists only to keep marking
proportional.

## Instructions (callgrind `Ir`, whole `mm run` process, deterministic)

See `instructions.md`. Ratios to main:

| Program | candidate | A: no marking | B: + main dict | marking's share of the extra Ir |
|---|---:|---:|---:|---:|
| startup | 1.076 | 1.020 | 1.000 | 74% |
| fib | 1.025 | 0.997 | 0.997 | 111% |
| loop | 1.129 | 1.000 | 1.000 | 100% |
| dict | 1.188 | 1.039 | 1.003 | 79% |
| str | 1.065 | 1.001 | 1.001 | 98% |
| json | 1.063 | 1.000 | 1.001 | 99% |
| closure | 1.081 | 1.000 | 1.000 | 100% |
| classes | 1.044 | 1.000 | 1.000 | 100% |
| generators | 1.098 | 0.997 | 0.997 | 103% |
| exceptions | 1.067 | 1.006 | 1.002 | 92% |
| fmt | 1.040 | 1.005 | 1.005 | 88% |
| sort | 1.037 | 1.001 | 1.001 | 98% |
| interp | 1.021 | 1.000 | 1.000 | 102% |

## Time (`measure.py programs`, 3 sessions × 21 rounds, 4 builds rotating per round)

```
taskset -c 2 python3 bench/evidence/measure.py programs session-N \
  --variants main,candidate,ablation-a,ablation-b
python3 bench/evidence/program_ratios.py . main
```

Boundary: Python `subprocess.run` spawn to exit (not `xtask bench`'s). Paired
per-round ratios, bootstrap 95% CI, 63 pairs per program (`paired-vs-main.md`):

| Program | main ms | candidate/main | A/main | B/main |
|---|---:|---:|---:|---:|
| startup | 3.1 | 1.006 [0.990, 1.015] | 1.010 [0.980, 1.026] | 0.997 [0.987, 1.013] |
| fib | 27.3 | 1.053 [1.036, 1.058] | 0.994 [0.981, 1.007] | 1.012 [1.002, 1.020] |
| loop | 70.4 | 1.155 [1.148, 1.161] | 0.991 [0.981, 0.994] | 1.020 [1.012, 1.026] |
| dict | 150.3 | 1.181 [1.158, 1.198] | 1.038 [1.031, 1.065] | 1.036 [1.018, 1.052] |
| str | 58.2 | 1.046 [1.041, 1.061] | 0.980 [0.969, 0.989] | 0.984 [0.975, 0.992] |
| json | 120.6 | 1.053 [1.049, 1.064] | 0.996 [0.992, 0.999] | 1.008 [1.002, 1.011] |
| closure | 49.6 | 1.121 [1.108, 1.132] | 0.992 [0.986, 1.001] | 1.019 [1.006, 1.025] |
| classes | 148.5 | 1.058 [1.054, 1.072] | 0.994 [0.987, 1.010] | 1.009 [0.998, 1.017] |
| generators | 292.6 | 1.112 [1.108, 1.122] | 0.992 [0.989, 0.997] | 1.018 [1.016, 1.024] |
| exceptions | 104.9 | 1.051 [1.042, 1.055] | 0.994 [0.987, 0.998] | 0.998 [0.992, 1.001] |
| fmt | 264.1 | 0.983 [0.975, 1.006] | 0.954 [0.944, 0.970] | 0.994 [0.961, 1.004] |
| sort | 242.8 | 1.033 [0.990, 1.084] | 1.047 [1.029, 1.073] | 0.990 [0.974, 1.032] |
| interp | 274.1 | 1.018 [1.007, 1.030] | 0.981 [0.976, 0.993] | 1.011 [0.998, 1.020] |

The candidate column reproduces the `xtask bench` result (loop 1.155 vs
1.15; dict 1.181 vs 1.19). Main's absolute times drifted upward from that
earlier run (sort 175 → 243 ms, dict 123 → 150 ms). No orb process was busy
and the cgroup reported no throttling; repeated sort runs vary 170–280 ms with
or without parent pinning. [E] The likely cause is noise on the shared host.
Paired ratios cancel slow drift; sort's wide intervals show that drift within
a round does not cancel.

## Findings

1. **Line marking is the candidate's execution cost.** On 11 of 13 programs
   it accounts for 88–111% of the extra instructions. With marking off, the
   candidate executes main's instruction count within ±0.6% on 11 programs,
   and its time is within about ±2% of main on 10 (the outliers are dict,
   sort and fmt).
2. **The dict-index rewrite costs the rest on dict** (+3.6% Ir: A 1.039 →
   B 1.003). It is a side effect of marking, so it disappears with marking.
3. **Timing alone attributes ±2–5% wrongly.** B matches main in instructions
   but times 1.02 on loop/closure/generators, and 1.036 on dict where its Ir
   is 1.003. A times below main on fmt (0.954) at +0.5% Ir. That is the
   dispatch-loop codegen/layout effect seen before. Small timing differences
   between builds of different source are not architecture evidence without
   instruction counts.
4. Startup: +7.6% Ir (74% marking; B is exactly main), not visible in time (≈0.8M Ir).

Not tested here: the runtime cost of a per-session on/off check (a branch on
every setter) versus a compile-time feature; memory cost of the line bitmaps;
delta correctness (by construction, the ablations break it).
