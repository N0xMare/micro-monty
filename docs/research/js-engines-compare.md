# micro-monty against the codemode JavaScript engines

Commit hashes of micro-monty cited here, other than the public history's
(root `2bcbb8c`), are pre-release history, kept in the private archive
`N0xMare/micro-monty-archive` (branch `history-0.0.1`;
[docs/history](../history/README.md) maps them); the numbers they qualify
are unchanged.

How does micro-monty compare with the JavaScript engines that do the same
code-mode job? The comparison covers pi's codemode on quickjs-wasi, plak's JS
host on rquickjs, djs (native and wasm), dquickts, and the `qjs` CLIs. Before
it, only raw interpreter speed had been compared with `qjs` (bench/README.md);
memory, host calls, start-up, control and durability had only been compared
with Monty. Every engine runs behind a small Rust host with the same CLI
(`bench/evidence/engines/hosts`). The JS engines run pi's unmodified prelude;
mm runs the equivalent typed tool module. All engines answer from the same
recorded tool results.

Numbers are labelled [M] measured, [S] sourced or [E] estimated. Unless a line
says otherwise, they are [M] medians over 5 rounds on one 16-vCPU Amp orb
(Intel Xeon @ 2.60 GHz, Linux 6.1, Debian 13), each process pinned with
`taskset -c 2`, with the engine order rotated every round. mm is at
`eb78933`, the 0.0.1 release candidate, measured on 2026-11-11. The first run
(mm at `5df122e`, 2026-11-09) is kept in `results/2026-11-09/`, and
[Since 5df122e](#since-5df122e) compares the two. Raw data, harness and full
tables: [`bench/evidence/engines/`](../../bench/evidence/engines/)
(`results/2026-11-11-eb78933/summary.md`, `raw.jsonl`). One command reruns
everything: `python3.14 bench/evidence/engines/run.py`.

## Summary

| [M] | mm native | mm-wasm | rquickjs (plak) | dquickts | quickjs-wasi (pi) | djs | djs-wasm |
|---|---:|---:|---:|---:|---:|---:|---:|
| Minimal one-script host, bytes | 2,409,456 | 2,203,338 | 1,281,232 | — | 637,405 | 1,600,432 | 929,693 |
| RSS per session after codemode setup (40 live) | **132 KB** | 459 KB | 436 KB | 461 KB | 620 KB | 764 KB | 1,377 KB |
| New session + codemode setup | **0.10 ms** | 0.58 ms | 4.1 ms | 3.3 ms | 9.7 ms | 5.1 ms | 11.3 ms |
| Session from a saved image | 0.15 ms | 0.48 ms | none | 0.35 ms | 0.67 ms | **0.036 ms** | not measured |
| Awaited host call, per call | 2.2 µs | 4.8 µs | 3.0 µs | **2.0 µs** | 25.1 µs | 4.6 µs | — |
| Fan-out of 100, per call | **1.9 µs** | 3.6 µs | 3.0 µs | 2.1 µs | 25.7 µs | 4.9 µs | — |
| Sync host call | 0.32 µs | 0.80 µs | 0.05 µs | **0.04 µs** | 1.62 µs | 0.13 µs | — |
| 22 codemode tasks, end to end (sum) | **12.5 ms** | 38.4 ms | 89.4 ms | 73.6 ms | 210.3 ms | 116.6 ms | — |
| …script run only (sum) | 10.2 ms | 24.7 ms | 7.2 ms | **6.4 ms** | 18.9 ms | 10.5 ms | — |
| 12 bench programs, time vs mm (geomean) | 1.00 | 1.99 | 1.66¹ | **0.85** | 3.35 | 1.78 | 3.58 |
| Stop a busy loop | **0.04 ms, parks** | 0.05 ms, parks | 0.08 ms, kills run | 0.13 ms, kills run | 0.23 ms, kills run | **0.04 ms, pauses** | — |
| Stop during one 1M-string sort | **0.70 ms, parks** | 3.0 ms, parks | 320 ms, kills run | 251 ms, kills run | 107 ms, **instance lost** | 226 ms, pauses | — |
| Stop during repeated long natives | **0.37 ms (join), 0.53 ms (sort)** | 1.1 ms (join), 1.3 ms (sort) | **never (10 s)** | **never (10 s)** | instance lost | 220 ms (sort); **never** (join) | — |
| Longest GC pause a host sees, 2M live pairs | **2.9 ms** (park request)² | not measured | 230 ms | 211 ms | not measured | 189 ms | not measured |
| Snapshot mid-await, zstd-1 | **16 KB** | 18 KB | none | 29 KB | 209 KB | 104 KB, same build only | — |
| Restore mid-await | **0.08 ms** | 0.36 ms | — | 0.41 ms | 0.64 ms | 0.24 ms | — |
| Language | Rust, no `unsafe` | same, in wasm | C | C | C in wasm | Rust; wasm build `unsafe-heap` | same |

¹ Built with gcc 14.2. The same host built with gcc 12 runs the bench 1.27×
faster [M], which puts rquickjs at about 1.31× mm [E] (§7). The other engines
move by 4% or less with the compiler.
² Between 10k-fuel parks, the way `drive_async` runs a cell, mm's longest gap is
42 ms (§8).

For code mode:

- **mm wins on per-session costs**: memory, setup, a whole task end to
  end, host calls and snapshots. pi's prelude costs the JS engines 3.2–11 ms
  per session; mm's typed tool module costs 53 µs. So a typical task takes
  0.44 ms on mm against 3.3–9.3 ms on the JS engines (median).
- **mm now also wins on control.** Since 5df122e, natives run in bounded
  steps (spec §5.1), so mm parks within 1 ms inside a 1M-string sort or a
  65 MB join, natively and in wasm, and the run resumes. No JS engine both
  stops inside a long native and resumes: QuickJS never stops inside one and
  ends the run, djs takes 220 ms or never returns, and quickjs-wasi loses the
  instance to the 100 ms grace trap. With park requests mm's longest GC pause
  is 2.9 ms at 2M live pairs, where the JS engines pause 189–230 ms.
- **mm loses on binary size**: native is 1.9× rquickjs, and wasm is 3.5×
  quickjs-wasi. It also loses on raw interpretation to Bellard's QuickJS:
  dquickts is 1.18× faster on the bench geomean and 1.4× faster on the
  script part of the tasks (geomean).
- **mm-wasm against pi's own engine (quickjs-wasi)**: mm-wasm wins on every
  dimension except size. It sets up a session 17× faster, makes host calls 5×
  faster, runs the benches 1.7× faster, uses 1.35× less memory per session,
  writes snapshots 11× smaller, and parks inside long natives where
  quickjs-wasi loses its instance.

## Since 5df122e

The JavaScript engines are at the same pins in both runs, so their movement is
the environment's: a different orb, about 1.9–2.1× faster on the bench and 1.4–1.6×
on session setup, and Debian 13's gcc 14.2 instead of gcc 12.2 for the C
engines [M]. Absolute times are therefore not comparable across the runs; the
ratios against the unchanged engines are.

| mm figure [M] | 5df122e (2026-11-09) | eb78933 (2026-11-11) | Unchanged engines, same figure |
|---|---:|---:|---|
| `hello`, native bytes | 2,275,864 | 2,409,456 (+5.9%) | djs size host 1,600,440 → 1,600,432 |
| `hello.wasm` bytes | 2,092,164 | 2,203,338 (+5.3%) | djs-wasm 929,693 → 929,693 |
| RSS per set-up session (40 live) | 131 KB | 132 KB | rquickjs 441 → 436, djs 764 → 764 KB |
| New session + codemode setup | 0.15 ms | 0.10 ms (×0.64) | ×0.61–0.71 |
| Session from the template | 0.23 ms | 0.15 ms (×0.63) | dquickts ×0.67, djs ×0.48 |
| Awaited host call | 3.3 µs | 2.2 µs (×0.65) | ×0.52–0.84 |
| 22 tasks end to end (sum) | 23.5 ms | 12.5 ms (×0.53) | ×0.60–0.64 |
| Script run, engine / mm (geomean) | dquickts 0.67, rquickjs 0.72, djs 1.09, quickjs-wasi 1.80 | 0.71, 0.79, 1.15, 2.05 | |
| Bench, engine / mm (geomean) | dquickts 0.78, rquickjs 1.16, djs 1.79, quickjs-wasi 3.07 | 0.85, 1.66 (≈1.31 on gcc 12 [E]), 1.78, 3.35 | |
| `sorted` of 1M strings, `mm run` | 2.7 s (CPython 0.36 s, qjs-ng 0.78 s) | 0.57 s (CPython 0.17 s, qjs-ng 0.39 s) | |
| Stop during one 1M-string sort | 2,659 ms, parks after it | 0.70 ms, parks inside | |
| mm-wasm, stop during a sort / a 65 MB join | trap, instance lost | 3.0 / 0.34 ms, parks | |
| Snapshot neg-fanout-40@0, zstd-1 / restore | 16,062 B / 120 µs | 16,249 B / 77 µs | dquickts restore ×0.65, djs ×0.56 |

**What moved.** Per-session costs moved with the machine, so they did not
really change. A whole task got about 12–17% cheaper relative to the JS
engines, almost all of it in the script run. On the bench, mm held level
against djs, the only engine that is pure Rust and so untouched by the
compiler change (1.79 → 1.78). It gained 8–9% against the wasm engines and
about 9% against dquickts. Two real changes stand out. Control: mm now parks
inside long natives, natively and on wasm, where the first run waited out a
2.7 s sort and lost wasm instances. And `sorted` on strings is 2.2× faster
relative to CPython. Size grew 5–6%.

## 1. Size [M]

A minimal host that runs one script, built like `cargo xtask size`: release,
fat LTO, stripped, std rebuilt with immediate abort, and `wasm-opt -Oz` for
wasm. mm's builds include its parser, compiler, prelude and snapshot code;
`hello` takes and restores a snapshot. C code is built with gcc 14.2.

| Engine | Target | Bytes | gzip -9 | Build |
|---|---|---:|---:|---|
| micro-monty `hello` | native | 2,409,456 | 1,409,971 | `cargo xtask size` |
| rquickjs 0.14 | native | 1,281,232 | 607,705 | `size/rquickjs`, same flags |
| djs | native | 1,600,432 | 775,381 | `size/djs`, same flags |
| quickjs-ng `qjs` CLI | native | 1,334,808 | 644,081 | cmake Release + IPO, stripped (C) |
| QuickJS `qjs` CLI | native | 1,137,952 | 530,717 | `make CONFIG_LTO=y`, stripped (C) |
| djs CLI | native | 5,319,648 | 2,208,145 | upstream release profile, stripped |
| micro-monty `hello.wasm` | wasm32-unknown-unknown | 2,203,338 | 1,083,582 | `cargo xtask size` |
| mm-wasm v1 guest | wasm32-unknown-unknown | 2,289,389 | 1,117,377 | `cargo xtask size` |
| quickjs-wasi 3.6.2 (pi) | wasm32-wasip1 | 637,405 | 286,642 | npm module, upstream `-Oz -flto` + `wasm-opt -Oz` |
| djs | wasm32-unknown-unknown | 929,693 | 421,721 | `scripts/build-wasm.sh`, then `wasm-opt -Oz` |

**What it means.** On size, mm loses to every JS engine: 1.5–1.9× native and
2.4–3.5× wasm. Most of mm's bytes are the Python standard library it bundles
(regex, datetime, decimal, json, unicodedata tables and about 50 Python
modules compiled in); see [size-map.md](size-map.md). A JS engine gets
`JSON`, `RegExp`, `Date` and `Intl`-free strings in its core, and code mode
needs nothing more. A codemode-only feature set (no `decimal`, `zoneinfo`,
`difflib`…) is the lever if wasm download size matters; native size rarely
matters for plak. The C builds grew 4.5–7.5% with gcc 14 (`size/rquickjs`
1,192,376 → 1,281,232 bytes from the same source), so the native ratio
against rquickjs flatters mm slightly against the first run's 1.9×.

Not built this way: quickjs-wasi (taken from npm: wasi-sdk is not on the orb,
and re-running `wasm-opt -Oz` saves 27 bytes), the C `qjs` CLIs (their own
builds), dquickts's `qjs` (failed to compile with gcc 12 at `quickjs.h:264`;
not retried), and rquickjs for wasm (needs a wasi-sdk C toolchain). See the
evidence README.

## 2. Memory [M]

`mem empty|setup N` creates N live sessions after one warm-up session. It
reports the RSS added (`/proc/self/statm`), the malloc heap added (`mallinfo2`)
and the engine's own count (QuickJS `malloc_size`, djs arena bytes, wasm
linear memory). "setup" means pi's prelude for the JS engines, or mm's typed
tool module (`neg-fanout-40`'s tools), plus a first cell. Each JS session is
one runtime and one context, as pi uses one VM per script. Values are KB added:
RSS / malloc heap / engine count.

| Engine | empty 1 | empty 8 | empty 40 | setup 1 | setup 8 | setup 40 |
|---|---:|---:|---:|---:|---:|---:|
| mm-native | 0 / 109 / 0 | 1,339 / 897 / 0 | 3,625 / 4,585 / 0 | 0 / 122 / 0 | 2,286 / 1,007 / 0 | 5,288 / 5,085 / 0 |
| mm-wasm | 0 / 32 / 1,507 | 1,946 / 273 / 12,059 | 12,218 / 1,442 / 60,293 | 0 / 33 / 1,507 | 3,367 / 278 / 12,059 | 18,342 / 1,454 / 60,293 |
| rquickjs | 0 / 161 / 107 | 1,819 / 1,339 / 859 | 7,311 / 6,736 / 4,294 | 139 / 438 / 363 | 3,113 / 3,558 / 2,903 | 17,457 / 17,783 / 14,515 |
| dquickts | 0 / 163 / 165 | 1,880 / 1,323 / 1,322 | 7,188 / 6,647 / 6,607 | 0 / 467 / 467 | 3,359 / 3,760 / 3,740 | 18,440 / 18,769 / 18,698 |
| quickjs-wasi | 0 / 42 / 1,376 | 2,437 / 353 / 11,010 | 12,743 / 1,792 / 55,050 | 369 / 45 / 1,638 | 4,899 / 357 / 13,107 | 24,797 / 1,797 / 65,536 |
| djs | 0 / 263 / 200 | 1,618 / 2,100 / 1,597 | 8,647 / 10,508 / 7,985 | 807 / 805 / 738 | 6,214 / 6,380 / 5,904 | 30,544 / 31,635 / 29,522 |
| djs-wasm | 0 / 10 / 590 | 2,273 / 94 / 4,719 | 12,464 / 506 / 23,593 | 1,159 / 11 / 1,573 | 10,953 / 95 / 12,583 | 55,067 / 507 / 62,915 |

Per session at 40 [M]: mm 91 KB empty and 132 KB set up; rquickjs 183 and 436;
dquickts 180 and 461; djs 216 and 764; quickjs-wasi 319 and 620; mm-wasm 305
and 459; djs-wasm 312 and 1,377. mm's facade has no per-session count (`0`).
For wasm engines, linear memory is reserved address space and RSS is what was
touched.

**What it means.** mm needs 3.3× less memory per set-up session than the best
JS engine, so 40 sessions take 5.3 MB of RSS in total against 17.5 MB on
rquickjs. Most of the JS cost is pi's prelude itself: about 250–280 KB per
QuickJS session and 550 KB per djs session. mm shares decoded modules
across sessions through its engine-wide module cache. Under wasm each session
is its own instance, so mm-wasm is 3.5× native but still below quickjs-wasi.

## 3. Start-up [M]

The times are in µs. "New runtime" means `Engine::new` for mm; for wasm
engines it is the module load, precompiled with wasmtime. "Codemode setup"
is pi's prelude, or mm's tool module. "From image" means a session from an
image saved after setup: mm's session template, dquickts's
`JS_WriteSnapshot`, djs's `Vm::image` and quickjs-wasi's memory snapshot.

| Engine | New runtime | New session | Codemode setup | First cell | From image | First cell after image |
|---|---:|---:|---:|---:|---:|---:|
| mm-native | 22.2 | 44.1 | 52.7 | 23.3 | 146.3 | 36.9 |
| mm-wasm | 2710.9 | 143.0 | 436.2 | 50.2 | 480.3 | 74.7 |
| rquickjs | 0.1 | 116.9 | 3982.3 | 26.3 | — | — |
| dquickts | 0.1 | 106.4 | 3203.2 | 23.0 | 349.6 | 19.9 |
| quickjs-wasi | 3127.8 | 360.6 | 9339.9 | 77.5 | 673.0 | 90.6 |
| djs | 0.0 | 15.1 | 5130.3 | 41.7 | 36.0 | 24.9 |
| djs-wasm | 3791.4 | 140.2 | 11140.5 | 22.6 | — | — |

**What it means.** Without images, mm reaches its first cell in 0.12 ms; the
JS engines take 3.3–11 ms, almost all of it compiling and running pi's
prelude. With images the gap closes. djs is fastest, at 36 µs plus a 25 µs
first cell, 3.0× faster than mm's template (146 + 37 µs). djs's base image is
one arena memcpy, while mm's template instantiation rebuilds session tables. If
plak starts sessions from templates, mm's template path is the number to
improve. The wasm runtime costs 2.7–3.8 ms once per process, about the same
for every wasm engine.

## 4. Host calls [M]

`hostcall KIND N` runs N tool calls and the same N calls to a local no-op
function. Each run uses a new session with one mocked tool, and the reported
cost is the difference divided by N. Sync calls are `hsync(i)`, a plain host
function. Awaited calls are `await tools.noop({i})`, or `await noop(i=i)` for
mm. Fan-out runs 100 rounds of `Promise.all`/`asyncio.gather` over 100 calls.
For the JS engines this is pi's protocol as pi pays it: the prelude's JSON
`bridge`, `settle`, draining the job queue, then `stalled()`. mm answers each
call with `Resume::Future` and then a `Batch`. Times in µs per call.

| Engine | Sync | Sync total | Awaited | Awaited total | Fan-out | Fan-out total |
|---|---:|---:|---:|---:|---:|---:|
| mm-native | 0.32 | 0.36 | 2.16 | 2.43 | 1.89 | 2.32 |
| mm-wasm | 0.80 | 0.87 | 4.79 | 5.23 | 3.64 | 4.42 |
| rquickjs | 0.05 | 0.12 | 2.96 | 3.90 | 3.05 | 4.35 |
| dquickts | 0.04 | 0.06 | 2.02 | 2.76 | 2.07 | 3.06 |
| quickjs-wasi | 1.62 | 1.74 | 25.10 | 27.06 | 25.67 | 29.03 |
| djs | 0.13 | 0.17 | 4.60 | 5.31 | 4.90 | 7.06 |

**What it means.** Awaited and fan-out calls, which are how code mode calls
tools, cost about the same on mm as on the best JS engine (dquickts). They
are 1.4–1.6× cheaper than on rquickjs and 12–14× cheaper than on pi's
quickjs-wasi, where every call crosses the wasm boundary twice with JSON. A
sync call costs mm 6× more than rquickjs, because it leaves the dispatch loop
and comes back through `resume`. That loss is real but stays near a third of
a microsecond, and code-mode tools are async. Every engine's per-call cost is
far below a real tool's latency.

## 5. Control [M]

`control KIND` sends a stop from another thread a fixed time after the
script's `hsync` mark: 50 ms for `loop`, 10 ms for `string`, 100 ms
otherwise. It reports the time until the host regains control, whether the
run can be resumed, and whether the engine instance still works. The stop is
mm's park handle, QuickJS's interrupt handler, djs's flag checked between
10k-fuel slices, and quickjs-wasi's `host_interrupt` import. The two wasm
engines also have mm-wasm-host's default 100 ms epoch grace, after which the
instance traps. `loop` is `while (true) n++`. `sort` is one sort of 1M
strings, then a busy loop, and `string` one `join` of 1M 64-char strings, then
a busy loop. `sort-loop` and `string-loop` repeat that operation forever.
There were 5 runs each, and a run that did not regain control within 10 s is
reported as "never".

| Engine | loop | sort | string | sort-loop | string-loop |
|---|---|---|---|---|---|
| mm-native | 0.04 ms, parked | 0.70 ms, parked | 0.29 ms, parked | 0.53 ms, parked | 0.37 ms, parked |
| mm-wasm | 0.05 ms, parked | 3.00 ms, parked | 0.34 ms, parked | 1.27 ms, parked | 1.09 ms, parked |
| rquickjs | 0.08 ms, interrupted | 320 ms, interrupted | 35 ms, interrupted | never | never |
| dquickts | 0.13 ms, interrupted | 251 ms, interrupted | 31 ms, interrupted | never | never |
| quickjs-wasi | 0.23 ms, interrupted | 107 ms, trap, instance lost | 107 ms, trap, instance lost | 105 ms, trap, instance lost | 107 ms, trap, instance lost |
| djs | 0.04 ms, paused at a slice | 226 ms, paused | 40 ms, paused | 220 ms, paused | never |

"Parked" and "paused" runs could be resumed and the session kept working.
"Interrupted" runs ended with an uncatchable error, but the runtime kept
working. After a trap the instance was lost.

Cost of polling [M]: each bench program ran with and without fuel slices or
an armed interrupt handler (`polling`). The differences were within ±5% on
`loop`, `fib` and `sort`, which is run-to-run noise, except djs on `fib`
(+8%). Arming the stop is nothing measurable on any engine; the cost lies in
how often each engine checks.

Why each engine behaves as it does [S, from source]:

- **mm** parks at any bytecode boundary, and since 5df122e its natives run in
  bounded steps: a sort, a join, a big `%` format, json and the container
  operations do their work a slice per step with a park check between
  (commits `cf9c214`, `8663d7c`, `5ba19df`; spec §5.1, a 50 ms step bound
  checked by `crates/micro-monty/tests/steps.rs`). So it parks inside one
  sort within 0.7 ms natively and 3 ms on wasm, well inside the 100 ms grace,
  and the run resumes where it stopped. The first run waited 2.7 s for the
  sort to end, and lost the wasm instance to the grace trap.
- **QuickJS** (rquickjs, dquickts) calls the interrupt handler every
  10,000 polls (`JS_INTERRUPT_COUNTER_INIT`). A poll happens at a call or a
  backward jump, never inside a native `sort` or `join`, so a stop during a
  sort waits for its end (251–320 ms here). In `sort-loop` an iteration
  makes only a few polls around a 250–320 ms sort, so a stop arrives after
  thousands of sorts, many minutes [E].
  The stop is not lost, but in practice it never arrives, and an interrupt
  always ends the run.
- **djs** runs builtins such as `sort` as fuel-charged state machines, but a
  stop 100 ms into this sort took 209–243 ms in all five runs here, where the
  first run saw 6–641 ms (median 30). The stop seems to land in a different
  phase of the sort on the faster orb, one that does not yield for about
  220 ms; we did not establish which. It charges `join` afterwards. With
  `string-loop` its slices took longer each round (1.3 → 1.7 → 3.6 → 6.9 →
  13 ms… in the first run's trace) and control never came back. djs's own CLI
  shows the same slowdown: ten 65 MB joins take 0.71 s unsliced and 13.7 s
  with `--suspend-every 10000`. That is an upstream djs performance issue
  under slicing; we did not establish its cause.

**What it means.** mm is the only engine that stops inside every one of
these within a few milliseconds and keeps a resumable session, on both
targets. The first run's remedies (a faster string comparison in
`sort_values`, chunked sorts) both landed: `sorted` of 1M strings now takes
0.57 s in `mm run`, against 0.17 s on CPython and 0.39 s on qjs-ng [M], and a
park no longer waits for it.

## 6. Durability [M]

`snapshot TASK SCRIPT N` runs a task until N tool results are delivered and
the script waits on more, then takes a snapshot. It restores the snapshot 10
times, and finishes the first restore with the remaining recorded replies,
which must make the same calls and succeed. The three points are `neg-fanout-40`
before any answer (40 calls in flight), `agg-avg-order-by-tier` after 3
answers, and `par-two-level-fanout` after 1 answer. Every restore finished its
task.

| Engine | Point | Kind | Raw bytes | zstd-1 bytes | Snapshot µs | Restore µs |
|---|---|---|---:|---:|---:|---:|
| mm-native | neg-fanout-40@0 | full image, mid-await | 69,535 | 16,249 | 56 | 77 |
| mm-native | agg-avg-order-by-tier@3 | | 144,586 | 26,655 | 117 | 139 |
| mm-native | par-two-level-fanout@1 | | 170,197 | 39,699 | 144 | 209 |
| mm-wasm | neg-fanout-40@0 | full image, mid-await | 73,239 | 18,216 | 224 | 359 |
| mm-wasm | agg-avg-order-by-tier@3 | | 149,652 | 29,008 | 374 | 564 |
| mm-wasm | par-two-level-fanout@1 | | 174,944 | 41,955 | 432 | 1,187 |
| dquickts | neg-fanout-40@0 | `JS_WriteSnapshot`, mid-await | 68,406 | 29,115 | 346 | 405 |
| dquickts | agg-avg-order-by-tier@3 | | 68,799 | 30,934 | 338 | 382 |
| dquickts | par-two-level-fanout@1 | | 63,681 | 28,494 | 331 | 340 |
| quickjs-wasi | neg-fanout-40@0 | whole linear memory | 1,638,440 | 209,131 | 61 | 638 |
| quickjs-wasi | agg-avg-order-by-tier@3 | | 1,638,440 | 208,677 | 429 | 614 |
| quickjs-wasi | par-two-level-fanout@1 | | 1,638,440 | 205,717 | 425 | 591 |
| djs | neg-fanout-40@0 | `Vm::image`, raw arena | 397,856 | 103,896 | 11 | 237 |
| djs | agg-avg-order-by-tier@3 | | 398,184 | 107,543 | 9 | 228 |
| djs | par-two-level-fanout@1 | | 761,784 | 142,724 | 20 | 602 |

rquickjs (quickjs-ng) and the djs-wasm host have no snapshots. pi does not
snapshot quickjs-wasi in code mode. Its durable code mode reruns the script
and replays the recorded calls deterministically
(`packages/durable/src/codemode/codemode.ts`) [S].
djs's raw image is "the same-build fast path" (`docs/portable-snapshots.md`
§3.8) [S]. Its durable form is a continuation dump, which needs pi's "next"
prelude and was not measured here.

**What it means.** mm's mid-await image is the smallest after compression
(1.8× smaller than dquickts, 6.4× smaller than djs, 13× smaller than
quickjs-wasi). Its restore is the fastest, and it is the only one of these
formats meant to outlive a process, crossing native and wasm and checked by
`cargo xtask durability`. dquickts's raw bytes are about the same and stay
flat as the session heap grows (mm's grow with it), but they compress worse
and take 5× longer to restore. Taking a djs image is a memcpy, the cheapest
snapshot, but it only restores on the same build.

## 7. Codemode workload [M]

The workload is 22 tasks from `conformance/codemode/tasks`: aggregate,
paginate, batch/fan-out, retry, filter and join. Their tool calls and replies
were recorded once on CPython with the real mocked tools (`record.py`), and
every engine replays them, so every engine sees the same tool results and no
engine pays for a Python mock. mm runs the existing set-`s` Python solutions.
The JS engines run pi-style solutions written for this comparison
(`js/<task>.js`, `await tools.x({...})`). All 132 engine × task runs pass the
real checker (`check.py`, which reads each engine's call log). Each figure is
the median ms per task: a new session, setup and the whole script, with the
script run alone in parentheses. djs-wasm cannot run the protocol (see the
evidence README), so native djs stands for it.

| Task | mm-native | mm-wasm | rquickjs | dquickts | quickjs-wasi | djs |
|---|---:|---:|---:|---:|---:|---:|
| agg-avg-order-by-tier | 1.14 (1.01) | 2.67 (1.96) | 4.47 (0.77) | 3.80 (0.67) | 10.62 (2.03) | 5.51 (0.94) |
| agg-customer-summary | 0.31 (0.20) | 1.03 (0.39) | 3.75 (0.17) | 3.20 (0.17) | 8.74 (0.39) | 4.92 (0.24) |
| agg-revenue-by-region | 0.17 (0.08) | 0.76 (0.18) | 4.02 (0.10) | 3.04 (0.09) | 8.86 (0.24) | 5.25 (0.13) |
| agg-top-customers | 0.19 (0.10) | 0.79 (0.20) | 3.98 (0.12) | 3.20 (0.11) | 10.00 (0.30) | 4.88 (0.14) |
| logs-access-top-ips | 1.95 (1.85) | 5.83 (5.20) | 5.20 (1.14) | 4.38 (0.97) | 12.32 (2.62) | 6.46 (1.30) |
| logs-errors-per-service | 1.55 (1.41) | 4.08 (3.38) | 4.52 (0.70) | 3.58 (0.58) | 10.45 (1.73) | 5.75 (0.96) |
| neg-fanout-40 | 0.53 (0.43) | 1.65 (1.01) | 4.76 (0.54) | 3.41 (0.42) | 10.60 (1.74) | 5.55 (0.79) |
| page-accounts-offset | 0.45 (0.35) | 1.55 (0.91) | 3.96 (0.28) | 3.66 (0.27) | 9.23 (0.68) | 5.73 (0.55) |
| page-all-pending | 0.33 (0.23) | 1.14 (0.53) | 3.91 (0.18) | 3.16 (0.17) | 9.38 (0.47) | 4.83 (0.23) |
| page-feb-orders | 0.58 (0.49) | 2.21 (1.62) | 3.85 (0.21) | 3.22 (0.17) | 9.29 (0.49) | 5.52 (0.28) |
| page-open-bugs | 0.22 (0.13) | 0.82 (0.26) | 3.78 (0.12) | 3.17 (0.11) | 8.98 (0.27) | 4.79 (0.16) |
| par-two-level-fanout | 0.72 (0.61) | 2.41 (1.72) | 4.00 (0.34) | 3.36 (0.32) | 10.19 (1.13) | 5.64 (0.71) |
| par-weather-warmest | 0.31 (0.21) | 1.33 (0.69) | 3.99 (0.18) | 3.13 (0.15) | 9.02 (0.46) | 4.88 (0.22) |
| rep-quotes-csv | 0.27 (0.18) | 0.96 (0.39) | 3.82 (0.16) | 3.18 (0.16) | 8.96 (0.47) | 4.87 (0.24) |
| reshape-nested-counts | 0.78 (0.67) | 2.36 (1.79) | 3.84 (0.29) | 3.31 (0.27) | 9.36 (0.66) | 5.68 (0.58) |
| retry-backoff | 0.23 (0.14) | 0.84 (0.28) | 3.77 (0.12) | 3.00 (0.10) | 8.67 (0.32) | 4.84 (0.18) |
| retry-gather-flaky | 0.44 (0.33) | 1.16 (0.60) | 3.94 (0.33) | 3.31 (0.33) | 9.63 (0.92) | 5.72 (0.67) |
| retry-give-up | 0.38 (0.29) | 1.07 (0.51) | 3.74 (0.22) | 3.13 (0.21) | 8.90 (0.65) | 4.95 (0.33) |
| retry-permanent-errors | 0.32 (0.22) | 0.98 (0.41) | 3.76 (0.18) | 3.16 (0.17) | 9.06 (0.47) | 4.92 (0.26) |
| stat-max-drawdown | 0.54 (0.43) | 1.52 (0.95) | 4.05 (0.43) | 3.29 (0.41) | 9.46 (1.18) | 5.18 (0.61) |
| wf-close-fixed | 0.65 (0.51) | 1.79 (1.05) | 3.93 (0.38) | 3.29 (0.33) | 9.65 (1.11) | 5.28 (0.65) |
| wf-customer-latest-order | 0.44 (0.31) | 1.45 (0.66) | 4.37 (0.26) | 3.58 (0.23) | 8.97 (0.53) | 5.46 (0.30) |
| **sum, total** | 12.5 | 38.4 | 89.4 | 73.6 | 210.3 | 116.6 |
| **sum, setup** | 2.3 | 13.8 | 81.9 | 67.1 | 191.1 | 106.0 |
| **sum, run** | 10.2 | 24.7 | 7.2 | 6.4 | 18.9 | 10.5 |

**What it means.** End to end, a task costs 6–17× more on a JS engine than
on mm, because each JS session evaluates pi's prelude first. pi does this on
every script; djs avoids it with prelude images, and one image would cut its
setup to about 0.04 ms (§3). Counting the script alone, mm is slower than
native QuickJS: the geomean of run time is 0.71× mm's for dquickts and 0.79×
for rquickjs, 1.15× for djs, and 2.0× for quickjs-wasi. Most of mm's loss is
still in `logs-access-top-ips` and `logs-errors-per-service`, at 1.4–1.9 ms
against 0.6–1.3 ms on the native JS engines. Both `import re` and run a regex
over a 58 KB log. Adding `re` and `collections` to the session template, as
`asyncio`/`json` are, remains the next measurement (`MM_TEMPLATE_IMPORTS` in
`hosts/mm` sets the template's imports). mm-wasm runs scripts at about 2.3×
mm native.

### Bench programs [M]

The 13 `bench/` programs and their `.js` twins, run in-process (each run is a
new session in the same host), in ms per run:

| Program | mm-native | mm-wasm | rquickjs | dquickts | quickjs-wasi | djs | djs-wasm |
|---|---:|---:|---:|---:|---:|---:|---:|
| startup | 0.0 | 0.1 | 0.1 | 0.1 | 0.3 | 0.0 | 0.2 |
| fib | 17.2 | 35.4 | 39.6 | 13.2 | 46.0 | 24.1 | 52.8 |
| loop | 51.4 | 110.2 | 119.2 | 52.8 | 239.3 | 123.4 | 172.2 |
| dict | 85.2 | 146.4 | 104.1 | 76.2 | 208.0 | 111.5 | 220.3 |
| str | 32.3 | 63.2 | 37.5 | 21.2 | 89.0 | 33.0 | 62.9 |
| json_rows | 83.3 | 172.4 | 184.1 | 138.0 | 449.2 | 98.7 | 197.9 |
| closure | 35.3 | 74.3 | 66.2 | 28.5 | 119.0 | 62.8 | 108.8 |
| classes | 71.4 | 139.6 | 101.6 | 61.0 | 218.9 | 96.6 | 179.5 |
| generators | 130.1 | 253.5 | 140.4 | 84.3 | 281.8 | 803.0 | 1747.0 |
| exceptions | 67.5 | 146.1 | 259.8 | 44.2 | 710.6 | 279.4 | 729.0 |
| fmt | 128.6 | 253.6 | 89.8 | 60.4 | 206.3 | 99.0 | 259.4 |
| sort | 104.9 | 179.9 | 283.9 | 154.6 | 394.4 | 289.4 | 514.4 |
| interp | 144.0 | 312.4 | 185.3 | 130.5 | 450.4 | 213.2 | 465.6 |

The same programs as CLI processes, timed from spawn to exit (`mm run` against
the `qjs` and djs CLIs):

| Program | qjs-ng | qjs-bellard | djs-cli | mm-cli |
|---|---:|---:|---:|---:|
| startup | 2.7 | 2.8 | 3.0 | 2.7 |
| fib | 16.4 | 15.0 | 29.3 | 18.7 |
| loop | 65.5 | 54.1 | 116.3 | 61.4 |
| dict | 88.5 | 84.1 | 116.2 | 82.4 |
| str | 37.7 | 27.4 | 33.6 | 36.0 |
| json_rows | 170.9 | 151.5 | 102.7 | 84.4 |
| closure | 36.1 | 31.5 | 62.8 | 35.2 |
| classes | 62.4 | 60.3 | 95.6 | 75.6 |
| generators | 68.2 | 80.0 | 847.2 | 136.5 |
| exceptions | 219.5 | 124.1 | 338.8 | 69.1 |
| fmt | 75.4 | 64.6 | 111.5 | 131.9 |
| sort | 167.2 | 162.8 | 296.6 | 112.6 |
| interp | 140.9 | 135.2 | 231.3 | 142.9 |

**Compiler control** [M]. rquickjs's slowdown against the first run (1.16 →
1.66× mm) is mostly gcc 14. The rquickjs and dquickts hosts were built again
with gcc 12.4 and timed against the gcc 14.2 builds, interleaved, 3 rounds of
3 runs: gcc 14 makes rquickjs (quickjs-ng 0.16.2) 1.27× slower on the
geomean (`fib` 2.3×, `loop` 1.6×, `closure` 1.7×) and dquickts 0.96×. The
quickjs-ng 0.18 `qjs` CLI moved like Bellard's (0.96 → 1.08× and 0.83 →
0.94× `mm run`), so the effect is specific to the bundled 0.16.2 sources or
their build.

**What it means.** On raw interpretation mm sits between the QuickJS builds.
Bellard's 2026-06-04 QuickJS (dquickts) is 1.18× faster than mm on the
geomean. rquickjs is 1.66× slower than mm as built here, and about 1.31×
slower [E] with gcc 12. mm is 1.8× faster than djs and 3.4× faster than
quickjs-wasi. mm wins `json_rows` and `sort` against every JS engine, and
`exceptions` and `dict` against all but Bellard's QuickJS. It loses `fmt` to
native QuickJS and djs, and most programs to Bellard's QuickJS. As CLIs mm is level
with `qjs-ng` (1.08× in mm's favour) and 1.06× slower than Bellard's `qjs`.

## 8. GC pauses [M]

`gc ITEMS 1024` builds ITEMS live pairs `[i, str(i)]` (about 59 and 199 MiB
live in mm at 550,000 and 2,000,000, spec §9.2's rows) under a 1 GiB limit,
times five explicit full collections, then runs four workloads beside them:
`garbage` (4M iterations of `y = [i]`), `cycles` (2M of a one-item list that
contains itself, garbage only a tracing pass frees), `writes` (4M in-order
writes of new strings into the old pairs) and `appends` (500k new pairs
kept). It reports the longest gap between the engine's control points, where
a host can stop the run: mm's 10k-fuel parks, QuickJS's interrupt handler
(every 10,000 polls), djs's 10k-fuel slices. A collection inside the run
lengthens the gap it falls in. mm is measured a second way, as spec §9.2 does:
no fuel, a park request every 1 ms, and the latency from request to park.
Medians of the 5 rounds' longest gaps, ms; "Full" is one explicit full
collection (mm `gc.collect()`, QuickJS `JS_RunGC`, djs `Vm::collect`). The
malloc heap column is what the live data holds in each engine.

| Engine | Pairs | Malloc heap | Full | garbage | cycles | writes | appends |
|---|---:|---:|---:|---:|---:|---:|---:|
| mm, park requests | 550k | 73 MB | 15.5 | 0.9 | 3.0 | 1.4 | 0.2 |
| mm, 10k-fuel parks | 550k | 73 MB | 14.4 | 22.1 | 21.7 | 24.2 | 1.6 |
| rquickjs | 550k | 123 MB | 14.8 | 0.7 | 61.3 | 0.7 | 57.6 |
| dquickts | 550k | 106 MB | 15.1 | 0.3 | 47.1 | 0.4 | 50.5 |
| djs | 550k | 69 MB | 28.4 | 34.6 | 36.1 | 48.1 | 60.0 |
| mm, park requests | 2M | 249 MB | 72.7 | 0.1 | 0.1 | 2.9 | 0.5 |
| mm, 10k-fuel parks | 2M | 249 MB | 74.0 | 24.7 | 1.0 | 42.0 | 1.6 |
| rquickjs | 2M | 443 MB | 135.5 | 0.6 | 218.4 | 0.6 | 229.9 |
| dquickts | 2M | 381 MB | 127.1 | 0.3 | 211.0 | 0.4 | 1.2 |
| djs | 2M | 195 MB | 138.6 | 145.0 | 145.4 | 189.0 | 0.6 |

Worst rounds: mm with park requests 3.4 ms (550k `cycles`) and 3.3 ms (2M
`writes`); mm between fuel parks 51.8 ms (2M `writes`); rquickjs 239.6 ms,
dquickts 212.7 ms, djs 206.0 ms (all at 2M).

How each engine collects [S, from source]:

- **mm** traces, and over 32 MiB live a due collection marks in slices of
  2^18 work units, one per safepoint, so a park request lands within one
  slice (spec §9.2). Its pause a host sees is therefore at most 3.4 ms at
  either size, and does not grow with the heap: the 2.4–2.7 ms the spec
  reports at 199 MiB, reproduced. Below 32 MiB, and when too little room is
  left, collections run in full (at 256 MiB, 2M pairs sit near the limit:
  45–78 ms gaps and a `MemoryError` on `appends` in a trial run [M]).
- **QuickJS** (rquickjs, dquickts) frees acyclic garbage by reference
  counting at once, so `garbage` and `writes` never collect (gaps under
  1 ms). Cycles wait for the cycle collector, which runs when malloc grows
  past 1.5× its size after the last collection and walks every GC object in
  one go: 47–61 ms at 550k pairs and 211–230 ms at 2M. dquickts's 1.2 ms on
  `appends` at 2M means its 500k appends never reached the trigger, not that
  the collection got shorter.
- **djs** compacts (mark-compact, `Heap::collect_auto`) at safepoints inside
  a slice, with no incremental marking: every workload that allocates
  pauses 35–60 ms at 550k pairs and 145–189 ms at 2M, about a full
  collection. pi seals its prelude into an old region that minor collections
  skip; data a script builds is young and is traced.

**mm between fuel parks: an open finding.** mm checks at a safepoint every
256 fuel units at most (`TICKS`) and runs a due collection step there, but
the step's work is not charged as fuel (`Vm::safepoint`, `charge_ticks`)
[S], so one 10k-fuel slice can hold up to about 39 steps, a
whole incremental mark of 59–199 MiB. A park request still lands within a
step (above). But a host that only regains control between fuel slices
(`Session::drive_async` yields every `ASYNC_SLICE_FUEL`, 10,000 units, "about
a millisecond") sees a 22–52 ms poll once 59–199 MiB are live. The longest
gap at 2M falls where the mark starts during `writes`: the parks around it
show the mark active for only one or two slices [M]. Charging collection
steps as fuel, or ending a slice after a step, would bound it; neither was
tried here.

**What it means.** For a host that stops mm with its park handle (§5, and
mm-wasm-host's park requests), mm's GC pause is a few milliseconds at any
heap size measured, where every JS engine pauses for a full trace of the
heap: 0.3–0.55 ms per MB of malloc heap on QuickJS and 0.5–1 ms on djs. Code-mode
sessions rarely hold tens of MB; at the 1–10 MB a typical task holds, every
engine's collection stays under about 10 ms [E, scaled from these rows].

## 9. Safety notes [S]

From each engine's source at the pins, plus what pi and plak set on top.

| Engine | Language | Default sandbox surface | Memory limit | Stack / depth limit | Output limit |
|---|---|---|---|---|---|
| micro-monty | Rust. `unsafe` is forbidden across the workspace, except the spec §5 exceptions: the wasm guest cdylib, its allocator (`mm-wasm-alloc`) and the cell worker's FFI. No panics on guest input (clippy denies `unwrap`, `expect`, `panic!`) | No ambient I/O (spec §1, §10): `os`, `pathlib` and `open` exist, but every operation that touches the outside calls a host function the embedder registered, or raises `PermissionError`. No sockets or processes. Clock, entropy and tools come from the host | `Limits::memory` 256 MiB by default: arena, code, stacks and scheduler tables; `MemoryError` past it | `recursion_depth` 1000 → `RecursionError`. Every Rust recursion is depth-counted, so it never overflows the Rust stack | None in the facade; the host's output callback decides. `mm-cell-worker` caps at 1 MiB (plak's value) |
| rquickjs 0.14 (quickjs-ng 0.16.2), plak's JS host | C engine with Rust bindings (`unsafe` FFI) | `Context::full`: ECMAScript intrinsics only; no `std`/`os` modules unless the host adds them | None by default (`JS_SetMemoryLimit`); pi and this harness set 256 MiB | 1 MiB C-stack guard by default (`JS_DEFAULT_STACK_SIZE`); this harness sets pi's 512 KiB | None (host `console`) |
| QuickJS 2026-06-04 / dquickts | C | As above. The `qjs` CLIs add `std`/`os` (files, processes); dquickts's own `qjs` build also links a CPython C-API layer | As rquickjs | 1 MiB default | None |
| quickjs-wasi 3.6.2 (pi) | C (quickjs-ng) compiled to wasm32-wasip1 | The wasm instance. Imports: 6 `env` host hooks and WASI `clock_time_get`, `random_get`, `fd_write`/`fd_close`/`fd_seek`/`fd_fdstat_get`. pi virtualises the clock and randomness and discards fd output | pi `memoryLimitBytes`: no default beyond wasm32's 4 GiB; 32 MiB on pi's Cloudflare runner | `MAX_STACK_SIZE` 512 KiB | pi's prelude: 16 Mi characters or 100,000 output items, then the script fails |
| djs | Rust, `no_std`. `forbid(unsafe_code)` natively; the shipped wasm module builds with `unsafe-heap` (unchecked arena access, `deny(unsafe_code)` elsewhere): "the wasm instance is the sandbox" | No I/O. Only the host functions the embedder defines. Nondeterminism (clock, `Math.random` seed, weak refs) comes from the host | `VmOptions::memory_limit`: arena cap, default `MAX_ARENA` (≈4 GiB); pi's VM contract sets 256 MB | `max_stack` 1 Mi value-stack slots → `RangeError`; parser depth 550 | None in the engine (pi's prelude applies its own) |

**What it means.** mm and djs are the memory-safe engines. The QuickJS
family is C, so a memory bug in the engine is a bug in the host; plak runs
rquickjs in a worker process and pi runs QuickJS inside wasm for that reason [S]. djs's shipped wasm module drops its own bounds checks
(`unsafe-heap`) and relies on the wasm sandbox. mm keeps its checks in both
builds. No engine caps output by itself: pi's prelude and plak's
`mm-cell-worker` (1 MiB) apply the caps.

## Method

- **Hosts.** One Rust host per engine implements the same `Engine` trait
  (`hosts/common`). Each step is timed in-process with `Instant`, and each
  process prints one JSON line. Hosts use release builds with fat LTO, one
  codegen unit, `panic = "abort"` and `strip`, and the mm hosts carry the
  repo's per-crate overrides. wasmtime 50.0.1 is configured as
  mm-wasm-host's default for every wasm engine: epoch interruption with a
  10 ms ticker, a 100 ms park grace and then a trap, a 4 MiB wasm stack, and a
  precompiled module.
- **Codemode setup.** The JS engines get pi's prelude `PRELUDE_SOURCE` at pi
  `7d073328`, unmodified, with a native `bridge` function. Each script runs as
  the body of `(async (tools, console) => {...})`, as in pi's `vm.ts`. The
  prelude has the 256 MiB heap and 512 KiB stack pi sets. mm gets the same
  tools as typed, keyword-only async functions (`Session::define`, with
  `dict_as_kwargs`).
- **GC.** `gc ITEMS LIMIT_MIB` (§8) runs mm, rquickjs, dquickts and djs on
  plain scripts in one session each; the gaps between control points are
  timed in the host at each slice end or interrupt-handler call. mm runs a
  second time with park requests every 1 ms instead of fuel slices.
- **Rounds.** `run.py` runs 5 rounds and starts every measurement as a new
  process on core 2, rotating the engine order each round. Within a process
  it repeats 3–20 times after one untimed warm-up. Medians are over every
  sample. Five control runs per engine and target.
- **Like for like, and where it is not.** The JS and Python solutions are
  equivalent but not identical code, and pi's prelude does more than mm's
  module (discovery, `store`/`load`, output caps). djs-wasm cannot decode
  host-call arguments outside djs's TypeScript reader, so the protocol
  results are native djs's. dquickts runs through its vendored rquickjs,
  not its `qjs`.

## Pins

| Engine | Revision |
|---|---|
| micro-monty | `main` at `eb78933`, the 0.0.1 release candidate (first run: `5df122e`) |
| rquickjs | 0.14.0, the latest on crates.io; bundles quickjs-ng 0.16.2 |
| quickjs-ng `qjs` | v0.18.0 (`d7215b0a`) |
| QuickJS `qjs` (bench/README's comparator) | 2026-06-04 tarball, sha256 `b376e839…` |
| quickjs-wasi | 3.6.2: the npm `quickjs.wasm` (sha256 `d4c9375f…`), tag `5a7a0eed`, quickjs-ng `6d46d07d`. pi's `packages/codemode` depends on it |
| pi | `7d073328` (2026-11-08) |
| djs | `main` at `0e58f62c` (2026-11-08) |
| dquickts | `main` at `dad0bafd` (2026-11-08), QuickJS 2026-06-04 |
| Toolchain | rustc 1.99.0, gcc 14.2.0 (first run: 12.2.0; §7's control: 12.4.0), binaryen 133, wasmtime 50.0.1 |

`setup.sh` fetches and builds all of it; build commands, the `/tmp/eng`
layout the hosts expect, and how to rerun are in
[`bench/evidence/engines/README.md`](../../bench/evidence/engines/README.md).
