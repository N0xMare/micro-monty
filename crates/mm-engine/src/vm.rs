//! The VM: session state, code loading, errors and object helpers.

use crate::host::{HostCall, HostModule};
use crate::types::{NTYPES, T, ty};
use alloc::{boxed::Box, collections::VecDeque, string::String, sync::Arc, vec, vec::Vec};
use core::sync::atomic::AtomicBool;
use mm_compile::bytecode::{Code, Konst, Program};
use mm_core::{Heap, HeapConfig, Ref, StrBuf, Value};

/// Marker for "a Python exception is pending"; the exception is in the VM.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Raised;

/// The arguments of a builtin call, on the VM stack: `n` positional values
/// from `base` (the receiver first, for methods) and, for functions with
/// keyword-only parameters, their values from `kw` (`UNDEFINED` if absent).
#[derive(Clone, Copy, Debug)]
pub struct CallArgs {
    /// Stack index of the first positional argument.
    pub base: u32,
    /// Number of positional arguments.
    pub n: u32,
    /// Stack index of the keyword-only values.
    pub kw: u32,
}

/// A bundled Python module, compiled to bytecode at build time; imported
/// on first `import` (docs/spec.md §12).
#[derive(Clone, Copy, Debug)]
pub struct StdModule {
    /// The dotted module name.
    pub name: &'static str,
    /// With `packing`, the sections of `Program::encode_sections`, each
    /// packed against that section's codes in [`Packing::headers`]: the
    /// LEB128 lengths of all but the last, then the sections. Without, the
    /// program (`Program::encode`) packed alone (`mm_compile::pack::pack`).
    pub code: &'static [u8],
    /// What the bundled modules share.
    pub packing: Option<&'static Packing>,
    /// With the `host-bundle` feature: `code` is empty and the module's
    /// bytes are at this offset and length of the host's stdlib bundle
    /// (region 1 of [`ModuleCache::bundle`]). Default builds have no such
    /// path at all.
    #[cfg(feature = "host-bundle")]
    pub external: (u32, u32),
}

/// What the bundled modules' [`StdModule::code`] shares: one set of
/// Huffman codes per section (`mm_compile::pack::pack_shared`) and the
/// strings several modules use (`mm_compile::SharedStrings`).
#[derive(Clone, Copy, Debug)]
pub struct Packing {
    /// Each section's codes, in `mm_compile::section` order.
    pub headers: [&'static [u8]; mm_compile::section::COUNT],
    /// The shared strings, one after another.
    pub text: &'static str,
    /// Where each shared string ends in `text`, as little-endian `u16`s.
    pub ends: &'static [u8],
}

/// One decoder per [`Packing`] section, built on first use and kept across
/// the modules a cell (or a restore) decodes. Each part is still
/// charged its decoder workspace, so charges do not depend on what was
/// built before.
pub(crate) type Unpackers =
    Option<Box<[mm_compile::pack::Unpacker<'static>; mm_compile::section::COUNT]>>;

impl Packing {
    /// The program packed in `code`, admitting each unpacked section and
    /// the decoded buffers within `budget`.
    fn decode(
        &'static self,
        code: &[u8],
        unpackers: &mut Unpackers,
        budget: &mut u64,
    ) -> Result<Program, mm_compile::DecodeError> {
        use mm_compile::{DecodeError, section::COUNT};
        let mut lens = [0usize; COUNT];
        let mut rest = code;
        for len in &mut lens[..COUNT - 1] {
            let mut shift = 0;
            loop {
                let (&b, tail) = rest.split_first().ok_or(DecodeError::Invalid)?;
                rest = tail;
                *len |= usize::from(b & 0x7f)
                    .checked_shl(shift)
                    .ok_or(DecodeError::Invalid)?;
                shift += 7;
                if b & 0x80 == 0 {
                    break;
                }
            }
        }
        let unpackers = unpackers.get_or_insert_with(|| {
            Box::new(core::array::from_fn(|k| {
                mm_compile::pack::Unpacker::new(self.headers[k])
            }))
        });
        let mut sections: [Vec<u8>; COUNT] = Default::default();
        for (k, len) in lens.into_iter().enumerate() {
            // The last section is the rest.
            let len = if k == COUNT - 1 { rest.len() } else { len };
            let (part, tail) = rest.split_at_checked(len).ok_or(DecodeError::Invalid)?;
            rest = tail;
            sections[k] = unpackers[k].unpack_bounded(part, budget)?;
        }
        let refs: [&[u8]; COUNT] = core::array::from_fn(|k| sections[k].as_slice());
        let shared = mm_compile::SharedStrings::new(self.text, self.ends);
        Program::decode_sections(&refs, shared, budget)
    }
}

impl StdModule {
    /// Module `i` of the stdlib, from `cache` or decoded within a workspace
    /// budget including unpacking, the string pool and the owned Program
    /// that stays alive while its codes are loaded. A cached module charges
    /// `budget` the workspace its decoding took, so a session's accounting
    /// and errors do not depend on what the cache holds.
    pub(crate) fn decode(
        &self,
        i: usize,
        cache: Option<&dyn ModuleCache>,
        unpackers: &mut Unpackers,
        budget: &mut u64,
    ) -> Result<Arc<DecodedModule>, mm_compile::DecodeError> {
        if let Some(m) = cache.and_then(|c| c.get(i)) {
            *budget = budget
                .checked_sub(m.workspace)
                .ok_or(mm_compile::DecodeError::TooLarge)?;
            return Ok(m);
        }
        let start = *budget;
        #[cfg(feature = "host-bundle")]
        let fetched = {
            let (at, len) = self.external;
            let mut b = alloc::vec![0; len as usize];
            if !cache.is_some_and(|c| c.bundle(1, at, &mut b)) {
                return Err(mm_compile::DecodeError::Invalid);
            }
            b
        };
        #[cfg(feature = "host-bundle")]
        let code: &[u8] = &fetched;
        #[cfg(not(feature = "host-bundle"))]
        let code = self.code;
        let program = if let Some(p) = self.packing {
            p.decode(code, unpackers, budget)?
        } else {
            let bytes = mm_compile::pack::unpack_bounded(code, budget)?;
            Program::decode_bounded(&bytes, budget)?
        };
        let m = Arc::new(DecodedModule {
            program,
            workspace: start - *budget,
        });
        if let Some(c) = cache {
            c.offer(i, &m);
        }
        Ok(m)
    }
}

/// A bundled module's decoded program, shared through a [`ModuleCache`].
#[derive(Debug)]
pub struct DecodedModule {
    pub(crate) program: Program,
    /// The decoder workspace it took, charged to each session that loads it.
    pub(crate) workspace: u64,
}

impl DecodedModule {
    /// The bytes it keeps alive: code objects, their shared allocations and
    /// buffers.
    #[must_use]
    pub fn bytes(&self) -> u64 {
        let per_code = (size_of::<Code>() + 3 * size_of::<usize>()) as u64;
        self.program
            .codes
            .iter()
            .map(|c| per_code + c.buffer_bytes())
            .sum()
    }
}

/// A chunk of the prelude's units, unpacked, shared through a
/// [`ModuleCache`].
#[derive(Debug)]
pub struct PreludeChunk {
    /// Its parts, unpacked: the nine program sections of its units, then
    /// their records.
    pub(crate) parts: [Vec<u8>; crate::prelude::PARTS],
    /// The unpacking workspace it took, charged to each session that uses
    /// it.
    pub(crate) workspace: u64,
}

impl PreludeChunk {
    /// The bytes it keeps alive.
    #[must_use]
    pub fn bytes(&self) -> u64 {
        self.parts.iter().map(|p| p.len() as u64).sum()
    }
}

/// Decoded bundled modules and unpacked prelude chunks that sessions
/// share: the embedder owns it and its memory (see
/// [`Vm::set_module_cache`]). Modules are indexed like
/// [`VmConfig::stdlib`], chunks below [`crate::PRELUDE_CHUNKS`].
pub trait ModuleCache: Send + Sync {
    /// Module `i`, if the cache holds it.
    fn get(&self, i: usize) -> Option<Arc<DecodedModule>>;
    /// A module `i` a session decoded; the cache may keep it or not.
    fn offer(&self, i: usize, module: &Arc<DecodedModule>);
    /// Prelude chunk `c`, if the cache holds it.
    fn prelude_chunk(&self, c: usize) -> Option<Arc<PreludeChunk>>;
    /// A prelude chunk `c` a session unpacked; the cache may keep it or
    /// not.
    fn offer_prelude_chunk(&self, c: usize, chunk: &Arc<PreludeChunk>);
    /// The host-module code compiled under `key`, if the cache holds it.
    fn host_program(&self, key: &[u8; 16]) -> Option<Arc<crate::HostProgram>>;
    /// Fills `out` from `offset` of the host's bundle `region` (0: the
    /// prelude's chunks after their header, 1: the bundled modules' code),
    /// for a build with the `host-bundle` feature. Whether it could.
    #[cfg(feature = "host-bundle")]
    fn bundle(&self, region: u32, offset: u32, out: &mut [u8]) -> bool {
        let _ = (region, offset, out);
        false
    }
    /// Host-module code a session compiled; the cache may keep it or not.
    fn offer_host_program(&self, program: &Arc<crate::HostProgram>);
}

/// VM settings.
#[derive(Clone, Copy, Debug)]
pub struct VmConfig {
    /// Heap settings (memory limit, GC trigger).
    pub heap: HeapConfig,
    /// Python frame depth limit (`RecursionError`).
    pub recursion_limit: u32,
    /// Instruction budget charged at safepoints; `None` is unlimited.
    pub fuel: Option<u64>,
    /// The importable bundled Python modules.
    pub stdlib: &'static [StdModule],
}

impl Default for VmConfig {
    fn default() -> Self {
        VmConfig {
            heap: HeapConfig::default(),
            recursion_limit: 1000,
            fuel: None,
            stdlib: &[],
        }
    }
}

/// Why execution stopped before completing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParkReason {
    /// The fuel budget ran out; `resume` continues with more.
    Fuel,
    /// The park flag ([`Vm::park_flag`]) was set.
    Requested,
    /// The deadline passed (checked against the host clock).
    Deadline,
}

/// How a run stopped.
#[derive(Clone, Debug, PartialEq)]
pub enum Exit {
    /// The module body returned this value.
    Complete(Value),
    /// An uncaught exception (see [`Vm::exception`]).
    Error,
    /// Stopped at a safepoint; [`Vm::resume`] continues.
    Parked(ParkReason),
    /// A host function was called; answer with [`Vm::resume_value`],
    /// [`Vm::resume_error`] or [`Vm::resume_future`].
    HostCall(HostCall),
    /// Every task waits on these pending host futures; answer with
    /// [`Vm::resume_batch`].
    Waiting(Vec<u64>),
}

/// What a cell is stopped at, as the VM holds it ([`Vm::stopped`]): a
/// restore checks it against the stop its image's header names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Stopped {
    /// No cell is active.
    Idle,
    /// At the host call with this id ([`Exit::HostCall`]).
    HostCall(u64),
    /// Every task waits on host futures ([`Exit::Waiting`]).
    Waiting,
    /// At a safepoint ([`Exit::Parked`]).
    Parked,
    /// A pending host call without saved registers: no exit leaves this.
    Inconsistent,
}

/// An output stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Stream {
    /// `print` and `sys.stdout`.
    Stdout,
    /// `sys.stderr`.
    Stderr,
}

/// A source file, for tracebacks.
pub(crate) struct Source {
    pub name: String,
    pub text: String,
    /// The shared host-module code its codes were loaded from
    /// ([`Vm::run_host_defs`]); snapshots refer to it by key.
    pub host: Option<Arc<crate::HostProgram>>,
    /// Between-cell compaction workspace, admitted with the source record.
    pub reclaim_index: u32,
}

/// A loaded code object: the compiler's code plus its materialised
/// constants and names. Immutable during execution; lives outside the arena
/// in the code table, which uniquely owns it during between-cell compaction.
pub(crate) struct CodeObj {
    pub code: Arc<Code>,
    pub consts: Vec<Value>,
    pub names: Vec<Value>,
    pub ic_base: u32,
    pub module_base: u32,
    pub source: u32,
    /// Engine code (the prelude, bundled modules, synthesized functions):
    /// its frames are hidden from tracebacks.
    pub prelude: bool,
    /// Whose globals the code runs in: [`GSLOT_USER`], [`GSLOT_PRELUDE`],
    /// or `GSLOT_STD + i` for bundled module `i`.
    pub gslot: u32,
    /// Between-cell compaction workspace, admitted with the code object.
    /// Not execution state and not serialized.
    pub reclaim_live: bool,
    pub reclaim_index: u32,
}

/// The user's globals (see [`CodeObj::gslot`]).
pub(crate) const GSLOT_USER: u32 = 0;
/// The prelude's globals.
pub(crate) const GSLOT_PRELUDE: u32 = 1;
/// The first bundled module's globals.
pub(crate) const GSLOT_STD: u32 = 2;

/// Header flag of a dict that is some code's globals: changes to its keys
/// invalidate the global inline caches.
pub(crate) const GLOBALS_FLAG: u8 = 32;

impl CodeObj {
    /// The trampoline: code `ci`, one `RETURN`.
    pub(crate) fn trampoline(ci: u32, ic_base: u32) -> CodeObj {
        CodeObj {
            code: Arc::new(Code {
                name: "<continuation>".into(),
                qualname: "<continuation>".into(),
                bytecode: alloc::vec![u32::from(mm_compile::bytecode::op::RETURN)],
                ..Code::default()
            }),
            consts: Vec::new(),
            names: Vec::new(),
            ic_base,
            module_base: ci,
            source: u32::MAX,
            prelude: true,
            gslot: GSLOT_PRELUDE,
            reclaim_live: false,
            reclaim_index: 0,
        }
    }
}

/// Slots below a called frame's registers: `[caller func, link]`.
///
/// The link is a `SmallInt`: `ret_pc:23 | caller code:16 | offset:9`,
/// where offset is the callee base minus the caller base. The caller called
/// from `ret_pc - CALL_LEN` and runs no coroutine. Any other frame (a
/// continuation, a coroutine or generator caller, another call length, a
/// field that does not fit) is special: offset 0, its full [`Frame`] on
/// [`Vm::specials`].
pub(crate) const HDR: usize = 2;

/// The most fuel a retry after `MemoryError` charges for its collection
/// (`Vm::collect_for_retry`): 64 MiB of a limit's worth at a unit per 64
/// bytes, within a slice's overdraft bound (docs/spec.md §5).
pub(crate) const RETRY_CHARGE: u64 = 1 << 20;

/// The length of the call instructions plain frames return to.
pub(crate) const CALL_LEN: usize = mm_compile::bytecode::layout("rbu").words as usize;

const OFF_BITS: u32 = 9;
const CODE_BITS: u32 = 16;
const PC_BITS: u32 = 23;
const LINK_MASK: u64 = (1 << 48) - 1;

/// The link of a plain frame, or `None` if the fields do not fit. `off`
/// is at least [`HDR`].
#[inline]
pub(crate) fn link(ret_pc: usize, ci: u32, off: usize) -> Option<Value> {
    if (ret_pc >> PC_BITS | (ci >> CODE_BITS) as usize | off >> OFF_BITS) != 0 {
        return None;
    }
    let bits = (ret_pc as u64) << (CODE_BITS + OFF_BITS) | u64::from(ci) << OFF_BITS | off as u64;
    Value::int(((bits << 16).cast_signed()) >> 16)
}

/// The link of a special frame.
pub(crate) const SPECIAL_LINK: Value = match Value::int(0) {
    Some(v) => v,
    None => Value::NONE,
};

/// A link's `(ret_pc, caller code, offset)`; offset 0 for a special frame.
/// Meaningless unless `v` is a `SmallInt`.
#[inline]
pub(crate) fn unlink(v: Value) -> (usize, u32, usize) {
    let bits = v.to_bits() & LINK_MASK;
    (
        (bits >> (CODE_BITS + OFF_BITS)) as usize,
        ((bits >> OFF_BITS) & ((1 << CODE_BITS) - 1)) as u32,
        (bits & ((1 << OFF_BITS) - 1)) as usize,
    )
}

/// A caller frame's saved registers: decoded from its callee's header, or
/// held whole on [`Vm::specials`] for special frames.
#[derive(Clone, Copy)]
pub(crate) struct Frame {
    pub code: u32,
    pub func: Value,
    pub base: u32,
    pub call_pc: u32,
    pub ret_pc: u32,
    /// The coroutine or generator object this frame runs, or `None`.
    pub coro: Value,
    /// What to do with the callee's result when it returns into this
    /// frame: 0, or 1 + its index in [`Vm::conts`] (kept out of the frame
    /// so that plain calls push and pop small frames).
    pub cont: u32,
}

/// Inline-cache entries, in per-code side tables outside the arena.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Ic {
    Empty,
    /// A global: entry `idx` of the globals entries buffer at `version`.
    Global {
        version: u64,
        entries: Ref,
        idx: u32,
    },
    /// A builtin, valid while the globals keys are at `version`.
    Builtin {
        version: u64,
        value: Value,
    },
    /// A builtin method of type `tid`.
    Method {
        tid: u8,
        value: Value,
    },
    /// A Python function with simple parameters.
    Call {
        func: Value,
        code: u32,
    },
    /// An instance attribute: classes at `version`, instances of `shape`,
    /// value slot `idx` of the instance.
    Attr {
        version: u64,
        shape: u32,
        idx: u32,
    },
    /// An instance attribute store: from `shape` to `to` (equal when the
    /// attribute exists), value slot `idx`.
    SetAttr {
        version: u64,
        shape: u32,
        to: u32,
        idx: u32,
    },
    /// `cls(...)` with a plain Python `__init__` (code `code`): the class
    /// whose version is `version`. Class versions come from one VM-wide
    /// counter, so the version alone identifies the class, which keeps the
    /// entry (and every `Ic`) at 24 bytes; a 32-byte `Ic` cost fib 3.7%
    /// and loop 1.8% Ir [M].
    New {
        version: u64,
        init: Value,
        code: u32,
    },
    /// A method found on the class (the instance's `shape` lacks the name).
    UMethod {
        version: u64,
        shape: u32,
        func: Value,
    },
}

// Every call site's cache is one `Ic`; the dispatch loop indexes them on
// every global, attribute and call (see `Ic::New`).
const _: () = assert!(size_of::<Ic>() == 24);

/// The registers of the running frame.
#[derive(Clone)]
pub(crate) struct State {
    pub pc: usize,
    pub base: usize,
    pub ci: u32,
    pub acc: Value,
    pub func: Value,
    /// The coroutine or generator the running frame belongs to, or `None`.
    pub coro: Value,
}

/// A task: one stack of frames the scheduler switches between. The running
/// task's stack and frames live in [`Vm::stack`], [`Vm::depth`] and
/// [`Vm::specials`]; a suspended one keeps them here, with its registers in
/// `st`.
/// [`Vm::loop_flags`] bits.
pub(crate) const LOOP_TOP_AWAIT: u8 = 1;
pub(crate) const LOOP_IN_RUN: u8 = 2;

pub(crate) struct Task {
    /// The `TASK` object it settles, or `None` for the cell's main task.
    pub handle: Value,
    pub stack: Vec<Value>,
    pub depth: u32,
    pub specials: Vec<Frame>,
    /// Saved registers; `None` before the task first runs.
    pub st: Option<State>,
    /// The exception being handled (`raise` with no argument), per task.
    pub handled: Value,
    /// The awaitable a blocked task waits on (`None` when runnable).
    pub awaiting: Value,
    /// Its `contextvars` context (a dict, or `None` when empty) while it
    /// is not running.
    pub context: Value,
}

impl Task {
    /// Bytes of stack and frame capacity this task holds while suspended.
    pub(crate) fn buffer_bytes(&self) -> u64 {
        (self.stack.capacity() * size_of::<Value>() + self.specials.capacity() * size_of::<Frame>())
            as u64
    }
}

/// A run-queue entry.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Ready {
    /// Step the task in this slot.
    Task(usize),
    /// A child of this gather settled: check it (gather's done callback).
    Gather(Value),
    /// Call this function with this argument (`Value::UNDEFINED`: none):
    /// a done callback or `loop.call_soon`.
    Call(Value, Value),
}

/// What a builtin asked the dispatch loop to do by returning `UNDEFINED`.
pub(crate) enum Request {
    /// Stop for a host call.
    Host(HostCall),
    /// `asyncio.run(coro)`: run the coroutine in place.
    #[cfg(feature = "asyncio")]
    Run(Value),
    /// Call `func(*args, **kw)` (a generator: `send(args[0])`) and finish
    /// with `cont`, then with `outer`.
    Call {
        func: Value,
        args: crate::cont::Args,
        /// Keyword arguments: a dict, or `None`.
        kw: Value,
        cont: crate::cont::Cont,
        outer: crate::cont::Cont,
    },
    /// Throw `exc` into generator `gn` and finish with `cont`.
    Throw {
        gn: Value,
        exc: Value,
        cont: crate::cont::Cont,
    },
}

/// Work left after a cell's main task stopped, done by prelude code in the
/// same dispatch loop: `str()` of an uncaught user exception, `repr()` and
/// `str()` of a final value that is a user object.
#[derive(Clone, Copy)]
pub(crate) enum Post {
    /// Rendering the uncaught exception.
    Render(Value),
    /// Rendering the completion value.
    Complete(Value),
}

/// A session: heap, frames, globals, code table and limits.
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent mode flags, not a state machine"
)]
pub struct Vm {
    /// The arena.
    pub heap: Heap,
    pub(crate) stack: Vec<Value>,
    /// Frames below the running one (each called frame has a header).
    pub(crate) depth: u32,
    /// The records of special frames (see [`HDR`]), innermost last.
    pub(crate) specials: Vec<Frame>,
    /// The base of the frame that called the running builtin.
    pub(crate) frame_base: usize,
    /// The code table. Shared so that the dispatch loop can borrow it for a
    /// whole run: entries are only appended while code runs (a running
    /// loop's clone stays valid for every index it knows) and compacted
    /// between cells.
    pub(crate) codes: Arc<Vec<Arc<CodeObj>>>,
    pub(crate) code_charge: u64,
    /// `code_charge` after the last reclamation between cells (0 in a new
    /// or restored session, so its first cell counts all code as
    /// reclaimable); see [`Vm::prepare_cell`].
    pub(crate) code_kept: u64,
    /// `code_bytes`' walk of `codes`, kept as codes are appended: the
    /// number of codes counted, their summed bytes and the largest verify
    /// workspace. Reset wherever codes are removed.
    pub(crate) code_sums: (usize, u64, u64),
    pub(crate) ics: Vec<Ic>,
    pub(crate) globals: Value,
    pub(crate) builtins: Value,
    pub(crate) gver: u64,
    /// The interned [`crate::class::Dn`] names, filled on first use; a GC
    /// root, not snapshotted (empty again after a restore).
    pub(crate) dunders: Vec<Value>,
    /// Bumped when a globals dict gains a key; builtin global caches check
    /// `gver + bver`.
    pub(crate) bver: u64,
    pub(crate) types: Vec<Value>,
    pub(crate) handled: Value,
    pub(crate) exc: Value,
    pub(crate) memory_error: Value,
    pub(crate) sources: Vec<Source>,
    pub(crate) output: Option<Output>,
    pub(crate) recursion_limit: u32,
    /// The configured recursion limit: `sys.setrecursionlimit` may lower
    /// the limit but not raise it past this.
    pub(crate) recursion_ceiling: u32,
    pub(crate) fuel: Option<u64>,
    /// Safepoints left before the next fuel, deadline and GC check.
    pub(crate) ticks: i32,
    /// What `ticks` was reset to: the safepoints the next check charges are
    /// `tick_period - ticks`, so fuel parks at the exact safepoint.
    pub(crate) tick_period: i32,
    /// Ticks set aside by [`Vm::request_gc`], which zeroes `ticks` so the
    /// next safepoint check runs the collection: the hot check reads one
    /// counter. Zero unless `gc_pending`.
    pub(crate) tick_spare: i32,
    /// Native fuel charges beyond the fuel left since the last
    /// [`Vm::set_fuel`] (`burn` saturates the budget at zero).
    pub(crate) fuel_overdraft: u64,
    pub(crate) saved: Option<State>,
    /// A collection is due at the next safepoint (set by
    /// [`Vm::request_gc`]).
    pub(crate) gc_pending: bool,
    /// The fuel an int operator wanted when it parked before starting
    /// (`Vm::fuel_short`); 0 otherwise. Cleared when a run continues.
    pub(crate) fuel_wanted: u64,
    /// Set by `resume` after a park before an operation (`fuel_wanted`):
    /// the next operator that would park for fuel runs.
    pub(crate) afford: bool,
    /// A native charged more than 64 units at once since the last
    /// safepoint (`Vm::burn`; cleared by `reset_ticks`).
    pub(crate) long_work: bool,
    /// Fuel units `burn` has charged since the last safepoint check
    /// (cleared by `reset_ticks`). Past 64 the next `CHECK` or back edge
    /// checks, as after one long charge: else a straight-line body of small
    /// native charges reached a check only once per 256 back edges, however
    /// long it was. Not in the image: a restore at a host call starts it
    /// at 0, delaying that one check by at most 64 units.
    pub(crate) burned: u64,
    /// An operator stopped before starting, for the safepoint long native
    /// work since the last one has made due (`Vm::stop_before`).
    pub(crate) check_before: bool,
    /// `BUILD_STRING`'s parts pass a step's work (`Vm::reserve_build`):
    /// `execute` joins them a step at a time.
    pub(crate) long_build: bool,
    /// Text comparisons inside a comparison walk past its visit budget
    /// (`spend_text`), charged with the walk's visits by `walked_of`.
    pub(crate) text_over: core::cell::Cell<u64>,
    /// Every fuel unit `burn` has charged, fuel limit or not (wrapping): a
    /// chunked native's step stops after [`crate::nstep::STEP_WORK`] of it.
    pub(crate) work: u64,
    /// Set by the dispatch loop's `CALL` just before it calls a builtin,
    /// taken by [`Vm::call_native`]: only such a call may park before it
    /// starts (re-running the instruction), not a builtin a native calls.
    pub(crate) direct_call: bool,
    /// Tasks of the running cell; slot 0 is the main task.
    pub(crate) tasks: Vec<Option<Task>>,
    /// Every task slot below this index is taken (a search start, not state:
    /// 0 is always valid).
    pub(crate) task_hint: usize,
    /// Buffer capacity held by suspended tasks, kept in step with every
    /// move into or out of a task so charging never walks the task table.
    pub(crate) parked_bytes: u64,
    /// The running task's slot.
    pub(crate) cur: usize,
    /// The run queue: tasks to step and gathers to check, in the order
    /// asyncio's `call_soon` would run them.
    pub(crate) ready: VecDeque<Ready>,
    pub(crate) blocked: Vec<usize>,
    /// Unsettled gathers, in creation order (children before parents).
    pub(crate) gathers: Vec<Value>,
    /// Entries at the front of `ready` that belong to the loop iteration
    /// running now (CPython's `_run_once` runs those ready when it starts).
    pub(crate) iter_left: usize,
    /// An iteration is running (it ends when `iter_left` reaches 0).
    pub(crate) iter_open: bool,
    /// Iteration ends until the main task's stop future settles
    /// (`_aio_stop`); 0 when it waits for none.
    pub(crate) stop_wait: u32,
    /// Done callbacks registered on pending awaitables: run-queue slots
    /// reserved for them.
    pub(crate) cb_slots: usize,
    /// Tasks named so far (`Task-N`), across cells as CPython's counter.
    pub(crate) task_counter: u64,
    /// Names the program gave tasks: a dict from task to name, or `None`.
    pub(crate) task_names: Value,
    /// The running task's `contextvars` context: a dict from `ContextVar`
    /// to value, or `None` while empty. The session's own outside tasks.
    pub(crate) context: Value,
    /// A `KeyboardInterrupt` or `SystemExit` a task raised: it leaves
    /// `asyncio.run` once the run has shut down, as CPython's escapes the
    /// loop (`None` otherwise).
    pub(crate) escape: Value,
    /// Failed tasks and futures whose exception may never be retrieved
    /// ([`crate::aio::aw::LOG_TB`]), for the end of `asyncio.run`.
    pub(crate) unretrieved: Vec<Value>,
    /// A loop runs for `get_running_loop()`: [`LOOP_TOP_AWAIT`] (the cell's
    /// top level awaits) and [`LOOP_IN_RUN`] (inside `asyncio.run`).
    pub(crate) loop_flags: u8,
    /// The cell's host futures by call id (settled ones stay until the
    /// cell ends, to tell "already settled" from "unknown"). Sorted by id.
    pub(crate) futures: Vec<(u64, Value)>,
    /// Settle counter: orders failures for `gather`.
    pub(crate) seq: u64,
    /// Virtual time for `asyncio.sleep` (the engine has no clock).
    pub(crate) vclock: f64,
    pub(crate) next_call: u64,
    /// The pending host call: its id and the pc of the calling instruction.
    pub(crate) host_call: Option<(u64, u32)>,
    pub(crate) request: Option<Request>,
    /// Imported modules by name (cleared by `reset`).
    pub(crate) modules: Value,
    pub(crate) host_modules: Vec<Arc<HostModule>>,
    /// Host function call names by id (`Vm::host_id`); never shrinks.
    pub(crate) host_ids: Vec<String>,
    pub(crate) park: Arc<AtomicBool>,
    pub(crate) clock: Option<Clock>,
    pub(crate) deadline: Option<u64>,
    /// The wall clock `time.time()` and `datetime.now()` read.
    pub(crate) wall_clock: Option<WallClock>,
    /// Where `os.urandom`, `random`'s seeding and `uuid4` get bytes.
    pub(crate) entropy: Option<Entropy>,
    /// The local time zone `time` and `datetime` read, as TZif data
    /// (RFC 8536); `None` is UTC.
    pub(crate) local_zone: Option<Vec<u8>>,
    /// Where `zoneinfo` gets a zone's TZif data (not session state).
    pub(crate) zone_source: Option<ZoneSource>,
    pub(crate) host_args: crate::host::HostArgs,
    /// A helper failed because the operation needs Python code (a dunder,
    /// a generator): the caller hands off to the slow path.
    pub(crate) slow: bool,
    /// Instance shapes (hidden classes), by id.
    pub(crate) shapes: Vec<crate::class::Shape>,
    /// The next class version tag.
    pub(crate) next_version: u64,
    /// The prelude's globals.
    pub(crate) prelude_globals: Value,
    /// The one-instruction (`RETURN`) code that composes continuations.
    pub(crate) tramp: u32,
    /// The sentinel prelude twins receive for absent optional arguments.
    pub(crate) missing: Value,
    /// The `()` singleton (`() is ()`, as in CPython).
    pub(crate) empty_tuple: Value,
    /// Post-mortem work of the main task (see [`Post`]).
    pub(crate) post: Option<Post>,
    /// Arena-owned `(repr, str)` of the completion value; traced until taken.
    pub(crate) completion_text: Option<(Value, Value)>,
    /// Whether a completed cell's value needs its text (user `__repr__`/`__str__` run in the prelude before the cell ends).
    pub(crate) want_text: bool,
    /// Native `repr`/`str` render oversized ints as `<int with N bits>`
    /// (see [`Vm::set_int_bits_text`]).
    pub(crate) int_bits_text: bool,
    /// Set while text is rendered for the host outside a cell, where
    /// `repr` cannot continue in the prelude.
    pub(crate) host_text: bool,
    /// While the host renders an uncaught exception
    /// ([`Vm::begin_report`]): the text length past which rendering
    /// stops (`usize::MAX`: no report), whether it stopped (at that length
    /// or at the deadline), and how many exceptions it has drawn.
    pub(crate) report_cap: usize,
    pub(crate) report_cut: bool,
    pub(crate) report_steps: u32,
    /// Set once guest code has built a `NameError` itself: only then does
    /// `raise` look for one to note (see `note_raise`). Not in the image;
    /// a restored session starts with it set.
    pub(crate) name_error_built: bool,
    /// Collect at every safepoint (see [`Vm::set_gc_stress`]).
    pub(crate) gc_stress: bool,
    /// The collection count that ends the collection `gc.collect` or a
    /// retry after `MemoryError` waits for ([`Vm::oom_retry`]); 0: none. Not in the
    /// image: a snapshot collects in full, so a restored wait is over.
    pub(crate) gc_wait: u64,
    /// A safepoint parked ahead of a collection step, which runs when the
    /// session resumes (`Vm::safepoint`). Not in the image: a snapshot
    /// collects in full.
    pub(crate) gc_owed: bool,
    /// No collection starts by itself (`SessionSettings::gc_off`,
    /// measuring). Not in the image.
    pub gc_off: bool,
    /// Under a fuel limit, a safepoint that ran a collection step parks
    /// with `ParkReason::Fuel` and fuel left (`Session::set_gc_yield`).
    /// Not in the image.
    pub gc_yield: bool,
    /// The faulting `CALL` runs again once that collection ends.
    pub(crate) oom_wait: bool,
    /// The instruction that may not run again after `MemoryError`
    /// ([`Vm::op_stamp`] when it was marked; `u64::MAX`: none): the rerun
    /// itself, so a native that allocates before failing does not retry
    /// for ever, or one that drew items from a one-shot iterator. A mark
    /// lasts until the next safepoint, which a host's fuel slice cannot
    /// move. Not in the image.
    pub(crate) oom_rerun: u64,
    /// The collection count a chunked step's deferred allocation waits for
    /// ([`Vm::oom_defer`]), until that allocation succeeds; 0: none. Not in
    /// the image: a snapshot collects in full, so a restored step at most
    /// defers once more.
    pub(crate) oom_defer_at: u64,
    /// The [`Vm::op_stamp`] `consumed` belongs to: entries from an earlier
    /// instruction are dropped, not read. Not in the image.
    pub(crate) consumed_at: u64,
    /// Bumped whenever ticks reset (every safepoint check): with
    /// `ticks_used`, it tells instructions apart ([`Vm::op_stamp`]). Not
    /// in the image.
    pub(crate) tick_epoch: u32,
    /// An incremental collection step at every safepoint, in tiny slices
    /// (see [`Vm::set_gc_inc_stress`]).
    pub(crate) gc_inc_stress: bool,
    /// Iterators the running builtin consumed (`collect`) and their items:
    /// its prelude twin gets the items instead of the spent iterator.
    pub(crate) consumed: Vec<(Value, crate::scratch::NativeValues)>,
    /// The iterable `collect` stopped gathering after a chunk or at an
    /// item that needs Python code, its iterator and the items it took:
    /// the prelude goes on from there (`Vm::slow_call`, `_collect_rest`).
    pub(crate) partial: Option<(Value, Value, crate::scratch::NativeValues)>,
    /// The continuations of live frames (see [`Frame::cont`]) and the free
    /// slots among them.
    pub(crate) conts: Vec<Option<crate::cont::Cont>>,
    pub(crate) cont_free: Vec<u32>,
    /// The constructor native of each builtin type, filled lazily
    /// (`CTOR_UNKNOWN`: not looked up yet, `CTOR_NONE`: none).
    pub(crate) ctors: Vec<u32>,
    /// A reusable buffer for building strings (see [`Vm::build_str`]).
    pub(crate) sbuf: StrBuf,
    /// The importable bundled Python modules.
    pub(crate) stdlib: &'static [StdModule],
    /// Per bundled module: the code-table range its program was loaded
    /// into (once per session, on first import) and its body's index.
    pub(crate) std_codes: Vec<Option<(u32, u32, u32)>>,
    /// Per bundled module: its globals, or `None` while not imported
    /// (since the last reset).
    pub(crate) std_globals: Vec<Value>,
    /// Maximum decoder workspace across loaded modules, reserved for restore.
    pub(crate) std_scratch: u64,
    /// Where decoded bundled modules are shared, if anywhere. Not in the
    /// image.
    pub(crate) module_cache: Option<Arc<dyn ModuleCache>>,
    /// Snapshots store host-module code instead of referring to it by key
    /// ([`Vm::set_host_code_in_snapshots`]); not saved.
    pub(crate) host_code_in_snapshots: bool,
    /// The bundled modules' section decoders, built by the cell's first
    /// decode and dropped when the cell ends. Not in the image.
    pub(crate) unpackers: Unpackers,
    /// Signature-code indices sorted by positional count and parameter names.
    /// Keys live in the code itself (see `_mm.signature`).
    pub(crate) synth: Vec<u32>,
    /// The host calls of the current (or last) cell, by id
    /// ([`Vm::call_log`]).
    pub(crate) call_log: Vec<crate::host::CallEntry>,
    /// Coroutines the cell created, until they start (pruned as they
    /// grow): the end of the cell warns about those never awaited.
    pub(crate) fresh: Vec<Value>,
    /// Every bundled module name, enabled in this build or not
    /// ([`Vm::set_known_modules`]).
    pub(crate) known_modules: &'static [&'static str],
}

/// The output callback: every `print` and stream write, in order.
pub type Output = Box<dyn FnMut(Stream, &str) + Send>;

/// A monotonic host clock, in any unit the deadline uses.
pub type Clock = Box<dyn Fn() -> u64 + Send>;

/// The host's wall clock: seconds since the Unix epoch (UTC), as
/// `time.time()` returns them.
pub type WallClock = Box<dyn Fn() -> f64 + Send>;

/// The host's time zone data: a zone key's TZif bytes (RFC 8536), or
/// `None` when it has no such zone.
pub type ZoneSource = Box<dyn FnMut(&str) -> Option<Vec<u8>> + Send>;

/// The host's entropy source: fills the buffer with random bytes.
pub type Entropy = Box<dyn FnMut(&mut [u8]) + Send>;

/// Safepoints between checks of fuel and the GC trigger.
pub(crate) const TICKS: i32 = 256;

pub(crate) const CTOR_UNKNOWN: u32 = u32::MAX;
const CTOR_NONE: u32 = u32::MAX - 1;

/// The decimal text of `i`, written into `buf`.
pub(crate) fn i64_text(i: i64, buf: &mut [u8; 20]) -> &str {
    let mut n = i.unsigned_abs();
    let mut k = buf.len();
    loop {
        k -= 1;
        #[allow(clippy::cast_possible_truncation, reason = "a digit")]
        {
            buf[k] = b'0' + (n % 10) as u8;
        }
        n /= 10;
        if n == 0 {
            break;
        }
    }
    if i < 0 {
        k -= 1;
        buf[k] = b'-';
    }
    core::str::from_utf8(&buf[k..]).unwrap_or("")
}

/// Exception slots.
pub(crate) mod exc_slot {
    pub const TYPE: u32 = 0;
    pub const ARGS: u32 = 1;
    pub const CAUSE: u32 = 2;
    pub const CONTEXT: u32 = 3;
    pub const TRACEBACK: u32 = 4;
    /// The attribute dict (`None` until an attribute is set).
    pub const DICT: u32 = 6;
    /// `str()` of an exception whose class defines `__str__`, made by the
    /// prelude when it is reported uncaught.
    pub const STR: u32 = 7;
    pub const FLAGS: u32 = 8;
}

/// Function slots.
pub(crate) mod fn_slot {
    pub const NAME: u32 = 0;
    pub const QUALNAME: u32 = 1;
    pub const DEFAULTS: u32 = 2;
    pub const KWDEFAULTS: u32 = 3;
    pub const CLOSURE: u32 = 4;
    pub const GLOBALS: u32 = 5;
    /// Attributes set on the function (`__wrapped__`, `__doc__`, ...): a
    /// dict, or `None`.
    pub const DICT: u32 = 6;
    pub const CODE: u32 = 7;
}

impl Vm {
    /// A fresh session with builtins installed. Not inlined, so it keeps
    /// this crate's opt-level inside the size-optimized facade.
    #[inline(never)]
    pub fn new(config: VmConfig) -> Result<Vm, Raised> {
        let mut heap = Heap::new(config.heap);
        // Refuse before allocating even the initial tables and register stack.
        heap.set_external_bytes(Self::fixed_bytes(config.stdlib.len()) + 256 * 8)
            .map_err(|_| Raised)?;
        let mut vm = Vm {
            heap,
            stack: vec![Value::UNDEFINED; 256],
            depth: 0,
            specials: Vec::new(),
            frame_base: 0,
            codes: Arc::new(Vec::new()),
            code_charge: Self::fixed_bytes(config.stdlib.len()),
            code_kept: 0,
            code_sums: (0, 0, 0),
            ics: Vec::new(),
            globals: Value::NONE,
            builtins: Value::NONE,
            gver: 0,
            dunders: Vec::new(),
            bver: 0,
            types: Vec::with_capacity(NTYPES),
            handled: Value::NONE,
            exc: Value::NONE,
            memory_error: Value::NONE,
            sources: Vec::new(),
            output: None,
            recursion_limit: config.recursion_limit,
            recursion_ceiling: config.recursion_limit,
            fuel: config.fuel,
            ticks: TICKS,
            tick_period: TICKS,
            tick_spare: 0,
            fuel_overdraft: 0,
            saved: None,
            gc_pending: false,
            fuel_wanted: 0,
            afford: false,
            long_work: false,
            burned: 0,
            check_before: false,
            long_build: false,
            text_over: core::cell::Cell::new(0),
            work: 0,
            direct_call: false,
            tasks: Vec::new(),
            task_hint: 0,
            parked_bytes: 0,
            cur: 0,
            ready: VecDeque::new(),
            blocked: Vec::new(),
            gathers: Vec::new(),
            iter_left: 0,
            iter_open: false,
            stop_wait: 0,
            cb_slots: 0,
            task_counter: 0,
            task_names: Value::NONE,
            context: Value::NONE,
            escape: Value::NONE,
            unretrieved: Vec::new(),
            loop_flags: 0,
            futures: Vec::new(),
            seq: 0,
            vclock: 0.0,
            next_call: 1,
            host_call: None,
            request: None,
            modules: Value::NONE,
            host_modules: Vec::new(),
            host_ids: Vec::new(),
            park: Arc::new(AtomicBool::new(false)),
            clock: None,
            deadline: None,
            wall_clock: None,
            entropy: None,
            local_zone: None,
            zone_source: None,
            host_args: crate::host::HostArgs::default(),
            slow: false,
            shapes: Vec::new(),
            next_version: 1,
            prelude_globals: Value::NONE,
            tramp: 0,
            missing: Value::NONE,
            empty_tuple: Value::NONE,
            post: None,
            completion_text: None,
            want_text: false,
            int_bits_text: false,
            host_text: false,
            report_cap: usize::MAX,
            report_cut: false,
            report_steps: 0,
            name_error_built: false,
            gc_stress: false,
            gc_wait: 0,
            gc_owed: false,
            gc_off: false,
            gc_yield: false,
            oom_wait: false,
            oom_rerun: u64::MAX,
            oom_defer_at: 0,
            consumed_at: u64::MAX,
            tick_epoch: 0,
            gc_inc_stress: false,
            consumed: Vec::new(),
            partial: None,
            conts: Vec::new(),
            cont_free: Vec::new(),
            ctors: vec![CTOR_UNKNOWN; NTYPES],
            sbuf: StrBuf::new(),
            stdlib: config.stdlib,
            std_codes: vec![None; config.stdlib.len()],
            std_globals: vec![Value::NONE; config.stdlib.len()],
            std_scratch: 0,
            module_cache: None,
            host_code_in_snapshots: false,
            unpackers: None,
            synth: Vec::new(),
            call_log: Vec::new(),
            fresh: Vec::new(),
            known_modules: &[],
        };
        vm.init()?;
        vm.reserve_code(0)?;
        vm.reset_ticks();
        Ok(vm)
    }

    fn init(&mut self) -> Result<(), Raised> {
        // The builtins and the prelude intern about 720 strings [M]; room
        // for them up front saves five rehashes of the table.
        let room = self.heap.reserve_interned(1024);
        self.hx(room)?;
        for t in crate::types::ALL {
            let name = self.intern(t.name())?;
            let r = {
                let res = self.heap.alloc_init(ty::TYPE, 0, &[name], &[t as u64]);
                self.hx(res)
            }?;
            self.types.push(Value::from_ref(r, ty::TYPE as u8));
        }
        self.builtins = self.new_dict()?;
        crate::builtins::install(self)?;
        self.fresh_globals()?;
        self.shapes.push(crate::class::Shape::default());
        self.tramp = self.add_trampoline();
        let obj = self.type_obj(T::object);
        self.missing = self.new_instance(obj, 0)?;
        self.empty_tuple = self.alloc_tuple(&[])?;
        self.prelude_globals = self.new_globals_dict()?;
        self.init_prelude()?;
        let args = self.alloc_tuple(&[])?;
        let t = self.types[T::MemoryError as usize];
        self.memory_error = self.alloc_exc(t, args)?;
        Ok(())
    }

    /// New globals (`__name__` = `'__main__'`) and an empty module table.
    fn fresh_globals(&mut self) -> Result<(), Raised> {
        self.globals = self.new_globals_dict()?;
        self.modules = self.new_dict()?;
        self.std_globals.fill(Value::NONE);
        let n = self.intern("__name__")?;
        let m = self.intern("__main__")?;
        let g = self.globals;
        self.dict_set(g, n, m)?;
        self.gver = self.gver.wrapping_add(1);
        self.ics.fill(Ic::Empty);
        Ok(())
    }

    /// A new globals dict (indexed, flagged [`GLOBALS_FLAG`]).
    pub(crate) fn new_globals_dict(&mut self) -> Result<Value, Raised> {
        let d = self.new_dict_indexed(8)?;
        if let Some(r) = d.as_obj() {
            let _ = self.heap.set_flags(r, GLOBALS_FLAG);
        }
        Ok(d)
    }

    /// The globals code `c` runs in.
    #[inline]
    pub(crate) fn globals_of(&self, c: &CodeObj) -> Value {
        match c.gslot {
            GSLOT_USER => self.globals,
            GSLOT_PRELUDE => self.prelude_globals,
            k => self
                .std_globals
                .get((k - GSLOT_STD) as usize)
                .copied()
                .unwrap_or(self.globals),
        }
    }

    /// Sets, or with `None` removes, the main module's `__name__` and
    /// `__file__` globals. A new or reset session has `__name__ ==
    /// '__main__'` and no `__file__`; without `__name__`, code sees the
    /// builtins module's (`'builtins'`), as `exec(code, {})` does in
    /// CPython, so its functions and classes have no module prefix.
    pub fn set_module_dunders(
        &mut self,
        name: Option<&str>,
        file: Option<&str>,
    ) -> Result<(), Raised> {
        let g = self.globals;
        for (key, v) in [("__name__", name), ("__file__", file)] {
            let k = self.intern(key)?;
            match v {
                Some(s) => {
                    let v = self.new_str(s)?;
                    self.dict_set(g, k, v)?;
                }
                None => {
                    self.dict_del(g, k)?;
                }
            }
        }
        self.gver = self.gver.wrapping_add(1);
        self.ics.fill(Ic::Empty);
        Ok(())
    }

    /// Ends any running cell and clears globals and imported modules.
    /// Unreachable user code is reclaimed; host functions and modules stay.
    pub fn reset(&mut self) -> Result<(), Raised> {
        self.end_cell();
        self.handled = Value::NONE;
        self.context = Value::NONE;
        self.exc = Value::NONE;
        self.post = None;
        self.fresh_globals()?;
        self.reclaim_code();
        Ok(())
    }

    /// Sets the callback that receives `print` and stream output.
    pub fn set_output(&mut self, out: Output) {
        self.output = Some(out);
    }

    /// Changes the fuel budget (`None` = unlimited). One unit is one
    /// safepoint (a call or a backward jump), plus what natives charge.
    pub fn set_fuel(&mut self, fuel: Option<u64>) {
        self.fuel = fuel;
        self.fuel_overdraft = 0;
        self.reset_ticks();
    }

    /// The recursion depth the session was created with (what
    /// `sys.setrecursionlimit` cannot raise).
    #[must_use]
    pub fn recursion_ceiling(&self) -> u32 {
        self.recursion_ceiling
    }

    /// Exact fuel accounting since the last [`Vm::set_fuel`]: the fuel
    /// left counting the safepoints run since the last check, and what
    /// natives charged beyond zero.
    #[must_use]
    pub fn fuel_account(&self) -> (Option<u64>, u64) {
        let used = self.ticks_used();
        match self.fuel {
            Some(f) => (
                Some(f.saturating_sub(used)),
                self.fuel_overdraft.saturating_add(used.saturating_sub(f)),
            ),
            None => (None, self.fuel_overdraft),
        }
    }

    /// Tells instructions apart for marks that must not outlive the one
    /// that set them (`oom_rerun`, `consumed`): equal at two points exactly
    /// when no safepoint tick (a call or a backward jump) ran between them.
    /// Natives' charges keep `ticks_used` (they shorten the period by the
    /// ticks they skip); a check at a tick starts a new epoch, and a check
    /// between ticks (an operator stopping before it starts, a host
    /// resuming) carries live marks into it (`Vm::reset_ticks`), so where
    /// a host's fuel slices end does not move them, and no hot path pays.
    pub(crate) fn op_stamp(&self) -> u64 {
        (u64::from(self.tick_epoch) << 32) | (self.ticks_used() & 0xFFFF_FFFF)
    }

    /// Drops what an earlier instruction's builtin consumed.
    pub(crate) fn fresh_consumed(&mut self) {
        let now = self.op_stamp();
        if self.consumed_at != now {
            self.consumed.clear();
            self.consumed_at = now;
        }
    }

    /// The safepoints run since the last check.
    fn ticks_used(&self) -> u64 {
        let left = (self.ticks + self.tick_spare).max(0);
        u64::from((self.tick_period - left).unsigned_abs())
    }

    /// Charges the safepoints run since the last check and starts a new
    /// period. Those run after natives spent the fuel (a check comes only
    /// when the period ends) are overdraft, as the natives' excess is.
    pub(crate) fn charge_ticks(&mut self) {
        let used = self.ticks_used();
        if let Some(f) = self.fuel {
            self.fuel = Some(f.saturating_sub(used));
            self.fuel_overdraft = self.fuel_overdraft.saturating_add(used.saturating_sub(f));
        }
        self.reset_ticks();
    }

    /// After a [`ParkReason::Fuel`] park: the fuel the next instruction, an
    /// int operator, will charge if it is more than was left, so it parked
    /// before starting; 0 for an ordinary fuel park. [`Vm::resume`] runs
    /// that instruction whatever the fuel then (charging the excess as
    /// overdraft); a host enforcing a budget stops instead of resuming.
    #[must_use]
    pub fn fuel_wanted(&self) -> u64 {
        self.fuel_wanted
    }

    /// Fuel left (`None` = unlimited), as of the last safepoint check.
    #[must_use]
    pub fn fuel(&self) -> Option<u64> {
        self.fuel
    }

    /// What the cell is stopped at: a park and a host call save the
    /// registers (a host call also its id); a wait saves none, with the
    /// cell's tasks still held.
    #[must_use]
    pub fn stopped(&self) -> Stopped {
        match (&self.saved, self.host_call) {
            (Some(_), Some((id, _))) => Stopped::HostCall(id),
            (Some(_), None) => Stopped::Parked,
            (None, None) if self.tasks.is_empty() => Stopped::Idle,
            (None, None) => Stopped::Waiting,
            (None, Some(_)) => Stopped::Inconsistent,
        }
    }

    /// Starts a new period of safepoints before the next check: `TICKS`,
    /// or the fuel left if that is less.
    pub(crate) fn reset_ticks(&mut self) {
        let was = self.op_stamp();
        self.tick_epoch = self.tick_epoch.wrapping_add(1);
        let p = match self.fuel {
            _ if self.gc_stress || self.gc_inc_stress => 1,
            Some(f) => i32::try_from(f.min(TICKS.unsigned_abs().into())).unwrap_or(TICKS),
            None => TICKS,
        };
        self.tick_period = p.max(1);
        self.ticks = self.tick_period;
        self.tick_spare = 0;
        self.long_work = false;
        self.burned = 0;
        if self.gc_pending {
            self.tick_spare = self.ticks;
            self.ticks = 0;
        }
        // Marks of the instruction running now stay its own.
        let now = self.op_stamp();
        if self.oom_rerun == was {
            self.oom_rerun = now;
        }
        if self.consumed_at == was {
            self.consumed_at = now;
        }
    }

    /// Asks for a collection at the next safepoint.
    pub(crate) fn request_gc(&mut self) {
        if !self.gc_pending {
            self.gc_pending = true;
            self.tick_spare = self.ticks;
            self.ticks = 0;
        }
    }

    /// Asks for a collection that starts after this call, run in steps at
    /// the coming safepoints (incremental over 32 MiB live, else in full
    /// at the first); [`Vm::gc_waiting`] says when it has ended. A retried
    /// `CALL` that runs again after a park keeps the wait it began
    /// (`fresh` false), so parks cannot push its end back; `gc.collect`
    /// always waits for one that starts after it (`fresh`).
    pub(crate) fn collect_in_steps(&mut self, fresh: bool) {
        let n = self.heap.stats().collections;
        if fresh || self.gc_wait <= n {
            self.gc_wait = n + 1 + u64::from(self.heap.marking());
            self.heap.force_collection();
        }
        self.request_gc();
    }

    /// Whether the collection [`Vm::collect_in_steps`] asked for is still
    /// running; if so, the next safepoint runs its next step.
    pub(crate) fn gc_waiting(&mut self) -> bool {
        if !self.gc_held() {
            self.gc_wait = 0;
            return false;
        }
        self.request_gc();
        true
    }

    /// Whether the guest waits for the collection [`Vm::collect_in_steps`]
    /// asked for: the safepoint runs its steps with no guest code between
    /// them, as for a held mark, so the steps a collection takes (which
    /// depend on parks, snapshots, `gc_yield` and the target) charge no
    /// fuel. A full collection that ran meanwhile (a snapshot's, or one
    /// between cells) leaves nothing to step: it ended the wait.
    pub(crate) fn gc_held(&self) -> bool {
        self.gc_wait != 0
            && self.heap.stats().collections < self.gc_wait
            && (self.heap.marking() || self.heap.sweeping() || self.heap.should_collect())
    }

    /// Builtin `n`, called straight from a `CALL`, raised `MemoryError`:
    /// garbage not yet collected counts against the limit, so if `n` is a
    /// method of an immutable builtin type (it changed nothing before
    /// failing, so it can run again), it consumed no iterator, at least
    /// 1 MiB was allocated since the last collection, and this is not the
    /// `CALL`'s rerun, the `CALL` runs again once a collection has ended, a
    /// step per safepoint. Returns whether it will.
    #[cold]
    #[inline(never)]
    pub(crate) fn oom_retry(&mut self, n: &crate::builtins::Native) -> bool {
        self.oom_retry_if(Self::native_pure(n))
    }

    /// Whether builtin `n` changes nothing before its result is allocated,
    /// so it may run again after a collection ([`Vm::oom_retry`]).
    fn native_pure(n: &crate::builtins::Native) -> bool {
        match n.owner {
            Some(T::str | T::bytes | T::int | T::float | T::tuple | T::frozenset) => true,
            // Methods of mutable types that only read: they build their
            // result before anything changes.
            Some(T::list | T::dict | T::set) if n.name() == "copy" => true,
            Some(T::dict) => n.name() == "fromkeys",
            Some(T::set) => matches!(
                n.name(),
                "union" | "intersection" | "difference" | "symmetric_difference"
            ),
            // The prelude twins' inserts of a key known absent, with its
            // hash (a comprehension or `set(xs)` over user `__hash__`
            // keys): a table grows before the entry is written, or the
            // entry is taken out again (`set_unput`), and nothing compares.
            None if n.module() == Some("_prelude") => {
                matches!(n.name(), "_set_insert" | "_dict_insert")
            }
            // The constructors that copy their argument.
            None => {
                n.module().is_none()
                    && matches!(n.name(), "list" | "tuple" | "set" | "frozenset" | "dict")
            }
            _ => false,
        }
    }

    /// An instruction the dispatch loop ran natively raised `MemoryError`
    /// straight from its native (a binary operator, `Vm::op_binop`;
    /// `SET_ITEM`, `Vm::op_setitem`; `BUILD_STRING`, `LIST_TO_TUPLE`,
    /// `UNPACK_EX`, `LIST_EXTEND` and `CALL_EX`, [`Vm::oom_retry_fault`]):
    /// as [`Vm::oom_retry`], it runs again
    /// once a collection has ended, unless it may have changed its left
    /// operand `a` before failing (`+=` and `|=` on a list, dict or set;
    /// `Value::NONE` for the others). A bytearray grows into a new block
    /// before it changes, so `ba += x` runs again: its old growth blocks
    /// were garbage a collection frees. User code never ran inside the
    /// failed native: a dunder runs as a request, not as an error. The
    /// hooks sit outside the dispatch loop's arms, which stay unchanged.
    #[cold]
    #[inline(never)]
    pub(crate) fn oom_retry_op(&mut self, a: Value, inplace: bool) {
        // A native handing the operation to its prelude twin (`slow`)
        // raised nothing: `exc` is stale.
        if self.slow {
            return;
        }
        // A builtin-type subclass's instance changes its backing object.
        let a = self.unbx(a);
        // A list's `+=` and `*=` build or grow its buffer before they
        // change it; a dict's or set's `|=` adds an item at a time.
        let changed = inplace && (a.is_obj_kind(ty::DICT as u8) || a.is_obj_kind(ty::SET as u8));
        self.oom_retry_if(!changed);
    }

    /// A fault the dispatch loop reports where the heap's `MemoryError`
    /// (`memory_error`) came from an instruction whose native changed
    /// nothing before its block did not fit, and no user code ran:
    /// [`Vm::oom_retry_op`] for it. `BUILD_STRING` (an f-string's join of
    /// its pieces, all str already); `LIST_TO_TUPLE` (a starred tuple
    /// display's tuple of its list); `UNPACK_EX` from a list or tuple (a
    /// starred target's list, written to its registers only once it
    /// exists); `LIST_EXTEND` from a list or tuple (the list grows once,
    /// to its final length, before any item is written); and `CALL_EX` of a
    /// Python function (its `*args` tuple and bound arguments are built
    /// before its frame is entered, so the function has not started). An
    /// iterator either may have drained is not retried, nor a call of
    /// anything else, which may have run code or changed its arguments.
    /// Read from the faulting frame, where `st.pc` and `st.acc` are still
    /// the instruction's, so the dispatch arms stay unchanged.
    #[cold]
    #[inline(never)]
    pub(crate) fn oom_retry_fault(&mut self, st: &State) -> bool {
        use mm_compile::bytecode::{op, operand};
        let seq = |v: Value| v.is_obj_kind(ty::LIST as u8) || v.is_obj_kind(ty::TUPLE as u8);
        let Some(bc) = self
            .codes
            .get(st.ci as usize)
            .filter(|_| st.ci != self.tramp)
            .map(|c| &c.code.bytecode)
        else {
            return false;
        };
        let rerun = match bc.get(st.pc).copied().map(crate::interp::op_of) {
            // A list grows into a new buffer before it changes
            // (`Vm::list_grow`).
            Some(op::BUILD_STRING | op::LIST_TO_TUPLE | op::LIST_APPEND) => true,
            Some(op::UNPACK_EX | op::LIST_EXTEND) => seq(st.acc),
            Some(op::CALL_EX) => self
                .stack
                .get(st.base + operand(bc, st.pc, 0))
                .is_some_and(|f| f.is_obj_kind(ty::FUNC as u8)),
            // A builtin type called as a constructor that copies its
            // argument (`list(l)`); a builtin called directly retries in
            // `call_native`.
            Some(op::CALL) => self
                .stack
                .get(st.base + operand(bc, st.pc, 0))
                .and_then(|&f| self.type_of_typeobj(f))
                .and_then(|t| self.ctor_of(t))
                .and_then(crate::native)
                .is_some_and(Self::native_pure),
            // A subscript read of a builtin container builds only its
            // result (a slice); a user `__getitem__` runs as a request.
            Some(op::GET_ITEM) => self
                .stack
                .get(st.base + operand(bc, st.pc, 0))
                .is_some_and(|&o| self.user_class(o).is_none() && !Self::is_class(o)),
            _ => false,
        };
        if !rerun {
            return false;
        }
        // The caller waits for the collection itself.
        let retry = self.oom_retry_if(true);
        self.oom_wait = false;
        retry
    }

    /// A chunked step's new table did not fit (`MemoryError` pending) where
    /// the step can leave it for its next step. Garbage may hold the room
    /// (a value dropped during an incremental mark survives it, and
    /// dropping one allocates nothing), so the error is dropped, the guest
    /// waits for a full collection at the safepoint before that step (held,
    /// as for a retry: no guest code or fuel between its steps), and the
    /// next step allocates again; true. A failure again before that
    /// allocation succeeded (the step clears `oom_defer_at`) stands; false.
    /// Not a count of bytes allocated since the collection: a table's
    /// first half that fit, dropped when its second did not, counted as
    /// progress, and a step at the limit collected for ever.
    #[cold]
    #[inline(never)]
    pub(crate) fn oom_defer(&mut self) -> bool {
        if self.exc_type(self.exc) != T::MemoryError || !self.room_may_help() {
            return false;
        }
        let retried = self.oom_defer_at != 0 && self.heap.stats().collections >= self.oom_defer_at;
        if retried {
            self.oom_defer_at = 0;
            return false;
        }
        self.exc = Value::NONE;
        self.collect_for_retry();
        self.oom_defer_at = self.gc_wait;
        true
    }

    /// Whether the `MemoryError` pending came from the heap refusing a
    /// request no larger than the memory limit, which a collection may make
    /// room for: one larger, or a size cap refused before the heap was
    /// asked (`list(range(10**9))`), raises at once, without a collection
    /// or its charge.
    fn room_may_help(&self) -> bool {
        let refused = self.heap.take_refused();
        refused != 0 && refused <= self.heap.memory_limit()
    }

    /// Whether a step deferred its new table ([`Vm::oom_defer`]) and the
    /// collection it waits for has not ended: the step ends.
    pub(crate) fn oom_deferred(&self) -> bool {
        self.oom_defer_at > self.heap.stats().collections
    }

    /// What an operation pays for a full collection it asks for out of
    /// turn ([`Vm::collect_for_retry`], `Vm::collect_for_result`): a unit
    /// per 64 bytes of the memory limit, at most [`RETRY_CHARGE`].
    pub(crate) fn collection_charge(&self) -> u64 {
        (self.heap.memory_limit() / 64).min(RETRY_CHARGE)
    }

    /// Asks for the collection a retry after `MemoryError` waits for: a
    /// fresh one, a mark in progress dropped (`Heap::restart_collection`),
    /// held as `gc.collect`'s is. Garbage is made by dropping references,
    /// which allocates nothing, and a value dropped during a mark survives
    /// it, so no count says a collection cannot help. The failing operation
    /// is charged for it instead, at once: a unit per 64 bytes of the
    /// memory limit (what one collection can mark at most), at most
    /// [`RETRY_CHARGE`] units. A constant of the session, so the charge
    /// does not depend on how the run is sliced or where snapshots fall,
    /// and a guest that catches `MemoryError` in a loop pays for each
    /// collection in fuel.
    fn collect_for_retry(&mut self) {
        self.burn(self.collection_charge());
        self.heap.restart_collection();
        self.collect_in_steps(false);
    }

    fn oom_retry_if(&mut self, pure: bool) -> bool {
        self.fresh_consumed();
        let rerun = core::mem::replace(&mut self.oom_rerun, u64::MAX) == self.op_stamp();
        let retry = !rerun
            && pure
            && self.exc_type(self.exc) == T::MemoryError
            && self.room_may_help()
            && self.consumed.is_empty()
            && self.partial.is_none();
        if retry {
            self.collect_for_retry();
            self.oom_wait = true;
        }
        retry
    }

    /// Clears a requested collection, giving back the ticks it set aside.
    pub(crate) fn gc_requested_done(&mut self) {
        self.gc_pending = false;
        self.ticks += self.tick_spare;
        self.tick_spare = 0;
    }

    /// The flag that parks the running cell at its next safepoint
    /// ([`ParkReason::Requested`]); any thread may set it. The engine
    /// clears it when it parks.
    #[must_use]
    pub fn park_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.park)
    }

    /// GC stress (testing): every safepoint checks and collects, so a value
    /// some root misses is reclaimed at the first chance. Very slow. Over
    /// 1 MiB live it collects after every 1/64 of the live
    /// bytes allocated (`Vm::stress_due`), so a case's stress work grows
    /// with what it allocates, not with its safepoints times its heap: own
    /// cases 425, 427, 463 and 466 took hours at every safepoint over a few
    /// MiB live, and the other 249 of 260 never pass 1 MiB \[M\].
    pub fn set_gc_stress(&mut self, on: bool) {
        self.gc_stress = on;
        self.reset_ticks();
    }

    /// Under GC stress: does this safepoint collect? Every one while less
    /// than 1 MiB is live; over it, once 1/64 of the live bytes were
    /// allocated since the last collection, so a new object still meets a
    /// collection soon after it is made. (A literal, not a `const`: every
    /// `const` item moves the snapshot build id.)
    pub(crate) fn stress_due(&self) -> bool {
        let st = self.heap.stats();
        st.live_bytes < 1 << 20 || st.allocated_since_gc >= st.live_bytes / 64
    }

    /// Incremental GC stress (testing): a mark is always in progress,
    /// advanced a few objects at every safepoint and checked against a full
    /// mark when it ends, so a write the dirty flags miss aborts.
    pub fn set_gc_inc_stress(&mut self, on: bool) {
        self.gc_inc_stress = on;
        if on {
            self.heap.set_incremental(16, 0);
            self.heap.set_incremental_verify(true);
        }
        self.reset_ticks();
    }

    /// Sets the host clock the deadline is compared with.
    pub fn set_clock(&mut self, clock: Option<Clock>) {
        self.clock = clock;
    }

    /// Shares decoded bundled modules through `cache` (`None`: each
    /// session decodes its own). A session behaves the same either way:
    /// a cached module is charged the workspace its decoding took.
    pub fn set_module_cache(&mut self, cache: Option<Arc<dyn ModuleCache>>) {
        self.module_cache = cache;
    }

    /// Whether snapshots store the code of host modules' current
    /// definitions (as they store user code) instead of referring to it by
    /// key, for images restored where the embedder cannot give it back
    /// cheaply. Not saved.
    pub fn set_host_code_in_snapshots(&mut self, on: bool) {
        self.host_code_in_snapshots = on;
    }

    /// The cache [`Vm::set_module_cache`] set.
    #[must_use]
    pub fn module_cache(&self) -> Option<&Arc<dyn ModuleCache>> {
        self.module_cache.as_ref()
    }

    /// Sets the wall clock `time.time()`, `datetime.now()` and friends read
    /// (`None`: they raise `RuntimeError`). The engine has no clock of its
    /// own (docs/spec.md §10).
    pub fn set_wall_clock(&mut self, clock: Option<WallClock>) {
        self.wall_clock = clock;
    }

    /// Sets the local time zone (TZif data, RFC 8536; `None`: UTC) that
    /// `time.localtime`, `time.mktime` and naive `datetime` conversions
    /// read when `time` is first imported. Saved in snapshots.
    pub fn set_local_zone(&mut self, tzif: Option<Vec<u8>>) {
        self.local_zone = tzif;
    }

    /// Sets where `zoneinfo.ZoneInfo(key)` gets a zone's TZif data
    /// (`None`: no zones but `UTC`). Not saved in snapshots; zones already
    /// loaded are.
    pub fn set_zone_source(&mut self, source: Option<ZoneSource>) {
        self.zone_source = source;
    }

    /// Sets the entropy source of `os.urandom`, unseeded `random` and
    /// `uuid.uuid4` (`None`: they raise `RuntimeError`).
    pub fn set_entropy(&mut self, entropy: Option<Entropy>) {
        self.entropy = entropy;
    }

    /// Parks with [`ParkReason::Deadline`] at the first safepoint where the
    /// clock reads at least `deadline`; one-shot (cleared when it fires).
    pub fn set_deadline(&mut self, deadline: Option<u64>) {
        self.deadline = deadline;
    }

    /// Starts rendering an uncaught exception for the host
    /// ([`Vm::exception_parts`], [`Vm::render_exception`]): text stops
    /// once it is longer than `cap` bytes, or once the deadline has passed
    /// during long work (a large `repr`, many chained or grouped
    /// exceptions), so the report is bounded however large the exception's
    /// values are. [`Vm::end_report`] ends it.
    pub fn begin_report(&mut self, cap: usize) {
        self.report_cap = cap;
        self.report_cut = false;
        self.report_steps = 0;
    }

    /// Ends [`Vm::begin_report`]: whether rendering stopped early.
    pub fn end_report(&mut self) -> bool {
        self.report_cap = usize::MAX;
        core::mem::take(&mut self.report_cut)
    }

    /// While a report renders: whether `len` bytes are past its cap or,
    /// with `long` (long work), the deadline has passed. Records the stop.
    pub(crate) fn report_stop(&mut self, len: usize, long: bool) -> bool {
        if self.report_cap == usize::MAX {
            return false;
        }
        let late = long
            && matches!((self.deadline, self.clock.as_ref()), (Some(d), Some(clock)) if clock() >= d);
        if len > self.report_cap || late {
            self.report_cut = true;
        }
        self.report_cut
    }

    /// Writes `s` to the output callback.
    pub(crate) fn write_out(&mut self, stream: Stream, s: &str) {
        if let Some(out) = self.output.as_mut() {
            out(stream, s);
        }
    }

    /// Loads a compiled program into the code table; returns the index of
    /// its module body.
    pub fn load(&mut self, prog: &Program, filename: &str, source: &str) -> Result<u32, Raised> {
        self.load_in(prog, filename, source, GSLOT_USER)
    }

    /// [`Vm::load`] for code that runs in the globals `gslot` names (engine
    /// code when not the user's).
    pub(crate) fn load_in(
        &mut self,
        prog: &Program,
        filename: &str,
        source: &str,
        gslot: u32,
    ) -> Result<u32, Raised> {
        let old_codes = self.codes.len();
        let old_ics = self.ics.len();
        let old_sources = self.sources.len();
        self.reserve_code(
            prog.codes
                .iter()
                .map(|c| CodeObj::load_bytes(c))
                .sum::<u64>()
                + (size_of::<Source>() + filename.len() + source.len()) as u64,
        )?;
        let loaded = (|| {
            // Exact growth keeps table capacities inside the admission estimate.
            let n_ics = prog.codes.iter().map(|c| u64::from(c.n_ics)).sum::<u64>();
            let n_ics = usize::try_from(n_ics).map_err(|_| self.err(T::MemoryError, ""))?;
            if Arc::make_mut(&mut self.codes)
                .try_reserve_exact(prog.codes.len())
                .is_err()
                || self.sources.try_reserve_exact(1).is_err()
                || self.ics.try_reserve_exact(n_ics).is_err()
            {
                return Err(self.err(T::MemoryError, ""));
            }
            let src = u32::try_from(self.sources.len()).unwrap_or(u32::MAX);
            self.sources.push(Source {
                name: filename.into(),
                text: source.into(),
                host: None,
                reclaim_index: 0,
            });
            let module_base = u32::try_from(self.codes.len()).unwrap_or(u32::MAX);
            for code in &prog.codes {
                let mut consts = Vec::with_capacity(code.consts.len());
                for k in &code.consts {
                    let v = match k {
                        Konst::None => Value::NONE,
                        Konst::Bool(b) => Value::from_bool(*b),
                        Konst::Ellipsis => Value::ELLIPSIS,
                        Konst::Int(i) => self.new_int(*i)?,
                        Konst::Float(f) => Value::float(*f),
                        Konst::Str(s) => self.intern(s)?,
                        Konst::BigInt(s) => {
                            match crate::intops::parse_int_text(s, 0, crate::intops::MAX_STR_DIGITS)
                            {
                                Ok(b) => self.new_big(&b)?,
                                Err(_) => {
                                    return Err(self.err(T::SyntaxError, "invalid integer literal"));
                                }
                            }
                        }
                        Konst::Bytes(b) => self.new_bytes(b)?,
                        Konst::Complex(re, im) => self.new_complex(*re, *im)?,
                        Konst::Names(ns) => {
                            let mut items = Vec::with_capacity(ns.len());
                            for n in ns {
                                items.push(self.intern(n)?);
                            }
                            self.alloc_tuple(&items)?
                        }
                    };
                    consts.push(v);
                }
                let mut names = Vec::with_capacity(code.names.len());
                for n in &code.names {
                    names.push(self.intern(n)?);
                }
                let ic_base = u32::try_from(self.ics.len()).unwrap_or(u32::MAX);
                self.ics
                    .extend(core::iter::repeat_n(Ic::Empty, code.n_ics as usize));
                Arc::make_mut(&mut self.codes).push(Arc::new(CodeObj {
                    code: code.clone(),
                    consts,
                    names,
                    ic_base,
                    module_base,
                    source: src,
                    prelude: gslot != GSLOT_USER,
                    gslot,
                    reclaim_live: false,
                    reclaim_index: 0,
                }));
            }
            Ok(module_base + prog.entry)
        })();
        if loaded.is_err() {
            self.code_sums = (0, 0, 0);
            Arc::make_mut(&mut self.codes).truncate(old_codes);
            self.ics.truncate(old_ics);
            self.sources.truncate(old_sources);
            Arc::make_mut(&mut self.codes).shrink_to_fit();
            self.ics.shrink_to_fit();
            self.sources.shrink_to_fit();
        }
        self.reserve_code(0)?;
        loaded
    }

    /// The one-instruction code (`RETURN`) that continuation frames run.
    fn add_trampoline(&mut self) -> u32 {
        let ci = u32::try_from(self.codes.len()).unwrap_or(0);
        let ic_base = u32::try_from(self.ics.len()).unwrap_or(0);
        Arc::make_mut(&mut self.codes).push(Arc::new(CodeObj::trampoline(ci, ic_base)));
        ci
    }

    /// The prelude function `name`.
    pub(crate) fn prelude_fn(&mut self, name: &str) -> Result<Value, Raised> {
        let k = self.intern(name)?;
        match self.prelude_get(k)? {
            Some(f) => Ok(f),
            None => Err(self.err(
                T::SystemError,
                &alloc::format!("micro-monty: prelude function {name} is not loaded"),
            )),
        }
    }

    /// Leaves a request to call prelude function `name` with `args` and
    /// finish with `cont`; returns `UNDEFINED` (the request marker).
    #[cold]
    pub(crate) fn prelude_request(
        &mut self,
        name: &str,
        args: Vec<Value>,
        cont: crate::cont::Cont,
    ) -> Result<Value, Raised> {
        let func = self.prelude_fn(name)?;
        Ok(self.request_call(func, args, cont))
    }

    /// Requests a call of `func` with `args`, as [`Vm::prelude_request`].
    pub(crate) fn request_call(
        &mut self,
        func: Value,
        args: Vec<Value>,
        cont: crate::cont::Cont,
    ) -> Value {
        self.request = Some(Request::Call {
            func,
            args: args.into(),
            kw: Value::NONE,
            cont,
            outer: crate::cont::Cont::None,
        });
        Value::UNDEFINED
    }

    /// Marks the pending failure as "needs Python code": the caller hands
    /// the whole operation to its prelude twin. If nothing does, the
    /// exception left pending says so.
    pub(crate) fn need_py(&mut self) -> Raised {
        let _ = self.err(
            T::SystemError,
            "micro-monty: this operation needs Python code here",
        );
        self.slow = true;
        Raised
    }

    /// A str-keyed dict lookup that never raises (`None` if absent or `d`
    /// is not a dict).
    pub(crate) fn dict_get_str(&self, d: Value, k: Value) -> Option<Value> {
        if !d.is_obj_kind(ty::DICT as u8) {
            return None;
        }
        self.dict_lookup_str(d, k)
    }

    // ----------------------------------------------------------- errors

    /// Maps a heap error to a pending `MemoryError` or `SystemError`.
    pub(crate) fn hx<X>(&mut self, r: Result<X, mm_core::Error>) -> Result<X, Raised> {
        match r {
            Ok(x) => Ok(x),
            Err(mm_core::Error::OutOfMemory) => {
                self.exc = self.memory_error;
                Err(Raised)
            }
            Err(_) => Err(self.err(
                T::SystemError,
                "micro-monty internal error: bad heap access",
            )),
        }
    }

    /// Raises `t(msg)`; returns the marker to propagate.
    pub(crate) fn err(&mut self, t: T, msg: &str) -> Raised {
        let tv = self.types[t as usize];
        let exc = (|| {
            let m = self.new_str(msg)?;
            let args = self.alloc_tuple(&[m])?;
            self.alloc_exc(tv, args)
        })();
        self.exc = match exc {
            Ok(e) => e,
            Err(Raised) => self.memory_error,
        };
        Raised
    }

    /// Raises `t(arg)` with a non-string argument (`KeyError(key)`).
    pub(crate) fn err_value(&mut self, t: T, arg: Value) -> Raised {
        let tv = self.types[t as usize];
        let exc = (|| {
            let args = self.alloc_tuple(&[arg])?;
            self.alloc_exc(tv, args)
        })();
        self.exc = match exc {
            Ok(e) => e,
            Err(Raised) => self.memory_error,
        };
        Raised
    }

    /// The pending or uncaught exception.
    #[must_use]
    pub fn exception(&self) -> Value {
        self.exc
    }

    /// `repr(v)` cut to at most `limit` bytes, and whether it was cut; on
    /// failure the exception is pending.
    pub fn repr_capped_of(&mut self, v: Value, limit: usize) -> Result<(String, bool), Raised> {
        let mut out = self.repr_capped(v, limit)?;
        let cut = out.len() > limit;
        let mut end = limit.min(out.len());
        while !out.is_char_boundary(end) {
            end -= 1;
        }
        out.text.truncate(end);
        if out.text.capacity() > limit {
            out.text.shrink_to_fit();
        }
        Ok((out.text, cut))
    }

    /// While set, native `repr` and `str` (not Python code, which keeps
    /// CPython's `ValueError`) render an int past 14,284 bits as
    /// `<int with N bits>` instead of its digits. For an embedder's
    /// presentation of a completion value; never on while guest code runs.
    pub fn set_int_bits_text(&mut self, on: bool) {
        self.int_bits_text = on;
    }

    /// Whether the next cell's completion value needs its `repr`/`str`:
    /// user `__repr__`/`__str__` then run before the cell ends.
    pub fn set_want_completion_text(&mut self, want: bool) {
        self.want_text = want;
        self.completion_text = None;
    }

    /// The `(repr, str)` of the last completion value, when user code
    /// computed them.
    pub fn take_completion_text(&mut self) -> Option<(String, String)> {
        self.take_completion_text_capped(usize::MAX)
            .map(|((r, _), (s, _))| (r, s))
    }

    /// `type(v).__name__`.
    #[must_use]
    pub fn type_name_of(&self, v: Value) -> &str {
        self.type_name(v)
    }

    pub(crate) fn alloc_exc(&mut self, typ: Value, args: Value) -> Result<Value, Raised> {
        let r = {
            let res = self.heap.alloc_init(
                ty::EXC,
                0,
                &[
                    typ,
                    args,
                    Value::NONE,
                    Value::NONE,
                    Value::NONE,
                    Value::NONE,
                    Value::NONE,
                    Value::NONE,
                ],
                &[0],
            );
            self.hx(res)
        }?;
        Ok(Value::from_ref(r, ty::EXC as u8))
    }

    // ------------------------------------------------------ slot access

    /// Value slot `i` of `r` (`UNDEFINED` if out of range).
    #[inline]
    pub(crate) fn slot(&self, r: Ref, i: u32) -> Value {
        self.heap.get(r, i).unwrap_or(Value::UNDEFINED)
    }

    /// Value slot `i` of the object `v` refers to.
    #[inline]
    pub(crate) fn vslot(&self, v: Value, i: u32) -> Value {
        match v.as_obj() {
            Some(r) => self.slot(r, i),
            None => Value::UNDEFINED,
        }
    }

    /// Raw slot `i` of `r` (0 if out of range).
    #[inline]
    pub(crate) fn raw(&self, r: Ref, i: u32) -> u64 {
        self.heap.get_raw(r, i).unwrap_or(0)
    }

    #[inline]
    pub(crate) fn set_slot(&mut self, r: Ref, i: u32, v: Value) {
        let _ = self.heap.set(r, i, v);
    }

    #[inline]
    pub(crate) fn set_raw(&mut self, r: Ref, i: u32, w: u64) {
        let _ = self.heap.set_raw(r, i, w);
    }

    // Out-of-line forms of the accessors above. Code that `Vm::dispatch`
    // does not inline calls these: one call instead of the inlined object
    // validation saves about 59 KB of `hello.wasm` [M]. Code that dispatch
    // inlines (interp.rs, ops.rs, this file, class.rs, cont.rs,
    // generator.rs, aio.rs, modules.rs, host.rs, intops.rs, dunder.rs,
    // dict.rs, json.rs and the hot natives that keep the inline forms)
    // keeps the inline forms: any change to what dispatch inlines moves
    // its register allocation, and with it the hot loop's instruction
    // count by several percent [M].

    /// [`Vm::slot`], out of line.
    #[inline(never)]
    pub(crate) fn slot_o(&self, r: Ref, i: u32) -> Value {
        self.heap.get(r, i).unwrap_or(Value::UNDEFINED)
    }

    /// [`Vm::vslot`], out of line.
    #[inline(never)]
    pub(crate) fn vslot_o(&self, v: Value, i: u32) -> Value {
        match v.as_obj() {
            Some(r) => self.heap.get(r, i).unwrap_or(Value::UNDEFINED),
            None => Value::UNDEFINED,
        }
    }

    /// [`Vm::raw`], out of line.
    #[inline(never)]
    pub(crate) fn raw_o(&self, r: Ref, i: u32) -> u64 {
        self.heap.get_raw(r, i).unwrap_or(0)
    }

    /// [`Vm::set_slot`], out of line.
    #[inline(never)]
    pub(crate) fn set_slot_o(&mut self, r: Ref, i: u32, v: Value) {
        let _ = self.heap.set(r, i, v);
    }

    /// [`Vm::set_raw`], out of line.
    #[inline(never)]
    pub(crate) fn set_raw_o(&mut self, r: Ref, i: u32, w: u64) {
        let _ = self.heap.set_raw(r, i, w);
    }

    /// [`Vm::str_ref`], out of line.
    #[inline(never)]
    pub(crate) fn str_ref_o(&self, v: Value) -> &str {
        v.as_obj()
            .and_then(|r| self.heap.as_str(r).ok())
            .unwrap_or("")
    }

    /// [`Vm::hx`] with its error path out of line.
    #[inline]
    pub(crate) fn hx_o<X>(&mut self, r: Result<X, mm_core::Error>) -> Result<X, Raised> {
        match r {
            Ok(x) => Ok(x),
            Err(e) => Err(self.hx_err(e)),
        }
    }

    #[inline(never)]
    fn hx_err(&mut self, e: mm_core::Error) -> Raised {
        if e == mm_core::Error::OutOfMemory {
            self.exc = self.memory_error;
            return Raised;
        }
        self.err(
            T::SystemError,
            "micro-monty internal error: bad heap access",
        )
    }

    /// [`Vm::int_of`] with everything past a small int out of line.
    #[inline]
    pub(crate) fn int_of_o(&self, v: Value) -> Option<i64> {
        if let Some(i) = v.as_int() {
            return Some(i);
        }
        self.int_of_slow(v)
    }

    #[inline(never)]
    fn int_of_slow(&self, v: Value) -> Option<i64> {
        self.int_of(v)
    }

    /// Slot count of object `r`.
    pub(crate) fn obj_len(&self, r: Ref) -> u32 {
        self.heap.header(r).map_or(0, mm_core::Header::len)
    }

    // ----------------------------------------------------- constructors

    /// An interned `str`.
    pub(crate) fn intern(&mut self, s: &str) -> Result<Value, Raised> {
        let r = {
            let res = self.heap.intern(s);
            self.hx(res)
        }?;
        Ok(Value::from_ref(r, ty::STR as u8))
    }

    /// A new `str`, shared (interned) when it is [`shared_char`].
    pub(crate) fn new_str(&mut self, s: &str) -> Result<Value, Raised> {
        if shared_char(s.as_bytes()) {
            return self.intern(s);
        }
        if s.len() >= 64 {
            self.burn_result(s.len(), true);
        }
        let r = {
            let res = self.heap.new_str(s);
            self.hx(res)
        }?;
        Ok(Value::from_ref(r, ty::STR as u8))
    }

    /// The constructor native of builtin type `t` ([`crate::builtins::ctor`],
    /// cached on first use).
    pub(crate) fn ctor_of(&mut self, t: T) -> Option<u32> {
        let slot = self.ctors.get_mut(t as usize)?;
        if *slot == CTOR_UNKNOWN {
            *slot = crate::builtins::ctor(t).unwrap_or(CTOR_NONE);
        }
        Some(*slot).filter(|&c| c != CTOR_NONE)
    }

    /// The decimal text of `i` as a new `str`.
    #[inline(never)]
    pub(crate) fn int_str(&mut self, i: i64) -> Result<Value, Raised> {
        let mut buf = [0u8; 20];
        self.new_str(i64_text(i, &mut buf))
    }

    /// [`Vm::build_text`] for `BUILD_STRING`, its only caller: past a
    /// step's work it builds nothing and sets `long_build`, and `execute`
    /// joins the parts a step at a time (`Vm::build_string_steps`).
    pub(crate) fn build_str(
        &mut self,
        bytes: usize,
        build: impl FnOnce(&Self, &mut StrBuf) -> Result<(), mm_core::Error>,
    ) -> Result<Value, Raised> {
        self.build_in::<true>(bytes, build)
    }

    /// Build exactly `bytes` UTF-8 bytes without mutating the VM. Reserve the
    /// native buffer before growing it and keep it charged while copying into
    /// the arena. Reconcile cached capacity on both success and failure.
    pub(crate) fn build_text(
        &mut self,
        bytes: usize,
        build: impl FnOnce(&Self, &mut StrBuf) -> Result<(), mm_core::Error>,
    ) -> Result<Value, Raised> {
        self.build_in::<false>(bytes, build)
    }

    fn build_in<const STEPS: bool>(
        &mut self,
        bytes: usize,
        build: impl FnOnce(&Self, &mut StrBuf) -> Result<(), mm_core::Error>,
    ) -> Result<Value, Raised> {
        let mut b = core::mem::take(&mut self.sbuf);
        b.clear();
        let old_capacity = b.capacity();
        let result = (|| {
            // Reusing the cached buffer keeps its existing reservation. Only
            // growth needs to rescan execution storage (including all tasks).
            if bytes > old_capacity {
                if STEPS {
                    self.reserve_build(bytes)?;
                } else {
                    self.reserve_execution(bytes as u64)?;
                }
            }
            let res = b.try_reserve_exact(bytes);
            self.hx(res)?;
            let res = build(self, &mut b);
            self.hx(res)?;
            self.str_from_buf(b)
        })();
        // This only releases the temporary reservation or transfers it to the
        // retained buffer. Neither path can increase the admitted capacity.
        if bytes > old_capacity || self.sbuf.capacity() != old_capacity {
            let _ = self.reserve_execution(0);
        }
        result
    }

    /// `BUILD_STRING`'s growth: a join past a step's work stops before
    /// copying anything, for `execute` to step it.
    #[inline(never)]
    fn reserve_build(&mut self, bytes: usize) -> Result<(), Raised> {
        if (bytes / 64) as u64 >= crate::nstep::STEP_WORK {
            (self.long_build, self.check_before) = (true, true);
            return Err(Raised);
        }
        self.reserve_execution(bytes as u64)
    }

    /// Appends the text of `str` value `v` to `b` (nothing for non-strings).
    pub(crate) fn sbuf_push(&self, b: &mut StrBuf, v: Value) {
        if let Some(r) = v.as_obj().filter(|_| Vm::is_str(v)) {
            let _ = b.push_ref(&self.heap, r);
        }
    }

    /// A `str` holding the text of `b`, as [`Vm::new_str`] makes one.
    #[inline(never)]
    fn str_from_buf(&mut self, b: StrBuf) -> Result<Value, Raised> {
        let r = if shared_char(b.as_bytes()) {
            let s = core::str::from_utf8(b.as_bytes()).unwrap_or("");
            self.intern(s)
        } else {
            if b.len() >= 64 {
                self.burn_result(b.len(), true);
            }
            let res = self.heap.new_str_from(&b);
            self.hx(res).map(|r| Value::from_ref(r, ty::STR as u8))
        };
        if b.capacity() <= 1 << 16 {
            self.sbuf = b;
        }
        r
    }

    /// A tuple of `items`.
    pub(crate) fn alloc_tuple(&mut self, items: &[Value]) -> Result<Value, Raised> {
        if items.is_empty() && self.empty_tuple != Value::NONE {
            return Ok(self.empty_tuple);
        }
        if items.len() >= 8 {
            self.burn_result(items.len() * 8, false);
        }
        let r = {
            let res = self.heap.alloc_init(ty::TUPLE, 0, items, &[]);
            self.hx(res)
        }?;
        Ok(Value::from_ref(r, ty::TUPLE as u8))
    }

    /// A list of `items`.
    pub fn new_list(&mut self, items: &[Value]) -> Result<Value, Raised> {
        // Exact room for given items (appends double it); an empty list
        // gets room for a few.
        let cap = if items.is_empty() { 4 } else { items.len() };
        if cap >= 8 {
            self.burn_result(cap * 8, false);
        }
        let buf = self.alloc_items(cap)?;
        let _ = self.heap.set_values(buf, 0, items);
        let r = {
            let res = self.heap.alloc_init(
                ty::LIST,
                0,
                &[Value::from_ref(buf, ty::BUF as u8)],
                &[items.len() as u64],
            );
            self.hx(res)
        }?;
        Ok(Value::from_ref(r, ty::LIST as u8))
    }

    pub(crate) fn alloc_buf(&mut self, n: usize) -> Result<Ref, Raised> {
        let n32 = u32::try_from(n).map_err(|_| self.err(T::MemoryError, ""))?;
        {
            let res = self.heap.alloc(ty::BUF, 0, n32, 0);
            self.hx(res)
        }
    }

    /// A list's item buffer of `n` slots, which its builder writes before
    /// reading: a large one is not filled (`Heap::alloc_unfilled`), so
    /// its pages fault in a step at a time as the items go in.
    pub(crate) fn alloc_items(&mut self, n: usize) -> Result<Ref, Raised> {
        let n32 = u32::try_from(n).map_err(|_| self.err(T::MemoryError, ""))?;
        {
            let res = self.heap.alloc_unfilled(ty::BUF, 0, n32);
            self.hx(res)
        }
    }

    /// An int: a `SmallInt` or a heap int.
    pub(crate) fn new_int(&mut self, i: i64) -> Result<Value, Raised> {
        if let Some(v) = Value::int(i) {
            return Ok(v);
        }
        self.new_big(&crate::bigint::Big::from_i64(i))
    }

    /// An int from an `i128` result.
    pub(crate) fn int_from_i128(&mut self, i: i128) -> Result<Value, Raised> {
        match i64::try_from(i) {
            Ok(v) => self.new_int(v),
            Err(_) => self.new_big(&crate::bigint::Big::from_i128(i)),
        }
    }

    /// The value of an int or bool, if `v` is one that fits `i64`.
    #[inline]
    pub(crate) fn int_of(&self, v: Value) -> Option<i64> {
        if let Some(i) = v.as_int() {
            return Some(i);
        }
        if v.is_obj_kind(ty::INT as u8) {
            return self.heap_int_i64(v);
        }
        if v == Value::TRUE {
            return Some(1);
        }
        if v == Value::FALSE {
            return Some(0);
        }
        None
    }

    /// The type object for `t`.
    pub(crate) fn type_obj(&self, t: T) -> Value {
        self.types[t as usize]
    }

    /// The `T` a type object stands for.
    pub(crate) fn type_of_typeobj(&self, v: Value) -> Option<T> {
        if !v.is_obj_kind(ty::TYPE as u8) {
            return None;
        }
        let r = v.as_obj()?;
        T::from_index(usize::try_from(self.raw(r, 1)).ok()?)
    }

    /// The Python type of `v`.
    pub(crate) fn type_of(&self, v: Value) -> T {
        if v.is_int() {
            return T::int;
        }
        if v.is_float() {
            return T::float;
        }
        if let Some(s) = v.as_special() {
            return match s {
                mm_core::Special::None => T::NoneType,
                mm_core::Special::True | mm_core::Special::False => T::bool,
                mm_core::Special::NotImplemented => T::NotImplementedType,
                mm_core::Special::Ellipsis => T::ellipsis,
                mm_core::Special::Undefined => T::object,
            };
        }
        if v.as_native().is_some() {
            return T::builtin;
        }
        let Some(kind) = v.obj_kind() else {
            return T::object;
        };
        match u16::from(kind) {
            ty::STR => T::str,
            ty::BYTES => T::bytes,
            ty::BYTEARRAY => T::bytearray,
            ty::MATCH => T::Match,
            ty::TUPLE => T::tuple,
            ty::LIST => T::list,
            ty::DICT => T::dict,
            ty::SET if self.set_frozen(v) => T::frozenset,
            ty::SET => T::set,
            ty::FUNC | ty::HOSTFN => T::function,
            ty::CELL => T::cell,
            ty::RANGE => T::range,
            ty::COMPLEX => T::complex,
            ty::TYPE | ty::CLASS => T::type_,
            ty::INT => T::int,
            ty::SLICE => T::slice,
            ty::BOUND => T::builtin,
            ty::ENUMERATE => T::enumerate,
            ty::ZIP => T::zip,
            ty::REVERSED => T::list_reverseiterator,
            ty::EXC => {
                let t = self.vslot(v, exc_slot::TYPE);
                self.class_base_t(t)
            }
            ty::METHOD => T::method,
            ty::GEN => T::generator,
            ty::PROPERTY => T::property,
            ty::STATICMETHOD => T::staticmethod,
            ty::CLASSMETHOD => T::classmethod,
            ty::SUPER => T::super_,
            ty::CALLITER => T::callable_iterator,
            ty::VIEW => match v.as_obj().map_or(0, |r| self.raw(r, 1)) {
                0 => T::dict_keys,
                1 => T::dict_values,
                _ => T::dict_items,
            },
            ty::ITER => crate::ops::iter_type(v.as_obj().map_or(0, |r| self.raw(r, 1))),
            ty::MODULE => T::module,
            ty::CORO | ty::SLEEP => T::coroutine,
            ty::TASK => T::Task,
            ty::FUTURE => T::Future,
            ty::GATHER => T::GatheringFuture,
            ty::STREAM => T::TextIOWrapper,
            ty::TRACEBACK => T::traceback,
            ty::AGEN => T::async_generator,
            ty::AGENOP
                if v.as_obj()
                    .map_or(0, |r| self.raw(r, crate::generator::ag::MODE))
                    == crate::generator::ag::SEND =>
            {
                T::async_generator_asend
            }
            ty::AGENOP => T::async_generator_athrow,
            _ => T::object,
        }
    }

    /// `type(v).__name__` (a user class's name for its instances).
    pub(crate) fn type_name(&self, v: Value) -> &str {
        if let Some(c) = self.user_class(v) {
            if self.uflags(v) & crate::class::f::TPNAME != 0
                && let Some(k) = self.find_interned("__mm_tpname__")
                && let Some(n) = self.class_lookup(c, k)
                && Vm::is_str(n)
            {
                return self.str_ref(n);
            }
            return self.str_ref(self.vslot(c, crate::class::cls::NAME));
        }
        self.type_of(v).name()
    }

    /// Is `v` a `str`?
    #[inline]
    pub(crate) fn is_str(v: Value) -> bool {
        v.is_obj_kind(ty::STR as u8)
    }

    /// The text of a `str` value (empty for non-strings).
    pub(crate) fn str_ref(&self, v: Value) -> &str {
        v.as_obj()
            .and_then(|r| self.heap.as_str(r).ok())
            .unwrap_or("")
    }

    /// The UTF-8 bytes of a `str` value.
    pub(crate) fn str_bytes(&self, v: Value) -> &[u8] {
        v.as_obj()
            .and_then(|r| self.heap.str_bytes(r).ok())
            .unwrap_or(&[])
    }

    /// Code-point length of a `str`.
    pub(crate) fn str_len(&self, v: Value) -> usize {
        v.as_obj()
            .and_then(|r| self.heap.str_chars(r).ok())
            .map_or(0, |n| usize::try_from(n).unwrap_or(0))
    }

    /// The byte offset of char `k` of `str` `v` (its length when `k` is at
    /// or past the end), found from the nearest known position
    /// (`Heap::str_offset`); the walk is charged as the byte natives are.
    pub(crate) fn str_offset(&mut self, v: Value, k: usize) -> usize {
        self.str_walk(v, k, false)
    }

    /// The char index of byte offset `at` of `str` `v`: [`Vm::str_offset`]
    /// the other way.
    pub(crate) fn str_index(&mut self, v: Value, at: usize) -> usize {
        self.str_walk(v, at, true)
    }

    #[inline(never)]
    fn str_walk(&mut self, v: Value, x: usize, by_byte: bool) -> usize {
        let found = v.as_obj().and_then(|r| {
            if by_byte {
                self.heap.str_index(r, x)
            } else {
                self.heap.str_offset(r, x)
            }
            .ok()
        });
        let (y, walked) = found.unwrap_or((0, 0));
        self.burn(walked as u64 / 64);
        y
    }

    /// Is the `str` pure ASCII?
    pub(crate) fn str_ascii(&self, v: Value) -> bool {
        v.as_obj()
            .and_then(|r| self.heap.header(r).ok())
            .is_some_and(|h| h.flags() & mm_core::flags::ASCII != 0)
    }

    /// Number of items of a tuple.
    pub(crate) fn tuple_len(&self, t: Value) -> usize {
        t.as_obj().map_or(0, |r| self.obj_len(r) as usize)
    }

    /// Items of a tuple, copied.
    pub(crate) fn tuple_items(&self, t: Value) -> Vec<Value> {
        match t.as_obj() {
            Some(r) => self.values_of(r, usize::MAX),
            None => Vec::new(),
        }
    }

    /// Length of a list.
    #[inline]
    pub(crate) fn list_len(&self, l: Value) -> usize {
        l.as_obj()
            .map_or(0, |r| usize::try_from(self.raw(r, 1)).unwrap_or(0))
    }

    /// The buffer of a list.
    #[inline]
    pub(crate) fn list_buf(&self, l: Value) -> Option<Ref> {
        l.as_obj().and_then(|r| self.slot(r, 0).as_obj())
    }

    /// Item `i` of a list (no bounds check beyond the heap's).
    #[inline]
    pub(crate) fn list_get(&self, l: Value, i: usize) -> Value {
        match self.list_buf(l) {
            Some(b) => self.slot(b, u32::try_from(i).unwrap_or(u32::MAX)),
            None => Value::UNDEFINED,
        }
    }

    /// The buffer and length of a list, read with one validation.
    #[inline]
    pub(crate) fn list_parts(&self, l: Value) -> Option<(Ref, u64)> {
        if !l.is_obj_kind(ty::LIST as u8) {
            return None;
        }
        let w = self.heap.slot_words(l.as_obj()?).ok()?;
        let [buf, len, ..] = *w else { return None };
        Some((Value::from_bits(buf)?.as_obj()?, len))
    }

    /// Where the items of list or tuple `v` are: the object holding them
    /// as value slots from 0, and how many there are.
    pub(crate) fn seq_slots(&self, v: Value) -> Option<(Ref, usize)> {
        if v.is_obj_kind(ty::TUPLE as u8) {
            let r = v.as_obj()?;
            return Some((r, self.obj_len(r) as usize));
        }
        let (b, n) = self.list_parts(v)?;
        Some((b, usize::try_from(n).ok()?))
    }

    /// Value slots `from..` of `src` into `out`, as many as both have.
    /// Not [`mm_core::Heap::read_values`]: once it had callers passing a
    /// start other than 0, LTO compiled it with three more instructions per
    /// call, and the generator fast path calls it on every resume (bench
    /// generators +0.5% Ir) [M].
    pub(crate) fn read_slots(heap: &Heap, src: Ref, from: usize, out: &mut [Value]) {
        let w = heap.slot_words(src).unwrap_or(&[]);
        for (o, &x) in out.iter_mut().zip(w.get(from..).unwrap_or(&[])) {
            *o = Value::from_bits(x).unwrap_or(Value::UNDEFINED);
        }
    }

    /// Copies `n` value slots of `src` from `from` into `dst` at `at`
    /// through a stack buffer: no native copy of the items. Callers charge
    /// the copy.
    #[inline(never)]
    pub(crate) fn copy_slots(&mut self, dst: Ref, at: usize, src: Ref, from: usize, n: usize) {
        let mut tmp = [Value::UNDEFINED; 256];
        let mut i = 0;
        while i < n {
            let k = (n - i).min(tmp.len());
            Self::read_slots(&self.heap, src, from + i, &mut tmp[..k]);
            if self
                .heap
                .set_values(dst, (at + i) as u32, &tmp[..k])
                .is_err()
            {
                return;
            }
            i += k;
        }
    }

    /// A tuple of `n` value slots of `sr` from `from`, slot to slot.
    pub(crate) fn tuple_from_slots(
        &mut self,
        sr: Ref,
        from: usize,
        n: usize,
    ) -> Result<Value, Raised> {
        if n == 0 {
            return self.alloc_tuple(&[]);
        }
        let t = {
            let res = self.heap.alloc(ty::TUPLE, 0, n as u32, 0);
            self.hx(res)
        }?;
        // As `alloc_tuple` charges: one unit per 8 slots.
        self.burn(n as u64 / 8);
        self.copy_slots(t, 0, sr, from, n);
        Ok(Value::from_ref(t, ty::TUPLE as u8))
    }

    /// `UNPACK_EX` of a list or tuple `v` of at least `before + after`
    /// items into the registers from `r`: the starred part is copied slot
    /// to slot, with no native copy of the items. False for anything else.
    #[inline(never)]
    pub(crate) fn unpack_ex_slots(
        &mut self,
        v: Value,
        r: usize,
        before: usize,
        after: usize,
    ) -> Result<bool, Raised> {
        let Some((sr, n)) = self.seq_slots(v).filter(|&(_, n)| n >= before + after) else {
            return Ok(false);
        };
        let mid = self.new_list(&[])?;
        self.extend_from(mid, sr, before, n - before - after)?;
        Self::read_slots(&self.heap, sr, 0, &mut self.stack[r..r + before]);
        self.stack[r + before] = mid;
        let rest = &mut self.stack[r + before + 1..r + before + 1 + after];
        Self::read_slots(&self.heap, sr, n - after, rest);
        Ok(true)
    }

    /// Appends `m` value slots of `sr` from `from` to list `l`. `sr` is
    /// still the source's slots when the source is `l` itself.
    fn extend_from(&mut self, l: Value, sr: Ref, from: usize, m: usize) -> Result<(), Raised> {
        let Some((r, buf, len)) = self.list_room(l, m)? else {
            return Ok(());
        };
        // One unit per item, as collecting them charged.
        self.burn(m as u64);
        self.copy_slots(buf, len, sr, from, m);
        self.set_raw_o(r, 1, (len + m) as u64);
        Ok(())
    }

    /// Appends `items` to list `l`.
    pub(crate) fn list_extend_values(&mut self, l: Value, items: &[Value]) -> Result<(), Raised> {
        let Some((r, buf, len)) = self.list_room(l, items.len())? else {
            return Ok(());
        };
        let _ = self.heap.set_values(buf, len as u32, items);
        self.set_raw_o(r, 1, (len + items.len()) as u64);
        Ok(())
    }

    /// List `l`'s buffer with room for `m` more items, grown once to the
    /// exact length if it has less: an extend's only allocation, so one
    /// that fails leaves `l` as it was and the op can run again. `l`'s
    /// object, the buffer and `l`'s length; None for a non-list.
    pub(crate) fn list_room(
        &mut self,
        l: Value,
        m: usize,
    ) -> Result<Option<(Ref, Ref, usize)>, Raised> {
        let (Some((buf, len)), Some(r)) = (self.list_parts(l), l.as_obj()) else {
            return Ok(None);
        };
        let len = usize::try_from(len).unwrap_or(usize::MAX);
        let need = len.saturating_add(m);
        if need <= self.obj_len(buf) as usize {
            return Ok(Some((r, buf, len)));
        }
        let nb = self.alloc_items(need)?;
        self.copy_slots(nb, 0, buf, 0, len);
        self.set_slot(r, 0, Value::from_ref(nb, ty::BUF as u8));
        Ok(Some((r, nb, len)))
    }

    /// The first `n` value slots of `r`, copied (fewer if it has fewer).
    pub(crate) fn values_of(&self, r: Ref, n: usize) -> Vec<Value> {
        let w = self.heap.slot_words(r).unwrap_or(&[]);
        w.iter()
            .take(n)
            .map(|&x| Value::from_bits(x).unwrap_or(Value::UNDEFINED))
            .collect()
    }

    /// A list of `n` items, to be filled before it is read.
    pub(crate) fn list_of_len(&mut self, n: usize) -> Result<Value, Raised> {
        self.list_with_room(n, n)
    }

    /// A list of `len` items, to be filled before they are read, with room for `cap` (appends up
    /// to it copy nothing).
    pub(crate) fn list_with_room(&mut self, len: usize, cap: usize) -> Result<Value, Raised> {
        // `new_list` leaves room for 4.
        let l = self.new_list(&[])?;
        let Some(r) = l.as_obj() else { return Ok(l) };
        if cap.max(len) > 4 {
            let buf = self.alloc_items(cap.max(len))?;
            self.set_slot(r, 0, Value::from_ref(buf, ty::BUF as u8));
        }
        self.set_raw(r, 1, len as u64);
        Ok(l)
    }

    /// The length of `v` if it is a list or tuple.
    pub(crate) fn seq_items_len(&self, v: Value) -> Option<usize> {
        (v.is_obj_kind(ty::LIST as u8) || v.is_obj_kind(ty::TUPLE as u8))
            .then(|| self.seq_words(v).len())
    }

    /// A new list of the items of list or tuple `v`, copied word by word
    /// through a stack buffer (no temporary `Vec` the size of `v`).
    #[inline(never)]
    pub(crate) fn seq_copy(&mut self, v: Value) -> Result<Value, Raised> {
        let n = self.seq_words(v).len();
        if n >= 8 {
            self.burn_result(n * 8, false);
        }
        let l = self.list_of_len(n)?;
        let Some(dst) = self.list_buf(l) else {
            return Ok(l);
        };
        let mut tmp = [Value::UNDEFINED; 256];
        let mut i = 0;
        while i < n {
            let k = {
                let src = self.seq_words(v).get(i..n).unwrap_or(&[]);
                for (t, &w) in tmp.iter_mut().zip(src) {
                    *t = Value::from_bits(w).unwrap_or(Value::NONE);
                }
                src.len().min(tmp.len())
            };
            if k == 0 {
                break;
            }
            let res = self.heap.set_values(dst, i as u32, &tmp[..k]);
            self.hx(res)?;
            i += k;
        }
        Ok(l)
    }

    /// List `to` takes list `from`'s items (its buffer, no copy), and
    /// `from` gets an empty one.
    pub(crate) fn list_move(&mut self, to: Value, from: Value) -> Result<(), Raised> {
        let fresh = self.alloc_buf(4)?;
        let (Some(t), Some(f)) = (to.as_obj(), from.as_obj()) else {
            return Ok(());
        };
        let (buf, len) = (self.slot(f, 0), self.raw(f, 1));
        self.set_slot(t, 0, buf);
        self.set_raw(t, 1, len);
        self.set_slot(f, 0, Value::from_ref(fresh, ty::BUF as u8));
        self.set_raw(f, 1, 0);
        Ok(())
    }

    /// Items of a list, copied.
    pub(crate) fn list_items(&self, l: Value) -> Vec<Value> {
        match self.list_parts(l) {
            Some((b, n)) => self.values_of(b, usize::try_from(n).unwrap_or(0)),
            None => Vec::new(),
        }
    }

    /// Appends to a list, growing its buffer by doubling.
    pub(crate) fn list_push(&mut self, l: Value, v: Value) -> Result<(), Raised> {
        match self.list_try_push(l, v)? {
            Some((buf, len)) => self.list_grow(l, buf, len, v),
            None => Ok(()),
        }
    }

    /// Appends to a list with room; a full one is left as it is and its
    /// buffer and length returned, for [`Vm::list_grow`] or a grow in steps.
    #[inline]
    pub(crate) fn list_try_push(
        &mut self,
        l: Value,
        v: Value,
    ) -> Result<Option<(Ref, u64)>, Raised> {
        let Some(r) = l.as_obj() else { return Ok(None) };
        let Some((buf, len)) = self.list_parts(l) else {
            return Err(self.err(T::SystemError, "invalid list buffer"));
        };
        let at = u32::try_from(len).unwrap_or(u32::MAX);
        if self.heap.set_values(buf, at, &[v]).is_err() {
            return Ok(Some((buf, len)));
        }
        if let Ok([n, ..]) = self.heap.raw_words_mut(r) {
            *n = len + 1;
        }
        Ok(None)
    }

    /// Appends `v` to full list `l` of `len` items in `buf`, through a
    /// buffer twice the size.
    #[cold]
    #[inline(never)]
    pub(crate) fn list_grow(
        &mut self,
        l: Value,
        buf: Ref,
        len: u64,
        v: Value,
    ) -> Result<(), Raised> {
        let Some(r) = l.as_obj() else { return Ok(()) };
        let at = u32::try_from(len).unwrap_or(u32::MAX);
        {
            let cap = u64::from(self.obj_len(buf));
            if len < cap {
                return Ok(());
            }
            // Doubling stops at the largest buffer (`MAX_OBJECT_BYTES`), so
            // a list can still grow to it one item at a time.
            let max = (mm_core::MAX_OBJECT_BYTES / 8).max(len + 1);
            let new_cap = usize::try_from((cap * 2).clamp(4, max)).unwrap_or(usize::MAX);
            let nb = self.alloc_items(new_cap)?;
            // Copy through a stack buffer: a temporary `Vec` of a big list
            // would touch as much fresh memory again.
            let mut tmp = [Value::UNDEFINED; 256];
            let mut i = 0;
            while u64::from(i) < len {
                let k = {
                    let src = self.heap.slot_words(buf).unwrap_or(&[]);
                    let src = src.get(i as usize..at as usize).unwrap_or(&[]);
                    let k = src.len().min(tmp.len());
                    for (t, &w) in tmp.iter_mut().zip(src) {
                        *t = Value::from_bits(w).unwrap_or(Value::UNDEFINED);
                    }
                    k
                };
                if k == 0 {
                    break;
                }
                let _ = self.heap.set_values(nb, i, &tmp[..k]);
                i += u32::try_from(k).unwrap_or(0);
            }
            let _ = self.heap.set_values(nb, at, &[v]);
            self.set_slot(r, 0, Value::from_ref(nb, ty::BUF as u8));
        }
        if let Ok([n, ..]) = self.heap.raw_words_mut(r) {
            *n = len + 1;
        }
        Ok(())
    }

    /// Replaces a list's contents.
    pub(crate) fn list_set_items(&mut self, l: Value, items: &[Value]) -> Result<(), Raised> {
        let Some(r) = l.as_obj() else { return Ok(()) };
        if items.len() >= 8 {
            self.burn_result(items.len() * 8, false);
        }
        let buf = self.alloc_buf(items.len().max(4))?;
        for (i, v) in items.iter().enumerate() {
            self.set_slot(buf, u32::try_from(i).unwrap_or(0), *v);
        }
        self.set_slot(r, 0, Value::from_ref(buf, ty::BUF as u8));
        self.set_raw(r, 1, items.len() as u64);
        Ok(())
    }

    /// The items of a list or tuple, if `v` is one.
    pub(crate) fn seq_items(&self, v: Value) -> Option<Vec<Value>> {
        (v.is_obj_kind(ty::LIST as u8) || v.is_obj_kind(ty::TUPLE as u8)).then(|| {
            self.seq_words(v)
                .iter()
                .map(|&x| Value::from_bits(x).unwrap_or(Value::UNDEFINED))
                .collect()
        })
    }

    /// The items of a list or tuple as value words, borrowed (empty for
    /// anything else).
    pub(crate) fn seq_words(&self, v: Value) -> &[u64] {
        let mut r = v.as_obj();
        let mut n = usize::MAX;
        if let Some((b, len)) = self.list_parts(v) {
            r = Some(b);
            n = usize::try_from(len).unwrap_or(0);
        } else if !v.is_obj_kind(ty::TUPLE as u8) {
            return &[];
        }
        let w = r.and_then(|r| self.heap.slot_words(r).ok()).unwrap_or(&[]);
        w.get(..n).unwrap_or(w)
    }

    /// A new cell holding `v`.
    pub(crate) fn new_cell(&mut self, v: Value) -> Result<Value, Raised> {
        let r = {
            let res = self.heap.alloc_init(ty::CELL, 0, &[v], &[]);
            self.hx(res)
        }?;
        Ok(Value::from_ref(r, ty::CELL as u8))
    }

    /// The source file of loaded code `ci`.
    pub(crate) fn source_of(&self, ci: u32) -> Option<&Source> {
        let c = self.codes.get(ci as usize)?;
        self.sources.get(c.source as usize)
    }
}

/// Whether UTF-8 text `b` is the empty str or one Latin-1 char (at most
/// U+00FF), which every str constructor shares instead of allocating, as
/// CPython caches them: the chars of a long text then cost a slot each,
/// not a str each.
#[inline]
pub(crate) fn shared_char(b: &[u8]) -> bool {
    b.len() <= 1 || (b.len() == 2 && b[0] < 0xC4)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, reason = "tests")]
    use super::*;

    /// `LIST_EXTEND` grows its list once, before it writes any item: a
    /// long list or tuple through a rebuild, anything else collected and
    /// then written. One that fails leaves the list as it was, so the op
    /// can run again after a collection.
    #[test]
    fn a_failing_list_extend_leaves_the_list_unchanged() {
        let mut vm = Vm::new(VmConfig {
            heap: HeapConfig {
                memory_limit: 8 << 20,
                ..HeapConfig::default()
            },
            ..VmConfig::default()
        })
        .expect("vm");
        let l = vm.new_list(&[Value::NONE; 3]).expect("list");
        let src = vm.list_of_len(600_000).expect("source");
        assert!(vm.extend_items(l, src).is_err());
        assert_eq!(vm.list_len(l), 3);
        let items = vec![Value::NONE; 600_000];
        assert!(vm.list_extend_values(l, &items).is_err());
        assert_eq!(vm.list_len(l), 3);
        // Within the limit, the items are appended in order.
        vm.list_extend_values(l, &[Value::FALSE, Value::TRUE])
            .expect("fits");
        assert_eq!(vm.list_len(l), 5);
        assert_eq!(vm.list_get(l, 4), Value::TRUE);
    }
}
