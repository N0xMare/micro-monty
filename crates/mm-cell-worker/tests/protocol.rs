//! The worker's side of `plakous-cell-protocol.v2`, driven over real pipes
//! with plak's frames: the py-ptc bootstrap, nested calls and their
//! `Delivery` verdicts, output order, globals and `Reset`, engine-parked
//! yields, `Cancel`, and the argument bounds of plak's Monty worker.

#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "test setup failures should abort the test"
)]
#![allow(
    clippy::similar_names,
    reason = "cell and call are the protocol's own field names"
)]

use std::{
    io::Write,
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    time::Duration,
};

use plakous_cell::{
    BindingInput, CELL_PROTOCOL_REVISION, CellOutcome, CellPrelude, CellValue, DeliveryVerdict,
    EngineConfig, HostFrame, LossClass, MAX_FRAME_BYTES, MontyBinding, MontyBootstrap, OutputItem,
    WorkerFrame, encode_frame, read_frame_blocking,
};

/// plak's generated py-ptc bootstrap for a dictionary tool `fetch` and a
/// string tool `patch` (`plakous_compose::python_bootstrap`).
fn bootstrap() -> MontyBootstrap {
    MontyBootstrap {
        source: "async def fetch(arguments):\n    return await _plakous_host_0(arguments)\n\
                 async def patch(arguments):\n    return await _plakous_host_1(arguments)\n"
            .to_owned(),
        bindings: vec![
            MontyBinding {
                function: "_plakous_host_0".into(),
                tool: "fetch_tool".into(),
                input: BindingInput::Dictionary,
            },
            MontyBinding {
                function: "_plakous_host_1".into(),
                tool: "patch_tool".into(),
                input: BindingInput::String,
            },
        ],
    }
}

struct Worker {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: ChildStdout,
}

impl Worker {
    fn spawn() -> Self {
        Self::spawn_with(Command::new(env!("CARGO_BIN_EXE_mm-cell-worker")))
    }

    fn spawn_with(mut command: Command) -> Self {
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn worker");
        let stdin = child.stdin.take();
        let stdout = child.stdout.take().expect("stdout");
        Self {
            child,
            stdin,
            stdout,
        }
    }

    /// A worker past `Hello` and `Configure`.
    fn configured() -> Self {
        let mut worker = Self::spawn();
        assert!(matches!(worker.recv(), WorkerFrame::Hello { .. }));
        worker.send(&configure(CELL_PROTOCOL_REVISION, None));
        worker
    }

    fn send(&mut self, frame: &HostFrame) {
        let bytes = encode_frame(frame).expect("encode");
        let stdin = self.stdin.as_mut().expect("stdin open");
        stdin.write_all(&bytes).expect("write frame");
        stdin.flush().expect("flush");
    }

    fn recv(&mut self) -> WorkerFrame {
        let body = read_frame_blocking(&mut self.stdout, MAX_FRAME_BYTES)
            .expect("read frame")
            .expect("a frame before end of stream");
        serde_json::from_slice(&body).expect("decode worker frame")
    }

    fn start(&mut self, cell: u64, source: &str, bootstrap: Option<MontyBootstrap>) {
        self.start_within(cell, source, bootstrap, None);
    }

    fn start_within(
        &mut self,
        cell: u64,
        source: &str,
        bootstrap: Option<MontyBootstrap>,
        window: Option<u64>,
    ) {
        self.send(&HostFrame::StartCell {
            cell,
            source: source.to_owned(),
            prelude: CellPrelude::Monty { bootstrap },
            window,
        });
    }

    /// Frames up to and including `cell`'s terminal: its text output
    /// joined, and the outcome.
    fn finish(&mut self, cell: u64) -> (String, CellOutcome) {
        let mut text = String::new();
        loop {
            match self.recv() {
                WorkerFrame::Output {
                    cell: c,
                    item: OutputItem::Text { text: piece },
                } if c == cell => text.push_str(&piece),
                WorkerFrame::CellDone { cell: c, outcome } if c == cell => return (text, outcome),
                other => panic!("unexpected frame before cell {cell} ended: {other:?}"),
            }
        }
    }

    /// Runs one cell that makes no nested calls: (output, final value or
    /// failure message).
    fn run(&mut self, cell: u64, source: &str) -> (String, Result<String, String>) {
        self.start(cell, source, None);
        let (text, outcome) = self.finish(cell);
        (text, result(outcome))
    }

    /// The next `NestedCall`: its call id, tool name and input.
    fn nested_call(&mut self, cell: u64) -> (u64, String, CellValue) {
        match self.recv() {
            WorkerFrame::NestedCall {
                cell: c,
                call,
                name,
                input,
            } if c == cell => (call, name, input),
            other => panic!("expected a nested call, got {other:?}"),
        }
    }

    fn deliver(&mut self, cell: u64, call: u64, outcome: Result<CellValue, String>) {
        self.send(&HostFrame::NestedResult {
            cell,
            call,
            outcome,
        });
    }

    fn shutdown(mut self) -> Option<i32> {
        self.send(&HostFrame::Shutdown);
        assert!(matches!(self.recv(), WorkerFrame::ResourceReport { .. }));
        self.exit_code()
    }

    fn exit_code(mut self) -> Option<i32> {
        drop(self.stdin.take());
        self.child.wait().expect("wait").code()
    }
}

fn configure(revision: &str, sdk_stubs: Option<String>) -> HostFrame {
    HostFrame::Configure {
        revision: revision.to_owned(),
        engine: EngineConfig::MONTY,
        sdk_stubs,
    }
}

fn result(outcome: CellOutcome) -> Result<String, String> {
    match outcome {
        CellOutcome::Completed { value, .. } => Ok(value.expect("a rendered value")),
        CellOutcome::Failed {
            message,
            generation_lost: false,
            ..
        } => Err(message),
        other => panic!("unexpected outcome {other:?}"),
    }
}

fn accepted(cell: u64, call: u64) -> WorkerFrame {
    WorkerFrame::Delivery {
        cell,
        call,
        verdict: DeliveryVerdict::Accepted,
    }
}

fn text(cell: u64, text: &str) -> WorkerFrame {
    WorkerFrame::Output {
        cell,
        item: OutputItem::Text {
            text: text.to_owned(),
        },
    }
}

fn dict(entries: &[(&str, CellValue)]) -> CellValue {
    CellValue::Dict(
        entries
            .iter()
            .map(|(key, value)| ((*key).to_owned(), value.clone()))
            .collect(),
    )
}

#[test]
fn hello_names_micro_monty_and_shutdown_reports_usage() {
    let mut worker = Worker::spawn();
    assert_eq!(
        worker.recv(),
        WorkerFrame::Hello {
            revision: CELL_PROTOCOL_REVISION.to_owned(),
            engine: "micro-monty".to_owned(),
            adapter: "micro-monty-cell-adapter.v1".to_owned(),
        }
    );
    worker.send(&configure(CELL_PROTOCOL_REVISION, None));
    assert_eq!(worker.shutdown(), Some(0));
}

/// The `Seccomp:` mode in `/proc/<pid>/status` (0 none, 2 filter).
#[cfg(target_os = "linux")]
fn seccomp_mode(worker: &Worker) -> String {
    let status =
        std::fs::read_to_string(format!("/proc/{}/status", worker.child.id())).expect("status");
    status
        .lines()
        .find_map(|line| line.strip_prefix("Seccomp:"))
        .expect("a Seccomp line")
        .trim()
        .to_owned()
}

#[cfg(target_os = "linux")]
#[test]
fn after_the_handshake_the_worker_runs_under_its_seccomp_filter() {
    for opt_out in [false, true] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_mm-cell-worker"));
        command.env_remove("MM_CELL_WORKER_NO_SECCOMP");
        if opt_out {
            command.env("MM_CELL_WORKER_NO_SECCOMP", "1");
        }
        let mut worker = Worker::spawn_with(command);
        assert!(matches!(worker.recv(), WorkerFrame::Hello { .. }));
        assert_eq!(seccomp_mode(&worker), "0");
        worker.send(&configure(CELL_PROTOCOL_REVISION, None));
        // Threads, a big heap and printing all work under the filter.
        let (out, value) = worker.run(
            0,
            "import random\ns = 'x' * (64 << 20)\nprint(len(s))\nrandom.random() < 1",
        );
        assert_eq!((out.as_str(), value), ("67108864\n", Ok("True".to_owned())));
        assert_eq!(seccomp_mode(&worker), if opt_out { "0" } else { "2" });
        worker.send(&HostFrame::Shutdown);
        let WorkerFrame::ResourceReport { usage } = worker.recv() else {
            panic!("no resource report");
        };
        assert!(
            usage.rss_bytes.is_some() && usage.threads.is_some(),
            "{usage:?}"
        );
        assert!(
            usage.fds.is_some() && usage.peak_rss_bytes.is_some(),
            "{usage:?}"
        );
        assert_eq!(worker.exit_code(), Some(0));
    }
}

#[test]
fn a_final_int_past_the_digit_limit_renders_as_its_bit_count() {
    let mut worker = Worker::configured();
    let started = std::time::Instant::now();
    assert_eq!(
        worker.run(0, "big = 1 << 64_000_000\nbig").1,
        Ok("<int with 64000001 bits>".to_owned())
    );
    assert_eq!(
        worker
            .run(1, "b = -(1 << 20_000)\n[b, 7, (b,), {'k': b}]")
            .1,
        Ok(
            "[<int with 20001 bits>, 7, (<int with 20001 bits>,), {'k': <int with 20001 bits>}]"
                .to_owned()
        )
    );
    assert!(started.elapsed() < Duration::from_secs(10));
    // Inside the cell, CPython's limit stands.
    let (_, failed) = worker.run(2, "str(big)");
    assert!(
        failed
            .as_ref()
            .is_err_and(|m| m.contains("ValueError: Exceeds the limit (4300 digits)")),
        "{failed:?}"
    );
    // 14,284 bits still render as digits (at most 4300 of them).
    let (_, digits) = worker.run(3, "(1 << 14_284) - 1");
    assert_eq!(digits.map(|d| d.len()), Ok(4300));
    assert_eq!(worker.shutdown(), Some(0));
}

#[test]
fn sdk_stubs_are_accepted_and_cells_run_unchecked() {
    let mut worker = Worker::spawn();
    assert!(matches!(worker.recv(), WorkerFrame::Hello { .. }));
    worker.send(&configure(
        CELL_PROTOCOL_REVISION,
        Some("def run_py() -> int: ...".into()),
    ));
    // A checker would refuse `len(1)`; unchecked, it fails when it runs.
    let (_, failed) = worker.run(0, "len(1)");
    assert!(
        failed.as_ref().is_err_and(|m| m.contains("TypeError")),
        "{failed:?}"
    );
    assert_eq!(worker.run(1, "1 + 1").1, Ok("2".into()));
    assert_eq!(worker.shutdown(), Some(0));
}

#[test]
fn other_revisions_and_a_repeated_configure_are_protocol_losses() {
    for frames in [
        vec![configure("plakous-cell-protocol.v1", None)],
        vec![
            configure(CELL_PROTOCOL_REVISION, None),
            configure(CELL_PROTOCOL_REVISION, None),
        ],
    ] {
        let mut worker = Worker::spawn();
        assert!(matches!(worker.recv(), WorkerFrame::Hello { .. }));
        for frame in &frames {
            worker.send(frame);
        }
        assert!(
            matches!(
                worker.recv(),
                WorkerFrame::HostLost {
                    class: LossClass::ProtocolViolation,
                    ..
                }
            ),
            "{frames:?}"
        );
        assert_eq!(worker.exit_code(), Some(4));
    }
}

#[test]
fn the_bootstrap_installs_once_and_a_cell_awaits_one_nested_call() {
    let mut worker = Worker::configured();
    worker.start(
        0,
        "reply = await fetch({'q': 'ping'})\nprint('got', reply)\nreply",
        Some(bootstrap()),
    );
    let (call, name, input) = worker.nested_call(0);
    assert_eq!(name, "fetch_tool");
    assert_eq!(input, dict(&[("q", CellValue::Str("ping".into()))]));
    assert_eq!(
        worker.recv(),
        WorkerFrame::Waiting {
            cell: 0,
            calls: vec![call]
        }
    );
    worker.deliver(0, call, Ok(CellValue::Str("pong".into())));
    assert_eq!(worker.recv(), accepted(0, call));
    assert_eq!(worker.recv(), text(0, "got pong\n"));
    let (_, outcome) = worker.finish(0);
    assert_eq!(result(outcome), Ok("pong".to_owned()));

    // Later cells use the installed wrappers; a second bootstrap is refused
    // like plak's Monty worker refuses it, and loses the generation.
    worker.start(1, "await patch('line 1\\nline 2')", None);
    let (call, name, input) = worker.nested_call(1);
    assert_eq!(
        (name.as_str(), input),
        ("patch_tool", CellValue::Str("line 1\nline 2".into()))
    );
    assert!(matches!(worker.recv(), WorkerFrame::Waiting { .. }));
    worker.deliver(1, call, Ok(CellValue::Str("applied".into())));
    assert_eq!(worker.recv(), accepted(1, call));
    assert_eq!(result(worker.finish(1).1), Ok("applied".to_owned()));
    worker.start(2, "1", Some(bootstrap()));
    assert!(matches!(
        worker.finish(2).1,
        CellOutcome::Failed { ref message, generation_lost: true, .. }
            if message == "Python tool bootstrap repeated"
    ));
    assert_eq!(worker.shutdown(), Some(0));
}

#[test]
fn a_gather_answered_out_of_order_in_two_batches_is_accepted_once_consumed() {
    let mut worker = Worker::configured();
    worker.start(
        0,
        "import asyncio\nprint(await asyncio.gather(fetch({'i': 0}), fetch({'i': 1}), fetch({'i': 2})))",
        Some(bootstrap()),
    );
    let calls: Vec<u64> = (0..3)
        .map(|i| {
            let (call, _, input) = worker.nested_call(0);
            assert_eq!(input, dict(&[("i", CellValue::Int(i))]));
            call
        })
        .collect();
    assert_eq!(
        worker.recv(),
        WorkerFrame::Waiting {
            cell: 0,
            calls: calls.clone()
        }
    );
    // The first batch completes nothing the cell waits on: no verdict yet.
    worker.deliver(0, calls[2], Ok(CellValue::Str("r2".into())));
    worker.deliver(0, calls[0], Ok(CellValue::Str("r0".into())));
    // A repeat and an unknown id are refused; the refusals are the next
    // frames, so nothing was accepted early.
    worker.deliver(0, calls[2], Ok(CellValue::Null));
    worker.deliver(0, 999, Ok(CellValue::Null));
    worker.deliver(7, calls[1], Ok(CellValue::Null));
    for call in [calls[2], 999, calls[1]] {
        assert!(matches!(
            worker.recv(),
            WorkerFrame::Delivery { call: c, verdict: DeliveryVerdict::Refused(_), .. } if c == call
        ));
    }
    worker.deliver(0, calls[1], Ok(CellValue::Str("r1".into())));
    for call in &calls {
        assert_eq!(worker.recv(), accepted(0, *call));
    }
    assert_eq!(worker.recv(), text(0, "['r0', 'r1', 'r2']\n"));
    assert_eq!(result(worker.finish(0).1), Ok("None".to_owned()));
    // A result for a cell that ended was never observed.
    worker.deliver(0, calls[0], Ok(CellValue::Null));
    assert!(matches!(
        worker.recv(),
        WorkerFrame::Delivery {
            verdict: DeliveryVerdict::NotObserved(_),
            ..
        }
    ));
    assert_eq!(worker.shutdown(), Some(0));
}

#[test]
fn the_private_function_called_from_sync_code_dispatches_at_once() {
    // plak's wrappers are coroutines; the private host function itself is
    // answered with a future, so a plain function can make the call and a
    // later `await` collects it.
    let mut worker = Worker::configured();
    worker.start(
        0,
        "def start(n):\n    return [_plakous_host_0({'n': i}) for i in range(n)]\n\
         pending = start(2)\nprint('dispatched')\nprint(await pending[1], await pending[0])",
        Some(bootstrap()),
    );
    let (first, _, _) = worker.nested_call(0);
    let (second, _, _) = worker.nested_call(0);
    assert_eq!(worker.recv(), text(0, "dispatched\n"));
    assert_eq!(
        worker.recv(),
        WorkerFrame::Waiting {
            cell: 0,
            calls: vec![second]
        }
    );
    worker.deliver(0, first, Ok(CellValue::Int(0)));
    worker.deliver(0, second, Ok(CellValue::Int(1)));
    assert_eq!(worker.recv(), accepted(0, second));
    // The first result was already delivered: the next wait completes at
    // once and the cell consumes it.
    assert_eq!(
        worker.recv(),
        WorkerFrame::Waiting {
            cell: 0,
            calls: vec![first]
        }
    );
    assert_eq!(worker.recv(), accepted(0, first));
    assert_eq!(worker.recv(), text(0, "1 0\n"));
    assert_eq!(result(worker.finish(0).1), Ok("None".to_owned()));
    assert_eq!(worker.shutdown(), Some(0));
}

#[test]
fn stdout_and_stderr_interleave_in_order_across_a_suspension() {
    let mut worker = Worker::configured();
    worker.start(
        0,
        "import sys\nprint('one')\nprint('two', file=sys.stderr)\nr = await fetch({})\n\
         print('three', r)\nsys.stderr.write('four\\n')\nprint('five')",
        Some(bootstrap()),
    );
    assert_eq!(worker.recv(), text(0, "one\n"));
    assert_eq!(worker.recv(), text(0, "two\n"));
    let (call, _, _) = worker.nested_call(0);
    assert!(matches!(worker.recv(), WorkerFrame::Waiting { .. }));
    worker.deliver(0, call, Ok(CellValue::Int(3)));
    assert_eq!(worker.recv(), accepted(0, call));
    let (output, outcome) = worker.finish(0);
    assert_eq!(output, "three 3\nfour\nfive\n");
    assert_eq!(result(outcome), Ok("None".to_owned()));
    assert_eq!(worker.shutdown(), Some(0));
}

#[test]
fn globals_persist_across_cells_and_reset_clears_them() {
    let mut worker = Worker::configured();
    worker.start(
        0,
        "x = 41\nasync def twice(v):\n    return 2 * await fetch({'v': v})",
        Some(bootstrap()),
    );
    assert_eq!(result(worker.finish(0).1), Ok("None".to_owned()));
    assert_eq!(worker.run(1, "x += 1\nx").1, Ok("42".to_owned()));
    worker.start(2, "await twice(x)", None);
    let (call, _, input) = worker.nested_call(2);
    assert_eq!(input, dict(&[("v", CellValue::Int(42))]));
    assert!(matches!(worker.recv(), WorkerFrame::Waiting { .. }));
    worker.deliver(2, call, Ok(CellValue::Int(21)));
    assert_eq!(worker.recv(), accepted(2, call));
    assert_eq!(result(worker.finish(2).1), Ok("42".to_owned()));

    worker.send(&HostFrame::Reset);
    let (_, failure) = worker.run(3, "x");
    let failure = failure.expect_err("x is gone");
    assert!(
        failure.ends_with("NameError: name 'x' is not defined"),
        "{failure}"
    );
    // Reset forgets the bootstrap too, so the next cell may install one.
    worker.start(4, "await fetch({})", Some(bootstrap()));
    let (call, _, _) = worker.nested_call(4);
    assert!(matches!(worker.recv(), WorkerFrame::Waiting { .. }));
    worker.deliver(4, call, Ok(CellValue::Str("fresh".into())));
    assert_eq!(worker.recv(), accepted(4, call));
    assert_eq!(result(worker.finish(4).1), Ok("fresh".to_owned()));
    assert_eq!(worker.shutdown(), Some(0));
}

const LONG_LOOP: &str = "total = 0\nfor i in range(3_000_000):\n    total += i % 7\n    if i % 1_000_000 == 0:\n        print('at', i)\ntotal";

#[test]
fn a_yield_window_parks_a_long_loop_and_resume_finishes_it_identically() {
    let mut worker = Worker::configured();
    let (plain_output, plain) = worker.run(0, LONG_LOOP);
    assert_eq!(plain, Ok("8999994".to_owned()));
    worker.start_within(1, LONG_LOOP, None, Some(20));
    let mut output = String::new();
    let mut yields = 0;
    let value = loop {
        match worker.recv() {
            WorkerFrame::Yielded { cell: 1 } => {
                yields += 1;
                worker.send(&HostFrame::Resume {
                    cell: 1,
                    window: 20,
                });
            }
            WorkerFrame::Output {
                cell: 1,
                item: OutputItem::Text { text },
            } => output.push_str(&text),
            WorkerFrame::CellDone { cell: 1, outcome } => break result(outcome),
            other => panic!("unexpected {other:?}"),
        }
    };
    assert!(yields > 0, "the loop never parked");
    assert_eq!((output, value), (plain_output, plain.clone()));
    // The next cell runs without a window: no stale deadline parks it.
    assert_eq!(worker.run(2, LONG_LOOP).1, plain);
    assert_eq!(worker.shutdown(), Some(0));
}

#[test]
fn a_cell_waiting_on_the_host_yields_when_its_window_runs_out() {
    let mut worker = Worker::configured();
    worker.start_within(0, "print(await fetch({}))", Some(bootstrap()), Some(30));
    let (call, _, _) = worker.nested_call(0);
    assert!(matches!(worker.recv(), WorkerFrame::Waiting { .. }));
    assert_eq!(worker.recv(), WorkerFrame::Yielded { cell: 0 });
    // A result while yielded is consumed (and accepted) without running
    // the cell on: it parks at once and stays yielded until `Resume`.
    worker.deliver(0, call, Ok(CellValue::Str("late".into())));
    assert_eq!(worker.recv(), accepted(0, call));
    worker.send(&HostFrame::Resume {
        cell: 0,
        window: 10_000,
    });
    let (output, outcome) = worker.finish(0);
    assert_eq!(
        (output.as_str(), result(outcome)),
        ("late\n", Ok("None".to_owned()))
    );
    assert_eq!(worker.shutdown(), Some(0));
}

#[test]
fn cancel_ends_a_running_waiting_or_yielded_cell_and_the_worker_lives_on() {
    let cancelled = Err("cell cancelled by the host".to_owned());
    let mut worker = Worker::configured();
    // Computing: the reader thread parks the engine.
    worker.start(0, "n = 0\nwhile True:\n    n += 1", Some(bootstrap()));
    std::thread::sleep(Duration::from_millis(100));
    worker.send(&HostFrame::Cancel { cell: 0 });
    assert_eq!(result(worker.finish(0).1), cancelled);
    assert_eq!(worker.run(1, "n > 0").1, Ok("True".to_owned()));
    // Waiting on the host: a later result is declined as never observed.
    worker.start(2, "print('before')\nawait fetch({})\nprint('after')", None);
    assert_eq!(worker.recv(), text(2, "before\n"));
    let (call, _, _) = worker.nested_call(2);
    assert!(matches!(worker.recv(), WorkerFrame::Waiting { .. }));
    worker.send(&HostFrame::Cancel { cell: 2 });
    assert_eq!(result(worker.finish(2).1), cancelled);
    worker.deliver(2, call, Ok(CellValue::Null));
    assert!(matches!(
        worker.recv(),
        WorkerFrame::Delivery { verdict: DeliveryVerdict::NotObserved(_), call: c, .. } if c == call
    ));
    // Parked at a yield.
    worker.start_within(3, "while True:\n    pass", None, Some(10));
    assert_eq!(worker.recv(), WorkerFrame::Yielded { cell: 3 });
    worker.send(&HostFrame::Cancel { cell: 3 });
    assert_eq!(result(worker.finish(3).1), cancelled);
    // A stale cancel changes nothing.
    worker.send(&HostFrame::Cancel { cell: 3 });
    assert_eq!(worker.run(4, "1 + 1").1, Ok("2".to_owned()));
    // A cancel right behind its start, even while the bootstrap runs, still
    // stops the cell, and the bootstrap stays installed.
    worker.send(&HostFrame::Reset);
    worker.start(5, "while True:\n    pass", Some(bootstrap()));
    worker.send(&HostFrame::Cancel { cell: 5 });
    assert_eq!(result(worker.finish(5).1), cancelled);
    worker.start(6, "await fetch({})", None);
    assert!(matches!(worker.nested_call(6).1.as_str(), "fetch_tool"));
    assert!(matches!(worker.recv(), WorkerFrame::Waiting { .. }));
    worker.send(&HostFrame::Cancel { cell: 6 });
    assert_eq!(result(worker.finish(6).1), cancelled);
    assert_eq!(worker.shutdown(), Some(0));
}

#[test]
fn out_of_bounds_arguments_raise_plaks_exceptions_without_dispatch() {
    let mut worker = Worker::configured();
    let source = r"
deep = 1
for _ in range(200):
    deep = [deep]
nested = 1
for _ in range(40):
    nested = [nested]
shared = [1]
for _ in range(40):
    shared = [shared, shared]
cases = [
    ('list', lambda: fetch([])),
    ('none', lambda: fetch(None)),
    ('two', lambda: _plakous_host_0({}, {})),
    ('kw', lambda: _plakous_host_0(arguments={})),
    ('str', lambda: patch({})),
    ('keys', lambda: fetch({1: 'x'})),
    ('size', lambda: fetch({'x': 'a' * (1 << 20)})),
    ('shared', lambda: fetch({'x': shared})),
    ('depth40', lambda: fetch({'x': nested})),
    ('depth128', lambda: fetch({'x': deep})),
]
for label, make in cases:
    try:
        await make()
    except Exception as error:
        print(label, type(error).__name__, error)
";
    worker.start(0, source, Some(bootstrap()));
    let (output, outcome) = worker.finish(0);
    assert_eq!(result(outcome), Ok("None".to_owned()));
    assert_eq!(
        output,
        "list TypeError Python tool requires exactly one dictionary argument\n\
         none TypeError Python tool requires exactly one dictionary argument\n\
         two TypeError Python tool requires exactly one dictionary argument\n\
         kw TypeError Python tool requires exactly one dictionary argument\n\
         str TypeError Python tool requires exactly one string argument\n\
         keys ValueError Python tool argument dict keys must be str\n\
         size ValueError Python tool argument exceeds 1 MiB\n\
         shared ValueError Python tool argument exceeds 1 MiB\n\
         depth40 ValueError Python tool argument nests deeper than 40 levels\n\
         depth128 ValueError Python tool argument nests deeper than 128 levels\n"
    );
    assert_eq!(worker.shutdown(), Some(0));
}

#[test]
fn values_without_a_json_form_cross_as_their_repr_and_results_come_back_typed() {
    let mut worker = Worker::configured();
    worker.start(
        0,
        "r = await fetch({'s': {3}, 't': (1, 2.5), 'f': len, 'n': None, 'b': b\"it's\\x00\", 'c': b'\\\\\\t', 'q': b'\\'\"'})\nprint(r['k'], r['l'])\nr['k']",
        Some(bootstrap()),
    );
    let (call, _, input) = worker.nested_call(0);
    assert_eq!(
        input,
        // JSON objects keep serde_json's (sorted) key order, as in plak.
        dict(&[
            ("b", CellValue::Str("b\"it's\\x00\"".into())),
            ("c", CellValue::Str("b'\\\\\\t'".into())),
            ("f", CellValue::Str("<built-in function len>".into())),
            ("n", CellValue::Null),
            ("q", CellValue::Str("b'\\'\"'".into())),
            ("s", CellValue::Str("{3}".into())),
            (
                "t",
                CellValue::List(vec![CellValue::Int(1), CellValue::Float(2.5)])
            ),
        ])
    );
    assert!(matches!(worker.recv(), WorkerFrame::Waiting { .. }));
    worker.deliver(
        0,
        call,
        Ok(dict(&[
            ("k", CellValue::BigInt("123".into())),
            ("l", CellValue::List(vec![CellValue::Bool(true)])),
        ])),
    );
    assert_eq!(worker.recv(), accepted(0, call));
    assert_eq!(worker.recv(), text(0, "123 [True]\n"));
    assert_eq!(result(worker.finish(0).1), Ok("123".to_owned()));
    assert_eq!(worker.shutdown(), Some(0));
}

#[test]
fn nested_failures_raise_runtime_error_and_an_undecodable_result_loses_the_worker() {
    let mut worker = Worker::configured();
    worker.start(
        0,
        "try:\n    await fetch({})\nexcept RuntimeError as e:\n    print(type(e).__name__, e)\nawait fetch({})",
        Some(bootstrap()),
    );
    let (call, _, _) = worker.nested_call(0);
    assert!(matches!(worker.recv(), WorkerFrame::Waiting { .. }));
    worker.deliver(0, call, Err("denied by host".into()));
    assert_eq!(worker.recv(), accepted(0, call));
    assert_eq!(worker.recv(), text(0, "RuntimeError denied by host\n"));
    let (call, _, _) = worker.nested_call(0);
    assert!(matches!(worker.recv(), WorkerFrame::Waiting { .. }));
    // Above i64::MAX: py-worker cannot hand it to the interpreter either.
    worker.deliver(0, call, Ok(CellValue::BigInt(u64::MAX.to_string())));
    assert!(matches!(
        worker.recv(),
        WorkerFrame::Delivery { verdict: DeliveryVerdict::NotObserved(_), call: c, .. } if c == call
    ));
    assert!(matches!(
        worker.recv(),
        WorkerFrame::HostLost {
            class: LossClass::ProtocolViolation,
            ..
        }
    ));
    assert_eq!(worker.exit_code(), Some(4));
}

#[test]
fn outstanding_calls_are_bounded_at_sixteen() {
    let mut worker = Worker::configured();
    worker.start(
        0,
        "import asyncio\nawait asyncio.gather(*[fetch({'n': n}) for n in range(17)])",
        Some(bootstrap()),
    );
    let calls: Vec<u64> = (0..16).map(|_| worker.nested_call(0).0).collect();
    let (_, outcome) = worker.finish(0);
    let failure = result(outcome).expect_err("the 17th call raises");
    assert!(
        failure.ends_with(
            "RuntimeError: local composition exceeds its profile-bound limit of 16 outstanding nested calls"
        ),
        "{failure}"
    );
    worker.deliver(0, calls[0], Ok(CellValue::Null));
    assert!(matches!(
        worker.recv(),
        WorkerFrame::Delivery {
            verdict: DeliveryVerdict::NotObserved(_),
            ..
        }
    ));
    assert_eq!(worker.shutdown(), Some(0));
}

#[test]
fn values_and_output_are_capped_at_one_mib_and_memory_errors_are_ordinary() {
    let mut worker = Worker::configured();
    let cap = 1_048_576 - 64;
    let (output, value) = worker.run(0, "print('x' * (2 << 20))\nprint('never')");
    assert!(output.ends_with("\n[Python output truncated at 1 MiB]\n"));
    assert_eq!(output.len(), cap);
    assert_eq!(value, Ok("None".to_owned()));
    let (_, value) = worker.run(1, "a = [1]\nfor _ in range(40):\n    a = [a, a]\na");
    let value = value.expect("a value");
    assert!(value.ends_with("\n[Python result truncated at 1 MiB]"));
    assert!(value.len() <= cap);
    assert_eq!(worker.run(2, "'text'").1, Ok("text".to_owned()));
    let (_, failure) = worker.run(3, "x = 'a' * (300 * 1024 * 1024)");
    assert!(failure.expect_err("too big").ends_with("MemoryError"));
    assert_eq!(worker.run(4, "1 + 1").1, Ok("2".to_owned()));
    let (_, failure) = worker.run(5, "raise ValueError('boom')");
    assert_eq!(
        failure,
        Err("Traceback (most recent call last):\n  File \"plak-py-ptc.py\", line 1, in <module>\n    \
             raise ValueError('boom')\nValueError: boom"
            .to_owned())
    );
    assert_eq!(worker.shutdown(), Some(0));
}

#[test]
fn a_host_future_from_an_earlier_cell_raises_instead_of_losing_the_worker() {
    // The call is not asked for again: cell end forgot it, so the host
    // could not answer it. Awaiting it raises, and the worker goes on.
    let mut worker = Worker::configured();
    worker.start(0, "f = _plakous_host_0({'q': 'later'})", Some(bootstrap()));
    let (stale, _, _) = worker.nested_call(0);
    assert_eq!(result(worker.finish(0).1), Ok("None".to_owned()));
    let (_, failure) = worker.run(1, "await f");
    let failure = failure.expect_err("nothing can answer f");
    assert!(
        failure.ends_with("RuntimeError: every task is waiting and no host call is pending"),
        "{failure}"
    );
    worker.deliver(0, stale, Ok(CellValue::Null));
    assert!(matches!(
        worker.recv(),
        WorkerFrame::Delivery {
            verdict: DeliveryVerdict::NotObserved(_),
            ..
        }
    ));
    assert_eq!(worker.run(2, "1 + 1").1, Ok("2".to_owned()));
    assert_eq!(worker.shutdown(), Some(0));
}
