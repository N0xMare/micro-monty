# Conformance results: micro-monty

[M] `cargo xtask conformance --engine micro-monty` over the 726 cases of `conformance/monty/` (Monty `66fff6bf`). Each case runs through `mm exec --json` (release build) under `conformance/oracle.py` on CPython 3.14.7, which checks the case's expectation the way Monty's datatest checks its Monty side (see `conformance/README.md`). Per-case results are in `micro-monty.jsonl`; `cargo xtask check` reruns the corpus with `--check` and fails on any change: a newly failing case is a regression, and a newly passing one has to be recorded by regenerating both files with `cargo xtask conformance --engine micro-monty`.

Tags are those of `summary.md`. `call-external` cases run with `mm exec --host-fixtures` (mm's emulation of the datatest's host functions). Skipped: `mount-fs` cases (they need the host filesystem), `excluded` cases (`refcount__*`) and ref-counts expectations. `crash` is a panic, signal or missing report from `mm`; `timeout` is a case that used more than 10 s of mm's CPU time (or ran past 60 s).

| Tag | Cases | Pass | Fail | Skip | Crash | Timeout | Error |
|---|---:|---:|---:|---:|---:|---:|---:|
| `cpython` | 568 | 546 | 22 | 0 | 0 | 0 | 0 |
| `host` | 81 | 62 | 7 | 12 | 0 | 0 | 0 |
| `monty-specific` | 8 | 2 | 6 | 0 | 0 | 0 | 0 |
| `excluded` | 69 | 0 | 0 | 69 | 0 | 0 | 0 |
| total | 726 | 610 | 35 | 81 | 0 | 0 | 0 |

## Failures by first cause (35)

The first thing that stops each case: a feature `mm` reports it does not support, a 64-bit int limit (`bigint`), one int operation's work past about 50 ms (`int work cap`, docs/spec.md §11), a builtin it lacks, else how the result differs (`wrong output`, `wrong exception`, `wrong traceback`, `no exception raised`, `unexpected <Type>`), or `crash`/`timeout`. Largest first, to order the milestones.

| Cause | Cases | Examples |
|---|---:|---|
| unexpected AssertionError | 20 | `args__macro_errors.py`, `bytes__percent_format.py`, `class__numeric.py` |
| missing builtin | 6 | `builtin__eval_exec.py`, `builtin__exec_import_traceback.py`, `builtin__exec_namespaces.py` |
| unexpected NotImplementedError | 2 | `builtin__setattr.py`, `dataclass__basic.py` |
| wrong exception | 2 | `fstring__error_invalid_spec.py`, `fstring__error_invalid_spec_str.py` |
| wrong traceback | 2 | `dataclass__call_field_error.py`, `dataclass__get_missing_attr_error.py` |
| int work cap | 1 | `builtin__math_funcs.py` |
| no exception raised | 1 | `import__star_error.py` |
| unexpected ValueError | 1 | `str__format.py` |

## Failing cases (35)

| Case | Tag | Status | Cause | Reason |
|---|---|---|---|---|
| `args__macro_errors.py` | cpython | fail | unexpected AssertionError | unexpected exception: AssertionError: py-missing-pos: replace expected at least 2 arguments, got 1 |
| `bytes__percent_format.py` | cpython | fail | unexpected AssertionError | unexpected exception: AssertionError |
| `class__numeric.py` | cpython | fail | unexpected AssertionError | unexpected exception: AssertionError |
| `collections__deque.py` | cpython | fail | unexpected AssertionError | unexpected exception: AssertionError: a NaN element makes the ordering false |
| `collections__namedtuple.py` | cpython | fail | unexpected AssertionError | unexpected exception: AssertionError: a NaN element leaves the pair unordered |
| `fstring__all.py` | cpython | fail | unexpected AssertionError | unexpected exception: AssertionError: expected precision too big for 0.0001 with '.2147482624g' |
| `functools__lru_cache.py` | cpython | fail | unexpected AssertionError | unexpected exception: AssertionError |
| `id__non_overlapping_lifetimes_distinct_types.py` | monty-specific | fail | unexpected AssertionError | unexpected exception: AssertionError |
| `id__non_overlapping_lifetimes_same_types.py` | cpython | fail | unexpected AssertionError | unexpected exception: AssertionError |
| `import__sys_monty.py` | monty-specific | fail | unexpected AssertionError | unexpected exception: AssertionError: version should be 3.14.0 (Monty), got '&lt;version> (micro-monty)' |
| `iter__generator_expr_type.py` | monty-specific | fail | unexpected AssertionError | unexpected exception: AssertionError |
| `iter__user_class.py` | cpython | fail | unexpected AssertionError | unexpected exception: AssertionError |
| `lookup__mutation.py` | cpython | fail | unexpected AssertionError | unexpected exception: AssertionError |
| `str__percent_format.py` | cpython | fail | unexpected AssertionError | unexpected exception: AssertionError |
| `typing__get_type_hints.py` | cpython | fail | unexpected AssertionError | unexpected exception: AssertionError |
| `typing__get_type_hints_classes.py` | cpython | fail | unexpected AssertionError | unexpected exception: AssertionError |
| `typing__get_type_hints_dataclasses.py` | cpython | fail | unexpected AssertionError | unexpected exception: AssertionError |
| `typing__get_type_hints_external.py` | host | fail | unexpected AssertionError | unexpected exception: AssertionError |
| `typing__get_type_hints_external_async.py` | host | fail | unexpected AssertionError | unexpected exception: AssertionError |
| `typing__get_type_hints_functions.py` | cpython | fail | unexpected AssertionError | unexpected exception: AssertionError |
| `builtin__eval_exec.py` | cpython | fail | missing builtin | unexpected exception: NameError: name 'eval' is not defined. micro-monty does not support eval() |
| `builtin__exec_import_traceback.py` | cpython | fail | missing builtin | traceback line 7: expected \~\~\~\~\~\~\~\~\~\~\~\~\~\~\~\~\~\~\~\~\~\~\~\~\~\~\~\~\~\~\~\~\~\~\~\~\~\~\~\~\~\~\~\~\~\~\~\~\~\~\~\~\~\~\~, got \~\~\~\~ |
| `builtin__exec_namespaces.py` | cpython | fail | missing builtin | unexpected exception: NameError: name 'exec' is not defined. micro-monty does not support exec() |
| `builtin__exec_traceback.py` | cpython | fail | missing builtin | traceback line 4: expected \~\~\~\~\~\~\~\~\~\~\~\~\~\~\~\~\~\~\~\~\~\~\~\~\~\~\~\~\~\~\~\~\~\~\~\~\~\~\~\~\~\~\~\~\~\~\~\~\~\~\~, got \~\~\~\~ |
| `ext_call__eval_exec.py` | host | fail | missing builtin | unexpected exception: NameError: name 'eval' is not defined. micro-monty does not support eval() |
| `inline__string_operations.py` | cpython | fail | missing builtin | unexpected exception: NameError: name 'eval' is not defined. micro-monty does not support eval() |
| `builtin__setattr.py` | host | fail | unexpected NotImplementedError | unexpected exception: NotImplementedError: mm's host fixtures do not provide host objects (make_mutable_point) until clas… |
| `dataclass__basic.py` | host | fail | unexpected NotImplementedError | unexpected exception: NotImplementedError: mm's host fixtures do not provide host objects (make_point) until classes land… |
| `fstring__error_invalid_spec.py` | monty-specific | fail | wrong exception | expected SyntaxError("Invalid format specifier '10xyz'"), got ValueError("Invalid format specifier '10xyz' for object of type 'int'… |
| `fstring__error_invalid_spec_str.py` | monty-specific | fail | wrong exception | expected SyntaxError("Invalid format specifier 'abc'"), got ValueError("Invalid format specifier 'abc' for object of type 'str'") |
| `dataclass__call_field_error.py` | host | fail | wrong traceback | traceback line 2: expected …aclass__call_field_error.py", line 6, in &lt;module>, got …aclass__call_field_error.py", line 5, in &lt;module> |
| `dataclass__get_missing_attr_error.py` | host | fail | wrong traceback | traceback line 2: expected …__get_missing_attr_error.py", line 6, in &lt;module>, got …__get_missing_attr_error.py", line 5, in &lt;module> |
| `builtin__math_funcs.py` | cpython | fail | int work cap | unexpected exception: MemoryError: integer operation too large |
| `import__star_error.py` | monty-specific | fail | no exception raised | no exception raised, expected Traceback (most recent call last):\\n  File "import__star_error.py", line 4, in &lt;module>\\n    from s… |
| `str__format.py` | cpython | fail | unexpected ValueError | unexpected exception: ValueError: Unknown format code '５' for object of type 'int' |
