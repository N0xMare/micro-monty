# mm-wasm-host (JavaScript)

A JavaScript host for the `mm-wasm` guest (`crates/mm-wasm-guest`, as
`cargo xtask size` ships it), for V8 runtimes: Node, and Cloudflare Workers
and Durable Objects (workerd). It follows the reference wasmtime host
(`crates/mm-wasm-host`) call for call, and its tests run that host's scenarios
on the shipped guest. docs/user-guide.md covers embedding the guest in
general.

No build step: the sources are TypeScript with erasable syntax only, so Node
(≥ 23.6) runs them as they are and any bundler (Wrangler's esbuild) strips
the types. Nothing here is published; `package.json` is private.

| File | What |
|---|---|
| `src/abi.ts` | The codec (`crates/mm-wasm-abi`): names, codes, values, the options record, modules, resumes, progress and errors. Decoders refuse unknown tags. |
| `src/host.ts` | `Runtime` (loaded builds, keyed by build id) and `Session` (one instance each, or K per instance as an opt-in), mirroring `mm_wasm_host`. |
| `src/driver.ts` | The async code-mode loop: async tools as futures, fan-outs answered in batches, fuel slices with yields and a budget. |
| `test/host.test.ts` | The reference host's scenarios (`parity.rs`, `upgrade.rs`) with absolute checks. |
| `scripts/build-guests.py` | The shipped guest plus the `core35` and no-stdlib builds (docs/user-guide.md's lists). |
| `bench/` | Measurements in Node and workerd against pi's codemode on quickjs-wasi; a run writes `bench/results/<date>/` (the earlier runs' data is at `da1c21b5`, and on the `evidence` branch of the private archive `N0xMare/micro-monty-archive`). |

## Use

```ts
import { Runtime, sessionOptions } from "./src/host.ts";
import { run } from "./src/driver.ts";

// Node: bytes. Workers: `import guest from "./mm_wasm_guest.wasm"` (a compiled
// WebAssembly.Module; Workers cannot compile bytes at run time).
const rt = await Runtime.create(bytes);
const s = rt.session(sessionOptions({ limits: { memory: 32 << 20 } }));
s.setOutput((stream, text) => console.log(text));
s.define({
	name: "",
	functions: [{ name: "get_order", params: [{ name: "order_id", kind: "keyword-only", default: null }], dictAsKwargs: true }],
});
const r = await run(s, "import asyncio\nxs = await asyncio.gather(*(get_order(order_id=i) for i in range(3)))\nlen(xs)", {
	tools: { get_order: async (c) => ({ id: c.args[0] }) },
	fuelSlice: 10_000, // yield to the event loop every 10k fuel
	fuelBudget: 5_000_000, // stop the cell (before a big native operation) past this
});
const image = s.snapshot(); // park a stopped session: store the image, drop the session
const again = rt.restore(image); // later, in any isolate with the same build loaded
```

A restore takes the session's host-side settings from its options as a new
session does (the limits, the clock, entropy and zone switches, `textCut`,
`intBitsText`, `gcStress`): pass the options the parked session had. The
text limit, `lastExprValue` and `hostArgs` travel in the image.

The low-level API is the reference host's: `define`, `start`, `resume`
(`{ kind: "value" | "error" | "future" | "batch" | "continue" }`), `cancel`,
`reset`, `snapshot`, `pending`, `phase`, fuel (`setFuel`, `fuelWanted`,
`setFuelTotal`), `setDeadline`, `requestPark`, hints, text cut, time zones,
call log, stubs, host modules, module dunders and the rest. Errors are
`MmError` with `kind` `trap` (the instance is gone: restore the last image),
`compile`, `resume`, `py`, `define`, `snapshot`, `abi` or `unknown-build`.

### K sessions per instance

By default every session has a wasm instance of its own, as the reference
host does: a trap loses one session. `RuntimeConfig.sessionsPerInstance`
(K, default 1) lets one instance hold up to K sessions. A new or restored
session takes a free slot in a live instance of its build, and a new instance
starts only when none has one. An instance is let go when its last session is
disposed, and `rt.stats()` reports live instances, sessions and linear memory.

```ts
const rt = await Runtime.create(bytes, { sessionsPerInstance: 16 });
```

What it buys: the instance's fixed part (the 832 KiB shadow stack, the data
segments, the allocator's slack) is paid once per K sessions. An idle session
costs about 1.57 MB of linear memory alone in an instance and about 0.1 MB as
one more session in a shared one [M]. What it costs:

- **A trap loses all K sessions.** A host callback that throws, a guest
  bug or a stack overflow ends the instance; every session in it then reports
  `isDead` and throws `MmError` `trap`. Sessions in other instances go on.
- **The instance's memory must hold the sum of its sessions' bounds**, and it
  never shrinks while any of them lives. Per session that is about
  3 × `limits.memory` + 16 MiB at worst (docs/user-guide.md), so K × that must fit
  under wasm32's 4 GiB, and under the isolate's 128 MB on Workers: K sessions
  of 16 MiB need up to K × 64 MiB in the worst case, against about K × 0.2 MB
  idle.
- Sessions of one instance run one at a time, which JavaScript does anyway.
  Output, time zones, park requests and deadlines stay per session: the guest
  passes the session id to every import.

Values follow Pyodide's mapping: `int` is a `number` when safe, else a
`bigint`; an integral `float` decodes as `Float` (so `2.0` stays a float);
`dict` decodes as a `Map` (Python's order and keys) and encodes from a `Map` or
a plain object; `tuple` is `Tuple`. `toJs()` turns a decoded value into plain
JSON-like data.

### Which guest

Two guest modules are shipped (docs/spec.md §10.3): the default `mm_wasm_guest.wasm` and the bundle guest
`mm_wasm_guest_bundle.wasm`. This host runs both. The bundle guest keeps the
stdlib and prelude bytecode out of its data segments, in the custom sections
`mm.prelude` and `mm.stdlib`, which this host reads once per module
(`WebAssembly.Module.customSections`) and serves through the import
`mm_host.bundle_read`. V8 copies data segments into every instance, so the
bundle guest takes about 300–340 KiB less RSS per idle session in Node and
workerd [M]. A host without `mm_host.bundle_read` must use the default guest:
the bundle guest fails to instantiate there. Snapshots cross between the two
(same build id). `npm run guests` writes both to `guests/` (`default.wasm`,
`bundle.wasm`); `MM_GUEST=bundle npm test` runs the tests on the bundle guest.

### Differences from the wasmtime host

V8 has no epoch interruption and no store limits, so:

- **No grace interrupt.** A park request (`requestPark()`, or a shared
  `Int32Array` another thread sets, `RuntimeConfig.parkSignal`) parks at the
  next safepoint check without a trap, as under wasmtime; one native operation
  that reaches no safepoint cannot be stopped from JavaScript. Bound CPU with
  fuel slices and `fuelWanted()` (the driver does), and rely on the platform as
  the last resort (a Node worker's `terminate()`, the Workers CPU limit).
- **Snapshot and restore cannot park.** A snapshot collects in full first:
  about 1.4 ms per MiB of image on V8, as natively and under wasmtime (about
  250 ms at 170 MiB) [M]. A park request or deadline does not stop one, and a
  host's own timeout must not cover a `snapshot()` or `restore()` call: cutting
  one loses the instance and the image. On Workers the platform's CPU limit is
  the remaining bound, so leave room for the largest image a session can make.
- **Deadlines** read `performance.now()`. On deployed Workers that clock stands
  still while JavaScript or wasm runs synchronously, so a deadline only fires
  after I/O; use fuel there. Local workerd's clock does advance.
- **Pointers are read as `u32`.** V8 passes every wasm `i32` to JavaScript as
  a signed number, so the host reads each pointer, length and offset the guest
  hands over with `>>> 0`; otherwise one at or above 2 GiB of linear memory
  would index the wrong bytes. Only a single Node instance can get that large
  (K sessions, or a limit over about 700 MiB); a Workers isolate cannot.
- **Memory** is capped by the session's `limits.memory` (the arena); linear
  memory grows a little past it and never shrinks while the instance lives.

## Tests

```bash
npm install
npm run guests        # cargo xtask size, then the core35 and no-stdlib builds (worktrees)
npm test              # node --test
npm run typecheck     # tsc (TypeScript 7)
```

## Measurements

```bash
# pi's codemode at the harness's pin, sharing this package's quickjs-wasi 3.6.2:
git clone https://github.com/earendil-works/pi /tmp/eng/pi && git -C /tmp/eng/pi checkout 7d073328
mkdir -p /tmp/eng/pi/packages/codemode/node_modules
ln -s "$PWD/node_modules/quickjs-wasi" /tmp/eng/pi/packages/codemode/node_modules/quickjs-wasi
python3.14 -I bench/run.py --rounds 5     # about 30 minutes; bench/results/<date>/
node --expose-gc bench/node.ts workload mm:default 5    # one measurement
```

`bench/node.ts` runs one measurement per Node process; `bench/worker/` is the
same measurements as a Worker for `workerd serve` (one process per engine and
round), plus a Durable Object that parks a cell in SQLite storage, survives a
killed workerd, and resumes it in a new process. `bench/wall.ts` is the V8
half of `cargo xtask wasm-wall`: the repository's bench programs on one guest
under `node --no-liftoff`.
