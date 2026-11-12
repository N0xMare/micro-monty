//! Every code, frame-stack and arena access goes through these helpers.
//!
//! Default build: ordinary bounds-checked indexing; the crate forbids `unsafe`.
//! `--features unchecked`: `get_unchecked`, to measure what bounds checks cost.
//! The unchecked build is for measurement only; it is unsound for untrusted
//! bytecode and is never what micro-monty would ship (docs/spec.md §5).

#[cfg(not(feature = "unchecked"))]
mod imp {
    #[inline(always)]
    pub fn rd8(code: &[u8], i: usize) -> u8 {
        code[i]
    }
    #[inline(always)]
    pub fn ld<T: Copy>(s: &[T], i: usize) -> T {
        s[i]
    }
    #[inline(always)]
    pub fn st<T>(s: &mut [T], i: usize, v: T) {
        s[i] = v;
    }
    #[inline(always)]
    pub fn sl<T>(s: &[T], start: usize, n: usize) -> &[T] {
        &s[start..start + n]
    }
    #[inline(always)]
    pub fn seg_ld(segs: &[Box<[u64]>], seg: usize, word: usize) -> u64 {
        segs[seg][word]
    }
    #[inline(always)]
    pub fn seg_st(segs: &mut [Box<[u64]>], seg: usize, word: usize, v: u64) {
        segs[seg][word] = v;
    }
}

#[cfg(feature = "unchecked")]
#[allow(unsafe_code, reason = "measurement-only feature `unchecked`")]
mod imp {
    #[inline(always)]
    pub fn rd8(code: &[u8], i: usize) -> u8 {
        debug_assert!(i < code.len());
        // SAFETY: trusted, hand-assembled bytecode only (measurement build).
        unsafe { *code.get_unchecked(i) }
    }
    #[inline(always)]
    pub fn ld<T: Copy>(s: &[T], i: usize) -> T {
        debug_assert!(i < s.len());
        // SAFETY: as above.
        unsafe { *s.get_unchecked(i) }
    }
    #[inline(always)]
    pub fn st<T>(s: &mut [T], i: usize, v: T) {
        debug_assert!(i < s.len());
        // SAFETY: as above.
        unsafe { *s.get_unchecked_mut(i) = v }
    }
    #[inline(always)]
    pub fn sl<T>(s: &[T], start: usize, n: usize) -> &[T] {
        debug_assert!(start + n <= s.len());
        // SAFETY: as above.
        unsafe { s.get_unchecked(start..start + n) }
    }
    #[inline(always)]
    pub fn seg_ld(segs: &[Box<[u64]>], seg: usize, word: usize) -> u64 {
        debug_assert!(seg < segs.len() && word < segs[seg].len());
        // SAFETY: as above.
        unsafe { *segs.get_unchecked(seg).get_unchecked(word) }
    }
    #[inline(always)]
    pub fn seg_st(segs: &mut [Box<[u64]>], seg: usize, word: usize, v: u64) {
        debug_assert!(seg < segs.len() && word < segs[seg].len());
        // SAFETY: as above.
        unsafe { *segs.get_unchecked_mut(seg).get_unchecked_mut(word) = v }
    }
}

pub use imp::{ld, rd8, seg_ld, seg_st, sl, st};

/// Little-endian u16 operand (jump targets).
#[inline(always)]
pub fn rd16(code: &[u8], i: usize) -> usize {
    rd8(code, i) as usize | (rd8(code, i + 1) as usize) << 8
}
