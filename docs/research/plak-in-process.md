# micro-monty in plak's process: a prototype on real sessions

What would it take for plak to run micro-monty in its own process instead of a
child worker, and what does it change on real agent sessions? This note
covers a working prototype on a local plak branch: the seam it uses, the
adapter's size, the problems it hit, and measurements in four modes (Monty
child, micro-monty child, micro-monty native in process, micro-monty wasm in
process), plus native in process with the decoded-stdlib cache prototype from
[session-templates.md](session-templates.md) §5. Numbers are labelled [M]
measured, [S] sourced or [E] estimated; unless a number says otherwise it was
measured on host1 (AMD Ryzen, 16 threads, Linux 7.0, bare metal) on
2026-11-06. The real-session records are in
[`conformance/codemode/real/`](../../conformance/codemode/real/README.md).

The plak branch was never pushed. It lives on host1 as `mm-inproc` in
`~/dev/plak-mm-inproc` (plak GitHub `main` e0809c84, plus the identity
override from [plak-integration.md](plak-integration.md), plus two
commits), with path dependencies on this checkout's `micro-monty` and
`mm-wasm-host`.

## Summary

- **The seam is the transport under `CellClient::spawn_within`.** Replacing
  the child process with a thread that speaks the same frames over an
  in-memory pipe keeps every plak policy (handshake, limits, truncation,
  quotas, the 16 outstanding calls, delivery verdicts, loss classes) where
  it was, unchanged. The
  prototype is a `WorkerProcess` enum (+112/−32 lines in `client.rs`) and an
  `mm_inproc` module (about 1,700 lines, 1,340 of them a copy of
  `mm-cell-worker`'s loop and codec) [M].
- **It works on real sessions.** 96 in-process `plak run` sessions (Codex and
  Grok, py-ptc and py-sdk, native, wasm and native with the decoded-stdlib
  cache) answered as often as the child modes, and all 1,694 of their cells
  replay identically on the micro-monty child [M]. plak's own tests: 38 of
  41 in process against 39 of 41 for the micro-monty child; the extra
  failure needs a worker pid [M].
- **Memory per session is the win.** At 40 concurrent sessions in one
  process: 3.4 MiB per session native, 4.4 MiB wasm, against 12.6 MiB for a
  micro-monty child and 34.3 MiB for Monty's (148 MiB in total against
  514 MiB and 1.35 GiB) [M]. A host call costs 88 µs native against 174 µs
  (mm child) and 272 µs (Monty child) at that load; about 20 µs in every mode
  at one session [M].
- **micro-monty's cost is imports:** 4.3 ms for `json, re, datetime,
  collections, asyncio` against 0.3 ms on Monty, 8 ms on wasm [M]. Real
  cells rarely import (their import cells take 0.5 ms), so no agent sees it.
- **The decoded-stdlib cache** cuts a warm import by 24–37% and RSS per
  session by 18% at 8–40 sessions, in process only [M]. It is invisible on
  one-shot `plak run` sessions, where each process holds one session. It
  pays off in a daemon that serves many sessions; its accounting caveats
  stand (below).
- **Refreshed on main 357e003** (below): the adapter needed no change; the
  import cell is 30% cheaper at one session and 49% at 40 native (one
  shared `Engine` cache), RSS per session at 40 native is 2.8 MiB (−18%),
  and 70 new real cells replay identically on the mm child [M].
- **What changes for plak:** no seccomp around the engine (it would confine
  plak), no per-worker RSS, a cooperative kill on native (a long native
  operation finishes first), crash-cut tests that need a process, and
  +18.3 MB of binary for wasmtime [M]. Native alone would add a few MB [E].

## The seam

plak's Python cell path is

```text
sessions / local_composition  (py-ptc, py-sdk: prompts, bindings, SDK stubs)
  └─ plakous-cell::CellClient   (crates/runtimes/cell/src/client.rs)
        handshake (Hello / Configure), command queue, event byte budget,
        nested-call delivery verdicts, loss classes, stop and kill
        └─ child process: plakous-py-host (Monty) or PLAK_PY_HOST
              plakous-cell-protocol.v2 frames on stdin / stdout
```

Most of what plak decides about a cell lives in `CellClient` or above it:
the engine limits it sends in `Configure`, the event byte budget, quotas,
the 100-call nested total, the order and acceptance of nested-call results,
and how a loss is classified. A few rules live in the worker, which mirrors
plak's Monty worker: at most 16 outstanding nested calls (the 17th raises
`RuntimeError: local composition exceeds its profile-bound limit of 16
outstanding nested calls`; there is no queue), output and result truncation
at 1 MiB with plak's notices, and, in Monty's worker only, the pre-run type
check of py-sdk cells. So the narrowest seam is **the transport under
`CellClient::spawn_within`**: where it builds a `tokio::process::Command`
and takes the child's stdin and stdout. Moving the worker's loop in process
unchanged keeps the worker-side rules too.

The prototype replaces those three things with a `WorkerProcess` enum:

- `WorkerProcess::Child(Child)`: unchanged.
- `WorkerProcess::Thread(InProcess)` (feature `mm-inproc`, selected at run
  time with `PLAK_PY_INPROC=native|wasm`, only when the expected worker is
  the Monty identity): a thread of plak's process running
  `mm-cell-worker`'s worker loop. Its input and output are the two ends of a
  `tokio::io::duplex` pipe (1 MiB each way), bridged to blocking
  `Read`/`Write` on the worker thread with `tokio_util::io::SyncIoBridge`.

`CellClient` still writes and reads the same frames, so the handshake,
`Configure` validation, the event byte budget, delivery verdicts and loss
classes run unchanged, and the worker-side rules (16 outstanding calls,
truncation, error mapping to Python exceptions) are `mm-cell-worker`'s own
code. No policy is new or moved. Two of
`WorkerProcess`'s methods differ by kind:

- `start_kill`: a child gets `SIGKILL`; a thread gets a kill flag plus
  repeated park requests (every 5 ms) until it ends.
- `needs_closed_pipes`: a thread only notices the end when its input
  closes, so `terminate` and the refusal path abort the client's I/O tasks
  (dropping plak's ends of the pipe) before waiting for the thread.

`child_id()` returns `None` for a thread (it returned plak's own pid in the
first draft; see Problems).

The engine sits behind one trait, `PySession` (define, start, resume,
cancel, reset, set_deadline, set_output, park), with two implementations:

- **native**: `micro_monty::Engine` and `Session`, linked into plak.
- **wasm**: `mm_wasm_host::Session` on one process-wide
  `mm_wasm_host::Runtime` (one wasmtime engine, the `mm-wasm` v1 guest
  precompiled next to `PLAK_PY_INPROC_WASM` on first use, the epoch ticker),
  with [wasm-embedding.md](wasm-embedding.md)'s defaults: epoch
  interruption, a 100 ms park grace period, and one session per store and
  instance.

### What did not move

- **Seccomp.** `mm-cell-worker` installs a filter after `Configure`. In
  process it would confine plak itself, so the thread has none. The engine
  never makes a system call on a guest's behalf (no ambient filesystem,
  processes or network), so the filter was defence in depth. In process,
  that depth is the engine's own boundary (native) or wasmtime's (wasm).
- **Process accounting.** A thread's `ResourceReport` carries its CPU time
  (`/proc/self/task/TID`) but no RSS: plak's own RSS includes every session.
  The engine's memory limit (256 MiB, `EngineConfig::MONTY`) still applies
  per session, from the arena and charged host values.
- **Crash cuts** (`delivery_crashes`, `real_cell_worker_crash`) need a
  process to kill at a cut point; they do not apply to a thread.
- **Kill.** A child dies at once. A native thread stops at its next
  safepoint, so a cell inside one long native operation (a big-int multiply,
  `isolation-modes.md`) ends when that operation does. A wasm thread is
  interrupted at the 100 ms grace epoch.
- **Panics.** plak's release profile uses `panic = "unwind"` (its default),
  which [isolation-modes.md](isolation-modes.md) requires for native in
  process: the facade turns an engine panic into a poisoned session [S]; a panic
  elsewhere on the worker thread ends the thread, its pipe closes, and
  `CellClient` reports the worker lost, as for a child that died.

## Adapter size

| part | lines | what |
|---|---:|---|
| `client.rs` | +112 / −32 | `WorkerProcess`, `start_worker`, kill and close paths |
| `mm_inproc/worker.rs` | 1,082 | `mm-cell-worker`'s loop at 1ae712b, about 350 lines changed: pipes instead of stdio, no seccomp, a thread's usage report, the kill flag |
| `mm_inproc/codec.rs` | 258 | `mm-cell-worker`'s codec, 5 lines changed |
| `mm_inproc/engine.rs` | 207 | `PySession` over the native facade and `mm-wasm-host` |
| `mm_inproc/mod.rs` | 135 | the thread, the pipe, kill and wait |
| `mm_inproc/trace.rs` | 180 | measurement only (`PLAK_PY_INPROC_TRACE`) |
| `Cargo.toml`, `lib.rs` | +9 | the feature and path dependencies |
| `examples/inproc_load.rs` | 401 | measurement only (the load harness) |

Without the measurement files, the adapter is about 1,800 lines, of which
1,340 are a copy of `mm-cell-worker`. A real adoption should not copy them:
`mm-cell-worker` would become a library (`serve(reader, writer, engine)`)
used by both the child binary and plak's in-process backend.

**Since 2026-11-07 it is** (HANDOFF scope decision 44): `mm_cell_worker`
is a library and the binary sits on it. The plak branch now depends on it
by path and the copy is gone [M]:

| part | lines | what |
|---|---:|---|
| `client.rs` | +112 / −32 | unchanged |
| `mm_inproc/mod.rs` | 136 | the thread, the duplex pipe, `Kill`, the thread's usage hook |
| `mm_inproc/engine.rs` | 143 | `Backend`; `NativeEngine` (one per process); `CellEngine`/`CellSession` over `mm-wasm-host` |
| `mm_inproc/trace.rs` | 180 | measurement only |
| `Cargo.toml`, `lib.rs` | +11 | the feature and path dependencies |

The adapter is now about 400 lines without the measurement files, against
about 1,800. plak's in-process lib tests give the same 38 of 41 in both
modes, with the same three failures. One wrinkle: the library depends on
`plakous-cell` (git rev 9ba53187), so plak's local `plakous-cell`, which
holds the backend, builds a second copy of the protocol crate, and cargo
needs `-p plakous-cell@0.0.3` to name plak's. Only bytes cross between the
two copies, plus `ProcessResourceUsage`, which the library re-exports
(`mm_cell_worker::plakous_cell`). A real adoption puts the backend in a
crate above `plakous-cell` (a `[patch]` that unifies the two copies would
otherwise make a dependency cycle).

Build cost [M]: the stripped release `plak` grows from 73.4 MB to 91.7 MB
(+18.3 MB) with both backends, almost all of it wasmtime (and Cranelift for
the first precompile). `Cargo.lock` gains 1,070 lines. A native-only build
would add about the facade's size, 3–4 MB [E] (the `mm` CLI is 3.5 MB).

## Problems hit

1. **`child_id()` returned plak's own pid.** The first draft kept
   `pid: u32` and filled it with `std::process::id()`. plak's
   `killing_the_python_worker_does_not_kill_parent` sends `SIGKILL` to the
   worker pid, and killed the test process. Fixed: `child_id()` is
   `Option<u32>` and `None` for a thread, so that test now fails cleanly ("no
   pid to kill"), which is expected.
2. **A refused handshake hung.** `refuse` kills, then waits for the worker;
   a thread only ends when its input closes, and plak still held the pipe.
   Fixed by dropping the pipe ends before the kill on every path that waits
   (`refuse`, `terminate`, the loss path).
3. **No process boundary means no process signals.** Kill is cooperative on
   native (above). A native cell in one long operation holds its thread
   until the operation ends; plak's other sessions keep running.
4. **Memory accounting.** A thread has no RSS of its own, so plak's
   `peak_rss_bytes` per worker is `None`; the per-session numbers below come
   from the process's RSS divided by the sessions.
5. **The SDK type check.** In process, as in the child, micro-monty runs
   py-sdk cells unchecked ([plak-integration.md](plak-integration.md)
   "SDK stubs are accepted"). plak's
   `real_python_sdk_driver_values_edits_state_and_quotas` fails at its one
   type-error cell, in all three micro-monty modes.

## plak's tests in process [M]

plak's library tests that drive a Python worker through `CellClient`
(`python_tests`, `real_monty_driver`, `real_python_sdk`; 41 tests):

| mode | passed | failing |
|---|---:|---|
| Monty child (`plakous-py-host`) | (reference) | |
| micro-monty child | 39 of 41 | allocator ceiling (Monty-specific), SDK type-error cell |
| native in process | 38 of 41 | the same two, plus `killing_the_python_worker_does_not_kill_parent` (no pid) |
| wasm in process | 38 of 41 | the same three |

plak's `plakous-py-worker` integration tests (`process`, `session`, 5 tests)
start the worker binary directly and pass on the micro-monty child (5 of 5);
they do not exercise the in-process backend. `delivery_crashes` and
`real_cell_worker_crash` need a process.

## Load: many sessions in one plak process [M]

`inproc_load` (plak branch `examples/inproc_load.rs`, copied to
`conformance/codemode/real/inproc_load.rs`) is the runner's shape without a
model: N sessions start at once in one tokio process, through plak's own
`CellClient`, and each runs

1. an import cell: `import json, re, datetime, collections, asyncio`;
2. a host-call cell: 200 sequential `await tool({})`, each answered at once,
   so it measures the boundary, not a tool;
3. the cells of one recorded real session (round robin over the 32 Codex
   sessions of Phase 1), each nested call answered with its recorded result.

Then all sessions wait at a barrier while the process's RSS (plus every child
worker's) is sampled. Medians over 3 runs (`results/load/sweep.jsonl`,
`loadsum.py`); host1 has 16 hardware threads, so N=40 oversubscribes it 2.5
times.

| N | mode | start ms | import cell ms | per host call µs | replayed cell ms | RSS per session KiB | total RSS MiB |
|---:|---|---:|---:|---:|---:|---:|---:|
| 1 | Monty child | 1.23 | 0.28 | 18.3 | 0.18 | 20,832 | 31.5 |
| 1 | mm child | 0.87 | 4.27 | 24.4 | 0.16 | 13,116 | 23.9 |
| 1 | mm native | 0.12 | 4.35 | 20.0 | 0.18 | 6,388 | 17.2 |
| 1 | mm wasm | 0.11 | 8.16 | 21.4 | 0.27 | 11,724 | 22.5 |
| 8 | Monty child | 1.35 | 0.21 | 24.9 | 0.14 | 19,808 | 165.8 |
| 8 | mm child | 0.93 | 6.21 | 34.1 | 0.18 | 12,055 | 105.3 |
| 8 | mm native | 0.25 | 4.88 | 25.2 | 0.15 | 3,203 | 36.2 |
| 8 | mm wasm | 0.24 | 10.28 | 28.5 | 0.20 | 4,484 | 46.2 |
| 40 | Monty child | 4.35 | 0.71 | 272.5 | 1.44 | 35,088 | 1,381.7 |
| 40 | mm child | 5.90 | 8.80 | 173.9 | 1.36 | 12,872 | 513.8 |
| 40 | mm native | 0.85 | 10.51 | 88.4 | 1.32 | 3,515 | 148.3 |
| 40 | mm wasm | 0.11 | 24.90 | 124.8 | 2.08 | 4,459 | 185.3 |

"Start" is `CellClient::spawn` through `Hello` and `Configure`; RSS per
session is (total − the process before the burst) / N, children included.

- **Memory is the in-process win.** At 40 sessions, native in process holds
  each session in 3.4 MiB against 12.6 MiB for the micro-monty child and
  34.3 MiB for Monty's: 148 MiB in total against 514 MiB and 1.35 GiB. wasm
  costs 0.9 MiB more per session than native (an instance and its linear
  memory) and 37 MiB more in total.
- **The boundary gets cheaper, most under load.** A host call costs about
  the same at N=1 in every mode (18–24 µs; the pipe and frame codec
  dominate, and in process they remain). At N=40 the child modes pay for 40
  processes' scheduling and pipes: 174 µs (mm) and 272 µs (Monty) against
  88 µs native and 125 µs wasm.
- **Imports are micro-monty's cost, in every mode.** The five-module import
  cell takes 4.3 ms on micro-monty against 0.3 ms on Monty, and doubles on
  wasm (8.2 ms). Monty's modules are native; micro-monty's are bundled
  bytecode that each session unpacks, decodes and runs. Under 2.5×
  oversubscription the cost grows with the CPU share (10.5 ms native, 24.9
  ms wasm). Per module, on `mm run` [M]: `json` 0.14 ms, `collections`
  0.65, `re` 0.70, `asyncio` 0.97, `datetime` 1.43. Real cells rarely import
  (22% of cells, mostly `json`; `conformance/codemode/real/README.md`).
- **Starting a session is 10 times cheaper in process** (0.1–0.9 ms against
  0.9–5.9 ms for a child). wasm starts are the cheapest at N=40, since the
  module is precompiled and instantiation is a copy-on-write map.
- **Monty's 16 failures at N=40 are a harness artifact.** The synthetic
  host-call cell calls `tool`, which the SDK stubs do not declare, so Monty's
  type checker refuses it in the 16 py-sdk sessions ("Name `tool` used when
  not defined; nothing ran") at every N. micro-monty does not type-check.
  Monty's host-call numbers come from the 24 py-ptc sessions.
- **Replayed real cells** take 0.15–0.3 ms at N≤8 in every mode: real cells
  are cheap, and the engine is not what an agent waits on.

## The decoded-stdlib cache, native in process [M]

The prototype patch from the snapshot thread (`StdModule::decode` keeps each
decoded bundled module in a process-wide `Mutex<Vec<…>>`, shared as
`Arc<Code>`; [session-templates.md](session-templates.md) §5) was applied
with `git am` to a separate worktree of main (5d260f0 + the patch, 44610d5),
and plak's in-process branch was built against it (`mm-inproc-cache`, same
code, other path). It was not pushed. Only native in process can use it:
the child modes start a new process per session, and wasm decodes inside
each instance.

Cold (the first session of the process pays the decode; `sweep.jsonl`,
3 runs each):

| N | mode | import cell ms | per host call µs | RSS per session KiB | total RSS MiB |
|---:|---|---:|---:|---:|---:|
| 1 | native | 4.35 | 20.0 | 6,388 | 17.2 |
| 1 | native + cache | 4.20 | 19.3 | 6,316 | 17.2 |
| 8 | native | 4.88 | 25.2 | 3,203 | 36.2 |
| 8 | native + cache | 5.01 | 24.7 | 3,036 | 34.7 |
| 40 | native | 10.51 | 88.4 | 3,515 | 148.3 |
| 40 | native + cache | 9.04 | 96.1 | 3,039 | 129.8 |

Warm (one session imports and ends before the burst, as in a runner that
has served cells before; `warm.jsonl`, 5 runs each):

| N | mode | import cell ms | per host call µs | RSS per session KiB | total RSS MiB |
|---:|---|---:|---:|---:|---:|
| 1 | native | 4.50 | 19.4 | 1,172 | 17.7 |
| 1 | native + cache | 3.41 | 19.4 | 2,356 | 18.9 |
| 8 | native | 4.75 | 25.2 | 2,504 | 36.0 |
| 8 | native + cache | 4.04 | 25.5 | 2,056 | 32.7 |
| 40 | native | 11.85 | 107.9 | 3,384 | 148.9 |
| 40 | native + cache | 7.44 | 90.7 | 2,776 | 125.1 |

- **Warm, the cache saves 24% of the import cell at N=1 (4.50 → 3.41 ms),
  15% at N=8 and 37% at N=40,** and 18% of RSS per session at N=8 and N=40
  (−0.4 and −0.6 MiB; 16% of the total at N=40). This matches the orb's
  20–35% on imports; the RSS saving is about half the orb's 37%, because
  here a session also holds plak's per-session state, the pipe buffers and
  a replayed real session.
- **Cold, it saves less:** 40 sessions that start together all miss, so the
  first wave decodes in parallel; the saving at N=40 is 14% of the import
  cell and 13% of RSS.
- **What it does not do:** decode is about a quarter of a fresh import;
  running the module bodies is the rest, so the import cell stays at
  3.4 ms against Monty's 0.3. Host calls and replayed cells do not change
  (within noise). At N=1 the cache's own entries show up as more RSS (the
  per-session number is small and noisy there).
- **Caveats, from the patch's author and seen here:** a cache-hit session
  is not charged for the shared code, so its memory budget differs from a
  cache-miss session's; the cache never evicts (it is bounded by the bundled
  stdlib's decoded size); and it needs `std` in mm-engine or a cache owned
  by the facade's `Engine`. The patch also adds one `unused_qualifications`
  warning (vm.rs:49).

## Real sessions in four modes [M]

The 8 review prompts of [the real-transcript census](../../conformance/codemode/real/README.md)
ran again through the in-process plak build, with the same models (Codex
`gpt-6-luna` high, Grok `grok-4.7` medium), compositions and worktrees:
`PLAK_PY_INPROC=native`, `=wasm`, and native on the cache build. The Monty
child and micro-monty child rows are Phase 1's sessions (plak e0809c84, the
same day). 160 sessions and 2,841 cells in total
(`conformance/codemode/real/results/modes.md`, `modes.py`):

| family | composition | mode | sessions | answered | cells | failed | median engine ms | p90 engine ms | median host call ms | memory KiB (median per session) |
|---|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| codex | py-ptc | monty | 8 | 8 | 61 | 9 | 0.37 | 0.67 | 54.6 | 10,120 (worker peak) |
| codex | py-ptc | mm | 8 | 8 | 43 | 6 | 0.42 | 0.57 | 58.5 | 4,948 (worker peak) |
| codex | py-ptc | native | 8 | 8 | 42 | 8 | 0.32 | 0.45 | 55.0 | 7,712 (plak growth) |
| codex | py-ptc | wasm | 8 | 8 | 55 | 6 | 0.44 | 0.73 | 54.0 | 8,852 (plak growth) |
| codex | py-ptc | cache | 8 | 8 | 43 | 8 | 0.34 | 0.47 | 54.1 | 7,654 (plak growth) |
| codex | py-sdk | monty | 8 | 8 | 60 | 4 | 2.39 | 16.17 | 34.3 | 30,436 (worker peak) |
| codex | py-sdk | mm | 8 | 8 | 62 | 3 | 2.54 | 5.93 | 31.0 | 7,072 (worker peak) |
| codex | py-sdk | native | 8 | 8 | 61 | 4 | 2.33 | 4.71 | 33.6 | 12,718 (plak growth) |
| codex | py-sdk | wasm | 8 | 7 | 57 | 3 | 2.85 | 6.08 | 29.7 | 12,488 (plak growth) |
| codex | py-sdk | cache | 8 | 8 | 60 | 2 | 2.02 | 3.85 | 26.2 | 12,314 (plak growth) |
| grok | py-ptc | monty | 8 | 8 | 171 | 7 | 0.48 | 1.33 | 37.7 | 12,160 (worker peak) |
| grok | py-ptc | mm | 8 | 7 | 219 | 2 | 0.50 | 0.97 | 42.2 | 5,878 (worker peak) |
| grok | py-ptc | native | 8 | 7 | 173 | 4 | 0.40 | 0.64 | 37.0 | 16,256 (plak growth) |
| grok | py-ptc | wasm | 8 | 8 | 179 | 5 | 0.68 | 1.91 | 40.3 | 20,086 (plak growth) |
| grok | py-ptc | cache | 8 | 7 | 230 | 3 | 0.40 | 0.64 | 37.7 | 16,348 (plak growth) |
| grok | py-sdk | monty | 8 | 7 | 239 | 15 | 1.74 | 2.89 | 33.9 | 30,926 (worker peak) |
| grok | py-sdk | mm | 8 | 7 | 292 | 20 | 1.95 | 3.26 | 36.8 | 7,278 (worker peak) |
| grok | py-sdk | native | 8 | 8 | 256 | 12 | 1.76 | 2.29 | 32.5 | 20,378 (plak growth) |
| grok | py-sdk | wasm | 8 | 7 | 282 | 10 | 2.58 | 3.76 | 34.0 | 21,430 (plak growth) |
| grok | py-sdk | cache | 8 | 8 | 256 | 12 | 1.78 | 2.46 | 32.2 | 20,468 (plak growth) |

"median host call ms" is the time a cell waits per nested call: the tool's
own work in plak (reading a file, grep, a shell command), the same in every
mode. "Memory" differs by mode: for a child it is the worker's peak RSS; in
process it is how much plak's whole process grew over the session, which
includes plak's own state for the session (transcript, tool output), so it
is an upper bound for the engine; the load sweep above isolates the engine.

- **Same success in every mode.** Every session answered except Grok's
  dict-review runs that reached plak's 1,200 s timeout while the model was
  still reviewing (three in process: grok-ptc native and cache, grok-sdk
  wasm; three in Phase 1: grok-ptc mm, grok-sdk Monty and mm) and one Codex
  py-sdk wasm session that ended on a provider error ("function call
  arguments are not strict JSON") after 6 good cells. The graded prompts are 24 of 24 correct in process (16 of 16
  in Phase 1).
- **Identical cell behaviour.** `replay.py` reran every in-process cell on
  the micro-monty child with the recorded host results: 1,694 of 1,694
  `same` (native 532, wasm 573, cache 589): same outcome, output, value and
  host calls. No worker was lost and no cell was stopped. The cell failures
  are the same kinds as in the child: model errors, plak tool errors, and
  6 refusals (`ast` 3, `subprocess` 2 by design, `tokenize` 1).
- **plak's policy holds in process.** One Codex py-sdk native cell read
  every file of `conformance/own` in a loop and was stopped at plak's
  profile-bound total of 100 nested calls ("local composition exceeded its
  profile-bound total of 100 nested calls"), by `CellClient` and the
  composition, as for a child.
- **Engine time is what the load sweep predicts.** Native in process is at
  or below the child modes (median 0.32–0.40 ms per py-ptc cell, 1.76–2.33
  ms per py-sdk cell, within noise of Monty's 0.37–0.48 and 1.74–2.39); wasm
  adds 0.1–0.8 ms per cell. Cells that
  import anything take 0.5 ms (native) to 0.8 ms (wasm, Monty) in the
  median, because real cells import `json` and little else. Against a
  median cell wall time of 40–60 ms and sessions of 0.5–20 minutes, none of
  it is visible to the agent: a session's time is the model's.
- **The cache does not show on real sessions** beyond noise (codex py-sdk
  2.33 → 2.02 ms median engine time, grok 1.76 → 1.78): few real cells
  import, and plak runs one Python session per `plak run` process, so every
  session is a first session and pays the decode.

## Refresh on main 357e003 (2026-11-07) [M]

The same plak branch, rebuilt against micro-monty 357e003. Since the runs
above, main has gained the Engine-owned decoded-module cache (1c03cff,
8 MiB, scope decision 45), the import-cost pass (b01fd7d), lazy asyncio,
the prelude chunk cache, the 0.1 API pass (`u64` byte quantities,
`DefineError`), the mm-wasm v2 ABI and stress fixes. **The adapter needed no
change**: the `mm_cell_worker` library and `mm-wasm-host` absorb the API
changes, and `mm_inproc/engine.rs` is still 143 lines. Native in process
now gets the cache for free: its sessions share the library's one
`NativeEngine`, and so one `Engine`.

Load sweep, same harness, corpus and machine, medians over 3 runs
(`results/load/sweep-357e003.jsonl`; before → after, before from the table
above):

| N | mode | import cell ms | per host call µs | RSS per session KiB |
|---:|---|---:|---:|---:|
| 1 | Monty child | 0.28 → 0.35 | 18.3 → 19.7 | 20,832 → 20,540 |
| 1 | mm child | 4.27 → 3.33 | 24.4 → 23.5 | 13,116 → 13,036 |
| 1 | mm native | 4.35 → 3.03 | 20.0 → 19.7 | 6,388 → 6,216 |
| 1 | mm wasm | 8.16 → 5.72 | 21.4 → 21.6 | 11,724 → 12,236 |
| 8 | Monty child | 0.21 → 0.18 | 24.9 → 24.2 | 19,808 → 19,819 |
| 8 | mm child | 6.21 → 3.27 | 34.1 → 35.7 | 12,055 → 12,069 |
| 8 | mm native | 4.88 → 3.48 | 25.2 → 24.7 | 3,203 → 3,014 |
| 8 | mm wasm | 10.28 → 7.54 | 28.5 → 28.1 | 4,484 → 4,388 |
| 40 | Monty child | 0.71 → 0.60 | 272.5 → 243.3 | 35,088 → 35,166 |
| 40 | mm child | 8.80 → 9.45 | 173.9 → 158.5 | 12,872 → 12,846 |
| 40 | mm native | 10.51 → 5.41 | 88.4 → 84.9 | 3,515 → 2,884 |
| 40 | mm wasm | 24.90 → 17.94 | 124.8 → 101.4 | 4,459 → 4,162 |

Total RSS at N=40: native 148.3 → 123.9 MiB, wasm 185.3 → 173.8 MiB,
mm child 513.8 → 513.0 MiB, Monty 1,381.7 → 1,384.9 MiB.

- **The import cell is 22–49% cheaper in every micro-monty mode** but one.
  At N=1 and N=8 every session misses the cache (the first of its process
  or instance, or one of 8 that start together), so the gain is mostly the
  import pass: native 4.35 → 3.03 ms
  (−30%), wasm 8.16 → 5.72 ms (−30%), the mm child 6.21 → 3.27 ms at N=8.
  At N=40 native gains most (10.5 → 5.4 ms, −49%), because 40 sessions
  share one `Engine` and so one cache, and decode once. The mm child at
  N=40 does not move (8.8 → 9.5 ms): there each of 40 processes pays its own
  decode on a 2.5× oversubscribed machine.
- **RSS per session at N=40 falls 18% native** (3,515 → 2,884 KiB), the
  same as the cache prototype gave (3,039 KiB cold), and 7% on wasm, which has no shared
  cache. The child modes do not move.
- **Host calls do not change** at N≤8 (within ±1.5 µs). At N=40 every
  mode is 4–19% faster, Monty's unchanged worker included (−11%), so that
  is run-to-run noise under oversubscription, not micro-monty.
- **The gap to Monty's import remains** (3.0 against 0.35 ms at N=1): the
  module bodies still run in every session; templates are the lever for it
  ([session-templates.md](session-templates.md)).
- Monty's 16 failures at N=40 are the same harness artifact as before; no
  micro-monty mode failed a cell.

Real sessions: Codex `gpt-6-luna` at high effort, py-ptc and py-sdk, native
and wasm in process, on three prompts (`loc-census`, `todo-census`,
`codec-review`) in worktrees of 357e003 (`results/phase2-357e003/`):

| arm | sessions answered | cells | failed | median engine ms | p90 engine ms | plak growth KiB (median) | Phase 2 engine ms, p90, growth |
|---|---:|---:|---:|---:|---:|---:|---|
| codex py-ptc native | 3 of 3 | 18 | 1 | 0.32 | 0.44 | 5,944 | 0.32, 0.45, 7,712 |
| codex py-ptc wasm | 3 of 3 | 13 | 2 | 0.40 | 0.55 | 5,680 | 0.44, 0.73, 8,852 |
| codex py-sdk native | 3 of 3 | 20 | 1 | 1.72 | 2.75 | 8,380 | 2.33, 4.71, 12,718 |
| codex py-sdk wasm | 3 of 3 | 19 | 2 | 2.42 | 3.10 | 6,644 | 2.85, 6.08, 12,488 |

- **All 70 cells replay identically on the mm child** (`mm-cell-worker`
  at 357e003, `replay.py`: 70 `same`) [M].
- The six failed cells are the model's or the tools': JavaScript
  (`console`, `cmd` undefined), `ctx.proc` instead of `proc`, and two paths
  outside the workspace that the file tools refused. None is an mm refusal
  or a wrong output.
- py-sdk cells are cheaper (median 2.33 → 1.72 ms native, p90 4.71 →
  2.75 ms), as the SDK prelude's imports are; the session's plak growth is
  smaller too. The sets differ (3 prompts against 8), so read the medians
  as a direction, not a measured ratio.

## For the real adoption

1. **Use `mm-cell-worker` as a library** (done on the micro-monty side,
   decision 44: `serve(reader, writer, engine, options)`; seccomp, the
   process usage report and crash cuts are the binary's hooks, and `Kill`
   is the library's). Put plak's backend in a crate above `plakous-cell`,
   which the library depends on.
2. **Keep the seam where it is.** `WorkerProcess` (child or thread) under
   `CellClient` changes no policy. Two things must stay with it: a thread
   has no pid (`child_id()` is `None`, and callers that signal a pid must
   handle that), and a thread ends only when its pipe closes (close before
   waiting, on every path).
3. **Choose the engine per profile**, as isolation-modes.md proposes:
   native by default (least memory, fastest boundary, cooperative kill),
   wasm where a hard interrupt matters (the 100 ms grace epoch) at +0.9 MiB
   per session and twice the import time.
4. **Account memory per process.** plak's resource report has no per-worker
   RSS in process; the engine's per-session memory limit still holds. If
   plak wants a process-level cap, it needs one for the daemon as a whole.
5. **The decoded-stdlib cache** is worth landing for a daemon once its
   accounting is decided (charge each session for the shared code it
   reaches, or count it once per process) and its owner is chosen (`std`
   in mm-engine or a cache on the facade's `Engine`). One-shot `plak run`
   gains nothing from it.
6. **Imports** remain micro-monty's slowest operation against Monty. The
   cache removes decode; running module bodies is the other three quarters
   (`datetime` alone is 1.4 ms). Session templates
   ([session-templates.md](session-templates.md)) are the larger lever.

## Engine findings

No micro-monty engine bug turned up in 2,841 real cells. Three modules the
agents reached for are not bundled: `ast` (3 cells, to parse Python files),
`tokenize` (2) and `subprocess` (3, refused by design with a pointer to the
tools). On 1ae712b, mm's `SyntaxError` text differed from CPython's on
cells with a stray backslash ("unexpected character after line continuation
character" where CPython 3.14 says "invalid syntax. Perhaps you forgot a
comma?"); both rejected the same cells. d984a97 (main since) gives CPython's
message: all 30 syntax-error cells of both censuses now match CPython's
[M].

## Reproducing

The plak branch is local to host1 (`~/dev/plak-mm-inproc`, branch
`mm-inproc`; the cache build is `~/dev/plak-mm-inproc-cache` against
`~/dev/mm-decode-cache`). With it:

```sh
just install release                                  # from the branch; composition-python enables mm-inproc
PLAK_PY_INPROC=native plak run --composition py-ptc -- '…'
PLAK_PY_INPROC=wasm PLAK_PY_INPROC_WASM=…/mm_wasm_guest.wasm plak run …
PLAK_PY_INPROC_TRACE=DIR …                            # frames in cell-tracer format
cargo build --release -p plakous-cell --features mm-inproc --example inproc_load
conformance/codemode/real/inproc_sweep.sh OUT CORPUS "1 8 40" 3
python3 conformance/codemode/real/run_sessions.py --plak-inproc … --plak-cache … --wasm … \
    --arms codex-ptc-native,codex-ptc-wasm,codex-ptc-cache,…
```
