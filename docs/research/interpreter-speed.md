# Interpreter speed: where mm loses to CPython, and what each fix costs

Commit hashes of micro-monty cited here, other than the public history's
(root `2bcbb8c`), are pre-release history, kept in the private archive
`N0xMare/micro-monty-archive` (branch `history-0.0.1`;
[docs/history](../history/README.md) maps them); the numbers they qualify
are unchanged.

The question was which interpreter changes would make micro-monty several
times faster on the programs where it trails CPython 3.14.7, and what each
change costs in size, risk, snapshot format, durability and rules. This note
covers the profiles, the four prototypes and their measured results, a
ranked list, and how much interpreter speed matters for codemode cells.
Numbers are labelled [M] measured, [S] sourced or [E] estimated.

All measurements ran in one orb (Intel Xeon @ 2.60 GHz, 16 vCPUs, Linux
6.1) on 2026-11-05. Base is `origin/main` 06f1fbc. The prototypes are on a
local branch, `research/speed` (four commits, +382/−48 lines in 11 engine
files). That branch was not pushed. Engine code on `main` is unchanged.

## Summary

- **The starting ratios are stale.** On current main, sort is 1.77× and
  exceptions 1.29× CPython in Ir, down from 9.1× and 3.39× on 2026-11-03 [M].
  Measured that way, the worst programs are generators (3.25×), interp
  (2.78×) and fmt (2.00×). Bare-metal wall clock (thread T-01a1aa37) gives
  the same ranking: generators 2.3–2.9×, interp 2.2–2.5×, fmt 1.6–2.2× [M,
  other thread].
- **No single change gives a multi-× speedup on a whole program.** Simple
  operations already beat CPython (`a + i` 0.52×, an empty loop
  iteration 0.68×, a global load 1.17× in Ir [M]). The gap sits in a dozen
  heavy operations: generator resume (5–7×), instance creation (3.4×),
  builtin calls (`len` 3.6×, `list.append` 3.6×, str methods 4.5×),
  attribute access (2.7–4×) and `%` formatting (4×). Each fix is local and
  gives about 10–30% on the programs that use it. Several of them together
  give 20–30% on the worst programs.
- **The four prototypes together** cut Ir on generators by 30%, classes
  19%, interp 17%, fmt 8% and sort 6%, with no regression elsewhere [M].
  Wall time fell by the same amounts. Pause-anywhere still holds: every
  durability gate passed. The snapshot format does not change. The cost is
  +6.8 KB of shipped wasm (+0.33%).
- **For codemode, interpreter speed hardly matters.** A typical cell (2
  host calls) spends a median 1.4 ms in the engine. Of that, the host
  boundary takes about 8 µs per call, so engine time is about 99%
  interpreter work [M]. Against real tool latencies of milliseconds to
  seconds, the interpreter is noise for the median cell [E]. The slow
  cells (p90 12 ms, p99 35 ms) run pure-Python stdlib code, mostly
  `datetime.fromisoformat`/`strptime` and `shlex`. Native `datetime`
  parsing helps those cells more than any dispatch work would.

## Tools added

- `bench/callgrind.py`: whole-process callgrind Ir for each bench program
  per engine, checked against `expected.txt`, as a Markdown table. Runs go
  in parallel because Ir does not depend on load. CPython runs with
  `PYTHONHASHSEED=0`; without it CPython's Ir varies by up to ±5%.
- `bench/opcost.py`: Ir per operation. Each case runs a loop body N and 2N
  times and takes the difference, so startup cancels. The empty loop's cost
  is subtracted.
- `conformance/codemode/timing.py`, plus a `timing` field in
  `mm-codemode`'s `done` record: where a codemode run's time goes (setup,
  compile, time inside `start`/`resume`, transport, tool handlers).
- The per-opcode histograms below come from a local-only instrumented
  build (an `AtomicU64` per opcode pair, written out under `MM_OPHIST`). It
  is not pushed because it touches the dispatch loop.

## 1. Profiles on current main

Ir in millions, whole process, `mm run` against `python3.14` [M]:

| Program | CPython | mm main | mm / CPython |
|---|---:|---:|---:|
| startup | 39.4 | 0.9 | 0.02 |
| fib | 328.3 | 333.4 | 1.02 |
| loop | 2,913.5 | 1,178.4 | 0.40 |
| dict | 924.1 | 1,169.3 | 1.27 |
| str | 330.1 | 491.4 | 1.49 |
| json | 1,386.0 | 1,140.5 | 0.82 |
| closure | 1,165.8 | 737.0 | 0.63 |
| classes | 1,138.0 | 1,745.1 | 1.53 |
| generators | 1,047.4 | 3,399.5 | 3.25 |
| exceptions | 772.7 | 998.9 | 1.29 |
| fmt | 942.7 | 1,887.6 | 2.00 |
| sort | 889.0 | 1,573.5 | 1.77 |
| interp | 990.7 | 2,752.9 | 2.78 |

Ir per operation [M] (`bench/opcost.py`, N = 20,000):

| Operation | CPython | mm main | mm / CPython |
|---|---:|---:|---:|
| empty loop iteration | 145 | 99 | 0.68 |
| `b = a + i` | 174 | 90 | 0.52 |
| global load | 58 | 68 | 1.17 |
| `l[3]` | 94 | 191 | 2.03 |
| `d['a']` | 157 | 401 | 2.55 |
| `o.x` | 74 | 201 | 2.72 |
| `o.x = i` | 62 | 250 | 4.03 |
| `f(i)` | 276 | 378 | 1.37 |
| `o.m(i)` | 293 | 510 | 1.74 |
| `C(i)` with `__init__` | 963 | 3,312 | 3.44 |
| `s.isdigit()` | 177 | 793 | 4.48 |
| `len(l)` | 149 | 532 | 3.57 |
| `next(gen)` | 428 | 2,442 | 5.71 |
| `for x in gen` (per item) | 313 | 1,642 | 5.25 |
| `sum(x for x in range(n))` (per item) | 352 | 2,664 | 7.57 |
| `raise` + `except` | 1,430 | 2,969 | 2.08 |
| `KeyError` caught | 1,398 | 2,622 | 1.88 |
| `f'u{i:06d}'` | 2,671 | 4,671 | 1.75 |
| `'%s:%d' % ('ab', i)` | 1,676 | 6,622 | 3.95 |
| `'\|'.join([...3])` | 1,130 | 3,297 | 2.92 |
| `l.append(i)` | 213 | 767 | 3.60 |

Dispatch is not the bottleneck. Ir per executed opcode is 110–170 on
classes, interp and generators (15.8M, 19.7M and 20.4M dispatches), but a
plain `LDAR`/`STAR`/`ADD` costs about 5–10 Ir [M]. `LDAR`+`STAR` are
29–36% of the dispatches and only about 1–2% of the Ir [E from the
counts]. Removing them (register operands everywhere) would barely show.

Dominant costs per program, inclusive Ir share on main [M]:

- **generators** (3.25×): `gen_resume` 27.6%, `gen_enter` 24.3%; the
  register copies (`Heap::set_values` 7.6%); `YIELD` going through
  `finish` + `drive` for every item. `sum(genexpr)` and `any(genexpr)`
  cannot drive a generator natively (no nested dispatch loop), so the
  prelude twin `_sum` materializes `list(it)` with one `list.append` native
  call per item (4.2%).
- **interp** (2.78×): `getitem` 18.8% (tuple index and `env[name]`);
  `op_compare`/`compare` 10.6%, because `tag == 'num'` has no str fast path
  and walks `eq_hard`; str predicates 8.9% (`isdigit` copies the receiver
  into a Rust `String`); `call_class` 7.8% (`Token(...)`); `new_str` and
  interning 5.3% and 4.6%; malloc/free 6.8%.
- **fmt** (2.00×): `%` 21.5% and `format` 22.7%. Small ints are formatted
  through the bigint digit loop (`Big::digits`/`big_decimal` 8–9%).
  malloc/free take 15.8%, from `format!` temporaries, `NativeVec` growth
  and string copies. Re-validating UTF-8 on every str read
  (`Heap::as_str` → `from_utf8`) takes 3.6%.
- **classes** (1.53×): `call_class` 23.8% (instance creation: dunder
  lookup through the MRO, `Vec` arguments, `NewInit` continuation) and
  `inst_guard` 18.3% (about 80 Ir per IC hit, mostly two validated object
  reads and `Option`-returning `Value` decoding; dropping the start-bitmap
  check saved 1% [M]).
- **sort** (1.77×): `merge_sort` with its key closure 41% (`order_pure` on
  tuple keys 30%). Key-function calls run through `Cont::Native`
  trampolines (`finish` → `native_step` 52% inclusive).
- **exceptions** (1.29×): `add_tb` 9.7%, `except` matching through
  `getattr`/`class_lookup` 8.7%, `KeyError` construction, dict probes.

## 2. Prototypes (local branch `research/speed`)

| | Change | Where |
|---|---|---|
| P1 | Generator fast path: `FOR_ITER` resumes a suspended or new plain generator with one validated read and no `Arc` clone (`gen_enter_fast`). `YIELD` saves through guarded writes and lands a value yielded to `for`/`next`/`send` directly, without `finish` + `drive`. The prelude's `_list` uses a comprehension (`LIST_APPEND`) instead of `out.append`. | generator.rs, interp.rs, prelude.py |
| P1 | Inline `str == str` in `TEST_EQ`/`TEST_NE`; str predicates borrow the receiver (no `String` copy); small ints formatted with `core::fmt` (`format`, `%`); `%` reserves its output once. | interp.rs, strmeth.rs, text.rs, percent.rs |
| P2 | 32 inline attribute slots instead of 16 (attributes past 16 went to the overflow buffer, which the attribute IC does not cache); inline `is`/`is not`; a one-byte str `in` searches bytes. | class.rs, interp.rs, ops.rs |
| P3 | `Ic::New`: `cls(...)` whose class has a plain Python `__init__` (no `__new__`, metaclass `__call__`, builtin base or exception base; simple parameters) allocates the instance and pushes `__init__` with `Cont::Init`. Guarded by class identity and class version. | interp.rs, vm.rs |
| P4 | Sort keys that are all small ints or all strs compare directly (int compare, byte compare), still through the same stable merge sort. Each comparison is charged one visit. | builtins.rs |

Ir in millions [M], CPython with `PYTHONHASHSEED=0`:

| Program | CPython | main | proto | proto / main | main / CPython | proto / CPython |
|---|---:|---:|---:|---:|---:|---:|
| startup | 39.4 | 0.9 | 0.9 | 1.00 | 0.02 | 0.02 |
| fib | 328.3 | 333.4 | 320.7 | 0.96 | 1.02 | 0.98 |
| loop | 2,913.5 | 1,178.4 | 1,190.4 | 1.01 | 0.40 | 0.41 |
| dict | 924.1 | 1,169.3 | 1,170.1 | 1.00 | 1.27 | 1.27 |
| str | 330.1 | 491.4 | 493.4 | 1.00 | 1.49 | 1.49 |
| json | 1,386.0 | 1,140.5 | 1,140.7 | 1.00 | 0.82 | 0.82 |
| closure | 1,165.8 | 737.0 | 724.1 | 0.98 | 0.63 | 0.62 |
| classes | 1,138.0 | 1,745.1 | 1,406.4 | 0.81 | 1.53 | 1.24 |
| generators | 1,047.4 | 3,399.5 | 2,384.7 | 0.70 | 3.25 | 2.28 |
| exceptions | 772.7 | 998.9 | 998.0 | 1.00 | 1.29 | 1.29 |
| fmt | 942.7 | 1,887.6 | 1,731.6 | 0.92 | 2.00 | 1.84 |
| sort | 889.0 | 1,573.5 | 1,473.2 | 0.94 | 1.77 | 1.66 |
| interp | 990.7 | 2,752.9 | 2,298.1 | 0.83 | 2.78 | 2.32 |

Wall clock [M], median of 11 runs per process, pinned with `taskset -c 2`
in the orb (not bare metal; the ratios are what matter), in ms:

| Program | CPython | main | proto | proto / main |
|---|---:|---:|---:|---:|
| startup | 13.6 | 3.4 | 3.3 | 0.97 |
| fib | 35.5 | 27.7 | 26.1 | 0.94 |
| loop | 206.9 | 70.9 | 71.3 | 1.01 |
| dict | 131.2 | 123.1 | 122.8 | 1.00 |
| str | 48.0 | 52.7 | 53.4 | 1.01 |
| json | 192.5 | 114.8 | 115.5 | 1.01 |
| closure | 86.8 | 51.3 | 49.8 | 0.97 |
| classes | 97.1 | 143.2 | 109.2 | 0.76 |
| generators | 92.5 | 285.2 | 199.3 | 0.70 |
| exceptions | 81.7 | 106.6 | 104.8 | 0.98 |
| fmt | 108.9 | 245.8 | 225.0 | 0.92 |
| sort | 121.3 | 166.1 | 152.0 | 0.92 |
| interp | 97.2 | 264.5 | 211.8 | 0.80 |

Per operation [M]: `for x in gen` 1,642 → 1,122 Ir; `sum(genexpr)` 2,664 →
1,694; `next(gen)` 2,442 → 2,116; `C(i)` 3,312 → 1,724; `s.isdigit()` 793
→ 607; `f'u{i:06d}'` 4,671 → 4,035; `'%s:%d' % t` 6,622 → 5,280. A
`shlex.split` loop (the slowest codemode cell's inner loop) went from 854M
to 668M Ir (−22%), against CPython's 174M including CPython's startup.

An early build inlined `gen_enter_fast` into `dispatch`. That shifted the
loop's register allocation and cost fib +3%, closure +5% and loop +10% Ir
[M]. `#[inline(never)]` removed the regression. Any fast path added to
`dispatch` needs the full table, not just its target program.

**Correctness and durability** [M], on the final prototype:

- `cargo xtask conformance --engine micro-monty --check`: results identical
  (610 pass).
- `cargo xtask durability`: 1180 of 1180 runs identical to an unparked
  run, 107,051 parks, each restored in a new process.
- `cargo xtask durability --cross-target`: 366 of 366 identical, 9,914
  parks across native and wasm32-wasip1.
- Hand checks against CPython: `__init__` reassigned after the cache filled,
  a subclass, `__init__` returning a value, `__init__` raising, wrong
  argument count, `cls(...)` inside a generator. The output and the
  traceback text matched CPython.

**Snapshot format:** unchanged. The generator fast path pushes the same
special-frame record (`Frame` plus `Cont::ForIter`) that `gen_enter` pushes,
so a park inside a resumed generator writes the same state. `Ic::New`
lives in the side IC table. That table is never snapshotted and is cleared
after every collection (`ics.fill(Ic::Empty)`), so its cached `Value`s
never outlive a GC, the same as `Ic::Call`. More inline slots only change
the layout of new instances. The `build_id` changes, as it does for any
engine change.

**Size** [M] (`cargo xtask size`, as shipped): wasm32-unknown-unknown
`hello.wasm` 2,081,288 → 2,088,091 bytes (+6,803, +0.33%); native `hello`
2,180,120 → 2,189,400 (+9,280, +0.43%); `mm_wasm_guest.wasm` 2,230,840 →
2,230,790. Both `hello` budgets have about 1.4 KB of headroom today, so
landing these needs a budget raise. About half the bytes are the generator
fast path, which duplicates `gen_enter`; folding the general path into it
should recover most of that [E].

## 3. Ranking

Gains are on the named programs; "wall" weights use the bare-metal ratios
reported by T-01a1aa37 (generators 2.3–2.9×, interp 2.2–2.5×, fmt 1.6–2.2×,
sort 1.2–1.4×, classes 1.2–1.3× of CPython).

| # | Change | Expected gain | Size | Risk | Rule or spec change |
|---|---|---|---|---|---|
| 1 | **Generator fast path** (P1, without the str parts) | generators −30% Ir and wall [M]; any `for` over a generator −32% per item [M] | ≈ +3 KB [E] | Low. Same frame records; durability and cross-target pass | None. §8.3 already describes it ("copy … with one validated block access each way"); the general path stays for throw, close, coroutines and closures |
| 2 | **`Ic::New` for `cls(...)`** (P3) | classes −19% Ir, −24% wall [M]; `C(i)` 3.4× → 1.8× CPython [M]; interp gets part of its −17% | ≈ +2 KB [E] | Low–medium. Invalidation relies on the class version covering `__init__` rebinding in the class and its bases; tested by hand, needs an own conformance case | Add the row to §8.1's IC table |
| 3 | **Str and format fast paths** (P1, P2: inline `str ==`, borrowed predicates, one-byte `in`, small-int digits, `%` buffer) | interp and fmt −8% each [M]; `shlex` −22% [M] | ≈ +2 KB [E] | Low | None |
| 4 | **No Rust heap temporaries on hot natives** (`format!`, `String` copies in `this()`, `Vec` arguments in `call_class`/`call_vec`, `NativeVec` regrowth) | malloc/free are 15.8% of fmt, 6.8% of interp, 5.6% of classes [M], so about −10% fmt, −5% interp and classes [E] | ±0 | Low; broad but mechanical | None (scratch stays admitted through `NativeVec`) |
| 5 | **Resident generator frames**: keep a suspended generator's registers in a side frame area instead of copying them into its `BUF` at every yield and back at every resume | generators toward 1.5× CPython (another −30–40%) [E] | ≈ +2 KB [E] | Medium–high: GC roots, snapshot encoding of suspended frames, `gi_frame` semantics | §8.3 ("Generators … own a copy of their frame") and §9.3 (a new snapshot section, format version bump) |
| 6 | **Native consumers for generator arguments**: `sum`/`any`/`all`/`min`/`max`/`str.join` over a generator currently materialize `list(it)` in the prelude | `sum(genexpr)` −36% already from P1's comprehension [M]; a bytecode loop twin per consumer (no list) another −20% per item [E] | small | Low | None (still prelude bytecode, no nested loop) |
| 7 | **Cheaper attribute IC guard**: one guarded read of the instance (class ref and shape word) plus a VM-wide class-mutation epoch instead of reading the class's version slot | `o.x` 200 → ~140 Ir, classes −8% [E]; P2's 32 inline slots already give −17% on `shlex` [M] | small | Medium: a global epoch invalidates every attribute IC on any class write | §8.1 IC table guard column |
| 8 | **Faster sort comparisons**: P4's homogeneous keys (−6% [M]); then a CPython-style pre-checked tuple compare (first elements all int or str), and key calls without a `Cont::Native` trampoline per element | sort another −10–15% [E] | small | Low | None |
| 9 | **Unchecked UTF-8 views** of arena strs (`from_utf8_unchecked`, valid because strs are validated at creation and at restore) | 1–4% on bench programs, about 13% of `shlex` [M]; byte-based natives get most of it without `unsafe` [E] | −1 KB [E] | Low technically | An `unsafe` exception in §5, or none if natives move to byte APIs |
| 10 | **Cheaper raise**: build the traceback lazily (`add_tb` is 9.7%), match `except` classes by identity before the MRO walk | exceptions −10–15% [E] | small | Low | None |
| 11 | **Native `datetime` parsing and formatting** (`fromisoformat`, `strptime`, `strftime`) | the slowest codemode cells: `datetime.fromisoformat` runs 3,000 times in 78 ms on mm, 13 ms on CPython (wall, startup included) [M] | +10–20 KB [E] | Medium (CPython's exact errors) | §12's hybrid rule (`datetime` is Python because it is not data-proportional); needs an owner exception |

Not recommended:

- Unchecked arena indexing (§5's other `unsafe` candidate). Spike S1
  measured bounds checks at 6–29% [M, docs/spikes/dispatch.md], but the
  profiles here put the cost in validated object decoding, not in index
  checks.
- Further dispatch work (superinstructions for `LDAR`/`STAR`, threaded
  dispatch). Simple opcodes are already cheaper than CPython's.

**What multi-× would take.** No single change gives it. Items 1–3 together
gave −17% to −30% on the four worst programs [M]. Adding items 4–8 might
bring generators, interp and fmt to roughly 1.3–1.6× CPython Ir [E], about
2× faster than main on generators and about 1.7× on interp. That is
several months of fast paths, each needing its own durability run. The
only structural lever left is item 5 (generator frames), which needs a
spec and format change.

## 4. Codemode: interpreter versus host calls

`conformance/codemode/timing.py` over the 1,782 census solutions, 1,707 of
which raise nothing on either engine; median 2 host calls, 8,135 in total;
8 runs in parallel [M]:

| median per solution | CPython 3.14 | micro-monty main |
|---|---:|---:|
| process wall (spawn to exit) | 86.5 ms | 5.4 ms |
| spawn to first host call or end | 69.0 ms | 3.3 ms |
| cell after the first call (engine + transport) | 0.72 ms | 0.95 ms |
| mocked tool handlers (harness) | 0.43 ms | 0.38 ms |
| mm setup (session, host module, bootstrap cell) | | 0.26 ms |
| mm compile | | 0.10 ms |
| mm inside `start`/`resume` | | 1.40 ms |

- mm's engine time: p50 1.4 ms, p90 12.2 ms, p99 34.4 ms, max 151 ms per
  run; 54% of the runner process's wall time over all runs [M].
- **Host-call boundary**: a cell awaiting the same tool 200 times costs 8.2
  µs of engine time per call sequentially and 7.2 µs under
  `asyncio.gather` (suspend, resume and converting the result, a JSON
  string of a few KB). The harness transport (the runner's JSON, the pipe,
  Python decode) adds 40 µs per call [M]. So 8,135 calls × ~8 µs ≈ 65 ms of
  the 7.7 s total engine time: **about 99% of engine time is interpreter
  work, and under 1% is the host boundary** [M]. The other thread's
  40-session soak (0.7 ms p50 for a 201-call cell natively, about 3.5 µs
  per call) agrees [M, other thread].
- **Against real tools**: at 50 ms per tool call [E], a median cell (2
  calls, 1.4 ms of engine time) is about 1.4% interpreter. Interpreter
  speed matters only for the heavy tail.
- **The heavy tail runs pure-Python stdlib code.** The slowest cells are
  `logs-5xx-paths` (`shlex.split` over three log files: 151 ms on mm
  against 22.6 ms on CPython, which also runs `shlex` in Python), then
  `dt-overlaps`, `dt-outside-hours` and `reshape-sales-pivot`
  (`datetime.fromisoformat`/`strptime`, `Decimal`: 50–67 ms on mm against
  under 5 ms on CPython, where they are C) [M]. With the prototypes the
  `shlex` cells fell to 91–95 ms [M]. The datetime cells did not move,
  because they are bound by Python-implemented datetime code, which is
  item 11.

## 5. What landed (2026-11-05)

The owner approved items 1–4 and item 11's parsers, deferred item 5,
rejected item 9's `unsafe` and let items 6–10 in only within ±0.5% Ir on
the other programs and the size rules (docs/HANDOFF.md scope decisions
35–37; numbers in "Speed landing" there).

| # | Status | Measured [M] |
|---|---|---|
| 1 | Landed | generators −29% Ir, −25% wall; +3,065 B |
| 2 | Landed; spec §8.1 IC row; own case 379 covers `__init__`/`__new__` rebinding in the class and a base, metaclasses, wrong arity, `__init__` returning or raising | classes −21% Ir with 4, −27% wall; +1,718 B |
| 3 | Landed, with P2's 32 inline instance slots | fmt −16%, interp −14% Ir (with 2); +1,415 B |
| 4 | Partly: short `BUILD_LIST`/`BUILD_TUPLE` and instance slots on the stack, small-int digits in a stack buffer, `%` reserves once. Open: `pad_number`, `NativeVec` reserves, `parse_int_text`, `getitem`, `getattr`, `call_class`, `exc_init` | +1,032 B |
| 5 | Deferred | |
| 6, 7, 10 | Open | |
| 8 | P4's homogeneous keys landed (sort −6.2% Ir, the rest within 0.01%); pre-checked tuple compares open | +1,604 B |
| 9 | `unsafe` rejected; byte-level natives open | |
| 11 | `fromisoformat` (CPython 3.14.7's C parser, ported) and a `strptime` fast path landed; `strftime` stays Python | `fromisoformat` 191K → 19.8K Ir per call (CPython 1.9K), `strptime` 423K → 91K (CPython 163K); +4,668 and +9,153 B |
| 12 | Aware `datetime` comparison, hash, arithmetic and `astimezone` after CPython's C, with `_mm.dt_ord`/`dt_ymd` (owner decision 40) | mixed-offset `<` 1.47M → 42K Ir (CPython 6.5K), `hash` 567K → 54K, `astimezone(ZoneInfo)` 1.35M → 131K; `dt-overlaps` 34–49 → 2.7–8.3 ms; +4,080 B |

Together: generators 2.25×, interp 2.29×, fmt 1.65×, sort 1.63×, classes
1.31× CPython in Ir, from 3.18×, 2.77×, 2.00×, 1.76× and 1.65×. fib is
+1.4% Ir and closure +6–8% wall on this orb (−0.8% Ir): dispatch-loop
layout, open. For codemode, `shlex` cells fell from 122 to 87 ms and
`dt-overlaps` from 23–50 to 23–36 ms; the rest of those cells was Python
`datetime` (an aware comparison was 17× CPython in Ir, about 225× with
mixed offsets). Row 12 then took `dt-overlaps` to 2.7–8.3 ms (CPython
0.5–0.9 ms) and all 148 `dt-*` cells from 1.39 to 0.75 s of engine time
[M]; what remains there is imports and Python formatting (docs/history/handoff-0.0.1.md "Aware
datetime").

## Reproduce

```bash
python3 bench/callgrind.py --engine cpython=python3.14 --engine "mm=target/release/mm run"
python3 bench/opcost.py                        # defaults: python3.14 and target/release/mm
python3.14 conformance/codemode/timing.py      # builds mm-codemode (default) first
```

For function-level profiles, build with symbols
(`CARGO_PROFILE_RELEASE_STRIP=false CARGO_PROFILE_RELEASE_DEBUG=true`), run
`valgrind --tool=callgrind`, and read the output with `callgrind_annotate
--inclusive=yes`.
