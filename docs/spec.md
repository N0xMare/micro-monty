# micro-monty: architecture

Status: released as 0.0.3; this file describes `main`. Owner: n0x.

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
  snapshots like the rest. Nothing finalizes one that is abandoned unfinished
  when it is collected (there are no finalizers); `asyncio.run` closes
  those it leaves unfinished when it ends (§8.5), and `anext(it, default)` is a `coroutine`, not an `anext_awaitable`.
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
  hint needs a traceback, `AttributeError` and `ImportError` hints do not,
  and neither does micro-monty's `. Did you forget 'await'?` (§8.4);
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
- A reset is a boundary: nothing a cell does reaches the cells after
  `Session::reset`, which report what they report in a new session with
  the same host definitions. What every reset of a session shares is
  therefore read-only to guest code, where CPython lets its Python-level
  objects be changed:
  - the functions and classes the prelude defines (the native modules'
    Python parts: `asyncio.Lock`, `asyncio.Future`, `json.dumps`,
    `sys.exit`, ...) and the host's definitions (typed host functions,
    host classes and host exception classes, with their methods, and the
    classes made for the host's records, `HostValue::Record`: a
    namedtuple class is writable in CPython, but a record a host function
    keeps as a default goes back to the host by its class's name and
    `_fields`; a reset drops the classes, so a record after it is of a
    new class of that name, as in a new session). Setting or deleting an attribute, also through `setattr`,
    `object.__setattr__` or `type.__setattr__`, raises CPython's error
    for a builtin function (`AttributeError: 'builtin_function_or_method'
    object has no attribute 'x' and no __dict__ for setting new
    attributes`) or an immutable type (`TypeError: cannot set 'x'
    attribute of immutable type 'Entry'`), the same for a deletion. A
    function's `__dict__` raises `AttributeError`; a host definition's
    `__defaults__` and `__kwdefaults__` do too, as a builtin function's,
    and a prelude function's `__kwdefaults__` is a copy; a class's
    `__dict__` and `__annotations__` are copies. A typed host function is
    named `builtin_function_or_method` in these and every other message,
    and `dir()` lists a builtin's names for it. An untyped host function
    refuses attributes as before. A native method called through its type
    on such an object refuses it as CPython's descriptor does
    (`BaseException.add_note(read, 'n')`: `TypeError: descriptor
    'add_note' for 'BaseException' objects doesn't apply to a
    'builtin_function_or_method' object`), an exception's `__init__`
    included (CPython's message names the class the `__init__` is of,
    this one `BaseException`). The prelude's own methods are refused the
    same writes: `self.x = v` in one, called through its class on such
    an object (`json.JSONEncoder.__init__(json.JSONEncoder, default=f)`,
    which CPython allows), raises as the statement does in a cell; only
    a prelude unit's top level writes them. `type(read)` is still
    `function` and `repr(read)` a function's.
  - asyncio's current loop object, the one `asyncio.set_event_loop` set
    and the one that stands for the running loop, is forgotten by a
    reset.
  - A class keeps a list of its subclasses, to give them new versions
    when it changes. A class a reset shares never changes once shared,
    and keeps no list: a cell's subclass of one is freed like any other
    object, by a reset or when the cell drops it. A deleted subclass of
    a guest's own class stays held by its base's list while the base
    lives (CPython's list is weak).
  - Not covered: `id()` and the address in a default `repr` after a
    reset depend on where earlier cells left the heap; they are equal
    to a new session's only by chance. An exception's `__init__` called
    through another exception class than the instance's
    (`OSError.__init__(ValueError('v'), 1)`) is accepted where CPython
    raises `TypeError`; `type.mro()` with no argument returns a list,
    and a dunder that is a method descriptor in CPython
    (`dict.__getitem__()`) has the slot wrapper's `descriptor ... needs
    an argument` text.
  - `sys`'s struct sequences (`version_info`, `float_info`, `hash_info`,
    `flags`, `int_info`) refuse attribute writes with CPython's errors
    (`readonly attribute` for a field, `... has no attribute 'x' and no
    __dict__ for setting new attributes` otherwise) and have no
    `__dict__`.

  Their instances, subclasses and the modules' own attributes
  (`json.dumps = f`) stay writable: a reset drops the modules, so the next
  import has the originals. A reset also restores the recursion limit and
  the limit `sys.set_int_max_str_digits` set, builds the builtins
  module's constants that are not scalars again from the host's
  definition (a list or dict a cell changed in place, a record a cell
  set attributes on). What a reset keeps counting is not
  program state: host call ids and `Task-N` numbers continue, the entropy
  source goes on, and code loaded for earlier cells stays loaded (an
  import charges less fuel the second time). `Session::reset_to` and a new
  session share nothing with the cells before. Checked by
  `conformance/reset`: probes, and a walk that writes to everything a cell
  can reach by attribute and item from the builtins, the host's
  definitions and every importable module, then looks for the writes
  after a reset; on the native facade and both mm-wasm hosts.
- A cell ended where it stood (cancelled, replaced by the next cell or
  reset while stopped or parked) inside a container's `repr` leaves no
  container marked as being rendered.
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
    Linux's seccomp filter (`prctl`, `seccomp`, and on musl `gettid`,
    which takes no argument: the filter names the installing thread as
    one musl's `abort` may signal with `tkill`) and read `getrusage`. Its
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
  or dict's table rebuild charges the probes past each key's free runs
  (`FREE_RUNS`) or slots (`FREE_SLOTS`), the same however addresses fall,
  and ends a step when the slots read, those probes and the free runs or
  slots of each key moved (a constant a key, uncharged) pass its budget,
  so a table of colliding keys rebuilds in steps and every step ends at
  the same key whatever a snapshot moved; a dict's growth in one step
  counts the free slots its entries walked instead, so a growth that
  walks long chains (memory too tight to grow in steps, say) goes on in
  steps, as before this allowance; where it ends then follows how far keys
  whose hashes follow addresses walked. A set's slice charges what
  its rebuild in one step would, the probes past the free runs, and asks
  for a clock check after it (`force_check`), since a step that charges 64
  units or fewer reaches none (`Vm::burn`); a dict's slice charges the
  entries it reads and the probes past the free slots. A lookup that meets a key only Python can compare
  stops just past it (`dict_probe_py`, `set_probe_py`) and the twin
  (`_dfind`, `_sfind`) hands the compare's truth back with the place: as
  in CPython's lookups, a compare that replaced the table or the compared
  slot's key starts the lookup again (only the same table goes on; CPython
  goes on in a new table its allocator put in the old block, which mm
  does not imitate), an equal key is found, and an
  unequal one goes on from the place over the table as it now is; an add
  takes a key whose compare was true before that check (`set_add_entry`).
  No lookup probes again after a miss.
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
  plus 0.75 ms per MiB of the memory limit, in wall time and in epoch
  ticks the guest runs through) waits at most that long on one. It is not an invariant: full collections (§5.1 clause 2) and sites
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
  - **Cancellation.** `cancel()` walks its tree twice (§8.5): a first pass
    that changes nothing, charges a fuel unit a visit and counts the
    run-queue room the settles need, one reservation, then a second pass
    that acts and charges nothing. Both passes in one step when they fit
    `aio::CANCEL_STEP` (2^14) units of work, with no state and the fuel
    of a cancel in one step. A unit is a visit that settles nothing
    costly; a visit costs more by what its settle scans (a future's
    waiters, 8 a unit; the blocked tasks for each task it wakes and for a
    sleep; the tracked gathers for each gather that listens), so a step
    is bounded in work, not in visits: cancelling 30,000 sleeping tasks
    was one step of 1.66 to 1.88 s at 16,384 visits' worth of fuel [M].
    A longer cancel keeps its frames (as deep as the tree: node, next
    child, took) in a buffer of an `op::CANCEL` state and goes on a step
    at a time, each followed by a forced check (`Vm::force_check`: the
    second pass charges nothing that would make one). The program's goes
    through `_steps`, whose calls are its only extra fuel (a unit or two
    a step). The scheduler's own cancels, for a task that blocks with a
    cancel pending and for the main task when `SystemExit` or
    `KeyboardInterrupt` leaves a task, cannot hand work to `_steps`: the
    state is `Vm::cancelling` (in the image), and `Vm::next_task` runs
    its steps before any queue entry, a safepoint with the trampoline's
    state after each, at the same fuel as one step. Nothing else runs
    between two steps of a cancel: guest code, callbacks and other tasks
    see the tree before the first pass's end or after the second's. A
    restored step reads each frame's children from the tree again and
    checks the frame against them (the node is the pending child its
    parent visited last, the depth within the first pass's, the
    callbacks within its count), so a forged or stale state raises
    `SystemError` (the scheduler's at the main task's `await`) and
    never panics. A step is bounded between visits, not inside one: a
    visit settles its awaitable whole.
  - **The scheduler's lists.** A wake and a settle do not search the
    blocked tasks or the tracked gathers: a blocked task holds its place
    in `Vm::blocked` (`Task::at`) and a tracked gather its place in
    `Vm::gathers` (above its flags, `aw::PLACE_SHIFT`), checked against
    the list where it is read, so a place alone is never trusted. Taking
    one out leaves a hole; a list is packed, in order, when it is full or
    its holes outnumber its entries, and before a sleep's settle scans
    the blocked tasks (a sleep lists no waiters). An image holds neither
    holes nor places: `prepare_snapshot` packs the gathers and takes
    their places out, the writers put them back, and a restore sets them
    from the list, so the session that goes on and the restored one
    differ in nothing an image, a report or fuel shows (a cancel's
    budget counts entries, not holes). The loop's walk for the host
    calls a blocked program waits on (`Vm::pending_ids`) marks the
    gathers it reaches (`aw::VISITED`, cleared before it returns). On
    shipped codegen: 30,000 sleeps due together, one step of 141.9 ms,
    now 11.9 ms; a future 30,000 gathers listen to, 199.8 ms, now
    4.7 ms; a loop idle on a gather of 40,000 futures, 233.0 ms, now
    7.2 ms; cancelling 40,000 tasks each waiting on a future, 631 ms in
    all, now 86 ms [M]. Cancelling many sleeping tasks is still
    quadratic in them in all (each sleep's settle scans the blocked
    tasks), in bounded steps. Not bounded yet: a step that starts many
    tasks or builds a gather of many children.

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
   epoch grace, 100 ms plus 0.75 ms per MiB of the memory limit in wall
   time and in epoch ticks the guest runs through (time its thread is
   held counts one tick a stretch), about 1.5–3 times the native time
   [E]. On V8 (Node, workerd) the clock does
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
   0.5–2. The first probe runs before any row: a machine past 2 is probed
   again every 15 s, and if it is still past 2 once the oracle has waited
   5 minutes in all, the oracle fails at once as unable to judge it, before
   the rows' quarter of an hour. A
   `Bounded` row over that limit runs again after every other row has run,
   once a round for up to six rounds, so its retries spread over the host's
   slow phases (1.6–2 times, outlasting three runs back to back [M]). A
   probe runs between retries, each retry is judged against the limit from
   the faster probe beside it, and the row passes if any run is under its
   limit: a load spike only lengthens a step. A retry runs only after a
   probe at or under 2, waiting as at the start, so a machine that slows
   past 2 after the limit was set judges no retry against a limit it
   cannot scale to; if the wait runs out, the rows still over are reported
   as not judged and the oracle fails as unable to judge the machine, not
   on a row.

   A collection the heap starts itself is not a native step: over 32 MiB
   live it marks a slice per safepoint, and under it it runs in full over
   at most about 64 MiB (30 ms native at worst [M]; §9.2). A park that is
   due goes ahead of a collection step, which runs when the session
   resumes, so one gap never holds both a native's step and a collection
   step (§9.2). Full collections over a large heap stay outside this bound
   (0.40 ms per MiB of live heap native, 1.3–1.6 times that on wasm [M]):
   one a native or the engine requests (`Vm::request_gc`: after a
   `MemoryError` is raised, after a byte job fails for memory), a
   snapshot's, code reclamation's, `Session::compile`'s for a cell that
   does not fit, and the one a cell start runs on a heap at its wall
   (`Vm::prepare_cell`, §9.2). The one a large result requests
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
   object gets depends on when collections run, and that depends on
   snapshots, GC stress and the session's limits (not on where fuel
   slices or deadlines end, §8.6). So `id()` values, default `repr`s
   (`<... object at 0x...>`) and the order of sets of identity-hashed
   objects can differ between runs snapshotted differently [E]. What
   holds: `id` is unique among live objects, `id(x)` is the same for
   `x`'s whole life across parks and restores, `is` is unaffected, and
   containers with value-hashed keys keep their order.
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
also on `plakous-cell`), and both, on musl only, on `mimalloc`, which
they register as their global allocator there (musl's `malloc` maps and
unmaps every block of 128 KiB or more; docs/user-guide.md, "musl"): C,
compiled with musl's compiler driver, in no engine crate and not in the
facade, whose only use of it is a musl-only dev-dependency for the step
oracle (§13); `mm-wasm-abi` on the facade and `mm-wasm-guest` on
both and on `mm-wasm-alloc`. The wasmtime reference host,
`crates/mm-wasm-host`, is its own workspace (excluded from the root), so
wasmtime never enters the root lockfile; `hosts/js` is a TypeScript host for
the same guest (Node, workerd), outside cargo.

`micro-monty` and the five crates it is built from are published; they pin
each other with `=` requirements, as one engine, and the five internal
crates have no API of their own to keep.

**The `mm-wasm` interface.** A host checks `ABI_VERSION` (`mm-wasm-abi`,
currently 3) and the required export list before it runs anything, and
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
bound (647 functions: Ir within 0.22%, `steps` failed as shipped [M]), and a
marked function no Ir workload runs can slow down unseen (an `await` loop
+17% and `gather` +21% with every gate green [M]). So a commit that adds
marks under `crates/` carries a trailer `Size-Marks: <n> added, <run> run
by an Ir workload (<how>)`, with `<n>` the marks it adds (a mark is its
attribute with the function it marks, so one moved to another function or
given another condition is added) and `<run>` at least 1; `cargo xtask
size` fails without it, per commit (it reads the statement, it does not
count the runs). A mark that slows only the wasm guests is native
only (`cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), …)`),
including one that slows a function it does not mark: `Vm::arg_o` gets the
stack's fields instead of `&Vm` only while LLVM proves every caller's `&Vm`
valid. A mark on for wasm only is written
`cfg_attr(all(mm_shipped, any(target_arch = "wasm32", mm_size_wasm)),
optimize(size))`, so the native replay worker `cargo xtask replay-ir`
builds with `--cfg mm_size_wasm` runs it too; `cargo xtask size` fails on
one written otherwise. Tests,
clippy and every other build stay on plain stable; `cargo xtask wasm`
builds its examples as shipped too. Native binaries stay
position-independent (the worker runs untrusted code). Shipped wasm modules
then go through binaryen's `wasm-opt -Oz` (version 133, installed by
`.agents/setup`) with exactly the features rustc enables and, on
`wasm32-wasip1` only, inlining capped at 16 KiB per function
(xtask/src/wasm.rs `wasm_opt_args`, the arguments' one owner), into
`target/<target>/release/shipped/`. It saves about 145 KB
on `hello.wasm`; plain `-Oz`, before the cap, cost 8–13% wall time on the
dispatch-heavy bench programs (fib, loop, closure) under wasmtime [M], and
the cap takes 4.9% of fib's and 7.5% of closure's guest instructions back
(bench geomean 0.975) [M]; every `wasm-opt` level
measured slower than none, and `-O3` is within a few percent of `-Oz` and
14 KB larger [M].
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
more, needs a git trailer `Size: <artifact> <±bytes> <why>` naming the
artifact in a commit since the base, as its reason; the measured net
change is what is judged, so the trailers' bytes need not add up to it
(owner, 2026-11-19);
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

**Sizes at v0.0.3** [M, the release commit on `0c970b8` and its landing, Rust 1.100.0,
binaryen 133, `cargo xtask size`]:

| Artifact | Bytes |
|---|---:|
| `hello.wasm` (`wasm32-unknown-unknown`, after `wasm-opt -Oz`) | 2,259,536 |
| native `hello` (x86_64, PIE) | 2,451,824 |
| `mm_wasm_guest.wasm` | 2,355,222 |
| `mm_wasm_guest_bundle.wasm` | 2,354,493 |

At v0.0.1 (`1cfcde3`; not re-measured since), `hello.wasm` was
1,811,762 bytes of code and 455,312 of data. Code by crate: mm-engine
1.02 MB, mm-core 317 KB (almost all inlined heap accessors), mm-compile
149 KB, std 141 KB, mm-parse 112 KB. The 65 stdlib modules' packed
bytecode was 281,480 bytes (12.4%), the prelude's 57,122. A build with no
stdlib modules (`struct` and `operator` stay) was 1,808,788 bytes of wasm
and 1,996,400 native; a `host-bundle` `hello.wasm` (every capability, no
bytecode) was 1,938,038. `uv run --with pyelftools python3
xtask/size_map.py` maps the shipped builds by section and crate into
`target/size-map/report/size-map.md`. The `mm-cell-worker` binary is larger
(serde, tokio) and is not what the budget measures.

**Bundled bytecode.** The bundled modules are written by
`Program::encode_sections` as nine byte streams: own strings, string
references, other metadata, opcodes, registers, `u` operands (inline-cache
slots predicted), and `b`, `i` and `j` operands. Strings that three or more
modules use are stored once, raw, and made `Arc`s only when a decode uses
them. Each stream is packed against one set of Huffman codes for every
module (`pack_shared`). The decoder tables of each header (nine for the
modules, ten for the prelude's parts) are built once and kept in the
facade `Engine`'s cache (`pack::Tables`, about 90 KB for all of them [M]),
else by the session for one cell or one restore; each unpacking is still
charged its decoder workspace, so a session's charges do not depend on
whether the tables existed. A restore unpacks a program's instruction
sections only when its code first runs, and then decodes the whole
program again (§9.3). The prelude is packed the
same way, in chunks of units (one per top-level `def`, class, instance or
literal binding), and a unit is built in a session on the first lookup of
its name (`mm-engine/src/prelude.rs`); no prelude Python runs at session
start. The stdlib and the prelude are
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
  block fits where a temporary was (`prefix + str(i)`). A live block keeps
  that room across a sweep (a free run after it no longer than the room is
  not listed). Bigger free blocks sit on classes of the same 64 sizes per
  doubling; a placement tries a few blocks of its own class, then the next
  non-empty class, where every block fits, and walks the rest of its own
  class only when no bigger free block is listed. An object's `id()`
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
reuse their addresses. A cluster that holds a dead string is laid out again
in its own slots, its survivors sorted by bucket, each at its bucket or right
after the one before it; a full table is replaced by one of twice its slots,
which takes the entries sorted the same way, with no probe. Neither allocates
another table for a prune, and both take O(n log n) for a cluster of n
whatever its buckets: removing or placing entries one by one takes time
quadratic in a cluster's length in one step (§5), and names chosen for their
buckets make long clusters (guest `hash()` shows a name's bucket). A snapshot
holds the table's slots as they lie, so the layout is part of what an image
holds (§9.3). No entry lies more than `INTERN_WALK_CAP` (128) slots after its
hash's slot, so a lookup reads at most 129 slots wherever a guest has put the
names: an insert that finds no free slot that near grows the table first, to
the first size at which at most 128 of the entries near its slot stay near it
(or the largest an object may be, 4,194,304 slots, where an insert still
crowded is refused: `MemoryError`). A layout sorted by bucket puts each entry
at the earliest slot any layout could, so neither a prune nor a growth moves
one past the cap. Each entry's kind byte is the top byte of its string's hash
(`hash_tag`): a lookup compares it before it reads a string, so a slot on the
way costs a word. The engine keeps the empty str and the chars up to U+00FF
in a cache beside the table (`Vm::shared_chars`, weak like the table, not in
an image, and charged to the memory budget with the engine's other buffers),
so a native that makes a char per item reads no table slot.
Growing the table and pruning it are each one step, linear in its slots; a
growth is sized for the name that found no slot, so chosen names can force up
to one growth per table size. Code and shape metadata explicitly trace the
names they own; builtin names remain reachable through the builtins and prelude.
The table retains its high-water capacity inside the arena budget until the
session is reset: no prune shrinks it, since a guest could then grow and
shrink it in a loop, a linear step each. `reset` replaces a table of at least
16 times the slots of one a quarter full of the names left (`INTERN_SHRINK_FROM`,
`INTERN_SHRINK_ROOM`) by that one, laid out sorted by bucket, and collects
again; it keeps the table where a name would lie past the cap in the small
one or there is no memory for it, and succeeds either way. `reset_to` and a
template restore replace the heap, and the table with it.

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
  callers) store offset 0 and keep a full record on a side stack. A call
  that `Ic::Call` caches (a Python function with simple parameters, called
  with exactly its positional ones, not a generator or coroutine) runs the
  callee in place at its call window, whether the cache hit or the call
  is the one that fills it; other calls put the frame above the caller's
  registers. So where a frame sits, and how far the stack grows, does not
  depend on what the caches hold, which a restore empties. Restore
  validates the header chain. Code objects sit in one flat table indexed by
  the link's code index. The register count per code object is fixed by the
  compiler.
- Generators and coroutines own a copy of their frame. Resuming copies it
  onto the stack, and yielding back, with one validated block access each
  way. A collection finds in the copy of a coroutine suspended by `send`
  only what the frame reads again (below). While the frame runs, the copy
  is replaced by the frame on the stack: a collection that can take roots
  from an idle heap first empties the copies of the generators and
  coroutines whose frames are on a stack,
  found through the caller records (`Vm::clear_running`), and a mark
  already running has marked what they held. A coroutine suspended by
  `send` is cleared, when it suspends, of the registers dead where it is
  suspended and where it resumes. A suspended generator's copy is cleared
  of the same registers before a collection takes roots from an idle heap
  (`Vm::clear_suspended`, below).
- The VM keeps `pc`, the frame base and the accumulator in locals during
  dispatch and writes them back at safepoints, so a park is "stop the loop".
- GC roots are the live registers of every frame on the stack. A code
  object's live table (`Code::live`) says for each instruction which
  registers hold a value when it starts: those below the allocated top at
  that instruction, without the unset ones, which the compiler has
  allocated and no instruction has stored to yet (a call window's reserved
  `R[r+1]`, which the caller never stores to, the arguments and display
  items still to come, a handler's registers before its first instruction,
  an or-pattern's captures before each alternative). A collection step
  that takes roots (a full collection, the start, each rescan pass and the
  remark of an incremental mark; not a lazy-sweep slice or a mark slice)
  first clears the other registers of each frame, up to the frame above:
  of the running frame at its pc, and of a caller the ones dead at its
  call pc and at its return pc alike. A blocked task's frames change only
  while it runs, so a task cleared and not run since is skipped, as are,
  for `Vm::clear_running`, the copies its frames replaced; the flags for
  this are not in an image, and a restored task is cleared again. An
  instruction that runs Python code before it stores to its target
  registers (an unpack of an iterator, `LOAD_METHOD` through a descriptor
  or `__getattr__`) first clears them itself, since they are temporaries
  it fills only once that code returns.
  So a register is a root only from the store that fills it: what an ended
  statement, an earlier use of the register or a returned callee's frame
  left there keeps nothing alive, and which objects a step that takes
  roots finds through a frame with the table on the stack, or in a
  coroutine's copy, does not depend on whether an earlier collection ran.
  The table adds no instruction and no operand. Its top is a count of at
  most 255 with no reserved value, so a unit that uses every register needs
  no mark for it. `Code::check_live` checks the table against the bytecode
  on every path, run or not (no instruction reads a dead register, and a
  register becomes live only by a store); the compiler's fuzz target and a
  test over the conformance cases, the prelude and the bundled modules run
  it, and nothing a session runs does. One kind of frame keeps more than it
  reads: a frame of the prelude or of a bundled module, whose code carries
  no table (it is stripped with the positions), keeps every register, those
  a returned callee's frame overlaid included, until it returns. A running
  generator's or coroutine's copy holds nothing at a collection that takes
  roots from an idle heap, and a coroutine's copy suspended by `send` only
  what its frame reads again (above); a mark already running keeps what a
  copy held when it began, as it keeps everything reachable then. A
  suspended generator's copy (`GEN`, `AGEN`) is cleared before a
  collection takes roots from an idle heap: the VM keeps every generator
  in a registry (`Vm::gens`, weak: a collection's weak pass drops the
  unmarked ones; its capacity is charged to the memory budget and is a
  function of its length alone, the length rounded up to four
  significant bits, kept so when it grows, after a completed collection,
  at a snapshot and at a restore, so a session is charged the same for
  the same generators whether or not it snapshotted), and for each
  suspended one not cleared since it last yielded it overwrites with
  `DEAD` the registers of the copy dead where the generator is suspended
  and where it resumes, then sets bit 63 of its yield pc (`g::CLEARED`).
  Every suspension writes the yield pc whole, which resets the bit, and
  every reader masks it (`mm-engine/tests/ypc_readers.rs`). What is
  cleared is a function of the generator's code and suspended pc, so it
  does not depend on when the pass runs, and neither a yield nor an object
  the mark scans pays for it. Before a full collection the pass runs
  whole in that step; before an incremental mark
  it runs a slice per step (`CLEAR_SLICE` work units: a generator, plus
  twice the registers of a copy it clears), and the mark starts at the
  step that ends it. The guest waits for those slices as for a held mark
  (§9.2): they run back to back, a park check between them and no guest
  code, so no generator yields behind the pass and the guest allocates
  nothing between the step that found the collection due and the mark. A
  step that only goes on with the last collection's lazy sweep runs none. A generator that yields again after the pass
  passed it, while a mark or a lazy sweep is in progress, keeps its new
  dead registers until the next collection's pass, as anything that dies
  during a mark does: its yield wrote the yield pc whole, so the bit is
  clear and that pass clears the copy, whatever the collector was doing
  at the yield. The pass clears only the copy's first 256
  registers, the most the table can name: the compiler gives a frame at
  most 255, so only a decoded image's frame can have more, and those past
  256 are kept until the generator resumes. The registry is not in an
  image: a restore rebuilds it in address order, in the walk that checks
  each generator (a generator not done keeps its registers in a `BUF` of
  at least its code's registers), and a snapshot rebuilds the session's
  registry by the same walk after its collection, so the session that went
  on and the restored one run the same passes.

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
  One departs from CPython: a `TypeError` or `AttributeError` (the
  engine's, not a subclass) whose message is one of CPython's templates
  with a `coroutine`, `Future` (a host future) or `_GatheringFuture` as
  the value of the wrong type ends its report with `. Did you forget
  'await'?` (`hints::forgot_await` and its `AWAIT_TEMPLATES`, read from
  the message when the report is rendered, so a raise does no more work;
  a message past 256 bytes is not read, which keeps rendering within a
  step, even for a chain of 64;
  `str(e)` and `args` stay CPython's; `conformance/own/576`).
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
under `coroutine.send()` suspends every coroutine frame from the `AWAIT`
up to the one `send()` runs: each saves its registers, its `AWAIT` pc and
what it waits on in its coroutine object (as a generator does at `YIELD`),
and `send()` returns the value; the next `send()`/`throw()` re-runs the
saved `AWAIT`s from the outermost.

A generator or coroutine handles exceptions on top of whoever runs it
now, as CPython's per-generator exception state does. The frame's
outermost handler keeps no saved exception in its register (`PUSH_EXC`
stores `UNDEFINED` there): the exception handled until then is the
caller's of this resumption, kept in the object's `OUTER` slot, and the
object's `HANDLED` slot is set, which marks the frame as handling an
exception of its own while it runs. A handler nested in it saves the
frame's own exception in its register, which no resumption changes. A
frame that suspends with the mark set (a `yield`, each level a `send()`
suspends) writes its handled exception to `HANDLED` and gives the
caller's back from `OUTER`; resuming writes the new caller's to `OUTER`
and puts the frame's own back; the outermost handler's `POP_EXC` reads
`UNDEFINED`, restores `OUTER` and clears the mark. So after a handler
that began under one resumption ends under another, `sys.exc_info()`, a
bare `raise` and the `__context__` of the next exception are the current
caller's, and a frame that handles the very exception its caller handles
keeps it while its handler lasts. A frame with no handler open writes
neither slot at a yield or a resume.

In a task, a yielded `None` yields to the scheduler (the task is queued again)
and any other value raises `RuntimeError`. No Rust frame holds a suspended
coroutine and no second dispatch loop runs.

`asyncio.create_task` spawns a scheduler task and returns its `Task`
handle, which runs when the creating task next yields, as on CPython.
A task's first step and a loop callback start with the exception
`asyncio.run`'s caller handles as the handled one (`Vm::run_handled`, in
the image's run queue; `None` in a cell whose top level awaits): CPython
runs them from the loop, beneath which that handler is still open, and
not under the task that made them.
Cancellation is CPython 3.14's `Task.cancel()`: the request goes to what
the task waits on (a task, a gather's children, a future, a sleep, a host
future) through one iterative walk (`Vm::cancel_walk`, no recursion and no
nested dispatch loop), in steps when the tree is long (§5, "Bounded native
steps"); when nothing there takes it, the task keeps
`_must_cancel` and the scheduler raises `CancelledError(msg)` at its pending
`AWAIT` (the same raise-at-pc path a deadlock uses), or throws it into the
`__await__` iterator it steps, so `except` and `finally` run in the task.
`asyncio.run` is prelude Python around the main coroutine: the loop counts
iterations as CPython's `_run_once` does, and when the coroutine ends the
remaining tasks are cancelled and run until done; then every async
generator that took its first step inside the run, has not finished and is
suspended where a handler covers its `yield` is closed (`_aio_asyncgens`,
a prelude global; `aclose()` awaited on each, 256 at a time), which
stands for CPython's `shutdown_asyncgens` and, as the engine has no
finalizers, for its close at collection too. One suspended where no
handler covers its `yield` has nothing to run when closed: the list drops
it, so the run does not hold it. The end of a cell clears the list, so no
later cell's run closes an earlier cell's generators. All of it is engine
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
release and callback order. A single waiter needs no buffer. Further
registrations append into spare capacity; when it is full, registration
charges for the scan, drops dead entries and doubles the room for the
live ones. The image keeps the spare capacity and the reader checks
that the length fits it. A gather is done in its children's done callbacks,
as with CPython's `_done_callback`: a child's settle queues one
notification that names it, and the gather counts them as they run
(`aw::LEFT`: its distinct children still pending, and its notifications
not yet run), so it settles in its last child's notification or, without
`return_exceptions`, in that of the first failed or cancelled child to
run, not when that child settles. A restore checks each tracked gather's
counts against its entries in the run queue, and that each entry names a
settled task, future or gather (one naming the gather itself, which is
pending, is refused). It does not check that the child is one of the
gather's: an image naming another settled awaitable there, which only a
forged one can, runs it as one of the gather's notifications, so the
gather fails with that awaitable's error, or settles a notification
early, a Python result and never a panic. A callback (`add_done_callback`,
`call_soon`, a timer's) runs in a task slot of its own, with no `Task`
handle, through the prelude's `_aio_callback` from the bottom of the slot's
stack, so a callback that raises goes to the loop's exception handler and a
cell parks inside one like anywhere else. `call_soon` queues its
`asyncio.Handle`; the scheduler skips one cancelled by then, charging the
tick and passing the safepoint its start would, and runs the callback in
the handle's own context, as CPython's `Handle._run` does.
`call_later`/`call_at` timers are scheduler-internal tasks sleeping on the
virtual clock, left out of `all_tasks()`; a due timer queues its handle
first in the iteration, where its task stood. A timer's task, and those of
`wait_for` and `as_completed(timeout=)`, is given its deadline when the
timer is made, so its first step allocates nothing.

A callback's start is everything before the callback itself runs: its
arguments, its context, its slot, `_aio_callback`'s frame stopped at its
call of the callback and, for a Python function or a method of one, the
function's frame entered by that call's own code (`Vm::call_ex`), so no
guest code that can fail for room runs first. A call that does not bind
(`TypeError`) is the callback's error, raised by that call when the slot
runs and reported; any other callable (a native, a `functools.partial`, an
object with `__call__`) is called when the slot runs, and that call is its
run.

The scheduler never discards what it accepted for lack of room. A queue
entry (a callback, or a task's first step) that cannot get its task slot,
stack or context copy goes back first in its iteration. The first time,
the scheduler waits for a full collection, held at safepoints with no
guest code between them as an instruction's retry after `MemoryError` is
(`Vm::start_failed`; the wait is the `LOOP_START_WAIT` loop flag, in the
image, so a park or a restore inside it resumes it), and tries once more.
A second failure raises `MemoryError` in the main task at its `await`,
where the guest can catch it, free memory and await again; the entry then
runs. So a full queue runs in one of two regimes, by what the collection
frees: if it frees the garbage of the callbacks that ran (their arguments
and context copies) or memory the program dropped, every start succeeds
after its wait, the `await` returns normally and the iteration runs to its
end, each failed start charged a collection (a unit per 64 bytes of the
limit, §9.2), so such a queue's fuel grows with how often its starts
reach the limit; if it frees nothing, every turn fails, charged that one
collection, and nothing runs until the guest drops memory. Where the edge
between them falls moves with the heap's layout. The asyncio natives the
guest uses to await, queue and settle again change nothing before what
they allocate, so after `MemoryError` they too wait for a collection and
run once more (§9.2, `Vm::native_pure`): `asyncio.sleep`,
`loop.create_future()`, a timer's deadline, `call_soon`'s queue entry,
`add_done_callback`, `remove_done_callback`, `set_result`,
`set_exception`'s settle, `cancel` and a task's name; `create_task`,
`gather`, `wait`'s and `as_completed`'s waits and `current_task()` do not (each takes a task number or slot, queues or
registers before an allocation that can fail), nor does making a prelude
object (`Handle`, `Event`, `Lock`, `Queue`), nor `result()` and
`exception()`, which raise the future's own exception. An exception that
leaves `_aio_callback` (`SystemExit`, `KeyboardInterrupt`, or a
`MemoryError` its report could not get past)
goes to the same `await`; anything else its report raises (a
`sys.stderr` that fails) is dropped, as CPython's
`call_exception_handler` drops it. In `asyncio.run` after the run's coroutine has
ended, `asyncio.run` raises it once its teardown is done (`Vm::escape`).
A cancel that cannot pass to what a task waits on for lack of room makes
the task raise `CancelledError` at its `await` instead. `Semaphore` and
`Lock` give a permit handed to a waiter back on any exception at its
`await`, not only `CancelledError`. Task numbers, names, callback slots,
the run queue (with its handles) and the loop flags are VM state in the
snapshot.

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

Backward jumps and calls count one tick each; where the ticks the
session has run reach a multiple of `TICKS` (256), or at every tick once
less fuel than that is left, they check one flag word:
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
check charges only the safepoints that ran.

The scheduler also checks when it starts a task or finishes a gather
notification: every 256 of these turns, after more than 16,384 units of
native work, or when the fuel runs out. These turns need not pass any
bytecode check. The scheduler check adds no tick of its own. The image
carries the turns since the last periodic or native-work check. Neither
a fuel-only park nor a restore restarts that interval.

**Regular checks.** The checks every run of a cell makes at the same
safepoints, however a host slices it (`Vm::regular_check`): where the
session's ticks reach a multiple of `TICKS` (the phase, `sp_phase`,
carries across parks and is in the image), after native work past 64
units, for a collection step a park left owed, and under GC stress. A
park adds checks (fuel, deadline, a request, an operator stopping before
it starts) but moves none: only a regular check starts or steps a
collection the heap's trigger makes due, and only one clears `burned` and
`long_work`, which are in the image too. So a park, or a restore at one,
does not move where a collection starts or what fuel a run spends. It is a rare opcode, so the
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
snapshot there needs nothing new. The resume after such a park makes that
regular check first, at the same instruction, as the unparked run does; a
`CALL` parked before its builtin (`Vm::native_short`) makes none, since
no `CALL` stops before it starts (`Vm::at_call`): the check ran a due
collection the unparked run did not, and a call that failed and was
retried unparked fit, one retry charge apart (65,537 units at 4 MiB
[S, collector study]). The hooks sit in out-of-line entry
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
  snapshots stop collecting. Every retry pays, also while a collection is
  asked for and not ended: the retry restarts it, and what it pays is a
  function of the failure, not of whether a mark or another request was
  in progress, which the GC mode and the fuel slices decide (paying only
  when no collection was pending moved a retry's fuel by up to 2^20
  units between drive modes and GC modes [S, collector study]). A result of 64 KiB or
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
  list or tuple or an iterator over one, `CALL_EX` of a Python function,
  of a builtin type whose constructor copies its argument
  (`dict(x, **kw)`) or of a builtin that changes nothing before its result
  is allocated (`'{k}'.format(**kw)`), and any `CALL_EX` whose arguments
  did not fit on the stack before its callee started (`max(*a)`,
  `Vm::oom_retry_expansion`), a call's keyword and `**` spread (`DICT_UPDATE` modes
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
  `_walk_repr` are retried natives), `CALL` of a builtin type whose constructor copies its
  argument (`list(l)`), and the instructions that only allocate their
  result: the displays `BUILD_LIST`, `BUILD_TUPLE`, `BUILD_SET`,
  `BUILD_DICT` and `BUILD_SLICE` (they read their registers and write the
  accumulator once the object exists; keys with a user `__hash__` or
  `__eq__` go to the prelude twin), `MAKE_FUNCTION`, `MAKE_CELL`, and a
  plain `CALL` of a builtin exception type, of `str`, `int` or `float`
  with builtin scalar arguments (int, float, bool, `None`, str, bytes,
  bytearray), or of a user class with no `__init__`, `__new__`, metaclass
  or builtin base other than an exception (`Vm::call_only_allocates`),
  all read at the fault exit (`Vm::oom_retry_fault`). These are what an
  `except MemoryError` handler runs first: dropping the data that filled
  the heap allocates nothing and straight-line code has no safepoint, so
  without the retry the handler's first allocation was refused beside a
  heap of garbage (a list of strs dropped at 8 MiB and up: the words
  space had 24 B of free blocks, or the mark stack could not double
  [M]). Everything else is not retried, so it may be refused there until
  a safepoint (a call of a Python function, a loop's backward jump) has
  collected. For example: a class with `__init__` or `__new__` and `str`,
  `int` or `float` of any other value (user code's result is checked back
  at the `CALL`, where a failure cannot be told from the first; these run
  when the callee's own frame reaches a safepoint first), keyword forms
  of these calls, `raise E` of a class not yet instantiated, an exception
  the engine raises itself (a failing `assert` raises `MemoryError`),
  `object()`, `range(5)`, `slice(1, n)`, `import`, attribute access that
  binds a method, a unary operator on a heap int, `with`, and iteration.
  Whether one of these is refused depends on the wall the fill hit and on
  what the statement allocates first [M]. A handler that keeps the data has only the room
  the fill left, which may be none: a collection frees nothing, and its
  allocation raises `MemoryError` again.
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
  `CALL_EX` after its spread; a binary operator, `SET_ITEM` or directly
  called builtin after a slice's rerun, whose own retry names no
  instruction and takes only such a mark for its rerun) retries as its own. The
  other order errs towards raising: a binary operator's mark names no
  instruction, so an instruction refused in the same tick after the
  operator's successful rerun (the display of `{'k': x * 3, 'j': 1}`)
  takes it for its own rerun and raises after the one charge. The hooks sit outside the dispatch loop's arms
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
  memory, each check sweeps 8 segments per space, and the code-liveness
  scan finishes it first. A snapshot neither marks nor sweeps: the image
  holds a mark in flight (marks, mark stack, partial scans, dirty flags)
  and the segments a sweep has not reached as they are (§9.3), so the
  restored session goes on with the same mark and sweep, and allocates the
  same blocks, as the one that took it (`conformance/own/533`). Code
  reclamation and a native's collection request abandon a mark for a full
  collection.
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
- **Snapshots at parks** collect nothing: a snapshot taken during a mark
  writes the mark as it is. Writing the image is the host's call, outside
  the step bound.
- **Also unbounded:** every collection under 32 MiB live (30 ms native
  at worst) runs in full, as do code reclamation's, and one
  a native or the engine requests (`Vm::request_gc`): after a
  `MemoryError` is raised and after a byte job fails for memory.
- **At the wall between cells.** A cell that fills the heap and drops its
  data leaves garbage that no safepoint collects: none runs between
  cells, and the trigger counts allocation, which a drop does not make.
  So a cell starts (`Vm::prepare_cell`, which a host definition runs
  first too) with a full collection, uncharged,
  when the heap is at its wall (`Heap::at_wall`): for the words or the
  bytes space, the memory left is under the cost of its next segment
  (1,081,344 B at full size) and its free blocks under 1/8 of the limit;
  or the object count is at least 3/4 of the mark stack's capacity and
  its doubling (4 bytes a slot) is more than the memory left. That
  collection is code reclamation's (Session lifetime), on a condition no snapshot
  resets, so whether dead code is dropped before the next load does not
  depend on whether the host snapshotted. A start, a `reset`'s new
  globals, a hint or a module dunder refused for room anyway runs once
  more after one, unless the heap was idle (below) before it (`reset`, a
  hint and a module dunder release before they allocate, so they end
  the idle state first, as does a typed host definition): the
  constants of the code reclamation drops after its collection are
  freed this way, so a start whose reclamation dropped code and that is
  then refused collects twice. That reclamation also drops the names
  only the dead code held, so a later cell that uses one again can be
  refused at the wall where the dead code, kept, would have let it run.
  `reset` first empties the old globals and modules in place, allocating
  nothing, so that collection frees what they held; if new tables still
  do not fit, the emptied ones stay, and the session is reset all the
  same; only `__name__`, if its interned name was collected and still
  finds no room, is left unset, and `reset` reports the `MemoryError`. A
  module body's name is interned when a function is made from it, not
  when it loads, so a cell such as `del xs` loads without allocating.
  None runs twice unless, since the last, code ran, something held
  between cells was released (a stopped cell, an error, globals, a host
  definition's old module, a hint, a module dunder, the code
  reclamation drops) or anything was allocated (a host definition, a
  start refused part way, the rerun of one)
  (`Heap::idle`, in the image, so a restored heap is idle where the
  session was). At 256 MiB it
  takes 7–34 ms over garbage and 38–105 ms over live data, which it
  repeats at each start while the heap stays at the wall, also when every
  cell completes: 2.5M strings and 1 kB bytes objects live at 256 MiB, 200
  cells of `n += 1` collect at each start, 25.8–32.0 ms a start, in 6.9–7.1 s
  against 1.1–1.2 s without the collection, shipped build [M]. A session
  kept short of the wall (90% of 256 MiB live) pays none.
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

- **FULL only, and exact:** the segments of both arena spaces as they are,
  garbage, free blocks and the collector's state included, plus VM state.
  `Session::snapshot()` writes without collecting and changes nothing in
  the session it is taken from; `Engine::restore` reads into a new session
  (same or another process) that goes on as that one does: within a GC
  mode, its outputs, outcomes, fuel and next image are the same whether
  or not, and wherever, the session was snapshotted or restored.
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
         object as a number), its blocks up to its top (live, garbage
         and free), dirty flag and mark bitmap; the allocation run, the
         free lists' heads, the segments the lazy sweep has not reached;
         object counts, the mark stack (capacity and entries), the
         incremental mark's state and partial scans, char positions,
         the idle flag
vm       registers, fuel left, limits, seq / virtual clock, the
         collector's schedule (owed and pending steps, waits), the
         capacities of the VM's tables, the code loaded since the last
         reclamation (in bytes of the writer's charge) and the bytes
         charged outside the arena, frames,
         continuations, scheduler, tasks and run queue, globals, modules,
         pending host calls and their ids, park reason,
         host modules (definition; for code the Engine shares, its 16-byte
         key, code hash and recipe), before the
         code table (Code::encode_into: the room each of a code's eleven
         tables keeps past its length, then the code; bundled-module code, loaded
         prelude units' code (by the unit's chunk and record offset) and
         the shared code of host modules' current definitions referenced,
         not stored)
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
  Restore work is bounded by the memory limit, not by image size: compact
  descriptions of free space and reserved capacity can expand in memory.
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

**Restore.** Order: magic, format version, checksum, `build_id`; a
template's trusted read (§10.1) skips the checksum. Lengths
are checked against the remaining input before any allocation; the heap is
checked against the engine's memory limit (`TooLarge`, §11 "Restore
admission").

Validated on restore: segment indices, run tiling, every block header, every
`str` (UTF-8, char count, flags), the intern table (identity, count, load,
every entry within `INTERN_WALK_CAP` slots of its hash's with no free slot
between and with its hash's byte; a template's own image, restored unchecked,
was written by this build from a table that held all of it),
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
table's untagged string references are checked by mm-core. The collector
state the image holds is checked as well, so a damaged image ends in
`Corrupt` at restore, never in a heap that later hands out live memory:
blocks tile each segment up to its top and a free block is a typed free
header; start bits and used counts are derived from the blocks; marks sit
on block starts only, and outside a mark only in segments the sweep has
not reached, which are present, distinct and not the bump segment; every
free-list and run entry is a free block of its list's size class, each
taken once and outside those segments, and the free counts and masks are
recomputed from them; mark-stack and partial-scan entries are marked
objects; during a mark, a marked object that is in no dirty segment and on
neither list references nothing unmarked (the intern table, which is weak,
aside); the object counts match; char positions name strs of their length.
Every arena access is also bounds- and start-bitmap-checked and fails with
an error.

Not saved: what `SessionSettings` gives a new or restored session (the
output callback, the wall clock, entropy, the zone source, the text cut,
`int_bits_text`, GC stress), the deadline and park flag, which the host sets
per run, and `Session::set_gc_yield`, which a host sets again after a
restore. Inline caches start empty.

**Bundled code on restore.** An image names bundled-module and prelude
code by reference, and a restore decodes each program it names without its
instructions: it unpacks the string, index and metadata sections and
builds every code object but its bytecode
(`Program::decode_sections_deferred`), so every check of the image against
the code (constant and name counts, keyword-name tuples, inline caches,
code indices) still runs at restore. When a code of that program first
runs, the program is decoded whole, as an import decodes it (the module
cache's copy when it has one), and its codes still without instructions
take theirs: in the dispatch loop, whose first fetch of such a code finds
a placeholder word (`bytecode::DEFERRED`, an opcode no compiled code
holds) and leaves the hot path for the unknown-opcode one, so a frame
start pays no check; when a class reads `__init__`'s stores and when a
loop callback enters `_aio_callback` at its `CALL_EX`
(`Vm::load_bytecode`). Decoding the whole program again keeps one decoder:
an instructions-only pass would save 2.2% of a durable replay's
instructions for about 5 KB per shipped artifact [M]. Programs that a
frame, a stopped state or a suspended generator or coroutine of the image
runs in are decoded during the restore, whose checks read their pcs; a
suspended generator's are decoded on a trusted restore too, as its resumes
and a collection (which clears its dead saved registers) read its
instructions without a check. A code without its instructions keeps their
buffer reserved (holding the placeholder), and the restore charges the
unpacking of the left-out sections as if it had run
(`pack::admit_shared`), so a restored session's memory charge is the same
before and after the decode, and the same as a session's that loaded the
code itself; the decode burns no fuel and charges nothing. A corrupt
`host-bundle` read at that point raises `SystemError` in the cell, as an
import's does. The engine's module cache keeps such programs too and takes
the decoded one in their place.

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
Session::reset() -> Result<(), PyException>                       // keeps loaded bundled code and prelude parts; gives back committed memory
Session::reset_to(&SessionTemplate) -> Result<(), SnapshotError> // a fresh session plus the template's setup, keeping what reset keeps
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

**Definition code is the host's, not a cell's.** The Python generated for a
module's typed functions, classes and exceptions runs between cells
(`Vm::run_host_defs`) on the one dispatch loop, to its end: without the
session's fuel, deadline, clock and park flag, which come back as they were
(fuel, overdraft and the fuel total unchanged), so no park is due in it and a
park request, pending or made meanwhile, stays for the next cell. A park there would leave a half-made module
and a stopped run that is no cell. It stays bounded without them: the code
has no loop of its own (a few calls per function, class and field), so its
work grows with the definition the host gave and with nothing a cell did:
charged as a cell's would be, 61 fuel units for one typed function, 556 for
100, 3,244 for 100 with 20 checked parameters each, 6,982 for 100 classes
of 10 fields and 3 methods, 1,384 for 100 exceptions [M]. It calls only the prelude's globals and natives
(`_mm.signature`, `property`, the class builder with `tuple` or a built-in
exception as base), which no cell can rebind: user code has no handle on the
prelude's globals (no `__globals__`, `__builtins__` or `builtins` module),
and built-in types and the prelude's functions and classes are immutable.
Collections it makes due still run at its safepoints.

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
taken after setup that instantiates by restore. Its image is read trusted:
without the checksum and without the validation walks. `capture` wrote it
from a live session in this process, `from_image` restores it checked
first, as `Engine::restore` always does, and the template keeps the bytes
private and never writes them again. A trusted read still compares the
magic, format version and `build_id` and bounds-checks every length; it
does not detect bytes of the template that change in memory after the
capture (a hardware fault), which then decode as they are: a typed error,
a contained panic or wrong state, never memory unsafety (§5). The hash was
5% of a py-sdk session's instructions [M]. `Session::reset_to`
restores it into an existing session in place of `reset` and a re-run of
the setup: the session keeps its host-side settings, park handle, limits,
fuel state, host arguments and hint overrides, takes the setup's recursion
limit and call log, and its call ids, task numbers and safepoint phase
continue, moved on as the setup moved them. Unlike `reset`, which keeps the
bundled code and prelude parts earlier cells loaded (charged to the
session), it keeps nothing they loaded: the next cell's output, host calls,
fuel total and memory charge are a new session's after the setup (where
the first period's safepoint checks fall can differ). It refuses
(`SnapshotError::Mismatch`) a template captured under other limits, host
definitions, function ids or local time zone than the session's, and
`Busy` for an image of a stopped session; every error leaves the session
unchanged.

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
| `Reset` | deferred to the next `StartCell`: `Session::reset_to` the state kept after its bootstrap if the bootstrap is the same, else `Session::reset()` and the bootstrap |
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
type check, which is the harness's. The harness resends the SDK bootstrap
with every cell after a `Reset`: the worker keeps the state a bootstrap
leaves when it runs right after a `Reset` (a `SessionTemplate`; one at a
time, replaced when another bootstrap is kept) and resets to it while the
bootstrap stays the same, instead of resetting, compiling and running it
again. A bootstrap that read the wall clock or entropy is not kept: it runs
after every `Reset`, so each cell sees its own values. After each bootstrap
it runs or resets to, the worker calls `Session::setup_done` (§11, Session
lifetime): the tool functions and the bootstrap's code stay for every cell. Against a local checkout of plak it
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
prelude's unpacked chunks, the bundled code's decoder tables and compiled
host-module programs, filled on first
use and capped at `DEFAULT_MODULE_CACHE` (8 MiB) unless
`Engine::with_module_cache` sets another cap. It is charged once to the
Engine, not to any session, and a session is charged and behaves the same
with or without it. Every bundled module decoded is about 2.9 MB, host
code about 0.67 MB per 100 typed functions; on json, asyncio, re,
collections and datetime a warm restore takes 9.95M instructions against
23.75M uncached, and RSS per session with 40 held is 1.2 MB against
1.85 MB [M]. For fully hostile code, process or wasm isolation
remains the stronger boundary.

Each mm-wasm instance keeps one such engine, with a shared cache and
limits set separately for each session. An idle session can be captured
as an instance-local template. Instantiation starts an independent session
in that same instance, with the captured limits and fresh host-side settings;
`reset_to` replaces Python state by the native facade's rules, retaining
the destination's host-side settings and continuing its counters. An
instance's templates can refer to its cached host code. Exported snapshots
still carry compiled host code and restore on any guest of their build.
The reference hosts reject a template from another instance as a mismatch.
A trap invalidates all templates and sessions of its instance, but not
snapshots already copied out. Dropping a template frees its saved state,
not the sessions made from it.

Hosts may drop a session and restore a same-build snapshot in the vacated
instance, keeping the engine and linear memory. Restore still checks the
whole image and supplies new host settings; it cannot revive a trapped
instance. Reusing an instance does not retain the dropped session's state.

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
collection is due anyway or the heap is at its wall (`Vm::prepare_cell`),
it collects in full and
reclaims user compilation units not referenced by live functions,
generators or saved tracebacks; dead code below that threshold stays, in
the session and in its images. Keeping a unit keeps its nested function
bodies too. Code, source and cache indices are compacted only between
cells, never under suspended frames. Reset also reclaims unreachable user
code. The cost includes a heap scan.

The count starts anew at each reclamation and at `Session::reset`, and at
`Session::setup_done`, by which a host says the code loaded so far is the
session's setup (host definitions, a bootstrap cell's functions). A setup
stays reachable, so counted, it brings the first reclamation forward to
the session's first cells, where the collection frees nothing; a new
session's count starts at zero, with the prelude's and the host modules'
records in it. The count is in an image (§9.3), as the code loaded since
it last started anew: a restored session (`Engine::restore`,
`SessionTemplate::instantiate`, `Session::reset_to`) goes on with the
count of the session the image was taken from, so one restored from a
marked setup's image is marked. The mark
changes only when the trigger fires: all code, the setup's too, stays
charged to the memory limit, and a reclamation frees every dead unit,
whenever it was loaded. So the dead code a session holds between
reclamations is at most the threshold above and what the last cell
loaded, plus, new with the mark, the units of the marked code the
program has since dropped, which wait for the next reclamation (after
the threshold of newer code, at the wall or at a reset).

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
  retained codes, so a same-budget restore can verify. Code is charged by
  its buffers' capacities: compiled code keeps the growth slack of its
  bytecode and tables, which an image carries and a restore reserves, so
  a restored session holds and is charged what the session it came from
  is, and stays so through its next loads. The charge is not in the
  image: a restore computes it from the code and capacities it holds, in
  its own target's sizes, and the session that went on counts the same
  (every growth of a charged table or workspace is charged where it
  happens, not at the next load). The image does hold the code loaded
  since the last reclamation (Session lifetime), as a count of the
  writer's bytes; a restore takes it from its own charge, all of it
  where the count is larger (a 64-bit target's image on wasm32). Host
  signature code has its decode's capacities either way.
- **The kept stop.** The stop a session keeps for `Session::pending` is
  charged by its length while the host holds it, in the session that
  stopped and in one restored from its image alike, and its charge goes
  with it: at the resume once the call is accepted, and wherever the stop
  is dropped. Its buffer is reused from stop to stop; the room it has past
  the stop, at most 4,096 bytes, is not charged.
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
  callback before it can become pending, and for the done callbacks of a
  task, gather or host future, which settle where nothing may fail, so
  settling and unwinding them need no queue allocation. A loop future
  (`asyncio.Future`), which only the program settles and which can outlive
  its cell, reserves for its done callbacks when it settles, and a cancel
  for everything its tree will queue, in one reservation before anything
  changes: no room raises `MemoryError` with nothing changed. So a
  program that fills memory and then settles such a future gets that
  `MemoryError` at `set_result` where CPython completes: a loud failure,
  never a dropped callback. The room
  already taken is a count kept as entries are queued and taken, not a
  walk of the queue. Host futures use an id-sorted vector with logarithmic
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

The measured high-water is 1.99 × L [M, on the 0.0.3 release tree], from
an exception's text (`exception_repr_args`: `repr` of a `ValueError`
whose args are a third of the limit of `'\x00'`s), at 64 and
256 MiB (1.49 × L at 16 MiB), over 74 probes at 16, 64 and 256 MiB.
`bytes.find`'s KMP table, 2.01 × L over 73 probes at 0.0.2, now reaches
0.67 to 0.68 × L at 16 and 64 MiB and ends in `MemoryError` at 256 MiB
(`bytes_find_kmp`) [M].

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
failed allocation returns `SnapshotError::TooLarge` and leaves the session
unchanged.

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
match can begin with matches, a step per char. Each iteration of a
possessive repeat is atomic, the first `min` included, as in sre
(`(?:a|ab){2}+` does not match `aba`; `(?>(?:a|ab){2})` does): between
the first `min` the stack is cut back to the repeat's barrier, keeping the
oldest undo record of each register (`conformance/own/562`). Each cut walks
the records the last one kept, so many groups under a large `min` cost
fuel: `(?:a|b()…()){20000,}+` with 1,000 groups on 20k chars charges
1,636,811 units against 3,320 for fe8b1c2's single atomic group, and
`{1000,}+` 112,209 [M].
Past `min`, and in a greedy repeat that ends its atomic group or
lookaround, the frames below the newest exit frame are dead (that exit
goes straight to the group's `END`), so the cut runs every 64th iteration
(`FOLD_EVERY`): the stack holds at most 64 iterations' frames (1.6M
chars in 11 MB RSS, `conformance/own/563` [M]). Other greedy ones keep
every iteration's frames: `re.fullmatch('(a|b)*', 'ab' * n)` completes
at the default limit at n = 650,000 and raises `MemoryError` at 653,000
[M] (16.5 stack words a char charged 12 bytes each [E]), where CPython
completes.

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
program by at most 0.5% (`startup` by at most 300 instructions; startup
drift is not accepted, owner) over the base and over the program's floor,
unless a commit says why with the table on a `Native-IR-Accepted:
<programs> <ceiling> <reason>` line, which passes only those programs up
to that ratio; a drop passes. `cargo xtask native-ir` checks it (in
`check`, Linux), building the base with its own toolchain and, where its
xtask has `shipped-mm`, its own recipe; the whole-process count is
reported beside it. Each count runs the binary from a scratch path of one
length (`/tmp/mm-native-ir-<run>/<job>/mm`) on a copy of the program
beside it, with an empty environment: `startup` moves about 22
instructions per environment variable, one per character of the binary's
path, and more with the script's path [M], so a count is the same in
every run and on every machine of one class: `71b9c5b5`'s `mm` run as
`/tmp/xref/mm` under `env -i` reads 581,149 on `startup.py` on two
machines (valgrind 3.24.0, glibc 2.41 [M]). The two parts left out
depend on where the binary's segments land, not on engine work: the dynamic loader's relocations, and `pthread_getattr_np`
parsing `/proc/self/maps`, which Rust's runtime calls from inside `main` to
find the main thread's stack. That is why counting from `main` alone is not
enough: one layout-only change moved `startup` +0.80% whole-process, +1.06%
from `main` and +0.03% gated [M]. On the other programs the two counts
differ by a constant of about 337K Ir, 0.10% of the smallest [M].

**Floors.** Each Ir gate keeps, per count, its floor in
`xtask/ir-floors/<gate>.txt` with the build that set it (a hash of the
blob ids of what a shipped build reads) and its key (the counting method,
the tools' versions, glibc and the CPU class valgrind shows, the program's
source). The gates compare the head with the base and the floor and build
no history (owner, 2026-11-14). Floors bind upward only (owner,
2026-11-19): a count below its floor passes and leaves the file as it is,
so a drop then a rise back to the old floor passes until the floors are
next lowered (`--write-floors` when someone runs it, or a tag's seed);
a geomean against the floors takes each program's base count where it
is below its floor, so a floor left above one program's drop is no
headroom for the others' rises (G1); a floor rises
only to the head's count and only with an accept line whose ceiling
covers the rise, for a name past a limit; a new count is seeded at or
below the head's. `--write-floors` writes the file; a moved line names the
head's build, so a push of several commits writes it at its tip (the gate
cannot count the commits in between). A floor whose key is
not the run's is stale and is recounted on its build, found on the
first-parent line from the last tag to the head, and judged against the
recount; the file keeps the base's line unless the head moves that count,
so two hosts with different keys both pass an unchanged tree, and keys
are rewritten only by a moved count or a seed. With `--base` at
a tag (`just pretag`) the head is compared with the tag, and every floor
must be on the tag's first-parent line up to the head, hold the tag's
count where it names the tag's build and the head's where it names the
head's, and hold an accepted rise at the head's count (a floor above the
head's count passes); each failure names
`just seed-floors`. `--write-floors` there seeds the file from the tag,
each floor at the tag's count with the tag's build (the head's where the
head counts less or rises with an accept line), whatever the file held;
`--programs` cannot seed. That is how each release starts its floors:
`just seed-floors` is the first commit after a tag (one that lands later
is judged with `cargo xtask check --release-gate --ir-base <tag>`, the
Ir gates from the tag and the rest from the base). A floor is the lowest its count has been
since the tag only because the pre-tag check forces that seed. When two
landings both move a floor, the raise wins if one side is an accepted
raise, else the lower count; the gate on the combined tree then sets it.
What is lost against counting every commit is which commit moved a
count.

Known limits (reviews of the floors, D2 to D9 and N1 to N3; Low, left as
they are):
floors are trusted per host class. A push that moves or adds a floor line
writes it under its host's key, so it passes only on that host; another
host judges it with `--base <tag>` (D2). At a tag base a line under
another host's key is recounted, not checked, so a hand edit there is
caught only by a host with that key (D3), and a floor may be raised by
hand up to an accept line's ceiling for a rise under the limit (D4). A
stale key is not rewritten until the next seed, so each fresh checkout
recounts its build (D5). A new count's line may name any build until the
next pre-tag check (D6). A mark is its attribute and its function's name,
so one moved between two functions of one name in a file is not an added
mark (D7). The pre-tag check forces the seed only when the first push
after a tag is judged against the tag (D9). A floor lowered by
`--write-floors` before a push's tip, or on another host, fails until
`--write-floors` is run again at the tip on the judging host (N1). At a
tag base a floor on an intermediate commit's build is bounded only by
an accept ceiling since the tag, not by the head's count (N2). A seed
over a stale or missing file does not sweep an accepted geomean-only
rise; the pre-tag check then asks for `--write-floors` once more (N3).

**Recorded sessions.** The bench programs reach a small part of the
engine (one size change marked 261 functions; they ran 20 of them [M]),
so the release gate also counts the 160 recorded real sessions: `cargo
xtask replay-ir` replays them through `conformance/codemode/real/gate.py`
on the `replay_worker` built as shipped plus `--cfg mm_size_wasm`, which
turns the wasm-only `optimize(size)` marks on natively, under callgrind,
counting everything each worker runs from glibc's `start_thread` (the
thread that serves) on, without `pthread_getattr_np`. From each
replay's callgrind dump it reads three numbers. The total. The judged
count: the total without the inclusive Ir of glibc's `malloc`, `free`,
`realloc`, `calloc` and `posix_memalign`, 17.6% of all the sessions' Ir,
1.2% to 28.4% of one and 16.3% of the `codex` sessions' [M, counted
with the cleanup below left out too]. And the allocator's calls: the
calls of those five. The
allocator's cost follows the heap's layout, not the work: a worker that
made one more allocation of 24 bytes moved a session's total by up to
0.67% against the same worker without it, both ways, while the judged
count moved within 0.05% [M]. The judged count alone sees a change that
allocates more often only in the worker's own instructions around each
call, 17.5 of the 157 Ir an added allocation costs (11%) [M]; the
allocator's calls see it whole and do not follow the layout: two
workers that do the same work differ by at most 27 calls a session
(0.031%), two runs of one by at most 1, where the sessions make 3,188 to
124 million calls [M] (without `free` the same-work drift is 0.057%).
What neither judged number sees is work glibc does for a call that
grows with its size: the copy inside `realloc` and the zeroing inside
`calloc`, left out with them, are 5.4% of `memcpy`'s Ir and 5.0% of
`memset`'s, and up to 4.6% of a session [M]. A count is only as good as
callgrind's view of glibc: without glibc's debug symbols it never sees
the local `start_thread` and collects nothing, so before the replays the
gate runs each worker on an empty stdin and stops unless it collects
something. An allocator linked in, or interposed without forwarding to
glibc's, leaves glibc's `malloc` and `free` out of the dumps, and a dump
that ran without them, or without its totals, is an error, never a
zero; one that forwards to glibc's is counted, its own instructions in
the judged count. `posix_memalign` is called once a session or not at
all and counts 0 when missing. No entry point may run inside another: a
dump holds callers, not call stacks, so the inner one's Ir would be
left out twice. glibc's cleanup at a thread's exit
(`__libc_thread_freeres`) was left out as an entry point until it was
found to do that: it flushes the thread's cache through `free`, 341 to
373 calls and 92,294 to 383,280 Ir a session that follow the heap's
layout, and with them left out twice a session read 1.0059 that reads
0.9997 with them left out once [M]. The cleanup's frees are now left
out once, as `free`, and what it runs outside `free` (about 3,900 Ir a
session) is in the judged count.
The rest of a count still follows the layout (probes keyed by
addresses), so the replay pins the two things that moved it from run to
run. `gate.py --lockstep` sends a frame only once every thread of the
worker sleeps in two looks with no thread run between them, so its
reader and serve threads take each frame in the same order; it reads
`/proc`, so it is for Linux only. And the worker preloads a fixed
`getrandom`, so std's `HashMap` keys, and the order a map drops its
entries in, are the same in every run. A race left in the worker's
channel still moves one binary's judged count on a session by up to
0.106% between runs (the highest of five runs on 16 cores, machine
load 90 to 230) [M]. A session that diverges from the recording
counts nothing, whether `gate.py` killed its worker or the worker ended,
exited or aborted first, or exited with an error or aborted after its
last cell (`gate.py` names each such session), so it is not compared: `phase1/codex-sdk-monty/codec-review` diverges at cell 12
in its recording, so on every build its first 11 cells go uncounted. A
session that diverged on the head and not on the base fails, and no
accept line covers it.
Counts are kept per worker binary: a head whose worker is the base's
byte for byte is counted once. One session holds 63.1% of the counted
instructions [M], so it judges per session (+0.5%) and by the median of
the sessions' ratios (+0.3%) of the judged count, and fails a session
whose allocator calls rise past +0.5% (fewer always pass), each against
the base and against the session's floors. An accept line covers the
number it names, up to its ceiling (under 2.0): `sessions` a session's
ratio of judged Ir and the median's, `sessions-calls` a session's ratio
of allocator calls, so a line written for one leaves the other judged.

Each session has two floors (**Floors** above), in
`xtask/ir-floors/replay-ir.txt`: `sessions/<id>`, the judged count, and
`sessions-calls/<id>`, the allocator's calls; the total is not floored.
The gate runs in the release gate only, so a rise that lands on the
dev tier is judged at the next release-gate run against the floor,
and rises under the limit cannot add up between two runs. The judged
count is not exact: one worker's reads of a session differ by up to
0.106% [M] (the race above). A floor moves only on a count past a band
below it, 0.15% of the judged count and 0.05% of the calls [E]; a head
within the band is the floor's count, so the file does not follow the
race. A line the head's file holds on the head's build within the band
of a read is that read's line, so a lowered, raised or swept line
written from one read passes the next; and `--write-floors` writes a
new count's line at the read wherever the file holds it above, so at a
gate's first landing the band is a margin over the race, not a limit
some sessions sit on. The median against the floors takes each
session's base count where it is below its floor, as the geomeans do. Every session's floors share one key (the counting, valgrind, the
host, the workload and the fixed `getrandom`), so a change to any of them
makes all of them stale and the gate replays the floors' build as a third
worker beside the head's and the base's.

## 13. Tests and conformance

Every gate is local: GitHub runs none. `cargo xtask check` (the dev tier:
a subset before each push, plus the stages that guard what the change
touches) and `cargo xtask check --release-gate` (every stage, on main's
tip after pushes and before a tag) run them; `AGENTS.md` says which tier
a change needs and lists the commands. A `v*` tag runs only
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
  own cases and every third corpus case also park at `1:100` in the dev
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
- **musl** (Linux; the release gate, and the dev tier for a change under
  `crates/mm-cell-worker/`): the `musl` stage builds `mm` as shipped for
  `<arch>-unknown-linux-musl`, runs the own cases and Monty's corpus on
  it, runs the worker's tests built for that target, then builds the
  worker as shipped and runs its protocol suite on that binary, under its
  seccomp filter, as mimalloc runs on one NUMA node and on two
  (`MIMALLOC_USE_NUMA_NODES=2`). In the release gate, `steps-musl` runs
  the step-time oracle's wall-time half there, alone, twice: on musl's
  `malloc` (`tests/steps.rs`) and on mimalloc
  (`tests/steps_mimalloc.rs`), the allocator `mm` and the worker
  register on musl; a failure on one still runs the other.
- **GC stress** (not in `check`): `cargo xtask gc-stress` reruns the own
  cases, the corpus and durability collecting at every safepoint
  (`--incremental`: an incremental mark in tiny slices at every safepoint,
  each checked against a full mark); output must not change, and within
  each stress mode durability's reports and fuel must not depend on parks,
  snapshots or restores (`--no-digest`).
- **Size gate** (`cargo xtask size`, in `check`): the shipped builds against
  the base's and the release's (§6).
- **Bench gate:** `cargo xtask bench --quick --engines cpython,micro-monty`
  (in `check`) runs every bench program on both engines and requires every
  checksum, warm-up included; a Python exception is a regression. The full
  `cargo xtask bench` times the engines it finds and writes
  `bench/results/`; run it alone on an idle machine. Native Ir follows the
  gated-count rule (§12): `cargo xtask native-ir` (in `check`, Linux) fails
  when a program's gated count rises past 0.5% (`startup` past 300
  instructions) over the base's or its floor; `cargo xtask replay-ir`
  (release gate) gates the recorded real sessions against the base.
- **wasm-ir** (in both tiers, Linux only, since it needs valgrind):
  `cargo xtask wasm-ir` builds the working tree (or `--head REV`) and the
  base (`--base REV`, default `origin/main`), the base in a worktree
  outside the checkout with its own toolchain and, when its xtask has
  `shipped-mm --target`, its own recipe, as shipped: the wasip1 `mm`
  precompiled by `wasmtime compile --target x86_64-unknown-linux-gnu`
  (baseline ISA, so every machine compiles the same code). It counts each
  bench program once under callgrind: the instructions at addresses in
  wasmtime's perf map (the guest's compiled code), plus the inclusive cost
  of wasmtime's `memory.copy` and `memory.fill` libcalls (share at 0.0.2:
  str 4.9%, json 3.2%, fmt 2.6%, dict 2.2%, sort 1.9% [M]; none on fib,
  loop or closure), so a loop turned into a
  bulk copy does not read as a gain. Limits, against the base and the
  floor (§12) alike: a program rising past 1.0%, the geomean of the ratios
  (without `startup`) past 0.5%, or `startup` (about 0.67M instructions)
  rising by more than 300 instructions; decreases always pass. An intended
  rise carries the table and a `Wasm-IR-Accepted: <names> <ceiling>
  <reason>` line in its commit message (`MM_WASM_IR_ACCEPT` with the same
  text before committing), which passes only the names it lists up to the
  ceiling and raises only their floors (an accepted geomean past its
  limit, those of every program that rose). A revision's counts are cached in
  `target/wasm-ir/cache/`, keyed by the counting method, the wasmtime,
  wasm-opt and valgrind versions, wasm-opt's wasip1 arguments and
  features, the host, the recipe and the toolchain, and each program's count by a hash of its `bench/*.py` source.
  Calibration [M]: the 0.0.1 Cranelift spill fails it (fib 1.080, closure
  1.102, geomean 1.042).
- **v8-ir** (release gate only, Linux): `cargo xtask v8-ir` counts V8's
  instructions on the bench programs (`startup` left out) in both
  `wasm32-unknown-unknown` guests, plain and bundle, which are built with
  other `wasm-opt` flags than the wasip1 `mm` wasm-ir counts. Each program
  runs in its own `node --no-liftoff --single-threaded` process under
  callgrind, twice in fresh sessions (`hosts/js/bench/count.ts`); the
  count is the second run's instructions in code without an object file
  (callgrind's `???`) that is not file-backed by the process's mappings:
  V8's generated code, that is the guest's TurboFan code, the wasm-to-js
  wrappers and the host's JS as V8 has compiled it. The `???` stubs in
  libstdc++, libc and node are left out, and code in a file was never
  counted: node's and libc's `memory.copy` and `memory.fill` (11.6% of
  plain/str's second run, 2.4% of bytearray's [M]) are outside the count,
  so work a change moves there costs it only the calls. The first run
  holds V8's lazy compiles; the second starts with a deopt and up to two
  recompiles, and repeats. Two things make the count repeat [M, on one
  host, one node and one valgrind]: `--single-threaded` keeps V8's
  compiles and collections on the main thread, so the host's JS reaches
  each tier at the same point in every run (it also implies
  `--no-wasm-tier-up` and synchronous single-task wasm compiles, without
  effect under `--no-liftoff`); and leaving out `???` code in a file
  removes a libstdc++ allocator stub whose calls moved by one, for no
  cause found. A count whose dump does not add up to its `totals:` line,
  or that has `???` code in no mapping, fails. Every count also runs from
  `/tmp/mm-v8-ir-<pid>/<job>/` with copies of the host JS, the
  guest and the program under fixed names and an empty environment, which
  keeps `NODE_OPTIONS` and `VALGRIND_OPTS` out; the gate runs only where
  valgrind shows the CPU as `x86-64-v3` (without AVX plain/floats counts
  1.556% more). A revision whose xtask has `size --head` builds its guests
  with its own recipe, so a change to the `wasm-opt` flags or the
  `optimize(size)` marks is counted at its commit; older ones are built
  with the head's recipe. Each revision's guests are driven by its own
  host (`hosts/js/src/`, `package.json`), as a host speaks one
  `ABI_VERSION`, under the head's `bench/count.ts`, the counting method;
  the host's compiled JS is in the count, so a host change is judged as a
  guest change is, and since a floor's key holds the head's driver it
  makes the floors stale, recounted on their builds with their own hosts.
  The floors (§12) are named `plain/<program>` and `bundle/<program>`.
  An accept line `V8-IR-Accepted: <names> <ceiling> <reason>` (programs or
  `geomean`, `plain/` or `bundle/` before one for one guest) passes only
  those counts up to that ratio and raises only their floors;
  `MM_V8_IR_ACCEPT` stands for it in an uncommitted tree only. Limits, on
  either guest against base and floor: a program rising past 0.5% or the geomean past
  0.3%; decreases pass. Counts are cached per guest content and driver, keyed by
  the counting method, node's version and binary, valgrind's version,
  glibc, the CPU class, the driver and the program's source, so only a
  guest whose bytes changed is counted again. Stability [M, on one host,
  one node and one valgrind]: of 2,997 counts of 1,980 keys (the guests
  of 76 commits), the 440 keys counted more than once repeated exactly,
  idle and under load, from checkouts at three path lengths, in other
  orders and under other environments; without `--single-threaded` a count moved by up to
  0.085%. Valgrind runs no AVX-512, so this is V8's AVX2 code.
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
  `mm-wasm` export, and images crossing native and wasm. `cargo xtask
  js-host` (release gate) runs `hosts/js`' tests (`node --test`, node 23.6
  or later) on the shipped default and bundle guests.

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
