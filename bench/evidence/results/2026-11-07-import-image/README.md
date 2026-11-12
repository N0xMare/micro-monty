# Import-image prototype (2026-11-07)

docs/research/import-image.md. x86-64 orb (16 cores), release builds. All
numbers are [M].

## Builds

- `base`: origin/main `61f216c` (size pass 2).
- `img`: local branch `local/import-image` (`0c1b5e9`, fixups `3967b26`,
  `bd81e26`; not pushed), with images for `json, re, collections, asyncio`.
  `MM_NO_IMAGES=1` makes the same build import without images (the body
  path); `MM_IMAGES=a,b` at build time picks the captured modules.

`-off` is `IMP_CACHE=0` (`Engine::with_module_cache(config, 0)`), `-on` the
facade default.

## Commands

```bash
# imp for each build: bench/evidence/import_cost on base / on the branch
cargo build --release --manifest-path bench/evidence/import_cost/Cargo.toml --target-dir target/import-cost-<build>
IMP=target/import-cost-<build>/release/imp [IMP_CACHE=0] python3.14 -I bench/evidence/import_cost/measure.py ir-<build>-<cache>.json --no-wall
python3 wall.py wall.json      # both builds, 3 rounds, pinned; run from the repo root
taskset -c 2 target/import-cost-wasm/release/wasm-imp GUEST.wasm 21 - json re collections asyncio json,re,collections json,re,collections,asyncio,datetime > wasm-<build>.jsonl
cargo xtask size               # size.txt; MM_IMAGES="" or "json,re,collections" for the other rows
python3.14 -I bench/evidence/import_image/parse.py IMAGE   # sections of one image
# graph equivalence (branch only; the harness needs the image-debug feature)
cargo build --release --manifest-path bench/evidence/import_image/Cargo.toml --target-dir target/import-image
img dump eq/sN.py MODS ; MM_NO_IMAGES=1 img dump eq/sN.py MODS   # and `img snap`
```

## Files

- `ir-{base,img}-{on,off}.json`: per import set, `steady_ir`
  ((Ir of 11 operations − Ir of 1) / 10; one operation = `Session::new`
  plus the import cell) and `first_ir`.
- `ir-heavy-base.json`: the same for `decimal, statistics, zoneinfo,
  tomllib` and `datetime` on base, cache warm (the projection's anchor).
- `wall.json`, `wall.py`: native wall time, the median of 51 per round, 3
  rounds, `taskset -c 2`; `median_min` is the minimum of the round medians.
- `wasm-{base,img}.jsonl`: the shipped `wasm32-unknown-unknown` guest
  under wasmtime, new instance + session + import, median of 21, pinned,
  with guest memory.
- `size.txt`: `cargo xtask size` on base and with all four images raw.
- `eq/`: the 18 equivalence scripts and `summary.txt` (graft vs body, the
  graph dump in-process and with a snapshot/restore after every cell; every
  line must say `same`, and the graft counts show which runs grafted).
- `durability.log`, `durability-cross.log`: `cargo xtask durability` and
  `--cross-target` on the branch with images active.
  `durability-imports.py`: the import script also parked every 1, 3, 12 and
  28 fuel units.
