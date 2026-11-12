# Isolation study on bare-metal x86-64 Linux (2026-11-05)

This is the Linux reference for the Apple Silicon run
([`../2026-11-05-m1max-aarch64-macos/`](../2026-11-05-m1max-aarch64-macos/)).
It is the same harness and commit, on bare metal rather than the orb's VM.
The comparison is in
[docs/research/isolation-modes.md](../../../../../docs/research/isolation-modes.md#apple-silicon-macos-and-bare-metal-linux-2026-11-05).

## Machine [M]

- "host1": a mini PC with an AMD Ryzen 9 8945HS (Zen 4, 8 cores, 16
  threads) and 28 GiB of RAM, not virtualised (`systemd-detect-virt`:
  `none`).
- CPU governor `performance`, boost on. One login, and idle apart from this
  run.
- Ubuntu 24.04.5 LTS, Linux 7.0.0-42-generic.
- Rust 1.99.0 (`x86_64-unknown-linux-gnu`), wasmtime 50, CPython 3.14.7,
  all from the unmodified Linux path of `.agents/setup`.
- The main run: `origin/main` a051ae6 with the harness port on top
  (6b02db1, the same tree as the Mac's 148f0fd). `at-ac10b28/` and
  `at-58d7aca/` are the same run at earlier rebases; at 58d7aca the port
  was in the working tree. On Linux the port changes nothing except
  `build.txt`'s extra lines. Each `build.txt` records the commit and every
  binary's SHA-256.

`bash bench/evidence/isolation/run.sh <this dir> all` produced `perf.jsonl`,
`faults.jsonl`, `report.md`, `build.txt` and `size.txt`. `cargo xtask check` was also run
on this host (see the macOS README for the per-gate results).
