# Spike S2: execution core redesign, results

Can a redesigned execution core reach about 1.2× QuickJS wall time on `fib`
and `closure`, before anyone ports it into `mm-engine`? This page reports the
prototype and its measurements. Source, exact commands and raw outputs are in
`spikes/dispatch2/` (README.md, `run_all.sh`, `results/`). Labels follow
AGENTS.md: [M] measured on this orb, [E] estimated.

## Answer

Yes, with margin on instructions and narrowly on wall time.

- Instructions, whole process [M]: fib 235.2M against qjs 252.3M (0.93×),
  closure 514.3M against 539.6M (0.95×). mm today runs 402.5M and 975.0M.
- Wall time, whole process, median of 21 pinned interleaved rounds, mean of
  two runs [M]: fib 25.6 ms against qjs 24.4 ms (1.05×), closure 39.6 ms
  against 42.9 ms (0.92×). mm takes 34.2 and 70.5 ms (1.40×, 1.64×).
- With mm's startup added to the spike's kernel time [E]: fib 1.15×, closure
  0.98× qjs.
- The spike keeps the engine's obligations on the paths it runs: exact fuel
  ticks at loop tails and calls, a GC-pending check at every safepoint, a
  recursion limit that raises, exception unwinding through a side table, and
  all state as plain data. Each kernel was parked every tick for its first
  3,000 safepoints and at three other intervals. At every park it was
  snapshotted, dropped and continued in a new VM, with identical results and
  tick counts.
- The hot loop is 10,351 bytes of machine code, against 39,597 + 29,487 for
  mm's `dispatch` + `dispatch_rare` [M]. Edits outside the loop moved no
  kernel; edits inside it moved kernels by at most 0.35% in the final build
  and 2.5% in the first [M]. mm moves ±15% (docs/history/handoff-0.0.1.md).

On fib the spike runs 7% fewer instructions than qjs but takes 5–8% longer.
The likely cause is the one shared indirect dispatch jump, against QuickJS's
computed goto (one jump per handler). That is a hypothesis: this orb has no
hardware counters, and valgrind's branch model is too simple to settle it
(see "Where the remaining cost is").

## What was built

Accumulator + registers as in the engine today, safe Rust only
(`#![forbid(unsafe_code)]`), with one dispatch loop (`Vm::run`).

- **Frames live in the value stack.** A frame is a window starting at its
  header base `hb`: slot 0 holds the running function, slot 1 a link word,
  and the registers start at slot 2. `CALL a n` takes the callee from caller
  register `a`, writes the link into `a+1`, and finds the arguments already
  in `a+2..`. The callee's window starts at caller `hb + a`, so the callee
  value becomes its slot 0 and the arguments its first registers, with no
  copy. There is no `Vec<Frame>`. The link is a SmallInt (`special:1 |
  ret_pc:23 | caller code:16 | hb offset:8`), so the GC and snapshots scan
  `stack[..top]` linearly. The `special` bit marks a generator, coroutine or
  continuation frame, and `RETURN` sends those down the cold path.
- **Code is an index into a flat table** of decoded hot fields (`CodeEntry`:
  bytecode, consts, window size, argc, locals to clear, cells, IC base,
  exception table). The table is an `Rc<[CodeEntry]>`, cloned once per
  `run`, never per call. `CALL` reads the callee's code index from the
  function object (one checked arena read), then indexes the table.
- **FOR_PREP / FOR_LOOP** replace `CALL range` + `GET_ITER` + `FOR_ITER`.
  `FOR_PREP a n exit` checks that `R[a]` is the builtin `range` and that the
  arguments are ints. If so, the counter goes in `R[a]`, the stop in
  `R[a+1]`, and the first value in `acc`. Otherwise it builds an iterator
  object in `R[a]` with `None` in `R[a+1]`, and both instructions take the
  generic path. `FOR_LOOP a body` is bottom-tested (Lua's FORLOOP): it steps,
  jumps back, and ticks once per execution, so fuel matches the engine's
  `JUMP_LOOP` count exactly.
- **Registers go through `&mut [Value; 256]` windows.** An operand is a u8,
  so register access needs no bounds check. The window is re-derived only on
  call, return, and the cold paths that need the whole VM.
- **Instructions are fixed 32-bit words** (`op:8 a:8 b:8 c:8`, `op a imm16`
  or `op imm24`), so one bounds check fetches a whole instruction. The
  engine's fused pairs (`LDA_GLOBAL_STAR`, `LDA_SMI_ADD/SUB`, `TEST_*_JF`)
  become single-word superinstructions.
- **Slow paths are out of line** (`#[cold] #[inline(never)]`). They take
  `&mut Heap` and the side tables, not `&mut Vm`, so the loop keeps its
  register window across them. Rare opcodes run in `cold_op`, which executes
  one instruction from `st` and returns. It is not a second dispatch loop.
- **Execution state** is `Vm::st` (`pc, hb, ci, depth, acc, ticks`), the
  value stack and the arena. A park writes the loop's locals to `st` and
  returns, and `run` continues from `st`. `snapshot::save` and `restore`
  write and read exactly that. ICs are not written and start empty.
- **The heap is a segmented checked arena** as in spec §7.2: every access is
  bounds-checked twice, out of range reads 0, and nothing panics. Functions
  carry closure cells inline. Dicts have an index plus entries. The globals
  version guards global ICs as `gver` does in the engine. The GC is a
  precise mark from the roots with a side bitmap and a work list. There is
  no sweep: the bump arena never reuses memory, so a collection only proves
  the roots are walkable and resets the trigger.

The hot match holds 40 opcodes by default (`full_hot`). The kernels use 23
of them. The other 17 form a realistic hot set that the kernels never
execute: the compare family, fused compare-jumps, MUL, attribute and method
ICs, list indexing, STA_DEREF, NEG, NOT, `is` and list append. They are there
so that register allocation sees a fuller loop. `--no-default-features`
builds the lean loop.

### Kernels

`fib`, `closure` and `loop` are `bench/*.py`. `callfn`, `loopfn` and
`dictint` are new, in `spikes/dispatch2/programs/` (Python and JS):

- `callfn`: `total = inc(total)` 1M times inside `main()`.
- `loopfn`: the same loop with the call replaced by its body,
  `total = total + 1`.
- `dictint`: 200k int keys inserted, then looked up, at module level.

The `callfn` program behind the brief's 642M was not available, and mine
measures 681M on mm.

The spike's bytecode copies mm-engine's compiler output for each program
(disassembled from `mm-compile` at a92b4c1). Only the call window and the
range loop change. Dispatches per iteration, engine → spike: fib 13 → 13
(n ≥ 2) and 4 → 4 (n < 2), closure 10 → 9, callfn 9 → 9, loopfn 4 → 4, loop
7 → 6, dictint 8 → 7 (insert) and 9 → 8 (lookup). The saved dispatch is the
bottom-tested `FOR_LOOP`. Measured dispatches [M]: fib 5,402,781, closure
9,000,018, callfn 9,000,015, loop 18,000,008.

### Obligations, and how they were checked

`dispatch2 test` (60 checks, all `ok`, `results/test.txt`) [M]:

- Results equal the bench programs' printed lines.
- Ticks are exact: fib 635,621 (one per call), loop 3,000,000, closure and
  callfn 2,000,001, dictint 400,000.
- Each kernel is parked every 1 tick for its first 3,000 safepoints, every 7
  for the first 1,000, and every 4,999 and every 100,003 throughout. At each
  park, `snapshot::save`, drop the VM, build a new VM from the program,
  `restore`, refuel and continue. The result, the tick total and the park
  count (⌊ticks / every⌋ when unbounded) must all match. fib parks at every
  call while descending to depth 28 and back.
- Each kernel is also parked every 50,021 ticks with a collection at every
  safepoint.
- A RecursionError at the limit (1,000, and 50) is caught by an exception
  table entry in the module, and the stack grows on demand.
- `range(0, 10, 1)` runs through the iterator-object path, and an empty
  `range(5, 5)` through the fast path.
- An i48 overflow raised three frames down unwinds to an uncaught error at
  depth 0. The engine would promote to bigint in that slow path.

## Method

- Instructions: callgrind, whole process with `--trace-children=yes`
  (`tools/callgrind.sh`), so startup is included. Each engine's `empty`
  program gives the startup to subtract.
- Wall: `tools/wall.py`, whole process, pinned with `taskset -c 2`. One
  untimed round, then 21 rounds. Each round runs every (engine, program)
  once, interleaved, and every run's stdout is checked. There were two
  separate runs.
- Machine: Xeon @ 2.60 GHz (family 6 model 106), KVM, 4 vCPUs, no PMU.
  Toolchain: rustc 1.99.0 with the root release profile (opt 3, fat LTO,
  1 CGU, panic=abort). mm: micro-monty a92b4c1, `target/release/mm`. qjs:
  dquickts b51e92c, `make qjs` (gcc 12.2, -O2, computed goto). Details in
  `results/env.txt`.
- Instruction counts on this orb match the brief for mm (fib 402M, closure
  975M, loop 1,476M) and for qjs (252M, 539M, 838M) [M].

## Results

### Instructions, whole process [M]

| program | spike | qjs | mm | spike / qjs | mm / qjs | spike / mm |
|---|---|---|---|---|---|---|
| fib | 235.2M | 252.3M | 402.5M | 0.93 | 1.59 | 0.58 |
| closure | 514.3M | 539.6M | 975.0M | 0.95 | 1.81 | 0.53 |
| callfn | 369.3M | 504.6M | 681.0M | 0.73 | 1.35 | 0.54 |
| loopfn | 123.3M | 238.6M | 275.4M | 0.52 | 1.15 | 0.45 |
| loop | 867.3M | 838.4M | 1,476.4M | 1.03 | 1.76 | 0.59 |
| dictint | 347.5M | 368.7M | 532.7M | 0.94 | 1.44 | 0.65 |
| empty | 0.3M | 1.5M | 10.8M | | | |

### Instructions per call [M]

| measure | spike | qjs | mm |
|---|---|---|---|
| call statement: (callfn − loopfn) / 1M | 246 | 266 | 406 |
| fib, all instructions per call (net of startup, 635,621 calls) | 370 | 395 | 616 |
| closure, all instructions per iteration (1 call each) | 514 | 538 | 964 |

The call statement covers the global load of `inc`, two moves, CALL, the
callee's dispatch, RETURN and the result store.

### Wall time, whole process, ms: median of 21 rounds [M]

| program | spike run 1 / 2 | qjs run 1 / 2 | mm run 1 / 2 | spike / qjs | mm / qjs |
|---|---|---|---|---|---|
| fib | 25.9 / 25.3 | 24.9 / 23.9 | 34.4 / 33.9 | 1.05 | 1.40 |
| closure | 40.0 / 39.1 | 43.9 / 41.9 | 72.9 / 68.0 | 0.92 | 1.64 |
| callfn | 28.6 / 28.6 | 41.1 / 39.8 | 49.6 / 47.2 | 0.71 | 1.20 |
| loopfn | 12.2 / 11.6 | 24.3 / 23.8 | 21.5 / 21.4 | 0.50 | 0.89 |
| loop | 54.6 / 54.4 | 71.9 / 70.1 | 95.8 / 94.6 | 0.77 | 1.34 |
| dictint | 38.9 / 37.7 | 96.6 / 92.3 | 53.1 / 51.1 | 0.41 | 0.55 |
| empty | 3.2 / 3.1 | 3.8 / 3.5 | 5.6 / 5.5 | | |

The spike's `empty` time is mostly fork, exec and `taskset`.

Net of each engine's `empty` (mean of the two runs), and projected with mm's
startup:

| program | spike | qjs | mm | spike / qjs [M] | spike kernel + mm startup, / qjs whole process [E] |
|---|---|---|---|---|---|
| fib | 22.5 | 20.7 | 28.6 | 1.08 | 1.15 |
| closure | 36.4 | 39.2 | 64.9 | 0.93 | 0.98 |
| callfn | 25.5 | 36.8 | 42.9 | 0.69 | 0.77 |
| loop | 51.4 | 67.4 | 89.6 | 0.76 | 0.80 |
| dictint | 35.1 | 90.8 | 46.5 | 0.39 | 0.43 |

Per-run maxima reached up to 2× the medians on every engine, so only
medians are reported. The two runs' ratios differ by at most 0.03
(`results/wall_run*.txt` keeps every sample).

### dictint: slow paths and handoffs [M]

Dict operations are ordinary out-of-line calls from the one loop
(`getitem`, `setitem`, then `dict_find`/`dict_set`). There is no second loop
and nothing to hand off. Dispatch-loop instructions: spike `run` 134.8M
against mm `dispatch` 233.9M. The dict work itself is spike `dict_set` +
`dict_find` 180.0M, against mm's 216.0M across `add_entry`, `probe_dv`,
`dv`, `dict_get` and `dict_set`, plus 17.2M of hashing. The spike's dict is simpler than the
engine's, so the total (0.65× mm) overstates what the core alone saves. The
GC also ran in this kernel (14.9M in `safepoint`, the inlined mark).

### Machine code [M]

| function | bytes |
|---|---|
| spike `Vm::run` (40 hot opcodes) | 10,351 |
| spike `Vm::run`, lean (23 hot opcodes) | 7,611 |
| spike `cold_op` / `safepoint` (with the mark) / `unwind` | 1,593 / 1,070 / 326 |
| mm `Vm::dispatch` / `Vm::dispatch_rare` (a92b4c1, same profile, unstripped) | 39,597 / 29,487 |

### Stability across unrelated edits [M]

Instruction change against the same build without the edit
(`tools/stability.sh`; the edits are cargo features):

| program | extra unused hot arm | RETURN arm moved to the top | extra `Vm` field | 10 more cold opcodes | edit inside a slow path |
|---|---|---|---|---|---|
| fib | +0.00% | +0.14% | +0.00% | −0.00% | −0.00% |
| closure | +0.00% | +0.19% | +0.00% | −0.00% | −0.00% |
| callfn | −0.00% | −0.00% | +0.00% | −0.00% | −0.00% |
| loop | −0.00% | +0.35% | +0.00% | −0.00% | −0.00% |
| dictint | −0.00% | +0.06% | +0.00% | −0.00% | −0.00% |

The same edits on the first, lean build of the loop
(`results/stability_v1.csv`): the extra hot arm moved closure +1.99%, loop
+2.53%, dictint +1.11% and fib +0.50%; the others moved at most 0.63%.
Edits outside `run` never moved a kernel. Edits inside it can still move it
by a few percent.

The development steps behaved the same way. Each was measured by callgrind
on the same programs [M]. Intermediate binaries were not kept, and the
first and final counts are in `results/`.

| step | fib | closure | callfn | loop |
|---|---|---|---|---|
| first lean loop | 247.3M | 546.3M | 389.3M | 951.3M |
| + `special` check at RETURN | 251.7M | 552.3M | 395.3M | 948.3M |
| + 17 hot arms (`full_hot`) | 259.7M | 572.3M | 406.3M | 957.3M |
| + `a+1` computed per use, not at the loop head | 247.6M | 547.3M | 390.3M | 936.3M |
| + `depth` and `ticks` as `Vm` fields, not loop locals | 243.1M | 539.3M | 382.3M | 933.3M |
| + `LDAR` = opcode 0, global IC caches the value's address | 235.2M | 514.3M | 369.3M | 867.3M |

The `special` check cost 6–7 instructions per return, not 2. Hoisting `a+1`
into the loop head cost 2 instructions per dispatch. These effects come from
what is live at the loop head, not from the handler bodies.

## Where the remaining cost is

- **Dispatch head**: 14 instructions per dispatch [M]: fetch with the pc
  bounds check, opcode range check, operand decode and the jump-table jump.
  That is 76M of fib's 235M and 126 of callfn's 369 per iteration. The code
  slice length and pointer are spilled to the stack (seven loop values in
  total).
- **Global access**: about 25–30 instructions per load hit and about 40
  per store hit, plus the dispatch head [M, loop's per-instruction
  profile]. That is the IC bounds check, the `gver` compare, and the
  segmented arena's two checks and two dependent loads. closure and loop
  each do four per iteration. loop is
  the one kernel above qjs in instructions (1.03×), and its wall time is
  still 0.77×.
- **Call + return**: about 85 + 38 instructions, read from the
  per-instruction profile of the lean build before the last two steps [M].
  CALL does the
  function-kind check, the arena read of the code index, the code-table
  check, argc, the depth limit, composing the link, the window re-derive,
  the tick and GC-flag check, and the locals clear.
- **IPC on fib**: fewer instructions than qjs but 5–8% more time [M].
  valgrind's branch simulator (a last-target predictor) puts the spike's one
  dispatch jump at ~100% mispredicted against 45% for qjs's per-handler
  jumps (`results/branchsim.txt`). This CPU's real predictor uses history,
  so the real rates are far lower and unknown [E]. Stable Rust offers no
  threaded dispatch (`become` is unstable), and
  `-C llvm-args=-tail-dup-indirect-size=64` did not duplicate the dispatch
  [M]. Hypothesis, unverified without a PMU.

## Porting into mm-engine

What would change, by component. Nothing here was tried in the engine.

- **Bytecode format** (`mm-compile`: `bytecode.rs`, `encode.rs`, the
  compiler):
  - Instructions become 32-bit words, and register operands become window
    indices (+2).
  - The register allocator reserves the link slot in every call window:
    callee `r`, link `r+1`, arguments from `r+2`. For methods, the method
    goes in `r` and `self` in `r+2`.
  - The fused pairs become single-word superinstructions, and the `fuse`
    pass goes away.
  - `CALL`'s u16 IC operand goes: the callee's code comes from the function
    object.
  - `range` loops compile to `FOR_PREP`/`FOR_LOOP`. Other `for` loops can
    use the same pair through its iterator path.
  - Operand sets that do not fit one word (`DICT_UPDATE` rbr, `MATCH` bru,
    `MAKE_FUNCTION` ubr, u16 names plus u16 IC) take an extension word.
  - Jumps are word pcs with 16 bits in compare-jumps and `FOR_LOOP`, and 24
    in `JUMP`/`JUMP_LOOP`. Functions above 64K words need a wide form.
  - `Code::verify` checks window bounds (`a + 2 + n ≤ win`), word jump
    targets and the `FOR_PREP` exit word. Exception, position and
    register-name tables move to word pcs. The disassembler changes.
- **`interp.rs` / `vm.rs`**:
  - `Vec<Frame>` and `Frame` go away.
  - `State` becomes `{pc, hb, ci, acc}` plus `depth`/`ticks` fields.
    `st.code: Rc<CodeObj>` becomes an index into the decoded table, cloned
    once per `run` and re-fetched after any operation that compiles code
    (`exec`, `eval`, import).
  - `func` moves to window slot 0. `call_pc` becomes `ret_pc − 1`.
  - `cont` and `coro` set the link's `special` bit, with their data in a
    side stack keyed by `hb` (or extra header slots on special frames only).
  - `push_frame`/`enter_frame` become the inline CALL path. `bind_args`
    stays as the slow path and binds in place.
  - Copying freevars into registers at entry goes away: cells are inline in
    the function object, and `LDA_DEREF` reads through slot 0.
  - Named locals are cleared at entry (`nclear`).
  - `dispatch` + `dispatch_rare` become one `run` with about 40 hot opcodes
    plus a `cold_op` that executes one instruction and returns.
  - Every slow path takes `&mut Heap` and the side tables. Ops that need
    the stack (prelude handoffs, dunder continuations, GC, stack growth)
    sync and reload.
  - Unwinding and tracebacks walk the link chain. The GC roots become
    `stack[..top]` plus the registers in `st`. Stack growth is a cold
    resize charged to execution memory.
- **`generator.rs`**: a generator or coroutine owns a copy of its window
  (func, link with `special`, registers). Resume places it at the
  resumer's top and rewrites the link. The link's offset field must then
  cover a full caller window: 9 bits, not 8, taken from `ret_pc`.
- **`aio.rs`**: links are relative (`hb` deltas), so a suspended task's
  frame chain is a position-independent block of the value stack. Parking
  copies `[base..top]`, and resuming copies it back and rewrites the bottom
  link. The running total of parked bytes stays.
- **`snapshot.rs`**:
  - The VM-registers section becomes `st` plus `stack[..top]` (headers
    included) and the continuation side stack, with no frame records.
  - The code table beside the arena is written in the new word format, so
    `format_ver` is bumped.
  - ICs are still not written.
- **Safepoints and fuel**: unchanged in meaning. `FOR_LOOP` ticks once per
  execution, as `JUMP_LOOP` did. Park pcs move (word pcs, no separate
  `JUMP_LOOP`), so `cargo xtask durability` must still produce equal
  reports, but individual park points differ.

## Caveats

- **It is a spike.** Bigint promotion, MRO lookup, keyword binding,
  generator and coroutine frames, dunder continuations, tracebacks, memory
  limits and code compiled at run time are absent or raise "unsupported"
  in cold paths. They stay off the hot paths measured here, but a port adds
  real code next to them, and the build-step table shows that what is live
  at the loop head moves counts by several percent.
- **Link field widths** (23-bit return pc, 65,536 code objects, 8-bit
  offset) need checking against engine limits. A third header word costs
  one more store per call. Not measured.
- **The spike's heap is simpler** (bump arena, simpler dict, names
  compared by identity). Dispatch-loop instructions compare cleanly; the
  natives' instructions do not.
- **qjs differs in semantics** (int32/float64 numbers, a `Map` for dict,
  `let` loop variables, not globals). Its startup is 1.2M instructions and
  about 0.5 ms more than the spike's.
- **One machine, no hardware counters**, valgrind's simple branch model.
  Wall ratios held within 0.03 between two runs; on every engine the
  maxima reached up to 2× the medians.
