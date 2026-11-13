//! Heap integers: the `ty::INT` layout, promotion from `SmallInt`, bigint
//! arithmetic with fuel and memory preflight, exact int/float comparison,
//! and int <-> str/float conversion with CPython's limits.
//!
//! ```text
//! INT  value slot 0: ByteRef of the magnitude, u32 limbs little-endian
//!      raw slot 1:   1 if negative
//! ```
//! An int is a `SmallInt` exactly when it fits one; every heap int is
//! outside `SmallInt` range, so identity and equality stay simple.

use crate::bigint::Big;
use crate::types::{T, ty};
use crate::vm::{Raised, Vm};
use alloc::{format, string::String, vec::Vec};
use core::cmp::Ordering;
use mm_compile::ast::BinOp;
use mm_core::Value;

/// CPython's default `sys.int_info.default_max_str_digits` limit.
pub(crate) const MAX_STR_DIGITS: usize = 4300;

/// Largest int, in bits: building a larger one fails with `MemoryError`
/// (also bounded by the heap's memory limit). Linear operations on ints
/// this size take at most about 21 ms, native release [M] (docs/spec.md
/// §11).
pub(crate) const MAX_BITS: u64 = 1 << 26;

/// Estimated nanoseconds of native release code (the `*_ns` models,
/// fitted to measured times [M]) past which a superlinear int operation
/// fails with `MemoryError` instead of running as one step of more than
/// about 50 ms (docs/spec.md §5, bounded native steps).
const WORK_MAX: u64 = 45_000_000;

/// Estimated nanoseconds of `bigint::mag_mul` on `la`- and `lb`-limb
/// magnitudes: schoolbook below Karatsuba's 40 limbs, the longer in
/// chunks of the shorter when twice as long, else `3^d` products of
/// pieces under 40 limbs; 1.25 ns a limb step and 4 ns a limb of copies
/// (82 ms for two 2^20-bit operands [M], 76 estimated).
#[inline(never)]
pub(crate) fn mul_ns(la: u64, lb: u64) -> u64 {
    let (a, b) = (la.max(lb), la.min(lb));
    let steps = if b < 40 {
        a.saturating_mul(b)
    } else {
        let (n, chunks) = if a >= 2 * b {
            (b, a.div_ceil(b))
        } else {
            (a, 1)
        };
        let (mut s, mut m) = (n, 1u64);
        while s >= 40 {
            s = s.div_ceil(2);
            m = m.saturating_mul(3);
        }
        chunks.saturating_mul(m).saturating_mul(s * s)
    };
    steps.saturating_mul(5) / 4 + 4 * (la + lb)
}

/// Estimated nanoseconds of `bigint::mag_divmod` of an `la`-limb by an
/// `lb`-limb magnitude: by up to four limbs (128 bits) a short division,
/// about 3 ns a limb (at the 2^26-bit cap 1.4-6.7 ms more than a linear
/// `a + 1`'s 12.3 ms [M]), counted as 4; else 0.9 ns a step of algorithm
/// D, a quotient limb costing about 24 more (56 ms for 2^19 by 2^18 bits
/// [M], 60 estimated).
#[inline(never)]
pub(crate) fn div_ns(la: u64, lb: u64) -> u64 {
    if lb <= 4 {
        return la.saturating_mul(4);
    }
    let q = (la + 1).saturating_sub(lb);
    q.saturating_mul(lb + 24).saturating_mul(9) / 10 + 4 * la
}

/// Fuel for dividing an `la`-limb by an `lb`-limb magnitude: a unit per
/// 8 steps of algorithm D (quotient limbs times divisor limbs, about 7 ns)
/// and one per 8 limbs read. A quotient of one limb charges next to
/// nothing however long the operands (review U1: `la * lb / 8` charged 2G
/// units for a 1.5 ms division).
pub(crate) fn div_units(la: u64, lb: u64) -> u64 {
    (la + 1).saturating_sub(lb).saturating_mul(lb) / 8 + la / 8
}

/// Estimated nanoseconds of Euclid's algorithm on `la`- and `lb`-limb
/// values: its first division takes the longer to the shorter's size,
/// then each step of two such values (134 ms for two 2^16-bit ones [M]).
#[inline(never)]
pub(crate) fn gcd_ns(la: u64, lb: u64) -> u64 {
    let (x, y) = (la.max(lb), la.min(lb));
    if y == 0 {
        return 0;
    }
    div_ns(x, y).saturating_add(y.saturating_mul(y).saturating_mul(32))
}

/// Estimated nanoseconds of `Big::pow` of an `abits`-bit base to `e`, by
/// its squarings and multiplications (47 ms for `3 ** 661562` [M], 59
/// estimated).
#[inline(never)]
fn pow_ns(abits: u64, mut e: u64) -> u64 {
    let (mut acc, mut base, mut ns) = (1u64, abits, 0u64);
    while e > 0 {
        if e & 1 == 1 {
            ns = ns.saturating_add(mul_ns(acc.div_ceil(32), base.div_ceil(32)));
            acc = acc.saturating_add(base);
        }
        e >>= 1;
        if e > 0 {
            ns = ns.saturating_add(mul_ns(base.div_ceil(32), base.div_ceil(32)));
            base = base.saturating_mul(2);
        }
    }
    ns
}

/// Estimated nanoseconds of `pow(a, e, m)` for an `la`-limb `a` and an
/// `ml`-limb modulus, by `Big::modpow`'s products and reductions at their
/// operands' sizes; about 0.7 of the model's single operations each (2048-
/// and 4096-bit operands, exponent all ones: 30 and 151 ms [M], 27 and 173
/// estimated).
#[inline(never)]
fn modpow_ns(la: u64, ml: u64, e: &Big) -> u64 {
    let step = |x: u64, y: u64| mul_ns(x, y).saturating_add(div_ns(x + y, ml));
    let bits = e.bit_length();
    let (mut acc, mut base, mut ones) = (1, la.clamp(1, ml.max(1)), 0);
    let mut ns = div_ns(la, ml);
    let mut i = 0;
    // Until both are the modulus's size, then each bit costs the same.
    while i < bits && (acc < ml || base < ml) {
        if e.bit(i) {
            ns = ns.saturating_add(step(acc, base));
            acc = (acc + base).min(ml);
            ones += 1;
        }
        if i + 1 < bits {
            ns = ns.saturating_add(step(base, base));
            base = (2 * base).min(ml);
        }
        i += 1;
    }
    let rest = (bits - i).saturating_add(e.count_ones().saturating_sub(ones));
    ns.saturating_add(rest.saturating_mul(step(ml, ml))) / 10 * 7
}

/// Estimated nanoseconds of `Big::isqrt` of an `l`-limb value: a division
/// by a half-size guess per Newton step (253 ms at 2^18 bits [M], 319
/// estimated).
#[inline(never)]
pub(crate) fn isqrt_ns(l: u64) -> u64 {
    let steps = u64::from(64 - (l * 32).leading_zeros()) + 2;
    steps.saturating_mul(div_ns(l, l / 2 + 1))
}

/// Estimated nanoseconds of `Big::digits(10)` of an `l`-limb value: 46-52
/// ms at 4,985 limbs (48,000 digits; 62 estimated) and 105 ms at 2^18 bits
/// (168 estimated) [M]. Smaller values cost more a limb squared, so the
/// estimate holds the largest one allowed (about 41,000 digits) to 38 ms.
#[inline(never)]
fn decimal_ns(l: u64) -> u64 {
    l.saturating_mul(l).saturating_mul(5) / 2
}

/// Fuel from which an int operator the fuel left cannot pay parks before
/// it starts (`Vm::fuel_short`); cheaper ones overdraft, by at most this.
const STOP_MIN: u64 = 1 << 16;

pub(crate) fn digits_error_text(limit: usize) -> String {
    format!(
        "Exceeds the limit ({limit} digits) for integer string conversion; use sys.set_int_max_str_digits() to increase the limit"
    )
}

impl Vm {
    /// Is `v` a heap int?
    #[inline]
    pub(crate) fn is_heap_int(v: Value) -> bool {
        v.is_obj_kind(ty::INT as u8)
    }

    /// Is `v` an int or bool (any size)?
    #[inline]
    pub(crate) fn is_intlike(v: Value) -> bool {
        v.is_int() || v == Value::TRUE || v == Value::FALSE || Vm::is_heap_int(v)
    }

    /// The limbs and sign of a heap int.
    pub(crate) fn heap_int_parts(&self, v: Value) -> (bool, &[u8]) {
        let Some(r) = v.as_obj() else {
            return (false, &[]);
        };
        let neg = self.raw(r, 1) == 1;
        let bytes = self
            .slot(r, 0)
            .as_byte_ref()
            .and_then(|b| self.heap.bytes(b).ok())
            .unwrap_or(&[]);
        (neg, bytes)
    }

    /// A heap int's value as `i64`, if it fits.
    #[inline(never)]
    pub(crate) fn heap_int_i64(&self, v: Value) -> Option<i64> {
        let (neg, bytes) = self.heap_int_parts(v);
        if bytes.len() > 8 {
            return None;
        }
        let mut w = [0u8; 8];
        w[..bytes.len()].copy_from_slice(bytes);
        let m = u64::from_le_bytes(w);
        if neg {
            if m <= 1 << 63 {
                Some((m as i64).wrapping_neg())
            } else {
                None
            }
        } else {
            i64::try_from(m).ok()
        }
    }

    /// The value of any int or bool as a `Big`.
    pub(crate) fn big_of(&self, v: Value) -> Option<Big> {
        if let Some(i) = v.as_int() {
            return Some(Big::from_i64(i));
        }
        if v == Value::TRUE {
            return Some(Big::from_i64(1));
        }
        if v == Value::FALSE {
            return Some(Big::zero());
        }
        if !Vm::is_heap_int(v) {
            return None;
        }
        let (neg, bytes) = self.heap_int_parts(v);
        let mut mag: Vec<u32> = bytes
            .chunks(4)
            .map(|c| {
                let mut w = [0u8; 4];
                w[..c.len()].copy_from_slice(c);
                u32::from_le_bytes(w)
            })
            .collect();
        while mag.last() == Some(&0) {
            mag.pop();
        }
        Some(Big {
            neg: neg && !mag.is_empty(),
            mag,
        })
    }

    /// The value of an int or bool as `i128`, if it fits: heap ints of
    /// at most four limbs, read without building a [`Big`].
    #[inline]
    pub(crate) fn int128_of(&self, v: Value) -> Option<i128> {
        if !Vm::is_heap_int(v) {
            return self.int_of(v).map(i128::from);
        }
        let (neg, bytes) = self.heap_int_parts(v);
        let mut w = [0u8; 16];
        w.get_mut(..bytes.len())?.copy_from_slice(bytes);
        let m = u128::from_le_bytes(w);
        if neg {
            0i128.checked_sub_unsigned(m)
        } else {
            i128::try_from(m).ok()
        }
    }

    /// An int value for `i`, as [`Vm::new_big`] of `Big::from_i128(i)`
    /// makes it (the same layout and charges), without the vectors.
    #[inline(never)]
    pub(crate) fn new_int128(&mut self, i: i128) -> Result<Value, Raised> {
        if let Some(v) = i64::try_from(i).ok().and_then(Value::int) {
            return Ok(v);
        }
        let bytes = i.unsigned_abs().to_le_bytes();
        // Whole limbs, without high zero limbs (a `Big`'s magnitude).
        let limbs = bytes
            .chunks(4)
            .rposition(|c| c != [0; 4])
            .map_or(0, |k| k + 1);
        self.heap_int(i < 0, &bytes[..4 * limbs])
    }

    /// An int value for `b`: a `SmallInt` when it fits, else a heap int.
    #[inline(never)]
    pub(crate) fn new_big(&mut self, b: &Big) -> Result<Value, Raised> {
        if let Some(i) = b.to_i64()
            && let Some(v) = Value::int(i)
        {
            return Ok(v);
        }
        let bytes: Vec<u8> = b.mag.iter().flat_map(|x| x.to_le_bytes()).collect();
        self.heap_int(b.neg, &bytes)
    }

    /// A heap int of magnitude `bytes` (whole limbs, no high zero limb).
    fn heap_int(&mut self, neg: bool, bytes: &[u8]) -> Result<Value, Raised> {
        if bytes.len() as u64 > MAX_BITS / 8 {
            return Err(self.err(T::MemoryError, ""));
        }
        // Building the result (and, for unary and bitwise operators,
        // computing it) is byte work (docs/spec.md §5).
        self.burn_bytes(bytes.len());
        self.collect_for_result(bytes.len());
        let data = {
            let res = self.heap.alloc_bytes(bytes);
            self.hx(res)
        }?;
        let r = {
            let res =
                self.heap
                    .alloc_init(ty::INT, 0, &[Value::from_byte_ref(data)], &[u64::from(neg)]);
            self.hx(res)
        }?;
        Ok(Value::from_ref(r, ty::INT as u8))
    }

    /// Charges for a str or bytes result of `bytes` bytes (callers check
    /// `bytes >= 64`): one unit per 64 bytes, as byte work is charged, and
    /// for a str or bytes (`gc`) a collection when the result is large
    /// next to the memory left ([`Vm::collect_for_result`]). The arena
    /// constructors call it (and charge tuples and list, dict and set
    /// tables one unit per 8 slots), so every native that builds a large
    /// result pays for the copy, however it got there (docs/spec.md §5).
    #[cold]
    #[inline(never)]
    pub(crate) fn burn_result(&mut self, bytes: usize, gc: bool) {
        if gc {
            self.collect_for_result(bytes);
        }
        self.burn(bytes as u64 / 64);
    }

    /// Requests a collection after a result of 64 KiB or more once the
    /// bytes allocated since the last collection (what it could free at
    /// most) reach half the memory left: natives cannot collect, so garbage
    /// that large results leave must go before the next ones are built.
    /// Before that, the charge's safepoint check leaves it to
    /// [`mm_core::Heap::should_collect`], instead of a full collection
    /// after every large string, bytes or int (a loop of 100 KB
    /// concatenations beside 1M live lists took 20 ms per iteration [M]).
    pub(crate) fn collect_for_result(&mut self, bytes: usize) {
        if bytes >= 1 << 16
            && self.heap.stats().allocated_since_gc >= self.heap.available_bytes() / 2
        {
            self.request_gc();
        }
    }

    /// Charges byte work over `bytes` bytes: one unit per 64 (docs/spec.md
    /// §5).
    #[inline]
    pub(crate) fn burn_bytes(&mut self, bytes: usize) {
        if bytes >= 64 {
            self.burn(bytes as u64 / 64);
        }
    }

    /// Charges `units` of fuel for native work (docs/spec.md §5); an
    /// exhausted budget parks at the next safepoint.
    #[inline(never)]
    pub(crate) fn burn(&mut self, units: u64) {
        self.work = self.work.wrapping_add(units);
        if let Some(f) = self.fuel {
            self.fuel = Some(f.saturating_sub(units));
            self.fuel_overdraft = self.fuel_overdraft.saturating_add(units.saturating_sub(f));
        }
        // Requests, deadlines and the collection trigger also need a check
        // after native work when the embedder has left fuel unlimited,
        // whether one charge or many small ones since the last check (a
        // straight-line body of slices would else reach one only every 256
        // back edges), and a spent budget one at once: the dispatch loop
        // checks after the instruction. The ticks this skips did not run,
        // so the period shrinks with them: the check charges only the
        // safepoints that did.
        self.burned = self.burned.saturating_add(units);
        if self.burned > 64 || self.fuel == Some(0) {
            self.long_work |= units > 64;
            let left = (self.ticks + self.tick_spare).max(0);
            self.ticks = self.ticks.min(1);
            self.tick_spare = self.tick_spare.min(1);
            self.tick_period -= left - (self.ticks + self.tick_spare).max(0);
        }
    }

    /// Whether an operator instruction must stop before it starts, for a
    /// safepoint: a native charged more than 64 units at once since the
    /// last one (`long_work`), and straight-line code has no other check
    /// before the next `CHECK` (C4, docs/spec.md §8.6). The caller returns
    /// `Err(Raised)`; [`Vm::execute`] runs the safepoint and the
    /// instruction again, or parks before it. (Not `ticks`: a fuel slice
    /// shorter than the tick period leaves it at 1 after every safepoint.)
    #[inline]
    pub(crate) fn stop_before(&mut self) -> bool {
        if !self.long_work {
            return false;
        }
        self.check_before = true;
        true
    }

    /// Whether the operator instruction `a <op> b` on ints must park before
    /// it starts: it would charge at least [`STOP_MIN`] fuel, more than is
    /// left, and this is not the first such operation since a `resume`
    /// (which runs it, charging the excess as before). Records the cost for
    /// [`Vm::fuel_wanted`]; the caller returns `Err(Raised)` and
    /// [`Vm::execute`] parks with the instruction not yet run.
    #[cold]
    #[inline(never)]
    pub(crate) fn fuel_short(&mut self, op: BinOp, a: Value, b: Value) -> bool {
        if self.fuel.is_none() {
            return false;
        }
        let cost = self.op_cost(op, a, b);
        self.cost_short(cost)
    }

    /// Whether a builtin called straight from a `CALL` instruction must
    /// park before it starts, as [`Vm::fuel_short`] for an operator: the
    /// big-int builtins whose one native step can take seconds (`pow`,
    /// `divmod`, `math.factorial`, `comb`, `perm`, `isqrt`, `gcd`, `lcm`),
    /// and `sorted`/`list.sort` of a long list or tuple.
    #[cold]
    #[inline(never)]
    pub(crate) fn native_short(&mut self, n: &crate::builtins::Native, a: &CallArgs) -> bool {
        let arg = |vm: &Vm, i: u32| {
            if i < a.n {
                vm.arg(a, i)
            } else {
                Value::UNDEFINED
            }
        };
        let (x, y, z) = (arg(self, 0), arg(self, 1), arg(self, 2));
        let limbs = |vm: &Vm, v: Value| match v.as_int() {
            Some(i) => u64::from(64 - i.unsigned_abs().leading_zeros()).div_ceil(32),
            None if Vm::is_heap_int(v) => (vm.heap_int_parts(v).1.len() as u64).div_ceil(4),
            None => 0,
        };
        let ints = |vm: &Vm, vs: &[Value]| {
            vs.iter().all(|&v| Vm::is_intlike(v)) && !vs.is_empty() && vm.fuel.is_some()
        };
        let cost = match (n.module(), n.name()) {
            (None, "pow") if z != Value::UNDEFINED && z != Value::NONE => {
                if !ints(self, &[x, y, z]) {
                    return false;
                }
                // `big_modpow`'s charge.
                let m = limbs(self, z);
                (m.saturating_mul(m).saturating_mul(limbs(self, y) * 32) / 16)
                    .saturating_add(div_units(limbs(self, x), m))
            }
            (None, "pow") => self.op_cost(BinOp::Pow, x, y),
            (None, "divmod") => self
                .op_cost(BinOp::FloorDiv, x, y)
                .saturating_add(self.op_cost(BinOp::Mod, x, y)),
            (Some("math"), "isqrt") if ints(self, &[x]) => {
                let l = limbs(self, x);
                if isqrt_ns(l) > WORK_MAX {
                    return false;
                }
                l.saturating_mul(l) / 4
            }
            (Some("math"), "gcd" | "lcm") => {
                // `gcd_all`'s charges, the running value at most the
                // largest size (lcm: the sizes so far).
                let vs: Vec<Value> = (0..a.n).map(|i| arg(self, i)).collect();
                if !ints(self, &vs) {
                    return false;
                }
                let lcm = n.name() == "lcm";
                // The running value starts at 0 (gcd) or 1 (lcm).
                let (mut acc, mut cost) = (u64::from(lcm), 0u64);
                for &v in &vs {
                    let l = limbs(self, v);
                    let mut ns = gcd_ns(acc, l);
                    if lcm {
                        ns = ns
                            .saturating_add(mul_ns(acc, l))
                            .saturating_add(div_ns(acc + l, acc.min(l)));
                    }
                    if ns > WORK_MAX {
                        return false;
                    }
                    cost = cost.saturating_add(ns / 10 + acc.max(l) / 8);
                    acc = if lcm { acc + l } else { acc.max(l) };
                }
                cost
            }
            (Some("math"), "factorial" | "perm" | "comb") => {
                if !ints(self, &[x]) || x.as_int().is_none() {
                    return false;
                }
                // `falling`'s charge: k factors of n's size; factorial and
                // a missing k are k = n, comb the smaller of k and n - k.
                let nn = x.as_int().unwrap_or(0).max(0).unsigned_abs();
                let k = match y.as_int() {
                    Some(k) if n.name() != "factorial" => {
                        let k = k.max(0).unsigned_abs().min(nn);
                        if n.name() == "comb" { k.min(nn - k) } else { k }
                    }
                    _ if n.name() == "factorial" || y == Value::UNDEFINED || y == Value::NONE => nn,
                    _ => return false,
                };
                let bits = k.saturating_mul(u64::from(64 - nn.leading_zeros()).max(1));
                // Results too large to build, or work the cap refuses,
                // fail at once (`falling`, `m_comb`).
                if k > 1 << 22 || bits > MAX_BITS {
                    return false;
                }
                let cost = crate::math::product_cost;
                if n.name() == "comb" {
                    // `k!` too, and the division of the two products.
                    let kbits = k.saturating_mul(u64::from(64 - k.leading_zeros()).max(1));
                    let made = cost(bits) + cost(kbits);
                    let div = div_ns(bits / 32 + 1, kbits / 32 + 1);
                    if made.saturating_mul(6).saturating_add(div) > WORK_MAX {
                        return false;
                    }
                    made + (bits / 32) * (kbits / 32) / 8
                } else if cost(bits).saturating_mul(6) > WORK_MAX {
                    return false;
                } else {
                    cost(bits)
                }
            }
            (None, "sorted" | "sort")
                if (n.name() == "sorted" || n.owner == Some(T::list))
                    && (x.is_obj_kind(ty::LIST as u8) || x.is_obj_kind(ty::TUPLE as u8)) =>
            {
                // `merge_sort`'s comparisons, a walk visit at least each.
                let len = self.seq_words(x).len() as u64;
                len.saturating_mul(u64::from(64 - len.leading_zeros()))
            }
            (None, "sum") | (Some("math"), "prod")
                if self.fuel.is_some()
                    && (x.is_obj_kind(ty::LIST as u8) || x.is_obj_kind(ty::TUPLE as u8)) =>
            {
                // The charges of the additions or multiplications, the
                // running total at most the sizes so far (lists and tuples
                // only: other iterables are not read twice).
                let sum = n.name() == "sum";
                let start = if sum { y } else { self.kwarg(a, 0) };
                let mut total = limbs(self, start);
                let mut cost = 0u64;
                for &w in self.seq_words(x) {
                    let l = Value::from_bits(w).map_or(0, |v| limbs(self, v));
                    cost = cost.saturating_add(if sum {
                        total.max(l) / 8
                    } else {
                        total.saturating_mul(l) / 8
                    });
                    total = if sum {
                        total.max(l)
                    } else {
                        total.saturating_add(l)
                    };
                }
                cost
            }
            _ => return false,
        };
        self.cost_short(cost)
    }

    /// Parks before an operation costing `cost` (see [`Vm::fuel_short`]).
    fn cost_short(&mut self, cost: u64) -> bool {
        let Some(fuel) = self.fuel else {
            return false;
        };
        if cost < STOP_MIN || core::mem::take(&mut self.afford) {
            return false;
        }
        let left = (self.ticks + self.tick_spare).max(0);
        let used = u64::from((self.tick_period - left).unsigned_abs());
        if cost <= fuel.saturating_sub(used) {
            return false;
        }
        self.fuel_wanted = cost;
        true
    }

    /// The fuel `big_binop`/`big_pow` charge for `a <op> b` (0 for other
    /// operands, and for results too large to build, which fail at once).
    fn op_cost(&self, op: BinOp, a: Value, b: Value) -> u64 {
        if !Vm::is_intlike(a) || !Vm::is_intlike(b) {
            return 0;
        }
        let bits = |v: Value| match v.as_int() {
            Some(i) => 64 - u64::from(i.unsigned_abs().leading_zeros()),
            None if Vm::is_heap_int(v) => self.heap_int_parts(v).1.len() as u64 * 8,
            None => 1,
        };
        let (la, lb) = (bits(a).div_ceil(32), bits(b).div_ceil(32));
        match op {
            // Work the cap refuses fails at once (`Vm::big_work`).
            BinOp::Mul if mul_ns(la, lb) > WORK_MAX => 0,
            BinOp::FloorDiv | BinOp::Mod if div_ns(la, lb) > WORK_MAX => 0,
            BinOp::Mul => la * lb / 8,
            BinOp::FloorDiv | BinOp::Mod => div_units(la, lb),
            // `big_truediv` scales its quotient to 55 bits.
            BinOp::Div => div_units(la.max(lb) + 2, la.max(lb)),
            BinOp::Pow => {
                let Some(e) = b.as_int().and_then(|e| u64::try_from(e).ok()) else {
                    return 0;
                };
                // `big_pow` answers these bases without multiplying.
                if matches!(a.as_int(), Some(-1..=1)) {
                    return 0;
                }
                let rbits = bits(a).saturating_mul(e);
                if rbits > MAX_BITS
                    || !self.heap.would_fit(rbits / 8 * 2 + 64)
                    || pow_ns(bits(a), e) > WORK_MAX
                {
                    return 0;
                }
                let limbs = rbits / 32;
                limbs.saturating_mul(limbs) / 64 + e.min(64)
            }
            _ => 0,
        }
    }

    /// Fails with `MemoryError` unless a `bits`-bit result can be built.
    fn big_room(&mut self, bits: u64) -> Result<(), Raised> {
        if bits > MAX_BITS || !self.heap.would_fit(bits / 8 * 2 + 64) {
            return Err(self.err(T::MemoryError, ""));
        }
        Ok(())
    }

    /// [`Vm::work_room`] for int operator `op` on `la`- and `lb`-limb
    /// operands: out of line, off the small-int paths.
    #[inline(never)]
    fn big_work(&mut self, op: BinOp, la: u64, lb: u64) -> Result<(), Raised> {
        let ns = if op == BinOp::Mul {
            mul_ns(la, lb)
        } else {
            div_ns(la, lb)
        };
        self.work_room(ns)
    }

    /// Fails with `MemoryError` if an int operation estimated to take `ns`
    /// nanoseconds (the `*_ns` models) would run past [`WORK_MAX`].
    pub(crate) fn work_room(&mut self, ns: u64) -> Result<(), Raised> {
        if ns > WORK_MAX {
            return Err(self.err(T::MemoryError, "integer operation too large"));
        }
        Ok(())
    }

    /// `a <op> b` for ints that fit `i128`, when the result does too and
    /// [`Vm::big_binop`] would charge nothing for it (at most four limbs,
    /// left shifts below 96); `None` otherwise: products, divisions and
    /// powers go to it.
    pub(crate) fn int128_binop(op: BinOp, x: i128, y: i128) -> Option<i128> {
        match op {
            BinOp::Add => x.checked_add(y),
            BinOp::Sub => x.checked_sub(y),
            BinOp::BitAnd => Some(x & y),
            BinOp::BitOr => Some(x | y),
            BinOp::BitXor => Some(x ^ y),
            BinOp::LShift if (0..96).contains(&y) => {
                let v = x << y;
                (v >> y == x).then_some(v)
            }
            BinOp::RShift if y >= 0 => Some(x >> y.min(127)),
            _ => None,
        }
    }

    /// `a <op> b` for ints of any size.
    pub(crate) fn big_binop(&mut self, op: BinOp, a: &Big, b: &Big) -> Result<Value, Raised> {
        let (la, lb) = (a.mag.len() as u64, b.mag.len() as u64);
        let r = match op {
            BinOp::Add => {
                self.burn(la.max(lb) / 8);
                a.add(b)
            }
            BinOp::Sub => {
                self.burn(la.max(lb) / 8);
                a.sub(b)
            }
            BinOp::Mul => {
                self.big_room(a.bit_length() + b.bit_length())?;
                self.big_work(op, la, lb)?;
                self.burn(la * lb / 8);
                a.mul(b)
            }
            BinOp::Div => {
                if b.is_zero() {
                    return Err(self.err(T::ZeroDivisionError, "division by zero"));
                }
                // Its quotient is two limbs at most (`big_truediv`), so
                // linear work the cap never refuses.
                self.burn(div_units(la.max(lb) + 2, la.max(lb)));
                let f = self.big_truediv(a, b)?;
                return Ok(Value::float(f));
            }
            BinOp::FloorDiv | BinOp::Mod => {
                if b.is_zero() {
                    return Err(self.err(T::ZeroDivisionError, "division by zero"));
                }
                self.big_work(op, la, lb)?;
                self.burn(div_units(la, lb));
                let (q, m) = a.divmod_floor(b);
                if op == BinOp::FloorDiv { q } else { m }
            }
            BinOp::Pow => return self.big_pow(a, b),
            BinOp::LShift => {
                if b.neg {
                    return Err(self.err(T::ValueError, "negative shift count"));
                }
                if a.is_zero() {
                    return Ok(Value::int(0).unwrap_or(Value::NONE));
                }
                let Some(n) = b.to_i64().and_then(|n| u64::try_from(n).ok()) else {
                    return Err(self.err(T::OverflowError, "too many digits in integer"));
                };
                self.big_room(a.bit_length().saturating_add(n))?;
                self.burn((la + n / 32) / 8);
                a.shl(n)
            }
            BinOp::RShift => {
                if b.neg {
                    return Err(self.err(T::ValueError, "negative shift count"));
                }
                let n = b
                    .to_i64()
                    .and_then(|n| u64::try_from(n).ok())
                    .unwrap_or(u64::MAX);
                self.burn(la / 8);
                a.shr(n)
            }
            BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor => {
                // As the shifts charge: one unit per 8 limbs.
                self.burn(la.max(lb) / 8);
                let c = match op {
                    BinOp::BitAnd => b'&',
                    BinOp::BitOr => b'|',
                    _ => b'^',
                };
                a.bitop(b, c)
            }
            BinOp::MatMul => {
                return Err(self.err(
                    T::TypeError,
                    "unsupported operand type(s) for @: 'int' and 'int'",
                ));
            }
        };
        self.new_big(&r)
    }

    /// `divmod(a, b)` for ints of any size, `b` nonzero: one division for
    /// both, charged as `//` and `%` are.
    pub(crate) fn big_divmod(&mut self, a: &Big, b: &Big) -> Result<Value, Raised> {
        let (la, lb) = (a.mag.len() as u64, b.mag.len() as u64);
        self.big_work(BinOp::FloorDiv, la, lb)?;
        self.burn(div_units(la, lb) * 2);
        let (q, m) = a.divmod_floor(b);
        let q = self.new_big(&q)?;
        let m = self.new_big(&m)?;
        self.alloc_tuple(&[q, m])
    }

    fn big_pow(&mut self, a: &Big, b: &Big) -> Result<Value, Raised> {
        if b.neg {
            // As float `**`: both convert first, then the zero check.
            let x = self.big_to_f64(a)?;
            let y = self.big_to_f64(b)?;
            if a.is_zero() {
                return Err(self.err(T::ZeroDivisionError, "zero to a negative power"));
            }
            return Ok(Value::float(crate::ops::powf(x, y)));
        }
        let one = Big::from_i64(1);
        if a.is_zero() || *a == one {
            return self.new_big(if b.is_zero() { &one } else { a });
        }
        if *a == one.neg() {
            let odd = b.bit(0);
            return self.new_big(&if odd { one.neg() } else { one });
        }
        let Some(e) = b.to_i64().and_then(|e| u64::try_from(e).ok()) else {
            return Err(self.err(T::MemoryError, ""));
        };
        let bits = a.bit_length().saturating_mul(e);
        self.big_room(bits)?;
        self.work_room(pow_ns(a.bit_length(), e))?;
        let limbs = bits / 32;
        self.burn(limbs.saturating_mul(limbs) / 64 + e.min(64));
        self.new_big(&a.pow(e))
    }

    /// `pow(a, e, m)` for ints.
    pub(crate) fn big_modpow(&mut self, a: &Big, e: &Big, m: &Big) -> Result<Value, Raised> {
        if m.is_zero() {
            return Err(self.err(T::ValueError, "pow() 3rd argument cannot be 0"));
        }
        let limbs = m.mag.len() as u64;
        // For a negative `e`, the inverse first.
        let inv = if e.neg { 2 * gcd_ns(limbs, limbs) } else { 0 };
        self.work_room(modpow_ns(a.mag.len() as u64, limbs, e).saturating_add(inv))?;
        // The products, and the reduction of `a` first (review U1: `pow(a,
        // 2, 1 << 64)` of a 2^26-bit `a` charged 2 units for 41-44 ms).
        let la = a.mag.len() as u64;
        self.burn((limbs * limbs * e.bit_length() / 16).saturating_add(div_units(la, limbs)));
        if e.neg {
            let mm = m.abs();
            let Some(inv) = a.modinv(&mm) else {
                return Err(self.err(
                    T::ValueError,
                    "base is not invertible for the given modulus",
                ));
            };
            let r = inv.modpow(&e.neg(), m);
            return self.new_big(&r);
        }
        self.new_big(&a.modpow(e, m))
    }

    /// `float(b)`, or `OverflowError`.
    pub(crate) fn big_to_f64(&mut self, b: &Big) -> Result<f64, Raised> {
        match b.to_f64() {
            Some(f) => Ok(f),
            None => Err(self.err(T::OverflowError, "int too large to convert to float")),
        }
    }

    /// The float value of an int, bool or float, raising `OverflowError`
    /// for ints too large; `None` for other types.
    pub(crate) fn num_to_f64(&mut self, v: Value) -> Result<Option<f64>, Raised> {
        if let Some(f) = v.as_float() {
            return Ok(Some(f));
        }
        if let Some(i) = self.int_of(v) {
            return Ok(Some(i as f64));
        }
        match self.big_of(v) {
            Some(b) => Ok(Some(self.big_to_f64(&b)?)),
            None => Ok(None),
        }
    }

    /// Correctly rounded `a / b` (`b != 0`).
    pub(crate) fn big_truediv(&mut self, a: &Big, b: &Big) -> Result<f64, Raised> {
        const EXACT: u64 = 53;
        if a.bit_length() <= EXACT && b.bit_length() <= EXACT {
            let (x, y) = (a.to_f64().unwrap_or(0.0), b.to_f64().unwrap_or(1.0));
            return Ok(x / y);
        }
        if a.is_zero() {
            return Ok(if b.neg { -0.0 } else { 0.0 });
        }
        // CPython's `long_true_divide`: scale so the quotient has 55..56
        // bits, or fewer when it is subnormal (then the bits below 2^-1074
        // go), keep a sticky bit for the remainder, round once to the
        // remaining 53-or-fewer bits by hand; the scaling back is exact.
        let diff = a.bit_length() as i64 - b.bit_length() as i64;
        if diff > 1024 {
            return Err(self.err(
                T::OverflowError,
                "integer division result too large for a float",
            ));
        }
        if diff < -1021 - 53 - 1 {
            return Ok(if a.neg == b.neg { 0.0 } else { -0.0 });
        }
        let shift = diff.max(-1021) - 53 - 2;
        // For `shift >= 0`, |a| / (|b| << shift) as (|a| >> shift) / |b|,
        // the bits shifted out part of the remainder: no shifted copy of
        // `b` (`/` at the int cap took 50 ms [M]).
        let (q, r, lost) = if shift >= 0 {
            let (num, lost) = a.shr_trunc(shift as u64, false);
            let (q, r) = num.divmod_trunc(b);
            (q, r, lost)
        } else {
            let (q, r) = a.shl((-shift) as u64).divmod_trunc(b);
            (q, r, false)
        };
        let mut m = q.abs().to_i64().unwrap_or(0) as u64;
        let bits = i64::from(64 - m.leading_zeros());
        let extra = bits.max(-1021 - shift) - 53;
        let mask = 1u64 << (extra - 1);
        let low = m | u64::from(lost || !r.is_zero());
        if low & mask != 0 && low & (3 * mask - 1) != 0 {
            m += mask;
        }
        m &= !(2 * mask - 1);
        let f = ldexp(m as f64, shift);
        if f.is_infinite() {
            return Err(self.err(
                T::OverflowError,
                "integer division result too large for a float",
            ));
        }
        Ok(if a.neg == b.neg { f } else { -f })
    }

    /// Bit length of a heap int's magnitude, without copying it.
    pub(crate) fn heap_int_bits(&self, v: Value) -> u64 {
        let (_, bytes) = self.heap_int_parts(v);
        let Some(top) = bytes.iter().rposition(|&b| b != 0) else {
            return 0;
        };
        top as u64 * 8 + u64::from(8 - bytes[top].leading_zeros())
    }

    /// The fuel units [`Vm::big_decimal`] charges for the decimal text of
    /// `v` (0 unless a heap int): what a budgeted walk checks before it
    /// converts, so one step holds at most one costly conversion.
    pub(crate) fn decimal_units(&self, v: Value) -> u64 {
        if !Vm::is_heap_int(v) {
            return 0;
        }
        let limbs = self.heap_int_bits(v).div_ceil(32);
        limbs * limbs / 16
    }

    /// Decimal text of any int, with CPython's 4300-digit limit.
    pub(crate) fn int_decimal(&mut self, v: Value) -> Result<String, Raised> {
        if let Some(i) = self.int_of(v) {
            return Ok(format!("{i}"));
        }
        let b = self.big_of(v).unwrap_or_default();
        self.big_decimal(&b)
    }

    /// `sys.get_int_max_str_digits()`: the prelude's `_int_max_str_digits`
    /// (a heap value, so it survives a snapshot); `usize::MAX` for 0, no
    /// limit.
    pub(crate) fn str_digits_limit(&self) -> usize {
        let n = self
            .find_interned("_int_max_str_digits")
            .and_then(|k| self.dict_lookup_str(self.prelude_globals, k))
            .and_then(|v| self.int_of(v))
            .map_or(MAX_STR_DIGITS, |n| {
                usize::try_from(n).unwrap_or(MAX_STR_DIGITS)
            });
        if n == 0 { usize::MAX } else { n }
    }

    /// Decimal text of `b`, with CPython's limit (4300 digits unless
    /// `sys.set_int_max_str_digits` changed it).
    pub(crate) fn big_decimal(&mut self, b: &Big) -> Result<String, Raised> {
        let limit = self.str_digits_limit();
        // Bits above which the text surely has more digits than the limit
        // (log2(10) < 3.322), checked before the quadratic conversion.
        if b.bit_length() > (limit as u64).saturating_mul(3322) / 1000 + 2 {
            return Err(self.err(T::ValueError, &digits_error_text(limit)));
        }
        let n = b.mag.len() as u64;
        self.work_room(decimal_ns(n))?;
        self.burn(n * n / 16);
        let d = b.digits(10);
        if d.len() > limit {
            return Err(self.err(T::ValueError, &digits_error_text(limit)));
        }
        Ok(if b.neg { format!("-{d}") } else { d })
    }

    /// Parses `int(s, base)` text into an int value; `Ok(None)` if the
    /// text is not a valid literal.
    #[cfg(feature = "json")]
    pub(crate) fn parse_int_value(&mut self, s: &str, base: u32) -> Result<Option<Value>, Raised> {
        let limit = self.str_digits_limit();
        // The quadratic conversion inside a JSON scan, which cannot park
        // first: at most what a step may charge past its fuel (2^21 units,
        // about 56,000 digits, 9 ms [M]), once within the digit limit
        // (that error comes first, as in CPython).
        if s.len() <= limit {
            let n = (s.len() as u64).saturating_mul(3322) / 32_000 + 1;
            if n.saturating_mul(n) / 16 > 1 << 21 {
                return Err(self.err(T::MemoryError, ""));
            }
        }
        match parse_int_text(s, base, limit) {
            Ok(b) => {
                let n = b.mag.len() as u64;
                self.burn(n * n / 16);
                Ok(Some(self.new_big(&b)?))
            }
            Err(None) => Ok(None),
            Err(Some(n)) => {
                let msg = format!(
                    "Exceeds the limit ({limit} digits) for integer string conversion: value has {n} digits; use sys.set_int_max_str_digits() to increase the limit"
                );
                Err(self.err(T::ValueError, &msg))
            }
        }
    }

    /// Exact three-way comparison of two numbers (ints of any size,
    /// bools, floats); `None` if either is not a number, `Some(None)` if
    /// unordered (NaN).
    #[allow(clippy::option_option, reason = "not a number vs unordered")]
    pub(crate) fn num_cmp(&self, a: Value, b: Value) -> Option<Option<Ordering>> {
        let fa = a.as_float();
        let fb = b.as_float();
        match (fa, fb) {
            (Some(x), Some(y)) => Some(x.partial_cmp(&y)),
            (None, None) => Some(Some(self.big_of(a)?.cmp(&self.big_of(b)?))),
            (Some(x), None) => Some(big_cmp_float(&self.big_of(b)?, x).map(Ordering::reverse)),
            (None, Some(y)) => Some(big_cmp_float(&self.big_of(a)?, y)),
        }
    }

    /// An index-sized integer for `v` (an int of any size): `Err` with
    /// CPython's message if it does not fit.
    pub(crate) fn index_overflow(&mut self, t: T) -> Raised {
        self.err(t, "cannot fit 'int' into an index-sized integer")
    }
}

/// Exact comparison of an int with a float.
pub(crate) fn big_cmp_float(b: &Big, f: f64) -> Option<Ordering> {
    if f.is_nan() {
        return None;
    }
    if f.is_infinite() {
        return Some(if f > 0.0 {
            Ordering::Less
        } else {
            Ordering::Greater
        });
    }
    let t = Big::from_f64(f);
    match b.cmp(&t) {
        Ordering::Equal => {
            let frac = crate::ops::fract(f);
            Some(if frac > 0.0 {
                Ordering::Less
            } else if frac < 0.0 {
                Ordering::Greater
            } else {
                Ordering::Equal
            })
        }
        o => Some(o),
    }
}

/// `x * 2^e`.
// Out of line: LLVM inlines it into each of its callers otherwise.
#[inline(never)]
pub(crate) fn ldexp(x: f64, e: i64) -> f64 {
    if x == 0.0 || !x.is_finite() {
        return x;
    }
    let pow2 = |k: i64| f64::from_bits(((1023 + k) as u64) << 52);
    // x = m * 2^k with 1 <= |m| < 2.
    let (mut m, mut k) = (x, 0i64);
    if x.to_bits() & (0x7FF << 52) == 0 {
        m *= pow2(54);
        k -= 54;
    }
    let bexp = ((m.to_bits() >> 52) & 0x7FF) as i64;
    k += bexp - 1023;
    m = f64::from_bits((m.to_bits() & !(0x7FF << 52)) | (1023 << 52));
    let total = k.saturating_add(e.clamp(-1 << 20, 1 << 20));
    if total > 1023 {
        return if x > 0.0 {
            f64::INFINITY
        } else {
            f64::NEG_INFINITY
        };
    }
    if total >= -1022 {
        return m * pow2(total);
    }
    if total < -2044 {
        return x * 0.0;
    }
    // Exact into the normal range, then one rounding into subnormals.
    m * pow2(total + 1022) * pow2(-1022)
}

/// Parses `int()` text ([`IntScan`]) in one go. `Err(None)` for an
/// invalid literal, `Err(Some(n))` for a valid one past the digit limit
/// (non-power-of-two bases).
pub(crate) fn parse_int_text(s: &str, base: u32, limit: usize) -> Result<Big, Option<usize>> {
    let mut sc = IntScan::new(base);
    let mut at = 0;
    crate::complex::scan_text(&mut sc, s.as_bytes(), true, &mut at, usize::MAX, &mut []);
    if !sc.finish() {
        return Err(None);
    }
    if sc.over(limit) {
        return Err(Some(usize::try_from(sc.digits).unwrap_or(usize::MAX)));
    }
    if sc.zero_lead_bad() {
        return Err(None);
    }
    sc.big(s.as_bytes(), true).ok_or(None)
}

// ----------------------------------------------- int() text, in steps

/// `int()` text, read a piece at a time (CPython's `long_from_string_base`
/// over the ASCII `complex::wide_char` makes): whitespace, a sign, a base
/// prefix, digits with single underscores between them, whitespace. It
/// counts the digits and the significant ones, keeps the first nonzero
/// one, and the byte range from the first digit to the last, which the
/// conversion reads again. Five words of state, so it parks between steps.
#[derive(Clone, Copy, Default)]
pub(crate) struct IntScan {
    phase: u8,
    base: u8,
    flags: u8,
    top: u8,
    digits: u64,
    sig: u64,
    start: u64,
    end: u64,
}

/// [`IntScan`] phases: leading whitespace, after the sign, after a leading
/// `0`, after a base prefix, before the first digit, in the digits, in
/// trailing whitespace, invalid; then converting the digits.
const LEAD: u8 = 0;
const SIGNED: u8 = 1;
const ZERO: u8 = 2;
const PREFIXED: u8 = 3;
const FIRST: u8 = 4;
const DIGITS: u8 = 5;
const TRAIL: u8 = 6;
const INVALID: u8 = 7;
const CONVERT: u8 = 8;
/// Invalid and long: looking for the quotes that decide its repr's.
const QUOTES: u8 = 9;

/// [`IntScan`] flags: a `-`, an underscore just read, base 0's leading
/// zero (no other digit may follow); in [`QUOTES`], a `'` and a `"` seen.
const NEG: u8 = 1;
const UNDER: u8 = 2;
const ZLEAD: u8 = 4;
const SQ: u8 = 8;
const DQ: u8 = 16;

/// Bytes of an invalid `int()` text searched for quotes a step.
const QUOTE_STEP: usize = 1 << 24;

/// Bytes of an invalid `int()` text its error message shows at most: 200
/// chars (`%.200R`) need at most 800.
const SHOWN: usize = 800;

/// Digits converted in the native call: past these an `int()` text's
/// digits are converted in steps ([`Vm::int_text_step`]).
const SHORT_POW2: u64 = 1 << 20;
const SHORT_RADIX: u64 = 1 << 13;

/// Digits a step of a power-of-two conversion reads, and limb
/// multiply-adds a step of another base's does (each a few milliseconds).
const CONV_DIGITS: usize = 1 << 21;
const CONV_WORK: usize = 1 << 22;

/// The value of ASCII digit `c` in `base`.
fn digit_of(c: u8, base: u8) -> Option<u8> {
    let d = match c {
        b'0'..=b'9' => c - b'0',
        b'a'..=b'z' => c - b'a' + 10,
        b'A'..=b'Z' => c - b'A' + 10,
        _ => return None,
    };
    (d < base).then_some(d)
}

/// The ASCII digits of `text[*at..end]`, underscores skipped, into `out`
/// (until it is full); returns how many, and what reading them counts
/// towards `complex::TEXT_STEP`.
#[inline(never)]
fn int_digits(
    text: &[u8],
    is_str: bool,
    at: &mut usize,
    end: usize,
    out: &mut [u8],
) -> (usize, usize) {
    let end = end.min(text.len());
    let mut n = 0;
    let mut spent = 0;
    let mut i = *at;
    while i < end && n < out.len() {
        spent += 1;
        let (c, w) = match text.get(i) {
            Some(&b) if b < 0x80 => (b, 1),
            _ => {
                spent += crate::complex::WIDE - 1;
                crate::complex::wide_char(text, i, is_str)
            }
        };
        if c != b'_'
            && let Some(o) = out.get_mut(n)
        {
            *o = c;
            n += 1;
        }
        i += w;
    }
    *at = i;
    (n, spent)
}

impl crate::complex::TextFeed for IntScan {
    fn feed(&mut self, c: u8, i: usize, w: usize, _: &mut [u8]) {
        self.feed(c, i as u64, w as u64);
    }

    fn done(&self) -> bool {
        self.phase == INVALID
    }

    fn words(&self) -> [u64; 5] {
        [
            u64::from(self.phase)
                | u64::from(self.base) << 8
                | u64::from(self.flags) << 16
                | u64::from(self.top) << 24,
            self.digits,
            self.sig,
            self.start,
            self.end,
        ]
    }
}

impl IntScan {
    fn new(base: u32) -> IntScan {
        IntScan {
            base: base as u8,
            ..IntScan::default()
        }
    }

    fn from_words(w: [u64; 5]) -> IntScan {
        IntScan {
            phase: w[0] as u8,
            base: (w[0] >> 8) as u8,
            flags: (w[0] >> 16) as u8,
            top: (w[0] >> 24) as u8,
            digits: w[1],
            sig: w[2],
            start: w[3],
            end: w[4],
        }
    }

    fn pow2(&self) -> bool {
        self.base.is_power_of_two()
    }

    /// Past CPython's digit limit (non-power-of-two bases only).
    fn over(&self, limit: usize) -> bool {
        !self.pow2() && self.digits > limit as u64
    }

    fn feed(&mut self, c: u8, i: u64, w: u64) {
        let space = crate::complex::num_space(c);
        loop {
            match self.phase {
                LEAD | TRAIL if space => {}
                LEAD if c == b'+' || c == b'-' => {
                    if c == b'-' {
                        self.flags |= NEG;
                    }
                    self.phase = SIGNED;
                }
                LEAD => {
                    self.phase = SIGNED;
                    continue;
                }
                SIGNED if c == b'0' => {
                    self.start = i;
                    self.phase = ZERO;
                }
                SIGNED => {
                    if self.base == 0 {
                        self.base = 10;
                    }
                    self.phase = FIRST;
                    continue;
                }
                ZERO => {
                    let p = match c | 0x20 {
                        b'x' => 16,
                        b'o' => 8,
                        b'b' => 2,
                        _ => 0,
                    };
                    if p != 0 && (self.base == 0 || self.base == p) {
                        self.base = p;
                        self.phase = PREFIXED;
                    } else {
                        self.zero();
                        continue;
                    }
                }
                // One underscore may follow a prefix.
                PREFIXED => {
                    self.phase = FIRST;
                    if c != b'_' {
                        continue;
                    }
                }
                FIRST => match digit_of(c, self.base) {
                    Some(d) => {
                        self.digit(d, i, w);
                        self.phase = DIGITS;
                    }
                    None => self.phase = INVALID,
                },
                DIGITS => match digit_of(c, self.base) {
                    Some(d) => {
                        self.digit(d, i, w);
                        self.flags &= !UNDER;
                    }
                    None if c == b'_' && self.flags & UNDER == 0 => self.flags |= UNDER,
                    None if space && self.flags & UNDER == 0 => self.phase = TRAIL,
                    _ => self.phase = INVALID,
                },
                _ => self.phase = INVALID,
            }
            return;
        }
    }

    /// The leading `0` was a digit, not a prefix's.
    fn zero(&mut self) {
        if self.base == 0 {
            self.base = 10;
            self.flags |= ZLEAD;
        }
        self.digit(0, self.start, 1);
        self.phase = DIGITS;
    }

    fn digit(&mut self, d: u8, i: u64, w: u64) {
        if self.digits == 0 {
            self.start = i;
        }
        self.end = i + w;
        self.digits += 1;
        if self.sig > 0 || d != 0 {
            if self.sig == 0 {
                self.top = d;
            }
            self.sig += 1;
        }
    }

    /// The end of the text: whether it was a valid literal.
    fn finish(&mut self) -> bool {
        if self.phase == ZERO {
            self.zero();
        }
        (self.phase == DIGITS && self.flags & UNDER == 0) || self.phase == TRAIL
    }

    /// Base 0's leading zero was followed by a nonzero digit: invalid,
    /// once the digit limit (checked first, as CPython does) is not hit.
    fn zero_lead_bad(&self) -> bool {
        self.flags & ZLEAD != 0 && self.sig > 0
    }

    fn neg(&self) -> bool {
        self.flags & NEG != 0
    }

    /// The value of the digits of a valid `text`, in one go.
    fn big(&self, text: &[u8], is_str: bool) -> Option<Big> {
        if self.sig == 0 {
            return Some(Big::zero());
        }
        let mut digits = alloc::vec![0u8; usize::try_from(self.digits).ok()?];
        let mut at = usize::try_from(self.start).ok()?;
        let end = usize::try_from(self.end).ok()?;
        let (n, _) = int_digits(text, is_str, &mut at, end, &mut digits);
        let b = Big::parse(digits.get(..n)?, u32::from(self.base))?;
        Some(if self.neg() { b.neg() } else { b })
    }

    /// Bits of the value: exact for power-of-two bases, else at most.
    fn bits(&self) -> u64 {
        if self.pow2() {
            let k = u64::from(self.base.trailing_zeros());
            (self.sig - 1) * k + u64::from(8 - self.top.leading_zeros())
        } else {
            let per = crate::bigint::radix_chunk(u32::from(self.base)).1 as u64;
            self.sig.div_ceil(per) * 32
        }
    }
}

impl Vm {
    /// CPython's error for an `int()` text that is no literal: its repr cut
    /// to 200 chars (`%.200R`). A long str's repr is rendered from its
    /// first [`SHOWN`] bytes, the quotes found in all of it (`quotes`,
    /// [`SQ`] and [`DQ`]) choosing its quote.
    fn int_invalid(&mut self, v: Value, base: u64, quotes: u8) -> Raised {
        let mut shown = String::new();
        let (data, is_str) = self.number_text(v);
        let mut head = Vec::new();
        let data = if !is_str {
            // CPython reprs bytes' first 200 only.
            data.get(..200).unwrap_or(data)
        } else if data.len() <= 2 * SHOWN {
            data
        } else {
            let mut k = SHOWN;
            while data.get(k).is_some_and(|&b| b & 0xC0 == 0x80) {
                k -= 1;
            }
            head.extend_from_slice(data.get(..k).unwrap_or(&[]));
            // Past the 200 chars shown: the quote the whole text decides.
            head.extend_from_slice(match (quotes & SQ != 0, quotes & DQ != 0) {
                (true, false) => b"'",
                (true, true) => b"'\"",
                _ => b"\"",
            });
            &head
        };
        crate::text::quoted_repr(
            data,
            !is_str,
            &mut crate::text::ReprWriter::new(&mut shown, SHOWN),
        );
        let shown: String = shown.chars().take(200).collect();
        let msg = format!("invalid literal for int() with base {base}: {shown}");
        self.err(T::ValueError, &msg)
    }

    /// Searches a long invalid `int()` text for quotes from `*at`, a step's
    /// worth; whether it is done.
    fn int_quotes(&mut self, v: Value, sc: &mut IntScan, at: &mut usize) -> bool {
        let (text, _) = self.number_text(v);
        let end = text.len().min(at.saturating_add(QUOTE_STEP));
        let part = text.get(*at..end).unwrap_or(&[]);
        if part.contains(&b'\'') {
            sc.flags |= SQ;
        }
        if part.contains(&b'"') {
            sc.flags |= DQ;
        }
        let done = end == text.len() || sc.flags & (SQ | DQ) == SQ | DQ;
        self.burn_bytes(end - *at);
        *at = end;
        done
    }

    fn int_digits_error(&mut self, limit: usize, n: u64) -> Raised {
        let msg = format!(
            "Exceeds the limit ({limit} digits) for integer string conversion: value has {n} digits; use sys.set_int_max_str_digits() to increase the limit"
        );
        self.err(T::ValueError, &msg)
    }

    /// `int(s, base)` for str or bytes-like `s` (base 0 or 2..=36): its
    /// first step, the rest in `_steps` (`nstep::op::NUM_TEXT`) when the
    /// text or its conversion is long.
    pub(crate) fn int_from_text(&mut self, v: Value, base: u32) -> Result<Value, Raised> {
        let mut sc = IntScan::new(base);
        let mut at = 0;
        let step = if self.text_part(v, &mut sc, &mut at, &mut []) {
            self.int_state(v, u64::from(base), at, sc, Value::NONE, Value::NONE)?
        } else {
            self.int_scanned(v, u64::from(base), sc, Value::NONE, 0)?
        };
        match step {
            crate::nstep::Step::Done(r) => Ok(r),
            crate::nstep::Step::More => Err(self.bad_step()),
        }
    }

    /// The text is read: its error, its value, or (long digits) the
    /// conversion's first step to come, in `state` (a new one if `NONE`);
    /// `at` where a search for quotes ([`QUOTES`]) goes on.
    fn int_scanned(
        &mut self,
        v: Value,
        base: u64,
        mut sc: IntScan,
        state: Value,
        mut at: usize,
    ) -> Result<crate::nstep::Step, Raised> {
        use crate::nstep::Step;
        let limit = self.str_digits_limit();
        let mut bad = sc.phase == QUOTES;
        if !bad {
            bad = !sc.finish();
            if !bad && sc.over(limit) {
                return Err(self.int_digits_error(limit, sc.digits));
            }
            bad |= sc.zero_lead_bad();
        }
        if bad {
            // Its repr's quote needs the whole text read (`int_invalid`).
            let (text, is_str) = self.number_text(v);
            let long = is_str && text.len() > 2 * SHOWN;
            if sc.phase != QUOTES {
                at = 0;
            }
            sc.phase = QUOTES;
            if !long || self.int_quotes(v, &mut sc, &mut at) {
                return Err(self.int_invalid(v, base, sc.flags));
            }
            return self.int_state(v, base, at, sc, Value::NONE, state);
        }
        let short = if sc.pow2() {
            sc.digits <= SHORT_POW2
        } else {
            sc.digits <= SHORT_RADIX
        };
        if short || sc.sig == 0 {
            let (text, is_str) = self.number_text(v);
            let Some(b) = sc.big(text, is_str) else {
                return Err(self.bad_step());
            };
            let n = b.mag.len() as u64;
            self.burn(if sc.pow2() { n / 8 } else { n * n / 16 });
            return self.new_big(&b).map(Step::Done);
        }
        let bits = sc.bits();
        self.big_room(bits)?;
        let block = {
            let res = self.heap.alloc_bytes_zeroed(bits.div_ceil(32) as usize * 4);
            self.hx(res)
        }?;
        sc.phase = CONVERT;
        let at = usize::try_from(sc.start).unwrap_or(usize::MAX);
        self.int_state(v, base, at, sc, Value::from_byte_ref(block), state)
    }

    /// Saves the progress of a long `int()` text at `at` in `state`, or a
    /// new state for the steps to come if it is `NONE`; `block` the
    /// result's magnitude (`NONE` until the conversion).
    fn int_state(
        &mut self,
        v: Value,
        base: u64,
        at: usize,
        sc: IntScan,
        block: Value,
        state: Value,
    ) -> Result<crate::nstep::Step, Raised> {
        use crate::complex::TextFeed;
        let Some(r) = state.as_obj() else {
            return self
                .text_rest(v, base + 1, block, at, sc.words())
                .map(crate::nstep::Step::Done);
        };
        self.set_slot(r, 1, block);
        self.text_save(state, at, sc.words(), [0, 0]);
        Ok(crate::nstep::Step::More)
    }

    /// One step of a long `int()` text in `state` ([`Vm::text_step`]):
    /// reading it, or converting its digits into the result's magnitude
    /// (`block`).
    #[allow(clippy::too_many_arguments, reason = "the state's words")]
    pub(crate) fn int_text_step(
        &mut self,
        state: Value,
        v: Value,
        block: Value,
        base: u64,
        mut at: usize,
        w: [u64; 5],
        conv: [u64; 2],
    ) -> Result<crate::nstep::Step, Raised> {
        use crate::complex::TextFeed;
        use crate::nstep::Step;
        let mut sc = IntScan::from_words(w);
        if sc.phase == QUOTES {
            return self.int_scanned(v, base, sc, state, at);
        }
        if sc.phase != CONVERT {
            if self.text_part(v, &mut sc, &mut at, &mut []) {
                self.text_save(state, at, sc.words(), [0, 0]);
                return Ok(Step::More);
            }
            return self.int_scanned(v, base, sc, state, 0);
        }
        let Some(block) = block.as_byte_ref() else {
            return Err(self.bad_step());
        };
        let end = usize::try_from(sc.end).unwrap_or(0);
        let (mut idx, mut len) = (conv[0], usize::try_from(conv[1]).unwrap_or(usize::MAX));
        let mut buf = [0u8; 4096];
        let mut work = 0;
        let (chunk, per) = crate::bigint::radix_chunk(u32::from(sc.base));
        let k = u64::from(sc.base.trailing_zeros());
        while at < end && work < CONV_WORK {
            // Whole groups of `per` digits (the last may be short), as
            // many as the work left allows.
            let want = if sc.pow2() {
                buf.len()
            } else {
                per * ((CONV_WORK - work) / len.max(1)).clamp(1, buf.len() / per)
            };
            let (text, is_str) = self.number_text(v);
            let (m, spent) = int_digits(text, is_str, &mut at, end, &mut buf[..want]);
            work += spent * CONV_WORK / CONV_DIGITS;
            let Ok(mag) = self.heap.bytes_mut(block) else {
                return Err(self.bad_step());
            };
            let digits = buf.get(..m).unwrap_or(&[]);
            if sc.pow2() {
                for &c in digits {
                    let d = crate::bigint::digit_value(c).unwrap_or(0);
                    let Some(p) = sc.digits.checked_sub(idx + 1) else {
                        return Err(self.bad_step());
                    };
                    if d != 0 {
                        crate::bigint::or_digit(mag, p * k, d);
                    }
                    idx += 1;
                }
            } else {
                for group in digits.chunks(per) {
                    let (mut x, mut mult) = (0u32, 1u32);
                    for &c in group {
                        x = x * u32::from(sc.base) + crate::bigint::digit_value(c).unwrap_or(0);
                        mult = mult.saturating_mul(u32::from(sc.base));
                    }
                    let mult = if group.len() == per { chunk } else { mult };
                    len = crate::bigint::mul_add(mag, len, mult, x);
                    work += len.max(1);
                }
                idx += m as u64;
            }
            if m == 0 {
                if at < end {
                    // No digit before `end`: a forged end past the text,
                    // which would park for ever without progress.
                    return Err(self.bad_step());
                }
                break;
            }
        }
        self.burn(work as u64 / 8);
        if at < end {
            self.text_save(state, at, sc.words(), [idx, len as u64]);
            return Ok(Step::More);
        }
        self.int_of_block(sc.neg(), block).map(Step::Done)
    }

    /// The int whose magnitude is byte block `block` (little-endian limbs,
    /// maybe high zero ones): the block itself when exact and long.
    fn int_of_block(&mut self, neg: bool, block: mm_core::ByteRef) -> Result<Value, Raised> {
        let Ok(mag) = self.heap.bytes(block) else {
            return Err(self.bad_step());
        };
        let used = mag.iter().rposition(|&b| b != 0).map_or(0, |i| i / 4 + 1) * 4;
        if used as u64 > MAX_BITS / 8 {
            return Err(self.err(T::MemoryError, ""));
        }
        if used <= 16 {
            let b = Big::from_bytes_le(mag.get(..used).unwrap_or(&[]), false);
            return self.new_big(&if neg { b.neg() } else { b });
        }
        let block = if used == mag.len() {
            block
        } else {
            let res = self.heap.alloc_bytes_zeroed(used);
            let exact = self.hx(res)?;
            let res = self.heap.copy_bytes(block, 0, exact, 0, used);
            self.hx(res)?;
            exact
        };
        self.burn_result(used, true);
        let r = {
            let res = self.heap.alloc_init(
                ty::INT,
                0,
                &[Value::from_byte_ref(block)],
                &[u64::from(neg)],
            );
            self.hx(res)
        }?;
        Ok(Value::from_ref(r, ty::INT as u8))
    }
}

// ------------------------------------------------------------ int methods

use crate::vm::CallArgs;

fn int_self(vm: &mut Vm, a: &CallArgs, name: &str) -> Result<Big, Raised> {
    let v = vm.arg(a, 0);
    if let Some(b) = vm.big_of(v) {
        vm.burn_bytes(b.mag.len() * 4);
        return Ok(b);
    }
    let msg = format!(
        "descriptor '{name}' for 'int' objects doesn't apply to a '{}' object",
        vm.type_name(v)
    );
    Err(vm.err(T::TypeError, &msg))
}

pub(crate) fn int_bit_length(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let b = int_self(vm, &a, "bit_length")?;
    vm.new_int(b.bit_length() as i64)
}

pub(crate) fn int_bit_count(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let b = int_self(vm, &a, "bit_count")?;
    vm.new_int(b.count_ones() as i64)
}

pub(crate) fn int_conjugate(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let b = int_self(vm, &a, "conjugate")?;
    vm.new_big(&b)
}

pub(crate) fn int_is_integer(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    int_self(vm, &a, "is_integer")?;
    Ok(Value::TRUE)
}

pub(crate) fn int_as_integer_ratio(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let b = int_self(vm, &a, "as_integer_ratio")?;
    let n = vm.new_big(&b)?;
    vm.alloc_tuple(&[n, Value::int(1).unwrap_or(Value::NONE)])
}

/// `byteorder` of `to_bytes`/`from_bytes` (`name`): `true` for
/// little-endian.
fn byteorder(vm: &mut Vm, v: Value, name: &str) -> Result<bool, Raised> {
    if v == Value::UNDEFINED {
        return Ok(false);
    }
    if !Vm::is_str(v) {
        let msg = format!(
            "{name}() argument 'byteorder' must be str, not {}",
            if v == Value::NONE {
                "None"
            } else {
                vm.type_name(v)
            }
        );
        return Err(vm.err(T::TypeError, &msg));
    }
    match vm.str_ref(v) {
        "little" => Ok(true),
        "big" => Ok(false),
        _ => Err(vm.err(T::ValueError, "byteorder must be either 'little' or 'big'")),
    }
}

pub(crate) fn int_to_bytes(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    // A heap int's magnitude is read in place (`mag`), a small one's from
    // `small`: no guest-sized copy besides the result.
    let v = vm.arg(&a, 0);
    let (neg, mag, small) = if let Some(r) = v.as_obj().filter(|_| Vm::is_heap_int(v)) {
        (vm.raw(r, 1) == 1, vm.slot(r, 0).as_byte_ref(), [0; 8])
    } else {
        let b = int_self(vm, &a, "to_bytes")?;
        let m = b.abs().to_bytes_le(8, false).unwrap_or_default();
        (b.neg, None, m.try_into().unwrap_or([0; 8]))
    };
    let len = match vm.arg(&a, 1) {
        Value::UNDEFINED => 1,
        v => vm.int_arg(v, "length")?,
    };
    let little = byteorder(vm, vm.arg(&a, 2), "to_bytes")?;
    let signed = {
        let s = vm.kwarg(&a, 0);
        s != Value::UNDEFINED && vm.truthy(s)
    };
    if len < 0 {
        return Err(vm.err(T::ValueError, "length argument must be non-negative"));
    }
    let len = usize::try_from(len).unwrap_or(usize::MAX);
    if len > (1 << 30) || !vm.heap.would_fit(len as u64) {
        return Err(vm.err(T::MemoryError, ""));
    }
    if neg && !signed {
        return Err(vm.err(T::OverflowError, "can't convert negative int to unsigned"));
    }
    // Its bytes, and whether it fits: a negative one may be `-2**(8n-1)`.
    let fit = |own: &[u8]| {
        let n = own.iter().rposition(|&b| b != 0).map_or(0, |i| i + 1);
        let top = own.get(n.wrapping_sub(1)).copied().unwrap_or(0);
        let bits = (n as u64 * 8).saturating_sub(u64::from(top.leading_zeros()));
        let low = own.get(..n.saturating_sub(1)).unwrap_or(&[]);
        let pow2 = top.is_power_of_two() && low.iter().all(|&b| b == 0);
        (own.len(), n, bits, pow2)
    };
    let (size, n, bits, pow2) = match mag {
        Some(m) => fit(vm.heap.bytes(m).unwrap_or(&[])),
        None => fit(&small),
    };
    vm.burn_bytes(size);
    let (bits, room) = (i128::from(bits), len as i128 * 8 - i128::from(signed));
    if bits > room.max(0) && !(neg && pow2 && bits == room + 1) {
        return Err(vm.err(T::OverflowError, "int too big to convert"));
    }
    // A 64 MiB result is built in about 31 ms, native release [M].
    let at = if little { 0 } else { len - n };
    vm.burn_result(len, true);
    let block = {
        let res = vm.heap.alloc_bytes_zeroed(len);
        vm.hx(res)
    }?;
    if let Some(m) = mag
        && vm.heap.copy_bytes(m, 0, block, at, n).is_err()
    {
        return Err(vm.bad_step());
    }
    let Ok(d) = vm.heap.bytes_mut(block) else {
        return Err(vm.bad_step());
    };
    if let Some(o) = d.get_mut(at..at + n) {
        if mag.is_none()
            && let Some(m) = small.get(..n)
        {
            o.copy_from_slice(m);
        }
        if !little {
            o.reverse();
        }
    }
    finish_bytes(d, n, neg, little);
    if neg {
        vm.burn_bytes(len);
    }
    vm.bytes_of_block(block)
}

/// Turns `d`, a magnitude in its byte order (little-endian with `little`,
/// its `n` bytes at the start, else at the end, zeros past them), into
/// the two's complement of its negation when `neg`: the lowest nonzero
/// byte negated, every byte above it inverted, the zeros past it `0xFF`.
fn finish_bytes(d: &mut [u8], n: usize, neg: bool, little: bool) {
    if !neg || n == 0 {
        return;
    }
    let at = if little { n } else { d.len().saturating_sub(n) };
    let Some((lo, hi)) = d.split_at_mut_checked(at) else {
        return;
    };
    let (m, pad) = if little { (lo, hi) } else { (hi, lo) };
    pad.fill(0xFF);
    if little {
        if let Some(i) = m.iter().position(|&b| b != 0) {
            m[i] = m[i].wrapping_neg();
            m[i + 1..].iter_mut().for_each(|b| *b = !*b);
        }
    } else if let Some(i) = m.iter().rposition(|&b| b != 0) {
        m[i] = m[i].wrapping_neg();
        m[..i].iter_mut().for_each(|b| *b = !*b);
    }
}

impl Vm {
    /// `int.from_bytes` of the long bytes in block `src`, read in place:
    /// copied into the result's block in its byte order, then negated
    /// there if it is signed and negative.
    fn int_of_bytes(
        &mut self,
        src: mm_core::ByteRef,
        n: usize,
        little: bool,
        signed: Value,
    ) -> Result<Value, Raised> {
        let signed = signed != Value::UNDEFINED && self.truthy(signed);
        self.burn_bytes(n);
        let block = {
            let res = self.heap.alloc_bytes_zeroed(n.div_ceil(4) * 4);
            self.hx(res)
        }?;
        if self.heap.copy_bytes(src, 0, block, 0, n).is_err() {
            return Err(self.bad_step());
        }
        let Some(d) = self.heap.bytes_mut(block).ok().and_then(|d| d.get_mut(..n)) else {
            return Err(self.bad_step());
        };
        if !little {
            d.reverse();
        }
        let neg = signed && d.last().is_some_and(|&b| b & 0x80 != 0);
        finish_bytes(d, n, neg, true);
        self.int_of_block(neg, block)
    }

    /// The `bytes` object over byte block `b`.
    fn bytes_of_block(&mut self, b: mm_core::ByteRef) -> Result<Value, Raised> {
        let res = self
            .heap
            .alloc_init(ty::BYTES, 0, &[Value::from_byte_ref(b)], &[]);
        let r = self.hx(res)?;
        Ok(Value::from_ref(r, ty::BYTES as u8))
    }
}

/// `int.from_bytes(bytes, byteorder='big', *, signed=False)`: a `bytes`
/// or `bytearray` object's block read in place, another iterable's bytes put
/// in one.
pub(crate) fn int_from_bytes(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let src = vm.arg(&a, 1);
    // CPython reads the byte order before the bytes.
    let little = byteorder(vm, vm.arg(&a, 2), "from_bytes")?;
    // A `bytes` block whole, a `bytearray`'s up to its length: read in
    // place, in one go (32 ms at 64 MiB [M]).
    let held = if Vm::is_bytes(src) {
        vm.vslot(src, 0).as_byte_ref().map(|b| {
            let n = vm.heap.bytes(b).map_or(0, <[u8]>::len);
            (b, n)
        })
    } else if let Some(r) = src
        .as_obj()
        .filter(|_| src.is_obj_kind(ty::BYTEARRAY as u8))
    {
        let n = usize::try_from(vm.raw(r, 1)).unwrap_or(0);
        vm.slot(r, 0).as_byte_ref().map(|b| (b, n))
    } else {
        None
    };
    let (block, n) = match held {
        Some(b) => b,
        _ if Vm::is_str(src) || Vm::is_intlike(src) || src.is_float() => {
            let msg = format!("cannot convert '{}' object to bytes", vm.type_name(src));
            return Err(vm.err(T::TypeError, &msg));
        }
        // `bytes(src)` in `_int_from_bytes`, as CPython's `PyObject_Bytes`.
        _ if vm.user_method(src, crate::class::Dn::Bytes).is_some() => {
            return Err(vm.need_py());
        }
        _ => {
            // As `bytes()` reads it; `__index__` in `_int_from_bytes`.
            let items = vm.collect(src)?;
            let (_scratch, out) = vm.ints_to_bytes(&items, false)?;
            let res = vm.heap.alloc_bytes(&out);
            (vm.hx(res)?, out.len())
        }
    };
    let v = vm.int_of_bytes(block, n, little, vm.kwarg(&a, 0))?;
    // CPython calls a subclass, `bool` included, with the value.
    let cls = vm.arg(&a, 0);
    if Vm::is_type(cls) && cls != vm.type_obj(T::int) {
        return Ok(vm.request_call(cls, alloc::vec![v], crate::cont::Cont::None));
    }
    Ok(v)
}

#[allow(clippy::unnecessary_wraps, reason = "the native calling convention")]
pub(crate) fn float_is_integer(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let f = vm.arg(&a, 0).as_float().unwrap_or(0.0);
    Ok(Value::from_bool(
        f.is_finite() && crate::ops::fract(f) == 0.0,
    ))
}

#[allow(clippy::unnecessary_wraps, reason = "the native calling convention")]
pub(crate) fn float_conjugate(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    Ok(Value::float(vm.arg(&a, 0).as_float().unwrap_or(0.0)))
}

pub(crate) fn float_as_integer_ratio(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let f = vm.arg(&a, 0).as_float().unwrap_or(0.0);
    if f.is_nan() {
        return Err(vm.err(T::ValueError, "cannot convert NaN to integer ratio"));
    }
    if f.is_infinite() {
        return Err(vm.err(T::OverflowError, "cannot convert Infinity to integer ratio"));
    }
    // f = mant * 2^e exactly; reduce by the common power of two.
    let bits = f.to_bits();
    let exp = ((bits >> 52) & 0x7FF) as i64;
    let mut mant = bits & ((1 << 52) - 1);
    let mut e = if exp == 0 {
        -1074
    } else {
        mant |= 1 << 52;
        exp - 1075
    };
    if mant == 0 {
        let z = Value::int(0).unwrap_or(Value::NONE);
        return vm.alloc_tuple(&[z, Value::int(1).unwrap_or(Value::NONE)]);
    }
    let tz = i64::from(mant.trailing_zeros());
    mant >>= tz;
    e += tz;
    let neg = f < 0.0;
    let (num, den) = if e >= 0 {
        (Big::from_u64(mant, neg).shl(e as u64), Big::from_i64(1))
    } else {
        (Big::from_u64(mant, neg), Big::from_i64(1).shl((-e) as u64))
    };
    let n = vm.new_big(&num)?;
    let d = vm.new_big(&den)?;
    vm.alloc_tuple(&[n, d])
}

/// `float.hex()`: CPython's `float_hex`, from the bits.
pub(crate) fn float_hex(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let f = vm.unbx(vm.arg(&a, 0)).as_float().unwrap_or(0.0);
    if !f.is_finite() || f == 0.0 {
        let s = match f {
            _ if f.is_nan() => "nan",
            _ if f.is_infinite() => {
                if f > 0.0 {
                    "inf"
                } else {
                    "-inf"
                }
            }
            _ if f.is_sign_negative() => "-0x0.0p+0",
            _ => "0x0.0p+0",
        };
        return vm.new_str(s);
    }
    let bits = f.to_bits();
    let frac = bits & ((1 << 52) - 1);
    let bexp = ((bits >> 52) & 0x7FF) as i64;
    // Subnormals keep a leading 0 and the smallest exponent.
    let (lead, e) = if bexp == 0 {
        (0, -1022)
    } else {
        (1, bexp - 1023)
    };
    let sign = if f < 0.0 { "-" } else { "" };
    vm.new_str(&format!("{sign}0x{lead}.{frac:013x}p{e:+}"))
}

/// `float.fromhex(s)` (a class method): CPython's `float_fromhex`, which
/// rounds half to even into the 53-bit significand.
pub(crate) fn float_fromhex(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let s = vm.unbx(vm.arg(&a, 1));
    if !Vm::is_str(s) {
        return Err(vm.err(T::TypeError, "bad argument type for built-in operation"));
    }
    // Called on an instance (`b''.fromhex`), the receiver is not a type.
    let cls = vm.arg(&a, 0);
    if Vm::is_type(cls) && cls != vm.type_obj(T::float) {
        // A subclass: `cls(float.fromhex(s))` in the prelude.
        return Err(vm.need_py());
    }
    // Byte work, one unit per 64 bytes read (a word at a time).
    vm.burn_bytes(vm.str_bytes(s).len());
    match hex_to_f64(vm.str_bytes(s)) {
        Ok(x) => Ok(Value::float(x)),
        Err(true) => Err(vm.err(
            T::OverflowError,
            "hexadecimal value too large to represent as a float",
        )),
        Err(false) => Err(vm.err(T::ValueError, "invalid hexadecimal floating-point string")),
    }
}

/// A word with each byte `0x01`.
const ONES: u64 = u64::MAX / 255;
/// Each byte's high bit.
const HIGH: u64 = 0x80 * ONES;
/// Eight ASCII `0`s.
const ZEROS: u64 = 0x30 * ONES;

/// The high bit of each byte of ASCII word `w` in `lo..=hi` (no byte
/// carries into the next). Its callers test `w & HIGH` without a branch,
/// so it also runs on non-ASCII words, whose sums may wrap: the result
/// is then meaningless and ignored.
#[inline]
fn in_range(w: u64, lo: u64, hi: u64) -> u64 {
    w.wrapping_add((0x80 - lo) * ONES) & !w.wrapping_add((0x7F - hi) * ONES) & HIGH
}

/// Whether every byte of `w` is an ASCII decimal digit.
#[inline]
fn dec_word(w: u64) -> bool {
    (w & HIGH == 0) & (in_range(w, b'0'.into(), b'9'.into()) == HIGH)
}

/// Whether every byte of `w` is an ASCII hex digit.
#[inline]
fn hex_word(w: u64) -> bool {
    (w & HIGH == 0)
        & (in_range(w, b'0'.into(), b'9'.into())
            | in_range(w | (0x20 * ONES), b'a'.into(), b'f'.into())
            == HIGH)
}

/// Whether every byte of `w` is whitespace `float.fromhex` strips.
#[inline]
fn space_word(w: u64) -> bool {
    (w & HIGH == 0) & (in_range(w, 9, 13) | in_range(w, 0x20, 0x20) == HIGH)
}

/// The word of the 8 bytes of `b` from `i`, if there are 8.
#[inline]
fn word_at(b: &[u8], i: usize) -> Option<u64> {
    let c = b.get(i..i + 8)?;
    Some(u64::from_le_bytes(c.try_into().unwrap_or([0; 8])))
}

/// The first index from `i` whose byte is not `ok`, read a word at a time
/// while `word` holds for all 8 bytes: `float.fromhex` reads 64 MiB in
/// about 30 ms, native release [M], where a byte at a time took 120.
#[inline(never)]
fn skip(b: &[u8], mut i: usize, word: impl Fn(u64) -> bool, ok: impl Fn(u8) -> bool) -> usize {
    let (words, _) = b.get(i..).unwrap_or(&[]).as_chunks::<8>();
    i += 8 * words
        .iter()
        .take_while(|&&c| word(u64::from_le_bytes(c)))
        .count();
    while b.get(i).is_some_and(|&x| ok(x)) {
        i += 1;
    }
    i
}

/// Whether any byte of `b` is not `0`.
fn any_nonzero(b: &[u8]) -> bool {
    let (words, rest) = b.as_chunks::<8>();
    let w = words
        .iter()
        .fold(0, |a, &c| a | (u64::from_le_bytes(c) ^ ZEROS));
    w != 0 || rest.iter().any(|&c| c != b'0')
}

/// The value of a `float.fromhex` text; `Err(true)` for overflow,
/// `Err(false)` for a malformed text. Its significand is kept as its first
/// [`HKEEP`] significant digits and whether any later one is nonzero: 80
/// bits and a sticky digit decide a 53-bit rounding.
fn hex_to_f64(text: &[u8]) -> Result<f64, bool> {
    let space = crate::complex::num_space;
    let start = skip(text, 0, space_word, space);
    let mut end = text.len();
    while end >= start + 8 && word_at(text, end - 8).is_some_and(space_word) {
        end -= 8;
    }
    while end > start && text.get(end - 1).is_some_and(|&c| space(c)) {
        end -= 1;
    }
    let t = text.get(start..end).unwrap_or(&[]);
    let (neg, t) = match t.first() {
        Some(b'-') => (true, &t[1..]),
        Some(b'+') => (false, &t[1..]),
        _ => (false, t),
    };
    let sign = |x: f64| if neg { -x } else { x };
    if t.eq_ignore_ascii_case(b"inf") || t.eq_ignore_ascii_case(b"infinity") {
        return Ok(sign(f64::INFINITY));
    }
    if t.eq_ignore_ascii_case(b"nan") {
        return Ok(sign(f64::NAN));
    }
    let mut i = 0;
    if t.len() > 1 && t[0] == b'0' && (t[1] | 0x20) == b'x' {
        i = 2;
    }
    // The coefficient: the integer digits `int`, then maybe a point and
    // the fraction digits `frac`.
    // Each run's leading zeros, then the rest of its digits.
    let zeros = |at| skip(t, at, |w| w == ZEROS, |c| c == b'0');
    let run = |at| {
        let z = zeros(at);
        (z, skip(t, z, hex_word, |c: u8| c.is_ascii_hexdigit()))
    };
    let (iz, end) = run(i);
    let int = i..end;
    i = end;
    let (mut fz, mut frac) = (i, i..i);
    if t.get(i) == Some(&b'.') {
        let (z, end) = run(i + 1);
        (fz, frac) = (z, i + 1..end);
        i = end;
    }
    if int.is_empty() && frac.is_empty() {
        return Err(false);
    }
    let mut exp: i64 = 0;
    if t.get(i).is_some_and(|&c| c | 0x20 == b'p') {
        i += 1;
        let eneg = t.get(i) == Some(&b'-');
        if matches!(t.get(i), Some(b'-' | b'+')) {
            i += 1;
        }
        let sig = zeros(i);
        let digits = skip(t, sig, dec_word, |c| c.is_ascii_digit());
        if digits == i {
            return Err(false);
        }
        // Past 2^40 every value is infinite or zero (strtol saturates).
        exp = 1 << 40;
        if digits - sig <= 13 {
            exp = t
                .get(sig..digits)
                .unwrap_or(&[])
                .iter()
                .fold(0, |e, &c| e * 10 + i64::from(c - b'0'))
                .min(1 << 40);
        }
        if eneg {
            exp = -exp;
        }
        i = digits;
    }
    // Anything after the number is a parse error, once it does not
    // overflow (CPython checks the end last).
    let junk = i != t.len();
    // The significant digits, from the first nonzero one.
    let mut first = iz.min(int.end);
    if first == int.end {
        first = fz.min(frac.end);
    }
    let runs = [
        first.max(int.start)..int.end,
        first.max(frac.start)..frac.end,
    ];
    let mut digits = [b'0'; HKEEP + 1];
    let (mut kept, mut dropped, mut sticky) = (0, 0usize, false);
    for r in runs {
        let part = t.get(r).unwrap_or(&[]);
        let k = part.len().min(HKEEP - kept);
        if let (Some(d), Some(p)) = (digits.get_mut(kept..kept + k), part.get(..k)) {
            d.copy_from_slice(p);
        }
        kept += k;
        let rest = part.get(k..).unwrap_or(&[]);
        dropped += rest.len();
        sticky |= any_nonzero(rest);
    }
    let mut e = exp - 4 * frac.len() as i64;
    if dropped > 0 {
        // A sticky digit below the kept ones.
        digits[kept] = if sticky { b'1' } else { b'0' };
        kept += 1;
        e += 4 * (dropped as i64 - 1);
    }
    let r = hex_value(digits.get(..kept.max(1)).unwrap_or(&[]), e, neg);
    if junk && r != Err(true) {
        return Err(false);
    }
    r
}

/// Significant hex digits [`hex_to_f64`] keeps.
const HKEEP: usize = 20;

/// The value of `float.fromhex`'s number: hex digits `b` (most significant
/// first) times 2 to the `exp`, rounded half to even.
#[inline(never)]
fn hex_value(b: &[u8], exp: i64, neg: bool) -> Result<f64, bool> {
    const MANT: i64 = 53;
    const MIN_EXP: i64 = -1021;
    const MAX_EXP: i64 = 1024;
    let sign = |x: f64| if neg { -x } else { x };
    let n = b.len();
    // `hd(j)`: the j-th digit from the least significant end.
    let hd = |j: usize| {
        b.get(n - 1 - j)
            .and_then(|&c| (c as char).to_digit(16))
            .unwrap_or(0)
    };
    let mut nd = n;
    while nd > 0 && hd(nd - 1) == 0 {
        nd -= 1;
    }
    if nd == 0 || exp < i64::MIN / 2 {
        return Ok(sign(0.0));
    }
    if exp > i64::MAX / 2 {
        return Err(true);
    }
    let mut top = exp + 4 * (nd as i64 - 1);
    let mut d = hd(nd - 1);
    while d != 0 {
        top += 1;
        d /= 2;
    }
    if top < MIN_EXP - MANT {
        return Ok(sign(0.0));
    }
    if top > MAX_EXP {
        return Err(true);
    }
    let lsb = top.max(MIN_EXP) - MANT;
    let mut x = 0.0;
    if exp >= lsb {
        for j in (0..nd).rev() {
            x = 16.0 * x + f64::from(hd(j));
        }
        return Ok(sign(ldexp(x, exp)));
    }
    let half = 1u32 << ((lsb - exp - 1) % 4);
    let key = usize::try_from((lsb - exp - 1) / 4).unwrap_or(0);
    for j in (key + 1..nd).rev() {
        x = 16.0 * x + f64::from(hd(j));
    }
    let d = if key < nd { hd(key) } else { 0 };
    x = 16.0 * x + f64::from(d & (16 - 2 * half));
    if d & half != 0 {
        let up = d & (3 * half - 1) != 0
            || (half == 8 && key + 1 < nd && hd(key + 1) & 1 != 0)
            || (0..key.min(nd)).any(|j| hd(j) != 0);
        if up {
            x += f64::from(2 * half);
            if top == MAX_EXP && x == ldexp(f64::from(2 * half), MANT) {
                return Err(true);
            }
        }
    }
    Ok(sign(ldexp(x, exp + 4 * key as i64)))
}
