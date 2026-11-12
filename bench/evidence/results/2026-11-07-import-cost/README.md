# Import-cost pass (2026-11-07)

docs/research/import-cost.md. x86-64 orb, release builds. All numbers are
[M].

## Builds

- `base`: origin/main `7541558`, before the pass and the module cache.
- `cacheonly`: `1c03cff`, the Engine's decoded-module cache (Scope decision
  45), before this pass.
- `final`: this pass on `1c03cff`.

`-off` is `IMP_CACHE=0` (`Engine::with_module_cache(config, 0)`), `-on` the
facade default (`Engine::new`; `base` has no cache).

## Commands

```bash
cargo build --release --manifest-path bench/evidence/import_cost/Cargo.toml --target-dir target/import-cost
IMP=<imp of the build> [IMP_CACHE=0] python3.14 -I bench/evidence/import_cost/measure.py ir-<build>-<cache>.json --no-wall
python3.14 -I bench/evidence/import_cost/measure.py wall-<build>.json     # idle machine, pinned
python3.14 -I bench/evidence/import_cost/split.py > split-final-on.md
IMP_CACHE=0 python3.14 -I bench/evidence/import_cost/split.py > split-final-off.md
target/import-cost-wasm/release/wasm-imp GUEST.wasm 21 - json re ... > wasm-<build>.jsonl
```

## Files

- `ir-*.json`: per import set, `steady_ir` ((Ir of 11 operations − Ir of 1)
  / 10, one operation = `Session::new` plus the import cell) and
  `first_ir` (Ir of a process doing one operation). Subtract the `-` entry
  (a session without imports) for the import alone. Their wall fields are
  zero (`--no-wall`).
- `wall-*.json`: the same harness with wall time (median of 51 after one
  warm-up, `taskset -c 2`) and RSS per session (40 sessions held).
- `split-final-{on,off}.md`: Ir per import inside `Vm::execute`, split into
  bundled-module decode, prelude units, and load plus module bodies.
- `wasm-*.jsonl`: the shipped `wasm32-unknown-unknown` guest through
  `mm-wasm-host`, a new instance and session plus the import cell, median of
  21.
- `stmts-final.txt`: the slowest top-level statements of the main modules
  (`stmts.py`).
