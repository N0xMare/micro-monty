# Built-in import images: a prototype for json, re and collections

Question (owner, 2026-11-07): after the import-cost pass
(docs/research/import-cost.md), the five-module import cell (`json, re,
collections, asyncio, datetime`) still costs 18.6M Ir per session with the
module cache warm. That splits into prelude units 6.1M, `load_in` 5.5M,
module bodies about 6M and the cache-hit path 1.9M. Monty imports in about
0.3 ms because its modules are Rust tables. Would it pay to run each pure
bundled module's import at build time and ship the resulting object graph
as a relocatable per-module image that an import grafts into the session
heap ("form 3")? The owner approved a prototype to get decision numbers,
not a rollout.

Labels: [M] measured here, [S] sourced, [E] estimated. x86-64 orb (Xeon
2.6 GHz, 16 cores), release builds, wall time pinned with `taskset -c 2`.
Base: origin/main `61f216c` (size pass 2). Prototype: local branch
`local/import-image` (`0c1b5e9` + fixups `3967b26`, `bd81e26`), not pushed;
rebased onto `f544ab8` (the core-module pass) as `480f558`, `d9d7424`,
`1bd4c84`, where the same four images capture and the 18 equivalence
scripts still match (numbers below are from `61f216c`).
Raw results: `bench/evidence/results/2026-11-07-import-image/`. Harness:
`bench/evidence/import_cost/` (unchanged), `bench/evidence/import_image/parse.py`
(image sections; images are in `mm-image`'s `OUT_DIR`) and, on the branch
only, the `img` graph-dump binary next to it (it needs the branch's
`image-debug` feature).

## Decision (owner, 2026-11-07): deferred

The full import image is deferred (HANDOFF "Scope decisions" 47). It saves
about 1 ms of native wall time once per importing session (the five-module
cell 2.67 → 1.72 ms warm), still leaves imports several times slower than
Monty's 0.3 ms, and costs 46–71 KB of shipped size plus about 2,800 lines
(the prototype's 2,000 and about 800 to finish it). The cheaper parts of
the same cost go to the import-cost work instead: lazy `asyncio`
attributes (most of the 6.1M Ir of prelude units) and an Engine-level
prelude-unit cache. `local/import-image` stays as it is, unpushed.

What could reopen it is in section 8: whether an image can replace its
module's bytecode instead of shipping next to it.

## Answer

- **The graft works and is exact [M].** For 18 scripts covering `del` and
  re-import, failed imports, `reset`, `from`-imports, `asyncio.queues`, prelude units
  already built, shapes and interned flags, the canonical object-graph dump
  after a graft equals the dump after running the body. That holds both
  in-process and with a snapshot and restore after every cell. Attribute
  caches still hit. Conformance and durability (1216/1216 runs, 120,801
  parks restored in new processes) are unchanged with images on.
- **What it buys [M]** (cache warm, the facade default):
  - `json` 0.77 → 0.19M Ir;
  - `re` 2.83 → 1.46M;
  - `collections` 2.36 → 1.39M;
  - `asyncio` 5.97 → 1.54M;
  - the five-module cell 18.55 → 11.66M Ir (−37%), with `datetime` still
    on the body path.

  Native wall time for the five-module cell goes from 2.67 to 1.72 ms. On
  the mm-wasm guest (new instance, session and import) it goes from 6.86 to
  5.67 ms (−17%).
- **What it costs [M].**
  - Size: +17 KB of code. The images are embedded raw in the prototype, so
    for json, re and collections they add another +109 KB. Huffman-packed
    with the stdlib's own packer they would be 29 KB, or 54 KB with
    asyncio.
  - Build: a host build of the engine as a build-dependency, which costs
    +10 s CPU and +0.2 s wall on a clean wasm32 release build.
  - Modules: `datetime` and every heavy module are refused today. They read
    the clock, synthesize functions, or import other bundled modules that
    have bodies.
- **Projection [E].**
  - The five-module cell would reach about 9.4M Ir (1.4 ms) if `datetime`
    deferred its clock reads. That is level with a whole-session template
    restore (9.95M [S]).
  - The heavy set (`decimal, statistics, zoneinfo, tomllib`) would go from
    82.6M Ir (11.3 ms) to about 15–21M (2–2.5 ms). That needs three engine
    features the prototype lacks, and it would add about 0.7 MB of raw
    images or 160–180 KB packed.
- **Recommendation:** go to a full implementation, but in a narrow form:
  - behind a feature, `import-images`, that is on for native embedders and
    off in the size-budgeted wasm builds until the owner sets a budget;
  - images packed with the shared Huffman packer and unpacked once per
    process through the module cache;
  - the common set first (json, re, collections, asyncio, then `datetime`
    once its clock reads are lazy);
  - a fuzzed image decoder.

  Do the heavy set as a second step only if the owner accepts its size.
  Details and owner decisions are in section 7.

## 1. What the prototype does

- **Capture (build time).** A new crate, `crates/mm-image`, has a
  `build.rs` that runs the engine on the host as a build-dependency with
  feature `image-capture`. For each module (default `json, re,
  collections, asyncio`; `MM_IMAGES=a,b` overrides), it imports the module
  in a fresh session that never collects. `Vm::image_begin`/`image_end`
  then diff the heap by address. The capture refuses a module whose import:
  - read the clock, entropy or zone data (a `host_reads` counter in
    `mmmod.rs`);
  - synthesized functions (`exec`-built code);
  - imported a bundled module that has a body;
  - mutated an existing object the image cannot name;
  - built a dict or set keyed by address-hashed values.

  The build script emits `IMAGES` and the stdlib features it captured
  with.
- **Image format** (`crates/mm-engine/src/image.rs`, header comment). An
  image has no addresses:
  - objects refer to each other by index;
  - strings are text plus their interned and hashed flags;
  - what the module shares with the session is named: builtin types,
    builtins, prelude globals, other modules and VM singletons;
  - instance shapes are stored as attribute-name lists;
  - the prelude units the import built are stored as programs in the
    snapshot `Code` encoding;
  - each code of the module's program has its consts and names;
  - prelude and builtins bindings, and subclass-list appends to named
    classes, are stored.

  Bytecode is not in the image. The module's program still comes from
  `StdModule::decode` or the module cache, so `std_codes`, `CODE_STDLIB`
  and the snapshot format are unchanged. Snapshots taken after a graft
  are indistinguishable from snapshots taken after a body run.
- **Graft (import time).** `find_module` tries `Vm::graft` before
  `start_std`. The graft:
  - checks the magic and the engine fingerprint;
  - interns the strings into the session's table;
  - resolves the externals;
  - allocates every object through mm-core's allocators and setters, so
    GC and write-barrier state are the arena's own;
  - appends the prelude-unit programs to the code table and loads the
    module's code with the image's consts and names (no `load_in`);
  - registers the module in the session's module table, as
    `Vm::imported` does after a body (micro-monty does not expose
    `sys.modules` to guest code).

  Prelude units the session already has are skipped as whole clusters. If
  the session has only part of a cluster, the graft returns "no image" and
  the import runs the body as today. After `reset`, a re-import reuses the
  loaded code's consts.
- **Facade.** The facade passes `IMAGES` to every VM unless
  `CAPTURE_FEATURES` differs from the stdlib's features, in which case it
  ships none. `MM_NO_IMAGES=1` disables images at run time for A/B runs.
  The images are hashed into the build id after `mm_stdlib::PACKING`. The
  `image-debug` feature exposes `Session::image_dump`/`image_grafts`. The
  decoded prelude-unit programs (`ImageCodes`) are cached in the
  `ModuleCache`, but they are not yet charged against its limit.

## 2. Image sizes [M]

Raw image and its sections (`parse.py`), the image compressed three ways,
and today's packed bundled bytecode for the same module (the stdlib's
Huffman packing, size pass 2). "Unpack" is the time for the stdlib's
Huffman unpacker to unpack the image, native, single run median.

| module | raw | Huffman | zstd-19 | zstd-1 | today's bytecode (packed) | unpack |
|---|---:|---:|---:|---:|---:|---:|
| `json` | 9,137 | 2,713 | 2,609 | 2,948 | — (native module; its prelude-unit code is 3,567 B of the image) | 18 µs |
| `re` | 63,036 | 15,516 | 14,395 | 17,248 | 15,119 | 116 µs |
| `collections` | 50,536 | 10,880 | 9,954 | 12,080 | 10,353 | 88 µs |
| `asyncio` | 90,990 | 25,333 | 23,141 | 28,029 | — (41,674 B of the image is prelude-unit code) | 211 µs |

Sections, raw bytes:

| module | objects (headers + bodies) | strings | per-code consts/names | prelude-unit code |
|---|---:|---:|---:|---:|
| `json` | 4,018 | 934 | 618 | 3,567 |
| `re` | 45,180 (313 objects) | 9,754 (632 strings) | 7,173 | 929 |
| `collections` | 34,617 | 8,614 | 6,970 | 335 |
| `asyncio` | 32,807 | 9,169 | 7,340 | 41,674 |

A packed image is about the size of the module's packed bytecode, and it
ships in addition to it. The image is roughly 0.65–0.7× the snapshot growth
of the same import. The snapshot bytes over a fresh session on the body path
are:
- `json` 11,411 B, `re` 91,736 B, `collections` 79,702 B, `asyncio`
  112,218 B, `datetime` 185,789 B;
- `decimal` 454,131 B, `statistics` 654,808 B, `zoneinfo` 505,441 B,
  `tomllib` 509,357 B;
- the heavy four together 1,029,266 B; the five-module cell 495,340 B.

## 3. Import cost [M]

Steady Ir per import, in millions: (Ir(11) − Ir(1)) / 10, minus a session
without imports (`bench/evidence/import_cost/measure.py`). "Cache warm" is
the facade default. "Cache off" is `Engine::with_module_cache(config, 0)`,
which is what one session per process pays (plak's worker, mm-wasm).

| import | cache warm, base → image | cache off, base → image |
|---|---|---|
| `json` | 0.77 → 0.19 | 0.77 → 0.37 |
| `re` | 2.83 → 1.46 | 5.72 → 4.47 |
| `collections` | 2.36 → 1.39 | 4.76 → 3.83 |
| `asyncio` | 5.97 → 1.54 | 6.02 → 3.58 |
| `json, re, collections` | 6.19 → 3.54 | 11.44 → 8.90 |
| `datetime` (no image) | 5.99 → 5.99 | 11.75 → 11.79 |
| five-module cell | 18.55 → 11.66 | 29.44 → 24.92 |

The first operation in a process (a cold cache), for the five-module cell,
goes from 29.48 to 24.63M Ir.

Native wall time in µs, base → image (the minimum over 3 rounds of the
median of 51, pinned):

| import | cache warm | first in process | cache off |
|---|---|---|---|
| `json` | 80 → 18 | 300 → 234 | 79 → 37 |
| `re` | 358 → 152 | 1,121 → 885 | 773 → 586 |
| `collections` | 293 → 135 | 923 → 741 | 642 → 469 |
| `asyncio` | 756 → 161 | 1,033 → 652 | 751 → 395 |
| `json, re, collections` | 764 → 453 | 1,985 → 1,586 | 1,616 → 1,129 |
| five-module cell | 2,672 → 1,715 | 4,839 → 4,056 | 3,808 → 3,427 (first in process: 4,752 → 4,032) |

RSS per session with the cache warm drops from 1,203 to 957 KiB for the
five-module cell and from 394 to 202 KiB for `asyncio`, because the session
no longer holds its own interned constants and prelude temporaries.

mm-wasm guest (shipped `wasm32-unknown-unknown`, wasmtime): new instance,
session and import, median of 21, pinned, µs, base → image (`wasm-*.jsonl`; an earlier unpinned
run gave 7,946 → 6,174 for the five-module cell):

| import | guest |
|---|---|
| none | 150 → 153 |
| `json` | 336 → 247 |
| `re` | 1,517 → 1,175 |
| `collections` | 1,311 → 1,009 |
| `asyncio` | 1,466 → 853 |
| `json, re, collections` | 2,813 → 2,130 |
| five-module cell | 6,863 → 5,666 |

Guest memory goes from 1.70 to 1.97 MB for an empty instance (the raw
images are data in the module) and from 3.34 to 3.80 MB after the five-module
cell. (The guest numbers are from the build with all four images.)

**Where a graft's time goes** (`re`, cache warm, 1.37M Ir, callgrind):
- interning the image's strings into the session: 0.56M (40%);
- `new_str`: 0.25M;
- decoding image values: 0.25M;
- `alloc_words`: 0.18M;
- `graft_program`: 0.13M;
- `alloc`: 0.12M.

That is about 22 Ir per raw image byte. Before `ImageCodes` cached the
decoded prelude programs, decoding them dominated `asyncio`'s graft. What
remains after a graft is the cache-hit path for the module's bytecode and
the cell itself.

## 4. Correctness [M]

- **Graph equivalence.** `img dump FILE MODS` prints a canonical dump of
  each module's reachable graph, numbered in traversal order: type, flags,
  values, raw words, string text and interned flag, class versions and
  caches, slots and shapes, `__slots__`/hints, and named externals by name.
  `img snap` does the same but snapshots after every cell and continues
  from the restore in a new session. Running one build with and without
  `MM_NO_IMAGES=1` compares graft against body. The 18 scripts compared
  (`eq/`) cover:
  - plain imports;
  - `del` and re-import, `import ... as` and a failed import in between
    (module identity: the re-import returns the same object);
  - `reset` then re-import;
  - `from`-imports;
  - `asyncio.queues`;
  - `json` after `asyncio` (prelude units already present);
  - classes subclassing grafted classes;
  - module use (`re.findall`, `Counter`, `asyncio.run`) before the dump.

  All 36 comparisons (18 × dump/snap) are byte-identical (`eq/summary.txt`;
  the graft counts confirm which runs grafted).
- **Attribute caches still hit.** For a loop over grafted classes'
  attributes and methods, the per-iteration Ir is 165,695 with the graft
  and 165,705 with the body.
- **Durability.**
  - `cargo xtask durability` with images (verified active: an
    `asyncio, re, collections` script costs 13.2M Ir in `mm` against 18.0M
    with `MM_NO_IMAGES=1`) gives 1216/1216 runs identical to unparked runs,
    120,801 parks each restored in a new process.
  - A dedicated import script (imports, `from`-imports, re-import under
    another name, `asyncio.queues`, module use) also passes parked every 1, 3, 12 and 28 fuel units (1,927,
    644, 163 and 71 parks).
  - `cargo xtask durability --cross-target`: 402/402 runs identical,
    10,856 snapshots each restored on the other target (native and
    `wasm32-wasip1`).
  - `sys.modules` is not a guest-visible attribute in micro-monty (`sys`
    has no `modules`), so its behaviour is covered only through import
    identity above; nothing a guest can observe differs.
- **Suites on the branch.**
  - `cargo test --workspace`: 271 passed, 0 failed.
  - clippy with `-D warnings` is clean.
  - `cargo xtask conformance --engine micro-monty --check` is unchanged.
- **Capture refusals**, as the build reports them:
  - synthesized functions: `decimal`, `typing`, `functools`, `fractions`;
  - clock, entropy or zone reads: `statistics`, `zoneinfo`, `tomllib`,
    `calendar`, `random`, `time`, `datetime`;
  - imports of bundled modules with bodies: `dataclasses`, `html.parser`,
    `textwrap`.

  `enum`, `itertools` and `string` capture.

## 5. Size and build [M]

`cargo xtask size` (shipped builds, bytes):

| build | `hello.wasm` | native `hello` | guest |
|---|---:|---:|---:|
| base `61f216c` | 1,999,470 | 2,177,048 | 2,103,826 |
| graft code, no images (`MM_IMAGES=""`) | 2,016,514 (+17,044) | 2,197,416 (+20,368) | 2,121,004 (+17,178) |
| `json, re, collections`, raw | 2,125,948 (+126,478) | 2,320,632 (+143,584) | 2,230,422 (+126,596) |
| `json, re, collections, asyncio`, raw | 2,212,694 (+213,224) | 2,411,704 (+234,656) | 2,317,168 (+213,342) |

Every image build is over the current ceilings. With Huffman-packed
images, `json, re, collections` would add about 17 + 29 = 46 KB, and with
`asyncio` about 17 + 54 = 71 KB (+3.6% of `hello.wasm`) [E, from the packed
sizes in section 2]. Unpacking costs 18 + 116 + 88 µs native for json, re
and collections. Through the module cache, that is paid once per process.
Without the cache, a native session would pay it on every import and lose
about half of the 487 µs it gains (`json, re, collections`, cache off). On
the guest, which has no cross-session cache, unpacking at instantiation
costs the same [E].

**Build stage.** `mm-image` depends on `mm-engine`/`mm-stdlib` (with
`image-capture`) as build-dependencies, so cargo compiles the engine a
second time for the host, at the build-script profile (opt-level 0), and
runs the import in `build.rs`.
- Cost: a clean `cargo build --release -p micro-monty --target
  wasm32-unknown-unknown` takes 20.2 s wall and 45.7 s CPU, against
  20.0 s and 34.7 s on base. The host engine builds in parallel with the
  target. The capture itself runs in 0.04 s (release) to 0.10 s (debug),
  at about 7 MB RSS.
- Cross-compiling works without a host toolchain beyond cargo's own:
  build-dependencies always target the host.
- Images are byte-identical across native debug, native release, wasm32
  and the import-cost builds (md5). They depend only on the stdlib
  sources, the stdlib features and the engine's object model, not on the
  target or the profile.
- The facade skips the images when the captured features differ, so a
  feature mismatch costs speed, not correctness. Before `mm-image`
  mirrored the optional `mod-*` features of `mm-cli`, the codemode runner
  and the guest, `mm` silently ran without images; that is fixed on the
  branch.
- **Build id**: the images are hashed into the facade build id after
  `PACKING`. Changing an image changes the build id, so snapshots from a
  build with different images are refused as from any other build. The
  snapshot format itself does not change, and a graft leaves no
  image-specific state.

## 6. Diff and risks

Prototype diff against `61f216c`, without lockfiles: 2,039 lines added
across 21 files [M].

| crate | lines | what | complexity |
|---|---:|---|---|
| `mm-engine` `image.rs` | 647 | format reader, graft, cluster skip, preset after reset | high: every object kind's raw words and refs must be mapped; it is the new trust boundary |
| `mm-engine` `image/capture.rs` | 909 | heap diff, refusals, writer, dump (build-only, `image-capture`) | high, but host-only and never shipped |
| `mm-engine` (`vm.rs`, `modules.rs`, `mmmod.rs`, `snapshot.rs`, `class.rs`, `lib.rs`) | 58 | hook in `find_module`, image table on the VM, `host_reads` counter | low |
| `mm-image` (new) | 132 | build script, feature mirroring | low, but it adds a build-dependency stage |
| `micro-monty` | 56 | `images()`, build id, `ImageCodes` in `ModuleCache`, `image-debug` | low |
| manifests (`mm-cli`, guest, codemode runner, workspace) | 22 | feature mirroring | low |
| evidence harness | 224 | `img`, `parse.py` | — |

A full implementation adds the following [E]:
- packed images and their unpacker wiring, about 100 lines;
- charging `ImageCodes` to the cache limit, about 30;
- an image validator and a fuzz target, about 250;
- bulk interning, about 80;
- removing the module's consts and names from the bundled stdlib where the
  image carries them, about 150;
- tests, about 200;
- for the heavy set: synthesized functions, transitive bodies and lazy
  clock reads, 600–900 lines in capture and in the stdlib modules.

Risks:
- **No panics, no `unsafe`.** The prototype has no `unsafe` (the workspace
  forbids it) and uses no `unwrap`/`expect`/indexing on image data in the
  graft. Every read goes through `mm_core::Reader` with bounds, and every
  failure becomes a `MemoryError` or "no image" (the body path). The
  build-only capture indexes freely, which is acceptable on the host only.
- **Restore validation of grafted objects.** Grafted objects are ordinary
  arena objects made through the same allocators and setters. A snapshot
  after a graft is validated by the existing restore checks like any other
  heap, and nothing new is trusted at restore. The new trust boundary is
  the image decoder at import time. Images are build-embedded and checked
  against the engine fingerprint, so they are not guest input today. They
  still need a validator before a full rollout: types in range, refs in
  bounds, raw-word counts per type. Otherwise a capture bug becomes a
  malformed heap that only a later operation detects.
- **Frozen formats.** The snapshot format is unchanged, and so is
  `mm-wasm` v1: no export is added, and images are internal to the guest
  module. The build id changes with the images, as intended. Making images
  a host-supplied input (for example to share them across guests) would be
  a new optional export, an additive v1 change.
- **Staleness.** An image is a cached result of running Python at build
  time. Any change to a captured module, to the prelude, or to an object
  layout must regenerate it. The build script does this automatically, and
  the fingerprint check refuses a stale image.
- **Coverage drift.** A module that starts reading the clock, synthesizing
  functions, or importing a module with a body silently loses its image
  (the build prints a warning). A full rollout should fail the build for
  modules on an "imaged" list.

## 7. Projections, recommendation and owner decisions

**Five-module cell [E].** With `datetime` changed to read the clock lazily,
its image would be about 0.66 × 185,789 ≈ 123 KB raw. At 22 Ir per byte
plus its 1.0M cache-hit path, that is about 3.7M instead of 5.99M, which
puts the cell at about 9.4M Ir and 1.4 ms native warm.

| | Ir, cache warm | native wall time |
|---|---:|---:|
| today | 18.55M [M] | 2.67 ms [M] |
| prototype | 11.66M [M] | 1.72 ms [M] |
| cached whole-session template restore of the codemode set | 9.95M [S] | — |
| Monty | — | 0.3 ms [S] |

So per-module images get to about the template's cost without templates'
loss of flexibility, but they do not reach Monty.

**Heavy set [E].** The four modules (`decimal`, `statistics`, `zoneinfo`,
`tomllib`) today cost 82.6M Ir, 11.3 ms native warm and 1.03 MB of
snapshot [M]. Images at 0.65–0.7 of that are about 0.7 MB raw. That is
about 15M Ir to graft, plus about 6M of cache-hit path, so about 15–21M Ir
(4–5× less), or 2–2.5 ms. This is the largest win per import. But it needs:
- capture of synthesized functions (`decimal`, `typing`, `functools`,
  `fractions` build functions with `exec`), which must carry their code in
  the image;
- capture across transitive bodies (one image for `statistics` covering
  `fractions`, `decimal` and `random`, or images that name each other);
- lazy clock, entropy and zone reads in `random`, `statistics`, `zoneinfo`
  and `tomllib`.

The size would be about 160–180 KB packed (+8–9% of `hello.wasm`).

**Recommendation** (not taken: the owner deferred the work, see the
Decision above). Proceed to a full implementation, staged:
1. **Common set, opt-in by feature (`import-images`).** Cover `json`,
   `re`, `collections` and `asyncio`, with Huffman-packed images unpacked
   once per process through the module cache and charged to its limit.
   Add an image validator with a fuzz target, bulk interning, and a
   build-time failure for listed modules that stop capturing. Turn it on
   for the native facade users (plak in-process, `mm`), where the cache is
   warm and the win is −37% Ir and −1 ms per five-module cell. Leave it off
   in `hello.wasm` and the guest until the owner raises the budget, because
   the guest gains less (−17%) and pays unpacking on every instance.
2. **`datetime`.** Make its clock reads lazy and add its image.
3. **Heavy set.** Do this only if the owner accepts about +170 KB packed
   for those modules or makes them a separate feature. It needs the three
   engine features above.

Do not land the prototype as is: its images are raw, over budget, and
unvalidated.

Owner decisions asked (answered by the deferral):
1. **Size budget.** Accept about +17 KB of code plus about 29 KB of packed
   images for json, re and collections (about 54 KB with `asyncio`) in
   `hello.wasm` and the guest, or keep images off in the wasm builds.
2. **Default.** Should images ship by default in the facade, or behind a
   feature that native embedders opt into?
3. **Build stage.** Accept a host build of the engine as a
   build-dependency of the facade (+10 s CPU on a clean build, no wall
   time on 16 cores, no extra toolchain), or keep capture as an `xtask`
   step that writes checked-in images? The checked-in variant needs a
   staleness check in `cargo xtask check`.
4. **Heavy set.** Fund the capture of synthesized functions, transitive
   bodies and lazy clock reads, and the size it adds.

## 8. Open question: can an image replace its module's bytecode?

Today an image ships next to the module's packed bytecode, and a packed
image is about the size of that bytecode (section 2). If the image were the
only copy, the shipped cost of json, re and collections would drop from
about +46 KB toward the +17 KB of graft code. This is the question that
could reopen the decision.

**Size: probably not as close to +17 KB as that [E].** The image holds the
module's objects, strings and per-code consts and names, but not
instructions: functions refer to the module program's code by index. An
image that replaced the bytecode would have to carry the instructions of
every function the module defines. Packed sections of today's bytecode
(`Program::encode_sections` on `f544ab8`, the stdlib's per-section Huffman
packing) [M]:

| module | packed total | strings, string refs, metadata | opcodes and operands |
|---|---:|---:|---:|
| `re` | 15,090 | 6,046 | 9,044 |
| `collections` | 11,770 | 5,800 | 5,970 |

The image's strings and consts would stand in for the first column. The
second must move into the image, less the module body's own code (run once
at capture; its share is not measured). So the net for `re` is about
15.5 + 9.0 − 15.1 ≈ +9.4 KB rather than +0.4 KB. For json, re and
collections together it is about +30–35 KB with the graft code, against
+46 KB today. `json` and `asyncio` have no module bytecode to drop (they
are engine modules whose image replaces prelude-unit building, and the
prelude's bytecode stays for other uses), so their images stay fully
additive. The `collections` row is after the core-module pass (10,353 B
on `61f216c`); its image would grow too.

**What it would lose.** Every place that falls back to, or re-reads, the
module's bytecode:
1. **The body path.** Today a graft returns "no image", and the import
   runs the body, when the session already has part of a prelude-unit
   cluster. The facade also ships no images when the stdlib features
   differ from the captured ones, and `MM_NO_IMAGES` turns them off.
   Without bytecode, all three must become guarantees: a feature mismatch
   becomes a build error, and partial clusters must graft unit by unit
   (capture would have to make units independent) or be refused at
   capture.
2. **Restore.** A snapshot names a bundled module's code by stdlib index
   and offset (`CODE_STDLIB`). Restore re-decodes the program with
   `StdModule::decode`, also for sessions that grafted. Restore would
   instead decode code from the image, so the image decoder joins the
   restore trust path. The snapshot format can stay only if the image keeps
   the program's code order, and the validator stops being optional.
3. **The correctness oracle.** Graft-against-body dumps, the A/B
   durability runs and the conformance comparison need the body. They
   would run on a test build that still has the bytecode, which is a
   second shipped configuration to keep equivalent.
4. **Modules that stop capturing.** A module that starts reading the
   clock or synthesizing functions keeps its bytecode, so the build must
   choose per module, and the stdlib table must allow a module that has
   only an image.

Capture itself is unaffected: it compiles and runs the bytecode on the
host at build time.

