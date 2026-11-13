# Handoff: where micro-monty work stands

Updated 2026-11-12. Read `AGENTS.md`, `docs/spec.md` (v4) and `docs/sdk.md` first.

## Current acceptance checklist

The goal is micro-monty proven end to end in its best architecture and
implementation; plak is a later consumer, not the driver. The owner reordered the
plan: speed first (it is the one area that could still need a design change), then
memory/latency accounting, conformance, and final fuzzing and spec v5. The
≤ 1.2× QuickJS bar is a target, not a ship gate; beating Monty on every row and
recording an explained gap is the gate. Conformance integration and interim
fuzzing run in parallel threads. Run targeted checks during development and the
full gate before each phase push. Detailed entries below are historical evidence,
not separate outstanding tasks.

- [x] Standalone VM, own parser, host calls, persistent cells, same-build FULL
  snapshots and Rust/Wasm round trips.
- [x] Retained code, execution/scheduler tables, shapes and collector workspace
  share the arena budget; unreachable code/shapes are reclaimed between cells.
  Reclamation workspace now belongs to admitted records, not temporary tables.
- [x] Native rendering output retains its reservation through consumer allocations
  and arena copies; refusal, recovery and same-budget restoration are tested.
- [ ] Speed: calls/globals fast paths, pair fusion, phase 3 (builtin
  lookup, globals across `del`, generator resume, instance guards) and the
  speed landing (generator fast path, `Ic::New`, str/format fast paths,
  native `datetime` parsers) landed (below); the dispatch function's
  register allocation is the remaining lever.
- [ ] Finish native allocation ownership: numeric conversion pieces, name/error
  helpers and remaining native copies/workspace. The Rust heap is bounded (2.5 × limit + 16 MiB while a cell runs, spec §11, checked by `memory_probes`); no RSS guarantee.
- [ ] Finish large-operation, compilation, GC and checkpoint latency acceptance;
  chunk or bound measured problem operations. Native steps are bounded at about 50 ms on the `Bounded` rows of `tests/steps.rs` (spec §5), which is every row; full collections, snapshots and restores are not.
- [x] Selectively integrate recovered conformance toward ≥540/568 CPython cases:
  540/568 (61/81 host) then; 546/568 (62/81 host) at eb78933 (`builtin__math_funcs.py` fails on the int work cap since 7e25c97). The owner approved a 2,275,000-byte
  native ceiling and the larger per-session baseline (see "Recovered
  conformance integration" below).
- [ ] Complete optional-module and explicit timezone contracts, correctly scoped
  host-side SDK work, fresh comparative performance and local plak reruns.
- [ ] Complete extended fuzzing and spec-v5/final acceptance documentation.

## 0.0.2 list (owner decisions, 2026-11-12)

In this order of priority:

1. **A deterministic wasmtime instruction gate.** Callgrind on the shipped
   `wasm32-wasip1` `mm`, precompiled for x86_64 (`wasmtime compile`), on
   the bench programs, so a flip of Cranelift's register allocation in
   `Vm::execute` is caught by the commit that causes it, either way. The
   wasm thread's bisect scripts are in T-01a1c114's orb at
   `/home/user/workspace/scratch/bis`. Its first target is the layout
   cost the final 0.0.1 batch added (the widened drift exception below:
   wasmtime closure 1.168, fib 1.105, classes 1.097 against 7ac25ef, with
   guest instructions up about 1%).
2. **The wasmtime spill, fixed** with the lean wasm loop below, which hits
   the same spill. 0.0.1 ships it as a known issue: since 4fe04e1,
   Cranelift spills the linear-memory base in `Vm::execute` and reloads it
   before each opcode fetch; guest instructions fib +8.0%, closure +10.2%,
   generators +6.3%, wall 1.055 / 1.087 / 1.042 [M]; V8 and native
   unaffected (sdk.md, "Embedding guarantees").
3. `copy.deepcopy` in steps: no longer needed. Held marks made its steps
   row Bounded at 25–26 ms [M] ("GC: park first", "Superseded by held
   marks" below).
4. Long ranges past 64 bits: a second range representation (B4).
5. NFC/NFD: step a run with no slice boundary in place of the 64 Ki-char
   cap (P5's option 1, 1.5–2.5 days [E]).
6. The dict index layout for 4,194,304 entries: the version word moves
   out of the index (P4's part B).
7. `sum`'s `s >> 63` overflow test calls a user `__rshift__` on what an
   item's `__radd__` returned; a type test only when a result is not a
   plain int fixes it at no cost.
8. The possessive regex `(?:a|b)*+c` is quadratic: 99 s on
   `'ab' * 80000` where CPython takes 15.7 s, fuel-bounded [M].
9. `a % m` and `pow(a, e, m)` with a one- or two-limb `m` as a stepped
   short division (below).
10. An exhausted list iterator keeps its list alive (`x = list(iter(big));
    x = None; big = None` leaves 105 one-MiB blocks of room where
    `list(big)` leaves 255 [M, reviewer]); CPython drops the sequence at
    exhaustion. The fix is a few lines in `iter_next`'s end branch, but 11
    sites read an iterator's sequence slot (the `iter_pos` bulk sources,
    `reversed`, set.rs's in-place reads, restore validation), and each
    needs checking against a `None` sequence.
11. Near the memory limit, an operation not run again after a collection
    can raise `MemoryError` unparked and succeed after a restore at a park
    just before it (sdk.md, "Other differences"): a snapshot collects
    first. Recording the retry mark in the image changes nothing (GC
    thread, measured). The fix (owner, option (a)): full retry coverage,
    an instruction-success hook so that every operation can be retried.
    For the same reason, the fuel total can change: an operation that
    fails partway charges for the work it did, and a snapshot just before
    it frees garbage that counted against the limit, so it gets further.
    The reviewer's `f(*big, **kw)` (guard-callex.py, 256 MiB) with a
    snapshot and restore at every 50,003-unit park costs 4,478,475 units
    against 4,449,803 unparked (+28,672, +0.64%) [M]. The cause [M, GC
    thread, on 1cfcde3]: park 57 comes just before the call, not inside
    the retry wait. Unparked, the mark that started after `t = None` is
    still running at the call, so `t`'s 2.4 MB of garbage counts, and the
    first attempt allocates 10.69 MB of the `**kw` copy before
    `MemoryError`; after the snapshot's collection it allocates 12.78 MB
    and is charged for the extra entries. The rerun after the retry's
    collection is the same in both runs. 4b32aa2 shows no difference
    only because its first attempt fails after 10.77 MB either way [M]
    (its allocator does not reuse the freed block there [E]). Equal fuel
    needs a snapshot that does not collect, or a failing native charged
    for its whole input up front, so the `CALL_EX` shape's fuel stays
    unchecked in `a_restore_inside_a_retry_wait_reruns_the_instruction_once`.
12. The reference wasm host's pooling allocator holds only for a while: at
    64 sessions with 128 slots, the swap cost creeps after about 6 minutes
    (6–12 ms, then 20–38 ms) while host malloc grows from 315 to 468 MiB
    [M, wasm thread]. Find what grows before recommending it beyond
    sdk.md's note.
13. Repetition and in-place multiply left after the GC thread's b2-5: of
    the reviewer's 1,224-line differential against python3.14, 18 lines
    still differ [M]: 14 in TypeError text only, and 4 where `1.5 * True`
    gives a float whose identity differs.
14. Smaller differences from the reviewer's passes [M, reviewer]:
    `list.__iadd__` and `list.__imul__` missing as attributes;
    list `+=`, and `list(x)`, `sorted(x)` and `[*x]` on a list subclass,
    not taking `__len__` as a size hint; `d |= DictSubclass(...)`
    ignoring an overridden `keys()`;
    bytearray missing `__reduce_ex__`; `bytearray_iterator` missing
    `__length_hint__`. From the guest-aborts thread: `bytes()` and
    `math.trunc` of an object with only `__int__` raise mm's "does not
    support user-defined objects" instead of CPython's error, and a float
    subclass's `__index__` is not used for indexing or `range`
    (`'ab'[F(0.5)]` raises in mm). From the reviewer's `%`/`*`
    cross-product: `str()` of a bytes subclass uses its `__repr__`;
    `bytearray % x` is unsupported; `SR('ab') * R()` returns
    `R.__rmul__`'s result where CPython raises; `str` has no `__mod__`,
    `__rmod__`, `__mul__` or `__add__` attribute; `S % ()` and
    `S.format()` on a str subclass return a plain `str`; the identity of `'plain' % b'y'` and `b'' * 3`;
    `object.__format__(5, 'x')` gives `'5'` where CPython raises. Int-subclass
    arguments (reviewer): `l[v] = x` and `del l[v]` reject an int
    subclass; `bytearray.append`, `ba[i] = v`, `v in bytes`, `bytes.find`
    and `bytes.hex(sep, v)` raise TypeError; `math.floor`/`ceil` call
    `__float__`; itertools `repeat`/`combinations`/`permutations`/
    `product`/`batched`, `deque(maxlen=)` and `random.choices(k=)` call
    `__int__`; `itertools.count(v)` keeps the subclass type;
    `splitlines(v)` skips `__bool__`; `slice.indices` and `int.__add__`
    are missing.
15. Dict-view set operations iterate a different operand from CPython's,
    so colliding keys come out in another order (S2, reviewer). `keys &
    set` is fixed for an exact set (S1's commit); a set subclass operand
    and two keys views of unequal length still differ, as does any `x |
    keys`: 78 of the reviewer's 528 cross-product lines differ, 68 with
    `|` and 10 with `&` [M, reviewer]. For example, `list({0, 8, 16} |
    dict.fromkeys([16, 8, 0]).keys())` gives `[0, 16, 8]` where CPython
    gives `[0, 8, 16]`. The causes: ops.rs:560 tests `to == T::set`,
    which a subclass also passes, where CPython uses `PySet_CheckExact`
    (a one-line fix, kept out of 0.0.1); ops.rs:548-563 never iterates
    the shorter of two views; and ops.rs:578-590 for `|`.
16. Fuel after a restore on long non-ASCII strs (owner, option (d) for
    0.0.1, documented in sdk.md and spec §5 guarantee 4): charge the
    char-position walk deterministically, or keep the cache in the image
    (P1, T-01a1c023-2113-7583-913b-3464a167943a, has the measurements).
17. fmt Ir trimming (owner): cumulative fmt is 1.0095 against 4b32aa2
    at the RC [M], mostly P1's `%` work (1.0028 and 1.0030) and T3
    (1.0022); bring it back within ±0.5% of 4b32aa2.
18. Faster gates for batched landings (`check`, `gc-stress`, `wasm-wall`).
19. `list.extend` and list `+=` read their whole iterable before the list
    grows, so a generator that reads the list sees it unchanged: `a +=
    g()`, where `g` yields `len(a)`, gives `[1, 1, 1, 1]` where CPython
    gives `[1, 1, 2, 3]` [M, reviewer; same on 4b32aa2]. Make them
    interleave as CPython does (sdk.md, "Iterators and ranges").
20. `math.hypot` and `math.dist` convert every argument before checking
    any, so `__float__` runs on items after a bad one (N1, reviewer;
    prelude.py:1171-1198, `_mcall`). Convert lazily, as `_pulled` does
    for `fsum`.
21. Old minors from the reviewer's final pass (N2): `int.from_bytes([IX(),
    1])` with an `__index__` item raises where CPython gives 51201;
    `'a\tb'.expandtabs(None)` returns a result where CPython raises
    TypeError (str, bytes and bytearray, also as `tabsize=None`); and
    three exception messages differ in text.

**The 0.0.1 drift check's recorded exception** (owner): `cargo xtask
wasm-wall --base 7ac25ef`, three runs pooled, may exceed the limits on
wasmtime fib, closure and generators, up to the wall ratios above (1.055,
1.087, 1.042). Any other drift failure goes to the owner before the tag.
Widened at the RC (owner): the final batch adds a layout flip, with no
single commit and no real work behind it (the wasm thread's bisect of
828eba9..ca39f934: guest instructions fib and closure 1.010, classes
1.002, no commit past 1.3%, zigzagging; gc-burn's `burn` counter the only
added call-path work, +0.47% on closure; host-tuned precompiles show the
wall cost, generic x86_64 ones do not) [M]. The drift check, three runs
pooled over 33 rounds, gives wasmtime fib 1.105, closure 1.168,
generators 1.021, classes 1.097, geomean 1.039; V8 fib 1.088, closure
1.055, classes 1.052; V8-bundle fib 1.110, closure 1.058 [M]. Against
828eba9 (two runs pooled): V8 closure 1.052, V8-bundle fib 1.065 and
closure 1.054, wasmtime closure 1.064 [M]. Item 1's instruction gate
tracks it; item 2's fix covers the layout too.

## Number formats in steps, division estimate (2026-11-11, P3)

A big int's power-of-two digits, numbers padded to widths of millions
and a lone f-string field are bounded steps (review T3 and T5; docs/spec.md
§5). Big-int division charges its quotient's work (review U1). `div_ns`
took a division by two limbs for 16 ns a quotient limb; it runs about 22,
so `pow(a, 2, 1 << 64)` and `a % m` with a two-limb `m` ran 46-60 ms at
the int cap. The recalibrated estimate refuses them with `MemoryError`
there (owner: accepted (b1) behaviour).

**For 0.0.2:**
- `a % m` and `pow(a, e, m)` with a one- or two-limb `m` are linear in
  `a`: a stepped short division (the remainder and the position as its
  state) would run them at the int cap instead of refusing them, as
  CPython does in a few milliseconds.

## The wasm32 lean dispatch loop: deferred to 0.0.2 (2026-11-10, owner decision)

Part 2 of the wasm build profile is a lean dispatch loop on wasm32 only,
with every slow path behind one call of a one-instruction step instance.
Natively it is cfg-stripped, so native `hello` is byte-identical. It
passed every gate on 7df5282 except `cargo xtask wasm-wall` on wasmtime,
so it moves to 0.0.2 with part 3 (B3 on wasm). The commit, with its
design notes, is local branch `arch/wasm-lean` (35c216e on 7df5282,
thread T-01a1c114). Measured on 7df5282 [M]:

- **Size:** `hello.wasm` 2,114,877 (+16,008), guest 2,201,243 (+16,011).
- **V8, default tiering** (Liftoff on, after one warm run, 11 rounds):
  geomean 0.904.

  | Program | Ratio |
  |---|---|
  | fib | 0.501 |
  | closure | 0.891 |
  | loop | 0.894 |
  | classes | 0.896 |
  | generators | 0.930 |
  | interp | 0.944 |
  | str | 0.956 |
  | fmt | 0.962 |
  | sort | 0.962 |
  | json | 0.998 |
  | exceptions | 1.019 |
  | dict | 1.052 |

  The base's dispatch stays in Liftoff, which is where the gain comes
  from. On ca3db9e, codemode on V8 was neutral (0.996).
- **V8, steady state** (`wasm-wall`, `--no-liftoff`): 0.956 and 0.961.
- **wasmtime** (`wasm-wall`, two 11-round runs): geomean 1.027 and 1.033.
  fib 1.048/1.061, loop 1.042/1.061, fmt 1.070/1.060, interp 1.063/1.043.
  The gate fails, and the interquartile ranges sit above 1.

**Cause** [M]. Cranelift spills the linear-memory base pointer in the lean
function. The measurement: callgrind on `wasmtime run --profile=perfmap`,
with named shipped builds precompiled by
`wasmtime compile --target x86_64-unknown-linux-gnu`, so the code runs
under valgrind. Per-address costs were mapped to the perf map and to
`objdump` of the `.cwasm`.

- On fib, the lean `dispatch::<false>` executes 492.0M instructions
  against 463.7M for the single loop (+6.1%); on loop, +2.7%.
- Linear-memory loads, stores, branches and calls are equal to within a
  few hundred, so the wasm-level work is the same.
- The difference is register allocation. The single loop keeps the memory
  base in `r13` for the whole function. The lean loop reloads it from
  `0x138(%rsp)` before guest memory accesses: 39.1M reloads on fib and
  135M on loop. Spill stores fall, but not by as much.
- Whether regalloc2 gives that whole-function value a callee-saved
  register depends on every live range in the function. The single loop
  wins it today by the same lottery.

**The earlier pass was luck.** The lean function's code and counts are
identical on ca3db9e. Its 1.008 there came from a slower orb: the base's
fib took 44.5 ms then and 36.5 ms now, and the extra reloads, which hit
L1, were hidden. A Rust-side fix would only steer the allocator's
heuristic, and any later package could undo it.

**For 0.0.2:**
- Before landing, gate on executed instructions under wasmtime (the
  callgrind setup above, deterministic), as well as on wall time.
- Look for a lean-loop shape whose hot values cannot lose their registers
  to calls, or accept a measured wasmtime cost for the V8 gain.

## GC: park first; deepcopy deferred to 0.0.2 (2026-11-10, owner decisions)

Park first (spec §9.2) replaces allocate-black, which made worst gaps
longer (3M zip 46 → 53–74 ms) and is off the RC list; the prototype stays
on local branch `gc-stack`. Collect-and-retry and `gc.collect()` in steps
landed before it (ca2e253). The integrator thread T-01a1bcc6 and the GC
thread T-01a1bf28 have the measurements.

**Superseded by held marks (spec §9.2, "Held marks"):** a mark that gives
up, or has too little room left, is now held, a slice per safepoint check
with no guest code between, instead of collecting in full; the deepcopy
row is Bounded at 25–26 ms [M] and the plan below is not needed for it.

**0.0.2: `copy.deepcopy` over a large live heap** (steps.rs Open row, 600k
`[[i, (i,)]]` over 1M old lists: 145–155 ms [M]; a documented known limit
in 0.0.1). Its memo dict is one large table: each insert dirties the
table's segment, every rescan pass rescans the whole table, and the mark
gives up for full collections. The plan, 3–5 days [E], medium risk:

- Retune the give-up rule off the hot path (require halving only above
  16 dirty segments, loosen the 25% rule): deepcopy's worst gap 23.8 ms
  [M]; re-measure the scattered-write loops. A per-large-segment dirty
  range in the setters fixed it too but cost +1% to +19% gated Ir [M].
- Collect-and-retry for container growth (list append/insert/extend,
  `LIST_APPEND`, `SET_ADD`, `MAP_ADD`, dict and set inserts with natively
  hashed keys): with marks that no longer give up, the memo table's
  64 MiB resize raises `MemoryError` at 146 MiB live [M]. Error path
  only; audit that each path mutates nothing before its failing
  allocation.
- Explain the one 118 ms step left with 500k old lists under the retuned
  rule (not yet classified).

## Review fixes 2: flat chains compile at any length, `hash('')` (2026-11-10)

B7 of the independent review of 18b0533, and B8's `hash('')`. Before,
every chain step was a level of the 200-level nesting limit, so 200 `+`
terms, 250 `elif` arms or 250 chained calls raised `RecursionError`, and
the 255-register file capped long f-strings and stacked decorators.
CPython accepts all of them.

- **Chains are one level, walked in loops.** Operator chains, attribute,
  subscript and call chains (`Expr::chain_left`), conditional expressions
  chained in their `else` and `elif` arms: the parser (`if_exp`), the
  symbol table, the compiler (`chain`/`chain_step`, `call_tail`; `If`
  compiles its `elif` arms in a loop), `walk.rs`, `ast.parse`
  (astmod.rs), the AST's `Drop` (`ExprKind::take_chain`, `Stmt`'s `elif`
  chain) and the ruff oracle's lowering (`chain_next`) all loop, so no
  Rust recursion grows with a chain's length. Annotation stringization
  (`unparse.rs`) still recurses and copies an annotation more than 400
  levels deep from the source. The pre-scan's chain estimate moves from
  10,000 to 100,000 (CPython's compiler overflows between 12,000 and
  50,000 `+` terms [M]). Real nesting keeps the 200-level limit.
- **Registers.** F-strings join their fields 64 at a time
  (`FSTRING_GROUP`), and decorators share one call frame (they took 3
  registers each, so 64 decorators failed; now about 250 compile).
  Unpacking into more than 255 targets, nested calls more than 85 deep
  and nested dicts more than 127 deep still fail with "micro-monty does
  not support expressions needing more than 255 registers": widening
  operands from u8 changes the instruction encoding. docs/sdk.md "Code
  size limits" documents all three limits and corrects "200 levels,
  CPython's".
- **`hash('')` and `hash(b'')` are 0**, as on CPython: `mm_core::hash_bytes`
  (and its const twin) returns 0 for empty input (owner rule: no
  compatibility constraints before 0.0.1). The snapshot build id hashes
  `hash_bytes(b"")` among mm-core's encoding samples, so it changes and
  older images are refused; golden images re-blessed. Own case 468 checks
  it, with a user key equal to `''` that hashes to 0.
- Own case 470 (300-term operator chain and string concatenation,
  250-step call, attribute and subscript chains, a 250-arm conditional
  chain and `elif` chain, a 300-field f-string, 200 stacked decorators)
  matches python3.14; on main it raised `RecursionError`.
  `ruff_parity.rs` compares both front ends on chains and nesting on
  either side of the limits. The two front ends place a nesting
  `RecursionError` at different columns (as before this change); the test
  compares the error, not its column.
- On d455c5e [M]: `hello.wasm` +10,326 to 2,098,534, native +11,008 to
  2,292,504, guest +10,316 to 2,184,685, about 5.5 KB of it the AST's
  iterative drop; the ceilings raised to 2,100,500 / 2,294,500 / 2,186,700
  (S1). Gated bench Ir within +0.23% (startup +0.23%, json +0.16%,
  interp +0.06%, dict +0.05%, the rest within ±0.04%). Corpus, real gate
  and codemode results unchanged (no corpus case orders a set holding `''`
  or `b''`).

## Review fixes 1: name mangling, `__getattribute__`, numeric corners (2026-11-10)

From the independent review of 18b0533 (B1, B2, B6, B8; integrator
T-01a1bcc6, owner decision on B2). On ca3db9e.

- **B1, private name mangling** (CPython's `_Py_Mangle`): inside a class,
  `__x` (not ending in `__`, no dot) is `_Class__x`, with the class name's
  leading underscores stripped and nothing mangled under an all-underscore
  class name. Done where CPython does it: every symbol-table name
  (`Scope::slot`/`position` mangle with `Scope::private`, inherited by
  nested functions, lambdas and comprehensions, replaced by each class),
  every name the bytecode names (`Compiler::name`: globals, class
  namespace, attributes, `import`/`from` names, as CPython's
  `IMPORT_NAME`/`IMPORT_FROM` do) and kw-only defaults' keys; call
  keywords and class-pattern keywords are not mangled, and `def`/`class`
  keep their unmangled `__name__`/`__qualname__`. Compile time only; the
  bundled modules' private names (`types`, `difflib`, `collections`,
  `datetime`, `html.parser`) are CPython's own and are read only inside
  their classes. Own case 467.
- **B2, user `__getattribute__`**: rejected at compile time like
  `__slots__` and `__del__` (both front ends; `session.rs`,
  `ruff_parity.rs`); one added later is ignored (docs/sdk.md).
- **B6**: `abs()` and `real`/`imag`/`numerator`/`denominator` of int and
  float subclass instances read the value. **B8**: `type(None)()`,
  `type(...)()` and `type(NotImplemented)()` give the singleton (CPython's
  "takes no arguments" otherwise); `math.pow` no longer continues in the
  builtin `pow`'s prelude twin (a module native never uses the twin of the
  builtin of the same name), and every `math`/`cmath` function of reals
  takes what `__index__` gives (`Vm::real` hands those to `_mcall`, which
  already converted them); `list.__reversed__` and `dict.__reversed__`
  exist, and `reversed()` of a list or dict subclass uses them whatever
  `__getitem__`/`__iter__` it overrides. Own case 468. Recorded in sdk
  instead: `hash('')`/`hash(b'')` (changing `hash_bytes` changes what
  images hold), `math.cbrt`'s last digit (glibc's algorithm is copyleft;
  libm.rs ports none), the Unicode errors' attributes, `Counter`'s double
  hash.
- Corpus, real gate and codemode results unchanged; golden images
  re-blessed (the prelude's bytecode and the native table moved the id).
- Size vs ca3db9e [M]: `hello.wasm` +2,756 to 2,086,391, native +2,608 to
  2,280,072, guest +2,751 to 2,172,547; the ceilings raised to 2,088,400 /
  2,282,100 / 2,174,600 (S1, reason in `xtask/src/main.rs`).
- Gated bench Ir vs ca3db9e [M]: str +0.17%, dict +0.13%, json +0.04%,
  fmt, sort and startup +0.03%, exceptions +0.02%, interp +0.01%, the
  rest 0.00%. `crates/micro-monty/tests/steps.rs`: `ys = xs + xs` is
  `Open` (its 64 MiB allocation sits on the 50 ms limit on clean main;
  the integrator's allocation-floor landing makes it `Bounded` again).

## Engine size review: accessors out of line and `optimize(size)` on shipped builds (2026-11-09, owner decision 69)

The engine size review (artifact "micro-monty engine size review
(2026-11-09)", thread T-01a1c0bd) found one structural cause: validated
arena access (`slot`, `raw`, `hx`, `str_ref`, `set_raw`, `int_of`,
`builtins::arg`, …) is inlined at about 1,500 call sites, 195 KB of
`hello.wasm` code [M]. It also found one constraint: `Vm::dispatch`'s
register allocation depends on everything LLVM inlines into it, so any
change there moves fib, loop and closure Ir by 3–9% in either direction [M].
Owner-approved levers B1 and C1 landed together:

- **B1, accessors out of line outside the dispatch closure.** `vm.rs`
  has out-of-line forms `slot_o`, `vslot_o`, `raw_o`, `set_slot_o`,
  `set_raw_o` and `str_ref_o` (`#[inline(never)]`), plus `hx_o` (inline,
  with an out-of-line error path) and `int_of_o` (small-int fast path
  inline). `builtins.rs` has `arg_o`. Call sites in the cold and
  breadth files use them. The files dispatch inlines from keep the inline
  forms: interp, ops, vm, class, cont, generator, aio, modules, host,
  intops, dunder, dict and json, plus the hot natives `call_native` ..
  `hand_to_twin`, `check_self`, `bi_str`, `list_append`, `strmeth::pred`
  and `format`, `percent`, `text::format`, and `exc::add_tb` and
  `attach_context` (inlined into `Vm::execute`). **New code outside those
  files calls the `_o` forms**, or reads an object's words once
  (`object_words`) and indexes the slice.
- **C1, `#[cfg_attr(mm_shipped, optimize(size))]`** on every function in
  mm-engine's math, cmath, libm, complex, hashlib, re, bytes, binascii,
  random, snapshot, unicodedata, mmmod and modules, and in mm-core's
  snapshot.rs. Not on percent, text, exc, intops or bigint: at size they
  cost fmt up to +8.4% and exceptions or interp Ir past 0.5% [M]. Not on
  json either: json_rows +7% (the 2026-11-03 measurement). Shipped builds
  pass `--cfg mm_shipped` (xtask's `shipped_cargo`, `xtask/size_map.py`,
  `hosts/js/scripts/build-guests.py`,
  `bench/evidence/stdlib_profile/removals.py`). Under that cfg, both lib.rs
  files enable `feature(optimize_attribute)` through the
  `RUSTC_BOOTSTRAP=1` the shipped builds already set for build-std. Normal
  builds, tests and clippy stay on plain stable and never see the
  attribute. Cargo.toml declares `mm_shipped` in `check-cfg`.
- **This revisits the 2026-11-03 decision** ("Size by build structure",
  decision 1) and decision 46 ("no `#![feature]` or nightly-only source
  before 1.101"). The unstable attribute is now used ahead of Rust 1.101,
  on shipped builds only. At 1.101 (about 2026-12-24 [E]), drop the
  `cfg_attr` and the feature line, keep the attribute, and tune per
  function toward the review's C2/C3 sets: another 20–30 KB [E]. Before
  that, keep the fmt-hot helpers (`percent`, `pct_one`, `format`,
  `pad_number`, `small_digits`, `text_needs_py`) and exc.rs out of the set.
- **Gate on shipped native codegen.** `cargo xtask check` ends with
  `shipped_own_cases`: it builds `mm` natively as shipped (build-std,
  immediate-abort, no unwind tables, `--cfg mm_shipped`) and runs the own
  cases against CPython through `crates/mm-cli/tests/conformance_own.rs`,
  which honours `MM=<path>`. It takes about 27 s [M], and `MM=/bin/false`
  fails it. Every other native gate still runs plain-stable builds. Wasm
  shipped codegen is covered by `cargo xtask wasm` (round trips),
  `durability --cross-target` and `wasm-host`.

Landed on c0e4045 [M]: `hello.wasm` 2,213,046 → 2,083,635 (−129,411),
native `hello` 2,418,552 → 2,277,464 (−141,088), guest 2,299,125 →
2,169,796 (−129,329); the ceilings were lowered to 2,085,600 / 2,279,500 /
2,171,800 (S1). On cf4aa86, before the rebase onto P4, the pair measured
−127,611 / −126,768 / −124,977. On f341fd6, the same
pair measured gated Ir within +0.41% on every program (fmt +0.41, interp
+0.20, exceptions +0.15; fib, loop, dict and closure unchanged), wasm wall
time neutral, and codemode engine time neutral over 1,707 runs.

**0.0.2 follow-ups from the review (owner, 2026-11-09):**

- **(a) Trust object tags in the accessors.** Every `get`/`get_raw`/
  `write_slot` keeps the segment and word bounds but drops the start-bitmap,
  header and slot-kind checks (the existing `get_guarded` contract), with
  the failure path out of line. No `unsafe`. Branch `size-review/trust`
  2bf05d9 on f341fd6 [M]: `hello.wasm` −40,872, native −19,712, guest
  −41,538. Wasm wall time 0.85–1.04× (geomean about 0.91). Native Ir:
  json −7.6%, interp −6.3%, exceptions −6.8%, sort −5.7%, startup −9.1%,
  but loop +8.9%, closure +2.5%, fib +1.9%, all from dispatch codegen.
  It changes §7.2's defense in depth: a stale or wrong `Ref` reads or
  writes another arena word instead of getting `UNDEFINED`. That needs a
  spec decision and a fix for the dispatch effect first.
- **Dispatch-closure spike.** Make the set of functions LLVM inlines into
  `Vm::dispatch` explicit and stable, so that changes outside it stop
  moving fib, loop and closure Ir. It unblocks B2: accessors out of line
  per function, every `_o` call outside dispatch's closure, measured −97,101
  `hello.wasm` / −75,952 native / −97,316 guest (`size-review/acc-v4`
  3aef7ca) but loop +4.0%, fib +2.8%, classes +2.2% and dict +1.5% [M].
  It also unblocks (a)'s native side. Pinning 17 dispatch slow paths
  `#[inline(never)]` changed nothing [M], so the spike has to find what
  actually decides dispatch's register allocation.

## The bundle guest (2026-11-09, owner decision 70, size review lever E)

A second shipped mm-wasm guest, `mm_wasm_guest_bundle.wasm`: `mm-wasm-guest`
built with `host-bundle`, for hosts where memory per instance binds (V8 and
Workers copy every data segment into each instance's linear memory). The
default guest is unchanged.

- **Engine.** With `host-bundle` (mm-engine, mm-stdlib, micro-monty,
  mm-wasm-guest), the build scripts link the prelude's Huffman headers and
  the stdlib's shared sections but not the packed chunks or module code:
  `StdModule::external` gives a module's offset and length in region 1, and
  the prelude's chunks come from region 0, both read through
  `ModuleCache::bundle` (default: none). The facade's `set_bundle_reader`
  (only under the feature) answers it, and the guest sets the reader to its
  import. Both hosts refuse at `load` a module that imports `bundle_read`
  without both sections (no fallback, owner rule for 0.0.1); a read that
  still fails raises `SystemError` ("the prelude is corrupt", "a bundled
  module is corrupt"), never a trap.
- **One build id.** The build id hashed the stdlib's code and the prelude's
  chunks byte for byte, so the two guests differed. The build scripts now
  hash them (`mm_stdlib::CODE_HASH`, the prelude's `PRELUDE_HASH`, FNV-1a)
  and the id uses the hashes, the same with or without `host-bundle`. Images
  cross between the guests and the native facade (`wasm-host` parity, the
  JS host's crossing test).
- **Interface.** `mm_host.bundle_read(region, offset, ptr, len) -> u32`
  (`mm_wasm_abi::import::BUNDLE_READ`), imported only by the bundle guest;
  the sections `mm.prelude` and `mm.stdlib` (`mm_wasm_abi::section`).
  `ABI_VERSION` stays 1. A host without the import must run the default
  guest: the bundle guest fails to link there. The wasmtime host reads the
  sections by walking the module file (seeking past the rest) once per
  `load`; the JS host uses `WebAssembly.Module.customSections`.
- **Pipeline and gates.** `cargo xtask size` builds the bundle guest in
  `target/mm-wasm-bundle` (so the default guest stays cached), optimizes
  it, appends the two sections from the build scripts' `prelude_bundle.bin`
  and `stdlib_bundle.bin`, and gates it (`BUNDLE_GUEST_BUDGET_BYTES`, S1).
  `cargo xtask wasm-host` always runs the mm-wasm-host tests against all
  three guests (wasip1, unknown-unknown and the bundle guest), with no
  option to skip one: every landing after E covers the bundle guest.
  `hosts/js`: `npm run guests` writes `bundle.wasm`, `MM_GUEST=bundle npm
  test` runs the scenarios on it.
- **Numbers** [M]: see "Two guests" in docs/research/wasm-embedding.md and
  bench/evidence/host-bundle/README.md. Landed on 8b4b19d: the default
  guest, `hello.wasm` and native `hello` are unchanged within 48 B
  (2,239,038 / 2,152,813 / 2,350,600), with 452,831 B of the guest's data
  segments and 20 initial pages; the bundle guest is 2,237,500 B with
  121,375 B and 15 pages, 331,456 B less data copied into each instance.
  `wasm-wall` against 8b4b19d within the limits: geomean wasmtime 1.008,
  V8 0.996, the bundle guest on V8 0.995 (fib 1.002 on wasmtime).
  Gated Ir unchanged. The build id is the same as without E (the golden
  images did not move).
- **Default builds carry none of it.** `StdModule::external`, its fetch,
  the prelude's fetch and `ModuleCache::bundle` exist only under
  `host-bundle`. A first version kept them as runtime branches in every
  build: +647 B on `hello.wasm`, one more wasm function, and wasmtime fib
  +7.0% wall time in `wasm-wall` (interquartile range 1.064–1.075) at the
  same executed instructions (463.78M vs 463.79M under callgrind on a
  generic-target `.cwasm`): code placement, not work [M]. RSS per idle session in Node 607 → 264 KiB and in
  workerd 570 → 265 KiB (the prototype on cf4aa86).
- **`wasm-wall`** times the head's bundle guest on V8 as a third row,
  `v8-bundle`, against the base's bundle guest (or its default guest when
  the base has none).

## Engine size review: prelude sections and `method_names` (2026-11-09, levers D and F)

Owner-approved with the size review (decision 69 above).

- **D, the prelude in the sectioned form.** Each prelude chunk now
  holds its units' programs as the nine sections the bundled modules use,
  one unit after another, plus a tenth part: the unit records (the
  record's length, the length of each of its program's sections, then
  what its kind needs). Strings that three or more unit programs use are
  stored once (`prelude_strings.txt`, as the stdlib's `strings.txt`). Each
  part is packed against one set of Huffman codes for every chunk. A
  lookup still unpacks one chunk (ten parts). A unit's sections start
  where the units before it in the chunk end, so `locate` sums their
  record lengths and needs no index. `prelude_get_all` copies each unit's
  sections and record out, and charges the copies held when it unpacks
  the next chunk. Prelude data 72,157 → 52,339 B (plus 3,027 B of
  shared strings) [M].
- **F, `method_names` from the method tables.** The "Did you mean"
  candidates for a builtin type are its method table's public names plus
  a short list of data attributes (`real`, `imag`, `start`, `args`, …),
  instead of a second hand-kept list of 168 names. Hints are unchanged on
  56 typo cases over 14 types (main and CPython 3.14 give the same text).
  The review's −10 KB [E] for F counted the whole "dir" section of
  prelude.py (777 lines, with the builtin-subclass helpers). The `dir`
  lists proper (`_DIR_EXTRA`) are 1,104 B packed and stay in the prelude:
  moved to Rust as text they would be larger.

Measured on cf4aa86 [M] (`hello.wasm` / native `hello` / guest): 2,152,625
/ 2,352,568 / 2,238,432 before, 2,136,153 / 2,332,808 / 2,222,009 with D
and F (−16,472 / −19,760 / −16,423). Gated Ir: every program within +0.03%
(json +0.03%, sort −0.27%). On wasm, D takes 16,827 B out of the data
segments: one 64 KiB page of initial memory less per instance. F is
−0.8 KB on wasm and −6.3 KB native.

- **Decoder workspace.** A chunk's ten parts are unpacked one at a time,
  each with its own decoders, and every part but the last gives its
  decoder workspace back (`mm_compile::pack::workspace`). With all ten
  decoders alive, a session's reserved workspace grew by about 57 KB and
  `session::sorted_json_entries_share_the_memory_budget` failed at its
  4,208 KiB limit. A chunk now peaks at its parts plus one workspace, as
  before.
- `Rd::uleb` is out of line (−2.5 KB of code on wasm, no Ir change).
- **The snapshot build id.** Sectioning changes the packed prelude bytes
  and adds the shared strings (`TEXT`, `ENDS`) outside them; both go into
  `PRELUDE_HASH` (`fnv1a(&[&blob, text, ends])` in mm-engine's build
  script), so the id moves by itself and `SNAPSHOT_EPOCH` stays. An image
  holds the prelude code a session loaded encoded in full (`CODE_ENCODED`;
  only stdlib code is stored by module and offset), so it names no chunk,
  chunk order or section offset.
- **Landed on 7ac25ef** with review item Q1 [M]: `hello.wasm` 2,168,786 →
  2,150,004 (−18,782), native `hello` 2,367,032 → 2,346,552 (−20,480),
  guest 2,254,974 → 2,236,230 (−18,744); D and F −17,386 / −20,304 /
  −17,348 of it, Q1 −1,396 / −176 / −1,396. Ceilings 2,152,000 /
  2,348,600 / 2,238,200 (S1). `method_names` now covers `re.Match` (its
  methods from the table plus its data attributes), which main listed
  without its methods.
- **Q1, mm-core's unused dirty bit.** `Heap::take_dirty`, `is_dirty`,
  `clear_dirty` and `SpaceId` had test callers only: delta snapshots were
  never adopted (spec §9.3). `Seg::dirty` keeps only `DIRTY_REFS`, which
  value setters set and the incremental mark rescans; about 20 stores of
  the other bit went from setters, allocation, the sweep and str hashing.

## Release hygiene for 0.0.1 (2026-11-09)

Owner request (via T-01a1a815): make the repository release-clean for
0.0.1 without bumping versions, tagging or publishing. Measured [M] on
origin/main d2fa244 in a 16-vCPU orb.

- **Supply chain.** `deny.toml` (cargo-deny 0.20.2) serves the root
  workspace, `fuzz/` and `crates/mm-wasm-host` (commands in AGENTS.md).
  With `--all-features` all three pass advisories (RustSec database of
  2026-11-08: no advisory, no yanked crate), licenses, bans and sources.
  The published crates depend on nothing outside the repository; the
  third-party licenses come from the CLI, the worker, ruff-oracle, the fuzz
  harness and wasmtime: MIT, Apache-2.0 (and WITH LLVM-exception), plus
  `Unicode-DFS-2016` (ruff-oracle's unicode_names2), `NCSA` (libfuzzer-sys),
  `BSD-3-Clause` (wasmtime's zstd; ittapi is `GPL-2.0-only OR
  BSD-3-Clause`) and `Zlib` (foldhash) as AND parts. Duplicate versions
  warn: syn 2 and 3 in the root workspace (tokio-macros), getrandom and syn
  in fuzz/, eight crates in wasmtime's tree. The one git source is
  plakous-cell, pinned by rev for `mm-cell-worker` only.
  **Not in `cargo xtask check` (proposal):** the advisories check fetches
  the RustSec database, so it needs the network and can fail on a day no
  commit changed anything; run it before tagging and in `ci.yml` on tags
  instead (owner decision; `.agents/setup` would then install
  cargo-deny 0.20.2). **Decided (owner, 2026-11-09): as proposed.** The
  `ci.yml` gates job (`v*` tags, manual dispatch) runs the three checks,
  `.agents/setup` installs cargo-deny 0.20.2, and AGENTS.md lists them
  among the steps before tagging. [Superseded 2026-11-12: `ci.yml` was deleted in `1cfcde3`; GitHub runs no gates, and the three checks run locally before a tag (AGENTS.md).]
- **Unused dependencies.** cargo-machete 0.9.2 and rustc's
  `unused_crate_dependencies` lint found one: `mm-core` in mm-stdlib, now
  removed. False positives: mm-stdlib's build dependencies (machete), the
  `mm-cell-worker` lib and binary (libc is the binary's, serde_json the
  lib's), `mm-wasm-alloc` in the guest (used on `wasm32-unknown-unknown`
  only). Every `mod-*` feature is read by a build script or forwarded.
- **Docs.** `cargo xtask docs` already runs docs/sdk.md's Rust blocks as
  the facade's doctests (10 run, 3 `ignore`); README has no Rust. It now
  also checks that every `mod-*` named in the guide's toml snippets is a
  micro-monty feature. Stale statements fixed in docs/sdk.md (fuzz target
  count, a renamed test, mm-wasm v1 → v2, mm's four opt-in modules,
  `exec`/`eval` that do not exist, `except*` in the Monty table, the
  constructors, what a restore resets, sizes dated) and in AGENTS.md
  (mm-wasm v2).
- **Publish metadata.** keywords and categories on the five internal
  crates; description, repository, license, readme and rust-version were
  already set.
- **Big-endian.** Every snapshot integer is written and read with
  `to_le_bytes`/`from_le_bytes`; no `to_ne_bytes`, `transmute` or
  `target_endian` in the engine. Under Miri on `s390x-unknown-linux-gnu`
  (nightly-2026-11-01) a fresh session's image (41,081 bytes) and one after a cell of ints,
  bigints, floats, non-ASCII str, bytes, dict, tuple and set (42,300
  bytes) are byte-identical to x86_64's, and each target restores the
  other's image and runs a cell on it (5 to 7 min under Miri; scratch
  example, not kept).
- **THIRD_PARTY.md** now also lists `fromisoformat`
  (`Modules/_datetimemodule.c`), `Python/suggestions.c`, the prelude's
  `unicode_format.h`, `genericaliasobject.c`, `unionobject.c` and asyncio,
  and the Python modules written after CPython's (`time`, `codecs`, ...).
- **Hygiene.** `.gitignore` ignores every `target/` (mm-wasm-host's, the
  codemode runner's and the bench hosts' were not). The engine crates'
  Rust has no TODO/FIXME/XXX; the nine in `crates/mm-stdlib/python/` are
  CPython's own comments in the adapted modules. Tracked `.log` files sit
  under `bench/evidence/` as evidence.
- **semver at 0.1.** At 0.0.x every release is breaking. From 0.1, before
  each release: build the last published `micro-monty` as the baseline and
  run `cargo semver-checks check-release -p micro-monty --baseline-version
  <last>` (cargo-semver-checks pinned in `.agents/setup` then); a reported
  break needs the minor bump (0.1 → 0.2) or is reverted. The five internal
  crates have no semver promise (their READMEs say so) and move with the
  facade by exact pins, so check only the facade, and the frozen mm-wasm
  ABI by `cargo xtask wasm-host`. A snapshot format or build-id change is
  not a semver matter: images restore only on their own build (U1).
- **Load-sensitive gates** (follow-up, owner request). `cargo xtask check`
  failed on untouched origin/main in a 16-vCPU orb: two debug wall-clock
  limits in `tests/session.rs` (`a_caught_name_error_costs_no_hint_until_it_is_rendered`
  < 3 s, measured 4.6–7.3 s; `a_cut_completion_repr_stops_walking_past_what_the_cut_keeps`
  < 5 s, 10.2–12.7 s) with the binary's 61 tests in parallel. Both now
  assert fuel instead. The hint test's class has 200 more attributes, so
  a hint computed when raised costs the caught errors 11.7M (names), 14.5M
  (class attributes) or 0.59M (module attributes) units against the
  cell's 51K; each eager variant fails it (scratch reverts in mm-engine,
  not kept), as the old 200,000 bound alone did not for attributes. The
  completion walk charges about 2.3 units per byte, so the cut test
  bounds fuel at three times the walk; reverting a20e801's bound gives
  its MemoryError, and a 1 MiB → 8 MiB `HEAD_TAIL_WALK` charges 19.2M and
  fails, which the time limit was the only check for. Also on fuel now:
  `a_long_container_repr_inside_a_cell_parks_on_fuel` (4,384 parks,
  bounded at 6,000; it took 43 s of its 60 s alone under 3× CPU
  oversubscription) and `a_tuple_hash_over_shared_nesting_finishes`
  (4.8M units). Left on the wall clock: deadline and watchdog tests
  (time is the contract, bounds 40× the 50 ms deadlines) and cost proxies
  with at least 5× margin whose regressions were uncharged work
  (`vm_bytecode_slow_seeds_finish_promptly`, huge `list(range)`, the
  report render, `doubling_a_repr…`, cyclic comparisons, the worker's
  bit-count render). All passed under 32 busy loops on 16 cores.
- **Conformance timeouts on CPU time.** `builtin__math_funcs.py` timed
  out at 10 s under load (correctness-triage orb). It is one operation:
  `pow(3, 2**33 - 1, 2**131072 ± 1)` twice, 0.75 s and 0.71 s on mm
  against about 0.5 s each on CPython 3.14 (1.46 s for the case, the rest
  under 0.05 s), not the harness. With a 2 s limit and 32 busy loops it
  timed out after 1.0 s of CPU. `conformance/oracle.py` now gives each mm
  process 10 s of its own CPU time (`RLIMIT_CPU`, SIGXCPU reported as
  `timeout`), with a wall-clock backstop six times as long
  (`MM_WALL_FACTOR`); xtask's conformance watchdog allows for it. The
  same case passes loaded; an endless loop and a 1 s limit still report
  `timeout`. Durability's processes get the same limit.
- **Docs vs code, stable findings** (owner request; audit artifact
  992ff5d9 at 7dfa304). spec §9.3: header format 6, and restore checks
  every reference's kind (`check_refs`), dict indexes and the decoded
  fields the refusal tests cover. spec §10.1 sketch: `define` returns
  `DefineError`; restore errors include `Internal`. `Engine::restore`'s
  rustdoc said frames, bundled-module unpacking and verification scratch
  were not charged; `Vm::read_snapshot` admits them, so the rustdoc and a
  new spec §11 "Restore admission" paragraph list what is charged and what
  is not (the image bytes, the pending stop's host values, the Engine's
  module cache, allocator transients). `fuel_wanted`'s rustdoc lists every
  site that estimates first (the int operators and, called directly,
  `pow`, `divmod`, `sum`/`math.prod`, the `math` big-int functions and
  `sorted`/`list.sort`). The guide's examples now run (they were bodies of
  uncalled functions) and assert their stated results: the first cell's
  output and value, `HostHandler`, the template's clean globals, the base
  image's identity and build id, a snapshot of a parked cell resuming.
  The in-flux findings (bounded steps, overdraft, returns as safepoints,
  repr parking, the memory bound, GC rules, caps, size and count tables)
  wait for the release candidate.
- **gc-stress takes about 85 minutes**, not 6 (xtask usage fixed): 5,032 s
  at 8873a0b on the x86 orb, 3,543 s at 874409b on the M1 Max (above and
  bench/evidence/isolation/results/2026-11-05-gc-stress-aarch64), not
  rerun.
- **Public surface without plak** (owner request, 2026-11-11, via
  T-01a1a815). micro-monty reads as a Python engine for code mode; plak is
  the local agent harness it is tested against. README, CHANGELOG,
  AGENTS.md's opening, docs/sdk.md, docs/spec.md (the cell protocol is
  §10.2's embedding example) and the published crates' rustdoc and
  comments describe consumers generically: lines naming plak in those
  files went from 110 to 18 on eb78933, the rest being the protocol and
  dependency names (`plakous-cell-protocol.v2`, `plakous-cell`), the
  worker's default script name and spec §18's reference.
  `crates/mm-cell-worker` and `conformance/codemode/real` are framed as
  local integration testing; docs/plak-integration.md moved to
  docs/research/ and is marked internal. Wire identifiers, recorded traces,
  docs/research, bench/evidence and this file keep their plak references.
- **Guarantees by target, known differences.** docs/sdk.md's "Embedding
  guarantees" opens with a table per target (native, wasmtime, Node,
  Workers): longest step, fuel overdraft, deadlines (they never fire on
  deployed Workers), memory and time per fuel unit (workers-v8.md §8).
  "Limitations" adds the iterators the folds and `set`/`dict`/`bytes` read
  to the end before an element fails, `range` past 64 bits and
  `__missing__` through `__getitem__` (independent review B3, B4, B8; B1,
  B2, B5–B7 and most of B8 were fixed on main), and a new "Other
  differences from CPython" list: every divergence this file and
  docs/research record that still reproduces against CPython 3.14.7
  (about 30, probed one by one; 13 recorded ones now match and are left
  out). The numbers are those of eb78933 and are remeasured on the release
  candidate.
- **bench/evidence/isolation builds again.** Six type errors from API
  changes before the freeze (`set_text_limit` takes `u64`, `DefineError`
  has no message, `HostValue` is non-exhaustive, `RuntimeConfig`'s sizes
  and `memory_bytes` are `u64`) and a stale lockfile entry; the native and
  child `perf` sections run.
- **`cargo fuzz build -s none` builds every target.** It failed to link
  `memory_probes`: cargo-fuzz instruments every bin of the fuzz package
  with SanitizerCoverage, whose callbacks only libFuzzer (linked by the
  targets) or a sanitizer runtime defines, and the replay has neither.
  The replay now needs the fuzz package's `memory-probes` feature
  (`required-features`), so cargo-fuzz skips it; `cargo xtask check`,
  AGENTS.md and `ci.yml` pass the feature. [Since 2026-11-12 there is no `ci.yml`: the fuzz build runs locally before a tag (AGENTS.md).]

## Non-ASCII str indexing (2026-11-08)

Owner request (via T-01a1a815): `s[i]` on a non-ASCII str was O(i) (the
C1 fuzz fix, 46c2d90, only charged its walk), so `for i in range(len(s)):
s[i]` over text with one accented letter was quadratic. The heap now keeps
up to 8 recent (str, char, byte) positions (`StrPos`, mm-core
`string.rs`); `Heap::str_offset`/`str_index` walk eight bytes per step
from the nearest of the string's ends and those, and remember the result
(a short walk moves the position it started from, a longer one takes a
new or the oldest slot). Collection clears them before sweep can reuse an
address; they are not in snapshots and add nothing per string (spec §7.2,
"Char positions"). Indexing, slices (with steps), the str method windows,
`re` pos/endpos and json `raw_decode` use them; fuel stays one unit per
64 bytes walked. `str.find`/`rfind`/`index`/`rindex` with a start match
patterns of up to 8 bytes as bytes instead of checking the window's UTF-8
on every call. The sparse per-string table was not needed: random access
into a long string is the one pattern the cache does not cover, and the
recorded plak cells do not do it.

- [M] 1M chars: sequential 0.122 s, reverse 0.116 s (CPython 0.123 /
  0.080; before about 122 s); random 6.6 s (CPython 0.24 s; before about
  125 s). The C1 seed 2.99 s (before 55.0 s, CPython 0.94 s). A 1 MB
  lexer loop (`find('\n', i)`, `startswith(x, i)`) 0.01 s (before 53.7 s,
  even on ASCII).
- Tests: own case 419 (matches python3.14, also under GC stress), the
  parking.rs fuel test (non-ASCII within 1.5× of ASCII; 1.07× measured,
  about 2,500× without the cache), mm-core walks against `char_indices`
  and a reused address.
- Size [M] vs 3720bf2: `hello.wasm` +838, native +736, guest +838, under
  the ceilings.
- Bench Ir vs 3720bf2 [M]: json +0.255%, interp −0.048%, every other
  program within ±0.01% except startup.
- **Bench-rule exception (owner decision, 2026-11-08):** startup +0.508%,
  over the ±0.5% rule. All of the excess (+4.4k Ir) is glibc's pre-main
  `/proc/self/maps` parsing (`__vfscanf_internal`, `strtoul`), which moves
  with the binary's layout; mm's own startup code changed by +5 Ir. The
  owner accepted it.

## `__import__` for 0.0.1, and two 0.0.2 follow-ups (2026-11-09, owner decisions)

**`__import__` lands in 0.0.1** (it was the most-hit gap in the corpora
that was not a missing module: 3 real plak cells, 3 census solutions;
correctness triage, 2026-11-09). CPython's
`__import__(name, globals=None, locals=None, fromlist=(), level=0)` is a
native over the `import` statement's machinery (`Vm::import`): a bundled
module's body runs as a call request, never a nested run, so it parks
like an `import`. A dotted name gives the top package unless `fromlist`
is non-empty; then the prelude's `_import_fromlist` (CPython's
`_handle_fromlist`) imports each missing `fromlist` name of a package as
a submodule and ignores missing names that are not submodules. Errors
are python3.14's: `module name must be a string`, `Empty module name`,
`level must be >= 0`, a relative import (`level > 0`: the `__name__`
`KeyError` without globals, else "no known parent package"), "Item in
``from list'' must be str", arity and keywords
(`conformance/own/425_dunder_import.py`). Moved [M]: the real gate's
`phase2/grok-sdk-wasm/fuel-loops/10` now completes (recorded with its
`why`); the other two (`dict-review/27` of phase1 grok-sdk-mm and phase2
grok-sdk-wasm) never reach their `__import__` call and replay as
recorded, before and after. Codemode: `a/dt-local-times`, `luna-a/logs-busiest-user` and
`luna-b/page-accounts-offset` pass with CPython's calls; mm-default
1,698 → 1,701 of 1,722.

- `__import__` is a `_prelude` native in `prelude::AS_BUILTINS`, bound in
  the builtins on its first lookup like `map`, and `_import_fromlist`
  imports submodules through `__import__` itself. As an ordinary builtin
  plus two prelude helper natives it cost startup +1.19% gated (+6.7K Ir:
  each native is interned and bound at `Vm::new`, and each builtin also
  scans `types::ALL` in `builtins::install`) [M].
- Bench Ir vs cf4aa86 [M] (gated; equal-length paths): startup +0.27%,
  str +0.01%, fmt and sort −0.01%, every other program within ±0.005%;
  whole-process startup +0.42%.
- Size vs 8cd850c [M]: `hello.wasm` +2,048 to 2,154,576, native +2,128 to
  2,354,552 (both ceilings raised to the measurement plus about 2 KB, S1),
  guest +2,034 to 2,236,374 (under its ceiling).

**Deferred to 0.0.2** (owner decision), with estimates [E]:

1. **Annotations as CPython 3.14 keeps them** (PEP 649/749). Today an
   annotation is its source text (`C.__annotations__ == {'x': 'int'}`,
   a string annotation keeps its quotes, functions have no
   `__annotations__`, `dataclasses.Field.type` is `'int'`,
   `get_type_hints` returns strings). Work: the compiler emits an
   `__annotate__(format)` code object per function, class body and
   module that builds the dict (evaluated lazily, so forward references
   keep working); `__annotations__` calls it once and caches; a slim
   `annotationlib` (`get_annotations`, `Format.VALUE`/`STRING`/
   `FORWARDREF` as far as no `eval` allows); `typing.get_type_hints` and
   `ForwardRef` evaluate string annotations with a small evaluator for
   the expressions annotations use (names, attributes, subscripts, `|`,
   `None`, literals, tuples), since there is no `eval`; `dataclasses`
   and `singledispatch` read the real types. Cost: 2–3 days; about
   +6–10 KB of `hello.wasm` (compiler and prelude), no bench Ir on call
   paths (thunks run only when read), a little compile time per
   annotated `def`. Gains: 6 corpus cases (`typing__get_type_hints*`),
   about 70 Lib/test methods (test_typing, test_dataclasses, test_functools
   `singledispatch`, test_grammar). Corpus use: 0 real cells, 0
   solutions read annotations [M].
2. **`locals()`.** PEP 667 semantics make it cheap: at module and class
   level it is the namespace (module level: the read-only `globals()`
   view of decision G2, or a writable class namespace); in a function it
   is a fresh snapshot dict of the bound fast locals and cells, and
   writes have no effect, as in CPython 3.13+. Work: a builtin that reads
   the calling frame's code names and slots (as `_mm.caller` reads the
   frame chain), plus `vars()` without arguments. Cost: about 1 day;
   +1–2 KB `hello.wasm`; no Ir outside the call. Gains: 38 Lib/test
   methods (test_patma 20, test_typing 12, test_scope 3); 0 real cells
   and 0 solutions [M].

## Pre-0.1 follow-ups (2026-11-08, owner-approved)

Four small items, one commit each, on the 0.0.1 candidate (e96ca7c).
Measured [M] against e96ca7c on the 16-vCPU orb.

1. **A cached restore checks the image's host-code definition** (fuzz
   orb finding, conformance/breadth/fuzz/README.md "Open"). Each
   `HostProgram` keeps `definition`, a hash (XXH64) of the module name,
   its spec as a snapshot encodes it and the recipe; a restore takes the
   cached program for the image's key only when the image's definition
   hashes the same (the engine hashes the spec bytes it reads, so no
   re-encoding). Otherwise it does what an uncached Engine does: generate
   the code from the image's definition, take it from the cache by the
   generated key or compile it, and refuse the image
   (`SnapshotError::Corrupt("host code")`) unless that key is the
   image's. **Decision: a hash mismatch alone does not refuse.**
   Definitions that differ only where the generated source does not
   (docstrings, untyped functions) share a key and so a cached program:
   refusing on the hash would refuse a session's valid image whenever
   another session of the Engine defined the module first with another
   docstring. Such an image costs a generation (no compile) on restore.
   Cost: a warm restore of 100 typed tools +62K Ir (4.17M → 4.24M,
   +1.5%), median 339 → 347 µs, min 314 → 319 µs; 10 tools +6K Ir
   (+0.7%), no wall change measurable (bench/evidence/host_defs, `hd
   restore`, `taskset`). A cold `define` (cache miss) hashes the encoded
   spec too: 100 tools +0.64M Ir on 61.7M (+1.0%); a warm `define` is
   unchanged (+12K Ir on 14.4M: the spec is still encoded once, and only
   on a miss before the code). `tests/host_defs.rs` restores an image
   whose definition renames a parameter under the old key on the warm,
   a cold and an uncached Engine (all refuse; the warm one restored it
   before), and an image whose definition differs only in a docstring on
   the warm Engine (restores, nothing compiled). The `snapshot_restore`
   fuzz target's mode bit 5 restores each input on both engines and
   asserts the same host-code verdict; seed `2100949f2000` (the renamed
   parameter) is replayed by `fuzzed_images_restore_to_errors_not_panics`
   and fails without the fix. API (engine crates, no semver):
   `HostProgram::definition` and `HostProgram::definition_hash`; the
   restore looks in the module cache itself, so the `HostCode` callback is
   only asked to build the code again. Size: `hello.wasm` −467 B, native
   +1,216, guest +212, under the ceilings.

2. **Lines end at a lone `\r`** (fuzz README "Open"). CPython
   translates `\r\n` and lone `\r` to `\n` before tokenizing, so a lone
   `\r` ends a line wherever the tokenizer sees one; mm's lexer already
   broke lines there, but the line table (`mm-parse` `lines.rs`) counted
   only `\n`, so every position after one was on the wrong line. Now the
   line table, the pre-scan (which also now ends comments and strings at
   a `\r`, as the tokenizer does), the parser's caret anchors, the ruff
   oracle's line table and the "detected at line" of an unterminated
   string at EOF count `\n`, `\r\n` and lone `\r` as one break each; and
   the source lines that SyntaxError rendering, `SyntaxWarning` and
   tracebacks show are split the same way (`mm_compile::source_lines`).
   Tests: 18 rows in `mm-parse` `syntax_errors_match_cpython` (code,
   comments, strings, continuations, indentation errors, unterminated
   strings, f-strings, mixed endings), the pre-scan limits and AST
   positions of a mixed source (`lone_carriage_returns_end_lines_as_in_cpython`),
   a mixed snippet in parse-diff (fails with the oracle's old line
   table), and the facade's rendered SyntaxError and traceback
   (`tests/session.rs`). parse-diff is clean. **String values too**
   (owner, 2026-11-08): a line break inside a triple-quoted string reads
   as `\n` (`'''a\r\nb\rc'''` is `'a\nb\nc'`), raw, bytes, f-string text,
   format specs and `{x=}` debug text included, while an escaped `\r`
   stays; escape-error positions count the translated body. CPython
   3.14.7 does this alike for a file, `compile` of a `str` and of
   `bytes`, and `ast.parse`; the facade's source string is the `str`
   case (mm has no `exec`/`compile`). The decoder (`strings.rs`) does it
   for the own parser; the ruff oracle, whose values keep the breaks,
   translates its source before ruff reads it (lines and columns do not
   move). Tests: `line_breaks_in_strings_read_as_newlines_as_in_cpython`
   (11 values from `ast.parse`) and 3 escape-error rows; mm's `ast.parse`
   and a CRLF file under `mm run` print what CPython prints. The
   `tokenize` module now reports a lone `\r`'s next token on the next
   row; CPython's readline-based `tokenize` does not split lines there
   at all (it yields `OP '\ry'`), so it matched neither before nor now.
   Size: `hello.wasm` +429 B,
   native −1,568, guest +537 for the positions, then +257 / +240 / +259
   for the string values, under the ceilings.

3. **`Session::pending()`** (pre-0.1 review M3). `pending() ->
   Option<Progress>` re-reports the stop as `start`/`resume` reported it:
   `HostCall { call, function, name, args, kwargs }`, `Waiting { calls }`
   (what is still pending after a partial batch) or `Parked { reason }`;
   `None` between cells. **Decision: the facade keeps the payload; the VM
   does not have it.** The engine converts a host call's arguments to
   `HostValue`s and hands them over; it keeps only the call id and pc.
   The Python values are not kept either: a host function called by a
   builtin (a sort key, a dunder, `map`) gets them from a Rust `Vec` that
   is gone once the call is reported, and converting again could differ
   (lenient conversion, the byte budget, `set_host_args`). So the session
   keeps the stop encoded as the snapshot header stores it (host values
   in the engine's snapshot encoding, `mm_engine::put_host_value` /
   `read_host_value`, iterative, so 1000-deep arguments cost no Rust
   stack; a cloned `HostValue` overflowed a debug test thread), decodes it
   on `pending()`, and writes it after the phase byte. **Snapshot format
   5 → 6** (images restore only on their build anyway). On restore the
   stop's host values are capped at the memory limit, outside the heap's
   budget (they are the host's, as snapshot output is), and a malformed
   stop is `Corrupt("session")`. `phase()` is derived from the stop.
   **Cost** [M]: every host call encodes its arguments once more. Per
   round trip of `f(i, 'abc', [1, 2, 3], k={'a': i})` ("small") and of
   `f(i, [{'id': j, 'name': 'item', 'tags': [1, 2]} for j in range(20)])`
   ("big"), answered at once (scratch harness, not kept; Ir per call
   over 2,000 calls, wall best of 3 × 200,000 calls):

   | | small Ir | big Ir | small µs | big µs |
   |---|---|---|---|---|
   | no `pending` (origin/main) | 10,595 | 216,137 | 1.04 | 19.2 |
   | eager encode, first version | 13,135 (+24%) | 243,098 (+12.5%) | 1.35 | 22.4 |
   | lazy: keep a clone, encode on demand | 13,444 (+27%) | 273,369 (+26%) | 1.34 | 23.9 |
   | eager encode into a `Vec`, all inline | 12,274 (+16%) | 227,631 (+5.3%) | 1.21 | 20.4 |
   | **same, writes out of line (kept)** | **12,471 (+18%)** | **232,250 (+7.5%)** | 1.24 | 20.4 |

   A clone costs more than the encoding: an allocation per string and
   container, and their frees at the next stop. So the stop stays
   encoded eagerly, and `put_host` now writes into a `Vec<u8>` instead of
   through the snapshot writer's per-field callback (the two snapshot
   callers, host-module constants and specs, encode into a scratch `Vec`;
   the bytes are the same). Its writes stay out of line: inlined, they
   cost about 600 B more in every build for about 110 Ir less per small
   call. What remains is the encoding (its work stack's allocations
   included) and the facade's header writes; the conversion itself is
   5.2K. Near zero
   would need no copy at all: the engine would keep the call's Python
   values reachable (a GC-root and engine snapshot change, in mm-engine)
   and convert them again on demand, with a result that depends on
   `set_host_args` at the time; not taken. Cloning is also recursive
   (1000-deep arguments overflowed a debug test thread's stack). Bench
   programs make no host calls (Ir unchanged, below).
   mm-wasm v2 gains the optional export `mm_pending` (`(s) -> u32` event:
   the stop's event and payload, `COMPLETE` without a completion between
   cells; a guest that reported a deadline park as `Requested` re-reports
   `Deadline`), and the reference host `Session::pending` (an error on a
   guest without the export). Tests: `tests/snapshots.rs`
   `pending_reports_the_stop_across_a_restore_in_a_new_process` (a host
   call with nested args and kwargs, two futures, a wait before and after
   a partial batch, a fuel park, idle; every image restored in a child
   process of the test binary reports the same stop); mm-wasm-host
   `pending_matches_the_native_facade_across_restores` (native, guest and
   images crossing both ways). docs: sdk.md (parking, snapshots), spec.md
   §10, wasm-embedding.md. Size [M]: `hello.wasm` +2,505 B, native
   +2,848, guest +3,119 (the decoder runs on every restore, so `hello`
   links it; `put_host` is out of line so the header and snapshots share
   one copy, which saved about 1.8 KB native; its `Vec` writer, above,
   is +921 / +784 / +925 of it). The ceilings were raised to the first
   version's measurement plus about 2 KB (S1): 2,086,600 / 2,270,100 /
   2,172,200; the branch measures 2,085,734 / 2,269,080 / 2,171,392, and
   2,086,027 / 2,269,256 / 2,171,692 rebased on string indexing (0234ca3).

4. **The `mm-cell-worker` binary stores host code in its images**
   (decision 60's open item). It serves one session per process, so it
   builds its `NativeEngine` with `with_host_code_in_snapshots(true)` (new
   builder `NativeEngine::with_host_code_in_snapshots`, passed to every
   Engine it makes, `with_limits` included); the in-process
   `NativeEngine::default()` keeps keyed images. **Changes nothing yet:**
   plak's bootstrap sends only untyped tool names, which generate no
   code, and the worker takes no snapshots. Test: `engine.rs`
   `host_code_in_snapshots_reaches_every_engine_it_makes` (an image from
   the binary's setting restores on a new Engine without compiling, for
   both limits; the default's compiles). The worker's library and
   protocol tests pass unchanged.

**Bench Ir** [M] (callgrind, `mm run`, stripped, all four commits
against origin/main 3720bf2, which the branch was rebased on): every
program within ±0.03% (`json` +0.023%); `startup` +0.55% (894,710 →
899,666, +4,956 Ir): **a bench-rule exception, accepted by the owner
(2026-11-08)**, as with 13, 39 and 45. +4,434 of it is glibc's
`pthread_getattr_np` in Rust's runtime init, before `main`, parsing
`/proc/self/maps`, which has two more lines for this binary (44
`sscanf` calls, 42 on main; its `RW` segments sit
differently on the pages, so the count follows the linker's layout, not
the code run). The rest is about +520 Ir (+0.06%). Rebased on string
indexing (0234ca3), whose binary already has those lines, every program
is within ±0.5% of it: `startup` +0.06% (899,267 → 899,824), `json`
+0.020%, the rest within ±0.001%. Gates on the rebased branch: `cargo
xtask check`, `cargo xtask wasm-host`, `conformance/codemode/run.py
--check`.

## CPython tests: core modules (2026-11-07)

More of CPython 3.14.7's `Lib/test` on mm with the asyncio batches'
harness (`conformance/breadth/cpython-tests`): json, re, collections,
datetime, str/string, dict, list, set, itertools, functools, dataclasses,
enum, math, statistics, decimal, textwrap, difflib, plus calendar, codecs,
hmac, htmlparser and zoneinfo. Tables, causes and what remains:
`docs/research/cpython-tests-core.md`.

- [M] All 130 units: 9,613 methods pass on CPython; mm 6,108 → 6,405, none
  lost. Requested units 3,254 → 3,452 of 4,130 (test_collections 57 → 76,
  test_enum 860 → 908, test_set 513 → 547, test_decimal 120 → 151,
  test_functools 102 → 115, datetimetester 437 → 449, test_json.test_enum
  0 → 6). Recorded 125 units: 5,964 → 6,261 of 8,989 (6,274 with the
  capability gaps below).
- Fixes, own cases 400–416 (each matches python3.14): json of
  builtin-type subclasses (IntEnum, StrEnum, OrderedDict, defaultdict,
  Counter) and `JSONEncoder`/`JSONDecoder`; PEP 560 `__mro_entries__`;
  str/int subclasses (StrEnum, IntEnum) as arguments of str methods, `in`,
  `join`, slices, `hex`, `bytes(n)`, `math`; ReprEnum `format`; container
  subclasses overriding `__getitem__`; UserDict/UserList/UserString/
  ChainMap; descriptor `__get__`; the metaclass class cell; the reflected
  `__ne__` after `NotImplemented`; builtin iterators' `__next__`;
  `dict.update` calling `keys()`; `sys.set_int_max_str_digits`; re
  ignore-case sets; `math.prod`/`sumprod` of Fraction/Decimal; `ord` of
  bytes and `deepcopy` of aware datetimes; `type()` through a base's
  metaclass; zero fill after an explicit alignment.
- Bench Ir vs 012435b [M]: startup +0.429%, json +0.335%, interp
  +0.211%, every other program within ±0.06% (vs 266bde7 before the
  rebase: json +0.338%, startup +0.206%, interp +0.199%, str +0.171%).
- Size [M]: vs 266bde7 `hello.wasm` +24,748, native +25,728, guest
  +24,763; on top of the capability gaps (below) +25,388 / +26,352 /
  +25,410 to 2,091,289 / 2,276,696 / 2,195,383; ceilings raised to the
  measurement plus about 2 KB under S1 (2,093,300 / 2,278,700 /
  2,197,400).

## Capability gaps: traceback, exception keywords, `ast`, `tokenize` (2026-11-07)

Owner request (via T-01a1a815): close the gaps real plak sessions and
earlier work exposed, each matching CPython 3.14, under S1 (size raised
only by the measurement plus about 2 KB, bench Ir within ±0.5%).

- **ImportError keywords** (5508313): `ImportError`/`ModuleNotFoundError`
  take `name`, `path` and `name_from` (instance dict) with CPython's
  "unexpected keyword argument" TypeError; the import system sets `name`,
  and `name_from` for from-imports. `BaseException.__init__` takes `**`
  keywords, so `super().__init__(..., name=...)` works in user subclasses
  of NameError and AttributeError (`conformance/own/391`).
- **traceback** (8e77d38): CPython's `TracebackException` (`from_exception`,
  `format`, `format_exception_only`, `print`, `stack`, chains, groups,
  notes); `format_exception*`/`print_exception` build one. The native
  `traceback._hint(exc, with_tb)` applies CPython's rules: a NameError's
  "Did you mean" needs a traceback (its stdlib "forget to import" note does
  not), AttributeError and ImportError hints work without one, `self.name`
  is suggested inside a method. SyntaxError has its attributes and
  CPython's `str()`; IndentationError and TabError are builtins
  (`conformance/own/390`). Left out: colors, stack-walking functions,
  keyword-typo hints for SyntaxErrors.
- **ast** (76c8803): `ast._parse` (mm-engine `astmod.rs`, feature `ast`)
  builds CPython's node classes from mm-parse's tree. The parser's `tree`
  mode (`mm_parse::parse_tree`) keeps annotations, parameter and alias
  positions, `__future__` imports and unfolded `-literal`s, with CPython's
  statement extents; f-string constants are positioned from the lexer's
  string segments; columns are UTF-8 byte offsets. `ast.py` adds dump,
  walk, NodeVisitor/NodeTransformer, literal_eval, get_docstring,
  get_source_segment, iter_fields/iter_child_nodes and the location
  helpers, with CPython's repr and DeprecationWarnings. `ast.dump` equals
  CPython's on 308 files (own cases, bench, bundled stdlib, 25 CPython Lib
  files), with and without attributes [M] (`conformance/own/392`). Left
  out: `unparse` (mm-parse's annotation stringizer covers expressions only
  and copies most kinds from source; a full unparser was not cheap),
  `compile()` of a tree, type parameters and `\N{...}` (compile errors as
  for the compiler), CPython's error position for some invalid `eval`
  inputs (`print "x"` in eval mode, a bare `import`'s end_offset).
- **tokenize** (b1013eb): `tokenize._tokens` (`tokmod.rs`, feature
  `tokenize`) over `mm_parse::tokens`, which adds what the lexer skips
  (comments, NL, the empty NL/NEWLINE at an unterminated last line,
  `{{`/`}}` splits, the empty FSTRING_MIDDLE that ends a format spec,
  keywords as NAME). `tokenize.py`/`token.py` give `generate_tokens`,
  `tokenize` (ENCODING, `detect_encoding`), `TokenInfo.exact_type` and
  CPython's TokenError/IndentationError/TabError arguments. Token for
  token equal to CPython on 311 files through both APIs [M]
  (`conformance/own/393`). Left out: `untokenize`, `open`, the regular
  expressions; `0777` raises where CPython's `tokenize` yields a token
  (`$` raised too until the `?`/`$` fix below); an f-string open at the end reports the lexer's message.
- The real-session gate (`conformance/codemode/real`): the 5 recorded
  cells that failed at `import ast`/`import tokenize` now pass it and stop
  at the sandbox's filesystem refusal, or (codex-sdk-native own-exceptions
  5) at a `ctx.list` call the recording never made, skipping its 8 later
  cells; `results/gate.jsonl` records the 13 with a `why`.

Size, each measured in order on 266bde7 [M] (`hello.wasm` / native / guest):

| Item | `hello.wasm` | native | guest |
|---|---:|---:|---:|
| before | 1,999,470 | 2,177,048 | 2,103,826 |
| ImportError keywords | +1,038 | +1,472 | +1,027 |
| traceback | +10,533 | +11,312 | +10,346 |
| ast | +48,131 | +52,288 | +48,079 |
| tokenize, token | +6,729 | +8,224 | +6,695 |
| after | 2,065,901 | 2,250,344 | 2,169,973 |

Packed bytecode [M]: `ast` 5,908, `traceback` 5,620, `tokenize` 1,663,
`token` 1,203. The rest of `ast` is Rust (native `hello` symbols [M]): the
node builder about 30 KB (`Build::expr` 8.7 KB, `Build::stmt` 6.8 KB,
`ast_parse` 3.7 KB, patterns, arguments, fields), and mm-parse's tree mode
about 8 KB once `parse`/`parse_with` and the lexer stop being inlined
into one caller; a first version that cloned a generator-expression call
argument pulled in 33 KB of AST `Clone` code and was rewritten. Smaller
forms, if the owner wants `ast` cheaper: drop it from the default set
(`mod-ast` is its own feature), or build the nodes in Python from a flat
native list (less Rust, slower).

Bench callgrind Ir vs 266bde7 [M] (`mm run`, stripped binaries at
equal-length paths): `str` +0.238%, `startup` +0.021%, `fmt` −0.013%,
every other program within ±0.016%.

Follow-ups (owner, 2026-11-07):

- **`mod-ast` is opt-in** (04f2f39): out of mm-stdlib's default set, so a
  default build's `import ast` raises `ModuleNotFoundError` with the "not
  enabled in this build" hint (`tests/features.rs`); docs/sdk.md documents
  the opt-in. `mm` enables it (with uuid, unicodedata, html.entities), so
  `cargo xtask check` still clippies and tests `astmod` and runs own case
  392 through conformance and durability. The three real-session ast cells
  are re-recorded with a `why`. Size on 168b7ac [M]: −43,874 / −46,304 /
  −46,254 (`hello.wasm` / native / guest), less than `ast` added because
  `warnings` and the parser's tree mode stay for the compiler's parser.
- **`?`, `$` and the backtick** (883b266) are `T::Unknown` tokens only the
  parser rejects, as CPython's tokenizer makes them `OP`s: an earlier parser
  error stands (`const x = 1; y = a ?? b` has its caret at `x`), `f'{a $ b}'`
  gets CPython's f-string message, and `tokenize` yields them as `OP`. The
  last real-session SyntaxError mismatch (phase2 codex-ptc-native
  `builtins-callbacks` 1) now renders as CPython's; four precedence rows.
  Size −5 / −128 / −6.
- Ceilings lowered to 2,049,700 / 2,232,600 / 2,151,400 (2,047,613 /
  2,230,584 / 2,149,327 [M] on 883b266, plus about 2 KB).

## Size pass 2: bundled data (2026-11-07, owner decision 46)

Owner-approved levers on stable Rust; `#[optimize(size)]` (lever 1) waits
for Rust 1.101 (below; partly done on shipped builds since decision 69). Keep every capability, bench Ir within ±0.5% per
program, conformance, durability, codemode and `wasm-host` unchanged.
Shipped builds, each lever in order on b01fd7d [M]:

| Lever | `hello.wasm` | native `hello` | guest |
|---|---:|---:|---:|
| before | 2,107,210 | 2,311,512 | 2,211,094 |
| `NATIVES` in its own static | −5,444 | −33,200 | −5,444 |
| bundled modules: sections, shared strings, per-section codes, denser operands | −102,523 | −101,520 | −102,771 |
| after | **1,999,243** (−5.1%) | **2,176,792** (−5.8%) | **2,102,879** (−4.9%) |

Ceilings lowered to 2,001,200 / 2,178,700 / 2,104,900 (guest 2,105,800
after the `mod-asyncio`/`mod-json` gates, below).

- **`NATIVES`** (from the size-map thread). `&PACKED.natives` borrowed a
  field of the const `Packed`, so the whole build-time struct shipped:
  string slots and blob at capacity, 42,496 B of linear memory in wasm
  (wasm-opt already drops its zero runs, so −5,444 B of file) and 47,168
  B natively, zeros included. A static copy of the array
  (`NATIVE_ARRAY`), as `LISTS` and `MODULES` already had.
- **Bundled modules.** `Program::encode_sections` writes nine streams per
  module (`mm_compile::section`): own strings (POOL), string references
  (INDEX), other metadata (META), opcodes (OP), `r` operands (REG), `b`
  operands (BYTE), `u` operands (U: 0 for the next inline-cache slot,
  else value + 1, 255 escapes), `i` operands and the escaped `u` rest
  (I, zigzag LEB128) and `j` operands relative to the instruction (JUMP,
  zigzag LEB128). Strings three or more modules use (675) are stored
  once, raw (`SharedStrings`, 4,517 + 1,350 B), and become `Arc`s only
  when a decode uses them. Each stream is packed against one set of
  Huffman codes for all modules (`pack_shared`; 9 headers, 1,422 B).
  The stdlib data in `hello.wasm` went from 386,268 to 278,662 B [M];
  unpacked from 776 KB to 653 KB. By stream (packed, the 63 default
  modules, on a72abfe's sources [M]): POOL 59,045, INDEX 46,751, META
  21,800, OP 49,829, REG 47,855, BYTE 4,935, U 31,251, I 2,605, JUMP
  7,820. The format is lossless at the `Program` level (build.rs checks
  every module round-trips); snapshots, `CODE_STDLIB` references and
  mm-wasm v1 are unchanged; the build id hashes the packing.
- **Decoders.** `Packing::decode` keeps one `pack::Unpacker` per section
  in `Vm::unpackers`, built by a cell's first decode and dropped when the
  cell ends (a restore keeps its own). Every unpacked section is charged
  its tables as before, so a charge does not depend on which decoders
  exist (the cache tests "same charge cold, warm or off" pass). Building
  them is most of the extra cold cost: `Huff::new` now fills its lookup
  table by doubling in canonical order (393K → 152K Ir for one cell's
  tables [M]), and the operand loop reads one byte per operand in the
  common case from a per-opcode `PLANS` table (kind and bit offset).
- **Speed** [M]. Bench Ir against b01fd7d: 12 programs within ±0.02%,
  `startup` +0.31% (+2.7K, libc `/proc/self/maps` parsing under
  valgrind, as in decision 45). Import Ir per session (the import-cost
  harness, `measure.py`), cache on (the default): steady −0.2% to −4.3%
  on every row, first import in a process −2.1% to +4.8%; cache off:
  steady −1.4% to +3.3% (json −1.4%, five-module cell −0.4%, re +1.1%,
  collections +1.9%, itertools +3.3%), RSS per session unchanged. Decode
  alone (`split.py`, cache off): re 2.48 → 2.60M, collections 1.89 →
  2.03M, decimal 9.96 → 10.16M, zoneinfo 13.85 → 13.68M, five-module
  cell 9.35 → 9.45M: about 0.1M per cell for nine table sets instead of
  two per module. Wall medians moved within the noise (±5% on most rows).
- **Rejected** [M] (packed stdlib bytes): one solid stream per section
  253,826 (imports would unpack every module); dependency-chain groups
  (34) 272,328, the import component 51 modules; LZMA (xz) solid 277–283
  K and per module 325–343 K; an order-2 context model over opcodes and
  fields is 145.7 KB by entropy alone against 145.0 KB for the packed
  code streams, so a stronger entropy coder than LZ + per-stream Huffman
  does not pay here; interleaved instructions with the same transforms
  183,235 for the code streams (split: 144,295); `b`/`i`/`j` merged into
  one stream +1,588 B for 3K Ir; a shared-string threshold of 2 or 4
  instead of 3: +1,796 / +553 B (277,758 at 3).
- **Not done** [E]: the prelude (67,124 B packed) in the same sectioned
  form, about −15 KB, but its units are lazily built from offsets into
  chunks, so each unit would need nine offsets (about 7 KB of index);
  revisit with the prelude-unit format. Done 2026-11-09 without the
  index ("Engine size review: prelude sections and `method_names`").

**`mod-asyncio` and `mod-json` gate their code** (owner, with this pass).
Both features gated nothing (the modules live in the engine: natives,
prelude units, `BUNDLED`). They now turn on mm-engine features `asyncio`
and `json`; off, their natives and builtins rows, their prelude sections
(build.rs skips units under `__name__ = 'asyncio…'`/`'json…'`) and,
for asyncio, about 1,000 lines [E] of the scheduler's task, gather, sleep,
callback and loop paths drop out, and the import raises
`ModuleNotFoundError: No module named 'asyncio'` with the "not enabled in
this build" hint other disabled modules get. Kept as core: coroutines,
`await`, async generators, `async with`, host futures (a cell is task 0
of the scheduler) with `done`/`result`/`exception`/`cancel`/`cancelled`
and their repr (`_aio_repr` moved out of the asyncio section). Type ids,
the snapshot layout and mm-wasm v1 are unchanged; features are already
in the build id. The facade's mm-stdlib default set is now its default
`stdlib` feature, so an embedder can turn it off (docs/sdk.md). Shipped
`hello` against the default build of the same tree [M] (`hello.wasm` /
native): without asyncio −51,274 / −56,528, without json −31,480 /
−34,432, without both −82,133 / −90,896 (1,917,336 / 2,086,152). The
default build: +227 / +256 B for the gates (guest +947, the facade's
`environment` branches; its ceiling set to 2,105,800), bench Ir within ±0.011%
(`startup` −0.24%). `cargo xtask check` now clippies mm-engine with no
module features and the facade without `stdlib`, and runs
`tests/features.rs` there (coroutines, async generators, `async with`
and awaited host futures without asyncio; the left-out modules are not
found). `struct._calclen` was the one row missing its feature gate
(db2521a).

**Lever 1, `#[optimize(size)]`, when Rust 1.101 is pinned.** The cold set
measured free (−141 KB native, −112/−153 KB Wasm, all bench programs
within ±0.2% [M], "Size by build structure"); mark every function in
these files, then rerun `cargo xtask size` and the bench Ir:
- `crates/mm-engine/src/`: `math.rs`, `cmath.rs`, `libm.rs`,
  `complex.rs`, `hashlib.rs`, `re.rs`, `percent.rs`, `bytes.rs`,
  `binascii.rs`, `random.rs`, `snapshot.rs`, `bigint.rs`,
  `unicodedata.rs`, `mmmod.rs`, `modules.rs`, `text.rs`, `intops.rs`,
  `exc.rs`;
- `crates/mm-core/src/snapshot.rs`.
Not `json.rs` (json_rows +7% at size). The decoders added here
(`encode.rs` `In`, `pack.rs`) are import-path code: measure them with
the import harness before adding them.

## Host-module code per Engine (2026-11-07, owner decision 60)

Pre-0.1 review M8, owner-approved. `define` generated and compiled each
typed host module's Python in every session, and every image stored that
code. Now the Engine compiles it once and images refer to it by key
(decision 60 has the design, API and owner questions).
bench/evidence/results/2026-11-07-host-code (harness
bench/evidence/host_defs) [M, 16-vCPU orb, pinned]:

| 100 typed tools | before | after |
|---|---:|---:|
| `Session::new` + `define`, first in a process | 6.49 ms | 6.46 ms |
| … warm Engine | 5.13 ms (61.5M Ir) | 0.96 ms (14.4M Ir) |
| RSS per warm session | 1,796 KiB | 596 KiB |
| image (base 41,081 B) | 597,818 B | 293,606 B |
| zstd -1 / with the startup prefix | 111,753 / 106,861 B | 34,222 / 27,948 B |
| restore, warm Engine | 0.85 ms | 0.25 ms |
| restore, new Engine | 0.85 ms | 6.6 ms (compiles) |
| … code stored (`with_host_code_in_snapshots`) | 0.85 ms, 597,818 B | 0.87 ms, 517,350 B |

- **10 tools:** warm define 629 → 130 µs; image 102,832 → 73,030 B
  (zstd -1 with prefix 15,295 → 6,315); restore warm 124 → 49 µs, on a new
  Engine 125 → 600 µs.
- **600 tools:** warm define 36.7 → 7.5 ms; first in a process 41.2 →
  42.8 ms; RSS per session 10.2 → 2.4 MiB; image 3.44 → 1.60 MB (zstd -1
  with prefix 593 → 136 KB); restore warm 5.05 → 1.91 ms, on a new Engine
  5.3 → 45.6 ms.
- **1,200 tools in 12 modules:** image 6.30 → 2.65 MB.
- **The Engine holds** about 0.69 MB of compiled code per 100 tools: the
  8 MiB default cap fits about 1,200 tools, or 790 beside every bundled
  module decoded (2.9 MB). Past it, sessions compile and store their own.
- Later cold Engines in one process measure slower than the first (100
  tools: 5.2 → 7.1 ms median) at equal Ir: the earlier Engines keep their
  code, so each new one defines into new memory.
- `mm-cell-worker` defines only untyped functions, which generate no code,
  so its shared Engine gains nothing until the cell protocol carries typed
  functions; its tests pass unchanged. The durability gate's `mm` fixtures
  are untyped too; `tests/host_defs.rs` covers keyed images (restore on new,
  uncached and warm Engines, parked inside generated code, earlier
  definitions, stored code, a definition that no longer reproduces its key).

## Dunder flags by hash (2026-11-07)

Owner-approved fix for the class-flag lookup the import-cost pass added in
9e272a0: on host1 (main 357e003), `flagged` was 18.6% of a warm
`itertools` import's cycles plus `memcmp` 15.2%, a binary search with
string compares per dunder key of every class in the MRO.

- **Change** (`mm_engine::class`): the flagged dunders' `hash_bytes` are
  a const table (`mm_core::hash_bytes_const`, tested against
  `hash_bytes`), with a 256-slot open-addressing index built at compile
  time. A class dict key is matched by its cached hash: one probe, then
  one text compare only for a real dunder. A builtin type last in the MRO
  (`object`) is skipped, since it decides nothing after it. Behaviour is
  unchanged: own case 418 covers names built at run time, near misses,
  `setattr`/`delattr` of computed names and builtin bases before a mixin;
  durability parks and restores it at every few fuel units.
- **Measured** [M, 16-vCPU Xeon orb, pinned, cache warm;
  bench/evidence/results/2026-11-07-dunder-flags]: `itertools` import
  1.92M → 1.55M Ir (−19.0%), 178 → 92 µs (−48%); five-module cell 12.91M
  → 12.53M Ir (−2.9%), 1,475 → 1,337 µs (−9.4%). The estimate was −25 to
  −30% and −6 to −7% wall [E].
- **Bench Ir** against 357e003: every program within ±0.01%, `startup`
  −0.75% (902,930 → 896,160: the prelude's classes) [M].
- **Size** [M]: `hello.wasm` +320 to 2,054,681, native +848 to 2,238,520,
  guest +309 to 2,159,773; under the ceilings, unchanged.

## Import-cost pass (2026-11-07)

Owner-approved pass on the cost of importing bundled modules
(docs/research/import-cost.md; bench/evidence/results/2026-11-07-import-cost;
harness bench/evidence/import_cost). No module moved to Rust; behaviour
unchanged.

- **Engine**: `reserve_code` keeps its code-byte sums incrementally
  (`Vm::code_sums`; it walked every code object on each load); `json`,
  `asyncio` and `sys` build their prelude attributes with
  `Vm::prelude_get_all` (one unpack per chunk, built in the same order);
  `pack::Unpacker` keeps a shared header's decoders across parts, with a
  tighter long-code path and output loop; a new class's inline-slot hint
  starts at what its bases use or its `__init__` stores on `self` (also
  when `__init__` is assigned later), so first instances hit the attribute
  caches; class attribute assignment recomputes the dunder flags only for
  flagged dunders, found by binary search.
- **Stdlib**: `zoneinfo._zoneinfo` inlines `calendar.isleap`/`monthrange`
  and no longer imports `calendar` (`dir(zoneinfo._zoneinfo)` loses
  `calendar`; identical results on 1,680 zone/date probes and CPython).
- **Ir per session import** [M] (7541558 → this pass, cache off; cache warm
  in parentheses): five-module cell 45.6M → 29.3M (18.6M), asyncio 13.1M →
  6.0M, zoneinfo 66.9M → 33.3M (17.5M), decimal 68.8M → 46.3M (34.7M),
  statistics 105.4M → 77.4M (58.1M).Wall, pinned [M]: the
  five-module cell 4.86 → 3.59 ms (2.49 ms with the cache), RSS per session
  1,852 → 1,203 KiB with the cache, mm-wasm guest 7.85 → 5.12 ms.
- **Checks** [M]: `cargo xtask check`, `cargo xtask wasm-host`, the codemode
  `--check` (1,793 solutions) pass, and oracle durability runs of a script
  importing 16 modules (1,911 parks, every 28 fuel units; its import line
  alone 3,810 parks, every 12) match their unparked runs. Bench Ir against
  1c03cff within ±0.031%, `startup` −0.23% (equal binary paths).
- **Size** on 1c03cff [M]: `hello.wasm` +4,227 to 2,107,210, native +5,376
  to 2,311,512, guest +4,270 to 2,211,094; ceilings raised to the
  measurement plus about 2 KB (per item in `xtask/src/main.rs`).
- **What remains** (cache warm, five-module cell 19.6M Ir in `execute`):
  prelude units 6.1M (asyncio's eager attributes), `load_in` 5.5M, bodies
  about 6M; heavy modules are dominated by their bodies (regex compiles at
  import, transitive imports, class machinery). Not done: lazy imports that
  CPython binds as public names (`statistics.Decimal`), a native regex
  parser (estimate only), an Engine-level prelude-unit cache.

## asyncio batch 3: `except*`, submodules, Barrier, Runner, contextvars (2026-11-06)

Owner-approved batch (size under S1), from 1f27970:

- **`except*`** with CPython 3.14's semantics. The AST `Try` has a `star`
  flag (own parser and ruff front end; `cargo xtask parse-diff` clean).
  The compiler keeps a state list `[orig, rest, raised...]`; the new
  opcode `EXC_STAR` (108, `rb`) calls the prelude's `_star_match` for a
  clause (the type checks, a naked exception wrapped in a group with this
  frame's traceback, `split`) and `_star_prep` at the end (a port of
  `_PyExc_PrepReraiseStar`: re-raised parts keep their place in the
  original group, new ones join a new group). "No match" is tested with
  `is None`, so a falsy group subclass still matches. `break`/`continue`/
  `return` in an `except*` block are CPython's SyntaxError, at the same
  caret (a literal return value, else the statement). New natives
  `_mm.exc_same_meta` and `_mm.exc_tb_here`; `derive` keeps
  `__traceback__`.
- **asyncio submodules**: `asyncio.events`, `exceptions`, `futures`,
  `locks`, `queues`, `runners`, `taskgroups`, `tasks` and `timeouts` import
  (also `from asyncio import taskgroups`) and hold `asyncio`'s own objects.
- **`Barrier`** (CPython's `locks.Barrier`: `wait`, `abort`, `reset`,
  `parties`/`n_waiting`/`broken`, `BrokenBarrierError`, context manager)
  over `Condition`, so its waiters are FIFO and park like any other.
- **`Runner`** (`run(context=)`, `close`, `get_loop`, context manager),
  **`new_event_loop`/`set_event_loop`** and `loop.run_until_complete()`
  on a coroutine, a gather, a task or any awaitable. Each run is one
  `asyncio.run` of the same dispatch loop (`_mm.aio_run(main, root,
  open)`), with no nested loop; the `RUN_OPEN` coroutine bit keeps the
  task numbering CPython's. `get_event_loop()` outside a run returns the
  loop `set_event_loop` set.
- **Per-task `contextvars`**: `contextvars.py` is rewritten over the
  engine's context (`Vm::context`, per-task `Task.context`, both
  snapshotted and traced). A task runs in a copy of its creator's context
  or the one given to `create_task(context=)` (TaskGroup too); callbacks
  take a copy; `asyncio.run` runs main in a copy.
- **Leftovers**: `SystemExit.code`; a `KeyboardInterrupt`/`SystemExit`
  raised in a task cancels the main task and leaves `asyncio.run` after the
  shutdown (`Vm::escape`), as does one from a loop callback; "Task/Future
  exception was never retrieved" is reported when `asyncio.run` ends
  (`Vm::unretrieved`).

Checks [M]: own cases 387 (`except*`), 388 (submodules, Barrier, Runner,
loops) and 389 (contextvars, escapes, never-retrieved) match CPython
3.14.7 line for line; the SyntaxError forms are a `session.rs` test
(a whole-file SyntaxError cannot be an own case, which the parking test
compiles). `cargo xtask durability --filter asyncio` 14 of 14 runs
identical across 4,218 parks, `--filter 387_except` 2 of 2 across 804. A
new host-call test parks and restores across two `Runner.run()`s, a
Barrier wait with a cancelled host call (`Abandoned` in `call_log()`) and
a host call awaited inside an `except*` handler.

CPython `Lib/test` methods [M] (harness, the same 18 asyncio units as the
two earlier batches, plus `test_except_star`, `test_exception_group` and
`test_context`; 941 methods, 703 pass on CPython): 312 → 478 on mm, no
regressions. The 18 units: 283 → 388 of 538. `test_taskgroups` 2 → 84 of
84, `test_locks` 49 → 71 of 72 (the rest: a second loop in
`test_ambiguous_loops`), `test_asyncio.test_context` 0 → 1 of 1,
`test_except_star` 0 → 35 of 60 (the rest: `BlockingIOError` 7,
`compile` 4, traceback-object identity about 12), `test_context` 0 → 26
of 53, `test_runners` 0 of 1 (it needs event-loop policies).

Bench Ir vs 1f27970 [M] (callgrind, `mm run`, equal-length paths): startup
−0.086%, json −0.066%, exceptions −0.009%, every other program within
±0.001%. Size vs 1f27970 [M]: `hello.wasm` 2,078,951 → 2,100,215
(+21,264), native `hello` 2,276,888 → 2,302,728 (+25,840), guest
2,182,840 → 2,204,237 (+21,397); ceilings 2,102,200 / 2,304,700 /
2,206,200 (measurement plus about 2 KB, owner decision S1).

Still differs: "never retrieved" reports come at the end of `asyncio.run`
(CPython: when the object is collected, so possibly earlier); a task that
escapes with `KeyboardInterrupt`/`SystemExit` is not itself logged as
never retrieved afterwards (CPython logs it); `run_until_complete()` and
`Runner.run()` cancel the tasks a run leaves (CPython keeps them pending
for the next run); event-loop policies, `loop.run_forever()`/`stop()` and
eager task factories are not available; unawaited coroutines still warn
at the end of the cell, once per name; functions have no `__code__`, so
task reprs lack `coro=<...>`; `loop.time()` is virtual time; TaskGroup and
shutdown cancel in creation order (CPython walks a set).

## Restore hardening and numeric follow-ups (2026-11-06)

From the host1 `snapshot_restore` campaign, the Mac thread's restore sweep
and difffuzz (seeds in `conformance/breadth/fuzz/artifacts/`, root causes
in that README). Each fix has a test that fails without it.

- **References name objects of their kind.** The pc panic
  (`interp.rs` `bc[pc]`) was a `GEN`-kind value in the VM part naming a
  non-generator object; the engine read `g::YPC` from it.
  `Vm::read_snapshot` now refuses any reference, in the VM part or an
  arena value slot, whose kind is not its object's type
  (`Corrupt("references")`), a `StoreReg`/`Unpack` continuation outside its
  frame's registers, and a bottom frame not at base 0.
- **Dict index tables are checked on restore** (`dict::check_indexes`):
  entry bits in `1..=used`, no duplicates, tags matching hashes, every live
  entry on its probe sequence, live count equal to `len`
  (`Corrupt("dict index")`). Release builds looked up the wrong slot
  silently.
- **A class's size hint is clamped** to the 32 inline slots in
  `Vm::new_instance`; a corrupt hint built a 5.6 GB native buffer (the two
  OOM seeds). Audit: the decoder already caps every preallocation by the
  bytes left and charges it to the commit budget; function code indexes,
  coroutine and generator pcs are checked or bounds-safe; inline caches
  restore empty; handler targets come from recompiled code.
- `tests/snapshots.rs` `fuzzed_images_restore_to_errors_not_panics` replays
  every seed (11) through the fuzz target's own input code, now shared as
  `fuzz/fuzz_targets/snapshot_restore_input.rs`; it fails on a panic the
  facade contained or on 512 MiB of peak memory growth.
- **`round(float, -n)` is correctly rounded** (own case 383): the float's
  exact value to a multiple of 10**n, half to even, then the nearest float,
  with CPython's `OverflowError` when that overflows.
  `round(1e40, -1)` was `1.0000000000000002e+40`. 200,000 random cases
  match CPython [M].
- **difffuzz allows 1 ulp on libm results** only (`_T`-wrapped
  `math.exp/log/log2/log10/sin/atan` and float `**`); exact operations
  stay exact (`conformance/breadth/difffuzz/README.md`).
- Size on 8cf3a11 [M]: hello.wasm 2,063,125 → 2,068,008 (+4,883: restore
  checks +4,097, `round` +786), native 2,261,064 → 2,266,424 (+5,360),
  guest 2,167,383 → 2,172,138 (+4,755); ceilings 2,070,000 / 2,268,400 /
  2,174,100. Bench Ir vs 8cf3a11 within ±0.18% on every program but
  startup, −0.78%: all of it (−6,916 instructions) is glibc parsing
  `/proc/self/maps` at process start, no engine code [M].

- **Native buffers grow geometrically up to the memory limit.**
  `ReprWriter`, `NativeText`, `NativeVec<u8>` and json's `Out` fell back to
  exact growth once doubling did not fit, which reallocated on every later
  write. That was O(n²) with a copying `realloc` (the `vm_bytecode`
  timeouts). They now share `scratch::grown`: double, or take what the
  budget has left once, after which growth fails. In the unit test, writes
  up to a 4 MiB limit took 524,301 reallocations before the fix and fewer
  than 40 after [M]. Which sizes fit is unchanged.
- **Repr text costs fuel by length** (one unit per 64 bytes, as the str
  methods charge). A 20,000-fuel cell doubling `repr` used to reach 3 MB and
  the memory limit; now it parks on fuel
  (`doubling_a_repr_near_the_memory_limit_is_bounded_by_fuel_and_time`).
- **"Did you mean" hints are computed when rendered** (campaign N1).
  NameError, AttributeError and missing module attributes keep a pending
  tuple in the exception's hint slot (`exc.rs` `hint_of`), resolved and
  charged one fuel unit per candidate by the report, as CPython's
  traceback does. A caught NameError costs no suggestion: 200,000 of them
  took 8.35 s (Mac, campaign) and take 0.09 s now [M]. The eval/exec hint
  and asyncio's group hints stay eager. Null bytes in source raise
  CPython's SyntaxError (N2), and the parse fuzz thread is 8 MiB (P1).
  Size on bb34446 [M]: hello.wasm +3,325 to 2,074,056 (hints +2,286, null
  bytes +1,039), native +2,864 to 2,272,632 (+2,224, +640), guest +3,073
  to 2,177,940 (+2,290, +783); ceilings raised to 2,076,000 / 2,274,600 /
  2,180,000 (measurement plus about 2 KB, owner decision S1). Bench Ir vs
  bccb902 within ±0.02% on every program but the three smallest, which
  move by a near-constant 7k–13k instructions at process start (startup
  −0.75%, callgrind −0.29%, opcost −0.26%; the class of owner decision 39)
  [M].
- **SyntaxError precedence as CPython's** (real plak sessions on host1:
  model-written calls with an unquoted regex, `ctx.grep(pattern=fn a|b\(,
  ...)`). mm let any later tokenizer error replace a parser error; CPython
  (`pegen_errors.c`) does only for one the parser fetched, or one its
  scan of the rest of the source raises at once. A stray `\\` it never
  fetched (`E_LINECONT`, deferred) leaves the parser's error standing,
  unless a bracket stays open from a line before the last token read
  ("was never closed"). The parser now records how far it read; the comma
  hint and a generic error at an operator no expression starts with
  (`pattern=^...`) are the errors known to stop where CPython's error pass
  stops (not with a plain string next to the gap or `b if` after it,
  which CPython reads on); every other parser error keeps the old order.
  The comma hint's range starts at a keyword or starred argument's value,
  as CPython's. All 5 cells match CPython 3.14 in message, line, offset and
  range (`tokenizer_and_parser_errors_take_cpythons_precedence`, 33
  sources; whole-file SyntaxErrors cannot be own cases). Remaining known
  differences, all older: a dict value or subscript with two expressions
  side by side, `$`, an unindent or tab error after a parser error, and
  CPython's file tokenizer shifting a line-continuation caret on lines
  after the first. On 1,200 own cases with random syntax damage, compared
  with python3.14: 63 errors newly match, 0 regress [M]; bench Ir vs
  f75899d +0.13% on the smallest programs (callgrind, opcost: 5k–6k more
  instructions to parse), 0.00% elsewhere [M]. Size vs f75899d [M]:
  hello.wasm +906 to 2,101,121, native +1,200 to 2,303,928, guest +904 to
  2,205,141, all under their ceilings. parse-diff needs no new recorded
  difference: ruff rejects every such source too.
- **Hints render as CPython's traceback renders them** (owner follow-up to
  N1). The hint slot holds `(kind, name, data, locals, note)`, which also
  backs `NameError.name`, `AttributeError.name` and `AttributeError.obj`
  and their constructor keywords (new; unexpected ones raise CPython's
  `TypeError`). The suggestion is computed from their current values each
  time a report renders. A `NameError` gets one only with a traceback,
  from the names of the frame that raised it (kept at `raise` for a hand-
  built one), so `format_exception_only` shows none, as in CPython 3.14;
  an `AttributeError` gets one from `name` and `obj` with or without.
  mm's own sentences (eval/exec, asyncio's groups) always show. Own case
  386. Not covered: `TracebackException` (mm's traceback module leaves it
  out), `ImportError`'s `name`/`path`/`name_from` keywords, and
  `super().__init__(..., name=...)` in a user `__init__`.
  The raise path tests one flag until guest code builds a `NameError`
  itself: bench Ir vs 09ea2b1 within ±0.05% on every program (exceptions
  +0.035%), startup −0.24% [M]. Size on 09ea2b1 [M]: hello.wasm +4,895 to
  2,078,951, native +4,256 to 2,276,888, guest +4,900 to 2,182,840;
  ceilings raised to 2,081,000 / 2,279,000 / 2,185,000 (measurement plus
  about 2 KB, owner decision S1).
- **Container `repr`/`str` hand off to the prelude twin on fuel** (owner
  decision, under "fuel in every guest-driven native loop"). Inside a cell
  with limited fuel, `Vm::repr` walks at most the fuel left in visits; then
  it charges them and raises `need_py`, and `_repr_of` renders level by level
  through `_walk_repr` (one native walk per item, the active set carried in
  `_REPR_ACTIVE`). `%` takes the hand-off through `_percent`. Error messages
  without a twin (`create_task`, duplicate match keys, `float()`) and host
  text (`repr_of`, `str_of`, exception text) use `repr_whole`/`host_text`
  and never hand off. The completion text stops at the cut: `Head` at the
  limit, `HeadTail` at max(8 × limit, 1 MiB) for containers, then the
  shaped cut or ` ... [more omitted]`. Repr seed: 18 s → 11 ms in a debug
  build; first-park overdraft in the shared-graph test 8,483,291 → under
  95k units [M]. Own case 385 (durability 2/2, 1,986 parks).
- Size on 863beb1 [M]: hello.wasm +2,690 to 2,070,731, native +3,184 to
  2,269,768, guest +2,679 to 2,174,850; ceilings raised to 2,072,700 /
  2,271,800 / 2,176,800 (measurement plus about 2 KB). Bench Ir vs 863beb1
  within ±0.02% on every program [M] (bench runs with fuel unlimited, so
  the twin is not on its path).
- `sorted_json_entries_share_the_memory_budget` lost its refusal window:
  sorted and plain dumps of its dict both fit from 4,224 KiB now [M]. The
  old window came from exact growth doubling once more, for the closing
  brace, after the sorted entries were freed. The test now refuses at
  4,208 KiB and checks for leaks by rebuilding the data in the same
  session. In the shapes probed, the sorted entries' charge no longer moves
  the threshold, so this test no longer isolates it.
- Size on 69749db [M]: hello.wasm +33 to 2,068,041, native +160 to
  2,266,584, guest +33 to 2,172,171, under the same ceilings. Bench Ir vs
  69749db within ±0.02% on every program [M].

## Size pass (2026-11-05, owner decisions 41–42)

Owner-approved size pass after the feature batches: win size back, keep
every capability and the speed (bench callgrind Ir within ±0.5% per
program; conformance, durability and codemode unchanged). Shipped builds
after `wasm-opt -Oz`, each lever measured in order on 874409b [M]:

| Lever | `hello.wasm` | native `hello` | guest |
|---|---:|---:|---:|
| before | 2,171,955 | 2,286,168 | 2,322,247 |
| `crates/mm-wasm-alloc` (ba84776) | −83,553 | 0 | −97,112 |
| facade at `opt-level = "z"` (07172f8) | −12,541 | −18,928 | −37,548 |
| bloat-audit fixes, together (57fcc72) | −12,736 | −6,176 | −20,204 |
| after | **2,063,125** (−5.0%) | **2,261,064** (−1.1%) | **2,167,383** (−6.7%) |

Ceilings lowered to 2,065,100 / 2,263,000 / 2,169,400. `wasm32-wasip1`
(reported only): `hello.wasm` 2,098,010 → 2,073,573, guest 2,225,971 →
2,169,244 [M]; the unknown-unknown guest is now smaller than the wasip1
one. Bench Ir of `mm run` (plain release), all three commits against
874409b [M]: startup +0.208%, exceptions −0.012%, every other program
within +0.001%; outputs equal. `embedding_bench` 68.75M → 68.56M Ir
(−0.28%) [M].

**1. dlmalloc `validate_size`** (decision 41). On `wasm32-unknown-unknown`
std's allocator calls `dlmalloc::Dlmalloc::free`/`realloc` with the layout;
both start with `validate_size` (two `assert!`s on the chunk header), and
fat LTO inlines them into every deallocation site (about 1,460 in
`hello.wasm`, 1,760 in the guest, 28 of them in `Vm::execute`). Two routes
were measured: (b) `crates/mm-wasm-alloc`, the same dlmalloc (`=0.2.14`,
std's `library/Cargo.lock`) called through `c_free`/`c_realloc`:
−83,553 / −97,112 [M]; (c) the checks as `debug_assert!`, simulated by
patching std's copy: −83,957 / −98,448 [M], but only after a dlmalloc
release, a rust-lang/rust bump, a stable release and a pin move. wasip1 is
unaffected: std uses wasi-libc's malloc there. The owner chose (b).
The guest and the `hello`/`roundtrip` examples register it; the facade
never does (a binary has one global allocator; docs/sdk.md tells
embedders how to opt in). `cargo xtask wasm` runs the crate's `realloc`
example under wasmtime: over-aligned blocks (16, 64, 4096) keep their
alignment and contents while growing and shrinking; the same example
returns 1 against an allocator that sends every `realloc` to `c_realloc`
[M]. The mm-wasm-host parity tests pass against the guest.

**2. Per-package opt-levels** (decision 42). Each package at `s`/`z` on
top of the rest, size alone on 874409b [M] (`hello.wasm` / native / guest)
and Ir where it mattered:

| Package | Result |
|---|---|
| `micro-monty` (facade: host-API rendering, help, environment, results, snapshot plumbing) `z` | −16,232 / −14,080 / −51,615 alone (before the snapshot buffer moved); `s` saves less (guest −41,873); Ir +0.81% startup until `Vm::new` was kept out of line (+0.21% after); `embedding_bench` +1.04% until the snapshot buffer moved into the engine (−0.28% after). **Taken.** |
| `mm-core` `s` / `z` | +42,358 / +62,964 B `hello.wasm` and +3% to +22% Ir: rejected |
| `mm-compile` `z` (is `s`) | +117 / +2,768 / +551: rejected |
| `mm-wasm-abi` `z` / `s` | `z` +4,671 guest; `s` −511 guest: not worth a profile entry |
| `mm-stdlib` `z` | 0 (its Rust is a table; the natives are `mm-engine`) |
| `mm-parse` | already `z` |

`mm-engine` holds the VM, the builtins and every native module, so it
stays at 3; finer splits need `#[optimize(size)]` (Rust 1.101, "Size by
build structure" above, still the bigger lever: −112/−153 KB Wasm then).
A trap found on the way: fat LTO optimizes at the root crate's level, and
`hello` is the facade's example, so at `z` its whole program was
optimized for size (another −59 KB of `hello.wasm`, an artefact no
embedder gets). `cargo xtask size` and `wasm` now build `hello` with
`cargo rustc -- -Copt-level=3` (`shipped_example`); a package override
also drops the profile's `strip` for its examples, so `strip = true` is
repeated.

**3. Bloat audit** (twiggy 0.8 on named `-g` modules, cargo-bloat natively).
No panic or formatting machinery survives `immediate-abort` (no
`panicking`, `unwrap_failed`, bounds-check messages; aborts are
`unreachable`), and monomorphization is modest (`drop_glue` 31.9 KB over 75
instances in hello; `NativeVec` family 4.3 KB). Fixed, cold paths only,
each alone on 07172f8 (`hello.wasm` / native / guest) [M]:
- traceback rendering's `BTreeSet<u64>` (cause/context cycle check) →
  `NativeIds`, and `BTreeSet<usize>` (caret lines) → a `Vec` of at most 8
  marks: −5,271 / −5,440 / −5,280;
- `{:?}` on names in host-definition errors and the host BigInt error →
  `text::debug_quoted` (core's Debug escaping and printable tables go):
  −4,213 / +1,472 / −4,099; identical to `{:?}` on quotes, backslashes and
  control characters (checked on 12 strings), except that other
  unprintable characters are left as they are;
- one shared Huffman header for the bundled modules (`pack_shared`, as the
  prelude's chunks; `StdModule::header`), fitted to the enabled modules:
  −3,231 / −2,208 / −3,219; changes the build id;
- `Session::environment` merges the engine's names into the sorted
  generated list, `host_modules` inserts in name order (stable): the `&str`
  and `HostModule` slice sorts go, 0 / 0 / −7,604.

Measured and rejected: `Box<Vm>` in `Session` (−30 / −101 B);
`-Zshare-generics=y` (+57 KB). Left, with estimates [E]: a non-generic
helper for `&str`-only `format!` messages (298 `format_inner` sites in
hello, 6–12 KB, some inside hot functions, needs Ir checks per site);
fewer `BTreeMap` key types in mm-compile (2–4 KB); one `float_repr` shared
by mm-compile and mm-engine (~1 KB). The large functions are the
interpreter (`Vm::execute` 78 KB in the guest), the guest's
`mm_session_restore` 43 KB, `mm_define` 33 KB, `mm_snapshot` 21 KB and
`Compiler::stmt_inner` 24 KB.

**4. What the stdlib bytecode costs** [M]. Data is 559 KB of `hello.wasm`;
the 63 default modules are 389,795 B packed (776,204 unpacked, 3,285 code
objects), the prelude 61,687, the printable table 2,982, the rest message
strings and core's float tables. Largest: `decimal` 39,414, `datetime`
26,652, `re` 22,215, `collections` 14,652, `statistics` 13,905, `enum`
11,497, `urllib.parse` 11,477, `dataclasses` 11,021, `logging` 10,521,
`time` 10,318. The packing is near what per-module general compressors
reach (zlib -9 388 KB, zstd -19 374 KB, xz -9e 346 KB per module), so
only structure helps, none implemented:
- one solid stream: 360,510 with this codec (−29 KB), xz -9e 283,168
  (−107 KB); but importing one module would then unpack all 776 KB. A
  middle way is chunks of related modules (as the prelude does), paying
  for the neighbours on first import;
- a stronger entropy coder (LZMA-style range coder with context models): up
  to about −43 KB per module (the xz figure) for a few KB of decoder [E];
  a 64 KB trained zstd dictionary does not pay (297,678 + 65,536);
- a denser bytecode encoding before packing (the 32-bit instruction words
  dominate the unpacked bytes) or dropping unused names: unmeasured;
- moving rarely imported modules (`decimal`, `statistics`, `calendar`) to
  opt-in: an owner decision against S1/T1, −40 KB for `decimal` alone.

## Aware datetime (2026-11-05, owner decision 40)

Aware `datetime` comparison, hashing, arithmetic, `utcoffset` and
`astimezone` follow CPython 3.14.7's C paths (`datetime_richcompare`,
`datetime_hash`, `add_datetime_timedelta`, `datetime_subtract`,
`tzinfo_fromutc`) in the Python `datetime` instead of building timedeltas
through the constructors: same-tzinfo comparisons compare field tuples,
the rest compare UTC keys `(days, seconds, us)` from fields, `timezone`'s
offset is read directly, and `timedelta`s come from a constructor-free
helper. Two natives do the calendar math, `_mm.dt_ord` and `_mm.dt_ymd`
(spec §12's `datetime` exception). Python `tzinfo` subclasses stay
ordinary calls, so no native code ever runs user code. `zoneinfo` drops
its `bisect` import for a local bisect and reads fields with `dt_ord`.

Per operation, Ir net of an empty loop [M] (CPython 3.14.7 / base 45f454b /
new):

| Operation | CPython | base | new |
|---|---:|---:|---:|
| `a < b`, different fixed offsets | 6,497 | 1,470K | 42K |
| `a == b`, different fixed offsets | 6,409 | 2,356K | 44K |
| `a == b`, zoneinfo vs `timezone.utc` | 6,820 | 2,391K | 55K |
| `aware == naive` | 1,799 | 852K | 11.5K |
| `a < b`, same tzinfo | 380 | 12.7K | 7.6K |
| `a < b`, Python `tzinfo` subclass | 21.4K | 1,573K | 75K |
| `hash(a)` | 7,839 | 567K | 54K |
| `a - b`, aware | 3,651 | 865K | 27K |
| `a + timedelta` | 777 | 452K | 22.5K |
| `a.utcoffset()` | 1,692 | 293K | 8.4K |
| `a.astimezone(timezone(...))` | 3,269 | 1,171K | 63K |
| `a.astimezone(ZoneInfo(...))` | 3,610 | 1,352K | 131K |
| `datetime(..., tzinfo=tz)` | 4,307 | 86K | 33K |

Codemode `dt-*` cells (148 solutions, `timing.py --filter dt- --jobs 4`)
[M]: engine total 1.387 → 0.749 s, p50 5.6 → 3.8 ms, p90 27.5 → 14.5,
max 49.1 → 22.3; `dt-overlaps` 34–49 → 2.7–8.3 ms (CPython 0.5–0.9).
The slowest left are `dt-local-times`, `dt-outside-hours` and
`dt-iso-week-revenue` at 15–22 ms, dominated by imports (`import zoneinfo`
71M Ir, of which `calendar` 44M; `import datetime` 14M [M]) and Python
formatting: per aware call `isoformat` 56K Ir (CPython 11K), `str` 64K,
`strftime('%Y-%m-%d %H:%M')` 225K (CPython 18.5K) [M]. Bench programs:
every one within 0.01% Ir of 45f454b and of d6d77f6 [M]. Size on 77f20ed
+4,080 / +4,240 / +4,069 B (`hello.wasm` / native / guest) [M], to
2,171,955 / 2,286,168 / 2,322,247 under 2,173,900 / 2,288,100 / 2,324,200
(the same within 2 B on 45f454b and d6d77f6); the snapshot format is
unchanged.

Checked by own case 381 (aware/naive mixes, fold with a Python tzinfo,
New York and London 2026 transitions, tzinfos returning None, raising,
returning a non-timedelta, ±24 h or a timedelta subclass, a datetime
subclass, `fromutc`) and `conformance/fuzz/datetime_aware.py`, which runs
3,000 random cases per seed on CPython and mm and requires equal output
(seeds 1–4 equal; base main differed on about 1,000 lines per seed).
Brought to CPython's behaviour on the way: `repr` puts `fold` before
`tzinfo`; C's messages for comparing and subtracting naive and aware
datetimes and times, for `fromutc` and for the base `tzinfo` methods; and
`astimezone` on a datetime whose tzinfo returns `None` treats it as naive.
`hash` values are consistent with `==` but are not CPython's (CPython's
depend on the hash seed anyway).

Open: the `zoneinfo`/`calendar` import cost, Python `isoformat`/`strftime`,
and `astimezone(ZoneInfo)` at 36× CPython (the transition search is
Python).

## asyncio primitives (2026-11-05)

Owner-approved batch (size under S1). Ported from CPython 3.14.7 into the
prelude: `Lock`, `Event`, `Condition`, `BoundedSemaphore` (and
`Semaphore`), `Queue`/`LifoQueue`/`PriorityQueue` with `QueueEmpty`,
`QueueFull`, `QueueShutDown` and `shutdown()`, `TaskGroup`, `timeout`/
`timeout_at`/`Timeout` (`reschedule`, `when`, `expired`), `shield`,
`wait` (all three modes, `timeout=`), `current_task`, `all_tasks`,
`ensure_future`, `isfuture`, `get_running_loop`/`get_event_loop` (one
loop, `asyncio.EventLoop`'s class: `time`, `create_task`, `create_future`,
`call_soon`/`call_later`/`call_at` with cancellable `Handle`/
`TimerHandle`, `call_exception_handler`), task names (`name=`,
`get_name`/`set_name`, numbered as CPython counts: named tasks take no
number, each `asyncio.run` takes three), `get_coro`/`get_loop`,
`add_done_callback`/`remove_done_callback`, `Future.set_result`/
`set_exception`, Task/Future reprs with reprlib's limits, and the task
name in the `asyncio.run` shutdown report. `ExceptionGroup`/
`BaseExceptionGroup` are builtins (`split`, `subgroup`, `derive`); an
uncaught group and `traceback.format_exception` draw CPython's nested
boxes; tracebacks name `asyncio.exceptions.CancelledError`;
`traceback.format_exception_only` now calls a class's own `__str__` (it
printed `<exception str() failed>`). Streams, subprocesses and threads
(`open_connection`, `to_thread`, `run_in_executor`, ...) raise with the
`Network`/`Processes` hints.

Engine pieces (spec §8.5): each task, future and gather keeps an ordered
waiter list (a gather, a task's wakeup, or a done callback), woken in
order on settle, which gives FIFO release and callback order; callbacks
run in handle-less task slots through `_aio_callback` from the slot's
bottom frame (`Ready::Call`, snapshot tag 2), so no nested dispatch loop
runs and a cell parks inside a callback; timers (`call_later`, the
`wait_for`/`as_completed` deadlines) are internal tasks left out of
`all_tasks()`. Task numbers, names, callback slots and loop flags are
snapshotted.

Checks [M]: own cases 381 (primitives, every line CPython 3.14.7's) and
382 (exception-group reports); `cargo xtask durability --filter asyncio`
10 of 10 runs identical across 2,736 parks, 382 2 of 2 across 937 (the
full gate on d6d77f6: 1,202 of 1,202 across 114,056 parks, cross-target
388 of 388 across 10,734); a
host-call test parks and restores in the middle of a Lock wait, a
full-Queue put and a TaskGroup teardown, with the abandoned calls in
`call_log()`. A review subagent's 25 cancellation probes found no crash
or hang; its Task-repr and TimerHandle-report findings are fixed.

CPython `Lib/test` methods (the same 18 units as asyncio cancellation; 773
methods, 538 pass on CPython) [M]: 142 → 283 on mm, no regressions:
`test_queues` 0 → 59 of 59, `test_timeouts` 0 → 34 of 34, `test_waitfor`
10 → 19 of 19, `test_locks` 12 → 49 of 72 (22 of the rest need
`Barrier`), `test_taskgroups` 0 → 2 of 84 (the unit imports
`from asyncio import taskgroups`; with that line rewritten 70 of 84, the
rest `except*` (12) and `create_task(context=)` (2)).

Bench Ir vs d6d77f6 [M] (callgrind, `mm run`, equal-length paths): every
program within ±0.2%: startup +0.193% (the larger prelude), json −0.020%,
exceptions −0.016%, interp −0.011%, the rest within ±0.001%. Size vs
d6d77f6 [M]: `hello.wasm` 2,131,850 → 2,167,875 (+36,025), native `hello`
2,241,768 → 2,281,928 (+40,160), guest 2,282,010 → 2,318,178 (+36,168);
budgets 2,169,800, 2,283,800 and 2,320,100 (measured plus about 1.9 KB).

Still differs: `except*` is a SyntaxError; `asyncio.taskgroups`-style
submodule imports fail; no per-task `contextvars` (`context=`); no
`Barrier`, `Runner`, policies or `new_event_loop`; functions have no
`__code__`, so task reprs lack `coro=<...>` and callback/shutdown reports
lack "at file:line"; "Task/Future exception was never retrieved" is never
printed; `loop.time()` and `TimerHandle` deadlines are virtual time;
leftover tasks at `asyncio.run`'s end and a failing TaskGroup's other
tasks are cancelled in creation order (CPython walks a set); a
`KeyboardInterrupt`/`SystemExit` raised in a task is stored like other
exceptions instead of escaping the loop, and `SystemExit` has no `.code`
(both older than this batch).

## Review follow-ups (2026-11-05)

Owner-approved items 2–4 of the review below (item 1, asyncio
cancellation, landed separately in e6d7251), plus the bool-format fix from
the speed thread. Each fix has a test that fails without it.

- **Big-int builtins park before starting.** A builtin a `CALL` runs
  directly (`Vm::direct_call`, taken by `call_native`) estimates its charge
  first (`Vm::native_short`): `pow` with two or three arguments, `divmod`,
  `sum` and `math.prod` of a list or tuple, `math.factorial`/`perm`/`comb`/
  `isqrt`/`gcd`/`lcm`; one of at least 2^16 units the fuel left cannot pay
  parks with `fuel_wanted()` set, as the operators do. A builtin called by a
  native (`map(pow, …)`) still charges after. `math.comb` now also charges
  its final division. Audit: `operator.mul`/`pow`, int dunders and
  `Fraction`/`statistics` go through the operators; `str`/`int` text
  conversion has the 4300-digit limit; `<<`, `hex`, `to_bytes`,
  `from_bytes` are linear [M].
- **Operators inside functions did not pre-park after a fuel park.**
  `resume` set `afford` after every park, so the first big operator after an
  ordinary fuel park (typically the first in a called function) ran
  unchecked; `afford` is now set only when the park was a pre-start one
  (`an_operator_after_an_ordinary_fuel_park_still_parks_before_starting`).
- **`{**m}` and `f(**m)` accept any mapping** (`keys()` and `__getitem__`;
  `_op_dict_update`), with CPython's messages; a dict subclass keeping
  dict's iteration is copied directly, as CPython does (own case 381).
- **View membership hashes.** `x in d.keys()` and `(k, v) in d.items()` in
  the twin look the key up by hash (`_dict_probe`/`_dict_value_at` read a
  view's dict) instead of comparing every key with `==` (own case 381).
- **Host conversion charges before copying** a list, tuple, dict or record's
  items (`host_open`, `dict_live_len`, `record_parts`/`record_copy`);
  `tests/host_conversion_charges.rs` (Linux, peak RSS) saw 16–32 MB copied
  before a refusal, now none [M].
- **`.cwasm` cache keyed on content**: `<stem>.<sha256>.mmw.cwasm`, the key
  (module bytes and wasmtime's `precompile_compatibility_hash`) also heads
  the file; the cache directory must be writable only by the host. Old cache
  files are not removed.
- **gc-stress durability** runs every own case with a per-process limit of
  3,600 s (it was 300 s; 310, 324 and 331 take 449 s, 517 s and 1,598 s
  unparked under stress [M]). `cargo xtask gc-stress` on 8873a0b (before
  the rebase onto the speed landing) [M]: own cases identical (1,577 s),
  corpus matches (293 s), durability 376 of 376 runs identical with 73,875
  parks (3,162 s); 5,032 s in all.
- **bool formats as int with a non-empty spec.** `format(True, '5')` gave
  `'True '` (a bool branch in `Vm::format` since the M2 core); CPython
  uses `int.__format__` whenever the spec is non-empty (`'    1'`), and an
  empty spec gives `'True'`. `(1).__format__('5')` and `True.__format__('5')`
  resolved to `object.__format__` and raised `TypeError`; `object_format`
  now formats int, bool, float, complex and str values as `format()` does
  (own case 382).

Measured [M]: bench Ir vs 1fe8e2d (callgrind, `mm run`, stripped binaries at
equal-length paths; outputs identical): callgrind +0.126%, classes −0.102%,
closure +0.001%, dict +0.087%, exceptions −0.047%, fib +0.002%, fmt +0.027%,
generators −0.208%, interp +0.206%, json_rows +0.063%, loop +0.000%,
opcost +0.100%, sort −0.054%, startup +0.551%, str +0.265%. Startup's
+4,823 Ir is code layout: built with symbols at equal-length paths it is
877,372 → 875,182 (−0.25%). A first version set `Vm::direct_call` around
the dispatch loop's calls and moved loop +1.6%, classes +1.1%, fib −2.4%;
the flag is now set in `call_value`'s builtin branch and the method fast
path only. Size vs 1fe8e2d [M]: `hello.wasm` 2,127,031 → 2,131,850 (+4,819),
native `hello` 2,236,616 → 2,241,768 (+5,152), guest 2,277,201 → 2,282,010
(+4,809); budgets 2,133,800, 2,243,700 and 2,283,900 (measured plus about
1.9 KB).

## Speed landing (2026-11-05, owner decisions 35–37)

From docs/research/interpreter-speed.md (§5 there has the item status).
Each commit was measured in order [M]: callgrind Ir in millions, release
builds without debuginfo, this orb (Intel Xeon @ 2.60 GHz); base is
926596b (engine as 9306b80).

| Program | base | + generators | + `Ic::New` | + str/format | + stack temps | + sort keys | final / base | final / CPython |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| fib | 317.2 | 318.1 | 342.9 | 340.1 | 321.6 | 321.6 | 1.014 | 0.98 |
| loop | 1,166.4 | 1,166.4 | 1,247.4 | 1,244.5 | 1,115.4 | 1,115.4 | 0.96 | 0.40 |
| closure | 721.1 | 721.1 | 762.1 | 751.1 | 715.0 | 715.0 | 0.99 | 0.64 |
| dict | 1,168.2 | 1,168.8 | 1,196.4 | 1,190.8 | 1,160.2 | 1,160.2 | 0.99 | 1.23 |
| str | 492.7 | 492.8 | 498.2 | 497.7 | 492.1 | 492.1 | 1.00 | 1.51 |
| json | 1,140.3 | 1,139.5 | 1,149.1 | 1,147.7 | 1,131.1 | 1,131.0 | 0.99 | 0.81 |
| classes | 1,745.2 | 1,741.6 | 1,482.0 | 1,491.3 | 1,385.1 | 1,385.1 | 0.79 | 1.31 |
| generators | 3,368.0 | 2,397.2 | 2,464.8 | 2,437.0 | 2,387.1 | 2,387.1 | 0.71 | 2.25 |
| exceptions | 990.9 | 989.7 | 1,018.5 | 1,015.5 | 992.0 | 992.0 | 1.00 | 1.26 |
| fmt | 1,892.5 | 1,892.1 | 1,904.9 | 1,598.7 | 1,561.5 | 1,561.5 | 0.83 | 1.65 |
| sort | 1,569.8 | 1,568.1 | 1,581.5 | 1,579.1 | 1,550.3 | 1,454.0 | 0.93 | 1.63 |
| interp | 2,744.1 | 2,744.1 | 2,646.0 | 2,362.9 | 2,273.3 | 2,273.3 | 0.83 | 2.29 |

The native `datetime` commit changes no bench program (within 0.01%).
Wall time [M], this orb, `taskset -c 2`, median of 11 (ms, base → final,
CPython): classes 152.6 → 111.0 (CPython 100.1), generators 279.6 →
210.8 (91.2), fmt 254.8 → 196.1 (111.3), sort 187.8 → 166.2 (135.0),
interp 274.2 → 224.3 (98.2), loop 75.8 → 71.5; fib +2% (interleaved);
dict, str, json and exceptions within ±3%. **closure is +6–8% wall** (48.9 → 51.9 ms,
interleaved, 25 runs) although its Ir is −0.8% and cachegrind's simulated
I1/D1 misses and mispredicts are no worse: a code-layout effect of the
dispatch loop on this CPU, not reproduced in Ir. The dispatch loop's
register allocation also explains the intermediate rows: `Ic::New` alone
cost fib +8% and loop +7% Ir until the stack-temporaries commit; moving
the `BUILD_LIST`/`BUILD_TUPLE` copy out of line made loop +9%, and a
debuginfo build of the final tree gives fib −3.9%. fib's +1.4% Ir is
reported, not fixed. Bare-metal reruns (T-01a1aa37's machines) would show
whether closure's wall regression is this VM's.

Sizes [M] (`hello.wasm` / native `hello` / guest): generators +3,065 /
+6,176 / +3,120, `Ic::New` +1,718 / +1,120 / +1,719, str and format
+1,415 / +1,744 / +1,418, stack temporaries +1,032 / +800 / +1,028, sort
keys +1,604 / +1,648 / +1,604, `datetime` parsers +13,821 / +18,112 /
+13,820 (`fromisoformat` +4,668 and `strptime` +9,153 of `hello.wasm`),
measured on 9306b80; on top of asyncio cancellation (e6d7251) the total
is +22,603 / +29,072 / +22,712, to 2,127,020 / 2,236,600 / 2,277,190;
budgets 2,128,900 / 2,238,500 / 2,279,100. The snapshot format is unchanged; 32 inline instance slots
(was 16) change where an instance's 17th to 32nd attributes live, which
same-build snapshots never cross.

**`datetime`.** `_mm.dt_iso` ports CPython 3.14.7's
`Modules/_datetimemodule.c` parsers (v3.14.7, not v3.14.0: the fraction
and hour-24 checks changed between them) and `_mm.dt_strptime` matches
`%Y %m %d %H %M %S %f %z %y %b %B %a %A %p %I %j %%` with the regex's
backtracking order, returning `None` for anything else so `_strptime.py`
runs. Per call, net of import [M]: `fromisoformat` 191K → 19.8K Ir with
`+00:00` (CPython 1.9K) and 425K → 34K with `-08:00` (CPython 2.4K);
`datetime.strptime` 423K → 91K (CPython 163K). Checked against CPython on
20,000 seeded random ISO strings (all equal; main differed on 594 of a
first 3,000)
and 20,000 strptime cases (equal to main and to CPython except main's
regex error positions, below); own cases 380 (parsers) and 379 (the fast
paths, `Ic::New` invalidation). Fixed on the way, to CPython's behaviour:
`'2026-03-09T'` and `'2026-03-09 '` are invalid, offsets of 24 h or more,
out-of-range fields and non-ASCII digits get CPython's messages, `+HH:MM`
past 24 h no longer reads as an invalid string, `date.fromisoformat`'s
`TypeError` text, and `strptime()`'s argument-type `TypeError`s.

**Codemode** (`conformance/codemode/timing.py`, 1,707 solutions, base →
final [M], 8 jobs, noisy): engine p50 1.36 → 1.36 ms, p90 14.7 → 13.7,
p99 32.9 → 32.2, max 122.5 → 112.0; total engine 7.98 → 7.65 s. The
`shlex` cells (`logs-5xx-paths`) 121.6 → 86.6 ms (CPython 15–19 ms);
`dt-overlaps` 23–50 → 23–36 ms (CPython 0.6–0.9 ms); all 148 `dt-*`
cells 1.353 → 1.232 s. What is left in the `dt-*` cells is Python
`datetime`: an aware comparison is 13K Ir (CPython 0.8K) [M], the
`fromisoformat` call 19.8K, most of it the Python around the parser
[E], `astimezone` and the `zoneinfo` import.

Open: item 4's remaining heap temporaries (`pad_number`'s `NativeText`,
`NativeVec` reserves in fmt, `parse_int_text` and `getitem` in interp,
`getattr`, `call_class` and `exc_init` in exceptions); items 6, 7, 9 (byte
APIs, no `unsafe`) and 10; item 5 deferred; the closure wall time above.
Found, not fixed: `format(True, '5')` gives `'True '` where CPython gives
`'    1'` (main too); `re.error` positions for a duplicated `strptime`
directive differ from CPython's (main too).

## asyncio cancellation as CPython 3.14 (2026-11-05)

Owner-approved. The three differences the review left open are gone:

- **Propagation.** `Task.cancel(msg)` goes to what the task waits on, as
  `_fut_waiter.cancel()` does: another task (recursively), a gather (each
  child; `_cancel_requested` makes even `return_exceptions=True` raise), a
  loop or host future, a sleep, the first-done waiter of
  `wait_for`/`as_completed`. One iterative walk (`Vm::cancel_tree`,
  depth-limited, charged one fuel unit per awaitable); when nothing takes
  it the task keeps `_must_cancel` and raises `CancelledError(msg)` at its
  next step, or throws it into the `__await__` iterator it steps.
  Awaitables gained a `CANCELLED` state; the message lives in the result
  slot, and a task ended by a `CancelledError` keeps that exception for the
  first retrieval (`_make_cancelled_error`), so object sizes are unchanged.
  A cancelled host future marks its call `Abandoned` at once; a host that
  answers it anyway is accepted and ignored (`FutureStatus::Cancelled`).
- **`asyncio.run`** is prelude Python (`_aio_main`): the main coroutine
  runs as its own task, the scheduler counts loop iterations as
  `_run_once` does (`iter_left`/`iter_open`, snapshotted), the loop stops
  one iteration after main ends, the remaining tasks are cancelled and
  gathered, and errors other than `CancelledError` are reported as
  "unhandled exception during asyncio.run() shutdown".
- **`cancel()` of the running task or one that never ran** returns `True`,
  counts in `cancelling()`, and raises at the next step (a never-run task
  is closed unrun, so it is not "never awaited"); `uncancel()` exists.
  `gather` results report cancelled children as `CancelledError('')` or
  with their message; the gathering future has `done`/`result`/`exception`/
  `cancel`/`cancelled`. `wait_for` is `timeouts.py`'s: it uncancels after
  its own timeout and leaves an outer cancel a `CancelledError`.

Own case 378 (CPython 3.14.7 output, `cargo xtask durability --filter
asyncio`: 8 of 8 runs identical across 2,001 parks [M]); host-call tests
for an abandoned call cancelled mid-wait and for `asyncio.run` waiting on a
cancelled task's cleanup host call through a snapshot.

CPython `Lib/test` methods (harness of `conformance/breadth/cpython-tests`
with 15 `test_asyncio` units added, plus `test_coroutines`, `test_asyncgen`
and `test_contextlib_async`; 773 methods, 538 pass on CPython) [M]: 139 →
142 on mm, no regressions (`test_waitfor`: `test_wait_for`,
`test_wait_for_reraises_exception_during_cancellation`,
`test_wait_for_waits_for_task_cancellation`). The other failures in these
units are missing APIs (`get_running_loop`, `shield`, `wait`, `Lock`,
`Queue`, `TaskGroup`, `timeout`, `new_event_loop`), not cancellation.

Bench Ir vs 9306b80 [M] (callgrind, `mm run`, equal-length paths): every
program within ±0.011% (exceptions +0.010%, json +0.008%, interp −0.010%,
startup +0.011%). Size vs 9306b80 [M]: `hello.wasm`
2,091,415 → 2,104,417 (+13,002), native `hello` 2,193,048 → 2,207,528
(+14,480), guest 2,240,993 → 2,254,478 (+13,485); budgets 2,106,300,
2,209,400 and 2,256,400 (measured plus about 1.9 KB).

Still differs: at the end of `asyncio.run` tasks are cancelled in creation
order (CPython iterates a set); the shutdown report has no task name or
coroutine and no asyncio-internal traceback frames; task names, callbacks
and `shield` do not exist.

## Review and hygiene pass (2026-11-05, 32125ff..5568b27)

An independent review of the day's commits by area (walk and fuel, host
API, asyncio, stdlib round, mm-wasm, docs), each confirmed bug with a test
that fails on 5568b27. Fixed:

- **Engine.** Subscripts on a dict subclass with a key whose `__hash__` is
  Python code recursed until `RecursionError` (`Counter` of Fractions, own
  case 374; the twins now use `dict.get`/`__setitem__`/`__delitem__` and
  `__missing__`). `fuel_wanted` was lost across a snapshot (now saved);
  `1 ** n`, `0 ** n`, `(-1) ** n` asked for absurd fuel (`op_cost`).
  Identical sequence items cost no fuel in the comparison walk and
  `_seq_scan` (now one visit per 64), subclass operands and dict views vs
  sets compared wrong in the twins (own case 377). Failed module and
  instance attribute lookups (also `hasattr`) copied every name for the
  hint with no fuel (`Vm::hint_names` counts first and charges).
  `_mm.tzif` passed guest keys to the host unvalidated.
- **asyncio** (own case 376, sessions tests): `_mm.aio_set`/`aio_first`
  accepted any object (a list could be emptied, a generator's snapshot
  corrupted); `as_completed` used quadratic memory; cancel was quadratic
  and uncharged; `create_task(asyncio.sleep())` was refused. The
  never-awaited list (`Vm::fresh`) was a GC root, so creating many
  unawaited coroutines ended in `MemoryError` (a regression from 89e945b):
  it is weak now (`Heap::collect_weak`, `Tracer::marked`), keeps the
  qualname at creation, dedups names before copying, and scheduled
  coroutines are not reported.
- **Host output.** `TextCut::HeadTail` left dict keys and record names
  uncut, and rendered fields for a namedtuple subclass with its own
  `__repr__` (the shaped cut is used only when it agrees with the real
  `repr` up to its first marker).
- **Stdlib** (own case 375): statistics on Decimals, Fraction as
  `numbers.Rational` (Decimal/Fraction comparisons), `global_enum` flag
  reprs, `adler32` start values, `HTMLCalendar.formatyearpage()`,
  `BytesIO.seek` argument types, hmac on bytearrays, `{} | globals()` and
  `reversed(globals())`.
- **mm-wasm.** The reference host trapped an instance on a park request
  made between calls (grace now counts from the call's start); the codec
  refused kwargs and lenient `TooDeep` markers the engine sends at the
  depth limit, and nested counts multiplied preallocation (capped at
  1,024 items per container).
- **Docs.** Plak count 45 of 48, size tables, the spec API block, README
  conformance count, AGENTS layout and the twelve `check` gates, sdk.md
  struct literals, hint and never-awaited text, new Limitations
  (big-int builtins do not pre-park, NaN identity, comparison depth 1000,
  json hooks after parsing, `help()`, asyncio cancellation), budget
  comment arithmetic, stale "F4 open" and "no never-awaited warning".
  Erratum: 1c500d8's message gives the pre-rebase budgets (2,052,500 /
  2,144,500); the commit set 2,082,700 / 2,181,500. Hygiene: removed the
  now-default modules from the extra features of `mm-cli` and the
  codemode runner.

Measured [M]: bench Ir vs 5568b27 (callgrind, `mm run`, binaries at
equal-length paths; outputs identical): classes +0.002%, closure −0.001%,
dict +0.090%, exceptions +0.002%, fib −0.001%, fmt +0.027%, generators
−0.001%, interp +0.125%, json_rows +0.078%, loop −0.000%, sort +0.128%,
startup −0.492%, str +0.172%. Size vs 5568b27: `hello.wasm` 2,081,288 →
2,091,415 (+10,127), native `hello` 2,180,120 → 2,193,048 (+12,928), guest
2,230,840 → 2,240,993 (+10,153); budgets 2,093,700, 2,195,300 and
2,243,300 (measured plus about 1.9 KB).

Other checks [M]: plak 45 of 48 (docs/plak-integration.md); all seven fuzz
targets 5 min each from empty corpora, no crash (one nested_data slow unit,
the known 2^8-path frozenset DAG, 0.08 s in release); `cargo xtask
gc-stress` on 5568b27: own cases identical (1,579 s), corpus matches, and
durability 368 of 374 identical, the 6 others timeouts (300 s) of cases
310, 324 and 331, whose deep comparisons do a full collection at every
safepoint (331 alone takes tens of minutes under stress; it postdates the
last recorded gc-stress run). Fuel totals differ under stress (tick
period 1; `Session::fuel()` lags by up to 255 unchecked safepoints and a
native `burn` saturates at the slice end), not output.

Open (not fixed, design-sized or outside the range): the three asyncio
cancellation differences (fixed since; see "asyncio cancellation as
CPython 3.14" above); `create_task`
with no loop running (top-level code without `await`) runs the task in the
next `asyncio.run` instead of raising `RuntimeError` (a first fix broke
`create_task` under top-level `await`, caught by the codemode check;
reverted, with a session test); `dict.__getitem__(sub, k)` ignores
`__missing__`; `StringIO.seek` accepts floats. The big-int builtins, `{**m}`,
view containment, host conversion charges, the `.cwasm` cache and
gc-stress durability are fixed ("Review follow-ups" above).

## Codemode host API (2026-11-05)

Owner-approved items A2, B2 and the message/host-API list of the codemode
checkpoint; scope decisions 5 and 6 below. API added (facade, docs/sdk.md
"Host errors", "Typed host functions", "Messages for models"):

- `HostSignature::dict_as_kwargs: bool` (A2; `HostSignature` is now
  `Default`): one positional dict binds as the keyword arguments, then the
  usual annotation checks; off by default.
- `HostError { kind, message, data: Option<HostValue>, class: Option<String> }`,
  `HostError::new(kind, message)`, `HostModule::exceptions:
  Vec<HostException { name, base: HostErrorKind, doc }>` (B2). The class is
  generated Python (`_host_make`), so `except ToolError`, `.tool`, `.data`
  and snapshot survival come from the heap; `Vm::host_exception` sets the
  attributes in the exception's dict.
- `HintGroup` (12 groups) and `Session::set_hint(group, Option<&str>)`:
  CPython's message plus one sentence in the report (exception hint slot,
  not `str(e)`), overrides in the prelude globals (`_hints`), so they
  survive reset and snapshots. `mm_stdlib::ALL_MODULES` lets
  `NotEnabled` name modules a build leaves out. OS hooks (`open`, `os`,
  `tempfile`) end with the `OsHooks` sentence instead of "the host must
  provide ..."; the first sentence is unchanged.
- "Did you mean" on module attributes and `from m import x`
  (`Vm::module_attr_hint`, CPython's `suggest`).
- `Engine::environment() -> String`, generated from `MODULES`, `BUNDLED`,
  `ALL_MODULES`, the asyncio natives and the limits.
- `Session::call_log() -> Vec<HostCallRecord { call, function, name,
  status: CallStatus }>`; `CallStatus::{Returned, Raised, Pending,
  Abandoned}`. `Vm::call_log` is execution state charged to the budget and
  saved in snapshots; `start` clears it, `end_cell` abandons what is
  pending.
- Never-awaited warning: `Vm::fresh` remembers coroutines until they start
  (pruned on growth); `finish_cell` (complete or uncaught error, not
  cancel) scans it and the unawaited host futures (`aw::AWAITED`) and
  writes `<sys>:0: RuntimeWarning: coroutine 'x' was never awaited` once
  per name. Only user code counts (`GSLOT_USER`); gather's already-wrapped
  items are dropped on its TypeError, as CPython wraps them first.
- `TextCut::{Head, HeadTail}` with `Session::set_text_cut`, and
  `micro_monty::head_tail(text, max_bytes)`; containers keep their shape
  (`render.rs`), `Completion::value` stays whole. Host-side setting.
- `__doc__`/`__module__` on raw host functions (from the definition's help
  entries), `__module__` on typed ones, `help()` in the prelude printing
  the stub (`_mm.host_help`), `dir(module)` unchanged (already listed
  attributes).
- Docs: structured results over JSON strings, `TimeoutError` conventions
  (host call: `HostErrorKind::TimeoutError`; cell: deadline park, then the
  host decides).

Fixed on the way: a host module whose classes have no methods (or only
exceptions) now runs its generated program (it was skipped when no
callable needed wrapping).

Tests: `tests/host_defs.rs` (`dict_as_kwargs_...`,
`host_functions_have_doc_name_module_dir_and_help`), `tests/host_calls.rs`
(`host_errors_carry_a_payload_...across_snapshots`,
`the_call_log_reports_...`), `tests/sessions.rs` (hints, environment,
head-and-tail), `render.rs` unit tests, own cases 370 (Did you mean,
CPython's import text) and 371 (never awaited), both equal to CPython.
`python3.14 conformance/codemode/run.py --check`: 1,793 solutions
unchanged [M], so no results were regenerated.

- **Size** [M]: `hello.wasm` 1,922,566 → 1,952,776 (+30,210), native
  `hello` 2,012,712 → 2,049,784 (+37,072). Ablations on `hello.wasm`:
  redirect hints and "Did you mean" +9,177, head-and-tail/container cut
  +9,108, help/`__doc__`/`__module__` +3,448, never-awaited scan +3,384,
  rest (B2, call log, snapshot fields) about +5,100 by subtraction.
  `dict_as_kwargs`, exception classes and `environment()` are facade code
  `hello` does not link (it defines no host module). Replacing a std
  `sort_by_key` with `heap_sort_by` saved 3,973 B. Budgets raised to
  1,954,700 and 2,051,700 (measured plus about 1.9 KB) under S1.
- **Bench Ir** vs ac10b28 [M] (callgrind, `mm run`, release; outputs
  identical): startup −0.75%, fib −0.00%, loop −0.00%, dict +0.03%, str
  +0.04%, json_rows +0.05%, closure −0.00%, classes +0.06%, generators
  −0.19%, exceptions +0.08%, fmt +0.74%, sort +0.05%, interp +0.03%. fmt
  is outside ±0.5%: the binary's own instructions differ by +0.05%
  (1,566,843,587 → 1,567,614,204); the rest is glibc `_int_free` and
  `_int_malloc` with identical `malloc`/`free` call counts, i.e. allocator
  free-list state, not engine work. Startup −0.75% is the improvement side.

Owner decisions (2026-11-05, "Scope decisions" 30–32): the 9 KB container
renderer (larger than the 1–2 KB estimate) is accepted; hosts choose their
own deadline-to-`TimeoutError` reporting, with no raising deadline mode;
fmt +0.74% is allocator noise. `mm-cell-worker` exposes none of this
(test-only harness, by rule).

## Embedding hardening: arena-stack comparisons, host-value charges, fuel before big ints (2026-11-05)

Owner-approved, for in-process hosting (plak's runner); requested with the
isolation study's findings (`docs/research/isolation-modes.md`).

- **F4 traced and fixed** (`Vm::dv` refuses non-dicts; `DICT_INSERT` into
  a non-dict raises `SystemError`): mutated bytecode jumped into a dict
  comprehension body with a list in the insert register; the list's one
  slot read as an index-less linear table, so every store scanned all
  entries without fuel. The artifact: 419 ms → 1.8 ms [M]. See
  `conformance/breadth/fuzz/README.md`.
- **Comparison walk on an explicit stack** (`crates/mm-engine/src/walk.rs`:
  `eq_hard`, `order_pure`, `set_issubset`): eight frames inline, the rest in
  a `NativeVec` charged to the arena limit; F1's semantics kept (depth
  across prelude handoffs, `max(WALK >> d, 1)` re-walks, `RecursionError` at
  the limit). Minimum `ulimit -s` for `mm run` [M]: cyclic dict `==` 519 →
  35 KiB, 600-deep frozenset `==` 439 → 39 KiB, deep list `==` 227 → 35 KiB,
  deep sort 183 → 35 KiB. The 16 MiB-thread test workaround is gone; tests
  run these shapes on 256 KiB threads, and `cargo xtask wasm` runs them under
  wasmtime's default stack (`examples/roundtrip.rs`). 1,500 generated
  comparison programs give identical output and fuel to the base [M].
- **Host-value conversion charged and iterative**: argument and completion
  trees reserve from `Limits::memory` (MemoryError / `TooLarge` / no host
  value past it) and convert on an explicit stack. A 64 MiB session
  returning 3M ints: 265,388 → 101,508 KiB peak RSS [M]; 1000-deep host
  arguments convert on a 256 KiB thread (overflowed before [M]).
- **Long single operations**: int `*`, `**`, `/`, `//`, `%` estimate their
  charge (`op_cost`, the formula `big_binop`/`big_pow` charge); one of at
  least 2^16 units that the fuel left cannot pay parks with the instruction
  not started, `Session::fuel_wanted()` reports the cost, and `resume` runs
  it. Total fuel unchanged (`x = 3**3000000; x*x`: 3,309,227,389 both [M]);
  durable runs park before each such op. Options assessed: a size cap
  (already `MAX_BITS` = 2^36 and the memory check; a lower cap would break
  valid programs), chunking Karatsuba/division into resumable steps
  (needs resumable native state in snapshots; large and size-costly), and
  pre-charging (adopted: cold path only on non-small-int operands, no
  nested loop, Ir unchanged). Limitation: with fuel unlimited a deadline or
  park request still waits for the operation.
- **Startup**: the word space's first segment is 64 KiB and small segments
  are reserved without zeroing (filled lazily); full 1 MiB segments are still
  zeroed (lazily filling them cost str +2.0%, json_rows +1.0% [M]). Startup Ir
  [M]: 32125ff 958,807 → 870,417; 6eba295 was 878,811, so −0.96%, just below
  the ±0.5% band (the improvement side). Tried in order: lazy grow 930,688;
  + lazy new segment 906,365; + 64 KiB first segment 870,4xx. The F1 growth
  came from one doubling (4096 → 8192 units at top 4019) in `Session::new`.
  Memory floor (`Session::new` + `x = 1`) unchanged, 109,554 B [M].
- **Bench Ir vs 32125ff** [M]: classes −0.48%, closure +0.82%, dict −0.72%,
  exceptions −0.93%, fib −2.00%, fmt +0.04%, generators −1.37%, interp
  −0.52%, json_rows −0.02%, loop −0.01%, sort −11.64% (flat int/str fast
  paths in the walk), startup −9.22%, str −0.05%; outputs identical. Closure
  +0.82% is layout from `Heap`'s new scratch fields (`Arc<AtomicUsize>` +
  flag, needed for `&self` reservations); packing the flag into the counter
  was worse (+1.5–5%), reordering fields and `#[inline(never)]` on the walk
  entries had no effect.
- **Stack audit** (recursive SCCs from the LLVM call graph, frame sizes from
  the disassembly [M]): mm-core none. mm-engine 29 groups, each either one
  backing hop (`binop`/`binop_bx`, `getitem`/`getitem_bx`, `truthy`,
  `call_value` → `call_class`, `iter_next`/`get_iter`, `exc_str`, `err` →
  `new_str`, prelude loading, `find_module`, `len_of`, `gen_throw`,
  `unary`, `int_arg`, libm helpers), depth-bounded with small frames
  (`hash_depth` MAX_DEPTH 200 × 104 B; `py_value_depth` 100 × 216 B), or log
  depth (`mag_mul` Karatsuba); host-side `HostValue` clone/drop bounded by the
  1000-container ceiling. mm-parse and mm-compile recurse over syntax,
  bounded by `MAX_NESTING` (200) but with large frames (`statement` 2,488 B,
  `simple_stmts` 1,928, `closed_pattern` 1,560, `brace_inner` 1,112), so
  compiling at the limit needs 470–650 KiB in release (ifexp 622, slices
  602, subscripts 598, walrus 566, dicts 546, calls 542) and up to 3.7 MiB in
  debug; it fits wasmtime's default stack. Documented as a separate compile
  minimum (1 MiB) rather than shrunk: owner decision below.
- **Docs**: docs/sdk.md "Embedding guarantees" (panic=unwind for in-process
  hosts, stack minimums, allocation charges, fuel, each with its test); spec
  §5, §10.3, §11 point to it.

- **Size** [M], on top of gap batch 3: `hello.wasm` 1,904,638 → 1,922,566
  (+17,928); native `hello` 2,000,888 → 2,012,712 (+11,824). Split,
  measured before the rebase: walk and F4 +10,631 wasm / +11,248 native;
  host-value charging, iterative conversion and the fuel pre-check +7,534 /
  +1,248. Budgets raised to 1,924,500 and 2,014,500
  (measured plus about 1.9 KB, the gap-batch rule).

Owner decisions (2026-11-05, recorded in "Scope decisions" 13–16): closure
+0.82% and startup −0.96% accepted; the budget raise accepted; parser
frames not shrunk for now; a deadline alone does not stop a big int
operation that has started (documented).

## SDK round: one host surface, call ids, tuples, `__prepare__`, a driver

Owner-approved; each part is its own commit with `cargo xtask check` green.

- **`__prepare__`** on a metaclass raises `NotImplementedError: micro-monty
  does not support __prepare__` (own case 313, spec §4).
- **`HostValue::Tuple`**: a tuple crosses both ways as itself (snapshot tag
  15). `mm-cell-worker` still sends it as a JSON array, so plak's protocol
  is unchanged.
- **`Session::define(&HostModule)`** replaces the four `define_host_*`
  calls. `""` is the builtins; each define replaces the module whole; the
  engine keeps the definition (a `HostValue`) for `stubs()` and
  `host_modules()`.
- **Call ids**: `Progress::HostCall { function: u32, .. }`, stable per call
  name (`Vm::host_ids`, in snapshots), returned by `define`.
- **Driver**: `Session::run`/`drive` with a `&mut dyn HostHandler`, and
  `run_async`/`drive_async` with a `&mut dyn AsyncHostHandler` (boxed `std`
  futures), so neither needs a feature or a dependency. The async driver
  runs the cell in `ASYNC_SLICE_FUEL` (10,000) slices and returns `Pending`
  (self-woken) between them, so any executor stays responsive. The
  session's own budget is charged exactly: `Vm::fuel_account` counts the
  unchecked ticks, and `fuel_overdraft` keeps what a native `burn` charged
  past zero (without it, a sort crossing a slice was under-charged). A test
  interleaves two sessions on a one-thread executor.
- **`define`** returns `Vec<HostFunctionRef { name, id }>`.
- **Own case 200** no longer prints object addresses (it was flaky).
- Size [M], on `c46955a`: `hello.wasm` 1,737,688 (after `__prepare__` and
  `Tuple`) → 1,739,727 (+2,039) for `define` and ids, 1,727 over the
  1,738,000 ceiling. Per the owner's rule the ceiling is now 1,741,000
  (overshoot plus about 1.3 KB). Native `hello` 1,826,648 → 1,828,840.

## Restore without zeroing (2026-11-04)

`docs/spikes/base-image-deltas.md` measured 62% of restore in `calloc` and
`memset`: every restored segment was allocated zeroed at its full capacity,
then overwritten with the image.

- Segment storage is a `Vec` (`Store for Vec<u64>` / `Vec<u8>`). A restored
  segment reserves its capacity and holds only `[0, top)`: the image's runs
  are appended, and only the gaps between runs are zero-filled. Bump
  allocation past the initialized prefix fills it in 4 KiB steps
  (`fill_restored`, cold). Segments made while running are still zeroed at
  their capacity (fresh pages from the allocator cost nothing to zero).
- `Seg` stores its capacity as a `u32` and derives its cost from it, so
  it stays 64 bytes. At 80 bytes every program in the bench was about 1%
  slower [M]; the segment table is indexed on every heap access.
- `restore_is_the_collected_heap` now also allocates 30k objects past the
  restored `top` and compares both heaps' images. It fails when the fill
  is disabled.

Restoring two images 200 times each (callgrind, whole process) [M]:

| | Before | After |
|---|---:|---:|
| `Engine::restore`, inclusive | 416.8M | 305.6M (−27%) |
| `calloc` + `memset` | 300.0M | 44.0M |
| `check_objects` | 115.3M | 115.3M |

Wall clock, mean of 2,000 restores, three runs each [M]: a fresh session's
35 KB image 14.9–16.2 µs before, 14.7–18.1 µs after (unchanged; its
segments are small). A 196 KB image (a cell that loaded 548 dict rows)
83.0–89.4 µs before, 78.7–86.1 µs after; best of 2,000 77.5–82.4 → 69.2–73.1 µs.
The spike's −50% estimate [E] was optimistic. Most of what is left is
validation (`check_objects` 36%, the checksum 14%), which untrusted input
needs.

- Callgrind on the 13 bench programs is within ±0.3% of `7cf859d` [M].
- Size [M]: `hello.wasm` 1,818,778 → 1,821,388 (+2,610, under the
  1,823,000 ceiling); native `hello` 1,909,464 → 1,912,904 (under
  1,914,000).

## Wall-clock bench refresh (2026-11-04, after speed phase 4)

`cargo xtask bench --engines cpython,qjs,monty,micro-monty`, 10 reps, pinned
to CPU 2 on an idle 16-core orb, `mm` built at `7cf859d` [M]. The full table is in
`bench/results/summary.md`. QuickJS is dquickts @ `f9a3efa` (version
2026-06-04), built with `make qjs`; Monty is `pydantic-monty-runtime` 1.4.0
in a uv venv. Median ms, micro-monty as a ratio of each engine:

| Program | micro-monty | ÷ CPython 3.14 | ÷ QuickJS | ÷ Monty |
|---|---:|---:|---:|---:|
| startup | 3.4 | 0.25× | 1.03× | 0.81× |
| fib | 23.1 | 0.75× | 1.21× | 0.22× |
| loop | 59.6 | 0.35× | 0.89× | 0.24× |
| dict | 115.9 | 0.85× | 0.90× | 0.60× |
| str | 46.5 | 1.10× | 1.36× | 0.63× |
| json | 98.2 | 0.59× | 0.50× | 0.43× |
| closure | 39.2 | 0.53× | 1.06× | 0.22× |
| classes | 103.2 | 1.35× | 1.39× | 0.25× |
| generators | 223.7 | 3.03× | 2.08× | unsupported |
| exceptions | 80.4 | 1.25× | 0.53× | unsupported |
| fmt | 185.3 | 2.11× | 2.32× | 1.07× |
| sort | 160.0 | 1.54× | 0.73× | 0.43× |
| interp | 204.1 | 2.80× | 1.42× | 0.49× |

- Monty 1.4.0 has no `yield` or class inheritance; the runner now records
  such programs as `# unsupported <program>: <reason>` for qjs and Monty
  instead of aborting (CPython and micro-monty stay strict).
- Monty is beaten on every program it runs except `fmt` (1.07×; its
  min is 168.6 vs 167.6 ms). The ≤1.2× QuickJS target holds for loop and
  closure; fib is at 1.21×.
- The largest gaps to CPython are generators (3.0×), interp (2.8×) and
  fmt (2.1×).
- The previous run (2026-11-03, `54ea791`) was on a 4-core machine, so
  ratios compare better than absolute times.
## Stdlib modules agent code imports (2026-11-04)

Nine modules joined the default build, one commit each, in this order.
Each has an own case (`conformance/own/313_contextlib.py` to
`321_html.py`) whose stdout, stderr and exit status equal CPython 3.14.7's,
and passes `cargo xtask durability`. The corpus is unchanged (546/568
CPython cases). Sizes are shipped builds (`cargo xtask size`), measured
at each commit on top of `586d3af` [M]:

| Commit | Module | `hello.wasm` | Δ | native `hello` | Δ |
|---|---|---:|---:|---:|---:|
| base `586d3af` | | 1,737,574 | | 1,826,792 | |
| contextlib (+ `sys.exception`, `print` to a replaced `sys.stdout`) | Python | 1,742,977 | +5,403 | 1,832,184 | +5,392 |
| traceback (+ traceback objects, `sys.exc_info`, notes in reports) | Python + Rust | 1,757,178 | +14,201 | 1,846,344 | +14,160 |
| logging (slim) | Python | 1,768,079 | +10,901 | 1,856,968 | +10,624 |
| urllib.parse | Python | 1,779,786 | +11,707 | 1,868,904 | +11,936 |
| pprint | Python | 1,786,958 | +7,172 | 1,876,152 | +7,248 |
| shlex | Python | 1,790,732 | +3,774 | 1,879,960 | +3,808 |
| difflib | Python | 1,798,078 | +7,346 | 1,887,384 | +7,424 |
| struct | Rust (`structmod.rs`) + Python | 1,812,991 | +14,913 | 1,904,008 | +16,624 |
| html (`html.entities` not default) | Python | 1,814,857 | +1,866 | 1,905,928 | +1,920 |
| **total** | | | **+77,283** | | **+79,136** |

Rebased onto the SDK round and speed phase 4's direct frame push
(`7cf859d`: 1,741,354 / 1,830,312) the nine modules measure 1,818,715 /
1,909,416 [M], +77,361 / +79,104. The ceilings were raised by the
measured overshoot plus a small margin, as the owner asked: `hello.wasm`
1,742,500 → 1,823,000, native 1,836,000 →
1,914,000. `html.entities` (the html5 named-reference table) packs to
32,470 bytes of bytecode [M] and is `mod-html-entities`, outside the
default set; without it `html.unescape` resolves numeric references and
the html5 entries for amp, lt, gt, quot and apos only. Packed bytecode of
the others [M]: logging 10,521, urllib.parse 11,477, difflib 7,282,
pprint 7,104, contextlib 4,690, shlex 3,715, traceback 3,570, struct 2,036.

How the ports were made: `urllib.parse`, `pprint`, `shlex`, `difflib` and
`html` are CPython 3.14.7's files after small, scripted edit passes (no
`__slots__`, `bytearray`, `memoryview`, `warnings` or `ipaddress`; see each
file's header); `contextlib`, `traceback` and `logging` are hand-trimmed.
`traceback` takes each frame's text from the engine (`exc.rs`:
`traceback._extract`, `_exception_only`), so its output equals the
uncaught-exception report. `struct` is native, with x86-64 Linux native
sizes on every target; user objects in `pack` are converted in Python and
the call retried, so no user code runs from Rust.

difflib's SequenceMatcher [M, `mm run` vs `python3.14`, both running the
same pure-Python code]: `unified_diff` over 3k lines 57–63 vs 22 ms,
`get_close_matches` over 5k words 68–71 vs 16–17 ms, a character ratio of
two 6 KB strings 5.9 vs 1.6–1.7 ms. That 2.6–4.3× is the interpreter's
general gap (the coverage programs are 2.1–5.8×), not a hot spot, so there
is no native helper.

Gaps found and not fixed (outside these modules, or owned by other
threads):

- `str.translate`/`str.maketrans` and `bytes.translate` are missing
  (shlex works around them).
- `sys.stdout.writelines` is missing (difflib's documented
  `sys.stdout.writelines(unified_diff(...))` fails).
- A `dict` subclass's `__missing__` is not called through a bound
  `d.__getitem__` (it is through `d[k]`); urllib's `_Quoter` works around it.
- No `bytearray`/`memoryview`, so `struct.pack_into` is left out.
  (`bytearray` and `pack_into` filled 2026-11-05; `memoryview` remains.)
- No async generators or `async with`, so
  `contextlib.asynccontextmanager`, `aclosing` and `AsyncContextDecorator`
  are left out. (Filled; see the next section.)

## Engine gaps (2026-11-04)

Recovered async work, excluding the separate bytearray draft. The first
three rows are the isolated branch's shipped-build measurements [S]; the
last row is the integrated build (`cargo xtask size`) [M]. The ceilings
track the measured overshoot plus a small margin.

| Part | `hello.wasm` | Δ | native `hello` | Δ |
|---|---:|---:|---:|---:|
| base `9219a05` | 1,818,778 | | 1,909,464 | |
| async for/with, async generators and comprehensions, `aiter`/`anext` (engine) | 1,829,076 | +10,298 | 1,920,728 | +11,264 |
| `contextlib.asynccontextmanager`, `aclosing`, `AsyncContextDecorator` | 1,829,764 | +688 | 1,921,416 | +688 |
| integrated with restore-without-zeroing and bounded indirect calls | 1,833,115 | +3,351 | 1,925,464 | +4,048 |

The integration keeps `7ab4864`'s default-limit recursion regression.
The host-call snapshot test also suspends async generators under `aclosing`
on host futures, restores at calls and at the batch wait, and checks cleanup
and stable call IDs before completion.

### Async iteration and async generators

- The AST's `For`, `With` and `Comprehension` carry `is_async`, set by
  both front ends (`parse-diff` checks them). The compiler emits
  `GET_AITER`, then per step `ANEXT` + `AWAIT` under a handler whose
  `END_ASYNC_FOR` ends the loop on `StopAsyncIteration` (else re-raises);
  `async with` is `WITH_ENTER r 1` + `AWAIT` and `WITH_EXIT r 2|3` +
  `AWAIT`, the exceptional exit testing the awaited result itself.
  Module code may use them, as it may use top-level `await` (cells).
- An async generator is `ty::AGEN`, the generator layout, made by
  `new_gen` for a coroutine code object. `asend`/`athrow`/`aclose` and
  `__anext__` return `ty::AGENOP` awaitables; awaiting one leaves a request
  that resumes (or throws into) the generator's frame on the awaiting
  task's stack with `Cont::ASend`/`Cont::AClose`, which turn its yield or
  return into the await's result. An `await` inside it is an ordinary
  `AWAIT` in that frame, so it can block the task, park and snapshot like
  any coroutine: no nested run (`cargo xtask durability --filter 322`:
  780 parks, identical).
- Checked against CPython 3.14.7 in `conformance/own/322_async_iter_with.py`
  (protocols, error messages, `asend`/`athrow`/`aclose` states, reuse,
  `aiter`/`anext`, async comprehensions and generator expressions) and
  `323_contextlib_async.py`. The `with` statement also gained CPython's
  hint when an object supports the other protocol ("Did you mean to use
  'async with'?").
- Left as deviations: an async generator abandoned unfinished is never
  finalized (CPython's `asyncio.run` `aclose()`s it at shutdown; micro-monty
  finalizes no generator), `anext(it, default)` returns a `coroutine`
  rather than an `anext_awaitable`, `asyncio.gather` takes no `asend()`
  awaitables. `ag_running` staying true while awaiting is also CPython's
  behavior, not a deviation (checked on CPython 3.14.7 [M]).

## Speed phase 4: interned dunders, in-place comparisons, dict probes, dispatch

Callgrind instructions of `mm run bench/<p>.py` (whole process, release
`mm`) per step; every step's outputs equal the previous step's [M].

### Step 1: dunder names interned once per session

`39efa1c` → step 1 [M]:

| Program | Before | After | Change |
|---|---:|---:|---:|
| startup | 821,599 | 821,716 | +0.0% |
| fib | 331,114,311 | 329,525,368 | -0.5% |
| loop | 1,259,400,547 | 1,271,400,649 | +1.0% |
| dict | 1,186,261,801 | 1,188,261,858 | +0.2% |
| str | 495,005,140 | 495,405,159 | +0.1% |
| json_rows | 1,144,456,283 | 1,144,747,556 | +0.0% |
| closure | 757,012,757 | 760,012,852 | +0.4% |
| classes | 1,895,615,895 | 1,799,715,058 | -5.1% |
| generators | 3,342,043,616 | 3,342,051,730 | +0.0% |
| exceptions | 1,011,716,552 | 1,004,898,375 | -0.7% |
| fmt | 1,905,872,261 | 1,907,231,420 | +0.1% |
| sort | 7,234,113,372 | 7,223,220,154 | -0.2% |
| interp | 2,826,233,474 | 2,783,061,451 | -1.5% |

Shipped sizes [M]: `hello.wasm` 1,726,586 → 1,728,003 (+1,417); native
`hello` 1,817,640 → 1,818,744 (+1,104).

- Every dunder lookup (`dunder`, `user_method`, `dunder_call`,
  `meta_check`, comparisons, binary operators) interned its name on each
  call. The names are now `class::Dn` (one `dunders!` list, 89 names);
  `Vm::dn` reads `Vm::dunders` and interns on first use only.
- `Vm::dunders` is a GC root, starts empty (a fixed array cost 3.4 KB of
  wasm in `Vm::new` and restore) and is not snapshotted. `snapshot` clears
  it before its collection, so a restored session, which refills it on use,
  saves the same bytes as the original (`snapshots.rs` checks this).
- `user_lookup` validates the MRO once and reads each class dict directly;
  a builtin base that has the method still stops the walk (`None`).

### Step 2: list and tuple comparisons in place

Step 1 → step 2 [M]:

| Program | Before | After | Change |
|---|---:|---:|---:|
| startup | 821,726 | 823,682 | +0.2% |
| fib | 329,525,341 | 329,211,956 | -0.1% |
| loop | 1,271,400,609 | 1,208,414,311 | -5.0% |
| dict | 1,188,261,818 | 1,179,468,415 | -0.7% |
| str | 495,405,139 | 494,707,363 | -0.1% |
| json_rows | 1,144,665,715 | 1,142,989,884 | -0.1% |
| closure | 760,012,890 | 747,022,581 | -1.7% |
| classes | 1,799,715,031 | 1,793,051,277 | -0.4% |
| generators | 3,342,051,539 | 3,327,029,922 | -0.4% |
| exceptions | 1,004,898,335 | 995,176,861 | -1.0% |
| fmt | 1,907,231,334 | 1,905,290,190 | -0.1% |
| sort | 7,223,219,611 | 6,717,895,891 | -7.0% |
| interp | 2,783,050,139 | 2,768,087,735 | -0.5% |

`sorted(xs)` for `xs = [((i * 7919) % 1000, (i * 104729) % 997, i) for i
in range(200_000)]` (whole process, 285M of it building `xs`): 3,779M →
1,241M instructions [M]. `loop` and `closure` moved with the dispatch
loop's code layout, not with this change's code.

- `order_pure` and `eq_hard` copied both operands' items into `Vec`s
  (`seq_items`) at every level of every comparison: about half of a tuple
  sort was malloc, free and the copies. `Vm::seq_words` borrows a tuple's
  slots or a list's live buffer prefix; `order_seq` walks the two in place
  (these walks run no Python code, so the borrow is safe) and orders a pair
  of small ints without `uflags` or a recursive call.
- `x in list` walks by index, rereading the items each step (an `__eq__`
  may resize the list, as in CPython), and the list reverse iterator reads
  the length without copying the list.
- Size [M]: `hello.wasm` 1,729,754 → 1,730,337 (+583), native `hello`
  1,820,120 → 1,818,744. That is 337 bytes over the 1,730,000 ceiling; the
  owner approved raising the ceilings for this phase by the measured
  overshoot plus a small margin, so the primary ceiling is now 1,731,000
  (`xtask/src/main.rs`, `docs/spec.md` §6). `seq_items` now copies from
  `seq_words`, which keeps the growth to this.

### Step 3: dict probes leave clustered runs

Step 2 → step 3 [M]:

| Program | Before | After | Change |
|---|---:|---:|---:|
| startup | 823,682 | 822,286 | -0.2% |
| fib | 329,211,956 | 329,210,578 | -0.0% |
| loop | 1,208,414,311 | 1,208,412,866 | -0.0% |
| dict | 1,179,468,415 | 1,181,780,115 | +0.2% |
| str | 494,707,363 | 494,706,684 | -0.0% |
| json_rows | 1,142,989,884 | 1,142,989,362 | -0.0% |
| closure | 747,022,581 | 747,021,234 | -0.0% |
| classes | 1,793,051,277 | 1,793,050,545 | -0.0% |
| generators | 3,327,029,922 | 3,327,031,502 | +0.0% |
| exceptions | 995,176,861 | 997,146,336 | +0.2% |
| fmt | 1,905,290,190 | 1,905,290,194 | +0.0% |
| sort | 6,717,895,891 | 6,717,949,637 | +0.0% |
| interp | 2,768,087,735 | 2,768,403,970 | +0.0% |

20,000 inserts and 20,000 lookups with keys of a class whose `__hash__`
returns `hash(self.v)` and whose `__eq__` compares `v`: 8,610M → 1,012M
instructions; 2,000 keys 178M → 103M, 4,000 keys 507M → 204M (now
linear) [M].

- The index probed linearly, so consecutive hashes (small ints, and any
  `__hash__` built on them) filled one long run. A key whose equality
  needs Python code cannot stop at its own slot: the native probe must
  collect every entry with the same hash, so it walked to the end of the
  run, and a lookup of key `i` among `N` cost `N − i` steps. Absent int
  keys next to a dense run paid the same walk.
- Probes now follow CPython's `i = 5i + 1 + perturb`, `perturb >>= 5`
  (perturb starting at the hash) in lookup, insertion and deletion alike.
  Consecutive hashes still get their own first slot; a probe that misses
  leaves the run after one step. `Geo::steps` bounds the walk so it visits
  every slot (perturb reaches zero within 13 steps). The build id already
  covers the engine source, so older snapshots are refused, not misread.
- Size [M]: `hello.wasm` 1,730,068 (under the 1,731,000 ceiling), native
  `hello` 1,818,904.

### Step 4: one-access global stores, GC requests folded into ticks

Step 3 → step 4 [M]:

| Program | Before | After | Change |
|---|---:|---:|---:|
| startup | 822,286 | 822,341 | +0.0% |
| fib | 329,210,578 | 320,632,244 | -2.6% |
| loop | 1,208,412,866 | 1,166,389,509 | -3.5% |
| dict | 1,181,780,115 | 1,178,577,139 | -0.3% |
| str | 494,706,684 | 493,006,391 | -0.3% |
| json_rows | 1,142,989,362 | 1,141,374,807 | -0.1% |
| closure | 747,021,234 | 715,005,696 | -4.3% |
| classes | 1,793,050,545 | 1,773,476,694 | -1.1% |
| generators | 3,327,031,502 | 3,316,464,683 | -0.3% |
| exceptions | 997,146,336 | 995,043,104 | -0.2% |
| fmt | 1,905,290,194 | 1,900,252,929 | -0.3% |
| sort | 6,717,949,637 | 6,729,494,822 | +0.2% |
| interp | 2,768,403,970 | 2,754,333,456 | -0.5% |

Against `54ea791`: closure 731.0M → 715.0M (−2.2%), loop 1,145.4M →
1,166.4M (+1.8%) [M].

- **Moving cold opcodes out of `dispatch` measured slower.** Opcode counts
  over the 13 programs (an instrumented build) put 11 arms in `dispatch`
  under 1,000 executions in total (`FOR_PREP`, `GET_ITER`,
  `MAKE_FUNCTION`, `MAKE_CELL`, `STA_DEREF`, `DICT_INSERT`, `NOT`, `MOV`,
  `NOP`, `LDA_TRUE`, `LDA_FALSE`). Moving them to `dispatch_rare` cost
  +0.5% (str) to +6.4% (loop) on every program; adding `BUILD_DICT`,
  `BUILD_STRING`, `FORMAT` and `UNPACK_SEQ` cost +0.3% to +2.5%. In both
  builds LLVM spilled `base` and the bytecode pointer in the loop head (26
  stack accesses per `loop` iteration, was 12). Removing the two redundant
  `as_int` checks from the fused range step also measured +1% to +3.5%.
  The dispatch loop's cost is its register allocation, not its arm count.
- **`STA_GLOBAL`** checked the cached entry's key (deleted globals, phase
  3) with a second heap access: `Heap::set_guarded_live` checks the key
  and writes the value with one segment lookup (16 instructions per store
  less; loop and closure store a global every iteration).
- **One counter per safepoint check.** Every backward jump and call ran
  `ticks -= 1; if ticks <= 0 || gc_pending`. `Vm::request_gc` (every place
  that set `gc_pending`) now also moves the remaining ticks into
  `tick_spare` and zeroes `ticks`, so the check reads `ticks` alone;
  `safepoint` charges `tick_period - (ticks + tick_spare)`, as before, and
  the collection gives the spare ticks back. Fuel, parks and snapshot
  bytes are unchanged (`tick_spare` is folded back before a snapshot).
- What `loop` still spends over `54ea791` is the deleted-global check of
  each cached load (two per iteration) and register allocation.
- Size [M]: `hello.wasm` 1,730,186 (ceiling 1,731,000), native `hello`
  1,818,632.

### Step 5a: keyed sorts as native continuation frames

Step 4 → step 5a [M]:

| Program | Before | After | Change |
|---|---:|---:|---:|
| startup | 822,341 | 822,432 | +0.0% |
| fib | 320,632,244 | 320,632,367 | +0.0% |
| loop | 1,166,389,509 | 1,166,389,579 | +0.0% |
| dict | 1,178,577,139 | 1,178,577,266 | +0.0% |
| str | 493,006,391 | 493,006,531 | +0.0% |
| json_rows | 1,141,374,807 | 1,141,440,395 | +0.0% |
| closure | 715,005,696 | 715,005,771 | +0.0% |
| classes | 1,773,476,694 | 1,773,476,817 | +0.0% |
| generators | 3,316,464,683 | 3,316,475,084 | +0.0% |
| exceptions | 995,043,104 | 995,043,208 | +0.0% |
| fmt | 1,900,252,929 | 1,898,114,864 | -0.1% |
| sort | 6,729,494,822 | 1,822,627,914 | -72.9% |
| interp | 2,754,333,456 | 2,754,342,072 | +0.0% |

`sorted(xs, key=f)` for 200,000 ints, net of building `xs`: 7,823M →
911M instructions [M].

- `Cont::Native { op, state }` (`nstep.rs`) as designed in
  `docs/spikes/native-continuations.md`: `finish` hands a callee's result
  to `Vm::native_step`, `finish_exc` an exception to `Vm::native_unwind`.
  The state is one `NSTATE` arena object (value slots, then raw words;
  arena type 54, no Python type); `STEPS` names each op's layout, and
  every step checks the state against it (`SystemError`, never a panic).
  Snapshot tag 26 stores `[state]` and `[op]`; `read_cont` refuses an op
  past `STEPS`, and `build_fingerprint` hashes `STEPS`.
- `sorted` and `list.sort` with `key=` keep the items in a list the
  operation owns, call the key on each item through the step (a park or
  snapshot can land inside any key call), then merge-sort an index
  permutation natively by the keys (`Vm::sort_by_keys`, the same
  `merge_sort` as keyless sorts, now taking `&mut dyn FnMut`). Keys that
  compare in Python go to prelude `_sort_keys` (the second half of `_sort`)
  and finish in `SORT_DONE`. `list.sort` detaches its items while it runs,
  raises "list modified during sort" if the list was refilled, and gets
  its items back unsorted when a key raises, as before.
- Own case `313_native_sort_keys` covers stability, `reverse`, a raising
  key, keys that inspect or append to the list, user-object keys, mixed
  types and a non-callable key; durability parks it 143 times.
- Size [M]: `hello.wasm` 1,730,186 → 1,737,000 (+6,814), native `hello`
  1,818,632 → 1,826,040 (+7,408). The owner approved raising the ceilings
  for this phase by the measured overshoot plus a small margin: the
  primary ceiling is now 1,738,000; native stays under 1,836,000.

### Step 5b: `iter(f, sentinel)` with a sentinel that compares in Python

Step 5a → step 5b [M]:

| Program | Before | After | Change |
|---|---:|---:|---:|
| startup | 822,432 | 822,341 | -0.0% |
| fib | 320,632,367 | 320,632,302 | -0.0% |
| loop | 1,166,389,579 | 1,166,389,566 | -0.0% |
| dict | 1,178,577,266 | 1,178,577,197 | -0.0% |
| str | 493,006,531 | 493,006,433 | -0.0% |
| json_rows | 1,141,440,395 | 1,141,387,758 | -0.0% |
| closure | 715,005,771 | 715,005,705 | -0.0% |
| classes | 1,773,476,817 | 1,775,092,740 | +0.1% |
| generators | 3,316,475,084 | 3,329,237,011 | +0.4% |
| exceptions | 995,043,208 | 995,203,116 | +0.0% |
| fmt | 1,898,114,864 | 1,898,114,704 | -0.0% |
| sort | 1,822,627,914 | 1,824,283,459 | +0.1% |
| interp | 2,754,342,072 | 2,755,182,459 | +0.0% |

- `iter(f, sentinel)` raised "SystemError: micro-monty: this operation
  needs Python code here" when the sentinel `==` needed a user `__eq__`
  (`Cont::CallIter` had no handoff). `CallIter` now compares natively
  (`eq_hard`) and, when Python must decide, calls prelude `_eq_bool` with
  the new fixed-shape `Cont::CallIterEq { it, exit, v }` (snapshot tag
  27): true exhausts the iterator, false yields the held `v`. No `NSTATE`.
- CPython 3.14 compares `sentinel == value`, the sentinel first (the old
  native path compared `value == sentinel`; equal for builtin values).
- Own case `314_calliter_user_eq` (for, `list`, `next` with a default,
  `sorted`, a `__eq__` returning a truthy object, a raising `__eq__`).
- `generators` +0.4%: layout of the (cold) `finish`, not new work.
- Size [M]: `hello.wasm` 1,737,000 → 1,737,574 (+574, under the 1,738,000
  ceiling), native `hello` 1,826,040 → 1,826,792.

### Step 5b′: direct frame push for continuation calls and implicit dunders

Step 5b → this change [M]:

| Program | Before | After | Change |
|---|---:|---:|---:|
| startup | 822,341 | 824,574 | +0.3% |
| fib | 320,632,302 | 323,494,815 | +0.9% |
| loop | 1,166,389,566 | 1,166,391,731 | +0.0% |
| dict | 1,178,577,197 | 1,178,379,477 | -0.0% |
| str | 493,006,433 | 493,008,691 | +0.0% |
| json_rows | 1,141,387,758 | 1,141,345,768 | -0.0% |
| closure | 715,005,705 | 715,007,950 | +0.0% |
| classes | 1,775,092,740 | 1,748,845,946 | -1.5% |
| generators | 3,329,237,011 | 3,345,655,833 | +0.5% |
| exceptions | 995,203,116 | 992,875,450 | -0.2% |
| fmt | 1,898,114,704 | 1,897,937,183 | -0.0% |
| sort | 1,824,283,459 | 1,774,066,203 | -2.8% |
| interp | 2,755,182,459 | 2,740,694,620 | -0.5% |

Micro-programs (whole process) [M]:

| Program | 5b | This change |
|---|---:|---:|
| 20k `p < q`, user `__lt__` (setup alone 209.5M) | 316.9M | 269.3M |
| `sorted` of 20k user objects (`__lt__`) | 2,166.0M | 1,590.3M |
| 100k `P(i)` with a Python `__init__` | 412.9M | 400.5M |
| `sorted(xs, key=f)`, 200k items (setup 132.4M) | 1,045.5M | 978.7M |

- Implicit dunders (`cmp_next` → `call_dunder`) and continuation calls
  (`Next::Invoke`, keyed-sort key calls) used the generic chain
  `land` → `drive` → `invoke` → `call_vec` → `call_value_at` →
  `call_value` → `enter_frame`, with a heap `Vec` for the arguments at
  each step; `call_class` reached `__init__` through `call_vec` too.
- `Vm::push_func`: a plain Python function (simple parameters, exactly
  its positional count, not a generator or coroutine) gets its arguments
  written straight into the new frame's registers and is entered. `drive`
  calls it for `Next::Invoke` without keywords before `invoke`; `call_vec`
  calls it first, which covers `__init__` and bound methods. Errors from
  it take `drive`'s `Err` arm (trampolines popped, `cont` finished with
  the raise), as `invoke`'s do.
- `cont::Args` replaces the `Vec` in `Next::Invoke` and `Request::Call`:
  up to four arguments inline (every dunder call), more on the heap.
  Requests never live across a safepoint, so tracing is unchanged.
- `cmp_next` stage 0: when the right operand's class is not a proper
  subclass of the left's and the left has the method, that method is
  called without looking up the reflected one (and without the candidate
  `Vec`). The order is CPython's; later stages still use `cmp_cands`.
  One `dunder()` MRO lookup is about 1.1k instructions [M]; the call path
  per comparison went from about 1.6k to 0.67k [M].
- `fib` +0.9% and `generators` +0.5% are inside `dispatch` itself
  (register allocation after `call_vec` changed); neither reaches the new
  code.
- Size [M]: on 5b, `hello.wasm` 1,737,574 → 1,738,713 (+1,139), native
  `hello` 1,826,792 → 1,827,544. Rebased onto the SDK round (ceiling
  1,741,000, `hello.wasm` 1,739,727), it is 1,741,354, 354 bytes over;
  per the owner's phase-4 rule (raise by the measured overshoot plus a
  small margin) the ceiling is now 1,742,500 (xtask, `docs/spec.md` §6).
  Native `hello` 1,830,312 (under 1,836,000).

### Step 5c: `min`/`max` with `key=` stay in the prelude (measured, not adopted)

A native continuation step for `min`/`max` with `key=` (`op::MINMAX`,
`minmax_start`/`minmax_step` in `nstep.rs`, prelude `_minmax_rest` for
keys that compare in Python) matched CPython on the test cases. On top
of the direct frame push it was still slower than the prelude loop
[M], so it was not landed:

| `min(xs, key=lambda x: x)`, 200k ints (whole process) | Instructions |
|---|---:|
| Prelude loop (`main`) | 384.1M |
| Native continuation step | 453.9M |

Before the direct push the native step was slower still (388M vs 251M
net of setup [M]). The native step now costs about 350 instructions more
per item (69.8M over 200k items [M]). Each key call is a special frame
carrying `Cont::Native`: `finish` → `native_step` → `drive` → `push_func`
→ `enter_frame`, and its return goes through `dispatch_rare` → `finish`
→ `minmax_step` (which re-reads the list through `seq_words`). The
prelude's `f(x)` is a plain inline-cached `CALL` with a linked frame.
Keyed sort still wins (`sort` 6,729.5M → 1,822.6M at step 5a) because its
comparisons then run natively over the computed keys. `min`/`max` have
no such phase, so only the per-item call cost counts. A native step for
them would pay off only if a continuation call cost no more than an
inline-cached call. That means a linked (non-special) frame record
whose return re-enters the native step without `finish` and `drive`.

## Speed phase 3: builtin lookup, globals across `del`, generator resume

Callgrind instructions of `mm run bench/<p>.py` (whole process, release
`mm`), `54ea791` → this change [M]:

| Program | Before | After | Change |
|---|---:|---:|---:|
| startup | 824,313 | 821,599 | −0.3% |
| fib | 326,032,030 | 331,114,311 | +1.6% |
| loop | 1,145,379,757 | 1,259,400,547 | +10.0% |
| dict | 1,164,196,694 | 1,186,261,801 | +1.9% |
| str | 490,014,128 | 495,005,140 | +1.0% |
| json | 1,158,622,743 | 1,144,456,283 | −1.2% |
| closure | 730,999,789 | 757,012,757 | +3.6% |
| classes | 2,132,682,979 | 1,895,615,895 | −11.1% |
| generators | 4,261,068,887 | 3,342,043,616 | −21.6% |
| exceptions | 2,375,947,938 | 1,011,716,552 | −57.4% |
| fmt | 1,988,506,113 | 1,905,872,261 | −4.2% |
| sort | 7,259,311,194 | 7,234,113,372 | −0.3% |
| interp | 2,888,680,989 | 2,826,233,474 | −2.2% |

Shipped sizes [M]: `hello.wasm` 1,729,846 → 1,726,474 (−3,372); native
`hello` 1,831,000 → 1,816,648 (−14,352).

- **Builtin methods.** `find_method` scanned all 450 `NATIVES` with string
  compares, and `own_lookup` validated the name's UTF-8 for each builtin
  base, so a miss through `Invalid → ValueError → Exception → BaseException
  → object` cost about 2,200 instructions. The literal table is now
  compile-time only; a `const fn` packs it into `NATIVES` (24 bytes per entry
  natively, so 16 on wasm32, where it was 40: names and parameter lists in one deduplicated blob, a
  module index, one function pointer) ordered as functions, then each type's
  methods as one run sorted by name length and bytes. `method_index` binary
  searches the run on the name's bytes; `class_lookup` validates the MRO and
  the name once. `Sref::get` stays out of line (inlined it cost 4.3 KB of
  wasm code).
- **Globals across `del`.** `except Invalid as error` deletes and re-adds a
  global each time, and every key change bumped `gver`, so all global caches
  missed after it (600K slow loads and stores per run). Adding a key now
  bumps only `bver`, which builtin caches check as `gver + bver`; deleting
  bumps neither. The fast paths treat a deleted entry as a miss: a load that
  reads `UNDEFINED`, a store whose key slot is `UNDEFINED`. Replacing an
  entries table (resize, clear) still bumps `gver`. Own case
  `88_shapes_and_caches` covers loads, stores and builtin shadowing across
  `del`.
- **Exception attributes.** `generic_setattr` and `generic_delattr` looked
  the name up twice (property, then descriptor); now once. Exception
  instances still have no attribute cache: their attributes live in a dict,
  so `self.code = …` and `error.code` walk the MRO each time (40K lookups,
  55M instructions, was 803M).
- **Small-int `%` and `//`** with a positive divisor run in `BINOP`'s fast
  path (they went through `op_binop`, 380 instructions each: 387K calls in
  exceptions, 208K in classes).
- **Generator resume.** The cost was not the copy but one validated heap
  access per register each way (and per generator field). `gen_enter` reads
  the generator with one validation and the registers with one
  `Heap::read_values`; `gen_yield` writes them with one `set_values` and the
  state and pc with one raw access. `gen_enter` went from 775 to 524
  instructions per resume (154 of them `enter_frame`). Copying only live
  registers would save about one register of four per resume, now a few
  instructions, so no liveness set is emitted; running the frame in place
  would need the dispatch loop to address heap registers.
- **Instance guards.** `inst_guard` made five validated reads per attribute
  or method cache hit (shape, class, version, length, slot); it now
  validates the instance once and reads the class version with
  `Heap::get_raw_guarded` (the `CLASS` kind byte proves the class, which the
  instance keeps alive): 107 → 91 instructions per call, and the slot read
  shares the validation. `LOAD_METHOD` checks its cache before
  `user_class`.

Regressions in the table are the dispatch function's register allocation,
not executed work [M]: with only the `NATIVES` change (no edit to
`dispatch`), loop measured 1,280M; with `find_method` left as the inlined
linear scan, 1,211M. The real per-iteration cost added to `loop` is the two
global-load and two global-store `UNDEFINED` checks (about 10 instructions,
under 3%). What is left: constructors (`Vec(…)` costs 2,750 instructions:
`dunder` interns `"__init__"` on every call, 454 instructions, and walks
the MRO, 569) want pre-interned dunder names, which need a `Vm` field and
GC roots; exceptions' raise/unwind (`add_tb` 83M) and their dict-based
attributes; a generator round trip still pays the generic special-frame
record, continuation and `finish`/`drive` (about 1,500 instructions).

## Size by build structure: shipped builds and two budgets

Measured on `8156ee8` [M]: stripped `hello` bytes, and callgrind instruction
counts of `mm run` on every bench program, relative to today's build (Rust
1.99.0, opt 3, fat LTO, panic=abort). Today: hello 2,273,048; startup 10.83M,
fib 402.5M, loop 1,476M, dict 1,342M, str 579.4M, json 1,746M, closure 975.0M.
Nothing below is committed; each needs a decision (end of section).

| Change | hello | Instructions | Needs |
|---|---|---|---|
| `-C relocation-model=static` (non-PIE) | −64,440 | startup −3.3%, fib −7.7%, loop −7.7%, dict −3.4%, str −1.5%, json −1.8%, closure −8.0% | stable; flag must reach only final binaries (it breaks proc-macro dylibs, e.g. the worker's `serde_derive`, unless built with explicit `--target`); executable loses ASLR |
| `-C link-arg=-no-pie` only | −66,056 | unchanged | stable; size only |
| `-Zbuild-std=std,panic_abort` alone | +1,416 | fib −1.6%, loop −3.9%, closure −3.5%, rest ≤0.5% | nightly |
| + `-Zunstable-options -Cpanic=immediate-abort` | −320,992 | vs today: fib +2.2%, loop −1.0%, rest within ±0.9% | nightly; removes std's panic/backtrace (gimli, addr2line, miniz, demangle) |
| + `-Zbuild-std-features=optimize_for_size` | −12,800 more | json +0.9% | nightly; not worth it |
| + `-Cforce-unwind-tables=no` (with build-std) | −140,000 more (`.eh_frame` 127 KB → 96 B) | unchanged | nightly (prebuilt std keeps its tables; on stable the flag saves 0) |
| `objcopy --remove-section=.eh_frame{,_hdr}` after link | −143,520 | unchanged | stable post-processing; fine under panic=abort |
| `#[optimize(size)]` on the cold breadth (below) | −141,000 | all within ±0.2% | nightly today; stable in Rust 1.101 |
| same, but `#[cold]` (stable) | −69,912 | fib +0.8%, loop +0.4% | stable; misleads branch weights |
| immediate-abort + static + `optimize(size)` | 1,759,920 (−22.6%) | startup −4.2%, fib −4.8%, loop −7.5%, dict −3.5%, str −2.9%, json −2.0%, closure −6.4% | nightly |
| + no unwind tables | **1,619,648 (−28.7%)** | same as the row above | nightly |

Cold breadth that measured free at `optimize(size)`: mm-engine `math`, `cmath`,
`libm`, `complex`, `hashlib`, `re`, `percent`, `bytes`, `binascii`, `random`,
`snapshot`, `bigint`, `unicodedata`, `mmmod`, `modules`, `text`, `intops`,
`exc`, and mm-core `snapshot` (631 functions; parser and compiler are already
`z`/`s` per package). `json` is not cold: at size it cost json_rows +7%.

Wasm `hello.wasm` (wasip1 / unknown-unknown), both round trips pass in every
row [M]: today 1,867,559 / 1,977,574; `optimize(size)` on the cold breadth
1,755,559 / 1,824,602; build-std + immediate-abort + `optimize(size)`
1,674,664 / 1,724,498 (−10.3% / −12.8%). Static relocation and unwind tables do
not apply to Wasm. `wasm-opt -Oz` (binaryen 108) on top: today 1,709,841 /
1,773,615; best build 1,543,036 / 1,589,456 (Wasm speed not measured).

Per-function `#[optimize(size)]` gives the hot/cold split without a crate split.
A crate split is not worth doing: the cold files are `impl Vm` methods on
crate-private state that `builtins.rs` tables name, so a separate crate would
need Vm internals exported, table registration inverted and edits in `vm.rs`.
Only leaf code (`libm`, `bigint`, about 40 KB of code) moves cleanly, for about
10 KB [E]. The attribute is stable since 1.101.0-nightly; with 1.99.0 on
2026-10-01 and a six-week cadence, 1.101 ships around 2026-12-24 [E].

Reproduce: build `hello` and `mm` with the flags above (`RUSTC_BOOTSTRAP=1` on
the pinned 1.99.0 with the `rust-src` component, or `nightly-2026-11-01`,
`--target x86_64-unknown-linux-gnu`), then `valgrind --tool=callgrind mm run
bench/<program>.py` for each program.

Owner decisions (2026-11-03), implemented in xtask, build config and docs:

1. Stay on the pinned 1.99.0. Shipped builds only (what `cargo xtask size`
   and `cargo xtask wasm` build: `shipped_build` in `xtask/src/main.rs`) use
   `RUSTC_BOOTSTRAP=1`, `-Zbuild-std=std,panic_abort`,
   `-Cpanic=immediate-abort` and `-Cforce-unwind-tables=no`, with `rust-src`
   in rust-toolchain.toml and `.agents/setup`. Tests, clippy and normal
   builds, including `mm` for conformance, durability and bench, stay plain
   stable. `optimize_for_size` is skipped.
2. PIE stays; no non-PIE default or opt-in.
3. Both budgets gate (`docs/spec.md` §6): `wasm32-unknown-unknown`
   `hello.wasm` primary, 1,863,611 bytes [M] under 1,873,000; native shipped
   `hello` secondary, 1,826,728 bytes [M] under 1,836,000 (was 2,273,048
   under 2,275,000). wasip1 `hello.wasm` is 1,791,753 [M], reported only.

**Waiting for Rust 1.101 (about 2026-12-24 [E]):** move the pin and mark the
cold set above `#[optimize(size)]` (−141 KB native, −112/−153 KB Wasm, no
speed cost [M]), then lower both ceilings to the new measurements. Do not
put `json` in the set. Superseded in part on 2026-11-09 (decision 69): part
of the cold set is marked now, on shipped builds only.

## Build pipeline: wasm-opt adopted, PGO and BOLT measured (not adopted)

Measured on `4e220da` [M], 4 vCPU orb, Rust 1.99.0 (LLVM 23.1.1). Engine
code unchanged.

**`wasm-opt -Oz` is in the shipped Wasm pipeline** (binaryen 133 from
`.agents/setup`; `xtask/src/wasm.rs`, `optimize`). It runs with exactly
the features rustc 1.99 enables, because stripped modules carry no
`target_features` section. `hello.wasm` shrinks 1,865,753 → 1,721,143 bytes
(unknown-unknown, −144,610) and 1,795,731 → 1,654,945 (wasip1); the primary
ceiling dropped from 1,873,000 to 1,730,000. Both round trips run on the
optimized modules and pass. The output is deterministic and takes about 3 s
per module. Speed cost: `mm` built as shipped for wasip1, precompiled by
wasmtime 50 and pinned to one CPU; medians of 5–11 runs over two to four
alternating rounds, in ms:

| Build | mm.wasm bytes | fib | loop | closure | dict |
|---|---|---|---|---|---|
| no wasm-opt | 2,509,863 | 40.6–42.1 | 96.5–98.8 | 72.5–76.4 | 140.6–149.7 |
| `-Oz` | 2,342,402 | 44.1–45.9 | 102.9–110.2 | 82.6–86.8 | 146.5–158.1 |
| `-O3` | 2,356,404 | 44.7–45.2 | 102.8–103.7 | 77.8–78.9 | 140.4–147.4 |
| `-Os` / `-O2` / `-O1` | 2,348,698 / 2,363,969 / 2,377,279 | 51.1 / 46.5 / 45.6 | 109.1 / 105.3 / 107.0 | 86.7 / 81.5 / 77.1 | 146.4 / 143.5 / 148.8 |

So every level is slower than none on the dispatch-heavy programs (`-Oz`
fib +8%, loop +8%, closure +13%, str +5%), while dict, json and startup
stay within noise [M]. Skipping single passes (`coalesce-locals`, `simplify-locals`,
`code-folding`, `merge-blocks`) did not recover it reliably. If Wasm
throughput matters more than 145 KB, the alternative is no `wasm-opt`.
That needs an owner decision; `-Oz` stays as asked.

**PGO (native), measured, not adopted.** Instrumented build:
`-Cprofile-generate` on the shipped native build works on the pinned
1.99.0 (build-std builds `profiler_builtins`). The training corpus is
`bench/*.py` and `conformance/own/*.py` (144 files, about 4 s) run on `mm`
and on `hello` (source as `argv[1]`). Profiles are merged with
`llvm-profdata` from the `llvm-tools` component and applied with
`-Cprofile-use`. Shipped flags on both sides; `mm` is `mm-cli` built as
shipped. Instructions are callgrind counts of `mm run bench/<p>.py`. Wall
times are the median of 11 runs pinned to CPU 2, averaged over two
alternating rounds:

| Program | Instructions base → PGO | Wall ms base → PGO |
|---|---|---|
| startup | 6.6M → 5.7M (−14.0%) | 3.3 → 3.2 (−2%) |
| fib | 414.1M → 375.4M (−9.4%) | 22.2 → 18.7 (−16%) |
| loop | 1,340.1M → 1,234.2M (−7.9%) | 58.6 → 48.7 (−17%) |
| dict | 1,338.3M → 1,091.4M (−18.4%) | 92.2 → 80.9 (−12%) |
| str | 569.8M → 445.6M (−21.8%) | 37.2 → 32.5 (−13%) |
| json | 1,735.4M → 1,352.2M (−22.1%) | 110.0 → 91.8 (−17%) |
| closure | 933.7M → 851.8M (−8.8%) | 40.0 → 37.3 (−7%) |

Holdout: `mm run` over the 723 `conformance/monty` files outside the
training set took 4.60/4.57 s base and 4.40/4.39 s with PGO (−4%), mostly
process startup [M]. Size: native `hello` 1,825,864 → 2,339,320 bytes
(+513,456, +28.1%), `mm` 2,544,648 → 2,963,496 (+16.5%) [M]. The growth
is spread over hot code (`Vm::dispatch` +121 KB, the JSON scanner and
encoder +57 KB, `call_value` +23 KB, mm-compile +51 KB, mm-parse +29 KB
despite `opt-level = "z"`). Lowering `-hot-callsite-threshold` to 325 saved
only 12 KB.

Why it is not adopted:
1. It breaks the native ceiling (1,836,000) by 500 KB; adopting it is an
   owner size decision.
2. **PGO exposed an engine hang.** `Vm::repeat` in
   `crates/mm-engine/src/ops.rs` runs `for _ in 0..k { out.extend_from_slice(&items) }`
   even when `items` is empty. The size check passes because the total is 0, so
   `[] * 2**62` (corpus `itertools__combinatoric.py`, through
   `itertools.product(repeat=2**62)`) and `() * 2**30` (`tuple__ops.py`)
   only finish because LLVM deletes the empty loop in the plain build. With
   PGO the loop stays: the first case times out after 20 s, the second takes
   1.1 s instead of 2 ms [M]. That native loop is also uninterruptible,
   because it burns no fuel. Fix: return early when the sequence is empty
   (engine change, outside this task). `conformance --check` with a PGO `mm`
   fails until it is fixed.
3. The tooling is workable but heavy. Each shipped build becomes three steps
   (instrument, train, rebuild): about 2× build time plus 4 s training. It
   needs `llvm-tools`. A committed `.profdata` would go stale silently: the
   profile keys on symbol hashes, which change with features, the cargo
   profile, or `strip=false`. Rebuilding with strip off left the profile
   mostly unmatched (`.text` 1.63 MB instead of 1.90 MB).

If the owner wants PGO for `mm` only (speed, not the size-gated `hello`),
fix item 2 first. Then add an xtask step: build `mm` with
`-Cprofile-generate`, run the training corpus, `llvm-profdata merge`,
and rebuild with `-Cprofile-use`.

**PGO on Wasm.** Instrumented builds do not exist for wasm32:
`-Cprofile-generate` fails with "can't find crate for
`profiler_builtins`". Applying the native profile to a wasip1 build mostly
matches (symbol names agree across targets): `hello.wasm` 1,795,731 →
1,976,672 (+10%), `mm.wasm` 2,509,863 → 2,693,792. Under wasmtime it gave
loop −5%, dict −7%, str −4%, json −2%, fib +2% and closure 0 [M]. That is not
worth +10% on the primary budget, so it is native-only in practice.

**BOLT (llvm-bolt 23 from apt.llvm.org), measured, not adopted.** I used
instrumentation mode (`llvm-bolt -instrument`, the same training corpus),
because orbs have no perf/LBR. The input was the PGO build linked with
`--emit-relocs`, unstripped via `-Cstrip=none` so the profile still
matched. Options: `-reorder-blocks=ext-tsp -reorder-functions=cdsort
-split-functions -split-all-cold`. On top of PGO it changed instructions by
−0.0% to −0.6% and wall time within noise. Taken branches fell 62%, but PGO
had already laid out the hot code. BOLT's new `.text` did not fit the old
one (`-use-old-text`), so stripped `mm` and `hello` grew to 8.4 MB. GNU
strip 2.40 also cannot handle its output; `llvm-strip` can. No gain, large
size cost, an extra non-rustup toolchain: dropped.

Reproduce (scripts were scratch, not committed): shipped flags plus
`--target x86_64-unknown-linux-gnu --target-dir <dir>`. Build `-p mm-cli` and
`-p micro-monty --example hello` in separate cargo invocations, so `mm-cli`'s
extra stdlib features do not unify into `hello`. Then run the corpus with
`LLVM_PROFILE_FILE=<dir>/%m_%p.profraw`, merge, and rebuild.

## Startup, stage A: shared code and one-pass class flags

Startup (an empty program, process start to exit) cost 10.8M instructions
[M, callgrind] against QuickJS's 1.5M; 82% was `Session::new`. Two causes
are gone, with no size cost:

- `Program::codes` is `Vec<Arc<Code>>`. Sessions loading a program (the
  prelude, a compiled cell, a restored snapshot) reference its code instead
  of cloning every bytecode buffer and name string (0.74M per session, and
  as much again to free it). Memory accounting is unchanged: each session
  still charges the code it holds. Decoding charges the shared allocation
  (`verify_bytecode` budget test updated).
- `compute_flags` walks the MRO once: a class dict's dunder keys, or one
  scan of a builtin type's natives. Before, it interned 78 names and ran
  `user_lookup` for each, and `find_method` scans all of `NATIVES`, so
  every prelude class cost about 9K instructions (1.9M in total). The first
  MRO entry still decides each name, so a builtin base's method hides a
  later user class's as before.

Startup is now 7.6M [M]. What is left: prelude decode 3.7M once per process
(unpack 1.9M for 53 KB, about 2,270 owned name strings 1.7M), session
build 2.7M (interning 1.6M: 2,147 calls, 722 new strings), first cell
0.65M. Fewer interned dunder names shrink the baseline arena, so
`sorted_json_entries_share_the_memory_budget` moves from 6,448 to 6,384 KiB
(its window is now 6,360-6,408 KiB [M]).

Stage B (6.7M [M]): `Code`'s name, qualname, names, varnames and
freenames are `Arc<str>`, so decoding hands each code object the pool's
string instead of a copy (decode 1.7M -> 1.1M; a decoded name is still
charged per use, as before). `Vm::init` sizes the intern table for 1,024
strings up front, saving five rehashes (interning 1.6M -> 1.2M). A rewrite
of the unpacker's copy loop measured worse (+0.2M) and was dropped.

An embedded heap image is out under the new budgets (a fresh snapshot is
122 KB against about 9 KB of headroom), and a per-process template only
helps the second session, while the CLI and the cell worker start one
session per process.

## Speed phase 2: fused pairs, and what the dispatch structure costs

An opcode-pair trace of fib/loop/closure (temporary instrumentation, kept out
of the tree) showed `LDA_GLOBAL→STAR`, `LDA_SMI→SUB`, `TEST_LT→JUMP_IF_FALSE`
and `FOR_ITER→STAR` among the most frequent pairs. `bytecode::fuse` now rewrites
the first opcode of such a pair after compilation and leaves the second in
place, so lengths, jump targets, line and exception tables and park points are
unchanged. A fused handler runs both instructions on its int/cache fast path and
otherwise exactly the first; the verifier requires the partner to follow.
Instructions [M, callgrind]: fib 435.5M → 402.5M, loopfn 1,098M → 1,032M, callfn
664M → 642M, loop −3.5%, closure −1.4%; interleaved medians 2–9% faster on all
six call/loop programs [M].

`asyncio.gather` over N tasks was quadratic (16,000 tasks: 3.75 s) because
charging execution memory walked every suspended task on each switch; a running
total of parked buffer bytes fixed it (0.04 s) together with per-gather pending
counts and waiter lists (`be68ff2`). Keeping `reserve_execution` out of line
saved about 5 KB: once small, LTO inlined it into a dozen growth paths.

Moving 19 colder opcodes out of `dispatch` cut 5–6% of instructions on loop,
closure and calls but cost 8% on fib and 1–3% on dict/str/json [M], so it was
not kept (patch and scripts: `../mm-perf`). Every instruction still stores
values to the stack at the loop head; LLVM's allocation of the 37 KB function
moves with unrelated edits. Next: decide which state lives in registers
deliberately (for example a small hot loop over a compact operand cache, or
fewer live values across arms) and judge each step by instruction counts on
all bench programs, not only the three targets.

## Speed phase 1: calls and globals

Profiled fib/loop/closure (perf sampling, callgrind instruction counts; this
orb exposes no hardware counters) [M]. The `match` dispatch is not the limit:
an empty 10M-iteration loop runs 158 ms against QuickJS's 202 ms, and `loop`
moved into a function is 1.08× QuickJS. The gap was (1) Python calls, 31.5 ns
each against 13.8 ns for QuickJS and 15.4 ns for CPython, and (2) module-level
globals going through the fully checked heap API on every cache hit.

Landed: the simple-call fast path pushes the frame inline (no second code
lookup, continuation slot or depth scan unless the frame table must grow or the
limit is near); closure cells are copied with one validated read of the closure
tuple; global loads and stores that hit their cache use `Heap::get_guarded` /
`set_guarded`, which skip the object-start and header checks the version guard
already proves (every key or entries-buffer change of a globals dict bumps
`gver`; restore and globals resets clear the caches); range iteration steps with
one validated raw-slot access. Interleaved same-machine medians, old → new [M]:
fib 39.1 → 33.9 ms, loop 120.4 → 90.7, closure 88.0 → 68.9; dict, str, json and
startup unchanged within noise. Full `cargo xtask check` and full GC stress
passed (1,135 s; 270 durability runs, 39,388 fresh-process restores). Native
hello grew 176 bytes to 2,221,544.

The 2026-11-03 `bench/results` run is about 10–20% slower than the 2026-11-01
run for every engine on this orb (CPython dict 125.7 → 154.7 ms); compare
ratios within one run. In it micro-monty is 1.52× (fib), 1.32× (loop) and
1.65× (closure) QuickJS, beats Monty on every row and CPython on fib, loop,
json and closure, and is slower than CPython on dict and str.

What did not work, and why it matters [M]: reading each instruction's operands
with one 8-byte load (+12–40% time), and one bounds check per instruction in the
hot opcodes (+10–16% instructions). Moving five cold opcodes out of `dispatch`
cut instructions about 5% everywhere, but moving four more changed nothing.
Small source edits inside the 35 KB `dispatch` function shift LLVM's register
allocation unpredictably; the hot dispatch head currently spills base, acc, pc
and base+1 on every instruction. Further speed needs a deliberate dispatch
structure (what lives in registers, which opcodes stay in the hot function),
measured with instruction counts and pinned timings, not local tweaks. Remaining
per-call cost is about 500 → 400 instructions against QuickJS's 266 per call
iteration.

## Clean checkpoint: native rendering ownership

Internal rendering now returns admitted text rather than dropping its reservation
before consumers allocate. This intentionally changes low-budget successes into
catchable `MemoryError`s where native text and the arena result coexist. Public
text APIs transfer ownership to the caller. Shared scratch growth uses atomic
admission against the non-scratch ceiling; borrowing input cannot overcommit
competing reservations. Remaining numeric/name/error helper buffers are explicitly
outside this phase, not silently claimed as accounted.

Final `cargo xtask check` passed (`/tmp/mm-render-final-check.log`): unchanged
483/568 CPython-tagged and 61/81 host results, 997 durability runs / 66,784
fresh-process restores, both Wasm round trips and every checksum [M]. Native
hello is 2,221,368 bytes; Wasm hello is 1,822,416 (wasip1) / 1,930,219
(unknown-unknown) bytes [M]. No GitHub checks run automatically: the workflow
is manual-only. A green gate means the recorded conformance baseline is unchanged,
not that every corpus case passes.

The public budget/restore probe passed 40 cases normally and the same 40 with
explicit GC stress; 245 output lines matched CPython in both modes [M]. The
baseline permits the 4 MiB repeated-repr and OSError cases that now refuse;
8 MiB controls succeed. The completion fallback now refuses at 3 MiB and parks,
restores and completes at 3.25 MiB (3 MiB + 320 KiB since the recovered
conformance integration grew the prelude). Large Unicode padding needs 32 MiB rather
than 16 MiB because both output copies count [M]. Full final-tree GC stress
and extended fuzzing have not run; previous campaigns do not cover this phase.

Pinned alternating public-facade medians [M]: large repr 1.262 → 1.338 ms
(+6.0%), ASCII 2.361 → 2.200 ms (-6.8%), exception composition 3.283 →
3.359 ms (+2.3%), large padding 0.775 → 0.765 ms (-1.3%), small padding
0.346 → 0.296 ms (-14.4%). Scalar results changed direction between trials;
do not claim a stable scalar improvement or universal speedup. Avoiding duplicate
UTF-8 scans and copying complete fill runs removed the prototype's large-padding
regression. Sources, raw samples, budget outcomes and semantic output are in
`bench/results/rendering-ownership.json`; these are admission measurements,
not final comparative performance acceptance.

No hard blocker is known for this checkpoint. The largest effort has been buffer
ownership under pressure while preserving callbacks, failure recovery and durable
state. Tiny phase boundaries and repeated full gates have also cost elapsed time.
Use bounded allocation inventories and feature-sized acceptance batches next.
Preserve the dirty recovered conformance worktree described below.

## On `main` (pushed, `cargo xtask check` green)

MVP (M1–M5, M8) plus M6 (own `no_std` parser, ruff only behind
`ruff-oracle` for `cargo xtask parse-diff`), M7 (stdlib pipeline, packed
bytecode) and M9 (match, complex/cmath, metaclasses, enum, abc, frozenset,
`%` formatting; re, datetime, time, random, hashlib, base64, csv/io,
os/pathlib with host functions; host wall clock and entropy on the facade).

The recovered hardening phase adds panic containment in unwind builds,
worker seccomp, bounded huge-int completion text, WASM snapshot round trips,
GC stress, three more fuzz targets and differential fuzzing. Fuzz findings
fixed: wrong-container bytecode mutations, failed set-growth rollback,
numeric formatting and parsing, and uncharged shared-container repr/str
traversal. Native calls now check safepoints after returning their result,
including keyword, method and unpacked calls. Native rendering still
finishes one memory-bounded operation before parking; it is not interrupted
mid-traversal. Keeping native fuel accounting out of line saves 3,920 bytes
[M] without changing its behavior.

Current [M], Rust 1.99.0 on the continuation orb: CPython-tagged 483/568,
host 61/81; durability 997 runs / 66,784 fresh-process restores identical;
`hello` 2,221,368 bytes (owner-approved 2,250,000-byte budget). All gates in
`cargo xtask check` pass, including both WASM round trips. The hardening
phase's GC-stress run passed: own/corpus outcomes matched, plus 270 durability
runs / 38,253 fresh-process restores, all identical, in 833 seconds overall
[M]. That GC-stress run predates the lifecycle subphase below.

After the architecture review, the owner authorized proceeding through the
revised roadmap to completion. The first resource-lifecycle subphase reclaims
unreachable user compilation units between cells and on reset. Live functions,
generators and saved tracebacks retain their units, including nested functions;
code/source/cache indices are remapped only with no executing frames. Failed
loads roll back partial code, source and cache tables. The retention and
failed-load regressions failed before the fixes and pass; an additional test
verifies retained code after compaction and restore. A repeated-cell probe now
writes a 237,813-byte snapshot
after 256 cells with a 128 KiB comment, and 106,638 bytes after restore/reset
[M]; this is serialized state, not RSS. Full session accounting is NOT done.

Retained code/source and inline-cache capacities now share the arena budget.
Loads reserve before cloning, then reconcile capacity; failed loads and code
reclamation release the reservation. Bundled and synthesized code count too.
Restore rejects over-budget retained state and retains the smaller of the
saved and restoring host's limits for future allocation. The retained-source
regression failed before this change. A tighter-budget probe then found that
decoder vector growth could prevent same-budget restore; exact-capacity
decoding fixes it, covered by a second public-session regression. All six
small-code/constant-heavy probes at 3, 4 and 8 MiB restored after exhausting
their budgets [M]. Final `cargo xtask check` passed every gate; log:
`/tmp/mm-code-budget-final-check.log`.

Source/code restore admission now checks before allocating the tables,
strings, decoded vectors and materialized code values. A bounded compiler
decoder charges decoded storage, not encoded input length, and distinguishes
malformed input from budget exhaustion. Boundary tests and round trips of all
compilable corpus code pass; final `cargo xtask check` passed every gate
(`/tmp/mm-decode-budget-check.log`). The snapshot fuzzer completed 11,413 runs
in 61 seconds without findings [M], starting from an empty corpus; log:
`/tmp/mm-decode-budget-fuzz.log`. This is a smoke run, not extended acceptance.

Register stacks, frame records and continuation tables now share that budget,
including suspended tasks' storage. Task switches move their charge; cell end,
cancel and reset release execution capacity. Continuation unwind slots reserve
up front, and restore admits exact-sized buffers before allocation. GC pressure
includes outside-arena reservations. The recursion and suspended-task restore
regressions fail on the preceding commit and pass now. Tests cover same-budget
restores at host calls and waits, lower-budget rejection, and allocation after
completion/cancel/reset. A probe also passed repeated dunder exhaustion,
snapshot and recovery under 3 MiB, with an 8 MiB success control [M].
`cargo xtask check` passed every gate (`/tmp/mm-frame-budget-check.log`);
snapshot fuzz smoke completed 12,813 runs in 61 seconds without findings [M]
(`/tmp/mm-frame-budget-fuzz.log`).

Scheduler tables and queues now share the budget, reserving all possible task
wakeups and gather callbacks before registration. Callback order is preserved.
Host futures use an id-sorted vector, and task restore avoids an intermediate
copy. The sleeping-task regression failed before admission and passes now.
Repeated/nested gathers and post-exhaustion snapshot/restore/recovery passed
at 3, 4 and 8 MiB [M] (`/tmp/mm-scheduler-probe.log`). Full `cargo xtask check`
passed (`/tmp/mm-scheduler-budget-check.log`), including unchanged conformance,
both WASM round trips and all durability runs. Snapshot fuzz smoke completed
15,080 runs in 61 seconds without findings [M]
(`/tmp/mm-scheduler-budget-fuzz.log`).

Synthesized methods now follow live functions rather than remaining permanent
with the prelude. Their cache stores budgeted, sorted indices into the code's
existing parameters instead of duplicate key strings. The namedtuple regression
failed before reclamation and passes through repeated cells, restores and reset,
including retained generated functions. Full `cargo xtask check` passed
(`/tmp/mm-synth-budget-check.log`); snapshot fuzz smoke completed 15,925 runs in
61 seconds without findings [M] (`/tmp/mm-synth-budget-fuzz.log`).
Instance shapes now share the budget, including restore admission, and unused
shapes are reclaimed between cells. Live instances and their transition ancestors
survive ID compaction. Objects wider than 32 attributes use a dictionary;
conversion clears old inline references as well as the overflow buffer. The
wide-instance probe shrank from 16,358,620 to 364,656 snapshot bytes [M] under
the same 8 MiB limit (`/tmp/mm-shape-{before,after}.log`). Admission, wide-object
metadata and shape-lifetime regressions failed before and pass now. Same-budget
restore and recovery after exhaustion passed at 3, 4 and 8 MiB [M]
(`/tmp/mm-shape-budget-probe.log`). Full `cargo xtask check` passed
(`/tmp/mm-shape-budget-check.log`); snapshot fuzz smoke completed 15,816 runs in
61 seconds without findings [M] (`/tmp/mm-shape-budget-fuzz.log`).

Interned strings are now weak: ordinary roots preserve identity, while dead
literals and names leave the table before sweep reuses their addresses. Shape
names are explicit roots. In-place deletion preserves wrapped probe chains
without a replacement allocation; table capacity remains charged inside the
arena. The persistent-session regression failed before at cell 30 and now
passes 128 distinct 64 KiB literals under 4 MiB, with live functions/attributes,
periodic restores and reset [M]. Full `cargo xtask check` passed
(`/tmp/mm-intern-check.log`). The independent arena model passed 6,208,885
operations, 79,711 collections and 52,003 OOM refusals [M]
(`/tmp/mm-intern-model-long.log`). Arena fuzz smoke completed 288,375 runs in
121 seconds and snapshot fuzz smoke 13,092 runs in 61 seconds without findings
[M] (`/tmp/mm-intern-{arena,snapshot}-fuzz.log`). Full GC-stress passed in
828 seconds, including unchanged own/corpus outcomes and 270 durability runs
with 38,252 fresh-process restores [M] (`/tmp/mm-intern-gc-stress.log`).

GC now traces VM roots directly from their owners rather than copying roots,
task-stack slices and task-frame slices into temporary vectors. Snapshot object
validation likewise checks intern tables during its existing object walk rather
than collecting a separate list. Full `cargo xtask check` passed unchanged
(`/tmp/mm-gc-roots-check.log`); snapshot fuzz smoke completed 17,308 runs in
61 seconds without findings [M] (`/tmp/mm-gc-roots-snapshot-fuzz.log`).
Full GC-stress passed in 866 seconds, including 270 durability runs and 38,252
fresh-process restores [M] (`/tmp/mm-gc-roots-stress.log`).

Collector mark-stack capacity now shares the budget and is reserved before
admitting objects, not while collecting at memory pressure. GC counts survivors
and releases excess capacity with hysteresis. Restore reserves matching
power-of-two capacity classes; an exact-sized restore could fail its first
allocation even when the original session had room. The exhaustion regression
failed before admission, and the zero-spare-budget restore regression caught
that capacity mismatch before the fix. Both now pass. Full `cargo xtask check`
passed (`/tmp/mm-mark-budget-check.log`). The final independent arena model
passed 6,208,885 operations, 79,711 collections and 52,055 OOM refusals [M]
(`/tmp/mm-mark-budget-final-model.log`). Final arena and snapshot fuzz smoke
completed 132,014 runs in 121 seconds and 17,027 runs in 61 seconds without
findings [M] (`/tmp/mm-mark-budget-final-{arena,snapshot}-fuzz.log`). The full
GC-stress run above predates mark-stack admission.

Arena segment descriptors, bitmap descriptors and vacant-index storage now
share the budget too. Allocation reserves all three tables together, including
every index GC could release. Restore uses matching capacity classes and reuses
the descriptors' dirty flags for index validation instead of allocating scratch
tables. Empty spaces allocate no tables, including a zero-budget empty restore.
The byte-only exhaustion regression failed before admission and passes now;
zero-spare-budget restore and allocation still pass. Full `cargo xtask check`
passed (`/tmp/mm-segment-budget-check.log`). The independent arena model passed
6,208,885 operations, 79,711 collections and 52,105 OOM refusals [M]
(`/tmp/mm-segment-budget-model.log`). Arena and snapshot fuzz smoke completed
111,328 and 31,959 runs respectively, each in 121 seconds, without findings [M]
(`/tmp/mm-segment-budget-{arena,snapshot}-fuzz.log`).

Snapshot encoders now stream chunks to a caller-owned sink. Heap live-run
counts use a second scan rather than a per-segment run vector; word endian
conversion uses a fixed 1 KiB stack buffer. This prepares bounded output without
a new codec dependency. Existing Vec-returning entry points remain. A fragmented
heap probe produced byte-identical 9,697,086-byte images; write medians increased
from 1.40–1.44 ms to 1.76–1.80 ms [M, Rust 1.99.0, opt-level 3, fat LTO,
pinned CPU 2, 51 samples per run]. This trades a second scan for removing
data-dependent scratch, not a throughput optimization. Bundled bytecode files
also remain byte-identical. Keeping scalar sink calls out of line brings `hello`
to 2,201,856 bytes [M]. Full `cargo xtask check` passed
(`/tmp/mm-snapshot-stream-final-check.log`); snapshot fuzz smoke completed
32,589 runs in 121 seconds without findings [M] (`/tmp/mm-snapshot-stream-fuzz.log`).

Snapshot output now has a separate capacity cap, defaulting to the session's
memory limit; `Session::snapshot_with_limit` overrides it per call. Streaming
growth preflights capacity and keeps checksum space; failure returns `TooLarge`
without consuming a pending call or poisoning the session. The output belongs
to the caller and does not steal the budget from a nearly full live session.
A 7 MiB session previously produced an 8 MiB-capacity buffer for a 4.3 MB image;
the regression now passes, including limits below the header, one byte short,
exact fit, restores and continued execution [M]. Full `cargo xtask check`
passed (`/tmp/mm-snapshot-output-check.log`); snapshot fuzz smoke completed
31,879 runs in 121 seconds without findings [M] (`/tmp/mm-snapshot-output-fuzz.log`).
Full GC stress passed in 805 seconds, including unchanged own/corpus outcomes
and 270 durability runs / 38,252 fresh-process restores [M]
(`/tmp/mm-snapshot-output-gc-stress-final.log`). This covers collector/segment
admission and both snapshot changes above. It used frozen copies of the verified
`mm` and `xtask` executables: the initial concurrent-build attempt passed own
cases but could not relaunch `xtask`, so the entire stress run was repeated.

Retained host-module names, functions, constant trees and table capacities now
share the budget. Registration admits a clone before allocation and keeps the
old definition on failure. Restore admits exact-sized buffers and keeps cached
charges; the iterative decoder also correctly unwinds nested empty containers.
Registration checks the 1000-container nesting ceiling before cloning. A tight
probe found a definition could fit but leave too little decoder workspace for
same-budget restore; registration now reserves the maximum workspace across
definitions too. The budget regression failed before admission and passes now.
All 18 payload/budget probes at 3, 4 and 8 MiB passed lower-budget rejection,
repeated same-budget restores and release/reset; three 1000-level nesting shapes
passed on a 2 MiB Rust thread stack [M] (`/tmp/mm-host-metadata-probe-final.log`).
The tight case now refuses admission while roomier controls register and restore
(`/tmp/mm-host-tight-probe-after.log`). Full `cargo xtask check` passed
(`/tmp/mm-host-metadata-final-check.log`), with final workspace clippy and host
tests rerun after preserving replacement order. Final snapshot fuzz smoke, now
seeded with nested host constants, completed 33,467 runs in 121 seconds without
findings [M] (`/tmp/mm-host-metadata-delivery-fuzz.log`). The GC-stress run above
predates this host-metadata phase.

Host imports now borrow immutable `Rc<HostModule>` definitions instead of
cloning whole constant trees, function lists and names. Child modules are visited
without collecting names; Rc storage is included in admission and restore.
The 12 MiB byte-constant probe's median peak RSS fell from 40,860 to 28,540 KiB
[M, three fresh processes per build, `/usr/bin/time`, 32 MiB engine budget,
Rust 1.99.0 release]. The no-import controls measured 28,356 and 28,172 KiB.
These are process high-water measurements, not a total-RSS guarantee. Logs:
`/tmp/mm-host-import-{before,after}.log`, with the probe source in the latter.
Existing host/session/snapshot tests pass unchanged; an additional probe passed
nested/overlapping names, mutable constant isolation, replacement, reset, restore
and qualified host calls (`/tmp/mm-host-import-lifecycle.log`). Full
`cargo xtask check` passed (`/tmp/mm-host-import-check.log`); snapshot fuzz smoke
completed 32,161 runs in 121 seconds without findings [M]
(`/tmp/mm-host-import-fuzz.log`). No temporary probe sources/binaries remain.

Bytecode verification now uses a one-bit-per-byte bitmap and reserves its
maximum workspace across retained codes during loading and restore. Frame and
generator PC checks allocate no bitmap. The load-budget regression failed before
admission and passes, including restore with no spare budget. Boundary tests
cover operands, partially filled bitmap bytes and malformed tails. Bytecode fuzzing
then found an argument-binding overflow: excess `__defaults__` are legal Python,
but subtraction in the too-many-arguments error used an unsigned count. A clean
checkout of `aa2b977` reproduced the panic and incorrect default selection on
ordinary Python. The fix uses signed error counts and right-aligned defaults;
the expanded own case matches CPython, including keywords, varargs and empty
defaults (`/tmp/mm-defaults-{before,after,python}.log`). The saved crash passed
ten replays. Final `cargo xtask check` passed (`/tmp/mm-verifier-final-check.log`);
bytecode and snapshot fuzz smoke completed 29,021 and 31,070 runs respectively,
each in 121 seconds without further findings [M]
(`/tmp/mm-defaults-bytecode-fuzz.log`, `/tmp/mm-verifier-final-fuzz.log`).
The temporary baseline worktree was removed. Full GC stress above predates these
verification/default-binding changes.

User-rendered completion text now stays in the arena until taken; the facade
copies only UTF-8-safe prefixes into caller-owned strings. Pending text is traced
and snapshots store its references. The capped-output regression failed before
on retained buffer capacity and passes with asymmetric Unicode strings and
zero/partial-character limits. A public-VM regression verifies untaken text
across stress collection, snapshot/restore, another collection and one-shot take.
Full `cargo xtask check` passed (`/tmp/mm-completion-check.log`); snapshot fuzz
smoke completed 29,794 runs in 121 seconds without findings [M]
(`/tmp/mm-completion-fuzz.log`). Full GC stress passed in 814 seconds: own/corpus
outcomes matched, plus 270 durability runs / 38,354 fresh-process restores [M]
(`/tmp/mm-completion-gc-stress.log`). It used frozen verified executables and
covers the host-definition, verifier/default-binding and completion changes.

Bundled-module import now admits unpacked input, Huffman workspace, the temporary
string pool and the owned decoded program. The maximum across loaded modules
stays reserved for same-budget restore; failed code loads release new workspace.
Restore holds only one decoded module rather than retaining all decoded programs
while cloning their codes. The tight-budget import regression failed before and
passes now, including retry and two restores with no spare budget. Compiler
boundary tests cover pooled/duplicated Unicode strings and unchanged arguments
on failure; unpack tests cover bounded output and malformed input. Full
`cargo xtask check` passed (`/tmp/mm-module-workspace-check.log`), including
unchanged conformance and both WASM round trips. Snapshot fuzz smoke, now with a
multiple-bundled-module base, completed 28,282 runs in 121 seconds without findings
[M] (`/tmp/mm-module-workspace-fuzz.log`). GC stress above predates this phase.

Native string builders now reserve exact output capacity before copying and
keep it charged during arena allocation; cached capacity stays counted too.
Join, replace and f-string budget regressions failed before and pass now, with
roomier success controls and recovery after refusal. Replace sizes actual
nonoverlapping matches, fixing spurious refusal for absent/zero-count matches.
An 8 MiB-budget probe previously built 128 MiB native output before rejecting
it; median peak RSS fell from 135,500 to 4,448 KiB for join and from 135,068
to 3,836 KiB for replace [M, three fresh release processes each]. This is not a
total-RSS guarantee. Pinned alternating string-benchmark medians changed from
37.36 to 38.83 ms [M, 31 samples each, about 4% slower]; raw measurements are in
`bench/results/session-lifecycle.txt`. Full `cargo xtask check` passed unchanged
(`/tmp/mm-string-builder-check.log`); bytecode and snapshot fuzz smoke completed
26,395 and 28,048 runs respectively, each in 121 seconds without findings [M]
(`/tmp/mm-string-builder-{bytecode,snapshot}-fuzz.log`). GC stress above predates
this phase; other collection/rendering buffers remain outside admission.

Fixed type/constructor tables, bundled-module code/global slots and the park
flag now reserve storage before creation or metadata decoding. The initial
register stack is admitted before construction too. Restore checks the fixed
type count before allocating its table. The oversized-catalog regression failed
before and passes now, with lower-budget restore rejection and an exact-budget
round trip. Full `cargo xtask check` passed unchanged
(`/tmp/mm-fixed-metadata-check.log`); snapshot fuzz smoke completed 29,502 runs
in 121 seconds without findings [M] (`/tmp/mm-fixed-metadata-fuzz.log`).
Full GC stress on that frozen build passed in 815 seconds, including unchanged
own/corpus outcomes and 270 durability runs / 38,354 fresh-process restores [M]
(`/tmp/mm-fixed-metadata-gc-stress.log`). It predates iterator accounting below.

Native iterable materialization (`Vm::collect`) now owns its capacity reservation
through nested collections and result allocation. One-shot iterator replay copies
and their table count too. Reservations release independently on drop; joins,
dictionary construction and starred unpacking preserve `MemoryError` instead of
rewriting it as `TypeError`. The iterator budget regression failed before and
passes with roomier success controls and recovery after refusal. Additional probes
passed one-shot sorting through user comparisons, GC stress and host-call snapshot
restores, plus byte-join/nested-dictionary exhaustion and recovery at 4, 8 and
16 MiB [M] (`/tmp/mm-iterator-probe.log`). Core tests cover independent charges,
refused growth, other-heap rejection and cross-thread release.
Full `cargo xtask check` passed unchanged (`/tmp/mm-iterator-check.log`). Final
bytecode/snapshot/arena fuzz smoke completed 24,402 / 27,174 / 47,887 runs in
121 / 121 / 61 seconds without findings [M]
(`/tmp/mm-iterator-{bytecode,snapshot,arena}-fuzz.log`). Borrowed iteration avoids
an extra owned iterator/drop implementation; sharing the available-budget check
out of line keeps the unchanged size gate. Pinned alternating dict/string/JSON
benchmarks measured about 3% / 1% / 1% slower medians [M, 21 samples each]; raw
samples are in `bench/results/session-lifecycle.txt`. Other direct list/tuple
copies, byte builders and other native buffers remain outside admission.

Byte concatenation/repetition, zero-filled construction, join, replace and
padding now reserve output capacity before building and through the arena copy.
The byte-builder regression failed before and passes with 8/16 MiB refusal/success
controls. A differential probe matched all 1,571 output lines against CPython,
including empty/overlapping/binary patterns, counts, signs and widths [M]. Full
`cargo xtask check` passed unchanged (`/tmp/mm-byte-builder-check.log`); bytecode
and snapshot fuzz smoke completed 23,443 / 27,392 runs, each in 121 seconds,
without findings [M] (`/tmp/mm-byte-builder-{bytecode,snapshot}-fuzz.log`). A
272 MiB join/replace probe under the default 256 MiB cap fell from 283,748 to
4,964 KiB median peak RSS [M, three fresh processes], still raising catchable
memory errors and recovering. A pinned mixed byte workload measured 23.709 →
24.693 ms median, about 4% slower [M, 21 alternating samples each]; source and
raw samples are in `bench/results/session-lifecycle.txt`. Byte input copies,
iterable conversion, codecs and rendering remain to be accounted. Full GC stress
on this frozen build passed in 916 seconds, including unchanged own/corpus outcomes
and 270 durability runs / 38,354 fresh-process restores [M]
(`/tmp/mm-byte-builder-gc-stress.log`). It covers iterator and byte-builder changes,
but predates borrowed inputs and case transformations below.

Byte search/count, prefix/suffix checks and predicates now borrow inputs;
integer needles use a stack byte, and affix tuples are visited without copying.
Case transformations borrow their source and mutate one admitted output buffer.
The regression failed before and passes with refusal at 8 MiB, success at
16 MiB and recovery after refusal. A differential probe matched all 4,989
output lines against CPython, normally and under GC stress, including every
byte value, search bounds, integer needles and mixed-type affix tuples [M].
Full `cargo xtask check` passed unchanged (`/tmp/mm-byte-input-check.log`).
Bytecode/snapshot fuzz smoke completed 13,279 / 19,068 runs, each in 121 seconds,
without findings [M] (`/tmp/mm-byte-input-{bytecode,snapshot}-fuzz.log`). Keeping
the shared case-conversion wrapper out of line saves 1,984 bytes [M] versus
inlining it, preserving the existing size gate. A pinned mixed search/case workload
measured 45.212 → 42.938 ms median, about 5% faster [M, 21 alternating samples
each]; source and raw samples are in `bench/results/session-lifecycle.txt`.

Join, replace and padding now borrow their remaining byte inputs; split
separators, strip sets and removal affixes also avoid copies. No-growth padding
returns the immutable input after argument validation. The memory-pressure
regression fails with the old copy behavior and passes now. Differential probes
match all 1,728 output lines against CPython, normally and under GC stress [M].
Full `cargo xtask check` passed unchanged (`/tmp/mm-byte-borrow-check.log`);
bytecode/snapshot fuzz smoke completed 24,616 / 27,756 runs, each in 121 seconds,
without findings [M] (`/tmp/mm-byte-borrow-{bytecode,snapshot}-fuzz.log`). A
32 MiB replacement probe's median peak RSS fell from 200,896 to 102,652 KiB
[M, three fresh processes]; the pinned mixed-builder workload measured
33.614 → 31.236 ms median, about 7% faster [M, 21 alternating samples each].
Source and raw samples are in `bench/results/session-lifecycle.txt`. The full
GC-stress run above predates this subphase.

Iterable-to-byte conversion and hex decoding now reserve their native output
through arena allocation. Hex encoding borrows its input and uses the admitted
string builder. Decoding counts non-whitespace bytes before reserving, so a
whitespace-heavy input is charged for its actual decoded length. The regression
failed before hex admission; independently disabling the decoding and iterable
reservations also makes it fail. Refusal, roomier success and recovery now pass.
The 1,882-line differential probe matches the preceding build, normally and under
GC stress [M]. This is not full CPython parity: existing divergences include
`b'ab'.hex(None)`, vertical tabs in `bytes.fromhex`, odd-digit inputs with
whitespace (`'0 '`, `'0 12'`), and unsupported user `__index__` in `bytes()`.
Keep their semantic fixes in the selective-conformance phase.
Full `cargo xtask check` passed unchanged (`/tmp/mm-byte-conversion-check.log`),
including both WASM round trips. Bytecode/snapshot fuzz smoke completed
18,793 / 27,009 runs, each in 121 seconds without findings [M]
(`/tmp/mm-byte-conversion-{vm,snapshot}-fuzz.log`). Pinned alternating medians
were hex 56.848 → 16.703 ms, fromhex 11.149 → 13.233 ms, iterable conversion
9.242 → 9.179 ms [M, 21 samples each]. Exact decoding admission adds a scan,
about 19% overhead in this workload; it is not a throughput improvement.
Sources and raw samples are in `bench/results/session-lifecycle.txt`. Full
GC stress above predates these changes.

Byte slicing, splitting, partitioning, stripping and affix removal now borrow
their sources and reuse unchanged immutable byte ranges. Small contiguous pieces
use a fixed 256-byte stack buffer; large pieces and strided slices reserve output
through arena allocation. Split tables use independently charged native values
through list allocation; partition uses a fixed array. Budget and unchanged-result
regressions failed before and pass now. Disabling only the split-table reservation
also makes the budget regression fail. The 5,590-line probe matches CPython,
normally and under GC stress [M], including split direction, limit/whitespace
boundaries, binary delimiters, invalid arguments and the stack-buffer boundary.
Full `cargo xtask check` passed unchanged (`/tmp/mm-byte-parts-check.log`), with
both WASM round trips. Final bytecode/snapshot fuzz smoke completed
19,230 / 26,935 runs, each in 121 seconds without findings [M]
(`/tmp/mm-byte-parts-delivery-vm-fuzz.log`, `/tmp/mm-byte-parts-snapshot-fuzz.log`).
Pinned split/slice medians were 9.243 → 11.137 ms (about 20% slower) and
14.022 → 9.101 ms (about 35% faster) [M, 21 alternating samples each]. Fixed
stack scratch and scanning each line through one borrow avoided the initial
roughly 2× split slowdown; remaining admission/lookup overhead is measured, not
hidden. The million-field split's median peak RSS fell from 69,012 to 60,020 KiB
[M, three fresh processes]. Sources and raw samples are in
`bench/results/session-lifecycle.txt`. Full GC stress above predates these changes.
Byte codecs and rendering buffers remain outside admission.

Extreme signed slice steps no longer panic and poison a debug session.
`slice_indices` takes an unsigned magnitude instead of negating `i64::MIN`;
selection, assignment and deletion saturate the unused cursor advance past the
last item. The actual Python step and selected positions stay unchanged.
Both negative-step negation and positive-step addition failures were reproduced
before the fix (`/tmp/mm-slice-{extreme,positive}-before.log`). The existing
slicing case now covers list, tuple, string and bytes, assignment/deletion and
empty slices at both signed boundaries. A 4,608-line boundary probe matches
CPython in debug builds normally and with GC stress [M]. Full `cargo xtask check`
passed (`/tmp/mm-slice-extreme-check.log`), including unchanged conformance,
both WASM round trips and 997 durability runs / 65,810 fresh-process restores [M].
Bytecode fuzz smoke completed 18,944 runs in 121 seconds without findings [M]
(`/tmp/mm-slice-extreme-vm-fuzz.log`).
Full GC stress on this frozen build passed in 885 seconds, including unchanged
own/corpus outcomes and 270 durability runs / 38,388 fresh-process restores [M]
(`/tmp/mm-slice-gc-stress.log`). It covers byte conversions, pieces and slice
boundaries, but predates the codec refactor below.

Codecs now borrow source strings/bytes, and encoding iterates Unicode scalars
without materializing a character vector. The 8,544-line probe matches the
preceding build in debug, release and debug GC stress [M], across constructor
and method callers, aliases, error policies, all byte values and malformed UTF-8.
CPython differences in this probe are pre-existing: unknown codecs on empty
byte input and surrogate-pass decoding. Full `cargo xtask check` passed unchanged
(`/tmp/mm-codec-input-check.log`); bytecode fuzz smoke completed 15,648 runs in
121 seconds without findings [M] (`/tmp/mm-codec-input-vm-fuzz.log`). Pinned
encode/decode medians were 31.221 → 27.060 ms (about 13% faster) and
31.896 → 30.012 ms (about 6% faster) [M, 21 alternating samples each].
The 8 MiB ASCII encode/decode probes' median peak RSS fell from 61,756 to
28,908 KiB and from 37,176 to 29,000 KiB [M, three fresh processes each].
Sources and raw samples are in `bench/results/session-lifecycle.txt`. This is
copy removal, not codec output or codec-name/error-handler buffer admission.

Codec outputs now reserve exact native capacity through the arena copy, including
error-policy and Latin-1 expansion. Large ignored inputs reserve empty output,
not a worst-case expansion. The budget regression failed before admission;
independently disabling non-UTF-8 encoder or decoder admission also makes it fail.
Refusal, roomier success and recovery now pass. The 8,544-line differential probe
still matches the preceding build normally and under GC stress [M]. Full
`cargo xtask check` passed (`/tmp/mm-codec-output-check.log`), including both WASM
round trips. Bytecode/snapshot fuzz smoke completed 15,342 / 27,681 runs, each
in 121 seconds without findings [M] (`/tmp/mm-codec-output-{vm,snapshot}-fuzz.log`).
Resolving error-handler names once avoids repeated string matching in both scans
and preserves the size gate; unknown handlers still fail only on invalid input.
Pinned encode/decode medians were 27.348 → 33.530 ms (about 23% slower) and
30.058 → 30.449 ms (about 1% slower) [M, 21 alternating samples each]. Exact
admission adds a sizing scan; this is not a throughput improvement. A 288 MiB
backslash-decoding output under the default 256 MiB cap now fails before building
it: median peak RSS fell from 372,872 to 151,484 KiB [M, three fresh processes],
with catchable memory errors and recovery in both builds. Sources and raw samples
are in `bench/results/session-lifecycle.txt`. Codec-name/error-handler buffers and
rendering remain outside admission; full GC stress above predates both codec phases.

Codec and error-handler names now stay borrowed, including constructor callers.
Normalization uses a fixed ten-byte buffer, sufficient for every supported alias;
name-bearing lookup errors use the admitted string builder. The 10 MiB-budget
regression failed before and passes now, with a 16 MiB success control, lazy
unknown-handler validation and recovery. The original codec probe plus 1,668
argument/alias cases match all 10,212 output lines against the preceding build,
normally and under GC stress [M]. These preserve existing semantics, not full
CPython parity. Full `cargo xtask check` passed (`/tmp/mm-codec-names-check.log`).
Bytecode/snapshot fuzz smoke completed 16,339 / 27,262 runs in 122 / 121 seconds
without findings [M] (`/tmp/mm-codec-names-{vm,snapshot}-fuzz.log`). A 32 MiB
invalid codec name's median peak RSS fell from 135,464 to 102,676 KiB [M, three
fresh processes per build], with the same full error-message length and recovery.
Source and samples are in `bench/results/session-lifecycle.txt`. Other rendering
and native scratch remain unaccounted; full GC stress above predates these changes.

Full GC stress on the frozen codec-name build passed in 869 seconds, including
unchanged own/corpus outcomes and 270 durability runs / 38,388 fresh-process
restores [M] (`/tmp/mm-codec-gc-stress.log`). It covers all three codec phases
above, but predates the rendering refactor below.

String leaves now append their quoted form directly to the enclosing renderer;
byte leaves borrow their input instead of copying it. Output and traversal limits
are unchanged. The 3,992-line differential probe matches the preceding build in
debug, release and debug GC stress [M], including byte values, Unicode escaping,
quote selection and cycles. This preserves existing output, not full Unicode
parity with CPython. Full `cargo xtask check` passed unchanged
(`/tmp/mm-render-input-check.log`); bytecode/snapshot fuzz smoke completed
12,389 / 26,857 runs, each in 121 seconds without findings [M]
(`/tmp/mm-render-input-{vm,snapshot}-fuzz.log`). Pinned mixed-rendering medians
were 66.223 → 66.882 ms [M, 21 alternating samples each], about 1% slower;
no throughput improvement is claimed. Source and raw samples are in
`bench/results/session-lifecycle.txt`. Container copies, traversal workspace
and rendering output still need admission.

Native container `repr` now visits entries through cursors, without copying
whole containers or scheduling all children at once. Cached backing-buffer
locations are safe only because native rendering neither mutates nor collects;
Python rendering still hands off. Delimiters, cycle handling and fuel charges
for values/literals remain unchanged. The 525-line probe matches the preceding
build in debug, release and debug GC stress, and all 146 hand-checked capped
completion cases pass normally and under GC stress [M]. This is baseline parity:
set layout after deletion/reinsertion and self-referential exception `repr` differ
from CPython already. Full `cargo xtask check` passed unchanged
(`/tmp/mm-render-walk-final-check.log`); bytecode/snapshot fuzz smoke completed
13,565 / 25,779 runs in 136 / 121 seconds without findings [M]
(`/tmp/mm-render-walk-final-{vm,snapshot}-fuzz.log`). Pinned mixed-container
medians were 13.617 → 14.330 ms, about 5% slower [M, 21 alternating samples each].
Caching the backing buffers removed the initial 33% slowdown. Million-item list
rendering fell from 72,848 to 19,948 KiB median peak RSS [M, three fresh processes
each]. Source and raw samples are in `bench/results/session-lifecycle.txt`.
Traversal/output buffers and the completion preflight's child copies still need
admission; full GC stress above predates both rendering refactors.

Nested exception arguments now hand string rendering to prelude bytecode rather
than recurse in Rust. Ordinary arguments keep the native path; completion and
uncaught-error reporting recognize the handoff too. The regression in
`conformance/own/17_exceptions_basic.py` aborts the preceding commit with Rust
stack overflow (exit 134), but now matches CPython for direct/mutual cycles,
both OSError fields, str/format/f-string callers and recovery [M]. A first fixed
native-depth guard was rejected because it broke valid 200/300-level messages;
the fallback preserves them and uses VM recursion limits and fuel. On a 2 MiB
Rust stack, normal and explicit GC-stress probes passed reporting, recovery and
eight mid-recursion snapshot/restores each [M] (`/tmp/mm-exception-thread-probe.log`).
Full `cargo xtask check` passed (`/tmp/mm-exception-check.log`), including both
WASM round trips and 997 durability runs / 66,807 fresh-process restores [M].
Bytecode/snapshot fuzz smoke completed 15,337 / 25,676 runs, each in 121 seconds,
without findings [M] (`/tmp/mm-exception-{vm,snapshot}-fuzz.log`).
The capped-completion probe above was also rerun with `Session::set_gc_stress`
explicitly enabled: the CLI environment flag alone had not enabled stress in
that standalone Rust probe. All 146 cases passed [M]
(`/tmp/mm-render-walk-actual-stress-caps.log`). The Python CLI probes did enable
stress correctly. Full GC stress on the frozen exception-fix build passed in
876 seconds, including unchanged own/corpus outcomes and 270 durability runs /
39,380 fresh-process restores [M] (`/tmp/mm-render-gc-stress.log`). This covers
the rendering refactors and exception handoff, before capped buffer growth.

Native `repr` now bounds output growth within leaves, keeping at most one UTF-8
character past the requested limit so exact fits and opaque host-budget refusal
remain distinguishable. Completion buffers release capacity above their limit.
The public regression failed before and passes now. Debug/release probes each
passed 610 boundary/opaque checks, including explicit GC stress; 4,168 escaping
output lines matched CPython and 527 container lines matched the preceding build,
normally and under GC stress [M]. Full `cargo xtask check` passed unchanged
(`/tmp/mm-capped-render-final-check.log`); final bytecode/snapshot fuzz smoke
completed 14,201 / 26,596 runs, each in 121 seconds without findings [M]
(`/tmp/mm-capped-render-final-{vm,snapshot}-fuzz.log`). The 16 MiB byte-completion
probe with a ten-byte text limit fell from 167,864 to 36,536 KiB median peak RSS;
both returned capacities fell from 128 MiB to ten bytes [M]. Pinned medians rose
21% for printable Unicode, 10% for mixed leaves and 13% for containers, while
ASCII bytes improved 13% [M, 21 alternating samples]. The first per-character
writer slowed mixed leaves 60%; shared quoting, printable runs and stack-buffer
escaping reduced that cost while preserving the size gate. Source and samples:
`bench/results/session-lifecycle.txt`. This bounds the output builder, not other
rendering helpers, traversal workspace or total session memory. Full GC stress
above predates this subphase.

Completion preflight now borrows child slots and exception arguments instead
of copying whole containers into its work queue. One cursor per ancestor keeps
the old last-child-first visitation and depth/cycle guards. Read-only traversal
keeps these borrows valid; it neither mutates nor collects. The targeted probe
matches completion text, callbacks, remaining fuel and snapshot/restore outcomes
against the preceding build, normally and under explicit GC stress. All 610
capped-output/opaque-conversion checks also pass [M]
(`/tmp/mm-preflight-{after,caps}.log`). Full `cargo xtask check` passed unchanged
(`/tmp/mm-preflight-check.log`), including both WASM round trips and all durability
runs. Bytecode/snapshot fuzz smoke completed 15,194 / 26,683 runs, each in
121 seconds without findings [M] (`/tmp/mm-preflight-{vm,snapshot}-fuzz.log`).
Million-item list completion with a ten-byte text limit fell from 35,104
to 19,624 KiB median peak RSS, and from 18.214 to 13.349 ms execution time [M,
21 alternating fresh-process samples pinned to CPU 2]. Source description and raw
samples are in `bench/results/session-lifecycle.txt`. Shared backing-storage lookup
avoids redundant set-header reads and preserves the unchanged size gate. Cursor
and visited-set storage remain outside admission; this is copy removal, not a
total memory or latency bound. Full GC stress above predates this subphase.

Native exception `str` now borrows its arguments too. OSError copies only the
individual field values it needs before rendering; nested exceptions still hand
off to prelude bytecode. Shared out-of-line slot decoding keeps the size gate
without duplicating validation across text walks. The 1,222-line exception probe
matches the preceding build normally and under GC stress [M], including custom
renderers, zero through six arguments, filename combinations and 300-level
exception nesting (`/tmp/mm-exc-args-{before,after,after-stress}.log`). The completion
probe still matches text, fuel, callbacks and restores under both GC modes
(`/tmp/mm-exc-args-preflight.log`). Full `cargo xtask check` passed unchanged
(`/tmp/mm-exc-args-check.log`). Bytecode/snapshot fuzz smoke completed
12,964 / 28,049 runs, each in 121 seconds without findings [M]
(`/tmp/mm-exc-args-{vm,snapshot}-fuzz.log`). Wide-exception and full-completion medians were
46.381 → 45.218 ms and 123.109 → 123.003 ms [M, 21 alternating samples, CPU 2];
neither showed a material peak-RSS improvement. The completion measurement
includes full rendering and JSON serialization, not isolated preflight overhead.
Source descriptions and raw samples are in `bench/results/session-lifecycle.txt`.
This removes temporary argument copies; output helpers and traversal workspace
still need admission. Full GC stress above predates this subphase.

Native format-width requests and precision-sized complex components now
preflight before allocating; large fixed complex precision shares the bounded
float helper. The audit reproduced three process aborts under a 1 GiB OS limit:
`format(1, '1099511627776d')`, `format(1+2j, '.65536f')` and
`format(1+2j, '.1073741824e')`. They now respectively raise catchable
`MemoryError`, produce the correct 131,078-character result, and raise catchable
`MemoryError`, normally and under GC stress [M]. The allocation preflight's
large-block overhead could also overflow near `u64::MAX`; checked addition fixes
the failing boundary regression. Public tests cover byte-aware fill admission,
grouping, f-strings, `.format`, custom renderers and cheap huge-precision results.
Width admission is conservative and precedes type-specific validation; this
does not yet reserve every formatting buffer. Format-spec parsing was left
unchanged in this phase to fix these aborts first; its refactor follows below.
Full `cargo xtask check` passed (`/tmp/mm-native-format-check.log`), with unchanged
conformance, both WASM round trips and all durability runs. The repro also survives
fresh-process restore in each GC mode. VM/snapshot fuzz smoke completed
10,945 / 26,752 runs in 122 / 121 seconds without findings [M]. The formatting
microbenchmark median changed 11.926 → 12.227 ms (+2.5%); the embedding-workload
process median changed 8.120 → 8.077 ms [M, CPU 2, 21 alternating samples].
Every checksum matched; raw samples and source are in
`bench/results/session-lifecycle.txt`. Full GC stress above predates this subphase.

Format-spec parsing now borrows UTF-8 slices instead of creating a character
vector and copied numeric fields. Unicode fill/type, malformed fields, overflow,
and long leading-zero fields retain their behavior: 4,006 normal cases and 128
GC-stress cases match the preceding CLI byte-for-byte [M]. Full `cargo xtask check`
passed (`/tmp/mm-spec-check.log`); VM/snapshot fuzz smoke completed 12,803 / 27,647
runs, each in 121 seconds without findings [M]. The embedding workload passes
with explicit GC stress, including all six fresh-session restores [M]
(`/tmp/mm-spec-embedding-stress.log`). This is not a full GC-stress corpus rerun.
The short-format process median fell 12.122 → 10.846 ms (10.5%); million-leading-zero
width/precision medians fell 7.299 → 5.014 / 8.951 → 5.326 ms, with median peak RSS
12,368 → 7,248 / 13,404 → 8,252 KiB [M, CPU 2, 21 alternating samples]. Short-case
RSS did not improve. Sources and raw samples are in `bench/results/session-lifecycle.txt`.
This parser phase left caller copies and other formatting/traversal workspace
unchanged. `hello` remained
2,209,824 bytes [M]; a string-pattern variant exceeded the unchanged size gate
and was rejected in favor of direct byte checks for the grammar's ASCII fields.

Builtin `format` and f-strings now pass their arena spec value directly. The
formatter accepts either that value or native `str.format`'s existing field
slice, avoiding an unnecessary arena allocation for field text. Parsing finishes
before mutating the VM; custom rendering still hands off before native parsing.
All 1,812 probe cases match the preceding CLI normally and under GC stress [M],
including builtin subclasses, custom renderers/errors, conversions and all three
formatting entry points. Full `cargo xtask check` passed
(`/tmp/mm-format-spec-check.log`); VM/snapshot fuzz smoke completed 6,609 / 27,137
runs in 127 / 121 seconds without findings [M]. The explicit-GC-stress embedding
workload also passed, including all six restores [M]. `hello` is 2,209,968 bytes,
only 32 bytes below the unchanged gate [M]. Pinned process medians were short
formatting 11.290 → 11.256 ms and long-spec builtin/f-string formatting
6.884 → 6.822 ms [M, 21 alternating samples]. Timing and peak RSS did not materially
improve; raw samples and source are in `bench/results/session-lifecycle.txt`.
Native `str.format` template/field storage, result buffers and traversal workspace
still need admission. Full GC stress above predates these formatting subphases.

The large-operation latency probe found a scheduling gap: 100 native sorts of
250,000 integers returned to the next host call without honoring a pending park
request, with median request-to-control latency 1,328.310 ms [M]. Native sort
now charges comparisons, and substantial native work brings forward polling even
when fuel is unlimited. The same probe now parks after the first sort, with a
12.298 ms median [M]. The remaining single-call duration is not interrupted.
Public regressions reproduce the missing sort fuel and the unlimited-fuel request
gap; restoring the old fuel-dependent polling condition makes the latter fail.
Requests, deadlines, fuel exhaustion and snapshot/resume pass after the fixes.
Full `cargo xtask check` passed (`/tmp/mm-native-poll-check.log`), including
66,778 fresh-process restores [M]. VM/snapshot fuzz smoke completed 9,384 / 26,648
runs, each in 121 seconds, without findings [M]. Explicit GC-stress embedding
acceptance passes too. `hello` exactly meets the size gate at 2,210,000 bytes [M].
Charging computed merge-pass work initially produced 2,210,080 bytes [M]; counting
actual comparisons fits and avoids adding work to the VM's dispatch loop.

`bench/latency.rs` preserves the standalone public-facade driver and build recipe.
Raw measurements and caveats are in `bench/results/native-latency.txt`: CPU affinity
2,3, release with the CLI's optional modules, one warmup and seven samples, actual
request timestamps, explicit distinction between a host event and a requested park.
Both recorded runs pass every output assertion. Single native repr/replace calls
still hold control for tens to over a hundred milliseconds on these large inputs.
At 100,000 persistent dict rows, baseline median cell start, snapshot (including
GC) and restore were 1.765 / 2.402 / 1.915 ms [M]. Shared strings and this one graph
shape do not bound arbitrary heaps. The before/after runs are sequential, not a
throughput regression study; other timing differences are not attributed to the
fix. Full GC stress above predates this phase, and sort's temporary buffers still
need admission.

Other native scratch and code/shape reclamation workspace still need accounting.
Owned-capacity admission does not cap allocator overhead or transient realloc peaks.

Parser-only `opt-level = "z"` recovers 14,704 bytes [M] of size headroom at
roughly 5–8% more compile time for a small cell and 10–12% for a 50 KB cell.
The compiler remains `s`, the VM `3`; see `session_bench` and raw samples in
`bench/results/session-lifecycle.txt`. Benchmark exceptions now fail the gate
instead of being accepted as "unsupported"; its CLI regression also failed
before the fix and passes. The complete lifecycle tree passed `cargo xtask
check`; log: `/tmp/mm-lifecycle-check.log`.

Further profile experiments at `b416519` did not justify changing these settings.
Core `s`/`z` and compiler `z`/`3` all grew the default-feature hello binary.
Engine `2` saved 6,688 bytes but slowed the loop median by 6.3%, beyond the 5%
regression threshold [M, CPU 2, 21 alternating samples]. Every benchmark checksum
matched; raw samples and exact sizes are in `bench/results/session-lifecycle.txt`.
No override was retained, and the normal size build was restored and rechecked.
Build hello separately from mm-cli when comparing sizes: the CLI's optional
stdlib features otherwise unify into hello and change what is being measured.

`cargo run --release --locked -p micro-monty --example embedding_bench` now
exercises a representative public-facade session: overlapping host pages,
string cleanup, filtering and joins; 32 persistent queries reusing compiled
code; reverse-order asynchronous batch replies with one caught host error;
an uncaught error, cancellation, recovery and reset. Six snapshots drop the old
session before restoring, including an outstanding page and pending futures.
The driver checks exact independent checksums, result order and host-call count.
The Python computations agree with CPython 3.14; temporarily disabling page
deduplication made the workload fail as intended (117 rows instead of 115).
Normal and `MM_GC_STRESS=1` runs pass. The normal workload is also part of
`cargo xtask check`; it fails on incorrect results, not on timing thresholds.
The complete gate passed, including both WASM round trips and 997 durability
runs / 66,807 fresh-process restores [M] (`/tmp/mm-embedding-check.log`).

On CPU 2, three warmups followed by 21 fresh release processes gave median
totals of 0.825 ms for session creation, 0.247 ms for seven compilations,
1.349 ms for 32 persistent queries, 0.819 ms for six snapshots and 2.096 ms
for six restores [M]. Largest observed individual start/resume calls were
0.179/0.290 ms; these small fixtures do **not** bound park latency for large
native operations. Snapshot sizes in lifecycle order were 187126, 117924,
115133, 118596, 116994 and 105157 bytes [M]. Raw measurements and summaries:
`bench/results/embedding.txt` (engine `b416519`, default features, Rust 1.99.0,
`panic=abort`). A separate 21-process run with `/usr/bin/time` measured median
peak RSS of 6,740 KiB [M], including the executable, caller data and allocator.
`query-cell` includes its start/resume and driver work; stage totals overlap
and must not be summed. Resume includes host conversion and execution; fixture
construction excludes real I/O. Restore is a new session in the same process,
not a fresh-process measurement. Serialized size is not peak RSS or total memory
admission; mock host effects do not establish exactly-once external effects.

The bytecode fuzzer completed 78,834 inputs in 301 seconds [M]. All three
saved bytecode timeouts replay successfully; the latest shared-container
timeout completes ten replays in 11.6 seconds [M], after previously timing
out on one input. These are short campaigns, not M10's 24-hour acceptance.
Final-build JSON smoke completed 44,731 inputs; nested-data corpus
initialization replayed 747 inputs, both without findings [M].
Differential smoke remains 97/100 identical [M]: Unicode decimal digits in
`int(chr(3562), 0)`, nonprintable Unicode in `int(chr(6892))` error text,
and true titlecase (`'ß'.title()`) remain conformance work. The differential
harness exits successfully even when it reports divergences.

External plak suites have not been rerun in this continuation. Historical
results [S]: 44/48 at `ac3373b`, 45/48 after huge-int rendering (see
`docs/plak-integration.md`). Local worker protocol and seccomp tests pass.

## Sort accounting (verified)

Native sorting now admits its merge workspace, and `list.sort` admits its input
copy. The successful sort reuses the list's backing store rather than allocating
another arena buffer. Refusal leaves the original order unchanged, including
reverse sorts. The public regression passes with 8/16 MiB refusal/success controls,
recovery and restore; independently disabling either reservation makes it fail
(`/tmp/mm-sort-{workspace,input}-negative.log`). The baseline also demonstrated
the previously unaccounted success at 8 MiB. All 305 differential output lines
match the preceding build normally and under GC stress, including stable mixed
numeric sorts, reverse, NaNs, errors, custom comparisons, one-shot iterators,
JSON keys, aliases and live iterators [M]. This preserves baseline semantics,
not full CPython parity.

The first `cargo xtask check` failed only the old size ceiling: `hello` is
2,210,552 bytes, 552 bytes over it [M] (`/tmp/mm-sort-workspace-check.log`).
After the owner approved a 2,250,000-byte ceiling, the full command passed
(`/tmp/mm-sort-workspace-approved-check.log`), including embedding, both WASM
round trips, unchanged conformance, 997 durability runs / 66,778 fresh-process
restores and quick benchmark checksums [M]. VM/snapshot fuzz smoke completed
9,600 / 26,154 runs, each in 121 seconds, without findings [M]
(`/tmp/mm-sort-workspace-{vm,snapshot}-fuzz.log`).

Pinned alternating sort-workload medians were 187.468 → 182.149 ms (2.8% faster),
with peak RSS 8,368 → 6,880 KiB [M]; sources and raw samples are in
`bench/results/session-lifecycle.txt`. This is a modest improvement, not a
structural speedup. The latest WASM hello sizes are 1,809,942 bytes (wasip1) and
1,919,861 bytes (unknown-unknown) [M]. Full GC stress on the frozen sort-accounting
build passed in 791 seconds: own/corpus outcomes matched, and 270 durability
runs / 39,381 fresh-process restores were identical [M]
(`/tmp/mm-sort-gc-stress.log`). This covers all accumulated formatting, scheduling
and sorting changes above, but predates the JSON output phase.
The 2,250,000-byte native ceiling preserves a smallness target without spending
more time optimizing away this small correctness cost. Profile settings have
not changed.

## JSON output accounting (verified)

Native JSON encoding now admits output capacity before growth and holds the
reservation through the arena copy. Quoting sizes actual escapes rather than
worst-case expansion; indentation preflights depth × UTF-8 bytes. Separators and
indentation stay in the arena. Geometric growth falls back to exact capacity when
slack cannot fit. The public regression failed before admission and passes with
12/20 MiB refusal/success, recovery, default callbacks and Unicode controls [M].
Output bytes also bring forward control polling for containers with a single
large leaf; the request/deadline regression failed before that charge and now
passes with snapshot/restore. A separate refusal-path control probe passed too.

All 1,717 differential output lines match CPython 3.14 and the preceding build,
normally and under GC stress [M]. Full `cargo xtask check` passed
(`/tmp/mm-json-output-delivery-check.log`), with unchanged conformance and
997 durability runs / 66,778 fresh-process restores [M]. An earlier run failed
only the existing match-pattern fixture's address prefix (`0x7f` versus `0x7e`);
the unchanged targeted rerun and then the entire gate passed. The fixture still
needs stable output; it was suggested separately, not masked or changed here.
Final JSON/snapshot fuzz smoke completed 66,577 / 27,004 runs, each in 121 seconds,
without findings [M] (`/tmp/mm-json-output-final-{json,snapshot}-fuzz.log`).
Native `hello` is 2,211,784 bytes; WASM hello is 1,811,576 bytes (wasip1) and
1,921,786 bytes (unknown-unknown), with both same-build round trips passing [M].

Pinned alternating medians changed from 95.187 to 102.478 ms for JSON rows and
24.251 to 26.405 ms for an 8 MiB ASCII string (+7.7% / +8.9%) [M]. The first
implementation's overhead was 14% / 37%; reusing the sizing result to bypass
unneeded escaping scans and keeping one borrow on no-growth appends reduced it.
Byte/chunk sizing experiments did not improve it and were discarded. A rejected
288 MiB escaped-output probe fell from 397,504 to 200,956 KiB median peak RSS,
with catchable errors and recovery on both builds [M]. ASCII peak RSS did not
improve. Sources, raw samples and measurement conditions are in
`bench/results/session-lifecycle.txt`. This is memory admission, not a throughput
improvement or a hard latency bound. Container copies, traversal tables, decoder
workspace and other native scratch remain unaccounted. Full GC stress above
predates JSON; this phase ran the differential probe under GC stress, not the
whole stress suite.

Native JSON encoding now walks lists, tuples and unsorted dictionaries through
bounded arena-storage cursors. It neither collects nor runs user code, so the
cached references stay valid; `default` still restarts through the prelude.
Sorted dictionaries keep their existing copied representation. The original
1,717-line probe and a 43,941-line container probe match the preceding build,
normally and under GC stress [M]. The latter covers buffer boundaries, sparse
dictionaries, deletion/reinsertion order, aliases, cycles, depth, skipped keys,
sorting and a default callback that mutates its input. It preserves an existing
CPython divergence: an indented dictionary whose keys are all skipped emits
blank/indented lines inside its braces rather than `{}`. Fix that separately in
the conformance work, not in this copy-removal refactor.

Full `cargo xtask check` passed (`/tmp/mm-json-cursor-check.log`), including
unchanged conformance, both WASM round trips and 997 durability runs / 66,778
fresh-process restores [M]. JSON/snapshot fuzz smoke completed 60,297 / 24,079
runs, each in 121 seconds without findings [M]
(`/tmp/mm-json-cursor-{json,snapshot}-fuzz.log`). Native hello is 2,212,984 bytes;
WASM hello is 1,812,812 (wasip1) / 1,922,868 (unknown-unknown) bytes [M].
Pinned alternating medians were 111.163 → 108.117 ms for rows (2.7% faster)
and 33.586 → 30.936 ms for a million-item list (7.9% faster) [M]. The latter's
median peak RSS fell from 30,968 to 23,864 KiB; rows did not materially improve.
Sources and raw samples are in `bench/results/session-lifecycle.txt`. Sorted
copies, traversal tables and decoder workspace still need admission; this is
copy removal, not a total-memory bound. Full GC stress above predates JSON.

Sorted JSON keys and ordered entries now use the existing independently charged
native-value storage. Key collection borrows dictionary slots instead of making
unsorted entry copies first. Entries remain charged while their dictionary is
encoded. The public regression failed on the preceding commit and passes with
6.25/8 MiB refusal/success controls, exact output checks, unchanged input,
unsorted recovery and snapshot/restore [M]
(`/tmp/mm-json-sort-test-{before,after}.log`). A separate budget sweep found
three previously unaccounted successes; other outcomes remained unchanged
(`/tmp/mm-json-sort-budget-{before,after}-fine.log`, printed budgets in 256 KiB
units). The 43,941-line container probe still matches normally and under GC
stress, and the original 1,717-line output probe matches too [M].

Full `cargo xtask check` passed (`/tmp/mm-json-sort-check.log`), including
unchanged conformance, both WASM round trips and all durability runs. Final
JSON/snapshot fuzz smoke completed 61,310 / 18,872 runs in 121 seconds each,
without findings [M] (`/tmp/mm-json-sort-{json,snapshot}-fuzz.log`). Native hello
is 2,213,168 bytes; WASM hello is 1,812,977 (wasip1) / 1,923,013
(unknown-unknown) bytes [M]. The sorted-JSON workload measured 95.128 → 93.180 ms
median (2.0% faster), with peak RSS 16,620 → 15,828 KiB [M]. Sources and raw
samples are in `bench/results/session-lifecycle.txt`; these modest improvements
are not total-memory or latency guarantees. Traversal tables and decoder
workspace remain unaccounted. Full GC stress on frozen executables
`/tmp/mm-json-gc-mm` and `/tmp/mm-json-gc-xtask` passed in 825 seconds,
including unchanged own/corpus outcomes and 270 durability runs / 39,381
fresh-process restores [M] (`/tmp/mm-json-gc-stress.log`). This covers all three
JSON encoder phases, but predates decoder accounting below.

## JSON decoder accounting (verified)

The scanner now admits its native input copy before allocation and holds that
charge until scanning finishes. Array values and object pairs use the existing
charged native-value buffer through arena container construction, replacing the
periodic count-based estimate. The input copy remains necessary while scanning
mutates the VM. The public regression failed on the preceding build and passes
with input/array 8/16 MiB and duplicate-key object 10/16 MiB refusal/success
controls [M]. Independently disabling array or object admission makes the
respective fixture fail; the object fixture's initial 8 MiB limit did not isolate
its reservation and was changed to a discriminating 10 MiB case.

All 370 differential output lines match the preceding build normally and under
GC stress [M], including malformed input, Unicode, buffer boundaries, duplicate
keys and memo boundaries. The only CPython difference in that probe is the
pre-existing substitution of lone surrogates with U+FFFD. Budget sweeps at
6/8/10/12/16 MiB pass same-budget snapshot/restore and small-JSON recovery
for every admitted input, normally and with explicit GC stress [M]
(`/tmp/mm-json-decode-recovery{,-stress}.log`). Pinned alternating JSON-row
medians were 108.102 → 113.053 ms (+4.6%); median peak RSS did not materially
change [M]. Sources and raw samples are in `bench/results/session-lifecycle.txt`.
Full `cargo xtask check` passed (`/tmp/mm-json-decode-check.log`), including
unchanged conformance, both WASM round trips and 997 durability runs / 66,778
fresh-process restores [M]. JSON/snapshot fuzz smoke completed 61,079 / 24,976
runs, each in 121 seconds, without findings [M]
(`/tmp/mm-json-decode-{json,snapshot}-fuzz.log`). Native hello is 2,214,760 bytes;
WASM hello is 1,814,390 (wasip1) / 1,925,075 (unknown-unknown) bytes [M].
Decoder traversal tables, key memoization and escaped-string buffers still
need admission. This is not complete native-workspace accounting or a total
memory/latency bound. The full GC-stress suite above predates this decoder phase.

JSON key memoization now keeps arena references in an admitted, half-full hash
table, instead of duplicating strings in an uncharged native tree. It preserves
per-document identity and the existing 4096-key ceiling. Both old and new tables
remain charged during rehashing. The sorted-vector experiment was rejected:
although it saved similar RSS, it slowed rows 9.1% and wide keys 67.3% [M].
The final version caches full hashes to reject bucket collisions before
comparing key text, avoiding repeated scans of long common prefixes.

The 370-line decoder probe and 40-line key-identity probe match the preceding
build normally and under GC stress [M]; the latter covers growth, reverse
insertion, Unicode escapes and both sides of the memo ceiling. A separate
chosen-collision probe passes table
wraparound, all growth stages, saturation and repeated-key identity. The own
JSON case retains a small collision/identity fixture and matches CPython under
explicit GC stress. Full GC stress on the intermediate memo version passed in
952 seconds, including unchanged own/corpus outcomes and 270 durability runs /
39,384 fresh-process restores [M] (`/tmp/mm-json-memo-gc-stress.log`). It used
frozen executables and predates the cached-hash filter and escaped-string change;
the final probes above cover those edits, not a full final-tree stress suite.

Escaped JSON strings now share the existing admitted output builder, retaining
its charge through arena string construction for values and keys. Unescaped
strings still borrow input ranges. The extended public budget regression failed
before and passes with 8/16 MiB refusal/success controls [M]
(`/tmp/mm-json-escape-test-{before,after}.log`). All admitted inputs in the
5–20 MiB budget sweep pass same-budget restore and recovery normally and under
explicit GC stress [M] (`/tmp/mm-json-escape-recovery{,-stress}.log`).

The final combined tree passed `cargo xtask check`
(`/tmp/mm-json-text-final-check.log`), with unchanged conformance, both WASM
round trips and 997 durability runs / 66,781 fresh-process restores [M]. Final
JSON/snapshot fuzz smoke completed 51,351 / 15,787 runs, each in 121 seconds,
without findings [M] (`/tmp/mm-json-text-final-{json,snapshot}-fuzz.log`).
Native hello is 2,213,088 bytes; WASM hello is 1,812,672 (wasip1) / 1,922,968
(unknown-unknown) bytes [M]. Final pinned alternating medians were rows
109.999 → 115.332 ms (+4.8%), wide keys 27.662 → 23.997 ms (-13.3%), repeated
short escapes 22.073 → 24.521 ms (+11.1%), and Unicode escapes
83.409 → 80.319 ms (-3.7%). Large-key median peak RSS fell from 137,368 to
104,356 KiB [M]. These are whole-process measurements, not isolated lookup
costs; budget enforcement costs throughput on some inputs. Source descriptions
and raw samples are in `bench/results/session-lifecycle.txt`. Decoder traversal
tables remain unaccounted; this is not a total memory or latency bound.

The admitted native-value buffer now shares its storage/reservation implementation
with a private `NativeVec<T>` for the remaining traversal frames. Existing callers
still use `NativeValues`; this refactor adds no admission behavior. Full
`cargo xtask check` passed (`/tmp/mm-native-vec-refactor-check.log`), with unchanged
conformance, 997 durability runs / 66,781 fresh-process restores and both WASM
round trips [M]. Native hello remains 2,213,088 bytes [M]. JSON traversal stacks
and cycle tracking are the next admission work.

JSON encoder and decoder traversal stacks now admit capacity before growth and
retain the charge while popping frames and constructing arena results. The
public regression covers deep lists and objects at 6/16 MiB refusal/success,
exact successful results, recovery and same-budget restore. Independently
disabling either stack's admission makes it fail [M]
(`/tmp/mm-json-stack-{decoder,encoder}-negative.log`). The 3–16 MiB budget sweep
also restores and recovers every completed operation under both GC modes.
The 370 decoder and 43,941 container output lines match the preceding build
normally and with GC stress; checks immediately below, at and above the
100,000-depth ceiling also match in both directions and both GC modes [M].

Full `cargo xtask check` passed (`/tmp/mm-json-stack-check.log`), including
unchanged conformance, both WASM round trips and 997 durability runs / 66,781
fresh-process restores [M]. JSON/snapshot fuzz smoke completed 55,189 / 24,248
runs, each in 121 seconds without findings [M]
(`/tmp/mm-json-stack-{json,snapshot}-fuzz.log`). Native hello is 2,214,720 bytes;
WASM hello is 1,814,422 (wasip1) / 1,924,456 (unknown-unknown) bytes [M].
Pinned alternating medians were rows 117.341 → 114.135 ms (-2.7%) and four
50,000-depth round trips 36.675 → 38.297 ms (+4.4%) [M, 21 samples each].
Sources and raw samples are in `bench/results/session-lifecycle.txt`. Cycle-table
storage and other native helpers remain unaccounted; full GC stress above
predates these changes. This is not a total-memory or latency bound.

JSON cycle tracking now admits a flat identity table for the active ancestor
path, including both tables during growth. Backward-shift deletion reuses slots
without allocating; capacity follows peak depth, not completed siblings.
Existing ancestors are checked before growth, preserving cycle errors at a full
table. Disabling circular checks allocates no table. The extended public budget
test fails with the former uncharged tree and passes at 9/16 MiB refusal/success,
including a same-input no-cycle-check success control, recovery and restore [M]
(`/tmp/mm-json-cycle-budget-negative.log`, `/tmp/mm-json-cycle-test-after.log`).

A temporary independent-set comparison passed 10,000 insert/delete operations
over 96 deliberately colliding identities, including wraparound and growth;
reversing the deletion condition made it fail. That implementation-level probe
was removed; public tests retain ancestor cycles and legal shared children.
The 370 decoder, 43,941 container and 18 depth-boundary output lines still match
the preceding build normally and with GC stress. The own JSON case matches
CPython under stress, and the 8–16 MiB sweep restores and recovers every
completed operation in both GC modes [M].

Full `cargo xtask check` passed (`/tmp/mm-json-cycle-check.log`), including
unchanged conformance, both WASM round trips and 997 durability runs / 66,784
fresh-process restores [M]. JSON/snapshot fuzz smoke completed 56,612 / 23,448
runs, each in 121 seconds without findings [M]
(`/tmp/mm-json-cycle-{json,snapshot}-fuzz.log`). Native hello is 2,216,864 bytes;
WASM hello is 1,816,713 (wasip1) / 1,926,975 (unknown-unknown) bytes [M].
Pinned alternating medians were rows 112.572 → 112.464 ms (-0.1%) and deep
round trips 37.239 → 38.786 ms (+4.2%); deep median peak RSS fell from 17,480
to 15,940 KiB [M]. Sources and raw samples are in
`bench/results/session-lifecycle.txt`. Full GC stress ran against frozen
executables `/tmp/mm-json-cycle-gc-{mm,xtask}` with log
`/tmp/mm-json-cycle-gc-stress.log`; it passed in 894 seconds,
including unchanged own/corpus outcomes and 270 durability runs / 39,388
fresh-process restores [M]. This covers the JSON cycle phase, not later changes.
Other native helpers, rendering and reclamation workspace remain unaccounted.

The admitted identity table is now private shared `NativeIds` storage in
`scratch.rs`, with heap errors translated by callers. This behavior-preserving
refactor prepares reuse by rendering without coupling text to the JSON module.
Full `cargo xtask check` passed (`/tmp/mm-native-ids-refactor-check.log`), with
unchanged conformance, both WASM round trips and 997 durability runs / 66,784
fresh-process restores [M]. Native hello is 2,217,248 bytes; WASM hello is
1,816,674 (wasip1) / 1,926,936 (unknown-unknown) bytes [M]. Rendering traversal
storage, preflight and output helpers still need admission.

Native `repr` now admits its traversal stack and active-identity table. Scalar
rendering keeps its first work item on the Rust stack and allocates no traversal
buffer. Cycle detection uses insertion's existing-identity result without a
second lookup. The public regression independently fails when either stack or
identity admission is disabled; it covers refusal, exact successful output,
same-budget restore and recovery for deep lists, tuples and dictionaries.
The 124-line rendering probe matched the baseline normally and under GC stress,
including aliases, cycles, sparse containers, views and user rendering. The
18-case budget/restore probe also passed both ways [M]. Full `cargo xtask check`
passed (`/tmp/mm-repr-traversal-check.log`), including unchanged conformance,
both WASM round trips and 997 durability runs / 66,784 fresh-process restores.
Nested-data and snapshot fuzz smoke completed 1,257 / 23,080 runs in 123 / 121
seconds without findings [M] (`/tmp/mm-repr-{nested,snapshot}-fuzz.log`). Native
hello is 2,211,008 bytes; WASM hello is 1,812,656 / 1,922,987 bytes [M]. Pinned
alternating medians: scalar 18.905 → 18.840 ms, container-heavy 19.864 → 22.443 ms
(+13.0%), deep rendering 21.888 → 24.150 ms (+10.3%); deep median peak RSS
9,192 → 11,192 KiB [M]. This buys admission, not speed or RSS reduction.
Recipes and raw samples are in `bench/results/session-lifecycle.txt`.
Completion preflight, output/formatting helpers and reclamation workspace still
need accounting; no total-memory or park-latency bound is established.

Completion preflight now admits its visited-identity table and uses a fixed
65-entry cursor stack. Admission refusal releases native workspace and chooses
the existing Python rendering fallback, rather than treating an incomplete scan
as proof that user code is absent. The fallback can park or raise `MemoryError`;
the completion text cap is not a bound on inspecting its input. The public
regression covers successful fallback at 3 MiB, refusal at 5 MiB, successful
larger input at 8 MiB, parked and completed restores, exact capped text and
recovery [M]. Treating admission refusal as a completed scan fails it. The
previous build completes the 5 MiB probe using its uncharged table.
Output, callbacks and fuel match the baseline in 78 normal / 75 GC-stress
completion and percent-formatting cases, including both sides of the depth
and visited-identity thresholds [M]. Full `cargo xtask check` passed
(`/tmp/mm-preflight-budget-check.log`), with unchanged conformance, both WASM
round trips and 997 durability runs / 66,784 fresh-process restores [M]. VM and
snapshot fuzz smoke completed 6,530 / 20,386 runs, each in 121 seconds without
findings [M] (`/tmp/mm-preflight-budget-{vm,snapshot}-fuzz.log`). Native hello
is 2,213,016 bytes; WASM hello is 1,813,414 / 1,923,746 bytes [M]. Pinned
alternating median execution times: million-element primitive completion
15.431 → 11.071 ms (-28.3%); 50k distinct child lists 6.564 → 7.504 ms (+14.3%);
scalar percent formatting 28.821 → 29.223 ms (+1.4%, noisy) [M]. Scanning
primitive runs under one heap borrow avoids repeated slot validation. Raw
samples and recipes are in `bench/results/session-lifecycle.txt`.
The 28-case builtin budget/recovery probe completed under explicit GC stress,
including after restoration. Its 28-case custom-rendering counterpart also
completed against frozen `/tmp/mm-preflight-budget-final`. Both logs match their
non-stress outcomes exactly, including expected budget refusals and callback
counts (`/tmp/mm-preflight-budget-{builtin,custom}-stress-final.log`).
Output/formatting helpers, explicit exception-method argument copies and
reclamation workspace remain unaccounted.

Explicit exception `__repr__` and `__str__` methods now read argument slots
without copying the whole tuple. Native preflight cannot run user code or collect
the argument storage; OSError reads only its individual fields. This preserves
existing behavior: 962 output lines covering argument-count boundaries, filenames,
KeyError, subclasses, user rendering, failures and cycles match the baseline
normally and under GC stress [M] (`/tmp/mm-exc-method-{before,after}{,-stress}.log`).
Full `cargo xtask check` passed (`/tmp/mm-exc-method-check.log`), with unchanged
conformance, both WASM round trips and 997 durability runs / 66,784 fresh-process
restores [M]. Native hello is 2,214,296 bytes; WASM hello is 1,814,928 / 1,925,149
bytes [M]. For one million scalar arguments, pinned alternating whole-process
medians were `__str__` 72.876 → 70.196 ms (-3.7%) and `__repr__` 69.273 → 67.379 ms
(-2.7%); median peak RSS fell from 35,440 to 31,368 / 31,384 KiB respectively [M].
Recipes and raw samples are in `bench/results/session-lifecycle.txt`.
Rendering output, other native helpers and reclamation workspace still need
accounting. This refactor does not establish a total-memory bound.

The follow-up GC-stress campaign on frozen `21368ed` executables passed own
cases, unchanged corpus outcomes, and 270 durability runs / 39,388 fresh-process
restores in 818 seconds [M] (`/tmp/mm-render-gc-stress.log`). It covers the renderer,
completion-preflight and explicit exception-method phases above, not subsequent
percent-format changes. The earlier preflight budget probes also completed:
28 builtin and 28 custom-rendering cases matched non-stress outcomes after
explicitly re-enabling GC stress on restore, including expected budget refusals
and callback counts [M] (`/tmp/mm-preflight-budget-{builtin,custom}-stress-final.log`).

Native percent formatting now admits its input copy and accumulated output,
retaining output capacity through arena allocation. Literal spans and consecutive
escaped percent signs append in batches; padded pieces reserve their complete
size together. Errors and Python handoff drop the independent reservations.
The public-session regression fails independently when input or output admission
is disabled and passes with both enabled, including both string and byte formats,
tight/roomy budgets and snapshot recovery [M]. Differential probes preserve
5,192 output lines, callback traces and fuel, normally and under GC stress [M].
Twenty budget/callback cases match under stress after parking and restoring,
including refusal before a callback, refusal after it and malformed-format errors
(`/tmp/mm-percent-budget{,-stress}-final.log`). Full `cargo xtask check` passed
(`/tmp/mm-percent-check.log`): unchanged conformance, 997 durability runs / 66,784
fresh-process restores, both WASM round trips. Native hello is 2,216,376 bytes;
WASM is 1,816,576 / 1,926,665 bytes [M]. Pinned alternating whole-process medians:
20k short formats 19.433 → 21.183 ms (+9.0%); 4 MiB literal formatting
18.301 → 17.687 ms (-3.3%); padding, escaped-percent and many-piece workloads
were approximately unchanged [M]. Raw samples and recipes are in
`bench/results/session-lifecycle.txt`. Individual conversion buffers and general
rendering output remain unaccounted. The benchmark exposed another concrete gap:
`pct_next` copied the entire tuple for every argument, making many-field formats
quadratic; the following refactor removes it.

Native percent argument lookup now reads one tuple slot, including `*` width and
precision arguments, without a whole-tuple copy. This preserves semantics and
adds no permanent tests: existing formatting/durability cases pass unchanged.
The expanded differential probe checks asymmetric tuples, missing/extra arguments,
conversions and mapping fallback: 5,256 output lines, callbacks and fuel match
the baseline normally and under GC stress [M]. The 20 budget/restore cases also
match both the previous phase and stressed execution. Full `cargo xtask check`
passed (`/tmp/mm-percent-args-check.log`): unchanged conformance, 997 durability
runs / 66,784 fresh-process restores and both WASM round trips. Native hello is
2,215,848 bytes; WASM is 1,816,674 / 1,926,637 bytes [M]. Pinned whole-process
medians for builtin `%s` fields: 10k 83.407 → 3.569 ms (23.4×), 20k 325.229 →
4.724 ms (68.8×), 100k 8,338.194 → 11.695 ms (713.0×); 20k short mixed formats
21.039 → 20.190 ms (-4.0%) [M]. All use alternating samples; 100k uses five
samples after one warmup, the others 21 after three. Raw samples and recipes are
in `bench/results/session-lifecycle.txt`. This is not a general VM speedup or a
linear-time guarantee for user conversions, whose prelude handoffs still restart
formatting. Remaining percent input copies include the preconverted-value list,
eager type-name text, and rejected `%c` inputs; output conversion pieces and
general rendering output still need admission.

Percent formatting now borrows the prelude conversion list's backing storage
for each native attempt, borrows original type names only on errors, and checks
`%c` length before copying accepted data. Native formatting neither mutates that
list nor collects; Python handoffs reacquire it on the next attempt. The expanded
differential probe matched 5,365 output lines, callbacks and fuel normally and
under GC stress. Twenty budget/host-call/park/restore cases matched the preceding
phase, including stressed restores. Pinned alternating whole-process medians:
short formats 20.118 → 19.652 ms (-2.3%), 100k builtin fields 11.053 → 10.278 ms
(-7.0%), 1,000 user conversions 38.645 → 26.285 ms (-32.0%), rejection of an
8 MiB byte `%c` input 11.473 → 8.711 ms (-24.1%) [M]. Recipes and raw samples
are in `bench/results/session-lifecycle.txt`. Full `cargo xtask check` passed
(`/tmp/mm-percent-inputs-check.log`): unchanged conformance, 997 durability runs /
66,784 fresh-process restores, both WASM round trips and every checksum. Native
hello is 2,216,168 bytes; WASM is 1,818,331 / 1,928,156 bytes [M]. Conversion
pieces and general rendering/printing output still need admission; borrowing
the conversion list does not remove repeated prelude restart work.

Native printing borrows separators, endings, builtin string arguments and plain
string-subclass backing instead of copying them before appending. Stream writes
pass the arena text directly to the host callback, including empty strings;
return values still count Unicode characters. Other conversions retain their
existing native/prelude behavior. Sixty-four differential scenarios matched
callback chunks, streams, fuel, host calls and outcomes before/after: 6/12 MiB
budgets, ordinary/GC-stressed execution, and unparked/snapshot-restored execution.
Cases include Unicode/NULs, subclasses, failed conversions, custom files/flush,
invalid arguments, and large input/separator/ending strings. Pinned alternating
whole-process medians with stdout discarded: short prints 7.801 → 6.880 ms
(-11.8%), 16 × 2 MiB prints 25.694 → 9.063 ms (-64.7%), equivalent stream
writes 9.039 → 6.071 ms (-32.8%), unused 2 MiB separator 17.185 → 3.595 ms
(-79.1%) [M]. Captured outputs were checked separately; recipes/raw samples are
in `bench/results/session-lifecycle.txt`. Full `cargo xtask check` passed
(`/tmp/mm-print-inputs-check.log`), including unchanged conformance, 997 durability
runs / 66,784 fresh-process restores and both WASM round trips. Native hello is
2,216,184 bytes; WASM is 1,818,231 / 1,927,585 bytes [M]. Printing's accumulated
output buffer and temporary non-string conversion buffers still need admission.

JSON's admitted text buffer now lives in `scratch::NativeText`; JSON keeps its
quoting/escaping methods. Growth, exact-capacity fallback and reservation lifetime
are unchanged. This prepares sharing the builder with printing without duplicating
admission logic; printing does not use it yet. Full `cargo xtask check` passed
(`/tmp/mm-native-text-refactor-check.log`): unchanged conformance, 997 durability
runs / 66,784 fresh-process restores, both WASM round trips and all checksums.
Native hello remains 2,216,184 bytes; WASM is 1,818,232 / 1,927,586 bytes [M].

Native `print` now admits accumulated output before growth and holds the charge
through its host callback, releasing it on error or prelude handoff. Failed
native attempts emit no partial output; custom-file writes retain their prelude
behavior. The public-session regression failed before admission and passes now:
an 8 MiB session refuses an 8 MiB-plus-ending buffer, while a 16 MiB control
succeeds using exact-capacity fallback. Repeated attempts test release on failure
and on user-conversion handoff. Eight ordinary/GC-stressed budget and restore
scenarios passed; 64 earlier output/fuel/callback/restore controls stayed identical.
The shared builder now sizes borrowed text from raw bytes, avoiding a duplicate
UTF-8 validation pass on growth. Pinned alternating whole-process medians:
20k short prints 6.849 → 8.367 ms (+22.2%, roughly 76 ns per call), 16 × 2 MiB
prints 9.839 → 9.881 ms (+0.4%), unchanged stream-write control 6.542 → 6.553 ms
(+0.2%), unused large separator 3.884 → 3.866 ms (-0.5%) [M]. Small-print
admission has a real cost; the large-output cases are effectively unchanged.
Raw samples/recipes are in `bench/results/session-lifecycle.txt`.
Full `cargo xtask check` passed (`/tmp/mm-print-budget-check.log`): unchanged
conformance, 997 durability runs / 66,784 fresh-process restores, both WASM round
trips and every checksum. Native hello is 2,215,640 bytes; WASM is 1,817,853 /
1,927,321 bytes [M]. Temporary non-string conversions, general renderer output
and other native/reclamation workspace still need admission.

Nonempty native text buffers now start at eight bytes when the budget permits,
avoiding repeated tiny allocations; empty buffers still allocate nothing, and
exact-capacity fallback stays intact. This recovers some, not all, of print
admission's small-call cost: pinned alternating whole-process medians for 20k
short prints were 7.970 → 7.633 ms (-4.2%); small JSON encoding 40.244 → 39.224 ms
(-2.5%) and escaped-string decoding 36.313 → 35.089 ms (-3.4%) [M]. Large prints,
stream writes and unused large separators stayed within 0.4%. All 72 budget,
output, fuel, callback, GC-stress and restore controls stayed identical. Raw
samples/recipes are in `bench/results/session-lifecycle.txt`. Full
`cargo xtask check` passed (`/tmp/mm-native-text-growth-check.log`): unchanged
conformance, 997 durability runs / 66,784 fresh-process restores and both WASM
round trips. Native hello is 2,215,656 bytes; WASM is 1,817,862 / 1,927,330 [M].

Percent-format text conversions now retain arena values and a precision cutoff
instead of copying builtin string/byte fields or prelude-converted text. Padding
admits its output before reacquiring the borrowed slice. Native formatting does
not collect or run user code; numeric/repr conversion buffers remain unchanged.
All 5,365 differential output lines and fuel match the preceding build separately
in normal and GC-stress modes. Twenty budget/host-call/restore cases match the
previous phase and stressed restores [M]. An additional precision-boundary probe
matches CPython and passes 15 normal / 16 stressed fresh-process restores [M],
covering Unicode versus byte cutoffs, both padding directions, empty/full/long
precision, custom str/repr/bytes/ascii results and repeated fields.

Pinned alternating whole-process medians: short formats 19.472 → 19.499 ms
(+0.1%), 100k builtin fields 10.116 → 9.230 ms (-8.8%), 1,000 custom fields
26.216 → 23.452 ms (-10.5%), 32 precision-truncated large Unicode fields
164.535 → 9.265 ms (-94.4%), equivalent bytes 31.392 → 9.388 ms (-70.1%),
custom Unicode 159.705 → 8.213 ms (-94.9%), and 16 whole Unicode fields
126.128 → 103.320 ms (-18.1%) [M]. Peak RSS did not materially improve;
input construction dominates these fixtures. Recipes/raw samples are in
`bench/results/session-lifecycle.txt`. The large-case assertions also pass on
CPython; their normal and stressed reports match the baseline. General rendering
output, native numeric/repr pieces and reclamation workspace still need admission.
Full `cargo xtask check` passed (`/tmp/mm-percent-pieces-check.log`): unchanged
conformance, 997 durability runs / 66,784 fresh-process restores, both WASM
round trips and every checksum. Native hello is 2,216,376 bytes; WASM is
1,818,058 / 1,927,437 bytes [M]. The first gate attempt caught an unnecessary
semicolon; Clippy's fix was applied before the successful full rerun. This phase
ran focused GC-stress probes, not the full GC-stress corpus or extended fuzzing.

## Reclamation workspace (verified)

Between-cell code/source/shape compaction now keeps live marks and remapping
indices in the admitted records themselves. Their existing `size_of` admission
and restore checks include this bookkeeping; it is not serialized. An owned,
fixed-size heap cursor allows slot writes between visits without copying object
addresses. Code and shape liveness share a scan. Rewriting is skipped when only
trailing records disappear; moved indices are rewritten before the tables shrink.
Shared traceback entries use temporary negative tags to prevent double remapping,
cleared before returning without callbacks, allocation or GC between the passes.
This closes the input-sized reclamation-buffer gap, not native rendering or
allocator-internal transient realloc/RSS accounting.

The first prototype passed session tests but introduced substantial scan overhead;
it was not accepted. Combining scans, retaining bitmap position in the cursor and
skipping identity remaps removed that regression on the large controls. Final
pinned public-facade cell-start medians for 100,000 retained objects [M]: tuples
1.212 → 1.088 ms, instances 2.695 → 1.862 ms, closures 5.455 → 3.970 ms,
tracebacks 14.169 → 7.756 ms. These include GC and tiny-cell execution, not parsing.
Two small controls regressed: empty closure fixture 35.040 → 37.383 µs (+6.7%)
and 1,000 instances 62.269 → 66.092 µs (+6.1%); no universal speedup is claimed.
Three alternating-order process pairs, three warmup cells and 21 timed cells
per process; source and raw samples are in `bench/results/session-lifecycle.txt`.
All fixtures check retained values, same-build restore, reset and saved traceback
source lines. An additional 24 controls pass at 3/4/8 MiB with normal/stressed GC.

`cargo xtask check` passed (`/tmp/mm-reclaim-check.log`): unchanged conformance,
997 durability runs / 66,784 fresh-process restores, both WASM round trips and
all checksums. Native hello is 2,213,952 bytes; WASM is 1,815,477 / 1,923,924
bytes [M]. The new public heap-cursor test covers sparse segments and mutation
during traversal; existing session, exhaustion and restoration tests pass.
Full GC-stress corpus and extended fuzzing remain separate outstanding checks.

## Recovered conformance integration (pushed)

The five recovered commits were cherry-picked onto `main` (conflicts resolved
against current admission code: codec normalization uses a fixed buffer and
wide encodes go through `build_bytes`; `parse_spec` keeps borrowed input).
They take CPython-tagged results from 483 to 531/568. The worktree's
uncommitted work (decimal, numbers, contextvars, unicodedata with regenerated
and byte-identical UCD tables, host TZif time zones) was validated and
committed after fixes, reaching 540/568, host 61/81, no existing-pass
regressions [M]. `decimal`/`unicodedata` are optional: the default facade
raises `ModuleNotFoundError`, `mm-cli` enables both; own case
`136_decimal_unicodedata.py` matches CPython. The host zone is saved in
snapshots (`the_host_time_zone_and_its_dst_rules_survive_restore`); the five
zone corpus cases pass durability over 2,232 fresh-process restores with
`mm resume` no longer re-supplying `TZ` [M].

Size [M]: main 2,221,368; recovered commits +9,672 / +1,216 / +3,888 /
+5,664 / +17,720; uncommitted work +5,712; codec table -1,392: 2,263,848.
Not reduced further by micro-shrinking; the owner raised the native ceiling
to 2,275,000 bytes (2026-11-03).

Session memory [M] (smallest arena running a trivial cell / fresh snapshot):
main 2,384,677 / 103,902 bytes; now 2,448,103 / 122,425 (+63.4 KB, +2.7% /
+17.8%), almost all prelude growth (`types.GenericAlias`/`Union` +40 KB,
MarkupIterator `str.format` +21.5 KB). The owner accepted this baseline:
`completion_preflight_releases_its_budget_before_rendering` now expects
completion at 3 MiB + 320 KiB, not + 256 KiB. For the same reason
`sorted_json_entries_share_the_memory_budget` moves from 6,400 KiB to
6,448 KiB: on 91f6a31 its refuse-sorted/recover-unsorted window is
6,368-6,400 KiB, with this work 6,432-6,464 KiB, a 64 KiB shift [M]. Trimming the prelude belongs
to the memory-accounting phase (continuation step 1), not to conformance.

Left out: the remaining 28 CPython failures (eval/exec, get_type_hints,
`_decimal`-exact messages, NaN ordering in deque/namedtuple, others), and
the stale `hardening` worktree (main supersedes it).

## Small first segments (mm-core)

Each space's first normal segment now starts at 32 KiB (4,096 units) and,
while it is the bump segment, grows by reallocation, doubling up to 1 MiB;
later segments are still committed whole. Addresses do not change, bitmaps
grow with the data, `committed_bytes` and `would_fit` charge what is
committed, and a FULL snapshot records each normal segment's capacity
(4 bytes per segment), so a restore commits the same.

Measured [M] (floor = binary search on `Limits::memory`, 1 KiB resolution,
`Session::new` plus `x = 1`):

| | Before (37163fc) | After |
|---|---|---|
| Memory floor | 2,387,968 | 394,240 (-83%, 6.1x smaller) |
| Fresh snapshot bytes | 122,403 | 122,411 (+8: two capacities) |
| Startup (`mm run bench/startup.py`, callgrind Ir) | 7,094,955 | 7,155,486 (+0.85%: growth copies) |
| fib / dict / str | 332.94M / 1171.57M / 495.93M | 333.00M / 1174.77M / 498.91M (+0.02 / +0.27 / +0.60%) |
| json_rows / sort | 1163.25M / 7269.28M | 1165.88M / 7272.02M (+0.23 / +0.04%) |

A first version kept the capacity as a `Seg` field; that made `Seg` 72
bytes and every segment lookup a multiply, costing 0.7-2.1% on the
benchmarks. The capacity is now the backing store's length. A zeroed
allocation plus copy for growth was tried and measured slower (startup
7,270,155) than `Vec::resize`.

Memory-window tests moved down by about the old fixed overhead (two whole
segments less ~1.9 MiB, or ~1 MiB where only one space was small). Each
tight value was re-found by binary search on the old and new trees so the
margin from the flip point stays the same or larger: native sort,
byte case, byte parts 8 MiB -> 6,208 KiB; JSON output 12 MiB -> 10,304 KiB
and 9 MiB -> 7,232 KiB; sorted JSON 6,448 -> 4,592 KiB (window 4,552-4,616
KiB passes); JSON decoder 8 MiB -> 6,208 KiB, 10 MiB -> 8,256 KiB; JSON
traversal 4 MiB -> 3,392 KiB, 9 MiB -> 8,640 KiB; rendered text 4 MiB ->
2,112 KiB; repr 7 MiB -> 6,080 KiB; byte conversions 8 -> 7 MiB, 6 -> 4
MiB, 20 -> 18 MiB; codec names 10 MiB -> 8,256 KiB; execution stacks 3 MiB
-> 1,088 KiB; instance shapes 4 MiB -> 3,392 KiB; completion preflight
3 MiB / 3 MiB + 320 KiB / 5 MiB -> 1,984 / 2,368 / 4,032 KiB;
`code_at_the_memory_limit_can_restore_in_the_same_budget` 3 MiB -> 1,088
KiB; `malformed_snapshots_are_typed_errors`' too-small engine 1 MiB -> 256
KiB. mm-core: the op interpreter's smallest limit 3 -> 1 MiB (OOM paths must
still run), and tests that assumed a full first segment now expect 32 KiB +
bitmaps. New test `first_segments_grow_without_moving_refs`. `arena_ops`
and `snapshot_restore` fuzzed 5 minutes each without findings.

## Recovered conformance branch (historical, 2026-11-04; worktrees may be gone)

| Branch | Worktree | Done (committed) | In progress |
|---|---|---|---|
| `recovered-conformance` (`06fb6ee`) | `../mm-recovery/conformance` | NaN identity, hashes, float()/int() grammar, slices, module dunders (`Session::set_module_dunders`), gc, GenericAlias / `int \| None`, tomllib, types, str.format via MarkupIterator, utf-16/32 codecs, many error messages | decimal (`_pydecimal`), numbers, contextvars; remaining AssertionError cases; get_type_hints; timezones; unicodedata |

The recovered conformance worktree retains 17 modified tracked files and
17 untracked files [M], not yet validated or integrated. Its Git directory
belongs to `../mm-recovery/repository`; the same committed branch tip is
also available in the main checkout. Do not lose its dirty files when
merging the branch.

Recovery history/manifests and the original reconstructed worktrees remain
under `../mm-recovery`. Do **not** rerun `reconstruct.py`: it would overwrite
later fixes with older raw copies. The exact continuation archive is in
[the recovery thread](https://ampcode.com/threads/T-01a1995b-33dd-72c9-aeec-f6123ca4ad09)
at `.amp/in/artifacts/micro-monty-continuation-final.tar.gz`, SHA-256
`a34dc922cf5ae702dd32fc478df36d342695bfe9fde94269b1cba43ddfaaffe8`.
It preserves the reconstructed state completely, not the accepted gaps in
the original old-orb recovery. The preserved hardening worktree predates
this phase's repr fuel/safepoint fixes; use `main` for current hardening.

## Snapshot persistence decision (2026-11-04, owner-approved)

FULL images plus host compression; no delta snapshots in the engine for now.
Evidence [M], all under `bench/evidence/results/`:

- The delta candidate (dirty-line tracking, `snapshot_delta`, `restore_chain`)
  was 4–19% slower than main on 11 of 13 bench programs (dict 1.19×, loop
  1.15×). Compiling line marking out restored main's instruction count within
  ±0.6% on 11 programs, so marking is the cost. A per-session switch (a flag
  read at each write) kept 20–73% of it when off and was slower than always-on
  when on.
- zstd-1 on FULL images gives 3.6× (small parked sessions) to 19× (100k rows),
  the same order as deltas + zlib on the one comparable workload.
- The candidate also panics on a corrupt snapshot header
  (`h.len() - h.nvals()` before the bounds check in `raw_words_mut`); main
  returns `BadSlot`.

The local branch `evidence-campaign` that held the candidate has been
deleted. Every measurement file, including the ablation and switch builds'
patches in `bench/evidence/ablation/`, is on main; only the rejected
candidate code and its `persistence.rs` probe were dropped. The one design
left open then, snapshot-time comparison against the fresh-session image,
was prototyped and measured on 2026-11-05 and not adopted either; engine-side
deltas are closed (scope decision 33).

The evidence harnesses (`bench/evidence/`, the repaired `cargo xtask bench`
runner, the snapshot fuzz target's API fix, async cleanup in difffuzz) are on
main. Correctness-breadth results go to `conformance/breadth/`.

## Scope decisions (2026-11-05, owner-approved)

1. **Stdlib tiers.** One `mod-*` feature per module (already the build).
   Language-support pieces ship always; tool/data modules are default-on as
   chosen by an import census of real tool code (run 2026-11-05: decisions
   7, 11 and 12, `conformance/codemode`); the long tail
   is opt-in.
2. **Gap-closing rule.** A change that closes a non-sandbox gap keeps bench
   instruction counts within ±0.5% of the previous build, records its size
   delta in the budget comments, and leaves the sandbox exclusions unchanged.
3. **Object-model features** (`__del__`/`weakref`, writable
   `locals()`/`globals()` and `exec` into them, frame objects/`settrace`)
   each need their own measured decision; they are not routine gaps.
4. **No CPython C-API.** It would give up pause-anywhere snapshots, metering,
   `unsafe`-freedom and Wasm. Use Rust-native data types and host modules;
   hosts route package-heavy cells to another engine.
5. **A2: one dict as the keyword arguments, per function.**
   `HostSignature::dict_as_kwargs` (default off) lets a typed host function
   take one positional `dict` as its keyword arguments, bound and then
   type-checked as usual. Off by default because it is ambiguous for a tool
   whose only parameter is itself a dict. Docs recommend structured
   `HostValue` results over JSON strings (311 of 396 string-result census
   solutions call `json.loads` [M]).
6. **B2: structured host errors.** `HostError` gains `data:
   Option<HostValue>` and `class: Option<String>` naming a
   `HostModule::exceptions` class (`ToolError(RuntimeError)`); such errors
   carry `.tool` (call name) and `.data`. Raising stays the default;
   returning errors as values is documented as an option. Bridge failures
   (unknown name, refused argument, missing OS hook) stay plain built-in
   exceptions. The exception is a heap object, so it survives snapshots.

From the codemode checkpoint (census, survey and isolation study; the
[checkpoint artifact](https://ampcode.com/artifacts/micro-monty-codemode-checkpoint-what-to-build-next-census-su-a149b115-a16c-4557-a0ea-a6db91453461)),
approved 2026-11-05:

7. **D2.** `decimal` (with `numbers` and `contextvars`) is in the default
   build, as are `statistics` (with `fractions`) and `tomllib`.
8. **J2.** `json.loads` takes all five hooks; with a hook the native scanner
   keeps literal text and a prelude pass calls the hooks in CPython's order.
   Without hooks the native path is unchanged.
9. **Z1.** `zoneinfo` is CPython's pure-Python reader over TZif bytes the
   host serves (`Session::set_zone_source`); no bundled tzdata. Without a
   source the error names the missing data; loaded zones survive snapshots.
10. **G2.** `globals()` is a read-only view: reads as CPython, every write
   raises `TypeError` (spec §4). Writable `globals()`/`locals()` remain a
   separate decision under 3.
11. **T1.** The census-gated modules (`struct`, `platform`, `difflib`,
   `pprint`, `logging`, `codecs`, `traceback`, `warnings`, `gc`) stay in the
   default build.
12. **S1.** One default build, no profiles; the `hello.wasm` budget goes to
    about 2 MB (2,169,800 now, after the asyncio primitives). Size
    optimization and build profiles are
    revisited later.

Robustness decisions from the embedding hardening, 2026-11-05:

13. Closure Ir +0.82% and startup −0.96% (outside ±0.5%) are accepted.
14. The ac10b28 budget raise (1,924,500 / 2,014,500) is accepted.
15. Parser frames are not shrunk for now: compiling at the nesting limit
    needs about 1 MiB of host stack (docs/sdk.md "Embedding guarantees").
16. A deadline alone does not stop a big int operation once it has started
    (with unlimited fuel it waits for the operation); this is documented,
    not changed.

The wasm embedding (`mm-wasm` v1, docs/research/wasm-embedding.md),
2026-11-05:

17. **Placement.** `crates/mm-wasm-abi` (codec, publishable) and
    `crates/mm-wasm-guest` (cdylib, `publish = false`) are workspace
    crates; `crates/mm-wasm-host` (wasmtime) is its own workspace. A spec §5
    exception covers only the guest's import block and
    `#[unsafe(no_mangle)]`. The shipped `wasm32-unknown-unknown` guest has a
    size budget beside `hello.wasm`, which stays the primary budget
    (2,223,853 B [M] at first, 2,318,178 after the asyncio primitives;
    ceiling 2,320,100).
18. **v1 is not frozen.** It carries the host-API and stdlib round's facade
    features (`dict_as_kwargs`, `HostError { data, class }`, host exception
    classes, hints, the environment summary, the call log, the text cut,
    `fuel_wanted`, stubs, the local time zone and the zone source) and grows
    with the facade until the owner freezes it.
19. **Park channel.** The guest polls the host's park flag through the
    deadline clock; no facade hook.
20. **Defaults.** Park grace period 100 ms, configurable per host; the call
    budget is off by default, an opt-in backstop. Hosts run cells in fuel
    slices and check `fuel_wanted()` before granting more fuel; natively that
    is the only defence against one long native operation (docs/sdk.md).
21. **One session per instance.**
22. **Async wasmtime** is plak's work.
23. **dlmalloc `validate_size`** (superseded by 41) on `wasm32-unknown-unknown` (about 90 KB of
    the guest [M], ~75 KB of `hello.wasm` [E]; why unknown-unknown is about
    88 KB larger than wasip1): accepted as is, option (a). It is in the
    docs/spec.md §6 size notes for the size pass; the `unsafe` exception is
    not widened for an allocator.
24. **v1 frozen** after four last exports (`host_modules`,
    `set_module_dunders`, `set_int_bits_text`, GC stress), each with a parity
    test against native. Within v1 only additive, optional exports; any other
    change bumps `ABI_VERSION` (docs/research/wasm-embedding.md). Publishing
    `mm-wasm-abi` is left for later.
25. **`cargo xtask wasm-host`** gates the shipped guests (both targets)
    against `crates/mm-wasm-host`'s release tests, outside `cargo xtask
    check`; required before pushes touching the wasm crates, the facade API
    or the snapshot format (AGENTS.md).

From the macOS/bare-metal study (thread T-01a1aa37-447b-71d0-bed1-b0e09ede2059),
2026-11-05:

26. micro-monty adds no OS limits for child workers on macOS; no Seatbelt
    profile.
27. The macOS child-mode stall is deferred; the owner expects at most about
    30–40 concurrent agents on the Mac.
28. No separate macOS native size budget.
29. The 6 own cases where CPython itself prints differently on macOS stay
    skipped there.

From the codemode host API checkpoint (its "Open" items), 2026-11-05:

30. **Container-aware repr truncation** (`TextCut`, `render.rs`, about
    9 KB of `hello.wasm` [M]) is accepted as built; no fallback to `Head`
    plus `head_tail` only.
31. **`TimeoutError` for deadlines stays a documented convention**
    (docs/sdk.md "Host errors"): a host call reports
    `HostErrorKind::TimeoutError`; a cell deadline parks and the host
    decides. There is no raising deadline mode.
32. **fmt +0.74% Ir** at 89e945b is accepted as allocator noise (the
    binary's own instructions +0.05%; the rest glibc free-list state).

From the base-relative snapshot study
(docs/research/base-relative-snapshots.md), 2026-11-05:

33. **Snapshots stay FULL; engine-side deltas are closed.** No delta,
    base-relative or dirty-tracking format in the engine or in mm-wasm v1.
    Hosts compress at the storage boundary with zstd level 1, the build's
    fresh-session image as a raw prefix (`--patch-from`), the frame checksum
    on, and each blob keyed by build id; a dictionary trained on the host's
    own parks is an optional extra (docs/sdk.md "Storing snapshots"). Why
    [M]: the fresh-session image is deterministic across processes, native
    and both wasm targets, but it is only 24–45% of a small park, so a
    snapshot-time block delta gains 1.2–1.4× over FULL+zstd-1, the same as
    the zstd prefix with no engine change; an engine version would add about
    22.5 KB to the guest, which has about 2.3 KB of headroom (2,240,993 B
    against 2,243,300 at 9306b80).

From plak integration, 2026-11-05:

34. **Pre-run type checking is plak's.** plak runs a host-side checker over
    `Session::stubs()` before a cell runs ("type errors; nothing ran").
    micro-monty adds no static checker: a cell runs up to a bad call to a
    typed host function and raises `TypeError` there. The two plak
    `real_python_sdk_driver_*` failures (the worker refuses `sdk_stubs`) are
    expected until plak has that checker (docs/plak-integration.md).

From the interpreter speed study (docs/research/interpreter-speed.md),
2026-11-05:

35. **Speed items 1–4 land** (HANDOFF "Speed landing"): the generator
    resume/yield fast path, `Ic::New` for `cls(...)` with a plain
    `__init__` (spec §8.1's IC table), the str and format fast paths, and
    stack temporaries instead of Rust heap ones on hot paths. The size
    budgets rise by what they measure (per item in `xtask/src/main.rs`).
    The snapshot format does not change.
36. **`datetime`'s parsers are native**, a narrow exception to spec §12's
    Rust-versus-Python rule: `fromisoformat` (a port of CPython 3.14.7's C
    parser) and a `strptime` fast path for the common directives that
    hands everything else to the Python `_strptime`. Reason: agents parse
    timestamps in loops and these were the slowest codemode cells
    (`fromisoformat` about 100× CPython per call in Ir). Size measured
    first: +4,668 B and +9,153 B of `hello.wasm` [M]. The rest of
    `datetime` stays Python.
37. **Item 5 (resident generator frames) is deferred.** The `unsafe`
    exception for unchecked UTF-8 str views (item 9) is rejected; natives
    use byte-level APIs where that helps. Items 6–10 land only when each
    stays within ±0.5% Ir on the other bench programs and the size rules;
    item 8's homogeneous sort keys did (sort −6.2%, the rest within
    0.01%), the others stay open (docs/research/interpreter-speed.md §5).

Snapshot upgrade policy, 2026-11-05:

38. **U1: keep old builds keyed by build id, drain, treat the unresumable as
    a crash.** A snapshot restores only on the build that wrote it; the
    engine adds no cross-build migration. A host that upgrades keeps the old
    build until its parked snapshots have drained; a cell no kept build can
    resume is a crash (plak's `OutcomeUnknown` and reconciliation).
    `mm-wasm-host`'s `Runtime` holds several guests keyed by build id
    (`load`, `unload`, `restore` by the image's bytes 10..42,
    `Error::UnknownBuild`); natively, keep the old binary or worker, or drain
    on the old build's wasm guest, which shares the native build id
    (docs/sdk.md "Upgrading: drain old builds", spec §9.3). The guest gains
    the opt-in stdlib features (`mod-uuid`, `mod-unicodedata`,
    `mod-html-entities`); `cargo xtask wasm-host` builds one with `mod-uuid`
    as the second build of `tests/upgrade.rs`.

Review follow-ups Ir, 2026-11-05:

39. **startup Ir +0.551%** from the review follow-ups (910be38..d6d77f6,
    measured against 1fe8e2d) is accepted as code-layout noise: with
    symbols at equal-length paths it measures −0.25% [M].

From the aware `datetime` study, 2026-11-05:

40. **Aware `datetime` paths follow CPython's C** (HANDOFF "Aware
    datetime"): comparison, hash, arithmetic, `utcoffset`, `astimezone` and
    `fromutc` compute from fields in Python, with two constant-time natives,
    `_mm.dt_ord` and `_mm.dt_ymd`, added to spec §12's `datetime` exception.
    Python `tzinfo` subclasses stay ordinary calls; no native runs user
    code. +4,080 B of `hello.wasm` [M].

From the size pass, 2026-11-05 (HANDOFF "Size pass"):

41. **`crates/mm-wasm-alloc`, a temporary §5 exception** (route (b);
    supersedes 23). std's dlmalloc called through `c_free`/`c_realloc` so
    its `validate_size` assertions are not inlined at every deallocation
    site on `wasm32-unknown-unknown`: −83,553 B `hello.wasm`, −97,112 B
    guest [M]. `Sync` only without `atomics`; registered by the guest and
    the `hello`/`roundtrip` examples, never by the facade; optional for
    embedders (docs/sdk.md). The exception is temporary: once a stable
    toolchain we pin ships dlmalloc with `validate_size` as a debug
    assertion, delete the crate and the exception. The owner may file the
    upstream issue; the draft is below.
42. **The facade builds at `opt-level = "z"`**; `mm-engine` and `mm-core`
    stay at 3, `mm-parse` `z`, `mm-compile` `s`. `cargo xtask size` builds
    `hello` with its own crate at opt 3 (fat LTO uses the root crate's
    level). Ceilings lowered to the measurements plus about 2 KB.
43. **The default text limit is 1 MiB** (2026-11-06), or the memory limit
    when that is less: a session that never calls `set_text_limit` cuts its
    completion `repr`/`str` there, under the configured `TextCut`;
    `Completion::value` stays whole and hosts can raise it. It matches plak's
    and the cell worker's 1 MiB caps, and a model cannot use more. It was the
    memory limit. Printed output is not covered: the text limit has only
    ever applied to completions, and print streams to the host's callback.
    Size on 59f0477 [M]: guest +17 to 2,174,867, `hello.wasm` and native 0;
    bench Ir +3 instructions per program (+0.000%) [M]. No own case prints
    over 101 KB (CPython [M]); `mm` reports module completions as `None`.
44. **`mm-cell-worker` is a library plus a thin binary** (owner, 2026-11-07),
    so plak/plakous depend on it for in-process workers instead of copying
    its loop (docs/research/plak-in-process.md). The library (protocol
    loop, codec, session driving; `serve` over std `Read`/`Write`, no
    tokio, no FFI, no seccomp) takes the engine as a `CellEngine`
    (`NativeEngine` for the facade; hosts wrap `mm-wasm-host` themselves),
    and its policy values as `Config` with plak's values as defaults. The
    binary keeps stdio, seccomp and the §5 FFI exception. Still no new
    policy features (no queueing). `publish = false` until a 0.1 release.
    Replaces the 2026-11-05 "test-only harness" scope (spec §10.2).
45. **The Engine owns a cache of decoded bundled modules** (2026-11-07,
    after the session-templates study and host1's in-process
    measurement).
    - **Ownership.** The facade's `Engine` owns the cache and shares it
      with its clones and every session made from them. The engine crates
      gain no global or static state: mm-engine defines a `ModuleCache`
      hook, which the facade passes to `Vm::set_module_cache` and
      `Vm::read_snapshot`.
    - **Memory.** The cache is charged once, to the Engine, up to
      `Engine::with_module_cache`'s cap (`DEFAULT_MODULE_CACHE`, 8 MiB;
      all bundled modules decode to about 2.9 MB [M]). Past the cap,
      sessions decode their own copy, and nothing is evicted. A session
      hit is charged the decoder workspace the module took, so a session's
      limits, errors and output do not depend on the cache.
    - **Not changed:** snapshots, build ids and restore.
    - **Gains on one Engine** [M]:
      - warm codemode-set restore 23.75M → 9.95M Ir;
      - heavy-set restore 60.2M → 16.6M Ir;
      - warm imports 20–33% fewer Ir;
      - RSS per session at N=40: −35% for the codemode set, −37% for the
        heavy set.
    - **Size** on 7541558 [M]: `hello.wasm` +1,862, native +2,208, guest
      +1,683 B, about twice the prototype's, for the Engine-level
      accounting. Ceilings were raised to the measurement plus about 2 KB.
    - **Bench Ir** [M]:
      - 12 programs within ±0.01%.
      - `startup`: +0.93% whole-process. +0.16% is the engine (`Engine::new`
        and the cache's drop). The rest is libc parsing two more
        `/proc/self/maps` lines at process start under valgrind, because
        the binary is larger.
      - **Accepted by the owner (2026-11-07)** as an exception to the
        ±0.5% bench rule, as with 13 and 39: the engine's share (+0.16%)
        is within the rule, and the rest is libc reading
        `/proc/self/maps` under valgrind, not engine work.
    - **Who gains.** One-session processes (plak's worker today, mm-wasm
      instances) gain nothing; in-process hosts such as host1's do.
      bench/evidence/results/2026-11-07-module-cache.

46. **Size pass 2: bundled data on stable** (owner, 2026-11-07). Lever 1
    (`#[optimize(size)]` on the cold breadth) waits for a pinned Rust
    1.101, with no `#![feature]` or nightly-only source before; its file
    list is in "Size pass 2". The bundled modules ship as nine sections
    packed against per-section codes, with shared strings stored once;
    section decoders live for one cell. `NATIVES` gets its own static.
    Ceilings 2,001,200 / 2,178,700 / 2,105,800. `mod-asyncio` and
    `mod-json` gate the engine's own modules (on by default); coroutines,
    `await` and host futures stay core; `cargo xtask check` covers the
    build without them.
47. **Built-in import images are deferred** (owner, 2026-11-07,
    docs/research/import-image.md). The prototype on `local/import-image`
    (unpushed, kept as is) grafts build-time images of `json`, `re`,
    `collections` and `asyncio`.
    - **What it buys [M]:** the five-module cell 18.55M → 11.66M Ir with
      the cache warm, 2.67 → 1.72 ms native and 6.86 → 5.67 ms on the
      mm-wasm guest.
    - **Why it is deferred:** that is about 1 ms once per importing
      session, still several times Monty's 0.3 ms. It costs 46–71 KB
      shipped (+17 KB of graft code plus packed images) and about 2,800
      lines.
    - **Moved to the import-cost work instead:** lazy `asyncio` attributes
      and an Engine-level prelude-unit cache.
    - **What could reopen it** (section 8 of the note): whether an image can
      replace its module's bytecode, which would bring the net size toward
      +17 KB. The estimate is about +30–35 KB for json, re and collections
      [E], because the image would have to carry the functions'
      instructions (`re`'s are 9.0 of its 15.1 KB packed [M]). It would
      lose the body path as a fallback (partial prelude clusters, feature
      mismatch, `MM_NO_IMAGES`). Restore would read code from the image,
      and graft-against-body checks would need a test build that still has
      the bytecode.
48. **asyncio's attributes on first use; the Engine also caches prelude
    chunks** (owner follow-up, 2026-11-07; docs/research/import-cost.md
    §5).
    - **Lazy attributes.** `import asyncio` binds its 38 prelude names to
      `UNDEFINED` (no Python code can hold it); the first lookup builds
      the unit, and reading `__dict__` builds the rest in one batch.
      `dir`, `hasattr`, `import *`, identity, submodules, errors and
      snapshots are unchanged (own case 417; durability at every fuel
      unit). `json` and `sys` stay eager.
    - **Chunk cache.** `ModuleCache` gains `prelude_chunk` and
      `offer_prelude_chunk` (default no-ops; `PRELUDE_CHUNKS` slots in the
      facade under the same cap). A hit charges the unpacking workspace,
      as decision 45's module hits do.
    - **Gains per session, native, cache warm [M]:** `import asyncio`
      6.01M → 0.16M Ir (758 → 12 µs); the five-module cell 19.55M →
      12.94M (2.83 → 1.92 ms); an asyncio primitives cell 8.93M → 3.64M.
      mm-wasm guest: five-module cell 7.36 → 5.86 ms, `import asyncio`
      1.32 → 0.06 ms. On the guest the cached chunks stay for the
      instance's life (+192 KiB linear memory for the five-module cell).
    - **Size** [M]: `hello.wasm` +2,571, native +3,312, guest +2,258 on
      168b7ac; +2,561 / +3,360 / +2,256 on e7d7b81 (after `mod-ast` left the
      default set) to 2,050,734 / 2,234,312 / 2,152,202; ceilings raised to
      the measurement plus about 2 KB (S1): 2,052,800 / 2,236,400 /
      2,154,300.
      Bench Ir within ±0.02%, `startup` −0.15%.
    - **Not done:** a cache of decoded unit programs (the primitives
      cell's remaining 1.24M of prelude work is decode and load).
    - **Off on the mm-wasm guest** (owner, 2026-11-07). The guest holds one
      session per instance, so cached chunks stay in its linear memory for
      the instance's life, and the 192 KiB per instance costs more than the
      80–180 µs they save. The guest builds its engines with
      `Engine::without_prelude_chunk_cache` (decoded modules are still
      cached within the session, as before); native and `hello.wasm` keep
      the chunk cache, and lazy asyncio stays everywhere. Guest, five-module
      cell, against c0f37cd [M]: linear memory 3,584 → 3,392 KiB, wall
      6,260 → 6,388 µs (median of 5 alternating rounds); guest −60 B
      (2,152,202 → 2,152,142), `hello.wasm` and native unchanged.

49. **0.1 API: outputs are `#[non_exhaustive]`, inputs have `Default`**
    (owner D1, 2026-11-07). `HostValue`, `Unconverted`, `Stream`, `Step`,
    `Phase` and the error enums are `#[non_exhaustive]`, as are the output
    structs `Completion`, `PyException`, `CompileError`, `HostCallRecord`,
    `HostFunctionRef` and the driver's `Call`; each output struct has a
    `new` for hosts that decode one (mm-wasm-abi, replay). `HostParam`,
    `HostClass` and `HostException` derive `Default` (`ParamKind` defaults
    to `PositionalOrKeyword`, `HostErrorKind` to `Exception`), so hosts
    build them with `..Default::default()`. `EngineConfig` is no longer
    `Copy`. A later variant or field is additive.
50. **Fuel is per cell, with an optional session total** (owner D2).
    `start` gives each cell `Limits::fuel`, or what `set_fuel` set since the
    last cell (once); `set_fuel` on a parked cell refuels it.
    `Session::set_fuel_total` caps all cells together: each starts with the
    smaller of its budget and what is left, and everything a cell charges
    (refuelled or not, async slices included) comes off the total; once
    spent, cells park on fuel at their first safepoint. Snapshots carry
    the total (format 4; superseded: 0.0.1 ships `SNAPSHOT_FORMAT` 1, and
    decisions 66 and 67 freeze the image's surfaces). `Engine::environment` and the new
    `Session::environment` describe exactly that ("N fuel units per cell"
    and "M left for the session"). `mm-cell-worker` sets no fuel (its
    `Limits::fuel` is `None` and it resumes fuel parks), so its behaviour is
    unchanged.
51. **mm-wasm v2 is frozen with 0.1** (owner D3; superseded: the interface
    froze with 0.0.1 at `ABI_VERSION` 1, docs/research/wasm-embedding.md,
    with decisions 66 and 67). `ABI_VERSION` 2 encodes
    the 0.1 error details (`DefineError` status 3, `ResumeError`'s phase,
    `SnapshotError::Incompatible { format, build_id }`, `Corrupt`'s text,
    `Internal`) and adds `mm_set_fuel_total`, `mm_fuel_total` and
    `mm_phase`. Every decoder refuses an unknown tag; encoders write the
    reserved `0xff` for a variant they have no code for. Within v2 only
    additive optional exports (docs/research/wasm-embedding.md).
52. **Licences and package contents** (owner D4). Every published crate is
    `MIT AND PSF-2.0 AND Unicode-3.0` and ships LICENSE, THIRD_PARTY.md and
    the README. HANDOFF.md, `spikes/` and
    `conformance/codemode/real/traces.tar.xz` stay in the repository; none
    is inside a published crate's directory, so no package contains them
    (`cargo xtask package` lists each package).
53. **Host misuse has its own errors** (owner D5, D6). `Session::define`
    returns `DefineError` (`Invalid` with the reason, `Busy`, `Raised` with
    a `MemoryError`, `Poisoned`) instead of a Python `ValueError`. A module
    named like one of micro-monty's own (bundled native or Python modules,
    enabled or not, `asyncio`/`json` submodules, `builtins`) is refused
    with `Invalid`, since `import` finds the bundled one first.
    `SnapshotError` is no longer `Copy`: `Incompatible { format, build_id }`
    names the image's, `Corrupt { what: String }`, and an engine panic
    during restore is `Internal { message }` (the image may be sound).
    `image_build_id` and `SNAPSHOT_FORMAT` are public.
54. **Park requests survive `start` and `cancel`** (owner D7). A request
    stays until a cell parks for it, so a host's kill or cancel cannot race
    a new cell. `mm-cell-worker`'s `Kill` and `Cancel` no longer re-request
    every 5 ms from a thread: one request each. The worker answers a stale
    `Requested` park (its `Cancel`'s cell already ended) with `Continue`.
    The mm-wasm guest's `mm_cancel` no longer clears the host's request.
55. **Publishing** (owner D8). The facade and the engine crates it needs
    (`mm-core`, `mm-compile`, `mm-parse`, `mm-engine`, `mm-stdlib`,
    `micro-monty`) publish, pinned to each other with `=`; MSRV is the
    pinned stable (`rust-version`). Each hashed its version and its own
    sources in its build script for `build_id()` until decision 66
    replaced the source hashes with a layout descriptor; either way a
    crates.io build of a revision has the workspace build's id (`cargo
    xtask package` checks it, in `check`). `mm-wasm-abi` is
    `publish = false` until `mm-wasm-host` publishes; `mm-cell-worker`
    stays unpublished while it depends on `plakous-cell` by git rev.
56. **Every public byte quantity is a `u64`** (owner, L6): `Limits::memory`,
    `MAX_SOURCE_BYTES`, `HostArgs::max_bytes`, `set_text_limit`,
    `snapshot_with_limit`, `head_tail`, the module cache, mm-cell-worker's
    `Config` caps (`max_print_bytes`, `max_result_bytes`,
    `max_nested_error_bytes`) and mm-wasm-host's `RuntimeConfig` and
    `memory_bytes`. Only the engine clamps to `usize`, where it allocates;
    a value past the address space is no limit, never a panic (sdk.md,
    "Limits"). Counts (`max_outstanding_nested`, `max_value_depth`) stay
    `usize`. The mm-wasm v2 encoding already carried `u64`s, so it is
    unchanged; snapshot bytes are unchanged too.
57. **The facade forwards every `mod-*` feature** (owner, M9). Its
    `default` feature is the default module set (the source of truth;
    mm-stdlib's own `default` is empty), so an embedder opts in to `ast`,
    `uuid`, `unicodedata` and `html.entities`, or out of `asyncio` and
    `json`, on `micro-monty` alone. mm-cli, mm-codemode and the mm-wasm
    guest use the facade's features. `mm-core`, `mm-compile`, `mm-parse`,
    `mm-engine` and `mm-stdlib` say "internal, no semver guarantees;
    depend on micro-monty" in their crate docs and their own READMEs.
    `tests/features.rs` checks that `asyncio`, `json`, `ast` and `uuid`
    import exactly when their facade feature is on, in the workspace build
    and in `check`'s build without `asyncio`/`json` and with `ast`.
58. **sdk.md is the crate documentation** (owner). `micro-monty`'s crate
    doc is `#![doc = include_str!("../sdk.md")]` (a symlink to
    `docs/sdk.md`, packaged as a file), and its ```` ```rust ```` blocks are
    doctests with hidden setup lines, so docs.rs and `check` read the same
    text. `cargo xtask docs` replaces `sdk-examples`: the doctests, then
    `RUSTDOCFLAGS=-D warnings cargo doc --no-deps` on the six published
    crates. The facade's rustdoc links to guide sections (`crate#limits`)
    or GitHub URLs instead of `docs/*.md` paths; the internal crates keep
    plain-text `docs/spec.md §n` references (no semver, not for
    embedders).
59. **`Engine::without_prelude_chunk_cache` is hidden** (2026-11-07). Of
    the cache-shape options, keeping `Engine::new`,
    `with_module_cache(config, max_bytes)` and `with_limits` reads best:
    folding the chunk switch into `EngineConfig` would break its 62
    struct literals and the recorded evidence commands, and only the
    mm-wasm guest needs it (decision 48; a native engine's chunks go with
    the engine). It stays public for the guest, `#[doc(hidden)]` and
    outside semver; the guest's behaviour is unchanged.
60. **Host-module code is compiled once per Engine; images refer to it by
    key** (owner-approved M8, 2026-11-07; HANDOFF "Host-module code per
    Engine").
    - **Sharing.** The program `define` generates for a module's typed
      functions, classes and exceptions is compiled by the first session
      of an Engine (or its clones) and kept in the Engine's module cache,
      as decision 45 keeps bundled modules: same byte cap, nothing
      evicted, past the cap a session compiles its own. Compiling was
      never charged to the session, so outcomes are the same cold, warm
      or off. The key is 128 bits over the generated source (`hash_bytes`
      and `xxh64`), so two definitions share code exactly when they
      generate the same Python.
    - **Snapshots (format 5).** Host modules are written before the code
      table. For code the Engine shares (the cache holds that same
      program), each module gets its key and *recipe* (the classes of
      other host modules its source names, in order), and the code table
      refers to it (`CODE_HOST`, module index and offset) instead of
      storing it. Otherwise the code is stored as before: past the cap,
      with the cache off, for an earlier definition a cell still holds,
      and on an Engine made with `with_host_code_in_snapshots(true)`.
    - **Restore never binds to other code.** The restoring Engine takes
      the program from its cache by key, or regenerates it from the
      definition and recipe in the image and compiles it (then caches
      it). Either way the program's key must equal the image's, or the
      image is refused (`SnapshotError::Corrupt("host code")`). (Since
      2026-11-08 the cache serves an image only the program built from the
      image's definition; "Pre-0.1 follow-ups" item 1.) Images
      restore only on their build (decision 38), so the generator is the
      same. The host does not have to define anything first.
    - **The mm-wasm guest stores its code**
      (`with_host_code_in_snapshots(true)`): its instance restores every
      image on a new Engine, where compiling again would cost what the
      `define` did, while stored code only decodes.
    - **API, all additive:** facade `Engine::with_host_code_in_snapshots
      (&self, on: bool) -> Engine`. Engine crates (no semver):
      `mm_engine::HostProgram` (key, program, recipe; `bytes()`),
      `mm_engine::HostCode` (the restore callback type),
      `ModuleCache::host_program` / `offer_host_program` (default
      methods), `Source::host`, `Vm::module_cache`,
      `Vm::set_host_code_in_snapshots`; `Vm::read_snapshot` takes the
      `HostCode` callback; `Vm::run_host_defs` and
      `Vm::define_host_module` take the program. mm-wasm v2 does not
      change; the guest's images store their code as before.
    - **Size** [M] (ceilings raised to the measurement plus about 2 KB,
      S1): `hello.wasm` +27,228 to 2,081,909; native +26,720 to 2,265,240;
      guest +6,467 to 2,166,240. Most of `hello`'s growth is the restore
      path regenerating code (the generator and `decode_module` now link
      into every build that restores); a variant that refuses images
      whose code the Engine lacks measured about +4.6 KB native.
      **Owner (2026-11-07): keep regeneration**, so restore stays
      self-contained. The generator is the code `define` uses, so a host
      that defines typed tools links it anyway: native `hello` plus one
      typed `define` (built as `cargo xtask size` builds `hello`) grew
      +4,976 B (2,289,032 → 2,294,008), against `hello`'s +26,720 [M].
      The other ~21.7 KB lands on builds that never define typed tools,
      such as `hello`.
    - **Bench Ir** [M]: every program within ±0.01%; `startup` +0.79%
      (898,848 → 905,966), of which the engine's code is about +167 Ir
      and the rest libc reading `/proc/self/maps` under valgrind for the
      larger binary. **Accepted by the owner (2026-11-07)** as an
      exception to the ±0.5% bench rule, as with 13, 39 and 45.
    - **Keyed images stay the default** (owner, 2026-11-07). A host with
      one session per process or instance turns on
      `with_host_code_in_snapshots(true)`, as the mm-wasm guest does and
      sdk.md says; a keyed image restored on a new Engine compiles again
      (100 tools: 0.85 → 6.6 ms; stored code 0.87 ms) [M].
    - **Per-tool image bytes left** [M]: about 2.1 KB raw per typed tool
      with a docstring (260 B for an untyped function): the session's
      own function and signature objects, the definition, its help stub
      and the generated messages. Removing those needs functions built
      on first use, not a cache. **Deferred by the owner until after
      0.0.1** (open item below).
61. **An uncaught exception's report is bounded by the text limit and the
    deadline** (owner, 2026-11-08, option (a) for release-fuzz finding C2;
    `conformance/breadth/fuzz/README.md`).
    - **The finding.** `Session::error` rendered `exception_parts` and the
      traceback unbounded: a 75-byte cell keying a dict with a frozenset
      that shares itself 24 levels deep (its `repr` doubles per level) took
      9 s in release and 196 s under ASan and built a report of about
      96 MB, past plak's 8 MiB frame \[M\]. Exception groups that share
      sub-groups multiply the same way.
    - **The cap.** Rendering the message (`str`) and the traceback stops
      once the text passes the session's text limit (default 1 MiB, or the
      memory limit if less), the walk a completion's `repr` gets: the limit
      with `TextCut::Head`, eight times it (at least 1 MiB) with
      `HeadTail`, never past the memory limit. Each text is then cut with
      the completion's `TextCut` and markers (`... [N bytes omitted] ...`,
      or ` ... [more omitted]` when rendering stopped). Inside a report a
      `repr` that hits the cap keeps its cut text instead of raising
      `MemoryError` (`Vm::begin_report`/`end_report`).
    - **The deadline.** Rendering also stops once the session's deadline
      has passed, checked every 128 `repr` steps and, after 64 exceptions,
      per exception of the chain or group; the text is cut the same way.
    - **One flag:** `PyException::truncated: bool`, true when the message
      or the traceback was cut, also an argument of `PyException::new`
      (the struct is `#[non_exhaustive]`). One flag rather than one per
      field: a host's choice is the same (store the full value elsewhere or
      tell the model the report was cut), and the traceback ends with the
      message, so a cut message almost always means a cut traceback.
    - **Changed for small limits:** the text limit now bounds reports as
      well as completions, so a host that sets it low (the parity test's
      40 bytes) gets cut tracebacks too; plak and `mm-cell-worker` use
      1 MiB.
    - **Unchanged:** a report under the limit is byte-identical to
      before, with `truncated` false; `mm-cell-worker` keeps its 16 KiB
      nested-error and 1 MiB result caps, and a cut report fails its cell
      cleanly within them (`tests/library.rs`).
    - **mm-wasm v2** appends the flag as a byte after the traceback. v2
      has no consumers yet, so there is no version bump (an exception to
      decision 51's freeze); the parity test compares it.
    - **Measured** [M] against e96ca7c: the C2 cell in release `mm` ran
      9.47 s at 267 MB peak RSS and still printed `KeyError: <exception
      str() failed>`; now 0.04 s at 8 MB, with message and traceback cut
      at 1 MiB. Eight levels of `ExceptionGroup`s that each hold one
      shared group 15 times render in 0.02 s (CPython had not finished
      after 20 s). Size: `hello.wasm` +767 B to 2,083,018; native `hello`
      +768 B to 2,266,320; guest +696 B to 2,167,274, all under their
      ceilings. Bench Ir: every program within ±0.02% (`json` +0.013%,
      `startup` +0.005%).

62. **No stdlib profile feature; the feature graph is checked** (owner,
    2026-11-08, after docs/research/stdlib-profile.md).
    - **No `stdlib-codemode-core` feature ships.** The measured code-mode
      list (34 features) saves about 4% of every shipped build and fails
      42 of the 1,782 synthetic codemode solutions, with no speed or
      memory gain. A named feature would be another configuration to test
      (build id, size, durability, wasm host). docs/sdk.md ("Smaller
      module sets for code-mode agents") gives it, and the smallest list
      every recorded plak cell succeeds on, as feature lists to copy,
      with their sizes and losses.
    - **The `mod-*` graph is complete for import time.** Ten imports that
      bundled modules run when imported were missing from their
      features: `typing` → `collections`, `functools`; `functools` →
      `collections`; `random` → `itertools`, `operator`; `datetime`,
      `time`, `os`, `hashlib` → `operator`; and `collections.abc` (shipped
      by `mod-collections`) → `abc`. A manifest-allowed list such as
      `["mod-typing"]` built a `typing` that failed at import. mm-stdlib's
      build script (`check_feature_graph`) now parses every bundled module
      and `Cargo.toml`'s `[features]` and fails, naming the module, the
      import and the feature to add, when an import-time import is not
      turned on. Imports inside functions stay outside the graph
      (docs/spec.md §12 lists the four that need a second feature).
    - **`mod-math` is gone.** `math` is an engine module and always
      built; the feature gated nothing. `FEATURES` leaves bit 1 clear.
      Dropping it changes the default builds' bytes though no code
      changes: a crate's enabled features feed Cargo's metadata hash,
      which orders constants and strings, and `FEATURES` itself changes.
      Against 3720bf2 the three builds keep their lengths (2,083,018 /
      2,266,344 / 2,167,274), with 10, 242,284 and 2,436 bytes different
      (hello.wasm, native `hello`, guest) [M]; mm-stdlib's `src/lib.rs`
      changes the build id. It was held for a 0.0.1 byte-identical to 10cf9f6, then
      landed before 0.0.1 when the owner moved the release fuzz to the
      combined commit.
    - The graph fix and its check alone (3720bf2) leave the default builds
      `cmp`-identical to 10cf9f6 [M] (hello.wasm 2,083,018, native `hello`
      2,266,344, guest 2,167,274), with the same build id: every module
      they add was on in the default set already, and the build script is
      not part of any fingerprint.

63. **The bench Ir rule applies to the gated count** (owner, 2026-11-08).
    `startup` broke the ±0.5% rule six times (decisions 13, 39, 45 and 60,
    and the string-indexing and small-items branches). Each time the cause
    was glibc's `pthread_getattr_np` parsing more or fewer
    `/proc/self/maps` lines because the binary's segments landed elsewhere,
    not engine work. `bench/callgrind.py` now counts each program twice.
    - **Gated**: collection on at glibc's `__libc_start_main` and off
      inside `pthread_getattr_np`. The rule applies to this count.
    - **Whole process**: as before, still reported.
    - **Why not `main` onward.** Rust's runtime calls `pthread_getattr_np`
      from inside `main` (the main thread's stack guard), so a count from
      `main` keeps the parse. The shipped binaries are stripped, so `main`
      cannot be named to callgrind anyway. On decision 60's pair
      (9671394^ → 9671394) `startup` measures +0.80% whole-process, +1.06%
      from `__libc_start_main`, and +0.027% gated (560,265 → 560,418 from
      `(below main)`; 561,293 → 561,444 as implemented) [M].
    - **Every program switches**, so there is one rule. Outside `startup`
      the gated count is the whole-process count minus a constant of about
      337K Ir (0.10% of `fib`, the smallest). On that pair every other
      program moves the same in both counts (within ±0.02%).
    - The gated count needs a glibc binary. callgrind.py fails a program
      whose gated run collects nothing.

64. **The host keeps control, without exceptions, before 0.0.1** (owner,
    2026-11-09, option (b) of the fuel-audit follow-ups). The 0.0.1 release
    fuzz restarts on this work. docs/spec.md §5 describes the mechanisms.
    - **`collect` parks between chunks.** An iterator that need not end
      is gathered `COLLECT_CHUNK` (2^16) items per native step; the rest
      goes on in `_collect_rest`, which calls the native again on the
      complete list. Lists, tuples, ranges and container iterators stay
      native. `list(enumerate(range(10**18)))` ran 0.49 s to `MemoryError`
      deaf to fuel and deadlines (`dict(zip(...))` 0.67 s); it now parks in
      a 100,000-unit slice and on a 50 ms deadline, through a snapshot.
      `map`, `filter` and generator expressions are prelude generators, so
      `list(map(...))` already parked; the native iterators that did not
      were `enumerate`, `zip` and the `itertools` natives. `list(gen)` over 1M items: 131 ms
      before, 123 ms after; `list(map(abs, range(10**6)))` 172 ms both
      (best of 7) [M].
    - **Long sorts and big factorials ask first.** `sorted`/`list.sort`
      of a list or tuple estimate n·⌈log₂ n⌉ visits, and `factorial`,
      `perm` and `comb` the product tree's Karatsuba cost, in
      `Vm::native_short`, as `pow` does. `factorial(400_000)` asks for
      298M units (2.8 s) where it charged 237,800 [M].
    - **The sweep** (release, native, default limits; the longest gap
      between safepoints under a deadline re-armed every 2 ms, before →
      after [M]):

      | Site | Before | After |
      |---|---|---|
      | `ast.parse`, one 1.8 MB non-ASCII line (`Lines::loc`) | > 80 s | 1.0 s whole parse (see below) |
      | `tokenize` lexer, same line, 0.6 MB | quadratic | 113 ms |
      | `re` set of 20,000 chars, `findall` over 5M | 18.4 s | 32 ms |
      | `strptime`, 5M-space run | 5.1 s | 26 ms (`_mm`), 71 ms (`datetime`) |
      | `re` with 50,000 groups, `search` over 2M | 1.9 s | 38 ms |
      | `B.x = v`, 60,000 subclasses (`class_changed`) | 1.3 s each | 12 ms |
      | `Root.__eq__ = f`, 6,000-deep chain (`compute_flags`) | 3.0 s | 89 ms (7,000 deep) |
      | building that chain (C3, flags per class) | 52 s total, 3.7M units | 4.4 s, 29M units |
      | `traceback.format_exc`, 950 frames, 240 KB source | 987 ms | 1.5 ms |
      | `str()` of 1M small lists, no fuel limit | 558 ms | 154 ms (a `join` and a collection) |
      | `repr` of a 500,000-entry dict | 124 ms | 77 ms |
      | `setattr` of 300,000 new names (`shape_add`) | 41 ms | 41 ms, now charged |
      | `int()` failing on 10M chars | 107 ms, uncharged | 86 ms, charged |
      | `complex()` failing on 10M chars | 37 ms, uncharged | 52 ms, charged |
      | `str.replace`, 10M matches | 0.55 s for 6 ms of fuel | charged 4 units per match |

    - **Long sorts run in slices, and strings sort faster.** A sort of
      more than about 2^17 visits (`sort_is_long`) parks between slices
      of its merge (`Vm::sort_more`, `_sort_rest`): runs of 1,024 items
      sorted in place first, then a bottom-up merge, the order the same
      stable one. Keys that are all small ints, all strs or all floats
      compare directly. 1M items, best of 5 [M], 5df122e → this:
      strs 1,703 → 676 ms (CPython 3.14: 270), sorted strs 1,835 → 130
      (67), reversed strs 1,694 → 773 (261), ints 282 → 112 (123), floats
      338 → 103 (112), `list.sort` ints 277 → 108 (105), tuples 799 → 812
      (930), `key=len` 260 → 247 (41). The 1M-string sort's longest gap
      between safepoints is now one slice; tuples cost about 2% more
      (merging through heap arrays).
    - **C4: straight-line code checks** (host1, fuzz run 3). The compiler
      puts a `CHECK` after 32 instructions that may work in proportion to
      their data (spec §8.6); it runs the safepoint only when a native
      charged more than 64 units or spent the fuel. Snapshot format
      unchanged; park positions add the `CHECK`s; the build id changes
      with the opcode table. Repros [M], 5df122e → this: 20,000 ×
      `b = s == t` over 10 MB strings, 50 ms deadline, Complete after
      74.7 s → parked after 86 ms; 2,000 of them with fuel 100, Complete
      after 7.5 s → parked after 42 ms; `s < t` 37.7 s → 62 ms; `'b' in s`
      41.0 s → 69 ms. Still open: up to about 32 long statements run
      between two checks, so over 60 MB strings (one `==` 25 ms) a 1 ms
      deadline waited 757 ms [M]; phase 2 of the bounded-steps work.
    - **Keyed sorts with a builtin key** (found by the new deadline test,
      as on 5df122e): `sorted(xs, key=len)` called the key from a native
      continuation, and a builtin returns without a safepoint, so 300k
      keys were one 60 ms step. Every 4,096th key is charged for the
      chunk and called through the prelude's `_invoke`, whose call
      checks: 8 ms [M].
    - **Size and Ir.** +33,193 B `hello.wasm` (2,128,617), +40,000 native
      `hello` (2,319,112), +33,166 guest (2,214,240) on e8900af, ceilings
      raised to the measurement plus about 2 KB (S1). Of `hello.wasm`'s
      growth, the chunked `collect`, charges and sweep are +12,391, the
      sliced sort +22,032 and C4 −1,213 [M]. The sort's merge loop runs
      once per key kind; one shared loop saves 11,551 B (native 16,672)
      and costs 15–23% on sorts of strs, ints and floats (1M strs 717 →
      833 ms) [M]: the owner's call. Gated bench Ir against 5df122e:
      `startup` +0.43%, `fmt` +0.09%, `sort` −0.74% (the faster int sort),
      every other program within ±0.04% [M]. Sharing the flags loop out of line (`own_flags`)
      had moved a register spill into the dispatch loop's global load,
      +0.54% on `loop`; it is `#[inline(always)]`.
    - **Open: phase 2** (owner, 2026-11-09: 0.0.1 waits for "no native
      step exceeds about 100 ms" as an engine invariant). One native step over a single object near
      the memory limit is still linear in its size, above 100 ms: building
      a 100 MB `str` 164 ms, `s + 'x'` 400 ms, `s.replace` with 100M
      matches 4.5 s, `int()` failing on it 0.97 s, `complex()` 0.57 s,
      `float()` 1.2 s (`MemoryError`: its message is the full repr, as
      CPython's). `ast.parse` and `tokenize` read guest text of any size in
      one step (0.34 s for 700 KB); the facade's compile refuses sources
      over 256 KiB, and applying that limit to them would bound it near
      100 ms. A collection with 3M live objects pauses 66 ms. Each needs a
      decision: chunked natives, smaller single-object limits, or accepting
      the bound. Taken up as scope decision 65.

65. **No native step longer than about 100 ms, as an engine invariant**
    (owner, 2026-11-09; 0.0.1 waits for it, and the 24 h release fuzz
    restarts on its last commit). The threshold enforced is 50 ms of
    native release time between two safepoints at the default limits, so
    mm-wasm (1.5–3 times slower [E]) stays under its epoch grace (100 ms plus 0.75 ms per MiB of the memory limit).
    The mechanism, the oracle and the rules are docs/spec.md §5, "Bounded
    native steps".
    - **Shared mechanism (landed first).** Chunked natives: a native's
      first step stops after `STEP_WORK` (2^17) charged units (`Vm::work`),
      saves its progress in an `NSTATE` object and requests `_steps`, which
      calls `_more(state)` a step at a time; no new `Cont` kind, no
      snapshot format change. First user: `x in` a list or tuple (231 ms
      for 10M items in one step → about 7 ms a step [M]). The oracle,
      `crates/micro-monty/tests/steps.rs`, runs in `cargo xtask check`.
    - **Sorts made fully bounded.** The first step of a long sort copied
      the input twice (4M ints: 115–129 ms) and the last copied the result
      back (33 ms). Now the arrays are allocated, not copied: `sorted` of
      a list without `key=` reads it in place, `list.sort` moves its buffer
      (`Vm::list_move`), pass 0 fills the first array from the input,
      reading it reversed for `reverse=True`, and a last pass reverses or
      maps indexes to items a slice per step; the keys list is allocated
      with room for every key. Every sort step of 4M ints is under 33 ms;
      what remains in the longest is allocating the 32 MB arrays and the
      collection after it [M].
    - **Size and Ir.** +7,656 B `hello.wasm` (2,151,125), +12,016 native
      `hello` (2,350,552), +7,591 guest (2,236,708) on 6dfe504 [M]. The
      sort's finishing pass works through 256-item stack buffers: a
      per-item loop saved about 1 KB but cost `sort` +5.4% Ir (all four
      of its sorts are keyed, so every item is mapped). Gated bench Ir
      against 042afb8: `sort` +0.04%, `fmt` +0.02%, `interp` −0.13%, every
      other program within ±0.01% [M].
    - **Fixed on the way:** with `list.sort` moving its buffer, a
      `MemoryError` allocating the merge arrays left the list empty;
      the first step now gives the items back as later steps do
      (`tests/session.rs` caught it).
    - **One object is at most 64 MiB** (owner decision (a), 2026-11-09):
      `mm_core::MAX_OBJECT_BYTES`, checked in `Space::new_large`, the one
      place every block over 128 KiB is placed, so a bigger str, bytes,
      bytearray, list or tuple buffer, or dict or set table is a
      `MemoryError` whatever the memory limit; no facade, `Limits` or ABI
      change. `list_push` doubling and `bytearray`'s over-allocation stop
      at the cap, so a list grows to 8,388,608 items and a bytearray to
      64 MiB by appends. Tables double, so a dict holds at most 2,097,152
      entries and a set 2,516,581 items [M] (CPython has no such limit; since 2428933 a set holds 5,033,164, spec §11).
      Natives whose whole work over one object at 64 MiB stays under
      50 ms need no chunking. Two `steps.rs` rows over the cap moved to
      it (`list(range(8M))`, `'ab' * 20M`). The same commit takes the
      NSTATE plumbing out of line (`nstate_words`, `nstate_set_raw`, a
      new `nstate_put` for runs of raw words): about 160 B of wasm per
      step call site were inlined, −3,325 B on P3's branch [M].
    - **Fuel overdraft is bounded and checked** (docs audit, 2026-11-09):
      every `steps.rs` row also runs on 1,000-unit fuel slices, and a
      `Bounded` row fails if a slice charges more than 2^21 + 2^17 units
      past what it was given (one bounded step: a chunked step charges
      about `STEP_WORK`; one unchunked operation over one 64 MiB object
      up to 2^20 read plus 2^20 built), unless the operation parked first
      with `fuel_wanted` and the slice gave that. On 7dfa304 [M]: chunked
      `in` 130,074, the sorts 0 (they park first), the C4 compare 936,500;
      the open rows `s.replace` 83.1M, `json.loads` 8.4M, `dict(zip)`
      3.6M, `a & b` 3.0M, `str(xs)` 2.2M. Returns are not safepoints (they
      charge no tick; the returns between two safepoints are at most the
      frames on the stack): the docs said otherwise and now do not.
    - **Work packages** (helper threads, after the mechanism; each touches
      its own files, a row of `steps.rs` and its own own case; rule: admit
      a guest-sized Rust buffer before filling it, §11). Landing order P4,
      P3, P1, P2, P5, then integration and the final gates.
      - P1 str (strmeth.rs, text.rs, percent.rs): `replace` (820 ms for
        25M matches), `split`, `join` (the 1M-piece join of `str(list)`,
        70 ms), repeat, case maps, encode/decode, format widths, `s + 'x'`.
      - P2 bytes (bytes.rs, bytearray.rs, binascii.rs, hashlib.rs): the
        same for bytes and bytearray, base64, hash updates.
      - P3 numbers (intops.rs, complex.rs, math.rs, mm-core bigint): failing
        `int()` (124 ms on 30M chars), `float()`, `complex()`; big-int
        operators per decision (b).
      - P4 containers (dict.rs, set.rs, the list paths of builtins.rs and
        ops.rs): the dict built from `dict(zip(...))` (463 ms for 2M
        pairs), set algebra (241 ms), `count`/`index`/`remove`, copies,
        `extend`, slice assignment.
      - P5 json and parsing (json.rs, astmod.rs, tokmod.rs, mm-parse
        lines.rs): `json.loads` (216 ms for 5M items), `dumps`, ast and
        tokenize per decision (c).
        Done for json and `unicodedata` (docs/spec.md §5): longest steps
        [M], before → after, release native: `loads` of 5M ints 222–241
        → 36 ms, of a 60 MB string 128 → 8 ms, of 100M digits failing
        392 → 1 ms, of a 100M-digit float 159 → 5 ms; `dumps` of 5M ints
        218 → 6 ms, `indent=2` on 3M 198 → 6 ms, `sort_keys` on 1M keys
        398 → 34 ms, a 60 MB str 146 → 15 ms, `['é' * 10M]` 442 → 8 ms;
        NFD of `'é' * 5M` 1.3 s → 2 ms, NFC of `'e\u0301' * 3M` 669 →
        1 ms, NFKC/`is_normalized` of 60M ASCII 55 ms → under 0.1 ms.
        A big object's table grew in the step's end-of-step flush, after
        a whole step of reading (600k keys: 50–60 ms; 1M: 51–56 ms): a
        step now ends when its items could grow an open object's table of
        2^16 entries or more, so the rebuild follows at most 1,024 values
        (600k keys 31–34 ms, 1M 37–38 ms, on ce8cb0e [M]).
        A 2M-key object loaded with one 82–84 ms step, the 2^20 to 2^21
        rebuild alone: the table now grows a slice per step through P4's
        `Vm::dict_grow_slice` (worst step 25 ms; 1M keys 26.7 ms, 600k
        22.7 ms [M]), also before a child closes into a nearly full
        object. `dumps` of 1M small dicts was 60 ms (a full collection,
        decision (d); Bounded since the GC thread's 9ad158b). NFC of one char and
        1M combining marks (161 ms in one step, no slice boundary) is
        closed by a cap (owner-approved option 2, 2026-11-10): a run with
        no boundary over 65,536 chars after decomposition raises
        `MemoryError` (worst step at the cap 10–15 ms, overdraw at most
        2.13M units [M]). The 128 Ki cap first proposed charged 4.19M
        units in one piece, past the 2^21 + 2^17 overdraft bound; stepping
        the run itself is on the 0.0.2 list.
        **Owner exception (2026-11-09): gated `json` bench Ir +2.1%,**
        1,138.38M → 1,162.50M on ce8cb0e [M] (+1.9% on cf4aa86, +2.6%
        before `dumps` wrote one growing block; every other program
        within ±0.05%). Inside the
        natives: `dumps` 290.9M → 296.2M, `loads` 321.8M → 337.9M. `dumps`
        appends each step's text to one byte block whose room is reserved
        ahead without a fill (`Heap::append_bytes`), so the text is copied
        once, as before the steps; what remains is the encoder keeping its
        containers as levels it can save between steps. The scanner went
        from one recursive-descent loop over a copy of the whole input to a
        state machine over a window of it, because a step must be able to
        stop between any two tokens and go on from heap state alone: each
        token now passes through a state dispatch (value, first item,
        item, key, colon, delimiter), keys and numbers go through shared
        helpers rather than inlined code, and a window ends at a cutoff
        where a token is read again; window size makes no difference
        (2^18 to 2^20: 342–345M on cf9c214). Small payloads typical of
        code mode (1–50 KB of mixed rows) on ce8cb0e, Ir a call: `loads`
        +1.0–1.4%, `dumps` +0.1–0.4%; wall time `loads` 0.7–4.6% less,
        `dumps` −0.3 to +2.8% [M]; a jump table over the encoder's per-value
        outcome had cost `dumps` +5% wall on them through indirect
        mispredicts, now three tests in turn. A dict loop read through
        `Heap::read_values` changed that function's code for every caller
        (`generators` +0.4%); json reads one slice instead.
        Decision (c), owner-ruled 2026-11-09: a source cap, `MemoryError`
        past it. At 256 KiB of adversarial source `ast.parse`'s longest
        step was 236 ms (`'x=1\n' * 65536`), 237 ms (non-ASCII names),
        110 ms (one-line list), `tokenize`'s 77 ms [M]. The worst shape
        found is a statement or token every two bytes (`'a\n' * n`):
        `ast.parse` 50 ms at 48 KiB, so capped at 32 KiB (29–38 ms native
        release; 47–83 ms in `mm` for wasm32-wasip1 under wasmtime,
        under its 100 ms grace), `tokenize` 37 ms at 128 KiB, capped at
        64 KiB (about 22 ms native, 24 ms wasm) [M]. Not fixed here: a
        one-line source makes `tokenize.py` copy the line into every
        token's `line` (`''.join` of one line), so one-line sources of
        64 KiB can still end in `MemoryError`, after up to 10 s of steps.
    - **Mine after the mechanism**: C4's granularity (up to 64 long
      statements between two checks: 671 ms over 60 MB strings [M]; every
      per-instruction check tried so far moved `loop` Ir by +3.8% to
      +16.7% through `dispatch`'s codegen), a deadline oracle in the fuzz
      targets, and a systematic search for long steps.
    - **C4 granularity** (spec §8.6): an operator that would start after
      a native charged more than 64 units at once since the last safepoint
      stops before it starts, and `execute` runs the safepoint and the
      instruction again, or parks before it (`Vm::stop_before`, the
      fuel-short park's path). The hooks sit in `op_binop`, `op_compare`,
      the long-int branch of `unary` and `convert`, out of line and called
      only by the dispatch loop; a first try that put one in the inlined
      `op_unary` cost `loop` +10% Ir. 64 × `s == t` over 60 MB strings:
      longest gap 671 ms → about 25 ms [M]; snapshot format unchanged, new
      park positions before those operators.
    - **repr past its slice, linear again** (regression from 9589c84,
      found by the GC thread): 30,000 nested dicts took 32 s in release
      (0.08 s before the slice), 555M fuel units [M]. Three costs per level
      of the prelude twin: it built each level's text, a string the size of
      the subtree, whose allocation from 64 KiB requests a full collection
      (84% of the time); every `_walk_repr` copied the whole active set
      into the walk's own; and each level tried a full slice again. Now the
      pieces of every level go to one list joined once, the walk looks a
      container up in `_REPR_IDS` when it meets one, and after a failed
      walk the next 1, 2, 4, ... containers render without one: 0.12 s,
      2.0M units [M]. `repr_traversal_shares_the_memory_budget` runs with
      20M fuel (debug: about 310 s → 1.8 s); own case 465 checks the
      twin's shapes against CPython. GC is the GC thread's (decision
      (d)); its oracle row is report-only.
    - **Large results collect only when memory is short** (the GC
      thread, from the item above). Every str, bytes or int result of
      64 KiB or more requested a full collection at the next safepoint,
      so beside 1M live small lists 200 `t = s + 'x'` on a 100 KB `s` took
      4.6 s, the same with bytes 4.6 s, 50 `n + 1` on a 106 KB int 1.2 s
      (60 KB: 8 ms; CPython 1 ms) [M]. Now such a result requests one only
      once the bytes allocated since the last collection reach half the
      memory left (`Vm::collect_for_result`); before that, its fuel charge
      already makes the next safepoint check `Heap::should_collect`. The
      request cannot simply go: natives cannot collect, so a tight session
      building 1 MB results in a loop needs each iteration's garbage gone
      at the back edge (dropped outright, an 8 MiB session raised
      `MemoryError` where main passed); with the rule, 5–16 MiB sessions
      pass and fail as before. The three loops take 10, 10 and 8 ms [M].
      Gated bench Ir against 4e67449: `str` −3.5% (fewer collections), an
      improvement outside ±0.5%, accepted as phase 1's `sort` −0.74%;
      every other program 0.00% [M].
    - **GC pauses: incremental marking over 32 MiB live** (decision (d),
      owner, 2026-11-09: option (a) of the GC thread's report, threshold
      plus give-up; cards and lazy sweep alone not taken; mm-wasm-host's
      grace already scales, 100 ms + 0.75 ms per MiB of limit). Spec
      §9.2. A full collection costs about 0.4 ms per live MiB natively
      and 0.55 ms on the guest [M]; from `gc::MIN_LIVE` (32 MiB live after
      the last collection) a due collection marks 2^18 work units per
      safepoint on the segments' existing `DIRTY_REFS` flags (no new
      barrier), remarks the roots and the dirty segments in one step, then
      sweeps lazily. The bench programs never get there.
      - **Two triggers.** Below 32 MiB a collection runs in full and is
        also due once live plus allocated bytes reach 64 MiB: with the
        ratio trigger alone a heap growing through the threshold was marked
        in full at up to 3 × it (48 MiB: 44–51 ms; now 30 ms at worst
        [M]). Over it, nothing is freed until the remark and natives cannot
        collect, so a collection is also due once the room left is no more
        than the bytes allocated since the last one, and one whose room is
        under half of those runs in full. Without that, `d = {i: i for i in
        range(1_500_000)}; e = dict(d); f = list(e.items())` raised
        `MemoryError` at 256 MiB where full collections pass; on a scan of
        four near-limit programs beside 0–1.2M junk lists, incremental
        failed 11 cases full collections pass, now 1, and passes 8 they
        fail [M].
      - **Bounded** [M], longest gap from a park request to the park,
        native, incremental / full: garbage loops 1.3 / 24, 2.5 / 50,
        2.1 / 88 ms at 58 / 125 / 199 MiB live (256 MiB limit) and
        6.1 / 340 ms at 778 MiB (1 GiB); in-order writes 2.2–2.7 /
        28–109 ms and 4.8 / 378 ms; appends 2.4–5.6 ms, except near the
        limit (199 MiB of 256: 68 / 87 ms, the headroom rule). Wasm from
        the prototype: garbage 3.2 ms at 199 MiB, 7.7 ms at 778 MiB.
        `steps.rs` gains two Bounded rows (6M garbage iterations beside 2M
        and 3M live lists: about 13 and 2 ms). On 5ba19df a session near
        the limit (199 MiB live of 256) has too little room left and its
        collections run in full: 59–62 ms, against 98 ms with full
        collections only (same harness).
      - **Pacing** (after P4, 5ba19df). P4's stepped set and dict builders
        allocate tables of tens of MiB in a few steps, so a slice per
        safepoint fell behind: building two 2M-item sets ended with a mark
        holding about 190 MiB of garbage, and `c = a & b` then raised
        `MemoryError` (a Bounded row). A step now marks a slice plus a unit
        per word allocated since the last step, and a mark finishes in the
        step where the room left is no more than the bytes allocated since
        it began. `c = a & b` 12.6 ms; P4's two Open rows become Bounded,
        `dict(zip())` of 2M pairs 28.5 ms and `a |= b` of 1.25M + 1.25M
        45.5 ms (P4: 52–68 and 51 ms, full collections) [M]. Finishing at
        half that room with 4 units a word moved those to 44.5 and 44.3 ms
        and `c = a & b` to 29.3 ms, so it was not taken. Memory probes:
        only set_build at 256 MiB moves, run peak 0.75 → 0.94 of the limit
        (`MemoryError` either way).
      - **Not bounded: new live data built during a mark.** Objects
        allocated during a mark are unmarked and traced only by later
        passes or the remark, so `k = d.items() | {(1, 1)}` over a 2M-entry
        dict (2M new tuples, 53 MiB) has one step of about 94 ms, against
        about 55 ms for its steps without GC (130 ms without the per-word
        pacing). The row is Open on main anyway (1b3723a: P4's steps there
        run 46-62 ms). Owner, 2026-11-09: land with this exception;
        allocating black during a mark, per segment in the allocator's slow
        path, is required before the RC. (Superseded 2026-11-10: park first
        replaces it, "GC: park first" above; the row is `Bounded` since 418bd51.)
      - **Not bounded: scattered writes into a large old heap.** The
        rescans never converge; without a give-up the remark took 155 ms
        at 199 MiB and 841 ms at 778 MiB native (194 ms wasm at 199 MiB)
        and the loop ran 2.3× slower native, 1.9–2× on wasm [M]. The
        give-up: rescans past 25% of the first mark, or a pass that does
        not halve the dirty segments, abandon the mark for an immediate
        full collection, and the next 4 due collections (doubling to 16)
        run in full. Prototype a7ccc81 [M]: native wall time against full
        collections +7% / +9% / +12% / +5% at 58.7 / 125.9 / 199 /
        778 MiB (3.28 / 4.14 / 17.2 / 31.2 s, against 3.55 / 5.17 / 36.4 /
        61.9 s without it); wasm 4.95 / 11.7 / 7.6 s against eager 4.96 /
        10.6 / 6.2 (55 MiB, 199 MiB, 778 MiB with 3M iterations). The
        landed policy, native: 3.5 / 4.3 / 17.8 / 34.0 s against 3.2 /
        4.3 / 16.8 / 32.9 s. Pauses are a full collection's (75–1,730 ms
        native; 106 / 499 / 934 ms wasm). Allocation and local writes
        rescan 0–2% of their first mark and never give up.
      - **Throughput** [M], native wall, incremental against full: garbage
        +7% / +23% / +9% / +11%, in-order writes +16% / +9% / +15% / +15%
        at 58 / 125 / 199 / 778 MiB; appends +27–81% on runs of 0.1–0.4 s
        (one collection each), −14% at 778 MiB.
      - **Durability**: mark state is outside the arena and never
        snapshotted. A snapshot, `gc.collect()`, code reclamation or a
        native's request finishes the mark as a full collection; a
        restored heap starts unmarked; image format and meaning unchanged.
        `tests/snapshots.rs` restores six snapshots taken mid-mark.
        `cargo xtask gc-stress --incremental` (`MM_GC_STRESS=inc`: slices
        of 16 units from 0 bytes live, every remark checked against a full
        mark, give-up off) reruns the own cases, the corpus and durability
        with a mark nearly always in progress; the oracle passes
        `MM_GC_STRESS` to the wasm side of `--cross-target` chains too.
      - **Ir and size** [M]: gated bench Ir against 5ba19df within
        ±0.5% on all 13 (startup +0.20%, dict −0.33%, str −0.42%, fmt
        −0.21%, the rest within ±0.11%; fib, loop, closure +0.01–0.02%).
        +11,525 B `hello.wasm` (2,214,612), +11,152 native `hello`
        (2,420,040), +11,350 guest (2,301,009) on 5ba19df; ceilings raised
        under S1 to 2,216,600 / 2,422,000 / 2,303,000.

66. **The snapshot build id is a layout descriptor** (owner, 2026-11-09;
    freeze item 1 of the architecture review; supersedes decision 55's
    source hashes). The id hashes what an image's meaning rests on instead
    of each crate's version and sources: `SNAPSHOT_FORMAT`, a layout epoch
    (`SNAPSHOT_EPOCH`), the code of the four snapshot codecs (the facade's
    header, now `src/image.rs`; mm-engine's and mm-core's `snapshot.rs`;
    mm-compile's `encode.rs`), every `const` and `#[repr]` item of mm-core,
    mm-compile and mm-engine, the opcode, native, type and native-step
    tables, the prelude's and bundled modules' bytecode (hashed at build
    time, as package E needs: a `host-bundle` guest does not link them),
    the features, value and hash encoding samples, and a recorded hash of
    what `Session::define` makes of a probe definition (docs/spec.md §9.3,
    "Compatibility"). An upgrade that
    changes no layout keeps the id, so its parked images restore on the
    new build without a drain.
    - **The epoch rule** (AGENTS.md "Rules", spec §9.3): bump
      `SNAPSHOT_EPOCH` in the same commit when a function body changes
      what an image holds or how it is read while no hashed item changes;
      explicitly, what a raw word or value slot of an existing NSTATE op
      holds with the same counts (`STEPS` hashes names and counts only),
      and what an existing `Cont` field or stage means. Code that writes or
      reads an image belongs in the four codec files, which are hashed
      whole.
    - **Every `const`, not a list of layout modules.** A hand-kept list at
      8cd850c missed the regex matcher's `op` and `bt` modules (a parked
      match saves its backtrack run in the arena) and mm-compile's operand
      meanings (`BinOp`, the match kinds). Each engine crate's build script
      now hashes every `const` and `#[repr]` item under `src/` (test
      modules and `const fn` aside), so a new layout module needs no
      registration; a tuning constant changes the id too, the safe side. Of
      the last 168 commits that touched the engine crates, 85 changed
      neither a bytecode source (prelude, stdlib Python, the compiler) nor
      a `const` item, and would have kept the id; 21 changed a `const` but
      no bytecode source, most of them real layout changes [M].
    - **Host code is checked, not trusted.** A keyed host module is
      generated and compiled again on restore, and the compiler is no
      longer in the id (only its output on the probe, the prelude and the
      bundled modules). `HostProgram` gains `code`, a hash of the compiled
      program, which images store beside the key; a rebuilt program must
      match both (8 bytes per keyed host module). The probe's spec now
      round-trips through the spec codec, as a restore does, and its hash
      is recorded (`host_defs::CODEGEN_PROBE`) and checked by a unit test
      that prints the new value when the codec, the generator or the
      compiler change it. Computed at run time it made the first
      `build_id()` call 1.2 ms; with the prelude, the stdlib bytecode and
      its packing hashed at build time too, the first call takes about
      60 µs, against 480 µs on main [M] (it runs once per process or wasm instance, on
      the first snapshot or restore).
    - **Evidence [M].** `crates/micro-monty/tests/golden.rs` restores two
      images blessed at this commit (a sync cell parked inside a keyed sort
      over most builtin types, user classes, a suspended generator and live
      iterators; an async cell waiting under `gather`): under the same id
      they continue to the recorded transcripts, under another id (a
      flipped id byte) they are refused as `Incompatible`. An 18-edit
      experiment, each edit reverted after: function-body edits in a
      native, the VM, the regex matcher, the compiler (same bytecode) and
      the facade, comment-only edits in two codecs, a prelude edit that
      compiled to the same bytecode and an opcode doc edit kept the id,
      and the golden images continued; swapping two class slots,
      renumbering a backtrack-frame tag, bumping the epoch, a code edit in
      each of the engine, header and code codecs, a tuning constant, a
      prelude edit that changed bytecode and a host-wrapper generator edit
      changed it, and the golden images were refused as `Incompatible`.
    - mm-parse lost its build script (nothing it does reaches an image);
      `SNAPSHOT_FORMAT` is 1: the first release starts every counter at
      1, with `ABI_VERSION` 1 and `SNAPSHOT_EPOCH` 1 (owner, 2026-11-09),
      so none shows a pre-release history.
    - Every change that moves the id blesses the golden images again in
      the same commit (`MM_BLESS=1 cargo test --release --locked -p
      micro-monty --test golden`; integrator, 2026-11-09), so each change
      that keeps it is checked against an earlier build's images. Blessing
      writes nothing and fails if a continuation differs from its
      committed transcript. `cargo xtask check` runs the test as its own
      gate, in the facade's own build (release, sharing the step oracle's
      build), and fails while the images carry another id; debug builds
      skip it, because under `cargo test --workspace` mm-cli's opt-in
      modules unify into the facade and give it another id [M].

67. **Extensible frozen surfaces, facade half** (owner, 2026-11-09;
    freeze item 2 of the architecture review). `Limits` and `EngineConfig`
    are `#[non_exhaustive]` with `const fn new()` and `const` `with_*`
    builders (`Limits::new().with_memory(..)` works in a `const`, as
    mm-wasm-host's tests need), so a limit or setting can be added after
    0.0.1 without breaking embedders; the workspace's struct literals
    (tests, examples, fuzz targets, bench evidence, mm-cli, mm-cell-worker,
    mm-wasm-abi, the guest, mm-wasm-host) moved to the builders.
    `SessionSettings` (`#[non_exhaustive]`, pub fields, `Default`) holds
    what an image does not carry: the output callback, wall clock, entropy,
    zone source, text cut, `int_bits_text` and GC stress. `Session::new_with`,
    `Engine::restore_with` and `SessionTemplate::instantiate_with` take it;
    `new`, `restore` and `instantiate` keep the defaults. Before, a restore
    reset the clock and entropy to the platform's, the text cut to `Head`,
    `int_bits_text` to `false` and dropped the output callback and zone
    source, and every host had to know which setters to call again.
    - **Moved, not shared.** The callbacks are per-session `FnMut`s (the
      guest's capture the session id), so a host builds the settings in
      one function and passes a fresh value to each call, rather than one
      `Clone` value with `Arc`'d `Fn + Sync` callbacks.
    - **The incremental GC test hooks too** (coordinator, 2026-11-09):
      `gc_inc_stress` and `gc_incremental` (doc-hidden, as their setters
      are) joined `gc_stress`, and `mm` builds one settings value from
      `MM_GC_STRESS` for `new_with` and `restore_with`, so a resumed
      process runs under the same stress as the one that saved it.
    - **Evidence [M].** `tests/snapshots.rs`
      `restore_with_and_instantiate_with_apply_the_hosts_settings`: a cell
      stopped at a host call is saved, finished, restored with the same
      settings and finished again, then captured as a template and
      instantiated; each run shows the same output through the callback,
      the pinned clock and entropy, the zone source's answer, both ends of
      a `HeadTail` cut and `<int with 16610 bits>`. With `restore_with`
      made to ignore its settings, it fails (CPython's `ValueError` for the
      int and a `Head` cut).
    - The guest builds its sessions' settings in one function for
      `mm_session_new` and `mm_session_restore` (no ABI change). The
      mm-wasm options record is freeze item 2's other half, landed on its
      own.

68. **The mm-wasm options record is tagged; the interface is frozen with
    0.0.1** (owner, 2026-11-09; freeze item 2's ABI half, option (B)).
    `mm_session_new` and `mm_session_restore` take a record instead of the
    positional options: a `u32` byte length, then entries in any order,
    each a tag byte, a `u32` payload length and the payload
    (`mm_wasm_abi::option`, wasm-embedding.md "Session options"). An absent
    entry is the option's default, and the encoder writes only the entries
    that differ, so the defaults are an empty record. A guest refuses a
    repeated tag, a payload that is not exactly its tag's encoding, and an
    unknown tag below `0x80`; it skips unknown tags from `0x80` (none is
    assigned). The record also carries the host-side settings an image
    does not carry (text cut, `int_bits_text`, GC stress, besides the
    clock, entropy and zone switches), and a restore applies them, as
    `SessionSettings` does natively (decision 67). `ABI_VERSION` is 1:
    the first release starts every counter at 1 (owner, 2026-11-09; with
    `SNAPSHOT_FORMAT` 1, decision 66).
    - **The rule from 0.0.1.** A release may add a session option (a new
      tag whose default keeps the behaviour without it) or an optional
      export without a bump; anything else bumps `ABI_VERSION`, with
      `crates/mm-wasm-host` and `hosts/js` in the same change.
    - **The ignorable range stays** (owner, 2026-11-09): it is a forward
      mechanism, not legacy glue. Only an option a guest can safely run
      without (a hint, a diagnostic) may take a tag from `0x80`; anything
      that limits or changes behaviour (a limit, the clock, entropy,
      output, the text cut) takes a strict tag, so an older guest refuses
      the record rather than silently run without it.
    - **Owner rule for 0.0.1, which this follows.** micro-monty is an
      internal prototype with nothing released, so changes go straight to
      the end state: no compatibility glue, shims, old-format readers,
      deprecated aliases or version history for formats and APIs that
      never shipped. So there is no reader for the positional record and no
      probe export, and the docs describe the frozen surface, not v1 and
      v2. Layout-keyed snapshot compatibility (decision 66) is the forward
      mechanism for 0.0.x and reads no older format.
    - **Removed with it:** the positional `put_options`/`get_options`;
      the probe for `mm_pending`, now a required export (`Option` in
      mm-wasm-host's `Funcs`, `mm_pending?` in hosts/js, and both hosts'
      "guest without mm_pending" errors); the "set it again after a
      restore" advice on the text cut, `int_bits_text` and GC stress
      setters; the v1/v2 history in the abi crate's docs, AGENTS.md,
      wasm-embedding.md, sdk.md, spec §6's crate and budget tables, the
      tests' names and the package descriptions. `text_cut_code` writes
      `RESERVED` for a cut it does not know, as every other encoder does,
      instead of `Head`.

69. **Accessors out of line and `optimize(size)` on shipped builds**
    (owner, 2026-11-09; revisits the 2026-11-03 decision and decision 46).
    The heap accessors are out of line outside `Vm::dispatch`'s inline
    closure. `#[cfg_attr(mm_shipped, optimize(size))]` covers part of the
    cold set on shipped builds only, through the `RUSTC_BOOTSTRAP=1` those
    builds already use. Plain-stable builds are unchanged. `cargo xtask
    check` runs the own cases on a natively built shipped `mm`. Details,
    numbers and the 0.0.2 follow-ups (trusting object tags, the
    dispatch-closure spike) are in "Engine size review" above.

70. **The bundle guest ships beside the default guest** (owner,
    2026-11-09; size review lever E). It keeps the stdlib and prelude
    bytecode in custom sections that the host serves through the new
    import `mm_host.bundle_read`; the default guest is unchanged and stays
    what a host without that import runs. Both guests have one build id.
    Its own size ceiling (S1). "The bundle guest" above.

**Open from the pre-0.1 review** (2026-11-07; fixed in the 0.1 API pass:
H1–H5, M1, M2, M4–M7, the M10–M12 items above, L2, L5's `image_build_id`
and `phase`; then M9, L6 and the docs.rs item, decisions 56–58):

- **M3 `Session::pending()`**: closed 2026-11-08 ("Pre-0.1 follow-ups"
  item 3; the facade keeps the payload in its snapshot header, format 6).
- **M8 per-Engine cache of host-module code**: closed by decision 60.
  Its follow-ups:
  - **mm-cell-worker's child binary stores host code**: closed
    2026-11-08 ("Pre-0.1 follow-ups" item 4).
  - **Typed host functions built on first use** (after 0.0.1). This would
    drop the ~2.1 KB per typed tool that images still carry. Additive, no
    API change.
- **L1 `_mm` importable by guests**, **L3 the facade phase byte not
  cross-checked on restore**, **L4 `test-hooks` / `ENGINE` / seccomp env
  switch naming**, **L8 doc-only edits change the build id** (closed by
  decision 66: comments and function bodies no longer do), **L9 host-built value names unvalidated**,
  **L10 `set_deadline` on `wasm32-unknown-unknown`**: hygiene, none blocks
  publishing.

**mm-wasm idle RSS regression (Mac soak, 2026-11-06) [M].** Per-instance
idle RSS 320 → 790 KiB and the runtime baseline 8 → 17 MiB came from the
host, not the guest: 9917a48 loaded the content-keyed `.cwasm` with
`Module::deserialize(bytes)`, which on macOS leaves wasmtime no
copy-on-write source for linear memory (its memfd fallback is Linux-only),
so each instance copied the 615 KB data segment. The host loads the cache
with `deserialize_file` again (key still in the file name; old key-headed
files are recompiled). Orb, 40 idle sessions on the 874409b guest:
copy-on-write off (macOS's former path) 694–714 KiB per instance; fixed host
450 KiB, runtime baseline 12.5 → 6.9 MiB. The guest's own growth from
5568b27 is about +38 KiB per instance (data section 583 → 615 KB, the stdlib
round). Native sessions unchanged at 75–77 KiB. No engine, guest or size
change. Confirmed on the Mac (329–330 KiB per instance). The runtime
baseline's remaining +3 MiB there: the host read the whole module on every
load for the cache key, which macOS's allocator keeps resident (+2.4 MiB,
reproduced on Linux with glibc set to retain); now hashed in chunks, read
whole only to compile. The other ~1.4 MiB is `.cwasm` code paged in at
`Runtime::new` by U1's build-id instance instead of at the first session,
not extra steady-state memory.

Draft upstream issue for alexcrichton/dlmalloc-rs (not filed; also in the
[artifact](https://ampcode.com/artifacts/dlmalloc-validate-size-routes-b-and-c-measured-03a99018-c089-458c-a58b-e29062dd1351)):

> **`validate_size` assertions in `free`/`realloc` cost ~4% of code size
> on wasm32-unknown-unknown**
>
> `Dlmalloc::free` and `Dlmalloc::realloc` (src/lib.rs) call
> `Dlmalloc::validate_size` (src/dlmalloc.rs) before every deallocation:
> two `assert!`s on the chunk header (`psize >= size + min_overhead` and,
> for non-mmapped chunks, `psize <= size + max_overhead`). Both methods
> are `#[inline]` and std's `sys::alloc::wasm` calls them from
> `GlobalAlloc::dealloc`/`realloc`, so with fat LTO the check and its
> panics are inlined into every deallocation site of the program.
>
> On a ~2.2 MB `wasm32-unknown-unknown` module (std rebuilt with
> `-Cpanic=immediate-abort`, fat LTO, `codegen-units = 1`, then
> `wasm-opt -Oz`), changing the two `assert!`s to `debug_assert!` shrinks
> it by 98,448 bytes (4.2%); a second binary by 83,957 bytes (3.9%). The
> checks' constant appears about 1,700 times in the module, against about
> 50 on `wasm32-wasip1`, where std uses wasi-libc's malloc.
>
> The checks only catch a caller passing the wrong `Layout`, which
> `GlobalAlloc::dealloc` already makes undefined behaviour; C dlmalloc
> does not validate it either. Proposal: make `validate_size` a
> `debug_assert!` (or gate it on the existing `debug` feature, like
> `check_malloc_state`). If keeping it in release matters, an out-of-line
> `#[cold]`/`#[inline(never)]` `validate_size` keeps the check and removes
> most of the size.
>
> Repro: any std program for `wasm32-unknown-unknown` with many
> `Vec`/`String`/`Box` drops; compare `wasm-opt -Oz` output before and
> after the two-line change.

### 2026-11-05 gap batch [M]

- Fixed two spec-rule violations: `isinstance`/`issubclass` against deeply
  nested class tuples overflowed the Rust stack (now an iterative, fuel-charged
  walk that hands off to the prelude); `Foo.__repr__ = Foo.__str__` hung
  (builtin-to-builtin request chains now count against the recursion limit).
- Added the warning classes and `warnings`, `codecs`, `platform`,
  `sys.path` (empty) and `dir()`. CPython-test methods (conformance/breadth):
  test_re 11 → 115, datetimetester 176 → 411, test_str 0 → 93, test_builtin
  6 → 45, test_enum 633 → 738.
- Size: hello.wasm 1,833,115 → 1,849,047 B, native 1,925,464 → 1,942,296 B
  (warnings ≈ 4.7 KB bytecode, codecs + platform ≈ 4.8 KB, dir ≈ 5.5 KB).
  Bench Ir vs main within +0.5% on all 13 programs (str +0.49%); startup
  +4.2% (≈35k instructions, the larger type table).

### 2026-11-05 breadth-campaign fixes [M]

From `conformance/breadth/difffuzz/README.md` and the CPython-test run:

- `repr` escapes exactly what `str.isprintable` rejects. Both use one
  generated table (`crates/mm-engine/src/ucd/printable.bin`, 2,982 B, from
  CPython 3.14.7's `isprintable` by `tools/gen_ucd.py`) in every build; a
  unit test checks it against the category table on every code point.
  The hand-written approximation left unassigned code points raw (604 of
  630 difffuzz divergences; `int(chr(6892))` error text).
- `Enum('E', type=int)` is an empty enum; `Enum('E')` raises CPython's
  TypeError. test_enum 738 → 856 methods.
- `print` writes the arguments before one whose native `str()` raises.
- `int / int` rounds once into the subnormal range (`long_true_divide`).
- sort stays merge sort; the comparison-order difference is in
  `docs/sdk.md` Limitations. Matching CPython's mixed-type error pair would
  need timsort's run detection and binary insertion, so it is left.
- difffuzz, 15 min, `--jobs 4`, seeds from 0, both binaries side by side:
  `dbd0bf8` 222 divergent of 15,124 programs (214 signatures; 194 of them
  repr escaping), this change 6 of 17,294 (5 signatures), all the
  documented `pow` preflight `MemoryError` (class (b)).
- Size: hello.wasm 1,849,047 → 1,851,950 B (+2,903), native 1,942,296 →
  1,945,304 B (+3,008). Bench Ir vs `dbd0bf8` within ±0.04% on 12 programs;
  startup −0.52% (the table lookup is cheaper than the category search).

### 2026-11-05 `bytearray` and `struct.pack_into` [M]

Design: docs/spec.md §7.4. One new arena type (`ty::BYTEARRAY`: a capacity
block `ByteRef` and a length), one new mm-core setter
(`Heap::copy_bytes`: overlapping, cross-segment, write barrier on the
destination), and the bytes methods shared through a method-table
fallback (bytearray's own run holds only the mutators). `struct.pack_into`
is Python over slice assignment. binascii's and base64's Python argument
checks accept a bytearray (as bytes), as CPython's do.

- Checked against CPython 3.14.7: `conformance/own/340_bytearray.py`
  (constructors, mutation, operators, comparisons, hashing, shared
  methods, repr, struct) and `341_bytearray_mutation.py` (sustained
  mutation, `__index__` sources). `cargo xtask durability --filter 34[01]`
  at `--every 1:5000`: 782 and 1,006 parks, identical.
- CPython-test methods (conformance/breadth harness, same CPython
  denominators): test_bytes 94 → 143 (/194), test_struct 23 → 26 (/36),
  test_binascii 16 → 34 (/76), test_base64 19 → 19 (/35; its remaining
  failures all need `memoryview`). The rest of test_bytes' ByteArray*
  failures are subclasses, `__slots__`, `memoryview`, `pickle`, `eval` and
  `expandtabs`.
- Size (rebased on `5a139bf`): hello.wasm 1,851,950 → 1,869,142 B
  (+17,192), native 1,945,304 → 1,964,488 B (+19,184); budgets 1,871,500
  and 1,966,500.
- Bench (callgrind Ir of `mm run bench/<p>.py`, release, `5a139bf` →
  this change; outputs identical): fib +0.00%, loop +0.00%, dict −0.07%,
  json_rows −0.01%, closure +0.00%, classes −0.02%, generators +0.04%,
  exceptions +0.07%, fmt +0.18%, sort −0.00%, interp +0.38%; outside the
  ±0.5% band: str −0.55% (faster; it uses no bytes, likely inlining around
  the `quoted_repr` split [E]) and startup +0.61% (872,686 → 878,011, +5.3k
  instructions in `Session::new`, type names and interning: two more
  builtin types and 13 more natives installed per session [M]).
- Left out: `memoryview`, `bytearray` subclasses, `bytearray % args`,
  bytearray host values; `hashlib`, `re` and `json` still take only
  `bytes`.

### 2026-11-05 gap batch 2 [M]

- Coroutine objects have `send`, `throw` and `close`. `send(None)` runs the
  coroutine's frame with a `CoroSend` continuation (snapshot tag 30) that
  turns its return into `StopIteration`; a coroutine runs to its end inside
  `send()` (no `__await__` iterator protocol, so an unsettled `await` in it
  suspends the calling task instead of yielding to the caller; documented in
  docs/sdk.md). A coroutine that raises `StopIteration` now raises
  `RuntimeError` (PEP 479), as generators did.
- `str.translate` (native for builtin dict tables, prelude twin
  otherwise), `str.maketrans` and `str.format_map` (prelude), `float.hex`
  and `float.fromhex` (CPython's round-half-even algorithm; 6,000 random
  strings bit-identical to CPython), `bytes.translate`, `bytes.maketrans`
  and `bytes.expandtabs` (bytearray's return bytearrays). Builtin class
  methods (`bytes.fromhex`, `float.fromhex`, `int.from_bytes`,
  `dict.fromkeys`, the `maketrans`es) now bind to the class when a user
  subclass inherits them (they used to get no receiver and raise
  `TypeError`); `float.fromhex`/`bytes.fromhex` on a subclass return
  `cls(result)`, while `int.from_bytes` and `dict.fromkeys` still return
  the base type.
- `tempfile` (`mod-tempfile`, default): importable; everything that would
  touch the filesystem raises `PermissionError` ("this sandbox has no
  ambient filesystem"), as `open()` does. Exclusions unchanged.
- `re` emits CPython's positional `count`/`maxsplit` DeprecationWarnings and
  "Possible set ..." / "Possible nested set" FutureWarnings; `datetime`
  emits the `utcnow()`/`utcfromtimestamp()` DeprecationWarnings and
  `strptime`'s day-without-year DeprecationWarning.
- CPython-test methods passing on mm (conformance/breadth harness, all 125
  units, 6ca26d8 build vs this one): 5,605 → 5,750 of 8,989 (+145).
  test_coroutines 1 → 47, test_contextlib_async 8 → 48, datetimetester
  412 → 437, test_math 60 → 67, test_float 27 → 33, test_bytes 143 → 148,
  test_with 46 → 49, test_asyncgen 0 → 2, +2 each in test_grammar,
  test_str, test_builtin and test_enum (`test_copy` no longer crashes), and
  +1 each in test_re, test_functools and test_positional_only_arg. Not
  recovered: the five test_re warning tests now see the warning but assert
  `w.filename == __file__` (warnings had no caller location then; gap
  batch 3 adds it); `Oddballs.test_extra_attributes` then needs `datetime`
  objects to refuse new attributes (`__slots__`); the remaining coroutine
  failures need `__await__`/`types.coroutine`; test_string_literals stays
  at 0 (its `setUp` calls `tempfile.mkdtemp()`, and the tests use `eval`).
- Size, on top of `bytearray` (6ca26d8): hello.wasm 1,869,142 →
  1,884,845 B (+15,703), native 1,964,488 → 1,980,840 B (+16,352). Per
  part (wasm/native, measured on dbd0bf8): float hex/fromhex 4,541/4,400;
  str translate/maketrans/format_map and subclass binding 3,927/3,952;
  bytes translate/maketrans/expandtabs 3,883/4,080; coroutine
  methods 1,958/1,664; re/datetime warnings 1,024/1,024; tempfile 828/912.
  Budgets 1,887,000 / 1,983,000.
- Bench callgrind Ir vs the 6ca26d8 build: all 13 programs within ±0.06%
  (startup +0.060%, generators −0.046%, json_rows +0.025%). Against
  5a139bf, startup once showed +0.53%, but the engine's own instructions
  moved +0.04%: the rest was glibc's `pthread_getattr_np` scanning two more
  `/proc/self/maps` lines (`sscanf` 44 → 46 calls), a memory-layout effect
  that also explains 5a139bf's recorded startup −0.52%.

### 2026-11-05 libFuzzer F1 and F3 (`conformance/breadth/fuzz/`) [M]

- **F1 cause** (callgrind of `b < a` on two `c = [x]; c.append(c)` lists,
  `x` 16 nested frozensets): each native walk spent its 2^14 visits before
  the depth limit, so it handed off to `_op_cmp`/`_op_eq`, whose items
  compared with `==`/`<`, which started the next native walk at depth 0
  with a full budget. Prelude frames count only against 10 × the
  recursion limit, so one comparison ran up to 10,000 prelude levels at
  two full walks each: fuel 16.6M at `--recursion-limit 50`, 33.3M at
  100, 133M at 400 (linear), about 333M at 1000. With fewer frozenset
  levels one walk reached depth 1000 first, hence the cliff between 14
  and 16 levels.
- **F1 fix:** the twins pass their nesting `d` and compare items with
  `_walk_eq`/`_walk_ord` (natives at depth `d`) and `_seq_scan`/
  `_dict_scan`/`_set_probe` take `d`, so the native depth limit holds for
  the whole comparison. A walk at nesting `d` gets `max(2^14 >> d, 1)`
  visits: the re-walk cost is about 2 × 2^14 plus one visit per level.
  Floors of 64–1024 cost the floor at every level of a chain longer than
  it (the 2^8-path DAG over 672 frozensets below took 0.34–12 s). `_ord`
  finds the first unequal pair of an ordering in one descent; a pair that
  needs Python code keeps CPython's `==`-then-`<` steps. `too_deep` no
  longer counts user frames for a prelude callee: it walked every frame on
  each call once a prelude descent passed the recursion limit (25% of the
  fuzz input's instructions).
- **F1 before → after** (release `mm`, `6fb33a9` → this change, unlimited
  fuel, wall time on a loaded machine; CPython 3.14.7 in brackets): the F1
  input 145.9 s, 665M fuel → 0.15 s, 0.99M fuel [4.0 s]; `b < a` with 16
  frozenset levels 62.7 s, 333M → 0.07 s, 0.41M [0.06 s]; nested_data
  `Jl0r` (`==` on a 2^8-path DAG over 672 frozensets, an F2 shape) 2.28 s,
  11.5M → 0.50 s, 4.2M [0.64 s]; 30 nested lists of 3,000 strings
  (`==`, `<`, 5×) 0.39 s, 8.8M → 0.16 s, 2.3M [0.05 s].
- **F3:** the `parked_bytes` consistency check moved from
  `execution_bytes` (every task switch) to `check_parked_bytes`, run when
  the task table doubles, when tasks are dropped and at each snapshot. The
  artifact: 28 s → 0.54 s in the fuzz binary.
- Tests: `comparing_cyclic_lists_of_deep_frozensets_raises_promptly`
  (session.rs: four comparisons within 20M fuel; the old code parks on
  fuel) and `conformance/own/331_deep_comparison_handoff.py` (cycles,
  150-deep lists and frozensets that hand off, user `__eq__`, mixed types).
  A cyclic dict or a 600-deep frozenset chain overflows the 2 MiB debug
  test threads in the native walk on old and new code alike (debug frames
  are several times larger; release fits 1000 levels in 2 MiB), so those
  shapes stay in the session.rs tests, which run on a 16 MiB thread.
- Fuzz reruns, 20 min each from empty corpora, the campaign's command:
  nested_data 1,207 s, 5,852 runs (4/s), final cov 9226 ft 26884 corp
  750/2476b, peak RSS 595 MB; vm_bytecode 1,201 s, 508,924 runs (423/s),
  cov 15146 ft 39105 corp 2078/21Kb, peak RSS 631 MB. No crash, panic,
  sanitizer report, OOM or timeout. Slow units (over 10 s in the fuzz
  binary): nested_data `9kEH` (`max` on a cyclic list of 489 nested
  dicts, F1's shape: release 32.5 s, 333M fuel → 0.50 s, 5.8M [CPython
  0.88 s]) and `PjUjLw==` (`x < y`, 2^8-path DAG over 367 frozensets
  inside 306 cycles: release over 300 s → 20.6 s, 151M [CPython 266 s]);
  both spend the harness's 5M fuel, which the instrumented build runs at
  about 200k fuel/s. vm_bytecode `igAAAAAASjBz…` (F4, fixed since in b442a99, also on
  `6fb33a9`): a mutated program stores 12,000 str keys into a dict whose
  object has no index slot (one value slot before the counts word), so
  each resize's index write is dropped, the table stays linear and every
  store scans all entries without charging fuel (0.46 s for 30k fuel in a
  plain release build, quadratic in stores). The F1 and F3 artifacts now
  take 8.1 s and 0.61 s in the fuzz binary; F2 0.93 s in release (was
  1.02 s), but 5.5M fuel (was 4.4M), so the harness parks it at 5M.
- Size (rebased on `6eba295`): hello.wasm 1,884,845 → 1,887,752 B
  (+2,907), native 1,980,840 → 1,983,480 B (+2,640); budgets 1,890,000
  and 1,985,500.
- Bench (callgrind Ir of `mm run bench/<p>.py`, release, `6eba295` → this
  change; outputs identical): fib +0.03%, loop +0.01%, dict +0.14%,
  json_rows +0.06%, closure +0.01%, classes +0.00%, generators +0.00%,
  exceptions +0.10%, fmt +0.05%, sort +0.01%, interp +0.06%, str +0.17%.
  Outside the ±0.5% band: startup +10.1% (878,853 → 967,980). About 68k
  of it is one more doubling of the arena's bump segment in `Session::new`
  (memcpy and memset of the segment), which the larger prelude crosses; a
  one-line prelude function adds 1.3k. An earlier build of this change
  measured `str` −0.66% against `dbd0bf8`, all in byte-space sweeps: its
  GC schedule shifts with the startup heap.

### 2026-11-05 gap batch 3 [M]

Owner-approved items, all on `main`:

- **Warnings carry the caller's location.** The native
  `_mm.caller(stacklevel, skip_file_prefixes)` walks the VM's frame
  chain (no frame objects), skips prelude frames, and returns
  `(filename, line, globals)`. `warnings.warn` uses it for `filename`,
  `lineno`, `stacklevel` and the caller's `__warningregistry__`. When
  `stacklevel` runs past the stack, the location is `<sys>:0`, as in
  CPython. `re` and `datetime` pass `skip_file_prefixes`. Own case
  `361_warning_location.py`.
- **`enum._iter_bits_lsb`** raises `ValueError` on negative input; before,
  it never ended and filled the heap. `Vm::unwind` now requests a GC
  when the exception in flight is the preallocated `MemoryError`. Then
  the largest `[0]*n` allocation that fits after a heap-exhausting loop
  went from 32,768 to 33,007,296 elements (fresh session 33,539,776).
  Requesting the GC in `hx` instead cost +50 KB, because it is inlined
  everywhere. Own case `362_enum_iter_bits.py`.
- **`__await__` and `types.coroutine`** (spec §8.5). AWAIT on an object
  whose `__await__` returns an iterator delegates to it through the
  prelude's `_await_step`/`_await_throw` (`Cont::AwaitStep`, snapshot tag
  31), so there is no nested dispatch loop. A value the iterator yields
  goes to the enclosing `coro.send()` caller, and the coroutine frames are
  saved in the coroutine (new state SUSPENDED). Under the event loop, a
  bare `None` yield re-queues the task and anything else raises
  `RuntimeError('Task got bad yield: …')`. `coro.__await__()` returns a
  `coroutine_wrapper`. `types.coroutine` marks a generator function as an
  iterable coroutine. Own case `363_await_protocol.py`. Durability
  `--every 1:5000` on it: 698 parks, same report. Session test: a
  coroutine suspended by `send` resumes in a later cell and after restore.
- **Docs.** sdk.md Limitations: `__slots__` is out of scope (0 of 594
  codemode solutions, and 0 of 1,782 with the GPT census, use it);
  `memoryview` is deferred; the `send()`/`__await__` paragraph is
  rewritten. An unsettled asyncio awaitable under `send()` still suspends
  the calling task. There is no `cr_frame`/`cr_await` and no "never
  awaited" warning.
- **`mm-cell-worker` scope** (owner decision): it is a test-only
  integration harness for plak's cell protocol and gets no new policy
  features. The 16 outstanding nested calls, truncation and queueing
  belong to the consumer (plak/plakous). The engine and facade impose no
  outstanding-call limit. The worker is expected to move into plak when
  plak adopts micro-monty. Recorded in AGENTS.md and spec §10.2.

Measurements:

- Harness (`conformance/breadth/cpython-tests`, all units, 32125ff build
  vs this one, same denominators): 5,746 → 5,781 of 8,987 (+35).
  test_coroutines 47 → 70/92, test_re 116 → 121/149 (the five warning
  tests), test_types 58 → 62/123, test_enum 856 → 858/1068, test_sort
  16 → 17/21 (`test_bug453523` uses `random`; probably flaky).
  test_contextlib_async stays at 48/57: what is left needs `__slots__`,
  `os.chdir`, bound-method `__self__` and docstrings. One run lost the
  test_posixpath denominator (35 → 13): CPython hit a leftover
  `/tmp/mm-cpython-tests/work/test_posixpath/@test_mm_tmp`. Removing it
  and rerunning that unit restored 35. Clear the work directory if a
  CPython count drops.
- Size (40cd934 → this): hello.wasm 1,887,752 → 1,904,638 B (+16,886),
  native 1,983,480 → 2,000,888 B (+17,408). Per part, wasm/native:
  warnings +2,845/+2,672, enum and GC +125/+112, await +13,916/+14,624.
  The budgets are now 1,906,500 and 2,002,500.
- Bench (callgrind Ir, 40cd934 → this; outputs identical): fib −0.000%,
  loop −0.000%, dict +0.000%, json_rows −0.009%, closure −0.000%,
  classes +0.127%, generators −0.421%, exceptions +0.129%, fmt +0.014%,
  sort +0.026%, interp +0.046%, str +0.002%, startup −0.009%. All are
  within ±0.5%. `Cont` checks in `enter_frame`/`cont_put` use
  `matches!(…, Cont::None)`, because the derived `==` compared payloads.

Open: `cr_frame`/`cr_await`/`cr_code`, the unawaited-coroutine
RuntimeWarning, coroutine pickling/copy, `compile`/`exec` in
test_coroutines; an asyncio awaitable under `send()` suspends the
task (documented); `memoryview` deferred; `__slots__` out of scope; F4
still open. (Since then the codemode host API added the never-awaited
warning, 89e945b, and b442a99 fixed F4.)

### 2026-11-05 codemode stdlib round [M]

Decisions 7–12 above, all on `main`:

- **Promoted to default:** `decimal`/`numbers`/`contextvars`, `statistics`/
  `fractions`, `tomllib` (`crates/mm-stdlib/Cargo.toml`).
- **`json.loads` hooks** (`_json_scan(s, True)` in `json.rs`, `_json_hooks`
  in the prelude: an explicit stack, children first in document order).
  `json.loads(raw, parse_float=Decimal)` works. Own case 364; durability
  884 parks, same report.
- **New modules:** `html.parser` with `_markupbase` (entity names from
  `html`'s table), `calendar` (no `locale`, no CLI; `enum.global_enum` now
  sets CPython's reprs, and `calendar` binds the members itself since there
  is no `sys.modules`), `hmac` (pure-Python path, no `__slots__`), `zlib`
  checksums (native `_adler32` next to `_crc32`), `utf-8-sig` (a native
  codec variant: BOM on encode, skipped once on decode, error positions
  after it). Own cases 365–368.
- **asyncio:** `create_task`, `Task`/`Future` methods, `CancelledError` and
  `InvalidStateError` (native types, module `asyncio.exceptions`),
  `Semaphore`/`BoundedSemaphore`, `wait_for`, `as_completed`, `Future()`.
  Cancelling a suspended task stores the `CancelledError` in its
  `awaiting` slot and the scheduler raises it at the pending `AWAIT`
  (`Next::Raise`). Deadlines are sleeps flagged to wait for pending host
  calls first. The helpers are `_mm.aio_*` natives, not `_prelude` ones:
  each eagerly bound prelude native costs startup (8 of them were +1.7%
  startup Ir). Own case 369; durability 661 parks; a session test runs
  `Semaphore(2)` + `wait_for` + `as_completed` over host futures through a
  snapshot at every batch (batches `[0, 1]`, `[2, 3]`, `[4]`).
- **zoneinfo:** `Session::set_zone_source`, `_mm.tzif`; the ZoneInfo cache
  is a dict (no weakref), so loaded zones are snapshot state.
  `io.BytesIO.seek` gained relative seeks (the reader needs them). Own case
  372 (DST gap and fold, footer rule in 2099, LMT, key validation);
  durability 895 parks and `--cross-target` 96 parks, same report; a
  snapshot test restores without a source and still answers Paris.
- **`globals()`:** prelude builtin over `_mm.caller`. Own case 373;
  session test of reads across cells and six kinds of write.

Size (ac10b28 → this, each part measured in order, wasm / native):
`decimal` +42,679 / +42,928, `statistics`+`fractions` +22,767 / +22,912,
`tomllib` +10,575 / +10,960, json hooks +1,291 / +1,104, `html.parser`
+9,014 / +9,136, `calendar` +8,670 / +8,736, `hmac` +2,213 / +2,288,
`zlib` +408 / +400 plus +1,556 / +1,728 for `_adler32` and the
`global_enum` reprs, `utf-8-sig` +457 / +592, asyncio +15,570 / +16,000,
`zoneinfo` +11,413 / +11,488, `globals()` +1,261 / +1,312. Total
`hello.wasm` 1,922,566 → 2,050,440 (+127,874; checkpoint estimate about
2.04–2.06 MB [E]), native 2,012,712 → 2,142,296 (+129,584), measured on
ac10b28. Rebased onto the codemode host API: 2,081,288 and 2,180,120
(+128,512 and +130,336 over it) [M]; budgets 2,082,700 and 2,181,500.

Bench (callgrind Ir of `mm run bench/<p>.py`, ac10b28 → this, binaries at
paths of equal length since startup Ir depends on the path; outputs
identical): classes −0.000%, closure +0.000%, dict +0.000%, exceptions
+0.000%, fib −0.001%, fmt −0.295%, generators +0.000%, interp +0.010%,
json_rows −0.027%, loop −0.000%, sort −0.011%, startup −0.466% (874,860 →
870,782), str −0.229%. All within ±0.5%.
Against 832b0b1 (after the rebase onto the host API): classes +0.000%,
closure +0.000%, dict +0.000%, exceptions +0.000%, fib −0.000%, fmt
−0.294%, generators +0.000%, interp +0.001%, json_rows −0.071%, loop
−0.000%, sort −0.011%, startup −0.201% (873,053 → 871,299), str −0.228%
[M].

Codemode census (`conformance/codemode`, default build, of CPython
passes): Claude 84.4% → 99.1% (572/577), Luna 87.6% → 98.2% (546/556), Sol
77.4% → 98.5% (580/589); checkpoint projection about 92% (98% with G) /
97% / 97% [E]. Default and full builds now agree. No earlier pass
regressed; every pass makes CPython's calls batch by batch. Left: `xml`
18, `email` 3, `__import__(...)` 3.

Open: `xml.etree` and `email.utils` (deferred, redirect messages are the
host-API thread's); `__import__` (3 census failures); asyncio `wait`,
`Queue`, `Lock`, `Event`, `TaskGroup`, `timeout`; `zoneinfo.
available_timezones()`; zlib deflate (out by decision).

## Continuation steps

The first large-operation latency/lifecycle pass is recorded above. Use the
measured single-call stalls and remaining unaccounted workspace to prioritize
further work; do not claim a memory or latency bound from those measurements.
The owner clarified that the size ceiling was an arbitrary smallness target and
asked about performance tradeoffs and static Rust/Wasm embedding. No demonstrated
massive architectural speedup was withheld solely for size; parser `s` versus `z`
is the measured tradeoff above. The owner subsequently approved the 2,250,000-byte
native ceiling used for sort accounting, then 2,275,000 bytes for the recovered
conformance work. Future size
decisions should consider measured improvements and the actual Wasm deployment,
not only native hello bytes. Both Wasm targets execute same-build round trips;
browser integration and cross-target snapshot portability are not established.

1. Finish resource accounting and admission with a finite allocation inventory:
   numeric conversion/grouping, class/name/error text, direct container copies,
   and remaining format input/workspace. Record each owner's lifetime and bound;
   fix missing admission in coherent batches, not unrelated micro-optimizations.
   Reclamation workspace is part of admitted records; snapshot output has a
   separate bounded allocation. The shared cap
   covers arena data/metadata, collector mark stack, retained code/source/caches, shapes,
   frame storage and scheduler tables/queues. Preserve
   the distinction between engine allocations, caller-owned code/host values
   and RSS. Interned strings are weak; the intern table keeps its budgeted capacity.
   Failed loads must not accumulate state; genuinely live growth
   must fail at a documented bound. Keep the approved 2,275,000-byte size gate.
   Include prelude trimming: recovered conformance grew the smallest working
   session by 63 KB (+2.7%) and a fresh snapshot by 18%.
   Exit: every inventoried input-sized allocation is admitted or explicitly
   bounded/excluded, with refusal/success controls, recovery and same-budget
   restore; full gate and final-tree GC stress pass.
2. Measure park latency separately from fuel, including large native operations,
   compilation, GC and snapshots. Chunk work or cap it based on measurements,
   not by turning every native into a continuation. Measure the new between-cell
   heap scan on large persistent sessions. `embedding_bench` now covers a small
   representative filtering/join, pagination, batching, error and hibernation
   workload; use it alongside large-data probes, not as a latency bound.
   Exit: publish measured worst-case controls and enforce the chosen bounds.
   Assess fuel overshoot explicitly: current native `burn` saturates the slice
   budget, whereas djs carries debt. Equal aggregate fuel is not currently
   promised to stop micro-monty at an identical instruction.
3. Validate and selectively integrate the recovered conformance work, including
   default/optional modules, explicit timezone/DST rules and restore behavior.
   Do not bulk-merge its dirty work. Expected conflicts:
   `conformance/results/micro-monty.{jsonl,md}` (regenerate with
   `cargo xtask conformance --engine micro-monty`), `crates/mm-stdlib`
   feature list and `FEATURES` bitmask, `xtask/src/main.rs` (size budget,
   new gates), engine files both touched (`builtins`, `interp`, `strmeth`,
   `text`, now `vm` code loading/lifetime).
   Exit: ≥540/568 CPython-tagged cases, no existing-pass regressions, optional
   modules tested enabled/disabled and timezone/DST behavior surviving restore.
4. Run combined gates and local plak suites (`docs/plak-integration.md`),
   then optimize measured bottlenecks and compare equivalent pinned embeddings
   on an idle machine. The ≤ 1.2× QuickJS target remains; historical results
   were 1.6–1.8×. Do not assume PGO, quickening, caches or a startup image
   must be implemented regardless of measured value.
   Exit: fresh plak lifecycle/cancel/restore results without modifying plak;
   fuel-slice/host-result-delivery/restore matrices preserving logical result
   order, and explicit external-effect crash-window tests. Genuine completion
   order differences need not yield identical async results. Benchmark cold and
   warm equivalent embeddings, snapshot costs and memory on an idle machine;
   meet ≤1.2× QuickJS on fib/loop/closure and beat Monty on every specified row,
   or record an explicit unmet acceptance item rather than treating checksum
   success as speed acceptance.
5. Update `docs/spec.md` (v5): §4 now allows metaclasses and wildcard
   imports; §6 size and opt-level; §10 facade additions (clock, entropy,
   module dunders, gc stress, panic poisoning); §10.2 seccomp allowlist;
   §12/§13 measured numbers; §14 milestone status. Update
   `docs/plak-integration.md` failure list. Refresh historical dquickts comparison
   text: current Python bytecode suspension support is broader than the old
   "JS only" description, though direct-call/host-entry restrictions remain.
6. Complete the spec's 24-hour clean fuzz campaigns on the final build, replay
   saved regressions, then run combined gates and full GC stress. Exit: a versioned
   evidence record identifying exact binaries, commands, durations, outcomes
   and remaining exclusions; spec v5 must describe demonstrated guarantees.
   This checkpoint does not start those long campaigns or the next code phase.

The djs source review (revision `f1abbb35ee5284b84156c837c4f9d21f6521e5ca`)
supports keeping explicit heap frames and one dispatch loop. Its generated native
state machines differ from our Python-prelude handoff, but neither interrupts
arbitrary Rust leaf operations. Transfer its scheduling/failure test techniques,
not a new builtin compiler, replay requirement, unsafe heap or cross-build format.
Snapshot compression belongs at the storage boundary if measurements justify it.
Review evidence and pinned source links are in the
[durable-runtime comparison](https://ampcode.com/artifacts/durable-runtimes-dquickts-djs-and-micro-monty-3383bfbd-ae99-4a25-aa90-d3e911a4d36a);
upstream tests were inspected, not run.

Owner decisions already made: SDK checking runs host-side before execution
("type errors; nothing ran"), not as an in-engine type checker; timezone
defaults to UTC with explicit host-supplied rules; `decimal` is in the
default build since decision D2, and `unicodedata` stays optional, measured
and tested both enabled and disabled. The
worker accepts SDK stubs and runs SDK cells unchecked (spec §10.2). Actual plak identity/configuration,
runtime-contract and discovery wiring is a separate harness-side change;
do not modify plak or Monty from this task.

## Commands

```bash
export PATH=$HOME/.cargo/bin:$PATH
cargo xtask check                                   # all gates, ~7 min
cargo xtask conformance --engine micro-monty --filter <case>
cargo xtask bench --engines cpython,qjs,monty,micro-monty   # QJS=/tmp/dquickts/qjs MONTY=/tmp/monty-cli/bin/monty
```
