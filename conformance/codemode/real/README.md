# Real transcripts: recorded sessions from a local agent harness

Recorded sessions from plak (<https://github.com/N0xMare/plak>, MIT), an open-source local agent
harness by micro-monty's author and micro-monty's integration target. plak runs code cells in worker
processes and talks to them over its cell protocol, `plakous-cell-protocol.v2`, which
`mm-cell-worker` speaks. Real agents review this repository through plak's two Python presentations:
py-ptc (programmatic tool calling, after Anthropic's PTC: globals persist across cells, tools are
async functions, no type checking) and py-sdk (each cell starts fresh and is type-checked against
generated stubs). Every cell they write is recorded with its imports, host calls, outcome, time and
memory. Each session ran once with plak's own Python worker, which runs Monty (`plakous-py-host`),
and once with `mm-cell-worker` in its place, then (Phase 2) with micro-monty in plak's process. `gate.py`
replays them on current mm as a gate of `cargo xtask check` ("Regression gate"). Numbers are [M] on
host1 (AMD Ryzen, 16 threads, Linux 7.0) unless labelled; the per-arm tables are in
`results/*/summary.md` and `results/modes.md`.

## What ran

- **plak**: GitHub `main` e0809c84 plus one local commit that lets a stand-in worker answer for the
  Monty identity (`PLAK_PY_HOST_IDENTITY`), built with `just install
  release` into an isolated prefix. Never pushed. A first Codex py-ptc set ran on plak d020df14; it is
  kept as a side set (`results/phase1-d020df14/`) and is not in the main tables.
- **Workers**: Monty `plakous-py-host` as plak e0809c84 ships it (sha256 `8f04d3c3…`), and
  `mm-cell-worker` from micro-monty 1ae712b (sha256 `1c36fe1e…`), which runs py-sdk cells without
  the pre-run type check. Monty's worker type-checks every
  py-sdk cell before it runs and refuses it on a type error ("type errors; nothing ran").
- **Models**: Codex `gpt-6-luna` at high effort, Grok `grok-4.7` at medium effort.
- **Prompts** (`prompts.json`, the same 8 for every arm): two reviews (`dict-review`,
  `codec-review`), two code searches (`fuel-loops`, `builtins-callbacks`), a format summary
  (`snapshot-format`), and three countable questions (`todo-census`, `loc-census`,
  `own-exceptions`). Each session runs in its own worktree of micro-monty at 1f27970, reset with
  `git clean` before each prompt; no session modified a file.
- **Recording**: `tracer/` (`cell-tracer`) sits between plak and the worker and logs every
  `plakous-cell-protocol.v2` frame, plus the worker's RSS and CPU after each cell from `/proc`. The
  in-process modes (Phase 2) write the same frames from inside plak (`PLAK_PY_INPROC_TRACE`).
- **Arms**: {codex, grok} × {py-ptc, py-sdk} × {monty, mm}, 8 sessions each, 64 sessions. Sessions ran
  four at a time, with a timeout of 1,200 s.

## Files

| file | what |
|---|---|
| `run_sessions.py` | runs the arms (`plak run --print-session-id`, `PLAK_VERBOSE=1`), one worktree per arm |
| `tracer/` | the frame recorder (its own Cargo workspace) |
| `census.py` | raw traces to `cells.jsonl`, `sessions.jsonl`, `summary.md` |
| `replay.py` | replays every recorded session on the *other* worker, cell by cell, answering each host call with its recorded result |
| `grade.py` | checks the two prompts with one right answer |
| `modes.py` | the five-mode table (`results/modes.md`): Phase 1's child arms against Phase 2's in-process arms |
| `jsvpy.py` | the JS-vs-Python table (`results/js-vs-python/compare.md`): py-ptc on mm against js-ptc on plak's QuickJS worker |
| `inproc_load.rs`, `inproc_sweep.sh`, `loadsum.py` | Phase 2 load harness (a copy of the local plak example) and its summary |
| `results/phase1/` | the main census: 64 sessions, 1,147 cells, with replay verdicts |
| `results/phase1-d020df14/` | side set: 16 Codex py-ptc sessions on plak d020df14 |
| `results/phase2/` | 96 in-process sessions (native, wasm, native + decoded-stdlib cache), 1,694 cells, replayed on the mm child |
| `results/phase2-357e003/` | refresh on main 357e003: 12 Codex sessions in process (native, wasm), 70 cells, all replayed identically on the mm child |
| `results/modes.md` | success, failures, engine time and memory per mode |
| `results/js-vs-python/` | the JS-vs-Python study, first sample (a 16-core cloud sandbox under gVisor): 32 sessions, 550 cells, {codex, grok} × {py-ptc on mm, js-ptc on QuickJS} |
| `results/js-vs-python-host1/` | the same study, second sample (host1, bare metal): 32 sessions, 491 cells, redacted as the Phase 1 records |
| `results/load/` | Phase 2 load sweep (`sweep.jsonl`), warm sweep (`warm.jsonl`) and the refresh on 357e003 (`sweep-357e003.jsonl`) |
| `traces.tar.xz` | the raw traces of Phase 1 and 2 (160 sessions, every frame), as recorded on host1, redacted ("Regression gate") |
| `traces-js-vs-python.tar.xz` | the raw traces, `run.json` and answers of the JS-vs-Python study (32 sessions); not replayed by `gate.py` |
| `traces-js-vs-python-host1.tar.xz` | the same for its second sample on host1 (32 sessions), redacted; not replayed by `gate.py` |
| `gate.py`, `results/gate.jsonl` | the regression gate: every recorded cell replayed on current mm ("Regression gate") |
| `idle_state.py`, `results/idle-state.md` | what globals hold between cells, recorded and synthetic ("Idle state and carrying it across builds") |
| `carry.py`, `results/carry.jsonl` | the host-side carry of an idle session to another build, and the names it leaves behind (same section) |

`cells.jsonl` has one record per cell: arm, prompt, cell number, `source`, `imports`,
`from_imports`, `host_calls` (tool name per call), `batches` (calls per batch), outcome and
first cause, a first-guess `class`, `wall_ms`, `engine_ms` (wall time minus the time spent waiting on
host calls), `rss_kb`/`hwm_kb` of the worker at the cell's end, and the replay verdict.

## Reproducing

```sh
cargo build --release --manifest-path tracer/Cargo.toml --target-dir ../../../target/cell-tracer
python3 run_sessions.py --raw RAW --plak PLAK --home ISOLATED_HOME --monty MONTY_WORKER \
    --mm MM_CELL_WORKER --tracer CELL_TRACER --worktrees W1,...,W8 --arms codex-ptc-monty,...
python3 replay.py RAW replay.jsonl --monty MONTY_WORKER --mm MM_CELL_WORKER
python3 census.py RAW results/phase1 --replay replay.jsonl
python3 grade.py RAW
```

The committed `replay.jsonl` is compacted (verdict and last lines only). The raw traces of
Phase 1 and 2 are in `traces.tar.xz` (below); the d020df14 side set's are not committed.

## Regression gate

```sh
python3.14 conformance/codemode/real/gate.py --check   # in `cargo xtask check`
python3.14 conformance/codemode/real/gate.py           # rewrite results/gate.jsonl
python3.14 conformance/codemode/real/gate.py --filter grok-sdk-mm/dict-review --check
```

`traces.tar.xz` (1.7 MB; 55 MB unpacked, sha256 `dbcb8befc54f9a83…`) holds the 160 traces and
`run.json` files of Phase 1 and 2 as the tracer and `PLAK_PY_INPROC_TRACE` wrote them, redacted:
2,841 cells with their `StartCell` frames, nested calls, recorded host results (`NestedResult`),
output and `CellDone`. Tool results are file contents of this repository at 1f27970, of plak, of
CPython's headers and of crates in host1's cargo registry, as the agents read them
(`THIRD_PARTY.md`); a pattern scan found no credentials. The archive is a test fixture for this
repository only: it is excluded from published packages.

The redaction, applied to every frame, to `results/gate.jsonl` and to the census records in
`results/phase1/` and `results/phase2/`, changes nothing a replay or a count can tell apart:

- The recording user's home directory is `/home/me` everywhere: cell sources, nested-call inputs,
  host results, output and messages, including inside the base64-encoded scripts one grok-ptc-mm
  `codec-review` session passed to `run_terminal_command`, and one output cut partway through the
  path. The user name in `ls` listings is `me`. Same length, so columns, carets and byte counts
  stay.
- One `env` listing (phase2 grok-ptc-native `codec-review`, cell 19) printed the machine's `PATH`;
  that line is removed from the cell's recorded tool result and from its recorded output alike,
  since the cell prints the result unchanged. Its `output_bytes` in `results/phase2/cells.jsonl`
  still counts the recording as it ran.
- The archive's member owners are 0/0.

`gate.py` replays each session in its recorded order, offline and without plak, through
`mm-cell-worker`'s `replay_worker` example: `mm_cell_worker::serve` over stdio with the shipped
`NativeEngine` session settings, as the binary runs it minus seccomp, with the wall clock pinned to
the session's recorded start and entropy a counter (no recorded cell reads the clock; the
`random` cells seed their own generators). It sends the recorded `Configure`, `Reset` and
`StartCell` frames and answers the k-th nested call of a cell with the recorded result of that
cell's k-th call when the worker reports it `Waiting`; a call whose name or input differs from the
recording diverges the session and skips its later cells. Sessions recorded on Monty's worker are
replayed the same way, so their cells check mm on Monty's transcripts.

For every cell it compares the nested calls (name and input), the batch sizes, the delivery
verdicts, the printed output and the whole `CellDone` outcome (kind, value, message with its
traceback, stored names) with the recording. `results/gate.jsonl` has one line per cell, in session
order, holding only the fields where current mm differs from the recording, with their current
value; each such cell of a micro-monty session carries a `why`. `--check` fails, with a diff, on any
cell that differs from that file; without `--check` the file is rewritten, keeping each `why` whose
cell still differs the same way. Commit it to record an intended change, as with
`conformance/results/`. A run takes 8.6 s on 16 cores and 16.7 s on one (the 16-core gVisor sandbox, [M]), most of it
one 4 s `dict-review` cell.

## Idle state and carrying it across builds

```sh
python3.14 conformance/codemode/real/idle_state.py [--check]                 # results/idle-state.md
python3.14 conformance/codemode/real/gate.py --carry --base v0.0.1 --check   # not in `cargo xtask check`
```

A snapshot restores only on the build that wrote it, so a host that upgrades drains its sessions
(docs/user-guide.md). Between cells it can carry a session instead, with ordinary cells and no engine
change. These two scripts measure how much that covers and gate a reference implementation.

**The census** (`idle_state.py`, UPG-1) replays every py-ptc session as `gate.py` does and runs a
probe cell after each recorded cell that classifies everything reachable from `globals()`; it also
runs the probe after each synthetic solution (`../solutions/`). On the 80 recorded py-ptc sessions
(1,216 states) [M]: 36 hold nothing, 983 only plain data, imports and top-level defs, 125 also stdlib
instances (`re.Match`, `defaultdict`, `re.Pattern`, `random.Random`, `PosixPath`) and 72 a
coroutine or generator; none holds a user class or instance, closure, lambda or task, and every
instance and coroutine state is Grok's. The synthetic solutions leave more: of the 1,732 that reach
the probe, 49 (2.8%) end with a user class, instance or lambda in their globals and 581 with a stdlib
instance (`Counter`, `re.Match`, `datetime`, ...). py-sdk sessions have no state between cells:
plak resets them before each one. The probes move no recorded cell.

**The carry** (`carry.py`, UPG-2) is the reference implementation. Before a cell, an export cell
on the old build prints the globals as a tagged, id-numbered value graph (shared and cyclic
containers, nan and inf as `float.hex`, big ints, bytes, ranges), in lines of at most 32 KiB and
cells of at most 900 KB, with modules by name and functions and classes by the statement that
bound them; it reports each global it could not carry and why. The host starts a worker of the
new build, runs the bootstrap, sends the records as literals, re-runs the binding statements
(`import`, `from ... import`, `def`, `class`, the last one of each name in the cells it sent) and
rebuilds the values. Lambdas, closures, user instances, coroutines, generators, tasks and stdlib
instances are not carried: a host drains such a session, or, as the gate does, drops the names.

**The gate** (`gate.py --carry [--base REV]`) replays each py-ptc session on a worker built from
`REV` (in a worktree under `target/carry/`; default: this build) and, before every cell but the
first, carries its state into a fresh worker of this build and runs the recorded cell there. Every
such cell must give what `results/gate.jsonl` expects of the straight replay, and the names left
behind must equal `results/carry.jsonl` (rerun without `--check` and commit it to record a change).
With `--base v0.0.1` [M]: 1,136 of 1,136 cells in 80 sessions match the straight replay, carried
from v0.0.1's build id to this one at every boundary; 190 boundaries in 7 sessions leave 15 names
behind (coroutines and generators, `re.Match`, `re.Pattern`, `defaultdict`, `random.Random`,
`PosixPath`), which no later cell reads. Control: carrying nothing, 74 cells differ. About 20 s on
16 cores after the base build (several minutes cold).
