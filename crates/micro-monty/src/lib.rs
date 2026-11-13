#![doc = include_str!("../user-guide.md")]
#![allow(clippy::doc_markdown, reason = "the docs name CPython in prose")]

use core::fmt::{self, Write as _};
use mm_engine::{Exit, FutureStatus, Vm, VmConfig};
use std::sync::{Arc, atomic::AtomicBool, atomic::AtomicU64, atomic::Ordering};
use std::time::Instant;

mod driver;
mod host_defs;
mod image;
mod render;
pub use driver::{ASYNC_SLICE_FUEL, AsyncHostHandler, BoxFuture, Call, HostHandler, Step};
pub use host_defs::{
    DefineError, HostClass, HostException, HostFunctionRef, HostModule, HostParam, HostSignature,
    ParamKind,
};
pub use image::{SNAPSHOT_FORMAT, image_build_id};
pub use render::{TextCut, head_tail};

pub use mm_engine::{
    CallStatus, Entropy, HintGroup, HostArgs, HostError, HostErrorKind, HostValue, MAX_HOST_DEPTH,
    Stream, Unconverted, WallClock, ZoneSource,
};

/// One host call of a cell, as [`Session::call_log`] reports it.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct HostCallRecord {
    /// The call id ([`Progress::HostCall`]'s `call`).
    pub call: u64,
    /// The function's id.
    pub function: u32,
    /// The call name.
    pub name: String,
    /// What became of it.
    pub status: CallStatus,
}

impl HostCallRecord {
    /// A record, for hosts that decode one (a wasm host's call log).
    #[must_use]
    pub fn new(call: u64, function: u32, name: String, status: CallStatus) -> Self {
        HostCallRecord {
            call,
            function,
            name,
            status,
        }
    }
}

/// The engine name that workers report in their protocol handshake.
pub const ENGINE: &str = "micro-monty";

/// This build's version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Largest accepted source, in bytes
/// ([spec](https://github.com/N0xMare/micro-monty/blob/main/docs/spec.md) §11).
pub const MAX_SOURCE_BYTES: u64 = 256 * 1024;

/// The default [`Session::set_text_limit`]: 1 MiB, or the memory limit if
/// that is less. Code-mode hosts cap a cell's result at about that, and a
/// model cannot use more.
const DEFAULT_TEXT_LIMIT: usize = 1 << 20;

/// The least a [`TextCut::HeadTail`] completion `repr` or `str` of a
/// container renders before it stops (eight times the text limit when that
/// is more); a longer text keeps its beginning, or its shape from the host
/// value.
const HEAD_TAIL_WALK: usize = 1 << 20;

/// Resource limits of a session ([Limits](crate#limits)). Start from
/// [`Limits::default`] (or the `const` [`Limits::new`]) and set what
/// differs with the `with_*` methods or the fields; later releases may add
/// limits, with defaults (`#[non_exhaustive]`).
///
/// ```
/// # use micro_monty::Limits;
/// let limits = Limits::default().with_memory(64 << 20).with_fuel(Some(1_000_000));
/// assert_eq!(limits.recursion_depth, Limits::default().recursion_depth);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Limits {
    /// Arena, retained code/source/caches, instance shapes, execution stacks/frames
    /// and scheduler tables/queues, in bytes; exhaustion raises `MemoryError`.
    /// Not an RSS bound: [`Engine::compile`], snapshot buffers, the
    /// Engine's module cache and allocator peaks are outside it (the crate
    /// docs' Embedding guarantees bound the Rust heap). Snapshot output has a separate
    /// capacity limit, defaulting to this value (see [`Session::snapshot`]).
    pub memory: u64,
    /// Python frame depth; exceeding it raises `RecursionError`.
    pub recursion_depth: u32,
    /// Fuel per cell: the units each cell may charge (one per call or
    /// backward jump, plus proportional charges by long natives) before it
    /// parks with [`ParkReason::Fuel`]. [`Session::start`] gives each cell
    /// this budget unless the host set one with [`Session::set_fuel`]
    /// since the last cell started; a session-wide cap is
    /// [`Session::set_fuel_total`]. `None` is unlimited.
    pub fuel: Option<u64>,
}

impl Limits {
    /// The defaults: 256 MiB, depth 1000, unlimited fuel (the same as
    /// [`Limits::default`], for `const` contexts).
    #[must_use]
    pub const fn new() -> Limits {
        Limits {
            memory: 256 << 20,
            recursion_depth: 1000,
            fuel: None,
        }
    }

    /// These limits with [`Limits::memory`] set to `bytes`.
    #[must_use]
    pub const fn with_memory(mut self, bytes: u64) -> Limits {
        self.memory = bytes;
        self
    }

    /// These limits with [`Limits::recursion_depth`] set to `depth`.
    #[must_use]
    pub const fn with_recursion_depth(mut self, depth: u32) -> Limits {
        self.recursion_depth = depth;
        self
    }

    /// These limits with [`Limits::fuel`] set to `fuel`.
    #[must_use]
    pub const fn with_fuel(mut self, fuel: Option<u64>) -> Limits {
        self.fuel = fuel;
        self
    }
}

impl Default for Limits {
    /// 256 MiB, depth 1000, unlimited fuel ([`Limits::new`]).
    fn default() -> Self {
        Limits::new()
    }
}

/// Engine settings. Start from [`EngineConfig::default`] (or the `const`
/// [`EngineConfig::new`]) and set what differs with the `with_*` methods;
/// later releases may add settings, with defaults (`#[non_exhaustive]`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct EngineConfig {
    /// Limits applied to every session of this engine.
    pub limits: Limits,
}

impl EngineConfig {
    /// The defaults (the same as [`EngineConfig::default`], for `const`
    /// contexts).
    #[must_use]
    pub const fn new() -> EngineConfig {
        EngineConfig {
            limits: Limits::new(),
        }
    }

    /// This config with [`EngineConfig::limits`] set to `limits`.
    #[must_use]
    pub const fn with_limits(mut self, limits: Limits) -> EngineConfig {
        self.limits = limits;
        self
    }
}

/// Compiles source and creates sessions. Clones share one module cache
/// (see [`Engine::with_module_cache`]).
#[derive(Clone, Debug)]
pub struct Engine {
    config: EngineConfig,
    modules: Option<Arc<ModuleCache>>,
    /// Its sessions' snapshots store their host-module code
    /// ([`Engine::with_host_code_in_snapshots`]).
    host_code_in_snapshots: bool,
}

/// The default [`Engine::with_module_cache`] limit: room for every bundled
/// module, decoded (about 2.9 MB on x86-64).
pub const DEFAULT_MODULE_CACHE: u64 = 8 << 20;

/// Bundled modules decoded once per [`Engine`] and shared by its sessions,
/// with the prelude's chunks unpacked once, up to a byte limit; past it,
/// sessions decode their own as without one.
struct ModuleCache {
    slots: Vec<std::sync::OnceLock<Arc<mm_engine::DecodedModule>>>,
    chunks: Vec<std::sync::OnceLock<Arc<mm_engine::PreludeChunk>>>,
    /// Host-module code compiled once ([`Session::define`]), by key.
    host: std::sync::Mutex<Vec<Arc<mm_engine::HostProgram>>>,
    /// Bytes still free under the limit.
    free: AtomicU64,
}

impl ModuleCache {
    /// Puts `v` of `n` bytes in `slot` if it is empty and fits.
    fn offer<V>(&self, slot: Option<&std::sync::OnceLock<Arc<V>>>, v: &Arc<V>, n: u64) {
        let Some(slot) = slot else {
            return;
        };
        if slot.get().is_none()
            && self
                .free
                .try_update(Ordering::Relaxed, Ordering::Relaxed, |free| {
                    free.checked_sub(n)
                })
                .is_ok()
            && slot.set(v.clone()).is_err()
        {
            // Another session cached it first.
            self.free.fetch_add(n, Ordering::Relaxed);
        }
    }
}

/// Reads the host's bundle for a `host-bundle` build: fills `out` from
/// `offset` of region `region` (0: the prelude's packed chunks, 1: the
/// bundled modules' packed code) and says whether it could. The mm-wasm
/// guest sets it to its `mm_host.bundle_read` import.
#[cfg(feature = "host-bundle")]
pub type BundleReader = fn(region: u32, offset: u32, out: &mut [u8]) -> bool;

#[cfg(feature = "host-bundle")]
static BUNDLE_READER: std::sync::OnceLock<BundleReader> = std::sync::OnceLock::new();

/// Sets the process's [`BundleReader`] once; later calls are ignored.
/// Without one, a `host-bundle` build raises `SystemError` on the first
/// prelude lookup or bundled import.
#[cfg(feature = "host-bundle")]
pub fn set_bundle_reader(f: BundleReader) {
    let _ = BUNDLE_READER.set(f);
}

impl mm_engine::ModuleCache for ModuleCache {
    #[cfg(feature = "host-bundle")]
    fn bundle(&self, region: u32, offset: u32, out: &mut [u8]) -> bool {
        BUNDLE_READER.get().is_some_and(|f| f(region, offset, out))
    }

    fn get(&self, i: usize) -> Option<Arc<mm_engine::DecodedModule>> {
        self.slots.get(i)?.get().cloned()
    }

    fn offer(&self, i: usize, module: &Arc<mm_engine::DecodedModule>) {
        ModuleCache::offer(self, self.slots.get(i), module, module.bytes());
    }

    fn prelude_chunk(&self, c: usize) -> Option<Arc<mm_engine::PreludeChunk>> {
        self.chunks.get(c)?.get().cloned()
    }

    fn offer_prelude_chunk(&self, c: usize, chunk: &Arc<mm_engine::PreludeChunk>) {
        ModuleCache::offer(self, self.chunks.get(c), chunk, chunk.bytes());
    }

    fn host_program(&self, key: &[u8; 16]) -> Option<Arc<mm_engine::HostProgram>> {
        let host = self
            .host
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        host.iter().find(|p| p.key == *key).cloned()
    }

    fn offer_host_program(&self, program: &Arc<mm_engine::HostProgram>) {
        let mut host = self
            .host
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !host.iter().any(|p| p.key == program.key)
            && self
                .free
                .try_update(Ordering::Relaxed, Ordering::Relaxed, |free| {
                    free.checked_sub(program.bytes())
                })
                .is_ok()
        {
            host.push(Arc::clone(program));
        }
    }
}

impl std::fmt::Debug for ModuleCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ModuleCache").finish_non_exhaustive()
    }
}

/// Compiled source: immutable, reusable across sessions.
#[derive(Clone, Debug)]
pub struct Code {
    program: mm_compile::bytecode::Program,
    filename: String,
    source: String,
}

impl Code {
    /// The file name tracebacks show.
    #[must_use]
    pub fn filename(&self) -> &str {
        &self.filename
    }

    /// Compile-time warnings, rendered as CPython prints them to stderr
    /// (`file:line: SyntaxWarning: message` and the stripped source line).
    #[must_use]
    pub fn warnings(&self) -> Vec<String> {
        self.program
            .warnings
            .iter()
            .map(|(pos, msg)| {
                let line = mm_compile::source_lines(&self.source)
                    .nth((pos.line as usize).saturating_sub(1))
                    .unwrap_or("")
                    .trim();
                format!(
                    "{}:{}: SyntaxWarning: {msg}\n  {line}\n",
                    self.filename, pos.line
                )
            })
            .collect()
    }
}

/// Source that does not compile: a `SyntaxError` (including "micro-monty
/// does not support X") or a `RecursionError` for nesting too deep.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct CompileError {
    /// The Python exception type name.
    pub type_name: String,
    /// The message.
    pub message: String,
    /// 1-based line (0 when unknown).
    pub line: u32,
    /// 0-based character column.
    pub column: u32,
    /// The report in CPython's format (`File "...", line N`, the source line,
    /// a caret, then `Type: message`).
    pub rendered: String,
}

impl CompileError {
    /// A compile error, for hosts that decode one (a wasm host).
    #[must_use]
    pub fn new(
        type_name: String,
        message: String,
        line: u32,
        column: u32,
        rendered: String,
    ) -> Self {
        CompileError {
            type_name,
            message,
            line,
            column,
            rendered,
        }
    }
}

impl fmt::Display for CompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.rendered)
    }
}

impl std::error::Error for CompileError {}

/// An uncaught Python exception.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct PyException {
    /// `type(exc).__name__`.
    pub type_name: String,
    /// `str(exc)`.
    pub message: String,
    /// The full report in CPython 3.14's format, chained exceptions included,
    /// ending with a newline.
    pub traceback: String,
    /// Whether `message` or `traceback` was cut. Rendering an uncaught
    /// exception stops once its text passes the session's text limit
    /// ([`Session::set_text_limit`]) or, during long work, once the
    /// deadline has passed ([`Session::set_deadline`]); the text is then
    /// cut as [`Session::set_text_cut`] says. A cut traceback may lack its
    /// final newline.
    pub truncated: bool,
}

impl PyException {
    /// An exception report, for hosts that decode one (a wasm host).
    #[must_use]
    pub fn new(type_name: String, message: String, traceback: String, truncated: bool) -> Self {
        PyException {
            type_name,
            message,
            traceback,
            truncated,
        }
    }
}

impl fmt::Display for PyException {
    /// The traceback, as CPython prints it.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.traceback)
    }
}

impl std::error::Error for PyException {}

/// The value of a final expression statement.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Completion {
    /// `repr(value)`.
    pub repr: String,
    /// `str(value)`.
    pub str: String,
    /// `type(value).__name__`.
    pub type_name: String,
    /// The value as plain data, if it converts within the session's
    /// [`HostArgs`] (else use `repr`).
    pub value: Option<HostValue>,
    /// Whether `repr` was cut at the session's text limit
    /// ([`Session::set_text_limit`]).
    pub repr_truncated: bool,
    /// Whether `str` was cut at the session's text limit.
    pub str_truncated: bool,
}

impl Completion {
    /// A completion, for hosts that decode one (a wasm host).
    #[must_use]
    pub fn new(
        repr: String,
        str: String,
        type_name: String,
        value: Option<HostValue>,
        repr_truncated: bool,
        str_truncated: bool,
    ) -> Self {
        Completion {
            repr,
            str,
            type_name,
            value,
            repr_truncated,
            str_truncated,
        }
    }
}

/// Why a session stopped before completing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ParkReason {
    /// The fuel budget ran out; add fuel ([`Session::set_fuel`]) and resume.
    Fuel,
    /// A [`ParkHandle`] (or [`Session::request_park`]) asked.
    Requested,
    /// The deadline ([`Session::set_deadline`]) passed.
    Deadline,
}

/// What [`Session::start`] or [`Session::resume`] ran to.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Progress {
    /// The code finished; the value of a final expression statement, if any.
    Complete(Option<Completion>),
    /// A host function was called. Answer with [`Resume::Value`],
    /// [`Resume::Error`] or [`Resume::Future`].
    HostCall {
        /// The call id; increases monotonically per session.
        call: u64,
        /// The function's id, as [`Session::define`] returned it.
        function: u32,
        /// The function's name (`module.name` for a host module's,
        /// `module.Class.method` for a method).
        name: String,
        /// Positional arguments.
        args: Vec<HostValue>,
        /// Keyword arguments, in call order.
        kwargs: Vec<(String, HostValue)>,
    },
    /// Every task waits on these host futures (ascending ids); deliver
    /// results with [`Resume::Batch`].
    Waiting {
        /// The pending call ids.
        calls: Vec<u64>,
    },
    /// Stopped at a safepoint; [`Resume::Continue`] continues.
    Parked {
        /// Why.
        reason: ParkReason,
    },
    /// An uncaught exception.
    Error(PyException),
}

/// How to continue a stopped cell.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Resume {
    /// Continue where execution parked.
    Continue,
    /// The host call returns this value.
    Value(HostValue),
    /// The host call raises this error.
    Error(HostError),
    /// The host call returns an awaitable; its result comes in a later
    /// [`Resume::Batch`].
    Future,
    /// Results of pending host futures, in any order; a partial batch is
    /// fine (the cell waits again for the rest).
    Batch(Vec<(u64, Result<HostValue, HostError>)>),
}

/// What a session is stopped at ([`Session::phase`]), and so which
/// [`Resume`] it takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Phase {
    /// No cell is stopped: between cells. It takes no [`Resume`].
    Idle,
    /// At a host call ([`Progress::HostCall`]): [`Resume::Value`],
    /// [`Resume::Error`] or [`Resume::Future`].
    HostCall,
    /// Every task waits on host futures ([`Progress::Waiting`]):
    /// [`Resume::Batch`].
    Waiting,
    /// Parked ([`Progress::Parked`]): [`Resume::Continue`].
    Parked,
}

impl Phase {
    /// The answers the phase takes, for messages.
    #[must_use]
    pub fn expects(self) -> &'static str {
        match self {
            Phase::Idle => "no answer: no cell is stopped",
            Phase::HostCall => "Resume::Value, Resume::Error or Resume::Future",
            Phase::Waiting => "Resume::Batch",
            Phase::Parked => "Resume::Continue",
        }
    }
}

/// A [`Resume`] the session refused; the cell is unchanged.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ResumeError {
    /// The answer does not fit what the cell stopped for.
    Unexpected {
        /// What the session is stopped at ([`Phase::expects`] names the
        /// answers it takes).
        phase: Phase,
    },
    /// The batch names a call that is not a host future of this cell.
    UnknownCall {
        /// The call id.
        call: u64,
    },
    /// The batch names a future that was already resolved (or names it
    /// twice).
    AlreadySettled {
        /// The call id.
        call: u64,
    },
}

impl fmt::Display for ResumeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ResumeError::Unexpected { phase } => {
                write!(f, "the session expects {}", phase.expects())
            }
            ResumeError::UnknownCall { call } => {
                write!(f, "call {call} is not a host future of the running cell")
            }
            ResumeError::AlreadySettled { call } => {
                write!(f, "host future {call} is already resolved")
            }
        }
    }
}

impl std::error::Error for ResumeError {}

/// Why a snapshot could not be taken or restored.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum SnapshotError {
    /// A build with another build id or format version wrote the image:
    /// restore it on the build its id names ([Upgrading: drain old
    /// builds](crate#upgrading-drain-old-builds)).
    Incompatible {
        /// The image's snapshot format version.
        format: u16,
        /// The image's build id (compare [`build_id`]).
        build_id: [u8; 32],
    },
    /// Not a snapshot, truncated, failed its checksum, or inconsistent;
    /// names the part that is.
    Corrupt {
        /// The part.
        what: String,
    },
    /// Restored state exceeds the memory limit, or snapshot output exceeds
    /// its capacity limit or cannot be allocated.
    TooLarge,
    /// The session is inside an operation and cannot be saved (never the
    /// case between calls of [`Session::start`] and [`Session::resume`]),
    /// or [`SessionTemplate::capture`] found a stopped cell.
    Busy,
    /// An engine panic poisoned the session (see the crate docs); it
    /// cannot be saved.
    Poisoned,
    /// The engine panicked while restoring (an engine bug, with
    /// `panic = "unwind"`; see the crate docs). The image itself may be
    /// sound: keep it.
    Internal {
        /// The panic's message.
        message: String,
    },
}

/// The first eight bytes of a build id in hex, for messages.
fn short_id(id: &[u8; 32]) -> String {
    id[..8].iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

impl fmt::Display for SnapshotError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SnapshotError::Incompatible {
                format,
                build_id: id,
            } => write!(
                f,
                "the snapshot was written by another micro-monty build (format {format}, \
                 build {}...; this is format {SNAPSHOT_FORMAT}, build {}...)",
                short_id(id),
                short_id(build_id())
            ),
            SnapshotError::Corrupt { what } => write!(f, "the snapshot is corrupt ({what})"),
            SnapshotError::TooLarge => f.write_str("the snapshot needs more memory than allowed"),
            SnapshotError::Busy => f.write_str("the session cannot be saved inside an operation"),
            SnapshotError::Poisoned => {
                f.write_str("the session is poisoned by an internal engine error")
            }
            SnapshotError::Internal { message } => f.write_str(message),
        }
    }
}

impl std::error::Error for SnapshotError {}

/// A session saved after its setup, to start many sessions from
/// ([Session templates](crate#session-templates)): define host functions, modules and
/// classes, run a bootstrap cell, then [`SessionTemplate::capture`].
/// [`SessionTemplate::instantiate`] restores the image, so each new
/// session starts with the setup done instead of re-running it. Like any
/// restore, host-side settings (output callback, deadline, clock and
/// entropy callbacks) are not part of it: give them to
/// [`SessionTemplate::instantiate_with`] ([`SessionSettings`]). `Send +
/// Sync`; clone it or share it behind an `Arc`.
#[derive(Clone, Debug)]
pub struct SessionTemplate {
    engine: Engine,
    image: Vec<u8>,
}

impl SessionTemplate {
    /// Saves `session` (between cells) as a template for `engine`, the
    /// engine it was created with.
    ///
    /// # Errors
    ///
    /// [`SnapshotError::Busy`] while a cell is stopped, and the errors of
    /// [`Session::snapshot`].
    pub fn capture(
        engine: &Engine,
        session: &mut Session,
    ) -> Result<SessionTemplate, SnapshotError> {
        if session.phase() != Phase::Idle {
            return Err(SnapshotError::Busy);
        }
        Ok(SessionTemplate {
            engine: engine.clone(),
            image: session.snapshot()?,
        })
    }

    /// A template from an image [`SessionTemplate::image`] returned (by
    /// this build), checked by restoring it once.
    ///
    /// # Errors
    ///
    /// The errors of [`Engine::restore`].
    pub fn from_image(engine: &Engine, image: Vec<u8>) -> Result<SessionTemplate, SnapshotError> {
        engine.restore(&image)?;
        Ok(SessionTemplate {
            engine: engine.clone(),
            image,
        })
    }

    /// A new session in the saved state.
    ///
    /// # Errors
    ///
    /// The errors of [`Engine::restore`]: [`SnapshotError::TooLarge`] if
    /// the engine's memory limit is now too small.
    pub fn instantiate(&self) -> Result<Session, SnapshotError> {
        self.engine.restore(&self.image)
    }

    /// [`SessionTemplate::instantiate`], with `settings` instead of the
    /// defaults ([`SessionSettings`]).
    ///
    /// # Errors
    ///
    /// The errors of [`Engine::restore`].
    pub fn instantiate_with(&self, settings: SessionSettings) -> Result<Session, SnapshotError> {
        self.engine.restore_with(&self.image, settings)
    }

    /// The saved image (a snapshot), e.g. to store it.
    #[must_use]
    pub fn image(&self) -> &[u8] {
        &self.image
    }
}

/// A hash of the header codec's code (`src/image.rs`, comments and blank
/// space aside; the build script), part of the [`build_id`].
const LAYOUT_HASH: u64 = include!(concat!(env!("OUT_DIR"), "/layout_hash.rs"));

/// This build's snapshot build id: what an image depends on to be read,
/// and nothing else. Two builds with the same id restore each other's
/// images, whatever else differs between them (the version, the code of
/// functions outside the codecs); [`Engine::restore`] refuses an image
/// written under another id with [`SnapshotError::Incompatible`], and
/// [`image_build_id`] reads an image's. It hashes:
///
/// - [`SNAPSHOT_FORMAT`] and the engine's layout epoch (bumped by hand
///   when a function body changes what an image means, docs/spec.md
///   §9.3);
/// - the code of the snapshot codecs (this crate's header, the engine's
///   sections, the heap and compiled code) and of every `const` and
///   `#[repr]` item of the engine crates, which declare the object,
///   frame, continuation and native-step layouts (comments and blank
///   space aside);
/// - the opcode table, the native table, the type table and the
///   native-step table, in order;
/// - the prelude's and every enabled bundled module's bytecode (hashed at
///   build time), and the enabled `mod-*` features;
/// - the encodings of values, object headers and string, int and float
///   hashes;
/// - a recorded hash of the code [`Session::define`] generates and
///   compiles for a definition that uses every kind of parameter, a class
///   and an exception (a unit test checks it).
///
/// Nothing in it depends on the target or on how the crates were fetched:
/// native and wasm builds of the same source and features share it, and a
/// build from the published packages has the workspace build's id.
#[must_use]
pub fn build_id() -> &'static [u8; 32] {
    static ID: std::sync::OnceLock<[u8; 32]> = std::sync::OnceLock::new();
    ID.get_or_init(|| {
        let mut ingredients = Vec::new();
        let mut out = |b: &[u8]| ingredients.extend_from_slice(b);
        mm_core::put_u32(&mut out, u32::from(SNAPSHOT_FORMAT));
        mm_core::put_u64(&mut out, LAYOUT_HASH);
        mm_core::put_u64(&mut out, mm_stdlib::FEATURES);
        // Restored sessions find bundled modules' code by index: their
        // code and the packing it unpacks with, hashed at build time, as
        // the prelude is, so a build that links no stdlib bytes has the
        // same id.
        mm_core::put_u64(&mut out, mm_stdlib::CODE_HASH);
        // A restored host module's code is generated and compiled again
        // (unless the image holds it), and its frames resume in it.
        mm_core::put_u64(&mut out, host_defs::CODEGEN_PROBE);
        ingredients.extend_from_slice(&mm_engine::build_fingerprint());
        let mut id = [0u8; 32];
        for (seed, part) in (0u64..).zip(id.as_chunks_mut::<8>().0) {
            *part = mm_core::xxh64(&ingredients, seed).to_le_bytes();
        }
        id
    })
}

/// [`Engine::environment`]'s text for `limits`, and a session's fuel total.
fn environment(limits: Limits, fuel_total: Option<u64>) -> String {
    let public = |n: &&str| !n.split('.').any(|p| p.starts_with('_'));
    // Both module lists are generated sorted (mm-stdlib's build script),
    // so the engine's few are merged in instead of sorting: a slice sort
    // costs about 10 KB in the guest [M].
    let mut modules: Vec<&str> = mm_stdlib::MODULES
        .iter()
        .map(|m| m.name)
        .filter(public)
        .collect();
    for name in mm_engine::BUNDLED.iter().copied().filter(public) {
        if let Err(at) = modules.binary_search(&name) {
            modules.insert(at, name);
        }
    }
    let mut missing: Vec<&str> = mm_stdlib::ALL_MODULES
        .iter()
        .copied()
        .filter(|n| !modules.contains(n) && public(n))
        .collect();
    // The engine's own modules a build can leave out.
    for name in ["asyncio", "json"] {
        if !modules.contains(&name)
            && let Err(at) = missing.binary_search(&name)
        {
            missing.insert(at, name);
        }
    }
    let mut out = format!(
        "Python {} subset (micro-monty {VERSION}), in a sandbox: no filesystem, network, \
         processes or threads except through the functions provided.\n\
         Modules: {}.\n",
        "3.14",
        modules.join(", ")
    );
    if !missing.is_empty() {
        let _ = writeln!(out, "Not enabled in this build: {}.", missing.join(", "));
    }
    // A build without `asyncio` leaves out its line and `gather`.
    let asyncio = mm_engine::BUNDLED.contains(&"asyncio");
    let _ = write!(
        out,
        "No third-party packages, no exec/eval/compile, no threading, subprocess or socket.\n\
         {}\
         Limits: memory {} MiB, recursion depth {}, {}.\n\
         Conventions: call the provided functions like ordinary functions; when one returns \
         an awaitable, await it{}. A failed call \
         raises an exception (TimeoutError when it timed out). print() output is returned; \
         the value of a final expression is shown.\n",
        if asyncio {
            format!("asyncio: {}.\n", mm_engine::asyncio_names())
        } else {
            String::new()
        },
        limits.memory >> 20,
        limits.recursion_depth,
        match (limits.fuel, fuel_total) {
            (Some(n), None) => format!("{n} fuel units per cell"),
            (None, None) => "no instruction budget".to_owned(),
            (Some(n), Some(t)) => {
                format!("{n} fuel units per cell and {t} left for the session")
            }
            (None, Some(t)) => format!("{t} fuel units left for the session"),
        },
        if asyncio {
            ", and run several at once with asyncio.gather"
        } else {
            ""
        }
    );
    out
}

/// Whether std reaches a system clock and OS randomness here. Not on
/// `wasm32-unknown-unknown`, where `SystemTime::now` and `Instant::now`
/// panic: there sessions start without a wall clock or entropy, and the
/// host provides them ([`Session::set_wall_clock`],
/// [`Session::set_entropy`], [`Session::set_deadline_with`]).
const HAS_OS: bool = !cfg!(all(target_family = "wasm", target_os = "unknown"));

/// The system clock as a [`WallClock`].
fn system_clock() -> WallClock {
    Box::new(|| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0.0, |d| d.as_secs_f64())
    })
}

/// Operating-system randomness as an [`Entropy`] source: SipHash with keys
/// std draws from the OS once per thread, over a counter.
fn os_entropy() -> Entropy {
    use std::hash::{BuildHasher, Hasher};
    let state = std::collections::hash_map::RandomState::new();
    let mut counter = 0u64;
    Box::new(move |buf: &mut [u8]| {
        for chunk in buf.chunks_mut(8) {
            counter = counter.wrapping_add(1);
            let mut h = state.build_hasher();
            h.write_u64(counter);
            let word = h.finish().to_le_bytes();
            chunk.copy_from_slice(&word[..chunk.len()]);
        }
    })
}

/// Test-only panic injection (feature `test-hooks`): proves that the facade
/// contains engine panics (crate docs).
#[cfg(feature = "test-hooks")]
#[doc(hidden)]
pub mod test_hooks {
    use std::cell::Cell;

    thread_local! {
        static ARMED: Cell<Option<&'static str>> = const { Cell::new(None) };
    }

    /// Makes the next facade operation `op` on this thread (`start`,
    /// `resume`, `snapshot`, `restore`, `compile`, `reset`, `cancel`) panic
    /// inside the engine part of its work, once.
    pub fn panic_next(op: &'static str) {
        ARMED.set(Some(op));
    }

    #[allow(clippy::panic, reason = "the point of the hook")]
    pub(crate) fn point(op: &'static str) {
        if ARMED.get() == Some(op) {
            ARMED.set(None);
            panic!("test hook: injected panic in {op}");
        }
    }
}

#[cfg(not(feature = "test-hooks"))]
mod test_hooks {
    #[inline(always)]
    pub(crate) fn point(_op: &'static str) {}
}

/// Asks a running session to park at its next safepoint, from any thread.
/// A request stays until the park it causes takes it, across
/// [`Session::start`] and [`Session::cancel`],
/// so a host's "stop this session" cannot be lost to a cell that was
/// starting or ending.
#[derive(Clone, Debug)]
pub struct ParkHandle(Arc<AtomicBool>);

impl ParkHandle {
    /// Requests the park ([`ParkReason::Requested`]).
    pub fn request(&self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

/// An output callback that also receives each piece's stream.
pub type StreamOutput = Box<dyn FnMut(Stream, &str) + Send>;

/// What a session takes from its host and snapshots do not carry: where
/// its output goes, the clock and randomness it reads, where zone data
/// comes from, how completion text is cut, and GC stress. A restore starts
/// from the settings it is given, not the saved session's, so give the
/// same settings to [`Session::new_with`], [`Engine::restore_with`] and
/// [`SessionTemplate::instantiate_with`] (build them in one function), and
/// a restored session behaves like the one that was saved. The setters
/// ([`Session::set_output_streams`] and the others named below) change
/// them afterwards.
///
/// Start from [`SessionSettings::default`] and set fields; later releases
/// may add settings, with defaults (`#[non_exhaustive]`).
///
/// ```
/// # use micro_monty::*;
/// fn settings() -> SessionSettings {
///     let mut s = SessionSettings::default();
///     s.wall_clock = Some(Box::new(|| 1_700_000_000.0));
///     s.text_cut = TextCut::HeadTail;
///     s
/// }
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let engine = Engine::new(EngineConfig::default());
/// let mut session = Session::new_with(&engine, settings())?;
/// let image = session.snapshot()?;
/// let restored = engine.restore_with(&image, settings())?;
/// # drop(restored);
/// # Ok(()) }
/// ```
#[non_exhaustive]
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent session switches"
)]
pub struct SessionSettings {
    /// Receives `print` and `sys.stdout`/`sys.stderr` output, each piece
    /// with its stream ([`Session::set_output_streams`]). `None`, the
    /// default, drops output.
    pub output: Option<StreamOutput>,
    /// [`Session::set_wall_clock`]. The default reads the system clock
    /// (`None` on `wasm32-unknown-unknown`).
    pub wall_clock: Option<WallClock>,
    /// [`Session::set_entropy`]. The default is keyed from the operating
    /// system's randomness (`None` on `wasm32-unknown-unknown`).
    pub entropy: Option<Entropy>,
    /// [`Session::set_zone_source`]. Default `None`.
    pub zone_source: Option<ZoneSource>,
    /// [`Session::set_text_cut`]. Default [`TextCut::Head`].
    pub text_cut: TextCut,
    /// [`Session::set_int_bits_text`]. Default `false`.
    pub int_bits_text: bool,
    /// [`Session::set_gc_stress`]. Default `false`.
    pub gc_stress: bool,
    /// [`Session::set_gc_inc_stress`] (engine testing). Default `false`.
    #[doc(hidden)]
    pub gc_inc_stress: bool,
    /// [`Session::set_gc_incremental`]'s slice and live-byte threshold
    /// (engine testing), applied after `gc_inc_stress`. Default `None`:
    /// the engine's.
    #[doc(hidden)]
    pub gc_incremental: Option<(u64, u64)>,
    /// Measuring the engine: no collection starts by itself (requested
    /// ones, `gc.collect()`'s and a native's, still run), so a step's time
    /// can be split into its own work and collections. Garbage stays until
    /// the memory limit. Default `false`.
    #[doc(hidden)]
    pub gc_off: bool,
}

impl Default for SessionSettings {
    fn default() -> Self {
        SessionSettings {
            output: None,
            wall_clock: HAS_OS.then(system_clock),
            entropy: HAS_OS.then(os_entropy),
            zone_source: None,
            text_cut: TextCut::Head,
            int_bits_text: false,
            gc_stress: false,
            gc_inc_stress: false,
            gc_incremental: None,
            gc_off: false,
        }
    }
}

impl fmt::Debug for SessionSettings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SessionSettings")
            .field("output", &self.output.is_some())
            .field("wall_clock", &self.wall_clock.is_some())
            .field("entropy", &self.entropy.is_some())
            .field("zone_source", &self.zone_source.is_some())
            .field("text_cut", &self.text_cut)
            .field("int_bits_text", &self.int_bits_text)
            .field("gc_stress", &self.gc_stress)
            .field("gc_inc_stress", &self.gc_inc_stress)
            .field("gc_incremental", &self.gc_incremental)
            .field("gc_off", &self.gc_off)
            .finish_non_exhaustive()
    }
}

impl Engine {
    /// An engine with `config` and a module cache of
    /// [`DEFAULT_MODULE_CACHE`] bytes.
    #[must_use]
    pub fn new(config: EngineConfig) -> Engine {
        Engine::with_module_cache(config, DEFAULT_MODULE_CACHE)
    }

    /// An engine whose sessions share bundled modules decoded once, and
    /// host-module code compiled once ([`Session::define`]), up to
    /// `max_bytes` of code held by the engine (0: no cache; each session
    /// decodes the modules it imports or restores and compiles its host
    /// modules). The cache only
    /// grows, and is outside every session's [`Limits::memory`]; a session
    /// is charged the same with or without it ([Embedding
    /// guarantees](crate#embedding-guarantees)).
    #[must_use]
    pub fn with_module_cache(config: EngineConfig, max_bytes: u64) -> Engine {
        Engine::with_caches(config, max_bytes, mm_engine::PRELUDE_CHUNKS)
    }

    /// [`Engine::new`], but its cache keeps only decoded modules, not the
    /// prelude chunks (asyncio's classes, for one) its sessions unpack.
    /// For an engine with one session for its whole life, such as the
    /// mm-wasm guest's, where the chunks would stay in memory for little
    /// gain. Sessions behave the same. The mm-wasm guest's knob, hidden and
    /// outside the semver guarantees: native engines keep the chunk cache.
    #[doc(hidden)]
    #[must_use]
    pub fn without_prelude_chunk_cache(config: EngineConfig) -> Engine {
        Engine::with_caches(config, DEFAULT_MODULE_CACHE, 0)
    }

    fn with_caches(config: EngineConfig, max_bytes: u64, chunks: usize) -> Engine {
        let modules = (max_bytes > 0).then(|| {
            Arc::new(ModuleCache {
                slots: mm_stdlib::MODULES
                    .iter()
                    .map(|_| std::sync::OnceLock::new())
                    .collect(),
                chunks: (0..chunks).map(|_| std::sync::OnceLock::new()).collect(),
                host: std::sync::Mutex::new(Vec::new()),
                free: max_bytes.into(),
            })
        });
        Engine {
            config,
            modules,
            host_code_in_snapshots: false,
        }
    }

    /// An engine with other limits that shares this one's module cache:
    /// for hosts whose sessions differ in limits (per tenant, per cell
    /// type). Sessions made from either reuse the same decoded modules.
    #[must_use]
    pub fn with_limits(&self, limits: Limits) -> Engine {
        Engine {
            config: EngineConfig { limits },
            modules: self.modules.clone(),
            host_code_in_snapshots: self.host_code_in_snapshots,
        }
    }

    /// An engine that shares this one's module cache and limits, and whose
    /// sessions' snapshots and templates store the code generated for
    /// their typed host functions and classes (`on`), as they store user
    /// code, instead of referring to the Engine's copy by key. For hosts
    /// that restore images on a new Engine every time (one session per
    /// process or wasm instance): restoring a keyed image there compiles
    /// the code again, which costs what [`Session::define`] did, while a
    /// stored copy only decodes ([Host functions and host
    /// modules](crate#host-functions-and-host-modules)). Either kind of
    /// image restores on any Engine of the build.
    #[must_use]
    pub fn with_host_code_in_snapshots(&self, on: bool) -> Engine {
        Engine {
            host_code_in_snapshots: on,
            ..self.clone()
        }
    }

    /// The limits this engine gives its sessions.
    #[must_use]
    pub fn limits(&self) -> Limits {
        self.config.limits
    }

    /// Bytes of decoded modules, unpacked prelude chunks and compiled
    /// host-module code the engine's cache holds now.
    #[must_use]
    pub fn module_cache_bytes(&self) -> u64 {
        self.modules.as_ref().map_or(0, |c| {
            let modules: u64 = c
                .slots
                .iter()
                .filter_map(|s| s.get())
                .map(|m| m.bytes())
                .sum();
            let chunks: u64 = c
                .chunks
                .iter()
                .filter_map(|s| s.get())
                .map(|m| m.bytes())
                .sum();
            let host: u64 = c
                .host
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .iter()
                .map(|p| p.bytes())
                .sum();
            modules + chunks + host
        })
    }

    fn module_cache(&self) -> Option<Arc<dyn mm_engine::ModuleCache>> {
        self.modules
            .clone()
            .map(|c| c as Arc<dyn mm_engine::ModuleCache>)
    }

    /// Continues a session from [`Session::snapshot`]'s bytes, written by
    /// this build in any process. The session keeps the limits, globals,
    /// stopped cell, pending host calls (with their ids) and settings it
    /// was saved with, and continues with the [`Resume`] it was waiting
    /// for. It has no output callback, deadline or park handle yet: set
    /// them as on a new session. The memory limit is the smaller of the
    /// saved and restoring engine's limits, including future allocations.
    /// Everything the session keeps must fit it, each part admitted before
    /// allocation: the arena, retained code and source, frames, scheduler
    /// tables, shapes, host definitions and fixed VM metadata, and with them
    /// the bytecode verification scratch and bundled-module decode
    /// workspace. The pending stop's host values ([`Session::pending`])
    /// are read within a cap of the restoring engine's memory limit, then
    /// charged to the session when they fit; they never fail the restore.
    /// Not charged: `bytes` itself, the Engine's shared module cache and
    /// the allocator's transient peaks (docs/spec.md §11, "Restore
    /// admission").
    ///
    /// # Errors
    ///
    /// [`SnapshotError::Incompatible`] for another build id's snapshot (with
    /// the image's format and build id), [`SnapshotError::Corrupt`] for
    /// anything malformed and [`SnapshotError::TooLarge`] for retained
    /// state over the memory limit. An engine panic while reading (with
    /// unwinding; see the crate docs) is [`SnapshotError::Internal`].
    pub fn restore(&self, bytes: &[u8]) -> Result<Session, SnapshotError> {
        self.restore_with(bytes, SessionSettings::default())
    }

    /// [`Engine::restore`], with `settings` instead of the defaults: give
    /// it the settings the saved session had ([`SessionSettings`]).
    ///
    /// # Errors
    ///
    /// The errors of [`Engine::restore`].
    pub fn restore_with(
        &self,
        bytes: &[u8],
        settings: SessionSettings,
    ) -> Result<Session, SnapshotError> {
        contain(|| self.restore_unguarded(bytes, settings)).unwrap_or_else(|payload| {
            Err(SnapshotError::Internal {
                message: internal_error_message(
                    payload.as_ref(),
                    "the image may be sound: keep it",
                ),
            })
        })
    }

    fn restore_unguarded(
        &self,
        bytes: &[u8],
        settings: SessionSettings,
    ) -> Result<Session, SnapshotError> {
        let image::Image {
            mut vm,
            returns_last_expr,
            want_value,
            stop,
            text_limit,
            fuel_total,
        } = image::read(self, bytes)?;
        vm.set_host_code_in_snapshots(self.host_code_in_snapshots);
        vm.set_known_modules(mm_stdlib::ALL_MODULES);
        test_hooks::point("restore");
        let stop_charge = vm.heap.reserve_scratch(stop.len()).ok();
        let mut session = Session {
            vm,
            returns_last_expr,
            want_value,
            stop,
            stop_charge,
            text_limit,
            text_cut: TextCut::Head,
            int_bits_text: false,
            cell_fuel: self.config.limits.fuel,
            next_fuel: None,
            fuel_total,
            poisoned: None,
        };
        session.apply(settings);
        Ok(session)
    }

    /// A summary of this build for a model's prompt: the bundled modules,
    /// what is left out, the limits of this engine's sessions and the
    /// conventions of the sandbox ([Limits](crate#limits)).
    /// Generated from the build, so it never drifts from what imports;
    /// add [`Session::stubs`] for the host's functions.
    #[must_use]
    pub fn environment(&self) -> String {
        environment(self.config.limits, None)
    }

    /// Compiles a module. `filename` appears in tracebacks.
    ///
    /// # Errors
    ///
    /// A [`CompileError`] for invalid or unsupported syntax, nesting too deep,
    /// or source larger than [`MAX_SOURCE_BYTES`]. An engine panic while
    /// compiling (with unwinding; see the crate docs) is a `SystemError`.
    pub fn compile(&self, source: &str, filename: &str) -> Result<Code, CompileError> {
        Engine::compile_contained(source, filename)
    }

    /// [`Engine::compile`], for [`Session::compile`] too.
    fn compile_contained(source: &str, filename: &str) -> Result<Code, CompileError> {
        contain(|| Engine::compile_unguarded(source, filename)).unwrap_or_else(|payload| {
            let message = internal_error_message(payload.as_ref(), "the source was not compiled");
            Err(CompileError {
                type_name: "SystemError".to_owned(),
                rendered: format!("SystemError: {message}\n"),
                message,
                line: 0,
                column: 0,
            })
        })
    }

    fn compile_unguarded(source: &str, filename: &str) -> Result<Code, CompileError> {
        if u64::try_from(source.len()).unwrap_or(u64::MAX) > MAX_SOURCE_BYTES {
            return Err(compile_error(
                &mm_compile::CompileError::unsupported(
                    "source files larger than 256 KiB",
                    mm_compile::ast::Pos::default(),
                ),
                source,
                filename,
            ));
        }
        Engine::compile_source(source, filename)
    }

    /// [`Engine::compile_unguarded`] without the source size limit, for
    /// code the host generates.
    fn compile_source(source: &str, filename: &str) -> Result<Code, CompileError> {
        let module = mm_parse::parse(source).map_err(|e| compile_error(&e, source, filename))?;
        test_hooks::point("compile");
        let program = mm_compile::compile(
            &module,
            mm_compile::Options {
                last_expr_value: true,
            },
        )
        .map_err(|e| compile_error(&e, source, filename))?;
        Ok(Code {
            program,
            filename: filename.to_owned(),
            source: source.to_owned(),
        })
    }
}

/// Runs `f`, catching a panic when panics unwind. With `panic = "abort"`
/// there is nothing to catch, and the error paths compile away.
#[inline]
#[allow(clippy::unnecessary_wraps, reason = "it can fail when panics unwind")]
fn contain<T>(f: impl FnOnce() -> T) -> Result<T, Box<dyn std::any::Any + Send>> {
    #[cfg(panic = "unwind")]
    return std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
    #[cfg(not(panic = "unwind"))]
    return Ok(f());
}

/// The message of a contained engine panic; `then` says what it left.
fn internal_error_message(payload: &(dyn std::any::Any + Send), then: &str) -> String {
    let what = payload
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
        .unwrap_or("unknown");
    format!("micro-monty internal error (engine panic: {what}); {then}")
}

/// The compile memory [`Session::compile`] reserves per source byte: the
/// worst measured is about 405 \[M\].
pub const COMPILE_BYTES_PER_SOURCE_BYTE: usize = 512;

/// The compile memory [`Session::compile`] reserves for any source: about
/// 6 KB for a one-line cell \[M\].
const COMPILE_FIXED_BYTES: usize = 16 << 10;

/// [`Session::compile`]'s error when the compile reservation does not fit.
fn compile_memory_error() -> CompileError {
    CompileError {
        type_name: "MemoryError".to_owned(),
        message: String::new(),
        rendered: "MemoryError\n".to_owned(),
        line: 0,
        column: 0,
    }
}

fn memory_error() -> PyException {
    PyException {
        type_name: "MemoryError".to_owned(),
        message: String::new(),
        traceback: "MemoryError\n".to_owned(),
        truncated: false,
    }
}

/// Renders a compile error as CPython prints a `SyntaxError`.
fn compile_error(e: &mm_compile::CompileError, source: &str, filename: &str) -> CompileError {
    let mut rendered = String::new();
    if e.pos.line > 0 {
        let _ = writeln!(rendered, "  File \"{filename}\", line {}", e.pos.line);
        if let Some(text) = mm_compile::source_lines(source).nth(e.pos.line as usize - 1) {
            // A null byte's line shows up to the null byte, with no caret.
            let null = e.msg == mm_parse::NULL_BYTES;
            let text = if null {
                text.split('\0').next().unwrap_or("")
            } else {
                text
            };
            let trimmed = text.trim_start();
            let indent = text.chars().count() - trimmed.chars().count();
            // CPython keeps the spaces before a null byte.
            let shown = if null { trimmed } else { trimmed.trim_end() };
            if !shown.is_empty() || null {
                let _ = writeln!(rendered, "    {shown}");
            }
            // CPython prints no caret for indentation errors.
            if !shown.is_empty() && e.kind != "IndentationError" && !null {
                let col = (e.pos.col as usize).saturating_sub(indent);
                let col = col.min(shown.chars().count());
                // A range on one line gets one caret per character.
                let width = if e.pos.end_line == e.pos.line && e.pos.end_col > e.pos.col {
                    (e.pos.end_col - e.pos.col) as usize
                } else {
                    1
                };
                let width = width.min(shown.chars().count().saturating_sub(col)).max(1);
                let _ = writeln!(rendered, "    {}{}", " ".repeat(col), "^".repeat(width));
            }
        }
    }
    let _ = writeln!(rendered, "{}: {}", e.kind, e.msg);
    CompileError {
        type_name: e.kind.to_owned(),
        message: e.msg.clone(),
        line: e.pos.line,
        column: e.pos.col,
        rendered,
    }
}

/// A Python session: globals, heap and the running program.
///
/// `Send` (it may be parked on one thread and resumed on another, so every
/// callback it holds is `Send` too) but not `Sync`.
pub struct Session {
    vm: Vm,
    returns_last_expr: bool,
    want_value: bool,
    /// What the stopped cell last reported ([`Session::pending`]), as the
    /// snapshot header stores it ([`image::write_stop`]; encoding a host call's
    /// arguments, unlike cloning them, takes no Rust stack per level);
    /// empty between cells.
    stop: Vec<u8>,
    /// [`Session::stop`]'s charge to the memory limit while it is kept.
    stop_charge: Option<mm_core::Scratch>,
    text_limit: usize,
    /// [`Session::set_text_cut`].
    text_cut: TextCut,
    /// [`Session::set_int_bits_text`].
    int_bits_text: bool,
    /// The fuel each cell starts with ([`Limits::fuel`] of the engine
    /// that made or restored the session).
    cell_fuel: Option<u64>,
    /// What the host set with [`Session::set_fuel`] since the last cell
    /// started: the next cell starts with it instead of `cell_fuel`.
    #[allow(
        clippy::option_option,
        reason = "not set, or set to a budget that may be unlimited"
    )]
    next_fuel: Option<Option<u64>>,
    /// [`Session::set_fuel_total`]: what is left of it.
    fuel_total: Option<u64>,
    /// Set by a contained engine panic: the message every later call
    /// reports. The heap may be half-mutated, so it is never touched again.
    poisoned: Option<String>,
}

impl fmt::Debug for Session {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Session").finish_non_exhaustive()
    }
}

impl Session {
    /// A fresh session with the engine's limits.
    ///
    /// # Errors
    ///
    /// A `MemoryError` if the memory limit cannot hold the builtins.
    pub fn new(engine: &Engine) -> Result<Session, PyException> {
        Session::new_with(engine, SessionSettings::default())
    }

    /// [`Session::new`], with `settings` instead of the defaults
    /// ([`SessionSettings`]).
    ///
    /// # Errors
    ///
    /// The errors of [`Session::new`].
    pub fn new_with(engine: &Engine, settings: SessionSettings) -> Result<Session, PyException> {
        let limits = engine.config.limits;
        let config = VmConfig {
            heap: mm_core::HeapConfig {
                memory_limit: limits.memory,
                ..mm_core::HeapConfig::default()
            },
            recursion_limit: limits.recursion_depth,
            fuel: limits.fuel,
            stdlib: mm_stdlib::MODULES,
        };
        match Vm::new(config) {
            Ok(mut vm) => {
                vm.set_known_modules(mm_stdlib::ALL_MODULES);
                vm.set_module_cache(engine.module_cache());
                vm.set_host_code_in_snapshots(engine.host_code_in_snapshots);
                let mut session = Session {
                    vm,
                    returns_last_expr: false,
                    want_value: true,
                    stop: Vec::new(),
                    stop_charge: None,
                    text_limit: usize::try_from(limits.memory)
                        .unwrap_or(usize::MAX)
                        .min(DEFAULT_TEXT_LIMIT),
                    text_cut: TextCut::Head,
                    int_bits_text: false,
                    cell_fuel: limits.fuel,
                    next_fuel: None,
                    fuel_total: None,
                    poisoned: None,
                };
                session.apply(settings);
                Ok(session)
            }
            Err(_) => Err(PyException {
                type_name: "MemoryError".to_owned(),
                message: format!(
                    "a new session does not fit the memory limit of {} bytes \
                     (Limits::memory); its builtins need more",
                    limits.memory
                ),
                traceback: format!(
                    "MemoryError: a new session does not fit the memory limit of {} bytes \
                     (Limits::memory); its builtins need more\n",
                    limits.memory
                ),
                truncated: false,
            }),
        }
    }

    /// Installs `settings` (a new or restored session's).
    fn apply(&mut self, settings: SessionSettings) {
        let SessionSettings {
            output,
            wall_clock,
            entropy,
            zone_source,
            text_cut,
            int_bits_text,
            gc_stress,
            gc_inc_stress,
            gc_incremental,
            gc_off,
        } = settings;
        if let Some(out) = output {
            self.vm.set_output(out);
        }
        self.vm.set_wall_clock(wall_clock);
        self.vm.set_entropy(entropy);
        self.vm.set_zone_source(zone_source);
        // A new or restored session runs without them.
        if gc_stress {
            self.vm.set_gc_stress(true);
        }
        if gc_inc_stress {
            self.vm.set_gc_inc_stress(true);
        }
        if let Some((slice, min_live)) = gc_incremental {
            self.vm.heap.set_incremental(slice, min_live);
        }
        self.vm.gc_off = gc_off;
        self.text_cut = text_cut;
        self.int_bits_text = int_bits_text;
    }

    /// Sets, or with `None` removes, the `__name__` and `__file__` globals
    /// cells run with. A new or reset session has `__name__ == '__main__'`
    /// and no `__file__`. Without `__name__`, code sees the builtins
    /// module's, as CPython's `exec(code, {})` does: functions and classes
    /// then carry no `__main__.` prefix in reprs and call errors.
    ///
    /// # Errors
    ///
    /// A `MemoryError` if the heap is full.
    pub fn set_module_dunders(
        &mut self,
        name: Option<&str>,
        file: Option<&str>,
    ) -> Result<(), PyException> {
        self.vm
            .set_module_dunders(name, file)
            .map_err(|_| self.exception())
    }

    /// Sets how host-call arguments (and [`Completion::value`]) convert:
    /// the byte budget, the depth bound, and whether what does not convert
    /// raises in Python (the default) or reaches the host as
    /// [`HostValue::Unconverted`] for it to decide.
    pub fn set_host_args(&mut self, args: HostArgs) {
        self.vm.set_host_args(args);
    }

    /// Sets the longest `repr` and `str` a [`Completion`] carries, in bytes
    /// (default: 1 MiB, or the memory limit if less); longer text is cut
    /// at a character boundary and [`Completion::repr_truncated`] or
    /// [`Completion::str_truncated`] is set. Printed output is not cut:
    /// the output callback receives all of it.
    /// A limit past the platform's address space means no limit.
    pub fn set_text_limit(&mut self, bytes: u64) {
        self.text_limit = usize::try_from(bytes).unwrap_or(usize::MAX);
    }

    /// How text past [`Session::set_text_limit`] is cut (default
    /// [`TextCut::Head`]). [`TextCut::HeadTail`] renders the whole text
    /// of a str, bytes or number first (bounded by the memory limit) and
    /// keeps both ends; for a container value it keeps its shape, or, when
    /// the text runs past eight times the limit (at least 1 MiB, where the
    /// rendering stops), its beginning and ` ... [more omitted]`.
    /// [`Completion::value`] stays whole either way. A host-side setting:
    /// not saved in snapshots ([`SessionSettings::text_cut`] sets it on a
    /// new or restored session).
    pub fn set_text_cut(&mut self, cut: TextCut) {
        self.text_cut = cut;
    }

    /// Replaces the sentence the engine appends for group `group` of
    /// unsupported imports or names (`Some("")` drops it, `None` restores
    /// the default; `{module}` stands for the module), for example
    /// `"Call the http tool instead"` for [`HintGroup::Network`]. Saved
    /// with the session and kept across [`Session::reset`].
    ///
    /// # Errors
    ///
    /// A `MemoryError` if the heap is full.
    pub fn set_hint(&mut self, group: HintGroup, text: Option<&str>) -> Result<(), PyException> {
        self.vm.set_hint(group, text).map_err(|_| self.exception())
    }

    /// The host calls of the running cell, or of the last one once it
    /// completed, raised or was cancelled, in call order: every call
    /// reported as [`Progress::HostCall`], with what became of it. Calls
    /// left unanswered when the cell ended are
    /// [`CallStatus::Abandoned`]. [`Session::start`] begins a new log;
    /// snapshots carry it.
    #[must_use]
    pub fn call_log(&self) -> Vec<HostCallRecord> {
        self.vm
            .call_log()
            .into_iter()
            .map(|(call, function, status)| HostCallRecord {
                call,
                function,
                name: self.vm.host_id_name(function).unwrap_or("").to_owned(),
                status,
            })
            .collect()
    }

    /// Whether a [`Completion`]'s text renders an int longer than 14,284
    /// bits (past CPython's 4300-digit limit) as `<int with N bits>`, at any
    /// depth of builtin containers, instead of the cell failing with
    /// CPython's `ValueError` (default `false`), a presentation code-mode
    /// hosts use for results a model reads. Text a user `__repr__`/`__str__` computes keeps
    /// CPython's behaviour, and so does every conversion inside the cell.
    /// Not saved in snapshots ([`SessionSettings::int_bits_text`] sets it
    /// on a new or restored session).
    pub fn set_int_bits_text(&mut self, on: bool) {
        self.int_bits_text = on;
    }

    /// GC stress, for testing the engine: collect garbage at every
    /// safepoint (every call and backward jump, not returns) instead of when
    /// allocation asks for it, so a value the collector cannot reach is
    /// reclaimed at once; over 1 MiB live, at the first safepoint after
    /// each 1/64 of the live bytes is allocated. Very slow. Not saved in
    /// snapshots ([`SessionSettings::gc_stress`]).
    pub fn set_gc_stress(&mut self, on: bool) {
        self.vm.set_gc_stress(on);
    }

    /// Under a fuel limit, ends a fuel slice early after a safepoint that
    /// ran a step of a garbage collection: the cell parks with
    /// [`ParkReason::Fuel`] and fuel left ([`Session::fuel`] above 0).
    /// Collection work charges no fuel (when collections fall due differs
    /// between targets and after a restore, and fuel totals do not), so a
    /// host that regains control only between fuel slices can otherwise
    /// wait for many collection steps in one slice (22–52 ms at 59–199 MiB
    /// live in 10,000-unit slices \[M\]). With it, it waits for at most
    /// one step plus a slice. Fuel charged and results are the same either
    /// way. Off by default, since a host that reads every fuel park as its
    /// budget running out must not set it; [`Session::drive_async`] turns
    /// it on while it slices. Not saved in snapshots.
    pub fn set_gc_yield(&mut self, on: bool) {
        self.vm.gc_yield = on;
    }

    /// Incremental GC stress, for testing the engine: an incremental mark
    /// is always in progress, a few objects per safepoint, and each one is
    /// checked against a full mark when it ends, so a write the collector
    /// does not see aborts the process. Very slow. Not saved in snapshots
    /// ([`SessionSettings::gc_inc_stress`]).
    #[doc(hidden)]
    pub fn set_gc_inc_stress(&mut self, on: bool) {
        self.vm.set_gc_inc_stress(on);
    }

    /// Testing: `slice` work units per incremental collection step (0: full
    /// collections only) from `min_live` live bytes (default 32 MiB). Not
    /// saved in snapshots ([`SessionSettings::gc_incremental`]).
    #[doc(hidden)]
    pub fn set_gc_incremental(&mut self, slice: u64, min_live: u64) {
        self.vm.heap.set_incremental(slice, min_live);
    }

    /// Testing: is an incremental collection in progress?
    #[doc(hidden)]
    #[must_use]
    pub fn gc_marking(&self) -> bool {
        self.vm.heap.marking()
    }

    /// Whether a completed cell reports the value of its last expression
    /// (default `true`). Running a script, turn it off: computing the
    /// value's text may run a user `__repr__` or `__str__`.
    pub fn set_last_expr_value(&mut self, want: bool) {
        self.want_value = want;
    }

    /// Sets the callback that receives `print` and `sys.stdout`/`sys.stderr`
    /// output, both streams merged in production order. Not saved in
    /// snapshots ([`SessionSettings::output`]).
    pub fn set_output(&mut self, mut out: Box<dyn FnMut(&str) + Send>) {
        self.vm.set_output(Box::new(move |_, s: &str| out(s)));
    }

    /// Like [`Session::set_output`], with each piece tagged by its stream.
    pub fn set_output_streams(&mut self, out: StreamOutput) {
        self.vm.set_output(out);
    }

    /// Sets the fuel the cell may still charge (`None` = unlimited). On a
    /// stopped cell, for example after [`ParkReason::Fuel`], it is the
    /// cell's new budget. Between cells it is the next cell's budget,
    /// instead of [`Limits::fuel`]; every later cell gets `Limits::fuel`
    /// again.
    pub fn set_fuel(&mut self, fuel: Option<u64>) {
        self.vm.set_fuel(fuel);
        self.next_fuel = self.stop.is_empty().then_some(fuel);
    }

    /// Caps the fuel of all of the session's cells together (`None`, the
    /// default, is no cap). Each cell starts with the smaller of its own
    /// budget and what is left of the total, and the fuel every cell
    /// charges under a limit is taken from it, so a session parks with
    /// [`ParkReason::Fuel`] for good once it is spent and
    /// [`Session::fuel_total`] is `Some(0)`. Refuelling a parked cell with
    /// [`Session::set_fuel`] is the host's decision and is not capped, but
    /// what the cell then charges still counts. Session state: snapshots
    /// and templates keep what is left.
    pub fn set_fuel_total(&mut self, total: Option<u64>) {
        self.fuel_total = total;
    }

    /// What is left of [`Session::set_fuel_total`]'s cap, or `None`
    /// without one.
    #[must_use]
    pub fn fuel_total(&self) -> Option<u64> {
        self.fuel_total
    }

    /// [`Engine::environment`] for this session: its own limits (a
    /// restored session's may be smaller than its engine's) and what is
    /// left of its fuel total.
    #[must_use]
    pub fn environment(&self) -> String {
        environment(
            Limits {
                memory: self.vm.heap.memory_limit(),
                recursion_depth: self.vm.recursion_ceiling(),
                fuel: self.cell_fuel,
            },
            self.fuel_total,
        )
    }

    /// What the session is stopped at, and so which [`Resume`] it takes:
    /// for a restored session, what its image was saved at.
    #[must_use]
    pub fn phase(&self) -> Phase {
        match self.stop.first() {
            Some(1) => Phase::HostCall,
            Some(2) => Phase::Waiting,
            Some(3) => Phase::Parked,
            _ => Phase::Idle,
        }
    }

    /// What the stopped cell is stopped at, as [`Session::start`] or
    /// [`Session::resume`] reported it: [`Progress::HostCall`] with its
    /// call id, function, name, arguments and keyword arguments,
    /// [`Progress::Waiting`] with the calls still pending, or
    /// [`Progress::Parked`] with its reason; `None` between cells. A
    /// restored session reports the stop its image was saved at, so a host
    /// need not keep the payload beside the image. The session keeps a
    /// copy of a host call's arguments until it is answered (snapshots
    /// carry it).
    #[must_use]
    pub fn pending(&self) -> Option<Progress> {
        // The session wrote it: no limit.
        let mut budget = u64::MAX;
        image::read_stop(&mut mm_core::Reader::new(&self.stop), &mut budget)
            .ok()
            .flatten()
    }

    /// Gives a starting cell its fuel: [`Limits::fuel`], or what the host
    /// set between cells, capped by what is left of the total.
    pub(crate) fn refuel(&mut self) {
        let grant = self.next_fuel.take().unwrap_or(self.cell_fuel);
        let grant = match self.fuel_total {
            Some(total) => Some(grant.map_or(total, |g| g.min(total))),
            None => grant,
        };
        self.vm.set_fuel(grant);
    }

    /// Runs one stretch of the cell and takes the fuel it charged from
    /// the total.
    fn metered(&mut self, run: impl FnOnce(&mut Vm) -> Exit) -> Exit {
        let (before, overdraft_before) = self.vm.fuel_account();
        let exit = run(&mut self.vm);
        if let (Some(total), Some(before)) = (self.fuel_total, before) {
            let (left, overdraft) = self.vm.fuel_account();
            let used = before
                .saturating_sub(left.unwrap_or(0))
                .saturating_add(overdraft.saturating_sub(overdraft_before));
            self.fuel_total = Some(total.saturating_sub(used));
        }
        exit
    }

    /// Fuel left (`None` = unlimited), as of the last fuel check: one
    /// unit per safepoint (call or backward jump) run, plus what natives
    /// charge for long operations. The engine checks every 256
    /// safepoints (every one once less fuel is left), so read while the
    /// cell runs (from a host call) it can be up to 255 units more than
    /// is left; at a park it is exact.
    #[must_use]
    pub fn fuel(&self) -> Option<u64> {
        self.vm.fuel()
    }

    /// After [`ParkReason::Fuel`]: the fuel the next operation will charge
    /// when that is more than was left, so it parked before starting; 0
    /// after an ordinary fuel park. Resuming runs it whatever fuel is then
    /// set, so a host that enforces a budget stops the cell instead of
    /// resuming.
    ///
    /// The operations that estimate their cost first, and park when it is
    /// 65,536 units or more and over the fuel left:
    ///
    /// - the int operators `*`, `**`, `/`, `//` and `%`;
    /// - called directly from Python code: `pow` (two and three
    ///   arguments), `divmod`, `sum` and `math.prod` of a list or tuple,
    ///   `math.factorial`, `perm`, `comb`, `isqrt`, `gcd` and `lcm`, and
    ///   `sorted` and `list.sort` of a list or tuple.
    ///
    /// The same builtins called by another native (`map(pow, ...)`, a
    /// `key=` function) and every other native charge after they run.
    #[must_use]
    pub fn fuel_wanted(&self) -> u64 {
        self.vm.fuel_wanted()
    }

    /// Like [`Session::set_deadline`], on the host's monotonic clock: `now`
    /// returns nanoseconds, and the cell parks at the first safepoint
    /// where `now() >= deadline`. For targets without
    /// [`std::time::Instant`] (`wasm32-unknown-unknown`) or a virtual clock.
    /// `now` is read before the park flag, so it may also poll a host for
    /// a park request and pass it on through a [`ParkHandle`]: the cell
    /// then parks there with [`ParkReason::Requested`].
    pub fn set_deadline_with(&mut self, now: Box<dyn Fn() -> u64 + Send>, deadline: u64) {
        self.vm.set_clock(Some(now));
        self.vm.set_deadline(Some(deadline));
    }

    /// Parks with [`ParkReason::Deadline`] at the first safepoint after
    /// `deadline` (`None` clears it). One-shot: it is cleared when it fires,
    /// so set a new one before resuming. (`Instant::now` panics on
    /// `wasm32-unknown-unknown`; use [`Session::set_deadline_with`] there.)
    pub fn set_deadline(&mut self, deadline: Option<Instant>) {
        match deadline {
            None => {
                self.vm.set_clock(None);
                self.vm.set_deadline(None);
            }
            Some(at) => {
                let base = Instant::now();
                let nanos =
                    |d: std::time::Duration| u64::try_from(d.as_nanos()).unwrap_or(u64::MAX);
                self.vm
                    .set_clock(Some(Box::new(move || nanos(base.elapsed()))));
                self.vm
                    .set_deadline(Some(nanos(at.saturating_duration_since(base))));
            }
        }
    }

    /// Sets the wall clock that `time.time()`, `datetime.now()`,
    /// `date.today()` and the other clock readers use: seconds since the
    /// Unix epoch, UTC. The default reads the system clock
    /// (`SystemTime::now`; none on `wasm32-unknown-unknown`); `None` makes
    /// those calls raise `RuntimeError`.
    /// The engine itself has no clock: a deterministic or virtual clock is
    /// the host's choice. `time.sleep` does not wait: it advances the
    /// clocks this session reads by the delay (unless the host registers a
    /// `time.sleep` host function, see [`Session::define`]). Not saved in
    /// snapshots ([`SessionSettings::wall_clock`]).
    pub fn set_wall_clock(&mut self, clock: Option<WallClock>) {
        self.vm.set_wall_clock(clock);
    }

    /// Sets the local time zone as TZif data (RFC 8536, the format of
    /// `/usr/share/zoneinfo` files; a v2+ footer's POSIX TZ rule covers
    /// instants past the last transition): what `time.localtime`,
    /// `time.mktime`, `time.timezone`/`tzname` and naive `datetime`
    /// conversions (`astimezone()`, `timestamp()`) see. The default, and
    /// `None`, is UTC: the engine reads no ambient zone. `time` reads it
    /// on first import. Unlike the clock, the zone is session state: a
    /// snapshot carries it, so a restored session keeps its rules.
    pub fn set_local_timezone(&mut self, tzif: Option<Vec<u8>>) {
        self.vm.set_local_zone(tzif);
    }

    /// Sets where `zoneinfo.ZoneInfo(key)` gets zone `key`'s TZif data
    /// (RFC 8536): the source returns the bytes, or `None` for an unknown
    /// key (`ZoneInfoNotFoundError`). Keys arrive validated as CPython
    /// validates them (relative, normalized, no `..`). The default, and
    /// `None`, is no source: only `ZoneInfo('UTC')` loads, and other keys
    /// raise `ZoneInfoNotFoundError` saying the host provides no time zone
    /// data. Like the clock, the source is not session state; the zones a
    /// session has loaded are, so a snapshot keeps their rules, and a
    /// restored session answers them from its cache without the source
    /// ([`SessionSettings::zone_source`] sets it on a restored session).
    pub fn set_zone_source(&mut self, source: Option<ZoneSource>) {
        self.vm.set_zone_source(source);
    }

    /// Sets where `os.urandom`, `random`'s seeding without a seed,
    /// `random.SystemRandom` and `uuid.uuid4` get random bytes. The
    /// default is SipHash keyed from the operating system's randomness
    /// (std's `RandomState`) over a counter (none on
    /// `wasm32-unknown-unknown`); `None` makes those calls raise
    /// `RuntimeError`. Not saved in snapshots
    /// ([`SessionSettings::entropy`]).
    pub fn set_entropy(&mut self, entropy: Option<Entropy>) {
        self.vm.set_entropy(entropy);
    }

    /// A handle that parks the running cell at its next safepoint; clone it
    /// to another thread. A request stays until the park it causes takes
    /// it: one made while no cell runs (between cells, or while a cell is
    /// stopped for the host) parks the next stretch of running code at its
    /// first safepoint, whatever [`Session::start`],
    /// [`Session::cancel`] or [`Session::resume`] comes in between.
    #[must_use]
    pub fn park_handle(&self) -> ParkHandle {
        ParkHandle(self.vm.park_flag())
    }

    /// Requests a park at the next safepoint (see [`Session::park_handle`]
    /// for other threads).
    pub fn request_park(&self) {
        self.park_handle().request();
    }

    /// [`Engine::compile`], charged to this session's memory limit: the
    /// parser's tree, the tokens and the compiler's tables are temporary
    /// memory that does not reach the arena, up to about 405 bytes per
    /// source byte \[M\] (one-character statements, just past a doubling of
    /// the statement vector), so [`COMPILE_BYTES_PER_SOURCE_BYTE`] per
    /// byte is reserved while compiling. A cell too big for the memory
    /// left (after a collection, when no cell is stopped) fails with a
    /// `MemoryError`. Use this for cells a guest or model writes;
    /// [`Engine::compile`] stays outside every session's limit.
    ///
    /// # Errors
    ///
    /// [`Engine::compile`]'s errors, and a [`CompileError`] whose
    /// `type_name` is `MemoryError` when the reservation does not fit.
    pub fn compile(&mut self, source: &str, filename: &str) -> Result<Code, CompileError> {
        if u64::try_from(source.len()).unwrap_or(u64::MAX) > MAX_SOURCE_BYTES {
            return Engine::compile_unguarded(source, filename);
        }
        let need = source
            .len()
            .saturating_mul(COMPILE_BYTES_PER_SOURCE_BYTE)
            .saturating_add(COMPILE_FIXED_BYTES);
        let hold = match self.vm.heap.reserve_scratch(need) {
            Ok(hold) => hold,
            // Garbage the last cell left may be what is in the way.
            Err(_) if self.stop.is_empty() => {
                self.vm.collect_now();
                self.vm
                    .heap
                    .reserve_scratch(need)
                    .map_err(|_| compile_memory_error())?
            }
            Err(_) => return Err(compile_memory_error()),
        };
        let code = Engine::compile_contained(source, filename);
        drop(hold);
        code
    }

    /// Runs `code` in this session's globals until it completes, raises or
    /// stops for the embedder. A cell that was still stopped is cancelled
    /// first; a pending park request stays and parks the new cell (see
    /// [`Session::park_handle`]). The cell's fuel is [`Limits::fuel`], or
    /// what [`Session::set_fuel`] set since the last cell, capped by
    /// [`Session::set_fuel_total`].
    pub fn start(&mut self, code: &Code) -> Progress {
        self.start_cell(code, true)
    }

    /// [`Session::start`], giving the cell its fuel first or not (the
    /// async driver has, and set its first slice).
    fn start_cell(&mut self, code: &Code, refuel: bool) -> Progress {
        self.guarded(|s| {
            s.vm.prepare_cell();
            s.vm.clear_call_log();
            if refuel {
                s.refuel();
            }
            let Ok(entry) = s.vm.load(&code.program, &code.filename, &code.source) else {
                return s.error();
            };
            s.returns_last_expr = code.program.returns_last_expr && s.want_value;
            s.vm.set_want_completion_text(s.returns_last_expr);
            let exit = s.metered(|vm| vm.run(entry));
            test_hooks::point("start");
            s.progress(exit)
        })
        .unwrap_or_else(Progress::Error)
    }

    /// Continues a stopped cell with the answer it waits for.
    ///
    /// # Errors
    ///
    /// A [`ResumeError`] if the answer does not fit what the cell stopped
    /// for, or a batch names a call that is unknown or already resolved;
    /// nothing changes then. An answer to a call whose future the cell
    /// cancelled (its record is [`CallStatus::Abandoned`]) is dropped.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "an answer is consumed; the engine converts it in place"
    )]
    pub fn resume(&mut self, resume: Resume) -> Result<Progress, ResumeError> {
        self.guarded(|s| s.resume_unguarded(&resume))
            .unwrap_or_else(|e| Ok(Progress::Error(e)))
    }

    fn resume_unguarded(&mut self, resume: &Resume) -> Result<Progress, ResumeError> {
        let fits = matches!(
            (self.phase(), resume),
            (Phase::Parked, Resume::Continue)
                | (
                    Phase::HostCall,
                    Resume::Value(_) | Resume::Error(_) | Resume::Future
                )
                | (Phase::Waiting, Resume::Batch(_))
        );
        if !fits {
            return Err(ResumeError::Unexpected {
                phase: self.phase(),
            });
        }
        if let Resume::Batch(results) = resume {
            let mut seen = std::collections::BTreeSet::new();
            for (call, _) in results {
                match self.vm.future_status(*call) {
                    FutureStatus::Unknown => return Err(ResumeError::UnknownCall { call: *call }),
                    FutureStatus::Settled => {
                        return Err(ResumeError::AlreadySettled { call: *call });
                    }
                    // The cell cancelled it while the host ran it: the
                    // answer is dropped.
                    FutureStatus::Cancelled => {}
                    FutureStatus::Pending => {
                        if !seen.insert(*call) {
                            return Err(ResumeError::AlreadySettled { call: *call });
                        }
                    }
                }
            }
        }
        let exit = self.metered(|vm| match resume {
            Resume::Value(v) => vm.resume_value(v),
            Resume::Error(e) => vm.resume_error(e),
            Resume::Future => vm.resume_future(),
            Resume::Batch(results) => vm.resume_batch(results),
            Resume::Continue => vm.resume(),
        });
        test_hooks::point("resume");
        Ok(self.progress(exit))
    }

    /// Ends the stopped cell (parked, at a host call or waiting) without
    /// running any more of it: no `finally` blocks run and pending host
    /// futures are dropped. Globals keep what the cell assigned, so the
    /// session stays usable. To stop a cell running on another thread,
    /// [`ParkHandle::request`] first, then cancel the parked cell. A park
    /// request made since the park stays for the next cell. Returns
    /// whether a cell was stopped.
    pub fn cancel(&mut self) -> bool {
        self.guarded(|s| {
            s.stop = Vec::new();
            let cancelled = s.vm.cancel();
            test_hooks::point("cancel");
            cancelled
        })
        .unwrap_or(false)
    }

    /// Saves the session: collects garbage, then writes its FULL image
    /// (heap, code, frames, tasks, pending host calls, globals and limits)
    /// for [`Engine::restore`]. Works whenever the session is not running:
    /// between cells, parked, at a host call or waiting on host futures.
    /// Output already delivered, the output callback, the deadline and host
    /// handles are the host's
    /// ([spec](https://github.com/N0xMare/micro-monty/blob/main/docs/spec.md) §9.1).
    ///
    /// Format: the magic `MMSNAP\0\x01`, a `u16` format version, the
    /// 32-byte build id, the facade's session state (last-expression
    /// flags, text limit, fuel total; not [`SessionSettings`]) and the stop
    /// ([`Session::pending`]), the heap's live runs per
    /// segment, the code table and the VM registers, then an XXH64 checksum
    /// of everything before it.
    ///
    /// Output capacity is separately limited to the session's memory limit.
    /// It is not charged against live state, so a full session can still be
    /// checkpointed. Use [`Session::snapshot_with_limit`] to choose a different
    /// cap. The returned buffer belongs to the caller; this is not an RSS cap.
    ///
    /// # Errors
    ///
    /// [`SnapshotError::Busy`] inside an operation (not reachable through
    /// this API); [`SnapshotError::Poisoned`] once an engine panic poisoned
    /// the session; [`SnapshotError::TooLarge`] if output cannot fit or allocate.
    pub fn snapshot(&mut self) -> Result<Vec<u8>, SnapshotError> {
        self.snapshot_with_limit(self.vm.heap.memory_limit())
    }

    /// Like [`Session::snapshot`], with an explicit output-buffer capacity cap
    /// including the header and checksum. Growth checks before allocating.
    /// A failed snapshot leaves the session usable, although GC may have run.
    /// Allocator overhead and transient reallocation peaks are not included.
    ///
    /// # Errors
    ///
    /// The same errors as [`Session::snapshot`], with [`SnapshotError::TooLarge`]
    /// when the output exceeds `max_bytes` or allocation fails. A cap past
    /// the platform's address space means no cap.
    pub fn snapshot_with_limit(&mut self, max_bytes: u64) -> Result<Vec<u8>, SnapshotError> {
        let max_bytes = usize::try_from(max_bytes).unwrap_or(usize::MAX);
        self.guarded(|s| s.snapshot_unguarded(max_bytes))
            .unwrap_or(Err(SnapshotError::Poisoned))
    }

    fn snapshot_unguarded(&mut self, max_bytes: usize) -> Result<Vec<u8>, SnapshotError> {
        image::write(self, max_bytes)
    }

    /// Cancels any stopped cell and clears globals and imported modules.
    /// Unreachable user code is reclaimed; host functions and host modules stay.
    ///
    /// # Errors
    ///
    /// A `MemoryError` if the heap is full.
    pub fn reset(&mut self) -> Result<(), PyException> {
        self.guarded(|s| {
            s.stop = Vec::new();
            let reset = s.vm.reset().map_err(|_| s.exception());
            test_hooks::point("reset");
            reset
        })
        .unwrap_or_else(Err)
    }

    /// Whether an engine panic poisoned this session (see the crate docs).
    #[must_use]
    pub fn is_poisoned(&self) -> bool {
        self.poisoned.is_some()
    }

    /// Runs `op` unless the session is poisoned, and poisons it if `op`
    /// panics (with `panic = "abort"` the catch is a no-op). The error is
    /// the `SystemError` every later call reports.
    #[inline]
    #[allow(clippy::unnecessary_wraps, reason = "it can fail when panics unwind")]
    fn guarded<T>(&mut self, op: impl FnOnce(&mut Session) -> T) -> Result<T, PyException> {
        // Nothing can poison a session that cannot unwind.
        #[cfg(not(panic = "unwind"))]
        return Ok(op(self));
        #[cfg(panic = "unwind")]
        if self.poisoned.is_none() {
            match contain(|| op(self)) {
                Ok(done) => return Ok(done),
                Err(payload) => {
                    self.stop = Vec::new();
                    self.poisoned = Some(internal_error_message(
                        payload.as_ref(),
                        "the session is poisoned",
                    ));
                }
            }
        }
        #[cfg(panic = "unwind")]
        let message = self.poisoned.clone().unwrap_or_default();
        #[cfg(panic = "unwind")]
        Err(PyException {
            type_name: "SystemError".to_owned(),
            traceback: format!("SystemError: {message}\n"),
            message,
            truncated: false,
        })
    }

    fn progress(&mut self, exit: Exit) -> Progress {
        self.stop_charge = None;
        // Keep a small buffer for the next stop; let a large one go.
        if self.stop.capacity() > 4096 {
            self.stop = Vec::new();
        } else {
            self.stop.clear();
        }
        match exit {
            Exit::Complete(v) => {
                if !self.returns_last_expr {
                    return Progress::Complete(None);
                }
                let whole = usize::try_from(self.vm.heap.memory_limit()).unwrap_or(usize::MAX);
                let limit = match self.text_cut {
                    TextCut::Head => self.text_limit,
                    // A leaf's text is one step: whole, so the cut keeps
                    // its end. A container's walk stops a bounded way past
                    // the limit (it may describe far more text than the
                    // memory holds); past that the cut keeps its shape from
                    // the host value, or its beginning.
                    _ if matches!(
                        self.vm.type_name_of(v),
                        "str" | "bytes" | "bytearray" | "int" | "float" | "complex"
                    ) =>
                    {
                        whole
                    }
                    _ => self
                        .text_limit
                        .saturating_mul(8)
                        .max(HEAD_TAIL_WALK)
                        .min(whole),
                };
                let text = if let Some(text) = self.vm.take_completion_text_capped(limit) {
                    text
                } else {
                    self.vm.set_int_bits_text(self.int_bits_text);
                    let text = (
                        self.vm.repr_capped_of(v, limit),
                        self.vm.str_capped(v, limit),
                    );
                    self.vm.set_int_bits_text(false);
                    let (Ok(repr), Ok(str)) = text else {
                        return self.error();
                    };
                    (repr, str)
                };
                let value = self.vm.value_to_host(v);
                let ((mut repr, mut repr_truncated), (mut str, mut str_truncated)) = text;
                if self.text_cut == TextCut::HeadTail {
                    let max = self.text_limit;
                    // The walk stopped at `limit`: the end is not known.
                    let (repr_ended, str_ended) = (!repr_truncated, !str_truncated);
                    if repr.len() > max {
                        // The shaped cut only where it agrees with the real
                        // `repr` up to its first marker: a class's own
                        // `__repr__` (a namedtuple subclass, say) is not in
                        // the host value.
                        repr = value
                            .as_ref()
                            .and_then(|h| render::structured(h, max))
                            .filter(|s| repr.starts_with(&s[..s.find("...(+").unwrap_or(s.len())]))
                            .unwrap_or_else(|| {
                                if repr_ended {
                                    render::cut_head_tail(&repr, max).0
                                } else {
                                    render::head_more(&repr, max)
                                }
                            });
                        repr_truncated = true;
                    }
                    if str.len() > max {
                        str = if str_ended {
                            render::cut_head_tail(&str, max).0
                        } else {
                            render::head_more(&str, max)
                        };
                        str_truncated = true;
                    }
                }
                Progress::Complete(Some(Completion {
                    repr,
                    str,
                    type_name: self.vm.type_name_of(v).to_owned(),
                    value,
                    repr_truncated,
                    str_truncated,
                }))
            }
            Exit::Parked(r) => {
                let reason = match r {
                    mm_engine::ParkReason::Fuel => ParkReason::Fuel,
                    mm_engine::ParkReason::Requested => ParkReason::Requested,
                    mm_engine::ParkReason::Deadline => ParkReason::Deadline,
                };
                self.stopped(Progress::Parked { reason })
            }
            Exit::HostCall(c) => self.stopped(Progress::HostCall {
                call: c.id,
                function: c.function,
                name: c.name,
                args: c.args,
                kwargs: c.kwargs,
            }),
            Exit::Waiting(calls) => self.stopped(Progress::Waiting { calls }),
            Exit::Error => self.error(),
        }
    }

    /// Reports stop `p`, keeping a copy for [`Session::pending`], charged
    /// to the memory limit while it is kept. It fits: a host call's
    /// arguments were just charged as their larger `HostValue` trees.
    fn stopped(&mut self, p: Progress) -> Progress {
        image::write_stop(&mut self.stop, &p);
        self.stop_charge = self.vm.heap.reserve_scratch(self.stop.capacity()).ok();
        p
    }

    fn exception(&mut self) -> PyException {
        match self.error() {
            Progress::Error(e) => e,
            _ => memory_error(),
        }
    }

    /// The uncaught exception's report, its text bounded as a completion's
    /// is: rendering stops past the text limit (or past the deadline during
    /// long work), and the text is cut by [`Session::set_text_cut`].
    fn error(&mut self) -> Progress {
        let exc = self.vm.exception();
        let whole = usize::try_from(self.vm.heap.memory_limit()).unwrap_or(usize::MAX);
        let walk = match self.text_cut {
            TextCut::Head => self.text_limit,
            TextCut::HeadTail => self.text_limit.saturating_mul(8).max(HEAD_TAIL_WALK),
        };
        self.vm.begin_report(walk.min(whole));
        let (type_name, message) = self.vm.exception_parts(exc);
        let message_stopped = self.vm.end_report();
        self.vm.begin_report(walk.min(whole));
        let traceback = self.vm.render_exception(exc);
        let traceback_stopped = self.vm.end_report();
        let (message, message_cut) = self.cut_report(message, message_stopped);
        let (traceback, traceback_cut) = self.cut_report(traceback, traceback_stopped);
        Progress::Error(PyException {
            type_name,
            message,
            traceback,
            truncated: message_stopped || traceback_stopped || message_cut || traceback_cut,
        })
    }

    /// Report text cut to the text limit: its beginning, or with
    /// [`TextCut::HeadTail`] both ends (or, when rendering `stopped`, its
    /// beginning and ` ... [more omitted]`).
    fn cut_report(&self, text: String, stopped: bool) -> (String, bool) {
        let max = self.text_limit;
        if text.len() <= max {
            return (text, false);
        }
        let cut = match self.text_cut {
            TextCut::Head => render::floor(&text, max).to_owned(),
            TextCut::HeadTail if stopped => render::head_more(&text, max),
            TextCut::HeadTail => render::cut_head_tail(&text, max).0,
        };
        (cut, true)
    }
}
