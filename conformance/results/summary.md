# Conformance results: CPython 3.14

[M] `cargo xtask conformance` on CPython 3.14.7, over the 726 cases of `conformance/monty/` (Monty `66fff6bf`). Per-case results are in `cpython-3.14.jsonl`; CI reruns the corpus with `--check` and fails if they change. Regenerate both files with `cargo xtask conformance`.

Tags come from each case's file name and `# /// monty` header: `excluded` (`refcount__*`), `monty-specific` (`xfail` includes `cpython`), `host` (`call-external` or `mount-fs`, run on CPython through the datatest's emulated fixtures) and `cpython` (the rest). Monty's own CI also runs these cases on CPython 3.14.

| Tag | Cases | Pass | Fail | Other |
|---|---:|---:|---:|---:|
| `cpython` | 568 | 564 | 4 | 0 |
| `host` | 81 | 81 | 0 | 0 |
| `monty-specific` | 8 | 0 | 8 | 0 |
| `excluded` | 69 | 2 | 0 | 67 |
| total | 726 | | | |

`Other` is `skip` (ref-counts expectations, which the datatest skips on CPython), `error` or `timeout`.

## `cpython` cases failing on CPython 3.14 (4)

By kind: CPython unexpected exception: 4.

| Case | Kind | Reason |
|---|---|---|
| `bytes__percent_format.py` | CPython unexpected exception | unexpected exception: AssertionError |
| `fstring__all.py` | CPython unexpected exception | unexpected exception: AssertionError: expected precision too big for 0.0001 with '.2147482624g' |
| `str__format.py` | CPython unexpected exception | unexpected exception: AssertionError |
| `str__percent_format.py` | CPython unexpected exception | unexpected exception: AssertionError |

## `host` cases failing on CPython 3.14 (0)


## `monty-specific` cases (8)

Expected to fail on CPython (strict `xfail`); 0 pass on 3.14.

| Case | Status | Reason |
|---|---|---|
| `fstring__error_invalid_spec.py` | fail | expected SyntaxError("Invalid format specifier '10xyz'"), got ValueError("Invalid format specifier '10xyz' for object of type 'int'… |
| `fstring__error_invalid_spec_str.py` | fail | expected SyntaxError("Invalid format specifier 'abc'"), got ValueError("Invalid format specifier 'abc' for object of type 'str'") |
| `id__bytes_literals_distinct.py` | fail | expected False, got True |
| `id__non_overlapping_lifetimes_distinct_types.py` | fail | unexpected exception: AssertionError |
| `import__star_error.py` | fail | no exception raised, expected a traceback |
| `import__sys_monty.py` | fail | unexpected exception: AssertionError: version should be 3.14.0 (Monty), got '&lt;sys.version>' |
| `iter__generator_expr_type.py` | fail | unexpected exception: AssertionError |
| `range__slice_overflow.py` | fail | unexpected exception: AssertionError: expected OverflowError from slice with overflowing step |
