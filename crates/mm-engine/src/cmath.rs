//! The `cmath` module (native, docs/spec.md §12): a port of CPython 3.14's
//! `Modules/cmathmodule.c` (PSF licence, see `THIRD_PARTY.md`), special
//! value tables (C99 Annex G) and `errno` rules included.

use crate::complex::{C, CErr, hypot, quot};
use crate::libm;
use crate::ops::sqrt;
use crate::types::T;
use crate::vm::{CallArgs, Raised, Vm};
use mm_core::Value;

const PI: f64 = core::f64::consts::PI;
const LN2: f64 = core::f64::consts::LN_2;
const LN10: f64 = core::f64::consts::LN_10;
const E: f64 = core::f64::consts::E;
/// `CM_LARGE_DOUBLE`.
const LARGE: f64 = f64::MAX / 4.0;
/// `log(CM_LARGE_DOUBLE)`.
const LOG_LARGE: f64 = 708.389_334_568_083_6;
/// `sqrt(CM_LARGE_DOUBLE)`.
const SQRT_LARGE: f64 = 6.703_903_964_971_298e153;
/// `sqrt(DBL_MIN)`.
const SQRT_DBL_MIN: f64 = 1.491_668_146_240_041_3e-154;
const SCALE_UP: i64 = 53;
const SCALE_DOWN: i64 = -27;

/// The value of one 4-bit special-table code.
#[cfg_attr(mm_shipped, optimize(size))]
fn code(c: u8) -> f64 {
    const V: [f64; 15] = [
        0.0,
        -0.0,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NAN,
        PI,
        -PI,
        PI / 2.0,
        -PI / 2.0,
        PI / 4.0,
        -PI / 4.0,
        0.75 * PI,
        -0.75 * PI,
        1.0,
        -1.0,
    ];
    V.get(usize::from(c)).copied().unwrap_or(f64::NAN)
}

/// `special_type`: the row/column of a special value table.
#[cfg_attr(mm_shipped, optimize(size))]
fn st(d: f64) -> usize {
    if d.is_nan() {
        6
    } else if d.is_infinite() {
        if d > 0.0 { 5 } else { 0 }
    } else if d != 0.0 {
        if d > 0.0 { 4 } else { 1 }
    } else if d.is_sign_negative() {
        2
    } else {
        3
    }
}

#[derive(Clone, Copy)]
enum Tab {
    Acos,
    Acosh,
    Asinh,
    Atanh,
    Cosh,
    Exp,
    Log,
    Sinh,
    Sqrt,
    Tanh,
    Rect,
}

/// The special value tables, one byte (real code << 4 | imag code) per
/// entry; CPython's placeholder `U` entries (unreachable) are NaN.
#[rustfmt::skip]
const TABLES: [[u8; 49]; 11] = [
    [0xb2, 0x52, 0x52, 0x53, 0x53, 0xb3, 0x42, 0x72, 0x44, 0x44, 0x44, 0x44, 0x73, 0x44,
     0x72, 0x44, 0x70, 0x71, 0x44, 0x73, 0x74, 0x72, 0x44, 0x70, 0x71, 0x44, 0x73, 0x74,
     0x72, 0x44, 0x44, 0x44, 0x44, 0x73, 0x44, 0x92, 0x02, 0x02, 0x03, 0x03, 0x93, 0x42,
     0x42, 0x44, 0x44, 0x44, 0x44, 0x43, 0x44],
    [0x2c, 0x26, 0x26, 0x25, 0x25, 0x2b, 0x24, 0x28, 0x44, 0x44, 0x44, 0x44, 0x27, 0x44,
     0x28, 0x44, 0x08, 0x07, 0x44, 0x27, 0x47, 0x28, 0x44, 0x08, 0x07, 0x44, 0x27, 0x47,
     0x28, 0x44, 0x44, 0x44, 0x44, 0x27, 0x44, 0x2a, 0x21, 0x21, 0x20, 0x20, 0x29, 0x24,
     0x24, 0x44, 0x44, 0x44, 0x44, 0x24, 0x44],
    [0x3a, 0x31, 0x31, 0x30, 0x30, 0x39, 0x34, 0x38, 0x44, 0x44, 0x44, 0x44, 0x37, 0x44,
     0x38, 0x44, 0x11, 0x10, 0x44, 0x37, 0x44, 0x28, 0x44, 0x01, 0x00, 0x44, 0x27, 0x44,
     0x28, 0x44, 0x44, 0x44, 0x44, 0x27, 0x44, 0x2a, 0x21, 0x21, 0x20, 0x20, 0x29, 0x24,
     0x24, 0x44, 0x41, 0x40, 0x44, 0x24, 0x44],
    [0x18, 0x18, 0x18, 0x17, 0x17, 0x17, 0x14, 0x18, 0x44, 0x44, 0x44, 0x44, 0x17, 0x44,
     0x18, 0x44, 0x11, 0x10, 0x44, 0x17, 0x14, 0x08, 0x44, 0x01, 0x00, 0x44, 0x07, 0x04,
     0x08, 0x44, 0x44, 0x44, 0x44, 0x07, 0x44, 0x08, 0x08, 0x08, 0x07, 0x07, 0x07, 0x04,
     0x08, 0x44, 0x44, 0x44, 0x44, 0x07, 0x44],
    [0x24, 0x44, 0x20, 0x21, 0x44, 0x24, 0x24, 0x44, 0x44, 0x44, 0x44, 0x44, 0x44, 0x44,
     0x40, 0x44, 0xd0, 0xd1, 0x44, 0x40, 0x40, 0x40, 0x44, 0xd1, 0xd0, 0x44, 0x40, 0x40,
     0x44, 0x44, 0x44, 0x44, 0x44, 0x44, 0x44, 0x24, 0x44, 0x21, 0x20, 0x44, 0x24, 0x24,
     0x44, 0x44, 0x40, 0x40, 0x44, 0x44, 0x44],
    [0x00, 0x44, 0x01, 0x00, 0x44, 0x00, 0x00, 0x44, 0x44, 0x44, 0x44, 0x44, 0x44, 0x44,
     0x44, 0x44, 0xd1, 0xd0, 0x44, 0x44, 0x44, 0x44, 0x44, 0xd1, 0xd0, 0x44, 0x44, 0x44,
     0x44, 0x44, 0x44, 0x44, 0x44, 0x44, 0x44, 0x24, 0x44, 0x21, 0x20, 0x44, 0x24, 0x24,
     0x44, 0x44, 0x41, 0x40, 0x44, 0x44, 0x44],
    [0x2c, 0x26, 0x26, 0x25, 0x25, 0x2b, 0x24, 0x28, 0x44, 0x44, 0x44, 0x44, 0x27, 0x44,
     0x28, 0x44, 0x36, 0x35, 0x44, 0x27, 0x44, 0x28, 0x44, 0x31, 0x30, 0x44, 0x27, 0x44,
     0x28, 0x44, 0x44, 0x44, 0x44, 0x27, 0x44, 0x2a, 0x21, 0x21, 0x20, 0x20, 0x29, 0x24,
     0x24, 0x44, 0x44, 0x44, 0x44, 0x24, 0x44],
    [0x24, 0x44, 0x31, 0x30, 0x44, 0x24, 0x24, 0x44, 0x44, 0x44, 0x44, 0x44, 0x44, 0x44,
     0x04, 0x44, 0x11, 0x10, 0x44, 0x04, 0x04, 0x04, 0x44, 0x01, 0x00, 0x44, 0x04, 0x04,
     0x44, 0x44, 0x44, 0x44, 0x44, 0x44, 0x44, 0x24, 0x44, 0x21, 0x20, 0x44, 0x24, 0x24,
     0x44, 0x44, 0x41, 0x40, 0x44, 0x44, 0x44],
    [0x23, 0x03, 0x03, 0x02, 0x02, 0x22, 0x42, 0x23, 0x44, 0x44, 0x44, 0x44, 0x22, 0x44,
     0x23, 0x44, 0x01, 0x00, 0x44, 0x22, 0x44, 0x23, 0x44, 0x01, 0x00, 0x44, 0x22, 0x44,
     0x23, 0x44, 0x44, 0x44, 0x44, 0x22, 0x44, 0x23, 0x21, 0x21, 0x20, 0x20, 0x22, 0x24,
     0x23, 0x44, 0x44, 0x44, 0x44, 0x22, 0x44],
    [0xe0, 0x44, 0xe1, 0xe0, 0x44, 0xe0, 0xe0, 0x44, 0x44, 0x44, 0x44, 0x44, 0x44, 0x44,
     0x14, 0x44, 0x11, 0x10, 0x44, 0x14, 0x14, 0x04, 0x44, 0x01, 0x00, 0x44, 0x04, 0x04,
     0x44, 0x44, 0x44, 0x44, 0x44, 0x44, 0x44, 0xd0, 0x44, 0xd1, 0xd0, 0x44, 0xd0, 0xd0,
     0x44, 0x44, 0x41, 0x40, 0x44, 0x44, 0x44],
    [0x24, 0x44, 0x30, 0x31, 0x44, 0x24, 0x24, 0x44, 0x44, 0x44, 0x44, 0x44, 0x44, 0x44,
     0x00, 0x44, 0x10, 0x11, 0x44, 0x00, 0x00, 0x00, 0x44, 0x01, 0x00, 0x44, 0x00, 0x00,
     0x44, 0x44, 0x44, 0x44, 0x44, 0x44, 0x44, 0x24, 0x44, 0x21, 0x20, 0x44, 0x24, 0x24,
     0x44, 0x44, 0x40, 0x40, 0x44, 0x44, 0x44],
];

/// `table[special_type(re)][special_type(im)]`.
#[cfg_attr(mm_shipped, optimize(size))]
fn special(t: Tab, re: f64, im: f64) -> C {
    let b = TABLES[t as usize][st(re) * 7 + st(im)];
    C {
        re: code(b >> 4),
        im: code(b & 15),
    }
}

#[cfg_attr(mm_shipped, optimize(size))]
fn finite(z: C) -> bool {
    z.re.is_finite() && z.im.is_finite()
}

#[cfg_attr(mm_shipped, optimize(size))]
fn cs(x: f64, y: f64) -> f64 {
    libm::copysign(x, y)
}

#[cfg_attr(mm_shipped, optimize(size))]
fn neg(z: C) -> C {
    C {
        re: -z.re,
        im: -z.im,
    }
}

/// `z * i` rotated forward (`(-im, re)`) and back (`(im, -re)`).
#[cfg_attr(mm_shipped, optimize(size))]
fn rot(z: C) -> C {
    C {
        re: -z.im,
        im: z.re,
    }
}

#[cfg_attr(mm_shipped, optimize(size))]
fn unrot(z: C) -> C {
    C {
        re: z.im,
        im: -z.re,
    }
}

/// A kernel's result: `Err(Dom)` is "math domain error", `Err(Range)`
/// "math range error".
type R = Result<C, CErr>;

#[cfg_attr(mm_shipped, optimize(size))]
fn c_sqrt(z: C) -> C {
    if !finite(z) {
        return special(Tab::Sqrt, z.re, z.im);
    }
    if z.re == 0.0 && z.im == 0.0 {
        return C { re: 0.0, im: z.im };
    }
    let (mut ax, ay) = (z.re.abs(), z.im.abs());
    let ldexp = crate::intops::ldexp;
    let s = if ax < f64::MIN_POSITIVE && ay < f64::MIN_POSITIVE {
        ax = ldexp(ax, SCALE_UP);
        ldexp(sqrt(ax + hypot(ax, ldexp(ay, SCALE_UP))), SCALE_DOWN)
    } else {
        ax /= 8.0;
        2.0 * sqrt(ax + hypot(ax, ay / 8.0))
    };
    let d = ay / (2.0 * s);
    if z.re >= 0.0 {
        C {
            re: s,
            im: cs(d, z.im),
        }
    } else {
        C {
            re: d,
            im: cs(s, z.im),
        }
    }
}

#[cfg_attr(mm_shipped, optimize(size))]
fn c_acos(z: C) -> C {
    if !finite(z) {
        return special(Tab::Acos, z.re, z.im);
    }
    if z.re.abs() > LARGE || z.im.abs() > LARGE {
        C {
            re: libm::atan2(z.im.abs(), z.re),
            im: -cs(libm::log(hypot(z.re / 2.0, z.im / 2.0)) + LN2 * 2.0, z.im),
        }
    } else {
        let s1 = c_sqrt(C {
            re: 1.0 - z.re,
            im: -z.im,
        });
        let s2 = c_sqrt(C {
            re: 1.0 + z.re,
            im: z.im,
        });
        C {
            re: 2.0 * libm::atan2(s1.re, s2.re),
            im: libm::asinh(s2.re * s1.im - s2.im * s1.re),
        }
    }
}

#[cfg_attr(mm_shipped, optimize(size))]
fn c_acosh(z: C) -> C {
    if !finite(z) {
        return special(Tab::Acosh, z.re, z.im);
    }
    if z.re.abs() > LARGE || z.im.abs() > LARGE {
        C {
            re: libm::log(hypot(z.re / 2.0, z.im / 2.0)) + LN2 * 2.0,
            im: libm::atan2(z.im, z.re),
        }
    } else {
        let s1 = c_sqrt(C {
            re: z.re - 1.0,
            im: z.im,
        });
        let s2 = c_sqrt(C {
            re: z.re + 1.0,
            im: z.im,
        });
        C {
            re: libm::asinh(s1.re * s2.re + s1.im * s2.im),
            im: 2.0 * libm::atan2(s1.im, s2.re),
        }
    }
}

#[cfg_attr(mm_shipped, optimize(size))]
fn c_asinh(z: C) -> C {
    if !finite(z) {
        return special(Tab::Asinh, z.re, z.im);
    }
    if z.re.abs() > LARGE || z.im.abs() > LARGE {
        let l = libm::log(hypot(z.re / 2.0, z.im / 2.0)) + LN2 * 2.0;
        C {
            re: if z.im >= 0.0 {
                cs(l, z.re)
            } else {
                -cs(l, -z.re)
            },
            im: libm::atan2(z.im, z.re.abs()),
        }
    } else {
        let s1 = c_sqrt(C {
            re: 1.0 + z.im,
            im: -z.re,
        });
        let s2 = c_sqrt(C {
            re: 1.0 - z.im,
            im: z.re,
        });
        C {
            re: libm::asinh(s1.re * s2.im - s2.re * s1.im),
            im: libm::atan2(z.im, s1.re * s2.re - s1.im * s2.im),
        }
    }
}

#[cfg_attr(mm_shipped, optimize(size))]
fn c_atanh(z: C) -> R {
    if !finite(z) {
        return Ok(special(Tab::Atanh, z.re, z.im));
    }
    if z.re < 0.0 {
        return c_atanh(neg(z)).map(neg);
    }
    let ay = z.im.abs();
    if z.re > SQRT_LARGE || ay > SQRT_LARGE {
        let h = hypot(z.re / 2.0, z.im / 2.0);
        Ok(C {
            re: z.re / 4.0 / h / h,
            im: cs(PI / 2.0, z.im),
        })
    } else if z.re == 1.0 && ay < SQRT_DBL_MIN {
        if ay == 0.0 {
            Err(CErr::Dom)
        } else {
            Ok(C {
                re: -libm::log(sqrt(ay) / sqrt(hypot(ay, 2.0))),
                im: cs(libm::atan2(2.0, -ay) / 2.0, z.im),
            })
        }
    } else {
        let d = 1.0 - z.re;
        Ok(C {
            re: libm::log1p(4.0 * z.re / (d * d + ay * ay)) / 4.0,
            im: -libm::atan2(-2.0 * z.im, d * (1.0 + z.re) - ay * ay) / 2.0,
        })
    }
}

/// `cosh` (`hyp = false`) or `sinh` (`hyp = true`).
#[cfg_attr(mm_shipped, optimize(size))]
fn c_cosh_sinh(z: C, sinh: bool) -> R {
    if !finite(z) {
        let r = if z.re.is_infinite() && z.im.is_finite() && z.im != 0.0 {
            let (c, s) = (libm::cos(z.im), libm::sin(z.im));
            let mut r = C {
                re: cs(f64::INFINITY, c),
                im: cs(f64::INFINITY, s),
            };
            if z.re < 0.0 {
                if sinh {
                    r.re = -r.re;
                } else {
                    r.im = -r.im;
                }
            }
            r
        } else {
            special(if sinh { Tab::Sinh } else { Tab::Cosh }, z.re, z.im)
        };
        return if z.im.is_infinite() && !z.re.is_nan() {
            Err(CErr::Dom)
        } else {
            Ok(r)
        };
    }
    let (c, s) = (libm::cos(z.im), libm::sin(z.im));
    let (ch, sh, k) = if z.re.abs() > LOG_LARGE {
        let x1 = z.re - cs(1.0, z.re);
        (libm::cosh(x1), libm::sinh(x1), E)
    } else {
        (libm::cosh(z.re), libm::sinh(z.re), 1.0)
    };
    let r = if sinh {
        C {
            re: c * sh * k,
            im: s * ch * k,
        }
    } else {
        C {
            re: c * ch * k,
            im: s * sh * k,
        }
    };
    range(r)
}

#[cfg_attr(mm_shipped, optimize(size))]
fn range(r: C) -> R {
    if r.re.is_infinite() || r.im.is_infinite() {
        Err(CErr::Range)
    } else {
        Ok(r)
    }
}

#[cfg_attr(mm_shipped, optimize(size))]
fn c_exp(z: C) -> R {
    if !finite(z) {
        let r = if z.re.is_infinite() && z.im.is_finite() && z.im != 0.0 {
            let v = if z.re > 0.0 { f64::INFINITY } else { 0.0 };
            C {
                re: cs(v, libm::cos(z.im)),
                im: cs(v, libm::sin(z.im)),
            }
        } else {
            special(Tab::Exp, z.re, z.im)
        };
        return if z.im.is_infinite() && (z.re.is_finite() || z.re > 0.0) {
            Err(CErr::Dom)
        } else {
            Ok(r)
        };
    }
    let (l, k) = if z.re > LOG_LARGE {
        (libm::exp(z.re - 1.0), E)
    } else {
        (libm::exp(z.re), 1.0)
    };
    range(C {
        re: l * libm::cos(z.im) * k,
        im: l * libm::sin(z.im) * k,
    })
}

#[cfg_attr(mm_shipped, optimize(size))]
fn c_log(z: C) -> R {
    if !finite(z) {
        return Ok(special(Tab::Log, z.re, z.im));
    }
    let (ax, ay) = (z.re.abs(), z.im.abs());
    let re = if ax > LARGE || ay > LARGE {
        libm::log(hypot(ax / 2.0, ay / 2.0)) + LN2
    } else if ax < f64::MIN_POSITIVE && ay < f64::MIN_POSITIVE {
        if ax > 0.0 || ay > 0.0 {
            let ldexp = crate::intops::ldexp;
            libm::log(hypot(ldexp(ax, 53), ldexp(ay, 53))) - 53.0 * LN2
        } else {
            return Err(CErr::Dom);
        }
    } else {
        let h = hypot(ax, ay);
        if (0.71..=1.73).contains(&h) {
            let (am, an) = if ax > ay { (ax, ay) } else { (ay, ax) };
            libm::log1p((am - 1.0) * (am + 1.0) + an * an) / 2.0
        } else {
            libm::log(h)
        }
    };
    Ok(C {
        re,
        im: libm::atan2(z.im, z.re),
    })
}

#[cfg_attr(mm_shipped, optimize(size))]
fn c_tanh(z: C) -> R {
    if !finite(z) {
        let r = if z.re.is_infinite() && z.im.is_finite() && z.im != 0.0 {
            C {
                re: cs(1.0, z.re),
                im: cs(0.0, 2.0 * libm::sin(z.im) * libm::cos(z.im)),
            }
        } else {
            special(Tab::Tanh, z.re, z.im)
        };
        return if z.im.is_infinite() && z.re.is_finite() {
            Err(CErr::Dom)
        } else {
            Ok(r)
        };
    }
    if z.re.abs() > LOG_LARGE {
        return Ok(C {
            re: cs(1.0, z.re),
            im: 4.0 * libm::sin(z.im) * libm::cos(z.im) * libm::exp(-2.0 * z.re.abs()),
        });
    }
    let tx = libm::tanh(z.re);
    let ty = libm::tan(z.im);
    let cx = 1.0 / libm::cosh(z.re);
    let txty = tx * ty;
    let denom = 1.0 + txty * txty;
    Ok(C {
        re: tx * (1.0 + ty * ty) / denom,
        im: ((ty / denom) * cx) * cx,
    })
}

/// `_Py_c_abs`.
#[cfg_attr(mm_shipped, optimize(size))]
fn c_abs(z: C) -> Result<f64, CErr> {
    if z.re.is_infinite() || z.im.is_infinite() {
        return Ok(f64::INFINITY);
    }
    if z.re.is_nan() || z.im.is_nan() {
        return Ok(f64::NAN);
    }
    let r = hypot(z.re, z.im);
    if r.is_infinite() {
        Err(CErr::Range)
    } else {
        Ok(r)
    }
}

/// The unary functions, by code.
#[inline(never)]
#[cfg_attr(mm_shipped, optimize(size))]
fn unary(f: u8, z: C) -> R {
    match f {
        0 => Ok(c_acos(z)),
        1 => Ok(c_acosh(z)),
        2 => Ok(unrot(c_asinh(rot(z)))),
        3 => Ok(c_asinh(z)),
        4 => c_atanh(rot(z)).map(unrot),
        5 => c_atanh(z),
        6 => c_cosh_sinh(rot(z), false),
        7 => c_cosh_sinh(z, false),
        8 => c_exp(z),
        9 => c_log(z).map(|r| C {
            re: r.re / LN10,
            im: r.im / LN10,
        }),
        10 => c_cosh_sinh(rot(z), true).map(unrot),
        11 => c_cosh_sinh(z, true),
        12 => Ok(c_sqrt(z)),
        13 => c_tanh(rot(z)).map(unrot),
        _ => c_tanh(z),
    }
}

impl Vm {
    /// A `cmath` argument (`PyComplex_AsCComplex`).
    #[inline(never)]
    #[cfg_attr(mm_shipped, optimize(size))]
    fn c_arg(&mut self, v: Value) -> Result<C, Raised> {
        if let Some((re, im)) = self.complex_of(v) {
            return Ok(C { re, im });
        }
        let re = self.real(v)?;
        Ok(C { re, im: 0.0 })
    }

    #[cfg_attr(mm_shipped, optimize(size))]
    fn cmath_err(&mut self, e: CErr) -> Raised {
        match e {
            CErr::Dom => self.err(T::ValueError, "math domain error"),
            CErr::Range => self.err(T::OverflowError, "math range error"),
        }
    }

    #[inline(never)]
    #[cfg_attr(mm_shipped, optimize(size))]
    fn c_result(&mut self, r: R) -> Result<Value, Raised> {
        match r {
            Ok(c) => self.new_complex(c.re, c.im),
            Err(e) => Err(self.cmath_err(e)),
        }
    }
}

macro_rules! unary {
    ($($name:ident = $k:expr),*) => {$(
        pub(crate) fn $name(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
            let z = vm.c_arg(vm.arg_o(&a, 0))?;
            vm.c_result(unary($k, z))
        }
    )*};
}

unary!(
    c_acos_ = 0,
    c_acosh_ = 1,
    c_asin_ = 2,
    c_asinh_ = 3,
    c_atan_ = 4,
    c_atanh_ = 5,
    c_cos_ = 6,
    c_cosh_ = 7,
    c_exp_ = 8,
    c_log10_ = 9,
    c_sin_ = 10,
    c_sinh_ = 11,
    c_sqrt_ = 12,
    c_tan_ = 13,
    c_tanh_ = 14
);

/// `cmath.log(z[, base])`.
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn c_log_(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let z = vm.c_arg(vm.arg_o(&a, 0))?;
    let b = vm.arg_o(&a, 1);
    let base = if b == Value::UNDEFINED {
        None
    } else {
        Some(vm.c_arg(b)?)
    };
    // CPython keeps going after a failed `c_log` (whose value is then
    // `-inf + phase*j`) and reports the last `errno` set.
    let val = |z: C| {
        c_log(z).unwrap_or(C {
            re: f64::NEG_INFINITY,
            im: libm::atan2(z.im, z.re),
        })
    };
    let r = match base {
        None => c_log(z),
        Some(b) => match (c_log(b), quot(val(z), val(b))) {
            (_, Err(e)) | (Err(e), _) => Err(e),
            (Ok(_), Ok(q)) => Ok(q),
        },
    };
    vm.c_result(r)
}

/// `cmath.phase(z)`.
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn c_phase(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let z = vm.c_arg(vm.arg_o(&a, 0))?;
    Ok(Value::float(libm::atan2(z.im, z.re)))
}

/// `cmath.polar(z)`.
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn c_polar(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let z = vm.c_arg(vm.arg_o(&a, 0))?;
    let phi = libm::atan2(z.im, z.re);
    let r = c_abs(z).map_err(|e| vm.cmath_err(e))?;
    vm.alloc_tuple(&[Value::float(r), Value::float(phi)])
}

/// `cmath.rect(r, phi)`.
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn c_rect(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let r = vm.real(vm.arg_o(&a, 0))?;
    let phi = vm.real(vm.arg_o(&a, 1))?;
    let z = if !r.is_finite() || !phi.is_finite() {
        if r != 0.0 && !r.is_nan() && phi.is_infinite() {
            return Err(vm.cmath_err(CErr::Dom));
        }
        if r.is_infinite() && phi.is_finite() && phi != 0.0 {
            let s = if r > 0.0 { 1.0 } else { -1.0 };
            C {
                re: s * cs(f64::INFINITY, libm::cos(phi)),
                im: s * cs(f64::INFINITY, libm::sin(phi)),
            }
        } else {
            special(Tab::Rect, r, phi)
        }
    } else if phi == 0.0 {
        C { re: r, im: r * phi }
    } else {
        C {
            re: r * libm::cos(phi),
            im: r * libm::sin(phi),
        }
    };
    vm.new_complex(z.re, z.im)
}

/// `cmath.isfinite`/`isinf`/`isnan`, by `k`.
#[cfg_attr(mm_shipped, optimize(size))]
fn c_is(vm: &mut Vm, a: &CallArgs, k: u8) -> Result<Value, Raised> {
    let z = vm.c_arg(vm.arg_o(a, 0))?;
    Ok(Value::from_bool(match k {
        0 => finite(z),
        1 => z.re.is_infinite() || z.im.is_infinite(),
        _ => z.re.is_nan() || z.im.is_nan(),
    }))
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn c_isfinite(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    c_is(vm, &a, 0)
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn c_isinf(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    c_is(vm, &a, 1)
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn c_isnan(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    c_is(vm, &a, 2)
}

/// `cmath.isclose(a, b, *, rel_tol=1e-09, abs_tol=0.0)`.
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn c_isclose(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let x = vm.c_arg(vm.arg_o(&a, 0))?;
    let y = vm.c_arg(vm.arg_o(&a, 1))?;
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
    if x.re == y.re && x.im == y.im {
        return Ok(Value::TRUE);
    }
    if !(x.re.is_finite() || x.re.is_nan())
        || !(x.im.is_finite() || x.im.is_nan())
        || !(y.re.is_finite() || y.re.is_nan())
        || !(y.im.is_finite() || y.im.is_nan())
    {
        return Ok(Value::FALSE);
    }
    let abs_c = |z: C| c_abs(z).unwrap_or(f64::INFINITY);
    let diff = abs_c(C {
        re: x.re - y.re,
        im: x.im - y.im,
    });
    let ok = diff <= rel * abs_c(y) || diff <= rel * abs_c(x) || diff <= abs;
    Ok(Value::from_bool(ok))
}

impl Vm {
    /// The module constants of `cmath`.
    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn cmath_constants(&mut self, m: Value) -> Result<(), Raised> {
        for (attr, x) in crate::math::CONSTANTS {
            self.module_set(m, attr, Value::float(x))?;
        }
        for (attr, im) in [("infj", f64::INFINITY), ("nanj", f64::NAN)] {
            let v = self.new_complex(0.0, im)?;
            self.module_set(m, attr, v)?;
        }
        Ok(())
    }
}
