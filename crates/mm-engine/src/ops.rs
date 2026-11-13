//! Generic operations on builtin values: arithmetic, comparison,
//! containment, subscripts, iteration, unpacking, attributes, truthiness.

use crate::bigint::Big;
use crate::class::Dn;
use crate::class::f;
use crate::scratch::{NativeValues, NativeVec};
use crate::types::{T, ty};
use crate::vm::{Raised, Vm, exc_slot, fn_slot};
use alloc::{format, string::String, vec, vec::Vec};
use core::cmp::Ordering;
use mm_compile::ast::BinOp;
use mm_core::{Ref, Value};

/// Iterator kinds (raw slot 1 of an `ITER`).
pub(crate) mod it {
    pub const LIST: u64 = 0;
    pub const TUPLE: u64 = 1;
    pub const STR: u64 = 2;
    pub const RANGE: u64 = 3;
    pub const DICT_KEYS: u64 = 4;
    pub const DICT_VALUES: u64 = 5;
    pub const DICT_ITEMS: u64 = 6;
    pub const SET: u64 = 7;
    pub const BYTES: u64 = 8;
    /// A `str` of ASCII chars only (`str_ascii_iterator`).
    pub const STR_ASCII: u64 = 9;
    pub const BYTEARRAY: u64 = 10;
}

/// Why an order could not be decided natively.
#[derive(Clone, Copy, Debug)]
pub(crate) enum OrdErr {
    /// The values' types do not order.
    Types(Value, Value),
    /// A user comparison method must run.
    Py,
    /// Nesting is too deep.
    Deep,
}

/// Position of an exhausted dict or set iterator (also after "keys changed
/// during iteration"): it stays exhausted, as CPython drops the container.
const DEAD: u64 = u64::MAX;

/// Items of an iterable that `collect` gathers in one native step before
/// the prelude goes on
/// (`Vm::partial`, `_collect_rest`, `_more`), so a fuel limit,
/// deadline or park request is seen at least once per chunk.
pub(crate) const COLLECT_CHUNK: usize = 1 << 16;

/// The Python type of an iterator of kind `k`.
pub(crate) fn iter_type(k: u64) -> T {
    match k {
        it::LIST => T::list_iterator,
        it::TUPLE => T::tuple_iterator,
        it::STR => T::str_iterator,
        it::STR_ASCII => T::str_ascii_iterator,
        it::RANGE => T::range_iterator,
        it::DICT_KEYS => T::dict_keyiterator,
        it::DICT_VALUES => T::dict_valueiterator,
        it::DICT_ITEMS => T::dict_itemiterator,
        it::BYTES => T::bytes_iterator,
        it::BYTEARRAY => T::bytearray_iterator,
        _ => T::set_iterator,
    }
}

/// The fractional part of `x` (`x - trunc(x)`); NaN for non-finite `x`.
pub(crate) fn fract(x: f64) -> f64 {
    if !x.is_finite() {
        return f64::NAN;
    }
    if x.abs() >= 4_503_599_627_370_496.0 {
        return 0.0;
    }
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        reason = "|x| < 2^52"
    )]
    let t = x as i64 as f64;
    x - t
}

/// `floor(x)` for finite `x`.
pub(crate) fn floor(x: f64) -> f64 {
    if !x.is_finite() || x.abs() >= 4_503_599_627_370_496.0 {
        return x;
    }
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        reason = "|x| < 2^52"
    )]
    let t = x as i64 as f64;
    if t > x { t - 1.0 } else { t }
}

/// Correctly rounded square root (bit-by-bit on the mantissa).
// Out of line: LLVM inlines it into each of its callers otherwise.
#[inline(never)]
pub(crate) fn sqrt(x: f64) -> f64 {
    if x.is_nan() || x < 0.0 {
        return f64::NAN;
    }
    if x == 0.0 || x.is_infinite() {
        return x;
    }
    // Correctly rounded: x = m * 2^e with m an integer, e even; the integer
    // square root of m << 60 has 57+ bits, and a sticky bit for an inexact
    // root makes the u128 -> f64 conversion round exactly as IEEE sqrt.
    let bits = x.to_bits();
    let biased = i32::try_from((bits >> 52) & 0x7ff).unwrap_or(0);
    let frac = bits & ((1u64 << 52) - 1);
    let (mant, exp) = if biased == 0 {
        let shift = frac.leading_zeros() - 11;
        (frac << shift, 1 - i32::try_from(shift).unwrap_or(0))
    } else {
        (frac | (1u64 << 52), biased)
    };
    let mut e = exp - 1075;
    let mut m = u128::from(mant);
    if e & 1 != 0 {
        m <<= 1;
        e -= 1;
    }
    m <<= 60;
    e -= 60;
    let (root, rem) = isqrt(m);
    let r = (root << 1) | u128::from(rem != 0);
    #[allow(clippy::cast_precision_loss, reason = "rounds to nearest, as intended")]
    let rf = r as f64;
    // root * 2^(e/2) = (r / 2) * 2^(e/2); |e/2 - 1| < 1022, so the scale is a
    // normal power of two and the product exact.
    let k = e / 2 - 1;
    rf * f64::from_bits(u64::try_from(k + 1023).unwrap_or(1023) << 52)
}

/// Floor square root of `n` and the remainder.
fn isqrt(n: u128) -> (u128, u128) {
    let mut op = n;
    let mut res = 0u128;
    let mut one = 1u128 << 126;
    while one > op {
        one >>= 2;
    }
    while one != 0 {
        if op >= res + one {
            op -= res + one;
            res = (res >> 1) + one;
        } else {
            res >>= 1;
        }
        one >>= 2;
    }
    (res, op)
}

/// Sorts `v` so that no element is `less` than the one before it
/// (heapsort: `O(n log n)`, and far smaller than the standard library's
/// sorts, which matters for the size budget). Not stable.
pub(crate) fn heap_sort_by<X>(v: &mut [X], less: impl Fn(&X, &X) -> bool) {
    let sift = |v: &mut [X], mut root: usize, end: usize| loop {
        let mut child = 2 * root + 1;
        if child >= end {
            break;
        }
        if child + 1 < end && less(&v[child], &v[child + 1]) {
            child += 1;
        }
        if !less(&v[root], &v[child]) {
            break;
        }
        v.swap(root, child);
        root = child;
    };
    let n = v.len();
    for i in (0..n / 2).rev() {
        sift(v, i, n);
    }
    for end in (1..n).rev() {
        v.swap(0, end);
        sift(v, 0, end);
    }
}

/// `x ** y` for floats: exact squaring for integral exponents, `sqrt` for
/// halves, and `exp(y ln x)` otherwise.
pub(crate) fn powf(x: f64, y: f64) -> f64 {
    if y == 0.5 && x >= 0.0 && x.is_finite() {
        // `+ 0.0`: C's `pow(-0.0, 0.5)` is `+0.0`, `sqrt(-0.0)` is not.
        return sqrt(x) + 0.0;
    }
    crate::libm::pow(x, y)
}

/// C99 `pow` for a zero or non-finite operand (both not NaN, `y != 0`,
/// `x != 1`).
pub(crate) fn powf_special(x: f64, y: f64) -> f64 {
    let yint = crate::libm::rint(y) == y;
    let odd = yint && y.abs() < 9.007_199_254_740_992e15 && float_divmod(y, 2.0).1 != 0.0;
    if y.is_infinite() {
        let ax = x.abs();
        return if ax == 1.0 {
            1.0
        } else if (ax < 1.0) == (y > 0.0) {
            0.0
        } else {
            f64::INFINITY
        };
    }
    if x == 0.0 {
        return if y > 0.0 {
            if odd { x } else { 0.0 }
        } else if odd {
            crate::libm::copysign(f64::INFINITY, x)
        } else {
            f64::INFINITY
        };
    }
    // x is infinite.
    let neg = x < 0.0 && odd;
    let r = if y > 0.0 { f64::INFINITY } else { 0.0 };
    if neg { -r } else { r }
}

impl Vm {
    // ------------------------------------------------------ truthiness

    /// `bool(v)`.
    #[inline]
    pub(crate) fn truthy(&self, v: Value) -> bool {
        if v == Value::TRUE {
            return true;
        }
        if v == Value::FALSE || v == Value::NONE {
            return false;
        }
        if let Some(i) = v.as_int() {
            return i != 0;
        }
        self.truthy_slow(v)
    }

    fn truthy_slow(&self, v: Value) -> bool {
        if let Some(f) = v.as_float() {
            return f != 0.0;
        }
        match self.type_of(v) {
            T::str => self.str_len(v) > 0,
            T::bytes | T::bytearray => !self.bytes_data(v).is_empty(),
            T::list => self.list_len(v) > 0,
            T::tuple => self.tuple_len(v) > 0,
            T::dict => self.dict_len(v) > 0,
            T::set | T::frozenset => self.set_len(v) > 0,
            T::int => Vm::is_heap_int(v) || self.int_of(v).is_some_and(|i| i != 0),
            T::range => self.range_len(v) > 0,
            T::complex => self
                .complex_of(v)
                .is_some_and(|(re, im)| re != 0.0 || im != 0.0),
            T::dict_keys | T::dict_values | T::dict_items => self.dict_len(self.vslot(v, 0)) > 0,
            _ => self.backing(v).is_none_or(|b| self.truthy(b)),
        }
    }

    /// A bulk source of the items of `v` and its flag: a keys view is
    /// its dict, read a key at a time as an iterator is (no presize), an
    /// items view its dict read as `(key, value)` pairs, a set subclass's
    /// instance its set.
    pub(crate) fn view_src(&mut self, v: Value) -> Result<(Value, u64), Raised> {
        match self.type_of(v) {
            T::dict_keys => Ok((self.vslot(v, 0), crate::set::act::VIEW)),
            T::dict_items => Ok((self.vslot(v, 0), crate::set::act::ITEMS)),
            // A set subclass's instance is read as its set
            // (`PyAnySet_Check`), whatever its `__iter__`.
            _ if self.backing(v).is_some_and(Vm::is_set) => Ok((self.unbx(v), 0)),
            _ => Ok((self.bulk_src(v)?, 0)),
        }
    }

    /// `len(v)`. A `u64`: a range can be longer than a wasm32 `usize`
    /// (`sys.maxsize` is 64-bit on every target).
    pub(crate) fn len_of(&mut self, v: Value) -> Result<u64, Raised> {
        let n = match self.type_of(v) {
            T::str => self.str_len(v),
            T::bytes | T::bytearray => self.bytes_data(v).len(),
            T::list => self.list_len(v),
            T::tuple => self.tuple_len(v),
            T::dict => self.dict_len(v),
            T::set | T::frozenset => self.set_len(v),
            T::range => {
                let n = self.range_len(v);
                if i64::try_from(n).is_err() {
                    return Err(self.err(
                        T::OverflowError,
                        "Python int too large to convert to C ssize_t",
                    ));
                }
                return Ok(n);
            }
            T::dict_keys | T::dict_values | T::dict_items => self.dict_len(self.vslot(v, 0)),
            _ => {
                if self.uflags(v) & f::LEN != 0 {
                    return Err(self.need_py());
                }
                if let Some(b) = self.backing(v) {
                    return self.len_of(b);
                }
                let msg = format!("object of type '{}' has no len()", self.type_name(v));
                return Err(self.err(T::TypeError, &msg));
            }
        };
        Ok(n as u64)
    }

    // ------------------------------------------------------ arithmetic

    /// `a + b` for two strings; a result of `long` bytes or more goes a
    /// slice per step, its value then a request (for the dispatch loop and
    /// `binop_top`, `strmeth::FILL_BYTES`; `usize::MAX` for all at once).
    #[inline(never)]
    pub(crate) fn str_concat(&mut self, a: Value, b: Value, long: usize) -> Result<Value, Raised> {
        let (Some(ra), Some(rb)) = (a.as_obj(), b.as_obj()) else {
            return Ok(a);
        };
        let res = self.heap.str_concat_n(ra, rb, long);
        let (r, n) = self.hx(res)?;
        if n >= 64 {
            if n == usize::MAX {
                return crate::strmeth::concat_steps(self, a, b);
            }
            self.burn_result(n, true);
        }
        Ok(Value::from_ref(r, ty::STR as u8))
    }

    /// `a <op> b`, or the in-place form (lists extend in place), all at
    /// once: for callers that cannot hand the rest to steps.
    pub(crate) fn binop(
        &mut self,
        op: BinOp,
        a: Value,
        b: Value,
        inplace: bool,
    ) -> Result<Value, Raised> {
        self.binop_in(op, a, b, inplace, false)
    }

    /// `a <op> b` as an operator instruction runs it: long container work
    /// goes on in steps, its value a request (docs/spec.md §5).
    pub(crate) fn binop_top(
        &mut self,
        op: BinOp,
        a: Value,
        b: Value,
        inplace: bool,
    ) -> Result<Value, Raised> {
        self.binop_in(op, a, b, inplace, true)
    }

    fn binop_in(
        &mut self,
        op: BinOp,
        a: Value,
        b: Value,
        inplace: bool,
        top: bool,
    ) -> Result<Value, Raised> {
        if (self.uflags(a) | self.uflags(b)) & f::ARITH != 0 {
            return Err(self.need_py());
        }
        if (a.is_obj_kind(ty::INST as u8) || b.is_obj_kind(ty::INST as u8))
            && (self.backing(a).is_some() || self.backing(b).is_some())
        {
            // A list's `+=` iterates its operand as `extend` does: only an
            // exact list or tuple, a dict's keys, and the list itself
            // (`a += a`) are read in place, so a subclass's own `__iter__`
            // runs, in the prelude twin.
            if op == BinOp::Add
                && inplace
                && a != b
                && b.is_obj_kind(ty::INST as u8)
                && self.unbx(a).is_obj_kind(ty::LIST as u8)
                && self.dunder(b, Dn::Iter).is_some()
            {
                return Err(self.need_py());
            }
            return self.binop_bx(op, a, b, inplace);
        }
        if let (Some(x), Some(y)) = (self.int_of(a), self.int_of(b)) {
            if a.as_special().is_some() && b.as_special().is_some() {
                // bool op bool stays bool for & | ^.
                match op {
                    BinOp::BitAnd => return Ok(Value::from_bool(x & y != 0)),
                    BinOp::BitOr => return Ok(Value::from_bool((x | y) != 0)),
                    BinOp::BitXor => return Ok(Value::from_bool((x ^ y) != 0)),
                    _ => {}
                }
            }
            return self.int_op(op, x, y);
        }
        if Vm::is_intlike(a) && Vm::is_intlike(b) {
            if let (Some(x), Some(y)) = (self.int128_of(a), self.int128_of(b))
                && let Some(r) = Vm::int128_binop(op, x, y)
            {
                return self.new_int128(r);
            }
            let (x, y) = (
                self.big_of(a).unwrap_or_default(),
                self.big_of(b).unwrap_or_default(),
            );
            return self.big_binop(op, &x, &y);
        }
        if (Vm::is_heap_int(a) && b.is_float()) || (a.is_float() && Vm::is_heap_int(b)) {
            let arith = matches!(
                op,
                BinOp::Add
                    | BinOp::Sub
                    | BinOp::Mul
                    | BinOp::Div
                    | BinOp::FloorDiv
                    | BinOp::Mod
                    | BinOp::Pow
            );
            let (mut x, mut y) = (0.0, 0.0);
            if arith {
                x = self.num_to_f64(a)?.unwrap_or(0.0);
                y = self.num_to_f64(b)?.unwrap_or(0.0);
            }
            return self.float_op(op, x, y, a, b);
        }
        if let (Some(x), Some(y)) = (self.num_f64(a), self.num_f64(b)) {
            return self.float_op(op, x, y, a, b);
        }
        if (Vm::is_complex(a) || Vm::is_complex(b))
            && (Vm::is_complex(a) || Vm::is_number(a))
            && (Vm::is_complex(b) || Vm::is_number(b))
            && let Some(r) = self.complex_binop(op, a, b)?
        {
            return Ok(r);
        }
        let (ta, tb) = (self.type_of(a), self.type_of(b));
        match (op, ta, tb) {
            (BinOp::Add, T::str, T::str) => {
                let long = if top {
                    crate::strmeth::FILL_BYTES
                } else {
                    usize::MAX
                };
                return self.str_concat(a, b, long);
            }
            (BinOp::Add, T::bytearray, T::bytes | T::bytearray) if inplace => {
                return self.bytearray_iadd(a, b);
            }
            (BinOp::Add, T::bytes | T::bytearray, T::bytes | T::bytearray) => {
                return self.bytes_concat(a, b);
            }
            (BinOp::Add, T::bytes | T::bytearray, _) if self.user_class(b).is_none() => {
                let msg = format!("can't concat {} to {}", self.type_name(b), ta.name());
                return Err(self.err(T::TypeError, &msg));
            }
            (BinOp::Add, T::list, T::list | T::tuple)
                if top
                    && self.list_len(a) + self.seq_words(b).len() >= SEQ_LONG
                    && (inplace || tb == T::list) =>
            {
                let (la, lb) = (self.list_len(a), self.seq_words(b).len());
                let segs = [Seg::of(3, 0, 1, la as u64), Seg::of(4, 0, 1, lb as u64)];
                if inplace {
                    return self.list_rebuild(a, a, &[a, b], &segs, la + lb);
                }
                return self.seq_new(false, &[a, b], &segs, la + lb);
            }
            (BinOp::Add, T::tuple, T::tuple)
                if top && self.tuple_len(a) + self.tuple_len(b) >= SEQ_LONG =>
            {
                let (la, lb) = (self.tuple_len(a), self.tuple_len(b));
                let segs = [Seg::of(3, 0, 1, la as u64), Seg::of(4, 0, 1, lb as u64)];
                return self.seq_new(true, &[a, b], &segs, la + lb);
            }
            (BinOp::Add, T::list, T::list) => {
                if inplace {
                    // Grown once, to the final length, before any item is
                    // written: a `MemoryError` leaves `a` as it was, as
                    // CPython's does.
                    let items = self.list_items(b);
                    self.list_extend_values(a, &items)?;
                    return Ok(a);
                }
                let mut items = self.list_items(a);
                items.extend(self.list_items(b));
                return self.new_list(&items);
            }
            (BinOp::Add, T::list, _) if inplace => {
                self.list_extend_from(a, b)?;
                return Ok(a);
            }
            (BinOp::Add, T::tuple, T::tuple) => {
                let mut items = self.tuple_items(a);
                items.extend(self.tuple_items(b));
                return self.alloc_tuple(&items);
            }
            (BinOp::Add, T::str | T::list | T::tuple, _) => {
                let what = ta.name();
                let msg = format!(
                    "can only concatenate {what} (not \"{}\") to {what}",
                    self.type_name(b)
                );
                return Err(self.err(T::TypeError, &msg));
            }
            (BinOp::Mul, T::str | T::list | T::tuple | T::bytes | T::bytearray, _)
            | (BinOp::Mul, _, T::str | T::list | T::tuple | T::bytes | T::bytearray) => {
                let (seq, n) =
                    if matches!(ta, T::str | T::list | T::tuple | T::bytes | T::bytearray) {
                        (a, b)
                    } else {
                        (b, a)
                    };
                let Some(k) = self.int_of(n) else {
                    if Vm::is_heap_int(n) {
                        return Err(self.index_overflow(T::OverflowError));
                    }
                    if self.uflags(n) & f::INDEX != 0 {
                        return Err(self.need_py());
                    }
                    let msg = format!(
                        "can't multiply sequence by non-int of type '{}'",
                        self.type_name(n)
                    );
                    return Err(self.err(T::TypeError, &msg));
                };
                // `n *= seq` repeats `seq` into a new object: only the left
                // operand's repetition is in place.
                return self.repeat(seq, k, inplace && seq == a, top);
            }
            (BinOp::Mod, T::str | T::bytes, _) => {
                return match self.percent(a, b, Value::NONE)? {
                    crate::percent::Pct::Done(v) => Ok(v),
                    crate::percent::Pct::Need(..) => Err(self.need_py()),
                };
            }
            (BinOp::BitOr | BinOp::BitAnd | BinOp::Sub | BinOp::BitXor, _, _)
                if (matches!(ta, T::dict_keys | T::dict_items)
                    || matches!(tb, T::dict_keys | T::dict_items))
                    && !(inplace && ta == T::dict) =>
            {
                return self.view_setop(op, a, b, top, true);
            }
            (
                BinOp::BitOr | BinOp::BitAnd | BinOp::Sub | BinOp::BitXor,
                T::set | T::frozenset,
                T::set | T::frozenset,
            ) => {
                return self.set_binop(op, a, b, inplace, top);
            }
            (BinOp::BitOr, T::dict, T::dict) => {
                use crate::set::{act, phase};
                if inplace {
                    return self.bulk(&[a, b], phase(act::DICT, 0, 1, 0), 0, top);
                }
                let out = self.new_dict()?;
                let prog = phase(act::DICT, 0, 1, 0) | phase(act::DICT, 0, 2, 0) << 16;
                return self.bulk(&[out, a, b], prog, 0, top);
            }
            // `|=` takes whatever `update()` takes.
            (BinOp::BitOr, T::dict, _) if inplace => {
                let (src, prog) = crate::builtins::dict_fill_phase(self, b, 1)?;
                return self.bulk(&[a, src], prog, 0, top);
            }
            // `int | None`: a `typing.Union`, built in Python.
            (BinOp::BitOr, T::type_ | T::NoneType, T::type_ | T::NoneType)
                if ta == T::type_ || tb == T::type_ =>
            {
                return Err(self.need_py());
            }
            _ => {}
        }
        let sym = match (inplace, op) {
            (false, BinOp::Pow) => String::from("** or pow()"),
            (true, _) => format!("{}=", op.symbol()),
            _ => op.symbol().into(),
        };
        let msg = format!(
            "unsupported operand type(s) for {sym}: '{}' and '{}'",
            String::from(self.type_name(a)),
            self.type_name(b)
        );
        Err(self.err(T::TypeError, &msg))
    }

    /// `a <op> b` (`&`, `|`, `-`, `^`) with a set-like dict view: a plain
    /// set, as CPython's `dictviews_*` make it. `exact`: a `set` operand is
    /// one (`PySet_CheckExact`), not a subclass's value.
    fn view_setop(
        &mut self,
        op: BinOp,
        a: Value,
        b: Value,
        top: bool,
        exact: bool,
    ) -> Result<Value, Raised> {
        use crate::set::{act, phase};
        let view = |t: T| matches!(t, T::dict_keys | T::dict_items);
        let (ta, tb) = (self.type_of(a), self.type_of(b));
        if op == BinOp::BitXor && ta == T::dict_items && tb == T::dict_items {
            // `dictitems_xor` compares values: the prelude twin.
            return Err(self.need_py());
        }
        let out = self.new_set()?;
        if op == BinOp::BitAnd {
            // `_PyDictView_Intersect`: an exact set at least as long as
            // the view reads the view (`set.intersection`); of two views
            // the longer is probed; otherwise the other operand is read.
            let (v, o) = if view(ta) { (a, b) } else { (b, a) };
            let lv = self.dict_len(self.vslot(v, 0));
            let (src, mut probe) = if (exact && self.type_of(o) == T::set && lv <= self.set_len(o))
                || (view(self.type_of(o)) && self.dict_len(self.vslot(o, 0)) > lv)
            {
                (v, o)
            } else {
                (o, v)
            };
            // Each item of `src` kept if `probe` has it.
            if self.type_of(probe) == T::dict_items {
                for &x in &self.collect(src)? {
                    if self.contains(probe, x)? {
                        self.set_add_plain(out, x)?;
                    }
                }
                return Ok(out);
            }
            if self.type_of(probe) == T::dict_keys {
                probe = self.vslot(probe, 0);
            }
            let (src, fs) = self.view_src(src)?;
            return self.bulk(&[out, src, probe], phase(act::AND | fs, 0, 1, 2), 0, top);
        }
        // `dictviews_to_set(a)`, then `b` taken in, removed or toggled: a
        // set or a dict toggles as it is, anything else as the set of its
        // items. Only a keys view's dict presizes the set, as a dict does.
        let (x, fx) = self.view_src(a)?;
        let fx = if ta == T::dict_keys { 0 } else { fx };
        let fill = phase(act::ADD | fx, 0, 1, 0);
        if !matches!(
            b.obj_kind().map(u16::from),
            Some(ty::LIST | ty::TUPLE | ty::SET | ty::DICT | ty::RANGE)
        ) && !view(tb)
        {
            // `set(a)` is made before `b` is read, as CPython's
            // `dictviews_or` does (its errors come first).
            self.bulk(&[out, x], fill, 0, false)?;
        }
        let (y, fy) = self.view_src(b)?;
        let regs = [out, x, y, Value::NONE];
        let prog = match op {
            BinOp::BitOr => fill | phase(act::ADD | fy, 0, 2, 0) << 16,
            BinOp::Sub => fill | phase(act::DISCARD | fy, 0, 2, 0) << 16,
            _ if Vm::is_set(y) || (y.is_obj_kind(ty::DICT as u8) && fy == 0) => {
                fill | phase(act::TOGGLE, 0, 2, 0) << 16
            }
            _ => {
                let tmp = self.new_set()?;
                let regs = [out, x, y, tmp];
                let prog =
                    phase(act::ADD | fy, 3, 2, 0) | fill << 16 | phase(act::TOGGLE, 0, 3, 0) << 32;
                return self.bulk(&regs, prog, 0, top);
            }
        };
        self.bulk(&regs, prog, 0, top)
    }

    /// `binop` with an instance operand: on the builtin values instances
    /// of builtin-type subclasses extend (`+=` on a list subclass keeps
    /// the instance).
    #[cold]
    pub(crate) fn binop_bx(
        &mut self,
        op: BinOp,
        a: Value,
        b: Value,
        inplace: bool,
    ) -> Result<Value, Raised> {
        let (x, y) = (self.unbx(a), self.unbx(b));
        // `Sub(3) *= [1, 2]`, `N() *= T((1,))`: CPython's in-place repeat
        // reads the left operand's sequence slots, which a class defined in
        // Python has (empty), so the right one repeats only through its
        // class's own `__mul__` or `__rmul__`.
        let seq = |t: T| matches!(t, T::list | T::tuple | T::str | T::bytes | T::bytearray);
        if inplace
            && op == BinOp::Mul
            && a.is_obj_kind(ty::INST as u8)
            && !seq(self.type_of(x))
            && seq(self.type_of(y))
            && self.dunder(b, Dn::Mul).is_none()
            && self.dunder(b, Dn::Rmul).is_none()
        {
            let msg = format!(
                "unsupported operand type(s) for *=: '{}' and '{}'",
                String::from(self.type_name(a)),
                self.type_name(b)
            );
            return Err(self.err(T::TypeError, &msg));
        }
        if op == BinOp::Mod && matches!(self.type_of(x), T::str | T::bytes) {
            // `%` formats its right operand as given, as CPython's
            // `PyUnicode_Format` does: a str subclass's `__str__`, a
            // mapping's `__getitem__`.
            return match self.percent(x, b, Value::NONE)? {
                crate::percent::Pct::Done(v) => Ok(v),
                crate::percent::Pct::Need(..) => Err(self.need_py()),
            };
        }
        if op == BinOp::BitAnd
            && (matches!(self.type_of(x), T::dict_keys | T::dict_items)
                || matches!(self.type_of(y), T::dict_keys | T::dict_items))
        {
            // A set subclass's value is not read as an exact set.
            return self.view_setop(op, x, y, false, false);
        }
        let r = match self.binop(op, x, y, inplace) {
            Ok(r) => r,
            Err(Raised) if !self.slow && self.exc_type(self.exc) == T::TypeError => {
                // Unsupported for the values: report the subclasses.
                let msg = self.exc_str(self.exc).unwrap_or_default();
                if msg.starts_with("unsupported operand") {
                    let sym = match (inplace, op) {
                        (false, BinOp::Pow) => String::from("** or pow()"),
                        (true, _) => format!("{}=", op.symbol()),
                        _ => op.symbol().into(),
                    };
                    let msg = format!(
                        "unsupported operand type(s) for {sym}: '{}' and '{}'",
                        String::from(self.type_name(a)),
                        self.type_name(b)
                    );
                    return Err(self.err(T::TypeError, &msg));
                }
                if msg.starts_with("can't multiply sequence") {
                    let seq =
                        |t: T| matches!(t, T::str | T::list | T::tuple | T::bytes | T::bytearray);
                    let n = if seq(self.type_of(x)) { b } else { a };
                    let msg = format!(
                        "can't multiply sequence by non-int of type '{}'",
                        self.type_name(n)
                    );
                    return Err(self.err(T::TypeError, &msg));
                }
                return Err(Raised);
            }
            Err(e) => return Err(e),
        };
        // An in-place operator that changed the backing object returns the
        // instance; an immutable one's result (`S('ab') * True`) is a new
        // value of the builtin type, as in CPython.
        let mutable = x.is_obj_kind(ty::LIST as u8)
            || x.is_obj_kind(ty::BYTEARRAY as u8)
            || x.is_obj_kind(ty::DICT as u8)
            || x.is_obj_kind(ty::SET as u8);
        Ok(if inplace && r == x && x != a && mutable {
            a
        } else {
            r
        })
    }

    fn repeat(&mut self, seq: Value, k: i64, inplace: bool, top: bool) -> Result<Value, Raised> {
        let t = self.type_of(seq);
        // CPython's `unicode_repeat` and `bytes_repeat`: a result longer
        // than `sys.maxsize` code points or bytes is an OverflowError,
        // before any allocation. 64-bit on every target, as `sys.maxsize`.
        let n = k.max(0).unsigned_abs();
        let len = match t {
            T::str => self.str_len(seq),
            T::bytes => self.bytes_data(seq).len(),
            _ => 0,
        };
        if (len as u64).saturating_mul(n) > i64::MAX.unsigned_abs() {
            let msg = if t == T::str {
                "repeated string is too long"
            } else {
                "repeated bytes are too long"
            };
            return Err(self.err(T::OverflowError, msg));
        }
        // Saturating: on a 32-bit target a count past `usize::MAX` must
        // still be a MemoryError, not an empty result.
        let k = usize::try_from(n).unwrap_or(usize::MAX);
        // CPython returns an exact tuple or bytes itself once.
        if k == 1 && matches!(t, T::tuple | T::bytes) {
            return Ok(seq);
        }
        if t == T::bytes {
            return self.bytes_repeat(seq, k);
        }
        if t == T::bytearray {
            return if inplace {
                self.bytearray_imul(seq, k)
            } else {
                self.bytes_repeat(seq, k)
            };
        }
        let unit = match t {
            T::str => self.str_bytes(seq).len(),
            T::list => self.list_len(seq) * 8,
            _ => self.tuple_len(seq) * 8,
        };
        let total = unit.saturating_mul(k);
        if total > (1 << 30) || !self.heap.would_fit(total as u64) {
            return Err(self.err(T::MemoryError, ""));
        }
        // An empty result needs no copying (`[] * 2**62` must not loop).
        let k = if total == 0 { 0 } else { k };
        let n = total / 8;
        if top && t != T::str && n >= SEQ_LONG {
            let len = self.seq_words(seq).len() as u64;
            let segs = [Seg {
                reps: k as u64,
                ..Seg::of(3, 0, 1, len)
            }];
            if inplace {
                return self.list_rebuild(seq, seq, &[seq], &segs, n);
            }
            return self.seq_new(t == T::tuple, &[seq], &segs, n);
        }
        // The copy is charged by the result's constructor (`new_str`,
        // `new_list`, `alloc_tuple`, `list_set_items`).
        match t {
            T::str => crate::strmeth::repeat(self, seq, k, total, top),
            T::list => {
                let items = self.list_items(seq);
                let mut out = Vec::with_capacity(items.len() * k);
                for _ in 0..k {
                    out.extend_from_slice(&items);
                }
                if inplace {
                    self.list_set_items(seq, &out)?;
                    Ok(seq)
                } else {
                    self.new_list(&out)
                }
            }
            _ => {
                let items = self.tuple_items(seq);
                let mut out = Vec::with_capacity(items.len() * k);
                for _ in 0..k {
                    out.extend_from_slice(&items);
                }
                self.alloc_tuple(&out)
            }
        }
    }

    /// Integer arithmetic (bools included).
    pub(crate) fn int_op(&mut self, op: BinOp, x: i64, y: i64) -> Result<Value, Raised> {
        let (x128, y128) = (i128::from(x), i128::from(y));
        match op {
            BinOp::Add => self.int_from_i128(x128 + y128),
            BinOp::Sub => self.int_from_i128(x128 - y128),
            BinOp::Mul => self.int_from_i128(x128 * y128),
            BinOp::Div => {
                if y == 0 {
                    return Err(self.err(T::ZeroDivisionError, "division by zero"));
                }
                // Past 2**53 an operand would round on conversion; divide
                // exactly and round once, as CPython does.
                if x.unsigned_abs() > 1 << 53 || y.unsigned_abs() > 1 << 53 {
                    let f = self.big_truediv(&Big::from_i64(x), &Big::from_i64(y))?;
                    return Ok(Value::float(f));
                }
                #[allow(clippy::cast_precision_loss, reason = "true division")]
                Ok(Value::float(x as f64 / y as f64))
            }
            BinOp::FloorDiv | BinOp::Mod => {
                if y == 0 {
                    return Err(self.err(T::ZeroDivisionError, "division by zero"));
                }
                let q = x128.div_euclid(y128);
                let r = x128.rem_euclid(y128);
                // Python: the remainder takes the divisor's sign.
                let (q, r) = if r != 0 && y128 < 0 {
                    (q - 1, r + y128)
                } else {
                    (q, r)
                };
                self.int_from_i128(if op == BinOp::FloorDiv { q } else { r })
            }
            BinOp::Pow => {
                if y < 0 {
                    if x == 0 {
                        return Err(self.err(T::ZeroDivisionError, "zero to a negative power"));
                    }
                    #[allow(clippy::cast_precision_loss, reason = "float power")]
                    return Ok(Value::float(powf(x as f64, y as f64)));
                }
                let mut acc: i64 = 1;
                let mut base = x;
                let mut e = y;
                loop {
                    if e & 1 == 1 {
                        match acc.checked_mul(base) {
                            Some(v) => acc = v,
                            None => break,
                        }
                    }
                    e >>= 1;
                    if e == 0 {
                        return self.new_int(acc);
                    }
                    match base.checked_mul(base) {
                        Some(v) => base = v,
                        None => break,
                    }
                }
                self.big_binop(op, &Big::from_i64(x), &Big::from_i64(y))
            }
            BinOp::LShift => {
                if y < 0 {
                    return Err(self.err(T::ValueError, "negative shift count"));
                }
                if x == 0 {
                    return Ok(Value::int(0).unwrap_or(Value::NONE));
                }
                if y < 64 {
                    let v = x128 << y;
                    if v >> y == x128 {
                        return self.int_from_i128(v);
                    }
                }
                self.big_binop(op, &Big::from_i64(x), &Big::from_i64(y))
            }
            BinOp::RShift => {
                if y < 0 {
                    return Err(self.err(T::ValueError, "negative shift count"));
                }
                self.new_int(if y >= 64 {
                    if x < 0 { -1 } else { 0 }
                } else {
                    x >> y
                })
            }
            BinOp::BitAnd => self.new_int(x & y),
            BinOp::BitOr => self.new_int(x | y),
            BinOp::BitXor => self.new_int(x ^ y),
            BinOp::MatMul => Err(self.err(
                T::TypeError,
                "unsupported operand type(s) for @: 'int' and 'int'",
            )),
        }
    }

    fn float_op(&mut self, op: BinOp, x: f64, y: f64, a: Value, b: Value) -> Result<Value, Raised> {
        let r = match op {
            BinOp::Add => x + y,
            BinOp::Sub => x - y,
            BinOp::Mul => x * y,
            BinOp::Div => {
                if y == 0.0 {
                    return Err(self.err(T::ZeroDivisionError, "division by zero"));
                }
                x / y
            }
            BinOp::FloorDiv | BinOp::Mod => {
                if y == 0.0 {
                    return Err(self.err(T::ZeroDivisionError, "division by zero"));
                }
                let (div, m) = float_divmod(x, y);
                if op == BinOp::FloorDiv { div } else { m }
            }
            BinOp::Pow => {
                if x == 0.0 && y < 0.0 && y.is_finite() {
                    return Err(self.err(T::ZeroDivisionError, "zero to a negative power"));
                }
                if x < 0.0 && x.is_finite() && y.is_finite() && fract(y) != 0.0 {
                    return self.real_pow_complex(x, y);
                }
                let r = powf(x, y);
                if r.is_infinite() && x.is_finite() && y.is_finite() {
                    // CPython raises `OverflowError(ERANGE, strerror)`.
                    let errno = self.new_int(34)?;
                    let text = self.new_str("Numerical result out of range")?;
                    self.exc = self.new_exception(T::OverflowError, &[errno, text])?;
                    return Err(Raised);
                }
                r
            }
            _ => {
                return Err(self.err(
                    T::TypeError,
                    &format!(
                        "unsupported operand type(s) for {}: '{}' and '{}'",
                        op.symbol(),
                        self.type_name(a),
                        self.type_name(b)
                    ),
                ));
            }
        };
        Ok(Value::float(r))
    }

    /// Unary `-`, `+`, `~`.
    pub(crate) fn unary(&mut self, op: u8, v: Value) -> Result<Value, Raised> {
        use mm_compile::bytecode::op as o;
        if let Some(i) = self.int_of(v) {
            return match op {
                o::NEG => self.int_from_i128(-i128::from(i)),
                o::POS => self.new_int(i),
                _ => self.new_int(!i),
            };
        }
        if op == o::POS && Vm::is_heap_int(v) {
            // `+x` is `x` for an exact int.
            return Ok(v);
        }
        if let Some(b) = self.big_of(v) {
            // Byte work on a long int (C4; only `op_unary` calls this).
            if self.stop_before() {
                return Err(Raised);
            }
            let r = match op {
                o::NEG => b.neg(),
                o::POS => b,
                _ => b.not(),
            };
            return self.new_big(&r);
        }
        if let Some(f) = v.as_float() {
            return match op {
                o::NEG => Ok(Value::float(-f)),
                o::POS => Ok(v),
                _ => Err(self.err(T::TypeError, "bad operand type for unary ~: 'float'")),
            };
        }
        if let Some((re, im)) = self.complex_of(v)
            && op != o::INVERT
        {
            return if op == o::NEG {
                self.new_complex(-re, -im)
            } else {
                self.new_complex(re, im)
            };
        }
        if self.uflags(v) & f::UNARY != 0 {
            return Err(self.need_py());
        }
        if let Some(b) = self.backing(v)
            && (Vm::is_number(b) || Vm::is_complex(b))
        {
            return self.unary(op, b);
        }
        let sym = match op {
            o::NEG => "-",
            o::POS => "+",
            _ => "~",
        };
        let msg = format!("bad operand type for unary {sym}: '{}'", self.type_name(v));
        Err(self.err(T::TypeError, &msg))
    }

    // ------------------------------------------------------ comparison

    /// Three-way order for `<`-family operators, `None` if unordered (NaN).
    pub(crate) fn order(
        &mut self,
        a: Value,
        b: Value,
        sym: &str,
    ) -> Result<Option<Ordering>, Raised> {
        let mut left = crate::dict::WALK;
        let r = self.order_pure(a, b, 0, &mut left);
        self.walked(left);
        r.map_err(|e| self.order_error(e, sym))
    }

    /// The exception for an undecided order.
    pub(crate) fn order_error(&mut self, e: OrdErr, sym: &str) -> Raised {
        match e {
            OrdErr::Py => self.need_py(),
            OrdErr::Deep => self.err(
                T::RecursionError,
                "maximum recursion depth exceeded in comparison",
            ),
            OrdErr::Types(x, y) => {
                let msg = format!(
                    "'{sym}' not supported between instances of '{}' and '{}'",
                    String::from(self.type_name(x)),
                    self.type_name(y)
                );
                self.err(T::TypeError, &msg)
            }
        }
    }

    /// A comparison opcode applied to `a`, `b`.
    pub(crate) fn compare(&mut self, opcode: u8, a: Value, b: Value) -> Result<Value, Raised> {
        use mm_compile::bytecode::op as o;
        let r = match opcode {
            o::TEST_EQ => self.eq_values(a, b)?,
            o::TEST_NE => !self.eq_values(a, b)?,
            o::TEST_IS => a == b,
            o::TEST_IS_NOT => a != b,
            o::TEST_IN => self.contains(b, a)?,
            o::TEST_NOT_IN => !self.contains(b, a)?,
            _ => {
                let sym = match opcode {
                    o::TEST_LT => "<",
                    o::TEST_LE => "<=",
                    o::TEST_GT => ">",
                    _ => ">=",
                };
                let setlike =
                    |t: T| matches!(t, T::set | T::frozenset | T::dict_keys | T::dict_items);
                let (ua, ub) = (self.unbx(a), self.unbx(b));
                let (ta, tb) = (self.type_of(ua), self.type_of(ub));
                if setlike(ta) && setlike(tb) && !(Vm::is_set(ua) && Vm::is_set(ub)) {
                    if ua != a || ub != b {
                        // A view and a set subclass's instance: the twin
                        // reads the instance as itself.
                        return Err(self.need_py());
                    }
                    // A set-like dict view: CPython's `all_contained_in`.
                    let lt = matches!(opcode, o::TEST_LT | o::TEST_LE);
                    let (x, y) = if lt { (a, b) } else { (b, a) };
                    let (lx, ly) = (self.len_of(x)?, self.len_of(y)?);
                    let strict = matches!(opcode, o::TEST_LT | o::TEST_GT);
                    if lx > ly || (strict && lx == ly) {
                        return Ok(Value::FALSE);
                    }
                    // `collect` hands a long operand off part way: the
                    // twin's own loop runs on the operands as they are.
                    if lx > 1024 {
                        return Err(self.need_py());
                    }
                    for &item in &self.collect(x)? {
                        if !self.contains(y, item)? {
                            return Ok(Value::FALSE);
                        }
                    }
                    return Ok(Value::TRUE);
                }
                let ord = self.order(a, b, sym)?;
                if Vm::is_set(ua) && Vm::is_set(ub) {
                    // A set subclass's instance compares as its set.
                    let (a, b) = (ua, ub);
                    let (sub, sup) = (self.set_issubset_r(a, b)?, self.set_issubset_r(b, a)?);
                    let n = self.set_len(a) != self.set_len(b);
                    return Ok(Value::from_bool(match opcode {
                        o::TEST_LT => sub && n,
                        o::TEST_LE => sub,
                        o::TEST_GT => sup && n,
                        _ => sup,
                    }));
                }
                match (opcode, ord) {
                    (_, None) => false,
                    (o::TEST_LT, Some(x)) => x == Ordering::Less,
                    (o::TEST_LE, Some(x)) => x != Ordering::Greater,
                    (o::TEST_GT, Some(x)) => x == Ordering::Greater,
                    (_, Some(x)) => x != Ordering::Less,
                }
            }
        };
        Ok(Value::from_bool(r))
    }

    /// The builtin value `v` extends, if that type has its own
    /// `__contains__` and `__iter__` (which a subclass that overrides only
    /// `__getitem__` keeps).
    pub(crate) fn container_backing(&self, v: Value) -> Option<Value> {
        self.backing(v).filter(|&b| {
            matches!(
                self.type_of(b),
                T::str | T::list | T::tuple | T::dict | T::set | T::frozenset | T::bytes
            )
        })
    }

    /// `item in container`.
    pub(crate) fn contains(&mut self, container: Value, item: Value) -> Result<bool, Raised> {
        match self.type_of(container) {
            T::str => {
                // A str subclass (StrEnum member) as its value.
                let item = if Vm::is_str(item) {
                    item
                } else {
                    self.unbx(item)
                };
                if !Vm::is_str(item) {
                    return Err(self.err(
                        T::TypeError,
                        &format!(
                            "'in <string>' requires string as left operand, not {}",
                            self.type_name(item)
                        ),
                    ));
                }
                // A one-byte needle (an ASCII char) searches the bytes
                // without re-validating UTF-8. The search is byte work
                // (docs/spec.md §5).
                let hay = self.str_bytes(container);
                let (found, n) = if let [b] = *self.str_bytes(item) {
                    (hay.contains(&b), hay.len())
                } else {
                    let hay = self.str_ref(container);
                    (hay.contains(self.str_ref(item)), hay.len())
                };
                self.burn_bytes(n);
                Ok(found)
            }
            T::list | T::tuple => {
                // By index, re-reading the items each step (as CPython
                // does: `__eq__` may resize the list), one fuel unit per
                // item compared.
                let mut i = 0;
                while let Some(&x) = self.seq_words(container).get(i) {
                    let x = Value::from_bits(x).unwrap_or(Value::UNDEFINED);
                    i += 1;
                    if self.eq_elem(x, item)? {
                        self.burn(i as u64);
                        return Ok(true);
                    }
                }
                self.burn(i as u64);
                Ok(false)
            }
            T::dict => Ok(self.dict_get(container, item)?.is_some()),
            T::dict_keys => {
                let d = self.vslot(container, 0);
                Ok(self.dict_get(d, item)?.is_some())
            }
            T::set | T::frozenset => self.set_contains(container, item),
            T::bytes | T::bytearray => self.bytes_contains(container, item),
            T::range => {
                let Some(i) = self.int_of(item) else {
                    // An int past `i64` is past every item (bounds fit
                    // `i64`). Anything else is compared with each item, as
                    // CPython's `_PySequence_IterSearch` does; a long range
                    // in the prelude twin, item by item, not collected.
                    if Vm::is_heap_int(item) {
                        return Ok(false);
                    }
                    if self.range_len(container) >= COLLECT_CHUNK as u64 {
                        return Err(self.need_py());
                    }
                    let items = self.collect(container)?;
                    return Ok(self.find_eq(&items, item)?.is_some());
                };
                let (start, stop, step) = self.range_parts(container);
                let inside = if step > 0 {
                    i >= start && i < stop
                } else {
                    i <= start && i > stop
                };
                Ok(inside && (i128::from(i) - i128::from(start)) % i128::from(step) == 0)
            }
            T::dict_items => {
                // `(k, v) in d.items()`: look `k` up (hashing it), compare `v`.
                if !item.is_obj_kind(ty::TUPLE as u8) || self.tuple_len(item) != 2 {
                    return Ok(false);
                }
                let (k, v) = (self.vslot(item, 0), self.vslot(item, 1));
                let d = self.vslot(container, 0);
                match self.dict_get(d, k)? {
                    Some(found) => self.eq_elem(found, v),
                    None => Ok(false),
                }
            }
            T::dict_values => {
                let items = self.collect(container)?;
                Ok(self.find_eq(&items, item)?.is_some())
            }
            T::list_iterator
            | T::tuple_iterator
            | T::str_iterator
            | T::str_ascii_iterator
            | T::bytes_iterator
            | T::bytearray_iterator
            | T::range_iterator
            | T::dict_keyiterator
            | T::dict_valueiterator
            | T::dict_itemiterator
            | T::set_iterator
            | T::enumerate
            | T::zip
            | T::list_reverseiterator => {
                // Iterators are consumed up to the first match, one fuel
                // unit per item. An iterator need not end (a range's can
                // run for hours), so past the walk budget the prelude twin
                // goes on from here in bytecode, which parks on fuel,
                // deadlines and requests.
                let mut n = 0;
                while let Some(x) = self.iter_next(container)? {
                    n += 1;
                    if self.eq_elem(x, item)? {
                        self.burn(u64::from(n));
                        return Ok(true);
                    }
                    if n == crate::dict::WALK {
                        self.burn(u64::from(n));
                        return Err(self.need_py());
                    }
                }
                self.burn(u64::from(n));
                Ok(false)
            }
            _ => {
                // A builtin container's subclass keeps its `__contains__`,
                // whatever `__iter__`/`__getitem__` it overrides.
                if self.container_backing(container).is_none()
                    && self.uflags(container) & (f::ITER | f::GETITEM) != 0
                    || matches!(
                        container.obj_kind().map(u16::from),
                        Some(ty::GEN | ty::CALLITER)
                    )
                {
                    return Err(self.need_py());
                }
                if let Some(b) = self.backing(container) {
                    return self.contains(b, item);
                }
                let msg = format!(
                    "argument of type '{}' is not a container or iterable",
                    self.type_name(container)
                );
                Err(self.err(T::TypeError, &msg))
            }
        }
    }

    /// `x in s` (`not in` with `neg`) for list or tuple `s`, compared by
    /// index from `i` and re-reading `s` each item, as [`Vm::contains`]
    /// does, one fuel unit per item. After [`crate::nstep::STEP_WORK`]
    /// units it stops: `Ok(None)` with `i` where to go on. An item whose
    /// `==` needs Python code hands the rest to `_seq_has` (the value is
    /// then `UNDEFINED` with that request).
    pub(crate) fn seq_in_scan(
        &mut self,
        s: Value,
        x: Value,
        i: &mut usize,
        neg: bool,
    ) -> Result<Option<Value>, Raised> {
        let start = self.work;
        let mut n = 0u64;
        let out = loop {
            let Some(&y) = self.seq_words(s).get(*i) else {
                break Some(Value::from_bool(neg));
            };
            let y = Value::from_bits(y).unwrap_or(Value::UNDEFINED);
            n += 1;
            match self.eq_elem(y, x) {
                Ok(true) => break Some(Value::from_bool(!neg)),
                Ok(false) => {}
                Err(Raised) if self.slow => {
                    self.slow = false;
                    self.burn(n);
                    let at = Value::int(*i as i64).unwrap_or(Value::NONE);
                    let args = alloc::vec![s, x, at, Value::from_bool(neg)];
                    let f = self.prelude_fn("_seq_has")?;
                    return Ok(Some(self.request_call(f, args, crate::cont::Cont::None)));
                }
                Err(e) => {
                    self.burn(n);
                    return Err(e);
                }
            }
            *i += 1;
            if n + self.work.wrapping_sub(start) >= crate::nstep::STEP_WORK {
                break None;
            }
        };
        self.burn(n);
        Ok(out)
    }

    /// One step of a long `x in s` ([`Vm::seq_in_scan`]).
    pub(crate) fn seq_in_step(&mut self, state: Value) -> Result<crate::nstep::Step, Raised> {
        use crate::nstep::{Step, op};
        let Some(([s, x], [_, i, neg])) = self.nstate_get::<2, 3>(op::SEQ_IN, state) else {
            return Err(self.bad_step());
        };
        let mut i = usize::try_from(i).unwrap_or(usize::MAX);
        if let Some(v) = self.seq_in_scan(s, x, &mut i, neg != 0)? {
            return Ok(Step::Done(v));
        }
        self.nstate_set_raw(state, 1, i as u64);
        Ok(Step::More)
    }

    /// `x in v` (`not in` with `neg`) for dict values view `v`, a step at
    /// a time ([`Vm::dv_in_scan`]), then more in `_steps`; out of line, as
    /// the `in` operator is on the dispatch loop's path.
    #[inline(never)]
    pub(crate) fn dv_in(&mut self, v: Value, x: Value, neg: bool) -> Result<Value, Raised> {
        let mut i = 0;
        if let Some(r) = self.dv_in_scan(v, x, &mut i, neg)? {
            return Ok(r);
        }
        self.chunk_rest(
            crate::nstep::op::DV_IN,
            &[v, x],
            &[i as u64, u64::from(neg)],
        )
    }

    /// `x in v` for dict values view `v` from entry `i` on, as
    /// [`Vm::seq_in_scan`] goes through a list: in place, one fuel unit
    /// per entry, `Ok(None)` after [`crate::nstep::STEP_WORK`] units. An
    /// item whose `==` needs Python code hands the whole test to
    /// `_op_contains`: the entries before it compared without Python
    /// code, so comparing them again shows nothing.
    fn dv_in_scan(
        &mut self,
        v: Value,
        x: Value,
        i: &mut usize,
        neg: bool,
    ) -> Result<Option<Value>, Raised> {
        let d = self.vslot_o(v, 0);
        let start = self.work;
        let mut n = 0u64;
        let out = loop {
            let e = u32::try_from(*i).unwrap_or(u32::MAX);
            let Some((k, y)) = self.dict_entry_at(d, e) else {
                break Some(Value::from_bool(neg));
            };
            *i += 1;
            n += 1;
            if k != Value::UNDEFINED {
                match self.eq_elem(y, x) {
                    Ok(true) => break Some(Value::from_bool(!neg)),
                    Ok(false) => {}
                    Err(Raised) if self.slow => {
                        self.slow = false;
                        self.burn(n);
                        let twin = if neg {
                            "_op_not_contains"
                        } else {
                            "_op_contains"
                        };
                        let f = self.prelude_fn(twin)?;
                        let call = self.request_call(f, vec![v, x], crate::cont::Cont::None);
                        return Ok(Some(call));
                    }
                    Err(e) => {
                        self.burn(n);
                        return Err(e);
                    }
                }
            }
            if n + self.work.wrapping_sub(start) >= crate::nstep::STEP_WORK {
                break None;
            }
        };
        self.burn(n);
        Ok(out)
    }

    /// One step of a long `x in d.values()` ([`Vm::dv_in_scan`]).
    pub(crate) fn dv_in_step(&mut self, state: Value) -> Result<crate::nstep::Step, Raised> {
        use crate::nstep::{Step, op};
        let Some(([v, x], [_, i, neg])) = self.nstate_get::<2, 3>(op::DV_IN, state) else {
            return Err(self.bad_step());
        };
        let mut i = usize::try_from(i).unwrap_or(usize::MAX);
        if let Some(r) = self.dv_in_scan(v, x, &mut i, neg != 0)? {
            return Ok(Step::Done(r));
        }
        self.nstate_set_raw(state, 1, i as u64);
        Ok(Step::More)
    }

    /// Builds or rebuilds a list or tuple from up to three segments of
    /// other sequences ([`Seg`]), writing the items to `dst` (a `BUF` or
    /// a tuple) from index `at` on, `dstep` apart. `target` is the list
    /// that takes `dst` as its buffer, `total` items long, at the end
    /// (`NONE`: none does); the value is `out`. Registers 3 to 5 are the
    /// segments' sources. Long work goes on in steps
    /// (`nstep::op::SEQ_BUILD`), so this is for callers that can hand
    /// their value to a request.
    #[allow(clippy::too_many_arguments, reason = "the build's parts")]
    pub(crate) fn seq_build(
        &mut self,
        target: Value,
        out: Value,
        dst: Value,
        srcs: &[Value],
        segs: &[Seg],
        total: usize,
        at: usize,
        dstep: i64,
    ) -> Result<Value, Raised> {
        let mut r = [target, out, dst, Value::NONE, Value::NONE, Value::NONE];
        for (o, &v) in r[3..].iter_mut().zip(srcs) {
            *o = v;
        }
        let mut w: Build = [0; 22];
        (w[0], w[4], w[5]) = (total as u64, at as u64, dstep.cast_unsigned());
        for (o, g) in w[7..].as_chunks_mut::<5>().0.iter_mut().zip(segs) {
            *o = g.words();
        }
        if let Some(v) = self.seq_build_run(&r, &mut w)? {
            return Ok(v);
        }
        self.chunk_rest(crate::nstep::op::SEQ_BUILD, &r, &w)
    }

    /// One step of a [`Vm::seq_build`].
    pub(crate) fn seq_build_step(&mut self, state: Value) -> Result<crate::nstep::Step, Raised> {
        use crate::nstep::{Step, op};
        let Some((r, raw)) = self.nstate_get::<6, 23>(op::SEQ_BUILD, state) else {
            return Err(self.bad_step());
        };
        let mut w: Build = [0; 22];
        w.copy_from_slice(&raw[1..]);
        if let Some(v) = self.seq_build_run(&r, &mut w)? {
            return Ok(Step::Done(v));
        }
        self.nstate_put(state, 1, &w);
        Ok(Step::More)
    }

    /// Batches of build `w` on registers `r` until done (its value) or a
    /// step's work is spent (`None`, `w` updated).
    fn seq_build_run(&mut self, r: &[Value; 6], w: &mut Build) -> Result<Option<Value>, Raised> {
        let start = self.work;
        let mut units = 0u64;
        let (total, dstep) = (w[0], w[5].cast_signed());
        let [mut seg, mut pos, mut rep, mut at] = [w[1], w[2], w[3], w[4]];
        let Some(dst) = r[2].as_obj() else {
            return Err(self.bad_step());
        };
        let mut tmp = [Value::UNDEFINED; 256];
        // Items moved: a copy is charged a unit per 8, but a step moves at
        // most `STEP_WORK` of them (about a millisecond).
        let mut moved = 0u64;
        loop {
            let spent = units + self.work.wrapping_sub(start);
            if spent >= crate::nstep::STEP_WORK || moved >= crate::nstep::STEP_WORK {
                self.burn(units);
                w[1..5].copy_from_slice(&[seg, pos, rep, at]);
                return Ok(None);
            }
            let Some(g) = Seg::at(w, seg) else {
                break;
            };
            if g.count == 0 || rep >= g.reps {
                (seg, pos, rep) = (seg + 1, 0, 0);
                continue;
            }
            // A batch from the segment's source, read in place; a range's
            // items (all small ints, `range_seq`) computed.
            let mut k = 0;
            {
                let src = r.get(usize::from(g.src)).copied().unwrap_or(Value::NONE);
                let ranged = src
                    .is_obj_kind(ty::RANGE as u8)
                    .then(|| self.range_parts(src));
                let words = self.seq_words(src);
                while k < tmp.len() && rep < g.reps {
                    let i = if g.skip {
                        // The `pos`-th index kept: every `step`-th from
                        // `start` is deleted.
                        let p = g.step.max(2).unsigned_abs() - 1;
                        g.start.saturating_add((pos + pos / p + 1).cast_signed())
                    } else {
                        g.start
                            .saturating_add(pos.cast_signed().saturating_mul(g.step))
                    };
                    tmp[k] = if let Some((start, _, step)) = ranged {
                        i64::try_from(i128::from(start) + i128::from(i) * i128::from(step))
                            .ok()
                            .and_then(Value::int)
                            .unwrap_or(Value::NONE)
                    } else {
                        let x = usize::try_from(i).ok().and_then(|i| words.get(i));
                        x.and_then(|&x| Value::from_bits(x)).unwrap_or(Value::NONE)
                    };
                    k += 1;
                    pos += 1;
                    if pos == g.count {
                        (pos, rep) = (0, rep + 1);
                    }
                }
            }
            moved += k as u64;
            if dstep == 1 {
                let res = self.heap.set_values(dst, at as u32, &tmp[..k]);
                self.hx(res)?;
                at += k as u64;
                units += (k as u64 / 8).max(1);
            } else {
                for &x in &tmp[..k] {
                    let res = self.heap.set(dst, at as u32, x);
                    self.hx(res)?;
                    at = at.wrapping_add(dstep.cast_unsigned());
                }
                units += k as u64;
            }
        }
        self.burn(units);
        let [target, out, dst, ..] = *r;
        if let Some(t) = target
            .as_obj()
            .filter(|_| target.is_obj_kind(ty::LIST as u8))
        {
            // The old buffer is garbage now, though a collection during
            // the rebuild found it in use: counted, so a collection (or an
            // instruction's out-of-memory retry) can reclaim it.
            if let Some(old) = self.list_buf(target) {
                self.heap.count_dropped(old);
            }
            self.set_slot(t, 0, dst);
            self.set_raw(t, 1, total);
        }
        Ok(Some(out))
    }

    /// Long range `r` of small ints as a new list or tuple, its items
    /// computed a slice per step, as a copy of a list goes: one buffer and
    /// no list of the items first (`None` for a short range, or one whose
    /// items are not all small ints, which `collect` gathers).
    pub(crate) fn range_seq(&mut self, r: Value, tuple: bool) -> Result<Option<Value>, Raised> {
        if !r.is_obj_kind(ty::RANGE as u8) {
            return Ok(None);
        }
        let n = self.range_len(r);
        let (start, _, step) = self.range_parts(r);
        let last = i128::from(start) + i128::from(n.saturating_sub(1)) * i128::from(step);
        let small = |x: i128| {
            (i128::from(Value::SMALL_INT_MIN)..=i128::from(Value::SMALL_INT_MAX)).contains(&x)
        };
        if n < SEQ_LONG as u64 || !small(i128::from(start)) || !small(last) {
            return Ok(None);
        }
        if n > 1 << 26 || !self.heap.would_fit(n * 8) {
            return Err(self.err(T::MemoryError, ""));
        }
        let n = n as usize;
        self.seq_new(tuple, &[r], &[Seg::of(3, 0, 1, n as u64)], n)
            .map(Some)
    }

    /// `seq` (a list or tuple, `kind`) repeated `k` times, or the items
    /// of `segs`, `total` long, as a new list or tuple, in steps.
    pub(crate) fn seq_new(
        &mut self,
        tuple: bool,
        srcs: &[Value],
        segs: &[Seg],
        total: usize,
    ) -> Result<Value, Raised> {
        // The batches charge for the result, a unit per 8 items, as its
        // constructor would.
        let (out, dst) = if tuple {
            let n = u32::try_from(total).map_err(|_| self.err(T::MemoryError, ""))?;
            let res = self.heap.alloc_unfilled(ty::TUPLE, 0, n);
            let t = Value::from_ref(self.hx(res)?, ty::TUPLE as u8);
            (t, t)
        } else {
            let l = self.list_of_len(total)?;
            let buf = self.vslot(l, 0);
            (l, buf)
        };
        self.seq_build(Value::NONE, out, dst, srcs, segs, total, 0, 1)
    }

    /// `l.extend(src)` for a list or tuple `src` with a long result:
    /// CPython's one copy, here a slice per step; `None` for any other.
    /// `list.extend` and `[*src]` (`LIST_EXTEND`).
    #[inline(never)]
    pub(crate) fn list_extend_long(
        &mut self,
        l: Value,
        src: Value,
    ) -> Option<Result<Value, Raised>> {
        if !(src.is_obj_kind(ty::LIST as u8) || src.is_obj_kind(ty::TUPLE as u8)) {
            return None;
        }
        let (n, m) = (self.list_len(l), self.seq_words(src).len());
        if n + m < SEQ_LONG {
            return None;
        }
        let segs = [Seg::of(3, 0, 1, n as u64), Seg::of(4, 0, 1, m as u64)];
        Some(self.list_rebuild(l, Value::NONE, &[l, src], &segs, n + m))
    }

    /// Appends up to `COLLECT_CHUNK` items of iterator `it` to list `l` as
    /// `it` yields them, one fuel unit each: true if `it` may have more.
    pub(crate) fn list_more(&mut self, l: Value, it: Value) -> Result<bool, Raised> {
        let mut n = 0;
        let more = loop {
            if n == COLLECT_CHUNK {
                break true;
            }
            let x = match self.iter_next(it) {
                Ok(Some(x)) => x,
                Ok(None) => break false,
                Err(e) => {
                    self.burn(n as u64);
                    return Err(e);
                }
            };
            self.list_push(l, x)?;
            n += 1;
        };
        self.burn(n as u64);
        Ok(more)
    }

    /// `l.extend(src)` and list `+=` for a `src` that is not a list or a
    /// tuple. A builtin iterator's items are appended as it yields them,
    /// as CPython's `list_extend_iter` does: code it runs sees the list
    /// grow, and an error keeps the items before it. Past a chunk, or at an
    /// item that needs Python code, the prelude twin goes on with the
    /// iterator. Any other iterable's items, which cannot see the list,
    /// at once.
    pub(crate) fn list_extend_from(&mut self, l: Value, src: Value) -> Result<(), Raised> {
        if Self::one_shot(src) {
            // A list or tuple iterator's items left are its length hint:
            // the buffer grows once to hold them first, so running out of
            // memory leaves the list as it was, as in CPython.
            if let (Some(r), true) = (src.as_obj(), src.is_obj_kind(ty::ITER as u8)) {
                let (seq, kind, at) = (self.vslot(src, 0), self.raw_o(r, 1), self.raw_o(r, 2));
                let n = match kind {
                    it::LIST => self.list_len(seq),
                    it::TUPLE => self.tuple_len(seq),
                    _ => 0,
                };
                let left = n.saturating_sub(usize::try_from(at).unwrap_or(usize::MAX));
                if left > 0 {
                    self.list_room(l, left)?;
                }
            }
            // What it yields is gone: not run again after a collection.
            self.oom_rerun = self.op_stamp();
            if self.list_more(l, src)? {
                return Err(self.need_py());
            }
            return Ok(());
        }
        let items = self.collect(src)?;
        self.list_extend_values(l, &items)
    }

    /// `LIST_EXTEND`'s items of `src`, or none when a long list or tuple
    /// extends list `l` a slice per step ([`Vm::list_extend_long`]).
    #[inline(never)]
    pub(crate) fn extend_items(&mut self, l: Value, src: Value) -> Result<NativeValues, Raised> {
        match self.list_extend_long(l, src) {
            Some(r) => r.map(|_| NativeValues::default()),
            None => self.collect(src),
        }
    }

    /// `LIST_TO_TUPLE`: a long list's items into the tuple in steps.
    #[inline(never)]
    pub(crate) fn list_to_tuple(&mut self, l: Value) -> Result<Value, Raised> {
        let n = self.list_len(l);
        if n >= SEQ_LONG {
            return self.seq_new(true, &[l], &[Seg::of(3, 0, 1, n as u64)], n);
        }
        let items = self.list_items(l);
        self.alloc_tuple(&items)
    }

    /// List `l` gets a new buffer of the items of `segs`, `total` long, in
    /// steps; the value is `out`.
    pub(crate) fn list_rebuild(
        &mut self,
        l: Value,
        out: Value,
        srcs: &[Value],
        segs: &[Seg],
        total: usize,
    ) -> Result<Value, Raised> {
        let buf = self.alloc_items(total.max(4))?;
        let buf = Value::from_ref(buf, ty::BUF as u8);
        self.seq_build(l, out, buf, srcs, segs, total, 0, 1)
    }

    /// `count` (`kind` 0), `index` (1) or `remove` (2) of `x` in list or
    /// tuple `s`, items `i..stop`, in steps.
    pub(crate) fn seq_scan(
        &mut self,
        s: Value,
        x: Value,
        kind: u64,
        i: usize,
        stop: usize,
    ) -> Result<Value, Raised> {
        let mut raw = [kind, i as u64, stop as u64, 0, 0];
        if let Some(v) = self.seq_scan_run(s, x, &mut raw)? {
            return Ok(v);
        }
        self.chunk_rest(crate::nstep::op::SEQ_SCAN, &[s, x], &raw)
    }

    /// One step of a [`Vm::seq_scan`].
    pub(crate) fn seq_scan_step(&mut self, state: Value) -> Result<crate::nstep::Step, Raised> {
        use crate::nstep::{Step, op};
        let Some(([s, x], [_, kind, i, stop, c, z])) = self.nstate_get::<2, 6>(op::SEQ_SCAN, state)
        else {
            return Err(self.bad_step());
        };
        let mut raw = [kind, i, stop, c, z];
        if let Some(v) = self.seq_scan_run(s, x, &mut raw)? {
            return Ok(Step::Done(v));
        }
        self.nstate_put(state, 1, &raw);
        Ok(Step::More)
    }

    /// A step of a scan, `raw` its `kind, i, stop, count`: its value, or
    /// `None` with more to go. An item whose `==` needs Python code hands
    /// the rest to `_seq_scan_rest`.
    fn seq_scan_run(
        &mut self,
        s: Value,
        x: Value,
        raw: &mut [u64; 5],
    ) -> Result<Option<Value>, Raised> {
        let start = self.work;
        let [kind, i, stop, c, _] = *raw;
        let (mut i, mut c, mut n) = (usize::try_from(i).unwrap_or(usize::MAX), c, 0u64);
        let stop = usize::try_from(stop).unwrap_or(usize::MAX);
        let found = loop {
            let Some(&y) = self.seq_words(s).get(i).filter(|_| i < stop) else {
                break false;
            };
            let y = Value::from_bits(y).unwrap_or(Value::UNDEFINED);
            n += 1;
            match self.eq_elem(y, x) {
                Ok(true) if kind == 0 => c += 1,
                Ok(true) => break true,
                Ok(false) => {}
                Err(Raised) if self.slow => {
                    self.slow = false;
                    self.burn(n);
                    let mut args = alloc::vec![s, x];
                    for w in [i as u64, stop as u64, kind, c] {
                        args.push(self.new_int(w.min(i64::MAX as u64).cast_signed())?);
                    }
                    let f = self.prelude_fn("_seq_scan_rest")?;
                    return Ok(Some(self.request_call(f, args, crate::cont::Cont::None)));
                }
                Err(e) => {
                    self.burn(n);
                    return Err(e);
                }
            }
            i += 1;
            if n + self.work.wrapping_sub(start) >= crate::nstep::STEP_WORK {
                self.burn(n);
                (raw[1], raw[3]) = (i as u64, c);
                return Ok(None);
            }
        };
        self.burn(n);
        match (kind, found) {
            (0, _) => Ok(Some(self.new_int(c.cast_signed())?)),
            (1, true) => Ok(Some(self.new_int(i as i64)?)),
            (2, true) => self.list_del_at(s, i, Value::NONE).map(Some),
            (1, false) => {
                let t = self.type_name(s);
                let msg = format!("{t}.index(x): x not in {t}");
                Err(self.err(T::ValueError, &msg))
            }
            _ => Err(self.err(T::ValueError, "list.remove(x): x not in list")),
        }
    }

    /// `l[start:..:step] = v` (`len` items) for a long list `l`, in
    /// steps: the new items of a list or tuple `v` are read in place,
    /// any other iterable's collected first. `None` for the short form.
    fn list_assign_long(
        &mut self,
        l: Value,
        v: Value,
        itv: Value,
        start: i64,
        step: i64,
        len: usize,
    ) -> Result<Option<Result<(), Raised>>, Raised> {
        let mut src = if v.is_obj_kind(ty::LIST as u8) || v.is_obj_kind(ty::TUPLE as u8) {
            v
        } else {
            let items = self.collect(itv)?;
            self.new_list(&items)?
        };
        let m = self.seq_words(src).len();
        let n = self.list_len(l);
        let r = if step == 1 {
            // CPython's `list_ass_slice`: the head, the new items, the tail.
            let s = usize::try_from(start.max(0)).unwrap_or(0).min(n);
            let e = (s + len).min(n);
            let segs = [
                Seg::of(3, 0, 1, s as u64),
                Seg::of(4, 0, 1, m as u64),
                Seg::of(3, e as i64, 1, (n - e) as u64),
            ];
            self.list_rebuild(l, Value::NONE, &[l, src], &segs, n - (e - s) + m)
        } else {
            if m != len {
                return Ok(Some(Err(self.err(
                    T::ValueError,
                    &format!(
                        "attempt to assign sequence of size {m} to extended slice of size {len}"
                    ),
                ))));
            }
            if src == l {
                // `l[::-1] = l`: the items as they were (CPython copies).
                src = self.seq_copy(l)?;
            }
            let Some(buf) = self.list_buf(l) else {
                return Ok(None);
            };
            let buf = Value::from_ref(buf, ty::BUF as u8);
            let segs = [Seg::of(3, 0, 1, m as u64)];
            let at = usize::try_from(start).unwrap_or(0);
            self.seq_build(Value::NONE, Value::NONE, buf, &[src], &segs, n, at, step)
        };
        Ok(Some(r.map(|_| ())))
    }

    /// `del l[key]` for a long list `l` (`n` items), in steps; `None` for
    /// a key the short form handles (and its errors).
    fn list_del_long(
        &mut self,
        l: Value,
        key: Value,
        n: usize,
    ) -> Result<Option<Result<(), Raised>>, Raised> {
        if let Some(i) = self.int_of(key) {
            let Some(k) = Vm::norm_index(i, n) else {
                return Ok(None);
            };
            return Ok(Some(self.list_del_at(l, k, Value::NONE).map(|_| ())));
        }
        if !key.is_obj_kind(ty::SLICE as u8) {
            return Ok(None);
        }
        let (start, _, step, len) = self.slice_indices(key, n)?;
        if len == 0 {
            return Ok(Some(Ok(())));
        }
        // The deleted indexes, ascending: `lo`, `lo + p`, ..., `hi`.
        let (lo, p) = if step > 0 {
            (start, step)
        } else {
            (start + (len as i64 - 1) * step, -step)
        };
        let (lo, p) = (usize::try_from(lo).unwrap_or(0), p.unsigned_abs() as usize);
        let hi = lo + (len - 1) * p;
        let mid = if p == 1 {
            Seg::of(3, 0, 1, 0)
        } else {
            Seg {
                skip: true,
                step: p as i64,
                ..Seg::of(3, lo as i64, 1, ((hi + 1 - lo) - len) as u64)
            }
        };
        let segs = [
            Seg::of(3, 0, 1, lo as u64),
            mid,
            Seg::of(3, (hi + 1) as i64, 1, (n - hi - 1) as u64),
        ];
        Ok(Some(
            self.list_rebuild(l, Value::NONE, &[l], &segs, n - len)
                .map(|_| ()),
        ))
    }

    /// Deletes item `k` of list `l`; the value is `out`. The tail moves
    /// down a slot in place, as CPython's `memmove` (a unit per 8 items):
    /// 8M items at the object cap take about 10 ms [M].
    pub(crate) fn list_del_at(&mut self, l: Value, k: usize, out: Value) -> Result<Value, Raised> {
        let Some((buf, n)) = self.list_parts(l) else {
            return Ok(out);
        };
        let n = usize::try_from(n).unwrap_or(0);
        if k < n
            && let Some(r) = l.as_obj()
        {
            let tail = n - k - 1;
            self.burn(tail as u64 / 8);
            let at = |i: usize| u32::try_from(i).unwrap_or(u32::MAX);
            let res = self.heap.move_values(buf, at(k + 1), at(k), at(tail));
            self.hx(res)?;
            self.set_slot(buf, at(n - 1), Value::UNDEFINED);
            self.set_raw(r, 1, (n - 1) as u64);
        }
        Ok(out)
    }

    /// The index of the first of `items` equal to `x` (`eq_elem`), charging
    /// one fuel unit per item compared, as a comparison walk charges per
    /// visit.
    pub(crate) fn find_eq(&mut self, items: &[Value], x: Value) -> Result<Option<usize>, Raised> {
        for (i, &y) in items.iter().enumerate() {
            if self.eq_elem(y, x)? {
                self.burn(i as u64 + 1);
                return Ok(Some(i));
            }
        }
        self.burn(items.len() as u64);
        Ok(None)
    }

    // ------------------------------------------------------- subscripts

    /// Normalises index `i` for length `n`; `None` if out of range.
    pub(crate) fn norm_index(i: i64, n: usize) -> Option<usize> {
        let n = i64::try_from(n).ok()?;
        let j = if i < 0 { i + n } else { i };
        if (0..n).contains(&j) {
            usize::try_from(j).ok()
        } else {
            None
        }
    }

    /// `slice.indices(n)`: start, stop, step and length.
    pub(crate) fn slice_indices(
        &mut self,
        s: Value,
        n: usize,
    ) -> Result<(i64, i64, i64, usize), Raised> {
        let (start, stop, step, len) = self.slice_span(s, i64::try_from(n).unwrap_or(i64::MAX))?;
        // At most `n`, so it fits.
        Ok((
            start,
            stop,
            step,
            usize::try_from(len).unwrap_or(usize::MAX),
        ))
    }

    /// [`Vm::slice_indices`] for a length in `i64`: a range can be longer
    /// than a wasm32 `usize`.
    pub(crate) fn slice_span(&mut self, s: Value, n: i64) -> Result<(i64, i64, i64, i64), Raised> {
        let get = |vm: &mut Vm, i: u32| -> Result<Option<i64>, Raised> {
            let v = vm.vslot(s, i);
            if v == Value::NONE {
                return Ok(None);
            }
            match vm.int_of(v) {
                Some(x) => Ok(Some(x)),
                None if Vm::is_heap_int(v) => Ok(Some(if vm.big_of(v).is_some_and(|b| b.neg) {
                    i64::MIN / 2
                } else {
                    i64::MAX / 2
                })),
                None if vm.uflags(v) & f::INDEX != 0 => Err(vm.need_py()),
                None => backed_slice_index(vm, v).map(Some),
            }
        };
        let step = get(self, 2)?.unwrap_or(1);
        if step == 0 {
            return Err(self.err(T::ValueError, "slice step cannot be zero"));
        }
        let (lo, hi) = if step < 0 { (-1, n - 1) } else { (0, n) };
        let fix = |v: Option<i64>, default: i64| -> i64 {
            match v {
                None => default,
                Some(x) if x < 0 => (x + n).max(lo),
                Some(x) => x.min(hi),
            }
        };
        let start = fix(get(self, 0)?, if step < 0 { hi } else { lo });
        let stop = fix(get(self, 1)?, if step < 0 { lo } else { hi });
        let len = if step > 0 {
            if stop > start {
                (stop - start - 1) / step + 1
            } else {
                0
            }
        } else if start > stop {
            ((start - stop - 1) as u64 / step.unsigned_abs()) as i64 + 1
        } else {
            0
        };
        Ok((start, stop, step, len))
    }

    /// The value words of `items` a slice picks, as values.
    fn slice_pick(items: &[u64], start: i64, step: i64, len: usize) -> Vec<Value> {
        let mut out = Vec::with_capacity(len);
        let mut i = start;
        for _ in 0..len {
            if let Some(&x) = usize::try_from(i).ok().and_then(|k| items.get(k)) {
                out.push(Value::from_bits(x).unwrap_or(Value::UNDEFINED));
            }
            i = i.saturating_add(step);
        }
        out
    }

    /// Do subscripts of `t` take an index (so a huge int overflows)?
    pub(crate) fn is_sequence_type(t: T) -> bool {
        matches!(
            t,
            T::list | T::tuple | T::str | T::range | T::bytes | T::bytearray
        )
    }

    /// `obj[key]`.
    pub(crate) fn getitem(&mut self, obj: Value, key: Value) -> Result<Value, Raised> {
        if obj.is_obj_kind(ty::DICT as u8) {
            return match self.dict_get(obj, key)? {
                Some(v) => Ok(v),
                None => Err(self.err_value(T::KeyError, key)),
            };
        }
        let t = self.type_of(obj);
        let key = if Vm::is_sequence_type(t) {
            self.unbx(key)
        } else {
            key
        };
        if Vm::is_heap_int(key) && self.int_of(key).is_none() && Vm::is_sequence_type(t) {
            return Err(self.index_overflow(T::IndexError));
        }
        let is_slice = key.is_obj_kind(ty::SLICE as u8);
        match t {
            T::list | T::tuple => {
                let n = if t == T::list {
                    self.list_len(obj)
                } else {
                    self.tuple_len(obj)
                };
                if let Some(i) = self.int_of(key) {
                    return match Vm::norm_index(i, n) {
                        Some(k) => Ok(if t == T::list {
                            self.list_get(obj, k)
                        } else {
                            self.vslot(obj, u32::try_from(k).unwrap_or(0))
                        }),
                        None => Err(self.err(
                            T::IndexError,
                            if t == T::list {
                                "list index out of range"
                            } else {
                                "tuple index out of range"
                            },
                        )),
                    };
                }
                if is_slice {
                    let (start, _, step, len) = self.slice_indices(key, n)?;
                    if len >= SEQ_LONG {
                        let segs = [Seg::of(3, start, step, len as u64)];
                        return self.seq_new(t == T::tuple, &[obj], &segs, len);
                    }
                    // Only the picked items are read (the result's
                    // constructor charges for them), not the whole sequence.
                    let picked = Vm::slice_pick(self.seq_words(obj), start, step, len);
                    return if t == T::list {
                        self.new_list(&picked)
                    } else {
                        self.alloc_tuple(&picked)
                    };
                }
                if self.uflags(key) & f::INDEX != 0 {
                    return Err(self.need_py());
                }
                Err(self.err(
                    T::TypeError,
                    &format!(
                        "{} indices must be integers or slices, not {}",
                        t.name(),
                        self.type_name(key)
                    ),
                ))
            }
            T::str => {
                if let Some(i) = self.int_of(key) {
                    let n = self.str_len(obj);
                    let Some(k) = Vm::norm_index(i, n) else {
                        return Err(self.err(T::IndexError, "string index out of range"));
                    };
                    let c = if self.str_ascii(obj) {
                        self.str_bytes(obj).get(k).map(|b| char::from(*b))
                    } else {
                        // From the nearest known position, not from the
                        // start, with the walk charged (docs/spec.md §5).
                        let at = self.str_offset(obj, k);
                        let b = self.str_bytes(obj);
                        let c = b.get(at..char_end(b, at)).unwrap_or(&[]);
                        core::str::from_utf8(c).ok().and_then(|c| c.chars().next())
                    };
                    let mut buf = [0u8; 4];
                    let s = c.unwrap_or(' ').encode_utf8(&mut buf);
                    return self.new_str(s);
                }
                if is_slice {
                    let n = self.str_len(obj);
                    let (start, _, step, len) = self.slice_indices(key, n)?;
                    if step == 1 {
                        // Only the slice is read, and copied once.
                        let st = usize::try_from(start.max(0)).unwrap_or(0);
                        let (from, to) = if self.str_ascii(obj) {
                            (st, st + len)
                        } else {
                            let from = self.str_offset(obj, st);
                            (from, self.str_offset(obj, st + len))
                        };
                        return crate::strmeth::slice_steps(self, obj, from, to);
                    }
                    return crate::strmeth::ext_slice(self, obj, start, step, len);
                }
                if self.uflags(key) & f::INDEX != 0 {
                    return Err(self.need_py());
                }
                Err(self.err(
                    T::TypeError,
                    &format!(
                        "string indices must be integers, not '{}'",
                        self.type_name(key)
                    ),
                ))
            }
            T::dict => match self.dict_get(obj, key)? {
                Some(v) => Ok(v),
                None => Err(self.err_value(T::KeyError, key)),
            },
            T::bytes | T::bytearray => self.bytes_getitem(obj, key),
            T::range => {
                let (start, _, step) = self.range_parts(obj);
                let n = self.range_len(obj);
                if let Some(i) = self.int_of(key) {
                    // In i128: a range may be longer than an isize.
                    let len = i128::from(n);
                    let j = if i < 0 {
                        i128::from(i) + len
                    } else {
                        i128::from(i)
                    };
                    let v = i128::from(start) + j * i128::from(step);
                    return match i64::try_from(v) {
                        Ok(v) if (0..len).contains(&j) => self.new_int(v),
                        _ => Err(self.err(T::IndexError, "range object index out of range")),
                    };
                }
                if is_slice {
                    return self.range_slice(obj, key);
                }
                if self.uflags(key) & f::INDEX != 0 {
                    return Err(self.need_py());
                }
                Err(self.err(
                    T::TypeError,
                    &format!(
                        "range indices must be integers or slices, not {}",
                        self.type_name(key)
                    ),
                ))
            }
            #[cfg(feature = "re")]
            T::Match => crate::re::match_item(self, obj, key),
            T::type_ if !Vm::is_class(obj) => {
                // `list[int]`: a `types.GenericAlias`, built in Python.
                if self
                    .type_of_typeobj(obj)
                    .is_some_and(crate::dunder::generic_type)
                {
                    return Err(self.need_py());
                }
                let msg = format!("type '{}' is not subscriptable", self.class_tpname(obj));
                Err(self.err(T::TypeError, &msg))
            }
            _ => {
                if let Some(b) = self.backing(obj) {
                    return self.getitem_bx(obj, b, key);
                }
                if self.user_class(obj).is_some() || Vm::is_class(obj) {
                    return Err(self.need_py());
                }
                let msg = format!("'{}' object is not subscriptable", self.type_name(obj));
                Err(self.err(T::TypeError, &msg))
            }
        }
    }

    /// `obj[key]` for an instance of a builtin-type subclass extending
    /// `b`: a `dict` subclass's `__missing__` answers missing keys.
    #[cold]
    fn getitem_bx(&mut self, obj: Value, b: Value, key: Value) -> Result<Value, Raised> {
        if b.is_obj_kind(ty::DICT as u8)
            && self.dict_get(b, key)?.is_none()
            && let Some(m) = self.user_method(obj, Dn::Missing)
        {
            let n = self.call_dunder(m, obj, &[key], crate::cont::Cont::None)?;
            return self.next_request(n);
        }
        self.getitem(b, key)
    }

    /// `obj[key] = v`.
    pub(crate) fn setitem(&mut self, obj: Value, key: Value, v: Value) -> Result<(), Raised> {
        if obj.is_obj_kind(ty::DICT as u8) {
            return self.dict_set_top(obj, key, v);
        }
        if Vm::is_heap_int(key) && self.int_of(key).is_none() && self.type_of(obj) == T::list {
            return Err(self.index_overflow(T::IndexError));
        }
        match self.type_of(obj) {
            T::list => {
                let n = self.list_len(obj);
                if let Some(i) = self.int_of(key) {
                    let Some(k) = Vm::norm_index(i, n) else {
                        return Err(self.err(T::IndexError, "list assignment index out of range"));
                    };
                    if let Some(b) = self.list_buf(obj) {
                        self.set_slot(b, u32::try_from(k).unwrap_or(0), v);
                    }
                    return Ok(());
                }
                if key.is_obj_kind(ty::SLICE as u8) {
                    let (start, _, step, len) = self.slice_indices(key, n)?;
                    let itv = match self.get_iter(v) {
                        Ok(i) => i,
                        Err(Raised) if self.slow => return Err(Raised),
                        Err(Raised) => {
                            let msg = "must assign iterable to extended slice";
                            return Err(self.err(T::TypeError, msg));
                        }
                    };
                    // A long list or tuple value goes the long way too: its
                    // iterator would stop at `COLLECT_CHUNK` and hand off to
                    // `_op_setitem`, which stores the same list again.
                    let m = self.seq_items_len(v).unwrap_or(0);
                    if len.max(n).max(m) >= SEQ_LONG
                        && let Some(r) = self.list_assign_long(obj, v, itv, start, step, len)?
                    {
                        return r;
                    }
                    let new = self.collect(itv)?;
                    let mut items = self.list_items(obj);
                    if step == 1 {
                        let s = usize::try_from(start.max(0)).unwrap_or(0).min(items.len());
                        let e = (s + len).min(items.len());
                        items.splice(s..e, new.iter().copied());
                    } else {
                        if new.len() != len {
                            return Err(self.err(
                                T::ValueError,
                                &format!(
                                    "attempt to assign sequence of size {} to extended slice of size {len}",
                                    new.len()
                                ),
                            ));
                        }
                        let mut i = start;
                        for &x in &new {
                            if let Some(slot) =
                                usize::try_from(i).ok().and_then(|k| items.get_mut(k))
                            {
                                *slot = x;
                            }
                            i = i.saturating_add(step);
                        }
                    }
                    return self.list_set_items(obj, &items);
                }
                if self.uflags(key) & f::INDEX != 0 {
                    return Err(self.need_py());
                }
                Err(self.err(
                    T::TypeError,
                    &format!(
                        "list indices must be integers or slices, not {}",
                        self.type_name(key)
                    ),
                ))
            }
            T::dict => self.dict_set_top(obj, key, v),
            T::bytearray => self.bytearray_setitem(obj, key, v),
            _ => {
                if let Some(b) = self.backing(obj) {
                    let key = if b.is_obj_kind(ty::LIST as u8) {
                        self.unbx(key)
                    } else {
                        key
                    };
                    return self.setitem(b, key, v);
                }
                if self.user_class(obj).is_some() {
                    return Err(self.need_py());
                }
                let msg = format!(
                    "'{}' object does not support item assignment",
                    self.type_name(obj)
                );
                Err(self.err(T::TypeError, &msg))
            }
        }
    }

    /// `del obj[key]`.
    pub(crate) fn delitem(&mut self, obj: Value, key: Value) -> Result<(), Raised> {
        if Vm::is_heap_int(key) && self.int_of(key).is_none() && self.type_of(obj) == T::list {
            return Err(self.index_overflow(T::IndexError));
        }
        match self.type_of(obj) {
            T::list => {
                let n = self.list_len(obj);
                if n >= SEQ_LONG
                    && let Some(r) = self.list_del_long(obj, key, n)?
                {
                    return r;
                }
                let mut items = self.list_items(obj);
                if let Some(i) = self.int_of(key) {
                    let Some(k) = Vm::norm_index(i, n) else {
                        return Err(self.err(T::IndexError, "list assignment index out of range"));
                    };
                    items.remove(k);
                } else if key.is_obj_kind(ty::SLICE as u8) {
                    let (start, _, step, len) = self.slice_indices(key, n)?;
                    // A mask, not a list of indices searched per item:
                    // `del l[::2]` was O(n²) in one native step.
                    let mut drop = alloc::vec![false; items.len()];
                    let mut i = start;
                    for _ in 0..len {
                        if let Some(d) = usize::try_from(i).ok().and_then(|k| drop.get_mut(k)) {
                            *d = true;
                        }
                        i = i.saturating_add(step);
                    }
                    let mut k = 0;
                    items.retain(|_| {
                        let keep = !drop[k];
                        k += 1;
                        keep
                    });
                } else {
                    if self.uflags(key) & f::INDEX != 0 {
                        return Err(self.need_py());
                    }
                    return Err(self.err(
                        T::TypeError,
                        &format!(
                            "list indices must be integers or slices, not {}",
                            self.type_name(key)
                        ),
                    ));
                }
                self.list_set_items(obj, &items)
            }
            T::dict => match self.dict_del(obj, key)? {
                Some(_) => Ok(()),
                None => Err(self.err_value(T::KeyError, key)),
            },
            T::bytearray => self.bytearray_delitem(obj, key),
            _ => {
                if let Some(b) = self.backing(obj) {
                    let key = if b.is_obj_kind(ty::LIST as u8) {
                        self.unbx(key)
                    } else {
                        key
                    };
                    return self.delitem(b, key);
                }
                if self.user_class(obj).is_some() {
                    return Err(self.need_py());
                }
                let msg = format!(
                    "'{}' object doesn't support item deletion",
                    self.type_name(obj)
                );
                Err(self.err(T::TypeError, &msg))
            }
        }
    }

    // ------------------------------------------------------------ range

    pub(crate) fn new_range(&mut self, start: i64, stop: i64, step: i64) -> Result<Value, Raised> {
        let r = {
            let res = self.heap.alloc_init(
                ty::RANGE,
                0,
                &[],
                &[
                    start.cast_unsigned(),
                    stop.cast_unsigned(),
                    step.cast_unsigned(),
                ],
            );
            self.hx(res)
        }?;
        Ok(Value::from_ref(r, ty::RANGE as u8))
    }

    /// `iter(r)`, or `reversed(r)` when `rev`: words `cur, left, step`, as
    /// CPython's, since a range can hold more than `i64::MAX` items and
    /// end at `i64::MIN`/`MAX`, where no stop past it fits.
    #[inline(never)]
    pub(crate) fn range_iter(&mut self, r: Value, rev: bool) -> Result<Value, Raised> {
        let (s, _, st) = self.range_parts(r);
        let n = self.range_len(r);
        let (cur, step) = if rev && n > 0 {
            // The last item, an item and so inside `i64`; `-st` wraps for
            // `i64::MIN`, which steps the same modulo 2^64.
            let last = i128::from(s) + i128::from(n - 1) * i128::from(st);
            (i64::try_from(last).unwrap_or(s), st.wrapping_neg())
        } else {
            (s, st)
        };
        self.new_iter(
            Value::NONE,
            it::RANGE,
            cur.cast_unsigned(),
            n,
            step.cast_unsigned(),
        )
    }

    /// `r[s]` for a slice `s`, in `i128`, as CPython's `compute_slice`: the
    /// slice's own stop, not one past its last item (`range(10)[::3]` keeps
    /// stop 10, not 12). Ends past `i64` are clamped, which keeps the items
    /// unless one is `i64::MIN`/`MAX`; that slice, like a step past `i64`,
    /// is the range's `OverflowError` (its parts are `i64`).
    #[inline(never)]
    fn range_slice(&mut self, r: Value, s: Value) -> Result<Value, Raised> {
        let (start, _, step) = self.range_parts(r);
        let n = i128::from(self.range_len(r));
        let mut part = [None; 3];
        for (i, p) in (0u32..).zip(part.iter_mut()) {
            let v = self.vslot(s, i);
            if v == Value::NONE {
                continue;
            }
            *p = Some(match self.int_of(v) {
                Some(x) => i128::from(x),
                None if Vm::is_heap_int(v) => {
                    let (neg, b) = self.heap_int_parts(v);
                    let mut w = [0u8; 16];
                    // Past 2^72 is past any end: a range has under 2^64 items.
                    let m = match w.get_mut(..b.len()).filter(|_| b.len() <= 9) {
                        Some(head) => {
                            head.copy_from_slice(b);
                            i128::from_le_bytes(w)
                        }
                        None => 1 << 72,
                    };
                    if neg { -m } else { m }
                }
                None if self.uflags(v) & f::INDEX != 0 => return Err(self.need_py()),
                None => i128::from(backed_slice_index(self, v)?),
            });
        }
        let st = part[2].unwrap_or(1);
        if st == 0 {
            return Err(self.err(T::ValueError, "slice step cannot be zero"));
        }
        let (lo, hi) = if st < 0 { (-1, n - 1) } else { (0, n) };
        let fix = |v: Option<i128>, default: i128| match v {
            None => default,
            Some(x) if x < 0 => (x + n).max(lo),
            Some(x) => x.min(hi),
        };
        let s0 = fix(part[0], if st < 0 { hi } else { lo });
        let s1 = fix(part[1], if st < 0 { lo } else { hi });
        let len = if st > 0 && s1 > s0 {
            (s1 - s0 - 1) / st + 1
        } else if st < 0 && s0 > s1 {
            (s0 - s1 - 1) / -st + 1
        } else {
            0
        };
        let at = |i: i128| {
            (i128::from(start) + i * i128::from(step)).clamp(i64::MIN.into(), i64::MAX.into())
                as i64
        };
        if let Some(new_step) = i128::from(step)
            .checked_mul(st)
            .and_then(|x| i64::try_from(x).ok())
        {
            let r = self.new_range(at(s0), at(s1), new_step)?;
            if i128::from(self.range_len(r)) == len {
                return Ok(r);
            }
        }
        Err(self.err(
            T::OverflowError,
            "Python int too large to convert to C ssize_t",
        ))
    }

    pub(crate) fn range_parts(&self, r: Value) -> (i64, i64, i64) {
        let Some(o) = r.as_obj() else {
            return (0, 0, 1);
        };
        (
            self.raw(o, 0).cast_signed(),
            self.raw(o, 1).cast_signed(),
            self.raw(o, 2).cast_signed(),
        )
    }

    pub(crate) fn range_len(&self, r: Value) -> u64 {
        let (a, b, s) = self.range_parts(r);
        let n = if s > 0 && b > a {
            (i128::from(b) - i128::from(a) - 1) / i128::from(s) + 1
        } else if s < 0 && a > b {
            (i128::from(a) - i128::from(b) - 1) / -i128::from(s) + 1
        } else {
            0
        };
        u64::try_from(n).unwrap_or(u64::MAX)
    }

    pub(crate) fn range_eq(&self, a: Value, b: Value) -> bool {
        let (la, lb) = (self.range_len(a), self.range_len(b));
        if la != lb {
            return false;
        }
        if la == 0 {
            return true;
        }
        let (sa, _, ta) = self.range_parts(a);
        let (sb, _, tb) = self.range_parts(b);
        sa == sb && (la == 1 || ta == tb)
    }

    // -------------------------------------------------------- iteration

    fn new_iter(&mut self, seq: Value, kind: u64, a: u64, b: u64, c: u64) -> Result<Value, Raised> {
        let r = {
            let res = self.heap.alloc_init(ty::ITER, 0, &[seq], &[kind, a, b, c]);
            self.hx(res)
        }?;
        Ok(Value::from_ref(r, ty::ITER as u8))
    }

    /// `iter(v)`.
    pub(crate) fn get_iter(&mut self, v: Value) -> Result<Value, Raised> {
        match self.type_of(v) {
            T::list => self.new_iter(v, it::LIST, 0, 0, 0),
            T::tuple => self.new_iter(v, it::TUPLE, 0, 0, 0),
            T::str => {
                let ascii = v
                    .as_obj()
                    .and_then(|r| self.heap.str_chars(r).ok())
                    .is_some_and(|n| n as usize == self.str_bytes(v).len());
                self.new_iter(v, if ascii { it::STR_ASCII } else { it::STR }, 0, 0, 0)
            }
            T::bytes => self.new_iter(v, it::BYTES, 0, 0, 0),
            T::bytearray => self.new_iter(v, it::BYTEARRAY, 0, 0, 0),
            T::range => self.range_iter(v, false),
            T::dict | T::dict_keys => {
                let d = if self.type_of(v) == T::dict {
                    v
                } else {
                    self.vslot(v, 0)
                };
                let n = self.dict_len(d) as u64;
                self.new_iter(d, it::DICT_KEYS, 0, n, 0)
            }
            T::dict_values => {
                let d = self.vslot(v, 0);
                let n = self.dict_len(d) as u64;
                self.new_iter(d, it::DICT_VALUES, 0, n, 0)
            }
            T::dict_items => {
                let d = self.vslot(v, 0);
                let n = self.dict_len(d) as u64;
                self.new_iter(d, it::DICT_ITEMS, 0, n, 0)
            }
            T::set | T::frozenset => {
                let n = self.set_len(v) as u64;
                self.new_iter(v, it::SET, 0, n, 0)
            }
            T::list_iterator
            | T::tuple_iterator
            | T::str_iterator
            | T::str_ascii_iterator
            | T::bytes_iterator
            | T::bytearray_iterator
            | T::range_iterator
            | T::dict_keyiterator
            | T::dict_valueiterator
            | T::dict_itemiterator
            | T::set_iterator
            | T::enumerate
            | T::zip
            | T::list_reverseiterator
            | T::generator
            | T::callable_iterator => Ok(v),
            _ => {
                let fl = self.uflags(v);
                if fl & f::ITER != 0 || fl & f::GETITEM != 0 && self.container_backing(v).is_none()
                {
                    return Err(self.need_py());
                }
                if let Some(b) = self.backing(v) {
                    return self.get_iter(b);
                }
                let msg = format!("'{}' object is not iterable", self.type_name(v));
                Err(self.err(T::TypeError, &msg))
            }
        }
    }

    /// An exhausted builtin iterator `r` drops its sequence, as CPython's
    /// do: it holds no memory, and one that grows later is not read again.
    fn iter_drop(&mut self, r: Ref) {
        self.set_slot_o(r, 0, Value::NONE);
    }

    /// `next(it)`: `None` when exhausted.
    pub(crate) fn iter_next(&mut self, itv: Value) -> Result<Option<Value>, Raised> {
        if itv.is_obj_kind(ty::ITER as u8) {
            let Some(r) = itv.as_obj() else {
                return Ok(None);
            };
            // Iterator slots: [seq; kind, a, b, c].
            let Ok(&[seq, kind, a, ..]) = self.heap.slot_words(r) else {
                return Ok(None);
            };
            let seq = Value::from_bits(seq).unwrap_or(Value::NONE);
            match kind {
                it::LIST => {
                    let (next, item) = match self.list_parts(seq) {
                        Some((buf, n)) if a < n => {
                            let item = self
                                .heap
                                .slot_words(buf)
                                .ok()
                                .and_then(|w| w.get(usize::try_from(a).ok()?))
                                .and_then(|&b| Value::from_bits(b));
                            (a + 1, Some(item.unwrap_or(Value::UNDEFINED)))
                        }
                        _ => (u64::MAX >> 1, None),
                    };
                    if let Ok([_, pos, ..]) = self.heap.raw_words_mut(r) {
                        *pos = next;
                    }
                    if item.is_none() {
                        self.iter_drop(r);
                    }
                    return Ok(item);
                }
                it::TUPLE => {
                    if (a as usize) < self.tuple_len(seq) {
                        self.set_raw(r, 2, a + 1);
                        return Ok(Some(self.vslot(seq, u32::try_from(a).unwrap_or(0))));
                    }
                    self.iter_drop(r);
                    return Ok(None);
                }
                it::BYTES | it::BYTEARRAY => {
                    let data = self.bytes_data(seq);
                    let i = usize::try_from(a).unwrap_or(usize::MAX);
                    let Some(&c) = data.get(i) else {
                        // A bytearray that grows later is not read again.
                        self.iter_drop(r);
                        return Ok(None);
                    };
                    self.set_raw(r, 2, a + 1);
                    return Ok(Some(Value::int(i64::from(c)).unwrap_or(Value::NONE)));
                }
                it::STR | it::STR_ASCII => {
                    let bytes = self.str_bytes(seq);
                    let i = usize::try_from(a).unwrap_or(usize::MAX);
                    if i >= bytes.len() {
                        self.iter_drop(r);
                        return Ok(None);
                    }
                    let lead = bytes[i];
                    let w = if lead < 0x80 {
                        1
                    } else if lead < 0xE0 {
                        2
                    } else if lead < 0xF0 {
                        3
                    } else {
                        4
                    };
                    let s: String = core::str::from_utf8(bytes.get(i..i + w).unwrap_or(&[]))
                        .unwrap_or("")
                        .into();
                    self.set_raw(r, 2, (i + w) as u64);
                    return Ok(Some(self.new_str(&s)?));
                }
                it::RANGE => {
                    let left = self.raw(r, 3);
                    if left == 0 {
                        return Ok(None);
                    }
                    // Wraps only past the last item, which is never read.
                    let next = a.wrapping_add(self.raw(r, 4));
                    self.set_raw(r, 2, next);
                    self.set_raw(r, 3, left - 1);
                    return Ok(Some(self.new_int(a.cast_signed())?));
                }
                it::DICT_KEYS | it::DICT_VALUES | it::DICT_ITEMS => {
                    if a == DEAD {
                        return Ok(None);
                    }
                    let expect = self.raw(r, 3);
                    if self.dict_len(seq) as u64 != expect {
                        // Sticky, as in CPython.
                        self.set_raw(r, 3, u64::MAX);
                        return Err(
                            self.err(T::RuntimeError, "dictionary changed size during iteration")
                        );
                    }
                    let used = self.dict_used(seq);
                    let mut i = usize::try_from(a).unwrap_or(usize::MAX);
                    while i < used {
                        if let Some((k, v)) = self.dict_entry(seq, i) {
                            // Deleted entries passed over, as byte work.
                            if i - a as usize >= 64 {
                                self.burn((i - a as usize) as u64 / 8);
                            }
                            // CPython's second guard: a live entry after all
                            // `len` were handed out means keys were replaced.
                            let yielded = self.raw(r, 4);
                            if yielded >= expect {
                                self.set_raw(r, 2, DEAD);
                                return Err(self.err(
                                    T::RuntimeError,
                                    "dictionary keys changed during iteration",
                                ));
                            }
                            self.set_raw(r, 4, yielded + 1);
                            self.set_raw(r, 2, (i + 1) as u64);
                            return Ok(Some(match kind {
                                it::DICT_KEYS => k,
                                it::DICT_VALUES => v,
                                _ => self.alloc_tuple(&[k, v])?,
                            }));
                        }
                        i += 1;
                    }
                    self.burn((i.saturating_sub(a as usize)) as u64 / 8);
                    // Exhausted iterators stay exhausted.
                    self.set_raw(r, 2, DEAD);
                    self.iter_drop(r);
                    return Ok(None);
                }
                _ => {
                    if a == DEAD {
                        return Ok(None);
                    }
                    let expect = self.raw(r, 3);
                    if self.set_len(seq) as u64 != expect {
                        self.set_raw(r, 3, u64::MAX);
                        return Err(self.err(T::RuntimeError, "Set changed size during iteration"));
                    }
                    // Empty slots passed over, as byte work, scanned in
                    // one pass: a long empty tail is read in a few ms.
                    let (i, v) = self.set_next_at(seq, usize::try_from(a).unwrap_or(usize::MAX));
                    let i = i as u64;
                    if let Some(v) = v {
                        if i + 1 - a >= 64 {
                            self.burn((i + 1 - a) / 8);
                        }
                        self.set_raw(r, 2, i + 1);
                        return Ok(Some(v));
                    }
                    self.burn(i.saturating_sub(a) / 8);
                    self.set_raw(r, 2, DEAD);
                    self.iter_drop(r);
                    return Ok(None);
                }
            }
        }
        match self.type_of(itv) {
            T::enumerate => {
                let Some(r) = itv.as_obj() else {
                    return Ok(None);
                };
                let inner = self.slot(r, 0);
                if Self::nests(inner) {
                    return self.iter_next_nested(itv);
                }
                let Some(x) = self.iter_next(inner)? else {
                    return Ok(None);
                };
                let n = self.raw(r, 2).cast_signed();
                let iv = if n == i64::MAX {
                    self.enum_big(r)?
                } else {
                    self.set_raw(r, 2, (n + 1).cast_unsigned());
                    self.new_int(n)?
                };
                Ok(Some(self.alloc_tuple(&[iv, x])?))
            }
            T::zip => {
                let iters = self.tuple_items(self.vslot(itv, 0));
                if iters.is_empty() {
                    return Ok(None);
                }
                if iters.iter().any(|&i| Self::nests(i)) {
                    return self.iter_next_nested(itv);
                }
                let mut out = Vec::with_capacity(iters.len());
                for i in iters {
                    match self.iter_next(i)? {
                        Some(x) => out.push(x),
                        None => return Ok(None),
                    }
                }
                Ok(Some(self.alloc_tuple(&out)?))
            }
            T::list_reverseiterator => {
                let Some(r) = itv.as_obj() else {
                    return Ok(None);
                };
                let seq = self.slot(r, 0);
                let i = self.raw(r, 1).cast_signed();
                let n = self.seq_words(seq).len();
                if i < 0 || usize::try_from(i).unwrap_or(usize::MAX) >= n {
                    self.iter_drop(r);
                    return Ok(None);
                }
                self.set_raw(r, 1, (i - 1).cast_unsigned());
                let k = usize::try_from(i).unwrap_or(0);
                Ok(Some(if seq.is_obj_kind(ty::LIST as u8) {
                    self.list_get(seq, k)
                } else {
                    self.vslot(seq, u32::try_from(k).unwrap_or(0))
                }))
            }
            _ => {
                if matches!(itv.obj_kind().map(u16::from), Some(ty::GEN | ty::CALLITER))
                    || self.uflags(itv) & f::NEXT != 0
                {
                    return Err(self.need_py());
                }
                let msg = format!("'{}' object is not an iterator", self.type_name(itv));
                Err(self.err(T::TypeError, &msg))
            }
        }
    }

    /// The count of enumerate `r` once its `i64` count (raw word 2) has
    /// reached `i64::MAX`: the int in value slot 1 (`None` until then, or
    /// a `start` past `i64`), which then goes up by one, as CPython's
    /// `en_longindex`. Raw word 2 stays at `i64::MAX`.
    #[cold]
    #[inline(never)]
    pub(crate) fn enum_big(&mut self, r: Ref) -> Result<Value, Raised> {
        let n = match self.slot(r, 1) {
            Value::NONE => self.new_int(i64::MAX)?,
            n => n,
        };
        let b = self.big_of(n).unwrap_or_else(Big::zero);
        let next = self.new_big(&b.add(&Big::from_i64(1)))?;
        self.set_slot(r, 1, next);
        Ok(n)
    }

    /// Is `it` an `enumerate` or `zip`, whose `next()` reads other
    /// iterators?
    fn nests(it: Value) -> bool {
        matches!(it.obj_kind().map(u16::from), Some(ty::ENUMERATE | ty::ZIP))
    }

    /// `next()` of an `enumerate` or `zip` that reads another of them
    /// (`e = enumerate(e)` in a loop), with the pending ones on an
    /// explicit stack instead of the Rust stack (docs/spec.md §5). Their
    /// innermost iterators are builtin ones that read no others. A visit
    /// of an `enumerate` or `zip` is a fuel unit.
    #[cold]
    #[inline(never)]
    fn iter_next_nested(&mut self, root: Value) -> Result<Option<Value>, Raised> {
        let mut visits = 0;
        let r = self.iter_walk(root, &mut visits);
        self.burn(visits);
        r
    }

    fn iter_walk(&mut self, root: Value, visits: &mut u64) -> Result<Option<Value>, Raised> {
        // Each pending `enumerate` or `zip`; for a zip, the iterator it
        // waits for and where its items so far start in `items`.
        let mut stack: NativeVec<(Value, u32, u32)> = NativeVec::default();
        let mut items = NativeValues::default();
        let mut pull = Some(root);
        let mut got = None;
        loop {
            if let Some(it) = pull.take() {
                if !Self::nests(it) {
                    got = self.iter_next(it)?;
                } else if stack.len() >= 100_000 {
                    // json's nesting limit. CPython 3.14 recurses in C here
                    // and crashes between 100,000 and 200,000 levels [M].
                    return Err(self.err(T::RecursionError, "maximum recursion depth exceeded"));
                } else {
                    *visits += 1;
                    let inner = self.vslot(it, 0);
                    let first = if it.is_obj_kind(ty::ZIP as u8) {
                        (self.tuple_len(inner) > 0).then(|| self.vslot(inner, 0))
                    } else {
                        Some(inner)
                    };
                    if let Some(first) = first {
                        let start = u32::try_from(items.len()).unwrap_or(u32::MAX);
                        let res = stack.push(&mut self.heap, (it, 0, start));
                        self.hx(res)?;
                        pull = Some(first);
                        continue;
                    }
                    // `zip()` of nothing.
                    got = None;
                }
            }
            // `got` is the item of the iterator the top one waits for.
            let Some(&(it, i, start)) = stack.last() else {
                return Ok(got);
            };
            if it.is_obj_kind(ty::ZIP as u8) {
                let iters = self.vslot(it, 0);
                let Some(x) = got else {
                    items.truncate(start as usize);
                    stack.pop();
                    continue;
                };
                let res = items.push(&mut self.heap, x);
                self.hx(res)?;
                if (i as usize) + 1 < self.tuple_len(iters) {
                    if let Some(top) = stack.last_mut() {
                        top.1 = i + 1;
                    }
                    pull = Some(self.vslot(iters, i + 1));
                    continue;
                }
                got = Some(self.alloc_tuple(items.get(start as usize..).unwrap_or(&[]))?);
                items.truncate(start as usize);
            } else if let (Some(x), Some(r)) = (got, it.as_obj()) {
                let n = self.raw(r, 2).cast_signed();
                let iv = if n == i64::MAX {
                    self.enum_big(r)?
                } else {
                    self.set_raw(r, 2, (n + 1).cast_unsigned());
                    self.new_int(n)?
                };
                got = Some(self.alloc_tuple(&[iv, x])?);
            }
            stack.pop();
        }
    }

    /// Every item of an iterable.
    pub(crate) fn collect(&mut self, v: Value) -> Result<NativeValues, Raised> {
        /// Items per fuel charge; capacity is admitted before every growth.
        const CHUNK: usize = 1024;
        let seq = if v.is_obj_kind(ty::TUPLE as u8) {
            v.as_obj().map(|r| (r, usize::MAX))
        } else {
            self.list_parts(v).map(|(r, n)| (r, n as usize))
        };
        if let Some((r, n)) = seq {
            let res = NativeValues::from_heap(&mut self.heap, r, n);
            let items = self.hx(res)?;
            // One unit per item, as for an iterator: the callers' loops
            // over the items are paid for here (docs/spec.md §5).
            if items.len() >= 8 {
                self.burn(items.len() as u64);
            }
            return Ok(items);
        }
        let itv = self.get_iter(v)?;
        if itv == v {
            // A one-shot iterator: what it yields is gone, so an
            // instruction or call failing while or after gathering it does
            // not run again after a collection (`Vm::oom_retry_if`): it
            // would gather what is left (`l[:] = zip(a, b)` emptied `l`).
            self.oom_rerun = self.op_stamp();
        }
        // A range knows its length: refuse what cannot fit up front.
        let left = self.range_left(itv);
        if let Some(n) = left
            && (n > 1 << 26 || !self.heap.would_fit(n * 8))
        {
            return Err(self.err(T::MemoryError, ""));
        }
        // An iterator need not end, and a container's (a str's chars, a
        // set's items) or a range's items can be many (8M of a range took
        // 155 ms in one step [M]), so items are gathered a chunk at a time,
        // the rest in the prelude, which parks between chunks (docs/spec.md
        // §5). Only an iterator that runs no Python code fails part way: a
        // container's never does.
        let endless = itv == v && !itv.is_obj_kind(ty::ITER as u8);
        let mut out = NativeValues::default();
        loop {
            let x = match self.iter_next(itv) {
                Ok(Some(x)) => x,
                Ok(None) => break,
                Err(e) => {
                    if self.slow && endless && !out.is_empty() {
                        // An item needs Python code: keep what came before.
                        self.burn((out.len() % CHUNK) as u64);
                        self.partial = Some((v, itv, out));
                    }
                    return Err(e);
                }
            };
            let res = out.push(&mut self.heap, x);
            self.hx(res)?;
            if out.len().is_multiple_of(CHUNK) {
                self.burn(CHUNK as u64);
                if out.len() >= COLLECT_CHUNK {
                    self.partial = Some((v, itv, out));
                    return Err(self.need_py());
                }
            }
        }
        self.burn((out.len() % CHUNK) as u64);
        if itv == v {
            self.fresh_consumed();
        }
        if itv == v && self.consumed.len() < 4 {
            // A one-shot iterator: remembered in case the caller hands off
            // to Python code that must see the items again.
            if self.consumed.capacity() < 4 {
                self.reserve_execution((4 * size_of::<(Value, NativeValues)>()) as u64)?;
                let res = self.consumed.try_reserve_exact(4 - self.consumed.len());
                let _ = self.reserve_execution(0);
                self.hx(res.map_err(|_| mm_core::Error::OutOfMemory))?;
            }
            let res = out.try_clone(&mut self.heap);
            let remembered = self.hx(res)?;
            self.consumed.push((v, remembered));
        }
        Ok(out)
    }

    /// Items left in range iterator `itv` (`None` for other iterators).
    fn range_left(&self, itv: Value) -> Option<u64> {
        let o = itv.as_obj().filter(|_| itv.is_obj_kind(ty::ITER as u8))?;
        if self.raw(o, 1) != it::RANGE {
            return None;
        }
        Some(self.raw(o, 3))
    }

    /// The sum of the items left in range iterator `itv`, in closed form
    /// (`None` for other iterators or past `i128`).
    pub(crate) fn range_sum(&self, itv: Value) -> Option<i128> {
        let n = i128::from(self.range_left(itv)?);
        let o = itv.as_obj()?;
        let [first, step] = [2, 4].map(|i| i128::from(self.raw(o, i).cast_signed()));
        if n == 0 {
            return Some(0);
        }
        let last = first.checked_add((n - 1).checked_mul(step)?)?;
        // n * (first + last) is even: either n or (first + last) is.
        let pair = first.checked_add(last)?;
        if n % 2 == 0 {
            (n / 2).checked_mul(pair)
        } else {
            n.checked_mul(pair / 2)
        }
    }

    /// The arguments of a prelude call that goes on from a native
    /// operation: if `collect` stopped part way through one of them
    /// (`Vm::partial`), that argument becomes a list of the items it took
    /// and the call becomes `_collect_rest(items, it, f, args, kw)`, which
    /// gathers the rest a chunk per safepoint and then calls `f` with the
    /// complete list. `kw` is a dict of keyword arguments for `f`.
    pub(crate) fn slow_call(
        &mut self,
        f: Value,
        mut args: Vec<Value>,
        kw: Value,
    ) -> Result<(Value, Vec<Value>, Value), Raised> {
        let Some((src, it, items)) = self.partial.take() else {
            return Ok((f, args, kw));
        };
        let Some(slot) = args.iter_mut().find(|a| **a == src) else {
            return Ok((f, args, kw));
        };
        // A container's or a range's items (`it` is not the argument, or a
        // range iterator) all go into the list: room for them up front, so
        // it does not double, copying and leaving garbage, while
        // `_collect_rest` fills it.
        let room = if let Some(n) = self.range_left(it) {
            items
                .len()
                .saturating_add(usize::try_from(n).unwrap_or(usize::MAX))
        } else if it == src {
            0
        } else if Self::is_str(src) {
            self.str_len(src)
        } else if Self::is_bytes(src) || src.is_obj_kind(ty::BYTEARRAY as u8) {
            self.bytes_data(src).len()
        } else if src.is_obj_kind(ty::DICT as u8) {
            self.dict_len(src)
        } else if matches!(self.type_of(src), T::set | T::frozenset) {
            self.set_len(src)
        } else {
            0
        };
        let xs = if room > items.len() {
            let room = room.min(mm_core::MAX_OBJECT_BYTES as usize / 8);
            let xs = self.list_with_room(0, room)?;
            for &x in &items {
                self.list_push(xs, x)?;
            }
            xs
        } else {
            self.new_list(&items)?
        };
        *slot = xs;
        let t = self.alloc_tuple(&args)?;
        let kw = if kw == Value::NONE {
            self.new_dict()?
        } else {
            kw
        };
        let rest = self.prelude_fn("_collect_rest")?;
        Ok((rest, alloc::vec![xs, it, f, t, kw], Value::NONE))
    }

    /// `v`, or a list of its items if the running builtin consumed it.
    pub(crate) fn unconsume(&mut self, v: Value) -> Result<Value, Raised> {
        self.fresh_consumed();
        match self.consumed.iter().position(|(k, _)| *k == v) {
            Some(i) => {
                let (_, items) = self.consumed.swap_remove(i);
                self.new_list(&items)
            }
            None => Ok(v),
        }
    }

    /// Unpacks `v` into exactly `n` values.
    pub(crate) fn unpack(&mut self, v: Value, n: usize) -> Result<Vec<Value>, Raised> {
        let items = if let Some(items) = self.seq_items(v) {
            if items.len() > n {
                // The copy, charged as list building is: one unit per 8.
                self.burn(items.len() as u64 / 8);
                return Err(self.err(
                    T::ValueError,
                    &format!(
                        "too many values to unpack (expected {n}, got {})",
                        items.len()
                    ),
                ));
            }
            items
        } else {
            if v.is_obj_kind(ty::DICT as u8) && self.dict_len(v) > n {
                // CPython 3.14 sizes an exact dict up front, like a list.
                let msg = format!(
                    "too many values to unpack (expected {n}, got {})",
                    self.dict_len(v)
                );
                return Err(self.err(T::ValueError, &msg));
            }
            let itv = match self.get_iter(v) {
                Ok(i) => i,
                Err(Raised) if self.slow => return Err(Raised),
                Err(Raised) => {
                    let msg = format!("cannot unpack non-iterable {} object", self.type_name(v));
                    return Err(self.err(T::TypeError, &msg));
                }
            };
            let mut out = Vec::with_capacity(n);
            while let Some(x) = self.iter_next(itv)? {
                if out.len() == n {
                    return Err(self.err(
                        T::ValueError,
                        &format!("too many values to unpack (expected {n})"),
                    ));
                }
                out.push(x);
            }
            out
        };
        if items.len() < n {
            return Err(self.err(
                T::ValueError,
                &format!(
                    "not enough values to unpack (expected {n}, got {})",
                    items.len()
                ),
            ));
        }
        Ok(items)
    }

    // ------------------------------------------------------- attributes

    /// `obj.name` for data attributes and bound methods.
    pub(crate) fn getattr(&mut self, obj: Value, name: Value) -> Result<Value, Raised> {
        if let Some(v) = self.user_getattr(obj, name)? {
            return Ok(v);
        }
        if obj.is_obj_kind(ty::MODULE as u8) {
            if let Some(v) = self.module_attr(obj, name)? {
                return Ok(v);
            }
            // PEP 562: the module's own `__getattr__(name)`.
            let k = self.dn(Dn::Getattr)?;
            let d = self.vslot(obj, crate::modules::module_slot::DICT);
            if let Some(f) = self.dict_get_str(d, k)
                && f.is_obj_kind(ty::FUNC as u8)
            {
                let n = crate::cont::Next::Invoke(
                    f,
                    crate::cont::Args::of(&[name]),
                    Value::NONE,
                    crate::cont::Cont::None,
                );
                return self.next_request(n);
            }
            let msg = format!(
                "module '{}' has no attribute '{}'",
                self.module_name(obj),
                self.str_ref(name)
            );
            let e = self.err(T::AttributeError, &msg);
            self.module_attr_hint(obj, name, false);
            return Err(e);
        }
        if obj.is_obj_kind(ty::FUNC as u8) {
            let d = self.vslot(obj, fn_slot::DICT);
            if d != Value::NONE
                && let Some(v) = self.dict_get(d, name)?
            {
                return Ok(v);
            }
        }
        let n_owned = String::from(self.str_ref(name));
        let n = n_owned.as_str();
        let t = self.type_of(obj);
        let data = match (t, n) {
            (T::function, "__name__") => Some(self.vslot(obj, fn_slot::NAME)),
            (T::function, "__qualname__") => Some(self.vslot(obj, fn_slot::QUALNAME)),
            (T::function, "__defaults__") => Some(self.vslot(obj, fn_slot::DEFAULTS)),
            (T::function, "__kwdefaults__") if obj.is_obj_kind(ty::FUNC as u8) => {
                Some(self.vslot(obj, fn_slot::KWDEFAULTS))
            }
            (T::function, "__dict__") if obj.is_obj_kind(ty::FUNC as u8) => {
                Some(self.func_dict(obj)?)
            }
            (T::function, "__module__") if obj.is_obj_kind(ty::HOSTFN as u8) => {
                let m = self.hostfn_module(obj);
                Some(self.new_str(&m)?)
            }
            (T::function, "__doc__") if obj.is_obj_kind(ty::HOSTFN as u8) => {
                Some(match self.hostfn_help(obj, 1) {
                    Some(d) => self.new_str(&d)?,
                    None => Value::NONE,
                })
            }
            (T::function, "__module__") => {
                let g = if obj.is_obj_kind(ty::FUNC as u8) {
                    self.vslot(obj, fn_slot::GLOBALS)
                } else {
                    Value::NONE
                };
                let k = self.intern("__name__")?;
                match g {
                    g if g.is_obj_kind(ty::DICT as u8) => self.dict_get(g, k)?,
                    _ => None,
                }
                .map_or(Ok(Value::NONE), Ok)
                .map(Some)?
            }
            (T::function, "__doc__") => {
                let cidx = u32::try_from(self.vslot_raw_of(obj, fn_slot::CODE)).unwrap_or(0);
                let doc: Option<String> = self
                    .codes
                    .get(cidx as usize)
                    .and_then(|c| c.code.doc.clone());
                Some(match doc {
                    Some(d) => self.new_str(&d)?,
                    None => Value::NONE,
                })
            }
            (T::type_, "__name__" | "__qualname__") => Some(self.vslot(obj, 0)),
            (T::builtin, "__self__") if obj.as_native().is_none() => Some(self.vslot(obj, 0)),
            (T::builtin, "__name__" | "__qualname__") => {
                let f = if obj.as_native().is_some() {
                    obj
                } else {
                    self.vslot(obj, 1)
                };
                let nm = f
                    .as_native()
                    .and_then(crate::builtins::native)
                    .map_or("?", |x| x.name());
                Some(self.intern(nm)?)
            }
            (_, "__class__") => Some(self.type_obj(t)),
            (T::range | T::slice, "start" | "stop" | "step") => {
                let i = match n {
                    "start" => 0,
                    "stop" => 1,
                    _ => 2,
                };
                if t == T::range {
                    let (a, b, c) = self.range_parts(obj);
                    Some(self.new_int([a, b, c][i])?)
                } else {
                    Some(self.vslot(obj, u32::try_from(i).unwrap_or(0)))
                }
            }
            (T::int | T::bool, "real" | "numerator") => {
                if t == T::bool {
                    Some(self.new_int(self.int_of(obj).unwrap_or(0))?)
                } else {
                    Some(obj)
                }
            }
            (T::int | T::bool, "imag") => Value::int(0),
            (T::int | T::bool, "denominator") => Value::int(1),
            (T::float, "real") => Some(obj),
            (T::float, "imag") => Some(Value::float(0.0)),
            (T::complex, "real" | "imag") => {
                let (re, im) = self.complex_of(obj).unwrap_or((0.0, 0.0));
                Some(Value::float(if n == "real" { re } else { im }))
            }
            _ if obj.is_obj_kind(ty::EXC as u8) => self.exc_attr(obj, name)?,
            (T::traceback, _) => self.traceback_attr(obj, n)?,
            #[cfg(feature = "re")]
            (T::Match, _) => crate::re::match_attr(self, obj, n)?,
            (T::TextIOWrapper, "name") => {
                let err = self.stream_of(obj) == Some(crate::vm::Stream::Stderr);
                Some(self.new_str(if err { "<stderr>" } else { "<stdout>" })?)
            }
            _ => None,
        };
        if let Some(v) = data {
            return Ok(v);
        }
        // The owner's table, then its bases' (`ValueError.__init__` is
        // `BaseException.__init__`).
        let mut owner = self.method_owner(obj);
        let mut found = crate::builtins::find_method(owner, n);
        while found.is_none() && owner != T::object {
            owner = owner.base();
            found = crate::builtins::find_method(owner, n);
        }
        if let Some(f) = found {
            if crate::builtins::native(f).is_some_and(crate::builtins::is_classmethod) {
                let cls = if t == T::type_ { obj } else { self.type_obj(t) };
                let r = {
                    let res = self
                        .heap
                        .alloc_init(ty::BOUND, 0, &[cls, Value::native(f)], &[]);
                    self.hx(res)
                }?;
                return Ok(Value::from_ref(r, ty::BOUND as u8));
            }
            if t == T::type_ {
                // `str.upper`: the unbound method descriptor.
                return Ok(Value::native(f));
            }
            let r = {
                let res = self
                    .heap
                    .alloc_init(ty::BOUND, 0, &[obj, Value::native(f)], &[]);
                self.hx(res)
            }?;
            return Ok(Value::from_ref(r, ty::BOUND as u8));
        }
        Err(self.attr_error(obj, name))
    }

    /// The type whose method table `obj.name` searches.
    pub(crate) fn method_owner(&self, obj: Value) -> T {
        let t = self.type_of(obj);
        if t == T::type_ {
            return self.type_of_typeobj(obj).unwrap_or(T::object);
        }
        if t.is_exception() {
            return T::BaseException;
        }
        t
    }

    /// `del obj.name` (a call request may be left).
    pub(crate) fn delattr(&mut self, obj: Value, name: Value) -> Result<(), Raised> {
        if self.user_class(obj).is_some() {
            return self.inst_delattr(obj, name);
        }
        if Vm::is_class(obj) {
            if self.meta_hook(obj, "__delattr__", f::DELATTR, vec![name])? {
                return Ok(());
            }
            return self.class_delattr(obj, name);
        }
        if obj.is_obj_kind(ty::EXC as u8) && self.inst_del(obj, name)? {
            return Ok(());
        }
        if self.type_of_typeobj(obj).is_some() {
            // The same TypeError as setting it.
            return self.setattr(obj, name, Value::NONE);
        }
        Err(self.attr_error(obj, name))
    }

    pub(crate) fn attr_error(&mut self, obj: Value, name: Value) -> Raised {
        let n = String::from(self.str_ref(name));
        let msg = if Vm::is_class(obj) {
            // A class standing in for a C type names itself as CPython's
            // `tp_name` does (its own `__mm_tpname__`).
            let own = self
                .find_interned("__mm_tpname__")
                .and_then(|k| self.dict_get_str(self.vslot(obj, crate::class::cls::DICT), k))
                .filter(|v| Vm::is_str(*v));
            let cname = match own {
                Some(v) => self.str_ref(v),
                None => self.class_name(obj),
            };
            format!("type object '{cname}' has no attribute '{n}'")
        } else if let Some(t) = self.type_of_typeobj(obj) {
            format!("type object '{}' has no attribute '{n}'", t.name())
        } else {
            format!("'{}' object has no attribute '{n}'", self.type_name(obj))
        };
        let e = self.err(T::AttributeError, &msg);
        self.set_attr_hint(obj, name);
        e
    }

    /// `obj.name = v` (a call request may be left: a property setter,
    /// `__setattr__`).
    pub(crate) fn setattr(&mut self, obj: Value, name: Value, v: Value) -> Result<(), Raised> {
        if obj.is_obj_kind(ty::MODULE as u8) {
            return self.module_setattr(obj, name, v);
        }
        if self.user_class(obj).is_some() {
            return self.inst_setattr(obj, name, v).map(|_| ());
        }
        if Vm::is_class(obj) {
            if self.meta_hook(obj, "__setattr__", f::SETATTR, vec![name, v])? {
                return Ok(());
            }
            return self.class_setattr(obj, name, v);
        }
        if obj.is_obj_kind(ty::EXC as u8) && self.exc_set_special(obj, name, v)? {
            return Ok(());
        }
        if obj.is_obj_kind(ty::EXC as u8) {
            return self.inst_set(obj, name, v).map(|_| ());
        }
        let n = String::from(self.str_ref(name));
        if obj.is_obj_kind(ty::EXC as u8) {
            match n.as_str() {
                "__cause__" => {
                    if let Some(r) = obj.as_obj() {
                        self.set_slot(r, exc_slot::CAUSE, v);
                        let f = self.raw(r, exc_slot::FLAGS) | 1;
                        self.set_raw(r, exc_slot::FLAGS, f);
                    }
                    return Ok(());
                }
                "__context__" => {
                    if let Some(r) = obj.as_obj() {
                        self.set_slot(r, exc_slot::CONTEXT, v);
                    }
                    return Ok(());
                }
                _ => {}
            }
        }
        let mut kind = T::AttributeError;
        let msg = if let Some(t) = self.type_of_typeobj(obj) {
            kind = T::TypeError;
            format!(
                "cannot set '{n}' attribute of immutable type '{}'",
                t.name()
            )
        } else if obj.is_obj_kind(ty::FUNC as u8) {
            return self.func_setattr(obj, &n, name, v);
        } else if self.type_of(obj) == T::function {
            format!("micro-monty does not support setting function attributes ('{n}')")
        } else if Vm::is_complex(obj) && matches!(n.as_str(), "real" | "imag") {
            String::from("readonly attribute")
        } else if let (T::int | T::bool | T::float, "real" | "imag")
        | (T::int | T::bool, "numerator" | "denominator") =
            (self.type_of(obj), n.as_str())
        {
            let owner = if obj.is_float() { "float" } else { "int" };
            format!("attribute '{n}' of '{owner}' objects is not writable")
        } else {
            format!(
                "'{}' object has no attribute '{n}' and no __dict__ for setting new attributes",
                self.type_name(obj)
            )
        };
        Err(self.err(kind, &msg))
    }
}

impl Vm {
    /// A function's attribute dict, made on first use.
    pub(crate) fn func_dict(&mut self, f: Value) -> Result<Value, Raised> {
        let d = self.vslot(f, fn_slot::DICT);
        if d != Value::NONE {
            return Ok(d);
        }
        let d = self.new_dict()?;
        if let Some(r) = f.as_obj() {
            self.set_slot(r, fn_slot::DICT, d);
        }
        Ok(d)
    }

    /// `f.name = v` for a Python function.
    fn func_setattr(&mut self, f: Value, n: &str, name: Value, v: Value) -> Result<(), Raised> {
        let Some(r) = f.as_obj() else {
            return Ok(());
        };
        let slot = match n {
            "__name__" | "__qualname__" => {
                if !Vm::is_str(v) {
                    let msg = format!("{n} must be set to a string object");
                    return Err(self.err(T::TypeError, &msg));
                }
                Some(if n == "__name__" {
                    fn_slot::NAME
                } else {
                    fn_slot::QUALNAME
                })
            }
            "__defaults__" => {
                if v != Value::NONE && !v.is_obj_kind(ty::TUPLE as u8) {
                    return Err(
                        self.err(T::TypeError, "__defaults__ must be set to a tuple object")
                    );
                }
                Some(fn_slot::DEFAULTS)
            }
            "__kwdefaults__" => {
                if v != Value::NONE && !v.is_obj_kind(ty::DICT as u8) {
                    return Err(
                        self.err(T::TypeError, "__kwdefaults__ must be set to a dict object")
                    );
                }
                Some(fn_slot::KWDEFAULTS)
            }
            "__dict__" => {
                if !v.is_obj_kind(ty::DICT as u8) {
                    return Err(self.err(T::TypeError, "__dict__ must be set to a dictionary"));
                }
                Some(fn_slot::DICT)
            }
            _ => None,
        };
        if let Some(i) = slot {
            self.set_slot(r, i, v);
            return Ok(());
        }
        let d = self.func_dict(f)?;
        self.dict_set(d, name, v)
    }
}

/// Python's float `divmod`.
pub(crate) fn float_divmod(vx: f64, wx: f64) -> (f64, f64) {
    let mut m = vx % wx;
    let mut div = (vx - m) / wx;
    if m == 0.0 {
        m = 0.0_f64.copysign(wx);
    } else if (wx < 0.0) != (m < 0.0) {
        m += wx;
        div -= 1.0;
    }
    let floordiv = if div == 0.0 {
        0.0_f64.copysign(vx / wx)
    } else {
        let f = floor(div);
        if div - f > 0.5 { f + 1.0 } else { f }
    };
    (floordiv, m)
}

/// Exact comparison of an int and a float.
pub(crate) fn cmp_int_float(i: i64, f: f64) -> Option<Ordering> {
    if f.is_nan() {
        return None;
    }
    if f >= 9.223_372_036_854_776e18 {
        return Some(Ordering::Less);
    }
    if f < -9.223_372_036_854_776e18 {
        return Some(Ordering::Greater);
    }
    let fl = floor(f);
    #[allow(clippy::cast_possible_truncation, reason = "in range")]
    let fi = fl as i64;
    match i.cmp(&fi) {
        Ordering::Equal => {
            if f > fl {
                Some(Ordering::Less)
            } else {
                Some(Ordering::Equal)
            }
        }
        o => Some(o),
    }
}

/// A slice bound that is an int subclass (IntEnum) without its own
/// `__index__`: its value; anything else is the TypeError.
#[cold]
#[inline(never)]
fn backed_slice_index(vm: &mut Vm, v: Value) -> Result<i64, Raised> {
    match vm.backing(v).and_then(|b| vm.int_of(b)) {
        Some(x) => Ok(x),
        None => Err(vm.err(
            T::TypeError,
            "slice indices must be integers or None or have an __index__ method",
        )),
    }
}

/// The end of the char whose UTF-8 starts at byte `at` of `b`.
fn char_end(b: &[u8], at: usize) -> usize {
    (at + 1..b.len())
        .find(|&e| b.get(e).is_some_and(|x| x & 0xC0 != 0x80))
        .unwrap_or(b.len())
}

/// Lists and tuples at least this long are built a slice per step
/// ([`Vm::seq_build`]); shorter ones at once.
pub(crate) const SEQ_LONG: usize = 1 << 15;

/// A segment of a [`Vm::seq_build`]: `count` items of register `src`
/// from `start`, `step` apart, `reps` times; with `skip`, the items of
/// `start..` with every `step`-th from `start` left out (`count` kept).
#[derive(Clone, Copy, Default)]
pub(crate) struct Seg {
    pub(crate) src: u8,
    pub(crate) skip: bool,
    pub(crate) start: i64,
    pub(crate) step: i64,
    pub(crate) count: u64,
    pub(crate) reps: u64,
}

impl Seg {
    /// `count` items of register `src` from `start`, `step` apart.
    pub(crate) fn of(src: u8, start: i64, step: i64, count: u64) -> Seg {
        Seg {
            src,
            skip: false,
            start,
            step,
            count,
            reps: 1,
        }
    }
}

/// A [`Vm::seq_build`] between two steps (`NSTATE` op `SEQ_BUILD`: the
/// registers `target, out, dst, s0, s1, s2`, then `op` and these words:
/// `total, seg, pos, rep, at, dstep`, a spare, five per segment).
type Build = [u64; 22];

impl Seg {
    fn words(&self) -> [u64; 5] {
        [
            u64::from(self.src) | u64::from(self.skip) << 8,
            self.start.cast_unsigned(),
            self.step.cast_unsigned(),
            self.count,
            self.reps,
        ]
    }

    /// Segment `i` of build `w`.
    fn at(w: &Build, i: u64) -> Option<Seg> {
        let o = w[7..].as_chunks::<5>().0.get(usize::try_from(i).ok()?)?;
        Some(Seg {
            src: (o[0] & 7) as u8,
            skip: o[0] >> 8 & 1 != 0,
            start: o[1].cast_signed(),
            step: o[2].cast_signed(),
            count: o[3],
            reps: o[4],
        })
    }
}
