# Spike: lazy prelude

The goal is to cut session startup, session memory and snapshot size by
multiples. This page measures where the prelude's cost goes and which prelude
code cells actually use, compares the options, and recommends a design with
an implementation plan. Base: `origin/main` at `4e220da`, on this orb. Labels
follow AGENTS.md: [M] measured, [S] sourced, [E] estimated. The
instrumentation and prototypes described under "Reproducing" were local
patches and were not committed.

## Answer

Load the prelude as a library of units, on demand. Build time splits
`prelude.py` into 165 units (161 top-level `def`s and 4 classes) plus a small
declarative manifest for the other top-level names. A session starts with an
empty prelude namespace. The first lookup of a prelude name decodes that one
unit, loads its code and builds its function or class in Rust. No Python
runs, and no new dispatch paths or continuations are needed. Snapshots store
only the units a session materialized, by reference, as they already do for
bundled modules.

- **Startup:** 6.70M instructions today. With no prelude at all, the same
  empty cell takes **0.80M (8.4× fewer)** [M]. A lazy session is estimated
  at:
  - about 0.85M when it touches nothing;
  - about 1.25M at the median of sessions that touch the prelude (5.4×);
  - about 2.3M at p99 (2.9×) [E].
- **Snapshot:** a fresh snapshot is 122,483 bytes today and 32,664 bytes with
  no prelude (3.75×) [M]. It drops to about 20 KB (6×) [E] once the intern
  table's start-up reservation is shrunk to match (1,024 → 256 entries).
- **Restore in a new process:** it no longer pays the 3.07M-instruction
  whole-prelude unpack and decode [M]. That is 40% of a measured 7.76M
  `mm resume`.
- **Session memory:** the prelude accounts for 188,729 bytes of each session's
  code budget and 72 KB of live heap [M]. The smallest working arena, however,
  only falls from 2,364,217 to 2,169,847 bytes (−8%) [M], because the arena
  always commits two 1 MiB segments (2,162,688 bytes) [M]. **Multiples in
  memory need a companion change in `mm-core`:** first segments that start
  small and grow. Estimated floor after both: 0.1–0.25 MB [E].
- **Size:** estimated +2 to +5 KB net [E], against 8.9 KB (Wasm) and 9.3 KB
  (native) of headroom under the size gates. This is the main risk; the plan measures it before
  anything else lands.

## Measurements

### Where startup goes

`mm run bench/startup.py` (`print(0)`), callgrind, release build with symbols
(`strip=none`, line tables; same total as the stripped build) [M]:

| Part | Instructions | Note |
|---|---|---|
| Whole process | 6,698,505 | matches the handoff log's 6.7M |
| Prelude unpack (`pack::unpack_bounded`) | 1,926,888 | 26,075 packed → 53,150 bytes; 36.3 instr/byte |
| Prelude decode (`Program::decode_bounded`) | 1,139,802 | 219 code objects, 650-string pool (`shareds` 0.64M) |
| `load_in` for the prelude | 1,185,379 | interning 1,047 names and 413 consts; 5,413 per code |
| Prelude module body (`Vm::run`) | 971,475 | 212 functions, 4 classes, dicts, struct seqs |
| First cell (`Session::start`) | 653,762 | `reclaim_code` 0.52M and GC 0.44M, both proportional to the prelude-sized heap |

Prototypes, same program and binary, selected by environment variable [M]:

| Variant | Instructions |
|---|---|
| Today | 6,694,954 |
| Prelude decoded (process-wide `OnceLock`) but not loaded or run | 3,878,807 |
| No prelude decoded, loaded or run | **798,540** |

`print(0)` needs no prelude, so the last row is the floor for a session that
touches none. The facade decodes the prelude once per process. The CLI and the
cell worker start one session per process, so the decode is never shared in
the deployments that matter.

`mm resume` of a fresh snapshot in a new process (`--snapshot-every 20`): 7,764,262
instructions, of which prelude unpack is 1,926,385 and decode 1,140,342
[M]. `read_snapshot` (1.12M) scales with the prelude-sized heap.

### Memory and snapshots

`Vm::new` alone (builtins), then with `load_prelude`, each measured after
`write_snapshot`'s collection [M]:

| | Builtins only | With prelude | Prelude's share |
|---|---|---|---|
| Snapshot bytes | 31,829 | 121,696 | 89,867 (74%) |
| Live heap bytes | 30,656 | 102,888 | 72,232 |
| Heap objects | 237 | 1,108 | 871: 588 strings, 212 functions, 22 tuples, 11 dicts, classes, instances |
| `code_bytes()` (session code budget) | 1,498 | 188,729 | 187,231 |
| Committed arena | 2,162,688 | 2,162,688 | 0 |

`code_bytes` charges each session for the shared `Arc<Code>` buffers it holds
(docs/history/handoff-0.0.1.md, stage A), so sharing code across sessions saves real memory without
lowering the budget a session needs.

The smallest memory limit at which `Session::new` plus a `x = 1` cell
complete (binary search on `Limits::memory`, 1 KiB resolution) [M]:

| | Floor | Fresh snapshot |
|---|---|---|
| Today | 2,364,217 | 122,483 |
| No prelude | 2,169,847 | 32,664 |

The handoff log (docs/history/handoff-0.0.1.md) records 2,448,103 for "the smallest working arena"; that figure came
from a different harness, so compare within this table. Without the prelude,
the floor is 2 × `SEG_COST` (2 × 1,081,344: a 1 MiB word segment and a 1 MiB
byte segment, each with two bitmaps) plus 7 KB. **The prelude is 8% of the
floor; segment granularity is 92%.**

The 32,664-byte no-prelude snapshot still holds the intern table, sized by
`Vm::init`'s `reserve_interned(1024)` at 2,049 words (16.4 KB). The builtins
alone intern about 130 strings.

### What cells touch

Instrumentation counted, per session, every prelude function entered
(`enter_frame`, plus generator and coroutine creation in `call_value`) and
every prelude global read (`load_global_slow` in prelude code, `prelude_fn`,
the twin lookup in `builtins.rs`, and `json`/`sys` module building). Counting
started after `load_prelude`, so the module body is excluded. All 870 files in
`conformance/monty`, `conformance/own` and `bench` were run with
`mm exec --json --host-fixtures`. 842 sessions were recorded; the other 28
are compile-error cases plus one abort, noted under "Out of scope". A
function's unit is its top-level name (`GenericAlias.__new__` →
`GenericAlias`) [M]:

| Per session | Value |
|---|---|
| Sessions touching no unit | 606 of 842 (72%) |
| Units touched: median / p90 / p99 / max | 0 / 9 / 26 / 58 (of 165) |
| Among touching sessions: median / p90 | 6 / 19 |
| Encoded bytes of the touched units: p90 / p99 / max | 3,439 / 10,045 / 16,586 (of 54,521) |
| Units touched by any session | 138 of 165 |
| bench: fib, loop, closure, dict, str, startup | none |
| bench: json_rows | `dumps`, `loads`, `dump`, `load`, `_json_kw`, `JSONDecodeError` (read when `json` is imported) |

Most frequent, in sessions out of 842 [M]: `_list` 156, `_class_getitem` 92,
`map` 79, `_map_gen` 78, `_khash` 63, `_bx_call` 54, `_dfind` 53, `_dset` 53,
`_tuple` 41, `_sort` 39, `_sorted` 38, `_op_fmt` 33, `GenericAlias` 25. The
four `sys` struct seqs are read in 75 sessions, all because `import sys`
copies them eagerly in `build_bundled`.

The corpus is feature tests, not agent cells, so it probably overstates how
much of the prelude a typical cell needs. The shape of the result is what
matters: most cells touch nothing, and the rest touch a handful of small units.

### Packing granularity

Prelude composition [M]: 219 code objects; 35,579 bytecode bytes; 98,491
`buffer_bytes`; 2,234 inline-cache slots; 650 distinct strings (8,152 bytes).
The module body is 6,487 encoded bytes (3,893 packed). Each unit is a
contiguous pre-order slice of the code table: a `def` or class code followed
by its nested codes.

Sizes of the 165 units, each encoded as its own `Program` with its own string
pool (54,521 bytes in total) [M]:

| Packing | Bytes | vs today (26,075) |
|---|---|---|
| Whole program, one stream (today) | 26,075 | — |
| All units, one stream | 23,354 | −2,721 (no module body) |
| Each unit packed alone (158-byte header each) | 55,063 | +28,988 |
| One shared Huffman table, LZ restarted per unit | 31,246 | +5,171 |
| Shared table, 8 / 16 / 24 / 48 chunks in source order | 26,081 / 26,800 / 27,919 / 28,814 | +6 / +725 / +1,844 / +2,739 |

The shared-table rows come from a scratch `pack::shared_table_size` that
reuses the packer's tokenizer and optimal parse. One stream ordered by touch
frequency would cost nothing in size. It still needs a 29 KB prefix unpacked
at p90 and 46 KB at p99, because the tail of rarely used units is long, so it
loses to chunks.

Bytes a session must unpack, for source-order chunks [M from traces]:

| Chunks | Unpack p90 | Unpack p99 | Median among touching sessions |
|---|---|---|---|
| 165 (one per unit) | 3,439 | 10,045 | 2,455 |
| 42 | 6,626 | 20,574 | 5,292 |
| 24 | 10,424 | 26,538 | 7,984 |
| 15 | 15,884 | 30,722 | 11,526 |
| 1 | 54,521 | 54,521 | 54,521 |

Unpacking is the dominant per-byte cost (36 instructions per byte, against 21
to decode).

## Options and what each saves

Estimates [E] are derived from the measured per-part costs above.

| Option | Startup | Session memory | Snapshot | Size | Verdict |
|---|---|---|---|---|---|
| A. Share decoded code across sessions (`Arc<Code>`, done in stage A) | 0 for one session per process; −3.07M per later session in a multi-session process | none (each session is charged) | none | 0 | Keep; does not help the CLI or worker |
| B. Lazy decode and load, objects still built eagerly | cannot: the module body needs every function | — | — | — | Not viable alone |
| C. **Lazy units, built in Rust on first lookup** (recommended) | 6.70M → about 0.85M with no units touched; about 1.25M at the median touching session; about 2.3M at p99 | −187 KB code, −72 KB heap; floor −8% | 122 KB → about 20–33 KB | +2 to +5 KB | Recommended |
| D. Prelude objects in a shared read-only region | as C, plus an image to map | −72 KB per extra session | needs dirty tracking | image ≥ 90 KB | Rejected: users can mutate prelude objects (`map.x = 1`, `_REPR_ACTIVE`, class versions), so it needs copy-on-write at the barrier |
| E. Eager prelude, excluded from snapshots and rebuilt on restore | 0 | 0 | 122 → 33 KB | 0 | Rejected: restore re-runs about 6M instructions, user objects referencing prelude objects need address-identical rebuilds, and mutated prelude objects must be detected |
| F. Embedded heap image | most of 6M | 0 | 0 | +90–122 KB | Rejected in the handoff log (stage B); the size budget rules it out |
| G. Store the prelude unpacked | −1.93M | 0 | 0 | +27 KB | Rejected on size; C gets most of the gain |
| H. Small first arena segments (companion, `mm-core`) | small | floor 2.17 MB → about 0.1–0.25 MB | 0 | about +1 KB | Needed for memory multiples; independent of the prelude |

C is the only option that improves all three targets. It also makes the
choice among A, E and F moot: what a session never touches is never built,
charged or written.

## Recommended design

### Units and the manifest (build time)

`build.rs` keeps compiling the whole prelude, as today, and additionally
splits it into units:

- **Function unit**: a top-level `def`. Its encoding is a small `Program`
  whose code 0 is an empty stub and whose code 1 onward is the `def` and its
  nested codes. Each unit is compiled by passing the existing compiler a module
  containing only that statement, so `MAKE_FUNCTION`'s `module_base + child`
  rule resolves nested functions without any bytecode rewriting. The manifest
  stores the defaults and keyword defaults as `Konst`s. `build.rs` fails if a
  default is not a literal, or if a top-level `def` is decorated (there are
  none today).
- **Class unit**: a top-level class whose body contains only `def`s (plain,
  `@property`, `@staticmethod` or `@classmethod`), literal assignments and
  `NAME = None`. The manifest stores the class name, the base names, and the
  `__module__` the body would have seen. That value is the `__name__` in
  effect at that point in `prelude.py`: `'json.decoder'` for
  `JSONDecodeError` and `'sys'` for `_StructSeq`; `GenericAlias` and `Union`
  set their own. The manifest also lists namespace entries in body order, each
  either a method (a code index in the unit) or a constant. The class-body code
  is not stored. All four classes qualify today, given one rewrite below.
- **Other bindings**, each a manifest entry built in Rust:
  - `Const` (a fresh literal per session): `_SYMS`, `_REPR_ACTIVE`,
    `_host_records`.
  - `Alias`: the three `_frozenset_*`.
  - `Type(T)`: `_KEYS`, `_ITEMS` and `_NoneType`, written in source as a
    recognized `type(<literal>)` form, or replaced by engine-installed names.
  - `Builtins` tuple: `_MATCH_SELF`.
  - `Instance` (an instance of a class unit, with literal attributes): the
    four `sys` struct seqs.
  - The final `__name__`.
  - `Native`: the 27 `_prelude` natives and `_MISSING`.
- **Rewrites in `prelude.py`**:
  - `_StructSeq.__name__ = 'structseq'` becomes a manifest rename, or is
    recognized as one.
  - The four `_StructSeq(...)` calls become `Instance` entries. `build.rs`
    accepts `NAME = C(<literals>)` only when `C.__init__` just stores its
    parameters on `self`. The alternative is a subclass per struct seq with
    class-level `_n`, `_f` and `_t`, materialized with `new_instance`.
  - `build.rs` rejects any other kind of top-level statement, so the prelude
    stays declarative.
- **Equivalence check at build time**: every unit code must equal the
  matching code of the whole-program compile, the same `Code` up to child
  indices, and the manifest must bind exactly the names the module body binds.
  The whole-program encoding is also emitted for a `cfg(test)` equivalence test
  (below). It never enters a shipped binary.
- **Packing**: units go into chunks of about 2–4 KB unpacked (24 is the
  starting point), packed with one shared pair of Huffman tables and an LZ
  window restarted per chunk. Units are ordered by co-usage measured from the
  corpus traces, so units used together share a chunk. This needs one new
  function each way in `mm-compile/src/pack.rs` (outside the core rewrite):
  `pack_chunks` and `unpack_chunk`. The manifest and the unit index (sorted
  names in one `&str` with `u16` offsets, then kind, chunk, offset and length)
  are generated Rust statics, so a lookup needs no unpacking.
- **Build id**: `build_fingerprint` hashes the packed chunks and the manifest
  instead of `prelude.bin`.

### Runtime resolver

All prelude access goes through one resolver, `Vm::prelude_get(name) ->
Result<Option<Value>, Raised>`, in a new file `crates/mm-engine/src/prelude.rs`:

1. Check `prelude_globals`.
2. If the name is not there, binary-search the unit index. If it is absent,
   return `None`; the callers' existing fallbacks for dynamic twin names stay
   as they are.
3. Materialize the unit:
   1. Unpack its chunk, cached per session in admitted scratch (the same
      pattern as `std_scratch`).
   2. Decode its `Program`.
   3. `load_in(.., GSLOT_PRELUDE)` and record `(unit → first code index)`.
   4. Build the value in Rust:
      - **Function:** the same object `MAKE_FUNCTION` builds, through one
        helper shared with `interp::make_function`. It sets name, qualname,
        the defaults tuple, the kwdefaults dict, no closure, the prelude
        globals and the code index.
      - **Class:** functions, `property` objects and constants into a dict,
        then `new_class(name, bases, ns)`. `build.rs` checks that no
        metaclass or `__init_subclass__` hook applies, so `_finish_class`
        never runs.
      - **Instance:** `new_instance` plus attribute stores.
      - **Const, Alias, Type, Native:** a direct value.
   5. Insert the value into `prelude_globals`.
4. Return the value.

Callers that change: `prelude_fn` (and through it `prelude_request`), the
twin lookup at `builtins.rs:1730`, `build_bundled` for `json` and `sys`
(`modules.rs:108, 142`), and `load_global_slow` when the running code's
`gslot` is `GSLOT_PRELUDE`. For prelude code the lookup order becomes:
prelude globals, then the unit index, then builtins.

`map`, `filter` and `open` must look like builtins from the first cell. On a
builtins miss in `load_global_slow`, a three-entry check materializes them and
inserts them into `builtins`. The builtins dict is reachable from guest code
only through name lookup today: there is no `builtins` module, no
`__builtins__` and no `__globals__`. Step 5 of the plan must re-check that
before relying on it. If that cannot hold, materialize the three eagerly,
which costs about 0.15–0.4M [E].

`Session::new` drops `load_prelude`. It only creates `prelude_globals`, as
`Vm::init` already does, and binds the final `__name__`.

### Invariants

- **No second dispatch loop, no new park points.** Materialization runs no
  Python. It is Rust that allocates, inside whichever native or slow path
  asked, exactly like `start_std`'s decode and load. Allocation only sets "GC
  requested", so no collection can run while the resolver holds `Value`s.
- **Atomic or nothing.** A unit is fully loaded and bound, or, on
  `MemoryError` or a corrupt chunk, the code table, ICs, sources and unit
  record are truncated back. This follows `load_in`'s existing rollback.
  Near the memory limit, an operation that needs a twin can now raise
  `MemoryError` where today the twin was preloaded. That is deterministic, so
  durability is unaffected, and it is documented.
- **GC.** `prelude_globals` stays a root. Every materialized value is
  reachable from it and is never collected. Unit codes are pinned: unit codes
  are exempt from `reclaim_code`, as the prelude range is today. On
  compaction, the unit records are remapped along with `std_codes`
  (`lifecycle.rs:292`). Interned names stay alive through the code objects'
  traced `consts` and `names`.
- **Inline caches.** Only prelude code reads `prelude_globals`, and the set
  of unit names is fixed at build time. Prelude code resolves unit names
  before builtins, so an `Ic::Builtin` in prelude code is only ever created
  for a name no unit can later define. Materialization therefore never makes
  a cached resolution wrong. Inserting a key into the flagged globals dict
  still bumps `gver`, as today, because the dict's entries buffer may move.
  The cost is one re-resolution per cached site per materialization, bounded
  by the number of units a session touches. Unit ICs start empty, as on every
  load.
- **Fuel.** Materialization charges no fuel, as the module body ran with fuel
  off. If a charge is wanted, it must be a pure function of the unit.
- **Tracebacks and recursion.** Unit codes keep `prelude: true`, so their
  frames stay hidden and are excluded from `too_deep`'s user count.
- **Equivalence.** A `cfg(test)` test materializes every name and compares it
  with the eager result of the embedded whole-program encoding: kinds,
  qualnames, code (`Code` equality), defaults, class MRO, namespace keys and
  values, flags, and struct-seq attributes. A guard test confirms that the
  string-literal names passed to `prelude_fn` and `prelude_request` all
  resolve.

### Durability

A snapshot's heap holds exactly the units the session materialized, with
`prelude_globals` binding them. A unit not yet materialized at snapshot time
is materialized after restore at the same program point at which the unparked
run materializes it. Materialization is a pure function of the build and the
unit, and its trigger is a lookup that the program reaches deterministically.
Parked, restored and unparked runs therefore compute the same values. Object
addresses can differ, as they already do, because writing a snapshot runs a
collection. The new rules are:

- Restore re-decodes each referenced unit from the binary (same build only,
  enforced by `build_id`), the way `CODE_STDLIB` is handled.
- Snapshots are only written at safepoints with no pending request, and
  materialization happens entirely inside one native step. So no snapshot can
  observe a half-loaded unit.
- `cargo xtask durability` covers this unchanged: it parks every few fuel
  units and at every host call, and resumes in a fresh `mm` process. Two own
  cases are added:
  - one that parks before its first use of several units in different
    chunks, so they materialize only after restore;
  - one that materializes units, parks, and uses them again after restore.

### Snapshot format (format 1 → 2)

- Header: `SNAPSHOT_FORMAT` becomes 2. The prelude's change of `build_id`
  already rejects old snapshots as `Incompatible`.
- VM section, code table:
  - The `(pfirst, pend)` prelude range is removed and `tramp` stays.
  - Added after `std_codes`: `unit_count u32`, which must equal the build's
    count, and then `(unit u32, first u32)` for each loaded unit, in load
    order.
  - A `CODE_PRELUDE` entry becomes `(unit u32, at u32)`, with `at`
    counting from the unit's stub code. On read it is checked as
    `first + at == ci && at < unit_len`, the unit is decoded once (keeping
    one decoded unit at a time, like `std_prog`), and its buffers are charged
    to the restore budget.
  - Module base, source, the prelude flag, `gslot`, `consts` and `names` are
    written per entry as today.
- Fixed registers: unchanged, including `prelude_globals`.
- `Vm::fixed_bytes` adds the per-unit record table.

Spec §9.3 ("As built") and §7.2's code-object row are updated in the same
change.

### Companion: small first segments (`mm-core`)

Laziness cannot move the floor below two committed segments (2,162,688
bytes). The first word segment and the first byte segment should start small
(about 32–64 KiB) and grow by reallocation, doubling up to `SEG_UNITS`.
Addresses are `segment << 17 | unit`, so growing a segment's backing store
does not change any `Ref`, and the arena stays non-moving at the Python
level. Dedicated large segments already prove that segments of other sizes
work. Bitmaps grow with their segment. A FULL snapshot already writes runs,
not whole segments. The cost is one copy of at most 1 MiB per space over a
session's life. The floor then tracks the live heap: about 30 KB of builtins
plus the intern table plus the code tables. Estimated floor 0.1–0.25 MB [E].
This is its own phase in `mm-core` (`space.rs`, `heap.rs` accounting,
`snapshot.rs` sizes), not in the files the core rewrite owns. It should land
after the prelude work, so each change's effect on the floor is measured
separately.

## Implementation plan

The core rewrite owns `interp.rs`, `vm.rs`, `generator.rs`, `aio.rs`,
`snapshot.rs`, `lifecycle.rs` and mm-compile's bytecode, compiler and encode
files. This plan keeps new logic in `mm-engine/src/prelude.rs`,
`mm-engine/build.rs`, `mm-compile/src/pack.rs` and `prelude.py`. It touches
the rewrite's files only through a few hooks: the code-table load call, a
shared `new_function` helper, the snapshot code-entry kind, and the remap
callback. Steps 3–5 land after the rewrite, or are rebased onto it.

1. **Make the prelude declarative (`prelude.py` only).**
   - Rewrite the struct-seq construction, the `_StructSeq.__name__` rename and
     the three `type(...)` bindings into the accepted forms. Eager loading
     still runs the module body.
   - Checks: `cargo xtask conformance --engine micro-monty --check` unchanged,
     `cargo xtask durability`, `cargo test --workspace`.
2. **Split, manifest and chunked packing (`build.rs`, `pack.rs`).**
   - Generate the unit index, the manifest and the chunks, with the
     build-time equivalence and subset checks. Keep emitting `prelude.bin`
     for now.
   - Unit tests for `pack_chunks`/`unpack_chunk`: round trip, bounded
     unpack, truncated input.
   - Measure with `cargo xtask size`, temporarily linking the new statics in.
     Decision point: if the net size cost exceeds the headroom, change the
     chunk count or move the index into a packed chunk before going further.
3. **Resolver and materializer, eager mode (`prelude.rs`, hooks).**
   - `Session::new` calls `materialize_all()` in manifest order instead of
     `load_prelude`. Behaviour is unchanged by construction.
   - Add the `cfg(test)` equivalence test.
   - Checks: `cargo xtask check` (all ten gates), and
     `cargo xtask gc-stress`, because materialization allocates while
     natives hold values.
4. **Snapshot format 2 (`snapshot.rs`, `lifecycle.rs` remap, spec §9.3).**
   - Checks: snapshot round-trip tests, the corrupt-input tests (extended to
     cover bad unit ids, overlapping units and `at` out of range), the
     snapshot fuzz target's seeds, `cargo xtask durability`, and
     `cargo xtask wasm` round trips.
5. **Go lazy.**
   - Drop `materialize_all` from `Session::new`. Add the builtins hook for
     `map`/`filter`/`open`, after the re-check that guest code cannot
     enumerate `builtins`.
   - Lower `reserve_interned` to about 256, measured from the builtins plus
     typical units.
   - Remove `prelude.bin`, `prelude_program` and the facade's `OnceLock`.
   - Re-baseline the memory-window tests: `sorted_json_entries_share_the_memory_budget`,
     `completion_preflight_releases_its_budget_before_rendering`, and
     `verify_bytecode`'s budget test.
   - Add the two durability own cases.
   - Checks: `cargo xtask check` and `cargo xtask gc-stress`.
   - Remeasure with the "Reproducing" commands: startup instructions, unit
     touches on the corpus, the arena floor and the fresh snapshot size.
     Record the results in HANDOFF.
6. **Optional: process-wide chunk cache.** For embedders that run many
   sessions per process, the facade provides an unpacked-chunk cache (std
   `OnceLock` per chunk) through a `&'static dyn` in `VmConfig`. The no_std
   default stays per-session. Measure with `examples/session_bench.rs`.
7. **Small first segments (`mm-core`)**, as above. Checks: mm-core's property
   tests and fuzzers, `cargo xtask check`, `cargo xtask gc-stress`, and the
   floor and snapshot measurements again.

## Risks and open questions

- **Size budget.** The estimate is +2 to +5 KB net [E]: chunking +0.7 to
  +1.8 KB, the index and manifest about +3 KB of rodata, materializer code
  about +3–5 KB, offset by removing the 3.9 KB module body and
  `load_prelude`. Headroom is 8,857 bytes on the primary gate (Wasm
  `hello.wasm` after `wasm-opt -Oz`, 1,721,143 under 1,730,000, as of
  `223030f`) and 9,272 on the native gate (1,826,728 under 1,836,000). The
  Wasm effect must be measured after `wasm-opt`. Step 2 measures this before the engine
  work starts. Fallbacks: fewer chunks, or the index inside a packed chunk.
  The latter makes the first touch unpack about 3 KB more (about 0.1M) [E].
- **Typical cells may touch more than the corpus.** Agent cells use `sorted`
  with `key=`, `map`, and f-strings on user objects. The p99 estimate (about
  2.3M, 2.9× better) bounds the heavy case. After step 5, rerun the trace on
  plak's worker tests (`docs/research/plak-integration.md`) to check.
- **Declarative subset drift.** A future prelude edit that adds imperative
  top-level code fails the build. That is intended, and the error must say
  which statement and which forms are accepted.
- **Class materialization correctness.** Rust-built classes must match what
  the class body would have produced: `__qualname__`, `__module__`, the order
  of the namespace dict, `property` objects, class flags and version tags.
  The equivalence test covers each of these. If `Union`'s `@property` handling
  turns out awkward, `Union` alone can become a bundled Python module
  imported by its twins.
- **Behaviour near the memory limit.** First use of a twin can raise
  `MemoryError` near the limit (see Invariants). This is deterministic, and
  much less memory is committed at start.
- **Conflict with the core rewrite.** The hooks are small, but steps 3–5
  touch the same files. Land them after the rewrite merges.
- **Out of scope, found while measuring.**
  `conformance/monty/recursion__instance_repr.py` overflows the Rust stack
  and aborts with the default recursion limit (1000, also at 3000 and 10000).
  It passes at 500 and below, and the conformance runner uses 51, so the gate
  does not see it. It was reported separately as a suggested fix; it is
  unrelated to the prelude.

## Reproducing

The instrumentation was local and not committed. To repeat it:

- **Touch trace.** Add to `Vm` an optional pair of name → count maps. Record
  a code's qualname when `enter_frame` (or `call_value`'s generator/coroutine
  branch) enters a `gslot == GSLOT_PRELUDE` code. Record the name in
  `load_global_slow` for prelude code, in `prelude_fn`, in the twin lookup
  (`builtins.rs`, before the `_<name>` probe) and in `build_bundled`'s
  `json`/`sys` reads. Enable it after `load_prelude` in `Session::new` when
  `MM_PTRACE=<file>` is set, and append the maps to the file in a
  `Drop for Session`. Then run
  `for f in conformance/{monty,own}/*.py bench/*.py; do MM_PTRACE=out/$(basename $f) mm exec --json --host-fixtures $f; done`.
- **Startup.**
  `CARGO_PROFILE_RELEASE_STRIP=none CARGO_PROFILE_RELEASE_DEBUG=line-tables-only cargo build --release -p mm-cli`,
  then `valgrind --tool=callgrind mm run bench/startup.py` and
  `callgrind_annotate --inclusive=yes`. The no-prelude rows replace
  `vm.load_prelude(prelude)` in `Session::new` with nothing (and the decode
  with an empty `Program`) behind environment variables.
- **Memory and snapshots.** An `mm-engine` example builds `Vm::new` with and
  without `load_prelude`, calls `write_snapshot`, and reads `heap.stats()`,
  `code_bytes()` and an `ObjectCursor` census. The floor is a facade example
  that binary-searches `Limits::memory` for `Session::new` plus `start`
  returning `Complete`.
- **Packing.** Split `prelude_program()` at codes whose qualname has no `.`.
  `Program::encode` each slice, then `pack::pack` them alone, concatenated
  and grouped. The shared-table rows use a scratch function that runs `pack`'s
  `tokens`/`matches`/`optimal` per chunk, with code lengths from all chunks'
  tokens, and sums the byte-aligned bit counts plus one 158-byte header.
