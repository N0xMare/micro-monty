# What codemode runtimes support, and what micro-monty should ship

Commit hashes of micro-monty cited here, other than the public history's
(root `2bcbb8c`), are pre-release history, kept in the private archive
`N0xMare/micro-monty-archive` (branch `history-0.0.1`;
[docs/history](../history/README.md) maps them); the numbers they qualify
are unchanged.

Research for three decisions (docs/HANDOFF.md, "Scope decisions"): the
stdlib's default tier, the messages a cell sees when it imports something
unsupported, and improvements to the host-tool API. Nothing in the engine
changes here.

Labels: [S] sourced (a file at the commit below, or a vendor page read on
2026-11-05), [M] measured in this orb on `main` @ `dbd0bf8`, [E] estimated or
inferred. "pi census" means djs's static analysis of 2,535 distinct codemode
scripts from one user's pi sessions ([djs-usage]); those scripts are
JavaScript, so their Python equivalents are [E].

## Sources

| Runtime | Repository or page | Commit or date |
|---|---|---|
| plak | `N0xMare/plak` | `36661c21f48de51ec63697f0c1a8c0675dd62d28` (2026-11-04), Monty pinned `=1.2.0` |
| Monty | `pydantic/monty` | tag `1.4.0` = `4b015dc4a1e08394b0a98e0255ed8c81de1fbbf6`; HEAD `2a3269043e544d0e7bd99e3e5cf0c306a8246076` (2026-11-04) |
| Pydantic AI Code Mode (runs Monty) | `pydantic/pydantic-ai` | `1eaf70020560980e8916089d846c6a1bb6accccd` |
| pi codemode (QuickJS) | `earendil-works/pi` | `d57c2d21916eaad1158031526ac6ec860eae5f14` |
| djs (pi's experimental engine) | `earendil-works/djs` | `05672101b697076a138532888d3c28937f858f2f` |
| Anthropic PTC and code execution | platform.claude.com docs ([ptc], [ce]) | read 2026-11-05 |
| Cloudflare codemode | `cloudflare/agents`, `packages/codemode` | `a2f1216993aa43982ef3550ec0a05e9a4d6d3175` |
| smolagents | `huggingface/smolagents` | `96f33faaf028479119ec8d34507b47694cf14e34` |
| oh-my-pi (OMP) | `can1357/oh-my-pi` | `88a69582c755bd000e8dc0290981fc787a4f400c` (2026-11-04) |

pi's prelude at `d57c2d2` is byte-identical to the copy djs vendors from
pi-private `d65de9db` (`tests/pi/prelude-source.ts`) [M, `diff`].

## In short

- **Two calling conventions.** Anthropic and plak's py-ptc give each tool
  one `dict` argument and a `str` result that the model `json.loads` [S].
  Pydantic AI, smolagents and plak's SDK use keyword arguments and
  structured results [S]. Every Python harness tells the model to use
  top-level `await` and `asyncio.gather`.
- **Module lists differ, and prompts drift from them.** Monty has 26
  modules at 1.4.0 and 28 at HEAD [S]. Pydantic AI's prompt advertises 25 of
  them [S]. Monty's own web-scraper example still tells the model that
  `json`, `re`, `collections` and `math` are unavailable [S]. Anthropic's
  container is full CPython 3.11 with pandas, numpy and about 30 more
  packages [S], and that is the environment Claude's PTC habits come from.
- **What scripts actually use is narrow** (pi census, JavaScript [S]):
  awaited tool calls in 98% of scripts, `Promise.all`/`allSettled` in 28%,
  JSON, string and array methods, regexes with `g`/`i`, dates and `Set`. No
  script uses classes, generators, Proxy, BigInt or typed arrays. pi's own
  prelude uses many of the features scripts never touch.
- **Failures should say what already happened.** Two harnesses do this.
  pi appends "Tool calls made before the failure (they are not undone)".
  Pydantic AI appends the started calls with their status [S].
- **micro-monty today** [M]: an unknown import raises a bare
  `ModuleNotFoundError: No module named 'numpy'`. `asyncio` has only `run`,
  `gather` (with `return_exceptions`, which Monty lacks) and `sleep`.
  Unawaited coroutines raise no warning. A misspelled module attribute
  gets no "Did you mean" suggestion. The spec's "host-configurable hint"
  for unsupported features (§4) is not in the facade.

- **oh-my-pi runs unsandboxed CPython and Bun** for its `eval` tool [S].
  Python is a plain `runner.py` subprocess on the user's interpreter, not
  Jupyter. It has no isolation and no durability; a dead kernel loses its
  state. OMP accepts a dict or keyword arguments, returns tool-reported
  errors as values, and puts no cap on outstanding calls. Its output keeps
  head and tail, with the full text spilled to an artifact (§1, "oh-my-pi").
- **The census now covers two model families**
  (conformance/codemode/README.md). `statistics` and the missing parsers
  hold up. `openai/gpt-6.1-sol` imports `decimal` in 9.1% of its solutions,
  so `decimal` is now a promotion candidate (§9, question 2).

Recommendations are in the last section. The headlines:
- Promote `statistics`, `tomllib` and `uuid` to the default tier.
- Keep `decimal` and `unicodedata` opt-in, pending the census and a size
  measurement.
- Add an import-redirect table that hosts can override.
- Add a per-cell host-call log for failure reports.
- Add structured host errors, a never-awaited warning, and a build-derived
  environment summary for prompts.

## 1. Per-runtime summaries

### plak (consumer; read only)

plak has three Python-facing surfaces on one Monty 1.2.0 worker, plus a
JavaScript one for comparison [S] ([plak-spec-sel]).

**py-ptc** (`exec_py`/`wait_py`).
- The bootstrap wraps each nested tool as
  `async def name(arguments): return await _plakous_host_N(arguments)`
  ([plak-render-py]).
- The model is told [S] ([plak-ptc], [plak-text]):
  - structured tools take a dictionary; freeform tools take the string
    shown in their signature;
  - "Tool results are strings";
  - "Calling without awaiting or scheduling creates an inert coroutine;
    use import asyncio and await asyncio.gather(...)", with at most 16
    outstanding calls;
  - "Cells cannot open files, scan directories, glob or walk paths, read
    the environment, use the network or sleep (those calls raise); use the
    nested tools";
  - "Globals and imports persist" until a reset;
  - "Printed lines and the last expression are both visible".
- Limits [S]:
  - `max_output_bytes` defaults to 1 MiB, and `exec_py` yields after at
    most 10 s ([plak-text]);
  - the source may be at most 256 KiB;
  - each compute segment is bounded at 90 s, and the bound is enforced by
    killing the worker ([plak-spec-limits]).
- Errors [S]:
  - a nested failure raises `RuntimeError` with the host's text, cut to
    16 KiB ([plak-codec]);
  - a bad argument shape raises
    `TypeError: Python tool requires exactly one dictionary argument`;
  - the 17th outstanding call raises `RuntimeError: local composition
    exceeds its profile-bound limit of 16 outstanding nested calls`;
  - arguments deeper than 40 levels or larger than 1 MiB raise
    `ValueError` ([plak-cell], [plak-spec-limits]);
  - OS calls are refused. A directory scan raises `RuntimeError:
    filesystem access is not available in cells; use the nested tools`;
    the other OS calls get Monty's `NotHandled` ([plak-cell-os]).

**SDK** (`run_py`, `PythonHarnessSdkV1`).
- Every cell runs in fresh globals, after a `Reset`.
- The bootstrap builds `namedtuple` records: `File`, `Listing`, `Hits`,
  `ProcResult`, `Ref`, `Omission`, `Job`, `Step`, ...
- Namespaces are classes whose bodies hold `async def` functions:
  `ctx.read/list/grep`, `fs.apply` with `Patch/Replace/Write/Delete/Move`
  constructors, `proc.run/shell/start/poll/stdin/cancel/join`,
  `state.get/put/list/drop` and `plan.update` ([plak-sdk-api]).
- Results come back typed as `CellValue::Record` values.
- The wrapper `_sdk_unwrap` maps host error records:
  `kind == 'value'` → `ValueError`, `'quota'` → `OverflowError`, anything
  else → `RuntimeError` ([plak-sdk-unwrap]).
- The prompt says "A type error means nothing ran", "Preserve omission
  notes and use ctx.read on an available Ref for omitted output", and "Use
  a File's revision as fs operation expect for compare-and-swap edits"
  ([plak-sdk-prompt]).
- Limits [S]:
  - each model-visible observation is cut to 16 KiB, ending with
    `[N output bytes omitted; full output: Ref('id')]`
    ([plak-sdk-render]);
  - 64 Refs and 16 MiB of retained text;
  - 1 MiB per result, 16 MiB per cell;
  - 300 s foreground ([plak-sdk-limits]).
- Type checking runs inside the Monty worker against generated stubs.
  The error reads "type errors; nothing ran" ([plak-cell]).

**JS PTC** (`exec_js`, rquickjs). Tools are `await tools.name({...})` and
output is `text(...)` ([plak-ptc]).

**What micro-monty needs in order to replace Monty in plak**
(see also docs/research/plak-integration.md):

1. *Identity.* plak checks the worker's `Hello` against
   `engine monty 1.2.0` / `plakous.monty-python-adapter.v9`
   ([plak-protocol]), and `EngineConfig` has no micro-monty variant. That
   is a plak-side change; today `mm-cell-worker` answers as Monty through
   a local override.
2. *SDK presentation.* plak sends `sdk_stubs` and expects in-worker type
   checking. `mm-cell-worker` refuses both `sdk_stubs` and `MontySdk`
   ([mm-worker], lines 118–139 and 525–536). Choose between a host-side
   checker over `Session::stubs()` and dropping "nothing ran" for runtime
   `TypeError`s at the call (spec §1 non-goals already chooses the second
   for the engine).
3. *The runtime contract.* `LocalCompositionRuntimeContract::PYTHON` is
   `wait_resumes_cell: false`, because Monty cannot park
   ([plak-contract]). Pause-anywhere makes `true` possible, which also
   gives `Cancel` without losing the generation and a 90 s bound by
   parking instead of killing. This is the main payoff, and it needs a plak
   change.
4. *Worker bounds to keep*: 16 outstanding calls, depth 40, 1 MiB per
   argument, nested errors cut to 16 KiB, `<int with N bits>`
   rendering. `HostArgs` and `set_int_bits_text` already cover the engine
   side [S, docs/sdk.md].
5. *OS refusals.* The engine's default text is host-facing:
   `PermissionError: open() is not available: this sandbox has no ambient
   filesystem; the host must provide an 'open' function` [M]. plak's
   prompt and its scan refusal use model-facing text instead. See R-H1.
6. *Typed functions and host classes* are not on the cell protocol yet
   (docs/sdk.md, "Remaining SDK gaps"). The SDK's records and namespaces
   would map onto them.

### Monty 1.4.0 and HEAD

- **Modules.** Monty bundles 26 modules at 1.4.0 and 28 at HEAD, which
  adds `annotationlib` and `fnmatch` [S] ([monty-index-140],
  [monty-modules]):
  - `asyncio`, `base64`, `binascii`, `bisect`, `collections`, `copy`, `csv`;
  - `dataclasses`, `datetime`, `decimal`, `functools`, `hashlib`,
    `itertools`, `json`, `math`;
  - `os`, `os.path`, `posixpath`, `pathlib`, `random`, `re`, `sys`, `time`;
  - `tomllib`, `typing`, `unicodedata`.

  Most cover only part of the CPython surface:
  - `collections` has only `deque`, `Counter`, `defaultdict` and
    `namedtuple`;
  - `functools` has only `reduce`, `partial`, `lru_cache` and `cache`;
  - `dataclasses` has `eq=` and `frozen=` only, and no `fields()` or
    `asdict()`.

  Missing names are absent rather than stubbed.
- **Notably absent** [S] ([monty-modules], "Notable modules NOT
  available"): `abc`, `argparse`, `array`, `contextlib`, `enum`,
  `fractions`, `heapq`, `hmac`, `http`, `inspect`, `io`, `logging`,
  `operator`, `pickle`, `queue`, `string`, `struct`, `tempfile`,
  `traceback`, `unittest`, `urllib`, `uuid`, `warnings`, `weakref`,
  `zipfile` and `zlib`. `socket`, `subprocess`, `multiprocessing`,
  `threading` and `ctypes` are absent by design.
- **Language.**
  - Rejected at parse time with `NotImplementedError: The monty syntax
    parser does not yet support ...` ([monty-index-140],
    [monty-parse]): class inheritance, metaclasses, method decorators
    (`@property` and friends), `yield`, `del`, `except*`, `async with`,
    `async for` and wildcard imports.
  - No user exception classes and no function attributes.
  - `map`, `zip` and `filter` are eager.
  - `assert` produces pytest-style messages.
  - `re` uses `fancy-regex`, without `bytes` patterns or `VERBOSE`.
- **asyncio** is `run`, `gather` and `sleep` only [S]
  ([monty-asyncio]). `gather` refuses every keyword:
  `NotImplementedError: gather() does not yet support keyword arguments`.
  So `return_exceptions=True`, Python's `Promise.allSettled`, does not
  exist. `create_task`, `wait_for`, `Semaphore`, `TaskGroup`, `timeout`
  and `as_completed` raise `AttributeError`.
- **Imports** of unknown modules are asked of the host. The host may
  answer with a host module or a `.py` file; otherwise the import raises
  `ModuleNotFoundError: No module named 'x'` [S] ([monty-modules]).
- **Prompting** [S]:
  - The docs recommend type checking "rather than memorising this page",
    with the same stubs passed to the model and to the checker. Their
    stated reason: "A model that writes `import statistics` produces code
    that is perfectly valid CPython and completely unrunnable here"
    ([monty-typecheck]).
  - The `web_scraper` example prompt lists the restrictions
    ([monty-example]). It says "you cannot use the standard library
    except ... `sys`, `typing`, `asyncio`", "`json`, `collections`, `re`,
    `math`, `datetime`, ... are NOT available", "you cannot define
    classes", "The last expression evaluated is the return value", and
    "use `asyncio.gather`". Its module list is stale against Monty
    itself.

### Pydantic AI Code Mode (Monty in production)

- **One `run_code` tool** over the sandboxed tools [S] ([pai-desc],
  [pai-docs]).
- **The prompt** says [S]:
  - "No third-party libraries", followed by an explicit list of 25
    importable modules (Monty HEAD's 28 minus `annotationlib`, `fnmatch`
    and `posixpath`);
  - "These functions are already defined in the sandbox ... All
    parameters are keyword-only";
  - "Calling without `await` returns an unresolved future, not the value";
  - "first `import asyncio`, then use `await asyncio.gather(...)` with
    positional awaitables. Monty does not support `asyncio.gather` keyword
    arguments or other task creation and wait APIs";
  - "No `import *`";
  - "The last expression's value is automatically captured as the return
    value", with print output returned as `{'output': ..., 'result': ...}`.

  A comment in the source records why the `import asyncio` instruction
  sits in its own paragraph: "next to 'without importing them', models
  drop `import asyncio` and the first snippet fails the type check"
  ([pai-desc]).
- **Tools** render as `async def` stubs with TypedDict arguments and
  results built from the JSON schemas. Tools without a return schema get
  `-> Any` and a `CodeModeReturnSchemaWarning` [S].
- **Limits** [S] ([pai-docs]):
  - 30 s execution and 256 MiB heap per snippet, with time spent waiting
    on tools excluded;
  - 100 nested calls per `run_code`, refused at the call site;
  - 10 MiB of print output;
  - Monty's `max_suspensions`, 1,000 per session.
- **Errors** become `ModelRetry` messages [S] ([pai-errors]):
  - `Syntax error in code:`, `Type error in code:` and `Runtime error:`,
    each followed by Monty's display;
  - on a limit or a crash, the list of started nested calls with what each
    was called with and whether it returned, did not finish or was denied;
  - after a timeout, "The code ran longer than `max_duration_secs` and was
    stopped, so the session was reset. Re-run any imports, recreate any
    state you need, and make the code do less work per call."

### pi codemode (QuickJS) and djs

**pi on quickjs-wasi** [S] ([pi-doc], [pi-tool], [pi-prelude]).
- **Input.** Raw JavaScript, run as the body of an async function, so
  top-level `await` and `return` work. The sandbox has no Node APIs, file
  system, network or timers. An optional first line
  `// @options: {"max_output_tokens", "timeout_ms"}` sets limits.
- **Globals.**
  - `tools.<name>(args)` takes one object. It resolves to a string, or to
    a structured value when the tool declares an output schema; `bash`
    resolves to `{ output, truncated, full_output_path?, exit_code, ... }`.
    It rejects with an `Error` that carries the tool's text and its own
    properties.
  - Output: `text()`, `image()`, `console.*` and `return`. `exit()` ends
    the script.
  - `store`/`load` keep JSON values across scripts: 256 KiB per value,
    1 MiB in total, and failed scripts publish no writes.
  - Discovery: `ALL_TOOLS`, `searchTools` (BM25), `describeTool`,
    `describeNamespace`.
  - `models.classify`/`generateImages` run with at most four in flight;
    "more calls wait for a free slot, so `Promise.all()` over many items
    is fine".
- **Prompt guideline.** "Use codemode to batch independent tool calls
  (Promise.allSettled), chain them, or filter large output, instead of
  many separate calls" ([pi-tool]).
- **Limits.**
  - 256 MB heap;
  - output capped at 16 Mi characters or 100,000 items, past which the
    script fails;
  - `max_output_tokens` defaults to 10,000. Longer output keeps its start
    and end, and the full text goes to a temp file whose path is in the
    result;
  - at most two workers per process.
- **Errors.**
  - The result starts with `Script completed` or `Script failed`, the wall
    time, and the output.
  - A failure appends `Script error:` with the stack, then
    `Tool calls made before the failure (they are not undone): read
    (ok), bash (error), ...`; a call's status is `running`, `ok`, `error`
    or `cancelled` ([pi-execute]).
  - A misspelled tool throws `TypeError: tools.x does not exist. Did you
    mean tools.y? ... ALL_TOOLS lists every tool; searchTools(query) finds
    tools by topic.`; with 20 or fewer tools it lists `Available: ...`
    ([pi-prelude], lines 227–244).
  - A script that waits on nothing fails with `The script is waiting on a
    promise that can never settle: no tool call is pending, and timers do
    not exist here.`
  - Out of memory is `InternalError: out of memory`.

**djs** [S] ([djs-readme], [djs-host]) is a `no_std` Rust engine for
wasm32 with one byte arena and suspend-anywhere.
- It runs pi's unmodified prelude behind pi's VM contract
  (`js/pi/vm.ts`) and passes 221 of 226 of pi's codemode tests.
- Interrupts are polled every 10,000 fuel units. Prelude images are
  reused, which brings a start down to 0.6 ms.
- Its "next" prelude makes tools engine-level async host calls, so a
  waiting session persists as a continuation dump with no replay
  ([djs-next]). That is the same design micro-monty has with FULL
  snapshots.
- djs's census ([djs-usage]) is the best usage data in this survey, and
  its conclusion carries over [E]: features that scripts never use may
  still be load-bearing for the harness's own prelude. Proxy, Reflect and
  getters are used by 0 scripts and by 92% of executions, through
  `tools`.

### Anthropic programmatic tool calling

All [S], from [ptc] and [ce].
- **Calling convention.** "Tools that allow a code execution caller are
  exposed to Claude's code as async Python functions, so Claude can run
  them in parallel with `asyncio.gather`. Each function takes a single
  dict of arguments and returns a string: the text of the `tool_result`
  you send back. Claude's code awaits these functions with top-level
  `await` and parses results that it needs as structured data, for example
  `rows = json.loads(await query_database({"sql": "<sql>"}))`."
- **Patterns shown:**
  - a loop over regions that sums `row["revenue"]` and picks
    `max(results.items(), key=lambda item: item[1])`, printed with
    `f"${...:,}"`;
  - early termination with `break`;
  - conditional tool selection on `json.loads(...)["size"]`;
  - log filtering with `splitlines()` and a comprehension, printing the
    last 10 matches.
- **Tool design advice.** "Provide detailed output descriptions", "Return
  structured data", and "If you specify that the tool returns JSON, Claude
  attempts to deserialize and process the result in code".
- **Timeouts.**
  - A pending call fails after about 4 minutes with `TimeoutError: Calling
    tool ['query_database'] timed out (no response after 270s).` in stderr,
    and "Claude ... typically retries the call".
  - Each REPL cell has a 90 s wall-clock limit. `code_execution_20260521`
    tells the model about it, and the cell returns a non-zero
    `return_code` with `detection_timeout`.
  - Idle containers are reclaimed after about 5 minutes.
- **Tool errors** are just result strings, for example `"Error: Query
  timeout - table lock exceeded 30 seconds"`: "Claude's code receives this
  error and can handle it appropriately". Results to programmatic calls
  must be text; images are rejected.
- **Container** ([ce]): Python 3.11, 5 GiB RAM, 1 CPU, no internet, and
  REPL state that persists with PTC. Pre-installed libraries are pandas,
  numpy, scipy, scikit-learn, statsmodels, matplotlib, seaborn, pyarrow,
  openpyxl, xlsxwriter, xlrd, pillow, python-pptx, python-docx, pypdf,
  pdfplumber, pypdfium2, pdf2image, pdfkit, tabula-py, reportlab,
  img2pdf, sympy, mpmath, tqdm, python-dateutil, pytz and joblib, plus
  CLI tools.
- **Evidence.** Anthropic reports 38% fewer billed input tokens on a
  75-tool benchmark, but about 8% *more* cost on τ²-bench's
  one-call-per-turn workflows.

### oh-my-pi (OMP): unsandboxed CPython and Bun

OMP is Stencil Labs' fork of pi, a coding-agent CLI on Bun ([omp-readme]).
The same organization publishes dquickts. Its programmatic tool calling is
the `eval` tool: one Python or JavaScript cell per call, and state persists
between calls ([omp-eval-doc]). All of this subsection is [S] at `88a6958`
unless labelled.

**The owner's belief is right, with one qualification.**
- JavaScript runs on full Bun. The cell is a retained worker in a
  subprocess that re-enters the `omp` binary, with an in-process Bun
  `Worker` as fallback ([omp-js-worker]). It runs user code by indirect
  `eval` rather than `node:vm`, because "Bun crashes the parent with SIGTRAP
  when Worker.terminate() fires mid-`vm.runInContext`" ([omp-indirect]).
  Cells see `Bun`, `Buffer`, `fetch`, `process`, `require` and `fs`, and can
  install packages with `%bun add`.
- Python runs on full CPython, but it is the user's interpreter, not one
  OMP ships. OMP picks the first interpreter that runs from: an active venv
  or conda env, `./.venv`, the managed `~/.omp/python-env`, then `python` or
  `python3` on PATH, any Python 3.8 or later ([omp-py-doc]). `%pip install`
  works in a cell.
- Neither runtime uses QuickJS, dquickts or rquickjs. `Cargo.lock` and
  `bun.lock` name none of them [M, `grep`]. One `pi-shell` comment calls its
  bash host "rquickjs-backed", but no such crate is linked [M].

**How the Python path runs.** It is a plain subprocess,
`python -u runner.py`, speaking NDJSON over stdin and stdout. It is neither
IPython nor Jupyter. The runner is a 3,237-line script bundled into the
binary ([omp-py-doc]). From January to May 2026 it was an IPython kernel
behind a shared Jupyter gateway ([omp-ipython]). Commit `8d144e17` replaced
that with the runner on 2026-05-12 ([omp-runner]). Details:
- **Kernel lifetime.** By default one kernel is kept per session, working
  directory and interpreter. `python.kernelMode = "per-call"` gives a fresh
  process for every call instead.
- **The cell loop.** Cells run on one persistent asyncio loop, so top-level
  `await`, `async for` and `async with` work, and `asyncio.run` fails.
- **Magics.** `%pip`, `%load`, `!cmd` and `%%bash` are rewritten into
  Python before the cell runs.

**Tool binding and naming.** One global proxy, `tool`, resolves any
attribute to a session tool: `await tool.read({"path": ...})`
([omp-prelude-py], L503–L530, L642–L700).
- **Names.** These are OMP's own agent tools (`read`, `grep`, `bash`, MCP
  tools and so on), plus unambiguous wire aliases. The eval prompt lists the
  names only. The model knows the schemas from the ordinary tool list,
  where the same tools are also directly callable ([omp-prompt]).
- **Python transport.** Each call does `asyncio.to_thread(_bridge_call, ...)`,
  a blocking `urllib` POST to a loopback Bun HTTP server that checks a
  bearer token ([omp-py-bridge]).
- **JavaScript transport.** It uses the worker's IPC.
- **Kernel-defined tools.** Cells can also define tools with `@tool`, with
  schemas inferred from type hints, for subagents to call.

**Arguments and results.**
- **Arguments.** A call takes one dict, keyword arguments, or both, merged:
  `tool.read({"path": p}, offset=5)`. Anything else raises
  `TypeError: tool.x(...) expects a dict of arguments`. The host validates
  the arguments against the tool's schema before running it
  ([omp-js-bridge], `callSessionTool`).
- **Results.** A result is the tool's text as a `str` when the tool returns
  no `details`, no images and no error. Otherwise it is a dict:
  `{text, details, images?, hasError?}`. MCP structured output arrives
  untruncated as `details.structuredContent`, even when the text rendering
  was cut ([omp-js-bridge], L171–L198; [omp-eval-doc], "MCP structured
  results").

**Errors and partial progress.**
- **Errors split two ways.** A tool that runs and reports an error
  *returns* a value with `hasError: True`, and nothing is raised. Everything
  the bridge refuses raises `RuntimeError(<message>)` in Python or rejects
  in JS: unknown tools, schema validation, aborts, tools that cannot be
  bridged ([omp-prelude-py], L460–L474).
- **Unknown tools.** A misspelled tool raises at the call,
  `RuntimeError: Unknown tool from js runtime: get_wether`, with that text
  even in Python. There is no suggestion ([omp-js-bridge], L94).
- **Cell failures.** A failing cell returns its output plus a traceback,
  with `Command exited with code N`. There is no list of the calls already
  made. The prompt instead says "On error, retry only the failed step;
  earlier steps may have taken effect" ([omp-prompt]).
- **Unawaited calls.** In JS, an unawaited rejected call fails the cell as
  `Unhandled rejection (missing await?)`.
- **Progress.** The model never sees progress mid-cell. The user sees
  per-call status events and MCP progress streamed live.

**Concurrency.**
- **No cap on calls.** OMP sets no bound on outstanding tool calls [M,
  `grep`]. Python's calls go through CPython's default executor, so beyond
  min(32, CPUs + 4) threads they queue rather than fail [S, CPython
  `concurrent.futures` docs; E for the effect].
- **Separate limits for model work.** Model calls (`completion`, `judge`)
  share a 32-request semaphore. Subagents count against `async.maxJobs`
  (100). `workpool()` queues items beyond `task.maxConcurrency` (32)
  ([omp-eval-doc], "Limits and errors").
- **One foreground cell.** The `eval` tool itself is
  `concurrency = "exclusive"` per agent session.

**Output and time.**
- **Output.** The model sees stdout and stderr only. Inline output is
  capped at 50 KiB, keeping head and tail with the middle elided, and at
  3,000 lines. The full text spills to an artifact of up to 16 MiB, cited
  as `artifact://<id>`, which `read()` can open ([omp-output]).
- **Time.** A cell has a 30 s idle timeout, clamped to 1–3,600 s, and `0`
  disables it. Ordinary tool calls count against it; waits on subagents and
  package installs pause it. On expiry the host sends SIGINT, which raises
  `KeyboardInterrupt` in the cell, and kills the kernel 5 s later if it is
  still running ([omp-py-doc]).
- **Auto-backgrounding.** This is off by default. When enabled, it turns a
  long cell into a background job.

**Durability.** There is none for cells. A kernel lives until reset, owner
disposal or process exit. A kernel that dies is replaced, and the next cell
opens with `Kernel restarted: the previous Python kernel is dead; state is
unavailable. Re-run needed setup ...` ([omp-restart]). Compaction keeps the
environment and the loaded paths, not variable values, and "resuming in a
fresh process does not restore historical kernel state" ([omp-eval-doc]).

**Sandboxing.** There is none. Cells have the user's filesystem, network,
subprocesses and package managers.
- **Environment.** The interpreter's environment is filtered: an allowlist,
  with common API keys stripped ([omp-py-doc]).
- **Approval.** `eval` sits in the approval tier `exec`. The docs warn that
  a `bash` deny pattern does not stop the same command run through `eval`
  ([omp-approval]).
- **Separate.** `pi-iso` provides copy-on-write worktrees for subagents,
  not containment.

**One unusual feature: speculation.** While the model is still streaming a
cell's source, a shadow planner starts the cell's `read` calls early. The
running cell then claims the results by call site and occurrence, and a
mismatch falls back to a normal call (`src/eval/speculation/`).

**Compared with dquickts and micro-monty** [E, except where cited]:

| | OMP `eval` | dquickts (same organization) | micro-monty (proposed) |
|---|---|---|---|
| Engine | user's CPython; Bun | QuickJS fork in C | own VM in safe Rust |
| Isolation | none; approval tier only | in-process C engine; host capabilities only | in-process, no ambient OS; host capabilities only |
| Tool call | `await tool.name({...})` through a proxy, HTTP or IPC | `JS_SuspendCall` host calls | host functions; `Resume::Value`/`Future`; `gather` gives one batch |
| Arguments | dict, keywords, or both | as the host defines | untyped or `HostSignature` (R-H8 adds dict-or-keywords) |
| Results | `str`, or `{text, details, hasError}` | host values | any `HostValue` |
| Tool errors | reported errors are values (`hasError`); bridge errors raise `RuntimeError` | host-defined | `HostError` kind and message (R-H4 adds a payload) |
| Concurrency | unbounded; Python queues past the thread pool | host-defined | unbounded in the engine (R-H6 proposes queueing) |
| Output | 50 KiB head and tail, artifact for the rest | host-defined | host callback; `set_text_limit` |
| Durability | none; a dead kernel loses state | full and delta snapshots, plain sync frames only (spec §9.1) | FULL snapshots at any safepoint, any frame |
| Python stdlib | all of it, plus `%pip` | n/a | the subset in §2 |

What this means for micro-monty's design:
- **OMP shows the other end of the design space.** It is the most capable
  surface here and gives up isolation and durability for it, so it is not a
  substitute for micro-monty's goals. dquickts gives OMP's own organization
  durable JavaScript, yet OMP does not use it for `eval` [M]. The likely
  reason is that a coding agent on the user's machine needs the user's
  packages and files [E].
- **OMP supports four of the R-H proposals:**
  - R-H8: one dict *or* keyword arguments, merged.
  - R-H4: tool-reported errors returned as values, separate from bridge
    failures. These are two error channels, worth naming in micro-monty's
    docs.
  - R-H6: queueing rather than refusing extra calls; Python's thread pool
    queues implicitly.
  - R-H7: head and tail output with a retrievable full copy.
- **Where micro-monty already does more:**
  - OMP reports no call log after a failure, only "earlier steps may have
    taken effect". R-H3 would give micro-monty pi's call log.
  - OMP gives no "Did you mean" for misspelled tools; micro-monty's
    `NameError` already does.
- **Prompt habits are compatible.** OMP teaches models `await tool.x({...})`
  with top-level `await`, and tells them `asyncio.run` fails. Code written
  for OMP's Python path runs on micro-monty when it stays inside the
  subset; it fails when it uses files, `%pip` or third-party packages.

### Others (brief)

- **Cloudflare codemode** [S] ([cf-shared], [cf-exec], [cf-trunc],
  [cf-readme]):
  - the model writes `async () => { ... }` JavaScript, and tools are
    `codemode.name({...})` or `connector.method({...})`;
  - discovery is `codemode.search(query)` and `codemode.describe(path)`;
  - `fetch()` and `connect()` are blocked by default
    (`globalOutbound: null`), and the timeout defaults to 60 s;
  - tool failures come back as `{ error: message }`, and a missing tool as
    `{ error: 'Tool "x" not found' }`;
  - results are truncated structurally: about 6,000 tokens at 4 characters
    per token, valid JSON of the same shape with the largest values cut,
    marked `--- TRUNCATED ---`.
- **smolagents `LocalPythonExecutor`** [S] ([smol-utils], [smol-exec],
  [smol-prompt]):
  - an AST interpreter over real CPython;
  - the default authorized imports are `collections`, `datetime`,
    `itertools`, `math`, `queue`, `random`, `re`, `stat`, `statistics`,
    `time` and `unicodedata`;
  - `os`, `pathlib`, `sys`, `io`, `subprocess`, `socket`, `shutil` and
    `builtins` are "dangerous", and `eval`/`exec`/`compile`/`globals`/
    `locals`/`__import__` are forbidden;
  - an import fails with `Import of x is not allowed. Authorized imports
    are: [...]`;
  - limits: 10M operations, 1M `while` iterations, 50,000 characters of
    printed output;
  - the prompt says: "DO NOT pass the arguments as a dict ... use the
    arguments directly as in `wikipedia_search(query=...)`" and "You can
    use imports in your code, but only from the following list of
    modules: {{authorized_imports}}".

## 2. Merged table: modules

Key:
- **mm**: micro-monty `main` today [M]. D = default build, O = opt-in
  feature, – = absent.
- **Monty**: 1.4.0 / HEAD [S].
- **PAI**: listed in Pydantic AI's prompt.
- **smol**: smolagents' default authorized imports.
- **Anth**: Anthropic's container (full CPython 3.11, so every stdlib module).
- **Evidence of use**: what relies on the module. JavaScript equivalents
  from the pi census are [E].
- **Packed bytecode**: [M], `cargo build -p mm-stdlib`, `OUT_DIR/*.bin`.

| Module | mm | Monty | PAI | smol | Anth | Evidence of use | Packed bytecode |
|---|---|---|---|---|---|---|---|
| `json` | D | ✓/✓ | ✓ | – | ✓ | Anthropic's `json.loads(await tool(...))` idiom; JSON in 14% of pi scripts (358/2,687) | native |
| `re` | D | ✓/✓ | ✓ | ✓ | ✓ | regex in 166 pi scripts, almost only `g`/`i`; plak's `ctx.grep` patterns | 21,721 + native |
| `math` | D | ✓/✓ | ✓ | ✓ | ✓ | `Math` in 30 pi scripts | native |
| `asyncio` | D | ✓/✓ (run, gather, sleep) | ✓ | – | ✓ | every Python harness prompt; `Promise.all`/`allSettled` in 28% of pi scripts | native |
| `collections` | D | ✓/✓ (4 names) | ✓ | ✓ | ✓ | plak SDK bootstrap (`namedtuple`); `Set`/`Map` usage | 14,652 (+ `.abc` 8,027) |
| `itertools`, `functools` | D | ✓/✓ | ✓ | ✓/– | ✓ | everyday idioms | 9,321 / 7,500 |
| `datetime`, `time` | D | ✓/✓ | ✓ | ✓ | ✓ | `Date` in 43 pi scripts (`toISOString`, `Date.now`) | 25,683 + 5,417 / 10,318 |
| `random` | D | ✓/✓ | ✓ | ✓ | ✓ | `Math.random` (rare) | 9,093 |
| `typing` | D | ✓/✓ | ✓ | – | ✓ | plak SDK stubs; annotations in model code | 8,595 |
| `dataclasses`, `copy` | D | ✓/✓ | ✓ | – | ✓ | record types | 11,021 / 3,368 |
| `base64`, `binascii`, `hashlib` | D | ✓/✓ | ✓ | – | ✓ | images as base64 in pi results; content hashes | 5,236 / 4,472 / 7,635 |
| `bisect` | D | ✓/✓ | ✓ | – | ✓ | | 1,598 |
| `heapq` | D | – | – | – | ✓ | top-N | 2,191 |
| `csv`, `io` | D | `csv` only | `csv` | – | ✓ | `csv.DictReader(io.StringIO(text))` [E] | 8,691 / 2,477 |
| `os`, `os.path`, `posixpath`, `pathlib` | D (OS hooks) | ✓/✓ | ✓ | – (dangerous) | ✓ | path manipulation; plak's prompt forbids I/O | 5,740 + 5,388 + 649 / 5,666 |
| `fnmatch` | D | –/✓ | – | – | ✓ | glob filters | 2,069 |
| `sys` | built in | ✓/✓ | ✓ | – (dangerous) | ✓ | `sys.stdout` | |
| `string`, `textwrap` | D | – | – | – | ✓ | | 2,142 / 3,422 |
| `enum`, `abc`, `operator`, `types` | D | – | – | – | ✓ | class-heavy code; pi scripts use no classes | 10,901 / 2,361 / 4,065 / 2,275 |
| `contextlib` | D | – | – | – | ✓ | | 5,378 |
| `traceback`, `warnings`, `logging` | D | – | – | – | ✓ | | 3,570 / 4,664 / 10,521 |
| `urllib.parse` | D | – | – | – | ✓ | URL building (`encodeURIComponent` in 0 pi scripts) | 11,477 |
| `pprint`, `shlex`, `difflib` | D | – | – | – | ✓ | `shlex.quote` for plak's `proc.shell` [E] | 7,104 / 3,715 / 7,282 |
| `struct`, `html`, `codecs`, `platform`, `gc` | D | – | – | – | ✓ | no binary data in pi's sample [S] | 2,036 + native / 1,794 / 3,470 / 1,188 / 858 |
| `cmath` | D | – | – | – | ✓ | | native |
| `statistics` (+ `fractions`) | O | – | – | **✓** | ✓ | means and medians over tool results [E] | 13,840 + 8,884 |
| `tomllib` | O | ✓/✓ | ✓ | – | ✓ | `pyproject.toml`/`Cargo.toml` in coding tasks [E] | 10,445 (4 units) |
| `uuid` | O | – | – | – | ✓ | Monty lists it as "not implemented yet" | 5,179 |
| `decimal` (+ `numbers`, `contextvars`) | O | ✓/✓ | ✓ | – | ✓ | money arithmetic [E] | 39,414 + 1,906 + 1,235 |
| `unicodedata` | O | ✓/✓ | ✓ | ✓ | ✓ | `normalize` in 0 pi scripts; NFKD slugs [E] | 193 + native tables (not measured) |
| `html.entities` | O | – | – | – | ✓ | | 32,470 |
| `annotationlib` | – | –/✓ | – | – | – (3.14) | | |
| `queue`, `stat` | – | – | – | ✓ | ✓ | | |
| `zoneinfo` | – | – | – | – | ✓ | `ZoneInfo("Europe/Paris")` [E] | |
| `inspect` | – | – | – | – | ✓ | | |
| `glob`, `shutil`, `tempfile`, `zipfile`, `gzip`, `zlib`, `pickle`, `sqlite3` | – | – | – | – | ✓ | | |
| `threading`, `subprocess`, `multiprocessing`, `socket`, `ctypes` | – (by design) | – (by design) | – | – (dangerous) | ✓ | | |
| `typing_extensions` | – | stubs only | – | – | ✓ | Monty's checker resolves it [S] | |
| third-party (numpy, pandas, requests, ...) | – | – | – ("No third-party") | – | numpy, pandas, ... | trained-in habit from Anthropic's container [E] | |

## 3. Merged table: builtins and language features

The pi columns count JavaScript scripts out of 2,535 [S]. The Python
equivalents are [E].

| Feature | mm | Monty | Relied on by |
|---|---|---|---|
| top-level `await` in a cell | ✓ [M] | ✓ | plak py-ptc, PAI, Anthropic; `await` in 98% of pi scripts |
| `asyncio.gather(*aws)` | ✓ | ✓ | every Python harness |
| `gather(..., return_exceptions=True)` | ✓ [M] | ✗ (`NotImplementedError`) | `Promise.allSettled` in 451 pi scripts (18%), pi's prompt guideline |
| `create_task`, `wait_for`, `Semaphore`, `TaskGroup`, `timeout`, `as_completed` | ✗ (`AttributeError`) [M] | ✗ | none advertise them; PAI's prompt forbids them [E: models reach for them] |
| last-expression value as result | ✓ | ✓ | plak, PAI, Monty examples; pi uses `return` |
| `print` as output | ✓ | ✓ | everyone; Anthropic returns only stdout |
| f-strings, format specs (`:,`, `:.1f`) | ✓ | ✓ | Anthropic examples; template literals in 998 pi scripts |
| comprehensions, lambda, `sorted(key=)`, `max(key=)` | ✓ | ✓ | Anthropic examples; `.map`/`.filter`/`.sort` |
| `try`/`except` around tool calls | ✓ | ✓ | 71 pi scripts; "Nested tool failures raise catchable Python exceptions" (plak) |
| `break`/early exit | ✓ | ✓ | Anthropic's early-termination pattern; pi's `exit()` |
| classes | ✓ (with inheritance) | simple only | 0 pi scripts; plak's SDK bootstrap uses classes as namespaces |
| user exception classes | ✓ | ✗ | |
| generators, `yield` | ✓ | ✗ | 0 pi scripts; pi's prelude needs generator syntax |
| `match` | ✓ | ✓ | `switch` in 1 pi script |
| `del` | ✓ | ✗ | `delete` in 1 pi script |
| `async for`/`async with` | ✓ | ✗ | for-await in 0 pi scripts |
| `eval`/`exec` | ✗ (`NameError ... does not support eval()`) | source text only | 0 pi scripts; smolagents forbids them |
| `open()`, `os.environ`, clock, randomness | host hooks | host `OSAccess` | PAI configures them; plak refuses all OS calls |
| regex: lookahead, named groups, `\p{}` | ✓ (no `\p`, as CPython) | ✓ (fancy-regex) | 0–1 pi scripts |
| `str.translate`, `bytearray`, `memoryview` | ✗ (docs/history/handoff-0.0.1.md) | `bytearray` ✗ | |

## 4. Tool-calling conventions compared

| | Call | Arguments | Result | Concurrency | Unknown tool |
|---|---|---|---|---|---|
| Anthropic PTC [S] | `await tool({...})` | one `dict` | `str` (the `tool_result` text) | `asyncio.gather`; no stated cap | not covered by the docs |
| plak py-ptc [S] | `await tool({...})`, or `await tool("text")` for freeform tools | one `dict` or one `str` | `str` | `gather`, at most 16 outstanding, then `RuntimeError` | `NameError` |
| plak SDK [S] | `await ctx.read(target, lines=None)` | positional or keyword, typed | namedtuple records | `gather` | `AttributeError` on the namespace class |
| Pydantic AI on Monty [S] | `await tool(arg=value)`; `sequential=True` tools are plain `def` | keyword-only | structured (TypedDict from the schema) | `gather` (positional only); 100 calls per snippet | `NameError` |
| smolagents [S] | `tool(arg=value)`, synchronous | keyword; dicts discouraged | structured when an output schema exists | none | `Forbidden function evaluation: 'x' is not among the explicitly allowed tools...` |
| pi on QuickJS [S] | `await tools.name({...})` | one object | `string`, or an object with an output schema | `Promise.all`/`allSettled`; `models.*` calls limited to 4 in flight, the rest queued | `TypeError: tools.x does not exist. Did you mean ...` |
| Cloudflare [S] | `await codemode.name({...})` | one object | the value, or `{ error }` | `Promise.all` | `{ error: 'Tool "x" not found' }` |
| Monty native [S] | `name(...)`, sync or async as the host chooses | as declared | host objects, dataclasses | `gather` | `NameError`, or a host name lookup |
| OMP `eval` [S] | `await tool.name({...})`, Python or JS | one `dict`, keywords, or both merged | `str`, or `{text, details, images?, hasError?}` | `gather`; no cap (Python queues past the default thread pool) | `RuntimeError: Unknown tool from js runtime: x` at the call |
| micro-monty [S, docs/sdk.md] | `name(...)` or `await name(...)`, decided per call by `Resume::Value`/`Future` | untyped `(*args, **kwargs)` or a typed `HostSignature` | any `HostValue`, records, host classes | `gather` yields one `Waiting` batch | `NameError` with "Did you mean" |

## 5. Error and missing-feature messaging

| Runtime | Missing module | Unsupported syntax or API | Tool failure | Limit | After failure |
|---|---|---|---|---|---|
| micro-monty [M] | `ModuleNotFoundError: No module named 'numpy'` | `SyntaxError: micro-monty does not support __slots__`; `NameError: name 'eval' is not defined. micro-monty does not support eval()`; `AttributeError: module 'asyncio' has no attribute 'create_task'` | host chooses one of 18 `HostErrorKind`s (kind and message only) | `MemoryError`; fuel and deadline park | traceback in CPython 3.14 format |
| Monty [S] | `ModuleNotFoundError: No module named 'x'` (asked of the host first) | `NotImplementedError: The monty syntax parser does not yet support ...`; `gather() does not yet support keyword arguments` | the host's exception | `suspension limit N exceeded` | |
| plak py-ptc [S] | Monty's | Monty's | `RuntimeError(<host text, ≤16 KiB>)` | `RuntimeError: local composition exceeds its profile-bound limit of 16 outstanding nested calls`; the worker is killed after 90 s | "The interpreter was reset, so names bound by earlier cells are gone" |
| plak SDK [S] | Monty's, or the type checker's | "type errors; nothing ran" | `ValueError`/`OverflowError`/`RuntimeError` by kind | `[N output bytes omitted; full output: Ref('id')]` | "SDK cell failed; see bounded output and reference" |
| Pydantic AI [S] | Monty's (type check first) | prefixed `Syntax error in code:` / `Type error in code:` / `Runtime error:` | a `ModelRetry` inside the tool surfaces as a runtime error | "ran longer than `max_duration_secs` ... session was reset. Re-run any imports" | list of started calls and their status |
| pi [S] | (JS) an `import` statement is a SyntaxError in the async body | QuickJS errors with a `codemode.js:line:col` frame | `Error` carrying the tool's text and own properties | "script output exceeded the limit of ..."; `InternalError: out of memory`; the stalled-promise message | "Tool calls made before the failure (they are not undone): ..." |
| Anthropic [S] | CPython's (third-party packages present) | CPython's | the result string, by convention `Error: ...` | `TimeoutError: Calling tool [...] timed out (no response after 270s).`; `detection_timeout` | stderr; Claude retries |
| smolagents [S] | `Import of x is not allowed. Authorized imports are: [...]` | `Forbidden access to module/function/dunder ...` | exception text in "Execution logs" | `Maximum number of 1000000 iterations in While loop exceeded` | |

What works, across these [E]:
- Keep CPython's first sentence, so that the model recognises the error,
  and add a short sentence that names the alternative. smolagents'
  allow-list in the error and pi's "Did you mean" both do this.
- Say what already ran, as pi and Pydantic AI do.
- Distinguish "not in this build" from "never available".

## 6. Result size and output handling

| Runtime | Bound | Over the bound |
|---|---|---|
| plak py-ptc [S] | 1 MiB `max_output_bytes`; final and nested results 1 MiB, 16 MiB per cell | truncation notice |
| plak SDK [S] | 16 KiB observation | cut, with a `Ref` to the full text that `ctx.read` can open |
| pi [S] | `max_output_tokens` 10,000 (adjustable per script); hard 16 Mi characters / 100k items | head and tail kept, full text in a temp file; past the hard limit the script fails |
| Pydantic AI [S] | 10 MiB of print output | model retry |
| Cloudflare [S] | about 6,000 tokens | structural JSON truncation with `--- TRUNCATED ---` markers |
| smolagents [S] | 50,000 characters of print output | truncated |
| Anthropic [S] | (tool results never enter the context; only stdout does) | `output_file_too_large` for bash |
| micro-monty [S, docs/sdk.md] | `set_text_limit` for completions; output is the host's callback | `repr`/`str` cut at a character boundary with `*_truncated` set |

## 7. Recommendations for micro-monty

### R-T: tiers

- **Always**, the language-support pieces: builtins, `sys`, `asyncio`,
  `typing`, `types`, `abc` and `collections.abc`. Every Python harness
  prompt relies on `asyncio` and `typing`, plak's SDK bootstrap imports
  `asyncio` [S], and the rest back `isinstance` and annotations [E].
- **Default-on**, kept from today, with evidence from at least two
  surveyed runtimes or from usage:
  - `json`, `re`, `math`, `collections`, `itertools`, `functools`;
  - `datetime`, `time`, `random`, `dataclasses`, `copy`;
  - `base64`, `binascii`, `hashlib`, `bisect`, `csv`;
  - `os`, `os.path`, `posixpath`, `pathlib`, `fnmatch`.

  Also kept, as idioms of longer programs with small bytecode, though only
  CPython has them [E]: `heapq`, `io`, `string`, `textwrap`, `enum`,
  `operator`, `contextlib`, `urllib.parse`, `shlex`.
- **Promote to default-on.** Together these are 38,348 B of packed
  bytecode [M]. The shipped `hello.wasm` delta was not measured; it is
  about the same, since bytecode is embedded as-is [E].
  - `statistics`: in smolagents' default list and plain CPython [S];
    `mean`/`median` over tool results is a common reduction [E].
    22,724 B together with `fractions` [M].
  - `tomllib`: in Monty and in Pydantic AI's advertised list [S];
    coding agents read `pyproject.toml` and `Cargo.toml` [E]. 10,445 B [M].
  - `uuid`: Monty lists it as missing ("not implemented yet") [S], and
    `uuid4()` for ids is a common idiom [E]. 5,179 B [M].
- **Keep opt-in, decide after the census:**
  - `decimal`: in Monty and Pydantic AI [S], but 42.5 KB with its
    dependencies [M], the largest module.
  - `unicodedata`: in Monty, Pydantic AI and smolagents [S]. Its native
    tables were not measured; measure them with `cargo xtask size` before
    deciding. `normalize` appeared in 0 of 2,535 pi scripts [S].
  - `html.entities`.
- **Census-gated** (default now, first to move to opt-in if the census
  shows no use): `struct`, `html`, `platform`, `difflib`, `pprint`,
  `logging`, `codecs`, `traceback`, `warnings` and `gc`. None of the
  surveyed sandboxes lists them [S]. Only CPython has them, and the pi
  sample has "no binary data" [S].
- **Small additions worth measuring** [E]:
  - `typing_extensions` as an alias of `typing`, covering the names
    `typing` has. Monty's checker resolves it [S], and models import it
    for `TypedDict` and `Literal`.
  - `asyncio.create_task`, `asyncio.Semaphore` and `asyncio.wait_for`/
    `timeout`, as the next asyncio work. These are the CPython idioms for
    bounded concurrency. No surveyed sandbox has them, and Pydantic AI
    spends prompt words to stop models from using them [S].

### R-M: redirect messages

Keep CPython's message and append one sentence, as micro-monty already
does for `eval()`. The engine should ship defaults and let hosts replace
the second sentence per group (R-H1), so that plak can say "use the nested
tools". Suggested defaults [E]:

| Imports | Message after `No module named 'x'` |
|---|---|
| `numpy`, `pandas`, `scipy`, `sklearn`, `polars`, `pyarrow`, `matplotlib`, `seaborn`, `sympy` | `. micro-monty runs a Python subset without third-party packages; use lists, dicts and the math, statistics, csv and json modules, or a tool.` |
| `requests`, `httpx`, `aiohttp`, `urllib3`, `urllib.request`, `http.client`, `socket` | `. There is no network in this sandbox; call a tool to fetch data.` |
| `subprocess`, `multiprocessing`, `threading`, `concurrent.futures`, `signal`, `pty` | `. There are no processes or threads in this sandbox; run commands with a tool, and run tool calls concurrently with asyncio.gather.` |
| `shutil`, `tempfile`, `glob`, `zipfile`, `tarfile`, `gzip`, `sqlite3`, `pickle` | `. There is no filesystem in this sandbox; read and write files with a tool.` |
| `dateutil`, `pytz`, `zoneinfo` | `. Use datetime with timezone.utc or fixed timedelta offsets.` (unless the host supplies zone data) |
| `pydantic`, `attrs` | `. Use dataclasses.` |
| `yaml` | `. Use json, or tomllib for TOML.` |
| `bs4`, `lxml` | `. Use re and str methods on the text, or a tool.` |
| a module behind a feature that is not enabled (`decimal`, `unicodedata`, ...) | `. 'decimal' is not enabled in this build of micro-monty.` |
| a missing `asyncio` name (`create_task`, `wait_for`, `Semaphore`, ...) | after `module 'asyncio' has no attribute 'create_task'`: `. micro-monty's asyncio has run, gather and sleep; pass coroutines to asyncio.gather.` |
| refused OS hooks (`open`, `Path.*`, `os.environ`) | model-facing by default, for example `PermissionError: open() is not available in this sandbox; read files with a tool`, with today's host-facing text kept in the docs |

Also match CPython 3.14's "Did you mean" for module attributes. Today
`json.lods` gives no suggestion in micro-monty and `Did you mean: 'loads'?`
in CPython [M]. Apply it to host modules too; pi's "does not exist. Did
you mean" on `tools` is the same idea [S].

### R-H: host-API improvements

1. **Configurable hints** (`Session::set_import_hint(name or group,
   text)` or a table in `EngineConfig`). Evidence: spec §4 promises "a
   host-configurable hint" and it is not in the facade [M]; plak's
   refusal wording differs from the engine's [S]; every harness that
   restricts imports explains it in its own words (smolagents' list,
   Pydantic AI's list) [S].
2. **An environment summary derived from the build**, such as
   `Engine::environment()`: the modules enabled in this build, the asyncio
   surface, and the limits, as text for prompts. Hosts would stop
   hand-writing lists. Evidence: Monty's own example prompt says `json`,
   `re` and `math` are unavailable [S]; Pydantic AI's list lags Monty HEAD
   by three modules [S]; plak's prompt hard-codes the I/O restrictions
   [S]. `stubs()` already does this for host functions.
3. **A per-cell host-call log.** `PyException`, or `Session::cell_calls()`,
   would list each host call of the cell with its name, call id and status
   (returned, raised, pending or abandoned), so a host can report what
   already happened. Evidence: pi's "Tool calls made before the failure
   (they are not undone)" [S], Pydantic AI's started-call summary in every
   retry [S], and plak's `Delivery{NotObserved}` receipts [S]. Call ids
   already exist (docs/sdk.md).
4. **Structured host errors.** `HostError` would carry an optional
   payload (`HostValue`) that becomes attributes or `args` of the raised
   exception, plus a host-named exception class (for example
   `ToolError(RuntimeError)` with `.tool` and `.data`). Evidence: pi
   copies the error's own properties onto the `Error` [S]; plak's SDK
   wraps `kind` in records and unwraps them in Python [S]; Cloudflare and
   Anthropic fall back to error strings [S]. Today `HostError` is kind and
   message only [M, `mm-engine/src/host.rs`].
5. **A never-awaited warning.** At the end of a cell, report the host-call
   coroutines that were created and never awaited, in CPython's words
   (`RuntimeWarning: coroutine 'read_file' was never awaited`). Evidence:
   plak's and Pydantic AI's prompts both spend a sentence on this mistake
   [S]; CPython prints the warning, and micro-monty prints nothing [M].
   This must not depend on GC timing (spec §1): scan at cell end instead
   [E].
6. **An outstanding-call bound that queues instead of raising.** A facade
   option `max_outstanding` would keep further calls inside the engine,
   unissued, until a slot frees. Evidence: plak raises `RuntimeError` at
   the 17th call [S]; pi queues model calls beyond four, so that "Promise.all()
   over many items is fine" [S]; Pydantic AI refuses calls past 100 at the
   call site [S]. Queueing lets `gather` over 200 items work without a
   `Semaphore`.
7. **Bounded rendering helpers.** Offer a head-and-tail mode for
   `set_text_limit`, plus container-aware truncation of completion `repr`s
   that keeps the shape and marks the omissions (Cloudflare's design [S]).
   Keep the full `HostValue` so that the host can spill it to a file or a
   `Ref` (pi and plak [S]).
8. **Both calling conventions in one signature.** Add a `HostSignature`
   flag that also accepts a single positional `dict` as the keyword
   arguments, and document that results should be structured `HostValue`s,
   not JSON strings. Evidence: Anthropic and plak py-ptc pass one dict and
   return strings that the model `json.loads` [S]; Pydantic AI, smolagents
   and plak's SDK use keyword arguments with structured results [S];
   smolagents' prompt has a rule against passing a dict [S]. The tradeoff:
   a tool with one `dict` parameter makes the flag ambiguous, so the flag
   is per function [E].
9. **Introspection of host functions.** Expose `__doc__` (from
   `HostSignature.doc`), `__name__`, `__module__`, `dir(module)`, and a
   `help()` that prints the stub. Evidence: pi's
   `describeTool`/`searchTools`/`ALL_TOOLS` [S], Cloudflare's
   `codemode.search`/`describe` [S], and Pydantic AI's `search_tools`
   [S]. Search itself belongs to the host; the engine only needs to answer
   these lookups.
10. **Timeouts the model can read.** Document `Resume::Error(TimeoutError)`
    as the way to time out a host call, matching Anthropic's
    `TimeoutError: Calling tool [...] timed out` [S]. Also offer a
    deadline mode that raises `TimeoutError` in the cell instead of
    parking, for hosts that want Anthropic's 90 s cell behaviour [S]
    rather than a park [E].

## 8. For the synthetic task suite (parallel thread)

The suite should cover these, so that the census measures what models do
[E]:

- **Fan-out and reduce**: one call per item through `gather`, then
  filtering, sorting and top-N, with `return_exceptions=True` for partial
  failure (pi's `allSettled` share [S]).
- **Sequential chains**: paging with a cursor in a `while` loop, early
  `break`, and conditional tool choice (Anthropic's patterns [S]).
- **String results**: `json.loads` on string results, and code that
  forgets it and indexes a `str`.
- **Text work on tool output**: `splitlines`, `re.findall`/`sub` with
  `g`/`i`-style flags, slicing, counting with `Counter`, grouping with
  `defaultdict`, and dedupe with `set`.
- **Dates**: `fromisoformat`, `strftime`, `timedelta` windows,
  `datetime.now(timezone.utc)` (the clock hook), and timezone names.
- **Data formats**: CSV through `csv.DictReader(io.StringIO(...))`, TOML
  config, base64 images, and `hashlib` digests.
- **Numbers**: means and medians, percentages with f-string format specs,
  and money with `decimal` versus `float`.
- **plak's SDK shapes**: records read by attribute, `Omission`/`Ref`
  follow-ups, compare-and-swap edits with `expect`, `proc.shell` with
  `shlex.quote`, and `state.put`/`get` across cells.
- **Negative tasks** that measure redirect quality:
  - the natural answer imports `pandas`, `requests`, `subprocess` or
    `zoneinfo`;
  - a misspelled tool or module attribute;
  - a forgotten `await`;
  - a failing tool, followed by a retry;
  - more than 16 concurrent calls;
  - output larger than the observation bound.
- **Longer programs**: helper functions, dataclasses, a small class with
  inheritance and `match`. The pi census saw no classes in short scripts
  [S], but Python models may write them in long cells [E].
- **Import census**: record every `import` and every `from x import y`
  name (module and attribute) that the suite's cells reach. That answers
  the tier questions above directly.

## 9. Open questions for the owner

1. Promote `statistics`, `tomllib` and `uuid` now (38 KB of bytecode
   [M]), or wait for the census?
2. `decimal` (42.5 KB [M]) and `unicodedata` (tables not measured): both
   are in Monty and Pydantic AI. Opt-in until the census?
   *Census answer* [M]: Claude and gpt-6-luna barely use `decimal` (2 and 5 of
   594 solutions). gpt-6.1-sol imports it in 54, and it is that model's
   largest default-build failure (52). Promoting it raises sol's default
   pass rate from 83.7% to 90.2%, counted after `statistics` and `tomllib`.
   That only holds with `json.loads(parse_float=...)` hooks, which fail 14
   sol solutions today. No family used `unicodedata`.
3. Should the redirect texts be engine defaults (R-M) or host-only
   (R-H1 with an empty default)?
4. `typing_extensions` as an alias of `typing`: acceptable as a
   non-CPython module name?
5. Which comes first: asyncio's `create_task`/`Semaphore`/`wait_for`, or a
   queueing outstanding-call bound (R-H6)? The bound covers the
   bounded-concurrency case without new asyncio surface.
6. plak's SDK: a host-side type checker over `stubs()`, or runtime
   `TypeError`s only? This decides whether "A type error means nothing
   ran" stays in plak's prompt.
7. Which corpus is the "import census of real tool code"? plak's py-ptc
   transcripts are not in the plak repository, and pi's harvest is
   JavaScript.

[ptc]: https://platform.claude.com/docs/en/agents-and-tools/tool-use/programmatic-tool-calling
[ce]: https://platform.claude.com/docs/en/agents-and-tools/tool-use/code-execution-tool
[plak-ptc]: https://github.com/N0xMare/plak/blob/36661c21f48de51ec63697f0c1a8c0675dd62d28/crates/harness/prompts/src/ptc.rs#L8-L11
[plak-text]: https://github.com/N0xMare/plak/blob/36661c21f48de51ec63697f0c1a8c0675dd62d28/crates/harness/compose/src/text.rs#L13-L23
[plak-render-py]: https://github.com/N0xMare/plak/blob/36661c21f48de51ec63697f0c1a8c0675dd62d28/crates/harness/compose/src/cell/render_py.rs
[plak-sdk-api]: https://github.com/N0xMare/plak/blob/36661c21f48de51ec63697f0c1a8c0675dd62d28/crates/harness/compose/src/cell/sdk_api.rs
[plak-sdk-unwrap]: https://github.com/N0xMare/plak/blob/36661c21f48de51ec63697f0c1a8c0675dd62d28/crates/harness/compose/src/cell/sdk_api.rs#L601-L610
[plak-sdk-prompt]: https://github.com/N0xMare/plak/blob/36661c21f48de51ec63697f0c1a8c0675dd62d28/crates/harness/prompts/src/bases/python_sdk.rs#L3
[plak-sdk-limits]: https://github.com/N0xMare/plak/blob/36661c21f48de51ec63697f0c1a8c0675dd62d28/crates/harness/compose/src/cell/sdk.rs#L138-L153
[plak-sdk-render]: https://github.com/N0xMare/plak/blob/36661c21f48de51ec63697f0c1a8c0675dd62d28/crates/plakous/src/local_composition/sdk/render.rs
[plak-cell]: https://github.com/N0xMare/plak/blob/36661c21f48de51ec63697f0c1a8c0675dd62d28/crates/runtimes/py-worker/src/cell.rs#L590-L630
[plak-cell-os]: https://github.com/N0xMare/plak/blob/36661c21f48de51ec63697f0c1a8c0675dd62d28/crates/runtimes/py-worker/src/cell.rs#L791-L812
[plak-codec]: https://github.com/N0xMare/plak/blob/36661c21f48de51ec63697f0c1a8c0675dd62d28/crates/runtimes/py-worker/src/codec.rs#L228-L240
[plak-protocol]: https://github.com/N0xMare/plak/blob/36661c21f48de51ec63697f0c1a8c0675dd62d28/crates/runtimes/cell/src/protocol.rs#L37-L41
[plak-contract]: https://github.com/N0xMare/plak/blob/36661c21f48de51ec63697f0c1a8c0675dd62d28/crates/core/agent/src/local_composition.rs#L60-L76
[plak-spec-sel]: https://github.com/N0xMare/plak/blob/36661c21f48de51ec63697f0c1a8c0675dd62d28/docs/spec/01-plakous-library.md?plain=1#L7942-L7952
[plak-spec-limits]: https://github.com/N0xMare/plak/blob/36661c21f48de51ec63697f0c1a8c0675dd62d28/docs/spec/01-plakous-library.md?plain=1#L8155-L8260
[mm-worker]: ../../crates/mm-cell-worker/src/worker.rs
[monty-index-140]: https://github.com/pydantic/monty/blob/4b015dc4a1e08394b0a98e0255ed8c81de1fbbf6/docs/limitations/index.md
[monty-modules]: https://github.com/pydantic/monty/blob/2a3269043e544d0e7bd99e3e5cf0c306a8246076/docs/limitations/modules.md
[monty-asyncio]: https://github.com/pydantic/monty/blob/2a3269043e544d0e7bd99e3e5cf0c306a8246076/docs/limitations/asyncio.md
[monty-parse]: https://github.com/pydantic/monty/blob/2a3269043e544d0e7bd99e3e5cf0c306a8246076/crates/monty/src/parse.rs#L3100
[monty-typecheck]: https://github.com/pydantic/monty/blob/2a3269043e544d0e7bd99e3e5cf0c306a8246076/docs/type-checking.md
[monty-example]: https://github.com/pydantic/monty/blob/2a3269043e544d0e7bd99e3e5cf0c306a8246076/examples/web_scraper/main.py#L44-L65
[pai-desc]: https://github.com/pydantic/pydantic-ai/blob/1eaf70020560980e8916089d846c6a1bb6accccd/src/pydantic_ai_harness/pydantic_ai_harness/code_mode/_toolset.py#L469-L580
[pai-errors]: https://github.com/pydantic/pydantic-ai/blob/1eaf70020560980e8916089d846c6a1bb6accccd/src/pydantic_ai_harness/pydantic_ai_harness/code_mode/_toolset.py#L1271-L1365
[pai-docs]: https://pydantic.dev/docs/ai/harness/code-mode/
[pi-doc]: https://github.com/earendil-works/pi/blob/d57c2d21916eaad1158031526ac6ec860eae5f14/packages/coding-agent/docs/codemode.md
[pi-tool]: https://github.com/earendil-works/pi/blob/d57c2d21916eaad1158031526ac6ec860eae5f14/packages/coding-agent/src/extensions/codemode/tool.ts#L108-L135
[pi-execute]: https://github.com/earendil-works/pi/blob/d57c2d21916eaad1158031526ac6ec860eae5f14/packages/coding-agent/src/extensions/codemode/execute.ts#L374-L390
[pi-prelude]: https://github.com/earendil-works/pi/blob/d57c2d21916eaad1158031526ac6ec860eae5f14/packages/codemode/src/runtime/prelude-source.ts
[djs-readme]: https://github.com/earendil-works/djs/blob/05672101b697076a138532888d3c28937f858f2f/README.md
[djs-host]: https://github.com/earendil-works/djs/blob/05672101b697076a138532888d3c28937f858f2f/docs/host.md
[djs-next]: https://github.com/earendil-works/djs/blob/05672101b697076a138532888d3c28937f858f2f/tests/pi/prelude-next.ts
[djs-usage]: https://github.com/earendil-works/djs/blob/05672101b697076a138532888d3c28937f858f2f/docs/codemode-feature-usage.md
[cf-shared]: https://github.com/cloudflare/agents/blob/a2f1216993aa43982ef3550ec0a05e9a4d6d3175/packages/codemode/src/shared.ts#L10-L19
[cf-exec]: https://github.com/cloudflare/agents/blob/a2f1216993aa43982ef3550ec0a05e9a4d6d3175/packages/codemode/src/executor.ts#L236-L256
[cf-trunc]: https://github.com/cloudflare/agents/blob/a2f1216993aa43982ef3550ec0a05e9a4d6d3175/packages/codemode/src/truncate.ts#L1-L22
[cf-readme]: https://github.com/cloudflare/agents/blob/a2f1216993aa43982ef3550ec0a05e9a4d6d3175/packages/codemode/README.md
[smol-utils]: https://github.com/huggingface/smolagents/blob/96f33faaf028479119ec8d34507b47694cf14e34/src/smolagents/utils.py#L49-L61
[smol-exec]: https://github.com/huggingface/smolagents/blob/96f33faaf028479119ec8d34507b47694cf14e34/src/smolagents/local_python_executor.py#L57-L153
[smol-prompt]: https://github.com/huggingface/smolagents/blob/96f33faaf028479119ec8d34507b47694cf14e34/src/smolagents/prompts/code_agent.yaml#L157-L168
[omp-readme]: https://github.com/can1357/oh-my-pi/blob/88a69582c755bd000e8dc0290981fc787a4f400c/README.md
[omp-eval-doc]: https://github.com/can1357/oh-my-pi/blob/88a69582c755bd000e8dc0290981fc787a4f400c/docs/tools/eval.md
[omp-py-doc]: https://github.com/can1357/oh-my-pi/blob/88a69582c755bd000e8dc0290981fc787a4f400c/docs/python-repl.md
[omp-prompt]: https://github.com/can1357/oh-my-pi/blob/88a69582c755bd000e8dc0290981fc787a4f400c/packages/coding-agent/src/prompts/tools/eval.md
[omp-prelude-py]: https://github.com/can1357/oh-my-pi/blob/88a69582c755bd000e8dc0290981fc787a4f400c/packages/coding-agent/src/eval/py/prelude.py#L391-L530
[omp-py-bridge]: https://github.com/can1357/oh-my-pi/blob/88a69582c755bd000e8dc0290981fc787a4f400c/packages/coding-agent/src/eval/py/tool-bridge.ts#L151-L215
[omp-js-bridge]: https://github.com/can1357/oh-my-pi/blob/88a69582c755bd000e8dc0290981fc787a4f400c/packages/coding-agent/src/eval/js/tool-bridge.ts
[omp-js-worker]: https://github.com/can1357/oh-my-pi/blob/88a69582c755bd000e8dc0290981fc787a4f400c/packages/coding-agent/src/eval/js/context-manager.ts#L1006-L1014
[omp-indirect]: https://github.com/can1357/oh-my-pi/blob/88a69582c755bd000e8dc0290981fc787a4f400c/packages/coding-agent/src/eval/js/shared/indirect-eval.ts
[omp-output]: https://github.com/can1357/oh-my-pi/blob/88a69582c755bd000e8dc0290981fc787a4f400c/packages/tui/src/tools/streaming-output.ts#L10-L24
[omp-restart]: https://github.com/can1357/oh-my-pi/blob/88a69582c755bd000e8dc0290981fc787a4f400c/packages/coding-agent/src/eval/executor-base.ts#L113
[omp-approval]: https://github.com/can1357/oh-my-pi/blob/88a69582c755bd000e8dc0290981fc787a4f400c/docs/approval-mode.md#L119
[omp-runner]: https://github.com/can1357/oh-my-pi/commit/8d144e17ecebb4419c7e487fcbc1d6c8b80b68b2
[omp-ipython]: https://github.com/can1357/oh-my-pi/commit/1d414c1383eaebb2b1ef7397cb6f69aa00471023
