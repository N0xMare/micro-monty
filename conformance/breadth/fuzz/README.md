# Bounded fuzz campaign on `main` (2026-11-04)

This campaign ran each of the seven libFuzzer targets in `fuzz/` for 20
minutes. The engine crates were the same as `main` at `11a89b0` (no diff in
`crates/` from `fec4ff6`, where the campaign started). The
`snapshot_restore.rs` fix to the `Session::define(&HostModule…)` API, now
committed in `11a89b0`, was in the tree.

**Result: no crash, panic, sanitizer report, OOM or timeout in any target.**
There were three slow-unit reports (libFuzzer's >10 s report threshold). All
three are bounded and none is a panic on guest input. See the findings below.
Nothing resembling the old `Heap::raw_words_mut` subtraction overflow (dict
`set_counts`, from the unaccepted branch) turned up on `main`.

## Setup

- Toolchain: `rustc 1.101.0-nightly (f9ceb4a4d 2026-10-31)`
  (`nightly-2026-11-01`) and `cargo-fuzz 0.13.2`. The build is cargo-fuzz's
  default: release with debug info, AddressSanitizer, and debug assertions
  and overflow checks enabled.
- Build check first:
  `cargo check --manifest-path fuzz/Cargo.toml --locked --bins`, which
  passes. `cargo +nightly-2026-11-01 fuzz build` builds all 7 targets.
- Corpora: none existed under `fuzz/corpus`, so every target started from
  an empty corpus. The corpora the campaign grew stay in `fuzz/corpus/<target>`
  (git-ignored).
- Cores: at most 6 at once. Six targets ran in parallel, one process each
  (21:23:33–21:43:43Z). `vm_bytecode` then ran alone (21:44:31–22:04:39Z).
  The machine was shared with other jobs (load average about 12 on 16
  CPUs), so exec/s figures are low compared with an idle machine.
- libFuzzer's defaults apply otherwise: `-rss_limit_mb=2048`, no `-max_len`
  (4096). I added `-timeout=60` because the default of 1200 s would never
  fire inside a 1200 s run, and `-print_final_stats=1`.

Command, run from `fuzz/`:

```bash
cargo +nightly-2026-11-01 fuzz run <target> -- -max_total_time=1200 -timeout=60 -print_final_stats=1
```

## Per target

The `cov`/`ft`/`corp` values come from the final `DONE` line. "Corpus on
disk" counts the files in `fuzz/corpus/<target>` afterwards.

| target | duration | executions | exec/s | cov | ft | corp (final line) | corpus on disk | peak RSS | findings |
|---|---|---|---|---|---|---|---|---|---|
| arena_ops | 1201 s | 712,447 | 593 | 1929 | 10614 | 2292 / 303 KB | 2287 files, 466 KB | 444 MB | none |
| json_loads | 1201 s | 922,709 | 768 | 7890 | 17645 | 1383 / 121 KB | 1362 files, 218 KB | 514 MB | none |
| nested_data | 1201 s | 4,264 | 3 | 9077 | 24395 | 692 / 2277 B | 663 files, 55 KB | 662 MB | 2 slow units (F1, F2) |
| parse | 1201 s | 1,891,267 | 1574 | 4323 | 17728 | 4530 / 295 KB | 4523 files, 617 KB | 844 MB | none |
| re | 1201 s | 267,120 | 222 | 9294 | 31329 | 2422 / 68 KB | 2378 files, 238 KB | 452 MB | none |
| snapshot_restore | 1201 s | 322,030 | 268 | 14538 | 32086 | 1424 / 18 KB | 1423 files, 126 KB | 954 MB | none |
| vm_bytecode | 1207 s | 339,850 | 281 | 14922 | 36685 | 1762 / 15 KB | 1760 files, 150 KB | 526 MB | 1 slow unit (F3) |

Every run ended with `Done N runs in ~1201 second(s)` and exit status 0.
Seeds: arena_ops 4280214886, json_loads 450727988, nested_data 844933149,
parse 1047217524, re 1521928387, snapshot_restore 1251329754, vm_bytecode
843620895.

## Findings

None of the findings is a crash. libFuzzer wrote each one as a `slow-unit-*`
artifact (a unit slower than 10 s, still under the 60 s timeout). I
minimized each with
`cargo +nightly-2026-11-01 fuzz tmin <target> <artifact> -r 30..50 -- -timeout=5`,
which turns slowness into a timeout that tmin can minimize. Then I
reproduced each one with the fuzz binary, a plain release build of the
same harness, and CPython 3.14 where that applies. Backtraces come from
gdb samples of the running fuzz binary.

### F1: nested_data, cyclic list of a frozenset chain compared with `<` (fuel-bounded slow path)

- Input: `artifacts/nested_data-slow-unit-7df227d9-min`. It is 3 bytes,
  `ff 65 27` (base64 `/2Un`). tmin could not shrink it further. The
  original is `fuzz/artifacts/nested_data/slow-unit-7df227d9576cae3e25dfc6739d5cce8d69a76b93`.
- Program: `_n(b'\x65\x27', [15, 15, 0])`. It builds 733 nested
  `frozenset([x, 2])` inside a self-referencing list `c = [x]; c.append(c)`,
  twice. Then it runs `max([y, x])` twice and `repr`.
- Reproduction:
  - In the fuzz binary the input took 19 s during the campaign and 39 s
    standalone on the loaded machine. It stops when the cell's
    5,000,000-unit fuel budget runs out.
  - In a plain release build of the facade, run time grows linearly with
    fuel: 1 M fuel parks after 0.22 s, 5 M after 1.0 s and 20 M after
    4.1 s.
  - `mm run`, which sets no fuel limit, had not finished after 120 s.
  - CPython 3.14 raises `RecursionError` in about 4 s.
- Backtrace, top frames (gdb sample, repeating):
  `set_issubset (mm-engine/src/set.rs:608)` ← `set_eq (set.rs:628)` ←
  `eq_hard (dict.rs:718)` ← `set_find_with (set.rs:154)` ←
  `set_find (set.rs:116)` ← `set_eq (set.rs:628)` ← … This is the native
  frozenset equality walk inside the list ordering (`order_pure` /
  `order_seq`, `ops.rs`).
- Scaling: with the 16-entry `frozenset` chain inside the cycle, `b < a`
  takes 8 ms at 14 levels and runs past 25 s at 16 levels. At that point
  the native walk runs out of its `WALK` visit budget and is restarted.
- Classification: not a panic, not a hang. Every walk is depth-counted and
  budgeted, and the work is charged to fuel, so the run stops at the
  configured fuel limit. It is a performance divergence from CPython:
  CPython fails fast with `RecursionError`, while micro-monty runs until
  fuel runs out, much longer without a fuel limit. I recommend it as a
  performance follow-up, not a reliability blocker. No engine change was
  made.

### F2: nested_data, `==` on a 2^8-path shared DAG of frozensets (instrumentation overhead)

- Input: `artifacts/nested_data-slow-unit-9587297f-min`. It is 3 bytes,
  `02 65 0b` (base64 `AmUL`), minimized from the 4-byte
  `fuzz/artifacts/nested_data/slow-unit-9587297f85c8e649fdcfedd8c535f128a827beae`
  (`02 65 0b 00`).
- Program: `_n(b'\x65\x0b', [2, 0, 0])`. It builds a 733-deep frozenset
  chain under a DAG that shares 8 levels, then runs `x == y` and
  `repr` twice.
- Reproduction:
  - In the fuzz binary the input took 11 s during the campaign and 23 s
    standalone.
  - A plain release build completes in 0.59 s, with 5 M and 50 M fuel
    alike.
  - CPython 3.14 takes 0.91 s.
- Classification: not a bug. This is expected exponential-in-sharing work,
  which the target caps at 2^8 paths on purpose (comment in
  `nested_data.rs`). It is fast in release. The slowness comes only from
  ASan, coverage and debug-assertion overhead.

### F3: vm_bytecode, many asyncio tasks (O(tasks) `debug_assert` per task switch)

- Input: `artifacts/vm_bytecode-slow-unit-7b67c590-min`. It is 6 bytes,
  `65 11 5d 26 01 07` (base64 `ZRFdJgEH`), minimized from the 9-byte
  `fuzz/artifacts/vm_bytecode/slow-unit-7b67c590dbb9b40671bc5cec78f3ea70e4d1fb5a`.
  It runs program 5 (the `asyncio.gather` program) with one edit:
  byte 791 of the 1047-byte encoding goes from `0x00` to `0x26`. The
  result passes `Code::verify` and parks after the first 20,000 fuel. The
  two 5,000-fuel resumes then switch among many tasks.
- Reproduction:
  - In the fuzz binary the input took 14 s during the campaign and 28 s
    standalone.
  - A copy of the same harness built optimized with debug assertions takes
    0.85–1.0 s.
  - The same copy built in release without debug assertions takes 16–21 ms.
- Backtrace, top frames (three gdb samples, all identical):
  `core::iter::…::sum` over `Option<Task>` ←
  `Vm::execution_bytes (mm-engine/src/lifecycle.rs:103)` ←
  `Vm::reserve_execution (lifecycle.rs:126)` ←
  `Vm::activate (aio.rs:559)` ← `Vm::next_task (aio.rs:656)` ←
  `execute (interp.rs:367)` ← `resume (interp.rs:212)` ←
  `vm_bytecode.rs:94`.
- Cause: the `debug_assert_eq!` in `execution_bytes` (`lifecycle.rs:97-105`)
  sums `buffer_bytes` over every task. It runs on each task activation, so
  task switching costs O(tasks²) when debug assertions are on. In release
  `execution_bytes` reads the cached `parked_bytes` in O(1).
- Classification: a timeout-like slow unit caused by a debug-only
  assertion, not a real hang. It is fuel-bounded and takes milliseconds in
  shipped builds. It only lowers fuzzing throughput, so it is optional
  harness or debug-build hygiene and not a reliability bug.

## Reproducing

```bash
cd fuzz
cargo check --manifest-path Cargo.toml --locked --bins
cargo +nightly-2026-11-01 fuzz build
# campaign (one per target; six in parallel, then vm_bytecode)
cargo +nightly-2026-11-01 fuzz run <target> -- -max_total_time=1200 -timeout=60 -print_final_stats=1
# minimize a slow unit (treated as a timeout at 5 s)
cargo +nightly-2026-11-01 fuzz tmin <target> artifacts/<target>/slow-unit-<hash> -r 50 -- -timeout=5
# reproduce
target/x86_64-unknown-linux-gnu/release/<target> ../conformance/breadth/fuzz/artifacts/<file>
```

## Follow-up (2026-11-05): F1 and F3 fixed

- F1: comparison depth now carries across the levels the prelude runs in
  bytecode, and a walk started at nesting `d` gets `max(2^14 >> d, 1)`
  visits (docs/spec.md §5, docs/HANDOFF.md "libFuzzer F1 and F3"). Release
  `mm`, unlimited fuel, CPython 3.14.7 in brackets: the F1 input 145.9 s
  and 665M fuel → 0.15 s and 0.99M [4.0 s]; `b < a` with 16 frozenset
  levels 62.7 s and 333M → 0.07 s and 0.41M [0.06 s]. The F2 shape is
  faster too: `Jl0r` (`==` on a 2^8-path DAG over 672 frozensets) 2.28 s
  → 0.50 s [0.64 s].
- F3: the `parked_bytes` check runs where it is O(1) amortized (task table
  doubling, dropped tasks, snapshots). The artifact takes 0.54 s in the
  fuzz binary (was 28 s).
- Rerun on `f1cac8d`, 20 minutes each from empty corpora, same command
  and build (machine shared with a `cargo xtask check`):

| target | duration | executions | exec/s | cov | ft | corp (final line) | peak RSS | findings |
|---|---|---|---|---|---|---|---|---|
| nested_data | 1207 s | 5,852 | 4 | 9226 | 26884 | 750 / 2476 B | 595 MB | 2 slow units (fuel-capped) |
| vm_bytecode | 1201 s | 508,924 | 423 | 15146 | 39105 | 2078 / 21 KB | 631 MB | 1 slow unit (F4, fixed) |

- nested_data `9kEH` (`f6 41 07`: `max` on a cyclic list of 489 nested
  dicts, the F1 shape) and `PjUjLw==` (`3e 35 23 2f`: `x < y` on a
  2^8-path DAG over 367 frozensets inside 306 cycles) took 25 s and 32 s
  standalone: each spends the cell's 5M fuel, which the instrumented
  build runs at about 200k fuel/s. Release `mm`, unlimited fuel: `9kEH`
  32.5 s → 0.50 s [CPython 0.88 s]; `PjUjLw==` over 300 s → 20.6 s
  [CPython 266 s]. Not bugs.
- F4, vm_bytecode `igAAAAAASjBzACWjMjUAAABKMHMAJaMyNQ==`
  (`artifacts/vm_bytecode-slow-unit-4272726e`, program 0 with
  five edits), open and also on `6fb33a9`: 34 s in the fuzz binary,
  0.46 s in a plain release copy of the harness for 30k fuel. 99.7% of
  the instructions are `dict_set` → `probe_val` → `scan`, the no-index
  path: the program stores 12,000 str keys into a dict whose object holds
  one value slot before its counts word, so no index slot exists, each
  resize's index write is dropped and the table stays linear. Every store
  then scans all entries (75.7M key hashes for 12k stores) without
  charging fuel. Ordinary dicts are not affected (16,000 str keys: 0.5 µs
  per key). Traced and fixed (`0d171dd`): the edits turn a
  `JUMP_IF_TRUE` 21 into 39 and an `LDA_SMI` 20 into 12308, so execution
  jumps into a dict comprehension's body with a *list* in the register its
  `DICT_INSERT` writes to. The list's single item slot was read as a linear
  dict table that never got an index. `Vm::dv` now returns no dict view
  unless the object is a dict with both its slots, `DICT_INSERT` into a
  non-dict raises `SystemError` (`verified_bytecode_rejects_wrong_container_types`), and a
  genuine linear table holds at most `LINEAR_MAX` entries, so the scan is
  bounded and the shape unreachable. The artifact: 419 ms → 1.8 ms [M].
- With fuel 5.5M now (4.4M before), the F2 input parks at the cell's 5M
  in the harness; release wall time 1.02 s → 0.93 s [CPython 0.83 s].

## 24 h campaign on host1 (2026-11-05/06): snapshot_restore crashes, early triage

The 24 h campaign started 2026-11-05T15:59:58Z at `5568b27` on host1
(Ryzen 9 8945HS, 16 threads, x86-64 Linux). Its full write-up follows when
it ends. This section is published early so the restore-validation fix can
use these crashes as regression seeds. snapshot_restore wrote **11 crashes**
in the first 9 h. All 11 reproduce with the campaign's fuzz binary on host1,
and with a 5568b27 build on the M1 Max Mac (aarch64 macOS), so they do not
depend on the platform [M].

**What the inputs are.** The target's input format is in its doc comment.
Each crash input minimizes (`-minimize_crash=1`) to 6 bytes: a mode byte, a
program pick, and one 4-byte edit. The edit xors one byte of a **valid
snapshot**, which the target takes of one of its six programs at start-up,
and then recomputes the checksum. So each crash is a restore of an image
with one byte changed and a correct checksum. `docs/spec.md` §9.3 and the
`snapshot.rs` module doc say `Vm::read_snapshot` trusts nothing and checks
every pc, so **both are real engine bugs**: no panic on any restored input.

| seed | bytes (hex) | edit | panic at 5568b27 | crashes it stands for |
|---|---|---|---|---|
| `snapshot_restore-crash-pc-5568b27` | `643d521aa4cd` | program 1 (parked in a generator in a method), tail offset 6738, xor 0xa4 | `interp.rs:2564` `w = bc[pc]`: index out of bounds, len 1, index 18445055305453801084 (near 2^64) | 8 (every interp crash hits tail offset 6738 mod 8192, with other xor values) |
| `snapshot_restore-crash-dict-a-5568b27` | `6156062cb200` | program 2 (a host call inside a sort key), head offset | `dict.rs:862` `(w & g.emask) - 1`: subtract with overflow | 1 |
| `snapshot_restore-crash-dict-b-5568b27` | `8a0000e13333` | program 0 (idle, classes and dicts in globals) | `dict.rs:862` | 1 |
| `snapshot_restore-crash-dict-c-5568b27` | `72d69efd2a92` | program 4 (recursion through dunders) | `dict.rs:862` | 1 |

**On the current head (c97ce06).** None of the 11 inputs crashes on
c97ce06, and they stop at 220a05f. That commit fixes nothing here. It adds
an 8-byte `fuel_wanted` field to the image, which moves every offset the
edits hit, and the speed landing changed the images further. So I scanned
the same edit over every position on a c97ce06 build (Mac): tail offsets
6400–6899 of program 1; every tail offset of programs 0, 2 and 4 (no hits);
and their head offsets with each seed's xor value (that scan found the hits
below; it was not run to completion). **Both bugs reproduce on c97ce06** [M]:

| seed (c97ce06) | bytes (hex) | panic |
|---|---|---|
| `snapshot_restore-crash-pc-c97ce06` | `643d2c1a28cd` | `interp.rs:2681` `w = bc[pc]`, the same index 18445055305453801084 (program 1, tail offset 6700, 38 bytes before the old 6738) |
| `snapshot_restore-crash-dict-c97ce06` | `01020e91b200` | `dict.rs:876` `(w & g.emask) - 1`: subtract with overflow (program 2, head offset) |
| `snapshot_restore-crash-dict-p4-c97ce06` | `0104441b2a00` | `dict.rs:876` (program 4, head offset; `010462d02a00` is another) |

- **pc.** A restored pc lands outside its code. The index is near 2^64, the
  same at both heads, so it is probably a pc read from an unchecked word
  (a frame or continuation return pc, or a generator's yield pc) rather
  than `RawState::pc`, which `check_state` validates with `is_instr_start`.
  This is a hypothesis; I have not traced which field the byte belongs to.
- **dict.** A restored dict index slot has a matching tag but entry bits 0.
  Overflow checks (the fuzz build) make it panic. A release build wraps to
  a huge entry index, which `kvs.get` then misses, so the lookup goes wrong
  silently instead. The decoder does not check index slots against the
  entry count.
- **Possibly related.** The same head-mode scan of program 4 also found an
  out-of-memory, over libFuzzer's 2 GiB RSS limit: `0104bdee2a00` at
  c97ce06. The campaign wrote 9 snapshot_restore OOMs. They are triaged
  with the rest of the campaign.

**Root causes and fix (2026-11-06, x86-64 Linux [M]).**

- **pc.** Not a frame, continuation or `RawState` pc. The edited byte is
  the kind byte of a reference in the VM part (program 1's suspended
  state `coro`): a `GEN`-kind value that names a non-generator object.
  The engine reads `g::YPC` from whatever object a `GEN` value names, and
  `check_generators` only checked real generator objects. Restore now
  requires every reference, in the VM part and in every arena value slot,
  to name an object of its own kind (`Corrupt("references")`), and checks
  continuation registers and the bottom frame's base.
- **dict.** Restore did not check index slots. It now checks every dict's
  index table: power-of-two capacity, entry bits in `1..=used` with no
  duplicates, a tag matching the entry's hash, every live entry reachable
  on its probe sequence, the live count equal to `len`
  (`Corrupt("dict index")`). `010022563300` (program 0) is the same bug.
- **OOM.** `0104bdee2a00` and `01049f392a00` edit a class's size hint (a
  raw word, never above 32 in an honest image). Creating an instance then
  built a native buffer of that many slots, 5.6 GB, before the arena could
  refuse it. Instances now clamp the hint to the 32 inline slots; the
  restore decoder itself already caps every preallocation by the bytes
  left and charges it to the commit budget. Whether the campaign's 9 OOMs
  are this class is not known: they do not reproduce on c97ce06 (offsets
  moved) and were never decoded.
- `crates/micro-monty/tests/snapshots.rs`
  (`fuzzed_images_restore_to_errors_not_panics`) replays every
  `snapshot_restore-*` seed here plus the four hex seeds above through the
  fuzz target's own input code (`fuzz/fuzz_targets/snapshot_restore_input.rs`),
  failing on any panic (the facade would contain it) or on peak memory
  growing by 512 MiB. On the unfixed engine it reports 5 panics and
  5.4 GB growth for each OOM seed.
- Scans with the fixes found no panic or abort (4 GB address limit): every
  byte of programs 0, 1 and 4 with xor 0x2a, 0x01, 0x80; every byte of
  programs 2, 3 and 5 with xor 0x2a, 0x01, 0x80, 0xff; the last 8 KiB of all
  six with xor 0x02, 0x10, 0x40, 0xaa. About 2.0M restores in all; a 15 min
  libFuzzer fork run (6 jobs) on top.

Both the seeds and the panics above were measured on aarch64. Snapshots are
architecture-independent (`bench/evidence/isolation/results/2026-11-05-crossarch-v1/`),
and the 5568b27 crashes behave the same on x86-64 Linux.

```bash
cd fuzz && cargo +nightly-2026-11-01 fuzz build snapshot_restore
target/<host-triple>/release/snapshot_restore ../conformance/breadth/fuzz/artifacts/snapshot_restore-crash-pc-c97ce06
```

### Replay on ce45914 (restore hardening, c8e69b2..ce45914)

[M] All of this ran on the Mac, aarch64, with cargo-fuzz's default build
(release, AddressSanitizer) of `snapshot_restore` and `vm_bytecode` at
ce45914. Every replay was checked to report `Executed`.

- **snapshot_restore: fixed.** The campaign's **13 crashes and 10 OOMs** so
  far, the 7 seeds above and the 6 sweep hits all restore to errors with no
  panic or OOM. So does a fresh sweep of one-byte edits on ce45914: 1,000
  tail edits of program 1, 24,576 tail edits and 196,602 head edits of
  programs 0, 2 and 4.
- **The 10 OOMs are one class.** At 5568b27, with `-malloc_limit_mb=2048`,
  every one fails in `Vm::new_instance` (from `alloc_of_class` and
  `call_class`) asking for 5.4–34 GB. Every input edits program 4 (classes
  with `__add__` and `__enter__`), at the class size hint that sizes a new
  instance's native Vec. ce45914 clamps that hint.
- **vm_bytecode: the 11 timeouts still reproduce on ce45914. One cause, a
  real performance bug, not a crash.** Each one times out at libFuzzer's 60 s,
  at 5568b27 and at ce45914 alike. Without the sanitizer
  (`cargo fuzz build -s none`) each still takes 2.4 s (9 inputs) or 10 s
  (2 inputs) for a fuel-bounded cell. Every stack, sampled at 8 s, is in
  `text::ReprWriter::reserve` under `push_str`, reached from
  `Vm::repr_into` or `ReprWriter::escape`/`quoted` (repr of a str), through
  `Vm::stringify` and `Vm::format`. A likely cause [E], not verified with a
  Python-level repro: `reserve` falls back to growing to exactly `needed`
  when doubling would exceed `heap.available_bytes()`. Close to the memory
  limit, each push then copies the whole buffer, so the repr becomes O(n²)
  in time, and fuel does not see it. Seeds:
  `vm_bytecode-timeout-repr-13fd8ca2` (10 s through `repr_into`) and
  `vm_bytecode-timeout-escape-17f52abd` (2.4 s through `escape`).
- **Cause confirmed and fixed (2026-11-06, x86-64 Linux [M]).** With exact
  growth, `repr` of an escape-heavy list in a 16 MiB session grew its buffer
  929k times in one cell (about 12 TB of nominal copies). glibc's `realloc`
  moves large blocks with `mremap`, so Linux barely shows it; macOS and ASan
  copy. Four builders had the fallback: `ReprWriter`, `NativeText`,
  `NativeVec<u8>` and json's `Out`. All now share `scratch::grown`: double, or
  else take everything the budget has left, at most once. The growth after
  that fails, so writes stay amortized O(n) up to the limit. Which sizes fit
  is unchanged for repr, str, format, `%`, json, join and hex. Repr text now
  also costs fuel, one unit per 64 bytes, as the str methods charge. With
  both changes, under ASan the escape seed takes 0.95 s (it was 14.6 s), and
  0.03 s in release.
- **The repr seed: bounded by fuel (2026-11-06 [M]).** It was not
  quadratic: one `repr` of shared containers (an f-string) walked 104M items
  to 62 MB of text in one native op, 69 s under ASan and 1.9 s in release.
  With fuel limited, the native walk now visits at most the fuel left and
  then hands the rest to the prelude twin, which parks on fuel; the seed's
  cell parks within its 20,000 units (11 ms in a debug build). A completion
  `repr` after a cell cannot run Python, so it stops at what the text cut
  keeps. `tests/session.rs` replays both seeds through the target's own
  input code, `fuzz/fuzz_targets/vm_bytecode_input.rs`.
- **Fixed at bb34446** (the repr growth fix eab794a/863beb1, the bounded repr
  walk a20e801/59f0477 and the 1 MiB default text limit). On the Mac [M],
  all 11 vm_bytecode timeouts finish, each reporting `Executed`: 13–758 ms
  under ASan (the repr seed 18 ms, the escape seed 403 ms) and 3–239 ms
  without it (4 ms and 106 ms), against more than 60 s and 2.4–10 s at
  ce45914.

## 24 h campaign on host1: final results and triage (2026-11-06)

[M] host1 (Ryzen 9 8945HS, 16 threads, x86-64 Linux, bare metal), unless a
line names the Mac (M1 Max, aarch64 macOS 27.2). The campaign ran
2026-11-05T15:59:58Z to 2026-11-06T16:03:56Z at `5568b27`. Seven
libFuzzer targets ran in fork mode with 2 workers each, using `-timeout=60`,
`-ignore_crashes/timeouts/ooms=1` and `-max_total_time=86400`, plus
`cargo xtask difffuzz --jobs 2 --start 5000000`. All 14 threads were busy;
the script is the one in the section above. Every target exited 0.

| target | execs | cov | artifacts |
|---|---|---|---|
| arena_ops | 76.99M | 2037 | none |
| json_loads | 100.5M | 7990 | none |
| nested_data | 99k | 9414 | 169 slow units |
| parse | 276.4M | 6793 | 1 crash (ASan stack overflow) |
| re | 11.4M | 10548 | none |
| snapshot_restore | 48.8M | 21376 | 24 crashes, 14 OOMs, 1 timeout |
| vm_bytecode | 40.4M | 17868 | 30 timeouts, 48 slow units |
| difffuzz | 1,152,168 programs | | 1,146,351 same, 327 divergent (131 signatures), 5,490 skipped (CPython timeout); 164 files kept |

### Replay on 5349520

Every artifact was replayed with two cargo-fuzz builds at `5349520`
(current main, after the restore hardening, the repr fixes and the 1 MiB
text limit): the default ASan build and `-s none`. Both used
`-rss_limit_mb=2048 -malloc_limit_mb=2048 -timeout=60`, and every run was
checked to report `Executed`.

- **snapshot_restore: all 39 fixed.** On the campaign's 5568b27 binary the
  24 crashes are the two known classes: 20 at `interp.rs:2564` (pc) and
  4 at `dict.rs:862` (dict index). All 14 OOMs fail in `Vm::new_instance`,
  asking for 5.4–34 GB. That is the class-size-hint class, which also covers
  the 4 OOMs written after the earlier decode. The timeout also
  reproduces at 5568b27. On 5349520 all 39 restore to errors, at most
  184 ms each under ASan.
- **vm_bytecode timeouts: all 30 fixed.** They take at most 2.7 s under
  ASan and 0.5 s without it. At 5568b27 they exceeded 60 s; they are the
  repr class fixed at bb34446.
- **nested_data slow units: not bugs.** The 169 inputs, 3–4 bytes each,
  take 4.6–5.0 s without a sanitizer and 39–41 s under ASan. A plain
  release build of the target's program on the Mac finishes each in
  0.22–0.48 s: 62 complete, and 107 stop at the harness's 5 M fuel cap.
  They are F1's fuel-bounded slow path, which sanitizer and coverage
  overhead make slower.
- **parse: harness, not engine (P1).** See below.
- **vm_bytecode slow units: a real performance and fuel bug (N1).** 24 of
  the 48 still take more than 10 s under ASan on 5349520 (21–23 s; 1.4–1.6 s
  without a sanitizer when run alone). See below.

### N1: every NameError computes its "Did you mean" hint eagerly; fuel does not cover it

- Seed: `artifacts/vm_bytecode-slow-namehint-7a9ffa8d-min`, 9 bytes,
  minimized with `-minimize_crash=1 -timeout=4` from
  `fuzz/artifacts/vm_bytecode/slow-unit-7a9ffa8d82443695d0f64451eeb021b99c6be868`
  (26 bytes).
- Stacks: gdb samples of all 24 slow units, taken without a sanitizer,
  are in `exc::levenshtein` ← `exc::suggest` ←
  `Vm::set_name_hint` ← `load_global_slow`. `set_name_hint` copies every
  global, builtin and prelude name into owned `String`s. Then it runs
  CPython's Levenshtein suggestion over all of them, collecting each name
  into a `Vec<char>`, on **every** NameError raised. CPython computes the
  suggestion lazily, only when a traceback is printed. A program that
  catches NameError pays for nothing.
- Python repro (Mac, release `mm` at 5349520 vs CPython 3.14.7):

  ```python
  for i in range(200000):
      try:
          undefined_name_here
      except NameError:
          pass
  ```

  mm 8.35 s, CPython 0.06 s: about 140×, or 42 µs per NameError. The same loop
  with `raise KeyError` takes 0.033 s on both.
- Fuel: `mm exec --json --report-fuel` charges the NameError loop
  199,940 units, about 1 per iteration. It charges the KeyError loop
  399,876 units in 0.032 s. Wall time per fuel unit is therefore about
  500× higher, so a fuel budget does not bound wall time for a cell that
  keeps raising NameError. The cost also grows with the number of globals.
- Classification: a real engine bug, in performance and fuel accounting,
  not a crash.
- **Fixed (2026-11-06 [M]).** NameError, AttributeError (instances,
  classes, builtin types, so `hasattr` misses too) and missing module
  attributes now record a pending hint: the name, the globals dict and the
  frame's bound local names. It is computed only when the report renders
  (a traceback, `traceback.format_exception_only`, the host's error) and
  charged one fuel unit per candidate name. The messages are unchanged,
  including after a snapshot and restore of an unrendered error. The loop
  above takes 0.09 s in release on x86-64 Linux (CPython 0.07 s). The seed
  and `a_caught_name_error_costs_no_hint_until_it_is_rendered` are
  regression tests in `crates/micro-monty/tests/session.rs`.

### N2: mm accepts NUL bytes in source (difffuzz)

```bash
printf '# a\0b\nprint(1)\n' > nul.py
python3.14 nul.py   # SyntaxError: source code cannot contain null bytes
mm run nul.py       # prints 1
```

The same holds for a NUL inside a string literal. CPython 3.14 rejects any
NUL byte in source; mm parses it. This is a small, real divergence, and
not a crash. It surfaced through a difffuzz artifact. When CPython's output
contained `\x00`, the fuzzer copied the raw output into the saved file's
`# CPython:` header comment, so the saved `seed-5206420.py` is invalid
for CPython but not for mm. That header write is also a difffuzz tooling bug.
The program as generated diverges only through the huge power below.

**Fixed (2026-11-06 [M]).** mm now raises CPython's `SyntaxError: source
code cannot contain null bytes` on the null byte's line, with the text up
to it and no caret. Like CPython's file tokenizer, a tokenizer error on an
earlier line still comes first, and every later error and parser error
comes after. ruff accepts a null byte in a comment or string, a recorded
parse-diff difference. difffuzz escapes non-printable characters in the
header comments of the files it saves.

### P1: parse, ASan stack overflow at 35 nested parentheses (harness)

- Seed: `artifacts/parse-asan-stack-overflow-d5d27626`, 154 bytes. Mode 3
  gives 110 nested `if` blocks, and the body has about 110 `(`. The
  overflow is at the 35th paren level.
- Under ASan the overflow reproduces on 5349520. Without a sanitizer it
  runs cleanly at 5568b27 and 5349520. Each paren level is seven recursive
  frames (`paren` → `named_expression` → `expression` → `pratt` → `power`
  → `primary` → `atom`). ASan's redzones make those frames about 10× larger
  [E], so 262 frames overflow the target's 1 MiB thread.
- On the Mac, a scratch release build (opt-level 3) of `mm_parse::parse` on
  `x = (((…1…)))` parses 200 levels, the prescan limit, on a 512 KiB
  thread. It rejects 201 levels with CPython's `too many nested
  parentheses` on threads of 256 KiB and up. 200 levels overflow a 256 KiB
  thread. That matches docs/sdk.md, which asks for 1 MiB on threads that
  compile and measures 650 KiB.
- Classification: the fuzz harness's 1 MiB thread is too small for ASan
  frames. It is not an engine bug for shipped builds.
- **Fixed (2026-11-06 [M]).** The harness thread is 8 MiB
  (`fuzz/fuzz_targets/parse_input.rs`, shared with the regression test
  `parse_seeds_fit_the_fuzz_targets_stack`). The seed runs in 3 ms under
  ASan. A debug build also overflowed 1 MiB on it.

### difffuzz: the 164 kept divergences on 5349520

Each was replayed with `mm` 5349520 against CPython 3.14.7 on host1, then
re-checked on the Mac.

| class | programs | verdict |
|---|---|---|
| fixed since 5568b27 (now identical) | 21 | `round(float, -n)` (ce45914) and others |
| huge power `2 ** 64 ** k` (2**2^30) raises MemoryError in mm | 128 | documented exclusion. The output differs downstream, in later lines, `finally` output and int-str-limit errors. Replacing `2 ** 64 **` with `2 ** 2 **` makes 126 of them identical; the other two are the next two rows. |
| `<` operand order in a TypeError from `sorted`/`max` | 4 (one also a huge power) | known, documented in `conformance/breadth/difffuzz/sort-comparison-order.py`; the smallest repro is `sorted([1, 2, 'a', 0])` |
| float last digit, exactly 1 ulp | 10 | 5 use `**`. On the Mac, CPython agrees with mm; host1's glibc `pow` is 1 ulp off (for `147 ** -3`, mm is the correctly rounded one). 5 use `math.sin/log2/exp/sqrt` and differ by 1 ulp on both platforms; the new 1-ulp rule covers them. mm's shortest repr round-trips in every case. |
| output past the memory limit | 2 | a list of 14 M one-character strings (`seed-5366320`) and 872 MB of printed doubling (`seed-5386263`): limits, not bugs |
| NUL bytes in source | 1 | N2 (also a huge power) |

## Targeted fuzz and soak on host1 at 357e003 (2026-11-07)

A short campaign, not the release campaign. The owner keeps the full
24 h run for the last step before 0.0.1. It was aimed at the code added
since the last campaign: `except*`, asyncio batch 3, SyntaxError precedence
and the tokenizer, the decoded-module cache (1c03cff) and the import pass
(b01fd7d). Engine crates at 357e003. 9671394 (snapshot format 5) and
8507ad2 (`class::flagged`) landed after it and were not fuzzed here.

**Result: no engine crash, panic, sanitizer report, timeout or
cached/uncached divergence.** The soak had no differing job. The one
finding (R1) is harness-side.

### Setup

- Same toolchain and cargo-fuzz defaults as above: ASan, debug assertions,
  `-timeout=60 -rss_limit_mb=2560`. 75 minutes per target
  (17:25–18:41Z), all five at once, alongside the soak. Nothing else ran on
  the machine.
- **New target `run_source`** (`fuzz_targets/run_source.rs`). It takes
  `parse`'s input and runs the source end to end: imports, asyncio and
  exceptions, in 40,000-fuel slices. It runs once on a shared `Engine`
  with the default module cache and once on an uncached one
  (`Engine::with_module_cache(_, 0)`), and requires the same transcript from
  both: output, every stop and the end. A difference is excused only when it
  can come from addresses (`id(`, `hash(`, ` at 0x`) or a `MemoryError`.
  - Seeds: the 223 own cases, the 1,785 codemode solutions of at most
    3,500 bytes that import, use `except*`, `async` or `await`, and one
    import cell per bundled module (25).
  - It ran with `-fork=4`. `compile` ran with `-fork=2`, on the `parse`
    corpus plus the `run_source` seeds.

| target | executions | exec/s | cov | ft | corpus | findings |
|---|---:|---:|---:|---:|---:|---|
| run_source | 3,865,232 | 177–235 | 35,758 | 130,230 | 6,242 | 1 OOM (R1) |
| compile | 7,694,559 | 972–1,407 | 12,637 | 61,268 | 9,320 | none |
| parse | 4,918,094 | 1,092 | 6,903 | 33,098 | 8,745 | none |
| vm_bytecode | 808,625 | 179 | 18,616 | 71,441 | 5,487 | none |
| snapshot_restore | 1,242,602 | 276 | 18,736 | 44,307 | 2,235 | none |

[M] Every count comes from the target's last status line. Seeds:
run_source 481325763, compile 488578560, parse 467311223, vm_bytecode
462473352, snapshot_restore 478394258.

### R1: run_source, a 23.5M-bit `**` under ASan (harness)

- Input: `artifacts/run_source-oom-asan-bigpow-11c5a481` (302 bytes: a
  tuple of big-int arithmetic). Minimized by hand to
  `artifacts/run_source-oom-asan-bigpow-11c5a481-min`:
  `x = <a 71-digit int>**99919`.
- The facade does what its contract says.
  - The power wants 9.98 G fuel, so the session parks before starting it
    (`fuel_wanted` = 9,983,806,625).
  - Resuming then runs it whatever fuel is set (`Session::fuel_wanted`'s
    doc). It is charged 8.48 G fuel (`mm exec --report-fuel`).
  - The target resumed regardless, and so ran the power on both engines.
- Cost [M]:

  | run | time | peak RSS |
  |---|---:|---:|
  | `mm run`, release | 5.0 s | 35 MB |
  | CPython 3.14 | 2.6 s | |
  | the same harness, no sanitizer, debug assertions | 61 s | 56 MB |
  | the ASan harness | | over 2.5 GB (OOM) |
  | ASan, with `quarantine_size_mb=0` and `allocator_release_to_os_interval_ms=0` | | over 6 GB |

  So the growth is ASan's per-allocation overhead on the power's large,
  short-lived buffers, not a leak in the engine. `repr` of the result
  raises CPython's 4,300-digit `ValueError` on both engines.
- Classification: harness. `run_source` now stops at a park whose
  `fuel_wanted` exceeds a slice, as a host that enforces a budget would.
  The input then runs in 7 ms under ASan.
- Side note: this power takes 1.9× CPython's time. That is a performance
  observation, not a bug.

### Soak (bench/evidence/soak, same window)

75 minutes, 40 sessions in flight. Native: one shared `Engine`, 4 threads.
mm-wasm: one instance per session, 3 threads. Guest built at 357e003 [M].

| | jobs | same | differ | memory-edge | parks | snapshots | migrations | RSS at 8 min → end | high-water | cache bytes |
|---|---:|---:|---:|---:|---:|---:|---:|---|---:|---:|
| native | 110,474 | 109,764 | 0 | 707 | 27.5 M | 10.2 M | 4.6 M | 248 → 259 MB | 259 MB | 3,386,576 of 8,388,608 |
| mm-wasm | 126,387 | 126,050 | 0 | 335 | 6.8 M | 2.7 M | 2.3 M | 182 → 167 MB | 229 MB | (per instance) |

- No job differed from its schedule replayed on an uncached engine, or from
  its unparked run. There were no refused snapshots, failed restores or
  wasm traps. Native skipped 3 items.
- **No RSS growth to speak of.**
  - Native rises 4% over the last 67 minutes, in two steps (at 23 and
    45 min), then stays flat for 30 minutes.
  - wasm moves between 150 and 190 MB with the guest memory queued, and
    its high-water mark has not moved since minute 23.
- **The cache's charged bytes** reach 3,386,576 B by minute 8 and stay
  there. Every bundled module is decoded once, well under the 8 MiB cap.

### Reproducing

```bash
cd fuzz
cargo +nightly-2026-11-01 fuzz run run_source corpus/run_source -- \
    -max_total_time=4500 -timeout=60 -rss_limit_mb=2560 -fork=4 -ignore_ooms=1
target/soak/release/soak WORKLOAD OUT --minutes 75 --threads 4 --inflight 40
target/soak-wasm/release/soak-wasm GUEST WORKLOAD OUT --minutes 75 --threads 3 --inflight 40
```

## Stable-parts campaign in an orb (2026-11-07/08)

[M] An Amp orb (16 vCPU, 31 GB, x86-64 Linux), 2026-11-07T04:23Z to
2026-11-08T05:27Z: 25.1 h of wall time, about 24 h of fuzzing (the orb
paused for about 50 min in the first hour, before a keep-alive lease held
it). Scope: the parts that change least, which are the parser and compiler, snapshot
decode and restore validation, and the mm-wasm-abi codec. libFuzzer fork
mode with `-ignore_crashes/timeouts/ooms=1 -rss_limit_mb=2048
-malloc_limit_mb=2048`; two builds per target sharing one corpus:
cargo-fuzz's ASan build (`-O -a`, debug assertions and overflow checks,
`-timeout=60`) and a no-sanitizer release build for slow units
(`-s none`, `-timeout=20 -report_slow_units=2`). 14 workers on 16 cores.
Every few hours: rebase on `origin/main`, rebuild, restart with the
corpora kept; nine rebuilds in all, following main from `1068b7a` to
`75640b1`, through mm-wasm v2 (`4ba8f65`) and snapshot format 5
(`9671394`). `parse`/`compile` corpora started from the ~950 repository
Python files, `snapshot_restore` from the earlier seeds.

New and changed targets:

- `compile` (`a2e5d78`): `Engine::compile` on the `parse` target's
  inputs. Errors are Python exception types; what compiles passes
  `Code::verify` and survives `Program::encode`/`decode` byte for byte.
- `wasm_abi` (`a2e5d78`, v2 in `11d93a3`): every mm-wasm decoder on
  arbitrary bytes; what decodes re-encodes to the same bytes.
- `snapshot_restore`: edits can insert and delete bytes (`a536dea`); a
  session asking for more than 2^20 fuel is not resumed (`e3eb214`, S1);
  typed host modules give the images `CODE_HOST` entries, and mode bits
  restore on an engine without a module cache or edit images that store
  their host code (`7474ff6`).

| target | execs (ASan / none) | final cov (ASan / none) | final ft (ASan / none) |
|---|---|---|---|
| compile | 127.4M / 166.0M | 13,837 / 9,226 | 82,613 / 54,160 |
| parse | 151.5M / 263.1M | 7,929 / 5,305 | 44,292 / 29,662 |
| snapshot_restore | 113.0M / 215.4M | 26,312 / 18,321 | 76,141 / 50,385 |
| wasm_abi | 1,943.2M (ASan only) | 2,288 (v2 codec; v1 stopped at 2,205) | 8,349 |

Coverage at the stop: `wasm_abi` flat since about 16:30Z on 11-07,
`compile` and `parse` growing by single units an hour, `snapshot_restore`
by a few hundred an hour (its harness changed at 19:32Z). Corpora at the
stop: compile 17,113 files, parse 14,453, snapshot_restore 21,187,
wasm_abi 2,442 (fork mode keeps superseded inputs; merge before reuse).

### Findings

- **F1: the parser overflowed the Rust stack on guest source** (`compile`,
  19 ASan crashes). The parser recurses once per bracket and relied on the
  pre-scan to stop at 200; the pre-scan read three shapes differently from
  the tokenizer (a comment ended by a lone `\r`, `\'` in a raw string, `\{`
  in an f-string), and brackets behind them went uncounted: 3,000 needed
  7.2 MiB in a release build. The tokenizer now raises CPython's `too many
  nested parentheses` at the 201st open bracket or field (`168b7ac`).
  Seed `compile-stack-overflow-cr-comment-1a670fb6`; tests
  `brackets_the_pre_scan_misreads_are_still_limited`,
  `parse_seeds_fit_the_fuzz_targets_stack`.
- **F2: restore panicked on a `CODE_HOST` offset past its code index**
  (`snapshot_restore`, 4 crashes within an hour of format 5). The
  contiguity check computed `ci - at` with `at` from the image: a panic
  with overflow checks; release wrapped and refused the image anyway.
  Now `ci.checked_sub(at)` (`75640b1`). Seed
  `snapshot_restore-crash-codehost-offset-9671394`, replayed by
  `fuzzed_images_restore_to_errors_not_panics`.
- **S1: 11 slow units, by design.** After a `fuel_short` park the next
  `resume` runs the int operation whatever the fuel (spec §5); one
  `3 ** 3_000_000` took 0.86 s. Harness change only (`e3eb214`).
- **S2: a debug assertion made code loading O(N²)** (`snapshot_restore`
  slow unit, 80.8 s under ASan, 0.68 s without debug assertions).
  `Vm::reserve_code` re-walked every loaded code on each load; it now does
  the full walk only at power-of-two code counts (`d2912a2`, 10.3 s).
  Seed `snapshot_restore-slow-codesums-574dda4` (format 4; kept for the
  record).
- Not findings: 13 slow units and 11 timeouts written in two bursts as the
  orb resumed from a pause (each replays in at most 1.3 s under ASan).

### Open

- A file whose lines end in a lone `\r` reports SyntaxError line numbers
  that differ from CPython's (`crates/mm-parse/src/lines.rs`). No crash.
  **Fixed** (2026-11-08, HANDOFF "Pre-0.1 follow-ups"): a lone `\r` ends
  a line in every position and source-line lookup, as in CPython.
- On an Engine whose cache holds an image's host-code key, restore takes
  the cached program without regenerating it from the image's definition,
  so an image with an edited definition and an unchanged key restores
  with the cached code; only an uncached Engine refuses it. Not a crash
  or safety issue; the definition the image reports can differ from the
  code that runs. **Fixed** (2026-11-08, HANDOFF "Pre-0.1 follow-ups"):
  the cache keeps a hash of each program's definition, and a restore
  whose definition hashes differently builds the code from it as an
  uncached Engine does; mode bit 5 asserts both engines' verdicts agree.

Run scripts (orb-local, not committed): `build.sh` installs both builds
per revision, `run.sh <target> <asan|none> <workers>` runs one job as an
`amp orb service`, with the flags above.

## 0.0.1 release campaign on host1 (2026-11-08/10): runs 1–3

[M] host1 (bare metal, 16 threads), libFuzzer fork mode with the flags
above (`-timeout=60 -rss_limit_mb=2560`, crashes, timeouts and OOMs kept
going), 15 workers as systemd units: `snapshot_restore`, `run_source` and
`vm_bytecode` with 3 ASan workers each; one each for `snapshot_restore`
and `run_source` without a sanitizer, `parse`, `compile` and `wasm_abi`;
and one rotating slot (`arena_ops`, `json_loads`, `nested_data`, `re`, a
quarter of the run each). Corpora: this repository's seeds, the earlier
host1 corpora and the orb's stable-parts corpora, merged with `-merge=1`
before each run. Run 3 also seeded every `conformance/own` case into
`run_source`, `parse` and `compile`.

Three release candidates were fuzzed in turn. Runs 1 and 2 stopped when
a finding's fix moved the candidate; run 3 ran its full 24 h. The owner
then held 0.0.1 until every native step can park inside it (bounded
native steps, with C4 and C5 below). So none of these hours are on the
final release commit: the 24 h release run restarts on the bounded-steps
commit.

| run | commit | window (UTC) | hours | executions |
|---|---|---|---:|---:|
| 1 | `1a1e102` | 11-08 05:40:25 – 15:19:25 | 9.65 | 976M |
| 2 | `10cf9f6` (C2 fix) | 11-08 15:26:45 – 23:58:29 | 8.53 | 844M |
| 3 | `5df122e` (C3, fuel audit) | 11-09 00:04:06 – 11-10 00:04:06 | 24.00 | 2,329M |
| total | | | 42.18 | 4,149M |

Per target and run [M]: executions, final coverage (`cov`) and corpus
size. Coverage counts are libFuzzer's edges, comparable within a target
and build only.

| target | build | run 1 | run 2 | run 3 | executions |
|---|---|---|---|---|---:|
| `run_source` | ASan | 11.1M, 38,469, 9,001 | 6.0M, 39,678, 9,547 | 14.5M, 44,146, 12,868 | 31.6M |
| `run_source` | none | 12.0M, 26,889, 8,673 | 7.2M, 27,962, 9,336 | 13.0M, 31,328, 12,129 | 32.2M |
| `snapshot_restore` | ASan | 28.7M, 28,423, 5,127 | 26.1M, 29,099, 5,409 | 68.2M, 29,217, 6,038 | 123.0M |
| `snapshot_restore` | none | 40.5M, 20,091, 5,065 | 36.9M, 20,702, 5,538 | 95.7M, 20,941, 6,057 | 173.0M |
| `vm_bytecode` | ASan | 30.2M, 19,220, 6,805 | 26.5M, 19,511, 6,884 | 80.1M, 19,954, 7,885 | 136.8M |
| `compile` | ASan | 13.1M, 13,993, 12,942 | 13.2M, 14,026, 13,589 | 36.8M, 14,109, 13,628 | 63.1M |
| `parse` | ASan | 25.5M, 8,002, 11,370 | 22.4M, 7,995, 10,671 | 62.1M, 8,073, 10,835 | 110.1M |
| `wasm_abi` | ASan | 801M, 2,288, 1,630 | 694M, 2,316, 1,604 | 1,939M, 2,316, 1,618 | 3,434M |
| `arena_ops` | ASan | 7.9M, 2,058, 2,593 | 7.2M, 2,058, 2,351 | 7.7M, 2,075, 2,343 | 22.7M |
| `json_loads` | ASan | 6.2M, 8,492, 2,195 | 4.4M, 8,493, 1,967 | 10.7M, 8,584, 1,927 | 21.4M |
| `nested_data` | ASan | — | — | 6,242, 10,404, 705 | 6,242 |
| `re` | ASan | — | — | 0.88M, 11,378, 2,711 | 0.88M |

`run_source` and `snapshot_restore` still grew through run 3 (+4,468
and +118 edges under ASan); `wasm_abi` has been flat at 2,316 since run 2.
`nested_data` runs about 0.3 inputs a second under ASan by design (it
builds containers ~60,000 deep and runs each to a 5M-fuel cap).

### Findings

Fixed during the campaign, each with a regression test and seeds under
`artifacts/`:

- **C1: indexing a long non-ASCII `str` was nearly free in fuel**
  (`run_source` slow units, four duplicates). `s[i]` walks the UTF-8 to
  character `i` but charged a constant. It now charges for the walk and no
  longer checks the whole string's UTF-8 (`46c2d90`). Indexing stays O(i),
  against CPython's O(1). Seeds `run_source-slow-strindex-58d44444`
  (and `-min`); test `indexing_a_long_non_ascii_str_charges_fuel_for_the_walk`.
- **C2: rendering an uncaught exception was unbounded** (`run_source`
  timeout, 196 s under ASan). A 75-byte cell keyed a dict with a frozenset
  that shares itself 24 levels deep; the `KeyError`'s message and
  traceback rendered its 2^24-fold `repr` outside fuel and the deadline
  (9.47 s and 267 MB in release `mm`). The report now stops at the text
  limit and the deadline and sets `PyException::truncated` (HANDOFF
  decision 61; 0.04 s; `10cf9f6`). Seeds `run_source-timeout-excrender-34f25cd9`
  (and `-min`, which starts with the target's control byte); test
  `an_uncaught_exception_report_stops_at_the_text_limit_and_the_deadline`.
- **C3: membership scans were free in fuel, and `in` on an iterator was
  unbounded** (`vm_bytecode` slow unit on `10cf9f6`, 33 s without a
  sanitizer). `x in list` (and `count`, `index`, `remove`, `in`
  `dict.values()`) compared item by item for no fuel: a 20,000-fuel slice
  of `1 in x` on a 4M-item list ran past 90 s. `x in iterator` ran
  natively to the end, so `-1 in iter(range(10**12))` ignored fuel,
  deadlines and cancel. Scans now charge one unit per item compared, and
  `in` on an iterator hands off to the prelude's `_op_contains` after
  `WALK` items (spec §5; `d490d2b`). Seeds `vm_bytecode-slow-in-08c658ac`
  (and `-min`); test `membership_scans_charge_fuel_and_an_endless_iterator_parks`.

Found in run 3, handed to the bounded-native-steps work (thread
T-01a1bcc6), which is the only work landing engine changes before 0.0.1:

- **C4: straight-line code had no safepoint.** Safepoints were backward
  jumps, calls and returns only, so a cell without a loop never checked
  fuel or the deadline, however much each statement did. Found by
  following up `run_source` slow units with long loop bodies. Repro:
  two 10 MB strings, then `b = s == t` 20,000 times, 220 KB of source at
  default limits. Seed `run_source-slow-straightline-c4` (2,000
  statements), written by `artifacts/gen-c4-straightline.py`. On
  `5df122e` [M]:

  | cell | limit | result |
  |---|---|---|
  | 20,000 × `b = s == t` | 50 ms deadline | `Complete` after 23.2 s |
  | 20,000 × `b = s < t` | 50 ms deadline | `Complete` after 11.9 s |
  | 20,000 × `b = "b" in s` | 50 ms deadline | `Complete` after 15.2 s |
  | 2,000 × `b = s == t` | fuel 100 | `Complete` after 2.36 s, not parked |

  Fixed by the compiler's safepoint checks in straight-line code
  (`538452d`, `9767bc3`): on `c0e4045`, main at the end of run 3, the
  2,000-statement cell parks at the 50 ms deadline (51 ms) and, with fuel
  100, after 7 ms [M].
- **C5: one big-int `*` or `**` is a single native step.** Fuel catches
  it (the power parks before it starts when `fuel_wanted` exceeds the
  budget; R1), but the deadline cannot stop it. Found from `vm_bytecode`
  slow unit `6971f080` (44 bytes; one 5,000-fuel resume took 427 ms, 69%
  in `bigint::mag_mul`); 15 `vm_bytecode` slow units in all and two of
  run 3's ASan OOMs (below). With default limits, no fuel limit and a 50 ms deadline [M]:

  | cell | `5df122e` | `c0e4045` | CPython 3.14 |
  |---|---:|---:|---:|
  | `x = 3 ** (2 ** 22)` (812 KiB) | 1.06 s | 0.56 s | 0.61 s |
  | then `y = x * x` | 3.27 s total | parks after the power | 1.21 s for `x * x` |
  | `3 ** (2 ** 23)` | | 1.69 s | |

  One squaring took 2.2 s at 812 KiB, 6.7 s at 1.6 MiB and 20.1 s at
  3.2 MiB on `5df122e` [M], roughly 3× per doubling, so near 60 s at
  6.5 MiB, still under the 256 MiB memory limit [E]. On `c0e4045`
  multiplication is about 4× faster but still one step: a squaring loop
  under a 1 s deadline returns after 1.69 s [M]. Open.

Harness items (fixed in the harness, no engine change):

- **H1:** `vm_bytecode` rendered completions uncapped, unlike the facade
  (`f9aa060`).
- **H2:** `run_source` compared two sessions with unseeded `random`; both
  now get the same entropy (`e96ca7c`).

### Triaged, not findings

- **Slow units with long straight-line loop bodies** (`run_source`, 106
  distinct in run 3): `while True:` around hundreds of statements of
  big-int and complex arithmetic (unary-minus chains, `%`, `*`, `**`).
  Fuel is one unit per backward jump, so such a body costs up to
  0.49 s per 40,000-fuel slice, but every one parks at each slice, and a
  50 ms deadline stopped each within 89 ms on a loaded machine (59–63 ms
  alone) [M]. Two of them, `timeout-04de2e26` and `timeout-cdfaccc4`, are
  run 3's two ASan timeouts: 20.8 s and 27 s without a sanitizer, slices
  of 0.28–0.30 s, past the 60 s `-timeout` under ASan's slowdown [M].
- **`vm_bytecode` OOMs** (three in run 3, ASan only): `oom-c9ea28dd` and
  `oom-5cab988f` are C5 powers in program 5 (1.48 s and 0.41 s in one
  resume in plain release, 11 MB RSS); ASan's allocator keeps the freed
  temporaries until RSS passes 2,560 MB, while the engine's live heap
  stays at 27 MB [M]. `oom-50807414` does not reproduce alone (466 MB
  under ASan, flat over ten repeats): RSS that earlier inputs left in the
  fork child [M].
- **`nested_data` slow units** (75): the longest 40,000-fuel slice
  in any of them, across the harness's whole 5M-fuel budget, was 62 ms [M].
- **`vm_bytecode` slow unit `c135ff96`:** `str()` of a 48 MB container,
  memory-bounded; it overdraws then parks.
- **Speed, not host control** (they park every slice):
  - a float `**` loop costs about 1.75 µs per operation against about
    40 ns on CPython (the engine's double-double `libm::pow`), charged
    about one fuel unit (`run_source` `d9c8bfa3`) [M];
  - a big-int `~a % b + c` loop costs 638 ns per iteration against
    150 ns on CPython [M].

### Open

- C5, and the 24 h release run on the bounded-steps commit, which will
  use the step oracle (`crates/micro-monty/tests/steps.rs`) in the
  targets if it is wired in.
- The orb campaign's two open items above still stand; neither needed
  fuzzing work.
