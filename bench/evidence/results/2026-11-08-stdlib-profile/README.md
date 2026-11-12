# Stdlib profile measurements (2026-11-08)

docs/research/stdlib-profile.md. Every number is [M], on `e96ca7c`
(origin/main), in an x86-64 orb (16 vCPUs), Rust 1.99.0, binaryen 133.
The scripts are in `bench/evidence/stdlib_profile/`.

## Commands

```bash
S=bench/evidence/stdlib_profile; R=bench/evidence/results/2026-11-08-stdlib-profile
python3.14 -I $S/features.py --check                 # imports outside each feature's declared closure
python3.14 -I $S/removals.py $R/removals.json --jobs 8 --configs EXTRA.json   # sizes
python3.14 -I $S/census.py $R/usage.json --profile NAME=mod,mod,...           # static usage
python3.14 -I $S/behaviour.py $R/behaviour.json NAME=mod,mod,... baseline     # both corpora, run
python3.14 -I $S/report.py $R                        # the doc's tables
```

Bench and import Ir: two worktrees at paths of equal length, `wt20` with
the default list and `wt21` with `profile:core35`, each with `mm-cli`'s
opt-in modules removed (`micro-monty = { workspace = true }`), so the two
`mm` binaries differ only in the module set:

```bash
python3 bench/callgrind.py --engine "default=/home/user/workspace/wt20/target/release/mm run" \
    --engine "core35=/home/user/workspace/wt21/target/release/mm run" > bench-ir.md
IMP=<wt>/target/import-cost/release/imp python3.14 -I bench/evidence/import_cost/measure.py \
    import-ir-<build>.json --no-wall --mods json,re,collections,asyncio,datetime,typing,functools,itertools,decimal,statistics,html.parser,json+re+collections+asyncio+datetime
IMP=... python3.14 -I bench/evidence/import_cost/measure.py import-wall-<build>.json --no-ir --mods json,re,collections,asyncio,datetime,json+re+collections+asyncio+datetime
```

## Files

- `removals.json`: per label, the shipped sizes of hello.wasm
  (wasm32-unknown-unknown, after `wasm-opt -Oz`), native `hello`
  (x86_64-unknown-linux-gnu) and `mm_wasm_guest.wasm` (after `wasm-opt
  -Oz`), with the module list built. `baseline` equals `cargo xtask size`
  on the checkout. `-m` drops `m` and every default module that imports
  it; `-m (declared group)` is the same with only the Cargo-declared
  edges (before the undeclared imports were found). `none` and
  `struct+operator` are whole-stdlib removals; `profile:*` are the sets.
- `profiles.json`: the measured sets (`default-minus-unused` is the default
  list without the 13 modules neither corpus uses).
- `usage.json`: the static census (`census.py`): per corpus, units using
  each module (directly or through an import-time dependency), direct
  importers, imports outside the default set, and per profile the units
  that import a module outside it.
- `behaviour.json`: per set, every real cell and synthetic solution whose
  result differs from the default build's committed expectation, with
  before/after outcomes and the message. `baseline` is the control (an
  unchanged build in a worktree: 0 and 0).
- `behaviour-minimality.json`: `profile:real-min` with one more module
  group removed, to check that no smaller set passes every real cell.
- `bench-ir.md`: whole-process callgrind Ir (millions) of the bench
  programs.
- `import-ir-*.json`, `import-wall-*.json`: `imp`'s steady and first
  Ir, and wall time and RSS (noisy in an orb), per import.
