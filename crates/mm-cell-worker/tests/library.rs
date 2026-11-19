//! The library over an in-process pipe, as a host that runs the worker on a
//! thread of its own process does: `Config` values reach the cells, and a
//! `Kill` ends a computing cell's worker without a process to signal.

#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "test setup failures should abort the test"
)]

use std::{
    io::{PipeReader, PipeWriter, Write},
    sync::{Arc, Mutex},
    thread::{self, JoinHandle},
    time::Instant,
};

use micro_monty::{HostModule, Progress, Resume};
use mm_cell_worker::{
    CellEngine, CellSession, Config, Kill, NativeEngine, NoHooks, Options, Park, ServeError,
    SessionSettings,
};
use plakous_cell::{
    BindingInput, CELL_PROTOCOL_REVISION, CellOutcome, CellPrelude, EngineConfig, HostFrame,
    LossClass, MAX_FRAME_BYTES, MontyBinding, MontyBootstrap, OutputItem, WorkerFrame,
    encode_frame, read_frame_blocking,
};

struct Thread {
    to_worker: PipeWriter,
    from_worker: PipeReader,
    worker: JoinHandle<Result<(), ServeError>>,
}

impl Thread {
    fn start(options: Options) -> Self {
        Self::start_with(options, NativeEngine::default())
    }

    fn start_with(options: Options, engine: impl CellEngine + Send + 'static) -> Self {
        let (worker_in, to_worker) = std::io::pipe().expect("pipe");
        let (from_worker, worker_out) = std::io::pipe().expect("pipe");
        let worker = thread::Builder::new()
            .stack_size(16 << 20)
            .spawn(move || {
                mm_cell_worker::serve(Box::new(worker_in), Box::new(worker_out), &engine, options)
            })
            .expect("worker thread");
        let mut this = Self {
            to_worker,
            from_worker,
            worker,
        };
        assert!(matches!(this.recv(), WorkerFrame::Hello { .. }));
        this.send(&HostFrame::Configure {
            revision: CELL_PROTOCOL_REVISION.to_owned(),
            engine: EngineConfig::MONTY,
            sdk_stubs: None,
        });
        this
    }

    fn send(&mut self, frame: &HostFrame) {
        let bytes = encode_frame(frame).expect("encode");
        self.to_worker.write_all(&bytes).expect("write frame");
    }

    fn recv(&mut self) -> WorkerFrame {
        let body = read_frame_blocking(&mut self.from_worker, MAX_FRAME_BYTES)
            .expect("read frame")
            .expect("a frame before end of stream");
        serde_json::from_slice(&body).expect("decode worker frame")
    }

    fn start_cell(&mut self, cell: u64, source: &str, bootstrap: Option<MontyBootstrap>) {
        self.send(&HostFrame::StartCell {
            cell,
            source: source.to_owned(),
            prelude: CellPrelude::Monty { bootstrap },
            window: None,
        });
    }

    /// The cell's output and its outcome; `calls` nested calls come first.
    fn finish(&mut self, cell: u64, mut calls: usize) -> (String, CellOutcome) {
        let mut text = String::new();
        loop {
            match self.recv() {
                WorkerFrame::NestedCall { .. } if calls > 0 => calls -= 1,
                WorkerFrame::Output {
                    item: OutputItem::Text { text: piece },
                    ..
                } => text.push_str(&piece),
                WorkerFrame::CellDone { cell: c, outcome } if c == cell && calls == 0 => {
                    return (text, outcome);
                }
                other => panic!("unexpected frame: {other:?}"),
            }
        }
    }
}

fn failure(outcome: CellOutcome) -> String {
    match outcome {
        CellOutcome::Failed { message, .. } => message,
        other => panic!("expected a failure, got {other:?}"),
    }
}

#[test]
fn config_values_reach_the_cells() {
    let mut worker = Thread::start(Options {
        config: Config {
            script_name: "custom.py".to_owned(),
            max_outstanding_nested: 2,
            outstanding_nested_error: "two at most".to_owned(),
            max_print_bytes: 100,
            print_notice: "[cut]".to_owned(),
            ..Config::default()
        },
        ..Options::default()
    });
    let bootstrap = MontyBootstrap {
        source: "async def fetch(arguments):\n    return await _plakous_host_0(arguments)\n"
            .to_owned(),
        bindings: vec![MontyBinding {
            function: "_plakous_host_0".into(),
            tool: "fetch_tool".into(),
            input: BindingInput::Dictionary,
        }],
    };
    // Two calls go out; the third raises the configured text.
    worker.start_cell(
        0,
        "import asyncio\nawait asyncio.gather(*[fetch({'n': n}) for n in range(3)])",
        Some(bootstrap),
    );
    let (_, outcome) = worker.finish(0, 2);
    let message = failure(outcome);
    assert!(message.ends_with("RuntimeError: two at most"), "{message}");
    assert!(message.contains("File \"custom.py\", line 2"), "{message}");
    // The print budget includes its notice.
    worker.start_cell(1, "print('x' * 500)", None);
    let (output, _) = worker.finish(1, 0);
    assert_eq!(output, format!("{}[cut]", "x".repeat(95)));
    worker.send(&HostFrame::Shutdown);
    assert!(matches!(worker.recv(), WorkerFrame::ResourceReport { .. }));
    assert_eq!(worker.worker.join().expect("join"), Ok(()));
}

#[test]
fn a_kill_ends_a_computing_cell_and_its_worker() {
    let kill = Kill::default();
    let mut worker = Thread::start(Options {
        kill: kill.clone(),
        hooks: Arc::new(NoHooks),
        ..Options::default()
    });
    worker.start_cell(0, "while True:\n    pass", None);
    kill.kill();
    match worker.recv() {
        WorkerFrame::HostLost {
            class: LossClass::EngineFailure,
            message,
        } => assert_eq!(message, "cell worker killed"),
        other => panic!("expected a loss, got {other:?}"),
    }
    assert_eq!(
        worker.worker.join().expect("join"),
        Err(ServeError::Lost("cell worker killed".to_owned()))
    );
}

#[test]
fn a_cut_exception_report_fails_its_cell_within_the_result_cap() {
    // The engine stops rendering an uncaught exception at the session's
    // text limit (the worker sets it to `max_result_bytes`): a key whose
    // `repr` is about 96 MB used to make a report too big for one frame.
    let mut worker = Thread::start(Options::default());
    worker.start_cell(
        0,
        "fs = 'f'\nfor _ in range(22):\n    fs = frozenset([fs, (fs, 1)])\n{0: 1}[fs]\n",
        None,
    );
    let (_, outcome) = worker.finish(0, 0);
    let message = failure(outcome);
    assert!(message.len() <= 1 << 20, "{}", message.len());
    assert!(message.starts_with("Traceback (most recent call last):\n"));
    assert!(message.contains("\nKeyError: frozenset({"));
    worker.start_cell(1, "print('next')", None);
    assert_eq!(worker.finish(1, 0).0, "next\n");
    worker.send(&HostFrame::Shutdown);
    assert!(matches!(worker.recv(), WorkerFrame::ResourceReport { .. }));
    assert_eq!(worker.worker.join().expect("join"), Ok(()));
}

/// A [`NativeEngine`] whose sessions log each bootstrap run (a source
/// starting `# boot`) and each `reset_to_kept` with its result.
struct Logged(NativeEngine, Arc<Mutex<Vec<String>>>);

struct LoggedSession(Box<dyn CellSession>, Arc<Mutex<Vec<String>>>);

impl CellEngine for Logged {
    fn session(&self, settings: &SessionSettings) -> Result<Box<dyn CellSession>, String> {
        Ok(Box::new(LoggedSession(
            self.0.session(settings)?,
            Arc::clone(&self.1),
        )))
    }
}

impl CellSession for LoggedSession {
    fn define(&mut self, module: &HostModule) -> Result<(), String> {
        self.0.define(module)
    }
    fn start(&mut self, source: &str, filename: &str) -> Result<Result<Progress, String>, String> {
        if source.starts_with("# boot") {
            self.1.lock().expect("log").push("bootstrap".into());
        }
        self.0.start(source, filename)
    }
    fn resume(&mut self, answer: Resume) -> Result<Progress, String> {
        self.0.resume(answer)
    }
    fn cancel(&mut self) {
        self.0.cancel();
    }
    fn reset(&mut self) -> Result<(), String> {
        self.0.reset()
    }
    fn keep(&mut self) {
        self.0.keep();
    }
    fn reset_to_kept(&mut self) -> bool {
        let done = self.0.reset_to_kept();
        self.1.lock().expect("log").push(format!("reset_to {done}"));
        done
    }
    fn set_deadline(&mut self, at: Option<Instant>) {
        self.0.set_deadline(at);
    }
    fn set_output(&mut self, out: Box<dyn FnMut(&str) + Send>) {
        self.0.set_output(out);
    }
    fn park(&self) -> Park {
        self.0.park()
    }
}

/// A bootstrap after each `Reset` runs once and is then restored, unless
/// it read entropy or the clock: then it runs every time, so each cell
/// sees its own random values.
#[test]
fn a_reset_bootstrap_runs_once_unless_it_reads_entropy_or_the_clock() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let mut worker = Thread::start_with(
        Options::default(),
        Logged(NativeEngine::default(), Arc::clone(&log)),
    );
    let bootstrap = |extra: &str| MontyBootstrap {
        source: format!("# boot\nclass T:\n    n = 0\n{extra}"),
        bindings: Vec::new(),
    };
    let mut cell = 0;
    for (extra, source) in [
        ("", "T.n += 1\nT.n"),
        ("import random\n", "random.getrandbits(64)"),
        ("import time\nT0 = time.time()\n", "T0"),
    ] {
        log.lock().expect("log").clear();
        let mut seen = Vec::new();
        for _ in 0..3 {
            worker.send(&HostFrame::Reset);
            worker.start_cell(cell, source, Some(bootstrap(extra)));
            match worker.finish(cell, 0).1 {
                CellOutcome::Completed { value, .. } => seen.push(value.expect("a value")),
                other => panic!("unexpected {other:?}"),
            }
            cell += 1;
        }
        let log = log.lock().expect("log").clone();
        if extra.is_empty() {
            assert_eq!(seen, ["1", "1", "1"]);
            assert_eq!(log, ["bootstrap", "reset_to true", "reset_to true"]);
        } else {
            assert_eq!(
                log.iter().filter(|l| *l == "bootstrap").count(),
                3,
                "{log:?}"
            );
            assert!(
                seen[0] != seen[1] && seen[1] != seen[2],
                "{extra}: {seen:?}"
            );
        }
    }
    // Kept only from a bootstrap cell that follows a reset, and a clock
    // read before the reset does not count.
    let mut run = |reset: bool, source: &str, boot: Option<&str>| {
        if reset {
            worker.send(&HostFrame::Reset);
        }
        worker.start_cell(cell, source, boot.map(bootstrap));
        cell += 1;
        format!("{:?}", worker.finish(cell - 1, 0).1)
    };
    log.lock().expect("log").clear();
    run(true, "x = 1", None);
    run(false, "x", Some("y = 2\n"));
    let fresh = run(true, "x", Some("y = 2\n"));
    assert!(fresh.contains("NameError"), "{fresh}");
    run(false, "import time\ntime.time()", None);
    run(true, "T.n", Some("z = 3\n"));
    run(true, "T.n", Some("z = 3\n"));
    assert_eq!(
        *log.lock().expect("log"),
        ["bootstrap", "bootstrap", "bootstrap", "reset_to true"]
    );
    worker.send(&HostFrame::Shutdown);
    assert!(matches!(worker.recv(), WorkerFrame::ResourceReport { .. }));
    assert_eq!(worker.worker.join().expect("join"), Ok(()));
}
