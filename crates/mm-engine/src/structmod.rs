//! `struct`'s data-proportional work, following CPython 3.14's
//! `Modules/_struct.c` (format grammar, native sizes and alignment of
//! x86-64 Linux on every target, conversions and messages; PSF license, see
//! `THIRD_PARTY.md`). `struct.py` holds the API. A function that fails with
//! `struct.error` returns the message as a `str` for `struct.py` to raise;
//! `_pack` returns `(ticket, i, kind)` when argument `i` needs conversion
//! in Python (index, float, bool or complex). The ticket holds the bytes
//! already packed, so a callback cannot change how earlier fields pack.

use crate::bytes::{JOB_WORDS, Job, R_BYTES, W_A, W_CH, W_END, W_I, W_I0, W_O, W_PASS, W_RES, job};
use crate::nstep::{STEP_WORK, op};
use crate::types::{T, ty};
use crate::vm::{CallArgs, Raised, Vm};
use alloc::{format, string::String};
use mm_core::{MAX_OBJECT_BYTES, Value};

/// One format item. Layout is computed before narrowing any object bounds.
struct Item {
    code: u8,
    num: u64,
    offset: u64,
    end: u64,
}

impl Item {
    fn values(&self) -> u64 {
        match self.code {
            b'x' => 0,
            b's' | b'p' => 1,
            _ => self.num,
        }
    }
}

fn byte_field(
    vm: &mut Vm,
    src: Value,
    at: usize,
    size: usize,
    unpack: bool,
) -> Result<Value, Raised> {
    vm.burn(1); // one field
    let mut j = vm.job_fixed(job::STRIDE, src, 0, 0, R_BYTES, size)?;
    (j.w[W_A], j.w[W_A + 1], j.w[W_A + 2]) = (at as u64, 1, u64::from(unpack));
    vm.job_run(j)
}

/// Size and alignment of `c` (`None` for a bad char).
#[cfg_attr(mm_shipped, optimize(size))]
fn code_size(c: u8, native: bool) -> Option<(usize, usize)> {
    let s = match c {
        b'x' | b'c' | b'b' | b'B' | b'?' | b's' | b'p' => 1,
        b'h' | b'H' | b'e' => 2,
        b'i' | b'I' | b'f' => 4,
        b'l' | b'L' if !native => 4,
        b'l' | b'L' | b'q' | b'Q' | b'd' | b'F' => 8,
        b'n' | b'N' | b'P' if native => 8,
        b'D' => 16,
        _ => return None,
    };
    let align = match c {
        _ if !native => 1,
        b'F' => 4,
        b'D' => 8,
        _ => s,
    };
    Some((s, align))
}

const TOO_LONG: &str = "total struct size too long";

/// Native layout, little endian, and the first byte after the prefix.
fn layout(fmt: &[u8]) -> (bool, bool, u64) {
    match fmt.first() {
        Some(b'@') => (true, true, 1),
        Some(b'=' | b'<') => (false, true, 1),
        Some(b'>' | b'!') => (false, false, 1),
        _ => (true, true, 0),
    }
}

/// Reads one item, or stops at the end of this window. `count` is zero
/// before any digits and otherwise one more than the decimal count so far.
fn scan(
    fmt: &[u8],
    end: usize,
    i: &mut u64,
    count: &mut u64,
    at: u64,
    native: bool,
) -> Result<Option<Item>, &'static str> {
    for &c in fmt.get(*i as usize..end).unwrap_or(&[]) {
        *i += 1;
        if c.is_ascii_digit() {
            *count = count
                .saturating_sub(1)
                .checked_mul(10)
                .and_then(|n| n.checked_add(u64::from(c - b'0') + 1))
                .ok_or(TOO_LONG)?;
            continue;
        }
        if *count == 0 && c.is_ascii_whitespace() {
            continue;
        }
        let num = core::mem::take(count).checked_sub(1).unwrap_or(1);
        let (size, align) = code_size(c, native).ok_or("bad char in struct format")?;
        let offset = at.next_multiple_of(align as u64);
        let end = num
            .checked_mul(size as u64)
            .and_then(|n| offset.checked_add(n))
            .filter(|&n| i64::try_from(n).is_ok())
            .ok_or(TOO_LONG)?;
        return Ok(Some(Item {
            code: c,
            num,
            offset,
            end,
        }));
    }
    if end == fmt.len() && *count != 0 {
        return Err("repeat count given without format specifier");
    }
    Ok(None)
}

fn format_len(vm: &mut Vm, v: Value) -> Result<usize, Raised> {
    if !Vm::is_str(v) && !Vm::is_bytes(v) {
        let msg = format!(
            "Struct() argument 1 must be a str or bytes object, not {}",
            vm.type_name(v)
        );
        return Err(vm.err(T::TypeError, &msg));
    }
    Ok(vm.job_data(v).len())
}

/// Public metadata calls still scan in one step.
#[cfg_attr(mm_shipped, optimize(size))]
fn format_of(vm: &mut Vm, v: Value, arity: bool) -> Result<Value, Raised> {
    let n = format_len(vm, v)?;
    vm.burn(n as u64 / 8);
    let fmt = vm.job_data(v);
    let (native, _, mut i) = layout(fmt);
    let (mut size, mut len, mut count) = (0, 0, 0);
    loop {
        match scan(fmt, n, &mut i, &mut count, size, native) {
            Ok(Some(it)) => (size, len) = (it.end, len + it.values()),
            Ok(None) => {
                let n = if arity { len } else { size };
                return vm.new_int(i64::try_from(n).unwrap_or(i64::MAX));
            }
            Err(m) => return vm.new_str(m),
        }
    }
}

/// `_calcsize(fmt)`.
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn calcsize(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    format_of(vm, vm.arg_o(&a, 0), false)
}

/// `_calclen(fmt)`: the number of values the format packs.
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn calclen(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    format_of(vm, vm.arg_o(&a, 0), true)
}

/// IEEE half precision, as `PyFloat_Pack2`; `None` on overflow.
#[cfg_attr(mm_shipped, optimize(size))]
fn pack_half(x: f64) -> Option<u16> {
    let sign = u16::from(x.is_sign_negative()) << 15;
    if x.is_nan() {
        let mant = x.to_bits() & 0x000F_FFFF_FFFF_FFFF;
        return Some(sign | 0x7C00 | ((mant >> 42) as u16 & 0x3FF));
    }
    if x.is_infinite() {
        return Some(sign | 0x7C00);
    }
    let bits = x.abs().to_bits();
    let exp = (bits >> 52) as i64;
    if exp == 0 {
        // Zero, or below 2**-1022: zero at half precision.
        return Some(sign);
    }
    // x = f * 2**e with f in [1, 2).
    let mut f = f64::from_bits((bits & 0x000F_FFFF_FFFF_FFFF) | (1023 << 52));
    let mut e = exp - 1023;
    if e >= 16 {
        return None;
    } else if e < -25 {
        f = 0.0;
        e = 0;
    } else if e < -14 {
        f *= f64::from_bits(((1023 + 14 + e) as u64) << 52);
        e = 0;
    } else {
        e += 15;
        f -= 1.0;
    }
    f *= 1024.0;
    let mut m = f as u16;
    f -= f64::from(m);
    if f > 0.5 || (f == 0.5 && m % 2 == 1) {
        m += 1;
        if m == 1024 {
            m = 0;
            e += 1;
            if e == 31 {
                return None;
            }
        }
    }
    Some(sign | ((e as u16) << 10) | m)
}

/// `PyFloat_Unpack2`.
#[cfg_attr(mm_shipped, optimize(size))]
fn unpack_half(h: u16) -> f64 {
    let neg = h & 0x8000 != 0;
    let e = i64::from((h >> 10) & 0x1F);
    let m = u64::from(h & 0x3FF);
    if e == 0x1F {
        let bits = (u64::from(neg) << 63) | (0x7FF << 52) | (m << 42);
        return f64::from_bits(bits);
    }
    let mut x = m as f64 / 1024.0;
    let e = if e == 0 {
        -14
    } else {
        x += 1.0;
        e - 15
    };
    x *= f64::from_bits(((1023 + e) as u64) << 52);
    if neg { -x } else { x }
}

/// `(float)x`'s bits, with NaN sign and payload kept on every target.
#[cfg_attr(mm_shipped, optimize(size))]
fn f32_bits(x: f64) -> u32 {
    if x.is_nan() {
        let b = x.to_bits();
        return ((b >> 32) as u32 & 0x8000_0000) | 0x7FC0_0000 | ((b >> 29) as u32 & 0x003F_FFFF);
    }
    (x as f32).to_bits()
}

#[cfg_attr(mm_shipped, optimize(size))]
fn f32_value(b: u32) -> f64 {
    let x = f32::from_bits(b);
    if x.is_nan() {
        let bits =
            (u64::from(b & 0x8000_0000) << 32) | (0x7FF << 52) | (u64::from(b & 0x007F_FFFF) << 29);
        return f64::from_bits(bits);
    }
    f64::from(x)
}

/// What packing one value produced.
enum Packed {
    Done,
    /// `struct.error` with this message.
    Error(String),
    /// Python must convert the argument with this kind first.
    Convert(u8),
}

/// The float in a float slot, if `v` is a builtin real number.
#[cfg_attr(mm_shipped, optimize(size))]
fn real_of(vm: &Vm, v: Value) -> Option<f64> {
    if let Some(f) = v.as_float() {
        return Some(f);
    }
    if Vm::is_heap_int(v) && vm.heap_int_parts(v).1.len() > 128 {
        return None;
    }
    vm.big_of(v).and_then(|b| b.to_f64())
}

#[allow(clippy::too_many_lines, reason = "one arm per format code")]
#[cfg_attr(mm_shipped, optimize(size))]
fn pack_one(vm: &Vm, code: u8, little: bool, v: Value, out: &mut [u8]) -> Packed {
    let user = vm.user_class(v).is_some();
    let put = |out: &mut [u8], le: &[u8]| {
        out.copy_from_slice(le);
        if !little {
            out.reverse();
        }
    };
    match code {
        b'c' => {
            let d = vm.bytes_data(v);
            if !Vm::is_bytes(v) || d.len() != 1 {
                return Packed::Error("char format requires a bytes object of length 1".into());
            }
            out[0] = d[0];
        }
        b'?' => {
            if user {
                return Packed::Convert(2);
            }
            out[0] = u8::from(vm.truthy(v));
        }
        b'e' | b'f' | b'd' => {
            if user {
                return Packed::Convert(1);
            }
            let Some(x) = real_of(vm, v) else {
                return Packed::Error("required argument is not a float".into());
            };
            match code {
                b'e' => match pack_half(x) {
                    Some(h) => put(out, &h.to_le_bytes()),
                    None => return Packed::Error(String::new()),
                },
                b'f' => {
                    let b = f32_bits(x);
                    if x.is_finite() && f32::from_bits(b).is_infinite() {
                        return Packed::Error(String::new());
                    }
                    put(out, &b.to_le_bytes());
                }
                _ => put(out, &x.to_le_bytes()),
            }
        }
        b'F' | b'D' => {
            if user {
                return Packed::Convert(3);
            }
            let Some((re, im)) = vm
                .complex_of(v)
                .or_else(|| real_of(vm, v).map(|x| (x, 0.0)))
            else {
                return Packed::Error("required argument is not a complex".into());
            };
            let h = out.len() / 2;
            for (k, x) in [re, im].into_iter().enumerate() {
                let part = &mut out[k * h..(k + 1) * h];
                if h == 4 {
                    put(part, &f32_bits(x).to_le_bytes());
                } else {
                    put(part, &x.to_le_bytes());
                }
            }
        }
        code => {
            if user {
                return Packed::Convert(0);
            }
            if !Vm::is_intlike(v) {
                return Packed::Error("required argument is not an integer".into());
            }
            let bits = out.len() * 8;
            let (lo, hi) = if code.is_ascii_lowercase() {
                let half = 1i128 << (bits - 1);
                (-half, half - 1)
            } else {
                (0, (1i128 << bits) - 1)
            };
            if let Some(x) = vm.int128_of(v).filter(|x| (lo..=hi).contains(x)) {
                put(out, &x.to_le_bytes()[..out.len()]);
            } else {
                return Packed::Error(format!(
                    "'{}' format requires {lo} <= number <= {hi}",
                    code as char
                ));
            }
        }
    }
    Packed::Done
}

// Value slots: format, source tuple or buffer, private result, and the
// offset argument (preflight only). W_O counts values, not bytes.
pub(crate) const MODE: usize = W_A;
pub(crate) const BASE: usize = W_A + 1;
pub(crate) const SIZE: usize = W_A + 2;
pub(crate) const LEN: usize = W_A + 3;
pub(crate) const AT: usize = W_A + 4;
pub(crate) const COUNT: usize = W_A + 5;
pub(crate) const SINGLE: usize = W_A + 6;
pub(crate) const CODE: usize = W_A + 7;
pub(crate) const REPEAT: usize = W_A + 8;
const PACK: u64 = 0;
const WHOLE: u64 = 1;
const FROM: u64 = 2;
const SCAN: u64 = 0;
const EMIT: u64 = 1;
const CONVERT: u64 = 2;

/// `_pack(fmt, args)` starts the job; `_pack(ticket, value)` consumes a
/// conversion ticket and writes the converted scalar in the same position.
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn pack(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let (fmt, src) = (vm.arg_o(&a, 0), vm.arg_o(&a, 1));
    if !fmt.is_obj_kind(ty::NSTATE as u8) {
        if !src.is_obj_kind(ty::TUPLE as u8) {
            return Err(vm.bad_step());
        }
        return vm.struct_start(fmt, src, PACK, Value::NONE);
    }
    let Some((v, [o, kind, w @ ..])) = vm.nstate_get::<4, { 2 + JOB_WORDS }>(op::BYTES, fmt) else {
        return Err(vm.bad_step());
    };
    if o != u64::from(op::BYTES) || kind != u64::from(job::STRUCT) || w[W_PASS] != CONVERT {
        return Err(vm.bad_step());
    }
    vm.nstate_set_raw(fmt, 0, u64::from(op::DONE));
    let mut j = Job {
        kind: job::STRUCT,
        v,
        w,
    };
    j.v[3] = src;
    vm.job_run(j)
}

/// `_unpack(fmt, buffer, offset, whole)`: the values, or a `struct.error`
/// message. `whole` is `unpack` (the buffer is exactly the struct);
/// otherwise `unpack_from` at `offset`.
pub(crate) fn unpack(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let (fmt, src) = (vm.arg_o(&a, 0), vm.arg_o(&a, 1));
    if vm.truthy(vm.arg_o(&a, 3)) {
        return vm.struct_start(fmt, src, WHOLE, Value::NONE);
    }
    let off = vm.arg_o(&a, 2);
    if !Vm::is_intlike(off) {
        return Err(vm.bad_step());
    }
    vm.struct_start(fmt, src, FROM, off)
}

impl Vm {
    fn struct_start(
        &mut self,
        fmt: Value,
        operand: Value,
        mode: u64,
        offset: Value,
    ) -> Result<Value, Raised> {
        let end = format_len(self, fmt)?;
        let (_, _, i) = layout(self.job_data(fmt));
        let mut j = Job::new(job::STRUCT, fmt, i as usize, end, R_BYTES);
        j.v[1] = operand;
        j.v[3] = offset;
        j.w[MODE] = mode;
        self.job_run(j)
    }

    pub(crate) fn struct_job(&mut self, j: &mut Job, start: u64) -> Result<Option<Value>, Raised> {
        let (native, little, _) = layout(self.job_data(j.v[0]));
        if j.w[W_PASS] == SCAN {
            // Also bound uncharged token and whitespace work, especially
            // runs of digits that never overflow (leading zeroes).
            let end = (j.at(W_I) + STEP_WORK as usize).min(j.at(W_END));
            self.burn((end / 8) as u64 - j.w[W_I] / 8);
            loop {
                match struct_item(self.job_data(j.v[0]), j, end, native) {
                    Ok(Some(it)) => {
                        j.w[LEN] += it.values();
                        j.w[AT] = it.end;
                        j.w[SINGLE] = if j.w[SINGLE] == 0 && it.code == b's' {
                            1
                        } else {
                            2
                        };
                    }
                    Err(m) => return self.new_str(m).map(Some),
                    Ok(None) if end != j.at(W_END) => return Ok(None),
                    Ok(None) => break,
                }
            }
            if let Some(v) = self.struct_begin(j)? {
                return Ok(Some(v));
            }
        }
        let before = j.w[AT] / 8 + j.w[W_O];
        let res = self.struct_emit(j, start, native, little, before);
        self.burn(j.w[AT] / 8 + j.w[W_O] - before);
        res
    }

    /// Call-level checks precede allocation, field errors and callbacks.
    fn struct_begin(&mut self, j: &mut Job) -> Result<Option<Value>, Raised> {
        let src = j.v[1];
        let (size, len) = (j.w[AT] as i64, j.w[LEN]);
        let pack = j.w[MODE] == PACK;
        let mut off = 0;
        if pack {
            let got = self.seq_words(src).len() as u64;
            if got != len {
                return self
                    .new_str(&format!(
                        "pack expected {len} items for packing (got {got})"
                    ))
                    .map(Some);
            }
            if size as u64 > MAX_OBJECT_BYTES || !self.heap.would_fit(size as u64 + 64) {
                return Err(self.err(T::MemoryError, ""));
            }
        } else {
            if !Self::is_bytes_like(src) {
                return Err(self.not_bytes_like(src));
            }
            let blen = self.bytes_data(src).len() as i64;
            if j.w[MODE] == WHOLE {
                if blen != size {
                    return self
                        .new_str(&format!("unpack requires a buffer of {size} bytes"))
                        .map(Some);
                }
            } else {
                let Some(n) = self.int_of_o(j.v[3]) else {
                    return Err(self.err(
                        T::OverflowError,
                        "Python int too large to convert to C ssize_t",
                    ));
                };
                off = n;
                if off < 0 {
                    if off + size > 0 {
                        let m = format!("not enough data to unpack {size} bytes at offset {off}");
                        return self.new_str(&m).map(Some);
                    }
                    if off + blen < 0 {
                        let m = format!("offset {off} out of range for {blen}-byte buffer");
                        return self.new_str(&m).map(Some);
                    }
                    off += blen;
                }
                if blen - off < size {
                    let end = size as u64 + off as u64;
                    let m = format!(
                        "unpack_from requires a buffer of at least {end} bytes for unpacking {size} bytes at offset {off} (actual buffer size is {blen})"
                    );
                    return self.new_str(&m).map(Some);
                }
            }
        }
        if j.w[SINGLE] == 1 && size as u64 / 8 >= STEP_WORK {
            let src = if pack {
                let v = self.vslot(src, 0);
                if !Self::is_bytes_like(v) {
                    return self
                        .new_str("argument for 's' must be a bytes object")
                        .map(Some);
                }
                v
            } else {
                src
            };
            return byte_field(self, src, off as usize, size as usize, !pack).map(Some);
        }
        j.v[2] = if pack {
            Value::from_byte_ref(self.job_block(size as usize)?)
        } else if len == 0 {
            self.alloc_tuple(&[])?
        } else {
            if len > MAX_OBJECT_BYTES / 8 {
                return Err(self.err(T::MemoryError, ""));
            }
            if len >= 8 {
                self.burn_result(len as usize * 8, false);
            }
            let res = self.heap.alloc_unfilled(ty::TUPLE, 0, len as u32);
            Value::from_ref(self.hx(res)?, ty::TUPLE as u8)
        };
        j.v[3] = Value::NONE;
        (j.w[W_PASS], j.w[SIZE], j.w[BASE], j.w[AT]) = (EMIT, size as u64, off as u64, 0);
        (j.w[W_I], j.w[SINGLE]) = (j.w[W_I0], 0);
        Ok(None)
    }

    #[allow(
        clippy::too_many_lines,
        reason = "advance the cursor and emit each kind of field"
    )]
    fn struct_emit(
        &mut self,
        j: &mut Job,
        start: u64,
        native: bool,
        little: bool,
        before: u64,
    ) -> Result<Option<Value>, Raised> {
        let (src, dst) = (j.v[1], j.v[2]);
        let pack = j.w[MODE] == PACK;
        let (block, tuple) = (dst.as_byte_ref(), dst.as_obj());
        let base = j.at(BASE);
        if !pack && base + j.at(SIZE) > self.bytes_data(src).len() {
            return Err(self.bad_step());
        }
        let end = (j.at(W_I) + STEP_WORK as usize).min(j.at(W_END));
        // Match the existing fuel tariff, while capping expensive scalar
        // conversions more tightly than a byte-only copy.
        while j.w[AT] / 8 + j.w[W_O] - before < self.job_left(start).min(STEP_WORK / 4) {
            if j.w[REPEAT] == 0 {
                let it = struct_item(self.job_data(j.v[0]), j, end, native)
                    .map_err(|_| self.bad_step())?;
                let Some(it) = it else {
                    if end != j.at(W_END) {
                        return Ok(None);
                    }
                    if j.w[AT] != j.w[SIZE] || j.w[W_O] != j.w[LEN] {
                        return Err(self.bad_step());
                    }
                    return if let Some(block) = block {
                        self.job_wrap(j, block, 0).map(Some)
                    } else {
                        Ok(Some(dst))
                    };
                };
                if it.end > j.w[SIZE] {
                    return Err(self.bad_step());
                }
                let n = it.values();
                if n == 0 {
                    j.w[AT] = it.end;
                    continue;
                }
                if j.w[W_O] + n > j.w[LEN] {
                    return Err(self.bad_step());
                }
                if matches!(it.code, b's' | b'p') {
                    let at = it.offset as usize;
                    let width = it.num as usize;
                    let pascal = it.code == b'p';
                    if let Some(block) = block {
                        let v = self.vslot(src, j.w[W_O] as u32);
                        if !Self::is_bytes_like(v) {
                            let m = format!(
                                "argument for '{}' must be a bytes object",
                                it.code as char
                            );
                            return self.new_str(&m).map(Some);
                        }
                        let head = usize::from(pascal && width != 0);
                        let copy = self.bytes_data(v).len().min(width - head);
                        if copy > 0 {
                            let source = self
                                .data_block(v)
                                .filter(|&b| b != block)
                                .ok_or_else(|| self.bad_step())?;
                            let res = self.heap.copy_bytes(source, 0, block, at + head, copy);
                            self.hx(res)?;
                        }
                        if head != 0 {
                            let res = self
                                .heap
                                .bytes_mut(block)
                                .map(|d| d[at] = copy.min(255) as u8);
                            self.hx(res)?;
                        }
                    } else {
                        let at = base + at;
                        let d = &self.bytes_data(src)[at..at + width];
                        let (skip, n) = if pascal {
                            (
                                1,
                                d.first()
                                    .map_or(0, |&n| usize::from(n).min(width.saturating_sub(1))),
                            )
                        } else {
                            (0, width)
                        };
                        let b = self.job_block(n)?;
                        if n != 0 {
                            let source = self.data_block(src).ok_or_else(|| self.bad_step())?;
                            let res = self.heap.copy_bytes(source, at + skip, b, 0, n);
                            self.hx(res)?;
                        }
                        let v = self.job_wrap(j, b, 0)?;
                        let tuple = tuple.ok_or_else(|| self.bad_step())?;
                        self.set_slot(tuple, j.w[W_O] as u32, v);
                    }
                    j.w[AT] = it.end;
                    j.w[W_O] += 1;
                    continue;
                }
                (j.w[AT], j.w[CODE], j.w[REPEAT]) = (it.offset, u64::from(it.code), n);
            }
            let code = j.w[CODE] as u8;
            let (size, _) = code_size(code, native).ok_or_else(|| self.bad_step())?;
            let at = j.at(AT);
            let mut wire = [0; 16];
            let wire = &mut wire[..size];
            if let Some(block) = block {
                let converted = j.w[W_PASS] == CONVERT;
                let v = if converted {
                    core::mem::replace(&mut j.v[3], Value::NONE)
                } else {
                    self.vslot(src, j.w[W_O] as u32)
                };
                match pack_one(self, code, little, v, wire) {
                    Packed::Done => j.w[W_PASS] = EMIT,
                    Packed::Error(m) if m.is_empty() => {
                        let m = format!("float too large to pack with {} format", code as char);
                        return Err(self.err(T::OverflowError, &m));
                    }
                    Packed::Error(m) => return self.new_str(&m).map(Some),
                    Packed::Convert(kind) if !converted => {
                        // `_more` marks the running state done. A different
                        // state survives as the single-use Python ticket.
                        j.w[W_PASS] = CONVERT;
                        let mut raw = [0; 2 + JOB_WORDS];
                        (raw[0], raw[1]) = (u64::from(op::BYTES), u64::from(job::STRUCT));
                        raw[2..].copy_from_slice(&j.w);
                        let ticket = self.nstate(op::BYTES, &j.v, &raw)?;
                        let i = self.new_int(j.w[W_O] as i64)?;
                        return self
                            .alloc_tuple(&[
                                ticket,
                                i,
                                Value::int(i64::from(kind)).unwrap_or(Value::NONE),
                            ])
                            .map(Some);
                    }
                    Packed::Convert(_) => return Err(self.bad_step()),
                }
                let res = self
                    .heap
                    .bytes_mut(block)
                    .map(|d| d[at..at + size].copy_from_slice(wire));
                self.hx(res)?;
            } else {
                wire.copy_from_slice(&self.bytes_data(src)[base + at..base + at + size]);
                let v = unpack_one(self, code, little, wire)?;
                let tuple = tuple.ok_or_else(|| self.bad_step())?;
                self.set_slot(tuple, j.w[W_O] as u32, v);
            }
            j.w[AT] += size as u64;
            j.w[W_O] += 1;
            j.w[REPEAT] -= 1;
        }
        Ok(None)
    }
}

fn struct_item(
    fmt: &[u8],
    j: &mut Job,
    end: usize,
    native: bool,
) -> Result<Option<Item>, &'static str> {
    let (mut i, mut count) = (j.w[W_I], j.w[COUNT]);
    let it = scan(fmt, end, &mut i, &mut count, j.w[AT], native);
    (j.w[W_I], j.w[COUNT]) = (i, count);
    it
}

fn unpack_one(vm: &mut Vm, code: u8, little: bool, wire: &mut [u8]) -> Result<Value, Raised> {
    if !little {
        wire.reverse();
    }
    let float = |b: &[u8]| -> f64 {
        match b.len() {
            2 => unpack_half(u16::from_le_bytes([b[0], b[1]])),
            4 => f32_value(u32::from_le_bytes([b[0], b[1], b[2], b[3]])),
            _ => f64::from_le_bytes(b.try_into().unwrap_or([0; 8])),
        }
    };
    match code {
        b'c' => vm.new_bytes(wire),
        b'?' => Ok(Value::from_bool(wire[0] != 0)),
        b'e' | b'f' | b'd' => Ok(Value::float(float(wire))),
        b'F' | b'D' => {
            let (a, b) = wire.split_at(wire.len() / 2);
            let (re, im) = if little { (a, b) } else { (b, a) };
            vm.new_complex(float(re), float(im))
        }
        _ => {
            let negative = code.is_ascii_lowercase() && wire[wire.len() - 1] >= 128;
            let mut bytes = [if negative { 255 } else { 0 }; 16];
            bytes[..wire.len()].copy_from_slice(wire);
            vm.new_int128(i128::from_le_bytes(bytes))
        }
    }
}

/// Restore checks stored bounds, not a mutable buffer's current length:
/// an abandoned job can outlive a later cell that shrinks its operand.
pub(crate) fn job_ok(vm: &Vm, v: &dyn Fn(usize) -> Value, w: &dyn Fn(usize) -> u64) -> bool {
    let fmt = vm.job_data(v(0));
    let (native, _, begin) = layout(fmt);
    let (i, count, mode) = (w(W_I), w(COUNT), w(MODE));
    let digit = (i > begin)
        .then(|| fmt.get((i - 1) as usize))
        .flatten()
        .filter(|d| d.is_ascii_digit());
    if !(Vm::is_str(v(0)) || Vm::is_bytes(v(0)))
        || w(W_END) != fmt.len() as u64
        || w(W_I0) != begin
        || i < begin
        || i > w(W_END)
        || w(W_CH) != 0
        || w(W_RES) != R_BYTES
        || mode > FROM
        || (count == 0) != digit.is_none()
        || digit.is_some_and(|&d| (count - 1) % 10 != u64::from(d - b'0') || i == w(W_END))
        || (REPEAT + 1..JOB_WORDS).any(|k| w(k) != 0)
    {
        return false;
    }
    let (at, len, size, base, k, reps) = (w(AT), w(LEN), w(SIZE), w(BASE), w(W_O), w(REPEAT));
    let src = v(1);
    if mode == PACK && !src.is_obj_kind(ty::TUPLE as u8) {
        return false;
    }
    if w(W_PASS) == SCAN {
        return k == 0
            && size == 0
            && base == 0
            && w(CODE) == 0
            && reps == 0
            && i64::try_from(at).is_ok()
            && len <= at + i
            && w(SINGLE) <= 2
            && v(2) == Value::NONE
            && if mode == FROM {
                Vm::is_intlike(v(3))
            } else {
                v(3) == Value::NONE
            };
    }
    if w(W_PASS) > CONVERT
        || v(3) != Value::NONE
        || w(SINGLE) != 0
        || at > size
        || k > len
        || size > MAX_OBJECT_BYTES
        || base > MAX_OBJECT_BYTES - size
        || (mode != FROM && base != 0)
    {
        return false;
    }
    let code = u8::try_from(w(CODE)).ok();
    if reps != 0 {
        let Some(c) = code.filter(|c| !matches!(c, b'x' | b's' | b'p')) else {
            return false;
        };
        let Some((width, _)) = code_size(c, native) else {
            return false;
        };
        if reps > (size - at) / width as u64
            || reps > len - k
            || count != 0
            || i.checked_sub(1).and_then(|i| fmt.get(i as usize)) != Some(&c)
        {
            return false;
        }
    } else if w(W_PASS) == CONVERT
        || code.is_none_or(|c| {
            c != 0 && (matches!(c, b'x' | b's' | b'p') || code_size(c, native).is_none())
        })
    {
        return false;
    }
    if mode == PACK {
        vm.tuple_len(src) as u64 == len
            && v(2)
                .as_byte_ref()
                .filter(|&b| Some(b) != vm.data_block(v(0)))
                .and_then(|b| vm.heap.bytes(b).ok())
                .is_some_and(|b| b.len() as u64 == size)
    } else {
        w(W_PASS) == EMIT
            && Vm::is_bytes_like(src)
            && v(2).is_obj_kind(ty::TUPLE as u8)
            && vm.tuple_len(v(2)) as u64 == len
    }
}
