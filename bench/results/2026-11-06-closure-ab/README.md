# Is the orb's `closure` regression real? Interleaved A/B (2026-11-05/06)

On the orb VM, `closure` got 6–8% slower in wall time from e6d7251 to the
speed landing at 1fe8e2d (49 → 52 ms), even though its Ir fell by 0.8%
[S, orb thread]. That suggests code layout on that CPU. Here the same builds
are compared on bare-metal hosts, with c97ce06 (the final head, after the
size pass) added.

**Method** (`ab.py`): release `mm run` (`cargo build --release -p mm-cli`,
each commit in its own worktree, same toolchain). In every round each
(build, program) pair runs once, in a shuffled order. The first 3 rounds are
discarded. Every output is checked against `bench/expected.txt`. The tables
give the median wall time per process (startup included) with the
interquartile range in brackets, and the ratio of medians with a 95%
bootstrap interval. fib, loop and dict are controls the change did not
target.

```bash
python3.14 ab.py REPS OUT.json e6d7251=MM_A 1fe8e2d=MM_B [c97ce06=MM_C] -- \
    bench/closure.py bench/fib.py bench/loop.py bench/dict.py
```

## Mac: M1 Max, macOS 27.2, AC power, not pinned [M]

No `cargo` or `rustc` ran during either run. The 2026-11-05 runs were at
load average about 5. The 2026-11-06 run (00:03:36–00:04:00Z) was at 7–8.

| run | program | e6d7251 | 1fe8e2d | c97ce06 | 1fe8e2d / e6d7251 | c97ce06 / e6d7251 |
|---|---|---|---|---|---|---|
| 40 rounds | **closure** | 38.3 [38.1–38.4] | 36.9 [36.7–37.2] | | **0.966** (0.961–0.969) | |
| 100 rounds, closure only | **closure** | 38.2 [38.1–38.4] | 37.0 [36.9–37.1] | | **0.968** (0.966–0.969) | |
| 40 rounds, 3 builds | **closure** | 37.9 [37.8–38.0] | 36.7 [36.6–37.0] | 36.8 [36.6–37.0] | **0.970** (0.967–0.973) | **0.972** (0.969–0.975) |
| | fib | 24.2 | 24.3 | 24.6 | 1.004 (0.998–1.010) | 1.015 (1.009–1.021) |
| | loop | 51.1 | 49.6 | 50.6 | 0.970 (0.967–0.973) | 0.991 (0.988–0.994) |
| | dict | 70.5 | 70.1 | 70.3 | 0.994 (0.987–1.002) | 0.998 (0.992–1.006) |

On aarch64, `closure` is about 3% **faster** at 1fe8e2d and at c97ce06, in
all three runs. The orb's slowdown does not happen here. The size pass
(c97ce06) costs fib 1.5% and loop 2% against 1fe8e2d on this machine, but
leaves `closure` unchanged.

### Mac at the final head, 4 builds (2026-11-06, 16:55:55–16:56:26Z) [M]

The run used 40 rounds with no `cargo` or `rustc` running. The owner's
`plak --resume` session used 35–40% of one core, and the load average was
about 4.

| program | e6d7251 | 1fe8e2d | c97ce06 | 5349520 | 1fe8e2d / e6d7251 | c97ce06 / e6d7251 | 5349520 / e6d7251 |
|---|---|---|---|---|---|---|---|
| **closure** | 37.0 | 35.8 | 35.9 | 36.3 | **0.967** (0.962–0.972) | **0.969** (0.963–0.974) | **0.980** (0.973–0.987) |
| fib | 23.7 | 23.6 | 23.8 | 23.8 | 0.997 | 1.005 | 1.006 |
| loop | 49.8 | 48.5 | 49.5 | 49.5 | 0.972 | 0.993 | 0.992 |
| dict | 65.8 | 65.7 | 65.4 | 65.6 | 0.998 | 0.994 | 0.997 |

At the final head, `closure` is still 2% faster than at e6d7251 on aarch64.
That is 1.2% less of a gain than at c97ce06, a small move within this
machine's run-to-run spread [E].

## host1: Ryzen 9 8945HS (Zen 4), x86-64 Linux, pinned [M]

The run was on bare metal after the fuzz campaign ended, finishing at
2026-11-06T16:10:56Z. host1 was otherwise idle. It ran pinned with
`taskset -c 2`, with the `performance` governor on Linux 7.0.0-42 and
rustc 1.99.1, for 100 rounds over three builds. The final head here is
5349520, current main, which includes c97ce06's size pass and the later
fixes.

| program | e6d7251 | 1fe8e2d | 5349520 | 1fe8e2d / e6d7251 | 5349520 / e6d7251 |
|---|---|---|---|---|---|
| **closure** | 29.5 [29.4–29.7] | 29.6 [29.4–29.7] | 29.6 [29.5–29.8] | **1.001** (0.998–1.003) | **1.002** (0.999–1.004) |
| fib | 15.9 | 15.6 | 15.6 | 0.976 (0.971–0.980) | 0.978 (0.974–0.982) |
| loop | 51.4 | 42.7 | 42.8 | 0.831 (0.829–0.832) | 0.833 (0.830–0.834) |
| dict | 68.0 | 68.0 | 68.0 | 0.999 (0.998–1.002) | 0.999 (0.998–1.001) |

**Verdict:** on bare-metal Zen 4, `closure` is unchanged, within 0.2%, at
1fe8e2d and at the final head. On the Mac it is about 3% faster. The orb's
6–8% slowdown is not real on either bare-metal CPU. It is specific to the
orb's CPU or VM, most likely code layout, as the Ir drop suggested. The
controls moved as the speed landing intended. `loop` is 17% faster on
Zen 4, against 3% on the Mac, and `fib` is 2% faster.
