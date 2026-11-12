# micro-monty against the codemode JavaScript engines

The harness and raw data behind `docs/research/js-engines-compare.md`. It
compares micro-monty with the JavaScript engines that do the same code-mode
job: pi's codemode (quickjs-wasi), plak's JS host (rquickjs), djs (native and
wasm), dquickts, and the `qjs` CLIs. It was measured on 2026-11-09 (mm at
`5df122e`, `results/2026-11-09/`) and again on 2026-11-11 (mm at `eb78933`,
the 0.0.1 release candidate, `results/2026-11-11-eb78933/`), each on one orb
with an Intel Xeon @ 2.60 GHz, 16 vCPUs and Linux 6.1. Every timed process was
pinned with `taskset -c 2`.

Contents:

- `run.py`: the campaign, one command (below). It runs `setup.sh`, then writes
  `results/<date>-<commit>/raw.jsonl` (one line per host process),
  `meta.json` (commit, rounds, CPU), `sizes.json`, `calls.jsonl`, `check.txt`,
  `summary.json` and `summary.md`, whose "Headline" table holds the figures
  the doc's summary quotes.
- `setup.sh`: fetches every engine at its pin into `/tmp/eng` (checked by
  commit or sha256) and builds mm, the CLIs, the djs wasm module, the hosts
  and the size hosts.
- `hosts/`: one small Rust host per engine. All of them implement
  `hosts/common`'s `Engine` trait and share its CLI and timers.
- `size/`: the minimal one-script hosts the size table measures (`build.sh`).
- `fixtures/`: 22 codemode tasks, recorded on CPython by `record.py`.
- `js/`: pi-style JS solutions of those tasks.
- `prelude/pi-prelude.js`: pi's codemode prelude, unmodified.
- `check.py`: judges an engine's tool calls with the real codemode checker.

## Pins

| Engine | Revision | Source |
|---|---|---|
| micro-monty | `main` at the commit of this directory | this repo |
| rquickjs | 0.14.0, crates.io latest, bundles quickjs-ng 0.16.2 | crates.io |
| quickjs-ng `qjs` | v0.18.0 (`d7215b0a`) | github.com/quickjs-ng/quickjs |
| QuickJS `qjs` (bench/README's comparator) | 2026-06-04 tarball, sha256 `b376e839…` | bellard.org/quickjs |
| quickjs-wasi | 3.6.2, the npm `quickjs.wasm`, sha256 `d4c9375f…`. Source tag `quickjs-wasi@3.6.2` (`5a7a0eed`), quickjs-ng submodule `6d46d07d` | npm / github.com/vercel-labs/quickjs-wasi |
| pi | `7d073328` (2026-11-08). `packages/codemode` depends on `quickjs-wasi` 3.6.2. `prelude-source.ts` is unchanged since `d57c2d2` | github.com/earendil-works/pi |
| djs | `main` at `0e58f62c` (2026-11-08) | github.com/earendil-works/djs |
| dquickts | `main` at `dad0bafd` (2026-11-08; QuickJS 2026-06-04, with its vendored rquickjs) | github.com/stencil-hq/dquickts |
| wasmtime (hosts) | 50.0.1, as `crates/mm-wasm-host` | crates.io |
| Rust / C | rustc 1.99.0; binaryen `wasm-opt` 133; gcc 12.2.0 (2026-11-09) and 14.2.0 (2026-11-11, Debian 13 orb) | |

The hosts find the external checkouts under `/tmp/eng` (`ENG`). Clone them
there at the revisions above:

```text
/tmp/eng/djs  /tmp/eng/dquickts  /tmp/eng/pi  /tmp/eng/quickjs-ng
/tmp/eng/qwasi-src  /tmp/eng/qwasi/quickjs.wasm  /tmp/eng/dl/quickjs-2026-06-04
```

## Builds

Every Rust host builds with release, fat LTO, `codegen-units = 1`,
`panic = "abort"` and `strip`. That is the repo's release profile, and the mm
hosts carry its per-crate overrides. Build each one with
`cd hosts/X && CARGO_TARGET_DIR=/tmp/eng/target-X cargo build --release`.
The mm-wasm, quickjs-wasi and djs-wasm hosts share `/tmp/eng/target-mm-wasm`
and the lockfile of `crates/mm-wasm-host`. The hosts are their own Cargo
workspaces, outside the repo's, and are harness code: `hosts/common` calls
glibc's `mallinfo2` and the wasmtime hosts load their precompiled module with
`Module::deserialize_file`, both `unsafe`. No engine crate is changed.

| Artifact | How it was built | Like `cargo xtask size`? |
|---|---|---|
| mm `hello`, `hello.wasm`, the mm-wasm guest | `cargo xtask size`: build-std with immediate abort; `wasm-opt -Oz` on the wasm | yes (the reference) |
| `size/rquickjs`, `size/djs` | `size/build.sh`: the same flags as xtask's `shipped_cargo` | yes |
| quickjs-wasi `quickjs.wasm` | **Not rebuilt**: the npm module, built upstream by wasi-sdk with `-Oz -flto`, then `wasm-opt -Oz`. wasi-sdk is not on this orb. Rerunning `wasm-opt -Oz` saves 27 bytes | equivalent |
| djs `djs_wasm.wasm` | djs `scripts/build-wasm.sh` (its `wasm` profile: `opt-level = "s"`, fat LTO, `unsafe-heap`, stripped), then `wasm-opt -Oz` with the feature flags of xtask's `wasm.rs`. 980,545 → 929,693 bytes | yes, plus upstream's own profile |
| djs CLI | `cargo +1.99.0 build --release -p djs-cli` (upstream release profile), then `strip` | no: upstream CLI, for the bench twins |
| quickjs-ng `qjs` | `cmake -DCMAKE_BUILD_TYPE=Release -DCMAKE_INTERPROCEDURAL_OPTIMIZATION=ON` (`-O3`), target `qjs_exe`, then `strip` | no: C CLI |
| QuickJS `qjs` | `make CONFIG_LTO=y qjs` (`-O2 -flto`), then `strip` | no: C CLI |
| dquickts `qjs` | **Failed**: `make CONFIG_LTO=y qjs` stops at `quickjs.h:264` (`JS_MKVAL` compound literal in a static initialiser, gcc 12). dquickts is measured through its rquickjs crate (`hosts/dquickts`) | |

No minimal *wasm* QuickJS host was built from Rust: rquickjs for wasm needs a
wasi-sdk C toolchain. quickjs-wasi is the wasm QuickJS in this comparison.

## Hosts

Each host is one engine behind the same `Engine` trait (`hosts/common`). The
common CLI times each step in-process with `Instant` and emits one JSON line
per process:

| Command | What it measures |
|---|---|
| `startup REPS FIXTURE` | a new runtime, a new session, codemode setup, the first cell, and a session from a saved image where the engine has one |
| `mem empty\|setup N FIXTURE` | RSS (`/proc/self/statm`) and malloc heap (`mallinfo2`) before and after N live sessions, plus the engine's own count (QuickJS `malloc_size`, djs arena, wasm linear memory) |
| `hostcall sync\|await\|fanout N REPS` | N tool calls against N local calls of the same shape, in a new session with one no-op tool. `fanout N` is N rounds of `Promise.all`/`asyncio.gather` over 100 calls |
| `control KIND` | a stop requested from another thread after the script's `hsync` mark (`loop` 50 ms, `string` 10 ms, others 100 ms); reports the latency to regain control, whether the run is resumable, and whether the engine instance survives |
| `snapshot FIXTURE SCRIPT AFTER REPS` | runs a task until AFTER tool results were delivered and the script waits on more, then snapshots it (raw and zstd-1 bytes), restores it REPS times, and finishes the first restore with the same replies |
| `workload FIXTURE SCRIPT REPS` | a new session, setup and the whole task, replaying the recorded tool results. The calls go to `check.py` |
| `bench FILE REPS` / `polling FILE REPS` | a `bench/` program in-process, without and with interrupt polling or fuel slices |
| `gc ITEMS [LIMIT_MIB] [requests]` | ITEMS live pairs `[i, str(i)]`, five explicit full collections, then four workloads beside them (`garbage`, `cycles`, `writes`, `appends`: `gc_script`). Reports the gaps between the engine's control points: mm's 10k-fuel parks, QuickJS's interrupt handler (every 10,000 polls), djs's 10k-fuel slices. With `requests` (mm), the latency from a park request made every 1 ms to the park, as spec §9.2 measures. mm, rquickjs, dquickts and djs only |

How each engine maps onto these commands:

- **mm** (`hosts/mm`): the facade. Typed tools are keyword-only async functions,
  answered with `Resume::Future`, then a batch. Fuel slices: `polling` sets fuel;
  `control` uses the park handle; its resumable check requests the second
  park before resuming (earlier results used a 20 ms timer).
- **mm-wasm**: the same over `crates/mm-wasm-host`, with its default
  `RuntimeConfig`: epoch interruption, a 100 ms park grace, then a trap.
- **rquickjs / dquickts**: one runtime and one full context per session, like
  pi's one wasm instance per VM. They use pi's 256 MiB heap limit and 512 KiB
  stack. pi's prelude talks to the host through a native `bridge`. Jobs are
  drained, then `stalled()` is called, as in pi's `vm.ts`. Stops go through
  QuickJS's interrupt handler. dquickts snapshots are `JS_WriteSnapshot` taken
  mid-await.
- **djs**: `djs-core` with the base snapshot from `djs-prelude`. It runs 10k-fuel
  slices and checks the stop between slices. Its snapshot is `Vm::image`, an
  in-process raw image: upstream says it is not a durable format.
- **quickjs-wasi**: driven through its C exports, the way its TypeScript wrapper
  and pi's `vm.ts` drive it. The interrupt flag is the `host_interrupt` import,
  with the same wasmtime config and 100 ms epoch grace as mm-wasm. Snapshots are
  quickjs-wasi's own: the whole linear memory plus the runtime pointers.
- **djs-wasm**: the module under wasmtime, given its base the way pi's
  `base.ts` does. It runs plain programs, new VMs and pi's prelude only: its
  exports hand host-call arguments out as raw arena values that only djs's
  TypeScript reader decodes. So the codemode protocol is measured on native
  djs, which runs the same engine code.

## Codemode workload

`record.py` picks 22 tasks across aggregate, paginate, batch/fan-out, retry,
filter and join. It runs set `s`'s Python solution of each on CPython through
`conformance/codemode/shim.py` with the real mocked tools, and records every
call and reply. The hosts answer from that table, so every engine sees the
same results and no engine pays for a Python mock. mm runs the same Python
solutions. The JS engines run `js/<task>.js`, written in pi's style
(`await tools.x({...})`, errors carry `e.type`). All of them pass under node
and on every engine (`results/<date>/check.txt`).

## Rerunning

One command, from a fresh orb (about 45 minutes, half of it builds):

```bash
python3.14 bench/evidence/engines/run.py   # setup.sh, 5 rounds, results/<date>-<commit>/summary.md
```

`--no-setup` skips the fetch and builds, `--rounds N` and `--only
startup,mem,hostcall,workload,bench,polling,control,gc,snapshot` narrow it,
and `--summarize-only --out DIR` rebuilds `summary.json`/`summary.md` from a
`raw.jsonl`. Every timed process runs pinned to `$CORE` (default 2); keep the
orb otherwise idle.
