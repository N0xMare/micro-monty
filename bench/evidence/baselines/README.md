# Independent comparator setup (2026-11-04)

Build/setup and bounded correctness evidence only. **No benchmark timing rounds
were run.** `[M]` means measured bytes or observed correctness; `[S]` means
published metadata/source documentation. No performance conclusions follow from
these probes. Monty's CLI writes its own duration to stderr; that incidental
diagnostic in the raw smoke log is not a campaign timing result.

## Pins and executable paths

All build products, downloads, environments and raw evidence live under
`B=/tmp/mm-evidence-baselines`. This directory contains only reusable adapters,
their dependency lockfiles, and these instructions. Existing benchmark programs
and the main engine/harness are untouched.

- `[S]` **Stock Bellard QuickJS 2026-06-04**, downloaded directly from
  <https://bellard.org/quickjs/quickjs-2026-06-04.tar.xz>, SHA-256
  `b376e839b322978313d929fd20663b11ba58b75df5a46c126dd19ea2fa70ad2a`.
  Release archive, not dquickts and not a GitHub default-branch build.
  Default native `make -j2 qjs libquickjs.a`: GCC 12.2.0, `-O2 -g -fwrapv`,
  no LTO, no sanitizers, no feature stripping. `bin/qjs-stock` is a stripped
  copy; the original debug-bearing executable remains in the source directory.
- `[S]` Live PyPI JSON on the check date reports **1.4.0** as latest for
  `pydantic-monty`, `pydantic-monty-client`, and `pydantic-monty-runtime`.
  This is the supported stable published release, **the same runtime version
  as the historical comparator**, not an upgrade beyond 1.4.0. Its source tag
  `1.4.0` is commit `4b015dc4a1e08394b0a98e0255ed8c81de1fbbf6`.
  Package JSON responses and the untouched source checkout are retained in `B`.
  Historical QuickJS results cannot be treated as this stock build merely
  because the displayed version date matches the earlier dquickts build.
- `[M]` Reference **CPython 3.14.7**, `/home/user/.local/bin/python3.14`,
  pre-existing uv-managed installation; not system Python 3.11.
- Monty isolated environment: `B/venv`; exact pins are in
  `requirements-linux-x86_64-cp314.lock`: three Monty packages at 1.4.0 and
  `typing-extensions==4.16.0`. CPython 3.14 GIL wheel, Linux x86_64,
  glibc >= 2.28. No telemetry extras. Actual downloaded wheel filenames, URLs,
  hashes, and sizes: `B/wheels.json`; archives: `B/wheels/`.

`[M]` Canonical native executable SHA-256 values:

| Path relative to `B` | Bytes | SHA-256 |
|---|---:|---|
| `bin/qjs-stock` | 1047880 | `987c28302c096201b2559fa95435785ff4ae2f8d44278316a1456e691460ff08` |
| `quickjs-2026-06-04/qjs` (unstripped) | 4998688 | `5a68293ffda947e02c0df8002f05b6d38dbeefb983158249bc4be30c4487fe1b` |
| `venv/bin/monty` (published, stripped) | 31551096 | `4479e7306fdf0c6a2a567aa139150ac47c75ab3c0138ec2c4efa010c382a87e0` |
| `bin/monty-warm` (our core-only host) | 9919384 | `76c021adc6ccba69ae8efd464b1a27511f7d9a97a0068a548bdfccf23410805d` |
| `bin/quickjs-warm` (our host, stripped) | 952928 | `f01409360d7343cbc3f80d5fa840cb7b32ad06a263f682aec0424c6a8d776d31` |

CPython executable SHA-256:
`241bf774a81580bb760adf960b48df7e6b23b1de026c9c912d4ca6be1a08241e`.
Client `_monty.cpython-314-x86_64-linux-gnu.so` SHA-256:
`d22875ef78df13925b3233e6337ea0a45f03a2e9ecbaa6bba01989d94d2cd2ad`.
`B/SHA256SUMS` records the binaries, source adapters, input files, and logs.
Hashes identify this orb's builds, not a claim of bit-reproducibility on another host.

## Reproduce setup, without modifying upstream sources

Run from the repository root, with a fresh `B` for a fresh installation:

```bash
R="$PWD"
B=/tmp/mm-evidence-baselines
A="$R/bench/evidence/baselines"
mkdir -p "$B/bin"
curl -fsSL https://bellard.org/quickjs/quickjs-2026-06-04.tar.xz \
  -o "$B/quickjs-2026-06-04.tar.xz"
printf '%s  %s\n' \
  b376e839b322978313d929fd20663b11ba58b75df5a46c126dd19ea2fa70ad2a \
  "$B/quickjs-2026-06-04.tar.xz" | sha256sum -c -
tar -xJf "$B/quickjs-2026-06-04.tar.xz" -C "$B"
make -C "$B/quickjs-2026-06-04" -j2 qjs libquickjs.a
cp "$B/quickjs-2026-06-04/qjs" "$B/bin/qjs-stock"
strip --strip-all "$B/bin/qjs-stock"
uv venv --python /home/user/.local/bin/python3.14 "$B/venv"
uv pip install --python "$B/venv/bin/python" --require-hashes \
  --only-binary :all: -r "$A/requirements-linux-x86_64-cp314.lock"
```

The current installation was additionally force-reinstalled from the saved
wheelhouse with `--reinstall --no-index --find-links "$B/wheels"` and the same
hash lock. This verifies actual wheel hashes rather than just trusting an
already-satisfied package. uv version: 0.12.9.

## Existing file invocation and checksum smoke

```bash
"$B/bin/qjs-stock" "$R/bench/fib.js"
"$B/venv/bin/monty" "$R/bench/fib.py"
/home/user/.local/bin/python3.14 "$R/bench/fib.py"

"$B/venv/bin/python" "$A/smoke.py" --repo "$R" \
  --python /home/user/.local/bin/python3.14 \
  --qjs "$B/bin/qjs-stock" --monty "$B/venv/bin/monty" \
  --output "$B/smoke.json"
```

No flags are needed for normal scripts; do not turn on Monty's type checker.
`json` maps to `json_rows.py` / `json_rows.js`. The adapter takes expected
stdout from the read-only `bench/expected.txt`, runs each file once with a
30-second outer correctness watchdog, and stores exit code/stdout/full stderr.
`QJS="$B/bin/qjs-stock"`, `MONTY="$B/venv/bin/monty"`, and
`PYTHON=/home/user/.local/bin/python3.14` are the compatible existing-harness
environment settings. No existing timing harness was run or edited.

`[M]` CPython: **13/13** checksum matches. Stock QuickJS: **13/13**.
Monty CLI: **11/13**, with these actual compile errors (also reproduced by
both core probe modes):

- `generators.py:6`: `NotImplementedError: The monty syntax parser does not yet support yield expressions`
- `exceptions.py:4`: `NotImplementedError: The monty syntax parser does not yet support class inheritance and metaclasses`

These are not checksum passes or silently skipped programs. They match the
historical runtime 1.4.0 gaps. Plain user classes (`classes`, `interp`) pass.

## Warm public-API hosts (no timers)

Build the reusable hosts outside the main Cargo workspace:

```bash
cp -a "$A/monty-warm" "$B/monty-warm" # fresh destination on a new setup
cargo +1.99.0 build --release --locked -j2 \
  --manifest-path "$B/monty-warm/Cargo.toml"
cp "$B/monty-warm/target/release/mm-evidence-monty-warm" "$B/bin/monty-warm"
gcc -std=c11 -O2 -g -Wall -Wextra -Werror \
  -isystem "$B/quickjs-2026-06-04" "$A/quickjs-warm.c" \
  "$B/quickjs-2026-06-04/libquickjs.a" -lm -lpthread -ldl \
  -o "$B/quickjs-warm"
cp "$B/quickjs-warm" "$B/bin/quickjs-warm"
strip --strip-all "$B/bin/quickjs-warm"

"$B/bin/monty-warm" compiled "$R/bench/fib.py" 196418
"$B/bin/monty-warm" session "$R/bench/fib.py" 196418
"$B/bin/quickjs-warm" "$R/bench/fib.js" 196418
```

Each invocation performs exactly two correctness evaluations, asserting both
stdout checksums. It emits boundary labels, **not durations**. Source is read
before the labeled boundaries and is never rewritten or wrapped.

- Monty `compiled`: `MontyRun::new` parses/prepares once; `MontyRun::run`
  reuses the compiled object but creates a fresh execution heap per call,
  including execution/output collection/final-value conversion in that call.
- Monty `session`: `MontyRepl::new` creates a persistent session; each
  `feed_run` **fuses compilation and execution** and retains globals. Do not
  call a feed an isolated evaluation-only measurement.
- QuickJS: one `JS_NewRuntime`; `JS_Eval(...COMPILE_ONLY)` once; serialize
  trusted bytecode with `JS_WriteObject`; create each fresh `JS_NewContext`,
  install `console.log`, load with `JS_ReadObject`, evaluate with
  `JS_EvalFunction`. The **load cost is a distinct boundary**, not equivalent
  to Monty's compiled-object reuse. This is a bytecode cache, not a paused-VM
  snapshot. It is a basic synchronous bench host, not a sandbox: no `std`/`os`
  modules, module loader, async job draining, or execution interrupt installed;
  source capped at 1 MiB, captured output at 16 KiB. Use trusted bench inputs.

`[M]` QuickJS host: all 13 files match twice. Each Monty mode: 11 files
match twice, with the same two explicit compiler gaps. Raw boundary/checksum
logs: `B/quickjs-warm-smoke.json` and `B/monty-warm-smoke.json`.

The Monty host uses **published crates** `monty=1.4.0` and
`monty-types=1.4.0`, not the checkout as a path dependency. `Cargo.lock` pins
transitives/checksums (including `monty-macros=1.4.0`); lock SHA-256
`d10f61d2ce31b951c1b887be31a1b6e563272bc7e91e3ce5ab75863dd5b5fcf4`.
Rust 1.99.0, release, fat LTO, one codegen unit, stripped; no optional Monty
features, no tzdb, no PGO, no type checker/pool/CLI, no custom allocator and
therefore **no memory-limit guarantee**. A 10-second feed execution ceiling
is a correctness backstop, not a scheduling/yield mechanism. This build is
not identical to the published PGO runtime worker. The Python public package
itself runs workers in subprocesses; it cannot honestly be labeled in-process
VM execution. This is why the warm in-process comparator is a Rust host.

## Suspension/snapshot boundaries: observations vs source claims

```bash
"$B/venv/bin/python" "$A/monty-boundaries.py" \
  --output "$B/monty-boundaries.json"
```

`[M]` Eighteen bounded observations pass on the installed published wheel:

- Idle session dump/load preserves globals; loaders reject the wrong dump kind
  and reject a session already driven by a feed.
- A host-call suspension exposes arguments and source position. Dump/resume
  yields 42. Restoring the same dump twice and answering 41/99 produces 42/100.
  A fresh Python process plus fresh Monty worker restores and finishes with 42;
  `print('before')` is **not replayed**, and only `after\n` is collected there.
- Feeding while paused raises `RuntimeError: monty worker protocol error: feed called while a suspension is awaiting an answer`.
  Resuming a cursor twice raises `RuntimeError: snapshot has already been resumed`.
- Undefined-name and OS-call suspensions dump/restore correctly. The OS call is
  resumed with an explicit `FileNotFoundError`; no host file is opened.
- An awaited host call answered with a pending future becomes a
  `FutureSnapshot`. After dump/restore, `resume_auto()` rejects with
  `RuntimeError: async external functions require AsyncMonty`; this does not
  consume the snapshot, and manual resolution by call ID returns 42.
- Pure `sum(range(100))` returns completion, not an intermediate snapshot.
  `fuel=1` is rejected with `TypeError: MontySession.feed_start() got an unexpected keyword argument 'fuel'`.
  A bounded pure infinite loop with a 0.01-second turn ceiling raises
  `MontyRuntimeError: TimeoutError: turn time limit exceeded: ...` rather than
  returning a resumable time-slice snapshot. That terminal session is discarded.

`[S]` The supported public progress variants are host call, OS call, future
resolution, name lookup, completion—not fuel/request-to-park. The above probes
do not establish arbitrary-instruction suspension or a worst-case response
bound. Documentation says idle versus suspended loaders must be used on fresh
sessions, and snapshots are **trusted, unmodified inputs**: they are not
authenticated or fully validated. Host callbacks, mounts, overlays, Python
instance stores and pending host coroutines are not all serialized; reattach
host state as documented. Cross-machine/cross-version restore, mount overlays,
real asynchronous host coroutines, and cross-target restore were not tested.

Pinned source references:

- [Public progress variants](https://github.com/pydantic/monty/blob/4b015dc4a1e08394b0a98e0255ed8c81de1fbbf6/crates/monty/src/run_progress.rs#L32-L50)
- [Compile/run separation](https://github.com/pydantic/monty/blob/4b015dc4a1e08394b0a98e0255ed8c81de1fbbf6/crates/monty/src/run.rs#L64-L86)
- [Python pool and session API](https://github.com/pydantic/monty/blob/4b015dc4a1e08394b0a98e0255ed8c81de1fbbf6/crates/monty-python/python/pydantic_monty/_monty.pyi#L578-L612)
- [Loaders, trust, suspension documentation](https://github.com/pydantic/monty/blob/4b015dc4a1e08394b0a98e0255ed8c81de1fbbf6/docs/snapshots.md#L10-L32)
- [Python restore exclusions](https://github.com/pydantic/monty/blob/4b015dc4a1e08394b0a98e0255ed8c81de1fbbf6/crates/monty-python/python/pydantic_monty/_monty.pyi#L836-L898)
- [Resource-error semantics](https://github.com/pydantic/monty/blob/4b015dc4a1e08394b0a98e0255ed8c81de1fbbf6/docs/resource-limits.md#L138-L166)

## Deployment-size accounting

`B/deployment.json` inventories actual wheel members and installed/native
files, ELF `NEEDED` entries, `ldd` resolution paths, library sizes/hashes,
and CPython's base installation. All byte counts here are `[M]` exact file
lengths, not RSS or compressed-vs-expanded comparisons disguised as like units.

- Downloaded four-wheel set: **19195790 bytes compressed**;
  **43192291 bytes expanded ZIP members**. Runtime wheel alone:
  14726134 compressed bytes. Runtime executable: 31551096 bytes.
- Python client native extension: **10416144 bytes**, in addition to the
  worker executable and Python source/stubs/typing dependency. The 7552-byte
  `pydantic-monty` wheel is a **metapackage**, not the whole engine.
- Standalone Monty CLI has no dependency on a Python executable at runtime.
  The Python API additionally needs compatible CPython. This orb's existing
  CPython executable is 32207448 bytes, complete base tree 105944720 bytes;
  neither is a minimized Python deployment. Do not count a venv's Python symlink
  as providing a self-contained interpreter.
- Monty worker and Python extension ELF dependencies are `libgcc_s`,
  `libpthread`, `libm`, `libdl`, `libc`, plus the dynamic loader. The wheel's
  maximum referenced GLIBC symbol version is 2.28. This orb uses glibc 2.36.
  Stock QuickJS resolves `libm`, `libc`, and the loader. Libraries shared by
  components should be counted once, or explicitly treated as base-OS costs.
- Core-only Monty host: 9919384 stripped bytes; QuickJS host: 952928 stripped
  bytes. Neither replaces the full CLI/package footprint. QuickJS's archive
  `libquickjs.a` is 9342472 bytes with debug objects, **not** its linked
  stripped deployment size. Debug-bearing QuickJS CLI is 4998688 bytes versus
  1047880 stripped. Always state stripping and product boundaries.
- `[S]` Published runtime source enables standalone CLI, links core, filesystem,
  worker protocol, allocator, type checker and timezone data; telemetry is
  optional/off by default. Its release workflow requests PGO for Linux x86_64;
  the actual installed wheel is identified by hash, not rebuilt here.
  The core-only host's complete locked Rust dependency/features inventory is
  in `B/monty-core-metadata.json` and `B/monty-core-features.txt`.

Size/source evidence can be rechecked with `stat -c '%s %n' FILE`,
`sha256sum FILE`, `readelf -d FILE`, `readelf -V FILE`, `ldd FILE`, and
`python -m zipfile -l WHEEL`.
[Runtime features/dependencies](https://github.com/pydantic/monty/blob/4b015dc4a1e08394b0a98e0255ed8c81de1fbbf6/crates/monty-runtime/Cargo.toml#L21-L47),
[published PGO build configuration](https://github.com/pydantic/monty/blob/4b015dc4a1e08394b0a98e0255ed8c81de1fbbf6/.github/workflows/ci.yml#L881-L908).

`[M]` All 63 original regular files from the QuickJS source archive match
after building; generated build outputs are additional files. Monty checkout
`git status` is clean. Adapter Rust formatting/clippy and strict C compilation
are checked independently of the main workspace. No commits or pushes.

Local source checkout: pydantic/monty at
`4b015dc4a1e08394b0a98e0255ed8c81de1fbbf6`,
`/tmp/mm-evidence-baselines/monty-source`.
