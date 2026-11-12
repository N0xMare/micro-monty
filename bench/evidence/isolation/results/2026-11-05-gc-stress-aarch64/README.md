# `cargo xtask gc-stress` on aarch64 macOS (2026-11-05)

These are the first gc-stress runs on Apple Silicon (MacBook Pro M1 Max,
macOS 27.2, Rust 1.99.0). With `MM_GC_STRESS=1`, `mm` collects at every
safepoint. Every number is [M]. The logs are next to this file.

| phase | 45f454b (`gc-stress-45f454b.log`) | 874409b (`gc-stress-874409b.log`) |
|---|---|---|
| own cases, `mm run` with and without stress | identical, 861 s | identical, 1,098 s |
| corpus, `conformance --engine micro-monty --check` | matches the committed results, 162 s | matches, 168 s |
| durability on the own cases | 374 of 380 identical, 73,201 parks, 548 s; **failed**: 310, 324 and 331 timed out at 300 s, once at each park setting | **390 of 390 identical**, 78,855 parks, 2,277 s |
| result | rc=1 | **rc=0, every phase passed in 3,543 s** |

- At 45f454b the only failures were the three cases the review thread had
  flagged as too slow for the 300 s limit. d6d77f6 raised it to 3,600 s, and
  at 874409b they finish.
- Most of the time goes to those slow cases.
  `331_deep_comparison_handoff.py` takes about 13 min under stress at
  99% CPU and 10 MiB RSS, against 0.30 s without it.
- The machine was shared with the owner's desktop apps and, at times, plak
  builds. That changes only wall time.
