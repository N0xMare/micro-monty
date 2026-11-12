# Real transcripts: recorded sessions from a local agent harness

The first census from real agent transcripts, next to the synthetic one in `../` (tasks written for
the census, solutions written by models in one shot). The traces are recorded sessions from a local
agent harness (plak, which defines the cell protocol `mm-cell-worker` speaks): real agents review this
repository through its two Python presentations, py-ptc and py-sdk, and every cell they write is
recorded with its imports, host calls, outcome, time and memory. Each session ran once with the
harness's own Monty worker (`plakous-py-host`) and once with `mm-cell-worker` in its place, then
(Phase 2) with micro-monty in the harness's process. Every number here is [M] on host1 (AMD Ryzen,
16 threads, Linux 7.0) unless it says otherwise.

## What ran

- **plak**: GitHub `main` e0809c84 plus one local commit that lets a stand-in worker answer for the
  Monty identity (`PLAK_PY_HOST_IDENTITY`, `docs/research/plak-integration.md`), built with `just install
  release` into an isolated prefix. Never pushed. A first Codex py-ptc set ran on plak d020df14; it is
  kept as a side set (`results/phase1-d020df14/`) and is not in the main tables.
- **Workers**: Monty `plakous-py-host` as plak e0809c84 ships it (sha256 `8f04d3c3…`), and
  `mm-cell-worker` from micro-monty 1ae712b (sha256 `1c36fe1e…`), which runs py-sdk cells without
  the pre-run type check (`docs/research/plak-integration.md`, "py-sdk"). Monty's worker type-checks every
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
| `jsvpy.py` | the JS-vs-Python table (`results/js-vs-python/compare.md`): py-ptc on mm against js-ptc on plak's QuickJS worker (`docs/research/js-vs-python-models.md`) |
| `inproc_load.rs`, `inproc_sweep.sh`, `loadsum.py` | Phase 2 load harness (a copy of the local plak example) and its summary |
| `results/phase1/` | the main census: 64 sessions, 1,147 cells, with replay verdicts |
| `results/phase1-d020df14/` | side set: 16 Codex py-ptc sessions on plak d020df14 |
| `results/phase2/` | 96 in-process sessions (native, wasm, native + decoded-stdlib cache), 1,694 cells, replayed on the mm child (`docs/research/plak-in-process.md`) |
| `results/phase2-357e003/` | refresh on main 357e003: 12 Codex sessions in process (native, wasm), 70 cells, all replayed identically on the mm child (`docs/research/plak-in-process.md` "Refresh") |
| `results/modes.md` | success, failures, engine time and memory per mode |
| `results/js-vs-python/` | the JS-vs-Python study, first sample (an Amp orb): 32 sessions, 550 cells, {codex, grok} × {py-ptc on mm, js-ptc on QuickJS} |
| `results/js-vs-python-host1/` | the same study, second sample (host1, bare metal): 32 sessions, 491 cells, redacted as the Phase 1 records |
| `results/load/` | Phase 2 load sweep (`sweep.jsonl`), warm sweep (`warm.jsonl`) and the refresh on 357e003 (`sweep-357e003.jsonl`) |
| `traces.tar.xz` | the raw traces of Phase 1 and 2 (160 sessions, every frame), as recorded on host1, redacted ("Regression gate") |
| `traces-js-vs-python.tar.xz` | the raw traces, `run.json` and answers of the JS-vs-Python study (32 sessions); not replayed by `gate.py` |
| `traces-js-vs-python-host1.tar.xz` | the same for its second sample on host1 (32 sessions), redacted; not replayed by `gate.py` |
| `gate.py`, `results/gate.jsonl` | the regression gate: every recorded cell replayed on current mm ("Regression gate") |

`cells.jsonl` has one record per cell: arm, prompt, cell number, `source`, `imports`,
`from_imports`, `host_calls` (tool name per call), `batches` (calls per batch), outcome and
first cause, a first-guess `class`, `wall_ms`, `engine_ms` (wall time minus the time spent waiting on
host calls), `rss_kb`/`hwm_kb` of the worker at the cell's end, and the replay verdict.

## Results

### Task success

| arm | sessions | answered | timed out (1,200 s) | graded correct (todo, loc) |
|---|---:|---:|---:|---:|
| codex-ptc-monty | 8 | 8 | 0 | 2 of 2 |
| codex-ptc-mm | 8 | 8 | 0 | 2 of 2 |
| codex-sdk-monty | 8 | 8 | 0 | 2 of 2 |
| codex-sdk-mm | 8 | 8 | 0 | 2 of 2 |
| grok-ptc-monty | 8 | 8 | 0 | 2 of 2 |
| grok-ptc-mm | 8 | 7 | 1 (`dict-review`) | 2 of 2 |
| grok-sdk-monty | 8 | 7 | 1 (`dict-review`) | 2 of 2 |
| grok-sdk-mm | 8 | 7 | 1 (`dict-review`) | 2 of 2 |

The three timeouts are the same prompt, on both workers: Grok kept reviewing `dict.rs` (113 to 600
frames, every cell finished) until plak's run timeout. Monty's py-ptc session finished it in time.
No session failed because of the worker. On `todo-census` the right answer is "none" (the four
`XXX` hits are `\uXXXX` strings); every arm said so.

### Cells

| arm | cells | failed | median engine ms | p90 engine ms | median worker peak RSS KiB |
|---|---:|---:|---:|---:|---:|
| codex-ptc-monty | 61 | 9 | 0.37 | 0.67 | 10,120 |
| codex-ptc-mm | 43 | 6 | 0.42 | 0.57 | 4,948 |
| codex-sdk-monty | 60 | 4 | 2.39 | 16.17 | 30,436 |
| codex-sdk-mm | 62 | 3 | 2.54 | 5.93 | 7,072 |
| grok-ptc-monty | 171 | 7 | 0.48 | 1.33 | 12,160 |
| grok-ptc-mm | 219 | 2 | 0.50 | 0.97 | 5,878 |
| grok-sdk-monty | 239 | 15 | 1.74 | 2.89 | 30,926 |
| grok-sdk-mm | 292 | 20 | 1.95 | 3.26 | 7,278 |

Engine time is small next to host calls on every arm: the median cell spends 40-60 ms in total, of
which 0.4-2.5 ms is the engine. py-sdk cells cost more engine time than py-ptc ones on both workers
(the SDK prelude's wrappers). mm's worker uses about half of Monty's memory on py-ptc and a quarter
on py-sdk (Monty's type checker is resident).

**Failed cells, by cause** (`results/phase1/summary.md` has every first cause):

- **No mm engine bug and no wrong output.** The replay (below) runs every recorded cell on the other
  worker; no cell changed from success to failure or back because of the engine.
- **mm refusals: 2 cells, both by design.** `import subprocess` (the sandbox has no processes; the
  message says to use a tool) and `import tokenize` (not bundled; the same cell then reads files with
  `Path.read_text`, which the sandbox refuses anyway). The `Path` methods refused for lack of an
  ambient filesystem (3 cells) are counted as model errors: the tools are the file API.
- **Model errors** are most of the rest: JavaScript written as Python (`{cmd: ...}`, `const`,
  `console.log`: 10 Codex py-ptc cells), `KeyError: 'stdout'` on a result without that key, Grok
  passing a regex to `ctx.grep(pattern=...)` without quotes (9 grok-sdk-mm syntax errors), and tool
  errors from plak (`path is outside the workspace`, invalid regex, read past the end).
- **Monty's type check refused 7 py-sdk cells** (1 Codex, 6 Grok). Replayed on mm, which does not
  check: 5 fail at once (syntax or name errors), 1 prints its first part and then fails on `import
  subprocess`, and 1 (`proc.shell([...])`, a list where a string is due) reaches plak, which refuses
  the call at run time (`ValueError: invalid arguments for proc.shell`, as in the codex-sdk-mm
  arm). So the check moved errors earlier and caught nothing that would have done harm, in this set.
- mm's syntax errors read differently from CPython's on 5 cells (`unexpected character after line
  continuation character` where CPython 3.14 says `invalid syntax. Perhaps you forgot a comma?`):
  both reject the cell. Fixed on main since (d984a97): all 30 syntax-error cells of Phase 1 and 2
  now get CPython's message.

### Cross-worker replay

`replay.py` reran each recorded session on the worker it did not use, with the recorded host-call
results:

| verdict | cells |
|---|---:|
| same | 1,079 |
| same outcome, other message | 39 |
| same outcome, other traceback | 25 |
| same outcome, other output | 1 |
| only replay failed | 1 |
| diverged | 1 |
| skipped (after the divergence) | 1 |

The message differences are repr and wording (mm writes CPython's `KeyError: 'stdout'`, Monty
`KeyError: stdout`); the tracebacks differ in caret lines. The other three are the type check:
"only replay failed" is a cell mm ran (`sys.path`, `import io`) that Monty's checker refuses,
"other output" is the partial-output cell above, and "diverged" is the `proc.shell` list cell. The
d020df14 side set (109 cells) replays with 89 same, 12 other traceback and 8 other message.

### In process (Phase 2)

The same prompts and models ran through a local plak build that runs
micro-monty on a thread of plak's process (native, wasm, and native with the
decoded-stdlib cache prototype). Every in-process cell replays identically
on the mm child (1,694 of 1,694 `same`), and success matches the child
modes; `results/modes.md` has the table and
[docs/research/plak-in-process.md](../../../docs/research/plak-in-process.md)
the analysis. Three more refusals appear there: `import ast` (3 cells), and
`subprocess` and `tokenize` again.

## Compared with the synthetic census

The synthetic census (`../results/summary.md`, 1,782 one-shot solutions to API/data tasks) and the
real one differ in kind:

| | synthetic | real (this set) |
|---|---|---|
| unit | one solution per task | one cell of a multi-turn session |
| cells with any import | most | 253 of 1,147 (22%) |
| `json` | 55% of solutions | 16.5% of cells (189, all Grok; 15 of 32 Grok sessions) |
| `asyncio` | 36% | 3.7% (43) |
| `re` | 17% | 0.7% (8) |
| `collections`, `datetime` | 15%, 12% | 0.3%, 0% |
| imports per family | families agree within a few points | Codex: 13 of 226 cells import anything; Grok: 240 of 921 |
| host calls per cell | fan-outs common (`gather`, up to 171 outstanding) | median 1; 74% of cells make exactly 1 |
| batches over 1 call | 163-204 solutions per family with 2-16 outstanding | 16 cells (all Grok), largest 10 |
| output per cell | mostly small | median 2.5 KiB, p90 14 KiB, max 84 KiB (file contents) |
| failures on mm | `xml`, `email` imports, 3 NameErrors | 2 refusals by design, no gaps that changed an outcome |

Real agents use a cell as a thin driver for one tool call at a time: read a file, grep, run a
command, print. They parse results with attribute access (py-sdk) or `json` (Grok on py-ptc), and
they rarely fan out. Codex barely imports at all. The synthetic set's stdlib breadth (`decimal`,
`html.parser`, `zoneinfo`) does not show up in code review; it would in data tasks. The synthetic
census remains the better test of engine coverage; this one is the better model of cost: what matters
is per-cell latency, host-call overhead and memory per session, not import breadth.

Import cost on mm (`mm run`, median of 40 runs, process start subtracted, [M]): `json` 0.14 ms,
`collections` 0.65, `re` 0.70, `asyncio` 0.97, `datetime` 1.43, all five 4.3 ms. Monty imports the
same five in 0.3 ms. In this census the cost lands on `json`, the cheapest.

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

The replay gave the same verdicts before and after (`--check` passed on both, with the same 59
differing cells at 73ccd57). In the census records only strings changed, each to its same-length redaction:
every record's other fields, the outcome, verdict, class and cause tallies, the numeric sums and
every number in `summary.md`, in order, are as before. The unredacted traces and results remain in
git history (266bde7, 7541558) by the owner's choice.

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
`conformance/results/`. A run takes 8.6 s on 16 cores and 16.7 s on one (the orb, [M]), most of it
one 4 s `dict-review` cell.

On 1068b7a, 46 cells differ from their recordings:

- 10 cells of micro-monty sessions (6 grok-sdk-mm, 4 grok-sdk-native), all SyntaxErrors whose
  message, line or caret moved with d984a97 (SyntaxError precedence) and now equal CPython 3.14's.
  The two Monty-session cells whose mm replay (1ae712b, `.amp/in/replay-p1.jsonl` on host1) differs
  today are the same change (the comma hint's range).
- 34 cells of Monty sessions failing with mm's message where Monty wrote its own (`KeyError:
  'stdout'`, the sandbox refusals, CPython's SyntaxError texts), as the cross-worker replay found.
- 2 cells of codex-sdk-monty `codec-review`: cell 12 diverges (`proc.shell` with a list, which
  Monty's type check refused and mm runs) and cell 13 is skipped after it.

Of the 36 cells that now fail with a SyntaxError, 35 rendered exactly as CPython 3.14 (message,
line, caret). The other, phase2 codex-ptc-native `builtins-callbacks` cell 1 (`const x = … x.stdout
?? x`), had CPython's message but its caret at the `??` where CPython's is at `x`: mm's lexer
rejected `?` at once, where CPython's tokenizer makes it an `OP` that only the parser rejects. Since
2026-11-07 it is a token only the parser rejects and all 36 match; the cell is re-recorded with a
`why`.
