# 40 concurrent agent sessions on Apple Silicon macOS (2026-11-05)

The owner's expected load on a Mac: about 40 agents at once, each running
codemode cells against host tools. Every number here is [M] on the MacBook
Pro M1 Max (8 performance + 2 efficiency cores, 32 GiB, macOS 27.2, AC
power) at 5568b27. The machine also ran the owner's desktop apps (load
average about 4 before the run); nothing else from this work ran alongside.

```bash
ISO_SOAK_SESSIONS=40 ISO_SOAK_SECS=300 ISO_SOAK_THINK_MS=0|100 ISO_MODES=native|abi|abi-uu \
  target/isolation/release/iso-harness perf soak      # one process per row
```

`soak` (in `harness/src/perf.rs`) builds 40 sessions, each on its own thread,
and runs this cell in each for 5 minutes. Each cell makes 100 sync `tool`
calls (JSON answers parsed with `json.loads`), awaits one future, runs 5
`asyncio.gather` batches of 20 futures, and keeps a bounded history in a
global across cells: 201 host calls, 207 host round trips counting the
`Waiting` batches. Every output is checked (`4950 4957`). A round trip is
timed from the host's answer to the next stop coming back, so it includes the
Python code between two calls. Latencies go into 1%-wide log histograms
(the soak's own bookkeeping is a few KiB per thread). Host RSS is sampled
every second. Modes: `native` (in-process facade), `abi` and `abi-uu`
(mm-wasm v1 through `crates/mm-wasm-host`, wasm32-wasip1 and
wasm32-unknown-unknown guests, one instance per session).

| mode | think | cells | cells/s | host calls/s | errors | round trip p50 / p99 / p99.9 | cell p50 / p99 / p99.9 | host RSS total: before / 40 idle / loaded peak | RSS per session: idle / loaded | linear memory per instance |
|---|---|---|---|---|---|---|---|---|---|---|
| native | 0 | 3,267,082 | 10,890 | 2.25 M | 0 | 2.1 / 38 / 133 µs | 0.68 / 62 / 106 ms | 5 / 8 / 51 MiB | 80 / 1,173 KiB | — |
| abi | 0 | 1,575,210 | 5,251 | 1.09 M | 0 | 5.0 / 87 / 3,139 µs | 1.39 / 82 / 132 ms | 8 / 20 / 56 MiB | 315 / 1,230 KiB | 2,240 KiB |
| abi-uu | 0 | 1,619,734 | 5,399 | 1.12 M | 0 | 5.0 / 87 / 2,260 µs | 1.37 / 82 / 135 ms | 8 / 21 / 58 MiB | 326 / 1,265 KiB | 2,304 KiB |
| native | 100 ms | 113,202 | 377 | 78 k | 0 | 2.0 / 39 / 45 µs | 0.72 / 1.0 / 2 ms | 5 / 8 / 38 MiB | 81 / 847 KiB | — |
| abi | 100 ms | 112,376 | 374 | 78 k | 0 | 4.7 / 83 / 91 µs | 1.39 / 1.6 / 2 ms | 8 / 20 / 52 MiB | 314 / 1,138 KiB | 2,240 KiB |

- **No errors and no growth.** Over 6.7 million cells and 1.38 billion
  host round trips, every output was correct. Host RSS after 60 s and at the end
  differ by under 2 MiB in every row, so the sessions do not leak.
- **Memory is small.** 40 loaded sessions cost 38–58 MiB of host RSS in
  total, about 1.1–1.3 MiB each, wasm instances included (RSS counts only
  touched linear-memory pages).
- **Latency at a realistic pace is flat.** With 100 ms of think time between
  cells (still far busier than an agent waiting on a model), a cell takes
  0.72 ms native and 1.39 ms on mm-wasm, with p99 1.0 and 1.6 ms. The
  round-trip p99 (39 / 83 µs) is the `Waiting` batch resumes, which run 20
  `json.loads` each (3% of round trips), not outliers.
- **Saturated, the tail is scheduling.** Running back to back, 40 threads
  share 10 cores; the cell median stays at 0.68 / 1.39 ms, but p99 rises to
  62–82 ms, which is macOS time-slicing, not engine work.
- mm-wasm costs about 2× native per cell here and the same memory.
