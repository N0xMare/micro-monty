//! 8-byte NaN-boxed values (docs/spec.md §7.1).
//!
//! ```text
//! f64      any non-NaN double, or a canonical NaN: 0x7FF8_0000_0000_0000,
//!          or 0xFFF8_0000_0000_0000 with the sign bit (`float('-nan')`)
//! tagged   0xFFF8_0000_0000_0000 | tag << 48 | payload (48 bits)
//!   tag 1  SmallInt: i48, two's complement
//!   tag 2  Ref: kind byte << 32 | u32 word-space reference (bits 40..48 zero)
//!   tag 3  Special: 0 None, 1 False, 2 True, 3 NotImplemented, 4 Ellipsis,
//!          5 Undefined
//!   tag 4  NativeFn: u32 table index
//!   tag 5  ByteRef: u32 byte-space reference (internal, see below)
//! ```
//!
//! Every NaN is canonicalised on construction (keeping only its sign), so the
//! tagged space (negative quiet NaNs from 0xFFF9) never collides with a float, and two values are the
//! same object exactly when their bits are equal.
//!
//! `ByteRef` values exist so the GC can find byte-space blocks (`str` and
//! `bytes` data, bigint limbs) generically: an object owns a block by holding
//! its `ByteRef` in a value slot. The engine stores them only in its own
//! internal slots and never hands one to Python code; `Undefined` is internal
//! in the same way.

use crate::heap::{ByteRef, Ref};

const CANON_NAN: u64 = 0x7FF8_0000_0000_0000;
const CANON_NEG_NAN: u64 = 0xFFF8_0000_0000_0000;
const PAYLOAD: u64 = (1 << 48) - 1;
const HI_INT: u64 = 0xFFF9;
pub(crate) const HI_REF: u64 = 0xFFFA;
const HI_SPECIAL: u64 = 0xFFFB;
const HI_NATIVE: u64 = 0xFFFC;
pub(crate) const HI_BYTES: u64 = 0xFFFD;

/// One of the singleton values.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Special {
    /// `None`.
    None = 0,
    /// `False`.
    False = 1,
    /// `True`.
    True = 2,
    /// `NotImplemented`.
    NotImplemented = 3,
    /// `Ellipsis` (`...`).
    Ellipsis = 4,
    /// An unbound local or an empty slot; never visible to Python.
    Undefined = 5,
}

/// An 8-byte NaN-boxed value. `Copy`, pointer-free, and always a valid
/// encoding: constructors canonicalise or reject everything else.
///
/// `==` compares bits: identity, not Python equality.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[repr(transparent)]
pub struct Value(pub(crate) u64);

impl Value {
    /// `None`.
    pub const NONE: Value = Value::special(Special::None);
    /// `False`.
    pub const FALSE: Value = Value::special(Special::False);
    /// `True`.
    pub const TRUE: Value = Value::special(Special::True);
    /// `NotImplemented`.
    pub const NOT_IMPLEMENTED: Value = Value::special(Special::NotImplemented);
    /// `Ellipsis`.
    pub const ELLIPSIS: Value = Value::special(Special::Ellipsis);
    /// Unbound local or empty slot; fresh object value slots start as this.
    pub const UNDEFINED: Value = Value::special(Special::Undefined);
    /// Smallest `SmallInt`: -2^47.
    pub const SMALL_INT_MIN: i64 = -(1 << 47);
    /// Largest `SmallInt`: 2^47 - 1.
    pub const SMALL_INT_MAX: i64 = (1 << 47) - 1;

    /// A float; every NaN becomes the canonical NaN of its sign.
    #[must_use]
    pub const fn float(f: f64) -> Value {
        if f.is_nan() {
            Value(if f.is_sign_negative() {
                CANON_NEG_NAN
            } else {
                CANON_NAN
            })
        } else {
            Value(f.to_bits())
        }
    }

    /// A `SmallInt`, or `None` outside `SMALL_INT_MIN..=SMALL_INT_MAX` (the
    /// engine promotes those to a heap bigint).
    #[must_use]
    pub const fn int(i: i64) -> Option<Value> {
        if i < Self::SMALL_INT_MIN || i > Self::SMALL_INT_MAX {
            return None;
        }
        #[allow(clippy::cast_sign_loss, reason = "two's complement truncation to i48")]
        let payload = i as u64 & PAYLOAD;
        Some(Value(HI_INT << 48 | payload))
    }

    /// `True` or `False`.
    #[must_use]
    pub const fn from_bool(b: bool) -> Value {
        if b { Value::TRUE } else { Value::FALSE }
    }

    /// A singleton.
    #[must_use]
    pub const fn special(s: Special) -> Value {
        Value(HI_SPECIAL << 48 | s as u64)
    }

    /// A reference to a word-space object, with a kind byte the engine may use
    /// for type checks that skip the header (0 if unused).
    #[must_use]
    pub const fn from_ref(r: Ref, kind: u8) -> Value {
        Value(HI_REF << 48 | (kind as u64) << 32 | r.raw() as u64)
    }

    /// A reference to a byte-space block (internal; see the module docs).
    #[must_use]
    pub const fn from_byte_ref(b: ByteRef) -> Value {
        Value(HI_BYTES << 48 | b.raw() as u64)
    }

    /// A native function, by table index.
    #[must_use]
    pub const fn native(index: u32) -> Value {
        Value(HI_NATIVE << 48 | index as u64)
    }

    /// The value with these bits, or `None` if they are not a valid encoding
    /// (a non-canonical NaN, an unused tag, or stray payload bits). For
    /// decoding untrusted images; values built in memory are always valid.
    #[must_use]
    pub const fn from_bits(bits: u64) -> Option<Value> {
        let payload = bits & PAYLOAD;
        let ok = match bits >> 48 {
            HI_INT => true,
            HI_REF => payload >> 40 == 0,
            HI_SPECIAL => payload <= Special::Undefined as u64,
            HI_NATIVE | HI_BYTES => payload >> 32 == 0,
            0xFFFE | 0xFFFF => false,
            _ => bits == CANON_NAN || bits == CANON_NEG_NAN || !f64::from_bits(bits).is_nan(),
        };
        if ok { Some(Value(bits)) } else { None }
    }

    /// The raw encoding.
    #[must_use]
    pub const fn to_bits(self) -> u64 {
        self.0
    }

    /// Is this a float (including the canonical NaN)?
    #[must_use]
    pub const fn is_float(self) -> bool {
        self.0 >> 48 < HI_INT
    }

    /// The float, if this is one.
    #[must_use]
    pub const fn as_float(self) -> Option<f64> {
        if self.is_float() {
            Some(f64::from_bits(self.0))
        } else {
            None
        }
    }

    /// Is this a `SmallInt`?
    #[must_use]
    pub const fn is_int(self) -> bool {
        self.0 >> 48 == HI_INT
    }

    /// The `SmallInt`, sign-extended, if this is one.
    #[must_use]
    pub const fn as_int(self) -> Option<i64> {
        if self.is_int() {
            #[allow(
                clippy::cast_possible_wrap,
                reason = "sign extension of the i48 payload"
            )]
            let i = ((self.0 << 16) as i64) >> 16;
            Some(i)
        } else {
            None
        }
    }

    /// The word-space reference, if this is a `Ref`.
    #[must_use]
    pub const fn as_obj(self) -> Option<Ref> {
        if self.0 >> 48 == HI_REF {
            #[allow(
                clippy::cast_possible_truncation,
                reason = "low 32 bits are the reference"
            )]
            let raw = self.0 as u32;
            Some(Ref::from_raw(raw))
        } else {
            None
        }
    }

    /// The kind byte of a `Ref`.
    #[must_use]
    pub const fn obj_kind(self) -> Option<u8> {
        if self.0 >> 48 == HI_REF {
            #[allow(clippy::cast_possible_truncation, reason = "bits 32..40 are the kind")]
            let kind = (self.0 >> 32) as u8;
            Some(kind)
        } else {
            None
        }
    }

    /// Is this a `Ref` with kind byte `kind`? One compare.
    #[must_use]
    pub const fn is_obj_kind(self, kind: u8) -> bool {
        self.0 >> 32 == (HI_REF << 16 | kind as u64)
    }

    /// The singleton, if this is one.
    #[must_use]
    pub const fn as_special(self) -> Option<Special> {
        if self.0 >> 48 != HI_SPECIAL {
            return None;
        }
        Some(match self.0 & PAYLOAD {
            0 => Special::None,
            1 => Special::False,
            2 => Special::True,
            3 => Special::NotImplemented,
            4 => Special::Ellipsis,
            _ => Special::Undefined,
        })
    }

    /// The native-function index, if this is one.
    #[must_use]
    pub const fn as_native(self) -> Option<u32> {
        if self.0 >> 48 == HI_NATIVE {
            #[allow(clippy::cast_possible_truncation, reason = "payload is a u32")]
            let index = self.0 as u32;
            Some(index)
        } else {
            None
        }
    }

    /// The byte-space reference, if this is a `ByteRef`.
    #[must_use]
    pub const fn as_byte_ref(self) -> Option<ByteRef> {
        if self.0 >> 48 == HI_BYTES {
            #[allow(clippy::cast_possible_truncation, reason = "payload is a u32")]
            let raw = self.0 as u32;
            Some(ByteRef::from_raw(raw))
        } else {
            None
        }
    }
}
