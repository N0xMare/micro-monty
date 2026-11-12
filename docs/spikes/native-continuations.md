# Spike: native continuation frames

The goal is to make builtins that may reach user code (`sorted` and
`list.sort` with `key=`, `min`/`max` with `key=`, comparisons of containers
that hold user objects, `iter(f, sentinel)`) run their loops in Rust, without
a second dispatch loop on the Rust stack. This page measures where those
builtins spend their time today, prototypes the proposed frame kind on keyed
sorting, compares the options, and gives a step-by-step plan. Base:
`origin/main` at `54ea791`, on this orb. Labels follow AGENTS.md: [M]
measured, [S] sourced, [E] estimated. The prototype and instrumentation were
local and were not committed; "Reproducing" says how to repeat them, and the
appendix keeps the prototype's core.

## Answer

Add one continuation kind, `Cont::Native { op, state }`. Its step function is
Rust and its state lives in one arena object. The dispatch loop drives it
through the machinery it already has (`finish`, `drive`, trampoline frames).
A step returns `Next::Invoke(f, args, kw, Cont::Native { op, state })` to say
"call `f`, then resume me with the result", or `Next::Acc(v)` to finish. The
step never calls `run`, so the rule of one dispatch loop holds, and every
park, GC or snapshot between two steps sees only ordinary heap values.

- **It works, and it pays off where a prelude loop wraps builtin work.** A
  prototype of keyed `sorted`/`list.sort` (keys from user calls, native merge
  sort over the keys) takes `bench/sort.py` from 7,259M to 2,335M
  instructions (3.1×) and from 493–557 ms to 177–238 ms wall (2.3–2.8×;
  CPython takes 83–97 ms) [M]. On 200,000 ints, `sorted(xs, key=f)` falls
  from 7,789M to 862M instructions net of setup (9.0×) [M]. Output and
  tracebacks match CPython on the semantics test.
- **It does not pay off where the time goes into user dunders.** Sorting
  20,000 user objects with `__lt__` costs 3,043M instructions, about 11.7k per
  comparison [M]. A bare `p < q` in a loop costs 9.1k, while an explicit
  `p.__eq__(q)` call costs 1.1k [M]. So about 80% of a user-object sort is the
  implicit dunder path (`Vm::dunder` interns and looks up the name on every
  call), and a native sort alone saves about 20% there [E]. Fixing that path
  is a separate, companion change; with both, a user-object sort is estimated
  at about 0.45G (6.8×) [E].
- **`min`/`max` with `key=` and `iter(f, sentinel)` are not performance
  problems.** `min(xs, key=f)` on 200,000 ints costs 255M net against 217M for
  `[f(x) for x in xs]` [M]. The `iter(f, sentinel)` SystemError is a missing
  handoff (the sentinel `==` needs a user `__eq__`); a small fixed-shape `Cont`
  variant fixes it without the native frame.
- **A prerequisite found on the way.** The native comparison of builtin
  sequences copies each tuple into a `Vec` before comparing: sorting 200,000
  two-tuples costs 3,308M net, 95% in `order_pure`, with `seq_items`,
  `values_of` and malloc/free over half of it [M]. Comparing in place is a
  "do less work" fix worth an estimated 2–3× on tuple keys [E], and
  `sort.py`'s `by_name` sort uses tuple keys.
- **Size is the blocking decision.** The prototype adds 6,195 bytes to the
  shipped `hello.wasm` (1,729,846 → 1,736,041) and 9,504 to native hello
  (1,831,000 → 1,840,504) [M]. Headroom is 154 bytes on Wasm and 5,000 on
  native [M]. The full rollout is estimated at +12 to +20 KB [E]. Either the
  budget moves, or this waits for Rust 1.101's `#[optimize(size)]`
  (−112/−153 KB Wasm per HANDOFF [S], expected around 2026-12-24 [E]).

## Measurements

All instruction counts are callgrind totals for `mm run FILE` on a release
build with symbols (`strip=none`, line tables; the same totals as the stripped
build). "Net" subtracts a setup-only variant of the same file. Wall times are
`taskset -c 3`, best and worst of 5.

### Where `bench/sort.py` goes today

| Function | Share of 7,259M [M] |
|---|---|
| Dispatch loop, self time (running prelude `_sort`'s merge loop and key comprehension) | 34% |
| `op_compare`, inclusive (the `<` the prelude merge executes) | 17% |
| `compare`, inclusive | 16% |
| `order_pure`, inclusive (native ordering of the key values) | 14.5% |

The inclusive rows nest in each other, so they do not add up.

About 60% of the instructions are spent running prelude Python around
comparisons that are themselves native. `sorted` without `key=` already sorts
natively (`sort_values` with `merge_sort`); with `key=` the builtin calls
`vm.need_py()` and `hand_to_twin` runs prelude `_sort`
(`crates/mm-engine/src/prelude.py:1114`) instead.

### Prototype: keyed sort as a native step

`Cont::Native { op: u8, state: Value }` with two ops, `SORT_KEYS` (one key
result arrived) and `SORT_DONE` (the prelude finished a sort whose keys need
Python comparisons). `sorted`/`list.sort` with a callable `key=` build the
state and request the first key call; each step stores the key and requests
the next; the last step merge-sorts an index permutation natively with
`order_pure` over the keys. If a key comparison needs Python
(`OrdErr::Py`), the step hands the keys to a new prelude `_sort_keys(xs, keys,
rev)` (the second half of today's `_sort`) and finishes in `SORT_DONE`.

| Program | Base | Prototype | Ratio |
|---|---|---|---|
| `bench/sort.py`, instructions [M] | 7,259M | 2,335M | 3.1× |
| `bench/sort.py`, wall [M] | 493–557 ms | 177–238 ms | 2.3–2.8× |
| `sorted(xs, key=f)`, 200k ints, net [M] | 7,789M | 862M | 9.0× |
| Reference: `[f(x) for x in xs]`, net [M] | 217M | 217M | — |
| Reference: `sorted(xs)`, net [M] | 334M | 334M | — |

The floor for the keyed microbenchmark is about the two references added,
551M [E]. The prototype's remaining 311M over that floor is mostly
`sort_keyed_finish` copying lists into `Vec`s (`list_items`, the index
permutation as `Value`s) and the per-call cost of returning to the loop
through `drive` → `push_tramps` → `invoke` (a key call costs about 1.5× a
comprehension's call) [M]. Pushing the callee frame directly, without the
`Vec` of arguments and the generic `invoke`, closes most of that [E].

Semantics checked against CPython 3.14 [M] (`/tmp/mmt/s1.py` in
"Reproducing"): stability; `reverse=True` stability; a key that raises (the
traceback shows the key's frame under the `sorted(...)` line, as CPython's
does); `list.sort` with a key that inspects the list (it sees `[]`, as in
CPython) and one that appends to it (`ValueError: list modified during
sort`); a key returning user objects (falls back to `_sort_keys`); keys of
mixed types (`TypeError` naming `<`).

### User objects: the dunder call dominates

`class P` with `__eq__`, `__hash__` and `__lt__`, 20,000 instances, setup
185.6M [M]:

| Operation (20,000 each) | Instructions, net [M] | Per item |
|---|---|---|
| `for p, q in zip(ps, qs): pass` | 37.6M | 1.9k |
| `p.__eq__(q)` in that loop | 22.3M over the loop | 1.1k |
| `p < q` in that loop | 181.9M over the loop | 9.1k |
| `ps == qs` (prelude `_eq` walk) | 145.9M | 7.3k |
| `sorted(rs)`, `rs` shuffled (prelude `_sort`) | 3,043M | 11.7k per comparison (about 261k comparisons) |
| `min(ps)` | 199M | — |
| `for x in iter(lambda: next(it), -1)` | 236M; a plain `f()` loop costs 208M | — |

In a `p < q` callgrind profile, `Vm::dunder` (interning the name, then
`user_lookup`/`own_lookup`) is 43% and `call_class` 28% [M]. The implicit
dunder path costs about 8× an explicit method call. Moving the loop around it
into Rust saves only the loop: about 2.6k of 11.7k per comparison in a sort
[E]. The dunder path is outside this design. The "less work" fix is to
pre-intern the fixed dunder names once per VM and to look the slot up without
building a name per call; it is not designed here.

### Builtin tuples: comparing copies

`sorted(ts)` for 200,000 `(x, -x)` tuples: 3,308M net [M], of which
`order_pure` 3,134M, `seq_items`/`values_of` 1,092M, and malloc/free about
1,000M. Each comparison copies both tuples' items into fresh `Vec`s. That is
about 970 instructions per comparison for two ints [E from 3.4M comparisons].
Comparing in place over the arena slots (an index walk, no copy) is estimated
at 2–3× fewer for this sort [E]. The same copies show in the prototype's
remaining profile.

### Timsort versus merge sort: comparison counts

CPython's timsort compared with a top-down stable merge sort, counting `<`
calls on `sort.py`'s inputs (CPython, `/tmp/mmt/cnt.py`) [M]:

| Sort | Timsort | Merge sort |
|---|---|---|
| `by_value` (random) | 714k | 732k |
| `by_name` (random tuples) | 688k | 695k |
| `by_bucket` (10 distinct keys, presorted runs) | 358k | 705k |
| `flat` (many ties) | 46.8k | 82.7k |

Timsort helps on presorted data and many ties; on random data it saves about
2%. With native comparisons at about 1k instructions each, the merge sort
already in the engine is enough. With Python comparisons at about 10k each,
fewer comparisons matter, and CPython-identical comparison sequences also make
a user `__lt__` with side effects behave as in CPython.

### `iter(f, sentinel)`

`Cont::CallIter` already drives `f` for every consumer (`list`, `sorted`,
`sum`, `min`, `join`, `zip`, unpacking, `for`), and their results match
CPython [M]. The SystemError appears only when the sentinel comparison needs a
user `__eq__`: `finish` calls `eq_values(v, sentinel)`, which calls
`need_py()`, and `CallIter` has no handoff. Repro: an iterator of `P` objects
with sentinel `P(0)` raises "SystemError: micro-monty: this operation needs
Python code here", where CPython returns `[P(1), P(2)]`. A list sentinel such
as `[0]` works.

### Size

`cargo xtask size` (shipped: release, `-Cpanic=immediate-abort`,
`wasm-opt -Oz`) [M]:

| Build | `hello.wasm` | native hello |
|---|---|---|
| Budget | 1,730,000 | 1,836,000 |
| `54ea791` | 1,729,846 (154 left) | 1,831,000 (5,000 left) |
| Prototype | 1,736,041 (+6,195) | 1,840,504 (+9,504) |

Changing `merge_sort` to take `&mut dyn FnMut` (one instance instead of
two) did not change the Wasm size measurably [M].

## Options

| Option | What it saves | Size | Verdict |
|---|---|---|---|
| A. Keep prelude handoffs and make dispatch faster | Dispatch is already the subject of two spikes; a 3–9× gap on keyed sort is not a dispatch constant | 0 | Not enough alone |
| B. Nested `run` from the builtin | All of it | small | Forbidden: two dispatch loops on the Rust stack break pause-anywhere (`AGENTS.md`, spec §8.2) |
| C. Decorate-sort-undecorate in the prelude (`keys` by comprehension, then a native sort of `(key, i)` tuples) | Keyed sort to about keys + tuple sort | prelude only | Rejected: builds n tuples per sort (more work, not less), tuple comparisons copy today, and Python comparisons still need a handoff |
| D. **`Cont::Native { op, state }` on the existing trampoline frames** (recommended) | Keyed sort 9.0× [M], `sort.py` 3.1× [M] | +6.2 KB Wasm prototype [M]; +12 to +20 KB full [E] | Recommended |
| E. A new frame-header kind for native frames (no trampoline) | D's per-call overhead (about 1.5× a comprehension call) | more than D | Later, only if the per-call cost still shows after the direct frame push in D's plan |
| F. Cache dunder lookups per class version | The 8× dunder path | small | Out of scope; first try doing less work (pre-interned names, direct slot lookup) |

D reuses what the loop already has. A trampoline frame holding a `Cont` is
already a frame whose "step function" is Rust: `finish` runs when the callee
returns, and `finish_exc` runs when it raises. The fixed-shape variants
(`ForIter`, `CallIter`, `Truth`, `Cmp`, `NewInit`, ...) are native steps with
at most three `Value`s and four words of state. `Native` adds the one thing
they lack: state of any size, in the arena.

## Recommended design

### The step contract

```rust
// crates/mm-engine/src/nstep.rs
pub(crate) enum Cont { ..., Native { op: u8, state: Value } }

impl Vm {
    /// The callee returned `v`: continue operation `op`.
    fn native_step(&mut self, op: u8, state: Value, v: Value) -> Result<Next, Raised>;
    /// The callee raised: undo what `op` must undo, then keep unwinding.
    fn native_unwind(&mut self, op: u8, state: Value) -> Result<Next, Raised>;
}
```

- A builtin starts an operation by allocating the state and setting
  `vm.request = Some(Request::Call { func, args, kw, cont: Cont::Native {..},
  outer })`, as builtins that call Python already do.
- A step returns one of `Next::Invoke(f, args, kw, Cont::Native { op, state })`
  (call and resume), `Next::Acc(v)` (the operation's value), `Next::Jump`
  (for operations at a bytecode site), or `Err(Raised)`.
- A step does bounded native work between calls. It may do a lot of it (a
  native merge pass over a million keys is one step, as `sort_values` is one
  native call today), but it never runs bytecode itself.
- `native_unwind` returns `Next::Pass` after restoring invariants (for
  `list.sort`, reattaching the items). It never swallows the exception.
- Ops are a closed table: `op` indexes a `const STEPS: [StepInfo; N]` with the
  name and the state layout (slot count, raw word count) of each op.

### State in the arena

One object per in-flight operation, of a new internal type `NSTATE`:

```text
header   type NSTATE, nvals = V (traced), len = V + W
slots    V values: the operation's inputs and buffers (lists it owns)
words    W raw u64 counters: indexes, run stack entries, flags, phase
```

- **GC tracing is free.** The value slots are traced like any object's; raw
  words are never looked at. Buffers that grow (the keys list, the merge
  buffer, a comparison walk's stack) are separate `list` objects referenced
  from value slots, so their size is not fixed by the layout.
- **Every write goes through mm-core's setters** (`set_values`,
  `raw_words_mut`), so the write barrier sees state changes like any other
  mutation.
- **No Rust state lives across steps.** A `Vec` built inside a step (the
  prototype's `list_items`) is dropped before the step returns. This is what
  makes a park between steps, and a restore into another process, sound.
- **Guests cannot reach it.** `NSTATE` objects are referenced only from
  continuations. The type has no Python name and no methods; `type()` of one
  can never be asked. Each step re-validates the slots it reads (types and
  index ranges) and raises `SystemError("micro-monty: bad native
  continuation")` on a mismatch, never panics.

The prototype used a plain `list` for the state; `NSTATE` with raw words
avoids boxing counters as `Value::int` and checking their types on every step.

### Fuel and deadlines

- Callee frames are charged per instruction, as all bytecode is.
- A step charges its native work with `vm.burn(units)`, as `sort_values` does
  today: one unit per comparison or visited element. `burn` already checks
  requests and deadlines after large native work.
- A step that may do unbounded native work splits it into chunks of at most
  `STEP_CHUNK` units (proposed 4,096) and asks for a safepoint between chunks
  with a new `Next::Yield(Cont::Native {..})`. `drive` turns that into a
  trampoline frame that returns immediately, so the loop reaches a safepoint
  (fuel park, preempt, GC) and then resumes the step. This makes a native merge
  sort of ten million keys parkable, which `sort_values` is not today. This is
  new behaviour and is optional for the first ops.

### Snapshots and the build id

- `put_cont` writes tag 26: `[state, NONE, NONE]` and words `[op, 0, 0, 0]`;
  `read_cont` checks `op < STEPS.len()`. Frame validation on restore also
  checks that `state` is an `NSTATE` object whose `nvals` and `len` match
  `STEPS[op]`'s layout. Values that refer into the arena need no further check
  (every arena access is bounds-checked).
- `build_fingerprint` hashes the `STEPS` table (name and layout per op) in
  order, like `NATIVES`. A snapshot taken with an operation in flight then
  restores only into a build whose steps mean the same thing; any other build
  reports `Incompatible`. The snapshot format number goes up once, when tag 26
  ships.
- State is `Value`s and `u64` words, so it is pointer-width independent and
  restores across native and Wasm like the rest of the image (`54ea791`).
- `cargo xtask durability` covers the new paths through own cases that park
  inside a key function, inside a `__lt__` called from a native sort, and
  between chunks.

### Tracebacks and errors

- Trampoline frames are prelude frames: they do not appear in tracebacks and
  do not count toward the recursion limit (`too_deep`). A key function's frame
  appears directly under the user frame that called `sorted`, as in CPython.
  The prototype matches CPython's traceback for an exception raised in a key.
- An error a step raises itself (a `TypeError` for unorderable keys,
  `ValueError: list modified during sort`) is raised through `Err(Raised)` from
  `finish`, at the pc of the call instruction that started the operation, as
  a builtin raising directly would be.
- An exception from the callee runs `native_unwind`, then continues unwinding
  through the frames below; `except` blocks in user code see it unchanged.

### Relation to the existing `Cont` variants

- Keep the 26 fixed-shape variants. They are the cheap case (no allocation)
  and moving them would grow code and change the snapshot format for nothing.
- New operations whose state fits in three `Value`s and four words stay
  fixed-shape. `iter(f, sentinel)` is one: it needs
  `Cont::CallIterEq { it, exit }`, which turns the result of the `==` call
  into "stop" or "yield the held value", with the value kept in the iterator
  object. No `NSTATE` is needed.
- `Native` is for operations with loops and buffers: sorting, comparison
  walks, and later any builtin whose prelude twin is a loop around builtin
  work.

### Which builtins move, in order

1. **Keyed `sorted`/`list.sort`** (the prototype): key calls through the step,
   native merge sort of the keys. Keys that need Python comparisons fall back
   to the prelude `_sort_keys` until step 4.
2. **`iter(f, sentinel)` with a user `__eq__`**: `Cont::CallIterEq`. A
   correctness fix.
3. **In-place builtin sequence comparisons** (`order_pure`/`eq_values` without
   `seq_items` copies). Not a native frame, but it is half of what keyed sorts
   with tuple keys still cost.
4. **Sorting with Python comparisons, as a resumable state machine.** The sort
   state holds the items, keys, merge buffer and run stack (raw words); a
   comparison that needs Python returns `Invoke(<the `<` operation>, [a, b])`
   with a `Cont::Truth` → `Native` chain for the result's truth. Use CPython's
   timsort here (run detection, minrun, galloping) so the comparison sequence
   matches CPython's. Worth doing only together with the dunder-path fix
   (alone about 20% [E]); it also deletes prelude `_sort`/`_sort_keys`.
5. **Comparison walks** (`_eq`/`_op_cmp`/`_ssub` over containers holding user
   objects): an explicit stack of `(a, b, i)` frames in a list in the state.
   Per element today: 7.3k, of which the dunder is most [M]; alone the walk
   saves the 1–2k of prelude loop per element [E].
6. **`min`/`max` with `key=`**: last, if at all. They are within 18% of a
   comprehension already [M].

### Expected savings

| Workload | Today [M] | After 1 | After 1+3 | With the dunder fix |
|---|---|---|---|---|
| `bench/sort.py` | 7,259M | 2,335M [M] | about 1.8G [E] | — |
| `sorted(xs, key=f)`, 200k ints, net | 7,789M | 862M [M] | about 0.65G [E] (direct frame push) | — |
| `sorted(ts)`, 200k tuples, net | 3,308M | 3,308M | about 1.2G [E] | — |
| `sorted(rs)`, 20k user objects, net | 3,043M | 3,043M | 3,043M | about 0.97G without step 4, 0.45G with it [E] |
| `ps == qs`, 20k user objects, net | 146M | 146M | 146M | about 50M with step 5 [E] |

## Implementation plan

Each step lands alone, with `cargo xtask check` green, and records its size
delta in the commit message.

0. **Decide the size question** (see "Answer"). Nothing below fits in 154
   bytes.
1. **Machinery.** `Cont::Native`, `STEPS`, `NSTATE` (mm-engine type table, no
   Python name), `native_step`/`native_unwind` dispatch, tag 26 in
   `put_cont`/`read_cont` with layout validation, `STEPS` in
   `build_fingerprint`, and the next `SNAPSHOT_FORMAT` (shared with the
   delta-snapshot spike if both land together).
   - Checks: snapshot round-trip and corrupt-input tests extended with a bad
     `op` and a wrong-layout state; the snapshot fuzz target's seeds.
2. **Keyed sort.** Port the prototype onto `NSTATE`. Split prelude `_sort` into
   `_sort` and `_sort_keys`. Keep "list modified during sort" and the detach
   of the list for `list.sort`.
   - Checks: own conformance cases for each behaviour listed under
     "Semantics checked" (CPython 3.14 output recorded by
     `cargo xtask conformance`), durability cases that park inside a key,
     `cargo xtask bench --quick` checksums, `cargo xtask gc-stress`.
   - Measure: `sort.py` instructions and wall, the 200k microbenchmark, size.
3. **Direct frame push for step calls.** Let `drive` push the callee frame for
   `Next::Invoke` with a plain function without building an argument `Vec` and
   without the generic `invoke` path. Measure the per-key-call cost against a
   comprehension's.
4. **`Cont::CallIterEq`.** Fix `iter(f, sentinel)` for user `__eq__`
   sentinels. Own case with the repro above.
5. **In-place sequence comparisons** in `order_pure` and `eq_values`. Checks:
   the corpus, `bench`, a microbenchmark on 200k tuples.
6. **Dunder path** (separate design, then) **resumable timsort** with Python
   comparisons and **comparison walks**, in that order, each deleting the
   prelude code it replaces. Durability cases park inside `__lt__` and
   `__eq__` called from these walks.
7. **Optional `Next::Yield` chunking** for long native steps, with a
   durability case that parks in the middle of a large native sort.

## Risks and open questions

- **Size.** The only blocker. The prototype's +6.2 KB is mostly `nstep.rs`
  and the keyed entry paths; prelude deletions in step 6 give back an
  estimated 1–2 KB of packed prelude [E].
- **Comparison-sequence compatibility.** A native merge sort calls `__lt__` in
  a different order and a different number of times than CPython. Step 1 only
  compares builtin keys natively, where that is unobservable; step 6 uses
  timsort for exactly this reason.
- **State validation.** A step trusts nothing it reads from `NSTATE`, because a
  snapshot is trusted input but still fuzzed. The cost is a few checks per
  step, negligible next to a call.
- **Per-call overhead.** If a key call still costs 1.5× a comprehension call
  after step 3, option E (a dedicated frame header) becomes worth measuring.
- **Out of scope, found while measuring.** Dictionary lookups with user keys
  (`__hash__` returning small ints, `__eq__` in Python) grow quadratically:
  20,000 lookups cost 8,135M; 2,500 cost 187M, 5,000 607M, 10,000 2,168M
  [M]. The cause (probing collisions versus the slow path collecting
  candidates) was not verified; it needs its own investigation.

## Reproducing

- **Builds.** Base:
  `CARGO_TARGET_DIR=/tmp/tgt-sym CARGO_PROFILE_RELEASE_STRIP=none CARGO_PROFILE_RELEASE_DEBUG=line-tables-only cargo build --release -p mm-cli --locked`;
  the prototype the same in its worktree. Count with
  `valgrind --tool=callgrind --callgrind-out-file=/dev/null mm run FILE`.
- **Microbenchmarks.** Setup `xs=[(i*7919)%200003 for i in range(200000)]`,
  `f=lambda x: x`; variants append `sorted(xs, key=f)`, `[f(x) for x in xs]`,
  `sorted(xs)`, `min(xs, key=f)`, or `ts=[(x, -x) for x in xs]` and
  `sorted(ts)`. User objects: `class P` with `__init__` storing `v`,
  `__eq__`, `__hash__` returning `v` and `__lt__`; `N = 20000`, `ps`, `qs`;
  variants append the operations in the table above.
- **Wall time.** `taskset -c 3 mm run bench/sort.py` and
  `taskset -c 3 python3.14 bench/sort.py`, 5 runs each.
- **Comparison counts.** On CPython, wrap each key in a class whose `__lt__`
  counts calls, and run the same inputs through `sorted` and through a
  top-down merge sort written in Python.
- **Size.** `cargo xtask size` in each worktree.

## Appendix: the prototype's core

The step, as prototyped (state in a `list`; `NSTATE` replaces it in the plan):

```rust
SORT_KEYS => {
    let xs = self.list_get(state, XS);
    let keys = self.list_get(state, KEYS);
    let n = self.list_len(xs);
    let i = /* state[I] as usize, validated against n */;
    self.heap.set_values(keys_buf, i, &[v])?;      // store key(xs[i])
    if i + 1 < n {
        self.state_set(state, I, i + 1)?;
        let key = self.list_get(state, KEY);
        let x = self.list_get(xs, i + 1);
        return Ok(Next::Invoke(key, vec![x], Value::NONE,
                               Cont::Native { op, state }));
    }
    self.sort_keyed_finish(state)   // native merge sort of an index
                                    // permutation by order_pure(keys);
                                    // OrdErr::Py → Invoke prelude
                                    // `_sort_keys`, resume in SORT_DONE
}
```

`finish` gained `(Cont::Native { op, state }, Val(v) | Stop(v)) =>
self.native_step(op, state, v)?` and `finish_exc` gained
`Cont::Native { op, state } => self.native_unwind(op, state)?`; the rest of
the loop was unchanged.
