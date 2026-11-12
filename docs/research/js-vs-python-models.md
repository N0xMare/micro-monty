# JavaScript on QuickJS or Python on micro-monty: which do models code better in?

The same two models ran the same eight review tasks through plak twice: once writing Python in
py-ptc cells on `mm-cell-worker`, once writing JavaScript in js-ptc cells on plak's rquickjs host
(`plakous-js-host`). Every cell's frames were recorded. It ran twice: sample 1 in an Amp orb
(E2B, 16 vCPUs, Linux 6.1), whose timings are directional, and sample 2 on host1 (bare metal, Ryzen,
16 threads, Linux 7.0) with the same arms, prompts, worktree commit and worker build. Every number
in sample 1's sections is [M] in the orb, and in sample 2's [M] on host1, unless it says otherwise.

## Summary, sample 1 (orb)

| arm | answered | correct (todo, loc) | model turns, median | input tokens, median | cells, median (total) | failed cells (other language) | median engine ms | median worker CPU ms / cell | median session s | median worker peak RSS KiB |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| codex py-ptc, mm | 8 of 8 | 2 of 2 | 7 | 94,516 | 6 (48) | 8 (5) | 3.21 | 0.83 | 54.4 | 2,858 |
| codex js-ptc, QuickJS | 8 of 8 | 2 of 2 | 6 | 113,141 | 5.5 (45) | **0** (0) | 9.15 | 6.24 | 45.5 | 2,816 |
| grok py-ptc, mm | 7 of 8 | 2 of 2 | 17 | 768,795 | 23 (228) | 8 (0) | 1.54 | 0.93 | 146.5 | 3,820 |
| grok js-ptc, QuickJS | 7 of 8 | 2 of 2 | 19 | 894,457 | 18 (229) | 6 (1) | 4.13 | 3.82 | 156.6 | 4,376 |

Codex is `gpt-6-luna` at high effort, Grok `grok-4.7` at medium. "Answered" counts sessions that
ended with an answer; the one Grok session per arm that did not is `dict-review`, which ran until
plak's 1,200 s run timeout on both languages (as in Phase 1). "Model turns" are model operations
per session from plak's `turn.usage` events. Engine time is the cell's wall time minus the time a
nested call was outstanding (`engine_ms_calls`, below). The full table, with p90s, output tokens
and wall times per cell, is `conformance/codemode/real/results/js-vs-python/compare.md`.

## Summary, sample 2 (host1, bare metal)

Run 2026-11-10 01:15–01:57Z on host1, otherwise as sample 1 ("What ran"): plak e0809c84 + the local
`mm-test` 8d58898f (plak 0.0.3, `~/plak-mm-root`), `mm-cell-worker` from 5df122e (sha256
`d1c7a49a…`), plak's `plakous-js-host` from the same plak build, worktrees at 1f27970, four at a
time, 1,200 s timeout, `env -i`. The machine was otherwise idle (the fuzz campaign had ended).

| arm | answered | correct (todo, loc) | model turns, median | input tokens, median | cells, median (total) | failed cells (other language) | median engine ms | median worker CPU ms / cell | median session s | median worker peak RSS KiB |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| codex py-ptc, mm | 8 of 8 | 2 of 2 | 7 | 90,775 | 6 (52) | 9 (7) | 0.38 | 0.30 | 65.6 | 5,130 |
| codex js-ptc, QuickJS | 8 of 8 | 2 of 2 | 8 | 121,783 | 6.5 (50) | **0** (0) | 1.50 | 1.45 | 52.1 | 4,872 |
| grok py-ptc, mm | 7 of 8 | 2 of 2 | 18 | 720,646 | 19 (188) | 6 (0) | 0.43 | 0.33 | 152.6 | 5,504 |
| grok js-ptc, QuickJS | 7 of 8 | 2 of 2 | 19 | 922,932 | 22 (201) | 7 (1) | 1.61 | 1.60 | 163.2 | 5,098 |

The full table is `conformance/codemode/real/results/js-vs-python-host1/compare.md`. The one Grok
session per arm without an answer is again `dict-review`, at the 1,200 s timeout in both languages.

## Both samples

Pooled per arm (16 sessions each), counts only; timings stay per machine, since the orb's
median cell took 1.8–3.3× host1's wall time [M]:

| arm | answered | correct (todo, loc) | model turns, median (total) | input tokens, median | cells, median (total) | failed cells | of which other language |
|---|---:|---:|---:|---:|---:|---:|---:|
| codex py-ptc, mm | 16 of 16 | 4 of 4 | 7 (116) | 94,516 | 6 (100) | 17 | 12 |
| codex js-ptc, QuickJS | 16 of 16 | 4 of 4 | 7 (111) | 113,141 | 6 (95) | **0** | 0 |
| grok py-ptc, mm | 14 of 16 | 4 of 4 | 18 (385) | 720,646 | 19 (416) | 14 | 0 |
| grok js-ptc, QuickJS | 14 of 16 | 4 of 4 | 19 (385) | 917,876 | 20.5 (430) | 13 | 2 |

Failed cells by cause, pooled:

| arm | other language | syntax error | runtime error | missing feature | refusal by design | tool error | terminated |
|---|---:|---:|---:|---:|---:|---:|---:|
| codex py-ptc, mm | 12 | 2 | 3 | 0 | 0 | 0 | 0 |
| codex js-ptc, QuickJS | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| grok py-ptc, mm | 0 | 3 | 4 | 1 | 3 | 2 | 1 |
| grok js-ptc, QuickJS | 2 | 3 | 0 | 0 | 2 | 6 | 0 |

Engine time and memory per machine (medians):

| | sample 1 (orb) | sample 2 (host1) |
|---|---|---|
| engine ms per cell, mm / QuickJS | codex 3.21 / 9.15, grok 1.54 / 4.13 (QuickJS 2.7–2.9×) | codex 0.38 / 1.50, grok 0.43 / 1.61 (QuickJS 3.7–3.9×) |
| worker CPU ms per cell, mm / QuickJS | 0.83–0.93 / 3.82–6.24 | 0.30–0.33 / 1.45–1.60 |
| cell wall ms, mm / QuickJS | codex 161.6 / 181.8, grok 74.2 / 84.9 | codex 53.6 / 55.2, grok 40.6 / 44.4 |
| worker peak RSS KiB, mm / QuickJS | codex 2,858 / 2,816, grok 3,820 / 4,376 | codex 5,130 / 4,872, grok 5,504 / 5,098 |

**Where the samples agree.** Every graded answer is right in both (8 of 8 each); the same prompt
(`dict-review`) timed Grok out in both languages both times; Codex never failed a js-ptc cell (0 of
95) and wrote JavaScript into py-ptc in both; median input tokens per session are within 8% between
samples for each arm, and median turns and cells within 1–4; and mm's engine time per cell is about
a third of QuickJS's in the orb and a quarter on host1. The conclusion holds: the language changes how many cells fail for
Codex, not whether either model finishes the task.

**What changed in sample 2:**

- **Codex's JavaScript-first habit was total**: all 8 py-ptc sessions began with a JavaScript
  cell (`const = await exec_command({cmd:…})`, 7 failed; `own-exceptions` began `const = 1`, which
  is valid Python), against 7 of 8 in sample 1.
- **No engine-side failure at all**: no missing feature, no refusal by design and no terminated
  cell on either worker. Sample 1's mm gaps (`__import__`, `globals()` writes) and its file-API
  refusals did not recur, and neither did the brute-force loop that mm ran 5.1× slower than
  CPython; grok-ptc-mm `dict-review` used one worker throughout.
- **Grok fanned out in Python**: 17 py-ptc cells used `asyncio.gather` (up to 8 nested calls
  outstanding), against 3 in sample 1; no JavaScript cell used `Promise.all` in either sample.
- **Grok's JavaScript failures moved to the tools**: 4 tool errors on js-ptc, three of them
  `repository query pattern is not a valid regular expression`. In one, a backslash in a regex
  inside an ordinary JavaScript string was eaten by the string (`"…|Object \{|…"` reaches `grep`
  as `Object {`), a mistake Python's raw strings avoid; the fourth asked for multiline grep, which
  the tool does not support.
- **Grok still computes in Python from JavaScript**: 21 js-ptc cells in 4 of 8 sessions ran
  `python3` through the terminal tool (25 in 5 in sample 1), and no cell reached for Node's
  `Buffer`.
- **Timings**: on bare metal the median cell is 41–55 ms of wall time, still almost all host tool
  time; the engine-time gap between QuickJS and mm widens to 3.7–3.9× because mm's per-cell
  overhead falls further than QuickJS's (whose 13–18 KB catalog prelude per cell is unchanged).
  Median peak RSS is higher on host1 for both workers: by 1.6–2.2 MiB on mm and 0.7–2.0 MiB on
  QuickJS [M]. Since both rose, it follows the machine (kernel 7.0 against 6.1, how pages are
  counted) rather than the engine [E].

Language-specific error kinds, both samples: Python written as JavaScript (Codex, 12 cells); a
JavaScript raw string written Python-style (`r'''…'''`) and a Python heredoc whose quoting broke a
JavaScript string (Grok, 2 cells); keyword arguments inside a dict literal
(`grep({"pattern": …, type="rust"})`, Grok py-ptc) and a grep flag as a JavaScript property name
(`{ -A: 25 }`, Grok js-ptc, 3 cells across samples); Python names that need an import
(`asyncio`, 3 cells) where JavaScript's `Promise` needs none; and JavaScript string escapes eating
regex backslashes (1 cell).

## Findings (sample 1, orb)

- **Both languages get the work done equally.** Every session either answered or ran to the timeout
  on the same prompt in both languages; all 8 graded answers are right (`todo-census`: none;
  `loc-census`: mm-engine, 51,988 lines), and all four arms counted 159 `own-exceptions` cases,
  which is what `rg -l -w -e raise -e try -e except conformance/own` finds on 1f27970. No session
  modified a file. On this sample, the language changes how many cells fail, not whether the
  task is done.
- **Codex writes its first Python cell as JavaScript.** In 7 of its 8 py-ptc sessions, Codex's
  first cell was JavaScript or began with a JavaScript fragment: `const = await
  exec_command({cmd:"…"}); console.log(const)` (5 cells, all failed: `NameError: name 'cmd' is not
  defined` or `SyntaxError`) or the no-op `const = 1`, which happens to be valid Python (3 cells,
  one followed by Python that failed on quoting). It then switched to Python and finished. On
  js-ptc, Codex had no failed cell in 45. plak's js-ptc *is* Codex's own Code Mode presentation
  (`tools.exec_command`, `text()`, plak's "Codex 0.153.3" profile), so Codex is on home ground
  there; the cost of Python for Codex is about one wasted turn per session (median 7 turns against
  6), not a worse answer. Phase 1 saw the same habit (10 such Codex py-ptc cells, on plak's own Monty worker and on mm
  alike), so it belongs to the model and the presentation, not to the engine.
- **Grok writes clean code in both languages,** with one JavaScript failure that comes from
  Python: a Python script pasted into a JavaScript raw string as `r'''…'''`
  (`SyntaxError: expecting ';'`). Its Python mistakes are Python-specific: `asyncio.gather`
  without `import asyncio` (2 cells; JavaScript's `Promise` needs no import), and using the
  standard library's file API where the sandbox has none (`open()`, `Path.exists`: 2 cells,
  refused by design with a message pointing to the tools).
- **On js-ptc, Grok still computes in Python.** In 25 JavaScript cells (5 of 8 sessions) Grok
  wrote a Python script and ran it through the terminal tool under CPython, twice by base64-encoding
  it with Node's `Buffer`, which plain QuickJS does not have (`ReferenceError: Buffer is not
  defined`). On py-ptc it did the same experiments in the cell, on mm (4 cells shelled out to
  `python3`). Both Codex arms shelled out to `python3` in 2 cells. When the task needs computing,
  the models reach for Python whichever language the cell is in.
- **Two mm gaps showed up, one known, one by design.** `__import__('math')` fails with `NameError`
  (an open gap that HANDOFF already lists, from the synthetic census), and `globals()["TEXT"] =
  text` is refused because `globals()` is a read-only view (owner decision G2, spec §4). One cell
  of each; the model rewrote both.
- **mm is slow on a brute-force loop.** One Grok py-ptc cell ran a 51-million-iteration search
  (`dict-review`); Grok waited 60 s with `wait_py`, terminated it, and plak replaced the worker,
  losing the session's globals. That cell takes 27.1 s on CPython 3.14 and 137.6 s on
  `mm run` (5.1×) in the orb [M]. On js-ptc, Grok ran the equivalent experiments under CPython
  through the terminal instead. This is the one cell in the study where the engine's speed changed
  what the model did.
- **Engine cost is small next to the model either way, and lower on mm.** The median cell is
  75-180 ms of wall time, nearly all of it the host's tool call; engine time is 1.5-3.2 ms on mm
  against 4.1-9.2 ms on QuickJS, and worker CPU per cell 0.8-0.9 ms against 3.8-6.2 ms. Most of
  QuickJS's extra time is plak's JavaScript host, not the language: every js-ptc `start_cell`
  carries the whole tool catalog as a 13-18 KB prelude (mm's is 38 bytes), and the QuickJS
  worker takes a median 3.1-4.9 ms to its first frame against 0.9-1.3 ms on mm. Worker memory is
  the same order: median peak RSS 2.8 MiB on both for Codex, 3.8 MiB (mm) against 4.4 MiB
  (QuickJS) for Grok; the largest is mm's 9,684 KiB in the brute-force session.
- **Neither language fanned out.** No JavaScript cell used `Promise.all`; three Grok Python cells
  used `asyncio.gather` (3, 4 and 8 calls outstanding). Every other cell had at most one host call
  outstanding, as in Phase 1.

## What ran

- **plak**: GitHub `main` e0809c84 plus the local `mm-test` commit 8d58898f (the
  `PLAK_PY_HOST_IDENTITY` override, `docs/research/plak-integration.md`), cloned from GitHub with the commit
  fetched from host1's bundle; built with `tools/build/build-plak.sh --install` (`just install
  release`) on Rust 1.99.0 into an isolated `HOME` (`~/plak-root`), with its own credential store
  (`XDG_STATE_HOME=~/plak-root/.local/state`). Never pushed.
- **JS modes in plak**: `plak run --composition` takes `direct`, `plakous-default`, `native-ptc`,
  `js-ptc`, `py-ptc` and `py-sdk`. There is **no JS SDK mode**, so this study has no sdk arms.
  js-ptc exposes `exec_js`/`wait_js` with the nested tools under `tools.*` and `text()` for output;
  py-ptc exposes `exec_py`/`wait_py` with the tools as top-level async functions and `print`.
  The JavaScript worker is located through `PLAK_JS_HOST` (else beside `plak`).
- **Workers**: `mm-cell-worker` built in release from micro-monty main 5df122e (sha256
  `c23b5046…`), as `PLAK_PY_HOST` with `PLAK_PY_HOST_IDENTITY=micro-monty/micro-monty-cell-adapter.v1`;
  plak's `plakous-js-host` from the same build (sha256 `1aac5739…`, engine `quickjs`, adapter
  `plakous-js-host-bootstrap.v7`, 64 MiB heap), as `PLAK_JS_HOST`.
- **Arms**: {Codex `gpt-6-luna` high, Grok `grok-4.7` medium} × {py-ptc on mm, js-ptc on
  QuickJS}: `codex-ptc-mm`, `codex-jsptc-qjs`, `grok-ptc-mm`, `grok-jsptc-qjs`. 32 sessions, four
  at a time (one arm per worktree), 1,200 s timeout, prompts in `prompts.json` order.
- **Prompts and worktrees**: Phase 1's 8 prompts (`conformance/codemode/real/prompts.json`), each
  on a worktree of micro-monty at 1f27970 (Phase 1's commit, so the graded answers are the same),
  reset with `git checkout -- . && git clean -fd` before every session.
- **Recording**: `cell-tracer` in front of both workers (a two-line `worker.sh` as
  `PLAK_PY_HOST` or `PLAK_JS_HOST`), so both languages produce the same records: every
  `plakous-cell-protocol.v2` frame with nested calls and their recorded results, and the worker's
  RSS, peak RSS and CPU time at each cell's start and end. `run_sessions.py` also keeps plak's
  session event log for the token and turn counts.
- **Environment**: `plak run` got only `HOME`, `USER`, `LOGNAME`, `SHELL`, `LANG`, `PATH` and the
  plak variables (`env -i`), so no orb secret could reach a tool's output.

Differences between the arms other than the language, which the comparison cannot separate:
js-ptc is Codex's native Code Mode presentation (plak's `YieldMode::EngineParked` and
`CellLifecycle::RootTurnJoined`, the catalog in every prelude) where py-ptc is plak's own
(`HostWindow`, `AgentIncarnationPersistent`); each model saw the same tools under the same names in
both (Codex `exec_command`; Grok `read_file`, `grep`, `run_terminal_command`, `list_dir`).

## Measures

- **Engine time** (`engine_ms_calls` in `cells.jsonl`) is the cell's wall time (`start_cell` to
  `cell_done`) minus the union of the intervals in which a nested call was outstanding (its
  `nested_call` to its `nested_result`). plak's QuickJS worker sends no `waiting` frames, so
  Phase 1's `engine_ms` (which subtracts from each `waiting` to the batch's last result) would
  count JavaScript's host calls as engine time; `engine_ms_calls` is the one measure both
  workers' frames support. On mm the two differ by 0 to 8.5 ms per cell (the time between a
  `nested_call` and its `waiting`).
- **Worker CPU** is the worker's CPU time between the probes at `start_cell` and `cell_done`
  (`/proc/PID/task/*/schedstat`).
- **Fan-out** (`max_outstanding`) is the most nested calls outstanding at once, from the same
  intervals, since `batches` needs `waiting` frames.
- **Failure causes** (`jsvpy.py`): model mistakes (code in the other language, other syntax
  errors, runtime errors), missing features (a CPython builtin mm lacks), refusals by design (no
  ambient filesystem or processes, `globals()` read-only, no Node APIs in QuickJS), tool errors
  reported by plak, and cells with no `cell_done` (terminated by the model).
- **Tracer overhead**: forwarding a frame through `cell-tracer` costs 0.03 ms for a 200-byte
  frame, 0.18 ms at 30 KB and 0.48 ms at 100 KB per pass (it writes and flushes each frame to its
  log before forwarding it; measured against a plain pipe), and the `/proc` probe at each
  `start_cell` takes a median 0.03 ms, outside the measured cell time. Both workers pay the same,
  so it does not bias the comparison; it is a few percent of the median cell's wall time and part
  of every engine-time figure here (large tool results are logged on the way in).

## Failed cells (sample 1, orb)

Sample 2's and the pooled counts are under "Both samples".

| arm | other language | syntax error | runtime error | missing feature | refusal by design | tool error | terminated |
|---|---:|---:|---:|---:|---:|---:|---:|
| codex py-ptc, mm | 5 | 1 | 2 | 0 | 0 | 0 | 0 |
| codex js-ptc, QuickJS | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| grok py-ptc, mm | 0 | 0 | 2 | 1 | 3 | 1 | 1 |
| grok js-ptc, QuickJS | 1 | 1 | 0 | 0 | 2 | 2 | 0 |

Examples, one per kind:

- Codex, Python written as JavaScript (`todo-census` cell 1): `const r = await exec_command({cmd:"rg
  -n --no-heading -i '(TODO|FIXME|XXX)' crates -g '*.rs'", …}); console.log(r);` →
  `SyntaxError: invalid syntax`.
- Codex, Python syntax (`loc-census` cell 3): an `awk` program with `"/"` inside a Python
  string delimited by `"` (cell 1 of the same session failed the same way at run time:
  `TypeError: unsupported operand type(s) for /: 'str' and 'str'`). Its js-ptc session wrote the
  counting as a `python3` heredoc inside `cmd:` and had no failure.
- Codex, runtime (`own-exceptions` cell 2): `print(r["stdout"])` on an `exec_command` result,
  whose field is `output` → `KeyError: 'stdout'`, as in Phase 1.
- Grok, JavaScript written as Python (`builtins-callbacks` cell 20): `const py = r'''…'''`.
- Grok, JavaScript syntax (`builtins-callbacks` cell 25): a grep flag as a property name,
  `tools.grep({ …, -A: 25 })` → `SyntaxError: invalid property name`.
- Grok, Python runtime (`fuel-loops` cell 23, `own-exceptions` cell 6):
  `await asyncio.gather(…)` without `import asyncio` → `NameError: name 'asyncio' is not defined.
  Did you forget to import 'asyncio'?`, as CPython says it.
- Missing feature (`dict-review`): `__import__('math').modf(2**53+1)` →
  `NameError: name '__import__' is not defined`.
- Refusals by design: `open("/tmp/dict.rs", "w")` and `Path(...).exists()` (no ambient
  filesystem), `globals()["TEXT"] = text` (G2), and on QuickJS `Buffer.from(...)` (2 cells).
- Tool errors: a path outside the workspace, an invalid glob, a file that does not exist
  (`crates/mm-engine/src/list.rs`).

No failure in either language came from an engine bug or a wrong result.

## Records

- `conformance/codemode/real/results/js-vs-python/`: `cells.jsonl` (one record per cell, 550
  cells), `sessions.jsonl` (32 sessions, with turns and tokens), `summary.md` (census.py),
  `grade.jsonl` (grade.py), `compare.md` (jsvpy.py).
- `conformance/codemode/real/traces-js-vs-python.tar.xz` (515 KB; sha256 `8b97454f…`): every
  trace, `run.json` and answer (`stdout.txt`) of the 32 sessions, as
  `mm-real-raw/jsvpy/<arm>/<prompt>/`. grok-ptc-mm `dict-review` has two traces: plak started a new
  worker after the terminated cell, whose first trace ends in `shutdown` with no `cell_done`.
- Redaction: the orb's account is the generic `user` (`/home/user`), not the owner's, so the
  same-length home-path redaction of the earlier records has nothing to replace and the files keep
  `/home/user`; no `ls -l` owner column and no environment listing appear; member owners are 0/0.
  A scan for the orb's secret values (by value) and for token shapes found none; the only email
  addresses are this repository's own commit authors, in one `git log` output.
- Sample 2: `conformance/codemode/real/results/js-vs-python-host1/` (the same five files, 491
  cells, 32 sessions; `census.py --machine "host1 (bare metal, Ryzen, 16 threads)"`) and
  `conformance/codemode/real/traces-js-vs-python-host1.tar.xz` (453 KB; sha256 `862b77ac…`), as
  `mm-real-raw/jsvpy-host1/<arm>/<prompt>/`, member owners 0/0. Redacted as Phase 1's host1
  records: the recording user's home directory is the same-length `/home/me` everywhere (1,355
  occurrences in the raw files), so every count and byte length is unchanged (`compare.md`
  regenerates identically). A scan found no environment listing, token-shaped string, private key,
  email address or `ls` owner column, and no base64 blob holding a home path.
- The two samples' records therefore use different home paths: `/home/user` (sample 1, the
  orb's generic account, kept as recorded) and `/home/me` (sample 2, redacted).
- The 16 sample 1 py-ptc sessions (276 cells, 17 worker traces) replay on current mm through `gate.py`'s
  replayer with every cell as recorded except the terminated one, which the replay runs to its end.

## Reproducing

```sh
cargo build --release -p mm-cell-worker
cargo build --release --manifest-path conformance/codemode/real/tracer/Cargo.toml --target-dir target/cell-tracer
python3 conformance/codemode/real/run_sessions.py --raw RAW --plak PLAK --home PLAK_HOME \
    --state PLAK_HOME/.local/state --mm target/release/mm-cell-worker --qjs PLAKOUS_JS_HOST \
    --tracer target/cell-tracer/release/cell-tracer --worktrees W1,W2,W3,W4 \
    --arms codex-ptc-mm,codex-jsptc-qjs,grok-ptc-mm,grok-jsptc-qjs
cd conformance/codemode/real
python3 census.py RAW results/js-vs-python --machine "…"
python3 grade.py RAW > results/js-vs-python/grade.jsonl
python3 jsvpy.py results/js-vs-python
```

Sample 2 is the same with `results/js-vs-python-host1`. The pooled tables come from `jsvpy.py`
run on a directory holding both samples' `cells.jsonl`, `sessions.jsonl` and `grade.jsonl`
concatenated (its "correct" column counts per prompt, so the pooled 4 of 4 is from the two
`grade.jsonl` files).

`PLAKOUS_JS_HOST` is the real file behind `~/.local/bin/plakous-js-host` (`readlink -f`). Run
`plak run` with a minimal environment (`env -i HOME=… PATH=…`) so no secret reaches a tool.
