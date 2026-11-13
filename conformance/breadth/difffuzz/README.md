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
- `cpython-timeouts.txt`: each CPython-timeout seed with its cause.
- The run's output, `raw/seed-N.py` (the minimized divergences, both outputs
  in the header) and `run.log` (with the signature table), moved to the
  `evidence` branch of the private archive `N0xMare/micro-monty-archive`, at
  the same paths; both are at `da1c21b5` too.

The run's triage report is in git history before the docs were cut to
`docs/spec.md` and `docs/user-guide.md`.
