//! `str` methods.

use crate::bytes::{Job, R_BYTEARRAY, R_BYTES};
use crate::nstep::{STEP_WORK, Step, op};
use crate::scratch::{NativeText, NativeVec};
use crate::text::FormatSpec;
use crate::text::char_len;
use crate::types::{T, ty};
use crate::vm::{CallArgs, Raised, Vm};
use alloc::{format, string::String, vec::Vec};
use mm_core::{ByteRef, Ref, Scratch, Value};

/// The receiver as text, charged as byte work: the copy, and the scans
/// the methods that take it make (docs/spec.md §5).
fn this(vm: &mut Vm, a: &CallArgs, name: &str) -> Result<String, Raised> {
    let s = vm.arg_o(a, 0);
    if !Vm::is_str(s) {
        return Err(vm.err(
            T::TypeError,
            &format!(
                "descriptor '{name}' for 'str' objects doesn't apply to a '{}' object",
                vm.type_name(s)
            ),
        ));
    }
    let s = String::from(vm.str_ref_o(s));
    vm.burn_bytes(s.len());
    Ok(s)
}

/// The receiver `str` value, or the `this` error.
fn this_value(vm: &mut Vm, a: &CallArgs, name: &str) -> Result<Value, Raised> {
    let s = vm.arg_o(a, 0);
    if Vm::is_str(s) {
        return Ok(s);
    }
    this(vm, a, name).map(|_| s)
}

/// A `str` argument as a value, or the `str_arg` error.
fn str_arg_value(vm: &mut Vm, v: Value, what: &str) -> Result<Value, Raised> {
    if Vm::is_str(v) {
        return Ok(v);
    }
    let b = vm.unbx(v);
    str_arg(vm, v, what).map(|_| b)
}

/// New strings for byte ranges of `str` `s` (char boundaries), as a list.
/// With `whole`, a lone piece that is all of `s` is `s` itself, as
/// CPython's split gives the receiver back.
fn list_of_ranges(
    vm: &mut Vm,
    s: Value,
    ranges: &[(usize, usize)],
    whole: bool,
) -> Result<Value, Raised> {
    if whole && ranges == [(0, vm.str_bytes(s).len())] {
        return vm.new_list(&[s]);
    }
    let Some(r) = s.as_obj() else {
        return vm.new_list(&[]);
    };
    let res = vm
        .heap
        .reserve_scratch(ranges.len().saturating_mul(size_of::<Value>()));
    let _items = vm.hx_o(res)?;
    let mut items = Vec::with_capacity(ranges.len());
    for &(st, en) in ranges {
        items.push(str_slice(vm, r, st, en)?);
    }
    vm.new_list(&items)
}

/// Bytes `st..en` (char boundaries) of `str` `r` as a new `str`, made as
/// [`Vm::new_str`] makes one. Short ASCII pieces skip the shared builder.
#[inline(never)]
fn str_slice(vm: &mut Vm, r: Ref, st: usize, en: usize) -> Result<Value, Raised> {
    let mut small = [0u8; 64];
    let n = en.saturating_sub(st);
    let ascii = vm.str_ascii(Value::from_ref(r, ty::STR as u8));
    if (2..=small.len()).contains(&n)
        && ascii
        && let Some(piece) = vm.heap.str_bytes(r).ok().and_then(|b| b.get(st..en))
    {
        let buf = &mut small[..n];
        buf.copy_from_slice(piece);
        let res = vm.heap.new_str_ascii(buf);
        let piece = vm.hx_o(res)?;
        return Ok(Value::from_ref(piece, ty::STR as u8));
    }
    vm.build_text(n, |vm, buf| buf.push_slice(&vm.heap, r, st, en))
}

/// Bytes `st..en` (char boundaries) of `str` `s`, a slice per step when
/// that is long (its value then a request): for a native's or a step's
/// result, which can hand the rest to steps.
#[inline(never)]
pub(crate) fn slice_steps(vm: &mut Vm, s: Value, st: usize, en: usize) -> Result<Value, Raised> {
    let n = en.saturating_sub(st);
    if (n / UNIT) as u64 >= FILL && n < text(vm, s).len() {
        return fill_parts(vm, &[(s, st, en)]);
    }
    slice_copy(vm, s, st, en)
}

/// `a + b` for two non-empty strs whose result is [`FILL_BYTES`] or
/// longer, a slice per step (`Vm::str_concat`).
#[inline(never)]
pub(crate) fn concat_steps(vm: &mut Vm, a: Value, b: Value) -> Result<Value, Raised> {
    let (la, lb) = (text(vm, a).len(), text(vm, b).len());
    fill_parts(vm, &[(a, 0, la), (b, 0, lb)])
}

/// Bytes `st..en` (char boundaries) of `str` `s` as a `str`: `s` itself
/// when that is all of it, and a long piece copied once, block to block
/// (a copy at memcpy speed; docs/spec.md §5).
#[inline(never)]
pub(crate) fn slice_copy(vm: &mut Vm, s: Value, st: usize, en: usize) -> Result<Value, Raised> {
    let Some(r) = s.as_obj() else {
        return vm.new_str("");
    };
    let n = en.saturating_sub(st);
    if st == 0 && n == vm.str_bytes(s).len() {
        return Ok(s);
    }
    if n < 1 << 16 {
        return str_slice(vm, r, st, en);
    }
    let chars = if vm.str_ascii(s) {
        n
    } else {
        vm.str_index(s, en) - vm.str_index(s, st)
    };
    let (src, h) = (data(vm, s)?, vm.str_bytes(s));
    if h.get(st..en).is_none() || ceil_char(h, st) != st || ceil_char(h, en) != en {
        return Err(vm.bad_step());
    }
    let out = block(vm, n)?;
    vm.burn_result(n, false);
    let res = out
        .as_byte_ref()
        .ok_or(mm_core::Error::BadRef)
        .and_then(|d| vm.heap.copy_bytes(src, st, d, 0, n).map(|()| d))
        .and_then(|d| vm.heap.str_from_block(d, chars as u64));
    let r = vm.hx_o(res)?;
    Ok(Value::from_ref(r, ty::STR as u8))
}

/// The first `b` in `hay`, eight bytes at a time.
fn find_byte(b: u8, hay: &[u8]) -> Option<usize> {
    const LO: u64 = 0x0101_0101_0101_0101;
    const HI: u64 = 0x8080_8080_8080_8080;
    let rep = u64::from(b) * LO;
    let (chunks, rest) = hay.as_chunks::<8>();
    for (k, c) in chunks.iter().enumerate() {
        let x = u64::from_le_bytes(*c) ^ rep;
        // The lowest zero byte of `x` sets the lowest bit of `t` exactly.
        let t = x.wrapping_sub(LO) & !x & HI;
        if t != 0 {
            return Some(k * 8 + (t.trailing_zeros() / 8) as usize);
        }
    }
    rest.iter()
        .position(|&c| c == b)
        .map(|p| chunks.len() * 8 + p)
}

/// The last `b` in `hay`, eight bytes at a time.
fn rfind_byte(b: u8, hay: &[u8]) -> Option<usize> {
    const LO: u64 = 0x0101_0101_0101_0101;
    const HI: u64 = 0x8080_8080_8080_8080;
    let rep = u64::from(b) * LO;
    let (rest, chunks) = hay.as_rchunks::<8>();
    for (k, c) in chunks.iter().enumerate().rev() {
        let x = u64::from_le_bytes(*c) ^ rep;
        // Some byte of `x` is zero; which one, the bytes say.
        if x.wrapping_sub(LO) & !x & HI != 0
            && let Some(p) = c.iter().rposition(|&y| y == b)
        {
            return Some(rest.len() + k * 8 + p);
        }
    }
    rest.iter().rposition(|&c| c == b)
}

/// Longest pattern searched byte by byte; longer ones use `str`'s
/// two-way searcher, whose worst case stays linear.
const SHORT_PAT: usize = 8;

/// The first occurrence of `p` (1 to `SHORT_PAT` bytes) in `rest`.
#[inline]
fn find_short(rest: &[u8], p: &[u8]) -> Option<usize> {
    let (&first, tail) = p.split_first()?;
    let (mut at, mut misses) = (0, 0usize);
    while let Some(k) = rest.get(at..).and_then(|r| find_byte(first, r)) {
        let i = at + k;
        if rest.get(i + 1..i + p.len()) == Some(tail) {
            return Some(i);
        }
        at = i + 1;
        misses += 1;
        if misses >= DENSE && misses * 8 >= at {
            // The first byte is everywhere ('ab' in 'aaa…'): a word
            // compare per position is several times faster from here.
            return find_dense(rest, p, at);
        }
    }
    None
}

/// Failed first-byte matches after which a short pattern search turns to
/// [`find_dense`] (when they are at least one in 8 bytes).
const DENSE: usize = 32;

/// The first occurrence of `p` (1 to `SHORT_PAT` bytes) in `rest` at or
/// after `at`, one masked word compare per position.
#[cold]
#[inline(never)]
fn find_dense(rest: &[u8], p: &[u8], mut at: usize) -> Option<usize> {
    let (want, mask) = dense_word(p);
    while let Some(w) = rest.get(at..at + 8) {
        if word(w) & mask == want {
            return Some(at);
        }
        at += 1;
    }
    (at..rest.len()).find(|&i| rest.get(i..i + p.len()) == Some(p))
}

/// The last occurrence of `p` (1 to `SHORT_PAT` bytes) ending at or
/// before `end` in `w`, one masked word compare per position.
#[cold]
#[inline(never)]
fn rfind_dense(w: &[u8], p: &[u8], end: usize) -> Option<usize> {
    let (want, mask) = dense_word(p);
    let mut st = end.checked_sub(p.len())?;
    loop {
        let hit = match w.get(st..st + 8) {
            Some(x) => word(x) & mask == want,
            None => w.get(st..st + p.len()) == Some(p),
        };
        if hit {
            return Some(st);
        }
        st = st.checked_sub(1)?;
    }
}

/// Pattern `p` (1 to 8 bytes) as a little-endian word and its mask.
fn dense_word(p: &[u8]) -> (u64, u64) {
    let mut b = [0u8; 8];
    let n = p.len().min(8);
    b[..n].copy_from_slice(&p[..n]);
    (u64::from_le_bytes(b), u64::MAX >> (8 * (8 - n)))
}

/// The first 8 bytes of `w` (8 or more) as a little-endian word.
fn word(w: &[u8]) -> u64 {
    let mut b = [0u8; 8];
    b.copy_from_slice(&w[..8]);
    u64::from_le_bytes(b)
}

/// Calls `f` with the start of each non-overlapping occurrence of
/// non-empty `pat` in `hay`, left to right (as `str::match_indices`), at
/// most `max` times; returns the count.
#[inline(never)]
fn each_match(hay: &str, pat: &str, max: usize, f: &mut dyn FnMut(usize)) -> usize {
    let (h, p) = (hay.as_bytes(), pat.as_bytes());
    let mut n = 0;
    if p.len() > SHORT_PAT {
        // One two-way searcher for every match: its setup reads the whole
        // needle, and one a match made a 400,000-char needle's `replace`
        // of 16 MB one 117 ms step [M]. No room for it: not searched.
        if p.len() <= h.len() {
            for (i, _) in hay.match_indices(pat).take(max) {
                f(i);
                n += 1;
            }
        }
        return n;
    }
    let mut from = 0;
    while n < max
        && let Some(i) = h.get(from..).and_then(|r| find_short(r, p))
    {
        f(from + i);
        n += 1;
        from += i + p.len();
    }
    n
}

/// A `str` (or str subclass, as its value) argument; `what` names it in
/// the error ("" for CPython's bare "must be str").
fn str_arg(vm: &mut Vm, v: Value, what: &str) -> Result<String, Raised> {
    let b = if Vm::is_str(v) { v } else { vm.unbx(v) };
    if Vm::is_str(b) {
        let s = String::from(vm.str_ref_o(b));
        vm.burn_bytes(s.len());
        return Ok(s);
    }
    let sp = if what.is_empty() { "" } else { " " };
    let tn = if v == Value::NONE {
        "None"
    } else {
        vm.type_name(v)
    };
    let msg = format!("{what}{sp}must be str, not {tn}");
    Err(vm.err(T::TypeError, &msg))
}

fn opt(v: Value) -> Option<Value> {
    if v == Value::UNDEFINED || v == Value::NONE {
        None
    } else {
        Some(v)
    }
}

/// Byte ranges of a `str`'s pieces, admitted as they grow: `n` bytes can
/// split into `n + 1` pieces, 16 bytes each here before any is made.
type Ranges = NativeVec<(usize, usize)>;

pub(crate) fn join(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let sep = this_value(vm, &a, "join")?;
    let sep = vm.unbx(sep);
    // A long list or tuple of strs is read in place, a step at a time.
    let seq = vm.arg_o(&a, 1);
    if let Some(n) = vm.seq_items_len(seq)
        && (4 * n) as u64 >= BIG
    {
        // Only the items' tags are read here; their lengths in steps.
        let strs = vm
            .seq_words(seq)
            .iter()
            .all(|&w| Value::from_bits(w).is_some_and(Vm::is_str));
        vm.burn(n as u64 / 8);
        if strs {
            return join_big(vm, sep, seq);
        }
    }
    let items = match vm.collect(vm.arg_o(&a, 1)) {
        Ok(items) => items,
        Err(_) if !vm.slow && vm.exc_type(vm.exc) == T::TypeError => {
            return Err(vm.err(T::TypeError, "can only join an iterable"));
        }
        Err(e) => return Err(e),
    };
    // One str is the result itself, as in CPython: no copy of a long one.
    if let [x] = items[..]
        && Vm::is_str(x)
    {
        return Ok(x);
    }
    let mut bytes = vm
        .str_bytes(sep)
        .len()
        .saturating_mul(items.len().saturating_sub(1));
    for x in &items {
        if !Vm::is_str(*x) {
            return join_values(vm, sep, &items);
        }
        bytes = bytes.saturating_add(vm.str_bytes(*x).len());
    }
    if (bytes / UNIT + 4 * items.len()) as u64 >= BIG {
        let items = vm.alloc_tuple(&items)?;
        return join_big(vm, sep, items);
    }
    let result = vm.build_text(bytes, |vm, out| {
        for (i, x) in items.iter().enumerate() {
            if i > 0 {
                vm.sbuf_push(out, sep);
            }
            vm.sbuf_push(out, *x);
        }
        Ok(())
    });
    // The copy is charged by the result's constructor (`str_from_buf`).
    result
}

/// `sep.join(items)` when an item is not an exact str: a str subclass
/// (StrEnum members) joins as its value; anything else is the TypeError.
#[cold]
#[inline(never)]
fn join_values(vm: &mut Vm, sep: Value, items: &[Value]) -> Result<Value, Raised> {
    // Sized first, as the exact-str path is: items can repeat one long
    // string, so the result can be any multiple of the arena it reads.
    let mut bytes = vm
        .str_bytes(sep)
        .len()
        .saturating_mul(items.len().saturating_sub(1));
    for (i, x) in items.iter().enumerate() {
        let v = vm.unbx(*x);
        if !Vm::is_str(v) {
            return Err(vm.err(
                T::TypeError,
                &format!(
                    "sequence item {i}: expected str instance, {} found",
                    vm.type_name(*x)
                ),
            ));
        }
        bytes = bytes.saturating_add(vm.str_bytes(v).len());
    }
    vm.build_text(bytes, |vm, out| {
        for (i, x) in items.iter().enumerate() {
            if i > 0 {
                vm.sbuf_push(out, sep);
            }
            vm.sbuf_push(out, vm.unbx(*x));
        }
        Ok(())
    })
}

fn maxsplit(vm: &mut Vm, v: Value) -> Result<i64, Raised> {
    match opt(v) {
        None => Ok(-1),
        Some(x) => vm.int_arg(x, "maxsplit"),
    }
}

pub(crate) fn split(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let s = this_value(vm, &a, "split")?;
    let max = maxsplit(vm, vm.arg_o(&a, 2))?;
    let max = usize::try_from(max).unwrap_or(usize::MAX);
    let sep = match opt(vm.arg_o(&a, 1)) {
        None => None,
        Some(sep) => {
            let sep = str_arg_value(vm, sep, "sep")?;
            if vm.str_bytes(sep).is_empty() {
                return Err(vm.err(T::ValueError, "empty separator"));
            }
            Some(sep)
        }
    };
    if let Some(sep) = sep
        && let Some(r) = split_long(vm, s, sep, max, false, None)
    {
        return r;
    }
    // Long, or more pieces than a step makes: a step at a time.
    let cap = (BIG / 4) as usize;
    let per = if sep.is_some() { UNIT } else { CHAR_UNIT };
    let mode = if sep.is_some() { sm::SEP } else { sm::SPACE };
    if big_text(vm, s, per) {
        return split_big(vm, s, sep.unwrap_or(Value::NONE), max, mode);
    }
    let (hay, heap) = (vm.str_ref_o(s), &vm.heap);
    let mut out = Ranges::default();
    let mut full = false;
    match sep {
        None => {
            let ws = |t: &str| t.len() - t.trim_start_matches(is_space).len();
            let mut pos = ws(hay);
            while pos < hay.len() {
                let rest = &hay[pos..];
                if out.len() >= cap {
                    break;
                }
                let (piece, next) = if out.len() >= max {
                    ((pos, hay.len()), hay.len())
                } else if let Some(k) = rest.find(is_space) {
                    ((pos, pos + k), pos + k + ws(&rest[k..]))
                } else {
                    ((pos, hay.len()), hay.len())
                };
                if out.push_hot(heap, piece).is_err() {
                    full = true;
                    break;
                }
                pos = next;
            }
        }
        Some(sep) => {
            let pat = vm.str_ref_o(sep);
            let mut last = 0;
            each_match(hay, pat, max.min(cap), &mut |i| {
                full |= out.push_hot(heap, (last, i)).is_err();
                last = i + pat.len();
            });
            full |= out.push_hot(heap, (last, hay.len())).is_err();
        }
    }
    if full {
        return Err(vm.err(T::MemoryError, ""));
    }
    if out.len() >= cap && max >= cap {
        return split_big(vm, s, sep.unwrap_or(Value::NONE), max, mode);
    }
    vm.burn(vm.str_bytes(s).len() as u64 / 64);
    list_of_ranges(vm, s, &out, sep.is_some() || max > 0)
}

pub(crate) fn rsplit(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let sv = this_value(vm, &a, "rsplit")?;
    vm.burn_bytes(vm.str_bytes(sv).len());
    let max = maxsplit(vm, vm.arg_o(&a, 2))?;
    let max = usize::try_from(max).unwrap_or(usize::MAX);
    let sep = match opt(vm.arg_o(&a, 1)) {
        None => None,
        Some(sep) => {
            let sep = str_arg_value(vm, sep, "sep")?;
            vm.burn_bytes(vm.str_bytes(sep).len());
            if vm.str_bytes(sep).is_empty() {
                return Err(vm.err(T::ValueError, "empty separator"));
            }
            Some(sep)
        }
    };
    if let Some(sep) = sep
        && let Some(r) = split_long(vm, sv, sep, max, true, None)
    {
        return r;
    }
    let cap = (BIG / 4) as usize;
    let per = if sep.is_some() { UNIT } else { CHAR_UNIT };
    let mode = if sep.is_some() { sm::RSEP } else { sm::RSPACE };
    if big_text(vm, sv, per) {
        return split_big(vm, sv, sep.unwrap_or(Value::NONE), max, mode);
    }
    let (s, heap) = (vm.str_ref_o(sv), &vm.heap);
    let mut out = Ranges::default();
    let mut full = false;
    // Pieces from the right, as offsets into `s`.
    let mut put = |piece: &str| {
        let at = piece.as_ptr() as usize - s.as_ptr() as usize;
        full |= out.push_hot(heap, (at, at + piece.len())).is_err();
        !full && out.len() < cap
    };
    match sep {
        None => {
            let mut rest = s.trim_end_matches(is_space);
            let mut n = 0;
            while !rest.is_empty() {
                if n >= max {
                    put(rest);
                    break;
                }
                n += 1;
                if let Some(k) = rest.rfind(is_space) {
                    let w = rest[k..].chars().next().map_or(1, char::len_utf8);
                    if !put(&rest[k + w..]) {
                        break;
                    }
                    rest = rest[..k].trim_end_matches(is_space);
                } else {
                    put(rest);
                    rest = "";
                }
            }
        }
        Some(sep) => {
            let sep = vm.str_ref_o(sep);
            if sep.len() > s.len() {
                // As `find_at`: no room for it, so not searched for.
                put(s);
            } else {
                for piece in s.rsplitn(max.saturating_add(1), sep) {
                    if !put(piece) {
                        break;
                    }
                }
            }
        }
    }
    if full {
        return Err(vm.err(T::MemoryError, ""));
    }
    if out.len() >= cap && max >= cap {
        return split_big(vm, sv, sep.unwrap_or(Value::NONE), max, mode);
    }
    out.reverse();
    list_of_ranges(vm, sv, &out, sep.is_some() || max > 0)
}

fn strip_impl(
    vm: &mut Vm,
    a: &CallArgs,
    name: &str,
    left: bool,
    right: bool,
) -> Result<Value, Raised> {
    let s = this_value(vm, a, name)?;
    let set: Option<Vec<char>> = match opt(vm.arg_o(a, 1)) {
        None => None,
        Some(c) => Some(str_arg(vm, c, "strip arg")?.chars().collect()),
    };
    if big_text(vm, s, CHAR_UNIT) {
        let chars = match &set {
            Some(cs) => vm.new_str(&cs.iter().collect::<String>())?,
            None => Value::NONE,
        };
        let kind = Value::int(sk::STRIP).unwrap_or(Value::NONE);
        let n = vm.str_bytes(s).len() as u64;
        let w = [0, n, u64::from(left) | u64::from(right) << 1, 0, 0, 0, 0];
        return walk(vm, op::STR_SCAN, [s, chars, kind, Value::NONE], w);
    }
    let (at, end) = strip_span(
        vm.str_bytes(s),
        vm.str_ascii(s),
        set.as_deref(),
        left,
        right,
    );
    let n = vm.str_bytes(s).len();
    vm.burn_bytes(n);
    if let Some(set) = &set {
        // Each stripped char was looked up in `set`.
        vm.burn_bytes((n - (end - at)).saturating_mul(set.len()));
    }
    slice_steps(vm, s, at, end)
}

/// The char at byte `i` of valid UTF-8 `h` and the byte after it.
fn char_at(h: &[u8], i: usize) -> Option<(char, usize)> {
    let e = ceil_char(h, i + 1);
    let w = h.get(i..e)?;
    let c = match w {
        [b] => char::from(*b),
        _ => core::str::from_utf8(w).ok()?.chars().next()?,
    };
    Some((c, e))
}

/// The bytes `h` keeps once its ends lose the chars in `set` (whitespace
/// when `None`): `(start, end)`.
fn strip_span(
    h: &[u8],
    ascii: bool,
    set: Option<&[char]>,
    left: bool,
    right: bool,
) -> (usize, usize) {
    let strip = |c: char| set.map_or_else(|| is_space(c), |set| set.contains(&c));
    let (mut at, mut end) = (0, h.len());
    let step = |i: usize| -> Option<(char, usize)> {
        if ascii {
            h.get(i).map(|&b| (char::from(b), i + 1))
        } else {
            char_at(h, i)
        }
    };
    while left && at < end {
        match step(at) {
            Some((c, e)) if strip(c) => at = e,
            _ => break,
        }
    }
    while right && end > at {
        let b = if ascii {
            end - 1
        } else {
            floor_char(h, end - 1)
        };
        match step(b) {
            Some((c, _)) if strip(c) => end = b,
            _ => break,
        }
    }
    (at, end)
}

pub(crate) fn strip(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    strip_impl(vm, &a, "strip", true, true)
}

pub(crate) fn lstrip(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    strip_impl(vm, &a, "lstrip", true, false)
}

pub(crate) fn rstrip(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    strip_impl(vm, &a, "rstrip", false, true)
}

/// Char-index window `[start, end)` of `str` `s` as byte offsets.
fn window(vm: &mut Vm, s: Value, start: Value, end: Value) -> Result<(usize, usize), Raised> {
    let n = i64::try_from(vm.str_len(s)).unwrap_or(0);
    let fix = |vm: &mut Vm, v: Value, d: i64| -> Result<i64, Raised> {
        let i = crate::bytes::search_bound(vm, v, d)?;
        Ok(if i < 0 { (i + n).max(0) } else { i.min(n) })
    };
    let raw = crate::bytes::search_bound(vm, start, 0)?;
    let st = fix(vm, start, 0)?;
    let en = fix(vm, end, n)?;
    if raw > n {
        // A start past the end finds nothing, not even ''.
        return Ok((usize::MAX, 0));
    }
    if st > en {
        return Ok((usize::MAX, 0));
    }
    let from = vm.str_offset(s, usize::try_from(st).unwrap_or(0));
    Ok((from, vm.str_offset(s, usize::try_from(en).unwrap_or(0))))
}

fn affix(vm: &mut Vm, a: &CallArgs, name: &str, prefix: bool) -> Result<Value, Raised> {
    let s = this_value(vm, a, name)?;
    // A byte window: the whole string needs no copy or char count.
    let (st, en) = if opt(vm.arg_o(a, 2)).is_none() && opt(vm.arg_o(a, 3)).is_none() {
        (0, vm.str_bytes(s).len())
    } else {
        window(vm, s, vm.arg_o(a, 2), vm.arg_o(a, 3))?
    };
    let pat = vm.arg_o(a, 1);
    let tuple = pat.is_obj_kind(ty::TUPLE as u8);
    let pats: Vec<Value> = if tuple {
        vm.tuple_items(pat)
    } else {
        alloc::vec![pat]
    };
    // Elements are checked in order, up to the first match; each is
    // charged as byte work, and a long tuple one unit per element.
    if pats.len() >= 8 {
        vm.burn(pats.len() as u64);
    }
    for p in pats {
        let p = if Vm::is_str(p) { p } else { vm.unbx(p) };
        if !Vm::is_str(p) {
            let tn = vm.type_name(p);
            let msg = if tuple {
                format!("tuple for {name} must only contain str, not {tn}")
            } else {
                format!("{name} first arg must be str or a tuple of str, not {tn}")
            };
            return Err(vm.err(T::TypeError, &msg));
        }
        // An empty window (a start past the end) matches nothing.
        let hay = (st != usize::MAX).then(|| vm.str_bytes(s).get(st..en).unwrap_or(&[]));
        let ps = vm.str_bytes(p);
        let n = ps.len();
        let hit =
            hay.is_some_and(|h| (prefix && h.starts_with(ps)) || (!prefix && h.ends_with(ps)));
        vm.burn_bytes(n);
        if hit {
            return Ok(Value::TRUE);
        }
    }
    Ok(Value::FALSE)
}

pub(crate) fn startswith(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    affix(vm, &a, "startswith", true)
}

pub(crate) fn endswith(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    affix(vm, &a, "endswith", false)
}

pub(crate) fn replace(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let s = this_value(vm, &a, "replace")?;
    let old = str_arg_value(vm, vm.arg_o(&a, 1), "replace() argument 1")?;
    let new = str_arg_value(vm, vm.arg_o(&a, 2), "replace() argument 2")?;
    let count = match opt(vm.arg_o(&a, 3)) {
        None => usize::MAX,
        Some(c) => usize::try_from(vm.int_arg(c, "count")?).unwrap_or(usize::MAX),
    };
    // Nothing to replace: the receiver itself, as CPython gives it.
    if count == 0 || old == new {
        return Ok(s);
    }
    if let Some(r) = split_long(vm, s, old, count, false, Some(new)) {
        return r;
    }
    // A long text, or more matches than a step copies, goes a step at a
    // time (docs/spec.md §5).
    let cap = (BIG / 4) as usize;
    let len = vm.str_bytes(s).len();
    if (len / UNIT) as u64 >= BIG / 2 || vm.str_bytes(old).is_empty() && count.min(len) >= cap {
        return replace_big(vm, s, old, new, count);
    }
    let (hay, pat, rep) = (vm.str_ref_o(s), vm.str_ref_o(old), vm.str_ref_o(new));
    let matches = if pat.is_empty() {
        count.min(char_len(hay).saturating_add(1))
    } else {
        each_match(hay, pat, count.min(cap), &mut |_| {})
    };
    if matches == cap && count > cap {
        return replace_big(vm, s, old, new, count);
    }
    if matches == 0 {
        vm.burn_bytes(len);
        return Ok(s);
    }
    let bytes = hay
        .len()
        .saturating_sub(pat.len().saturating_mul(matches))
        .saturating_add(rep.len().saturating_mul(matches));
    let result = vm.build_text(bytes, |vm, out| {
        let (hay, pat, rep) = (vm.str_ref_o(s), vm.str_ref_o(old), vm.str_ref_o(new));
        if pat.is_empty() {
            let mut k = 0;
            for c in hay.chars() {
                if k < count {
                    out.push_str(rep);
                    k += 1;
                }
                out.push(c);
            }
            if k < count {
                out.push_str(rep);
            }
        } else {
            let mut last = 0;
            each_match(hay, pat, count, &mut |i| {
                out.push_str(&hay[last..i]);
                out.push_str(rep);
                last = i + pat.len();
            });
            out.push_str(&hay[last..]);
        }
        Ok(())
    });
    // The two scans as byte work. Each match is found twice and copied in
    // two pieces: about 55 ns, so four units, beyond the byte charges (10M
    // matches charged 6 ms).
    vm.burn_bytes(2 * len);
    vm.burn(bytes as u64 / 64 + (matches as u64).saturating_mul(4));
    result
}

/// A case map, `translate` or `expandtabs` of a long receiver, a step at
/// a time (`None` for a short one, or not a str).
fn big_map(vm: &mut Vm, a: &CallArgs, kind: i64, arg: Value) -> Option<Result<Value, Raised>> {
    let s = vm.arg_o(a, 0);
    let n = vm.str_bytes(s).len();
    if (n as u64) < BIG {
        return None;
    }
    let per = if kind <= mk::CASEFOLD && vm.str_ascii(s) {
        CHAR_UNIT
    } else {
        1
    };
    ((n / per) as u64 >= BIG).then(|| map_big(vm, s, kind, arg))
}

/// Admits a case mapping of `s` before it is built: the result is at most
/// each char's longer mapping (up to three times the text), and `lowered`
/// adds `str::to_lowercase`'s copy, which can grow by doubling.
fn case_room(vm: &mut Vm, s: &str, lowered: bool) -> Result<(usize, Scratch), Raised> {
    let (mut most, mut low) = (0usize, 0usize);
    for c in s.chars() {
        let u: usize = c.to_uppercase().map(char::len_utf8).sum();
        let l: usize = c.to_lowercase().map(char::len_utf8).sum();
        most = most.saturating_add(u.max(l));
        low = low.saturating_add(l);
    }
    let bytes = if lowered {
        most.saturating_add(low.saturating_mul(2))
    } else {
        most
    };
    let res = vm.heap.reserve_scratch(bytes);
    Ok((most, vm.hx_o(res)?))
}

pub(crate) fn upper(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    if let Some(v) = big_map(vm, &a, mk::UPPER, Value::NONE) {
        return v;
    }
    let s = this(vm, &a, "upper")?;
    let (room, _native) = case_room(vm, &s, false)?;
    let mut out = String::with_capacity(room);
    for c in s.chars() {
        if case_from_17(c) {
            out.push(c);
        } else {
            out.extend(c.to_uppercase());
        }
    }
    vm.new_str(&out)
}

pub(crate) fn lower(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    if let Some(v) = big_map(vm, &a, mk::LOWER, Value::NONE) {
        return v;
    }
    let s = this(vm, &a, "lower")?;
    let (room, _native) = case_room(vm, &s, true)?;
    if !s.is_ascii() && s.chars().any(case_from_17) {
        return vm.new_str(&recase(&s, room, &mut |_, _| Case::Lower));
    }
    vm.new_str(&s.to_lowercase())
}

pub(crate) fn swapcase(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    if let Some(v) = big_map(vm, &a, mk::SWAPCASE, Value::NONE) {
        return v;
    }
    let s = this(vm, &a, "swapcase")?;
    let (room, _native) = case_room(vm, &s, true)?;
    let out = recase(&s, room, &mut |_, c| {
        if is_upper(c) {
            Case::Lower
        } else if is_lower(c) {
            Case::Upper
        } else {
            Case::Keep
        }
    });
    vm.new_str(&out)
}

/// What a search gives (`op::STR_SCAN` kinds `FIND` and `RFIND`, raw
/// word 2): `find`'s index or -1, `index`'s or `ValueError`, or `in`'s or
/// `not in`'s bool.
mod fm {
    pub const FIND: u64 = 0;
    pub const INDEX: u64 = 1;
    pub const IN: u64 = 2;
    pub const NOT_IN: u64 = 3;
}

/// `find`, `rfind` (`rev`), `index` and `rindex`: a long window is
/// searched a step at a time (`scan_find`).
fn find_impl(vm: &mut Vm, a: &CallArgs, name: &str, rev: bool, mode: u64) -> Result<Value, Raised> {
    let s = this_value(vm, a, name)?;
    let sub = str_arg_value(vm, vm.arg_o(a, 1), &format!("{name}() argument 1"))?;
    let (st, en) = window(vm, s, vm.arg_o(a, 2), vm.arg_o(a, 3))?;
    if st == usize::MAX || st > en {
        return found(vm, s, None, mode);
    }
    if let Some(r) = long_find(vm, s, sub, (st, en), Some(mode), rev) {
        return r;
    }
    let empty = vm.str_bytes(sub).is_empty();
    if !empty && (en - st) as u64 >= BIG {
        let kind = if rev { sk::RFIND } else { sk::FIND };
        return scan_find(vm, s, sub, kind, st, en, mode);
    }
    // The search is byte work over the window (docs/spec.md §5).
    vm.burn_bytes(en - st);
    // Only the window is read, and a short pattern is matched as bytes
    // (a UTF-8 match is a char-boundary match) without checking the
    // window's UTF-8, so `s.find(x, i)` along a string stays linear.
    let hay = vm.str_bytes(s).get(st..en).unwrap_or(&[]);
    let p = vm.str_bytes(sub);
    let at = if empty {
        Some(if rev { hay.len() } else { 0 })
    } else if rev {
        rfind_in(hay, 0, hay.len(), p)
    } else {
        find_in(hay, 0, hay.len(), p)
    };
    found(vm, s, at.map(|b| st + b), mode)
}

/// `find`'s value (`index`'s when `index`) for the match at byte offset
/// `at` of `s`: of a [`crate::bytes::find_job`] over a str.
pub(crate) fn found_char(
    vm: &mut Vm,
    s: Value,
    at: Option<usize>,
    index: bool,
) -> Result<Value, Raised> {
    found(vm, s, at, if index { fm::INDEX } else { fm::FIND })
}

/// A search (`fm` mode, or `count` when `None`) for a needle of [`BIG`]
/// bytes or more in bytes `st..en` of `s`: the bytes search, a step at a
/// time, keeps the needle's factorization between steps, where the
/// window search reads the whole needle again in each (a 16M-char needle
/// was 73 ms a step [M]). `None` for a shorter needle.
fn long_find(
    vm: &mut Vm,
    s: Value,
    sub: Value,
    w: (usize, usize),
    mode: Option<u64>,
    rev: bool,
) -> Option<Result<Value, Raised>> {
    use crate::bytes::found;
    if (vm.str_bytes(sub).len() as u64) < BIG {
        return None;
    }
    let mode = match mode {
        None => found::COUNT,
        Some(fm::INDEX) => found::INDEX,
        Some(fm::IN) => found::IN,
        Some(fm::NOT_IN) => found::NOT_IN,
        Some(_) => found::FIND,
    };
    let s = vm.unbx(s);
    Some(crate::bytes::find_job(vm, s, sub, w, mode, rev))
}

/// `split` (`rsplit` when `right`, `replace` by `new`) of `s` at `sep`,
/// at most `max` times, when `sep` is [`BIG`] bytes or more and fits in
/// `s`: the prelude twin, whose searches are [`long_find`]s.
fn split_long(
    vm: &mut Vm,
    s: Value,
    sep: Value,
    max: usize,
    right: bool,
    new: Option<Value>,
) -> Option<Result<Value, Raised>> {
    let n = vm.str_bytes(sep).len();
    if (n as u64) < BIG {
        return None;
    }
    let s = vm.unbx(s);
    if n > vm.str_bytes(s).len() {
        return None;
    }
    let max = match vm.new_int(i64::try_from(max).unwrap_or(-1)) {
        Ok(m) => m,
        Err(e) => return Some(Err(e)),
    };
    Some(match new {
        Some(new) => vm.prelude_request(
            "_str_replace_long",
            alloc::vec![s, sep, new, max],
            crate::cont::Cont::None,
        ),
        None => vm.prelude_request(
            "_str_split_long",
            alloc::vec![s, sep, max, Value::from_bool(right)],
            crate::cont::Cont::None,
        ),
    })
}

/// A search's value (`fm`) for the match at byte offset `at` of `s`.
fn found(vm: &mut Vm, s: Value, at: Option<usize>, mode: u64) -> Result<Value, Raised> {
    match (at, mode) {
        (_, fm::IN | fm::NOT_IN) => Ok(Value::from_bool(at.is_some() == (mode == fm::IN))),
        (Some(b), _) => {
            let i = vm.str_index(s, b);
            vm.new_int(i64::try_from(i).unwrap_or(-1))
        }
        (None, fm::INDEX) => Err(vm.err(T::ValueError, "substring not found")),
        (None, _) => vm.new_int(-1),
    }
}

/// `x in s` (`not in` with `neg`) for a long str `s`: searched a step at
/// a time; out of line, as the `in` operator is on the dispatch loop's
/// path. `None` when `x` is not a str (the usual `in` raises).
#[inline(never)]
pub(crate) fn str_in(vm: &mut Vm, s: Value, x: Value, neg: bool) -> Option<Result<Value, Raised>> {
    if !Vm::is_str(x) {
        return None;
    }
    if vm.str_bytes(x).is_empty() {
        return Some(Ok(Value::from_bool(!neg)));
    }
    let n = vm.str_bytes(s).len();
    let mode = if neg { fm::NOT_IN } else { fm::IN };
    if let Some(r) = long_find(vm, s, x, (0, n), Some(mode), false) {
        return Some(r);
    }
    Some(scan_find(vm, s, x, sk::FIND, 0, n, mode))
}

/// Whether `in` over str `s` searches in steps ([`str_in`]).
pub(crate) fn str_in_long(vm: &Vm, s: Value) -> bool {
    vm.str_bytes(s).len() as u64 >= BIG
}

pub(crate) fn find(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    find_impl(vm, &a, "find", false, fm::FIND)
}

pub(crate) fn rfind(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    find_impl(vm, &a, "rfind", true, fm::FIND)
}

pub(crate) fn index(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    find_impl(vm, &a, "index", false, fm::INDEX)
}

pub(crate) fn rindex(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    find_impl(vm, &a, "rindex", true, fm::INDEX)
}

pub(crate) fn count(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    if opt(vm.arg_o(&a, 2)).is_none() && opt(vm.arg_o(&a, 3)).is_none() {
        let s = this_value(vm, &a, "count")?;
        let sub = str_arg_value(vm, vm.arg_o(&a, 1), "count() argument 1")?;
        let len = vm.str_bytes(s).len();
        if let Some(r) = long_find(vm, s, sub, (0, len), None, false) {
            return r;
        }
        let cap = (BIG / 4) as usize;
        if (len / UNIT) as u64 >= BIG / 2 && !vm.str_bytes(sub).is_empty() {
            return scan_big(vm, s, sub, sk::COUNT, 0, len);
        }
        let (hay, pat) = (vm.str_ref_o(s), vm.str_ref_o(sub));
        let n = if pat.is_empty() {
            vm.str_len(s) + 1
        } else {
            each_match(hay, pat, cap, &mut |_| {})
        };
        if n == cap {
            return scan_big(vm, s, sub, sk::COUNT, 0, len);
        }
        vm.burn(hay.len() as u64 / 64);
        return vm.new_int(i64::try_from(n).unwrap_or(0));
    }
    let s = this_value(vm, &a, "count")?;
    let subv = str_arg_value(vm, vm.arg_o(&a, 1), "count() argument 1")?;
    let (st, en) = window(vm, s, vm.arg_o(&a, 2), vm.arg_o(&a, 3))?;
    if st == usize::MAX {
        return vm.new_int(0);
    }
    if let Some(r) = long_find(vm, s, subv, (st, en.max(st)), None, false) {
        return r;
    }
    if (en.saturating_sub(st) / UNIT) as u64 >= BIG / 2 && !vm.str_bytes(subv).is_empty() {
        return scan_big(vm, s, subv, sk::COUNT, st, en);
    }
    let sub = String::from(vm.str_ref_o(subv));
    let hay = vm.str_bytes(s).get(st..en).unwrap_or(&[]);
    let hay = core::str::from_utf8(hay).unwrap_or("");
    let n = if sub.is_empty() {
        char_len(hay) + 1
    } else {
        each_match(hay, &sub, usize::MAX, &mut |_| {})
    };
    // The window's bytes, as the count without bounds charges them.
    vm.burn_bytes(hay.len());
    vm.new_int(i64::try_from(n).unwrap_or(0))
}

/// Predicate `kind` over a long receiver, a step at a time (`None` for a
/// short one, or not a str).
fn big_scan(vm: &mut Vm, a: &CallArgs, kind: i64) -> Option<Result<Value, Raised>> {
    let s = vm.arg_o(a, 0);
    let n = vm.str_bytes(s).len();
    ((n / CHAR_UNIT) as u64 >= BIG).then(|| scan_big(vm, s, s, kind, 0, n))
}

fn pred(
    vm: &mut Vm,
    a: &CallArgs,
    name: &str,
    kind: i64,
    f: fn(char) -> bool,
) -> Result<Value, Raised> {
    let v = this_value(vm, a, name)?;
    // One look at the text: short ones are tested at once.
    let b = vm.str_bytes(v);
    let n = b.len();
    if (n / CHAR_UNIT) as u64 >= BIG {
        return scan_big(vm, v, v, kind, 0, n);
    }
    let r = !b.is_empty() && core::str::from_utf8(b).is_ok_and(|s| s.chars().all(f));
    vm.burn_bytes(n);
    Ok(Value::from_bool(r))
}

pub(crate) fn isdigit(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    pred(vm, &a, "isdigit", sk::DIGIT, crate::text::is_digit)
}

pub(crate) fn isdecimal(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    pred(vm, &a, "isdecimal", sk::DECIMAL, |c| {
        crate::text::is_decimal(c as u32)
    })
}

pub(crate) fn isnumeric(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    pred(vm, &a, "isnumeric", sk::NUMERIC, crate::text::is_numeric)
}

pub(crate) fn isalpha(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    pred(vm, &a, "isalpha", sk::ALPHA, ucd::alpha)
}

/// `str.isidentifier()`: XID_Start (or `_`) then XID_Continue.
pub(crate) fn isidentifier(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    if let Some(v) = big_scan(vm, &a, sk::IDENTIFIER) {
        return v;
    }
    let s = this(vm, &a, "isidentifier")?;
    let mut cs = s.chars();
    let start = cs.next().is_some_and(|c| c == '_' || ucd::xid_start(c));
    Ok(Value::from_bool(
        start && cs.all(|c| c == '_' || ucd::xid_continue(c)),
    ))
}

pub(crate) fn isalnum(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    pred(vm, &a, "isalnum", sk::ALNUM, ucd::alnum)
}

pub(crate) fn isspace(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    pred(vm, &a, "isspace", sk::SPACE, is_space)
}

pub(crate) fn isprintable(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    if let Some(v) = big_scan(vm, &a, sk::PRINTABLE) {
        return v;
    }
    let s = this(vm, &a, "isprintable")?;
    Ok(Value::from_bool(s.chars().all(crate::text::is_printable)))
}

pub(crate) fn isascii(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let s = this_value(vm, &a, "isascii")?;
    Ok(Value::from_bool(vm.str_ascii(s)))
}

/// CPython's `istitle`: cased runs start upper or title case, then lower.
pub(crate) fn istitle(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    if let Some(v) = big_scan(vm, &a, sk::TITLE) {
        return v;
    }
    let s = this(vm, &a, "istitle")?;
    let (mut cased, mut prev) = (false, false);
    for c in s.chars() {
        if is_upper(c) || is_title(c) {
            if prev {
                return Ok(Value::FALSE);
            }
            (prev, cased) = (true, true);
        } else if is_lower(c) {
            if !prev {
                return Ok(Value::FALSE);
            }
            (prev, cased) = (true, true);
        } else {
            prev = false;
        }
    }
    Ok(Value::from_bool(cased))
}

fn cased(vm: &mut Vm, a: &CallArgs, name: &str, upper: bool) -> Result<Value, Raised> {
    if let Some(v) = big_scan(vm, a, if upper { sk::UPPER } else { sk::LOWER }) {
        return v;
    }
    let s = this(vm, a, name)?;
    let mut any = false;
    for c in s.chars() {
        if is_title(c) {
            return Ok(Value::FALSE);
        }
        if is_upper(c) || is_lower(c) {
            any = true;
            if is_upper(c) != upper {
                return Ok(Value::FALSE);
            }
        }
    }
    Ok(Value::from_bool(any))
}

pub(crate) fn isupper(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    cased(vm, &a, "isupper", true)
}

pub(crate) fn islower(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    cased(vm, &a, "islower", false)
}

pub(crate) fn title(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    if let Some(v) = big_map(vm, &a, mk::TITLE, Value::NONE) {
        return v;
    }
    let s = this(vm, &a, "title")?;
    let (room, _native) = case_room(vm, &s, true)?;
    let mut prev_cased = false;
    let out = recase(&s, room, &mut |_, c| {
        let case = if prev_cased { Case::Lower } else { Case::Title };
        // Titlecase letters are cased too, even though neither predicate
        // includes them. Uncased letters, such as Chinese, end a word.
        prev_cased = is_lower(c) || is_upper(c) || is_title(c);
        case
    });
    vm.new_str(&out)
}

pub(crate) fn capitalize(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    if let Some(v) = big_map(vm, &a, mk::CAPITALIZE, Value::NONE) {
        return v;
    }
    let s = this(vm, &a, "capitalize")?;
    let (room, _native) = case_room(vm, &s, true)?;
    let out = recase(&s, room, &mut |i, _| {
        if i == 0 { Case::Title } else { Case::Lower }
    });
    vm.new_str(&out)
}

pub(crate) fn casefold(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    if let Some(v) = big_map(vm, &a, mk::CASEFOLD, Value::NONE) {
        return v;
    }
    let s = this(vm, &a, "casefold")?;
    let (room, _native) = case_room(vm, &s, false)?;
    let mut out = String::with_capacity(room);
    for c in s.chars() {
        if case_from_17(c) {
            out.push(c);
        } else if !ucd::fold(c, &mut out) {
            out.extend(c.to_lowercase());
        }
    }
    vm.new_str(&out)
}

enum Case {
    Lower,
    Upper,
    Title,
    Keep,
}

/// `s` with each character's case changed as `pick` says, given its
/// index. Lowering takes the character's part of `str::to_lowercase`, so
/// `Σ`'s Final_Sigma context is decided over the whole string, as in
/// CPython's `lower_ucs4`.
fn recase(s: &str, room: usize, pick: &mut dyn FnMut(usize, char) -> Case) -> String {
    let low = s.to_lowercase();
    let mut low = low.chars();
    let mut out = String::with_capacity(room);
    for (i, c) in s.chars().enumerate() {
        let lowered = low.by_ref().take(c.to_lowercase().len());
        let case = pick(i, c);
        match if case_from_17(c) { Case::Keep } else { case } {
            Case::Lower => out.extend(lowered),
            case => {
                lowered.for_each(drop);
                match case {
                    Case::Keep => out.push(c),
                    Case::Title if ucd::title(c, &mut out) => {}
                    _ => out.extend(c.to_uppercase()),
                }
            }
        }
    }
    out
}

/// CPython's `Py_UNICODE_ISSPACE`: White_Space and U+001C..U+001F.
fn is_space(c: char) -> bool {
    c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c)
}

/// Code points `core`'s case tables (Unicode 17 in Rust 1.99) case but
/// Unicode 16, CPython 3.14's, leaves unassigned: no case and no case
/// mapping here. `case_tables_are_unicode_16` checks the lists.
fn uncased_in_16(c: char) -> bool {
    matches!(
        c,
        '\u{a7ce}' | '\u{a7cf}' | '\u{a7d2}' | '\u{a7d4}' | '\u{a7f1}' | '\u{16ea0}'..='\u{16ed3}'
    )
}

/// Code points whose case mappings `core` takes from Unicode 17: those
/// above, and U+A7D3 and U+A7D5, whose uppercase is new in 17. They map
/// to themselves here.
fn case_from_17(c: char) -> bool {
    uncased_in_16(c) || matches!(c, '\u{a7d3}' | '\u{a7d5}')
}

/// CPython's `Py_UNICODE_ISLOWER` (Lowercase in Unicode 16): U+0295 is
/// Ll there and Lo in Unicode 17.
fn is_lower(c: char) -> bool {
    (c.is_lowercase() && !uncased_in_16(c)) || c == '\u{295}'
}

/// CPython's `Py_UNICODE_ISUPPER` (Uppercase in Unicode 16).
fn is_upper(c: char) -> bool {
    c.is_uppercase() && !uncased_in_16(c)
}

/// A titlecase letter (Lt), all 31 of them in Unicode 16.
fn is_title(c: char) -> bool {
    matches!(
        c,
        '\u{1c5}'
            | '\u{1c8}'
            | '\u{1cb}'
            | '\u{1f2}'
            | '\u{1f88}'..='\u{1f8f}'
            | '\u{1f98}'..='\u{1f9f}'
            | '\u{1fa8}'..='\u{1faf}'
            | '\u{1fbc}'
            | '\u{1fcc}'
            | '\u{1ffc}'
    )
}

/// Character classes CPython takes from its Unicode database: exact with
/// the `unicodedata` tables, approximated from `core`'s without them.
#[cfg(feature = "unicodedata")]
mod ucd {
    use crate::unicodedata::category_of;
    use alloc::string::String;

    /// `str.isalpha`: Lu, Ll, Lt, Lm, Lo.
    pub(super) fn alpha(c: char) -> bool {
        category_of(u32::from(c)).starts_with('L')
    }

    pub(super) fn alnum(c: char) -> bool {
        alpha(c) || crate::text::is_numeric(c)
    }

    /// XID_Start: letters, Nl and Other_ID_Start, less what NFKC maps out.
    pub(super) fn xid_start(c: char) -> bool {
        let cat = category_of(u32::from(c));
        ((cat.starts_with('L') || cat == "Nl")
            && !matches!(
                c,
                '\u{37a}' | '\u{e33}' | '\u{eb3}' | '\u{2e2f}' | '\u{fc5e}'
                    ..='\u{fc63}'
                        | '\u{fdfa}'
                        | '\u{fdfb}'
                        | '\u{fe70}'
                        | '\u{fe72}'
                        | '\u{fe74}'
                        | '\u{fe76}'
                        | '\u{fe78}'
                        | '\u{fe7a}'
                        | '\u{fe7c}'
                        | '\u{fe7e}'
                        | '\u{ff9e}'
                        | '\u{ff9f}'
            ))
            || matches!(c, '\u{1885}' | '\u{1886}' | '\u{2118}' | '\u{212e}')
    }

    /// XID_Continue: XID_Start, marks, Nd, Pc and Other_ID_Continue.
    pub(super) fn xid_continue(c: char) -> bool {
        xid_start(c)
            || matches!(c, '\u{e33}' | '\u{eb3}' | '\u{ff9e}' | '\u{ff9f}')
            || (matches!(category_of(u32::from(c)), "Mn" | "Mc" | "Nd" | "Pc")
                && !matches!(c, '\u{fe70}'..='\u{fe7e}'))
            || matches!(
                c,
                '\u{b7}' | '\u{387}' | '\u{1369}'
                    ..='\u{1371}' | '\u{19da}' | '\u{200c}' | '\u{200d}' | '\u{30fb}' | '\u{ff65}'
            )
    }

    pub(super) fn title(c: char, out: &mut String) -> bool {
        special_case(c, TITLE_RANGES, TITLE_ONE, out)
    }

    pub(super) fn fold(c: char, out: &mut String) -> bool {
        special_case(c, FOLD_RANGES, FOLD_ONE, out)
    }

    /// Appends `c`'s mapping from a table of differences, if it has one.
    fn special_case(
        c: char,
        ranges: &[(u32, u32, i32)],
        one: &[(u32, &str)],
        out: &mut String,
    ) -> bool {
        let u = u32::from(c);
        if let Ok(i) = one.binary_search_by_key(&u, |e| e.0) {
            out.push_str(one[i].1);
            return true;
        }
        for &(first, last, delta) in ranges {
            if (first..=last).contains(&u)
                && let Some(m) = u.checked_add_signed(delta).and_then(char::from_u32)
            {
                out.push(m);
                return true;
            }
        }
        false
    }

    // Where CPython 3.14's per-character `str.title()` differs from
    // `char::to_uppercase` and `str.casefold()` from `char::to_lowercase`
    // (generated by comparing them over every code point).
    /// `title` ranges: (first, last, delta to add).
    const TITLE_RANGES: &[(u32, u32, i32)] = &[
        (0x10d0, 0x10fa, 0),
        (0x10fd, 0x10ff, 0),
        (0x1f80, 0x1f87, 8),
        (0x1f88, 0x1f8f, 0),
        (0x1f90, 0x1f97, 8),
        (0x1f98, 0x1f9f, 0),
        (0x1fa0, 0x1fa7, 8),
        (0x1fa8, 0x1faf, 0),
    ];
    const TITLE_ONE: &[(u32, &str)] = &[
        (0xdf, "Ss"),
        (0x1c4, "\u{1c5}"),
        (0x1c5, "\u{1c5}"),
        (0x1c6, "\u{1c5}"),
        (0x1c7, "\u{1c8}"),
        (0x1c8, "\u{1c8}"),
        (0x1c9, "\u{1c8}"),
        (0x1ca, "\u{1cb}"),
        (0x1cb, "\u{1cb}"),
        (0x1cc, "\u{1cb}"),
        (0x1f1, "\u{1f2}"),
        (0x1f2, "\u{1f2}"),
        (0x1f3, "\u{1f2}"),
        (0x587, "\u{535}\u{582}"),
        (0x1fb2, "\u{1fba}\u{345}"),
        (0x1fb3, "\u{1fbc}"),
        (0x1fb4, "\u{386}\u{345}"),
        (0x1fb7, "\u{391}\u{342}\u{345}"),
        (0x1fbc, "\u{1fbc}"),
        (0x1fc2, "\u{1fca}\u{345}"),
        (0x1fc3, "\u{1fcc}"),
        (0x1fc4, "\u{389}\u{345}"),
        (0x1fc7, "\u{397}\u{342}\u{345}"),
        (0x1fcc, "\u{1fcc}"),
        (0x1ff2, "\u{1ffa}\u{345}"),
        (0x1ff3, "\u{1ffc}"),
        (0x1ff4, "\u{38f}\u{345}"),
        (0x1ff7, "\u{3a9}\u{342}\u{345}"),
        (0x1ffc, "\u{1ffc}"),
        (0xfb00, "Ff"),
        (0xfb01, "Fi"),
        (0xfb02, "Fl"),
        (0xfb03, "Ffi"),
        (0xfb04, "Ffl"),
        (0xfb05, "St"),
        (0xfb06, "St"),
        (0xfb13, "\u{544}\u{576}"),
        (0xfb14, "\u{544}\u{565}"),
        (0xfb15, "\u{544}\u{56b}"),
        (0xfb16, "\u{54e}\u{576}"),
        (0xfb17, "\u{544}\u{56d}"),
    ];
    /// `fold` ranges: (first, last, delta to add).
    const FOLD_RANGES: &[(u32, u32, i32)] = &[
        (0x13a0, 0x13f5, 0),
        (0x13f8, 0x13fd, -8),
        (0x1c83, 0x1c84, -6210),
        (0xab70, 0xabbf, -38864),
    ];
    const FOLD_ONE: &[(u32, &str)] = &[
        (0xb5, "\u{3bc}"),
        (0xdf, "ss"),
        (0x149, "\u{2bc}n"),
        (0x17f, "s"),
        (0x1f0, "j\u{30c}"),
        (0x345, "\u{3b9}"),
        (0x390, "\u{3b9}\u{308}\u{301}"),
        (0x3b0, "\u{3c5}\u{308}\u{301}"),
        (0x3c2, "\u{3c3}"),
        (0x3d0, "\u{3b2}"),
        (0x3d1, "\u{3b8}"),
        (0x3d5, "\u{3c6}"),
        (0x3d6, "\u{3c0}"),
        (0x3f0, "\u{3ba}"),
        (0x3f1, "\u{3c1}"),
        (0x3f5, "\u{3b5}"),
        (0x587, "\u{565}\u{582}"),
        (0x1c80, "\u{432}"),
        (0x1c81, "\u{434}"),
        (0x1c82, "\u{43e}"),
        (0x1c85, "\u{442}"),
        (0x1c86, "\u{44a}"),
        (0x1c87, "\u{463}"),
        (0x1c88, "\u{a64b}"),
        (0x1e96, "h\u{331}"),
        (0x1e97, "t\u{308}"),
        (0x1e98, "w\u{30a}"),
        (0x1e99, "y\u{30a}"),
        (0x1e9a, "a\u{2be}"),
        (0x1e9b, "\u{1e61}"),
        (0x1e9e, "ss"),
        (0x1f50, "\u{3c5}\u{313}"),
        (0x1f52, "\u{3c5}\u{313}\u{300}"),
        (0x1f54, "\u{3c5}\u{313}\u{301}"),
        (0x1f56, "\u{3c5}\u{313}\u{342}"),
        (0x1f80, "\u{1f00}\u{3b9}"),
        (0x1f81, "\u{1f01}\u{3b9}"),
        (0x1f82, "\u{1f02}\u{3b9}"),
        (0x1f83, "\u{1f03}\u{3b9}"),
        (0x1f84, "\u{1f04}\u{3b9}"),
        (0x1f85, "\u{1f05}\u{3b9}"),
        (0x1f86, "\u{1f06}\u{3b9}"),
        (0x1f87, "\u{1f07}\u{3b9}"),
        (0x1f88, "\u{1f00}\u{3b9}"),
        (0x1f89, "\u{1f01}\u{3b9}"),
        (0x1f8a, "\u{1f02}\u{3b9}"),
        (0x1f8b, "\u{1f03}\u{3b9}"),
        (0x1f8c, "\u{1f04}\u{3b9}"),
        (0x1f8d, "\u{1f05}\u{3b9}"),
        (0x1f8e, "\u{1f06}\u{3b9}"),
        (0x1f8f, "\u{1f07}\u{3b9}"),
        (0x1f90, "\u{1f20}\u{3b9}"),
        (0x1f91, "\u{1f21}\u{3b9}"),
        (0x1f92, "\u{1f22}\u{3b9}"),
        (0x1f93, "\u{1f23}\u{3b9}"),
        (0x1f94, "\u{1f24}\u{3b9}"),
        (0x1f95, "\u{1f25}\u{3b9}"),
        (0x1f96, "\u{1f26}\u{3b9}"),
        (0x1f97, "\u{1f27}\u{3b9}"),
        (0x1f98, "\u{1f20}\u{3b9}"),
        (0x1f99, "\u{1f21}\u{3b9}"),
        (0x1f9a, "\u{1f22}\u{3b9}"),
        (0x1f9b, "\u{1f23}\u{3b9}"),
        (0x1f9c, "\u{1f24}\u{3b9}"),
        (0x1f9d, "\u{1f25}\u{3b9}"),
        (0x1f9e, "\u{1f26}\u{3b9}"),
        (0x1f9f, "\u{1f27}\u{3b9}"),
        (0x1fa0, "\u{1f60}\u{3b9}"),
        (0x1fa1, "\u{1f61}\u{3b9}"),
        (0x1fa2, "\u{1f62}\u{3b9}"),
        (0x1fa3, "\u{1f63}\u{3b9}"),
        (0x1fa4, "\u{1f64}\u{3b9}"),
        (0x1fa5, "\u{1f65}\u{3b9}"),
        (0x1fa6, "\u{1f66}\u{3b9}"),
        (0x1fa7, "\u{1f67}\u{3b9}"),
        (0x1fa8, "\u{1f60}\u{3b9}"),
        (0x1fa9, "\u{1f61}\u{3b9}"),
        (0x1faa, "\u{1f62}\u{3b9}"),
        (0x1fab, "\u{1f63}\u{3b9}"),
        (0x1fac, "\u{1f64}\u{3b9}"),
        (0x1fad, "\u{1f65}\u{3b9}"),
        (0x1fae, "\u{1f66}\u{3b9}"),
        (0x1faf, "\u{1f67}\u{3b9}"),
        (0x1fb2, "\u{1f70}\u{3b9}"),
        (0x1fb3, "\u{3b1}\u{3b9}"),
        (0x1fb4, "\u{3ac}\u{3b9}"),
        (0x1fb6, "\u{3b1}\u{342}"),
        (0x1fb7, "\u{3b1}\u{342}\u{3b9}"),
        (0x1fbc, "\u{3b1}\u{3b9}"),
        (0x1fbe, "\u{3b9}"),
        (0x1fc2, "\u{1f74}\u{3b9}"),
        (0x1fc3, "\u{3b7}\u{3b9}"),
        (0x1fc4, "\u{3ae}\u{3b9}"),
        (0x1fc6, "\u{3b7}\u{342}"),
        (0x1fc7, "\u{3b7}\u{342}\u{3b9}"),
        (0x1fcc, "\u{3b7}\u{3b9}"),
        (0x1fd2, "\u{3b9}\u{308}\u{300}"),
        (0x1fd3, "\u{3b9}\u{308}\u{301}"),
        (0x1fd6, "\u{3b9}\u{342}"),
        (0x1fd7, "\u{3b9}\u{308}\u{342}"),
        (0x1fe2, "\u{3c5}\u{308}\u{300}"),
        (0x1fe3, "\u{3c5}\u{308}\u{301}"),
        (0x1fe4, "\u{3c1}\u{313}"),
        (0x1fe6, "\u{3c5}\u{342}"),
        (0x1fe7, "\u{3c5}\u{308}\u{342}"),
        (0x1ff2, "\u{1f7c}\u{3b9}"),
        (0x1ff3, "\u{3c9}\u{3b9}"),
        (0x1ff4, "\u{3ce}\u{3b9}"),
        (0x1ff6, "\u{3c9}\u{342}"),
        (0x1ff7, "\u{3c9}\u{342}\u{3b9}"),
        (0x1ffc, "\u{3c9}\u{3b9}"),
        (0xfb00, "ff"),
        (0xfb01, "fi"),
        (0xfb02, "fl"),
        (0xfb03, "ffi"),
        (0xfb04, "ffl"),
        (0xfb05, "st"),
        (0xfb06, "st"),
        (0xfb13, "\u{574}\u{576}"),
        (0xfb14, "\u{574}\u{565}"),
        (0xfb15, "\u{574}\u{56b}"),
        (0xfb16, "\u{57e}\u{576}"),
        (0xfb17, "\u{574}\u{56d}"),
    ];
}

#[cfg(not(feature = "unicodedata"))]
mod ucd {
    use alloc::string::String;

    pub(super) fn alpha(c: char) -> bool {
        c.is_alphabetic()
    }

    pub(super) fn alnum(c: char) -> bool {
        c.is_alphanumeric()
    }

    pub(super) fn xid_start(c: char) -> bool {
        c.is_alphabetic()
    }

    pub(super) fn xid_continue(c: char) -> bool {
        c.is_alphanumeric()
            || matches!(c, '\u{300}'..='\u{36f}' | '\u{1ab0}'..='\u{1aff}' | '\u{1dc0}'..='\u{1dff}'
                | '\u{20d0}'..='\u{20ff}' | '\u{fe20}'..='\u{fe2f}' | '\u{200c}' | '\u{200d}')
    }

    pub(super) fn title(_: char, _: &mut String) -> bool {
        false
    }

    pub(super) fn fold(_: char, _: &mut String) -> bool {
        false
    }
}
pub(crate) fn zfill(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let sv = this_value(vm, &a, "zfill")?;
    let w = vm.int_arg(vm.arg_o(&a, 1), "width")?;
    let (n, len) = (vm.str_len(sv), vm.str_bytes(sv).len());
    // A width past `usize` (wasm32) saturates, so it fails the size check
    // as on 64-bit targets; a negative one pads nothing.
    let w = if w < 0 {
        0
    } else {
        usize::try_from(w).unwrap_or(usize::MAX)
    };
    if w <= n {
        return Ok(sv);
    }
    let bytes = (w - n).saturating_add(len);
    if !vm.heap.would_fit(bytes as u64) {
        return Err(vm.err(T::MemoryError, ""));
    }
    let sign = usize::from(matches!(vm.str_bytes(sv).first(), Some(b'+' | b'-')));
    if (bytes / UNIT) as u64 >= FILL {
        let z = vm.new_str("0")?;
        return fill_parts(vm, &[(sv, 0, sign), (z, 0, w - n), (sv, sign, len)]);
    }
    let s = String::from(vm.str_ref_o(sv));
    vm.burn_bytes(len);
    let mut out = String::new();
    out.push_str(&s[..sign]);
    for _ in 0..w - n {
        out.push('0');
    }
    out.push_str(&s[sign..]);
    vm.new_str(&out)
}

pub(crate) fn expandtabs(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    this_value(vm, &a, "expandtabs")?;
    let size = match opt(vm.arg_o(&a, 1)) {
        None => 8,
        Some(v) => {
            // A C int.
            let n = if Vm::is_heap_int(vm.unbx(v)) {
                None
            } else {
                Some(vm.int_arg(v, "tabsize")?).filter(|n| i32::try_from(*n).is_ok())
            };
            let Some(n) = n else {
                return Err(vm.err(T::OverflowError, "Python int too large to convert to C int"));
            };
            n
        }
    };
    if let Some(v) = big_map(
        vm,
        &a,
        mk::EXPANDTABS,
        Value::int(size).unwrap_or(Value::NONE),
    ) {
        return v;
    }
    let s = this(vm, &a, "expandtabs")?;
    // Sized first: the tabs can widen the text by any factor, and the
    // result is admitted before it is built.
    let mut bytes = 0usize;
    tabs(&s, size, &mut |c, n| {
        bytes = bytes.saturating_add(n.saturating_mul(c.len_utf8()));
    });
    if bytes as u64 >= BIG {
        // A short text with wide tabs: written a step at a time, as a
        // long text is (`'\t'.expandtabs(60_000_000)` was one 107 ms step
        // charged 60.9M units [M]).
        let s = vm.arg_o(&a, 0);
        return map_big(
            vm,
            s,
            mk::EXPANDTABS,
            Value::int(size).unwrap_or(Value::NONE),
        );
    }
    // A fill: a unit per 64 bytes written.
    vm.burn((bytes / UNIT) as u64);
    let res = vm.heap.reserve_scratch(bytes);
    let _native = vm.hx_o(res)?;
    let mut out = String::new();
    if out.try_reserve_exact(bytes).is_err() {
        return Err(vm.err(T::MemoryError, ""));
    }
    tabs(&s, size, &mut |c, n| {
        out.extend(core::iter::repeat_n(if c == '\t' { ' ' } else { c }, n));
    });
    vm.new_str(&out)
}

/// `s` with its tabs expanded to `size` columns, as `n` copies of a char at
/// a time; a tab comes as `n` tabs, each one space.
fn tabs(s: &str, size: i64, out: &mut dyn FnMut(char, usize)) {
    let mut col: i64 = 0;
    for c in s.chars() {
        // `col` restarts at each tab stop: only its offset matters.
        match c {
            '\t' if size > 0 => {
                out('\t', usize::try_from(size - col % size).unwrap_or(0));
                col = 0;
            }
            '\t' => {}
            '\n' | '\r' => {
                out(c, 1);
                col = 0;
            }
            _ => {
                out(c, 1);
                col += 1;
            }
        }
    }
}

fn pad(vm: &mut Vm, a: &CallArgs, name: &str, align: char) -> Result<Value, Raised> {
    let sv = this_value(vm, a, name)?;
    let w = vm.int_arg(vm.arg_o(a, 1), "width")?;
    let fill = match opt(vm.arg_o(a, 2)) {
        None => ' ',
        Some(f) => {
            let fs = str_arg(vm, f, "fill character")?;
            let mut cs = fs.chars();
            match (cs.next(), cs.next()) {
                (Some(c), None) => c,
                _ => {
                    return Err(vm.err(
                        T::TypeError,
                        "The fill character must be exactly one character long",
                    ));
                }
            }
        }
    };
    let (n, slen) = (vm.str_len(sv), vm.str_bytes(sv).len());
    // A width past `usize` (wasm32) saturates, so it fails the size check
    // as on 64-bit targets; a negative one pads nothing.
    let w = if w < 0 {
        0
    } else {
        usize::try_from(w).unwrap_or(usize::MAX)
    };
    if w <= n {
        return Ok(sv);
    }
    let total = w - n;
    // The width is guest-chosen: refuse what cannot fit before building it.
    let bytes = total.saturating_mul(fill.len_utf8()).saturating_add(slen);
    if !vm.heap.would_fit(bytes as u64) {
        return Err(vm.err(T::MemoryError, ""));
    }
    let (l, r) = match align {
        '<' => (0, total),
        '>' => (total, 0),
        _ => {
            // CPython's centering: the extra fill goes left when both the
            // margin and the width are odd.
            let left = total / 2 + (total & w & 1);
            (left, total - left)
        }
    };
    if (bytes / UNIT) as u64 >= FILL {
        let f = vm.new_str(fill.encode_utf8(&mut [0; 4]))?;
        let fl = fill.len_utf8();
        return fill_parts(vm, &[(f, 0, l * fl), (sv, 0, slen), (f, 0, r * fl)]);
    }
    let s = String::from(vm.str_ref_o(sv));
    vm.burn_bytes(slen);
    let mut out = String::with_capacity(w);
    out.extend(core::iter::repeat_n(fill, l));
    out.push_str(&s);
    out.extend(core::iter::repeat_n(fill, r));
    vm.new_str(&out)
}

pub(crate) fn center(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    pad(vm, &a, "center", '^')
}

pub(crate) fn ljust(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    pad(vm, &a, "ljust", '<')
}

pub(crate) fn rjust(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    pad(vm, &a, "rjust", '>')
}

pub(crate) fn splitlines(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let sv = this_value(vm, &a, "splitlines")?;
    vm.burn_bytes(vm.str_bytes(sv).len());
    let keep = opt(vm.arg_o(&a, 1)).is_some_and(|v| vm.truthy(v));
    let mode = if keep { sm::LINES_KEEP } else { sm::LINES };
    if big_text(vm, sv, CHAR_UNIT) {
        return split_big(vm, sv, Value::NONE, usize::MAX, mode);
    }
    let cap = (BIG / 4) as usize;
    // A byte scan: the multi-byte breaks (U+0085, U+2028, U+2029) start
    // with lead bytes, which never occur inside another character.
    let (b, heap) = (vm.str_bytes(sv), &vm.heap);
    let mut parts = Ranges::default();
    let mut full = false;
    let (mut start, mut i) = (0, 0);
    let maybe = |x: &u8| *x <= 0x1e || *x == 0xc2 || *x == 0xe2;
    let mut more = false;
    while let Some(k) = b.get(i..).and_then(|t| t.iter().position(maybe)) {
        i += k;
        let n = match b[i] {
            b'\r' if b.get(i + 1) == Some(&b'\n') => 2,
            b'\n' | b'\r' | 0x0b | 0x0c | 0x1c..=0x1e => 1,
            0xc2 if b.get(i + 1) == Some(&0x85) => 2,
            0xe2 if b.get(i + 1) == Some(&0x80) && matches!(b.get(i + 2), Some(0xa8 | 0xa9)) => 3,
            _ => {
                i += 1;
                continue;
            }
        };
        if parts
            .push_hot(heap, (start, if keep { i + n } else { i }))
            .is_err()
        {
            full = true;
            break;
        }
        if parts.len() >= cap {
            more = true;
            break;
        }
        i += n;
        start = i;
    }
    if more {
        return split_big(vm, sv, Value::NONE, usize::MAX, mode);
    }
    let len = b.len();
    if start < len && !full {
        full = parts.push_hot(heap, (start, len)).is_err();
    }
    if full {
        return Err(vm.err(T::MemoryError, ""));
    }
    list_of_ranges(vm, sv, &parts, true)
}

pub(crate) fn partition(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    partition_impl(vm, &a, false)
}

pub(crate) fn rpartition(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    partition_impl(vm, &a, true)
}

fn partition_impl(vm: &mut Vm, a: &CallArgs, right: bool) -> Result<Value, Raised> {
    // The receiver, maybe a subclass instance (`Vm::native_self`).
    let this = vm.arg_o(a, 0);
    let s = vm.unbx(this);
    if !Vm::is_str(s) {
        this_value(vm, a, if right { "rpartition" } else { "partition" })?;
    }
    let arg = vm.arg_o(a, 1);
    let sep = str_arg_value(vm, arg, "")?;
    if vm.str_bytes(sep).is_empty() {
        return Err(vm.err(T::ValueError, "empty separator"));
    }
    let n = vm.str_bytes(s).len();
    if n as u64 >= BIG {
        // Its search a step at a time, in the prelude twin (a 101-char
        // separator in 64 MiB was one 89 ms step [M]).
        let args = alloc::vec![s, sep, arg, Value::from_bool(right), this];
        return vm.prelude_request("_str_partition", args, crate::cont::Cont::None);
    }
    vm.burn_bytes(n);
    let (hay, p) = (vm.str_bytes(s), vm.str_bytes(sep));
    let k = p.len();
    let at = if right {
        rfind_in(hay, 0, n, p)
    } else {
        find_in(hay, 0, n, p)
    };
    let Some(i) = at else {
        // Not found: the str itself, a subclass instance too, as CPython.
        let e = vm.new_str("")?;
        let items = if right { [e, e, this] } else { [this, e, e] };
        return vm.alloc_tuple(&items);
    };
    // A found separator is the argument itself (a str subclass stays one).
    let items = [slice_copy(vm, s, 0, i)?, arg, slice_copy(vm, s, i + k, n)?];
    vm.alloc_tuple(&items)
}

fn remove_affix(vm: &mut Vm, a: &CallArgs, name: &str, prefix: bool) -> Result<Value, Raised> {
    let s = this_value(vm, a, name)?;
    let p = str_arg(vm, vm.arg_o(a, 1), &format!("{name}() argument"))?;
    let n = vm.str_bytes(s).len();
    vm.burn_bytes(n);
    let h = vm.str_bytes(s);
    let (st, en) = match (prefix, p.as_bytes()) {
        (true, p) if h.starts_with(p) => (p.len(), n),
        (false, p) if h.ends_with(p) => (0, n - p.len()),
        _ => (0, n),
    };
    slice_steps(vm, s, st, en)
}

pub(crate) fn removeprefix(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    remove_affix(vm, &a, "removeprefix", true)
}

pub(crate) fn removesuffix(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    remove_affix(vm, &a, "removesuffix", false)
}

/// `str.format` with positional and keyword fields and format specs.
pub(crate) fn format(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let sv = vm.arg(&a, 0);
    if Vm::is_str(sv) && big_text(vm, sv, 1) {
        return format_long(vm, sv);
    }
    let s = this(vm, &a, "format")?;
    let kw = vm.kwarg(&a, 0);
    let begun = vm.work;
    let mut out = NativeText::default();
    let mut auto = 0u32;
    // 1: automatic numbering, 2: manual.
    let mut manual = 0u8;
    // Braces are ASCII, so byte offsets at them are char boundaries.
    let cs = s.as_bytes();
    let mut i = 0;
    while i < cs.len() {
        let c = cs[i];
        if c == b'{' {
            if cs.get(i + 1) == Some(&b'{') {
                out.push(vm, "{")?;
                i += 2;
                continue;
            }
            let mut j = i + 1;
            while j < cs.len() && cs[j] != b'}' {
                j += 1;
            }
            let field = &s[i + 1..j.min(cs.len())];
            let (name, spec) = field.split_once(':').unwrap_or((field, ""));
            let (name, conv) = match name.split_once('!') {
                Some((n, c)) => (n, Some(c)),
                None => (name, None),
            };
            // Field names with parts, nested specs, unusual conversions and
            // numbering mistakes: the prelude twin (CPython's parser).
            let numeric = name.is_empty() || name.bytes().all(|b| b.is_ascii_digit());
            let mode = if name.is_empty() { 1 } else { 2 };
            if j >= cs.len()
                || field.contains(['{', '[', '.'])
                || !name.is_ascii()
                || conv.is_some_and(|c| c != "r" && c != "s")
                || (numeric && manual != 0 && manual != mode)
            {
                return Err(vm.need_py());
            }
            if numeric {
                manual = mode;
            }
            let conv = conv.and_then(|c| c.chars().next());
            let v = if name.is_empty() {
                let v = vm.arg(&a, auto + 1);
                auto += 1;
                v
            } else if let Ok(k) = name.parse::<u32>() {
                vm.arg(&a, k + 1)
            } else {
                let key = vm.new_str(name)?;
                if kw == Value::UNDEFINED {
                    Value::UNDEFINED
                } else {
                    vm.dict_get(kw, key)?.unwrap_or(Value::UNDEFINED)
                }
            };
            if v == Value::UNDEFINED {
                if name.is_empty() || name.parse::<u32>().is_ok() {
                    return Err(vm.err(
                        T::IndexError,
                        &format!(
                            "Replacement index {} out of range for positional args tuple",
                            if name.is_empty() {
                                auto - 1
                            } else {
                                name.parse::<u32>().unwrap_or(0)
                            }
                        ),
                    ));
                }
                let key = vm.new_str(name)?;
                return Err(vm.err_value(T::KeyError, key));
            }
            // A long argument, a wide spec, a long output or a step's work
            // so far (big ints' conversions): the prelude twin, which
            // builds each piece and joins them a step at a time
            // (docs/spec.md §5). The work here stays under about a step.
            if out.len() as u64 >= BIG
                || vm.work.wrapping_sub(begun) + vm.decimal_units(v) > STEP_WORK
                || long_piece(vm, v, spec)
            {
                return Err(vm.need_py());
            }
            let v = match conv {
                Some('r') => {
                    let r = vm.repr(v)?;
                    vm.new_str(&r)?
                }
                // `str(s)` is `s` itself.
                Some('s') if Vm::is_str(v) => v,
                Some('s') => {
                    let r = vm.stringify(v)?;
                    vm.new_str(&r)?
                }
                _ => v,
            };
            if vm.long_number_format(v, spec) {
                // Its `format` goes in steps: the twin calls it.
                return Err(vm.need_py());
            }
            if out.is_empty() && j + 1 == cs.len() && Vm::is_str(vm.unbx(v)) {
                // The template's last write, into nothing: the formatted
                // str itself, as CPython's writer gives it (a str the spec
                // leaves as it is, a subclass instance too).
                return vm.format_str(v, FormatSpec::Text(spec));
            }
            let text = vm.format(v, FormatSpec::Text(spec))?;
            out.push(vm, &text)?;
            i = j + 1;
        } else if c == b'}' {
            if cs.get(i + 1) == Some(&b'}') {
                out.push(vm, "}")?;
                i += 2;
                continue;
            }
            return Err(vm.err(T::ValueError, "Single '}' encountered in format string"));
        } else {
            let end = cs[i..]
                .iter()
                .position(|&b| b == b'{' || b == b'}')
                .map_or(cs.len(), |k| i + k);
            out.push(vm, &s[i..end])?;
            i = end;
        }
    }
    vm.new_str(&out)
}

/// Whether `str.format` field `v` with format spec `spec` can make a
/// long piece: `v` a long str or bytes-like, or a number in `spec` (a
/// width or precision) of `BIG` or more.
fn long_piece(vm: &Vm, v: Value, spec: &str) -> bool {
    if (Vm::is_str(v) || Vm::is_bytes_like(v)) && text(vm, v).len() as u64 >= BIG {
        return true;
    }
    spec.split(|c: char| !c.is_ascii_digit())
        .any(|d| d.len() > 6 || d.parse::<u64>().is_ok_and(|n| n >= BIG))
}

/// `str.translate(table)`: a builtin dict whose values are `None`, ints
/// and strs is mapped here; any other table or value continues in the
/// prelude twin, which indexes the table as CPython does.
/// `str.format` of a long template: itself when it has no braces (as
/// CPython returns it), else the prelude twin, which finds the literal
/// runs natively and joins a step at a time (docs/spec.md §5).
#[cold]
fn format_long(vm: &mut Vm, s: Value) -> Result<Value, Raised> {
    let h = vm.str_bytes(s);
    let n = h.len();
    let braces = h.contains(&b'{') || h.contains(&b'}');
    vm.burn_bytes(n);
    if braces {
        return Err(vm.need_py());
    }
    Ok(s)
}

pub(crate) fn translate(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let s = this_value(vm, &a, "translate")?;
    let table = vm.arg_o(&a, 1);
    if !table.is_obj_kind(ty::DICT as u8) {
        return Err(vm.need_py());
    }
    if let Some(v) = big_map(vm, &a, mk::TRANSLATE, table) {
        return v;
    }
    vm.burn(vm.str_bytes(s).len() as u64);
    let mut out = NativeText::default();
    // One copy of the text: `str_ref` checks the whole string each call,
    // which made this loop quadratic.
    let text = String::from(vm.str_ref_o(s));
    for c in text.chars() {
        let k = Value::int(i64::from(u32::from(c))).unwrap_or(Value::NONE);
        match vm.dict_get(table, k)? {
            None => out.push(vm, c.encode_utf8(&mut [0; 4]))?,
            Some(Value::NONE) => {}
            Some(v) if Vm::is_str(v) => out.push_value(vm, v)?,
            Some(v) => {
                let ch = vm
                    .int_of(v)
                    .and_then(|n| u32::try_from(n).ok())
                    .and_then(char::from_u32);
                match ch {
                    Some(ch) => out.push(vm, ch.encode_utf8(&mut [0; 4]))?,
                    // Not an int, out of range, or a surrogate: CPython's
                    // checks in the twin.
                    None => return Err(vm.need_py()),
                }
            }
        }
    }
    vm.new_str(&out)
}

// ------------------------------------------------- chunked str work
//
// Bounded native steps (docs/spec.md §5): an operation whose work over a
// long str would pass a step goes a step at a time (`nstep.rs`). A result
// is written straight into an arena byte block of its final size, a slice
// per step, and becomes a `str` at the end (`Heap::str_from_block`): no
// Rust buffer the size of the text. Every operation keeps the same state:
// value slots `[s, a, b, out]`, raw words `w` (after the operation's).

/// Bytes of text a fuel unit pays for when copied or scanned for bytes.
const UNIT: usize = 64;

/// Bytes a fuel unit pays for when each char is looked at (case maps,
/// predicates, `translate`, `expandtabs`): about 10 ns of that work.
const CHAR_UNIT: usize = 8;

/// The fuel units of work at which an operation goes a step at a time
/// (about 5 ms); below it, it stays one native call as before.
pub(crate) const BIG: u64 = 4 * STEP_WORK;

/// The same for a result copied from pieces (`*`, padding, `format`): past
/// one step's copy (8 MB), so a shorter one stays a single copy.
const FILL: u64 = STEP_WORK;

/// [`FILL`] in bytes: `s + t` goes a step at a time from here.
pub(crate) const FILL_BYTES: usize = FILL as usize * UNIT;

/// The map kinds (`op::STR_MAP`, value slot 2).
pub(crate) mod mk {
    pub const UPPER: i64 = 0;
    pub const LOWER: i64 = 1;
    pub const SWAPCASE: i64 = 2;
    pub const TITLE: i64 = 3;
    pub const CAPITALIZE: i64 = 4;
    pub const CASEFOLD: i64 = 5;
    pub const TRANSLATE: i64 = 6;
    pub const EXPANDTABS: i64 = 7;
    pub const SLICE: i64 = 8;
    pub const REPR: i64 = 9;
    pub const ASCII: i64 = 10;
    /// The repr of a `bytes`: `ascii()`'s escapes over its bytes as
    /// Latin-1 chars, after a `b`.
    pub const BYTES_REPR: i64 = 11;
}

/// The scan kinds (`op::STR_SCAN`, value slot 2): a count, or a
/// predicate over every char.
mod sk {
    pub const COUNT: i64 = 0;
    pub const DIGIT: i64 = 1;
    pub const DECIMAL: i64 = 2;
    pub const NUMERIC: i64 = 3;
    pub const ALPHA: i64 = 4;
    pub const ALNUM: i64 = 5;
    pub const SPACE: i64 = 6;
    pub const PRINTABLE: i64 = 7;
    pub const IDENTIFIER: i64 = 8;
    pub const UPPER: i64 = 9;
    pub const LOWER: i64 = 10;
    pub const TITLE: i64 = 11;
    /// `strip` (`lstrip`, `rstrip`): `w[2]` holds the ends still to go
    /// (1 left, 2 right), the chars are value slot 1 (`None`: whitespace).
    pub const STRIP: i64 = 12;
    /// `find` (`index`, `in`) over `[w[0], w[1])`, `w[0]` moving up;
    /// `w[2]` what it gives (`fm`).
    pub const FIND: i64 = 13;
    /// `rfind` (`rindex`) over `[w[0], w[1])`, `w[1]` moving down.
    pub const RFIND: i64 = 14;
}

/// A chunked str operation between two steps.
struct Walk {
    op: u8,
    v: [Value; 4],
    w: [u64; 7],
    /// `Vm::work` when the step started.
    start: u64,
    /// Fuel units of the step's own work, charged in one sum when it
    /// ends: a charge over 64 units makes the next safepoint check the
    /// clock (`Vm::burn`).
    owed: u64,
}

impl Walk {
    /// Whether the step has done its work.
    fn spent(&self, vm: &Vm) -> bool {
        vm.work.wrapping_sub(self.start) + self.owed >= STEP_WORK
    }

    /// The fuel units left in the step (at least one).
    fn left(&self, vm: &Vm) -> usize {
        usize::try_from(STEP_WORK.saturating_sub(vm.work.wrapping_sub(self.start) + self.owed))
            .unwrap_or(1)
            .max(1)
    }

    /// Raw word `i` as an index.
    fn at(&self, i: usize) -> usize {
        self.w
            .get(i)
            .map_or(usize::MAX, |&x| usize::try_from(x).unwrap_or(usize::MAX))
    }

    fn kind(&self) -> i64 {
        self.v[2].as_int().unwrap_or(-1)
    }

    fn run(&mut self, vm: &mut Vm) -> Result<Option<Value>, Raised> {
        let r = match self.op {
            op::STR_FILL | op::STR_JOIN => fill_run(vm, self),
            op::STR_REPLACE => replace_run(vm, self),
            op::STR_SPLIT => split_run(vm, self),
            op::STR_MAP => map_run(vm, self),
            _ => scan_run(vm, self),
        };
        vm.burn(core::mem::take(&mut self.owed));
        r
    }
}

/// Chunked operation `op`: its first step here, the rest in `_steps`.
fn walk(vm: &mut Vm, op: u8, v: [Value; 4], w: [u64; 7]) -> Result<Value, Raised> {
    let mut k = Walk {
        op,
        v,
        w,
        start: vm.work,
        owed: 0,
    };
    match k.run(vm)? {
        Some(x) => Ok(x),
        None => vm.chunk_rest(op, &k.v, &k.w),
    }
}

impl Vm {
    /// One step of a chunked str operation (`nstep.rs`).
    pub(crate) fn str_step(&mut self, op: u8, state: Value) -> Result<Step, Raised> {
        let (Some((v, w)), Some(r)) = (self.nstate_get::<4, 8>(op, state), state.as_obj()) else {
            return Err(self.bad_step());
        };
        // A scan's text and its window `[at, end)` of it: a forged image
        // with other words would make no progress, parking for ever.
        if op == op::STR_SCAN
            && !(Vm::is_str(v[0]) && w[1] <= w[2] && w[2] <= self.str_bytes(v[0]).len() as u64)
        {
            return Err(self.bad_step());
        }
        let mut raw = [0; 7];
        raw.copy_from_slice(&w[1..]);
        let mut k = Walk {
            op,
            v,
            w: raw,
            start: self.work,
            owed: 0,
        };
        match k.run(self) {
            Ok(Some(x)) => Ok(Step::Done(x)),
            Ok(None) => {
                for (i, &x) in k.v.iter().enumerate() {
                    self.set_slot_o(r, i as u32, x);
                }
                self.nstate_put(state, 1, &k.w);
                Ok(Step::More)
            }
            // A mapping value that needs Python code: the twin translates
            // the whole text (nothing it has done shows).
            Err(Raised) if self.slow && op == op::STR_MAP => {
                self.slow = false;
                let a = self.alloc_tuple(&[k.v[0], k.v[1]])?;
                let f = self.prelude_fn("_str_translate")?;
                Ok(Step::Done(self.request_call(
                    f,
                    alloc::vec![a],
                    crate::cont::Cont::None,
                )))
            }
            Err(e) => Err(e),
        }
    }
}

/// A new byte block of `n` bytes for a result built a step at a time.
fn block(vm: &mut Vm, n: usize) -> Result<Value, Raised> {
    let res = vm.heap.alloc_bytes_zeroed(n);
    let b = vm.hx_o(res)?;
    vm.collect_for_result(n);
    Ok(Value::from_byte_ref(b))
}

/// Copies `n` bytes of block `src` from `from` into block `out` at `to`,
/// charging the step one unit per 64; the chars copied (counted unless
/// `ascii`).
#[allow(clippy::too_many_arguments, reason = "a copy and its charge")]
fn put(
    vm: &mut Vm,
    k: &mut Walk,
    out: Value,
    to: usize,
    src: ByteRef,
    from: usize,
    n: usize,
    ascii: bool,
) -> Result<u64, Raised> {
    let Some(ob) = out.as_byte_ref() else {
        return Err(vm.bad_step());
    };
    if !seam_ok(vm, ob, to, src, from) || vm.heap.copy_bytes(src, from, ob, to, n).is_err() {
        return Err(vm.bad_step());
    }
    k.owed += (n / UNIT) as u64 + 1;
    if ascii {
        return Ok(n as u64);
    }
    let b = vm.heap.bytes(ob).ok().and_then(|b| b.get(to..to + n));
    Ok(b.map_or(0, |b| b.iter().filter(|&&c| c & 0xC0 != 0x80).count()) as u64)
}

/// The continuation bytes the char begun at the end of `t` still needs
/// (`usize::MAX` when its last bytes start no char).
fn missing(t: &[u8]) -> usize {
    let tail = &t[t.len().saturating_sub(4)..];
    let Some(k) = tail.iter().rposition(|&c| c & 0xC0 != 0x80) else {
        return if t.is_empty() { 0 } else { usize::MAX };
    };
    let width: usize = match tail[k] {
        0xF0.. => 4,
        0xE0.. => 3,
        0xC0.. => 2,
        _ => 1,
    };
    width.checked_sub(tail.len() - k).unwrap_or(usize::MAX)
}

/// Whether bytes copied from `src` at `from` go on from the first `o`
/// bytes of block `out` as UTF-8: the continuation bytes at `from` are
/// those the char begun at `o` needs, so a forged offset cannot cut a
/// char or splice two.
fn seam_ok(vm: &Vm, out: ByteRef, o: usize, src: ByteRef, from: usize) -> bool {
    let need = vm
        .heap
        .bytes(out)
        .ok()
        .and_then(|b| b.get(..o))
        .map(missing);
    let have = vm
        .heap
        .bytes(src)
        .ok()
        .and_then(|s| s.get(from..))
        .map(|s| s.iter().take(3).take_while(|&&c| c & 0xC0 == 0x80).count());
    need.is_some() && need == have
}

/// Writes `text` into block `out` at `to`.
fn put_text(vm: &mut Vm, out: Value, to: usize, text: &[u8]) -> Result<(), Raised> {
    let dst = out
        .as_byte_ref()
        .and_then(|b| vm.heap.bytes_mut(b).ok())
        .and_then(|b| b.get_mut(to..to + text.len()));
    match dst {
        Some(d) => {
            d.copy_from_slice(text);
            Ok(())
        }
        None => Err(vm.bad_step()),
    }
}

/// The UTF-8 bytes of `str` `x` (the data of a `bytes`), out of line:
/// the step code reads them at many sites.
#[inline(never)]
fn text(vm: &Vm, x: Value) -> &[u8] {
    if Vm::is_bytes_like(x) {
        vm.bytes_data(x)
    } else {
        vm.str_bytes(x)
    }
}

/// The data block of `str` `x`.
fn data(vm: &mut Vm, x: Value) -> Result<ByteRef, Raised> {
    info(vm, x).map(|i| i.0)
}

/// The data block of `str` `x`, its length in bytes and chars, and
/// whether it is ASCII (a bytes-like: its data, counted as ASCII).
fn info(vm: &mut Vm, x: Value) -> Result<(ByteRef, usize, u64, bool), Raised> {
    if Vm::is_bytes_like(x) {
        let Some(b) = vm.data_block(x) else {
            return Err(vm.bad_step());
        };
        let len = vm.bytes_data(x).len();
        return Ok((b, len, len as u64, true));
    }
    let Some(Ok((b, chars, ascii))) = x.as_obj().map(|r| vm.heap.str_block(r)) else {
        return Err(vm.bad_step());
    };
    let len = if ascii {
        usize::try_from(chars).unwrap_or(0)
    } else {
        vm.heap.bytes(b).map_or(0, <[u8]>::len)
    };
    Ok((b, len, chars, ascii))
}

/// The `str` over block `out`, all `o` bytes of it written.
fn finish(vm: &mut Vm, out: Value, o: usize, chars: u64) -> Result<Option<Value>, Raised> {
    let Some(b) = out.as_byte_ref() else {
        return Err(vm.bad_step());
    };
    // All written, the last char whole.
    if vm.heap.bytes(b).map(|t| (t.len(), missing(t))) != Ok((o, 0)) {
        return Err(vm.bad_step());
    }
    let res = vm.heap.str_from_block(b, chars);
    let r = vm.hx_o(res)?;
    Ok(Some(Value::from_ref(r, ty::STR as u8)))
}

/// The `bytes` over block `out`, all `o` bytes of it written.
fn finish_bytes(vm: &mut Vm, out: Value, o: usize, ba: bool) -> Result<Option<Value>, Raised> {
    let Some(b) = out.as_byte_ref() else {
        return Err(vm.bad_step());
    };
    if vm.heap.bytes(b).map(<[u8]>::len) != Ok(o) {
        return Err(vm.bad_step());
    }
    let res = if ba { R_BYTEARRAY } else { R_BYTES };
    vm.job_wrap(&Job::new(0, Value::NONE, 0, 0, res), b, 0)
        .map(Some)
}

/// An index as a small int.
pub(crate) fn smi(n: usize) -> Value {
    Value::int(i64::try_from(n).unwrap_or(i64::MAX)).unwrap_or(Value::NONE)
}

/// The smallest char boundary of `h` at or above `i` (its length past it).
fn ceil_char(h: &[u8], mut i: usize) -> usize {
    while h.get(i).is_some_and(|&b| b & 0xC0 == 0x80) {
        i += 1;
    }
    i.min(h.len())
}

/// The largest char boundary of `h` at or below `i`.
fn floor_char(h: &[u8], mut i: usize) -> usize {
    i = i.min(h.len());
    while i > 0 && h.get(i).is_some_and(|&b| b & 0xC0 == 0x80) {
        i -= 1;
    }
    i
}

/// Fuel for setting up one [`find_in`] or [`rfind_in`] for a needle of
/// `pl` bytes, beyond the bytes it reads: std's two-way searcher passes
/// over a long needle twice (its maximal suffixes) on every call, and a
/// step that searches again after each match pays that each time (a
/// 524,287-byte separator with adjacent matches made `split` one 220 ms
/// step charged 16,000 units [M]).
fn setup_units(pl: usize) -> u64 {
    if pl > SHORT_PAT {
        (2 * pl / UNIT) as u64
    } else {
        0
    }
}

/// The first match of `p` (non-empty, valid UTF-8) in `h` that starts in
/// `from..lim` (`from` a char boundary).
fn find_in(h: &[u8], from: usize, lim: usize, p: &[u8]) -> Option<usize> {
    let end = ceil_char(h, lim.saturating_add(p.len().saturating_sub(1)));
    let w = h.get(from..end)?;
    let k = if p.len() <= SHORT_PAT {
        find_short(w, p)
    } else {
        // In pieces that double, each read as UTF-8 and searched once
        // (the next starts `p.len() - 1` back): checking all of `w` on
        // each call read the rest of the text once a match, so a 9-byte
        // needle's adjacent matches made `count` of 3 MB one 10.9 s
        // step [M].
        let p = core::str::from_utf8(p).ok()?;
        let (mut at, mut grow) = (0usize, p.len().saturating_mul(4).max(64));
        loop {
            let e = ceil_char(w, at.saturating_add(grow));
            if let Some(k) = core::str::from_utf8(w.get(at..e)?).ok()?.find(p) {
                break Some(at + k);
            }
            if e >= w.len() {
                break None;
            }
            at = floor_char(w, e + 1 - p.len());
            grow = grow.saturating_mul(2);
        }
    };
    k.map(|k| from + k).filter(|&i| i < lim)
}

/// The last match of `p` (non-empty, valid UTF-8) in `h[lo..hi]`.
fn rfind_in(h: &[u8], lo: usize, hi: usize, p: &[u8]) -> Option<usize> {
    let w = h.get(lo..hi)?;
    if p.len() > SHORT_PAT {
        // As `find_in`, in pieces from the end.
        let p = core::str::from_utf8(p).ok()?;
        let (mut e, mut grow) = (w.len(), p.len().saturating_mul(4).max(64));
        loop {
            let st = floor_char(w, e.saturating_sub(grow));
            if let Some(k) = core::str::from_utf8(w.get(st..e)?).ok()?.rfind(p) {
                return Some(lo + st + k);
            }
            if st == 0 {
                return None;
            }
            e = ceil_char(w, st + p.len() - 1);
            grow = grow.saturating_mul(2);
        }
    }
    let (&last, _) = p.split_last()?;
    let (mut e, mut misses) = (w.len(), 0usize);
    while let Some(k) = rfind_byte(last, w.get(..e)?) {
        if let Some(st) = (k + 1).checked_sub(p.len())
            && w.get(st..=k) == Some(p)
        {
            return Some(lo + st);
        }
        e = k;
        misses += 1;
        if misses >= DENSE && misses * 8 >= w.len() - e {
            // As in `find_short`: the last byte is everywhere.
            return rfind_dense(w, p, e).map(|st| lo + st);
        }
    }
    None
}

// --- pieces: `*`, padding, `join`

/// Builds the concatenation of `parts`, each bytes `a..b` of a str
/// repeated (`a` is 0 when `b` passes its length), a step at a time.
pub(crate) fn fill_parts(vm: &mut Vm, parts: &[(Value, usize, usize)]) -> Result<Value, Raised> {
    fill_kind(vm, parts, false)
}

/// [`fill_parts`] making a `bytes` when `bytes` (its pieces are strs or
/// `bytes`).
pub(crate) fn fill_kind(
    vm: &mut Vm,
    parts: &[(Value, usize, usize)],
    bytes: bool,
) -> Result<Value, Raised> {
    let total = parts
        .iter()
        .try_fold(0usize, |t, &(_, a, b)| t.checked_add(b.saturating_sub(a)));
    let Some(total) = total.filter(|&t| vm.heap.would_fit(t as u64)) else {
        return Err(vm.err(T::MemoryError, ""));
    };
    let out = block(vm, total)?;
    let mut items = Vec::with_capacity(parts.len() * 3);
    for &(x, a, b) in parts {
        items.extend([x, smi(a), smi(b)]);
    }
    let items = vm.alloc_tuple(&items)?;
    walk(
        vm,
        op::STR_FILL,
        [Value::NONE, items, Value::from_bool(bytes), out],
        [0; 7],
    )
}

/// Part `j` of a fill or join: a str and its byte range.
fn part(vm: &Vm, k: &Walk, j: usize) -> Option<(Value, usize, usize)> {
    let items = vm.seq_words(k.v[1]);
    let word = |i: usize| items.get(i).and_then(|&w| Value::from_bits(w));
    let x = if k.op == op::STR_JOIN {
        if j.is_multiple_of(2) {
            word(j / 2)?
        } else {
            word(j / 2 + 1)?;
            k.v[0]
        }
    } else {
        let at = |i| {
            word(i)
                .and_then(Value::as_int)
                .and_then(|n| usize::try_from(n).ok())
        };
        return Some((word(3 * j)?, at(3 * j + 1)?, at(3 * j + 2)?));
    };
    Some((x, 0, text(vm, x).len()))
}

/// A fill or a join: `w` is `[part, bytes of it done, out, chars]`, and
/// for a join `[.., phase, ..]` ([`join_size`]).
fn fill_run(vm: &mut Vm, k: &mut Walk) -> Result<Option<Value>, Raised> {
    if k.op == op::STR_JOIN && !join_size(vm, k)? {
        return Ok(None);
    }
    let out = k.v[3];
    let Some(ob) = out.as_byte_ref() else {
        return Err(vm.bad_step());
    };
    loop {
        let (j, q, o) = (k.at(0), k.at(1), k.at(2));
        let Some((x, a, b)) = part(vm, k, j) else {
            // A fill's slot 2 is its bytes flag; a join's is `None` (str),
            // `True` (bytes) or `False` (bytearray).
            let ba = k.op == op::STR_JOIN && k.v[2] == Value::FALSE;
            if k.v[2] == Value::TRUE || ba {
                return finish_bytes(vm, out, o, ba);
            }
            return finish(vm, out, o, k.w[3]);
        };
        let (src, xl, xc, ascii) = info(vm, x)?;
        let len = b.saturating_sub(a);
        if q >= len {
            (k.w[0], k.w[1]) = (k.w[0] + 1, 0);
            continue;
        }
        if k.spent(vm) {
            return Ok(None);
        }
        let room = k.left(vm).saturating_mul(UNIT);
        let (from, at, n) = if a + q < xl {
            (src, a + q, (len - q).min(xl - a - q))
        } else if a == 0 && xl > 0 {
            // Repeated: from the copies already written.
            let p = q / xl * xl;
            (ob, o - p, (len - q).min(p))
        } else {
            return Err(vm.bad_step());
        };
        let n = n.min(room);
        // A str's bytes go on as UTF-8 ([`seam_ok`]); a join of bytes-likes
        // (slot 2 not `None`) or a fill of bytes (`True`) need not.
        let text = if k.op == op::STR_JOIN {
            k.v[2] == Value::NONE
        } else {
            k.v[2] != Value::TRUE
        };
        if (text && !seam_ok(vm, ob, o, from, at))
            || vm.heap.copy_bytes(from, at, ob, o, n).is_err()
        {
            return Err(vm.bad_step());
        }
        k.owed += (n / UNIT) as u64 + 4;
        let chars = if ascii {
            n as u64
        } else if n == xl {
            xc
        } else {
            let w = vm.heap.bytes(ob).ok().and_then(|w| w.get(o..o + n));
            w.map_or(0, |w| w.iter().filter(|&&c| c & 0xC0 != 0x80).count()) as u64
        };
        if q + n >= len {
            (k.w[0], k.w[1]) = (k.w[0] + 1, 0);
        } else {
            k.w[1] += n as u64;
        }
        k.w[2] += n as u64;
        k.w[3] += chars;
    }
}

/// `sep.join(items)` (a list or tuple of exact strs) a step at a time:
/// sized, then built.
pub(crate) fn join_big(vm: &mut Vm, sep: Value, items: Value) -> Result<Value, Raised> {
    walk(
        vm,
        op::STR_JOIN,
        [sep, items, Value::NONE, Value::NONE],
        [0; 7],
    )
}

/// `sep.join(items)` for a bytes-like `sep` and a list or tuple of
/// bytes-likes, each read in place, a step at a time as a str's (value
/// slot 2 `True` for a `bytes` result, `False` for a `bytearray`).
pub(crate) fn join_bytes(vm: &mut Vm, sep: Value, items: Value) -> Result<Value, Raised> {
    walk(
        vm,
        op::STR_JOIN,
        [sep, items, Value::from_bool(Vm::is_bytes(sep)), Value::NONE],
        [0; 7],
    )
}

/// A join's sizing (`w[4]` is 0): `w[5]` bytes so far, `w[6]` items.
fn join_size(vm: &mut Vm, k: &mut Walk) -> Result<bool, Raised> {
    while k.w[4] == 0 {
        if k.spent(vm) {
            return Ok(false);
        }
        let (i, n) = (k.at(6), vm.seq_words(k.v[1]).len());
        let end = n.min(i.saturating_add(k.left(vm) / 2).max(i + 1));
        let mut bytes = k.at(5);
        for j in i..end {
            let w = vm.seq_words(k.v[1]).get(j).copied().unwrap_or(0);
            let x = Value::from_bits(w).unwrap_or(Value::NONE);
            bytes = bytes.saturating_add(text(vm, x).len());
        }
        k.owed += 2 * (end - i) as u64 + 1;
        (k.w[5], k.w[6]) = (bytes as u64, end as u64);
        if end >= n {
            let sep = text(vm, k.v[0]).len();
            let bytes = bytes.saturating_add(sep.saturating_mul(n.saturating_sub(1)));
            if !vm.heap.would_fit(bytes as u64) {
                return Err(vm.err(T::MemoryError, ""));
            }
            k.v[3] = block(vm, bytes)?;
            k.w = [0, 0, 0, 0, 1, 0, 0];
        }
    }
    Ok(true)
}

/// `format(s, spec)` of exact str `s` whose result is long: `s` itself
/// for an empty spec, else its pieces (fill, the text cut at the
/// precision, fill) a step at a time; `None`
/// for a short result or a spec the native path checks (and refuses).
pub(crate) fn format_big(vm: &mut Vm, s: Value, spec: &str) -> Option<Result<Value, Raised>> {
    if !Vm::is_str(s) {
        return None;
    }
    if spec.is_empty() {
        // `str(s)`, which is `s` itself, as CPython gives it: a copy of a
        // long text was one step.
        return Some(Ok(s));
    }
    let p = crate::text::Spec::parse(spec)?;
    let zeros = p.zero && p.fill == '0' && p.align == Some('=');
    if !matches!(p.ty, None | Some('s'))
        || p.sign.is_some()
        || p.zflag
        || p.alt
        || p.grouping.is_some()
        || p.frac_grouping.is_some()
        || p.align == Some('=') && !zeros
    {
        return None;
    }
    let (len, n) = (text(vm, s).len(), vm.str_len(s));
    let chars = p.precision.map_or(n, |c| c.min(n));
    let fill = p.width.saturating_sub(chars);
    if fill == 0 && chars == n {
        // The text as it is: `s` itself, as CPython gives it.
        return Some(Ok(s));
    }
    let bytes = fill.checked_mul(p.fill.len_utf8())?.checked_add(len)?;
    if ((bytes / UNIT) as u64) < FILL {
        return None;
    }
    Some((|| {
        let end = if chars == n {
            len
        } else {
            vm.str_offset(s, chars)
        };
        let (l, r) = match p.align {
            Some('>') => (fill, 0),
            Some('^') => (fill / 2, fill - fill / 2),
            _ => (0, fill),
        };
        let f = vm.new_str(p.fill.encode_utf8(&mut [0; 4]))?;
        let fl = p.fill.len_utf8();
        if !vm.heap.would_fit(bytes as u64) {
            return Err(vm.err(T::MemoryError, ""));
        }
        fill_parts(vm, &[(f, 0, l * fl), (s, 0, end), (f, 0, r * fl)])
    })())
}

/// `s * k` for a result of `bytes` bytes, a step at a time when that is
/// big (docs/spec.md §5).
/// Only the dispatch loop and `binop_top` (`top`) take a request.
#[inline(never)]
pub(crate) fn repeat(
    vm: &mut Vm,
    s: Value,
    k: usize,
    bytes: usize,
    top: bool,
) -> Result<Value, Raised> {
    if top && (bytes / UNIT) as u64 >= FILL {
        return fill_parts(vm, &[(s, 0, bytes)]);
    }
    let t = vm.str_ref_o(s).repeat(k);
    vm.new_str(&t)
}

// --- replace

/// `w`: `[phase (0 counting, 1 building), at, out, matches (counted, then
/// left), most, all matches, chars]`.
fn replace_run(vm: &mut Vm, k: &mut Walk) -> Result<Option<Value>, Raised> {
    let [s, old, new, _] = k.v;
    let (len, pl, rl) = (text(vm, s).len(), text(vm, old).len(), text(vm, new).len());
    while k.w[0] == 0 {
        // Counting: a window of the text a step.
        let (i, n, most) = (k.at(1), k.at(3), k.at(4));
        if i >= len || n >= most {
            let total = (len - pl * n).checked_add(rl.saturating_mul(n));
            let Some(total) = total.filter(|&t| vm.heap.would_fit(t as u64)) else {
                return Err(vm.err(T::MemoryError, ""));
            };
            let chars = vm.str_len(s) - vm.str_len(old) * n + vm.str_len(new) * n;
            k.v[3] = block(vm, total)?;
            k.w = [1, 0, 0, n as u64, 0, n as u64, chars as u64];
            break;
        }
        if k.spent(vm) {
            return Ok(None);
        }
        let lim = ceil_char(text(vm, s), i.saturating_add(k.left(vm) * UNIT));
        let found = find_in(text(vm, s), i, lim, text(vm, old));
        // Past the match found, or to the window's end.
        let to = found.map_or(lim, |m| m + pl);
        k.w[1] = to as u64;
        k.w[3] += u64::from(found.is_some());
        k.owed += ((to - i) / UNIT) as u64 + 1 + setup_units(pl);
    }
    let out = k.v[3];
    let (src, rep) = (data(vm, s)?, data(vm, new)?);
    loop {
        let (i, o, left) = (k.at(1), k.at(2), k.at(3));
        if i >= len && (left == 0 || pl > 0) {
            return finish(vm, out, o, k.w[6]);
        }
        if k.spent(vm) {
            return Ok(None);
        }
        // The next match, or the end of what this step copies. The chars
        // are known (raw word 6): `put` need not count them.
        let lim = ceil_char(text(vm, s), i.saturating_add(k.left(vm) * UNIT));
        let m = match left {
            0 => None,
            _ if pl == 0 => Some(i),
            _ => {
                // The needle read past the copy, and the searcher's setup.
                k.owed += (pl / UNIT) as u64 + setup_units(pl);
                find_in(text(vm, s), i, lim, text(vm, old))
            }
        };
        let to = m.unwrap_or(lim);
        put(vm, k, out, o, src, i, to - i, true)?;
        let (mut i, mut o) = (to, o + (to - i));
        if m.is_some() {
            put(vm, k, out, o, rep, 0, rl, true)?;
            o += rl;
            i += pl;
            k.w[3] -= 1;
            k.owed += 2;
            if pl == 0 && i < len {
                // An empty pattern goes before each char.
                let c = ceil_char(text(vm, s), i + 1);
                put(vm, k, out, o, src, i, c - i, true)?;
                o += c - i;
                i = c;
            }
        }
        k.w[1] = i as u64;
        k.w[2] = o as u64;
    }
}

/// `str.replace` with `count` at most, a step at a time when it is long.
fn replace_big(
    vm: &mut Vm,
    s: Value,
    old: Value,
    new: Value,
    count: usize,
) -> Result<Value, Raised> {
    let pl = text(vm, old).len();
    let w = if pl == 0 {
        // No count to take: a match before each char and at the end.
        let n = count.min(vm.str_len(s).saturating_add(1));
        [0, text(vm, s).len() as u64, 0, n as u64, n as u64, 0, 0]
    } else {
        [0, 0, 0, 0, count as u64, 0, 0]
    };
    walk(vm, op::STR_REPLACE, [s, old, new, Value::NONE], w)
}

// --- split

/// The split modes (raw word 3).
pub(crate) mod sm {
    pub const SEP: u64 = 0;
    pub const SPACE: u64 = 1;
    pub const LINES: u64 = 2;
    pub const LINES_KEEP: u64 = 3;
    pub const RSEP: u64 = 4;
    pub const RSPACE: u64 = 5;
}

/// `w`: `[piece start (end from the right), search position, splits
/// left, mode, in a piece, items reversed, long piece copied]`; the list
/// is value slot 3. A piece of [`FILL`] units or more is copied a window
/// per step into its block, value slot 2: from `w[0]` (its end, from the
/// right), then the search goes on from `w[1]` (`u64::MAX`: the end).
fn split_run(vm: &mut Vm, k: &mut Walk) -> Result<Option<Value>, Raised> {
    let [s, sep, _, list] = k.v;
    let len = text(vm, s).len();
    let mode = k.w[3];
    let rev = mode >= sm::RSEP;
    loop {
        if k.spent(vm) {
            return Ok(None);
        }
        if k.v[2] != Value::NONE {
            let Some(x) = piece_run(vm, k, s, rev)? else {
                continue;
            };
            vm.list_push(list, x)?;
            (k.v[2], k.w[6]) = (Value::NONE, 0);
            match k.w[1] {
                u64::MAX if rev => k.w[5] = 1,
                u64::MAX => return Ok(Some(list)),
                n => k.w[0] = n,
            }
            continue;
        }
        let (i, j, left) = (k.at(0), k.at(1), k.at(2));
        if k.w[5] > 0 {
            // From the right: the pieces were found last first.
            let n = vm.list_len(list);
            let (Some(buf), half) = (vm.list_buf(list), n / 2) else {
                return Ok(Some(list));
            };
            let a0 = k.at(5) - 1;
            let end = half.min(a0 + k.left(vm) * 8);
            for a in a0..end {
                let b = (n - 1 - a) as u32;
                if let (Ok(x), Ok(y)) = (vm.heap.get(buf, a as u32), vm.heap.get(buf, b)) {
                    let _ = vm.heap.set_values(buf, a as u32, &[y]);
                    let _ = vm.heap.set_values(buf, b, &[x]);
                }
            }
            k.owed += (end.saturating_sub(a0) / 8) as u64 + 1;
            if end >= half {
                return Ok(Some(list));
            }
            k.w[5] = end as u64 + 1;
            continue;
        }
        let room = k.left(vm) * UNIT;
        let h = text(vm, s);
        // A piece found, and where the search goes on; `None` for the end.
        let (piece, next): (Option<(usize, usize)>, Option<usize>) = match mode {
            sm::SEP => {
                let lim = ceil_char(h, j.saturating_add(room));
                let p = text(vm, sep);
                let m = if left == 0 {
                    None
                } else {
                    k.owed += (p.len() / UNIT) as u64 + setup_units(p.len());
                    find_in(h, j, lim, p)
                };
                k.owed += ((m.unwrap_or(lim) - j) / UNIT) as u64;
                match m {
                    _ if left == 0 => (Some((i, len)), None),
                    Some(m) => {
                        k.w[2] -= 1;
                        (Some((i, m)), Some(m + p.len()))
                    }
                    None if lim >= len => (Some((i, len)), None),
                    None => {
                        k.w[1] = lim as u64;
                        (None, Some(i))
                    }
                }
            }
            sm::RSEP => {
                let p = text(vm, sep);
                // The window and a separator ending at its top: the
                // search then goes on from `lo + len - 1`, below `j` (a
                // separator longer than the room went up, for ever).
                let lo = floor_char(h, j.saturating_sub(room + p.len() - 1));
                let m = if left == 0 {
                    None
                } else {
                    k.owed += setup_units(p.len());
                    rfind_in(h, lo, j, p)
                };
                k.owed += ((j - m.unwrap_or(lo)) / UNIT) as u64;
                match m {
                    _ if left == 0 => (Some((0, i)), None),
                    Some(m) => {
                        k.w[2] -= 1;
                        (Some((m + p.len(), i)), Some(m))
                    }
                    None if lo == 0 => (Some((0, i)), None),
                    None => {
                        // A match may end past `lo`: search again from it.
                        k.w[1] = floor_char(h, lo + p.len() - 1) as u64;
                        (None, Some(i))
                    }
                }
            }
            sm::SPACE | sm::RSPACE => {
                // In a piece, look for its end (a space); between pieces,
                // for the next one's start. A char at a time, both ways.
                let inside = k.w[4] != 0;
                let lim = room / UNIT * CHAR_UNIT;
                let mut p = j;
                let mut hit = None;
                if rev {
                    let stop = j.saturating_sub(lim);
                    while p > stop {
                        let b = floor_char(h, p - 1);
                        let c = core::str::from_utf8(&h[b..p])
                            .ok()
                            .and_then(|t| t.chars().next());
                        if c.is_some_and(is_space) == inside {
                            hit = Some(p);
                            break;
                        }
                        p = b;
                    }
                } else {
                    let stop = j.saturating_add(lim).min(len);
                    while p < stop {
                        let e = ceil_char(h, p + 1);
                        let c = core::str::from_utf8(&h[p..e])
                            .ok()
                            .and_then(|t| t.chars().next());
                        if c.is_some_and(is_space) == inside {
                            hit = Some(p);
                            break;
                        }
                        p = e;
                    }
                }
                let at_end = if rev { p == 0 } else { p >= len };
                k.owed += (p.abs_diff(j) / CHAR_UNIT) as u64;
                // The far end of the text, as the piece ends there.
                let far = |x: usize| if rev { (0, x) } else { (x, len) };
                match (inside, hit) {
                    (false, Some(x)) if left == 0 => (Some(far(x)), None),
                    (false, Some(x)) => {
                        k.w[4] = 1;
                        k.w[0] = x as u64;
                        k.w[1] = x as u64;
                        (None, Some(x))
                    }
                    (false, None) if at_end => (None, None),
                    (true, Some(x)) => {
                        k.w[2] -= 1;
                        k.w[4] = 0;
                        (Some(if rev { (x, i) } else { (i, x) }), Some(x))
                    }
                    (true, None) if at_end => (Some(far(i)), None),
                    (_, None) => {
                        k.w[1] = p as u64;
                        (None, Some(i))
                    }
                }
            }
            _ => {
                // Lines: the multi-byte breaks (U+0085, U+2028, U+2029)
                // start with lead bytes, which never occur inside another
                // char.
                let lim = j.saturating_add(room / UNIT * CHAR_UNIT).min(len);
                let maybe = |x: &u8| *x <= 0x1e || *x == 0xc2 || *x == 0xe2;
                let mut at = j;
                let mut found = None;
                while let Some(x) = h.get(at..lim).and_then(|t| t.iter().position(maybe)) {
                    at += x;
                    let n = match h[at] {
                        b'\r' if h.get(at + 1) == Some(&b'\n') => 2,
                        b'\n' | b'\r' | 0x0b | 0x0c | 0x1c..=0x1e => 1,
                        0xc2 if h.get(at + 1) == Some(&0x85) => 2,
                        0xe2 if h.get(at + 1) == Some(&0x80)
                            && matches!(h.get(at + 2), Some(0xa8 | 0xa9)) =>
                        {
                            3
                        }
                        _ => {
                            at += 1;
                            continue;
                        }
                    };
                    found = Some((at, n));
                    break;
                }
                k.owed += ((found.map_or(lim, |f| f.0) - j) / CHAR_UNIT) as u64;
                match found {
                    Some((at, n)) => {
                        let end = if mode == sm::LINES_KEEP { at + n } else { at };
                        (Some((i, end)), Some(at + n))
                    }
                    None if lim >= len => ((i < len).then_some((i, len)), None),
                    None => {
                        k.w[1] = lim as u64;
                        (None, Some(i))
                    }
                }
            }
        };
        if let Some((a, b)) = piece {
            // A piece ends where it starts or after (a forged position
            // could cross it).
            let Some(n) = b.checked_sub(a) else {
                return Err(vm.bad_step());
            };
            if (n / UNIT) as u64 >= FILL {
                k.v[2] = block(vm, n)?;
                k.w[0] = if rev { b } else { a } as u64;
                k.w[1] = next.map_or(u64::MAX, |n| n as u64);
                continue;
            }
            let x = slice_copy(vm, s, a, b)?;
            vm.list_push(list, x)?;
            k.owed += 4;
        }
        match next {
            None if rev => k.w[5] = 1,
            None => return Ok(Some(list)),
            Some(n) if piece.is_some() => {
                k.w[0] = n as u64;
                k.w[1] = n as u64;
            }
            Some(_) => {}
        }
    }
}

/// A window of the long piece being copied into block `k.v[2]` (`w[6]`:
/// the bytes copied, and the chars past bit 32); the piece once whole.
fn piece_run(vm: &mut Vm, k: &mut Walk, s: Value, rev: bool) -> Result<Option<Value>, Raised> {
    let out = k.v[2];
    let (src, _, _, ascii) = info(vm, s)?;
    let n = out
        .as_byte_ref()
        .and_then(|b| vm.heap.bytes(b).ok())
        .map_or(0, <[u8]>::len);
    let a = if rev {
        k.at(0).saturating_sub(n)
    } else {
        k.at(0)
    };
    let (q, c) = ((k.w[6] & 0xFFFF_FFFF) as usize, k.w[6] >> 32);
    let m = n.saturating_sub(q).min(k.left(vm) * UNIT);
    let c = c + put(vm, k, out, q, src, a + q, m, ascii)?;
    if q + m < n {
        k.w[6] = (q + m) as u64 | c << 32;
        return Ok(None);
    }
    finish(vm, out, n, c)
}

/// A split of `s` a step at a time, in `mode`; `sep` for a separator.
fn split_big(vm: &mut Vm, s: Value, sep: Value, max: usize, mode: u64) -> Result<Value, Raised> {
    let list = vm.new_list(&[])?;
    let at = if mode >= sm::RSEP {
        text(vm, s).len() as u64
    } else {
        0
    };
    walk(
        vm,
        op::STR_SPLIT,
        [s, sep, Value::NONE, list],
        [at, at, max as u64, mode, 0, 0, 0],
    )
}

// --- maps: case maps, translate, expandtabs, extended slices

/// Whether `Σ` lowers to final `ς` at the end of `cs` (std's context
/// rule, which `str::to_lowercase` applies and `char` cannot ask).
fn sigma_probe(cs: &[char]) -> bool {
    let s: String = cs.iter().collect();
    s.to_lowercase().ends_with('ς')
}

/// Whether `c` is case-ignorable and whether it is cased, as std's
/// final-sigma rule reads them (CPython's `lower_ucs4` reads the same
/// properties); probed once per char a step, `misses` counting probes.
fn class(c: char, cache: &mut [(u32, u8); 64], misses: &mut u64) -> (bool, bool) {
    if c.is_ascii() {
        return (
            matches!(c, '\'' | '.' | ':' | '^' | '`'),
            c.is_ascii_alphabetic(),
        );
    }
    let u = u32::from(c);
    let slot = &mut cache[u as usize % 64];
    if slot.0 != u {
        *misses += 1;
        let cased = sigma_probe(&[c, 'Σ']);
        let ign = !cased && sigma_probe(&['A', c, 'Σ']);
        *slot = (u, u8::from(ign) | u8::from(cased) << 1);
    }
    (slot.1 & 1 != 0, slot.1 & 2 != 0)
}

/// Appends `c` mapped as map kind `kind` to `out`. `st` is the kind's
/// state (a column for tabs; bit 0 whether the previous char was cased
/// for `title`); `part` bytes of `c`'s output were written in an earlier
/// step, and `room` more may be now. `sigma` is set when a `Σ` was
/// lowered (to `σ`, final or not decided here). Returns whether all of
/// `c`'s output was written.
#[allow(clippy::too_many_arguments, reason = "one step's state")]
fn map_char(
    vm: &mut Vm,
    k: &Walk,
    c: char,
    pos: usize,
    st: &mut u64,
    part: &mut usize,
    room: usize,
    out: &mut String,
    sigma: &mut bool,
) -> Result<bool, Raised> {
    let case = match k.kind() {
        mk::UPPER => Case::Upper,
        mk::SWAPCASE if is_upper(c) => Case::Lower,
        mk::SWAPCASE if is_lower(c) => Case::Upper,
        mk::TITLE => {
            let case = if *st & 1 != 0 {
                Case::Lower
            } else {
                Case::Title
            };
            *st = *st & !1 | u64::from(is_lower(c) || is_upper(c) || is_title(c));
            case
        }
        mk::CAPITALIZE if pos == 0 => Case::Title,
        mk::LOWER | mk::CAPITALIZE => Case::Lower,
        mk::CASEFOLD => {
            if case_from_17(c) {
                out.push(c);
            } else if !ucd::fold(c, out) {
                out.extend(c.to_lowercase());
            }
            return Ok(true);
        }
        mk::TRANSLATE => return translate_char(vm, k.v[1], c, part, room, out),
        mk::EXPANDTABS => {
            let size = k.v[1].as_int().unwrap_or(8);
            match c {
                '\t' if size > 0 => {
                    let col = i64::try_from(*st).unwrap_or(0);
                    let n = usize::try_from(size - col % size).unwrap_or(0);
                    let m = n.saturating_sub(*part).min(room);
                    out.extend(core::iter::repeat_n(' ', m));
                    *part += m;
                    if *part < n {
                        return Ok(false);
                    }
                    (*part, *st) = (0, 0);
                }
                '\t' => {}
                '\n' | '\r' => {
                    out.push(c);
                    *st = 0;
                }
                _ => {
                    out.push(c);
                    *st += 1;
                }
            }
            return Ok(true);
        }
        _ => Case::Keep,
    };
    match if case_from_17(c) { Case::Keep } else { case } {
        Case::Lower if c == 'Σ' => {
            out.push('σ');
            *sigma = true;
        }
        Case::Lower => out.extend(c.to_lowercase()),
        Case::Keep => out.push(c),
        Case::Title if ucd::title(c, out) => {}
        _ => out.extend(c.to_uppercase()),
    }
    Ok(true)
}

/// `c` through `translate`'s table (a builtin dict); see [`map_char`].
fn translate_char(
    vm: &mut Vm,
    table: Value,
    c: char,
    part: &mut usize,
    room: usize,
    out: &mut String,
) -> Result<bool, Raised> {
    let key = Value::int(i64::from(u32::from(c))).unwrap_or(Value::NONE);
    match vm.dict_get(table, key)? {
        None => out.push(c),
        Some(Value::NONE) => {}
        Some(v) if Vm::is_str(v) => {
            let t = text(vm, v);
            let end = floor_char(t, part.saturating_add(room.max(4)));
            let end = if end <= *part {
                ceil_char(t, part.saturating_add(1))
            } else {
                end
            };
            out.push_str(core::str::from_utf8(t.get(*part..end).unwrap_or(&[])).unwrap_or(""));
            if end < t.len() {
                *part = end;
                return Ok(false);
            }
            *part = 0;
        }
        Some(v) => {
            let ch = vm
                .int_of(v)
                .and_then(|n| u32::try_from(n).ok())
                .and_then(char::from_u32);
            match ch {
                Some(ch) => out.push(ch),
                None => return Err(vm.need_py()),
            }
        }
    }
    Ok(true)
}

/// An ASCII case map of ASCII `text`, at byte `at` of its str.
fn ascii_map(kind: i64, text: &str, at: usize, st: &mut u64, out: &mut String) {
    for (x, b) in text.bytes().enumerate() {
        let up = match kind {
            mk::UPPER => true,
            mk::SWAPCASE => b.is_ascii_lowercase(),
            mk::TITLE => {
                let up = *st & 1 == 0;
                *st = u64::from(b.is_ascii_alphabetic());
                up
            }
            mk::CAPITALIZE => at + x == 0,
            _ => false,
        };
        out.push(char::from(if up {
            b.to_ascii_uppercase()
        } else {
            b.to_ascii_lowercase()
        }));
    }
}

/// Writes `ς` over the pending `σ` at `at` of the result (in `buf` when
/// at or after `o`, where `buf` goes).
fn final_sigma(
    vm: &mut Vm,
    out: Value,
    o: usize,
    at: usize,
    buf: &mut String,
) -> Result<(), Raised> {
    if at >= o {
        if buf.get(at - o..at - o + 2) == Some("σ") {
            buf.replace_range(at - o..at - o + 2, "ς");
        }
        return Ok(());
    }
    put_text(vm, out, at, "ς".as_bytes())
}

/// `w`: `[phase (0 sizing, 1 building), at, out, chars, state, bytes,
/// part]`; the map kind is value slot 2, its argument slot 1. For the
/// kinds that lower, state bit 1 is whether the last char that is not
/// case-ignorable was cased, and `part` the pending `σ`'s offset plus
/// one (Final_Sigma is decided by the next such char).
fn map_run(vm: &mut Vm, k: &mut Walk) -> Result<Option<Value>, Raised> {
    if k.kind() == mk::SLICE {
        return slice_run(vm, k);
    }
    let (s, kind) = (k.v[0], k.kind());
    let len = text(vm, s).len();
    let quoted = kind >= mk::REPR;
    let lowering = matches!(kind, mk::LOWER | mk::SWAPCASE | mk::TITLE | mk::CAPITALIZE);
    let fast = kind <= mk::CASEFOLD && vm.str_ascii(s);
    // A bytearray's repr wraps its bytes': `bytearray(b'...')`.
    let ba = usize::from(Vm::is_bytearray(s));
    let mut buf = String::new();
    let mut cache = [(0u32, 0u8); 64];
    loop {
        let (i, o, building) = (k.at(1), k.at(2), k.w[0] != 0);
        if i >= len && (lowering || k.w[6] == 0) {
            if lowering && k.w[6] != 0 {
                // A pending `σ` at the end is final.
                if building {
                    final_sigma(vm, k.v[3], o, k.at(6) - 1, &mut String::new())?;
                }
                k.w[6] = 0;
            }
            if !building {
                let mut total = k.at(5);
                let st = k.w[4];
                if quoted {
                    // CPython's quote: `"` when the text has a `'` and no
                    // `"`; the `'`s are escaped otherwise.
                    let q = if st > 1 && st & 1 == 0 { b'"' } else { b'\'' };
                    let singles = if q == b'\'' || ba == 1 { st >> 1 } else { 0 };
                    total = total
                        .saturating_add(2 + usize::from(kind == mk::BYTES_REPR) + 11 * ba)
                        .saturating_add(singles as usize);
                    k.v[1] = Value::int(i64::from(q)).unwrap_or(Value::NONE);
                }
                if !vm.heap.would_fit(total as u64) {
                    return Err(vm.err(T::MemoryError, ""));
                }
                k.v[3] = block(vm, total)?;
                k.w = [1, 0, 0, 0, 0, total as u64, 0];
                if quoted {
                    let q = u8::try_from(k.v[1].as_int().unwrap_or(0)).unwrap_or(b'\'');
                    let mut open = *b"bytearray(b'";
                    open[11] = q;
                    let skip = if ba == 1 {
                        0
                    } else {
                        10 + usize::from(kind != mk::BYTES_REPR)
                    };
                    let open = &open[skip..];
                    put_text(vm, k.v[3], 0, open)?;
                    (k.w[2], k.w[3]) = (open.len() as u64, open.len() as u64);
                }
                continue;
            }
            if quoted {
                // The closing quote (and a bytearray's parenthesis).
                let q = u8::try_from(k.v[1].as_int().unwrap_or(0)).unwrap_or(b'\'');
                let close = &[q, b')'][..=ba];
                put_text(vm, k.v[3], o, close)?;
                return finish(vm, k.v[3], o + close.len(), k.w[3] + close.len() as u64);
            }
            return finish(vm, k.v[3], o, k.w[3]);
        }
        if k.spent(vm) {
            return Ok(None);
        }
        // A slice of the text, copied: mapping can call into the arena.
        let n = k.left(vm)
            * if fast || quoted {
                CHAR_UNIT / 2 * (1 + usize::from(fast))
            } else {
                1
            };
        let h = text(vm, s);
        let e = i.saturating_add(n).max(i + 1);
        let (e, text) = if kind == mk::BYTES_REPR {
            let e = e.min(len);
            let raw = h.get(i..e).unwrap_or(&[]);
            (e, raw.iter().map(|&b| char::from(b)).collect::<String>())
        } else {
            let e = ceil_char(h, e);
            let raw = h.get(i..e).unwrap_or(&[]);
            (e, String::from(core::str::from_utf8(raw).unwrap_or("")))
        };
        if text.is_empty() {
            return Err(vm.bad_step());
        }
        let res = vm.heap.reserve_scratch(text.len() * 5);
        let _scratch = vm.hx_o(res)?;
        buf.clear();
        let (mut st, mut part) = (k.w[4], k.at(6));
        let mut done = e - i;
        let mut misses = 0;
        if fast {
            ascii_map(kind, &text, i, &mut st, &mut buf);
        } else if quoted {
            let q = building.then(|| u8::try_from(k.v[1].as_int().unwrap_or(0)).unwrap_or(b'\''));
            repr_text(&text, q, kind != mk::REPR, ba == 1, &mut st, &mut buf);
        } else {
            for (x, c) in text.char_indices() {
                let (ign, cased) = if lowering {
                    class(c, &mut cache, &mut misses)
                } else {
                    (true, false)
                };
                if lowering && !ign && part != 0 {
                    if !cased && building {
                        final_sigma(vm, k.v[3], o, part - 1, &mut buf)?;
                    }
                    part = 0;
                }
                let room = (n * 4).saturating_sub(buf.len());
                let mut sigma = false;
                if room == 0
                    || !map_char(
                        vm,
                        k,
                        c,
                        i + x,
                        &mut st,
                        &mut part,
                        room,
                        &mut buf,
                        &mut sigma,
                    )?
                {
                    done = x;
                    break;
                }
                if sigma && st & 2 != 0 {
                    part = o + buf.len() - 1;
                }
                if lowering && !ign {
                    st = st & !2 | u64::from(cased) << 1;
                }
            }
        }
        k.owed += ((if fast { done / CHAR_UNIT } else { done }) + buf.len() / UNIT) as u64
            + misses * 8
            + 1;
        (k.w[4], k.w[6]) = (st, part as u64);
        k.w[1] = (i + done) as u64;
        if building {
            if o + buf.len() > k.at(5) {
                return Err(vm.bad_step());
            }
            put_text(vm, k.v[3], o, buf.as_bytes())?;
            k.w[2] += buf.len() as u64;
            k.w[3] += char_len(&buf) as u64;
        } else {
            k.w[5] += buf.len() as u64;
        }
    }
}

/// Appends the text of `text` as a quoted repr shows it, quoted by `quote`
/// (`None` while sizing: the quotes are counted in `st`, bit 0 for a `"`
/// and the rest the `'`s, and appended as they are); `ascii` for
/// `ascii()`, which escapes every non-ASCII char too.
/// A bytearray's (`ba`) escapes every `'`, whatever its quote, as CPython's.
fn repr_text(text: &str, quote: Option<u8>, ascii: bool, ba: bool, st: &mut u64, out: &mut String) {
    let q = quote.map_or('\0', char::from);
    let mut w = crate::text::ReprWriter::new(out, usize::MAX);
    let mut start = 0;
    for (i, c) in text.char_indices() {
        let plain = if c.is_ascii() {
            (' '..'\x7f').contains(&c) && c != '\\'
        } else {
            !ascii && crate::text::is_printable(c)
        };
        if quote.is_none() && (c == '"' || c == '\'') {
            *st += if c == '"' { 1 - (*st & 1) } else { 2 };
            continue;
        }
        if plain && (quote.is_none() || c != q) && !(ba && c == '\'') {
            continue;
        }
        w.push_str(&text[start..i]);
        if c == '\'' {
            w.push_str("\\'");
        } else {
            w.escape(c, quote.unwrap_or(b'\''));
        }
        start = i + c.len_utf8();
    }
    w.push_str(&text[start..]);
}

/// `repr(s)` (`ascii(s)` with `ascii`) of a long str or bytes-like, a
/// step at a time; `None` for anything else.
pub(crate) fn repr_big(vm: &mut Vm, s: Value, ascii: bool) -> Option<Result<Value, Raised>> {
    let kind = if Vm::is_bytes_like(s) {
        mk::BYTES_REPR
    } else if ascii {
        mk::ASCII
    } else {
        mk::REPR
    };
    big_repr(vm, s).then(|| map_big(vm, s, kind, Value::NONE))
}

/// Whether `s` is a str or bytes-like whose repr goes a step at a time.
pub(crate) fn big_repr(vm: &Vm, s: Value) -> bool {
    (Vm::is_str(s) || Vm::is_bytes_like(s)) && big_text(vm, s, CHAR_UNIT / 2)
}

/// Map kind `kind` of `s` with argument `arg`, a step at a time.
fn map_big(vm: &mut Vm, s: Value, kind: i64, arg: Value) -> Result<Value, Raised> {
    let n = text(vm, s).len();
    let kv = Value::int(kind).unwrap_or(Value::NONE);
    // ASCII case maps keep the length: no sizing pass.
    if kind <= mk::CASEFOLD && vm.str_ascii(s) {
        if !vm.heap.would_fit(n as u64) {
            return Err(vm.err(T::MemoryError, ""));
        }
        let out = block(vm, n)?;
        return walk(
            vm,
            op::STR_MAP,
            [s, arg, kv, out],
            [1, 0, 0, 0, 0, n as u64, 0],
        );
    }
    walk(vm, op::STR_MAP, [s, arg, kv, Value::NONE], [0; 7])
}

/// Whether per-char work over `s` (`per` bytes a fuel unit) is big
/// enough to go a step at a time.
fn big_text(vm: &Vm, s: Value, per: usize) -> bool {
    (text(vm, s).len() / per) as u64 >= BIG
}

/// An extended slice: `w` is `[phase, at, out, chars, picks left, bytes,
/// picked any]`; value slot 1 is `(step, first pick's offset, picks)`.
fn slice_run(vm: &mut Vm, k: &mut Walk) -> Result<Option<Value>, Raised> {
    let s = k.v[0];
    let arg = vm.seq_words(k.v[1]);
    let arg: [i64; 3] = core::array::from_fn(|j| {
        arg.get(j)
            .and_then(|&w| Value::from_bits(w))
            .and_then(Value::as_int)
            .unwrap_or(0)
    });
    let [step, first, picks] = arg;
    let ascii = vm.str_ascii(s);
    let mut buf = Vec::new();
    loop {
        let (mut at, o, left, building) = (k.at(1), k.at(2), k.at(4), k.w[0] != 0);
        if left == 0 {
            if building {
                return finish(vm, k.v[3], o, k.w[3]);
            }
            let total = k.at(5);
            if !vm.heap.would_fit(total as u64) {
                return Err(vm.err(T::MemoryError, ""));
            }
            k.v[3] = block(vm, total)?;
            k.w = [1, first as u64, 0, 0, picks as u64, total as u64, 0];
            continue;
        }
        if step == 0 {
            return Err(vm.bad_step());
        }
        if k.spent(vm) {
            return Ok(None);
        }
        let budget = k.left(vm);
        let res = vm.heap.reserve_scratch(budget * 8 * 4);
        let _scratch = vm.hx_o(res)?;
        buf.clear();
        let h = text(vm, s);
        let (mut walked, mut taken) = (0usize, 0usize);
        while taken < left && taken < budget * 8 && walked < budget * UNIT {
            if k.w[6] != 0 || taken > 0 {
                let from = at;
                if ascii {
                    at = at.wrapping_add_signed(step as isize);
                } else {
                    let mut n = step.unsigned_abs();
                    while n > 0 {
                        at = if step > 0 {
                            at + 1
                        } else {
                            at.saturating_sub(1)
                        };
                        n -= u64::from(h.get(at).is_none_or(|x| x & 0xC0 != 0x80));
                    }
                }
                walked += at.abs_diff(from);
            }
            buf.extend_from_slice(h.get(at..ceil_char(h, at + 1)).unwrap_or(&[]));
            taken += 1;
        }
        k.owed += (walked / UNIT + taken / 8) as u64 + 1;
        k.w[6] = 1;
        k.w[1] = at as u64;
        k.w[4] -= taken as u64;
        if building {
            if o + buf.len() > k.at(5) {
                return Err(vm.bad_step());
            }
            put_text(vm, k.v[3], o, &buf)?;
            k.w[2] += buf.len() as u64;
            k.w[3] += taken as u64;
        } else {
            k.w[5] += buf.len() as u64;
        }
    }
}

/// `s[start::step]` of `picks` chars (`step` not 1, `start` a char
/// index): to the first pick from the nearest known position, then `step`
/// chars at a time, charged as one walk; a long one a step at a time.
#[inline(never)]
pub(crate) fn ext_slice(
    vm: &mut Vm,
    s: Value,
    start: i64,
    step: i64,
    picks: usize,
) -> Result<Value, Raised> {
    let first = vm.str_offset(s, usize::try_from(start).unwrap_or(0));
    // The walk is between picks: one pick never walks, and more than one
    // means `|step|` is less than the length.
    let gaps = picks.saturating_sub(1) as u64;
    let walked = gaps.saturating_mul(step.unsigned_abs());
    if gaps == 0 || (picks as u64) / 8 + walked / 64 < BIG {
        let b = text(vm, s);
        let mut out = Vec::new();
        let mut at = first;
        for j in 0..picks {
            let mut n = if j == 0 { 0 } else { step.unsigned_abs() };
            while n > 0 {
                at = if step > 0 {
                    at + 1
                } else {
                    at.saturating_sub(1)
                };
                n -= u64::from(b.get(at).is_none_or(|x| x & 0xC0 != 0x80));
            }
            out.extend_from_slice(b.get(at..ceil_char(b, at + 1)).unwrap_or(&[]));
        }
        vm.burn(at.abs_diff(first) as u64 / 64);
        return vm.new_str(core::str::from_utf8(&out).unwrap_or(""));
    }
    let arg = [
        Value::int(step).unwrap_or(Value::NONE),
        smi(first),
        smi(picks),
    ];
    let arg = vm.alloc_tuple(&arg)?;
    let kv = Value::int(mk::SLICE).unwrap_or(Value::NONE);
    if vm.str_ascii(s) {
        if !vm.heap.would_fit(picks as u64) {
            return Err(vm.err(T::MemoryError, ""));
        }
        let out = block(vm, picks)?;
        let w = [1, first as u64, 0, 0, picks as u64, picks as u64, 0];
        return walk(vm, op::STR_MAP, [s, arg, kv, out], w);
    }
    let w = [0, first as u64, 0, 0, picks as u64, 0, 0];
    walk(vm, op::STR_MAP, [s, arg, kv, Value::NONE], w)
}

// --- scans: count, predicates

/// `w`: `[at, end, count, state, any cased]`; the kind is value slot 2,
/// a substring slot 1.
fn scan_run(vm: &mut Vm, k: &mut Walk) -> Result<Option<Value>, Raised> {
    let [s, sub, _, _] = k.v;
    let kind = k.kind();
    let pl = text(vm, sub).len();
    loop {
        let (i, end) = (k.at(0), k.at(1));
        if k.spent(vm) {
            return Ok(None);
        }
        let room = k.left(vm);
        if kind == sk::STRIP {
            return strip_run(vm, k, room);
        }
        let h = text(vm, s);
        // A search's window: the step's work less the pattern, which it
        // reads past the window's end (and the two-way searcher reads in
        // full to start); at least as long as the pattern when that fits
        // the step, so a long one still moves on.
        let span = (room * UNIT)
            .saturating_sub(pl)
            .max(pl.min(room * UNIT))
            .max(UNIT);
        if kind == sk::COUNT {
            if i >= end || pl == 0 {
                return Ok(Some(smi(k.at(2))));
            }
            let h = h.get(..end).unwrap_or(h);
            let lim = ceil_char(h, i.saturating_add(span));
            let m = find_in(h, i, lim, text(vm, sub));
            let to = m.map_or(lim, |m| m + pl);
            k.w[2] = k.w[2].saturating_add(u64::from(m.is_some()));
            k.w[0] = to as u64;
            // The bytes the search read: to the match, or the window
            // and the pattern past it.
            let read = m.map_or(lim + pl, |m| m + pl) - i;
            k.owed += (read / UNIT) as u64 + 1 + setup_units(pl);
            continue;
        }
        if kind == sk::FIND || kind == sk::RFIND {
            let p = text(vm, sub);
            let h = h.get(..end).unwrap_or(h);
            let at = if kind == sk::FIND {
                if i >= end {
                    return found(vm, s, None, k.w[2]).map(Some);
                }
                let lim = ceil_char(h, i.saturating_add(span)).min(end);
                k.owed += ((lim - i + pl) / UNIT) as u64 + 1 + setup_units(pl);
                k.w[0] = lim as u64;
                find_in(h, i, lim, p)
            } else {
                let lo = floor_char(h, end.saturating_sub(span + pl)).max(i);
                k.owed += ((end - lo) / UNIT) as u64 + 1 + setup_units(pl);
                let m = rfind_in(h, lo, end, p);
                if m.is_none() && lo > i {
                    // A match ending past `lo` starts below it.
                    k.w[1] = ceil_char(h, lo + pl - 1).min(end) as u64;
                    continue;
                }
                k.w[1] = i as u64;
                m
            };
            if at.is_some() || (kind == sk::RFIND && k.at(1) <= i) {
                return found(vm, s, at, k.w[2]).map(Some);
            }
            continue;
        }
        let n = h.len();
        if i >= n {
            let r = match kind {
                sk::UPPER | sk::LOWER | sk::TITLE => k.w[4] != 0,
                sk::PRINTABLE => true,
                _ => n > 0,
            };
            return Ok(Some(Value::from_bool(r)));
        }
        let e = ceil_char(h, i.saturating_add(room * CHAR_UNIT));
        let t = core::str::from_utf8(h.get(i..e).unwrap_or(&[])).unwrap_or("");
        let (mut prev, mut any) = (k.w[3] != 0, k.w[4] != 0);
        let ok = t.chars().enumerate().all(|(x, c)| match kind {
            sk::DIGIT => crate::text::is_digit(c),
            sk::DECIMAL => crate::text::is_decimal(c as u32),
            sk::NUMERIC => crate::text::is_numeric(c),
            sk::ALPHA => ucd::alpha(c),
            sk::ALNUM => ucd::alnum(c),
            sk::SPACE => is_space(c),
            sk::PRINTABLE => crate::text::is_printable(c),
            sk::IDENTIFIER if i == 0 && x == 0 => c == '_' || ucd::xid_start(c),
            sk::IDENTIFIER => c == '_' || ucd::xid_continue(c),
            sk::TITLE => {
                // Cased runs start upper or title case, then lower.
                let (up, low) = (is_upper(c) || is_title(c), is_lower(c));
                let ok = !(up && prev || low && !prev);
                prev = up || low;
                any |= prev;
                ok
            }
            _ => {
                let (up, low) = (is_upper(c), is_lower(c));
                any |= up || low;
                !is_title(c) && (!(up || low) || up == (kind == sk::UPPER))
            }
        });
        k.owed += ((e - i) / CHAR_UNIT) as u64 + 1;
        if !ok {
            return Ok(Some(Value::FALSE));
        }
        (k.w[0], k.w[3], k.w[4]) = (e as u64, u64::from(prev), u64::from(any));
    }
}

/// A step of a long `strip`: `w` is `[start, end, ends to go]`; each end
/// loses its chars a window at a time, then the rest is one slice copy.
fn strip_run(vm: &mut Vm, k: &mut Walk, room: usize) -> Result<Option<Value>, Raised> {
    let [s, chars, _, _] = k.v;
    let set: Option<Vec<char>> = Vm::is_str(chars).then(|| vm.str_ref_o(chars).chars().collect());
    let per = 1 + set.as_ref().map_or(0, Vec::len) / CHAR_UNIT;
    let (at, end, sides) = (k.at(0), k.at(1), k.w[2]);
    let h = text(vm, s);
    let ascii = vm.str_ascii(s);
    let span = (room * CHAR_UNIT / per).max(4);
    let (a, e) = if sides & 1 != 0 {
        let lim = ceil_char(h, at.saturating_add(span)).min(end);
        let w = h.get(..lim).unwrap_or(&[]);
        let (a, _) = strip_span(
            w.get(at..).unwrap_or(&[]),
            ascii,
            set.as_deref(),
            true,
            false,
        );
        let a = at + a;
        k.w[2] &= if a < lim || lim == end { !1 } else { !0 };
        (a, end)
    } else {
        let lo = floor_char(h, end.saturating_sub(span)).max(at);
        let w = h.get(lo..end).unwrap_or(&[]);
        let (_, e) = strip_span(w, ascii, set.as_deref(), false, true);
        k.w[2] &= if e > 0 || lo == at { !2 } else { !0 };
        (at, lo + e)
    };
    k.owed += ((a - at + end - e) * per / CHAR_UNIT) as u64 + 1;
    (k.w[0], k.w[1]) = (a as u64, e as u64);
    if k.w[2] & 3 != 0 {
        return Ok(None);
    }
    vm.burn(core::mem::take(&mut k.owed));
    slice_steps(vm, s, a, e).map(Some)
}

/// Search kind `kind` (`FIND` or `RFIND`) for `sub` in `s` over bytes
/// `from..to`, giving `mode` (`fm`), a step at a time.
fn scan_find(
    vm: &mut Vm,
    s: Value,
    sub: Value,
    kind: i64,
    from: usize,
    to: usize,
    mode: u64,
) -> Result<Value, Raised> {
    let kv = Value::int(kind).unwrap_or(Value::NONE);
    let w = [from as u64, to as u64, mode, 0, 0, 0, 0];
    walk(vm, op::STR_SCAN, [s, sub, kv, Value::NONE], w)
}

/// Scan kind `kind` of `s` over bytes `from..to` (`sub` for a count), a
/// step at a time.
fn scan_big(
    vm: &mut Vm,
    s: Value,
    sub: Value,
    kind: i64,
    from: usize,
    to: usize,
) -> Result<Value, Raised> {
    let kv = Value::int(kind).unwrap_or(Value::NONE);
    let w = [from as u64, to as u64, 0, 0, 0, 0, 0];
    walk(vm, op::STR_SCAN, [s, sub, kv, Value::NONE], w)
}

#[cfg(all(test, feature = "unicodedata"))]
mod tests {
    use super::{case_from_17, is_lower, is_upper, uncased_in_16};
    use crate::unicodedata::category_of;

    /// `core`'s case tables may be newer than Unicode 16 (CPython 3.14's):
    /// every code point they case is assigned in 16.0 unless
    /// `uncased_in_16` lists it, each listed one is unassigned there, a
    /// code point maps to one of them only if `case_from_17` lists it, and
    /// every Ll and Lu is lower and upper case here.
    #[test]
    fn case_tables_are_unicode_16() {
        for c in (0..=0x10_FFFF).filter_map(char::from_u32) {
            let cat = category_of(u32::from(c));
            let cased = c.is_lowercase()
                || c.is_uppercase()
                || c.to_lowercase().ne(core::iter::once(c))
                || c.to_uppercase().ne(core::iter::once(c));
            let at = u32::from(c);
            if uncased_in_16(c) {
                assert_eq!(cat, "Cn", "U+{at:04X} is assigned in Unicode 16");
            } else if cased {
                assert_ne!(cat, "Cn", "U+{at:04X} is cased by core only");
            }
            let to_new = c.to_lowercase().any(uncased_in_16) || c.to_uppercase().any(uncased_in_16);
            assert!(
                !to_new || case_from_17(c),
                "U+{at:04X} maps to a code point new in 17"
            );
            assert!(
                !case_from_17(c) || uncased_in_16(c) || to_new,
                "U+{at:04X} need not be listed"
            );
            assert!(cat != "Ll" || is_lower(c), "U+{at:04X} is Ll");
            assert!(cat != "Lu" || is_upper(c), "U+{at:04X} is Lu");
        }
    }
}
