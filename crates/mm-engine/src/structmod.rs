//! `struct`'s data-proportional work, following CPython 3.14's
//! `Modules/_struct.c` (format grammar, native sizes and alignment of
//! x86-64 Linux on every target, conversions and messages; PSF license, see
//! `THIRD_PARTY.md`). `struct.py` holds the API. A function that fails with
//! `struct.error` returns the message as a `str` for `struct.py` to raise;
//! `_pack` returns `(i, kind)` when argument `i` is a user object that
//! Python must convert first (kind 0 `operator.index`, 1 `float`, 2 `bool`,
//! 3 `complex`), so no user code runs from here.

use crate::bigint::Big;
use crate::scratch::{NativeValues, NativeVec};
use crate::types::T;
use crate::vm::{CallArgs, Raised, Vm};
use alloc::{format, string::String, vec::Vec};
use mm_core::{Heap, Value};

/// One format item: code, repeat count, byte size of one, offset.
struct Item {
    code: u8,
    num: usize,
    size: usize,
    offset: usize,
}

struct Format {
    /// Admitted: a format can be up to one item per byte. Their counts and
    /// offsets fit a `usize` whenever `size` does; a larger format never
    /// reaches them (no buffer is that long, and `pack` cannot allocate it).
    items: NativeVec<Item>,
    /// In `u64`, bounded by `sys.maxsize` (64-bit on every target), so
    /// `calcsize` agrees on wasm32.
    size: u64,
    /// Values packed or unpacked.
    len: u64,
    little: bool,
}

/// Size and alignment of `c` (`None` for a bad char).
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

/// [`parse`]'s error when the items do not fit the memory limit.
const NO_ROOM: &str = "";

/// Parses a format, or returns `struct.error`'s message ([`NO_ROOM`] for a
/// `MemoryError`).
fn parse(fmt: &[u8], heap: &Heap) -> Result<Format, &'static str> {
    let (native, little, rest) = match fmt.first() {
        Some(b'@') => (true, true, &fmt[1..]),
        Some(b'=' | b'<') => (false, true, &fmt[1..]),
        Some(b'>' | b'!') => (false, false, &fmt[1..]),
        _ => (true, true, fmt),
    };
    let mut items = NativeVec::default();
    let (mut size, mut len) = (0u64, 0u64);
    let mut i = 0;
    while i < rest.len() {
        let mut c = rest[i];
        i += 1;
        if c.is_ascii_whitespace() {
            continue;
        }
        let num = if c.is_ascii_digit() {
            let mut n = u64::from(c - b'0');
            loop {
                let Some(&d) = rest.get(i) else {
                    return Err("repeat count given without format specifier");
                };
                i += 1;
                if !d.is_ascii_digit() {
                    c = d;
                    break;
                }
                n = n
                    .checked_mul(10)
                    .and_then(|n| n.checked_add(u64::from(d - b'0')))
                    .ok_or(TOO_LONG)?;
            }
            n
        } else {
            1
        };
        let (s, align) = code_size(c, native).ok_or("bad char in struct format")?;
        let (s, align) = (s as u64, align as u64);
        if align > 1 && size % align != 0 {
            size = size.checked_add(align - size % align).ok_or(TOO_LONG)?;
        }
        let offset = size;
        size = num
            .checked_mul(s)
            .and_then(|n| size.checked_add(n))
            .filter(|&n| i64::try_from(n).is_ok())
            .ok_or(TOO_LONG)?;
        len += match c {
            b'x' => 0,
            b's' | b'p' => 1,
            _ => num,
        };
        items
            .push_shared(
                heap,
                Item {
                    code: c,
                    num: num as usize,
                    size: s as usize,
                    offset: offset as usize,
                },
            )
            .map_err(|_| NO_ROOM)?;
    }
    Ok(Format {
        items,
        size,
        len,
        little,
    })
}

/// A parsed format, or `Ok(Err(message))` for `struct.error`; `Err`
/// raised for a wrong type.
fn format_of(vm: &mut Vm, v: Value) -> Result<Result<Format, Value>, Raised> {
    let text = Vm::is_str(v);
    if !text && !Vm::is_bytes(v) {
        let msg = format!(
            "Struct() argument 1 must be a str or bytes object, not {}",
            vm.type_name(v)
        );
        return Err(vm.err(T::TypeError, &msg));
    }
    let n = if text {
        vm.str_bytes(v).len()
    } else {
        vm.bytes_data(v).len()
    };
    vm.burn(n as u64 / 8);
    let fmt = if text {
        vm.str_ref_o(v).as_bytes()
    } else {
        vm.bytes_data(v)
    };
    match parse(fmt, &vm.heap) {
        Ok(f) => Ok(Ok(f)),
        Err(NO_ROOM) => Err(vm.err(T::MemoryError, "")),
        Err(m) => Ok(Err(vm.new_str(m)?)),
    }
}

/// `_calcsize(fmt)`.
pub(crate) fn calcsize(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    match format_of(vm, vm.arg_o(&a, 0))? {
        Ok(f) => vm.new_int(i64::try_from(f.size).unwrap_or(i64::MAX)),
        Err(m) => Ok(m),
    }
}

/// `_calclen(fmt)`: the number of values the format packs.
pub(crate) fn calclen(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    match format_of(vm, vm.arg_o(&a, 0))? {
        Ok(f) => vm.new_int(i64::try_from(f.len).unwrap_or(i64::MAX)),
        Err(m) => Ok(m),
    }
}

/// IEEE half precision, as `PyFloat_Pack2`; `None` on overflow.
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
fn f32_bits(x: f64) -> u32 {
    if x.is_nan() {
        let b = x.to_bits();
        return ((b >> 32) as u32 & 0x8000_0000) | 0x7FC0_0000 | ((b >> 29) as u32 & 0x003F_FFFF);
    }
    (x as f32).to_bits()
}

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
fn real_of(vm: &Vm, v: Value) -> Option<f64> {
    if let Some(f) = v.as_float() {
        return Some(f);
    }
    vm.big_of(v).and_then(|b| b.to_f64())
}

#[allow(clippy::too_many_lines, reason = "one arm per format code")]
fn pack_one(vm: &mut Vm, it: &Item, little: bool, v: Value, out: &mut [u8]) -> Packed {
    let user = vm.user_class(v).is_some();
    let put = |out: &mut [u8], le: &[u8]| {
        out.copy_from_slice(le);
        if !little {
            out.reverse();
        }
    };
    match it.code {
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
            match it.code {
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
            let h = it.size / 2;
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
            if v.is_float() || vm.big_of(v).is_none() {
                return Packed::Error("required argument is not an integer".into());
            }
            let b = vm.big_of(v).unwrap_or_else(Big::zero);
            let signed = code.is_ascii_lowercase();
            if let Some(le) = b.to_bytes_le(it.size, signed) {
                put(out, &le);
            } else {
                let bits = it.size as u64 * 8;
                let one = Big::from_i64(1);
                let (lo, hi) = if signed {
                    let half = one.shl(bits - 1);
                    (format!("-{}", half.digits(10)), half.sub(&one))
                } else {
                    (String::from("0"), one.shl(bits).sub(&one))
                };
                return Packed::Error(format!(
                    "'{}' format requires {lo} <= number <= {}",
                    code as char,
                    hi.digits(10)
                ));
            }
        }
    }
    Packed::Done
}

/// `_pack(fmt, args)`: the packed `bytes`, a `struct.error` message, or
/// `(i, kind)` to convert `args[i]` first.
pub(crate) fn pack(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let f = match format_of(vm, vm.arg_o(&a, 0))? {
        Ok(f) => f,
        Err(m) => return Ok(m),
    };
    let args = vm.tuple_items(vm.arg_o(&a, 1));
    if args.len() as u64 != f.len {
        let m = format!(
            "pack expected {} items for packing (got {})",
            f.len,
            args.len()
        );
        return vm.new_str(&m);
    }
    let Some(size) = usize::try_from(f.size)
        .ok()
        .filter(|_| vm.heap.would_fit(f.size + 64))
    else {
        return Err(vm.err(T::MemoryError, ""));
    };
    vm.burn(f.size / 8 + args.len() as u64);
    let mut out = Vec::new();
    if out.try_reserve_exact(size).is_err() {
        return Err(vm.err(T::MemoryError, ""));
    }
    out.resize(size, 0);
    let mut k = 0;
    for it in &f.items {
        match it.code {
            b'x' => {}
            b's' | b'p' => {
                let v = args[k];
                if !Vm::is_bytes_like(v) {
                    let m = format!("argument for '{}' must be a bytes object", it.code as char);
                    return vm.new_str(&m);
                }
                let d = vm.bytes_data(v);
                let dst = &mut out[it.offset..it.offset + it.num];
                if it.code == b's' {
                    let n = d.len().min(it.num);
                    dst[..n].copy_from_slice(&d[..n]);
                } else if it.num > 0 {
                    let n = d.len().min(it.num - 1);
                    dst[1..=n].copy_from_slice(&d[..n]);
                    dst[0] = u8::try_from(n).unwrap_or(255);
                }
                k += 1;
            }
            _ => {
                for j in 0..it.num {
                    let at = it.offset + j * it.size;
                    match pack_one(vm, it, f.little, args[k], &mut out[at..at + it.size]) {
                        Packed::Done => {}
                        Packed::Error(m) if m.is_empty() => {
                            let m =
                                format!("float too large to pack with {} format", it.code as char);
                            return Err(vm.err(T::OverflowError, &m));
                        }
                        Packed::Error(m) => return vm.new_str(&m),
                        Packed::Convert(kind) => {
                            let i = vm.new_int(k as i64)?;
                            return vm.alloc_tuple(&[
                                i,
                                Value::int(i64::from(kind)).unwrap_or(Value::NONE),
                            ]);
                        }
                    }
                    k += 1;
                }
            }
        }
    }
    vm.new_bytes(&out)
}

/// `_unpack(fmt, buffer, offset, whole)`: the values, or a `struct.error`
/// message. `whole` is `unpack` (the buffer is exactly the struct);
/// otherwise `unpack_from` at `offset`.
pub(crate) fn unpack(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let f = match format_of(vm, vm.arg_o(&a, 0))? {
        Ok(f) => f,
        Err(m) => return Ok(m),
    };
    let buf = vm.arg_o(&a, 1);
    if !Vm::is_bytes_like(buf) {
        let m = format!(
            "a bytes-like object is required, not '{}'",
            vm.type_name(buf)
        );
        return Err(vm.err(T::TypeError, &m));
    }
    let whole = vm.truthy(vm.arg_o(&a, 3));
    let mut off = vm.int_of_o(vm.arg_o(&a, 2)).unwrap_or(0);
    let blen = vm.bytes_data(buf).len() as i64;
    let size = f.size as i64;
    if whole {
        if blen != size {
            return vm.new_str(&format!("unpack requires a buffer of {size} bytes"));
        }
    } else {
        if off < 0 {
            if off + blen < 0 {
                let m = format!("offset {off} out of range for {blen}-byte buffer");
                return vm.new_str(&m);
            }
            if -off < size {
                let m = format!("not enough data to unpack {size} bytes at offset {off}");
                return vm.new_str(&m);
            }
            off += blen;
        }
        if blen - off < size {
            let m = format!(
                "unpack_from requires a buffer of at least {} bytes for unpacking {size} bytes \
                 at offset {off} (actual buffer size is {blen})",
                size.saturating_add(off)
            );
            return vm.new_str(&m);
        }
    }
    vm.burn(f.size / 8 + f.len);
    let base = usize::try_from(off).unwrap_or(0);
    let mut values = NativeValues::default();
    let res = values.reserve(&mut vm.heap, f.len as usize);
    vm.hx_o(res)?;
    for it in &f.items {
        let at = base + it.offset;
        match it.code {
            b'x' => {}
            b's' => {
                let d = vm.bytes_data(buf)[at..at + it.num].to_vec();
                let v = vm.new_bytes(&d)?;
                let res = values.push(&mut vm.heap, v);
                vm.hx_o(res)?;
            }
            b'p' => {
                let d = &vm.bytes_data(buf)[at..at + it.num];
                let n = d
                    .first()
                    .map_or(0, |&n| usize::from(n).min(it.num.saturating_sub(1)));
                let d = d.get(1..=n).unwrap_or(&[]).to_vec();
                let v = vm.new_bytes(&d)?;
                let res = values.push(&mut vm.heap, v);
                vm.hx_o(res)?;
            }
            code => {
                for j in 0..it.num {
                    let s = at + j * it.size;
                    let mut le = vm.bytes_data(buf)[s..s + it.size].to_vec();
                    if !f.little {
                        le.reverse();
                    }
                    let float = |b: &[u8]| -> f64 {
                        match b.len() {
                            2 => unpack_half(u16::from_le_bytes([b[0], b[1]])),
                            4 => f32_value(u32::from_le_bytes([b[0], b[1], b[2], b[3]])),
                            _ => f64::from_le_bytes(b.try_into().unwrap_or([0; 8])),
                        }
                    };
                    let v = match code {
                        b'c' => vm.new_bytes(&le)?,
                        b'?' => Value::from_bool(le[0] != 0),
                        b'e' | b'f' | b'd' => Value::float(float(&le)),
                        b'F' | b'D' => {
                            let h = it.size / 2;
                            // Big-endian parts were reversed as a whole.
                            let (re, im) = if f.little {
                                (float(&le[..h]), float(&le[h..]))
                            } else {
                                (float(&le[h..]), float(&le[..h]))
                            };
                            vm.new_complex(re, im)?
                        }
                        _ => {
                            let b = Big::from_bytes_le(&le, code.is_ascii_lowercase());
                            vm.new_big(&b)?
                        }
                    };
                    let res = values.push(&mut vm.heap, v);
                    vm.hx_o(res)?;
                }
            }
        }
    }
    vm.alloc_tuple(&values)
}
