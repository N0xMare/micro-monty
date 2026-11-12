# Running plak's Python cell tests against `mm-cell-worker`

Internal: a record of local integration testing against the external agent
harness behind `mm-cell-worker`'s cell protocol, not part of micro-monty's
public surface.

plak has no micro-monty worker identity yet (docs/spec.md §10.2), so its
Python cell tests only accept the Monty worker `plakous-py-host`. This page
records the local, never-pushed change to a plak checkout that points those
tests at `mm-cell-worker`, how to run them, and what they showed for M3.

## Setup

```bash
export PATH=$HOME/.cargo/bin:$PATH
git clone https://github.com/N0xMare/plak /tmp/plak
git -C /tmp/plak checkout 9ba53187ce597c27b948c89c11fe2821471f1781  # the rev Cargo.toml pins
git -C /tmp/plak apply /path/to/the/diff/below.patch

# The worker: an ordinary build, and one with plak's crash cuts.
cargo build -p mm-cell-worker
cargo build -p mm-cell-worker --features test-hooks --target-dir /tmp/mm-hooks-target

export PLAK_PY_HOST_IDENTITY=micro-monty/micro-monty-cell-adapter.v1
export PLAK_PY_HOST=$PWD/target/debug/mm-cell-worker
```

How plak finds and checks the worker:

- `crates/plakous/src/local_composition/python_tests.rs` and
  `sessions/tests/{python,cell_crashes,sdk}.rs` read `PLAK_PY_HOST` (else a
  sibling `plakous-py-host`); `crates/runtimes/py-worker/tests/*.rs` use the
  compile-time `CARGO_BIN_EXE_plakous-py-host`, which the diff overrides with
  `PLAK_PY_HOST`.
- `CellClient` refuses a `Hello` whose engine and adapter differ from
  `MONTY_WORKER` (`monty` / `plakous.monty-python-adapter.v9`). The diff
  lets `PLAK_PY_HOST_IDENTITY=engine/adapter` stand in for the Monty identity,
  in `check_hello` only.
- The engine contract digest (`cell_engine_contract_identity`) is built
  from the `MONTY_WORKER` constants, not from the binary, and attachment
  hashes whichever file it is given, so the full-stack tests need nothing
  more.
- `EngineConfig::MONTY` (256 MiB) is the configuration the worker accepts as
  the stand-in for a micro-monty one.

## Commands

```bash
cd /tmp/plak
# Integration tests of the worker binary (the crate's unit tests test Monty
# internals and do not run the worker).
cargo test -p plakous-py-worker --no-fail-fast --test process --test session
# py-ptc on the session facade, the full-stack driver and the SDK.
cargo test -p plakous --features composition-python,sessions-sqlite --lib -- \
    python_tests real_monty_driver real_python_sdk
# Crash cuts: the hooked worker, and for cell_crashes a hooked JS host.
cargo build -p plakous-js-runtime --features test-hooks --bin plakous-js-host
PLAK_PY_HOST=/tmp/mm-hooks-target/debug/mm-cell-worker \
    cargo test -p plakous-py-worker --features test-hooks --test delivery_crashes
PLAK_PY_HOST=/tmp/mm-hooks-target/debug/mm-cell-worker \
PLAK_JS_HOST=/tmp/plak/target/debug/plakous-js-host \
    cargo test -p plakous --features composition-python,sessions-sqlite --lib -- \
    real_cell_worker_crash
```

## The library (2026-11-07)

`mm-cell-worker` is a library plus a thin binary (HANDOFF scope decision
44, spec §10.2). The tests on this page drive the binary, which serves the
library's loop over stdio with `NativeEngine`, default `Config`, and its
own hooks (seccomp, the process resource report, crash cuts). A host that
runs workers in its own process calls the library instead:

```rust
use mm_cell_worker::{Kill, NativeEngine, Options};

// On a thread with a large stack (the binary uses 16 MiB):
let kill = Kill::default(); // kill.kill() ends a computing cell's worker
let result = mm_cell_worker::serve(
    Box::new(reader),        // any std::io::Read + Send: host frames
    Box::new(writer),        // any std::io::Write + Send: worker frames
    &NativeEngine::default(), // or a CellEngine over mm-wasm-host
    Options { kill: kill.clone(), ..Options::default() },
);
```

`Config` holds the policy values with plak's defaults (16 outstanding
nested calls and their `RuntimeError` text, 1 MiB print and result caps,
`plak-py-ptc.py`, depth 128, 16 KiB nested errors); there is no queueing.

On the library split [M] (plak e0809c84, release worker): 46 of 48, as
before it: process + session 5/5, `python_tests` + `real_*` 39/41,
`delivery_crashes` 1/1, `real_cell_worker_crash` 1/1. The two failures are
unchanged (the Monty allocator ceiling and the SDK test's type-error cell).
plak's in-process prototype on the library passes 38 of 41 lib tests in
native and in wasm mode, as on its earlier copy of the loop
(docs/research/plak-in-process.md).

## Seccomp

After `Configure` the worker installs its seccomp filter
(`crates/mm-cell-worker/src/seccomp.rs`, docs/spec.md §10.2); a call outside
the allowlist kills it with `SIGSYS` (`openat` fails with `EACCES`), which
the kernel's audit log records as `type=1326` (`sudo dmesg`). To strace the
worker or rule the filter out, set `MM_CELL_WORKER_NO_SECCOMP=1`.

On the `hardening` branch (2026-11-01) [M]: 44 of 48 with the filter (as
without it, debug and release workers), then 45 of 48 once the worker
renders a final int past 4300 digits as `<int with N bits>` (plak's
`format_monty`, through `Session::set_int_bits_text`): process + session
5/5, `python_tests` + `real_*` 38/41, `delivery_crashes` 1/1,
`real_cell_worker_crash` 1/1. Still failing: the Monty-specific allocator
ceiling and the two SDK drivers.

## Results (2026-11-06, main at e4ac16e, plak e0809c84, release worker) [M]

46 of 48, with the seccomp filter: process + session 5/5, `python_tests` +
`real_*` 39/41, `delivery_crashes` 1/1, `real_cell_worker_crash` 1/1. plak
was GitHub `main` at `e0809c84` (its cell protocol equals the pinned
`9ba53187`'s; the client and worker diffs since move tests into their own
files), with only the diff below, which applies unchanged. The same 46 of
48 held on plak `d020df14`. Before e4ac16e the count was 45 of 48 on both.

**SDK stubs are accepted.** `mm-cell-worker` used to refuse `Configure {
sdk_stubs: Some(_) }`. It now accepts the stubs and runs SDK cells without
the pre-run type check: the check belongs to plak (HANDOFF scope decision
34), micro-monty has none, and the worker is a test harness, so this is
protocol compatibility rather than policy. plak's client sends an SDK cell
as `Reset` then `StartCell` with the SDK bootstrap in a Monty prelude, which
the worker already ran; the SDK bootstrap needed no engine change. A
`MontySdk` prelude never reaches a worker and stays refused, as in plak's
Monty worker.

- `real_python_sdk_driver_foreground_stop` now passes.
- `real_python_sdk_driver_values_edits_state_and_quotas` fails only at its
  type-error cell (`await fs.apply([fs.Write('not-run', 'bad')])` then
  `await fs.apply(42)`), which expects `type errors; nothing ran`.
  Unchecked, the first call runs, `fs.apply(42)` raises `ValueError: invalid
  arguments for fs.apply` from plak's host-side argument validation, and the
  test's model fixture stops on the unexpected text. With that one cell's
  source replaced by `print('type errors; nothing ran')` in a scratch copy,
  the whole test passes on mm-cell-worker (state, quotas, `fs`, `proc`,
  refs, output omission). This is the expected cost of running without a
  checker, not an engine gap.
- `allocator_exit_after_nested_result_is_not_acceptance`: unchanged,
  Monty-specific (below).

## Results (2026-11-05, main at 06f1fbc, plak 9ba53187, debug workers) [M]

45 of 48, with the seccomp filter: process + session 5/5, `python_tests` +
`real_*` 38/41, `delivery_crashes` 1/1, `real_cell_worker_crash` 1/1. The
three failures:

- `allocator_exit_after_nested_result_is_not_acceptance` (`left: Accepted,
  right: HostLost`): Monty's hard allocator ceiling kills its worker after
  the 240 MiB string; micro-monty holds it within the 256 MiB limit and the
  cell completes. Monty-specific.
- `real_python_sdk_driver_foreground_stop` and
  `real_python_sdk_driver_values_edits_state_and_quotas` (`RecoveryRequired
  (AgentError { kind: OutcomeUnknown, .. })` at `sdk.rs:368`): the SDK
  presentation sends `sdk_stubs`, which the worker refuses
  (`crates/mm-cell-worker/src/worker.rs`, `sdk_stubs: Some(_)`). Expected
  until plak has its host-side checker over `stubs()`: pre-run type
  checking is plak's, and micro-monty adds no static checker (HANDOFF
  scope decision 34).

The engine-gap failures of the MVP run below (complex numbers, `os`/`time`/
`random`, named-tuple records, `<int with N bits>`) all pass now.

## Results (2026-11-01 at the MVP, plak 9ba53187, debug builds) [M]

Monty itself passes all 38 `python_tests` on this checkout. With
`mm-cell-worker`:

| Suite | Pass | Fail |
|---|---|---|
| `plakous` `python_tests` (37 + 1 in `session`) | 33 | 5 |
| `real_monty_driver_yields_steers_and_waits_for_the_result` | 1 | 0 |
| `real_cell_worker_crash_recovery_boundaries` (JS and Python, both cuts) | 1 | 0 |
| `real_python_sdk_driver_*` | 0 | 2 |
| `plakous-py-worker` `process` + `session` | 4 | 1 |
| `plakous-py-worker` `delivery_crashes` | 1 | 0 |

Failures, by cause:

- Engine gaps:
  - `final_values_render_as_python_str`: `1 + 2j`, no complex numbers.
  - `a_large_int_result_renders_in_bounded_host_time_and_tears_down`:
    `1 << 64_000_000` as the last expression; plak expects Monty's
    `<int with 64000001 bits>`, micro-monty raises CPython's 4300-digit
    `ValueError` when rendering it.
  - `os_calls_are_refused_and_clock_and_random_are_answered_in_the_worker`
    and `the_session_outlives_upstreams_default_suspension_budget`: no `os`,
    `time`, `random`, `datetime` or `pathlib` modules (the first also pins
    Monty's per-call OS refusals).
  - `records_and_bytes_reach_the_interpreter_as_typed_values`: no `bytes`
    host values and no named tuples (a `Record` arrives as a plain tuple).
- Monty-specific:
  - `allocator_exit_after_nested_result_is_not_acceptance`: expects Monty's
    hard allocator ceiling to kill the worker after a 240 MiB string; the
    engine holds it within its 256 MiB limit and the cell completes.
  - `real_python_sdk_driver_*`: the SDK presentation configures type-checking
    stubs; micro-monty has no type checker and refuses `sdk_stubs` with
    `HostLost{ProtocolViolation}`.

## The diff

```diff
diff --git a/crates/runtimes/cell/src/client.rs b/crates/runtimes/cell/src/client.rs
index 975c838a..e83f3f83 100644
--- a/crates/runtimes/cell/src/client.rs
+++ b/crates/runtimes/cell/src/client.rs
@@ -617,7 +617,32 @@ async fn refuse(child: &mut Child, error: CellStartupError) -> CellStartupError
     }
 }
 
+/// LOCAL micro-monty integration only: `PLAK_PY_HOST_IDENTITY=engine/adapter`
+/// lets a stand-in Python worker answer for the Monty identity.
+fn python_worker_override(expected: WorkerIdentity) -> Option<(String, String)> {
+    if expected != crate::MONTY_WORKER {
+        return None;
+    }
+    let value = std::env::var("PLAK_PY_HOST_IDENTITY").ok()?;
+    let (engine, adapter) = value.split_once('/')?;
+    Some((engine.to_owned(), adapter.to_owned()))
+}
+
 fn check_hello(frame: &WorkerFrame, expected: WorkerIdentity) -> Result<(), String> {
+    if let (
+        Some((want_engine, want_adapter)),
+        WorkerFrame::Hello {
+            revision,
+            engine,
+            adapter,
+        },
+    ) = (python_worker_override(expected), frame)
+        && revision == CELL_PROTOCOL_REVISION
+        && *engine == want_engine
+        && *adapter == want_adapter
+    {
+        return Ok(());
+    }
     match frame {
         WorkerFrame::Hello {
             revision,
diff --git a/crates/runtimes/py-worker/tests/delivery_crashes.rs b/crates/runtimes/py-worker/tests/delivery_crashes.rs
index 9b155101..b0228998 100644
--- a/crates/runtimes/py-worker/tests/delivery_crashes.rs
+++ b/crates/runtimes/py-worker/tests/delivery_crashes.rs
@@ -10,13 +10,20 @@ use plakous_cell::{
     MONTY_WORKER, MontyBinding, MontyBootstrap, NestedDeliveryOutcome, ObservationMode,
 };
 
+/// LOCAL micro-monty integration only: the worker under test, `PLAK_PY_HOST`
+/// or this package's own build.
+fn py_host() -> String {
+    std::env::var("PLAK_PY_HOST")
+        .unwrap_or_else(|_| env!("CARGO_BIN_EXE_plakous-py-host").to_owned())
+}
+
 #[tokio::test]
 async fn cell_worker_delivery_crash_cuts_preserve_only_proven_acceptance() {
     for (cut, accepted) in [("before_delivery", false), ("after_accepted", true)] {
         let directory = tempfile::tempdir().expect("directory");
         let ready = directory.path().join("ready");
         let wrapper = directory.path().join("worker");
-        let worker = env!("CARGO_BIN_EXE_plakous-py-host");
+        let worker = py_host();
         assert!(!worker.contains('\''));
         assert!(!ready.to_string_lossy().contains('\''));
         std::fs::write(
diff --git a/crates/runtimes/py-worker/tests/process.rs b/crates/runtimes/py-worker/tests/process.rs
index e2156b68..66b536e4 100644
--- a/crates/runtimes/py-worker/tests/process.rs
+++ b/crates/runtimes/py-worker/tests/process.rs
@@ -17,10 +17,17 @@ use plakous_cell::{
     WorkerFrame, encode_frame, read_frame_blocking,
 };
 
+/// LOCAL micro-monty integration only: the worker under test, `PLAK_PY_HOST`
+/// or this package's own build.
+fn py_host() -> String {
+    std::env::var("PLAK_PY_HOST")
+        .unwrap_or_else(|_| env!("CARGO_BIN_EXE_plakous-py-host").to_owned())
+}
+
 /// Writes `frames`, closes stdin, and returns every frame the worker wrote
 /// and whether it exited successfully.
 fn exchange(frames: &[HostFrame]) -> (Vec<WorkerFrame>, bool) {
-    let mut child = Command::new(env!("CARGO_BIN_EXE_plakous-py-host"))
+    let mut child = Command::new(py_host())
         .stdin(Stdio::piped())
         .stdout(Stdio::piped())
         .stderr(Stdio::null())
@@ -41,6 +48,20 @@ fn exchange(frames: &[HostFrame]) -> (Vec<WorkerFrame>, bool) {
     (received, child.wait().expect("worker exits").success())
 }
 
+/// LOCAL micro-monty integration only: `PLAK_PY_HOST_IDENTITY=engine/adapter`.
+fn expected_identity() -> (String, String) {
+    std::env::var("PLAK_PY_HOST_IDENTITY")
+        .ok()
+        .and_then(|value| {
+            let (engine, adapter) = value.split_once('/')?;
+            Some((engine.to_owned(), adapter.to_owned()))
+        })
+        .unwrap_or((
+            MONTY_WORKER.engine.to_owned(),
+            MONTY_WORKER.adapter.to_owned(),
+        ))
+}
+
 fn configure(engine: EngineConfig) -> HostFrame {
     HostFrame::Configure {
         sdk_stubs: None,
@@ -56,8 +77,8 @@ fn the_worker_says_hello_and_ends_cleanly_with_its_input() {
         received,
         [WorkerFrame::Hello {
             revision: CELL_PROTOCOL_REVISION.to_owned(),
-            engine: MONTY_WORKER.engine.to_owned(),
-            adapter: MONTY_WORKER.adapter.to_owned(),
+            engine: expected_identity().0,
+            adapter: expected_identity().1,
         }]
     );
     assert!(success);
@@ -111,7 +132,7 @@ async fn records_and_bytes_reach_the_interpreter_as_typed_values() {
         MontyBootstrap, NestedDeliveryOutcome,
     };
     let client = CellClient::spawn(
-        std::path::Path::new(env!("CARGO_BIN_EXE_plakous-py-host")),
+        std::path::Path::new(&py_host()),
         MONTY_WORKER,
         EngineConfig::MONTY,
     )
diff --git a/crates/runtimes/py-worker/tests/session.rs b/crates/runtimes/py-worker/tests/session.rs
index 3b951280..acd10f9a 100644
--- a/crates/runtimes/py-worker/tests/session.rs
+++ b/crates/runtimes/py-worker/tests/session.rs
@@ -14,16 +14,19 @@ use plakous_cell::{
     ObservationMode, OutputItem,
 };
 
+/// LOCAL micro-monty integration only: the worker under test, `PLAK_PY_HOST`
+/// or this package's own build.
+fn py_host() -> String {
+    std::env::var("PLAK_PY_HOST")
+        .unwrap_or_else(|_| env!("CARGO_BIN_EXE_plakous-py-host").to_owned())
+}
+
 #[tokio::test]
 async fn nested_responder_depth_guard_rejects_only_the_call_and_preserves_the_session() {
     let client = Arc::new(
-        CellClient::spawn(
-            Path::new(env!("CARGO_BIN_EXE_plakous-py-host")),
-            MONTY_WORKER,
-            EngineConfig::MONTY,
-        )
-        .await
-        .expect("worker"),
+        CellClient::spawn(Path::new(&py_host()), MONTY_WORKER, EngineConfig::MONTY)
+            .await
+            .expect("worker"),
     );
     let (session, mut events) = CellSession::new(
         client,
```
