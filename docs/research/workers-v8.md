# micro-monty on V8 and Cloudflare Workers

Commit hashes of micro-monty cited here, other than the public history's
(root `2bcbb8c`), are pre-release history, kept in the private archive
`N0xMare/micro-monty-archive` (branch `history-0.0.1`;
[docs/history](../history/README.md) maps them); the numbers they qualify
are unchanged.

How does micro-monty perform as a code-mode runtime in V8, on Cloudflare
Workers and Durable Objects, next to pi's codemode on quickjs-wasi? And what
does its binary size (the shipped guest is 2.23 MB against quickjs-wasi's
637 KB) actually cost there? Every earlier measurement used wasmtime
([wasm-embedding.md](wasm-embedding.md),
[js-engines-compare.md](js-engines-compare.md)), and mm-wasm had no
JavaScript host. This note adds one ([`hosts/js`](../../hosts/js)) and
measures it in Node and in workerd, Cloudflare's open-source runtime, run
locally. Two follow-ups extend it: K sessions per wasm instance (§7), and
the CPU time a fuel unit stands for, which is the only CPU bound a Worker
has (§8).

Numbers are labelled [M] measured, [S] sourced or [E] estimated. [M] numbers
are medians over 5 rounds on one 16-vCPU Amp orb (Intel Xeon @ 2.60 GHz,
Linux 6.1). Every measurement is a fresh process pinned to cores 2–5, with
the engine order rotated each round. mm is at `8cd850c`, which includes the
guest's 832 KiB shadow stack (`1958bac`); the first version of this note
measured `7dfa304`, with Rust's default 1 MiB. Node is v26.12.1 (V8 14.6),
and workerd is the npm binary `workerd 2026-11-04`. Raw data, harness and
summary are in
[`hosts/js/bench/results/2026-11-09/`](../../hosts/js/bench/results/2026-11-09/).

## Summary

- **Memory binds first on Workers.** Each session is by default its own
  wasm instance, with its own linear memory: 1.57 MB when idle after setup
  (pi's VM: 1.64 MB) [M]. That memory grows up to about the session's
  memory limit plus 2 MB, and it never shrinks while the instance lives
  [M]. The isolate has 128 MB in total, shared by concurrent requests and
  by co-located Durable Objects [S]. So an isolate holds about 55 idle
  sessions, or 5 sessions that are filling a 16 MiB limit [E]. A hostile
  cell can take more than that fill. mm's documented bound is about
  3 × limit + 16 MiB of linear memory (64 MiB at 16 MiB) [S, sdk.md], so
  only one such session fits per isolate (see Settings).
- **K sessions per instance cut idle memory 7×.** With the new opt-in
  `sessionsPerInstance`, an idle set-up session costs 1,572,864 bytes of
  linear memory alone in an instance, 491,520 at K = 4 and 225,280 at
  K = 16 (about 0.1 MB for each session past the first) [M], and a session
  in a free slot is created in 0.9–1.0 ms instead of 1.9 ms in Node (0.7–0.8
  against 1.5 ms in workerd) [M].
  The price: a trap loses all K sessions, and one session's peak stays in
  the shared instance until every session in it has ended (§7).
- **A fuel unit is 0.03–0.5 µs on V8, but fuel does not bound one native
  step.** The bench programs run at 0.029–0.45 µs per charged unit in
  workerd (native: 0.014–0.25), and a code-mode cell at about 0.85 µs,
  most of it compiling, imports and host calls, which charge little fuel
  [M]. Natives that charge after they run overdraw a slice: at a 16 MiB
  limit the longest such step is 132 ms in workerd (`str.translate` on 4M
  characters), and at the default 256 MiB it is 2.1 s. Garbage collection
  is not charged at all [M] (§8).
- **CPU binds second, and only on the Free plan.** A warm code-mode task
  costs 2.0 ms of CPU on mm (median of 22 tasks; the largest is 8 ms)
  against 7.0 ms on pi in workerd [M]. That is under the Free plan's
  10 ms. The first session in a new isolate costs 24 ms on mm and 37 ms on
  pi, because V8 compiles lazily the functions that the first session
  touches [M]. Running one session in the Worker's global scope moves that
  work into startup. The first request then costs 3 ms on mm (14 ms on pi),
  and startup grows by 30–40 ms of its 1 s limit [M]. A hostile cell can
  exceed 10 ms in one native step whatever its fuel (§8).
- **Size does not bind.** The guest is 3.3% of the 64 MiB Worker limit
  [M, S]. workerd starts about 10 ms slower with it than with no module
  [M]. V8 validates the module in 4 ms and compiles it lazily, 23 ms if
  compiled eagerly. Loading it costs 3.8 MB of RSS, against 1.2 MB for
  quickjs-wasi [M]. Where size does show up is in every session's linear
  memory: each instance gets its own copy of the data segments (469 KB in
  the default build, 114 KB in quickjs-wasi) [M]. K sessions per instance
  share one copy.
- **Fewer stdlib modules buy little.** `core35` is 85 KB (3.8%) smaller
  and uses 131 KB (8%) less linear memory per session. Compile time, session
  time and the workload stay within noise [M]. The build without stdlib
  modules saves 475 KB and 328 KB per session, but 17 of the 22 tasks then
  fail on `import asyncio`/`re`/`json` [M].
- **Per session, mm beats pi's engine in V8, as it did under wasmtime.**
  The 22-task workload takes 61 ms on mm against 150 ms on pi in workerd
  (Node: 60 against 141) [M]. An awaited tool call costs 8.5 µs against
  18.5 µs, and a call in a fan-out 5.0 µs against 16.5 µs [M]. A parked
  mid-await image is 73–175 KB (18–43 KB gzip) and restores in 1.5–2 ms
  [M]. quickjs-wasi's own snapshot is the whole 1.6 MB of linear memory
  (210–211 KB compressed) [M], and pi does not use it [S].
- **Hibernate-and-resume works on Durable Objects.** A cell waiting on 1–40
  tools is parked in the object's SQLite storage (snapshot 2–3 ms, write
  under 1 ms). workerd is then killed, and a new process restores and
  finishes the cell: 9 of 9 runs, all of them judged correct by the real
  checker [M].
- **Settings for code mode on Workers:** `limits.memory` 16 MiB; cells over
  16 KiB refused; fuel slices of 10,000 (`ASYNC_SLICE_FUEL`) with a yield
  between them; a per-cell fuel budget counted from what the engine charged,
  checked with `fuelWanted()`; at most 4 sessions running at once per
  isolate (1 if admitting by the worst-case bound); K = 1 for sessions that
  are parked between requests, K = 4 for a Worker that holds many live
  sessions; every session that waits longer than a tool round trip is
  parked to storage and dropped; one session in the global scope on the
  Free plan. Details are in "What it means for code mode on Workers" below.

## What was built

[`hosts/js`](../../hosts/js) is a TypeScript host for the shipped
wasm32-unknown-unknown guest. It has no dependency and no build step:
erasable syntax only, so Node runs it as it is and Wrangler's esbuild strips
the types. It has three parts:

- `src/abi.ts`: the v2 codec, about 1,000 lines. Like the Rust codec, its
  decoders refuse unknown tags.
- `src/host.ts`: `Runtime` (loaded builds keyed by build id: `load`,
  `restore` by the image's build, `unload`, `stats`) and `Session`, with
  every v2 export plus the optional `mm_pending`. A session has an instance
  of its own, or with `sessionsPerInstance` K a slot in a shared one (§7).
- `src/driver.ts`: the async code-mode loop. Async tools are answered as
  futures, fan-outs are answered in batches, and fuel slices yield to the
  event loop under a per-cell budget. The budget counts the fuel the engine
  charged, read from the session's fuel total, so overdrafts count (§8).

`test/host.test.ts` runs the scenarios of the reference host's tests
(`parity.rs`, `upgrade.rs`) on the shipped guest: structured host errors
and exception classes, `dict_as_kwargs`, call log, hints, text cut, zones,
`fuel_wanted` before a big int, stubs, host modules, dunders, int-bits
text, GC stress, fuel total, phases, typed define, resume and snapshot
errors, truncation, `pending` across restores, exact value round trips,
park requests between calls and from inside a cell, a park from another
thread through a shared `Int32Array`, deadlines, two loaded builds, and the
driver's awaited calls, fan-outs, sync tools, slices and budget (an
overdrawing native included). Two more cover K sessions per instance:
placement and slot reuse, isolation of globals, output and park requests
between sessions that are stopped mid-cell in one instance, a restore into
a shared instance, and a trap that ends exactly its instance's sessions.
All 14 tests pass [M]. The reference host compares the guest with the
native facade on the same bytes; these tests check the facade's documented
results directly.

Three differences from wasmtime follow from V8, and the README documents
them:

- **No grace interrupt.** V8 has no epoch interruption. A park request
  parks at the next safepoint check, as under wasmtime, but one long native
  step cannot be stopped at all: §8 measures how long one can be. Fuel
  with `fuelWanted()` is the guard. Otherwise the platform's limit is the
  last resort: a worker's `terminate()` in Node, the CPU limit on Workers.
  pi notes that the CPU limit resets a whole Durable Object [S].
- **No deadline on deployed Workers.** `Date.now()` and `performance.now()`
  stand still during synchronous execution [S, pi's `inline.ts`], so
  `setDeadline` only fires after I/O. Fuel is the CPU bound there. Local
  workerd's clock does advance, which is what makes the in-Worker timings
  below possible.
- **No store memory limit.** `limits.memory` (the arena) is the cap.

## 1. Size and compile [M]

| | mm default | mm core35 | mm no stdlib | quickjs-wasi (pi) | Monty wasm |
|---|---:|---:|---:|---:|---:|
| Module bytes | 2,234,340 | 2,148,875 | 1,759,112 | 637,405 | 25,451,300 |
| gzip -9 | 1,087,754 | 1,005,586 | 710,322 | 286,642 | 8,036,302 |
| Data segments (copied into every instance) | 468,757 | 386,941 | 136,642 | 113,905 | 3,011,288 |
| Shadow stack at the bottom of linear memory | 832 KiB | 832 KiB | 832 KiB | 1 MiB | — |
| Node: compile, lazy (V8 default), ms | 3.98 | 4.27 | 3.81 | 1.44 | ~36 |
| Node: compile, `--no-wasm-lazy-compilation`, ms | 23.2 | 26.5 | 21.6 | 8.8 | ~215 |
| Node: RSS added by compiling | 3.8 MB | 3.5 MB | 3.2 MB | 1.2 MB | — |
| workerd: spawn to first response, ms (no module: 33.7) | 45.3 | 52.8 | 48.7 | 41.2 | — |
| … with one session run in the global scope | 81.6 | 73.6 | — | 72.7 | — |

The guests are built as `cargo xtask size` ships them
(`hosts/js/scripts/build-guests.py`). `core35` and the build without stdlib
modules replace the facade's default feature list with docs/sdk.md's
code-mode list (34 features) or with nothing, in git worktrees with the
checkout's path length, as stdlib-profile.md did. quickjs-wasi is pi's
pinned npm 3.6.2. The workerd spread across rounds is about ±5 ms, so the
module costs workerd roughly 10 ms of startup.

**Monty** ([S], `@pydantic/monty` 1.5.0 on npm, published 2026-11-06): its
`@pydantic/monty/wasm` build is a WASI 0.2 component. Its core module is
25.45 MB (22.4 MB code, 3.0 MB data), and it runs in a Web Worker or a Node
worker thread, never on the calling thread. Workers have no threads [S], so
it does not run on Workers as packaged. Its size and compile times above
were measured outside the campaign (three runs, first version) [M]. At 11×
mm's size, it still fits the 64 MiB limit.

**What V8 does with the bytes** [S]. Workers compile a Worker's wasm modules
before its code runs. Since 2020 they use Liftoff with tier-up, which cut
compile time 5–10×, and they keep no wasm code cache (Kenton Varda,
community.cloudflare.com/t/184668, 2020). workerd PR #7664 (merged
2026-10-20) lets an embedder keep V8 code caches, but only for JavaScript
modules. V8 now compiles wasm lazily, function by function on first call,
so a 2.23 MB module costs 4 ms to validate, and its compile cost moves into
the first session.

## 2. First session and setup [M]

Times in ms. "Setup" is codemode setup: mm's typed tool module (`define`),
or pi's prelude. For pi, the whole `start()` of a trivial script is
measured, because pi creates the VM, evaluates its prelude and runs the
script in one call.

| | mm default | mm core35 | mm no stdlib | pi (quickjs-wasi) |
|---|---:|---:|---:|---:|
| Node, first session after compile: session / setup / first cell | 2.70 / 14.6 / 3.00 | 2.68 / 16.5 / 3.45 | 2.41 / 14.8 / 3.18 | — |
| Node, first session, total | 20.6 | 23.0 | 20.6 | 30.3 |
| Node, first session, total, module compiled eagerly | 3.5 | 4.0 | 3.7 | 18.9 |
| Node, warm: session / setup / first cell | 0.64 / 0.68 / 0.14 | 0.67 / 0.75 / 0.15 | 0.49 / 0.69 / 0.13 | — |
| Node, warm, total | 1.53 | 1.73 | 1.35 | 6.62 |
| workerd, first request of a new isolate, total | 24 | 25 | 26 | 37 |
| workerd, the same after a session in the global scope | 3 | 2 | — | 14 |
| workerd, warm, total (mean of 20) | 3.25 | 2.9 | 2.85 | 7.9 |

The first session of an isolate pays for V8's lazy compilation of
everything it touches: about 15 ms on mm. The module's size hardly matters
here (the build without stdlib modules is no faster), because the session
touches the same functions in every build. A Worker can pay this inside its
startup limit instead of inside the first request's CPU time.
`hosts/js/bench/worker/worker.ts` shows how: built with `WARMUP`, it creates
a session in the global scope, defines the tools, runs
`import asyncio, json`, and drops the session.

workerd's clock ticks in whole milliseconds, so workerd times are means or
medians of whole-millisecond readings. mm's warm session costs about 2×
Node's in workerd (3.25 against 1.53 ms; pi's costs 7.9 against 6.6).
The likely cause is how workerd collects garbage with many wasm memories
alive [E].

## 3. Memory [M]

`mem N` creates 40 live sessions after one warm-up session. For mm, a live
session has its tools defined and has run a first cell. For pi, it is a VM
that ran its prelude and is blocked mid-await on a tool that never answers.
Linear memory is summed over every exported `WebAssembly.Memory`, so pi's
VMs are counted too.

| Per live session | mm default | mm core35 | mm no stdlib | pi (quickjs-wasi) |
|---|---:|---:|---:|---:|
| Linear memory (byteLength) | 1,572,864 | 1,441,792 | 1,245,184 | 1,638,400 |
| … of which shadow stack | 851,968 | 851,968 | 851,968 | 1,048,576 |
| JS heap (Node) | 170 KB | 170 KB | 166 KB | 268 KB |
| RSS (Node) | 853 KB | 752 KB | 502 KB | 876 KB |
| RSS (workerd process) | 319 KB | 294 KB | 210 KB | 376 KB |
| Linear memory, 41 sessions (workerd) | 64.5 MB | 59.1 MB | 51.1 MB | 67.2 MB |

| Session memory limit (Node) | mm: linear after a cell that allocates until it fails | pi: linear at "out of memory" |
|---|---|---|
| 32 MiB | 35.1 MB, `MemoryError`; the session runs the next cell, and memory stays 35.1 MB | 34.7 MB |
| 256 MiB (the default) | 269.5 MB, `MemoryError`; the same | 269.5 MB |

The 22 tasks peak at 1.57–3.28 MB of linear memory per session (the two
log tasks are the largest) [M].

Linear memory is reserved address space. RSS counts only the pages that
were touched, which is why RSS is below the linear size. Cloudflare counts
"the JavaScript heap and WebAssembly allocations" against the 128 MB [S].
We assume that means the instance's memory size, which is what V8 reports
as external memory [E]. Under that assumption, an idle mm session costs
about 1.74 MB of the isolate's 128 MB (linear plus heap), and pi's costs
1.9 MB. Wasm memory never shrinks. A disposed instance's memory goes back
only when V8 collects the instance. At the end of a workerd round (about 250
sessions created and dropped, plus 41 held and then released), the process
stood at 304 MB of RSS for mm and 212 MB for pi [M].

## 4. Host calls [M]

`await N`: N awaited calls of a no-op tool, against N calls of a local
async function. `fanout N`: N rounds of 100 calls through
`asyncio.gather`/`Promise.all`. mm goes through `src/driver.ts`. pi goes
through its own `start()`/`deliver()`. Times are µs per call.

| | mm, Node | pi, Node | mm, workerd | pi, workerd | mm-wasm, wasmtime | quickjs-wasi, wasmtime |
|---|---:|---:|---:|---:|---:|---:|
| Awaited call | 8.2 | 12.5 | 8.5 | 18.5 | 7.8 | 29.8 |
| In a fan-out of 100 | 6.1 | 11.7 | 5.0 | 16.5 | 6.8 | 29.9 |

The wasmtime columns come from js-engines-compare.md §4, which used pi's
protocol driven from Rust.

## 5. The 22-task codemode workload [M]

The 22 tasks are those of js-engines-compare.md §7: each engine replays the
same recorded tool results, mm runs the set-`s` Python solutions, and pi
runs the pi-style JS solutions. Each figure is the median ms of a task end
to end: a new session, setup and the whole script, after one warm-up. The
real checker (`bench/evidence/engines/check.py`) judges every run's calls,
and 145 of 145 pass for mm default, mm core35 and pi, in Node, in workerd,
and in the Durable Object resumes [M]. The build without stdlib modules
fails 17 of 22, on its imports.

| Task | mm, Node | mm core35, Node | pi, Node | mm, workerd | pi, workerd |
|---|---:|---:|---:|---:|---:|
| agg-avg-order-by-tier | 6.96 | 7.72 | 9.76 | 4 | 8 |
| agg-customer-summary | 2.12 | 1.97 | 6.68 | 2 | 7 |
| agg-revenue-by-region | 1.57 | 1.46 | 6.18 | 1 | 6 |
| agg-top-customers | 1.53 | 1.64 | 5.96 | 2 | 6 |
| logs-access-top-ips | 7.10 | 7.63 | 7.17 | 8 | 8 |
| logs-errors-per-service | 5.69 | 5.90 | 6.72 | 6 | 7 |
| neg-fanout-40 | 2.83 | 2.67 | 7.01 | 3 | 7 |
| page-accounts-offset | 2.26 | 2.17 | 6.12 | 2 | 7 |
| page-all-pending | 1.78 | 1.83 | 5.84 | 2 | 6 |
| page-feb-orders | 2.81 | 3.16 | 5.82 | 3 | 6 |
| page-open-bugs | 1.44 | 1.50 | 5.85 | 2 | 6 |
| par-two-level-fanout | 3.31 | 3.32 | 6.39 | 3 | 7 |
| par-weather-warmest | 1.67 | 1.70 | 5.77 | 2 | 7 |
| rep-quotes-csv | 1.61 | 1.72 | 5.76 | 2 | 7 |
| reshape-nested-counts | 3.02 | 3.19 | 5.87 | 4 | 7 |
| retry-backoff | 1.73 | 1.52 | 5.76 | 2 | 7 |
| retry-gather-flaky | 2.00 | 1.96 | 6.08 | 2 | 7 |
| retry-give-up | 1.70 | 1.68 | 6.09 | 2 | 7 |
| retry-permanent-errors | 1.69 | 1.81 | 5.75 | 2 | 7 |
| stat-max-drawdown | 2.29 | 2.28 | 6.90 | 2 | 7 |
| wf-close-fixed | 2.55 | 2.54 | 6.78 | 3 | 7 |
| wf-customer-latest-order | 2.00 | 2.04 | 6.27 | 2 | 6 |
| **sum** | **59.7** | **61.4** | **140.5** | **61** | **150** |
| median | 2.06 | 2.00 | 6.10 | 2.0 | 7.0 |

Under wasmtime, the same sums were 64.2 ms for mm-wasm and 346.0 ms for
quickjs-wasi (js-engines-compare.md §7). V8 runs quickjs-wasi more than 2×
faster than wasmtime did, and mm-wasm at about the same speed. In both
runtimes, mm's per-task cost is mostly the new instance and `define`,
1.1 ms of the 2.7 ms in Node. pi's per-task cost is mostly its prelude.

## 6. Snapshots and the Durable Object demo [M]

| | neg-fanout-40@0 | agg-avg-order-by-tier@3 | par-two-level-fanout@1 |
|---|---:|---:|---:|
| mm image, raw bytes | 73,039 | 149,468 | 174,656 |
| … zstd-1 / gzip | 18,101 / 18,636 | 28,809 / 30,249 | 41,828 / 42,997 |
| … snapshot / restore, Node, ms | 0.22 / 1.66 | 0.18 / 1.67 | 0.19 / 2.03 |
| … restore, workerd, ms | 1.5 | 2 | 2 |
| quickjs-wasi `QuickJS.snapshot` (whole linear memory), raw / zstd-1 / gzip | 1,638,428 / 209,510 / 211,456 | — | — |
| … snapshot / restore, Node, ms | 1.72 / 1.80 | — | — |

The mm images restore and finish with the remaining recorded replies, and
those runs pass the checker. Every image fits a SQLite-backed Durable
Object value (2 MB) as it is [S, M]. In KV-backed storage (128 KiB),
mm's two larger images need compression, which brings all three under
43 KB. quickjs-wasi's image does not fit even compressed. pi does not
snapshot quickjs-wasi: its durable code mode reruns the script and replays
the recorded calls (`packages/durable/src/codemode/codemode.ts`) [S].

**Hibernate and resume** (`/hib/start`, then `/hib/resume` in
`bench/worker/worker.ts`, class `Cells`; 3 rounds × 3 points):

1. A cell runs in the Durable Object until it waits on tools that have not
   answered: 40, 1 or 13 open calls.
2. The object snapshots the session, writes the image and the open calls to
   its SQLite storage (`ctx.storage.sql`), and drops the session.
3. The runner kills workerd (SIGKILL), which evicts everything, and starts
   a new process.
4. The next request reaches a new isolate (its id differs). It reads the
   row, restores the image, and `pending()` reports the wait. It answers
   the open calls and finishes the cell.

| Step | ms |
|---|---:|
| snapshot | 2–3 |
| SQLite write / read | 0–1 / 0–1 |
| workerd spawn to ready (new process) | 41–50 |
| restore in the new isolate | 5–11 |
| finish (cold isolate: includes lazy compilation) | 14–23 |

All 9 resumed cells printed the uninterrupted run's output and made the
same calls, and the checker passed them. Restore costs 5–11 ms here against
1.5–2 ms in a warm isolate, because the new isolate compiles lazily.

## 7. K sessions per instance [M]

`RuntimeConfig.sessionsPerInstance` (K, default 1) lets one instance hold
up to K sessions. The v2 interface always allowed it: every export and
import names its session. A new or restored session takes a free slot in a
live instance of its build, and a new instance starts only when none has
one. An instance is let go when its last session ends. `ksessions K 32`
creates 32 sessions on a runtime with that K, either bare or set up (tools
defined and a first cell run, as in §3), and reads the runtime's total
linear memory. Creation times are means over the 32, split by whether the
session started a new instance or took a slot in a live one.

| K | Instances for 32 sessions | Linear memory per session, bare | … set up | Create a set-up session, Node, ms: mean / new instance / free slot | … workerd |
|---|---:|---:|---:|---|---|
| 1 | 32 | 1,572,864 | 1,572,864 | 1.90 / 1.89 / — | 1.50 / 1.50 / — |
| 4 | 8 | 475,136 | 491,520 | 1.31 / 1.90 / 0.90 | 0.94 / 1.50 / 0.79 |
| 16 | 2 | 200,704 | 225,280 | 1.04 / 1.74 / 0.98 | 0.69 / 1.00 / 0.67 |

A bare session (no tools, no cell) is created in 0.74 ms in its own
instance and 0.17 ms in a free slot in Node (0.59 and 0.08–0.10 ms in
workerd). Node's RSS per set-up session is 898 KB at K = 1, 444 KB at
K = 4 and 264 KB at K = 16 [M].

What a shared instance saves is its fixed part: the 832 KiB stack, the
468 KB of data segments, and the allocator's slack. The guest makes one
`Engine` per session, so sessions in one instance share no decoded modules:
each bare session past the first adds about 109 KB, and each set-up one
about 135 KB (from the K = 16 totals). The review's figure for an extra
idle session in a shared instance (108 KiB, Node, 20 sessions) matches
the bare one. Creation is faster in a free slot because no instance starts: no
`WebAssembly.Instance`, no data segments copied, no first `memory.grow`.

What it costs, on Workers in particular:

- **A trap loses all K sessions.** A host callback that throws, an engine
  bug that traps, or a stack overflow ends the instance, and every session
  in it then throws `MmError` `trap`. Sessions in other instances go on (the
  K tests check both). With parked sessions in storage, a trap costs the
  K sessions' work since their last park.
- **Memory is pooled, and pinned at its peak.** The instance's memory must
  hold the sum of its sessions' bounds, and linear memory never shrinks. One
  session that fills a 16 MiB limit grows the shared instance by about
  18 MB, and that memory stays until every session of the instance has
  ended, not just the one that used it. With K = 1 it goes back at the next
  GC after the session is dropped. The worst case per session is about
  3 × limit + 16 MiB [S, sdk.md], so K sessions at 16 MiB are up to
  K × 64 MiB, against about K × 0.2 MB idle; under wasm32's 4 GiB that also
  caps K × limit.
- **One session at a time.** Sessions of an instance run one call at a
  time, as JavaScript does anyway, so this costs nothing in one isolate.

## 8. Time per fuel unit [M]

On deployed Workers the clock does not move while wasm runs, so a deadline
cannot stop a cell, and V8 has no epoch interrupt. Fuel is the only CPU
bound: the host runs a cell in slices of fuel and decides at every fuel
park whether to grant more. How much CPU a unit of fuel stands for decides
what a fuel budget means. `fuelRate` runs a program in 10,000-unit slices,
as the driver does, granting `fuelWanted()` when a native asks for more,
and times it. It counts the fuel two ways:

- **Charged:** what the engine charged, from the session's fuel total. This
  is the true count, the same as `mm exec --report-fuel` reports natively.
- **Granted:** what the slices gave, less what was left. A native that
  charges after it runs can take more than its slice in one step, and
  `fuel()` then stops at 0, so this misses the overdraft. A host that counts
  this way (the driver did, until this change) undercounts any loop of such
  natives.

**Bench programs and code-mode cells.** The 12 bench programs (`bench/`,
without `startup`) and the 22 tasks, µs per charged fuel unit:

| Program | Fuel charged | Native, µs/unit | Node | workerd | workerd, per granted unit | Longest slice, workerd, ms |
|---|---:|---:|---:|---:|---:|---:|
| classes | 1,250,096 | 0.051 | 0.086 | 0.106 | 0.106 | 3 |
| closure | 2,000,004 | 0.014 | 0.024 | 0.029 | 0.029 | 1 |
| dict | 994,561 | 0.086 | 0.140 | 0.151 | 0.181 | 15 |
| exceptions | 425,541 | 0.139 | 0.225 | 0.268 | 0.268 | 4 |
| fib | 635,623 | 0.026 | 0.041 | 0.044 | 0.044 | 1 |
| fmt | 469,413 | 0.246 | 0.378 | 0.447 | 0.656 | 19 |
| generators | 6,388,863 | 0.018 | 0.031 | 0.038 | 0.041 | 6 |
| interp | 2,117,392 | 0.062 | 0.099 | 0.117 | 0.117 | 3 |
| json | 1,427,744 | 0.049 | 0.088 | 0.094 | 0.609 | 41 |
| loop | 3,000,003 | 0.017 | 0.028 | 0.030 | 0.030 | 1 |
| sort | 3,510,225 | 0.032 | 0.049 | 0.050 | 0.063 | 44 |
| str | 911,295 | 0.035 | 0.064 | 0.066 | 0.176 | 14 |
| **22 code-mode tasks** (one cell each, through the driver) | 42,255 in all, 50–16,098 each | — | 0.86 | 0.85 | — | — |

The native column is the `mm` binary at the same commit (fuel from
`mm exec --report-fuel`, which agrees with the guest's count to within
0.01%); its times are wall time less an empty program's, on one core.

Bytecode costs 0.03–0.05 µs a unit in workerd (loops, calls,
generators); programs whose work is in natives (formatting, exceptions,
`dict` methods) cost more per unit, because those natives charge a unit
for many bytes or items. Node takes 1.5–1.8× native's time per unit, and
workerd 1.5–2.1×. The "per granted unit" column is what a host counting
granted fuel would have seen: up to 6.5× the true rate for `json`, whose
`dumps` and `loads` overdraw their slices. A
code-mode cell is different again: the 22 tasks charge only 50 to 16,100
units each, and their time is mostly compiling the cell, imports and host
calls, which charge little fuel. A cell's CPU is therefore a fixed part (up
to about 8 ms here) plus its fuel.

**Long native steps** (`FUEL_PROBES` in `bench/measure.ts`). Each site runs
at the largest size the session's memory limit admits (found by doubling,
then bisecting), on 10,000-unit slices; "longest step" is the longest time
between two fuel parks, which on Workers is the longest the isolate's event
loop is blocked. Times in ms, workerd (Node within about 20%).

| Site | Size at 16 MiB (`{n}`) | Charged | µs per charged unit | Longest step, 16 MiB | Overdraft, 16 MiB | Longest step, 256 MiB | Overdraft, 256 MiB |
|---|---:|---:|---:|---:|---:|---:|---:|
| `str.translate` | 1,375,000 | 4,189,454 | 0.032 | 132 | 4,179,454 | 2,088 | 67,021,251 |
| `str.upper` | 2,750,000 | 171,875 | 0.483 | 83 | 161,875 | 927 | 2,040,001 |
| `str.replace` | 2,050,000 | 8,520,312 | 0.007 | 58 | 8,510,312 | 583 | 91,427,501 |
| `str.count` | 8,200,000 | 256,251 | 0.203 | 52 | 246,251 | 202 | 1,015,001 |
| `str.find`, no match | 16,400,000 | 256,251 | 0.320 | 82 | 246,251 | 310 | 1,015,001 |
| `bytes.find`, no match | 8,200,000 | 128,171 | 0.374 | 48 | 118,171 | 388 | 1,015,046 |
| `int(str)`, invalid | 8,200,000 | 128,128 | 0.336 | 43 | 118,128 | 421 | 1,015,003 |
| `hashlib.sha256` | 8,000,000 | 500,039 | 0.070 | 35 | 490,020 | 295 | 4,090,020 |
| `bytes.hex` | 3,300,000 | 103,126 | 0.184 | 19 | 93,126 | 173 | 1,015,001 |
| `bytes.decode` | 2,750,000 | 171,875 | 0.087 | 15 | 161,875 | 210 | 2,040,001 |
| `json.loads` | 390,000 | 625,930 | 0.022 | 14 | 615,929 | 271 | 9,910,303 |
| `dict(zip(it, it))` | 260,000 | 364,448 | 0.096 | 19 | 216,253 | 290 | 3,640,813 |
| `set & set` | 155,000 | 198,688 | 0.045 | 9 | 188,688 | 221 | 3,496,728 |
| `dict(d)` | 130,000 | 226,253 | 0.062 | 14 | 216,253 | 204 | 3,640,813 |
| `max(list)` | 1,020,000 | 1,020,001 | 0.012 | 12 | 1,010,001 | 111 | 8,310,001 |
| `hash(tuple)` | 1,020,000 | 1,020,001 | 0.012 | 12 | 1,010,001 | 109 | 8,310,001 |
| `str(list)`, repr | 65,000 | 864,332 | 0.106 | 9 | 129,932 | 103 | 2,278,619 |
| `gc.collect()` | 260,000 | 3 | 1,667 | 5 | 0 | 85 | 0 |
| `sorted`, list of str (parks first) | 170,000 | 2,309,003 | 0.038 | 88 | 0 | 2,580 | 0 |
| `math.gcd` (parks first) | 60,000 | 6,927,425 | 0.164 | 1,134 | 0 | 1,074 | 0 |
| `divmod`, big ints (parks first) | 60,000 | 58,666,695 | 0.005 | 267 | 1,645 | 259 | 1,645 |
| `math.isqrt` (parks first) | 60,000 | 6,927,589 | 0.015 | 105 | 165 | 93 | 165 |
| `pow(b, e, m)` (parks first) | 1,500 | 18,347,489 | 0.038 | 691 | 0 | 661 | 0 |
| `re`, backtracking `(a\|a)*c` | 20 | 763,988 | 0.254 | 3 | 57 | 3 | 57 |
| `re.search`, no match | 6,000 | 1,126,724 | 0.075 | 1 | 392 | 1 | 392 |
| a loop building lists | 100,000 | 200,001 | 0.095 | 2 | 0 | 2 | 0 |

The parks-first rows run at fixed sizes, the same under both limits; their
"longest step" is the one slice the host granted after `fuelWanted()`
asked for it, which a budget would have refused. Node's steps are 0–25%
shorter than workerd's.

Three kinds of work escape a fuel budget:

- **Natives that charge after they run** overdraw the slice they started
  in, once. The step's length is set by the data, which the memory limit
  bounds, not by the fuel: at 16 MiB the longest is 132 ms
  (`str.translate`), and 40–90 ms is common for `str` and `bytes` methods.
  At the default 256 MiB they reach 0.1–2.1 s.
- **Natives charged cheaply.** Per charged unit, `str.upper` costs
  0.48 µs, `int(str)`, `bytes.find` and `str.find` 0.3–0.4 µs, against
  0.03–0.05 µs for bytecode: a unit is not the same time across natives
  (finding 3).
- **Garbage collection** is not charged: a collection over 260,000 live
  lists costs 5 ms at a 16 MiB limit, and 85 ms over 4.2M at 256 MiB.

The natives that park first with `fuelWanted()` (big ints, `sorted`) cost
0.005–0.16 µs per unit, so a budget that refuses them when the wanted fuel
is over what is left bounds them. `re` charges per backtracking step and
parks inside a match [M].

## What it means for code mode on Workers

**The constraint that binds first is memory.** The other limits leave
room:

- **Size:** 2.23 MB of 64 MiB, now that the compressed limits are gone.
- **Startup:** about 10 ms of the 1 s limit, or 40 ms with a warm-up
  session.
- **CPU:** a warm task takes 2 ms. Under the Free plan's 10 ms only the
  first session of a cold isolate is over, and the global-scope warm-up
  fixes that, for cells that are not hostile. Under the Paid plan's
  5 minutes, nothing comes close.

Memory is different. Every session is an instance whose linear memory
starts at 1.57 MB and only grows, and the isolate's 128 MB is shared with
concurrent requests and co-located Durable Objects. A cell that allocates
until it fails grows a session with the default 256 MiB limit to 270 MB
[M], twice the isolate's limit by itself. Cloudflare then replaces the
isolate, and every session in it is lost [S]. pi has the same shape:
1.64 MB per VM, and its Cloudflare runner defaults to a 32 MiB VM limit
[S].

Settings that follow [E, from the measurements above]:

- **Session memory limit: 16 MiB** (`sessionOptions({ limits: { memory: 16 << 20 } })`).
  The 22 tasks peak at 3.3 MB, and a 16 MiB session that fills its limit
  takes about 18.4 MB of linear memory [E, from 35.1 MB at 32 MiB; the
  first version measured 18.6 MB with the 1 MiB stack]. Use 32 MiB only for
  sessions known to need it.
- **Cell size:** refuse cells over 16 KiB in the host. Compiling is charged
  to the session since `8bdbaac` (512 bytes per source byte + 16 KiB, a
  `MemoryError` when it does not fit), so a 16 MiB session cannot compile
  much more than 32 KiB anyway; the host's check fails such cells before
  they reach the guest.
- **Sessions per isolate:** budget about 100 MB for sessions. That leaves
  15–25 MB for the runtime, the compiled code (4 MB at load, plus about
  12 MB of RSS once the first session ran [M]) and the Worker's JavaScript.
  The budget then holds up to 5 sessions running at the 16 MiB limit (2 at
  32 MiB), or about 55 idle ones at 1.74 MB (several hundred at K = 16).
  Admit at
  most 4 running cells per isolate. That count assumes the measured fill.
  The engine's bound for any guest input is larger: the Rust heap stays
  within 2.5 × limit + 16 MiB while a cell runs, and sdk.md tells wasm
  hosts to allow 3 × limit + 16 MiB of linear memory, which is 64 MiB at
  16 MiB and 40 MiB at 8 MiB [S, sdk.md]. The measured worst case is
  2.01 × the limit [S]. Admitting cells by that bound gives 1 running cell
  per isolate at 16 MiB, or 2 at 8 MiB. Choose between the two counts by
  how much an isolate reset costs: a reset loses only the sessions that are
  not parked. In a Durable Object design (one object per conversation),
  keep at most one live session per object. Co-located objects share the
  isolate.
- **K sessions per instance:** keep K = 1 when sessions are parked to
  storage between requests, which is the design below: there are then few
  live sessions per isolate, a trap costs one, and a session's peak goes
  back when it is dropped. Use K = 4 in a Worker that keeps many sessions
  live at once (a pool behind a router, say): idle memory falls from 1.57
  to 0.49 MB per session and creation from 1.9 to 1.3 ms, for 4 sessions
  per trap and a shared peak. K = 16 saves a further 0.27 MB per idle
  session, which matters only past about 200 live sessions per isolate.
- **Park, don't hold:** a session that waits on a slow tool or a person is
  snapshotted into its object's SQLite storage (73–175 KB, 2–3 ms) and
  dropped. Its memory then returns at the next GC. Resuming costs 1.5–2 ms
  warm, or 5–11 ms in a new isolate. Re-instantiating for every cell creates
  garbage: 304 MB of process RSS after about 250 sessions in workerd [M].
  K > 1 reuses an instance's slots, or `reset()` a conversation's session
  instead of creating a new one [E, not measured].
- **CPU:** run cells in 10,000-fuel slices (`fuelSlice`) and yield between
  them, so one cell cannot starve its isolate's other requests. A slice of
  bytecode is 0.3–0.5 ms in workerd. Set a per-cell `fuelBudget` from the
  plan's CPU limit at 0.5 µs a unit (the slowest program, `fmt`, is 0.45;
  the dearest native per charged unit, `str.upper`, 0.48), and leave room
  for one long step and the cell's fixed part: CPU per cell is at most
  about 8 ms + 0.5 µs × budget + one step (≤ 130 ms at 16 MiB) + uncharged
  GC (≤ 5 ms at 16 MiB) [E]. A budget of 1,000,000 units is then under
  0.7 s. The driver counts the charged fuel, so a loop of overdrawing
  natives stops after the step that crosses the budget. On Workers this is
  the only bound, because deadlines do not advance and no grace interrupt
  exists. Hitting Cloudflare's CPU limit resets the object instead of
  failing one cell [S, pi's `inline.ts`].
- **Free plan:** run one session in the global scope (the `WARMUP` bundle).
  Startup then costs about 80 ms instead of 45 ms, and the first request's
  session costs 3 ms instead of 24 ms. A hostile cell can still pass 10 ms
  of CPU in one native step: at 16 MiB a step reaches 130 ms, and keeping
  steps under 10 ms would need a limit near 1 MiB [E]. On the Free plan,
  expect such a cell to fail its request on Cloudflare's CPU limit.
- **Build:** the default module set. `core35` saves 8% of a session's
  memory and changes nothing else measurable. The build without stdlib
  modules cannot run code mode's `asyncio`.

Against pi on Workers, mm keeps the per-session lead that it had under
wasmtime. It is 2.5× faster end to end (workerd, 61 against 150 ms over 22
tasks). Host calls cost 2.2–3.3× less. At the same mid-await point its image
is 11× smaller than quickjs-wasi's whole-memory snapshot after compression
(18 KB against 210 KB). The memory per session is now a little lower
(1.57 MB against 1.64 MB): mm's 832 KiB stack against quickjs-wasi's
1 MiB, offset by 355 KB more data segments. Binary size costs mm 2.6 MB
more resident code per isolate than quickjs-wasi, about 10 ms of startup,
and 355 KB more data per session (shared by K sessions). None of that
comes near a limit.

## Findings for the engine and the interface (not changed here)

1. **The guest reserved Rust's default 1 MiB shadow stack in every
   instance.** That was 59% of an idle session's 1.69 MiB, and it is
   counted in the linear memory a Workers isolate pays for [M; the
   accounting is E]. *Fixed in `1958bac`:* measured on painted stacks, the
   deepest legal input (nested lambda defaults at the parser's nesting
   limit) takes 670 KB, and running code at most 41 KB. The guest now links
   832 KiB, so an idle session is 1,572,864 bytes instead of 1,769,472
   (-11%) [M, this campaign]. The rest of the old 1 MiB cannot go without
   making the parser recurse less per nesting level (about 3.4 KB per
   level).
2. **The interface needed no change.** v2 maps onto JavaScript directly:
   `i64` arrives as `BigInt`, and the output buffer is read in place. The
   optional `mm_pending` is what makes the Durable Object resume simple, and
   the session id every import carries is what makes K sessions per
   instance possible without a change. One trap for JS hosts, not an
   interface problem: the u64 results (`mm_fuel`, `mm_fuel_total`,
   `mm_fuel_wanted`, `mm_output`) arrive as signed `BigInt`s, so `u64::MAX`
   ("none") is `-1n`; read them with `BigInt.asUintN(64, …)`. `hosts/js` did
   not, which made `fuel()` and `fuelTotal()` report -1 instead of `null`
   until this change. One small gap: `SessionOptions` has no source limit
   (the engine's 256 KiB is fixed); since `8bdbaac` compiling is charged to
   the session, which makes a host-side cap a convenience rather than a
   memory bound.
3. **Fuel units are not the same time across natives, and some steps are
   long** (§8, workerd, at the largest size each limit admits; the step
   oracle in `crates/micro-monty/tests/steps.rs` covers native time on the
   native build):
   - Sites overdrawing a slice by more than steps.rs's bound for bounded
     rows (2^21 + 2^17 = 2,228,224 units) that steps.rs does not list, at
     256 MiB: `str.translate` 67.0M units in one 2.1 s step, `max(list)`
     and `hash(tuple)` 8.3M (110 ms), `hashlib.sha256` 4.1M (295 ms) and
     `dict(d)` 3.6M (204 ms). The listed open rows show the same:
     `str.replace` 91.4M (583 ms), `json.loads` 9.9M, `dict(zip)` 3.6M,
     `set & set` 3.5M, `str(xs)` 2.3M.
   - Sites inside the unit bound whose one step is long on V8, because a
     unit stands for 0.2–0.5 µs there: `str.upper` 2.0M units in 927 ms
     (one unit per 32 characters), `int(str)` of an invalid 65M-digit
     string 421 ms, `bytes.find` 388 ms, `str.find` 310 ms, `bytes.decode`
     210 ms, `str.count` 202 ms, `bytes.hex` 173 ms. sdk.md's "about one
     20 ms step [E]" holds for the cheap natives (0.007–0.03 µs a unit:
     `replace`, `translate`, `max`), not for these.
   - `gc.collect()` charges 3 units for 85 ms over 4.2M live lists.
   These are V8 times; native runs about 1.5–2× faster per unit (§8), so
   the long steps are roughly half as long natively [E].

## Method

- **Engines.** mm runs the shipped guest and the two variants on
  `hosts/js`. Its setup is the harness's typed tool module: keyword-only
  parameters defaulting to `None`, `dict_as_kwargs`, async tools answered
  as futures and then a batch. pi runs `InlineCodemodeSandbox` from
  `packages/codemode/src/portable.ts` at pi `7d073328`, which is what pi's
  Cloudflare entry runs, driven through `start()` and `deliver()`. Its
  `execute()` drops an error's `type`, which the harness's JS solutions
  read. quickjs-wasi is 3.6.2.
- **Node** (`bench/node.ts`): one command per process, with
  `--expose-gc`. Startup compiles synchronously, as workerd does when it
  loads a module.
- **workerd** (`bench/worker/`): esbuild bundles the same measurement code.
  `run.py` writes a config per engine, with the engine's wasm as the module
  `engine.wasm` (compiled by workerd at load, like Wrangler's
  `CompiledWasm`) and SQLite-backed Durable Object storage on disk. It
  starts one `workerd serve` per engine and round, and one each for the K
  and fuel measurements. Timings are taken inside the Worker. Memory is the
  process's RSS plus the Worker's own sum of linear memory.
- **K and fuel** (`bench/measure.ts`: `kSessions`, `fuelRate`,
  `fuelPrograms`, `fuelTasks`, `fuelWorst`): mm default only; the 256 MiB
  searches run in the first round only. Native rates
  (`bench/native_fuel.py`, `native-fuel.json`) are `mm exec --json
  --report-fuel` for the fuel and the median of 7 `mm run` wall times less
  an empty program's, on one core, outside the campaign.
- **Cores.** Every process is pinned to cores 2–5. With one core, as
  js-engines-compare.md pins, V8's background compilation and GC made warm
  sessions 2–4× slower and noisy in a trial run [M], for both engines.
- **Not measured:** deployed Cloudflare (CPU accounting, real memory
  accounting, multi-tenant placement) and the wasip1 guest.

## How to rerun

```bash
cd hosts/js
npm install && npm run guests && npm test
# pi at 7d073328 in /tmp/eng/pi, its quickjs-wasi linked to ours (hosts/js/README.md)
python3.14 -I bench/run.py --rounds 5        # about 30 minutes
python3.14 -I bench/run.py --rounds 5 --only k   # K sessions and fuel only
cargo build --release -p mm-cli && python3.14 -I bench/native_fuel.py native-fuel.json
```
