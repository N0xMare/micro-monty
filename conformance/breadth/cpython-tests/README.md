# CPython's own test suite on micro-monty

This directory measures how much of CPython 3.14.7's `Lib/test` passes on
micro-monty, for the modules that cover the subset's language core and its
bundled stdlib (`docs/spec.md` §1, §4, §12; `docs/sdk.md` "Limitations"). The
unit is the test method. A method counts as passing on mm only if it passes
on CPython with the same shim and the same transformed source. Like the
dquickts comparison, this is a count of per-test methods.

## Result [M]

Run of 2026-11-09 on the correctness-triage fixes (typing aliases in
`isinstance`, `collections.abc`/`json.decoder` after a package import,
float formats without a type, bound cached methods and method hashing),
same inputs and units. The 2026-11-07 run (core-module pass and capability
gaps, `docs/research/cpython-tests-core.md`) had 6,274:

- **10,328 test methods** in 125 units (modules, or package members such
  as `test_json.test_decode`).
- **8,989 pass on CPython 3.14.7** with the shim. The rest: 1,140 are
  skipped there (C-accelerator variants, `cpython_only`, resources, threads,
  subprocess), and the others fail there because of a shim gap
  (`unittest.mock`, `os_helper.EnvironmentVarGuard` and similar). Those
  methods are left out of the denominator.
- **6,344 of those 8,989 pass on micro-monty (70.6%)**, from 6,274; 67
  of the 70 come from the fixes (test_typing +48, test_exception_group +9,
  test_patma +4, test_float +2, test_functools +2, test_class +1,
  test_isinstance +1), measured against the unfixed engine on the same
  orb, and 3 methods that timed out before passed. Earlier: 5,964 (66.3%)
  on 266bde7, 6,261 with the core-module pass alone, 13 more from the
  capability gaps (traceback, `ast`, `tokenize`: test_fstring,
  test_exceptions); 4,779 (53.2%) on 2026-11-04.
- 678 of the 8,989 fail on mm for reasons the spec excludes on purpose:
  `exec`/`eval`/`compile`/`globals`, `__slots__`/`__del__`, ambient
  filesystem, `weakref`/threads/`inspect`, `difflib.HtmlDiff`. Leaving
  those out gives 6,344 / 8,311 (76.3%; 75.5% on 2026-11-07).
- Process-level failures are counted per method: 3 timeouts, no crash.
  All 125 units load on mm once the parts mm rejects at compile time are
  isolated (see "mm compile isolation"). The rejections are mostly
  `__slots__`, PEP 695 type parameters, `__del__` and `\N{...}` escapes.

`results/summary.md` has the full cause table. `results/methods.jsonl` has one
line per method, and `results/units.jsonl` has the per-unit details: compile
stubs, chunking, top-level errors and process attempts.

## Versions and inputs

| | |
|---|---|
| Engine | `target/release/mm` built from the correctness-triage fixes (2026-11-09; the commit that records these results), SHA-256 `8a3abaa76f5f0cd0b3a417155f0bcf2b365edffbbed77480d0693128be580756`. The 2026-11-07 run used the core-module pass build `4e7e61b4fdb2080d47b2755c16f29403491e1f3b63f425d42eba83d51237ccc1`. The 2026-11-04 run used the frozen `main` build `ec95bd9e...8b8b` |
| Oracle | `~/.local/bin/python3.14` = CPython 3.14.7 (uv `cpython-3.14.7-linux-x86_64-gnu`) |
| Tests | `Lib/test` from `https://www.python.org/ftp/python/3.14.7/Python-3.14.7.tgz`, SHA-256 `62859805f6fdf25e2bcbf3fa3217801e1996887ca33e6a2af80674bdfa2dbe07`, sigstore bundle `Python-3.14.7.tgz.sigstore` verified (`sigstore 4.5.1 verify identity --cert-identity hugo@python.org --cert-oidc-issuer https://github.com/login/oauth`: OK). The uv install ships no `Lib/test` |

## Commands

```bash
# 1. CPython 3.14.7 Lib/test, in its own empty directory
mkdir -p /tmp/cpython-src-3147 && cd /tmp/cpython-src-3147
curl -sSfLO https://www.python.org/ftp/python/3.14.7/Python-3.14.7.tgz
curl -sSfLO https://www.python.org/ftp/python/3.14.7/Python-3.14.7.tgz.sigstore
sha256sum Python-3.14.7.tgz   # 62859805...2dbe07
uvx sigstore verify identity --bundle Python-3.14.7.tgz.sigstore \
  --cert-identity hugo@python.org --cert-oidc-issuer https://github.com/login/oauth Python-3.14.7.tgz
mkdir x && tar -xzf Python-3.14.7.tgz -C x Python-3.14.7/Lib/test

# 2. run (about 4 minutes wall with 4 jobs; at most 4 engine processes at a time)
cd conformance/breadth/cpython-tests
LIBTEST=/tmp/cpython-src-3147/x/Python-3.14.7/Lib/test
~/.local/bin/python3.14 -I harness.py --lib-test $LIBTEST \
  --mm /tmp/mm-evidence-layers/main/mm --jobs 4 --out /tmp/mm-cpython-tests/full
#    --only test_bool test_list    runs a subset

# 3. summarize into results/
~/.local/bin/python3.14 -I report.py /tmp/mm-cpython-tests/full --lib-test $LIBTEST
```

`harness.py` writes generated sources and per-process output to
`/tmp/mm-cpython-tests/work/<unit>/` (`$MM_CPYTHON_TESTS_WORK` overrides it).
Rerun a single file by hand with `mm run work/<unit>/generated-mm-0.py` or
`python3.14 -I work/<unit>/generated.py`. Result lines go to stderr.

## How it works

Files:

- `modules.txt`: the units, grouped as language core, object model, builtin
  types and bundled stdlib. Every bundled stdlib module that has a CPython
  test module is in it. Left out, and why: doctest-only modules
  (`test_genexps`, `test_extcall`, `test_metaclass`, `test_unpack_ex`; the
  shim runs no doctests, which is also why `test_unpack` and `test_setcomps`
  count only one method each); `test_bigmem`; `test_logging` and
  `test_traceback` (nearly all subprocess, threads and frames); `test_asyncio`
  (event loops beyond `run`/`gather` are documented as unsupported).
- `shim/mmunittest.py`: a unittest and `test.support` replacement written
  in the subset. It provides `TestCase` with the assert methods, including
  `assertRaises`/`assertRaisesRegex`/`assertWarns`/`assertLogs` as context
  managers or calls, plus `subTest`, `skip*`, `expectedFailure`,
  `setUp`/`tearDown`, class and module setup and cleanups,
  `enterContext`, and `IsolatedAsyncioTestCase` through `asyncio.run`. It
  also provides the parts of `test.support` these modules use
  (`ALWAYS_EQ`, `swap_attr`, `subTests`, `captured_stdout`,
  `import_helper`, `warnings_helper`, `os_helper.FakePath`/`TESTFN`, and
  so on). The runner prints `@@MMSTART` and then one `@@MMT` line per
  method: id, status (`pass fail error skip xfail xpass`), exception type,
  traceback line numbers and message.
- `harness.py`: builds, runs and records. `report.py`: categorizes and
  writes `results/`.

Uniform source transforms (`Builder.transform`). Every unit gets exactly
these and nothing else; no individual test is edited:

1. The shim is prepended. `import unittest` / `from unittest import X`
   bind the shim's namespace. `from test import support`,
   `test.support.*` and `test.support.<helper>` bind shim namespaces. Any
   support name the shim lacks is a placeholder that raises
   `_MMShimMissing` when used.
2. Other `Lib/test` helper modules (`seq_tests`, `list_tests`,
   `string_tests`, `mapping_tests`, `test_math` for `test_cmath`,
   `test_contextlib`, `re_tests`, `test_json/__init__`,
   `support/testcase.py`, `support/numbers.py`, ...) are inlined as a
   function that returns a namespace. The subset has no packages or
   `sys.path`.
3. Every other top-level statement is wrapped in `try`/`except
   BaseException` and reports a failure as `@@MMTOP` (file:line, type,
   message), so one failing statement does not stop the module. Top-level
   imports that fail bind a placeholder that raises
   `_MMMissingModuleError("import failed: No module named 'x'")` when used.
   The consequence lands on the methods that use it, not on the whole
   module.
4. In `if __name__ == '__main__':` the name is replaced by a constant that
   is never equal, so only the `else` branch runs. `from __future__` is
   hoisted to line 1. Comments are stripped and module blocks are indented
   by one space instead of four, because mm refuses sources over 256 KiB.
   Lines inside multi-line strings are never re-indented. A wildcard import
   inside an inlined helper is expanded to the names CPython exports.
5. Names passed as literals to `import_helper.import_module` /
   `import_fresh_module` are imported up front, because the subset has no
   `__import__`. `import_fresh_module(..., fresh=['_c_accel'])` returns
   `None`, so C-accelerator test classes skip or fail on both engines and
   drop out of the denominator.
6. Non-`.py` entries of the unit's source directory (data files such as
   `mathdata/`, `test_tomllib/data`) are symlinked next to the generated
   file, so `__file__`-relative data reads work on CPython.

Discovery runs on CPython: `TestCase` subclasses bound to module globals
(including classes built by factory functions, as in `test_abc`), and their
`test*` methods as `dir()` lists them, the way unittest's loader works. The
same plan then runs on both engines.

Isolation and process failures. Each method gets a fresh `TestCase`
instance, its own `setUp`/`tearDown`/cleanups and its own exception
boundary, and prints its own result. Methods of a unit share one process,
as under unittest. If a process dies (crash, abort, timeout after 120 s),
the method that was running is recorded as `crash`/`timeout`, and a new
process continues with the next method. Each unit has a budget of 600 s
per engine, and methods left when it runs out count as `timeout`. A unit
whose process dies before any method runs records every method as `load`.
At most 4 units run at once, so at most 4 engine processes are alive. Each
process is capped at 4 GiB of address space.

mm compile isolation. mm compiles the whole file before running it, so one
unsupported construct would reject the whole module. The harness compiles
the generated file on mm (with an exit after the shim's first line), reads
the error line, and replaces the smallest enclosing unit with a stub that
raises `_MMExcluded("mm compile error: ...")`. The smallest unit is the
outermost method (a `def` directly in a class), else the outermost `def`,
else the enclosing class, else the top-level statement. It repeats until
the file compiles. The tests that hit the stub fail on mm with that reason;
`units.jsonl` lists every stub. CPython always runs the unstubbed file.
Files over mm's 256 KiB source limit (`test_str`, `test_bytes`,
`test_enum`, `test_decimal`, `datetimetester`, `test_typing`) are split
into chunks. Each chunk keeps everything except the bodies of the other
chunks' test classes, which nothing else references.

Categories (`report.py`, `cause()`): the first exception mm reported for
the method. A `NameError` for a top-level name whose defining statement
failed at import (a test class whose body raised) is attributed to that
statement's error. "Deliberate" means excluded by `docs/spec.md` §1/§4 or
`docs/sdk.md` "Limitations".

## Per-module table [M]

| unit | methods | pass CPython | pass mm | mm % | mm failure categories (methods) |
|---|---:|---:|---:|---:|---|
| test_grammar | 75 | 72 | 34 | 47 | missing builtin (deliberate) 23, missing attribute 7, missing module 5, wrong result 1 |
| test_opcodes | 8 | 6 | 4 | 67 | missing builtin (deliberate) 2 |
| test_scope | 41 | 38 | 24 | 63 | missing builtin (deliberate) 7, missing builtin 3, missing attribute 2, excluded: compile (deliberate) 2 |
| test_global | 20 | 20 | 0 | 0 | missing module 20 |
| test_named_expressions | 74 | 74 | 39 | 53 | missing builtin (deliberate) 31, wrong result 2, missing builtin 2 |
| test_patma | 328 | 328 | 242 | 74 | missing builtin (deliberate) 44, missing builtin 22, missing attribute 13, wrong result 4 |
| test_with | 54 | 53 | 46 | 87 | missing builtin (deliberate) 3, missing attribute 3, wrong result 1 |
| test_raise | 37 | 36 | 30 | 83 | missing attribute 3, wrong result 2, wrong exception/error 1 |
| test_exceptions | 108 | 90 | 32 | 36 | missing attribute 25, missing builtin 8, missing module (deliberate) 7, wrong result 6 |
| test_baseexception | 11 | 11 | 9 | 82 | no ambient I/O (deliberate) 1, missing attribute 1 |
| test_exception_variations | 30 | 30 | 16 | 53 | excluded: compile (unsupported syntax) 14 |
| test_exception_group | 52 | 52 | 1 | 2 | missing builtin 50, missing attribute 1 |
| test_except_star | 60 | 60 | 0 | 0 | excluded: compile (unsupported syntax) 37, missing builtin 19, missing builtin (deliberate) 4 |
| test_augassign | 7 | 7 | 6 | 86 | missing builtin (deliberate) 1 |
| test_unpack | 1 | 1 | 0 | 0 | missing builtin (deliberate) 1 |
| test_keywordonlyarg | 11 | 11 | 7 | 64 | missing builtin (deliberate) 3, missing attribute 1 |
| test_positional_only_arg | 28 | 28 | 17 | 61 | missing builtin (deliberate) 5, missing attribute 4, missing module 1, wrong result 1 |
| test_call | 186 | 4 | 1 | 25 | excluded: compile (deliberate) 1, wrong exception/error 1, missing builtin (deliberate) 1 |
| test_decorators | 16 | 16 | 11 | 69 | missing builtin (deliberate) 3, missing attribute 2 |
| test_listcomps | 65 | 64 | 0 | 0 | missing builtin (deliberate) 64 |
| test_setcomps | 1 | 0 | 0 | - |  |
| test_generators | 50 | 49 | 25 | 51 | missing attribute 12, missing module (deliberate) 3, wrong exception/error 2, missing builtin (deliberate) 2 |
| test_coroutines | 99 | 92 | 1 | 1 | missing attribute 78, missing builtin (deliberate) 5, missing builtin 4, wrong result 2 |
| test_asyncgen | 85 | 85 | 0 | 0 | missing attribute 73, missing builtin (deliberate) 5, wrong exception/error 4, missing builtin 3 |
| test_string_literals | 20 | 20 | 0 | 0 | missing attribute 20 |
| test_fstring | 91 | 83 | 36 | 43 | missing builtin (deliberate) 27, missing module 14, excluded: compile (unsupported syntax) 3, missing builtin 2 |
| test_unary | 6 | 6 | 4 | 67 | missing builtin (deliberate) 2 |
| test_compare | 16 | 16 | 13 | 81 | wrong result 2, missing builtin 1 |
| test_richcmp | 7 | 7 | 7 | 100 |  |
| test_contains | 4 | 4 | 4 | 100 |  |
| test_isinstance | 23 | 23 | 4 | 17 | wrong exception/error 17, wrong result 1, process: crash 1 |
| test_iter | 57 | 53 | 24 | 45 | missing module 14, no ambient I/O (deliberate) 12, wrong result 1, missing attribute 1 |
| test_index | 55 | 55 | 34 | 62 | missing attribute 10, missing builtin 8, wrong result 2, wrong exception/error 1 |
| test_class | 23 | 22 | 11 | 50 | wrong result 5, wrong exception/error 3, excluded: compile (deliberate) 2, excluded: compile (unsupported syntax) 1 |
| test_descr | 163 | 147 | 52 | 35 | missing attribute 25, wrong result 24, excluded: compile (deliberate) 18, wrong exception/error 13 |
| test_property | 31 | 29 | 4 | 14 | wrong result 9, wrong exception/error 7, missing attribute 5, excluded: compile (deliberate) 4 |
| test_super | 40 | 36 | 16 | 44 | wrong result 7, wrong exception/error 7, skipped on mm 3, missing attribute 2 |
| test_subclassinit | 17 | 17 | 14 | 82 | missing attribute 3 |
| test_funcattrs | 35 | 34 | 13 | 38 | missing attribute 16, wrong result 2, missing builtin 2, excluded: compile (unsupported syntax) 1 |
| test_types | 128 | 123 | 58 | 47 | wrong exception/error 28, missing attribute 20, wrong result 8, missing module 6 |
| test_hash | 30 | 14 | 10 | 71 | wrong result 2, missing builtin 2 |
| test_bool | 31 | 31 | 23 | 74 | missing module 3, wrong exception/error 1, no ambient I/O (deliberate) 1, wrong result 1 |
| test_int | 40 | 32 | 12 | 38 | missing attribute 14, missing builtin 3, excluded: compile (unsupported syntax) 1, wrong result 1 |
| test_long | 47 | 38 | 27 | 71 | missing attribute 6, wrong exception/error 3, missing builtin (deliberate) 2 |
| test_int_literal | 6 | 6 | 6 | 100 |  |
| test_float | 54 | 51 | 27 | 53 | missing attribute 11, wrong result 4, missing builtin 3, no ambient I/O (deliberate) 2 |
| test_complex | 37 | 37 | 23 | 62 | wrong exception/error 6, wrong result 3, missing attribute 2, missing builtin (deliberate) 2 |
| test_pow | 7 | 7 | 7 | 100 |  |
| test_list | 68 | 64 | 55 | 86 | wrong result 4, missing module 3, missing attribute 1, wrong exception/error 1 |
| test_tuple | 38 | 33 | 27 | 82 | missing module 3, wrong result 2, wrong exception/error 1 |
| test_dict | 121 | 110 | 79 | 72 | excluded: compile (deliberate) 7, wrong exception/error 7, missing module 6, wrong result 5 |
| test_dictviews | 16 | 16 | 13 | 81 | missing attribute 1, wrong result 1, missing module 1 |
| test_set | 630 | 626 | 481 | 77 | wrong exception/error 84, missing builtin 22, missing module 12, missing attribute 8 |
| test_str | 139 | 125 | 0 | 0 | missing module 124, wrong exception/error 1 |
| test_bytes | 206 | 194 | 94 | 48 | missing builtin 79, wrong exception/error 8, missing attribute 5, missing module 4 |
| test_format | 18 | 16 | 10 | 62 | missing builtin 3, missing builtin (deliberate) 1, missing module 1, wrong result 1 |
| test_slice | 11 | 11 | 5 | 45 | wrong result 2, missing module (deliberate) 1, wrong exception/error 1, missing attribute 1 |
| test_range | 29 | 29 | 9 | 31 | wrong exception/error 10, missing module 6, missing attribute 4 |
| test_sort | 21 | 21 | 17 | 81 | excluded: compile (deliberate) 2, wrong result 1, wrong exception/error 1 |
| test_enumerate | 105 | 78 | 55 | 71 | wrong exception/error 12, missing module 6, missing attribute 3, skipped on mm 1 |
| test_builtin | 133 | 104 | 6 | 6 | missing module 90, wrong result 6, wrong exception/error 1, excluded: compile (unsupported syntax) 1 |
| test_print | 9 | 9 | 3 | 33 | missing builtin (deliberate) 6 |
| test_userdict | 28 | 28 | 20 | 71 | missing attribute 5, wrong exception/error 2, wrong result 1 |
| test_userlist | 54 | 53 | 41 | 77 | wrong exception/error 7, wrong result 3, missing attribute 1, missing module 1 |
| test_userstring | 71 | 69 | 52 | 75 | wrong exception/error 13, missing attribute 3, wrong result 1 |
| test_math | 88 | 81 | 60 | 74 | wrong exception/error 10, missing attribute 8, no ambient I/O (deliberate) 2, wrong result 1 |
| test_cmath | 33 | 32 | 13 | 41 | no ambient I/O (deliberate) 17, wrong exception/error 2 |
| test_json.test_decode | 38 | 18 | 8 | 44 | wrong exception/error 5, missing attribute 4, missing builtin (deliberate) 1 |
| test_json.test_default | 6 | 3 | 1 | 33 | missing attribute 1, wrong exception/error 1 |
| test_json.test_dump | 17 | 8 | 6 | 75 | wrong result 1, wrong exception/error 1 |
| test_json.test_encode_basestring_ascii | 7 | 3 | 1 | 33 | missing attribute 1, wrong exception/error 1 |
| test_json.test_enum | 12 | 6 | 0 | 0 | wrong exception/error 6 |
| test_json.test_fail | 14 | 7 | 5 | 71 | missing attribute 1, wrong result 1 |
| test_json.test_float | 8 | 4 | 4 | 100 |  |
| test_json.test_indent | 4 | 2 | 2 | 100 |  |
| test_json.test_pass1 | 2 | 1 | 1 | 100 |  |
| test_json.test_pass2 | 2 | 1 | 1 | 100 |  |
| test_json.test_pass3 | 2 | 1 | 1 | 100 |  |
| test_json.test_recursion | 12 | 5 | 1 | 20 | missing attribute 4 |
| test_json.test_scanstring | 8 | 4 | 0 | 0 | missing attribute 4 |
| test_json.test_separators | 4 | 2 | 2 | 100 |  |
| test_json.test_unicode | 34 | 17 | 7 | 41 | excluded: compile (unsupported syntax) 5, wrong exception/error 2, missing module 1, missing builtin (deliberate) 1 |
| test_re | 165 | 149 | 11 | 7 | missing builtin 133, missing attribute 3, missing builtin (deliberate) 1, wrong result 1 |
| test_collections | 101 | 97 | 56 | 58 | wrong exception/error 17, missing builtin 10, missing attribute 7, wrong result 4 |
| test_ordered_dict | 178 | 140 | 92 | 66 | wrong exception/error 22, missing attribute 10, wrong result 8, excluded: compile (deliberate) 4 |
| test_defaultdict | 13 | 13 | 9 | 69 | wrong result 2, missing module 1, missing builtin (deliberate) 1 |
| test_deque | 80 | 77 | 64 | 83 | missing module 6, wrong result 3, missing module (deliberate) 2, missing builtin (deliberate) 1 |
| test_functools | 325 | 198 | 100 | 51 | missing attribute 31, wrong exception/error 28, wrong result 22, missing module (deliberate) 7 |
| test_itertools | 136 | 111 | 99 | 89 | wrong exception/error 7, missing module (deliberate) 3, wrong result 1, missing builtin (deliberate) 1 |
| test_operator | 110 | 52 | 43 | 83 | missing module (deliberate) 3, missing module 3, wrong exception/error 1, missing builtin 1 |
| test_copy | 81 | 80 | 63 | 79 | missing module (deliberate) 6, missing attribute 2, wrong result 2, wrong exception/error 2 |
| test_heapq | 68 | 33 | 33 | 100 |  |
| test_bisect | 46 | 23 | 23 | 100 |  |
| test_string.test_string | 42 | 41 | 18 | 44 | missing attribute 14, wrong result 7, wrong exception/error 2 |
| test_textwrap | 68 | 68 | 66 | 97 | excluded: compile (unsupported syntax) 2 |
| test_enum | 1077 | 1068 | 633 | 59 | wrong exception/error 177, wrong result 113, missing builtin 105, missing attribute 16 |
| test_dataclasses | 280 | 266 | 197 | 74 | missing attribute 22, excluded: compile (deliberate) 13, wrong result 11, missing module 10 |
| test_abc | 72 | 72 | 26 | 36 | missing module 36, missing attribute 6, wrong result 2, excluded: compile (deliberate) 1 |
| test_abstract_numbers | 7 | 7 | 3 | 43 | missing builtin 4 |
| test_contextlib | 91 | 88 | 73 | 83 | missing module (deliberate) 7, wrong result 4, missing builtin (deliberate) 1, excluded: compile (deliberate) 1 |
| test_contextlib_async | 58 | 57 | 8 | 14 | missing attribute 43, missing module/attribute 3, wrong result 2, excluded: compile (deliberate) 1 |
| test_context | 56 | 53 | 0 | 0 | missing attribute 28, skipped on mm 22, wrong result 3 |
| test_base64 | 41 | 35 | 19 | 54 | missing builtin 16 |
| test_binascii | 93 | 76 | 16 | 21 | missing builtin 40, missing module 19, wrong result 1 |
| test_struct | 44 | 36 | 23 | 64 | missing builtin 4, missing module 3, missing attribute (deliberate) 2, wrong exception/error 1 |
| test_hashlib | 82 | 63 | 1 | 2 | missing module 43, skipped on mm 18, missing module (deliberate) 1 |
| test_random | 9 | 6 | 3 | 50 | missing attribute 2, missing builtin 1 |
| test_statistics | 371 | 345 | 294 | 85 | wrong exception/error 30, missing attribute 18, wrong result 3 |
| test_fractions | 50 | 50 | 35 | 70 | wrong exception/error 11, missing module 1, no ambient I/O (deliberate) 1, missing attribute 1 |
| test_decimal | 368 | 159 | 120 | 75 | missing attribute 32, missing builtin (deliberate) 4, missing module 2, wrong result 1 |
| datetimetester | 553 | 502 | 176 | 35 | missing builtin 291, missing module 14, wrong result 8, missing builtin (deliberate) 5 |
| test_csv | 134 | 127 | 46 | 36 | missing module (deliberate) 59, missing attribute 16, missing module 5, wrong result 1 |
| test_urlparse | 77 | 77 | 62 | 81 | missing builtin 14, wrong exception/error 1 |
| test_posixpath | 94 | 35 | 5 | 14 | no ambient I/O (deliberate) 23, missing builtin 4, skipped on mm 2, missing module 1 |
| test_fnmatch | 24 | 24 | 23 | 96 | missing module 1 |
| test_shlex | 45 | 32 | 29 | 91 | missing module (deliberate) 3 |
| test_difflib | 39 | 38 | 0 | 0 | missing attribute (deliberate) 38 |
| test_html | 2 | 2 | 2 | 100 |  |
| test_pprint | 45 | 44 | 34 | 77 | missing builtin 3, missing builtin (deliberate) 3, wrong exception/error 3, wrong result 1 |
| test_typing | 699 | 677 | 192 | 28 | wrong result 214, wrong exception/error 117, missing attribute 63, missing builtin (deliberate) 25 |
| test_tomllib.test_data | 2 | 2 | 0 | 0 | no ambient I/O (deliberate) 2 |
| test_tomllib.test_error | 8 | 8 | 7 | 88 | missing builtin 1 |
| test_tomllib.test_misc | 8 | 6 | 2 | 33 | missing module (deliberate) 2, missing attribute 1, missing module 1 |
| test_uuid | 24 | 2 | 0 | 0 | missing attribute 2 |
| test_unicodedata | 59 | 51 | 19 | 37 | missing attribute 26, wrong exception/error 6 |

## Top failure causes, grouped by root cause [M]

Methods that pass on CPython and fail on mm, grouped by the first
exception mm reported. Each repro is the smallest program that shows the
difference; the output shown is CPython 3.14.7 → mm.

| # | methods | root cause | kind | repro (CPython → mm) |
|---:|---:|---|---|---|
| 1 | 485 | warning categories are not builtins (`DeprecationWarning`, `Warning`, ...). Mostly through class bodies that name one in a decorator, e.g. all of `test_re.ReTests` and 289 `datetimetester` methods | missing feature; cheap, likely an oversight | `print(issubclass(DeprecationWarning, Warning))` → `True` / `NameError: name 'DeprecationWarning' is not defined` |
| 2 | 323 | `exec`, `eval`, `compile`, `globals` not defined (`test_listcomps` runs every case through `exec`) | deliberate exclusion | `print(eval("1"))` → `1` / `NameError ... micro-monty does not support eval()` |
| 3 | 140 | no `bytearray` (`test_bytes` ByteArray*, `test_binascii`, `test_base64`) | missing feature (documented: "no `bytearray`") | `print(bytearray(b"ab"))` → `bytearray(b'ab')` / `NameError` |
| 4 | 135 | no ambient filesystem: `open()`, `tempfile`, `Path.scan` (`test_csv` file tests, `test_cmath`/`test_math` data files, `test_posixpath`) | deliberate exclusion | `open(__file__)` → file / `PermissionError: open() is not available: this sandbox has no ambient filesystem` |
| 5 | 132 | no `dir()` builtin (`test_enum` 105) | missing feature | `print("append" in dir([]))` → `True` / `NameError: name 'dir' is not defined` |
| 6 | 123 | no `codecs` module. `test_str`'s `setUp` calls `codecs.register`, so all of `StrTest` fails before any str assertion runs | missing module (fixture dependency) | `import codecs` → ok / `ModuleNotFoundError` |
| 7 | 116 | no `pickle` (pickling/copyreg round-trip tests across types) | not bundled; not in the spec's lists | `import pickle` → ok / `ModuleNotFoundError` |
| 8 | 108 | coroutine objects lack `send`/`close`/`throw` (`test_coroutines` 53+, `test_contextlib_async` 43, `async with`/`async for` tests that drive coroutines by hand) | missing feature (generators have them) | `async def f(): return 1` then `f().send(None)` → `StopIteration(1)` / `AttributeError: 'coroutine' object has no attribute 'send'` |
| 9 | 105 | `Enum('E', type=int)` (functional API without names) is treated as a value lookup. All of `TestMixed{Int,Str}Function` and `TestMixedIntFlagFunction` | **likely real bug** | `from enum import Enum; E = Enum("E", type=int); print(E, len(E))` → `<enum 'E'> 0` / `ValueError: 'E' is not a valid Enum` |
| 10 | 90 | no `platform` module; `test_builtin`'s `BuiltinTest` body uses it, so 90 builtin tests never run | missing module (fixture dependency) | `import platform` → ok / `ModuleNotFoundError` |

Generic buckets that are not one cause: 214 assertion failures in
`test_typing`, 70 of them "TypeError not raised" (typing is a runtime no-op
by design, so generic-alias checks do not raise), and 113 in `test_enum`.
Smaller root causes (methods): `ExceptionGroup` missing (61; `except*` is
planned parity), `asyncio.new_event_loop` (55; documented), compile-time
`except*` (52) and `__slots__` (51), `sys.get_int_max_str_digits` (48),
`importlib` (44), `weakref` (40), class bases `typing.List[int]` → `TypeError:
bases must be types, not _GenericAlias` (40), `difflib.HtmlDiff` (38;
documented), subclassing `frozenset` (37), `memoryview` (33),
`types.coroutine` (29), `locals()` (27), `contextvars.Context` (27).

Process failures and parser findings, each a likely real bug:

- **Rust stack overflow** (abort, rc −6) in
  `test_isinstance.test_isinstance_recursion_limit`. The rules require
  `RecursionError` instead:
  `t = (int,)` then `for i in range(200000): t = (t,)`, then
  `isinstance(1, t)` → CPython `RecursionError` / mm `thread 'main' has
  overflowed its stack`.
- **Hang** in `test_descr.test_repr_as_str` (timeout):
  `class Foo: pass; Foo.__repr__ = Foo.__str__; str(Foo())` → CPython
  `RecursionError` / mm prints nothing for over 20 s.
- **Quadratic `bytes.find`** (`test_bytes` `test_adaptive_find` timeout):
  `A, B = b"a"*N, b"b"*N; (A+A+B+A+A).find(A+B+B+A)` takes 0.29 s at
  N=10⁵ on mm against 0.003 s on CPython. `str.find` on mm is linear
  (0.001 s).
- **Parser**: `c[1:2, 3]` (a tuple of slices in a subscript) →
  `SyntaxError: invalid slice` on mm; CPython gives
  `(slice(1, 2, None), 3)`.
- `\N{...}` escapes are rejected at compile time (15 stubs; undocumented):
  `"\N{BULLET}"`.

## Caveats

- One run, not repeated. CPython's counts do not depend on timing. The
  three process failures reproduce standalone with the repros above.
- Methods of one unit share a process, as under unittest, so state leaks
  between methods (for example a global changed by one test) behave as
  they would under unittest.
- Fixture dependencies count against mm. Examples: `codecs` in
  `test_str.setUp`, `platform` in `test_builtin`, `sys.path` in
  `test_string_literals.setUp`, `warnings` in `test_global.setUpModule`.
  The functionality those tests check may work, but the test cannot run
  without the fixture. The categories name the module, so such cases can be
  separated.
- The shim is not unittest: failure-message formats differ, and
  `assertWarns` needs a `warnings` module (absent on mm). CPython runs the
  same shim, so a shim limitation can only remove a method from the
  denominator, never count a false mm pass. The one exception is
  `warnings_helper.ignore_warnings`, which is a no-op on mm because mm
  emits no warnings.
