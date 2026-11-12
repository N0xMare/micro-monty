//! `mm-cell-worker`: the `mm_cell_worker` library's loop as a child process.
//!
//! Speaks the cell protocol on stdin and stdout; diagnostics go to stderr.
//! What the binary adds to the library: the stdio transport, a seccomp
//! filter after the handshake (`seccomp.rs`, the crate's one `unsafe`
//! exception, docs/spec.md §5), its own resource report, the crash-cut
//! test hooks, and a last-gasp `HostLost` frame on a panic.
//!
//! `mm-cell-worker --version` prints the engine version and the snapshot
//! `build_id` (hex) instead of serving.

use std::{io, panic, process::ExitCode, sync::Arc, thread};

use mm_cell_worker::{Hooks, NativeEngine, Options, ServeError};
use plakous_cell::{LossClass, ProcessResourceUsage, WorkerFrame, write_frame_blocking};

#[cfg(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
mod seccomp;
/// Elsewhere there is no filter (docs/spec.md §10.2 names Linux seccomp).
#[cfg(not(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
)))]
#[allow(
    clippy::unused_self,
    clippy::unnecessary_wraps,
    reason = "the same signatures as the Linux module"
)]
mod seccomp {
    pub(crate) struct Report;
    impl Report {
        pub(crate) fn prepare() -> Report {
            Report
        }
        pub(crate) fn sample(&self) -> plakous_cell::ProcessResourceUsage {
            plakous_cell::sample_self_rusage()
        }
    }
    pub(crate) fn install(_report: &Report, _out_fds: &[u32]) -> Result<(), String> {
        Ok(())
    }
}
#[cfg(all(feature = "test-hooks", debug_assertions))]
mod test_hooks;
#[cfg(not(all(feature = "test-hooks", debug_assertions)))]
mod test_hooks {
    pub(crate) fn boundary(_name: &str) {}
    pub(crate) fn probe() -> bool {
        false
    }
    pub(crate) fn prepare() -> Vec<u32> {
        Vec::new()
    }
}

/// The binary's hooks: seccomp after the handshake, the process's usage,
/// and the crash cuts.
struct Process {
    /// The resource report's sources, opened before the seccomp filter.
    report: seccomp::Report,
}

impl Hooks for Process {
    fn confine(&self) -> Result<(), String> {
        seccomp::install(&self.report, &test_hooks::prepare())
    }

    fn usage(&self) -> ProcessResourceUsage {
        self.report.sample()
    }

    fn boundary(&self, name: &str) {
        test_hooks::boundary(name);
    }
}

/// Native stack for the worker thread; the engine's own recursion is
/// depth-counted, so this is headroom, reserved rather than committed.
const WORKER_STACK_SIZE: usize = 16 * 1024 * 1024;

fn main() -> ExitCode {
    if std::env::args().nth(1).as_deref() == Some("--version") {
        let id = micro_monty::build_id()
            .iter()
            .fold(String::new(), |mut id, b| {
                let _ = std::fmt::Write::write_fmt(&mut id, format_args!("{b:02x}"));
                id
            });
        println!("mm-cell-worker {}\nbuild_id {id}", micro_monty::VERSION);
        return ExitCode::SUCCESS;
    }
    if test_hooks::probe() {
        return ExitCode::SUCCESS;
    }
    install_panic_hook();
    match thread::Builder::new()
        .name("mm-cell-worker".to_owned())
        .stack_size(WORKER_STACK_SIZE)
        .spawn(serve)
    {
        // A panic has already written its loss frame in the hook.
        Ok(worker) => worker.join().unwrap_or(ExitCode::from(101)),
        Err(error) => {
            eprintln!("mm-cell-worker: failed to spawn the worker thread: {error}");
            ExitCode::FAILURE
        }
    }
}

/// The worker on its large-stack thread.
fn serve() -> ExitCode {
    let options = Options {
        hooks: Arc::new(Process {
            report: seccomp::Report::prepare(),
        }),
        ..Options::default()
    };
    // One session per process: images store their host code, so one
    // restored in a new process need not compile it again. (plak's
    // bootstrap sends untyped tool names, which generate no code, and the
    // worker takes no snapshots yet, so this changes nothing today.)
    match mm_cell_worker::serve(
        Box::new(io::stdin()),
        Box::new(io::stdout()),
        &NativeEngine::default().with_host_code_in_snapshots(true),
        options,
    ) {
        Ok(()) => ExitCode::SUCCESS,
        Err(ServeError::Hello(_)) => ExitCode::from(3),
        Err(ServeError::Lost(message)) => {
            eprintln!("mm-cell-worker: {message}");
            ExitCode::from(4)
        }
    }
}

/// Writes a best-effort `HostLost` frame before the default panic handling,
/// so the parent gets a parseable last gasp. A stream that ends without a
/// terminal is a loss for the parent either way.
fn install_panic_hook() {
    let default_hook = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        let _ = write_frame_blocking(
            &mut io::stdout(),
            &WorkerFrame::HostLost {
                class: LossClass::EngineFailure,
                message: format!("micro-monty worker panicked: {info}"),
            },
        );
        default_hook(info);
    }));
}
