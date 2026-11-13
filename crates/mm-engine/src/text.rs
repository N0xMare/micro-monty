//! `repr`, `str` and `format`: Python's text forms of builtin values.
//!
//! Container `repr` is iterative with an explicit work stack and a depth
//! limit, so deep nesting raises `RecursionError` instead of overflowing the
//! Rust stack; self-references print as `[...]`.

use crate::scratch::{NativeIds, NativeText, NativeVec};
use crate::types::{T, ty};
use crate::vm::{Raised, Vm, exc_slot, fn_slot};
use alloc::{
    format,
    string::{String, ToString},
    vec::Vec,
};
use core::fmt::Write;
use mm_core::{Error, Heap, Ref, Scratch, Value};

/// `s` in double quotes for error messages, as Rust's `{:?}` writes most
/// strings, without core's `Debug` escaping and its printable tables
/// (about 5 KB \[M\]): `"`, `\`, `\t`, `\r`, `\n` and `\0` escaped, other
/// control characters as `\u{..}`. Unlike `{:?}`, other unprintable
/// characters stay as they are.
#[must_use]
pub fn debug_quoted(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            '\n' => out.push_str("\\n"),
            '\0' => out.push_str("\\0"),
            c if c.is_control() => {
                let _ = write!(out, "\\u{{{:x}}}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Deepest container nesting `repr` follows.
const MAX_REPR_DEPTH: usize = 50_000;

/// Fuel units of work one native `repr` walk may do before the prelude
/// twin goes on (its visits and each big int's decimal text, checked
/// before converting it; about 25 ms), so a long container's text also
/// parks for a deadline or cancel. A constant, not the fuel left, so the
/// path and the fuel spent do not depend on how the host slices the run
/// (docs/spec.md §5.1, clause 4).
pub(crate) const REPR_SLICE: u64 = 1 << 17;

/// With [`Vm::set_int_bits_text`], ints longer than this many bits (a bound
/// for CPython's 4300-digit limit) render as `<int with N bits>`.
const INT_BITS_TEXT_ABOVE: u64 = 14_284;

/// Power-of-two digits a format writes in one go; past these a big int's
/// are written a step at a time ([`Vm::pow2_text`]).
pub(crate) const POW2_LONG: usize = 1 << 20;

/// What [`Vm::format_in`] makes: native text, or a str in the arena.
enum Fmt {
    Text(NativeText),
    Str(Value),
}

/// Python's `repr(float)`.
#[must_use]
pub fn float_repr(f: f64) -> String {
    if f.is_nan() {
        return "nan".into();
    }
    if f.is_infinite() {
        return if f > 0.0 { "inf".into() } else { "-inf".into() };
    }
    let s = format!("{f:e}");
    let (neg, s) = match s.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, s.as_str()),
    };
    let (mant, exp) = s.split_once('e').unwrap_or((s, "0"));
    let exp: i32 = exp.parse().unwrap_or(0);
    let mut digits: String = mant.chars().filter(char::is_ascii_digit).collect();
    nearest_shortest(f.abs(), &mut digits, exp);
    let mut out = String::new();
    if neg {
        out.push('-');
    }
    let decpt = exp + 1;
    let n = i32::try_from(digits.len()).unwrap_or(0);
    if (-4 < decpt) && decpt <= 16 {
        if decpt <= 0 {
            out.push_str("0.");
            for _ in 0..-decpt {
                out.push('0');
            }
            out.push_str(&digits);
        } else if decpt >= n {
            out.push_str(&digits);
            for _ in 0..decpt - n {
                out.push('0');
            }
            out.push_str(".0");
        } else {
            let d = usize::try_from(decpt).unwrap_or(0);
            out.push_str(&digits[..d]);
            out.push('.');
            out.push_str(&digits[d..]);
        }
    } else {
        out.push_str(&digits[..1]);
        if digits.len() > 1 {
            out.push('.');
            out.push_str(&digits[1..]);
        }
        let e = decpt - 1;
        let _ = write!(out, "e{}{:02}", if e < 0 { '-' } else { '+' }, e.abs());
    }
    out
}

/// `core` prints a shortest round-tripping digit string, but when two
/// strings of that length round-trip it may not pick the one nearest `f`
/// (CPython does, ties to even). Checks the neighbours of the last digit.
fn nearest_shortest(f: f64, digits: &mut String, exp: i32) {
    use crate::bigint::Big;
    let n = digits.len();
    if n < 2 {
        return;
    }
    let q = exp - (i32::try_from(n).unwrap_or(0) - 1);
    let last = digits.as_bytes()[n - 1];
    let candidates = [
        last.checked_sub(1).filter(|&c| c >= b'0'),
        Some(last + 1).filter(|&c| c <= b'9'),
    ];
    let parses = |d: u8| -> bool {
        let mut t = String::from(&digits[..n - 1]);
        t.push(char::from(d));
        let _ = write!(t, "e{q}");
        t.parse::<f64>().ok() == Some(f)
    };
    let alts: Vec<u8> = candidates
        .iter()
        .flatten()
        .copied()
        .filter(|&d| parses(d))
        .collect();
    if alts.is_empty() {
        return;
    }
    // Exact distances: scale D * 10^q and m * 2^e to integers.
    let bits = f.to_bits();
    let be = ((bits >> 52) & 0x7ff) as i64;
    let (m, e2) = if be == 0 {
        (bits & ((1 << 52) - 1), -1074)
    } else {
        ((bits & ((1 << 52) - 1)) | (1 << 52), be - 1075)
    };
    let ten = Big::from_i64(10);
    let scale10 = ten.pow(u64::from((-q).max(0).unsigned_abs()));
    let up10 = ten.pow(u64::from(q.max(0).unsigned_abs()));
    let fx = Big::from_u64(m, false)
        .shl(u64::try_from(e2.max(0)).unwrap_or(0))
        .mul(&scale10);
    let shift2 = u64::try_from((-e2).max(0)).unwrap_or(0);
    let dist = |d: u8| -> Big {
        let mut t = String::from(&digits[..n - 1]);
        t.push(char::from(d));
        let dv = Big::parse(t.as_bytes(), 10).unwrap_or_default();
        dv.mul(&up10).shl(shift2).sub(&fx).abs()
    };
    let mut best = last;
    let mut best_d = dist(last);
    for d in alts {
        let dd = dist(d);
        match dd.cmp(&best_d) {
            core::cmp::Ordering::Less => {
                best = d;
                best_d = dd;
            }
            core::cmp::Ordering::Equal if d % 2 == 0 => best = d,
            _ => {}
        }
    }
    if best != last {
        digits.pop();
        digits.push(char::from(best));
    }
}

/// Python's `repr(str)`.
#[must_use]
pub fn str_repr(s: &str) -> String {
    let mut out = String::new();
    quoted_repr(
        s.as_bytes(),
        false,
        &mut ReprWriter::new(&mut out, usize::MAX),
    );
    out
}

/// Keeps one UTF-8 character past the limit so callers can distinguish
/// an exact fit from truncation (including opaque host-value conversion).
pub(crate) struct ReprWriter<'a> {
    out: &'a mut String,
    limit: usize,
    budget: Option<(&'a Heap, &'a mut Scratch, &'a mut Option<Error>)>,
}

impl<'a> ReprWriter<'a> {
    pub(crate) fn new(out: &'a mut String, limit: usize) -> Self {
        Self {
            out,
            limit,
            budget: None,
        }
    }

    fn charged(
        out: &'a mut String,
        limit: usize,
        heap: &'a Heap,
        charge: &'a mut Scratch,
        failed: &'a mut Option<Error>,
    ) -> Self {
        Self {
            out,
            limit,
            budget: Some((heap, charge, failed)),
        }
    }

    pub(crate) fn full(&self) -> bool {
        self.out.len() > self.limit
            || self
                .budget
                .as_ref()
                .is_some_and(|(_, _, failed)| failed.is_some())
    }

    pub(crate) fn remaining(&self) -> usize {
        self.limit.saturating_sub(self.out.len())
    }

    #[inline(never)]
    fn reserve(&mut self, additional: usize) -> bool {
        let cap = self.limit.saturating_add(4);
        let needed = self.out.len().saturating_add(additional).min(cap);
        let old = self.out.capacity();
        if needed > old {
            let mut capacity = needed.max(old.saturating_mul(2)).min(cap);
            if let Some((heap, charge, failed)) = &mut self.budget {
                capacity = crate::scratch::grown(old, needed, 8, heap.available_bytes()).min(cap);
                if let Err(error) = charge.resize(heap, capacity) {
                    **failed = Some(error);
                    return false;
                }
                if self
                    .out
                    .try_reserve_exact(capacity - self.out.len())
                    .is_err()
                {
                    let _ = charge.resize(heap, old);
                    **failed = Some(Error::OutOfMemory);
                    return false;
                }
                return true;
            }
            self.out.reserve_exact(capacity - self.out.len());
        }
        true
    }

    #[inline(never)]
    pub(crate) fn push_str(&mut self, s: &str) {
        if self.full() {
            return;
        }
        let room = self.limit - self.out.len();
        let s = if s.len() > room {
            let mut end = room + 1;
            while !s.is_char_boundary(end) {
                end += 1;
            }
            &s[..end]
        } else {
            s
        };
        if s.len() > self.out.capacity() - self.out.len() && !self.reserve(s.len()) {
            return;
        }
        self.out.push_str(s);
    }

    #[inline(never)]
    pub(crate) fn escape(&mut self, c: char, quote: u8) {
        let simple = match c {
            '\\' => Some("\\\\"),
            '\t' => Some("\\t"),
            '\n' => Some("\\n"),
            '\r' => Some("\\r"),
            c if c == char::from(quote) => Some(if quote == b'\'' { "\\'" } else { "\\\"" }),
            _ => None,
        };
        if let Some(s) = simple {
            self.push_str(s);
            return;
        }
        let mut buf = *b"\\U00000000";
        let c = c as u32;
        let (prefix, digits) = if c < 0x100 {
            (b'x', 2)
        } else if c < 0x10000 {
            (b'u', 4)
        } else {
            (b'U', 8)
        };
        buf[1] = prefix;
        let mut n = c;
        for digit in buf[2..2 + digits].iter_mut().rev() {
            *digit = b"0123456789abcdef"[(n & 15) as usize];
            n >>= 4;
        }
        self.push_str(core::str::from_utf8(&buf[..2 + digits]).unwrap_or(""));
    }
}

impl Write for ReprWriter<'_> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        self.push_str(s);
        if self.full() {
            Err(core::fmt::Error)
        } else {
            Ok(())
        }
    }
}

/// Quoted builtin text. String inputs are UTF-8; byte inputs escape every
/// non-ASCII byte. Printable runs share the same bounded writer.
#[inline(never)]
pub(crate) fn quoted_repr(s: &[u8], bytes: bool, out: &mut ReprWriter<'_>) {
    quoted(s, bytes, false, out);
}

/// A bytearray's data as its repr quotes it: as bytes, except that a `'`
/// is escaped inside `"` quotes too (CPython's `bytearray_repr`).
pub(crate) fn bytearray_quoted(s: &[u8], out: &mut ReprWriter<'_>) {
    quoted(s, true, true, out);
}

fn quoted(s: &[u8], bytes: bool, apostrophe: bool, out: &mut ReprWriter<'_>) {
    out.reserve(s.len().saturating_add(if bytes { 3 } else { 2 }));
    let q = if s.contains(&b'\'') && !s.contains(&b'"') {
        "\""
    } else {
        "'"
    };
    let quote = q.as_bytes()[0];
    if bytes {
        out.push_str("b");
    }
    out.push_str(q);
    let mut chars = core::str::from_utf8(if bytes { &[] } else { s })
        .unwrap_or("")
        .chars();
    let mut start = 0;
    let mut room = out.remaining();
    let mut at = 0;
    while at < s.len() {
        if at - start > room {
            out.push_str(core::str::from_utf8(&s[start..at]).unwrap_or(""));
            return;
        }
        let c = if bytes {
            char::from(s[at])
        } else if let Some(c) = chars.next() {
            c
        } else {
            break;
        };
        let width = if bytes { 1 } else { c.len_utf8() };
        let printable = if bytes {
            (' '..'\x7f').contains(&c)
        } else {
            is_printable(c)
        };
        if c != '\\' && c != char::from(quote) && !(apostrophe && c == '\'') && printable {
            at += width;
            continue;
        }
        if at > start {
            out.push_str(core::str::from_utf8(&s[start..at]).unwrap_or(""));
        }
        if c == '\'' {
            out.push_str("\\'");
        } else {
            out.escape(c, quote);
        }
        if out.full() {
            return;
        }
        at += width;
        start = at;
        room = out.remaining();
    }
    out.push_str(core::str::from_utf8(&s[start..]).unwrap_or(""));
    out.push_str(q);
}

/// `str.isprintable`'s table (tools/gen_ucd.py, `printable.bin`): u16
/// plane starts, then the low 16 bits of each boundary code point.
static PRINTABLE: &[u8] = include_bytes!("ucd/printable.bin");

fn printable_entry(i: usize) -> u32 {
    PRINTABLE
        .get(2 * i..2 * i + 2)
        .map_or(0, |b| u32::from(b[0]) | u32::from(b[1]) << 8)
}

/// `str.isprintable` for one character, which also decides `repr`
/// escaping. Exact for Unicode 16.0, with or without the `unicodedata`
/// feature: a code point is printable when an even number of boundaries
/// lie at or below it.
#[must_use]
pub fn is_printable(c: char) -> bool {
    if c.is_ascii() {
        return (' '..'\x7f').contains(&c);
    }
    let cp = u32::from(c);
    let plane = (cp >> 16) as usize;
    let (mut lo, mut hi) = (
        printable_entry(plane) as usize,
        printable_entry(plane + 1) as usize,
    );
    while lo < hi {
        let mid = usize::midpoint(lo, hi);
        if printable_entry(18 + mid) <= cp & 0xFFFF {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    lo % 2 == 0
}

enum Work {
    Val(Value, usize),
    Lit(&'static str),
    Leave(Value),
    // Buffer, next slot, end slot, child depth, container type. Rendering
    // neither mutates containers nor collects, so cached storage stays valid.
    Items(Ref, u32, u32, usize, T),
}

/// The decimal digits of `i` at the end of `buf`; returns where they start.
pub(crate) fn i64_digits(i: i64, buf: &mut [u8; 20]) -> usize {
    let mut n = i.unsigned_abs();
    let mut k = buf.len();
    loop {
        k -= 1;
        #[allow(clippy::cast_possible_truncation, reason = "a digit")]
        if let Some(b) = buf.get_mut(k) {
            *b = b'0' + (n % 10) as u8;
        }
        n /= 10;
        if n == 0 || k == 0 {
            break;
        }
    }
    if i < 0 && k > 0 {
        k -= 1;
        if let Some(b) = buf.get_mut(k) {
            *b = b'-';
        }
    }
    k
}

impl Vm {
    /// `str(i)`: the decimal text of `i` as a new `str`, written straight
    /// into the arena as ASCII (no UTF-8 validation or char count). One
    /// char is interned, as [`Vm::new_str`] does.
    pub(crate) fn int_text(&mut self, i: i64) -> Result<Value, Raised> {
        let mut buf = [0u8; 20];
        let k = i64_digits(i, &mut buf);
        let digits = buf.get(k..).unwrap_or(&[]);
        if digits.len() <= 1 {
            return self.new_str(core::str::from_utf8(digits).unwrap_or(""));
        }
        let res = self.heap.new_str_ascii(digits);
        let r = self.hx_o(res)?;
        Ok(Value::from_ref(r, ty::STR as u8))
    }

    /// `repr(v)` inside a cell. Text longer than the memory limit (a small
    /// graph of shared containers can describe one) raises `MemoryError`.
    /// A list, tuple, set, dict or view whose walk would visit more items
    /// than [`REPR_SLICE`] continues in the prelude twin (`_repr_of`,
    /// through [`Vm::need_py`]) after charging the visits, so the rest parks
    /// at safepoints. So does a long str or bytes-like, whose repr
    /// `repr()` builds a step at a time.
    pub(crate) fn repr(&mut self, v: Value) -> Result<NativeText, Raised> {
        if !self.host_text && crate::strmeth::big_repr(self, v) {
            return Err(self.need_py());
        }
        let budget = (!self.host_text && Vm::repr_twin_handles(self, v)).then_some(REPR_SLICE);
        let mut out = NativeText::default();
        let limit = usize::try_from(self.heap.memory_limit()).unwrap_or(usize::MAX);
        // In a host report the text stops at the report's cap (or the
        // deadline) and is kept, cut ([`Vm::begin_report`]).
        let report = self.report_cap != usize::MAX;
        match self.repr_walk(
            v,
            &mut out,
            limit.min(self.report_cap),
            0,
            Value::NONE,
            budget,
        )? {
            Some(true) => Ok(out),
            Some(false) if report => {
                self.report_cut = true;
                Ok(out)
            }
            Some(false) => Err(self.err(T::MemoryError, "")),
            None => Err(self.need_py()),
        }
    }

    /// `repr(v)` that never hands off: for the host after a cell, and for
    /// error messages raised where no prelude twin can continue.
    pub(crate) fn repr_whole(&mut self, v: Value) -> Result<NativeText, Raised> {
        let mut out = NativeText::default();
        let limit = usize::try_from(self.heap.memory_limit()).unwrap_or(usize::MAX);
        if !self.repr_into(v, &mut out, limit)? {
            return Err(self.err(T::MemoryError, ""));
        }
        Ok(out)
    }

    /// Whether the prelude's `_repr_of` renders `v` as the native walk does:
    /// the builtin containers it walks level by level.
    fn repr_twin_handles(&self, v: Value) -> bool {
        !v.is_obj_kind(ty::INST as u8)
            && matches!(
                self.type_of(v),
                T::list
                    | T::tuple
                    | T::set
                    | T::frozenset
                    | T::dict
                    | T::dict_keys
                    | T::dict_values
                    | T::dict_items
            )
    }

    /// `repr(v)`, stopped once it is longer than `limit` bytes (it may then
    /// end anywhere past `limit`).
    pub(crate) fn repr_capped(&mut self, v: Value, limit: usize) -> Result<NativeText, Raised> {
        let mut out = NativeText::default();
        self.repr_into(v, &mut out, limit)?;
        Ok(out)
    }

    /// Takes user-rendered `(repr, str)` text, copying at most `limit` bytes
    /// from each arena string, at character boundaries. Each flag says whether
    /// that string was truncated. Returned buffers belong to the caller.
    pub fn take_completion_text_capped(
        &mut self,
        limit: usize,
    ) -> Option<((String, bool), (String, bool))> {
        let (r, s) = self.completion_text.take()?;
        let cap = |v| {
            let s = self.str_ref_o(v);
            (
                String::from(&s[..floor_boundary(s, limit)]),
                s.len() > limit,
            )
        };
        Some((cap(r), cap(s)))
    }

    /// `str(v)` cut to at most `limit` bytes (at a character boundary), and
    /// whether it was cut.
    pub fn str_capped(&mut self, v: Value, limit: usize) -> Result<(String, bool), Raised> {
        if Vm::is_str(v) {
            let s = self.str_ref_o(v);
            return Ok((
                String::from(&s[..floor_boundary(s, limit)]),
                s.len() > limit,
            ));
        }
        if let Some(b) = self.str_backing(v) {
            return self.str_capped(b, limit);
        }
        let mut out = if v.is_obj_kind(ty::EXC as u8) {
            self.host_text = true;
            let r = self.exc_str(v);
            self.host_text = false;
            r?
        } else if self.uflags(v) & crate::class::f::STR != 0 {
            return Err(self.need_py());
        } else {
            self.repr_capped(v, limit)?
        };
        let cut = out.len() > limit;
        out.text.truncate(floor_boundary(&out, limit));
        if out.text.capacity() > limit {
            out.text.shrink_to_fit();
        }
        Ok((out.text, cut))
    }

    /// `str(v)`.
    pub(crate) fn stringify(&mut self, v: Value) -> Result<NativeText, Raised> {
        if Vm::is_str(v) {
            let mut out = NativeText::default();
            out.push_value(self, v)?;
            return Ok(out);
        }
        if v.is_obj_kind(ty::EXC as u8) {
            return self.exc_str(v);
        }
        if let Some(b) = self.str_backing(v) {
            let mut out = NativeText::default();
            out.push_value(self, b)?;
            return Ok(out);
        }
        if self.uflags(v) & crate::class::f::STR != 0 {
            return Err(self.need_py());
        }
        self.repr(v)
    }

    /// The `str` a `str` subclass instance without its own `__str__`
    /// extends (`str(x)` is that value, whatever its `__repr__`).
    pub(crate) fn str_backing(&self, v: Value) -> Option<Value> {
        self.backing(v)
            .filter(|&b| Vm::is_str(b) && self.uflags(v) & crate::class::f::STR == 0)
    }

    /// Decode borrowed slots without duplicating validation in each text walk.
    #[inline(never)]
    fn text_value(bits: u64) -> Value {
        Value::from_bits(bits).unwrap_or(Value::UNDEFINED)
    }

    /// `str(e)` for an exception.
    pub(crate) fn exc_str(&mut self, e: Value) -> Result<NativeText, Raised> {
        let cached = self.vslot_o(e, exc_slot::STR);
        if Vm::is_str(cached) {
            let mut out = NativeText::default();
            out.push_value(self, cached)?;
            return Ok(out);
        }
        if self.uflags(e) & crate::class::f::STR != 0 {
            return Err(self.need_py());
        }
        let args = self.vslot_o(e, exc_slot::ARGS);
        let items = args
            .as_obj()
            .and_then(|r| self.heap.slot_words(r).ok())
            .unwrap_or(&[]);
        // Exception arguments can cycle, including through OSError's errno
        // and message. Use VM frames/fuel rather than recursing in Rust.
        if items
            .iter()
            .any(|&bits| Self::text_value(bits).is_obj_kind(ty::EXC as u8))
        {
            return Err(self.need_py());
        }
        if let Some(t) = self.syntax_str(e)? {
            let mut s = NativeText::default();
            s.push(self, &t)?;
            return Ok(s);
        }
        let items = args
            .as_obj()
            .and_then(|r| self.heap.slot_words(r).ok())
            .unwrap_or(&[]);
        match items {
            [] => Ok(NativeText::default()),
            // `[Errno 2] No such file or directory: 'name' -> 'name2'`
            [no, msg, rest @ ..] if rest.len() <= 3 && self.exc_type(e).is_subclass(T::OSError) => {
                let (no, msg) = (Self::text_value(*no), Self::text_value(*msg));
                let (file, file2) = (
                    rest.first().copied().map(Self::text_value),
                    rest.get(2).copied().map(Self::text_value),
                );
                let mut s = NativeText::default();
                s.push(self, "[Errno ")?;
                let text = self.stringify(no)?;
                s.push(self, &text)?;
                drop(text);
                s.push(self, "] ")?;
                let text = self.stringify(msg)?;
                s.push(self, &text)?;
                drop(text);
                if let Some(f) = file.filter(|f| *f != Value::NONE) {
                    s.push(self, ": ")?;
                    let text = self.repr(f)?;
                    s.push(self, &text)?;
                    drop(text);
                    if let Some(f2) = file2.filter(|f| *f != Value::NONE) {
                        s.push(self, " -> ")?;
                        let text = self.repr(f2)?;
                        s.push(self, &text)?;
                    }
                }
                Ok(s)
            }
            [a] => {
                let a = Self::text_value(*a);
                if self.type_of(e) == T::KeyError {
                    self.repr(a)
                } else {
                    self.stringify(a)
                }
            }
            _ => self.repr(args),
        }
    }

    /// `module.qualname` of a class for reprs (no module for builtins).
    pub(crate) fn class_repr_name(&self, c: Value) -> String {
        if !Vm::is_class(c) {
            return String::from(self.class_name(c));
        }
        let q = self.str_ref_o(self.vslot_o(c, crate::class::cls::QUALNAME));
        let d = self.vslot_o(c, crate::class::cls::DICT);
        let m = self
            .find_interned("__module__")
            .and_then(|k| self.dict_get_str(d, k));
        match m {
            Some(m) if Vm::is_str(m) && self.str_ref_o(m) != "builtins" => {
                format!("{}.{q}", self.str_ref_o(m))
            }
            _ => String::from(q),
        }
    }

    /// Backing storage for builtin container text, bounded to used slots.
    /// These references stay valid while rendering/preflight remains read-only.
    #[inline(never)]
    fn text_items(&self, v: Value, t: T) -> Option<(Ref, u32)> {
        match t {
            T::list => self.list_parts(v).map(|(buf, n)| {
                (
                    buf,
                    self.obj_len(buf).min(u32::try_from(n).unwrap_or(u32::MAX)),
                )
            }),
            T::tuple => v.as_obj().map(|buf| (buf, self.obj_len(buf))),
            T::set | T::frozenset => self
                .set_parts(v)
                .map(|(_, buf, _, n)| (buf, u32::try_from(n).unwrap_or(u32::MAX))),
            _ => self.dict_items_buf(if t == T::dict { v } else { self.vslot_o(v, 0) }),
        }
    }

    /// Does `repr`/`str` of `v` (a completion value) need Python code?
    pub(crate) fn text_needs_py(&mut self, v: Value) -> bool {
        if v.as_obj().is_none() {
            return false;
        }
        // The scan is charged as byte work: one unit per 8 slots.
        let mut scanned = 0;
        let r = self.text_needs_py_scan(v, &mut scanned);
        self.burn(scanned / 8);
        r
    }

    fn text_needs_py_scan(&mut self, v: Value, scanned: &mut u64) -> bool {
        // The existing depth bound fits a fixed cursor stack. Keep heap
        // locations rather than borrows so the identity table can reserve
        // through the heap; neither operation mutates or collects objects.
        let mut work = [(Ref::from_raw(0), 0, T::NoneType); 65];
        let mut depth = 0;
        let mut seen = NativeIds::default();
        let mut v = v;
        loop {
            if depth > 64 || seen.len() > 100_000 {
                return true;
            }
            if self.uflags(v) & (crate::class::f::REPR | crate::class::f::STR) != 0
                || v.is_obj_kind(ty::TASK as u8)
                || v.is_obj_kind(ty::FUTURE as u8)
            {
                return true;
            }
            if v.is_obj_kind(ty::EXC as u8) && self.exc_needs_py(v) {
                return true;
            }
            if matches!(
                v.obj_kind().map(u16::from),
                Some(ty::LIST | ty::TUPLE | ty::DICT | ty::SET | ty::METHOD | ty::EXC)
            ) && match seen.insert(&mut self.heap, v) {
                Ok(inserted) => inserted,
                // As at the depth/visit threshold, use budgeted Python
                // rendering when native preflight cannot finish cheaply.
                Err(_) => return true,
            } {
                let t = self.type_of(v);
                let parts = match t {
                    T::list | T::tuple | T::set | T::frozenset | T::dict => self.text_items(v, t),
                    T::method => v.as_obj().map(|r| (r, 1)),
                    _ => self.text_items(self.vslot_o(v, exc_slot::ARGS), T::tuple),
                };
                if let Some((r, n)) = parts {
                    let slots = self.heap.slot_words(r).unwrap_or(&[]);
                    work[depth] = (r, slots.len().min(n as usize), t);
                    depth += 1;
                }
            }
            v = 'next: loop {
                if depth == 0 {
                    return false;
                }
                let skip_leaves = depth <= 64 && seen.len() <= 100_000;
                let (r, n, t) = &mut work[depth - 1];
                let slots = self.heap.slot_words(*r).unwrap_or(&[]);
                while *n > 0 {
                    *scanned += 1;
                    let bits = slots[*n - 1];
                    // Keep the original last-child-first scan, skipping
                    // deleted dict pairs together and vacant set slots.
                    if *t == T::dict
                        && *n % 2 == 0
                        && Self::text_value(slots[*n - 2]) == Value::UNDEFINED
                    {
                        *n -= 2;
                        continue;
                    }
                    *n -= 1;
                    let child = Self::text_value(bits);
                    if matches!(*t, T::set | T::frozenset) && child == Value::UNDEFINED {
                        continue;
                    }
                    // Primitive leaves need neither user-code checks nor
                    // identity admission; scan them under one heap borrow.
                    if skip_leaves && child.as_obj().is_none() {
                        continue;
                    }
                    break 'next child;
                }
                depth -= 1;
            };
        }
    }

    /// Does reporting exception `e` (and its chain) need Python code for
    /// `str()`?
    #[inline(never)]
    pub(crate) fn exc_needs_py(&self, e: Value) -> bool {
        let mut x = e;
        for _ in 0..100 {
            if !x.is_obj_kind(ty::EXC as u8) {
                return false;
            }
            if self.uflags(x) & crate::class::f::STR != 0
                && !Vm::is_str(self.vslot_o(x, exc_slot::STR))
            {
                return true;
            }
            let args = self.vslot_o(x, exc_slot::ARGS);
            let items = args
                .as_obj()
                .and_then(|r| self.heap.slot_words(r).ok())
                .unwrap_or(&[]);
            if items.iter().any(|&bits| {
                let a = Self::text_value(bits);
                a.is_obj_kind(ty::EXC as u8)
                    || self.uflags(a) & (crate::class::f::REPR | crate::class::f::STR) != 0
            }) {
                return true;
            }
            let c = self.vslot_o(x, exc_slot::CAUSE);
            x = if c.is_obj_kind(ty::EXC as u8) {
                c
            } else {
                self.vslot_o(x, exc_slot::CONTEXT)
            };
        }
        false
    }

    fn addr(v: Value) -> u64 {
        0x7f00_0000_0000 + (u64::from(v.as_obj().map_or(0, Ref::raw)) << 3)
    }

    /// Appends `repr(v)` to `out`; `false` if it stopped because `out`
    /// grew longer than `limit` bytes.
    pub(crate) fn repr_into(
        &mut self,
        v: Value,
        out: &mut NativeText,
        limit: usize,
    ) -> Result<bool, Raised> {
        Ok(self
            .repr_walk(v, out, limit, 0, Value::NONE, None)?
            .unwrap_or(true))
    }

    /// Whether container `v` is one the prelude is rendering (`outer`, its
    /// `_REPR_IDS`). Looked up per container met: copying them into the
    /// walk's own set cost every walk the whole nesting, quadratic in its
    /// depth (30,000 nested dicts: 32 s [M]).
    fn repr_outer_has(&mut self, outer: Value, v: Value) -> Result<bool, Raised> {
        let Some(r) = v.as_obj().filter(|_| outer.is_obj_kind(ty::DICT as u8)) else {
            return Ok(false);
        };
        // `id(v)`, as `bi_id` gives it.
        let id = self.new_int(0x7f00_0000_0000 + (i64::from(r.raw()) << 3))?;
        Ok(self.dict_get(outer, id)?.is_some())
    }

    /// [`Vm::repr_into`] for a value at nesting `from` inside containers
    /// the prelude is rendering (`outer`, its `_REPR_IDS` dict from `id` to
    /// container, or `None`: shown as `...` if met again), doing at most
    /// `budget` fuel units of work: `None` once it would do more (before a
    /// long text, or a big int whose conversion would pass it), its visits
    /// charged.
    #[allow(clippy::too_many_lines, reason = "one arm per type")]
    pub(crate) fn repr_walk(
        &mut self,
        v: Value,
        out: &mut NativeText,
        limit: usize,
        from: usize,
        outer: Value,
        budget: Option<u64>,
    ) -> Result<Option<bool>, Raised> {
        let res = out.parts(&mut self.heap);
        let (out, charge) = self.hx_o(res)?;
        let start = out.len();
        let mut failed = None;
        // Borrow the heap only during a write, not across traversal mutations.
        macro_rules! writer {
            () => {
                ReprWriter::charged(out, limit, &self.heap, charge, &mut failed)
            };
        }
        let mut work = NativeVec::default();
        let mut first = Some(Work::Val(v, from));
        let mut active = NativeIds::default();
        let mut units = 0;
        // The budget is fuel units of work since the walk began: visits,
        // and each big int's decimal conversion by its size.
        let begun = self.work;
        while let Some(w) = first.take().or_else(|| work.pop()) {
            // Shared containers can expand exponentially. Charge traversal,
            // not distinct heap objects, and poll at the next safepoint.
            // Cursors only schedule the same values/literals as before.
            units += u64::from(!matches!(w, Work::Items(..)));
            if units == 128 {
                self.burn(units);
                units = 0;
                if self.report_stop(0, true) {
                    return Ok(Some(false));
                }
                if budget.is_some_and(|b| self.work.wrapping_sub(begun) > b) {
                    self.burn((out.len().saturating_sub(start) / 64) as u64);
                    return Ok(None);
                }
            }
            if let Some(error) = failed {
                return self.hx_o(Err(error));
            }
            if out.len() > limit {
                return Ok(Some(false));
            }
            let (v, depth) = match w {
                Work::Lit(s) => {
                    writer!().push_str(s);
                    continue;
                }
                Work::Leave(id) => {
                    active.remove(id);
                    continue;
                }
                Work::Items(buf, mut at, end, depth, t) => {
                    let first = at == 0;
                    let pair = matches!(t, T::dict | T::dict_keys | T::dict_values | T::dict_items);
                    let stride = if pair { 2 } else { 1 };
                    let mut k = Value::UNDEFINED;
                    while at < end {
                        k = self.slot_o(buf, at);
                        if k != Value::UNDEFINED || matches!(t, T::list | T::tuple) {
                            break;
                        }
                        at += stride;
                    }
                    if at == end {
                        continue;
                    }
                    let x = if pair {
                        self.slot_o(buf, at + 1)
                    } else {
                        Value::NONE
                    };
                    let res =
                        work.push(&mut self.heap, Work::Items(buf, at + stride, end, depth, t));
                    self.hx_o(res)?;
                    match t {
                        T::dict => {
                            let res = work.push(&mut self.heap, Work::Val(x, depth));
                            self.hx_o(res)?;
                            let res = work.push(&mut self.heap, Work::Lit(": "));
                            self.hx_o(res)?;
                            let res = work.push(&mut self.heap, Work::Val(k, depth));
                            self.hx_o(res)?;
                        }
                        T::dict_values => {
                            let res = work.push(&mut self.heap, Work::Val(x, depth));
                            self.hx_o(res)?;
                        }
                        T::dict_items => {
                            let res = work.push(&mut self.heap, Work::Lit(")"));
                            self.hx_o(res)?;
                            let res = work.push(&mut self.heap, Work::Val(x, depth));
                            self.hx_o(res)?;
                            let res = work.push(&mut self.heap, Work::Lit(", "));
                            self.hx_o(res)?;
                            let res = work.push(&mut self.heap, Work::Val(k, depth));
                            self.hx_o(res)?;
                            let res = work.push(&mut self.heap, Work::Lit("("));
                            self.hx_o(res)?;
                        }
                        _ => {
                            let res = work.push(&mut self.heap, Work::Val(k, depth));
                            self.hx_o(res)?;
                        }
                    }
                    if !first {
                        let res = work.push(&mut self.heap, Work::Lit(", "));
                        self.hx_o(res)?;
                    }
                    continue;
                }
                Work::Val(v, d) => (v, d),
            };
            if let Some(i) = v.as_int() {
                let _ = write!(writer!(), "{i}");
                continue;
            }
            if let Some(f) = v.as_float() {
                writer!().push_str(&float_repr(f));
                continue;
            }
            // A long text, or a big int once the budget is spent, is left
            // to the twin, which renders it a step at a time (`repr`) or
            // in a call of its own.
            if budget.is_some_and(|b| {
                crate::strmeth::big_repr(self, v)
                    || self.work.wrapping_sub(begun) + self.decimal_units(v) > b
            }) {
                self.burn((out.len().saturating_sub(start) / 64) as u64);
                return Ok(None);
            }
            if Vm::is_str(v) {
                quoted_repr(self.str_ref_o(v).as_bytes(), false, &mut writer!());
                continue;
            }
            if Vm::is_bytes(v) {
                quoted_repr(self.bytes_data(v), true, &mut writer!());
                continue;
            }
            if Vm::is_bytearray(v) {
                writer!().push_str("bytearray(");
                bytearray_quoted(self.bytes_data(v), &mut writer!());
                writer!().push_str(")");
                continue;
            }
            if Vm::is_heap_int(v) {
                if self.int_bits_text {
                    let bits = self.heap_int_bits(v);
                    if bits > INT_BITS_TEXT_ABOVE {
                        let _ = write!(writer!(), "<int with {bits} bits>");
                        continue;
                    }
                }
                let text = self.int_decimal(v)?;
                writer!().push_str(&text);
                continue;
            }
            if let Some((re, im)) = self.complex_of(v) {
                writer!().push_str(&crate::complex::repr(re, im));
                continue;
            }
            if self.uflags(v) & crate::class::f::REPR != 0 {
                return Err(self.need_py());
            }
            if let Some(b) = self.backing(v) {
                // A set subclass shows its class, as CPython's `set_repr`:
                // `S()`, `S({1})`.
                if matches!(self.type_of(b), T::set | T::frozenset) {
                    let name = String::from(self.class_name(self.class_of(v)));
                    let _ = write!(writer!(), "{name}(");
                    let res = work.push(&mut self.heap, Work::Lit(")"));
                    self.hx_o(res)?;
                    if self.set_len(b) == 0 {
                        continue;
                    }
                }
                let res = work.push(&mut self.heap, Work::Val(b, depth));
                self.hx_o(res)?;
                continue;
            }
            if let Some(kind) = v.obj_kind().map(u16::from)
                && matches!(
                    kind,
                    ty::INST
                        | ty::CLASS
                        | ty::METHOD
                        | ty::GEN
                        | ty::AGEN
                        | ty::AGENOP
                        | ty::PROPERTY
                        | ty::STATICMETHOD
                        | ty::CLASSMETHOD
                        | ty::SUPER
                        | ty::CALLITER
                )
            {
                match kind {
                    ty::INST => {
                        let c = self.vslot_o(v, crate::class::inst::CLASS);
                        let name = self.class_repr_name(c);
                        let _ = write!(writer!(), "<{name} object at 0x{:x}>", Vm::addr(v));
                    }
                    ty::CLASS => writer!().push_str(&self.class_repr(v)),
                    ty::METHOD => {
                        let f = self.vslot_o(v, 1);
                        // A callable instance (an lru_cache wrapper) shows
                        // its own `__qualname__`, as CPython's getattr finds.
                        let own = if f.is_obj_kind(ty::INST as u8) {
                            self.find_interned("__qualname__")
                                .and_then(|k| self.inst_get(f, k))
                                .filter(|&q| Vm::is_str(q))
                        } else {
                            None
                        };
                        let q = if f.is_obj_kind(ty::FUNC as u8) {
                            self.str_ref_o(self.vslot_o(f, fn_slot::QUALNAME))
                        } else if let Some(q) = own {
                            self.str_ref_o(q)
                        } else {
                            "?"
                        };
                        let _ = write!(writer!(), "<bound method {q} of ");
                        let res = work.push(&mut self.heap, Work::Lit(">"));
                        self.hx_o(res)?;
                        let receiver = self.vslot_o(v, 0);
                        let res = work.push(&mut self.heap, Work::Val(receiver, depth + 1));
                        self.hx_o(res)?;
                    }
                    ty::GEN | ty::AGEN => writer!().push_str(&self.gen_repr(v)),
                    ty::AGENOP => {
                        let t = self.type_of(v).name();
                        let _ = write!(writer!(), "<{t} object at 0x{:x}>", Vm::addr(v));
                    }
                    ty::STATICMETHOD | ty::CLASSMETHOD => {
                        let k = if kind == ty::STATICMETHOD {
                            "staticmethod"
                        } else {
                            "classmethod"
                        };
                        let _ = write!(writer!(), "<{k}(");
                        let res = work.push(&mut self.heap, Work::Lit(")>"));
                        self.hx_o(res)?;
                        let wrapped = self.vslot_o(v, 0);
                        let res = work.push(&mut self.heap, Work::Val(wrapped, depth + 1));
                        self.hx_o(res)?;
                    }
                    ty::SUPER => {
                        // CPython names both types by `tp_name`: a class's
                        // bare `__name__`, a builtin type's name. Bound to
                        // a class (`super(A, B)`), the object type is the
                        // class itself.
                        let (c, o) = (self.vslot_o(v, 0), self.vslot_o(v, 1));
                        let is_type = |x| Vm::is_class(x) || self.type_of_typeobj(x).is_some();
                        let own = if is_type(o) && self.type_of_typeobj(c) != Some(T::type_) {
                            self.class_name(o)
                        } else {
                            self.type_name(o)
                        };
                        let _ = write!(
                            writer!(),
                            "<super: <class '{}'>, <{own} object>>",
                            self.class_name(c),
                        );
                    }
                    _ => {
                        let _ = write!(
                            writer!(),
                            "<{} object at 0x{:x}>",
                            self.type_name(v),
                            Vm::addr(v)
                        );
                    }
                }
                continue;
            }
            let t = self.type_of(v);
            let container = matches!(
                t,
                T::list
                    | T::tuple
                    | T::dict
                    | T::set
                    | T::frozenset
                    | T::dict_keys
                    | T::dict_values
                    | T::dict_items
            ) || v.is_obj_kind(ty::EXC as u8);
            if container {
                if depth > MAX_REPR_DEPTH {
                    return Err(self.err(
                        T::RecursionError,
                        "maximum recursion depth exceeded while getting the repr of an object",
                    ));
                }
                if matches!(t, T::set | T::frozenset) && self.set_len(v) == 0 {
                    writer!().push_str(if t == T::set { "set()" } else { "frozenset()" });
                    continue;
                }
                let seen = self.repr_outer_has(outer, v)? || {
                    let res = active.insert(&mut self.heap, v);
                    !self.hx_o(res)?
                };
                if seen {
                    writer!().push_str(match t {
                        T::list => "[...]",
                        T::dict => "{...}",
                        _ => "...",
                    });
                    continue;
                }
            }
            match t {
                T::NoneType => writer!().push_str("None"),
                T::bool => writer!().push_str(if v == Value::TRUE { "True" } else { "False" }),
                T::ellipsis => writer!().push_str("Ellipsis"),
                T::NotImplementedType => writer!().push_str("NotImplemented"),
                T::int => {
                    let _ = write!(writer!(), "{}", self.int_of_o(v).unwrap_or(0));
                }
                T::list
                | T::tuple
                | T::set
                | T::frozenset
                | T::dict
                | T::dict_keys
                | T::dict_values
                | T::dict_items => {
                    let (open, close) = match t {
                        T::list => ("[", "]"),
                        T::tuple => ("(", if self.tuple_len(v) == 1 { ",)" } else { ")" }),
                        T::frozenset => ("frozenset({", "})"),
                        T::set | T::dict => ("{", "}"),
                        _ => {
                            writer!().push_str(t.name());
                            ("([", "])")
                        }
                    };
                    writer!().push_str(open);
                    let res = work.push(&mut self.heap, Work::Leave(v));
                    self.hx_o(res)?;
                    let res = work.push(&mut self.heap, Work::Lit(close));
                    self.hx_o(res)?;
                    if let Some((buf, end)) = self.text_items(v, t) {
                        let res = work.push(&mut self.heap, Work::Items(buf, 0, end, depth + 1, t));
                        self.hx_o(res)?;
                    }
                }
                T::type_ => {
                    let t = self.type_of_typeobj(v).unwrap_or(T::object);
                    match t.module() {
                        "builtins" => _ = write!(writer!(), "<class '{}'>", t.name()),
                        m => _ = write!(writer!(), "<class '{m}.{}'>", t.name()),
                    }
                }
                T::function if v.is_obj_kind(ty::HOSTFN as u8) => {
                    let q = self.vslot_o(v, fn_slot::QUALNAME);
                    let _ = write!(
                        writer!(),
                        "<function {} at 0x{:x}>",
                        self.str_ref_o(q),
                        Vm::addr(v)
                    );
                }
                T::function => {
                    let q = self.vslot_o(v, fn_slot::QUALNAME);
                    let _ = write!(
                        writer!(),
                        "<function {} at 0x{:x}>",
                        self.str_ref_o(q),
                        Vm::addr(v)
                    );
                }
                T::builtin => {
                    if let Some(i) = v.as_native() {
                        let n = crate::builtins::native(i);
                        match n.and_then(|n| n.owner) {
                            // Functions of modules CPython writes in
                            // Python (`asyncio`) are Python functions there.
                            None if n.is_some_and(|n| n.module() == Some("asyncio")) => {
                                let _ = write!(
                                    writer!(),
                                    "<function {} at 0x{:x}>",
                                    n.map_or("?", |n| n.name()),
                                    0x7f00_0000_0000_u64 + (u64::from(i) << 4)
                                );
                            }
                            None => {
                                let _ = write!(
                                    writer!(),
                                    "<built-in function {}>",
                                    n.map_or("?", |n| n.name())
                                );
                            }
                            Some(o) => {
                                let _ = write!(
                                    writer!(),
                                    "<method '{}' of '{}' objects>",
                                    n.map_or("?", |n| n.name()),
                                    o.name()
                                );
                            }
                        }
                    } else {
                        let recv = self.vslot_o(v, 0);
                        let f = self.vslot_o(v, 1);
                        let name = f
                            .as_native()
                            .and_then(crate::builtins::native)
                            .map_or("?", |n| n.name());
                        let _ = write!(
                            writer!(),
                            "<built-in method {name} of {} object at 0x{:x}>",
                            self.type_name(recv),
                            Vm::addr(recv)
                        );
                    }
                }
                T::range => {
                    let r = v.as_obj();
                    let get = |i| r.map_or(0, |r| self.raw_o(r, i).cast_signed());
                    let (a, b, c) = (get(0), get(1), get(2));
                    if c == 1 {
                        let _ = write!(writer!(), "range({a}, {b})");
                    } else {
                        let _ = write!(writer!(), "range({a}, {b}, {c})");
                    }
                }
                T::slice => {
                    writer!().push_str("slice(");
                    let res = work.push(&mut self.heap, Work::Lit(")"));
                    self.hx_o(res)?;
                    for i in (0..3).rev() {
                        let part = self.vslot_o(v, i);
                        let res = work.push(&mut self.heap, Work::Val(part, depth + 1));
                        self.hx_o(res)?;
                        if i > 0 {
                            let res = work.push(&mut self.heap, Work::Lit(", "));
                            self.hx_o(res)?;
                        }
                    }
                }
                _ if v.is_obj_kind(ty::EXC as u8) => {
                    let args = self.vslot_o(v, exc_slot::ARGS);
                    writer!().push_str(self.type_name(v));
                    let res = work.push(&mut self.heap, Work::Leave(v));
                    self.hx_o(res)?;
                    if self.tuple_len(args) == 1 {
                        writer!().push_str("(");
                        let res = work.push(&mut self.heap, Work::Lit(")"));
                        self.hx_o(res)?;
                        let arg = self.vslot_o(args, 0);
                        let res = work.push(&mut self.heap, Work::Val(arg, depth + 1));
                        self.hx_o(res)?;
                    } else {
                        let res = work.push(&mut self.heap, Work::Val(args, depth + 1));
                        self.hx_o(res)?;
                    }
                }
                #[cfg(feature = "re")]
                T::Match => {
                    let (a, b, g) = crate::re::match_repr_parts(self, v)?;
                    let _ = write!(writer!(), "<re.Match object; span=({a}, {b}), match=");
                    let res = work.push(&mut self.heap, Work::Lit(">"));
                    self.hx_o(res)?;
                    let res = work.push(&mut self.heap, Work::Val(g, depth + 1));
                    self.hx_o(res)?;
                }
                T::module => {
                    let _ = write!(writer!(), "<module '{}' (built-in)>", self.module_name(v));
                }
                T::coroutine => {
                    let f = self.vslot_o(v, 0);
                    let q = if v.is_obj_kind(ty::SLEEP as u8) {
                        "sleep"
                    } else {
                        self.str_ref_o(self.vslot_o(f, fn_slot::QUALNAME))
                    };
                    let _ = write!(writer!(), "<coroutine object {q} at 0x{:x}>", Vm::addr(v));
                }
                // A task's or future's repr shows its result through reprlib
                // (the prelude's `_aio_repr`).
                T::Task | T::Future => return Err(self.need_py()),
                T::TextIOWrapper => {
                    let name = if self.stream_of(v) == Some(crate::vm::Stream::Stderr) {
                        "<stderr>"
                    } else {
                        "<stdout>"
                    };
                    let _ = write!(
                        writer!(),
                        "<_io.TextIOWrapper name='{name}' mode='w' encoding='utf-8'>"
                    );
                }
                T::cell => {
                    let inner = self.vslot_o(v, 0);
                    if inner == Value::UNDEFINED {
                        let _ = write!(writer!(), "<cell at 0x{:x}: empty>", Vm::addr(v));
                    } else {
                        let _ = write!(
                            writer!(),
                            "<cell at 0x{:x}: {} object at 0x{:x}>",
                            Vm::addr(v),
                            self.type_name(inner),
                            Vm::addr(inner)
                        );
                    }
                }
                _ => {
                    let _ = write!(writer!(), "<{} object at 0x{:x}>", t.name(), Vm::addr(v));
                }
            }
        }
        // The text costs what the str methods charge for it, one unit per 64
        // bytes, so a cell that doubles a repr parks before the memory
        // limit (a single long string is one traversal step).
        self.burn(units + (out.len().saturating_sub(start) / 64) as u64);
        if let Some(error) = failed {
            return self.hx_o(Err(error));
        }
        Ok(Some(out.len() <= limit))
    }

    // ---------------------------------------------------------- format

    /// `format(v, spec)`, borrowing arena strings through parsing.
    /// `format(v, spec)` as a new str (a long str result a step at a
    /// time, docs/spec.md §5).
    pub(crate) fn format_str(&mut self, v: Value, spec: FormatSpec<'_>) -> Result<Value, Raised> {
        // A str subclass spec (`format(5, S('>3'))`, an f-string's lone
        // nested field) is read as the text it extends.
        let spec = match spec {
            FormatSpec::Value(x) => FormatSpec::Value(self.unbx(x)),
            t @ FormatSpec::Text(_) => t,
        };
        // A str, or a str subclass instance without its own `__format__`.
        let base = self.unbx(v);
        if Vm::is_str(base) && self.uflags(v) & crate::class::f::FORMAT == 0 {
            // A spec that leaves the text as it is gives `v` itself, as
            // CPython's writer does (a subclass instance too): no copy of
            // a long text. An empty one is `str(v)`, a subclass's own.
            let text = match spec {
                FormatSpec::Value(x) => self.str_bytes(x),
                FormatSpec::Text(t) => t.as_bytes(),
            };
            // An empty text is written as nothing: `''`, never a subclass.
            let same = if self.str_bytes(base).is_empty() {
                base
            } else {
                v
            };
            if text == b"s" || (text.is_empty() && v == base) {
                return Ok(same);
            }
            if !text.is_empty()
                && let Some(r) = self.format_long(base, &spec)
            {
                return r.map(|r| if r == base { same } else { r });
            }
        }
        match self.format_in(v, spec, true)? {
            Fmt::Str(s) if s == base && Vm::is_str(base) && !self.str_bytes(base).is_empty() => {
                Ok(v)
            }
            Fmt::Str(s) => Ok(s),
            Fmt::Text(t) => self.new_str(&t),
        }
    }

    /// [`crate::strmeth::format_big`] when the result can be long: a long
    /// str, or a width of 7 digits or more. With no spec, the str itself,
    /// as in CPython: no copy.
    #[inline(never)]
    #[cfg_attr(mm_shipped, optimize(size))]
    fn format_long(&mut self, v: Value, spec: &FormatSpec<'_>) -> Option<Result<Value, Raised>> {
        let text = match *spec {
            FormatSpec::Value(x) => self.str_bytes(x),
            FormatSpec::Text(t) => t.as_bytes(),
        };
        if text.is_empty() {
            return Some(Ok(v));
        }
        if text.len() < 8 && self.str_bytes(v).len() < 1 << 20 {
            return None;
        }
        let text = String::from(core::str::from_utf8(text).unwrap_or(""));
        crate::strmeth::format_big(self, v, &text)
    }

    /// `format(v, spec)` for an int, float or complex `v` whose text can be
    /// long: a big int's power-of-two digits ([`Vm::pow2_text`]), or a
    /// number padded to a width of millions, its text without the padding
    /// as [`Vm::format`] makes it, then padded ([`Vm::num_pad`]).
    #[cfg_attr(mm_shipped, optimize(size))]
    #[inline(never)]
    fn format_num(&mut self, v: Value, spec: FormatSpec<'_>) -> Option<Result<Value, Raised>> {
        use crate::builtins::pw;
        let t = self.type_of(v);
        let int = matches!(t, T::int | T::bool);
        if !(int || matches!(t, T::float | T::complex))
            || self.uflags(v) & crate::class::f::FORMAT != 0
        {
            return None;
        }
        let spec = String::from(match spec {
            FormatSpec::Value(x) => self.str_ref(x),
            FormatSpec::Text(t) => t,
        });
        if spec.contains(",_") || spec.contains("_,") {
            return None;
        }
        let mut s = Spec::parse(&spec)?;
        let bytes = s.width.checked_mul(s.fill.len_utf8())?;
        if !self.heap.would_fit(bytes as u64) {
            return None;
        }
        let (shift, letter) = match s.ty {
            Some('b') => (1, b'b'),
            Some('o') => (3, b'o'),
            Some(c @ ('x' | 'X')) => (4, c as u8),
            _ => (0, 0),
        };
        let bits = v
            .as_obj()
            .filter(|_| Vm::is_heap_int(v))
            .and_then(|r| self.slot_o(r, 0).as_byte_ref())
            .map_or(0, |d| self.heap.bytes(d).map_or(0, <[u8]>::len) * 8);
        if shift > 0
            && s.precision.is_none()
            && !s.zflag
            && s.frac_grouping.is_none()
            && s.grouping != Some(',')
            && bits > POW2_LONG * shift
        {
            let sign = match s.sign {
                Some(c @ ('+' | ' ')) => c as u8,
                _ => 0,
            };
            let conf = shift as u64
                + u64::from(s.ty == Some('X')) * pw::UPPER
                + u64::from(s.grouping.is_some()) * u64::from(b'_') * pw::SEP
                + u64::from(sign) * pw::SIGN
                + u64::from(s.alt) * u64::from(letter) * pw::PREFIX
                + u64::from(s.align.unwrap_or('>') as u8) * pw::ALIGN
                + u64::from(s.fill) * pw::FILL;
            return Some(self.pow2_text(v, conf, s.width as u64));
        }
        if bytes < crate::strmeth::FILL_BYTES {
            return None;
        }
        let mut pad = Spec::parse("")?;
        (pad.fill, pad.align, pad.width, pad.grouping, pad.ty) = (
            s.fill,
            Some(s.align.unwrap_or('>')),
            core::mem::take(&mut s.width),
            s.grouping,
            s.ty,
        );
        let prefix = 2 * usize::from(int && s.alt && shift > 0);
        Some((|| {
            let body = self.format(v, FormatSpec::Text(&Spec::without_width(&spec)))?;
            let text = body.text.as_bytes();
            // The sign, and an int's prefix, before an `=` alignment's fill.
            let sign = pad.ty != Some('c') && matches!(text.first(), Some(b'-' | b'+' | b' '));
            let head = usize::from(sign) + prefix;
            let g = pad.grouping.map_or(0, |g| g as u32);
            let int_len = text
                .get(head..)
                .unwrap_or(&[])
                .iter()
                .take_while(|&&c| {
                    c.is_ascii_digit() || shift > 0 && c.is_ascii_hexdigit() || u32::from(c) == g
                })
                .count();
            let body = self.new_str(&body.text)?;
            self.num_pad(body, head, int_len, &pad)
        })())
    }

    /// Whether `format(v, spec)` of a number can be long enough to go a
    /// step at a time ([`Vm::format_num`]): an int past [`POW2_LONG`] bits,
    /// or a width of [`crate::strmeth::FILL_BYTES`] or more.
    #[inline(never)]
    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn long_number_format(&self, v: Value, spec: &str) -> bool {
        let number = matches!(self.type_of(v), T::int | T::bool | T::float | T::complex);
        let wide = Spec::parse(spec).is_some_and(|s| {
            s.width.saturating_mul(s.fill.len_utf8()) >= crate::strmeth::FILL_BYTES
        });
        number && (wide || Vm::is_heap_int(v) && self.heap_int_bits(v) > POW2_LONG as u64)
    }

    /// Number text `body` padded as `s` says (fill, alignment, width, and
    /// for zero padding its grouping), a step at a time
    /// ([`crate::strmeth::fill_parts`]): `head` bytes of sign and prefix
    /// before an `=` alignment's fill, then `int_len` bytes of integer
    /// digits, which CPython's zero padding extends to the left, grouped
    /// like them.
    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn num_pad(
        &mut self,
        body: Value,
        head: usize,
        int_len: usize,
        s: &Spec,
    ) -> Result<Value, Raised> {
        let len = self.str_bytes(body).len();
        let chars = self.str_len(body);
        let mut fill = s.width.saturating_sub(chars);
        let fl = s.fill.len_utf8();
        let f = self.new_str(s.fill.encode_utf8(&mut [0; 4]))?;
        let mut zeros = (f, 0);
        if let Some(g) = s.grouping
            && fill > 0
            && int_len > 0
            && s.fill == '0'
            && s.align == Some('=')
        {
            // The width's zeros as CPython's `group` adds them: `n` digits
            // grouped are `total` chars, a separator never first.
            let every = if matches!(s.ty, Some('b' | 'o' | 'x' | 'X')) {
                4
            } else {
                3
            };
            let d = int_len - int_len / (every + 1);
            let min = s.width - (chars - int_len);
            let n = d.max(min - min.saturating_sub(1) / (every + 1));
            let total = n + (n - 1) / every;
            let p = every + 1;
            let pat: String = (0..p)
                .map(|j| {
                    if (total + p - j).is_multiple_of(p) {
                        g
                    } else {
                        '0'
                    }
                })
                .collect();
            zeros = (self.new_str(&pat)?, total - int_len);
            fill = 0;
        }
        let (l, m, r) = match s.align {
            Some('=') => (0, fill, 0),
            Some('^') => (fill / 2, 0, fill - fill / 2),
            Some('<') => (0, 0, fill),
            _ => (fill, 0, 0),
        };
        if fill == 0 && zeros.1 == 0 {
            return Ok(body);
        }
        crate::strmeth::fill_parts(
            self,
            &[
                (f, 0, l * fl),
                (body, 0, head),
                (zeros.0, 0, zeros.1),
                (f, 0, m * fl),
                (body, head, len),
                (f, 0, r * fl),
            ],
        )
    }

    /// The ValueError for code point `c`, a surrogate, which a `str` (UTF-8)
    /// cannot hold, asked of `what` (CPython makes a lone surrogate).
    #[cold]
    pub(crate) fn surrogate(&mut self, what: &str, c: i64) -> Raised {
        let msg = format!(
            "{what} arg is a surrogate ({c:#x}), which micro-monty strings cannot hold: they are UTF-8"
        );
        self.err(T::ValueError, &msg)
    }

    pub(crate) fn format(&mut self, v: Value, spec: FormatSpec<'_>) -> Result<NativeText, Raised> {
        match self.format_in(v, spec, false)? {
            Fmt::Text(t) => Ok(t),
            Fmt::Str(s) => {
                let mut t = NativeText::default();
                t.push_value(self, s)?;
                Ok(t)
            }
        }
    }

    /// `format(v, spec)`; with `steps`, a long number's text a step at a
    /// time ([`Vm::format_num`]) and a str under an empty spec itself, as
    /// CPython's, both as a str.
    fn format_in(&mut self, v: Value, spec: FormatSpec<'_>, steps: bool) -> Result<Fmt, Raised> {
        let fspec = spec;
        let t = self.type_of(v);
        if self.uflags(v) & crate::class::f::FORMAT != 0 {
            return Err(self.need_py());
        }
        let text = match spec {
            FormatSpec::Value(v) => self.str_ref(v),
            FormatSpec::Text(text) => text,
        };
        if text.is_empty() {
            if steps && Vm::is_str(v) {
                return Ok(Fmt::Str(v));
            }
            return self.stringify(v).map(Fmt::Text);
        }
        if let Some(b) = self.backing(v) {
            return self.format_in(b, spec, steps);
        }
        let spec = text;
        if spec.contains(",_") || spec.contains("_,") {
            return Err(self.err(T::ValueError, "Cannot specify both ',' and '_'."));
        }
        let Some(mut s) = Spec::parse(spec) else {
            if Spec::missing_precision(spec) {
                return Err(self.err(T::ValueError, "Format specifier missing precision"));
            }
            let msg = format!(
                "Invalid format specifier '{spec}' for object of type '{}'",
                t.name()
            );
            return Err(self.err(T::ValueError, &msg));
        };
        // Bound requested padding before zero-filled digit groups or native
        // strings are built. Width is in characters; fill can be multibyte.
        if s.width
            .checked_mul(s.fill.len_utf8())
            .is_none_or(|bytes| !self.heap.would_fit(bytes as u64))
        {
            return Err(self.err(T::MemoryError, ""));
        }
        if steps
            && (Vm::is_heap_int(v)
                || s.width.saturating_mul(s.fill.len_utf8()) >= crate::strmeth::FILL_BYTES)
            && let Some(r) = self.format_num(v, fspec)
        {
            return r.map(Fmt::Str);
        }
        // A str the spec leaves as it is: no copy (`Fmt::Str` of it).
        let mut same = false;
        // The text, in a closure so its returns are native text.
        let text = (|| -> Result<NativeText, Raised> {
            // A fraction separator ('.6_n') is refused only with 'n'.
            let grouping = s.grouping.or(s.frac_grouping.filter(|_| s.ty == Some('n')));
            if let (Some(g), Some(c)) = (grouping, s.ty)
                && matches!(t, T::int | T::bool | T::float | T::complex)
                && !matches!(c, 'd' | 'e' | 'f' | 'g' | 'E' | 'G' | '%' | 'F')
                && (g == ',' || !matches!(c, 'b' | 'o' | 'x' | 'X'))
            {
                return Err(self.err(T::ValueError, &format!("Cannot specify '{g}' with '{c}'.")));
            }
            match t {
                T::str => {
                    // A bare `0` before the width left-aligns a string with '0's.
                    let zeros = s.zero && s.fill == '0' && s.align == Some('=');
                    // CPython's order: grouping, type, sign, z, #, `=`.
                    let c = s.ty.unwrap_or('s');
                    if let Some(g) = s.grouping
                        && !matches!(c, 'd' | 'e' | 'f' | 'g' | 'E' | 'G' | '%' | 'F')
                        && (g == ',' || !matches!(c, 'b' | 'o' | 'x' | 'X'))
                    {
                        return Err(
                            self.err(T::ValueError, &format!("Cannot specify '{g}' with '{c}'."))
                        );
                    }
                    let msg = if c != 's' {
                        return Err(self.bad_code(s.ty, "str"));
                    } else if s.sign == Some(' ') {
                        "Space not allowed in string format specifier"
                    } else if s.sign.is_some() {
                        "Sign not allowed in string format specifier"
                    } else if s.zflag {
                        "Negative zero coercion (z) not allowed in string format specifier"
                    } else if s.alt {
                        "Alternate form (#) not allowed in string format specifier"
                    } else if s.align == Some('=') && !zeros {
                        "'=' alignment not allowed in string format specifier"
                    } else {
                        ""
                    };
                    if !msg.is_empty() {
                        return Err(self.err(T::ValueError, msg));
                    }
                    // Arena strings are UTF-8. Locate the requested prefix in raw
                    // storage, then validate only that prefix once after admission.
                    let text = self.str_bytes(v);
                    let end = s.precision.map_or(text.len(), |p| {
                        text.iter()
                            .enumerate()
                            .filter(|&(_, &b)| b & 0xc0 != 0x80)
                            .nth(p)
                            .map_or(text.len(), |(i, _)| i)
                    });
                    if end == text.len() && self.str_len(v) >= s.width {
                        same = true;
                        return Ok(NativeText::default());
                    }
                    let mut text = NativeText::default();
                    text.reserve(self, end)?;
                    text.text
                        .push_str(core::str::from_utf8(&self.str_bytes(v)[..end]).unwrap_or(""));
                    if zeros {
                        s.align = Some('<');
                    }
                    if char_len(&text) >= s.width {
                        return Ok(text);
                    }
                    s.pad(self, &text, '<')
                }
                T::int | T::bool => {
                    // A small int in a plain integer presentation: digits on
                    // the stack, no bigint.
                    if let Some(n) = v.as_int()
                        && s.precision.is_none()
                        && s.grouping.is_none()
                        && !s.zflag
                        && let Some((radix, upper, prefix)) = int_radix(s.ty)
                    {
                        let mut buf = [0; 64];
                        let digits = small_digits(n.unsigned_abs(), radix, upper, &mut buf);
                        let prefix = if s.alt { prefix } else { "" };
                        return s.pad_number(self, sign_str(n < 0, s.sign), prefix, digits, '>');
                    }
                    let i = self.big_of(v).unwrap_or_default();
                    match s.ty {
                        Some('e' | 'E' | 'f' | 'F' | 'g' | 'G' | '%') => {
                            let f = self.big_to_f64(&i)?;
                            self.format_float(f, &s)
                        }
                        _ if s.zflag && s.precision.is_none() => Err(self.err(
                            T::ValueError,
                            "Negative zero coercion (z) not allowed in integer format specifier",
                        )),
                        _ => self.format_int(&i, &s),
                    }
                }
                T::float => {
                    let f = v.as_float().unwrap_or(0.0);
                    self.format_float(f, &s)
                }
                T::complex => {
                    let (re, im) = self.complex_of(v).unwrap_or((0.0, 0.0));
                    self.format_complex(re, im, &s)
                }
                _ => {
                    let msg = format!(
                        "unsupported format string passed to {}.__format__",
                        self.type_name(v)
                    );
                    Err(self.err(T::TypeError, &msg))
                }
            }
        })()?;
        if same {
            return Ok(Fmt::Str(v));
        }
        Ok(Fmt::Text(text))
    }

    fn bad_code(&mut self, c: Option<char>, tname: &str) -> Raised {
        self.err(
            T::ValueError,
            &format!(
                "Unknown format code '{}' for object of type '{tname}'",
                c.unwrap_or(' ')
            ),
        )
    }

    /// The digits of `i` in power-of-two `radix`, lowercase unless `upper`,
    /// with a charge for them and the `copies - 1` copies the caller makes
    /// before its result is admitted: an int of n bytes has up to 8n binary
    /// digits, eight times the arena it came from.
    pub(crate) fn pow2_digits(
        &mut self,
        i: &crate::bigint::Big,
        radix: u32,
        upper: bool,
        copies: usize,
    ) -> Result<(String, Scratch), Raised> {
        let bits = u64::from(radix.trailing_zeros()).max(1);
        let n = usize::try_from(i.bit_length().div_ceil(bits).max(1)).unwrap_or(usize::MAX);
        let res = self.heap.reserve_scratch(n.saturating_mul(copies));
        let charge = self.hx_o(res)?;
        let mut d = i.digits(radix);
        if upper {
            d.make_ascii_uppercase();
        }
        Ok((d, charge))
    }

    fn format_int(&mut self, i: &crate::bigint::Big, s: &Spec) -> Result<NativeText, Raised> {
        if s.precision.is_some() {
            return Err(self.err(
                T::ValueError,
                "Precision not allowed in integer format specifier",
            ));
        }
        // Grouping copies the digits once more, a third larger.
        let copies = if s.grouping.is_some() { 3 } else { 1 };
        let (mut digits, prefix, _native) = match s.ty {
            None | Some('d' | 'n') => {
                let d = self.big_decimal(&i.abs())?;
                (d, "", None)
            }
            Some(c @ ('b' | 'o' | 'x' | 'X')) => {
                let (radix, prefix) = match c {
                    'b' => (2, "0b"),
                    'o' => (8, "0o"),
                    'x' => (16, "0x"),
                    _ => (16, "0X"),
                };
                let (d, charge) = self.pow2_digits(i, radix, c == 'X', copies)?;
                (d, prefix, Some(charge))
            }
            Some('c') => {
                if s.sign.is_some() {
                    return Err(self.err(
                        T::ValueError,
                        "Sign not allowed with integer format specifier 'c'",
                    ));
                }
                if s.alt {
                    return Err(self.err(
                        T::ValueError,
                        "Alternate form (#) not allowed with integer format specifier 'c'",
                    ));
                }
                let n = i.to_i64();
                let Some(c) = n
                    .and_then(|n| u32::try_from(n).ok())
                    .and_then(char::from_u32)
                else {
                    if let Some(n) = n.filter(|n| (0xD800..=0xDFFF).contains(n)) {
                        return Err(self.surrogate("%c", n));
                    }
                    let msg = if n.is_none() {
                        "Python int too large to convert to C long"
                    } else {
                        "%c arg not in range(0x110000)"
                    };
                    return Err(self.err(T::OverflowError, msg));
                };
                // A number: right-aligned by default, as CPython's 'c' is.
                return s.pad(self, c.encode_utf8(&mut [0; 4]), '>');
            }
            c => return Err(self.bad_code(c, "int")),
        };
        let sign = sign_str(i.neg, s.sign);
        let prefix = if s.alt { prefix } else { "" };
        if let Some(g) = s.grouping {
            let every = if matches!(s.ty, Some('b' | 'o' | 'x' | 'X')) {
                4
            } else {
                3
            };
            let min = s.zero_pad_width(sign.len() + prefix.len());
            digits = group(&digits, g, every, min);
        }
        s.pad_number(self, sign, prefix, &digits, '>')
    }

    pub(crate) fn format_float(&mut self, f: f64, s: &Spec) -> Result<NativeText, Raised> {
        let p = s.precision.unwrap_or(0);
        if p > i32::MAX as usize {
            return Err(self.err(T::ValueError, "precision too big"));
        }
        let pads = f.is_finite() && (matches!(s.ty, Some('f' | 'F' | '%' | 'e' | 'E')) || s.alt);
        // Held while the text is built: up to about four precision-sized
        // copies live at once (the digits, case or exponent rewrites,
        // fraction grouping) before the result is admitted.
        let _native = if pads && p > 1100 {
            let res = self.heap.reserve_scratch(p.saturating_mul(4));
            Some(self.hx_o(res)?)
        } else {
            None
        };
        let a = f.abs();
        let mut body = if a.is_nan() || a.is_infinite() {
            let t = if a.is_nan() { "nan" } else { "inf" };
            match s.ty {
                Some('E' | 'F' | 'G') => t.to_uppercase(),
                Some('%') => format!("{t}%"),
                None | Some('e' | 'f' | 'g' | 'n') => t.to_string(),
                c => return Err(self.bad_code(c, "float")),
            }
        } else {
            match s.ty {
                Some('f' | 'F') => fixed(a, s.precision.unwrap_or(6)),
                Some('%') => {
                    let mut t = fixed(a * 100.0, s.precision.unwrap_or(6));
                    t.push('%');
                    t
                }
                Some('e' | 'E') => {
                    let t = exp_form(a, s.precision.unwrap_or(6));
                    if s.ty == Some('E') {
                        t.to_uppercase()
                    } else {
                        t
                    }
                }
                Some('g' | 'G') => {
                    let t = general(a, s.precision.unwrap_or(6), s.alt, false);
                    if s.ty == Some('G') {
                        t.to_uppercase()
                    } else {
                        t
                    }
                }
                None => match s.precision {
                    None => float_repr(a),
                    Some(p) => {
                        // 'g' that keeps a digit past the point, so it
                        // switches to exponent form one digit earlier.
                        let mut t = general(a, p, s.alt, true);
                        if !t.contains('.') && !t.contains('e') && !t.contains("inf") {
                            t.push_str(".0");
                        }
                        t
                    }
                },
                Some('n') => general(a, s.precision.unwrap_or(6), s.alt, false),
                c => return Err(self.bad_code(c, "float")),
            }
        };
        // Percent scaling can overflow even when the input is finite.
        let finite = body.starts_with(|c: char| c.is_ascii_digit());
        if s.alt && finite && !body.contains('.') {
            if let Some(e) = body.find(['e', 'E', '%']) {
                body.insert(e, '.');
            } else {
                body.push('.');
            }
        }
        s.group_fraction(&mut body);
        // `z`: a value that rounds to zero loses its minus sign.
        let zero = finite && body.bytes().all(|c| !c.is_ascii_digit() || c == b'0');
        let neg = f.is_sign_negative() && !f.is_nan() && !(s.zflag && zero);
        let sign = sign_str(neg, s.sign);
        if let Some(g) = s.grouping
            && finite
        {
            let (int_part, rest) = match body.find(|c: char| !c.is_ascii_digit()) {
                Some(k) => body.split_at(k),
                None => (body.as_str(), ""),
            };
            let min = s.zero_pad_width(sign.len() + rest.len());
            body = format!("{}{rest}", group(int_part, g, 3, min));
        }
        s.pad_number(self, sign, "", &body, '>')
    }
}

/// The radix, case and `#` prefix of an integer presentation type with
/// digits (`d`, `n`, `b`, `o`, `x`, `X` or none).
pub(crate) fn int_radix(ty: Option<char>) -> Option<(u64, bool, &'static str)> {
    Some(match ty {
        None | Some('d' | 'n') => (10, false, ""),
        Some('b') => (2, false, "0b"),
        Some('o') => (8, false, "0o"),
        Some('x') => (16, false, "0x"),
        Some('X') => (16, true, "0X"),
        _ => return None,
    })
}

/// The digits of `n` in `radix` (2 to 16), written at the end of `buf`.
pub(crate) fn small_digits(mut n: u64, radix: u64, upper: bool, buf: &mut [u8; 64]) -> &str {
    let set = if upper {
        b"0123456789ABCDEF"
    } else {
        b"0123456789abcdef"
    };
    let mut i = buf.len();
    loop {
        i -= 1;
        buf[i] = set[(n % radix) as usize];
        n /= radix;
        if n == 0 {
            break;
        }
    }
    core::str::from_utf8(&buf[i..]).unwrap_or("")
}

fn sign_str(neg: bool, sign: Option<char>) -> &'static str {
    if neg {
        "-"
    } else {
        match sign {
            Some('+') => "+",
            Some(' ') => " ",
            _ => "",
        }
    }
}

/// ASCII `digits` with `sep` every `every` digits from the right. With
/// `min`, CPython's zero padding first: leading zeros, grouped like the
/// digits, until the text is at least `min` long (a separator never
/// leads, so it may end one longer).
#[inline(never)]
pub(crate) fn group(digits: &str, sep: char, every: usize, min: usize) -> String {
    let n = digits.len().max(min - min.saturating_sub(1) / (every + 1));
    let mut out = String::with_capacity(n + n / every);
    let bytes = digits.as_bytes();
    for i in 0..n {
        if i > 0 && (n - i).is_multiple_of(every) {
            out.push(sep);
        }
        let k = (i + digits.len()).checked_sub(n);
        out.push(k.map_or('0', |k| char::from(bytes[k])));
    }
    out
}

/// `{:.p}` of a double. Rust's formatter takes at most 65535 digits; past
/// 1100 fractional digits a double's expansion is all zeros.
pub(crate) fn fixed(a: f64, p: usize) -> String {
    let q = p.min(1100);
    let mut t = format!("{a:.q$}");
    t.extend(core::iter::repeat_n('0', p - q));
    t
}

/// `{:.pe}` (Rust's layout): past 800 digits a double's mantissa is all
/// zeros.
fn sci(a: f64, p: usize) -> String {
    let q = p.min(800);
    let t = format!("{a:.q$e}");
    match t.split_once('e') {
        Some((m, e)) if q < p => format!("{m}{}e{e}", "0".repeat(p - q)),
        _ => t,
    }
}

/// `{:.pe}` laid out the Python way (`1.5e+03`).
pub(crate) fn exp_form(a: f64, p: usize) -> String {
    let t = sci(a, p);
    let (m, e) = t.split_once('e').unwrap_or((&t, "0"));
    let e: i32 = e.parse().unwrap_or(0);
    format!("{m}e{}{:02}", if e < 0 { '-' } else { '+' }, e.abs())
}

/// The `g` presentation.
/// With `dot0`, CPython's `Py_DTSF_ADD_DOT_0` (a float's format with a
/// precision and no type): exponent form from exponent `p - 1` on.
pub(crate) fn general(a: f64, p: usize, alt: bool, dot0: bool) -> String {
    // Without `#` the zeros past a double's digits are stripped anyway.
    let p = if alt { p.max(1) } else { p.clamp(1, 800) };
    if a == 0.0 {
        return match (dot0 && p == 1, alt) {
            (true, false) => "0e+00".into(),
            (true, true) => "0.e+00".into(),
            (false, true) => fixed(0.0, p - 1),
            (false, false) => "0".into(),
        };
    }
    let t = sci(a, p - 1);
    let exp: i32 = t.split_once('e').map_or(0, |(_, e)| e.parse().unwrap_or(0));
    let pi = i32::try_from(p).unwrap_or(i32::MAX);
    let mut out = if -4 <= exp && exp < pi - i32::from(dot0) {
        fixed(a, usize::try_from(pi - 1 - exp).unwrap_or(0))
    } else {
        exp_form(a, p - 1)
    };
    if !alt {
        // Strip trailing zeros of the fraction.
        let (m, e) = match out.find('e') {
            Some(k) => (out[..k].to_string(), out[k..].to_string()),
            None => (out.clone(), String::new()),
        };
        let m = if m.contains('.') {
            m.trim_end_matches('0').trim_end_matches('.').to_string()
        } else {
            m
        };
        out = m + &e;
    }
    out
}

/// A spec already in the arena, or a field slice owned by `str.format`.
#[derive(Clone, Copy)]
pub(crate) enum FormatSpec<'a> {
    Value(Value),
    Text(&'a str),
}

/// A width or precision parsed as `u64`, as on a 64-bit target, then
/// saturated: on wasm32 one past `usize::MAX` must still be too big to
/// allocate, not a parse error.
fn sat(n: u64) -> usize {
    usize::try_from(n).unwrap_or(usize::MAX)
}

/// A parsed format spec.
pub(crate) struct Spec {
    pub fill: char,
    pub align: Option<char>,
    pub sign: Option<char>,
    pub zflag: bool,
    pub alt: bool,
    pub zero: bool,
    pub width: usize,
    pub grouping: Option<char>,
    pub precision: Option<usize>,
    /// Grouping of the fraction digits (`.2_f`).
    pub frac_grouping: Option<char>,
    pub ty: Option<char>,
}

impl Spec {
    /// Parses `[[fill]align][sign][z][#][0][width][grouping][.precision][type]`.
    pub(crate) fn parse(mut spec: &str) -> Option<Spec> {
        let mut s = Spec {
            fill: ' ',
            align: None,
            sign: None,
            zflag: false,
            alt: false,
            zero: false,
            width: 0,
            grouping: None,
            precision: None,
            frac_grouping: None,
            ty: None,
        };
        let is_align = |c: char| matches!(c, '<' | '>' | '^' | '=');
        // Only fill and type can be non-ASCII. Keep the remaining input
        // borrowed; even a long digit run needs no character or string copy.
        let mut prefix = spec.chars();
        let mut fill_given = false;
        if let Some(first) = prefix.next() {
            let rest = prefix.as_str();
            if let Some(&align) = rest.as_bytes().first()
                && matches!(align, b'<' | b'>' | b'^' | b'=')
            {
                fill_given = true;
                s.fill = first;
                s.align = Some(char::from(align));
                spec = &rest[1..];
            } else if is_align(first) {
                s.align = Some(first);
                spec = rest;
            }
        }
        if let Some(&sign) = spec.as_bytes().first()
            && matches!(sign, b'+' | b'-' | b' ')
        {
            s.sign = Some(char::from(sign));
            spec = &spec[1..];
        }
        if let Some(rest) = spec.strip_prefix('z') {
            s.zflag = true;
            spec = rest;
        }
        if let Some(rest) = spec.strip_prefix('#') {
            s.alt = true;
            spec = rest;
        }
        if let Some(rest) = spec.strip_prefix('0') {
            s.zero = true;
            spec = rest;
        }
        let digits = spec.bytes().take_while(u8::is_ascii_digit).count();
        if digits != 0 {
            s.width = sat(spec[..digits].parse().ok()?);
            spec = &spec[digits..];
        }
        if let Some(&group) = spec.as_bytes().first()
            && matches!(group, b',' | b'_')
        {
            s.grouping = Some(char::from(group));
            spec = &spec[1..];
        }
        if let Some(rest) = spec.strip_prefix('.') {
            let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
            spec = &rest[digits..];
            if digits != 0 {
                s.precision = Some(sat(rest[..digits].parse().ok()?));
            } else if !spec.starts_with([',', '_']) {
                return None;
            }
            if let Some(&group) = spec.as_bytes().first()
                && matches!(group, b',' | b'_')
            {
                s.frac_grouping = Some(char::from(group));
                spec = &spec[1..];
            }
        }
        s.ty = spec.chars().next();
        if spec.len() != s.ty.map_or(0, char::len_utf8) {
            return None;
        }
        // `0` pads with zeros unless a fill is given, also after an
        // explicit alignment (`<08s` is `ab000000`).
        // `zero` stays set only for that implicit `=` (which strings take).
        if s.zero && !fill_given {
            s.fill = '0';
            if s.align.is_none() {
                s.align = Some('=');
            } else {
                s.zero = false;
            }
        }
        Some(s)
    }

    /// `spec` without its width's digits, parsing as `spec` with width 0.
    pub(crate) fn without_width(spec: &str) -> String {
        let b = spec.as_bytes();
        let align = |c: u8| matches!(c, b'<' | b'>' | b'^' | b'=');
        // `[[fill]align]`, then each flag (sign, `z`, `#`, `0`) once.
        let first = spec.chars().next().map_or(0, char::len_utf8);
        let mut at = if b.get(first).copied().is_some_and(align) {
            first + 1
        } else {
            usize::from(b.first().copied().is_some_and(align))
        };
        for flags in [&b"+- "[..], b"z", b"#", b"0"] {
            at += usize::from(b.get(at).is_some_and(|c| flags.contains(c)));
        }
        let end = at + b[at..].iter().take_while(|c| c.is_ascii_digit()).count();
        format!("{}{}", &spec[..at], &spec[end..])
    }

    /// Groups the fraction digits of `body` (`.2_f`) when asked to.
    pub(crate) fn group_fraction(&self, body: &mut String) {
        let Some(g) = self.frac_grouping else { return };
        let Some(dot) = body.find('.') else { return };
        let end = body[dot + 1..]
            .find(|c: char| !c.is_ascii_digit())
            .map_or(body.len(), |k| dot + 1 + k);
        let mut out = String::from(&body[..=dot]);
        for (i, c) in body[dot + 1..end].chars().enumerate() {
            if i > 0 && i % 3 == 0 {
                out.push(g);
            }
            out.push(c);
        }
        out.push_str(&body[end..]);
        *body = out;
    }

    /// Does `spec` fail only for a `.` without precision digits?
    pub(crate) fn missing_precision(spec: &str) -> bool {
        spec.find('.').is_some_and(|k| {
            !spec[k + 1..].starts_with(|c: char| c.is_ascii_digit() || c == ',' || c == '_')
        })
    }

    /// With zero padding (`0` fill, `=` alignment) the width the grouped
    /// digits must fill, besides `other` characters; else 0.
    fn zero_pad_width(&self, other: usize) -> usize {
        if self.fill == '0' && self.align == Some('=') {
            self.width.saturating_sub(other)
        } else {
            0
        }
    }

    /// Pads `text` to the width; `default` alignment when none is given.
    pub(crate) fn pad(&self, vm: &mut Vm, text: &str, default: char) -> Result<NativeText, Raised> {
        self.pad_number(vm, "", "", text, default)
    }

    fn pad_number(
        &self,
        vm: &mut Vm,
        sign: &str,
        prefix: &str,
        digits: &str,
        default: char,
    ) -> Result<NativeText, Raised> {
        let body_len = sign.len() + prefix.len() + char_len(digits);
        let fill = self.width.saturating_sub(body_len);
        let bytes = fill
            .checked_mul(self.fill.len_utf8())
            .and_then(|n| n.checked_add(sign.len()))
            .and_then(|n| n.checked_add(prefix.len()))
            .and_then(|n| n.checked_add(digits.len()))
            .ok_or_else(|| vm.err(T::MemoryError, ""))?;
        let mut out = NativeText::default();
        out.reserve(vm, bytes)?;
        let (left, middle, right) = match self.align.unwrap_or(default) {
            '>' => (fill, 0, 0),
            '=' => (0, fill, 0),
            '^' => (fill / 2, 0, fill - fill / 2),
            _ => (0, 0, fill),
        };
        // Capacity is admitted above. Doubling a complete UTF-8 character
        // run avoids a per-character append on large padding widths.
        let append_fill = |out: &mut String, count: usize| {
            if count == 0 {
                return;
            }
            let start = out.len();
            let bytes = count * self.fill.len_utf8();
            out.push(self.fill);
            while out.len() - start < bytes {
                let written = out.len() - start;
                out.extend_from_within(start..start + written.min(bytes - written));
            }
        };
        append_fill(&mut out.text, left);
        out.text.push_str(sign);
        out.text.push_str(prefix);
        append_fill(&mut out.text, middle);
        out.text.push_str(digits);
        append_fill(&mut out.text, right);
        Ok(out)
    }
}

/// The largest character boundary of `s` at or below `at`.
fn floor_boundary(s: &str, at: usize) -> usize {
    let mut end = at.min(s.len());
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    end
}

/// The first code point of each Unicode block of decimal digits (`Nd`),
/// Unicode 16.
const ND_ZEROS: &[u32] = &[
    0x30, 0x660, 0x6F0, 0x7C0, 0x966, 0x9E6, 0xA66, 0xAE6, 0xB66, 0xBE6, 0xC66, 0xCE6, 0xD66,
    0xDE6, 0xE50, 0xED0, 0xF20, 0x1040, 0x1090, 0x17E0, 0x1810, 0x1946, 0x19D0, 0x1A80, 0x1A90,
    0x1B50, 0x1BB0, 0x1C40, 0x1C50, 0xA620, 0xA8D0, 0xA900, 0xA9D0, 0xA9F0, 0xAA50, 0xABF0, 0xFF10,
    0x104A0, 0x10D30, 0x10D40, 0x11066, 0x110F0, 0x11136, 0x111D0, 0x112F0, 0x11450, 0x114D0,
    0x11650, 0x116C0, 0x116D0, 0x116DA, 0x11730, 0x118E0, 0x11950, 0x11BF0, 0x11C50, 0x11D50,
    0x11DA0, 0x11F50, 0x16130, 0x16A60, 0x16AC0, 0x16B50, 0x16D70, 0x1CCF0, 0x1D7CE, 0x1D7D8,
    0x1D7E2, 0x1D7EC, 0x1D7F6, 0x1E140, 0x1E2F0, 0x1E4F0, 0x1E5F1, 0x1E950, 0x1FBF0,
];

/// Is `c` a Unicode decimal digit (`str.isdecimal`)?
pub(crate) fn is_decimal(c: u32) -> bool {
    if c < 0x80 {
        return (0x30..=0x39).contains(&c);
    }
    let i = ND_ZEROS.partition_point(|&z| z <= c);
    i > 0 && c - ND_ZEROS[i - 1] < 10
}

/// The value of a Unicode decimal digit (`unicodedata.decimal`).
pub(crate) fn decimal_value(c: char) -> Option<u32> {
    let c = c as u32;
    if !is_decimal(c) {
        return None;
    }
    let i = ND_ZEROS.partition_point(|&z| z <= c);
    Some(c - ND_ZEROS[i.saturating_sub(1)])
}

/// Digits that are not decimal digits (`str.isdigit`), Unicode 16 ranges.
const DIGIT_ONLY: &[(u32, u32)] = &[
    (0xB2, 0xB3),
    (0xB9, 0xB9),
    (0x1369, 0x1371),
    (0x19DA, 0x19DA),
    (0x2070, 0x2070),
    (0x2074, 0x2079),
    (0x2080, 0x2089),
    (0x2460, 0x2468),
    (0x2474, 0x247C),
    (0x2488, 0x2490),
    (0x24EA, 0x24EA),
    (0x24F5, 0x24FD),
    (0x24FF, 0x24FF),
    (0x2776, 0x277E),
    (0x2780, 0x2788),
    (0x278A, 0x2792),
    (0x10A40, 0x10A43),
    (0x10E60, 0x10E68),
    (0x11052, 0x1105A),
    (0x1F100, 0x1F10A),
];

/// Numeric characters that are not digits (`str.isnumeric`), Unicode 16 ranges.
const NUMERIC_ONLY: &[(u32, u32)] = &[
    (0xBC, 0xBE),
    (0x9F4, 0x9F9),
    (0xB72, 0xB77),
    (0xBF0, 0xBF2),
    (0xC78, 0xC7E),
    (0xD58, 0xD5E),
    (0xD70, 0xD78),
    (0xF2A, 0xF33),
    (0x1372, 0x137C),
    (0x16EE, 0x16F0),
    (0x17F0, 0x17F9),
    (0x2150, 0x2182),
    (0x2185, 0x2189),
    (0x2469, 0x2473),
    (0x247D, 0x2487),
    (0x2491, 0x249B),
    (0x24EB, 0x24F4),
    (0x24FE, 0x24FE),
    (0x277F, 0x277F),
    (0x2789, 0x2789),
    (0x2793, 0x2793),
    (0x2CFD, 0x2CFD),
    (0x3007, 0x3007),
    (0x3021, 0x3029),
    (0x3038, 0x303A),
    (0x3192, 0x3195),
    (0x3220, 0x3229),
    (0x3248, 0x324F),
    (0x3251, 0x325F),
    (0x3280, 0x3289),
    (0x32B1, 0x32BF),
    (0x3405, 0x3405),
    (0x3483, 0x3483),
    (0x382A, 0x382A),
    (0x3B4D, 0x3B4D),
    (0x4E00, 0x4E00),
    (0x4E03, 0x4E03),
    (0x4E07, 0x4E07),
    (0x4E09, 0x4E09),
    (0x4E24, 0x4E24),
    (0x4E5D, 0x4E5D),
    (0x4E8C, 0x4E8C),
    (0x4E94, 0x4E94),
    (0x4E96, 0x4E96),
    (0x4EAC, 0x4EAC),
    (0x4EBF, 0x4EC0),
    (0x4EDF, 0x4EDF),
    (0x4EE8, 0x4EE8),
    (0x4F0D, 0x4F0D),
    (0x4F70, 0x4F70),
    (0x4FE9, 0x4FE9),
    (0x5006, 0x5006),
    (0x5104, 0x5104),
    (0x5146, 0x5146),
    (0x5169, 0x5169),
    (0x516B, 0x516B),
    (0x516D, 0x516D),
    (0x5341, 0x5341),
    (0x5343, 0x5345),
    (0x534C, 0x534C),
    (0x53C1, 0x53C4),
    (0x56DB, 0x56DB),
    (0x58F1, 0x58F1),
    (0x58F9, 0x58F9),
    (0x5E7A, 0x5E7A),
    (0x5EFE, 0x5EFF),
    (0x5F0C, 0x5F0E),
    (0x5F10, 0x5F10),
    (0x62D0, 0x62D0),
    (0x62FE, 0x62FE),
    (0x634C, 0x634C),
    (0x67D2, 0x67D2),
    (0x6D1E, 0x6D1E),
    (0x6F06, 0x6F06),
    (0x7396, 0x7396),
    (0x767E, 0x767E),
    (0x7695, 0x7695),
    (0x79ED, 0x79ED),
    (0x8086, 0x8086),
    (0x842C, 0x842C),
    (0x8CAE, 0x8CAE),
    (0x8CB3, 0x8CB3),
    (0x8D30, 0x8D30),
    (0x920E, 0x920E),
    (0x94A9, 0x94A9),
    (0x9621, 0x9621),
    (0x9646, 0x9646),
    (0x964C, 0x964C),
    (0x9678, 0x9678),
    (0x96F6, 0x96F6),
    (0xA6E6, 0xA6EF),
    (0xA830, 0xA835),
    (0xF96B, 0xF96B),
    (0xF973, 0xF973),
    (0xF978, 0xF978),
    (0xF9B2, 0xF9B2),
    (0xF9D1, 0xF9D1),
    (0xF9D3, 0xF9D3),
    (0xF9FD, 0xF9FD),
    (0x10107, 0x10133),
    (0x10140, 0x10178),
    (0x1018A, 0x1018B),
    (0x102E1, 0x102FB),
    (0x10320, 0x10323),
    (0x10341, 0x10341),
    (0x1034A, 0x1034A),
    (0x103D1, 0x103D5),
    (0x10858, 0x1085F),
    (0x10879, 0x1087F),
    (0x108A7, 0x108AF),
    (0x108FB, 0x108FF),
    (0x10916, 0x1091B),
    (0x109BC, 0x109BD),
    (0x109C0, 0x109CF),
    (0x109D2, 0x109FF),
    (0x10A44, 0x10A48),
    (0x10A7D, 0x10A7E),
    (0x10A9D, 0x10A9F),
    (0x10AEB, 0x10AEF),
    (0x10B58, 0x10B5F),
    (0x10B78, 0x10B7F),
    (0x10BA9, 0x10BAF),
    (0x10CFA, 0x10CFF),
    (0x10E69, 0x10E7E),
    (0x10F1D, 0x10F26),
    (0x10F51, 0x10F54),
    (0x10FC5, 0x10FCB),
    (0x1105B, 0x11065),
    (0x111E1, 0x111F4),
    (0x1173A, 0x1173B),
    (0x118EA, 0x118F2),
    (0x11C5A, 0x11C6C),
    (0x11FC0, 0x11FD4),
    (0x12400, 0x1246E),
    (0x16B5B, 0x16B61),
    (0x16E80, 0x16E96),
    (0x1D2C0, 0x1D2D3),
    (0x1D2E0, 0x1D2F3),
    (0x1D360, 0x1D378),
    (0x1E8C7, 0x1E8CF),
    (0x1EC71, 0x1ECAB),
    (0x1ECAD, 0x1ECAF),
    (0x1ECB1, 0x1ECB4),
    (0x1ED01, 0x1ED2D),
    (0x1ED2F, 0x1ED3D),
    (0x1F10B, 0x1F10C),
    (0x20001, 0x20001),
    (0x20064, 0x20064),
    (0x200E2, 0x200E2),
    (0x20121, 0x20121),
    (0x2092A, 0x2092A),
    (0x20983, 0x20983),
    (0x2098C, 0x2098C),
    (0x2099C, 0x2099C),
    (0x20AEA, 0x20AEA),
    (0x20AFD, 0x20AFD),
    (0x20B19, 0x20B19),
    (0x22390, 0x22390),
    (0x22998, 0x22998),
    (0x23B1B, 0x23B1B),
    (0x2626D, 0x2626D),
    (0x2F890, 0x2F890),
];

fn in_ranges(t: &[(u32, u32)], c: u32) -> bool {
    let i = t.partition_point(|&(lo, _)| lo <= c);
    i > 0 && c <= t[i - 1].1
}

/// `str.isdigit` of one character.
pub(crate) fn is_digit(c: char) -> bool {
    is_decimal(c as u32) || in_ranges(DIGIT_ONLY, c as u32)
}

/// `str.isnumeric` of one character.
pub(crate) fn is_numeric(c: char) -> bool {
    is_digit(c) || in_ranges(NUMERIC_ONLY, c as u32)
}

/// Code points in `s`, out of line: `chars().count()` inlines to several
/// hundred bytes of counting loop at each call site.
#[inline(never)]
pub(crate) fn char_len(s: &str) -> usize {
    s.chars().count()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, reason = "tests")]
    use super::ReprWriter;
    use alloc::string::String;
    use mm_core::{Heap, HeapConfig};

    /// A charged writer growing up to the memory limit reallocates a
    /// logarithmic number of times, then fails: it does not creep by each
    /// write once doubling no longer fits (every creep copied the buffer;
    /// the `vm_bytecode` timeouts).
    #[test]
    fn a_charged_writer_grows_geometrically_up_to_the_limit() {
        let heap = Heap::new(HeapConfig {
            memory_limit: 4 << 20,
            ..HeapConfig::default()
        });
        let mut out = String::new();
        let mut charge = heap.reserve_scratch(0).expect("scratch");
        let mut failed = None;
        let mut w = ReprWriter::charged(&mut out, usize::MAX, &heap, &mut charge, &mut failed);
        let mut grew = 0;
        let mut cap = 0;
        while !w.full() {
            w.push_str("\\x01");
            if w.out.capacity() != cap {
                cap = w.out.capacity();
                grew += 1;
            }
        }
        let len = out.len();
        assert!(failed.is_some(), "stopped without a memory error");
        assert!(len > 3 << 20, "failed early, at {len} bytes");
        assert!(grew < 40, "{grew} reallocations for {len} bytes");
    }
}
