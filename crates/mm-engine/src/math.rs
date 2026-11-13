//! The `math` module (native, docs/spec.md §12), with CPython 3.14's error
//! messages.

use crate::bigint::Big;
use crate::class::Dn;
use crate::cont::Cont;
use crate::libm;
use crate::text::float_repr;
use crate::types::{T, ty};
use crate::vm::{CallArgs, Raised, Vm};
use alloc::{format, vec::Vec};
use mm_core::Value;

/// Module constants.
pub(crate) const CONSTANTS: [(&str, f64); 5] = [
    ("pi", core::f64::consts::PI),
    ("e", core::f64::consts::E),
    ("tau", core::f64::consts::TAU),
    ("inf", f64::INFINITY),
    ("nan", f64::NAN),
];

impl Vm {
    /// A real-number argument as `f64` (`TypeError` for other types,
    /// `OverflowError` for ints too large).
    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn real(&mut self, v: Value) -> Result<f64, Raised> {
        if let Some(f) = self.num_to_f64(v)? {
            return Ok(f);
        }
        // `__float__`, else `__index__` (the prelude's `_mcall` calls it).
        if self.uflags(v) & (crate::class::f::FLOAT | crate::class::f::INDEX) != 0 {
            return Err(self.need_py());
        }
        // A float or int subclass without its own `__float__`: its value.
        if let Some(b) = self.backing(v)
            && let Some(f) = self.num_to_f64(b)?
        {
            return Ok(f);
        }
        let msg = format!("must be real number, not {}", self.type_name(v));
        Err(self.err(T::TypeError, &msg))
    }

    /// An integer argument as `Big` (`TypeError` otherwise).
    #[cfg_attr(mm_shipped, optimize(size))]
    fn integer(&mut self, v: Value) -> Result<Big, Raised> {
        if let Some(b) = self.big_of(v) {
            return Ok(b);
        }
        if self.uflags(v) & crate::class::f::INDEX != 0 {
            return Err(self.need_py());
        }
        // An int subclass without its own `__index__`: its value.
        if let Some(b) = self.backing(v).and_then(|b| self.big_of(b)) {
            return Ok(b);
        }
        let msg = format!(
            "'{}' object cannot be interpreted as an integer",
            self.type_name(v)
        );
        Err(self.err(T::TypeError, &msg))
    }

    #[cfg_attr(mm_shipped, optimize(size))]
    fn math_range(&mut self) -> Raised {
        self.err(T::OverflowError, "math range error")
    }

    #[cfg_attr(mm_shipped, optimize(size))]
    fn math_domain(&mut self) -> Raised {
        self.err(T::ValueError, "math domain error")
    }

    #[cfg_attr(mm_shipped, optimize(size))]
    fn math_expected(&mut self, what: &str, x: f64) -> Raised {
        let msg = format!("expected {what}, got {}", float_repr(x));
        self.err(T::ValueError, &msg)
    }
}

#[cfg_attr(mm_shipped, optimize(size))]
fn arg_f(vm: &mut Vm, a: &CallArgs, i: u32) -> Result<f64, Raised> {
    vm.real(vm.arg_o(a, i))
}

/// A float result: a finite input giving an infinite result overflows.
#[cfg_attr(mm_shipped, optimize(size))]
fn finish(vm: &mut Vm, x: f64, r: f64) -> Result<Value, Raised> {
    if r.is_infinite() && x.is_finite() {
        return Err(vm.math_range());
    }
    Ok(Value::float(r))
}

macro_rules! unary {
    ($name:ident, $f:expr) => {
        pub(crate) fn $name(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
            let x = arg_f(vm, &a, 0)?;
            let r: f64 = $f(x);
            finish(vm, x, r)
        }
    };
}

unary!(m_exp, libm::exp);
unary!(m_expm1, libm::expm1);
unary!(m_atan, libm::atan);
unary!(m_sinh, libm::sinh);
unary!(m_cosh, libm::cosh);
unary!(m_tanh, libm::tanh);
unary!(m_asinh, libm::asinh);
unary!(m_erf, libm::erf);
unary!(m_erfc, libm::erfc);
unary!(m_cbrt, libm::cbrt);
unary!(m_fabs, libm::fabs);
unary!(m_degrees, |x: f64| x * (180.0 / core::f64::consts::PI));
unary!(m_radians, |x: f64| x * (core::f64::consts::PI / 180.0));
unary!(m_exp2, |x: f64| {
    if libm::rint(x) == x && libm::fabs(x) < 2000.0 {
        crate::intops::ldexp(1.0, x as i64)
    } else {
        libm::pow(2.0, x)
    }
});

#[cfg_attr(mm_shipped, optimize(size))]
fn trig(vm: &mut Vm, a: &CallArgs, f: fn(f64) -> f64) -> Result<Value, Raised> {
    let x = arg_f(vm, a, 0)?;
    if x.is_infinite() {
        return Err(vm.math_expected("a finite input", x));
    }
    Ok(Value::float(f(x)))
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn m_sin(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    trig(vm, &a, libm::sin)
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn m_cos(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    trig(vm, &a, libm::cos)
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn m_tan(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    trig(vm, &a, libm::tan)
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn m_asin(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let x = arg_f(vm, &a, 0)?;
    if libm::fabs(x) > 1.0 {
        return Err(vm.math_expected("a number in range from -1 up to 1", x));
    }
    Ok(Value::float(libm::asin(x)))
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn m_acos(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let x = arg_f(vm, &a, 0)?;
    if libm::fabs(x) > 1.0 {
        return Err(vm.math_expected("a number in range from -1 up to 1", x));
    }
    Ok(Value::float(libm::acos(x)))
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn m_acosh(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let x = arg_f(vm, &a, 0)?;
    if x < 1.0 {
        return Err(vm.math_expected("argument value not less than 1", x));
    }
    Ok(Value::float(libm::acosh(x)))
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn m_atanh(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let x = arg_f(vm, &a, 0)?;
    if libm::fabs(x) >= 1.0 {
        return Err(vm.math_expected("a number between -1 and 1", x));
    }
    Ok(Value::float(libm::atanh(x)))
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn m_atan2(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let y = arg_f(vm, &a, 0)?;
    let x = arg_f(vm, &a, 1)?;
    Ok(Value::float(libm::atan2(y, x)))
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn m_sqrt(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let x = arg_f(vm, &a, 0)?;
    if x < 0.0 {
        return Err(vm.math_expected("a nonnegative input", x));
    }
    Ok(Value::float(crate::ops::sqrt(x)))
}

/// `log` of any int or float argument; ints too large for a float use
/// their bit length, as CPython's `loghelper`.
#[cfg_attr(mm_shipped, optimize(size))]
fn log_of(vm: &mut Vm, v: Value, f: fn(f64) -> f64) -> Result<f64, Raised> {
    if Vm::is_intlike(v) {
        let b = vm.big_of(v).unwrap_or_default();
        // Its copy and shift, a unit per 8 limbs as the shifts charge.
        vm.burn(b.mag.len() as u64 / 4);
        if b.neg || b.is_zero() {
            return Err(vm.err(T::ValueError, "expected a positive input"));
        }
        if let Some(x) = b.to_f64() {
            return Ok(f(x));
        }
        // x = m * 2^e with m a float near 1.
        let e = b.bit_length() - 64;
        let m = b.shr(e).to_f64().unwrap_or(1.0);
        return Ok(f(m) + (e as f64) * f(2.0));
    }
    let x = vm.real(v)?;
    if x.is_nan() {
        return Ok(x);
    }
    if x <= 0.0 {
        return Err(vm.math_expected("a positive input", x));
    }
    Ok(f(x))
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn m_log(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let num = log_of(vm, vm.arg_o(&a, 0), libm::log)?;
    let base = vm.arg_o(&a, 1);
    if base == Value::UNDEFINED {
        return Ok(Value::float(num));
    }
    let den = log_of(vm, base, libm::log)?;
    if den == 0.0 {
        return Err(vm.err(T::ZeroDivisionError, "division by zero"));
    }
    Ok(Value::float(num / den))
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn m_log2(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let r = log_of(vm, vm.arg_o(&a, 0), libm::log2)?;
    Ok(Value::float(r))
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn m_log10(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let r = log_of(vm, vm.arg_o(&a, 0), libm::log10)?;
    Ok(Value::float(r))
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn m_log1p(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let x = arg_f(vm, &a, 0)?;
    if x <= -1.0 {
        return Err(vm.math_expected("argument value > -1", x));
    }
    Ok(Value::float(libm::log1p(x)))
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn m_pow(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let x = arg_f(vm, &a, 0)?;
    let y = arg_f(vm, &a, 1)?;
    if x.is_finite() && y.is_finite() {
        if x == 0.0 && y < 0.0 {
            return Err(vm.math_domain());
        }
        if x < 0.0 && libm::rint(y) != y {
            return Err(vm.math_domain());
        }
    }
    let r = if y == 0.0 || x == 1.0 {
        1.0
    } else if x.is_nan() || y.is_nan() {
        f64::NAN
    } else if y.is_infinite() {
        let ax = libm::fabs(x);
        if ax == 1.0 {
            1.0
        } else if (y > 0.0) == (ax > 1.0) {
            f64::INFINITY
        } else {
            0.0
        }
    } else if x.is_infinite() {
        let odd = libm::fabs(crate::ops::float_divmod(y, 2.0).1) == 1.0;
        if y > 0.0 {
            if x < 0.0 && odd { x } else { libm::fabs(x) }
        } else if x < 0.0 && odd {
            -0.0
        } else {
            0.0
        }
    } else {
        libm::pow(x, y)
    };
    // Overflow only from finite operands: `pow(2, inf)` is inf, as CPython's.
    finish(
        vm,
        if x.is_finite() && y.is_finite() {
            0.0
        } else {
            f64::INFINITY
        },
        r,
    )
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn m_fmod(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let x = arg_f(vm, &a, 0)?;
    let y = arg_f(vm, &a, 1)?;
    if x.is_nan() || y.is_nan() {
        return Ok(Value::float(f64::NAN));
    }
    if x.is_infinite() || y == 0.0 {
        return Err(vm.math_domain());
    }
    Ok(Value::float(libm::fmod(x, y)))
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn m_remainder(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let x = arg_f(vm, &a, 0)?;
    let y = arg_f(vm, &a, 1)?;
    if x.is_nan() || y.is_nan() {
        return Ok(Value::float(f64::NAN));
    }
    if x.is_infinite() || y == 0.0 {
        return Err(vm.math_domain());
    }
    if y.is_infinite() {
        return Ok(Value::float(x));
    }
    let absx = libm::fabs(x);
    let absy = libm::fabs(y);
    let m = libm::fmod(absx, absy);
    let c = absy - m;
    let r = if m < c {
        m
    } else if m > c {
        -c
    } else {
        m - 2.0 * libm::fmod(0.5 * (absx - m), absy)
    };
    Ok(Value::float(libm::copysign(1.0, x) * r))
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn m_copysign(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let x = arg_f(vm, &a, 0)?;
    let y = arg_f(vm, &a, 1)?;
    Ok(Value::float(libm::copysign(x, y)))
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn m_isnan(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let x = arg_f(vm, &a, 0)?;
    Ok(Value::from_bool(x.is_nan()))
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn m_isinf(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let x = arg_f(vm, &a, 0)?;
    Ok(Value::from_bool(x.is_infinite()))
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn m_isfinite(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let x = arg_f(vm, &a, 0)?;
    Ok(Value::from_bool(x.is_finite()))
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn m_isclose(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let x = arg_f(vm, &a, 0)?;
    let y = arg_f(vm, &a, 1)?;
    let rel = match vm.kwarg(&a, 0) {
        Value::UNDEFINED => 1e-9,
        v => vm.real(v)?,
    };
    let abs = match vm.kwarg(&a, 1) {
        Value::UNDEFINED => 0.0,
        v => vm.real(v)?,
    };
    if rel < 0.0 || abs < 0.0 {
        return Err(vm.err(T::ValueError, "tolerances must be non-negative"));
    }
    if x == y {
        return Ok(Value::TRUE);
    }
    if x.is_infinite() || y.is_infinite() {
        return Ok(Value::FALSE);
    }
    let diff = libm::fabs(y - x);
    let ok = (diff <= libm::fabs(rel * y)) || (diff <= libm::fabs(rel * x)) || diff <= abs;
    Ok(Value::from_bool(ok))
}

/// `floor`/`ceil`: an int; `up` selects ceil.
#[cfg_attr(mm_shipped, optimize(size))]
fn round_int(vm: &mut Vm, a: &CallArgs, up: bool) -> Result<Value, Raised> {
    let v = vm.arg_o(a, 0);
    let hook = if up { Dn::Ceil } else { Dn::Floor };
    if let Some(r) = vm.dunder_call(v, hook, &[], Cont::None)? {
        return Ok(r);
    }
    if Vm::is_intlike(v) {
        let b = vm.big_of(v).unwrap_or_default();
        return vm.new_big(&b);
    }
    let x = vm.real(v)?;
    if x.is_nan() {
        return Err(vm.err(T::ValueError, "cannot convert float NaN to integer"));
    }
    if x.is_infinite() {
        return Err(vm.err(T::OverflowError, "cannot convert float infinity to integer"));
    }
    let f = crate::ops::floor(x);
    let r = if up && f != x { f + 1.0 } else { f };
    vm.new_big(&Big::from_f64(r))
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn m_floor(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    round_int(vm, &a, false)
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn m_ceil(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    round_int(vm, &a, true)
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn m_trunc(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let v = vm.arg_o(&a, 0);
    if let Some(r) = vm.dunder_call(v, Dn::Trunc, &[], Cont::None)? {
        return Ok(r);
    }
    // An int or float subclass truncates as its value (`Vm::num_backed`).
    let v = if vm.num_backed(v) { vm.unbx(v) } else { v };
    if Vm::is_intlike(v) {
        let b = vm.big_of(v).unwrap_or_default();
        return vm.new_big(&b);
    }
    if let Some(x) = v.as_float() {
        if x.is_nan() {
            return Err(vm.err(T::ValueError, "cannot convert float NaN to integer"));
        }
        if x.is_infinite() {
            return Err(vm.err(T::OverflowError, "cannot convert float infinity to integer"));
        }
        return vm.new_big(&Big::from_f64(x));
    }
    if vm.user_class(v).is_some() {
        return Err(vm.need_py());
    }
    let msg = format!("type {} doesn't define __trunc__ method", vm.type_name(v));
    Err(vm.err(T::TypeError, &msg))
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn m_modf(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let x = arg_f(vm, &a, 0)?;
    let (frac, int) = if x.is_infinite() {
        (libm::copysign(0.0, x), x)
    } else if x.is_nan() {
        (x, x)
    } else {
        let i = libm::copysign(x - crate::ops::fract(x), x);
        (libm::copysign(crate::ops::fract(x), x), i)
    };
    vm.alloc_tuple(&[Value::float(frac), Value::float(int)])
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn m_frexp(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let x = arg_f(vm, &a, 0)?;
    let (m, e) = libm::frexp(x);
    let ev = vm.new_int(i64::from(e))?;
    vm.alloc_tuple(&[Value::float(m), ev])
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn m_ldexp(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let x = arg_f(vm, &a, 0)?;
    let e = vm.integer(vm.arg_o(&a, 1))?;
    let e = e
        .to_i64()
        .unwrap_or(if e.neg { i64::MIN / 2 } else { i64::MAX / 2 });
    let r = crate::intops::ldexp(x, e);
    finish(vm, x, r)
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn m_gamma(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let x = arg_f(vm, &a, 0)?;
    match libm::gamma(x) {
        None => Err(vm.math_expected("a noninteger or positive integer", x)),
        Some(r) => finish(vm, x, r),
    }
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn m_lgamma(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let x = arg_f(vm, &a, 0)?;
    match libm::lgamma(x) {
        None => Err(vm.math_expected("a noninteger or positive integer", x)),
        Some(r) => finish(vm, x, r),
    }
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn m_nextafter(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let x = arg_f(vm, &a, 0)?;
    let y = arg_f(vm, &a, 1)?;
    Ok(Value::float(nextafter(x, y)))
}

#[cfg_attr(mm_shipped, optimize(size))]
fn nextafter(x: f64, y: f64) -> f64 {
    if x.is_nan() || y.is_nan() {
        return f64::NAN;
    }
    if x == y {
        return y;
    }
    if x == 0.0 {
        return libm::copysign(f64::from_bits(1), y);
    }
    let b = x.to_bits();
    let up = (y > x) == (x > 0.0);
    f64::from_bits(if up { b + 1 } else { b - 1 })
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn m_ulp(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let x = libm::fabs(arg_f(vm, &a, 0)?);
    if x.is_nan() || x.is_infinite() {
        return Ok(Value::float(x));
    }
    let next = nextafter(x, f64::INFINITY);
    let r = if next.is_infinite() {
        x - nextafter(x, 0.0)
    } else {
        next - x
    };
    Ok(Value::float(r))
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn m_factorial(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let n = vm.integer(vm.arg_o(&a, 0))?;
    factorial(vm, &n)
}

#[cfg_attr(mm_shipped, optimize(size))]
fn factorial(vm: &mut Vm, n: &Big) -> Result<Value, Raised> {
    if n.neg {
        return Err(vm.err(T::ValueError, "factorial() not defined for negative values"));
    }
    let Some(n) = n.to_i64() else {
        return Err(vm.err(
            T::OverflowError,
            "factorial() argument should not exceed 9223372036854775807",
        ));
    };
    let r = falling(vm, &Big::from_i64(n), n)?;
    vm.new_big(&r)
}

/// Fuel for `falling`'s product tree over a `bits`-bit result: the top
/// product (two halves) is Karatsuba's 3^d multiplications of `s`-limb
/// pieces at `s^2` limb steps each, and every level below adds two thirds
/// of the one above, so about three top products in all; one unit per six
/// limb steps, measured on factorials of 50,000 to 800,000. Never less than
/// a unit per 32 bits for building the factors.
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn product_cost(bits: u64) -> u64 {
    let (mut s, mut m) = ((bits / 32).div_ceil(2), 1u64);
    while s >= 80 {
        s = s.div_ceil(2);
        m = m.saturating_mul(3);
    }
    let top = m.saturating_mul(s * s);
    (top / 2).max(bits / 32)
}

/// `n * (n-1) * ... * (n-k+1)` by pairwise products.
#[cfg_attr(mm_shipped, optimize(size))]
fn falling(vm: &mut Vm, n: &Big, k: i64) -> Result<Big, Raised> {
    if k <= 0 {
        return Ok(Big::from_i64(1));
    }
    let bits = (k as u64).saturating_mul(n.bit_length().max(1));
    if k > 1 << 22 || bits > crate::intops::MAX_BITS || !vm.heap.would_fit(bits / 4) {
        return Err(vm.err(T::MemoryError, ""));
    }
    // About 1 ns a limb step, six per unit of `product_cost` (46 ms for
    // `factorial(50000)` [M], 47 estimated).
    vm.work_room(product_cost(bits).saturating_mul(6))?;
    vm.burn(product_cost(bits));
    let one = Big::from_i64(1);
    let mut parts: Vec<Big> = Vec::new();
    let mut cur = n.clone();
    let mut acc: Option<u64> = Some(1);
    for _ in 0..k {
        match (acc, cur.to_i64().and_then(|c| u64::try_from(c).ok())) {
            (Some(x), Some(c)) if x.checked_mul(c).is_some() => acc = Some(x * c),
            _ => {
                if let Some(x) = acc {
                    parts.push(Big::from_u64(x, false));
                }
                acc = None;
                parts.push(cur.clone());
            }
        }
        if acc.is_none() {
            acc = Some(1);
        }
        cur = cur.sub(&one);
    }
    if let Some(x) = acc {
        parts.push(Big::from_u64(x, false));
    }
    while parts.len() > 1 {
        let mut next = Vec::with_capacity(parts.len().div_ceil(2));
        for pair in parts.chunks(2) {
            next.push(if pair.len() == 2 {
                pair[0].mul(&pair[1])
            } else {
                pair[0].clone()
            });
        }
        parts = next;
    }
    Ok(parts.pop().unwrap_or(one))
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn m_comb(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let n = vm.integer(vm.arg_o(&a, 0))?;
    let k = vm.integer(vm.arg_o(&a, 1))?;
    if n.neg {
        return Err(vm.err(T::ValueError, "n must be a non-negative integer"));
    }
    if k.neg {
        return Err(vm.err(T::ValueError, "k must be a non-negative integer"));
    }
    if k.cmp(&n) == core::cmp::Ordering::Greater {
        return vm.new_int(0);
    }
    let nk = n.sub(&k);
    let kk = if nk.cmp(&k) == core::cmp::Ordering::Less {
        nk
    } else {
        k
    };
    let Some(kk) = kk.to_i64() else {
        return Err(vm.err(
            T::OverflowError,
            "min(n - k, k) must not exceed 9223372036854775807",
        ));
    };
    // Both products and their division, one step.
    let bits = |b: u64| (kk as u64).saturating_mul(b.max(1));
    let (nb, kb) = (
        bits(n.bit_length()),
        bits(u64::from(64 - kk.leading_zeros())),
    );
    let made = product_cost(nb).saturating_add(product_cost(kb));
    vm.work_room(
        made.saturating_mul(6)
            .saturating_add(crate::intops::div_ns(nb / 32 + 1, kb / 32 + 1)),
    )?;
    let num = falling(vm, &n, kk)?;
    let den = falling(vm, &Big::from_i64(kk), kk)?;
    // The division, charged as `//` is (`Vm::native_short` estimates it).
    let (ln, ld) = (num.mag.len() as u64, den.mag.len() as u64);
    vm.burn(ln * ld / 8);
    vm.new_big(&num.divmod_floor(&den).0)
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn m_perm(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let n = vm.integer(vm.arg_o(&a, 0))?;
    let kv = vm.arg_o(&a, 1);
    if kv == Value::UNDEFINED || kv == Value::NONE {
        return factorial(vm, &n);
    }
    let k = vm.integer(kv)?;
    if n.neg {
        return Err(vm.err(T::ValueError, "n must be a non-negative integer"));
    }
    if k.neg {
        return Err(vm.err(T::ValueError, "k must be a non-negative integer"));
    }
    if k.cmp(&n) == core::cmp::Ordering::Greater {
        return vm.new_int(0);
    }
    let Some(k) = k.to_i64() else {
        return Err(vm.err(T::OverflowError, "k must not exceed 9223372036854775807"));
    };
    let r = falling(vm, &n, k)?;
    vm.new_big(&r)
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn m_isqrt(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let n = vm.integer(vm.arg_o(&a, 0))?;
    if n.neg {
        return Err(vm.err(T::ValueError, "isqrt() argument must be nonnegative"));
    }
    let l = n.mag.len() as u64;
    vm.work_room(crate::intops::isqrt_ns(l))?;
    vm.burn(l * l / 4);
    vm.new_big(&n.isqrt())
}

#[cfg_attr(mm_shipped, optimize(size))]
fn gcd_all(vm: &mut Vm, a: &CallArgs, lcm: bool) -> Result<Value, Raised> {
    use crate::intops::{WORK_MAX, div_ns, gcd_ns, mul_ns};
    // The estimate of one pair: `lcm(a, b)` is the longer times the
    // shorter divided by the gcd, so only a short value is divided.
    let pair_ns = |la: u64, lx: u64| {
        let ns = gcd_ns(la, lx);
        if lcm {
            let short = la.min(lx);
            ns.saturating_add(mul_ns(la, lx))
                .saturating_add(div_ns(short, short))
        } else {
            ns
        }
    };
    // The whole call's estimate, an upper bound from the arguments' sizes
    // alone: past `WORK_MAX` with more than two arguments the prelude goes
    // on pairwise, a native call and step per pair, before any work here,
    // so one step never covers many large arguments (40 at the cap took
    // 659 ms in one step [M, review H2]).
    let limbs = |vm: &Vm, v: Value| {
        if Vm::is_heap_int(v) {
            vm.heap_int_parts(v).1.len().div_ceil(4) as u64
        } else {
            2
        }
    };
    // Two ints whose lcm is more than a quarter of a step's work: the
    // prelude runs the gcd, the short operand's division and the product,
    // a native step each (one step took 28 to 31 ms at the cap [M]).
    if lcm && a.n == 2 {
        let (x, y) = (vm.arg_o(a, 0), vm.arg_o(a, 1));
        if Vm::is_intlike(x)
            && Vm::is_intlike(y)
            && pair_ns(limbs(vm, x), limbs(vm, y)) > WORK_MAX / 4
        {
            return Err(vm.need_py());
        }
    }
    if a.n > 2 {
        // The accumulator's limbs: 1 starts an lcm, 0 a gcd.
        let (mut acc, mut total) = (u64::from(lcm), 0u64);
        for i in 0..a.n {
            let lx = limbs(vm, vm.arg_o(a, i));
            total = total.saturating_add(pair_ns(acc, lx));
            acc = if lcm {
                acc + lx
            } else if i == 0 {
                lx
            } else {
                acc.min(lx)
            };
        }
        if total > WORK_MAX {
            return Err(vm.need_py());
        }
    }
    let mut acc = if lcm { Big::from_i64(1) } else { Big::zero() };
    for i in 0..a.n {
        let x = vm.integer(vm.arg_o(a, i))?;
        let l = x.mag.len().max(acc.mag.len()) as u64;
        let ns = pair_ns(acc.mag.len() as u64, x.mag.len() as u64);
        vm.work_room(ns)?;
        // A unit for about 10 ns of the estimate: linear when one is small.
        vm.burn(ns / 10 + l / 8);
        acc = if lcm {
            if acc.is_zero() || x.is_zero() {
                Big::zero()
            } else {
                let g = acc.gcd(&x);
                let (long, short) = if acc.mag.len() >= x.mag.len() {
                    (&acc, &x)
                } else {
                    (&x, &acc)
                };
                let mut m = long.mul(&short.divmod_floor(&g).0);
                m.neg = false;
                m
            }
        } else {
            acc.gcd(&x)
        };
    }
    vm.new_big(&acc)
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn m_gcd(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    gcd_all(vm, &a, false)
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn m_lcm(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    gcd_all(vm, &a, true)
}

/// `math.fsum`: Shewchuk's exact partials (CPython's `math_fsum`).
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn m_fsum(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let xs = vm.fold_seq(vm.arg_o(&a, 0))?;
    vm.fold_start(FSUM, [xs, Value::NONE, Value::NONE])
}

/// `math.prod(iterable, *, start=1)` over builtin numbers; other values
/// go through the generic `*`.
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn m_prod(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let xs = vm.fold_seq(vm.arg_o(&a, 0))?;
    let start = match vm.kwarg(&a, 0) {
        Value::UNDEFINED => Value::int(1).unwrap_or(Value::NONE),
        v => v,
    };
    vm.fold_start(PROD, [xs, start, start])
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn m_hypot(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let items: Vec<Value> = (0..a.n).map(|i| vm.arg_o(&a, i)).collect();
    let t = vm.alloc_tuple(&items)?;
    vm.hypot_of(t)
}

impl Vm {
    /// Whether the `CALL_EX` at register `f` is `math.hypot(*t)` with a
    /// long argument list or tuple `t` and no keywords: [`Vm::hypot_of`]
    /// then folds `t` itself, a slice per step, instead of the call laying
    /// a copy of it out on the stack in one go.
    #[inline(never)]
    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn is_hypot_star(&self, f: usize) -> bool {
        let [callee, t, kw] = [f, f + 1, f + 2].map(|i| self.stack[i]);
        (t.is_obj_kind(ty::TUPLE as u8) || t.is_obj_kind(ty::LIST as u8))
            && self.seq_words(t).len() >= crate::ops::SEQ_LONG
            && (kw == Value::NONE || self.dict_len(kw) == 0)
            && callee
                .as_native()
                .and_then(crate::builtins::native)
                .is_some_and(|n| n.module() == Some("math") && n.name() == "hypot")
    }

    /// `math.hypot(*t)`, the coordinates the items of list or tuple `t`.
    #[inline(never)]
    pub(crate) fn hypot_of(&mut self, t: Value) -> Result<Value, Raised> {
        self.fold_start(HYPOT, [t, t, Value::NONE])
    }
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn m_dist(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let p = vm.fold_seq(vm.arg_o(&a, 0))?;
    let q = vm.fold_seq(vm.arg_o(&a, 1))?;
    if vm.seq_words(p).len() != vm.seq_words(q).len() {
        return Err(vm.err(
            T::ValueError,
            "both points must have the same number of dimensions",
        ));
    }
    vm.fold_start(DIST, [p, q, Value::NONE])
}

/// `math.sumprod(p, q)`: CPython's three accumulators — exact machine
/// ints, a triple-length float sum, and the generic `+`/`*` fallback,
/// each path abandoned for good at the first pair it cannot take.
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn m_sumprod(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let p = vm.fold_seq(vm.arg_o(&a, 0))?;
    let q = vm.fold_seq(vm.arg_o(&a, 1))?;
    if vm.seq_words(p).len() != vm.seq_words(q).len() {
        return Err(vm.err(T::ValueError, "Inputs are not the same length"));
    }
    let zero = Value::int(0).unwrap_or(Value::NONE);
    vm.fold_start(SUMPROD, [p, q, zero])
}

// ------------------------------------------------- math folds, in steps

/// The chunked folds over a sequence (`nstep::op::MATH_FOLD`, docs/spec.md
/// §5): its value slots, and its raw words after the kind and position.
///
/// ```text
/// FSUM     xs, -, partials (a byte block)  partials, special, inf_sum
/// PROD     xs, acc, start
/// SUMPROD  p, q, total                     flags, int total, three floats
/// DIST     p, q, -                         pass, max, nan, csum, frac1, frac2
///          (pass 0 finds the largest, pass 1 sums the scaled squares)
/// HYPOT    args, args, -                   as DIST
/// ```
const FSUM: u64 = 0;
const PROD: u64 = 1;
const SUMPROD: u64 = 2;
const DIST: u64 = 3;
const HYPOT: u64 = 4;

/// Fsum's partials at most: they do not overlap, so at most one per bit
/// position from 2^-1074 to 2^1023.
const PARTIALS: usize = 2112;

/// SUMPROD flags: the int path is on, it holds a total, the float path
/// is on, it holds one.
const INT_ON: u64 = 1;
const INT_USED: u64 = 2;
const FLT_ON: u64 = 4;
const FLT_USED: u64 = 8;

impl Vm {
    /// A list or tuple as itself; any other iterable's items as a list.
    /// An iterator's are pulled an item at a time in `_mcall`, so a bad
    /// one leaves the rest in it, as in CPython.
    #[cfg_attr(mm_shipped, optimize(size))]
    fn fold_seq(&mut self, v: Value) -> Result<Value, Raised> {
        if v.is_obj_kind(ty::LIST as u8) || v.is_obj_kind(ty::TUPLE as u8) {
            return Ok(v);
        }
        if self.is_iterator(v) {
            return Err(self.need_py());
        }
        let items = self.collect(v)?;
        self.new_list(&items)
    }

    /// A fold's first step, the rest in `_steps` when it is long.
    #[cfg_attr(mm_shipped, optimize(size))]
    fn fold_start(&mut self, kind: u64, mut vals: [Value; 3]) -> Result<Value, Raised> {
        let mut raw = [0u64; 8];
        if kind == SUMPROD {
            raw[0] = INT_ON | FLT_ON;
        }
        let mut i = 0;
        let mut partials = Vec::new();
        if let Some(v) = self.fold_run(kind, &mut vals, &mut i, &mut raw, &mut partials)? {
            return Ok(v);
        }
        if kind == FSUM {
            let mut bytes = alloc::vec![0u8; PARTIALS * 8];
            for (o, x) in bytes.as_chunks_mut::<8>().0.iter_mut().zip(&partials) {
                o.copy_from_slice(&x.to_le_bytes());
            }
            let res = self.heap.alloc_bytes(&bytes);
            vals[2] = Value::from_byte_ref(self.hx_o(res)?);
            raw[0] = partials.len() as u64;
        }
        let mut words = alloc::vec![kind, i as u64];
        words.extend_from_slice(&raw);
        self.chunk_rest(crate::nstep::op::MATH_FOLD, &vals, &words)
    }

    /// One step of a long fold.
    #[inline(never)]
    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn fold_step(&mut self, state: Value) -> Result<crate::nstep::Step, Raised> {
        use crate::nstep::{Step, op};
        let Some((mut vals, w)) = self.nstate_get::<3, 11>(op::MATH_FOLD, state) else {
            return Err(self.bad_step());
        };
        let kind = w[1];
        let mut i = usize::try_from(w[2]).unwrap_or(usize::MAX);
        let mut raw = [0u64; 8];
        raw.copy_from_slice(&w[3..]);
        let mut partials = Vec::new();
        let block = vals[2].as_byte_ref();
        if kind == FSUM {
            let (Some(b), Ok(n)) = (block, usize::try_from(raw[0])) else {
                return Err(self.bad_step());
            };
            let Some(bytes) = self
                .heap
                .bytes(b)
                .ok()
                .and_then(|d| d.get(..n.checked_mul(8)?))
            else {
                return Err(self.bad_step());
            };
            partials.extend(
                bytes
                    .as_chunks::<8>()
                    .0
                    .iter()
                    .map(|c| f64::from_le_bytes(*c)),
            );
        }
        if let Some(v) = self.fold_run(kind, &mut vals, &mut i, &mut raw, &mut partials)? {
            return Ok(Step::Done(v));
        }
        if let (FSUM, Some(b)) = (kind, block) {
            raw[0] = partials.len() as u64;
            let Ok(out) = self.heap.bytes_mut(b) else {
                return Err(self.bad_step());
            };
            for (o, x) in out.as_chunks_mut::<8>().0.iter_mut().zip(&partials) {
                o.copy_from_slice(&x.to_le_bytes());
            }
        }
        if let Some(r) = state.as_obj() {
            self.set_slot_o(r, 1, vals[1]);
            self.set_slot_o(r, 2, vals[2]);
        }
        let mut words = [i as u64; 9];
        words[1..].copy_from_slice(&raw);
        self.nstate_put(state, 2, &words);
        Ok(Step::More)
    }

    /// Runs fold `kind` from item `*i` for up to `nstep::STEP_WORK` units:
    /// its value, or `None` with the progress kept in `vals`, `raw` and
    /// `partials`. An item that needs Python code runs the whole call
    /// again in `_mcall`, as the native would have. One unit per item,
    /// charged at the end in one go, as `Vm::seq_in_scan` does (a charge
    /// over 64 units brings the next safepoint's check forward).
    #[cfg_attr(mm_shipped, optimize(size))]
    fn fold_run(
        &mut self,
        kind: u64,
        vals: &mut [Value; 3],
        i: &mut usize,
        raw: &mut [u64; 8],
        partials: &mut Vec<f64>,
    ) -> Result<Option<Value>, Raised> {
        let mut n = 0;
        let r = self.fold_items(kind, vals, i, raw, partials, &mut n);
        self.burn(n);
        r
    }

    #[cfg_attr(mm_shipped, optimize(size))]
    fn fold_items(
        &mut self,
        kind: u64,
        vals: &mut [Value; 3],
        i: &mut usize,
        raw: &mut [u64; 8],
        partials: &mut Vec<f64>,
        n: &mut u64,
    ) -> Result<Option<Value>, Raised> {
        let start = self.work;
        let r = loop {
            if *n + self.work.wrapping_sub(start) >= crate::nstep::STEP_WORK {
                return Ok(None);
            }
            let item = |vm: &Vm, s: Value, i: usize| {
                vm.seq_words(s).get(i).and_then(|&w| Value::from_bits(w))
            };
            let (x, y) = (item(self, vals[0], *i), item(self, vals[1], *i));
            *n += 1;
            let r = match kind {
                FSUM => match x {
                    Some(x) => self.real(x).and_then(|x| self.fsum_add(x, partials, raw)),
                    None => break self.fsum_total(partials, raw),
                },
                PROD => match x {
                    Some(x) => self
                        .binop(mm_compile::ast::BinOp::Mul, vals[1], x, false)
                        .and_then(|v| self.steps_done(v))
                        .map(|v| vals[1] = v),
                    None => break Ok(vals[1]),
                },
                SUMPROD => {
                    let pair = x.zip(y);
                    match self.sumprod_add(pair, vals, raw) {
                        Ok(()) if pair.is_none() => break Ok(vals[2]),
                        r => r,
                    }
                }
                _ => match (x, y) {
                    (Some(x), Some(y)) => self.norm_add(kind, x, y, raw),
                    _ if raw[0] == 0 => {
                        // The largest found: the scaled squares next, the
                        // sum starting at 1 as `libm::vector_norm`'s.
                        raw[0] = 1;
                        raw[3] = 1.0f64.to_bits();
                        *i = 0;
                        continue;
                    }
                    _ => break Ok(Value::float(norm_total(raw, self.seq_words(vals[0]).len()))),
                },
            };
            if let Err(e) = r {
                if !self.slow {
                    return Err(e);
                }
                self.slow = false;
                return self.fold_restart(kind, vals).map(Some);
            }
            *i += 1;
        };
        r.map(Some)
    }

    /// `_mcall` of the fold's native with its arguments (the sequences it
    /// read, iterators already as lists).
    #[cfg_attr(mm_shipped, optimize(size))]
    fn fold_restart(&mut self, kind: u64, vals: &[Value; 3]) -> Result<Value, Raised> {
        let name = ["fsum", "prod", "sumprod", "dist", "hypot"][kind as usize % 5];
        let Some(idx) = crate::builtins::NATIVES
            .iter()
            .position(|n| n.module() == Some("math") && n.name() == name)
        else {
            return Err(self.bad_step());
        };
        let kw = self.new_dict()?;
        let args = match kind {
            FSUM => self.alloc_tuple(&vals[..1])?,
            PROD => {
                let k = self.intern("start")?;
                self.dict_set(kw, k, vals[2])?;
                self.alloc_tuple(&vals[..1])?
            }
            HYPOT => vals[0],
            _ => self.alloc_tuple(&vals[..2])?,
        };
        let f = Value::native(idx as u32);
        self.prelude_request("_mcall", alloc::vec![f, args, kw], Cont::None)
    }

    /// Adds `x` to fsum's partials (Shewchuk, as CPython's `math_fsum`);
    /// `raw[1]` and `raw[2]` hold its special and infinite sums.
    #[cfg_attr(mm_shipped, optimize(size))]
    fn fsum_add(
        &mut self,
        mut x: f64,
        partials: &mut Vec<f64>,
        raw: &mut [u64; 8],
    ) -> Result<(), Raised> {
        let xsave = x;
        let mut i = 0;
        for j in 0..partials.len() {
            let mut y = partials[j];
            if libm::fabs(x) < libm::fabs(y) {
                core::mem::swap(&mut x, &mut y);
            }
            let hi = x + y;
            let lo = y - (hi - x);
            if lo != 0.0 {
                partials[i] = lo;
                i += 1;
            }
            x = hi;
        }
        if x == 0.0 {
            partials.truncate(i);
        } else if x.is_finite() {
            partials.truncate(i);
            if partials.len() >= PARTIALS {
                return Err(self.bad_step());
            }
            partials.push(x);
        } else {
            if xsave.is_finite() {
                return Err(self.err(T::OverflowError, "intermediate overflow in fsum"));
            }
            if xsave.is_infinite() {
                raw[2] = (f64::from_bits(raw[2]) + xsave).to_bits();
            }
            raw[1] = (f64::from_bits(raw[1]) + xsave).to_bits();
            partials.clear();
        }
        Ok(())
    }

    #[cfg_attr(mm_shipped, optimize(size))]
    fn fsum_total(&mut self, partials: &[f64], raw: &[u64; 8]) -> Result<Value, Raised> {
        let (special, inf_sum) = (f64::from_bits(raw[1]), f64::from_bits(raw[2]));
        if special != 0.0 {
            if inf_sum.is_nan() {
                return Err(self.err(T::ValueError, "-inf + inf in fsum"));
            }
            return Ok(Value::float(special));
        }
        let mut hi = 0.0;
        let mut n = partials.len();
        if n > 0 {
            n -= 1;
            hi = partials[n];
            let mut lo = 0.0;
            while n > 0 {
                let x = hi;
                n -= 1;
                let y = partials[n];
                hi = x + y;
                let yr = hi - x;
                lo = y - yr;
                if lo != 0.0 {
                    break;
                }
            }
            if n > 0 && ((lo < 0.0 && partials[n - 1] < 0.0) || (lo > 0.0 && partials[n - 1] > 0.0))
            {
                let y = lo * 2.0;
                let x = hi + y;
                let yr = x - hi;
                if y == yr {
                    hi = x;
                }
            }
        }
        Ok(Value::float(hi))
    }

    /// One pair of `sumprod` (`None`: the end, flushing the paths' totals
    /// into `vals[2]`); `raw` holds the flags, the int total and the float
    /// triple.
    #[cfg_attr(mm_shipped, optimize(size))]
    fn sumprod_add(
        &mut self,
        pair: Option<(Value, Value)>,
        vals: &mut [Value; 3],
        raw: &mut [u64; 8],
    ) -> Result<(), Raised> {
        use mm_compile::ast::BinOp;
        let exact = |v: Value| v.as_int().is_some() || v.is_obj_kind(ty::INT as u8);
        if raw[0] & INT_ON != 0 {
            if let Some((x, y)) = pair.filter(|&(x, y)| exact(x) && exact(y)) {
                let prod = self
                    .int_of(x)
                    .zip(self.int_of_o(y))
                    .and_then(|(x, y)| x.checked_mul(y))
                    .and_then(|m| (raw[1] as i64).checked_add(m));
                if let Some(t) = prod {
                    raw[1] = t as u64;
                    raw[0] |= INT_USED;
                    return Ok(());
                }
            }
            raw[0] &= !INT_ON;
            if raw[0] & INT_USED != 0 {
                let t = self.new_int(raw[1] as i64)?;
                vals[2] = self.binop(BinOp::Add, vals[2], t, false)?;
                raw[0] &= !INT_USED;
            }
        }
        if raw[0] & FLT_ON != 0 {
            let int_like = |v: Value| exact(v) || v == Value::TRUE || v == Value::FALSE;
            let fl = match pair {
                Some((x, y)) => match (x.as_float(), y.as_float()) {
                    (Some(fx), Some(fy)) => Some((fx, fy)),
                    (Some(fx), None) if int_like(y) => {
                        self.num_to_f64(y).ok().flatten().map(|fy| (fx, fy))
                    }
                    (None, Some(fy)) if int_like(x) => {
                        self.num_to_f64(x).ok().flatten().map(|fx| (fx, fy))
                    }
                    _ => None,
                },
                None => None,
            };
            let tl = (
                f64::from_bits(raw[2]),
                f64::from_bits(raw[3]),
                f64::from_bits(raw[4]),
            );
            if let Some((x, y)) = fl {
                let (ph, pl) = libm::two_prod(x, y);
                let sm = libm::two_sum(tl.0, ph);
                let r1 = libm::two_sum(tl.1, pl);
                let r2 = libm::two_sum(r1.0, sm.1);
                let next = (sm.0, r2.0, tl.2 + r1.1 + r2.1);
                if next.0.is_finite() {
                    (raw[2], raw[3], raw[4]) =
                        (next.0.to_bits(), next.1.to_bits(), next.2.to_bits());
                    raw[0] |= FLT_USED;
                    return Ok(());
                }
            }
            raw[0] &= !FLT_ON;
            if raw[0] & FLT_USED != 0 {
                let last = libm::two_sum(tl.1, tl.0);
                let t = Value::float(tl.2 + last.1 + last.0);
                vals[2] = self.binop(BinOp::Add, vals[2], t, false)?;
                raw[0] &= !FLT_USED;
            }
        }
        let Some((x, y)) = pair else { return Ok(()) };
        let term = self.binop(BinOp::Mul, x, y, false)?;
        vals[2] = self.binop(BinOp::Add, vals[2], term, false)?;
        Ok(())
    }

    /// One coordinate of `dist` (`|x - y|`) or `hypot` (`|x|`, `x` and `y`
    /// the same): pass 0 finds the largest and any NaN (`raw[1]`,
    /// `raw[2]`), pass 1 sums the scaled squares (`libm::vector_norm`).
    #[cfg_attr(mm_shipped, optimize(size))]
    fn norm_add(
        &mut self,
        kind: u64,
        x: Value,
        y: Value,
        raw: &mut [u64; 8],
    ) -> Result<(), Raised> {
        let d = if kind == HYPOT {
            libm::fabs(self.real(x)?)
        } else {
            let (fx, fy) = (self.real(x)?, self.real(y)?);
            libm::fabs(fx - fy)
        };
        if raw[0] == 0 {
            raw[2] |= u64::from(d.is_nan());
            if d > f64::from_bits(raw[1]) {
                raw[1] = d.to_bits();
            }
            return Ok(());
        }
        let (pre, scale) = norm_scale(f64::from_bits(raw[1]));
        let x = d / pre * scale;
        let (ph, pl) = libm::two_prod(x, x);
        let csum = f64::from_bits(raw[3]);
        let sh = csum + ph;
        let sl = (csum - sh) + ph;
        raw[3] = sh.to_bits();
        raw[4] = (f64::from_bits(raw[4]) + pl).to_bits();
        raw[5] = (f64::from_bits(raw[5]) + sl).to_bits();
        Ok(())
    }
}

/// The factors `libm::vector_norm` divides and then multiplies
/// coordinates by for largest `max` (subnormal ones first by
/// `f64::MIN_POSITIVE`), its result scaled back by both in turn.
#[cfg_attr(mm_shipped, optimize(size))]
fn norm_scale(max: f64) -> (f64, f64) {
    let (m, pre) = if libm::frexp(max).1 < -1023 {
        (max / f64::MIN_POSITIVE, f64::MIN_POSITIVE)
    } else {
        (max, 1.0)
    };
    (pre, crate::intops::ldexp(1.0, -i64::from(libm::frexp(m).1)))
}

/// `libm::vector_norm`'s result from the sums `norm_add` kept.
#[cfg_attr(mm_shipped, optimize(size))]
fn norm_total(raw: &[u64; 8], n: usize) -> f64 {
    let max = f64::from_bits(raw[1]);
    if max.is_infinite() {
        return max;
    }
    if raw[2] != 0 {
        return f64::NAN;
    }
    if max == 0.0 || n <= 1 {
        return max;
    }
    let mut csum = f64::from_bits(raw[3]);
    let mut frac1 = f64::from_bits(raw[4]);
    let mut frac2 = f64::from_bits(raw[5]);
    let mut h = crate::ops::sqrt(csum - 1.0 + (frac1 + frac2));
    let (ph, pl) = libm::two_prod(-h, h);
    let sh = csum + ph;
    let sl = (csum - sh) + ph;
    csum = sh;
    frac1 += pl;
    frac2 += sl;
    let x = csum - 1.0 + (frac1 + frac2);
    h += x / (2.0 * h);
    let (pre, scale) = norm_scale(max);
    h / scale * pre
}

/// `math.fma(x, y, z)`: `x * y + z` with one rounding, computed exactly
/// on integers scaled by a common power of two.
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn m_fma(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let x = arg_f(vm, &a, 0)?;
    let y = arg_f(vm, &a, 1)?;
    let z = arg_f(vm, &a, 2)?;
    let r = fma(x, y, z);
    if !r.is_finite() {
        if r.is_nan() {
            if !x.is_nan() && !y.is_nan() && !z.is_nan() {
                return Err(vm.err(T::ValueError, "invalid operation in fma"));
            }
        } else if x.is_finite() && y.is_finite() && z.is_finite() {
            return Err(vm.err(T::OverflowError, "overflow in fma"));
        }
    }
    Ok(Value::float(r))
}

#[cfg_attr(mm_shipped, optimize(size))]
fn fma(x: f64, y: f64, z: f64) -> f64 {
    if !(x.is_finite() && y.is_finite() && z.is_finite()) || x == 0.0 || y == 0.0 {
        return x * y + z;
    }
    // f = mant * 2^exp with an integral 53-bit mantissa.
    let split = |f: f64| {
        let (m, e) = libm::frexp(f);
        (
            Big::from_f64(m * 9_007_199_254_740_992.0),
            i64::from(e) - 53,
        )
    };
    let ((mx, ex), (my, ey)) = (split(x), split(y));
    let (prod, ep) = (mx.mul(&my), ex + ey);
    let (sum, e) = if z == 0.0 {
        (prod, ep)
    } else {
        let (mz, ez) = split(z);
        let e = ep.min(ez);
        (prod.shl((ep - e) as u64).add(&mz.shl((ez - e) as u64)), e)
    };
    if sum.is_zero() {
        return 0.0;
    }
    let mag = sum.abs();
    let drop = (mag.bit_length() as i64 - 53).max(-1074 - e);
    let (q, e) = if drop > 0 {
        let d = drop as u64;
        let mut q = mag.shr(d);
        let rem = mag.sub(&q.shl(d));
        let half = Big::from_i64(1).shl(d - 1);
        let c = rem.cmp(&half);
        if c == core::cmp::Ordering::Greater || (c == core::cmp::Ordering::Equal && q.bit(0)) {
            q = q.add(&Big::from_i64(1));
        }
        (q, e + drop)
    } else {
        (mag, e)
    };
    let f = crate::intops::ldexp(q.to_f64().unwrap_or(f64::INFINITY), e);
    if sum.neg { -f } else { f }
}
