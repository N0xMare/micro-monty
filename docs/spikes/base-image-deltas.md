# Spike: shared base image and delta snapshots

The goal is to cut snapshot bytes, restore time and session fork cost by
multiples for long sessions that park many times, as agent sessions do. Today
every snapshot is a FULL image of the session: its heap (prelude units,
loaded stdlib objects, user data) and its VM registers. This page measures
what those images contain and how much of each one changes between parks,
compares ways to store less, and recommends a design with an implementation
plan. Base: `origin/main` at `54ea791`, on this orb. Labels follow AGENTS.md:
[M] measured, [S] sourced, [E] estimated. The probes were local and were not
committed; "Reproducing" says how to repeat them.

## Answer

Write **deltas against the parent snapshot**, with the dirty set recorded by
mm-core's setters at 64-byte granularity, and make the **base image** the
reproducible root of every chain: the image `Session::new` produces, which
is identical in every process of a build. Do not share base memory between
sessions yet; at today's base size (35 KB) a memcpy is cheaper than
copy-on-write bookkeeping.

- **Most of a snapshot is unchanged since the previous park.** On a 38-park
  agent-like session (the embedding workload, hibernating after every cell),
  FULL images total 1,970,366 bytes, median 50,138 per park [M]. Only the
  changed words, written as runs against the parent image, total 136,586
  bytes (14.4× less), median 192 bytes per park [M]. At 64-byte dirty-line
  granularity, which the write barrier can record with one bit per line,
  they total 217,524 bytes (9.1×), median 308 bytes [M]. Both count an
  estimated 117-byte header per park.
- **A delta against the base alone is not a multiple.** Against the fresh
  `Session::new` image, the same parks total 769,270 bytes (2.6×), median
  18,232 [M]. The prelude is already lazy and bundled code is stored by
  reference, so the static part of a session image is small: a fresh session
  snapshots to 35,314 bytes [M]. Most of every image is the session's own data,
  which only a parent-relative delta avoids rewriting.
- **Host-side deduplication (spec §9.3's first answer) gets 2.7–4.7×.**
  Content-addressed fixed 4 KiB pages of the FULL images store 733,246 bytes
  (2.7×); content-defined chunks of about 512 bytes store 423,524 bytes
  (4.7×) [M]. FULL images do not keep objects at fixed offsets (live runs are
  written back to back), so a small change shifts the rest.
- **dquickts' prefix/suffix patch does not fit hash tables.** The intern
  table (16,400 bytes) changes in 7–22 scattered words per park [M]. A
  prefix/suffix patch rewrites about 16 KB of it every park; word runs cost
  90–270 bytes [M]. Borrow dquickts' FULL/DELTA kinds, base id and
  "nondeterministic state stays out of the base", but encode changes as runs.
- **Restore is mostly zeroing today.** Restoring a 50 KB image takes 43.4 µs
  [M]; 62% of its instructions are `calloc`/`memset` committing and zeroing
  whole segment capacities, 14% `check_objects`, 5% the checksum [M]. Not
  zeroing what the image overwrites is an independent "do less work" fix,
  estimated at −50% [E]. Restoring base + chain from a validated in-process
  template is estimated at 5–10 µs [E].
- **Fork.** `Session::new` takes 25.0 µs and restoring the fresh image 14.3 µs
  [M]. Forking a session from an in-process template (memcpy of the segments
  plus the VM registers) is estimated at 3–5 µs for a 50 KB heap [E].

## Measurements

The workload is `crates/micro-monty/examples/embedding_bench.rs` (a paginated
host load, 32 query cells, an `asyncio.gather` batch over host futures, an
uncaught host error, a fuel park cancelled mid-loop, `reset`). It
"hibernates" (snapshot, drop, restore into a new session) at 6 points. The
long variant also hibernates after each of the 32 query cells: 38 parks. A
probe copy of the example wrote every image to disk and took a snapshot right
after `Session::new` and the host function definitions (the base).

### Today

`cargo run --release -p micro-monty --example embedding_bench` at `54ea791`
[M]:

| | Value |
|---|---|
| Snapshot bytes, 6 parks | 118,320; 53,019; 50,130; 54,249; 52,352; 40,544 |
| Snapshot (GC + write), 6 calls | 0.32–0.55 ms total (about 55–90 µs each) |
| Restore, 6 calls | 0.51–0.53 ms total (86 µs each; max 141 µs) |
| Fresh session image | 35,314 bytes |
| `Session::new`, best of 50 | 25.0 µs |
| Restore of the fresh image, best of 50 | 14.3 µs |

`bench/results/embedding.txt` still shows 187,126 bytes for the first park;
it predates the lazy prelude and small first segments.

What a fresh image holds [M]:

| Part | Bytes |
|---|---|
| Header (magic, format, build id, session flags) | 53 |
| Word space: 276 objects | 29,953 |
| — intern table (2,050 words) | 16,400 |
| — strings | 5,344 |
| — `RAW` and `BUF` (dict and list storage) | 5,448 |
| — types | 2,184 |
| Byte space: 167 blocks | 3,601 |
| VM registers, code table, sources | 1,643 |

The image is deterministic: two processes produce byte-identical fresh images
[M]. A warm base (the fresh session plus a cell that imports `asyncio`, runs
a coroutine and touches the string and dict methods the workload uses) is
41,200 bytes [M]; the deltas against it for parks 2–6 are 16,195; 13,370;
17,527; 15,556; 4,492 bytes [M], barely better than against the cold base.

### How much changes per park

A parser for the image's heap section split each image into objects by
address (segment, word); the arena is non-moving, so an object that survives
keeps its address. For each park it compared the image with the base and
with the previous park's image [M]:

| Encoding of the 38 parks | Total bytes | Median per park | vs FULL |
|---|---|---|---|
| FULL today | 1,970,366 | 50,138 | 1× |
| Host dedup, fixed 4 KiB pages | 733,246 | 13,506 | 2.7× |
| Host dedup, content-defined chunks of about 512 B | 423,524 | 5,994 | 4.7× |
| Host dedup, content-defined chunks of about 128 B | 490,901 | 8,805 | 4.0× |
| Delta vs base, new objects whole and changed objects as word runs | 769,270 | 18,232 | 2.6× |
| Delta vs parent, whole dirty 4 KiB pages | 970,500 | 8,368 | 2.0× |
| Delta vs parent, dirty 512-byte lines | 388,730 | 1,200 | 5.1× |
| Delta vs parent, dirty 64-byte lines | 217,524 | 308 | 9.1× |
| Delta vs parent, changed words as runs | 136,586 | 192 | 14.4× |

The delta rows add a 117-byte header per park [E]; the parent-relative rows
also add the VM section as a byte diff against the parent's (10,288 bytes in
total over 38 parks, 271 on average [M]); the delta-vs-base row writes the VM
section whole. The host-dedup rows count a 32-byte reference per chunk. The
largest word-run deltas are the first park (the session's data arriving:
83,476 bytes) and park 2 (34,270) [M]; the 32 query parks then change a list
slot and a length word each.

Changed base objects in a park are the intern table (7–22 changed words of
2,050) and one to three small objects (4-word `dict` headers; in the first
park also a `BUF` and a `RAW` storage block) [M]. Prefix/suffix patches
(dquickts' `snap_put_changed_body`) cost 15,556–16,022 bytes per park for
these, because the changed intern-table words are spread across it; word runs
cost 90–270 bytes for the intern table [M].

### Restore

`Engine::restore` of park 3's image (49,914 bytes), 200 times, callgrind [M]:

| Part | Share |
|---|---|
| `calloc` + `memset` (segment capacity, both spaces) | 62% |
| `Heap::check_objects` | 14% |
| xxh64 checksum | 5% |
| Rest (VM registers, code table) | 19% |

Wall: 43.4 µs per restore for this image and 16.8 µs for the fresh image,
in a loop of 200 with warm caches [M]. Inside the benchmark, between other
work, a restore averages 86 µs [M].

### dquickts

`stencil-hq/dquickts` at `b51e92c`, `quickjs-snapshot.c` lines 1–56 and
`snap_put_changed_body` [S]:

- FULL and DELTA kinds. A DELTA names its base by id; the base id is the hash
  of the base snapshot written with a null id, with the low bit set.
- A DELTA numbers base entities by their base indexes, lists changed base
  cells, and stores each changed body as a prefix length, a suffix length
  (LEB128) and the middle bytes.
- Bases are reproducible. `Math.random` state and the time origin stay out of
  the base; FULL images and deltas carry them in their roots.
- Deltas are not chained: each is against the base. The base keeps every body
  it had.
- README: an empty runtime's base is 12,931 bytes; deltas reach 17–20 KB;
  the drepl session is about 400 bytes.

micro-monty needs no entity numbering: base objects have the same `Ref`s in
every session, because the arena is non-moving and every session starts from
the same image.

## Options

| Option | Bytes per park (median) [M] | Hot-path cost | Verdict |
|---|---|---|---|
| A. FULL + host dedup at 4 KiB (spec §9.3 today) | 13,506 (2.7×) | none | Keep as the fallback; not a multiple worth stopping at |
| B. FULL + content-defined chunking on the host | 5,994 (4.7×) | none | Host's choice; no engine change |
| C. Delta vs base only (dquickts' model) | 18,232 (2.6×) | dirty bits | Rejected: the base is too small to matter |
| D. Delta vs parent, 4 KiB dirty pages (spec §9.3's in-engine option) | 8,368 (2.0×) | one bit per page | Rejected: scattered writes dirty whole pages |
| E. **Delta vs parent, 64-byte dirty lines** (recommended) | 308 (9.1×) | one bit set per setter call | Recommended |
| F. Delta vs parent, word runs by diffing dirty lines against a parent copy | 192 (14.4×) | as E, plus a copy of each line on its first write per epoch | Later, if bytes matter more than memory |
| G. Shared in-memory base with copy-on-write per segment | — | a branch per setter | Not yet: every session interns a string in its first cell, which copies the base's main segment anyway |
| H. Shared in-memory base with copy-on-write per 4 KiB page | — | a branch per setter | Later, when bases grow past several hundred KB |

E stores only what the barrier saw written, with no comparison and no copy.
F writes 1.6× fewer bytes than E, but it needs a second copy of every line
written since the last park and a word compare at snapshot time; E is the
"less work" choice.

## Recommended design

### Dirty lines in mm-core

- Each segment of both spaces gets a dirty bitmap with one bit per 64-byte
  line (8 words): 0.2% of the segment, allocated with the segment's mark
  bitmap.
- Every setter (`set`, `set_raw`, `set_values`, `raw_words_mut`, `bytes_mut`,
  `set_flags`, `set_guarded`) and every allocation and free (header writes)
  already sets the segment's `dirty` flag. It sets the line bits of the range
  it wrote instead. `raw_words_mut` and `bytes_mut` hand out a slice, so they
  mark the whole slice.
- GC marks live in a side bitmap, so marking dirties nothing. Sweeping a dead
  object writes a free-block header and dirties its first line; the delta
  then records the free.
- `Heap::take_dirty` becomes "iterate dirty lines, then clear", and
  `snapshot_epoch` (a counter) goes up on each FULL or DELTA write.

### Snapshot kinds and format

The next format number (4, or 5 if native continuations land first):

```text
header  magic, format u16 = 4, build_id [u8; 32],
        kind u8 (FULL = 0, DELTA = 1, BASE = 2),
        image_id [u8; 32]     // xxh3-128 or blake3 of this image with image_id zeroed
        parent_id [u8; 32]    // DELTA only: the image it applies to
        session flags (returns_last_expr, want_value, phase, text_limit)
heap    FULL/BASE: as today.
        DELTA: per space: segment-table size, bump, vacant list, then per
        segment with dirty lines: index, capacity, new top, and runs of
        (first line, line count, bytes); segments absent in the parent
        are written whole.
vm      FULL: as today. DELTA: the registers whole (frames, scheduler,
        globals, counters: small), then the sources and code table as
        "the parent's first n entries" plus new entries; a table the
        session reclaimed from is written whole. The whole section is
        2.6–5.1 KB per park; its byte diff against the parent averages
        271 bytes [M], which this encoding approaches [E].
trailer xxh64
```

- `build_id` stays and still gates everything: a base, and every delta on it,
  belongs to one build.
- `image_id` is a hash of the image itself, so a store can verify that a
  DELTA's parent is the image it holds. A BASE's id is computable by any
  process of the build without storing it (the base is reproducible).
- A chain is BASE or FULL, then DELTAs, each naming its parent. Restore takes
  the chain in order: `Engine::restore_chain(&[&[u8]])`.
- `Session::snapshot()` keeps returning FULL. `Session::snapshot_delta()`
  returns a DELTA against the last image this session wrote or was restored
  from, or FULL if there is none (a fresh session's parent is the BASE).
- The host decides when to rewrite a FULL: proposed default, when the chain's
  deltas exceed half of the last FULL or 64 links. On the long session, the
  32 query parks never reach either limit.

### What the base contains, and what stays out

The base is `Session::new(&engine)` at a given `EngineConfig`, before any
cell, host function definition or `set_*` call:

- **In:** builtins, types, the intern table with its start-up names, the
  prelude namespace dict (empty; units are lazy), the module table, the
  empty-tuple and `MemoryError` singletons.
- **Out, carried by every image instead:** limits and GC configuration
  (today in the heap header; they move to the session section), fuel left,
  the virtual clock, `seq` and `next_call`, the host time zone, host
  functions and modules (registered after `new`), the entropy source (host
  owned, never in an image), and every imported module, including `random`
  and its generator state. Inline caches and scratch buffers are never
  written.
- Host function definitions are cheap and differ per embedder, so they are
  ordinary session mutations in the first delta (the probe's base included
  four of them; they are a few hundred bytes [E]).
- **Reproducibility check:** two processes of one build produce byte-identical
  fresh images [M]. Native and Wasm images of the same build are expected to
  be identical too, since images are pointer-width independent since
  `54ea791` [E]; step 1 of the plan checks this under wasmtime. If they
  differed, each target would just have its own base id; correctness does not
  depend on it.

### Copy-on-write of base objects

Sessions mutate base objects all the time: interning a name writes the
intern table, defining a host function or importing a module writes the
module table, and a cell's assignments write dicts the base created. In this
design that needs no special path:

- **On disk**, a base object a session changed is written by the delta's dirty
  lines like any other object. The base image itself is never written again.
- **In memory**, each session owns its heap; a fork or restore copies the
  base's segments (a memcpy of 35 KB) instead of sharing them. That is the
  "copy" in copy-on-write done eagerly, because at this size it costs less
  than tracking.

When bases grow (an embedder preloading a large dataset or many modules into
its own base), option H makes sharing worthwhile:

- Segments of a base are `Arc`-shared, in 4 KiB pages. The setter's existing
  branch on the segment's `dirty` flag becomes "first write to this page in
  this session?"; on that slow path it copies the page into the session's
  segment.
- GC must not write into shared pages: base objects are treated as always
  live (not swept), and the base pages a session has copied are the GC's
  remembered set (objects in them can point at session objects). Collections
  then mark only from roots and copied pages, so GC work scales with the
  session's own heap.
- The base's free space is sealed: sessions allocate in their own segments.

### Restore, fork and portability

- **Restore from bytes:** read the BASE or FULL, apply each DELTA's line runs
  in order, then validate once: `check_objects` and the intern-table check run
  over the result, and the code-table and frame checks run on the last image's
  VM section. Untrusted input stays fully checked.
- **Do not zero what is about to be overwritten.** Segments are allocated at
  their capacity and zeroed (62% of restore today [M]). Build each segment
  from the image instead: extend a `Vec` with the image's runs and zero only
  the gaps between them, and commit the capacity above `top` lazily. No
  `unsafe` is needed. Estimated −50% restore time [E].
- **Restore from a template:** the facade keeps a per-`Engine` validated
  template of the BASE (segments and VM registers) once one has been built.
  `Session::new` and `restore_chain` starting at the BASE then copy the
  template and skip its parsing and validation. Estimated restore of a
  50 KB session from template + a short chain: 5–10 µs [E] against 43 µs
  [M].
- **Fork:** `Session::fork(&self)` = the same copy from a live session
  (segments plus VM registers), at a safepoint. Estimated 3–5 µs for a 50 KB
  heap [E]; today a fork is snapshot + restore, about 55 + 43 µs [M].
- **Portability:** DELTAs are bytes over the same pointer-width-independent
  encoding as FULL images, so a chain written by a native worker restores
  under Wasm and back, as FULL images do since `54ea791`. The round-trip
  check in `cargo xtask wasm` gets a chain case.

### Estimated effect on the long session

| | Today [M] | Recommended [E] |
|---|---|---|
| Bytes written, 38 parks | 1,970,366 | about 218 KB (9.1×) |
| Median bytes per query park | 50,138 | about 310 |
| Snapshot time per park | 35–90 µs (GC + full write) | GC + about 1 µs of writing |
| Restore of the last park, cold (bytes) | 43 µs | about 25 µs (no zeroing; chain apply is small) |
| Restore of the last park, warm template | 43 µs | 5–10 µs |
| Fork | about 100 µs (snapshot + restore) | 3–5 µs |
| Host store, if it keeps every park | 1.97 MB | about 0.25 MB including periodic FULLs |

GC still runs before every snapshot, as today, so garbage is never written;
at these heap sizes GC is a small part of the snapshot time [E].

## Implementation plan

1. **Measure the remaining unknowns.**
   - Build the fresh image under wasmtime (`cargo xtask wasm`'s roundtrip
     example) and compare it byte for byte with the native one.
   - Measure the setter cost of a line bit on the dispatch microbenchmarks
     (`bench --quick`), with the bit set unconditionally.
   - Commit the parser as an `xtask` subcommand (`cargo xtask snapdiff`) so the
     table above can be regenerated.
2. **Restore without zeroing** (independent, lands first). Change
   `Space::read_snapshot`/`install_normal` to build segments from the image's
   runs. Checks: mm-core property tests and fuzzers, the snapshot fuzz target,
   `cargo xtask durability`, `cargo xtask wasm`; remeasure restore.
3. **Dirty lines in mm-core.** Bitmaps per segment, setters and header writes
   mark lines, `take_dirty` iterates lines. The existing barrier tests
   (`crates/mm-core/tests/heap.rs`, `support/ops.rs`) gain line-level
   assertions: every setter marks exactly the lines it wrote.
4. **New format: kinds, ids, DELTA writing and reading.** Move limits and GC
   configuration out of the heap header into the session section.
   `Session::snapshot_delta`, `Engine::restore_chain`. Checks: round trips
   comparing `restore_chain(FULL, d1..dn)` with a FULL written at the same
   point (byte-identical after a FULL rewrite); corrupt-input tests for a
   wrong parent id, a truncated run, a run past a segment's top, and a delta
   applied to the wrong base; fuzz seeds for DELTA.
5. **Durability over chains.** `cargo xtask durability` gains a mode that
   parks as now but continues each park from base + chain in a new process
   (`mm exec --snapshot-delta`, `mm resume DIR` applying the chain). Output
   must equal an unparked run's, as today.
6. **Template and fork.** The facade's per-`Engine` base template;
   `Session::new` and `restore_chain` from it; `Session::fork`. Remeasure
   `Session::new`, restore and fork, and rerun the embedding benchmark with
   deltas; record the results in HANDOFF and `bench/results/embedding.txt`.
7. **plak.** The cell worker writes deltas and plak's content-addressed store
   keeps chains (tested against a local plak checkout; plak itself is not
   modified).
8. **Later, only when needed:** word-run deltas (option F) and shared pages
   with copy-on-write (option H).

## Risks and open questions

- **Setter cost.** Setting a line bit is a shift, an OR and a store on every
  setter call. If `bench --quick` shows it, the bit can be set only when the
  segment's existing `dirty` flag is false plus a per-line test, at the cost of
  one more branch [E].
- **Size.** The DELTA writer and reader, the chain API and the line bitmaps
  are estimated at +4 to +8 KB on Wasm [E], against 154 bytes of headroom
  [M]. Like the native-continuation spike, this waits for a budget decision or
  Rust 1.101's `#[optimize(size)]`. A Wasm build that never writes deltas
  could leave the writer out behind a feature, if the facade's Wasm consumers
  only restore.
- **Chain management is the host's job.** A lost parent makes its whole chain
  unrestorable. The engine reports a parent mismatch as `SnapshotError::
  Incompatible`; the host keeps FULLs often enough.
- **Determinism of the base across builds** is not needed (the `build_id`
  changes anyway), but determinism within a build is, and a future change
  that puts a hash seed or time into `Session::new` would break it. A test
  asserts two fresh images are byte-identical.
- **GC before every delta.** Collecting is O(live heap). For multi-MB heaps
  that park every cell it may dominate; then skip the collection on deltas
  and let the next one record the frees.

## Reproducing

- **Images.** Copy `embedding_bench.rs` to a scratch example that writes each
  `session.snapshot()` to `/tmp/snaps/N.bin`, snapshots once more after
  `Session::new` and the four `define_host_function` calls (`0.bin`), and,
  when `LONG` is set, hibernates after every query cell. Time `Engine::restore`
  of `0.bin` and `Session::new` 50 times each (best). With `WARM=<source>`,
  run that cell before the base snapshot.
- **Parser.** After 53 header bytes, the heap section is: memory limit (u64),
  GC ratio (u32), GC minimum (u64), five u64 stats, intern-table ref (u32);
  then per space: segment count, bump, vacant count and indices, present
  count, and per present segment its index (u32), large flag (u8), top and run
  count (u32 each), capacity (u32, normal segments), then runs of offset,
  unit count and data. A word-space object is its header plus `len` words;
  a byte block is one header unit plus `ceil(len / 8)`. The rest of the file
  is the VM section and the checksum.
- **Comparisons.** Key objects by (segment, offset); an object is new when
  absent from the parent or of a different type or length, otherwise compare
  word by word. Lines are `offset // 8` (64 bytes) per segment. Host dedup:
  SHA-256 of fixed or gear-hash content-defined chunks, counting new chunks in
  full and 32 bytes per reference.
- **Restore profile.** A scratch example restoring one image 200 times under
  `valgrind --tool=callgrind`, built with
  `CARGO_PROFILE_RELEASE_STRIP=none CARGO_PROFILE_RELEASE_DEBUG=line-tables-only`.
