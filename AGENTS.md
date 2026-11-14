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
just check               the landing tier: every gate (logs in target/check/logs/)
just release-gate        the release gate (`cargo xtask check --release-gate`)
just conformance [args]  Monty's corpus on mm vs conformance/results (`--filter X`)
just durability [args]   every passing case parked, snapshotted, restored: reports, fuel and
                         final-state digests vs conformance/durability-results.txt
just wasm                the facade on both wasm targets, round trip under wasmtime
just size                the shipped builds against the base, the release and its budget
just bench [args]        bench checksums on CPython and mm (`--reps N` for timings)
just pretag [base]       the pre-tag checklist against the previous `v*` tag
```

Run `just --list` for the rest. Gates without a recipe are `cargo xtask`
commands: `wasm-ir`, `v8-ir` (V8's instructions on both
wasm32-unknown-unknown guests vs base and anchor; release gate, Linux),
`native-ir` (the gated bench Ir on the shipped native `mm` vs base and
anchor; in `check`, Linux), `wasm-host`, `wasm-wall`, `gc-stress`,
`parse-diff`, `docs`, `difffuzz`, `handoffs` (writes
`xtask/handoff-sites.txt`, the `need_py()` hand-off sites; `--check` is in
`check`) and `shipped-mm` (`mm` built natively as shipped, the binary bench
Ir measures).

"Shipped" means built as embedders ship it (spec §6): std rebuilt with
`-Cpanic=immediate-abort` (`RUSTC_BOOTSTRAP=1` on the pinned stable) and
`--cfg mm_shipped`, which turns on `#[optimize(size)]` through
`cfg_attr(mm_shipped, …)`. `size`, `wasm`, `wasm-ir`, `v8-ir`,
`wasm-host`, `native-ir` and `shipped-mm` build that way, `shipped-cases`
runs the own cases and Monty's corpus on the shipped `mm`, and the
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
- Tiers: `cargo xtask check` before every push to main, rebased onto the
  latest main. It runs the release gate by itself (and says why) before a
  tag and for any change whose files since `origin/main`, committed or not,
  old and new names of a rename both counted, include a path of
  `RELEASE_GATE_PATHS` in `xtask/src/main.rs`: in `crates/mm-engine/src/`
  `interp.rs`, `snapshot.rs`, `nstep.rs`, `cont.rs`, `vm.rs`,
  `generator.rs` or `aio.rs`; `crates/mm-core/src/gc*` or `snapshot.rs`;
  `crates/micro-monty/src/image.rs` or `tests/golden/`;
  `crates/mm-compile/src/encode.rs`; a `crates/mm-wasm-*` crate;
  `xtask/src/wasm.rs` or `shipped.rs`; `rust-toolchain.toml`,
  `Cargo.lock`, `.cargo/` or `.agents/setup`. A line of a `[profile.*]`
  table, an `optimize(size)` mark added or removed, or no merge base does
  the same. `--landing-tier` forces the landing tier; say why in the commit
  if you use it. A facade API or snapshot format change also needs
  `cargo xtask wasm-host` (in the release gate).
- Ir: drops always pass; rises are gated against the base and against an
  anchor (each count's lowest since the last `v*` tag, so steps under a
  limit cannot add up; the gates need the tags fetched). Three gates, on
  Linux:
  - `cargo xtask native-ir` (both tiers): the gated count
    `python3 bench/callgrind.py` prints first, on the shipped native `mm`
    (`cargo xtask shipped-mm`). Fails on a program past +0.5% or `startup`
    past +2,900 instructions. Base and anchor build with their own
    toolchain and, when their xtask has `shipped-mm`, their own recipe.
  - `cargo xtask wasm-ir` (both tiers): the shipped wasip1 `mm`'s guest
    instructions under wasmtime. Fails on a program past +1.0%, the
    geomean past +0.5% or `startup` past +50,000 instructions.
  - `cargo xtask v8-ir` (release gate): V8's generated code on both
    `wasm32-unknown-unknown` guests. Fails on a program past +0.5% or a
    guest's geomean past +0.3%.

  An intended rise says why in its commit, with the table. native-ir and
  v8-ir take `Native-IR-Accepted:` / `V8-IR-Accepted: <names> <ceiling>
  <reason>`, such as `Native-IR-Accepted: dict,startup 1.006 <reason>`:
  it passes only the names it lists (programs or `geomean`; v8-ir may put
  `plain/` or `bundle/` before one) up to the ceiling ratio, and later runs
  re-anchor only those. `MM_IR_ACCEPT` / `MM_V8_IR_ACCEPT` with the same
  text stand for the line in an uncommitted tree only. wasm-ir takes
  `MM_WASM_IR_ACCEPT=<reason>` and a `Wasm-IR-Accepted:` line. Report the
  whole-process count `bench/callgrind.py` prints second too.
- Snapshots: keep code that writes or reads an image in the codec files
  (`micro-monty/src/image.rs`, `mm-engine/src/snapshot.rs`,
  `mm-core/src/snapshot.rs`, `mm-compile/src/encode.rs`). Bump
  `SNAPSHOT_EPOCH` when another function body changes what an image holds
  or means (spec §9.3). A change that moves the build id re-blesses the
  golden images in the same commit:
  `MM_BLESS=1 cargo test --release --locked -p micro-monty --test golden`.
- Size: `cargo xtask size` compares each shipped artifact with the base
  and the last `v*` tag, both built outside the checkout. A rise past the
  base needs a git trailer `Size: <artifact> <±bytes> <why>` in the commit
  that makes it; the trailers since the base must add up to the change
  within 10% (at least 1 KiB), a reverted commit's trailer counts for
  nothing, and an uncommitted rise always fails. A rise past the tag must
  fit that release's budget in `xtask/size-budget.txt`, read from the
  base, with signed bytes; only the owner adds budget lines, and one
  lands before the growth it allows. A push is judged once, head against
  base, on its net change, so an intended rise lands in its own push.
  `xtask/size-ceilings.txt` is history; do not write ceiling lines.
- Results that move on purpose are rerun and committed:
  `conformance/results/` (`cargo xtask conformance`), and
  `conformance/codemode/real/results/gate.jsonl` with a `why`. The
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
