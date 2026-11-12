//! Float kernels `core` lacks in `no_std`: exp, log, trig, hyperbolic and
//! gamma functions, evaluated in double-double and rounded once where the
//! algorithm is our own. Nothing here derives from glibc or other copyleft
//! code; notices for ported code are in `THIRD_PARTY.md`.
//!
//! Origin of each function:
//!
//! - fdlibm (Sun Microsystems, freely redistributable): `log_fd`
//!   (`e_log.c`), `rem_pio2` (`e_rem_pio2.c`, medium range), `atan_fd`
//!   (`s_atan.c`), `atan2` (`e_atan2.c`), `asin_fd`/`acos_fd`/`rpq`
//!   (`e_asin.c`, `e_acos.c`), `erf`/`erfc`/`erf_pp`/`erf_pq`/`erf_tail`
//!   (`s_erf.c`), `log10` (`e_log10.c`), `expm1` (`s_expm1.c`), `log1p`
//!   (`s_log1p.c`), `sinh`/`cosh`/`tanh` (`e_sinh.c`, `e_cosh.c`,
//!   `s_tanh.c`), `asinh`/`acosh`/`atanh` (`s_asinh.c`, `e_acosh.c`,
//!   `e_atanh.c`).
//! - FreeBSD msun (fdlibm licence; optimized by Bruce D. Evans): `cbrt`
//!   (`s_cbrt.c`).
//! - CPython `Modules/mathmodule.c` (PSF licence): `sinpi` (`m_sinpi`),
//!   `lanczos_sum`, `gamma` (`m_tgamma`), `lgamma` (`m_lgamma`),
//!   `vector_norm`.
//! - Textbook algorithms written here: `two_sum` (Knuth), `quick_two_sum`,
//!   `two_prod` (Dekker/Veltkamp splitting), the `Dd` double-double
//!   arithmetic, `exp_dd`/`exp`, `log_dd`/`log`/`log2` (Newton on `exp`),
//!   `pow`/`pow_log`, `sincos_dd`/`sincos_of`/`sin`/`cos`/`tan` (Taylor
//!   series), `reduce`/`reduce_big` (Payne-Hanek with a 2/pi constant
//!   computed for this file), the Newton corrections in `atan`/`asin`/`acos`,
//!   and the bit-level helpers `hi`, `lo`, `with_hi`, `with_lo`, `fabs`,
//!   `copysign`, `rint`, `frexp`, `fmod`, `scale`.

#![allow(
    clippy::unreadable_literal,
    clippy::excessive_precision,
    clippy::approx_constant,
    reason = "constants are kept digit for digit as in fdlibm, to compare"
)]
#![allow(
    clippy::neg_cmp_op_on_partial_ord,
    clippy::nonminimal_bool,
    clippy::verbose_bit_mask,
    clippy::manual_midpoint,
    reason = "NaN-aware comparisons and bit tests follow the C sources"
)]

#[cfg_attr(mm_shipped, optimize(size))]
fn hi(x: f64) -> u32 {
    (x.to_bits() >> 32) as u32
}

#[cfg_attr(mm_shipped, optimize(size))]
fn lo(x: f64) -> u32 {
    x.to_bits() as u32
}

#[cfg_attr(mm_shipped, optimize(size))]
fn with_hi(x: f64, h: u32) -> f64 {
    f64::from_bits((u64::from(h) << 32) | u64::from(lo(x)))
}

#[cfg_attr(mm_shipped, optimize(size))]
fn with_lo(x: f64, l: u32) -> f64 {
    f64::from_bits((x.to_bits() & 0xFFFF_FFFF_0000_0000) | u64::from(l))
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn fabs(x: f64) -> f64 {
    f64::from_bits(x.to_bits() & !(1 << 63))
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn copysign(x: f64, y: f64) -> f64 {
    f64::from_bits((x.to_bits() & !(1 << 63)) | (y.to_bits() & (1 << 63)))
}

/// Round half to even, as C's `rint`.
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn rint(x: f64) -> f64 {
    if !x.is_finite() || fabs(x) >= 4_503_599_627_370_496.0 {
        return x;
    }
    let t = crate::ops::floor(x);
    let d = x - t;
    let r = if d > 0.5 || (d == 0.5 && (t * 0.5) != crate::ops::floor(t * 0.5)) {
        t + 1.0
    } else {
        t
    };
    copysign(r, x)
}

const LN2_HI: f64 = 6.931_471_803_691_238_164_90e-01;
const LN2_LO: f64 = 1.908_214_929_270_587_700_02e-10;

/// Natural logarithm (fdlibm `e_log.c`); `-inf` at 0, NaN below.
#[cfg_attr(mm_shipped, optimize(size))]
fn log_fd(x: f64) -> f64 {
    const LG1: f64 = 6.666_666_666_666_735_130e-01;
    const LG2: f64 = 3.999_999_999_940_941_908e-01;
    const LG3: f64 = 2.857_142_874_366_239_149e-01;
    const LG4: f64 = 2.222_219_843_214_978_396e-01;
    const LG5: f64 = 1.818_357_216_161_805_012e-01;
    const LG6: f64 = 1.531_383_769_920_937_332e-01;
    const LG7: f64 = 1.479_819_860_511_658_591e-01;
    let mut x = x;
    let mut hx = hi(x) as i32;
    let lx = lo(x);
    let mut k: i32 = 0;
    if hx < 0x0010_0000 {
        if ((hx & 0x7fff_ffff) as u32 | lx) == 0 {
            return f64::NEG_INFINITY;
        }
        if hx < 0 {
            return f64::NAN;
        }
        k -= 54;
        x *= 1.801_439_850_948_198_4e16;
        hx = hi(x) as i32;
    }
    if hx >= 0x7ff0_0000 {
        return x + x;
    }
    k += (hx >> 20) - 1023;
    hx &= 0x000f_ffff;
    let i = (hx + 0x95f64) & 0x10_0000;
    x = with_hi(x, (hx | (i ^ 0x3ff0_0000)) as u32);
    k += i >> 20;
    let f = x - 1.0;
    let dk = f64::from(k);
    if (0x000f_ffff & (2 + hx)) < 3 {
        if f == 0.0 {
            return if k == 0 {
                0.0
            } else {
                dk * LN2_HI + dk * LN2_LO
            };
        }
        let r = f * f * (0.5 - 0.333_333_333_333_333_33 * f);
        return if k == 0 {
            f - r
        } else {
            dk * LN2_HI - ((r - dk * LN2_LO) - f)
        };
    }
    let s = f / (2.0 + f);
    let z = s * s;
    let mut i = hx - 0x6147a;
    let w = z * z;
    let j = 0x6b851 - hx;
    let t1 = w * (LG2 + w * (LG4 + w * LG6));
    let t2 = z * (LG1 + w * (LG3 + w * (LG5 + w * LG7)));
    i |= j;
    let r = t2 + t1;
    if i > 0 {
        let hfsq = 0.5 * f * f;
        if k == 0 {
            f - (hfsq - s * (hfsq + r))
        } else {
            dk * LN2_HI - ((hfsq - (s * (hfsq + r) + dk * LN2_LO)) - f)
        }
    } else if k == 0 {
        f - s * (f - r)
    } else {
        dk * LN2_HI - ((s * (f - r) - dk * LN2_LO) - f)
    }
}

/// `x = m * 2^e` with `0.5 <= |m| < 1` (0 for 0, non-finite unchanged).
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn frexp(x: f64) -> (f64, i32) {
    if x == 0.0 || !x.is_finite() {
        return (x, 0);
    }
    let mut x = x;
    let mut e = 0;
    if (x.to_bits() >> 52) & 0x7ff == 0 {
        x *= 1.801_439_850_948_198_4e16;
        e = -54;
    }
    let be = ((x.to_bits() >> 52) & 0x7ff) as i32;
    e += be - 1022;
    let m = f64::from_bits((x.to_bits() & !(0x7ff << 52)) | (1022 << 52));
    (m, e)
}

const PIO2_HI: f64 = 1.570_796_326_794_896_558_00e+00;
const PIO2_LO: f64 = 6.123_233_995_736_766_035_87e-17;
const PI: f64 = 3.141_592_653_589_793_116_00e+00;

/// `x` reduced modulo pi/2: `(n, y0, y1)` (fdlibm medium reduction).
#[cfg_attr(mm_shipped, optimize(size))]
fn rem_pio2(x: f64) -> (i32, f64, f64) {
    const INVPIO2: f64 = 6.366_197_723_675_813_824_33e-01;
    const PIO2_1: f64 = 1.570_796_326_734_125_614_17e+00;
    const PIO2_1T: f64 = 6.077_100_506_506_192_249_32e-11;
    const PIO2_2: f64 = 6.077_100_506_303_965_976_60e-11;
    const PIO2_2T: f64 = 2.022_266_248_795_950_631_54e-21;
    const PIO2_3: f64 = 2.022_266_248_711_166_455_80e-21;
    const PIO2_3T: f64 = 8.478_427_660_368_899_569_97e-32;
    let ix = hi(x) & 0x7fff_ffff;
    let fnn = rint(x * INVPIO2);
    let n = fnn as i64 as i32;
    let mut r = x - fnn * PIO2_1;
    let mut w = fnn * PIO2_1T;
    let j = (ix >> 20) as i32;
    let mut y0 = r - w;
    let mut i = j - ((hi(y0) >> 20) & 0x7ff) as i32;
    if i > 16 {
        let t = r;
        w = fnn * PIO2_2;
        r = t - w;
        w = fnn * PIO2_2T - ((t - r) - w);
        y0 = r - w;
        i = j - ((hi(y0) >> 20) & 0x7ff) as i32;
        if i > 49 {
            let t = r;
            w = fnn * PIO2_3;
            r = t - w;
            w = fnn * PIO2_3T - ((t - r) - w);
            y0 = r - w;
        }
    }
    let y1 = (r - y0) - w;
    (n, y0, y1)
}

/// `atan(x)` (fdlibm `s_atan.c`).
#[cfg_attr(mm_shipped, optimize(size))]
fn atan_fd(x: f64) -> f64 {
    const ATANHI: [f64; 4] = [
        4.636_476_090_008_060_935_15e-01,
        7.853_981_633_974_482_789_99e-01,
        9.827_937_232_473_290_540_82e-01,
        1.570_796_326_794_896_558_00e+00,
    ];
    const ATANLO: [f64; 4] = [
        2.269_877_745_296_168_709_24e-17,
        3.061_616_997_868_383_017_93e-17,
        1.390_331_103_123_099_845_16e-17,
        6.123_233_995_736_766_035_87e-17,
    ];
    const AT: [f64; 11] = [
        3.333_333_333_333_293_180_27e-01,
        -1.999_999_999_987_648_324_76e-01,
        1.428_571_427_250_346_637_11e-01,
        -1.111_111_040_546_235_578_80e-01,
        9.090_887_133_436_506_561_96e-02,
        -7.691_876_205_044_829_994_95e-02,
        6.661_073_137_387_531_206_69e-02,
        -5.833_570_133_790_573_486_45e-02,
        4.976_877_994_615_932_360_17e-02,
        -3.653_157_274_421_691_552_70e-02,
        1.628_582_011_536_578_236_23e-02,
    ];
    if x.is_nan() {
        return x;
    }
    let ix = hi(x) & 0x7fff_ffff;
    let neg = x.is_sign_negative();
    if ix >= 0x4410_0000 {
        let r = ATANHI[3] + ATANLO[3];
        return if neg { -r } else { r };
    }
    let mut x = x;
    let id: i32;
    if ix < 0x3fdc_0000 {
        if ix < 0x3e20_0000 {
            return x;
        }
        id = -1;
    } else {
        x = fabs(x);
        if ix < 0x3ff3_0000 {
            if ix < 0x3fe6_0000 {
                id = 0;
                x = (2.0 * x - 1.0) / (2.0 + x);
            } else {
                id = 1;
                x = (x - 1.0) / (x + 1.0);
            }
        } else if ix < 0x4003_8000 {
            id = 2;
            x = (x - 1.5) / (1.0 + 1.5 * x);
        } else {
            id = 3;
            x = -1.0 / x;
        }
    }
    let z = x * x;
    let w = z * z;
    let s1 = z * (AT[0] + w * (AT[2] + w * (AT[4] + w * (AT[6] + w * (AT[8] + w * AT[10])))));
    let s2 = w * (AT[1] + w * (AT[3] + w * (AT[5] + w * (AT[7] + w * AT[9]))));
    if id < 0 {
        return x - x * (s1 + s2);
    }
    let i = id as usize;
    let z = ATANHI[i] - ((x * (s1 + s2) - ATANLO[i]) - x);
    if neg { -z } else { z }
}

/// `atan2(y, x)` (fdlibm `e_atan2.c` special cases).
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn atan2(y: f64, x: f64) -> f64 {
    const PI_LO: f64 = 1.224_646_799_147_353_177_2e-16;
    if x.is_nan() || y.is_nan() {
        return x + y;
    }
    if x == 1.0 {
        return atan(y);
    }
    let yneg = y.is_sign_negative();
    let xneg = x.is_sign_negative();
    if y == 0.0 {
        return if xneg { copysign(PI, y) } else { y };
    }
    if x == 0.0 {
        return copysign(PIO2_HI, y);
    }
    if x.is_infinite() {
        if y.is_infinite() {
            let r = if xneg { 3.0 * PI / 4.0 } else { PI / 4.0 };
            return copysign(r, y);
        }
        return if xneg {
            copysign(PI, y)
        } else {
            copysign(0.0, y)
        };
    }
    if y.is_infinite() {
        return copysign(PIO2_HI, y);
    }
    let ex = ((x.to_bits() >> 52) & 0x7ff) as i32;
    let ey = ((y.to_bits() >> 52) & 0x7ff) as i32;
    let k = ey - ex;
    if k > 64 {
        // |y/x| > 2**64: +-pi/2 (as musl and glibc round it).
        return copysign(PIO2_HI, y);
    }
    if xneg && k < -60 {
        return copysign(PI, y);
    }
    // atan(|y/x|) in double-double (Newton from fdlibm's estimate, as
    // `atan` does), then `pi - t` for negative x, rounded once.
    let q = Dd::from(fabs(y)).div(Dd::from(fabs(x)));
    let t = if q.0 < 1e-8 {
        q
    } else if q.0 > 1e15 {
        Dd(PIO2_HI, PIO2_LO).sub(Dd::from(1.0).div(q))
    } else {
        let y0 = atan_fd(q.0);
        let (s, c) = sincos_of(y0);
        let f = s.sub(c.mul(q));
        let df = c.add(s.mul(q));
        Dd::from(y0).sub(f.div(df))
    };
    let t = if xneg { Dd(PI, PI_LO).sub(t) } else { t };
    let r = t.round();
    if yneg { -r } else { r }
}

const PS: [f64; 6] = [
    1.666_666_666_666_666_574_15e-01,
    -3.255_658_186_224_009_154_05e-01,
    2.012_125_321_348_629_258_81e-01,
    -4.005_553_450_067_941_140_27e-02,
    7.915_349_942_898_145_321_76e-04,
    3.479_331_075_960_211_675_70e-05,
];
const QS: [f64; 4] = [
    -2.403_394_911_734_414_218_78e+00,
    2.020_945_760_233_505_694_71e+00,
    -6.882_839_716_054_532_930_30e-01,
    7.703_815_055_590_193_527_91e-02,
];

#[cfg_attr(mm_shipped, optimize(size))]
fn rpq(t: f64) -> f64 {
    let p = t * (PS[0] + t * (PS[1] + t * (PS[2] + t * (PS[3] + t * (PS[4] + t * PS[5])))));
    let q = 1.0 + t * (QS[0] + t * (QS[1] + t * (QS[2] + t * QS[3])));
    p / q
}

/// `asin(x)` for `|x| <= 1` (fdlibm `e_asin.c`).
#[cfg_attr(mm_shipped, optimize(size))]
fn asin_fd(x: f64) -> f64 {
    const PIO4_HI: f64 = 7.853_981_633_974_482_789_99e-01;
    let ax = fabs(x);
    if ax >= 1.0 {
        if ax == 1.0 {
            return x * PIO2_HI + x * PIO2_LO;
        }
        return f64::NAN;
    }
    if ax < 0.5 {
        if ax < 7.450_580_596_923_828e-9 {
            return x;
        }
        return x + x * rpq(x * x);
    }
    let w = 1.0 - ax;
    let t = w * 0.5;
    let s = crate::ops::sqrt(t);
    let r = if ax >= 0.975 {
        PIO2_HI - (2.0 * (s + s * rpq(t)) - PIO2_LO)
    } else {
        let w = with_lo(s, 0);
        let c = (t - w * w) / (s + w);
        let p = 2.0 * s * rpq(t) - (PIO2_LO - 2.0 * c);
        let q = PIO4_HI - 2.0 * w;
        PIO4_HI - (p - q)
    };
    if x < 0.0 { -r } else { r }
}

/// `acos(x)` for `|x| <= 1` (fdlibm `e_acos.c`).
#[cfg_attr(mm_shipped, optimize(size))]
fn acos_fd(x: f64) -> f64 {
    let ax = fabs(x);
    if ax >= 1.0 {
        if x == 1.0 {
            return 0.0;
        }
        if x == -1.0 {
            return PI + 2.0 * PIO2_LO;
        }
        return f64::NAN;
    }
    if ax < 0.5 {
        if ax <= 6.938_893_903_907_228e-18 {
            return PIO2_HI + PIO2_LO;
        }
        return PIO2_HI - (x - (PIO2_LO - x * rpq(x * x)));
    }
    if x < 0.0 {
        let z = (1.0 + x) * 0.5;
        let s = crate::ops::sqrt(z);
        let w = rpq(z) * s - PIO2_LO;
        return PI - 2.0 * (s + w);
    }
    let z = (1.0 - x) * 0.5;
    let s = crate::ops::sqrt(z);
    let df = with_lo(s, 0);
    let c = (z - df * df) / (s + df);
    let w = rpq(z) * s + c;
    2.0 * (df + w)
}

/// CPython's `m_sinpi`: `sin(pi * x)` with exact zeros at integers.
#[cfg_attr(mm_shipped, optimize(size))]
fn sinpi(x: f64) -> f64 {
    let y = crate::ops::float_divmod(fabs(x), 2.0).1;
    let n = rint(2.0 * y) as i32;
    let r = match n {
        0 => sin(PI * y),
        1 => cos(PI * (y - 0.5)),
        2 => sin(PI * (1.0 - y)),
        3 => -cos(PI * (y - 1.5)),
        _ => sin(PI * (y - 2.0)),
    };
    copysign(1.0, x) * r
}

const LANCZOS_G: f64 = 6.024_680_040_776_729_583_740_234_375;
const LANCZOS_G_MINUS_HALF: f64 = 5.524_680_040_776_729_583_740_234_375;
const LANCZOS_NUM: [f64; 13] = [
    23_531_376_880.410_759_688_572_007_674_451_636_754_734_846_804_940,
    42_919_803_642.649_098_768_957_899_047_001_988_850_926_355_848_959,
    35_711_959_237.355_668_049_440_185_451_547_166_705_960_488_635_843,
    17_921_034_426.037_209_699_919_755_754_458_931_112_671_403_265_390,
    6_039_542_586.352_028_005_064_291_644_307_297_921_069_938_842_070_8,
    1_439_720_407.311_721_673_663_223_072_794_912_393_971_548_578_677_2,
    248_874_557.862_054_156_511_460_386_413_229_423_216_321_251_278_01,
    31_426_415.585_400_194_380_614_231_628_318_205_362_874_684_987_640,
    2_876_370.628_935_372_441_225_409_051_620_849_613_599_114_537_876_8,
    186_056.265_395_223_495_040_294_989_716_045_699_282_207_842_363_28,
    8_071.672_002_365_816_210_638_002_902_272_250_613_821_851_632_502_4,
    210.824_277_751_579_345_872_509_733_920_713_362_711_669_695_802_91,
    2.506_628_274_631_000_270_164_908_177_133_837_338_626_431_079_340_8,
];
const LANCZOS_DEN: [f64; 13] = [
    0.0,
    39_916_800.0,
    120_543_840.0,
    150_917_976.0,
    105_258_076.0,
    45_995_730.0,
    13_339_535.0,
    2_637_558.0,
    357_423.0,
    32_670.0,
    1_925.0,
    66.0,
    1.0,
];

#[cfg_attr(mm_shipped, optimize(size))]
fn lanczos_sum(x: f64) -> f64 {
    let (mut num, mut den) = (0.0, 0.0);
    if x < 5.0 {
        for i in (0..13).rev() {
            num = num * x + LANCZOS_NUM[i];
            den = den * x + LANCZOS_DEN[i];
        }
    } else {
        for i in 0..13 {
            num = num / x + LANCZOS_NUM[i];
            den = den / x + LANCZOS_DEN[i];
        }
    }
    num / den
}

/// `Gamma(x)` for valid `x` (CPython's `m_tgamma`); `None` for a domain
/// error, infinity for overflow.
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn gamma(x: f64) -> Option<f64> {
    if !x.is_finite() {
        if x.is_nan() || x > 0.0 {
            return Some(x);
        }
        return None;
    }
    if x == 0.0 {
        return None;
    }
    if x == crate::ops::floor(x) {
        if x < 0.0 {
            return None;
        }
        if x <= 23.0 {
            let mut f = 1.0;
            let mut k = 2.0;
            while k < x {
                f *= k;
                k += 1.0;
            }
            return Some(f);
        }
    }
    let absx = fabs(x);
    if absx < 1e-20 {
        return Some(1.0 / x);
    }
    if absx > 200.0 {
        if x < 0.0 {
            return Some(0.0 / sinpi(x));
        }
        return Some(f64::INFINITY);
    }
    let y = absx + LANCZOS_G_MINUS_HALF;
    let z = if absx > LANCZOS_G_MINUS_HALF {
        let q = y - absx;
        q - LANCZOS_G_MINUS_HALF
    } else {
        let q = y - LANCZOS_G_MINUS_HALF;
        q - absx
    };
    let z = z * LANCZOS_G / y;
    let r = if x < 0.0 {
        let mut r = -PI / sinpi(absx) / absx * exp(y) / lanczos_sum(absx);
        r -= z * r;
        if absx < 140.0 {
            r /= pow(y, absx - 0.5);
        } else {
            let sp = pow(y, absx / 2.0 - 0.25);
            r /= sp;
            r /= sp;
        }
        r
    } else {
        let mut r = lanczos_sum(absx) / exp(y);
        r += z * r;
        if absx < 140.0 {
            r *= pow(y, absx - 0.5);
        } else {
            let sp = pow(y, absx / 2.0 - 0.25);
            r *= sp;
            r *= sp;
        }
        r
    };
    Some(r)
}

/// `log|Gamma(x)|` (CPython's `m_lgamma`); `None` for a domain error.
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn lgamma(x: f64) -> Option<f64> {
    const LOGPI: f64 = 1.144_729_885_849_400_174_143_427_351_353_058_711_647;
    if !x.is_finite() {
        return Some(if x.is_nan() { x } else { f64::INFINITY });
    }
    if x == crate::ops::floor(x) && x <= 2.0 {
        if x <= 0.0 {
            return None;
        }
        return Some(0.0);
    }
    let absx = fabs(x);
    if absx < 1e-20 {
        return Some(-log(absx));
    }
    let mut r = log(lanczos_sum(absx)) - LANCZOS_G;
    r += (absx - 0.5) * (log(absx + LANCZOS_G - 0.5) - 1.0);
    if x < 0.0 {
        r = LOGPI - log(fabs(sinpi(absx))) - log(absx) - r;
    }
    Some(r)
}

const E_ERX: f64 = 8.45062911510467529297e-01;
const E_EFX: f64 = 1.28379167095512586316e-01;
const E_EFX8: f64 = 1.02703333676410069053e+00;
const E_PP0: f64 = 1.28379167095512558561e-01;
const E_PP1: f64 = -3.25042107247001499370e-01;
const E_PP2: f64 = -2.84817495755985104766e-02;
const E_PP3: f64 = -5.77027029648944159157e-03;
const E_PP4: f64 = -2.37630166566501626084e-05;
const E_QQ1: f64 = 3.97917223959155352819e-01;
const E_QQ2: f64 = 6.50222499887672944485e-02;
const E_QQ3: f64 = 5.08130628187576562776e-03;
const E_QQ4: f64 = 1.32494738004321644526e-04;
const E_QQ5: f64 = -3.96022827877536812320e-06;
const E_PA0: f64 = -2.36211856075265944077e-03;
const E_PA1: f64 = 4.14856118683748331666e-01;
const E_PA2: f64 = -3.72207876035701323847e-01;
const E_PA3: f64 = 3.18346619901161753674e-01;
const E_PA4: f64 = -1.10894694282396677476e-01;
const E_PA5: f64 = 3.54783043256182359371e-02;
const E_PA6: f64 = -2.16637559486879084300e-03;
const E_QA1: f64 = 1.06420880400844228286e-01;
const E_QA2: f64 = 5.40397917702171048937e-01;
const E_QA3: f64 = 7.18286544141962662868e-02;
const E_QA4: f64 = 1.26171219808761642112e-01;
const E_QA5: f64 = 1.36370839120290507362e-02;
const E_QA6: f64 = 1.19844998467991074170e-02;
const E_RA0: f64 = -9.86494403484714822705e-03;
const E_RA1: f64 = -6.93858572707181764372e-01;
const E_RA2: f64 = -1.05586262253232909814e+01;
const E_RA3: f64 = -6.23753324503260060396e+01;
const E_RA4: f64 = -1.62396669462573470355e+02;
const E_RA5: f64 = -1.84605092906711035994e+02;
const E_RA6: f64 = -8.12874355063065934246e+01;
const E_RA7: f64 = -9.81432934416914548592e+00;
const E_SA1: f64 = 1.96512716674392571292e+01;
const E_SA2: f64 = 1.37657754143519042600e+02;
const E_SA3: f64 = 4.34565877475229228821e+02;
const E_SA4: f64 = 6.45387271733267880336e+02;
const E_SA5: f64 = 4.29008140027567833386e+02;
const E_SA6: f64 = 1.08635005541779435134e+02;
const E_SA7: f64 = 6.57024977031928170135e+00;
const E_SA8: f64 = -6.04244152148580987438e-02;
const E_RB0: f64 = -9.86494292470009928597e-03;
const E_RB1: f64 = -7.99283237680523006574e-01;
const E_RB2: f64 = -1.77579549177547519889e+01;
const E_RB3: f64 = -1.60636384855821916062e+02;
const E_RB4: f64 = -6.37566443368389627722e+02;
const E_RB5: f64 = -1.02509513161107724954e+03;
const E_RB6: f64 = -4.83519191608651397019e+02;
const E_SB1: f64 = 3.03380607434824582924e+01;
const E_SB2: f64 = 3.25792512996573918826e+02;
const E_SB3: f64 = 1.53672958608443695994e+03;
const E_SB4: f64 = 3.19985821950859553908e+03;
const E_SB5: f64 = 2.55305040643316442583e+03;
const E_SB6: f64 = 4.74528541206955367215e+02;
const E_SB7: f64 = -2.24409524465858183362e+01;

#[cfg_attr(mm_shipped, optimize(size))]
fn erf_pp(z: f64) -> f64 {
    let r = E_PP0 + z * (E_PP1 + z * (E_PP2 + z * (E_PP3 + z * E_PP4)));
    let s = 1.0 + z * (E_QQ1 + z * (E_QQ2 + z * (E_QQ3 + z * (E_QQ4 + z * E_QQ5))));
    r / s
}

#[cfg_attr(mm_shipped, optimize(size))]
fn erf_pq(s: f64) -> (f64, f64) {
    let p = E_PA0 + s * (E_PA1 + s * (E_PA2 + s * (E_PA3 + s * (E_PA4 + s * (E_PA5 + s * E_PA6)))));
    let q = 1.0 + s * (E_QA1 + s * (E_QA2 + s * (E_QA3 + s * (E_QA4 + s * (E_QA5 + s * E_QA6)))));
    (p, q)
}

/// The `exp(-x^2) * R/S` tail shared by erf and erfc for `|x| >= 1.25`.
#[cfg_attr(mm_shipped, optimize(size))]
fn erf_tail(ax: f64, ix: u32, cut: u32) -> f64 {
    let s = 1.0 / (ax * ax);
    let (r, ss) = if ix < cut {
        (
            E_RA0
                + s * (E_RA1
                    + s * (E_RA2
                        + s * (E_RA3 + s * (E_RA4 + s * (E_RA5 + s * (E_RA6 + s * E_RA7)))))),
            1.0 + s
                * (E_SA1
                    + s * (E_SA2
                        + s * (E_SA3
                            + s * (E_SA4 + s * (E_SA5 + s * (E_SA6 + s * (E_SA7 + s * E_SA8))))))),
        )
    } else {
        (
            E_RB0 + s * (E_RB1 + s * (E_RB2 + s * (E_RB3 + s * (E_RB4 + s * (E_RB5 + s * E_RB6))))),
            1.0 + s
                * (E_SB1
                    + s * (E_SB2
                        + s * (E_SB3 + s * (E_SB4 + s * (E_SB5 + s * (E_SB6 + s * E_SB7)))))),
        )
    };
    let z = with_lo(ax, 0);
    exp(-z * z - 0.5625) * exp((z - ax) * (z + ax) + r / ss)
}

/// `erf` (fdlibm `s_erf.c`).
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn erf(x: f64) -> f64 {
    let hx = hi(x);
    let ix = hx & 0x7fff_ffff;
    let neg = hx >> 31 == 1;
    if ix >= 0x7ff0_0000 {
        if x.is_nan() {
            return x;
        }
        return if neg { -1.0 } else { 1.0 };
    }
    if ix < 0x3feb_0000 {
        if ix < 0x3e30_0000 {
            if ix < 0x0080_0000 {
                return 0.125 * (8.0 * x + E_EFX8 * x);
            }
            return x + E_EFX * x;
        }
        return x + x * erf_pp(x * x);
    }
    if ix < 0x3ff4_0000 {
        let (p, q) = erf_pq(fabs(x) - 1.0);
        return if neg { -E_ERX - p / q } else { E_ERX + p / q };
    }
    if ix >= 0x4018_0000 {
        return if neg { -1.0 } else { 1.0 };
    }
    let ax = fabs(x);
    let r = erf_tail(ax, ix, 0x4006_DB6E);
    if neg { r / ax - 1.0 } else { 1.0 - r / ax }
}

/// `erfc` (fdlibm `s_erf.c`).
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn erfc(x: f64) -> f64 {
    let hx = hi(x);
    let ix = hx & 0x7fff_ffff;
    let neg = hx >> 31 == 1;
    if ix >= 0x7ff0_0000 {
        if x.is_nan() {
            return x;
        }
        return if neg { 2.0 } else { 0.0 };
    }
    if ix < 0x3feb_0000 {
        if ix < 0x3c70_0000 {
            return 1.0 - x;
        }
        let y = erf_pp(x * x);
        if (hx as i32) < 0x3fd0_0000 {
            return 1.0 - (x + x * y);
        }
        let mut r = x * y;
        r += x - 0.5;
        return 0.5 - r;
    }
    if ix < 0x3ff4_0000 {
        let (p, q) = erf_pq(fabs(x) - 1.0);
        if neg {
            return 1.0 + (E_ERX + p / q);
        }
        return (1.0 - E_ERX) - p / q;
    }
    if ix < 0x403c_0000 {
        if neg && ix >= 0x4018_0000 && ix >= 0x4006_DB6D {
            return 2.0;
        }
        let ax = fabs(x);
        let r = erf_tail(ax, ix, 0x4006_DB6D);
        return if neg { 2.0 - r / ax } else { r / ax };
    }
    if neg { 2.0 } else { 0.0 }
}

/// Exact product `a * b = hi + lo` (Dekker).
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn two_prod(a: f64, b: f64) -> (f64, f64) {
    const SPLIT: f64 = 134_217_729.0;
    const BIG: f64 = 6.696_928_794_914_171e299; // 2^996: splitting would overflow
    if !a.is_finite() || !b.is_finite() {
        return (a * b, 0.0);
    }
    if fabs(a) > BIG || fabs(b) > BIG {
        let (sa, sb) = if fabs(a) > BIG {
            (a * 1.110_223_024_625_156_5e-16, b)
        } else {
            (a, b * 1.110_223_024_625_156_5e-16)
        };
        let (h, l) = two_prod(sa, sb);
        return (h * 9_007_199_254_740_992.0, l * 9_007_199_254_740_992.0);
    }
    let p = a * b;
    if !p.is_finite() {
        return (p, 0.0);
    }
    let split = |v: f64| {
        let c = SPLIT * v;
        let h = c - (c - v);
        (h, v - h)
    };
    let (ah, al) = split(a);
    let (bh, bl) = split(b);
    let err = ((ah * bh - p) + ah * bl + al * bh) + al * bl;
    (p, err)
}

/// CPython's `vector_norm`: the Euclidean norm of finite values whose
/// largest magnitude is `max`.
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn vector_norm(vec: &mut [f64], max: f64, found_nan: bool) -> f64 {
    if max.is_infinite() {
        return max;
    }
    if found_nan {
        return f64::NAN;
    }
    if max == 0.0 || vec.len() <= 1 {
        return max;
    }
    let (_, max_e) = frexp(max);
    if max_e < -1023 {
        for v in vec.iter_mut() {
            *v /= f64::MIN_POSITIVE;
        }
        return f64::MIN_POSITIVE * vector_norm(vec, max / f64::MIN_POSITIVE, found_nan);
    }
    let scale = crate::intops::ldexp(1.0, -i64::from(max_e));
    let mut csum = 1.0;
    let mut frac1 = 0.0;
    let mut frac2 = 0.0;
    for &v in vec.iter() {
        let x = v * scale;
        let (ph, pl) = two_prod(x, x);
        let sh = csum + ph;
        let sl = (csum - sh) + ph;
        csum = sh;
        frac1 += pl;
        frac2 += sl;
    }
    let mut h = crate::ops::sqrt(csum - 1.0 + (frac1 + frac2));
    let (ph, pl) = two_prod(-h, h);
    let sh = csum + ph;
    let sl = (csum - sh) + ph;
    csum = sh;
    frac1 += pl;
    frac2 += sl;
    let x = csum - 1.0 + (frac1 + frac2);
    h += x / (2.0 * h);
    h / scale
}

/// `fmod(x, y)` for finite `x` and non-zero `y`: exact.
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn fmod(x: f64, y: f64) -> f64 {
    if y.is_infinite() {
        return x;
    }
    let ax = fabs(x);
    let ay = fabs(y);
    if ax < ay {
        return x;
    }
    // Long division on the exact binary values.
    let mut r = ax;
    while r >= ay {
        let (_, er) = frexp(r);
        let (_, ey) = frexp(ay);
        let mut d = crate::intops::ldexp(ay, i64::from(er - ey));
        if d > r {
            d /= 2.0;
        }
        r -= d;
    }
    copysign(r, x)
}

// ------------------------------------------------------- double-double

/// An unevaluated sum `hi + lo` with `|lo| <= ulp(hi)/2`: about 106 bits.
/// Results computed in this precision and rounded once are correctly
/// rounded except in rare near-halfway cases, which is what makes them
/// agree with a good libm.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Dd(pub f64, pub f64);

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn two_sum(a: f64, b: f64) -> Dd {
    let s = a + b;
    let bb = s - a;
    Dd(s, (a - (s - bb)) + (b - bb))
}

#[cfg_attr(mm_shipped, optimize(size))]
fn quick_two_sum(a: f64, b: f64) -> Dd {
    let s = a + b;
    Dd(s, b - (s - a))
}

impl Dd {
    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn from(x: f64) -> Dd {
        Dd(x, 0.0)
    }

    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn add(self, o: Dd) -> Dd {
        let s = two_sum(self.0, o.0);
        let t = two_sum(self.1, o.1);
        let s = quick_two_sum(s.0, s.1 + t.0);
        quick_two_sum(s.0, s.1 + t.1)
    }

    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn neg(self) -> Dd {
        Dd(-self.0, -self.1)
    }

    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn sub(self, o: Dd) -> Dd {
        self.add(o.neg())
    }

    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn mul(self, o: Dd) -> Dd {
        let (p, e) = two_prod(self.0, o.0);
        let e = e + (self.0 * o.1 + self.1 * o.0);
        quick_two_sum(p, e)
    }

    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn mul_f(self, b: f64) -> Dd {
        let (p, e) = two_prod(self.0, b);
        quick_two_sum(p, e + self.1 * b)
    }

    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn div(self, o: Dd) -> Dd {
        let q1 = self.0 / o.0;
        let r = self.sub(o.mul_f(q1));
        let q2 = r.0 / o.0;
        let r = r.sub(o.mul_f(q2));
        let q3 = r.0 / o.0;
        quick_two_sum(q1, q2).add(Dd::from(q3))
    }

    /// The nearest double.
    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn round(self) -> f64 {
        self.0 + self.1
    }
}

const LN2_DD: Dd = Dd(6.931_471_805_599_453e-1, 2.319_046_813_846_299_6e-17);

/// `e^x` in double-double for `|x| <= 709` (no overflow checks): reduce by
/// `ln 2`, scale down by 2^8, Taylor series, square back up.
#[cfg_attr(mm_shipped, optimize(size))]
fn exp_dd(x: Dd) -> (Dd, i64) {
    let k = rint(x.0 / LN2_DD.0);
    let r = x.sub(LN2_DD.mul_f(k));
    let r = Dd(r.0 / 256.0, r.1 / 256.0);
    // Taylor series of e^r - 1 to r^11 (|r| < 0.0014).
    let mut term = r;
    let mut sum = r;
    let mut n = 2.0;
    while n < 12.0 {
        term = term.mul(r);
        term = term.div(Dd::from(n));
        sum = sum.add(term);
        n += 1.0;
    }
    // (1 + s)^2 - 1 = 2s + s^2, eight times, keeps the small part exact.
    for _ in 0..8 {
        sum = sum.mul_f(2.0).add(sum.mul(sum));
    }
    (sum.add(Dd::from(1.0)), k as i64)
}

/// `d * 2^k`, rounded once.
#[cfg_attr(mm_shipped, optimize(size))]
fn scale(d: Dd, k: i64) -> f64 {
    crate::intops::ldexp(d.round(), k)
}

/// `e^x`.
// Out of line: LLVM inlines it into each of its callers otherwise.
#[inline(never)]
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn exp(x: f64) -> f64 {
    if x.is_nan() {
        return x;
    }
    if x > 709.782_712_893_384 {
        return f64::INFINITY;
    }
    if x < -745.133_219_101_941_2 {
        return 0.0;
    }
    if fabs(x) < 1e-300 {
        return 1.0 + x;
    }
    let (d, k) = exp_dd(Dd::from(x));
    scale(d, k)
}

/// `log(x)` in double-double for a positive finite double-double `x`
/// (one Newton step on `exp` from the fdlibm value).
#[cfg_attr(mm_shipped, optimize(size))]
fn log_dd(x: Dd) -> Dd {
    let y0 = log_fd(x.0);
    let (e, k) = exp_dd(Dd::from(y0));
    let e = Dd(crate::intops::ldexp(e.0, k), crate::intops::ldexp(e.1, k));
    Dd::from(y0).add(x.sub(e).div(e))
}

/// Natural logarithm; `-inf` at 0, NaN below.
// Out of line: LLVM inlines it into each of its callers otherwise.
#[inline(never)]
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn log(x: f64) -> f64 {
    if !(x > 0.0) || x.is_infinite() {
        return log_fd(x);
    }
    if x == 1.0 {
        return 0.0;
    }
    if (x.to_bits() >> 52) & 0x7ff == 0 {
        // Subnormal: log(x * 2^54) - 54 ln 2.
        let s = log_dd(Dd::from(x * 1.801_439_850_948_198_4e16));
        return s.sub(LN2_DD.mul_f(54.0)).round();
    }
    log_dd(Dd::from(x)).round()
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn log2(x: f64) -> f64 {
    if !(x > 0.0) || x.is_infinite() {
        return log_fd(x);
    }
    let (m, e) = frexp(x);
    if m == 0.5 {
        return f64::from(e - 1);
    }
    let l = log_dd(Dd::from(2.0 * m)).div(LN2_DD);
    l.add(Dd::from(f64::from(e - 1))).round()
}

/// `x ** y` for floats, as C's `pow` (no domain checks; callers handle
/// zeros, infinities and NaNs).
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn pow(x: f64, y: f64) -> f64 {
    if y == 0.0 || x == 1.0 {
        return 1.0;
    }
    if x.is_nan() || y.is_nan() {
        return f64::NAN;
    }
    if !x.is_finite() || !y.is_finite() || x == 0.0 {
        return crate::ops::powf_special(x, y);
    }
    let yint = rint(y) == y;
    if x < 0.0 && !yint {
        return f64::NAN;
    }
    let odd =
        yint && fabs(y) < 9.007_199_254_740_992e15 && crate::ops::float_divmod(y, 2.0).1 != 0.0;
    let ax = fabs(x);
    let r = if yint && fabs(y) < 4_294_967_296.0 {
        let n = fabs(y) as u64;
        match powi_dd(Dd::from(ax), n) {
            None => pow_log(ax, y),
            Some(acc) if y > 0.0 => acc.round(),
            // Below 2^-969 the low word of `ax ** n` is subnormal and has
            // lost bits, and `1 / acc` may overflow (a NaN in `div`): take
            // `(1 / ax) ** n` instead, whose low word stays normal.
            Some(acc) if acc.0 < 2.004_168_360_008_973e-292 => {
                match powi_dd(Dd::from(1.0).div(Dd::from(ax)), n) {
                    Some(r) => r.round(),
                    None => pow_log(ax, y),
                }
            }
            Some(acc) => Dd::from(1.0).div(acc).round(),
        }
    } else {
        pow_log(ax, y)
    };
    if x < 0.0 && odd { -r } else { r }
}

/// `base ** n` by square and multiply in double-double; `None` once a
/// product is zero or not finite.
#[cfg_attr(mm_shipped, optimize(size))]
fn powi_dd(mut base: Dd, mut n: u64) -> Option<Dd> {
    let mut acc = Dd::from(1.0);
    while n > 0 {
        if n & 1 == 1 {
            acc = acc.mul(base);
        }
        n >>= 1;
        if n > 0 {
            base = base.mul(base);
        }
        if !acc.0.is_finite() || !base.0.is_finite() || acc.0 == 0.0 || base.0 == 0.0 {
            return None;
        }
    }
    Some(acc)
}

#[cfg_attr(mm_shipped, optimize(size))]
fn pow_log(ax: f64, y: f64) -> f64 {
    let l = if (ax.to_bits() >> 52) & 0x7ff == 0 {
        log_dd(Dd::from(ax * 1.801_439_850_948_198_4e16)).sub(LN2_DD.mul_f(54.0))
    } else {
        log_dd(Dd::from(ax))
    };
    let t = l.mul_f(y);
    if t.0 > 709.782_712_893_384 {
        return f64::INFINITY;
    }
    if t.0 < -745.2 {
        return 0.0;
    }
    let (e, k) = exp_dd(t);
    scale(e, k)
}

/// `sin` and `cos` of a reduced argument `|y| <= pi/4`, in double-double.
#[cfg_attr(mm_shipped, optimize(size))]
fn sincos_dd(y: Dd) -> (Dd, Dd) {
    let y2 = y.mul(y);
    // sin: y - y^3/3! + ...; cos: 1 - y^2/2! + ...
    let mut s_term = y;
    let mut s = y;
    let mut c_term = Dd::from(1.0);
    let mut c = Dd::from(1.0);
    let mut n = 1.0;
    for _ in 0..14 {
        c_term = c_term.mul(y2).div(Dd::from(-(n * (n + 1.0))));
        c = c.add(c_term);
        s_term = s_term.mul(y2).div(Dd::from(-((n + 1.0) * (n + 2.0))));
        s = s.add(s_term);
        n += 2.0;
    }
    (s, c)
}

/// `x` reduced modulo pi/2 in double-double.
#[cfg_attr(mm_shipped, optimize(size))]
fn reduce(x: f64) -> (i32, Dd) {
    if fabs(x) <= 0.785_398_163_397_448_3 {
        return (0, Dd::from(x));
    }
    if fabs(x) >= 1_647_099.0 {
        let (n, r) = reduce_big(fabs(x));
        return if x < 0.0 { (-n, r.neg()) } else { (n, r) };
    }
    let (n, y0, y1) = rem_pio2(x);
    (n, quick_two_sum(y0, y1))
}

/// `floor(2/pi * 2**1280)` in hex: enough bits that reducing any double
/// keeps more than 120 bits of the remainder (Payne-Hanek).
const TWO_OVER_PI: [&str; 5] = [
    "a2f9836e4e441529fc2757d1f534ddc0db6295993c439041fe5163abdebbc561",
    "b7246e3a424dd2e006492eea09d1921cfe1deb1cb129a73ee88235f52ebb4484",
    "e99c7026b45f7e413991d639835339f49c845f8bbdf9283b1ff897ffde05980f",
    "ef2f118b5a0a6d1f6d367ecf27cb09b74f463f669e5fea2d7527bac7ebe5f17b",
    "3d0739f78a5292ea6bfb5fb11f8d5d0856033046fc7b6babf0cfbc209af4361d",
];

/// `x >= 2**20 * pi/2` reduced modulo pi/2: the quadrant and the remainder
/// in `[-pi/4, pi/4]`, by exact integer multiplication with 2/pi.
#[cfg_attr(mm_shipped, optimize(size))]
fn reduce_big(x: f64) -> (i32, Dd) {
    use crate::bigint::Big;
    const K: u64 = 1280;
    const PIO2: Dd = Dd(1.570_796_326_794_896_6, 6.123_233_995_736_766e-17);
    let digits: alloc::string::String = TWO_OVER_PI.concat();
    let Some(c) = Big::parse(digits.as_bytes(), 16) else {
        return (0, Dd::from(f64::NAN));
    };
    let (m, e) = frexp(x);
    let e = i64::from(e) - 53;
    let p = Big::from_f64(m * 9_007_199_254_740_992.0).mul(&c);
    // x * 2/pi = p * 2**(e - K); `shift` bits are fractional.
    let shift = (K as i64 - e) as u64;
    let mut q = p.shr(shift);
    let mut f = p.sub(&q.shl(shift));
    let one = Big::from_i64(1);
    if f.bit(shift - 1) {
        q = q.add(&one);
        f = f.sub(&one.shl(shift));
    }
    let n = i32::from(q.bit(0)) | (i32::from(q.bit(1)) << 1);
    let mag = f.abs();
    let bits = mag.bit_length();
    if bits == 0 {
        return (n, Dd::from(0.0));
    }
    // The top 106 bits of the fraction as a double-double.
    let lo_shift = bits.saturating_sub(53);
    let hi = mag.shr(lo_shift);
    let rest = mag.sub(&hi.shl(lo_shift));
    let rest_shift = lo_shift.saturating_sub(53);
    let hi_f = hi.to_f64().unwrap_or(0.0);
    let lo_f = rest.shr(rest_shift).to_f64().unwrap_or(0.0);
    let scale = |v: f64, s: u64| crate::intops::ldexp(v, s as i64 - shift as i64);
    let frac = quick_two_sum(scale(hi_f, lo_shift), scale(lo_f, rest_shift));
    let r = frac.mul(PIO2);
    (n, if f.neg { r.neg() } else { r })
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn sin(x: f64) -> f64 {
    if !x.is_finite() {
        return f64::NAN;
    }
    if fabs(x) < 1e-8 {
        return x;
    }
    let (n, y) = reduce(x);
    let (s, c) = sincos_dd(y);
    match n & 3 {
        0 => s.round(),
        1 => c.round(),
        2 => -s.round(),
        _ => -c.round(),
    }
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn cos(x: f64) -> f64 {
    if !x.is_finite() {
        return f64::NAN;
    }
    let (n, y) = reduce(x);
    let (s, c) = sincos_dd(y);
    match n & 3 {
        0 => c.round(),
        1 => -s.round(),
        2 => -c.round(),
        _ => s.round(),
    }
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn tan(x: f64) -> f64 {
    if !x.is_finite() {
        return f64::NAN;
    }
    if fabs(x) < 1e-8 {
        return x;
    }
    let (n, y) = reduce(x);
    let (s, c) = sincos_dd(y);
    if n & 1 == 0 {
        s.div(c).round()
    } else {
        c.div(s).neg().round()
    }
}

/// `sin` and `cos` of a double in double-double.
#[cfg_attr(mm_shipped, optimize(size))]
fn sincos_of(y: f64) -> (Dd, Dd) {
    let (n, r) = reduce(y);
    let (s, c) = sincos_dd(r);
    match n & 3 {
        0 => (s, c),
        1 => (c, s.neg()),
        2 => (s.neg(), c.neg()),
        _ => (c.neg(), s),
    }
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn atan(x: f64) -> f64 {
    let y0 = atan_fd(x);
    if !x.is_finite() || fabs(x) < 1e-8 || fabs(x) > 1e15 {
        return y0;
    }
    // Newton on f(y) = sin y - x cos y.
    let (s, c) = sincos_of(y0);
    let f = s.sub(c.mul_f(x));
    let df = c.add(s.mul_f(x));
    Dd::from(y0).sub(f.div(df)).round()
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn asin(x: f64) -> f64 {
    let y0 = asin_fd(x);
    if !(fabs(x) < 0.999) || fabs(x) < 1e-8 {
        return y0;
    }
    let (s, c) = sincos_of(y0);
    Dd::from(y0).sub(s.sub(Dd::from(x)).div(c)).round()
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn acos(x: f64) -> f64 {
    let y0 = acos_fd(x);
    if !(fabs(x) < 0.999) {
        return y0;
    }
    let (s, c) = sincos_of(y0);
    Dd::from(y0).add(c.sub(Dd::from(x)).div(s)).round()
}

/// Cube root (FreeBSD msun `s_cbrt.c`, fdlibm as optimized by Bruce D.
/// Evans; the same algorithm as musl's `cbrt.c`): a 5-bit estimate from the
/// exponent, a polynomial to 23 bits, then one Newton step (< 0.667 ulp).
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn cbrt(x: f64) -> f64 {
    const B1: u32 = 715_094_163; // (1023 - 1023/3 - 0.03306235651) * 2**20
    const B2: u32 = 696_219_795; // (1023 - 1023/3 - 54/3 - 0.03306235651) * 2**20
    const P0: f64 = 1.87595182427177009643;
    const P1: f64 = -1.88497979543377169875;
    const P2: f64 = 1.621429720105354466140;
    const P3: f64 = -0.758397934778766047437;
    const P4: f64 = 0.145996192886612446982;
    let mut hx = hi(x);
    let sign = hx & 0x8000_0000;
    hx ^= sign;
    if hx >= 0x7ff0_0000 {
        return x + x;
    }
    let mut t = if hx < 0x0010_0000 {
        if hx | lo(x) == 0 {
            return x;
        }
        let t = x * 18_014_398_509_481_984.0; // 2**54
        f64::from_bits(u64::from(sign | ((hi(t) & 0x7fff_ffff) / 3 + B2)) << 32)
    } else {
        f64::from_bits(u64::from(sign | (hx / 3 + B1)) << 32)
    };
    let r = (t * t) * (t / x);
    t *= (P0 + r * (P1 + r * P2)) + ((r * r) * r) * (P3 + r * P4);
    t = f64::from_bits((t.to_bits() + 0x8000_0000) & 0xffff_ffff_c000_0000);
    let s = t * t;
    let r = x / s;
    let w = t + t;
    let r = (r - t) / (w + r);
    t + t * r
}

// ----------------------------------------------- fdlibm functions

/// `log10` (fdlibm `e_log10.c` on top of the correctly rounded `log`).
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn log10(x: f64) -> f64 {
    const IVLN10: f64 = 4.342_944_819_032_518_166_68e-01;
    const LOG10_2HI: f64 = 3.010_299_956_636_117_713_06e-01;
    const LOG10_2LO: f64 = 3.694_239_077_158_930_786_16e-13;
    if !(x > 0.0) || x.is_infinite() {
        return log_fd(x);
    }
    let mut x = x;
    let mut k: i32 = 0;
    let mut hx = hi(x) as i32;
    if hx < 0x0010_0000 {
        k -= 54;
        x *= 1.801_439_850_948_198_4e16;
        hx = hi(x) as i32;
    }
    k += (hx >> 20) - 1023;
    let i = ((k as u32) & 0x8000_0000) >> 31;
    let hx = ((hx as u32) & 0x000f_ffff) | ((0x3ff - i) << 20);
    let y = f64::from(k + i as i32);
    x = with_hi(x, hx);
    let z = y * LOG10_2LO + IVLN10 * log(x);
    z + y * LOG10_2HI
}

/// `e^x - 1` (fdlibm `s_expm1.c`).
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn expm1(x: f64) -> f64 {
    const Q1: f64 = -3.333_333_333_333_313_164_28e-02;
    const Q2: f64 = 1.587_301_587_254_814_601_65e-03;
    const Q3: f64 = -7.936_507_578_674_879_424_73e-05;
    const Q4: f64 = 4.008_217_827_329_362_395_52e-06;
    const Q5: f64 = -2.010_992_181_836_243_713_26e-07;
    const INVLN2: f64 = 1.442_695_040_888_963_387_00e+00;
    let mut hx = hi(x);
    let neg = hx & 0x8000_0000 != 0;
    hx &= 0x7fff_ffff;
    if hx >= 0x4043_687A {
        if hx >= 0x4086_2E42 {
            if x.is_nan() {
                return x;
            }
            if x.is_infinite() {
                return if neg { -1.0 } else { x };
            }
            if x > 7.097_827_128_933_839_730_96e+02 {
                return f64::INFINITY;
            }
        }
        if neg {
            return -1.0;
        }
    }
    let (xr, c, k): (f64, f64, i32);
    if hx > 0x3fd6_2e42 {
        let (hi_, lo_);
        if hx < 0x3FF0_A2B2 {
            if neg {
                hi_ = x + LN2_HI;
                lo_ = -LN2_LO;
                k = -1;
            } else {
                hi_ = x - LN2_HI;
                lo_ = LN2_LO;
                k = 1;
            }
        } else {
            k = (INVLN2 * x + if neg { -0.5 } else { 0.5 }) as i32;
            let t = f64::from(k);
            hi_ = x - t * LN2_HI;
            lo_ = t * LN2_LO;
        }
        xr = hi_ - lo_;
        c = (hi_ - xr) - lo_;
    } else if hx < 0x3c90_0000 {
        return x;
    } else {
        xr = x;
        c = 0.0;
        k = 0;
    }
    let hfx = 0.5 * xr;
    let hxs = xr * hfx;
    let r1 = 1.0 + hxs * (Q1 + hxs * (Q2 + hxs * (Q3 + hxs * (Q4 + hxs * Q5))));
    let t = 3.0 - r1 * hfx;
    let mut e = hxs * ((r1 - t) / (6.0 - xr * t));
    if k == 0 {
        return xr - (xr * e - hxs);
    }
    e = xr * (e - c) - c;
    e -= hxs;
    if k == -1 {
        return 0.5 * (xr - e) - 0.5;
    }
    if k == 1 {
        if xr < -0.25 {
            return -2.0 * (e - (xr + 0.5));
        }
        return 1.0 + 2.0 * (xr - e);
    }
    let add_exp = |y: f64| with_hi(y, hi(y).wrapping_add((k << 20) as u32));
    if k <= -2 || k > 56 {
        let y = 1.0 - (e - xr);
        let y = if k == 1024 {
            add_exp(y * 0.5) * 2.0
        } else {
            add_exp(y)
        };
        return y - 1.0;
    }
    if k < 20 {
        let t = f64::from_bits(u64::from(0x3ff0_0000u32 - (0x20_0000u32 >> k)) << 32);
        let y = t - (e - xr);
        add_exp(y)
    } else {
        let t = f64::from_bits(u64::from((0x3ffu32 - k as u32) << 20) << 32);
        let mut y = xr - (e + t);
        y += 1.0;
        add_exp(y)
    }
}

/// `log(1 + x)` (fdlibm `s_log1p.c`).
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn log1p(x: f64) -> f64 {
    const LP: [f64; 7] = [
        6.666_666_666_666_735_130e-01,
        3.999_999_999_940_941_908e-01,
        2.857_142_874_366_239_149e-01,
        2.222_219_843_214_978_396e-01,
        1.818_357_216_161_805_012e-01,
        1.531_383_769_920_937_332e-01,
        1.479_819_860_511_658_591e-01,
    ];
    let hx = hi(x) as i32;
    let ax = hx & 0x7fff_ffff;
    let mut k = 1;
    let mut f = 0.0;
    let mut hu: i32 = 0;
    let mut c = 0.0;
    if hx < 0x3FDA_827A {
        if ax >= 0x3ff0_0000 {
            if x == -1.0 {
                return f64::NEG_INFINITY;
            }
            return f64::NAN;
        }
        if ax < 0x3e20_0000 {
            if ax < 0x3c90_0000 {
                return x;
            }
            return x - x * x * 0.5;
        }
        if hx > 0 || hx <= 0xbfd2_bec3_u32 as i32 {
            k = 0;
            f = x;
            hu = 1;
        }
    }
    if hx >= 0x7ff0_0000 {
        return x + x;
    }
    if k != 0 {
        let mut u;
        if hx < 0x4340_0000 {
            u = 1.0 + x;
            hu = hi(u) as i32;
            k = (hu >> 20) - 1023;
            c = if k > 0 { 1.0 - (u - x) } else { x - (u - 1.0) };
            c /= u;
        } else {
            u = x;
            hu = hi(u) as i32;
            k = (hu >> 20) - 1023;
            c = 0.0;
        }
        hu &= 0x000f_ffff;
        if hu < 0x6a09e {
            u = with_hi(u, (hu | 0x3ff0_0000) as u32);
        } else {
            k += 1;
            u = with_hi(u, (hu | 0x3fe0_0000) as u32);
            hu = (0x0010_0000 - hu) >> 2;
        }
        f = u - 1.0;
    }
    let hfsq = 0.5 * f * f;
    let dk = f64::from(k);
    if hu == 0 {
        if f == 0.0 {
            if k == 0 {
                return 0.0;
            }
            c += dk * LN2_LO;
            return dk * LN2_HI + c;
        }
        let r = hfsq * (1.0 - 0.666_666_666_666_666_66 * f);
        if k == 0 {
            return f - r;
        }
        return dk * LN2_HI - ((r - (dk * LN2_LO + c)) - f);
    }
    let s = f / (2.0 + f);
    let z = s * s;
    let r = z
        * (LP[0] + z * (LP[1] + z * (LP[2] + z * (LP[3] + z * (LP[4] + z * (LP[5] + z * LP[6]))))));
    if k == 0 {
        return f - (hfsq - s * (hfsq + r));
    }
    dk * LN2_HI - ((hfsq - (s * (hfsq + r) + (dk * LN2_LO + c))) - f)
}

/// `sinh` (fdlibm `e_sinh.c`).
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn sinh(x: f64) -> f64 {
    let jx = hi(x) as i32;
    let ix = jx & 0x7fff_ffff;
    if ix >= 0x7ff0_0000 {
        return x + x;
    }
    let h = if jx < 0 { -0.5 } else { 0.5 };
    if ix < 0x4036_0000 {
        if ix < 0x3e30_0000 {
            return x;
        }
        let t = expm1(fabs(x));
        if ix < 0x3ff0_0000 {
            return h * (2.0 * t - t * t / (t + 1.0));
        }
        return h * (t + t / (t + 1.0));
    }
    if ix < 0x4086_2E42 {
        return h * exp(fabs(x));
    }
    if fabs(x) <= 7.104_758_600_739_439e2 {
        let w = exp(0.5 * fabs(x));
        let t = h * w;
        return t * w;
    }
    x * f64::INFINITY
}

/// `cosh` (fdlibm `e_cosh.c`).
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn cosh(x: f64) -> f64 {
    let ix = (hi(x) & 0x7fff_ffff) as i32;
    if ix >= 0x7ff0_0000 {
        return x * x;
    }
    if ix < 0x3fd6_2e43 {
        let t = expm1(fabs(x));
        let w = 1.0 + t;
        if ix < 0x3c80_0000 {
            return w;
        }
        return 1.0 + (t * t) / (w + w);
    }
    if ix < 0x4036_0000 {
        let t = exp(fabs(x));
        return 0.5 * t + 0.5 / t;
    }
    if ix < 0x4086_2E42 {
        return 0.5 * exp(fabs(x));
    }
    if fabs(x) <= 7.104_758_600_739_439e2 {
        let w = exp(0.5 * fabs(x));
        let t = 0.5 * w;
        return t * w;
    }
    f64::INFINITY
}

/// `tanh` (fdlibm `s_tanh.c`).
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn tanh(x: f64) -> f64 {
    let jx = hi(x) as i32;
    let ix = jx & 0x7fff_ffff;
    if ix >= 0x7ff0_0000 {
        return if jx >= 0 {
            1.0 / x + 1.0
        } else {
            1.0 / x - 1.0
        };
    }
    let z = if ix < 0x4036_0000 {
        if ix < 0x3c80_0000 {
            return x * (1.0 + x);
        }
        if ix >= 0x3ff0_0000 {
            let t = expm1(2.0 * fabs(x));
            1.0 - 2.0 / (t + 2.0)
        } else {
            let t = expm1(-2.0 * fabs(x));
            -t / (t + 2.0)
        }
    } else {
        1.0
    };
    if jx >= 0 { z } else { -z }
}

const LN2: f64 = 6.931_471_805_599_452_862_27e-01;

/// `asinh` (fdlibm `s_asinh.c`).
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn asinh(x: f64) -> f64 {
    let hx = hi(x) as i32;
    let ix = hx & 0x7fff_ffff;
    if ix >= 0x7ff0_0000 {
        return x + x;
    }
    if ix < 0x3e30_0000 {
        return x;
    }
    let w = if ix > 0x41b0_0000 {
        log(fabs(x)) + LN2
    } else if ix > 0x4000_0000 {
        let t = fabs(x);
        log(2.0 * t + 1.0 / (crate::ops::sqrt(x * x + 1.0) + t))
    } else {
        let t = x * x;
        log1p(fabs(x) + t / (1.0 + crate::ops::sqrt(1.0 + t)))
    };
    if hx > 0 { w } else { -w }
}

/// `acosh` for `x >= 1` (fdlibm `e_acosh.c`).
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn acosh(x: f64) -> f64 {
    let hx = hi(x) as i32;
    if hx >= 0x41b0_0000 {
        if hx >= 0x7ff0_0000 {
            return x + x;
        }
        return log(x) + LN2;
    }
    if x == 1.0 {
        return 0.0;
    }
    if hx > 0x4000_0000 {
        let t = x * x;
        return log(2.0 * x - 1.0 / (x + crate::ops::sqrt(t - 1.0)));
    }
    let t = x - 1.0;
    log1p(t + crate::ops::sqrt(2.0 * t + t * t))
}

/// `atanh` for `|x| < 1` (fdlibm `e_atanh.c`).
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn atanh(x: f64) -> f64 {
    let ax = fabs(x);
    if ax < 3.725_290_298_461_914e-9 {
        return x;
    }
    let t = if ax < 0.5 {
        let t = ax + ax;
        0.5 * log1p(t + t * ax / (1.0 - ax))
    } else {
        0.5 * log1p((ax + ax) / (1.0 - ax))
    };
    if x >= 0.0 { t } else { -t }
}
