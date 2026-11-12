# micro-monty against MicroPython and PocketPy

The harness and raw data behind `docs/research/python-engines-compare.md`. It
compares micro-monty with the two other small embeddable Python engines,
MicroPython 1.29.0 and PocketPy 2.2.0, the latest releases on 2026-11-09, as
engines for sandboxed code-mode cells. It reuses the JS study's host harness
(`bench/evidence/engines/hosts/common`: the same CLI, timers, fixtures and
checker) and its mm host. Measured on one orb (Intel Xeon @ 2.60 GHz, 16
vCPUs, Linux 6.1); every timed process pinned with `taskset -c 2`.

One command rebuilds and reruns everything:

```bash
bench/evidence/python-engines/rerun.sh            # every engine; about 85 minutes, 5 rounds
# A release candidate: rerun mm alone and copy the pinned engines' records
# from an earlier run on the same machine.
bench/evidence/python-engines/rerun.sh --engines mm --reuse results/2026-11-11 --out results/<date>   # about 10 minutes
bench/evidence/python-engines/rerun.sh --rounds 1 --only startup,control   # a subset
python3.14 bench/evidence/python-engines/run.py --summarize-only --out bench/evidence/python-engines/results/<date>
```

`--engines` picks the engines a run measures. `--reuse DIR` copies the other
engines' records from DIR: `raw.jsonl` lines for sections the new directory
has none of, their capability files, and their workload calls. MicroPython,
PocketPy and Monty are pinned, so only the machine can move their numbers:
reuse a directory measured on the same machine type, and say so.
`results/<date>/reused-from.txt` records it.

Contents:

- `rerun.sh`: fetches the pinned sources, builds every engine, size build
  and host, then runs `run.py`.
- `run.py`: the campaign. It writes `results/<date>/`: `raw.jsonl` (one line
  per host process), `corpus-*.jsonl`, `own-*.jsonl`, `codemode-*.jsonl`
  (one line per case), `sizes.json`, `calls.jsonl` and `check.txt` (the
  workload calls through the real checker), `summary.json` and
  `summary.md`. It also writes `inputs/`: the per-engine scripts it fed the
  hosts.
- `capability.py`: the corpus, own-case and codemode runs (below).
- `adapters/pyexec.py`: `mm exec --json` for MicroPython and PocketPy, so
  `conformance/oracle.py` judges them unchanged.
- `codemode/{micropython,pocketpy}_shim.py`: conformance/codemode's runner
  protocol inside each engine.
- `hosts/{micropython,pocketpy}`: the engines behind the common `Engine`
  trait (a C shim per engine, linked into a Rust host). `hosts/monty`:
  Monty, for the `gcpause` section only.
- `micropython/`: the `sandbox` variant of the unix port and its build.
- `size/`: the size builds (`build.sh`, `hello_pocketpy.c`).
- `probe/`: the feature probes behind the doc's §1 table. `p.py` runs each
  snippet through `exec` on CPython, MicroPython and PocketPy
  (`<cli> probe/p.py`); `probe/mm/` holds the same snippets as files for
  `mm run`, which has no `exec`.

## Pins

| Engine | Revision | Source |
|---|---|---|
| micro-monty | `main` at the commit of this directory | this repo |
| MicroPython | 1.29.0 release tarball (2026-08-24) | github.com/micropython/micropython/releases |
| micropython-lib | `660ddd89` (2026-10-28) | github.com/micropython/micropython-lib |
| MicroPython wasm | npm `@micropython/micropython-webassembly-pyscript` 1.29.0-6 | npm |
| Monty | `v1.5.0` (`c7072042`, 2026-11-05), `gcpause` only | github.com/pydantic/monty |
| PocketPy | `v2.2.0` (`6cbece9e`, 2026-09-02); the release's `pocketpy.c`/`pocketpy.h` are amalgamate.py's output | github.com/pocketpy/pocketpy |
| wasi-sdk | 34 | github.com/WebAssembly/wasi-sdk |
| CPython (oracle) | 3.14.7 | `.agents/setup` |
| Toolchain | rustc 1.99.0, gcc 12.2.0, binaryen 133 | |

## Builds: each engine in its best configuration for this job

| Artifact | How it was built | Why |
|---|---|---|
| MicroPython CLI and host library | unix port, `sandbox` variant (`micropython/variant-sandbox`): the standard variant's feature level (`EXTRA_FEATURES`: MPZ big ints, double floats, asyncio, json, re, f-strings, KeyboardInterrupt, stack check) without ffi, sockets, ssl, btree, termios, FAT/LittleFS, `machine`, `os.system`, threads. `-O2 -DNDEBUG` | the standard variant minus what reaches outside a sandbox. `-O2`: the speed build |
| MicroPython frozen modules | asyncio, plus micropython-lib's `__future__`, `abc`, `base64`, `bisect`, `collections-defaultdict`, `contextlib`, `copy`, `datetime`, `fnmatch`, `functools`, `heapq`, `hmac`, `inspect`, `itertools`, `keyword`, `logging`, `operator`, `pprint`, `string`, `textwrap`, `types`, `uuid`, `warnings` (`manifest.py`) | the pure-Python stdlib a cell imports, with no filesystem |
| MicroPython CLI runs (capability) | the sandbox CLI with a 256 MiB heap (`-X heapsize=256M`, mm's default memory limit; the unix default is 2 MiB, too small for the bench) and `MICROPYPATH=.frozen:<all of micropython-lib's python-stdlib>` | the most stdlib MicroPython offers |
| PocketPy CLI (capability, CLI bench) | `src/*.c` + `src2/main.c`, `-std=c11 -O2 -DNDEBUG -ffp-contract=off`: upstream CMakeLists' release flags | upstream's own build |
| PocketPy host | the amalgamated `pocketpy.c` at the same flags, `PK_ENABLE_OS=0` (no `os`, files, sockets, dll) and `PK_ENABLE_WATCHDOG=1` (the only stop it has) | sandbox: no OS access, and a way to stop a run |
| MicroPython size | the sandbox variant at upstream's `-Os`, stripped. The standard variant (`-Os`, stripped) for reference | upstream's size flags |
| PocketPy size | `pocketpy.c` + `size/hello_pocketpy.c` (read one script, run it), `-Os -flto`, gc-sections, `PK_ENABLE_OS=0`, stripped. Wasm: the same with wasi-sdk 34 `-Oz -flto` for wasm32-wasip1, then `wasm-opt -Oz` | like `cargo xtask size` builds mm's `hello` |
| MicroPython wasm | **not rebuilt**: the webassembly port as upstream publishes it on npm (emscripten, `-Os`). Emscripten is not on the orb; MicroPython has no wasi port | |
| mm | `cargo xtask size` (`hello`, `hello.wasm`, the guest), `mm` (`cargo build --release -p mm-cli`), and the JS study's host `bench/evidence/engines/hosts/mm` | the reference |

## Hosts

`hosts/micropython` and `hosts/pocketpy` implement the JS study's `Engine`
trait, so `startup`, `mem`, `hostcall`, `control`, `polling`, `bench` and
`workload` mean what they mean there (`bench/evidence/engines/README.md`
"Hosts"). This study added three commands to `hosts/common`, which every
host now has: `control catch|catch-all|bigint`, `hostile FILE` and
`isolation`.

- **MicroPython.** One interpreter per process: its state is the global
  `mp_state_ctx` (`py/mpstate.h:342`). A session is a fresh globals dict,
  kept alive in `sys.modules` (a GC root). The heap is one 256 MiB GC arena
  and the C-stack check allows 1 MiB. Output goes through a native `print`
  per session. A tool is `async def name(arguments=None, **kw)` around the
  native `_hc(name, json)`, which the host answers synchronously. JSON text
  carries both directions, as pi's bridge does. The VM cannot hand control
  back with a call outstanding, so `asyncio.gather` over tools makes its
  calls one after another. A stop is `mp_sched_keyboard_interrupt()` from
  another thread (`py/scheduler.c:49`, what the unix SIGINT handler calls):
  `KeyboardInterrupt` at the next backward jump or call. `heap N` reports the
  GC heap in use after N set-up sessions.
- **PocketPy.** A session is one VM (`py_switchvm`); PocketPy has 16 per
  process (`src/public/GlobalSetup.c:14`), so `mem … 40` cannot run. A VM is
  reset with `py_resetvm` when reused. There is no `async`/`await` (neither
  is a keyword in `src/compiler/lexer.c`), so tools are plain functions over
  the same `_hc`. A stop is the watchdog. `py_watchdog_begin(0)` is called
  from the stopping thread: PocketPy has no thread-safe interrupt, and with
  threads off its current VM is a plain global. It raises `TimeoutError` at
  the VM's next step (`src/interpreter/ceval.c:118`). The documented use is
  a CPU-time budget set before a run (`clock()`).
- **mm**: `bench/evidence/engines/hosts/mm`, with the new common commands.
  Its `control` now requests the second park before resuming, rather than
  from a 20 ms timer, and reruns the probe if the resumed run ended before
  parking. A park request stays until a park takes it
  (`Session::park_handle`), so a timer firing as the run ended parked the
  probe and reported the session unusable.

- **Monty** (`hosts/monty`, `gcpause` only): one `MontyRun` per run with
  Monty's defaults and `max_memory` 256 MiB (mm's default limit); `hsync` is
  an external function the host answers. Monty frees by reference counting
  and collects cycles by trial deletion every 100,000 GC-tracked
  allocations (`DEFAULT_GC_INTERVAL`, `crates/monty-types/src/resource.rs`).

**`gcpause LIVE GARBAGE [cyclic]`** (common, every Python host): builds a
live set of LIVE one-item lists, then makes GARBAGE more beside it (each a
self-cycle with `cyclic`), then drops the live set (`live = None`). The
script calls `hsync` every 1,000 allocations and the host timestamps each
call, so the longest gap between timestamps is the longest stall, whatever
causes it: a collection, an incremental slice, a heap growing, or a
refcount cascade. It reports the gaps while the live set grows (`build`),
while garbage is made beside it (`steady`) and across the drop. `run.py`
runs LIVE 0, 1M and 2M with 20M garbage, enough for several collections in
MicroPython's 256 MiB arena.

Neither engine can snapshot or resume a run, so `snapshot` is not run for
them. Durability is [S], from their sources (the doc's §6).

## Inputs and transforms

- **Corpus** (`capability.py corpus`): `conformance/oracle.py --engine
  micro-monty` with `adapters/pyexec.py` as the runner. The adapter splits
  off the last expression with CPython's `ast` (as `mm exec` reports it) and
  wraps top-level await, as the oracle wraps `run-async` cases. Nothing else
  is translated. `call-external` cases need the datatest's host fixtures,
  which only mm emulates. They are reported as skipped, so the comparison is
  the 568 `cpython`-tagged cases.
- **Own cases** (`capability.py own`): each `conformance/own` case as a
  script on the engine's CLI against CPython: stdout and exit status (`pass`),
  plus stderr (`strict`).
- **Codemode** (`capability.py codemode`): all 1,793 solutions through
  `conformance/codemode/run.py`'s `run_one` with the real mocked tools and
  checkers. MicroPython compiles no top-level await, so those solutions are
  wrapped in `async def` + `global` + `asyncio.run`
  (`wrap_top_level_await`). PocketPy runs them as written (`pocketpy`), and
  again with the async layer removed (`pocketpy-sync`: `sync_rewrite` drops
  `await`, `async`, `asyncio.run`, `asyncio.gather`), reported apart. mm's
  outcomes are `conformance/codemode/results/runs.jsonl` (`mm-full`), which
  `conformance/codemode/run.py --check` confirms on this build.
- **Workload** (host `workload`): the JS study's 22 recorded tasks. mm runs
  set `s` as written, MicroPython the wrapped scripts, PocketPy the sync
  rewrite (`results/<date>/inputs/`).
- **Bench**: PocketPy rejects `3_000_000` (underscores in numeric literals),
  so its runs use the programs with those underscores removed
  (`inputs/pocketpy/`). MicroPython's `dict.py` takes over 8 minutes per
  run and is measured once (`dict-micropython`).
- **Hostile** (host `hostile FILE`, under `RLIMIT_AS` 8 GiB and a 60 s
  timeout): `run.py`'s `hostile_cases()`, including the three minimized
  `run_source` units of mm's last fuzz campaign
  (`conformance/breadth/fuzz/artifacts`).
