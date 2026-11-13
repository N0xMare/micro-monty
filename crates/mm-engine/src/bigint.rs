//! Arbitrary-precision integers (docs/spec.md §7.2): sign plus magnitude in
//! little-endian `u32` limbs. Pure arithmetic, no heap access; the VM stores
//! a `Big` as a `ty::INT` object whose limbs live in the byte space.
//!
//! Algorithms: schoolbook addition and subtraction; schoolbook
//! multiplication with Karatsuba above [`KARATSUBA`] limbs; Knuth's
//! algorithm D for division; square-and-multiply powers; two's-complement
//! views for bitwise operators; radix conversion by repeated division by the
//! largest power of the radix that fits a limb (bit slicing for powers of
//! two).

use alloc::{string::String, vec, vec::Vec};
use core::cmp::Ordering;

/// Limb count above which multiplication switches to Karatsuba.
const KARATSUBA: usize = 40;

/// CPython's numeric hash modulus, 2^61 - 1.
const MODULUS: u64 = (1 << 61) - 1;

/// CPython's hash of an int from its sign and little-endian magnitude
/// bytes, read in place: `sign * (|v| mod (2^61 - 1))`, with -1 as -2.
pub fn hash_le(neg: bool, mag: &[u8]) -> i64 {
    // Most significant limb first, `x` kept below the modulus: as
    // `2^61 = 1` modulo the Mersenne prime, `x * 2^32` is its low 29 bits
    // shifted up plus its high bits.
    let (limbs, top) = mag.as_chunks::<4>();
    let mut x = top.iter().rev().fold(0u64, |a, &b| (a << 8) | u64::from(b));
    for l in limbs.iter().rev() {
        let s = (((x << 32) & MODULUS) | u64::from(u32::from_le_bytes(*l))) + (x >> 29);
        x = if s >= MODULUS { s - MODULUS } else { s };
    }
    let h = x.cast_signed();
    let h = if neg { -h } else { h };
    if h == -1 { -2 } else { h }
}

/// An integer of any size. `mag` has no high zero limbs; zero is an empty
/// `mag` with `neg == false`.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub(crate) struct Big {
    pub neg: bool,
    pub mag: Vec<u32>,
}

fn trim(v: &mut Vec<u32>) {
    while v.last() == Some(&0) {
        v.pop();
    }
}

fn mag_cmp(a: &[u32], b: &[u32]) -> Ordering {
    if a.len() != b.len() {
        return a.len().cmp(&b.len());
    }
    for i in (0..a.len()).rev() {
        if a[i] != b[i] {
            return a[i].cmp(&b[i]);
        }
    }
    Ordering::Equal
}

fn mag_add(a: &[u32], b: &[u32]) -> Vec<u32> {
    let (a, b) = if a.len() >= b.len() { (a, b) } else { (b, a) };
    let mut out = Vec::with_capacity(a.len() + 1);
    let mut carry = 0u64;
    for (i, &x) in a.iter().enumerate() {
        let s = u64::from(x) + u64::from(b.get(i).copied().unwrap_or(0)) + carry;
        out.push(s as u32);
        carry = s >> 32;
    }
    if carry != 0 {
        out.push(carry as u32);
    }
    out
}

/// `a - b` for `a >= b`.
fn mag_sub(a: &[u32], b: &[u32]) -> Vec<u32> {
    let mut out = Vec::with_capacity(a.len());
    let mut borrow = 0i64;
    for (i, &x) in a.iter().enumerate() {
        let mut d = i64::from(x) - i64::from(b.get(i).copied().unwrap_or(0)) - borrow;
        if d < 0 {
            d += 1 << 32;
            borrow = 1;
        } else {
            borrow = 0;
        }
        out.push(d as u32);
    }
    trim(&mut out);
    out
}

/// `acc[at..] += b`, with carry propagation (acc is long enough).
fn add_into(acc: &mut [u32], b: &[u32], at: usize) {
    let mut carry = 0u64;
    let mut i = 0;
    while i < b.len() || carry != 0 {
        let Some(slot) = acc.get_mut(at + i) else {
            return;
        };
        let s = u64::from(*slot) + u64::from(b.get(i).copied().unwrap_or(0)) + carry;
        *slot = s as u32;
        carry = s >> 32;
        i += 1;
    }
}

fn mul_school(a: &[u32], b: &[u32]) -> Vec<u32> {
    // The shorter operand drives the outer loop, so a long value times a
    // small one is one tight pass, not a carry loop per limb.
    let (a, b) = if a.len() > b.len() { (b, a) } else { (a, b) };
    let mut out = vec![0u32; a.len() + b.len()];
    for (i, &x) in a.iter().enumerate() {
        if x == 0 {
            continue;
        }
        let mut carry = 0u64;
        for (j, &y) in b.iter().enumerate() {
            let t = u64::from(x) * u64::from(y) + u64::from(out[i + j]) + carry;
            out[i + j] = t as u32;
            carry = t >> 32;
        }
        let mut k = i + b.len();
        while carry != 0 && k < out.len() {
            let t = u64::from(out[k]) + carry;
            out[k] = t as u32;
            carry = t >> 32;
            k += 1;
        }
    }
    trim(&mut out);
    out
}

/// Magnitude product; the recursion depth is log2 of the operand size.
fn mag_mul(a: &[u32], b: &[u32]) -> Vec<u32> {
    if a.is_empty() || b.is_empty() {
        return Vec::new();
    }
    if a.len() < KARATSUBA || b.len() < KARATSUBA {
        return mul_school(a, b);
    }
    // Unbalanced operands: split the longer into chunks of the shorter.
    let (a, b) = if a.len() >= b.len() { (a, b) } else { (b, a) };
    if a.len() >= 2 * b.len() {
        let mut out = vec![0u32; a.len() + b.len() + 1];
        let mut at = 0;
        while at < a.len() {
            let end = (at + b.len()).min(a.len());
            let mut chunk = a[at..end].to_vec();
            trim(&mut chunk);
            let p = mag_mul(&chunk, b);
            add_into(&mut out, &p, at);
            at = end;
        }
        trim(&mut out);
        return out;
    }
    let half = a.len().div_ceil(2);
    let split = |x: &[u32]| -> (Vec<u32>, Vec<u32>) {
        if x.len() <= half {
            let mut lo = x.to_vec();
            trim(&mut lo);
            return (lo, Vec::new());
        }
        let mut lo = x[..half].to_vec();
        trim(&mut lo);
        let mut hi = x[half..].to_vec();
        trim(&mut hi);
        (lo, hi)
    };
    let (a0, a1) = split(a);
    let (b0, b1) = split(b);
    let z0 = mag_mul(&a0, &b0);
    let z2 = mag_mul(&a1, &b1);
    let z1 = mag_mul(&mag_add(&a0, &a1), &mag_add(&b0, &b1));
    let mut mid = mag_sub(&z1, &z0);
    mid = mag_sub(&mid, &z2);
    let mut out = vec![0u32; a.len() + b.len() + 1];
    add_into(&mut out, &z0, 0);
    add_into(&mut out, &mid, half);
    add_into(&mut out, &z2, 2 * half);
    trim(&mut out);
    out
}

/// `a / d` and `a % d` for a divisor `d > 0` of one or two limbs: a short
/// division of 64-bit words by `d` normalized, each quotient word by the
/// precomputed reciprocal (Möller and Granlund, "Improved division by
/// invariant integers", 2011, algorithm 4), multiplications only: the
/// int cap by any such `d` in a few ms (algorithm D's estimate refused
/// two limbs there). Without `keep_q` the quotient is not kept (empty).
fn divmod_word(a: &[u32], d: u64, keep_q: bool) -> (Vec<u32>, u64) {
    let word = |i: usize| {
        let lo = a.get(2 * i).copied().unwrap_or(0);
        let hi = a.get(2 * i + 1).copied().unwrap_or(0);
        u64::from(lo) | (u64::from(hi) << 32)
    };
    let n = a.len().div_ceil(2);
    let s = d.leading_zeros();
    let dn = d << s;
    // floor((2^128 - 1) / dn) - 2^64; `dn`'s top bit is set.
    let v = (((u128::from(!dn) << 64) | u128::from(u64::MAX)) / u128::from(dn)) as u64;
    let below = |i: usize| {
        if s == 0 || i == 0 {
            0
        } else {
            word(i - 1) >> (64 - s)
        }
    };
    let mut q = vec![0u32; if keep_q { 2 * n } else { 0 }];
    // The bits the shift moves above the top word, under `dn`.
    let mut r = if s == 0 || n == 0 {
        0
    } else {
        word(n - 1) >> (64 - s)
    };
    for i in (0..n).rev() {
        let u0 = (word(i) << s) | below(i);
        // `v * r + (r, u0)`, modulo 2^128.
        let p =
            (u128::from(v) * u128::from(r)).wrapping_add((u128::from(r) << 64) | u128::from(u0));
        let mut q1 = ((p >> 64) as u64).wrapping_add(1);
        let mut rem = u0.wrapping_sub(q1.wrapping_mul(dn));
        if rem > p as u64 {
            q1 = q1.wrapping_sub(1);
            rem = rem.wrapping_add(dn);
        }
        if rem >= dn {
            q1 += 1;
            rem -= dn;
        }
        if keep_q {
            q[2 * i] = q1 as u32;
            q[2 * i + 1] = (q1 >> 32) as u32;
        }
        r = rem;
    }
    trim(&mut q);
    (q, r >> s)
}

/// `a / d` and `a % d` for a divisor `d` of three or four limbs
/// (`2^64 <= d < 2^128`): as [`divmod_word`], a quotient word at a time
/// from three words of the remainder and dividend, by the reciprocal of
/// `d` normalized (Möller and Granlund, algorithms 5 and 6).
fn divmod_dword(a: &[u32], d: u128, keep_q: bool) -> (Vec<u32>, u128) {
    let word = |i: usize| {
        let lo = a.get(2 * i).copied().unwrap_or(0);
        let hi = a.get(2 * i + 1).copied().unwrap_or(0);
        u64::from(lo) | (u64::from(hi) << 32)
    };
    let n = a.len().div_ceil(2);
    // `d >= 2^64`: under 64 bits of shift.
    let s = d.leading_zeros();
    let dn = d << s;
    let (d1, d0) = ((dn >> 64) as u64, dn as u64);
    // Algorithm 6: the reciprocal of `(d1, d0)` from `d1`'s.
    let mut v = (((u128::from(!d1) << 64) | u128::from(u64::MAX)) / u128::from(d1)) as u64;
    let mut p = d1.wrapping_mul(v).wrapping_add(d0);
    if p < d0 {
        v = v.wrapping_sub(1);
        if p >= d1 {
            v = v.wrapping_sub(1);
            p = p.wrapping_sub(d1);
        }
        p = p.wrapping_sub(d1);
    }
    let t = u128::from(v) * u128::from(d0);
    let (t1, t0) = ((t >> 64) as u64, t as u64);
    p = p.wrapping_add(t1);
    if p < t1 {
        v = v.wrapping_sub(1);
        if ((u128::from(p) << 64) | u128::from(t0)) >= dn {
            v = v.wrapping_sub(1);
        }
    }
    let below = |i: usize| {
        if s == 0 || i == 0 {
            0
        } else {
            word(i - 1) >> (64 - s)
        }
    };
    let mut q = vec![0u32; if keep_q { 2 * n } else { 0 }];
    let mut r: u128 = if s == 0 || n == 0 {
        0
    } else {
        u128::from(word(n - 1) >> (64 - s))
    };
    for i in (0..n).rev() {
        // Algorithm 5: `(u2, u1, u0) / (d1, d0)`, `(u2, u1) = r < dn`.
        let u0 = (word(i) << s) | below(i);
        let (u2, u1) = ((r >> 64) as u64, r as u64);
        let qq = (u128::from(v) * u128::from(u2)).wrapping_add(r);
        let (mut q1, q0) = ((qq >> 64) as u64, qq as u64);
        let r1 = u1.wrapping_sub(q1.wrapping_mul(d1));
        let t = u128::from(d0) * u128::from(q1);
        let mut rem = ((u128::from(r1) << 64) | u128::from(u0))
            .wrapping_sub(t)
            .wrapping_sub(dn);
        q1 = q1.wrapping_add(1);
        if (rem >> 64) as u64 >= q0 {
            q1 = q1.wrapping_sub(1);
            rem = rem.wrapping_add(dn);
        }
        if rem >= dn {
            q1 = q1.wrapping_add(1);
            rem -= dn;
        }
        if keep_q {
            q[2 * i] = q1 as u32;
            q[2 * i + 1] = (q1 >> 32) as u32;
        }
        r = rem;
    }
    trim(&mut q);
    (q, r >> s)
}

fn shl_bits(a: &[u32], s: u32, extra: bool) -> Vec<u32> {
    let mut out = Vec::with_capacity(a.len() + 1);
    if s == 0 {
        out.extend_from_slice(a);
        if extra {
            out.push(0);
        }
        return out;
    }
    let mut carry = 0u32;
    for &x in a {
        out.push((x << s) | carry);
        carry = x >> (32 - s);
    }
    if extra {
        out.push(carry);
    }
    out
}

/// `a / b` and `a % b` for a non-zero `b` of up to four limbs (128 bits):
/// a short division ([`divmod_word`], [`divmod_dword`]); the quotient only
/// with `keep_q`.
fn short_divmod(a: &[u32], b: &[u32], keep_q: bool) -> (Vec<u32>, Vec<u32>) {
    let d = b
        .iter()
        .rev()
        .fold(0u128, |d, &x| (d << 32) | u128::from(x));
    let (q, r) = match u64::try_from(d) {
        Ok(d) => {
            let (q, r) = divmod_word(a, d, keep_q);
            (q, u128::from(r))
        }
        Err(_) => divmod_dword(a, d, keep_q),
    };
    let mut r = (0..4).map(|i| (r >> (32 * i)) as u32).collect();
    trim(&mut r);
    (q, r)
}

/// Truncating magnitude division (Knuth, TAOCP vol. 2, 4.3.1, algorithm D).
/// `b` is non-empty.
fn mag_divmod(a: &[u32], b: &[u32]) -> (Vec<u32>, Vec<u32>) {
    if mag_cmp(a, b) == Ordering::Less {
        return (Vec::new(), a.to_vec());
    }
    if b.len() <= 4 {
        return short_divmod(a, b, true);
    }
    let n = b.len();
    let s = b[n - 1].leading_zeros();
    let v = shl_bits(b, s, false);
    let mut u = shl_bits(a, s, true);
    let m = u.len() - n;
    let mut q = vec![0u32; m];
    let vtop = u64::from(v[n - 1]);
    let vnext = u128::from(v[n - 2]);
    for j in (0..m).rev() {
        let num = (u64::from(u[j + n]) << 32) | u64::from(u[j + n - 1]);
        let mut qhat = num / vtop;
        let mut rhat = num % vtop;
        while qhat >> 32 != 0
            || u128::from(qhat) * vnext > ((u128::from(rhat) << 32) | u128::from(u[j + n - 2]))
        {
            qhat -= 1;
            rhat += vtop;
            if rhat >> 32 != 0 {
                break;
            }
        }
        let mut borrow = 0i64;
        let mut carry = 0u64;
        for i in 0..n {
            let p = qhat * u64::from(v[i]) + carry;
            carry = p >> 32;
            let t = i64::from(u[i + j]) - borrow - (p & 0xFFFF_FFFF) as i64;
            u[i + j] = t as u32;
            borrow = i64::from(t < 0);
        }
        let t = i64::from(u[j + n]) - borrow - carry as i64;
        u[j + n] = t as u32;
        if t < 0 {
            qhat -= 1;
            let mut c = 0u64;
            for i in 0..n {
                let s2 = u64::from(u[i + j]) + u64::from(v[i]) + c;
                u[i + j] = s2 as u32;
                c = s2 >> 32;
            }
            u[j + n] = u[j + n].wrapping_add(c as u32);
        }
        q[j] = qhat as u32;
    }
    trim(&mut q);
    // Remainder: the low n limbs of u, shifted back.
    let mut r = vec![0u32; n];
    for i in 0..n {
        let lo = u[i] >> s;
        let hi = if s == 0 || i + 1 >= u.len() {
            0
        } else {
            u[i + 1] << (32 - s)
        };
        r[i] = lo | hi;
    }
    trim(&mut r);
    (q, r)
}

/// The digits of magnitude `cur` in `radix`, least significant first, into
/// `out`: `cur` divided in place by `chunk` (`radix` to the `per`) until
/// zero.
#[inline]
fn radix_digits(cur: &mut Vec<u32>, chunk: u32, per: usize, radix: u32, out: &mut Vec<u8>) {
    let d = u64::from(chunk);
    while !cur.is_empty() {
        let mut r = 0u64;
        for x in cur.iter_mut().rev() {
            let v = (r << 32) | u64::from(*x);
            *x = (v / d) as u32;
            r = v % d;
        }
        trim(cur);
        let mut r = r as u32;
        for _ in 0..per {
            out.push(b"0123456789abcdefghijklmnopqrstuvwxyz"[(r % radix) as usize]);
            r /= radix;
            if cur.is_empty() && r == 0 {
                break;
            }
        }
    }
}

/// The value of digit `c` in bases up to 36.
pub fn digit_value(c: u8) -> Option<u32> {
    match c {
        b'0'..=b'9' => Some(u32::from(c - b'0')),
        b'a'..=b'z' => Some(u32::from(c - b'a') + 10),
        b'A'..=b'Z' => Some(u32::from(c - b'A') + 10),
        _ => None,
    }
}

/// The largest power of `radix` (2..=36) that fits a limb, and its
/// exponent: the digits one multiply-add takes.
pub fn radix_chunk(radix: u32) -> (u32, usize) {
    let mut chunk = radix;
    let mut per = 1;
    while let Ok(c) = u32::try_from(u64::from(chunk) * u64::from(radix)) {
        chunk = c;
        per += 1;
    }
    (chunk, per)
}

/// Ors digit `d` (at most 5 bits) in at bit `bit` of the little-endian
/// magnitude `mag`; bits past its end are dropped.
pub fn or_digit(mag: &mut [u8], bit: u64, d: u32) {
    let at = usize::try_from(bit / 8).unwrap_or(usize::MAX);
    let v = d << (bit % 8);
    if let Some(b) = mag.get_mut(at) {
        *b |= v as u8;
    }
    if v >> 8 != 0
        && let Some(b) = mag.get_mut(at.saturating_add(1))
    {
        *b |= (v >> 8) as u8;
    }
}

/// `mag = mag * mult + add` over the `len` low limbs of the little-endian
/// magnitude `mag` (bytes, four a limb), which has room for the result;
/// returns its limbs.
pub fn mul_add(mag: &mut [u8], len: usize, mult: u32, add: u32) -> usize {
    let mut carry = u64::from(add);
    for c in mag.as_chunks_mut::<4>().0.iter_mut().take(len) {
        let x = u32::from_le_bytes(*c);
        let t = u64::from(x) * u64::from(mult) + carry;
        c.copy_from_slice(&(t as u32).to_le_bytes());
        carry = t >> 32;
    }
    match mag.get_mut(4 * len..4 * len + 4) {
        Some(c) if carry != 0 => {
            c.copy_from_slice(&(carry as u32).to_le_bytes());
            len + 1
        }
        _ => len,
    }
}

impl Big {
    /// Zero.
    pub fn zero() -> Big {
        Big::default()
    }

    fn from_mag(neg: bool, mut mag: Vec<u32>) -> Big {
        trim(&mut mag);
        let neg = neg && !mag.is_empty();
        Big { neg, mag }
    }

    /// From a `u64` magnitude and a sign.
    pub fn from_u64(m: u64, neg: bool) -> Big {
        Big::from_mag(neg, vec![m as u32, (m >> 32) as u32])
    }

    /// From an `i64`.
    pub fn from_i64(i: i64) -> Big {
        Big::from_u64(i.unsigned_abs(), i < 0)
    }

    /// From an `i128`.
    pub fn from_i128(i: i128) -> Big {
        let m = i.unsigned_abs();
        Big::from_mag(
            i < 0,
            vec![
                m as u32,
                (m >> 32) as u32,
                (m >> 64) as u32,
                (m >> 96) as u32,
            ],
        )
    }

    /// Is this zero?
    pub fn is_zero(&self) -> bool {
        self.mag.is_empty()
    }

    /// The value as an `i64`, if it fits.
    pub fn to_i64(&self) -> Option<i64> {
        if self.mag.len() > 2 {
            return None;
        }
        let m = u64::from(self.mag.first().copied().unwrap_or(0))
            | u64::from(self.mag.get(1).copied().unwrap_or(0)) << 32;
        if self.neg {
            if m <= 1 << 63 {
                Some((m as i64).wrapping_neg())
            } else {
                None
            }
        } else {
            i64::try_from(m).ok()
        }
    }

    /// Number of bits of `|self|` (0 for zero).
    pub fn bit_length(&self) -> u64 {
        match self.mag.last() {
            None => 0,
            Some(&top) => (self.mag.len() as u64 - 1) * 32 + u64::from(32 - top.leading_zeros()),
        }
    }

    /// Number of one bits of `|self|`.
    pub fn count_ones(&self) -> u64 {
        self.mag.iter().map(|x| u64::from(x.count_ones())).sum()
    }

    /// Three-way comparison.
    pub fn cmp(&self, o: &Big) -> Ordering {
        match (self.neg, o.neg) {
            (false, true) => Ordering::Greater,
            (true, false) => Ordering::Less,
            (false, false) => mag_cmp(&self.mag, &o.mag),
            (true, true) => mag_cmp(&o.mag, &self.mag),
        }
    }

    /// `-self`.
    pub fn neg(&self) -> Big {
        Big::from_mag(!self.neg, self.mag.clone())
    }

    /// `|self|`.
    pub fn abs(&self) -> Big {
        Big::from_mag(false, self.mag.clone())
    }

    /// `self + o`.
    pub fn add(&self, o: &Big) -> Big {
        if self.neg == o.neg {
            return Big::from_mag(self.neg, mag_add(&self.mag, &o.mag));
        }
        match mag_cmp(&self.mag, &o.mag) {
            Ordering::Equal => Big::zero(),
            Ordering::Greater => Big::from_mag(self.neg, mag_sub(&self.mag, &o.mag)),
            Ordering::Less => Big::from_mag(o.neg, mag_sub(&o.mag, &self.mag)),
        }
    }

    /// `self - o`.
    pub fn sub(&self, o: &Big) -> Big {
        self.add(&o.neg())
    }

    /// `self * o`.
    pub fn mul(&self, o: &Big) -> Big {
        Big::from_mag(self.neg != o.neg, mag_mul(&self.mag, &o.mag))
    }

    /// Truncating division: `(q, r)` with `self = q*o + r`, `r` taking
    /// `self`'s sign. `o` must be non-zero.
    pub fn divmod_trunc(&self, o: &Big) -> (Big, Big) {
        let (q, r) = mag_divmod(&self.mag, &o.mag);
        (
            Big::from_mag(self.neg != o.neg, q),
            Big::from_mag(self.neg, r),
        )
    }

    /// Python's floor division and modulo. `o` must be non-zero.
    pub fn divmod_floor(&self, o: &Big) -> (Big, Big) {
        let (q, r) = self.divmod_trunc(o);
        if !r.is_zero() && (r.neg != o.neg) {
            (q.sub(&Big::from_i64(1)), r.add(o))
        } else {
            (q, r)
        }
    }

    /// `self ** e`.
    pub fn pow(&self, mut e: u64) -> Big {
        let mut acc = Big::from_i64(1);
        let mut base = self.clone();
        while e > 0 {
            if e & 1 == 1 {
                acc = acc.mul(&base);
            }
            e >>= 1;
            if e > 0 {
                base = base.mul(&base);
            }
        }
        acc
    }

    /// `self ** e % m` for `e >= 0`, `m != 0`, Python's sign rules.
    pub fn modpow(&self, e: &Big, m: &Big) -> Big {
        let mm = m.abs();
        let mut base = self.divmod_floor(&mm).1;
        let mut acc = Big::from_i64(1).divmod_floor(&mm).1;
        let bits = e.bit_length();
        for i in 0..bits {
            if e.bit(i) {
                acc = acc.mul(&base).divmod_floor(&mm).1;
            }
            if i + 1 < bits {
                base = base.mul(&base).divmod_floor(&mm).1;
            }
        }
        if m.neg && !acc.is_zero() {
            acc = acc.add(m);
        }
        acc
    }

    /// The modular inverse of `self` modulo `m > 0`, if it exists.
    pub fn modinv(&self, m: &Big) -> Option<Big> {
        let (mut r0, mut r1) = (m.clone(), self.divmod_floor(m).1);
        let (mut t0, mut t1) = (Big::zero(), Big::from_i64(1));
        while !r1.is_zero() {
            let (q, r) = r0.divmod_floor(&r1);
            let t2 = t0.sub(&q.mul(&t1));
            r0 = core::mem::replace(&mut r1, r);
            t0 = core::mem::replace(&mut t1, t2);
        }
        if r0 != Big::from_i64(1) {
            return None;
        }
        Some(t0.divmod_floor(m).1)
    }

    /// Bit `i` of `|self|`.
    pub fn bit(&self, i: u64) -> bool {
        let limb = usize::try_from(i / 32).unwrap_or(usize::MAX);
        self.mag.get(limb).is_some_and(|x| x >> (i % 32) & 1 == 1)
    }

    /// `self << n`.
    pub fn shl(&self, n: u64) -> Big {
        if self.is_zero() {
            return Big::zero();
        }
        let limbs = usize::try_from(n / 32).unwrap_or(usize::MAX);
        let mut mag = vec![0u32; limbs];
        mag.extend(shl_bits(&self.mag, (n % 32) as u32, true));
        Big::from_mag(self.neg, mag)
    }

    /// `self >> n` (floor, as Python).
    pub fn shr(&self, n: u64) -> Big {
        let (q, lost) = self.shr_trunc(n, self.neg);
        if self.neg && lost {
            q.sub(&Big::from_i64(1))
        } else {
            q
        }
    }

    /// `|self| >> n` with sign `neg`, and whether a bit shifted out was
    /// set.
    pub fn shr_trunc(&self, n: u64, neg: bool) -> (Big, bool) {
        let limbs = usize::try_from(n / 32).unwrap_or(usize::MAX);
        if limbs >= self.mag.len() {
            return (Big::zero(), !self.is_zero());
        }
        let s = (n % 32) as u32;
        let src = &self.mag[limbs..];
        let mut lost = self.mag[..limbs].iter().any(|&x| x != 0);
        if s != 0 && src[0] & ((1u32 << s) - 1) != 0 {
            lost = true;
        }
        let mut mag = Vec::with_capacity(src.len());
        for i in 0..src.len() {
            let lo = src[i] >> s;
            let hi = if s == 0 {
                0
            } else {
                src.get(i + 1).map_or(0, |x| x << (32 - s))
            };
            mag.push(lo | hi);
        }
        (Big::from_mag(neg, mag), lost)
    }

    fn twos(&self, len: usize) -> Vec<u32> {
        let mut v = self.mag.clone();
        v.resize(len, 0);
        if self.neg {
            let mut carry = 1u64;
            for x in &mut v {
                let t = u64::from(!*x) + carry;
                *x = t as u32;
                carry = t >> 32;
            }
        }
        v
    }

    fn from_twos(mut v: Vec<u32>) -> Big {
        let neg = v.last().is_some_and(|x| x >> 31 == 1);
        if neg {
            let mut carry = 1u64;
            for x in &mut v {
                let t = u64::from(!*x) + carry;
                *x = t as u32;
                carry = t >> 32;
            }
        }
        Big::from_mag(neg, v)
    }

    /// Bitwise `&`, `|` or `^` (`op` is `b'&'`, `b'|'`, `b'^'`).
    pub fn bitop(&self, o: &Big, op: u8) -> Big {
        let len = self.mag.len().max(o.mag.len()) + 1;
        let (a, b) = (self.twos(len), o.twos(len));
        let v = a
            .iter()
            .zip(&b)
            .map(|(x, y)| match op {
                b'&' => x & y,
                b'|' => x | y,
                _ => x ^ y,
            })
            .collect();
        Big::from_twos(v)
    }

    /// `~self`.
    pub fn not(&self) -> Big {
        self.neg().sub(&Big::from_i64(1))
    }

    /// The nearest float (ties to even), or `None` if it overflows.
    pub fn to_f64(&self) -> Option<f64> {
        let n = self.bit_length();
        let f = if n <= 64 {
            let m = u64::from(self.mag.first().copied().unwrap_or(0))
                | u64::from(self.mag.get(1).copied().unwrap_or(0)) << 32;
            m as f64
        } else {
            if n > 1024 {
                return None;
            }
            // The top 64 bits with a sticky bit for everything below: the
            // u64 -> f64 conversion then rounds exactly once, correctly.
            let shift = n - 64;
            let top = self.abs().shr(shift);
            let mut m = u64::from(top.mag.first().copied().unwrap_or(0))
                | u64::from(top.mag.get(1).copied().unwrap_or(0)) << 32;
            let limbs = usize::try_from(shift / 32).unwrap_or(0);
            let rem_bits = (shift % 32) as u32;
            let lost = self.mag[..limbs].iter().any(|&x| x != 0)
                || (rem_bits != 0 && self.mag[limbs] & ((1u32 << rem_bits) - 1) != 0);
            if lost {
                m |= 1;
            }
            let mut f = m as f64;
            let mut e = shift;
            while e > 0 {
                let step = e.min(512);
                f *= f64::from_bits((1023 + step) << 52);
                e -= step;
            }
            if f.is_infinite() {
                return None;
            }
            f
        };
        Some(if self.neg { -f } else { f })
    }

    /// The integer part of a finite float.
    #[inline(never)]
    pub fn from_f64(f: f64) -> Big {
        let bits = f.to_bits();
        let neg = bits >> 63 == 1;
        let exp = ((bits >> 52) & 0x7FF) as i64;
        if exp == 0 {
            return Big::zero();
        }
        let mant = (bits & ((1 << 52) - 1)) | (1 << 52);
        let e = exp - 1075;
        if e >= 0 {
            Big::from_u64(mant, neg).shl(e as u64)
        } else if -e >= 53 {
            Big::zero()
        } else {
            Big::from_u64(mant >> (-e), neg)
        }
    }

    /// Digits of `|self|` in `radix` (2..=36), lowercase, no sign or prefix.
    pub fn digits(&self, radix: u32) -> String {
        if self.is_zero() {
            return String::from("0");
        }
        let mut out: Vec<u8> = Vec::new();
        if radix.is_power_of_two() {
            let bits = u64::from(radix.trailing_zeros());
            let n = self.bit_length();
            let count = n.div_ceil(bits);
            out.reserve_exact(usize::try_from(count).unwrap_or(0));
            // Each digit from the two limbs its bits lie in.
            let mask = (1u64 << bits) - 1;
            let limb = |i: u64| {
                usize::try_from(i)
                    .ok()
                    .and_then(|i| self.mag.get(i))
                    .map_or(0, |&x| u64::from(x))
            };
            for k in 0..count {
                let at = k * bits;
                let w = limb(at / 32) | limb(at / 32 + 1) << 32;
                let d = (w >> (at % 32)) & mask;
                out.push(b"0123456789abcdefghijklmnopqrstuvwxyz"[d as usize]);
            }
        } else {
            let (chunk, per) = radix_chunk(radix);
            let mut cur = self.mag.clone();
            if radix == 10 {
                // Constant divisors, which compile to multiplications.
                radix_digits(&mut cur, 1_000_000_000, 9, 10, &mut out);
            } else {
                radix_digits(&mut cur, chunk, per, radix, &mut out);
            }
            while out.len() > 1 && out.last() == Some(&b'0') {
                out.pop();
            }
        }
        out.reverse();
        String::from_utf8(out).unwrap_or_default()
    }

    /// Parses digits (no sign, prefix or underscores) in `radix`; `None` on
    /// an invalid digit or empty input.
    pub fn parse(digits: &[u8], radix: u32) -> Option<Big> {
        if digits.is_empty() {
            return None;
        }
        if radix.is_power_of_two() {
            // Each digit's bits in place, from the top: linear.
            let k = u64::from(radix.trailing_zeros());
            let n = digits.len() as u64;
            let mut mag = vec![0u8; usize::try_from((n * k).div_ceil(32) * 4).ok()?];
            for (i, &c) in digits.iter().enumerate() {
                let d = digit_value(c).filter(|&d| d < radix)?;
                or_digit(&mut mag, (n - 1 - i as u64) * k, d);
            }
            return Some(Big::from_bytes_le(&mag, false));
        }
        let (chunk, per) = radix_chunk(radix);
        let mut mag: Vec<u32> = Vec::new();
        for group in digits.chunks(per) {
            let mut v = 0u32;
            let mut mult = 1u32;
            for &c in group {
                let d = digit_value(c).filter(|&d| d < radix)?;
                v = v * radix + d;
                mult = mult.saturating_mul(radix);
            }
            let mult = if group.len() == per { chunk } else { mult };
            // mag = mag * mult + v
            let mut carry = u64::from(v);
            for x in &mut mag {
                let t = u64::from(*x) * u64::from(mult) + carry;
                *x = t as u32;
                carry = t >> 32;
            }
            if carry != 0 {
                mag.push(carry as u32);
            }
        }
        Some(Big::from_mag(false, mag))
    }

    /// Two's-complement bytes, little-endian, exactly `len` long; `None` if
    /// the value does not fit (or is negative and `!signed`).
    pub fn to_bytes_le(&self, len: usize, signed: bool) -> Option<Vec<u8>> {
        if self.neg && !signed {
            return None;
        }
        let limbs = len.div_ceil(4) + 1;
        let tw = self.twos(limbs.max(self.mag.len() + 1));
        let mut out: Vec<u8> = tw.iter().flat_map(|x| x.to_le_bytes()).collect();
        let fill = if self.neg { 0xFF } else { 0 };
        if out[len..].iter().any(|&b| b != fill) {
            return None;
        }
        out.truncate(len);
        if signed && len > 0 && (out[len - 1] >> 7 == 1) != self.neg {
            return None;
        }
        if signed && len == 0 && !self.is_zero() {
            return None;
        }
        Some(out)
    }

    /// From little-endian bytes, as two's complement if `signed`.
    pub fn from_bytes_le(b: &[u8], signed: bool) -> Big {
        let mut v: Vec<u32> = b
            .chunks(4)
            .map(|c| {
                let mut w = [0u8; 4];
                w[..c.len()].copy_from_slice(c);
                u32::from_le_bytes(w)
            })
            .collect();
        if signed && b.last().is_some_and(|x| x >> 7 == 1) {
            let ext = b.len() % 4;
            if ext != 0
                && let Some(last) = v.last_mut()
            {
                *last |= u32::MAX << (8 * ext);
            }
            v.push(u32::MAX);
            return Big::from_twos(v);
        }
        Big::from_mag(false, v)
    }

    /// Floor square root of a non-negative value (Newton's method).
    pub fn isqrt(&self) -> Big {
        if self.is_zero() {
            return Big::zero();
        }
        let n = self.bit_length();
        let mut x = Big::from_i64(1).shl(n.div_ceil(2));
        loop {
            let y = x.add(&self.divmod_floor(&x).0).shr(1);
            if y.cmp(&x) != Ordering::Less {
                return x;
            }
            x = y;
        }
    }

    /// Greatest common divisor of the magnitudes.
    pub fn gcd(&self, o: &Big) -> Big {
        // A long value and one of up to 128 bits: the first remainder by a
        // short division that keeps no quotient, and no copy of the long
        // one (the int cap by 3 in about 3 ms, not a 8 MiB quotient).
        let (long, short) = if self.mag.len() >= o.mag.len() {
            (self, o)
        } else {
            (o, self)
        };
        if !short.is_zero() && short.mag.len() <= 4 && long.mag.len() > 4 {
            let r = short_divmod(&long.mag, &short.mag, false).1;
            return short.abs().gcd(&Big::from_mag(false, r));
        }
        let (mut a, mut b) = (self.abs(), o.abs());
        while !b.is_zero() {
            let r = a.divmod_floor(&b).1;
            a = core::mem::replace(&mut b, r);
        }
        a
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "tests")]
mod tests {
    use super::*;

    fn b(i: i128) -> Big {
        Big::from_i128(i)
    }

    #[test]
    fn small_ops_match_i128() {
        let vals: [i128; 12] = [
            0,
            1,
            -1,
            7,
            -7,
            1 << 40,
            -(1 << 40),
            (1 << 62) + 12345,
            -(1 << 63),
            i64::MAX as i128,
            999_999_999_999,
            -123_456_789_012_345,
        ];
        for &x in &vals {
            for &y in &vals {
                assert_eq!(b(x).add(&b(y)), b(x + y));
                assert_eq!(b(x).sub(&b(y)), b(x - y));
                if x.abs() < 1 << 63 && y.abs() < 1 << 63 {
                    assert_eq!(b(x).mul(&b(y)), b(x * y));
                }
                if y != 0 {
                    let (q, r) = b(x).divmod_floor(&b(y));
                    assert_eq!(
                        q,
                        b(x.div_euclid(y) - i128::from(x.rem_euclid(y) != 0 && y < 0))
                    );
                    assert_eq!(q.mul(&b(y)).add(&r), b(x));
                }
                assert_eq!(b(x).bitop(&b(y), b'&'), b(x & y));
                assert_eq!(b(x).bitop(&b(y), b'|'), b(x | y));
                assert_eq!(b(x).bitop(&b(y), b'^'), b(x ^ y));
            }
            assert_eq!(b(x).shr(3), b(x >> 3));
            assert_eq!(b(x).shr(100), b(x >> 100));
            assert_eq!(b(x).not(), b(!x));
            assert_eq!(b(x).digits(10), alloc::format!("{}", x.unsigned_abs()));
            assert_eq!(b(x).digits(16), alloc::format!("{:x}", x.unsigned_abs()));
        }
    }

    #[test]
    fn karatsuba_matches_schoolbook() {
        let mut x = Big::from_i64(3);
        for _ in 0..12 {
            x = x.mul(&x).add(&Big::from_i64(0x1234_5678));
        }
        let y = x.add(&Big::from_i64(987_654_321)).pow(2);
        assert!(x.mag.len() > 2 * KARATSUBA);
        assert_eq!(mag_mul(&x.mag, &y.mag), mul_school(&x.mag, &y.mag));
        let p = x.mul(&y);
        let (q, r) = p.divmod_floor(&y);
        assert_eq!(q, x);
        assert!(r.is_zero());
    }

    #[test]
    fn parse_round_trip() {
        let s = "123456789012345678901234567890123456789";
        let v = Big::parse(s.as_bytes(), 10).unwrap();
        assert_eq!(v.digits(10), s);
        assert_eq!(Big::parse(b"ff", 16).unwrap(), b(255));
    }
}
