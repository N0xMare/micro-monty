# Changelog

micro-monty's releases. Versions are `0.0.x` until 0.1: Cargo treats every
`0.0.x` as incompatible with the next, so each release may change any API,
and a snapshot restores only on a build with the build id that wrote it
(docs/user-guide.md, "Upgrading: drain old builds"); each release lists its
build ids. Numbers are labelled [M] measured, as in the rest of the
repository. The 0.0.1 entry stays as published: the docs it cites
(docs/sdk.md, docs/research) are those of the `v0.0.1` tag, and its commit
hashes are pre-release history, kept in the private archive
`N0xMare/micro-monty-archive` (branch `history-0.0.1`).

## 0.0.2 (unreleased)

### Changed

- `mm-compile`'s `CompileError` is boxed: it is now
  `CompileError(Box<ErrorData>)`, and its `kind`, `msg` and `pos` fields
  moved to the new public `ErrorData`. Reads such as `e.msg` still compile
  through `Deref`; a struct literal becomes
  `CompileError::from(ErrorData { .. })`, and moving a field out needs a
  clone. A `Result<(), CompileError>` shrinks from 56 to 8 bytes on x86_64.
  The shipped `hello.wasm` falls by 54,039 B and native `hello` by
  42,912 B [M]. The facade's `micro_monty::CompileError`, the mm-wasm ABI
  and the build id do not change.

## 0.0.1 (2026-11-12)

The first release: the engine, its Rust SDK, the `mm` CLI and an mm-wasm
guest, from a from-scratch implementation of
docs/spec.md (v4) built from 2026-10-31 (this entry as of 2026-11-12).

### Build ids

A snapshot restores only on a build with the id that wrote it [M, on
e1328bf, `micro-monty`'s `build_id` example]:

- `538462b891323bfea7e92dcee4d3b0351097aa276cfb2abb5403bf2000fc8b58`:
  the facade's default features: an embedding with no opt-in module, the
  shipped mm-wasm guest and the bundle guest, and `mm-cell-worker` built
  on its own.
- `5039e83da95c8c6dcd983f748a11178e31fd4ccaa936f84e4adf428a304f54a8`:
  with `mod-uuid`, `mod-unicodedata`, `mod-html-entities` and `mod-ast`,
  as `mm` (native and `wasm32-wasip1`) builds it, and any binary built
  together with `mm`.

### Published crates

`micro-monty` (the facade and SDK) and the five crates it is built from,
`mm-core`, `mm-compile`, `mm-parse`, `mm-engine` and `mm-stdlib`. They pin
each other with `=` requirements, as one engine; the five internal crates
have no API guarantees of their own. License `MIT AND PSF-2.0 AND Unicode-3.0`; every package ships
LICENSE, THIRD_PARTY.md and its README. Not published yet: `mm-cli`,
`mm-cell-worker` (a local integration-test worker; its protocol crate is
unpublished),
`mm-wasm-abi`, `mm-wasm-guest`, `mm-wasm-host` and `mm-wasm-alloc`.

### Engine

- A Python 3.14 subset for code that LLM agents write: classes with C3
  inheritance, `super()`, properties and metaclasses; generators,
  coroutines, async generators, `async for`/`async with`; `match`
  (PEP 634); `except*` and exception groups; f-strings; arbitrary-precision
  ints, complex and `bytearray`; `__import__` over the bundled and host
  modules. Out of scope by design: `exec`/`eval`,
  threads, `weakref`, `__del__`, `__slots__`, a user `__getattribute__`
  and ambient I/O; not yet
  supported: `memoryview` and PEP 695 `type` aliases (docs/sdk.md,
  "Limitations").
- Its own parser (`mm-parse`, checked against ruff's on every corpus file by
  `cargo xtask parse-diff`), an accumulator-register bytecode with `match`
  dispatch, and a NaN-boxed arena heap with a tracing GC, all in safe Rust
  (`unsafe` is forbidden in every engine crate).
- Pause-anywhere: a cell parks at any safepoint and any call depth on fuel,
  a deadline or a park request, and on every host call; nothing native
  holds a second dispatch loop on the Rust stack.
- CPython 3.14's error text: tracebacks with column carets, "Did you mean"
  hints and SyntaxError messages, checked against CPython 3.14.7.
- Bounded resources: one limit per exhaustible resource (memory, fuel,
  recursion, text, host-argument size). Some caps are fixed whatever
  `Limits::memory` allows and raise `MemoryError`: one object holds at
  most 64 MiB of data or slots (`mm_core::MAX_OBJECT_BYTES`; a list of
  8,388,608 items, a dict of 2,097,152 entries), an int at most 2^26 bits,
  a superlinear int operation estimated past 45 ms is refused before it
  starts (`intops::WORK_MAX`), `ast.parse` takes at most 32 KiB of
  source and `tokenize` 64 KiB, and a `unicodedata` run with no
  normalization boundary at most 65,536 characters. A source over
  256 KiB (`MAX_SOURCE_BYTES`) is a `CompileError`. Guest input never panics the
  engine; every guest-reachable error is a Python exception, and every
  Rust recursion is iterative or depth-counted.
- Fuel in proportion to work: natives charge for their results, byte work
  and walks; long sorts, `math.factorial`/`perm`/`comb` and big-int
  operators ask for their cost before starting; iterators are gathered
  and long sorts merged a chunk per step, and the compiler puts checks in
  straight-line code, so fuel, deadlines and park requests are seen
  inside them (docs/spec.md §5, §8.6). Collections charge none, and fuel
  totals match across targets; with `Session::set_gc_yield`, which
  `drive_async` and hosts/js's sliced driver turn on, a collection step
  ends a fuel slice instead (the optional `mm_set_gc_yield` export).
- Bounded native steps: at the default limits no native step runs longer
  than about 50 ms between two safepoints on the sites
  `crates/micro-monty/tests/steps.rs` marks `Bounded`, which
  `cargo xtask check` times, so a deadline or park request waits at most
  about that long. Every row is `Bounded`; full collections, snapshots,
  restores and compilation are not steps (docs/spec.md §5.1).
- Memory in proportion to the limit: while a cell runs, the Rust heap
  stays within 2.5 × `Limits::memory` + 16 MiB for any guest input
  (measured worst 2.01 × [M]). `Session::compile` charges a cell's parse
  and compile to the session too, so a cell too big for the memory left
  is a `MemoryError` (docs/spec.md §11).

### Stdlib

55 bundled modules in the default build (about 2.2 MB; `hello.wasm`
2,271,773 bytes [M] on e1328bf), plus `sys`, `math`, `cmath` and the
builtins, and `ast`, `uuid`, `unicodedata` and `html.entities` as opt-in
features. Every bundled module has a `mod-*` feature (`sys`, `math` and
`cmath` are always built); a module brings the modules it imports. Rust
for builtin types and data-proportional work (`json`, `re`'s matcher,
`hashlib`, `struct`, the datetime parsers); Python adapted from CPython 3.14
for the rest, compiled to bytecode at build time.

### Durability

- FULL snapshots of a session at any stop, validated structure by structure
  on restore (`SnapshotError::Corrupt`, never a panic), with an XXH64
  trailer. Snapshot format 1.
- Same-build-id restore only: a 32-byte build id is compared on restore
  (`SnapshotError::Incompatible`). It hashes what an image's meaning rests
  on, not the source: the snapshot codecs, the declared layouts (every
  `const` and `#[repr]` item of the engine crates), the opcode, native,
  type and native-step tables, the prelude's and enabled modules' bytecode
  and a layout epoch. A later build that changes no layout restores this
  release's images. Images move between processes and between native,
  `wasm32-wasip1` and `wasm32-unknown-unknown` builds with the same id.
- Session templates (`SessionTemplate`), and an Engine-wide cache of decoded
  modules and host-module code shared by its sessions.

### Host boundary

- `Session::define(HostModule)`: host functions (typed signatures checked
  at the call with CPython's `TypeError` messages, or untyped), host
  classes as immutable values, constants and exceptions, with `.pyi` stubs
  from `Session::stubs()`.
- Host calls return like ordinary functions; `await` and `asyncio.gather`
  give concurrency, with call ids, batches (`Resume::Batch`) and
  `Session::pending()` to re-read the stop after a restore.
- No ambient effects: the clock, entropy, time zones, output and the OS
  hooks (`open`, `os.getenv`, `pathlib`) are the host's.
- `SessionSettings` holds what an image does not carry (output, the wall
  clock, entropy, the zone source, the text cut, `int_bits_text`, GC
  stress); `Session::new_with`, `Engine::restore_with` and
  `SessionTemplate::instantiate_with` take it, so a restored session
  behaves like the one that was saved. `Limits`, `EngineConfig` and
  `SessionSettings` are `#[non_exhaustive]`, with `const` builders on the
  first two, so settings can be added after 0.0.1.

### Binaries and embeddings

- `mm` (`run`, `exec --json`, `resume`, `version`), which the conformance,
  durability and bench gates drive.
- `mm-cell-worker`, an embedding example used in local integration
  testing: a worker for one external cell protocol, as a library
  (`mm_cell_worker::serve`) and a stdio binary with seccomp on Linux.
- The mm-wasm guest (`crates/mm-wasm-guest`, `ABI_VERSION` 1, frozen with
  0.0.1: a release may add a session option or an optional export) with a
  wasmtime reference host (`crates/mm-wasm-host`) and a JavaScript one for
  V8 (`hosts/js`). A session's options are a tagged record that new and
  restored sessions both take, so a restore gets the host-side settings an
  image does not carry. It ships in two builds: the default guest, and the
  bundle guest, which keeps the stdlib and prelude bytecode in custom
  sections that the host serves through `mm_host.bundle_read`, so they are
  not copied into every instance's linear memory (about 330 KB less data
  per instance [M]). A host without that import uses the default guest
  (docs/research/wasm-embedding.md, "Two guests").

### Measured at release [M]

- 546 of the 568 CPython-scored cases of Monty's test corpus
  (`conformance/results/micro-monty.md`).
- 2,841 recorded cells from 160 real code-mode sessions replay with their
  recorded outcomes (`conformance/codemode/real/gate.py --check`).
- Every passing case parked every few fuel units and at every host call,
  each park restored in a new process, and the own cases across native and
  `wasm32-wasip1` (`cargo xtask durability`, `--cross-target`).
- Against the code-mode JavaScript engines: 131 KB of RSS and 0.15 ms per
  new session after setup, 3.3 µs per awaited host call; 1.9× rquickjs's
  native size, and native QuickJS 1.3× faster on the bench geomean
  (docs/research/js-engines-compare.md, at 5df122e).

### Platforms

Tested: x86_64 Linux, aarch64 macOS, `wasm32-wasip1` and
`wasm32-unknown-unknown`. Never run: Windows. Snapshot images are
endian-independent; big-endian was checked only under Miri (README,
"Platform support").

### Known issues

- wasmtime runs the dispatch loop 4–9% slower than it did before
  4fe04e1: Cranelift now spills the linear-memory base pointer in
  `Vm::execute` and reloads it before each opcode fetch. Guest
  instructions rise 8.0% on fib, 10.2% on closure and 6.3% on generators,
  wall time 1.055, 1.087 and 1.042 × [M]. V8 and native builds are
  unaffected by the spill. The last batch before the release moved code
  layout again (guest instructions +1% on fib and closure): against
  7ac25ef, wasmtime wall time is 1.105 on fib, 1.168 on closure and
  1.097 on classes, and V8's and the V8 bundle's fib and closure 5–11%
  slower, on hosts with AVX-512 [M] (docs/sdk.md, "Embedding
  guarantees").
- Restores are not fully transparent (docs/sdk.md, "Other differences
  from CPython"; docs/spec.md §5.1, guarantee 4). Near the memory limit,
  an operation the engine does not run again after a collection can
  raise `MemoryError` unparked and succeed after a snapshot, because a
  snapshot collects first; a snapshot inside an out-of-memory retry wait
  can change the fuel total once per wait (+0.64% on `f(*big, **kw)` at
  256 MiB [M]); and indexing a long non-ASCII str is charged its
  char-position walk again after every restore: 43,313 units unparked
  against 8,687,009 when restored at every 20,011-unit park [M]. Fixes
  are planned for 0.0.2.
