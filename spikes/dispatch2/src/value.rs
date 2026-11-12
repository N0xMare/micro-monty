//! 8-byte NaN-boxed values, encoded as in docs/spec.md §7.1.
//!
//! ```text
//! f64      any non-NaN double, or the canonical NaN 0x7FF8_0000_0000_0000
//! tagged   0xFFF8_0000_0000_0000 | tag << 48 | payload (48 bits)
//!          tag 1 SmallInt (i48), 2 Ref (kind byte << 32 | u32 offset),
//!          3 Special (None/False/True/Undefined), 4 NativeFn (u32 index)
//! Copied from spike S1 (`spikes/dispatch/src/value.rs`), plus the frame link word.
//! ```

/// A NaN-boxed value.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(transparent)]
pub struct Value(pub u64);

const QNAN: u64 = 0xFFF8_0000_0000_0000;
const CANON_NAN: u64 = 0x7FF8_0000_0000_0000;
const PAYLOAD: u64 = (1 << 48) - 1;
const HI_INT: u64 = (QNAN >> 48) | 1;
const HI_REF: u64 = (QNAN >> 48) | 2;
const HI_SPECIAL: u64 = (QNAN >> 48) | 3;
const HI_NATIVE: u64 = (QNAN >> 48) | 4;

/// Object kinds carried in a Ref's spare bits (so hot type checks skip the header).
pub mod kind {
    pub const FUNC: u8 = 1;
    pub const CELL: u8 = 2;
    pub const STR: u8 = 3;
    pub const DICT: u8 = 6;
    pub const RANGE: u8 = 7;
    pub const BUF: u8 = 8;
    pub const EXC: u8 = 9;
    pub const INST: u8 = 10;
    pub const LIST: u8 = 11;
}

impl Value {
    pub const NONE: Value = Value(HI_SPECIAL << 48);
    pub const FALSE: Value = Value(HI_SPECIAL << 48 | 1);
    pub const TRUE: Value = Value(HI_SPECIAL << 48 | 2);
    /// Unbound local / empty dict slot; never visible to Python.
    pub const UNDEF: Value = Value(HI_SPECIAL << 48 | 5);

    #[inline(always)]
    pub fn int(i: i64) -> Value {
        Value(HI_INT << 48 | (i as u64 & PAYLOAD))
    }
    #[inline(always)]
    pub fn is_int(self) -> bool {
        self.0 >> 48 == HI_INT
    }
    /// Sign-extended SmallInt payload (only meaningful if `is_int`).
    #[inline(always)]
    pub fn int_val(self) -> i64 {
        ((self.0 << 16) as i64) >> 16
    }
    /// Does `i` fit in a SmallInt (i48)?
    #[inline(always)]
    pub fn fits_int(i: i64) -> bool {
        (i << 16) >> 16 == i
    }
    #[inline(always)]
    pub fn float(f: f64) -> Value {
        if f.is_nan() {
            Value(CANON_NAN)
        } else {
            Value(f.to_bits())
        }
    }
    #[inline(always)]
    pub fn is_float(self) -> bool {
        self.0 & QNAN != QNAN
    }
    #[inline(always)]
    pub fn float_val(self) -> f64 {
        f64::from_bits(self.0)
    }
    #[inline(always)]
    pub fn from_bool(b: bool) -> Value {
        Value(Self::FALSE.0 + b as u64)
    }
    #[inline(always)]
    pub fn obj(k: u8, addr: u32) -> Value {
        Value(HI_REF << 48 | (k as u64) << 32 | addr as u64)
    }
    /// Is this a Ref whose kind byte is `k`? One compare.
    #[inline(always)]
    pub fn is_obj(self, k: u8) -> bool {
        self.0 >> 32 == (HI_REF << 16 | k as u64)
    }
    #[inline(always)]
    pub fn is_ref(self) -> bool {
        self.0 >> 48 == HI_REF
    }
    #[inline(always)]
    pub fn addr(self) -> u32 {
        self.0 as u32
    }
    #[inline(always)]
    pub fn native(i: u32) -> Value {
        Value(HI_NATIVE << 48 | i as u64)
    }
    #[inline(always)]
    pub fn is_native(self) -> bool {
        self.0 >> 48 == HI_NATIVE
    }
    /// Native-function index (only meaningful if `is_native`).
    #[inline(always)]
    pub fn native_idx(self) -> u32 {
        self.0 as u32
    }

    /// A frame's link word (header slot 1): where the caller resumes.
    /// A SmallInt, so a linear scan of the value stack (GC roots, snapshot
    /// checks) never mistakes it for a reference:
    /// `special:1 | ret_pc:23 | caller code:16 | window offset:8`. `special`
    /// marks a frame whose return is not a plain return (generator,
    /// coroutine, continuation of a dunder call): `RETURN` takes the cold
    /// path for it.
    #[inline(always)]
    pub fn link(ret_pc: usize, code: usize, off: usize) -> Value {
        Value(
            HI_INT << 48
                | (ret_pc as u64 & 0x7F_FFFF) << 24
                | (code as u64 & 0xFFFF) << 8
                | (off as u64 & 0xFF),
        )
    }
    pub const LINK_SPECIAL: u64 = 1 << 47;
    #[inline(always)]
    pub fn link_special(self) -> bool {
        self.0 & Self::LINK_SPECIAL != 0
    }
    #[inline(always)]
    pub fn link_off(self) -> usize {
        self.0 as u8 as usize
    }
    #[inline(always)]
    pub fn link_code(self) -> usize {
        (self.0 >> 8) as u16 as usize
    }
    #[inline(always)]
    pub fn link_pc(self) -> usize {
        ((self.0 >> 24) & 0x7F_FFFF) as usize
    }
}
