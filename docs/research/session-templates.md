# Session templates

Question (owner, 2026-11-06): can a host make imports nearly free per cell
with session templates and no engine change? A template is a snapshot
taken after the common imports and restored for each new session, instead
of `Session::new` plus the imports in every session.

Labels: [M] measured here, [S] sourced from code or earlier evidence, [E]
estimated. Raw results, commands and provenance:
`bench/evidence/results/2026-11-06-session-templates/`. Harnesses:
`bench/evidence/session_templates/`. Engine: main at `1f27970`, unchanged.
The facade already has the helper (`SessionTemplate`: `capture`,
`instantiate`, `image`, `from_image`; docs/sdk.md "Session templates") [S].

## Answer

**Templates work and are safe with three rules, but they do not make
imports nearly free.**

- **Restoring is cheaper than importing [M]**: 1.8–3.6× in Ir and
  1.9–4.3× in wall time for real import sets. A restore
  still decodes the packed bytecode of every imported bundled module again:
  that is 55% of a codemode-set restore and 66% of a heavy-set restore [M].
  The template saves only the execution of module bodies.
- **Restore cost grows with image size**, about 5–6 µs per KB natively [M].
  A broad template costs more than the imports most cells actually make. On
  the codemode suite, starting every solution from a template of the five
  common modules made the median cell slower: setup + engine went from 2.1
  to 4.6 ms in a long-lived host [M]. It helps the tail only where cells
  import heavy modules: on dt-* cells with zoneinfo in the template, p90
  went from 15.2 to 11.0 ms [M].
- **The engine change that makes imports cheap** is a per-process cache of
  decoded bundled modules, shared by every session, as the prelude's code
  already is. A local prototype (not pushed) cut a codemode-set restore from
  22.9M to 9.2M Ir (1.07 ms) and a heavy-set restore from 59.9M to 16.5M Ir
  (2.6 ms) [M]. It cut fresh imports by 20–35% and RSS per session by about
  37%, and it costs 799 B of `hello.wasm` and 801 B of the mm-wasm guest,
  within both budgets [M]. It helps only hosts that run many sessions in
  one process. plak's worker (one session per process) and mm-wasm (one session per instance) gain nothing from it.

**Recommendation.**

1. Document the template pattern with its rules and reapply list
   (docs/sdk.md "Session templates").
2. Recommend templates only to hosts whose cells mostly import heavy modules
   (zoneinfo, decimal, statistics, tomllib). Use fresh sessions for
   codemode-like workloads.
3. Propose the decoded-module cache as an engine change for long-lived,
   multi-session hosts. It is small (under 1.4 KB on every target) and
   fits the budgets. It needs its memory accounting finished and an owner
   decision on `std` in `mm-engine` or a cache held by the facade's
   `Engine`. It does nothing for plak's current process model, so it is
   not urgent.
4. No new facade helper: `SessionTemplate` already exists, and the rest is
   rules a helper cannot know (which names to delete, whether `random` was
   loaded, which clock the host uses).

## 1. Cost of a session: fresh imports against a template restore

Native x86-64, release, `taskset -c 2`. A fresh operation is `Session::new`,
the zone source, then an import cell that loads the set and deletes the
names. A restore is `Engine::restore` of that session's image. Ir comes from
callgrind differencing: steady = (Ir of 11 operations − Ir of 1) / 10.
Wall time is the median of 51 in one process. RSS is (after − before) / N
while holding N sessions (`measure.py`). All [M]:

| Set | Ir fresh | Ir restore | wall fresh | wall restore | image | zstd-1 | zstd-1, startup prefix | RSS / session fresh, restore |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| empty | 0.49M | 0.37M | 34 µs | 25 µs | 40,796 B | 7,767 | 19 | 75, 64 KiB |
| json | 1.82M | 0.80M | 169 µs | 59 µs | 52,230 B | 11,777 | 4,354 | 110, 97 KiB |
| codemode: json, asyncio, re, collections, datetime | 40.5M | 22.9M | 4.87 ms | 2.62 ms | 517,238 B | 125,343 | 119,450 | 1,817, 1,792 KiB |
| codemode + zoneinfo, calendar | 84.5M | 38.7M | 11.85 ms | 4.37 ms | 796,582 B | 199,261 | 194,726 | 3,175, 3,127 KiB |
| heavy: decimal, statistics, zoneinfo, html.parser, tomllib | 213.0M | 59.9M | 30.6 ms | 7.10 ms | 1,215,760 B | 289,074 | 284,614 | 5,470, 5,322 KiB |

- **First operation in a new process** (plak's worker model): a restore is
  3.0M Ir for the empty template, against 0.37M steady, and 647 µs, against
  147 µs for a fresh session. Most of the 2.6M Ir gap is hashing the build
  id once per process [E]. A codemode-set restore is 3.9 ms cold
  against 6.2 ms fresh [M].
- **Where a restore goes** (callgrind, codemode set) [M]:
  - `StdModule::decode` 12.6M of 22.9M: the same bundled modules' bytecode
    the imports decoded, unpacked again;
  - `Code::verify` 1.5M;
  - `Heap::check_objects` 1.4M;
  - the checksum 0.7M;
  - dropping the session 2.1M.

  Fresh imports spend the same 12.6M in decode. Heavy set: decode is 39.5M
  of a 59.9M restore.
- **RSS** is the same either way: each instance is a full copy of the heap
  and the code. Templates save CPU, not memory.
- **Images** are 0.5–1.2 MB for real import sets and compress about 4.2×
  with zstd-1. The startup-image prefix (scope decision 33) hardly helps
  here (125 KB → 119 KB), because the base is a small part of a template.
  zstd-1 decode adds 0.6 ms (codemode set) to 1.5 ms (heavy) when the
  template is stored compressed [M]. Keep templates uncompressed in memory.

## 2. The codemode suite from templates

`conformance/codemode/runner` gained an opt-in `MM_CODEMODE_TEMPLATE` (an
image path): setup restores it instead of calling `Session::new`, sets the
fuel budget again, then defines `_ptc` and runs the tool bootstrap as
before. With it unset, nothing changes. `MM_CODEMODE_WARM` times a second
setup in the same process (a long-lived host). Engine time is the runner's
own counter: time inside `start`/`resume`, which includes every import a
cell makes. 1,708 census solutions ran without an error in every
configuration, one at a time on a pinned core (`codemode.py`). All [M]:

| Cells | start | engine p50 / p90 / p99 ms | setup p50 ms | setup + engine p50 / p90 / p99 ms |
|---|---|---:|---:|---:|
| all (1,708), warm | fresh | 2.02 / 12.83 / 26.13 | 0.09 | 2.13 / 12.94 / 26.24 |
| | json template | 1.89 / 12.93 / 25.43 | 0.18 | 2.08 / 13.11 / 25.61 |
| | codemode template | 1.14 / 10.96 / 22.89 | 3.35 | 4.63 / 14.31 / 26.99 |
| | codemode + tz template | 1.55 / 10.10 / 25.47 | 6.17 | 7.94 / 16.38 / 32.43 |
| dt-* (148), warm | fresh | 4.15 / 15.12 / 20.46 | 0.09 | 4.25 / 15.22 / 20.67 |
| | json template | 3.96 / 14.92 / 18.84 | 0.18 | 4.14 / 15.10 / 19.03 |
| | codemode template | 1.37 / 10.53 / 13.69 | 3.37 | 4.93 / 13.81 / 17.01 |
| | codemode + tz template | 1.86 / 4.58 / 13.55 | 6.25 | 8.38 / 11.02 / 19.72 |
| all (1,708), new process | fresh | 1.92 / 12.64 / 25.40 | 0.23 | 2.17 / 12.86 / 25.88 |
| | json template | 1.77 / 12.45 / 24.94 | 0.82 | 2.64 / 13.20 / 26.04 |
| | codemode template | 1.06 / 10.47 / 23.89 | 4.63 | 5.80 / 15.06 / 29.50 |
| | codemode + tz template | 1.51 / 10.00 / 24.82 | 7.61 | 9.24 / 17.89 / 32.78 |
| dt-* (148), new process | fresh | 3.60 / 14.70 / 17.49 | 0.24 | 3.83 / 14.94 / 17.80 |
| | codemode + tz template | 1.80 / 3.91 / 13.39 | 7.69 | 9.58 / 11.94 / 21.37 |

- The template does what it promises inside the cell. Engine p50 falls 45%
  with the codemode set. dt-* engine p90 falls from 15.1 to 4.6 ms once
  zoneinfo is preloaded.
- Setup grows by more than that. The median solution imports `json` and
  perhaps `asyncio`, about 1 ms of work, and restoring 517 KB costs 3.3 ms.
- Per-cell total is worse at p50 for every broad template and about equal
  at p90/p99 over all cells. It is better only at the tail of dt-* cells:
  warm p90 goes from 15.2 to 13.8 ms (codemode template) and 11.0 ms
  (codemode + tz), and p99 falls too. In a new process, dt-* p99 is worse
  (17.8 to 21.4 ms with codemode + tz).
- A `json`-only template is neutral warm (2.13 against 2.08 ms) and worse
  in a new process (2.17 against 2.64 ms), where the first restore pays its
  one-time cost.
- Engine p50 is higher with the larger template (1.55 against 1.14 ms)
  although more is preloaded. A plausible cause is that collections mark a
  larger live heap [E, not profiled].

## 3. Correctness

**Behaviour** [M]:

- **Codemode:** all 1,793 solutions (probes included) matched a fresh
  session from each of the json, codemode and codemode + tz templates: outcome,
  error, checker verdict, tool calls, batches and stdout.
- **Own cases:** all 198 matched fresh imports from the codemode and heavy
  templates (output and how the cell ended; 45 end in the same error both
  ways).
- **Wasm:** a template written natively restored on both wasm targets and
  ran a cell correctly.

**Rule 1: the setup cell must not leave names behind.** The first codemode
run used a setup cell of plain `import json, asyncio, re, collections,
datetime`. 24 solutions then differed. For example, solutions that use
`asyncio.gather` without `import asyncio` passed from the template, while
fresh sessions and CPython raise `NameError` [M]. Writing the setup as
`import json, asyncio, …` followed by `del json, asyncio, …` keeps the
modules loaded (a later `import` finds them) and the globals clean. With
that change, all 24 matched.

**Rule 2: reseed `random` if the setup loaded it.** Sessions from one
template share the generator state captured in it. Two instances of the
heavy template (statistics imports random) drew the same
`random.random()` (both 0.0832316278664722 in `leak-heavy.json`), while
two fresh sessions differ [M]. A `random.seed()` cell after instantiating draws from the session's
entropy and gives each instance its own sequence [M]. `os.urandom` is not
affected: it reads the entropy callback at call time. `hash()` is the same
in every session anyway [M].

**Rule 3: capture with the clock the instances will use.** `time`
(imported by `datetime`) records two clock readings at import: the zero of
`process_time()` and the floor of `monotonic()` [S
`crates/mm-stdlib/python/time.py`]. Both are frozen into the template [M]:

- With the system clock, an instance's `process_time()` counts from the
  capture: 3.06 s for a template 3 s old, where a fresh session starts near
  0.
- An instance whose clock is earlier than the capture's sees `monotonic()`
  stand still at the capture's reading. The codemode runner pins its clock
  to 2026-03-15, so its templates were captured with that pinned clock.

**Isolation** [M]:

- A mutation in one instance (module attributes, `json.dumps = None`, class
  attributes, `random.seed`, globals) was invisible to an instance made
  before it and to one made after it.
- Eight threads instantiating one `Arc<SessionTemplate>` 200 times, each
  mutating its instance, saw only clean instances.

**Reapply list.** Each setting was set on the setup session before capture
and then probed in an instance, next to a control session with the same
setting (`tpl settings`) [M].

Not in the image, so reapply after every `instantiate`/`restore`:

| Setting | after restore |
|---|---|
| `set_output` / `set_output_streams` | none: output is dropped |
| `set_text_cut` | `TextCut::Head` |
| `set_int_bits_text` | off |
| `set_gc_stress` | off [S `mm-engine` snapshot.rs] |
| `set_wall_clock` | the system clock (none on `wasm32-unknown-unknown`) |
| `set_entropy` | OS entropy (none on `wasm32-unknown-unknown`) |
| `set_zone_source` | none: zones loaded before capture still work, new keys fail |
| `set_deadline` / `set_deadline_with` | none |
| `park_handle` | a new flag: take the handle again |
| `set_fuel` | **kept as the fuel left at capture**: set the per-cell budget again |

Kept in the image (set them before capture, or again after): module
dunders, `set_last_expr_value`, `set_host_args`, `set_text_limit`,
`set_hint`, `set_local_timezone`, zones already loaded, host modules
(`define`), and the limits. The recursion depth stays as captured, and the
memory limit is the smaller of the image's and the restoring engine's. On
mm-wasm, `mm_session_restore` applies only the options' limits and the
clock, entropy and zone switches [S `mm-wasm-host`]. Set the rest through
the exports again.

**Build ids (U1)** [M]:

- A template image carries its build id at bytes 10..42 like any snapshot.
  An image from another build fails with `SnapshotError::Incompatible`, and
  `mm-wasm-host`'s `Runtime::restore` picks the guest by that id
  (`Error::UnknownBuild` when it is not loaded) [S].
- A template is not session state: rebuild it from its setup code on the
  new build instead of draining the old one. Key stored templates by build
  id and setup code.
- Template bytes are not reproducible: two captures of the codemode set
  differ in 14 bytes (the clock readings above), and the heavy set in 2,495
  bytes (random's state). With clock and entropy pinned during capture, the
  images are byte-identical [M]. So do not key templates by their bytes.
- The empty and json templates are byte-identical across native and both
  wasm guests.

## 4. Wasm

`mm-wasm-host`, shipped guests (`-Cpanic=immediate-abort`, `wasm-opt -Oz`),
precompiled `.cwasm`, one instance per session, `taskset -c 2`, median of
21 [M]. Fresh is a new instance plus `mm_session_new` plus the import cell.
Restore is a new instance plus `mm_session_restore` of the native image.

| Set | unknown-unknown fresh | unknown-unknown restore | wasip1 fresh | wasip1 restore | guest memory fresh, restore |
|---|---:|---:|---:|---:|---:|
| empty | 0.15 ms | 0.78 ms | 0.15 ms | 0.85 ms | 1.8, 3.4 MB |
| json | 0.43 ms | 0.87 ms | 0.80 ms | 1.41 ms | 1.8, 3.4 MB |
| codemode | 8.55 ms | 5.01 ms | 8.94 ms | 5.33 ms | 3.3, 3.8 MB |
| codemode + tz | 18.9 ms | 8.57 ms | 19.7 ms | 8.92 ms | 4.9, 5.2 MB |
| heavy | 53.2 ms | 13.4 ms | 54.8 ms | 14.2 ms | 7.0, 7.7 MB |

- A restore through the guest has a fixed cost of about 0.6 ms over a new
  session: the image is copied into guest memory, and the input buffer
  grows the instance (3.4 MB against 1.8 MB).
- Small templates therefore lose on wasm. The codemode set gains 1.7× and
  the heavy set 4×, the same shape as native at about twice native's
  times. Precompiling the guest (`RuntimeConfig::precompiled`, the
  default) is already in these numbers. The runtime loads its `.cwasm` in
  about 5 ms, once.
- A decoded-module cache would not help here: each instance is its own
  process memory.

## 5. The engine change that would make imports cheap

**Update (2026-11-07):** landed in a different form, as an Engine-owned
cache charged once with a byte cap (HANDOFF "Scope decisions" 45,
docs/sdk.md "Embedding guarantees"). The engine crates have no static
state. A session hit is charged the decoder workspace, and the cache is
on by default. Its numbers are in
bench/evidence/results/2026-11-07-module-cache. The prototype below is
the historical record.

Prototype on a local branch (`local/template-decode-cache`, not pushed): `StdModule::decode`
keeps each decoded bundled module in a process-wide table and hands later
sessions the same `Arc<Code>`s. That is about 25 lines in `mm-engine`
behind `extern crate std`. The workspace budget is not charged on a hit, so
a real version would need the accounting, a no-`std` story for the engine
crate, and a decision on where the cache lives (probably the facade's
`Engine`). Same table as section 1, with the prototype [M]:

| Set | Ir fresh | Ir restore | wall fresh | wall restore | RSS / session restore |
|---|---:|---:|---:|---:|---:|
| codemode | 26.4M (−35%) | 9.15M (−60%) | 3.21 ms | 1.07 ms | 1,126 KiB (−37%) |
| codemode + tz | 58.4M (−31%) | 13.3M (−66%) | 8.02 ms | 1.53 ms | 1,880 KiB (−40%) |
| heavy | 169.8M (−20%) | 16.5M (−72%) | 25.0 ms | 2.64 ms | 3,242 KiB (−39%) |

- The first session in a process still pays the decode (codemode set, first
  restore 24.2M Ir).
- Behaviour was unchanged: all 198 own cases matched from the codemode
  template, and the eight-thread isolation test passed.
- Size (`cargo xtask size`, prototype against main `1f27970`) [M]:
  - `hello.wasm`: 2,078,951 → 2,079,750 B (+799; budget 2,081,000);
  - native `hello`: 2,276,888 → 2,278,264 B (+1,376; budget 2,279,000);
  - mm-wasm guest: 2,182,840 → 2,183,641 B (+801; budget 2,185,000).
- Bench Ir (`bench/callgrind.py`, `mm run`, whole process): equal to main
  to 0.1M on all 13 programs [M]; a single-session process decodes each
  module once either way.
- With the cache, a codemode-set restore (1.07 ms) costs about what the
  median cell's own imports cost, so a template would break even at p50 and
  win at the tail [E].
