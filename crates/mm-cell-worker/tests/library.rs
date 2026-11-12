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
    sync::Arc,
    thread::{self, JoinHandle},
};

use mm_cell_worker::{Config, Kill, NativeEngine, NoHooks, Options, ServeError};
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
        let (worker_in, to_worker) = std::io::pipe().expect("pipe");
        let (from_worker, worker_out) = std::io::pipe().expect("pipe");
        let worker = thread::Builder::new()
            .stack_size(16 << 20)
            .spawn(move || {
                mm_cell_worker::serve(
                    Box::new(worker_in),
                    Box::new(worker_out),
                    &NativeEngine::default(),
                    options,
                )
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
