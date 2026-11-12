# micro-monty: engine spec (v4)

Status: MVP built (M1–M5 and M8, §14); this file now describes the engine as
built, with v4 deviations listed below. Owner: n0x.
v1 written 2026-10-27; v2 2026-10-31 after a source review of Monty, a code-mode agent
harness and dquickts; v3 2026-10-31 after a first-principles architecture review; v4
2026-11-01 at the MVP, after measuring the built engine (America/New_York).

For: a coding agent starting with no prior context. Everything needed is in this
file, the repository's `AGENTS.md`, and the references in §18.

Labels on every number:

- **[M]** measured (machine and toolchain named where it matters)
- **[S]** sourced from code or docs (linked)
- **[E]** estimate or target, not measured

## 0. Summary

micro-monty is a from-scratch embeddable Python-subset engine in Rust, built on
the QuickJS philosophy, whose defining property is **pause-anywhere**: a running
cell can stop at any safepoint, at any call depth, with all of its state in the
heap, and continue later in the same process or, from a snapshot, in another one.

- **Language:** a Python 3.14 subset that LLM agents write: "near-exact Python,
  minus what can't be made durable". Not a DSL.
- **Host calls look synchronous.** Because any frame can pause, a host function
  simply returns its value (`text = fs.read(p)`); `async`/`await` and
  `asyncio.gather` remain for concurrency. A forgotten `await` stops being a
  class of model error (§10.1).
- **Safety:** the whole engine is safe Rust. The heap is a segmented arena
  addressed by `u32` offsets, so an engine bug corrupts Python objects, never
  host memory. Every Rust recursion is depth-bounded.
- **Stdlib:** Rust for primitives and data-proportional work; Python, compiled
  to bytecode at build time, for the rest (§12).
- **Embedding:** a pure-Rust crate, usable in process (with two conditions,
  §10.3), as an mm-wasm guest, or behind a worker process; `mm-cell-worker`
  (§10.2) is an embedding example over an engine-neutral cell protocol.
- **Size and speed:** dquickts-class: a small stripped binary and QuickJS-class
  speed on interpreter-bound code (targets in §6 and §12), substantially smaller
  and faster than Monty.

Monty (pydantic) is a donor, not a foundation: its test corpus, error wording and
subset decisions are reused; its value representation, heap, refcounting, serde
snapshots and protocol are not.

### What changed from v1

| Topic | v1 | v2 |
|---|---|---|
| Heap addressing | `u32` handle table → (page, slot) | `u32` offsets into a word arena; no handle table (§7) |
| `unsafe` | allowed in mm-core and dispatch | forbidden everywhere; exceptions need a spec change (§5) |
| Native re-entry | nested dispatch loops up to depth 8 | never: native fast path, prelude-bytecode slow path (§8.2) |
| Frames | one heap object per call | contiguous frame stack in the arena (§8.3) |
| Interned strings | separate `Sym` value tag | ordinary `str` objects with an interned flag (§7.3) |
| Builtin signature | `fn(&mut Vm, &[Value])` | `fn(&mut Vm, CallArgs) -> Result<Value, Raised>` (§5) |
| Parser | ruff for the MVP, own parser at M9 | ruff only to start; own parser before the MVP size gate (§6) |
| Integration | Monty-compatible host protocol | a Rust SDK and an engine-neutral cell protocol; no Monty compatibility (§10) |
| Durability | FULL/BASE/DELTA pages, mmap CoW, journal replay | pause-anywhere invariant; FULL first; dedup host-side; no replay (§9) |
| Limits | Monty's exception texts | typed preempts; no text matching (§11) |
| Crates | 8 | 7 plus `xtask`; vm and builtins merged; snapshot folded in (§6) |
| Bytecode shape | decide at M2 | decided by a dispatch spike before M1 (§8.1) |
| Build order | builtins → host → snapshots | host calls and parking early, gated by a code-mode py-ptc spike (§14) |

### What changed from v2

| Topic | v2 | v3 |
|---|---|---|
| Host-call style | `async def` wrappers awaited by the model | synchronous by default; `await` and `gather` for concurrency (§10.1) |
| Stdlib | hand-written in Rust | hybrid: Rust for primitives and data-proportional work, Python-written modules compiled at build time (§12) |
| Language order | classes and generators at parity | classes, inheritance, generators and decorators early: the Python stdlib needs them (§14) |
| Arena | one `Vec<u64>` | fixed-size segments; snapshots skip free runs (§7.2) |
| Rust recursion | not addressed | every native recursion depth-bounded; a rule and a fuzz target (§5) |
| Bytecode shape | stack, decided | accumulator-register with `match` dispatch, decided by spike S1 (§8.1) |
| Errors | CPython wording | CPython 3.14 tracebacks with column carets and "did you mean" hints (§8.4) |
| Embedding | worker only | in-process guarantees and conditions; seccomp for the worker (§10.3) |
| Host objects | `HostRef` handles | plain data only; harnesses wrap handle ids in prelude classes (§10.1) |
| Limits | memory, fuel, recursion | one limit per exhaustible resource (§11) |
| Language target and oracle | CPython 3.12 | CPython 3.14, oracle pinned to 3.14.7 (§4, §13) |
| Bytecode | stack vs accumulator, to be measured | accumulator-register with `match` dispatch (S1, §8.1) |

### What changed at the MVP (v4, measured)

Every v3 architecture decision held under measurement; nothing was redesigned.
These are the deviations and refinements the build produced:

| Topic | v3 | As built |
|---|---|---|
| Native modules | `mm-stdlib` features | `json`, `math`, `asyncio`, bigint and bytes natives live in `mm-engine`; `mm-stdlib` holds the bundled Python modules and the feature bitmask |
| Prelude | `mm-stdlib/python/` | `mm-engine/src/prelude.py`: the slow-path twins of builtins and VM operations, which a native fast path hands off to on the first user `__eq__`/`__hash__`/`__repr__`, `key=` function or generator (§8.2); compiled by `mm-engine/build.rs` into units (one per top-level `def`, class, instance or literal binding), packed in chunks (since 2026-11-09 in the bundled modules' sectioned form: each chunk's unit programs as the nine sections, with the strings three or more units use stored once, beside the unit records; one set of Huffman codes per part) and built in a session on the first lookup of their name (`mm-engine/src/prelude.rs`; design in `docs/spikes/lazy-prelude.md`). No prelude Python runs at session start. `mm-stdlib/python/` arrives with M7 |
| `dict` layout | index table plus entries | three forms: inline (≤ 8 entries in the object), linear (8-entry buffer, scanned by stored hash), indexed (compact u32 slots with hash tags); 48 B for a 2-entry dict vs CPython's 184 (§7.2) |
| `str` hash | cached | computed lazily on first use, cached behind a `HASHED` flag |
| Fuel | checked every 256 safepoints | charged exactly per safepoint; natives charge per item in chunks (`collect()`, `json`, `join`) |
| Snapshot trailer | blake3 | xxh64, hand-written (no dependency); snapshots are trusted input, so the checksum detects corruption, not tampering (§9.3) |
| `build_id` | engine version, features, prelude, native table, `Value` layout | a layout descriptor, not the version or the source: a layout epoch, the snapshot codecs' code, every `const` and `#[repr]` item of mm-core, mm-compile and mm-engine, the opcode, native, type and native-step tables, prelude and bundled bytecode, features, value and hash encodings, and a host-codegen probe; an edit to other function bodies keeps old snapshots restorable (scope decision 66, §9.3 "Compatibility") |
| `ParkReason` | yield window, cancel, fuel, deadline | `Fuel`, `Requested`, `Deadline`; a yield window is a deadline, cancel is `Session::cancel()` |
| Size budget | 2.3 MB for `hello` | interim 3.1 MB while ruff parses (`hello` is 3.01 MB [M]); M6 restores 2.3 MB, then 2.0 MB; since owner decision S1 (2026-11-05) one default build of about 2 MB, each ceiling set to the latest measurement plus about 2 KB (current ceilings in `xtask/size-ceilings.txt`, §6) |
| SDK presentation | refused until a type-checking plan exists | `mm-cell-worker` accepts `sdk_stubs` and runs SDK cells unchecked (2026-11-06); the pre-run check is the host's (HANDOFF decision 34) |

## 1. Context, goals, non-goals

### Context

Code-mode harnesses run agents whose action language is code: the model
writes a cell, the harness runs it in a sandbox engine, and every tool is a
host function the cell calls. The local agent harness micro-monty is tested
against (§18) shows the usual shape [S]. Its runtimes speak one
engine-neutral protocol, `plakous-cell-protocol.v2`: length-prefixed JSON
frames between the host and a worker process. One worker runs JavaScript on
QuickJS; another runs Python on Monty 1.2.0 in process. Python cells come in
two presentations [S]:

- **py-ptc** (programmatic tool calling): globals persist across cells, no
  type checking. Its bootstrap is only `async def f(arguments): return await
  private(arguments)` wrappers over host callbacks.
- **SDK**: a reset before every cell, type checking against generated stubs
  inside the worker ("type errors; nothing ran"), namedtuple records and
  namespace classes.

The runtime contract shows the gap micro-monty closes [S]: the JS worker can
yield a running cell and resume it on the next wait; the Monty worker cannot,
because Monty cannot park a running cell, so a Python cell keeps running
between observations, cancelling it costs the worker, and the compute bound
(90 s there) is enforced by killing the worker.

Monty itself (main @ `66fff6bf`, latest release 1.4.0) is closing the language
gap [S]: open PRs add inheritance with C3 MRO and exception subclasses
(pydantic/monty#1035) and lazy generators with `yield from` (#1020). Its
architecture does not change: a refcounted heap, serde dumps, and dunder calls
that "run to completion synchronously, so cannot yield to the host". Preemption
would have to be retrofitted the way dquickts retrofitted QuickJS, with the same
limits (§9.1). micro-monty's reason to exist is therefore pause-anywhere, safety,
size and speed, not "features Monty lacks".

### Goals, in priority order

1. **Correct semantics for the subset.** Where a feature is supported, output
   equals CPython 3.14's. Unsupported features fail at compile time with a clear
   message.
2. **Pause-anywhere** (§9.1) as a hard engine invariant, and FULL snapshots that
   restore into a fresh process running a build with the same build id.
3. **Sandbox safety.** No ambient I/O: every effect is a host call. Memory, fuel,
   time and recursion are bounded. No panic and no undefined behaviour on any
   source; zero `unsafe`.
4. **A full-featured Python for agents:** the language and stdlib that models
   write, errors they can act on, and host calls that look like ordinary
   functions. A cell-protocol worker running both presentations is the first
   proof: py-ptc first, then the SDK.
5. **Speed:** at or near QuickJS-ng on calls, closures and loops; beat Monty on
   every benchmark row (§12).
6. **Size:** a small stripped binary under a CI-enforced budget (§6).
7. **Portability:** the engine crates are `no_std` + `alloc`.

### Non-goals

- Full CPython compatibility, C extensions, pip packages.
- Threads, `__del__` timing, `__slots__` semantics, `exec`/`eval` of
  arbitrary modules, `weakref` (it would make GC timing observable).
- Filesystem, network, clock or randomness except through host calls.
- **Replay and cross-version resume.** Code-mode harnesses reject both: no
  source is re-evaluated and no tool is replayed, and a session reopens only
  on the same engine identity [S]. A snapshot from another build is refused, not migrated.
- **Monty protocol compatibility.** The integration points are the Rust SDK,
  the mm-wasm guest and an engine-neutral cell protocol.
- An in-engine type checker. Synchronous host calls remove its main job
  (catching a forgotten `await`); host functions validate their own arguments
  with CPython-style `TypeError`s at the call (§10.1). Pre-run checking is
  the host's, a checker over `stubs()` (owner decision 2026-11-05).
- Mapping snapshots copy-on-write from disk; a FULL restore reads and validates the whole image (§9.3).

## 2. Glossary

| Term | Meaning |
|---|---|
| Monty | pydantic's Rust Python-subset interpreter, MIT. Donor of tests and wording |
| Harness | The host program that runs an agent's cells and owns its tools; it embeds micro-monty |
| Cell protocol | `plakous-cell-protocol.v2`: `HostFrame`/`WorkerFrame` JSON frames between a harness and a worker (`mm-cell-worker`, §10.2) |
| py-ptc / SDK | The two Python presentations of code mode (§1) |
| dquickts | A QuickJS fork with suspension and full/delta snapshots; the reference for §9.1 |
| Safepoint | A bytecode boundary where GC, parking and snapshots may happen: backward jumps, calls and the `CHECK`s the compiler puts in straight-line code; returns charge no tick and check nothing (§8.6) |
| Pause-anywhere | Any safepoint at any depth can stop execution with all state in the arena (§9.1) |
| Park | Stop at a safepoint because the host asked (a yield window, `Cancel`, a preempt) rather than for a host call |
| Fuel | An instruction budget charged at safepoints and by natives in proportion to their work |
| Arena | The session heap: a word space (`Vec<u64>`) and a byte space (`Vec<u8>`) |
| Ref | A `u32` word offset into the arena. Values never hold pointers |
| FULL snapshot | Both arena spaces plus VM registers and a header |
| build_id | Hash of what an image's meaning rests on: a layout epoch, the codecs and declared layouts, the opcode, native, type and native-step tables, prelude and bundled bytecode, features and value encodings (§9.3 "Compatibility"). Snapshots load only on a match |
| Host value | The plain data that crosses the host boundary, copied by value (§10) |
| Conformance corpus | Monty's `test_cases/*.py`, copied into `conformance/monty/` |

## 3. Why QuickJS is small and Monty is not

QuickJS is about 1.3 MB engine-only in rquickjs [M v1]: hand-written C with
almost no generics, builtins as data tables (`JSCFunctionListEntry`) sharing one
calling convention, its own cut-down regex, unicode and bignum, no unwinding,
compact values.

Monty is 10.2 MB stripped at opt 3 [M v1]. The size comes from generic
monomorphization (about 2.35 MB of duplicate instances before LTO), general
crates (the regex stack about 774 KiB, `unicode_names2` about 0.86 MB, decimal,
num-bigint, chrono/jiff/speedate), panic/unwind/fmt metadata (about 0.7 MB of
`.eh_frame`), builtins written as large `match` dispatch, serde derives on the
heap (about 24% of its LLVM IR), and stdlib that can't be gated because heap
enum variants keep every type alive [M v1].

Rust stays the language: Rust hosts embed it in process, memory safety matters for an
untrusted sandbox, and size comes from the rules in §5, not from the language.

## 4. Language subset (Python 3.14)

★ marks the first slice; the rest is parity.

Why 3.14: Monty's corpus is written against CPython 3.14, so it is the oracle
that makes the corpus usable (on 3.14.7, 645 of 649 non-`monty-specific`
cases pass; on 3.12.14, 443 [M]), and 3.14's error messages and tracebacks
are the most helpful to models (§8.4). 3.14 syntax additions are either
rejected (t-strings) or need no evaluation (annotations are parsed and never
evaluated, matching deferred annotations). Status in Monty is noted where
micro-monty differs.

- ★ Literals: `int` (arbitrary precision), `float`, `str`, `bytes`, f-strings,
  `list`, `tuple`, `dict`, `set`, `None`, `True`, `False`, `Ellipsis`; `complex`
  at parity.
- ★ Functions: defaults, keyword-only, `*args`, `**kwargs`, closures,
  `nonlocal`, `global`, lambdas, decorators.
- ★ Control flow: `if`/`elif`/`else`, `while`/`for` with `else`,
  `break`/`continue`, `try`/`except`/`else`/`finally`, `raise ... from`, `with`,
  `assert`, `del`, walrus, conditional expressions, chained comparisons.
- ★ Comprehensions and lazy generator expressions (Monty materializes a list).
- ★ `async def`, `await`, top-level `await` in cells, `asyncio.gather`/`run`;
  `asyncio.create_task`, `Task`/`Future` `done`/`result`/`exception`/
  `cancel`/`cancelled`, `CancelledError`, `wait_for`, `as_completed`,
  `Semaphore`/`BoundedSemaphore` (§8.5, `conformance/own/369`); `Lock`,
  `Event`, `Condition`, the queues, `TaskGroup`, `timeout`/`timeout_at`,
  `shield`, `wait`, the running loop's callbacks and timers, task names and
  done callbacks, `ExceptionGroup` (`conformance/own/381`, `382`);
  `Barrier`, `Runner`, `new_event_loop`/`set_event_loop` with
  `run_until_complete`, the asyncio submodules, per-task `contextvars`,
  the never-retrieved report and `KeyboardInterrupt`/`SystemExit` leaving
  `asyncio.run` (`conformance/own/388`, `389`).
- `async for`, `async with` and async comprehensions (allowed in module
  code too, as top-level `await` is), `__aiter__`/`__anext__`,
  `__aenter__`/`__aexit__`, async generators (`asend`, `athrow`, `aclose`,
  async generator expressions) and `aiter`/`anext`, with CPython's
  messages (`conformance/own/322_async_iter_with.py`). An async generator
  runs as a generator frame on the awaiting task's stack, so it parks and
  snapshots like the rest. As with generators, nothing finalizes one that
  is abandoned unfinished: `asyncio.run` does not `aclose()` it at the end,
  and `anext(it, default)` is a `coroutine`, not an `anext_awaitable`.
- Classes, early, because the SDK bootstrap and the Python-written stdlib (§12)
  need them: class bodies, methods, `property`, `staticmethod`, `classmethod`,
  single and multiple inheritance with C3 MRO, `super()`, exception subclasses,
  class decorators, and the core dunder set (`__init__`, simple `__new__`,
  `__repr__`, `__str__`, comparisons, arithmetic and reflected forms with
  `NotImplemented`, `__bool__`, `__len__`, `__iter__`, `__next__`,
  `__contains__`, item access, `__call__`, `__enter__`/`__exit__`,
  `__getattr__`/`__setattr__`, `__init_subclass__`, `__set_name__`).
- Generators, early for the same reason: `yield`, `yield from`, `send`,
  `close`, `throw`.
- Parity: `match`,
  `typing` as runtime no-ops, `enum` (open question §16: a narrow metaclass
  hook or a native `Enum`). `except*` (`conformance/own/387`): the
  compiler keeps a state list `[orig, rest, raised...]`; opcode `EXC_STAR`
  calls the prelude's `_star_match` per clause and `_star_prep` (CPython's
  `_PyExc_PrepReraiseStar`) at the end. t-strings: no.
- Metaclasses: a `type` subclass as `metaclass=`, with class keywords passed to
  its `__new__`/`__init__`, `__call__` overrides, metaclass methods and
  properties, inheritance of the metaclass, and three-argument calls of the
  metaclass (`conformance/own/311_metaclasses.py`). A metaclass that defines
  or inherits `__prepare__` raises `NotImplementedError: micro-monty does not
  support __prepare__` when the class is created, instead of silently running
  the body in a plain dict (`313_metaclass_prepare.py`). A base that is not
  a class goes through its `__mro_entries__` (PEP 560; `typing`'s generic
  aliases, `__orig_bases__` recorded; `conformance/own/401`), and
  `type(name, bases, ns)` with a base whose metaclass is not `type` builds
  the class through that metaclass, as CPython's winning metaclass does
  (`413`).
- Imports: bundled stdlib modules and host-provided modules only, also
  through `__import__` (CPython's signature and errors; `conformance/own/425`;
  owner decision, 2026-11-09). Wildcard
  imports at module level bind the public names, or `__all__` when the module
  has one (`conformance/own/312_wildcard_imports.py`); elsewhere they are a
  `SyntaxError`, as in CPython. No relative imports.
- Bundled stdlib (§12): the default build also carries the modules agent
  code most often imports: `contextlib` (no `chdir`), `traceback`
  (CPython's `TracebackException`, `StackSummary` and `FrameSummary` over
  the engine's frames, with CPython's "Did you mean" rules: a `NameError`
  hint needs a traceback, `AttributeError` and `ImportError` hints do not;
  no colors, no stack-walking functions (`extract_stack`, `format_stack`,
  `print_stack`, `walk_stack`), no keyword-typo hints for SyntaxErrors), a slim `logging` (no `logging.handlers`/`config`, files or
  frame data: records carry CPython's unknown-caller values),
  `urllib.parse`, `pprint`, `shlex`, `difflib` (no `HtmlDiff` or
  `diff_bytes`), `struct` (native; `pack_into` writes into a `bytearray`,
  §7.4), `html` (`html.entities` is its own feature,
  `mod-html-entities`) and `warnings` (filters, `catch_warnings`,
  `deprecated`; with no frame objects, `warn` finds the frame CPython's
  `_warnings` would from the VM's frame chain (`_mm.caller`: filename,
  line of the call, globals for the module and `__warningregistry__`;
  `stacklevel` and `skip_file_prefixes` as CPython's, prelude frames
  skipped), and `<sys>` line 0 in module `sys` off the stack). The
  builtins include CPython's `Warning` hierarchy. `codecs` (Python) has
  the codec and error-handler registries over the codecs `str.encode`
  implements; search functions and error handlers registered from Python
  are seen by `codecs.lookup`/`encode`/`decode` and `lookup_error` only,
  not by `str.encode`/`bytes.decode`; no incremental or stream codecs.
  `platform` (Python) answers with fixed sandbox values that reveal
  nothing about the host (`system()` is `'Linux'` like `sys.platform`,
  node, release, version and machine are empty,
  `python_implementation()` is `'micro-monty'`); the version queries
  read `sys.version_info`. `sys.path` is an empty list: imports never
  search it (`conformance/own/325`–`326`). `tempfile` exists so that
  modules importing it load; everything in it that would touch the
  filesystem raises the sandbox's `PermissionError`, as `open()` does
  (`conformance/own/347`). The bundled `re` and `datetime` emit CPython's
  `DeprecationWarning`s and `FutureWarning`s (`conformance/own/348`).
  Exceptions have traceback objects (`__traceback__`, `tb_next`,
  `tb_lineno`, `with_traceback`), and
  `sys.exception()`, `sys.exc_info()` and a replaceable `sys.stdout`/`stderr`
  for `print` exist for them (`conformance/own/313`–`321`).

- `globals()` (owner decision G2, 2026-11-05): a read-only view of the
  calling module's globals. `[]`, `in`, `get`, `keys`/`values`/`items`,
  iteration, `len` and `==` behave as on CPython's dict and see names bound
  later; every write (item assignment, `del`, `update`, `setdefault`,
  `pop`, `popitem`, `clear`, `|=`) raises `TypeError`, so code that relies
  on writing through `globals()` is refused rather than silently diverging
  (`conformance/own/373`). Writable `globals()`/`locals()` stay separate
  object-model decisions (docs/HANDOFF.md "Scope decisions" 3).
- Codemode stdlib (owner decisions D2, J2, Z1, T1, 2026-11-05): `json.loads`
  takes all five hooks (with one, the native scanner keeps number and
  constant literals as text and objects as flat pairs, and a prelude pass
  calls the hooks children first in document order, CPython's order;
  `conformance/own/364`); `html.parser` (CPython's, with `_markupbase`),
  `calendar` (no `locale`: the C locale only; no command line), `hmac`
  (CPython's pure-Python path over `hashlib`; `compare_digest` is not
  constant-time), `zlib` with only `crc32` and
  `adler32` (`compress`, `decompress` and the rest raise `AttributeError`),
  the `utf-8-sig` codec, and `zoneinfo`: CPython's pure-Python TZif reader
  over TZif bytes the host serves by key (`Session::set_zone_source`).
  Without a source only `ZoneInfo('UTC')` loads and other keys raise
  `ZoneInfoNotFoundError` naming the missing data; `available_timezones()`
  raises `NotImplementedError` (no zone listing). Loaded zones are engine
  state, so a snapshot keeps their rules and a restored session answers them
  without the source (`conformance/own/365`–`368`, `372`). `ast` (opt-in,
  `mod-ast`; not in the default build, where `import ast` raises
  `ModuleNotFoundError` with the "not enabled in this build" hint) builds
  CPython's node classes from the own parser's tree (`parse` in `exec`,
  `eval` and `single` modes, `dump`, `walk`, `NodeVisitor`,
  `NodeTransformer`, `literal_eval`, `get_docstring`, `get_source_segment`
  and the location helpers; no `unparse`, no `compile()` of a tree; type
  parameters and `\N{...}` stay compile errors), and `tokenize` (with
  `token`) yields CPython's `generate_tokens`/`tokenize` stream from the own
  lexer (no `untokenize` or `open`; `0777` is a tokenizer error where
  CPython's `tokenize` module yields a token)
  (`conformance/own/392`, `393`). `json.dumps`
  encodes instances of `str`, `int`, `float`, `list`, `tuple` and `dict`
  subclasses (`IntEnum`, `StrEnum`, `OrderedDict`, `defaultdict`,
  `Counter`) as the values they extend, natively; a `dict` subclass with
  its own `items` and every error (with CPython's `__notes__`) go through
  the prelude's Python encoder. `JSONEncoder`/`JSONDecoder` (`cls=`,
  `raw_decode`, `strict=False`) and the `json.encoder`/`json.decoder`
  submodules exist (`conformance/own/400`). `sys.set_int_max_str_digits`/
  `get_int_max_str_digits` and `sys.int_info` move the 4300-digit limit of
  int/str conversion, also across a snapshot (`409`).

Rejected at compile time with "micro-monty does not support X": `__slots__` semantics,
`__del__`, a user `__getattribute__` (it would cost a check on the attribute
fast path; decision for 0.0.1, 2026-11-10), threads, `exec`/`eval` of modules, imports of anything not bundled or
host-registered, C extensions. A missing import keeps CPython's
`No module named 'x'` and the report adds one sentence per group (data
libraries, network, processes, filesystem, time zones, ...; modules a
build leaves out say so), which the host can replace or drop per group
(`Session::set_hint`; docs/sdk.md "Messages for models").

Source of truth for subset decisions: Monty's `docs/limitations/*.md` and error
messages; copy wording where behaviour matches.

## 5. Engineering rules

- **No `unsafe`.** The workspace sets `unsafe_code = "forbid"`. A crate that
  needs an exception overrides `[lints]` in its own manifest and the exception is
  recorded here first. Expected candidates, each only with evidence: unchecked
  indexing in the dispatch loop if bounds checks cost measurably; a hard
  allocator ceiling in the worker binary.
  **Worker-only exception:** `mm-cell-worker/src/seccomp.rs` may use FFI to
  install Linux's seccomp filter (`prctl`, `seccomp`) and read `getrusage`.
  Its pointers refer only to initialized, owned structures that outlive the
  syscall; the kernel copies the BPF program during installation. The worker
  denies `unsafe` elsewhere. This exception does not permit unchecked engine
  accesses, a custom allocator, or changes to any engine crate's forbid lint.
  The subprocess seccomp test must prove allowed work survives and a forbidden
  syscall terminates the process; worker integration must run with the filter.
  **Wasm-guest-only exception** (owner decision, 2026-11-05):
  `crates/mm-wasm-guest` may use `unsafe` syntax in exactly two places, its
  `mm_host` import block (`unsafe extern "C"` with `safe fn` items: the
  host's clock, park flag, wall clock, entropy, output and zone source,
  and for the bundle guest `bundle_read`) and
  the `#[unsafe(no_mangle)]` attributes of its `mm-wasm` exports. Each is
  a module-level `#[allow(unsafe_code)]`; the crate denies `unsafe`
  elsewhere, has no `unsafe` blocks, and reads and writes its input and
  output buffers as safe `Vec<u8>`s. Pointers passed to imports are to
  live, owned buffers of the stated length. This exception does not permit
  `unsafe` blocks, a custom allocator (the wasm allocator is its own
  exception, below), or changes to any engine crate's forbid lint. `crates/mm-wasm-host`, outside the workspace, has
  one `unsafe` call (wasmtime's `Module::deserialize` of a module it
  precompiled itself, cached under a content key); it is not part of the shipped engine.
  **Wasm-allocator exception** (owner decision, 2026-11-05, temporary):
  `crates/mm-wasm-alloc` may implement `GlobalAlloc` and `Sync` for one
  `UnsafeCell<dlmalloc::Dlmalloc>`. It forwards to the same dlmalloc
  version std uses on `wasm32-unknown-unknown`, with `dealloc`/`realloc`
  through `c_free`/`c_realloc`, so std's `validate_size` assertions are not
  inlined into every deallocation site (the §6 size note). It is built only
  for that target, and `Sync` only without `atomics`. Only binaries
  register it (the guest and the facade's `hello` and `roundtrip`
  examples), never the facade; for embedders it is optional (docs/sdk.md).
  The crate denies `unsafe` elsewhere; its `realloc` example, run under
  wasmtime by `cargo xtask wasm`, checks over-aligned `realloc`. When a
  stable toolchain we pin ships dlmalloc with that check as a debug
  assertion, delete the crate and this exception.
  **Fuzz-allocator exception** (owner decision, 2026-11-09): the fuzz
  crate (`fuzz/`, its own workspace, never shipped or published) may
  implement `GlobalAlloc` in `fuzz/fuzz_targets/memory_meter.rs`: a
  counting allocator that forwards every call to `System` unchanged and
  keeps the current and peak heap in atomics. Only test binaries register
  it (the `memory_bound` fuzz target, the `memory_probes` replay that
  `cargo xtask check` runs, and tests of the memory bound, §11). The
  module allows `unsafe` for that impl alone. This exception does not
  permit `unsafe` in any workspace crate or a custom allocator in a
  shipped build.
- **One builtin calling convention:**
  `fn(&mut Vm, CallArgs) -> Result<Value, Raised>`. `CallArgs` names the
  arguments on the VM stack (`base`, positional count, optional keyword-names
  tuple) instead of borrowing a slice, so the callee can still take `&mut Vm`.
  `Raised` is a zero-sized marker; the exception itself is stored in the VM.
- **Builtins as data.** Functions and methods are static table entries
  `{name, min, max, flags, f}`. Keyword arguments go through one table-driven
  binder. The source table is compile-time only: a `const fn` packs it into
  the shipped `NATIVES` (names and parameter lists in one string blob,
  addressed by `u16` offsets; the implementation pointer is the only
  pointer) with functions first and each builtin type's methods as one run
  sorted by name length, then bytes, so a method lookup is a binary search
  of that type's run.
- **No nested dispatch loops.** A builtin that can reach user code (a dunder, a
  key function, a generator, a user `__hash__`/`__eq__` in a dict probe) takes a
  native fast path while every element is builtin-typed and hands off to
  prelude bytecode on the first user object. This covers `sorted`, `list.sort`,
  `min`/`max`, `sum`, `in`, container `repr`/`==`, `str.join` over a generator,
  and dict lookups.
- **Every Rust recursion is depth-bounded.** Pause-anywhere covers Python
  recursion, not native recursion over syntax or data. The tokenizer, parser,
  compiler, and `repr`/`str`/`==`/`hash`/`copy`/`json` on nested containers are
  iterative or carry an explicit depth counter. Exceeding it raises
  `RecursionError` (or CPython's `SyntaxError: too many nested parentheses` /
  "too many statically nested blocks" at compile time), never a Rust stack
  overflow, which would abort the process. A fuzz target feeds deeply nested
  source and data. At run time nesting lives in the arena: comparison walks
  (`walk.rs`), `repr`, `json`, `copy` and host-value conversion keep explicit
  stacks, so a session that does not compile runs on a 256 KiB host thread
  and under wasmtime's default stack. Only the parser and compiler recurse
  natively, bounded by `MAX_NESTING`; at the limit they need about 650 KiB
  in release builds [M], also inside a session (`Session::compile`,
  `ast.parse` with `mod-ast`)
  (docs/sdk.md, Embedding guarantees). The mm-wasm guest links an 832 KiB
  shadow stack (`crates/mm-wasm-guest/build.rs`): its deepest legal input,
  nested lambda defaults at the limit, takes 670 KB [M], every own case,
  corpus file and bench program at most 41 KB, and
  `crates/mm-wasm-host/tests/stack.rs` keeps the deepest inputs under 90%
  of it.
- **Natives charge fuel** in proportion to work (sort, replace, join, bigint,
  json, re), so a long native call still reaches the deadline check. Int
  `*`, `**`, `/`, `//` and `%`, and the big-int builtins a `CALL` runs
  directly (`pow`, `divmod`, `sum`/`math.prod` of a list or tuple,
  `math.factorial`/`perm`/`comb`/`isqrt`/`gcd`/`lcm`, `sorted` and
  `list.sort` of a list or tuple; `Vm::native_short`), estimate their
  charge first; one of at least 2^16 units that the fuel left cannot pay
  parks with the instruction not started (`Vm::fuel_short`,
  `Vm::fuel_wanted`), and the next `resume` runs it (only after such a
  park: after any other, the next one still checks). A sort estimates
  n·⌈log₂ n⌉ visits (2 s for 4M ints [M]), and one that long also merges
  in slices: a sort of more than 2^17 estimated comparisons
  (`builtins::sort_is_long`) keeps its two arrays and position in a state
  list and merges at most `SORT_SLICE` (2^17) visits, or copies at most
  2^17 items, per native step (`Vm::sort_more`), the rest in `_sort_rest`
  a slice per loop step (`_more`), so a deadline or request is honoured
  inside it. Its first step allocates the two arrays and reads the input
  in place: `sorted` of a list without `key=` runs no Python code and
  never writes it, and `list.sort` moves the list's buffer, not its items
  (`Vm::list_move`). Pass 0 fills the first array with runs of 1,024 items
  (`SORT_RUN_BITS`), each sorted, read reversed for `reverse=True`; the
  merge passes follow, bottom-up: the order is the same stable one; a
  mixed-type list may fail on another pair first. A last pass reverses
  the merged array (`reverse=True`) or maps indexes to items (`key=`), a
  slice per step. The items are detached from the list being sorted, as
  for `key=`; the keys list has room for every key, so it never grows by
  copying. Keys that
  are all small ints, all strs or all floats compare directly (CPython's
  pre-checked compares), and two runs already in order take one
  comparison while that pays (a quarter of the tries in a pass). 1M
  items, best of 5 [M]: strs 1,703 → 676 ms (CPython 270), sorted strs
  1,835 → 130, ints 282 → 112, tuples 799 → 812, `key=len` 260 → 247. `factorial`'s product tree is
  charged at Karatsuba's cost, about three top-level products of 3^d
  multiplications of s-limb pieces at s² limb steps, a unit per six
  (`math::product_cost`: 9–10 ns per unit from n = 50,000 to 800,000,
  where a loop iteration is about 10 [M]; it charged 237,800 units for
  2.4 s before). Superlinear int work estimated past 45 ms
  (`intops::WORK_MAX`) and ints over 2^26 bits (`intops::MAX_BITS`) raise
  `MemoryError` before starting, whatever the fuel and without a fuel
  park (§11): `math.factorial(50_000)`, estimated at 47 ms, is refused.
- **Comparison walks are budgeted.** Native `==`, ordering and key probes
  over builtin data visit at most a fixed number of pairs (`WALK`, 2^14) per
  call and charge one fuel unit per visit. Past the budget they hand off to
  the prelude twin (`_op_eq`, `_op_cmp`, `_sfind`) exactly as for a user
  `__eq__`; the twin descends one level in bytecode and retries each item
  natively, with `_seq_scan`/`_dict_scan` comparing long sequences and dicts
  in native slices. Shared data with exponentially many paths (`x = [x, 1,
  x]`, slow on CPython too) therefore parks on fuel, deadlines and requests.
  The budget is a constant, so parked and unparked runs take the same path.
  Depth carries through set and dict probes, so nested frozensets and keys
  raise `RecursionError`. It also carries across the levels a twin runs in
  bytecode: the twins compare items with `_walk_eq`/`_walk_ord` at their
  nesting `d` rather than with `==`/`<`, so the native depth limit holds
  for the whole comparison. A walk started at nesting `d` gets
  `max(WALK >> d, 1)` visits, so re-walking what the twin then descends
  into costs about 2 × `WALK` plus one visit per level, not `WALK` per
  level (cyclic lists of deep frozensets used to take minutes to raise). Ordering
  finds the first unequal pair in one descent (`_ord`), as `order_seq`
  does natively; a pair that needs Python code falls back to CPython's
  `==`-then-`<` steps.
- **Membership scans charge per item.** `in` on a list, tuple or
  `dict.values()` (or a range, for an item that is not an int), and
  `count`/`index` on lists and tuples and `list.remove`, charge one fuel unit per
  item compared, as a walk charges per visit. Over a long list or tuple
  they go in chunked steps (`nstep::op::SEQ_IN`, `SEQ_SCAN`; "Bounded
  native steps", below); `in` on `dict.values()`, or on a range for an
  item that is not an int, collects the items and compares them in one
  native step (`Vm::contains`), bounded only by the data.
  `in` on an iterator, which need not end, scans `WALK` items natively and
  then hands off to `_op_contains`, which goes on from the same position
  in bytecode and parks on fuel, deadlines and requests. `any` and `all`
  do the same (`_any_rest`, `_all_rest`) on an iterator or range, and
  at the first item with a user `__bool__` or `__len__`.
- **Collecting an iterator parks between chunks.** `Vm::collect` copies a
  list or tuple, and gathers the iterators of a container's data (lists,
  tuples, dicts, sets, str, bytes and their views) in one native step,
  one fuel unit per item: their data bounds them. A range (whose length
  is checked against the memory limit first) can be long, `list(range(8M))`
  took 155–165 ms in one step [M], and any other native iterator (zip,
  enumerate, itertools; a generator, `map` and `filter` included, always
  runs in the twin) need not end, so after `COLLECT_CHUNK` (2^16) items
  `collect` keeps what it took in `Vm::partial` and hands off: 10–11 ms
  a step for the 8M range [M]. A range object goes on from a new
  iterator at the items taken, and a range's list is sized up front.
  `list` and `tuple` of a long range of small ints skip that list: they
  build their result from the range in place, a slice per step, as a
  copy of a list goes (`Vm::range_seq`).
  `Vm::slow_call`
  replaces that iterator in the twin's arguments with a list of the items
  and calls `_collect_rest(xs, it, f, args, kw)` instead, which gathers the
  rest a chunk per loop step with `_more` (each a safepoint) and
  then calls `f`, the native itself or the operator's twin, on the
  complete list. An item that needs Python code part way keeps the items
  before it the same way. Every caller goes through this: `list`, `tuple`,
  `set`, `frozenset`, `dict` (`fromkeys`, pairs from `zip`), `sorted`,
  `str.join`, `bytes`/`bytearray`, `min`/`max`/`sum`, `list.extend`,
  `+=`, unpacking (`*a`, `a, *b = it`, `f(*it)`), the `math` sums and
  products, slice assignment and `e.args = it`. A dict-update pair that
  is itself an iterator goes to the twin before any of it is taken.
  `list(enumerate(range(10**18)))` parks within a fuel slice, a deadline or
  a request, and ends in `MemoryError` at the memory limit as before.
- **Bounded native steps** (the 0.0.1 target, HANDOFF scope decision
  65). On every site `crates/micro-monty/tests/steps.rs` lists as
  `Bounded`, no native step runs longer than about 50 ms between two
  safepoints at the default limits, so a deadline, a park request or
  mm-wasm's epoch grace (100 ms plus 0.75 ms per MiB of the
  memory limit) waits at most that long on one. It is not yet an
  invariant: full collections (§5.1 clause 2) and sites no row covers
  can run longer. A native whose work
  grows with its data runs its first step itself, allocating nothing
  while the work is short, and stops after `nstep::STEP_WORK` (2^17) fuel
  units of it (`Vm::work` counts every unit `burn` charges, fuel limit or
  not). It then keeps its progress in an `NSTATE` object, raw word 0
  naming the operation (`nstep::op`, `nstep::STEPS`, part of the build
  fingerprint), and requests `_steps(state)`, which calls `_more(state)`
  until it gives something other than `state` back: each step is a call,
  a safepoint, and a park or snapshot between two steps sees a prelude
  frame and a heap object, so the snapshot format does not change
  (`Vm::chunk_rest`, `Vm::chunk_step`). A step that meets work needing
  Python code hands the rest to a prelude twin from where it is, the
  request's value being the operation's. `x in s` over a list or tuple is
  the first (`nstep::op::SEQ_IN`, twin `_seq_has`): 10M items ran 231 ms
  in one step, now about 7 ms a step [M]. A chunk that builds a
  guest-sized Rust buffer admits it against the memory limit first
  (§11). `crates/micro-monty/tests/steps.rs` is the oracle: each row runs
  a site near the default limits with a deadline set again after every
  park, and fails if a `Bounded` row's longest gap between two
  safepoints passes 50 ms on the reference machine, scaled to the one
  running it (§5.1 clause 2; release; `cargo xtask check` runs it alone);
  a row marked `Open` is reported without failing (none is, at 0.0.1).
  The `run_source` fuzz target is the search for sites no row knows: each input also runs with
  a 1 ms deadline set again after every park, and a start or resume that
  overshoots it by more than `MM_FUZZ_STEP_MS` (1,000 ms; the
  instrumented build runs native steps 2 to 15 times slower than
  release [M]) twice in a row is a finding. `MM_STEP_MS=N mm run FILE`
  runs a program the same way in release and exits with status 3 when a
  start or resume runs more than N ms past its deadline, so difffuzz and
  the soak check the bound too (`MM_STEP_MS=250` under their load); it
  found the range collect above in steps.rs's own setups. Fuel is bounded the same way:
  on slices of 1,000 units, no slice of a bounded row charges more than
  2^21 + 2^17 units past what it was given (a chunked step charges about
  `STEP_WORK`; one unchunked operation over one object at the cap reads
  at most 2^20 units' worth and builds as much), unless the operation
  parked first with `fuel_wanted` and the slice gave that; the oracle
  checks it on every `Bounded` row. No object is bigger than
  64 MiB (`mm_core::MAX_OBJECT_BYTES`, §11; owner decision (a),
  2026-11-09), so a native whose work over one object stays under 50 ms
  at that size (a copy, a fill) is one step and needs no chunking.
  Dicts and sets growing an insert at a time (P4): once a full table
  holds 262,144 live entries (`set::GROW_STEPS`), the insert that needs
  room hands the growth to `BULK` (`Vm::bulk_grow_top`), as `update`
  grows a table: the new tables a step each, the old entries a slice per
  step, then the item, in CPython's order. Only callers that can hand a
  request on do so (the store opcodes, comprehensions,
  `dict.__setitem__`, `setdefault`, `set.add`); a key hashed in Python
  stores through the prelude twin, whose growth is one step, as is any
  native's own insert. `SET_ADD` steps from its out-of-line
  `set_add_inner`, leaving what `Vm::dispatch` inlines unchanged: every
  shape that changed it moved wasmtime's loop bench 13–16% [M]. A dict's doubling at 1,048,576 entries took
  24–42 ms in one step and a set's growth to 8M slots 90–101 ms; now
  10–16 ms and 26–32 ms (`set.add`, a set comprehension), the step
  allocating the 64 MiB keys table [M].
  Only while a 32nd of the limit is left beside the new tables
  (`Vm::grow_in_steps`): between two steps a collection may begin an
  incremental mark that holds all garbage until its remark, and near the
  limit that failed allocations the one-step growth survives
  (`copy.deepcopy` of 600k lists beside 1M old ones raised `MemoryError`
  at 256 MiB; it needs 242.5 MiB [M]).
  A chunked native that fills a dict of its own moves it a slice a step
  with `Vm::dict_grow_slice` (its doc states what the state carries).
  Numbers (P3): `int()`, `float()` and `complex()` read a long text 2 MB
  (a non-ASCII char counting 32) per step in one chunked text engine
  (`op::NUM_TEXT`: one char loop and step, a grammar per op), keeping a
  few words of state: `float`/`complex` reduce it to a short canonical
  text (800 significant digits and a sticky one, one exponent), `int` its
  digit range, and a
  long `int()` text's digits are then converted into the result's block a
  piece per step. A failing `int()` shows CPython's `%.200R` of its first
  800 bytes, its quote chosen from the whole text; a failing `float()` or
  `complex()` past 64 KB raises in `_num_error`, whose repr is the
  prelude's. The `math` folds (`fsum`, `prod`, `sumprod`, `dist`,
  `hypot`) go a slice per step (`op::MATH_FOLD`). Under the 64 MiB
  object cap two stay one step: `float.fromhex` reads a word at a time
  (38 ms at 64 MiB [M]) and `int.to_bytes` fills its result in the arena
  (34 ms at 64 MiB [M]). Failing `int()` on 30M chars: 126 ms in one
  step, now 14 ms a step [M]. A big int's power-of-two digits (`bin`,
  `oct`, `hex`, `format` and f-strings with `b o x X`, `_` groups, sign,
  prefix and padding; `%x %X %o` past a million digits through
  `_percent`'s `format`) are written from the end of their str's block
  2M digits a step (`op::NUM_TEXT`; `format(n, 'b')` at the int cap was
  203-283 ms in one step, now 1.5-2.2 ms a step [M]). A number padded to
  a width past 8 MB is formatted unpadded, then padded and zero-filled,
  grouped as CPython's, by `STR_FILL` (71-294 ms in one step before, now
  4-5 ms [M]). A lone f-string field is its formatted str: the compiler
  emits no `BUILD_STRING` for it, as CPython 3.14 emits only
  `FORMAT_SIMPLE`/`FORMAT_WITH_SPEC`.
  json and `unicodedata` (P5): `loads` copies a window of input a step
  (`nstep::op::JSON_SCAN`), the containers still open kept between steps
  as arena containers of their items so far, a step's items joining
  them at its end. A step ends before its items could fill an open
  object's table of 2^16 entries or more, or before a child closes into
  such an object that is nearly full, and the next steps grow the table
  while nothing writes to the object: the new entries table, the new
  index, then the entries moved a slice per step (`Vm::dict_grow_slice`;
  the 2^20 to 2^21 rebuild was one 66–84 ms step, now 25 ms at worst for
  2M keys [M]). A string, number or
  whitespace run too long for a window is read in the arena a slice a
  step and written into a byte block of its final size. `dumps`
  (`op::JSON_DUMP`) keeps its open containers between steps and appends
  each step's text to one byte block that becomes the result
  (`Heap::str_from_block`, no parts and no join). Once large the block
  has a segment of its own whose room is reserved ahead, unfilled, and
  admitted against the limit first (`Heap::append_bytes`); the room
  grows by what the text needs, at most 16 MiB at once, in a step of its
  own when the last step's text would not fit, and the result gives the
  rest back. Growing in place may copy the whole block in one step (glibc
  serving it from its heap after a large free, dlmalloc on wasm: 32–41 ms
  for 53 MiB on wasmtime [M]), so a text of 4 MiB or more grows by moving
  to a new block 4 MiB a step instead, and falls back to growing in place
  only when the limit cannot admit the new block's spare room. Either way
  the old text and the new room are admitted together (§9.2). A text past
  64 MiB is a `MemoryError`. It writes a str over 4 KiB a slice a step
  and leaves the keys of a dict over 4,096 entries under `sort_keys` to
  the prelude's `list.sort`.
  `unicodedata.normalize` and `is_normalized` (`op::UNI_NORM`) take a
  long str a slice a step, each slice ending before a char whose
  decomposition starts with a starter that is no composition's second,
  so the slices normalize independently, and a step ends before a slice
  that would take it past `STEP_WORK`. A run without such a char is
  normalized in one step, so it is capped at 65,536 chars after
  decomposition (10–15 ms, at most 2^21 units [M]; 1M combining marks
  took 161 ms before); past it, `MemoryError`. `ast.parse` and `tokenize`
  lex, parse and build in one step, so their source is capped: 32 KiB
  for `ast.parse` (the worst shape found, a statement every two bytes,
  29–38 ms) and 64 KiB for `tokenize` (a token a byte, about 22 ms) [M];
  a longer source raises `MemoryError`.
- **Results and byte work are charged where they are built.** The arena
  constructors charge one unit per 64 bytes of a str, bytes or big-int
  result and one per 8 slots of a tuple, list, dict or set table
  (`Vm::burn_result`), so every native that builds something large pays
  for it, whatever path got there. Scans over text (str and bytes methods,
  `in`, hashing bytes, `re`'s literal skip-ahead, comparing long texts
  inside a walk, where one visit covers 64 bytes and any excess over the
  budget is charged with the walk) charge one unit per 64 bytes they read.
  Collecting a list or tuple into a native charges one unit per item, as
  collecting an iterator does; set and dict algebra and equality charge one
  visit per entry, identity hits included. `hash` of a tuple visits at
  most `WALK` tuples natively, then hands off to `_hash`, which hashes each
  tuple once, so shared nesting (`t = (t, t)`) is linear.
- **One native step stays short.** Native work that was quadratic in its
  input is linear: a class's subclasses are revisited once (version
  stamps, not a `seen` list), a single-base class takes its base's dunder
  flags and its own, the C3 merge counts tail occurrences, a column on a
  non-ASCII line counts at most 256 bytes from a checkpoint (`Lines`), a
  traceback splits each source once, and `strptime` counts the whitespace
  it rescans as steps. Work counted as one step is counted as what it
  reads: a `re` set a step per 16 words, clearing the group registers a
  step per 16, a shape's transitions a unit per 8. A container `repr`
  walks at most `REPR_SLICE` (2^17) visits natively, with or without a
  fuel limit, before the twin goes on. Before the 64 MiB object cap and
  the chunked steps ("Bounded native steps", above), work over one object
  near the memory limit was one linear step: building a 100 MB str took
  164 ms, `s + 'x'` 400 ms, `int()` failing on it 0.97 s and `ast.parse`
  0.34 s for 700 KB [M] (docs/HANDOFF.md, scope decision 64). No object
  can now be that large, `s + 'x'` and a failing `int()` go in steps, and
  `ast.parse` and `tokenize` sources are capped at 32 KiB and 64 KiB.
- **Allocations outside the arena are bounded too.** The compiler's AST and
  bytecode buffers count against the source limits; natives build results in the
  arena, not in unbounded Rust `Vec`s.
- **Generics:** none across crate APIs; inside a crate only when monomorphized
  once or twice. Prefer `&mut dyn Write` and `&dyn Trait` on cold paths.
  The private admitted-buffer `NativeVec<T>` also serves JSON's encoder and
  decoder frames and renderer work items alongside `Value`; this shares
  allocation/charge lifetimes, with the extra specializations measured by the
  size gate.
- **No serde in the engine crates.** Snapshot and host-value encodings are
  hand-written. (`mm-cell-worker` uses serde through its protocol crate,
  `plakous-cell`.)
- **Features gate size.** Each stdlib module is a cargo feature that removes its
  types, not just its import.
- **No panics on guest input.** Every guest-reachable error is a Python
  exception. Clippy denies `unwrap`, `expect`, `panic!`, `todo!`. The size
  builds use `panic=immediate-abort`; any panic is a bug the fuzzers should
  find. In-process embedders build with `panic = "unwind"` (§10.3).
- **Avoid `{:?}` in shipping paths; use `core::fmt` sparingly.**
- **Use what `core` provides before hand-rolling.** Float formatting:
  `{:e}` already yields shortest round-trip digits (verified for `0.1`, `1e16`,
  `1/3`, `5e-324` [M]); lay them out with Python's repr rules. Float parsing:
  `f64::from_str`. Case mapping: `char::to_uppercase`/`to_lowercase` (full
  mappings, `"ß".upper()` is `"SS"` [M]); check `isalpha`/`isdigit`/`isnumeric`
  against Python's categories, which differ in places.
- **Hand-roll the rest:** a fast fixed-seed hash (a folded 64×64→128 multiply
  per 8 bytes; plain Fx spreads poorly in power-of-two tables [M]), a compact
  backtracking regex with fuel, bigint, unicode tables `core` lacks. A crate is
  allowed only when it beats the hand-rolled size in the size budget.
- `random` uses MT19937, so seeded sequences match CPython.

### 5.1 The bounded-work contract for natives

What every native (builtin, operator, method or module function) whose
work a guest can make large promises the host, stated once (architecture
review item 4; owner decision 2026-11-09). The mechanisms that meet it
today are the rules above; consolidating them into one is 0.0.2 work, after
the P1–P5 packages. Where today's code falls short, the clause says so
under **Today**.

1. **A step does Rust work over arena data and nothing else.** It runs no
   Python code and no second dispatch loop (§8.2), reads guest data in
   place, builds results in the arena, and admits a guest-sized Rust buffer
   against the memory limit before filling it (§11). It keeps no state on
   the Rust stack past its return: whatever it has done so far is in the
   heap, where a collection, a park or a snapshot sees it.
2. **A step is short.** Between two safepoints a native does at most
   about 50 ms of work in a native release build at the default limits
   (256 MiB memory, no object over 64 MiB, `mm_core::MAX_OBJECT_BYTES`):
   at most `nstep::STEP_WORK` (2^17) units for a chunked step, or one
   operation over one object at the cap. On wasm the bound is the host's
   epoch grace, 100 ms plus 0.75 ms per MiB of the memory limit, about
   1.5–3 times the native time [E]. On V8 (Node, workerd) the clock does
   not move while wasm runs, so fuel is the only bound, and a step's time
   is its units times the native's time per unit: 0.03–0.05 µs a unit for
   bytecode, up to 0.48 µs for the natives charged most cheaply
   (docs/research/workers-v8.md §8 [M]). The 50 ms bound is a native
   number, and does not carry over to V8 by itself: there a step is short
   only when its units are few, which the chunked steps' `STEP_WORK` cap
   gives (2^17 units, about 4–63 ms) and an unchunked operation over an
   object at the cap does not (up to about 2^21 units, about 1 s at
   0.48 µs). **Today:** the native bound is
   measured on every `Bounded` row of `crates/micro-monty/tests/steps.rs`;
   every row is `Bounded`. The allocation floor is closed:
   a large list buffer, tuple or set key table comes unfilled from a
   fresh zeroed segment (`Heap::alloc_unfilled`), so its pages fault in
   as the steps write it, a set's keys are marked empty a slice per step
   without the write barrier, a sort's second merge array comes in a
   step of its own, and a full list of 1M items or more grows a slice
   per step on `append` and `insert` [M: a 4M `sort(reverse=True)`
   36–39 → 11–14 ms, `a |= b` 26–30 → 12–14 ms, `xs += xs` 23 → under
   1 ms for the allocation]. The 50 ms is on a
   reference machine (an Amp orb, 16 cores under gVisor); the oracle
   scales it by the machine it runs on, so a row's verdict means the same
   on every orb and on a developer's Mac. Three probes, timed before and
   after the rows (the faster run counts, as load only slows a probe):
   allocating and filling a fresh 64 MiB block, the cost of a step that
   allocates an object at the cap; four chunked `STEP_WORK` steps of
   `x in list`, the cost of native work; and a million dependent loads
   from random slots of a 64 MiB table, the memory latency that bounds a
   mark or a probe over a big table (rows took twice as long on an orb
   with 144 ns loads as on one with 64 ns [M]). The limit is 50 ms times
   the largest ratio of a probe's time to the reference's (28.1 ms,
   11.6 ms and 144 ms [M]), clamped to 0.5–2; a machine past 2 fails the oracle as unable
   to judge it. A `Bounded` row over that limit runs twice more and its
   least counts, as a probe's does: a load spike only lengthens a step. Before the calibration, one row (`a |= b` at 1.25M+1.25M)
   measured 45 ms on one orb and 60–70 ms on another [M]. A collection the
   heap starts itself is not a native step: over 32 MiB live it marks a
   slice per safepoint, and under it it runs in full over at most about
   64 MiB (30 ms native at worst [M]; §9.2). A park that is due goes
   ahead of a collection step, which runs when the session resumes, so
   one gap never holds both a native's step and a collection step
   (§9.2). Full collections over a
   large heap stay outside this bound (0.40 ms per MiB of live heap
   native, 1.3–1.6 times that on wasm [M]): one a native or the engine
   requests (`Vm::request_gc`: after a `MemoryError` is raised, after a
   large result once allocation reaches half the room left, after a byte
   job fails for memory), a snapshot's and code reclamation's. A mark with too little room left
   and one that gives up on scattered writes are held, a slice per check
   with no guest code between (§9.2). A fresh 64 MiB block costs one step of about
   30–40 ms natively, most of it page faults and the `UNDEFINED` fill in
   `Heap::alloc_object` (the probe's 28 ms of it on the reference [M];
   single runs to 57 ms on gVisor), until mm-core leaves a fresh block
   unfilled where a caller writes its slots in steps; and on V8 at 256 MiB, natives inside the unit
   bound took 0.2–0.9 s a step on 8cd850c (`str.upper` 2.0M units
   in 927 ms; `int(str)`, `bytes.find` and `str.find` about 1.0M units
   in 0.3–0.4 s [M], workers-v8.md §8), before the P1–P3 packages
   chunked them; at a 16 MiB limit they and `str.translate` took
   43–132 ms. Neither V8 nor the shipped guest under wasmtime has been
   measured per site since.
3. **Fuel pays for the work, and a slice is overdrawn by at most one
   step.** Every unit of work is charged: a unit per item visited,
   compared or collected, per 64 bytes read or built, per 8 slots of a
   table built, per safepoint run. No slice of fuel is overdrawn by more
   than 2^21 + 2^17 units, unless the operation parked first, before
   starting, with `Session::fuel_wanted` set to its cost and the host then
   gave that much. The contract's rule is to charge **before** the work: a
   step reserves its cost (at most `STEP_WORK`), or, when one operation
   costs more than the fuel left and at least 2^16 units, it parks with
   the operation not started. **Today:** most natives charge after the
   work (`Vm::burn`), and the overdraft bound holds because each step is
   short; only the int operators, the big-int builtins and the sorts
   estimate first (`Vm::native_short`, `Vm::fuel_short`). The bound is
   checked on every `Bounded` row, which is every row at 0.0.1.
   A sweep of 1,137 calls (every public
   method of the builtin types, the builtins and the native modules'
   functions, on a receiver near the default limits) found 202 that
   overdraw a 1,000-unit slice on 152994a [M], 64 operations in all:
   `str.replace` (246M), `str.translate`, `bytes.expandtabs` and an
   all-safe `urllib.parse.quote` (61–63M), collecting a 60M-char str
   (17M), and sets, dicts, json, reprs, hashes and the folds (2.3–8.4M).
   The P1–P5 packages have since given `Bounded` rows to `str.replace`,
   `str.translate`, `bytes.expandtabs`, `urllib.parse.quote` and the other
   sites steps.rs lists, among them `min`, `max`, `sum`, `any`, `all`,
   `hash(tuple)` and `Vm::collect` over str, bytes, set, frozenset and
   dict sources (`nstep::op::FOLD`, `_collect_rest`); the sweep has not
   been rerun.
4. **Results do not depend on how the host slices the run.** Output,
   exceptions, the values left and the fuel spent are the same whether
   the cell runs unparked, in fuel slices of any size, past deadlines or
   through snapshots restored in other processes and on the other target.
   Budgets that pick a native or a twin path are constants, not the fuel
   left. **Today:** outputs and outcomes are checked by durability
   (parked every few units, and across targets) and the fuel spent by
   `Vm::charge_ticks` (c266d16: within 0.2% across slices of 1 to 30,000
   [M]); a container `repr` walks `REPR_SLICE` visits natively whatever
   the fuel left; and durability compares reports, not fuel totals. Near the
   memory limit, a snapshot collects garbage before it writes the image,
   so an operation the engine does not run again after a collection
   (§9.2) can raise `MemoryError` unparked and succeed after a restore at
   a park just before it; operations that do run again give the same
   outcome either way. An operation that fails partway near the limit
   is charged for the work it did, so a snapshot just before it,
   restored or not, can change the fuel total (+28,672 units, +0.64%, on
   the reviewer's `f(*big, **kw)` [M]; docs/sdk.md, Other differences).
   Fuel totals can also depend on restores when a cell indexes or
   searches a long non-ASCII str: the char-position cache is not in the image, so the
   first walk after each restore is charged again (an index loop: 43,313
   units unparked, 8,687,009 with a restore at every 20,011-unit park
   [M]; docs/sdk.md, Other differences).
5. **A long operation parks between steps and resumes where it stopped.**
   A step ends in one of three ways: with the operation's value; with its
   progress saved in an `NSTATE` object and a request to the prelude's
   `_steps`, which calls `_more(state)` once per step (each call a
   safepoint); or by handing the rest to a prelude twin with its progress
   passed explicitly, so nothing is done twice. A park or snapshot
   between steps sees only prelude frames and heap objects, so the
   snapshot format does not change for a new chunked operation; its
   `nstep::STEPS` entry is part of the build fingerprint, and a
   durability case parks inside it. **Today:** the `need_py` hand-off
   (74 sites) restarts the whole call in the twin from its reconstructed
   arguments, undoing what it consumed (`Vm::consumed`), and eight `Vm`
   mode flags carry the hand-off state, four of them in snapshots; keyed
   sorts resume through `Cont::Native`; iterators collect through
   `_collect_rest`.
6. **Up-front routing.** Builtin-typed data takes the native path; data
   that can reach user code goes to the Python implementation before any
   work is done. **Today:** natives find user code part way and hand off
   then (clause 5).

The contract is checked by `steps.rs` (clauses 2 and 3, `Bounded` rows),
the `run_source` fuzz target's step-time oracle (clause 2, sites no row
knows), durability on both targets (clauses 4 and 5) and the own cases
that park inside every chunked operation (clause 5).

## 6. Crates, size and the parser

| Crate | std? | LOC first slice / parity [E] | Contents |
|---|---|---|---|
| `mm-core` | no_std | 3k / 4.5k | `Value`, arena (word and byte spaces), mark-sweep GC with side mark bitmap, write barrier, interning, shapes, FULL arena export/import |
| `mm-compile` | no_std | 3k / 5k | Engine AST, symbol tables, bytecode, exception and line tables, IC slot allocation, constant folding, superinstructions. Depends on no heap |
| `mm-engine` | no_std | 8k / 15k | Dispatch loop, frame stack, calls, exceptions, generators and coroutines, async scheduler, ICs, fuel, parking, host calls, builtin types and functions as tables, the prelude |
| `mm-stdlib` | no_std | 1k / 3k Rust + Python modules | One `mod-*` feature per module; the bundled Python modules under `python/`, compiled at build time (§12), and the feature bitmask; native parts live in `mm-engine` |
| `mm-parse` | no_std | 0.3k / 6–8k | The own parser: indentation tokenizer and Pratt parser to the engine AST. The phase-1 ruff front end survives only behind the `ruff-oracle` feature, the oracle of `cargo xtask parse-diff` |
| `micro-monty` | std | 1k / 1.5k | Public API: sessions, limits, progress and resume, park, snapshots |
| `mm-cell-worker` | std, bin | 0.5k / 1k | An embedding example: a worker for one external cell protocol (`plakous-cell-protocol.v2`), used in local integration testing; `publish = false` |
| `mm-wasm-abi` | std | 1k | The `mm-wasm` guest interface: export and import names, codes, the binary encoding (docs/research/wasm-embedding.md) |
| `mm-wasm-guest` | std, cdylib | 0.5k | micro-monty as a wasm module speaking `mm-wasm`; `publish = false`; §5 exception |
| `mm-wasm-alloc` | no_std | 0.1k | std's dlmalloc for `wasm32-unknown-unknown` without the per-site `validate_size` checks; registered by binaries only; `publish = false`; §5 exception (temporary) |
| `xtask` | std, bin | — | size budget, conformance runner, benches |

Dependency direction: `mm-core` and `mm-compile` depend on nothing; `mm-engine`
on both; `mm-stdlib` on `mm-engine`; `mm-parse` on `mm-compile`; the facade on
all; the worker on the facade and `plakous-cell`; `mm-wasm-abi` on the facade and
`mm-wasm-guest` on both. The wasmtime reference host, `crates/mm-wasm-host`,
is its own workspace (excluded from the root), so wasmtime never enters the
root lockfile.

The `mm-wasm` interface is frozen with 0.0.1 (`ABI_VERSION` 1;
docs/research/wasm-embedding.md). A release may add an optional export, or
a session option: a new tag in the options record whose default keeps the
behaviour without it. Only an option a guest can safely run without (a
hint, a diagnostic) may take an ignorable tag (`0x80` and up), which a
guest that predates it skips. An option that limits or changes behaviour
(a limit, the clock, entropy, output, the text cut) takes a strict tag
(below `0x80`), so an older guest refuses the record rather than run
without it. Anything else bumps `ABI_VERSION`.

`plakous-cell`, the protocol crate, comes from a pinned git revision because
it is not on crates.io. It brings tokio and nix (the host-side client; nix is
used for `getrusage` in the worker's resource report). Only the unpublished
worker depends on it.

### Size

Reference points:

| Binary (stripped, opt 3, fat LTO, panic=abort) | Size |
|---|---|
| empty Rust binary | 0.29 MB [M, 4 vCPU orb, Rust 1.99.1] |
| `ruff_python_parser::parse_module` hello | 1.63 MB [M, same] |
| `unicode_names2::character` hello | 1.16 MB [M, same] |
| rquickjs (quickjs-ng) hello | 1.61 MB [M v1] |
| dquickts hello | 1.84 MB [M v1] |
| Monty hello | 10.2 MB; 9.0 MB with fat LTO + panic=abort [M v1] |
| MicroPython / PocketPy | 0.54 MB / 0.61 MB [M v1] |

`unicode_names2` is a required dependency of `ruff_python_parser` 0.0.14 (used
for `\N{}` escapes) [S], so ruff costs about 1.3 MB over an empty binary, not the
258 KiB of `.text` v1 counted. Consequences:

- ruff is used only to get the compiler and VM running; the own parser lands
  before the MVP size gate (§14).
- `\N{...}` is unsupported by default.

Budgets for the facade's `hello` example and the two mm-wasm guests,
enforced by `cargo xtask size`. Raising any needs a spec change. Since 2026-11-03 (owner decision) both
measure **shipped builds**: the pinned stable toolchain with
`RUSTC_BOOTSTRAP=1` rebuilds std (`-Zbuild-std=std,panic_abort`, from the
`rust-src` component) with `-Cpanic=immediate-abort` and
`-Cforce-unwind-tables=no`, which removes std's panic formatting, backtrace
symbolization (gimli, addr2line, miniz, demangle) and `.eh_frame`. A panic in
a shipped binary aborts without a message. Since 2026-11-09 (owner decision
69) shipped builds also pass `--cfg mm_shipped`, which turns on
`#[optimize(size)]` for part of the cold breadth through
`cfg_attr(mm_shipped, …)` (the file list is in `docs/HANDOFF.md`, "Engine
size review"). `cargo xtask check` runs the own cases on a natively built
shipped `mm` so that shipped codegen has a native gate. Tests, clippy and
every other build stay on plain stable; `cargo xtask wasm` builds its examples as shipped
too. Native binaries stay position-independent (the worker runs untrusted
code). `optimize_for_size` std was measured (−12.8 KB native, json +0.9%
instructions) and not taken. Shipped wasm modules then go through binaryen's
`wasm-opt -Oz` (version 133, installed by `.agents/setup`) with exactly the
features rustc enables; `cargo xtask size` gates and `cargo xtask wasm` runs
the optimized modules (`target/<target>/release/shipped/`). It saves 144,610
bytes on `hello.wasm` [M] and costs 8–13% wall time on the dispatch-heavy
bench programs (fib, loop, closure; str +5%, dict and json within noise) when
`mm` built for wasip1 runs under wasmtime 50 [M]. Every `wasm-opt` level
measured slower than none; `-O3` is within a few percent of `-Oz` and 14 KB
larger [M] (`docs/HANDOFF.md`). PGO (native `mm` −8% to −22% instructions
on the bench programs, `hello` +28% bytes) and BOLT on top of it (no gain)
were measured and not adopted; the numbers are in `docs/HANDOFF.md`.

| Budget | Artifact | In `xtask/size-ceilings.txt` |
|---|---|---|
| Primary | `wasm32-unknown-unknown` `hello.wasm`, the in-process embedding, after `wasm-opt -Oz` | `hello.wasm` |
| Secondary | native stripped `hello` (x86-64 Linux, PIE) | `hello` |
| Guest | `wasm32-unknown-unknown` `mm_wasm_guest.wasm` (the mm-wasm guest, `crates/mm-wasm-guest`, no `test-hooks`), after `wasm-opt -Oz` | `mm_wasm_guest.wasm` |
| Bundle guest | `wasm32-unknown-unknown` `mm_wasm_guest_bundle.wasm` (the guest built with `host-bundle`; its stdlib and prelude bytecode in the custom sections `mm.prelude` and `mm.stdlib`, served by the host through `mm_host.bundle_read`), after `wasm-opt -Oz`, sections included | `mm_wasm_guest_bundle.wasm` |

The ceilings, the measurements behind them and their history are in
`xtask/size-ceilings.txt`, not here: each ceiling is the sum of its
artifact's lines there, and `cargo xtask size` prints each artifact's
current size beside its ceiling.

The guest budget (owner decision, 2026-11-05) gates the module an in-process
wasm embedder ships; `hello.wasm` stays the primary budget. It was set about
0.5% above its first measurement. The guest was 142,565 bytes over
`hello.wasm` then and is 86,903 over it now [M]: the `mm-wasm` codec and
exports, and facade calls `hello` does not make (per-export ablations in
`xtask/size-ceilings.txt`).

**Capability gaps (2026-11-07, owner request under S1).** Each measured in
order on 266bde7 [M] (`hello.wasm` / native `hello` / guest); bench
callgrind Ir within ±0.24% on every program (`str` +0.238%):

| Item | `hello.wasm` | native | guest |
|---|---:|---:|---:|
| before | 1,999,470 | 2,177,048 | 2,103,826 |
| ImportError `name`/`path`/`name_from`, `super().__init__(name=...)` | +1,038 | +1,472 | +1,027 |
| `traceback.TracebackException`, hint rules, SyntaxError attributes | +10,533 | +11,312 | +10,346 |
| `ast` over the own parser | +48,131 | +52,288 | +48,079 |
| `tokenize` and `token` over the own lexer | +6,729 | +8,224 | +6,695 |
| after | 2,065,901 | 2,250,344 | 2,169,973 |

`ast` is mostly Rust: the native node builder (`astmod.rs`, at the
engine's opt-level 3) and the tree's drop glue; its Python is 5,908 bytes
of packed bytecode [M]. It then left the default set (owner decision,
2026-11-07; `mod-ast` opt-in): −43,874 / −46,304 / −46,254 on 168b7ac
[M], less than it added because `warnings` and the parser's tree mode stay.
Details in `docs/HANDOFF.md` ("Capability gaps").

**Size pass (2026-11-05, owner decision 41).** Each lever in order on
874409b [M] (`hello.wasm` / native `hello` / guest), bench callgrind Ir
within +0.21% (startup) and every other program within ±0.012%:

| Lever | `hello.wasm` | native | guest |
|---|---:|---:|---:|
| before | 2,171,955 | 2,286,168 | 2,322,247 |
| `crates/mm-wasm-alloc` (below; §5 exception) | −83,553 | 0 | −97,112 |
| facade (`micro-monty`) at `opt-level = "z"` | −12,541 | −18,928 | −37,548 |
| audit fixes, together | −12,736 | −6,176 | −20,204 |
| after | 2,063,125 | 2,261,064 | 2,167,383 |

The facade is the one package that gains from a size level: `mm-core` at
`s`/`z` costs 3–22% Ir; `mm-compile` at `z`, `mm-wasm-abi` at `s`/`z` and
`mm-stdlib` at `z` save nothing (the VM, builtins and stdlib natives are
`mm-engine`, which stays at 3). `Vm::new` is kept out of line and the
snapshot writer lives in the engine (`Vm::write_snapshot_sized`), so
neither runs at `z`. Fat LTO optimizes the whole program at the root crate's level, so
`cargo xtask size` builds `hello`, the facade's example, with its own
crate at opt 3 (`cargo rustc -- -Copt-level=3`), as an embedder's binary
is; built at `z` it would be another 59 KB smaller, a saving no embedder
gets. Audit fixes alone on the facade commit: traceback chain and caret
sets in `NativeIds`/a `Vec` instead of `BTreeSet`s −5,271 / −5,440 /
−5,280; `text::debug_quoted` instead of `{:?}` −4,213 / +1,472 / −4,099;
one shared Huffman header for the bundled modules −3,231 / −2,208 /
−3,219; merges instead of slice sorts in `environment`/`host_modules`
0 / 0 / −7,604. Details, rejected levers and the stdlib bytecode
analysis are in `docs/HANDOFF.md` ("Size pass").

**Size pass 2: bundled data (2026-11-07, owner decision 46).** On b01fd7d
[M] (`hello.wasm` / native `hello` / guest), bench callgrind Ir within
±0.02% on 12 programs and +0.31% on `startup`:

| Lever | `hello.wasm` | native | guest |
|---|---:|---:|---:|
| before | 2,107,210 | 2,311,512 | 2,211,094 |
| `NATIVES` in its own static, not a borrow of the whole const `Packed` | −5,444 | −33,200 | −5,444 |
| bundled modules in sections, with shared strings and per-section codes | −102,523 | −101,520 | −102,771 |
| after | 1,999,243 | 2,176,792 | 2,102,879 |

The bundled modules are written by `Program::encode_sections` as nine
byte streams: own strings, string references, other metadata, opcodes,
registers, `u` operands (inline-cache slots predicted), and `b`, `i` and
`j` operands. Strings that three or more modules use are stored once,
raw, and made `Arc`s only when a decode uses them. Each stream is packed
against one set of Huffman codes for every module (`pack_shared`). A
session keeps the nine decoders it built for its first decode. The packed
stdlib went from 386,198 to 277,758 bytes [M]. `#[optimize(size)]` on the
cold breadth waited for Rust 1.101; since owner decision 69 part of it is
applied on shipped builds (above), and the rest waits for 1.101
(`docs/HANDOFF.md`, "Engine size review").

**dlmalloc `validate_size` (solved in the size pass by
`crates/mm-wasm-alloc`, owner decision 41; history below).** On
`wasm32-unknown-unknown`, std's allocator is dlmalloc, and its `free` path
runs debug-style `validate_size` assertions that fat LTO inlines into every
deallocation site: the constant they test appears 1,687 times in the guest
against 52 on `wasm32-wasip1` [M]. That is about 90 KB of the guest (a
custom allocator measured 1,925,300 against 2,015,061 bytes at ac10b28 [M])
and roughly 75 KB of `hello.wasm` [E, ~1,413 sites], and it is most of why
the unknown-unknown guest is about 88 KB larger than the wasip1 one. Accepted
as is at first (option (a)); the size pass measured −83,553 / −97,112
bytes [M] for a `#[global_allocator]` wrapping the same dlmalloc through
`c_free`/`c_realloc`, and the owner took it with a temporary §5 exception
until std's dlmalloc makes the check a debug assertion.

The secondary budget is enforced only when `cargo xtask size` runs on
`x86_64-unknown-linux-gnu`. On other hosts (aarch64 macOS, for one) the
native `hello` size is reported but not gated, because the executable format
and code size differ per target (owner decision, 2026-11-05).

Both ceilings were set about 0.5% above the measurement (1,721,143 and
1,825,864), so the savings are kept. Engine work then grew them to
1,726,284 and 1,831,432; outlining the char counter and the software
`sqrt`, `ldexp`, `exp` and `log` (instruction counts unchanged) took
17,782 and 29,696 bytes back [M]. Lowering the ceilings to match is an
owner decision. Speed phase 4 (2026-11-04): the owner approved raising
the ceilings by what the phase's speed work measures over them; in-place
list and tuple comparisons took `hello.wasm` from 1,729,754 to 1,730,337
[M], so the primary ceiling became 1,731,000; native continuation frames
for keyed sorts (`Cont::Native`, `nstep.rs`) took it from 1,730,186 to
1,737,000 [M], so it became 1,738,000. The SDK round (2026-11-04, same
owner rule: the measured overshoot plus a small margin) made the host
surface one declarative `Session::define` with call ids, taking it from
1,737,688 to 1,739,727 [M], 1,727 bytes over; the ceiling became
1,741,000. The direct frame push for continuation calls and implicit
dunders (`Vm::push_func`, inline `cont::Args`) took it to 1,741,354 [M],
354 bytes over; the ceiling became 1,742,500. The nine stdlib modules
agent code most often imports (§4: `contextlib`, `traceback`, `logging`,
`urllib.parse`, `pprint`, `shlex`, `difflib`, `struct`, `html`) took
`hello.wasm` from 1,741,354 to 1,818,715 and native `hello` from 1,830,312
to 1,909,416 [M]; by the same rule both ceilings were raised to 1,823,000
and 1,914,000. Per-module sizes are in `docs/HANDOFF.md`. The engine
gaps then followed, by the same rule, with per-feature sizes in the same
place: `async for`/`async with`/async generators took `hello.wasm` to
1,829,764 and native `hello` to 1,921,416 on their isolated branch [S].
Integrated with restore-without-zeroing and bounded indirect calls, they
measure 1,833,115 and 1,925,464 [M], so the ceilings became 1,834,000 and
1,926,500. The 2026-11-05 batches then raised both by their measured
growth (the record is `xtask/size-ceilings.txt`). Owner decision S1
(2026-11-05) keeps one default build of about 2 MB, with size optimization
and build profiles revisited later: the codemode stdlib round measures
2,081,288 and 2,180,120 [M] with the codemode host API, and the review
fixes take them to 2,091,415 and 2,193,048 [M]; asyncio cancellation to
2,104,417 and 2,207,528 [M]; the speed landing (the interpreter fast
paths +8,834 and the native `datetime` parsers +13,821 on `hello.wasm`,
measured on 9306b80) to 2,127,020 and 2,236,600 [M], and the review
follow-ups to 2,131,850 and 2,241,768 [M], and the asyncio primitives
(+36,025 and +40,160 on d6d77f6) to 2,167,875 and 2,281,928 [M], and the
aware `datetime` fast paths (+4,080 and +4,240 on 77f20ed) to 2,171,955
and 2,286,168 [M], under 2,173,900 and 2,288,100. The size pass then
lowered both ceilings (`xtask/size-ceilings.txt`). The third asyncio batch (`except*`,
per-task `contextvars`, `Barrier`, `Runner`, the submodules; +21,264,
+25,840 and guest +21,397 on 1f27970) raised them under S1 to the
measurement plus about 2 KB (`xtask/size-ceilings.txt`).
Further:
with the prebuilt std, native `hello` was 2,273,048 bytes under the
owner-approved 2,275,000 and `hello.wasm` 1,977,574; before `wasm-opt`
joined the pipeline, `hello.wasm` measured 1,865,753 [M] under 1,873,000.
`wasm32-wasip1` `hello.wasm` is reported, not gated: 2,073,573 [M,
2026-11-05, size pass] after `wasm-opt`, 2,098,010 before the size pass
(its wasip1 guest 2,169,244, 2,225,971 before; wasip1 never paid the
dlmalloc checks). Earlier history: the
owner approved 2,250,000 bytes on 2026-11-02 and 2,275,000 on 2026-11-03 for
the recovered conformance work (2,263,848 bytes [M]): smallness remains a
goal, but individual bytes should not exclude measured correctness or
performance improvements. The prebuilt-std numbers below are historical.

**Current sort-accounting build [M, 2026-11-02, Rust 1.99.0]:**
2,210,552 bytes, under the 2,250,000-byte gate. This is 552 bytes above the
previous ceiling; the sort-workload median improved 2.8% while its temporary
buffers became budgeted. Raw measurements are in `bench/results/session-lifecycle.txt`.

The earlier shared-host-module build measured 2,201,712 bytes; the preceding
lifecycle build measured 2,204,472 bytes [M]. On that build, parser-only
`opt-level = "z"` (compiler stays `s`, VM stays `3`) saved 14,704 bytes versus
parser `s`. The in-process `session_bench` example measured roughly 5–8%
more compilation time for a 92-byte cell and 10–12% for a 50,006-byte cell
(about 0.6 ms extra for the latter). Raw samples and machine information are
in `bench/results/session-lifecycle.txt`. These are compilation measurements,
not a claim about interpreter throughput. Reproduce with:

```bash
cargo build --release -p micro-monty --example hello --example session_bench --locked
stat -c '%s' target/release/examples/hello
taskset -c 2 target/release/examples/session_bench
# Compare on the same machine, without concurrent builds or tests:
cargo --config 'profile.release.package.mm-parse.opt-level="s"' build --release -p micro-monty --example hello --example session_bench --locked
stat -c '%s' target/release/examples/hello
taskset -c 2 target/release/examples/session_bench
```

**At the MVP [M, 2026-11-01]:** `hello` compiles and runs a cell, snapshots
and restores it, so it links the whole engine: 3.01 MB stripped. `cargo
bloat` (`.text`): mm-engine 724 KiB, std 326 KiB, ruff_python_parser 218 KiB,
mm-compile 145 KiB, mm-parse 60 KiB, mm-core 30 KiB; the rest of the file is
read-only data, mostly `unicode_names2`'s tables. Removing ruff and
`unicode_names2` (≈ 1.3 MB, above) puts the engine at ≈ 1.7 MB, inside the
1.6–2.3 MB target. The budget is 3.1 MB until M6 lands the own parser, then
2.3 MB, then 2.0 MB. (Before the MVP, `hello` only printed a version string
and measured 0.29 MB; that number measured nothing.)

| Component (own parser) | Target [E] |
|---|---|
| mm-core + mm-engine | 0.8–1.2 MB |
| mm-compile + own parser | 0.2–0.3 MB |
| mm-stdlib default set | 0.2–0.4 MB |
| Total `hello` | 1.6–2.3 MB |

The `mm-cell-worker` binary is larger (serde, tokio) and is not what the budget
measures.

## 7. Data model

### 7.1 Value: 8 bytes, NaN-boxed

```text
f64            any non-NaN double, or the canonical NaN
tagged (qNaN)  0xFFF8_0000_0000_0000 | tag << 48 | payload (48 bits)
tags           SmallInt(i48) | Ref(u32 offset, optional kind byte)
               | Special(None/True/False/NotImplemented/Ellipsis/Undefined)
               | NativeFn(u32 table index)
```

- `f64::to_bits`/`from_bits` make this safe Rust.
- A `Ref` payload may carry the object's kind in spare bits, so type checks on
  hot paths don't load the header.
- `SmallInt` overflow promotes to a heap bigint.
- `Value` is `Copy` and pointer-free.
- `Undefined` marks unbound locals and is never visible to Python.

### 7.2 Arena and objects

- **Word space:** segments (`Box<[u64]>`, up to 1 MiB each) in a
  segment table. A `Ref` is `segment << k | word`, still a `u32` covering
  32 GiB. Object headers, inline slots, list/tuple/dict buffers and frames live
  here.
- **Byte space:** segments the same way, for `str` and `bytes` data and bigint
  limbs.
- Why segments and not one `Vec`: growing a single `Vec` copies the whole heap
  and briefly needs twice its memory, which matters near a 256 MiB limit.
  Segments are never moved, the memory limit counts segments exactly, and a
  FULL snapshot is the segment list. The cost is one load from a small, hot
  segment table per access.
- An object of 128 KiB or more gets a dedicated segment of exactly its size,
  released when it dies (a run of separate `Box` segments could not hand out a
  contiguous slice without `unsafe`). Segments are therefore not all 1 MiB.
- The byte space's first segment starts at 32 KiB and the word space's at
  64 KiB, and each, while it is the bump
  segment, grows by reallocation, doubling up to 1 MiB; later segments are
  committed whole. A `Ref` addresses the segment's index and offset, so
  growing its backing store changes no `Ref` and the arena stays non-moving
  at the Python level. Bitmaps grow with it, the limit counts what is
  committed, and a snapshot records each segment's capacity, so a restore
  commits the same. A dedicated segment's room past its object (a block
  growing over several steps) is recorded as a number, never as bytes, and
  restore checks it against the object's length, `MAX_OBJECT_BYTES` and
  the memory limit before reserving it. A small session's arena then tracks its live heap: the
  floor for `Session::new` plus `x = 1` fell from 2,387,968 to 394,240 bytes
  [M].
- Every access is bounds-checked. A stale offset reads the wrong Python object,
  never host memory.
- **Char positions.** `s[i]`, slices, the `str` method windows, `re`
  positions and `json` offsets on a non-ASCII `str` need the byte offset of
  a char index (or the reverse). `Heap::str_offset`/`str_index` walk from
  the nearest of the string's ends and up to 8 remembered (string, char,
  byte) positions, eight bytes per step, and remember the result: a short
  walk moves the position it started from (a cursor), a longer one takes
  the oldest slot. Sequential, reverse, two-ended and nearby access cost
  about one char per index [M]; random access into a long string still
  walks to the nearest position. Collection clears the positions before
  sweep can reuse an address, so an entry never outlives its string; a
  collection follows at least k × live bytes of allocation, so the walks
  it causes stay in proportion. Natives charge the bytes walked (one fuel
  unit per 64). Positions are not in snapshots; a restored heap starts
  without them.
- The arena is custom. Existing crates don't fit: `slab`, `slotmap` and
  `generational-arena` store one element type behind 8-byte versioned keys;
  `bumpalo` and `gc-arena` hand out pointers and have no snapshot form;
  `segvec` is typed with growing chunks. The arena is about 600–1,000 lines
  [E], borrowing size classes from mimalloc and line/block marking from Immix,
  and is the most heavily property-tested and fuzzed code in the repo.
- **Non-moving:** size-class free lists, large objects as contiguous runs, no
  compaction. `id()` is stable for free.
- Header (one word): `type:16 | flags:8 | nvals:8 | len:32`. `nvals` counts the
  leading value slots the GC traces (255 = all slots); the rest are raw words.
  Shape ids live in a value slot as a SmallInt. A side object-start bitmap
  validates every reference, so a stale or forged one is an error.
  **No mark bit**: GC marks live in a side bitmap so a collection doesn't dirty
  every live page.

| Type | Layout |
|---|---|
| `str` | byte-space UTF-8, char count, cached hash, ASCII flag; a non-ASCII char index is walked to from the nearest of the string's ends and the heap's last 8 positions found (per heap, not per string, not snapshotted; see below) |
| `int` | `SmallInt`, or heap bigint (sign + limbs), own implementation [E 1.5–2k LOC] |
| `dict` | insertion-ordered entries in one of three forms: inline in the object (literals ≤ 8 entries), linear (an 8-entry buffer scanned by stored hash), indexed (compact u32 slots with hash tags, stored hashes, power-of-two capacity); globals dicts are always indexed so inline caches apply |
| `list` / `tuple` | length plus a word buffer of `Value`s |
| instances | shape id plus inline slots; fall back to a dict past 32 attributes, on deletion or on `__dict__` access |
| classes | name, bases, cached C3 MRO, namespace dict, version tag for IC invalidation |
| functions | code ref, globals, defaults, kwdefaults, closure cells, qualname |
| code objects | immutable bytecode in an engine-owned table outside the arena (written beside the arena in snapshots); functions refer to them by index; unreachable user compilation units are reclaimed and indices remapped between cells |
| exceptions | ordinary instances: args, `__cause__`, `__context__`, compact traceback of (code, pc) pairs |

### 7.3 Interning

Interned strings are ordinary `str` objects with the interned flag, so `str`
has one representation. The arena's intern table is weak: live strings retain
their identity, but unreachable literals and names are removed before sweep can
reuse their addresses. In-place backward-shift deletion preserves probe chains
without allocating another table. Code and shape metadata explicitly trace the
names they own; builtin names remain reachable through the builtins and prelude.
The table retains its high-water capacity inside the arena budget.

### 7.4 `bytearray` (design note)

`bytearray` is the one mutable byte type; `memoryview` stays out (below).

- **Arena.** `ty::BYTEARRAY` objects have one value slot, the `ByteRef` of a
  capacity block in the byte space, and one raw slot, the length (≤ the
  block's length). Growth allocates a larger block and copies with
  mm-core's `Heap::copy_bytes` (an overlapping, cross-segment copy that
  is the write barrier for its destination, like `bytes_mut`); the slot and
  the length change through `Heap::set`/`set_raw`. Capacity follows
  CPython: an append within 1.125× grows to `n + n/8 + (3 or 6)`, a larger
  jump to exactly `n`; a length under a quarter of a capacity above 64
  bytes reallocates to fit, so `clear()` and `del` give memory back.
- **Snapshots.** Nothing new: a bytearray is an ordinary object plus a
  byte block, written and validated with the segments. The spare capacity
  is saved with it. A length past the block (only a corrupt image) reads
  as the whole block, never out of bounds.
- **Memory limit.** The capacity block is arena memory, so the limit
  counts it exactly; a growth that does not fit raises `MemoryError`
  before the length changes. The old block is garbage until the next
  collection; a block of 64 KiB or more requests one once the bytes
  allocated since the last collection reach half the memory left, as
  `bytes`, `str` and int results do.
  Self-referential edits (`b += b`, `b[1:1] = b`) copy through an admitted
  scratch buffer.
- **Sharing with `bytes`.** `bytes_data` reads both types, and every
  bytes-like argument (`bytes_like`, `need_bytes`, needles, `join` items,
  `in`, `+`, comparisons) accepts both. The read-only methods are bytes'
  table entries: a method lookup on `bytearray` that misses its own run
  continues in bytes' run, and those methods build their results "like"
  the receiver (`bytes_slice`/`build_kind` take the kind from it), so
  `bytearray(b'a,b').split(b',')` is a list of bytearrays without a second
  implementation. `bytearray`'s own run holds only the mutators.
- **In scope.** Constructor forms `bytearray()`, `(int)`, `(bytes-like)`,
  `(iterable of ints)`, `(str, encoding[, errors])` and `fromhex`; item,
  slice and extended-slice assignment and deletion; `append`, `extend`,
  `insert`, `pop`, `remove`, `clear`, `reverse`, `copy`; `+`, `*`, `+=`
  and `*=` in place; `in`, `len`, iteration (`bytearray_iterator`, which
  sees mutation), `reversed`, `repr`/`str`; `==` and ordering with `bytes`
  and `bytearray`; `hash()` raises `TypeError`; slices are bytearrays;
  bytes' methods return bytearrays; `bytes(ba)`, `int`, `float`,
  `int.from_bytes`, `str(ba, encoding)`, `decode`, `struct.unpack`,
  `unpack_from`, `iter_unpack` and the new `struct.pack_into` (Python
  over slice assignment, with CPython's checks and messages).
- **Left out.** `memoryview` (views need exported-buffer pinning that
  blocks resizing: not trivial), subclasses of `bytearray`, `__alloc__`,
  `bytearray % args`, `bytes.translate`/`maketrans` (absent for `bytes`
  too), bytearray host values (the host boundary carries `bytes`), and
  bytearray arguments to modules that check for `bytes` (`hashlib`, `re`,
  `json`; `binascii` and `base64` take them as bytes). The shared methods keep `bytes` in
  descriptor names and argument errors (`bytes.find()`).

## 8. VM

### 8.1 Bytecode

- **Shape: accumulator-register (V8 Ignition style), decided by spike S1**
  (`docs/spikes/dispatch.md`). One implicit accumulator plus frame registers:
  `Ldar r`, `Star r`, `LdaSmi imm`, `Add r` (acc = r + acc), `TestLessThan r` +
  `JumpIfFalse`, `LdaGlobal`, `GetNamedProperty`, `CallProperty`, `Return`.
  Locals and expression temporaries are frame registers; the compiler allocates
  temporaries stack-like per expression. Instructions are one or two 32-bit
  words: opcode in bits 0..8, then `r`/`b` 8-bit, `u`/`i` 16-bit and `j`
  24-bit (word pc) operands (`mm_compile::bytecode::layout`). A call window is
  the top of the caller's temporaries: callee in `R[r]`, `R[r+1]` reserved,
  arguments from `R[r+2]` (`R[r+3]` after `LOAD_METHOD`, `self` in
  `R[r+2]`), so arguments are passed in place, not copied.
- **Dispatch: one `match` loop** with pc, frame base and the accumulator in
  locals; handlers `#[inline(always)]`. Rust has no computed goto and `become`
  is unstable (rustc 1.99.0 rejects it, `E0658` [M]).
- Evidence (S1, median ms, `match` dispatch, shared substrate, results checked
  against CPython 3.12) [M]:

| Kernel | stack | accumulator | register3 | CPython 3.12 | qjs |
|---|---|---|---|---|---|
| fib(27) | 18.6 | 17.9 | 18.1 | 28.1 | 19.1 |
| loop 3M | 39.5 | 31.1 | 31.4 | 100.1 | 63.9 |
| closure 1M | 31.0 | 25.5 | 33.7 | 55.3 | 33.7 |
| attr 1M | 32.1 | 27.1 | 29.6 | 56.0 | 35.2 |
| method 1M | 34.9 | 32.0 | 36.4 | 55.1 | 63.4 |
| list 1M | 45.9 | 40.2 | 42.9 | 78.1 | 72.8 |
| dict 200k | 24.3 | 17.8 | 19.9 | 23.8 | 56.5 |

  - Accumulator was fastest on all seven kernels, 4–27% (about 15% on
    average) faster than stack; its bytecode is 19–43% larger and its dispatch
    function 3% larger (6,291 vs 6,123 bytes).
  - A function-pointer handler table was 35–65% slower than `match` in every
    design and kernel.
  - Bounds checks cost 6–29%; they stay (§5).
  - The pre-set rule (register/accumulator only if ≥ 20% faster on call and
    loop kernels) was not met: loop −21%, closure −18%, method −8%, fib −4%.
    The owner chose accumulator anyway: it won everywhere, and the costs the
    threshold guarded against (code size, compiler complexity) measured small.
  - Caveat: the spike has no GC, bigint or exceptions and about 22 opcodes per
    design; absolute numbers are optimistic, the comparison between designs is
    what it measures. Published evidence agrees in direction [S]: a register
    JVM executed about 47% fewer VM instructions with about 25% larger
    bytecode (Shi et al., VEE 2005 / TACO 2008).
- A side exception table (pc range → handler, depth), as CPython 3.11+: no
  runtime setup cost.
- Fused and specialized forms, added only when a benchmark shows a win:
  compare-and-jump, `GetMethod` + `CallProperty`, int-specialized `Add`/`Sub`
  that deopt to the generic op, small-immediate forms (`AddSmi`).
- **Inline caches live outside the arena**, in per-code side tables, and start
  empty after a restore. They are never snapshotted and never dirty code pages.

| Cache | Guard / content |
|---|---|
| `LOAD_GLOBAL` / `STORE_GLOBAL` | entries-table version → entry index (a deleted entry is a miss, so `del` and re-adding a global keep other caches); a builtin also checks a globals-added counter |
| `LOAD_ATTR` / `STORE_ATTR` | shape id → slot |
| `LOAD_METHOD` | class version → function |
| `CALL` | callee kind |
| `BINARY_OP` | type pair |
| `CALL` on a class (`Ic::New`) | class version → `__init__` function and code; filled only when `type.__call__` would just allocate and run a plain Python `__init__` taking the call's arguments (metaclass `type`, no `__new__`, no builtin or exception base, simple parameters). Class versions come from one VM-wide counter, so the version names the class; rebinding `__init__` or `__new__` in the class or a base bumps it |

### 8.2 Calls never recurse on the Rust stack

- Python → Python calls push a frame and continue in the same loop.
- Dunders invoked by VM ops (`__add__`, `__getitem__`, `__eq__`, `__iter__`,
  `__init__`) push a frame with a continuation tag; the finishing op runs when
  it returns.
- Builtins that call back into Python follow §5's fast path / prelude rule.
- Result: the Rust stack never holds two dispatch loops, so every safepoint can
  park, and GC never runs while Rust code holds a `Value`.

### 8.3 Frames

- Frames live in the VM's value stack (`Vec<Value>`), traced as GC roots and
  written with the VM state in snapshots. A called frame's two-slot header
  overlays its call window's `R[r]` and `R[r+1]`: the caller's function and a
  link small int (`ret_pc:23 | caller code index:16 | base offset:9`). Frames
  whose caller record does not fit (continuations, generator and coroutine
  callers) store offset 0 and keep a full record on a side stack. Restore
  validates the header chain. Code objects sit in one flat table indexed by
  the link's code index. The register count per code object is fixed by the
  compiler.
- Generators and coroutines own a copy of their frame. Resuming copies it
  onto the stack, and yielding back, with one validated block access each
  way; the copy is a few instructions per register, so a per-yield liveness
  set would save little and is not emitted.
- The VM keeps `pc`, the frame base and the accumulator in locals during
  dispatch and writes them back at safepoints, so a park is "stop the loop".
- GC roots are every register of every live frame; the compiler clears dead
  temporaries at statement ends so they don't keep objects alive.

### 8.4 Exceptions

Raise unwinds through the side table, then frames. `finally` and `with` compile
to handlers. Error text matches CPython, or Monty where Monty deviated
deliberately.

Errors are written for a model to act on, because each misread error costs a
model turn:

- **Positions:** a compressed position table per code object records line and
  column ranges per instruction, as CPython 3.11+ does, so tracebacks show the
  source line with `^~~~` carets under the failing sub-expression (which
  access in `x['a'].b.c` was `None`). Cost: a few bytes per instruction [E].
- **Format:** CPython 3.14's traceback layout, so it matches what models saw in
  training.
- **Hints:** CPython's "Did you mean ...?" suggestions for `NameError`,
  `AttributeError`, `ImportError` names and unexpected keyword arguments.
- **Host functions** declare signatures, so a bad call fails with a CPython-style
  `TypeError` before any host round trip.

### 8.5 Async

Coroutines are frames. The scheduler is a run queue in the VM: ready tasks
plus tasks waiting on host calls. `asyncio.gather` surfaces every host call that
is pending at the same moment as one `Waiting` batch. `asyncio.sleep` is a host
call when the embedder asks for real time; by default it runs on a
deterministic virtual clock that never waits. There is no wall clock in the
engine.

`await` on an object whose class defines `__await__` (or on a
`types.coroutine` generator) wraps the iterator it returns in a delegation
the `AWAIT` steps through the prelude (`_await_step`/`_await_throw`, a
`Cont::AwaitStep` continuation, snapshot tag 31), as `yield from` would.
A yielded value under `coroutine.send()` suspends every coroutine frame
from the `AWAIT` up to the one `send()` runs: each saves its registers,
its `AWAIT` pc and what it waits on in its coroutine object (as a
generator does at `YIELD`), and `send()` returns the value; the next
`send()`/`throw()` re-runs the saved `AWAIT`s from the outermost. In a
task, a yielded `None` yields to the scheduler (the task is queued again)
and any other value raises `RuntimeError`. No Rust frame holds a
suspended coroutine and no second dispatch loop runs.

`asyncio.create_task` spawns a scheduler task and returns its `Task`
handle, which runs when the creating task next yields, as on CPython.
Cancellation is CPython 3.14's `Task.cancel()`: the request goes to what
the task waits on (a task, a gather's children, a future, a sleep, a host
future) through one iterative walk (`Vm::cancel_tree`, no recursion and no
nested dispatch loop); when nothing there takes it, the task keeps
`_must_cancel` and the scheduler raises `CancelledError(msg)` at its pending
`AWAIT` (the same raise-at-pc path a deadlock uses), or throws it into the
`__await__` iterator it steps, so `except` and `finally` run in the task.
`asyncio.run` is prelude Python around the main coroutine: the loop counts
iterations as CPython's `_run_once` does, and when the coroutine ends the
remaining tasks are cancelled and run until done. All of it is engine
state, so a cell parks and snapshots in the middle of a cancellation.
`Semaphore`,
`wait_for` (CPython 3.12+'s: the awaitable runs in the calling task and a
timer task cancels it) and `as_completed` are prelude Python over a
loop-settled `Future` and a first-done gather. Deadlines are virtual-time
sleeps that wait for pending host calls first: host calls take no virtual
time, so `wait_for(host_call(), t)` never times out on the engine's clock;
real timeouts belong to the host (`HostError` `TimeoutError`).

The other primitives (`Lock`, `Event`, `Condition`, the queues,
`TaskGroup`, `timeout`, `shield`, `wait`, the loop's `call_soon`/
`call_later`/`call_at`) are ports of CPython 3.14's Python sources in the
prelude, over three engine pieces. A pending task, future or gather keeps
an ordered list of what waits on it (a gather, a task's wakeup, or a done
callback), woken in that order when it settles, which gives CPython's FIFO
release and callback order. A callback (`add_done_callback`,
`call_soon`, a timer's) runs in a task slot of its own, with no `Task`
handle, through the prelude's `_aio_callback` from the bottom of the slot's
stack, so a callback that raises goes to the loop's exception handler and a
cell parks inside one like anywhere else. `call_later`/`call_at` timers are
scheduler-internal tasks sleeping on the virtual clock, left out of
`all_tasks()`. Task numbers, names, callback slots and the loop flags are
VM state in the snapshot.

The running `contextvars` context is a dict in `Vm::context`; each task
keeps its own (`Task.context`, a copy of its creator's at spawn, or the
one `create_task(context=)` gives), swapped in and out when the task is
activated and blocked, and snapshotted with it. `Runner.run()` and
`loop.run_until_complete()` are each one `asyncio.run` of the same
dispatch loop (`_mm.aio_run(main, root, open)`; the `RUN_OPEN` coroutine
bit skips the two numbered shutdown tasks of `Runner.close()`), so no
nested loop runs. A `KeyboardInterrupt` or `SystemExit` that ends a task
other than the main one is kept in `Vm::escape`, the main task is
cancelled, and `_aio_main` raises it after the shutdown; failed tasks and
futures nobody retrieved (`Vm::unretrieved`) are reported when
`asyncio.run` ends, not when they are collected.

### 8.6 Safepoints

Backward jumps and calls count one tick each; every `TICKS` (256) ticks,
or every tick once less fuel than that is left, they check one flag word:
fuel exhausted, deadline poll due, GC requested, park requested (yield
window, `Cancel`, host preempt), host results pending. The common path is
one branch, so a park request or deadline is seen within 256 safepoints
and `Vm::fuel` lags the work by up to 255 ticks between checks (exact at a
park). Returns charge no tick and check nothing: the returns run between
two safepoints are at most the frames on the stack (ten times the
recursion limit), each a few instructions.

Straight-line code has none of those, and a cell may hold some 20,000
statements, each a bounded but long native operation (`b = s == t` over
10 MB strings ran 75 s past a 50 ms deadline, and 2,000 of them 7.5 s
with 100 units of fuel [M], finding C4). So the compiler puts a `CHECK`
instruction where a path has run `CHECK_EVERY` (32) instructions that may
work in proportion to their data since its last safepoint; moves,
constants, name loads and stores, identity tests and branches do not
count, and a call restarts the count (it ticks once it returns). Counts
are per path: a forward jump carries its count to the label, which takes
the most of what reaches it; a loop head restarts it (its back edge
ticks), so a path runs fewer than 64 counted instructions between two
checks; a handler starts with the most its block reached. `CHECK` charges
nothing: natives that charged more than 64 units since the last check,
in one charge or many small ones (`Vm::burn` counts them, `burned`), or
spent the fuel, left the tick counter at most 1, and only then does
`CHECK` run the safepoint. Counting only a single charge over 64 units
left a long body of small ones (900 slices of a 3,840-char str, 59 units
each) checked once per 256 back edges: 117–129 ms past a deadline, and
256 × 300 slices of garbage between collection checks raised
`MemoryError` at 256 MiB where CPython runs it [M, 99364cb]. Fuel totals
do not change: a check charges only the safepoints that ran. `burned` is
not in the image: every park runs a check, which clears it, and a restore
at a host call starts it at 0, which can delay that one check by at most
64 units but changes nothing a run charges or prints. It is a rare opcode, so the dispatch loop is
unchanged; loop bodies of the bench programs hold none. A park lands
between two instructions with `acc` and the registers in the frame, as at
any safepoint, so the snapshot format does not change.

A `CHECK` every 32 such instructions still let up to 63 long ones run
between two checks (64 `s == t` over 60 MB strings: 671 ms [M]). So an
operator that would start after a native charged more than 64 units at
once since the last safepoint (`long_work`, set by `Vm::burn`) stops
before it starts (`Vm::stop_before`): a binary or comparison operator
(`in` included), unary `-`/`~` on a long int, and an f-string field's
conversion of a value that is not a str. It returns `Err(Raised)` with
`check_before` set; `Vm::execute` runs the safepoint (collection, park
request, deadline, fuel) and dispatches the same instruction again, or
parks before it, as an int operator short of fuel does (that check comes
first, so its park still reports `fuel_wanted`). Its state is
unchanged until then, so a park or snapshot there needs nothing new. The
hooks sit in out-of-line entry functions that only the dispatch loop
calls, so a stop never cuts a native in half, and the loop's own code is
unchanged (a hook in the inlined `op_unary` cost `loop` 10% Ir). Between
two safepoints straight-line code then runs at most one long operator,
plus instructions that copy into a new object (slices, displays, f-string
joins), which the memory limit bounds. Garbage is still collected only at
safepoints, which these stops now provide between long statements.

## 9. Durability, GC and snapshots

### 9.1 Pause-anywhere (invariant)

| Level | Capture points | Who |
|---|---|---|
| L1 | Waiting on the host (call, await, name lookup) | Monty |
| L2 | Any safepoint, any depth | micro-monty (all frames); dquickts (plain sync frames only) |

dquickts suspends only when every frame is a plain synchronous JS function called
by the host (`js_can_suspend_from` in `quickjs.c` [S]). Generator and async
frames, promise jobs (any code after an `await`), getters, `toString`, Proxy
traps, `forEach`/`JSON.stringify` callbacks and `eval` defer the request,
because QuickJS recurses on the C stack. micro-monty can park in all of those,
because of §8.2.

What pause-anywhere buys a code-mode harness: engine-parked yields (the cell
protocol's `Yielded`/`Resume{window}`), cancelling a cell without losing the
worker, and the compute bound as a clean preempt rather than a worker kill.

What no engine can make durable, so the host owns it: results of in-flight host
effects, host handles after a restore, output printed since the last snapshot,
timer deadlines.

### 9.2 GC

- Precise, non-moving mark-sweep at safepoints only, with the side mark bitmap.
- Roots: VM registers, the frame stack, the scheduler, the module table, pinned
  host values. Trace owners directly; do not allocate temporary copies of root
  lists or suspended tasks' stack/frame slices.
- No rooting API: natives never observe a collection, because allocation inside
  a native only sets "GC requested". A native allocating far past the threshold
  relies on the memory limit.
- **Collect and retry.** Garbage not yet collected counts against the memory
  limit, so a native can fail where a collection would have made room (a loop
  of 65 MB joins raised `MemoryError` on its third pass at 256 MiB [M]). A
  method of an immutable builtin type (`str`, `bytes`, `int`, `float`,
  `tuple`, `frozenset`) called straight from a `CALL` that raises
  `MemoryError` changed nothing and consumed no iterator, so the `CALL` runs
  again once a collection that started after the failure has ended
  (`Vm::oom_retry`). It waits as a held mark does (`Vm::gc_held`): a step
  per safepoint check with no guest code between, incremental over 32 MiB
  live, a park check after each; the `CALL` runs again once after the
  steps, also after a snapshot and restore (the rerun mark is in the
  image). It retries
  once per failure, and only after at least 1 MiB was allocated since the last
  collection, so a result that cannot fit raises after one collection. That
  count is the engine's own (`Heap::allocated_since_own_gc`): a snapshot's
  collection keeps it (`Heap::keep_allocated_since_gc`; it is in the image),
  except one taken during the wait, which is that collection,
  as does a collection between cells (`Vm::collect_now`: a compile that did
  not fit, code reclamation),
  so whether an operation runs again does not depend on whether, or where,
  the host took snapshots, while the collection trigger counts from the
  snapshot's collection, which paid for what came before (counting it again
  made a yielding session, `set_gc_yield`, collect and park at the first
  safepoint after every restore). Its
  fuel is charged again. An instruction whose native raises `MemoryError`
  straight back to the dispatch loop retries the same way
  (`Vm::oom_retry_op`): a binary operator (`Vm::op_binop`) unless it may
  have changed its left operand first (an in-place operator on a dict or
  set, or on an instance of a subclass of one, which changes its backing
  object; a list's `+=` and `*=` build or grow its buffer first, and a
  bytearray grows into a new block, before they change it), an
  f-string's `BUILD_STRING` (its pieces are str already), a starred
  display's `LIST_TO_TUPLE`, `UNPACK_EX` from a list or tuple (its
  registers are written once its list exists), `LIST_EXTEND` from a list
  or tuple (its list grows once, to its final length, before any item is
  written), `CALL_EX` of a Python function (its `*args` tuple and bound
  arguments are built before its frame is entered; a call of anything
  else is not retried), and `SET_ITEM` (stores are idempotent, and a list
  slice builds its new buffer before installing it). User code never runs
  inside those natives (a dunder runs as a request), so it never runs
  twice. Nothing that drew items from a one-shot iterator (an iterator
  that is its own iterator) runs again: the rerun would gather only what
  is left (`l[:] = zip(a, b)` once emptied `l`), so `Vm::collect` marks
  the failure as a rerun's (`oom_rerun`) as it starts on one. That mark,
  the rerun's own, and the items a builtin consumed (`consumed`) belong to
  the instruction that set them (`Vm::op_stamp`: the ticks' reset count and
  the safepoint ticks run since, which natives' charges leave alone; a
  check between ticks carries them), so they end at the next tick wherever
  a host's fuel slices end, and whether a later instruction runs again does
  not depend on the slices (it once did: only a slice ending between them
  dropped the marks). An instruction right after one that set a mark, with
  no tick between, is not retried. The hooks
  sit outside the dispatch loop's arms (`Vm::op_binop`, `Vm::op_setitem`,
  and the run loop's fault path, `Vm::oom_retry_fault`), so its code is
  unchanged. These raised `MemoryError` beside 60 MB live where CPython
  runs them, once `import time` had moved the collections: `ba += x`,
  `bytes +=`, an f-string and a list slice store, each building about
  60 MB [M, 99364cb]; `(*l, *l)` and `a, *m = l` over 2M items, run
  40 times [M, 8af0a5e]; `(*l, *l)` over 2.5M items, 40 times, and
  `f(*t, *c)` growing `t` to 7M items [M, 4b32aa2 with star-shapes].
- Trigger: bytes allocated since the last collection exceed twice the live
  bytes after it (`gc_ratio_percent` = 200, live floored at 1 MiB), or memory
  pressure: at least 1/64 of the limit allocated since then and either under
  1/8 of the limit left uncommitted with at least half of that allocated, or
  a space that cannot commit its next segment with under 1/8 of the limit
  free (`Heap::should_collect`); plus the 64 MiB and room rules below.
- No finalizers, no weakrefs.
- **Large blocks that grow.** A byte block written over several steps
  (`json.dumps`, §5) that grows by moving to a new block holds both blocks
  until the move ends; the old one is then garbage, and a collection is
  requested by the result rule (`Vm::collect_for_result`) so it is not
  counted against the next growth. When the limit cannot admit the new
  block's spare room, the move falls back to growing the block in place,
  which the allocator may do by copying it in one step and so holding
  both buffers for a moment: that growth is admitted for both too
  (`Heap::append_bytes`). A growing text therefore reaches about half the
  memory left (a `dumps` text of 0.48 × the limit with nothing else live
  [M]), and past that is a `MemoryError`, not a copy outside the limit.
- **Incremental over 32 MiB live** (HANDOFF scope decision 65, GC; owner,
  2026-11-09). A full collection pauses about 0.4 ms per live MiB of small
  containers natively and 0.55 ms on the mm-wasm guest [M], past the
  50 ms step bound (§5) from about 125 MiB. From `gc::MIN_LIVE` (32 MiB
  live after the last collection) a due collection marks a slice of 2^18
  work units per safepoint check (§8.6: every 256 safepoints, sooner after a long native step) instead (`Heap::collect_step`), and a park,
  deadline or snapshot can follow any slice. Below it a collection runs
  in full and is also due once live and allocated bytes reach 64 MiB, so
  a heap growing through the threshold is marked in full at most at about
  64 MiB: 30 ms native at worst [M] (with 48 MiB and the ratio trigger
  alone, 44–51 ms). There is no write barrier beyond the setters' segment
  flags: a value-slot write sets
  `DIRTY_REFS` on its segment, the marked objects of such segments are
  rescanned in later slices until a pass finds few, and a remark of the
  roots and the remaining dirty segments ends the mark in one step
  (mostly-parallel marking). Objects allocated during a mark are unmarked;
  what reaches them is a root or a written slot, so the remark finds them.
  The sweep that follows is lazy (`Space::pending`): allocation sweeps a
  few segments before committing memory, each check sweeps 8 segments per space, and
  everything that walks the heap (`write_snapshot`, the code-liveness scan)
  finishes it first. Mark state lives outside the arena and is never
  snapshotted: a snapshot, code reclamation and a native's collection
  request abandon a mark for a full collection, and a restored
  heap starts with none. The snapshot format does not change.
- **Headroom.** Nothing is freed until the remark, and natives cannot
  collect, so a native that allocates more than the room left during a
  mark raises `MemoryError` where a full collection at the mark's start
  would have made room. Over `MIN_LIVE` a collection is therefore also due
  once the room left (uncommitted budget plus free blocks) is no more than
  the bytes allocated since the last one, and a due collection whose room
  is under half of those (a native allocated them at once) is held (below).
  Without that, plain programs near the default limit failed that pass
  with full collections (`d = {i: i for i in range(1_500_000)}`,
  `dict(d)`, `list(d.items())` at 256 MiB) [M].
- **Pacing.** Natives allocate a lot in few steps (P4's stepped set and
  dict builders: tables of tens of MiB a step), so a slice a safepoint
  falls behind: building two 2M-item sets ended with a mark holding about
  190 MiB of garbage, and the next `a & b` raised `MemoryError`. A step
  therefore marks a slice plus a work unit per word allocated since the
  last step, and a mark is held (below) from the step where the room left
  is no more than the bytes allocated since it began [M: `c = a & b`
  12.6 ms, `dict(zip())` of 2M pairs 28.5 ms, `a |= b` of 1.25M + 1.25M
  45.5 ms].
- **Held marks.** A mark the guest cannot go on allocating beside (it
  starts with less room than half the bytes allocated since the last
  collection, or reaches the pacing point above) and one that does not
  converge (below) is held (`Heap::holding`): it goes on a slice per
  safepoint check with no guest code between slices (`Vm::safepoint`
  loops), so nothing is written or allocated meanwhile, and a park,
  deadline or cancel can still follow any slice; a park leaves the next
  slice owed, run first by `Vm::resume`. A held mark ends after a rescan
  pass that finds nothing dirty and never gives up. These collections
  ran in full in one step before; worst gaps, f305b88 → now, native [M]:
  `list(zip(xs, xs))` of 4M built by a comprehension over 200k old lists
  44–54 → 24–25 ms (a collection due with 9 MiB of room at 195 MiB
  live), 4M over 600k old lists 50–60 → 23–24 ms, `k = d.items() - e`
  at 224 MiB 41–52 → 25–32 ms, `copy.deepcopy` of 600k lists over 1M
  old ones 105–113 → 25–26 ms, scattered writes into 199 MiB 264–280 →
  8–9 ms.
- **Park first.** A safepoint runs a due collection step before its park
  checks, so a gap could hold a native's step and the collection step
  it made due: `ys = xs + xs` of 4M items (a 64 MiB result making a full
  collection of `xs` due) took 46–76 ms from 68a955d [M]. Now, when a
  park is due at a safepoint where a collection step is due, the park
  goes first and the step is owed: `Vm::resume` runs it before any code,
  with a park check after it. A step is owed once, never twice, so with
  fuel slices (every check a fuel park) collections still run; stress
  testing collects in place as before. Worst gaps, ca3db9e → now [M]:
  `ys = xs + xs` 39.5 → 24.4 ms, `list(zip(xs, xs))` of 3M over 1M old
  lists 45 → 38 ms, `dict(zip(ss, xs))` of 1M str keys 52 → 45 ms,
  `a |= b` of 1.25M + 1.25M 43.7 → 27.8 ms, and on d455c5e `u = t * 2`
  of a 4M tuple 44–46 → 28–30 ms; with the deadline 3 or
  10 ms ahead instead of at every safepoint, the overshoot past it goes
  35.6 → 22.1 ms, 45.6 → 38.3 ms and 42.3 → 24.8 ms on the same rows.
  Allocating black (objects allocated during a mark count as marked)
  was measured too and made worst gaps longer (3M zip: 53–74 ms): it
  scans the garbage made during a mark, and pacing it puts mark steps
  of 46–75 ms right after a native's large chunk [M].
- **Fuel and collections.** Collection work charges no fuel: when a
  collection falls due depends on byte sizes (32- or 64-bit) and on
  restores, and fuel totals must not. A host that regains control only
  between fuel slices (deployed Workers, where deadlines never fire)
  would then wait for every collection step in a slice: 22–52 ms in
  10,000-unit slices at 59–199 MiB live [M]. With `Session::set_gc_yield`
  (off by default; `drive_async` and hosts/js's sliced driver turn it
  on, the latter through the optional `mm_set_gc_yield` export) a
  safepoint that ran a collection step ends the slice with a `Fuel` park
  and fuel left, so a slice holds at most one collection step. Fuel
  charged and results are the same with it on or off and on native or
  wasm32: 3,900,004 units on all four for 2M live lists with garbage and
  scattered writes, worst slice 42–46 → 14–17 ms native and 57–58 →
  18–19 ms on wasmtime [M].
- **What it bounds** [M], the longest gap from a park request to the park,
  native / wasm guest, at 199 MiB and 778 MiB live (1 GiB limit): garbage
  loops 2.4 / 3.2 ms and 8.2 / 7.7 ms; in-order writes into old objects
  2.7 ms and 4.4 ms native; appends 5.5 ms native at 778 MiB. Full
  collections there take 61–101 ms and 269–338 ms native, 131 ms and
  477–518 ms on wasm.
- **Scattered writes into a large old heap.** Writes into old objects
  all over the heap dirty nearly every segment between passes, the
  rescans do not converge, and the remark approaches a full mark: 155 ms
  at 199 MiB and 841 ms at 778 MiB native, 194 ms at 199 MiB on wasm, at
  2.3× the wall time of full collections [M]. So a mark whose rescans
  pass a quarter of its first mark, or whose rescan pass does not halve
  the dirty segments, gives up: it is held from then on, and so are the
  next 4 due collections from their start (doubling up to 16 while marks
  keep failing). Such a loop costs about what full collections cost
  (they ran in full until f305b88: +5–12% wall time native and ±0–21% on
  wasm instead of 2.3× and 1.9–2×), in slices: 3M writes into 199 MiB
  take 11.9 s with held marks and 12.1 s with full collections, worst gap
  8–9 ms against 264–280 ms [M]. Allocation, appends and in-order writes
  rescan at most 2% of their first mark and never give up [M].
  `d = dict(zip(ss, xs))` of 1M str keys over 600k old lists finished
  its mark in one step and gave up there for a full collection, 36–49 ms
  in that step (7df5282); its worst gap is the native's own [M].
- **New live data built during a mark.** Objects allocated during a mark
  are unmarked and traced by later passes or the remark, so a native that
  builds tens of MiB of objects that stay live pays for tracing them in
  the remark, a step of its own (park first): `list(zip(xs, xs))` of 4M
  over 200k old lists 34 ms, `dict(zip(ss, xs))` of 1M str keys over
  600k old lists 43 ms [M] (54 and 49 ms on ca3db9e, where the remark
  shared a gap with the native's last chunk). Near the memory limit the
  mark runs out of room and is held (above): the same zip with no old
  lists took a 77 ms step at 184 MiB live (96 ms on ca3db9e) and 35–38 ms
  on f305b88, 22–24 ms now; with 600k old lists 50–60 → 23–24 ms [M].
- **Snapshots at parks.** Mark state is not snapshotted, so a snapshot
  taken at any park, owed collection step or not, first collects in full
  and then writes the image: on the rows above at 192 MiB, 153–164 ms
  for `a |= b` and 248–250 ms for the 3M zip, worst park of each [M].
  That is the host's call, outside the step bound.
- **Also unbounded:** every collection under 32 MiB live (30 ms native
  at worst, above) runs in full, as do a snapshot's, code reclamation's,
  and one a native or the engine requests (`Vm::request_gc`): after a
  `MemoryError` is raised, after a result of 64 KiB or more once
  allocation since the last collection reaches half the memory left, and
  after a byte job fails for memory.
- **`gc.collect()`** asks for a collection that starts after the call
  (`Heap::force_collection`) and waits for it as a held mark does
  (`Vm::gc_held`): a step per safepoint check, no guest code and no fuel
  between steps, a park check after each, so over 32 MiB live it can park
  between slices: 4M live lists went from one 70 ms step to steps of at
  most 1.4–1.8 ms [M].

### 9.3 Snapshots

- **FULL first:** after a GC, the segments of both arena spaces plus VM
  registers. Free runs are written as zero-length gaps, so a fragmented heap
  doesn't inflate snapshots. Write and restore are memcpy plus header checks.
- **Restore cost** [M, eb78933, one 16-core Amp orb]: about a snapshot's per
  MiB of image (§9.2, docs/sdk.md). Small objects take 1.2–1.5 ms/MiB
  native, 1.0–1.8 under wasmtime and 0.9–1.7 on V8; large blocks take
  0.3–0.9. A process's first restore takes up to 1.8 ms/MiB, and V8's first
  call into a new instance adds a warm-up (154 ms at 35 MiB). A large block's
  recorded room is reserved, not filled, and costs nothing measurable.
  `bench/evidence/restore-cost/run.sh` reruns these on any commit.
- **Every mutation goes through mm-core setters (the write barrier) from day
  one,** so dirty tracking can be added without touching callers.
- **Deltas are an efficiency feature, not a semantic one, and stay out of the
  engine** (owner decision 2026-11-05, HANDOFF scope decision 33). Hosts make
  images small at the storage boundary (below).

```text
header  { magic "MMSNAP\0\1", format_ver u16,
          build_id [u8;32],     // layouts, codecs, tables, bytecode,
                                // features, layout epoch ("Compatibility")
          vm_regs { frame stack top, scheduler, pending host calls, fuel used, limits },
          segment_count }
segments [{ space: word|byte, index u32, runs: [(offset, len, data)] }]  // free runs omitted
trailer  { xxh64 of all above }
```

- Same build id only: a `build_id` or format mismatch is a typed
  `SnapshotError::Incompatible { format, build_id }`, naming the image's.
  The id hashes what an image's meaning rests on, not the source, so builds
  that differ only in function bodies share it (below, "Compatibility").
- **Upgrades drain; nothing migrates** (owner decision U1, HANDOFF scope
  decision 38). The engine adds no cross-build migration. A host that
  upgrades keeps the old build, keyed by build id, until that build's parked
  snapshots have drained, and treats a cell no kept build can resume like a
  crash (an unknown outcome the host reconciles). The id hashes no
  target-specific input, so native and wasm builds of one source and feature
  set share it, and a native host can drain old images on that build's wasm
  guest. `mm-wasm-host`'s `Runtime` holds several guests keyed by build id
  and restores each image on its own (docs/sdk.md, "Upgrading: drain old
  builds").
- Snapshots are trusted input: the host integrity-protects them and never loads
  one produced by the guest or the user. The reader still bounds-checks
  everything and is fuzzed.
- **Persistence: FULL images, compressed by the host.** The engine writes
  FULL, uncompressed images. Hosts compress with zstd level 1, using the
  build's fresh-session image (`Session::new` then `snapshot`) as a raw prefix
  (`--patch-from`), with the frame checksum on, and key each blob by build id;
  a dictionary trained on the host's own parks is optional (docs/sdk.md,
  "Storing snapshots"). Measured [M]: plain zstd-1 gives 3.6× on small parked
  sessions and about 19× on 100k retained rows; the base prefix adds 1.3–1.4×
  on small parks (codemode median 102.8 KB → 19.7 KB), and base plus a
  trained dictionary reaches 8.1× against FULL on held-out codemode parks.
- Two engine-side delta designs were built, measured and not adopted
  (`bench/evidence/results/`). Write-time dirty-line tracking cost 4–19%
  execution time on 11 of 13 bench programs, and a per-session switch kept
  20–73% of that cost when off. Snapshot-time comparison against the
  fresh-session image costs nothing at run time, but the base is only 24–45%
  of a small park, so it gains no more than the zstd prefix, and it would add
  about 22.5 KB to the mm-wasm guest
  (docs/research/base-relative-snapshots.md).

Targets [E]: 3–4 MB per 100k small objects before host dedup; write and restore
in ≤ 5 ms. For comparison [M v1]: Monty 10.63 MB / 17 ms / 89 ms; dquickts
3.06 MB / 41 ms / 28 ms.

**As built (M8).** `Session::snapshot()` runs a GC and writes; `Engine::restore`
reads into a new session (same or another process).

```text
header   magic "MMSNAP\0\x01", format u16 = 1 (SNAPSHOT_FORMAT), build_id [u8;32],
         returns_last_expr, want_value, the stop (phase byte: 0 idle,
         1 host call, 2 waiting, 3 parked; then the stop as reported),
         text_limit, fuel total
         (0, or 1 and a u64)
heap     memory limit, GC ratio and floor, GC statistics, intern table;
         per space: segment-table size, bump segment, vacant indices,
         each segment's capacity (a dedicated segment's room past its
         object as a number) and live runs (free space omitted)
vm       registers, fuel left, limits, seq / virtual clock, frames,
         continuations, scheduler, tasks and run queue, globals, modules,
         pending host calls and their ids, park reason,
         host modules (definition; for code the Engine shares, its 16-byte
         key, code hash and recipe), before the
         code table (Code::encode_into; bundled-module code and the shared
         code of host modules' current definitions referenced, not stored;
         loaded prelude units stored like user code)
trailer  xxh64 of everything above
```

- **Compatibility: the build id is a layout descriptor** (owner decision,
  HANDOFF scope decision 66, superseding 55). Two builds with the same id
  restore each other's images; the id hashes what an image's meaning rests
  on and nothing else, so an edit to a function body (a fix in a native,
  the interpreter, the compiler, the facade) keeps parked images
  restorable, while a layout change refuses them as `Incompatible` before
  anything is read. It hashes:
  - `SNAPSHOT_FORMAT` (micro-monty `src/image.rs`) and `SNAPSHOT_EPOCH` (mm-engine `snapshot.rs`);
  - the code (comments and blank space aside) of the snapshot codecs, whole
    files: the facade's header (`micro-monty/src/image.rs`), the engine's
    sections, mode flags and `Cont` encoders and decoders
    (`mm-engine/src/snapshot.rs`), the heap (`mm-core/src/snapshot.rs`) and
    compiled code (`mm-compile/src/encode.rs`);
  - every `const` and `#[repr]` item of mm-core, mm-compile and mm-engine
    (test modules and `const fn` aside), which declare type ids, header
    flags, the value encoding, object, frame, iterator and continuation
    slots, `Cont` stages, native-step ops, operand meanings (`BinOp`, match
    kinds) and the regex matcher's programs and backtrack frames; each
    crate's build script hashes them (`LAYOUT_HASH`), so a new layout module
    is covered without a list to keep, and a tuning constant changes the
    id too (the safe side);
  - the opcode table (number, name, operand format), the native, type and
    native-step (`nstep::STEPS`: name, value slots, raw words) tables in
    order;
  - the packed prelude units and every enabled bundled module's name and
    packed bytecode, with the section headers and shared strings it
    unpacks with, each hashed at build time (`PRELUDE_HASH`,
    `mm_stdlib::CODE_HASH`), so a build that does not link those bytes
    has the same id; and the enabled `mod-*` features;
  - samples of the value, header, string-hash, int-hash and float-hash
    encodings;
  - `CODEGEN_PROBE`, a recorded hash of what `Session::define` makes of a
    probe definition that uses every feature of the generator: the spec an
    image stores, the source generated from it and the compiled program. A
    facade unit test recomputes it and fails, printing the new value, when
    the spec codec, the generator or the compiler change it, so
    `build_id()` compiles nothing at run time (its first call takes about
    60 µs natively, against 480 µs when it hashed the linked bytecode
    [M]). A restore also checks each host program it builds again against
    the key and code hash the image stored.
- **The epoch rule.** Bump `SNAPSHOT_EPOCH` in the same commit whenever a
  function body changes what an image holds or how it is read while none
  of the hashed items above changes. In particular:
  - what a raw word or value slot of an existing NSTATE step holds, with
    the same counts: `STEPS` hashes only each op's name and counts, so a
    package that changes what word 3 of an existing op means must bump;
  - what an existing `Cont` field or stage means;
  - a layout written as literal slot or word numbers in code (`list`,
    `dict`, `set`, `range`, iterators, ints, `bytes`, `bytearray`, the
    regex matcher's saved run), or the hash, probing or order of dict and
    set tables;
  - what an opcode's operands mean, with the same format;
  - how frames, generators or coroutines keep their state.

  `crates/micro-monty/tests/golden.rs` restores images blessed by an
  earlier build (`tests/golden/`): under the id that wrote them each must
  continue exactly as the session that wrote it did, and under another it
  must be refused as `Incompatible`. A change that moves the id blesses
  them again in the same commit (`MM_BLESS=1 cargo test --release --locked
  -p micro-monty --test golden`, the build `cargo xtask check` tests them
  in), which writes nothing and fails if a continuation differs from its
  committed transcript; `check` fails while they carry another id. So
  every change that keeps the id is checked against images an earlier
  build wrote. `cargo run -p micro-monty --example build_id` prints the
  id. Measured on this rule [M]: function-body edits in a native, the VM,
  the regex matcher, the compiler (same bytecode) and the facade, and
  comment edits in the codecs, kept the id, and the golden images
  continued; a class-slot swap, a renumbered backtrack-frame tag, an epoch
  bump, a code edit in any of the four codecs, a tuning constant, a
  prelude edit that changed bytecode and a host-wrapper generator edit
  (through the probe) changed it, and the golden images were refused as
  `Incompatible`. Over the last 168 commits that
  touched the engine crates, 85 changed no bytecode source and no `const`
  item, so under this rule they would have kept the id [M].
- Restore order: magic, format version, checksum, `build_id`. Lengths are
  checked against the remaining input before any allocation; the heap is
  checked against the engine's memory limit (`TooLarge`).
- Validated on restore: segment indices, run tiling, every block header, every
  `str` (UTF-8, char count, flags), the intern table, every code object
  (`Code::verify`: operands in range, jumps on instruction starts, only
  `JUMP_LOOP` backward, sane handler ranges, a terminator last; and, for
  stored code, every `YIELD_FROM` right after its `SEND`), frame and
  generator pcs, frame bounds, code indices, tasks and host-call state, the
  intern table (identity, count, load), every dict's index (entry bits,
  duplicates, tags, probe reachability, length), continuation targets and
  registers, shape transitions, class size hints and bundled-module entries.
  A generator's code is its function's in every state; a frame that runs
  for a generator or coroutine runs that object's code while it is running
  (`Corrupt("frame owner")`). Each continuation slot is held by one frame
  at most, and the free list names each empty slot once. A dict without an
  index uses no more entries than it holds. A chunked native's state
  (`NSTATE`) keeps the words its next step trusts as an index, a stride, a
  count or a slice's bounds in range (`Corrupt("native step")`: a str
  split's search position, pieces and reversal, an extended str slice's
  step, a str map's position, a list build's next item, a list scan's
  count, a str fill's or join's parts and offsets, a replace's matches, an
  `int()` text's digits, a power-of-two digit text's block (ASCII), a
  sum's stage, a sort's list, a set or dict table a bulk operation is
  preparing, every byte job's pass and output counts,
  a `bytes` repeat's source, a bytes whitespace split's piece, a
  bytearray's strided deletion and a splice after growth). The bytes a str
  build has written are UTF-8 holding the chars it counts, and each step
  checks the seam it copies onto. A chunked native's last step can change
  its data in place without saving its words (a pending `σ` written as
  `ς`), and `_steps` holds its state until the step's fuel is charged,
  which may park; so a finished state is marked done (`nstep::op::DONE`),
  which no step runs and restore does not check. Other words are checked by the steps,
  with checked arithmetic (`SystemError`) in every build profile, and the
  bytes a build has written are not compared with its sources.
  Every reference, in the VM part and in every arena value slot, must name an
  object of its kind (`check_refs`, `Corrupt("references")`); the intern
  table's untagged string references are checked by mm-core. Free lists,
  bitmaps and size classes are rebuilt by a sweep. Every arena access is
  also bounds- and start-bitmap-checked and fails with an error.
- Not saved: what `SessionSettings` gives a new or restored session (the
  output callback, the wall clock, entropy, the zone source, the text cut,
  `int_bits_text`, GC stress), the deadline and park flag, which the
  host sets per run, and `Session::set_gc_yield`, which a host sets again
  after a restore. Inline caches start empty.
- Host-call ids survive a restore: the restored session accepts the same
  `Resume`, and `Session::pending` re-reports the stop (a host call with
  its arguments, the calls a wait is on, a park's reason) from the
  facade's header.

Measured [M, release, `taskset -c 2`, median of 21, 2026-11-01]:

| Session | Size | Write | Restore |
|---|---|---|---|
| empty | 94.6 KB | 0.06 ms | 0.20 ms |
| 100k `(i, -i)` tuples | 3.5 MB | 1.9 ms | 1.5 ms |
| 100k `str(i)` | 5.9 MB | 2.6 ms | 3.8 ms |
| 100k `{'i': i, 's': str(i)}` | 10.7 MB | 7.8 ms | 7.7 ms |
| parked in 150 nested generator + method frames | 127 KB | 0.08 ms | 0.32 ms |

Small objects meet the size target; the dict-of-strings case is at Monty's
size with 2–11× faster write and restore. Size tracks the live heap, so the
remaining lever is object layout, not the snapshot format.

## 10. Host boundary

### 10.1 Engine API (facade)

```rust
Engine::new(config: EngineConfig) -> Engine            // builds the prelude state
Engine::compile(src, filename) -> Result<Code, CompileError>
Session::start(code) -> Progress                       // runs to the next event
Session::resume(Resume) -> Progress
Session::request_park()                                // thread-safe flag
Session::reset()                                       // clear globals, keep the engine
Session::snapshot() -> Vec<u8>;  Engine::restore(bytes) -> Result<Session, SnapshotError>

enum Progress {
    Complete(Option<Completion>),                      // last expression: HostValue if convertible, else repr()
    HostCall { call: u64, name: String, args: Vec<HostValue>, kwargs: Vec<(String, HostValue)> },
    Waiting { calls: Vec<u64> },                       // all tasks blocked on host calls
    Parked { reason: ParkReason },                     // yield window, cancel, fuel, deadline
    Error(PyException),
}
enum Resume { Value(HostValue), Error(HostError), Future, Batch(Vec<(u64, Result<HostValue, HostError>)>), Continue }
```

As built (`crates/micro-monty/src/lib.rs`):

```rust
Engine::new(EngineConfig::default().with_limits(Limits::default().with_memory(m).with_fuel(f))) -> Engine
    // Limits { memory, recursion_depth, fuel } and EngineConfig { limits } are #[non_exhaustive]: Default or const new(), then with_* or fields
Engine::compile(&self, source, filename) -> Result<Code, CompileError>
Session::compile(&mut self, source, filename) -> Result<Code, CompileError>   // charged to the session (512 B per source byte + 16 KiB); MemoryError when it does not fit; inside the memory bound (§11)
Engine::restore(&self, &[u8]) -> Result<Session, SnapshotError>   // Incompatible{format, build_id} | Corrupt{what} | TooLarge | Busy | Poisoned | Internal{message}
Session::new(&Engine) -> Result<Session, PyException>             // binds the prelude's natives; units load on first use; `mm run print(0)` 0.90M instructions whole process, 0.57M gated (`bench/callgrind.py`, eb78933) [M]
Session::new_with(&Engine, SessionSettings) / Engine::restore_with(&[u8], SessionSettings) / SessionTemplate::instantiate_with(SessionSettings)
    // SessionSettings { output, wall_clock, entropy, zone_source, text_cut, int_bits_text, gc_stress }: what images do not
    // carry, given the same way to new and restored sessions (#[non_exhaustive]; the plain forms use its Default)
Session::define(&HostModule { name, functions, classes, constants, exceptions }) -> Result<Vec<HostFunctionRef { name, id }>, DefineError>   // Invalid(reason) | Busy | Raised(MemoryError) | Poisoned
    // "" = builtins; replaces the module whole; between cells; survives reset, snapshots, templates
Session::host_modules() / stubs() / host_function_id(name) / host_function_name(id)
Session::run(&Code, &mut dyn HostHandler) / drive(Progress, &mut dyn HostHandler) -> Result<Progress, ResumeError>
Session::run_async(&Code, ..) / drive_async(Progress, &mut dyn AsyncHostHandler).await
    // optional driver; Step::{Resume, Stop}; async runs in ASYNC_SLICE_FUEL slices, yielding Pending between them
Session::set_host_args(HostArgs) / set_text_limit(bytes) / set_text_cut(TextCut) / set_last_expr_value(bool)
Session::set_hint(HintGroup, Option<&str>) -> Result<(), PyException>   // redirect sentences per group; session state
Session::call_log() -> Vec<HostCallRecord { call, function, name, status: CallStatus }>
Session::phase() -> Phase / pending() -> Option<Progress>         // the stop as reported (HostCall, Waiting, Parked); snapshots carry it
Engine::environment() -> String                                  // prompt summary generated from the build
HostError { kind, message, data: Option<HostValue>, class: Option<String> }   // HostModule::exceptions
HostSignature { .., dict_as_kwargs: bool }                        // one positional dict binds as keywords
Session::set_output(Box<dyn FnMut(&str) + Send>) / set_output_streams(StreamOutput)
Session::set_fuel(Option<u64>) / fuel() / set_fuel_total(Option<u64>) / fuel_total() / set_deadline(Option<Instant>) / set_deadline_with(now, deadline)   // deadline one-shot
Session::fuel_wanted() -> u64                                    // after Parked{Fuel}: what an operation that parked before starting wants (int operators; direct pow/divmod/sum/math.prod/big-int math/sorted/list.sort); 0 otherwise
Session::set_zone_source(Option<ZoneSource>)                      // TZif bytes by key for zoneinfo; not session state
micro_monty::head_tail(text, max_bytes) -> (String, bool)          // the HeadTail cut, for output text too
Session::park_handle() -> ParkHandle (Send; request()) / request_park()
Session::start(&Code) -> Progress
Session::resume(Resume) -> Result<Progress, ResumeError>          // Unexpected | UnknownCall | AlreadySettled
Session::cancel() -> bool                                         // drops the cell's frames, tasks, futures; keeps globals
Session::snapshot(&mut self) / snapshot_with_limit(max_bytes) -> Result<Vec<u8>, SnapshotError>   // output capped at the memory limit by default
Session::reset() -> Result<(), PyException>
SessionTemplate::capture(&Engine, &mut Session) / instantiate() / image() / from_image(..)
// Progress::Complete(Option<Completion { repr, str, type_name, value: Option<HostValue>, *_truncated }>)
// Progress::HostCall { call, function: u32 (id from define), name, args, kwargs }
// ParkReason: Fuel | Requested | Deadline
```

Output streams through a callback in production order. `HostValue` is plain
data: `None`, `bool`, `int` (bigint as decimal past `i64`), `float`, `str`,
`bytes`, `list`, `dict` with `str` keys, `Record { name, fields }` for
namedtuples, `Tuple`, which keeps a tuple's identity across the boundary
(the cell worker sends it as a JSON array), and `Object { class, fields }`
for host class instances.

**Host calls are synchronous by default.** Calling a host function from any
frame (a loop, a comprehension, a sort key, a dunder) yields `HostCall`; the
embedder answers with `Resume::Value`/`Error` and the call returns its value
in place. This is what pause-anywhere buys at the language level, and it is the
recommended contract for harnesses: models write `text = fs.read(p)`, and a
forgotten `await` is no longer a class of error. For concurrency, the embedder
answers `Resume::Future` and the call returns an awaitable instead, so
`await asyncio.gather(fs.read_async(a), fs.read_async(b))` (or a harness
`parallel(...)` helper) surfaces one `Waiting` batch. Which functions are sync,
async or both is the harness's API decision; the engine supports both.

**Host objects stay plain data.** There are no proxy objects whose attribute
access calls the host. A harness that wants handles (`proc.Job`, a file
revision) defines a host class (`HostModule::classes`): an immutable
`tuple` subclass with named fields whose methods are host functions, crossing
as `HostValue::Object { class, fields }`. That keeps every engine value
snapshot-safe and the boundary small.

**The host surface is one declarative value per module.** `HostModule` is
plain data; `define` replaces the module whole (`Vm::define_host_module`),
and the engine keeps the definition as a `HostValue` for `host_modules()` and
`stubs()`. Each host function has a `u32` id, stable per call name for the
session's lifetime (`Vm::host_id`, a table in the snapshot); `HOSTFN` objects
carry it in a raw slot, so `HostCall` reports it without a lookup.

**Typed host functions and classes are generated Python.** The facade
generates one program per module and the engine runs it once, between
cells, in the prelude's globals (`Vm::run_host_defs`). The program is
compiled once per Engine and shared (`HostProgram`, kept by the Engine's
cache under a 128-bit hash of its source); snapshots refer to it by that
key and carry the module's definition, from which a restoring Engine
without it generates and compiles it again, refusing an image whose key it
does not reproduce (HANDOFF scope decision 60). They are hidden from
tracebacks, survive `reset` and ride in snapshots, and the host-call path
gains only a `_host_defs` lookup when a host function is built. Binding uses
`_mm.signature`; annotation checks raise `TypeError` at the call; there is
no static checker (§1). `Session::stubs()` renders `.pyi` text.
`SessionTemplate` is a snapshot taken after setup that instantiates by
restore (docs/sdk.md).

### 10.2 An embedding example: the cell-protocol worker (`mm-cell-worker`)

`mm-cell-worker` runs micro-monty behind one external, engine-neutral cell
protocol, `plakous-cell-protocol.v2` (length-prefixed JSON frames), and is
used in local integration testing against the harness that defines it
(`publish = false`). It shows how a host maps a frame protocol onto the
facade:

| Cell protocol | Engine |
|---|---|
| `Hello { engine: "micro-monty", adapter }` | — |
| `Configure` | engine limits. The protocol has no micro-monty `EngineConfig` variant, so the worker accepts the Monty variant (a soft memory limit) |
| `StartCell { source, prelude, window }` | install the bootstrap (the protocol's `MontyBootstrap` source is plain Python), compile, `start`; `window` arms a park |
| `NestedCall` ← `HostCall`; `Waiting` ← `Waiting` | — |
| `NestedResult` (batched) | `resume(Batch)`; `Delivery{Accepted}` once the interpreter's next event proves it consumed the batch |
| `Yielded` ← `Parked(window)`; `Resume { window }` | `resume(Continue)` with a new window |
| `Cancel` | `Session::cancel()` discards the cell's frames, tasks and futures (no `finally` blocks run) and keeps globals and the worker |
| `Reset` | `Session::reset()` |
| `CellDone` | `Complete` / `Error` |
| `Shutdown` | `ResourceReport` |

The worker runs the py-ptc presentation: bootstrap, nested calls with
the protocol's Monty worker's argument bounds and messages, Waiting batches and Delivery
verdicts, ordered output with the 1 MiB cap, yields (`window` sets an engine
deadline; a cell waiting on the host also yields when its window ends), and
Cancel, which ends the cell with `Failed{"cell cancelled by the host",
generation_lost: false}` and keeps the worker. For the SDK presentation it
accepts `sdk_stubs` and runs the cells (the harness sends `Reset` and the
SDK bootstrap as a Monty prelude) without a pre-run type check, which is
the harness's (HANDOFF decision 34). Against a local checkout of the
harness it passes 46 of 48 worker-driven protocol tests (2026-11-06 [M]);
the two failures are Monty-only behaviour (the allocator ceiling) and the
SDK test's type-error cell, which expects a checker
(`docs/research/plak-integration.md`, internal).

**Scope (owner decisions, 2026-11-05 and 2026-11-07).** `mm-cell-worker`
is a library a Rust host can run in process, plus a thin binary
(HANDOFF scope decision 44; it was a test-only harness before):

- **Library** (`src/lib.rs`, `unsafe` forbidden): `serve(reader, writer,
  engine, options)` runs one worker over any std `Read`/`Write` and blocks
  the calling thread (one reader thread; no async runtime, so a tokio host
  bridges its pipe with `SyncIoBridge`). The engine is a `CellEngine`:
  `NativeEngine` links the facade; a host can wrap `mm-wasm-host`'s session
  in `CellSession` itself, so the library never links wasmtime. `Hooks`
  (`confine`, `usage`, `boundary`) and `Kill` (park-and-lose, for a worker
  without a process to signal) are what an embedding adds.
- **Binary** (`src/main.rs`): stdio, the seccomp filter (`src/seccomp.rs`,
  the §5 exception), its process resource report, the crash-cut hooks and
  the panic's last-gasp `HostLost`.
- **Policy.** The library adds no policy features (no queueing). Its policy
  values are `Config` with the protocol's Monty worker's values as defaults: 16
  outstanding nested calls and their `RuntimeError` text, the 1 MiB print
  and result caps and their notices, the script name `plak-py-ptc.py`,
  value nesting depth 128 and the 16 KiB nested-error bound. The engine and
  facade impose no outstanding-call limit (pending host futures are bounded
  only by the memory budget, §11).

An in-process prototype (docs/research/plak-in-process.md) runs the
library on a thread of the host's process over `tokio::io::duplex`.
Registering the worker with a harness (its identity, config and discovery)
is the harness's work, not this repository's; integration tests run against
a local checkout of it.

**Worker hardening (binary only):** after the handshake, the worker installs a Linux seccomp
filter allowing only reads and writes on its pipes, memory mapping, and exit.
The engine makes no syscalls, so this costs nothing and turns any engine bug
into, at worst, a dead worker.

### 10.3 Embedding

micro-monty is a pure-Rust crate (`cargo add micro-monty` once 0.0.1 is
published; until then a git dependency on this repository): no C toolchain, no
bindings, and the stdlib's Python modules are compiled into the crate at build
time, so nothing ships beside it. It builds for wasm.

| | rquickjs | dquickts | micro-monty |
|---|---|---|---|
| Stop a running script | interrupt: abort only | suspend if every frame is plain sync code | park at any safepoint, any frame kind |
| Snapshot, resume elsewhere | no | full and delta, same build | FULL, same build id; host dedup |
| Durable host calls | no | yes (`JS_SuspendCall`) | yes, sync and `await` |
| Engine memory safety | C | C | safe Rust, bounds-checked arena |

In-process guarantees: safe Rust and checked arena access protect host memory;
guest effects use host-provided capabilities. Arena memory, execution fuel and
Python/native recursion have limits. The arena limit is not yet a bound on all
session allocations, and fuel is not a hard park-latency guarantee (§11);
docs/sdk.md ("Embedding guarantees") lists what is guaranteed, the minimum
host thread stacks and the test behind each. Two conditions:

1. **Panics.** The release profile uses `panic = "abort"`, and shipped builds
   `panic=immediate-abort` (§6), where an engine bug that panics kills the
   host, without a message in shipped builds. In-process embedders build
   with unwinding; the facade catches the panic and poisons only that session.
2. **Threads.** The engine runs on the caller's thread until the next event.
   Run it on a worker thread, or hand it fuel in slices so it parks and
   returns. `Session` is `Send` (not `Sync`): a session parked on one thread
   resumes on another, so an async host can move it into `spawn_blocking`.
   Its callbacks (output, clocks, entropy) are `Send` and the code table is
   an `Arc` (`tests/send.rs`). `Engine` and `Code` are `Send + Sync`.

Sessions own their arenas and the engine crates have no global or static
state, so one process can run many sessions with independent limits. The one
thing sessions share is immutable: the facade's `Engine` holds a cache of
decoded bundled modules (their bytecode, as the prelude's is shared), the
prelude's unpacked chunks and compiled host-module programs, filled on first
use and capped at `DEFAULT_MODULE_CACHE` (8 MiB) unless
`Engine::with_module_cache` sets another cap. It is charged once to the Engine, not to
any session, and a session is charged and behaves the same with or without it
(HANDOFF "Scope decisions" 45). For fully hostile code, process
or wasm isolation remains the stronger boundary.

Durability guarantees are pause-anywhere and same-build snapshots. Work after
the last snapshot is redone on restore, and an in-flight host call's outcome is
the host's to resolve.

## 11. Limits

One limit per resource the engine can exhaust:

| Resource | Limit | Mechanism | Default |
|---|---|---|---|
| Heap memory | arena segments in use, including their GC bitmaps (+3.1%), plus retained code, frame, scheduler, shape and host-definition storage (the admission paragraphs below) | preflight before big allocations (`*` repeat, `pow`, `<<`, `str.replace`, `join`, `range` → list); raises a catchable `MemoryError` | host-set (facade default 256 MiB) |
| One object | 64 MiB of data or slots (`mm_core::MAX_OBJECT_BYTES`): a str's or bytes' data, a list's or tuple's slots, a dict's or set's tables | checked where a dedicated segment is placed; a bigger allocation raises `MemoryError` whatever the memory limit, so one operation on one object is bounded work (§5). Growth by doubling stops at the cap, whatever the memory limit: a dict holds at most 2,097,152 entries (its index of 2 x cap + 1 words, the last its version, is one word over 64 MiB at the next capacity, 2^22) and a set 5,033,164 items (a table of 8M slots 60% full) [M] | fixed |
| CPU | fuel | charged at safepoints and by natives; exhaustion parks (`Parked{Fuel}`) | unlimited (`Limits::fuel = None`); per cell, host-set; `Session::set_fuel_total` caps the session |
| Wall time | deadline | the host's clock (`set_deadline`, or `set_deadline_with` and the host's own `now()`) read at each safepoint check (§8.6: every 256 safepoints, every one once less fuel is left, and at the next safepoint once natives have charged more than 64 units since the last check); one-shot, parks (`Parked{Deadline}`) | host-set (none by default) |
| Python stack | frame depth | `RecursionError` | 1000 |
| Rust stack | native nesting depth | parser and compiler depth-counted; run-time nesting on arena stacks (§5) | 256 KiB host thread to run, about 650 KiB to compile [M] |
| CPU in one native call | int ↔ str digits; fuel charged by natives | CPython's 4300-digit limit; proportional fuel; an operation estimated at 65,536 units or more parks before starting when the fuel left cannot pay (`Session::fuel_wanted`): the int operators `*`, `**`, `/`, `//`, `%` and, called straight from Python code, `pow`, `divmod`, `sum` and `math.prod` of a list or tuple, `math.factorial`/`perm`/`comb`/`isqrt`/`gcd`/`lcm`, `sorted` and `list.sort`; other long natives run in steps of at most 2^17 units (§5) | 4300 |
| Int size and superlinear int work | bits; estimated native time | an int past 2^26 bits (`intops::MAX_BITS`; linear operations on it take at most about 21 ms [M]) and a superlinear operation (`*`, `//`, `%`, `divmod`, `**`, `pow(a, e, m)`, `str()` and `int()` with the digit limit off, `math.factorial`/`comb`/`perm`/`isqrt`/`gcd`/`lcm`, `round(x, -k)`) whose time, estimated from its algorithm (Karatsuba's products, the squarings of `**`), is past 45 ms (`intops::WORK_MAX`) raise `MemoryError` before starting (the latter with "integer operation too large"), so no such step runs past 50 ms (scope decision 65 (b1)). CPython computes these: `3 ** (2 ** 22)` in 0.61 s, `math.factorial(50_000)` | 2^26 bits; 45 ms |
| Output | byte cap | host callback | host-set |
| Source | size and nesting | checked before and during parsing | 256 KiB |

**Session lifetime.** Before starting another cell, the facade cancels the old
computation. Once the code and shapes loaded since the last reclamation
pass an eighth of the live heap (at least 16 KiB) or a quarter of the memory
left, or when a full collection is due anyway (`Vm::prepare_cell`), it
collects in full and reclaims user compilation units not referenced by
live functions, generators or saved tracebacks; dead code below that
threshold stays, in the session and in its images.
Keeping a unit keeps its nested function bodies too. Code, source and cache
indices are compacted only between cells, never under suspended frames.
Reset also reclaims unreachable user code. The cost includes a heap scan;
measure this on large persistent sessions rather than treating it as free.

Reclamation's live marks and remapping indices belong to the code, source and
shape records, admitted with those records during loading and restore. A fixed
heap cursor visits code and shape references without collecting object addresses
or sorting a slot table. Live references are rewritten only when their indices
move; removing trailing records needs no rewrite scan. Shared traceback entries
are temporarily tagged during remapping and untagged before reclamation returns,
without allocations, callbacks or collections between those passes. The records'
workspace is not serialized. This removes input-sized native reclamation buffers;
allocator-internal transient storage during capacity shrinking is not an RSS bound.

**Retained-code admission.** Code buffers, materialized constant/name vectors,
source strings and code/source/inline-cache table capacities reserve space from
the same limit as the arena. Loads preflight this reservation before cloning;
reclamation and failed loads release it. Bundled and synthesized code also
count. Restore rejects retained state that exceeds the smaller of the saved
and restoring engine's limits, and keeps that smaller limit for future growth.
Source/code table allocations and decoded code buffers check the remaining
budget before allocation, including the expansion of compact encoded vectors.
Bytecode verification uses one scratch bit per byte of code. Loading reserves
the largest such workspace across retained codes for a same-budget restore;
restoration admits it before verification and reuses that allowance across codes.
Frame and generator instruction-boundary checks scan without allocating.

**Host-module admission.** Retained module/function names, constant keys, nested
host-value buffers and their tables share the session budget. Registration
counts the new clone before allocating it; replacement keeps the old definition
live until success, then releases its charge. Caller-owned arguments do not
count, but the session's copies do. Definitions survive reset. Nesting is checked
before cloning, with a 1000-container ceiling. The maximum decoder-stack workspace
across definitions is reserved too, so filling the session cannot consume space
needed for a same-budget restore. Restore admits exact-sized child buffers and
checks depth iteratively; cached byte/workspace charges avoid rescanning host
trees during stack growth. Imports borrow shared immutable definitions rather
than cloning their host-value trees or collecting child names; reference-count
headers count in admission too. Guest values are still separate arena copies,
so guest mutation does not change definitions retained across reset. Other
native scratch still needs accounting.

**Synthesized methods.** Namedtuple/dataclass signature code is hidden from
tracebacks, but is not permanent prelude code. Only reachable functions retain
it between cells and across reset. Its cache holds sorted code indices, compares
parameters already owned by the code, and shares the memory budget; no duplicate
signature-key strings are retained. Reclamation drops dead cache entries and
remaps surviving indices. Restore starts the cache empty; surviving functions
still retain their code.

**Frame-storage admission.** Register stacks, frame records and continuation
tables share that budget, including suspended tasks' stacks. Capacity growth
checks before allocation; task switches transfer rather than duplicate the
charge. Cell completion, cancellation and reset release frame storage, keeping
only a small register buffer. Continuation free-index storage is reserved up
front so unwinding does not need to allocate it. Restore admits these buffers
before decoding and avoids capacity inflation. GC pressure includes outside-arena
reservations. This bounds owned capacities, not an allocator's transient peak
while growing a buffer.

**Scheduler admission.** Task, future and gather tables and blocked/ready queues
share the budget too, including on restore. Registration reserves space for
every task wakeup and gather callback before it can become pending. Duplicate
callbacks retain their order; coalescing them can change interleaving. Settling
and unwinding therefore need no queue allocation. Host futures use an id-sorted
vector so its capacity is measurable, retaining logarithmic lookup and settled
ids until the cell ends. Restoring tasks decodes into their final table without
an intermediate copy. Cell end releases scheduler capacities.

**Instance shapes.** Shape tables, name lists and transition buffers share the
budget, admitted before growth and restore. Above 32 attributes, an instance
switches to a dictionary rather than retaining a quadratic chain of name-list
prefixes. Conversion releases the old inline references and overflow buffer.
Between cells, GC identifies live instances; shape reclamation keeps their
shapes and transition ancestors, compacts IDs, and releases unused capacity.
Inline caches are already cleared by that collection. Shape names are explicit
GC roots; after unused shapes are dropped, weak interning allows their otherwise
unreachable names to die at the next collection.

**Collector scratch.** Before admitting a word object, reserve capacity for its
mark-stack entry from the same budget. Every object can be a root, so the stack
must hold one `u32` per allocated object without growing during marking. GC
recounts survivors from the marks and releases excess capacity with hysteresis.
Restore reserves the same power-of-two capacity classes before decoding engine
metadata; exact-sized restored stacks could otherwise exceed the original
capacity on the first subsequent allocation. Arena commitment statistics still
exclude this outside-arena storage; `Heap::available_bytes` includes all current
reservations when deciding whether more storage fits.

**Segment metadata.** Segment/bitmap descriptors and vacant-index capacity
share the budget. Table growth reserves enough slots for every possible segment
release, so GC need not allocate them at memory pressure. Empty spaces allocate
no tables. Restore admits the same capacity classes, decodes vacant indices
directly into their final storage and validates index coverage using descriptor
flags, without temporary index tables.

**Completion text.** The facade cuts a completion's `repr` and `str` at the
session's text limit, 1 MiB by default or the memory limit if that is less
(owner decision, HANDOFF "Scope decisions" 43); printed output is not cut.
User-rendered `repr` and `str` stay in the budgeted arena
until handed to the caller. Pending text is a GC root and snapshots store its
value references, without separate string copies. The facade copies only the
requested UTF-8 prefix into caller-owned buffers; truncating output does not
retain the full user string's capacity. This does not interrupt user rendering
or change the host-value conversion budget. Native completion buffers also
release capacity beyond the requested limit. Native `repr` bounds its builder
while appending, including within string and byte leaves, rather than rendering
a whole leaf before truncation. It keeps at most one UTF-8 character (four bytes)
past the limit to distinguish exact fit from truncation; opaque host conversion
still rejects incomplete representations. Quote selection still scans the input.
Temporary strings from other rendering helpers and traversal workspace are not
covered by this output-buffer cap.

**Bundled-module workspace.** Import admits unpacked bytes, Huffman tables,
the decoder's string pool and the decoded program before copying them. The
maximum workspace across loaded modules stays reserved for same-budget restore,
including while retained codes are cloned and constants materialized. A failed
code load releases any new workspace reservation. Restore admits that reservation
before decoding and keeps only one decoded module at a time, replacing it before
decoding another. The charge conservatively includes temporary buffers that need
not coexist; it does not reserve the sum of all imported programs. A
cell's section decoders for bundled modules live until the cell ends.
Every section a decode unpacks is charged their tables, so a charge does
not depend on which decoders were already built, and the reserved
workspace covers the decoders a cell keeps.

**Native string builders.** `join`, `replace`, f-string concatenation and the
string pieces used by splitting/regex admit their builder capacity before
copying. The native buffer remains charged while the arena string is allocated;
successful conversion does not hide a double-buffer peak. Small reusable builder
capacity stays charged; success and failure release temporary reservations.
Builders borrow the VM read-only while filling pre-sized buffers.
Replacement sizing counts actual nonoverlapping
matches and UTF-8 bytes, including empty patterns and zero counts, rather than
rejecting a large worst-case estimate when the actual result is small.

**Fixed VM tables.** Builtin type and constructor tables, per-bundled-module
code/global slots and the shared park flag reserve their capacity before
session construction or metadata decoding. Catalog entries count even before
import; the caller-owned catalog and packed module bytes do not. Restore checks
the fixed type count before allocating its table. The initial register stack
also reserves its capacity before construction, rather than after initialization.

**Restore admission.** `Engine::restore` reads into one budget, the
smaller of the saved and the restoring engine's memory limits, which the
session keeps: the arena, retained code and source, frame storage, scheduler
tables, shapes, host definitions, collector and segment metadata, fixed VM
tables, bytecode verification scratch and the bundled-module decode
workspace are each admitted before allocation (`Vm::read_snapshot`,
`TooLarge` past it). Not in that budget: the caller's image bytes, the
pending stop's host values that `Session::pending` re-reports (the host's
data, capped separately at the memory limit, as snapshot output is), the
decoded modules the Engine's cache shares, and the allocator's transient
peaks.

**Native iterable buffers.** `Vm::collect` reserves native value capacity
before copying or growing. The buffer owns its reservation until it
is dropped, including while the caller allocates an arena result or collects a
nested iterable. Copies saved for a one-shot iterator's prelude fallback count
separately; the replay table counts as execution storage and is released at cell
end. Core scratch reservations share the arena/external budget but release
independently, so nested operations cannot replace each other's charges. They
can grow while borrowing arena input: an atomic update admits against the
fixed non-scratch budget, so concurrent owners cannot overcommit it. They do
not root values or permit GC inside natives. Allocation refusal remains a
`MemoryError`, including through joins, dictionary construction and starred
unpacking. Native sorting admits its merge workspace separately; `list.sort`
also admits its copied input and reuses the list's existing backing store when
writing the successful result. Failed workspace admission leaves the list
unchanged, including reverse sorts. Other direct list/tuple copies still need
admission; JSON's admitted buffers and remaining gaps are described below.

**Byte result builders.** Byte concatenation, repetition, zero-filled construction,
join, replace and padding reserve their native output before copying and retain
the charge during arena allocation. Builders receive a fixed-size slice, not a
growable buffer. Join, replace and padding borrow their inputs; replace sizes
actual nonoverlapping matches, including empty patterns and bounded counts.
Padding that adds no bytes returns its immutable input after validating arguments,
so it needs no result allocation at memory pressure. Case transformations borrow
their source and mutate a single admitted output buffer in place. Search, count,
prefix/suffix checks and predicates borrow byte inputs without copying them;
integer needles use one stack byte, and tuple affixes are visited directly.
Iterable-to-byte conversion and hex decoding reserve their output through the
arena copy; the iterable's value buffer keeps its independent reservation.
Hex encoding borrows its input and uses the admitted string builder. Hex decoding
counts non-whitespace bytes first, so whitespace-heavy input reserves only its
decoded length. Slicing, splitting, partitioning, stripping and prefix/suffix
removal borrow their sources; unchanged byte ranges reuse the immutable value.
Contiguous pieces up to 256 bytes use fixed stack scratch, and larger pieces and
strided slices reserve native output through the arena copy. Split result tables
own their reservations through list allocation; partition uses a fixed array.
Codecs borrow their string/byte inputs; encoders iterate Unicode scalars without
materializing a character vector. Codec output reserves its exact encoded byte
capacity before allocation and retains the charge through the arena copy.
Sizing includes error-policy expansion and Latin-1-to-UTF-8 expansion; ignored
characters need no output capacity. The sizing scan and conversion share invalid
UTF-8 run boundaries. Codec and error-handler names stay borrowed; normalization
uses a fixed ten-byte buffer (the longest supported alias), and name-bearing
lookup errors use the admitted string builder. Other rendering buffers still
need admission.

**Rendering inputs.** String leaves append their quoted form directly to the
enclosing renderer; byte leaves borrow their source. This removes intermediate
leaf copies without changing quoting, escaping or traversal limits. Native
`repr` visits container entries through cursors rather than copying their values
and scheduling all children at once. Cursor storage grows with nesting depth,
not container width. Backing-buffer locations stay valid because native rendering
neither mutates containers nor collects; user rendering still hands off to Python.
Completion preflight likewise borrows child slots, with one cursor per ancestor,
and scans exception arguments without copying them. It preserves last-child-first
visitation, cycle detection and the existing depth/visited-container thresholds.
Its depth limit fits a fixed 65-entry cursor stack; visited identities share
the session budget. If identity admission fails, preflight releases its storage
and selects Python rendering, as it already does at its traversal thresholds.
This fallback can park or raise `MemoryError`; a capped completion does not
guarantee that inspecting its input or producing user-rendered text fits.
Native `repr` admits its work-stack capacity and active-identity table before
growing them, retaining both identity tables during rehash. Refusal raises
`MemoryError` and releases all traversal reservations, including on truncated
completion output. Scalar rendering needs no traversal allocation.

**Native rendering output.** `repr`, `str`, exception text, ASCII conversion and
final format padding return text with its own reservation. Builders admit
capacity before growth, keep it through consumer allocations (including the
arena result copy), and release it on success, error or Python handoff. Writers
borrow the heap only while appending, not across traversal mutations. A failed
write is a `MemoryError`, not truncated successful output. Public text APIs
transfer the completed string to the caller and release its engine charge.
The completion output cap above is separate from this shared budget; preflight
fallback can still need full rendering before the caller receives a prefix.

ASCII conversion reuses already-ASCII text, otherwise admits the exact escaped
length and releases the source when conversion finishes. Format padding admits
UTF-8 bytes, places sign/prefix separately from padding, and fills long runs by
copying complete character runs within the reserved buffer. String precision
copies only the requested character prefix. OSError assembly admits its
accumulator and one converted field at a time. Numeric intermediate strings,
class-name/error-message helpers and some string-format input/workspace copies
still need admission; these rules are not a total-memory or RSS guarantee.

**Printing.** Native `print` borrows separators, endings, builtin strings
and the string backing of subclasses without their own `__str__`. Other values
retain the existing conversion and prelude handoff. Native stream `write` passes
the arena string directly to the host output callback; it still returns the
Unicode character count and delivers empty writes. Native output does not collect
or run Python while borrowing text. Callback chunk boundaries and stream routing
are unchanged. Accumulated `print` output uses the shared admitted text buffer,
reserving before growth and retaining its charge through the host callback.
Nonempty buffers start with at least eight bytes when that slack fits; empty
buffers allocate nothing, and growth falls back to exact capacity under pressure.
Errors and prelude handoff release the buffer; a failed native attempt emits no
partial output. Refusal may precede a later conversion error or user callback.
Custom files keep their existing per-piece prelude writes. The builder sizes
borrowed strings from raw storage before reserving, then validates UTF-8 once
when appending, rather than twice on growth. Non-string conversion results
retain their own reservation while copied into the print buffer; intermediate
rendering helpers have the remaining gaps above. The host's output cap remains
separate from this workspace.

**Nested exception text.** Exception arguments that are themselves exceptions
hand `str` rendering to the existing prelude. Ordinary arguments retain the
native path, borrowing argument slots rather than copying the tuple. OSError
copies only its individual field values before formatting. These renderers neither
mutate nor collect the argument storage. Explicit exception `__repr__` and
`__str__` methods likewise read argument slots without a whole-tuple copy;
user rendering still hands off before any Python code runs. Cycles through
errno/message use VM frames, fuel and recursion limits rather than the Rust stack.
Completion and uncaught error reporting recognize the same handoff; failed
reporting preserves the original exception and uses `<exception str() failed>`.

**Native formatting preflight.** After parsing a builtin format spec, requested
width × UTF-8 fill size must fit the allocation budget, before type-specific
rendering or zero-filled digit grouping. This is conservative: an oversized
width can raise `MemoryError` before a type-specific format error. User-defined
`__format__` still receives its spec unchanged. Complex precision preflights the
finite components actually rendered when the presentation grows with precision;
non-alternate `g` and non-finite components do not reserve useless digit space.
Fixed complex formatting shares the float helper that appends zeros beyond an
exact double expansion, avoiding Rust's smaller precision ceiling. Allocation
preflight checks overflow through rounding, headers and large-block overhead.
Format-spec parsing borrows UTF-8 slices and parses numeric fields in place;
only the fill and presentation type need Unicode scalar decoding. It creates
neither a character vector nor copied width/precision strings. Builtin `format`
and f-strings pass their arena spec value without copying it. Native `str.format`
passes its existing field slice; its template/field storage and other formatting
helpers still allocate temporary buffers.
These checks prevent oversized requests; they do not reserve every intermediate
formatting buffer or close the native-scratch accounting gap.

**Percent-format buffers.** Native `str % args` and `bytes % args` admit their
format-input copy before allocation and retain its reservation until the run
returns. Accumulated output admits capacity before growth, falling back to exact
capacity when geometric slack cannot fit, and keeps its reservation through the
arena-result allocation. A padded conversion reserves its prefix, padding and
suffix together; literal spans append in batches. Errors and Python handoff
release both buffers before unwinding or executing user conversions. Tight budgets
may therefore raise `MemoryError` before a later formatting error or user callback.
Positional argument lookup reads the selected tuple slot rather than copying the
whole tuple for each field or `*` width/precision. User conversions still use the
existing prelude restart protocol; this does not make repeated user handoffs linear.
Each attempt borrows the prelude's converted-value list through its backing-buffer
reference and length; native formatting neither mutates it nor collects. Type names
are borrowed only for errors, retaining the original argument's type after conversion.
`%c` validates string/byte length before copying, so rejected inputs need no native
data copy and accepted inputs copy at most one UTF-8 character or byte.
Builtin string/byte fields and prelude-converted text retain their arena values
through precision and padding rather than copying the whole conversion first.
Precision records a byte cutoff (at a character boundary for string formats);
padding admits output capacity before reacquiring and copying that slice.
Native numeric/repr conversion buffers and other formatting helpers still need admission.

**Native JSON output.** Encoding admits native output capacity before each growth
and holds that reservation through allocation of the arena result. Quoted strings
size actual escapes, including UTF-16 surrogate pairs for non-BMP characters under
`ensure_ascii`; indentation checks depth × UTF-8 bytes before repetition. Growth
uses exact capacity when geometric slack cannot fit. Indentation and separators
remain borrowed arena values rather than copied native strings. Generated bytes
bring forward control polling even when a container has fewer than 1,024 values;
the native operation still finishes before parking. Lists, tuples and unsorted
dictionaries are traversed through bounded arena-storage cursors, without copying
whole containers. Those references remain valid because native encoding neither
collects nor runs user code; a `default` callback restarts encoding in the prelude.
Sorted dictionaries admit their key list and ordered entry storage before copying;
the entry reservation lasts until that dictionary finishes encoding. They no
longer copy the whole unsorted entry table first. Encoder traversal frames admit
their capacity before growth and keep the reservation until encoding returns,
including after frames are popped. Cycle tracking uses an admitted half-full
identity table for the current ancestor path, not all visited containers.
Growth charges both old and new tables until rehashing finishes; closing a
container removes its identity without allocating, using backward-shift deletion.
Capacity tracks peak depth rather than the number of completed siblings. A
repeated ancestor reports a cycle before attempting growth; shared children on
completed branches remain legal. `check_circular=False` allocates no cycle table.

**Native JSON input.** Decoding admits its native input copy before allocation;
the copy stays charged until scanning and error-position conversion finish.
The scanner retains this copy because it mutates the VM while reading input.
Array values and object key/value buffers reserve capacity before growth and
remain charged through construction of their arena containers. Actual retained
capacity replaces the former periodic value-count estimate. Value-count fuel
charges remain unchanged. Per-document key memoization keeps arena references
in an admitted, half-full linear-probing table, with the same 4096-key ceiling.
It no longer copies key text into a native tree. Growth admits both old and new
tables during rehashing; no native GC occurs while these references are held.
Cached full hashes reject bucket collisions before comparing key text. Escaped
strings use the admitted JSON output builder and retain its reservation through
arena string construction, including keys. Unescaped strings still borrow input
ranges. Decoder traversal frames admit their capacity before growth and keep
that charge while completed children become arena containers. A failed traversal
drops its frames and their independent child-buffer reservations. Both JSON
directions retain the existing depth ceiling; insufficient memory can refuse
earlier with a catchable `MemoryError`.

**Host-value conversion.** The `HostValue` trees the engine builds for
host-call arguments and completion values reserve from the session budget as
they grow (doubling reservations), and the conversion keeps its nesting on an
explicit stack whose entries are charged too. Past the budget, a strict
argument conversion raises `MemoryError` at the call, a lenient one yields
`TooLarge`, and a completion gets no host value (its `repr` still comes back).
The charge is released when the tree is handed to the host, which owns it from
then on. A 64 MiB session returning a list of 3,000,000 ints peaked at
265,388 KiB RSS before this and 101,508 KiB after [M].

**Guest-sized native buffers** (memory audit, 2026-11-09). A native whose
Rust buffer the guest can make larger than the data it reads admits that
buffer before filling it, or bounds it by a constant: `re` matches waiting
for the arena (a call ends its slice after 2^16 span words) and the
backtrack stack (12 bytes a word past 2^16, for it and a slice's saved
copies); `str.join` over str subclasses; `struct` format items; split
ranges; `strptime`'s fast path (formats past 4096 tokens go to Python);
`ast.parse` (512 bytes per source byte: the native tree reaches about 410
[M]; at most 32 KiB of source) and `tokenize` (64; at most 64 KiB); `unicodedata.normalize` (16 bytes per decomposed
char); power-of-two digits and their copies (`format`, `%x` of ints up to
a million digits; `bin`, `oct`, `hex` and longer formats write theirs
straight into the result's block);
float and complex precision text; `expandtabs` and case mapping (sized
first); JSON escape staging; host-call argument vectors.

**The memory bound** (owner decision, 2026-11-09). For any guest input,
while a cell runs, a session with `Limits::memory = L` keeps the
process's Rust heap within **2.5 × L + 16 MiB** of what it held before.
The parts are:
- the arena, at most L;
- at most about one transient native copy of the largest value a native
  reads (the copies listed under "Accounting still to complete" below);
- fixed tables and capped scratch: the `re` and native-scratch
  allowances, the bundled modules a cell decodes.

The measured high-water is 2.01 × L [M], from `bytes.find`'s KMP table,
over 73 probes at 16, 64 and 256 MiB. The ratio is the same at each
limit.

Compiling a cell is inside the bound when it goes through
`Session::compile` (owner decision, 2026-11-09). The session reserves 512
bytes per source byte + 16 KiB as scratch while it parses and compiles,
releases it after, and returns a `MemoryError` compile error when that
does not fit what is left (after a collection, when no cell is stopped).
The worst measured is 405 bytes per source byte [M]: one-character
statements, just past a doubling of the statement vector. The cell
worker and the mm-wasm guest compile cells this way.

Outside the bound:
- **`Engine::compile`:** a host compiling ahead of any session, at most
  512 bytes per source byte + 8 MiB [M].
- **Snapshots:** writing or restoring one holds the arena and the image
  once: heap ≤ L + image + 16 MiB [M]. The image is measured, then written
  into a buffer of exactly its size. A pending host call's encoded
  arguments, which the session keeps for `Session::pending` and the image,
  are charged to the session while kept.

Enforcement:
- `cargo xtask check` runs `fuzz/src/bin/memory_probes.rs`, which replays
  `fuzz/memory_probes/*.py` under a counting allocator (§5), snapshots
  and restores each session after its cell, and fails on any probe over
  the bound, the snapshot bound or the compile bound.
- The `memory_bound` fuzz target, at an 8 MiB limit, searches for inputs
  that break either.

Hosts size process memory from the bound. A wasm host gives the
instance about 3 × L + 16 MiB of linear memory, which covers the run
bound and a snapshot. The guest frees an input or output buffer larger
than 64 KiB once it has been read, so a written or restored image is
memory the arena can reuse, not a lasting addition (linear memory never
shrinks).

**Accounting still to complete.** Copies of up to about one more time the
data a native reads stay outside the budget (receiver copies of `str`
methods, `list + list`, repetition, big-int operands and temporaries,
the KMP table, `str + str`'s staging, call-argument vectors, exception
text); they set the factor 2.5 in the memory bound above. `Limits::memory`
is still not a total session-memory limit. `Engine::compile`'s
allocations (up to about 405 bytes per source byte [M], so about 101 MiB at
`MAX_SOURCE_BYTES` [E]) and host-owned `Code`/`HostValue` objects must be
distinguished from session-owned memory, allocator overhead and process
RSS.

**Snapshot output.** `Session::snapshot` caps output-buffer capacity at the
session's memory limit, independently of live state, so a nearly full session
can still checkpoint. `snapshot_with_limit(max_bytes)` overrides that cap for
one call. Heap, code and VM encoders stream twice: once to measure the
image, then into one buffer of exactly its size, the header and checksum
included, allocated only if it fits the cap. Oversized output or a failed
allocation returns `SnapshotError::TooLarge`; the session remains usable,
although GC may have run. The successful buffer belongs to the caller. The
cap excludes allocator overhead and other snapshot scratch; it is not a
process-memory guarantee.

**Responsiveness.** A native fuel charge exceeding the small-work polling threshold
brings forward the next safepoint even with unlimited fuel, so pending requests
and deadlines do not wait through subsequent large calls. Native sorting charges
each comparison, including comparisons made before a failure or Python handoff.
A native whose work grows with its data does at most `nstep::STEP_WORK`
(2^17) fuel units per step, and each step is a safepoint (§5, "Bounded
native steps"), so sorting, copying, rendering and conversion park between
steps. `crates/micro-monty/tests/steps.rs` fails if a `Bounded` row's
longest gap between two safepoints passes 50 ms (release, scaled from its
reference machine); a row may be marked `Open` for a known site that
still exceeds it, and none is at 0.0.1. Not steps, and not cut by a park request or deadline: a full
collection (§9.2), `Session::snapshot`, `Engine::restore` and
compilation. `bench/latency.rs` measures request-to-control separately
from fuel: a host call also returns control, and is distinguished from an
explicit park. Do not promise hard real-time cancellation. Fuel is per
cell (`Limits::fuel`, or what `Session::set_fuel` set for the next cell);
`Session::set_fuel_total` caps all of a session's cells together, though
refuelling a parked cell with `set_fuel` is not capped by it.

Disk, network, processes and concurrent host calls are harness resources;
the harness enforces their quotas and resolves uncertain external effects.

Fuel and deadlines surface as typed `Parked` reasons, never as exception text
the host must parse; memory, object-size, int-size and recursion limits raise
Python exceptions (`MemoryError`, `RecursionError`) that guest code can
catch, an uncaught one reaching the host as `Progress::Error` with its
`type_name`, and source limits fail compilation (`CompileError`). Errors raised to Python use CPython
wording.

## 12. Features and performance targets

The default module set, the facade's `default` features
(`crates/micro-monty/Cargo.toml` is the source of truth): `mod-json`, `mod-re`, `mod-collections`,
`mod-itertools`, `mod-functools`, `mod-datetime`, `mod-random`,
`mod-asyncio`, `mod-typing`, `mod-base64`, `mod-hashlib`,
`mod-dataclasses`, `mod-operator`, `mod-copy`, `mod-heapq`, `mod-bisect`,
`mod-string`, `mod-textwrap`, `mod-enum`, `mod-abc`, `mod-binascii`,
`mod-time`, `mod-os`, `mod-pathlib`, `mod-fnmatch`, `mod-posixpath`,
`mod-csv`, `mod-io`, `mod-gc`, `mod-types`, `mod-contextlib`,
`mod-traceback`, `mod-logging`, `mod-urllib`, `mod-pprint`, `mod-shlex`,
`mod-difflib`, `mod-struct`, `mod-html` (with `html.parser`),
`mod-warnings`, `mod-codecs`, `mod-platform`, `mod-tempfile`, and since
the codemode census (owner decisions D2, T1 and S1, 2026-11-05)
`mod-decimal` (with `mod-numbers`, `mod-contextvars`), `mod-fractions`,
`mod-statistics`, `mod-tomllib`, `mod-calendar`, `mod-hmac`, `mod-zlib`
and `mod-zoneinfo`, and since 2026-11-07 (owner request; 2 of 2,841 recorded
real cells) `mod-tokenize` (with `token`; mm-engine feature `tokenize`).
Optional: `mod-uuid`, `mod-unicodedata`, `mod-html-entities` (32,470 bytes
of bytecode [M]) and `mod-ast` (mm-engine feature `ast`; about 48 KB of
`hello.wasm` for 3 of 2,841 recorded real cells, so opt-in by owner decision,
2026-11-07; `mm` enables it, so `cargo xtask check` runs its own case,
conformance and durability); later:
`unicode-names`, `bytecode-only`. `mod-asyncio` and `mod-json` gate the
engine's own modules (mm-engine features `asyncio` and `json`): their
natives, prelude units and, for `asyncio`, the scheduler's task, gather,
sleep and loop paths. Coroutines, `await` and host futures are core and
stay. The facade forwards every `mod-*` feature and its `default`
feature is the default set (mm-stdlib's own default is empty).
`math`, `cmath` and `sys` are engine modules, always built, with no
feature (`mod-math`, which gated nothing, was dropped by owner decision
62). Each `mod-*` feature turns on every module its
modules import when they are imported; mm-stdlib's build script reads
the bundled modules and `Cargo.toml` and fails on a missing one, so
every module set the manifest allows imports cleanly. Imports inside
functions are not part of the graph, so these need the second feature
listed too: `time.strptime` needs `mod-datetime` (which itself needs
`mod-time`, so declaring it would make a cycle), `Fraction.from_decimal`
`mod-decimal`, `os.path.expandvars` `mod-re`, and `types.coroutine` on a
plain function `mod-functools` (none of them is reached by the 2,841
recorded real cells or the 1,782 codemode solutions,
docs/research/stdlib-profile.md);
`cargo xtask check` builds the engine with no module features and the
facade with none, and runs `crates/micro-monty/tests/features.rs` on a
facade that opts out of `asyncio` and `json` and into `ast`.

### Native or Python: the hybrid stdlib

- **Rust:** builtin types and their methods (`str`, `bytes`, `list`, `dict`,
  `set`, sorting), and modules whose cost grows with input size or that every
  other module calls: `json`, `re`, `hashlib`, `base64`, `math`, `random`'s
  generator, and `datetime`'s string parsers (the exception below).
- **Python:** API surface and logic, adapted from CPython's pure-Python modules
  where they exist (`_pydatetime` formatting helpers, `collections` extras,
  `functools`, `itertools` glue, `heapq`, `bisect`, `textwrap`, `string`,
  `fnmatch`, `shlex`, `difflib`, `statistics`, `fractions`, `copy`, `pprint`,
  `dataclasses`, `enum`). `dataclasses` and `namedtuple` must be rewritten to
  build methods without `exec`/`eval`. PSF license; notices in
  `THIRD_PARTY.md`.
- Python modules live in `crates/mm-stdlib/python/`, are compiled to bytecode
  at build time, embedded in the crate, and imported lazily, so session start
  stays near 1 ms.
- A module moves from Python to Rust when a benchmark shows it hot.
- **Exception: `datetime`'s parsers** (owner decision, 2026-11-05).
  `datetime` is Python because its work is not data-proportional, but
  `date`/`time`/`datetime.fromisoformat` and `strptime`'s matching are
  native (`mm-engine/src/datetimemod.rs`, feature `datetime` from
  `mod-datetime`, as `_mm.dt_iso` and `_mm.dt_strptime`). Reason: agents
  parse timestamps in loops, and these were the slowest codemode cells
  (per call, `fromisoformat` about 100–175× CPython in Ir and `strptime`
  2.6× [M], 2026-11-05).
  `fromisoformat` ports CPython 3.14.7's C parser line by line over the
  UTF-8 bytes, so results and errors are CPython's; `strptime` takes the
  common directives (`%Y %m %d %H %M %S %f %z %y %b %B %a %A %p %I %j %%`)
  and returns `None` for anything else or any error, and the Python
  `_strptime` (CPython's own Python code) takes over. Python still raises
  and builds the objects, skipping the constructors' checks for fields the
  parser checked. Both charge fuel per input byte, run between safepoints
  and add no snapshot state. Cost: +4,668 B (`fromisoformat`) and +9,153 B
  (`strptime`) of `hello.wasm` [M]. Aware arithmetic and comparison
  (owner decision, 2026-11-05) add only `_mm.dt_ord` and `_mm.dt_ymd`, the
  ordinal of a valid `(y, m, d)` and its inverse, constant-time on ints and
  `None` for anything else; the aware paths themselves stay Python,
  rewritten after CPython's C, so `tzinfo` subclasses run as ordinary
  calls. The rest of `datetime` (formatting, `strftime`) stays Python
  unless the owner decides otherwise.

Why: models should never hit a missing module, and hand-writing every module
in Rust (Monty's approach) makes the long tail the schedule. Python-written
modules are pausable for free, carry CPython's semantics, and bytecode is
denser than machine code. The cost is per-call speed. CPython 3.12.14's own C
modules against its pure-Python versions [M, this orb]:

| Operation | C | Pure Python | Ratio |
|---|---|---|---|
| `datetime + timedelta` | 0.06 µs | 7.3 µs | ~120× |
| `datetime.fromisoformat` | 0.13 µs | 3.4 µs | ~26× |
| `datetime.isoformat` | 0.50 µs | 1.4 µs | ~2.7× |
| `bisect`, 1 lookup in 10k | 0.13 µs | 0.95 µs | ~7× |
| `heapq` heapify + pop 10k | 2.8 ms | 15.5 ms | ~5.5× |
| `OrderedDict` 1k inserts + moves | 67 µs | 493 µs | ~7× |
| `json.loads` 2k rows (~150 KB) | 1.3 ms | 13.6 ms | ~10× |

Per operation that is 5–10× typical and up to ~100×; per agent cell it is
usually noise next to host calls of milliseconds to seconds, except for
data-proportional work, which is why that work is native.

### `re`

`re`: an own backtracking engine with fuel (QuickJS libregexp ideas
[E 2.5k LOC, 100 KiB]); lookaround, backrefs, Unicode `\w\d\s\b`; `\p{..}`
rejected as in CPython. `re.Match` is a native type (`re.rs`), and
`Pattern.match`, `search` and `fullmatch` build it in one native call
(`_match`) that runs one step slice; a longer run, or arguments it does
not take, continue in `re.py`'s `_exec` loop from the state that slice
saved, a safepoint per slice. A search skips the start positions where
none of the single-width items a match can begin with matches, a step per
char.

Speed targets in ms [E]; reference numbers [M v1]:

| Bench | micro-monty | Monty 1.4 | QuickJS-ng | CPython 3.13 |
|---|---|---|---|---|
| startup | ≤ 1.5 | 1.5 | 1.3 | 9.7 |
| fib(27) | ≤ 45 | 84.7 | 44.4 | 27.7 |
| loop 3M | ≤ 250 | 624.7 | 210.4 | 321.4 |
| dict 200k str keys | ≤ 120 | 160.0 | 163.4 | 117.7 |
| string build/split/replace | ≤ 60 | 97.0 | 63.5 | 46.9 |
| json 50k rows | ≤ 160 | 218.2 | 284.2 | 158.3 |
| closures 1M calls | ≤ 90 | 229.9 | 89.5 | 88.2 |

Worth-switching bar: beat Monty on every row; ≤ 1.2× QuickJS-ng on fib, loop
and closure.

**At the MVP** [M, `cargo xtask bench`, median ms per process including
startup, Xeon 2.6 GHz, pinned with `taskset -c 2`, 2026-11-01; QuickJS is
dquickts' `qjs`]:

| Program | micro-monty | Monty 1.4 | CPython 3.14.7 | QuickJS | mm / Monty |
|---|---:|---:|---:|---:|---:|
| startup | 3.8 | 4.1 | 13.2 | 3.4 | 0.93 |
| fib | 36.0 | 107.8 | 35.0 | 22.0 | 0.33 |
| loop | 115.5 | 242.0 | 217.9 | 69.3 | 0.48 |
| dict | 141.9 | 180.9 | 125.7 | 118.6 | 0.78 |
| str | 50.6 | 73.5 | 45.9 | 38.7 | 0.69 |
| json | 150.9 | 231.7 | 184.1 | 214.9 | 0.65 |
| closure | 73.7 | 195.3 | 88.9 | 40.2 | 0.38 |

micro-monty beats Monty on every row and CPython on loop, json and closure.
It is 1.6–1.8× QuickJS on fib, loop and closure, so the ≤ 1.2× QuickJS bar is
not met. Process startup is mostly `exec`: `/bin/true` takes 1.2 ms on this
machine; in process, a new session plus compiling and running one statement
takes 0.46 ms median [M]. The S1 spike (docs/spikes/dispatch.md) puts the
remaining gap in dispatch and attribute access, the M10 work.

**Coverage programs.** The seven rows above are mostly calls and loops, so
`bench/` also holds six programs without targets that guard against
overfitting them: classes, generators, exceptions, fmt, sort and interp
(`bench/README.md`). On 2026-11-03 [M, `cargo xtask bench` and callgrind,
`bench/results/`] micro-monty passes all six but is 2.1–5.8× CPython 3.14's
wall time on them (sort worst: 677 vs 117 ms, 8.2G vs 0.9G instructions).

**Instruction counts.** `bench/callgrind.py` counts each program's Ir
under callgrind twice. The *gated* count runs from glibc's
`__libc_start_main` onward and leaves out glibc's `pthread_getattr_np`; the
*whole-process* count includes everything. The ±0.5% bench rule (HANDOFF
scope decision 2) applies to the gated count of every program, and the
whole-process count is reported beside it. The two parts left out depend
on where the binary's segments land, not on engine work: the dynamic
loader's relocations, and `pthread_getattr_np` parsing `/proc/self/maps`,
which Rust's runtime calls from inside `main` to find the main thread's
stack. That is why counting from `main` alone is not enough: on
decision 60's change, `startup` moved +0.80% whole-process, +1.06% from
`main` and +0.03% gated [M]. On the other programs the two counts differ by
a constant of about 337K Ir, 0.10% of the smallest (owner decision
63, 2026-11-08).

## 13. Tests and conformance

- **Conformance corpus:** `conformance/monty/` holds Monty's 726 test cases at
  `66fff6bf` (MIT; notice in `THIRD_PARTY.md`). The runner (`cargo xtask
  conformance`, next phase) understands Monty's `# /// monty` headers and
  `# Return=`/`# Raise=`/`TRACEBACK` expectations, scores each case against
  CPython 3.14.7 (primary oracle, patch-pinned because patch releases change
  error text) and tags it `cpython`, `monty-specific` or
  `unsupported`. The 69 `refcount__*` cases are excluded. Cases needing Monty's
  host fixtures (69 `call-external`, 12 `mount-fs` [M]) are tagged until the
  runner has an equivalent host. The corpus also runs on `mm` built for
  `wasm32-wasip1` under wasmtime (`cargo xtask conformance --engine
  micro-monty --target wasm32-wasip1 --check`, in `check`), which must give
  the native results case for case: one engine, whatever the target's build.
- **Own tests** for what Monty lacks: inheritance, generators, `del`, async
  iteration, lazy genexprs, exception subclasses, parking.
- **Differential fuzzing:** a grammar-based generator of subset programs, run
  on micro-monty and CPython; plus cargo-fuzz targets for the parser and
  compiler, the VM on mutated code, the snapshot reader, `re`, `json.loads`,
  and deeply nested source and data (§5). No panics, no stack overflows, no
  hangs past fuel.
- **Error quality:** traceback cases compare micro-monty's traceback text,
  including carets and hints, with CPython 3.14's.
- **Durability tests:** for every passing case, park every N fuel units, take a
  FULL snapshot, restore in a new process and continue; output must be
  identical. Also park at every host call. As built: `cargo xtask durability`
  (part of `cargo xtask check`) runs every own case and every corpus case
  `micro-monty.jsonl` records as passing through `oracle.py --durability`.
  Each case parks every max(5, ⌈fuel / 50⌉) fuel units; own cases and one
  corpus case in three also park every 1 unit (capped at ~1000 parks). Every
  park is written by one `mm` process and continued by a new one. At the MVP
  [M]: 808 runs, 20,466 parks, all identical to an unparked run, 34 s on
  4 workers.
- **Size CI:** `cargo xtask size` against the budget; report sections and a
  per-crate bloat diff on PRs.
- **Bench CI:** the §12 programs, flag regressions above 5%. On wasm,
  `cargo xtask wasm-wall` times them against a base revision on wasmtime and
  V8 (steady state, `--no-liftoff`) in alternating rounds and fails on a
  geomean of median ratios above 1.03 or a program above 1.05 (lower
  quartile above 1); calibrated on the same build against itself at 0.991
  and 0.999 [M]. Run before pushes that touch dispatch, the per-target
  build or the wasm crates, and on release tags (AGENTS.md).

## 14. Milestones [E durations]

| # | Milestone | Weeks | Acceptance |
|---|---|---|---|
| M0 | Scaffold | done | Workspace, lints, CI (fmt, clippy, tests, size), orb setup, corpus, worker handshake |
| S1 | Dispatch spike | done | {stack, accumulator-register} × {`match`, fn table} on fib, loop, closure, attribute, method, dict/list kernels; results in `docs/spikes/dispatch.md`; done: accumulator + `match` (§8.1) |
| M0b | Harness | done | Conformance runner scoring CPython 3.14.7 on the corpus; bench programs and runner |
| M1 | mm-core | done | Value, segmented arena, GC with side bitmap, write barrier, interning; property tests and fuzzing for alloc and GC; 1M small allocations < 50 ms |
| M2 | Compile + VM | done (core) | ruff → AST → bytecode with position tables; dispatch, calls, closures, exceptions, frame stack, depth-bounded natives; fib ≤ 60 ms, closure ≤ 120 ms |
| M3 | Host calls + parking (py-ptc spike) | done | Sync and async host calls, `gather`, park at any safepoint; `mm-cell-worker` runs a harness's py-ptc bootstrap and cells against a local checkout of it, including yields and `Cancel` |
| M4 | Classes and generators | done | Inheritance and C3, `super`, the dunder set, exception subclasses, generators, `yield from`; own subset tests green |
| M5 | Builtins by corpus | done (391/568) | Core types and formatting via tables; CPython tracebacks with carets and hints; ≥ 60% of `cpython`-tagged cases |
| M6 | Own parser + size gate | 3 | `no_std` parser on the same AST, corpus parity with ruff; `hello` ≤ 2.0 MB |
| M7 | Python stdlib pipeline + SDK surface | 2 | Build-time compilation of `mm-stdlib/python/`; `namedtuple(defaults=)`, `collections`, `functools`, `dataclasses`; `Reset`; the SDK bootstrap runs |
| M8 | FULL snapshots | done | Snapshot and restore in a new process; durability tests pass on all passing cases |
| M9 | Stdlib by pass rate | 4–8 | Modules in order of failing cases, native or Python per §12, each PR with its size delta |
| M10 | Parity | — | ≥ 95% of `cpython` cases; all §12 targets; budget met; fuzzers 24 h clean |

### Current execution order after the architecture review

Keep the continuation VM, safe segmented arena, hybrid stdlib and same-build
FULL snapshots. The historical M10 targets above remain targets; neither a
corpus score nor a synthetic speed win establishes useful persistent sessions.
Finish these phases in order, committing only verified boundaries:

1. **Resource lifecycle:** reclaim transient code, bound retained and temporary
   session allocations, reject over-budget loads without accumulating state,
   and bound snapshot/restore. Stable live state must not accumulate historical
   cells; live functions, generators and tracebacks must survive reclamation.
2. **Scheduling:** measure native, compilation, GC and checkpoint latency;
   fix measured stalls while preserving resumability and Python semantics.
3. **Embedding workloads:** persistent cells, tool-result filtering/joins,
   pagination, batching, errors and hibernation. Separate compile, execution,
   host conversion, suspend/resume, checkpoint and restore costs; record exact
   revisions, features, panic strategy, raw timings and memory measurements.
   All established benchmark programs must remain supported.
4. **Selective conformance:** preserve the recovered work and integrate semantic
   fixes separately from module expansion. `decimal` and `unicodedata` remain
   optional, tested/measured enabled and disabled. Time defaults to UTC with
   explicit host-supplied rules. Validate combined gates and the local harness's suites.
5. **Optimization and acceptance:** optimize demonstrated bottlenecks, rerun
   equivalent pinned-engine comparisons and complete the extended fuzz gates.

Build reusable representative workloads during the resource phase. Model-task
evaluation adds completion rate, repair turns, tokens and partial effects once
the deterministic workloads are reliable. SDK preflight checking remains
host-side and before execution; runtime validation does not replace its
"type errors; nothing ran" promise. Capability schemas, retry/effect policy,
checkpoint storage and production harness wiring remain harness responsibilities,
not reasons to expand the engine or block core-engine completion.

**MVP = M1–M5 plus M8.** Acceptance: `mm-cell-worker` runs the py-ptc
presentation (bootstrap, sync and async host calls, `gather`, parking for
yields and `Cancel`) against a local checkout of the harness; FULL snapshots restore in
a new process with durability tests passing on all passing cases; ≥ 60% of
`cpython`-tagged corpus cases pass; every §12 benchmark beats Monty 1.4 on
the same machine. The own parser (M6) and the size gate follow the MVP.

**MVP status (2026-11-01) [M]:** all four acceptance criteria are met.

| Criterion | Result |
|---|---|
| py-ptc against a local harness checkout | `mm-cell-worker` passes 40 of 48 worker-driven protocol tests (`docs/research/plak-integration.md`); bootstrap, sync and async host calls, `gather`, yields and `Cancel` all pass. Failures: missing modules (`os`, `time`, `random`), complex numbers, named-tuple `Record`s, Monty's `<int with N bits>` rendering, Monty's allocator ceiling, and the refused SDK presentation |
| FULL snapshots, durability on all passing cases | 808 of 808 durability runs identical, 20,466 restores in new processes (§13); snapshot sizes and times in §9.3 |
| ≥ 60% of `cpython` cases | 391 of 568 (68.8%); host 50 of 81; monty-specific 1 of 8 |
| every §12 bench beats Monty 1.4 | yes, 0.33–0.93× Monty (§12) |

Since then [M, 2026-11-05]: 45 of 48 protocol tests
and 547 of 568 `cpython` cases (`conformance/results/micro-monty.md`).

Also verified: zero `unsafe`; fuzz targets for arena operations and snapshot
restore (the restore fuzzer found and drove fixes for six reader bugs); every
gate in `cargo xtask check` (fmt, clippy, tests, size, both conformance
checks, durability, bench checksums).

Largest gaps after the MVP, by failing corpus cases: missing modules (107:
`collections`, `itertools`, `dataclasses`, `typing`, `datetime`, `time`,
`functools`, `re`, …, which is M7 and M9), `match` statements (18), complex
numbers (12), `eval`/`exec` (12, out of the subset), frozenset; then the own
parser and the 2.3 MB size gate (M6), and the ≤ 1.2× QuickJS speed bar
(M10).

Build-order rule: no stdlib modules before M7, apart from what M3's host-call
tests need (`asyncio.gather`, `print`).

## 15. Reuse from Monty

Take: the test corpus and the datatest idea; `docs/limitations/*.md` as the
subset list; error messages and "not supported" wording; preflight limit ideas;
semantic reference for builtins, str/bytes methods, formatting, json and
datetime, and scoping corner cases (read, don't copy structure);
`ruff_python_parser` 0.0.14 until M6.

Skip: the `Value` enum, `HeapData` and refcount drop discipline, reader/guard
generics, the cycle collector, serde/minicbor dumps, match-heavy builtins,
fancy-regex/regex-*, `unicode_names2`, decimal, num-bigint,
chrono/jiff/speedate, jiter, the ty checker, monty-proto.

Do not fork Monty: it moves fast (21 commits, +15.8k lines in the 4 days after
1.4.0 [S]) and rewriting its value, heap and GC would touch most of its core.

## 16. Risks and open questions

| Risk | Mitigation |
|---|---|
| Semantic long tail eats the schedule | Corpus-gated milestones; copy CPython algorithms; keep exotic features unsupported |
| Monty closes the language gap | micro-monty's case is pause-anywhere, safety, size and speed; M3 proves the code-mode payoff early |
| Engine bugs in an untrusted sandbox | Zero `unsafe`, bounds-checked arena, fuzzing, snapshot reader bounds checks |
| Bounds checks cost speed | Measure at M2; an `unsafe` exception only with benchmark evidence and a spec change |
| Size creep | §5 rules, CI budget, per-PR deltas |
| ruff is 1.3 MB | Own parser at M6 |
| A harness's SDK keeps its async contract and needs type checking | Sync host calls remove the main need; if a harness keeps `async def` wrappers, run a checker host-side in a separate process, or add a compiler lint for unawaited host calls |
| Python-written stdlib is slow somewhere it matters | Benchmarks per module; move hot modules or functions to Rust (§12) |
| Adapting CPython modules drags in `exec`, metaclasses or C helpers | Rewrite those parts; reject modules whose adaptation costs more than a Rust version |
| Native recursion overflows the Rust stack | Depth counters everywhere (§5) and the nesting fuzz target |
| Cell-protocol drift | Pinned `plakous-cell` revision; `Hello`/`Configure` revision check fails at startup |

Open questions:

- String representation: UTF-8 with a lazy index (default) vs PEP 393. Revisit
  if index-heavy benches regress.
- `SmallInt` width: i48 (default) vs i32 with cheaper checks.
- Unicode names: compressed own table [E 0.2–0.3 MB] or leave `\N{}` unsupported.
- Wasm: test `wasm32-unknown-unknown` with the own parser after M6.
- `enum`: a narrow metaclass hook (`__prepare__`/`__new__` on one known
  metaclass) or a native `Enum`. Decide at M7.
- Cancellation semantics beyond `Cancel` (TaskGroup, `wait_for`): host
  combinators, with the engine delivering cancellation as an exception.

## 17. Constraints for the implementing agent

- Work in this repository on `main`. Commit at verified phase boundaries and
  push to `main` when a phase is complete and verified; no other pushes, PRs,
  issues or messages unless n0x asks.
- Do not modify Monty or the harness behind the cell protocol. Test the
  worker against your own local checkout of it.
- Label every reported number [M], [S] or [E]; commit benchmark and size
  outputs with the jobs that produce them.
- Keep MIT notices for anything taken from Monty (tests, text), QuickJS
  (algorithm ports) or ruff (dependency) in `THIRD_PARTY.md`.

## 18. References

- Monty: <https://github.com/pydantic/monty>, main @ `66fff6bf` (corpus copy),
  release 1.4.0 @ `4dd8e3a`. Corpus `crates/monty/test_cases/`, limitations
  `docs/limitations/`, open PRs #1035 (inheritance) and #1020 (generators).
- plak, the local agent harness behind the cell protocol:
  <https://github.com/N0xMare/plak>, main @ `9ba53187`. Protocol
  `crates/runtimes/cell/src/protocol.rs`; Monty worker
  `crates/runtimes/py-worker/`; runtime contract
  `crates/core/agent/src/local_composition.rs`; spec
  `docs/spec/01-plakous-library.md` §9.4 and §9.9.
- dquickts: <https://github.com/stencil-hq/dquickts> @ `f9a3efa`. README
  "Snapshots" and "How it works"; `js_can_suspend_from` in `quickjs.c`;
  `quickjs-snapshot*.c`.
- QuickJS: <https://bellard.org/quickjs/>, quickjs-ng, rquickjs; libregexp,
  libunicode, `JSCFunctionListEntry`.
- ruff parser: `ruff_python_parser` 0.0.14 on crates.io.
- CPython: PEP 659 (adaptive interpreter, inline caches), the compact ordered
  dict, C3 MRO (<https://www.python.org/download/releases/2.3/mro/>).
- CPython pure-Python stdlib modules (`Lib/_pydatetime.py`, `Lib/heapq.py`,
  `Lib/bisect.py`, `Lib/collections/__init__.py`, `Lib/json/`), PSF license.
- Dispatch: Shi, Gregg, Beatty, Ertl, "Virtual Machine Showdown: Stack Versus
  Registers" (VEE 2005,
  <https://www.usenix.org/legacy/events/vee05/full_papers/p153-yunhe.pdf>;
  TACO 2008); Rohou, Swamy, Seznec, "Branch Prediction and the Performance of
  Interpreters: Don't Trust Folklore" (CGO 2015); V8 Ignition (accumulator
  bytecode).
- Floors: MicroPython, PocketPy.
