//! `Value` encoding round-trips at the edges.

#![allow(clippy::unwrap_used, clippy::float_cmp, reason = "tests")]

use mm_core::{ByteRef, Ref, Special, Value};

const SPECIALS: [(Special, Value); 6] = [
    (Special::None, Value::NONE),
    (Special::False, Value::FALSE),
    (Special::True, Value::TRUE),
    (Special::NotImplemented, Value::NOT_IMPLEMENTED),
    (Special::Ellipsis, Value::ELLIPSIS),
    (Special::Undefined, Value::UNDEFINED),
];

/// Exactly one decoder answers for every value.
fn kinds(v: Value) -> usize {
    [
        v.as_float().is_some(),
        v.as_int().is_some(),
        v.as_obj().is_some(),
        v.as_special().is_some(),
        v.as_native().is_some(),
        v.as_byte_ref().is_some(),
    ]
    .iter()
    .filter(|&&b| b)
    .count()
}

fn roundtrip(v: Value) {
    assert_eq!(kinds(v), 1, "{v:?}");
    assert_eq!(Value::from_bits(v.to_bits()), Some(v));
}

#[test]
fn value_is_eight_bytes() {
    assert_eq!(size_of::<Value>(), 8);
    assert_eq!(size_of::<Option<Value>>(), 16);
}

#[test]
fn small_int_bounds() {
    for i in [
        0,
        1,
        -1,
        42,
        Value::SMALL_INT_MAX,
        Value::SMALL_INT_MIN,
        Value::SMALL_INT_MAX - 1,
        Value::SMALL_INT_MIN + 1,
        i64::from(i32::MAX),
        i64::from(i32::MIN),
    ] {
        let v = Value::int(i).unwrap();
        assert_eq!(v.as_int(), Some(i));
        assert!(v.is_int() && !v.is_float());
        roundtrip(v);
    }
    for i in [
        Value::SMALL_INT_MAX + 1,
        Value::SMALL_INT_MIN - 1,
        i64::MAX,
        i64::MIN,
        1 << 48,
    ] {
        assert_eq!(Value::int(i), None, "{i}");
    }
    assert_eq!(Value::SMALL_INT_MAX, (1 << 47) - 1);
}

#[test]
fn floats_at_the_edges() {
    for f in [
        0.0,
        -0.0,
        1.5,
        -1.5,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::MAX,
        f64::MIN,
        f64::MIN_POSITIVE,
        f64::EPSILON,
        5e-324,
        -5e-324,
    ] {
        let v = Value::float(f);
        assert!(v.is_float());
        assert_eq!(v.as_float().unwrap().to_bits(), f.to_bits(), "{f}");
        roundtrip(v);
    }
    // -0.0 and 0.0 are different values (bitwise identity).
    assert_ne!(Value::float(0.0), Value::float(-0.0));
}

#[test]
fn every_nan_is_canonicalised() {
    // One canonical NaN per sign (`float('-nan')` keeps its sign bit).
    let canon = Value::float(f64::NAN);
    let neg = Value::float(-f64::NAN);
    assert!(canon.as_float().unwrap().is_nan());
    assert!(!canon.as_float().unwrap().is_sign_negative());
    assert!(neg.as_float().unwrap().is_sign_negative());
    assert_eq!(neg.to_bits(), 0xFFF8_0000_0000_0000);
    roundtrip(canon);
    roundtrip(neg);
    for bits in [
        0x7FF0_0000_0000_0001,
        0x7FF8_0000_0000_0001,
        0x7FFF_FFFF_FFFF_FFFF,
        0xFFF0_0000_0000_0001,
        0xFFF8_0000_0000_0000,
        0xFFF9_0000_0000_0000, // what a SmallInt 0 would look like
        0xFFFA_0000_0000_1234, // what a Ref would look like
        0xFFFF_FFFF_FFFF_FFFF,
    ] {
        let f = f64::from_bits(bits);
        assert!(f.is_nan());
        let (pos, negf) = if f.is_sign_negative() {
            (-f, f)
        } else {
            (f, -f)
        };
        assert_eq!(Value::float(pos), canon, "{bits:#x}");
        assert_eq!(Value::float(negf), neg, "{bits:#x}");
    }
    let inf_nan = f64::INFINITY - f64::INFINITY;
    assert!(Value::float(inf_nan) == canon || Value::float(inf_nan) == neg);
}

#[test]
fn specials() {
    for (s, v) in SPECIALS {
        assert_eq!(Value::special(s), v);
        assert_eq!(v.as_special(), Some(s));
        roundtrip(v);
    }
    assert_eq!(Value::from_bool(true), Value::TRUE);
    assert_eq!(Value::from_bool(false), Value::FALSE);
    let all: std::collections::HashSet<Value> = SPECIALS.iter().map(|p| p.1).collect();
    assert_eq!(all.len(), 6);
}

#[test]
fn refs_natives_and_byte_refs() {
    for raw in [0, 1, 0x1_0000, u32::MAX] {
        for kind in [0, 1, 0x7F, 0xFF] {
            let v = Value::from_ref(Ref::from_raw(raw), kind);
            assert_eq!(v.as_obj(), Some(Ref::from_raw(raw)));
            assert_eq!(v.obj_kind(), Some(kind));
            assert!(v.is_obj_kind(kind));
            assert!(!v.is_obj_kind(kind.wrapping_add(1)));
            roundtrip(v);
        }
        let n = Value::native(raw);
        assert_eq!(n.as_native(), Some(raw));
        roundtrip(n);
        let b = Value::from_byte_ref(ByteRef::from_raw(raw));
        assert_eq!(b.as_byte_ref(), Some(ByteRef::from_raw(raw)));
        assert_eq!(b.as_obj(), None);
        roundtrip(b);
    }
    assert!(!Value::NONE.is_obj_kind(0));
    assert_eq!(Ref::from_raw(3 << 17 | 5).segment(), 3);
}

#[test]
fn from_bits_rejects_invalid_encodings() {
    for bits in [
        0x7FF8_0000_0000_0001, // non-canonical NaN
        0xFFF8_0000_0000_0001, // non-canonical negative quiet NaN
        0xFFF0_0000_0000_0001, // signalling NaN
        0xFFFA_0100_0000_0000, // Ref with stray bits above the kind
        0xFFFB_0000_0000_0006, // Special out of range
        0xFFFC_0001_0000_0000, // NativeFn payload over 32 bits
        0xFFFD_0001_0000_0000, // ByteRef payload over 32 bits
        0xFFFE_0000_0000_0000, // unused tag
        0xFFFF_0000_0000_0000, // unused tag
    ] {
        assert_eq!(Value::from_bits(bits), None, "{bits:#x}");
    }
    // Every valid bit pattern decodes to exactly one kind.
    let mut x: u64 = 0x1234_5678;
    for _ in 0..100_000 {
        x = x
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        for bits in [x, x | 0xFFF8 << 48, x | 0xFFF0 << 48] {
            if let Some(v) = Value::from_bits(bits) {
                assert_eq!(kinds(v), 1, "{bits:#x}");
                assert_eq!(v.to_bits(), bits);
            }
        }
    }
}
