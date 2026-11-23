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

- A `NameError` for a module name says "Did you forget to import 'x'?"
  for every module `import` finds in the build, `tomllib`, `fnmatch`,
  `binascii`, `numbers` and others newly, and no longer for one it
  cannot import (`argparse`, `array`, `builtins`, `inspect`, `queue`,
  `secrets`, `timeit`, `unittest`, `weakref`, or a bundled module whose
  feature is off). A module the sandbox leaves out (`subprocess`,
  `socket`, `shutil`, ...) gets its redirect sentence instead of an
  import hint, and the other names of CPython's 297
  `sys.stdlib_module_names` get none (213 with the default modules, 210
  with `mm`'s) [M, own case 603]. The engine's
  list of 71 names is gone: `hello` is 2,512 bytes smaller [M].
- `exit()` and `quit()` are `sys.exit`, and `input()` raises
  `EOFError: input(): there is no stdin in this sandbox; put the value
  in the code or get it from a tool` after writing its prompt, as
  CPython does with stdin at its end, so an `except EOFError` fallback
  is taken; each was a `NameError` that
  said nothing of what to do [M, own case 590]. With three more builtins
  to compare, a `NameError`'s name hint a guest formats through
  `traceback` costs 3 fuel units more [M].
- A `TypeError` or `AttributeError` over a coroutine, a host future or a
  gather used where a value was expected (`json.loads(fetch())`,
  `len(coro)`, `coro['k']`, `'n: ' + fetch()`, `json.dumps(fetch())`,
  `a, b = fetch()`) ends its report with `. Did you forget 'await'?`;
  `str(e)` and `e.args` stay CPython's. Formatting through `traceback`
  costs one fuel unit more for a `TypeError` without the hint and three
  more for one with it, two more for an `AttributeError` with it, and
  nothing more for any other exception.
- `x86_64-unknown-linux-musl` is a tested platform: `cargo xtask check`'s
  `musl` stage builds `mm` there as shipped (static-pie), runs the own
  cases and Monty's corpus on it, and runs the worker's tests there; the
  release gate's `steps-musl` stage runs the timed step-time oracle there,
  with musl's `malloc` and with mimalloc.
  The workspace tests, durability and the recorded sessions (in memory
  and durable, and on the seccomp-filtered worker binary) also pass on it
  [M].
- `mm` and `mm-cell-worker` built for musl register mimalloc (MIT, C
  compiled with `musl-gcc`, which `.agents/setup` installs) as their
  global allocator; the engine crates and the facade take no new
  dependency. On musl only, the worker's seccomp filter allows what mimalloc calls,
  out of memory included: `getcpu`, which it calls on a machine with two
  or more NUMA nodes, and `writev`, with which it prints when
  `MIMALLOC_SHOW_ERRORS` or `MIMALLOC_VERBOSE` is set. The confined
  worker obeys its parent's `MIMALLOC_*` variables. The musl binaries
  need a CPU with POPCNT. Over the 158 typical recorded sessions in
  memory, the release-recipe worker binaries take 3.33 to 3.37 s of cell
  time on glibc, 4.79 to 4.89 s on musl with musl's `malloc` and 3.72 to
  3.81 s with mimalloc; the worker grows by 146 KiB [M].
- `Session::setup_done()`: the code loaded so far is the session's setup
  and does not count toward the trigger that reclaims unreachable code
  when a cell starts (a full collection once 16 KiB of code, or an eighth
  of the live heap, was loaded since the last one). A setup's code stays
  in use, so counted, it brought that collection forward to a session's
  first cells, where it frees nothing. The mark is not in an image: a
  host calls it again after `SessionTemplate::instantiate` and
  `Session::reset_to`, whose sessions otherwise count all the template's
  code and collect at their first cell's start, as before.
  `Session::reset` starts the count anew. The setup's code stays charged
  to the memory limit, and what a cell drops of it is freed by the next
  reclamation. `mm-cell-worker` calls it after each bootstrap it runs or
  resets to. The mm-wasm guest exports it as `mm_setup_done`
  (`ABI_VERSION` 2, a required export; a host built for version 1 refuses
  the guest and the reverse): `Session::setup_done` on the reference host
  and `session.setupDone()` on the JavaScript host, called after the
  definitions and the bootstrap cell, after a setup run again following a
  reset, and after restoring a setup's image.
  Over the recorded sessions on the replay worker, against the build
  before: the 79 fresh-per-cell (py-sdk) sessions counted take a median
  0.893 of their instructions (collections 1,952 to 362: a session reset
  to its template no longer collects at every cell's start), and the
  persistent (py-ptc) sessions of five cells or fewer a median 0.960
  (collections 23 to 5 in 29 sessions); allocator calls do not move [M].
  Not in an image: no snapshot format change.

### Changed

- A new session starts in fewer instructions: `Vm::new` binds the builtin
  functions and the prelude's natives from two lists made at compile time
  with the natives table, instead of comparing every native's name with
  every type's and every native's module with the prelude's at each
  start, and the `Engine`'s module-cache slots for decoded modules and
  decoder tables are allocated on first use. `mm run` of an empty program
  takes 18.7% fewer instructions natively (535,676 to 435,751) and 21.4%
  fewer on wasm32-wasip1 under wasmtime (855,689 to 672,382) [M]. The
  builtins, the prelude's globals and a new session's image are the same,
  byte for byte, and the build id does not move.
- The Huffman decoder tables of the bundled modules and the prelude are
  built once per `Engine` and kept in its module cache (about 90 KB),
  instead of once per prelude chunk part and per cell or restore. A
  restore on a fresh `Engine` builds each table at most once: over 6,241
  recorded code-mode images it takes 7.5% fewer instructions (53.9 G to
  49.9 G) and its native p50 falls from 1.09 to 0.97 ms [M]. Charges and
  behavior do not change: each unpacking is still charged its decoder
  workspace.
- A restore decodes the bundled modules and prelude units its image names
  without their instructions, and decodes a program whole only when one
  of its codes first runs; every check of the image still runs at
  restore. With the table reuse above, a restore on a fresh `Engine`
  takes 26% fewer instructions over 6,241 recorded code-mode images
  (54.4 G to 40.0 G; on a warm `Engine` 27.0 G either way), its p50 falls
  from 1.13 to 0.77 ms natively and from 2.68 to 1.77 ms on a new
  wasmtime instance, and a durable replay of the 158 typical recorded
  sessions takes 6.5% fewer instructions in all (108.7 G to 101.7 G) [M].
  Charges and fuel do not depend on whether code was decoded yet.

- `SessionTemplate::instantiate` and `Session::reset_to` no longer hash the
  template's image: a trusted read skips the checksum as it skips the
  validation walks. The bytes are the template's own, written by `capture`
  in this process or checked once in full by `from_image`, and never
  written again. Not detected any more: those bytes changing in memory
  after the capture (a hardware fault); `Engine::restore` and `from_image`
  check as before. Over the 79 recorded py-sdk sessions the median falls
  4.8% in instructions and the best 10.9% [M]; the others do not move.

### Fixed

- `cancel()` of a large gather or task tree no longer walks it in one
  native step: past 16,384 units of work (a visit, and more for a settle
  that scans the blocked tasks or a long waiters tuple) the walk goes on
  a step at a time, a safepoint after each, also when the loop itself
  cancels (a task that blocks with a cancel pending, the main task when
  `SystemExit` leaves a task). On shipped codegen `G.cancel()` of a
  gather of 1.2M futures was one step of 87 to 187 ms and is now steps
  of at most 2.5 to 2.6 ms; of 30,000 sleeping tasks one of 1.66 to
  1.88 s, now 5.1 to 5.6 ms; a task blocking on 1.2M futures with a
  cancel pending one of 91 to 104 ms, now 3.2 to 3.4 ms [M]. A cancel
  that fits one step runs as before, at the same fuel; a longer one the
  program calls charges a unit or two more a step (1,200,295 against
  1,200,002 units at 1.2M futures [M]), the loop's own the same. A
  cancel parked part way snapshots and restores; nothing else runs
  between its steps. The total time of cancelling many blocked tasks is
  unchanged (quadratic in them). The build id moves [M, own case 579].
- The asyncio scheduler no longer searches its blocked tasks or its
  tracked gathers at a wake or a settle: each knows its place in its
  list. On shipped codegen, 30,000 sleeps due together were one step of
  141.9 ms and are now 11.9 ms; settling a future that 30,000 gathers
  listen to, 199.8 ms, now 4.7 ms; a loop idle on a gather of 40,000
  futures, 233.0 ms, now 7.2 ms; cancelling 40,000 tasks each waiting on
  a future took 631 ms in all, now 86 ms [M]. Results, the order tasks
  resume in and fuel are unchanged, and an image holds what it held (the
  places are kept in memory only). The build id moves [M, own case 595].
- A gather is done when its last child's done callback runs or, without
  `return_exceptions`, that of its first failed or cancelled child, as
  CPython 3.14's `_done_callback` counts, and no longer one loop
  iteration earlier when a child settled while another child's
  notification was still queued; its waiters and done callbacks run
  then [M, own case 601]. A queued notification names its child and a
  gather keeps two counts, so an image holds more: `SNAPSHOT_EPOCH` 16
  and the build id moves.
- An `except MemoryError` handler that drops the data that filled memory
  can allocate again. Displays (`[...]`, `(...)`, `{...}`, `x[a:b]`),
  `lambda` and `def`, `str`/`int`/`float` of numbers, strs and bytes,
  builtin exceptions and instances of classes without `__init__` or
  `__new__` now wait once for a collection when refused, as string
  operations and copies did: after `xs = None`, `d = {'k': [1, 2, 3]}`
  raised `MemoryError` again at 8, 16, 32, 64 and 256 MiB with nearly the
  whole heap garbage, and ran at 2, 4 and 12 MiB only because the fill
  happened to be refused at a large request [M]. Other first statements
  may still be refused, for example an instance of a class with
  `__init__`, `object()`, `slice(1, n)` or a failing `assert`. A handler
  that keeps the data can still be refused: it has only the room the fill
  left.
- An operation refused after it had entered a frame is raised in the
  frame it started from. `g.throw(e)` into a generator delegating with
  `yield from`, with room under the recursion limit for the generator's
  frame but not its sub-generator's, left the generator "already
  executing" for good; it now ends with the `RecursionError`, as
  `next(g)` there already did and as CPython ends a generator whose
  resumption is refused. (A generator whose own frame is the one refused
  is still left as it was, where CPython ends it.) `asyncio.run` refused the memory for its task after
  entering the coroutine ended in `AttributeError: 'bool' object has no
  attribute 'aio_stop'` instead of `MemoryError`. A coroutine driven by
  `send()` and a host function called as a dunder no longer leave frames
  entered when their next step is refused.
- A `repr` or `str` of nested containers that is refused memory or stack
  part of the way no longer leaves a container marked as being rendered:
  the same `repr` showed it as `[...]` afterwards, and an outer level
  could end in `KeyError` instead of the `MemoryError` [M].
- `Session::define` of a typed function, a class or an exception no longer
  fails with `SystemError: host definition code did not complete` when a
  park is due: after `request_park()`, with a deadline that has passed, or
  with fuel 0 or 1 set (since 0.0.2; on the wasm guests, whose clock stays
  armed, after any cell with a request pending). The generated code now
  runs to its end off the session's fuel, deadline and clock: a define
  charges the session no fuel, and a park request stays for the next cell
  (it was consumed by the failed define).
- A collection that dropped names sharing one run of the intern table's
  slots no longer takes seconds in one step. Attribute names computed at
  run time are interned (`hasattr`, `getattr`, `setattr`), and guest
  `hash()` shows which bucket a name gets, so a guest could pick names
  that fill one run; each dead one was removed by shifting the rest of
  the run back. 36,000 such names, dropped: 1.67 s in the collection's
  last step, now 5 to 6 ms: a run that holds a dead name is laid out
  again once, its survivors sorted by bucket. The table doubling over
  such a run probed for every entry past the ones before it (235 ms for
  34,000 names): the doubled table now takes the entries sorted, with no
  probe (5 ms). The table's slots are laid out otherwise than before,
  so `SNAPSHOT_EPOCH` and the build id move; results and fuel are those
  of the build before on every own case, Monty's corpus and the
  recorded sessions [M, `--release`, x86-64].
- A lookup in the intern table no longer walks a run of names a guest
  picked. Every path to the table walked it: computed attribute names
  (88 to 112 ms between two looks at the clock over a run of 72,000),
  an import's name (198 ms), a cell's string literals and names while
  it is loaded (2,000 of them: 1.2 s with no safepoint), and the
  one-char strs every str constructor shares (`list(text)` of 20,000
  chars: 10.3 s in one step). Now no interned string lies more than 128
  slots past its hash's slot (`INTERN_WALK_CAP`): an insert that finds
  no free slot that near grows the table first, in one pass, to the
  size at which the names near it part, and a lookup reads at most 129
  slots. Each entry carries a byte of its string's hash, so a lookup
  passes an entry of another hash without reading its string. The
  empty str and the chars up to U+00FF are also found without the
  table, through a cache the engine keeps beside it (the same objects:
  `'a' is chr(97)`). The rows above now take 0.1 to 20 ms. A checked
  restore refuses a table with an entry past the cap, behind a free
  slot or without its hash's byte; the build id moves. Names crowded
  on one place of the largest table (4,194,304 slots) raise
  `MemoryError`: about 130 names whose hashes agree in 22 bits, which
  a guest finds among some 4 million candidates. Such names leave the
  table at that size (Known issues) until `reset()`, which now replaces
  a table far larger than the names left need by a small one: after
  it the session's image is 51 kB again, not 33.6 MB [M]. Results and fuel are
  those of the build before on every own case, Monty's corpus and the
  recorded sessions, none of which grows the table for a crowded name
  [M, shipped codegen, x86-64].
- `reset()` gives back the memory earlier cells committed and no longer
  use, without moving an object: after a cell filled a 3 MiB heap with
  4 KB buffers, a 1 MB `bytearray` raised `MemoryError` after the reset
  and a 200 KB fill fit 4 buffers against a new session's 15; now 1 MB
  and 2 MB fit and the fill fits 15, also across a snapshot and restore
  [M].
- `Session::reset` is a boundary: a cell after it reports what it
  reports in a new session with the same host definitions. Before, a cell
  could leave state and code for the cells after a reset [M]:
  `read.__defaults__ = ('/etc/shadow',)` on a typed host function changed
  what the next cell's `read()` sent to the host; `Entry.__repr__ = f` on
  a host class (or a host exception class's `__str__`, or a record's
  class) ran `f` in the next cell, in its globals and with its host
  functions; a list or dict constant of the builtins module kept what was
  appended; `json.dumps.__kwdefaults__['sort_keys'] = True`,
  `sys.set_int_max_str_digits(640)`, `sys.version_info._t = (2, 7)` and
  `sys.setrecursionlimit` reached it; and 40 MB left on any of them stayed
  charged. Now guest code cannot set or delete attributes of what every
  reset shares: the functions and classes the prelude defines
  (`asyncio.Future.leak = 1`, `json.dumps.x = 1`) and the host's
  definitions (typed host functions, host classes and host exception
  classes, with their methods, and the classes made for the host's
  records, which CPython's namedtuple classes do not refuse). It gets CPython's `AttributeError` for a
  builtin function or `TypeError` for an immutable type; a host
  function's `__defaults__`, `__kwdefaults__` and `__dict__` raise
  `AttributeError`, a stdlib function's `__kwdefaults__` and a shared
  class's `__annotations__` are copies, and `sys`'s struct sequences
  refuse attribute writes as CPython's do. Instances, subclasses and
  module attributes stay writable. A reset restores the recursion limit
  and the int-to-str digit limit and builds the builtins module's list,
  dict and record constants again. `BaseException.with_traceback` and
  `add_note` called through the type on an object that is not an
  exception raise CPython's `TypeError`: they wrote its slots, emptying
  a host function's closure or a class's namespace and giving a `sys`
  struct sequence a `__notes__` list, past every refusal [M]. The
  prelude's own methods are held to the same refusals: `self.x = v` in
  one ran as the prelude's write whatever `self` was, so
  `json.JSONEncoder.__init__(json.JSONEncoder, default=f)` made every
  later cell's encoder call `f`, and `json.JSONEncoder.__init__(read,
  default=gen())` or `asyncio.Queue.__init__(type(ROW), 7)` left values
  on a host function or a record class [M]; they raise the `TypeError`
  or `AttributeError` now, which CPython does not for a class. The loop
  `asyncio.set_event_loop` set, with what a cell left on it, was returned
  by `asyncio.get_event_loop()` after a reset [M]; a reset forgets it. An
  exception's `__init__` through the type refuses an object that is not
  an exception (it returned `None`), and a method called with no
  argument at all says `unbound method str.upper() needs an argument`
  as CPython does. A subclass of a class a reset shares
  (`class Sub(Entry): blob = bytearray(30_000_000)`, the same under a
  host exception class, a record class, `asyncio.Lock`,
  `json.JSONEncoder`, `ExceptionGroup`) stayed on its base's subclass
  list with all it held: the next cell had 8,986 pages of 4 KiB at a
  64 MiB limit where a new session has 16,310, and 20,000 empty
  subclasses cost 2,006 pages [M]. A shared class no longer lists its
  subclasses (it never changes, so it has none to invalidate): the
  next cell has a new session's room within 7 pages, and within 33
  after 20,000 or 200,000 subclasses [M]. The classes made for the
  host's records are dropped at a reset: one is kept per record name and
  field list, so a tool that returns records a cell names (20,000
  namedtuples echoed) left the next cell 2,526 pages of 16,307; now its
  60 MB fit, in 14,765 pages at once after the first reset and 15,887
  after the third, where the same cell without the echo leaves 15,996
  [M]. Host call ids and `Task-N` numbers continue, and loaded code
  stays loaded; `reset_to` and a new session share nothing. A host whose
  cells set attributes on a typed host function (`tool.calls = 0`) now
  gets the `AttributeError`: none of the 3,487 recorded Python cells does
  [M]. The test is `conformance/reset`: 100 probes, a sweep that calls every method of the
  builtin types and of the prelude's classes with each shared object as `self`, and a walk that writes
  to each of the 11,000 objects a cell can reach from the builtins, the
  host's definitions and every importable module, and finds no write
  after a reset (181 on the build before) [M], on the native facade and
  through both mm-wasm hosts.
- A cell ended inside a container's `repr` (stopped at a host call or out
  of fuel there, then cancelled, replaced by the next cell or reset) no
  longer leaves the container marked as being rendered: the prelude held
  it for good (40 MB in the test, also across a reset) and every later
  `repr` showed it as `[...]` [M].
- The scratch counter's own 24 bytes are charged from a session's start,
  not from its first scratch reservation, which a restored session had
  not made: a parked session had 24 bytes less room than one restored
  from its image.
- A resumed slice has the room of a run that never parked: the stop a
  park reports, charged while the host holds it, was released only once
  the resumed slice had run, and `cancel`, `reset` and a new cell's start
  kept its charge until the next stop. 8 bytes near the wall: a fill to
  the limit then `print` completed unparked and raised `MemoryError`
  parked; asyncio's start rule queued one entry fewer with 13 units less
  fuel parked, at 22 of 64 limits 8 bytes apart from 6 MiB, and at none
  after [M]. With this and the three retry, `CALL` and wait fixes below,
  near-limit random programs at 2 to 8 MiB, under the default GC or
  incremental marks in slices of 4,096, that differ between a plain run
  and parks every 997 or 20,011 units: 43 of 1,200 before, none after;
  asyncio programs, 58 of 600 before, none after; plain programs on
  wasm32-wasip1, 7 of 400 before, none after [M]. These fixes do not move
  the build id.
  This is plain against parked only: a snapshot at a stop still collects,
  so a session that was snapshotted or restored can still end with
  another outcome or fuel than the plain run near the limit.
- The stop a session keeps for `Session::pending` is charged the same in
  the session that took a snapshot and in one restored from it, by its
  length: the first charged its reused buffer's capacity. The buffer's
  room past the stop is held uncharged, at most 4,096 bytes, as the
  buffer was between stops.
- Host signature code keeps exact buffer capacities, as a restore decodes
  it. Compiled code still keeps the growth slack of its buffers, which a
  restore decodes without: code is charged by capacity, so a restored
  session is charged less for the same code than the one it came from
  (46,820 against 39,056 bytes in a 2 MiB session [M]) until restores are
  made exact.
- Growing the mark stack during an incremental mark reserves the
  capacity it charges: it reserved that many slots beyond the entries on
  the stack, up to 4 bytes per object uncharged, and a restored session
  then had other room than the one that went on.
- A restore no longer moves where a cached call's frame sits. A restore
  empties the call caches, and the call that missed one put the callee's
  frame above all of the caller's registers where a hit runs it in place,
  so each restore in a recursion moved the deeper frames up and the stack
  could grow past a doubling the session that went on never needed: at
  4 MiB beside a 3,800 KiB pad, `f(900)` printed `ok` going on and raised
  `MemoryError` restored at every park of 7 units [M]. The call that fills
  the cache now runs the callee in place too.
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
  list of strs filling the heap) [M]. A `MemoryError` from `reset` now
  means the session was reset with only `__name__`, or a constant of the
  builtins module that did not fit again, unset.
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
- The step bound (docs/spec.md §5) holds on musl on every `Bounded` row
  of the step-time oracle, with musl's `malloc` and with mimalloc, but
  for two rows with mimalloc (Known issues). 15 bounded rows of the
  step-time oracle took 51 to 200 ms between safepoints against a 44 ms
  limit, `t = s[::2]` over two-byte chars 39 to 40 ms against 38.6 to
  44 ms, and the queue test's 50,000 `list.insert(0, i)` 2.7 s against a
  0.91 s budget, all in libc calls musl makes slow: a `memcpy` of
  run-time length per bigint limb (`/`, `%`, `//`, `divmod`, `pow`,
  `math.gcd`/`lcm` on 64 to 67 M-bit ints), per `json.loads` string
  piece and per char of a stepped slice, `memcmp` (a byte loop in musl)
  for `==` on long `str`, `bytes` and ints, and a backward `memmove` for
  `insert(0, x)`. Limbs are now read as fixed-size words and short json
  pieces pushed byte by byte, on every target, which is faster on glibc
  too: a script of 67 M-bit `divmod` and `/` went from 0.36 s to 0.30 s
  and five `json.loads` of 45 MB from 1.26 s to 1.02 s [M]. On musl
  only, equality compares 32 bytes at a time, a slice's chars are copied
  at fixed sizes and short moves up go through a stack buffer; elsewhere
  those stay on the libc path, which is faster there. Values, fuel and
  outcomes are unchanged. On musl the slowest of those rows now takes
  36 ms against a limit of 38.6 to 40.6 ms, and the inserts 0.25 s [M].
  A `bytearray` with spare capacity that opens a gap in place (`insert`,
  a slice assignment that grows) moved its tail with the same backward
  `memmove`: 68 to 72 ms at 64 MiB against a limit of 55 to 56 ms. On
  musl the tail now moves in pieces that copy forward: 49 to 50 ms, as
  on glibc (47 to 48 ms against 50) [M], which is still at the limit
  (Known issues).
- On musl, `json.dumps` of a million-key dict under `sort_keys` and
  `indent` ran steps of 50 to 57 ms, past the bound, with musl's
  `malloc` and with mimalloc: each of an item's short appends to the
  text was a libc `memcpy` call. Appends of 64 bytes or less now copy at
  fixed sizes on musl: 25 to 46 ms against limits of 53 to 58 [M].
  Elsewhere they stay on the libc path.
- `mm-cell-worker` on musl: an abort (`std::process::abort`, a failed
  allocation) killed the worker with the seccomp filter's `SIGSYS`
  instead of `SIGABRT`, as musl's `abort` signals with `tkill` where
  glibc's uses `tgkill`. (A panic in a shipped build is an illegal
  instruction, `SIGILL`, and makes no system call.) On musl the filter
  allows `tkill` aimed at the main thread or the thread that installed it; an
  abort on the reader thread still ends in `SIGSYS` there.
- A retry after `MemoryError` is always charged for its collection, also
  while another collection was asked for and not ended: the charge no
  longer depends on whether a mark or a result's collection was in
  progress, which the GC mode and the host's fuel slices decide.
- A `CALL` that parked before a costly builtin (`sorted`, `sum`, a big-int
  builtin short of fuel) resumes without a safepoint check the unparked
  run does not make. That check could run a due collection first, so the
  call fit where the unparked run failed and retried, and fuel differed
  by a retry's charge.
- A park inside the wait of a retry after `MemoryError` no longer leaves
  that wait set once its collection has ended. Left set, it made the next
  request for a collection a step that found nothing due where the
  unparked run collected in full, so outcomes and fuel moved with the
  host's fuel slices under incremental marking.
- Differences from CPython that were silent, each checked against
  python3.14 by an own case [M]:
  - A generator or coroutine that entered a handler under one resumption
    kept that resumption's caller's exception as the one being handled
    after its own handler ended, and showed a later resumption the
    earlier caller's inside the handler: `sys.exc_info()`, a bare `raise`
    and the `__context__` of what it raised read wrongly (0.0.2's known
    issue, older than 0.0.1). A handler in such a frame now gives back
    what the current resumption's caller is handling, through `yield
    from` and `send()` chains and asyncio tasks, parked and restored or
    not (own case 593; spec §8.5). `SNAPSHOT_EPOCH` moves: a suspended
    frame inside a handler holds something else than before.
  - A task's first step and a loop callback start with the exception
    being handled where `asyncio.run` was called, as the main coroutine
    did; they read `None` (own case 594).
  - `asyncio.run` and `Runner.close()` close the async generators the
    run leaves unfinished, so their `finally` blocks run, after the
    left-over tasks are cancelled. Without finalizers one that became
    garbage is closed at the run's end, not at its collection, and
    nothing is closed for a cell whose top level awaits or a generator
    driven outside a loop. The run keeps only generators with something
    to run when closed (a `try`, `with` or `async with` open at their
    `yield`), closes them 256 at a time, then once more those first
    stepped during the closes, and no later cell's run closes an earlier
    cell's. Each `asyncio.run` costs one fuel unit more, and
    each async generator first stepped inside a run one (own case 591).
  - `asyncio.create_task` outside a running loop raises `RuntimeError: no
    running event loop`, CPython's text, followed by a hint CPython does
    not give (`; await in this cell, or create the task inside
    asyncio.run()`); it was accepted and the task ran in the next
    `asyncio.run`. A session that made a task in one cell without `await`
    and awaited it in a later one gets the error in the first (own case
    592).
  - `gen.throw(cls)`, `coroutine.throw(cls)` and `athrow(cls)` with a
    bare class call it as CPython does and throw in the instance or the
    error its constructor raises; a user class was thrown in without its
    `__init__` run, and a builtin's constructor error was raised at the
    caller (0.0.2's known issue; own case 586). The constructor still
    runs in `throw()`'s caller's frame.
  - `asyncio.Semaphore.release()` that raises `MemoryError` adds no
    permit, so the call can be made again (known issue).
  - A gather, an int or `None` passed to `add_done_callback` is called
    when the future is done and reported as not callable; a gather was
    taken for a child of itself and ended done with `[None, None]`
    (known issue; own case 587).
  - `OSError(2, 'x')` is a `FileNotFoundError`, and errnos 1, 13, 17, 20,
    21 and 110 give their subclasses (own case 588). A dataclass without
    a doc-string gets its signature as `__doc__`, which costs fuel when
    the class is made: 17 units for three fields; a default whose repr
    raises `ValueError` or `TypeError` gives the class name alone, as on
    CPython (own case 589).
  - `L.remove(x)` whose `__eq__` shortens `L` returns as CPython does
    instead of raising `IndexError` (own case 583), and `deque.remove`
    under the same says `deque mutated during remove().` (own case 584).
    `math.trunc` of an object whose class has no `__trunc__` raises
    CPython's `TypeError` (own case 585). A Python descriptor stored as
    a special method (a `functools.partial`, a callable with `__get__`)
    is bound (own case 582). The duplicate-keyword and
    name-and-position `TypeError`s name the method or class called
    (`list.sort()`, not `function`; own cases 580 and 581).
  - A task that awaits itself gets CPython's `RuntimeError: Task cannot
    await on itself: <Task …>` at its next step, where it can catch it; it
    waited on itself for ever, and cancelling it (as the end of
    `asyncio.run` does) raised `RecursionError` out of the run (own case
    596). Not copied: after that, CPython's C task raises `await wasn't
    used with future` at any later `await` of the task while it is
    pending.
  - A done callback, or a `call_soon`, queued in the iteration where
    `asyncio.run`'s main coroutine ended, with no task left to cancel,
    runs: the run goes on the four loop iterations CPython's
    `shutdown_asyncgens()` and `shutdown_default_executor()` make, where
    it stopped at once and dropped the queue (own case 597). A run that
    ends with nothing queued costs the same fuel.
  - A cancelled gather whose last listed child swallowed its cancel
    raises `CancelledError` with the message of the last cancel that
    child had to raise itself, as CPython's does (`gather(t).cancel('m')`
    while `t` is at a bare `sleep(0)` or has a cancelled waiter); the
    message was lost (own case 598).
  - A gather made over children already done settles before `gather()`
    returns, as CPython 3.14's runs their done callbacks there in
    argument order: with the first of them that failed or was cancelled
    (unless `return_exceptions`), or with its list when every child is
    done. A child already cancelled was returned as a result,
    `[CancelledError('')]`, a pending sibling held the failure back
    (the run could end in `every task is waiting`), and the failure
    taken was the earliest to finish, not the first listed (own case
    599).
  - `uncancel()` on a task already done cancelled no longer changes what
    later awaits of it raise: they raised `CancelledError` with the
    error it ended with as its argument (`(CancelledError('m'),)` where
    CPython has `('m',)`), and a task that failed after a cancel lost its
    "never retrieved" report (own case 600).

### Added

- `Session::reset_to(&SessionTemplate)`: reset a session to the state a
  template was captured in, in place of `reset` and a re-run of the setup.
  The session keeps its host-side settings, park handle, limits, fuel
  state and hint overrides; call ids, `Task-N` numbers and the safepoint
  phase continue. It keeps nothing earlier cells loaded, so the next
  cell's output, fuel and memory are those of a new session after the
  setup (`reset`, documented now, keeps loaded bundled code and prelude
  parts). It refuses a template captured under other limits, host
  definitions, function ids or local time zone with the new
  `SnapshotError::Mismatch`, and leaves the session unchanged on every
  error. `Session::outside_reads` counts the session's reads of the wall
  clock and entropy. `mm-cell-worker`
  uses it for py-sdk cells: a `Reset` waits for
  the next `StartCell`, and a bootstrap equal to the one whose state the
  worker kept is not compiled or run again.
- A template from `SessionTemplate::capture` is read without the
  validation walks from its first instantiation or `reset_to`, where
  0.0.2 walked it once: this build wrote its image from a live session,
  and the checksum is still compared each time. `from_image` still
  restores its image checked, as `Engine::restore` always does. A
  two-cell py-sdk session's reset to its bootstrap takes about 1.87 M
  fewer instructions [M].

### Known issues

- A growth or a prune of the intern table is one step, linear in the
  table, and passes the 50 ms bound on a large one [M, shipped
  codegen]. The doubling at 524,288 names (computed attribute names, of
  any bucket), to 2,097,152 slots, takes 56 to 75 ms (54 to 77 before
  this change) and the last one, at 1,048,576 names, 73 to 111 ms; the
  prune of the 4,194,304 slots once 2,000,000 names are dropped takes
  47 to 57 ms (253 ms before). A guest reaches a growth as long with
  fewer names: 100,000 names of any bucket and then 130 crowded on one
  place of every table grow it from 262,144 slots to the largest in one
  step of 54 to 94 ms. A growth sizes the table for the name that
  found no slot, so a guest can force up to one growth per table size:
  128 names whose hashes agree in 22 bits and 13 that agree with them
  in 9 to 21, after 30,000 other names, grow the table five times,
  which is one step of 86 to 120 ms as the literals of one cell and 80
  to 142 ms between two safepoints as a `hasattr` loop. The step
  oracle's four rows for these are `Open`. The fix, growing and
  pruning a slice at a time, is not in this change.
- A session held without `reset()` keeps an intern table at the largest
  size it grew to, also once its names are dropped: about 130 names
  crowded on one place of every table leave 4,194,304 slots, 32 MiB of
  the session's memory limit, a 33.6 MB image, and 3.8 ms more in every
  later collection [M, `--release`]. No collection shrinks it (a guest
  could then grow and shrink it in a loop); `reset()`, `reset_to` and a
  template restore give it back.
- A task with many waiters (done callbacks, gathers that hold it)
  cancelled many times by separate `cancel()` calls pays a read of its
  waiters at each call, which hands the cancel message to the gathers
  whose last child it is: N callbacks cancelled N times take about 3.4
  times as long as before this release at equal fuel (0.39 s against
  0.11 s at 10,000, 3.15 s against 0.92 s at 30,000 [S, the review,
  release builds]). Each call is a short step; within one cancel the
  reads count toward its step's work, so no step is long. Remembering
  which gathers already hold the message would need image state and is
  not built.
- A `SystemExit` or `KeyboardInterrupt` that leaves a task: two
  differences from CPython remain, found by the asyncio review and not
  fixed (owner's ruling). `asyncio.run` still raises the exception in
  both. CPython cancels every pending task directly at the escape as
  well as through the main task, so `cancelling()` reads 2 on a task
  the main task's cancel also reaches, and 1 here. And when the
  exception leaves in the loop iteration where the main coroutine ends,
  with another task pending, CPython 3.14.7 raises `RuntimeError: Event
  loop stopped before Future completed.` instead; here the exception
  itself is raised. Why not: cancelling every task at the escape is one
  native step with no bound. A prototype of it (nine lines, a cancel of
  each other task at the escape) made 36,000 tasks in 40 chains of 900
  one step 487 to 495 ms past the deadline, against 129 ms without it
  [M, release build, `MM_STEP_MS=50`], and the two differences go
  together: the first alone makes the second's counts worse.
- After a task awaited itself, CPython 3.14.7 raises `RuntimeError:
  await wasn't used with future` at any later `await` of that task
  while it is pending; here a second self-await raises `Task cannot
  await on itself` again and another task awaits it normally. The
  task's repr in the message lists no callbacks.
- With an async generator to close and no task left, the end of
  `asyncio.run` makes fewer loop iterations than CPython: a `call_soon`
  chain runs links 0 to 3 here and 0 to 7 on CPython 3.14.7, so a
  callback queued late there is dropped. Not new with the fix for
  callbacks queued as the main coroutine ends (own case 597) [S, the
  review].
- A gather made after its last listed child, a task that cancelled
  itself with a message and caught it, had already returned: when the
  gather is cancelled, CPython 3.14.7 raises `CancelledError` with that
  message (the task's `_cancel_message`, kept after it is done); here
  the message is gone once the task's result is set, and the gather
  raises `CancelledError()`. A gather pending at the cancel gets the
  message (own case 598). Keeping a done task's message is image state
  and is not built [M, rv seeds 1029, 1656 and 1697 reduced; 1020 and
  2068 read as the same, E].
- Differences from CPython older than this release, found by the review
  of the CPython fixes and not fixed [S, that review; the same on
  17a2daf]: `gen.throw(exc)` and `gen.close()` overwrite a `__context__`
  the program set on the exception by hand; `close()` through `yield
  from` passes the inner generator's `GeneratorExit`, and its context,
  outward; a `StopIteration` raised by a resumed generator gets a
  `__context__`; a `call_soon` callback sometimes runs one loop step
  early; `asyncio.gather(coro)` outside a running loop is accepted.
- A run that leaves 800,000 or more async generators unfinished with
  something to run when closed (a `try`, `with` or `async with` open at
  their `yield`) has native steps past the 50 ms bound: the list of
  tracked generators is walked whole in one step, by `track_asyncgen`'s
  prune each time its length reaches a power of two and by
  `take_asyncgens` at the run's end. At 800,000 a step ran 50 to 51 ms
  past its deadline in 2 of 4 runs, at 1,600,000 52 to 67 ms in 4 of 4
  [M, shipped codegen]; 200,000 and 400,000 pass. The step oracle's row
  for 1,600,000 is `Open`. Before this release such generators were not
  tracked, or closed.
- A `bytearray` with spare capacity that opens a gap near its start in
  place (`b.insert(0, x)`, `b[0:0] = ...` on 64 MiB never written) is one
  step of 96 to 97% of the 50 ms bound, with glibc and with musl's
  `malloc`, and over it on glibc in 3 of 15 runs [M]: the step
  first-touches 64 MiB of pages. The step oracle's three rows for it are
  `Open`. The fix, moving the tail a step at a time, is not in this
  release.
- With mimalloc on musl (what `mm` and `mm-cell-worker` register there),
  a row of the step oracle exceeds the bound on some runs and is not
  judged with that allocator; with glibc and with musl's `malloc` it
  is, and passes. A dict of 2^20 colliding keys filled beside a 46 MiB bytearray:
  36.9 to 61.3 ms against 52.5 to 53.2, over in 4 of 8 [M].
- On musl, recorded agent sessions take 54% more engine time in memory
  and 82% more with a snapshot and restore on a fresh `Engine` at every
  host call (snapshot 2.8×, restore 1.7×) than on glibc on the same
  machine: musl's `malloc` maps and unmaps every allocation of 128 KiB or
  more (644 K `mmap` calls against 2.5 K), and its `memcpy` and `memcmp`
  are slow on the small copies the snapshot codecs make. mimalloc as the
  global allocator, which this repository's `mm` and `mm-cell-worker`
  now register there, leaves +14% and +42% [M].
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
- A `UnicodeEncodeError` raised by encoding an instance of a `str`
  subclass holds the plain `str` as its `object`, where CPython's holds
  the instance itself (`e.object is s`).
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
