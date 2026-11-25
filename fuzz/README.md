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
