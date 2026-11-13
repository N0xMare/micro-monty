# micro-monty

A small, fast, durable Python-subset sandbox engine in Rust. The design lives
in `docs/spec.md`; read it before changing engine code. It is built for code
mode: agents that call tools by writing Python cells.

## Layout

- `crates/mm-core` (no_std): values, arena heap, GC, write barrier, interning.
- `crates/mm-compile` (no_std): engine AST, symbol tables, bytecode.
- `crates/mm-engine` (no_std): VM, builtins as tables, async, fuel, parking.
- `crates/mm-stdlib` (no_std): bundled modules, one `mod-*` feature each.
- `crates/mm-parse` (no_std): Python source to the engine AST. The phase-1
  ruff front end survives only behind the `ruff-oracle` feature, as the
  oracle for `cargo xtask parse-diff`.
- `crates/micro-monty`: the public facade; `examples/hello.rs` is what the
  size budget measures.
- `crates/mm-cell-worker`: a worker for one external cell protocol
  (`plakous-cell-protocol.v2`), used in local integration testing, as a
  library (`mm_cell_worker::serve` over any `Read`/`Write`, the codec and
  session driving; no FFI, no seccomp) that the protocol's host can run
  in process, and the thin `mm-cell-worker` binary on top (stdio,
  seccomp, the §5 FFI exception, crash-cut hooks). `publish = false`.
  Policy values (16 outstanding nested calls, 1 MiB caps, …)
  are `Config` with the protocol host's values as defaults; no new policy features such
  as queueing, and the engine and facade impose no outstanding-call limit
  (`docs/spec.md` §10.2).
- `crates/mm-cli`: the `mm` binary (`run`, `exec --json`, `resume`) the
  conformance, durability and bench gates drive.
- `crates/mm-wasm-abi` (the `mm-wasm` codec), `crates/mm-wasm-guest`
  (the guest cdylib; the spec §5 `unsafe` exception) and
  `crates/mm-wasm-host` (the wasmtime reference host, its own workspace,
  tested by `cargo xtask wasm-host`). `crates/mm-wasm-alloc` is std's
  `wasm32-unknown-unknown` allocator without its per-site size checks,
  registered by the guest and the `hello`/`roundtrip` examples, never by
  the facade (spec §5, temporary).
- `xtask`: `cargo xtask size`, `conformance`, `durability`, `bench` and the
  other gates below.
- `conformance/monty`: Monty's test corpus (MIT, see `THIRD_PARTY.md`). Do
  not edit these files; add own cases under `conformance/own`.
  `conformance/oracle.py` runs cases on CPython 3.14 as Monty's datatest does,
  with Monty's helpers vendored unmodified in `conformance/monty-scripts`.
- `conformance/codemode`: synthetic programmatic-tool-calling tasks and model-written
  solutions, run on CPython and mm (`runner/`, crate `mm-codemode`); see its README.
  `conformance/codemode/real`: recorded sessions from a local agent harness,
  frame by frame (`traces.tar.xz`), and the gate that replays them (README
  there).
- `spikes`: throwaway benchmarks that decided early design questions, each
  its own cargo workspace (results in `docs/spikes`); nothing there ships.
- `bench`: the §12 programs (`.py` and `.js`) and `expected.txt` checksums.
- `hosts/js`: a TypeScript host for the shipped mm-wasm guest (Node,
  workerd / Cloudflare Workers), its tests (`npm test`) and the V8 and
  Workers measurements (docs/research/workers-v8.md). Outside cargo; not in
  `check`.

## Commands

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo xtask docs                    # docs/sdk.md's examples run as doctests; published rustdoc has no warnings
cargo xtask package                 # `cargo package` the published crates; a build from the packages keeps the build id
cargo xtask parse-diff              # own parser vs the ruff oracle on the corpus
cargo xtask size                    # shipped hello.wasm (primary), native hello and mm_wasm_guest.wasm budgets
cargo run --release --locked -p micro-monty --example embedding_bench   # embedding acceptance workload
cargo test --release --locked -p micro-monty --test golden   # golden snapshot images restore and continue (MM_BLESS=1 re-blesses)
cargo run --release --locked --manifest-path fuzz/Cargo.toml --features memory-probes --bin memory_probes   # Rust heap <= 2.5 x limit + 16 MiB (spec §11)
cargo xtask wasm                    # facade on wasm32-wasip1/-unknown-unknown; round trip under wasmtime
cargo xtask conformance --check     # corpus on CPython 3.14 vs conformance/results/
cargo xtask conformance --engine micro-monty --check   # corpus on mm vs results/
cargo xtask conformance --engine micro-monty --target wasm32-wasip1 --check   # corpus on wasm mm vs the same results
python3.14 conformance/codemode/real/gate.py --check   # 2,841 recorded real cells replayed on mm-cell-worker
cargo xtask durability              # passing cases parked, snapshotted, resumed in new processes
cargo xtask durability --cross-target   # own cases, each snapshot restored on the other target
cargo xtask bench --quick --engines cpython,micro-monty   # checksums match
cargo xtask wasm-ir                 # wasmtime guest instructions vs origin/main: +1.0% a program, +0.5% geomean (callgrind; Linux)
python3.14 conformance/codemode/run.py --check   # codemode workload outcomes unchanged (not in `check`)
cargo xtask durability --fuel       # also fuel totals parked vs unparked, against conformance/durability-fuel-allow.txt (not in `check`)
cargo xtask wasm-host               # shipped mm-wasm guests vs crates/mm-wasm-host tests (not in `check`)
cargo xtask wasm-wall               # wasm wall time vs origin/main on wasmtime and V8 (advisory; not in `check`)
uv run --with pyelftools python3 xtask/size_map.py   # size map of the shipped builds (docs/research/size-map.md; not in `check`)
cargo deny --all-features check     # licenses, advisories, bans, sources (deny.toml; not in `check`; before a release tag)
cargo deny --all-features --manifest-path fuzz/Cargo.toml check
cargo deny --all-features --manifest-path crates/mm-wasm-host/Cargo.toml check
```

`cargo xtask check` runs the first nineteen (`wasm-ir` on Linux only),
plus clippy on mm-engine with no module features and on the facade with none, `crates/micro-monty/tests/features.rs`
on a facade without `asyncio` and `json` but with `ast`, and the step-time oracle
(`crates/micro-monty/tests/steps.rs`, release, alone: no `Bounded` site's native
step may pass 50 ms; docs/spec.md §5). Its stages run at the same time where
they can (`xtask/src/check.rs`; the order and why in `fn check`): the debug
gates build in `target/check/debug`, the release and shipped ones in
`target/`, stages that write the same files run in turn, durability (the
longest) runs at normal priority and the rest at `nice` 10, and the
step-time oracle runs last with the machine to itself. Durability runs its
slowest jobs first, by the times the last run recorded in
`target/durability/`. Each stage logs to
`target/check/logs/<stage>.log`; the first failure stops the rest and
prints the end of its log. `--serial` runs one stage at a time with the
output on the terminal.

`check` has two tiers over the same stages; which one a change needs is
fixed by the paths it touches:

- **Landing tier**, plain `cargo xtask check` (10:53 warm after an engine
  change on a 16-core orb [M]). Builds that only run (the `mm` and workers
  of conformance, durability, the replay and bench, the golden images) use
  the `gate` profile (release with thin LTO, 16 codegen units); the test
  stages use `gate-test` (dev at opt 1, debug assertions kept); durability
  parks the own cases and the corpus subset at `1:100` beyond `5:50`, still
  at every host call.
- **Release gate**, `cargo xtask check --release-gate` (22:18 warm [M]):
  those builds in `release` and `cargo test`'s profile, durability at
  `1:1000`, plus `wasm-host` and the codemode check.

Sizes, `embedding_bench`, the shipped builds and both halves of the
step-time oracle build with `release` in both tiers; the oracle's fuel half
(`slice_charges`) runs beside the other stages and only its wall-time half
runs alone. The release gate is required, instead of the landing tier,
before a release tag and for any change whose
files since `origin/main` (`git diff --name-only origin/main...HEAD`, plus
uncommitted ones) include `crates/mm-engine/src/interp.rs`, `snapshot.rs`
or `nstep.rs`, `crates/mm-core/src/gc.rs` or `snapshot.rs`,
`crates/micro-monty/src/image.rs`, `crates/mm-compile/src/encode.rs`, a
wasm crate (`crates/mm-wasm-*`), or a line of a `[profile.*]` table in
`Cargo.toml`. Every other change lands on the landing tier. A landing that
rebased cleanly onto new commits reruns the tier it needs in the same
checkout: the target directories are warm, so only what the rebase changed
rebuilds. Every gate still runs: the combined tree is new.
Verification is local; GitHub runs no gates (owner, 2026-11-12). A `v*`
tag runs only `.github/workflows/release.yml`, which builds the release
artifacts and opens a draft release. Before tagging, run on the tagged tree:
the release gate, the fuzz build (`cargo check --manifest-path
fuzz/Cargo.toml --features memory-probes --bins`), `cargo xtask wasm-ir
--base <previous v* tag>` and `cargo xtask wasm-wall --base <previous v*
tag>` when there is one (`wasm-ir` catches drift built up from commits each
under its limits), and the three `cargo deny`
checks (cargo-deny 0.20.2 from `.agents/setup`), which stay out of `check`
because the advisory database is fetched over the network.
`python3.14` (CPython 3.14.7, installed by `.agents/setup` through uv; override
with `PYTHON`) is the conformance oracle.

- `cargo xtask wasm-host` builds `crates/mm-wasm-guest` as shipped for
  `wasm32-wasip1` and `wasm32-unknown-unknown`, optimizes both, then runs
  clippy and the release tests of `crates/mm-wasm-host` (its own workspace
  and lockfile, so wasmtime stays out of `check`) against each: parity with
  the native facade on every `mm-wasm` export, images crossing native and
  wasm. **Required before any push that touches the wasm crates
  (`crates/mm-wasm-*`), the facade API or the snapshot format.** The
  interface is frozen with 0.0.1 (`ABI_VERSION` 1): a release may add a
  session option (a new tag in the options record whose default keeps the
  behaviour without it) or an optional export; anything else bumps
  `ABI_VERSION`, with `crates/mm-wasm-host` and `hosts/js` in the same
  change (docs/research/wasm-embedding.md). A new option takes a strict
  tag (below `0x80`) unless a guest can safely run without it: only hints
  and diagnostics take an ignorable one, never a limit, the clock,
  entropy, output or the text cut.
- `cargo xtask wasm-wall [--base REV] [--rounds N]` builds the working
  tree and `REV` (default `origin/main`, in a worktree under
  `target/wasm-wall/`) as shipped, the wasip1 `mm` and the
  `wasm32-unknown-unknown` guest, and times the bench programs on both in
  alternating rounds (default 11; about 5 minutes plus the builds): `mm
  run` under wasmtime, and the guest under `node --no-liftoff`
  (`hosts/js/bench/wall.ts`, TurboFan only, so steady state). Each round
  keeps the fastest of 3 runs per build; checksums must match. It fails
  when an engine's geomean of the per-program median ratios head / base
  passes 1.03, or a program's median passes 1.05 with its lower quartile
  above 1. Calibration [M], the same build against itself, 11 rounds:
  geomeans 0.991 (V8) and 0.999 (wasmtime), every program within ±4.7%.
  Timing is noisy on a shared orb, so it stays out of `check`; run it
  alone. Each run writes its rounds to `target/wasm-wall/rounds/`; when
  runs disagree, judge them together with `cargo xtask wasm-wall --pool
  FILE...` (same base and head), never by the best run. Advisory since
  0.0.2: `wasm-ir` is the required wasm speed gate. Run it for wall time
  the instruction count cannot see, such as code layout on the host's
  CPU, and before a release tag (below).
- `cargo xtask wasm-ir [--base REV] [--head REV] [--programs a,b]` builds
  the working tree (or `--head`) and `REV` (default `origin/main`, each in
  a worktree under `target/wasm-ir/`) as shipped, the wasip1 `mm`
  precompiled by `wasmtime compile --target x86_64-unknown-linux-gnu`
  (baseline ISA, so the same code on every machine), and counts each bench
  program's guest instructions under callgrind: the instructions at
  addresses in wasmtime's perf map (`--profile=perfmap`), the guest's
  compiled code only. The count does not depend on load or host, so one
  run decides; about 30 s warm on 16 cores [M]. Unrelated commits move
  the run loop's register allocation by up to about 1% either way, so it
  fails only when a program's count rises past 1.0% or the geomean of the
  ratios past 0.5%; fewer instructions always pass, with the table
  printed. `MM_WASM_IR_ACCEPT=<reason>` passes an intended rise; the
  commit message then carries the table and the reason. Drift from
  commits each under the limits is caught before a release tag, against
  the previous tag (below). It is in `check` on Linux (valgrind).
  Calibration [M]: 4fe04e1's Cranelift spill fails it (fib 1.080, closure
  1.102, loop 1.087, geomean 1.039); the same build against itself gives
  1.0000. Each of the 0.0.1 batch's 30 commits against its parent
  (828eba9..ca39f934): one trips it, ca99d1b8 at closure 1.0103 (layout,
  no hot-path change); a two-sided ±0.5% tripped six. The batch end to end
  (closure 1.0104, geomean 1.0033) trips it too, as the pre-tag run would
  have. It cannot see the host-tuned layout cost that
  batch added (wall 1.03–1.08 on AVX-512 precompiles, counts at most
  1.010 generic and 1.006 at AVX2; valgrind does not run AVX-512 code);
  `wasm-wall` measures that. **Required before every push**, as part of
  `check`.
- `cargo xtask parse-diff` builds `mm-parse` with `ruff-oracle` in a
  separate cargo invocation (ruff never enters the default build) and
  requires identical ASTs, or identical micro-monty errors, from both front
  ends on every corpus file, own case, bench program and the prelude.
- The size ceilings are in `xtask/size-ceilings.txt`: each artifact's
  ceiling is the sum of its lines. Move one by appending a line (signed
  bytes and why, with the measurement and the decision), never by editing
  one; git merges the file as a union (`.gitattributes`), so raises on two
  branches add up on a rebase without a conflict.
- `cargo xtask size` and `cargo xtask wasm` build as shipped: release, std
  rebuilt with `-Cpanic=immediate-abort` and no unwind tables, enabled on the
  pinned stable with `RUSTC_BOOTSTRAP=1` (`docs/spec.md` §6); wasm modules
  then go through `wasm-opt -Oz` (binaryen 133 from `.agents/setup`;
  `$WASM_OPT`, `~/.local/bin`, `PATH`; required) into
  `target/<target>/release/shipped/`. Everything else builds on plain stable.
- `cargo xtask wasm` builds `micro-monty` and its examples in release for
  `wasm32-wasip1` and `wasm32-unknown-unknown` (targets listed in
  `rust-toolchain.toml`), prints `hello.wasm`'s size on each before and
  after `wasm-opt`, and runs the optimized `roundtrip` example (park,
  snapshot, restore, finish) on both under
  wasmtime when it is installed (`$WASMTIME`, `~/.local/bin`, `PATH`;
  skipped otherwise).
- `cargo xtask gc-stress` (not in `check`; about 85 minutes) reruns the own
  cases, the corpus (`--check`) and durability on the own cases with
  `MM_GC_STRESS=1`, which makes `mm` collect at every safepoint
  (`Session::set_gc_stress`); output must not change. `--incremental`
  (about 6 minutes; run it on GC changes) does the same with
  `MM_GC_STRESS=inc`: an incremental mark at every safepoint in tiny
  slices, each checked against a full mark (`Session::set_gc_inc_stress`).
- `cargo xtask conformance` reruns the corpus and rewrites
  `conformance/results/`; commit the change when results move on purpose.
  `--filter <substring>` runs a subset and prints failures in full.
- `conformance/codemode/real/gate.py --check` replays the 160 recorded
  agent-harness sessions (2,841 cells, codex and grok on py-ptc and py-sdk) in
  session order through `mm_cell_worker::serve` (the `replay_worker`
  example: the shipped library and session settings over stdio, wall clock
  pinned to the recording, counter entropy), answering each nested call with
  its recorded result. It fails if any cell's outcome, output, exception
  message and traceback, value or nested-call sequence moves from
  `conformance/codemode/real/results/gate.jsonl`, which holds only where
  current mm differs from the recording, each mm-recorded cell with a
  `why`. Rerun without `--check` and commit the file to record an intended
  change, adding its `why`. About 9 s on 16 cores, 17 s on one.
- `cargo xtask conformance --engine micro-monty` runs the corpus on
  `target/release/mm` (built first; `MM` overrides) and rewrites
  `conformance/results/micro-monty.jsonl` and `micro-monty.md` (pass/fail/skip
  per tag, failures by first cause). Its `--check` fails on any change: a
  newly failing case is a regression; a newly passing one must be recorded by
  regenerating and committing the results. With `--target wasm32-wasip1`
  it runs `mm` built for wasm32-wasip1 under wasmtime instead (the build
  durability's cross-target run uses; about 4 s on 16 cores) and scores it
  against the same `micro-monty.jsonl`, which it never rewrites: every
  target must give the native results.
- `cargo xtask durability` runs every passing case (all of
  `conformance/own`, the corpus cases `micro-monty.jsonl` records as
  `pass`) through `conformance/oracle.py --durability`: parked every few
  fuel units and at every host call, each park saved by one `mm` process
  (`mm exec --snapshot-every N --snapshot-dir D`) and continued by a new one
  (`mm resume --json D`); the report must equal an unparked run's.
  `--every N[:M]` and `--filter <substring>` narrow it. `--fuel` also
  compares the fuel the chain spent (the last `mm` prints it) with the
  unparked run's, prints every run whose total moved and fails on a case
  `conformance/durability-fuel-allow.txt` does not name (one case and
  reason per line). It stays out of `check` until that list holds only
  the exceptions spec §5 guarantee 4 documents.
- `cargo xtask durability --cross-target` runs the own cases the same way,
  but alternates the parked chain between the native `mm` and `mm` for
  `wasm32-wasip1` under wasmtime (built and precompiled first; wasmtime is
  required), once starting on each, so every snapshot is restored on the
  other target.
- `cargo xtask bench` times every engine found (`cpython`; `qjs` via `QJS`,
  `monty` via `MONTY`; `micro-monty` as `mm run`, built in release or `MM`)
  and writes `bench/results/`. Every listed program must succeed and match
  its checksum, including warm-up; a Python exception is a regression.
  Requested CPython and micro-monty engines must be available. Run it alone
  on an idle machine; `--reps N` sets repetitions (default 10).

## Rules

- Bench Ir moves at most ±0.5% per change (HANDOFF scope decision 2), on
  the *gated* count `python3 bench/callgrind.py` prints first: from glibc's
  `__libc_start_main` onward, without glibc's `pthread_getattr_np`
  (`/proc/self/maps` parsing, which follows the binary's layout). Report
  the whole-process count it prints second as well (`docs/spec.md` §12).
  wasmtime's guest instructions have their own limits, set by layout
  noise: a program may rise 1.0% and the geomean 0.5% (`cargo xtask
  wasm-ir`, in `check`); a rise past them says why in the commit, with
  the table.

- No `unsafe`. The workspace forbids it; an exception needs a spec change
  first (`docs/spec.md` §5).
- The Rust stack never holds two dispatch loops. Builtins that can reach user
  code use a native fast path for builtin-typed data and hand off to prelude
  bytecode otherwise. This is what makes pause-anywhere hold; do not add a
  nested `run` call.
- Every Rust recursion (parser, compiler, `repr`/`==`/`hash`/`copy`/`json` on
  nested data) is iterative or depth-counted and raises `RecursionError`,
  never overflows the Rust stack.
- Stdlib: Rust for builtin types and data-proportional modules; everything
  else is Python under `crates/mm-stdlib/python/`, compiled at build time
  (`docs/spec.md` §12).
- No panics on guest input: every guest-reachable error is a Python
  exception. Clippy denies `unwrap`, `expect`, `panic!` and `todo!`; tests may
  allow them with a reason.
- Builtins are table entries with the one calling convention
  `fn(&mut Vm, CallArgs) -> Result<Value, Raised>`.
- No serde, no generics across crate APIs, no new dependency in engine crates
  unless it wins in the size budget.
- Every mutation of the arena goes through mm-core's setters.
- Snapshots restore on any build with the same build id, which hashes the
  snapshot codecs (`micro-monty/src/image.rs`, `mm-engine/src/snapshot.rs`,
  `mm-core/src/snapshot.rs`, `mm-compile/src/encode.rs`), every `const` and
  `#[repr]` item of mm-core, mm-compile and mm-engine, the tables and the
  bytecode, but no other function body (`docs/spec.md` §9.3,
  "Compatibility"). Keep code that writes or reads an image in those
  codec files, and bump `SNAPSHOT_EPOCH` (mm-engine `snapshot.rs`) in the
  same commit when another function body changes what an image holds or
  how it is read: in particular what a raw word or value slot of an
  existing NSTATE op holds with the same counts (`nstep::STEPS` hashes
  only names and counts), what an existing `Cont` field or stage means, or
  a layout written as literal slot numbers in code. A change that moves
  the id (a hashed file, `const` or table, a new or recounted NSTATE op,
  an epoch bump) blesses the golden images again in the same commit:
  `MM_BLESS=1 cargo test --release --locked -p micro-monty --test golden`.
  It rewrites `crates/micro-monty/tests/golden/*.mmsnap`, but writes nothing
  and fails if a continuation transcript (`*.txt`) changed; `cargo xtask
  check` fails while the images carry another id. To change a transcript
  on purpose, delete it, bless again and say why in the commit.
- Label reported numbers [M] measured, [S] sourced, [E] estimated.

## Delivery

Work on `main`. Commit at verified phase boundaries and push to `main` when a
phase is complete and verified: `cargo xtask check` passes, or `cargo xtask
check --release-gate` for a change the path rule under Commands sends to
the release gate, and always before a release tag. No other
pushes, PRs, issues or messages unless the owner asks. Do not modify plak or
Monty; test plak integration against a local plak checkout.
