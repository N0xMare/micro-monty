# A supported wasm embedding for micro-monty: `mm-wasm`

Commit hashes of micro-monty cited here, other than the public history's
(root `2bcbb8c`), are pre-release history, kept in the private archive
`N0xMare/micro-monty-archive` (branch `history-0.0.1`;
[docs/history](../history/README.md) maps them); the numbers they qualify
are unchanged.

**Frozen with micro-monty 0.0.1** (owner decisions; docs/HANDOFF.md scope
decisions 24, 51 and 68). A host checks `ABI_VERSION` (1) and the required
export list (`mm_wasm_abi::export::REQUIRED`) before it runs anything, and
refuses a guest of another version. Two kinds of change keep the version:

- **A new session option:** a new tag in the options record
  ([Session options](#session-options)) whose default keeps the behaviour
  without it. Its tag says whether a guest that does not know it may skip
  it (tags from `0x80`) or must refuse the record. Only an option a guest
  can safely run without, a hint or a diagnostic, may take an ignorable
  tag; one that limits or changes behaviour (a limit, the clock, entropy,
  output, the text cut) takes a strict tag, so a guest that predates it
  refuses the record rather than run without it.
- **A new optional export:** one a host looks for and works without, as
  `mm_set_gc_yield` and `mm_test_panic`.

Anything else (a new or changed import, a changed signature or meaning, a
new required export, a changed code or encoding) bumps `ABI_VERSION`, and
`crates/mm-wasm-host` and `hosts/js` change with it. The snapshot format is
separate: images carry the engine's build id, and `mm_build_id` tells a
host which images a guest restores.

The other encodings are closed. Every decoder refuses an unknown tag (park
reason, phase, resume, error tags) with `Bad`. The facade's enums are
`#[non_exhaustive]`; an encoder writes the reserved tag `0xff` for a
variant its version has no code for, and every decoder refuses `0xff`, so
a new variant is a version bump, never a silent misdecode.

The interface carries the facade's host API: `HostSignature::dict_as_kwargs`,
`HostError { data, class }`, `HostModule::exceptions`, `Session::set_hint`,
`Session::environment` (the session's limits and fuel total),
`Session::call_log`, the text cut, `Session::fuel_wanted`, the fuel total,
`Session::phase`, `Session::pending`, `Session::stubs`,
`Session::set_local_timezone`, the zoneinfo zone source (the
`mm_host.zone` import), `Session::host_modules`,
`Session::set_module_dunders`, `int_bits_text` and GC stress, and the
facade's typed errors (`DefineError`, the phase of
`ResumeError::Unexpected`, the format and build id of
`SnapshotError::Incompatible`, `SnapshotError::Internal`).

### Two guests: the default guest and the bundle guest (owner, 2026-11-09)

Two v2 guest modules are built and shipped. Both have the same exports and
the same build id, so a snapshot written by one restores on the other. They
differ in where the prelude's and the bundled modules' packed bytecode
lives:

| Guest | File (`cargo xtask size`) | Bytecode | Imports | Hosts |
|---|---|---|---|---|
| **default** | `target/wasm32-unknown-unknown/shipped/mm_wasm_guest.wasm` | in data segments: copied into every instance's linear memory | the six of the table below | every v2 host |
| **bundle** | `target/mm-wasm-bundle/wasm32-unknown-unknown/shipped/mm_wasm_guest_bundle.wasm` (`mm-wasm-guest` with `host-bundle`) | in two custom sections of the module, `mm.prelude` and `mm.stdlib` (`mm_wasm_abi::section`), which wasm never copies into linear memory | the six, plus `mm_host.bundle_read` | hosts that serve `bundle_read`: `crates/mm-wasm-host` and `hosts/js` do |

`bundle_read(region: u32, offset: u32, ptr: u32, len: u32) -> u32` copies
`len` bytes at `offset` of region 0 (`mm.prelude`) or 1 (`mm.stdlib`) to
guest memory at `ptr` and returns 1, or 0 if the range is out of bounds.
The guest calls it the first time a session needs a prelude chunk (at most
about 2 KB packed) or imports a bundled module (its packed code, at most
about 28 KB). A host reads the two sections once per loaded module, for
example with `WebAssembly.Module.customSections` or by walking the module's
sections as the reference host does, and shares them between instances.

**A host without `mm_host.bundle_read` must use the default guest.** Wasm
imports are not optional: such a host fails to instantiate the bundle
guest with a link error, before any session exists. A host that provides
the import refuses to load a module that imports it without both sections
(both reference hosts do, at `load`); there is no fallback.
The default guest declares no new import, so every v2 host keeps working.
`ABI_VERSION` stays 1.

What the bundle guest buys is memory per instance, not file size [M,
D+F+E on cf4aa86]: its data segments are 120,278 B against the default
guest's 450,526 B, and its initial linear memory 18 pages against 23. The
file is 680 B larger. V8 (Node, workerd) copies data segments into each
instance eagerly, so RSS per idle session falls by about 300–340 KiB there;
wasmtime maps them copy-on-write, so it saves little RSS but the same
linear memory. Session creation is faster on V8, and a module's first
import costs one host call and one copy. The measurements are in
`bench/evidence/host-bundle/README.md`.

The owner wants native in-process as the default way to host micro-monty and
wasmtime in-process as a supported option per tenant or profile
([isolation-modes.md](isolation-modes.md)). That needs a stable guest
interface: before it, the only wasm embedding was the isolation harness's
`iso_guest.wasm`, whose `mm_in`/`mm_call` exports carry ad hoc JSON. This note
specifies `mm-wasm`, the guest and the reference wasmtime host that
implement it, the park-without-trap mechanism, the measurements, and where the
code lives. Numbers are labelled [M] measured, [S] sourced or [E] estimated.
It was prototyped under `bench/evidence/isolation/wasm/` and now lives in
[`crates/mm-wasm-abi`](../../crates/mm-wasm-abi) and
[`crates/mm-wasm-guest`](../../crates/mm-wasm-guest) (root workspace) and
[`crates/mm-wasm-host`](../../crates/mm-wasm-host) (its own workspace), with
a TypeScript host in [`hosts/js`](../../hosts/js) (workers-v8.md). No engine
crate or facade code changed for it.

**Placement and defaults (owner decisions, 2026-11-05, docs/HANDOFF.md
"Scope decisions" 17–25):** placed as proposed below; the park poll stays in
the deadline clock (no facade hook); the reference host defaults to a 100 ms
grace period and no call budget; one session per instance; an async
wasmtime host is plak's work.

`cargo xtask wasm-host` (not in `cargo xtask check`, so wasmtime stays out of
the main gate) builds the shipped guest for both targets and runs
`crates/mm-wasm-host`'s clippy and release tests against each; AGENTS.md
requires it before any push that touches the wasm crates, the facade API or
the snapshot format.

## Summary

- **Interface (frozen):** 31 required functions plus `memory`, the optional
  `mm_set_gc_yield` and `mm_test_panic`, and 6 imports (`mm_host`). Binary encoding, no
  serde. It covers sessions with an options record, `define`, start/resume
  with sync calls, futures and batches, completion, errors, snapshot/restore,
  park, cancel, reset, fuel and deadlines. One source builds for wasip1 and
  unknown-unknown.
- **Park without a trap works, with no engine change.** wasmtime's epoch
  callback cannot write guest memory, so the guest polls a host import at
  the engine's existing safepoint check. A park request stops a Python loop
  in 0.03 ms with the instance alive, against 0.10 ms *and a lost instance*
  with the epoch trap. It costs about 1% on the bench programs (2.10× native
  against 2.08× for the epoch-trap guest at ac10b28) [M]. The epoch
  interrupt stays as the fallback, 100 ms after the request (the grace
  period), for a single native operation that reaches no safepoint.
- **wasm32-unknown-unknown performs like wasip1** (2.06× native, host call
  0.98 µs, the same fault behaviour) and is **2,015,061 bytes shipped after
  `wasm-opt -Oz`**, 92,495 bytes (4.8%) more than `hello.wasm` at the same
  commit [M]. Snapshots move between native, wasip1 and unknown-unknown in
  every direction.
- **Placement (done):** the abi and guest crates are in the workspace (no new
  dependencies; the root lockfile gains only their two path packages; a spec
  §5 exception for the import block and the export attributes). The wasmtime
  host is outside (about 187 external crates). The shipped unknown-unknown
  guest has its own size budget in `cargo xtask size` (`mm_wasm_guest.wasm`
  in `xtask/size-ceilings.txt`, with its measurements).

## The interface

One module, built from the same source for `wasm32-wasip1` and
`wasm32-unknown-unknown`. It exports `memory` and the functions below, imports
six functions from module `mm_host`, and nothing else on unknown-unknown.
The wasip1 build also imports std's start-up WASI functions (`fd_write`,
`environ_*`, `proc_exit`, and `clock_time_get` and `random_get` for std's own
defaults, which the guest replaces with its imports before any cell runs).
One instance can hold several sessions (`u32` ids). The reference host uses
one session per instance, so a trap loses one session; `hosts/js` offers K
sessions per instance as an opt-in (workers-v8.md §7).

**Calling convention.** Variable-size data crosses through two buffers the
guest owns. The host calls `mm_input(len)`, writes `len` bytes at the
returned address, then calls an export. The export leaves its payload in the
output buffer, which `mm_output()` locates. The guest reads both buffers with
safe Rust (they are `Vec<u8>`s the host overwrote), so the only `unsafe`
syntax in the guest is the import block and the `#[unsafe(no_mangle)]`
attributes.

### Exports

| Export | Signature | Input → output |
|---|---|---|
| `mm_abi_version` | `() -> u32` | returns 1; a host refuses other versions |
| `mm_build_id` | `() -> u32` | → the 32-byte snapshot build id |
| `mm_input` | `(len: u32) -> u32` | sizes the input buffer, returns its address |
| `mm_output` | `() -> u64` | the output buffer as `address << 32 \| length` |
| `mm_session_new` | `() -> i32` | the options record → id ≥ 0; `-1` and a `PyException`; `-2` and a bad-request string |
| `mm_session_restore` | `() -> i32` | the options record, then the image → id; `-1` and a `SnapshotError`; `-2` |
| `mm_session_drop` | `(s) -> u32` | drops the session and its heap |
| `mm_define` | `(s) -> u32` | a `HostModule` → its `HostFunctionRef`s, or a `DefineError` (0 invalid + text, 1 busy, 2 raised + `PyException`, 3 poisoned + text) |
| `mm_start` | `(s) -> u32` event | filename, source → compiles and starts the cell |
| `mm_resume` | `(s) -> u32` event | a `Resume` → the next stop |
| `mm_cancel` | `(s) -> u32` | `Session::cancel`; 1 if a cell was stopped |
| `mm_reset` | `(s) -> u32` | `Session::reset`; a `PyException` on failure |
| `mm_snapshot` | `(s, max_bytes: u64) -> u32` | → the FULL image, or a `SnapshotError`; `u64::MAX` keeps the default cap |
| `mm_set_fuel` | `(s, fuel: u64) -> u32` | `Session::set_fuel`; `u64::MAX` is unlimited |
| `mm_fuel` | `(s) -> u64` | `Session::fuel` |
| `mm_set_deadline` | `(s, ns: u64) -> u32` | one-shot deadline on `mm_host.now_ns`; `u64::MAX` clears it |
| `mm_fuel_wanted` | `(s) -> u64` | `Session::fuel_wanted` (check it before granting another fuel slice) |
| `mm_set_hint` | `(s) -> u32` | hint group key, optional text → `Session::set_hint`; a `PyException` on failure |
| `mm_set_text_cut` | `(s, cut: u32) -> u32` | `Session::set_text_cut` (0 `Head`, 1 `HeadTail`); also an option |
| `mm_set_local_timezone` | `(s) -> u32` | optional TZif bytes → `Session::set_local_timezone` (session state) |
| `mm_call_log` | `(s) -> u32` | → `Session::call_log`: call id, function id, name, status (0 returned, 1 raised, 2 pending, 3 abandoned) |
| `mm_environment` | `(s) -> u32` | → `Session::environment`: the session's limits and fuel total |
| `mm_set_fuel_total` | `(s, total: u64) -> u32` | `Session::set_fuel_total`; `u64::MAX` is no cap |
| `mm_fuel_total` | `(s) -> u64` | `Session::fuel_total`; `u64::MAX` is no cap |
| `mm_phase` | `(s) -> u32` | `Session::phase` (0 idle, 1 host call, 2 waiting, 3 parked) |
| `mm_stubs` | `(s) -> u32` | → `Session::stubs` as (module, text) pairs |
| `mm_host_modules` | `(s) -> u32` | → `Session::host_modules`: a count, then each module as `mm_define` takes it |
| `mm_set_module_dunders` | `(s) -> u32` | optional `__name__`, optional `__file__` → `Session::set_module_dunders`; a `PyException` on failure |
| `mm_set_int_bits_text` | `(s, on: u32) -> u32` | `Session::set_int_bits_text`; also an option |
| `mm_set_gc_stress` | `(s, on: u32) -> u32` | `Session::set_gc_stress` (engine testing); also an option |
| `mm_set_gc_yield` | `(s, on: u32) -> u32` | optional (2026-11-11): `Session::set_gc_yield`, a collection step ends a fuel slice too |
| `mm_pending` | `(s) -> u32` event | `Session::pending`: the stop as the event that reported it (`HOST_CALL`, `WAITING`, `PARKED`), or `COMPLETE` without a completion between cells |
| `mm_test_panic` | `(op: u32) -> u32` | optional, only with the `test-hooks` feature: arms an engine panic |

The guest also exports its `__stack_pointer` global (2026-11-09, optional):
its value before the first call is the top of the 832 KiB shadow stack, so
a test host can paint the stack and measure its deepest use
(`RuntimeConfig::paint_stack`). Hosts must not write it.

`mm_wasm_abi::export::REQUIRED` lists every required export; the reference
host refuses a module that lacks one before instantiating it.

Status codes: `0` OK, `1` `PyException`, `2` `SnapshotError`, `3`
`DefineError`, `7` bad request.
Events from `mm_start`/`mm_resume`: `0` Complete, `1` HostCall, `2` Waiting,
`3` Parked, `4` Error (uncaught exception), `5` CompileError, `6` ResumeError,
`7` bad request. Each event's payload mirrors the facade type of the same
name, so a host can rebuild `micro_monty::Progress` exactly
(`mm_wasm_abi::get_progress`).

### Session options

`mm_session_new` takes an options record, and `mm_session_restore` one
followed by the image (`mm_wasm_abi::put_options`). The record is a `u32`
byte length, then entries in any order, each a tag byte, a `u32` payload
length and the payload. An absent entry is the option's default, and
`put_options` writes only the entries that differ from it: an empty record
(`00 00 00 00`) is the defaults, and a guest that predates an option sees
its entry only when a host uses it. The guest refuses (`-2`, bad request) a
record with a repeated tag, a payload that is not exactly its tag's
encoding, or a tag it does not know below `0x80`, and skips unknown tags
from `0x80` (none is assigned). A tag's payload and default never change;
a new option is a new tag. Tags from `0x80` are only for options a guest
can safely run without (hints, diagnostics); every option that limits or
changes behaviour, as all of those below do, takes a strict tag.

| Tag | Option | Payload | Default | Restore |
|---|---|---|---|---|
| `0x01` | `Limits::memory` | `u64` | 256 MiB | the smaller of it and the image's |
| `0x02` | `Limits::recursion_depth` | `u32` | 1000 | the image's |
| `0x03` | `Limits::fuel` | `u64`, `u64::MAX` unlimited | unlimited | the budget of later cells |
| `0x04` | `Session::set_text_limit` | `u64` | 1 MiB, or the memory limit if less | the image's |
| `0x05` | `Session::set_last_expr_value` | bool | on | the image's |
| `0x06` | `HostArgs::max_bytes` | `u64` | 16 MiB | the image's |
| `0x07` | `HostArgs::max_depth` | `u32` | 1000 | the image's |
| `0x08` | `HostArgs::lenient` | bool | off | the image's |
| `0x09` | `mm_host.wall_clock` as the wall clock | bool | on | applies |
| `0x0a` | `mm_host.entropy` as entropy | bool | on | applies |
| `0x0b` | `mm_host.zone` as the zone source | bool | on | applies |
| `0x0c` | `Session::set_text_cut` | `u8`: 0 head, 1 head-tail | head | applies |
| `0x0d` | `Session::set_int_bits_text` | bool | off | applies |
| `0x0e` | `Session::set_gc_stress` | bool | off | applies |

The limits apply on a restore as `Engine::restore` applies the restoring
engine's. The last six are what an image does not carry
(`micro_monty::SessionSettings` natively), so a host that restores with the
options the session was created with gets a session that behaves as the
saved one did.

### Imports (`mm_host`)

| Import | Signature | Used for |
|---|---|---|
| `now_ns` | `(s) -> i64` | the host's monotonic clock: deadlines |
| `park` | `(s, clear: u32) -> u32` | whether a park of `s` is requested; `clear` takes it |
| `wall_clock` | `(s) -> f64` | `time.time()`, `datetime.now()` (seconds since the epoch) |
| `entropy` | `(s, ptr, len)` | `os.urandom`, unseeded `random`, `uuid4` |
| `output` | `(s, stream: u32, ptr, len)` | `print`, `sys.stdout` (0) and `sys.stderr` (1), in production order |
| `zone` | `(s, key_ptr, key_len, buf_ptr, cap) -> i64` | `zoneinfo.ZoneInfo(key)`: `-1` unknown, else the TZif length, written when it fits in `cap`; the guest asks with 4 KiB and once more at the reported length, so the host answers both calls alike (the reference host caches the last key) |
| `bundle_read` | `(region, offset, ptr, len) -> u32` | the bundle guest only ("Two guests" above): its prelude and stdlib bytecode, from the module's custom sections |

These are exactly what sdk.md says unknown-unknown lacks (clock, entropy),
plus output and the park channel. Session options can switch the wall clock
and entropy off (`time.time()` then raises `RuntimeError`, as on a native
session with `set_wall_clock(None)`).

### Encoding

Hand-written little-endian, no serde (`mm_wasm_abi`, about 1,300 lines shared by
guest and host). Strings, byte strings and sequences have a `u32` length.
A `HostValue` is a tag byte (0 `None`, 1/2 `False`/`True`, 3 `int` as `i64`,
4 big int as decimal text, 5 `float` bits, 6 `str`, 7 `bytes`, 8 `list`,
9 `tuple`, 10 `dict`, 11 record, 12 host-class object, 13–16 the four
`Unconverted` forms) and its payload. Decoding never panics: counts are
checked against the remaining input, each container preallocates at most
1,024 items (every nesting level checks its count against the same remaining
bytes), and nesting stops at 1000, plus the `TooDeep` marker the engine's
lenient conversion puts one level below; dict, record and object fields
count one level, as list items do. An optional item is a `0`/`1` byte, then
the item. Session options are a tagged record (above). A signature ends with
its `dict_as_kwargs` byte; a module ends with its exception classes (name,
the base's `HostErrorKind` name, doc); an exception is type name, message,
traceback and its `truncated` byte; a host error is kind, message, optional
`data` value and optional `class` name. `crates/mm-wasm-abi/tests/codec.rs`
round-trips these and checks that every truncation is refused, and that the
options record refuses what it must and skips only ignorable tags.

### What is deliberately not in the interface

- No host callbacks during a call other than the six imports: host calls are
  stops (`HostCall`, `Waiting`), as in the facade, so a host never re-enters
  the guest.
- No templates (`SessionTemplate`): a template is a snapshot, so
  `mm_session_restore` covers it.
- No `SessionTemplate` export, `set_output` without streams (the host
  merges the two streams itself), or `set_deadline` on `Instant` (the
  deadline is on `mm_host.now_ns`). Later facade features arrive as session
  options or optional exports under the rule above.

## Park without a trap

The ask was to export the park flag's address and have the epoch deadline
callback write it through the store. wasmtime 50 does not allow that: the
callback receives a `StoreHookState`, which "ensures that store data can be
read, embedder-specific data can be mutated, but nothing within the store
otherwise can be mutated" [S: `wasmtime/src/runtime/store/context.rs`].
Linear memory is store state, so the callback cannot set a byte in it.

The equivalent that works needs no engine change: **the guest asks the host.**
Before every `start`/`resume` the guest installs a deadline clock
(`Session::set_deadline_with`) that calls `mm_host.park(s, 0)`. The engine
already polls its deadline clock at every safepoint check (every 256
safepoints, `TICKS`, next to the park flag), so a park request reaches the
cell at the same point a native `ParkHandle::request` would. The clock then
reports "past the deadline", the engine parks with `Deadline`, and the guest
reports `Parked { reason: Requested }` (it asks `park(s, 1)` to tell the two
apart and to clear the request). A real host deadline is kept beside it: the
clock returns `now_ns` unless a park is pending.

The epoch callback stays as the last resort. On each tick (10 ms) it
interrupts the instance only if a requested park is older than the grace
period (100 ms by default), which means the cell is inside one long native
operation that reaches no safepoint, or if the call has run past its optional
budget. Otherwise it returns `Continue(1)`.

| Stop (runaway `while True: pass`, request at 200 ms) [M, ac10b28] | wasip1 | unknown-unknown | old guest (epoch trap) |
|---|---|---|---|
| park request → `Parked{Requested}`, instance alive, session reusable | 0.033 ms | 0.029 ms | — |
| epoch interrupt (instance lost) | — | — | 0.13 ms |
| park request during one ~7 s bigint operation | grace interrupt 100.7 ms after the request | 100.8 ms | 0.13 ms |

| Bench geomean vs native (12 programs, fresh session each) [M] | at 58d7aca | at ac10b28 |
|---|---|---|
| old guest: epoch trap, JSON transport (`wasm`) | 2.14× | 2.08× |
| mm-wasm wasip1 (`abi`): park poll + epoch fallback | 2.17× | 2.10× |
| mm-wasm unknown-unknown (`abi-uu`) | 2.16× | 2.06× |
| mm-wasm unknown-unknown, shipped and `wasm-opt -Oz` | 2.10× | 2.05× |
| no interruption at all (`wasm-plain`) | 1.79× | 1.76× |

The poll is one import call per 256 safepoints; the measured difference
from the epoch-trap guest (+0.8% to +1.2%) is about the size of the run's
noise. Epoch interruption itself (its checks in every loop) is the larger
cost: about 18% over no interruption [M].

A facade hook would make this tidier, not faster: a `Session::set_park_poll`
callback polled where the park flag is would avoid borrowing the deadline
slot and the `Deadline` → `Requested` remapping. The interface does not need
it, so no engine or facade code changed.

## Reference host (`mm-wasm-host`)

[`crates/mm-wasm-host`](../../crates/mm-wasm-host) wraps the interface in an API close to
`micro_monty::Session`:

```rust
let rt = Runtime::new(Path::new("mm_wasm_guest.wasm"), RuntimeConfig::default())?;
let mut s = rt.session(&SessionOptions::new().with_limits(limits))?;
s.set_output(Box::new(|stream, text| print!("{text}")));
let ids = s.define(&HostModule::untyped("", &["lookup"]))?;
let mut p = s.start("lookup('a') * 3", "cell.py")?;      // compile + start
while let Progress::HostCall { .. } = p {
    p = s.resume(&Resume::Value(HostValue::Int(2)))?;
}
s.set_zone_source(Some(Box::new(|key| std::fs::read(format!("/usr/share/zoneinfo/{key}")).ok())));
s.set_hint(HintGroup::Network, Some("Use the http tool."))?;
let log = s.call_log()?;                   // Vec<HostCallRecord>, as natively
let park = s.park_handle();                // Send + Clone: park.request() from any thread
let image = s.snapshot(None)?;             // Vec<u8>, same format as native
let s2 = rt.restore(&image, &options)?;    // or micro_monty::Engine::restore_with natively
```

`Runtime` holds the engine, the linked module (precompiled
`<stem>.<key>.mmw.cwasm` cached next to the `.wasm`, keyed by SHA-256 over
the module's bytes and wasmtime's `precompile_compatibility_hash`, and
loaded with `Module::deserialize_file` so wasmtime maps the file and builds
each instance's linear memory copy-on-write from it: loading from bytes left
macOS without a copy-on-write source, and every instance copied the 600 KB
data segment, about +470 KiB idle RSS each [M]; the key guards against a stale or mismatched cache,
not a hostile writer, so the directory must be writable only by the host),
and the epoch ticker. `Session` owns one store and one
instance. `Error::Trap` means the instance is gone: restore the last snapshot
into a new session. `Compile`, `Resume`, `Py` and `Snapshot` errors are the
facade's types, and `Abi` is an interface violation. The configuration is the
one chosen in isolation-modes.md: epoch interruption with a 10 ms tick, a
4 MiB `max_wasm_stack`, a store memory limit (784 MiB default: about
3 × the session's 256 MiB limit + 16 MiB, spec §11's memory bound), no wasmtime
fuel, and no pooling. A requested park that has not happened after
`park_grace` plus `park_grace_per_mib` for each MiB of the session's
memory limit (100 ms + 0.75 ms/MiB by default, 292 ms at 256 MiB,
configurable per host) interrupts the instance; the per-MiB part covers a
garbage collection, which cannot park and takes about 0.55 ms per live MiB
of small containers on the guest [M]; `call_budget` (off by default) is an
opt-in backstop for one export call. It is synchronous; an async host (wasmtime's `call_async`, or
`spawn_blocking` around this one) is plak's work. One `Runtime` holds several
guest modules keyed by build id, for the upgrade policy (docs/sdk.md,
"Upgrading: drain old builds"): `load` makes a module current for new
sessions, `restore` picks the module of the build that wrote the image
(`Error::UnknownBuild` when it is not loaded), `unload` retires a drained
build; `tests/upgrade.rs` covers it. `tests/parity.rs` runs scenarios
covering every export on a native session and on the guest and requires
identical transcripts, output, call logs, stubs, environment and
`fuel_wanted` values, then crosses the images both ways, restoring with the
host-side settings from the options record on the guest and from
`SessionSettings` natively (passes on both targets [M]; dropping
`HostError::class` in the decoder makes it fail).

**Bounding CPU.** Run cells in fuel slices and read `fuel_wanted()` on each
`Parked { reason: Fuel }` before granting more fuel: a big int operator that
needs more than was left parks *before* it starts, and a host enforcing a
budget stops the cell there instead of resuming. Natively that is the only
defence against one long native operation (a park request waits for it); in
wasm it keeps the grace interrupt, which loses the instance, rare.

## Measurements

All [M], 16-vCPU orb, wasmtime 50.0.1, release guests unless marked
(the shipped `wasm-opt` row has no `test-hooks`). Two full runs: at
`58d7aca` (before thread A's hardening) and at `ac10b28` (after). Raw data
is in `bench/evidence/isolation/results/2026-11-05-wasm-abi/`. The table
uses ac10b28; isolation-modes.md has the other modes' tables.

| | native | old wasm guest | mm-wasm wasip1 | mm-wasm unknown-unknown | child |
|---|---|---|---|---|---|
| Cold start, `main` → first cell done (precompiled) | 0.29 ms | 1.45 ms | 1.19 ms | 1.25 ms | 2.00 ms |
| New session (instance + session), median | 39 µs | 158 µs | 167 µs | 161 µs | 1,784 µs |
| Host call round trip | 0.22 µs | 2.54 µs | 1.02 µs | 0.98 µs | 54 µs |
| Codemode cell, 400 calls | 1.99 ms | 5.53 ms | 4.42 ms | 4.31 ms | 28.0 ms |
| Bench geomean vs native | 1.00× | 2.08× | 2.10× | 2.06× (2.05× shipped, wasm-opt) | 0.99× |
| Snapshot / restore, 56 KB codemode park (base run) | 62 / 111 µs | 621 / 699 µs | 596 / 737 µs | 554 / 716 µs | 598 / 2,095 µs |
| Snapshot / restore, 18 MB image (base run) | 25.5 / 19.7 ms | 67.5 / 48.6 ms | 63.9 / 48.7 ms | 63.3 / 49.1 ms | 71.1 / 45.0 ms |
| Host RSS per idle / active session | 74 / 386 KiB | 385 / 1,331 KiB | 349 / 1,299 KiB | 345 / 1,290 KiB | 333 / 805 KiB worker PSS |
| 1,024 sessions, cells/s (base run) | 1,502 | 1,819 | 1,614 | 1,917 | 533 |

The binary encoding more than halves the old guest's host-call cost (JSON
both ways). Images are 48–96 bytes smaller than the old guest's, because
the old service set a different text limit; they restore across all modes.

**Size** (shipped: std rebuilt with `panic=immediate-abort`, then
`wasm-opt -Oz` with rustc's features, as `cargo xtask size`):

| Module | before wasm-opt | after | vs `hello.wasm` at the same commit |
|---|---|---|---|
| `mm_wasm_guest.wasm`, wasm32-unknown-unknown, ac10b28 | 2,205,342 | **2,015,061** | +92,495 (1,922,566) |
| `mm_wasm_guest.wasm`, wasm32-wasip1, ac10b28 | 2,111,851 | 1,926,756 | — (not gated) |
| unknown-unknown, 58d7aca | 2,187,068 | 1,996,809 | +92,171 (1,904,638) |

The ~92 KB over `hello.wasm` is what the guest adds: the codec, the
seventeen exports of that prototype, keeping `define`/`reset`/`snapshot_with_limit`/
`set_host_args` and the error types live, and the per-session table. Why the
unknown-unknown module is 88 KB larger than wasip1's was not investigated
(both use Rust's dlmalloc; open question).

**Fault matrix on the new modes** (each row with a bystander session in the
same host; full rows in `faults.jsonl`):

| Fault | mm-wasm wasip1 and unknown-unknown (identical) |
|---|---|
| Cyclic dict `==`, 600-deep frozenset `==` | 58d7aca: instance traps at 512 KiB `max_wasm_stack`, fine from 1 MiB. ac10b28: `RecursionError`/`True` at every size |
| Engine panic (release, and shipped unknown-unknown) | instance traps (`unreachable`); host, bystander and fresh session fine |
| Runaway Python loop | fuel: parks; 10k-fuel slices: 0.08–0.11 ms; **park: 0.03 ms, no trap**; call budget 200 ms: traps at 210 ms |
| Runaway single bigint op | ac10b28: fuel parks *before* the op (0.05 ms); a park request waits for the grace period and traps at 100.7 ms |
| Arena limit (64 MiB) | `MemoryError` |
| Out-of-arena conversion (`[0] * 7_000_000`) | 58d7aca: traps at the 256 MiB store limit. ac10b28: completion converts within the limit; host-call argument raises `MemoryError`; session reusable |
| Snapshots | 25/25 pairs across native, old wasm, wasip1, unknown-unknown and child resumed with identical output |

## Where the code lives (owner decision 1)

| Piece | Proposal | Root lockfile | `cargo xtask size` | Other cost |
|---|---|---|---|---|
| `mm-wasm-abi` (codec and constants, ~1,100 lines) | **`crates/mm-wasm-abi`** in the workspace, publishable so plak's host can depend on the same version | two path packages added, no external ones | none | needs the workspace lints (`missing_docs`, clippy pedantic); it builds on every target |
| `mm-wasm-guest` (cdylib, ~550 lines) | **`crates/mm-wasm-guest`** in the workspace, `publish = false`; `cargo xtask size` builds and optimizes it beside `hello.wasm` | (as above) | its own budget beside `hello.wasm`, which stays primary: 2,223,853 B at 585c79f [M], ceiling 2,235,000 | the export attributes and the import block are `unsafe` syntax, so the crate needs a §5 exception like `mm-cell-worker`'s seccomp one (`unsafe_code = "deny"`, allowed only on the import block and `#[unsafe(no_mangle)]`); on host targets it builds with stub imports, so `cargo test --workspace` and clippy cover it |
| `mm-wasm-host` (wasmtime reference host, ~800 lines) | **`crates/mm-wasm-host`, its own workspace** (excluded from the root, own `Cargo.lock`, builds into `target/mm-wasm-host`); plak writes the tokio host | adding it to the root would bring **~187 external crates** (wasmtime alone is 137) into a lockfile of 104 packages | none either way | not in `cargo xtask check`; pins a wasmtime major version (the `.cwasm` cache is per version) |
| The isolation harness | stays where it is (`bench/evidence/isolation`) | — | — | depends on the host by path; `run.sh` builds the guest in the root workspace |

Alternatives considered:
- **One guest crate per target**: not needed; one source builds for both,
  and the only difference (WASI start-up imports) is std's.
- **The abi inside `micro-monty`** (a `wasm-abi` feature): avoids a crate,
  but puts a wire format into the facade's semver surface and its size
  build. A separate crate keeps the facade unchanged.
- **The component model / WIT**: richer types and generated bindings, but
  it needs `wit-bindgen` in the guest (a dependency engine crates avoid),
  the component toolchain, and wasmtime's component runtime on the host.
  Revisit if plak's host wants components for other tools.
- **Host in the root workspace behind a feature**: a feature does not keep
  packages out of `Cargo.lock`, so the lockfile cost is the same.

## Decisions (owner, 2026-11-05)

Recorded in docs/HANDOFF.md "Scope decisions" 17–25.

1. **Placement** as above; the §5 exception covers only the guest's import
   block and `#[unsafe(no_mangle)]`; the shipped unknown-unknown guest has its
   own budget and `hello.wasm` stays primary.
2. **Freeze:** the interface took the facade's host-API and stdlib
   features before it was frozen; it is frozen with 0.0.1 under the rule at
   the top (decisions 24, 51 and 68 in HANDOFF).
3. **Park channel:** the poll stays in the deadline clock; no facade hook.
4. **Defaults:** 100 ms grace period, configurable per host; the call budget
   is off by default, an opt-in backstop; hosts run cells in fuel slices and
   check `fuel_wanted()` (above).
5. **One session per instance.** The guest still accepts several ids; hosts
   do not use that by default. `hosts/js` has K sessions per instance as an
   opt-in (`sessionsPerInstance`, owner, 2026-11-09): about 0.11–0.14 MB per
   extra session instead of 1.57 MB per instance, for a trap that ends all
   K and a shared, never-shrinking peak (workers-v8.md §7).
6. **Async wasmtime** is plak's work.
7. **Why unknown-unknown is about 88 KB larger than wasip1** [M]: std's
   dlmalloc on unknown-unknown runs `validate_size` assertions that fat LTO
   inlines into every deallocation site (the constant they test appears
   1,687 times against 52 on wasip1); a custom allocator measured 1,925,300
   against 2,015,061 bytes at ac10b28, about 90 KB. `hello.wasm` pays roughly
   75 KB of the same [E]. Accepted as is at first (option (a)); the size
   pass (owner decision 41 in HANDOFF) solved it with `crates/mm-wasm-alloc`,
   a temporary spec §5 exception: −97,112 bytes of the guest and −83,553 of
   `hello.wasm` [M], after which the unknown-unknown guest (2,167,383) is
   smaller than the wasip1 one (2,169,244) [M].

**Engine caches on the guest (owner, 2026-11-07; HANDOFF scope decision
48).** The guest makes one `Engine` per session, so its decoded-module
cache only shares modules within that session. The prelude chunk cache
(docs/research/import-cost.md §5) is off on the guest
(`Engine::without_prelude_chunk_cache`): with one session per instance,
cached chunks would stay in linear memory for the instance's life, 192 KiB
for the five-module cell (3,584 → 3,392 KiB without them), for 50–180 µs
of that session's imports [M]. Lazy asyncio attributes apply on the guest
as everywhere.

## How to rerun

```bash
cargo xtask size                                 # gates the shipped unknown-unknown guest
cargo xtask wasm-host                            # shipped guests (both targets) against mm-wasm-host's tests
bash bench/evidence/isolation/run.sh OUT all     # builds both guests, size, perf and the fault matrix
ISO_MODES=abi,abi-uu target/isolation/release/iso-harness perf bench hostcall cross
target/isolation/release/iso-harness fault abi-uu runaway src=native guard=park
```

`run.sh` writes `size.txt` (shipped guests before and after `wasm-opt -Oz`)
next to `perf.jsonl` and `faults.jsonl`. The guests are `mm_wasm_guest.wasm`
under `target/isolation/<target>/release/` (with `test-hooks`, for the fault
matrix) and `target/isolation-size/<target>/release/` (shipped, without).
