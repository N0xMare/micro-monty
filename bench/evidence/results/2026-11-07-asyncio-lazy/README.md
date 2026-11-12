# asyncio attributes on first use, and the prelude chunk cache (2026-11-07)

docs/research/import-cost.md §5. Three builds, all on 168b7ac:
`base` (168b7ac), `lazy` (asyncio's prelude attributes built on first use)
and `new` (`lazy` plus the Engine's cache of unpacked prelude chunks).

Cells: `asyncio`, the five-module cell `json, re, collections, asyncio,
datetime` (imported, then deleted), and two cells that use asyncio
(`bench/evidence/import_cost/cells/`): `asyncio_gather.py` (gather and run)
and `asyncio_primitives.py` (Queue, Lock, Semaphore, TaskGroup,
create_task, wait_for).

- `native-<build>-cache<1|0>.json`: `measure.py` (steady Ir per session,
  first-in-process, RSS per session with 40 held), with the Engine's cache
  on (1) or off (0). Steady Ir includes `Session::new` (the `-` row,
  0.50M).
- `wall-alternating.json`: wall time, pinned to core 2, `imp time MODS 101`,
  7 rounds alternating the builds; the medians of the 7 medians.
- `wasm.jsonl`: the shipped `mm_wasm_guest.wasm` of each build through
  `mm-wasm-host` (`wasm-imp GUEST 21 ...`, 3 rounds alternating; one
  session per instance, so the Engine cache only helps within a session).
- `wasm-guest-nochunks.jsonl`: the guest with the chunk cache off (`new`,
  the owner's decision) against c0f37cd's (`base`), 5 alternating rounds of
  21; `guest_memory` is the instance's linear memory.
- `split-on.md`, `split-off.md`: `split.py` of `new` (Ir inside
  `Vm::execute`, by phase).
- `bench-ir.md`: the 13 bench programs, `base` against `new`.

Reproduce:

```bash
cargo build --release --manifest-path bench/evidence/import_cost/Cargo.toml --target-dir target/import-cost
C=$PWD/bench/evidence/import_cost/cells
M="asyncio,json+re+collections+asyncio+datetime,@$C/asyncio_gather.py,@$C/asyncio_primitives.py"
python3.14 -I bench/evidence/import_cost/measure.py OUT.json --mods "$M"
IMP_CACHE=0 python3.14 -I bench/evidence/import_cost/measure.py OUT.json --mods "$M"
target/import-cost-wasm/release/wasm-imp GUEST.wasm 21 - asyncio json,re,collections,asyncio,datetime @$C/asyncio_gather.py @$C/asyncio_primitives.py
```
