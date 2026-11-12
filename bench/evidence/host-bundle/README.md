# The bundle guest: stdlib and prelude bytecode from the host

Size review 2026-11-09, lever E, shipped as a second guest artifact
(owner, 2026-11-09; docs/research/wasm-embedding.md, "Two guests"). The
measurements below were taken on the prototype (E alone on cf4aa86); the
landed form differs only in the build pipeline (`cargo xtask size` appends
the sections) and in build ids, which are now equal for both guests.

## What it does

A guest built with the `host-bundle` feature (mm-wasm-guest →
micro-monty → mm-stdlib → mm-engine) links neither the bundled modules'
packed code nor the prelude's packed chunks. The prelude's shared Huffman
header and the modules' shared packing sections stay in the module. The
code is read on demand, the first time a session needs a prelude chunk or
imports a module, through one new import:

```text
mm_host.bundle_read(region: u32, offset: u32, ptr: u32, len: u32) -> u32
  region 0: the prelude's chunks after their header
  region 1: the bundled modules' code
  copies len bytes from offset of region to guest memory at ptr; 1 if it could
```

The bytes travel in the same module, as the custom sections `mm.prelude`
and `mm.stdlib`. `cargo xtask size` appends them after `wasm-opt`
(`xtask/src/wasm.rs`, `shipped_bundle_guest`). Wasm
never copies custom sections into linear memory. The host reads them once
per loaded module: the JS host through `WebAssembly.Module.customSections`,
the wasmtime host by walking the module's sections. Every instance then
shares that one copy. The artifact stays a single `.wasm` file, so build
skew between code and bytecode is impossible.

`#[link_section]` statics were tried first and rejected [M]: rustc still
emits the initializer into the data section, so the bytes appeared twice.
A 5-line crate shows the same.

Engine side: `StdModule` has a new field `external: Option<(offset, len)>`.
`ModuleCache` has a new default method `bundle(region, offset, out)`, and
the facade answers it from a process-wide `set_bundle_reader`. The guest
sets the reader to the import. Both hosts refuse to load a module that
imports `bundle_read` without both sections; a read that still fails
raises `SystemError` ("the prelude is corrupt", "a bundled module is
corrupt"), never a trap.

## Interface and fallback

- Only `host-bundle` builds declare the import; the default guest imports
  nothing new. No export, codec or snapshot-format change.
- **No fallback.** Wasm imports are not optional, so a host without
  `bundle_read` fails to instantiate the bundle guest with a `LinkError`
  and runs the default guest. A host with it refuses a bundle guest that
  lacks either section at load. `cargo xtask wasm-host` always tests all
  three guests (wasip1, unknown-unknown and the bundle guest).
- Native builds are unaffected. The feature is meant for wasm guests only.

## Measurements [M]

Guest `wasm32-unknown-unknown`, built as shipped (build-std,
immediate-abort, `wasm-opt -Oz`), cf4aa86 sources.

| | default | host-bundle | Δ |
|---|---:|---:|---:|
| file | 2,237,782 | 2,238,468 (with sections) | +686 |
| code section | 1,762,670 | 1,762,940 | +270 |
| data segments | 468,693 | 117,853 | **−350,840** |
| initial linear memory | 24 pages | 18 pages | −384 KiB |
| custom sections | — | `mm.stdlib` 279,738, `mm.prelude` 72,010 | |

All 65 bundled modules import with the same `sorted(dir(m))` SHA-256 on
both guests (Node).

### wasmtime 50 (crates/mm-wasm-host, `examples/bundle_mem.rs`)

100 live sessions, precompiled module (copy-on-write data), 2 runs each:

| per session | default | host-bundle |
|---|---:|---:|
| linear memory, idle / after imports | 1,728 / 2,624 KiB | 1,344 / 2,240 KiB |
| RSS, idle | 247–248 KiB | 226–227 KiB |
| RSS, after `x = 1 + 2` | 257 KiB | 232 KiB |
| RSS, after `import json, re, collections` | 1,340 KiB | 1,233 KiB |
| new session (median of 41) | 122–144 µs | 119–122 µs |
| first `import json` / `re` / `collections` / `datetime` | 237–257 / 1,152–1,164 / 1,128–1,130 / 2,333–2,550 µs | 224–226 / 1,180–1,187 / 1,080–1,128 / 2,394–2,491 µs |

wasmtime maps data segments copy-on-write, so untouched data was never
resident; the RSS saving is the touched part (−21 KiB idle, −107 KiB after
three imports). The host holds one 352 KB copy per loaded module.

### Node v26.12.1 (hosts/js host, `node.ts`)

40 live sessions, 3 runs:

| per session | default | host-bundle |
|---|---:|---:|
| RSS, idle | 601 / 607 / 620 KiB | 257 / 264 / 270 KiB |
| RSS, after a cell | 699–719 KiB | 354–381 KiB |
| RSS, after `import json, re, collections` | 1,618–1,692 KiB | 1,412–1,458 KiB |
| linear memory, idle | 1,728 KiB | 1,344 KiB |
| new session (median of 41) | 535–612 µs | 327–384 µs |
| first import json / re / collections / datetime | 261–307 / 1,128–1,187 / 1,095–1,142 / 2,206–2,536 µs | 260–299 / 1,170–1,275 / 1,065–1,189 (one 1,778) / 2,236–2,743 µs |
| compile | 5.7–6.9 ms | 6.1–7.1 ms |

V8 copies data segments into each instance eagerly, so the idle saving is
nearly the whole data: −343 KiB RSS per session, −57%. The new session is
also about 200 µs faster, since it copies 350 KB less.

### workerd (hosts/js node_modules, `run_workerd.py`)

40 live sessions, 5 runs for memory and 2 for times; times are
`performance.now()` outside, over batches of 50 fresh sessions (workerd's
clock is ms-grained):

| per session | default | host-bundle |
|---|---:|---:|
| RSS, idle | 566–576 KiB | 261–270 KiB |
| RSS, after a cell | 593–605 KiB | 281–373 KiB |
| RSS, after `import json, re, collections` | 1,522–1,545 KiB | 1,317–1,430 KiB |
| new session + first cell | 592–625 µs | 396–465 µs |
| first import json / re / collections / datetime | 191–250 / 1,167–1,235 / 1,037–1,152 / 2,451–2,554 µs | 319–379 / 1,346–1,360 / 1,248–1,374 / 2,632–2,885 µs |

**First imports on workerd, 12 rounds (landed guests, D+F+E on cf4aa86).**
The first two rounds above suggested 0.1–0.3 ms slower first imports. To
settle it: `CORE=3 run_workerd.py --times 12`, guests alternating within
each round, a fresh workerd per guest and round pinned to one core, each
number the median of 5 batches of 50 fresh sessions
(`workerd-times-2026-11-09.json`) [M]:

| µs per session | default (median, range) | bundle (median, range) | paired difference (median) | rounds slower |
|---|---:|---:|---:|---:|
| new session + `x = 1` | 1,190 (1,021–1,258) | 978 (782–1,269) | **−229** | 2 of 12 |
| first `import json` | 131 | 228 | +137 | 10 of 12 |
| first `import re` | 1,312 | 1,423 | +110 | 7 of 12 |
| first `import collections` | 960 | 1,056 | +89 | 8 of 12 |
| first `import datetime` | 2,321 | 2,492 | +119 | 8 of 12 |

The slowdown is real but about 0.1 ms per first import, about the same for
a small module (json) and a large one (datetime), so it is not the copy of
the module's bytes (at most about 28 KB). Session creation is 0.23 ms
faster, so a session that imports two bundled modules breaks even. Node
and wasmtime showed no import difference.

## Reproduce

```bash
cargo xtask size    # both guests: target/wasm32-unknown-unknown/shipped/mm_wasm_guest.wasm,
                    # target/mm-wasm-bundle/wasm32-unknown-unknown/shipped/mm_wasm_guest_bundle.wasm
(cd crates/mm-wasm-host && cargo run --release --example bundle_mem -- GUEST.wasm 100)
node --expose-gc bench/evidence/host-bundle/node.ts GUEST.wasm 40
python3 bench/evidence/host-bundle/run_workerd.py 40 default.wasm bundle.wasm
```
