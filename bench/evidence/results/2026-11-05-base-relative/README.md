# Base-relative snapshots (2026-11-05)

[M] unless labelled. Engine: `06f1fbc` (unchanged; `5568b27`, the mm-wasm v1
freeze, changes no engine crate). The prototype is host-side and links the
unchanged facade: `bench/evidence/base_relative/` (`br`, SHA-256
`e3cc869cdfd88e09…`, full stdlib feature set as `mm`). Analysis and
recommendation: `docs/research/base-relative-snapshots.md`.

```
cargo build --release --manifest-path bench/evidence/base_relative/Cargo.toml --target-dir target/base-relative
cargo build --release --locked -p mm-codemode --features optional-modules --target-dir target/codemode/full
python3.14 -I bench/evidence/base_relative/run.py OUT --jobs 14                 # startup base
python3.14 -I bench/evidence/base_relative/run.py OUT --jobs 14 --only own,codemode \
    --warm "import json, asyncio, re, collections, datetime" --keep KEEP       # warm base
python3.14 -I bench/evidence/base_relative/summarize.py OUT                     # summary-*.md
python3.14 -I bench/evidence/base_relative/dictionary.py KEEP OUT/base.mmsnap   # dictionary.md
# timing.md: run.py --jobs 1 under `taskset -c 2`, one workload at a time
# (--only own --filter 369_asyncio_tasks, --only codemode --filter luna-b/enc-etag,
# --only bench --filter fib.py, --only bench --filter dict.py, --only state),
# then summarize.py --timing over those directories.
```

| File | What |
|---|---|
| `summary-startup-base.md` | Sizes, byte and segment identity, ratios and behaviour against the startup base |
| `summary-warm-base.md` | The same for own and codemode against a warm base (fresh session after five imports) |
| `dictionary.md` | zstd-1 with a trained dictionary, held out by solution set |
| `timing.md` | Save and restore times on a pinned core (ms; engine + transform + zstd) |
| `images-startup-base.jsonl` | Per image: every own, bench and state park measured, and the first park of each codemode run |
| `checks-startup-base.jsonl` | Per run: swap run (every park restored from base + delta) against the plain run or the unchanged codemode runner |

Determinism (fresh-session image SHA-256):

| Build | Where | Bytes | SHA-256 |
|---|---|---:|---|
| full features (`br`) | native x86-64, 5 processes; 2 more with `env -i TZ=Asia/Tokyo` | 40,762 | `06f4c530…` (all 7) |
| full features (`br`) | `wasm32-wasip1` under wasmtime | 40,762 | `06f4c530…` |
| default features | native facade; mm-wasm guest `wasm32-unknown-unknown` (3 instances), guest `wasm32-wasip1`, guest after `wasm-opt -Oz` | 40,702 | `ef009a06…` (all) |

Wrong-base rejection (`br reject`, delta of `own/01_args_defaults.py`'s first park):

| Wrong base | base id check | id forced, rebuilt-image checksum | no rebuild check, `Engine::restore` | no checks at all |
|---|---|---|---|---|
| warm base (same build) | refused | refused | `Corrupt { checksum }` | `Corrupt { heap }` |
| default-feature build's base | refused | refused | `Corrupt { checksum }` | **restores**, into a different session |

zstd with a wrong raw prefix: without zstd's checksum flag the warm base
decodes to different bytes (33,432 of 50,169 differ; the image's own XXH64
trailer is what `Engine::restore` would then reject); with the flag, or with
the other build's base, zstd refuses.

Guest size with relative snapshot/restore exports (`guest-size-probe.patch`
plus `delta.rs` in `crates/mm-wasm-guest/src/`, shipped build flags,
`wasm-opt -Oz`, on `06f1fbc`): 2,223,853 → 2,246,328 B (+22,475; gzip −9
+8,487).
