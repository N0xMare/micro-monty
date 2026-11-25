# fuzz/

Its own cargo workspace: the libFuzzer targets in `fuzz_targets/` and the
standalone checkers in `src/bin/`, which are not fuzz targets. Each checker
needs its feature, so `cargo fuzz build` skips them. `memory_probes` is in
`cargo xtask check` (docs/spec.md §11). The sweeps below are in no tier
and run by hand: before a tag, and after a change to how natives charge
fuel or memory.

## `fuel_sweep`: every public native in small fuel slices

```bash
cargo run --release --locked --manifest-path fuzz/Cargo.toml --features fuel-sweep --bin fuel_sweep
```

It measures docs/spec.md §5.1 clause 3. It runs each fixture of
`fuel_sweep.txt` in fresh sessions, at sizes on both sides of the step and
chunk limits, in fuel slices of 64 units. A run fails when a slice is
overdrawn by more than 2^21 + 2^17 units without first parking with
`fuel_wanted` set, or when it ends with an outcome other than its
fixture's. The sweep also fails if the native inventory (`--inventory`
lists it) has an entry with no fixture. Pass a substring to run only
matching keys, such as `fuel_sweep str.`. Each failure prints its setup
and probe to stderr.

The table's format is in its header. A fixture measures the native's own
work: the result goes to a variable, so the completion `repr` is not
measured. Its setup runs unmetered.

## `budget_sweep`: one refused memory-budget charge per run

```bash
python3.14 fuzz/budget_sweep.py
```

It checks that a refused charge against the session's memory budget
(§11's admissions) is a catchable `MemoryError` and never a crash, a hang
or a session that cannot go on. The driver builds
`budget_sweep` with `--features budget-sweep`, which turns on `mm-core`'s
test-only `budget-fault-injection`: every positive charge that would fit
is counted, and the Nth is refused as if the budget were full. Each own
case (`conformance/own/*.py`) first runs unarmed. That run is its
baseline and gives its charge count. Then the case runs once for each of
up to `--per-case` indices (default 256), spread evenly from the first
charge to the last. Every run is its own process, and after the cell a
canary cell runs in the same session. `--catch` is a control case that
must catch a `MemoryError` from `import statistics`.

The driver prints one line per distinct non-baseline result, with the
indices that gave it, then the totals per class:

- `same as baseline`, `raised MemoryError` (uncaught) and `printed
  MemoryError` (the output now has more `MemoryError` text than the
  baseline: the case caught one and printed it) are expected. Judge a
  `printed MemoryError` line against the case: the text it prints for the
  error is the case's.
- `review: outcome changed` and `review: output changed` are changed
  results without `MemoryError` text: a case that caught and reported the
  error in its own words, or a wrong result reported as success. Judge
  them by hand.
- `failure: abort or hang` (a panic, a signal, the per-child timeout or
  2,000 slices of 10^6 fuel), `failure: session cannot continue` (the
  session is poisoned, a resume fails or the canary does not return 42)
  and `failure: missed injection` fail the sweep.

To replay one run, build as the driver does and run the binary on a case
and a charge index:
`fuzz/target/release/budget_sweep "$PWD/conformance/own/CASE.py" N`. It
prints `charges<TAB>outcome<TAB>detail`, then the cell's output, and an
uncaught exception's report on stderr. Pass the absolute path, as the
driver does: the file name is in traceback text, so its length moves the
charge indices. Under a debugger, `mm_core::budget_fault::refused` runs
only at the refused charge, with the charging site on the stack.
