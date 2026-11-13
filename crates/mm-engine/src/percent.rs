//! printf-style formatting: `str % args` and `bytes % args`, following
//! CPython 3.14's `PyUnicode_Format` and `_PyBytes_FormatEx` (argument
//! walk, flags, error messages and their order).
//!
//! Builtin values format natively. A value that needs Python code (a user
//! object's `__str__`, `__index__`, a user mapping's `__getitem__`, ...)
//! hands over to the prelude's `_percent`, which reruns the format through
//! `_pct` with the conversions it has done so far, one more each round.
//!
//! A format longer than [`WINDOW`] is parsed a window at a time, and a
//! step at a time (`nstep::op::STR_PCT`, docs/spec.md §5): long literal
//! and `%%` runs become pieces of the result, uncopied, which a fill
//! builds a step at a time; str and `bytes` share the one parse.

use crate::bigint::Big;
use crate::class::Dn;
use crate::class::f;
use crate::cont::Cont;
use crate::nstep::{STEP_WORK, Step, op};
use crate::scratch::{NativeText, NativeVec};
use crate::strmeth::smi;
use crate::text::char_len;
use crate::text::{ReprWriter, Spec};
use crate::types::T;
use crate::vm::{CallArgs, Raised, Vm};
use alloc::{format, string::String, vec::Vec};
use mm_core::{Ref, Value};

/// The outcome of a formatting run.
pub(crate) enum Pct {
    Done(Value),
    /// Python code must convert this value (`conv`: `s`, `r`, `a`, `b`
    /// bytes, `i` int, `x` index, `f` float, `k` the item `v[0][v[1]]`).
    Need(Value, char),
}

const INT_MAX: u64 = i32::MAX as u64;

/// Bytes of a piece of a str result that makes it long: the result is
/// then built a step at a time (docs/spec.md §5).
const PIECE: usize = 1 << 20;

/// Format bytes in one window of a long format: the parse reads a copy of
/// a window at a time (docs/spec.md §5).
const WINDOW: usize = 1 << 16;

/// In a format longer than [`WINDOW`], a literal or `%%` run at least
/// this long is a piece of the result, uncopied.
const RUN: usize = 1 << 10;

/// Digits a double's exact expansion ends within (1,074 after the point,
/// 767 significant): any more are zeros, which a huge precision gets as a
/// piece, unwritten.
const EXACT: u64 = 1100;

/// Fuel units each conversion of a format longer than [`WINDOW`] costs
/// besides its bytes.
const SPEC_UNITS: u64 = 8;

/// `ascii()` of a repr.
pub(crate) fn ascii_escape(vm: &mut Vm, s: NativeText) -> Result<NativeText, Raised> {
    if s.is_ascii() {
        return Ok(s);
    }
    let bytes = s
        .chars()
        .try_fold(0usize, |n, c| {
            n.checked_add(match c as u32 {
                0..0x80 => 1,
                0x80..0x100 => 4,
                0x100..0x10000 => 6,
                _ => 10,
            })
        })
        .ok_or_else(|| vm.err(T::MemoryError, ""))?;
    let mut out = NativeText::default();
    out.reserve(vm, bytes)?;
    for c in s.chars() {
        if c.is_ascii() {
            out.text.push(c);
        } else {
            // Exact capacity is already admitted; use the quoted renderer's
            // fixed-size escape rather than formatting each hex digit.
            ReprWriter::new(&mut out.text, usize::MAX).escape(c, 0);
        }
    }
    Ok(out)
}

/// Characters in UTF-8 text.
fn chars(b: &[u8]) -> usize {
    b.iter().filter(|&&c| c & 0xC0 != 0x80).count()
}

struct Run {
    bytes: bool,
    /// The format, the arguments it was given and whether they are a
    /// mapping.
    fmt: Value,
    given: Value,
    mapping: bool,
    /// The format's length in bytes and the next byte to read.
    n: usize,
    i: usize,
    /// The arguments the next conversion reads (`given`, or the item a
    /// `%(key)` named).
    args: Value,
    arglen: i64,
    argidx: i64,
    // Native formatting neither collects nor runs Python; the prelude's
    // converted-value list stays unchanged until this attempt returns.
    pre: Option<(Ref, u64)>,
    used: u32,
}

impl Run {
    /// The raw words of a long format's state (`Vm::pct_step`).
    fn words(&self) -> [u64; 5] {
        [
            self.i as u64,
            self.argidx.cast_unsigned(),
            self.arglen.cast_unsigned(),
            u64::from(self.used),
            u64::from(self.bytes) | u64::from(self.mapping) << 1,
        ]
    }

    /// The next conversion Python code did, or a request for it.
    fn converted(&mut self, vm: &Vm, v: Value, conv: char) -> Result<Value, Pct> {
        if let Some((items, len)) = self.pre
            && u64::from(self.used) < len
        {
            let p = vm.slot_o(items, self.used);
            self.used += 1;
            return Ok(p);
        }
        Err(Pct::Need(v, conv))
    }
}

/// Appends piece `x` bytes `a..b` to `parts`, extending the last piece
/// when it is the same text and ends at `a`, or the same repeated text
/// (`a` 0, as `%%` runs are).
fn pct_piece(parts: &mut Vec<(Value, usize, usize)>, x: Value, a: usize, b: usize) {
    if let Some(last) = parts.last_mut()
        && last.0 == x
    {
        if last.2 == a {
            last.2 = b;
            return;
        }
        if a == 0 && last.1 == 0 {
            last.2 += b;
            return;
        }
    }
    parts.push((x, a, b));
}

/// Where the conversion after a `%` at `i - 1` of `f` ends, or `None`
/// when `f` ends first; it reads what the parse in [`Vm::pct_run`] reads.
fn spec_end(f: &[u8], mut i: usize) -> Option<usize> {
    let at = |i: usize| f.get(i).copied();
    if at(i)? == b'(' {
        let mut depth = 1;
        i += 1;
        while depth > 0 {
            match at(i)? {
                b'(' => depth += 1,
                b')' => depth -= 1,
                _ => {}
            }
            i += 1;
        }
    }
    while b"-+ #0".contains(&at(i)?) {
        i += 1;
    }
    for part in 0..2 {
        if part == 1 {
            if at(i)? != b'.' {
                break;
            }
            i += 1;
        }
        if at(i)? == b'*' {
            i += 1;
        } else {
            while at(i)?.is_ascii_digit() {
                i += 1;
            }
        }
    }
    if matches!(at(i)?, b'h' | b'l' | b'L') {
        i += 1;
    }
    at(i)?;
    Some(i + 1)
}

/// One formatted conversion: `head` is the length of its sign and prefix
/// (where zero padding goes); `None` for text, which pads with spaces.
/// A conversion's text, where a numeric zero fill goes (after the sign
/// and prefix), and zeros it has besides its text: `(at, count)`, a run
/// too long to write natively (a huge precision), `(0, 0)` without.
type Piece = (Text, Option<usize>, (usize, usize));

enum Text {
    Native(Vec<u8>),
    Rendered(NativeText),
    // Native formatting does not collect or run Python. Keep an arena value
    // and a byte limit instead of copying text before applying precision.
    Arena(Value, usize),
}

impl Text {
    fn bytes<'a>(&'a self, vm: &'a Vm) -> &'a [u8] {
        match self {
            Self::Native(bytes) => bytes,
            Self::Rendered(text) => text.as_bytes(),
            Self::Arena(v, end) => {
                let bytes = if Vm::is_bytes_like(*v) {
                    vm.bytes_data(*v)
                } else {
                    vm.str_bytes(*v)
                };
                &bytes[..(*end).min(bytes.len())]
            }
        }
    }
}

macro_rules! need {
    ($e:expr) => {
        match $e {
            Ok(v) => v,
            Err(need) => return Ok(Err(need)),
        }
    };
}

impl Vm {
    /// `fmt % args`; `pre` is the prelude's conversion list, or `None`.
    /// A format longer than [`WINDOW`] is parsed a window at a time, the
    /// rest a step at a time once this first step has done [`STEP_WORK`]
    /// units (`nstep::op::STR_PCT`, [`Vm::pct_step`]).
    pub(crate) fn percent(&mut self, fmt: Value, args: Value, pre: Value) -> Result<Pct, Raised> {
        let bytes = Vm::is_bytes(fmt);
        let n = self.pct_data(fmt).len();
        if n >= PIECE && !self.pct_data(fmt).contains(&b'%') {
            return self.pct_plain(fmt, args, bytes);
        }
        // A builtin-type subclass instance is typed by the value it
        // extends; a tuple's items are read from it (`PyTuple_Check`).
        let ta = self.type_of(self.unbx(args));
        let tuple = ta == T::tuple;
        let args = if tuple { self.unbx(args) } else { args };
        let mapping = !tuple
            && ta != T::str
            && !(bytes && ta == T::bytes)
            && (matches!(ta, T::dict | T::list | T::bytes | T::range)
                || (self.uflags(args) & f::GETITEM != 0
                    // C deque has `sq_item` only.
                    && self.type_name(args) != "collections.deque"));
        let mut r = Run {
            bytes,
            fmt,
            given: args,
            mapping,
            n,
            i: 0,
            args,
            arglen: if tuple {
                self.tuple_len(args) as i64
            } else {
                -1
            },
            argidx: if tuple { 0 } else { -2 },
            pre: self.list_parts(pre),
            used: 0,
        };
        let mut out = NativeVec::<u8>::default();
        // A long result: its pieces, built a step at a time (`out` holds
        // the short text since the last piece).
        let mut parts: Vec<(Value, usize, usize)> = Vec::new();
        // One admitted reservation for the common case, a short format; a
        // long one (a huge width) grows as it goes and fails only when
        // its output does not fit.
        if out.reserve(&mut self.heap, n.min(256) + 32).is_err() {
            return Err(self.err(T::MemoryError, ""));
        }
        let start = self.work;
        match self.pct_run(&mut r, &mut parts, &mut out, start)? {
            Ok(true) => self.pct_done(&r, Value::NONE, parts, out).map(Pct::Done),
            Ok(false) => {
                self.pct_flush(&mut out, &mut parts, bytes)?;
                let list = self.pct_keep(Value::NONE, &parts)?;
                let v = [fmt, args, r.args, pre, list];
                let w = r.words();
                self.chunk_rest(op::STR_PCT, &v, &w).map(Pct::Done)
            }
            Err(need) => Ok(need),
        }
    }

    /// One step of a long `%` format (`nstep::op::STR_PCT`): state values
    /// `[fmt, args, current args, pre, pieces so far]`, raw words after
    /// the operation `[next byte, argidx, arglen, used, flags]`.
    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn pct_step(&mut self, state: Value) -> Result<Step, Raised> {
        let Some(([fmt, given, args, pre, list], [_, i, argidx, arglen, used, flags])) =
            self.nstate_get::<5, 6>(op::STR_PCT, state)
        else {
            return Err(self.bad_step());
        };
        let n = self.pct_data(fmt).len();
        let (Ok(i), Ok(used)) = (usize::try_from(i), u32::try_from(used)) else {
            return Err(self.bad_step());
        };
        if !(Vm::is_str(fmt) || Vm::is_bytes(fmt)) || i > n || list == Value::NONE {
            return Err(self.bad_step());
        }
        let mut r = Run {
            bytes: flags & 1 != 0,
            fmt,
            given,
            mapping: flags & 2 != 0,
            n,
            i,
            args,
            arglen: arglen.cast_signed(),
            argidx: argidx.cast_signed(),
            pre: self.list_parts(pre),
            used,
        };
        let mut parts = Vec::new();
        let mut out = NativeVec::<u8>::default();
        let start = self.work;
        match self.pct_run(&mut r, &mut parts, &mut out, start)? {
            Ok(true) => Ok(Step::Done(self.pct_done(&r, list, parts, out)?)),
            Ok(false) => {
                self.pct_flush(&mut out, &mut parts, r.bytes)?;
                self.pct_keep(list, &parts)?;
                if let Some(s) = state.as_obj() {
                    self.set_slot_o(s, 2, r.args);
                }
                self.nstate_put(state, 1, &r.words());
                Ok(Step::More)
            }
            // A value Python code must convert: `_pct`'s caller gets the
            // request; the operator hands the format to `_percent`.
            Err(Pct::Need(v, c)) if pre != Value::NONE => Ok(Step::Done(self.pct_need(v, c)?)),
            Err(_) => {
                let f = self.prelude_fn("_percent")?;
                let v = self.request_call(f, alloc::vec![fmt, given], Cont::None);
                Ok(Step::Done(v))
            }
        }
    }

    /// Parses `r`'s format from byte `r.i` on, into `parts` and `out`:
    /// `true` once it is all read, `false` when a long format's step has
    /// done [`STEP_WORK`] units since `begun` (at a window's or a
    /// conversion's end), or the value Python code must convert.
    fn pct_run(
        &mut self,
        r: &mut Run,
        parts: &mut Vec<(Value, usize, usize)>,
        out: &mut NativeVec<u8>,
        begun: u64,
    ) -> Result<Result<bool, Pct>, Raised> {
        let (n, bytes, fmt) = (r.n, r.bytes, r.fmt);
        let long = n > WINDOW;
        // A literal or `%%` run this long is a piece of the result.
        let piece = if long { RUN } else { PIECE };
        let mut f = Vec::new();
        let mut win = WINDOW;
        // A step stops only past where it began: a conversion longer than
        // a window is read in the one step that reaches it.
        let from = r.i;
        while r.i < n {
            if long && r.i > from && self.work.wrapping_sub(begun) >= STEP_WORK {
                return Ok(Ok(false));
            }
            // A copy of the window: formatting can allocate. Its end is
            // at a char boundary, so `out` always holds whole chars.
            let base = r.i;
            let most = (n - base).min(win.saturating_add(3));
            let res = self.heap.reserve_scratch(most);
            let _input_charge = self.hx_o(res)?;
            f.clear();
            let res = f
                .try_reserve_exact(most)
                .map_err(|_| mm_core::Error::OutOfMemory);
            self.hx_o(res)?;
            let data = self.pct_data(fmt);
            let mut end = base.saturating_add(win).min(n);
            while data.get(end).is_some_and(|&b| b & 0xC0 == 0x80) {
                end += 1;
            }
            f.extend_from_slice(data.get(base..end).unwrap_or(&[]));
            let m = f.len();
            self.burn(m as u64 / 8);
            let last = end == n;
            let at = |i: usize| f.get(i).copied().unwrap_or(0);
            let mut i = 0;
            while i < m {
                let c = f[i];
                i += 1;
                if c != b'%' {
                    let end = f[i..].iter().position(|&b| b == b'%').map_or(m, |k| i + k);
                    if end - (i - 1) >= piece {
                        self.pct_flush(out, parts, bytes)?;
                        pct_piece(parts, fmt, base + i - 1, base + end);
                    } else {
                        let res = out.extend_from_slice(&mut self.heap, &f[i - 1..end]);
                        self.hx_o(res)?;
                    }
                    i = end;
                    continue;
                }
                // Where this conversion starts, and the arguments before it:
                // a costly one past this step's work goes in the next step.
                let conv_at = i - 1;
                let before = (r.args, r.arglen, r.argidx, r.used);
                if at(i) == b'%' {
                    let start = i - 1;
                    i += 1;
                    while at(i) == b'%' && at(i + 1) == b'%' {
                        i += 2;
                    }
                    let k = (i - start) / 2;
                    if k >= piece {
                        self.pct_flush(out, parts, bytes)?;
                        let pc = self.intern("%")?;
                        pct_piece(parts, pc, 0, k);
                    } else {
                        let len = out.len().saturating_add(k);
                        let res = out.resize(&mut self.heap, len, b'%');
                        self.hx_o(res)?;
                    }
                    continue;
                }
                if !last && spec_end(&f, i).is_none() {
                    // The conversion goes past the window: the next one
                    // starts at it (twice as long when it is the first).
                    i -= 1;
                    break;
                }
                if at(i) == b'(' {
                    if !r.mapping {
                        return Err(self.err(T::TypeError, "format requires a mapping"));
                    }
                    i += 1;
                    let start = i;
                    let mut depth = 1;
                    while i < m {
                        match f[i] {
                            b'(' => depth += 1,
                            b')' => depth -= 1,
                            _ => {}
                        }
                        if depth == 0 {
                            break;
                        }
                        i += 1;
                    }
                    if depth != 0 {
                        return Err(self.err(T::ValueError, "incomplete format key"));
                    }
                    let key = &f[start..i];
                    i += 1;
                    let key = if bytes {
                        self.new_bytes(key)?
                    } else {
                        self.new_str(core::str::from_utf8(key).unwrap_or(""))?
                    };
                    let args = r.given;
                    r.args = if self.user_class(args).is_some() {
                        let pair = self.alloc_tuple(&[args, key])?;
                        match r.converted(self, pair, 'k') {
                            Ok(v) => v,
                            Err(need) => return Ok(Err(need)),
                        }
                    } else {
                        self.getitem(args, key)?
                    };
                    r.arglen = -1;
                    r.argidx = -2;
                }
                let mut flags = 0u8;
                while let Some(k) = b"-+ #0".iter().position(|&x| x == at(i)) {
                    flags |= 1 << k;
                    i += 1;
                }
                let mut width = 0u64;
                let mut prec: Option<u64> = None;
                for part in 0..2 {
                    if part == 1 {
                        if at(i) != b'.' {
                            break;
                        }
                        i += 1;
                    }
                    let mut v = 0u64;
                    if at(i) == b'*' {
                        i += 1;
                        let a = self.pct_next(r)?;
                        let w = self.pct_star(a, part == 1)?;
                        if w < 0 && part == 0 {
                            flags |= 1;
                        }
                        v = if part == 0 {
                            w.unsigned_abs()
                        } else {
                            w.max(0).unsigned_abs()
                        };
                    } else {
                        // CPython bounds a width by `sys.maxsize` and a
                        // precision by a C int.
                        let max = if part == 0 { i64::MAX as u64 } else { INT_MAX };
                        while at(i).is_ascii_digit() {
                            v = v.saturating_mul(10).saturating_add(u64::from(at(i) - b'0'));
                            if v > max {
                                let msg = match (part, bytes) {
                                    (0, _) => "width too big",
                                    (_, true) => "prec too big",
                                    _ => "precision too big",
                                };
                                return Err(self.err(T::ValueError, msg));
                            }
                            i += 1;
                        }
                    }
                    if part == 0 {
                        width = v;
                    } else {
                        prec = Some(v);
                    }
                }
                if matches!(at(i), b'h' | b'l' | b'L') {
                    i += 1;
                }
                if i >= m {
                    return Err(self.err(T::ValueError, "incomplete format"));
                }
                let conv = f[i];
                let pos = i;
                i += 1;
                let v = self.pct_next(r)?;
                if Vm::is_heap_int(v)
                    && base + conv_at > from
                    && self.work.wrapping_sub(begun) + self.decimal_units(v) > STEP_WORK
                {
                    (r.args, r.arglen, r.argidx, r.used) = before;
                    r.i = base + conv_at;
                    return Ok(Ok(false));
                }
                let ok: &[u8] = if bytes {
                    b"sbracdiuxXoeEfFgG"
                } else {
                    b"sracdiuxXoeEfFgG"
                };
                if !ok.contains(&conv) {
                    return Err(self.pct_unsupported(fmt, base + pos, bytes));
                }
                let (text, head, (zat, zeros)) = match self.pct_one(r, conv, v, prec, flags)? {
                    Ok(p) => p,
                    Err(need) => return Ok(Err(need)),
                };
                let data = text.bytes(self);
                let dl = data.len();
                // Width: numbers zero-fill after their sign and prefix.
                let len = if width == 0 {
                    0
                } else if bytes {
                    data.len() + zeros
                } else {
                    chars(data) + zeros
                } as u64;
                if width > len && !self.heap.would_fit(width) {
                    return Err(self.err(T::MemoryError, ""));
                }
                let fill = usize::try_from(width.saturating_sub(len)).unwrap_or(0);
                let (split, pad) = match head {
                    _ if flags & 1 != 0 => (dl, b' '),
                    Some(h) if flags & 16 != 0 => (h, b'0'),
                    _ => (0, b' '),
                };
                if dl >= PIECE || fill >= PIECE || zeros > 0 {
                    // Long: the text and the fill as pieces, uncopied.
                    let zero = self.intern("0")?;
                    let pad = if pad == b'0' { zero } else { self.intern(" ")? };
                    // A bytearray is a piece too: no Python code runs
                    // between here and the fill.
                    let whole = match text {
                        Text::Arena(v, end) if Vm::is_str(v) || Vm::is_bytes_like(v) => {
                            Some((v, end.min(dl)))
                        }
                        _ => None,
                    };
                    self.pct_flush(out, parts, bytes)?;
                    let t = if let Some((v, _)) = whole {
                        v
                    } else {
                        let t = text.bytes(self).to_vec();
                        self.pct_text(&t, bytes)?
                    };
                    let tl = whole.map_or_else(|| self.pct_data(t).len(), |w| w.1);
                    if zeros == 0 {
                        parts.extend([(t, 0, split), (pad, 0, fill), (t, split, tl)]);
                    } else if flags & 1 != 0 {
                        // Left aligned: the fill after the text and zeros.
                        parts.extend([(t, 0, zat), (zero, 0, zeros), (t, zat, tl), (pad, 0, fill)]);
                    } else {
                        parts.extend([
                            (t, 0, split),
                            (pad, 0, fill),
                            (t, split, zat),
                            (zero, 0, zeros),
                            (t, zat, tl),
                        ]);
                    }
                    continue;
                }
                // Admit the whole padded piece, rather than growing separately
                // for its prefix, padding and suffix.
                let start = out.len();
                let needed = start.saturating_add(dl).saturating_add(fill);
                let res = out.resize(&mut self.heap, needed, pad);
                self.hx_o(res)?;
                let text = text.bytes(self);
                out[start..start + split].copy_from_slice(&text[..split]);
                out[start + split + fill..].copy_from_slice(&text[split..]);
                if long {
                    self.burn(SPEC_UNITS);
                }
                // Any format ends a step at a conversion past its work (a
                // big int's decimal text is charged by its size).
                if self.work.wrapping_sub(begun) >= STEP_WORK {
                    r.i = base + i;
                    return Ok(Ok(false));
                }
            }
            win = if i == 0 {
                win.saturating_mul(2)
            } else {
                WINDOW
            };
            r.i = base + i;
        }
        Ok(Ok(true))
    }

    /// The end of a run: CPython's check that every argument was used,
    /// then the result, from the pieces kept in `list` (or `None`), then
    /// `parts`, then `out`.
    fn pct_done(
        &mut self,
        r: &Run,
        list: Value,
        mut parts: Vec<(Value, usize, usize)>,
        mut out: NativeVec<u8>,
    ) -> Result<Value, Raised> {
        if r.argidx < r.arglen && !r.mapping {
            let msg = format!(
                "not all arguments converted during {} formatting",
                if r.bytes { "bytes" } else { "string" }
            );
            return Err(self.err(T::TypeError, &msg));
        }
        if list == Value::NONE && parts.is_empty() {
            return if r.bytes {
                self.new_bytes(&out)
            } else {
                self.new_str(core::str::from_utf8(&out).unwrap_or(""))
            };
        }
        self.pct_flush(&mut out, &mut parts, r.bytes)?;
        let kept = self.seq_words(list).as_chunks::<3>().0.iter().map(|w| {
            let v = |i: usize| Value::from_bits(w[i]).unwrap_or(Value::NONE);
            let at = |i: usize| v(i).as_int().and_then(|x| usize::try_from(x).ok());
            (v(0), at(1).unwrap_or(0), at(2).unwrap_or(0))
        });
        let all: Vec<_> = kept.chain(parts).collect();
        crate::strmeth::fill_kind(self, &all, r.bytes)
    }

    /// `list` (a new one for `None`) with `parts` appended, a piece as
    /// three items: the pieces of a long format between its steps.
    fn pct_keep(&mut self, list: Value, parts: &[(Value, usize, usize)]) -> Result<Value, Raised> {
        let list = if list == Value::NONE {
            self.list_with_room(0, 3 * parts.len())?
        } else {
            list
        };
        for &(x, a, b) in parts {
            for v in [x, smi(a), smi(b)] {
                self.list_push(list, v)?;
            }
        }
        Ok(list)
    }

    /// `(v, conversion)`: `_pct`'s request that Python code convert `v`.
    fn pct_need(&mut self, v: Value, c: char) -> Result<Value, Raised> {
        let c = self.new_str(c.encode_utf8(&mut [0; 4]))?;
        self.alloc_tuple(&[v, c])
    }

    /// A new `bytes` (`str`) of `t`.
    fn pct_text(&mut self, t: &[u8], bytes: bool) -> Result<Value, Raised> {
        if bytes {
            self.new_bytes(t)
        } else {
            self.new_str(core::str::from_utf8(t).unwrap_or(""))
        }
    }

    /// The short text since the last piece of a long result, as a piece.
    #[cold]
    fn pct_flush(
        &mut self,
        out: &mut NativeVec<u8>,
        parts: &mut Vec<(Value, usize, usize)>,
        bytes: bool,
    ) -> Result<(), Raised> {
        if !out.is_empty() {
            let t = self.pct_text(out, bytes)?;
            parts.push((t, 0, out.len()));
            out.truncate(0);
        }
        Ok(())
    }

    /// The text of a str `fmt`, or the data of a `bytes` one.
    fn pct_data(&self, fmt: Value) -> &[u8] {
        if Vm::is_bytes(fmt) {
            self.bytes_data(fmt)
        } else {
            self.str_bytes(fmt)
        }
    }

    /// `fmt % args` for a long `fmt` without conversions: `fmt` itself,
    /// or CPython's error when `args` has items to convert.
    #[cold]
    fn pct_plain(&mut self, fmt: Value, args: Value, bytes: bool) -> Result<Pct, Raised> {
        let ta = self.type_of(self.unbx(args));
        let ok = if ta == T::tuple {
            self.tuple_len(self.unbx(args)) == 0
        } else {
            ta != T::str
                && !(bytes && ta == T::bytes)
                && (matches!(ta, T::dict | T::list | T::bytes | T::range)
                    || (self.uflags(args) & f::GETITEM != 0
                        && self.type_name(args) != "collections.deque"))
        };
        if !ok {
            let msg = format!(
                "not all arguments converted during {} formatting",
                if bytes { "bytes" } else { "string" }
            );
            return Err(self.err(T::TypeError, &msg));
        }
        self.burn_bytes(self.pct_data(fmt).len());
        Ok(Pct::Done(fmt))
    }

    #[cold]
    fn pct_unsupported(&mut self, fmt: Value, pos: usize, bytes: bool) -> Raised {
        let f = self.pct_data(fmt);
        let (c, at) = if bytes {
            if f.get(pos).is_some_and(|&b| b >= 0x80) {
                return self.err(
                    T::OverflowError,
                    "character argument not in range(0x110000)",
                );
            }
            (char::from(f.get(pos).copied().unwrap_or(0)), pos)
        } else {
            let s = core::str::from_utf8(f.get(pos..).unwrap_or(&[])).unwrap_or("?");
            let c = s.chars().next().unwrap_or('?');
            (c, chars(f.get(..pos).unwrap_or(&[])))
        };
        let code = c as u32;
        let shown = if bytes || (31..=126).contains(&code) {
            c
        } else {
            '?'
        };
        let msg = format!("unsupported format character '{shown}' (0x{code:x}) at index {at}");
        self.err(T::ValueError, &msg)
    }

    fn pct_next(&mut self, r: &mut Run) -> Result<Value, Raised> {
        if r.argidx < r.arglen {
            r.argidx += 1;
            if r.arglen < 0 {
                return Ok(r.args);
            }
            let k = u32::try_from(r.argidx - 1).unwrap_or(0);
            return Ok(self.vslot_o(r.args, k));
        }
        Err(self.err(T::TypeError, "not enough arguments for format string"))
    }

    /// A `*` width (`ssize_t`) or precision (`int`).
    fn pct_star(&mut self, v: Value, prec: bool) -> Result<i64, Raised> {
        // An int subclass instance is its value (`PyLong_Check`).
        let v = self.unbx(v);
        if !Vm::is_intlike(v) {
            return Err(self.err(T::TypeError, "* wants int"));
        }
        let i = self.int_of_o(v);
        let ok = if prec {
            i.and_then(|i| i32::try_from(i).ok()).map(i64::from)
        } else {
            i
        };
        ok.ok_or_else(|| {
            let msg = format!(
                "Python int too large to convert to C {}",
                if prec { "int" } else { "ssize_t" }
            );
            self.err(T::OverflowError, &msg)
        })
    }

    /// One conversion, before padding (`flags`: bit 1 `+`, 2 space,
    /// 3 `#`).
    #[inline(never)]
    fn pct_one(
        &mut self,
        r: &mut Run,
        conv: u8,
        mut v: Value,
        prec: Option<u64>,
        flags: u8,
    ) -> Result<Result<Piece, Pct>, Raised> {
        let bytes = r.bytes;
        let user = self.user_class(v).is_some();
        let original = v;
        let uf = self.uflags(v);
        // CPython tests `PyLong_Check` / `PyFloat_Check` before calling a
        // conversion dunder: an int or float subclass is its own value.
        let back = self.backing(v);
        let int_back = back.filter(|&b| Vm::is_intlike(b));
        let float_back = back.filter(|b| b.is_float());
        let mut text = match conv {
            b's' | b'r' | b'a' if !bytes => {
                if user || self.text_needs_py(v) {
                    let s = need!(r.converted(self, v, char::from(conv)));
                    Text::Arena(s, usize::MAX)
                } else if conv == b's' && Vm::is_str(v) {
                    Text::Arena(v, usize::MAX)
                } else {
                    let saved = self.exc;
                    let s = if conv == b's' {
                        self.stringify(v)
                    } else {
                        self.repr(v)
                    };
                    match s {
                        // A long container `repr` continues in Python, on fuel.
                        Err(_) if self.slow => {
                            self.slow = false;
                            self.exc = saved;
                            let s = need!(r.converted(self, v, char::from(conv)));
                            Text::Arena(s, usize::MAX)
                        }
                        Err(e) => return Err(e),
                        Ok(s) if conv == b'a' => Text::Rendered(ascii_escape(self, s)?),
                        Ok(s) => Text::Rendered(s),
                    }
                }
            }
            b's' | b'b' => {
                // CPython's `format_obj`: a bytes or bytearray (a subclass
                // too) as it is, before `__bytes__`.
                if let Some(b) = self.backing(v)
                    && Vm::is_bytes_like(b)
                {
                    v = b;
                } else if user && self.user_method(v, Dn::Bytes).is_some() {
                    v = need!(r.converted(self, v, 'b'));
                }
                if !Vm::is_bytes_like(v) {
                    let tn = self.type_name(original);
                    let msg = format!(
                        "%b requires a bytes-like object, or an object that implements __bytes__, not '{tn}'"
                    );
                    return Err(self.err(T::TypeError, &msg));
                }
                Text::Arena(v, usize::MAX)
            }
            b'r' | b'a' => {
                if user || self.text_needs_py(v) {
                    let s = need!(r.converted(self, v, 'a'));
                    Text::Arena(s, usize::MAX)
                } else {
                    let saved = self.exc;
                    match self.repr(v) {
                        Err(_) if self.slow => {
                            self.slow = false;
                            self.exc = saved;
                            let s = need!(r.converted(self, v, 'a'));
                            Text::Arena(s, usize::MAX)
                        }
                        Err(e) => return Err(e),
                        Ok(text) => Text::Rendered(ascii_escape(self, text)?),
                    }
                }
            }
            b'c' => {
                // A one-char str (a one-byte bytes for `b'%c'`) before
                // `__index__`, as CPython's `formatchar`/`byte_converter`.
                let same = |b: Value| {
                    if bytes {
                        Vm::is_bytes(b)
                    } else {
                        Vm::is_str(b)
                    }
                };
                if let Some(b) = int_back.or(back.filter(|&b| same(b))) {
                    v = b;
                } else if uf & f::INDEX != 0 {
                    v = need!(r.converted(self, v, 'x'));
                }
                return self
                    .pct_char(v, bytes, original)
                    .map(|t| Ok((Text::Native(t), None, (0, 0))));
            }
            b'e' | b'E' | b'f' | b'F' | b'g' | b'G' => {
                // An int subclass converts by its own `__float__` only.
                if let Some(b) = float_back {
                    v = b;
                } else if uf & f::FLOAT != 0 || int_back.is_none() && uf & f::INDEX != 0 {
                    v = need!(r.converted(self, v, 'f'));
                } else if let Some(b) = int_back {
                    v = b;
                }
                return self
                    .pct_float(v, conv, prec, flags, bytes, original)
                    .map(Ok);
            }
            _ => {
                // d i u x X o
                let int_only = matches!(conv, b'x' | b'X' | b'o');
                let wants = if int_only {
                    f::INDEX
                } else {
                    f::INDEX | f::INT
                };
                if let Some(b) = int_back {
                    v = b;
                } else if uf & wants != 0 {
                    v = need!(r.converted(self, v, if int_only { 'x' } else { 'i' }));
                }
                if int_only && let Some((spec, head)) = self.pct_long_pow2(v, conv, prec, flags) {
                    // A long int's digits a step at a time, by `format`
                    // in Python (`_percent`).
                    let spec = self.new_str(&spec)?;
                    let req = self.alloc_tuple(&[v, spec])?;
                    let t = need!(r.converted(self, req, 'p'));
                    return Ok(Ok((Text::Arena(t, usize::MAX), Some(head), (0, 0))));
                }
                return self.pct_int(v, conv, prec, flags, original).map(Ok);
            }
        };
        if let Some(p) = prec {
            let p = usize::try_from(p).unwrap_or(usize::MAX);
            let data = text.bytes(self);
            let cut = if bytes {
                p.min(data.len())
            } else {
                data.iter()
                    .enumerate()
                    .filter(|&(_, &c)| c & 0xC0 != 0x80)
                    .nth(p)
                    .map_or(data.len(), |(k, _)| k)
            };
            match &mut text {
                Text::Native(data) => data.truncate(cut),
                Text::Rendered(text) => text.text.truncate(cut),
                Text::Arena(_, end) => *end = cut,
            }
        }
        Ok(Ok((text, None, (0, 0))))
    }

    /// `%c`.
    fn pct_char(&mut self, v: Value, bytes: bool, original: Value) -> Result<Vec<u8>, Raised> {
        let (limit, what) = if bytes {
            (256, "an integer in range(256) or a single byte")
        } else {
            (0x11_0000, "an int or a unicode character")
        };
        let v = self.unbx(v);
        if Vm::is_intlike(v) {
            let Some(c) = self.int_of_o(v).filter(|&i| (0..limit).contains(&i)) else {
                let msg = format!(
                    "%c arg not in range({})",
                    if bytes { "256" } else { "0x110000" }
                );
                return Err(self.err(T::OverflowError, &msg));
            };
            if bytes {
                return Ok(alloc::vec![c as u8]);
            }
            let Some(c) = char::from_u32(c as u32) else {
                return Err(self.surrogate("%c", c));
            };
            return Ok(String::from(c).into_bytes());
        }
        let (len, data) = if bytes && Vm::is_bytes_like(v) {
            let d = self.bytes_data(v);
            (d.len(), d)
        } else if !bytes && Vm::is_str(v) {
            let s = self.str_ref_o(v);
            (char_len(s), s.as_bytes())
        } else {
            let tn = self.type_name(original);
            let msg = format!("%c requires {what}, not {tn}");
            return Err(self.err(T::TypeError, &msg));
        };
        if len == 1 {
            return Ok(data.to_vec());
        }
        let kind = if bytes { "a bytes object" } else { "a string" };
        let msg = format!("%c requires {what}, not {kind} of length {len}");
        Err(self.err(T::TypeError, &msg))
    }

    /// `%d %i %u %x %X %o`.
    fn pct_int(
        &mut self,
        v: Value,
        conv: u8,
        prec: Option<u64>,
        flags: u8,
        original: Value,
    ) -> Result<Piece, Raised> {
        let int_only = matches!(conv, b'x' | b'X' | b'o');
        let v = self.unbx(v);
        if let Some(n) = v.as_int() {
            // A small int: digits on the stack, no bigint.
            let (radix, upper, prefix) = match conv {
                b'x' => (16, false, "0x"),
                b'X' => (16, true, "0X"),
                b'o' => (8, false, "0o"),
                _ => (10, false, ""),
            };
            let mut buf = [0; 64];
            let digits = crate::text::small_digits(n.unsigned_abs(), radix, upper, &mut buf);
            return self.pct_digits(n < 0, prefix, digits, prec, flags);
        }
        let b = if let Some(b) = self.big_of(v) {
            b
        } else if let (false, Some(x)) = (int_only, v.as_float()) {
            if x.is_nan() {
                return Err(self.err(T::ValueError, "cannot convert float NaN to integer"));
            }
            if x.is_infinite() {
                return Err(self.err(T::OverflowError, "cannot convert float infinity to integer"));
            }
            Big::from_f64(x)
        } else {
            let what = if int_only {
                "an integer"
            } else {
                "a real number"
            };
            let tn = self.type_name(original);
            let msg = format!("%{} format: {what} is required, not {tn}", char::from(conv));
            return Err(self.err(T::TypeError, &msg));
        };
        if prec.unwrap_or(0) > INT_MAX - 3 {
            return Err(self.err(T::OverflowError, "precision too large"));
        }
        // The digits and `pct_digits`' copy of them.
        let (digits, prefix, _native) = match conv {
            b'x' | b'X' | b'o' => {
                let radix = if conv == b'o' { 8 } else { 16 };
                let (d, charge) = self.pow2_digits(&b, radix, conv == b'X', 2)?;
                let prefix = match conv {
                    b'x' => "0x",
                    b'X' => "0X",
                    _ => "0o",
                };
                (d, prefix, Some(charge))
            }
            _ => (self.big_decimal(&b.abs())?, "", None),
        };
        self.pct_digits(b.neg, prefix, &digits, prec, flags)
    }

    /// For `%x %X %o` of an int of more than a million digits, without
    /// zeros for a precision: the `format` spec that gives the same text
    /// ([`Vm::pow2_text`], a step at a time) and its sign and prefix bytes.
    #[cfg_attr(mm_shipped, optimize(size))]
    fn pct_long_pow2(
        &self,
        v: Value,
        conv: u8,
        prec: Option<u64>,
        flags: u8,
    ) -> Option<(String, usize)> {
        let r = v.as_obj().filter(|_| Vm::is_heap_int(v))?;
        let bits = self.heap_int_bits(v);
        let digits = bits.div_ceil(if conv == b'o' { 3 } else { 4 });
        if digits <= crate::text::POW2_LONG as u64 || prec.unwrap_or(0) > digits {
            return None;
        }
        let mut spec = String::new();
        match flags & 6 {
            2 | 6 => spec.push('+'),
            4 => spec.push(' '),
            _ => {}
        }
        let alt = flags & 8 != 0;
        if alt {
            spec.push('#');
        }
        spec.push(char::from(conv));
        let neg = self.raw_o(r, 1) == 1;
        let head = usize::from(neg || flags & 6 != 0) + 2 * usize::from(alt);
        Some((spec, head))
    }

    /// The text of `%d` and the like from an int's sign and digits.
    fn pct_digits(
        &mut self,
        neg: bool,
        prefix: &str,
        digits: &str,
        prec: Option<u64>,
        flags: u8,
    ) -> Result<Piece, Raised> {
        let p = prec.unwrap_or(0);
        if p > INT_MAX - 3 {
            return Err(self.err(T::OverflowError, "precision too large"));
        }
        self.burn(digits.len() as u64 / 8);
        if !self.heap.would_fit(p) {
            return Err(self.err(T::MemoryError, ""));
        }
        let zeros = usize::try_from(p).unwrap_or(0).saturating_sub(digits.len());
        let mut text: Vec<u8> = Vec::with_capacity(1 + prefix.len() + zeros + digits.len());
        text.extend_from_slice(match (neg, flags & 6) {
            (true, _) => b"-",
            (_, 2 | 6) => b"+",
            (_, 4) => b" ",
            _ => b"",
        });
        if flags & 8 != 0 {
            text.extend_from_slice(prefix.as_bytes());
        }
        let head = text.len();
        // A precision this long is a piece of zeros, unwritten.
        let (inline, extra) = if zeros >= PIECE {
            (0, zeros)
        } else {
            (zeros, 0)
        };
        text.resize(head + inline, b'0');
        text.extend_from_slice(digits.as_bytes());
        Ok((Text::Native(text), Some(head), (head, extra)))
    }

    /// `%e %E %f %F %g %G`.
    fn pct_float(
        &mut self,
        v: Value,
        conv: u8,
        prec: Option<u64>,
        flags: u8,
        bytes: bool,
        original: Value,
    ) -> Result<Piece, Raised> {
        let v = self.unbx(v);
        let x = match v.as_float() {
            Some(x) => Some(x),
            None if Vm::is_intlike(v) => self.big_of(v).and_then(|b| b.to_f64()),
            None => None,
        };
        let Some(x) = x else {
            if !bytes && Vm::is_intlike(v) {
                return Err(self.err(T::OverflowError, "int too large to convert to float"));
            }
            let tn = self.type_name(original);
            let msg = if bytes {
                format!("float argument required, not {tn}")
            } else {
                format!("must be real number, not {tn}")
            };
            return Err(self.err(T::TypeError, &msg));
        };
        let mut p = prec.unwrap_or(6);
        let alt = flags & 8 != 0;
        if matches!(conv, b'g' | b'G') && !alt {
            // Digits past a double's exact expansion are zeros the `g`
            // presentation strips.
            p = p.min(800);
        }
        let extra = if x.is_finite() && p >= EXACT + PIECE as u64 {
            let extra = p - EXACT;
            p = EXACT;
            extra
        } else {
            0
        };
        if x.is_finite()
            && !self
                .heap
                .would_fit(p.saturating_add(extra).saturating_mul(2))
        {
            return Err(self.err(T::MemoryError, ""));
        }
        let spec = Spec {
            fill: ' ',
            align: None,
            sign: match flags & 6 {
                2 | 6 => Some('+'),
                4 => Some(' '),
                _ => None,
            },
            zflag: false,
            alt,
            zero: false,
            width: 0,
            grouping: None,
            precision: Some(usize::try_from(p).unwrap_or(usize::MAX)),
            frac_grouping: None,
            ty: Some(char::from(conv)),
        };
        self.burn(p / 8);
        let s = self.format_float(x, &spec)?;
        let head = usize::from(s.starts_with(['-', '+', ' ']));
        if extra == 0 {
            return Ok((Text::Rendered(s), Some(head), (0, 0)));
        }
        // The zeros go before an exponent, else at the end.
        let at = s.find(['e', 'E']).unwrap_or(s.len());
        let extra = usize::try_from(extra).unwrap_or(usize::MAX);
        Ok((Text::Rendered(s), Some(head), (at, extra)))
    }
}

/// `_pct(fmt, args, pre)`: the prelude's rerun (see the module docs);
/// returns the result or a `(value, conversion)` request.
pub(crate) fn pl_pct(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let (fmt, args, pre) = (vm.arg_o(&a, 0), vm.arg_o(&a, 1), vm.arg_o(&a, 2));
    match vm.percent(fmt, args, pre)? {
        Pct::Done(v) => Ok(v),
        Pct::Need(v, c) => vm.pct_need(v, c),
    }
}
