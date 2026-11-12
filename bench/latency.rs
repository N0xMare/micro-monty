//! Standalone public-facade latency probe, not a timing acceptance threshold.
//!
//! Build the CLI's feature set and select its facade artifact explicitly:
//! ```sh
//! cargo build --release --locked -p mm-cli --message-format=json > /tmp/mm-build.json
//! lib=$(jq -r 'select(.reason == "compiler-artifact" and .target.name == "micro_monty") | .filenames[] | select(endswith(".rlib"))' /tmp/mm-build.json)
//! rustc --edition=2024 -C opt-level=3 -C panic=abort bench/latency.rs \
//!   -L dependency=target/release/deps --extern "micro_monty=$lib" -o /tmp/mm-latency
//! taskset -c 2,3 /tmp/mm-latency
//! ```
//! Run alone on an idle machine. One warmup and seven measured samples per
//! case, with a fresh session for each native-operation sample. A separate
//! thread requests a park after sleeping 1 ms; measure from its actual request,
//! not the intended wakeup. The main thread stops at a requested park OR a host
//! call; report which. `None` means it stopped before the requester timestamp.
//! Setup, completion-text rendering and post-operation assertions are outside
//! the response measurement. Native operations are not interrupted midway.
//! Lifecycle samples keep the guest graph alive and drop the old session before
//! restore. Start includes between-cell GC/reclamation and a tiny query;
//! snapshot includes GC. Neither isolates collector time or measures RSS.
//! Assertions intentionally fail the driver on wrong outputs or progress.

use micro_monty::{HostModule, Engine, EngineConfig, HostValue, ParkReason, Progress, Resume, Session};
use std::sync::mpsc;
use std::time::{Duration, Instant};

fn host(progress: &Progress, expected: &str) {
    assert!(
        matches!(progress, Progress::HostCall { name, .. } if name == expected),
        "{progress:?}"
    );
}

fn finish(session: &mut Session, mut progress: Progress) {
    loop {
        match progress {
            Progress::Complete(_) => return,
            Progress::Parked { .. } => progress = session.resume(Resume::Continue).unwrap(),
            _ => panic!("unexpected completion: {progress:?}"),
        }
    }
}

fn native(engine: &Engine, label: &str, setup: &str, operation: &str, check: &str) {
    let source = format!("{setup}\nbefore()\n{operation}\nafter()\n{check}\n");
    let code = engine.compile(&source, "latency.py").unwrap();
    for sample in 0..8 {
        let mut session = Session::new(engine).unwrap();
        session.set_last_expr_value(false);
        session
            .define(&HostModule::untyped("", &["before", "after"]))
            .unwrap();
        host(&session.start(&code), "before");
        let handle = session.park_handle();
        let (tx, rx) = mpsc::sync_channel(0);
        let requester = std::thread::spawn(move || {
            rx.recv().unwrap();
            std::thread::sleep(Duration::from_millis(1));
            let requested = Instant::now();
            handle.request();
            requested
        });
        let start = Instant::now();
        tx.send(()).unwrap();
        let mut progress = session.resume(Resume::Value(HostValue::None)).unwrap();
        let stopped = Instant::now();
        let requested = requester.join().unwrap();
        let reason = if matches!(
            progress,
            Progress::Parked {
                reason: ParkReason::Requested
            }
        ) {
            "requested"
        } else {
            host(&progress, "after");
            "host"
        };
        let latency = stopped
            .checked_duration_since(requested)
            .map(|t| t.as_nanos());
        if sample > 0 {
            println!(
                "[M] request case={label} sample={sample} resume_ns={} request_after_ns={} stop={reason} response_ns={latency:?}",
                stopped.duration_since(start).as_nanos(),
                requested.duration_since(start).as_nanos()
            );
        }
        while matches!(progress, Progress::Parked { .. }) {
            progress = session.resume(Resume::Continue).unwrap();
        }
        host(&progress, "after");
        let progress = session.resume(Resume::Value(HostValue::None)).unwrap();
        finish(&mut session, progress);
    }
}

fn lifecycle(engine: &Engine, n: usize) {
    let setup = engine
        .compile(
            &format!("data = [{{'i': i, 's': 'tag'}} for i in range({n})]"),
            "setup.py",
        )
        .unwrap();
    let query = engine
        .compile(
            &format!("assert len(data) == {n}\nassert data[-1]['i'] == {}", n - 1),
            "query.py",
        )
        .unwrap();
    let mut session = Session::new(engine).unwrap();
    session.set_last_expr_value(false);
    let progress = session.start(&setup);
    finish(&mut session, progress);
    // Warm up the between-cell reclamation paths before recording.
    for sample in 0..8 {
        let start = Instant::now();
        let progress = session.start(&query);
        let start_ns = start.elapsed().as_nanos();
        finish(&mut session, progress);
        let start = Instant::now();
        let image = session.snapshot().unwrap();
        let snapshot_ns = start.elapsed().as_nanos();
        drop(session);
        let start = Instant::now();
        session = engine.restore(&image).unwrap();
        let restore_ns = start.elapsed().as_nanos();
        session.set_last_expr_value(false);
        let progress = session.start(&query);
        finish(&mut session, progress);
        if sample > 0 {
            println!(
                "[M] lifecycle rows={n} sample={sample} start_ns={start_ns} snapshot_with_gc_ns={snapshot_ns} restore_ns={restore_ns} snapshot_bytes={}",
                image.len()
            );
        }
    }
}

fn main() {
    let engine = Engine::new(EngineConfig::default());
    native(
        &engine,
        "loop",
        "x = 0",
        "for i in range(200000):\n    x += i",
        "assert x == 19999900000",
    );
    native(
        &engine,
        "repr",
        "data = ['x'*64] * 200000",
        "result = repr(data)",
        "assert len(result) == 13600000\nassert result.count('x') == 12800000\nassert result[:2] == \"['\" and result[-2:] == \"']\"",
    );
    native(
        &engine,
        "join",
        "data = ['x'*64] * 200000",
        "result = ''.join(data)",
        "assert len(result) == 12800000\nassert result.count('x') == 12800000",
    );
    native(
        &engine,
        "replace",
        "data = 'ab' * 8000000",
        "result = data.replace('a', 'xyz')",
        "assert len(result) == 32000000\nassert result.count('xyzb') == 8000000",
    );
    native(
        &engine,
        "json",
        "import json\ndata = ['x'*64] * 200000",
        "result = json.dumps(data)",
        "assert len(result) == 13600000\nassert result.count('x') == 12800000\nassert result[:2] == '[\"' and result[-2:] == '\"]'",
    );
    native(
        &engine,
        "sort",
        "data = list(range(249999, -1, -1))",
        "data.sort()",
        "assert len(data) == 250000\nassert all(data[i] == i for i in range(250000))",
    );
    native(
        &engine,
        "sort-repeat",
        "data = list(range(249999, -1, -1))",
        "for _ in range(100):\n    data.sort()",
        "assert len(data) == 250000\nassert all(data[i] == i for i in range(250000))",
    );
    for n in [1000, 10000, 100000] {
        lifecycle(&engine, n);
    }
    println!("latency probes: every result assertion passed");
}
