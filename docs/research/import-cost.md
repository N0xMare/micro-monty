# Import cost: where an import goes, and the module-body pass

Commit hashes of micro-monty cited here, other than the public history's
(root `2bcbb8c`), are pre-release history, kept in the private archive
`N0xMare/micro-monty-archive` (branch `history-0.0.1`;
[docs/history](../history/README.md) maps them); the numbers they qualify
are unchanged.

Question (owner, 2026-11-07): importing the common modules is micro-monty's
largest fixed engine cost. One cell importing `json, re, collections,
asyncio, datetime` took 4.3 ms on mm against about 0.3 ms on Monty, whose
modules are Rust [M, host1, docs/research/plak-in-process.md]. The
decoded-module cache (HANDOFF "Scope decisions" 45, 1c03cff) removes the
decode. This pass targets the rest without changing behaviour and without
moving modules to Rust.

Labels: [M] measured here, [S] sourced, [E] estimated. x86-64 orb (Xeon
2.6 GHz), release builds, wall time pinned with `taskset -c 2`. Raw results:
`bench/evidence/results/2026-11-07-import-cost/`. Harness:
`bench/evidence/import_cost/` (`imp`, `measure.py`, `split.py`,
`stmts.py`, `wasm/`).

## Answer

- **Follow-up (§5):** asyncio's prelude attributes are now built on first
  use (`import asyncio` 6.01M → 0.16M Ir, 758 → 12 µs), and the Engine's
  cache also holds unpacked prelude chunks (native and in-process wasm; the
  mm-wasm guest turns it off). The five-module cell is 12.94M Ir and 1.9 ms
  native with the cache warm, about 6 ms on the mm-wasm guest [M].
- **The five-module cell goes from 45.6M to 29.3M Ir per session without
  the cache (−36%) and to 18.6M with it (−59% against 7541558) [M].**
  Native wall time pinned: 4.86 ms → 3.59 ms without the cache, 2.49 ms with
  it; RSS per session 1,852 → 1,203 KiB with the cache. On the mm-wasm
  guest (which cannot share decoded modules): 7.85 → 5.12 ms [M]. Scaled to
  host1 (where 7541558 took 4.3 ms), the warm cell would be about 2.2 ms
  [E]: still about 7× Monty's 0.3 ms.
- **Most of an import was not module bodies [M].** Before this pass, the
  five-module cell spent 12.4M Ir decoding bundled bytecode, 12.7M building
  prelude units (9M of it unpacking 4 KB prelude chunks again for each
  unit), 6.7M recomputing the session's code bytes, and about 6M running
  module bodies; bytecode dispatch was under 1% of the import. `re`'s body
  runs in 233 µs of a 724 µs import, `collections`' in 141 of 550 µs.
- **Five engine fixes and one module change**, each behaviour-neutral (own
  cases, the corpus, durability, the 1,793 codemode solutions and
  `cargo xtask wasm-host` unchanged; a script importing 16 modules parked
  1,911 times, and its import line alone 3,810 times, every 28 and 12 fuel
  units, each park continued in a new process, matched the unparked runs):
  1. the code-bytes reservation is kept incrementally (it walked every code
     object on each load: quadratic over an import);
  2. `json`, `asyncio` and `sys` build their prelude attributes in one batch,
     unpacking each chunk once (asyncio 13.1M → 6.0M);
  3. `pack::Unpacker` builds a shared header's decoders once, and the
     unpacker's long-code path and output loop are tighter;
  4. a new class's instances get inline slots for what `__init__` stores on
     `self` (and for the bases'), so the first instance's attributes are
     cached; this cut the first `re.compile` of a session by 60% (15.9M → 6.3M
     Ir) and helps any class with one long-lived instance;
  5. assigning a class attribute recomputes the class's dunder flags only
     for the dunders they depend on, by binary search (since replaced by a
     lookup of each key's cached hash: docs/history/handoff-0.0.1.md "Dunder flags by hash");
  6. `zoneinfo` inlines `calendar.isleap` and `calendar.monthrange` instead
     of importing `calendar` (zoneinfo 66.9M → 33.3M).
- **Size**: +4,227 B `hello.wasm`, +5,376 native, +4,270 guest on 1c03cff
  [M], ceilings raised to the measurement plus about 2 KB. Bench Ir against
  1c03cff within ±0.031% on 12 programs, `startup` −0.23% [M]
  (`bench-ir.md`).
- **What remains** (five-module cell, cache warm, 19.6M Ir inside
  `Vm::execute`) [M]: prelude units 6.1M (asyncio's eager attributes 5.3M),
  `load_in` 5.5M (constants and names interned into the session, code
  objects), module bodies about 6.1M (classes, functions, dict stores), the
  cache-hit path 1.9M. For the heavy modules, bodies dominate: regex
  compiles at import, `enum` and ABC class machinery, and the transitive
  imports CPython's modules make. Section 4 has the breakdown and what
  would remove each part.

## 1. Per-module import cost

Steady Ir per import (`imp ir MODS N`, (Ir(11) − Ir(1)) / 10, minus a
session without imports) [M]. "Cache warm" is the facade default
(`Engine::new`): sessions after the first reuse the decoded modules; "no
cache" is `Engine::with_module_cache(config, 0)`, the cost each session pays
when a process holds one session (plak's worker, mm-wasm).

| import | 7541558 | 1c03cff no cache | 1c03cff cache warm | this pass, no cache | this pass, cache warm |
|---|---:|---:|---:|---:|---:|
| (new session, no import) | 0.49 | 0.49 | 0.49 | 0.49 | 0.49 |
| `json` | 1.33 | 1.33 | 1.33 | 0.78 | 0.78 |
| `re` | 6.74 | 6.77 | 3.06 | 5.64 | 2.88 |
| `collections` | 5.32 | 5.32 | 2.54 | 4.67 | 2.45 |
| `asyncio` | 13.07 | 13.10 | 13.07 | 5.95 | 5.95 |
| `datetime` | 13.77 | 13.78 | 6.58 | 11.63 | 5.99 |
| `typing` | 16.03 | 16.03 | 10.28 | 12.75 | 8.18 |
| `enum` | 7.63 | 7.60 | 5.67 | 6.11 | 4.61 |
| `dataclasses` | 5.72 | 5.71 | 3.31 | 4.45 | 2.51 |
| `functools` | 9.07 | 9.08 | 4.91 | 7.50 | 4.22 |
| `itertools` | 3.53 | 3.53 | 1.85 | 3.28 | 1.91 |
| `decimal` | 68.80 | 69.00 | 54.09 | 46.32 | 34.74 |
| `statistics` | 105.42 | 105.51 | 81.18 | 77.38 | 58.10 |
| `zoneinfo` | 66.93 | 67.00 | 41.59 | 33.32 | 17.48 |
| `tomllib` | 59.71 | 59.58 | 42.44 | 44.00 | 30.23 |
| `html.parser` | 35.35 | 35.35 | 29.74 | 32.81 | 28.58 |
| `calendar` | 37.92 | 37.87 | 25.31 | 30.88 | 20.81 |
| five-module cell (`json, re, collections, asyncio, datetime`) | 45.59 | 45.59 | 31.85 | 29.32 | 18.62 |

Wall time and RSS per session (`imp time`, median of 51 after one warm-up,
pinned; `imp rss` holding 40 sessions) [M]:

| import | 7541558 µs | 1c03cff cache warm µs | this pass, no cache µs | this pass, cache warm µs | RSS / session KiB: 7541558 | this pass, no cache | this pass, cache warm |
|---|---:|---:|---:|---:|---:|---:|---:|
| (new session, no import) | 28 | 25 | 27 | 27 | 109 | 111 | 107 |
| `json` | 114 | 120 | 68 | 75 | 98 | 94 | 94 |
| `re` | 704 | 306 | 644 | 291 | 414 | 414 | 260 |
| `collections` | 554 | 246 | 550 | 262 | 384 | 386 | 226 |
| `asyncio` | 1,254 | 1,218 | 634 | 692 | 394 | 391 | 391 |
| `datetime` | 1,645 | 624 | 1,395 | 610 | 800 | 802 | 452 |
| `typing` | 1,803 | 1,088 | 1,487 | 979 | 787 | 787 | 478 |
| `enum` | 794 | 499 | 694 | 509 | 345 | 345 | 246 |
| `dataclasses` | 599 | 278 | 531 | 269 | 325 | 320 | 206 |
| `functools` | 988 | 500 | 904 | 736 | 536 | 538 | 312 |
| `itertools` | 324 | 130 | 373 | 219 | 259 | 259 | 169 |
| `decimal` | 6,630 | 4,749 | 5,280 | 3,575 | 2,193 | 2,193 | 1,473 |
| `statistics` | 11,012 | 7,805 | 8,720 | 7,140 | 3,216 | 3,216 | 2,136 |
| `zoneinfo` | 7,756 | 5,001 | 4,076 | 2,555 | 2,912 | 2,113 | 1,183 |
| `tomllib` | 6,088 | 4,355 | 4,840 | 3,788 | 2,140 | 2,140 | 1,298 |
| `html.parser` | 3,362 | 2,894 | 2,902 | 2,821 | 820 | 823 | 582 |
| `calendar` | 3,719 | 3,017 | 3,485 | 3,012 | 1,605 | 1,608 | 991 |
| five-module cell | 4,855 | 3,247 | 3,593 | 2,490 | 1,852 | 1,863 | 1,203 |

- RSS per session is (RSS holding 40 sessions − RSS before) / 40, each
  session holding its imports; the session row is a session alone. Decoded
  modules the cache holds are shared, so they are not in the per-session
  figure (the cache commit measured them: 0.6 MB for the codemode set).
- `itertools` is the one module slower in wall time (324 → 373 µs without
  the cache) although its Ir fell (3.53M → 3.28M). The class commit adds
  about 42K Ir to it (the `__init__` scans of its 25 classes), but a build
  without the hint is as slow, so the rest is most likely code layout [E].
  The five-module cell is the same in both builds.

mm-wasm guest (`wasm-imp`, the shipped `wasm32-unknown-unknown` guest under
`mm-wasm-host`, precompiled, a new instance and session plus the import
cell, median of 21, pinned) [M]:

| import | 7541558 µs | this pass µs | guest memory MB, 7541558 | this pass |
|---|---:|---:|---:|---:|
| (new instance and session) | 148 | 113 | 1.8 | 1.8 |
| `json` | 255 | 137 | 1.8 | 1.8 |
| `re` | 1,291 | 1,017 | 2.2 | 2.2 |
| `collections` | 1,025 | 765 | 2.1 | 2.1 |
| `asyncio` | 1,952 | 913 | 2.1 | 2.1 |
| `datetime` | 2,624 | 2,056 | 2.7 | 2.7 |
| five-module cell | 7,851 | 5,117 | 3.4 | 3.4 |
| `typing` | 2,762 | 2,501 | 2.6 | 2.6 |
| `enum` | 1,476 | 1,295 | 2.4 | 2.4 |
| `dataclasses` | 932 | 864 | 2.1 | 2.1 |
| `functools` | 1,698 | 1,558 | 2.5 | 2.5 |
| `itertools` | 596 | 626 | 2.1 | 2.1 |
| `decimal` | 12,648 | 9,417 | 4.6 | 4.6 |
| `statistics` | 19,266 | 15,865 | 6.3 | 6.3 |
| `zoneinfo` | 11,550 | 7,021 | 4.8 | 4.6 |
| `tomllib` | 9,605 | 9,044 | 4.6 | 4.6 |
| `html.parser` | 5,456 | 6,493 | 3.1 | 3.1 |
| `calendar` | 6,021 | 6,685 | 4.4 | 4.4 |

The first seven rows are medians of 21, the rest of 11. Single heavy modules
vary about ±10% between runs here: two more runs of 31 gave `html.parser`
5.7–6.4 ms (7541558) against 6.1–6.3 ms, `calendar` 7.1 against 6.4–6.7 ms,
`tomllib` 10.6–10.7 against 8.3–9.2 ms [M]. The guest cannot use the module
cache (one session per instance), so it gains only this pass's part.

## 2. Where the time went, and what changed

Method: callgrind on `imp ir` with symbols (`split.py`), and per
top-level-statement wall time inside one cell (`stmts.py`: the module's
statements run with a host clock reading between each two).

**The code-bytes walk.** `Vm::reserve_code` runs before every load (each
bundled module and each prelude unit) and summed `buffer_bytes` over every
code object loaded so far: 6.7M Ir of the five-module cell [M]. The sums are
now kept in `Vm::code_sums` as codes are appended, reset where codes are
removed (a failed load's truncation, reclamation, restore), and checked
against the full walk by a `debug_assert` in every debug test. The memory
reserved is the same number as before.

**Prelude units.** A prelude unit (a function or class the prelude defines,
built on first lookup) unpacked its whole 4 KB chunk each time: about
200K Ir per unit [M]. `asyncio` binds 38 prelude attributes eagerly (its
module is built in Rust), `json` 5 and `sys` 4. `Vm::prelude_get_all`
copies the records of the units still to build chunk by chunk, one unpack
per chunk (asyncio: 38 → 15 unpacks), then builds them in the original
order, so class creation order, `__subclasses__()` and object identity are
as before. The copied records are charged as workspace beside the chunk.

**The unpacker.** `mm-compile` builds at `opt-level = "s"`, which left
`Huff::decode` a call per symbol. It is now `#[inline(always)]`, the
long-code path starts at the table's 10 bits (canonical first codes per
length instead of bit-by-bit from length 1), the output is written in place,
and `pack::Unpacker` keeps a shared header's tables across parts. Prelude
chunk unpacking fell from 30.6M to 23.3M Ir per 10 asyncio imports [M].

**First instances.** Instances get `cls::HINT` inline slots, and the hint
was learned only as instances overflowed (starting at 2). Attributes past
the inline slots live in an overflow buffer that the attribute caches do not
cover, so the first instance of a class with more attributes ran every
attribute access through `class_lookup` for its whole life. The first
`re.compile` of a session spent 10M of its 15.9M Ir there (3,284 cache
misses, against 49 for a second compile) [M]. `Vm::new_class` now starts
the hint at the largest of 2, the bases' hints and the number of distinct
`self.<name> =` stores in the class's own `__init__` bytecode (also when
`__init__` is assigned later, as `dataclasses` does); the hint still grows
as before. First compile: 6.3M Ir. decimal 69.0M → 46.3M, statistics
105.5M → 77.4M, tomllib 59.6M → 44.0M (with the other changes) [M]. Classes
whose first instance is long-lived in user code (agents' one-off helper
classes) get the same benefit; the classes bench moved −0.03%.

**Class flags.** `compute_flags` decides each class's dunder flags over its
MRO, scanning 80 names per dict key and every native method of builtin
bases. It ran on every class attribute assignment (`timedelta.min = ...`,
`struct_time.__module__ = ...`) for the class and all subclasses. Now it
runs only when the assigned name is one of the flagged dunders, and the name
lookup is a binary search over a table sorted at compile time (verified
against the old scan for every dunder name).

**zoneinfo.** `zoneinfo._zoneinfo` imported `calendar` (with `datetime`,
`enum`, `itertools`) for `isleap` and `monthrange` in its POSIX-TZ-string
rules. They are inline now, checked against `calendar.monthrange` for every
month of years −500..10,499 on CPython (0 mismatches). The only visible
difference: `dir(zoneinfo._zoneinfo)` no longer lists `calendar` (a private
module; CPython's lists it).

## 3. What was not changed

- **Deferring imports that keep a public attribute.** `statistics` imports
  `decimal` (3.7 ms of its 10 ms [M]) only for type checks, `functools`
  imports `collections` for `namedtuple` and builds `_CacheInfo`, `typing`
  imports `collections` and `functools`. CPython's modules bind these names
  (`statistics.Decimal`, `functools.namedtuple`), so deferring them needs a
  module `__getattr__`/`__dir__` pair that itself shows in `dir()` and
  `vars()`. Not done; the saving is listed below.
- **Deferring work the first use repeats.** `decimal._parser`,
  `html.parser`'s and `tomllib`'s module-level regexes, `zoneinfo`'s `re`
  and `collections`: the module's first real use compiles or imports them
  anyway, so a cell would pay the same.
- **No module moved to Rust**, and no native helper was landed.

## 4. What remains, and what would remove it

Split of the final build (`split.py`, Ir per import inside `Vm::execute`,
cache warm; `split-final-on.md`, and `split-final-off.md` without the cache)
[M]:

| import | total | decode (cache hit) | prelude units | load + body | of which `load_in` |
|---|---:|---:|---:|---:|---:|
| five-module cell | 19.57 | 1.89 | 6.06 | 11.62 | 5.51 |
| `re` | 3.28 | 0.49 | 0.39 | 2.39 | 0.93 |
| `datetime` | 6.81 | 1.00 | 0.00 | 5.82 | 1.93 |
| `asyncio` | 5.40 | 0.00 | 5.31 | 0.09 | 0.97 |
| `typing` | 8.66 | 0.79 | 1.71 | 6.16 | 1.85 |
| `decimal` | 36.31 | 2.00 | 1.73 | 32.58 | 5.04 |
| `statistics` | 60.67 | 3.36 | 1.69 | 55.63 | 7.20 |
| `zoneinfo` | 19.97 | 2.80 | 0.72 | 16.45 | 6.51 |
| `tomllib` | 32.02 | 2.40 | 2.03 | 27.58 | 5.62 |
| `html.parser` | 29.20 | 0.77 | 0.41 | 28.03 | 1.45 |

Without the cache (one session per process: plak's worker, mm-wasm), add
the decode: 9.3M for the five-module cell, 8–17M for each heavy module.

What dominates, and what would remove it:

1. **Prelude units (a third of the common cell).** `asyncio` builds 38
   prelude classes and functions eagerly; each needs its chunk unpacked, its
   bytecode decoded and loaded, and the class built. Any first use of a
   prelude feature pays the same (a session's first `int | str` builds 4
   units: +1.1M Ir [M]). An Engine-held cache of unpacked prelude chunks or decoded units,
   like the module cache, would remove the unpack and decode (most of it
   [E]); a built-in image would remove all of it.
2. **`load_in` (about 28% of the common cell).** Each session interns every
   constant and name of each module it imports into its own heap and builds
   its code objects. Only a prebuilt heap (a built-in import image or a
   template) avoids it.
3. **Module bodies.** In the common cell they are small (about 6M Ir). In the
   heavy modules they dominate:
   - **Regex compiles at import** [M, `stmts.py`]: `decimal` 0.84 ms
     (`_parser`, `_parse_format_specifier_regex`), `html.parser` about
     1.5 ms, `tomllib._re` about 1.0 ms, `fractions` about 1.3 ms. The
     pattern parser is Python (`re._parse_pattern`, CPython's sre parser):
     420 µs of a 484 µs warm compile of `decimal._parser` [M]; codegen is
     63 µs. A native parser is the one native helper that would remove a
     hot spot here: compiles would fall to about the codegen time [E], at
     roughly 25–40 KB of `hello.wasm` for a port of `_parser.py` [E, not
     prototyped]. Not landed (owner's Rust-versus-Python decision).
   - **Transitive imports CPython's modules make**: `statistics` →
     `fractions`, `decimal`, `random`; `typing` → `collections`,
     `functools`; `dataclasses` → `copy`; `functools` → `collections`.
     `statistics`'s `from decimal import Decimal` alone was 3.7 ms of its
     10.1 ms before this pass [M]; making it lazy needs a module `__getattr__` that shows in
     `dir()`.
   - **Class creation** with metaclasses and `__set_name__`/
     `__init_subclass__` hooks: `enum`'s `IntFlag`, `Enum`, `Flag`,
     `StrEnum`, `IntEnum` take 0.4 ms together [M]; `compute_flags` over
     builtin bases' method tables is still about 10% of `import enum` [M].
4. **For a built-in import image**: with the cache, a codemode-set template
   restore costs 9.95M Ir warm [S, 2026-11-07-module-cache], against 18.6M
   for fresh imports now (it was 42.6M uncached): a restore is 1.9× cheaper
   than importing, down from 4.3×. An image of the common modules built
   into the engine would replace prelude units, `load_in` and bodies (about
   17M of the 18.6M) with a restore-like copy whose cost grows with the
   image (about 5–6 µs per KB natively [S, session-templates.md]); the
   template rules (no leftover names, `random` reseeding, clock readings)
   would apply to it too.

## 5. Follow-up: asyncio's attributes on first use, and a prelude chunk cache

Owner follow-up (2026-11-07), in place of the built-in import image
(HANDOFF "Scope decisions" 47). Raw results:
`bench/evidence/results/2026-11-07-asyncio-lazy/`. Builds on 168b7ac:
`base`; `lazy` (step 1); `new` (steps 1 and 2).

1. **asyncio's 38 prelude classes and functions are built on first use.**
   `import asyncio` binds each name to `UNDEFINED`, which no Python code can
   hold, and the first lookup (`getattr`, `hasattr`, `from asyncio import`,
   `import *`) builds that unit and binds it. Reading `__dict__` (`vars`,
   `dir`) first builds the rest in one batch. Key order, `dir`, identity,
   the submodules, error text and hints are unchanged, and a snapshot keeps
   which names are still unbuilt. Own case 417 matches python3.14, and
   passes durability parked at every fuel unit (190 parks, including
   between the import and the first lookup) [M].
2. **The Engine's cache also holds unpacked prelude chunks**, under the
   same cap and the same accounting as decision 45: a hit charges the
   session the workspace the unpacking took. A session building units
   unpacked a 1–2 KB chunk again for each one (18 unpacks for the
   primitives cell). `prelude_chunks_from_the_cache_are_charged_as_unpacked`
   compares outcomes and images, cold against warm, at 275 memory limits;
   it fails if a hit is not charged. asyncio's chunks take 51.6 KB of the
   cache [M].

Per session, native, net of an empty session (0.50M Ir, 34 µs) [M]. Ir is
steady state (`measure.py`), wall time the median of 7 alternating rounds of
`imp time` medians (pinned to core 2):

| cell | cache | Ir base | Ir lazy | Ir new | µs base | µs lazy | µs new |
|---|---|---:|---:|---:|---:|---:|---:|
| `import asyncio` | on | 6.01M | 0.16M | 0.16M | 758 | 12 | 12 |
| | off | 6.02M | 0.16M | 0.16M | 757 | 12 | 12 |
| five-module cell | on | 19.55M | 13.77M | 12.94M | 2,829 | 2,059 | 1,919 |
| | off | 31.37M | 24.94M | 24.93M | 4,091 | 3,531 | 3,273 |
| asyncio gather + run | on | 7.09M | 1.87M | 1.04M | 953 | 236 | 130 |
| | off | 7.11M | 1.87M | 1.87M | 954 | 238 | 240 |
| asyncio primitives | on | 8.93M | 6.06M | 3.64M | 1,256 | 868 | 540 |
| | off | 8.93M | 6.06M | 6.02M | 1,255 | 868 | 868 |

With the cache off, `new` runs the same instructions as `lazy`; the
five-module cell's 3,531 against 3,273 µs is noise or code layout, not
work.

RSS per session with 40 held, cache on [M]: `import asyncio` 391 → 79 KiB,
the five-module cell 1,262 → 944 KiB, the primitives cell 449 → 290 KiB.

The mm-wasm guest (shipped `mm_wasm_guest.wasm`, one session per
instance, so the cache only helps within the session; median of 3
alternating rounds of 21, net of an empty instance and session, 148 µs)
[M]:

| cell | base | lazy | new | guest memory base → new |
|---|---:|---:|---:|---:|
| `import asyncio` | 1,323 µs | 56 µs | 55 µs | 1,984 → 1,728 KiB |
| five-module cell | 7,359 µs | 5,939 µs | 5,860 µs | 4,544 → 3,584 KiB |
| asyncio gather + run | 1,632 µs | 469 µs | 429 µs | 1,984 → 1,728 KiB |
| asyncio primitives | 2,151 µs | 1,505 µs | 1,365 µs | 2,048 → 1,984 KiB |

On the guest, the chunk cache keeps the chunks a session unpacked alive
for the instance's life: the five-module cell's linear memory is
3,392 KiB on `lazy` and 3,584 KiB on `new` [M]. Against Monty's 0.3 ms,
the five-module cell is now about 1.9 ms native with the cache warm and
5.9 ms on the guest [M]; scaled to host1 (where 7541558 took 4.3 ms), about
1.7 ms [E].

**The guest turns the chunk cache off** (owner decision, HANDOFF scope
decision 48). With one session per instance, 192 KiB of linear memory for
every instance's life costs more than the 50–180 µs the chunks save that
one session. The guest's engines are `Engine::without_prelude_chunk_cache`:
decoded modules are still cached within the session, as before; native and
`hello.wasm` keep the chunk cache, and lazy asyncio stays everywhere. On
the guest, against c0f37cd, median of 5 alternating rounds of 21, net of an
empty instance and session (about 153 µs) [M, `wasm-guest-nochunks.jsonl`]:

| cell | chunk cache on | off | linear memory on → off |
|---|---:|---:|---:|
| `import asyncio` | 58 µs | 59 µs | 1,728 → 1,728 KiB |
| five-module cell | 6,260 µs | 6,388 µs | 3,584 → 3,392 KiB |
| asyncio gather + run | 475 µs | 522 µs | 1,728 → 1,728 KiB |
| asyncio primitives | 1,442 µs | 1,624 µs | 1,984 → 1,920 KiB |

This run's machine was slower than the table above's (5.86 ms then for
the same guest); compare within a table. Guest size −60 B (2,152,202 →
2,152,142); `hello.wasm` and native unchanged [M].

**Size** [M] on 168b7ac: step 1 `hello.wasm` +1,289, native +1,888, guest
+1,279; step 2 +1,282, +1,424, +979; on e7d7b81 together +2,561, +3,360,
+2,256 to 2,050,734, 2,234,312 and 2,152,202; ceilings raised to the
measurement plus about 2 KB (S1). **Bench Ir** against 168b7ac: every program within
±0.02%, `startup` −0.15% [M] (`bench-ir.md`).

**What remains** in the five-module cell, cache warm (`split-on.md`, Ir
inside `Vm::execute`) [M]: 14.45M, of which the decoded-module cache's hit
path 1.97M, prelude units 0.67M, and load plus bodies 11.81M (`load_in`
4.46M: constants and names interned into the session and its code objects;
the rest the module bodies, mostly `re`, `datetime` and `collections`). In
the primitives cell, 3.22M: prelude units 1.24M (now the unit programs'
decode and load, not the unpack) and the cell's own work 1.98M. A cache of
decoded unit programs would remove part of the 1.24M [E]; `load_in` and
the bodies are what only a prebuilt heap (decision 47's image) removes.

## Reproduce

```bash
cargo build --release --manifest-path bench/evidence/import_cost/Cargo.toml --target-dir target/import-cost
python3.14 -I bench/evidence/import_cost/measure.py OUT.json            # Ir, wall, RSS; cache on
IMP_CACHE=0 python3.14 -I bench/evidence/import_cost/measure.py OUT.json
CARGO_PROFILE_RELEASE_STRIP=false CARGO_PROFILE_RELEASE_DEBUG=1 cargo build --release \
    --manifest-path bench/evidence/import_cost/Cargo.toml --target-dir target/import-cost-sym
python3.14 -I bench/evidence/import_cost/split.py                       # decode / prelude / load + body
python3.14 -I bench/evidence/import_cost/stmts.py decimal               # slowest top-level statements
cargo build --release --manifest-path bench/evidence/import_cost/wasm/Cargo.toml --target-dir target/import-cost-wasm
target/import-cost-wasm/release/wasm-imp GUEST.wasm 21 - json re json,re,collections,asyncio,datetime
```
