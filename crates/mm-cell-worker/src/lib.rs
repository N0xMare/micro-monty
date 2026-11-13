//! A micro-monty worker for one external cell protocol,
//! `plakous-cell-protocol.v2`, as a library. The protocol is plak's
//! (<https://github.com/N0xMare/plak>, MIT), an open-source local agent
//! harness by micro-monty's author that runs code cells in worker
//! processes; this crate is an embedding example used in local
//! integration testing against plak, not part of micro-monty's published
//! API.
//!
//! [`serve`] runs one cell worker over any byte transport: a child's stdin
//! and stdout (the `mm-cell-worker` binary) or an in-process pipe (a host
//! that runs the worker on a thread of its own process). It speaks the
//! py-ptc presentation (programmatic tool calling: globals persist across
//! cells, tools are async functions, no type checking) the way plak's
//! Monty-based Python worker (`plakous-py-host`) does: the session
//! bootstrap's private functions are host functions answered as futures,
//! prints stream as `Output`, nested results resume the cell in batches
//! with proved `Delivery` verdicts. On top of that, `StartCell { window }`
//! parks the engine for engine-parked yields and `Cancel` ends a cell
//! without losing the worker.
//!
//! The library holds the protocol loop, the value codec and the session
//! driving. It has no seccomp filter and no FFI (`unsafe` is forbidden
//! here); the binary adds those (docs/spec.md §5, §10.2). Policy values
//! live in [`Config`], whose defaults are plak's; the library adds no
//! policy of its own (no queueing of nested calls past the outstanding
//! limit).
//!
//! It uses std I/O and threads, not an async runtime: [`serve`] blocks the
//! calling thread, and starts one reader thread (and, while a `Cancel` is
//! pending, one park-repeating thread). An async host bridges its pipe to
//! blocking `Read`/`Write` (for tokio, `tokio_util::io::SyncIoBridge`).

#![forbid(unsafe_code)]

use std::{
    io::{Read, Write},
    sync::{
        Arc, Mutex, MutexGuard, PoisonError,
        atomic::{AtomicBool, Ordering},
    },
};

mod codec;
mod engine;
mod worker;

pub use engine::{CellEngine, CellSession, NativeEngine, Park, SessionSettings};
/// The protocol crate this library speaks, for [`Hooks::usage`]'s type.
pub use plakous_cell;
use plakous_cell::ProcessResourceUsage;

/// The adapter revision the worker reports in `Hello`.
pub const ADAPTER: &str = "micro-monty-cell-adapter.v1";

/// The worker's policy values. The defaults are those of plak's Monty
/// worker; a host that changes one changes what its cells see.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    /// The file name tracebacks show (`plak-py-ptc.py`).
    pub script_name: String,
    /// Nested calls a cell may have outstanding (16); the next one raises
    /// `RuntimeError` with [`Config::outstanding_nested_error`]. There is
    /// no queue.
    pub max_outstanding_nested: usize,
    /// That `RuntimeError`'s text. Change it with the limit.
    pub outstanding_nested_error: String,
    /// A cell's cumulative print budget in bytes, its notice included
    /// (1 MiB less 64 bytes of room for plak's status header).
    pub max_print_bytes: u64,
    /// Appended where prints are cut.
    pub print_notice: String,
    /// The bound on a cell's rendered final value, its notice included
    /// (as the print budget).
    pub max_result_bytes: u64,
    /// Appended where a final value is cut.
    pub result_notice: String,
    /// The deepest value nesting either way across the boundary (128).
    pub max_value_depth: usize,
    /// The bound on a nested failure's diagnostic as the cell sees it
    /// (16 KiB).
    pub max_nested_error_bytes: u64,
}

impl Default for Config {
    fn default() -> Self {
        const OUTSTANDING: usize = 16;
        Self {
            script_name: "plak-py-ptc.py".to_owned(),
            max_outstanding_nested: OUTSTANDING,
            outstanding_nested_error: format!(
                "local composition exceeds its profile-bound limit of {OUTSTANDING} outstanding nested calls"
            ),
            max_print_bytes: 1_048_576 - 64,
            print_notice: "\n[Python output truncated at 1 MiB]\n".to_owned(),
            max_result_bytes: 1_048_576 - 64,
            result_notice: "\n[Python result truncated at 1 MiB]".to_owned(),
            max_value_depth: 128,
            max_nested_error_bytes: 16 * 1024,
        }
    }
}

/// What the embedding adds to the loop. Every method has a default.
pub trait Hooks: Send + Sync {
    /// Called once the session and every worker thread exist, before the
    /// first frame after `Configure`. The binary installs seccomp here.
    ///
    /// # Errors
    /// The worker is lost with this message.
    fn confine(&self) -> Result<(), String> {
        Ok(())
    }

    /// The `ResourceReport` sent at `Shutdown`. The default reports
    /// nothing; a process reports its own usage, a thread its CPU time.
    fn usage(&self) -> ProcessResourceUsage {
        ProcessResourceUsage::default()
    }

    /// A named point in delivery (`before_delivery`, `after_accepted`),
    /// where plak's crash-cut tests stop a worker.
    fn boundary(&self, _name: &str) {}
}

/// No hooks.
pub struct NoHooks;

impl Hooks for NoHooks {}

/// Ends a worker that runs without a process of its own, as killing its
/// process would: a computing cell parks at its next safepoint and the
/// worker is lost (`cell worker killed`). A worker that waits for frames
/// ends when its input closes, so a host closes the pipe too.
#[derive(Clone, Default)]
pub struct Kill(Arc<KillState>);

#[derive(Default)]
struct KillState {
    killed: AtomicBool,
    park: Mutex<Option<Park>>,
}

impl Kill {
    /// Kills the worker this handle was given to: one park request, which
    /// stays until a cell parks for it, the running cell or the next one.
    pub fn kill(&self) {
        if self.0.killed.swap(true, Ordering::SeqCst) {
            return;
        }
        if let Some(park) = lock(&self.0.park).as_ref() {
            park();
        }
    }

    /// Whether [`Kill::kill`] was called.
    #[must_use]
    pub fn is_killed(&self) -> bool {
        self.0.killed.load(Ordering::SeqCst)
    }

    fn attach(&self, park: Park) {
        let mut slot = lock(&self.0.park);
        // A kill before `serve` began.
        if self.is_killed() {
            park();
        }
        *slot = Some(park);
    }
}

/// Marks the worker ended, however [`serve`] returns.
struct Ended(Kill);

impl Drop for Ended {
    fn drop(&mut self) {
        *lock(&self.0.0.park) = None;
    }
}

/// How [`serve`] runs.
pub struct Options {
    /// Policy values.
    pub config: Config,
    /// The embedding's hooks.
    pub hooks: Arc<dyn Hooks>,
    /// The kill switch, for a worker without a process of its own.
    pub kill: Kill,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            config: Config::default(),
            hooks: Arc::new(NoHooks),
            kill: Kill::default(),
        }
    }
}

/// How a worker ended other than cleanly.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ServeError {
    /// `Hello` could not be written.
    Hello(String),
    /// The worker was lost; a best-effort `HostLost` frame was written.
    Lost(String),
}

impl std::fmt::Display for ServeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Hello(error) => write!(f, "failed to write hello: {error}"),
            Self::Lost(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for ServeError {}

/// Runs one worker: writes `Hello`, checks `Configure`, then serves cells
/// on one session from `engine` until the input ends, `Shutdown`, a loss or
/// a kill. Blocks the calling thread. Every cell is compiled on it, so
/// give it at least 1 MiB of stack in release builds, 4 MiB in debug
/// (docs/user-guide.md, "A bounded Rust stack"); Rust's 2 MiB thread default is
/// enough in release. The binary reserves 16 MiB as headroom.
///
/// # Errors
/// [`ServeError`]: the worker did not end cleanly.
pub fn serve(
    reader: Box<dyn Read + Send>,
    writer: Box<dyn Write + Send>,
    engine: &dyn CellEngine,
    options: Options,
) -> Result<(), ServeError> {
    let _ended = Ended(options.kill.clone());
    worker::serve(reader, writer, engine, options)
}

/// Locks a mutex; a panic while it was held cannot leave its data
/// half-written in a way that matters, so poisoning is ignored.
pub(crate) fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}
