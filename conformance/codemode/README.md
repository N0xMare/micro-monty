# codemode: a synthetic programmatic-tool-calling workload

This directory holds 198 tool-orchestration tasks with mocked tools, and 1,782 solutions written cold by
models from two families: 594 by Claude subagents (sets `a`, `b`, `s`), and 594 each by `openai/gpt-6-luna`
and `openai/gpt-6.1-sol` (sets `luna-*`, `sol-*`). Each family wrote one solution per task in each of three
prompt styles. It runs every solution on CPython 3.14.7 and on micro-monty through
its real host-call path. The results decide three things: which stdlib modules belong in the default
tier (docs/HANDOFF.md "Scope decisions" 1), which unsupported imports need redirect messages, and
where micro-monty fails code written by models. Numbers below are [M] (measured) unless labelled.

## Run it

```bash
python3.14 conformance/codemode/run.py            # rerun, rewrite results/runs.jsonl and results/summary.md
python3.14 conformance/codemode/run.py --check    # regression benchmark: fail if any outcome moved
python3.14 conformance/codemode/run.py --filter b/par- --verbose   # a subset, non-passes printed in full
python3.14 conformance/codemode/timing.py         # where the time goes: engine vs host calls (docs/research/interpreter-speed.md §4)
```

The run builds `mm-codemode` (`runner/`) twice in release into `target/codemode/{default,full}`:

- `default`: the stdlib modules an embedder gets by default;
- `full`: plus every optional module (`--features optional-modules`, the same set `mm` enables).

It then runs every solution on the three engines in parallel. A full run takes about 20 s on 16 cores.

The run is deterministic and offline. Tools are mocked over fixture data built from fixed seeds
(`tasks/worlds.py`). The clock is pinned to 2026-03-15T12:00:00Z on both engines, sleeps are virtual,
and mm's entropy is a counter. `--check` passed twice in a row on this build.

The census below was built from `origin/main` 119a97d plus this suite (binaries `default`
`e1e87adb…`, `full` `a657a42f…`). The first census ran on 5a139bf, and every one of its 605 outcomes was
unchanged then. `results/` now holds the rerun after the 2026-11-05 stdlib round ("After the stdlib
round" below), with binary SHA-256 values:

- `default`: `327a99c5bda4656669b158373ee107b1757dd5affc9308733d64c02640033730`
- `full`: `f91d20e318213815340e834094ddf8d840e070e0c5d10e4d535c06bbd074ef7b`

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
| negative habits (survey §8) | 7 | | | |

The negative tasks invite habits that reach past a small sandbox: a dataframe pivot, "make a GET
request", the output of `wc`, IANA time zones, a 40-way fan-out, dumping all logs, and a correlation
matrix. Their checkers accept any correct answer.

Retry tasks inject `TimeoutError`, 503 and 429 failures (`config['fail']`), and the checkers count
attempts. Early-exit and pagination checkers fail a solution that makes unnecessary calls.

## How the solutions were written

There were 24 subagents, each given about 25 tasks. Each saw only its prompt files, as rendered by
`export.py`: the task, the tool stubs, and the environment note "a sandboxed Python 3.14 environment;
no filesystem or network except via tools". None of them saw micro-monty or its limits. Each was told
to write its first natural answer once, without running anything.

| set | prompt style | results |
|---|---|---|
| `a` | Anthropic PTC: JSON tool schemas, the PTC note quoted | strings |
| `b` | plak's py-ptc catalog: `async def name(arguments: {...}) -> str` signatures, plak's run-description wording | strings |
| `s` | style `a` with "returns the tool result as a Python value ... that needs no parsing" | structured |

The solutions are kept verbatim; none was edited. Two task bugs found on CPython were fixed in the
tasks, not the solutions:

- `dt-outside-hours` expected id order where the prompt says start order.
- The `list_holidays` fixture lacked 2027.

**A prompt-template defect affects 34 solutions.** The template asks every task to finish with
`submit_answer`, including the 29 checker-graded tasks that do not define it. Models then guarded the call
with `if "submit_answer" in globals():`, which mm rejects. The report counts these solutions, but they
are prompt-induced. Rates without them are given beside the raw rates. Template version 2 fixes this (below);
`export.py --v1` still renders the original prompts byte for byte.

### Second model family

The Claude sets were written by subagents of one model, so the census was rerun with GPT models.

**Template version 2.** The template now asks for `submit_answer` only on the 169 tasks that define it.
The other 29 prompts end with the user request. Nothing else changed: on tasks with an answer, version 2
renders byte-identically to version 1.

**Who wrote them.** 24 separate Amp threads wrote the solutions: 12 in agent mode `openai/gpt-6-luna` and
12 in `openai/gpt-6.1-sol`. Each mode covered the three styles in four alphabetical chunks of 48 to 52
tasks. Set `luna-a` uses style `a`, and so on.
- **What each thread saw:** an orb with no project, a tarball of its chunk's prompts rendered by
  `export.py`, and the set-`a` or set-`b` instructions above. It did not see the existing solutions,
  micro-monty, or this repository.
- **One clarification:** the first instructions limited shell commands so strictly that some threads asked
  how to read and write files. All 24 then got the same note: they may read the prompts and write the
  outputs with their file tools, and may still run nothing.
- **Collection:** the files came back through Amp's file transfer and were copied verbatim; none was
  edited.
- **One gap:** the luna `b` chunk-1 thread skipped one file, `par-order-customers`, and wrote it on
  request; its other files are byte-identical to the first delivery.

No GPT solution uses `globals()`.

## Results

### After the stdlib round (2026-11-05)

The owner-approved stdlib items (docs/HANDOFF.md "Scope decisions": D2, J2, Z1, G2, T1, S1) moved
`decimal`, `statistics` and `tomllib` into the default build and added `json.loads` hooks, `html.parser`,
`calendar`, `hmac`, `zlib.crc32`/`adler32`, `utf-8-sig`, the asyncio additions, `zoneinfo` (the runner
serves `/usr/share/zoneinfo`, as plak would) and a read-only `globals()`. The default and full builds now
give the same results [M]:

| family | CPython pass | mm default (of CPython pass) | before | checkpoint projection [E] |
|---|---:|---:|---:|---:|
| Claude | 577 | 572 (99.1%) | 84.4% | about 92% (98% with G) |
| gpt-6-luna | 556 | 546 (98.2%) | 87.6% | about 97% |
| gpt-6.1-sol | 589 | 580 (98.5%) | 77.4% | about 97% |
| **all** | 1,722 | 1,698 (98.6%) | 83.0% | |

Every earlier pass still passes, all 1,698 make the same calls as CPython batch by batch, and there are
still 0 wrong outputs. What is left: `import xml` 18 (2 + 7 + 9), `import email` 3 (2 + 1 + 0) and
`__import__(...)` 3 (1 + 2 + 0). The tables below are the census before this round.

### Before the stdlib round

| family | solutions | CPython pass | mm default (of CPython pass) | mm full (of CPython pass) |
|---|---:|---:|---:|---:|
| Claude (`a`, `b`, `s`) | 594 | 577 (97.1%) | 487 (84.4%) | 514 (89.1%) |
| Claude without the 34 `globals()` | 543 | 543 | 487 (89.7%) | 514 (94.7%) |
| gpt-6-luna | 594 | 556 (93.6%) | 487 (87.6%) | 511 (91.9%) |
| gpt-6.1-sol | 594 | 589 (99.2%) | **456 (77.4%)** | 531 (90.2%) |
| **all** | 1,782 | 1,722 (96.6%) | 1,430 (83.0%) | 1,556 (90.4%) |

By set (Claude, as before):

| set | solutions | CPython pass | mm default (of CPython pass) | mm full (of CPython pass) |
|---|---:|---:|---:|---:|
| a | 198 | 198 | 171 (86.4%) | 179 (90.4%) |
| b | 198 | 198 | 162 (81.8%) | 172 (86.9%) |
| s | 198 | 181 | 154 (85.1%) | 163 (90.1%) |

The GPT sets per style are in `results/summary.md`; within a family the styles differ by up to 6.2 points.

- **Every mm failure is a refusal, for all three models.** There are 0 wrong outputs. Every failure is a
  missing module, builtin, codec, or (new with sol) `json.loads` hook.
- **Calls match CPython in every passing solution.** All 1,430 default-build and 1,556 full-build passes
  make the same calls as CPython, batch by batch ("Same calls" above). Before the batch rule, 28 GPT
  retry solutions under `gather` differed only in order: the shim ran a failed item's retry at once,
  while mm delivers a whole batch before any retry runs.
- **CPython failures are solution bugs:** Claude 17, luna 38, sol 5. Luna's include 7 more missing
  `import asyncio` (all set `luna-s`), 3 tools called as undefined names, `KeyError`s on result fields,
  a regex error and wrong answers. sol's are 3 `int()` parses of a malformed field, 1 wrong answer and 1
  `AttributeError`.

### mm failure causes (first cause, CPython-passing solutions)

By family; each cell is default build / full build:

| cause | Claude | gpt-6-luna | gpt-6.1-sol |
|---|---:|---:|---:|
| `decimal` not in default build | 2 / 0 | 5 / 0 | **52 / 0** |
| `statistics` not in default build | 22 / 0 | 16 / 0 | 35 / 0 |
| `html.parser` missing | 8 / 8 | 19 / 19 | 16 / 17 |
| `globals()` / `__import__` not defined | 34 / 34 | 2 / 2 (`__import__`) | 0 / 0 |
| `zoneinfo` missing | 6 / 6 | 7 / 7 | 9 / 9 |
| `xml` missing | 2 / 2 | 7 / 7 | 9 / 9 |
| `json.loads(..., parse_float=Decimal)`: `NotImplementedError: json.loads hooks are not supported` | 0 / 0 | 0 / 0 | 0 / **14** |
| `calendar`, `zlib`, `hmac` missing | 9 / 9 | 9 / 9 | 9 / 9 |
| `tomllib` not in default build | 3 / 0 | 3 / 0 | 3 / 0 |
| `email` missing | 2 / 2 | 1 / 1 | 0 / 0 |
| `utf-8-sig` codec missing | 2 / 2 | 0 / 0 | 0 / 0 |

The 14 hook failures are hidden behind `import decimal` on the default build. The extra `html.parser`
failure on sol's full build is the same effect. The Claude rows below are unchanged from the first
census.

| cause (Claude) | default | full | minimal repro (raises on mm, works on CPython) |
|---|---:|---:|---|
| `globals()` not defined (prompt-induced, see above) | 34 | 34 | `"x" in globals()` → `NameError` |
| `statistics` not in default build | 22 | 0 | `import statistics` |
| `html.parser` missing | 8 | 8 | `from html.parser import HTMLParser` |
| `zoneinfo` missing | 6 | 6 | `from zoneinfo import ZoneInfo` |
| `calendar` missing | 3 | 3 | `import calendar` (`monthrange`) |
| `zlib` missing | 3 | 3 | `import zlib` (`crc32`) |
| `hmac` missing | 3 | 3 | `import hmac` |
| `tomllib` not in default build | 3 | 0 | `import tomllib` |
| `utf-8-sig` codec missing | 2 | 2 | `b"\xef\xbb\xbfhi".decode("utf-8-sig")` → `LookupError` |
| `xml` missing | 2 | 2 | `import xml.etree.ElementTree` |
| `decimal` not in default build | 2 | 0 | `import decimal` |
| `email` missing | 2 | 2 | `import email.utils` (`parsedate_to_datetime`) |

Some failures are hidden behind an earlier cause, because only the first cause counts:

- `__import__` (1 solution, behind zoneinfo) raises `NameError`.
- `asyncio.Semaphore` (1 solution, on an unreached retry path) raises `AttributeError`.

Each repro above was run through `target/codemode/*/release/mm-codemode`, and each gave the error shown.

By category, mm default passes every CPython-passing solution in aggregation, log triage, pagination,
retries and the negative tasks. The weakest categories are:

| category | mm default | mm full | main causes |
|---|---:|---:|---|
| numeric statistics | 31/51 | 51/51 | `statistics` |
| report formatting | 27/41 | 27/41 | `globals()` |
| multistep workflow | 37/51 | 37/51 | `globals()` |
| URL/encoding/hashing | 38/48 | 38/48 | `zlib`, `hmac`, `html.parser`, `utf-8-sig` |
| regex extraction | 40/51 | 40/51 | `html.parser`, `xml`, `email`, `globals()` |

## Import census

**By family.** Share of each family's 594 solutions importing a module, for every module that any family
imported in more than 1%, or that differs between families:

| module | tier | Claude | gpt-6-luna | gpt-6.1-sol |
|---|---|---:|---:|---:|
| json | default | 55.6% | 55.6% | 54.7% |
| asyncio | default | 37.9% | 32.9% | 38.0% |
| re | default | 19.7% | 16.4% | 15.7% |
| collections | default | 14.8% | 13.8% | 15.8% |
| datetime | default | 11.8% | 11.5% | 13.5% |
| **decimal** | **optional** | 0.3% | 0.8% | **9.1%** |
| csv / io | default | 5.7% | 6.7% | 6.1% |
| **statistics** | **optional** | 3.9% | 2.9% | **6.2%** |
| urllib.parse | default | 4.5% | 4.4% | 4.5% |
| hashlib | default | 3.0% | 3.0% | 3.0% |
| html.parser | **unsupported** | 1.3% | **3.2%** | 2.9% |
| math | default | 2.4% | 3.0% | 1.9% |
| base64 | default | 2.5% | 2.5% | 2.5% |
| html | default | 3.0% | 1.7% | 2.0% |
| zoneinfo | **unsupported** | 1.3% | 1.5% | 1.5% |
| xml.etree.ElementTree | **unsupported** | 0.3% | 1.5% | 1.5% |
| tomllib | optional | 0.5% | 0.5% | 0.5% |
| calendar, zlib, hmac, email.utils, textwrap | | 0.5% each | 0.5% each | 0.5% each |

- **The families agree on the default tier.** The top seven modules match within a few points.
- **sol reaches for `decimal`.** 54 of its 594 solutions import it, mostly for money totals
  (`Decimal(str(x))` in 43 of them). 15 of them parse tool results with `json.loads(raw, parse_float=Decimal)`.
- **The GPT models parse HTML and XML with the stdlib parsers** more than Claude did: `html.parser` 19
  and 17 solutions, `xml` 9 and 9.
- **Nobody imports third-party packages,** not even on the negative tasks. That holds for 1,782
  solutions, and also covers subprocess, threading and typing_extensions.
- **The GPT models use no `asyncio.Semaphore`, `create_task`, `TaskGroup`, `as_completed` or
  `wait_for`.**
- `__import__("json")` inline appears in 2 luna solutions.

**Claude** (the first census), share of its 594 solutions. Tiers are those of this build.

| module | share | tier | | module | share | tier |
|---|---:|---|---|---|---:|---|
| json | 55.6% | default | | html.parser | 1.3% | **unsupported** |
| asyncio | 37.9% | default | | zoneinfo | 1.3% | **unsupported** |
| re | 19.7% | default | | calendar | 0.5% | **unsupported** |
| collections | 14.8% | default | | email.utils | 0.5% | **unsupported** |
| datetime | 11.8% | default | | hmac | 0.5% | **unsupported** |
| csv | 5.7% | default | | zlib | 0.5% | **unsupported** |
| io | 5.7% | default | | tomllib | 0.5% | optional |
| urllib.parse | 4.5% | default | | textwrap | 0.5% | default |
| statistics | 3.9% | **optional** | | decimal | 0.3% | optional |
| hashlib | 3.0% | default | | xml.etree.ElementTree | 0.3% | **unsupported** |
| html | 3.0% | default | | binascii | 0.2% | default |
| base64 | 2.5% | default | | shlex | 0.2% | default |
| math | 2.4% | default | | | | |

Third-party packages (pandas, numpy, requests and the like) were never imported, not even on the
negative tasks. Neither were subprocess, threading or typing_extensions. Under a sandbox note and a PTC
framing, these models wrote the dataframe pivot with `csv`, the correlation matrix by hand, and `wc` with
`str` methods.

The most used names were `json.loads` 321, `asyncio.gather` 208, `re.compile` 79, `collections.Counter`
63, `datetime.datetime` 45, `datetime.timezone` 40, `asyncio.sleep` 39 and `io.StringIO` 34. The full
list is in `results/summary.md`.

**Modules the survey named** (docs/research/codemode-survey.md §7 R-T), by solutions importing each:

| module | solutions | | module | solutions |
|---|---:|---|---|---:|
| statistics | 23 | | struct | 0 |
| tomllib | 3 | | platform | 0 |
| decimal | 2 | | difflib | 0 |
| uuid | 0 | | pprint | 0 |
| unicodedata | 0 | | logging | 0 |
| fractions | 0 | | codecs | 0 |
| html (`html.unescape`) | 18 | | traceback | 0 |
| html.parser (unbundled) | 8 | | warnings | 0 |
| zoneinfo (unbundled) | 8 | | gc | 0 |

**Builtins and language features.** The language surface fully passes on mm:

- comprehensions, generator expressions, lambdas, starred calls, f-strings with format specs, `try`;
- `async def`, `async with`, walrus, `yield`, `class` with `super()`, `isinstance`, `divmod`.

The builtins that fail are `globals` (34 solutions) and `__import__` (1). The asyncio name that fails
is `Semaphore` (1, unreached here).

## String vs structured results

| mode | solutions | CPython pass | mm default | mm full |
|---|---:|---:|---:|---:|
| string (`a`, `b`) | 396 | 396 | 333 (84.1%) | 351 (88.6%) |
| structured (`s`) | 198 | 181 | 154 (85.1%) | 163 (90.1%) |
| string, without `globals()` | 369 | 369 | 333 (90.2%) | 351 (95.1%) |
| structured, without `globals()` | 174 | 174 | 154 (88.5%) | 163 (93.7%) |

- **mm handles both modes alike.** No failure depends on the mode. Every cause above occurs in both.
- **Structured results remove most parsing.** `json.loads` appears in 311 of 396 string solutions and 10
  of 198 structured ones (defensive `isinstance(x, str)` checks).
- **CPython's lower structured rate is not about the mode.** It comes from one author's missing
  `import asyncio`.

## Concurrency, output size and probes

**Concurrency.** The most host futures outstanding in a single `Waiting`, per solution. The count is the
larger of the two builds, since a solution that fails on an import in the default build never fans out
there. The first census read the default build only, which is why Claude's 0-1 count moved from 411 to 400.

| outstanding | Claude | gpt-6-luna | gpt-6.1-sol |
|---|---:|---:|---:|
| 0-1 | 400 | 432 | 398 |
| 2-16 | 186 | 153 | 188 |
| more than 16 | 8 | 9 | 8 |

The GPT solutions above 16:
- **`neg-fanout-40`:** 40 outstanding, 6 of 6 sets.
- **`par-many-calls`:** 22, 5 sets.
- **`wf-close-fixed`:** 17, 3 sets.
- **luna-a only:** `rep-ascii-chart` (171 outstanding; the solution then fails on CPython too),
  `rep-daily-errors` and `retry-logs` (57 each). These fetch one day per call across the whole range.

None of them bounds concurrency. All three families exceed 16 on the same tasks, so plak's 16-call bound
would break 1.3 to 1.5% of solutions in each family unless extra calls queue.

The Claude solutions above 16 come from three tasks:

- `neg-fanout-40`: 40 outstanding, all three authors (one added a Semaphore on a retry path);
- `par-many-calls`: 22 outstanding, sets `a` and `b`;
- `wf-close-fixed`: 17 outstanding, all three sets (one `get_issue` per in-progress issue).

None of these prompts asked for bounded concurrency. mm itself has no outstanding-call bound. A host such as plak (16
concurrent nested calls [S]) would need to queue the extra calls; failing them would break these
solutions.

**Output size.** The largest stdout was 197 KB (`neg-dump-logs`, which asks for every log). That is under
plak's 1 MiB bound [S] but past a 16 KiB observation bound. Claude printed everything in 1 of 3 sets, luna in
2 of 3, and sol in 3 of 3. Apart from `neg-dump-logs`, no solution in any family printed more than 2.7 KB.

**Probes** (`solutions/p/`) are 11 hand-written one-liners, kept out of every count above. Each records
the error message on both engines:

- **Same message on both engines:** `import pandas` / `numpy` / `requests` / `yaml`, a misspelled module
  attribute (`json.lods`), a misspelled tool (`get_wether`), indexing a string result as a dict, and a
  forgotten await (`json.loads(get_weather(...))` gives `TypeError: ... not coroutine`). CPython's
  "Did you mean" hint lives in its traceback rendering, not in the message, and this harness compares
  messages only.
- **Works on CPython, fails on mm with no hint:** `from zoneinfo import ZoneInfo`, `import subprocess`
  and `asyncio.create_task`.

## Recommendations from the data

The second family confirms most of the first census and changes three things:
- **`decimal` becomes a promotion candidate,** where it was "opt-in".
- **`json.loads` hooks are a new gap** to fill.
- **`globals()` drops to low priority.**

| recommendation | Claude | gpt-6-luna | gpt-6.1-sol | verdict |
|---|---|---|---|---|
| promote `statistics` | 22 default failures | 16 | 35 | **holds, stronger** |
| promote `tomllib` | 3 (every TOML task) | 3 | 3 | holds (weak) |
| keep `decimal` opt-in | 2 | 5 | **52, sol's largest cause** | **changed**: promote, or accept a 6.5-point loss on sol-like models |
| add `html.parser` | 8 | 19 | 17 | holds; now the largest missing module (44 solutions) |
| add `zoneinfo`, `xml`, `calendar`, `zlib`, `hmac` | 17 | 23 | 27 | holds |
| read-only `globals()` | 34 (prompt-induced) | 0 | 0 | **dropped**: 0 uses once the template is fixed; `__import__` (2) is the same size |
| new: `json.loads` hooks (`parse_float`) | 0 | 0 | 14 | add `parse_float`/`parse_int`/`object_hook`; needed once `decimal` is on |
| queue calls past a bound (R-H6) | 8 above 16 | 9 | 8 | holds |
| bounded output | 1 of 3 | 2 of 3 | 3 of 3 | holds, stronger |
| asyncio `Semaphore`/`create_task` | 1 | 0 | 0 | holds as low priority |

Promotion arithmetic, as default-build pass rates of CPython-passing solutions [M, computed from
`results/runs.jsonl`]:

| default build | Claude | gpt-6-luna | gpt-6.1-sol |
|---|---:|---:|---:|
| today | 84.4% | 87.6% | 77.4% |
| plus `statistics` and `tomllib` | 88.7% | 91.0% | 83.7% |
| plus `decimal` too | 89.1% | 91.9% | 90.2% |

`decimal` costs 42.5 KB of bytecode with its dependencies [S: survey]. The question is whether that is
worth 6.5 points on sol-like code. The hooks have to come with it: without them, 14 of sol's 52 decimal
solutions still fail.

The first census's text follows unchanged.

**Default tier.**

- **Keep:** json, asyncio, re, collections, datetime, csv, io, urllib.parse, hashlib, html, base64,
  math, textwrap, binascii and shlex. All were used, and each covers a whole task category.
- **Promote `statistics`.** It was used in 3.9% of solutions, ahead of hashlib and base64, and it is
  the largest single cause of default-build failures (22). All 51 statistics solutions pass once it is
  enabled.
- **Promote `tomllib`, weakly.** It was used in 3 solutions, every time a TOML file was offered, and
  its bytecode is small (10.4 KB [S: survey]).

**Opt-in.**

- `decimal`: 2 solutions, both reachable with `round`. It is the largest module (42.5 KB [S]).
- `uuid`, `fractions`, `unicodedata` and `html.entities`: 0 uses here. The suite has no id-minting or
  Unicode-normalization task, so this is weak evidence against promoting them.

**Census-gated.** struct, platform, difflib, pprint, logging, codecs, traceback, warnings and gc had 0
imports in 594 solutions. The data supports moving them to opt-in if size matters. The workload has no
binary-format or long-program tasks, so treat this as "unused in tool orchestration", not "never used".

**Missing modules worth adding**, in order of solutions affected:

| module | solutions | covers |
|---|---:|---|
| `html.parser` | 8 | link and meta scraping |
| `zoneinfo` | 6 | needs host-supplied tzdata; `set_local_timezone` already takes TZif bytes |
| `calendar.monthrange` | 3 | |
| `zlib.crc32` | 3 | |
| `hmac` | 3 | small; hashlib already exists |
| `email.utils.parsedate_to_datetime` | 2 | RFC 2822 dates in RSS |
| `xml.etree.ElementTree` | 2 | |

Also add the `utf-8-sig` codec (2 solutions).

**Missing builtins.** Models reach for `globals()` to test whether an injected name exists (34
solutions, though prompt-induced). A read-only `globals()` returning a snapshot dict would cover that
use without the writable-namespace decision (HANDOFF scope decision 3).

**Redirect messages**, appended after CPython's `No module named 'x'`:

| imports | data | suggested message |
|---|---|---|
| `zoneinfo` | 8 imports | point at `datetime.timezone` / fixed offsets, unless the host supplies zones |
| `html.parser`, `xml`, `email` | 12 imports | "use `re` and `html.unescape` on the text" |
| `calendar`, `zlib`, `hmac` | until added | name the bundled alternative (`hashlib` for hmac) |
| a module behind a disabled feature | statistics, tomllib, decimal | "'statistics' is not enabled in this build of micro-monty" |
| `asyncio.create_task` / `Semaphore` | probe; 1 solution | "micro-monty's asyncio has run, gather and sleep" |
| pandas, numpy, requests, subprocess | 0 natural imports; probes only | the survey's R-M texts; the lowest priority by this data |

## Limits of this measurement

- **The authors are two model families.** These are Claude subagents and two GPT models under three
  prompt styles, not real transcripts. sol's `decimal` habit shows that one family can move a tier
  decision on its own; a third family could do the same.
- **Tasks are short.** Most need one cell of 10 to 60 lines. Long programs, classes and binary
  formats are rare.
- **Only messages are compared.** Messages are compared, tracebacks are not, so "Did you mean"
  parity is not measured.
- **The mm runner uses `Resume::Future` for every call.** Synchronous host calls are not exercised,
  and models never call tools without `await` under this convention.
- **Batches are matched by construction, not observed in CPython.** CPython has no batches. The shim
  derives them from loop iterations, which match mm's `Waiting`s on every solution here but could split
  differently for code that starts tasks with `create_task` outside a `gather`.
