//! `json`: the native encoder and scanner behind the prelude's `dumps` and
//! `loads` (docs/spec.md §12). Both walk the data iteratively with an
//! explicit stack, so nesting depth costs heap, not Rust stack. Instances of
//! `str`, `int`, `float`, `list`, `tuple` and `dict` subclasses encode as the
//! value they extend, as CPython's C encoder does. Anything else (a
//! `default=` call, an error with its `__notes__`, a `dict` subclass with its
//! own `items`) needs Python: the encoder reports that and the prelude
//! re-encodes in Python.
//!
//! Both go on in steps (docs/spec.md §5, "Bounded native steps"): an
//! encoding that spends `STEP_WORK` keeps its open containers in an
//! `NSTATE` object (`nstep::op::JSON_DUMP`) and its text so far as byte
//! blocks, joined into the result a slice a step; a long str is written a
//! slice at a time; a dict under `sort_keys` with many keys has its keys
//! sorted by the prelude's `_json_steps` (`list.sort`, itself in steps).

use crate::nstep::{STEP_WORK, op};
use crate::scratch::{NativeIds, NativeText as Output, NativeValues, NativeVec};
use crate::text::{float_repr, i64_digits};
use crate::types::{T, ty};
use crate::vm::{CallArgs, Raised, Vm};
use alloc::{format, string::String};
use core::fmt::Write as _;
use mm_core::{Ref, Scratch, StrBuf, Value};

/// Deepest nesting either direction follows before `RecursionError`.
const MAX_DEPTH: usize = 100_000;

/// A str longer than this is a level of its own, written a slice of
/// [`STR_SLICE`] bytes a time.
const STR_LONG: usize = 1 << 12;
const STR_SLICE: usize = 1 << 16;

/// A dict under `sort_keys` with more keys has them sorted by the prelude.
const SORT_NATIVE: usize = 1 << 12;

/// The work of a float's text (`float_repr`, about a microsecond).
const FLOAT_WORK: u64 = 96;

/// The encoder's output. Arena strings are appended as they are: their
/// UTF-8 is never validated or decoded again, and the finished text becomes
/// a `str` without another pass. Its capacity is charged to the memory
/// budget while it grows.
#[derive(Default)]
struct Out {
    buf: StrBuf,
    charge: Option<Scratch>,
}

impl Out {
    fn reserve(&mut self, vm: &mut Vm, additional: usize) -> Result<(), Raised> {
        let needed = self.buf.len().saturating_add(additional);
        if needed <= self.buf.capacity() {
            return Ok(());
        }
        self.grow(vm, needed)
    }

    #[inline(never)]
    fn grow(&mut self, vm: &mut Vm, needed: usize) -> Result<(), Raised> {
        let old = self.buf.capacity();
        let capacity = crate::scratch::grown(old, needed, 64, vm.heap.available_bytes());
        let res = match &mut self.charge {
            Some(charge) => charge.resize(&vm.heap, capacity),
            None => vm.heap.reserve_scratch(capacity).map(|charge| {
                self.charge = Some(charge);
            }),
        };
        vm.hx(res)?;
        let res = self.buf.try_reserve_exact(capacity - self.buf.len());
        if res.is_err()
            && let Some(charge) = &mut self.charge
        {
            let _ = charge.resize(&vm.heap, old);
        }
        vm.hx(res)
    }

    fn push(&mut self, vm: &mut Vm, s: &str) -> Result<(), Raised> {
        self.reserve(vm, s.len())?;
        self.buf.push_str(s);
        Ok(())
    }

    fn push_int(&mut self, vm: &mut Vm, i: i64) -> Result<(), Raised> {
        let mut digits = [0u8; 20];
        let k = i64_digits(i, &mut digits);
        let d = digits.get(k..).unwrap_or(&[]);
        self.reserve(vm, d.len())?;
        let res = self.buf.push_ascii(d);
        vm.hx(res)
    }

    /// Appends `str` `v` as a JSON string literal, or nothing and `false`
    /// when it is longer than [`STR_LONG`] (written a slice a time).
    /// Text with nothing to escape (the common case) is copied straight
    /// from the arena.
    fn quote_value(&mut self, vm: &mut Vm, v: Value, ascii: bool) -> Result<bool, Raised> {
        let Some(r) = v.as_obj() else {
            self.quote(vm, "", ascii)?;
            return Ok(true);
        };
        let plain = vm.heap.str_bytes(r).map(|b| {
            let ok = b.len() <= STR_LONG
                && b.iter()
                    .all(|&c| c >= 0x20 && c != b'"' && c != b'\\' && (!ascii || c < 0x7f));
            (b.len(), ok)
        });
        let (len, ok) = vm.hx(plain)?;
        if len > STR_LONG {
            return Ok(false);
        }
        if !ok {
            self.quote_text(vm, v, ascii)?;
            return Ok(true);
        }
        self.reserve(vm, len.saturating_add(2))?;
        self.buf.push_str("\"");
        let res = self.buf.push_ref(&vm.heap, r);
        vm.hx(res)?;
        self.buf.push_str("\"");
        Ok(true)
    }

    /// [`Out::quote`] of `str` `v`'s text, which has something to escape.
    #[cold]
    #[inline(never)]
    fn quote_text(&mut self, vm: &mut Vm, v: Value, ascii: bool) -> Result<(), Raised> {
        let s = String::from(vm.str_ref(v));
        self.quote(vm, &s, ascii)
    }

    fn quote(&mut self, vm: &mut Vm, s: &str, ascii: bool) -> Result<(), Raised> {
        self.push(vm, "\"")?;
        self.escape(vm, s, ascii)?;
        self.push(vm, "\"")
    }

    /// Appends the body of a JSON string literal for `s`.
    fn escape(&mut self, vm: &mut Vm, s: &str, ascii: bool) -> Result<(), Raised> {
        let len = escaped_len(s, ascii);
        self.reserve(vm, len)?;
        // Escapes can make the literal six times the text: the staging
        // copy is admitted too, at its exact size.
        let res = vm.heap.reserve_scratch(len);
        let _staged = vm.hx(res)?;
        let mut t = String::new();
        if t.try_reserve_exact(len).is_err() {
            return Err(vm.err(T::MemoryError, ""));
        }
        push_escaped(&mut t, s, ascii, len);
        self.buf.push_str(&t);
        Ok(())
    }

    fn len(&self) -> usize {
        self.buf.len()
    }

    /// The text as a `str`, with no second pass over it.
    fn finish(self, vm: &mut Vm) -> Result<Value, Raised> {
        let n = self.buf.len();
        if n <= 1 {
            return vm.intern(core::str::from_utf8(self.buf.as_bytes()).unwrap_or(""));
        }
        if n >= 64 {
            vm.burn_result(n, true);
        }
        let res = vm.heap.new_str_from(&self.buf);
        Ok(Value::from_ref(vm.hx(res)?, ty::STR as u8))
    }
}

fn escaped_len(s: &str, ascii: bool) -> usize {
    s.chars().fold(0usize, |n, c| {
        n.saturating_add(match c {
            '"' | '\\' | '\n' | '\r' | '\t' | '\u{8}' | '\u{c}' => 2,
            c if c < ' ' => 6,
            c if ascii && c >= '\u{7f}' => 6 * c.len_utf16(),
            c => c.len_utf8(),
        })
    })
}

/// Appends the body of `s`'s JSON string literal (CPython's
/// `encode_basestring` and `encode_basestring_ascii`).
fn push_escaped(out: &mut String, s: &str, ascii: bool, escaped_len: usize) {
    if escaped_len == s.len() {
        // Escaping only expands. Reuse the sizing pass rather than scanning
        // unescaped text again for ASCII and control characters.
        out.push_str(s);
    } else if ascii && s.bytes().any(|b| b >= 0x7f) {
        for c in s.chars() {
            match c {
                c if (c as u32) < 0x7f => push_ascii_escaped(out, c.encode_utf8(&mut [0; 4])),
                c => {
                    let mut units = [0u16; 2];
                    for u in c.encode_utf16(&mut units) {
                        let _ = write!(out, "\\u{u:04x}");
                    }
                }
            }
        }
    } else {
        push_ascii_escaped(out, s);
    }
}

/// The body of a JSON string literal for `s`: quotes, backslashes and
/// control characters escaped, everything else (non-ASCII included) copied.
fn push_ascii_escaped(out: &mut String, s: &str) {
    let mut last = 0;
    for (i, &c) in s.as_bytes().iter().enumerate() {
        let esc = match c {
            b'"' => "\\\"",
            b'\\' => "\\\\",
            b'\n' => "\\n",
            b'\r' => "\\r",
            b'\t' => "\\t",
            8 => "\\b",
            12 => "\\f",
            c if c < 0x20 => "",
            _ => continue,
        };
        out.push_str(s.get(last..i).unwrap_or(""));
        if esc.is_empty() {
            let _ = write!(out, "\\u{c:04x}");
        } else {
            out.push_str(esc);
        }
        last = i + 1;
    }
    out.push_str(s.get(last..).unwrap_or(""));
}

/// A float's JSON text.
fn float_text(vm: &mut Vm, f: f64, allow_nan: bool) -> Result<String, Raised> {
    if f.is_finite() {
        return Ok(float_repr(f));
    }
    if !allow_nan {
        let msg = format!(
            "Out of range float values are not JSON compliant: {}",
            float_repr(f)
        );
        return Err(vm.err(T::ValueError, &msg));
    }
    Ok(String::from(if f.is_nan() {
        "NaN"
    } else if f > 0.0 {
        "Infinity"
    } else {
        "-Infinity"
    }))
}

fn is_int(v: Value) -> bool {
    v.as_int().is_some() || v.is_obj_kind(ty::INT as u8)
}

/// `_json_str(s, ensure_ascii)`.
pub(crate) fn pl_json_str(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let ascii = vm.truthy(vm.arg(&a, 1));
    let s = vm.unbx(vm.arg(&a, 0));
    let mut out = Out::default();
    if !out.quote_value(vm, s, ascii)? {
        let enc = Enc {
            indent: None,
            key_sep: String::new(),
            item_sep: String::new(),
            flags: if ascii { ASCII } else { 0 },
        };
        return dump_start(vm, &enc, s, Value::NONE, Value::NONE, Value::NONE);
    }
    vm.burn(out.len() as u64 / 8);
    out.finish(vm)
}

/// `_json_float(f, allow_nan)`.
pub(crate) fn pl_json_float(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let f = vm.unbx(vm.arg(&a, 0)).as_float().unwrap_or(0.0);
    let allow = vm.truthy(vm.arg(&a, 1));
    let s = float_text(vm, f, allow)?;
    vm.new_str(&s)
}

/// `json.dumps`'s options: the separators (copied once: they are written
/// between every item), and flags.
struct Enc {
    indent: Option<String>,
    key_sep: String,
    item_sep: String,
    flags: u64,
}

const SORT_KEYS: u64 = 1;
const SKIPKEYS: u64 = 2;
const ALLOW_NAN: u64 = 4;
const ASCII: u64 = 8;
const CIRCULAR: u64 = 16;

impl Enc {
    fn has(&self, f: u64) -> bool {
        self.flags & f != 0
    }
}

/// An open container, or a long str being written. A container's `id` is
/// the value written: the list, tuple or dict, or an instance extending
/// one.
enum Level {
    /// A list's or tuple's items `next..end` in `items`.
    Seq {
        items: Ref,
        end: u32,
        next: u32,
        id: Value,
    },
    /// A dict's entries from `next` on: from its entry table `items`, or
    /// with [`SORTED`] in the order of list `keys`, whose storage `items`
    /// is. `flags` has [`FIRST`] before the first key.
    Map {
        items: Ref,
        end: u32,
        next: u32,
        flags: u8,
        id: Value,
        keys: Value,
    },
    /// A str's text from byte `next` on; a dict key's is followed by the
    /// key separator and its value `then` (`UNDEFINED` for a value).
    Str { v: Value, next: usize, then: Value },
}

/// [`Level::Map`] flags: no key written yet; keys in a sorted list; that
/// list for the prelude to sort before the step goes on.
const FIRST: u8 = 1;
const SORTED: u8 = 2;
const SORT_FIRST: u8 = 4;

/// What a value turned into: finished text, a container to descend (with
/// keys for the prelude to sort), or a value only the prelude's Python
/// encoder handles.
enum Step {
    Done,
    Open(Level),
    NeedPython,
}

/// What an encoding step came to: the text is complete, the prelude
/// encodes in Python, the step's work is spent, or a list of keys is to be
/// sorted first.
enum Ended {
    Done,
    Python,
    More,
    Sort(Value),
}

/// `_json_encode(obj, indent, key_sep, item_sep, sort_keys, skipkeys,
/// allow_nan, ensure_ascii, check_circular, default)`: the JSON text, or
/// `_MISSING` when a value needs the prelude's Python encoder; a long one
/// goes on in `_json_steps` ([`Vm::json_dump_step`]).
pub(crate) fn pl_json_encode(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let indent = vm.arg(&a, 1);
    let (ks, is) = (vm.arg(&a, 2), vm.arg(&a, 3));
    for (v, n) in [(ks, 5), (is, 6)] {
        if !Vm::is_str(v) {
            let msg = format!(
                "make_encoder() argument {n} must be str, not {}",
                vm.type_name(v)
            );
            return Err(vm.err(T::TypeError, &msg));
        }
    }
    let indent = if Vm::is_str(indent) {
        indent
    } else {
        Value::NONE
    };
    let mut flags = 0;
    for (i, f) in [SORT_KEYS, SKIPKEYS, ALLOW_NAN, ASCII, CIRCULAR]
        .into_iter()
        .enumerate()
    {
        if vm.truthy(vm.arg(&a, 4 + i as u32)) {
            flags |= f;
        }
    }
    let enc = Enc::read(vm, indent, ks, is, flags);
    dump_start(vm, &enc, vm.arg(&a, 0), indent, ks, is)
}

impl Enc {
    fn read(vm: &Vm, indent: Value, ks: Value, is: Value, flags: u64) -> Enc {
        Enc {
            indent: Vm::is_str(indent).then(|| String::from(vm.str_ref(indent))),
            key_sep: String::from(vm.str_ref(ks)),
            item_sep: String::from(vm.str_ref(is)),
            flags,
        }
    }
}

/// Encodes `root`: its text, `_MISSING`, or the request that goes on in
/// `_json_steps` (the strs are `enc`'s options, kept in the state).
fn dump_start(
    vm: &mut Vm,
    enc: &Enc,
    root: Value,
    indent: Value,
    ks: Value,
    is: Value,
) -> Result<Value, Raised> {
    let mut out = Out::default();
    let mut stack: NativeVec<Level> = NativeVec::default();
    let mut active = NativeIds::default();
    let ended = encode(
        vm,
        enc,
        Some(root),
        &mut stack,
        &mut active,
        &mut out,
        vm.work,
    )?;
    // Even a container with one large leaf must bring forward control polling.
    vm.burn(out.len() as u64 / 64);
    let r = match ended {
        Ended::Done => return out.finish(vm),
        Ended::Python => return Ok(vm.missing),
        Ended::More => Value::UNDEFINED,
        Ended::Sort(l) => l,
    };
    let levels = vm.new_list(&[])?;
    save_levels(vm, &stack, levels)?;
    let mut raw = [u64::from(op::JSON_DUMP), enc.flags, 0, 0, 0, 0];
    let mut text = Value::NONE;
    append(vm, &out, &mut text, &mut raw)?;
    let state = vm.nstate(
        op::JSON_DUMP,
        &[levels, text, indent, ks, is, Value::NONE],
        &raw,
    )?;
    let r = if r == Value::UNDEFINED { state } else { r };
    vm.prelude_request(
        "_json_steps",
        alloc::vec![state, r],
        crate::cont::Cont::None,
    )
}

/// Raw words of an encoding's state: the operation, the flags, the text's
/// chars so far, the bytes the last step wrote, the room reserved for the
/// text (bytes), and while it moves to a bigger block, the bytes moved.
mod dw {
    pub(super) const FLAGS: usize = 1;
    pub(super) const CHARS: usize = 2;
    pub(super) const LAST: usize = 3;
    pub(super) const ROOM: usize = 4;
    pub(super) const MOVED: usize = 5;
}

/// The most the text's room grows by at once.
const GROW_MAX: usize = 16 << 20;

/// A text this long grows by moving to a new block [`MOVE_STEP`] bytes a
/// step: growing a block in place may copy all of it at once (glibc
/// serving it from its heap, dlmalloc on wasm: 52.8 to 64 MiB took
/// 32-41 ms on wasmtime [M]).
const MOVE_MIN: usize = 4 << 20;
const MOVE_STEP: usize = 4 << 20;

/// The room for a text that needs `need` bytes.
fn room_for(need: usize) -> usize {
    let max = usize::try_from(mm_core::MAX_OBJECT_BYTES).unwrap_or(usize::MAX);
    (need + need.min(GROW_MAX)).min(max).max(need)
}

/// Appends `data` to the result's byte block `text` (made on first use),
/// whose room grows first when `need` passes it: by what it needs, up to
/// [`GROW_MAX`] and the largest object, or just to `need` when that does not
/// fit the memory limit. A text over the largest object is a `MemoryError`.
/// Says whether the room grew.
fn put(
    vm: &mut Vm,
    text: &mut Value,
    raw: &mut [u64; 6],
    data: &[u8],
    need: usize,
) -> Result<bool, Raised> {
    let b = if let Some(b) = text.as_byte_ref() {
        b
    } else {
        let res = vm.heap.alloc_bytes(&[]);
        vm.hx(res)?
    };
    let old = usize::try_from(raw[dw::ROOM]).unwrap_or(usize::MAX);
    let mut room = old;
    if need > old {
        room = room_for(need);
        // Live in the state until the end: no collection is requested.
        vm.burn_result(room - old, false);
    }
    let mut res = vm.heap.append_bytes(b, data, room);
    if res.is_err() && room > need {
        room = need;
        res = vm.heap.append_bytes(b, data, room);
    }
    *text = Value::from_byte_ref(vm.hx(res)?);
    raw[dw::ROOM] = room as u64;
    Ok(room > old)
}

/// Appends the step's text to the result's byte block `text`: the text is
/// written once, with no parts and no join.
fn append(vm: &mut Vm, out: &Out, text: &mut Value, raw: &mut [u64; 6]) -> Result<(), Raised> {
    let len = text
        .as_byte_ref()
        .map_or(0, |b| vm.heap.bytes(b).map_or(0, <[u8]>::len));
    let n = out.len();
    put(vm, text, raw, out.buf.as_bytes(), len.saturating_add(n))?;
    raw[dw::CHARS] += out.buf.chars() as u64;
    raw[dw::LAST] = n as u64;
    Ok(())
}

/// A step of moving text block `text` (`len` bytes) to `next`, a new block
/// with the room `need` calls for: [`MOVE_STEP`] bytes through a stack
/// buffer, so no step copies the whole text. When it is done, `text` is
/// the new block. A room the memory limit refuses grows in place instead.
#[cfg_attr(mm_shipped, optimize(size))]
fn move_text(
    vm: &mut Vm,
    text: &mut Value,
    next: &mut Value,
    raw: &mut [u64; 6],
    len: usize,
    need: usize,
) -> Result<(), Raised> {
    let (Some(old), room) = (text.as_byte_ref(), room_for(need)) else {
        return Err(vm.bad_step());
    };
    let mut nb = if let Some(b) = next.as_byte_ref() {
        b
    } else {
        // As growing in place: the copy is charged as it is made.
        vm.burn_result(room.saturating_sub(raw[dw::ROOM] as usize), false);
        raw[dw::MOVED] = 0;
        let res = vm.heap.alloc_bytes(&[]);
        vm.hx(res)?
    };
    let mut at = usize::try_from(raw[dw::MOVED]).unwrap_or(usize::MAX);
    let end = len.min(at.saturating_add(MOVE_STEP));
    let mut buf = [0u8; 1 << 14];
    while at < end {
        let k = (end - at).min(buf.len());
        match vm.heap.bytes(old).ok().and_then(|b| b.get(at..at + k)) {
            Some(src) => buf[..k].copy_from_slice(src),
            None => return Err(vm.bad_step()),
        }
        if let Ok(b) = vm.heap.append_bytes(nb, &buf[..k], room) {
            nb = b;
        } else {
            // No room for both blocks: grow in place.
            *next = Value::NONE;
            raw[dw::MOVED] = 0;
            put(vm, text, raw, &[], need)?;
            return Ok(());
        }
        at += k;
    }
    vm.burn((end - usize::try_from(raw[dw::MOVED]).unwrap_or(0)) as u64 / 64);
    if at < len {
        (*next, raw[dw::MOVED]) = (Value::from_byte_ref(nb), at as u64);
    } else {
        (*text, *next) = (Value::from_byte_ref(nb), Value::NONE);
        (raw[dw::ROOM], raw[dw::MOVED]) = (room as u64, 0);
        // The old block is garbage now: collected before the text grows
        // again, it is not counted against the next growth.
        vm.collect_for_result(len);
    }
    Ok(())
}

impl Vm {
    /// One step of a long `json.dumps` ([`pl_json_encode`]): the text,
    /// `_MISSING`, a list of keys for `_json_steps` to sort, or `state`.
    pub(crate) fn json_dump_step(&mut self, state: Value) -> Result<crate::nstep::Step, Raised> {
        use crate::nstep::Step as S;
        let Some(([levels, mut text, indent, ks, is, mut next], mut raw)) =
            self.nstate_get::<6, 6>(op::JSON_DUMP, state)
        else {
            return Err(self.bad_step());
        };
        if !levels.is_obj_kind(ty::LIST as u8) || !dump_ok(self, text, next, &raw) {
            return Err(self.bad_step());
        }
        // Room too small for another step like the last grows in a step of
        // its own: growth may copy the text. Only a text that passes the
        // largest object is a `MemoryError`.
        let len = text
            .as_byte_ref()
            .map_or(0, |b| self.heap.bytes(b).map_or(0, <[u8]>::len)) as u64;
        let want = len
            .saturating_add(raw[dw::LAST])
            .min(mm_core::MAX_OBJECT_BYTES);
        if want > raw[dw::ROOM] {
            let want = usize::try_from(want).unwrap_or(usize::MAX);
            let len = len as usize;
            if len >= MOVE_MIN {
                move_text(self, &mut text, &mut next, &mut raw, len, want)?;
            } else {
                put(self, &mut text, &mut raw, &[], want)?;
            }
            self.nstate_put(state, 1, &raw[1..]);
            if let Some(st) = state.as_obj() {
                let res = self.heap.set_values(st, 1, &[text]);
                self.hx(res)?;
                let res = self.heap.set_values(st, 5, &[next]);
                self.hx(res)?;
            }
            return Ok(S::More);
        }
        let enc = Enc::read(self, indent, ks, is, raw[dw::FLAGS]);
        let w0 = self.work;
        let mut stack = load_levels(self, levels)?;
        let mut active = NativeIds::default();
        if enc.has(CIRCULAR) {
            for l in &stack {
                if let Level::Seq { id, .. } | Level::Map { id, .. } = l {
                    let res = active.insert(&mut self.heap, *id);
                    self.hx(res)?;
                }
            }
        }
        self.burn(stack.len() as u64 / 4);
        let mut out = Out::default();
        let ended = encode(self, &enc, None, &mut stack, &mut active, &mut out, w0)?;
        self.burn(out.len() as u64 / 64);
        append(self, &out, &mut text, &mut raw)?;
        let r = match ended {
            Ended::Python => return Ok(S::Done(self.missing)),
            Ended::More => None,
            Ended::Sort(l) => Some(l),
            Ended::Done => {
                // The result over the block (its spare room given back).
                let Some(b) = text.as_byte_ref() else {
                    return Err(self.bad_step());
                };
                let res = self.heap.append_bytes(b, &[], 0);
                let b = self.hx(res)?;
                let res = self.heap.str_from_block(b, raw[dw::CHARS]);
                return Ok(S::Done(Value::from_ref(self.hx(res)?, ty::STR as u8)));
            }
        };
        let levels = self.new_list(&[])?;
        save_levels(self, &stack, levels)?;
        if let Some(st) = state.as_obj() {
            let res = self.heap.set_values(st, 0, &[levels, text]);
            self.hx(res)?;
        }
        self.nstate_put(state, 1, &raw[1..]);
        Ok(r.map_or(S::More, S::Done))
    }
}

/// Whether an encoding's saved words `raw` (text block `text`, the block
/// it moves to `next`) are ones a step can leave: known flags, the text's
/// chars for its bytes, its room at least its bytes, and the bytes moved
/// those of `next` (docs/spec.md §9.3).
#[cfg_attr(mm_shipped, optimize(size))]
fn dump_ok(vm: &Vm, text: Value, next: Value, raw: &[u64; 6]) -> bool {
    let len = |v: Value| {
        v.as_byte_ref()
            .and_then(|r| vm.heap.bytes(r).ok())
            .map(|x| x.len() as u64)
    };
    let Some(n) = len(text) else { return false };
    let moved = if next == Value::NONE {
        Some(0)
    } else {
        len(next)
    };
    let max = mm_core::MAX_OBJECT_BYTES;
    raw[dw::FLAGS] & !(SORT_KEYS | SKIPKEYS | ALLOW_NAN | ASCII | CIRCULAR) == 0
        && raw[dw::CHARS] <= n
        && n <= raw[dw::CHARS].saturating_mul(4)
        && raw[dw::LAST] <= max
        && (n..=max).contains(&raw[dw::ROOM])
        && moved == Some(raw[dw::MOVED])
        && raw[dw::MOVED] <= n
}

/// Saves `stack` into list `levels`, four items a level: the value written
/// (the str for a long str), a dict's sorted keys, the next index, and the
/// kind (0 sequence, 1 dict, 2 sorted dict, 3 str; +4 before a dict's
/// first key, or for a key's str, whose value is the second item).
fn save_levels(vm: &mut Vm, stack: &NativeVec<Level>, levels: Value) -> Result<(), Raised> {
    for l in stack {
        let (id, src, next, kind) = match l {
            Level::Seq { id, next, .. } => (*id, Value::NONE, *next as usize, 0),
            Level::Map {
                id,
                keys,
                next,
                flags,
                ..
            } => {
                let kind = if flags & SORTED == 0 { 1 } else { 2 };
                (
                    *id,
                    *keys,
                    *next as usize,
                    kind + 4 * u64::from(flags & FIRST),
                )
            }
            Level::Str { v, next, then } if *then == Value::UNDEFINED => (*v, *v, *next, 3),
            Level::Str { v, next, then } => (*v, *then, *next, 7),
        };
        let next = vm.new_int(next as i64)?;
        let kind = Value::int(kind as i64).unwrap_or(Value::NONE);
        for x in [id, src, next, kind] {
            vm.list_push(levels, x)?;
        }
    }
    Ok(())
}

/// The levels [`save_levels`] saved, read back from the arena.
fn load_levels(vm: &mut Vm, levels: Value) -> Result<NativeVec<Level>, Raised> {
    let mut stack = NativeVec::default();
    let n = vm.list_len(levels);
    for at in (0..n.saturating_sub(3)).step_by(4) {
        let [id, src, next, kind] = [0, 1, 2, 3].map(|i| vm.list_get(levels, at + i));
        let next = next.as_int().and_then(|i| usize::try_from(i).ok());
        let back = vm.unbx(id);
        let level = match (kind.as_int(), next) {
            (Some(0), Some(next)) => vm.seq_storage(back).map(|(items, end)| Level::Seq {
                items,
                end,
                next: u32::try_from(next).unwrap_or(u32::MAX),
                id,
            }),
            (Some(k @ (1 | 5 | 2 | 6)), Some(next)) => {
                let table = if k & 3 == 1 {
                    vm.dict_items_buf(back)
                        .map(|(items, slots)| (items, slots / 2))
                } else {
                    vm.seq_storage(src)
                        .filter(|_| src.is_obj_kind(ty::LIST as u8))
                };
                table.map(|(items, end)| Level::Map {
                    items,
                    end,
                    next: u32::try_from(next).unwrap_or(u32::MAX),
                    flags: if k & 3 == 1 { 0 } else { SORTED } | if k >= 5 { FIRST } else { 0 },
                    id,
                    keys: src,
                })
            }
            (Some(k @ (3 | 7)), Some(next))
                if Vm::is_str(id)
                    && next <= vm.str_bytes(id).len()
                    && vm.str_bytes(id).get(next).is_none_or(|&c| c & 0xC0 != 0x80) =>
            {
                Some(Level::Str {
                    v: id,
                    next,
                    then: if k == 7 { src } else { Value::UNDEFINED },
                })
            }
            _ => None,
        };
        let Some(level) = level else {
            return Err(vm.bad_step());
        };
        let res = stack.push(&mut vm.heap, level);
        vm.hx(res)?;
    }
    Ok(stack)
}

impl Vm {
    /// The item storage and length of list or tuple `v`.
    #[allow(
        clippy::inline_always,
        reason = "json's encode loop: measured, bench Ir"
    )]
    #[inline(always)]
    fn seq_storage(&self, v: Value) -> Option<(Ref, u32)> {
        if v.is_obj_kind(ty::LIST as u8) {
            self.list_parts(v).map(|(buf, n)| {
                (
                    buf,
                    self.obj_len(buf).min(u32::try_from(n).unwrap_or(u32::MAX)),
                )
            })
        } else if v.is_obj_kind(ty::TUPLE as u8) {
            v.as_obj().map(|buf| (buf, self.obj_len(buf)))
        } else {
            None
        }
    }
}

fn newline(vm: &mut Vm, out: &mut Out, indent: &str, level: usize) -> Result<(), Raised> {
    let bytes = indent.len().saturating_mul(level).saturating_add(1);
    out.reserve(vm, bytes)?;
    out.buf.push_str("\n");
    for _ in 0..level {
        out.buf.push_str(indent);
    }
    Ok(())
}

/// Encodes `root` (or goes on with `stack`) into `out` until the text is
/// complete or the step that started at work `w0` is spent.
fn encode(
    vm: &mut Vm,
    enc: &Enc,
    root: Option<Value>,
    stack: &mut NativeVec<Level>,
    active: &mut NativeIds,
    out: &mut Out,
    w0: u64,
) -> Result<Ended, Raised> {
    // Native encoding neither collects nor runs user code. Container storage
    // stays valid until the step ends or hands the operation to Python.
    let mut pending = root;
    let mut work: u64 = 0;
    // Work and output already charged: each charge covers what came since
    // the last (charging the whole output each time was quadratic).
    let (mut paid, mut charged) = (0, 0);
    macro_rules! end {
        ($e:expr) => {{
            vm.burn(work - paid);
            return Ok($e);
        }};
    }
    // Work and output are charged a batch of values at a time (a value's
    // text is short: a longer str is a level of its own), and the step ends
    // once its work is spent.
    macro_rules! settle {
        () => {
            vm.burn(work - paid + (out.len() - charged) as u64 / 64);
            (paid, charged) = (work, out.len());
            if vm.work.wrapping_sub(w0) >= STEP_WORK && !stack.is_empty() {
                end!(Ended::More);
            }
        };
    }
    loop {
        if let Some(v) = pending.take() {
            work += 1;
            // Three outcomes, tested in turn: a jump table here
            // mispredicted on every value (+5% time on small payloads).
            let level = match value(vm, enc, v, out, &mut work)? {
                Step::Done => None,
                Step::Open(level) => Some(level),
                Step::NeedPython => end!(Ended::Python),
            };
            if let Some(mut level) = level {
                let sort = match &mut level {
                    Level::Map { flags, keys, .. } if *flags & SORT_FIRST != 0 => {
                        *flags &= !SORT_FIRST;
                        Some(*keys)
                    }
                    _ => None,
                };
                let (open, id) = match &level {
                    Level::Seq { id, .. } => ("[", Some(*id)),
                    Level::Map { id, .. } => ("{", Some(*id)),
                    Level::Str { .. } => ("\"", None),
                };
                if let Some(id) = id {
                    if enc.has(CIRCULAR) {
                        let res = active.insert(&mut vm.heap, id);
                        if !vm.hx(res)? {
                            // Raised by the Python encoder, with `__notes__`.
                            end!(Ended::Python);
                        }
                    }
                    if stack.len() >= MAX_DEPTH {
                        return Err(vm.err(
                            T::RecursionError,
                            "maximum recursion depth exceeded while encoding a JSON object",
                        ));
                    }
                }
                out.push(vm, open)?;
                let seq = matches!(level, Level::Seq { .. });
                let res = stack.push_hot(&vm.heap, level);
                vm.hx(res)?;
                // An object's first newline waits for its first key:
                // with `skipkeys` it may have none (`{}`).
                if let Some(ind) = &enc.indent
                    && seq
                {
                    newline(vm, out, ind, stack.len())?;
                }
                if let Some(keys) = sort {
                    end!(Ended::Sort(keys));
                }
            }
            if work - paid >= 1024 {
                settle!();
            }
        }
        // Continue the innermost container.
        let depth = stack.len();
        let Some(top) = stack.last_mut() else {
            end!(Ended::Done);
        };
        let sep = |vm: &mut Vm, out: &mut Out| -> Result<(), Raised> {
            out.push(vm, &enc.item_sep)?;
            if let Some(ind) = &enc.indent {
                newline(vm, out, ind, depth)?;
            }
            Ok(())
        };
        let mut long_key = None;
        let finished = match top {
            Level::Seq {
                items, end, next, ..
            } => {
                if *next < *end {
                    if *next > 0 {
                        sep(vm, out)?;
                    }
                    pending = Some(vm.slot(*items, *next));
                    *next += 1;
                    None
                } else {
                    Some(("]", true))
                }
            }
            Level::Map {
                items,
                end,
                next,
                flags,
                id,
                ..
            } => {
                let mut close = Some(("}", *flags & FIRST == 0));
                while *next < *end {
                    let n = *next;
                    *next += 1;
                    // One slice per entry: a key and its value, or a sorted
                    // key.
                    let at = if *flags & SORTED == 0 { n * 2 } else { n } as usize;
                    let words = vm.heap.object_words(*items).map_or(&[][..], |(_, w)| w);
                    let word = |i: usize| {
                        words
                            .get(i)
                            .and_then(|&w| Value::from_bits(w))
                            .unwrap_or(Value::UNDEFINED)
                    };
                    let k = word(at);
                    let v = if *flags & SORTED == 0 {
                        word(at + 1)
                    } else {
                        Value::UNDEFINED
                    };
                    if k == Value::UNDEFINED {
                        // A deleted entry passed over is work too.
                        work += 1;
                        continue;
                    }
                    let v = if v == Value::UNDEFINED {
                        vm.dict_get(*id, k)?.unwrap_or(Value::NONE)
                    } else {
                        v
                    };
                    let key = if Vm::is_str(k) {
                        None
                    } else {
                        match key_text(vm, enc, k)? {
                            KeyText::Text(t) => Some(t),
                            KeyText::Skip => continue,
                            KeyText::Python => end!(Ended::Python),
                        }
                    };
                    if *flags & FIRST != 0 {
                        *flags &= !FIRST;
                        if let Some(ind) = &enc.indent {
                            newline(vm, out, ind, depth)?;
                        }
                    } else {
                        sep(vm, out)?;
                    }
                    close = None;
                    match key {
                        None if !out.quote_value(vm, k, enc.has(ASCII))? => {
                            long_key = Some(long_str(k, v));
                            break;
                        }
                        None => {}
                        Some(t) => out.quote(vm, &t, enc.has(ASCII))?,
                    }
                    out.push(vm, &enc.key_sep)?;
                    pending = Some(v);
                    break;
                }
                close
            }
            Level::Str { v, next, .. } => {
                let ascii = enc.has(ASCII);
                let (len, e, plain) = {
                    let b = vm.str_bytes(*v);
                    let mut e = b.len().min(*next + STR_SLICE);
                    while b.get(e).is_some_and(|&c| c & 0xC0 == 0x80) {
                        e -= 1;
                    }
                    let piece = b.get(*next..e).unwrap_or(&[]);
                    let plain = piece
                        .iter()
                        .all(|&c| c >= 0x20 && c != b'"' && c != b'\\' && (!ascii || c < 0x7f));
                    (b.len(), e, plain)
                };
                if *next < len {
                    let (r, from) = (v.as_obj(), *next);
                    *next = e;
                    if plain && let Some(r) = r {
                        out.reserve(vm, e - from)?;
                        let res = out.buf.push_slice(&vm.heap, r, from, e);
                        vm.hx(res)?;
                    } else {
                        let s = String::from(
                            core::str::from_utf8(vm.str_bytes(*v).get(from..e).unwrap_or(&[]))
                                .unwrap_or(""),
                        );
                        // Escaping goes a char at a time.
                        work += (e - from) as u64 / 4;
                        out.escape(vm, &s, ascii)?;
                    }
                    settle!();
                    None
                } else {
                    Some(("\"", false))
                }
            }
        };
        if let Some(level) = long_key {
            // A long key opens; its value follows it.
            out.push(vm, "\"")?;
            let res = stack.push(&mut vm.heap, level);
            vm.hx(res)?;
        } else if let Some((close, items)) = finished {
            match stack.pop() {
                Some(Level::Seq { id, .. } | Level::Map { id, .. }) if enc.has(CIRCULAR) => {
                    active.remove(id);
                }
                Some(Level::Str { then, .. }) if then != Value::UNDEFINED => {
                    out.push(vm, "\"")?;
                    out.push(vm, &enc.key_sep)?;
                    pending = Some(then);
                    continue;
                }
                _ => {}
            }
            if let Some(ind) = &enc.indent
                && items
            {
                newline(vm, out, ind, stack.len())?;
            }
            out.push(vm, close)?;
        }
    }
}

/// A dict key that is not a str.
enum KeyText {
    Text(String),
    /// Left out (`skipkeys`).
    Skip,
    /// Raised by the Python encoder, with `__notes__`.
    Python,
}

/// The text of key `k` (not a str): a float, bool, None or int, also as
/// the value a subclass instance extends.
#[cold]
#[inline(never)]
fn key_text(vm: &mut Vm, enc: &Enc, k: Value) -> Result<KeyText, Raised> {
    let k = vm.unbx(k);
    Ok(KeyText::Text(if Vm::is_str(k) {
        String::from(vm.str_ref(k))
    } else if let Some(f) = k.as_float() {
        if !f.is_finite() && !enc.has(ALLOW_NAN) {
            return Ok(KeyText::Python);
        }
        float_text(vm, f, enc.has(ALLOW_NAN))?
    } else if k == Value::TRUE {
        "true".into()
    } else if k == Value::FALSE {
        "false".into()
    } else if k == Value::NONE {
        "null".into()
    } else if is_int(k) {
        vm.int_decimal(k)?
    } else if enc.has(SKIPKEYS) {
        return Ok(KeyText::Skip);
    } else {
        return Ok(KeyText::Python);
    }))
}

/// Writes a scalar, or opens a container.
fn value(vm: &mut Vm, enc: &Enc, v: Value, out: &mut Out, work: &mut u64) -> Result<Step, Raised> {
    if Vm::is_str(v) {
        if !out.quote_value(vm, v, enc.has(ASCII))? {
            return Ok(Step::Open(long_str(v, Value::UNDEFINED)));
        }
    } else if v == Value::NONE {
        out.push(vm, "null")?;
    } else if v == Value::TRUE {
        out.push(vm, "true")?;
    } else if v == Value::FALSE {
        out.push(vm, "false")?;
    } else if let Some(i) = v.as_int() {
        out.push_int(vm, i)?;
    } else if is_int(v) {
        let s = vm.int_decimal(v)?;
        out.push(vm, &s)?;
        // Charged by the conversion: the step checks its work now.
        *work += 1024;
    } else if let Some(f) = v.as_float() {
        if !f.is_finite() && !enc.has(ALLOW_NAN) {
            return Ok(Step::NeedPython);
        }
        // The shortest repr takes about a microsecond.
        vm.burn(FLOAT_WORK);
        let s = float_text(vm, f, enc.has(ALLOW_NAN))?;
        out.push(vm, &s)?;
    } else if v.is_obj_kind(ty::LIST as u8) || v.is_obj_kind(ty::TUPLE as u8) {
        return seq_value(vm, v, v, out);
    } else if v.is_obj_kind(ty::DICT as u8) {
        if vm.dict_len(v) == 0 {
            out.push(vm, "{}")?;
            return Ok(Step::Done);
        }
        let Some((items, slots)) = vm.dict_items_buf(v) else {
            out.push(vm, "{}")?;
            return Ok(Step::Done);
        };
        if !enc.has(SORT_KEYS) {
            return Ok(Step::Open(map_level(v, items, slots / 2)));
        }
        let count = vm.dict_len(v).min(slots as usize / 2);
        let mut keys = NativeValues::default();
        let res = keys.reserve(&mut vm.heap, count);
        vm.hx(res)?;
        for at in (0..slots).step_by(2) {
            let key = vm.slot(items, at);
            if key != Value::UNDEFINED {
                let res = keys.push(&mut vm.heap, key);
                vm.hx(res)?;
            }
        }
        if count > SORT_NATIVE
            && keys
                .iter()
                .all(|&k| Vm::is_str(k) || is_int(k) || k.as_float().is_some())
        {
            // Sorted by the prelude, in steps: the level is read again
            // after.
            let l = vm.new_list(&keys)?;
            let mut level = sorted_level(vm, v, l)?;
            if let Level::Map { flags, .. } = &mut level {
                *flags |= SORT_FIRST;
            }
            return Ok(Step::Open(level));
        }
        crate::builtins::sort_values(vm, &mut keys, false)?;
        let l = vm.new_list(&keys)?;
        return Ok(Step::Open(sorted_level(vm, v, l)?));
    } else if let Some(b) = vm.backing(v) {
        return subclass_value(vm, enc, v, b, out, work);
    } else {
        return Ok(Step::NeedPython);
    }
    Ok(Step::Done)
}

/// Dict `id`'s entries, from entry table `items` of `end` entries.
fn map_level(id: Value, items: Ref, end: u32) -> Level {
    Level::Map {
        items,
        end,
        next: 0,
        flags: FIRST,
        id,
        keys: Value::NONE,
    }
}

/// Dict `id`'s entries in the order of key list `keys`.
fn sorted_level(vm: &mut Vm, id: Value, keys: Value) -> Result<Level, Raised> {
    let Some((items, end)) = vm.seq_storage(keys) else {
        return Err(vm.bad_step());
    };
    Ok(Level::Map {
        items,
        end,
        next: 0,
        flags: FIRST | SORTED,
        id,
        keys,
    })
}

/// A long str to write a slice a time, then `then` as a key's value.
fn long_str(v: Value, then: Value) -> Level {
    Level::Str { v, next: 0, then }
}

/// List or tuple `src` (`id` itself, or the value instance `id` extends).
#[allow(
    clippy::inline_always,
    reason = "json's encode loop: measured, bench Ir"
)]
#[inline(always)]
fn seq_value(vm: &mut Vm, id: Value, src: Value, out: &mut Out) -> Result<Step, Raised> {
    if let Some((items, end)) = vm.seq_storage(src).filter(|(_, n)| *n != 0) {
        return Ok(Step::Open(Level::Seq {
            items,
            end,
            next: 0,
            id,
        }));
    }
    out.push(vm, "[]")?;
    Ok(Step::Done)
}

/// A builtin-type subclass instance `v` encodes as the value `b` it extends
/// (its `__repr__`/`__str__` are not used), its containers keeping `v`'s
/// identity for the circular check. A `dict` subclass with its own `items`
/// (which CPython calls) or under `sort_keys` goes to the Python encoder.
#[cold]
#[inline(never)]
fn subclass_value(
    vm: &mut Vm,
    enc: &Enc,
    v: Value,
    b: Value,
    out: &mut Out,
    work: &mut u64,
) -> Result<Step, Raised> {
    if Vm::is_str(b) {
        if !out.quote_value(vm, b, enc.has(ASCII))? {
            return Ok(Step::Open(long_str(b, Value::UNDEFINED)));
        }
    } else if let Some(f) = b.as_float() {
        if !f.is_finite() && !enc.has(ALLOW_NAN) {
            return Ok(Step::NeedPython);
        }
        let s = float_text(vm, f, enc.has(ALLOW_NAN))?;
        out.push(vm, &s)?;
    } else if is_int(b) {
        let s = vm.int_decimal(b)?;
        out.push(vm, &s)?;
        // Charged by the conversion: the step checks its work now.
        *work += 1024;
    } else if b.is_obj_kind(ty::LIST as u8) || b.is_obj_kind(ty::TUPLE as u8) {
        return seq_value(vm, v, b, out);
    } else if b.is_obj_kind(ty::DICT as u8) && !enc.has(SORT_KEYS) {
        let key = vm.intern("items")?;
        if vm
            .class_lookup(vm.class_of(v), key)
            .is_some_and(|m| m.as_native().is_none())
        {
            return Ok(Step::NeedPython);
        }
        match vm.dict_items_buf(b).filter(|_| vm.dict_len(b) != 0) {
            Some((items, slots)) => {
                return Ok(Step::Open(map_level(v, items, slots / 2)));
            }
            None => out.push(vm, "{}")?,
        }
    } else {
        return Ok(Step::NeedPython);
    }
    Ok(Step::Done)
}

// ------------------------------------------------------------- decoding
//
// A document is scanned a window of input at a time (docs/spec.md §5,
// "Bounded native steps"): a step copies at most `WINDOW` bytes from where
// the scan stands and reads values until it spends `STEP_WORK` or nears the
// window's end. Between steps the scan's state is an `NSTATE` object
// (`nstep::op::JSON_SCAN`): the input, the containers still open (each made
// from its items so far), the document's value once read, and raw words
// for the position. A token too long for a window (a string, a number or
// whitespace over a mebibyte) is read in the arena itself, a slice a step,
// and a long string or number text is written into a byte block of its
// final size.

/// A scan failure: CPython's message and the byte offset it points at.
/// [`CUT`] is none: the window ended inside a token, which is read again.
type Fail = (&'static str, usize);

/// A `\uXXXX` escape of a surrogate that is not half of a pair.
const LONE_SURROGATE: &str =
    "Lone surrogate escape, which micro-monty strings cannot hold: they are UTF-8";

const CUT: &str = "";

/// The input a scan step copies at most, and how close to its end a new
/// value no longer starts in it.
const WINDOW: usize = 1 << 18;
const MARGIN: usize = 1 << 12;

/// Input bytes a long token's step reads, and a long string's slice
/// decoded at a time.
const LONG_BYTES: usize = 1 << 21;
const FILL_BYTES: usize = 1 << 16;

/// The states a scan stops and goes on in, each starting at whitespace: a
/// value; an array's first item or `]`; an item after a comma; an object's
/// first key or `}`; a key after a comma; the `:` after a key; the
/// delimiter after a value; the end after the document's value.
mod at {
    pub(super) const VALUE: u64 = 0;
    pub(super) const FIRST: u64 = 1;
    pub(super) const ITEM: u64 = 2;
    pub(super) const KEY0: u64 = 3;
    pub(super) const KEY: u64 = 4;
    pub(super) const COLON: u64 = 5;
    pub(super) const AFTER: u64 = 6;
    pub(super) const TRAIL: u64 = 7;
}

/// The raw words of a scan's state after its operation: the position, the
/// state ([`at`]), the offset of the comma before it, the flags, the long
/// token being read (0 none, then `LONG_*`), and that token's words.
mod w {
    pub(super) const I: usize = 0;
    pub(super) const AT: usize = 1;
    pub(super) const COMMA: usize = 2;
    pub(super) const FLAGS: usize = 3;
    pub(super) const LONG: usize = 4;
    /// A long token: where it starts, where the read stands, then a
    /// string's decoded bytes, chars, where it ends and the bytes written,
    /// or a number's part (0 integer, 1 fraction, 2 exponent), the ends of
    /// its integer and fraction, the start of its exponent and its first
    /// and last nonzero mantissa digits.
    pub(super) const Q: usize = 5;
    pub(super) const R: usize = 6;
    pub(super) const A: usize = 7;
    pub(super) const B: usize = 8;
    pub(super) const C: usize = 9;
    pub(super) const D: usize = 10;
    pub(super) const E: usize = 11;
    pub(super) const F: usize = 12;
    /// A big object's table growing ([`grow_step`]): old entries read and
    /// entries moved.
    pub(super) const GAT: usize = 13;
    pub(super) const GN: usize = 14;
    pub(super) const N: usize = 15;
}

const RAW: u64 = 1;
const LAX: u64 = 2;
const IDX: u64 = 4;
/// The scan stopped before closing the innermost container, whose parent's
/// table grows first.
const UP: u64 = 8;

/// The entries a big object's table keeps free: with fewer, it grows a
/// slice per step before the scan goes on.
const GROW_AT: usize = 1 << 11;
const LONG_STR: u64 = 1;
const LONG_FILL: u64 = 2;
const LONG_NUM: u64 = 3;
const LONG_TEXT: u64 = 4;

/// An open container of this step. Its items so far sit on the step's one
/// value stack from `start` on, so opening a container allocates nothing;
/// an object's key waits beside it for its value.
enum Ctx {
    Arr(usize),
    Obj(usize, Value),
}

impl Ctx {
    fn start(&self) -> usize {
        match *self {
            Ctx::Arr(s) | Ctx::Obj(s, _) => s,
        }
    }
}

/// A scan's native state for one step.
struct Dec {
    raw: bool,
    strict: bool,
    stack: NativeVec<Ctx>,
    vals: NativeValues,
    memo: KeyMemo,
    /// Containers earlier steps left open, outermost first, three items
    /// each: the container so far (a list, or a dict), the key waiting for
    /// its value (or None), and True for an object. None until a step
    /// leaves one open.
    ctxs: Value,
    /// The innermost of `ctxs` (whether it is an object) and its key: its
    /// new items wait on the value stack below those of `stack`.
    top: Option<(Value, bool)>,
    key: Value,
    /// Entries `top`, a big table, takes before it grows (else
    /// `usize::MAX`): the step ends before its items could grow it, and
    /// the next steps grow it a slice at a time ([`grow_step`]; the 2^20 to
    /// 2^21 rebuild alone took 66-84 ms [M]).
    room: usize,
}

/// Per-document key identity, without duplicate string storage. Like the core
/// intern table, this uses linear probing and stays at most half full.
#[derive(Default)]
struct KeyMemo {
    keys: NativeValues,
    len: usize,
}

impl KeyMemo {
    fn key(&mut self, vm: &mut Vm, text: &str) -> Result<Value, Raised> {
        let hash = mm_core::hash_bytes(text.as_bytes());
        if !self.keys.is_empty() {
            let mask = self.keys.len() - 1;
            let mut slot = usize::try_from(hash & mask as u64).unwrap_or(0);
            while self.keys[slot] != Value::UNDEFINED {
                let key = self.keys[slot];
                if key.as_obj().and_then(|r| vm.heap.str_hash(r).ok()) == Some(hash)
                    && vm.str_bytes(key) == text.as_bytes()
                {
                    return Ok(key);
                }
                slot = (slot + 1) & mask;
            }
        }
        let key = vm.new_str(text)?;
        if self.len == 4096 {
            return Ok(key);
        }
        // Cache the full hash once: bucket collisions must not repeatedly
        // compare long common prefixes, and rehashing must not rescan text.
        vm.hash(key)?;
        if (self.len + 1) * 2 > self.keys.len() {
            let capacity = (self.keys.len() * 2).max(8);
            let mut keys = NativeValues::default();
            let res = keys.reserve(&mut vm.heap, capacity);
            vm.hx(res)?;
            for _ in 0..capacity {
                let res = keys.push(&mut vm.heap, Value::UNDEFINED);
                vm.hx(res)?;
            }
            for &key in &self.keys {
                if let Some(r) = key.as_obj() {
                    let res = vm.heap.str_hash_mut(r);
                    let hash = vm.hx(res)?;
                    Self::insert(&mut keys, key, hash);
                }
            }
            self.keys = keys;
        }
        Self::insert(&mut self.keys, key, hash);
        self.len += 1;
        Ok(key)
    }

    fn insert(keys: &mut [Value], key: Value, hash: u64) {
        let mask = keys.len() - 1;
        let mut slot = usize::try_from(hash & mask as u64).unwrap_or(0);
        while keys[slot] != Value::UNDEFINED {
            slot = (slot + 1) & mask;
        }
        keys[slot] = key;
    }
}

fn skip_ws(b: &[u8], mut i: usize) -> usize {
    while i < b.len() && matches!(b[i], b' ' | b'\t' | b'\n' | b'\r') {
        i += 1;
    }
    i
}

/// `_json_scan(s, raw=False, idx=None, strict=True)`: `(value,)`, or
/// `(msg, pos)` for a `JSONDecodeError`. With `raw` true (`json.loads`
/// hooks), a number or `NaN`/`Infinity`/`-Infinity` is a 1-tuple of its
/// source text and an object is a flat tuple of its keys and values in
/// document order, for the prelude's `_json_hooks` to finish. With `idx`
/// (`JSONDecoder.raw_decode`), the value starts at that character, nothing
/// after it is checked, and success is `(value, end, None)`. `strict` false
/// allows control characters in strings. A long document goes on in
/// `_steps` ([`Vm::json_scan_step`]).
pub(crate) fn pl_json_scan(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let sv = vm.arg(&a, 0);
    let idx = vm.arg(&a, 2).as_int();
    let mut flags = 0;
    if vm.arg(&a, 1) == Value::TRUE {
        flags |= RAW;
    }
    if vm.arg(&a, 3) == Value::FALSE {
        flags |= LAX;
    }
    let start = match idx {
        // The byte offset of character `i` (the end when past it); no
        // whitespace is skipped there.
        Some(i) => {
            flags |= IDX;
            let at = vm.str_offset(sv, usize::try_from(i).unwrap_or(0));
            if vm.str_bytes(sv).get(at).is_none_or(u8::is_ascii_whitespace) {
                return scan_result(vm, sv, Err(("Expecting value", at)));
            }
            at
        }
        None => 0,
    };
    let mut wd = [0u64; w::N];
    wd[w::I] = start as u64;
    wd[w::FLAGS] = flags;
    let mut aux = Value::NONE;
    let mut d = Dec::new(vm, flags, Value::NONE);
    if let Some(v) = scan_go(vm, sv, &mut d, &mut wd, &mut aux)? {
        return Ok(v);
    }
    spill(vm, &mut d)?;
    let none = Value::NONE;
    vm.chunk_rest(op::JSON_SCAN, &[sv, d.ctxs, aux, none, none], &wd)
}

impl Dec {
    fn new(vm: &Vm, flags: u64, ctxs: Value) -> Dec {
        let mut d = Dec {
            raw: flags & RAW != 0,
            strict: flags & LAX == 0,
            stack: NativeVec::default(),
            vals: NativeValues::default(),
            memo: KeyMemo::default(),
            ctxs,
            top: None,
            key: Value::NONE,
            room: usize::MAX,
        };
        d.load_top(vm);
        d
    }

    /// Reads the innermost of `ctxs`.
    fn load_top(&mut self, vm: &Vm) {
        let n = vm.list_len(self.ctxs);
        let mut top = [Value::NONE; 3];
        if let (Some(buf), Some(at)) = (vm.list_buf(self.ctxs), n.checked_sub(3)) {
            let words = vm.heap.slot_words(buf).unwrap_or(&[]);
            for (t, &w) in top.iter_mut().zip(words.get(at..).unwrap_or(&[])) {
                *t = Value::from_bits(w).unwrap_or(Value::NONE);
            }
        }
        self.top = (n >= 3).then_some((top[0], top[2] == Value::TRUE));
        self.key = top[1];
        self.room = self
            .top
            .filter(|&(_, obj)| obj && !self.raw)
            .and_then(|(c, _)| big_table(vm, c))
            .map_or(usize::MAX, |(cap, used)| cap.saturating_sub(used));
    }
}

/// The capacity and entries of dict `c`'s table when it is big.
fn big_table(vm: &Vm, c: Value) -> Option<(usize, usize)> {
    let dv = vm.dv(c.as_obj()?)?;
    let cap = vm.heap.raw_words(dv.index?).ok()?.len() / 2;
    (cap >= 1 << 16).then_some((cap, dv.used as usize))
}

/// The open object of `ctxs` (the innermost, or with `up` its parent) whose
/// table must grow before it takes more entries, and its new capacity:
/// fewer than [`GROW_AT`] entries free, and a bigger table under the
/// object cap.
#[cfg_attr(mm_shipped, optimize(size))]
fn grow_target(vm: &mut Vm, ctxs: Value, up: bool) -> Result<Option<(Value, usize)>, Raised> {
    let at = vm.list_len(ctxs).checked_sub(if up { 6 } else { 3 });
    let (Some(buf), Some(at)) = (vm.list_buf(ctxs), at) else {
        return Ok(None);
    };
    let (c, obj) = {
        let w = vm.heap.slot_words(buf).unwrap_or(&[]);
        let v = |i: usize| w.get(at + i).and_then(|&x| Value::from_bits(x));
        (v(0).unwrap_or(Value::NONE), v(2) == Some(Value::TRUE))
    };
    match big_table(vm, c).filter(|&(cap, used)| obj && cap - used.min(cap) < GROW_AT) {
        Some((cap, used)) => Ok(vm
            .dict_reserve(c, used + GROW_AT)?
            .filter(|&n| n > cap)
            .map(|n| (c, n))),
        None => Ok(None),
    }
}

/// A step of growing a big open object's table before the scan goes on,
/// with [`Vm::dict_grow_slice`]: the new entries table, then the new index,
/// then the entries moved a slice per step. The innermost object grows when
/// it has fewer than [`GROW_AT`] entries free; after a stop before closing
/// the innermost container ([`UP`]), its parent does. Nothing writes to the
/// object meanwhile. Whether this step grew a table.
#[cfg_attr(mm_shipped, optimize(size))]
fn grow_step(
    vm: &mut Vm,
    ctxs: Value,
    tabs: &mut [Value; 2],
    wd: &mut [u64; w::N],
) -> Result<bool, Raised> {
    let mut target = None;
    if wd[w::FLAGS] & RAW == 0 {
        target = grow_target(vm, ctxs, false)?;
        if target.is_none() && wd[w::FLAGS] & UP != 0 {
            target = grow_target(vm, ctxs, true)?;
        }
    }
    let [ne, ni] = tabs;
    let Some((c, cap)) = target else {
        wd[w::FLAGS] &= !UP;
        if (*ne, *ni) != (Value::NONE, Value::NONE) {
            return Err(vm.bad_step());
        }
        return Ok(false);
    };
    if *ne == Value::NONE {
        let r = vm.dict_entries_table(cap)?;
        *ne = Value::from_ref(r, ty::BUF as u8);
        (wd[w::GAT], wd[w::GN]) = (0, 0);
        return Ok(true);
    }
    let entries = ne
        .as_obj()
        .filter(|&r| ne.is_obj_kind(ty::BUF as u8) && vm.obj_len(r) as usize == 2 * cap);
    if entries.is_none() {
        return Err(vm.bad_step());
    }
    if *ni == Value::NONE {
        let r = vm.dict_index_table(cap)?;
        *ni = Value::from_ref(r, ty::RAW as u8);
        return Ok(true);
    }
    let (mut at, mut n) = (wd[w::GAT], wd[w::GN]);
    let index = ni
        .as_obj()
        .filter(|_| ni.is_obj_kind(ty::RAW as u8))
        .and_then(|r| vm.heap.raw_words(r).ok())
        .map(<[u64]>::len);
    if index != Some(2 * cap + 1) || n > at || at > vm.dict_used(c) as u64 {
        return Err(vm.bad_step());
    }
    let Some((done, installed)) = vm.dict_grow_slice(c, *ne, *ni, &mut at, &mut n, STEP_WORK)
    else {
        return Err(vm.bad_step());
    };
    vm.burn(done);
    (wd[w::GAT], wd[w::GN]) = (at, n);
    if installed {
        *tabs = [Value::NONE; 2];
        (wd[w::GAT], wd[w::GN]) = (0, 0);
    }
    Ok(true)
}

impl Vm {
    /// One step of a long `json.loads` ([`pl_json_scan`]).
    pub(crate) fn json_scan_step(&mut self, state: Value) -> Result<crate::nstep::Step, Raised> {
        use crate::nstep::Step;
        let Some(([sv, ctxs, mut aux, ne, ni], raw)) =
            self.nstate_get::<5, { w::N + 1 }>(op::JSON_SCAN, state)
        else {
            return Err(self.bad_step());
        };
        let mut wd = [0u64; w::N];
        wd.copy_from_slice(&raw[1..]);
        if !Vm::is_str(sv)
            || (ctxs != Value::NONE && !ctxs.is_obj_kind(ty::LIST as u8))
            || !scan_ok(self, sv, aux, &wd)
        {
            return Err(self.bad_step());
        }
        let mut tabs = [ne, ni];
        if grow_step(self, ctxs, &mut tabs, &mut wd)? {
            if let Some(r) = state.as_obj() {
                let res = self.heap.set_values(r, 3, &tabs);
                self.hx(res)?;
            }
            self.nstate_put(state, 1, &wd);
            return Ok(Step::More);
        }
        let mut d = Dec::new(self, wd[w::FLAGS], ctxs);
        if let Some(v) = scan_go(self, sv, &mut d, &mut wd, &mut aux)? {
            return Ok(Step::Done(v));
        }
        spill(self, &mut d)?;
        if let Some(r) = state.as_obj() {
            let res = self.heap.set_values(r, 1, &[d.ctxs, aux]);
            self.hx(res)?;
        }
        self.nstate_put(state, 1, &wd);
        Ok(Step::More)
    }
}

/// Whether a scan's saved words `wd` (input `sv`, long token block
/// `aux`) are ones a step can leave: positions within the input, the scan
/// at a char boundary, and a long token's positions and counts consistent
/// with its block. A forged image is then a `bad_step`, never an index
/// past the input (docs/spec.md §9.3).
#[cfg_attr(mm_shipped, optimize(size))]
fn scan_ok(vm: &Vm, sv: Value, aux: Value, wd: &[u64; w::N]) -> bool {
    let b = vm.str_bytes(sv);
    let len = b.len() as u64;
    let byte = |i: u64| b.get(usize::try_from(i).unwrap_or(usize::MAX)).copied();
    let lead = |i: u64| i <= len && byte(i).is_none_or(|c| c & 0xC0 != 0x80);
    let blk = aux
        .as_byte_ref()
        .and_then(|r| vm.heap.bytes(r).ok())
        .map(|x| x.len() as u64);
    let [i, at, comma, flags, long, q, r, a, n, c, d, e, f, ..] = *wd;
    // The token starts in the input (at a quote for a string), and is
    // read up to `r`.
    let token = q <= r && r <= len && q < len && lead(q);
    let quoted = token && byte(q) == Some(b'"') && r > q;
    let ok = match long {
        0 => true,
        // Measuring: decoded bytes and chars so far.
        LONG_STR => quoted && n <= a && a <= r - q,
        // Filling the block of the decoded bytes, `d` of them so far.
        LONG_FILL => quoted && r <= c && c <= len && blk == Some(a) && n <= a && d <= a,
        // A raw number's text, copied into its block as it is read.
        LONG_TEXT => token && r <= c && c <= len && blk == Some(c - q) && n == c - q && d == r - q,
        // A number: its part (0 integer, 1 fraction, 2 exponent), the ends
        // of its integer (set past the integer part, at a non-digit) and
        // fraction, its exponent's start (set in the exponent part), and
        // its first and last nonzero mantissa digits.
        LONG_NUM => {
            let part = match a {
                0 => n == 0 && c == 0 && d == 0,
                1 => q < n && n <= r && c == 0 && d == 0,
                2 => q < n && n < d && d <= r && (c == 0 || (n..d).contains(&c)),
                _ => false,
            };
            token
                && part
                && (byte(q) != Some(b'-') || r > q)
                && (e == u64::MAX || ((q..r).contains(&e) && (n == 0 || e != n)))
                && f <= r
        }
        _ => false,
    };
    ok && lead(i) && at <= at::TRAIL && comma <= len && flags & !(RAW | LAX | IDX | UP) == 0
}

/// The scan's result for `_json_scan`: `(value,)`, `(value, end, None)`
/// with an index, or `(msg, pos)`; positions in chars.
fn scan_result(
    vm: &mut Vm,
    sv: Value,
    r: Result<(Value, Option<usize>), Fail>,
) -> Result<Value, Raised> {
    match r {
        Ok((v, None)) => vm.alloc_tuple(&[v]),
        Ok((v, Some(end))) => {
            let e = vm.str_index(sv, end);
            let e = vm.new_int(i64::try_from(e).unwrap_or(0))?;
            vm.alloc_tuple(&[v, e, Value::NONE])
        }
        Err((msg, pos)) => {
            let m = vm.new_str(msg)?;
            let p = vm.str_index(sv, pos);
            let p = vm.new_int(i64::try_from(p).unwrap_or(0))?;
            vm.alloc_tuple(&[m, p])
        }
    }
}

/// Scans from the state in `d`, `wd` and `aux` until the document is read
/// (its `_json_scan` result) or the step's work is spent (`None`, the state
/// left for the next step).
fn scan_go(
    vm: &mut Vm,
    sv: Value,
    d: &mut Dec,
    wd: &mut [u64; w::N],
    aux: &mut Value,
) -> Result<Option<Value>, Raised> {
    let w0 = vm.work;
    let idx = wd[w::FLAGS] & IDX != 0;
    loop {
        let spent = vm.work.wrapping_sub(w0);
        if spent >= STEP_WORK {
            return Ok(None);
        }
        // A big table about to fill grows in the next steps ([`grow_step`]).
        let pending = d.stack.first().map_or(d.vals.len(), Ctx::start) / 2;
        if spent != 0 && d.room.saturating_sub(pending) < GROW_AT {
            return Ok(None);
        }
        let room = usize::try_from(STEP_WORK - spent)
            .unwrap_or(0)
            .saturating_mul(16);
        if wd[w::LONG] != 0 {
            let token = match long_step(vm, sv, d, wd, aux, room)? {
                Ok(Some(t)) => t,
                Ok(None) => continue,
                Err(f) => return scan_result(vm, sv, Err(f)).map(Some),
            };
            if let Some(v) = token_done(vm, d, wd, aux, token)? {
                return scan_result(vm, sv, Ok((v, Some(wd[w::I] as usize)))).map(Some);
            }
            continue;
        }
        // Whitespace is read in the arena: a run of it may fill windows.
        let i = wd[w::I] as usize;
        let (j, len) = {
            let b = vm.str_bytes(sv);
            let lim = b.len().min(i.saturating_add(room));
            (i + skip_ws(&b[i.min(lim)..lim], 0), b.len())
        };
        vm.burn((j - i) as u64 / 16);
        wd[w::I] = j as u64;
        if j == len.min(i.saturating_add(room)) && j < len {
            continue;
        }
        if wd[w::AT] == at::TRAIL {
            let r = if j == len {
                Ok((*aux, None))
            } else {
                Err(("Extra data", j))
            };
            return scan_result(vm, sv, r).map(Some);
        }
        // The window, ending at a char boundary.
        let mut end = len.min(j + WINDOW);
        while vm.str_bytes(sv).get(end).is_some_and(|&c| c & 0xC0 == 0x80) {
            end -= 1;
        }
        let res = vm.heap.reserve_scratch(end - j);
        let _charge = vm.hx(res)?;
        let mut s = String::new();
        let res = s
            .try_reserve_exact(end - j)
            .map_err(|_| mm_core::Error::OutOfMemory);
        vm.hx(res)?;
        // Only the window is validated: `str_ref` would check the whole input.
        s.push_str(core::str::from_utf8(&vm.str_bytes(sv)[j..end]).unwrap_or(""));
        vm.burn(s.len() as u64 / 16);
        match run(vm, d, &s, j, end == len, wd, w0)? {
            Ran::Cut if wd[w::I] as usize == j => {
                // The token at the window's start fills it.
                let c = s.as_bytes().first().copied().unwrap_or(0);
                wd[w::Q] = j as u64;
                wd[w::R] = j as u64 + 1;
                wd[w::A..].fill(0);
                if c == b'"' {
                    wd[w::LONG] = LONG_STR;
                } else {
                    wd[w::LONG] = LONG_NUM;
                    wd[w::R] = j as u64 + u64::from(c == b'-');
                    wd[w::E] = u64::MAX;
                }
            }
            Ran::More | Ran::Cut => {}
            Ran::Full => return Ok(None),
            Ran::Done(v) if idx => {
                return scan_result(vm, sv, Ok((v, Some(wd[w::I] as usize)))).map(Some);
            }
            Ran::Done(v) => {
                *aux = v;
                wd[w::AT] = at::TRAIL;
            }
            Ran::Fail(f) => return scan_result(vm, sv, Err(f)).map(Some),
        }
    }
}

/// What a window's scan came to: the document's value (the position after
/// it), a stop (spent work or near the window's end), a stop that ends the
/// step (a table must grow first), a token cut by the window's end, or a
/// failure.
enum Ran {
    Done(Value),
    More,
    Full,
    Cut,
    Fail(Fail),
}

/// Scans window `s` (the input from byte `base`; `last` when it reaches
/// the end) from the state in `wd`, which it keeps current.
fn run(
    vm: &mut Vm,
    d: &mut Dec,
    s: &str,
    base: usize,
    last: bool,
    wd: &mut [u64; w::N],
    w0: u64,
) -> Result<Ran, Raised> {
    let b = s.as_bytes();
    let cutoff = if last {
        usize::MAX
    } else {
        b.len().saturating_sub(MARGIN)
    };
    let mut i = (wd[w::I] as usize).saturating_sub(base);
    let mut st = wd[w::AT];
    let mut count: u64 = 0;
    // Values (at least one an entry) before the window ends for `d.room`,
    // which then ends the step ([`scan_go`]).
    let pending = d.stack.first().map_or(d.vals.len(), Ctx::start) / 2;
    let mut room = d.room.saturating_sub(pending + 1024) as u64;
    // The state goes on at `j` (a token there is read again).
    macro_rules! stop {
        ($ran:expr, $j:expr) => {{
            wd[w::I] = (base + $j) as u64;
            wd[w::AT] = st;
            return Ok($ran);
        }};
    }
    // Closes the innermost container, unless it is the innermost of
    // `ctxs` and its parent's table must grow first: then the step ends.
    macro_rules! close {
        ($j:expr) => {{
            let top = d.stack.is_empty();
            let Some(v) = close_or_grow(vm, d, wd)? else {
                stop!(Ran::Full, $j);
            };
            if top {
                room = count.saturating_add(d.room.saturating_sub(1025) as u64);
            }
            (v, $j + 1)
        }};
    }
    macro_rules! fail {
        ($msg:expr, $j:expr) => {
            return Ok(Ran::Fail(($msg, base + $j)))
        };
    }
    loop {
        let j = skip_ws(b, i);
        let c = b.get(j).copied();
        if c.is_none() && !last {
            stop!(Ran::Cut, j);
        }
        let (v, end) = match st {
            at::VALUE | at::FIRST | at::ITEM => {
                if st != at::VALUE && c == Some(b']') {
                    if st == at::ITEM {
                        let comma = (wd[w::COMMA] as usize).saturating_sub(base);
                        fail!("Illegal trailing comma before end of array", comma);
                    }
                    close!(j)
                } else {
                    if st == at::FIRST && depth(vm, d) > MAX_DEPTH {
                        return Err(depth_error(vm, "array"));
                    }
                    if j >= cutoff {
                        stop!(Ran::More, j);
                    }
                    count += 1;
                    if count.is_multiple_of(1024) {
                        vm.burn(1024);
                        if vm.work.wrapping_sub(w0) >= STEP_WORK || count > room {
                            stop!(Ran::More, j);
                        }
                    }
                    match c {
                        None => fail!("Expecting value", j),
                        Some(b'"') => match string(vm, s, j + 1, d.strict, last)? {
                            Ok((t, e)) => (vm.new_str(t.text(s))?, e),
                            Err((CUT, _)) => stop!(Ran::Cut, j),
                            Err((m, p)) => fail!(m, p),
                        },
                        Some(b'{') => {
                            let res = d
                                .stack
                                .push(&mut vm.heap, Ctx::Obj(d.vals.len(), Value::NONE));
                            vm.hx(res)?;
                            (st, i) = (at::KEY0, j + 1);
                            continue;
                        }
                        Some(b'[') => {
                            let res = d.stack.push(&mut vm.heap, Ctx::Arr(d.vals.len()));
                            vm.hx(res)?;
                            (st, i) = (at::FIRST, j + 1);
                            continue;
                        }
                        Some(b'0'..=b'9') => match num_extent(b, j) {
                            // Digits may go on past the window.
                            Some((e, _)) if !last && e + 3 > b.len() => stop!(Ran::Cut, j),
                            Some((e, float)) => match number(vm, &s[j..e], float, d.raw)? {
                                Some(v) => (v, e),
                                None => fail!("Expecting value", j),
                            },
                            None => fail!("Expecting value", j),
                        },
                        Some(_) => {
                            let rest = &b[j..];
                            let lit = [
                                (&b"null"[..], Value::NONE),
                                (b"true", Value::TRUE),
                                (b"false", Value::FALSE),
                                (b"NaN", Value::float(f64::NAN)),
                                (b"Infinity", Value::float(f64::INFINITY)),
                                (b"-Infinity", Value::float(f64::NEG_INFINITY)),
                            ]
                            .into_iter()
                            .find(|(t, _)| rest.starts_with(t));
                            match lit {
                                Some((t, _)) if d.raw && !matches!(t[0], b'n' | b't' | b'f') => {
                                    (raw_text(vm, &s[j..j + t.len()])?, j + t.len())
                                }
                                Some((t, v)) => (v, j + t.len()),
                                None => match num_extent(b, j) {
                                    // Digits may go on past the window.
                                    Some((e, _)) if !last && e + 3 > b.len() => stop!(Ran::Cut, j),
                                    Some((e, float)) => match number(vm, &s[j..e], float, d.raw)? {
                                        Some(v) => (v, e),
                                        None => fail!("Expecting value", j),
                                    },
                                    None if !last && rest.len() < 9 => stop!(Ran::Cut, j),
                                    None => fail!("Expecting value", j),
                                },
                            }
                        }
                    }
                }
            }
            at::KEY0 | at::KEY => {
                if c == Some(b'}') {
                    if st == at::KEY {
                        let comma = (wd[w::COMMA] as usize).saturating_sub(base);
                        fail!("Illegal trailing comma before end of object", comma);
                    }
                    close!(j)
                } else {
                    if st == at::KEY0 && depth(vm, d) > MAX_DEPTH {
                        return Err(depth_error(vm, "object"));
                    }
                    if c != Some(b'"') {
                        fail!("Expecting property name enclosed in double quotes", j);
                    }
                    match string(vm, s, j + 1, d.strict, last)? {
                        Ok((t, e)) => {
                            let k = d.memo.key(vm, t.text(s))?;
                            set_key(d, k);
                            let n = skip_ws(b, e);
                            (st, i) = if b.get(n) == Some(&b':') {
                                (at::VALUE, n + 1)
                            } else {
                                (at::COLON, e)
                            };
                            continue;
                        }
                        Err((CUT, _)) => stop!(Ran::Cut, j),
                        Err((m, p)) => fail!(m, p),
                    }
                }
            }
            at::COLON => {
                if c != Some(b':') {
                    fail!("Expecting ':' delimiter", j);
                }
                (st, i) = (at::VALUE, j + 1);
                continue;
            }
            _ => {
                let obj = top_is_obj(d);
                if c == Some(if obj { b'}' } else { b']' }) {
                    close!(j)
                } else if c == Some(b',') {
                    wd[w::COMMA] = (base + j) as u64;
                    (st, i) = (if obj { at::KEY } else { at::ITEM }, j + 1);
                    continue;
                } else {
                    fail!("Expecting ',' delimiter", j);
                }
            }
        };
        if attach(vm, d, v)? {
            st = at::TRAIL;
            stop!(Ran::Done(v), end);
        }
        (st, i) = (at::AFTER, end);
    }
}

fn depth(vm: &Vm, d: &Dec) -> usize {
    d.stack.len() + vm.list_len(d.ctxs) / 3
}

fn top_is_obj(d: &Dec) -> bool {
    match d.stack.last() {
        Some(c) => matches!(c, Ctx::Obj(..)),
        None => d.top.is_some_and(|t| t.1),
    }
}

/// Sets the key of the innermost (object) container.
fn set_key(d: &mut Dec, k: Value) {
    match d.stack.last_mut() {
        Some(Ctx::Obj(_, slot)) => *slot = k,
        Some(Ctx::Arr(_)) => {}
        None => d.key = k,
    }
}

/// Puts `v` in the innermost open container; `true` when none is open
/// (`v` is the document's value).
#[allow(clippy::inline_always, reason = "json's scan loop: measured, bench Ir")]
#[inline(always)]
fn attach(vm: &mut Vm, d: &mut Dec, v: Value) -> Result<bool, Raised> {
    let key = match d.stack.last_mut() {
        Some(Ctx::Arr(_)) => None,
        Some(Ctx::Obj(_, k)) => Some(core::mem::replace(k, Value::NONE)),
        None => match d.top {
            None => return Ok(true),
            Some((_, false)) => None,
            Some((_, true)) => Some(core::mem::replace(&mut d.key, Value::NONE)),
        },
    };
    if let Some(k) = key {
        let res = d.vals.push(&mut vm.heap, k);
        vm.hx(res)?;
    }
    let res = d.vals.push(&mut vm.heap, v);
    vm.hx(res)?;
    Ok(false)
}

/// Adds the first `n` values of the stack to the innermost of `ctxs`.
fn flush(vm: &mut Vm, d: &Dec, n: usize) -> Result<(), Raised> {
    let items = d.vals.get(..n).unwrap_or(&[]);
    let Some((c, obj)) = d.top.filter(|_| n != 0) else {
        return Ok(());
    };
    if obj && !d.raw {
        for &[k, v] in items.as_chunks::<2>().0 {
            vm.dict_set(c, k, v)?;
        }
        return Ok(());
    }
    // List `c` takes `items` after its own, in a buffer grown once.
    let (Some((buf, len)), Some(r)) = (vm.list_parts(c), c.as_obj()) else {
        return Err(vm.bad_step());
    };
    let len = len as usize;
    let mut dst = buf;
    if len + n > vm.obj_len(buf) as usize {
        dst = vm.alloc_items((len + n).max(len * 2))?;
        // Copied through a stack buffer, as `Vm::list_push` grows a list:
        // a native copy of a long list would be as large again.
        let mut tmp = [Value::UNDEFINED; 256];
        let mut i = 0;
        while i < len {
            let k = {
                let src = vm.heap.slot_words(buf).unwrap_or(&[]);
                let src = src.get(i..len).unwrap_or(&[]);
                for (t, &w) in tmp.iter_mut().zip(src) {
                    *t = Value::from_bits(w).unwrap_or(Value::UNDEFINED);
                }
                src.len().min(tmp.len())
            };
            if k == 0 {
                return Err(vm.bad_step());
            }
            let res = vm.heap.set_values(dst, i as u32, &tmp[..k]);
            vm.hx(res)?;
            i += k;
        }
        vm.set_slot(r, 0, Value::from_ref(dst, ty::BUF as u8));
    }
    let res = vm.heap.set_values(dst, len as u32, items);
    vm.hx(res)?;
    vm.set_raw(r, 1, (len + n) as u64);
    Ok(())
}

/// Closes the innermost open container ([`close`]), or when it is the
/// innermost of `ctxs` and its parent's table must grow first, flags that
/// ([`UP`]) and gives `None`.
#[cfg_attr(mm_shipped, optimize(size))]
fn close_or_grow(vm: &mut Vm, d: &mut Dec, wd: &mut [u64; w::N]) -> Result<Option<Value>, Raised> {
    if d.stack.is_empty() && !d.raw && grow_target(vm, d.ctxs, true)?.is_some() {
        wd[w::FLAGS] |= UP;
        return Ok(None);
    }
    close(vm, d).map(Some)
}

/// Closes the innermost open container: its value.
fn close(vm: &mut Vm, d: &mut Dec) -> Result<Value, Raised> {
    if let Some(ctx) = d.stack.pop() {
        let start = ctx.start();
        let items = d.vals.get(start..).unwrap_or(&[]);
        let v = match ctx {
            Ctx::Arr(_) => vm.new_list(items)?,
            Ctx::Obj(..) if d.raw => vm.alloc_tuple(items)?,
            Ctx::Obj(..) => vm.dict_from_pairs(items)?,
        };
        d.vals.truncate(start);
        return Ok(v);
    }
    let (Some((c, obj)), Some(buf)) = (d.top, vm.list_buf(d.ctxs)) else {
        return Err(vm.bad_step());
    };
    flush(vm, d, d.vals.len())?;
    d.vals.truncate(0);
    let n = u32::try_from(vm.list_len(d.ctxs) - 3).unwrap_or(0);
    let res = vm.heap.set_values(buf, n, &[Value::NONE; 3]);
    vm.hx(res)?;
    if let Some(r) = d.ctxs.as_obj() {
        vm.set_raw(r, 1, u64::from(n));
    }
    d.load_top(vm);
    if obj && d.raw {
        // The pairs so far were a list.
        let items = vm.collect(c)?;
        return vm.alloc_tuple(&items);
    }
    Ok(c)
}

/// Ends a step: the values of the innermost of `ctxs` join it, and the
/// containers the step opened become arena containers of their items so
/// far, in `ctxs`.
fn spill(vm: &mut Vm, d: &mut Dec) -> Result<(), Raised> {
    flush(vm, d, d.stack.first().map_or(d.vals.len(), Ctx::start))?;
    if let (Some(buf), Some(_)) = (vm.list_buf(d.ctxs), d.top) {
        let n = vm.list_len(d.ctxs) - 2;
        vm.set_slot(buf, n as u32, d.key);
    }
    if !d.stack.is_empty() && d.ctxs == Value::NONE {
        d.ctxs = vm.new_list(&[])?;
    }
    for k in 0..d.stack.len() {
        let (start, key, obj) = match d.stack[k] {
            Ctx::Arr(s) => (s, Value::NONE, false),
            Ctx::Obj(s, key) => (s, key, true),
        };
        let end = d.stack.get(k + 1).map_or(d.vals.len(), Ctx::start);
        let items = d.vals.get(start..end).unwrap_or(&[]);
        let c = if obj && !d.raw {
            vm.dict_from_pairs(items)?
        } else {
            vm.new_list(items)?
        };
        for x in [c, key, Value::from_bool(obj)] {
            vm.list_push(d.ctxs, x)?;
        }
    }
    d.stack.truncate(0);
    d.vals.truncate(0);
    d.load_top(vm);
    Ok(())
}

/// A long token is read: a string (key or value), a number's value, or
/// its text. Goes on after it: `Some` with the document's value when it
/// ends the document under an index.
fn token_done(
    vm: &mut Vm,
    d: &mut Dec,
    wd: &mut [u64; w::N],
    aux: &mut Value,
    (v, end): (Value, usize),
) -> Result<Option<Value>, Raised> {
    wd[w::LONG] = 0;
    wd[w::I] = end as u64;
    *aux = Value::NONE;
    if matches!(wd[w::AT], at::KEY0 | at::KEY) {
        set_key(d, v);
        wd[w::AT] = at::COLON;
    } else if attach(vm, d, v)? {
        if wd[w::FLAGS] & IDX != 0 {
            return Ok(Some(v));
        }
        *aux = v;
        wd[w::AT] = at::TRAIL;
    } else {
        wd[w::AT] = at::AFTER;
    }
    Ok(None)
}

/// A step of the long token in `wd` (reading at most `room` input bytes):
/// the token's value and end once read, `None` while more remain, or a
/// failure.
fn long_step(
    vm: &mut Vm,
    sv: Value,
    d: &Dec,
    wd: &mut [u64; w::N],
    aux: &mut Value,
    room: usize,
) -> Result<Result<Option<(Value, usize)>, Fail>, Raised> {
    let q = wd[w::Q] as usize;
    let mut r = wd[w::R] as usize;
    let room = room.clamp(64, LONG_BYTES);
    match wd[w::LONG] {
        LONG_STR => {
            // Measure first: the decoded bytes and chars, and errors.
            let mut n = [wd[w::A], wd[w::B]];
            let (res, len) = {
                let b = vm.str_bytes(sv);
                let lim = b.len().min(r + room);
                (str_run(b, &mut r, lim, d.strict, q, &mut n, None), b.len())
            };
            vm.burn((r as u64 - wd[w::R]) / 16);
            (wd[w::R], wd[w::A], wd[w::B]) = (r as u64, n[0], n[1]);
            match res {
                Err(f) => return Ok(Err(f)),
                Ok(false) if r >= len => return Ok(Err(("Unterminated string starting at", q))),
                Ok(false) => return Ok(Ok(None)),
                Ok(true) => {}
            }
            let blk = alloc_block(vm, n[0])?;
            *aux = Value::from_byte_ref(blk);
            (wd[w::LONG], wd[w::C], wd[w::R], wd[w::D]) = (LONG_FILL, r as u64, q as u64 + 1, 0);
            Ok(Ok(None))
        }
        LONG_FILL | LONG_TEXT => {
            let Some(blk) = aux.as_byte_ref() else {
                return Err(vm.bad_step());
            };
            let end = wd[w::C] as usize;
            let mut at = wd[w::D] as usize;
            let mut buf: alloc::vec::Vec<u8> = alloc::vec::Vec::new();
            let lim = end.min(r + room);
            while r < lim {
                buf.clear();
                let step = (r + FILL_BYTES).min(lim);
                {
                    let b = vm.str_bytes(sv);
                    if wd[w::LONG] == LONG_TEXT {
                        buf.extend_from_slice(b.get(r..step).unwrap_or(&[]));
                        r = step;
                    } else {
                        // Measured already: only a forged position fails.
                        let mut n = [0; 2];
                        let res = str_run(b, &mut r, step, false, q, &mut n, Some(&mut buf));
                        if res.is_err() {
                            return Err(vm.bad_step());
                        }
                    }
                }
                let dst = vm.heap.bytes_mut(blk).ok();
                let Some(dst) = dst.and_then(|d| d.get_mut(at..at + buf.len())) else {
                    return Err(vm.bad_step());
                };
                dst.copy_from_slice(&buf);
                at += buf.len();
            }
            vm.burn((r as u64 - wd[w::R]) / 16);
            (wd[w::R], wd[w::D]) = (r as u64, at as u64);
            if r < end {
                return Ok(Ok(None));
            }
            let res = vm.heap.str_from_block(blk, wd[w::B]);
            let s = Value::from_ref(vm.hx(res)?, ty::STR as u8);
            let v = if wd[w::LONG] == LONG_TEXT {
                vm.alloc_tuple(&[s])?
            } else {
                s
            };
            Ok(Ok(Some((v, end))))
        }
        LONG_NUM => long_number(vm, sv, d, wd, aux, room),
        _ => Err(vm.bad_step()),
    }
}

/// A zero-filled byte block of `n` bytes for a long result.
fn alloc_block(vm: &mut Vm, n: u64) -> Result<mm_core::ByteRef, Raised> {
    let n = usize::try_from(n).unwrap_or(usize::MAX);
    vm.burn_result(n, true);
    let res = vm.heap.alloc_bytes_zeroed(n);
    vm.hx(res)
}

/// Reads string body `b[*r..]` up to about `lim` (an escape is read whole)
/// in the string whose quote is at `q`, as [`string`] does, adding its
/// decoded bytes and chars to `n` and, with `out`, the decoded text:
/// `true` after the closing quote.
fn str_run(
    b: &[u8],
    r: &mut usize,
    lim: usize,
    strict: bool,
    q: usize,
    n: &mut [u64; 2],
    mut out: Option<&mut alloc::vec::Vec<u8>>,
) -> Result<bool, Fail> {
    while *r < lim {
        let rest = b.get(*r..lim).unwrap_or(&[]);
        let k = rest
            .iter()
            .position(|&c| c == b'"' || c == b'\\' || (strict && c <= 0x1f))
            .unwrap_or(rest.len());
        let run = &rest[..k];
        n[0] += k as u64;
        n[1] += run.iter().filter(|&&c| c & 0xC0 != 0x80).count() as u64;
        if let Some(o) = out.as_deref_mut() {
            o.extend_from_slice(run);
        }
        *r += k;
        let Some(&c) = b.get(*r).filter(|_| *r < lim) else {
            break;
        };
        if c == b'"' {
            *r += 1;
            return Ok(true);
        }
        if c != b'\\' {
            return Err(("Invalid control character at", *r));
        }
        let (ch, e) = escape(b, *r, q)?;
        let mut tmp = [0u8; 4];
        let t = ch.encode_utf8(&mut tmp);
        n[0] += t.len() as u64;
        n[1] += 1;
        if let Some(o) = out.as_deref_mut() {
            o.extend_from_slice(t.as_bytes());
        }
        *r = e;
    }
    Ok(false)
}

/// A step of a long number (CPython's `_match_number_unicode`): its digit
/// runs are read in the arena; a float's value comes from its first 800
/// significant digits (more cannot change the rounding), an integer past
/// `sys.get_int_max_str_digits()` fails as `int()` does, and a raw
/// number's text is written into a block ([`LONG_TEXT`]).
fn long_number(
    vm: &mut Vm,
    sv: Value,
    d: &Dec,
    wd: &mut [u64; w::N],
    aux: &mut Value,
    room: usize,
) -> Result<Result<Option<(Value, usize)>, Fail>, Raised> {
    let q = wd[w::Q] as usize;
    let mut r = wd[w::R] as usize;
    let (done, len) = {
        let b = vm.str_bytes(sv);
        let digit = |j: usize| b.get(j).is_some_and(u8::is_ascii_digit);
        let lim = b.len().min(r + room);
        let mut done = false;
        while r < lim && !done {
            let part = wd[w::A];
            let k = b[r..lim]
                .iter()
                .position(|c| !c.is_ascii_digit())
                .unwrap_or(lim - r);
            if part < 2
                && let Some(nz) = b[r..r + k].iter().position(|&c| c != b'0')
            {
                wd[w::E] = wd[w::E].min((r + nz) as u64);
                let last = b[r..r + k].iter().rposition(|&c| c != b'0').unwrap_or(nz);
                wd[w::F] = (r + last) as u64;
            }
            r += k;
            if r == lim && lim < b.len() {
                break;
            }
            // The run ends at `r`: what follows it. (A long number does
            // not start with `0`: that integer part is one digit.)
            if part == 0 {
                wd[w::B] = r as u64;
                if b.get(r) == Some(&b'.') && digit(r + 1) {
                    (wd[w::A], r) = (1, r + 1);
                    continue;
                }
            }
            if part < 2 {
                if part == 1 {
                    wd[w::C] = r as u64;
                }
                if matches!(b.get(r), Some(b'e' | b'E')) {
                    let e = r + 1 + usize::from(matches!(b.get(r + 1), Some(b'-' | b'+')));
                    if digit(e) {
                        (wd[w::A], wd[w::D], r) = (2, (r + 1) as u64, e);
                        continue;
                    }
                }
            }
            done = true;
        }
        (done, b.len())
    };
    vm.burn((r as u64 - wd[w::R]) / 16);
    wd[w::R] = r as u64;
    if !done && r < len {
        return Ok(Ok(None));
    }
    let float = wd[w::C] != 0 || wd[w::D] != 0;
    if d.raw {
        let blk = alloc_block(vm, (r - q) as u64)?;
        *aux = Value::from_byte_ref(blk);
        (wd[w::LONG], wd[w::R], wd[w::B], wd[w::C], wd[w::D]) =
            (LONG_TEXT, q as u64, (r - q) as u64, r as u64, 0);
        return Ok(Ok(None));
    }
    let neg = vm.str_bytes(sv).get(q) == Some(&b'-');
    if !float {
        let digits = r - q - usize::from(neg);
        let limit = vm.str_digits_limit();
        if digits > limit {
            let msg = format!(
                "Exceeds the limit ({limit} digits) for integer string conversion: value has {digits} digits; use sys.set_int_max_str_digits() to increase the limit"
            );
            return Err(vm.err(T::ValueError, &msg));
        }
        // Allowed by `sys.set_int_max_str_digits`: one long conversion.
        let text = String::from(core::str::from_utf8(&vm.str_bytes(sv)[q..r]).unwrap_or(""));
        return match vm.parse_int_value(&text, 10)? {
            Some(v) => Ok(Ok(Some((v, r)))),
            None => Ok(Err(("Expecting value", q))),
        };
    }
    // 0.DIGITS × 10^exp, DIGITS from the first nonzero mantissa digit.
    let (int_end, frac_end, exp_at) = (wd[w::B] as usize, wd[w::C] as usize, wd[w::D] as usize);
    let mant_end = if frac_end != 0 { frac_end } else { int_end };
    let mut t = String::from(if neg { "-0." } else { "0." });
    let mut exp: i64 = 0;
    if let Some(nz) = usize::try_from(wd[w::E]).ok().filter(|&nz| nz < mant_end) {
        let b = vm.str_bytes(sv);
        exp = if nz < int_end {
            (int_end - nz) as i64
        } else {
            -((nz - int_end - 1) as i64)
        };
        let mut j = nz;
        while j < mant_end && t.len() < 803 {
            if b[j] != b'.' {
                t.push(char::from(b[j]));
            }
            j += 1;
        }
        if (wd[w::F] as usize) >= j {
            t.push('1');
        }
        if exp_at != 0 {
            let e = b.get(exp_at..r).unwrap_or(&[]);
            let (sign, e) = match e.first() {
                Some(b'-') => (-1, &e[1..]),
                Some(b'+') => (1, &e[1..]),
                _ => (1, e),
            };
            let mut x: i64 = 0;
            for &c in e {
                x = (x * 10 + i64::from(c - b'0')).min(1 << 40);
            }
            exp += sign * x;
        }
    } else {
        t.push('0');
    }
    let _ = write!(t, "e{exp}");
    Ok(Ok(Some((
        Value::float(t.parse::<f64>().unwrap_or(f64::NAN)),
        r,
    ))))
}

fn depth_error(vm: &mut Vm, what: &str) -> Raised {
    let msg = format!(
        "maximum recursion depth exceeded while decoding a JSON {what} from a unicode string"
    );
    vm.err(T::RecursionError, &msg)
}

fn hex4(b: &[u8], i: usize) -> Option<u32> {
    let mut c = 0;
    for &d in b.get(i..i + 4)? {
        c = c * 16 + char::from(d).to_digit(16)?;
    }
    Some(c)
}

/// The escape whose backslash is at `b[at]`, in the string whose quote is
/// at `q`: its char and the offset after it.
fn escape(b: &[u8], at: usize, q: usize) -> Result<(char, usize), Fail> {
    let Some(&c) = b.get(at + 1) else {
        return Err(("Unterminated string starting at", q));
    };
    if c != b'u' {
        let r = match c {
            b'"' | b'\\' | b'/' => c,
            b'b' => 8,
            b'f' => 12,
            b'n' => b'\n',
            b'r' => b'\r',
            b't' => b'\t',
            _ => return Err(("Invalid \\escape", at)),
        };
        return Ok((char::from(r), at + 2));
    }
    let mut end = at + 6;
    // A complete escape at the very end leaves the string unterminated.
    let Some(mut c) = hex4(b, at + 2).filter(|_| end <= b.len()) else {
        return Err(("Invalid \\uXXXX escape", at + 1));
    };
    // An escape right after a high surrogate is read as its pair's second
    // half, and fails as CPython's does when it is not a whole escape.
    if (0xD800..0xDC00).contains(&c) && b.get(end..end + 2) == Some(b"\\u") {
        let Some(c2) = hex4(b, end + 2).filter(|_| end + 6 <= b.len()) else {
            return Err(("Invalid \\uXXXX escape", end + 1));
        };
        if (0xDC00..0xE000).contains(&c2) {
            c = 0x10000 + ((c - 0xD800) << 10) + (c2 - 0xDC00);
            end += 6;
        }
    }
    // A str is UTF-8 and cannot hold a lone surrogate (docs/user-guide.md).
    match char::from_u32(c) {
        Some(ch) => Ok((ch, end)),
        None => Err((LONE_SURROGATE, at)),
    }
}

/// A scanned JSON string: a byte range of the input when it has no escapes.
enum Scanned {
    Range(usize, usize),
    Owned(Output),
}

impl Scanned {
    fn text<'a>(&'a self, s: &'a str) -> &'a str {
        match self {
            Scanned::Range(a, b) => s.get(*a..*b).unwrap_or(""),
            Scanned::Owned(t) => &t.text,
        }
    }
}

/// The string whose body starts at `start` (after the quote), and the
/// offset after its closing quote (CPython's `scanstring_unicode`; control
/// characters are an error only when `strict`). Unless `last`, a string
/// the window's end cuts is [`CUT`].
fn string(
    vm: &mut Vm,
    s: &str,
    start: usize,
    strict: bool,
    last: bool,
) -> Result<Result<(Scanned, usize), Fail>, Raised> {
    let b = s.as_bytes();
    let plain = b.get(start..).and_then(|rest| {
        if strict {
            rest.iter()
                .position(|&c| c == b'"' || c == b'\\' || c <= 0x1f)
        } else {
            rest.iter().position(|&c| c == b'"' || c == b'\\')
        }
    });
    if let Some(k) = plain
        && b[start + k] == b'"'
    {
        return Ok(Ok((Scanned::Range(start, start + k), start + k + 1)));
    }
    let begin = start - 1;
    let mut out = Output::default();
    let mut end = start;
    loop {
        let mut next = end;
        while next < b.len() && b[next] != b'"' && b[next] != b'\\' {
            if b[next] <= 0x1f && strict {
                return Ok(Err(("Invalid control character at", next)));
            }
            next += 1;
        }
        // An escape is read whole, with the pair a surrogate may start.
        if !last && next + 16 > b.len() {
            return Ok(Err((CUT, next)));
        }
        if next >= b.len() {
            return Ok(Err(("Unterminated string starting at", begin)));
        }
        out.push(vm, &s[end..next])?;
        if b[next] == b'"' {
            end = next + 1;
            break;
        }
        let (ch, e) = match escape(b, next, begin) {
            Ok(r) => r,
            Err(f) => return Ok(Err(f)),
        };
        end = e;
        let mut buf = [0u8; 4];
        out.push(vm, ch.encode_utf8(&mut buf))?;
    }
    Ok(Ok((Scanned::Owned(out), end)))
}

/// `(text,)`: a literal left for a `json.loads` hook.
fn raw_text(vm: &mut Vm, text: &str) -> Result<Value, Raised> {
    let t = vm.new_str(text)?;
    vm.alloc_tuple(&[t])
}

/// The end of the number at `i` (CPython's `_match_number_unicode`) and
/// whether it is a float, or `None`.
fn num_extent(b: &[u8], i: usize) -> Option<(usize, bool)> {
    let digit = |j: usize| b.get(j).is_some_and(u8::is_ascii_digit);
    let mut j = i;
    if b[j] == b'-' {
        j += 1;
    }
    match b.get(j) {
        Some(b'1'..=b'9') => {
            j += 1;
            while digit(j) {
                j += 1;
            }
        }
        Some(b'0') => j += 1,
        _ => return None,
    }
    let mut float = false;
    if b.get(j) == Some(&b'.') && digit(j + 1) {
        float = true;
        j += 2;
        while digit(j) {
            j += 1;
        }
    }
    if matches!(b.get(j), Some(b'e' | b'E')) {
        let e_start = j;
        j += 1;
        if matches!(b.get(j), Some(b'-' | b'+')) {
            j += 1;
        }
        while digit(j) {
            j += 1;
        }
        if digit(j - 1) {
            float = true;
        } else {
            j = e_start;
        }
    }
    Some((j, float))
}

/// The value of number `text`, or `None`; with `raw`, its text as
/// [`raw_text`].
fn number(vm: &mut Vm, text: &str, float: bool, raw: bool) -> Result<Option<Value>, Raised> {
    if raw {
        return raw_text(vm, text).map(Some);
    }
    if float {
        return Ok(Some(Value::float(text.parse::<f64>().unwrap_or(f64::NAN))));
    }
    if text.len() <= 18
        && let Ok(n) = text.parse::<i64>()
    {
        return vm.new_int(n).map(Some);
    }
    vm.parse_int_value(text, 10)
}
