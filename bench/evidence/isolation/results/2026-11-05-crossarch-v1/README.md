# Cross-machine snapshot chains on the mm-wasm v1 head (2026-11-05)

Every hop of every chain is a new process. Hops alternate between two
machines, and the chain's working directory (snapshot images and driver
state) is copied with rsync on each hop, so every restore reads an image
that the other machine wrote:

- **Mac**: MacBook Pro, Apple M1 Max, aarch64 macOS 27.2.
- **host1**: AMD Ryzen 9 8945HS, x86-64 Linux, bare metal, reached over ssh.
  It was running the 24 h fuzz campaign (16 threads busy) at the time, which
  affects only wall time.

Both machines ran `main` at `5568b27` (the mm-wasm v1 freeze), plus this
directory's harness additions (`iso-harness xchain`, which is under
`bench/evidence/isolation/harness`). Native `mm` reports the same build id
on both machines: `348d799c3ae266d9afd3608b7b995fb9cc80f31c70135222c8d3c7e7d64a1a40`.
Each machine built its own binaries: native `mm` and `mm` for
`wasm32-wasip1` (precompiled with `wasmtime compile`), plus the two mm-wasm
guests (`mm-wasm-guest --features test-hooks` for `wasm32-wasip1` and
`wasm32-unknown-unknown`). The wasm side used wasmtime 50.

`crossarch.py` is the driver; its docstring has the usage. Run at
2026-11-05, from 17:04:50Z (`started.txt`).

## 1. Host-API state: `iso-harness xchain` (`xchain.jsonl`)

The scenario is crates/mm-wasm-host/tests/parity.rs's v1 cell, extended:

- a host module with a `dict_as_kwargs` function, an untyped function, a
  constant and a host exception class;
- four answered host calls and one structured host error
  (`HostError { class: "tools.ToolError", data: {"code": 503} }`), caught
  in Python;
- `zoneinfo` zones `Europe/Paris` and `America/New_York` loaded from the
  host zone source;
- a `HintGroup::Network` hint;
- `7 ** 100000`, which parks before it starts (`fuel_wanted`);
- a result cut with `TextCut::HeadTail` at 40 bytes.

The cell runs on 2,000-unit fuel slices, so it parks 9 times and makes 5
host calls: 15 events in all. Each hop handles one event: it restores the
image, sets the host-side settings again (text cut, text limit, the zone
source, the fuel left), answers the pending event, runs to the next event,
and saves a new image.

The hint, the host module and the zones are set only when the session is
created, so they must travel in the images. At the end, the call log is
read. A tail cell then runs with **no zone source**: `ZoneInfo('Europe/Paris')`
and `ZoneInfo('America/New_York')` must come from the image. Finally
`import requests` must end with the hint. The final report must equal a
single-process run with no snapshots.

| What | Result [M] |
|---|---|
| Single-process references (native, `abi` = wasip1 guest, `abi-uu` = unknown-unknown guest; on each machine) | 6 of 6 identical |
| Chains with one engine throughout, machines alternating (native, abi, abi-uu; each starts on either machine) | 6 of 6 identical |
| Chains rotating the three engines, all 6 orders, each starting on either machine | 12 of 12 identical |
| Hops / restores, all from an image the other machine wrote | 270 / 252 |
| Each mixed chain covers | all 6 (machine, engine) pairs |

"Identical" covers every event's `Progress` (Debug text), the output, the
call log (`HostCallRecord`s with `Returned`/`Raised`), the tail's
completion and printed zone offsets, and the hinted `ModuleNotFoundError`.

**Finding X1: `fuel_wanted()` reads 0 after a restore.** The one field
that differs, in all 18 chains, is `fuel_wanted()` read after the fuel park
in front of `7 ** 100000`. A single process reads 1,373,355 there. A chain
reads it in the next hop, after restoring the image, and gets 0 [M]:

- `crates/mm-engine/src/snapshot.rs:1506` builds the restored VM with
  `fuel_wanted: 0`.
- The images still park in the right place, and the operation then runs on
  resume, as the docs say.
- The docs (docs/sdk.md "Bound CPU with fuel slices",
  docs/research/wasm-embedding.md "Bounding CPU") tell a host to read
  `fuel_wanted()` before granting more fuel. They do not say that the
  value is lost across a snapshot. So a durable host that saves on a fuel
  park and decides after a restore grants a large operation without seeing
  its cost. The cost here is 1.37M fuel; up to 2^36-bit results are
  allowed.
- This is the same on native and both guests and on both machines, so it
  is engine state, not wasm or architecture.

Not fixed here. The options were to save the value in the image (a format
change) or to document "read it before you snapshot".

**X1 is fixed in 220a05f**, which saves `fuel_wanted` in the image. I
verified this on the Mac at 45f454b and at 874409b [M]. The same
`iso-harness xchain` chains (one engine at a time, plus mixed
native → abi → abi-uu and abi-uu → abi → native, each hop a new process)
now read 1,373,355 after the restore, as a single process does. Every
report, `wanted` included, is identical to the single-process reference on
all three engines.

## 2. Own cases through `mm exec`/`mm resume` (`own-*.jsonl`)

Same method as `../2026-11-05-m1max-aarch64-macos/crossarch.py`. Each of
the 183 own cases is parked about 6 times (`--snapshot-every` =
fuel/6, at least 5), starting once on each machine, and the final report
must equal an unparked native run on the Mac. `/private/tmp` is normalised
to `/tmp`. The wasm hops run `mm.cwasm` under `wasmtime run`, with the
scratch directory preopened at its own path, as
conformance/oracle.py's `wasm_command` does.

| Engines per hop | Chains (183 cases × 2 start machines) | Identical to the unparked run [M] | Parks = restores on the other machine [M] |
|---|---|---|---|
| native `mm` on both | 366 | 366 | 2,132 |
| `mm` wasm32-wasip1 under wasmtime on both | 366 | 366 | 2,132 |
| mixed: hop k is wasm when ⌊k/2⌋ is odd, so each machine runs both | 366 | 366 | 2,132 (292 chains reach all four machine/engine pairs) |

In each row, 30 chains (15 cases from each start) ran in one hop because
they end before the first park. Each case parks at most 30 times. All three rows finished between
17:04:50Z and 17:32:28Z, including the host-API chains.

## Reproducing

```bash
# both machines, at the same commit: mm, mm.cwasm, both guests, iso-harness
cargo build --release --locked -p mm-cli
cargo build --release --locked -p mm-cli --target wasm32-wasip1
wasmtime compile target/wasm32-wasip1/release/mm.wasm -o target/wasm32-wasip1/release/mm.cwasm
for t in wasm32-wasip1 wasm32-unknown-unknown; do
  CARGO_TARGET_DIR=$PWD/target/isolation cargo build --release --locked -p mm-wasm-guest --features test-hooks --target $t
done
(cd bench/evidence/isolation && CARGO_TARGET_DIR=$PWD/../../../target/isolation cargo build --release --locked -p iso-harness)
# on the Mac, with MM, MM_WASM, WASMTIME, HARNESS, ISO_ROOT and the R_* equivalents for host1 set
python3.14 crossarch.py xchain xchain.jsonl
for e in native wasip1 mixed; do python3.14 crossarch.py own $e own-$e.jsonl; done
```
