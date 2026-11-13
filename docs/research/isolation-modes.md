# Isolation modes for hosting micro-monty in plak's runner

Commit hashes of micro-monty cited here, other than the public history's
(root `2bcbb8c`), are pre-release history, kept in the private archive
`N0xMare/micro-monty-archive` (branch `history-0.0.1`;
[docs/history](../history/README.md) maps them); the numbers they qualify
are unchanged.

How should a host daemon (plak's runner) run micro-monty cells: linked into
the daemon, inside wasmtime in the daemon, or in a child process as plak's
rquickjs and Monty workers do today? This note measures all three with the same
session code and workloads, runs a fault matrix against each, and makes a
recommendation. Numbers are labelled [M] measured, [S] sourced or [E] estimated.
Unless a number says otherwise it was measured on a 16-vCPU, 31 GiB Amp orb
(an E2B VM on x86-64 Linux 6.1) with Rust 1.99.0 and wasmtime 50.0.1, at
`origin/main` 32125ff. The raw results are in
[`bench/evidence/isolation/results/2026-11-05/`](../../bench/evidence/isolation/results/2026-11-05/)
(`perf.jsonl`, `faults.jsonl`, `report.md`, `build.txt`).

## Recommendation

*Updated 2026-11-05 at `ac10b28`, after the robustness thread's hardening
(see "Rows the robustness thread changed"). The owner has since chosen
native in-process as the default, with wasmtime in-process as a supported
option per tenant or profile; [wasm-embedding.md](wasm-embedding.md) has
that interface.*

1. **Native in-process is the performance winner by an order of magnitude on
   the boundary.** It is also the only mode where an engine fault can take the
   daemon down. Run it with these conditions:
   - Build with `panic = "unwind"`. The facade then turns an engine panic
     into a poisoned session (`SystemError`) and nothing else. With
     `panic = "abort"` or shipped `immediate-abort` the daemon dies [M].
   - Give session threads headroom: 2 MiB or more (tokio's `spawn_blocking`
     default is enough). At 32125ff and 58d7aca, the cyclic-dict `==`
     aborted the whole process on 256–512 KiB threads. At ac10b28 both stack
     shapes pass at 256 KiB [M]; the margin is for native recursion not yet
     found.
   - Give cells fuel, and cancel with fuel slices or `ParkHandle::request`
     (park in 0.04–0.07 ms [M]). At ac10b28 a big-int operator the remaining
     fuel cannot pay for parks before it starts (0.04 ms, against 7.4 s
     before). A park request or deadline still waits for an operation in
     progress (7.3 s for a 4M-digit multiply [M]).
   - The out-of-arena `HostValue` conversion found here (487/645 MiB host
     RSS for a 64 MiB session) is charged to the session at ac10b28: the same
     shapes now peak at about 115 MiB, or raise `MemoryError` [M].
2. **Wasmtime in-process contains every fault measured without leaving the
   daemon.** Stack exhaustion, panics in any build, runaway native calls and
   out-of-arena allocation each trap only the one instance. The daemon, the
   other sessions and new sessions are unaffected [M]. It costs about 2× CPU
   on the bench programs, 1.0 µs per host call with the mm-wasm v1 binary
   interface (2.5 µs with the JSON harness guest), and 1.6–4× the memory of
   native. With mm-wasm v1, a park request no longer loses the instance
   (0.03 ms). Only a single long native operation still needs the epoch trap
   [M]. Use it for tenants or profiles that need a hard fault boundary.
3. **The child process remains the conservative option.** CPU throughput
   equals native (0.97–1.04×), every fault stays in the worker, and only this
   mode takes OS controls: rlimits (`RLIMIT_AS` and `RLIMIT_CPU` worked [M]),
   seccomp (`mm-cell-worker` installs one [S]) and per-worker cgroups. It
   costs 52 µs per host call in this VM, 1.8–2 ms per new worker and about
   0.3–1 MiB PSS per worker [M].

Snapshots move between all three modes in every direction (9/9 combinations
resumed with identical output [M]). Choosing a mode is therefore not
permanent: a session can be parked in one mode and continued in another, for
example demoted to a child process after a suspicious fault.

## The three modes as built

All three run one shared crate, `iso-service`, which holds one
`micro_monty::Session` behind a request/response API (new session, run,
resume, snapshot, restore, cancel, park, inject panic). The modes differ only
in transport, so the comparison is apples to apples:

| Mode | Container | Transport | Async stop |
|---|---|---|---|
| native | the harness process, the session on the caller's thread | direct call | `ParkHandle::request()` from another thread |
| wasm | one wasmtime `Store` and instance per session, guest `iso_guest.wasm` (wasm32-wasip1 reactor) | message in linear memory (`mm_in` / `mm_call` exports) | epoch interrupt (traps the instance) |
| child | one `iso-worker` process per session, session on a 16 MiB thread (as `mm-cell-worker`) | `u32 LE` length-prefixed frames over stdin/stdout | a `Park` frame, which the worker's reader thread turns into a park request |

`iso-worker` is a minimal equivalent of `mm-cell-worker`, not plak's
protocol: JSON frames with serde, without the bootstrap, `Delivery`
verdicts or seccomp. This keeps the child numbers comparable with the other
two modes. `mm-cell-worker`'s seccomp filter and protocol overheads are not
measured here (open question 5).

### Wasmtime configuration (chosen)

| Setting | Choice | Why [M unless noted] |
|---|---|---|
| Interruption | **epoch**, a 10 ms ticker thread, a deadline callback that checks a per-call budget and an interrupt flag | The only way to stop a call that sits inside one native operation in-process: 0.16 ms against 7.2 s for park/fuel. Costs 2.04× native against 1.68× without (+21%). |
| Wasmtime fuel | **off** | 2.15× native (+28% over plain), and it traps where epoch also traps. The engine's own fuel already gives deterministic, non-fatal budgets (`Parked{Fuel}`). |
| `max_wasm_stack` | **4 MiB** (also `async_stack_size` = 5 MiB, which wasmtime 50 validates even for sync calls) | The 512 KiB default traps on both stack cases; 1 MiB passes both. 4 MiB leaves margin. |
| Memory limit | `StoreLimits::memory_size` at **2× `Limits::memory` or more** (512 MiB for a 256 MiB session in the harness) | Linear memory holds the arena plus what sits outside it (conversion, compiler, snapshot buffers). With 256 MiB around a 64 MiB session, the out-of-arena case trapped at 171 MiB host peak. |
| Allocator | **on-demand** (pooling off) | Pooling did not change instantiate + session time (175 µs against 179 µs), and it reserves the pool's address space up front. Revisit only for very high instance churn. |
| Modules | **precompiled** `.cwasm` (`Engine::precompile_module`, `Module::deserialize_file`) | Loads in 0.40 ms against 320 ms to compile. A `.cwasm` is tied to the wasmtime version and config, so build it with the daemon. |
| WASI | p1 with an empty `WasiCtx` (no preopens, no inherited stdio, no env) | The engine needs only clocks and random numbers. A narrower linker would remove the rest. |

## Performance [M]

### Startup

| Cold, new process | main → first cell done | module load | spawn → exit |
|---|---|---|---|
| native | 0.26 ms | — | 2.15 ms |
| wasm, precompiled | 1.47 ms | 0.40 ms | 3.88 ms |
| wasm, compiled at start | 321 ms | 320 ms | 334 ms |
| child | 2.06 ms | — | 4.03 ms |

| Warm | session ready (median) | first cell done (median / p99) |
|---|---|---|
| native: new session | 41 µs | 44 / 97 µs |
| wasm: new instance + session | 179 µs | 196 / 277 µs |
| wasm-pool: new instance + session | 175 µs | 192 / 268 µs |
| child: new worker + session | 1,790 µs | 1,941 / 3,440 µs |
| native: another session in an existing container | — | 92 / 166 µs |
| wasm: another session in an existing instance | — | 223 / 339 µs |
| child: another session in an existing worker | — | 162 / 257 µs |

The second "native" row is slower than the first because it includes dropping
the previous session's heap.

### Host calls

The round trip is a loop of 20,000 sync host calls minus the same loop calling
a Python function. The codemode cell (`drive.rs::CODEMODE`) makes 200 sync
calls whose JSON results are parsed with `json.loads`, then 10
`asyncio.gather` batches of 20 async calls, in `Waiting`/`Batch` style.

| Mode | host call round trip | codemode cell (400 calls) | per call in the cell |
|---|---|---|---|
| native | 0.17 µs | 1.97 ms | 4.9 µs |
| wasm | 2.45 µs | 5.47 ms | 13.7 µs |
| child | 52.0 µs | 25.96 ms | 64.9 µs |

The wasm overhead is JSON encoding through linear memory plus the call. The
child overhead is two pipe writes, two thread wakeups and a channel hop per
call. On this VM a pipe round trip is expensive; bare-metal numbers should be
lower [E]. Against real tool latencies (milliseconds) all three are small;
for cells with thousands of cheap calls only native and wasm are invisible.

### Bench programs (ms per run, median of 5, fresh session each)

| program | native | wasm (epoch) | wasm (plain) | wasm (fuel) | child |
|---|---|---|---|---|---|
| fib | 25.7 | 56.4 | 49.6 | 54.4 | 30.3 |
| loop | 79.6 | 150 | 115 | 155 | 70.6 |
| dict | 157 | 245 | 209 | 248 | 127 |
| str | 50.8 | 91.4 | 85.9 | 96.1 | 51.1 |
| json_rows | 112 | 245 | 211 | 256 | 109 |
| closure | 49.8 | 106 | 81.8 | 107 | 45.2 |
| classes | 144 | 335 | 252 | 364 | 147 |
| generators | 286 | 618 | 508 | 669 | 287 |
| exceptions | 98.3 | 213 | 176 | 232 | 100 |
| fmt | 234 | 456 | 389 | 511 | 235 |
| sort | 181 | 370 | 292 | 388 | 167 |
| interp | 269 | 607 | 473 | 647 | 264 |
| **geomean vs native** | 1.00× | 2.04× | 1.68× | 2.15× | 0.97× |

The guest is a plain release build without `wasm-opt`. Spec §6 measured
`wasm-opt -Oz` at 8–13% slower on the dispatch-heavy programs [S]. The child
is not faster than native in any meaningful sense: native runs these on the
harness's main thread and the child on its own, and the 3% is within the
spread of a VM run.

### Snapshots

| Parked state | bytes | native save / restore | wasm save / restore | child save / restore |
|---|---|---|---|---|
| codemode cell at call 100 | 56,049 | 62 / 111 µs | 621 / 699 µs | 598 / 2,095 µs |
| 10k small dicts at a host call | 1,891,618 | 1.9 / 0.9 ms | 5.2 / 3.6 ms | 5.7 / 5.8 ms |
| 100k small dicts at a host call | 17,999,811 | 25.5 / 19.7 ms | 67.5 / 48.6 ms | 71.1 / 45.0 ms |

The wasm and child times include moving the image out of the container
(linear-memory copy, pipe). Sizes are identical across modes because the
image is the same. Cross-mode restore (save in one, restore and finish in
another): all 9 pairs resumed, output identical. The build ids agree
because all three builds enable the same features (spec §10.1, sdk.md
"Cross-target restore").

### Memory per session (100 sessions, after warm-up)

| Mode | idle (`x = 1`) | active (1,000 dicts, codemode cell stopped at call 100) |
|---|---|---|
| native: host RSS Δ | 107 KiB | 570 KiB |
| wasm: host RSS Δ | 450 KiB (linear memory 1,664 KiB, mostly untouched) | 1,558 KiB (linear 2,560 KiB) |
| child: worker RSS / PSS | 1,060 / 393 KiB | 3,448 / 1,036 KiB |

Child PSS counts shared library and binary pages once across workers. The
host's own cost per child is ≈0.

### Many sessions on one host

N sessions, codemode cells spread over min(N, 16) threads (256 cells, or one
per session at 1,024):

| Mode | 1 | 16 | 64 | 256 | 1,024 sessions |
|---|---|---|---|---|---|
| native cells/s | 482 | 3,288 | 2,674 | 1,414 | 1,502 |
| native p99 cell | 2.8 ms | 12.0 ms | 15.2 ms | 17.6 ms | 16.9 ms |
| native host RSS | 6 MiB | 23 MiB | 61 MiB | 212 MiB | 822 MiB |
| wasm cells/s | 180 | 1,435 | 1,790 | 1,506 | 1,819 |
| wasm p99 cell | 6.0 ms | 14.5 ms | 12.8 ms | 13.5 ms | 14.4 ms |
| wasm host RSS | 160 MiB | 163 MiB | 240 MiB | 544 MiB | 1,761 MiB |
| child cells/s | 39 | 707 | 496 | 681 | 533 |
| child p99 cell | 28 ms | 35 ms | 39 ms | 37 ms | 38 ms |
| child workers' PSS sum | 3.5 MiB | 22 MiB | 77 MiB | 294 MiB | 1,165 MiB |

No errors in any mode at 1,024 sessions. Native's RSS at 1,024 (≈0.8 MiB per
session, each having run one codemode cell) and wasm's (≈1.6 MiB) are what
one daemon would hold. Child's host side stays at 132 MiB, mostly the
harness's thread stacks.

## Fault matrix [M]

*This table is the state at 32125ff (the same at 58d7aca). The rows that
changed at ac10b28 are in "Rows the robustness thread changed" below.*

Each row is one process. It holds a **bystander** session stopped at a host
call in the codemode cell. A **victim** session in the same mode meets the
fault. Then the harness checks the victim's session (`print(2)`), resumes
the bystander to its end and starts a **fresh** session. "host" is the
harness process, which stands for plak's daemon. `faults.jsonl` has the full
record of every row.

| Fault | native (in-process) | wasmtime (in-process) | child process |
|---|---|---|---|
| **Rust stack: cyclic dict `a == b`** | 256 KiB and 512 KiB threads: **host aborts** (SIGABRT, "stack overflow"). 1, 2 and 8 MiB: `RecursionError`, all fine | `max_wasm_stack` 512 KiB (the default): **instance traps** ("call stack exhausted"); host, bystander and fresh session fine. 1 and 4 MiB: `RecursionError` | 256 KiB and 512 KiB worker stacks: **worker aborts**; host and bystander fine. 16 MiB: `RecursionError` |
| **Rust stack: 600-deep frozenset `==`** | 256 KiB: **host aborts**. 512 KiB and up: `True` (CPython: `True`) | 512 KiB: **instance traps**. 1 MiB and up: `True` | 256 KiB: **worker aborts**. 512 KiB and up: `True` |
| **Engine panic** (injected at `start`) | release (`panic=abort`): **host aborts**. Shipped `immediate-abort`: **host dies on SIGILL**, without a message. `release-unwind`: the victim gets `SystemError: … the session is poisoned` and every later call errors; bystander and fresh session fine | release and shipped guests both trap (`unreachable`); instance lost, host and others fine | abort and shipped workers die (SIGABRT, SIGILL); unwind worker poisons the session and keeps serving; host fine |
| **Allocation outside arena accounting** (64 MiB session; `[0] * 7_000_000` as the completion value, or passed to a host call) | arena check passes (56 MB list); host peak RSS **487 MiB** (completion) and **645 MiB** (host-call argument). Nothing bounds it: a larger list is a host OOM | store limit 256 MiB: **instance traps** (allocation failure → abort → `unreachable`) at 171 / 117 MiB host peak; others fine | no rlimit: completes (the host-call argument then costs the *host* 443 MiB to decode the 7M-element frame). `RLIMIT_AS` 256 MiB: **worker aborts** on allocation failure; host at 2 MiB |
| **Arena memory limit** (64 MiB, appending 10 KB strings) | `MemoryError`, session reusable | same | same |
| **Runaway Python loop** | engine fuel 50M: `Parked{Fuel}` at 345 ms; then cancel, session reusable. 10k-fuel slices + host flag: stops 0.063 ms after the flag. Park request: 0.066 ms | fuel: parks at 588 ms; slices: 0.043 ms; epoch interrupt: 0.099 ms, **but traps the instance**; epoch budget (20 ticks): traps at 209 ms; wasmtime fuel 2e9: traps at 234 ms | fuel: 314 ms; slices: 0.040 ms; park frame: 0.192 ms; SIGKILL: worker gone 0.46 ms later; `RLIMIT_CPU` 1 s: killed at 998 ms |
| **Runaway single native call** (`7**4_000_000` squared, ≈7 s) | fuel, slices and park all return only when the multiply ends: **7.2 s** after the stop. A host can neither interrupt nor abandon the thread | engine fuel and slices: 16 s; **epoch: 0.16 ms** (trap) | engine paths: 7.2 s; **SIGKILL: 0.75 ms**; `RLIMIT_CPU`: 1.0 s |
| **Cancellation latency** (from the stop to control back in the host) | 0.06–0.07 ms if the cell is at safepoints; unbounded inside one native call | 0.04 ms (slices) non-fatal; 0.1–0.16 ms with an epoch trap, even inside native calls | 0.04–0.19 ms non-fatal; 0.5–0.8 ms by SIGKILL, even inside native calls |
| **Bystander session in every row** | lost whenever the host died; otherwise completed | always completed | always completed |
| **Snapshots between modes** | 9/9 save→restore pairs across native, wasm and child resumed with identical output | | |

### OS-level controls

| Control | native | wasmtime | child |
|---|---|---|---|
| rlimits (`AS`, `CPU`, `NOFILE`) | process-wide, so they would kill the daemon, not one session | process-wide; wasmtime's `StoreLimits` (memory, instances, tables) give the per-session equivalent of `RLIMIT_AS`, and epoch/fuel that of `RLIMIT_CPU` | per worker, measured working: `RLIMIT_AS` makes the worker abort on allocation failure, `RLIMIT_CPU` kills at the limit |
| seccomp | per thread in Linux, but the thread shares the daemon's memory and file descriptors, so a filter protects nothing the engine could not already reach | not needed for the guest (its only syscalls are the WASI functions linked in); applies to the daemon as a whole | per worker; `mm-cell-worker` installs a filter allowing only pipe I/O, memory mapping and exit, tested to kill on a forbidden syscall [S: `crates/mm-cell-worker/src/seccomp.rs`, `tests/protocol.rs`] |
| cgroups (memory.max, cpu.max, pids.max) | daemon only | daemon only | per worker or per pool of workers [E, not measured] |
| Kill | none: a Rust thread cannot be stopped from outside | epoch interrupt (instance lost) | SIGKILL |

### Rows the robustness thread changed (rerun at ac10b28)

The robustness thread (T-01a1a9e5-58ef-77b6-acce-60185cc80e65) landed as
`b442a99..ac10b28`. Its changes are: comparison walks on an explicit stack,
`HostValue` conversion charged to the session's memory limit and made
iterative, big-int operators that park on fuel before they start, and a
smaller startup heap. The whole matrix (104 rows, now including the
[mm-wasm](wasm-embedding.md) modes) was run at `58d7aca`, just before it
landed, and again at `ac10b28`.
`results/2026-11-05-wasm-abi/faults-diff.md` lists every changed row
(`diff.py`) [M]:

| Rows | 58d7aca | ac10b28 |
|---|---|---|
| Stack, cyclic dict: native 256/512 KiB threads | **host aborts** | `RecursionError`, host and bystander fine |
| Stack, 600-deep frozenset: native 256 KiB | **host aborts** | `True` |
| Stack, both shapes: wasm, abi, abi-uu at 512 KiB `max_wasm_stack` | instance traps | `RecursionError` / `True` |
| Stack, both shapes: child 256/512 KiB worker stack | worker aborts | `RecursionError` / `True` |
| Out-of-arena, completion value, native | completes, host peak **487 MiB** | completes, host peak 114 MiB |
| Out-of-arena, host-call argument, native | host call delivered, host peak **645 MiB** | `MemoryError` in the cell, host peak 111 MiB |
| Out-of-arena, host-call argument, child without rlimit | the host spends 443 MiB decoding the frame | `MemoryError` in the worker, host 2 MiB |
| Out-of-arena, both shapes: wasm, abi, abi-uu (256 MiB store) and child with `RLIMIT_AS` | instance trap / worker abort | completion fits; argument raises `MemoryError`; session reusable |
| Runaway bigint op, engine fuel 50M, every mode | parks after the op: 7.4 s native, 16 s wasm | **parks before the op: 0.04–0.19 ms** |
| Runaway bigint op, 10k-fuel slices + host flag | 7.1 s native, 16 s wasm | 2.8 s native, 6.4 s wasm (one refill runs the shorter `pow` before the multiply parks) |
| Runaway bigint op, park request (native, child) | 7.3 s | unchanged, 7.3 s: a park request does not stop an operation in progress |

Nothing else changed: panics, arena limits, loop runaways, kills, rlimits
and snapshot moves behave as before. As the earlier version of this note
predicted, every stack row now passes in every mode and at every stack size
measured. The 256 KiB rows show the native walk no longer depends on the
thread's stack.

**What this changes in the recommendation:**
- The stack condition for native in-process is met by the engine: no
  measured shape needs more than 256 KiB. Keep a generous thread stack
  anyway (2 MiB or more) as headroom for native recursion not yet found.
- The conversion hole (open question 1) is closed: a 64 MiB session no
  longer takes a native host past about 115 MiB on these shapes.
- Long native operations are bounded by fuel now, if the host gives fuel:
  budgeted cells park before an operation they cannot pay for. A host that
  stops cells only with park requests or deadlines (unlimited fuel) is still
  exposed for as long as one operation runs. The robustness thread lists
  that as an owner decision.

Bare-metal Linux and Apple Silicon macOS give the same outcomes at
a051ae6, apart from six OS-dependent rows ("Fault matrix: macOS against
Linux" below).

## What each fault costs, by mode

| | native | wasmtime | child |
|---|---|---|---|
| Engine bug that overflows the Rust stack | daemon lost | instance lost | worker lost |
| Engine panic | unwind build: session poisoned; abort builds: daemon lost | instance lost | session poisoned (unwind) or worker lost |
| Out-of-arena allocation | daemon OOM risk | instance lost at the store limit | worker lost at `RLIMIT_AS`/cgroup |
| Long native op | thread stuck until it ends | interrupted by epoch (instance lost) | killed |
| Recovering a lost container | — | new instance (0.2 ms) + restore of the last snapshot | new worker (2 ms) + restore |

## Open questions

1. ~~**Charge `HostValue` conversion to the session.**~~ Done at ac10b28. `Vm::value_to_host` and
   host-call argument conversion build the whole tree on the Rust heap.
   `HostArgs::max_bytes` (16 MiB by default) counts encoded bytes, while the
   tree costs about 70–90 bytes per element (487 MiB for 7M ints [M]). Charging the
   tree (or bounding elements) against `Limits::memory` would close the
   largest in-process hole found. The completion value matters only when
   `set_last_expr_value(true)`, which `mm-cell-worker` uses.
2. **Bounded native operations.** A single bigint multiply or `pow` runs for
   seconds past fuel, deadline and park (spec §11 "Responsiveness" [S]). In
   process, only wasmtime can stop it. Bounding operand sizes or making the
   long natives resumable would let native in-process meet a latency target.
3. ~~**wasm stop without a trap.**~~ Prototyped in
   [wasm-embedding.md](wasm-embedding.md): the guest polls a host import at
   each safepoint check (0.03 ms, instance alive). Originally: the engine's park flag lives in linear
   memory, so a synchronous wasmtime embedder cannot set it from another
   thread. A guest export that returns the flag's address, written through
   shared memory, or an epoch callback that yields (async wasmtime), would
   give a non-fatal async park. Today the non-fatal wasm stop is fuel slices.
4. **Async integration with plak.** These measurements drive sessions from
   OS threads. Under tokio, native means `spawn_blocking` with 2 MiB stacks
   by default, or the facade's `run_async` with 10k-fuel slices. wasmtime has
   its own async path with fibers and the `async_stack_size` coupling. Both
   need measuring inside the runner.
5. **`mm-cell-worker` itself.** Its seccomp filter, serde frames and the
   py-ptc bootstrap are not in these child numbers. Its host-call latency
   should be close to `iso-worker`'s [E], but this was not measured.
6. **Bare-metal numbers.** Pipe round trips (52 µs) and process spawn are
   expensive in this VM. Measured on 2026-11-05 on bare-metal Linux (10.5 µs
   per call, 0.83 ms per new worker) and on Apple Silicon macOS (9.6 µs,
   2.2 ms); see "Apple Silicon macOS and bare-metal Linux" below.
7. ~~**`wasm32-unknown-unknown`**~~ Embedded through mm-wasm v1
   ([wasm-embedding.md](wasm-embedding.md)): performance and faults as
   wasip1, with the host supplying the clock and entropy as imports.

## Apple Silicon macOS and bare-metal Linux (2026-11-05)

The same harness and workloads, run on two more hosts:

- **mac**: MacBook Pro, M1 Max (8 performance and 2 efficiency cores),
  32 GiB, macOS 27.2 (26B123), on AC power.
- **host1**: AMD Ryzen 9 8945HS (Zen 4, 8 cores, 16 threads), 28 GiB,
  Ubuntu 24.04.5 on Linux 7.0, bare metal, `performance` governor.

Both ran Rust 1.99.0 and wasmtime 50 at a051ae6 plus the harness's macOS
port (the commit that adds this section; it changes no engine code). The
orb column is `results/2026-11-05-wasm-abi/after-ac10b28/`, which has the
same engine, so all three columns compare directly. **mac against host1
shows platform effects; host1 against the orb shows what the VM costs.**
The raw data, machine details and what was ported are in
[`results/2026-11-05-m1max-aarch64-macos/`](../../bench/evidence/isolation/results/2026-11-05-m1max-aarch64-macos/)
and
[`results/2026-11-05-ryzen-x86_64-linux/`](../../bench/evidence/isolation/results/2026-11-05-ryzen-x86_64-linux/).
Each directory also has the same run at 58d7aca and at ac10b28
(`at-58d7aca/`, `at-ac10b28/`). Every number in this section is [M] unless
marked otherwise.

### The engine on aarch64 macOS

`cargo xtask check` passes on the Mac and on host1 (gate by gate in the
macOS results README). It needed no engine change, only five tooling fixes:

- a clippy allow on `mm-cell-worker`'s seccomp stub for platforms other than
  Linux;
- `TZ=UTC` in the own-case oracle test, as `oracle.py` already does for the
  corpus;
- six own cases not compared against CPython on macOS, where CPython's
  output comes from Apple's libc and libm, and a larger oracle stack for a
  seventh;
- `/usr/bin/false` in xtask's bench tests, because macOS has no
  `/bin/false`;
- the native size budget enforced on x86-64 Linux only (owner decision).
  Native `hello` on aarch64 macOS is 1,806,784 B and is reported, not gated
  (2,012,704 B on x86-64 Linux; `hello.wasm` is 1,922,566 B on both).

All 173 own cases give byte-identical mm output on both architectures. A
snapshot taken on either one resumes on the other: 346 of 346 chains that
alternate between the Mac and host1 matched, across 2,000 restores between
architectures (at 58d7aca and again at ac10b28).

### Comparison

`abi` is mm-wasm v1 on wasm32-wasip1 ([wasm-embedding.md](wasm-embedding.md)).

| | orb (VM) | host1 (bare metal) | mac |
|---|---|---|---|
| **Cold start**, main → first cell: native / wasm precompiled / abi / child | 0.29 / 1.45 / 1.19 / 2.00 ms | 0.17 / 0.82 / 0.81 / 0.98 ms | 0.15 / 1.80 / 1.74 / 2.42 ms |
| Cold, spawn → exit: native / wasm / child | 2.11 / 3.76 / 4.02 ms | 1.15 / 1.87 / 1.97 ms | 2.94 / 5.15 / 6.10 ms |
| wasm module: precompiled load / compile at start | 0.37 / 284 ms | 0.32 / 179 ms | 1.08 / 220 ms |
| **Warm session creation**, to session ready: native / wasm / abi / **child (new worker)** | 39 / 158 / 167 / 1,784 µs | 23 / 107 / 106 / 789 µs | 30 / 96 / 94 / 2,208 µs |
| Child, first cell p99 with a new worker | 2,644 µs | 1,085 µs | 3,693 µs |
| Another session in an existing worker | 150 µs | 45 µs | 55 µs |
| **Host-call round trip**: native / wasm / abi / **child (pipe)** | 0.22 / 2.54 / 1.02 / 54.3 µs | 0.13 / 1.45 / 0.56 / 10.4 µs | 0.14 / 1.37 / 0.60 / 9.23 µs |
| **Codemode cell, 400 calls**: native / wasm / abi / child | 1.99 / 5.53 / 4.42 / 28.0 ms | 1.07 / 2.86 / 2.33 / 5.73 ms | 1.17 / 2.88 / 2.42 / 5.89 ms |
| **Bench throughput**, native geomean over the 12 programs, time relative to the orb | 1.00 | 0.54 (1.86× faster) | 0.58 (1.72× faster) |
| Bench, wasm (epoch) / abi / child relative to native on the same host | 2.08× / 2.10× / 0.99× | 2.05× / 2.05× / 1.02× | 1.95× / 1.98× / 1.02× |
| **Snapshot**, codemode at call 100 (56 KB), native save / restore | 58.5 / 115 µs | 33.5 / 64.2 µs | 45.0 / 82.5 µs |
| Snapshot, 100k dicts (18 MB): native / wasm / child save | 26.6 / 65.0 / 73.6 ms | 14.4 / 43.0 / 45.1 ms | 10.9 / 22.4 / 19.9 ms |
| Restore, 100k dicts: native / wasm / child | 21.6 / 48.7 / 50.1 ms | 11.4 / 29.4 / 25.0 ms | 6.61 / 14.3 / 15.0 ms |
| **Memory per session**, host RSS Δ, idle / active: native | 74 / 386 KiB | 70 / 385 KiB | 86 / 384 KiB |
| wasm | 385 / 1,331 KiB | 393 / 1,333 KiB | 368 / 1,217 KiB |
| child worker, PSS (Linux) or phys_footprint (macOS), idle / active | 333 / 805 KiB | 337 / 809 KiB | 1,440 / 2,704 KiB |
| **1,024 concurrent sessions**, threads | 16 | 16 | 10 |
| native: cells/s, p99, host RSS | 1,373, 19.2 ms, 603 MiB | 3,269, 6.7 ms, 606 MiB | 4,170, 12.1 ms, 543 MiB |
| wasm: cells/s, p99, host RSS | 1,859, 13.2 ms, 1,521 MiB | 2,369, 7.0 ms, 1,507 MiB | 1,892, 19.3 ms, 1,860 MiB¹ |
| abi: cells/s, p99, host RSS | 1,557, 14.0 ms, 1,584 MiB | 2,785, 6.0 ms, 1,575 MiB | 2,190, 15.6 ms, 1,929 MiB¹ |
| **child**: cells/s, p99, workers' PSS or footprint | 431, 46.6 ms, 881 MiB | 1,177, 18.6 ms, 893 MiB | 272, 68.2 ms, 2,848 MiB |
| Errors at 1,024 (all modes) | 0 | 0 | 0 |

¹ The scale section runs native, then wasm, abi and child in one process.
macOS keeps freed malloc pages resident, so the Mac's host-RSS figures
after the native phase include about 540 MiB it left behind.

### What the comparison says

- **The VM cost about 1.9× on CPU, 2.3× on spawning a worker and 5× on each
  pipe round trip.** Against host1, the orb ran the bench programs 1.80–1.90×
  slower in every mode. A new worker took 1,784 µs against 789 µs, and a
  child host call 54 µs against 10.4 µs. On bare metal the child's 400-call
  codemode cell costs 5.7 ms rather than 28 ms, so most of the child mode's
  boundary cost was the VM. This answers open question 6 for Linux: on bare
  metal plak pays about 10 µs per call and under 1 ms per new worker.
- **On the Mac, process creation is expensive and pipes are cheap.** A new
  worker takes 2.2 ms (2.8× host1, and more than the orb), with a p99 of
  3.7 ms. Even the native harness's spawn → exit takes 2.9 ms. A child host
  call costs 9.2 µs, level with bare-metal Linux.
- **Child mode does not scale with concurrency on macOS.** With 16 or more
  concurrent workers the Mac stays at about 300 cells/s (272–336 over four
  runs, at every N from 16 to 1,024). host1 reaches 1,070–1,360, and a
  single worker runs at the same rate on both (170 against 177 cells/s). The
  likely cause is XNU's pipe wakeups and its scheduling across many
  processes on 10 cores [E, not profiled]. The only hint so far: one rerun
  of the whole scale section took 36.6 s of system time against 26.4 s of
  user time. The in-process modes do scale on the Mac: at 1,024 sessions
  native runs 4,170 cells/s, abi 2,190 and wasm 1,892.
- **CPU throughput is close between the platforms.** The M1 Max is 3–9%
  slower than the Zen 4 laptop part, depending on the mode, and the modes
  keep the same ratios on both hosts: wasm and abi about 2× native, child
  equal to native. The exception is bigint multiplication: the
  `7**4_000_000` squaring runs 7.3 s on the Mac against 3.9 s on Zen 4
  (7.3 s on the orb).
- **Large snapshots are faster on the Mac**: 1.3–2.3× faster than host1 for
  the 18 MB image, saving and restoring, in every mode. The 56 KB codemode
  image is a little slower (45 / 83 µs against 34 / 64 µs). The images have
  the same sizes on every host.
- **Per-session memory is a property of the engine, not the host.** Native
  (70–86 KiB idle, about 385 KiB active) and wasm (1.2–1.3 MiB active)
  agree on all three hosts within 25%. The Mac's child figure is
  phys_footprint, which is not PSS and should not be compared with it
  (macOS README).
- `wasm-opt -Oz` output differs by a few bytes between hosts (the
  wasip1 guest: 1,926,729 B on the Mac, 1,926,733 B on host1, 1,926,756 B
  on the orb), although the input module is identical. Size budgets with a
  margin are unaffected.

### Fault matrix: macOS against Linux

The orb and host1 agree on all 104 rows. The Mac agrees on 98 of them. The
other six depend on the OS:

| Row | Linux (orb, host1) | macOS |
|---|---|---|
| Panic in shipped `immediate-abort` builds (native, child) | SIGILL | SIGTRAP (`brk` rather than `ud2`); still fatal to the process |
| `RLIMIT_CPU` 1 s, child (loop, native call) | SIGKILL at about 1.0 s | SIGXCPU at 1.0 s; just as effective |
| **`RLIMIT_AS` 256 MiB**, child (both out-of-arena rows) | the worker runs capped (completes; `MemoryError`) | **refused**: `setrlimit(RLIMIT_AS)` returns EINVAL, so the worker cannot be started with the cap. `RLIMIT_DATA` is refused too, and macOS has no cgroups |

Stopping a runaway cell has the same outcome on every host, and every row
that has a measured time to regain control shows Linux-like latency on the
Mac. In a Python loop, fuel slices and park requests take 0.007–0.11 ms and
SIGKILL 0.55 ms. mm-wasm v1's park without a trap takes 0.02 ms in a loop,
and 102–112 ms inside a native call (its 100 ms grace period, then the epoch
trap). The one outlier is the plain wasmtime epoch interrupt: 0.40–0.46 ms
on the Mac against 0.04 ms on host1. The epoch budget rows fire at
277–278 ms against 210 ms. This is consistent with coarser timer wakeups
for the 10 ms ticker thread on macOS [E].

At 58d7aca the Mac also differed on the cyclic-dict stack rows at 512 KiB
(`RecursionError` where Linux aborted, because aarch64 frames are smaller).
That difference disappeared with the iterative comparison walk.

### What it means for the recommendation

The owner's default, native in-process, behaves on macOS as it does on
Linux, and it is the fastest mode there at every concurrency measured. The
wasmtime and mm-wasm options carry over too, including `StoreLimits` for
memory and park without a trap.

**The child process loses most of its case on macOS.** There is no
`RLIMIT_AS`, no cgroups and no seccomp, which leaves `RLIMIT_CPU` and
SIGKILL. It also pays 2.2 ms per new worker and stalls at about 300 cells/s
with many workers. For a hard fault boundary on Apple Silicon, wasmtime
in-process is the better choice. A macOS sandbox profile (Seatbelt,
`sandbox_init`) would be the analogue of seccomp for workers; it is not
measured here [E].

## The mm-wasm v1 head on the Mac and host1 (2026-11-05)

Measured after the host API, the stdlib round and the mm-wasm v1 freeze
(`5568b27`). Every number is [M], from the machine named.

- **Gates on the Mac (aarch64 macOS 27.2).** `cargo xtask check` passes in
  full, including durability (1,180 of 1,180 runs, 107,752 parks) and
  cross-target durability (366 of 366, native ↔ wasm32-wasip1). The new
  `cargo xtask wasm-host` gate also passes. It covers both guests, and
  `tests/parity.rs`'s two tests pass on each guest.
- **Bench.** Fresh `cargo xtask bench` runs are in
  `bench/results/2026-11-05-host1-ryzen-x86_64-linux/` and
  `bench/results/2026-11-05-mac-m1max-aarch64-macos/`; the orb results are
  unchanged. The geomean of mm/CPython is 1.11 on the orb (2026-11-04),
  1.09 on host1 and 0.92 on the Mac.
- **40 concurrent sessions on the Mac.** See
  [`results/2026-11-05-soak-40-sessions/`](../../bench/evidence/isolation/results/2026-11-05-soak-40-sessions/).
  There were no errors over 6.7 M codemode cells, and 40 loaded sessions
  take 38–58 MiB of RSS in total. At 100 ms think time the cell p99 is
  1.0 ms (native) and 1.6 ms (mm-wasm).
- **Snapshot chains across machines and engines.** See
  [`results/2026-11-05-crossarch-v1/`](../../bench/evidence/isolation/results/2026-11-05-crossarch-v1/).
  Every hop is a new process, alternating between the Mac and host1. The
  host-API scenario (host module and exception class, structured host
  errors, the call log, a hint, zoneinfo zones without the source after
  restore) gives identical results in 18 of 18 chains. Those are the
  native, wasip1-guest and unknown-unknown-guest engines alone, and all six
  rotations of the three, starting on either machine: 252 restores of an
  image written on the other machine.
  The 183 own cases through `mm exec`/`mm resume` are also identical in
  366 of 366 chains for each of native, wasip1 and a mixed native/wasip1
  rotation: 2,132 cross-machine restores per row.
  One gap was found: `fuel_wanted()` read 0 after a restore (finding X1
  there). It is fixed in 220a05f, which saves the value in the image, and
  the chains read it correctly at 874409b on all three engines.
- **Later heads on the Mac (874409b, c97ce06).** `cargo xtask gc-stress`
  passes on aarch64 at 874409b, with 390 of 390 durability runs identical
  under stress; see
  [`results/2026-11-05-gc-stress-aarch64/`](../../bench/evidence/isolation/results/2026-11-05-gc-stress-aarch64/).
  At c97ce06, `cargo xtask check` and `cargo xtask wasm-host` pass, and
  parity passes on both guests. The 40-session soak at both heads is in
  [`results/2026-11-06-soak-40-sessions-final/`](../../bench/evidence/isolation/results/2026-11-06-soak-40-sessions-final/).
  It ran 9.19 M cells with no errors. Native is unchanged. An mm-wasm
  instance's idle RSS rose from about 320 KiB at 5568b27 to about 790 KiB,
  which is not root-caused. On the Mac, bench at c97ce06 gives a geomean of
  0.855 for mm/CPython, against 0.923 at 5568b27. The interleaved A/B shows
  `closure` about 3% faster on aarch64 (`bench/results/2026-11-06-closure-ab/`).
- **Final head 5349520, both hosts.**
  - **Mac.** `cargo xtask check` and `cargo xtask wasm-host` pass, and
    parity passes on both guests [M]. Since then, the idle RSS growth was
    root-caused and fixed in the host (b057751, 2fa2e3e, 239b26d). The
    40-session soak at 5349520 gives 328–330 KiB idle per mm-wasm instance
    and a runtime baseline of 8.8–8.9 MiB, against 8 MiB at 5568b27, with
    no errors [M].
  - **Bench.** mm/CPython geomean is 0.840 on the Mac and 1.008 on host1
    (pinned Zen 4, against 1.090 at 06f1fbc) [M]. On host1, mm runs at
    0.922× of its 06f1fbc time.
  - **`closure`.** It is unchanged on host1 (1.002 against e6d7251) and 2%
    faster on the Mac, so the orb's slowdown is specific to the orb [M].
  - **Results.** See
    `bench/results/2026-11-06-{mac-m1max-aarch64-macos,host1-ryzen-x86_64-linux}-5349520/`.

## How to rerun

```bash
# All builds and the full run (≈15 min with the builds cached; first build ≈10 min):
bash bench/evidence/isolation/run.sh bench/evidence/isolation/results/$(date +%F) all
# Only the fault matrix, or only performance:
bash bench/evidence/isolation/run.sh OUT faults
bash bench/evidence/isolation/run.sh OUT perf
# One section or one fault, after a run has built everything:
target/isolation/release/iso-harness perf hostcall
target/isolation/release/iso-harness fault wasm stack-cyclic-dict max_wasm_stack_kib=512
python3 bench/evidence/isolation/faults.py OUT 'panic'     # rows whose label contains 'panic'
```

`run.sh` builds `iso-harness`/`iso-worker` in release (`panic=abort`, the
repository's profile) and `release-unwind`, `iso-guest` for wasm32-wasip1,
and shipped builds (std rebuilt with `-Cpanic=immediate-abort`, as
`cargo xtask size`) of the worker, the native-only harness and the guest. It
records the commit, toolchain and every binary's SHA-256 in `build.txt`. The
harness is its own Cargo workspace (`bench/evidence/isolation/Cargo.toml`,
with its own `Cargo.lock`) building into `target/isolation*`. wasmtime, serde
and the harness therefore never enter the engine crates, the root lockfile,
the default build or `cargo xtask size`. The harness links the facade with
`test-hooks` for panic injection. Facade features are not among the build
id's ingredients [S: `micro_monty::build_id`], so the harness's snapshots
are those of a plain build with the same stdlib features.
