# Differential fuzz divergences

Output of one 40-minute run of `fuzz/difffuzz/difffuzz.py` (a grammar-based
program generator) against CPython 3.14.7, on 2026-11-04 [M]. No gate reads
these files.

```bash
python3.14 fuzz/difffuzz/difffuzz.py --mm target/release/mm --python python3.14 \
    --jobs 4 --minutes 40 --seeds 10000000 --start 0 --out conformance/breadth/difffuzz/raw
```

- `*.py` here: minimized repros. Run each with `python3.14 -X utf8 F` and
  `mm run F` to compare.
- `raw/seed-N.py`: the fuzzer's minimized divergences, with both outputs in
  the header.
- `run.log`: the fuzzer's output, including the signature table.
- `cpython-timeouts.txt`: each CPython-timeout seed with its cause.

The run's triage report is in git history before the docs were cut to
`docs/spec.md` and `docs/user-guide.md`.
