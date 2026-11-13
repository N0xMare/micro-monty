# Handoff: where micro-monty stands

Updated 2026-11-12, after the 0.0.1 release. Read `AGENTS.md`,
`docs/spec.md` (v4) and `docs/sdk.md` first. Labels: [M] measured, [S]
sourced, [E] estimated.

This file is the current state: where the project stands, the open 0.0.2
list and the owner decisions still in force. The dated log behind it (every
phase, measurement and decision up to 0.0.1, 6,463 lines) is
[docs/history/handoff-0.0.1.md](history/handoff-0.0.1.md), unchanged; a
section named in quotes here ("Size pass", "Pre-0.1 follow-ups") is a
section there. Commit hashes in both files, other than those of the public
history (root `2bcbb8c`), are pre-release history: see
[docs/history/README.md](history/README.md).

## Where the project stands

- **0.0.1 is released** (2026-11-12): tag `v0.0.1` on `1cfcde3`;
  `micro-monty` and the five crates it is built from (`mm-core`,
  `mm-compile`, `mm-parse`, `mm-engine`, `mm-stdlib`) on crates.io, pinned
  to each other with `=`; the draft GitHub release from
  `.github/workflows/release.yml` (`mm` and `mm-cell-worker` for four
  targets, the shipped wasm examples, SHA256SUMS, build ids). What 0.0.1
  contains, its build ids and its known issues: `CHANGELOG.md`.
- **The public history** starts at root commit `2bcbb8c`, a squash of the
  pre-release history (2026-10-31 to 2026-11-12), which is kept in the
  private archive `N0xMare/micro-monty-archive`, branch `history-0.0.1`.
- **Shipped sizes** on `ebb22d52` [M] (`cargo xtask size`; ceilings in
  `xtask/size-ceilings.txt`): `hello.wasm` 2,272,614 B, native `hello`
  2,491,264, the mm-wasm guest 2,359,517 and the bundle guest 2,357,952.
- **Conformance** [M]: Monty's corpus on mm 546/568 `cpython` cases and
  62/81 `host` cases (`conformance/results/micro-monty.md`); 301 own cases
  match CPython 3.14.7; codemode 1,701 of the 1,722 solutions CPython
  passes (`conformance/codemode/results/summary.md`); the real gate
  replays 2,841 recorded cells.
- **Verification is local** (owner, 2026-11-12): GitHub runs no gates, and
  a `v*` tag runs only `release.yml`. The landing tier (`cargo xtask
  check`, 10:53 warm after an engine change on a 16-core orb [M]) or the
  release gate (`--release-gate`, 22:18 [M]) runs before every push to
  main, by the path rule in AGENTS.md; before a tag, also the fuzz build,
  `wasm-ir` and `wasm-wall` against the previous tag and the three
  `cargo deny` checks.
  Since `e0fd02c0`, `check` includes `wasm-ir`, the wasmtime guest
  instruction gate, and `wasm-wall` is advisory.
- **0.0.2 is planned** in lanes, each with an owner thread: the
  [0.0.2 plan](https://ampcode.com/artifacts/micro-monty-0-0-2-plan-cleanup-size-speed-and-the-deferred-l-0eb3fee4-6b6e-480c-adc9-7bada40d1ec0)
  (gates first, then wasm and dispatch speed, size, code cleanup,
  correctness, import cost). Its targets: `hello.wasm` at most 2.15 MB;
  wasmtime at most 1.00 against `7ac25ef` on every bench program, by the
  instruction gate; fmt within ±0.5% of `4b32aa2`; one chunked path per
  builtin. The list below is the work it draws from.

### Open since before 0.0.1 (the long-run acceptance list)

- **Speed:** the dispatch function's register allocation is the remaining
  lever (calls/globals fast paths, pair fusion, phase 3 and the speed
  landing are in). The ≤ 1.2× QuickJS bar is a target, not a ship gate;
  beating Monty on every row, or recording an explained gap, is the gate.
- **Native allocation ownership:** numeric conversion pieces, name/error
  helpers and remaining native copies and workspace. The Rust heap is
  bounded (2.5 × limit + 16 MiB while a cell runs, spec §11, checked by
  `memory_probes`); there is no RSS guarantee.
- **Latency:** native steps are bounded at about 50 ms on every row of
  `tests/steps.rs` (spec §5); full collections, snapshots, restores and
  compilation are not steps and are not bounded.
- **Optional modules and time zones:** complete the optional-module and
  explicit time-zone contracts, host-side SDK work, fresh comparative
  performance and local plak reruns (`docs/research/plak-integration.md`).
- **Fuzzing and spec v5:** the spec's 24-hour clean fuzz campaigns on a
  final build, then spec v5 describing what is demonstrated.

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
   **Landed** in `e0fd02c0` as `cargo xtask wasm-ir`, in `check` on Linux
   (since `b6e40be3` it fails on rises only, past 1.0% for a program or
   0.5% for the geomean; AGENTS.md); `wasm-wall` is advisory since. It cannot see the host-tuned layout cost above (counts
   at most 1.010 generic); `wasm-wall` measures that (AGENTS.md).
2. **The wasmtime spill, fixed** with the lean wasm loop below, which hits
   the same spill. 0.0.1 ships it as a known issue: since 4fe04e1,
   Cranelift spills the linear-memory base in `Vm::execute` and reloads it
   before each opcode fetch; guest instructions fib +8.0%, closure +10.2%,
   generators +6.3%, wall 1.055 / 1.087 / 1.042 [M]; V8 and native
   unaffected (sdk.md, "Embedding guarantees").
3. `copy.deepcopy` in steps: no longer needed. Held marks made its steps
   row Bounded at 25–26 ms [M] ("GC: park first" and its
   "Superseded by held marks" in the log).
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

### Details behind the list

- **The lean wasm loop** (items 1 and 2; owner, 2026-11-10; "The wasm32
  lean dispatch loop"). A dispatch loop on wasm32 only, every slow path
  behind one call of a one-instruction step instance; cfg-stripped
  natively. Local branch `arch/wasm-lean` (`35c216e` on `7df5282`, wasm
  thread T-01a1c114). On `7df5282` [M]: `hello.wasm` +16,008, guest
  +16,011; V8 default tiering geomean 0.904 (fib 0.501), steady state
  0.956 and 0.961; wasmtime 1.027 and 1.033, failing `wasm-wall`, because
  Cranelift reloads the linear-memory base from the stack (39.1M reloads
  on fib, 135M on loop). Find a shape whose hot values cannot lose their
  registers to calls, or accept a measured wasmtime cost for the V8 gain.
- **Stepped short division** (item 9; "Number formats in steps"). `div_ns`
  divides by two limbs at about 22 ns a quotient limb, so `pow(a, 2, 1 <<
  64)` and `a % m` with a two-limb `m` ran 46–60 ms at the int cap, and
  are refused with `MemoryError` there (owner: accepted (b1) behaviour). A
  stepped short division, with the remainder and the position as its
  state, would run them at the int cap, as CPython does in a few ms.

### Other deferred work, by origin

- **Annotations as CPython 3.14 keeps them** (PEP 649/749; owner,
  2026-11-09, "`__import__` for 0.0.1"): `__annotate__` code objects, a
  slim `annotationlib`, `get_type_hints` with a small evaluator. 2–3 days
  [E], +6–10 KB `hello.wasm` [E]; gains 6 corpus cases and about 70
  Lib/test methods; 0 real cells and 0 solutions read annotations [M].
- **`locals()` with PEP 667 semantics**, and `vars()` without arguments
  (same origin): about 1 day [E], +1–2 KB [E]; 38 Lib/test methods, 0
  real cells and 0 solutions [M].
- **Size review follow-ups** (owner, 2026-11-09, "Engine size review";
  lane S of the plan):
  - (a) trust object tags in the accessors: branch `size-review/trust`
    `2bf05d9` [M]: `hello.wasm` −40,872, native −19,712, guest −41,538;
    wasm wall 0.85–1.04×; native Ir json −7.6%, startup −9.1%, but loop
    +8.9%, closure +2.5%, fib +1.9%. It weakens spec §7.2's defense in
    depth, so it needs a spec decision and a fix for the dispatch effect
    first.
  - The dispatch-closure spike: make what LLVM inlines into
    `Vm::dispatch` explicit and stable. It unblocks B2, accessors out of
    line per function (`size-review/acc-v4` `3aef7ca`: −97,101
    `hello.wasm` / −75,952 native / −97,316 guest, but loop +4.0%, fib
    +2.8% [M]). Pinning 17 slow paths `#[inline(never)]` changed nothing
    [M].
  - `optimize(size)` at Rust 1.101 (about 2026-12-24 [E]; owner D4, a
    toolchain bump in its own commit with a re-bless): drop the
    `cfg_attr(mm_shipped, …)` and the feature line, keep the attribute,
    tune per function toward the review's C2/C3 sets, another 20–30 KB
    [E]; keep the fmt-hot helpers and exc.rs out of the set.
- **Typed host functions built on first use** (after 0.0.1, decision 60):
  drops the ~2.1 KB per typed tool that images still carry. Additive.
- **Hygiene from the pre-0.1 review**, none blocking: L1 `_mm` importable
  by guests, L3 the facade phase byte not cross-checked on restore, L4
  `test-hooks` / `ENGINE` / seccomp switch naming, L9 host-built value
  names unvalidated, L10 `set_deadline` on `wasm32-unknown-unknown`.
- **A one-line `tokenize` source** (decision 65 (c)): `tokenize.py` copies
  the line into every token, so a 64 KiB one-line source can end in
  `MemoryError` after up to 10 s of steps.
- **Built-in import images** (decision 47, prototype on unpushed
  `local/import-image`) and **resident generator frames**
  (decision 37, item 5) stay deferred; lane I of the plan revisits import
  cost with the shared decoded-module cache first.
- **The dlmalloc upstream issue** (decision 41): the draft is at the end of
  "Scope decisions" in the log; not filed. Once a pinned stable ships
  `validate_size` as a debug assertion, delete `crates/mm-wasm-alloc` and
  its §5 exception.

## Owner decisions in force ("Scope decisions")

Rules that bind every change are in AGENTS.md ("Rules", "Delivery") and
docs/spec.md; this list is the decisions behind them and the rest. The
numbered ones keep the numbers of the log's "Scope decisions" (2026-11-05
onward), which docs and code comments cite as "HANDOFF scope decision N";
the full text of each is there.

### 2026-11-12, after 0.0.1

- **GitHub runs no gates.** `ci.yml` was deleted (`1cfcde3`); a `v*` tag
  runs only `release.yml`, which builds the artifacts and opens a draft
  release. Before tagging, run the release gate, the fuzz build,
  `wasm-ir` and `wasm-wall --base <previous tag>` and the three
  `cargo deny` checks
  locally (AGENTS.md).
- **D1 (a): evidence out of the tree, history kept.** Measurement outputs
  no gate, test, xtask command or script reads move to a public
  `N0xMare/micro-monty-evidence` repository at the same paths, with a README
  pointer left in each place; the root commit keeps the old copies. Not
  done yet: the repository has to be created by the owner (an agent's
  GitHub token cannot create repositories). The set is `bench/evidence`
  except `engines/check.py`, `engines/fixtures/` and `engines/js/` (which
  `hosts/js/bench` reads), `hosts/js/bench/results`,
  `conformance/breadth/cpython-tests/results` and
  `conformance/breadth/difffuzz/raw` with its `run.log`: 1,300 files,
  22.6 MB [M].
- **D2: the size gate becomes a delta against the base**, with the reason
  in the commit, replacing the ceiling ratchet; `xtask/size-ceilings.txt`
  stays as read-only history.
- **D3: stdlib out of the core module** is decided after the size map (S1).
- **D4: toolchain bump for `optimize(size)`** when Rust 1.101 is stable.
- **The 0.0.1 drift exception** (above) and the known issues CHANGELOG
  lists for 0.0.1 stand until their 0.0.2 items land.

### Release and process

- **Bench Ir moves at most ±0.5% per change** (2; HANDOFF scope decision
  2 in AGENTS.md), on the gated count (63): from glibc's
  `__libc_start_main`, without `pthread_getattr_np`. Exceptions the owner
  accepted are recorded with each change (below, and the json steps'
  +2.1%, decision 65).
- **Exceptions accepted as code-layout or libc noise** (13, 32, 39, 45,
  60, 63): `startup` moved with glibc parsing `/proc/self/maps`, which is
  why the gated count exists.
- **Size raises** (S1, 12): raise a ceiling to the measurement plus about
  2 KB, with the reason, by appending to `xtask/size-ceilings.txt` (until
  D2 replaces it).
- **semver from 0.1**: before each release, `cargo semver-checks
  check-release -p micro-monty` against the last published version; the
  internal crates have no semver promise ("Release hygiene for 0.0.1").
- **Publishing** (55; its source hashes in the build id were replaced by
  66): the facade and its five engine crates publish,
  pinned with `=`; MSRV is the pinned stable. `mm-wasm-abi` waits for
  `mm-wasm-host`; `mm-cell-worker` stays unpublished while it depends on
  `plakous-cell` by git rev. **Licences** (52): every published crate is
  `MIT AND PSF-2.0 AND Unicode-3.0` and ships LICENSE, THIRD_PARTY.md and
  its README.

### Scope and stdlib

- **1.** Stdlib tiers: one `mod-*` feature per module; tool and data modules
  default-on by the import census; the long tail opt-in.
- **2.** Gap-closing rule: a change that closes a non-sandbox gap keeps bench Ir
  within ±0.5%, records its size delta and leaves the sandbox exclusions
  unchanged.
- **3.** Object-model features (`__del__`/`weakref`, writable
  `locals()`/`globals()` and `exec` into them, frame objects/`settrace`)
  each need their own measured decision.
- **4.** No CPython C-API.
- **5.** A2: `HostSignature::dict_as_kwargs` (default off).
- **6.** B2: structured host errors, `HostError { data, class }`; raising stays
  the default.
- **7.** D2: `decimal` (with `numbers`, `contextvars`), `statistics` (with
  `fractions`) and `tomllib` are in the default build.
- **8.** J2: `json.loads` takes all five hooks.
- **9.** Z1: `zoneinfo` over TZif bytes the host serves
  (`Session::set_zone_source`); no bundled tzdata.
- **10.** G2: `globals()` is a read-only view.
- **11.** T1: the census-gated modules stay in the default build.
- **12.** S1: one default build, no profiles (see also 62).
- **13–16.** Embedding hardening: the closure/startup Ir and `ac10b28` budget
  exceptions accepted; parser frames not shrunk (about 1 MiB of host stack
  at the nesting limit); a deadline alone does not stop a big-int
  operation once started (documented).
- **26–29.** macOS: no OS limits or Seatbelt profile for child workers; the
  child-mode stall deferred (30–40 concurrent agents expected); no
  separate macOS size budget; the 6 own cases CPython prints differently
  on macOS stay skipped there.
- **30.** Container-aware repr truncation (`TextCut`) as built.
- **31.** `TimeoutError` for deadlines is a documented convention; a cell
  deadline parks and the host decides.
- **34.** Pre-run type checking is the host's ("type errors; nothing ran");
  micro-monty adds no static checker; the worker runs SDK cells unchecked
  (spec §10.2).
- **35–37.** Speed items 1–4 landed; `datetime`'s parsers are native (a narrow
  §12 exception); resident generator frames deferred, no `unsafe` str
  views, items 6–10 only within ±0.5% Ir and the size rules.
- **40.** Aware `datetime` follows CPython's C, with `_mm.dt_ord` and
  `_mm.dt_ymd`.
- **43.** The default text limit is 1 MiB, or the memory limit when less;
  printed output is not cut.
- **46.** Bundled data in nine packed sections; `mod-asyncio` and `mod-json`
  gate the engine's own modules.
- **62.** No stdlib profile feature ships; the `mod-*` graph is checked at build
  time; `mod-math` is gone.
- Time zones default to UTC with explicit host-supplied rules;
  `unicodedata` stays optional, tested enabled and disabled.

### Wasm embedding

- **17.** Placement: `mm-wasm-abi` and `mm-wasm-guest` in the workspace,
  `mm-wasm-host` its own workspace; the §5 exception covers the guest's
  import block and `#[unsafe(no_mangle)]`.
- **19–22.** The guest polls the park flag through the deadline clock; park
  grace 100 ms; the call budget off by default; one session per instance;
  async wasmtime is the host's work.
- **24, 51, 68.** The interface is frozen with 0.0.1 at `ABI_VERSION` 1 (68
  supersedes 18, 24's v1 and 51's v2): the options record is tagged; a
  release may add a session option or an optional export, anything else
  bumps `ABI_VERSION` with `crates/mm-wasm-host` and `hosts/js` in the same
  change; only hints and diagnostics take an ignorable tag (from `0x80`).
- **25.** `cargo xtask wasm-host` gates the shipped guests, outside `check`;
  required before pushes touching the wasm crates, the facade API or the
  snapshot format.
- **41.** `crates/mm-wasm-alloc`, a temporary §5 exception (supersedes 23);
  registered by the guest and examples, never by the facade.
- **48.** asyncio's attributes on first use; the Engine caches prelude chunks,
  off on the guest (192 KiB per instance for 80–180 µs).
- **70.** The bundle guest ships beside the default guest, with one build id.

### Facade API

- **44.** `mm-cell-worker` is a library plus a thin binary; `Config` with the
  protocol host's values as defaults; no new policy features.
- **45.** The Engine owns a cache of decoded bundled modules, charged once to the
  Engine (`DEFAULT_MODULE_CACHE`, 8 MiB); nothing evicted.
- **49.** Outputs are `#[non_exhaustive]`, inputs have `Default`.
- **50.** Fuel is per cell, with an optional session total
  (`Session::set_fuel_total`).
- **53.** Host misuse has its own errors (`DefineError`, `SnapshotError`'s
  variants); a module named like a bundled one is refused.
- **54.** Park requests survive `start` and `cancel`.
- **56.** Every public byte quantity is a `u64`; only the engine clamps.
- **57.** The facade forwards every `mod-*` feature; the internal crates say
  "no semver guarantees; depend on micro-monty".
- **58.** sdk.md is the crate documentation (doctests in `cargo xtask docs`).
- **59.** `Engine::without_prelude_chunk_cache` is `#[doc(hidden)]`.
- **60.** Host-module code is compiled once per Engine; images refer to it by key
  and restore regenerates it (keyed images stay the default;
  `with_host_code_in_snapshots(true)` for one session per process). A
  cached restore checks the image's definition hash ("Pre-0.1
  follow-ups" 1).
- **61.** An uncaught exception's report is bounded by the text limit and the
  deadline (`PyException::truncated`).
- **67.** `Limits` and `EngineConfig` are `#[non_exhaustive]` with `const`
  builders; `SessionSettings` carries what an image does not.
- `Session::pending()` re-reports the stop; the facade keeps the payload
  ("Pre-0.1 follow-ups" 3).

### Snapshots and durability

- **33.** Snapshots stay FULL; engine-side deltas are closed. Hosts compress at
  the storage boundary (zstd level 1 with the fresh-session image as a
  prefix; docs/sdk.md "Storing snapshots").
- **38.** U1: a snapshot restores only on the build that wrote it; hosts keep old
  builds keyed by build id and drain; no cross-build migration.
- **66.** The build id is a layout descriptor (supersedes 55's source hashes):
  bump `SNAPSHOT_EPOCH` when a function body changes what an image means
  with no hashed item changed; every id move re-blesses the golden images
  in the same commit (AGENTS.md "Rules").
- From 0.0.1 every counter started at 1 (`SNAPSHOT_FORMAT`, `ABI_VERSION`,
  `SNAPSHOT_EPOCH`). Before 0.0.1, changes went straight to the end state
  with no compatibility glue (68); from 0.0.1 the frozen surfaces above
  apply.

### Bounded work

- **64.** The host keeps control without exceptions: `collect` parks between
  chunks; long sorts and big factorials ask first; straight-line code
  checks (C4).
- **65.** No native step longer than about 100 ms, enforced as 50 ms native
  between safepoints at the default limits (spec §5 "Bounded native
  steps"), with:
  - (a) one object at most 64 MiB (`MAX_OBJECT_BYTES`);
  - (b) superlinear big-int operations estimated first and refused past
     `WORK_MAX`, including (b1)'s refusals above;
  - (c) source caps: `ast.parse` 32 KiB, `tokenize` 64 KiB;
  - (d) incremental marking over 32 MiB live, then park first and held
     marks (spec §9.2, superseding allocate-black);
  - NFC/NFD: a run with no boundary over 65,536 chars raises
     `MemoryError` (owner option 2, 2026-11-10; item 5 steps it instead);
  - fuel overdraft at most 2^21 + 2^17 units past a slice, checked by
     `steps.rs`.
- **Fuel after a restore on long non-ASCII strs** (owner option (d) for
  0.0.1): documented in sdk.md and spec §5 guarantee 4; item 16 fixes it.
- **Near-limit retries** (owner option (a)): full retry coverage through an
  instruction-success hook (item 11).

### Size and build

- **69.** Accessors out of line outside `Vm::dispatch`'s inline closure: new code
  outside the dispatch files calls the `_o` forms; shipped builds use
  `#[cfg_attr(mm_shipped, optimize(size))]` on the cold set ("Engine size
  review" has the file lists).
- **42.** The facade builds at `opt-level = "z"`; `mm-engine` and `mm-core` at 3,
  `mm-parse` `z`, `mm-compile` `s`.
- Shipped builds: release, std rebuilt with `-Cpanic=immediate-abort`, no
  unwind tables, `wasm-opt -Oz` on wasm (owner, 2026-11-03, "Size by build
  structure"); PGO (+28% size), BOLT, other `wasm-opt` levels and a
  code-mode stdlib profile were measured and not adopted.
