# codemode: a synthetic programmatic-tool-calling workload

198 tool-orchestration tasks with mocked tools, and 1,782 solutions written cold by models of two
families (sets `a`, `b`, `s` by Claude subagents; `luna-*` and `sol-*` by `openai/gpt-6-luna` and
`openai/gpt-6.1-sol`), one per task in each of three prompt styles. Programmatic tool calling (PTC,
Anthropic's name for it) means the model writes code that calls its tools as functions, rather than
calling one tool per turn. One prompt style follows py-ptc, the Python PTC presentation of plak
(<https://github.com/N0xMare/plak>, MIT), an open-source local agent harness by micro-monty's author
and its integration target: globals persist across cells, tools are async functions, and nothing is
type-checked. `run.py` runs every solution on
CPython 3.14.7 and on micro-monty through its real host-call path; `--check` fails if any outcome moves
from `results/runs.jsonl`. Numbers are [M] unless labelled.

## Run it

```bash
python3.14 conformance/codemode/run.py            # rerun, rewrite results/runs.jsonl and results/summary.md
python3.14 conformance/codemode/run.py --check    # regression benchmark: fail if any outcome moved
python3.14 conformance/codemode/run.py --filter b/par- --verbose   # a subset, non-passes printed in full
python3.14 conformance/codemode/timing.py         # where the time goes: engine vs host calls
```

The run builds `mm-codemode` (`runner/`) twice in release into `target/codemode/{default,full}`:

- `default`: the stdlib modules an embedder gets by default;
- `full`: plus every optional module (`--features optional-modules`, the same set `mm` enables).

It then runs every solution on the three engines in parallel. A full run takes about 20 s on 16 cores.

The run is deterministic and offline. Tools are mocked over fixture data built from fixed seeds
(`tasks/worlds.py`). The clock is pinned to 2026-03-15T12:00:00Z on both engines, sleeps are virtual,
and mm's entropy is a counter.

`results/summary.md` has every table regenerated, and `results/runs.jsonl` has one record per solution
(imports, from-import names, builtins, features, and each engine's outcome and first cause).

## How it works

| file | role |
|---|---|
| `ptc.py` | tool and task registry, value codec, transcript, answer comparison |
| `tasks/` | the fixture worlds and tools (`worlds.py`) and 198 tasks in 13 categories |
| `export.py` | renders the exact prompt each solution author saw (`export.py [--v1] OUT_DIR`) |
| `shim.py` | the CPython side: the tools as `async def name(arguments)`, top-level await |
| `runner/` | the micro-monty side: the Rust facade (see below) |
| `run.py`, `summary.py` | runs, AST scan, first-cause grouping, report |

**The tool convention** follows Anthropic's programmatic tool calling:

- Each tool is an async function that takes one dict and returns a string, and code parses the result
  with `json.loads`. Text tools such as `read_file` and `fetch_logs` return raw text.
- Set `s` uses the same tasks with tools that return parsed values (structured results).
- Every task with an answer has a `submit_answer` tool whose arguments are checked. Tasks with side
  effects (emails, files, incidents, comments) are checked from the tool-call transcript.

**micro-monty path:**

1. `runner/src/main.rs` defines host module `_ptc` with one untyped function per tool (`Session::define`).
2. A bootstrap cell binds `async def tool(arguments): return await _ptc.tool(arguments)`, as plak's
   bootstrap does.
3. The solution starts.
4. Every `Progress::HostCall` is sent to the harness over stdio and resumed with `Resume::Future`.
5. Every `Progress::Waiting` is answered with one `Resume::Batch`, so `asyncio.gather` yields one batch.

Both engines see byte-identical tool behaviour, because one `ToolHost` answers both.

**Pass rule:**

- A solution passes on an engine when it raises nothing and its checker accepts the transcript.
- A pass on mm counts only if CPython also passes.
- Solutions that fail on CPython are listed separately as task or solution errors.

**Same calls** (`same_calls`, reported beside the pass rule, not part of it) compares the tool calls of an mm
run with CPython's, batch by batch:
- **A batch** is the set of calls the cell issues between two deliveries of results. On mm that is the calls
  one `Waiting` collects, so one `asyncio.gather` fan-out is one batch, and the next batch holds the calls
  the resumed code issues. The CPython shim tags each call with the event-loop iteration of the latest
  result delivered before it, which gives the same grouping. Both engines send the tag with each call.
- **Within a batch**, calls (name, arguments and error) compare as a multiset, since `gather`'s children may
  run in either engine's order. **Across batches**, order is kept, so a call that moves to another batch, or
  any missing, extra or changed call, makes the runs differ.
- **Failed calls.** The shim raises a failed call's error after the same loop turn a result takes, as a
  failed host future surfaces on mm. It used to raise at once, which ran a retry before the other items'
  first calls.

## Tasks

| category | tasks | | category | tasks |
|---|---:|---|---|---:|
| API/DB aggregation | 17 | | parallel calls (`gather`) | 17 |
| log and text triage | 17 | | pagination | 12 |
| CSV/JSON reshaping | 17 | | retries and errors | 13 |
| date/time arithmetic | 17 | | numeric statistics | 17 |
| regex extraction | 17 | | URL/encoding/hashing | 16 |
| multistep workflows with early exit | 17 | | report formatting | 14 |
| negative habits | 7 | | | |

The negative tasks invite habits that reach past a small sandbox: a dataframe pivot, "make a GET
request", the output of `wc`, IANA time zones, a 40-way fan-out, dumping all logs, and a correlation
matrix. Their checkers accept any correct answer.

Retry tasks inject `TimeoutError`, 503 and 429 failures (`config['fail']`), and the checkers count
attempts. Early-exit and pagination checkers fail a solution that makes unnecessary calls.

## How the solutions were written

Each author saw only its prompt files, as `export.py` renders them: the task, the tool stubs and the
note "a sandboxed Python 3.14 environment; no filesystem or network except via tools", never
micro-monty or its limits, and wrote its first natural answer once without running anything. The
solutions are kept verbatim.

| set | prompt style | results |
|---|---|---|
| `a` | Anthropic PTC: JSON tool schemas, the PTC note quoted | strings |
| `b` | plak's py-ptc catalog: `async def name(arguments: {...}) -> str` signatures, plak's run-description wording | strings |
| `s` | style `a` with "returns the tool result as a Python value ... that needs no parsing" | structured |

Template version 1 (`export.py --v1`, the Claude sets) asks every task to finish with `submit_answer`,
including the 29 checker-graded tasks that do not define it; 34 solutions guard the call with
`if "submit_answer" in globals():`, which mm rejects. Version 2 (the GPT sets) asks for it only where
the task defines it.

`results/summary.md` has the outcome tables, the mm failure causes and the import census.
