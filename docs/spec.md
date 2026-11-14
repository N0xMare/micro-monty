# micro-monty: architecture

Status: released as 0.0.1; this file describes `main`. Owner: n0x.

This is the architecture of micro-monty: how it works, the rules it keeps,
its contracts with hosts and its limits. Read it before changing engine
code. The repository's `AGENTS.md` has the commands and gates;
`docs/user-guide.md` is the embedder's guide.

Labels on every number:

- **[M]** measured (machine and toolchain named where it matters)
- **[S]** sourced from code or docs (linked)
- **[E]** estimate or target, not measured
- **[M, reviewer]** measured by an independent review of the code, not by
  the change's author

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
- **Size and speed:** a stripped `hello.wasm` of about 2.3 MB (§6) and
  interpreter speed at or near QuickJS-ng on calls, closures and loops (§12),
  smaller and faster than Monty.

Monty (pydantic) is the inspiration and a donor, not a foundation: its test
corpus, error wording and subset decisions are reused; its value
representation, heap, refcounting, serde snapshots and protocol are not.

**Compatibility.** micro-monty is published as 0.0.x, with no compatibility
layers and no migrations. An mm-wasm ABI change bumps `ABI_VERSION`, with
`crates/mm-wasm-host` and `hosts/js` changed in the same commit (§6). A
change to what a snapshot holds moves the build id, or bumps
`SNAPSHOT_EPOCH` when a function body changes what an image means and no
hashed item changed; images restore only on a build with the same build id
(§9.3).

## 1. Context, goals, non-goals

### Context

Code-mode harnesses run agents whose action language is code: the model
writes a cell, the harness runs it in a sandbox engine, and every tool is a
host function the cell calls. micro-monty's integration target is plak
(<https://github.com/N0xMare/plak>, MIT), an open-source local agent harness
by micro-monty's author; it shows the usual shape [S]. plak runs cells in
worker processes and talks to them over a cell protocol of length-prefixed
JSON frames; one worker runs JavaScript on QuickJS, another Python on Monty.
Python cells come in two presentations [S]:

- **py-ptc**, after Anthropic's programmatic tool calling (PTC): globals
  persist across cells, tools are async functions the cell awaits, and
  nothing is type-checked.
- **SDK**: every cell starts from a fresh session and is type-checked
  against generated stubs before it runs ("type errors; nothing ran").

The JavaScript worker can stop a running cell and resume it later. Monty
cannot: a Python cell, once started, runs until it ends, so the harness
cannot pause it between observations, cancelling it costs the worker, and
the compute bound is enforced by killing the worker process [S]. Monty's
architecture (a refcounted heap, serde dumps, dunder calls that run to
completion synchronously) would need preemption retrofitted, the way
dquickts retrofitted QuickJS, with the same limits (§9.1). micro-monty's
reason to exist is pause-anywhere, safety, size and speed.

### Goals, in priority order

1. **Correct semantics for the subset.** Where a feature is supported, output
   equals CPython 3.14's. Unsupported features fail at compile time with a clear
   message.
2. **Pause-anywhere** (§9.1) as a hard engine invariant, and FULL snapshots that
   restore into a fresh process running a build with the same build id.
3. **Sandbox safety.** No ambient I/O: every effect is a host call. Memory, fuel,
   time and recursion are bounded. No panic and no undefined behaviour on any
   source; zero `unsafe` in the engine crates.
4. **A full-featured Python for agents:** the language and stdlib that models
   write, errors they can act on, and host calls that look like ordinary
   functions.
5. **Speed:** at or near QuickJS-ng on calls, closures and loops; faster than
   Monty on every benchmark row (§12).
6. **Size:** a small stripped binary whose every byte of growth `cargo
   xtask check` gates (§6).
7. **Portability:** the engine crates are `no_std` + `alloc`.

### Non-goals

- Full CPython compatibility, C extensions, pip packages.
- Threads, `__del__` timing, `__slots__` semantics, `exec`/`eval` of
  arbitrary modules, `weakref` (it would make GC timing observable).
- Filesystem, network, clock or randomness except through host calls.
- **Replay and cross-version resume.** No source is re-evaluated and no tool
  is replayed, and a session reopens only on the same engine identity. A
  snapshot from another build is refused, not migrated (§9.3).
- **Monty protocol compatibility.** The integration points are the Rust SDK,
  the mm-wasm guest and an engine-neutral cell protocol.
- An in-engine type checker. Synchronous host calls remove its main job
  (catching a forgotten `await`); host functions validate their own arguments
  with CPython-style `TypeError`s at the call (§10.1). Pre-run checking is
  the host's, a checker over `Session::stubs()`.
- Mapping snapshots copy-on-write from disk; a FULL restore reads and
  validates the whole image (§9.3).
- Snapshot deltas in the engine; hosts compress images at the storage
  boundary (§9.3).

## 2. Glossary

| Term | Meaning |
|---|---|
| Monty | pydantic's Rust Python-subset interpreter, MIT. Source of the conformance corpus and error wording |
| Harness | The host program that runs an agent's cells and owns its tools; it embeds micro-monty |
| plak | An open-source (MIT) local agent harness by micro-monty's author; the integration target (§1, §14) |
| Cell protocol | Frames a harness and a worker process exchange to run cells. `mm-cell-worker` implements plak's, `plakous-cell-protocol.v2` (`HostFrame`/`WorkerFrame` JSON frames, §10.2) |
| py-ptc | Programmatic tool calling in Python: globals persist across cells, tools are async functions, no type checking (§1) |
| SDK | A code-mode presentation in which every cell starts fresh and is type-checked against generated stubs before it runs (§1). Not the Rust SDK, which is the facade |
| dquickts | A QuickJS fork with suspension and full/delta snapshots; the comparison point for §9.1 |
| Safepoint | A bytecode boundary where GC, parking and snapshots may happen: backward jumps, calls and the `CHECK`s the compiler puts in straight-line code; returns charge no tick and check nothing (§8.6) |
| Pause-anywhere | Any safepoint at any depth can stop execution with all state in the arena (§9.1) |
| Park | Stop at a safepoint because the host asked (a yield window, `Cancel`, a preempt) or fuel ran out, rather than for a host call |
| Fuel | An instruction budget charged at safepoints and by natives in proportion to their work |
| Native step | The Rust work a native does between two safepoints (§5, §5.1) |
| Prelude | `mm-engine/src/prelude.py`: Python twins of builtins and VM operations that natives hand off to (§5, §8.2) |
| Arena | The session heap: a word space and a byte space of segments (§7.2) |
| Ref | A `u32` word offset into the arena. Values never hold pointers |
| FULL snapshot | Both arena spaces plus VM registers and a header (§9.3) |
| build_id | Hash of what an image's meaning rests on (§9.3 "Compatibility"). Snapshots load only on a match |
| Host value | The plain data that crosses the host boundary, copied by value (§10) |
| Conformance corpus | Monty's `test_cases/*.py`, copied into `conformance/monty/` |

## 3. Why QuickJS is small and Monty is not

QuickJS is about 1.3 MB engine-only in rquickjs [M]: hand-written C with
almost no generics, builtins as data tables (`JSCFunctionListEntry`) sharing one
calling convention, its own cut-down regex, unicode and bignum, no unwinding,
compact values.

Monty is 10.2 MB stripped at opt 3 [M, Monty 1.4]. The size comes from
generic monomorphization (about 2.35 MB of duplicate instances before LTO),
general crates (the regex stack about 774 KiB, `unicode_names2` about
0.86 MB, decimal, num-bigint, chrono/jiff/speedate), panic/unwind/fmt
metadata (about 0.7 MB of `.eh_frame`), builtins written as large `match`
dispatch, serde derives on the heap (about 24% of its LLVM IR), and stdlib
that can't be gated because heap enum variants keep every type alive [M].

micro-monty takes the QuickJS side of each of those (§5, §6). Rust stays the
language: Rust hosts embed it in process, memory safety matters for an
untrusted sandbox, and size comes from the rules in §5, not from the language.

## 4. Language subset (Python 3.14)

Why 3.14: Monty's corpus is written against CPython 3.14, so it is the oracle
that makes the corpus usable (on 3.14.7, 645 of 649 non-`monty-specific`
cases pass; on 3.12.14, 443 [M]), and 3.14's error messages and tracebacks
are the most helpful to models (§8.4). 3.14 syntax additions are either
rejected (t-strings) or need no evaluation (annotations are parsed and never
evaluated, matching deferred annotations).

- Literals: `int` (arbitrary precision), `float`, `complex`, `str`, `bytes`,
  `bytearray` (§7.4), f-strings, `list`, `tuple`, `dict`, `set`, `None`,
  `True`, `False`, `Ellipsis`.
- Functions: defaults, keyword-only, `*args`, `**kwargs`, closures,
  `nonlocal`, `global`, lambdas, decorators.
- Control flow: `if`/`elif`/`else`, `while`/`for` with `else`,
  `break`/`continue`, `try`/`except`/`else`/`finally`, `raise ... from`, `with`,
  `assert`, `del`, walrus, conditional expressions, chained comparisons,
  `match` (PEP 634).
- Comprehensions and lazy generator expressions.
- `async def`, `await`, top-level `await` in cells, `asyncio.gather`/`run`;
  `asyncio.create_task`, `Task`/`Future` `done`/`result`/`exception`/
  `cancel`/`cancelled`, `CancelledError`, `wait_for`, `as_completed`,
  `Semaphore`/`BoundedSemaphore`, `Lock`, `Event`, `Condition`, the queues,
  `TaskGroup`, `timeout`/`timeout_at`, `shield`, `wait`, the running loop's
  callbacks and timers, task names and done callbacks, `ExceptionGroup`,
  `Barrier`, `Runner`, `new_event_loop`/`set_event_loop` with
  `run_until_complete`, the asyncio submodules, per-task `contextvars`,
  the never-retrieved report and `KeyboardInterrupt`/`SystemExit` leaving
  `asyncio.run` (§8.5; `conformance/own/369`, `381`, `382`, `388`, `389`).
- `async for`, `async with` and async comprehensions (allowed in module
  code too, as top-level `await` is), `__aiter__`/`__anext__`,
  `__aenter__`/`__aexit__`, async generators (`asend`, `athrow`, `aclose`,
  async generator expressions) and `aiter`/`anext`, with CPython's
  messages (`conformance/own/322_async_iter_with.py`). An async generator
  runs as a generator frame on the awaiting task's stack, so it parks and
  snapshots like the rest. As with generators, nothing finalizes one that
  is abandoned unfinished: `asyncio.run` does not `aclose()` it at the end,
  and `anext(it, default)` is a `coroutine`, not an `anext_awaitable`.
- Classes: class bodies, methods, `property`, `staticmethod`, `classmethod`,
  single and multiple inheritance with C3 MRO, `super()`, exception subclasses,
  class decorators, and the core dunder set (`__init__`, simple `__new__`,
  `__repr__`, `__str__`, comparisons, arithmetic and reflected forms with
  `NotImplemented`, `__bool__`, `__len__`, `__iter__`, `__next__`,
  `__contains__`, item access, `__call__`, `__enter__`/`__exit__`,
  `__getattr__`/`__setattr__`, `__init_subclass__`, `__set_name__`).
- Generators: `yield`, `yield from`, `send`, `close`, `throw`.
- `typing` as runtime no-ops; `enum` in Python over the metaclass support.
  `except*` (`conformance/own/387`): the compiler keeps a state list
  `[orig, rest, raised...]`; opcode `EXC_STAR` calls the prelude's
  `_star_match` per clause and `_star_prep` (CPython's
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
  through `__import__` (CPython's signature and errors;
  `conformance/own/425`). Wildcard imports at module level bind the public
  names, or `__all__` when the module has one
  (`conformance/own/312_wildcard_imports.py`); elsewhere they are a
  `SyntaxError`, as in CPython. No relative imports.
- Bundled stdlib (§12): the default build carries the modules agent
  code most often imports: `contextlib` (no `chdir`), `traceback`
  (CPython's `TracebackException`, `StackSummary` and `FrameSummary` over
  the engine's frames, with CPython's "Did you mean" rules: a `NameError`
  hint needs a traceback, `AttributeError` and `ImportError` hints do not;
  no colors, no stack-walking functions (`extract_stack`, `format_stack`,
  `print_stack`, `walk_stack`), no keyword-typo hints for SyntaxErrors), a
  slim `logging` (no `logging.handlers`/`config`, files or frame data:
  records carry CPython's unknown-caller values), `urllib.parse`, `pprint`,
  `shlex`, `difflib` (no `HtmlDiff` or `diff_bytes`), `struct` (native;
  `pack_into` writes into a `bytearray`, §7.4), `html` (`html.entities` is
  its own feature, `mod-html-entities`) and `warnings` (filters,
  `catch_warnings`, `deprecated`; with no frame objects, `warn` finds the
  frame CPython's `_warnings` would from the VM's frame chain
  (`_mm.caller`: filename, line of the call, globals for the module and
  `__warningregistry__`; `stacklevel` and `skip_file_prefixes` as
  CPython's, prelude frames skipped), and `<sys>` line 0 in module `sys`
  off the stack). The builtins include CPython's `Warning` hierarchy.
  `codecs` (Python) has the codec and error-handler registries over the
  codecs `str.encode` implements; search functions and error handlers
  registered from Python are seen by `codecs.lookup`/`encode`/`decode` and
  `lookup_error` only, not by `str.encode`/`bytes.decode`; no incremental
  or stream codecs. `platform` (Python) answers with fixed sandbox values
  that reveal nothing about the host (`system()` is `'Linux'` like
  `sys.platform`, node, release, version and machine are empty,
  `python_implementation()` is `'micro-monty'`); the version queries read
  `sys.version_info`. `sys.path` is an empty list: imports never search it
  (`conformance/own/325`–`326`). `tempfile` exists so that modules
  importing it load; everything in it that would touch the filesystem
  raises the sandbox's `PermissionError`, as `open()` does
  (`conformance/own/347`). The bundled `re` and `datetime` emit CPython's
  `DeprecationWarning`s and `FutureWarning`s (`conformance/own/348`).
  Exceptions have traceback objects (`__traceback__`, `tb_next`,
  `tb_lineno`, `with_traceback`), and `sys.exception()`, `sys.exc_info()`
  and a replaceable `sys.stdout`/`stderr` for `print` exist for them
  (`conformance/own/313`–`321`).
- `globals()`: a read-only view of the calling module's globals. `[]`,
  `in`, `get`, `keys`/`values`/`items`, iteration, `len` and `==` behave as
  on CPython's dict and see names bound later; every write (item
  assignment, `del`, `update`, `setdefault`, `pop`, `popitem`, `clear`,
  `|=`) raises `TypeError`, so code that relies on writing through
  `globals()` is refused rather than silently diverging
  (`conformance/own/373`). There is no writable `globals()` or `locals()`.
- Code-mode stdlib: `json.loads` takes all five hooks (with one, the
  native scanner keeps number and constant literals as text and objects as
  flat pairs, and a prelude pass calls the hooks children first in document
  order, CPython's order; `conformance/own/364`); `html.parser` (CPython's,
  with `_markupbase`), `calendar` (no `locale`: the C locale only; no
  command line), `hmac` (CPython's pure-Python path over `hashlib`;
  `compare_digest` is not constant-time), `zlib` with only `crc32` and
  `adler32` (`compress`, `decompress` and the rest raise
  `AttributeError`), the `utf-8-sig` codec, and `zoneinfo`: CPython's
  pure-Python TZif reader over TZif bytes the host serves by key
  (`Session::set_zone_source`). Without a source only `ZoneInfo('UTC')`
  loads and other keys raise `ZoneInfoNotFoundError` naming the missing
  data; `available_timezones()` raises `NotImplementedError` (no zone
  listing). Loaded zones are engine state, so a snapshot keeps their rules
  and a restored session answers them without the source
  (`conformance/own/365`–`368`, `372`). `ast` (opt-in, `mod-ast`; not in
  the default build, where `import ast` raises `ModuleNotFoundError` with
  the "not enabled in this build" hint) builds CPython's node classes from
  the own parser's tree (`parse` in `exec`, `eval` and `single` modes,
  `dump`, `walk`, `NodeVisitor`, `NodeTransformer`, `literal_eval`,
  `get_docstring`, `get_source_segment` and the location helpers; no
  `unparse`, no `compile()` of a tree; type parameters and `\N{...}` stay
  compile errors), and `tokenize` (with `token`) yields CPython's
  `generate_tokens`/`tokenize` stream from the own lexer (no `untokenize`
  or `open`; `0777` is a tokenizer error where CPython's `tokenize` module
  yields a token) (`conformance/own/392`, `393`). `json.dumps` encodes
  instances of `str`, `int`, `float`, `list`, `tuple` and `dict` subclasses
  (`IntEnum`, `StrEnum`, `OrderedDict`, `defaultdict`, `Counter`) as the
  values they extend, natively; a `dict` subclass with its own `items` and
  every error (with CPython's `__notes__`) go through the prelude's Python
  encoder. `JSONEncoder`/`JSONDecoder` (`cls=`, `raw_decode`,
  `strict=False`) and the `json.encoder`/`json.decoder` submodules exist
  (`conformance/own/400`). `sys.set_int_max_str_digits`/
  `get_int_max_str_digits` and `sys.int_info` move the 4300-digit limit of
  int/str conversion, also across a snapshot (`409`).

Rejected at compile time with "micro-monty does not support X": `__slots__`
semantics, `__del__`, a user `__getattribute__` (it would cost a check on
the attribute fast path), t-strings, type parameters and `type` aliases,
named Unicode escapes (`\N{...}`), threads, `exec`/`eval` of modules,
imports of anything not bundled or host-registered, C extensions. A missing
import keeps CPython's `No module named 'x'` and the report adds one
sentence per group (data libraries, network, processes, filesystem, time
zones, ...; modules a build leaves out say so), which the host can replace
or drop per group (`Session::set_hint`; `docs/user-guide.md`, "Messages for
models"). `memoryview` is not supported (§7.4).

Source of truth for subset decisions: Monty's `docs/limitations/*.md` and error
messages; copy wording where behaviour matches.

## 5. Engineering rules

- **No `unsafe`.** The workspace sets `unsafe_code = "forbid"`. A crate that
  needs an exception overrides `[lints]` in its own manifest and the
  exception is recorded here first, only with evidence. The exceptions:
  - **Worker:** `mm-cell-worker/src/seccomp.rs` may use FFI to install
    Linux's seccomp filter (`prctl`, `seccomp`) and read `getrusage`. Its
    pointers refer only to initialized, owned structures that outlive the
    syscall; the kernel copies the BPF program during installation. The
    worker denies `unsafe` elsewhere. The subprocess seccomp test proves
    allowed work survives and a forbidden syscall terminates the process;
    worker integration runs with the filter.
  - **Wasm guest:** `crates/mm-wasm-guest` may use `unsafe` syntax in
    exactly two places, its `mm_host` import block (`unsafe extern "C"`
    with `safe fn` items: the host's clock, park flag, wall clock, entropy,
    output and zone source, and for the bundle guest `bundle_read`) and the
    `#[unsafe(no_mangle)]` attributes of its `mm-wasm` exports. Each is a
    module-level `#[allow(unsafe_code)]`; the crate denies `unsafe`
    elsewhere, has no `unsafe` blocks, and reads and writes its input and
    output buffers as safe `Vec<u8>`s. Pointers passed to imports are to
    live, owned buffers of the stated length. `crates/mm-wasm-host`, outside
    the workspace, has one `unsafe` call (wasmtime's `Module::deserialize`
    of a module it precompiled itself, cached under a content key); it is
    not part of the shipped engine.
  - **Wasm allocator** (temporary): `crates/mm-wasm-alloc` may implement
    `GlobalAlloc` and `Sync` for one `UnsafeCell<dlmalloc::Dlmalloc>`. It
    forwards to the same dlmalloc version std uses on
    `wasm32-unknown-unknown`, with `dealloc`/`realloc` through
    `c_free`/`c_realloc`, so std's `validate_size` assertions are not
    inlined into every deallocation site (§6). It is built only for that
    target, and `Sync` only without `atomics`. Only binaries register it
    (the guest and the facade's `hello` and `roundtrip` examples), never the
    facade; for embedders it is optional (`docs/user-guide.md`, "Embedding
    guarantees"). Its `realloc` example, run under wasmtime by `cargo xtask
    wasm`, checks over-aligned `realloc`. When a pinned stable toolchain
    ships dlmalloc with that check as a debug assertion, the crate and this
    exception go.
  - **Fuzz allocator:** the fuzz crate (`fuzz/`, its own workspace, never
    shipped or published) may implement `GlobalAlloc` in
    `fuzz/fuzz_targets/memory_meter.rs`: a counting allocator that forwards
    every call to `System` unchanged and keeps the current and peak heap in
    atomics. Only test binaries register it (the `memory_bound` fuzz target
    and the `memory_probes` replay, §11).

  None of these permits `unsafe` blocks in an engine crate, a custom
  allocator in the facade, or a change to any engine crate's forbid lint.
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
  and dict lookups (§8.2).
- **Every Rust recursion is depth-bounded.** Pause-anywhere covers Python
  recursion, not native recursion over syntax or data. The tokenizer,
  parser, compiler, and `repr`/`str`/`==`/`hash`/`copy`/`json` on nested
  containers are iterative or carry an explicit depth counter. Exceeding it
  raises `RecursionError` (or CPython's `SyntaxError: too many nested
  parentheses` / "too many statically nested blocks" at compile time),
  never a Rust stack overflow, which would abort the process. A fuzz target
  feeds deeply nested source and data. At run time nesting lives in the
  arena: comparison walks (`walk.rs`), `repr`, `json`, `copy` and
  host-value conversion keep explicit stacks, so a session that does not
  compile runs on a 256 KiB host thread and under wasmtime's default stack.
  Only the parser and compiler recurse natively, bounded by `MAX_NESTING`
  (200); at the limit they need about 650 KiB in release builds [M], also
  inside a session (`Session::compile`, `ast.parse` with `mod-ast`;
  `docs/user-guide.md`, "Embedding guarantees"). The mm-wasm guest links an
  832 KiB shadow stack (`crates/mm-wasm-guest/build.rs`): its deepest legal
  input, nested lambda defaults at the limit, takes 670 KB [M], every own
  case, corpus file and bench program at most 41 KB, and
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
  `Session::fuel_wanted`), and the next `resume` runs it (only after such a
  park: after any other, the next one still checks). `factorial`'s product
  tree is charged at Karatsuba's cost (`math::product_cost`: 9–10 ns per
  unit from n = 50,000 to 800,000, where a loop iteration is about 10 [M]).
  Superlinear int work estimated past 45 ms (`intops::WORK_MAX`) and ints
  over 2^26 bits (`intops::MAX_BITS`) raise `MemoryError` before starting,
  whatever the fuel and without a fuel park (§11).
- **Sorting** estimates n·⌈log₂ n⌉ visits. A sort of more than 2^17
  estimated comparisons (`builtins::sort_is_long`) keeps its two arrays and
  position in a state list and merges at most `SORT_SLICE` (2^17) visits,
  or copies at most 2^17 items, per native step (`Vm::sort_more`), the rest
  in the prelude's `_sort_rest` a slice per loop step, so a deadline or
  request is honoured inside it. Its first step allocates the two arrays
  and reads the input in place: `sorted` of a list without `key=` runs no
  Python code and never writes it, and `list.sort` moves the list's buffer,
  not its items (`Vm::list_move`). Pass 0 fills the first array with runs
  of 1,024 items (`SORT_RUN_BITS`), each sorted, read reversed for
  `reverse=True`; bottom-up merge passes follow, in the same stable order
  (a mixed-type list may fail on another pair first). A last pass reverses
  the merged array (`reverse=True`) or maps indexes to items (`key=`), a
  slice per step. Keys that are all small ints, all strs or all floats
  compare directly (CPython's pre-checked compares), and two runs already in
  order take one comparison while that pays. 1M items, best of 5 [M]: strs
  676 ms (CPython 270), sorted strs 130, ints 112, tuples 812, `key=len`
  247. A short sort, or a long sort's runs in one step, spends at most
  `SORT_CAP` (2^20) visits comparing str and other keys; past it the
  prelude's `_sort_keys` sorts the rest, a comparison per safepoint, so keys
  that compare long cannot hold a step. json's `sort_keys` is not capped: it
  sorts up to 4,096 keys natively, bounded by its 64 MiB text cap (21 ms at
  4,000 keys of 12 KB [M]), and leaves more to `list.sort`.
- **Comparison walks are budgeted.** Native `==`, ordering and key probes
  over builtin data visit at most `WALK` (2^14) pairs per call and charge
  one fuel unit per visit. Past the budget they hand off to the prelude
  twin (`_op_eq`, `_op_cmp`, `_sfind`) exactly as for a user `__eq__`; the
  twin descends one level in bytecode and retries each item natively, with
  `_seq_scan`/`_dict_scan` comparing long sequences and dicts in native
  slices. Shared data with exponentially many paths (`x = [x, 1, x]`, slow
  on CPython too) therefore parks on fuel, deadlines and requests. The
  budget is a constant, so parked and unparked runs take the same path. A
  set insert spends one walk for all its compares, as a lookup does. A set's
  or dict's table rebuild charges the probes past each key's first run of
  slots, beyond one a key, and ends a step when the slots read and those
  probes pass its budget, so a table of colliding keys rebuilds in steps.
  Depth carries through set and dict probes, so nested frozensets and keys
  raise `RecursionError`. It also carries across the levels a twin runs in
  bytecode: the twins compare items with `_walk_eq`/`_walk_ord` at their
  nesting `d` rather than with `==`/`<`, so the native depth limit holds
  for the whole comparison. A walk started at nesting `d` gets
  `max(WALK >> d, 1)` visits, so re-walking what the twin then descends
  into costs about 2 × `WALK` plus one visit per level. Ordering finds the
  first unequal pair in one descent (`_ord`); a pair that needs Python
  code falls back to CPython's `==`-then-`<` steps.
- **Membership scans charge per item.** `in` on a list, tuple or
  `dict.values()` (or a range, for an item that is not an int), and
  `count`/`index` on lists and tuples and `list.remove`, charge one fuel
  unit per item compared. Over a long list or tuple they go in chunked
  steps (`nstep::op::SEQ_IN`, `SEQ_SCAN`; "Bounded native steps", below).
  `in` on `dict.values()` collects the items with `Vm::collect`
  (`Vm::contains`), which hands off past `COLLECT_CHUNK` items as below;
  a range of fewer than `COLLECT_CHUNK` items does the same for an item
  that is not an int, and a longer one hands off to the twin at once,
  which compares item by item. An int past 64 bits is in no range (a
  range's bounds fit 64 bits). `in` on an iterator, which
  need not end, scans `WALK` items natively and then hands off to
  `_op_contains`, which goes on from the same position in bytecode and
  parks on fuel, deadlines and requests. `any` and `all` do the same
  (`_any_rest`, `_all_rest`) on an iterator or range, and at the first item
  with a user `__bool__` or `__len__`.
- **Collecting an iterator parks between chunks.** `Vm::collect` copies a
  list or tuple whole in one native step, one fuel unit per item. Anything
  else, containers and their views included, it gathers through the
  iterator: a container's items can be many, a range (whose length is
  checked against the memory limit first) long, and any other native
  iterator (zip, enumerate, itertools; a generator, `map` and `filter`
  always run in the twin) need not end, so after `COLLECT_CHUNK` (2^16)
  items `collect` keeps what it took and hands off, and every caller must
  be able to continue in its twin (an 8M range: 10–11 ms a step [M]). A
  range object goes on from a new iterator at the items taken, and a
  range's list is sized up front. `list` and `tuple` of a long
  range of small ints build their result from the range in place, a slice
  per step (`Vm::range_seq`). `Vm::slow_call` replaces the iterator in the
  twin's arguments with a list of the items and calls
  `_collect_rest(xs, it, f, args, kw)` instead, which gathers the rest a
  chunk per loop step with `_more` (each a safepoint) and then calls `f`,
  the native itself or the operator's twin, on the complete list. An item
  that needs Python code part way keeps the items before it the same way.
  Every caller goes through this: `list`, `tuple`, `set`, `frozenset`,
  `dict` (`fromkeys`, pairs from `zip`), `sorted`, `str.join`,
  `bytes`/`bytearray`, `min`/`max`/`sum`, `list.extend`, `+=`, unpacking
  (`*a`, `a, *b = it`, `f(*it)`), the `math` sums and products, slice
  assignment and `e.args = it`. A dict-update pair that is itself an
  iterator goes to the twin before any of it is taken.
  `list(enumerate(range(10**18)))` parks within a fuel slice, a deadline or
  a request, and ends in `MemoryError` at the memory limit.
- **Bounded native steps.** On every site
  `crates/micro-monty/tests/steps.rs` lists as `Bounded`, no native step
  runs longer than about 50 ms between two safepoints at the default
  limits, so a deadline, a park request or mm-wasm's epoch grace (100 ms
  plus 0.75 ms per MiB of the memory limit) waits at most that long on
  one. It is not an invariant: full collections (§5.1 clause 2) and sites
  no row covers can run longer.

  A native whose work grows with its data runs its first step itself,
  allocating nothing while the work is short, and stops after
  `nstep::STEP_WORK` (2^17) fuel units of it (`Vm::work` counts every unit
  `burn` charges, fuel limit or not). It then keeps its progress in an
  `NSTATE` object, raw word 0 naming the operation (`nstep::op`,
  `nstep::STEPS`, part of the build id), and requests `_steps(state)`,
  which calls `_more(state)` until it gives something other than `state`
  back: each step is a call, a safepoint, and a park or snapshot between
  two steps sees a prelude frame and a heap object, so the snapshot format
  does not change (`Vm::chunk_rest`, `Vm::chunk_step`). A step that meets
  work needing Python code hands the rest to a prelude twin from where it
  is. `x in s` over a list or tuple is the model (`nstep::op::SEQ_IN`,
  twin `_seq_has`): 10M items take about 7 ms a step [M]. A chunk that
  builds a guest-sized Rust buffer admits it against the memory limit
  first (§11).

  No object is bigger than 64 MiB (`mm_core::MAX_OBJECT_BYTES`, §11), so a
  native whose work over one object stays under 50 ms at that size (a copy,
  a fill) is one step and needs no chunking. The chunked operations:

  - **Tables growing an insert at a time.** Once a full table holds 262,144
    live entries (`set::GROW_STEPS`), the insert that needs room hands the
    growth to `BULK` (`Vm::bulk_grow_top`), as `update` grows a table: the
    new tables a step each, the old entries a slice per step, then the
    item, in CPython's order. Only callers that can hand a request on do so
    (the store opcodes, comprehensions, `dict.__setitem__`, `setdefault`,
    `set.add`); a key hashed in Python stores through the prelude twin,
    whose growth is one step, as is any native's own insert. `SET_ADD`
    steps from its out-of-line `set_add_inner`, leaving what
    `Vm::dispatch` inlines unchanged (every shape that changed it moved
    wasmtime's loop bench 13–16% [M]). It grows in steps only while a 32nd
    of the limit is left beside the new tables (`Vm::grow_in_steps`):
    between two steps a collection may begin an incremental mark that holds
    all garbage until its remark, and near the limit that fails
    allocations the one-step growth survives. A chunked native that fills
    a dict of its own moves it a slice a step with `Vm::dict_grow_slice`.
  - **Numbers.** `int()`, `float()` and `complex()` read a long text 2 MB
    (a non-ASCII char counting 32) per step in one chunked text engine
    (`op::NUM_TEXT`: one char loop and step, a grammar per op), keeping a
    few words of state: `float`/`complex` reduce it to a short canonical
    text (800 significant digits and a sticky one, one exponent), `int`
    its digit range, and a long `int()` text's digits are then converted
    into the result's block a piece per step. A failing `int()` shows
    CPython's `%.200R` of its first 800 bytes, its quote chosen from the
    whole text; a failing `float()` or `complex()` past 64 KB raises in
    `_num_error`. The `math` folds (`fsum`, `prod`, `sumprod`, `dist`,
    `hypot`) go a slice per step (`op::MATH_FOLD`). Under the object cap
    two stay one step: `float.fromhex` reads a word at a time (38 ms at
    64 MiB [M]) and `int.to_bytes` fills its result in the arena (34 ms at
    64 MiB [M]). A big int's power-of-two digits (`bin`, `oct`, `hex`,
    `format` and f-strings with `b o x X`, `_` groups, sign, prefix and
    padding; `%x %X %o` past a million digits through `_percent`'s
    `format`) are written from the end of their str's block 2M digits a
    step (1.5–2.2 ms a step at the int cap [M]). A number padded to a width
    past 8 MB is formatted unpadded, then padded and zero-filled, grouped
    as CPython's, by `STR_FILL`. A lone f-string field is its formatted
    str: the compiler emits no `BUILD_STRING` for it, as CPython 3.14
    emits only `FORMAT_SIMPLE`/`FORMAT_WITH_SPEC`.
  - **json.** `loads` copies a window of input a step
    (`nstep::op::JSON_SCAN`), the containers still open kept between steps
    as arena containers of their items so far, a step's items joining them
    at its end. A step ends before its items could fill an open object's
    table of 2^16 entries or more, or before a child closes into such an
    object that is nearly full, and the next steps grow the table while
    nothing writes to the object (`Vm::dict_grow_slice`; at worst 25 ms
    for 2M keys [M]). A string, number or whitespace run too long for a
    window is read in the arena a slice a step and written into a byte
    block of its final size. `dumps` (`op::JSON_DUMP`) keeps its open
    containers between steps and appends each step's text to one byte
    block that becomes the result (`Heap::str_from_block`, no parts and no
    join). Once large the block has a segment of its own whose room is
    reserved ahead, unfilled, and admitted against the limit first
    (`Heap::append_bytes`); the room grows by what the text needs, at most
    16 MiB at once, in a step of its own when the last step's text would
    not fit, and the result gives the rest back. Growing in place may copy
    the whole block in one step (glibc after a large free, dlmalloc on
    wasm: 32–41 ms for 53 MiB on wasmtime [M]), so a text of 4 MiB or more
    grows by moving to a new block 4 MiB a step instead, and falls back to
    growing in place only when the limit cannot admit the new block's
    spare room. Either way the old text and the new room are admitted
    together (§9.2). A text past 64 MiB is a `MemoryError`. `dumps` writes
    a str over 4 KiB a slice a step and leaves the keys of a dict over
    4,096 entries under `sort_keys` to the prelude's `list.sort`.
  - **unicodedata, ast, tokenize.** `unicodedata.normalize` and
    `is_normalized` (`op::UNI_NORM`) take a long str a slice a step, each
    slice ending before a char whose decomposition starts with a starter
    that is no composition's second, so the slices normalize
    independently, and a step ends before a slice that would take it past
    `STEP_WORK`. A run without such a char is normalized in one step, so it
    is capped at 65,536 chars after decomposition (10–15 ms, at most 2^21
    units [M]); past it, `MemoryError`. `ast.parse` and `tokenize` lex,
    parse and build in one step, so their source is capped: 32 KiB for
    `ast.parse` (the worst shape found, a statement every two bytes,
    29–38 ms) and 64 KiB for `tokenize` (a token a byte, about 22 ms) [M];
    a longer source raises `MemoryError`.

  `crates/micro-monty/tests/steps.rs` is the oracle: each row runs a site
  near the default limits with a deadline set again after every park, and
  fails if a `Bounded` row's longest gap between two safepoints passes
  50 ms on the reference machine, scaled to the one running it (§5.1
  clause 2; release; `cargo xtask check` runs it alone). A row marked
  `Open` is reported without failing; none is. Fuel is bounded the same
  way: on slices of 1,000 units, no slice of a bounded row charges more
  than 2^21 + 2^17 units past what it was given (a chunked step charges
  about `STEP_WORK`; one unchunked operation over one object at the cap
  reads at most 2^20 units' worth and builds as much), unless the
  operation parked first with `fuel_wanted` and the slice gave that. The
  `run_source` fuzz target searches for sites no row knows: each input
  also runs with a 1 ms deadline set again after every park, and a start
  or resume that overshoots it by more than `MM_FUZZ_STEP_MS` (1,000 ms;
  the instrumented build runs native steps 2 to 15 times slower than
  release [M]) twice in a row is a finding. `MM_STEP_MS=N mm run FILE`
  runs a program the same way in release and exits with status 3 when a
  start or resume runs more than N ms past its deadline, so difffuzz and
  the soak check the bound too.
- **Results and byte work are charged where they are built.** The arena
  constructors charge one unit per 64 bytes of a str, bytes or big-int
  result and one per 8 slots of a tuple, list, dict or set table
  (`Vm::burn_result`), so every native that builds something large pays for
  it, whatever path got there. Scans over text (str and bytes methods, `in`,
  hashing bytes, `re`'s literal skip-ahead, comparing long texts or big ints
  inside a walk (a heap int compares in place: sign, length, then from the
  top; as long a pair spends one visit and unit per 64 bytes), where one
  visit covers 64 bytes and any excess over the budget is charged with the
  walk) charge one unit per 64 bytes they read. Collecting a list or tuple
  into a native charges one unit per item, as collecting an iterator does;
  set and dict algebra and equality charge one visit per entry, identity
  hits included. `hash` of a tuple visits at most `WALK` tuples natively,
  its big ints spending a visit (and a unit) per 32 bytes they hash, then
  hands off to `_hash`, which hashes each tuple once, so shared nesting (`t
  = (t, t)`) is linear.
- **One native step stays linear.** A class's subclasses are revisited
  once (version stamps, not a `seen` list), a single-base class takes its
  base's dunder flags and its own, the C3 merge counts tail occurrences, a
  column on a non-ASCII line counts at most 256 bytes from a checkpoint
  (`Lines`), a traceback splits each source once, and `strptime` counts the
  whitespace it rescans as steps. Work counted as one step is counted as
  what it reads: a `re` set a step per 16 words, clearing the group
  registers a step per 16, a shape's transitions a unit per 8. A container
  `repr` walks at most `REPR_SLICE` (2^17) visits natively, with or
  without a fuel limit, before the twin goes on.
- **Allocations outside the arena are bounded too.** The compiler's AST and
  bytecode buffers count against the source limits; natives build results in the
  arena, not in unbounded Rust `Vec`s (§11).
- **Generics:** none across crate APIs; inside a crate only when monomorphized
  once or twice. Prefer `&mut dyn Write` and `&dyn Trait` on cold paths.
  The private admitted-buffer `NativeVec<T>` also serves JSON's encoder and
  decoder frames and renderer work items alongside `Value`, sharing
  allocation and charge lifetimes.
- **No serde in the engine crates.** Snapshot and host-value encodings are
  hand-written. (`mm-cell-worker` uses serde through plak's protocol crate,
  `plakous-cell`.)
- **Features gate size.** Each stdlib module is a cargo feature that removes its
  types, not just its import (§12).
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
  backtracking regex with fuel, bigint, unicode tables `core` lacks, xxh64
  for snapshot checksums. A crate is allowed only when it beats the
  hand-rolled size in the size budget.
- `random` uses MT19937, so seeded sequences match CPython.

### 5.1 The bounded-work contract for natives

What every native (builtin, operator, method or module function) whose
work a guest can make large promises the host. The mechanisms that meet it
are the rules above. Where the code falls short, the clause says so under
**Today**.

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
   bytecode, up to 0.48 µs for the natives charged most cheaply [M]. The
   50 ms bound is a native number and does not carry over to V8 by itself:
   there a step is short only when its units are few, which the chunked
   steps' `STEP_WORK` cap gives (2^17 units, about 4–63 ms) and an
   unchunked operation over an object at the cap does not (up to about
   2^21 units, about 1 s at 0.48 µs).

   **Today:** the native bound is measured on every row of
   `crates/micro-monty/tests/steps.rs`, and every row is `Bounded`. The
   allocation floor is closed: a large list buffer, tuple or set key table
   comes unfilled from a fresh zeroed segment (`Heap::alloc_unfilled`), so
   its pages fault in as the steps write it; a set's keys are marked empty a
   slice per step without the write barrier, and its hashes stay unfilled
   and fault in at random as the first inserts probe them (24–26 ms for 32
   MiB [M]; writing them first in steps of their own delayed the rebuild, so
   the old 32 MiB table stayed live through a collection and two rounds of
   656k-item `a |= b` raised `MemoryError` at 256 MiB [M]); a sort's second
   merge array comes in a step of its own; and a full list of 1M items or
   more grows a slice per step on `append` and `insert`. The 50 ms is on a
   reference machine (16 cores under gVisor); the oracle scales it by the
   machine it runs on, so a row's verdict means the same on every Linux
   machine and on a developer's Mac. Three probes, timed before and after
   the rows (the faster run counts, as load only slows a probe): allocating
   and filling a fresh 64 MiB block, the cost of a step that allocates an
   object at the cap; four chunked `STEP_WORK` steps of `x in list`, the
   cost of native work; and a million dependent loads from random slots of a
   64 MiB table, the memory latency that bounds a mark or a probe over a big
   table (rows took twice as long on a machine with 144 ns loads as on one
   with 64 ns [M]). The limit is 50 ms times the largest ratio of a probe's
   time to the reference's (28.1 ms, 11.6 ms and 144 ms [M]), clamped to
   0.5–2; a machine past 2 fails the oracle as unable to judge it. A
   `Bounded` row over that limit runs again after every other row has run,
   once a round for up to six rounds, so its retries spread over the host's
   slow phases (1.6–2 times, outlasting three runs back to back [M]). A
   probe runs between retries, each retry is judged against the limit from
   the faster probe beside it, and the row passes if any run is under its
   limit: a load spike only lengthens a step.

   A collection the heap starts itself is not a native step: over 32 MiB
   live it marks a slice per safepoint, and under it it runs in full over
   at most about 64 MiB (30 ms native at worst [M]; §9.2). A park that is
   due goes ahead of a collection step, which runs when the session
   resumes, so one gap never holds both a native's step and a collection
   step (§9.2). Full collections over a large heap stay outside this bound
   (0.40 ms per MiB of live heap native, 1.3–1.6 times that on wasm [M]):
   one a native or the engine requests (`Vm::request_gc`: after a
   `MemoryError` is raised, after a byte job fails for memory), a
   snapshot's and code reclamation's. The one a large result requests
   once allocation reaches half the room left (`Vm::collect_for_result`)
   runs as `gc.collect`'s does: a step per safepoint, the guest waiting
   until it ends (`Vm::collect_in_steps`). A
   mark with too little room left and one that gives up on scattered
   writes are held, a slice per check with no guest code between (§9.2).
   A fresh 64 MiB block costs one step of about 30–40 ms natively, most
   of it page faults and the `UNDEFINED` fill in
   `Heap::alloc_object`, where a caller does not take it unfilled. Neither
   V8 nor the shipped guest under wasmtime is measured per site.
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
   checked on every `Bounded` row; a sweep over every public method of the
   builtin types and native modules has not been rerun since the chunked
   steps landed.
4. **Results do not depend on how the host slices the run.** Output,
   exceptions, the values left and the fuel spent are the same, with the
   exceptions under Today, whether the cell runs unparked, in fuel slices
   of any size, past deadlines or through snapshots restored in other
   processes and on the other target.
   Budgets that pick a native or a twin path are constants, not the fuel
   left (`re`'s matcher included: its budget per slice follows the
   subject and the backtrack stack). **Today:** outputs and outcomes are
   checked by durability
   (parked every few units, and across targets) and the fuel spent by
   `Vm::charge_ticks` (within 0.2% across slices of 1 to 30,000 [M]); a
   container `repr` walks `REPR_SLICE` visits natively whatever the fuel
   left. `cargo xtask durability` (in `check`) compares the report, the fuel
   total and a final-state digest (the image's checksum at the end) of each
   case unparked, parked in one process, parked with a snapshot at every
   stop and parked with every park restored in a new process (and, with
   `--cross-target`, on and across both targets), and the restore chain also
   against the session that took the same snapshots and went on
   (`restore-vs-snapshot`, whose only recorded differences are the char
   cache's fuel in own cases 419, 472 and 498, and 419's digest), and fails
   on a difference `conformance/durability-results.txt` does not record.
   `--record` only removes entries; a change that makes a case differ for
   a known cause adds its line by hand, naming the cause and the step
   that closes it. Its first full run (1,416 runs) gave
   identical reports in all and equal fuel in 1,397 [M]. An operation the
   engine does not run again after a collection (§9.2), such as one that
   drew items from a one-shot iterator, can raise `MemoryError` while
   garbage not yet collected still counts against the limit, where CPython
   succeeds. A snapshot
   collects garbage before it writes the image, which also moves when the
   next collection falls due [E], so such an operation can come out either
   way: raise `MemoryError` unparked and succeed after a restore, or
   succeed unparked and raise after one. The snapshot need not be at the
   park just before the operation: one kept snapshot at an earlier park
   moved it [M, reviewer].
   Operations that do run again wait for a full collection before they
   raise, so a snapshot's collection does not decide their outcome; free
   space fragmentation and the bytes counted outside the arena, which
   differ after a restore and between targets, still can. An
   operation that fails partway for memory is charged for the work it
   did, so such a snapshot, restored or not, can raise or lower the fuel
   total. A reviewed `f(*big, **kw)` charged +28,672 units (+0.64%).
   Six such calls under `try` beside a 24 MB pad print `ok` six times
   unparked and `ok ME ok ME ok ME` with a snapshot at every park, and the
   fuel falls 11% (6,867,483 → 6,107,194 units) [M, reviewer]. Other
   operations §9.2 does not retry behave the same way, and even retried
   ones are charged per collection asked for, which a snapshot can move
   (§9.2). Fuel totals also
   depend on snapshots when a cell indexes or searches a long non-ASCII str:
   every collection forgets the char positions, a snapshot collects first,
   and a restore starts without them, so the next walk after any
   `Session::snapshot()`, restored or not, is charged again
   (`conformance/own/419`: +9,775 units
   with snapshots and no restore, +826 more with restores [M];
   `docs/user-guide.md`, "Other differences from CPython").
   Address-derived values are an exception today: which address a new
   object gets depends on when collections run, and that depends on slice
   boundaries, snapshots, GC stress and the session's limits. So `id()`
   values, default `repr`s (`<... object at 0x...>`) and the order of sets
   of identity-hashed objects can differ between runs sliced differently
   [M, reviewer]. What holds: `id` is unique among live
   objects, `id(x)` is the same for `x`'s whole life across parks and
   restores, `is` is unaffected, and containers with value-hashed keys keep
   their order.
5. **A long operation parks between steps and resumes where it stopped.**
   A step ends in one of three ways: with the operation's value; with its
   progress saved in an `NSTATE` object and a request to the prelude's
   `_steps`, which calls `_more(state)` once per step (each call a
   safepoint); or by handing the rest to a prelude twin with its progress
   passed explicitly, so nothing is done twice. A park or snapshot
   between steps sees only prelude frames and heap objects, so the
   snapshot format does not change for a new chunked operation; its
   `nstep::STEPS` entry is part of the build id, and a durability case
   parks inside it. **Today:** the `need_py` hand-off (its sites listed
   in `xtask/handoff-sites.txt`, which `cargo xtask handoffs` writes and
   `check` verifies) restarts the whole call in the twin from its reconstructed arguments, undoing what it
   consumed (`Vm::consumed`), and `Vm` mode flags carry the hand-off
   state, some of them in snapshots; keyed sorts resume through
   `Cont::Native`; iterators collect through `_collect_rest`.
6. **Up-front routing.** Builtin-typed data takes the native path; data
   that can reach user code goes to the Python implementation before any
   work is done. **Today:** natives find user code part way and hand off
   then (clause 5).

The contract is checked by `steps.rs` (clauses 2 and 3, `Bounded` rows),
the `run_source` fuzz target's step-time oracle (clause 2, sites no row
knows), durability on both targets (clauses 4 and 5) and the own cases
that park inside every chunked operation (clause 5).

## 6. Crates, size and the parser

| Crate | std? | Lines [M] | Contents |
|---|---|---:|---|
| `mm-core` | no_std | 5.0k | `Value`, arena (word and byte spaces), mark-sweep GC with side mark bitmap, incremental marking, write barrier, interning, shapes, FULL arena export/import |
| `mm-compile` | no_std | 8.7k | Engine AST, symbol tables, bytecode, exception and position tables, IC slot allocation, constant folding, superinstructions, the packed-code encoding. Depends on no heap |
| `mm-engine` | no_std | 75k Rust, 5.9k Python | Dispatch loop, frame stack, calls, exceptions, generators and coroutines, async scheduler, ICs, fuel, parking, host calls, builtin types and functions as tables, the native modules (`json`, `re`, `math`, `asyncio`, bigint, bytes, `datetime` parsers, `struct`, `ast`, `tokenize`, …), the prelude (`prelude.py`) |
| `mm-stdlib` | no_std | 0.1k Rust, 43k Python | One `mod-*` feature per module; the bundled Python modules under `python/`, compiled at build time (§12), and the feature bitmask |
| `mm-parse` | no_std | 7.9k | The own parser: indentation tokenizer and Pratt parser to the engine AST. The ruff front end survives only behind the `ruff-oracle` feature, the oracle of `cargo xtask parse-diff` |
| `micro-monty` | std | 4.8k | The facade and Rust SDK: engine, sessions, limits, progress and resume, park, snapshots, host modules |
| `mm-cli` | std, bin | 1.3k | The `mm` binary (`run`, `exec --json`, `resume`) the conformance, durability and bench gates drive; `publish = false` |
| `mm-cell-worker` | std, lib + bin | 2.4k | An embedding example: a worker for one external cell protocol (`plakous-cell-protocol.v2`), used in local integration testing (§10.2); `publish = false` |
| `mm-wasm-abi` | std | 1.6k | The `mm-wasm` guest interface: export and import names, codes, the binary encoding; `publish = false` |
| `mm-wasm-guest` | std, cdylib | 0.7k | micro-monty as a wasm module speaking `mm-wasm`; `publish = false`; §5 exception |
| `mm-wasm-alloc` | no_std | 0.1k | std's dlmalloc for `wasm32-unknown-unknown` without the per-site `validate_size` checks; registered by binaries only; `publish = false`; §5 exception (temporary) |
| `xtask` | std, bin | — | Size budget, conformance, durability, bench and the other gates (§13) |

Dependency direction: `mm-core` and `mm-compile` depend on nothing;
`mm-parse` on `mm-compile`; `mm-engine` on `mm-core` and `mm-compile`, and
on `mm-parse` only with its `ast` or `tokenize` feature (its build script
uses `mm-parse` and `mm-compile` to compile the prelude); `mm-stdlib` on
`mm-engine` (its build script compiles the bundled modules the same way);
the facade on all five; `mm-cli` and the worker on the facade (the worker
also on `plakous-cell`); `mm-wasm-abi` on the facade and `mm-wasm-guest` on
both and on `mm-wasm-alloc`. The wasmtime reference host,
`crates/mm-wasm-host`, is its own workspace (excluded from the root), so
wasmtime never enters the root lockfile; `hosts/js` is a TypeScript host for
the same guest (Node, workerd), outside cargo.

`micro-monty` and the five crates it is built from are published; they pin
each other with `=` requirements, as one engine, and the five internal
crates have no API of their own to keep.

**The `mm-wasm` interface.** A host checks `ABI_VERSION` (`mm-wasm-abi`,
currently 1) and the required export list before it runs anything, and
refuses a guest of another version. A change to the interface, a new
option tag or optional export included, bumps `ABI_VERSION`, with
`crates/mm-wasm-host` and `hosts/js` updated in the same change; the tag
classes and optional exports below describe the format, not a way to add
without a bump. Session options travel in a tagged options record: a tag
below `0x80` is strict (a guest that does not know it refuses the record), a tag
from `0x80` (`option::IGNORABLE`) is skipped by a guest that does not know
it. Only an option a guest can safely run without (a hint, a diagnostic)
takes an ignorable tag; one that limits or changes behaviour (a limit, the
clock, entropy, output, the text cut) takes a strict one. Some exports are
optional, looked for and done without by a host (`mm_set_gc_yield`, the
test-hook panic). Every other tagged encoding is closed: a decoder refuses
a tag it does not know. The snapshot format is separate: images carry the
engine's build id, and `mm_build_id` tells a host which images a guest
restores (§9.3).

`plakous-cell`, plak's protocol crate, comes from a pinned git revision because
it is not on crates.io. It brings tokio and nix (the host-side client; nix is
used for `getrusage` in the worker's resource report). Only the unpublished
worker depends on it.

**The parser.** The engine parses with its own `no_std` parser
(`mm-parse`). `cargo xtask parse-diff` builds `mm-parse` with `ruff-oracle`
(`ruff_python_parser` 0.0.14) in a separate cargo invocation, so ruff never
enters the default build, and requires identical ASTs, or identical
micro-monty errors, from both front ends on every corpus file, own case,
bench program and the prelude. ruff costs about 1.3 MB over an empty binary,
most of it `unicode_names2` (1.16 MB alone [M]), which it needs for
`\N{...}` escapes; micro-monty leaves `\N{...}` unsupported for that reason.

### Size

Reference points [M; stripped, opt 3, fat LTO, panic=abort]:

| Binary | Size |
|---|---|
| empty Rust binary | 0.29 MB |
| rquickjs (quickjs-ng) hello | 1.61 MB |
| dquickts hello | 1.84 MB |
| Monty 1.4 hello | 10.2 MB; 9.0 MB with fat LTO + panic=abort |
| MicroPython / PocketPy | 0.54 MB / 0.61 MB |

**Shipped builds.** `cargo xtask size` measures the builds an embedder
ships. The pinned stable toolchain with `RUSTC_BOOTSTRAP=1` rebuilds std
(`-Zbuild-std=std,panic_abort`, from the `rust-src` component) with
`-Cpanic=immediate-abort` and `-Cforce-unwind-tables=no`, which removes
std's panic formatting, backtrace symbolization (gimli, addr2line, miniz,
demangle) and `.eh_frame`; a panic in a shipped binary aborts without a
message. Shipped builds also pass `--cfg mm_shipped`, which turns on
`#[optimize(size)]` for part of the cold breadth through
`cfg_attr(mm_shipped, …)`; the attribute is unstable (rustc 1.100.0
rejects it, `E0658` [M]), so only `RUSTC_BOOTSTRAP=1` builds enable it.
Shipped codegen has two native gates in `cargo xtask check`: the own
cases and the corpus run on a natively built shipped `mm` (`cargo xtask
shipped-mm`), and the step-time oracle (§5.1) is built fully as shipped,
std and libtest rebuilt, each test in its own process
(`-Zpanic-abort-tests`), so a failing row aborts without its panic
message but its `OVER` line is printed. Bench Ir (§12) measures the shipped `mm` too:
an `optimize(size)` set can pass Ir and still slow shipped steps past the
bound (647 functions: Ir within 0.22%, `steps` failed as shipped [M]). Tests,
clippy and every other build stay on plain stable; `cargo xtask wasm`
builds its examples as shipped too. Native binaries stay
position-independent (the worker runs untrusted code). Shipped wasm modules
then go through binaryen's `wasm-opt -Oz` (version 133, installed by
`.agents/setup`) with exactly the features rustc enables, into
`target/<target>/release/shipped/`. It saves about 145 KB on `hello.wasm`
and costs 8–13% wall time on the dispatch-heavy bench programs (fib, loop,
closure) under wasmtime [M]; every `wasm-opt` level measured slower than
none, and `-O3` is within a few percent of `-Oz` and 14 KB larger [M].
`optimize_for_size` std, PGO (native `mm` −8% to −22% instructions, `hello`
+28% bytes) and BOLT were measured and not taken [M].

**Profiles.** `[profile.release]` is opt 3, fat LTO, one codegen unit,
`panic = "abort"`, stripped. Off the dispatch path, `mm-parse` builds at
`z`, `mm-compile` at `s` and the facade at `z` (51.6 KB off the guest,
16.2 KB off `hello.wasm`, bench Ir within +0.21% [M]); `mm-core` at `s`/`z`
costs 3–22% Ir, and `mm-engine`, which holds the VM, builtins and native
modules, stays at 3. `Vm::new` is kept out of line and the snapshot writer
lives in the engine (`Vm::write_snapshot_sized`), so neither runs at `z`.
Fat LTO optimizes the whole program at the root crate's level, so `cargo
xtask size` builds `hello`, the facade's example, with its own crate at
opt 3 (`cargo rustc -- -Copt-level=3`), as an embedder's binary is.

| Budget | Artifact | Name in the gate |
|---|---|---|
| Primary | `wasm32-unknown-unknown` `hello.wasm`, the in-process embedding, after `wasm-opt -Oz` | `hello.wasm` |
| Secondary | native stripped `hello` (x86-64 Linux, PIE) | `hello` |
| Guest | `wasm32-unknown-unknown` `mm_wasm_guest.wasm` (the mm-wasm guest, no `test-hooks`), after `wasm-opt -Oz` | `mm_wasm_guest.wasm` |
| Bundle guest | `wasm32-unknown-unknown` `mm_wasm_guest_bundle.wasm` (the guest built with `host-bundle`: its stdlib and prelude bytecode in the custom sections `mm.prelude` and `mm.stdlib`, served by the host through `mm_host.bundle_read`), after `wasm-opt -Oz`, sections included | `mm_wasm_guest_bundle.wasm` |

`cargo xtask size [--base REV]` builds each artifact as shipped, and the
same for two more revisions built on the same host, each in a worktree
outside the checkout (so it reads none of the checkout's `.cargo/`) and
cached per commit, recipe and toolchain: the base (the merge base with
`REV`, default `origin/main`) and the release (the latest `v*` tag the
base descends from; with none reachable the gate fails and names the
fetch). A revision whose xtask has `size --head` builds itself with its
own recipe, so a change to the recipe, such as the `wasm-opt` flags, is
charged like any other rise; an older one (v0.0.1) is built with this
checkout's recipe and its own toolchain. The head's artifact names must
be the base's and the release's. Any rise past the base, one byte or
more, needs git trailers `Size: <artifact> <±bytes> <why>` in the commits
since the base, each giving its own commit's change, whose bytes summed
cover the rise and exceed the change by at most a tenth (at least 1 KiB);
a commit a later one reverts counts for nothing, and a rise with
uncommitted changes, untracked build inputs included, always fails. A
rise past the release must stay within that release's budget in
`xtask/size-budget.txt` as the base holds it (`<tag> <artifact> <±bytes>
<why>`, the last line holding, 0 without one), which only the owner
extends, so small rises cannot add up over a release; a budget line lands
before the growth it allows, since the gate reads it from the base. A push
is judged once, head against base, on its net change, so an intended rise
lands in its own push (owner, 2026-11-13). The native artifact
is gated only on `x86_64-unknown-linux-gnu`; other hosts report its size,
because executable formats and code size differ per target. The ceilings
until 2026-11-13 and the reasons for their raises stay in
`xtask/size-ceilings.txt`, read-only (owner decision D2).

**Sizes at v0.0.1** [M, 1cfcde3, Rust 1.99.0, binaryen 133,
`cargo xtask size`]:

| Artifact | Bytes |
|---|---:|
| `hello.wasm` (`wasm32-unknown-unknown`, after `wasm-opt -Oz`) | 2,271,969 |
| native `hello` (x86_64, PIE) | 2,490,336 |
| `mm_wasm_guest.wasm` | 2,358,872 |
| `mm_wasm_guest_bundle.wasm` | 2,357,307 |

`hello.wasm` is 1,811,762 bytes of code and 455,312 of data. Code by
crate: mm-engine 1.02 MB, mm-core 317 KB (almost all inlined heap
accessors), mm-compile 149 KB, std 141 KB, mm-parse 112 KB. The 65 stdlib
modules' packed bytecode is 281,480 bytes (12.4%), the prelude's 57,122. A
build with no stdlib modules (`struct` and `operator` stay) is 1,808,788
bytes of wasm and 1,996,400 native; a `host-bundle` `hello.wasm` (every
capability, no bytecode) is 1,938,038. `uv run --with pyelftools python3
xtask/size_map.py` maps the shipped builds by section and crate into
`target/size-map/report/size-map.md`. The `mm-cell-worker` binary is larger
(serde, tokio) and is not what the budget measures.

**Bundled bytecode.** The bundled modules are written by
`Program::encode_sections` as nine byte streams: own strings, string
references, other metadata, opcodes, registers, `u` operands (inline-cache
slots predicted), and `b`, `i` and `j` operands. Strings that three or more
modules use are stored once, raw, and made `Arc`s only when a decode uses
them. Each stream is packed against one set of Huffman codes for every
module (`pack_shared`). A session keeps the nine decoders it built for its
first decode. The prelude is packed the same way, in chunks of units (one
per top-level `def`, class, instance or literal binding), and a unit is
built in a session on the first lookup of its name (`mm-engine/src/prelude.rs`);
no prelude Python runs at session start. The stdlib and the prelude are
part of the one engine module: a default build, the shipped guest
included, carries their packed bytecode in its data segments. The bundle
guest (`host-bundle`) keeps the same bytes in the custom sections
`mm.prelude` and `mm.stdlib`, which the host serves through
`mm_host.bundle_read`, read as each unit or import needs them.

**The wasm allocator.** On `wasm32-unknown-unknown`, std's allocator is
dlmalloc, and its `free` path runs debug-style `validate_size` assertions
that fat LTO inlines into every deallocation site (the constant they test
appears 1,687 times in the guest against 52 on `wasm32-wasip1` [M]).
`crates/mm-wasm-alloc` wraps the same dlmalloc through `c_free`/`c_realloc`
and saved 83,553 bytes of `hello.wasm` and 97,112 of the guest [M] (§5
exception, temporary).

## 7. Data model

### 7.1 Value: 8 bytes, NaN-boxed

```text
f64            any non-NaN double, or a canonical NaN (0x7FF8_0000_0000_0000,
               or 0xFFF8_0000_0000_0000 with the sign bit)
tagged (qNaN)  0xFFF8_0000_0000_0000 | tag << 48 | payload (48 bits)
tags           1 SmallInt (i48) | 2 Ref (u32 offset, optional kind byte)
               | 3 Special (None/False/True/NotImplemented/Ellipsis/Undefined)
               | 4 NativeFn (u32 table index) | 5 ByteRef (u32 byte-space offset)
```

- `f64::to_bits`/`from_bits` make this safe Rust. Every NaN is
  canonicalised on construction, so the tagged space never collides with a
  float, and two values are the same object exactly when their bits are
  equal.
- A `Ref` payload may carry the object's kind in spare bits, so type checks on
  hot paths don't load the header.
- `SmallInt` overflow promotes to a heap bigint.
- `Value` is `Copy` and pointer-free.
- `Undefined` marks unbound locals and `ByteRef` lets the GC find byte-space
  blocks generically; neither is ever visible to Python.

### 7.2 Arena and objects

- **Word space:** segments (up to 1 MiB each) in a segment table. A `Ref`
  is a 15-bit segment index and a word offset, still a `u32` covering
  32 GiB. Object headers, inline slots, list/tuple/dict buffers and frames
  live here.
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
  64 KiB, and each, while it is the bump segment, grows by reallocation,
  doubling up to 1 MiB; later segments are committed whole. A `Ref`
  addresses the segment's index and offset, so growing its backing store
  changes no `Ref` and the arena stays non-moving at the Python level.
  Bitmaps grow with it, the limit counts what is committed, and a snapshot
  records each segment's capacity, so a restore commits the same. A
  dedicated segment's room past its object (a block growing over several
  steps) is recorded as a number, never as bytes, and restore checks it
  against the object's length, `MAX_OBJECT_BYTES` and the memory limit
  before reserving it. A small session's arena therefore tracks its live
  heap: `Session::new` plus `x = 1` commits 394,240 bytes [M].
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
  `segvec` is typed with growing chunks. It borrows size classes from
  mimalloc and line/block marking from Immix, and is the most heavily
  property-tested and fuzzed code in the repository.
- **Non-moving:** size-class free lists, large objects as contiguous runs, no
  compaction. A block of up to 504 bytes fills freed space before the bump
  frontier, and a bigger one bumps first: a small object kept after each
  block of a growing series (`rows = rows + [row]`) once split the space
  the series freed into blocks each just too short for the next, and such
  a loop raised `MemoryError` with under 2 MB live. A bigger block takes
  room rounded up to one of 64 sizes per doubling (except when bumped in a
  segment that is not yet full-size, where the room would only fill more),
  the rest a hole that is reused with it once it dies, so a slightly longer
  block fits where a temporary was (`prefix + str(i)`). An object's `id()`
  does not change while it lives; which address a new object gets depends
  on when collections run (§5.1, clause 4).
- Header (one word): `type:16 | flags:8 | nvals:8 | len:32`. `nvals` counts the
  leading value slots the GC traces (255 = all slots); the rest are raw words.
  Shape ids live in a value slot as a SmallInt. A side object-start bitmap
  validates every reference, so a stale or forged one is an error.
  **No mark bit**: GC marks live in a side bitmap so a collection doesn't dirty
  every live page.

| Type | Layout |
|---|---|
| `str` | byte-space UTF-8, char count, lazily computed hash cached behind a `HASHED` flag, ASCII flag; non-ASCII char indexes walk from the nearest known position (above) |
| `int` | `SmallInt`, or heap bigint (sign + limbs), own implementation |
| `dict` | insertion-ordered entries in one of three forms: inline in the object (literals ≤ 8 entries), linear (an 8-entry buffer scanned by stored hash), indexed (compact u32 slots with hash tags, stored hashes, power-of-two capacity); globals dicts are always indexed so inline caches apply. A 2-entry dict is 48 B, against CPython's 184 [M] |
| `list` / `tuple` | length plus a word buffer of `Value`s |
| instances | shape id plus inline slots; fall back to a dict past 32 attributes (`MAX_SHAPE_ATTRS`), on deletion or on `__dict__` access |
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

### 7.4 `bytearray`

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
- **Snapshots.** Nothing special: a bytearray is an ordinary object plus a
  byte block, written and validated with the segments. The spare capacity
  is saved with it. A length past the block (only a corrupt image) reads
  as the whole block, never out of bounds.
- **Memory limit.** The capacity block is arena memory, so the limit
  counts it exactly; a growth that does not fit raises `MemoryError`
  before the length changes. The old block is garbage until the next
  collection; a block of 64 KiB or more requests one once the bytes
  allocated since the last collection reach half the memory left, as
  `bytes`, `str` and int results do. Self-referential edits (`b += b`,
  `b[1:1] = b`) copy through an admitted scratch buffer.
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
  `unpack_from`, `iter_unpack` and `struct.pack_into` (Python over slice
  assignment, with CPython's checks and messages).
- **Left out.** `memoryview` (views need exported-buffer pinning that
  blocks resizing), subclasses of `bytearray`, `__alloc__`,
  `bytearray % args`, `bytes.translate`/`maketrans` (absent for `bytes`
  too), bytearray host values (the host boundary carries `bytes`), and
  bytearray arguments to modules that check for `bytes` (`hashlib`, `re`,
  `json`; `binascii` and `base64` take them as bytes). The shared methods
  keep `bytes` in descriptor names and argument errors (`bytes.find()`).

## 8. VM

### 8.1 Bytecode

- **Shape: accumulator-register** (V8 Ignition style). One implicit
  accumulator plus frame registers: `LDAR r`, `STAR r`, `LDA_SMI imm`,
  `ADD r` (`acc = R[r] + acc`), `TEST_LT r` + `JUMP_IF_FALSE`,
  `LDA_GLOBAL`, `GET_ATTR`, `CALL_METHOD`, `RETURN`. Locals and expression
  temporaries are frame registers; the compiler allocates temporaries
  stack-like per expression. Instructions are one or two 32-bit words:
  opcode in bits 0..8, then `r`/`b` 8-bit, `u`/`i` 16-bit and `j` 24-bit
  (word pc) operands (`mm_compile::bytecode::layout`); bits that hold no
  operand are zero. A call window is the top of the caller's temporaries:
  callee in `R[r]`, `R[r+1]` reserved, arguments from `R[r+2]` (`R[r+3]`
  after `LOAD_METHOD`, `self` in `R[r+2]`), so arguments are passed in
  place, not copied.
- **Dispatch: one `match` loop** with pc, frame base and the accumulator in
  locals; handlers `#[inline(always)]`. Rust has no computed goto and `become`
  is unstable (rustc 1.100.0 rejects it, `E0658` [M]).
- **Why** [M]: a dispatch spike over fib, loop, closure, attribute, method,
  list and dict kernels, without GC, bigint or exceptions, measured the
  accumulator design fastest on all seven, 4–27% faster than a stack
  design, with 19–43% larger bytecode and a 3% larger dispatch function; a
  function-pointer handler table was 35–65% slower than `match` in every
  design; bounds checks cost 6–29% and stay (§5). Published evidence agrees
  in direction [S]: a register JVM executed about 47% fewer VM instructions
  with about 25% larger bytecode (Shi et al., §14).
- A side exception table (pc range → handler, depth), as CPython 3.11+: no
  runtime setup cost.
- Fused and specialized forms, added only when a benchmark shows a win:
  compare-and-jump (`TEST_LT_JF` and its siblings), `LOAD_METHOD` +
  `CALL_METHOD`, `ADD`/`SUB`/`MUL` with int fast paths beside the generic
  `BINOP`, small-immediate forms (`LDA_SMI_ADD`, `LDA_SMI_SUB`) and
  range loops (`FOR_RANGE`).
- **Inline caches live outside the arena**, in per-code side tables, and start
  empty after a restore. They are never snapshotted and never dirty code
  pages. Each entry is one 24-byte `Ic`:

| Cache (`Ic`) | Guard / content |
|---|---|
| `Global` (`LDA_GLOBAL` / `STA_GLOBAL`) | entries-table version → entry index (a deleted entry is a miss, so `del` and re-adding a global keep other caches) |
| `Builtin` | a builtin, valid while the globals' keys are at a version |
| `Attr` / `SetAttr` | class version and shape id → value slot (and, for a store, the shape it moves to) |
| `UMethod` / `Method` | a method found on the class (class version, shape → function), or a builtin type's method (type → native) |
| `Call` | a Python function with simple parameters → its code |
| `New` (a `CALL` on a class) | class version → `__init__` function and code; filled only when `type.__call__` would just allocate and run a plain Python `__init__` taking the call's arguments (metaclass `type`, no `__new__`, no builtin or exception base, simple parameters). Class versions come from one VM-wide counter, so the version names the class; rebinding `__init__` or `__new__` in the class or a base bumps it |

### 8.2 Calls never recurse on the Rust stack

- Python → Python calls push a frame and continue in the same loop.
- Dunders invoked by VM ops (`__add__`, `__getitem__`, `__eq__`, `__iter__`,
  `__init__`) push a frame with a continuation (`Cont`); the finishing op
  runs when it returns.
- Builtins that call back into Python follow §5's fast path / prelude rule:
  the prelude (`mm-engine/src/prelude.py`) holds the slow-path twins of
  builtins and VM operations, which a native hands off to on the first user
  `__eq__`/`__hash__`/`__repr__`, `key=` function or generator.
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
  access in `x['a'].b.c` was `None`).
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
`Cont::AwaitStep` continuation), as `yield from` would. A yielded value
under `coroutine.send()` suspends every coroutine frame from the `AWAIT` up
to the one `send()` runs: each saves its registers, its `AWAIT` pc and what
it waits on in its coroutine object (as a generator does at `YIELD`), and
`send()` returns the value; the next `send()`/`throw()` re-runs the saved
`AWAIT`s from the outermost. In a task, a yielded `None` yields to the
scheduler (the task is queued again) and any other value raises
`RuntimeError`. No Rust frame holds a suspended coroutine and no second
dispatch loop runs.

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
`Semaphore`, `wait_for` (CPython 3.12+'s: the awaitable runs in the calling
task and a timer task cancels it) and `as_completed` are prelude Python
over a loop-settled `Future` and a first-done gather. Deadlines are
virtual-time sleeps, and every sleep waits first for the cell's pending
host calls (each unsettled host future of the cell, awaited or not): host
calls take no virtual time, so `wait_for(host_call(), t)` never times out
on the engine's clock and a loop polling with `sleep(t)` gets its answer;
real timeouts belong to the host (`HostError` `TimeoutError`). A host
future does not outlive its cell: cell end forgets its call, and awaiting
it in a later cell raises the scheduler's `RuntimeError`.

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

`mod-asyncio` gates the module (its natives, prelude units and the
scheduler's task, gather, sleep and loop paths); coroutines, `await` and
host futures are core and stay without it.

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

**`CHECK`.** Straight-line code has none of those, and a cell may hold
some 20,000 statements, each a bounded but long native operation (2,000
`b = s == t` over 10 MB strings run 7.5 s with 100 units of fuel when
nothing checks between them [M]). So the compiler puts a `CHECK` instruction where a path has
run `CHECK_EVERY` (32) instructions that may work in proportion to their
data since its last safepoint; moves, constants, name loads and stores,
identity tests and branches do not count, and a call restarts the count
(it ticks once it returns). Counts are per path: a forward jump carries its
count to the label, which takes the most of what reaches it; a loop head
restarts it (its back edge ticks), so a path runs fewer than 64 counted
instructions between two checks; a handler starts with the most its block
reached. `CHECK` charges nothing: natives that charged more than 64 units
since the last check, in one charge or many small ones (`Vm::burn` counts
them, `burned`), or spent the fuel, left the tick counter at most 1, and
only then does `CHECK` run the safepoint. Fuel totals do not change: a
check charges only the safepoints that ran. `burned` is not in the image:
every park runs a check, which clears it, and a restore at a host call
starts it at 0, which can delay that one check by at most 64 units but
changes nothing a run charges or prints. It is a rare opcode, so the
dispatch loop is unchanged; loop bodies of the bench programs hold none. A
park lands between two instructions with `acc` and the registers in the
frame, as at any safepoint, so the snapshot format does not change.

**Stop before a long operator.** A `CHECK` every 32 such instructions
still lets up to 63 long ones run between two checks. So an operator that
would start after a native charged more than 64 units at once since the
last safepoint (`long_work`, set by `Vm::burn`) stops before it starts
(`Vm::stop_before`): a binary or comparison operator (`in` included), unary
`-`/`~` on a long int, and an f-string field's conversion of a value that
is not a str. It returns `Err(Raised)` with `check_before` set;
`Vm::execute` runs the safepoint (collection, park request, deadline, fuel)
and dispatches the same instruction again, or parks before it, as an int
operator short of fuel does (that check comes first, so its park still
reports `fuel_wanted`). Its state is unchanged until then, so a park or
snapshot there needs nothing new. The hooks sit in out-of-line entry
functions that only the dispatch loop calls, so a stop never cuts a native
in half, and the loop's own code is unchanged (a hook in the inlined
`op_unary` cost `loop` 10% Ir [M]). Between two safepoints straight-line
code then runs at most one long operator, plus instructions that copy into
a new object (slices, displays, f-string joins), which the memory limit
bounds. Garbage is still collected only at safepoints, which these stops
provide between long statements.

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
- No finalizers, no weakrefs.
- **Trigger.** Bytes allocated since the last collection exceed twice the
  live bytes after it (`gc_ratio_percent` = 200, live floored at 1 MiB), or
  memory pressure: at least 1/64 of the limit allocated since then and
  either under 1/8 of the limit left uncommitted with at least half of that
  allocated, or a space that cannot commit its next segment with under 1/8
  of the limit free (`Heap::should_collect`); plus the 64 MiB and headroom
  rules below.
- **Collect and retry.** Garbage not yet collected counts against the
  memory limit, so a native can fail where a collection would have made
  room. A method of an immutable builtin type (`str`, `bytes`, `int`,
  `float`, `tuple`, `frozenset`), a copying method of a mutable one (`copy`
  of a list, dict or set, `dict.fromkeys`, `set.union`, `intersection`,
  `difference`, `symmetric_difference`), a growth that writes nothing
  until its new buffer or table is in place (`list.append`, `set.add`,
  `list.extend` from a list or tuple), `sorted` and `list.sort` (with a
  `reverse` that is a bool or an int; a key is called from the sort's
  later steps, and `list.sort` gives its list the items back first), or a
  constructor that builds or copies its argument (`list`, `tuple`, `set`,
  `frozenset`, `dict`, `bytes`, `bytearray`), called straight
  from a `CALL` that raises `MemoryError`, changed nothing and consumed no
  iterator, so the `CALL` runs again once a collection that started after
  the failure has ended (`Vm::oom_retry`). That collection is a fresh one:
  a mark in progress is dropped (`Heap::restart_collection`), so its work
  follows what is reachable when it starts. It waits as a held mark does
  (`Vm::gc_held`, below), and runs again once after the steps, also after
  a snapshot and restore (the rerun mark is in the image). It retries once
  per failure, so a result that cannot fit raises after one collection; a
  request larger than the limit, or a size cap refused before the heap is
  asked (`list(range(10**9))`), raises at once, uncharged
  (`Heap::take_refused`, `Vm::room_may_help`). No
  count of bytes allocated decides whether a collection can help: garbage
  is made by dropping references, which allocates nothing, and a value
  dropped during an incremental mark survives it (two copies of a 2M-item
  set made and dropped left the third `MemoryError` with a quarter of the
  limit live [M]). The failing operation is charged for the collection
  instead, at once: a unit per 64 bytes of the memory limit, the most one
  collection marks, capped at `RETRY_CHARGE` (2^20 units;
  `Vm::collect_for_retry`), so a guest that catches `MemoryError` in a
  loop pays for each collection in fuel. The charge per collection is a
  constant of the session (a charge by the live bytes the collection left
  differed by 4 units after a restore [M]); how many collections a run
  asks for still moves with snapshots, whose collection can leave a retry
  nothing to wait for (-4.2M to +5.0M units [M, reviewer]), until
  snapshots stop collecting. A collection is charged once: a retry while
  one is asked for and not ended pays nothing more. A result of 64 KiB or
  more that asks for a collection (`Vm::collect_for_result`) with less
  than 1/64 of the limit allocated since the engine's own last one tops
  its result's charge up to the same amount, only for the request that
  starts the wait: near the limit a loop of such results ran a full
  collection each for 1,094 units [M, reviewer], and a top-up per result
  charged one `split` into 61 parts 32.6 million units [M, reviewer]. A
  chunked set or dict operation whose new table does not fit, at its
  first step or a later one, leaves the table for its next step, which
  waits for the same collection, charged the same way, and allocates
  again; a second failure raises (`Vm::oom_defer`). That the step has
  waited is in its own state, so in the image: kept in the VM, it was
  lost on a restore, and a chain restored at every park deferred for ever
  [M, reviewer]. Its work so far is kept. A long sort's second array, or
  a slice's room, waits the same way, its wait in the sort's state, and so
  does a keyed sort's first array, which comes after the key calls (the
  call cannot run them again): through a call to the prelude's `_gc_wait`,
  whose safepoint runs the collection, its wait a bit of the sort's state
  word. The
  retried `CALL`'s fuel is
  charged again. An instruction whose native raises `MemoryError` straight
  back to the dispatch loop retries the same way (`Vm::oom_retry_op`): a
  binary operator (`Vm::op_binop`) unless it may have changed its left
  operand first (an in-place operator on a dict or set, or on an instance
  of a subclass of one), an f-string's `BUILD_STRING`, a starred display's
  `LIST_TO_TUPLE`, `UNPACK_EX` from a list or tuple, `LIST_EXTEND` from a
  list or tuple or an iterator over one, `CALL_EX` of a Python function
  or of a builtin type whose constructor copies its argument
  (`dict(x, **kw)`), a call's keyword and `**` spread (`DICT_UPDATE` modes
  2 and 1: a failed insert writes nothing, and a spread first takes out
  the keys it put in, so its rerun refuses none of its own), `SET_ITEM` (stores are
  idempotent, and a list slice builds its new buffer before installing it),
  `GET_ITEM` of a builtin container (a slice builds only its result),
  `LIST_APPEND` (a comprehension's list grows into a new buffer before it
  changes), `SET_ADD` and `DICT_INSERT` (a comprehension's set or dict
  takes the entry out again, or writes nothing, when its table does not
  grow: its outgrown tables are garbage only a collection frees),
  `FORMAT` and `FORMAT_SPEC` (a user `__repr__`, `__str__` or
  `__format__` anywhere in the value hands the whole format to the
  prelude twin `_op_fmt`, whose `repr`, `ascii`, `format` and walk
  `_walk_repr` are retried natives) and `CALL` of a builtin type whose constructor copies its
  argument (`list(l)`), the last ones read at the fault exit
  (`Vm::oom_retry_fault`).
  User code never runs inside those natives (a dunder runs as a request),
  so it never runs twice. Nothing that drew items from a one-shot iterator
  runs again: `Vm::collect` marks the failure as a rerun's (`oom_rerun`)
  as it starts on one. A builtin iterator over a list or tuple, forward or
  reversed, is copied as a slice of it instead, and ends once the copy's
  buffer is allocated (`Vm::seq_items_left`), so a copy that did not fit
  changed nothing and runs again. That mark, the rerun's own, and the items a builtin
  consumed (`consumed`) belong to the instruction that set them
  (`Vm::op_stamp`), so they end at the next tick wherever a host's fuel
  slices end, and whether a later instruction runs again does not depend
  on the slices. A rerun's mark also names its instruction (`oom_rerun_at`,
  in the image): the next instruction in the same tick (`f(**d)`'s
  `CALL_EX` after its spread) retries as its own. The hooks sit outside the dispatch loop's arms
  (`Vm::op_binop`, `Vm::op_setitem`, `Vm::oom_retry_fault`), so its code is
  unchanged. A size the heap refuses before the native allocates
  (`Heap::would_fit`, `list(range(n))`) is retried as a block that did not
  fit is. A refusal no retry took dies with its `MemoryError`
  (`Vm::unwind`), and a builtin that goes on another way when a request
  fails, raising nothing, drops the refusal it made, so a refusal does not
  reach a later request or a park. Each of these raises a fresh `MemoryError()` with empty args and
  no `__context__`, as CPython's (`Vm::raise_no_memory`); the shared one
  is raised only when there is no room for a new one.
- **Large blocks that grow.** A byte block written over several steps
  (`json.dumps`, §5) that grows by moving to a new block holds both blocks
  until the move ends; the old one is then garbage, and a collection is
  requested by the result rule (`Vm::collect_for_result`) so it is not
  counted against the next growth. When the limit cannot admit the new
  block's spare room, the move falls back to growing the block in place,
  which the allocator may do by copying it and so holding both buffers for
  a moment: that growth is admitted for both too (`Heap::append_bytes`). A
  growing text therefore reaches about half the memory left, and past that
  is a `MemoryError`, not a copy outside the limit.
- **Incremental over 32 MiB live.** A full collection pauses about 0.4 ms
  per live MiB of small containers natively and 0.55 ms on the mm-wasm
  guest [M], past the 50 ms step bound (§5) from about 125 MiB. From
  `gc::MIN_LIVE` (32 MiB live after the last collection) a due collection
  marks a slice of 2^18 work units per safepoint check (§8.6) instead
  (`Heap::collect_step`), and a park, deadline or snapshot can follow any
  slice. Below it a collection runs in full and is also due once live and
  allocated bytes reach 64 MiB, so a heap growing through the threshold is
  marked in full at most at about 64 MiB: 30 ms native at worst [M]. There
  is no write barrier beyond the setters' segment flags: a value-slot write
  sets `DIRTY_REFS` on its segment, the marked objects of such segments are
  rescanned in later slices until a pass finds few, and a remark of the
  roots and the remaining dirty segments ends the mark in one step
  (mostly-parallel marking). Objects allocated during a mark are unmarked;
  what reaches them is a root or a written slot, so the remark finds them.
  (Allocating black was measured and made worst gaps longer: it scans the
  garbage made during a mark [M].) The sweep that follows is lazy
  (`Space::pending`): allocation sweeps a few segments before committing
  memory, each check sweeps 8 segments per space, and everything that walks
  the heap (`write_snapshot`, the code-liveness scan) finishes it first. A
  snapshot's own collection sweeps at once, segment by segment in ascending
  order, as a restore lists its free blocks, so the session that goes on and
  the restored one allocate the same blocks (`Heap::collect_swept`,
  `conformance/own/533`). Mark state lives outside the arena and is never
  snapshotted: a snapshot, code reclamation and a native's collection
  request abandon a mark for a full collection, and a restored heap starts
  with none.
- **Headroom.** Nothing is freed until the remark, and natives cannot
  collect, so a native that allocates more than the room left during a
  mark raises `MemoryError` where a full collection at the mark's start
  would have made room. Over `MIN_LIVE` a collection is therefore also due
  once the room left (uncommitted budget plus free blocks) is no more than
  the bytes allocated since the last one, and a due collection whose room
  is under half of those (a native allocated them at once) is held (below).
- **Pacing.** Natives allocate a lot in few steps (stepped set and dict
  builders make tables of tens of MiB a step), so a slice a safepoint falls
  behind. A step therefore marks a slice plus a work unit per word
  allocated since the last step, at most two slices more, the rest owed to
  the next steps (a set's 32 MiB of hashes, allocated in one step, gives
  steps of 4–9 ms [M]), and a mark is held (below) from the step where the
  room left is no more than the bytes allocated since it began.
- **Held marks.** A mark the guest cannot go on allocating beside (it
  starts with less room than half the bytes allocated since the last
  collection, or reaches the pacing point above) and one that does not
  converge (below) is held (`Heap::holding`): it goes on a slice per
  safepoint check with no guest code between slices (`Vm::safepoint`
  loops), so nothing is written or allocated meanwhile, and a park,
  deadline or cancel can still follow any slice; a park leaves the next
  slice owed, run first by `Vm::resume`. A held mark ends after a rescan
  pass that finds nothing dirty and never gives up. Worst gaps on such
  rows are 23–32 ms native [M].
- **Park first.** When a park is due at a safepoint where a collection
  step is due, the park goes first and the step is owed: `Vm::resume` runs
  it before any code, with a park check after it. A step is owed once,
  never twice, so with fuel slices (every check a fuel park) collections
  still run; stress testing collects in place. So one gap never holds both
  a native's step and the collection step it made due (`ys = xs + xs` of
  4M items: 24.4 ms worst gap [M]).
- **Fuel and collections.** Collection work charges no fuel: when a
  collection falls due depends on byte sizes (32- or 64-bit) and on
  restores, and fuel totals must not. A host that regains control only
  between fuel slices (deployed Workers, where deadlines never fire)
  would then wait for every collection step in a slice. With
  `Session::set_gc_yield` (off by default; `drive_async` and `hosts/js`'s
  sliced driver turn it on, the latter through the optional
  `mm_set_gc_yield` export) a safepoint that ran a collection step ends
  the slice with a `Fuel` park and fuel left, so a slice holds at most one
  collection step. Fuel charged and results are the same with it on or off
  and on native or wasm32 [M].
- **What it bounds** [M], the longest gap from a park request to the park,
  native / wasm guest, at 199 MiB and 778 MiB live (1 GiB limit): garbage
  loops 2.4 / 3.2 ms and 8.2 / 7.7 ms; in-order writes into old objects
  2.7 ms and 4.4 ms native; appends 5.5 ms native at 778 MiB. Full
  collections there take 61–101 ms and 269–338 ms native, 131 ms and
  477–518 ms on wasm.
- **Scattered writes into a large old heap.** Writes into old objects all
  over the heap dirty nearly every segment between passes, the rescans do
  not converge, and the remark approaches a full mark. So a mark whose
  rescans pass a quarter of its first mark, or whose rescan pass does not
  halve the dirty segments, gives up: it is held from then on, and so are
  the next 4 due collections from their start (doubling up to 16,
  `MAX_BACKOFF`, while marks keep failing). Such a loop then costs about
  what full collections cost, in slices: 3M writes into 199 MiB take
  11.9 s with held marks and 12.1 s with full collections, worst gap
  8–9 ms against 264–280 ms [M]. Allocation, appends and in-order writes
  rescan at most 2% of their first mark and never give up [M].
- **New live data built during a mark** is unmarked and traced by later
  passes or the remark, so a native that builds tens of MiB of objects
  that stay live pays for tracing them in the remark, a step of its own
  (`list(zip(xs, xs))` of 4M over 200k old lists: 34 ms [M]).
- **Snapshots at parks.** Mark state is not snapshotted, so a snapshot
  taken at any park first collects in full and then writes the image
  (153–250 ms at 192 MiB [M]). That is the host's call, outside the step
  bound.
- **Also unbounded:** every collection under 32 MiB live (30 ms native
  at worst) runs in full, as do a snapshot's, code reclamation's, and one
  a native or the engine requests (`Vm::request_gc`): after a
  `MemoryError` is raised and after a byte job fails for memory.
- **`gc.collect()`** asks for a collection that starts after the call
  (`Heap::force_collection`) and waits for it as a held mark does
  (`Vm::gc_held`): a step per safepoint check, no guest code and no fuel
  between steps, a park check after each, so over 32 MiB live it can park
  between slices (4M live lists: steps of at most 1.4–1.8 ms [M]). The
  collection after a result of 64 KiB or more, once allocation since the
  last collection reaches half the memory left (`Vm::collect_for_result`),
  runs the same way (`Vm::collect_in_steps`), so its garbage goes before
  guest code or the next growth allocates (`json.dumps` of 1M small dicts:
  worst step 7.2–10.9 ms [M]).
  The collection is not charged to fuel, so fuel alone does not bound a
  cell's CPU time: 4,000 calls with 100,000 live lists took 5 s for
  120,076 units [M]; a deadline does.

### 9.3 Snapshots

- **FULL only:** after a GC, the segments of both arena spaces plus VM
  state. Free runs are omitted, so a fragmented heap doesn't inflate
  snapshots. Write and restore are copies plus header checks.
  `Session::snapshot()` runs a GC and writes; `Engine::restore` reads into a
  new session (same or another process).
- **Every mutation goes through mm-core setters (the write barrier),** which
  also keep the dirty-segment flags of the incremental mark (§9.2).
- **No deltas in the engine.** Deltas are an efficiency feature, not a
  semantic one; hosts make images small at the storage boundary (below).
  Two engine-side designs were built, measured and not adopted [M]:
  write-time dirty-line tracking cost 4–19% execution time on 11 of 13
  bench programs, and snapshot-time comparison against the fresh-session
  image gained no more than the compression prefix below while adding
  about 22.5 KB to the guest (`bench/evidence/base_relative/` at `2bcbb8c`).

```text
header   magic "MMSNAP\0\x01", format u16 = 1 (SNAPSHOT_FORMAT), build_id [u8;32]
         (bytes 10..42, a permanent layout: micro_monty::image_build_id),
         returns_last_expr, want_value, the stop (phase byte: 0 idle,
         1 host call, 2 waiting, 3 parked; then the stop as reported),
         text_limit, fuel total (0, or 1 and a u64)
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

- **Same build id only.** A `build_id` or format mismatch is a typed
  `SnapshotError::Incompatible { format, build_id }`, naming the image's.
  Other errors: `Corrupt { what }`, `TooLarge`, `Busy`, `Poisoned`,
  `Internal { message }`.
- **Upgrades drain; nothing migrates.** The engine adds no cross-build
  migration. A host that upgrades keeps the old build, keyed by build id,
  until that build's parked snapshots have drained, and treats a cell no
  kept build can resume like a crash (an unknown outcome the host
  reconciles). The id hashes no target-specific input, so native and wasm
  builds of one source and feature set share it, and a native host can
  drain old images on that build's wasm guest. `mm-wasm-host`'s `Runtime`
  holds several guests keyed by build id and restores each image on its
  own (`docs/user-guide.md`, "Upgrading: drain old builds").
- **Trusted input.** The host integrity-protects snapshots and never loads
  one produced by the guest or the user. The checksum detects corruption,
  not tampering. The reader still bounds-checks everything and is fuzzed.
- **Persistence: FULL images, compressed by the host.** The engine writes
  FULL, uncompressed images. Hosts compress with zstd level 1, using the
  build's fresh-session image (`Session::new` then `snapshot`) as a raw
  prefix (`--patch-from`), with the frame checksum on, and key each blob by
  build id; a dictionary trained on the host's own parks is optional
  (`docs/user-guide.md`, "Storing snapshots"). Measured [M]: plain zstd-1
  gives 3.6× on small parked sessions and about 19× on 100k retained rows;
  the base prefix adds 1.3–1.4× on small parks (code-mode parks' median
  102.8 KB → 19.7 KB), and base plus a trained dictionary reaches 8.1×
  against FULL on held-out code-mode parks.

**Compatibility: the build id is a layout descriptor.** Two builds with the
same id restore each other's images; the id hashes what an image's meaning
rests on and nothing else, so an edit to a function body (a fix in a
native, the interpreter, the compiler, the facade) keeps parked images
restorable, while a layout change refuses them as `Incompatible` before
anything is read. It hashes:

- `SNAPSHOT_FORMAT` (micro-monty `src/image.rs`) and `SNAPSHOT_EPOCH`
  (mm-engine `snapshot.rs`);
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
  is covered without a list to keep, and a tuning constant changes the id
  too (the safe side);
- the opcode table (number, name, operand format), the native, type and
  native-step (`nstep::STEPS`: name, value slots, raw words) tables in
  order;
- the packed prelude units and every enabled bundled module's name and
  packed bytecode, with the section headers and shared strings it unpacks
  with, each hashed at build time (`PRELUDE_HASH`, `mm_stdlib::CODE_HASH`),
  so a build that does not link those bytes (`host-bundle`) has the same
  id; and the enabled `mod-*` features;
- samples of the value, header, string-hash, int-hash and float-hash
  encodings;
- `CODEGEN_PROBE`, a recorded hash of what `Session::define` makes of a
  probe definition that uses every feature of the generator: the spec an
  image stores, the source generated from it and the compiled program. A
  facade unit test recomputes it and fails, printing the new value, when
  the spec codec, the generator or the compiler change it, so `build_id()`
  compiles nothing at run time (its first call takes about 60 µs natively
  [M]). A restore also checks each host program it builds again against
  the key and code hash the image stored.

**The epoch rule.** Bump `SNAPSHOT_EPOCH` in the same commit whenever a
function body changes what an image holds or how it is read while none of
the hashed items above changes. In particular:

- what a raw word or value slot of an existing NSTATE step holds, with
  the same counts: `STEPS` hashes only each op's name and counts, so a
  change to what word 3 of an existing op means must bump;
- what an existing `Cont` field or stage means;
- a layout written as literal slot or word numbers in code (`list`,
  `dict`, `set`, `range`, iterators, ints, `bytes`, `bytearray`, the
  regex matcher's saved run), or the hash, probing or order of dict and
  set tables;
- what an opcode's operands mean, with the same format;
- how frames, generators or coroutines keep their state.

**Golden images.** `crates/micro-monty/tests/golden.rs` restores images
blessed by an earlier build (`tests/golden/`): under the id that wrote them
each must continue exactly as the session that wrote it did, and under
another it must be refused as `Incompatible`. A change that moves the id
blesses them again in the same commit (`MM_BLESS=1 cargo test --release
--locked -p micro-monty --test golden`, the build `cargo xtask check` tests
them in), which writes nothing and fails if a continuation differs from its
committed transcript (`*.txt`); `check` fails while the images carry another
id. So every change that keeps the id is checked against images an earlier
build wrote. `cargo run -p micro-monty --example build_id` prints the id.
Function-body edits in a native, the VM, the regex matcher, the compiler
(same bytecode) and the facade keep the id and the golden images continue;
a class-slot swap, a renumbered backtrack-frame tag, an epoch bump, a code
edit in any of the four codecs, a tuning constant, a prelude edit that
changes bytecode and a host-wrapper generator edit change it [M].

**Restore.** Order: magic, format version, checksum, `build_id`. Lengths
are checked against the remaining input before any allocation; the heap is
checked against the engine's memory limit (`TooLarge`, §11 "Restore
admission").

Validated on restore: segment indices, run tiling, every block header, every
`str` (UTF-8, char count, flags), the intern table (identity, count, load),
every code object (`Code::verify`: operands in range, jumps on instruction
starts, only `JUMP_LOOP` backward, sane handler ranges, a terminator last;
and, for stored code, every `YIELD_FROM` right after its `SEND`), frame and
generator pcs, frame bounds, code indices, tasks and host-call state, every
dict's index (entry bits, duplicates, tags, probe reachability, length),
continuation targets and registers, shape transitions, class size hints and
bundled-module entries. A generator's code is its function's in every state;
a frame that runs for a generator or coroutine runs that object's code while
it is running (`Corrupt("frame owner")`). Each continuation slot is held by
one frame at most, and the free list names each empty slot once. A dict
without an index uses no more entries than it holds. A chunked native's
state (`NSTATE`) keeps the words its next step trusts as an index, a stride,
a count or a slice's bounds in range (`Corrupt("native step")`); a set's or
dict's rebuild into new tables (a bulk operation's, or json's growth of an
object) holds in them exactly the live entries of the old table it has
moved; the bytes a str build has written are UTF-8 holding the chars it
counts, and each step checks the seam it copies onto. A chunked native's
last step can change its data in place without saving its words, and
`_steps` holds its state until the step's fuel is charged, which may park;
so a finished state is marked done (`nstep::op::DONE`), which no step runs
and restore does not check. Other words are checked by the steps, with
checked arithmetic (`SystemError`) in every build profile; a long sort's
positions must follow from its pass, and its two arrays differ from each
other and from the list sorted. Every reference, in the VM part and in every
arena value slot, must name an object of its kind (`check_refs`,
`Corrupt("references")`). The stop the facade's header names (idle, a host
call and its id, a wait on host futures, a park) must match the state the VM
holds (`Corrupt("stop")`), and a waiting image must hold an answer or a
future status, a cancelled one included, for every pending call; the intern
table's untagged string references are checked by mm-core. Free lists,
bitmaps and size classes are rebuilt by a sweep. Every arena access is also
bounds- and start-bitmap-checked and fails with an error.

Not saved: what `SessionSettings` gives a new or restored session (the
output callback, the wall clock, entropy, the zone source, the text cut,
`int_bits_text`, GC stress), the deadline and park flag, which the host sets
per run, and `Session::set_gc_yield`, which a host sets again after a
restore. Inline caches and char positions start empty.

Host-call ids survive a restore: the restored session accepts the same
`Resume`, and `Session::pending` re-reports the stop (a host call with its
arguments, the calls a wait is on, a park's reason) from the facade's
header.

**Cost** [M, release, `taskset -c 2`, median of 21; the range over five
runs; 5a926cf7, 16 cores under gVisor, Xeon at 2.60 GHz]:

| Session | Size | Write | Restore |
|---|---|---|---|
| empty | 41.3 KB | 0.03–0.06 ms | 0.03–0.06 ms |
| 100k `(i, -i)` tuples | 3.5 MB | 3.8–4.2 ms | 4.5–4.9 ms |
| 100k `str(i)` | 5.9 MB | 6.5–7.1 ms | 8.4–8.8 ms |
| 100k `{'i': i, 's': str(i)}` | 10.7 MB | 13.9–14.7 ms | 17.1–18.5 ms |
| parked 151 generator frames deep (methods `yield from` each other) | 78.2 KB | 0.08 ms | 0.08–0.09 ms |

Size tracks the live heap, so the remaining lever is object layout, not the
snapshot format. Monty takes 10.63 MB / 17 ms / 89 ms and dquickts
3.06 MB / 41 ms / 28 ms on 100k small objects [M]. A snapshot, its full
collection included, takes about 1.4 ms per MiB of image natively, under
wasmtime and on V8 (about 250 ms at 170 MiB; the table's rows 1.1–1.4). A
restore costs about as much per MiB or a little more: small objects
1.2–1.5 ms/MiB native (the table's rows 1.3–1.8), 1.0–1.8 under wasmtime
and 0.9–1.7 on V8; large blocks 0.6–0.9 native and 0.3–0.7 on wasm. A
process's first restore takes up to 1.7 ms/MiB native and 1.8 on wasm, and
V8's first call into a new instance adds a warm-up (154 ms at 35 MiB) [M,
eb78933, 16 cores under gVisor].
`bench/evidence/restore-cost/run.sh` at `2bcbb8c` reruns these.

## 10. Host boundary

### 10.1 Engine API (facade)

The facade (`crates/micro-monty/src/lib.rs`; `docs/user-guide.md` is the
embedder's guide):

```rust
Engine::new(EngineConfig::default().with_limits(Limits::default().with_memory(m).with_fuel(f))) -> Engine
    // Limits { memory, recursion_depth, fuel } and EngineConfig { limits } are #[non_exhaustive]: Default or const new(), then with_* or fields
Engine::compile(&self, source, filename) -> Result<Code, CompileError>
Session::compile(&mut self, source, filename) -> Result<Code, CompileError>   // charged to the session (512 B per source byte + 16 KiB); MemoryError when it does not fit (§11)
Engine::restore(&self, &[u8]) -> Result<Session, SnapshotError>   // Incompatible{format, build_id} | Corrupt{what} | TooLarge | Busy | Poisoned | Internal{message}
Session::new(&Engine) -> Result<Session, PyException>             // binds the prelude's natives; units load on first use; `mm run print(0)` 0.90M instructions whole process, 0.57M gated (bench/callgrind.py) [M]
Session::new_with(&Engine, SessionSettings) / Engine::restore_with(&[u8], SessionSettings) / SessionTemplate::instantiate_with(SessionSettings)
    // SessionSettings { output, wall_clock, entropy, zone_source, text_cut, int_bits_text, gc_stress }: what images do not
    // carry, given the same way to new and restored sessions (#[non_exhaustive]; the plain forms use its Default)
Session::define(&HostModule { name, functions, classes, constants, exceptions }) -> Result<Vec<HostFunctionRef { name, id }>, DefineError>   // Invalid(reason) | Busy | Raised(MemoryError) | Poisoned
    // "" = builtins; replaces the module whole; between cells; survives reset, snapshots, templates
Session::host_modules() / stubs() / host_function_id(name) / host_function_name(id)
Session::run(&Code, &mut dyn HostHandler) / drive(Progress, &mut dyn HostHandler) -> Result<Progress, ResumeError>
Session::run_async(&Code, ..) / drive_async(Progress, &mut dyn AsyncHostHandler).await
    // optional driver; Step::{Resume, Stop}; async runs in ASYNC_SLICE_FUEL (10,000) slices, yielding Pending between them
Session::set_host_args(HostArgs) / set_text_limit(bytes) / set_text_cut(TextCut) / set_last_expr_value(bool)
Session::set_hint(HintGroup, Option<&str>) -> Result<(), PyException>   // redirect sentences per group; session state
Session::set_wall_clock / set_entropy / set_local_timezone / set_module_dunders
Session::call_log() -> Vec<HostCallRecord { call, function, name, status: CallStatus }>
Session::phase() -> Phase / pending() -> Option<Progress>         // the stop as reported (HostCall, Waiting, Parked); snapshots carry it
Engine::environment() -> String                                  // prompt summary generated from the build
HostError { kind, message, data: Option<HostValue>, class: Option<String> }   // HostModule::exceptions
HostSignature { .., dict_as_kwargs: bool }                        // one positional dict binds as keywords
Session::set_output(Box<dyn FnMut(&str) + Send>) / set_output_streams(StreamOutput)
Session::set_fuel(Option<u64>) / fuel() / set_fuel_total(Option<u64>) / fuel_total() / set_deadline(Option<Instant>) / set_deadline_with(now, deadline)   // deadline one-shot
Session::fuel_wanted() -> u64                                    // after Parked{Fuel}: what an operation that parked before starting wants; 0 otherwise
Session::set_zone_source(Option<ZoneSource>)                      // TZif bytes by key for zoneinfo; not session state
Session::set_gc_yield(bool)                                       // a slice ends after a collection step (§9.2)
micro_monty::head_tail(text, max_bytes) -> (String, bool)          // the HeadTail cut, for output text too
micro_monty::build_id() / image_build_id(&[u8])                    // the build's id; an image's
micro_monty::set_bundle_reader(f)                                  // host-bundle: the process's bytecode reader, set once
Session::park_handle() -> ParkHandle (Send; request()) / request_park()
Session::start(&Code) -> Progress
Session::resume(Resume) -> Result<Progress, ResumeError>          // Unexpected | UnknownCall | AlreadySettled
Session::cancel() -> bool                                         // drops the cell's frames, tasks, futures; keeps globals
Session::snapshot(&mut self) / snapshot_with_limit(max_bytes) -> Result<Vec<u8>, SnapshotError>   // output capped at the memory limit by default
Session::reset() -> Result<(), PyException>
SessionTemplate::capture(&Engine, &mut Session) / instantiate() / image() / from_image(..)
Engine::with_module_cache(bytes) / with_host_code_in_snapshots(bool)

enum Progress {
    Complete(Option<Completion { repr, str, type_name, value: Option<HostValue>, *_truncated }>),
    HostCall { call: u64, function: u32 /* id from define */, name, args: Vec<HostValue>, kwargs: Vec<(String, HostValue)> },
    Waiting { calls: Vec<u64> },                       // all tasks blocked on host calls
    Parked { reason: ParkReason },                     // Fuel | Requested | Deadline
    Error(PyException),
}
enum Resume { Value(HostValue), Error(HostError), Future, Batch(Vec<(u64, Result<HostValue, HostError>)>), Continue }
```

A yield window is a deadline; cancel is `Session::cancel()`. A park request
stays until the park it causes takes it, across `start` and `cancel`, so a
host's "stop this session" cannot be lost to a cell that was starting or
ending.

Output streams through a callback in production order. `HostValue` is plain
data: `None`, `Bool`, `Int` (`BigInt` as decimal past `i64`), `Float`,
`Str`, `Bytes`, `List`, `Tuple` (which keeps a tuple's identity across the
boundary; the cell worker sends it as a JSON array), `Dict` with `str`
keys, `Record { name, fields }` for namedtuples, `Object { class, fields }`
for host class instances, and `Unconverted` for a value that has no host
form.

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
does not reproduce. They are hidden from tracebacks, survive `reset` and
ride in snapshots, and the host-call path gains only a `_host_defs` lookup
when a host function is built. Binding uses `_mm.signature`; annotation
checks raise `TypeError` at the call; there is no static checker (§1).
`Session::stubs()` renders `.pyi` text. `SessionTemplate` is a snapshot
taken after setup that instantiates by restore.

### 10.2 An embedding example: the cell-protocol worker (`mm-cell-worker`)

`mm-cell-worker` runs micro-monty behind one external, engine-neutral cell
protocol, plak's `plakous-cell-protocol.v2` (length-prefixed JSON frames),
and is used in local integration testing against plak (`publish = false`).
It shows how a host maps a frame protocol onto the facade:

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

The worker runs the py-ptc presentation: bootstrap, nested calls with the
argument bounds and messages of plak's Monty-based Python worker, Waiting
batches and Delivery verdicts, ordered output with the 1 MiB cap, yields
(`window` sets an engine deadline; a cell waiting on the host also yields
when its window ends), and Cancel, which ends the cell with `Failed{"cell
cancelled by the host", generation_lost: false}` and keeps the worker. For
the SDK presentation it accepts `sdk_stubs` and runs the cells (the harness
sends `Reset` and the SDK bootstrap as a Monty prelude) without a pre-run
type check, which is the harness's. Against a local checkout of plak it
passes 46 of 48 of plak's worker-driven protocol tests [M]; the two failures
are Monty-only behaviour (the allocator ceiling) and the SDK test's
type-error cell, which expects a checker.

`mm-cell-worker` is a library a Rust host can run in process, plus a thin
binary:

- **Library** (`src/lib.rs`, `unsafe` forbidden): `serve(reader, writer,
  engine, options)` runs one worker over any std `Read`/`Write` and blocks
  the calling thread (one reader thread; no async runtime, so a tokio host
  bridges its pipe with `SyncIoBridge`, or runs it on a thread over an
  in-memory duplex). The engine is a `CellEngine`: `NativeEngine` links the
  facade; a host can wrap `mm-wasm-host`'s session in `CellSession`
  itself, so the library never links wasmtime. `Hooks` (`confine`,
  `usage`, `boundary`) and `Kill` (park-and-lose, for a worker without a
  process to signal) are what an embedding adds.
- **Binary** (`src/main.rs`): stdio, the seccomp filter (`src/seccomp.rs`,
  the §5 exception), its process resource report, the crash-cut hooks and
  the panic's last-gasp `HostLost`. After the handshake it installs a
  Linux seccomp filter allowing only reads and writes on its pipes, memory
  mapping, and exit. The engine makes no syscalls, so this costs nothing
  and turns any engine bug into, at worst, a dead worker.
- **Policy.** The library adds no policy features (no queueing). Its policy
  values are `Config` with plak's Python worker's values as defaults: 16
  outstanding nested calls and their `RuntimeError` text, the 1 MiB print
  and result caps and their notices, the script name `plak-py-ptc.py`, value
  nesting depth 128 and the 16 KiB nested-error bound. The engine and facade
  impose no outstanding-call limit (pending host futures are bounded only by
  the memory budget, §11).

Registering the worker with a harness (its identity, config and discovery)
is the harness's work, not this repository's; integration tests run against
a local checkout of plak.

### 10.3 Embedding

micro-monty is a pure-Rust crate (`cargo add micro-monty`): no C toolchain,
no bindings, and the stdlib's Python modules are compiled into the crate at
build time, so nothing ships beside it. It builds for wasm.

| | rquickjs | dquickts | micro-monty |
|---|---|---|---|
| Stop a running script | interrupt: abort only | suspend if every frame is plain sync code | park at any safepoint, any frame kind |
| Snapshot, resume elsewhere | no | full and delta, same build | FULL, same build id; host compression |
| Durable host calls | no | yes (`JS_SuspendCall`) | yes, sync and `await` |
| Engine memory safety | C | C | safe Rust, bounds-checked arena |

In-process guarantees: safe Rust and checked arena access protect host memory;
guest effects use host-provided capabilities. Arena memory, execution fuel and
Python/native recursion have limits. The arena limit is not a bound on all
session allocations, and fuel is not a hard park-latency guarantee (§11);
`docs/user-guide.md` ("Embedding guarantees") lists what is guaranteed, the
minimum host thread stacks and the test behind each. Two conditions:

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
`Engine::with_module_cache` sets another cap. It is charged once to the
Engine, not to any session, and a session is charged and behaves the same
with or without it. Every bundled module decoded is about 2.9 MB, host
code about 0.67 MB per 100 typed functions; on json, asyncio, re,
collections and datetime a warm restore takes 9.95M instructions against
23.75M uncached, and RSS per session with 40 held is 1.2 MB against
1.85 MB [M]. For fully hostile code, process or wasm isolation
remains the stronger boundary.

Durability guarantees are pause-anywhere and same-build snapshots. Work after
the last snapshot is redone on restore, and an in-flight host call's outcome is
the host's to resolve.

## 11. Limits

One limit per resource the engine can exhaust:

| Resource | Limit | Mechanism | Default |
|---|---|---|---|
| Heap memory | arena segments in use, including their GC bitmaps (+3.1%), plus retained code, frame, scheduler, shape and host-definition storage (admission, below) | preflight before big allocations (`*` repeat, `pow`, `<<`, `str.replace`, `join`, `range` → list); raises a catchable `MemoryError` | host-set (facade default 256 MiB) |
| One object | 64 MiB of data or slots (`mm_core::MAX_OBJECT_BYTES`): a str's or bytes' data, a list's or tuple's slots, a dict's or set's tables | checked where a dedicated segment is placed; a bigger allocation raises `MemoryError` whatever the memory limit, so one operation on one object is bounded work (§5). Growth by doubling stops at the cap: a dict holds at most 2,097,152 entries (its index of 2 × cap + 1 words is one word over 64 MiB at the next capacity, 2^22) and a set 5,033,164 items (a table of 8M slots 60% full) [M] | fixed |
| CPU | fuel | charged at safepoints and by natives; exhaustion parks (`Parked{Fuel}`) | unlimited (`Limits::fuel = None`); per cell, host-set; `Session::set_fuel_total` caps the session |
| Wall time | deadline | the host's clock (`set_deadline`, or `set_deadline_with` and the host's own `now()`) read at each safepoint check (§8.6); one-shot, parks (`Parked{Deadline}`) | host-set (none by default) |
| Python stack | frame depth | `RecursionError` | 1000 |
| Rust stack | native nesting depth | parser and compiler depth-counted (`MAX_NESTING`); run-time nesting on arena stacks (§5) | 256 KiB host thread to run, about 650 KiB to compile [M] |
| CPU in one native call | int ↔ str digits; fuel charged by natives | CPython's 4300-digit limit; proportional fuel; an operation estimated at 65,536 units or more parks before starting when the fuel left cannot pay (`Session::fuel_wanted`): the int operators `*`, `**`, `/`, `//`, `%` and, called straight from Python code, `pow`, `divmod`, `sum` and `math.prod` of a list or tuple, `math.factorial`/`perm`/`comb`/`isqrt`/`gcd`/`lcm`, `sorted` and `list.sort`; other long natives run in steps of at most 2^17 units (§5) | 4300 |
| Int size and superlinear int work | bits; estimated native time | an int past 2^26 bits (`intops::MAX_BITS`; linear operations on it take at most about 21 ms [M]) and a superlinear operation (`*`, `//`, `%`, `divmod`, `**`, `pow(a, e, m)`, `str()` and `int()` with the digit limit off, `math.factorial`/`comb`/`perm`/`isqrt`/`gcd`/`lcm`, `round(x, -k)`) whose time, estimated from its algorithm (Karatsuba's products, the squarings of `**`), is past 45 ms (`intops::WORK_MAX`) raise `MemoryError` before starting (the latter with "integer operation too large"), so no such step runs past 50 ms. Reducing an int at the cap by a modulus of up to 128 bits (`%`, `//`, `divmod`, `pow(a, e, m)`'s base, `math.gcd`) takes short division and is not refused [M]. CPython computes these: `3 ** (2 ** 22)` in 0.61 s, `math.factorial(50_000)` | 2^26 bits; 45 ms |
| Output | byte cap | host callback | host-set |
| Completion text | `repr`/`str` of a cell's result | cut by the facade (`Session::set_text_limit`, `set_text_cut`) | 1 MiB, or the memory limit if less |
| Source | size and nesting | checked before and during parsing | 256 KiB (`MAX_SOURCE_BYTES`) |

Fuel and deadlines surface as typed `Parked` reasons, never as exception text
the host must parse; memory, object-size, int-size and recursion limits raise
Python exceptions (`MemoryError`, `RecursionError`) that guest code can
catch, an uncaught one reaching the host as `Progress::Error` with its
`type_name`, and source limits fail compilation (`CompileError`). Errors
raised to Python use CPython wording. Disk, network, processes and
concurrent host calls are harness resources; the harness enforces their
quotas and resolves uncertain external effects.

### Session lifetime

Before starting another cell, the facade cancels the old computation. Once
the code and shapes loaded since the last reclamation pass an eighth of the
live heap (at least 16 KiB) or a quarter of the memory left, or when a full
collection is due anyway (`Vm::prepare_cell`), it collects in full and
reclaims user compilation units not referenced by live functions,
generators or saved tracebacks; dead code below that threshold stays, in
the session and in its images. Keeping a unit keeps its nested function
bodies too. Code, source and cache indices are compacted only between
cells, never under suspended frames. Reset also reclaims unreachable user
code. The cost includes a heap scan.

Reclamation's live marks and remapping indices belong to the code, source
and shape records, admitted with those records during loading and restore.
A fixed heap cursor visits code and shape references without collecting
object addresses or sorting a slot table. Live references are rewritten
only when their indices move; removing trailing records needs no rewrite
scan. Shared traceback entries are temporarily tagged during remapping and
untagged before reclamation returns, without allocations, callbacks or
collections between those passes.

### Admission: storage outside the arena

Everything a session retains outside the arena reserves its capacity from
the same budget as the arena, before it allocates, and releases it when the
storage goes. `Heap::available_bytes` includes every current reservation
when deciding whether more fits, and GC pressure counts them. These rules
bound owned capacities, not an allocator's transient peak or process RSS.

- **Retained code.** Code buffers, materialized constant and name vectors,
  source strings and the code, source and inline-cache tables. Loads
  preflight before cloning; reclamation and failed loads release. Bundled
  and synthesized code count too. Bytecode verification uses one scratch
  bit per byte of code; loading reserves the largest such workspace across
  retained codes, so a same-budget restore can verify.
- **Host modules.** Retained names, constant keys, nested host-value
  buffers and their tables. Registration counts the new clone before
  allocating it; replacement keeps the old definition live until success,
  then releases its charge. Definitions survive reset. Nesting is checked
  before cloning, with a 1000-container ceiling, and the largest
  decoder-stack workspace across definitions stays reserved. Imports borrow
  the shared immutable definitions; guest values are separate arena copies,
  so guest mutation does not change retained definitions.
- **Synthesized methods.** Namedtuple and dataclass signature code is hidden
  from tracebacks but is not permanent prelude code: only reachable
  functions retain it between cells and across reset. Its cache holds sorted
  code indices, shares the budget, and is in the image, validated on read,
  so a restore keeps the same signature codes and counts toward the 4,096
  limit (`MAX_SYNTH`) from where it was (`conformance/own/532`).
- **Frames.** Register stacks, frame records and continuation tables,
  including suspended tasks' stacks. Task switches transfer rather than
  duplicate the charge. Cell completion, cancellation and reset release
  frame storage, keeping a small register buffer. Continuation free-index
  storage is reserved up front so unwinding does not allocate.
- **Scheduler.** Task, future and gather tables and the blocked and ready
  queues. Registration reserves room for every task wakeup and gather
  callback before it can become pending, so settling and unwinding need no
  queue allocation. Host futures use an id-sorted vector with logarithmic
  lookup, keeping settled ids until the cell ends. Cell end releases
  scheduler capacities.
- **Instance shapes.** Shape tables, name lists and transition buffers.
  Above 32 attributes an instance switches to a dict rather than retaining
  a quadratic chain of name-list prefixes. Between cells, GC finds live
  instances, and shape reclamation keeps their shapes and transition
  ancestors, compacts ids and releases unused capacity. Shape names are GC
  roots; once unused shapes go, weak interning lets their names die.
- **Collector scratch.** Admitting a word object reserves its mark-stack
  entry, so the mark stack (one `u32` per allocated object) never grows
  during marking. GC recounts survivors and releases excess capacity with
  hysteresis.
- **Segment metadata.** Segment and bitmap descriptors and vacant-index
  capacity. Table growth reserves slots for every possible segment release,
  so GC need not allocate at memory pressure.
- **Bundled-module workspace.** Import admits unpacked bytes, Huffman
  tables, the decoder's string pool and the decoded program before copying
  them. The largest workspace across loaded modules stays reserved for a
  same-budget restore. A cell's section decoders live until the cell ends;
  every section a decode unpacks is charged its tables, so a charge does not
  depend on which decoders were already built.
- **Fixed VM tables.** Builtin type and constructor tables, per-module code
  and global slots, the shared park flag and the initial register stack
  reserve their capacity before session construction or metadata decoding.

**Restore admission.** `Engine::restore` reads into one budget, the
smaller of the saved and the restoring engine's memory limits, which the
session keeps: the arena, retained code and source, frame storage,
scheduler tables, shapes, host definitions, collector and segment metadata,
fixed VM tables, bytecode verification scratch and the bundled-module
decode workspace are each admitted before allocation (`Vm::read_snapshot`,
`TooLarge` past it), in the same capacity classes the session had, decoded
straight into their final storage. Not in that budget: the caller's image
bytes, the pending stop's host values that `Session::pending` re-reports
(the host's data, capped separately at the memory limit, as snapshot output
is), the decoded modules the Engine's cache shares, and the allocator's
transient peaks.

### Admission: native work buffers

A native whose Rust buffer the guest can make large admits that buffer
against the session budget before filling it, holds the charge through the
arena allocation of its result (so a successful conversion does not hide a
double-buffer peak), and releases it on success, error or Python hand-off.
Refusal is a catchable `MemoryError`, which can come before a later
formatting error or user callback. Natives borrow arena data in place where
they can, since a native neither collects nor runs Python code while it
borrows (§5).

- **Collecting and sorting.** `Vm::collect` reserves its value buffer before
  copying or growing; the buffer owns its reservation until dropped,
  including while a nested iterable is collected. Copies saved for a
  one-shot iterator's prelude fallback count separately. Scratch
  reservations share the budget but release independently, so nested
  operations cannot replace each other's charges. Native sorting admits its
  merge workspace; `list.sort` reuses the list's backing store, and a failed
  admission leaves the list unchanged.
- **String and byte builders.** `join`, `replace`, f-string concatenation,
  split and `re` pieces, byte concatenation, repetition, zero-filled
  construction, padding, case mapping, hex and codec output admit their
  exact capacity before copying. Replacement sizing counts the actual
  non-overlapping matches. Slicing, splitting, partitioning, stripping,
  search and predicates borrow their sources; unchanged ranges reuse the
  immutable value; codecs iterate scalars without a character vector.
- **Rendering.** `repr`, `str`, exception text, ASCII conversion and format
  padding return text with its own reservation. Native `repr` visits
  container entries through cursors (storage grows with depth, not width),
  admits its work stack and active-identity table before growing them, and
  bounds its builder while appending, so a completion cut at the text limit
  never renders a whole leaf first (it keeps at most one UTF-8 character
  past the limit to tell exact fit from truncation). Completion preflight
  borrows child slots with a fixed 65-entry cursor stack; if its identity
  table cannot be admitted, it releases its storage and selects Python
  rendering, which can park or raise `MemoryError`. Exception arguments that
  are exceptions hand `str` to the prelude; others are read in place.
- **Formatting.** After parsing a builtin format spec, width × UTF-8 fill
  size must fit the budget before type-specific rendering (so an oversized
  width can raise `MemoryError` before a type error); user `__format__`
  gets its spec unchanged. `str % args` and `bytes % args` admit their input
  copy and output before growth, read the selected tuple slot per field,
  and keep prelude-converted text as arena values through precision and
  padding.
- **Printing.** Native `print` borrows separators, endings and builtin
  strings; accumulated output uses an admitted buffer kept through the host
  callback, and a failed native attempt emits no partial output. Stream
  `write` passes the arena string to the host callback directly. Custom
  files keep their per-piece prelude writes.
- **JSON.** Encoding admits output capacity before each growth, sizes
  escapes exactly (UTF-16 surrogate pairs under `ensure_ascii`), traverses
  lists, tuples and unsorted dicts through arena cursors, admits sorted
  dicts' key and entry storage, and tracks cycles with an admitted identity
  table of the current ancestor path (none with `check_circular=False`).
  Decoding admits its input copy, its array and object buffers, a
  per-document key memo (arena references in a half-full table, 4,096 keys
  at most) and escaped strings; unescaped strings borrow input ranges. Both
  directions keep the depth ceiling.
- **Host-value conversion.** The `HostValue` trees the engine builds for
  host-call arguments and completion values reserve from the session
  budget as they grow, and the conversion keeps its nesting on a charged
  explicit stack. Past the budget, a strict argument conversion raises
  `MemoryError` at the call, a lenient one yields `TooLarge`, and a
  completion gets no host value (its `repr` still comes back). The charge
  is released when the tree is handed to the host (a 64 MiB session
  returning 3,000,000 ints peaks at 101,508 KiB RSS [M]).
- **Guest-sized buffers elsewhere** are admitted or bounded by a constant:
  `re` matches waiting for the arena (a call ends its slice after 2^16 span
  words) and the backtrack stack (12 bytes a word past 2^16); `str.join`
  over str subclasses; `struct` format items; split ranges; `strptime`'s
  fast path (formats past 4,096 tokens go to Python); `ast.parse` (512
  bytes per source byte, at most 32 KiB of source) and `tokenize` (64; at
  most 64 KiB); `unicodedata.normalize` (16 bytes per decomposed char);
  power-of-two digits; float and complex precision text; `expandtabs`;
  host-call argument vectors.

### The memory bound

For any guest input, while a cell runs, a session with `Limits::memory = L`
keeps the process's Rust heap within **2.5 × L + 16 MiB** of what it held
before. The parts are:

- the arena, at most L;
- at most about one transient native copy of the largest value a native
  reads: receiver copies of `str` methods, `list + list`, repetition,
  big-int operands and temporaries, the KMP table of `bytes.find`,
  `str + str`'s staging, call-argument vectors, exception text, numeric
  intermediate strings and some formatting workspace. These are not yet
  admitted, and they set the factor 2.5;
- fixed tables and capped scratch: the `re` and native-scratch allowances,
  the bundled modules a cell decodes.

The measured high-water is 2.01 × L [M], from `bytes.find`'s KMP table,
over 73 probes at 16, 64 and 256 MiB; the ratio is the same at each limit.

Compiling a cell is inside the bound when it goes through
`Session::compile`: the session reserves 512 bytes per source byte +
16 KiB (`COMPILE_BYTES_PER_SOURCE_BYTE`) as scratch while it parses and
compiles, releases it after, and returns a `MemoryError` compile error when
that does not fit what is left (after a collection, when no cell is
stopped). The worst measured is 405 bytes per source byte [M]: one-character
statements, just past a doubling of the statement vector. The cell worker
and the mm-wasm guest compile cells this way.

Outside the bound:

- **`Engine::compile`:** a host compiling ahead of any session, at most
  512 bytes per source byte + 8 MiB [M] (up to about 405 bytes per source
  byte measured, so about 101 MiB at `MAX_SOURCE_BYTES` [E]).
- **Snapshots:** writing or restoring one holds the arena and the image
  once: heap ≤ L + image + 16 MiB [M]. A pending host call's encoded
  arguments, which the session keeps for `Session::pending` and the image,
  are charged to the session while kept.
- Host-owned `Code` and `HostValue` objects, allocator overhead and process
  RSS: `Limits::memory` is not a total session-memory or RSS limit.

Enforcement: `cargo xtask check` runs `fuzz/src/bin/memory_probes.rs`,
which replays `fuzz/memory_probes/*.py` under a counting allocator (§5),
snapshots and restores each session after its cell, and fails on any probe
over the bound, the snapshot bound or the compile bound. The `memory_bound`
fuzz target, at an 8 MiB limit, searches for inputs that break either.

Hosts size process memory from the bound. A wasm host gives the instance
about 3 × L + 16 MiB of linear memory, which covers the run bound and a
snapshot. The guest frees an input or output buffer larger than 64 KiB once
it has been read, so a written or restored image is memory the arena can
reuse, not a lasting addition (linear memory never shrinks).

**Snapshot output.** `Session::snapshot` caps output-buffer capacity at the
session's memory limit, independently of live state, so a nearly full
session can still checkpoint; `snapshot_with_limit(max_bytes)` overrides
the cap for one call. Heap, code and VM encoders stream twice: once to
measure the image, then into one buffer of exactly its size, the header and
checksum included, allocated only if it fits the cap. Oversized output or a
failed allocation returns `SnapshotError::TooLarge`; the session remains
usable, although GC may have run.

**Completion text.** The facade cuts a completion's `repr` and `str` at the
session's text limit, 1 MiB by default or the memory limit if that is less;
printed output is not cut. User-rendered `repr` and `str` stay in the
budgeted arena until handed to the caller; pending text is a GC root and
snapshots store its value references. The facade copies only the requested
UTF-8 prefix into caller-owned buffers.

### Responsiveness

A native fuel charge over the small-work polling threshold brings forward
the next safepoint even with unlimited fuel, so pending requests and
deadlines do not wait through later large calls (§8.6). A native whose work
grows with its data does at most `nstep::STEP_WORK` (2^17) fuel units per
step, and each step is a safepoint (§5, "Bounded native steps"), so sorting,
copying, rendering and conversion park between steps;
`crates/micro-monty/tests/steps.rs` checks the 50 ms bound (§5.1). Not
steps, and not cut by a park request or deadline: a full collection (§9.2),
`Session::snapshot`, `Engine::restore` and compilation. `bench/latency.rs`
measures request-to-control separately from fuel: a host call also returns
control, and is distinguished from an explicit park. Do not promise hard
real-time cancellation. Fuel is per cell (`Limits::fuel`, or what
`Session::set_fuel` set for the next cell); `Session::set_fuel_total` caps
all of a session's cells together, though refuelling a parked cell with
`set_fuel` is not capped by it.

## 12. Features and performance targets

The default module set is the facade's `default` features
(`crates/micro-monty/Cargo.toml` is the source of truth): `mod-json`,
`mod-re`, `mod-collections`, `mod-itertools`, `mod-functools`,
`mod-datetime`, `mod-random`, `mod-asyncio`, `mod-typing`, `mod-base64`,
`mod-hashlib`, `mod-dataclasses`, `mod-operator`, `mod-copy`, `mod-heapq`,
`mod-bisect`, `mod-string`, `mod-textwrap`, `mod-enum`, `mod-abc`,
`mod-binascii`, `mod-time`, `mod-os`, `mod-pathlib`, `mod-fnmatch`,
`mod-posixpath`, `mod-csv`, `mod-io`, `mod-gc`, `mod-types`,
`mod-contextlib`, `mod-traceback`, `mod-logging`, `mod-urllib`,
`mod-pprint`, `mod-shlex`, `mod-difflib`, `mod-struct`, `mod-html` (with
`html.parser`), `mod-warnings`, `mod-codecs`, `mod-platform`,
`mod-tempfile`, `mod-decimal`, `mod-numbers`, `mod-contextvars`,
`mod-fractions`, `mod-statistics`, `mod-tomllib`, `mod-calendar`,
`mod-hmac`, `mod-zlib`, `mod-zoneinfo`, `mod-tokenize` (with `token`;
mm-engine feature `tokenize`), `mod-xml` and `mod-email`. Optional:
`mod-uuid`, `mod-unicodedata`, `mod-html-entities` (32,470 bytes of bytecode
[M]) and `mod-ast` (mm-engine feature `ast`; about 48 KB of `hello.wasm`
[M]). `mm` enables all four, so `cargo xtask check` runs their own cases,
conformance and durability.

`mod-asyncio` and `mod-json` gate the engine's own modules (mm-engine
features `asyncio` and `json`): their natives, prelude units and, for
`asyncio`, the scheduler's task, gather, sleep and loop paths. The facade
forwards every `mod-*` feature and its `default` feature is the default set
(mm-stdlib's own default is empty). `math`, `cmath` and `sys` are engine
modules, always built, with no feature. Module features are part of the
build id (§9.3). Each `mod-*` feature turns on every module its modules
import when they are imported; mm-stdlib's build script reads the bundled
modules and `Cargo.toml` and fails on a missing one, so every module set the
manifest allows imports cleanly. Imports inside functions are not part of
the graph, so these need the second feature listed too: `time.strptime`
needs `mod-datetime` (which itself needs `mod-time`, so declaring it would
make a cycle), `Fraction.from_decimal` `mod-decimal`, `os.path.expandvars`
`mod-re`, and `types.coroutine` on a plain function `mod-functools`.
`cargo xtask check` builds the engine with no module features and the
facade with none, and runs `crates/micro-monty/tests/features.rs` on a
facade that opts out of `asyncio` and `json` and into `ast`.

### Native or Python: the hybrid stdlib

- **Rust:** builtin types and their methods (`str`, `bytes`, `list`, `dict`,
  `set`, sorting), and modules whose cost grows with input size or that every
  other module calls: `json`, `re`, `hashlib`, `base64`, `math`, `struct`,
  `random`'s generator, and `datetime`'s string parsers (the exception
  below).
- **Python:** API surface and logic, adapted from CPython's pure-Python modules
  where they exist (`_pydatetime` formatting helpers, `collections` extras,
  `functools`, `itertools` glue, `heapq`, `bisect`, `textwrap`, `string`,
  `fnmatch`, `shlex`, `difflib`, `statistics`, `fractions`, `copy`, `pprint`,
  `dataclasses`, `enum`). `dataclasses` and `namedtuple` build methods
  without `exec`/`eval`. PSF license; notices in `THIRD_PARTY.md`.
- Python modules live in `crates/mm-stdlib/python/`, are compiled to bytecode
  at build time, embedded in the crate (§6), and imported lazily, so session
  start stays near 1 ms.
- A module moves from Python to Rust when a benchmark shows it hot.
- **Exception: `datetime`'s parsers.** `datetime` is Python because its
  work is not data-proportional, but `date`/`time`/`datetime.fromisoformat`
  and `strptime`'s matching are native (`mm-engine/src/datetimemod.rs`,
  feature `datetime` from `mod-datetime`, as `_mm.dt_iso` and
  `_mm.dt_strptime`): agents parse timestamps in loops, and these were the
  slowest code-mode cells (per call, `fromisoformat` about 100–175× CPython
  in Ir and `strptime` 2.6× in Python [M]). `fromisoformat` ports CPython
  3.14.7's C parser line by line over the UTF-8 bytes, so results and
  errors are CPython's; `strptime` takes the common directives (`%Y %m %d
  %H %M %S %f %z %y %b %B %a %A %p %I %j %%`) and returns `None` for
  anything else or any error, and the Python `_strptime` (CPython's own
  Python code) takes over. Python still raises and builds the objects,
  skipping the constructors' checks for fields the parser checked. Both
  charge fuel per input byte, run between safepoints and add no snapshot
  state. Cost: +4,668 B (`fromisoformat`) and +9,153 B (`strptime`) of
  `hello.wasm` [M]. Aware arithmetic and comparison add only `_mm.dt_ord`
  and `_mm.dt_ymd`, the ordinal of a valid `(y, m, d)` and its inverse,
  constant-time on ints and `None` for anything else; the aware paths
  themselves stay Python, rewritten after CPython's C, so `tzinfo`
  subclasses run as ordinary calls. The rest of `datetime` (formatting,
  `strftime`) is Python.

Why: models should never hit a missing module, and hand-writing every module
in Rust (Monty's approach) makes the long tail the schedule. Python-written
modules are pausable for free, carry CPython's semantics, and bytecode is
denser than machine code. The cost is per-call speed. CPython 3.12.14's own C
modules against its pure-Python versions [M]:

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

`re`: an own backtracking engine with fuel (QuickJS libregexp ideas);
lookaround, backrefs, Unicode `\w\d\s\b`; `\p{..}` rejected as in CPython.
`re.Match` is a native type (`re.rs`), and `Pattern.match`, `search` and
`fullmatch` build it in one native call (`_match`) that runs one step
slice; a longer run, or arguments it does not take, continue in `re.py`'s
`_exec` loop from the state that slice saved, a safepoint per slice. A
search skips the start positions where none of the single-width items a
match can begin with matches, a step per char.

### Speed

Targets [E], against reference numbers for the other engines [M]:

| Bench | micro-monty target | Monty 1.4 | QuickJS-ng | CPython 3.13 |
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

Measured [M, `cargo xtask bench`, median ms per process including startup,
one untimed warm-up, AMD Ryzen 9 8945HS, pinned with `taskset -c 2`;
`bench/results/2026-11-07-host1-ryzen-x86_64-linux-b01fd7d/`]. The first
seven programs are the target rows; the other six (classes, generators,
exceptions, fmt, sort, interp; `bench/README.md`) guard against overfitting
them:

| Program | micro-monty | CPython 3.14.7 | QuickJS | Monty 1.4 |
|---|---:|---:|---:|---:|
| startup | 1.1 | 7.4 | 1.4 | 1.8 |
| fib | 16.0 | 19.7 | 14.0 | 71.9 |
| loop | 43.1 | 134.1 | 52.0 | 215.0 |
| dict | 68.4 | 75.8 | 72.7 | 133.3 |
| str | 30.7 | 29.1 | 24.9 | 48.4 |
| json | 67.2 | 111.9 | 129.3 | 164.5 |
| closure | 29.9 | 57.4 | 28.4 | 143.4 |
| classes | 62.0 | 57.8 | 53.8 | 306.9 |
| generators | 118.3 | 56.5 | 74.2 | fails |
| exceptions | 55.2 | 43.4 | 96.6 | fails |
| fmt | 101.0 | 57.4 | 51.6 | 123.3 |
| sort | 76.6 | 62.0 | 126.5 | 212.1 |
| interp | 115.6 | 57.3 | 111.3 | 293.6 |

micro-monty beats Monty on every row and is within 1.15× of QuickJS on
fib, loop and closure, so the worth-switching bar is met on that machine;
it trails CPython on generators, fmt and interp (about 2×).

**Instruction counts.** `bench/callgrind.py` counts each program's Ir under
callgrind twice, on the shipped native `mm` (`target/<host>/release/mm`,
built by `cargo xtask shipped-mm`; the script stops if it is missing). The
*gated* count runs from glibc's `__libc_start_main` onward and leaves out glibc's `pthread_getattr_np`; the *whole-process*
count includes everything. A change may raise the gated count of a
program by at most 0.5% (`startup` by at most 2,900 instructions) over
the base and over the anchor (its lowest count since the last `v*` tag),
unless a commit says why with the table on a `Native-IR-Accepted:
<programs> <ceiling> <reason>` line, which passes only those programs up
to that ratio; a drop passes. `cargo xtask native-ir` checks it (in
`check`, Linux), building base and anchor history with their own
toolchain and, where their xtask has `shipped-mm`, their own recipe; the
whole-process count is
reported beside it. The two parts left out depend on where the binary's segments land, not
on engine work: the dynamic loader's relocations, and `pthread_getattr_np`
parsing `/proc/self/maps`, which Rust's runtime calls from inside `main` to
find the main thread's stack. That is why counting from `main` alone is not
enough: one layout-only change moved `startup` +0.80% whole-process, +1.06%
from `main` and +0.03% gated [M]. On the other programs the two counts
differ by a constant of about 337K Ir, 0.10% of the smallest [M].

## 13. Tests and conformance

Every gate is local: GitHub runs none. `cargo xtask check` (the landing
tier) and `cargo xtask check --release-gate` run them; `AGENTS.md` says
which tier a change needs and lists the commands. A `v*` tag runs only
`.github/workflows/release.yml`, which builds the release artifacts and
opens a draft release.

- **Conformance corpus:** `conformance/monty/` holds Monty's 726 test cases
  at `66fff6bf` (MIT; notice in `THIRD_PARTY.md`). `cargo xtask
  conformance` understands Monty's `# /// monty` headers and
  `# Return=`/`# Raise=`/`TRACEBACK` expectations, scores each case through
  `conformance/oracle.py` against CPython 3.14.7 (the oracle,
  patch-pinned because patch releases change error text) and tags it
  `cpython`, `host`, `monty-specific` or `excluded` (the 69 `refcount__*`
  cases). `call-external` cases run with `mm exec --host-fixtures`, mm's
  emulation of the datatest's host functions; `mount-fs` cases are skipped.
  `--engine micro-monty` writes `conformance/results/micro-monty.jsonl` and
  `.md` (currently `cpython` 546 of 568, `host` 62 of 81, `monty-specific`
  2 of 8 [M]); `--check` fails on any change, a newly passing case included
  until it is recorded. The corpus also runs on `mm` built for
  `wasm32-wasip1` under wasmtime (`--target wasm32-wasip1 --check`), which
  must give the native results case for case: one engine, whatever the
  target.
- **Own tests** (`conformance/own/`) for what Monty lacks: inheritance,
  generators, async iteration, lazy genexprs, exception subclasses, parking,
  the stdlib modules, and a park inside every chunked operation (§5.1).
  `crates/mm-cli/tests/conformance_own.rs` runs them against CPython, also
  on a natively built shipped `mm`.
- **Parser parity:** `cargo xtask parse-diff` (§6).
- **Differential fuzzing:** a grammar-based generator of subset programs
  (`cargo xtask difffuzz`), run on micro-monty and CPython; plus cargo-fuzz
  targets for the parser and compiler, the VM on mutated code, the snapshot
  reader, `re`, `json.loads`, deeply nested source and data (§5), arena
  operations, the mm-wasm codec, the memory bound and the step-time oracle
  (`run_source`). No panics, no stack overflows, no hangs past fuel.
- **Error quality:** traceback cases compare micro-monty's traceback text,
  including carets and hints, with CPython 3.14's.
- **Durability:** `cargo xtask durability` runs every own case and every
  corpus case `micro-monty.jsonl` records as passing through `oracle.py
  --durability`: once unparked, and once parked every few fuel units and at
  every host call, each park saved to a FULL snapshot by one `mm` process
  and continued by a new one. It compares each mode's report (stdout, the
  result, the exception), fuel total and final-state digest with the
  unparked run's, and the restore chain's with the session's that took the
  same snapshots and went on (§5.1 clause 4); a difference
  `conformance/durability-results.txt` does not record fails. Every case
  parks at `5:50` (every 5 units, or every ⌈fuel / 50⌉ for a long case); the
  own cases and every third corpus case also park at `1:100` in the landing
  tier and `1:1000` in the release gate.
  `--cross-target` runs the own cases with every snapshot restored on the
  other target: the chain alternates between the native `mm` and `mm` for
  `wasm32-wasip1` under wasmtime, once starting on each.
- **Golden images:** images an earlier build wrote restore and continue
  exactly (§9.3).
- **Real sessions:** `conformance/codemode/real/gate.py --check` replays 160
  recorded plak sessions (2,841 cells) through `mm_cell_worker::serve`,
  answering each nested call with its recorded result, and fails if any
  cell's outcome, output, exception, value or nested-call sequence moves
  from `results/gate.jsonl`.
  `conformance/codemode/run.py --check` (release gate) runs the synthetic
  code-mode tasks on CPython and mm.
- **Memory and steps:** the memory probes (§11) and the step-time oracle
  (`crates/micro-monty/tests/steps.rs`, §5.1, built fully as shipped in
  `target/check/steps`: its fuel half beside the
  other stages, its wall-time half last, alone).
- **GC stress** (not in `check`): `cargo xtask gc-stress` reruns the own
  cases, the corpus and durability collecting at every safepoint
  (`--incremental`: an incremental mark in tiny slices at every safepoint,
  each checked against a full mark); output must not change.
- **Size gate** (`cargo xtask size`, in `check`): the shipped builds against
  the base's and the release's (§6).
- **Bench gate:** `cargo xtask bench --quick --engines cpython,micro-monty`
  (in `check`) runs every bench program on both engines and requires every
  checksum, warm-up included; a Python exception is a regression. The full
  `cargo xtask bench` times the engines it finds and writes
  `bench/results/`; run it alone on an idle machine. Native Ir follows the
  gated-count rule (§12): `cargo xtask native-ir` (in `check`, Linux) fails
  when a program's gated count rises past 0.5% (`startup` past 2,900
  instructions) over the base's or the anchor's.
- **wasm-ir** (in both tiers, Linux only, since it needs valgrind):
  `cargo xtask wasm-ir` builds the working tree (or `--head REV`), the base
  (`--base REV`, default `origin/main`) and an anchor, each in a worktree
  under `target/wasm-ir/`, as shipped: the wasip1 `mm` precompiled by
  `wasmtime compile --target x86_64-unknown-linux-gnu` (baseline ISA, so
  every machine compiles the same code). It counts each bench program once
  under callgrind: the instructions at addresses in wasmtime's perf map
  (the guest's compiled code), plus the inclusive cost of wasmtime's
  `memory.copy` and `memory.fill` libcalls (share: str 6.3%, json 4.2%,
  dict 3.6%, fmt 3.1%, sort 2.2%, generators 1.4%, under 1% elsewhere, none
  on fib, loop or closure [M]), so a loop turned into a bulk copy does not
  read as a gain. The anchor is the latest `v*` tag the base descends
  from, or the latest commit since it whose message has a
  `Wasm-IR-Accepted:` line (`--anchor REV` names another); a checkout
  without the tag fails and says to run `git fetch --tags` or pass
  `--anchor`. The anchor's count for each program is the lowest among the
  anchor, the base and every first-parent commit between them that
  changed build inputs (crates, `Cargo.toml`, `Cargo.lock`,
  `rust-toolchain.toml`, `.cargo`; tests, `mm-wasm-host` and `*.md`
  excluded), so after a gain later rises count from the new level; this
  bounds creep from commits that each stay under the limits. Limits,
  against base and anchor alike: a program rising past 1.0%, the geomean
  of the ratios (without `startup`) past 0.5%, or `startup` (about 0.9M
  instructions) rising by more than 50,000 instructions; decreases always
  pass. `MM_WASM_IR_ACCEPT=<reason>` passes an intended rise; the commit
  message then carries the table and a `Wasm-IR-Accepted: <reason>` line.
  A revision's counts are cached in `target/wasm-ir/cache/`, keyed by the
  counting method and the wasmtime, wasm-opt and valgrind versions, and
  each program's count by a hash of its `bench/*.py` source. The first run
  after a tag or an accepted commit builds the anchor, the changed commits
  since it and the base (about 5–6 min in `check` on 16 cores); a cached
  rerun of one program takes 7 s [M]. Calibration [M]: the 0.0.1
  Cranelift spill fails it (fib 1.080, closure 1.102, geomean 1.042).
- **v8-ir** (release gate only, Linux): `cargo xtask v8-ir` counts V8's
  instructions on the bench programs (`startup` left out) in both
  `wasm32-unknown-unknown` guests, plain and bundle, which are built with
  other `wasm-opt` flags than the wasip1 `mm` wasm-ir counts. Each program
  runs in its own `node --no-liftoff` process under callgrind, twice in
  fresh sessions (`hosts/js/bench/count.ts`); the count is the second
  run's instructions in code without an object file (callgrind's `???`):
  V8's generated code. The first run holds V8's lazy compiles. A revision
  whose xtask has `size --head` builds its guests with its own recipe, so
  a change to the `wasm-opt` flags or the `optimize(size)` marks is
  counted at its commit; older ones are built with the head's recipe.
  Base and the anchor's least-of history work as wasm-ir's, with the last
  `v*` tag as the anchor. An accept line `V8-IR-Accepted: <names>
  <ceiling> <reason>` (programs or `geomean`, `plain/` or `bundle/` before
  one for one guest) passes only those counts up to that ratio, and later
  runs re-anchor only those; `MM_V8_IR_ACCEPT` stands for it in an
  uncommitted tree only. Limits, on either guest
  against base and anchor: a program rising past 0.5% or the geomean past
  0.3%; decreases pass. Counts are cached per guest content, keyed by
  node's and valgrind's versions, the driver and the program's source, so
  only a guest whose bytes changed is counted again. Stability [M]: the
  same guest counts within 0.024% run to run; unrelated commits move a
  program by 0.00–0.06%. Valgrind runs no AVX-512, so this is V8's AVX2
  code.
- **wasm-wall** (advisory, not in `check`): the count cannot see
  host-tuned layout costs (AVX-512 precompiles), so `cargo xtask wasm-wall`
  times the bench programs on wasmtime and V8 (`node --no-liftoff`, steady
  state) in alternating rounds against a base and fails on a geomean of
  median ratios above 1.03 or a program above 1.05 with its lower quartile
  above 1. Run it alone, and before a release tag.
- **wasm hosts:** `cargo xtask wasm` runs the shipped `roundtrip` example
  (park, snapshot, restore, finish) on both wasm targets under wasmtime;
  `cargo xtask wasm-host` (release gate) runs `crates/mm-wasm-host`'s
  tests against the shipped guests: parity with the native facade on every
  `mm-wasm` export, and images crossing native and wasm. `hosts/js` has its
  own tests (`npm test`), outside cargo.

## 14. Sources

- **Monty** (<https://github.com/pydantic/monty>, MIT): the inspiration
  and the source of the conformance corpus (`crates/monty/test_cases/` at
  main `66fff6bf`, copied into `conformance/monty/`), the subset list
  (`docs/limitations/`), error wording and "not supported" messages, and
  the comparison engine of §12 (release 1.4.0). Its value enum, heap,
  refcounting, serde snapshots, regex and bigint crates and protocol are
  not reused. Monty's helpers for the datatest are vendored unmodified in
  `conformance/monty-scripts`.
- **CPython 3.14.7**: the conformance oracle (installed by `.agents/setup`
  through uv; `PYTHON` overrides). Its pure-Python stdlib modules
  (`Lib/_pydatetime.py`, `Lib/heapq.py`, `Lib/bisect.py`,
  `Lib/collections/__init__.py`, `Lib/json/`, …) are the base of the
  Python-written stdlib, under the PSF license; PEP 659 (inline caches),
  the compact ordered dict and C3 MRO
  (<https://www.python.org/download/releases/2.3/mro/>) shaped the VM.
- **plak** (<https://github.com/N0xMare/plak>, MIT): an open-source local
  agent harness by micro-monty's author, whose worker processes run
  code-mode cells in JavaScript on QuickJS and in Python on Monty. It is
  micro-monty's integration target: `mm-cell-worker` speaks its cell
  protocol, takes its Python worker's limits and messages as defaults and
  is tested against its worker tests, and its recorded sessions are the
  real-session corpus (§13).
- **QuickJS** (<https://bellard.org/quickjs/>), quickjs-ng and rquickjs:
  the size philosophy (builtins as `JSCFunctionListEntry` tables, own
  regex, unicode and bignum); libregexp's ideas behind `re`.
- **dquickts** (<https://github.com/stencil-hq/dquickts>): suspension and
  snapshots for QuickJS, the comparison point of §9.1
  (`js_can_suspend_from` in `quickjs.c`).
- **ruff** (`ruff_python_parser` 0.0.14, crates.io): the parse-diff oracle
  behind `mm-parse`'s `ruff-oracle` feature.
- **Dispatch:** Shi, Gregg, Beatty, Ertl, "Virtual Machine Showdown: Stack
  Versus Registers" (VEE 2005,
  <https://www.usenix.org/legacy/events/vee05/full_papers/p153-yunhe.pdf>;
  TACO 2008); Rohou, Swamy, Seznec, "Branch Prediction and the Performance
  of Interpreters: Don't Trust Folklore" (CGO 2015); V8 Ignition.
- **Size floors:** MicroPython, PocketPy.

MIT and PSF notices for what is taken from Monty (tests, text), QuickJS
(algorithm ports), ruff (oracle dependency) and CPython (modules) are in
`THIRD_PARTY.md`.
