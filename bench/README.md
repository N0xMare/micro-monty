# Benchmarks

The seven programs of `docs/spec.md` §12, plus six that cover other common
Python (classes, generators, exceptions, formatting, sorting and a small
interpreter), each as a Python file and an equivalent JavaScript file for
QuickJS. Each prints one checksum line, the same
on every engine; `expected.txt` holds them.

| Name | Files | Work |
|---|---|---|
| startup | `startup.*` | an empty program (prints `0`) |
| fib | `fib.*` | `fib(27)`, recursive, through a global lookup |
| loop | `loop.*` | module-level sum over `range(3_000_000)` |
| dict | `dict.*` | 200k string keys: insert, then look each up |
| str | `str.*` | build a ~1 MB string, split it, replace in it |
| json | `json_rows.*` | 50k rows: `dumps`, then `loads` |
| closure | `closure.*` | 1M calls of a closure |
| classes | `classes.*` | 2k small objects stepped 60 times through methods that read and write attributes; 200k `__init__` calls |
| generators | `generators.*` | a three-stage generator pipeline, `yield from`, generator expressions in `sum` and `any` |
| exceptions | `exceptions.*` | 100k iterations raising and catching: a builtin exception, a custom subclass with `else`/`finally`, `KeyError` |
| fmt | `fmt.*` | 60k records through f-strings with format specs, `%` formatting and `str.join` |
| sort | `sort.*` | list comprehensions; 50k tuples sorted by key functions (lambda, tuple key, reverse, stable ties) |
| interp | `interp.*` | tokenize, parse (recursive descent) and evaluate 300 statements of a tiny language, 25 rounds |

`json` lives in `json_rows.*` so the script cannot shadow Python's `json`
module. The checksums avoid floats, so Python and JavaScript print the same. The JS
twins use the nearest idiom where JavaScript has no equivalent: generator
functions for generator expressions, comparators for key functions, a thrown
error for a missing `Map` key, and a flooring `%` in `interp`'s operator table.
The six added programs take 80–115 ms each on CPython 3.14 [M].

## Running

```bash
cargo xtask bench                          # all engines, 10 reps, writes results/
cargo xtask bench --engines cpython,qjs --reps 20
cargo xtask bench --quick --engines cpython    # 1 run each, checksums only (CI)
```

Every repetition is a separate process timed by wall clock from spawn to exit,
so startup counts (like hyperfine), after one checked warm-up run. Engine order
rotates each round within each program. The runner reports median and min,
retains every raw sample, checks each checksum, and pins the child with
`taskset -c 2` when possible. Do not run other work alongside it.

Engines: `cpython` (required; `$PYTHON`, else `~/.local/bin/python3.14`, else
`python3.14`), `qjs` (`$QJS` or `qjs` on `PATH`), `monty` (`$MONTY` or `monty`
on `PATH`; the Monty CLI ships in the `pydantic-monty-runtime` wheel) and
`micro-monty` (`mm run`; `$MM`, else `target/release/mm`, built in release
first). These are the default engines. Additional explicit configurations are
`dquickts` (`$DQUICKTS`), `qpy` (`$QPY`, run with `-B`), `djs` (`$DJS`, run with
`run`) and `micro-monty-main` (`$MM_MAIN`, an independently built main control).
**Every explicitly requested engine is required.** Without `--engines`, absent
optional qjs/Monty executables are skipped with a note.

An exception, wrong checksum or crash is a **failure on every engine**, not an
automatically inferred unsupported feature. Quick mode stops at the first
failure. Timed mode retains successful cells and failure details, then exits
nonzero if any cell failed. Determine missing capabilities separately from
actual error evidence; failed cells receive no performance score.

Results: `results/<engine>.txt` (machine, version, median/min and failures),
`samples.jsonl` (ordered warmup and timed samples, stdout/stderr, exit status),
`manifest.json` (binary hashes, source HEAD/status, toolchain and boundaries),
and `summary.md` (**only this invocation's engines**, never older files).
Use `--out-dir PATH` for an isolated campaign; relative paths are workspace-root
relative. `--quick` still writes nothing. `--timeout-secs N` optionally wraps
each run in GNU `timeout --kill-after=1 N`; its process overhead is included
and recorded, so compare like configurations. A timeout remains a failure.
`results/instructions.md` holds historical callgrind counts, not a current
comparison; `python3 bench/callgrind.py` measures current ones (per program
and engine, outputs checked: the gated Ir, from glibc's `__libc_start_main`
onward without `pthread_getattr_np`'s `/proc/self/maps` parsing, which the
±0.5% bench rule applies to, and the whole-process Ir; docs/spec.md §12) and `python3 bench/opcost.py` the
Ir of single operations (docs/research/interpreter-speed.md). See
[evidence adapters](evidence/) for reproducible public-API probes.

## Native-operation and persistent-session latency

`latency.rs` is a standalone Rust driver using the public facade. Its header
contains the build/run commands, including explicit selection of the CLI-feature
facade library. Run it alone on an idle machine; it is not part of the quick gate.
It measures actual request-to-control latency during large native operations,
distinguishes an explicit park from a host-call stop, and checks results after
resuming. Persistent-session samples measure cell start, snapshot (including GC)
and restore at several graph sizes, dropping the old session before restoration.

`results/native-latency.txt` records the initial before/after measurements and
limitations. These are not worst-case bounds or throughput acceptance thresholds;
one native operation can still finish before yielding control.
