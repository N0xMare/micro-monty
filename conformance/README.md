# Conformance corpus

- `monty/`: the 726 test cases from Monty's `crates/monty/test_cases/` at
  pydantic/monty commit `66fff6bfdfa65d3966633ebc9da77057d5d2b3e8`, copied
  unmodified. MIT, Copyright (c) Pydantic Services Inc.; see `THIRD_PARTY.md`.
  Cases configure themselves with a `# /// monty` TOML header and state
  expectations as asserts, `# Return=`, `# Raise=` or a `TRACEBACK` docstring.
- `monty-scripts/`: the CPython-side helpers Monty's datatest imports
  (`scripts/test_fixtures.py`, `run_traceback.py`, `cpython_watchdog.py`) from
  the same commit, unmodified: the emulated host functions, virtual
  filesystem and environment, and the traceback runner.
- `oracle.py`: runs cases on CPython 3.14 the way the CPython side of Monty's
  datatest (`crates/monty-datatest/src/main.rs`) does, and prints one JSON
  object per case. Usage: `python3.14 conformance/oracle.py CASE.py...`.
  With `--engine micro-monty --mm MM` (or `$MM`) it runs each case through
  `mm exec --json` instead; see "The micro-monty engine" below.
- `results/`: `cpython-3.14.jsonl` (one line per case: file, tags, status,
  reason; deterministic) and `summary.md`, both written by the runner; and
  `micro-monty.jsonl` (the same, plus each non-pass's first-cause `category`)
  and `micro-monty.md`.
- `own/` (to come): cases for features Monty lacks.

## Running

```bash
cargo xtask conformance                    # run everything, print, write results/
cargo xtask conformance --filter dict__    # a subset; prints failures in full, writes nothing
cargo xtask conformance --check            # fail if results differ from results/ (`cargo xtask check`)
cargo xtask conformance --engine micro-monty           # mm: writes micro-monty.*
cargo xtask conformance --engine micro-monty --check   # mm: fail on any change
```

The oracle interpreter is `$PYTHON`, else `~/.local/bin/python3.14` (installed
by `.agents/setup`), else `python3.14` on `PATH`; it must be CPython 3.14. Each
case runs in its own process, in parallel across CPUs, with a 30 s timeout.

## Tags

Every case gets one primary tag, derived from its file name and header:

- `excluded`: `refcount__*` (micro-monty has no reference counting).
- `monty-specific`: `xfail` includes `cpython`; the case expects Monty-only
  behaviour, so it should fail on CPython.
- `host`: `call-external` or `mount-fs`; it needs Monty's host fixtures, which
  the oracle emulates on CPython as the datatest does. The engine side needs a
  host to run these. A non-`host` case that needs one carries `host` second.
- `cpython`: everything else.

Statuses: `pass`, `fail`, `error` (malformed case or harness problem), `skip`
(ref-counts expectations, which the datatest does not check on CPython) and
`timeout`; micro-monty adds `crash`.

## What the oracle replicates

Header parsing (strict, as `monty-fixture`), expectation parsing, the
`Return`/`Return.str`/`Return.type`/`Raise` comparisons
(`format_cpython_exception`), `TRACEBACK` cases through `run_traceback.py`
(recursion limit 50 + 5, frame filtering, caret normalisation), the async
wrapper for `run-async`, the shared fixture globals for every case, the
`mount-fs` temp tree as `root` (first on `sys.path`), `TZ` from `timezone`
(else UTC), and `__name__`/`__file__` for `cpython-main-module`. Code runs as
pyo3 runs it: compiled as `<string>` without inherited flags, on a non-main
thread with no Python frames below it, so recursion-limit cases see the same
depth budget.

Differences: each case gets a fresh interpreter (the datatest shares one, so
module state, `sys.modules` and the virtual filesystem can leak between its
cases); the case's stdout is discarded; the datatest's watchdog is the runner's
timeout. Monty's CI runs the CPython side on 3.14, so `cpython` cases failing
on 3.14 reflect differences between CPython patch releases or real divergences; see `results/summary.md`.

## The micro-monty engine

`cargo xtask conformance --engine micro-monty` builds `mm` with `cargo build
--release -p mm-cli` (or uses `$MM`) and runs `oracle.py --engine micro-monty`
per case, in parallel. The oracle parses the header and expectation exactly as
for CPython, then does what the Monty side of the datatest (`try_run_test`)
does: the case's code without its expectation (the last-line comment or the
`TRACEBACK` docstring) runs as one module named after the case file, through
`mm exec --json --recursion-limit 51 FILE` in a scratch directory, with a limit of 10 s of mm's own CPU time
(`RLIMIT_CPU`, so a loaded machine does not turn a slow case into a
timeout) and a wall-clock backstop six times as long. Then:

- `Return=` / `Return.str=` / `Return.type=` compare the `repr` / `str` /
  type name of the final expression statement (`None` if the code ends with
  another statement); plain-assert cases pass when mm reports no exception;
  `Raise=` compares `Type('message')` built as `format_cpython_exception`
  builds it; `TRACEBACK` compares the traceback after the CPython side's
  normalisation (`run_traceback.normalize_debug_range`, trailing whitespace).
- `call-external` cases run with `mm exec --host-fixtures`: `mm` registers
  the datatest's fixture functions (`add_ints`, ..., `async_call`,
  `async_fail`) as host functions and the `tools`/`tools.nested` host
  modules, defines the `CONST_*` names as globals, answers sync calls in
  place and `async_call`/`async_fail` with futures resolved when the cell
  waits. The host-object fixtures (`make_point`, ...) raise
  `NotImplementedError` until classes exist (M4).
- Module dunders as on the CPython side: a case runs in a namespace without
  `__name__` (so code sees the builtins module's, functions report `f()`
  and classes have no `__main__.` prefix), except traceback cases (which
  CPython runs through `runpy.run_path(run_name='__main__')`) and the
  `cpython-main-module` header, which get `mm exec --main-module`
  (`__name__ == '__main__'`, `__file__` the absolute path).
- Skipped: `excluded` cases (`refcount__*`), `mount-fs` cases ("needs the
  mount-fs host filesystem") and ref-counts expectations.
- `crash` is a panic, a signal, a non-zero exit or no JSON report from `mm`;
  `timeout` is a case past 10 s of CPU time (or 60 s of wall-clock time).
  Both are statuses of their own.

Normalisations, each because mm and the datatest differ in how a case is run,
not in what the engine computes:

- the scratch file's absolute path in a traceback becomes the case's base
  name, as `run_traceback.py` does on CPython and as Monty names the module;
- the recursion limit is 51: Monty's datatest uses `max_recursion_depth(50)`,
  which does not count the module frame, and mm's limit counts it (as
  CPython's does);
- a compile error, which `mm` reports as `python FILE` prints one, gets the
  `Traceback (most recent call last):` header that CPython's `runpy` (and
  Monty's parse errors) put on it;
- mm reports `str(exc)`, not `args[0]`; for `KeyError` the key is read back
  from its repr. Other exceptions whose `str` differs from `args[0]` (several
  args, a non-str argument) can show as `wrong exception`.

Every non-pass gets a first-cause `category`: `does not support X` when mm's
error says "micro-monty does not support X" (`async` merged), `bigint` for
64-bit int limits, `missing builtin` for a `NameError` on a CPython builtin,
else `wrong output`, `wrong exception`, `wrong traceback`, `no exception
raised`, `unexpected <Type>`, `crash` or `timeout`. `micro-monty.md` counts
them, largest first.

`--check` fails on any change to `micro-monty.jsonl` and lists newly failing
cases (regressions), newly passing ones (record them by rerunning without
`--check` and committing) and other changes separately. `cargo xtask check`
runs `--check` on both engines.

To refresh the corpus, replace `monty/` and `monty-scripts/` from a newer Monty
commit, update the commit here and in `THIRD_PARTY.md`, and rerun
`cargo xtask conformance`.
