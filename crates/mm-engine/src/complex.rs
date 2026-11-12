//! `complex`: a heap object holding two `f64`s (docs/spec.md §7.1: a
//! NaN-boxed `Value` has no room for a pair). Arithmetic, division,
//! powers, parsing, `repr` and `format` follow CPython 3.14's
//! `Objects/complexobject.c` and `Python/formatter_unicode.c` (PSF
//! licence, see `THIRD_PARTY.md`), mixed-mode rules included: a real
//! operand only touches its own part.

use crate::class::Dn;
use crate::libm;
use crate::text::{Spec, float_repr};
use crate::types::{T, ty};
use crate::vm::{CallArgs, Raised, Vm};
use alloc::{format, string::String};
use mm_compile::ast::BinOp;
use mm_core::Value;

/// A complex number.
#[derive(Clone, Copy, Debug)]
pub(crate) struct C {
    pub re: f64,
    pub im: f64,
}

const ONE: C = C { re: 1.0, im: 0.0 };

/// Errors of the complex kernels (CPython's `errno`).
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum CErr {
    /// `EDOM`: division by zero.
    Dom,
    /// `ERANGE`: overflow.
    Range,
}

#[cfg_attr(mm_shipped, optimize(size))]
fn cs(x: f64, y: f64) -> f64 {
    libm::copysign(x, y)
}

#[cfg_attr(mm_shipped, optimize(size))]
fn inf01(x: f64) -> f64 {
    cs(if x.is_infinite() { 1.0 } else { 0.0 }, x)
}

#[cfg_attr(mm_shipped, optimize(size))]
fn nan0(x: f64) -> f64 {
    if x.is_nan() { cs(0.0, x) } else { x }
}

/// `_Py_c_prod`, with C11 Annex G's recovery of infinities.
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn prod(z: C, w: C) -> C {
    let (mut a, mut b, mut c, mut d) = (z.re, z.im, w.re, w.im);
    let (ac, bd, ad, bc) = (a * c, b * d, a * d, b * c);
    let mut r = C {
        re: ac - bd,
        im: ad + bc,
    };
    if r.re.is_nan() && r.im.is_nan() {
        let mut recalc = false;
        if a.is_infinite() || b.is_infinite() {
            a = inf01(a);
            b = inf01(b);
            c = nan0(c);
            d = nan0(d);
            recalc = true;
        }
        if c.is_infinite() || d.is_infinite() {
            c = inf01(c);
            d = inf01(d);
            a = nan0(a);
            b = nan0(b);
            recalc = true;
        }
        if !recalc && (ac.is_infinite() || bd.is_infinite() || ad.is_infinite() || bc.is_infinite())
        {
            a = nan0(a);
            b = nan0(b);
            c = nan0(c);
            d = nan0(d);
            recalc = true;
        }
        if recalc {
            r.re = f64::INFINITY * (a * c - b * d);
            r.im = f64::INFINITY * (a * d + b * c);
        }
    }
    r
}

/// `_Py_c_quot` (Smith's algorithm, C11 Annex G recovery).
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn quot(a: C, b: C) -> Result<C, CErr> {
    let (abs_br, abs_bi) = (b.re.abs(), b.im.abs());
    let mut r;
    if abs_br >= abs_bi {
        if abs_br == 0.0 {
            return Err(CErr::Dom);
        }
        let ratio = b.im / b.re;
        let denom = b.re + b.im * ratio;
        r = C {
            re: (a.re + a.im * ratio) / denom,
            im: (a.im - a.re * ratio) / denom,
        };
    } else if abs_bi >= abs_br {
        let ratio = b.re / b.im;
        let denom = b.re * ratio + b.im;
        r = C {
            re: (a.re * ratio + a.im) / denom,
            im: (a.im * ratio - a.re) / denom,
        };
    } else {
        r = C {
            re: f64::NAN,
            im: f64::NAN,
        };
    }
    if r.re.is_nan() && r.im.is_nan() {
        if (a.re.is_infinite() || a.im.is_infinite()) && b.re.is_finite() && b.im.is_finite() {
            let (x, y) = (inf01(a.re), inf01(a.im));
            r.re = f64::INFINITY * (x * b.re + y * b.im);
            r.im = f64::INFINITY * (y * b.re - x * b.im);
        } else if (abs_br.is_infinite() || abs_bi.is_infinite())
            && a.re.is_finite()
            && a.im.is_finite()
        {
            let (x, y) = (inf01(b.re), inf01(b.im));
            r.re = 0.0 * (a.re * x + a.im * y);
            r.im = 0.0 * (a.im * x - a.re * y);
        }
    }
    Ok(r)
}

/// `_Py_rc_quot`: a real dividend.
#[cfg_attr(mm_shipped, optimize(size))]
fn rc_quot(a: f64, b: C) -> Result<C, CErr> {
    let (abs_br, abs_bi) = (b.re.abs(), b.im.abs());
    let mut r;
    if abs_br >= abs_bi {
        if abs_br == 0.0 {
            return Err(CErr::Dom);
        }
        let ratio = b.im / b.re;
        let denom = b.re + b.im * ratio;
        r = C {
            re: a / denom,
            im: (-a * ratio) / denom,
        };
    } else if abs_bi >= abs_br {
        let ratio = b.re / b.im;
        let denom = b.re * ratio + b.im;
        r = C {
            re: (a * ratio) / denom,
            im: (-a) / denom,
        };
    } else {
        r = C {
            re: f64::NAN,
            im: f64::NAN,
        };
    }
    if r.re.is_nan()
        && r.im.is_nan()
        && a.is_finite()
        && (abs_br.is_infinite() || abs_bi.is_infinite())
    {
        let (x, y) = (inf01(b.re), inf01(b.im));
        r.re = 0.0 * (a * x);
        r.im = 0.0 * (-a * y);
    }
    Ok(r)
}

/// `hypot`, as `math.hypot` computes it.
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn hypot(x: f64, y: f64) -> f64 {
    let (ax, ay) = (x.abs(), y.abs());
    let nan = ax.is_nan() || ay.is_nan();
    let mut max = 0.0;
    for v in [ax, ay] {
        if v > max {
            max = v;
        }
    }
    libm::vector_norm(&mut [ax, ay], max, nan)
}

/// `_Py_ADJUST_ERANGE2`.
#[cfg_attr(mm_shipped, optimize(size))]
fn adjust(r: C) -> Result<C, CErr> {
    if r.re.is_infinite() || r.im.is_infinite() {
        Err(CErr::Range)
    } else {
        Ok(r)
    }
}

/// `_Py_c_pow`.
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn c_pow(a: C, b: C) -> Result<C, CErr> {
    if b.re == 0.0 && b.im == 0.0 {
        return Ok(ONE);
    }
    if a.re == 0.0 && a.im == 0.0 {
        if b.im != 0.0 || b.re < 0.0 {
            return Err(CErr::Dom);
        }
        return Ok(C { re: 0.0, im: 0.0 });
    }
    let vabs = hypot(a.re, a.im);
    let mut len = libm::pow(vabs, b.re);
    let at = libm::atan2(a.im, a.re);
    let mut phase = at * b.re;
    if b.im != 0.0 {
        len /= libm::exp(at * b.im);
        phase += b.im * libm::log(vabs);
    }
    adjust(C {
        re: len * libm::cos(phase),
        im: len * libm::sin(phase),
    })
}

#[cfg_attr(mm_shipped, optimize(size))]
fn powu(x: C, n: i64) -> C {
    let mut r = ONE;
    let mut p = x;
    let mut mask: i64 = 1;
    while mask > 0 && n >= mask {
        if n & mask != 0 {
            r = prod(r, p);
        }
        mask <<= 1;
        p = prod(p, p);
    }
    r
}

/// `complex ** complex` (`complex_pow`): small integral exponents by
/// repeated multiplication.
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn pow(a: C, b: C) -> Result<C, CErr> {
    if b.im == 0.0 && b.re == crate::ops::floor(b.re) && b.re.abs() <= 100.0 {
        #[allow(clippy::cast_possible_truncation, reason = "|b.re| <= 100")]
        let n = b.re as i64;
        let p = if n > 0 {
            powu(a, n)
        } else {
            quot(ONE, powu(a, -n))?
        };
        return adjust(p);
    }
    c_pow(a, b)
}

/// Python's text of one part of a complex `repr` (no `.0`).
#[cfg_attr(mm_shipped, optimize(size))]
fn part_repr(f: f64) -> String {
    let s = float_repr(f);
    match s.strip_suffix(".0") {
        Some(t) => String::from(t),
        None => s,
    }
}

/// `repr(complex)`.
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn repr(re: f64, im: f64) -> String {
    let imag = part_repr(im);
    if re == 0.0 && re.is_sign_positive() {
        return format!("{imag}j");
    }
    let sign = if imag.starts_with('-') { "" } else { "+" };
    format!("({}{sign}{imag}j)", part_repr(re))
}

/// CPython's hash of a complex number.
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn hash(re: f64, im: f64) -> i64 {
    let hr = crate::dict::hash_float(re).cast_unsigned();
    let hi = crate::dict::hash_float(im).cast_unsigned();
    let h = hr.wrapping_add(hi.wrapping_mul(1_000_003)).cast_signed();
    if h == -1 { -2 } else { h }
}

/// The longest prefix of `s` that is a float literal as
/// `PyOS_string_to_double` reads it: `(value, bytes used)`.
#[cfg_attr(mm_shipped, optimize(size))]
fn float_prefix(s: &[u8]) -> Option<(f64, usize)> {
    let mut i = 0;
    if matches!(s.first(), Some(b'+' | b'-')) {
        i = 1;
    }
    let rest = &s[i..];
    for word in ["infinity", "inf", "nan"] {
        if rest.len() >= word.len() && rest[..word.len()].eq_ignore_ascii_case(word.as_bytes()) {
            let v = if word == "nan" {
                f64::NAN
            } else {
                f64::INFINITY
            };
            let v = if s.first() == Some(&b'-') { -v } else { v };
            return Some((v, i + word.len()));
        }
    }
    let digits = |j: &mut usize| {
        let st = *j;
        while s.get(*j).is_some_and(u8::is_ascii_digit) {
            *j += 1;
        }
        *j - st
    };
    let mut j = i;
    let mut n = digits(&mut j);
    if s.get(j) == Some(&b'.') {
        let mut k = j + 1;
        let m = digits(&mut k);
        if n + m > 0 {
            j = k;
            n += m;
        }
    }
    if n == 0 {
        return None;
    }
    if matches!(s.get(j), Some(b'e' | b'E')) {
        let mut k = j + 1;
        if matches!(s.get(k), Some(b'+' | b'-')) {
            k += 1;
        }
        if digits(&mut k) > 0 {
            j = k;
        }
    }
    let text = core::str::from_utf8(&s[..j]).ok()?;
    Some((text.parse::<f64>().unwrap_or(f64::NAN), j))
}

/// `complex(str)` of canonical text ([`NumScan`]: no underscores);
/// `None` for a malformed string.
#[cfg_attr(mm_shipped, optimize(size))]
fn parse(s: &[u8]) -> Option<C> {
    let space = |b: u8| matches!(b, b' ' | b'\t' | b'\n' | b'\r' | b'\x0b' | b'\x0c');
    let mut i = 0;
    while i < s.len() && space(s[i]) {
        i += 1;
    }
    let bracket = s.get(i) == Some(&b'(');
    if bracket {
        i += 1;
        while i < s.len() && space(s[i]) {
            i += 1;
        }
    }
    let (mut x, mut y) = (0.0, 0.0);
    let is_j = |b: Option<&u8>| matches!(b, Some(b'j' | b'J'));
    if let Some((z, n)) = float_prefix(&s[i..]) {
        i += n;
        if matches!(s.get(i), Some(b'+' | b'-')) {
            x = z;
            if let Some((v, m)) = float_prefix(&s[i..]) {
                y = v;
                i += m;
            } else {
                y = if s[i] == b'+' { 1.0 } else { -1.0 };
                i += 1;
            }
            if !is_j(s.get(i)) {
                return None;
            }
            i += 1;
        } else if is_j(s.get(i)) {
            i += 1;
            y = z;
        } else {
            x = z;
        }
    } else {
        if matches!(s.get(i), Some(b'+' | b'-')) {
            y = if s[i] == b'+' { 1.0 } else { -1.0 };
            i += 1;
        } else {
            y = 1.0;
        }
        if !is_j(s.get(i)) {
            return None;
        }
        i += 1;
    }
    while i < s.len() && space(s[i]) {
        i += 1;
    }
    if bracket {
        if s.get(i) != Some(&b')') {
            return None;
        }
        i += 1;
        while i < s.len() && space(s[i]) {
            i += 1;
        }
    }
    if i != s.len() {
        return None;
    }
    Some(C { re: x, im: y })
}

/// One part of a formatted complex number: `(negative, digits)`.
#[cfg_attr(mm_shipped, optimize(size))]
fn fmt_part(f: f64, code: char, prec: usize, s: &Spec) -> (bool, String) {
    use crate::text::{exp_form, fixed, general};
    let mut neg = f.is_sign_negative() && !f.is_nan();
    let a = f.abs();
    let upper = matches!(code, 'E' | 'F' | 'G');
    let mut body = if a.is_nan() || a.is_infinite() {
        String::from(if a.is_nan() { "nan" } else { "inf" })
    } else {
        match code {
            'f' | 'F' => fixed(a, prec),
            'e' | 'E' => exp_form(a, prec),
            'g' | 'G' => general(a, prec, s.alt, false),
            _ => part_repr(a),
        }
    };
    if s.alt && a.is_finite() && !body.contains('.') && !matches!(code, 'g' | 'G') {
        match body.find('e') {
            Some(e) => body.insert(e, '.'),
            None => body.push('.'),
        }
    }
    if upper {
        body = body.to_uppercase();
    }
    if s.zflag && neg && body.bytes().all(|c| matches!(c, b'0' | b'.')) {
        neg = false;
    }
    s.group_fraction(&mut body);
    if let Some(g) = s.grouping {
        let k = body
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(body.len());
        let (int_part, rest) = body.split_at(k);
        body = format!("{}{rest}", crate::text::group(int_part, g, 3, 0));
    }
    (neg, body)
}

impl Vm {
    /// A new `complex(re, im)`.
    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn new_complex(&mut self, re: f64, im: f64) -> Result<Value, Raised> {
        let res = self
            .heap
            .alloc_init(ty::COMPLEX, 0, &[], &[re.to_bits(), im.to_bits()]);
        let r = self.hx_o(res)?;
        Ok(Value::from_ref(r, ty::COMPLEX as u8))
    }

    #[cfg_attr(mm_shipped, optimize(size))]
    fn c_value(&mut self, c: C) -> Result<Value, Raised> {
        self.new_complex(c.re, c.im)
    }

    /// The parts of a `complex` value.
    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn complex_of(&self, v: Value) -> Option<(f64, f64)> {
        if !v.is_obj_kind(ty::COMPLEX as u8) {
            return None;
        }
        let r = v.as_obj()?;
        Some((
            f64::from_bits(self.raw_o(r, 0)),
            f64::from_bits(self.raw_o(r, 1)),
        ))
    }

    /// Is `v` a `complex`?
    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn is_complex(v: Value) -> bool {
        v.is_obj_kind(ty::COMPLEX as u8)
    }

    /// Is `v` an int, bool or float (a real number of a builtin type)?
    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn is_number(v: Value) -> bool {
        v.is_float() || Vm::is_intlike(v)
    }

    /// Raises the exception of a failed complex kernel.
    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn c_err(&mut self, e: CErr, dom: &str, range: &str) -> Raised {
        match e {
            CErr::Dom => self.err(T::ZeroDivisionError, dom),
            CErr::Range => self.err(T::OverflowError, range),
        }
    }

    /// `a <op> b` where one operand is a complex and the other a number;
    /// `None` for operators complex lacks.
    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn complex_binop(
        &mut self,
        op: BinOp,
        a: Value,
        b: Value,
    ) -> Result<Option<Value>, Raised> {
        if !matches!(
            op,
            BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Pow
        ) {
            return Ok(None);
        }
        let (ca, cb) = (self.complex_of(a), self.complex_of(b));
        let x = match ca {
            Some((re, im)) => C { re, im },
            None => C {
                re: self.num_to_f64(a)?.unwrap_or(0.0),
                im: 0.0,
            },
        };
        let y = match cb {
            Some((re, im)) => C { re, im },
            None => C {
                re: self.num_to_f64(b)?.unwrap_or(0.0),
                im: 0.0,
            },
        };
        let (ra, rb) = (ca.is_none(), cb.is_none());
        let r = match op {
            BinOp::Add => match (ra, rb) {
                (true, _) => C {
                    re: x.re + y.re,
                    im: y.im,
                },
                (_, true) => C {
                    re: x.re + y.re,
                    im: x.im,
                },
                _ => C {
                    re: x.re + y.re,
                    im: x.im + y.im,
                },
            },
            BinOp::Sub => match (ra, rb) {
                (true, _) => C {
                    re: x.re - y.re,
                    im: -y.im,
                },
                (_, true) => C {
                    re: x.re - y.re,
                    im: x.im,
                },
                _ => C {
                    re: x.re - y.re,
                    im: x.im - y.im,
                },
            },
            BinOp::Mul => match (ra, rb) {
                (true, _) => C {
                    re: x.re * y.re,
                    im: x.re * y.im,
                },
                (_, true) => C {
                    re: x.re * y.re,
                    im: x.im * y.re,
                },
                _ => prod(x, y),
            },
            BinOp::Div => {
                let q = if rb {
                    if y.re == 0.0 {
                        Err(CErr::Dom)
                    } else {
                        Ok(C {
                            re: x.re / y.re,
                            im: x.im / y.re,
                        })
                    }
                } else if ra {
                    rc_quot(x.re, y)
                } else {
                    quot(x, y)
                };
                match q {
                    Ok(c) => c,
                    Err(_) => return Err(self.err(T::ZeroDivisionError, "division by zero")),
                }
            }
            _ => match pow(x, y) {
                Ok(c) => c,
                Err(e) => {
                    return Err(self.c_err(
                        e,
                        "zero to a negative or complex power",
                        "complex exponentiation",
                    ));
                }
            },
        };
        self.c_value(r).map(Some)
    }

    /// `x ** y` for a negative float base and a fractional exponent.
    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn real_pow_complex(&mut self, x: f64, y: f64) -> Result<Value, Raised> {
        match c_pow(C { re: x, im: 0.0 }, C { re: y, im: 0.0 }) {
            Ok(c) => self.c_value(c),
            Err(e) => Err(self.c_err(
                e,
                "zero to a negative or complex power",
                "complex exponentiation",
            )),
        }
    }

    /// `abs(z)`.
    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn complex_abs(&mut self, re: f64, im: f64) -> Result<Value, Raised> {
        let r = hypot(re, im);
        if r.is_infinite() && re.is_finite() && im.is_finite() {
            return Err(self.err(T::OverflowError, "absolute value too large"));
        }
        Ok(Value::float(r))
    }

    /// `format(z, spec)` for a non-empty spec.
    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn format_complex(
        &mut self,
        re: f64,
        im: f64,
        s: &Spec,
    ) -> Result<crate::scratch::NativeText, Raised> {
        if s.precision.unwrap_or(0) > i32::MAX as usize {
            return Err(self.err(T::ValueError, "precision too big"));
        }
        let mut skip_re = false;
        let mut parens = false;
        let mut default_prec = 6;
        let mut code = match s.ty {
            None => {
                default_prec = 0;
                if re == 0.0 && re.is_sign_positive() {
                    skip_re = true;
                } else {
                    parens = true;
                }
                'r'
            }
            Some('n') => 'g',
            Some(c @ ('e' | 'E' | 'f' | 'F' | 'g' | 'G')) => c,
            Some(c) => {
                let msg = format!("Unknown format code '{c}' for object of type 'complex'");
                return Err(self.err(T::ValueError, &msg));
            }
        };
        if s.fill == '0' {
            return Err(self.err(
                T::ValueError,
                "Zero padding is not allowed in complex format specifier",
            ));
        }
        if s.align == Some('=') {
            return Err(self.err(
                T::ValueError,
                "'=' alignment flag is not allowed in complex format specifier",
            ));
        }
        let prec = match s.precision {
            None => default_prec,
            Some(p) => {
                if code == 'r' {
                    code = 'g';
                }
                p
            }
        };
        // Non-alternate g strips excess zeros, and non-finite parts need
        // no precision-sized buffer. Count only parts that will be rendered.
        let finite_parts = u64::from(!skip_re && re.is_finite()) + u64::from(im.is_finite());
        // Held while the text is built: each part's copies (as for a
        // float) and the doubling output that joins them.
        let _native =
            if prec > 1100 && (matches!(code, 'f' | 'F' | 'e' | 'E') || s.alt) && finite_parts != 0
            {
                let bytes = prec.saturating_mul(6 * finite_parts as usize);
                let res = self.heap.reserve_scratch(bytes);
                Some(self.hx_o(res)?)
            } else {
                None
            };
        let mut out = String::new();
        if parens {
            out.push('(');
        }
        let sign_of = |neg: bool, sign: Option<char>| match (neg, sign) {
            (true, _) => "-",
            (false, Some('+')) => "+",
            (false, Some(' ')) => " ",
            _ => "",
        };
        if !skip_re {
            let (neg, body) = fmt_part(re, code, prec, s);
            out.push_str(sign_of(neg, s.sign));
            out.push_str(&body);
        }
        let (neg, body) = fmt_part(im, code, prec, s);
        let isign = if skip_re { s.sign } else { Some('+') };
        out.push_str(sign_of(neg, isign));
        out.push_str(&body);
        out.push('j');
        if parens {
            out.push(')');
        }
        s.pad(self, &out, '>')
    }
}

// ------------------------------------------------ number text, in steps

/// What a number text step reads at most (docs/spec.md §5, bounded native
/// steps), an ASCII byte counting one and any other char [`WIDE`]: a few
/// milliseconds of scanning.
pub(crate) const TEXT_STEP: usize = 1 << 21;

/// What a non-ASCII char counts towards [`TEXT_STEP`] (decoding and
/// mapping one takes about 30 times an ASCII byte's time [M]).
pub(crate) const WIDE: usize = 32;

/// Canonical text a [`NumScan`] writes at most: a valid one is at most two
/// literals of `KEEP + 1` digits and an exponent, with a few signs,
/// spaces, brackets and letters; longer is malformed.
const CANON: usize = 2048;

/// Significant digits a literal keeps: correct rounding needs at most 768
/// of them, and any later nonzero digit only as such (kept as one `1`).
const KEEP: u16 = 800;

/// Texts whose error message shows their whole repr natively; longer ones
/// raise in `_num_error`, whose repr and join go in steps.
const SHORT_REPR: usize = 1 << 16;

/// The ASCII byte a number parser reads for the non-ASCII char at `text[i]`,
/// and the char's width: for str CPython's
/// `_PyUnicode_TransformDecimalAndSpaceToASCII` (whitespace a space,
/// decimal digits theirs), anything else (and any non-ASCII byte of
/// bytes) 0x80, which no number grammar accepts.
#[cold]
#[inline(never)]
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn wide_char(text: &[u8], i: usize, is_str: bool) -> (u8, usize) {
    let b = text.get(i).copied().unwrap_or(0);
    if !is_str {
        return (0x80, 1);
    }
    let w = match b {
        0xF0.. => 4,
        0xE0.. => 3,
        _ => 2,
    };
    let c = text
        .get(i..i + w)
        .and_then(|s| core::str::from_utf8(s).ok())
        .and_then(|s| s.chars().next());
    let m = match c {
        Some(c) if c.is_whitespace() => b' ',
        Some(c) => crate::text::decimal_value(c).map_or(0x80, |d| b'0' + d as u8),
        None => 0x80,
    };
    (m, w)
}

/// The ASCII whitespace the number parsers skip.
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn num_space(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | b'\r' | b'\x0b' | b'\x0c')
}

/// A number grammar read a char at a time ([`NumScan`], `intops::IntScan`):
/// the per-op part of the chunked text engine ([`scan_text`],
/// [`Vm::text_step`]). Five words of state, so it parks between steps.
pub(crate) trait TextFeed {
    /// Reads `c` (an ASCII char, or [`wide_char`]'s), `w` bytes at `i`;
    /// `buf` is the op's output block.
    fn feed(&mut self, c: u8, i: usize, w: usize, buf: &mut [u8]);
    /// Whether the outcome is known, so the rest need not be read.
    fn done(&self) -> bool;
    fn words(&self) -> [u64; 5];
}

/// Reads the chars of `text` from `*at` into `sc` for at most `budget`
/// (see [`TEXT_STEP`]), stopping early once it is done.
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn scan_text<S: TextFeed>(
    sc: &mut S,
    text: &[u8],
    is_str: bool,
    at: &mut usize,
    budget: usize,
    buf: &mut [u8],
) {
    let mut i = *at;
    let mut spent = 0;
    while i < text.len() && spent < budget && !sc.done() {
        let (c, w) = match text.get(i) {
            Some(&b) if b < 0x80 => (b, 1),
            _ => {
                spent += WIDE - 1;
                wide_char(text, i, is_str)
            }
        };
        sc.feed(c, i, w, buf);
        i += w;
        spent += 1;
    }
    *at = i.max(*at);
}

/// `float()` and `complex()` text, reduced a piece at a time to a short
/// canonical text that [`float_text`] or [`parse`] read to the same value
/// or error: whitespace runs as one space, underscores checked and
/// dropped, each literal as its first [`KEEP`] significant digits (a `1`
/// after them if a later one is nonzero) and one decimal exponent. Any
/// other char is copied, so the canonical text is malformed exactly when
/// the text is; where that is already certain (a `.` or `e` that no
/// literal can continue with, or text past [`CANON`]) it records so and
/// writes no more. Five words of state, so it parks between steps.
#[derive(Clone, Copy, Default)]
pub(crate) struct NumScan {
    phase: u8,
    flags: u16,
    out: u16,
    kept: u16,
    frac: u64,
    dropped: u64,
    exp: u64,
}

/// [`NumScan`] phases: outside a literal, in its integer digits, its
/// fraction, after its `e`, after the exponent's sign, in its digits.
const OUT: u8 = 0;
const INT: u8 = 1;
const FRAC: u8 = 2;
const ESTART: u8 = 3;
const ESIGN: u8 = 4;
const EDIG: u8 = 5;

/// [`NumScan`] flags.
const SIG: u16 = 1;
const STICKY: u16 = 2;
const DIGIT: u16 = 4;
const PREV: u16 = 8;
const US: u16 = 16;
const ENEG: u16 = 32;
/// An underscore not between two digits: `could not convert`.
const BAD_US: u16 = 64;
/// Malformed.
const BAD: u16 = 128;
/// `complex()` text, not `float()`.
const COMPLEX: u16 = 256;

impl TextFeed for NumScan {
    #[cfg_attr(mm_shipped, optimize(size))]
    fn feed(&mut self, c: u8, _: usize, _: usize, buf: &mut [u8]) {
        self.feed(c, buf);
    }

    #[cfg_attr(mm_shipped, optimize(size))]
    fn done(&self) -> bool {
        self.decided()
    }

    #[cfg_attr(mm_shipped, optimize(size))]
    fn words(&self) -> [u64; 5] {
        [
            u64::from(self.phase)
                | u64::from(self.flags) << 8
                | u64::from(self.out) << 24
                | u64::from(self.kept) << 40,
            self.frac,
            self.dropped,
            self.exp,
            0,
        ]
    }
}

impl NumScan {
    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn from_words(w: [u64; 5]) -> NumScan {
        NumScan {
            phase: w[0] as u8,
            flags: (w[0] >> 8) as u16,
            out: (w[0] >> 24) as u16,
            kept: (w[0] >> 40) as u16,
            frac: w[1],
            dropped: w[2],
            exp: w[3],
        }
    }

    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn new(complex: bool) -> NumScan {
        NumScan {
            flags: if complex { COMPLEX } else { 0 },
            ..NumScan::default()
        }
    }

    #[cfg_attr(mm_shipped, optimize(size))]
    fn has(&self, f: u16) -> bool {
        self.flags & f != 0
    }

    /// Whether the outcome is known: an underscore error, or for `float()`
    /// any error.
    #[cfg_attr(mm_shipped, optimize(size))]
    fn decided(&self) -> bool {
        self.has(BAD_US) || (!self.has(COMPLEX) && self.has(BAD))
    }

    #[cfg_attr(mm_shipped, optimize(size))]
    fn emit(&mut self, buf: &mut [u8], c: u8) {
        match buf.get_mut(usize::from(self.out)) {
            Some(b) => {
                *b = c;
                self.out += 1;
            }
            None => self.flags |= BAD,
        }
    }

    #[cfg_attr(mm_shipped, optimize(size))]
    fn feed(&mut self, c: u8, buf: &mut [u8]) {
        let digit = c.is_ascii_digit();
        if self.has(US) {
            self.flags &= !US;
            if !digit {
                self.flags |= BAD_US;
                return;
            }
        }
        if c == b'_' {
            self.flags |= if self.has(PREV) { US } else { BAD_US };
            self.flags &= !PREV;
            return;
        }
        if digit {
            self.flags |= PREV;
        } else {
            self.flags &= !PREV;
        }
        if self.has(BAD) {
            return;
        }
        let e = c | 0x20 == b'e';
        match self.phase {
            INT | FRAC if digit => {
                self.digit(c - b'0', buf);
                if self.phase == FRAC {
                    self.frac = self.frac.saturating_add(1);
                }
                return;
            }
            INT if c == b'.' => {
                self.phase = FRAC;
                return;
            }
            INT | FRAC if e && self.has(DIGIT) => {
                self.phase = ESTART;
                return;
            }
            ESTART if c == b'+' || c == b'-' => {
                if c == b'-' {
                    self.flags |= ENEG;
                }
                self.phase = ESIGN;
                return;
            }
            ESTART | ESIGN | EDIG if digit => {
                self.phase = EDIG;
                self.exp = (self.exp * 10 + u64::from(c - b'0')).min(1 << 40);
                return;
            }
            // No literal goes on after these, and none can start right
            // after a literal: malformed.
            ESTART | ESIGN => {
                self.flags |= BAD;
                return;
            }
            FRAC | EDIG if c == b'.' || e => {
                self.flags |= BAD;
                return;
            }
            INT | FRAC | EDIG => self.end_literal(buf),
            _ => {}
        }
        if self.has(BAD) {
            return;
        }
        if digit {
            self.phase = INT;
            self.digit(c - b'0', buf);
        } else if c == b'.' {
            self.phase = FRAC;
        } else if num_space(c) {
            if self.out == 0 || buf.get(usize::from(self.out) - 1) != Some(&b' ') {
                self.emit(buf, b' ');
            }
        } else {
            self.emit(buf, if c < 0x80 { c } else { b'?' });
        }
    }

    #[cfg_attr(mm_shipped, optimize(size))]
    fn digit(&mut self, d: u8, buf: &mut [u8]) {
        self.flags |= DIGIT;
        if d == 0 && !self.has(SIG) {
            return;
        }
        self.flags |= SIG;
        if self.kept < KEEP {
            self.emit(buf, b'0' + d);
            self.kept += 1;
        } else {
            self.dropped = self.dropped.saturating_add(1);
            if d != 0 {
                self.flags |= STICKY;
            }
        }
    }

    /// Writes the end of the literal just read: `0`, or its exponent.
    #[cfg_attr(mm_shipped, optimize(size))]
    fn end_literal(&mut self, buf: &mut [u8]) {
        if !self.has(DIGIT) {
            // A `.` alone.
            self.flags |= BAD;
            return;
        }
        if self.has(SIG) {
            let sticky = self.has(STICKY);
            if sticky {
                self.emit(buf, b'1');
            }
            let exp = i128::from(self.exp);
            let e = if self.has(ENEG) { -exp } else { exp } + i128::from(self.dropped)
                - i128::from(sticky)
                - i128::from(self.frac);
            // Past these every value is infinite or zero.
            let e = e.clamp(-100_000, 100_000) as i64;
            self.emit(buf, b'e');
            let mut digits = [0u8; 20];
            let k = crate::text::i64_digits(e, &mut digits);
            for &d in digits.get(k..).unwrap_or(&[]) {
                self.emit(buf, d);
            }
        } else {
            self.emit(buf, b'0');
        }
        self.phase = OUT;
        self.flags &= !(SIG | STICKY | DIGIT | ENEG);
        self.kept = 0;
        (self.frac, self.dropped, self.exp) = (0, 0, 0);
    }

    /// The end of the text.
    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn finish(&mut self, buf: &mut [u8]) {
        if self.has(US) {
            self.flags |= BAD_US;
        }
        if self.has(BAD) || self.has(BAD_US) {
            return;
        }
        match self.phase {
            INT | FRAC | EDIG => self.end_literal(buf),
            ESTART | ESIGN => self.flags |= BAD,
            _ => {}
        }
    }
}

/// `float()` of canonical text: surrounding whitespace, a sign, then a
/// literal or `inf`/`infinity`/`nan`.
#[cfg_attr(mm_shipped, optimize(size))]
fn float_text(s: &[u8]) -> Option<f64> {
    let t = core::str::from_utf8(s)
        .ok()?
        .trim_matches(|c: char| num_space(c as u8));
    let (neg, body) = match t.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, t.strip_prefix('+').unwrap_or(t)),
    };
    let lower = body.to_ascii_lowercase();
    let f = match lower.as_str() {
        "inf" | "infinity" => f64::INFINITY,
        "nan" => f64::NAN,
        _ if lower.starts_with(['+', '-']) || lower.contains(['i', 'n']) => return None,
        _ => body.parse::<f64>().ok()?,
    };
    Some(if neg { -f } else { f })
}

impl Vm {
    /// The text of a str (`true`) or bytes-like value.
    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn number_text(&self, v: Value) -> (&[u8], bool) {
        if Vm::is_str(v) {
            (self.str_bytes(v), true)
        } else {
            (self.bytes_data(v), false)
        }
    }

    /// `float(s)` (`complex(s)` with `complex`) for str or bytes-like `s`:
    /// its first step, the rest in `_steps` (`nstep::op::NUM_TEXT`) when
    /// the text is long.
    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn num_from_text(&mut self, v: Value, complex: bool) -> Result<Value, Raised> {
        let mut buf = [0u8; CANON];
        let mut sc = NumScan::new(complex);
        let mut at = 0;
        if self.text_part(v, &mut sc, &mut at, &mut buf) {
            let block = {
                let res = self.heap.alloc_bytes(&buf);
                self.hx_o(res)
            }?;
            return self.text_rest(v, 0, Value::from_byte_ref(block), at, sc.words());
        }
        self.num_text_done(v, sc, &mut buf)
    }

    /// Reads `v`'s text from `*at` into `sc`, a step's worth: whether
    /// more is left to read.
    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn text_part<S: TextFeed>(
        &mut self,
        v: Value,
        sc: &mut S,
        at: &mut usize,
        buf: &mut [u8],
    ) -> bool {
        let start = *at;
        let (text, is_str) = self.number_text(v);
        let len = text.len();
        scan_text(sc, text, is_str, at, TEXT_STEP, buf);
        self.burn_bytes(*at - start);
        *at < len && !sc.done()
    }

    /// The rest of a long number text in `_steps`: `kind` 0 for
    /// `float()`/`complex()`, else 1 + `int()`'s base; `block` its output.
    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn text_rest(
        &mut self,
        v: Value,
        kind: u64,
        block: Value,
        at: usize,
        w: [u64; 5],
    ) -> Result<Value, Raised> {
        let raw = [kind, at as u64, w[0], w[1], w[2], w[3], w[4], 0, 0];
        self.chunk_rest(crate::nstep::op::NUM_TEXT, &[v, block], &raw)
    }

    /// Saves a long number text's progress in `state`: where it reads
    /// next, its scanner's words, and a conversion's progress.
    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn text_save(&mut self, state: Value, at: usize, w: [u64; 5], conv: [u64; 2]) {
        let words = [at as u64, w[0], w[1], w[2], w[3], w[4], conv[0], conv[1]];
        self.nstate_put(state, 2, &words);
    }

    /// One step of a long number text (`nstep::op::NUM_TEXT`).
    #[inline(never)]
    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn text_step(&mut self, state: Value) -> Result<crate::nstep::Step, Raised> {
        use crate::nstep::{Step, op};
        let Some(([v, block], [_, kind, at, w0, w1, w2, w3, w4, idx, len])) =
            self.nstate_get::<2, 10>(op::NUM_TEXT, state)
        else {
            return Err(self.bad_step());
        };
        let w = [w0, w1, w2, w3, w4];
        if kind == crate::builtins::DIGITS {
            return self.digits_step(state, v, block, at, w);
        }
        // Only a str or bytes-like text reads as one; any other value from
        // a forged image would read as no text, parking for ever.
        if !(Vm::is_str(v) || Vm::is_bytes_like(v)) {
            return Err(self.bad_step());
        }
        let mut at = usize::try_from(at).unwrap_or(usize::MAX);
        if kind > 0 {
            return self.int_text_step(state, v, block, kind - 1, at, w, [idx, len]);
        }
        let Some(block) = block.as_byte_ref() else {
            return Err(self.bad_step());
        };
        let mut buf = [0u8; CANON];
        match self.heap.bytes(block) {
            Ok(b) if b.len() == CANON => buf.copy_from_slice(b),
            _ => return Err(self.bad_step()),
        }
        let mut sc = NumScan::from_words(w);
        if self.text_part(v, &mut sc, &mut at, &mut buf) {
            if let Ok(b) = self.heap.bytes_mut(block) {
                b.copy_from_slice(&buf);
            }
            self.text_save(state, at, sc.words(), [0, 0]);
            return Ok(Step::More);
        }
        self.num_text_done(v, sc, &mut buf).map(Step::Done)
    }

    /// The value of a scanned text, or its error.
    #[cfg_attr(mm_shipped, optimize(size))]
    fn num_text_done(
        &mut self,
        v: Value,
        mut sc: NumScan,
        buf: &mut [u8],
    ) -> Result<Value, Raised> {
        let complex = sc.has(COMPLEX);
        sc.finish(buf);
        let canon = buf.get(..usize::from(sc.out)).unwrap_or(&[]);
        if !sc.has(BAD_US) && !sc.has(BAD) {
            if complex {
                if let Some(c) = parse(canon) {
                    return self.new_complex(c.re, c.im);
                }
            } else if let Some(f) = float_text(canon) {
                return Ok(Value::float(f));
            }
        }
        if complex && !sc.has(BAD_US) {
            return Err(self.err(T::ValueError, "complex() arg is a malformed string"));
        }
        let what = if complex { "complex" } else { "float" };
        if self.number_text(v).0.len() > SHORT_REPR {
            let w = self.intern(what)?;
            return self.prelude_request("_num_error", alloc::vec![v, w], crate::cont::Cont::None);
        }
        let shown = self.repr_whole(v)?;
        let msg = format!("could not convert string to {what}: {shown}");
        Err(self.err(T::ValueError, &msg))
    }
}

// ------------------------------------------------------------- builtins

/// `complex(real=0, imag=0)`.
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn bi_complex(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let r = vm.arg_o(&a, 0);
    let i = vm.arg_o(&a, 1);
    if r == Value::UNDEFINED && i == Value::UNDEFINED {
        return vm.new_complex(0.0, 0.0);
    }
    if i == Value::UNDEFINED && Vm::is_complex(r) && vm.user_class(r).is_none() {
        return Ok(r);
    }
    if Vm::is_str(r) {
        if i != Value::UNDEFINED || a.kw != u32::MAX {
            return Err(vm.err(
                T::TypeError,
                "complex() argument 'real' must be a real number, not str",
            ));
        }
        return vm.num_from_text(r, true);
    }
    let (cr, cr_complex) = vm.complex_arg(r, i == Value::UNDEFINED, "real")?;
    let (ci, ci_complex) = if i == Value::UNDEFINED {
        (C { re: cr.im, im: 0.0 }, false)
    } else {
        vm.complex_arg(i, false, "imag")?
    };
    let mut re = cr.re;
    let mut im = ci.re;
    if ci_complex {
        re -= ci.im;
    }
    if cr_complex && i != Value::UNDEFINED {
        im += cr.im;
    }
    vm.new_complex(re, im)
}

impl Vm {
    /// One argument of `complex()`: its value and whether it was complex.
    #[cfg_attr(mm_shipped, optimize(size))]
    fn complex_arg(&mut self, v: Value, only: bool, name: &str) -> Result<(C, bool), Raised> {
        if v == Value::UNDEFINED {
            return Ok((C { re: 0.0, im: 0.0 }, false));
        }
        if let Some((re, im)) = self.complex_of(v) {
            return Ok((C { re, im }, true));
        }
        if let Some(f) = self.num_to_f64(v)? {
            return Ok((C { re: f, im: 0.0 }, false));
        }
        if self.user_class(v).is_some()
            && (self.user_method(v, Dn::Complex).is_some()
                || self.user_method(v, Dn::Float).is_some()
                || self.user_method(v, Dn::Index).is_some() && !self.num_backed(v))
        {
            return Err(self.need_py());
        }
        if let Some(b) = self.backing(v) {
            return self.complex_arg(b, only, name);
        }
        let msg = if only && name == "real" {
            format!(
                "complex() argument must be a string or a number, not {}",
                self.type_name(v)
            )
        } else {
            format!(
                "complex() argument '{name}' must be a real number, not {}",
                self.type_name(v)
            )
        };
        Err(self.err(T::TypeError, &msg))
    }
}

/// `complex.from_number(x)`.
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn complex_from_number(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    if a.n != 2 {
        let msg = format!(
            "complex.from_number() takes exactly one argument ({} given)",
            a.n.saturating_sub(1)
        );
        return Err(vm.err(T::TypeError, &msg));
    }
    let v = vm.arg_o(&a, 1);
    if Vm::is_str(v) {
        let msg = format!("must be real number, not {}", vm.type_name(v));
        return Err(vm.err(T::TypeError, &msg));
    }
    let (c, _) = vm.complex_arg(v, true, "real")?;
    vm.new_complex(c.re, c.im)
}

/// `complex.conjugate()`.
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn complex_conjugate(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let (re, im) = vm.complex_of(vm.arg_o(&a, 0)).unwrap_or((0.0, 0.0));
    vm.new_complex(re, -im)
}

/// `complex.__complex__()`.
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn complex_complex(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let v = vm.arg_o(&a, 0);
    let (re, im) = vm.complex_of(v).unwrap_or((0.0, 0.0));
    vm.new_complex(re, im)
}
