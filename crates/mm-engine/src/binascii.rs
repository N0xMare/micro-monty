//! `binascii`'s hot loops: base64, hex and the CRCs, following CPython's
//! `Modules/binascii.c` (scan rules and messages; PSF license, see
//! `THIRD_PARTY.md`). `binascii.py` holds the API, the argument checks and
//! the rarer codecs. A decoder that fails returns the message as a `str`
//! for `binascii.py` to raise as `binascii.Error`.

use crate::bytes::{Feed, Job, Out, R_BYTES, W_A, W_C, job};
use crate::vm::{CallArgs, Raised, Vm};
use mm_core::Value;

pub(crate) const B64: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// The reflected CRC-32 table (polynomial 0xEDB88320).
const CRC32: [u32; 256] = {
    let mut t = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut c = i as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
            k += 1;
        }
        t[i] = c;
        i += 1;
    }
    t
};

/// The data argument: `bytes` (or an ASCII `str` where the caller allows).
#[cfg_attr(mm_shipped, optimize(size))]
fn data(vm: &Vm, v: Value) -> &[u8] {
    vm.job_data(v)
}

#[cfg_attr(mm_shipped, optimize(size))]
fn b64_value(c: u8) -> Option<u8> {
    match c {
        b'A'..=b'Z' => Some(c - b'A'),
        b'a'..=b'z' => Some(c - b'a' + 26),
        b'0'..=b'9' => Some(c - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

/// `_b2a_base64(data, newline)`.
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn b2a_base64(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let v = vm.arg_o(&a, 0);
    let n = data(vm, v).len();
    let nl = vm.arg_o(&a, 1) == Value::TRUE;
    let len = n.div_ceil(3) * 4 + usize::from(nl);
    let mut j = vm.job_fixed(job::B64E, v, 0, n, R_BYTES, len)?;
    j.w[W_A] = u64::from(nl);
    vm.job_run(j)
}

/// `_a2b_base64(data, strict)`: the bytes, or the error message.
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn a2b_base64(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let v = vm.arg_o(&a, 0);
    let n = data(vm, v).len();
    let mut j = Job::new(job::B64D, v, 0, n, R_BYTES);
    j.w[W_A] = u64::from(vm.arg_o(&a, 1) == Value::TRUE);
    vm.job_run(j)
}

/// A window `d` of a base64 decode (`first` if it starts the data): the
/// quad position, the bits left over and the pad count carried in `st`.
/// `Some` when it ends the decode: a message, or nothing more to read.
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn b64_feed(
    vm: &mut Vm,
    d: &[u8],
    first: bool,
    strict: bool,
    st: &mut u64,
    out: &mut Out,
) -> Result<Option<Feed>, Raised> {
    let (mut quad, mut left, mut pads) = (*st & 0xff, (*st >> 8 & 0xff) as u8, *st >> 16);
    let mut r = None;
    for (i, &c) in d.iter().enumerate() {
        if c == b'=' {
            pads += 1;
            if (quad >= 2 && quad + pads <= 4) || !strict {
                continue;
            }
            if quad == 1 {
                r = Some(Feed::Stop);
                break;
            }
            return Ok(Some(Feed::Msg(if quad == 0 && i == 0 && first {
                "Leading padding not allowed"
            } else {
                "Excess padding not allowed"
            })));
        }
        let Some(v) = b64_value(c) else {
            if strict {
                return Ok(Some(Feed::Msg("Only base64 data is allowed")));
            }
            continue;
        };
        if pads != 0 && strict {
            return Ok(Some(Feed::Msg(if quad + pads == 4 {
                "Excess data after padding"
            } else {
                "Discontinuous padding not allowed"
            })));
        }
        pads = 0;
        match quad {
            0 => left = v,
            1 => {
                out.byte(vm, (left << 2) | (v >> 4))?;
                left = v & 0x0f;
            }
            2 => {
                out.byte(vm, (left << 4) | (v >> 2))?;
                left = v & 0x03;
            }
            _ => out.byte(vm, (left << 6) | v)?,
        }
        quad = (quad + 1) % 4;
    }
    *st = quad | u64::from(left) << 8 | pads << 16;
    Ok(r)
}

/// `_hexlify(data)`.
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn hexlify(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let v = vm.arg_o(&a, 0);
    let n = data(vm, v).len();
    let j = vm.job_fixed(job::HEX, v, 0, n, R_BYTES, n.saturating_mul(2))?;
    vm.job_run(j)
}

/// `_unhexlify(data)`: the bytes, or the error message.
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn unhexlify(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let v = vm.arg_o(&a, 0);
    let n = data(vm, v).len();
    if !n.is_multiple_of(2) {
        return vm.new_str("Odd-length string");
    }
    let j = vm.job_fixed(job::UNHEX, v, 0, n, R_BYTES, n / 2)?;
    vm.job_run(j)
}

/// A checksum job over `data` from carry `acc`.
#[cfg_attr(mm_shipped, optimize(size))]
fn checksum(vm: &mut Vm, a: &CallArgs, kind: u8, acc: u64) -> Result<Value, Raised> {
    let v = vm.arg_o(a, 0);
    let n = data(vm, v).len();
    let mut j = Job::new(kind, v, 0, n, 0);
    j.w[W_C] = acc;
    vm.job_run(j)
}

/// `_crc32(data, crc)` with `crc` already reduced modulo 2**32.
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn crc32(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let crc = vm.int_arg(vm.arg_o(&a, 1), "")? as u32;
    checksum(vm, &a, job::CRC32, u64::from(!crc))
}

/// `_adler32(data, value)` (zlib's Adler-32) with `value` already reduced
/// modulo 2**32.
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn adler32(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let v = vm.int_arg(vm.arg_o(&a, 1), "")? as u32;
    // Reduced first: with no data the feed never reduces them.
    let (lo, hi) = ((v & 0xffff) % 65521, (v >> 16) % 65521);
    checksum(vm, &a, job::ADLER, u64::from(lo) | u64::from(hi) << 32)
}

/// `_crc_hqx(data, crc)` (CRC-CCITT, polynomial 0x1021) with `crc` already
/// reduced to 16 bits.
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn crc_hqx(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let crc = vm.int_arg(vm.arg_o(&a, 1), "")? & 0xffff;
    checksum(vm, &a, job::HQX, crc as u64)
}

/// Feeds `d` to checksum `kind`'s carry `acc`.
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn crc_feed(kind: u8, acc: &mut u64, d: &[u8]) {
    match kind {
        job::CRC32 => {
            let mut c = *acc as u32;
            for &b in d {
                c = CRC32[((c ^ u32::from(b)) & 0xff) as usize] ^ (c >> 8);
            }
            *acc = u64::from(c);
        }
        job::ADLER => {
            let (mut lo, mut hi) = (*acc as u32, (*acc >> 32) as u32);
            // 5552 bytes is the longest run whose sums cannot overflow u32.
            for chunk in d.chunks(5552) {
                for &b in chunk {
                    lo += u32::from(b);
                    hi += lo;
                }
                lo %= 65521;
                hi %= 65521;
            }
            *acc = u64::from(lo) | u64::from(hi) << 32;
        }
        _ => {
            let mut crc = *acc as u16;
            for &b in d {
                crc ^= u16::from(b) << 8;
                for _ in 0..8 {
                    crc = if crc & 0x8000 != 0 {
                        (crc << 1) ^ 0x1021
                    } else {
                        crc << 1
                    };
                }
            }
            *acc = u64::from(crc);
        }
    }
}

/// Checksum `kind`'s value from its carry.
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn crc_end(kind: u8, acc: u64) -> i64 {
    match kind {
        job::CRC32 => i64::from(!(acc as u32)),
        job::ADLER => i64::from((acc >> 32) as u32) << 16 | (acc & 0xffff_ffff) as i64,
        _ => acc as i64,
    }
}
