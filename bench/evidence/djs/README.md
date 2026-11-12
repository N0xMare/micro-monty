# Pinned native djs comparator setup

Setup/smoke only, prepared 2026-11-04. No benchmark timings have been collected.
No djs source or existing `bench/*.js` fixture was changed. The executable already
supports those fixtures; this directory's external Rust adapter is only for the
compile-once warm probe and bounded persistence correctness probes.

## Revision and builds

Repository: <https://github.com/earendil-works/djs>.
Default branch `main` was fetched and checked against the remote at
`548ab4874a08d85f3a6f0b07feaa510df1aaf969`. The local clone is detached there.

```bash
mkdir -p /tmp/mm-evidence-djs
git clone --depth 1 --filter=blob:none --no-tags \
  https://github.com/earendil-works/djs.git /tmp/mm-evidence-djs/source
git -C /tmp/mm-evidence-djs/source checkout --detach \
  548ab4874a08d85f3a6f0b07feaa510df1aaf969

# From /tmp/mm-evidence-djs/source:
CARGO_TARGET_DIR=/tmp/mm-evidence-djs/target cargo +1.99.0 build \
  --release --locked -p djs-cli -p djs-core -p djs-prelude -j 2

# From the micro-monty root; creates an independent Cargo project under /tmp:
bash bench/evidence/djs/build.sh
```

Rust is explicitly pinned for both builds: stable `rustc 1.99.0
(b940084d7 2026-09-28)`, Cargo `1.99.0 (5f94df478 2026-08-27)`,
`x86_64-unknown-linux-gnu`, LLVM `23.1.1` [S]. The orb's default outside this
repository is a nightly, so **do not omit `+1.99.0`**. An initial nightly CLI
build was superseded by the stable build, and all fixture smokes were rerun.

The normal upstream release profile is `opt-level=3`, `debug=1`, not stripped;
no added LTO, PGO or `unsafe-heap`. Defaults include `full`, `unicode`, `dump`,
native builtin state machines and the hot interpreter loop. This is a native
CLI/library build, not the upstream size-oriented wasm profile. Only the native
build was prepared. `libm` is exactly `0.2.16`; the upstream lockfile SHA-256 is
`4bf373d4b355788b7aa7d9b17e946f6cf528d5b17e517aa47826ac743943bb73` [M].

Stable-build artifacts [M]:

| Artifact | Path under `/tmp/mm-evidence-djs/` | SHA-256 |
| --- | --- | --- |
| Native CLI | `target/release/djs` | `4af4a2d7f40e0d2607641b0ffa7828b6e952a955d3e77337326359277510f902` |
| Native core Rust library | `target/release/libdjs_core.rlib` | `cc7aefb1e5ba37c35d537b72f0efac4926d485a83206afd15fbce3cd9176c5c8` |
| Native prelude Rust library | `target/release/libdjs_prelude.rlib` | `3f274e9e46718f61cd807f4d4329141ffa5135a29bf82973224559673ac797b5` |
| External probe | `probe-target/release/djs-probe` | `6397c122b38fa53083b272fc7b9643a71b4b2102024c05c1d6c6058e4bc2d1fd` |

These are local artifact fingerprints, not promises of byte-identical builds
with different checkout paths or toolchains. CLI size is 29,478,840 bytes and
probe size 23,375,656 bytes, both including debug information [M]. The embedded
raw base image is 186,360 bytes [M], SHA-256
`54044a8827f265f0456a078be4c64125816c56ace7eab5dcf1f74644ff516bb0`.

## Existing workload invocation and checksum preservation

```bash
DJS=/tmp/mm-evidence-djs/target/release/djs
"$DJS" run /home/user/workspace/repo/bench/loop.js
# stdout: 4499998500000 followed by one newline

# Bounded smoke with stdout/stderr kept separate (no timing wrapper):
/tmp/mm-evidence-djs/source/scripts/timeout.sh 45 "$DJS" run \
  /home/user/workspace/repo/bench/sort.js \
  > /tmp/mm-evidence-djs/sort.stdout 2> /tmp/mm-evidence-djs/sort.stderr
```

Use **Script** mode: no `--body`, source wrapping or fixture rewriting. `--backend
native` is the default; `--backend bytecode` is a different comparator setting.
Do not benchmark through `cargo run`. The normal cold process boundary includes
CLI startup, file read, base-image copy, host setup, source compile, execution,
console host-call formatting and process exit; it is not just VM execution.

All existing JS workloads passed one stable-build, exact-byte checksum smoke
with empty stderr [M]: `startup`, `fib`, `loop`, `dict`, `str`, `json_rows`,
`closure`, `classes`, `generators`, `exceptions`, `fmt`, `sort`, `interp`.
The `json_rows.js` fixture's expected key is `json` in `bench/expected.txt`.
Whitespace matters, including `fmt`'s padding. No unsupported existing workload
was found. This is not a claim of general JavaScript conformance: upstream
explicitly excludes modules, sloppy mode, direct eval, Intl, Temporal and
SharedArrayBuffer/Atomics, among others.

Evidence is in `/tmp/mm-evidence-djs/smoke/`: original stdout/stderr per fixture
and `results.json` with exact commands, exit codes and byte-comparison verdicts.
`manifest.json` at the setup root records fixture and artifact hashes.

## Compile-once warm public-API probe

```bash
PROBE=/tmp/mm-evidence-djs/probe-target/release/djs-probe
/tmp/mm-evidence-djs/source/scripts/timeout.sh 15 "$PROBE" check 1000 3
# {"mode":"check-only","n":1000,"warm_repetitions":3,"checked_result":499500}

# Run ONLY later on an otherwise idle machine, not alongside compilation:
"$PROBE" warm 3000000 10
```

`check` executes the same setup/compile/call path without reading a clock. `warm`
adds wall-clock nanoseconds, as a separate JSON record after the checked result:
`setup_ns`, `compile_ns`, `initialize_ns`, `first_ns`, `warm_ns` (each repetition,
not an aggregate). Timed mode was deliberately not run during this setup.

Exact fixture: `numeric-loop.js`, SHA-256
`a0803261ab85534842c47b85ee4bbdaf37dc29c90466e7d34f99105a9eed4afe` [M].
It defines `numericLoop(n)`, uses local `total` and `i`, sums `0..n-1`, returns the
number and evaluates the function as the Script completion value. This is
**not** the existing module-level `bench/loop.js` contract.

Boundaries:

- **Setup:** `djs_prelude::new_vm(VmOptions::default())`; copies the initialized
  builtin base, does not compile builtins at runtime. No console/tools installed.
- **Compile:** one `Vm::compile` of the embedded fixture in Script mode,
  including creation of its top-level closure; no filesystem read in this region.
- **Initialize:** execute that Script once, obtain the function and root it in
  a public GC handle.
- **First/warm:** retrieve the handle, convert numeric input, `Vm::call` with the
  global object as `this`, `Vm::run(u64::MAX)`, require `Done`, convert/check the
  returned number. Includes that Rust API/check overhead, not just interpreter
  dispatch. Same VM and function throughout; no recompile/reset/forced GC.
  Natural GC remains enabled and its cost, if any, belongs to execution.
- Expected numeric result `n*(n-1)/2` is prepared outside execution timers.
  JSON formatting, printing and final handle release are outside the timers.
  There are no guest host calls, promises, output formatting or adapter prelude.
- Defaults retain upstream's memory cap (`0xfffffff8` bytes [S], not an eager
  allocation); this is not a matched micro-monty memory/fuel policy. No budget
  normalization or universal host ABI equivalence is implied.

Correctness-only checks passed for inputs 0, 1, 10, 1000 and 100000, with repeated
execution in one VM; oversized input was rejected [M]. Input is bounded at
10,000,000 so every intermediate sum is an exact binary64 integer. Timed JSON
serialization was not exercised; its timing values remain unmeasured.

## Images, semantic dumps, and parking

```bash
/tmp/mm-evidence-djs/source/scripts/timeout.sh 15 \
  /tmp/mm-evidence-djs/probe-target/release/djs-probe capabilities \
  /tmp/mm-evidence-djs/capabilities
```

Bounded, public-API correctness observations [M], with no timing:

1. A counting Script stopped with fuel 17; `image().to_vec()` produced a
   187,120-byte raw image. Fully verified `Vm::from_bytes` in a fresh VM resumed
   to 499500. `Vm::dump` at that stop refused because the VM was not idle.
2. A synchronous host call parked with function id 7 and argument 9. Its raw
   image restored with that call/argument intact; delivering 33 resumed to 42.
   A semantic dump refused because a synchronous call was parked.
3. `async-park.js` reached idle awaiting async host `later(33)`, function id 8.
   A format-2 semantic dump, 370 bytes, loaded into a fresh base VM with the
   host name/id mapping restored. The open call id and argument survived;
   delivering 33 made `probeResult` equal 42. The dump writes function source,
   a pending call, an `await#1` site and semantic slots, not an instruction image.

Files under `capabilities/` retain the images, semantic dump and refusal reasons.
All restores in this probe are **same-process, fresh-VM** tests; no cross-process,
cross-target or cross-version compatibility was established. Upstream's current
policy is that raw images are an in-process cache/transport and stress mechanism,
not a supported durable session format, even though the Rust byte-image API
exists. CLI image-file commands were removed. Durable semantic dumps are only
available at idle boundaries, not arbitrary fuel or sync-host-call stops.
Format-3 plain dumps also exist upstream; this probe tests format 2, not format 3.

Verified setup checks: native stable release build, all existing JS checksums,
public-API probe build, probe `check` controls, capability probes, standalone
`rustfmt --check`, `bash -n`, and external-project `cargo clippy --release
--locked --no-deps -- -D warnings`. No fuzz campaign or full upstream suite ran.

## Upstream guidance and source at the pinned revision

- [Native build and CLI guidance](https://github.com/earendil-works/djs/blob/548ab4874a08d85f3a6f0b07feaa510df1aaf969/README.md#L96-L112).
- [CLI Script mode, native backend and semantic resume](https://github.com/earendil-works/djs/blob/548ab4874a08d85f3a6f0b07feaa510df1aaf969/docs/cli.md#L1-L79).
- [Embedding API, handles and VM defaults](https://github.com/earendil-works/djs/blob/548ab4874a08d85f3a6f0b07feaa510df1aaf969/crates/djs-core/src/vm/mod.rs#L1-L136).
- [Raw-image verified/trusted loading distinction](https://github.com/earendil-works/djs/blob/548ab4874a08d85f3a6f0b07feaa510df1aaf969/crates/djs-core/src/vm/mod.rs#L268-L294).
- [Idle-only semantic dump guard](https://github.com/earendil-works/djs/blob/548ab4874a08d85f3a6f0b07feaa510df1aaf969/crates/djs-core/src/vm/dump_flight.rs#L189-L206).
- [Images versus durable dumps policy](https://github.com/earendil-works/djs/blob/548ab4874a08d85f3a6f0b07feaa510df1aaf969/docs/guide/snapshots.md#L1-L45).
