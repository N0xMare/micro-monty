# micro-monty against MicroPython and PocketPy

How does micro-monty compare with the two other small embeddable Python
engines, as an engine for sandboxed, model-written code-mode cells? This
study measures MicroPython 1.29.0 and PocketPy 2.2.0 (both the latest release
on 2026-11-09) next to mm on capability, embedding and host calls, control,
fault tolerance, durability, speed, memory and size, and GC pauses, where
Monty 1.5.0 joins them (§9). It reuses the JS study's
harness ([js-engines-compare.md](js-engines-compare.md)): every engine runs
behind a small host with the same CLI and answers the same recorded tool
results.

Numbers are labelled [M] measured, [S] sourced or [E] estimated. Unless a line
says otherwise, they are [M] on one 16-vCPU Amp orb (Intel Xeon @ 2.60 GHz,
Linux 6.1). Timed figures are medians over 5 rounds, with each process pinned
with `taskset -c 2` and the engine order rotated every round. mm is at
`eb78933` (the 0.0.1 RC rerun, 2026-11-11); the first run, at `3624a3d`, is
`results/2026-11-09`, and "Rerun at eb78933" below lists what moved. The
other engines are pinned, so the rerun measured mm and the GC pauses of all
four, and copied the rest of MicroPython's and PocketPy's records from
`results/2026-11-09` (same orb, same builds; `reused-from.txt`). Raw data,
harness and full tables:
[`bench/evidence/python-engines/`](../../bench/evidence/python-engines/)
(`results/2026-11-11/summary.md`, `raw.jsonl`, the per-case `*.jsonl`).
`bench/evidence/python-engines/rerun.sh` rebuilds and reruns everything;
`rerun.sh --engines mm --reuse results/2026-11-11 --out results/<date>` reruns
mm alone in about 10 minutes [M].

Each engine was built in its best configuration for this job (evidence
README "Builds"). MicroPython uses the unix port's standard feature level
without the modules that reach outside a sandbox (ffi, sockets, ssl, `os.system`,
`machine`), a 256 MiB heap, and its pure-Python stdlib frozen in. The CLI runs
also get every micropython-lib stdlib module on its path. PocketPy uses
upstream's release flags, with `PK_ENABLE_OS=0` and its watchdog compiled in.

## Summary

| [M] unless marked | micro-monty | MicroPython 1.29.0 | PocketPy 2.2.0 |
|---|---:|---:|---:|
| Native binary, bytes (gzip -9) | 2,409,464 (1,409,980) | **547,688** (238,373) | **462,088** (232,304) |
| Wasm, bytes (gzip -9) | 2,203,338 (1,083,582) | **449,553** (197,274), upstream emscripten build | **451,815** (192,262) |
| Monty corpus, 568 CPython cases passed | **546** | 78 | 53 |
| mm's own cases, 278, output identical to CPython | **278** | 10 | 3 |
| Model-written codemode solutions, 1,722 passing on CPython | **1,701** | 829 | 0 as written (283 with `async` removed by the harness) |
| 22 recorded codemode tasks pass the checker | **22** | 19 | 5 (sync rewrite) |
| `await` on a host call, with calls in flight | **yes**: the run parks with N calls open | no: each call blocks the VM | no `async`/`await` |
| Session isolation | **yes** | **no**: one interpreter, shared builtins and modules | **yes**, at most 16 VMs per process |
| Memory limit | **256 MiB, `MemoryError`** | heap size, `MemoryError` | **none**: a 1 TiB string kills the host |
| Hostile inputs (19) that crash the host | **0** (none time out) | 1 (deep `==`); 4 timeouts | 8 |
| Stop a busy loop | 0.06 ms, **parks, resumable** | 0.08 ms, run lost | 0.10 ms, run lost |
| Stop a loop wrapped in `except Exception:` | **parks** | stops (run lost) | **never** |
| Stop a loop wrapped in bare `except:` | **parks** | **never** | **never** |
| Stop inside one 1M-string `sorted` | **1.5 ms, parks** | 704 ms, run lost | 434 ms, run lost |
| Cost of being stoppable | within ±1.1% | none (always on) | **+2,840–3,480%** when armed |
| Snapshot mid-await, across processes and native ↔ wasm | **16–40 KB zstd, 0.09–0.26 ms restore** | none [S] | none [S] |
| Big ints | yes | yes | **no: 64-bit, wraps silently** (`2**64 == 0`) |
| Bench programs, time vs mm (geomean, programs both run correctly) | 1.00 | 2.1× slower (9 of 13; `sort` wrong, `dict` 8.6 min) | 1.5× slower (6 of 13, literals edited) |
| New session + codemode setup | 114 µs | 129 µs (a dict in the shared interpreter) | 578 µs (a VM) |
| RSS per set-up session | 135 KB (40 live) | not isolated: 1.7 KB of shared heap | 709 KB (8 live; 40 impossible) |
| Host call, sync / awaited | 0.36 / 2.95 µs | 0.13 / 2.02 µs (blocking) | **0.02** µs / — |
| Longest GC pause, 2M live objects (§9) | **5.7 ms; 12 ms with cyclic garbage** | 75–81 ms | 66–71 ms |
| Language | Rust, no `unsafe` | C | C11 |

**What it means.** On the job this repo exists for, running model-written
Python in a sandbox, neither engine is a substitute for mm. Of the 1,722
codemode solutions CPython runs, MicroPython runs half (829) and PocketPy none
as written. Neither can keep a session across processes, neither can resume a
stopped run, and each has a stop that ordinary `try/except` code defeats.
PocketPy lets one cell crash the host. The extra ~1.9 MB buys CPython
semantics and stdlib breadth (most of the difference), plus pause-anywhere,
snapshots, isolation, a memory limit and GC pauses 6–13× shorter at 2M live
objects. Where they are better: both are
4–5× smaller in size, MicroPython's and PocketPy's plain sync host calls are
cheaper, MicroPython runs generators 2.9× faster than mm and a pathological
regex 3.9× faster, and PocketPy runs string building and `dict` faster. mm's
own weak spots showed up too. Its regex engine is slower than CPython's on
catastrophic backtracking (21.4 s against 11.3 s, §4), and its int work cap
refuses big-int operations CPython computes, such as one multiply of two
1M-bit ints, with an immediate `MemoryError` (§3). PocketPy's VMs are fully isolated
like mm's sessions; MicroPython's sessions are not.

## 1. Capability [M]

Three runs, each judged against CPython 3.14.7. The **Monty corpus** runs
through `conformance/oracle.py` unchanged, with an adapter in place of
`mm exec --json`: 568 `cpython`-tagged cases. The 81 `host` cases need the
datatest's host fixtures, which only mm emulates. **mm's own cases** run as
scripts against CPython (stdout and exit status; "strict" also compares
stderr). The **codemode solutions** run through
`conformance/codemode/run.py`'s runner with the real mocked tools and
checkers; mm's outcomes are `results/runs.jsonl`, which `run.py --check`
confirmed on this build. MicroPython compiles no top-level `await`, so its
solutions are wrapped in `async def` + `global` + `asyncio.run`. PocketPy has
no `async`, so it also ran a rewrite with `await`, `async` and `asyncio.*`
removed (`pocketpy-sync`, a harness transform). Failures are grouped by first
cause.

| Engine | Corpus (568) | Own (278; strict) | Codemode (1,722) |
|---|---:|---:|---:|
| mm | **546** | **278; 278** | **1,701** |
| MicroPython | 78 | 10; 7 | 829 |
| PocketPy | 53 (13 crash the CLI) | 3; 3 | 0; sync rewrite 283 |

First causes of failure:

| Engine | Corpus | Own | Codemode |
|---|---|---|---|
| mm | semantics 17 (one is `builtin__math_funcs`, refused by the int work cap since the first run), missing builtin 5 (`eval`/`exec`) | — | missing module 21 (`xml` 18, `email` 3) |
| MicroPython | traceback format 140, semantics 126, exception message 87, missing attribute or method 41, syntax 38, missing module 37, missing builtin 9 | semantics 106, missing attribute or method 53, syntax 35, missing builtin 16, missing module 15+ | missing module 537 (`collections.Counter` 177, `csv` 101, `statistics` 63, `decimal` 44, `urllib` 43, `html.parser` 40, `zoneinfo` 22), missing attribute or method 179 (`re.findall`, `re.I`, `hashlib.md5`, `str.isalnum`…), semantics 147, syntax 17 |
| PocketPy | syntax 148, semantics 113, traceback format 97, exception message 72, missing attribute or method 44, crash 13 | syntax 153, semantics 64, missing attribute or method 30 | as written: syntax 1,682 (`await`), crash 40. Sync rewrite: syntax 908 (776 generator expressions), missing module 340 (`re` 100, `csv` 69, `urllib.parse` 26, `hashlib` 24), semantics 80, missing attribute 73, crash 38 |

Feature probes, one snippet each ([M]; `probe/p.py` in the evidence directory,
run on each CLI, and `probe/mm/`, the same snippets as files, since mm has no
`exec`):

| Feature | mm | MicroPython | PocketPy |
|---|---|---|---|
| Big ints | yes | yes (MPZ) | **no**: `2**64` is `0`, `2**63` is negative, no error |
| Unicode strings | yes | yes, but `'straße é'.upper()` is `'STRAßE é'` (ASCII-only case mapping), no `casefold` | same as MicroPython; `"\u00e9"` is a SyntaxError |
| `dict` insertion order | yes | **no** (`{'b':1,'a':2}` lists `a, b`) | yes |
| Stable `sort` | yes | **no** (bench `sort` checksum differs) | yes |
| Classes: C3 MRO, `super()` in a diamond | yes (`DBCA`) | no `__mro__`; `super()` gives `DBA` | same as MicroPython |
| Descriptors, `__iadd__`, `__slots__` | yes, yes, **no** | yes, yes, yes | no, no, yes |
| Exceptions: chaining, `try/else`, `finally`, `except*` | yes | no chaining, no `except*` | none of the four |
| Tracebacks | CPython's format, carets included | no source lines | no frames after `exec` |
| Generators: genexp, `send`, `yield from` | yes | yes | **no genexp**, no `send` |
| `async`/`await`, `asyncio.gather` | yes | yes (no top-level `await`) | **no** |
| `match` | yes, with class patterns | **no** | literal patterns only; `re.match` does not parse (`match` is a keyword) |
| Walrus, starred middle (`a, *b, c`) | yes, yes | yes, yes | yes, no |
| f-string format specs | yes | yes | `{x:>4}`, `{x:x}` fail |
| `json.dumps(indent=, sort_keys=, separators=)` | yes | no keywords | `indent` only |
| `re` | CPython's syntax, `findall`, named groups | `match`/`search`/`sub` only; named groups "too complex" | no `re` module |
| Stdlib for code mode | ~50 modules ([size-map.md](size-map.md)) | ~25 (asyncio, json, re-lite, datetime-lite, collections without `Counter`) | ~20 C modules (json, math, random, datetime, collections, dataclasses…); no `re`, `itertools`, `hashlib`, `csv` |

**What it means.** mm matches CPython on the probes above but one and on 546
of the 568 CPython-scored corpus cases [M], but it is not CPython: docs/sdk.md
("Limitations") lists where it still differs, a few of them silently (an
iterator read to the end before an element fails, `range` past 64 bits, the
last digit of `math.cbrt`). MicroPython is a different dialect of Python: correct on its subset, but
models write `collections.Counter`, `re.findall`, `csv`, `statistics`, ordered
dicts and stable sorts all the time, so half their code fails. PocketPy is
further away. It has no `async`, no generator expressions, no `try/else` or
`finally`, no `re`, and ints that wrap. A model writing for it would need a
prompt describing a different language. Worse, wrapping ints and an unstable
sort give wrong answers silently.

## 2. Embedding and host calls [M]

Each host implements the JS study's `Engine` trait, so the commands mean the
same. MicroPython has one interpreter per process (`mp_state_ctx`,
`py/mpstate.h:342`) [S], so a session is a fresh globals dict. A PocketPy
session is one VM; there are 16 per process (`src/public/GlobalSetup.c:14`)
[S]. On both, a tool is a Python function around a native call the host
answers synchronously, with JSON text both ways, as pi's bridge does. Times in µs per call, tool
minus a local function of the same shape (total in parentheses); see the JS
study §4 for the scripts.

| Engine | Sync | Awaited | Fan-out of 100 (`gather`) |
|---|---:|---:|---:|
| mm | 0.36 (0.41) | 2.95 (3.32) | 2.73 (3.38) |
| MicroPython | 0.13 (0.21) | 2.02 (2.20) | 2.02 (5.30) |
| PocketPy | **0.02 (0.12)** | — (no `await`) | — |

**What it means.** A plain host function costs less on both C engines: a
native call with no trip back to the host loop. Their "awaited" calls are not
comparable to mm's, though. On MicroPython a tool call blocks the VM inside
the native function until the host answers. A `gather` of 100 calls therefore
makes 100 calls one after another, and the host cannot see them together,
batch them or run them concurrently, short of answering from other threads.
mm hands the host every open call at once (`Progress::Waiting`). It can park
the run with them open, snapshot it, and resume it in another process (§5).
That is the protocol plak and the codemode runner use. Every engine's per-call
cost is far below a real tool's latency.

## 3. Control [M]

`control KIND` stops a script from another thread a fixed time after its
`hsync` mark, as in the JS study §5. The stop is mm's park handle,
MicroPython's `mp_sched_keyboard_interrupt()` (what its unix port's SIGINT
handler calls, `py/scheduler.c:49`) and PocketPy's watchdog fired now. New kinds:
`bigint` is one multiply of two 1M-bit ints. `catch` and `catch-all` run a
busy loop inside `try` whose handler (`except Exception:` / bare `except:`)
restarts it, as model-written retry code does. Five runs each; "never" means
no control within 10 s.

| Engine | loop | sort (1M strings) | string (65 MB join) | sort-loop | string-loop | bigint | catch | catch-all |
|---|---|---|---|---|---|---|---|---|
| mm | **0.06 ms, parked** | **1.5 ms, parked** | **0.31 ms, parked** | **1.6 ms, parked** | **1.0 ms, parked** | refused: `MemoryError` before it starts (int work cap) | **0.09 ms, parked** | **0.08 ms, parked** |
| MicroPython | 0.08 ms, run lost | 704 ms, run lost | 156 ms, run lost | 765 ms, run lost | 68 ms, run lost | 3,275 ms, run lost | 0.09 ms, run lost | **never** |
| PocketPy | 0.10 ms, run lost | 434 ms, run lost | 75 ms, run lost | 446 ms, run lost | 82 ms, run lost | n/a (no big ints) | **never** | **never** |

Times are medians. "Parked" runs resumed and parked again; "run lost"
means the run ended with an exception but the interpreter kept working. In
every case the session then ran a new cell. mm's `sort` park took 0.8–3.1 ms
across rounds in the first run.

Two mm rows changed since the first run. A 65 MB `join` is now built in
bounded steps, so the stop lands in 0.31 ms instead of 86 ms; and the
`string-loop` `MemoryError` of the first run (the 256 MiB limit counted
garbage not yet collected, so a plain loop of such joins failed on its third
pass) no longer happens: all 5 runs resumed, and `mm run` finishes the loop
[M]. `bigint`, one multiply of two 1M-bit ints, is now refused before it
starts: the int work cap raises `MemoryError: integer operation too large`
for a superlinear int operation estimated past 45 ms (spec §5,
`intops::WORK_MAX`) [S], so no stop is needed and the session stays usable.
CPython, MicroPython (3.3 s) and the first run's mm compute it.

Cost of being stoppable (bench programs with the stop armed and never fired;
`polling`): mm's fuel slices measured +1.1% on `loop`, −0.9% on `fib` and
−0.3% on `sort`, within run-to-run noise. MicroPython's check
is always compiled in and always on. PocketPy's armed watchdog makes `fib`
29× and `loop` 36× slower: it calls `clock()`, a system call, at every
bytecode step (`src/interpreter/ceval.c:118`) [S].

Memory and depth limits [M, §4]: mm stops at its 256 MiB `Limits::memory`
with `MemoryError`. MicroPython's heap is a fixed arena, so allocation past it
raises `MemoryError`. PocketPy has no limit (`PK_MALLOC` is `malloc`) [S].
Recursion: mm and PocketPy raise `RecursionError` at depth 1000.
MicroPython's C-stack check raises `RuntimeError: maximum recursion depth
exceeded` (there is no `RecursionError` class, so `except RecursionError`
itself fails).

**What it means.** All three stop a busy loop in well under a millisecond,
but only mm keeps the run. On MicroPython and PocketPy a stop is an exception
raised inside the cell, so the cell's own handlers see it. PocketPy's
`TimeoutError` is an `Exception`, which every `except Exception:` retry loop
swallows. MicroPython's `KeyboardInterrupt` gets past those, but a bare
`except:` swallows it. Both are common in model-written code, and then the
host has no further stop short of killing the process. mm's park is not an
exception, so script code cannot see it, and mm now parks inside a long
`sorted` (1.5 ms; the JS study measured 2.7 s before the bounded-steps
work) and inside a 65 MB `join` (0.31 ms). The price is the int work cap:
mm refuses some big-int work outright where the C engines run it and lose
the run when stopped. PocketPy's
watchdog is also too expensive to leave armed.

## 4. Fault tolerance and isolation [M]

`hostile FILE` runs one script in a set-up session, then a probe cell in the
same session, under `RLIMIT_AS` 8 GiB and a 60 s timeout. The cases are deep
nesting, recursion, allocation, a pathological regex, huge ints, and the
three minimized `run_source` units of mm's last fuzz campaign
(`conformance/breadth/fuzz/artifacts`).

(outcome; ms; whether the same session then ran the probe)

| Case | mm | micropython | pocketpy |
|---|---|---|---|
| alloc-format-1GB | MemoryError; 0 ms; usable | timeout (60 s) | ok: 1000000000; 5,182 ms; usable |
| alloc-grow | MemoryError; 137 ms; usable | MemoryError: memory allocation failed, alloc…; 1,537 ms; usable | host died: signal 11 |
| alloc-grow-caught | ok: caught True; 138 ms; usable | ok: caught True; 1,539 ms; usable | host died: signal 11 |
| alloc-list-1TiB | MemoryError; 0 ms; usable | MemoryError: memory allocation failed, alloc…; 8 ms; usable | ok: ; 0 ms; usable |
| alloc-str-1TiB | MemoryError; 0 ms; usable | MemoryError: memory allocation failed, alloc…; 7 ms; usable | host died: signal 11 |
| bigint-1e8-digits | MemoryError; 0 ms; usable | timeout (60 s) | ok: 0; 0 ms; usable |
| fuzz-asan | MemoryError: integer operation too large; 0 ms; usable | timeout (60 s) | SyntaxError: int literal is too large; 0 ms; usable |
| fuzz-excrender | KeyError: frozenset({(frozenset({(frozenset(…; 34 ms; usable | timeout (60 s) | NameError: name 'frozenset' is not defined; 0 ms; usable |
| fuzz-strindex | ok: ; 3 ms; usable | ok: ; 307 ms; usable | SyntaxError: invalid escape char; 0 ms; usable |
| nest-data-eq | RecursionError: maximum recursion depth exce…; 181 ms; usable | host died: signal 6 | host died: signal 11 |
| nest-data-json | RecursionError: maximum recursion depth exce…; 94 ms; usable | RuntimeError: maximum recursion depth exceed…; 83 ms; usable | host died: signal 6 |
| nest-data-repr | RecursionError: maximum recursion depth exce…; 88 ms; usable | RuntimeError: maximum recursion depth exceed…; 80 ms; usable | host died: signal 6 |
| nest-list-source | SyntaxError: too many nested parentheses; 1 ms; usable | ok: ; 40,401 ms; usable | host died: signal 6 |
| nest-parens-source | SyntaxError: too many nested parentheses; 1 ms; usable | ok: ; 37,617 ms; usable | host died: signal 6 |
| recursion | RecursionError: maximum recursion depth exce…; 0 ms; usable | RuntimeError: maximum recursion depth exceed…; 8 ms; usable | RecursionError: maximum recursion depth exce…; 0 ms; usable |
| recursion-caught | ok: caught; 0 ms; usable | NameError: name 'RecursionError' isn't defin…; 6 ms; usable | ok: caught; 0 ms; usable |
| recursion-generator | RecursionError: maximum recursion depth exce…; 1 ms; usable | RuntimeError: maximum recursion depth exceed…; 25 ms; usable | RecursionError: maximum recursion depth exce…; 1 ms; usable |
| recursion-repr | RecursionError: maximum recursion depth exce…; 1 ms; usable | RuntimeError: maximum recursion depth exceed…; 5 ms; usable | RecursionError: maximum recursion depth exce…; 0 ms; usable |
| regex-redos | ok: None; 21,426 ms; usable | ok: None; 5,471 ms; usable | SyntaxError: expected '@id', got 'match'; 0 ms; usable |

Isolation (`isolation`): session A patches `builtins.len`, `json.dumps` and
`asyncio.gather`; session B, created earlier, then checks them.

| Engine | Session B sees | Isolated |
|---|---|---|
| mm | `[2, '1', True]` | **yes**: A's patches stay in A (the stdlib module objects are per session) |
| MicroPython | `[-1, '1', False]` | **no**: `builtins.len` and `asyncio.gather` changed for every session (`json` is a read-only C module) |
| PocketPy | `[2, '1', True]` | **yes** (separate VMs) |

No stop was armed in these runs. mm took 21.4 s on the `(a+)+$` regex where
CPython 3.14 takes 11.3 s and MicroPython 5.5 s [M]. The int work cap now
refuses `10 ** (10 ** 8)` (the first run timed out at 60 s; CPython takes over
120 s) and the `fuzz-asan` unit (the first run finished it in 9.0 s) with an
immediate `MemoryError` [M]. MicroPython
timed out on four cases, and parsed 100,000-deep source for about 40 s.
PocketPy accepted `[0] * (1 << 40)` without an error and returned `0` for
`10 ** (10 ** 8) % 7`: wrapping ints again.

**What it means.** mm survived every hostile input with a Python exception
or a result, and a working session. That is its "no panics on guest input"
rule, and every Rust recursion is depth-counted. MicroPython survived all but
one: a deep `==` overflowed the C stack (SIGABRT here, SIGSEGV in its CLI),
since its stack check does not cover that recursion. PocketPy took the host
down eight times: allocation, deep data and deep source all crash it. On PocketPy, as on any C engine, hostile
code needs a process boundary, as plak gives rquickjs. MicroPython's
sessions share one interpreter, so a cell can change what the next cell sees.
An embedder needs one process per tenant, which costs the process's RSS (§7).

## 5. Durability [M for mm, S for the others]

| Engine | Snapshot of a run | Across processes | Native ↔ wasm |
|---|---|---|---|
| mm | the whole session, mid-cell or mid-await (`Session::snapshot`) | yes; `cargo xtask durability` checks every passing case parked and resumed in new processes | yes (`--cross-target`) |
| MicroPython | none: no API serializes frames, generators or the heap [S] | — | — |
| PocketPy | none for a run. `pickle` (`py_pickle_dumps`, `src/modules/pickle.c:459`) serializes values, not frames [S] | — | — |

mm's mid-await images on this build (`snapshot`; restored 10 times, the first
finished with the remaining recorded replies):

| Point | Raw bytes | zstd-1 bytes | Snapshot µs | Restore µs | Restored run finished |
|---|---:|---:|---:|---:|---|
| neg-fanout-40@0 | 69,535 | 16,249 | 65.2 | 89.7 | True |
| agg-avg-order-by-tier@3 | 144,586 | 26,655 | 118.9 | 173.0 | True |
| par-two-level-fanout@1 | 170,197 | 39,699 | 166.2 | 259.7 | True |

**What it means.** Durable code mode is mm's alone among the three. With
MicroPython or PocketPy, a run that must survive a restart has to be rerun
from the start with its tool calls replayed, as pi's durable code mode does
for JS. That only works when the script is deterministic.

## 6. Speed [M]

The 13 `bench/` programs in-process (a new session per run) and as CLI
processes, in ms. PocketPy rejects `3_000_000`, so its programs have those
underscores removed (a literal-only edit). MicroPython's `dict.py` takes
8.6 minutes (516.6 s) [M] per run, so it was timed once. Its 16-bit string hash
(`MICROPY_QSTR_BYTES_IN_HASH` is at most 2, `py/qstr.h:63-68` [S]) puts
200,000 keys into at most 65,536 hash values, and probing turns quadratic.

In-process, ms per run (a new session per run):

| Program | mm | micropython | pocketpy |
|---|---:|---:|---:|
| startup | 0.1 | 0.0 | 0.6 |
| fib | 23.4 | 38.1 | 67.3 |
| loop | 74.0 | 131.7 | 151.8 |
| dict | 129.3 | 516,628 (once) | 99.6 |
| str | 49.3 | 139.3 | 36.4 |
| json_rows | 113.4 | 9,338.4 | fails |
| closure | 46.6 | 81.9 | 95.1 |
| classes | 105.6 | 125.2 | 184.3 |
| generators | 196.0 | 67.3 | fails |
| exceptions | 101.7 | 155.7 | fails |
| fmt | 196.0 | 250.5 | fails |
| sort | 143.3 | 5,134.5 (wrong checksum) | fails |
| interp | 212.0 | fails | fails |

As CLI processes, ms from spawn to exit:

| Program | mm | micropython | pocketpy |
|---|---:|---:|---:|
| startup | 4.8 | 6.4 | 6.7 |
| fib | 29.2 | 46.9 | 85.0 |
| loop | 70.7 | 140.2 | 198.4 |
| dict | 123.7 | skipped | 126.0 |
| str | 53.0 | 160.3 | 47.4 |
| json_rows | 116.8 | 9,494.2 | fails |
| closure | 52.0 | 91.7 | 117.1 |
| classes | 107.1 | 133.0 | 246.3 |
| generators | 193.9 | 76.0 | fails |
| exceptions | 102.6 | 162.2 | fails |
| fmt | 194.3 | 283.9 | fails |
| sort | 142.7 | 5,317.9 (wrong checksum) | fails |
| interp | 221.1 | fails | fails |

"fails" means the program raised: MicroPython's `interp` calls a `str`
method it lacks (`AttributeError`). PocketPy rejects generator expressions,
`try/else`, `{x:>4}` format specs and `json.dumps(separators=)`. The full
errors are in `summary.md`.

Codemode workload: the JS study's 22 recorded tasks. mm runs the set-`s`
solutions as written, MicroPython the wrapped ones and PocketPy the sync
rewrite. Times are ms per task: new session, setup and the script. Only tasks
the real checker passes count.

| Task | mm | micropython | pocketpy |
|---|---:|---:|---:|
| agg-avg-order-by-tier | 1.41 (1.26) | 2.04 (1.84) | fails |
| agg-customer-summary | 0.41 (0.26) | 0.42 (0.26) | 0.88 (0.20) |
| agg-revenue-by-region | 0.23 (0.11) | 0.26 (0.10) | 0.74 (0.08) |
| agg-top-customers | 0.24 (0.12) | 0.28 (0.11) | 0.71 (0.08) |
| logs-access-top-ips | 2.56 (2.40) | fails | fails |
| logs-errors-per-service | 1.87 (1.72) | 3.02 (2.79) | fails |
| neg-fanout-40 | 0.68 (0.56) | 1.08 (0.89) | fails |
| page-accounts-offset | 0.61 (0.47) | 0.68 (0.51) | fails |
| page-all-pending | 0.44 (0.31) | 0.49 (0.32) | fails |
| page-feb-orders | 0.76 (0.64) | fails | fails |
| page-open-bugs | 0.29 (0.18) | 0.37 (0.21) | 0.82 (0.19) |
| par-two-level-fanout | 0.91 (0.77) | 0.72 (0.53) | fails |
| par-weather-warmest | 0.39 (0.27) | 0.38 (0.23) | fails |
| rep-quotes-csv | 0.39 (0.25) | 0.56 (0.34) | fails |
| reshape-nested-counts | 0.98 (0.84) | fails | fails |
| retry-backoff | 0.31 (0.19) | 0.31 (0.16) | fails |
| retry-gather-flaky | 0.55 (0.42) | 0.56 (0.40) | fails |
| retry-give-up | 0.49 (0.37) | 0.51 (0.36) | fails |
| retry-permanent-errors | 0.43 (0.30) | 0.43 (0.28) | fails |
| stat-max-drawdown | 0.72 (0.56) | 1.14 (0.95) | fails |
| wf-close-fixed | 0.87 (0.70) | 0.99 (0.81) | fails |
| wf-customer-latest-order | 0.60 (0.43) | 0.72 (0.53) | 1.17 (0.46) |
| **sum, total** | 16.1 | 15.0 | 4.3 |
| **sum, run** | 13.1 | 11.6 | 1.0 |
| **tasks that ran** | 22 | 19 | 5 |

The sums cover different task sets: compare per task.

**What it means.** On interpretation MicroPython is 2.1× slower than mm on
the geomean of the nine programs it runs correctly (CLI: 2.3×). It wins
`generators` (2.9× faster) but loses everything that hashes strings:
`json_rows` is 82× slower, `sort` 36× slower (with a wrong checksum), and
`dict` takes 4,000× as long. Without `json_rows` the geomean is 1.36×.
PocketPy is 1.5× slower on the six programs it can run (CLI: 1.8×), and
faster than mm on `str` and `dict`. On the workload tasks both ran,
MicroPython's scripts take 1.10× mm's time (geomean of 19) and PocketPy's
sync rewrites 0.84× (5 tasks); mm's are the async originals. For code
mode the per-task cost is below a millisecond on all three, so tool latency
dominates. The capability gap matters far more than the speed gap.

## 7. Efficiency [M]

Start-up, µs: a new runtime, a new session, the codemode setup (tools
defined), and a first `print(1)` cell.

| Engine | New runtime | New session | Codemode setup | First cell |
|---|---:|---:|---:|---:|
| mm | 22.4 | 51.5 | 62.8 | 31.3 |
| micropython | 1,389.5 | 0.4 | 128.4 | 6.5 |
| pocketpy | 1,284.3 | 540.1 | 38.3 | 4.4 |

Memory: `mem empty|setup N` keeps N sessions alive after one warm-up and
reports the RSS and malloc heap added, in KB.

| Engine | empty 1 | empty 8 | empty 40 | setup 1 | setup 8 | setup 40 |
|---|---:|---:|---:|---:|---:|---:|
| mm | 0 / 111 | 1,401 / 901 | 3,564 / 4,589 | 0 / 127 | 602 / 1,012 | 5,382 / 5,084 |
| micropython | 0 / -0 | 0 / 2 | 0 / 3 | 0 / 2 | 0 / 2 | 0 / 3 |
| pocketpy | 524 / 17 | 5,657 / 8,059 | n/a | 528 / 13 | 5,669 / 8,122 | n/a |

MicroPython's own count: its GC heap in use after 1, 8 and 40 set-up
sessions is 7,552, 19,520 and 74,432 bytes (1.7 KB per session). Its RSS
does not move measurably at that scale. PocketPy cannot hold 40 VMs. The process's RSS
before any session: mm 2.6 MB, MicroPython 6.3 MB, PocketPy 4.2 MB. mm's RSS
added by 8 sessions is noisy (0.6–2.4 MB across rounds, as its starting RSS
moved between 2.4 and 4.5 MB); its malloc heap is steady at about 126 KB per
set-up session.

**What it means.** mm's sessions are cheap and isolated: about 135 KB of RSS
each when set up, with the decoded stdlib shared through the engine's module
cache. A MicroPython session is just a dict in a shared heap, 1.7 KB of live
heap each, which is cheap because nothing is isolated. Its process costs about
6 MB of RSS, the price of each isolated session. PocketPy's VMs are isolated
like mm's but cost about 710 KB each, and a process holds at most 16. Starting
a runtime costs 1.2–1.4 ms on both C engines against mm's 22 µs. For
MicroPython most of that is preparing the 256 MiB heap arena.

## 8. Size [M]

| Engine | Target | Bytes | gzip -9 | Build |
|---|---|---:|---:|---|
| mm `hello` | native | 2,409,464 | 1,409,980 | `cargo xtask size` (parser, compiler, ~50 stdlib modules, snapshots) |
| MicroPython, sandbox variant | native | 547,688 | 238,373 | unix port, upstream `-Os`, stripped |
| MicroPython, standard variant | native | 839,040 | 393,911 | unix port as shipped (ffi, sockets, ssl, btree, VFS), `-Os`, stripped |
| PocketPy | native | 462,088 | 232,304 | `pocketpy.c` + a one-script main, `-Os -flto`, gc-sections, `PK_ENABLE_OS=0`, stripped |
| mm `hello.wasm` | wasm32-unknown-unknown | 2,203,338 | 1,083,582 | `cargo xtask size` |
| mm-wasm guest | wasm32-unknown-unknown | 2,289,389 | 1,117,377 | `cargo xtask size` |
| MicroPython | wasm32 (emscripten) | 449,553 | 197,274 | upstream's npm build (+110 KB JS glue); emscripten is not on the orb |
| PocketPy | wasm32-wasip1 | 451,815 | 192,262 | wasi-sdk 34 `-Oz -flto`, then `wasm-opt -Oz` |

The mm rows are at `eb78933`. At `3624a3d`, the first run, they were
2,366,008, 2,164,619 and 2,251,239 bytes: the bounded-steps and GC work
since then added 1.7–1.8%.

**What it means.** mm is 4.4–5.2× larger natively and 4.9× larger as wasm
(5.5–5.6× gzipped). The size goes on the four things in §§1–5 the other
engines lack: CPython's semantics and messages (the corpus and own-case
columns), the bundled stdlib code mode imports (`re`, `datetime`, `decimal`,
`json`, `collections`, unicode tables, about 50 Python modules: see
[size-map.md](size-map.md)), the park/snapshot machinery, and per-session
isolation with limits. The stdlib is the largest part [S, size-map.md], and it
is the only one the earlier size study found negotiable: a codemode-only module
set is the lever if wasm download size matters.

## 9. GC pauses [M]

`gcpause LIVE GARBAGE [cyclic]` builds a live set of LIVE one-item lists,
then makes 20M more one-item lists of garbage beside it (with `cyclic`, each
list holds itself), then drops the live set (`live = None`). The script calls
`hsync` every 1,000 allocations and the host timestamps each call, so the
longest gap between timestamps is the longest stall, whatever causes it: a
collection, an incremental slice, a heap growing, or a refcount cascade. A
gap also holds 1,000 iterations of the loop (median column). Monty 1.5.0
(pydantic/monty, mm's ancestor, whose corpus is `conformance/monty`) joins
here, in a host of its own with its defaults and a 256 MiB `max_memory`
(`hosts/monty`). Medians of 5 rounds, in ms; ">10 ms" counts gaps; peak RSS
in MB.

| Engine | Live | Garbage | Steady max | p99 | median | >10 ms | Build max | Drop | Total | Peak RSS MB |
|---|---:|---|---:|---:|---:|---:|---:|---:|---:|---:|
| mm | 0 | acyclic | 0.23 | 0.09 | 0.047 | 0 | — | 0.00 | 1,009 | 8 |
| mm | 0 | cyclic | 0.65 | 0.35 | 0.170 | 0 | — | 0.00 | 3,656 | 8 |
| mm | 1,000,000 | acyclic | 5.80 | 0.16 | 0.048 | 0 | 16.66 | 0.00 | 1,518 | 147 |
| mm | 1,000,000 | cyclic | 11.96 | 0.32 | 0.173 | 27 | 16.51 | 0.00 | 4,385 | 148 |
| mm | 2,000,000 | acyclic | 5.66 | 0.18 | 0.048 | 0 | 16.34 | 0.00 | 2,069 | 175 |
| mm | 2,000,000 | cyclic | 12.00 | 0.32 | 0.172 | 66 | 16.46 | 0.00 | 5,297 | 175 |
| MicroPython | 0 | acyclic | 19.96 | 0.11 | 0.066 | 4 | — | 0.00 | 1,387 | 259 |
| MicroPython | 0 | cyclic | 18.93 | 0.14 | 0.095 | 4 | — | 0.00 | 2,036 | 259 |
| MicroPython | 1,000,000 | acyclic | 50.83 | 0.12 | 0.065 | 6 | 5.47 | 0.00 | 1,751 | 259 |
| MicroPython | 1,000,000 | cyclic | 50.79 | 0.14 | 0.094 | 6 | 5.43 | 0.00 | 2,377 | 259 |
| MicroPython | 2,000,000 | acyclic | 74.64 | 0.10 | 0.063 | 10 | 11.00 | 0.00 | 2,296 | 259 |
| MicroPython | 2,000,000 | cyclic | 81.38 | 0.18 | 0.094 | 10 | 10.94 | 0.00 | 2,983 | 259 |
| PocketPy | 0 | acyclic | 7.01 | 0.17 | 0.070 | 0 | — | 0.00 | 2,133 | 27 |
| PocketPy | 0 | cyclic | 7.14 | 0.24 | 0.118 | 0 | — | 0.00 | 3,162 | 26 |
| PocketPy | 1,000,000 | acyclic | 38.80 | 0.17 | 0.074 | 126 | 24.74 | 0.00 | 6,832 | 199 |
| PocketPy | 1,000,000 | cyclic | 36.55 | 0.29 | 0.123 | 126 | 24.39 | 0.00 | 7,835 | 199 |
| PocketPy | 2,000,000 | acyclic | 66.33 | 0.16 | 0.074 | 126 | 49.15 | 0.00 | 13,689 | 371 |
| PocketPy | 2,000,000 | cyclic | 70.52 | 0.21 | 0.120 | 126 | 48.35 | 0.00 | 14,548 | 370 |
| Monty | 0 | acyclic | 0.23 | 0.13 | 0.071 | 0 | — | 0.00 | 1,492 | 7 |
| Monty | 0 | cyclic | 12.37 | 1.04 | 0.169 | 4 | — | 0.01 | 5,102 | 27 |
| Monty | 1,000,000 | acyclic | 0.23 | 0.13 | 0.076 | 0 | 65.65 | 50.13 | 2,250 | 167 |
| Monty | 1,000,000 | cyclic | 23.67 | 0.53 | 0.168 | 200 | 66.12 | 49.00 | 7,824 | 179 |
| Monty | 2,000,000 | acyclic | 0.25 | 0.10 | 0.073 | 0 | 130.49 | 95.55 | 3,534 | 327 |
| Monty | 2,000,000 | cyclic | 38.14 | 0.54 | 0.165 | 200 | 132.23 | 109.57 | 11,212 | 334 |

How each engine collects [S]: mm traces from its roots, in full below 32 MiB
live and incrementally above it, in slices at safepoints (spec §9.2);
MicroPython stops the world to mark and sweep its one fixed arena
(`py/gc.c`); PocketPy stops the world to mark and sweep its heap; Monty frees
by reference count and collects cycles by trial deletion every 100,000
GC-tracked allocations (`DEFAULT_GC_INTERVAL`), which the 200 gaps over
10 ms of its cyclic rows match (20M allocations / 100,000).

- **mm** keeps every gap at 12 ms or less, with up to 2M live lists and any
  garbage. The 16.5 ms while the live set grows is a full collection below
  the 32 MiB threshold (the spec allows about 30 ms there [S]). Cyclic
  garbage gives more and longer slices (66 gaps over 10 ms at 2M live) than
  acyclic (none). These gaps are longer than the 1.3–2.5 ms HANDOFF reports
  for its garbage loops; that figure is the gap from a park request to the
  park, a different measurement, so the two do not compare directly [E].
- **MicroPython** pauses 19–20 ms with nothing live, 51 ms at 1M and
  75–81 ms at 2M live lists. Its sweep covers the whole 256 MiB arena, so a
  smaller heap would pause less, and more often [E].
- **PocketPy** pauses 7 ms with nothing live, 37–39 ms at 1M and 66–71 ms at
  2M, 126 times per run, and also 24–49 ms while the live set grows. Its run
  is the slowest: 14 s at 2M live.
- **Monty** never pauses on acyclic garbage (0.23 ms, refcounting frees it
  at once), but dropping the live set frees it in one step: 50 ms for 1M
  lists, 96–110 ms for 2M. Building the live set stalls 66 ms at 1M and
  130 ms at 2M, and cyclic garbage costs 24–38 ms every 100,000
  allocations.

**What it means.** mm's incremental marking holds the longest pause to about
12 ms where the stop-the-world C engines stop for 66–81 ms, and each slice
ends at a safepoint, so a park or deadline lands within one slice. A
MicroPython or PocketPy stop cannot interrupt a collection [E]. Monty's
refcounting is better on acyclic garbage, with no pause at all, but drops
and cycles stall it for longer than mm. The price of mm's design is
throughput: beside 2M live lists, mm's cyclic loop takes 5.3 s where
MicroPython's takes 3.0 s.

## Rerun at eb78933 (0.0.1 RC, 2026-11-11)

What moved for mm since the first run at `3624a3d` (`results/2026-11-09`).
Both [M], same orb; the other engines are pinned and were not rerun except
for §9, which is new.

| Figure | 3624a3d | eb78933 |
|---|---:|---:|
| Corpus, of 568 | 547 | 546 (`builtin__math_funcs`: int work cap) |
| Own cases passing | 259 of 259 | 278 of 278 (MicroPython 10, PocketPy 3) |
| Codemode solutions, of 1,722 | 1,701 | 1,701 |
| Native / wasm `hello`, bytes | 2,366,008 / 2,164,619 | 2,409,464 / 2,203,338 |
| Stop inside 1M-string `sorted` | 2.3 ms | 1.5 ms |
| Stop inside a 65 MB `join` | 86 ms | 0.31 ms |
| `string-loop` resumed without `MemoryError` | 1 of 5 | 5 of 5 |
| Stop inside a 1M-bit multiply | 0.08 ms, parked | refused: `MemoryError` before it starts |
| Hostile `10 ** (10 ** 8)` / `fuzz-asan` | timeout / ok in 9.0 s | `MemoryError` in 0 ms, both |
| Hostile `(a+)+$` regex | 20.6 s | 21.4 s |
| Bench `sort` / `dict`, in-process | 172.8 / 140.1 ms | 143.3 / 129.3 ms |
| Other bench programs, in-process | — | within ±2.6% |
| Bench geomean vs mm, MicroPython / PocketPy | 2.14× / 1.49× | 2.14× / 1.51× |
| Host call, sync / awaited | 0.39 / 3.05 µs | 0.36 / 2.95 µs |
| New session + codemode setup | 114 µs | 114 µs |
| RSS per set-up session, 40 live | 127 KB | 135 KB |
| Snapshot mid-await, zstd / restore | 16–40 KB / 0.09–0.29 ms | 16–40 KB / 0.09–0.26 ms |
| Codemode workload, 22 tasks | 16.1 ms | 16.1 ms |

## Where MicroPython and PocketPy are better

- **Size**: 4–5× smaller, native and wasm (§8).
- **Sync host calls**: 3× (MicroPython) and about 20× (PocketPy) cheaper than mm's
  plain host function, which leaves and re-enters mm's dispatch loop (§2).
- **Generators**: MicroPython runs `bench/generators.py` 2.9× faster than mm;
  PocketPy runs `str` and `dict` faster (§6).
- **Big ints**: MicroPython multiplies two 1M-bit ints (3.3 s) and computes
  `10 ** (10 ** 8)`; mm's int work cap refuses both with `MemoryError` (§3, §4).
- **Allocation throughput**: MicroPython runs §9's cyclic garbage loop beside
  2M live lists in 3.0 s, mm in 5.3 s.
- **Catastrophic regex backtracking**: MicroPython's `re` took 5.5 s on
  `(a+)+$`, mm's 21.4 s and CPython's 11.3 s (§4).
- **Start-up of a fresh process**: about equal (CLI `startup`: mm 4.8 ms,
  MicroPython 6.4 ms, PocketPy 6.7 ms). A new MicroPython session is just a
  dict (0.4 µs against mm's 52 µs), but it is not isolated.
- **Embedding surface**: both are C with a small C API and run on
  microcontrollers. MicroPython has a mature embed port. PocketPy also has
  pybind11-style C++ bindings, a debugger (DAP), a profiler, and
  `ComputeThread` for running VMs in parallel threads [S, pocketpy docs].
  mm has no C API (the cell worker's FFI is the §5 exception) and no
  debugger.
- **`__slots__`**: both accept it; mm rejects it with a clear SyntaxError.
- **Isolation without a session layer**: a PocketPy VM is fully separate, as
  mm's sessions are (MicroPython's are not).

## Method

- **Hosts.** One host per engine implements the JS study's `Engine` trait
  (`bench/evidence/engines/hosts/common`): a C shim per C engine, linked into
  a Rust host built with release, fat LTO, one codegen unit and
  `panic = "abort"`. mm runs the JS study's own host. This study added three
  commands to the common harness (`control catch|catch-all|bigint`,
  `hostile`, `isolation`), and the rerun added `gcpause` (§9) and a
  Monty host for it. mm's `control` now requests the second park
  before resuming, rather than from a 20 ms timer, and reruns the probe when
  the resumed run ended before parking. A park request stays until a park
  takes it (`Session::park_handle`), so the timer's request could land
  after the run ended and park the probe. The JS study's mm rows used the
  timer.
- **Rounds.** `run.py` runs 5 rounds. Every host measurement is a new
  process on core 2, with the engine order rotated each round; within a
  process it repeats 3–20 times after one untimed warm-up. Capability,
  hostile and isolation runs are deterministic and run once. `--engines`
  picks the engines to run and `--reuse DIR` copies the others' records
  from an earlier run; own cases always rerun on every engine, since they
  grow with mm.
- **Like for like, and where it is not.** The tools cross as JSON text on
  MicroPython and PocketPy, and as typed values on mm. MicroPython's codemode
  scripts are wrapped for top-level `await`, and PocketPy's have their async
  layer removed. Both are harness transforms, reported apart. Sleeps are
  virtual on every engine. The MicroPython CLI runs (capability) get every
  micropython-lib module on its path, and the host gets the frozen subset. The
  corpus adapter adds a missing exception line to PocketPy's tracebacks (its
  `format_exc` gives none after `exec`), which can only help PocketPy.

## Pins

| Engine | Revision |
|---|---|
| micro-monty | `main` at `eb78933` (first run `3624a3d`) |
| MicroPython | 1.29.0 release (2026-08-24); micropython-lib `660ddd89`; wasm: npm `@micropython/micropython-webassembly-pyscript` 1.29.0-6 |
| PocketPy | `v2.2.0` (`6cbece9e`, 2026-09-02) |
| Monty (§9 only) | `v1.5.0` (`c7072042`, 2026-11-05) |
| Toolchain | rustc 1.99.0, gcc 12.2.0, wasi-sdk 34, binaryen 133, CPython 3.14.7 |

Build commands, the `/tmp/peng` layout and how to rerun are in
[`bench/evidence/python-engines/README.md`](../../bench/evidence/python-engines/README.md).
