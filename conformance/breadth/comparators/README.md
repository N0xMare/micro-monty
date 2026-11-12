# Correctness breadth: micro-monty vs Monty 1.4.0 vs qpy, against CPython 3.14.7

Measured in an orb on 2026-11-04. All numbers are `[M]` (measured here) unless
marked `[S]` (taken from a source). This measures correctness only. No timings
were taken, and no engine source, corpus file or `conformance/oracle.py` was
edited.

## Engines, builds and hashes

| Engine | Binary | How it runs a case | SHA-256 |
|---|---|---|---|
| CPython 3.14.7 (oracle) | `/home/user/.local/bin/python3.14` (uv-managed) | `python3.14 -B FILE` | `241bf774a81580bb760adf960b48df7e6b23b1de026c9c912d4ca6be1a08241e` (matches `bench/evidence/baselines/README.md`) |
| micro-monty (frozen `main` build, not rebuilt) | `/tmp/mm-evidence-layers/main/mm` | `mm run FILE` | `ec95bd9e7dc963747fbb205f7ef2da3438543f329cd429778e99e47af91f8b8b` (matches that directory's `artifacts.sha256`) |
| Monty 1.4.0 (pydantic, published CLI) | `/tmp/mm-evidence-baselines/venv/bin/monty` | `monty FILE` | `4479e7306fdf0c6a2a567aa139150ac47c75ab3c0138ec2c4efa010c382a87e0` (**matches** the README) |
| qpy (dquickts `feat/py` @ `68be18f241e2c92a9a54fbcc3e6ceb3ad4914955`) | `/tmp/mm-evidence-dquickts/py/qpy` | `qpy -B FILE`, `QPYHOME=/tmp/mm-evidence-dquickts/py` | `f6273964f0abf38cfcdcf151c59b8218d353ae8da910f8c9605a3570bb367131` (**differs** from the README's `59ecbe59…`; see below) |

### Monty 1.4.0

This follows `bench/evidence/baselines/README.md`, with `B=/tmp/mm-evidence-baselines`.
Only the Monty venv was rebuilt; stock QuickJS and the warm hosts are not
needed here.

```bash
uv venv --python /home/user/.local/bin/python3.14 "$B/venv"
uv pip install --python "$B/venv/bin/python" --require-hashes \
  --only-binary :all: -r bench/evidence/baselines/requirements-linux-x86_64-cp314.lock
```

uv 0.12.9 installed `pydantic-monty`, `pydantic-monty-client` and
`pydantic-monty-runtime` 1.4.0, plus `typing-extensions` 4.16.0, with
`--require-hashes`. The four wheels were also fetched into `$B/wheels/` from
the PyPI JSON URLs (`$B/wheels.txt`). All four pass `sha256sum -c` against
the lock's hashes. The installed `venv/bin/monty` (31551096 bytes) and the
client `_monty.cpython-314-x86_64-linux-gnu.so`
(`d22875ef78df13925b3233e6337ea0a45f03a2e9ecbaa6bba01989d94d2cd2ad`) both
match the README. `monty -V` prints `monty-runtime 1.4.0`.

### qpy

This follows `bench/evidence/dquickts/README.md`. Both pins resolved:
`master` = `b51e92c0…`, and the `feat/py` worktree is at `68be18f2…`. Only
`qpy` and `libquickjs.a` were built; the JS `qjs` and `api_probe` were not
needed. The Debian bookworm `clang-14`, `libclang-cpp14`, `libllvm14`,
`libclang-common-14-dev` and `libclang1-14` 1:14.0.6-12 packages were fetched
with `apt-get download` (verified by apt against the signed archive; hashes
in `/tmp/mm-evidence-dquickts/toolchain/packages.sha256`). They were
extracted with `dpkg-deb -x` into `toolchain/root`, and nothing was installed
system-wide.

```bash
cd /tmp/mm-evidence-dquickts
export LD_LIBRARY_PATH=/tmp/mm-evidence-dquickts/toolchain/root/usr/lib/x86_64-linux-gnu
compiler='/tmp/mm-evidence-dquickts/toolchain/root/usr/bin/clang-14 -include limits.h -include ctype.h'
make -C py -j4 OBJDIR=.obj-pure-clang CONFIG_CLANG=y \
  CC="$compiler" HOST_CC="$compiler" CONFIG_PYTHON=y CONFIG_PYTHON_CAPI= \
  CONFIG_QPY_NO_BASE=y qpy libquickjs.a        # log: build-py-nobase.log
```

Apart from `-j4` instead of `-j2`, the command is the README's. The build
succeeded first time and `git status` shows only the untracked object
directory. **Hash mismatch:** `py/qpy` is `f6273964…` (23273120 bytes) and
`py/libquickjs.a` is `1076a4a9…`, against the README's `59ecbe59…` and
`af4e8cfd…`. `python/sysmodule.c`, `codeobject.c` and `capi_stable.c` use
`__DATE__`/`__TIME__`, and the binary embeds `3.15.2 (main, Nov  4 2026,
21:22:56) [Clang 14.0.6 ]`, so no rebuild can reproduce the recorded hash.
The README itself disclaims reproducibility. The rebuilt binary does
reproduce the README's smoke observation: 12/13 bench programs, with
`json_rows.py` failing on `_sre`.

This configuration matters for the results. `CONFIG_PYTHON_CAPI=` means none
of the C extension modules exist: `itertools`, `math`, `_sre` (so `re` and
`json`), `_csv`, `binascii`, `unicodedata`, `_struct` and `cmath`. qpy's
pure-Python `collections`, `dataclasses`, `asyncio`, `typing`, `datetime`,
`random`, `decimal` and `tomllib` import some of them, so 197 of qpy's 228
failures are `ModuleNotFoundError: Standard library module 'X' was not found`.
A CAPI-enabled build was not attempted, because the README does not build one.

## Commands

```bash
# Every case on all four engines, 4 parallel processes, 30 s timeout each:
python3.14 -I conformance/breadth/comparators/run.py --jobs 4
# Tables and cause lists below (from results.jsonl):
python3.14 -I conformance/breadth/comparators/run.py --report
```

Engine paths can be overridden with `PYTHON`, `MM`, `MONTY`, `QPY` and
`QPYHOME`. The full run (2940 processes) takes about a minute. It was run
three times on a busy machine (load up to 29) and gave identical per-case
categories each time.

## Method

- **Corpora.**
  - `own`: all 154 `conformance/own/*.py`, run unchanged as scripts.
  - `monty`: the 568 `conformance/monty/*.py` cases whose primary tag in
    `conformance/results/cpython-3.14.jsonl` is `cpython`. This is the
    comparable set; `host`, `monty-specific` and `excluded` cases are left
    out. Each case is parsed with `oracle.py`'s own `parse_fixture`,
    `split_code_for_module` and `wrap_code_for_async`, imported read-only.
    The script is the case's code without its expectation comment or
    `TRACEBACK` docstring. `Return=`, `Return.str=` and `Return.type=` cases
    end with `print(repr(EXPR))`, `print(str(EXPR))` or
    `print(type(EXPR).__name__)` of the final expression line. The 19
    `run-async` cases get the datatest's `asyncio.run` wrapper.
  - `bench`: the 13 `bench/*.py` programs; stdout must equal
    `bench/expected.txt` exactly.
- **Same file, same conditions.** Every engine runs the identical file as a
  script, in a fresh process with an empty scratch working directory. The
  environment has `PYTHON*` variables removed, plus `PYTHONHASHSEED=0`,
  `PYTHONDONTWRITEBYTECODE=1` and `TZ` (the header's `timezone`, else
  `UTC`, as `oracle.py` sets). The timeout is 30 s. No engine gets
  recursion-limit or other flags.
- **Pass.** Each engine's observable result is compared with CPython's run
  of the same file: stdout, and the final uncaught exception's type (without
  module prefix) and message, read from the traceback on stderr. Paths
  become base names and `0x…` addresses are masked. As `oracle.py` compares
  `args[0]`, two display-only differences are normalised for every engine:
  - CPython-style "Did you mean …?" suggestions appended by the traceback
    printer are dropped.
  - A `KeyError` message that is a string literal is read back to the key,
    because Monty's CLI prints `KeyError: b` where CPython prints
    `KeyError: 'b'`; in-language, Monty's `str(e)` is `'b'`.
- **Categories of failure.** A failure is never dropped from the
  denominator. Each gets one category:
  - `timeout`: over 30 s.
  - `crash`: a signal, panic, fatal error, or non-zero exit with no
    exception.
  - `unsupported-syntax`: a `SyntaxError` where CPython has none.
  - `unsupported-feature`: a `NotImplementedError` or an
    "(does) not (yet) supported" message.
  - `missing-module`: `ModuleNotFoundError` or `ImportError`.
  - `missing-name` and `missing-attribute`: `NameError` and `AttributeError`.
  - `unexpected-exception`, `wrong-exception`, `no-exception` and
    `wrong-stdout`.

  A finer `cause` per failure comes from a regex table in `run.py`. No
  engine crashed, and the only timeouts are Monty's 2.
- **Files.**
  - `run.py`: the runner and report.
  - `results.jsonl`: one line per case and engine, 2940 lines. Fields are
    `category`, `cause`, `exc`, `ref_exc`, `stdout_diff` (the first
    differing line), `raised_at` (the last frame and source line), and for
    non-passes `stdout`/`stderr_tail`. Monty-corpus rows also carry the
    expectation kind; CPython rows carry `expected_ok`.

### Reference-sensitive cases (11 of the 568)

In 11 cases, CPython's script run disagrees with the case's encoded
expectation (`expected_ok: false`). Each still counts against the CPython
reference as for any other case. They are listed here because they favour
an engine that behaves like a CPython *script* over one that behaves like
Monty's embedded `exec`:

- `__main__.` prefixes (6): `class__repr`, `function__call_duplicate_kwargs`,
  `function__err_unpack_duplicate_arg`, `function__err_unpack_duplicate_heap`,
  `function__err_unpack_int` and `function__err_unpack_not_mapping`. A
  script's functions and classes live in `__main__`, while the datatest
  `exec`s without `__name__`. micro-monty (`mm run`) behaves as a script;
  Monty's CLI does not.
- CPython 3.14.7 vs the cases' "precision too big" expectations (4):
  `str__format`, `str__percent_format`, `bytes__percent_format` and
  `fstring__all`. These are the 4 cases that also fail in
  `conformance/results/summary.md`. Monty satisfies the case's own
  expectation here, but CPython does not.
- `async__not_imported` (1): the `asyncio.run` wrapper imports `asyncio`.

On the remaining 557 cases: micro-monty 539 (96.8%), Monty 529 (95.0%) and
qpy 385 (69.1%).

## Results

| Corpus | Cases | CPython | micro-monty | Monty 1.4.0 | qpy |
|---|---:|---:|---:|---:|---:|
| `conformance/own` | 154 | 154 | **154 (100%)** | 42 (27.3%) | 106 (68.8%) |
| `conformance/monty`, `cpython` tag | 568 | 568 | **549 (96.7%)** | 530 (93.3%) | 389 (68.5%) |
| `bench/*.py` | 13 | 13 | **13** | 11 | 12 |

Caveats on what these numbers show:

- `conformance/own` was written for micro-monty's own development, so
  micro-monty's 100% there is expected and not neutral.
- `conformance/monty` is Monty's own corpus at `66fff6bf`. That commit is
  28 commits *after* the 1.4.0 tag `4b015dc4` (GitHub compare: ahead 28,
  behind 0), so a few Monty failures may be features added after the
  release.
- qpy's pure-Python results are dominated by the missing C modules of its
  documented configuration. Leaving out `missing-module` failures, qpy
  passes 389/410 (94.9%) of the Monty corpus and 106/116 of own.

## Per corpus: pass and failure categories

Generated by `run.py --report`.

### `own` (154 cases)

| Engine | Pass | % | Failure categories |
|---|---:|---:|---|
| cpython | 154/154 | 100.0 | - |
| micro-monty | 154/154 | 100.0 | - |
| monty | 42/154 | 27.3 | unsupported-feature 62, missing-attribute 17, unexpected-exception 9, missing-module 8, wrong-stdout 8, wrong-exception 4, timeout 2, missing-name 1, unsupported-syntax 1 |
| qpy | 106/154 | 68.8 | missing-module 38, wrong-stdout 8, unexpected-exception 1, wrong-exception 1 |

### `monty` (568 cases)

| Engine | Pass | % | Failure categories |
|---|---:|---:|---|
| cpython | 568/568 | 100.0 | - |
| micro-monty | 549/568 | 96.7 | unexpected-exception 12, unsupported-feature 6, wrong-exception 1 |
| monty | 530/568 | 93.3 | unexpected-exception 13, missing-module 8, wrong-exception 6, no-exception 5, missing-attribute 4, unsupported-syntax 2 |
| qpy | 389/568 | 68.5 | missing-module 158, unexpected-exception 11, wrong-exception 6, unsupported-feature 3, no-exception 1 |

### `bench` (13 cases)

| Engine | Pass | % | Failure categories |
|---|---:|---:|---|
| cpython | 13/13 | 100.0 | - |
| micro-monty | 13/13 | 100.0 | - |
| monty | 11/13 | 84.6 | unsupported-feature 2 |
| qpy | 12/13 | 92.3 | missing-module 1 |

## Main failure causes per engine

**micro-monty: 19 failures, all in the Monty corpus.**

- `exec`/`eval` are not supported (5): `builtin__eval_exec`,
  `builtin__exec_import_traceback`, `builtin__exec_namespaces`,
  `builtin__exec_traceback` and `inline__string_operations`.
- `typing.get_type_hints` returns different results (4).
- Comparisons of `deque` and namedtuple with a NaN element (2). CPython's
  identity shortcut makes `deque([nan]) < deque([nan, 1])` false; mm
  returns `True`.
- Exception-message text (3):
  - `args__macro_errors`: mm says `replace expected at least 2 arguments,
    got 1`.
  - `class__numeric`: mm says `__float__ returned non-float` without the
    `C.` qualifier.
  - `iter__user_class`: the `''.join` message for a class with
    `__iter__ = None`.
- Single cases (5):
  - `json__module`: no `json.loads` hooks.
  - `functools__lru_cache`: after a re-entrant call during eviction,
    `reentrant_calls` is not CPython's `[1, 2, 3, 4, 5]`.
  - `lookup__mutation`: a dict lookup whose `__eq__` mutates the dict.
  - `id__non_overlapping_lifetimes_same_types`: `id([]) == id([])` is False
    on mm, while CPython reuses the freed address.
  - `str__format`: `'{:５}'` with a full-width digit raises `ValueError` on
    mm. This is one of the reference-sensitive cases, where CPython 3.14.7
    fails a different assertion.

**Monty 1.4.0: 152 failures (112 own, 38 Monty corpus, 2 bench).**

- Parser/compiler gaps (67):
  - no class inheritance or metaclasses (24, including bench
    `exceptions`);
  - no `yield` (15, including bench `generators`);
  - no `del` statement (15);
  - no `classmethod`/`staticmethod`/`property` (4);
  - no wildcard import (3);
  - `nonlocal` binding rejected at compile time (2:
    `issue369__function_default_walrus`, `nonlocal__ops`);
  - class bodies limited to methods and simple variables (2);
  - keyword-argument form not parsed (1);
  - format spec rejected at compile time (1).
- Missing modules or imports (16): `fnmatch` (3), `gc` (2),
  `typing.get_type_hints` (4), `cmath`, `difflib`, `heapq`, `html`, `io`,
  `shlex` and `statistics`.
- Missing attributes or builtins (22):
  - `sys.setrecursionlimit`/`getrecursionlimit` (5);
  - `type.__name__` on exception types in `except`/`__exit__` (3);
  - `sys.stdout.write` (2);
  - `function.__name__`/`__defaults__`;
  - `__context__`;
  - `re.subn`;
  - `Match.lastindex`;
  - `datetime.isocalendar`;
  - `time.struct_time`;
  - `unicodedata.numeric`;
  - `str.upper` as an unbound method;
  - `callable`.
- `json.dumps(default=...)` and other unsupported keyword arguments (3), and
  `raise E(a, b)` with several arguments as an internal error (2).
- Behaviour differences (42):
  - **10 of the 11 reference-sensitive cases.** In 4 of them Monty
    satisfies the case's own expectation but CPython 3.14.7 does not. The
    other 6 are script `__main__.` qualifiers; own
    `63_genexpr_and_star_calls` is a seventh.
  - Float results differ in the last digit (`1.0` vs `0.9999999999999999`;
    3 own cases).
  - Set and `**kwargs` ordering (2).
  - `next()` fails with `'list' object is not an iterator` on what CPython
    gives as an iterator (2).
  - `sorted`/`min`/`max` of user classes raise `'<' not supported` (2).
  - `str.find`/`bytes.find` with an empty needle past the end returns 5,
    not -1 (2).
  - `datetime.astimezone()` ignores `TZ` (4).
  - The `AssertionError` message includes the assert source (2).
  - `{True: 'a', 1: 'b'}` repr, function repr quoting, `csv`
    `QUOTE_NONNUMERIC`, `float(bytes)`, `Counter` repr with `None`, the
    `list += int` message, "precision too big" raised early, a big-shift
    `OverflowError`, `TypeError` suggestion text, and the order of
    `iter(callable, sentinel)` `__eq__`, `bytes(list)` with an out-of-range
    byte, and `%` formatting with user mapping objects.
  - 2 timeouts (`28_list_ops`, `94_truthiness`: no progress after 30 s;
    CPython finishes instantly).

**qpy (dquickts `feat/py`, pure-Python configuration): 228 failures.**

- C extension modules absent in this build (`CONFIG_PYTHON_CAPI=`) (197):
  - `itertools` (104). It is imported by qpy's pure-Python
    `collections`, `dataclasses` and `asyncio` (which covers all 19
    `run-async` cases) and by modules built on them.
  - `math` (55). This includes `datetime`, `random`, `decimal` and
    `tomllib`.
  - `_sre` (21): `re`, `json`, and bench `json_rows`.
  - `_csv`, `binascii` and `unicodedata` (5 each).
  - `_struct` and `cmath` (1 each).
- `hashlib` algorithms absent (3).
- Version identity and newer message texts (16). qpy reports Python 3.15.2,
  and the newer forms below are consistent with that, though this was not
  checked against a CPython 3.15:
  - `version_info` is `(3, 15)` (2);
  - `sys.flags.utf8_mode` (1);
  - `X.__bool__() must return a bool, not int`, `… __repr__() must return
    a str …`, `… __iter__() must return an iterator …` and `format
    argument: %x requires an integer` (5);
  - `Value after **/* must be …` without the callee name (5);
  - `slice indices must be integers or have an __index__ method` (2);
  - `duplicate parameter` (1).
- Other differences (12): `str(e)` texts checked by asserts
  (`builtin__eval_exec`, `class__dynamic_type`, `complex__all`,
  `function__call_unpack`, `import__dotted_error`, `str__ops`), `codecs`
  resolving `utf.8`, `bytes` OverflowError at i64::MIN, `bisect` with an
  `__index__` key, `bytes % dict`, and the two "precision too big" cases
  (`fstring__all`, own `208_format_edges`).

## Where micro-monty fails but a comparator passes

micro-monty fails 19 cases. A comparator passes 14 of them, all in the
Monty corpus:

| Case | Monty | qpy | Cause on micro-monty |
|---|---|---|---|
| `builtin__exec_import_traceback` | pass | pass | no `exec()` |
| `builtin__exec_traceback` | pass | pass | no `exec()` |
| `id__non_overlapping_lifetimes_same_types` | pass | pass | `id([]) == id([])` is False (no immediate address reuse) |
| `builtin__exec_namespaces` | pass | missing `math` | no `exec()` |
| `builtin__eval_exec` | pass | assertion | no `eval()` |
| `inline__string_operations` | pass | no sha256 | no `eval()` |
| `json__module` | pass | missing `_sre` | no `json.loads` hooks |
| `args__macro_errors` | pass | missing `itertools` | `str.replace` arity message |
| `class__numeric` | pass | missing `math` | `__float__` non-float message lacks the class name |
| `collections__deque` | pass | missing `itertools` | NaN element ordering |
| `collections__namedtuple` | pass | missing `itertools` | NaN element ordering |
| `functools__lru_cache` | pass | missing `itertools` | re-entrant eviction call count |
| `iter__user_class` | pass | missing `itertools` | `''.join` message for `__iter__ = None` |
| `lookup__mutation` | pass | missing `itertools` | dict lookup when `__eq__` mutates the dict |

**micro-monty alone fails** (both comparators pass) 3 cases:
`builtin__exec_import_traceback`, `builtin__exec_traceback` and
`id__non_overlapping_lifetimes_same_types`. The other 5 micro-monty failures
fail on Monty and qpy too: the 4 `typing__get_type_hints*` cases and
`str__format`.

In the other direction, micro-monty is the only engine to pass 45 own cases
and 21 Monty-corpus cases. All three fail together on 5 Monty-corpus cases.
Monty passes all 14 cases in the table. It is Monty's own corpus.

## All failures by cause (generated)

The `cause` labels come from `run.py`'s regex table. "assertion failed" and
"different stdout" are behaviour differences; `raised_at` and `stdout_diff`
in `results.jsonl` give the line for each.

### micro-monty: 19 failures by cause

- **assertion failed (semantic difference)** (12): `args__macro_errors`, `class__numeric`, `collections__deque`, `collections__namedtuple`, `functools__lru_cache`, `id__non_overlapping_lifetimes_same_types`, `iter__user_class`, `lookup__mutation`, `typing__get_type_hints`, `typing__get_type_hints_classes`, `typing__get_type_hints_dataclasses`, `typing__get_type_hints_functions`
- **no exec()** (3): `builtin__exec_import_traceback`, `builtin__exec_namespaces`, `builtin__exec_traceback`
- **no eval()** (2): `builtin__eval_exec`, `inline__string_operations`
- **no json.loads hooks** (1): `json__module`
- **wrong-exception: ValueError** (1): `str__format`

### monty: 152 failures by cause

- **no class inheritance/metaclasses** (24): `110_builtin_subclass` (own), `111_namedtuple` (own), `113_dataclasses_more` (own), `115_typing_runtime` (own), `118_operator_abc_string_textwrap` (own), `204_metaclasses` (own), `205_enum` (own), `206_abc_collections_abc` (own), `311_metaclasses` (own), `313_metaclass_prepare` (own), `315_logging` (own), `317_pprint` (own), `320_struct` (own), `69_inheritance_mro` (own), `70_super_chains` (own), `72_operator_overloading` (own), `77_exception_subclasses` (own), `82_repr_str_format` (own), `83_class_creation_hooks` (own), `91_tb_user_exception_chain` (own), `93_mixed_comparisons` (own), `96_polymorphic_sites` (own), `97_new_corners` (own), `exceptions` (bench)
- **no del statement** (15): `05_unboundlocal` (own), `109_small_dicts` (own), `13_del_augassign` (own), `29_dict_ops` (own), `309_for_range` (own), `310_bounded_comparisons` (own), `46_dict_order_deletion` (own), `50_list_slicing_edges` (own), `53_scoping_more` (own), `68_class_basics` (own), `74_hash_eq_dicts` (own), `75_getattr_setattr` (own), `81_container_protocol` (own), `88_shapes_and_caches` (own), `98_dunder_lookup_rules` (own)
- **no yield (generators)** (15): `101_with_and_generators` (own), `103_tb_generators_methods` (own), `200_match_patterns` (own), `313_contextlib` (own), `314_traceback` (own), `322_async_iter_with` (own), `323_contextlib_async` (own), `78_generators_send_throw` (own), `79_yield_from` (own), `80_lazy_genexpr` (own), `85_recursion_through_dunders` (own), `89_generator_iterators` (own), `95_generator_finally` (own), `99_iteration_corners` (own), `generators` (bench)
- **assertion failed (semantic difference)** (11): `12_walrus_assert` (own), `39_err_assert` (own), `bytes__methods`, `csv__writer`, `datetime__zone_kathmandu`, `datetime__zone_london`, `datetime__zone_new_york`, `datetime__zone_sydney`, `dict__ops`, `function__ops`, `str__methods`
- **different stdout** (8): `02_arg_errors` (own), `107_math` (own), `314_calliter_user_eq` (own), `31_builtins` (own), `33_sets` (own), `51_call_binding_errors` (own), `60_numeric_mix` (own), `63_genexpr_and_star_calls` (own)
- **wrong-exception: TypeError** (8): `104_bytes` (own), `87_builtins_user_objects` (own), `function__call_duplicate_kwargs`, `function__err_unpack_duplicate_arg`, `function__err_unpack_duplicate_heap`, `function__err_unpack_int`, `function__err_unpack_not_mapping`, `type_error__list_iadd_int`
- **unexpected-exception: TypeError** (6): `15_comprehensions` (own), `203_percent_user_objects` (own), `54_iterators_views` (own), `73_rich_compare_sorting` (own), `collections__counter`, `float__from_str`
- **no exception raised** (5): `bytes__percent_format`, `class__repr`, `fstring__all`, `str__format`, `str__percent_format`
- **missing sys.setrecursionlimit** (4): `json__dumps_recursion`, `recursion__deep_copy`, `recursion__deep_repr`, `recursion__limit_depth`
- **missing typing.get_type_hints** (4): `typing__get_type_hints`, `typing__get_type_hints_classes`, `typing__get_type_hints_dataclasses`, `typing__get_type_hints_functions`
- **no classmethod/staticmethod/property** (4): `114_functools_more` (own), `71_properties_descriptors` (own), `86_traceback_methods` (own), `90_super_edges` (own)
- **missing keyword argument support** (3): `106_json` (own), `json__dumps_errors`, `json__dumps_formatting`
- **missing module fnmatch** (3): `305_os_path_pathlib_fnmatch` (own), `fnmatch__module`, `fnmatch__posix`
- **missing type.__name__** (3): `22_exc_hierarchy` (own), `76_context_managers` (own), `92_tb_with_statement` (own)
- **no wildcard import** (3): `207_wildcard_import` (own), `312_wildcard_imports` (own), `316_urllib_parse` (own)
- **class bodies limited to methods and simple variables** (2): `102_class_scope_genexpr` (own), `117_class_annotations` (own)
- **internal error: exception with several args** (2): `17_exceptions_basic` (own), `49_exc_flow_nested` (own)
- **missing _io.TextIOWrapper.write** (2): `65_async_gather` (own), `67_imports` (own)
- **missing module gc** (2): `functools__gc`, `itertools__gc`
- **nonlocal binding rejected at compile time** (2): `issue369__function_default_walrus`, `nonlocal__ops`
- **timeout** (2): `28_list_ops` (own), `94_truthiness` (own)
- **missing ValueError.__context__** (1): `19_context_chain` (own)
- **missing builtin_function_or_method.__name__** (1): `100_errors_in_dunders` (own)
- **missing collections.__name__** (1): `112_stdlib_imports` (own)
- **missing datetime.datetime.isocalendar** (1): `302_datetime_format` (own)
- **missing function.__defaults__** (1): `01_args_defaults` (own)
- **missing function.__name__** (1): `08_decorators` (own)
- **missing module cmath** (1): `202_complex_cmath` (own)
- **missing module difflib** (1): `319_difflib` (own)
- **missing module heapq** (1): `116_copy_heapq_bisect` (own)
- **missing module html** (1): `321_html` (own)
- **missing module io** (1): `306_csv_io` (own)
- **missing module shlex** (1): `318_shlex` (own)
- **missing module statistics** (1): `307_fractions_statistics_uuid` (own)
- **missing name callable** (1): `84_callables_methods` (own)
- **missing re.Match.lastindex** (1): `300_re_differential` (own)
- **missing re.subn** (1): `301_re_edges` (own)
- **missing sys.getrecursionlimit** (1): `108_sys_sets_misc` (own)
- **missing time.struct_time** (1): `308_time_struct` (own)
- **missing unicodedata.numeric** (1): `136_decimal_unicodedata` (own)
- **missing-attribute: AttributeError** (1): `313_native_sort_keys` (own)
- **parser does not support keyword arguments** (1): `66_async_errors` (own)
- **unexpected-exception: OverflowError** (1): `105_bigint` (own)
- **unexpected-exception: ValueError** (1): `208_format_edges` (own)
- **unsupported-syntax: SyntaxError** (1): `52_builtin_error_messages` (own)

### qpy: 228 failures by cause

- **C module itertools absent (CONFIG_PYTHON_CAPI off)** (104): `111_namedtuple` (own), `112_stdlib_imports` (own), `113_dataclasses_more` (own), `114_functools_more` (own), `115_typing_runtime` (own), `116_copy_heapq_bisect` (own), `200_match_patterns` (own), `206_abc_collections_abc` (own), `305_os_path_pathlib_fnmatch` (own), `308_time_struct` (own), `313_contextlib` (own), `314_traceback` (own), `316_urllib_parse` (own), `317_pprint` (own), `318_shlex` (own), `319_difflib` (own), `322_async_iter_with` (own), `323_contextlib_async` (own), `65_async_gather` (own), `66_async_errors` (own), `67_imports` (own), `args__macro_errors`, `async__asyncio_run`, `async__basic`, `async__closure`, `async__double_await_coroutine`, `async__exception`, `async__gather_all`, `async__gather_commit_rollback`, `async__gather_double_spawn`, `async__gather_double_spawn_traceback`, `async__nested_await`, `async__not_awaitable`, `async__not_imported`, `async__return_types`, `async__sequential`, `async__traceback`, `async__transitive_closure`, `async__with_args`, `bisect__behavior`, `bisect__errors`, `bool__ops`, `class__eq_hash`, `class__index_dunder`, `class__protocols`, `collections__counter`, `collections__defaultdict`, `collections__deque`, `collections__namedtuple`, `copy__all`, `dataclass__field`, `dataclass__generated_methods`, `dataclass__init`, `dataclass__is_dataclass`, `dataclass__options`, `dataclass__repr_eq`, `decimal__context`, `decimal__methods`, `decorator__function_async`, `fnmatch__module`, `fnmatch__posix`, `functools__gc`, `functools__lru_cache`, `functools__lru_cache_async`, `functools__partial`, `functools__reduce`, `generic_alias`, `import__local_scope`, `import__type_checking_guard`, `import__typing`, `import__typing_type_ignore`, `iter__user_class`, `itertools__adaptors`, `itertools__combinatoric`, `itertools__count_repeat`, `itertools__gc`, `itertools__groupby`, `itertools__reentrancy`, `itertools__tee`, `lookup__mutation`, `match__all`, `match__async`, `match__class_error_traceback`, `module__dunders`, `object__setattr`, `os__functions`, `pathlib__import`, `pathlib__pure`, `recursion__deep_copy`, `recursion__deep_hash`, `repr__mutation`, `sequence__getitem_type_error`, `sequence__repeat`, `sequence__search_bounds`, `star__unpack_iterables`, `str__percent_format`, `time__struct_time`, `typing__get_type_hints`, `typing__get_type_hints_classes`, `typing__get_type_hints_dataclasses`, `typing__get_type_hints_functions`, `typing__types`, `typing__union`, `unary__ops`
- **C module math absent (CONFIG_PYTHON_CAPI off)** (55): `107_math` (own), `207_wildcard_import` (own), `302_datetime_format` (own), `303_random_seeds` (own), `307_fractions_statistics_uuid` (own), `312_wildcard_imports` (own), `attr__class_object_errors`, `bool__arithmetic`, `builtin__exec_namespaces`, `builtin__getattr`, `builtin__hasattr`, `class__numeric`, `datetime__format`, `datetime__zone_default`, `datetime__zone_kathmandu`, `datetime__zone_london`, `datetime__zone_new_york`, `datetime__zone_sydney`, `decimal__arithmetic`, `decimal__format`, `decimal__math`, `math__acos_domain_error`, `math__acosh_domain_error`, `math__aggregates`, `math__asin_domain_error`, `math__atanh_domain_error`, `math__cos_inf_error`, `math__cosh_overflow_error`, `math__exp_overflow_error`, `math__factorial_float_error`, `math__factorial_negative_error`, `math__floor_inf_error`, `math__floor_nan_error`, `math__floor_str_error`, `math__fmod_inf_error`, `math__gamma_neg_int_error`, `math__gcd_float_error`, `math__isqrt_negative_error`, `math__ldexp_overflow_error`, `math__log1p_domain_error`, `math__log_base1_error`, `math__log_zero_error`, `math__module`, `math__pow_domain_error`, `math__sin_inf_error`, `math__sqrt_negative_error`, `math__tan_inf_error`, `math__trunc_str_error`, `random__errors`, `random__seeded`, `str__argument_errors`, `str__pyformat`, `time__core`, `tomllib__errors`, `tomllib__loads`
- **C module _sre absent (CONFIG_PYTHON_CAPI off)** (21): `106_json` (own), `118_operator_abc_string_textwrap` (own), `300_re_differential` (own), `301_re_edges` (own), `315_logging` (own), `321_html` (own), `class__type_errors`, `json__dumps_errors`, `json__dumps_formatting`, `json__dumps_keys`, `json__dumps_recursion`, `json__int_max_str_digits`, `json__literals`, `json__loads_invalid_messages`, `json__module`, `re__basic`, `re__grouping`, `re__match`, `str__format`, `str__module_arguments`, `json_rows` (bench)
- **assertion failed (semantic difference)** (10): `builtin__eval_exec`, `bytes__methods`, `class__dynamic_type`, `codecs__all`, `complex__all`, `function__call_unpack`, `import__dotted_error`, `import__sys`, `str__ops`, `version__cpython`
- **different stdout** (8): `108_sys_sets_misc` (own), `203_percent_user_objects` (own), `51_call_binding_errors` (own), `63_genexpr_and_star_calls` (own), `81_container_protocol` (own), `82_repr_str_format` (own), `94_truthiness` (own), `99_iteration_corners` (own)
- **wrong-exception: TypeError** (6): `57_tb_slice_type` (own), `builtin__iter_err_unpack_int`, `bytes__percent_format`, `function__err_unpack_int`, `function__err_unpack_not_mapping`, `slice__invalid_indices`
- **C module _csv absent (CONFIG_PYTHON_CAPI off)** (5): `306_csv_io` (own), `csv__dict`, `csv__module`, `csv__reader`, `csv__writer`
- **C module binascii absent (CONFIG_PYTHON_CAPI off)** (5): `304_hashlib_base64_binascii` (own), `base64__all`, `binascii__all`, `import__multi`, `int__bigint`
- **C module unicodedata absent (CONFIG_PYTHON_CAPI off)** (5): `136_decimal_unicodedata` (own), `str__methods`, `unicodedata__bad_arg`, `unicodedata__core`, `unicodedata__name_error`
- **hashlib algorithms absent** (3): `hashlib__all`, `hashlib__errors`, `inline__string_operations`
- **C module _struct absent (CONFIG_PYTHON_CAPI off)** (1): `320_struct` (own)
- **C module cmath absent (CONFIG_PYTHON_CAPI off)** (1): `202_complex_cmath` (own)
- **no exception raised** (1): `fstring__all`
- **unexpected-exception: TypeError** (1): `bisect__index`
- **unexpected-exception: ValueError** (1): `208_format_edges` (own)
- **wrong-exception: SyntaxError** (1): `function__err_duplicate_param`

