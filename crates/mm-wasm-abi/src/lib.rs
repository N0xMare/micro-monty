//! The micro-monty wasm guest interface, `mm-wasm`
//! (docs/spec.md §10.3): the names and codes both sides agree
//! on, and the hand-written binary encoding of what crosses the boundary.
//! `mm-wasm-guest` implements the guest side; a wasm host (the reference
//! one is `crates/mm-wasm-host`, outside the workspace) depends on this
//! crate for the same encoding.
//!
//! A host checks [`ABI_VERSION`] and the export list ([`export::REQUIRED`])
//! before it runs anything, and refuses a guest of another version.
//! micro-monty is 0.0.x with no compatibility layers: a change to the
//! interface (an import, a signature or meaning, a required export, a code
//! or an encoding) bumps [`ABI_VERSION`], with `crates/mm-wasm-host` and
//! `hosts/js` updated in the same change.
//!
//! - Session options travel in a tagged record ([`put_options`],
//!   [`option`]). A tag says whether a guest that does not know it may
//!   skip it ([`option::IGNORABLE`]) or must refuse the record. Only an
//!   option a guest can safely run without, a hint or a diagnostic, takes
//!   an ignorable tag; one that limits or changes behaviour (a limit, the
//!   clock, entropy, output, the text cut) takes a strict tag.
//! - Optional exports are ones a host looks for and works without, as
//!   [`export::SET_GC_YIELD`] and [`export::TEST_PANIC`].
//!
//! The snapshot format is separate: images carry the
//! engine's build id, and `mm_build_id` tells a host which images a guest
//! restores.
//!
//! The other tagged encodings are closed: a decoder refuses a tag it does
//! not know with [`Bad`] rather than guess. The facade's enums are
//! `#[non_exhaustive]`, so an encoder writes [`RESERVED`] for a variant
//! this version has no code for (a later facade's), which every decoder
//! refuses; a new variant therefore bumps [`ABI_VERSION`].
//!
//! The encoding is little-endian. `u32` lengths prefix strings (UTF-8),
//! byte strings and sequences. Values are one tag byte and a payload
//! ([`put_value`]). An optional item is a `0`/`1` byte, then the item.
//! Decoding never panics: bad input is a [`Bad`], counts are checked
//! against the remaining input, a container preallocates at most 1,024
//! items, and nesting is depth-counted ([`MAX_DEPTH`]).

use micro_monty::{
    CallStatus, CompileError, Completion, DefineError, HintGroup, HostArgs, HostCallRecord,
    HostClass, HostError, HostErrorKind, HostException, HostFunctionRef, HostModule, HostParam,
    HostSignature, HostValue, Limits, ParamKind, ParkReason, Phase, Progress, PyException, Resume,
    ResumeError, SnapshotError, TextCut, Unconverted,
};

/// What [`export::ABI_VERSION`] returns. A host refuses other versions.
pub const ABI_VERSION: u32 = 1;

/// The tag an encoder writes for a variant this version has no code for;
/// every decoder refuses it.
pub const RESERVED: u8 = 0xff;

/// The import module the guest needs from the host ([`import`]).
pub const IMPORT_MODULE: &str = "mm_host";

/// The guest's exports. All are `extern "C"`; `u32` and `i32` are wasm
/// `i32`, `u64` is `i64`, and `s` is a session id. The module also exports
/// `memory`.
pub mod export {
    /// `() -> u32`: [`super::ABI_VERSION`].
    pub const ABI_VERSION: &str = "mm_abi_version";
    /// `() -> u32`: the 32-byte snapshot build id in the output.
    pub const BUILD_ID: &str = "mm_build_id";
    /// `(len: u32) -> u32`: sizes the guest-owned input buffer to `len`
    /// bytes and returns its address.
    pub const INPUT: &str = "mm_input";
    /// `() -> u64`: the last output buffer, `address << 32 | length`.
    pub const OUTPUT: &str = "mm_output";
    /// `() -> i32`: a session from the options record in the input
    /// ([`super::put_options`]); its id, `-1` with a `PyException`, or `-2`
    /// with a bad-request string.
    pub const SESSION_NEW: &str = "mm_session_new";
    /// `() -> i32`: a session restored from the input (the options
    /// record, then the image); its id, `-1` with a `SnapshotError`, or
    /// `-2`.
    pub const SESSION_RESTORE: &str = "mm_session_restore";
    /// `(s) -> u32`: drops the session and its heap.
    pub const SESSION_DROP: &str = "mm_session_drop";
    /// `(s) -> u32`: defines the `HostModule` in the input; the output
    /// holds its `HostFunctionRef`s, or a `DefineError`
    /// ([`super::status::DEFINE_ERROR`]).
    pub const DEFINE: &str = "mm_define";
    /// `(s) -> u32` event: compiles the input (filename, source) and
    /// starts it.
    pub const START: &str = "mm_start";
    /// `(s) -> u32` event: answers the stop with the `Resume` in the input.
    pub const RESUME: &str = "mm_resume";
    /// `(s) -> u32`: `Session::cancel`, 1 if a cell was stopped.
    pub const CANCEL: &str = "mm_cancel";
    /// `(s) -> u32`: `Session::reset`; a `PyException` on failure.
    pub const RESET: &str = "mm_reset";
    /// `(s, max_bytes: u64) -> u32`: the image in the output, or a
    /// `SnapshotError`; `u64::MAX` keeps the default cap.
    pub const SNAPSHOT: &str = "mm_snapshot";
    /// `(s, fuel: u64) -> u32`: `Session::set_fuel`; `u64::MAX` is
    /// unlimited.
    pub const SET_FUEL: &str = "mm_set_fuel";
    /// `(s) -> u64`: `Session::fuel`; `u64::MAX` is unlimited.
    pub const FUEL: &str = "mm_fuel";
    /// `(s) -> u64`: `Session::fuel_wanted`.
    pub const FUEL_WANTED: &str = "mm_fuel_wanted";
    /// `(s, total: u64) -> u32`: `Session::set_fuel_total`; `u64::MAX` is
    /// no cap.
    pub const SET_FUEL_TOTAL: &str = "mm_set_fuel_total";
    /// `(s) -> u64`: `Session::fuel_total`; `u64::MAX` is no cap.
    pub const FUEL_TOTAL: &str = "mm_fuel_total";
    /// `(s) -> u32`: `Session::phase` ([`super::phase`]).
    pub const PHASE: &str = "mm_phase";
    /// `(s, ns: u64) -> u32`: a one-shot deadline on `mm_host.now_ns`;
    /// `u64::MAX` clears it.
    pub const SET_DEADLINE: &str = "mm_set_deadline";
    /// `(s) -> u32`: `Session::set_hint` from the input (the group's key,
    /// [`HintGroup::key`](micro_monty::HintGroup::key), then an optional
    /// string); a `PyException` on failure.
    pub const SET_HINT: &str = "mm_set_hint";
    /// `(s, cut: u32) -> u32`: `Session::set_text_cut`
    /// ([`super::text_cut`]).
    pub const SET_TEXT_CUT: &str = "mm_set_text_cut";
    /// `(s) -> u32`: `Session::set_local_timezone` from the input (optional
    /// `TZif` bytes).
    pub const SET_LOCAL_TIMEZONE: &str = "mm_set_local_timezone";
    /// `(s) -> u32`: `Session::call_log` in the output
    /// ([`super::put_call_log`]).
    pub const CALL_LOG: &str = "mm_call_log";
    /// `(s) -> u32`: `Session::environment` (the session's limits and fuel
    /// total), a string in the output.
    pub const ENVIRONMENT: &str = "mm_environment";
    /// `(s) -> u32`: `Session::stubs` in the output ([`super::put_stubs`]).
    pub const STUBS: &str = "mm_stubs";
    /// `(s) -> u32`: `Session::host_modules` in the output
    /// ([`super::put_modules`]).
    pub const HOST_MODULES: &str = "mm_host_modules";
    /// `(s) -> u32`: `Session::set_module_dunders` from the input (an
    /// optional `__name__`, then an optional `__file__`); a `PyException`
    /// on failure.
    pub const SET_MODULE_DUNDERS: &str = "mm_set_module_dunders";
    /// `(s, on: u32) -> u32`: `Session::set_int_bits_text`.
    pub const SET_INT_BITS_TEXT: &str = "mm_set_int_bits_text";
    /// `(s, on: u32) -> u32`: `Session::set_gc_stress` (engine testing).
    pub const SET_GC_STRESS: &str = "mm_set_gc_stress";
    /// `(s, on: u32) -> u32`: `Session::set_gc_yield`. Optional: a guest
    /// that predates it lacks it, and a host works without it.
    pub const SET_GC_YIELD: &str = "mm_set_gc_yield";
    /// `(op: u32) -> u32`: only with the guest's `test-hooks` feature; the
    /// next `start` (0), `resume` (1), `snapshot` (2) or `restore` (3)
    /// panics inside the engine.
    pub const TEST_PANIC: &str = "mm_test_panic";
    /// `(s) -> u32` event: `Session::pending`, the stop re-reported as the
    /// event that reported it (`HOST_CALL`, `WAITING` or `PARKED`, with the
    /// same payload), or `COMPLETE` without a completion when no cell is
    /// stopped.
    pub const PENDING: &str = "mm_pending";

    /// Every export a host may rely on (all but [`SET_GC_YIELD`] and
    /// [`TEST_PANIC`]).
    pub const REQUIRED: [&str; 32] = [
        ABI_VERSION,
        BUILD_ID,
        INPUT,
        OUTPUT,
        SESSION_NEW,
        SESSION_RESTORE,
        SESSION_DROP,
        DEFINE,
        START,
        RESUME,
        CANCEL,
        RESET,
        SNAPSHOT,
        SET_FUEL,
        FUEL,
        FUEL_WANTED,
        SET_FUEL_TOTAL,
        FUEL_TOTAL,
        PHASE,
        SET_DEADLINE,
        SET_HINT,
        SET_TEXT_CUT,
        SET_LOCAL_TIMEZONE,
        CALL_LOG,
        ENVIRONMENT,
        STUBS,
        HOST_MODULES,
        SET_MODULE_DUNDERS,
        SET_INT_BITS_TEXT,
        SET_GC_STRESS,
        PENDING,
        "memory",
    ];
}

/// The functions the guest imports from [`IMPORT_MODULE`]. Pointers are
/// guest addresses (`i32`), `s` the session id.
pub mod import {
    /// `(s) -> i64`: the host's monotonic clock in nanoseconds, for
    /// deadlines.
    pub const NOW_NS: &str = "now_ns";
    /// `(s, clear: u32) -> u32`: whether a park of `s` is requested;
    /// `clear` takes the request.
    pub const PARK: &str = "park";
    /// `(s) -> f64`: seconds since the Unix epoch (`time.time()`).
    pub const WALL_CLOCK: &str = "wall_clock";
    /// `(s, ptr, len)`: fills `len` bytes at `ptr` with randomness.
    pub const ENTROPY: &str = "entropy";
    /// `(s, stream: u32, ptr, len)`: output in production order, UTF-8;
    /// stream 0 is stdout, 1 stderr.
    pub const OUTPUT: &str = "output";
    /// `(s, key_ptr, key_len, buf_ptr, buf_cap) -> i64`: zone `key`'s `TZif`
    /// data for `zoneinfo.ZoneInfo`. The host returns `-1` for an unknown
    /// key, else the data's full length, and writes the data at `buf_ptr`
    /// when it fits in `buf_cap` bytes. When it does not, the guest calls
    /// again with a buffer of that length, so the answer must not change
    /// between the two calls.
    pub const ZONE: &str = "zone";
    /// `(region, offset, ptr, len) -> u32`, imported only by the bundle
    /// guest (`mm-wasm-guest` built with `host-bundle`): copies `len`
    /// bytes from `offset` of bundle region `region` (0 the module's
    /// [`super::section::PRELUDE`] custom section, 1 its
    /// [`super::section::STDLIB`]) to `ptr`; 1 if it could, else 0. A host
    /// without it runs the default guest, which links the bytecode in.
    pub const BUNDLE_READ: &str = "bundle_read";
}

/// Custom sections of the bundle guest: the bytecode `mm_host.bundle_read`
/// serves. A host reads them once per loaded module.
pub mod section {
    /// Region 0: the prelude's packed chunks.
    pub const PRELUDE: &str = "mm.prelude";
    /// Region 1: the bundled modules' packed code.
    pub const STDLIB: &str = "mm.stdlib";
}

/// Events: what `mm_start` and `mm_resume` return; the output holds the
/// payload.
pub mod event {
    /// `Progress::Complete`.
    pub const COMPLETE: u32 = 0;
    /// `Progress::HostCall`.
    pub const HOST_CALL: u32 = 1;
    /// `Progress::Waiting`.
    pub const WAITING: u32 = 2;
    /// `Progress::Parked`.
    pub const PARKED: u32 = 3;
    /// `Progress::Error` (an uncaught exception).
    pub const ERROR: u32 = 4;
    /// `start`: the source did not compile ([`super::put_compile_error`]).
    pub const COMPILE_ERROR: u32 = 5;
    /// `resume`: the answer was refused ([`super::put_resume_error`]).
    pub const RESUME_ERROR: u32 = 6;
    /// The request itself was bad (no such session, undecodable input); a
    /// string says what.
    pub const BAD_REQUEST: u32 = 7;
}

/// Status of the calls that are not events: `OK`, or an error whose
/// payload is in the output.
pub mod status {
    /// Success.
    pub const OK: u32 = 0;
    /// A `PyException` (`reset`, `set_hint`, `set_module_dunders`).
    pub const PY_ERROR: u32 = 1;
    /// A `SnapshotError` (`snapshot`).
    pub const SNAPSHOT_ERROR: u32 = 2;
    /// A `DefineError` (`define`, [`super::put_define_error`]).
    pub const DEFINE_ERROR: u32 = 3;
    /// As [`super::event::BAD_REQUEST`].
    pub const BAD_REQUEST: u32 = 7;
}

/// `mm_phase` codes, as [`super::put_resume_error`] encodes a phase too.
pub mod phase {
    /// `Phase::Idle`.
    pub const IDLE: u32 = 0;
    /// `Phase::HostCall`.
    pub const HOST_CALL: u32 = 1;
    /// `Phase::Waiting`.
    pub const WAITING: u32 = 2;
    /// `Phase::Parked`.
    pub const PARKED: u32 = 3;
}

/// The [`phase`] code of `p`; [`RESERVED`] for a phase this version does
/// not know.
#[must_use]
pub fn phase_code(p: Phase) -> u32 {
    match p {
        Phase::Idle => phase::IDLE,
        Phase::HostCall => phase::HOST_CALL,
        Phase::Waiting => phase::WAITING,
        Phase::Parked => phase::PARKED,
        _ => u32::from(RESERVED),
    }
}

/// The phase of a [`phase`] code.
///
/// # Errors
///
/// An unknown code.
pub fn phase_from_code(code: u32) -> Result<Phase, Bad> {
    Ok(match code {
        phase::IDLE => Phase::Idle,
        phase::HOST_CALL => Phase::HostCall,
        phase::WAITING => Phase::Waiting,
        phase::PARKED => Phase::Parked,
        _ => return Err(Bad("phase")),
    })
}

/// `mm_set_text_cut` codes.
pub mod text_cut {
    /// `TextCut::Head`.
    pub const HEAD: u32 = 0;
    /// `TextCut::HeadTail`.
    pub const HEAD_TAIL: u32 = 1;
}

/// The [`text_cut`] code of `cut`; [`RESERVED`] for a cut this version
/// does not know.
#[must_use]
pub fn text_cut_code(cut: TextCut) -> u32 {
    match cut {
        TextCut::Head => text_cut::HEAD,
        TextCut::HeadTail => text_cut::HEAD_TAIL,
        _ => u32::from(RESERVED),
    }
}

/// The `TextCut` of a [`text_cut`] code.
///
/// # Errors
///
/// An unknown code.
pub fn text_cut_from_code(code: u32) -> Result<TextCut, Bad> {
    match code {
        text_cut::HEAD => Ok(TextCut::Head),
        text_cut::HEAD_TAIL => Ok(TextCut::HeadTail),
        _ => Err(Bad("text cut")),
    }
}

/// The hint group whose [`HintGroup::key`] is `key`.
#[must_use]
pub fn hint_group(key: &str) -> Option<HintGroup> {
    HintGroup::ALL.iter().copied().find(|g| g.key() == key)
}

/// What the deadline clock import (`mm_host.now_ns`) is compared against
/// when the host has set no deadline: past any real clock, below the
/// sentinel the guest's clock returns for a park request (`u64::MAX`).
pub const ARMED: u64 = i64::MAX.cast_unsigned() - 1;

/// Undecodable input; the text says what was wrong.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bad(pub &'static str);

/// Deepest value nesting either side decodes (the engine caps
/// `HostArgs::max_depth` at 1000).
pub const MAX_DEPTH: u32 = 1000;

// ------------------------------------------------------------- primitives

/// Appends a byte.
pub fn put_u8(out: &mut Vec<u8>, v: u8) {
    out.push(v);
}

/// Appends a `u32`.
pub fn put_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

/// Appends a `u64`.
pub fn put_u64(out: &mut Vec<u8>, v: u64) {
    out.extend_from_slice(&v.to_le_bytes());
}

/// Appends a length or count (saturated at `u32::MAX`, which no decoder
/// accepts past the input's end).
pub fn put_len(out: &mut Vec<u8>, n: usize) {
    put_u32(out, u32::try_from(n).unwrap_or(u32::MAX));
}

/// Appends a byte string.
pub fn put_bytes(out: &mut Vec<u8>, b: &[u8]) {
    put_len(out, b.len());
    out.extend_from_slice(b);
}

/// Appends a string.
pub fn put_str(out: &mut Vec<u8>, s: &str) {
    put_bytes(out, s.as_bytes());
}

/// Appends a bool as `0` or `1`.
pub fn put_bool(out: &mut Vec<u8>, b: bool) {
    out.push(u8::from(b));
}

/// Appends an optional `u64`, `None` as `u64::MAX`.
pub fn put_opt_u64(out: &mut Vec<u8>, v: Option<u64>) {
    put_u64(out, v.unwrap_or(u64::MAX));
}

/// Appends an optional string.
pub fn put_opt_str(out: &mut Vec<u8>, s: Option<&str>) {
    put_bool(out, s.is_some());
    if let Some(s) = s {
        put_str(out, s);
    }
}

/// Appends optional bytes.
pub fn put_opt_bytes(out: &mut Vec<u8>, b: Option<&[u8]>) {
    put_bool(out, b.is_some());
    if let Some(b) = b {
        put_bytes(out, b);
    }
}

/// A cursor over input.
pub struct Reader<'a> {
    data: &'a [u8],
}

impl<'a> Reader<'a> {
    /// A reader at the start of `data`.
    #[must_use]
    pub fn new(data: &'a [u8]) -> Reader<'a> {
        Reader { data }
    }

    /// What is left.
    #[must_use]
    pub fn rest(&self) -> &'a [u8] {
        self.data
    }

    /// The next `n` bytes.
    ///
    /// # Errors
    ///
    /// Fewer than `n` bytes left.
    pub fn take(&mut self, n: usize) -> Result<&'a [u8], Bad> {
        if self.data.len() < n {
            return Err(Bad("truncated"));
        }
        let (head, tail) = self.data.split_at(n);
        self.data = tail;
        Ok(head)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], Bad> {
        let mut a = [0u8; N];
        a.copy_from_slice(self.take(N)?);
        Ok(a)
    }

    /// A byte.
    ///
    /// # Errors
    ///
    /// The input ended.
    pub fn u8(&mut self) -> Result<u8, Bad> {
        Ok(self.array::<1>()?[0])
    }

    /// A bool: `0` or `1`.
    ///
    /// # Errors
    ///
    /// The input ended, or another byte.
    pub fn bool(&mut self) -> Result<bool, Bad> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(Bad("bool")),
        }
    }

    /// A `u32`.
    ///
    /// # Errors
    ///
    /// The input ended.
    pub fn u32(&mut self) -> Result<u32, Bad> {
        Ok(u32::from_le_bytes(self.array()?))
    }

    /// A `u64`.
    ///
    /// # Errors
    ///
    /// The input ended.
    pub fn u64(&mut self) -> Result<u64, Bad> {
        Ok(u64::from_le_bytes(self.array()?))
    }

    /// An optional `u64` ([`put_opt_u64`]).
    ///
    /// # Errors
    ///
    /// The input ended.
    pub fn opt_u64(&mut self) -> Result<Option<u64>, Bad> {
        Ok(Some(self.u64()?).filter(|v| *v != u64::MAX))
    }

    /// A count of items, each at least `min` bytes: refuses counts the
    /// input cannot hold before anything is allocated.
    ///
    /// # Errors
    ///
    /// The input ended, or it cannot hold that many items.
    pub fn count(&mut self, min: usize) -> Result<usize, Bad> {
        let n = usize::try_from(self.u32()?).map_err(|_| Bad("count"))?;
        if n.saturating_mul(min.max(1)) > self.data.len() {
            return Err(Bad("count"));
        }
        Ok(n)
    }

    /// A byte string.
    ///
    /// # Errors
    ///
    /// The input ended.
    pub fn bytes(&mut self) -> Result<&'a [u8], Bad> {
        let n = usize::try_from(self.u32()?).map_err(|_| Bad("length"))?;
        self.take(n)
    }

    /// A string.
    ///
    /// # Errors
    ///
    /// The input ended, or the bytes are not UTF-8.
    pub fn string(&mut self) -> Result<String, Bad> {
        String::from_utf8(self.bytes()?.to_vec()).map_err(|_| Bad("utf-8"))
    }

    /// An optional string ([`put_opt_str`]).
    ///
    /// # Errors
    ///
    /// As [`Reader::string`].
    pub fn opt_string(&mut self) -> Result<Option<String>, Bad> {
        if self.bool()? {
            Ok(Some(self.string()?))
        } else {
            Ok(None)
        }
    }

    /// Optional bytes ([`put_opt_bytes`]).
    ///
    /// # Errors
    ///
    /// The input ended.
    pub fn opt_bytes(&mut self) -> Result<Option<&'a [u8]>, Bad> {
        if self.bool()? {
            Ok(Some(self.bytes()?))
        } else {
            Ok(None)
        }
    }
}

// ----------------------------------------------------------------- values

const V_NONE: u8 = 0;
const V_FALSE: u8 = 1;
const V_TRUE: u8 = 2;
const V_INT: u8 = 3;
const V_BIGINT: u8 = 4;
const V_FLOAT: u8 = 5;
const V_STR: u8 = 6;
const V_BYTES: u8 = 7;
const V_LIST: u8 = 8;
const V_TUPLE: u8 = 9;
const V_DICT: u8 = 10;
const V_RECORD: u8 = 11;
const V_OBJECT: u8 = 12;
const V_OPAQUE: u8 = 13;
const V_KEY_NOT_STR: u8 = 14;
const V_TOO_DEEP: u8 = 15;
const V_TOO_LARGE: u8 = 16;

fn put_fields(out: &mut Vec<u8>, fields: &[(String, HostValue)]) {
    put_len(out, fields.len());
    for (k, v) in fields {
        put_str(out, k);
        put_value(out, v);
    }
}

/// Appends a value: a tag byte (0 `None`, 1/2 `False`/`True`, 3 `int` as
/// `i64`, 4 big int as decimal text, 5 `float` bits, 6 `str`, 7 `bytes`,
/// 8 `list`, 9 `tuple`, 10 `dict`, 11 record, 12 host-class object,
/// 13–16 the `Unconverted` forms; [`RESERVED`] for a kind this version
/// does not know) and its payload. The recursion follows
/// the value, which the engine already bounded by `HostArgs::max_depth`
/// (or the host built).
pub fn put_value(out: &mut Vec<u8>, v: &HostValue) {
    match v {
        HostValue::None => put_u8(out, V_NONE),
        HostValue::Bool(false) => put_u8(out, V_FALSE),
        HostValue::Bool(true) => put_u8(out, V_TRUE),
        HostValue::Int(i) => {
            put_u8(out, V_INT);
            out.extend_from_slice(&i.to_le_bytes());
        }
        HostValue::BigInt(s) => {
            put_u8(out, V_BIGINT);
            put_str(out, s);
        }
        HostValue::Float(f) => {
            put_u8(out, V_FLOAT);
            put_u64(out, f.to_bits());
        }
        HostValue::Str(s) => {
            put_u8(out, V_STR);
            put_str(out, s);
        }
        HostValue::Bytes(b) => {
            put_u8(out, V_BYTES);
            put_bytes(out, b);
        }
        HostValue::List(items) | HostValue::Tuple(items) => {
            let tag = if matches!(v, HostValue::List(_)) {
                V_LIST
            } else {
                V_TUPLE
            };
            put_u8(out, tag);
            put_values(out, items);
        }
        HostValue::Dict(fields) => {
            put_u8(out, V_DICT);
            put_fields(out, fields);
        }
        HostValue::Record { name, fields }
        | HostValue::Object {
            class: name,
            fields,
        } => {
            let tag = if matches!(v, HostValue::Record { .. }) {
                V_RECORD
            } else {
                V_OBJECT
            };
            put_u8(out, tag);
            put_str(out, name);
            put_fields(out, fields);
        }
        HostValue::Unconverted(u) => match u {
            Unconverted::Opaque { type_name, repr } => {
                put_u8(out, V_OPAQUE);
                put_str(out, type_name);
                put_str(out, repr);
            }
            Unconverted::KeyNotStr => put_u8(out, V_KEY_NOT_STR),
            Unconverted::TooDeep => put_u8(out, V_TOO_DEEP),
            Unconverted::TooLarge { type_name } => {
                put_u8(out, V_TOO_LARGE);
                put_str(out, type_name);
            }
            _ => put_u8(out, RESERVED),
        },
        _ => put_u8(out, RESERVED),
    }
}

/// Items preallocated per container: each nesting level's count is checked
/// against the same remaining bytes, so a full preallocation at every level
/// would multiply them by the depth.
const PREALLOC: usize = 1024;

fn get_fields(r: &mut Reader<'_>, depth: u32) -> Result<Vec<(String, HostValue)>, Bad> {
    let n = r.count(5)?;
    let mut fields = Vec::with_capacity(n.min(PREALLOC));
    for _ in 0..n {
        let k = r.string()?;
        fields.push((k, get_value_at(r, depth)?));
    }
    Ok(fields)
}

/// A value ([`put_value`]).
///
/// # Errors
///
/// Truncated or malformed input, or nesting past [`MAX_DEPTH`].
pub fn get_value(r: &mut Reader<'_>) -> Result<HostValue, Bad> {
    get_value_at(r, 0)
}

fn get_value_at(r: &mut Reader<'_>, depth: u32) -> Result<HostValue, Bad> {
    // The engine's lenient conversion puts `Unconverted::TooDeep` one
    // level below `MAX_DEPTH`.
    if depth > MAX_DEPTH + 1 {
        return Err(Bad("value nested too deep"));
    }
    Ok(match r.u8()? {
        V_NONE => HostValue::None,
        V_FALSE => HostValue::Bool(false),
        V_TRUE => HostValue::Bool(true),
        V_INT => HostValue::Int(r.u64()?.cast_signed()),
        V_BIGINT => HostValue::BigInt(r.string()?),
        V_FLOAT => HostValue::Float(f64::from_bits(r.u64()?)),
        V_STR => HostValue::Str(r.string()?),
        V_BYTES => HostValue::Bytes(r.bytes()?.to_vec()),
        tag @ (V_LIST | V_TUPLE) => {
            let n = r.count(1)?;
            let mut items = Vec::with_capacity(n.min(PREALLOC));
            for _ in 0..n {
                items.push(get_value_at(r, depth + 1)?);
            }
            if tag == V_LIST {
                HostValue::List(items)
            } else {
                HostValue::Tuple(items)
            }
        }
        V_DICT => HostValue::Dict(get_fields(r, depth + 1)?),
        tag @ (V_RECORD | V_OBJECT) => {
            let name = r.string()?;
            let fields = get_fields(r, depth + 1)?;
            if tag == V_RECORD {
                HostValue::Record { name, fields }
            } else {
                HostValue::Object {
                    class: name,
                    fields,
                }
            }
        }
        V_OPAQUE => HostValue::Unconverted(Unconverted::Opaque {
            type_name: r.string()?,
            repr: r.string()?,
        }),
        V_KEY_NOT_STR => HostValue::Unconverted(Unconverted::KeyNotStr),
        V_TOO_DEEP => HostValue::Unconverted(Unconverted::TooDeep),
        V_TOO_LARGE => HostValue::Unconverted(Unconverted::TooLarge {
            type_name: r.string()?,
        }),
        _ => return Err(Bad("value tag")),
    })
}

fn put_values(out: &mut Vec<u8>, vs: &[HostValue]) {
    put_len(out, vs.len());
    for v in vs {
        put_value(out, v);
    }
}

fn get_values(r: &mut Reader<'_>) -> Result<Vec<HostValue>, Bad> {
    let n = r.count(1)?;
    (0..n).map(|_| get_value(r)).collect()
}

fn put_opt_value(out: &mut Vec<u8>, v: Option<&HostValue>) {
    put_bool(out, v.is_some());
    if let Some(v) = v {
        put_value(out, v);
    }
}

fn get_opt_value(r: &mut Reader<'_>) -> Result<Option<HostValue>, Bad> {
    if r.bool()? {
        Ok(Some(get_value(r)?))
    } else {
        Ok(None)
    }
}

// --------------------------------------------------------------- sessions

/// What `mm_session_new` and `mm_session_restore` take, as an options
/// record ([`put_options`]). A restored session takes the limits and the
/// host-side settings from it, and the rest (the text limit, the last
/// expression's value, host-call conversion) from the image.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent switches, one record entry each"
)]
pub struct SessionOptions {
    /// The session's limits.
    pub limits: Limits,
    /// `Session::set_text_limit`; `None` keeps the default (1 MiB, or the
    /// memory limit if less). New sessions only.
    pub text_limit: Option<u64>,
    /// `Session::set_last_expr_value`. New sessions only.
    pub last_expr_value: bool,
    /// `Session::set_host_args`. New sessions only.
    pub host_args: HostArgs,
    /// Install `mm_host.wall_clock` as the session's wall clock; without
    /// it, `time.time()` raises `RuntimeError`.
    pub wall_clock: bool,
    /// Install `mm_host.entropy` as the session's entropy; without it,
    /// `os.urandom` raises `RuntimeError`.
    pub entropy: bool,
    /// Install `mm_host.zone` as the session's zone source
    /// (`Session::set_zone_source`); without it, only `ZoneInfo('UTC')`
    /// and zones a restored image already loaded are found.
    pub zone_source: bool,
    /// `Session::set_text_cut`.
    pub text_cut: TextCut,
    /// `Session::set_int_bits_text`.
    pub int_bits_text: bool,
    /// `Session::set_gc_stress` (engine testing).
    pub gc_stress: bool,
}

impl SessionOptions {
    /// The defaults: what an empty record means ([`option`]). Spelled out
    /// rather than taken from the facade's defaults, which a release may
    /// change; the record's may not.
    #[must_use]
    pub const fn new() -> SessionOptions {
        SessionOptions {
            limits: Limits::new()
                .with_memory(256 << 20)
                .with_recursion_depth(1000)
                .with_fuel(None),
            text_limit: None,
            last_expr_value: true,
            host_args: HostArgs {
                max_bytes: 16 << 20,
                max_depth: 1000,
                lenient: false,
            },
            wall_clock: true,
            entropy: true,
            zone_source: true,
            text_cut: TextCut::Head,
            int_bits_text: false,
            gc_stress: false,
        }
    }

    /// These options with [`SessionOptions::limits`] set to `limits`.
    #[must_use]
    pub const fn with_limits(mut self, limits: Limits) -> SessionOptions {
        self.limits = limits;
        self
    }
}

impl Default for SessionOptions {
    fn default() -> Self {
        SessionOptions::new()
    }
}

/// The options record's tags ([`put_options`]). Each names one option,
/// with an encoding and a default that never change; an absent entry is
/// the default. A guest refuses a record with a tag it does not know,
/// unless the tag is [`IGNORABLE`].
pub mod option {
    /// Tags from here up name options a guest that does not know them
    /// skips; below, ones it must refuse the record for. Only an option a
    /// guest can safely run without (a hint, a diagnostic) may take a tag
    /// from here up; anything that limits or changes behaviour (a limit,
    /// the clock, entropy, output, the text cut) takes one below, so a
    /// guest that predates it refuses the record rather than run without
    /// it. None is assigned yet.
    pub const IGNORABLE: u8 = 0x80;
    /// `u64`: `Limits::memory`. Default 256 MiB.
    pub const MEMORY: u8 = 0x01;
    /// `u32`: `Limits::recursion_depth`. Default 1000.
    pub const RECURSION_DEPTH: u8 = 0x02;
    /// `u64`: `Limits::fuel`, `u64::MAX` unlimited. Default unlimited.
    pub const FUEL: u8 = 0x03;
    /// `u64`: `Session::set_text_limit`. Default 1 MiB, or the memory limit
    /// if less.
    pub const TEXT_LIMIT: u8 = 0x04;
    /// bool: `Session::set_last_expr_value`. Default on.
    pub const LAST_EXPR_VALUE: u8 = 0x05;
    /// `u64`: `HostArgs::max_bytes`. Default 16 MiB.
    pub const HOST_ARGS_MAX_BYTES: u8 = 0x06;
    /// `u32`: `HostArgs::max_depth`. Default 1000.
    pub const HOST_ARGS_MAX_DEPTH: u8 = 0x07;
    /// bool: `HostArgs::lenient`. Default off.
    pub const HOST_ARGS_LENIENT: u8 = 0x08;
    /// bool: the `mm_host.wall_clock` import. Default on.
    pub const WALL_CLOCK: u8 = 0x09;
    /// bool: the `mm_host.entropy` import. Default on.
    pub const ENTROPY: u8 = 0x0a;
    /// bool: the `mm_host.zone` import. Default on.
    pub const ZONE_SOURCE: u8 = 0x0b;
    /// `u8`, a [`super::text_cut`] code: `Session::set_text_cut`. Default
    /// head.
    pub const TEXT_CUT: u8 = 0x0c;
    /// bool: `Session::set_int_bits_text`. Default off.
    pub const INT_BITS_TEXT: u8 = 0x0d;
    /// bool: `Session::set_gc_stress`. Default off.
    pub const GC_STRESS: u8 = 0x0e;
}

/// Appends the options record: a `u32` byte length, then an entry for
/// each option that differs from its default, in [`option`] tag order: the
/// tag, a `u32` payload length and the payload. A guest older than an
/// option therefore sees its entry only when the host uses it.
pub fn put_options(out: &mut Vec<u8>, o: &SessionOptions) {
    let d = SessionOptions::new();
    let mut e = Vec::new();
    let mut entry = |tag: u8, payload: &[u8]| {
        put_u8(&mut e, tag);
        put_bytes(&mut e, payload);
    };
    if o.limits.memory != d.limits.memory {
        entry(option::MEMORY, &o.limits.memory.to_le_bytes());
    }
    if o.limits.recursion_depth != d.limits.recursion_depth {
        entry(
            option::RECURSION_DEPTH,
            &o.limits.recursion_depth.to_le_bytes(),
        );
    }
    if o.limits.fuel != d.limits.fuel {
        entry(
            option::FUEL,
            &o.limits.fuel.unwrap_or(u64::MAX).to_le_bytes(),
        );
    }
    if let Some(t) = o.text_limit {
        entry(option::TEXT_LIMIT, &t.to_le_bytes());
    }
    if o.last_expr_value != d.last_expr_value {
        entry(option::LAST_EXPR_VALUE, &[u8::from(o.last_expr_value)]);
    }
    if o.host_args.max_bytes != d.host_args.max_bytes {
        entry(
            option::HOST_ARGS_MAX_BYTES,
            &o.host_args.max_bytes.to_le_bytes(),
        );
    }
    if o.host_args.max_depth != d.host_args.max_depth {
        entry(
            option::HOST_ARGS_MAX_DEPTH,
            &o.host_args.max_depth.to_le_bytes(),
        );
    }
    if o.host_args.lenient != d.host_args.lenient {
        entry(option::HOST_ARGS_LENIENT, &[u8::from(o.host_args.lenient)]);
    }
    if o.wall_clock != d.wall_clock {
        entry(option::WALL_CLOCK, &[u8::from(o.wall_clock)]);
    }
    if o.entropy != d.entropy {
        entry(option::ENTROPY, &[u8::from(o.entropy)]);
    }
    if o.zone_source != d.zone_source {
        entry(option::ZONE_SOURCE, &[u8::from(o.zone_source)]);
    }
    if o.text_cut != d.text_cut {
        let code = u8::try_from(text_cut_code(o.text_cut)).unwrap_or(RESERVED);
        entry(option::TEXT_CUT, &[code]);
    }
    if o.int_bits_text != d.int_bits_text {
        entry(option::INT_BITS_TEXT, &[u8::from(o.int_bits_text)]);
    }
    if o.gc_stress != d.gc_stress {
        entry(option::GC_STRESS, &[u8::from(o.gc_stress)]);
    }
    put_bytes(out, &e);
}

/// The options record ([`put_options`]). Entries may come in any order.
///
/// # Errors
///
/// A truncated record, a repeated tag, a payload that is not exactly its
/// tag's encoding, or a tag this version does not know that is not
/// [`option::IGNORABLE`].
pub fn get_options(r: &mut Reader<'_>) -> Result<SessionOptions, Bad> {
    let mut entries = Reader::new(r.bytes()?);
    let mut o = SessionOptions::new();
    let mut seen = [false; 256];
    while !entries.rest().is_empty() {
        let tag = entries.u8()?;
        let mut p = Reader::new(entries.bytes()?);
        if core::mem::replace(&mut seen[usize::from(tag)], true) {
            return Err(Bad("repeated option"));
        }
        match tag {
            option::MEMORY => o.limits.memory = p.u64()?,
            option::RECURSION_DEPTH => o.limits.recursion_depth = p.u32()?,
            option::FUEL => o.limits.fuel = p.opt_u64()?,
            option::TEXT_LIMIT => o.text_limit = Some(p.u64()?),
            option::LAST_EXPR_VALUE => o.last_expr_value = p.bool()?,
            option::HOST_ARGS_MAX_BYTES => o.host_args.max_bytes = p.u64()?,
            option::HOST_ARGS_MAX_DEPTH => o.host_args.max_depth = p.u32()?,
            option::HOST_ARGS_LENIENT => o.host_args.lenient = p.bool()?,
            option::WALL_CLOCK => o.wall_clock = p.bool()?,
            option::ENTROPY => o.entropy = p.bool()?,
            option::ZONE_SOURCE => o.zone_source = p.bool()?,
            option::TEXT_CUT => o.text_cut = text_cut_from_code(u32::from(p.u8()?))?,
            option::INT_BITS_TEXT => o.int_bits_text = p.bool()?,
            option::GC_STRESS => o.gc_stress = p.bool()?,
            t if t >= option::IGNORABLE => continue,
            _ => return Err(Bad("unknown option")),
        }
        if !p.rest().is_empty() {
            return Err(Bad("option length"));
        }
    }
    Ok(o)
}

// ----------------------------------------------------------------- define

fn put_signature(out: &mut Vec<u8>, s: &HostSignature) {
    put_str(out, &s.name);
    put_len(out, s.params.len());
    for p in &s.params {
        put_str(out, &p.name);
        put_str(out, &p.annotation);
        put_opt_value(out, p.default.as_ref());
        put_u8(
            out,
            match p.kind {
                ParamKind::PositionalOrKeyword => 0,
                ParamKind::KeywordOnly => 1,
                ParamKind::VarPositional => 2,
                ParamKind::VarKeyword => 3,
                _ => RESERVED,
            },
        );
    }
    put_str(out, &s.returns);
    put_str(out, &s.doc);
    put_bool(out, s.dict_as_kwargs);
}

fn get_signature(r: &mut Reader<'_>) -> Result<HostSignature, Bad> {
    let name = r.string()?;
    let n = r.count(10)?;
    let mut params = Vec::with_capacity(n);
    for _ in 0..n {
        let name = r.string()?;
        let annotation = r.string()?;
        let default = get_opt_value(r)?;
        let kind = match r.u8()? {
            0 => ParamKind::PositionalOrKeyword,
            1 => ParamKind::KeywordOnly,
            2 => ParamKind::VarPositional,
            3 => ParamKind::VarKeyword,
            _ => return Err(Bad("param kind")),
        };
        params.push(HostParam {
            name,
            annotation,
            default,
            kind,
        });
    }
    Ok(HostSignature {
        name,
        params,
        returns: r.string()?,
        doc: r.string()?,
        dict_as_kwargs: r.bool()?,
    })
}

fn get_kind(r: &mut Reader<'_>) -> Result<HostErrorKind, Bad> {
    HostErrorKind::from_name(&r.string()?).ok_or(Bad("host error kind"))
}

/// Appends a host module: name, functions, classes, constants and
/// exception classes (each its name, its base's `HostErrorKind::name` and
/// its doc).
pub fn put_module(out: &mut Vec<u8>, m: &HostModule) {
    put_str(out, &m.name);
    put_len(out, m.functions.len());
    for f in &m.functions {
        put_signature(out, f);
    }
    put_len(out, m.classes.len());
    for c in &m.classes {
        put_str(out, &c.name);
        put_len(out, c.fields.len());
        for (k, a) in &c.fields {
            put_str(out, k);
            put_str(out, a);
        }
        put_len(out, c.methods.len());
        for f in &c.methods {
            put_signature(out, f);
        }
        put_str(out, &c.doc);
    }
    put_fields(out, &m.constants);
    put_len(out, m.exceptions.len());
    for e in &m.exceptions {
        put_str(out, &e.name);
        put_str(out, e.base.name());
        put_str(out, &e.doc);
    }
}

/// A host module ([`put_module`]).
///
/// # Errors
///
/// Truncated or malformed input, or an unknown exception base.
pub fn get_module(r: &mut Reader<'_>) -> Result<HostModule, Bad> {
    let name = r.string()?;
    let n = r.count(17)?;
    let functions = (0..n).map(|_| get_signature(r)).collect::<Result<_, _>>()?;
    let n = r.count(16)?;
    let mut classes = Vec::with_capacity(n);
    for _ in 0..n {
        let name = r.string()?;
        let k = r.count(8)?;
        let fields = (0..k)
            .map(|_| Ok((r.string()?, r.string()?)))
            .collect::<Result<_, Bad>>()?;
        let k = r.count(17)?;
        let methods = (0..k).map(|_| get_signature(r)).collect::<Result<_, _>>()?;
        classes.push(HostClass {
            name,
            fields,
            methods,
            doc: r.string()?,
        });
    }
    let constants = get_fields(r, 0)?;
    let n = r.count(12)?;
    let exceptions = (0..n)
        .map(|_| {
            Ok(HostException {
                name: r.string()?,
                base: get_kind(r)?,
                doc: r.string()?,
            })
        })
        .collect::<Result<_, Bad>>()?;
    Ok(HostModule {
        name,
        functions,
        classes,
        constants,
        exceptions,
    })
}

/// Appends what `define` returned.
pub fn put_function_refs(out: &mut Vec<u8>, refs: &[HostFunctionRef]) {
    put_len(out, refs.len());
    for f in refs {
        put_str(out, &f.name);
        put_u32(out, f.id);
    }
}

/// What `define` returned ([`put_function_refs`]).
///
/// # Errors
///
/// Truncated or malformed input.
pub fn get_function_refs(r: &mut Reader<'_>) -> Result<Vec<HostFunctionRef>, Bad> {
    let n = r.count(8)?;
    (0..n)
        .map(|_| {
            let name = r.string()?;
            Ok(HostFunctionRef::new(name, r.u32()?))
        })
        .collect()
}

// ----------------------------------------------------- queries on a session

const CALL_STATUS: [CallStatus; 4] = [
    CallStatus::Returned,
    CallStatus::Raised,
    CallStatus::Pending,
    CallStatus::Abandoned,
];

/// Appends a call log: each record's call id, function id, name and status
/// (0 returned, 1 raised, 2 pending, 3 abandoned, [`RESERVED`] for one
/// this version does not know).
pub fn put_call_log(out: &mut Vec<u8>, log: &[HostCallRecord]) {
    put_len(out, log.len());
    for c in log {
        put_u64(out, c.call);
        put_u32(out, c.function);
        put_str(out, &c.name);
        let code = CALL_STATUS.iter().position(|s| *s == c.status);
        put_u8(
            out,
            code.and_then(|i| u8::try_from(i).ok()).unwrap_or(RESERVED),
        );
    }
}

/// A call log ([`put_call_log`]).
///
/// # Errors
///
/// Truncated or malformed input.
pub fn get_call_log(r: &mut Reader<'_>) -> Result<Vec<HostCallRecord>, Bad> {
    let n = r.count(17)?;
    (0..n)
        .map(|_| {
            Ok(HostCallRecord::new(
                r.u64()?,
                r.u32()?,
                r.string()?,
                *CALL_STATUS
                    .get(usize::from(r.u8()?))
                    .ok_or(Bad("call status"))?,
            ))
        })
        .collect()
}

/// Appends `Session::host_modules`: a count, then each module
/// ([`put_module`]).
pub fn put_modules(out: &mut Vec<u8>, modules: &[HostModule]) {
    put_len(out, modules.len());
    for m in modules {
        put_module(out, m);
    }
}

/// `Session::host_modules` ([`put_modules`]).
///
/// # Errors
///
/// Truncated or malformed input.
pub fn get_modules(r: &mut Reader<'_>) -> Result<Vec<HostModule>, Bad> {
    let n = r.count(20)?;
    (0..n).map(|_| get_module(r)).collect()
}

/// Appends `Session::stubs`: (module name, stub text) pairs.
pub fn put_stubs(out: &mut Vec<u8>, stubs: &[(String, String)]) {
    put_len(out, stubs.len());
    for (name, text) in stubs {
        put_str(out, name);
        put_str(out, text);
    }
}

/// `Session::stubs` ([`put_stubs`]).
///
/// # Errors
///
/// Truncated or malformed input.
pub fn get_stubs(r: &mut Reader<'_>) -> Result<Vec<(String, String)>, Bad> {
    let n = r.count(8)?;
    (0..n).map(|_| Ok((r.string()?, r.string()?))).collect()
}

// ------------------------------------------------------- resume, progress

/// Appends a host error: kind name, message, optional data, optional class.
pub fn put_host_error(out: &mut Vec<u8>, e: &HostError) {
    put_str(out, e.kind.name());
    put_str(out, &e.message);
    put_opt_value(out, e.data.as_ref());
    put_opt_str(out, e.class.as_deref());
}

/// A host error ([`put_host_error`]).
///
/// # Errors
///
/// Truncated or malformed input, or an unknown kind.
pub fn get_host_error(r: &mut Reader<'_>) -> Result<HostError, Bad> {
    let kind = get_kind(r)?;
    let message = r.string()?;
    Ok(HostError {
        kind,
        message,
        data: get_opt_value(r)?,
        class: r.opt_string()?,
    })
}

/// Appends a `Resume`: tag 0 continue, 1 value, 2 error, 3 future, 4 batch
/// (each item its call id, then `0` and a value or `1` and an error).
pub fn put_resume(out: &mut Vec<u8>, resume: &Resume) {
    match resume {
        Resume::Continue => put_u8(out, 0),
        Resume::Value(v) => {
            put_u8(out, 1);
            put_value(out, v);
        }
        Resume::Error(e) => {
            put_u8(out, 2);
            put_host_error(out, e);
        }
        Resume::Future => put_u8(out, 3),
        Resume::Batch(items) => {
            put_u8(out, 4);
            put_len(out, items.len());
            for (call, result) in items {
                put_u64(out, *call);
                match result {
                    Ok(v) => {
                        put_u8(out, 0);
                        put_value(out, v);
                    }
                    Err(e) => {
                        put_u8(out, 1);
                        put_host_error(out, e);
                    }
                }
            }
        }
        _ => put_u8(out, RESERVED),
    }
}

/// A `Resume` ([`put_resume`]).
///
/// # Errors
///
/// Truncated or malformed input.
pub fn get_resume(r: &mut Reader<'_>) -> Result<Resume, Bad> {
    Ok(match r.u8()? {
        0 => Resume::Continue,
        1 => Resume::Value(get_value(r)?),
        2 => Resume::Error(get_host_error(r)?),
        3 => Resume::Future,
        4 => {
            let n = r.count(10)?;
            let mut items = Vec::with_capacity(n);
            for _ in 0..n {
                let call = r.u64()?;
                let result = if r.bool()? {
                    Err(get_host_error(r)?)
                } else {
                    Ok(get_value(r)?)
                };
                items.push((call, result));
            }
            Resume::Batch(items)
        }
        _ => return Err(Bad("resume tag")),
    })
}

/// Appends an exception: type name, message, traceback, whether they were
/// cut ([`PyException::truncated`]).
pub fn put_exception(out: &mut Vec<u8>, e: &PyException) {
    put_str(out, &e.type_name);
    put_str(out, &e.message);
    put_str(out, &e.traceback);
    put_bool(out, e.truncated);
}

/// An exception ([`put_exception`]).
///
/// # Errors
///
/// Truncated or malformed input.
pub fn get_exception(r: &mut Reader<'_>) -> Result<PyException, Bad> {
    let type_name = r.string()?;
    let message = r.string()?;
    let traceback = r.string()?;
    Ok(PyException::new(type_name, message, traceback, r.bool()?))
}

/// Appends a compile error.
pub fn put_compile_error(out: &mut Vec<u8>, e: &CompileError) {
    put_str(out, &e.type_name);
    put_str(out, &e.message);
    put_u32(out, e.line);
    put_u32(out, e.column);
    put_str(out, &e.rendered);
}

/// A compile error ([`put_compile_error`]).
///
/// # Errors
///
/// Truncated or malformed input.
pub fn get_compile_error(r: &mut Reader<'_>) -> Result<CompileError, Bad> {
    let type_name = r.string()?;
    let message = r.string()?;
    let line = r.u32()?;
    let column = r.u32()?;
    Ok(CompileError::new(
        type_name,
        message,
        line,
        column,
        r.string()?,
    ))
}

/// Appends a resume error: 0 unexpected (with the [`phase`] code),
/// 1 unknown call, 2 already settled (each with the call id).
pub fn put_resume_error(out: &mut Vec<u8>, e: &ResumeError) {
    match e {
        ResumeError::Unexpected { phase } => {
            put_u8(out, 0);
            put_u8(out, u8::try_from(phase_code(*phase)).unwrap_or(RESERVED));
        }
        ResumeError::UnknownCall { call } => {
            put_u8(out, 1);
            put_u64(out, *call);
        }
        ResumeError::AlreadySettled { call } => {
            put_u8(out, 2);
            put_u64(out, *call);
        }
        _ => put_u8(out, RESERVED),
    }
}

/// A resume error ([`put_resume_error`]).
///
/// # Errors
///
/// Truncated or malformed input, or an unknown tag or phase.
pub fn get_resume_error(r: &mut Reader<'_>) -> Result<ResumeError, Bad> {
    Ok(match r.u8()? {
        0 => ResumeError::Unexpected {
            phase: phase_from_code(u32::from(r.u8()?))?,
        },
        1 => ResumeError::UnknownCall { call: r.u64()? },
        2 => ResumeError::AlreadySettled { call: r.u64()? },
        _ => return Err(Bad("resume error tag")),
    })
}

/// Appends a snapshot error: 0 incompatible (the image's format as a
/// `u32`, then its 32-byte build id), 1 corrupt (with what), 2 too large,
/// 3 busy, 4 poisoned, 5 internal (with the message).
pub fn put_snapshot_error(out: &mut Vec<u8>, e: &SnapshotError) {
    match e {
        SnapshotError::Incompatible { format, build_id } => {
            put_u8(out, 0);
            put_u32(out, u32::from(*format));
            out.extend_from_slice(build_id);
        }
        SnapshotError::Corrupt { what } => {
            put_u8(out, 1);
            put_str(out, what);
        }
        SnapshotError::TooLarge => put_u8(out, 2),
        SnapshotError::Busy => put_u8(out, 3),
        SnapshotError::Poisoned => put_u8(out, 4),
        SnapshotError::Internal { message } => {
            put_u8(out, 5);
            put_str(out, message);
        }
        _ => put_u8(out, RESERVED),
    }
}

/// A snapshot error ([`put_snapshot_error`]).
///
/// # Errors
///
/// Truncated or malformed input, or an unknown tag.
pub fn get_snapshot_error(r: &mut Reader<'_>) -> Result<SnapshotError, Bad> {
    Ok(match r.u8()? {
        0 => {
            let format = u16::try_from(r.u32()?).map_err(|_| Bad("snapshot format"))?;
            let id = r.take(32)?;
            let mut build_id = [0; 32];
            build_id.copy_from_slice(id);
            SnapshotError::Incompatible { format, build_id }
        }
        1 => SnapshotError::Corrupt { what: r.string()? },
        2 => SnapshotError::TooLarge,
        3 => SnapshotError::Busy,
        4 => SnapshotError::Poisoned,
        5 => SnapshotError::Internal {
            message: r.string()?,
        },
        _ => return Err(Bad("snapshot error tag")),
    })
}

/// Appends a define error: 0 invalid (with the text), 1 busy, 2 raised
/// (with the exception, [`put_exception`]), 3 poisoned (with the text).
pub fn put_define_error(out: &mut Vec<u8>, e: &DefineError) {
    match e {
        DefineError::Invalid(text) => {
            put_u8(out, 0);
            put_str(out, text);
        }
        DefineError::Busy => put_u8(out, 1),
        DefineError::Raised(e) => {
            put_u8(out, 2);
            put_exception(out, e);
        }
        DefineError::Poisoned(text) => {
            put_u8(out, 3);
            put_str(out, text);
        }
        _ => put_u8(out, RESERVED),
    }
}

/// A define error ([`put_define_error`]).
///
/// # Errors
///
/// Truncated or malformed input, or an unknown tag.
pub fn get_define_error(r: &mut Reader<'_>) -> Result<DefineError, Bad> {
    Ok(match r.u8()? {
        0 => DefineError::Invalid(r.string()?),
        1 => DefineError::Busy,
        2 => DefineError::Raised(get_exception(r)?),
        3 => DefineError::Poisoned(r.string()?),
        _ => return Err(Bad("define error tag")),
    })
}

/// Appends a progress' payload and returns its [`event`] code.
#[must_use]
pub fn put_progress(out: &mut Vec<u8>, p: &Progress) -> u32 {
    match p {
        Progress::Complete(c) => {
            put_bool(out, c.is_some());
            if let Some(c) = c {
                put_str(out, &c.repr);
                put_str(out, &c.str);
                put_str(out, &c.type_name);
                put_bool(out, c.repr_truncated);
                put_bool(out, c.str_truncated);
                put_opt_value(out, c.value.as_ref());
            }
            event::COMPLETE
        }
        Progress::HostCall {
            call,
            function,
            name,
            args,
            kwargs,
        } => {
            put_u64(out, *call);
            put_u32(out, *function);
            put_str(out, name);
            put_values(out, args);
            put_fields(out, kwargs);
            event::HOST_CALL
        }
        Progress::Waiting { calls } => {
            put_len(out, calls.len());
            for c in calls {
                put_u64(out, *c);
            }
            event::WAITING
        }
        Progress::Parked { reason } => {
            put_u8(
                out,
                match reason {
                    ParkReason::Fuel => 0,
                    ParkReason::Requested => 1,
                    ParkReason::Deadline => 2,
                    _ => RESERVED,
                },
            );
            event::PARKED
        }
        Progress::Error(e) => {
            put_exception(out, e);
            event::ERROR
        }
        _ => {
            put_str(out, "an event this ABI version does not know");
            event::BAD_REQUEST
        }
    }
}

/// The inverse of [`put_progress`] for the progress events.
///
/// # Errors
///
/// Truncated or malformed input, or `code` is not a progress event.
pub fn get_progress(code: u32, r: &mut Reader<'_>) -> Result<Progress, Bad> {
    Ok(match code {
        event::COMPLETE => Progress::Complete(if r.bool()? {
            let repr = r.string()?;
            let str = r.string()?;
            let type_name = r.string()?;
            let repr_truncated = r.bool()?;
            let str_truncated = r.bool()?;
            Some(Completion::new(
                repr,
                str,
                type_name,
                get_opt_value(r)?,
                repr_truncated,
                str_truncated,
            ))
        } else {
            None
        }),
        event::HOST_CALL => Progress::HostCall {
            call: r.u64()?,
            function: r.u32()?,
            name: r.string()?,
            args: get_values(r)?,
            kwargs: get_fields(r, 0)?,
        },
        event::WAITING => {
            let n = r.count(8)?;
            Progress::Waiting {
                calls: (0..n).map(|_| r.u64()).collect::<Result<_, _>>()?,
            }
        }
        event::PARKED => Progress::Parked {
            reason: match r.u8()? {
                0 => ParkReason::Fuel,
                1 => ParkReason::Requested,
                2 => ParkReason::Deadline,
                _ => return Err(Bad("park reason")),
            },
        },
        event::ERROR => Progress::Error(get_exception(r)?),
        _ => return Err(Bad("not a progress event")),
    })
}
