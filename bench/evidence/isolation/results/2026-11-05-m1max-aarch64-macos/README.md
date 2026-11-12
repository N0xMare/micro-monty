# Isolation study and platform proof on Apple Silicon macOS (2026-11-05)

The comparison with the orb and with bare-metal Linux is in
[docs/research/isolation-modes.md](../../../../../docs/research/isolation-modes.md#apple-silicon-macos-and-bare-metal-linux-2026-11-05).
This directory holds the raw data.

| File | What |
|---|---|
| `perf.jsonl`, `faults.jsonl`, `report.md`, `build.txt`, `size.txt` | `bash bench/evidence/isolation/run.sh <this dir> all` at `origin/main` a051ae6 plus the harness port, including the mm-wasm v1 modes (`abi`, `abi-uu`) |
| `at-ac10b28/` | the same run at ac10b28, before mm-wasm v1 joined the harness (no `abi` modes) |
| `at-58d7aca/` | the same run at 58d7aca, before upstream's comparison-walk, conversion-charging and fuel-before-int-op commits |
| `crossarch.jsonl`, `crossarch.py` | snapshot chains alternating between this Mac and host1 (x86-64 Linux), at ac10b28 (`at-58d7aca/crossarch.jsonl`: the first run) |

## Machine [M]

- MacBook Pro, Apple M1 Max: 10 cores (8 performance, 2 efficiency), 32 GiB.
  On AC power with Low Power Mode off. The machine was otherwise idle except
  for this agent.
- macOS 27.2 (build 26B123), Darwin 27.2.0, xnu-13432.41.4, arm64.
- Rust 1.99.0 (`aarch64-apple-darwin`), wasmtime crate 50.0.1 in the
  harness, wasmtime CLI 50.0.0, binaryen `wasm-opt` 133, CPython 3.14.7
  (uv). All were installed by `.agents/setup`, which gained a macOS branch
  for this run.
- The main run: `origin/main` a051ae6 with this port committed on top
  (148f0fd here, the same tree as 6b02db1 on host1; `git am` gave it a
  different hash there). `at-ac10b28/` and `at-58d7aca/` are earlier
  rebases; at 58d7aca the port was uncommitted in the working tree. Each
  `build.txt` records the commit and every binary's SHA-256. The engine is
  the same at a051ae6 and ac10b28.
- macOS cannot pin threads. The scale section used min(N, 10) threads here,
  against 16 on the orb and on host1.

## What was ported, and why

| Linux assumption | macOS replacement |
|---|---|
| `run.sh`: `realpath -m` (BSD `realpath` rejects `-m`, so `set -e` stopped the run), `nproc`, `sha256sum`, `stat -c %s` | create the directory, then `cd && pwd`; `getconf _NPROCESSORS_ONLN`; `shasum -a 256` when `sha256sum` is missing; `wc -c`. `build.txt` now also records `uname -a` and `sw_vers` |
| RSS and PSS read from `/proc/<pid>/status` and `smaps_rollup`, and `VmHWM` from `/proc/self/status` (on macOS every read returned 0) | `proc_pid_rusage(RUSAGE_INFO_V2)`: `ri_resident_size` for RSS and `ri_phys_footprint` in the PSS column, with the report labelling it "phys_footprint"; `getrusage` `ru_maxrss` for peak RSS. This is a small `libc` FFI block in the harness, owner-approved |
| the rlimit rows run the worker under `prlimit`, which macOS lacks | `setrlimit` in a `pre_exec` hook. A limit the kernel refuses becomes a "not started" row rather than a harness error |
| seccomp | not available on macOS. Not measured, and `mm-cell-worker` builds without its filter here by design |

macOS has no PSS. phys_footprint (private dirty plus compressed memory) is
the closest per-process figure but is not the same thing: it counts the
private pages of the shared binary that PSS would split across workers.
**Compare the worker columns across hosts only roughly.**

## Platform proof [M]

The gates below were first run one by one at 58d7aca. After the rebase onto
ac10b28, `cargo xtask check` was run in full on both hosts.

| Gate | aarch64 macOS (this Mac) | x86-64 Linux (host1) |
|---|---|---|
| `cargo fmt --all --check` | pass | pass |
| `cargo clippy … -D warnings` | **failed at first**: `mm-cell-worker`'s seccomp stub for platforms other than Linux hit `unused_self` and `unnecessary_wraps`. Tooling, now fixed | pass |
| `cargo test --workspace` | **failed at first**: `own_cases_match_cpython`, 7 cases (see below), and, in the full `check`, two xtask bench tests that run `/bin/false`, which macOS lacks (now `/usr/bin/false`). Tooling and oracle, now fixed | **failed at first**: `308_time_struct` (the host's zone is America/New_York; the orb runs in UTC). Fixed by `TZ=UTC` |
| `cargo xtask parse-diff` | pass | pass |
| `cargo xtask size` (at ac10b28) | `hello.wasm` 1,922,566 B (budget 1,924,500); native `hello` 1,806,784 B, reported only (Mach-O, aarch64; 1,790,304 B at 58d7aca) | `hello.wasm` 1,922,566 B; native `hello` 2,012,704 B (budget 2,014,500) |
| `embedding_bench` | pass | pass |
| `cargo xtask wasm` | pass, round trip on both wasm targets under wasmtime; build id `9538bd20…` | pass, same build id |
| `conformance --check` (CPython 3.14.7) | pass | pass |
| `conformance --engine micro-monty --check` | pass, 609 of 726 as recorded | pass |
| `durability` | 1,158 of 1,158 runs identical (99,699 parks at 58d7aca, 99,705 at ac10b28) | 1,158 of 1,158 (99,687 and 99,693 parks) |
| `durability --cross-target` | 346 of 346; aarch64 native to wasm and back | 346 of 346 |
| `bench --quick` | every checksum matched | every checksum matched |

The two hosts' park counts differ by 12 out of about 99,700. The cause was
not investigated. Every run matched its unparked report on both hosts.

**No engine bug was found.** All 173 own cases produce byte-identical output
(stdout, stderr and exit status) from `mm` on aarch64 macOS and on x86-64
Linux [M]. Each of the 7 macOS failures is CPython on macOS disagreeing with
CPython on Linux, where mm agrees with Linux:

| Cases | CPython on macOS | mm and CPython on Linux | Handling |
|---|---|---|---|
| `105_bigint`, `52_builtin_error_messages` | `strerror(ERANGE)` = "Result too large" | "Numerical result out of range" | not compared on macOS |
| `107_math`, `202_complex_cmath` | Apple libm: `tan`, `atan`, `sinh`, `cosh`, `expm1`, `log1p` and some `cmath` results differ in the last bit | glibc values | not compared on macOS |
| `308_time_struct`, `315_logging` | `gmtime().tm_zone` = "UTC" | "GMT" | not compared on macOS |
| `324_deep_class_tuples_and_native_chains` | `RecursionError: Stack overflow (used 8144 kB)`: the 8 MiB main-thread stack is too small for CPython's own guard | `True …` | compared, with CPython run under `ulimit -s hard` (64 MiB) |

These are listed in `ORACLE_DIFFERS` in `crates/mm-cli/tests/conformance_own.rs`.

**Snapshots across architectures** (`crossarch.jsonl`, `crossarch.py`): every
own case was parked about 6 times. The chain alternated between native mm on
this Mac and native mm on host1, once starting on each, and copied the
snapshot directory with every hop. **346 of 346 chains matched their unparked
report, over 2,000 restores across architectures [M]**, both at 58d7aca and
at ac10b28. In the first run (58d7aca) 39 chains first differed only in
traceback file paths. macOS resolves the unparked run's
`/tmp` to `/private/tmp` and that path is printed in tracebacks; this was the
driver's doing. Those 39 records were checked to be equal after mapping
`/private/tmp/` to `/tmp/`, and they carry a `note`. `crossarch.py` now
applies the same mapping; the ac10b28 run needed no note.

## Fault rows that differ from Linux [M]

At a051ae6, 98 of the 104 rows match host1 and the orb, which agree on all
104. The six that differ:

- the shipped panic signal (SIGTRAP, not SIGILL), native and child;
- `RLIMIT_CPU` (SIGXCPU, not SIGKILL), for both runaway shapes;
- the two `RLIMIT_AS` rows. macOS refuses `setrlimit(RLIMIT_AS)` with
  EINVAL, as it refuses `ulimit -v` and `ulimit -d`, so these rows record
  "not started". macOS has no per-process memory limit a host can set, and
  no cgroups.

The table is in
[isolation-modes.md](../../../../../docs/research/isolation-modes.md#fault-matrix-macos-against-linux).

At 58d7aca the cyclic-dict stack rows at 512 KiB also differed: they gave
`RecursionError` here where Linux aborted, because aarch64 frames are
smaller. From ac10b28 on, every stack row gives `RecursionError` or `True`
on every host.
