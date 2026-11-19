This repo is **micro-monty**, a small, fast, durable Python 3.14 subset
engine in safe Rust for code mode: agents that call tools by writing Python
cells. It has no ambient I/O; every effect is a host call, and memory, fuel,
recursion and output are bounded per session. A cell can stop at any
safepoint, at any call depth, and continue later in the same process or from
a snapshot in another process or target (native, `wasm32-wasip1`,
`wasm32-unknown-unknown`). The engine crates (`mm-core`, `mm-compile`,
`mm-parse`, `mm-engine`, `mm-stdlib`) are `no_std` and published, pinned to
each other, under the `micro-monty` facade; `mm-cli` (the `mm` binary),
`mm-cell-worker`, the mm-wasm crates and `hosts/js` embed it. Conformance is
checked against CPython 3.14.7 on Monty's corpus (`conformance/monty`, do not
edit), own cases (`conformance/own`), code-mode workloads and recorded agent
sessions.

## First reading

1. docs/spec.md
2. docs/user-guide.md
3. AGENTS.md files and READMEs next to the tool you run (`conformance/`,
   `conformance/codemode/`, `bench/`, `hosts/js/`)

Load further docs only when changing the normative behavior they describe.

## Dev

`.agents/setup` installs everything: Rust 1.100.0 (pinned in
`rust-toolchain.toml`, with both wasm targets and rust-src), CPython 3.14.7
through uv (the oracle; `PYTHON` overrides `python3.14`), wasmtime 50.0.0,
binaryen 133 (`wasm-opt`), valgrind, cargo-fuzz 0.13.2 with nightly-2026-11-01,
cargo-deny 0.20.2 and just 1.59.0. Work and push on `main` unless told
otherwise. Prefer `just check` before push.

```text
just build               cargo build of the workspace
just test [args]         the workspace's tests (debug)
just fmt                 format every crate
just lint                rustfmt --check and clippy with warnings denied
just check               the dev tier, before a push (logs in target/check/logs/)
just release-gate        every stage: main's tip after pushes, and before a tag
just conformance [args]  Monty's corpus on mm vs conformance/results (`--filter X`)
just durability [args]   every passing case parked, snapshotted, restored: reports, fuel and
                         final-state digests vs conformance/durability-results.txt
just wasm                the facade on both wasm targets, round trip under wasmtime
just size                the shipped builds against the base, the release and its budget
just bench [args]        bench checksums on CPython and mm (`--reps N` for timings)
just pretag [base]       the pre-tag checklist against the previous `v*` tag
just seed-floors [tag]   the Ir floors seeded from the tag's counts: the first
                         commit after a tag
just release-dry-run     release.yml dispatched on GitHub, no release (after
                         pretag; it runs on GitHub, so only with the owner's go)
```

Run `just --list` for the rest. Gates without a recipe are `cargo xtask`
commands: `wasm-ir`, `v8-ir` (V8's instructions on both
wasm32-unknown-unknown guests vs base and floors; release gate, Linux),
`native-ir` (the gated bench Ir on the shipped native `mm` vs base and
floors; in `check`, Linux), `replay-ir` (the recorded real sessions' Ir on
the replay worker vs base and floors; release gate, Linux), `wasm-host`, `wasm-wall`, `gc-stress`,
`parse-diff`, `docs`, `difffuzz`, `handoffs` (writes
`xtask/handoff-sites.txt`, the `need_py()` hand-off sites; `--check` is in
`check`) and `shipped-mm` (`mm` built natively as shipped, the binary bench
Ir measures).

"Shipped" means built as embedders ship it (spec §6): std rebuilt with
`-Cpanic=immediate-abort` (`RUSTC_BOOTSTRAP=1` on the pinned stable) and
`--cfg mm_shipped`, which turns on `#[optimize(size)]` through
`cfg_attr(mm_shipped, …)`. `size`, `wasm`, `wasm-ir`, `v8-ir`,
`wasm-host`, `native-ir`, `replay-ir` and `shipped-mm` build that way, `shipped-cases`
runs the own cases and Monty's corpus on the shipped `mm`, `musl` does the
same on `mm` shipped for `<arch>-unknown-linux-musl` and runs the worker's
tests there (Linux), and the
step-time oracle is built fully as shipped (std and libtest rebuilt, each
test in its own process; a row over the limit prints `OVER`).

`check` runs its stages at the same time (`xtask/src/check.rs`): `mm`,
durability and the workspace tests at normal priority, the rest at `nice`
10, and the step-time oracle's wall-time half last, alone; each stage logs
to `target/check/logs/<stage>.log`.

## Working agreements

- Do not preserve backwards compatibility. micro-monty is published as
  0.0.x, and every 0.0.x release may change any API: remove obsolete paths
  instead of adding compatibility layers, fallbacks, old-format readers or
  migrations. An mm-wasm ABI change bumps `ABI_VERSION`, with
  `crates/mm-wasm-host` and `hosts/js` in the same change; a snapshot change
  moves the build id (below), and images restore only on their own build.
- Choose the simplest implementation that fully meets the current
  architecture. Avoid speculative abstractions, configuration and
  indirection.
- Grow in layers: the smallest end-to-end working engine first, then add
  capability to an engine that already passes every gate. Never trade a
  working engine for unfinished complexity.
- Keep crates modular and concerns separated. Visibility as narrow as
  possible (private, then `pub(crate)`, then `pub`).
- Prefer what is already in the tree. Do not reimplement common
  functionality or add a crate without checking the existing crates, docs
  and types; engine crates take no new dependency, no serde and no generics
  across crate APIs unless it wins on size.
- Make architectural decisions for the long term; no stopgap meant to be
  replaced later. Locked decisions change only through a spec change the
  owner approves.
- Optimize for the reader: explicit data flow, small modules, early returns,
  enums for distinct states, typed errors with sources. No clever
  abstractions, hidden control flow or one-line pass-through helpers.
- Keep parsing and presentation at the product boundary: neutral core
  types; the facade, `mm` and the hosts convert and render.
- Comments explain non-obvious invariants and tradeoffs, not control flow
  the code already shows. Update docs/spec.md and docs/user-guide.md with
  the behavior they describe.
- Test externally meaningful behavior and failure modes, deterministically:
  own cases checked against CPython, `tests/steps.rs` rows, durability. The
  daily bar is unit and offline integration tests; the release gate and the
  long runs (`gc-stress`, fuzzing) close a phase.
- No credentials, auth headers, private session data or unsanitized
  recordings in the tree, logs or tests. The workspace forbids `unsafe`.
- Tiers (owner, 2026-11-21): three, so the loop before a push is short.
  **Dev tier**: `cargo xtask check` before every push to main, rebased
  onto the latest main: fmt, clippy, docs, the workspace tests, the
  feature builds, conformance natively, durability at `1:100`, the golden
  images, size and native-ir (`DEV_STAGES` in `xtask/src/main.rs`). A
  change adds the stages that guard its paths (`DEV_ADDS`, printed with
  the path): what an image holds (`crates/mm-engine/src/snapshot.rs`,
  `crates/mm-core/src/gc*` or `snapshot.rs`,
  `crates/micro-monty/src/image.rs` or `tests/golden/`,
  `crates/mm-compile/src/encode.rs`) adds durability at `1:1000` and
  cross-target; the dispatch loop and park state (in
  `crates/mm-engine/src/` `interp.rs`, `nstep.rs`, `cont.rs`, `vm.rs`,
  `generator.rs` or `aio.rs`) adds the step oracle's fuel half; a
  `crates/mm-wasm-*` crate adds `wasm` and `wasm-host`. **Release gate**:
  `cargo xtask check --release-gate`, every stage (the timed step oracle,
  memory probes, package, wasm-ir, v8-ir, replay-ir, the wasip1 runs,
  wasm-host, codemode, the real replay). It runs on main's tip after
  each push, in the background, and always before a tag; main may be
  pushed on the dev tier, and a release-gate failure on main is fixed or
  reverted in the next push, before anything else lands. A commit that
  expects a wasm-ir, v8-ir or replay-ir rise runs that gate by hand
  first, for its accept line and floor. **Before a tag**: `just pretag`
  and the long runs (`gc-stress`, `difffuzz`, the fuzz targets, the
  correctness sweeps). Plain `check` runs the release gate by itself (and
  says why) for any change whose files since `origin/main`, committed or
  not, old and new names of a rename both counted, include a path of
  `RELEASE_GATE_PATHS` in `xtask/src/main.rs`, which no dev-tier stage
  judges: `xtask/src/wasm.rs` or `shipped.rs`; the release-gate-only Ir
  gates' code (`xtask/src/v8_ir.rs`, `replay_ir.rs`, `accept.rs`,
  `floors.rs`), `xtask/ir-floors/v8-ir.txt` and `replay-ir.txt` and what
  only they read (`conformance/codemode/real/gate.py`, `carry.py` and
  `traces.tar.xz`; `hosts/js/src/`, `hosts/js/bench/count.ts`,
  `hosts/js/package.json`); `rust-toolchain.toml`,
  `Cargo.lock`, `.cargo/` or `.agents/setup`. A line of a `[profile.*]`
  table, an `optimize` mark (`size`, `speed` or `none`, bare or in
  `cfg_attr`) added, removed, edited or moved to another function (a
  changed line under `crates/` that names `optimize(`, or a `.rs` file
  under `crates/` whose `optimize` attributes, each with the next `fn`
  outside comments and attributes, differ from the merge base's), or no
  merge base does the same. Not seen, so such a change needs
  `--release-gate` by hand: a mark chosen by a macro invocation, by a cfg
  set in `build.rs` or by a feature list; in a file under `crates/` that
  is not `.rs` (an `include!`d one), a mark in an untracked file, a
  multi-line condition edited and a mark moved to another function (one
  added on one line of a tracked file is seen); and a change around a
  mark on a closure or an async block, which is paired with the next
  function of the file. `--base <rev>` measures a stack from its
  parent instead of `origin/main` (the tier's paths, size, the Ir gates),
  so a stack can be gated while its parent is gated or waits to be
  pushed; it is pushed once the parent is main. `check` prints the base
  and head it judged, and refuses a `--base` that already holds HEAD
  when the tree is clean. A facade API or snapshot format change also
  needs `cargo xtask wasm-host` (in the release gate).
- Ir: drops always pass; rises are gated against the base and against
  each count's floor, the lowest it has been since the floors were seeded
  at the last `v*` tag (the pre-tag check makes that seed happen), kept
  in `xtask/ir-floors/<gate>.txt` with the build that set it and its count
  key (counting method, tools, host, program source), so steps under a
  limit cannot add up and no gate builds history. Floors bind upward
  only (owner): a count below its floor passes with no file change, so a
  drop then a rise back to the old floor passes until the floors are next
  lowered, by `cargo xtask <gate> --write-floors` when someone runs it or
  by the tag's seed. A geomean against the floors takes each program's
  base count where it is below its floor, so a floor left above a drop
  is no headroom for the other programs' rises. A new count needs a line
  once, at or below its count.
  A written line names the build of the push's tip. A floor
  rises only to the head's count, with an accept line that covers the rise
  and is past a limit. A stale floor (another key) is recounted on its
  build and judged against the recount; the file keeps the base's line
  unless the head moves that count, so hosts with different keys both pass
  an unchanged tree. Keys are rewritten only by a moved count or a seed
  (`--base <tag> --write-floors`). Two landings that both move a floor: take the raise
  when one side is an accepted raise, else the lower count; the gate then
  sets it. Startup drift is not accepted (owner): `startup` may move 300
  instructions without a line. Four gates, on Linux:
  - `cargo xtask native-ir` (both tiers): the gated count
    `python3 bench/callgrind.py` prints first, on the shipped native `mm`
    (`cargo xtask shipped-mm`), run with an empty environment from a fixed
    path. Fails on a program past +0.5% or `startup` past +300
    instructions. The base builds with its own toolchain and, when its
    xtask has `shipped-mm`, its own recipe.
  - `cargo xtask wasm-ir` (release gate): the shipped wasip1 `mm`'s guest
    instructions under wasmtime. Fails on a program past +1.0%, the
    geomean past +0.5% or `startup` past +300 instructions. Other revisions
    build with their own toolchain and, when their xtask has `shipped-mm
    --target`, their own recipe.
  - `cargo xtask v8-ir` (release gate): V8's generated code on both
    `wasm32-unknown-unknown` guests. Fails on a program past +0.5% or a
    guest's geomean past +0.3%.
  - `cargo xtask replay-ir` (release gate): the 160 recorded real sessions
    (`conformance/codemode/real`) on the replay worker built as shipped
    plus `--cfg mm_size_wasm` (the wasm-only `optimize(size)` marks on),
    against the base and each session's floors. Fails on a session past
    +0.5% or the sessions' median ratio past +0.3% (`sessions`,
    instructions outside glibc's allocator), on a session whose allocator
    calls rise past +0.5% (`sessions-calls`), and on a session that
    diverged on the head only. A session's count races, so its floors
    move only past a band (0.15% of its Ir, 0.05% of its calls), a line
    the file holds on the head's build within the band of a read is that
    read's line, and a gate's first `--write-floors` writes every line
    above the read at the read. Its counts are cached by the worker's
    hash (`target/replay-ir/cache`): prove written floors on a recount
    with the cache removed.

  An intended rise says why in its commit, with the table. Each gate takes
  a line `<Prefix> <names> <ceiling> <reason>`: `Native-IR-Accepted:`
  (native-ir and replay-ir), `Wasm-IR-Accepted:` or `V8-IR-Accepted:`,
  such as `Native-IR-Accepted: dict,startup 1.006 <reason>`. It passes
  only the names it lists (programs, `sessions`, `sessions-calls` or
  `geomean`; v8-ir may put `plain/` or `bundle/` before one) up to the
  ceiling ratio, and raises only the floors of those past a limit; an
  accepted geomean (replay-ir: `sessions`' median) past its limit raises
  the floor of every program (session) in it that rose.
  `MM_IR_ACCEPT`, `MM_WASM_IR_ACCEPT` and `MM_V8_IR_ACCEPT` with the same
  text stand for the line in an uncommitted tree only. A tag starts new
  floors: the first commit after it is `just seed-floors`, which runs each
  gate with `--base <tag> --write-floors` and writes every floor at the
  tag's count and build, or the head's where it is lower or an accepted
  rise, whatever the file held, stale keys included; `--programs` cannot
  seed. A seed that lands late is judged with `--base <tag>`. `just
  pretag` runs each gate against the previous tag and fails a floor whose
  build is not on the tag's first-parent line up to HEAD, a floor on the
  tag's or the head's build that does not hold its count, and an accepted
  rise not raised, each naming `just seed-floors`. With native-ir, report the whole-process
  count `bench/callgrind.py` prints second too.
- Snapshots: keep code that writes or reads an image in the codec files
  (`micro-monty/src/image.rs`, `mm-engine/src/snapshot.rs`,
  `mm-core/src/snapshot.rs`, `mm-compile/src/encode.rs`). Bump
  `SNAPSHOT_EPOCH` when another function body changes what an image holds
  or means (spec §9.3). A change that moves the build id re-blesses the
  golden images in the same commit:
  `MM_BLESS=1 cargo test --release --locked -p micro-monty --test golden`.
  Every commit that moves the id is blessed, not only a stack's tip:
  under one id a later commit's images must continue as an earlier
  one's did, which is what catches a change of meaning that does not
  move the id. `cargo xtask restack <commits>` cherry-picks a stack and
  blesses each commit whose images fail where the id moved from HEAD's;
  where blessing would keep HEAD's id it stops, as the commit changes
  what an image means (a `SNAPSHOT_EPOCH` bump is missing), brings a
  scenario blessed elsewhere or changes a scenario itself; `restack
  --resolved` finishes the pick it stopped at. It copies each commit's
  author and committer, and refuses a commit, or a HEAD to stack onto,
  authored or committed by another identity than the one the commits
  rule below names.
- Size: `cargo xtask size` compares each shipped artifact with the base
  and the last `v*` tag, both built outside the checkout. A rise past the
  base needs a git trailer `Size: <artifact> <±bytes> <why>` in the commit
  that makes it, as its reason: the gate judges the measured net change,
  not the trailers' sum (owner, 2026-11-19). A reverted commit's trailer
  counts for nothing, and an uncommitted rise always fails. A rise past
  the tag must
  fit that release's budget in `xtask/size-budget.txt`, read from the
  base, with signed bytes; only the owner adds budget lines, and one
  lands before the growth it allows. A push is judged once, head against
  base, on its net change, so an intended rise lands in its own push.
  A commit that adds `optimize(size)` marks under `crates/` says how many
  an Ir workload runs, in a trailer `Size-Marks: <n> added, <run> run by
  an Ir workload (<how>)`, `<run>` at least 1; a mark moved to another
  function or given another condition counts as added, and `cargo xtask
  size` fails without it. A mark
  on for wasm only is `cfg_attr(all(mm_shipped, any(target_arch =
  "wasm32", mm_size_wasm)), optimize(size))`, so replay-ir counts it.
  `xtask/size-ceilings.txt` is history; do not write ceiling lines.
- Results that move on purpose are rerun and committed:
  `conformance/results/` (`cargo xtask conformance`), and
  `conformance/codemode/real/results/gate.jsonl` with a `why`.
  `conformance/durability-results.txt` is recorded once per push, on the
  stack's tip (`cargo xtask durability --record`, then `--cross-target
  --record`): the gate judges the head's file. A `report` or `fuel`
  difference is a bug and is not recorded. The
  idle-state census and the cross-build carry gate are not in `check`:
  `python3.14 conformance/codemode/real/idle_state.py --check` and
  `python3.14 conformance/codemode/real/gate.py --carry --base v0.0.1 --check`
  (`results/idle-state.md`, `results/carry.jsonl`; README there).
- Commits are authored and committed by
  `n0x <54751288+N0xMare@users.noreply.github.com>`, with no Amp-Thread-ID
  or Co-authored-by lines.
- Label reported numbers [M] measured, [S] sourced, [E] estimated.
- Do not modify Monty or the agent harness micro-monty is tested against.

## Locked (do not re-litigate)

- No `unsafe`: the workspace forbids it. The spec §5 exceptions are the
  mm-wasm guest's import block, `mm-wasm-alloc` (temporary) and the
  `mm-cell-worker` binary's seccomp FFI; a new one needs a spec change.
- One dispatch loop on the Rust stack: builtins that can reach user code
  hand off to prelude bytecode, never a nested `run`, so a cell can pause
  anywhere.
- Every guest-reachable error is a Python exception, never a panic, and
  every Rust recursion is iterative or depth-counted (`RecursionError`).
- Native steps are bounded: about 50 ms between safepoints at the default
  limits (`tests/steps.rs`, timed on shipped codegen).
- CPython 3.14 is the oracle.
- No serde or new engine dependency unless it wins on size.
