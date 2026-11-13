# Correctness breadth: micro-monty vs Monty vs qpy, against CPython 3.14.7

`run.py` runs the own cases, the Monty corpus and the bench programs on
CPython 3.14.7 (the oracle), micro-monty (`mm run`), Monty (the published
`monty` CLI) and qpy, and classifies each case per engine; `results.jsonl`
holds the 2026-11-04 run [M]. No gate reads these files. Correctness only, no
timings.

## Commands

```bash
# Every case on all four engines, 4 parallel processes, 30 s timeout each:
python3.14 -I conformance/breadth/comparators/run.py --jobs 4
# Tables and cause lists (from results.jsonl):
python3.14 -I conformance/breadth/comparators/run.py --report
```

Engine paths can be overridden with `PYTHON`, `MM`, `MONTY`, `QPY` and
`QPYHOME`. The full run (2940 processes) takes about a minute [M].

The run's report (engines, hashes, method, per-engine failure causes) is in
git history before the docs were cut to `docs/spec.md` and
`docs/user-guide.md`.
