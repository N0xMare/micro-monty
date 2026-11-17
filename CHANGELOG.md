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

## 0.0.3 (unreleased)

### Added

- A `TypeError` or `AttributeError` over a coroutine, a host future or a
  gather used where a value was expected (`json.loads(fetch())`,
  `len(coro)`, `coro['k']`, `'n: ' + fetch()`, `json.dumps(fetch())`,
  `a, b = fetch()`) ends its report with `. Did you forget 'await'?`;
  `str(e)` and `e.args` stay CPython's. Formatting through `traceback`
  costs one fuel unit more for a `TypeError` without the hint and three
  more for one with it, two more for an `AttributeError` with it, and
  nothing more for any other exception.

### Fixed

- A session whose heap is at its wall no longer keeps failing cell starts
  with `MemoryError` and fuel 0 beside garbage it could collect. A start
  at the wall reclaims dead code with its one collection, whether or not
  the host snapshotted, and a start refused after reclamation dropped
  code runs again after collecting that code's constants. Anything
  allocated between cells (a start refused part way, a host definition,
  a hint, a module dunder) leaves the next start at the wall to collect,
  and a module body's name is interned only when a function is made from
  it, so `del xs` loads without allocating. 8,000 dead string constants
  then a live fill at 64 MiB: the next three cells run; at 6 MiB, `del
  xs` after failed starts of 200 kB literal cells, or after a `define`
  before each start, runs [M].
- `Session::set_hint` and `Session::set_module_dunders` refused for room
  with no cell stopped run once more after a full collection, as a cell
  start does, also when the heap was idle, as they release before they
  allocate; one refused part way leaves what it released to the next
  start's collection, and so does a host redefinition, typed or not,
  refused after it deleted its stale definitions.
- A `reset` after a cell whose `NameError` found no room frees the old
  globals: the shared `MemoryError` raised in its place kept the error's
  hint data, the globals included, so the reset returned `Ok` and every
  later cell was refused (6 MiB, a kept fill, then `append`) [M].
- `mm-wasm-host`: a requested park no longer ends as `Error::Trap("wasm
  trap: interrupt")` when the session's thread is held past the grace
  once (preempted, or in one slow call to the output sink) before the
  guest reaches its next park check: the grace now needs as many epoch
  ticks the guest ran through as it spans, as well as the wall time, and
  one stretch the guest does not run counts as one tick. The new test
  failed 20 of 20 runs before [M].
- `Session::reset` over a heap full of live data resets the session: it
  returned `MemoryError` and reset nothing, as its new globals were refused
  while the old ones still held the data. The old globals and modules are
  now emptied in place first, allocating nothing (6 and 64 MiB, a kept
  list of strs filling the heap) [M]. An object that outlives the reset
  through the host, such as one kept on a host function's attribute, sees
  them emptied: a kept `globals()` view shows only `__name__`, an iterator
  over it stopped part way raises `RuntimeError: dictionary changed size
  during iteration`, and a kept `NameError` raised again loses its "Did
  you mean" suggestion. A `MemoryError` from `reset` now means the
  session was reset with only `__name__` unset.
- The dead-register regression listed in 0.0.2's known issues is fixed
  for every shape but a suspended generator's, which the last entry
  below covers: a register is a root only from the store that fills it
  (docs/spec.md §8.3), so what an ended statement or a returned callee's
  frame left in a register keeps nothing alive. That covers a call's or
  a display's slots not yet stored to, the targets of an unpack or a
  method load that runs Python code first, and the saved frame of a
  running generator or of a coroutine suspended by `send`. `n =
  len(load())` and then `m = len(load())`, with `load()` returning three
  50 MB bytearrays and 20,000 statements between them, prints `ok 3 3`
  at fuel 4,707,521 unparked, parked and with a snapshot at every park,
  where 0.0.2 raised `MemoryError` at 4,974,841 unparked [M]. The build
  id moves.
- Generator, coroutine and async generator methods called unbound on an
  object of another type, such as `type(gen()).close(d)`, raise CPython's
  `TypeError` and leave the object alone, where 0.0.2 wrote the
  generator's slots into it (0.0.2's known issues) [M, own case 566].
- `throw()` into a generator that has not started, and `athrow()` or
  `aclose()` on such an async generator, drop its arguments, where 0.0.2
  kept them alive as long as the generator lived; the three also check
  their `self` as the other methods do (0.0.2's known issues) [M, own
  cases 557 and 566].
- `close()` on a generator that has not started, or has finished, keeps
  the handled exception: inside an `except` block `sys.exc_info()` and a
  bare `raise` still see it, where 0.0.2 emptied it (0.0.2's known
  issues) [M, own case 558].
- A coroutine first driven by `send()` inside its caller's `except`
  block no longer takes the caller's exception as its own: after the
  handler, `sys.exc_info()` in its body is empty, an exception thrown in
  has no stale `__context__`, and a bare `raise` raises `RuntimeError`.
  This is the first half of 0.0.2's `send()` known issue [M, own case
  567].
- A generator suspended at a yield no longer keeps the dead temporaries
  its yield saved: before a collection takes roots, the suspended
  generators not cleared since they last yielded have their dead saved
  registers cleared (docs/spec.md §8.3), before an incremental mark a
  slice per step with the guest waiting, so a saved copy no longer makes
  whether a program completes depend on where a snapshot's collection
  fell. Objects that are not generators pay nothing for it. With 100 MB
  live, a generator that drops 95 MB before its yield, while an
  incremental mark runs, completes at fuel 5,570,145 unparked, parked
  and with snapshots, where 0.0.2 raised `MemoryError` at 8,137,672
  with a snapshot [M, own case 574]. The VM's
  generator registry is charged to the memory budget, and a restore
  refuses a generator whose saved registers are not a buffer its frame
  fits. The build id moves.
- A coroutine driven by hand with `send()`, suspended inside a handler of
  its own and run on after its caller left its handler, gives the
  caller's handled exception back when it returns, raises, is thrown into
  or is closed: the caller's `sys.exc_info()` no longer reads the
  coroutine's old exception for the rest of the module, and that
  exception no longer stays alive (a 180 MiB one ended in `MemoryError`
  where CPython completes). This is the second half of 0.0.2's `send()`
  known issue, except what Known issues keeps [M, own case 570].
- asyncio near the memory limit no longer drops what the loop accepted.
  A done callback, a `call_soon` entry, a timer or a task's first step
  that has no room to start stays queued: the scheduler waits for one
  full collection and tries again, and a second failure raises
  `MemoryError` at the main task's `await`, where the guest can free
  memory and await again. When that collection frees the garbage earlier
  callbacks left, the start succeeds and the `await` returns normally,
  each such collection charged (a unit per 64 bytes of the limit: 26,822
  done callbacks at 6 MiB cost 52 to 124 million units where the failing
  turns of the same cell cost 3.6 million [M]); when it frees nothing,
  every `await` raises. Before, such entries were dropped and the awaits
  returned normally. `asyncio.sleep`, `loop.create_future()`,
  `call_soon`'s queueing, `add_done_callback`, `remove_done_callback`,
  settling and cancelling a future and a task's name wait for a collection after `MemoryError` and run once more, so
  awaiting again after dropping memory needs no `gc.collect()`; before,
  `asyncio.sleep(0)` right after `pad = None` raised `MemoryError` [M].
  `create_task`, `gather` and the prelude's objects (`Event`, `Lock`,
  `Queue`, a `Handle`) still raise beside uncollected garbage. An
  exception that leaves a callback unreported
  (`SystemExit`, `KeyboardInterrupt`, or `MemoryError` in its own report)
  reaches the same `await`, also in a top-level-await cell, where
  `SystemExit` from a callback was lost with no limit set. `Semaphore` and
  `Lock` hand a permit or the lock on when a waiter gets any exception at
  its `await`, not only `CancelledError`. A callback's frame is entered by
  the scheduler, and its start is a safepoint charged one unit, so a
  `call_soon` round trip (the call and its callback) charges 8 fuel units
  instead of 12 and a done callback what it did before [M]. The snapshot
  epoch and the build id move.
- `call_later`, `call_at`, `wait_for` and `as_completed(timeout=)` make
  their deadline at the call, so a NaN delay (`ValueError: Invalid delay:
  NaN`) or a delay of the wrong type (`call_at('x')`,
  `as_completed(timeout='x')`: `TypeError`) raises there. Before, the call
  returned and the timer's task failed unseen: the timer never fired,
  `wait_for(x, nan)` returned or ended in `RuntimeError`, and
  `as_completed` raised `TimeoutError` at once.
- An exception's fields read from its `args` (`OSError.errno`,
  `StopIteration.value`, `SystemExit.code`, `ImportError.msg`,
  `SyntaxError`'s), a call's `__defaults__`, and `isinstance` and
  `issubclass` with a tuple read the items they need from the tuple in
  place; with millions of items each was one uncharged step (0.0.2's
  known issue "Steps past the 50 ms bound": 73 ms a field read, 85 ms a
  call, up to 2.0 s for `isinstance` [M, on e54f65f]). A tuple longer
  than a native walk goes to the prelude, a visit per item under fuel.
  `e.args = t` keeps `t` itself, as CPython does.
- `except t` with a tuple longer than a native walk checks it in the
  prelude, in charged steps, in CPython's order and with its errors
  (0.0.2's known issue: 8,000,000 classes were one step of 206 ms [M, on
  e54f65f]); a user class's MRO is read in place, so an `except` or
  `isinstance` over a class chain thousands deep is no longer one step of
  110 to 258 ms.
- `copy.copy` and `copy.deepcopy` keep an `AttributeError`'s `name` (it
  was `None` on the copy; `obj` is not copied, as in CPython) and set an
  exception's state with `setattr`, as `BaseException.__setstate__` does.
  A shallow copy of an `OrderedDict` subclass instance keeps its
  attributes: `OrderedDict` has CPython's `__reduce__` and no `__copy__`,
  so the copy is the class called with nothing, the attributes, then each
  item of `items()` stored with `[]` (0.0.2's known issue, among the
  smaller differences). The build id moves.
- `UnicodeDecodeError` and `UnicodeEncodeError` have CPython's `encoding`,
  `object`, `start`, `end` and `reason` (0.0.2's known issue: they had
  none, and `args` was one message). `args` is those five for the codecs'
  own errors and for constructed ones. The two types have their own
  `__init__` and `__str__`: `__init__` checks the arguments as CPython's
  does and keeps the five fields, which a later `args` does not change,
  and the members check what is assigned to them; `str()` is made from
  the fields. `object` is the value of the `str` an encode was given
  and a `bytes` equal to what a decode read: the input itself when it is
  a `bytes`, else a copy (of a `bytearray`, or of what follows
  utf-8-sig's BOM) made once, a step at a time, which waits for a
  collection beside garbage.
  `xml.etree.ElementTree` reports ill-formed bytes as `ParseError` (it
  reads `e.start`, and raised `AttributeError`). The build id moves.

### Known issues

- A start at the wall reclaims dead code, and with it the names only
  that code held; a later cell that uses such a name again (the fill's
  `append`, a loop's `pop`) can be refused at the wall where 0.0.2,
  still holding the dead code, ran it: 20 of 6,000 random sessions at 2
  to 8 MiB, against 305 where 0.0.3 runs a cell 0.0.2 refused [M].
- A session at its wall on live data collects at every start refused
  part way (one full collection, 47 ms a start at 256 MiB, where 0.0.2
  refused it without one), and a start that completes takes 9 to 17 ms
  more for reclamation's reference scan: 67 ms mean on a 256 MiB live
  heap of 4M strings, against 50 ms [M].
- `mm-wasm-host`: an output sink slower than about 2.3 ms a call can
  still run a requested park's grace out for a cell that prints in a
  loop, as in 0.0.2: every sink call that crosses an epoch tick counts
  one, and the next park check can be 65 `print` calls away (64 MiB
  limit; a 3 ms sink traps after about 49 calls) [M].
- A collection step is not yet bounded where every task runs between
  collections. A collection skips the frames of an asyncio task that has
  not run since its dead registers were last cleared, so with tasks that
  stay blocked the longest step is no longer than before 0.0.2's
  dead-register clearing; but the first collection after every task ran
  walks all their frames. With 1,000 tasks of 500 frames each waking
  between collections, the longest collection call is 44.4 to 46.5 ms
  natively against 34.3 to 36.9 ms on 71b9c5b, the build before the
  clearing, and on wasip1 the first step over 60 ms reads 60.3 to 63.5
  ms where 71b9c5b stayed under 60 ms [S, the GC design review's b4
  measurements, on an earlier base; not rerun]. Marking each caller
  record cleared instead of each task would bound it.
- A coroutine whose body enters a handler under one `send()` and is run
  on by a later one still shows that handler's exception to its body,
  as the one being handled, until it next returns, raises or is closed
  (`sys.exc_info()` in its body, a bare `raise`, the `__context__` of
  what it raises); a generator does the same. Older than 0.0.1.
- `Semaphore.release()` that raises `MemoryError` (its waiter's callbacks
  have no room) has already added its permit and woken no one; a guest
  that retries the call releases twice. A second `release()` reaches
  CPython's end state.
- A gather passed to `add_done_callback` as the callback is accepted, and
  each settle counts as one of its children finishing: registered on two
  futures, a gather of two pending futures ends done with `[None, None]`.
  python3.14 reports `TypeError: '_GatheringFuture' object is not
  callable` for each and the gather waits for its children.
- A `UnicodeEncodeError` raised by encoding an instance of a `str`
  subclass holds the plain `str` as its `object`, where CPython's holds
  the instance itself (`e.object is s`).
- `gen.throw(UnicodeDecodeError)`, the class with no arguments, raises
  its constructor's `TypeError` at the caller, where CPython raises it
  inside the generator. A user class whose `__init__` needs an argument
  is thrown in without its `__init__` run, as in 0.0.2.

## 0.0.2 (2026-11-18)

The second release, from `7e59d6c` (this entry as of 2026-11-18). Every API may have changed since 0.0.1, and images written
by 0.0.1 do not restore: the build ids below are new.

### Build ids

A snapshot restores only on a build with the id that wrote it [M, on
the release commit, `micro-monty`'s `build_id` example]:

- `06717fdeb70bb48b5e5461b21b1816428052fc4f71846c8b79cefbb14713231d`: the facade's default features: an
  embedding with no opt-in module, the shipped mm-wasm guest and the
  bundle guest, and `mm-cell-worker` built on its own.
- `099c0eda0275bba7194dc97691e2f5e475b416f4e86c75d58f2da4bdcb9e0a06`: with `mod-uuid`, `mod-unicodedata`,
  `mod-html-entities` and `mod-ast`, as `mm` (native and `wasm32-wasip1`)
  builds it, and any binary built together with `mm`.

### Published crates

The same six as 0.0.1, at 0.0.2 and pinning each other with `=0.0.2`:
`micro-monty` and `mm-core`, `mm-compile`, `mm-parse`, `mm-engine` and
`mm-stdlib`. Still not published: `mm-cli`, `mm-cell-worker`,
`mm-wasm-abi`, `mm-wasm-guest`, `mm-wasm-host` and `mm-wasm-alloc`. The
GitHub release also attaches the two mm-wasm guests.

### Added

- `xml.etree.ElementTree` (features `mod-xml`, default on): CPython's
  Element, ElementTree, ElementPath paths, TreeBuilder and serializer over
  a small non-validating parser in place of expat, with expat's messages,
  codes and positions for `ParseError`. It expands the predefined
  entities, character references and a DOCTYPE's internal entities; no
  `iterparse`, `XMLPullParser`, `canonicalize`, `xml.dom` or `xml.sax`.
- `email.utils`'s date functions (`mod-email`, default on):
  `parsedate_to_datetime`, `parsedate`, `parsedate_tz`, `mktime_tz`,
  `formatdate` and `format_datetime`. No address or message parsing.
- `html.unescape` in a build without `mod-html-entities` resolves HTML 4's
  252 named references (CPython's `html5` entries for them, with and
  without the semicolon), not only the five of `escape()`.
- The codemode census passes 1,722 of 1,722 CPython-passing solutions on
  the default build (1,701 before). `hello.wasm` +17,780 B, native `hello`
  +18,432 B [M]. The default build's id moves.

### Changed

- Shipped `wasm32-wasip1` modules (`mm`, `hello`, `roundtrip`, the wasip1
  guests) go through `wasm-opt -Oz` with inlining capped at 16 KiB per
  function: wasmtime runs fib in 4.9% and closure in 7.5% fewer guest
  instructions (bench geomean 0.975), and `mm.wasm` grows 729 B [M].
  `wasm32-unknown-unknown` modules, what V8 runs, are byte-identical. The
  arguments have one owner, xtask's `wasm_opt_args`, and one entry,
  `cargo xtask wasm-opt TARGET IN OUT`, which
  `hosts/js/scripts/build-guests.py` now runs, so building the guests
  needs binaryen 133 exactly and a built xtask.

- A host record's fields read as its fields whatever their names: a field
  named `count` or `index` returned the tuple method before. Each field is
  a property, as in a namedtuple, and the class has `__match_args__`, so
  `case R(a, b)` matches positionally.
- `import xml` and `import email` no longer carry a redirect hint;
  `HintGroup::XmlEmail` covers no modules and keeps its key (`xml`), so a
  host that sets it still succeeds.
- A host `Record`'s names are checked as `collections.namedtuple` checks
  them, in its order: `ValueError` with its messages (was `TypeError`),
  and `SyntaxError` for fields NFKC makes equal, such as `ﬁ` and `fi`. A
  field name holding a character whose NFKC micro-monty does not carry,
  such as `ℌ`, is refused with the lexer's `SyntaxError`.
- `collections.namedtuple` refuses two fields NFKC makes equal with
  CPython's `SyntaxError` (after its `defaults` check) where micro-monty
  carries the mapping: fullwidth ASCII, mathematical Latin letters and
  digits, the Latin ligatures, `ſ`, `ª`, `º`, `µ`. Characters
  micro-monty maps are replaced and the rest kept, so `['café', 'á']` is
  accepted and `['ｅ\u0301', 'e\u0301']` refused, as in CPython, but
  pairs CPython refuses are accepted too: of the 3,640 single code points
  whose pair CPython refuses, micro-monty refuses 761 and accepts 2,879
  (`ℌ` and `H`, CJK compatibility ideographs, Arabic presentation forms),
  and composed sequences such as `['é', 'e\u0301']`. Such fields never
  collide: each reads as itself. Where a refused field keeps a character
  CPython's NFKC changes, the message names micro-monty's key
  (`'e\u0301'` where CPython names `'é'`); the offsets are CPython's.
- `str.isidentifier` is exact for Unicode 16 without `mod-unicodedata`
  (it accepted `x²` and refused `a·b`): XID_Start and XID_Continue tables
  of 4,388 B. With the name check, `hello.wasm` +6,357 B, native `hello`
  +6,176 B [M].
- `mm-compile`'s `CompileError` is boxed: it is now
  `CompileError(Box<ErrorData>)`, and its `kind`, `msg` and `pos` fields
  moved to the new public `ErrorData`. Reads such as `e.msg` still compile
  through `Deref`; a struct literal becomes
  `CompileError::from(ErrorData { .. })`, and moving a field out needs a
  clone. A `Result<(), CompileError>` shrinks from 56 to 8 bytes on x86_64.
  The shipped `hello.wasm` falls by 54,039 B and native `hello` by
  42,912 B [M]. The facade's `micro_monty::CompileError`, the mm-wasm ABI
  and the build id do not change.
- `SessionTemplate::instantiate` restores its image checked once per
  template, then skips the validation walks (strings, references, dict
  indexes, bytecode, generators, native steps) and keeps the checksum,
  the bounds checks done while reading and the memory charges. A
  template from `from_image` is checked by that call; one from `capture`
  by its first instantiation. A tools template (eight typed host
  functions, `json`, `asyncio`) instantiates in 77.8 µs instead of
  175.1 µs, and restore Ir falls 49–66% [M]. `Engine::restore` stays fully
  checked. The build id moves.
- Images refer to the code of the prelude's units (the json and asyncio
  classes and functions, for one) by the unit's place in the packed
  prelude, as they refer to bundled modules' code, instead of storing it:
  a tools template's image is 85,313 → 78,972 B. A restore takes the
  unit's decoded program from the engine's module cache, or decodes it
  once and offers it there, and charges the session the same either way.
  A tools template's checked restore falls from 1.75M to 0.96M Ir
  (175.1 to 80.2 µs), a trusted instantiation to 0.43M Ir (39.5 µs) [M].
  The build id moves.
- Imports intern less. A loaded code keeps its interned name and
  qualified name, so making a function no longer interns them. A
  program's load interns each distinct string once. The intern table
  grows and probes without revalidating each slot. Per session [M]: the
  five-module cell (`json`, `re`, `collections`, `asyncio`, `datetime`)
  falls from 13.11M to 11.13M Ir, `import json` from 1.28M to 1.22M Ir,
  and `startup` from 0.585M to 0.554M gated Ir. Images carry each
  code's two names, so restores read 16 more bytes per code: a codemode
  template's image is 3.1% larger and its trusted restore 4.8% slower in
  Ir. The build id moves.
- A session no longer stays unable to start a cell after memory filled:
  a cell that filled the heap and dropped its data left garbage nothing
  collected, so every later cell, and `reset` and host definitions beside
  that garbage, failed with `MemoryError` and fuel 0 (older than 0.0.1). A
  cell start on a heap at its wall now collects first, uncharged and
  outside the step bound: 7–34 ms over 256 MiB of garbage, 38–105 ms over
  256 MiB live, repeated at each start while the heap stays there, also
  when the cells succeed (about 26–32 ms a start with 256 MiB of live
  strings) [M]. `reset` over a heap full of live data still fails.

### Measured at release [M]

On the release commit's tree (`7e59d6c` with the version bump):

- 546 of the 568 CPython-scored cases of Monty's test corpus
  (`conformance/results/micro-monty.md`), as at 0.0.1.
- 2,841 recorded cells from 160 real code-mode sessions replay as
  `conformance/codemode/real/results/gate.jsonl` records them
  (`gate.py --check`): the same 53 cells as at 0.0.1 differ from their
  recording, each with its reason there.
- The codemode census: 1,722 of the 1,722 CPython-passing solutions pass,
  with CPython's tool calls batch by batch (`conformance/codemode`).
- Shipped sizes (`cargo xtask size`), against 0.0.1: `hello.wasm`
  2,233,281 B (−38,688 from 2,271,969), native `hello` 2,413,200 B
  (−77,120 from 2,490,336), `mm_wasm_guest.wasm` 2,324,026 B (−34,846
  from 2,358,872), `mm_wasm_guest_bundle.wasm` 2,322,562 B (−34,745 from
  2,357,307).
- Instructions against v0.0.1's own counts, the pre-tag forms of the Ir
  gates:
  - Gated native Ir on the shipped `mm` (`native-ir --base v0.0.1`):
    `startup` 0.540M against 0.584M (−44,129 instructions), the programs
    0.9904 (`interp`) to 1.0076. Two are past +0.5%: `exceptions` 1.0076
    (the bounded-step and memory work, accepted in `763db34`) and `dict`
    1.0066 (1,163.2M to 1,170.8M), the sum of the cycle's dict and set
    work, each step inside its own gate: charged set-insert walks and
    table rebuilds (`901e0377`, +0.32%), lookups charging the slots they
    pass (`1827fdcb`, +0.25%), hot functions taken off `optimize(size)`
    (`f147bc0d`, +0.22%), list extend interleaving (`cdf5c099`, +0.21%)
    and twin inserts rebuilt in steps (`6a188bc0`, +0.20%), less the
    cold functions put under it (`01519d07`, −0.19%), the heap's
    medium-block reuse (`1d7c5698`, −0.15%) and Rust 1.100 (−0.09%).
  - wasmtime guest instructions (`wasm-ir --base v0.0.1`): geomean
    0.9932, `startup` −101,801 instructions, the programs 0.9635 (`json`)
    to 1.0054 (`fmt`).
  - V8's generated code on the `wasm32-unknown-unknown` guests (`v8-ir
    --base v0.0.1`): geomean 0.9973 on the plain guest and 0.9972 on the
    bundle guest, the programs 0.9874 (`interp`) to 1.0049 (`str`).
  - The 159 compared real sessions replayed under callgrind against
    v0.0.1's worker (the `replay-ir` gate, which lands after this
    release; run from its own commits over this tree): Ir without the
    allocator 0.9921 in total and 0.9755 at the median, allocator calls
    0.9235 in total; `phase1.codex-sdk-monty.codec-review` diverges on
    this build, as in `gate.jsonl`, and is compared on neither side. Past
    its limits, and recorded here instead of an accept line:
    `phase2.grok-ptc-wasm.todo-census` at 1.0183 (`sessions 1.019`): one
    more full collection with code reclamation at a cell start (three
    against two in the session), because the prelude's globals start
    presized (`0e50b7b`) and move the session across the collection
    threshold, not the parser; and the allocator calls of three
    codex-sdk sessions, 1.0051 to 1.0063 (`sessions-calls 1.007`):
    `collections.namedtuple`'s NFKC check of its field names (`462adb7`)
    allocates and frees a few more blocks per class, 1.2–2.5% more calls
    than before it on those sessions and 0.5–0.6% more than 0.0.1, every
    one freed.

### Known issues

Each was found or carried during this cycle and is planned for 0.0.3;
none is a regression of this release unless it says so.

- Restores are still not fully transparent. 0.0.1 planned all three of
  its restore-transparency fixes for 0.0.2; all three moved to 0.0.3 with
  the collector work they need (docs/spec.md §5.1, guarantee 4):
  - Near the memory limit, an operation the engine does not run again
    after a collection can raise `MemoryError` unparked and succeed after
    a snapshot, or the reverse, because a snapshot collects first.
  - An operation that fails part way for memory is charged for what it
    did, so a snapshot can move the fuel total: +28,672 units (+0.64%) on
    `f(*big, **kw)`, and 11% less fuel with `ok ME ok ME ok ME` for six
    such calls that print `ok` six times unparked [M].
  - Indexing or searching a long non-ASCII str is charged its
    char-position walk again after every snapshot: +9,775 units with
    snapshots and no restore, +826 more with restores, on
    `conformance/own/419` [M].
- A register that an ended statement left a value in can keep that value
  alive: a call or a display allocates its slots before it evaluates an
  argument, and a generator's saved frame keeps what its last `yield`
  saved, also while it runs. So a program that drops one large object
  and makes another can raise `MemoryError` where python3.14 completes:
  `n = len(load())` and then `m = len(load())`, with `load()` returning
  three 50 MB bytearrays, under the default 256 MiB limit [M]; the same
  through an unpack or a method load that runs Python code, and through
  a generator's temporaries. Whether it does can depend on where a
  snapshot's collection fell, a regression of this release (the live
  top it added to collections): with 20,000 statements between the two
  loads, `MemoryError` at fuel 4,974,841 unparked and `ok 3 3` at
  4,707,521 with a snapshot every 13, 997 or 5,000 parks, restored [M].
- asyncio near the memory limit can drop an entry it accepted: a done
  callback, a `call_soon` handle, a timer or a task's first step that has
  no room to start is lost without an error (2 callbacks and 2 `call_soon`
  entries at 6 MiB [M]). The fix moved to 0.0.3 whole.
- Memory freed inside a cell is not reused by that cell: after a
  `MemoryError` and `del xs`, a later statement of the same cell can raise
  `MemoryError` again (`print`, `import json`, `from collections import
  Counter` at 6 and 16 MiB) where python3.14 completes, because nothing
  allocated since the last collection. `gc.collect()` after the `del`
  works around it; the next cell recovers (Changed, above) [M].
- While live data fills the heap to its wall, every cell start runs a full
  collection, also when the cells succeed: 26 to 32 ms a start with
  256 MiB of live strs, 203 cells in 6.9 to 7.1 s against 1.1 to 1.2 s on
  the build before [M]. This is the cost of letting a session run again
  after memory filled (Changed, above).
- `Session::reset` over a heap full of live data returns `MemoryError`
  and resets nothing, so it cannot recover such a session; beside garbage
  it succeeds [M]. Older than 0.0.1.
- Two kinds of garbage are still left beside a full heap with nothing
  that collects them, so the next cells fail with `MemoryError` and fuel
  0 as before this release's fix: the constants of a start whose program
  load was refused part way (at 6 MiB, two failed starts of a cell with
  a 200 kB literal leave 400 kB less memory), and the constants of dead
  cells' code that a reclamation between cells drops after its own
  collection (8,000 dead string constants, then a live fill to a caught
  `MemoryError`: the next three cells fail at 64 MiB) [M]. A cell that
  loads without allocating, or `gc.collect()` in one, recovers.
- A run that was parked once has 24 bytes less memory than one that never
  stopped, so at a memory limit its `MemoryError` can come one step
  earlier (25,962 against 25,961 entries in a 6 MiB reproducer) [M].
- A coroutine driven by hand with `send()` confuses its handled exception
  with its caller's. Started by `send()` inside the caller's `except`
  block, it takes the caller's exception as its own: after that handler,
  `sys.exc_info()` in its body reads it, an exception thrown in takes it
  as `__context__`, and a bare `raise` re-raises it where CPython raises
  `RuntimeError` [M]. Suspended inside a handler of its own and run on
  after the caller left its handler, it leaves that old exception as the
  caller's handled exception when it returns, raises, is thrown into or
  is closed: the caller's `sys.exc_info()` reads it for the rest of the
  module, later exceptions take it as `__context__`, and it stays alive,
  so a large one can end in `MemoryError`. A generator, and a coroutine
  after a handler in its body that began under an earlier `send()`, see
  that stale exception until they return, raise or are closed. Older
  than 0.0.1.
- `UnicodeDecodeError` and `UnicodeEncodeError` have no `encoding`,
  `object`, `start`, `end` or `reason`, and their `args` is one message;
  so `xml.etree.ElementTree` reports ill-formed bytes with
  `AttributeError` instead of `ParseError` [M].
- Generator methods: called unbound on an object of another type, such
  as `type(gen()).close(d)`, they do not raise `TypeError` and write a
  generator's slots into it (a dict `{'a': 1, 'b': 2, 'c': 3}` reads
  `{'a': None, None: None, 'c': 3}`) [M]; `close()` on a generator that
  has not started, inside an `except` block, empties the handled
  exception [M]; `throw()` into one that has not started, and `athrow()`
  or `aclose()` on such an async generator, keep its arguments alive as
  long as it lives.
- Steps past the 50 ms bound on this release's reference host [M]:
  `del b[::2]` on a 64 MiB bytearray (69.4 to 70.7 ms); a greedy
  `(a|b)*c` over 450,000 chars and more (228 to 327 ms per slice end, and
  `MemoryError` past about 650,000 chars, where CPython completes); a
  collection with 1,000 tasks of 500 frames that all run between
  collections (44 to 47 ms, past 60 ms on wasip1); single dict lookups,
  `map(d.get)`, `%`, itemgetter, comparisons and items views over many
  colliding keys (55 to 80 ms). Tuples of millions of items, each in one
  step [M, on e54f65f]: `except t` with 8,000,000 classes (206 ms); the
  fields `OSError.errno`, `StopIteration.value`, `SystemExit.code`,
  `ImportError.msg` and `SyntaxError`'s read from such an `args` (73 ms
  a read, 563 ms for `SyntaxError.filename`); a call that takes such a
  `__defaults__` (85 ms); `isinstance(x, t)` (254 ms to 2.0 s); and
  `isinstance` or `except` over a class chain thousands deep (110 to
  258 ms).
- Restoring a forged image can hang instead of being refused.
- Smaller differences from CPython: no `locals` builtin; dict bulk
  operations never raise `dictionary keys changed during iteration`, and
  some results depend on the table's layout; `BaseException.__init__` and
  `__str__` called unbound skip the receiver check, `__get__`,
  `__qualname__` and `KeyError`'s quoting; `True + I(1)`, `0.5 + I(1)` and
  `J(1) + I(1)` call an int subclass's `__radd__` where CPython does not;
  `in`, `find`, `count`, `index`, `rfind` and `rindex` on `bytes` and
  `bytearray` refuse an object with `__index__` with `TypeError`, where
  CPython takes it; `add_done_callback(cb, context=ctx)` does not run `cb`
  in `ctx`; `copy.copy` and `copy.deepcopy` of an `AttributeError` lose
  its `name`, and a shallow copy of an `OrderedDict` subclass instance
  loses its attributes [M].

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
