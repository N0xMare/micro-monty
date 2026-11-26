//! FULL snapshots of a session (docs/spec.md §9.3).
//!
//! [`Vm::write_snapshot`] collects nothing and writes the session exactly
//! as it is: the heap (mm-core's segment form, garbage, free blocks and the
//! collector's state included), the sources and code table, every VM
//! register and the VM's side of the collector's schedule: the
//! frame stack and continuations, the scheduler's tasks, run queue and host
//! futures, the pending host call, globals, modules, shapes, host modules
//! and limits, and the capacities of the tables the memory limit charges.
//! Inline caches, constructor caches and the builtin's consumed iterators
//! are not written. Host-owned state (the output callback, the
//! clock and deadline, the park flag) is not part of a session's image.
//!
//! [`Vm::read_snapshot`] trusts nothing: every count is checked against the
//! remaining input before anything is allocated, every value must be a
//! valid encoding, user code must pass [`Code::verify`], and every pc, frame
//! base, code index, continuation, task and register range is checked
//! against the code and stacks it indexes. Every arena access is
//! bounds-checked and validated against the object-start bitmap, but the
//! engine trusts a reference's kind to name its object's type (it reads a
//! generator's yield pc from whatever a `GEN` value names), so every
//! reference, in the VM part and in the arena's value slots, must name an
//! object of its kind. Over the restored heap, each generator's code is its
//! function's, each suspended generator's yield pc and coroutine's pc is
//! checked, and each dict's index table against its entries; each frame of
//! a generator or coroutine runs that object's code while it runs, and each
//! continuation slot has one owner (a frame or the free list). A linear
//! dict uses no more entries than it holds, and each chunked native's state
//! keeps the words its next step trusts as an index, a stride, a count or
//! a slice's bounds in range, and a str build's bytes UTF-8 with the chars
//! it counts (`check_steps`); a finished state, whose words its last step
//! did not save, is marked done and not checked. The engine bounds what it
//! builds from other arena words (a class's size hint is clamped to the
//! inline slots).

use crate::class::Shape;
use crate::cont::Cont;
use crate::host::{HostArgs, HostModule, HostValue, MAX_HOST_DEPTH_CEILING, Unconverted};
use crate::interp::walk_frames;
use crate::types::{NTYPES, ty};
use crate::vm::{CodeObj, Frame, HDR, Post, Ready, Source, State, Task, Vm};
use alloc::{collections::VecDeque, string::String, sync::Arc, vec, vec::Vec};
use core::{ops::RangeInclusive, sync::atomic::AtomicBool};
use mm_compile::bytecode::{Code, Konst, op};
use mm_core::{Heap, Reader, Sink, Value, put_bytes, put_len, put_u8, put_u32, put_u64};

/// Why a snapshot cannot be read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SnapError {
    /// Truncated or inconsistent; names the part that is.
    Corrupt(&'static str),
    /// Its arena or retained code needs more memory than the limit allows.
    TooLarge,
}

/// Why a session cannot be written now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NotAtSafepoint;

/// Code-table entry kinds.
const CODE_TRAMPOLINE: u8 = 0;
const CODE_ENCODED: u8 = 2;
const CODE_STDLIB: u8 = 3;
const CODE_HOST: u8 = 4;
const CODE_PRELUDE: u8 = 5;

/// The engine's layout epoch, part of the snapshot build id
/// (`micro_monty::build_id`, docs/spec.md §9.3). The id hashes this file,
/// mm-core's heap codec and mm-compile's code codec whole, and every
/// `const` and `#[repr]` item of the three crates, but no other function
/// body. Bump the epoch when a function body elsewhere changes what an
/// image holds or how it is read, with none of those changed:
/// - what a raw word or value slot of an existing NSTATE step holds
///   (`nstep::STEPS` hashes only names and counts), or what an existing
///   `Cont` field or stage means;
/// - a layout written as literal slot or word numbers in code (`list`,
///   `dict`, `set`, `range`, iterators, ints, `bytes`, `bytearray`, the
///   regex matcher's saved run), or the hash, probing or order of dict and
///   set tables;
/// - what an opcode's operands mean, with the same format;
/// - how frames, generators or coroutines keep their state.
pub(crate) const SNAPSHOT_EPOCH: u32 = 21;

/// This build's engine fingerprint for snapshot build ids: what an image
/// written by this engine means. The layout epoch; the codecs and every
/// `const` and `#[repr]` item of mm-core, mm-compile and this crate (their
/// `LAYOUT_HASH`es); the opcode table; the packed prelude (hashed at build
/// time); the native, type and native-step tables in order; and the
/// encodings of values, headers, string, int and float hashes. Nothing
/// else: an edit to a function body elsewhere keeps images restorable.
#[must_use]
#[cfg_attr(mm_shipped, optimize(size))]
pub fn build_fingerprint() -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut out = Sink::Buf(&mut bytes);
    put_u32(&mut out, SNAPSHOT_EPOCH);
    put_u64(&mut out, crate::LAYOUT_HASH);
    put_u64(&mut out, mm_core::LAYOUT_HASH);
    put_u64(&mut out, mm_compile::LAYOUT_HASH);
    put_u64(&mut out, mm_compile::bytecode::TABLE_HASH);
    put_u64(&mut out, crate::prelude::fingerprint());
    put_len(&mut out, crate::builtins::NATIVES.len());
    for n in crate::builtins::NATIVES {
        put_bytes(&mut out, n.name().as_bytes());
        put_bytes(&mut out, n.module().unwrap_or("").as_bytes());
        put_bytes(
            &mut out,
            n.owner.map_or("", crate::types::T::name).as_bytes(),
        );
    }
    for t in crate::types::ALL {
        put_bytes(&mut out, t.name().as_bytes());
    }
    for (name, nvals, nraw) in crate::nstep::STEPS {
        put_bytes(&mut out, name.as_bytes());
        put_len(&mut out, nvals as usize);
        put_len(&mut out, nraw as usize);
    }
    for v in [
        Value::NONE,
        Value::FALSE,
        Value::TRUE,
        Value::NOT_IMPLEMENTED,
        Value::ELLIPSIS,
        Value::UNDEFINED,
        Value::native(5),
        Value::from_ref(mm_core::Ref::from_raw(0x0012_3456), ty::LIST as u8),
        Value::int(-3).unwrap_or(Value::NONE),
        Value::float(0.5),
    ] {
        put_u64(&mut out, v.to_bits());
    }
    for w in mm_core::encoding_samples() {
        put_u64(&mut out, w);
    }
    // Dict and set tables keep their keys' hashes.
    for i in [-1, 7, i64::MAX, i64::MIN, 1 << 61] {
        put_u64(&mut out, crate::dict::hash_int(i).cast_unsigned());
    }
    for f in [0.5, -1.5e300, 1e-310, f64::INFINITY] {
        put_u64(&mut out, crate::dict::hash_float(f).cast_unsigned());
    }
    bytes
}

impl crate::HostProgram {
    /// A hash of `program`'s code: FNV-1a over its entry and each code
    /// object as an image encodes it. Images keep it beside a keyed host
    /// module's key ([`crate::HostProgram::code`]); computed here, with the
    /// codec, so the build id covers how.
    #[must_use]
    #[cfg_attr(mm_shipped, optimize(size))]
    pub fn code_hash(program: &mm_compile::bytecode::Program) -> u64 {
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        let mut eat = |bytes: &[u8]| {
            for &b in bytes {
                hash = (hash ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3);
            }
        };
        eat(&program.entry.to_le_bytes());
        for c in &program.codes {
            c.encode_to(&mut eat);
        }
        hash
    }
}

// ------------------------------------------------------------- writing

#[cfg_attr(mm_shipped, optimize(size))]
fn put_value(out: &mut Sink<'_>, v: Value) {
    put_u64(out, v.to_bits());
}

#[cfg_attr(mm_shipped, optimize(size))]
fn put_values(out: &mut Sink<'_>, vs: &[Value]) {
    put_len(out, vs.len());
    for &v in vs {
        put_value(out, v);
    }
}

#[cfg_attr(mm_shipped, optimize(size))]
fn put_str(out: &mut Sink<'_>, s: &str) {
    put_bytes(out, s.as_bytes());
}

#[cfg_attr(mm_shipped, optimize(size))]
fn put_bool(out: &mut Sink<'_>, b: bool) {
    put_u8(out, u8::from(b));
}

#[cfg_attr(mm_shipped, optimize(size))]
fn put_opt_u64(out: &mut Sink<'_>, v: Option<u64>) {
    put_bool(out, v.is_some());
    put_u64(out, v.unwrap_or(0));
}

#[cfg_attr(mm_shipped, optimize(size))]
fn put_state(out: &mut Sink<'_>, st: &State) {
    put_len(out, st.pc);
    put_len(out, st.base);
    put_u32(out, st.ci);
    put_value(out, st.acc);
    put_value(out, st.func);
    put_value(out, st.coro);
}

#[cfg_attr(mm_shipped, optimize(size))]
fn put_opt_state(out: &mut Sink<'_>, st: Option<&State>) {
    put_bool(out, st.is_some());
    if let Some(st) = st {
        put_state(out, st);
    }
}

#[cfg_attr(mm_shipped, optimize(size))]
fn put_frames(out: &mut Sink<'_>, frames: &[Frame]) {
    put_len(out, frames.len());
    for f in frames {
        put_u32(out, f.code);
        put_value(out, f.func);
        put_u32(out, f.base);
        put_u32(out, f.call_pc);
        put_u32(out, f.ret_pc);
        put_value(out, f.coro);
        put_u32(out, f.cont);
    }
}

/// A stack: its length, then every register.
#[cfg_attr(mm_shipped, optimize(size))]
fn put_stack(out: &mut Sink<'_>, stack: &[Value]) {
    // Every register: those above the live top are not traced, but a later
    // frame whose registers cover them is, as in the session that goes on.
    put_len(out, stack.len());
    put_values(out, stack);
}

#[cfg_attr(mm_shipped, optimize(size))]
fn put_cont(out: &mut Sink<'_>, c: &Cont) {
    let (tag, vals, words): (u8, [Value; 3], [u32; 4]) = match *c {
        Cont::None => (0, [Value::NONE; 3], [0; 4]),
        Cont::Discard => (1, [Value::NONE; 3], [0; 4]),
        Cont::Init(v) => (2, [v, Value::NONE, Value::NONE], [0; 4]),
        Cont::NewInit { cls, args, kw } => (3, [cls, args, kw], [0; 4]),
        Cont::Raise => (4, [Value::NONE; 3], [0; 4]),
        Cont::Truth {
            mode,
            target,
            len,
            orig,
        } => (
            5,
            [orig, Value::NONE, Value::NONE],
            [mode.into(), target, len.into(), 0],
        ),
        Cont::Bin { op, stage, a, b } => (6, [a, b, Value::NONE], [op.into(), stage.into(), 0, 0]),
        Cont::Cmp { op, stage, a, b } => (7, [a, b, Value::NONE], [op.into(), stage.into(), 0, 0]),
        Cont::Len => (8, [Value::NONE; 3], [0; 4]),
        Cont::Check(k) => (9, [Value::NONE; 3], [k.into(), 0, 0, 0]),
        Cont::Contains(neg) => (10, [Value::NONE; 3], [neg.into(), 0, 0, 0]),
        Cont::ForIter(exit) => (11, [Value::NONE; 3], [exit, 0, 0, 0]),
        Cont::Send(exit) => (12, [Value::NONE; 3], [exit, 0, 0, 0]),
        Cont::Delegate { exit, at } => (13, [Value::NONE; 3], [exit, at, 0, 0]),
        Cont::Next(v) => (14, [v, Value::NONE, Value::NONE], [0; 4]),
        Cont::GenSend => (15, [Value::NONE; 3], [0; 4]),
        Cont::Close => (16, [Value::NONE; 3], [0; 4]),
        Cont::CallIter { it, exit } => (17, [it, Value::NONE, Value::NONE], [exit, 0, 0, 0]),
        Cont::WithExit(v) => (18, [v, Value::NONE, Value::NONE], [0; 4]),
        Cont::Unpack { r, n } => (19, [Value::NONE; 3], [r.into(), n.into(), 0, 0]),
        Cont::StoreReg(r) => (20, [Value::NONE; 3], [r.into(), 0, 0, 0]),
        Cont::AttrDefault(v) => (21, [v, Value::NONE, Value::NONE], [0; 4]),
        Cont::HasAttr => (22, [Value::NONE; 3], [0; 4]),
        Cont::GetattrFallback(a, b) => (23, [a, b, Value::NONE], [0; 4]),
        Cont::Import {
            module,
            dotted,
            top,
            from_,
        } => (
            24,
            [module, dotted, Value::NONE],
            [top.into(), from_.into(), 0, 0],
        ),
        Cont::BxWrap(c) => (25, [c, Value::NONE, Value::NONE], [0; 4]),
        Cont::Native { op, state } => (26, [state, Value::NONE, Value::NONE], [op.into(), 0, 0, 0]),
        Cont::CallIterEq { it, exit, v } => (27, [it, v, Value::NONE], [exit, 0, 0, 0]),
        Cont::ASend => (28, [Value::NONE; 3], [0; 4]),
        Cont::AClose => (29, [Value::NONE; 3], [0; 4]),
        Cont::CoroSend => (30, [Value::NONE; 3], [0; 4]),
        Cont::AwaitStep(w) => (31, [w, Value::NONE, Value::NONE], [0; 4]),
    };
    put_u8(out, tag);
    for v in vals {
        put_value(out, v);
    }
    for w in words {
        put_u32(out, w);
    }
}

/// One step of writing a host value without recursion.
enum HostItem<'a> {
    Value(&'a HostValue),
    Key(&'a str),
}

// Out of line: snapshots and the facade's header (`put_host_value`) share
// one copy. Into a `Vec` rather than through `put_*`'s callback: the
// facade encodes every host call's arguments (`Session::pending`), and a
// dynamic call per field was most of that cost [M]. The writes stay out of
// line: inlined, they cost about 600 B for about 110 Ir less a call [M].
#[inline(never)]
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn put_host(out: &mut Vec<u8>, v: &HostValue) {
    #[inline(never)]
    #[cfg_attr(mm_shipped, optimize(size))]
    fn len(out: &mut Vec<u8>, n: usize) {
        out.extend_from_slice(&u32::try_from(n).unwrap_or(u32::MAX).to_le_bytes());
    }
    #[inline(never)]
    #[cfg_attr(mm_shipped, optimize(size))]
    fn tag(out: &mut Vec<u8>, t: u8) {
        out.push(t);
    }
    #[inline(never)]
    #[cfg_attr(mm_shipped, optimize(size))]
    fn word(out: &mut Vec<u8>, t: u8, w: u64) {
        out.push(t);
        out.extend_from_slice(&w.to_le_bytes());
    }
    #[inline(never)]
    #[cfg_attr(mm_shipped, optimize(size))]
    fn text(out: &mut Vec<u8>, b: &[u8]) {
        len(out, b.len());
        out.extend_from_slice(b);
    }
    // What is left to write, past `next`; a scalar allocates nothing.
    let mut work = Vec::new();
    let mut next = Some(HostItem::Value(v));
    while let Some(item) = next.take().or_else(|| work.pop()) {
        let v = match item {
            HostItem::Key(k) => {
                text(out, k.as_bytes());
                continue;
            }
            HostItem::Value(v) => v,
        };
        match v {
            HostValue::None => tag(out, 0),
            HostValue::Bool(b) => {
                tag(out, 1);
                tag(out, u8::from(*b));
            }
            HostValue::Int(i) => {
                word(out, 2, i.cast_unsigned());
            }
            HostValue::BigInt(s) => {
                tag(out, 3);
                text(out, s.as_bytes());
            }
            HostValue::Float(f) => {
                word(out, 4, f.to_bits());
            }
            HostValue::Str(s) => {
                tag(out, 5);
                text(out, s.as_bytes());
            }
            HostValue::Bytes(b) => {
                tag(out, 6);
                text(out, b);
            }
            HostValue::List(items) | HostValue::Tuple(items) => {
                tag(
                    out,
                    if matches!(v, HostValue::Tuple(_)) {
                        15
                    } else {
                        7
                    },
                );
                len(out, items.len());
                work.extend(items.iter().rev().map(HostItem::Value));
            }
            HostValue::Dict(fields)
            | HostValue::Record { fields, .. }
            | HostValue::Object { fields, .. } => {
                match v {
                    HostValue::Record { name, .. } => {
                        tag(out, 9);
                        text(out, name.as_bytes());
                    }
                    HostValue::Object { class, .. } => {
                        tag(out, 14);
                        text(out, class.as_bytes());
                    }
                    _ => tag(out, 8),
                }
                len(out, fields.len());
                for (k, v) in fields.iter().rev() {
                    work.push(HostItem::Value(v));
                    work.push(HostItem::Key(k));
                }
            }
            HostValue::Unconverted(u) => match u {
                Unconverted::Opaque { type_name, repr } => {
                    tag(out, 10);
                    text(out, type_name.as_bytes());
                    text(out, repr.as_bytes());
                }
                Unconverted::KeyNotStr => tag(out, 11),
                Unconverted::TooDeep => tag(out, 12),
                Unconverted::TooLarge { type_name } => {
                    tag(out, 13);
                    text(out, type_name.as_bytes());
                }
            },
        }
    }
}

impl Vm {
    /// Appends the session's FULL image to `out`, collecting nothing.
    /// Call it while the session is stopped: between cells, parked, at a
    /// host call or waiting on host futures.
    ///
    /// # Errors
    ///
    /// [`NotAtSafepoint`] if a builtin's request is still pending (never
    /// the case when the session has returned to the host).
    #[cfg_attr(mm_shipped, optimize(size))]
    pub fn write_snapshot(&self, out: &mut Vec<u8>) -> Result<(), NotAtSafepoint> {
        self.write_snapshot_to(&mut Sink::Buf(out))
    }

    /// Streams [`Vm::write_snapshot`]'s form without owning an output buffer.
    /// Returns the same safepoint error as that method.
    #[cfg_attr(mm_shipped, optimize(size))]
    pub fn write_snapshot_to(&self, out: &mut Sink<'_>) -> Result<(), NotAtSafepoint> {
        self.prepare_snapshot()?;
        self.emit_snapshot(out);
        Ok(())
    }

    /// [`Vm::write_snapshot`]'s form after `header`, in one buffer of
    /// exactly its size plus `trailer` bytes of spare capacity, or `None`
    /// when that passes `limit` bytes or memory runs out. The image is
    /// measured before it is written, so the buffer never grows: a growing
    /// one would hold up to twice the image while it doubled.
    ///
    /// # Errors
    ///
    /// [`NotAtSafepoint`], as [`Vm::write_snapshot`].
    #[cfg_attr(mm_shipped, optimize(size))]
    pub fn write_snapshot_sized(
        &self,
        header: &[u8],
        trailer: usize,
        limit: usize,
    ) -> Result<Option<Vec<u8>>, NotAtSafepoint> {
        self.prepare_snapshot()?;
        let mut count = Sink::Count(header.len().saturating_add(trailer));
        self.emit_snapshot(&mut count);
        let Sink::Count(len) = count else {
            return Ok(None);
        };
        let mut out = Vec::new();
        let fits = len <= limit && out.try_reserve_exact(len).is_ok();
        if fits {
            out.extend_from_slice(header);
            self.emit_snapshot(&mut Sink::Buf(&mut out));
        }
        Ok(fits.then_some(out))
    }

    /// Readies the session for [`Vm::emit_snapshot`]. Nothing is
    /// collected and nothing the collector or the charges read changes: the
    /// image holds the session exactly as it is, garbage, marks and free
    /// lists included, so the session that goes on and a restored one are
    /// the same session (docs/spec.md §9.3).
    #[cfg_attr(mm_shipped, optimize(size))]
    fn prepare_snapshot(&self) -> Result<(), NotAtSafepoint> {
        if self.request.is_some()
            || (self.saved.is_none() && self.codes.get(self.tramp as usize).is_none())
        {
            return Err(NotAtSafepoint);
        }
        Ok(())
    }

    /// Writes the image of a prepared session ([`Vm::prepare_snapshot`]);
    /// it reads the session only, so two calls write the same bytes.
    #[cfg_attr(mm_shipped, optimize(size))]
    fn emit_snapshot(&self, out: &mut Sink<'_>) {
        self.heap.write_snapshot_to(out);

        put_len(out, self.sources.len());
        for s in &self.sources {
            put_str(out, &s.name);
            put_str(out, &s.text);
        }
        // Host modules come before the code table, which refers to the
        // shared code of their definitions (`CODE_HOST`): the code the
        // embedder's cache holds, unless the embedder asked for it stored.
        let keyed: Vec<Option<&Arc<crate::HostProgram>>> = self
            .host_modules
            .iter()
            .map(|m| {
                m.program.as_ref().filter(|p| {
                    !self.host_code_in_snapshots
                        && self
                            .module_cache
                            .as_ref()
                            .and_then(|c| c.host_program(&p.key))
                            .is_some_and(|q| Arc::ptr_eq(p, &q))
                })
            })
            .collect();
        put_len(out, self.host_modules.len());
        for (m, program) in self.host_modules.iter().zip(&keyed) {
            put_str(out, &m.name);
            put_len(out, m.functions.len());
            for f in &m.functions {
                put_str(out, f);
            }
            put_len(out, m.constants.len());
            let mut host = Vec::new();
            for (k, v) in &m.constants {
                put_str(out, k);
                host.clear();
                put_host(&mut host, v);
                out.put(&host);
            }
            host.clear();
            put_host(&mut host, &m.spec);
            out.put(&host);
            put_bool(out, program.is_some());
            if let Some(p) = program {
                out.put(&p.key);
                put_u64(out, p.code);
                put_len(out, p.recipe.len());
                for r in &p.recipe {
                    put_str(out, r);
                }
            }
        }
        put_u32(out, self.tramp);
        put_u64(out, self.std_scratch);
        // Bundled modules: the code range each was loaded into, its body
        // and its globals (its code is referenced, not stored).
        put_len(out, self.std_codes.len());
        for (k, r) in self.std_codes.iter().enumerate() {
            let (first, end, entry) = r.unwrap_or((u32::MAX, u32::MAX, u32::MAX));
            put_u32(out, first);
            put_u32(out, end);
            put_u32(out, entry);
            put_value(out, self.std_globals.get(k).copied().unwrap_or(Value::NONE));
        }
        put_len(out, self.codes.len());
        // One buffer for every encoded code: a new one per code grew from
        // empty each time, several allocator calls a code.
        let mut encoded = Vec::new();
        for (i, c) in self.codes.iter().enumerate() {
            let i = u32::try_from(i).unwrap_or(u32::MAX);
            let std = self
                .std_codes
                .iter()
                .enumerate()
                .find_map(|(k, r)| match r {
                    Some((first, end, _)) if (*first..*end).contains(&i) && c.prelude => {
                        Some((k, i - first))
                    }
                    _ => None,
                });
            // Code of a host module's current definition is the Engine's;
            // an earlier definition's (still referenced) is stored.
            let host = self
                .sources
                .get(c.source as usize)
                .and_then(|s| s.host.as_ref())
                .and_then(|p| {
                    keyed
                        .iter()
                        .position(|q| q.is_some_and(|q| Arc::ptr_eq(p, q)))
                });
            if i == self.tramp {
                put_u8(out, CODE_TRAMPOLINE);
            } else if let Some(m) = host {
                put_u8(out, CODE_HOST);
                put_u32(out, u32::try_from(m).unwrap_or(u32::MAX));
                put_u32(out, i - c.module_base);
            } else if let Some((k, at)) = std {
                put_u8(out, CODE_STDLIB);
                put_u32(out, u32::try_from(k).unwrap_or(u32::MAX));
                put_u32(out, at);
            } else if let Some([chunk, at]) =
                self.sources.get(c.source as usize).and_then(|s| s.unit)
            {
                // A prelude unit's code, by the unit's place in the
                // packed prelude the build id covers.
                put_u8(out, CODE_PRELUDE);
                put_u32(out, chunk.into());
                put_u32(out, at.into());
                put_u32(out, i - c.module_base);
            } else {
                put_u8(out, CODE_ENCODED);
                encoded.clear();
                c.code.encode_into(&mut encoded);
                out.put(&encoded);
            }
            put_u32(out, c.module_base);
            put_u32(out, c.source);
            put_bool(out, c.prelude);
            put_u32(out, c.gslot);
            put_values(out, &c.consts);
            put_values(out, &c.names);
            put_value(out, c.name);
            put_value(out, c.qualname);
        }
        // The signature index (`_mm.signature`): a restore reuses the same
        // codes and counts toward `MAX_SYNTH` from where the session was.
        put_len(out, self.synth.len());
        for &ci in &self.synth {
            put_u32(out, ci);
        }

        for v in [
            self.globals,
            self.builtins,
            self.handled,
            self.exc,
            self.memory_error,
            self.modules,
            self.prelude_globals,
            self.missing,
            self.empty_tuple,
        ] {
            put_value(out, v);
        }
        put_values(out, &self.types);
        put_u64(out, self.gver);
        put_u32(out, self.recursion_limit);
        put_u32(out, self.recursion_ceiling);
        put_opt_u64(out, self.fuel);
        put_u32(out, self.ticks.cast_unsigned());
        put_u32(out, self.tick_period.cast_unsigned());
        // Where the regular checks fall and what forces the next one
        // (`Vm::regular_check`): a restored session checks where the one
        // that went on does.
        put_u32(out, self.sp_phase.cast_unsigned());
        put_u64(out, self.burned);
        put_bool(out, self.long_work);
        put_bool(out, self.slow);
        put_u64(out, self.seq);
        put_u64(out, self.vclock.to_bits());
        put_u64(out, self.next_call);
        put_bool(out, self.host_call.is_some());
        let (hid, hpc) = self.host_call.unwrap_or((0, 0));
        put_u64(out, hid);
        put_u32(out, hpc);
        put_u64(out, self.next_version);
        put_bool(out, self.want_text);
        match &self.completion_text {
            Some((r, s)) => {
                put_bool(out, true);
                put_value(out, *r);
                put_value(out, *s);
            }
            None => put_bool(out, false),
        }
        // The host's time zone is session state: a restored session keeps
        // its DST rules even when `time` is first imported after restore.
        put_bool(out, self.local_zone.is_some());
        put_bytes(out, self.local_zone.as_deref().unwrap_or(&[]));
        match self.post {
            None => put_u8(out, 0),
            Some(Post::Render(v)) => {
                put_u8(out, 1);
                put_value(out, v);
            }
            Some(Post::Complete(v)) => {
                put_u8(out, 2);
                put_value(out, v);
            }
        }

        // The running task's stack and frames, and its registers if stopped.
        put_stack(out, &self.stack);
        put_u32(out, self.depth);
        put_frames(out, &self.specials);
        put_opt_state(out, self.saved.as_ref());
        put_len(out, self.cur);
        self.check_parked_bytes();
        put_len(out, self.tasks.len());
        for t in &self.tasks {
            let Some(t) = t else {
                put_bool(out, false);
                continue;
            };
            put_bool(out, true);
            put_value(out, t.handle);
            put_value(out, t.handled);
            put_value(out, t.awaiting);
            put_value(out, t.context);
            put_stack(out, &t.stack);
            put_u32(out, t.depth);
            put_frames(out, &t.specials);
            put_opt_state(out, t.regs());
        }
        put_len(out, self.ready.len());
        for r in &self.ready {
            match *r {
                Ready::Task(i) => {
                    put_u8(out, 0);
                    put_u64(out, i as u64);
                }
                Ready::Gather(g, c) => {
                    put_u8(out, 1);
                    put_value(out, g);
                    put_value(out, c);
                }
                Ready::Call(f, a) => {
                    put_u8(out, 2);
                    put_value(out, f);
                    put_value(out, a);
                }
                Ready::Handle(h) => {
                    put_u8(out, 3);
                    put_value(out, h);
                }
            }
        }
        put_len(out, self.blocked.len());
        for &b in &self.blocked {
            put_len(out, b);
        }
        put_values(out, &self.gathers);
        put_len(out, self.iter_left);
        put_bool(out, self.iter_open);
        put_u32(out, self.stop_wait);
        put_len(out, self.cb_slots);
        put_len(out, self.queue_spare());
        put_u8(out, self.sched_turns as u8);
        put_u64(out, self.task_counter);
        put_value(out, self.task_names);
        put_value(out, self.context);
        put_value(out, self.escape);
        put_value(out, self.cancelling);
        put_value(out, self.run_handled);
        put_bool(out, self.asyncgens_listed);
        put_values(out, &self.unretrieved);
        put_u8(out, self.loop_flags);
        put_len(out, self.futures.len());
        for &(id, f) in &self.futures {
            put_u64(out, id);
            put_value(out, f);
        }
        put_len(out, self.conts.len());
        for c in &self.conts {
            match c {
                Some(c) => {
                    put_bool(out, true);
                    put_cont(out, c);
                }
                None => put_bool(out, false),
            }
        }
        put_len(out, self.cont_free.len());
        for &i in &self.cont_free {
            put_u32(out, i);
        }

        put_len(out, self.shapes.len());
        for s in &self.shapes {
            put_values(out, &s.names);
            put_len(out, s.trans.len());
            for &(k, to) in &s.trans {
                put_value(out, k);
                put_u32(out, to);
            }
        }
        put_len(out, self.host_ids.len());
        for n in &self.host_ids {
            put_str(out, n);
        }
        put_u64(out, self.host_args.max_bytes);
        put_u32(out, self.host_args.max_depth);
        put_bool(out, self.host_args.lenient);
        // What a parked operator asked for (`Session::fuel_wanted`).
        put_u64(out, self.fuel_wanted);
        // Whether the instruction parked at is the rerun of one that ran
        // out of memory, parked in its wait for the collection
        // (`Vm::oom_retry_if`): it raises if it fails again. Which one it is
        // (`oom_rerun_at`).
        put_bool(out, self.oom_rerun == self.op_stamp());
        put_u64(out, self.oom_rerun_at);
        put_len(out, self.call_log.len());
        for &(id, function, status) in &self.call_log {
            put_u64(out, id);
            put_u32(out, function);
            put_u8(out, status.code());
        }
        put_values(out, &self.fresh);
        // The generator registry as it is, in its order, and where the
        // clearing pass goes on: a snapshot neither collects nor rebuilds
        // it, so the restored session runs the passes the one that went on
        // runs.
        put_values(out, &self.gens);
        put_len(out, self.gens_at);
        // The VM's side of the collector's schedule and of the charges:
        // nothing collects before an image, so a restored session goes on
        // with them where the one that went on does.
        put_bool(out, self.gc_owed);
        put_bool(out, self.gc_pending);
        put_u32(out, self.tick_spare.cast_unsigned());
        put_u64(out, self.gc_wait);
        put_bool(out, self.oom_wait);
        put_u64(out, self.oom_defer_at);
        put_values(out, &self.dunders);
        // The code charge is not written: it counts this target's sizes, so
        // a restore computes its own (`Vm::reserve_code`). What the image
        // holds of it is the debt reclamation waits on (`Vm::prepare_cell`),
        // the code loaded since the last reclamation or `Vm::setup_done`.
        put_u64(out, self.code_charge.saturating_sub(self.code_kept));
        put_u64(out, self.heap.external_bytes());
        // Capacities the charges count (`Vm::code_bytes`,
        // `Vm::execution_bytes`): a restore reserves them again.
        self.write_capacities(out);
    }

    /// The capacities of the tables the memory limit charges by capacity,
    /// in the order [`Vm::reserve_capacities`] takes them.
    #[cfg_attr(mm_shipped, optimize(size))]
    fn write_capacities(&self, out: &mut Sink<'_>) {
        for c in [
            self.stack.capacity(),
            self.specials.capacity(),
            self.conts.capacity(),
            self.cont_free.capacity(),
            self.tasks.capacity(),
            self.ready.capacity(),
            self.blocked.capacity(),
            self.gathers.capacity(),
            self.futures.capacity(),
            self.call_log.capacity(),
            self.fresh.capacity(),
            self.consumed.capacity(),
            self.unretrieved.capacity(),
            self.sbuf.capacity(),
            self.codes.capacity(),
            self.sources.capacity(),
            self.ics.capacity(),
            self.synth.capacity(),
            self.shapes.capacity(),
            self.host_modules.capacity(),
            self.host_ids.capacity(),
            self.gens.capacity(),
            self.shared_chars.capacity(),
        ] {
            put_len(out, c);
        }
        for t in self.tasks.iter().flatten() {
            put_len(out, t.stack.capacity());
            put_len(out, t.specials.capacity());
        }
        for s in &self.shapes {
            put_len(out, s.names.capacity());
            put_len(out, s.trans.capacity());
        }
    }

    /// Reserves the capacities [`Vm::write_capacities`] wrote, each charged to
    /// `budget` first.
    #[cfg_attr(mm_shipped, optimize(size))]
    fn reserve_capacities(
        &mut self,
        caps: &[[u8; 4]],
        ics_len: usize,
        budget: &mut u64,
    ) -> Res<()> {
        fn grow<T>(v: &mut Vec<T>, cap: usize, budget: &mut u64) -> Res<()> {
            let extra = cap.saturating_sub(v.capacity());
            charge(budget, extra as u64 * size_of::<T>() as u64)?;
            v.try_reserve_exact(cap - v.len().min(cap))
                .map_err(|_| SnapError::TooLarge)
        }
        let mut caps = caps.iter().map(|c| u32::from_le_bytes(*c) as usize);
        let mut cap = || need(caps.next(), "capacities");
        grow(&mut self.stack, cap()?, budget)?;
        grow(&mut self.specials, cap()?, budget)?;
        grow(&mut self.conts, cap()?, budget)?;
        grow(&mut self.cont_free, cap()?, budget)?;
        grow(&mut self.tasks, cap()?, budget)?;
        let ready = cap()?;
        charge(
            budget,
            ready.saturating_sub(self.ready.capacity()) as u64 * size_of::<Ready>() as u64,
        )?;
        self.ready
            .try_reserve_exact(ready - self.ready.len().min(ready))
            .map_err(|_| SnapError::TooLarge)?;
        grow(&mut self.blocked, cap()?, budget)?;
        grow(&mut self.gathers, cap()?, budget)?;
        grow(&mut self.futures, cap()?, budget)?;
        grow(&mut self.call_log, cap()?, budget)?;
        grow(&mut self.fresh, cap()?, budget)?;
        grow(&mut self.consumed, cap()?, budget)?;
        grow(&mut self.unretrieved, cap()?, budget)?;
        let sbuf = cap()?;
        charge(budget, sbuf as u64)?;
        self.sbuf
            .try_reserve_exact(sbuf)
            .map_err(|_| SnapError::TooLarge)?;
        grow(Arc::make_mut(&mut self.codes), cap()?, budget)?;
        grow(&mut self.sources, cap()?, budget)?;
        let ics_cap = cap()?;
        grow(&mut self.synth, cap()?, budget)?;
        grow(&mut self.shapes, cap()?, budget)?;
        grow(&mut self.host_modules, cap()?, budget)?;
        grow(&mut self.host_ids, cap()?, budget)?;
        grow(&mut self.gens, cap()?, budget)?;
        // Empty or its full room: the cache's slots refill as it is used
        // (`Vm::shared_str`), and only its capacity is charged.
        grow(&mut self.shared_chars, cap()?, budget)?;
        for t in self.tasks.iter_mut().flatten() {
            grow(&mut t.stack, cap()?, budget)?;
            grow(&mut t.specials, cap()?, budget)?;
        }
        for s in &mut self.shapes {
            grow(&mut s.names, cap()?, budget)?;
            grow(&mut s.trans, cap()?, budget)?;
        }
        // The inline-cache table is allocated last, on purpose: every load
        // grows it by exactly its code's caches (`Vm::load_in`), and the
        // host's allocator extends a block in place only while nothing was
        // allocated after it. With a table reserved behind it each load of
        // a restored session copied the whole table (12.6 M of 311 M
        // instructions in a recorded SDK session). No test can hold this
        // without `unsafe` (it is the host allocator's layout, not engine
        // state), so keep this the last allocation of a restore.
        charge(
            budget,
            ics_cap.saturating_sub(ics_len) as u64 * size_of::<crate::vm::Ic>() as u64,
        )?;
        self.ics
            .try_reserve_exact(ics_cap.max(ics_len))
            .map_err(|_| SnapError::TooLarge)?;
        self.ics.resize(ics_len, crate::vm::Ic::Empty);
        Ok(())
    }
}

// ------------------------------------------------------------- reading

type Res<T> = Result<T, SnapError>;

/// The reader of the VM part, after the heap: every value it reads that
/// refers into the arena must name an object of its kind, which the engine
/// otherwise trusts (a generator's yield pc is read from whatever object a
/// `GEN` value names).
struct Rd<'a, 'h> {
    r: &'h mut Reader<'a>,
    heap: &'h Heap,
}

impl<'a> core::ops::Deref for Rd<'a, '_> {
    type Target = Reader<'a>;
    #[cfg_attr(mm_shipped, optimize(size))]
    fn deref(&self) -> &Reader<'a> {
        self.r
    }
}

impl<'a> core::ops::DerefMut for Rd<'a, '_> {
    #[cfg_attr(mm_shipped, optimize(size))]
    fn deref_mut(&mut self) -> &mut Reader<'a> {
        self.r
    }
}

impl Rd<'_, '_> {
    /// A value whose reference, if any, names an object of its kind.
    #[inline(never)]
    #[cfg_attr(mm_shipped, optimize(size))]
    fn value(&mut self) -> Option<Value> {
        self.r.value().filter(|&v| ref_ok(self.heap, v))
    }
}

/// Does `v`, if a reference, name an object of its kind?
#[cfg_attr(mm_shipped, optimize(size))]
fn ref_ok(heap: &Heap, v: Value) -> bool {
    if !heap.snapshot_ref_live(v) {
        return false;
    }
    match (v.as_obj(), v.obj_kind()) {
        (Some(o), Some(kind)) => heap.header(o).is_ok_and(|h| h.type_id() == u16::from(kind)),
        _ => true,
    }
}

/// Every reference in the arena's value slots names an object of its kind.
#[cfg_attr(mm_shipped, optimize(size))]
fn check_refs(heap: &Heap) -> Res<()> {
    let mut ok = true;
    let mut waiters_ok = true;
    heap.for_each_object(&mut |r, h| {
        // The intern table holds untagged string references; mm-core
        // checks it when it reads the heap.
        if !ok || h.type_id() == mm_core::types::INTERN_TABLE {
            return;
        }
        let Ok(w) = heap.slot_words(r) else {
            ok = false;
            return;
        };
        let n = (h.nvals() as usize).min(w.len());
        ok = w[..n]
            .iter()
            .all(|&b| Value::from_bits(b).is_none_or(|v| ref_ok(heap, v)));
        if matches!(h.type_id(), ty::TASK | ty::GATHER | ty::FUTURE | ty::SLEEP)
            && let Ok(v) = heap.get(r, crate::aio::aw::WAITERS)
            && v.is_obj_kind(ty::BUF as u8)
            && let Some(b) = v.as_obj()
        {
            waiters_ok &= heap
                .get(b, 0)
                .ok()
                .and_then(Value::as_int)
                .and_then(|n| u32::try_from(n).ok())
                .is_some_and(|n| n >= 2 && heap.header(b).is_ok_and(|h| n < h.nvals()));
        }
    });
    if !waiters_ok {
        Err(SnapError::Corrupt("waiters"))
    } else if ok {
        Ok(())
    } else {
        Err(SnapError::Corrupt("references"))
    }
}

/// A dict without an index table (a linear one, which `check_indexes`
/// leaves) has no more entries in use than its entries hold: iterators
/// and lookups walk them all.
#[cfg_attr(mm_shipped, optimize(size))]
fn linear_dicts_ok(heap: &Heap) -> bool {
    let mut ok = true;
    heap.for_each_object(&mut |r, h| {
        if !ok || h.type_id() != ty::DICT {
            return;
        }
        let Ok((_, w)) = heap.object_words(r) else {
            return;
        };
        let vals = &w[..(h.nvals() as usize).min(w.len())];
        let buf = |i: usize, kind: u16| {
            vals.get(i)
                .and_then(|&b| Value::from_bits(b))
                .filter(|v| v.is_obj_kind(kind as u8))
                .and_then(Value::as_obj)
        };
        let (Some(&c), None) = (w.last(), buf(1, ty::RAW)) else {
            return;
        };
        let used = (c >> 32) as usize;
        // The entries are a buffer's, or the dict's own value slots.
        ok = match buf(0, ty::BUF) {
            Some(entries) => heap
                .slot_words(entries)
                .is_ok_and(|kvs| used <= kvs.len() / 2),
            None => used <= vals.len() / 2,
        };
    });
    ok
}

#[cfg_attr(mm_shipped, optimize(size))]
fn need<T>(v: Option<T>, what: &'static str) -> Res<T> {
    v.ok_or(SnapError::Corrupt(what))
}

#[cfg_attr(mm_shipped, optimize(size))]
fn charge(budget: &mut u64, bytes: u64) -> Res<()> {
    *budget = budget.checked_sub(bytes).ok_or(SnapError::TooLarge)?;
    Ok(())
}

#[cfg_attr(mm_shipped, optimize(size))]
fn read_host_string(r: &mut Reader<'_>, budget: &mut u64) -> Res<String> {
    let s = need(r.str(), "host value")?;
    charge(budget, s.len() as u64)?;
    Ok(String::from(s))
}

#[cfg_attr(mm_shipped, optimize(size))]
fn read_index(r: &mut Rd<'_, '_>, what: &'static str) -> Res<usize> {
    need(r.u32(), what).map(|n| n as usize)
}

/// A state as written; checked once the code table and stacks are known.
#[derive(Clone, Copy)]
struct RawState {
    pc: usize,
    base: usize,
    ci: u32,
    acc: Value,
    func: Value,
    coro: Value,
}

#[cfg_attr(mm_shipped, optimize(size))]
fn read_opt_state(r: &mut Rd<'_, '_>) -> Res<Option<RawState>> {
    if !need(r.bool(), "state")? {
        return Ok(None);
    }
    Ok(Some(RawState {
        pc: read_index(r, "state")?,
        base: read_index(r, "state")?,
        ci: need(r.u32(), "state")?,
        acc: need(r.value(), "state")?,
        func: need(r.value(), "state")?,
        coro: need(r.value(), "state")?,
    }))
}

#[cfg_attr(mm_shipped, optimize(size))]
fn read_frames(r: &mut Rd<'_, '_>, budget: &mut u64) -> Res<Vec<Frame>> {
    let n = need(r.count(36), "frames")?;
    charge(budget, n as u64 * size_of::<Frame>() as u64)?;
    let mut frames = Vec::with_capacity(n);
    for _ in 0..n {
        frames.push(Frame {
            code: need(r.u32(), "frame")?,
            func: need(r.value(), "frame")?,
            base: need(r.u32(), "frame")?,
            call_pc: need(r.u32(), "frame")?,
            ret_pc: need(r.u32(), "frame")?,
            coro: need(r.value(), "frame")?,
            cont: need(r.u32(), "frame")?,
        });
    }
    Ok(frames)
}

/// Check the vector's count before charging it, then validate every
/// reference in stream order.
#[inline(never)]
fn read_values(
    r: &mut Rd<'_, '_>,
    budget: &mut u64,
    count: RangeInclusive<usize>,
    what: &'static str,
) -> Res<Vec<Value>> {
    let n = need(r.count(8), what)?;
    let (min, max) = count.into_inner();
    if n < min || n > max {
        return Err(SnapError::Corrupt(what));
    }
    charge(budget, n as u64 * size_of::<Value>() as u64)?;
    let mut values = Vec::with_capacity(n);
    for _ in 0..n {
        values.push(need(r.value(), what)?);
    }
    Ok(values)
}

/// A stack as written: every register.
#[cfg_attr(mm_shipped, optimize(size))]
fn read_stack(r: &mut Rd<'_, '_>, budget: &mut u64) -> Res<Vec<Value>> {
    let len = read_index(r, "stack")?;
    read_values(r, budget, len..=len, "stack")
}

#[cfg_attr(mm_shipped, optimize(size))]
fn read_cont(r: &mut Rd<'_, '_>) -> Res<Cont> {
    let tag = need(r.u8(), "continuation")?;
    let mut v = [Value::NONE; 3];
    for x in &mut v {
        *x = need(r.value(), "continuation")?;
    }
    let mut w = [0u32; 4];
    for x in &mut w {
        *x = need(r.u32(), "continuation")?;
    }
    let byte = |x: u32| need(u8::try_from(x).ok(), "continuation");
    let flag = |x: u32| match x {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(SnapError::Corrupt("continuation")),
    };
    Ok(match tag {
        0 => Cont::None,
        1 => Cont::Discard,
        2 => Cont::Init(v[0]),
        3 => Cont::NewInit {
            cls: v[0],
            args: v[1],
            kw: v[2],
        },
        4 => Cont::Raise,
        5 => Cont::Truth {
            mode: byte(w[0])?,
            target: w[1],
            len: flag(w[2])?,
            orig: v[0],
        },
        6 | 7 => {
            let (op, stage, a, b) = (byte(w[0])?, byte(w[1])?, v[0], v[1]);
            // `Bin` resumes at `stage + 1`. `Cmp` resumes at
            // `(stage & 0x7f) + 1`: bit 7 marks an inverted `__eq__`, and
            // `object.__ne__` parks at 0xff (inverted, no stage left).
            if tag == 6 && stage == u8::MAX {
                return Err(SnapError::Corrupt("continuation"));
            }
            if tag == 6 {
                Cont::Bin { op, stage, a, b }
            } else {
                Cont::Cmp { op, stage, a, b }
            }
        }
        8 => Cont::Len,
        9 => Cont::Check(byte(w[0])?),
        10 => Cont::Contains(flag(w[0])?),
        11 => Cont::ForIter(w[0]),
        12 => Cont::Send(w[0]),
        13 => Cont::Delegate {
            exit: w[0],
            at: w[1],
        },
        14 => Cont::Next(v[0]),
        15 => Cont::GenSend,
        16 => Cont::Close,
        17 => Cont::CallIter {
            it: v[0],
            exit: w[0],
        },
        18 => Cont::WithExit(v[0]),
        19 => Cont::Unpack {
            r: byte(w[0])?,
            n: need(u16::try_from(w[1]).ok(), "continuation")?,
        },
        20 => Cont::StoreReg(byte(w[0])?),
        21 => Cont::AttrDefault(v[0]),
        22 => Cont::HasAttr,
        23 => Cont::GetattrFallback(v[0], v[1]),
        24 => Cont::Import {
            module: v[0],
            dotted: v[1],
            top: flag(w[0])?,
            from_: flag(w[1])?,
        },
        25 => Cont::BxWrap(v[0]),
        26 if (w[0] as usize) < crate::nstep::STEPS.len() => Cont::Native {
            op: byte(w[0])?,
            state: v[0],
        },
        27 => Cont::CallIterEq {
            it: v[0],
            exit: w[0],
            v: v[1],
        },
        28 => Cont::ASend,
        29 => Cont::AClose,
        30 => Cont::CoroSend,
        31 => Cont::AwaitStep(v[0]),
        _ => return Err(SnapError::Corrupt("continuation")),
    })
}

/// A host container being read.
struct HostFrame {
    /// The key it is the value of, in its parent map.
    key: Option<String>,
    /// `Some(name)` for a record or host object, `None` for a list or dict.
    record: Option<String>,
    /// The container's tag: 7 list, 8 dict, 9 record, 14 object, 15 tuple.
    tag: u8,
    map: bool,
    items: Vec<HostValue>,
    fields: Vec<(String, HostValue)>,
    left: usize,
}

pub(crate) const HOST_FRAME_BYTES: u64 = size_of::<HostFrame>() as u64;

/// Reads a host value as live conversion or host-definition cloning
/// charges it. Live conversion does not charge the marker it returns
/// when it runs out of room; a host `definition` counts it like any value.
#[cfg_attr(mm_shipped, optimize(size))]
fn read_host(
    r: &mut Reader<'_>,
    budget: &mut u64,
    scratch: &mut u64,
    definition: bool,
) -> Res<(HostValue, u64)> {
    let mut stack: Vec<HostFrame> = Vec::new();
    let value = 'read: loop {
        let mut key = match stack.last() {
            Some(f) if f.map => Some(read_host_string(r, budget)?),
            _ => None,
        };
        let tag = need(r.u8(), "host value")?;
        let mut done = match tag {
            0 => HostValue::None,
            1 => HostValue::Bool(need(r.bool(), "host value")?),
            2 => HostValue::Int(need(r.u64(), "host value")?.cast_signed()),
            3 => HostValue::BigInt(read_host_string(r, budget)?),
            4 => HostValue::Float(f64::from_bits(need(r.u64(), "host value")?)),
            5 => HostValue::Str(read_host_string(r, budget)?),
            6 => {
                let bytes = need(r.bytes(), "host value")?;
                charge(budget, bytes.len() as u64)?;
                HostValue::Bytes(bytes.to_vec())
            }
            7..=9 | 14 | 15 => {
                let seq = tag == 7 || tag == 15;
                if stack.len() >= MAX_HOST_DEPTH_CEILING as usize {
                    return Err(SnapError::Corrupt("host value"));
                }
                let record = if tag == 9 || tag == 14 {
                    Some(read_host_string(r, budget)?)
                } else {
                    None
                };
                let n = need(r.count(if seq { 1 } else { 5 }), "host value")?;
                if stack.len() == stack.capacity() {
                    let bytes = (stack.len() + 1) as u64 * HOST_FRAME_BYTES;
                    charge(budget, bytes.saturating_sub(*scratch))?;
                    *scratch = (*scratch).max(bytes);
                    stack
                        .try_reserve_exact(1)
                        .map_err(|_| SnapError::TooLarge)?;
                }
                charge(
                    budget,
                    n as u64
                        * if seq {
                            size_of::<HostValue>()
                        } else {
                            size_of::<(String, HostValue)>()
                        } as u64,
                )?;
                stack.push(HostFrame {
                    key,
                    record,
                    tag,
                    map: !seq,
                    items: Vec::with_capacity(if seq { n } else { 0 }),
                    fields: Vec::with_capacity(if seq { 0 } else { n }),
                    left: n,
                });
                if n > 0 {
                    continue;
                }
                let Some(f) = stack.pop() else {
                    return Err(SnapError::Corrupt("host value"));
                };
                let (k, v) = finish_host(f);
                // Empty containers use the same ancestor unwind as scalars.
                key = k;
                v
            }
            10 => HostValue::Unconverted(Unconverted::Opaque {
                type_name: read_host_string(r, budget)?,
                repr: read_host_string(r, budget)?,
            }),
            11 => HostValue::Unconverted(Unconverted::KeyNotStr),
            12 => HostValue::Unconverted(Unconverted::TooDeep),
            13 => HostValue::Unconverted(Unconverted::TooLarge {
                type_name: if definition {
                    read_host_string(r, budget)?
                } else {
                    String::from(need(r.str(), "host value")?)
                },
            }),
            _ => return Err(SnapError::Corrupt("host value")),
        };
        let mut k = key;
        loop {
            if let Some(v) = attach_host(&mut stack, k, done) {
                break 'read v;
            }
            if stack.last().is_none_or(|f| f.left != 0) {
                break;
            }
            let Some(f) = stack.pop() else {
                return Err(SnapError::Corrupt("host value"));
            };
            (k, done) = finish_host(f);
        }
    };
    // Keep the largest workspace reservation for future same-budget restores.
    Ok((value, stack.capacity() as u64 * HOST_FRAME_BYTES))
}

/// Appends `v` to `out` as snapshots store host values (for an embedder's
/// own header, such as the facade's).
#[cfg_attr(mm_shipped, optimize(size))]
pub fn put_host_value(out: &mut Vec<u8>, v: &HostValue) {
    put_host(out, v);
}

/// Reads a value [`put_host_value`] wrote, taking the bytes it builds (as
/// a restore counts host values) from `budget`: `TooLarge` when they do
/// not fit, before they are allocated, `Corrupt` if it is malformed. The
/// decode stack's charge goes back once the value is built, as the live
/// conversion releases its open containers'.
#[cfg_attr(mm_shipped, optimize(size))]
pub fn read_host_value(r: &mut Reader<'_>, budget: &mut u64) -> Res<HostValue> {
    let mut scratch = 0;
    let (v, _) = read_host(r, budget, &mut scratch, false)?;
    *budget = budget.saturating_add(scratch);
    Ok(v)
}

#[cfg_attr(mm_shipped, optimize(size))]
fn finish_host(f: HostFrame) -> (Option<String>, HostValue) {
    let v = match (f.record, f.map) {
        (Some(class), _) if f.tag == 14 => HostValue::Object {
            class,
            fields: f.fields,
        },
        (Some(name), _) => HostValue::Record {
            name,
            fields: f.fields,
        },
        (None, true) => HostValue::Dict(f.fields),
        (None, false) if f.tag == 15 => HostValue::Tuple(f.items),
        (None, false) => HostValue::List(f.items),
    };
    (f.key, v)
}

#[cfg_attr(mm_shipped, optimize(size))]
fn attach_host(stack: &mut [HostFrame], key: Option<String>, v: HostValue) -> Option<HostValue> {
    let Some(f) = stack.last_mut() else {
        return Some(v);
    };
    if f.map {
        f.fields.push((key.unwrap_or_default(), v));
    } else {
        f.items.push(v);
    }
    f.left = f.left.saturating_sub(1);
    None
}

/// Does every `YIELD_FROM` of verified `code` follow its `SEND`, as the
/// compiler emits them? `throw()` into a generator suspended at one jumps
/// to the exit of the instruction before it.
#[cfg_attr(mm_shipped, optimize(size))]
fn sends_ok(code: &Code) -> bool {
    use mm_compile::bytecode::{instr_len, opcode};
    let bc = &code.bytecode;
    let (mut pc, mut last) = (0, op::NOP);
    while pc < bc.len() {
        let o = opcode(bc, pc);
        if o == op::YIELD_FROM && last != op::SEND {
            return false;
        }
        last = o;
        pc += instr_len(o);
    }
    true
}

/// What decodes a restored bundled code's instructions where a check
/// needs them: the codes a frame runs, a stopped state is at or a
/// suspended generator or coroutine resumes in.
struct Bundled<'a> {
    sources: &'a [Source],
    stdlib: &'static [crate::vm::StdModule],
    cache: Option<&'a dyn crate::vm::ModuleCache>,
    unpackers: &'a mut crate::vm::Unpackers,
}

impl Bundled<'_> {
    /// Decodes the instructions of `codes[ci]`'s program, if they were
    /// left out; any other code, or an index out of range, is left to
    /// the check.
    fn load(&mut self, codes: &mut [Arc<CodeObj>], ci: u32) -> Res<()> {
        if !codes
            .get(ci as usize)
            .is_some_and(|c| c.code.bytecode_deferred())
        {
            return Ok(());
        }
        crate::vm::load_bytecode_in(
            codes,
            ci as usize,
            self.sources,
            self.stdlib,
            self.cache,
            self.unpackers,
        )
        .map_err(|_| SnapError::Corrupt("bundled code"))
    }
}

/// Each code's instruction starts, one bit per word, taken in one walk
/// the first time a restore checks a pc of it: `Code::is_instr_start`
/// walks the code for each pc, and many generators or frames can name one
/// long code. The bits are a 64th of the codes' bytecode at most; where
/// there is no room for them, the walk per pc gives the same answer.
#[derive(Default)]
struct Starts {
    /// One past where each code's bits start in `bits`; 0 before they are
    /// taken.
    at: Vec<u64>,
    bits: Vec<u64>,
}

impl Starts {
    /// Whether `pc` starts an instruction of `code`, code `ci`.
    fn has(&mut self, ci: u32, code: &Code, pc: usize) -> bool {
        let (i, len) = (ci as usize, code.bytecode.len());
        if self.at.len() <= i && self.at.try_reserve(i + 1 - self.at.len()).is_ok() {
            self.at.resize(i + 1, 0);
        }
        let Some(at) = self.at.get_mut(i) else {
            return code.is_instr_start(pc);
        };
        if *at == 0 {
            let off = self.bits.len();
            if self.bits.try_reserve_exact(len.div_ceil(64)).is_err() {
                return code.is_instr_start(pc);
            }
            self.bits.resize(off + len.div_ceil(64), 0);
            let bits = &mut self.bits[off..];
            if !code.each_instr_start(&mut |s| bits[s / 64] |= 1 << (s % 64)) {
                self.bits.truncate(off);
                return false;
            }
            *at = off as u64 + 1;
        }
        pc < len && self.bits[*at as usize - 1 + pc / 64] >> (pc % 64) & 1 == 1
    }
}

/// Checks a raw state against the code table and its stack.
#[cfg_attr(mm_shipped, optimize(size))]
fn check_state(
    codes: &mut [Arc<CodeObj>],
    bundled: &mut Bundled<'_>,
    starts: &mut Starts,
    s: RawState,
    stack_len: usize,
) -> Res<State> {
    bundled.load(codes, s.ci)?;
    let code = need(codes.get(s.ci as usize), "state code")?;
    let top = s.base.checked_add(code.code.nregs as usize);
    if !starts.has(s.ci, &code.code, s.pc) || top.is_none_or(|t| t > stack_len) {
        return Err(SnapError::Corrupt("state"));
    }
    Ok(State {
        pc: s.pc,
        base: s.base,
        ci: s.ci,
        acc: s.acc,
        func: s.func,
        coro: s.coro,
    })
}

/// Checks the frames below `st` (their headers in `stack`, the records of
/// special frames, their continuations) against the code table and the
/// stack they index, and `st` and each frame against the generator or
/// coroutine it runs for ([`frame_owner_ok`]). Marks the continuations the
/// frames hold in `held`: one frame holds each.
#[cfg_attr(mm_shipped, optimize(size))]
#[allow(clippy::too_many_arguments, reason = "one reader's state")]
fn check_frames(
    heap: &Heap,
    codes: &mut [Arc<CodeObj>],
    bundled: &mut Bundled<'_>,
    starts: &mut Starts,
    stack: &[Value],
    specials: &[Frame],
    depth: u32,
    st: Option<&State>,
    conts: &[Option<Cont>],
    held: &mut [bool],
) -> Res<()> {
    let base = st.map_or(0, |s| s.base);
    let frames = need(
        walk_frames(stack, specials, codes, base, depth),
        "frame headers",
    )?;
    // Every stack's bottom frame runs at base 0, below which no header can
    // read as a link: the dispatch loop's plain `RETURN` trusts that.
    if st.is_some() && frames.last().map_or(base, |f| f.base as usize) != 0 {
        return Err(SnapError::Corrupt("frame headers"));
    }
    if !st.is_none_or(|s| frame_owner_ok(heap, s.coro, s.ci))
        || !frames.iter().all(|f| frame_owner_ok(heap, f.coro, f.code))
    {
        return Err(SnapError::Corrupt("frame owner"));
    }
    let mut callee = base;
    for f in &frames {
        bundled.load(codes, f.code)?;
        let code = need(codes.get(f.code as usize), "frame code")?;
        let mut at = |pc: u32| starts.has(f.code, &code.code, pc as usize);
        let nregs = code.code.nregs as usize;
        // The callee's header sits in this frame's window, past its own.
        let low = (f.base as usize).checked_add(HDR);
        let high = low.and_then(|l| l.checked_add(nregs));
        let ok = at(f.call_pc)
            && at(f.ret_pc)
            && low.is_some_and(|l| l <= callee)
            && high.is_some_and(|h| callee <= h);
        callee = f.base as usize;
        let cont_ok = f.cont == 0
            || match conts.get(f.cont as usize - 1) {
                Some(Some(c)) => match *c {
                    Cont::Truth { target, .. } => at(target),
                    Cont::ForIter(e) | Cont::Send(e) => at(e),
                    Cont::Delegate { exit, at: a } => at(exit) && at(a),
                    Cont::CallIter { exit, .. } | Cont::CallIterEq { exit, .. } => {
                        exit == u32::MAX || at(exit)
                    }
                    // Registers of the frame the result lands in.
                    Cont::StoreReg(r) => (r as usize) < nregs,
                    Cont::Unpack { r, n } => r as usize + n as usize <= nregs,
                    _ => true,
                },
                _ => false,
            };
        // A continuation goes to the frame that takes it: two frames on
        // different stacks sharing one would each finish with the other's.
        let once = f.cont == 0
            || held
                .get_mut(f.cont as usize - 1)
                .is_some_and(|h| !core::mem::replace(h, true));
        if !ok || !cont_ok || !once {
            return Err(SnapError::Corrupt("frame"));
        }
    }
    Ok(())
}

/// What [`Vm::read_snapshot`] asks the embedder for each host module whose
/// code the image refers to, unless the module cache holds a program under
/// the image's key built from the image's definition
/// ([`crate::HostProgram::definition`]): the code built again for the
/// module (its name, spec and the program's recipe), which must have the
/// key (the last argument). `None` (or another key's code) refuses the
/// image.
pub type HostCode<'a> =
    &'a mut dyn FnMut(&str, &HostValue, &[String], &[u8; 16]) -> Option<Arc<crate::HostProgram>>;

impl Vm {
    /// Rebuilds a session from [`Vm::write_snapshot`]'s output (the part
    /// after any header the facade adds). Arena, retained code, frame storage and scheduler tables share
    /// `max_commit` bytes, admitted before allocation, including verification
    /// and bundled-program workspace and fixed VM metadata.
    /// The result has no output callback,
    /// clock or deadline; every inline cache is empty.
    ///
    /// # Errors
    ///
    /// [`SnapError::Corrupt`] for anything truncated or inconsistent, or a
    /// host module whose code `host_code` cannot give under the image's
    /// key; [`SnapError::TooLarge`] if accounted storage does not fit
    /// `max_commit`.
    #[cfg_attr(mm_shipped, optimize(size))]
    pub fn read_snapshot(
        r: &mut Reader<'_>,
        stdlib: &'static [crate::vm::StdModule],
        module_cache: Option<Arc<dyn crate::vm::ModuleCache>>,
        host_code: HostCode<'_>,
        max_commit: u64,
    ) -> Res<Vm> {
        Self::read_snapshot_as(r, stdlib, module_cache, host_code, max_commit, true)
    }

    /// [`Vm::read_snapshot`] of an image this process already read with it
    /// under the same module cache (a session template's): the same
    /// session, built and admitted the same way, with every count, index
    /// and host-code key still checked as it is read, but without the
    /// validation walks ([`Heap::read_snapshot_trusted`], the reference,
    /// dict, bytecode, generator and native-step checks). Only for bytes
    /// that passed the checked read and have not changed since: what the
    /// walks would find there, a later run trusts.
    ///
    /// # Errors
    ///
    /// As [`Vm::read_snapshot`], apart from what the skipped walks find.
    #[doc(hidden)]
    #[cfg_attr(mm_shipped, optimize(size))]
    pub fn read_snapshot_trusted(
        r: &mut Reader<'_>,
        stdlib: &'static [crate::vm::StdModule],
        module_cache: Option<Arc<dyn crate::vm::ModuleCache>>,
        host_code: HostCode<'_>,
        max_commit: u64,
    ) -> Res<Vm> {
        Self::read_snapshot_as(r, stdlib, module_cache, host_code, max_commit, false)
    }

    /// [`Vm::read_snapshot`], with the validation walks only if `check`.
    #[cfg_attr(mm_shipped, optimize(size))]
    fn read_snapshot_as(
        r: &mut Reader<'_>,
        stdlib: &'static [crate::vm::StdModule],
        module_cache: Option<Arc<dyn crate::vm::ModuleCache>>,
        host_code: HostCode<'_>,
        max_commit: u64,
        check: bool,
    ) -> Res<Vm> {
        let heap = if check {
            Heap::read_snapshot(r, max_commit)
        } else {
            Heap::read_snapshot_trusted(r, max_commit)
        }
        .map_err(|e| match e {
            mm_core::Error::OutOfMemory => SnapError::TooLarge,
            _ => SnapError::Corrupt("heap"),
        })?;
        if check {
            check_refs(&heap)?;
            let indexes = crate::dict::check_indexes(&heap).ok_or(SnapError::TooLarge)?;
            if !indexes || !linear_dicts_ok(&heap) {
                return Err(SnapError::Corrupt("dict index"));
            }
        }
        let r = &mut Rd { r, heap: &heap };

        let mut budget = heap.available_bytes();
        charge(&mut budget, Self::fixed_bytes(stdlib.len()))?;
        let n = need(r.count(8), "sources")?;
        charge(&mut budget, n as u64 * size_of::<Source>() as u64)?;
        let mut sources = Vec::with_capacity(n);
        for _ in 0..n {
            let name = need(r.str(), "source")?;
            let text = need(r.str(), "source")?;
            charge(&mut budget, (name.len() + text.len()) as u64)?;
            sources.push(Source {
                name: name.into(),
                text: text.into(),
                host: None,
                unit: None,
                reclaim_index: 0,
            });
        }
        let nmods = need(r.count(12), "host modules")?;
        charge(
            &mut budget,
            nmods as u64 * size_of::<Arc<HostModule>>() as u64,
        )?;
        let mut host_modules = Vec::with_capacity(nmods);
        let mut host_scratch = 0;
        for _ in 0..nmods {
            let before = budget;
            charge(
                &mut budget,
                (size_of::<HostModule>() + 2 * size_of::<usize>()) as u64,
            )?;
            let scratch_before = host_scratch;
            let mut scratch = 0;
            let name = read_host_string(r, &mut budget)?;
            let nf = need(r.count(4), "host modules")?;
            charge(&mut budget, nf as u64 * size_of::<String>() as u64)?;
            let mut functions = Vec::with_capacity(nf);
            for _ in 0..nf {
                functions.push(read_host_string(r, &mut budget)?);
            }
            let nc = need(r.count(5), "host modules")?;
            charge(
                &mut budget,
                nc as u64 * size_of::<(String, HostValue)>() as u64,
            )?;
            let mut constants = Vec::with_capacity(nc);
            for _ in 0..nc {
                let k = read_host_string(r, &mut budget)?;
                let (value, workspace) = read_host(r, &mut budget, &mut host_scratch, true)?;
                scratch = scratch.max(workspace);
                constants.push((k, value));
            }
            let encoded = *r.rest_mut();
            let (spec, workspace) = read_host(r, &mut budget, &mut host_scratch, true)?;
            let encoded = &encoded[..encoded.len() - r.remaining()];
            scratch = scratch.max(workspace);
            // Its shared code: the cached program, if it was built from
            // this definition, else built again by the embedder from the
            // spec and recipe. Never another key's.
            let program = if need(r.bool(), "host modules")? {
                let key: [u8; 16] = need(r.take(16).and_then(|k| k.try_into().ok()), "host code")?;
                let code = need(r.u64(), "host code")?;
                let n = need(r.count(1), "host code")?;
                charge(&mut budget, n as u64 * size_of::<String>() as u64)?;
                let mut recipe = Vec::with_capacity(n);
                for _ in 0..n {
                    recipe.push(String::from(need(r.str(), "host code")?));
                }
                let definition = crate::host::definition_hash_of(&name, encoded, &recipe);
                let p = need(
                    module_cache
                        .as_ref()
                        .and_then(|c| c.host_program(&key))
                        .filter(|p| p.definition == definition)
                        .or_else(|| host_code(&name, &spec, &recipe, &key))
                        .filter(|p| p.key == key && p.code == code),
                    "host code",
                )?;
                // The recipe is dropped here, so the room it took is the
                // lookup's and not the module's: `bytes` is then what
                // `Vm::define_host_module` counts for the same definition.
                drop(recipe);
                budget += n as u64 * size_of::<String>() as u64;
                Some(p)
            } else {
                None
            };
            host_modules.push(Arc::new(HostModule {
                name,
                functions,
                constants,
                spec,
                program,
                bytes: before - budget - (host_scratch - scratch_before),
                scratch,
            }));
        }
        let tramp = need(r.u32(), "code table")?;
        let mut std_scratch = need(r.u64(), "bundled workspace")?;
        charge(&mut budget, std_scratch)?;
        let nstd = need(r.count(20), "bundled modules")?;
        if nstd != stdlib.len() {
            return Err(SnapError::Corrupt("bundled modules"));
        }
        let mut std_codes = Vec::with_capacity(nstd);
        let mut std_globals = Vec::with_capacity(nstd);
        let mut std_prog: Option<(usize, Arc<crate::vm::DecodedModule>)> = None;
        let mut unit_prog: Option<([u16; 2], Arc<crate::vm::DecodedModule>)> = None;
        // A prelude chunk unpacked without its instructions, for the units
        // after the first that the image names in it.
        let mut partial = None;
        let mut unpackers = None;
        for _ in 0..nstd {
            let first = need(r.u32(), "bundled modules")?;
            let end = need(r.u32(), "bundled modules")?;
            let entry = need(r.u32(), "bundled modules")?;
            std_codes.push(if first == u32::MAX {
                None
            } else if first <= entry && entry < end {
                Some((first, end, entry))
            } else {
                return Err(SnapError::Corrupt("bundled modules"));
            });
            std_globals.push(need(r.value(), "bundled modules")?);
        }
        let n = need(r.count(14), "code table")?;
        // Table pointers, Arc headers and inline CodeObjs, before any allocation.
        charge(
            &mut budget,
            n as u64 * (size_of::<CodeObj>() + 3 * size_of::<usize>()) as u64,
        )?;
        let mut codes: Vec<Arc<CodeObj>> = Vec::with_capacity(n);
        let mut n_ics = 0u32;
        let mut verify_scratch = 0;
        for i in 0..n {
            let ci = u32::try_from(i).map_err(|_| SnapError::Corrupt("code table"))?;
            let kind = need(r.u8(), "code")?;
            let mut host_at = None;
            let mut unit_at = None;
            let mut std_at = None;
            let code = match kind {
                CODE_TRAMPOLINE if ci == tramp => {
                    let c = CodeObj::trampoline(ci, 0).code;
                    charge(&mut budget, c.buffer_bytes())?;
                    c
                }
                CODE_STDLIB => {
                    let k = need(r.u32(), "code")? as usize;
                    let at = need(r.u32(), "code")?;
                    let Some(Some((first, end, _))) = std_codes.get(k).copied() else {
                        return Err(SnapError::Corrupt("bundled code"));
                    };
                    if first.checked_add(at) != Some(ci) || ci >= end {
                        return Err(SnapError::Corrupt("bundled code"));
                    }
                    if std_prog.as_ref().is_none_or(|(key, _)| *key != k) {
                        // Keep only one decoded module alive (beyond what the
                        // module cache holds). The reservation is a maximum,
                        // not the sum of imported programs.
                        drop(std_prog.take());
                        // The saved workspace was measured on the writer's
                        // target; this one may need more (wider pointers).
                        let allowance = budget.saturating_add(std_scratch);
                        let mut workspace = allowance;
                        // Instructions are decoded when a code first runs
                        // (`Vm::load_bytecode`), charged here all the same.
                        let p = stdlib[k]
                            .decode(
                                k,
                                module_cache.as_deref(),
                                &mut unpackers,
                                &mut workspace,
                                true,
                            )
                            .map_err(|e| match e {
                                mm_compile::DecodeError::Invalid => {
                                    SnapError::Corrupt("bundled code")
                                }
                                mm_compile::DecodeError::TooLarge => SnapError::TooLarge,
                            })?;
                        let used = allowance - workspace;
                        if used > std_scratch {
                            charge(&mut budget, used - std_scratch)?;
                            std_scratch = used;
                        }
                        if usize::try_from(end - first).ok() != Some(p.program.codes.len()) {
                            return Err(SnapError::Corrupt("bundled code"));
                        }
                        std_prog = Some((k, p));
                    }
                    let c = need(
                        std_prog
                            .as_ref()
                            .and_then(|(_, p)| p.program.codes.get(at as usize)),
                        "bundled code",
                    )?;
                    charge(&mut budget, c.buffer_bytes())?;
                    std_at = Some((k, first));
                    c.clone()
                }
                CODE_HOST => {
                    let m = need(r.u32(), "code")? as usize;
                    let at = need(r.u32(), "code")?;
                    let c = need(
                        host_modules
                            .get(m)
                            .and_then(|m| m.program.as_ref())
                            .and_then(|p| p.program.codes.get(at as usize)),
                        "host code",
                    )?;
                    charge(&mut budget, c.buffer_bytes())?;
                    host_at = Some((m, at));
                    c.clone()
                }
                CODE_PRELUDE => {
                    let mut half = || need(r.u32().and_then(|v| u16::try_from(v).ok()), "code");
                    let unit = [half()?, half()?];
                    let at = need(r.u32(), "code")?;
                    if unit_prog.as_ref().is_none_or(|(u, _)| *u != unit) {
                        // One unit's program at a time; its codes are
                        // consecutive.
                        drop(unit_prog.take());
                        let allowance = budget.saturating_add(std_scratch);
                        let p = crate::prelude::unit_program(
                            unit,
                            module_cache.as_deref(),
                            &mut unpackers,
                            allowance,
                            Some(&mut partial),
                        )
                        .map_err(|e| match e {
                            mm_compile::DecodeError::Invalid => SnapError::Corrupt("prelude code"),
                            mm_compile::DecodeError::TooLarge => SnapError::TooLarge,
                        })?;
                        if p.workspace > std_scratch {
                            charge(&mut budget, p.workspace - std_scratch)?;
                            std_scratch = p.workspace;
                        }
                        unit_prog = Some((unit, p));
                    }
                    let c = need(
                        unit_prog
                            .as_ref()
                            .and_then(|(_, p)| p.program.codes.get(at as usize)),
                        "prelude code",
                    )?;
                    charge(&mut budget, c.buffer_bytes())?;
                    unit_at = Some((unit, at));
                    c.clone()
                }
                CODE_ENCODED => Arc::new(Code::decode_bounded(r.rest_mut(), &mut budget).map_err(
                    |e| match e {
                        mm_compile::DecodeError::Invalid => SnapError::Corrupt("code"),
                        mm_compile::DecodeError::TooLarge => SnapError::TooLarge,
                    },
                )?),
                _ => return Err(SnapError::Corrupt("code")),
            };
            let scratch = code.verify_scratch_bytes() as u64;
            charge(&mut budget, scratch.saturating_sub(verify_scratch))?;
            verify_scratch = verify_scratch.max(scratch);
            if check && kind == CODE_ENCODED && !(code.verify() && sends_ok(&code)) {
                return Err(SnapError::Corrupt("code verification"));
            }
            let module_base = need(r.u32(), "code")?;
            let source = need(r.u32(), "code")?;
            let is_prelude = need(r.bool(), "code")?;
            let gslot = need(r.u32(), "code")?;
            if gslot as usize >= crate::vm::GSLOT_STD as usize + nstd
                || (gslot != crate::vm::GSLOT_USER && !is_prelude)
            {
                return Err(SnapError::Corrupt("code"));
            }
            let consts = read_values(
                r,
                &mut budget,
                code.consts.len()..=code.consts.len(),
                "code values",
            )?;
            let names = read_values(
                r,
                &mut budget,
                code.names.len()..=code.names.len(),
                "code values",
            )?;
            // A function made from the code takes them as its names.
            let mut both = [Value::NONE; 2];
            for v in &mut both {
                *v = need(r.value(), "code values")?;
                if *v != Value::NONE && !v.is_obj_kind(ty::STR as u8) {
                    return Err(SnapError::Corrupt("code values"));
                }
            }
            let [name, qualname] = both;
            if module_base > ci || (source != u32::MAX && source as usize >= sources.len()) {
                return Err(SnapError::Corrupt("code"));
            }
            // Whose instructions a first run decodes (`load_bytecode_in`):
            // a stdlib module's codes from its first, in its globals. Every
            // other code has its instructions.
            if let Some((k, first)) = std_at
                && (module_base != first
                    || u64::from(gslot) != crate::vm::GSLOT_STD as u64 + k as u64)
            {
                return Err(SnapError::Corrupt("bundled code"));
            }
            if std_at.is_none() && unit_at.is_none() && code.bytecode_deferred() {
                return Err(SnapError::Corrupt("code"));
            }
            if let Some((unit, at)) = unit_at {
                // A unit's codes sit together, under its own source.
                let s = sources.get_mut(source as usize);
                let Some(s) = s.filter(|s| s.unit.is_none_or(|u| u == unit)) else {
                    return Err(SnapError::Corrupt("prelude code"));
                };
                if ci.checked_sub(at) != Some(module_base)
                    || !is_prelude
                    || gslot != crate::vm::GSLOT_PRELUDE
                {
                    return Err(SnapError::Corrupt("prelude code"));
                }
                s.unit = Some(unit);
            }
            if let Some((m, at)) = host_at {
                // A host program's codes sit together, under one source.
                let p = host_modules.get(m).and_then(|m| m.program.as_ref());
                let s = sources.get_mut(source as usize);
                let (Some(p), Some(s)) = (p, s) else {
                    return Err(SnapError::Corrupt("host code"));
                };
                // `at` comes from the image and may exceed `ci`.
                if ci.checked_sub(at) != Some(module_base)
                    || s.host.as_ref().is_some_and(|q| !Arc::ptr_eq(p, q))
                {
                    return Err(SnapError::Corrupt("host code"));
                }
                s.host = Some(Arc::clone(p));
            }
            // Keyword-name tuples are read for their length.
            for (k, v) in code.consts.iter().zip(&consts) {
                if let Konst::Names(ns) = k {
                    let len = v
                        .as_obj()
                        .filter(|_| v.is_obj_kind(ty::TUPLE as u8))
                        .and_then(|o| heap.header(o).ok())
                        .filter(|h| h.type_id() == ty::TUPLE)
                        .map(mm_core::Header::len);
                    if len != u32::try_from(ns.len()).ok() {
                        return Err(SnapError::Corrupt("code constants"));
                    }
                } else if let Konst::Native(offset) = k {
                    let range = crate::builtins::PRIVATE_NATIVE_RANGE;
                    let i = range.start + u32::from(*offset);
                    if (unit_at.is_none() && std_at.is_none())
                        || i >= range.end
                        || *v != Value::native(i)
                    {
                        return Err(SnapError::Corrupt("code constants"));
                    }
                }
            }
            let ic_base = n_ics;
            charge(
                &mut budget,
                u64::from(code.n_ics) * size_of::<crate::vm::Ic>() as u64,
            )?;
            n_ics = need(n_ics.checked_add(code.n_ics), "code table")?;
            codes.push(Arc::new(CodeObj {
                code,
                consts,
                names,
                name,
                qualname,
                ic_base,
                module_base,
                source,
                prelude: is_prelude,
                gslot,
                reclaim_live: false,
                reclaim_index: 0,
            }));
        }
        if std_codes
            .iter()
            .flatten()
            .any(|&(_, end, _)| end as usize > codes.len())
        {
            return Err(SnapError::Corrupt("bundled modules"));
        }
        if tramp as usize >= codes.len() {
            return Err(SnapError::Corrupt("code table"));
        }
        if n_ics > (1 << 24) {
            return Err(SnapError::Corrupt("code table"));
        }
        let synth = read_synth(r, &codes)?;
        charge(&mut budget, (synth.capacity() * size_of::<u32>()) as u64)?;

        let mut fixed = [Value::NONE; 9];
        for v in &mut fixed {
            *v = need(r.value(), "registers")?;
        }
        let [
            globals,
            builtins,
            handled,
            exc,
            memory_error,
            modules,
            prelude_globals,
            missing,
            empty_tuple,
        ] = fixed;
        if !empty_tuple.is_obj_kind(ty::TUPLE as u8) {
            return Err(SnapError::Corrupt("registers"));
        }
        if need(r.count(8), "types")? != NTYPES {
            return Err(SnapError::Corrupt("types"));
        }
        let mut types = Vec::with_capacity(NTYPES);
        for _ in 0..NTYPES {
            types.push(need(r.value(), "types")?);
        }
        let gver = need(r.u64(), "registers")?;
        let recursion_limit = need(r.u32(), "registers")?;
        let recursion_ceiling = need(r.u32(), "registers")?;
        let fuel = {
            let some = need(r.bool(), "registers")?;
            let f = need(r.u64(), "registers")?;
            some.then_some(f)
        };
        let ticks = need(r.u32(), "registers")?.cast_signed();
        let tick_period = need(r.u32(), "registers")?.cast_signed();
        let sp_phase = need(r.u32(), "registers")?.cast_signed();
        let burned = need(r.u64(), "registers")?;
        let long_work = need(r.bool(), "registers")?;
        // A period ends at the next multiple of `TICKS` at the latest, and
        // `long_work` comes from a charge past 64 units (`Vm::burn`).
        if !(0..crate::vm::TICKS).contains(&sp_phase)
            || !(1..=crate::vm::TICKS - sp_phase).contains(&tick_period)
            || !(0..=tick_period).contains(&ticks)
            || (long_work && burned <= 64)
        {
            return Err(SnapError::Corrupt("registers"));
        }
        let slow = need(r.bool(), "registers")?;
        let seq = need(r.u64(), "registers")?;
        let vclock = f64::from_bits(need(r.u64(), "registers")?);
        let next_call = need(r.u64(), "registers")?;
        let has_call = need(r.bool(), "registers")?;
        let call = (need(r.u64(), "registers")?, need(r.u32(), "registers")?);
        let host_call = has_call.then_some(call);
        let next_version = need(r.u64(), "registers")?;
        let want_text = need(r.bool(), "registers")?;
        let completion_text = if need(r.bool(), "registers")? {
            let mut values = [Value::NONE; 2];
            for v in &mut values {
                *v = need(r.value(), "completion text")?;
                let obj = need(v.as_obj().filter(|_| Vm::is_str(*v)), "completion text")?;
                heap.as_str(obj)
                    .map_err(|_| SnapError::Corrupt("completion text"))?;
            }
            Some((values[0], values[1]))
        } else {
            None
        };
        let has_zone = need(r.bool(), "registers")?;
        let zone = need(r.bytes(), "registers")?;
        charge(&mut budget, zone.len() as u64)?;
        let local_zone = has_zone.then(|| zone.to_vec());
        let post = match need(r.u8(), "registers")? {
            0 => None,
            1 => Some(Post::Render(need(r.value(), "registers")?)),
            2 => Some(Post::Complete(need(r.value(), "registers")?)),
            _ => return Err(SnapError::Corrupt("registers")),
        };

        let stack = read_stack(r, &mut budget)?;
        let depth = need(r.u32(), "frames")?;
        let specials = read_frames(r, &mut budget)?;
        let saved = read_opt_state(r)?;
        let cur = read_index(r, "tasks")?;
        let ntasks = need(r.count(1), "tasks")?;
        charge(
            &mut budget,
            ntasks as u64 * size_of::<Option<Task>>() as u64,
        )?;
        let mut bundled = Bundled {
            sources: &sources,
            stdlib,
            cache: module_cache.as_deref(),
            unpackers: &mut unpackers,
        };
        let mut starts = Starts::default();
        let mut tasks = Vec::with_capacity(ntasks);
        for _ in 0..ntasks {
            if !need(r.bool(), "task")? {
                tasks.push(None);
                continue;
            }
            let handle = need(r.value(), "task")?;
            let t_handled = need(r.value(), "task")?;
            let awaiting = need(r.value(), "task")?;
            let t_context = need(r.value(), "task")?;
            let t_stack = read_stack(r, &mut budget)?;
            let t_depth = need(r.u32(), "frames")?;
            let t_specials = read_frames(r, &mut budget)?;
            let st = read_opt_state(r)?
                .map(|s| check_state(&mut codes, &mut bundled, &mut starts, s, t_stack.len()))
                .transpose()?;
            let mut task = Task::new(handle, t_context);
            task.stack = t_stack;
            task.depth = t_depth;
            task.specials = t_specials;
            task.handled = t_handled;
            task.awaiting = awaiting;
            if let Some(st) = st {
                task.st = st;
                task.saved = true;
            }
            tasks.push(Some(task));
        }
        let nready = need(r.count(9), "run queue")?;
        charge(&mut budget, nready as u64 * size_of::<Ready>() as u64)?;
        let mut ready = VecDeque::with_capacity(nready);
        for _ in 0..nready {
            ready.push_back(match need(r.u8(), "run queue")? {
                0 => {
                    let i = need(r.u64(), "run queue")?;
                    Ready::Task(need(usize::try_from(i).ok(), "run queue")?)
                }
                1 => Ready::Gather(need(r.value(), "run queue")?, need(r.value(), "run queue")?),
                2 => Ready::Call(need(r.value(), "run queue")?, need(r.value(), "run queue")?),
                3 => Ready::Handle(need(r.value(), "run queue")?),
                _ => return Err(SnapError::Corrupt("run queue")),
            });
        }
        let nblocked = need(r.count(4), "blocked")?;
        charge(&mut budget, nblocked as u64 * size_of::<usize>() as u64)?;
        let mut blocked = Vec::with_capacity(nblocked);
        let mut blocked_holes = 0;
        for k in 0..nblocked {
            let i = read_index(r, "blocked")?;
            if i == u32::MAX as usize {
                blocked.push(crate::vm::HOLE);
                blocked_holes += 1;
                continue;
            }
            // Its place in the list (`Task::at`): a task is listed once.
            if let Some(Some(t)) = tasks.get_mut(i) {
                if t.at != crate::vm::NOT_BLOCKED {
                    return Err(SnapError::Corrupt("blocked"));
                }
                t.at = u32::try_from(k).unwrap_or(crate::vm::NOT_BLOCKED);
            }
            blocked.push(i);
        }
        let gathers = read_values(r, &mut budget, 0..=usize::MAX, "gathers")?;
        let gathers_holes = gathers.iter().filter(|&&g| g == Value::UNDEFINED).count();
        let iter_left = read_index(r, "run queue")?;
        if iter_left > ready.len() {
            return Err(SnapError::Corrupt("run queue"));
        }
        let iter_open = need(r.bool(), "run queue")?;
        let stop_wait = need(r.u32(), "run queue")?;
        let cb_slots = read_index(r, "run queue")?;
        let spare = read_index(r, "run queue")?;
        // The full domain of the encoded byte is exactly the valid interval.
        const { assert!(crate::aio::SCHED_TURNS == 256) };
        let sched_turns = u32::from(need(r.u8(), "run queue")?);
        let task_counter = need(r.u64(), "run queue")?;
        let task_names = need(r.value(), "run queue")?;
        let context = need(r.value(), "run queue")?;
        let escape = need(r.value(), "run queue")?;
        // The scheduler's cancel in progress: its step checks the state.
        let cancelling = need(r.value(), "run queue")?;
        if !(cancelling == Value::NONE || cancelling.is_obj_kind(ty::NSTATE as u8)) {
            return Err(SnapError::Corrupt("run queue"));
        }
        let run_handled = need(r.value(), "run queue")?;
        let asyncgens_listed = need(r.bool(), "run queue")?;
        let unretrieved = read_values(r, &mut budget, 0..=usize::MAX, "run queue")?;
        let loop_flags = need(r.u8(), "run queue")?;
        let nfut = need(r.count(16), "futures")?;
        charge(&mut budget, nfut as u64 * size_of::<(u64, Value)>() as u64)?;
        let mut futures = Vec::with_capacity(nfut);
        for _ in 0..nfut {
            let id = need(r.u64(), "futures")?;
            if futures.last().is_some_and(|&(prev, _)| prev >= id) {
                return Err(SnapError::Corrupt("futures"));
            }
            futures.push((id, need(r.value(), "futures")?));
        }
        let nconts = need(r.count(1), "continuations")?;
        charge(
            &mut budget,
            nconts as u64 * (size_of::<Option<Cont>>() + size_of::<u32>()) as u64,
        )?;
        let mut conts = Vec::with_capacity(nconts);
        for _ in 0..nconts {
            conts.push(if need(r.bool(), "continuations")? {
                Some(read_cont(r)?)
            } else {
                None
            });
        }
        let nfree = need(r.count(4), "continuations")?;
        if nfree > nconts {
            return Err(SnapError::Corrupt("continuations"));
        }
        let mut cont_free = Vec::with_capacity(nconts);
        for _ in 0..nfree {
            cont_free.push(need(r.u32(), "continuations")?);
        }

        let nshapes = need(r.count(8), "shapes")?;
        charge(&mut budget, nshapes as u64 * size_of::<Shape>() as u64)?;
        let mut shapes = Vec::with_capacity(nshapes);
        for _ in 0..nshapes {
            let names = read_values(r, &mut budget, 0..=crate::class::MAX_SHAPE_ATTRS, "shapes")?;
            let nt = need(r.count(12), "shapes")?;
            charge(&mut budget, nt as u64 * size_of::<(Value, u32)>() as u64)?;
            let mut trans = Vec::with_capacity(nt);
            for _ in 0..nt {
                trans.push((need(r.value(), "shapes")?, need(r.u32(), "shapes")?));
            }
            shapes.push(Shape {
                names,
                trans,
                reclaim_index: 0,
            });
        }
        // A transition adds one name and points to a newer shape.
        let trans_ok = shapes.iter().enumerate().all(|(i, sh)| {
            sh.trans.iter().all(|&(_, to)| {
                to as usize > i
                    && shapes
                        .get(to as usize)
                        .is_some_and(|t| t.names.len() == sh.names.len() + 1)
            })
        });
        if shapes.is_empty() || !trans_ok {
            return Err(SnapError::Corrupt("shapes"));
        }
        let nids = need(r.count(4), "host ids")?;
        charge(&mut budget, nids as u64 * size_of::<String>() as u64)?;
        let mut host_ids = Vec::with_capacity(nids);
        for _ in 0..nids {
            host_ids.push(read_host_string(r, &mut budget)?);
        }
        let mut host_args = HostArgs {
            // A limit written on a wider target clamps to this one's.
            max_bytes: need(r.u64(), "host args")?,
            max_depth: need(r.u32(), "host args")?,
            lenient: need(r.bool(), "host args")?,
        };
        let fuel_wanted = need(r.u64(), "fuel wanted")?;
        let rerun = need(r.bool(), "rerun")?;
        let rerun_at = need(r.u64(), "rerun")?;
        let ncalls = need(r.count(13), "call log")?;
        charge(
            &mut budget,
            ncalls as u64 * size_of::<crate::host::CallEntry>() as u64,
        )?;
        let mut call_log: Vec<crate::host::CallEntry> = Vec::with_capacity(ncalls);
        for _ in 0..ncalls {
            let id = need(r.u64(), "call log")?;
            let function = need(r.u32(), "call log")?;
            let status = need(
                r.u8().and_then(crate::host::CallStatus::from_code),
                "call log",
            )?;
            if call_log.last().is_some_and(|&(prev, _, _)| prev >= id) {
                return Err(SnapError::Corrupt("call log"));
            }
            call_log.push((id, function, status));
        }
        let fresh = read_values(r, &mut budget, 0..=usize::MAX, "coroutines")?;
        let gens = read_values(r, &mut budget, 0..=usize::MAX, "generators")?;
        let gens_at = need(r.u32(), "generators")? as usize;
        if gens_at > gens.len() {
            return Err(SnapError::Corrupt("generators"));
        }
        let gc_owed = need(r.bool(), "collector")?;
        let gc_pending = need(r.bool(), "collector")?;
        let tick_spare = need(r.u32(), "collector")?.cast_signed();
        if !(0..=tick_period - ticks).contains(&tick_spare) {
            return Err(SnapError::Corrupt("collector"));
        }
        let gc_wait = need(r.u64(), "collector")?;
        let oom_wait = need(r.bool(), "collector")?;
        let oom_defer_at = need(r.u64(), "collector")?;
        let ndunders = need(r.count(8), "collector")?;
        // The dunder-name cache: empty, or each name's interned `str`.
        if ndunders != 0 && ndunders != crate::class::NDUNDERS {
            return Err(SnapError::Corrupt("dunders"));
        }
        let mut dunders = Vec::with_capacity(ndunders);
        for i in 0..ndunders {
            let v = need(r.value(), "dunders")?;
            let ok = v == Value::UNDEFINED
                || v.as_obj().is_some_and(|o| {
                    heap.header(o).is_ok_and(|h| {
                        h.type_id() == mm_core::types::STR
                            && h.flags() & mm_core::flags::INTERNED != 0
                    }) && heap
                        .str_bytes(o)
                        .is_ok_and(|b| b == crate::class::dunder_name(i).as_bytes())
                });
            if !ok {
                return Err(SnapError::Corrupt("dunders"));
            }
            dunders.push(v);
        }
        let code_debt = need(r.u64(), "collector")?;
        let external = need(r.u64(), "collector")?;
        let ncaps = 23 + 2 * tasks.iter().flatten().count() + 2 * shapes.len();
        if r.remaining() != ncaps * 4 {
            return Err(SnapError::Corrupt("capacities"));
        }
        let caps = need(r.take(ncaps * 4), "capacities")?.as_chunks::<4>().0;

        // Cross-checks: every state, frame, continuation and task index.
        let saved = match saved {
            Some(s) => Some(check_state(
                &mut codes,
                &mut bundled,
                &mut starts,
                s,
                stack.len(),
            )?),
            None => None,
        };
        charge(&mut budget, nconts as u64)?;
        let mut held = vec![false; nconts];
        check_frames(
            &heap,
            &mut codes,
            &mut bundled,
            &mut starts,
            &stack,
            &specials,
            depth,
            saved.as_ref(),
            &conts,
            &mut held,
        )?;
        for t in tasks.iter().flatten() {
            check_frames(
                &heap,
                &mut codes,
                &mut bundled,
                &mut starts,
                &t.stack,
                &t.specials,
                t.depth,
                t.regs(),
                &conts,
                &mut held,
            )?;
        }
        // The free list names each empty slot once, and no slot a frame
        // holds: `cont_put` hands its slots out.
        for &i in &cont_free {
            match (conts.get(i as usize), held.get_mut(i as usize)) {
                (Some(None), Some(h)) if !*h => *h = true,
                _ => return Err(SnapError::Corrupt("continuations")),
            }
        }
        let task_ok = |i: usize| i < tasks.len();
        if (!tasks.is_empty() && !task_ok(cur))
            || !blocked.iter().all(|&i| i == crate::vm::HOLE || task_ok(i))
            || !ready.iter().all(|e| match e {
                Ready::Task(i) => task_ok(*i),
                Ready::Gather(..) | Ready::Call(..) | Ready::Handle(_) => true,
            })
        {
            return Err(SnapError::Corrupt("tasks"));
        }
        if let (Some((_, pc)), Some(st)) = (host_call, &saved)
            && !starts.has(st.ci, &codes[st.ci as usize].code, pc as usize)
        {
            return Err(SnapError::Corrupt("host call"));
        }
        // The registry names every generator in the heap once, which the
        // walk that checks them lists in address order.
        if check {
            check_generators(&heap, &mut codes, &mut bundled, &mut starts, &gens)?;
        } else {
            // The checked image or live template already supplied the registry.
            // A suspended generator needs its instructions before a collection
            // clears its dead registers, even when it never resumes.
            for &v in &gens {
                let r = need(v.as_obj(), "generator")?;
                if heap.get_raw(r, crate::generator::g::STATE) == Ok(crate::generator::g::SUSPENDED)
                {
                    let ci = need(heap.get_raw(r, crate::generator::g::CODE).ok(), "generator")?;
                    bundled.load(&mut codes, need(ci.try_into().ok(), "generator")?)?;
                }
            }
        }
        // `HostArgs` setters cap the depth; keep the cap.
        host_args.max_depth = host_args.max_depth.min(1000);

        let ics_len = n_ics as usize;
        let mut vm = Vm {
            heap,
            stack,
            depth,
            specials,
            frame_base: 0,
            codes: Arc::new(codes),
            code_charge: 0,
            code_kept: 0,
            code_sums: (0, 0, 0),
            ics: Vec::new(),
            globals,
            builtins,
            gver,
            dunders,
            bver: 0,
            types,
            handled,
            exc,
            memory_error,
            sources,
            output: None,
            recursion_limit,
            recursion_ceiling,
            fuel,
            ticks,
            tick_period,
            tick_spare,
            sp_phase,
            fuel_overdraft: 0,
            saved,
            gc_pending,
            fuel_wanted,
            afford: false,
            long_work,
            burned,
            check_before: false,
            long_build: false,
            text_over: core::cell::Cell::new(0),
            work: 0,
            direct_call: false,
            tasks,
            task_hint: 0,
            parked_bytes: 0,
            cur,
            ready,
            blocked,
            blocked_holes,
            gathers,
            gathers_holes,
            iter_left,
            iter_open,
            stop_wait,
            cb_slots,
            callback_queue: 0,
            sched_turns,
            task_counter,
            task_names,
            context,
            escape,
            cancelling,
            run_handled,
            unretrieved,
            loop_flags,
            asyncgens_listed,
            futures,
            seq,
            vclock,
            next_call,
            host_call,
            request: None,
            modules,
            host_modules,
            host_ids,
            park: Arc::new(AtomicBool::new(false)),
            clock: None,
            deadline: None,
            wall_clock: None,
            entropy: None,
            local_zone,
            outside_reads: 0,
            zone_source: None,
            host_args,
            slow,
            shapes,
            next_version,
            prelude_globals,
            tramp,
            missing,
            empty_tuple,
            shared_chars: Vec::new(),
            post,
            completion_text,
            want_text,
            int_bits_text: false,
            host_text: false,
            report_cap: usize::MAX,
            report_cut: false,
            report_steps: 0,
            name_error_built: true,
            gc_stress: false,
            gc_wait,
            gc_owed,
            gc_off: false,
            gc_yield: false,
            oom_wait,
            oom_rerun: u64::MAX,
            oom_rerun_at: u64::MAX,
            oom_defer_at,
            consumed_at: u64::MAX,
            tick_epoch: 0,
            gc_inc_stress: false,
            consumed: Vec::new(),
            partial: None,
            conts,
            cont_free,
            ctors: vec![crate::vm::CTOR_UNKNOWN; NTYPES],
            sbuf: mm_core::StrBuf::new(),
            stdlib,
            std_codes,
            std_globals,
            std_scratch,
            module_cache,
            host_code_in_snapshots: false,
            unpackers,
            synth,
            call_log,
            fresh,
            gens,
            gens_at,
            dead_cache: Vec::new(),
            known_modules: &[],
        };
        if rerun {
            vm.oom_rerun = vm.op_stamp();
        }
        vm.oom_rerun_at = rerun_at;
        // Both of its terms are in the image (the queue, the gathers' LEFT).
        vm.callback_queue = vm.count_callback_queue();
        vm.reserve_code(u64::from(n_ics) * size_of::<crate::vm::Ic>() as u64)
            .map_err(|_| SnapError::TooLarge)?;
        // The queue's spare room too: a restored queue grows as the running
        // one would (`Vm::queue_spare`).
        vm.reserve_scheduler(ntasks, vm.gathers.len(), spare)
            .map_err(|_| SnapError::TooLarge)?;
        // The capacities and charges of the session that wrote the image,
        // so the restored one is charged what that one is.
        vm.reserve_capacities(caps, ics_len, &mut budget)?;
        vm.parked_bytes = vm.tasks.iter().flatten().map(Task::buffer_bytes).sum();
        // The code charge with the capacities reserved above, as the
        // session that wrote the image counts its own.
        vm.code_charge = vm.code_bytes();
        // The debt was counted in the writer's sizes; where this target's
        // charge is smaller, all of it is debt.
        vm.code_kept = vm.code_charge.saturating_sub(code_debt);
        vm.heap
            .set_external_bytes(external)
            .map_err(|_| SnapError::TooLarge)?;
        if check {
            check_steps(&vm)?;
            check_gathers(&vm)?;
        }
        Ok(vm)
    }
}

/// Each tracked gather's counts (`aw::LEFT`) against the run queue:
/// its children still pending are at most its notifications not run,
/// those at most its children, and its entries in the queue number the
/// difference; each entry's child is a settled task, future or gather.
/// Whether that child is one of the gather's is not checked (a pass over
/// the children for each entry): an entry naming another settled
/// awaitable runs as one of the gather's notifications, and fails it
/// with that awaitable's error, as a Python result, never a panic.
fn check_gathers(vm: &Vm) -> Res<()> {
    use crate::aio::aw;
    let word = |v: Value, i: u32| v.as_obj().map_or(0, |r| vm.raw(r, i));
    let place = |g: Value| (word(g, aw::FAILS) >> aw::PLACE_SHIFT) as usize - 1;
    let mut queued = vec![0u64; vm.gathers.len()];
    for e in &vm.ready {
        let Ready::Gather(g, c) = *e else {
            continue;
        };
        let awaitable = matches!(
            c.obj_kind().map(u16::from),
            Some(ty::TASK | ty::FUTURE | ty::GATHER)
        );
        if !awaitable || word(c, aw::STATE) == aw::PENDING {
            return Err(SnapError::Corrupt("run queue"));
        }
        if vm.tracked(g) {
            queued[place(g)] += 1;
        }
    }
    for (k, &g) in vm.gathers.iter().enumerate() {
        if g == Value::UNDEFINED {
            continue;
        }
        if !g.is_obj_kind(ty::GATHER as u8) || !vm.tracked(g) || place(g) != k {
            return Err(SnapError::Corrupt("gathers"));
        }
        let left = word(g, aw::LEFT);
        let (pending, unrun) = (left & aw::LEFT_MASK, left >> aw::UNRUN_SHIFT);
        let children = vm.tuple_len(vm.vslot(g, aw::EXTRA)) as u64;
        if pending > unrun || unrun > children || unrun - pending != queued[k] {
            return Err(SnapError::Corrupt("gathers"));
        }
    }
    Ok(())
}

/// The signature index (`Vm::synth`): each entry a signature code exactly
/// as `_mm.signature` builds it, in strictly ascending key order (its
/// binary search), at most `MAX_SYNTH`.
#[cfg_attr(mm_shipped, optimize(size))]
fn read_synth(r: &mut Reader<'_>, codes: &[Arc<CodeObj>]) -> Res<Vec<u32>> {
    let n = need(r.count(4), "signatures")?;
    if n > crate::mmmod::MAX_SYNTH {
        return Err(SnapError::Corrupt("signatures"));
    }
    let mut synth = Vec::with_capacity(n);
    let mut prev: Option<(u32, &[Arc<str>])> = None;
    for _ in 0..n {
        let ci = need(r.u32(), "signatures")?;
        let Some(c) = codes.get(ci as usize).map(|c| &c.code) else {
            return Err(SnapError::Corrupt("signatures"));
        };
        let Some((_, names)) = c.varnames.split_last() else {
            return Err(SnapError::Corrupt("signatures"));
        };
        let argc = c.params.argcount as usize;
        let shown: Vec<String> = names.iter().map(|s| String::from(&**s)).collect();
        if argc > names.len()
            || names.len() > crate::mmmod::MAX_PARAMS
            || **c != crate::mmmod::signature_code(&shown, argc)
        {
            return Err(SnapError::Corrupt("signatures"));
        }
        let key = (c.params.argcount, names);
        if prev.is_some_and(|p| p >= key) {
            return Err(SnapError::Corrupt("signatures"));
        }
        prev = Some(key);
        synth.push(ci);
    }
    Ok(synth)
}

/// A chunked native's state (`NSTATE`, `nstep.rs`) holds the progress its
/// next step trusts as an index, a loop's stride or a slice's bounds in
/// range for the data it names, as every state the step writes does: the
/// words whose forged values crashed or hung a release build. A state that
/// does not name its own operation's layout is left to the step
/// (`bad_step`), and the bytes a build has written are not compared with
/// its sources.
#[cfg_attr(mm_shipped, optimize(size))]
fn check_steps(vm: &Vm) -> Res<()> {
    use crate::nstep::STEPS;
    let heap = &vm.heap;
    let mut ok = true;
    heap.for_each_object(&mut |r, h| {
        if !ok || h.type_id() != ty::NSTATE {
            return;
        }
        let Ok((h, words)) = heap.object_words(r) else {
            return;
        };
        let (vals, raw) = words.split_at((h.nvals() as usize).min(words.len()));
        let Some(o) = raw.first().and_then(|&o| u8::try_from(o).ok()) else {
            return;
        };
        let layout = STEPS.get(o as usize).map(|s| (s.1 as usize, s.2 as usize));
        if layout != Some((vals.len(), raw.len())) {
            return;
        }
        let v = |i: usize| {
            vals.get(i)
                .and_then(|&b| Value::from_bits(b))
                .unwrap_or(Value::NONE)
        };
        ok = step_ok(vm, o, &v, &raw[1..]);
    });
    if ok {
        Ok(())
    } else {
        Err(SnapError::Corrupt("native step"))
    }
}

/// Whether the new tables `nk`, `nv` of a bulk rebuild of `out` hold
/// what it moved, the live entries of `out`'s table before `j`: as many
/// keys (a set, whose `n` is 0), or `n` entries in as many index slots (a
/// dict). Restarted from an earlier `j`, it would put them in again.
pub(crate) fn rebuild_ok(vm: &Vm, out: Value, nk: Value, nv: Value, j: u64, n: u64) -> bool {
    let empty = Value::UNDEFINED.to_bits();
    let live = |w: &[u64], step: usize| w.iter().step_by(step).filter(|&&x| x != empty).count();
    let j = usize::try_from(j).unwrap_or(usize::MAX);
    if let Some((_, ok, _, _)) = vm.set_parts(out) {
        let new = nk.as_obj().and_then(|k| vm.heap.slot_words(k).ok());
        let old = vm.heap.slot_words(ok).ok().and_then(|o| o.get(..j));
        return n == 0 && matches!((old, new), (Some(o), Some(k)) if live(o, 1) == live(k, 1));
    }
    let (Some(o), Some(ix)) = (
        out.as_obj().and_then(|r| vm.dv(r)),
        nv.as_obj().and_then(|i| vm.heap.raw_words(i).ok()),
    ) else {
        return false;
    };
    #[allow(clippy::cast_possible_truncation, reason = "the two halves")]
    let filled: usize = ix
        .get(..ix.len() / 2)
        .unwrap_or(&[])
        .iter()
        .map(|&w| usize::from(w as u32 != 0) + usize::from((w >> 32) as u32 != 0))
        .sum();
    let old = vm.heap.slot_words(o.entries).ok();
    let moved = old
        .and_then(|e| e.get(..j.checked_mul(2)?))
        .map(|e| live(e, 2));
    usize::try_from(n).is_ok_and(|n| moved == Some(n) && filled == n)
}

/// [`check_steps`] for one state of chunked native `o`: its value slots
/// `v` and its raw words after the operation word, `w`.
#[cfg_attr(mm_shipped, optimize(size))]
fn step_ok(vm: &Vm, o: u8, v: &dyn Fn(usize) -> Value, w: &[u64]) -> bool {
    use crate::nstep::op;
    let w = |i: usize| w.get(i).copied().unwrap_or(u64::MAX);
    let n = |x: Value| vm.job_data(x).len() as u64;
    // Positions and counts in an object, which a step takes as a `usize`
    // without a check: within the largest object, as every state the steps
    // write, so a step indexes alike on 32- and 64-bit targets.
    let small = |i: &[usize]| i.iter().all(|&i| w(i) <= mm_core::MAX_OBJECT_BYTES);
    // The chars in the first `o` bytes of a str build's block `out`, if
    // it has `o` bytes and they are UTF-8, its last char begun (counted)
    // but cut unless `whole`: a copy goes on from it ([`crate::strmeth`]
    // checks each seam) and the `str` it becomes counts them.
    let built = |out: Value, o: u64, whole: bool| {
        let o = usize::try_from(o).ok()?;
        let t = vm.heap.bytes(out.as_byte_ref()?).ok()?.get(..o)?;
        let (s, cut) = match core::str::from_utf8(t) {
            Ok(s) => (s, 0),
            Err(e) if !whole && e.error_len().is_none() => {
                (core::str::from_utf8(t.get(..e.valid_up_to())?).ok()?, 1)
            }
            Err(_) => return None,
        };
        Some(s.chars().count() as u64 + cut)
    };
    match o {
        // A sort's list for `list.sort` (or `None`): its items go back
        // there, a list's buffer and length.
        op::SORT_KEYS | op::SORT_DONE => v(3) == Value::NONE || v(3).is_obj_kind(ty::LIST as u8),
        // A bulk operation (`prog, pc, i, j, n, res`) whose new table has
        // no hashes or index yet (`nv` `None`, after its keys or entries
        // `nk`) has put nothing in it: a set's keys are marked empty a
        // slice at a time (`j` so far), a dict's entries come with the
        // index (`j` and `n` 0). The rebuild that follows fills it, and a
        // full table is probed for ever.
        op::BULK if v(7) == Value::NONE && v(6).is_obj_kind(ty::BUF as u8) => {
            let p = w(0).checked_shr(16 * (w(1) & 7) as u32).unwrap_or(0);
            if Vm::is_set(v(((p >> 4) & 7) as usize)) {
                let keys = v(6).as_obj().and_then(|k| vm.heap.slot_words(k).ok());
                let done = usize::try_from(w(3)).unwrap_or(usize::MAX);
                keys.and_then(|k| k.get(..done))
                    .is_some_and(|k| k.iter().all(|&x| x == Value::UNDEFINED.to_bits()))
            } else {
                w(3) == 0 && w(4) == 0
            }
        }
        // One with hashes or an index: what it moved is in them.
        op::BULK if v(6).is_obj_kind(ty::BUF as u8) && v(7).is_obj_kind(ty::RAW as u8) => {
            let p = w(0).checked_shr(16 * (w(1) & 7) as u32).unwrap_or(0);
            rebuild_ok(vm, v(((p >> 4) & 7) as usize), v(6), v(7), w(3), w(4))
        }
        op::SEQ_BUILD => {
            // `total, seg, pos, rep, at, dstep`, a spare, then five words a
            // segment, its `count` and `reps` last: item `pos` of segment
            // `seg` is next (`pos` wraps to 0 at `count`), and counts and
            // repeats fit an object.
            let word = |k: u64, i: usize| w(7 + 5 * usize::try_from(k).unwrap_or(3) + i);
            let fit = |k| u32::try_from(word(k, 3)).is_ok() && u32::try_from(word(k, 4)).is_ok();
            (0..3).all(fit) && (w(1) >= 3 || w(2) == 0 || w(2) < word(w(1), 3))
        }
        // `kind, i, stop, c`: the items counted were passed.
        op::SEQ_SCAN => w(3) <= w(1),
        // `sv, ctxs, aux`, then a growing object's new entries and index.
        #[cfg(feature = "json")]
        op::JSON_SCAN => crate::json::grow_tables_ok(vm, v(1), (v(3), v(4)), &w),
        op::STR_FILL | op::STR_JOIN => {
            // `part, its bytes done, out, chars`: what is done of a part
            // was written, and a str's chars are those written. A join
            // sizes (`phase` 0, items counted in `w[6]`, nothing built),
            // then builds its parts, a separator between two items; its
            // slot 2 is `None` for a str, a fill's `False`.
            let (j, parts) = (w(0), vm.seq_words(v(1)).len() as u64);
            let (parts_ok, text) = if o == op::STR_FILL {
                (j <= parts / 3, v(2) != Value::TRUE)
            } else if w(4) == 0 {
                (w(6) <= parts && j == 0 && w(2) == 0 && w(3) == 0, false)
            } else {
                (j <= 2 * parts, v(2) == Value::NONE)
            };
            parts_ok
                && w(1) <= w(2)
                && w(3) <= w(2)
                && (!text || built(v(3), w(2), false) == Some(w(3)))
        }
        op::STR_REPLACE => {
            // `phase, at, out, matches, most, all matches, chars`: the
            // matches counted fit the text before `at` (an empty pattern's
            // are counted before the text is, to `at` = its end).
            let (s, old) = (v(0), v(1));
            let (len, pl, cs) = (n(s), n(old), vm.str_len(s) as u64);
            let fits = |m: u64, upto: u64| {
                m.checked_mul(pl).is_some_and(|b| b <= upto)
                    && m.checked_mul(vm.str_len(old) as u64)
                        .is_some_and(|c| c <= cs)
            };
            let (at, left, all) = (w(1), w(3), w(5));
            // The result's chars, known when the build starts.
            let times = |x: Value| (vm.str_len(x) as u64).checked_mul(all);
            let chars = times(v(2))
                .and_then(|a| a.checked_add(cs))
                .and_then(|c| c.checked_sub(times(old)?));
            at <= len
                && if w(0) == 0 {
                    fits(left, at) && (pl > 0 || (at == len && left <= cs + 1))
                } else {
                    left <= all
                        && (fits(all, len) || (pl == 0 && all <= cs + 1))
                        && chars == Some(w(6))
                        && built(v(3), w(2), false).is_some()
                }
        }
        op::STR_SPLIT => {
            // `piece start, search position, splits left, mode, inside,
            // items reversed, long piece copied`: both in the text, a
            // piece's start before the search (from the right, its end
            // after the search, or less than a separator before: a match
            // may end past the window searched), the position `u64::MAX` (the end) only while a long
            // piece is copied into slot 2 (`w[6]` its bytes and, past bit
            // 32, its chars); a reversal at most half the list along. A
            // piece between spaces was found with a split left.
            use crate::strmeth::sm;
            let (len, rev) = (n(v(0)), w(3) >= sm::RSEP);
            let (i, j, mode) = (w(0), w(1), w(3));
            let piece =
                v(2) == Value::NONE || built(v(2), w(6) & 0xFFFF_FFFF, false) == Some(w(6) >> 32);
            mode <= sm::RSPACE
                && i <= len
                && (j == u64::MAX && v(2) != Value::NONE
                    || j <= len
                        && if rev {
                            j <= i.saturating_add(n(v(1)))
                        } else {
                            i <= j
                        })
                && piece
                && w(5) <= vm.list_len(v(3)) as u64 / 2 + 1
                && (!matches!(mode, sm::SPACE | sm::RSPACE) || w(4) == 0 || w(2) > 0)
        }
        // A map (`phase, at, out, chars, state, bytes, part`) reads the
        // text from a char's start, `at`; it sizes (`phase` 0, nothing
        // written) or builds, `w[2]` bytes and `w[3]` chars (whole ones) of
        // its block (slot 3) written, and its byte counts stay counts. A
        // lowering's pending `σ` (`part`, its offset plus one) was written
        // (a `translate` or `expandtabs` keeps the part of a char's
        // replacement written there). An extended slice `(step, first,
        // picks)`: two picks are a step apart in the text, so a step passes
        // no more chars than it has. While sizing, `part` is only a flag.
        op::STR_MAP => {
            use crate::strmeth::mk;
            let kind = v(2).as_int();
            let lowering = matches!(
                kind,
                Some(mk::LOWER | mk::SWAPCASE | mk::TITLE | mk::CAPITALIZE)
            );
            let ok = if kind == Some(mk::SLICE) {
                vm.seq_words(v(1))
                    .first()
                    .and_then(|&b| Value::from_bits(b))
                    .and_then(Value::as_int)
                    .is_some_and(|step| step.unsigned_abs() <= n(v(0)))
            } else {
                let b = v(3).as_byte_ref().and_then(|b| vm.heap.bytes(b).ok());
                let at = usize::try_from(w(6)).ok().and_then(|p| p.checked_sub(1));
                let sigma = at.and_then(|a| b?.get(a..a.checked_add(2)?));
                !lowering
                    || w(0) == 0
                    || w(6) == 0
                    || (w(6) < w(2) && sigma == Some("σ".as_bytes()))
            };
            let made = if w(0) == 0 {
                w(2) == 0 && w(3) == 0
            } else {
                built(v(3), w(2), true) == Some(w(3))
            };
            let at = usize::try_from(w(1)).unwrap_or(usize::MAX);
            let start =
                !Vm::is_str(v(0)) || vm.job_data(v(0)).get(at).is_none_or(|&c| c & 0xC0 != 0x80);
            ok && made && start && w(1) <= n(v(0)) && w(5) <= 1 << 40
        }
        // A strip (`at, end, ends to go, chars read`): its window and its
        // cursor in `chars` at char boundaries, and its bitmap (slot 3,
        // none for no `chars`) of the size the operands give it, neither
        // operand's own block.
        op::STR_SCAN if v(2).as_int() == Some(crate::strmeth::sk::STRIP) => {
            let (s, chars) = (v(0), v(1));
            let len = n(chars);
            let boundary = |s, at| {
                vm.job_data(s)
                    .get(at as usize)
                    .is_none_or(|b| b & 0xc0 != 0x80)
            };
            let bitmap = || {
                let b = v(3).as_byte_ref()?;
                let size = crate::strmeth::strip_bitmap(vm, s, chars).0;
                let own = [vm.data_block(s), vm.data_block(chars)];
                (vm.heap.bytes(b).ok()?.len() == size && !own.contains(&Some(b))).then_some(())
            };
            Vm::is_str(s)
                && (chars == Value::NONE || Vm::is_str(chars))
                && w(0) <= w(1)
                && w(1) <= n(s)
                && boundary(s, w(0))
                && boundary(s, w(1))
                && w(2) <= 3
                && w(3) <= len
                && boundary(chars, w(3))
                && w(4) == 0
                && if len == 0 {
                    v(3) == Value::NONE
                } else {
                    bitmap().is_some()
                }
        }
        // A fold (`kind, stage, totals`): a `sum`'s stage is one of its
        // four (`stage + 1` is the next).
        op::FOLD => w(1) <= 3,
        // An `int()` text (`kind` its base plus one; then `at` and the
        // scan's words `flags, digits, significant, start, end`, then the
        // conversion's `idx, len`): its digits counted in the text, the
        // significant and converted ones among them.
        // A power-of-two digit text (`kind` `DIGITS`, then `at, done`)
        // becomes a `str` of its whole block, which holds ASCII digits and
        // zeros; it writes down from `at` in it, and has read `done` of the
        // int's bytes.
        op::NUM_TEXT if w(0) == crate::builtins::DIGITS => {
            v(1).as_byte_ref()
                .and_then(|b| vm.heap.bytes(b).ok())
                .is_some_and(|b| b.is_ascii() && w(1) <= b.len() as u64)
                && small(&[2])
        }
        op::NUM_TEXT => w(0) == 0 || (w(3) <= 1 << 40 && w(4) <= w(3) && w(7) <= w(3)),
        #[cfg(feature = "datetime")]
        op::STRPTIME => crate::datetimemod::strptime_state(vm, [v(0), v(1), v(2)], w(0)),
        op::BYTES => {
            // Any byte job (its kind, then its words): a pass starts at or
            // before its position and its end, the output's bytes counted
            // or written stay a count, a char takes a byte, and a `str`
            // result's chars are those written (`W_PASS` 1).
            use crate::bytes::{R_STR, W_CH, W_END, W_I, W_I0, W_O, W_PASS, W_RES};
            let j = |i: usize| w(i + 1);
            let text = j(W_RES) == R_STR && j(W_PASS) != 0;
            small(&[W_I + 1, W_END + 1, W_I0 + 1])
                && j(W_I0) <= j(W_I)
                && j(W_I0) <= j(W_END)
                && j(W_O) <= 1 << 40
                && j(W_CH) <= j(W_O)
                && (!text || built(v(2), j(W_O), true) == Some(j(W_CH)))
                && byte_job_ok(vm, v, &w)
        }
        _ => true,
    }
}

/// [`step_ok`] for a byte job (`crate::bytes::Job`): `w` from its kind.
#[cfg_attr(mm_shipped, optimize(size))]
fn byte_job_ok(vm: &Vm, v: &dyn Fn(usize) -> Value, w: &dyn Fn(usize) -> u64) -> bool {
    use crate::bytearray::fin;
    use crate::bytes::{S_POS, W_A, W_C, W_I, W_S, cut, job};
    let kind = w(0);
    let w = |i: usize| w(i + 1);
    match u8::try_from(kind).unwrap_or(u8::MAX) {
        // Copies of the source, doubling: a copy has a byte.
        job::REPEAT => !vm.job_data(v(0)).is_empty(),
        job::DEL_STRIDE => {
            // `first, step, len, write`: the bytes kept move down, to
            // before the one read, from the first deleted, and a count of
            // bytes deleted. Not against the bytearray's length: a state
            // its cell's cancel left in the heap names a bytearray later
            // cells may shrink, and its step is safe for any count.
            let (first, step, i) = (w(W_A), w(W_A + 1), w(W_I));
            step >= 1
                && i64::try_from(step).is_ok()
                && first <= i
                && w(W_A + 3) <= i
                && w(W_A + 2) <= mm_core::MAX_OBJECT_BYTES
        }
        // A SHAKE digest's output: what is written of it.
        #[cfg(feature = "hashlib")]
        job::SQUEEZE => w(crate::bytes::W_O) <= w(crate::bytes::W_END),
        // `[lo:hi] = src` after the growth: `lo` in the bytearray.
        job::GROW if w(W_A) == fin::SPLICE => w(W_A + 1) <= vm.bytearray_data(v(3)).len() as u64,
        // Tuple affixes: the fixed window, and the compared bytes of its
        // current candidate. A discarded job can still name a bytearray
        // a later cell resized; the step checks its actual slice on use.
        job::AFFIX => {
            use crate::bytes::{NO_AFFIX_WINDOW, W_END, W_I0, W_O};
            let (src, pats) = (v(0), v(1));
            let (lo, hi, done) = (w(W_I), w(W_END), w(W_O));
            let i = w(W_C);
            let p = vm.unbx(vm.vslot(pats, i as u32));
            let len = |s| {
                if Vm::is_bytearray(s) {
                    mm_core::MAX_OBJECT_BYTES
                } else {
                    vm.job_data(s).len() as u64
                }
            };
            (Vm::is_str(src) || Vm::is_bytes_like(src))
                && pats.is_obj_kind(ty::TUPLE as u8)
                && i < vm.tuple_len(pats) as u64
                && w(W_A) <= 1
                && w(W_I0) == 0
                && ((lo as usize, hi as usize) == NO_AFFIX_WINDOW || lo <= hi && hi <= len(src))
                && (done == 0 || done < len(p) && done <= hi.saturating_sub(lo))
        }
        #[cfg(feature = "struct")]
        job::STRUCT => crate::structmod::job_ok(vm, v, &w),
        #[cfg(feature = "struct")]
        job::STRIDE if w(W_A + 1) == 1 => {
            use crate::bytes::{R_BYTES, W_O, W_RES};
            Vm::is_bytes_like(v(0))
                && w(W_A) <= mm_core::MAX_OBJECT_BYTES
                && w(W_A + 2) <= 1
                && w(W_RES) == R_BYTES
                && v(2)
                    .as_byte_ref()
                    .and_then(|b| vm.heap.bytes(b).ok())
                    .is_some_and(|b| w(W_O) <= b.len() as u64)
        }
        // A strict decode's error (`job::DECODE_ERR`): its source read to
        // its end from `W_I0`, its copy not yet allocated (nothing written)
        // or a block of that many bytes, its reason a `str`, and the bytes
        // it names inside what the copy holds.
        job::DECODE_ERR => {
            use crate::bytes::{W_END, W_I0, W_O};
            let len = w(W_END).saturating_sub(w(W_I0));
            let block = match v(2) {
                Value::NONE => w(W_O) == 0,
                b => b
                    .as_byte_ref()
                    .and_then(|b| vm.heap.bytes(b).ok())
                    .is_some_and(|b| b.len() as u64 == len && w(W_O) <= len),
            };
            w(W_END) == vm.job_data(v(0)).len() as u64
                && block
                && Vm::is_str(v(1))
                && w(W_A + 1) <= w(W_A + 2)
                && w(W_A + 2) <= len
                && w(W_C) <= 1
        }
        // A split's pieces counted (`W_C`) are a count; at whitespace, a
        // piece found starts at or before where the search for its end
        // goes on.
        job::SPLIT => {
            w(W_C) <= 1 << 40
                && (w(W_A) != cut::SPACE || w(W_C + 2) == 0 || w(W_I) <= w(W_S + S_POS))
        }
        _ => true,
    }
}

/// The code index of function `f`, if it is a function.
#[cfg_attr(mm_shipped, optimize(size))]
fn func_code(heap: &Heap, f: Value) -> Option<u64> {
    f.as_obj()
        .filter(|_| f.is_obj_kind(ty::FUNC as u8))
        .and_then(|f| heap.get_raw(f, crate::vm::fn_slot::CODE).ok())
}

/// Does a frame of `ci` belong to `coro`, the generator or coroutine its
/// state names (`None` for a plain frame)? Its generator or coroutine runs,
/// and runs that code: a yield or an `await` keeps the frame's pc for the
/// code the object names, where the next resume runs it.
#[cfg_attr(mm_shipped, optimize(size))]
fn frame_owner_ok(heap: &Heap, coro: Value, ci: u32) -> bool {
    use crate::{aio::coro, generator::g};
    let Some(r) = coro.as_obj() else {
        return coro == Value::NONE;
    };
    let raw = |i| heap.get_raw(r, i).ok();
    match coro.obj_kind().map(u16::from) {
        Some(ty::GEN | ty::AGEN) => {
            raw(g::STATE) == Some(g::RUNNING) && raw(g::CODE) == Some(u64::from(ci))
        }
        Some(ty::CORO) => {
            raw(coro::STATE).map(|s| s & 3) == Some(coro::RUNNING)
                && heap
                    .get(r, coro::FUNC)
                    .is_ok_and(|f| func_code(heap, f) == Some(u64::from(ci)))
        }
        _ => false,
    }
}

/// Checks the registry against the heap, each generator's code, saved
/// registers and yield position, and each suspended coroutine's `AWAIT`.
#[cfg_attr(mm_shipped, optimize(size))]
fn check_generators(
    heap: &Heap,
    codes: &mut [Arc<CodeObj>],
    bundled: &mut Bundled<'_>,
    starts: &mut Starts,
    registry: &[Value],
) -> Res<()> {
    use crate::generator::g;
    let mut ok = true;
    let mut gens = Vec::new();
    let mut room = gens.try_reserve_exact(registry.len()).is_ok();
    heap.for_each_object(&mut |r, h| {
        if !matches!(h.type_id(), ty::GEN | ty::AGEN) || !ok || !room {
            return;
        }
        if gens.try_reserve(1).is_err() {
            room = false;
            return;
        }
        #[allow(clippy::cast_possible_truncation, reason = "type ids are below 256")]
        gens.push(Value::from_ref(r, h.type_id() as u8).to_bits());
        let state = heap.get_raw(r, g::STATE).unwrap_or(u64::MAX);
        let ypc = heap
            .get_raw(r, g::YPC)
            .map_or(u64::MAX, |y| y & !g::CLEARED);
        let ci = heap.get_raw(r, g::CODE).ok();
        let func = heap.get(r, g::FUNC).ok().and_then(|f| func_code(heap, f));
        let ci = ci
            .filter(|_| ci == func)
            .and_then(|i| u32::try_from(i).ok());
        // A suspended generator's code has run: a resume and a collection
        // (`Vm::clear_saved`) read its instructions without a check. A
        // created one's first fetch decodes it (`Rare::Load`).
        if state == g::SUSPENDED
            && let Some(i) = ci
            && bundled.load(codes, i).is_err()
        {
            ok = false;
            return;
        }
        let code = ci.and_then(|i| codes.get(i as usize).map(|c| (i, c)));
        let regs_ok = |c: &CodeObj| {
            heap.get(r, g::REGS)
                .ok()
                .filter(|v| v.is_obj_kind(ty::BUF as u8))
                .and_then(Value::as_obj)
                .and_then(|b| heap.header(b).ok())
                .is_some_and(|bh| bh.type_id() == ty::BUF && bh.nvals() >= c.code.nregs)
        };
        ok = match (state, code) {
            (g::SUSPENDED, Some((i, c))) => {
                regs_ok(c)
                    && usize::try_from(ypc).is_ok_and(|p| {
                        starts.has(i, &c.code, p)
                            && matches!(
                                c.code.bytecode.get(p).map(|&w| crate::interp::op_of(w)),
                                Some(op::YIELD | op::YIELD_FROM)
                            )
                    })
            }
            (g::CREATED | g::RUNNING, Some((_, c))) => regs_ok(c),
            (g::DONE, Some(_)) => true,
            _ => false,
        };
    });
    if !room {
        return Err(SnapError::TooLarge);
    }
    if !ok {
        return Err(SnapError::Corrupt("generator"));
    }
    let mut listed: Vec<u64> = registry.iter().map(|v| v.to_bits()).collect();
    for words in [&mut gens, &mut listed] {
        crate::ops::heap_sort_by(words, |a, b| a < b);
    }
    if gens != listed {
        return Err(SnapError::Corrupt("generators"));
    }
    // A coroutine a `send()` suspended resumes at its `AWAIT`.
    heap.for_each_object(&mut |r, h| {
        if h.type_id() != ty::CORO || !ok {
            return;
        }
        if heap.get_raw(r, crate::aio::coro::STATE).unwrap_or(u64::MAX) & 3
            != crate::aio::coro::SUSPENDED
        {
            return;
        }
        let pc = heap.get_raw(r, crate::aio::coro::PC).unwrap_or(u64::MAX);
        let ci = heap
            .get(r, crate::aio::coro::FUNC)
            .ok()
            .and_then(|f| func_code(heap, f))
            .and_then(|ci| u32::try_from(ci).ok());
        if let Some(i) = ci
            && bundled.load(codes, i).is_err()
        {
            ok = false;
            return;
        }
        let code = ci.and_then(|i| codes.get(i as usize).map(|c| (i, c)));
        ok = code.is_some_and(|(i, c)| {
            usize::try_from(pc).is_ok_and(|p| {
                starts.has(i, &c.code, p)
                    && c.code.bytecode.get(p).map(|&w| crate::interp::op_of(w)) == Some(op::AWAIT)
            })
        });
    });
    if ok {
        Ok(())
    } else {
        Err(SnapError::Corrupt("coroutine"))
    }
}

#[cfg(test)]
#[path = "snapshot_tests.rs"]
mod tests;
