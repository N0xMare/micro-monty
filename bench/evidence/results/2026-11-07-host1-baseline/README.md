# Bare-metal baseline on host1 (2026-11-07)

Machine: host1, AMD Ryzen 9 8945HS (Zen 4), 16 threads, Linux 7.0, idle,
pinned to core 2 (`taskset -c 2`). The full `cargo xtask bench` run is in
`bench/results/2026-11-07-host1-ryzen-x86_64-linux-b01fd7d/`.

Commits: `7541558` (before the decoded-module cache and the import-cost
pass), `b01fd7d` (both landed; the cache off with
`Engine::with_module_cache(_, 0)` or warm), and `357e003` for the
`class::flagged` profile below.

## Bench at b01fd7d

[M] micro-monty's geomean is 0.998× of the 5349520 run
(`bench/results/2026-11-06-host1-ryzen-x86_64-linux-5349520`). The geomean ratios are
mm/CPython 0.998 and mm/QuickJS 0.979. Monty 1.4.0 fails `generators` and
`exceptions`, as before.

`perf-bench.md` / `.csv` (`perf stat` per bench program, made by
`perfsum.py`) [M]: IPC is 3.2–5.5 on everything but `startup` (0.71, a
57% frontend stall in 3 M instructions). L1i misses are at most
1 per 1,000 instructions (`fmt` 0.99, `exceptions` 0.68), so
instruction-cache pressure does not limit the interpreter on this core.

## Import cells, native

`wall-*.json`, from `bench/evidence/import_cost` (`measure.py --no-ir`):
the median of a fresh session importing the module(s), in µs [M]. Columns:
7541558, b01fd7d with the cache off, b01fd7d with the cache warm.

| import | 7541558 | b01fd7d off | b01fd7d warm |
|---|---:|---:|---:|
| (empty session) | 20.0 | 29.8 | 20.2 |
| json | 78.9 | 61.5 | 60.8 |
| re | 455.7 | 394.3 | 202.8 |
| collections | 333.1 | 312.2 | 165.0 |
| asyncio | 748.6 | 412.1 | 415.9 |
| datetime | 905.0 | 825.2 | 387.5 |
| itertools | 193.1 | 236.6 | 150.1 |
| decimal | 4,291 | 3,082 | 2,294 |
| statistics | 7,476 | 5,781 | 4,439 |
| json, re, collections, asyncio, datetime | 2,917 | 2,064 | 1,601 |

The five-module cell goes 2.92 → 2.06 ms with no cache, and to 1.60 ms
warm [M]. The orb measured 4.86 → 2.49 ms [S] (b01fd7d's commit).

## Import cells on the mm-wasm guest

`wasm-*.jsonl`: the shipped guest under wasmtime (one instance per
session, so no cross-session cache), median µs [M].

| import | 1ae712b | b01fd7d |
|---|---:|---:|
| (empty session) | 106 | 105 |
| json | 234 | 199 |
| asyncio | 1,178 | 853 |
| itertools | 538 | 557 |
| decimal | 8,470 | 6,383 |
| zoneinfo | 8,326 | 4,651 |
| json, re, collections, asyncio, datetime | 4,931 | 3,929 |

## itertools: slower in wall time, fewer instructions

The import-cost pass saw itertools get about 50 µs slower in wall time
while its Ir fell, and suspected code layout. `perf-import-raw.txt` [M],
cache off, 7541558 → b01fd7d (a loop of itertools imports):

- instructions 7.71 → 7.17 G (−7%), cycles 2.13 → 2.49 G (+17%);
- L1i misses about the same (1.61 → 1.72 M), iTLB misses small;
- L2 misses from instruction and data fetches 1.97 → 3.31 M;
- `backend_bound_memory` 11.9% → 24.0% (topdown).

So it is not code layout. `perf record` shows the cause: the new
`mm_engine::class::flagged` (9e272a0) takes 11.9% of cycles, and
`memcmp` goes from 5.2% to 9.3%. `flagged` is a binary search over
`FLAGGED_BY_NAME`, comparing names as strings (`class.rs`, `name(j)`); it
runs from `compute_flags` for every `__`-prefixed key of a class dict, and
from `class_changed`. itertools' prelude classes have many dunders.

Still hot on 357e003 [M] (symbolized build, `perf record` on core 2):

| cell | cache | `flagged` self | `memcmp` | wall |
|---|---|---:|---:|---:|
| itertools | off | 11.2% | 8.5% | 296 µs |
| itertools | warm | 18.6% | 15.2% | 160 µs |
| five-module | off | 1.6% | 2.1% | |
| five-module | warm | 3.3% | 4.4% | |

Comparing interned ids instead of strings (a name's symbol id against a
table sorted by id, or a flag stored on the interned name) would remove
most of `flagged`'s self time and nearly all of its `memcmp`. That is
about 25–30% of a warm itertools import, about 40 µs, and about 6–7% of the
warm five-module cell [E] (an estimate from the profile shares; not
built). No engine change was made here.

Since then, 8507ad2 replaced the search with a hashed const table, one
probe per key, and skips a builtin type last in the MRO. On a pinned core
with the cache warm, its commit measures itertools at 178 → 92 µs (−48%)
and the five-module cell at 1,475 → 1,337 µs (−9%) [S]. That is more than
the estimate above, because the skip removes more than the compares.
