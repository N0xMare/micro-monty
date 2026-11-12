# Dunder flags by hash (2026-11-07)

`base.json`: 357e003; `new.json`: the class-flag commit. `imp` from
`bench/evidence/import_cost` built against each, measured on a 16-vCPU x86-64
orb (Xeon), wall time pinned to CPU 2, 201 repetitions:

```bash
cargo build --release --manifest-path bench/evidence/import_cost/Cargo.toml --target-dir target/import-cost
IMP=imp-base python3.14 -I bench/evidence/import_cost/measure.py base.json --mods itertools,json+re+collections+asyncio+datetime --reps 201
IMP=imp-new  python3.14 -I bench/evidence/import_cost/measure.py new.json  --mods itertools,json+re+collections+asyncio+datetime --reps 201
```

| import, cache warm | steady Ir 357e003 | after | wall µs 357e003 | after |
|---|---:|---:|---:|---:|
| `itertools` | 1,918,419 | 1,553,971 (−19.0%) | 178 | 92 (−48%) |
| `json, re, collections, asyncio, datetime` | 12,912,481 | 12,533,403 (−2.9%) | 1,475 | 1,337 (−9.4%) |

Steady Ir and wall are the import alone (minus a session without one). Under
callgrind (`imp ir itertools 11`), `compute_flags` fell from 4.92M to 1.03M Ir
inclusive and `memcmp` from 1.77M to 0.25M.
