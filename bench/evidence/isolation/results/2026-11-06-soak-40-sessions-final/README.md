# 40 concurrent sessions on the Mac at 874409b and c97ce06 (2026-11-05/06)

This repeats `../2026-11-05-soak-40-sessions/` (5568b27, same cell, harness
and machine) after the speed landing, the asyncio primitives and the
aware-datetime fast paths (874409b, 5 min per row), and again at the final
head after the size pass (c97ce06, 2 min per row). The machine is a MacBook
Pro M1 Max, macOS 27.2, on AC power. Every number is [M].

```bash
ISO_SOAK_SESSIONS=40 ISO_SOAK_SECS=300|120 ISO_SOAK_THINK_MS=0|100 ISO_MODES=native|abi|abi-uu \
  target/isolation/release/iso-harness perf soak      # one process per row
```

**Contention.** The owner's plak work kept running on the Mac. Each row
started only after 30 s with no `cargo` or `rustc` process (`run-874409b.sh`
and the c97ce06 driver). `sampler.sh` logged builds and load every 10 s
(`contention.log`). **No cargo or rustc process ran in any row's window.**
The 1-minute load reached 100–240 in the saturated (think 0) rows. That is
the soak's own 40 runnable threads plus the desktop apps, against 4–14 in
the paced rows. The 5568b27 run had the same desktop load and no sampler.

## Results

Round trip and cell are p50 / p99 / p99.9. Host RSS is before the sessions /
with 40 idle sessions / loaded peak. Per-session RSS is idle / loaded peak.

| head | mode | think | cells | cells/s | host calls/s | errors | round trip | cell | host RSS | per session | linear memory |
|---|---|---|---|---|---|---|---|---|---|---|---|
| 5568b27 | native | 0 | 3,267,082 | 10,890 | 2.25 M | 0 | 2.1 / 38 / 133 µs | 0.68 / 62 / 106 ms | 5 / 8 / 51 MiB | 80 / 1,173 KiB | — |
| 874409b | native | 0 | 3,165,394 | 10,551 | 2.18 M | 0 | 2.0 / 36 / 117 µs | 0.69 / 66 / 139 ms | 5 / 8 / 54 MiB | 77 / 1,247 KiB | — |
| c97ce06 | native | 0 | 1,236,290 | 10,302 | 2.13 M | 0 | 2.1 / 39 / 125 µs | 0.72 / 74 / 155 ms | 5 / 8 / 51 MiB | 76 / 1,176 KiB | — |
| 5568b27 | abi | 0 | 1,575,210 | 5,251 | 1.09 M | 0 | 5.0 / 87 / 3,139 µs | 1.39 / 82 / 132 ms | 8 / 20 / 56 MiB | 315 / 1,230 KiB | 2,240 KiB |
| 874409b | abi | 0 | 1,541,913 | 5,140 | 1.06 M | 0 | 4.9 / 83 / 6,687 µs | 1.40 / 94 / 180 ms | 17 / 48 / 87 MiB | 793 / 1,780 KiB | 2,368 KiB |
| c97ce06 | abi | 0 | 630,311 | 5,252 | 1.09 M | 0 | 5.0 / 86 / 632 µs | 1.42 / 98 / 175 ms | 130* / 160 / 198 MiB | 778 / 1,729 KiB | 2,368 KiB |
| 5568b27 | abi-uu | 0 | 1,619,734 | 5,399 | 1.12 M | 0 | 5.0 / 87 / 2,260 µs | 1.37 / 82 / 135 ms | 8 / 21 / 58 MiB | 326 / 1,265 KiB | 2,304 KiB |
| 874409b | abi-uu | 0 | 1,644,097 | 5,480 | 1.13 M | 0 | 4.9 / 83 / 1,890 µs | 1.39 / 91 / 178 ms | 18 / 49 / 87 MiB | 794 / 1,777 KiB | 2,560 KiB |
| c97ce06 | abi-uu | 0 | 660,553 | 5,504 | 1.14 M | 0 | 4.9 / 82 / 493 µs | 1.39 / 99 / 180 ms | 132* / 162 / 200 MiB | 780 / 1,744 KiB | 2,560 KiB |
| 5568b27 | native | 100 ms | 113,202 | 377 | 78 k | 0 | 2.0 / 39 / 45 µs | 0.72 / 1.0 / 2 ms | 5 / 8 / 38 MiB | 81 / 847 KiB | — |
| 874409b | native | 100 ms | 111,247 | 371 | 77 k | 0 | 2.0 / 38 / 51 µs | 0.78 / 1.2 / 2 ms | 5 / 8 / 48 MiB | 75 / 1,080 KiB | — |
| c97ce06 | native | 100 ms | 44,401 | 370 | 77 k | 0 | 2.1 / 42 / 54 µs | 0.84 / 1.2 / 2 ms | 5 / 8 / 47 MiB | 77 / 1,056 KiB | — |
| 5568b27 | abi | 100 ms | 112,376 | 374 | 78 k | 0 | 4.7 / 83 / 91 µs | 1.39 / 1.6 / 2 ms | 8 / 20 / 52 MiB | 314 / 1,138 KiB | 2,240 KiB |
| 874409b | abi | 100 ms | 111,104 | 370 | 77 k | 0 | 4.7 / 80 / 88 µs | 1.43 / 1.6 / 2 ms | 17 / 48 / 82 MiB | 793 / 1,659 KiB | 2,368 KiB |
| c97ce06 | abi-uu | 100 ms | 44,159 | 368 | 76 k | 0 | 4.6 / 82 / 103 µs | 1.43 / 2.0 / 4 ms | 17 / 48 / 84 MiB | 794 / 1,727 KiB | 2,560 KiB |

\* In the first `abi` and `abi-uu` rows after the guests were rebuilt,
`mm_wasm_host::Runtime::new` compiled the module in-process. Its 130 MiB
baseline is that compile, which happens once per new guest. The paced
`abi-uu` row after it started from 17 MiB. Per-session figures are
deltas from each row's own baseline.

## What changed

- **Still no errors, no growth.** That makes 9.19 M more cells and 1.90 billion
  host round trips, all correct. RSS after 60 s and at the end differ by
  under 2 MiB in every row.
- **Native is unchanged.** Throughput is within 5%, and the per-session
  footprint is the same: about 77 KiB idle and 1.1–1.25 MiB loaded. The
  host-call median stays at 2.0–2.1 µs. On this cell the speed landing does
  not show at the cell level. The cell spends most of its time in host round
  trips and `json.loads`, and the bench programs it sped up (generators,
  classes, fmt) are not in it.
- **mm-wasm sessions cost about 2.5× more idle memory than at 5568b27.**
  Per-instance idle RSS rose from 315–326 KiB to 778–794 KiB. A
  runtime's own baseline rose from 8 to 17 MiB. 40 loaded mm-wasm sessions
  now take 82–87 MiB of host RSS instead of 52–58 MiB. Linear memory per
  instance grew only from 2,240 to 2,368 KiB (abi) and from 2,304 to
  2,560 KiB (abi-uu), so the guest touches more of its memory at session
  creation. **Root cause (found afterwards, fixed in mm-wasm-host):** not
  the guest. 9917a48 (content-keyed `.cwasm` cache) loaded the module with
  `Module::deserialize(bytes)` instead of `deserialize_file`. wasmtime then
  has no file to map the data segment copy-on-write from, and its fallback,
  a memfd, exists only on Linux, so on macOS every instance copied the
  615 KB data segment eagerly; the runtime also kept the read `.cwasm` in
  anonymous memory (the 8 → 17 MiB baseline). On the orb with wasmtime's
  copy-on-write turned off (macOS's path) the 874409b guest measures
  694–714 KiB idle per instance against 300–387 with it, and the fixed
  host (`deserialize_file`) 450 KiB with a 6.9 MiB runtime baseline
  instead of 12.5 MiB [M, Linux]. The guest itself grew only about 38 KiB
  per instance from 5568b27 (data section 583 → 615 KB). Confirmed on the
  Mac at b057751: 329–330 KiB idle per instance, 40 loaded sessions 61–66 MiB.
- **The remaining runtime baseline (11 MiB on the Mac at b057751, 8 at
  5568b27)** has two parts, measured on the orb with each host on the same
  guest, warm cache [M, Linux]. (1) Since 9917a48 the host read the whole
  2.4 MB module on every load to compute the cache key. glibc returns that
  to the OS, but an allocator that keeps freed memory, as macOS's does, keeps
  it resident: with glibc made to retain (`MALLOC_MMAP_THRESHOLD_`,
  `MALLOC_TRIM_THRESHOLD_`) the runtime's anonymous memory goes from 0.2 to
  2.6 MiB. Fixed: the key is now hashed in 64 KiB chunks (same key, so
  existing caches stay valid) and the module is read whole only to compile
  it; anonymous memory is back to 0.26 MiB with the retaining allocator.
  (2) About 1.4 MiB of `.cwasm` code pages (file-backed) are touched at
  `Runtime::new` instead of at the first session, because U1's
  `Runtime::load` instantiates the guest once to read its build id. That is
  the same memory a first session touches anyway: after one session both
  hosts are at 7.4–8.0 MiB. The guest's larger data segment does not show
  in the baseline (5568b27 and current guests within 0.3 MiB on either
  host), and the wasmtime configuration did not change.
- **Saturated tails are a little longer** (cell p99 62–82 → 74–99 ms). That
  is time-slicing of 40 threads on 10 cores, with more desktop load than on
  2026-11-05. The medians did not move.

## Rerun at b057751: the mm-wasm idle-RSS fix (2026-11-06)

The coordinating thread traced the growth to the host, not the guest.
9917a48 switched `mm-wasm-host` to `Module::deserialize(bytes)`, which loses
copy-on-write linear memory on macOS. b057751 goes back to
`deserialize_file`. This run uses the same harness, guests from c97ce06
(b057751 changes only the host) and 120 s per row
(`soak-b057751.jsonl`, `run-b057751.sh`). Two 5 s warm-up rows first
rebuilt the `.cwasm` caches under their new file names; they are not in the
table. Every number is [M].

| head | mode | think | cells/s | errors | cell p50 / p99 | host RSS before / 40 idle / loaded peak | per session idle / loaded |
|---|---|---|---|---|---|---|---|
| 5568b27 | abi | 0 | 5,251 | 0 | 1.39 / 82 ms | 8 / 20 / 56 MiB | 315 / 1,230 KiB |
| 874409b | abi | 0 | 5,140 | 0 | 1.40 / 94 ms | 17 / 48 / 87 MiB | 793 / 1,780 KiB |
| **b057751** | abi | 0 | 4,553 † | 0 | 1.43 / 74 ms | **11 / 24 / 64 MiB** | **330 / 1,370 KiB** |
| **b057751** | abi-uu | 0 | 4,794 † | 0 | 1.40 / 77 ms | **11 / 23 / 66 MiB** | **329 / 1,420 KiB** |
| **b057751** | abi | 100 ms | 380 | 0 | 1.52 / 3.7 ms † | **11 / 23 / 61 MiB** | **329 / 1,278 KiB** |
| **b057751** | abi-uu | 100 ms | 380 | 0 | 1.49 / 3.4 ms † | **11 / 23 / 62 MiB** | **329 / 1,304 KiB** |
| b057751 | native (control) | 0 | 7,774 † | 0 | 0.74 / 57 ms | 5 / 8 / 51 MiB | 76 / 1,164 KiB |

- **Fixed.** Idle RSS per mm-wasm instance is back to 329–330 KiB, against
  315–326 KiB at 5568b27 and 778–794 KiB at 874409b and c97ce06. Linear
  memory per instance is unchanged (2,368 / 2,560 KiB), so pages are shared
  copy-on-write again. 40 loaded mm-wasm sessions take 61–66 MiB of host
  RSS, against 82–87 MiB before the fix.
- **The runtime baseline is 11 MiB**, against 8 MiB at 5568b27 and 17 MiB at
  874409b. The process without a runtime is unchanged at 5 MiB (native). So
  about 3 MiB of the runtime's growth since 5568b27 remains. It is not
  per instance. Cause and fix (2fa2e3e): see "The remaining runtime
  baseline" above: a whole-module read for the cache key that macOS's
  allocator keeps (+2.4 MiB, fixed), and code pages U1's build-id instance
  touches before the first session (~1.4 MiB, not extra memory).
- † **Throughput and latency in this run are contended.** No cargo or rustc
  ran in any row (`contention.log`), but the owner's plak process used about
  1.4 cores throughout, and load reached 100–200 in the saturated rows. Even
  the native control fell to 7,774 cells/s from 10.3–10.9 k. Compare the
  memory columns only. Zero errors in every row.

## Final head 5349520: runtime-baseline check (2026-11-06)

This is a short rerun at 5349520, the final head for report 3. It includes
b057751's copy-on-write fix and 2fa2e3e/239b26d, which stop reading the
whole module for the cache key. It uses the same harness and cell, 120 s
per row, with guests and harness built at 5349520 (`build-5349520.txt`,
`run-5349520.sh`, `soak-5349520.jsonl`). The run was 16:57–17:09Z. Two 5 s
warm-up rows first recompiled the `.cwasm` caches for the new guests; they
are not in the table. Every number is [M].

**Load.** No `cargo` or `rustc` ran in any row (`contention-5349520.log`).
The owner's `plak --resume` session used 33–42% of one core, much less than
the ~1.4 cores during the b057751 run. The native control's 11,197 cells/s
matches the uncontended 10.3–10.9 k, so throughput and latency here are
comparable to the 5568b27–c97ce06 rows.

| head | mode | think | cells/s | host calls/s | errors | round trip p50 / p99 / p99.9 | cell p50 / p99 | host RSS before / 40 idle / loaded peak | per session idle / loaded peak | linear memory |
|---|---|---|---|---|---|---|---|---|---|---|
| 5568b27 | abi | 0 | 5,251 | 1.09 M | 0 | 5.0 / 87 / 3,139 µs | 1.39 / 82 ms | 8 / 20 / 56 MiB | 315 / 1,230 KiB | 2,240 KiB |
| b057751 | abi | 0 | 4,553 † | 0.94 M | 0 | 4.8 / 83 / 9,103 µs | 1.43 / 74 ms | 11 / 24 / 64 MiB | 330 / 1,370 KiB | 2,368 KiB |
| **5349520** | abi | 0 | 5,630 | 1.17 M | 0 | 4.8 / 86 / 550 µs | 1.46 / 84 ms | **8.9 / 21.8 / 62 MiB** | **328 / 1,371 KiB** | 2,368 KiB |
| **5349520** | abi-uu | 0 | 5,781 | 1.20 M | 0 | 4.7 / 81 / 4,581 µs | 1.35 / 80 ms | **8.8 / 21.6 / 65 MiB** | **330 / 1,431 KiB** | 2,560 KiB |
| **5349520** | abi | 100 ms | 381 | 79 k | 0 | 4.6 / 83 / 100 µs | 1.44 / 1.9 ms | 8.9 / 21.8 / 59 MiB | 329 / 1,274 KiB | 2,368 KiB |
| **5349520** | abi-uu | 100 ms | 381 | 79 k | 0 | 4.6 / 85 / 104 µs | 1.44 / 2.1 ms | 8.8 / 21.6 / 61 MiB | 329 / 1,338 KiB | 2,560 KiB |
| **5349520** | native | 0 | 11,197 | 2.32 M | 0 | 2.0 / 37 / 138 µs | 0.69 / 48 ms | 5.8 / 8.8 / 53 MiB | 76 / 1,219 KiB | — |

† Contended (see the b057751 section).

- **The runtime baseline is fixed.** `before_sessions` is 8.8–8.9 MiB, in
  the expected 8.5–9 MiB, against 11 MiB at b057751, 17 MiB at 874409b and
  8 MiB at 5568b27. 40 idle sessions add 12.8 MiB, which is 328–330 KiB per
  instance and unchanged from b057751.
- **The unknown-unknown guest with c97ce06's allocator** behaves like
  wasip1. Idle and loaded RSS per session are within 5%. Its linear memory
  stays 2,560 KiB against wasip1's 2,368 KiB.
- **Throughput** is the best of any head on this machine: mm-wasm 5,630 and
  5,781 cells/s, against 5,140–5,504 before; native 11,197 against
  10,302–10,890. Cell and host-call medians are unchanged.
- The two warm-up rows, which compiled the guests in-process, started at
  129 and 134 MiB of host RSS because of the cache build. That cost is paid
  only once per new guest.
- Zero errors in every row.
