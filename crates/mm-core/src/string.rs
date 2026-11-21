//! `str` objects, the string hash and the intern table (docs/spec.md §7.3).
//!
//! A `str` is a word-space object of type `types::STR`: value slot 0 holds the
//! `ByteRef` of its UTF-8 data, raw slot 1 the char count, raw slot 2 the hash,
//! computed on first use (`Heap::str_hash_mut`, or at construction for interned
//! strings) and then flagged `HASHED`; most strings are never hashed. The
//! `ASCII` flag is set when every byte is ASCII. Strings are immutable: engine
//! setters refuse mm-core objects.
//!
//! The intern table is an arena object (`types::INTERN_TABLE`) with open
//! addressing and linear probing over `str` refs, kept at most half full; it
//! grows by allocating a doubled table, so the old one is ordinary garbage.
//! Entries are weak: collection removes strings not reachable from ordinary
//! roots, preserving probe chains without allocating another table.

use crate::heap::{ByteRef, Error, Header, Heap, Ref, flags, types};
use crate::space::Store;
use crate::value::Value;
use alloc::vec::Vec;

const SLOT_DATA: u32 = 0;
const SLOT_CHARS: u32 = 1;
const SLOT_HASH: u32 = 2;
const INTERN_MIN_CAP: u32 = 64;

/// The string hash, Fx-style: one multiply per 8 bytes with a fixed seed,
/// deterministic across runs, processes and platforms (docs/spec.md §5).
/// Each step is a folded multiply (the high and low halves of a 64x64-bit
/// product xor-ed together, as `rustc-hash` 2 does for byte strings), so every input bit
/// reaches the low bits that power-of-two tables index by; a plain Fx multiply
/// leaves the low bits blind to the high input bytes. The length is mixed in
/// last; the empty string hashes to 0, so `hash('')` and `hash(b'')` are 0
/// as in Python. Dict lookups and the intern table use this;
/// the engine derives Python's `hash()` from it. Like any fixed-seed hash it
/// is not resistant to chosen-key flooding.
#[must_use]
#[inline(never)]
pub fn hash_bytes(data: &[u8]) -> u64 {
    const K: u64 = 0xF135_7AEA_2E62_A9C5;
    const SEED: u64 = 0x243F_6A88_85A3_08D3;
    if data.is_empty() {
        return 0;
    }
    let mut h = SEED;
    let (chunks, rest) = data.as_chunks::<8>();
    for chunk in chunks {
        h = folded_mul(h ^ u64::from_le_bytes(*chunk), K);
    }
    if !rest.is_empty() {
        let mut w = [0u8; 8];
        w[..rest.len()].copy_from_slice(rest);
        h = folded_mul(h ^ u64::from_le_bytes(w), K);
    }
    folded_mul(h ^ data.len() as u64, K.rotate_left(32))
}

/// [`hash_bytes`] as a `const fn`, for tables built at compile time (a
/// slower loop; the tests check both agree).
#[must_use]
pub const fn hash_bytes_const(data: &[u8]) -> u64 {
    const K: u64 = 0xF135_7AEA_2E62_A9C5;
    const SEED: u64 = 0x243F_6A88_85A3_08D3;
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_lossless,
        reason = "the two halves of the product; `From` is not const"
    )]
    const fn folded_mul(a: u64, b: u64) -> u64 {
        let p = a as u128 * b as u128;
        p as u64 ^ (p >> 64) as u64
    }
    if data.is_empty() {
        return 0;
    }
    let mut h = SEED;
    let mut i = 0;
    while i < data.len() {
        let mut w = [0u8; 8];
        let mut k = 0;
        while k < 8 && i + k < data.len() {
            w[k] = data[i + k];
            k += 1;
        }
        h = folded_mul(h ^ u64::from_le_bytes(w), K);
        i += 8;
    }
    folded_mul(h ^ data.len() as u64, K.rotate_left(32))
}

/// The high and low halves of `a * b`'s 128-bit product, xor-ed.
#[cfg(not(target_arch = "wasm32"))]
#[inline]
fn folded_mul(a: u64, b: u64) -> u64 {
    let p = u128::from(a) * u128::from(b);
    #[allow(
        clippy::cast_possible_truncation,
        reason = "the two halves of the product"
    )]
    let (lo, hi) = (p as u64, (p >> 64) as u64);
    lo ^ hi
}

/// [`folded_mul`] where no 64x64-bit multiply gives 128 bits: wasm32
/// compiles a `u128` product to a call of compiler-builtins' `__multi3`,
/// a full 128x128-bit multiply through memory. The same value from four
/// 32x32-bit products: a restore checks every hashed str's hash against
/// [`hash_bytes`] and the dunder table is hashed at compile time with the
/// 128-bit product, so the wasm conformance run and the cross-target
/// durability run fail if the two forms ever differ.
#[cfg(target_arch = "wasm32")]
#[inline]
fn folded_mul(a: u64, b: u64) -> u64 {
    let (a_lo, a_hi) = (a & 0xFFFF_FFFF, a >> 32);
    let (b_lo, b_hi) = (b & 0xFFFF_FFFF, b >> 32);
    // No sum overflows: (2^32 - 1)^2 + 2 * (2^32 - 1) = 2^64 - 1.
    let low = a_lo * b_lo;
    let mid = a_hi * b_lo + (low >> 32);
    let cross = a_lo * b_hi + (mid & 0xFFFF_FFFF);
    let high = a_hi * b_hi + (mid >> 32) + (cross >> 32);
    a.wrapping_mul(b) ^ high
}

/// Positions in non-ASCII strings remembered by [`Heap::str_offset`] and
/// [`Heap::str_index`].
const POS_WAYS: usize = 8;

/// The longest walk that moves the position it started from instead of
/// adding one.
const STEP_BYTES: usize = 256;

/// Recently found positions in non-ASCII `str`s, each (string, char index,
/// byte offset of that char), so walking to a char index starts from the
/// nearest of the string's ends and these: sequential and nearby indexing
/// costs about one char per step instead of a walk from an end. Only for
/// `Heap`'s own use: [`Heap::collect`] clears it before sweep can reuse an
/// address, so an entry never outlives its string, and it is not part of
/// a snapshot (a restored heap starts empty).
#[derive(Default)]
pub(crate) struct StrPos {
    /// At most `POS_WAYS` positions: string, [char, byte].
    at: Vec<(Ref, [usize; 2])>,
    /// The slot a new position takes once all are used.
    next: usize,
}

impl StrPos {
    /// Forgets every position.
    pub(crate) fn clear(&mut self) {
        self.at.clear();
    }

    /// Notes position `pos` ([char, byte]) of `r`, found `walked` bytes
    /// from `entry`: over that entry after a short walk (a cursor stepping
    /// along a string keeps one entry), else in a new or the oldest slot,
    /// so two cursors far apart in one string do not take turns moving one
    /// entry between them.
    fn remember(&mut self, entry: Option<usize>, walked: usize, r: Ref, pos: [usize; 2]) {
        let i = match entry {
            Some(i) if walked <= STEP_BYTES => i,
            _ if self.at.len() < POS_WAYS => {
                self.at.push((r, pos));
                return;
            }
            _ => {
                let i = self.next;
                self.next = (i + 1) % POS_WAYS;
                i
            }
        };
        if let Some(e) = self.at.get_mut(i) {
            *e = (r, pos);
        }
    }
}

/// Is `b` the first byte of a UTF-8 sequence?
#[inline]
fn lead(b: u8) -> bool {
    b & 0xC0 != 0x80
}

/// Lead bytes in eight bytes: those other than `10xxxxxx`.
#[inline]
fn leads8(w: [u8; 8]) -> usize {
    const HI: u64 = 0x8080_8080_8080_8080;
    let w = u64::from_le_bytes(w);
    // Bit 7 of each continuation byte (bit 7 set, bit 6 shifted up into
    // bit 7 of the same byte clear), moved to bit 0 and summed into the
    // top byte by the multiply.
    let cont = (w & !(w << 1) & HI) >> 7;
    8 - (cont.wrapping_mul(0x0101_0101_0101_0101) >> 56) as usize
}

/// `a == b`. On musl 32 bytes at a time: a slice compare is one libc
/// `memcmp` call, a byte loop there (about 7× glibc's time on 60 MB
/// \[M\]), where fixed-size compares inline to vector code. Elsewhere the
/// libc's `memcmp` (or the wasm guest's) is faster than the chunks: 10%
/// on glibc \[M\].
#[must_use]
#[inline]
pub fn bytes_eq(a: &[u8], b: &[u8]) -> bool {
    #[cfg(target_env = "musl")]
    {
        let ((wa, ta), (wb, tb)) = (a.as_chunks::<32>(), b.as_chunks::<32>());
        a.len() == b.len() && wa.iter().zip(wb).all(|(x, y)| x == y) && ta == tb
    }
    #[cfg(not(target_env = "musl"))]
    {
        a == b
    }
}

/// Lead bytes in `b`: the chars that start in it.
fn count_leads(b: &[u8]) -> usize {
    let (chunks, rest) = b.as_chunks::<8>();
    // The rest padded with continuation bytes, which count nothing.
    let mut last = [0x80; 8];
    if let Some(l) = last.get_mut(..rest.len()) {
        l.copy_from_slice(rest);
    }
    chunks.iter().fold(leads8(last), |n, c| n + leads8(*c))
}

/// The offset `p >= from` with `k` lead bytes in `b[from..p]` and a lead
/// byte (or the end) at `p`: `k` chars on from `from`, clamped to the end.
fn walk_forward(b: &[u8], from: usize, k: usize) -> usize {
    let mut rem = k;
    let mut p = from;
    while let Some(w) = b.get(p..p + 8).and_then(|w| <[u8; 8]>::try_from(w).ok()) {
        let c = leads8(w);
        if c > rem {
            break;
        }
        rem -= c;
        p += 8;
    }
    while let Some(&x) = b.get(p) {
        if lead(x) {
            if rem == 0 {
                return p;
            }
            rem -= 1;
        }
        p += 1;
    }
    b.len()
}

/// The offset `p <= to` at a lead byte with `k` lead bytes in `b[p..to]`:
/// `k` chars back from `to`, clamped to the start.
fn walk_back(b: &[u8], to: usize, k: usize) -> usize {
    let mut rem = k;
    let mut p = to.min(b.len());
    while let Some(w) = p
        .checked_sub(8)
        .and_then(|s| b.get(s..p))
        .and_then(|w| <[u8; 8]>::try_from(w).ok())
    {
        let c = leads8(w);
        if c >= rem {
            break;
        }
        rem -= c;
        p -= 8;
    }
    while rem > 0 && p > 0 {
        p -= 1;
        if b.get(p).is_some_and(|&x| lead(x)) {
            rem -= 1;
        }
    }
    p
}

/// Code points in `s`.
fn char_count(s: &str) -> usize {
    if s.is_ascii() {
        s.len()
    } else {
        s.chars().count()
    }
}

/// A `str` under construction: valid UTF-8 and its char count, built from
/// `&str` pieces and existing `str` objects without validating or counting
/// them again. [`Heap::new_str_from`] turns it into a `str`. Reuse one buffer
/// (`clear`) to avoid an allocation per string.
#[derive(Default, Debug)]
pub struct StrBuf {
    data: Vec<u8>,
    chars: usize,
}

impl StrBuf {
    /// An empty buffer.
    #[must_use]
    pub fn new() -> StrBuf {
        StrBuf::default()
    }

    /// Empties the buffer, keeping its capacity.
    pub fn clear(&mut self) {
        self.data.clear();
        self.chars = 0;
    }

    /// Length in bytes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Is the buffer empty?
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Capacity in bytes.
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.data.capacity()
    }

    /// Length in chars (code points).
    #[must_use]
    pub fn chars(&self) -> usize {
        self.chars
    }

    /// The text so far.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.data
    }

    /// Reserves exactly the required room for `n` more bytes, without geometric
    /// growth. The caller admits this capacity against its allocation budget.
    pub fn try_reserve_exact(&mut self, n: usize) -> Result<(), Error> {
        self.data
            .try_reserve_exact(n)
            .map_err(|_| Error::OutOfMemory)
    }

    /// Appends `s`.
    pub fn push_str(&mut self, s: &str) {
        self.data.extend_from_slice(s.as_bytes());
        self.chars += char_count(s);
    }

    /// Appends one char.
    pub fn push(&mut self, c: char) {
        let mut b = [0u8; 4];
        self.data
            .extend_from_slice(c.encode_utf8(&mut b).as_bytes());
        self.chars += 1;
    }

    /// Appends the whole of `str` `r`.
    pub fn push_ref(&mut self, heap: &Heap, r: Ref) -> Result<(), Error> {
        let (bytes, chars) = heap.str_data(r)?;
        self.data.extend_from_slice(bytes);
        self.chars += usize::try_from(chars).map_err(|_| Error::BadType)?;
        Ok(())
    }

    /// Appends `bytes`, which must be ASCII (`Error::BadType` otherwise,
    /// appending nothing).
    pub fn push_ascii(&mut self, bytes: &[u8]) -> Result<(), Error> {
        if !bytes.is_ascii() {
            return Err(Error::BadType);
        }
        self.data.extend_from_slice(bytes);
        self.chars += bytes.len();
        Ok(())
    }

    /// Appends bytes `start..end` of `str` `r`; both ends must be char
    /// boundaries (`Error::BadSlot` otherwise).
    pub fn push_slice(
        &mut self,
        heap: &Heap,
        r: Ref,
        start: usize,
        end: usize,
    ) -> Result<(), Error> {
        let ascii = heap.header(r)?.flags() & flags::ASCII != 0;
        let data = heap.str_bytes(r)?;
        let boundary = |i: usize| data.get(i).is_none_or(|&b| b & 0xC0 != 0x80);
        if start > end || end > data.len() || !boundary(start) || !boundary(end) {
            return Err(Error::BadSlot);
        }
        let piece = data.get(start..end).ok_or(Error::BadSlot)?;
        self.data.extend_from_slice(piece);
        self.chars += if ascii {
            piece.len()
        } else {
            piece.iter().filter(|&&b| b & 0xC0 != 0x80).count()
        };
        Ok(())
    }
}

impl Heap {
    /// Allocates a `str` holding a copy of `s`.
    pub fn new_str(&mut self, s: &str) -> Result<Ref, Error> {
        self.new_str_flagged(s, None)
    }

    /// Allocates a `str` holding the text of `b`.
    pub fn new_str_from(&mut self, b: &StrBuf) -> Result<Ref, Error> {
        self.new_str_counted(&b.data, b.chars, None)
    }

    fn new_str_flagged(&mut self, s: &str, hash: Option<u64>) -> Result<Ref, Error> {
        self.new_str_counted(s.as_bytes(), char_count(s), hash)
    }

    /// `bytes` is valid UTF-8 of `chars` code points (callers guarantee it).
    /// A `hash` given makes the string interned (and hashed).
    fn new_str_counted(
        &mut self,
        bytes: &[u8],
        chars: usize,
        hash: Option<u64>,
    ) -> Result<Ref, Error> {
        let data = self.alloc_bytes(bytes)?;
        let ascii = if chars == bytes.len() {
            flags::ASCII
        } else {
            0
        };
        let extra = if hash.is_some() {
            flags::INTERNED | flags::HASHED
        } else {
            0
        };
        self.new_str_block(data, chars as u64, ascii, extra, hash.unwrap_or(0))
    }

    /// `a + b`: a new `str`, or `a` or `b` itself when the other is empty.
    pub fn str_concat(&mut self, a: Ref, b: Ref) -> Result<Ref, Error> {
        self.str_concat_n(a, b, usize::MAX).map(|(r, _)| r)
    }

    /// `str_concat`, with the bytes it copied (0 when it returned an
    /// operand), for callers that charge for them; `(a, usize::MAX)`,
    /// copying nothing, when the result would be `long` bytes or more (the
    /// caller builds that one in steps).
    pub fn str_concat_n(&mut self, a: Ref, b: Ref, long: usize) -> Result<(Ref, usize), Error> {
        let (ha, da, ca) = self.str_parts(a)?;
        let (hb, db, cb) = self.str_parts(b)?;
        if cb == 0 {
            return Ok((a, 0));
        }
        if ca == 0 {
            return Ok((b, 0));
        }
        let (sa, sb) = (self.bytes(da)?, self.bytes(db)?);
        let (la, lb) = (sa.len(), sb.len());
        let ascii = ha.flags() & hb.flags() & flags::ASCII;
        // The parts are borrowed from the arena, which allocating needs
        // mutably: join short ones on the stack, copy long ones from block
        // to block (one copy, no buffer).
        let mut small = [0u8; 64];
        let data = if let Some(out) = small.get_mut(..la + lb) {
            let (x, y) = out.split_at_mut(la);
            x.copy_from_slice(sa);
            y.copy_from_slice(sb);
            self.alloc_bytes(out)?
        } else {
            let Some(d) = self.concat_blocks(da, la, db, lb, long)? else {
                return Ok((a, usize::MAX));
            };
            d
        };
        let r = self.new_str_block(data, ca + cb, ascii, 0, 0)?;
        Ok((r, la + lb))
    }

    /// A new block of the first `la` bytes of `da` then the first `lb` of
    /// `db`, one copy each, or `None` when that is `long` bytes or more
    /// (out of line, off the short concatenation's path).
    #[inline(never)]
    fn concat_blocks(
        &mut self,
        da: ByteRef,
        la: usize,
        db: ByteRef,
        lb: usize,
        long: usize,
    ) -> Result<Option<ByteRef>, Error> {
        if la.saturating_add(lb) >= long {
            return Ok(None);
        }
        let d = self.alloc_bytes_zeroed(la + lb)?;
        self.copy_bytes(da, 0, d, 0, la)?;
        self.copy_bytes(db, 0, d, la, lb)?;
        Ok(Some(d))
    }

    /// A `str` over byte block `data`, which the caller has filled with
    /// valid UTF-8 of `chars` code points (a result built a slice at a
    /// time); the block must not change after this. The text is the
    /// block's whole length, as `Heap::bytes` gives it: a block written
    /// only in part must first be cut to the part written (its header
    /// length), or the rest becomes part of the str.
    pub fn str_from_block(&mut self, data: ByteRef, chars: u64) -> Result<Ref, Error> {
        let len = self.bytes(data)?.len() as u64;
        let ascii = if chars == len { flags::ASCII } else { 0 };
        self.new_str_block(data, chars, ascii, 0, 0)
    }

    /// The data block of `str` `r`, its char count, and whether it is
    /// ASCII (then the count is its length in bytes too).
    pub fn str_block(&self, r: Ref) -> Result<(ByteRef, u64, bool), Error> {
        let (h, d, c) = self.str_parts(r)?;
        Ok((d, c, h.flags() & flags::ASCII != 0))
    }

    /// The header, data block and char count of `str` `r`.
    #[inline]
    fn str_parts(&self, r: Ref) -> Result<(Header, ByteRef, u64), Error> {
        let (h, &[d, c, _]) = self.str_header_slots(r)? else {
            return Err(Error::BadType);
        };
        Ok((h, Value(d).as_byte_ref().ok_or(Error::BadType)?, c))
    }

    /// The `str` object over byte block `data` (valid UTF-8 of `chars`
    /// code points), with these flags and hash.
    fn new_str_block(
        &mut self,
        data: ByteRef,
        chars: u64,
        ascii: u8,
        extra: u8,
        hash: u64,
    ) -> Result<Ref, Error> {
        let (a, slots) = self.alloc_words(types::STR, ascii | extra, 1, 2)?;
        // In SLOT_DATA, SLOT_CHARS, SLOT_HASH order.
        if let [d, c, h] = slots {
            *d = Value::from_byte_ref(data).0;
            *c = chars;
            *h = hash;
        }
        Ok(Ref::from_raw(a))
    }

    /// The slots of `str` `r`: data, char count, hash.
    #[inline]
    fn str_slots(&self, r: Ref) -> Result<&[u64], Error> {
        self.str_header_slots(r).map(|(_, w)| w)
    }

    /// The header and slots of `str` `r`.
    #[inline]
    fn str_header_slots(&self, r: Ref) -> Result<(Header, &[u64]), Error> {
        let (s, off) = self.words.block(r.raw()).ok_or(Error::BadRef)?;
        let h = Header::from_word(s.data.get(off).copied().ok_or(Error::BadRef)?);
        if h.type_id() != types::STR || h.len() != 3 {
            return Err(Error::BadType);
        }
        Ok((h, s.data.get(off + 1..off + 4).ok_or(Error::BadSlot)?))
    }

    /// The UTF-8 bytes of `str` `r`, without validation.
    pub fn str_bytes(&self, r: Ref) -> Result<&[u8], Error> {
        let data = self
            .str_slots(r)?
            .get(SLOT_DATA as usize)
            .copied()
            .unwrap_or(0);
        self.bytes(Value(data).as_byte_ref().ok_or(Error::BadType)?)
    }

    /// `str` `r` as `&str`, borrowed from the arena. Safe Rust has to validate
    /// the UTF-8, an O(n) pass; use `str_bytes` where bytes suffice.
    pub fn as_str(&self, r: Ref) -> Result<&str, Error> {
        core::str::from_utf8(self.str_bytes(r)?).map_err(|_| Error::BadType)
    }

    /// The hash of `str` `r` (`hash_bytes` of its data): the cached one, or
    /// computed (O(n)) without caching it.
    pub fn str_hash(&self, r: Ref) -> Result<u64, Error> {
        let (h, w) = self.str_header_slots(r)?;
        if h.flags() & flags::HASHED != 0 {
            return w.get(SLOT_HASH as usize).copied().ok_or(Error::BadSlot);
        }
        Ok(hash_bytes(self.str_bytes(r)?))
    }

    /// The hash of `str` `r`, computed and cached on first use.
    #[inline]
    pub fn str_hash_mut(&mut self, r: Ref) -> Result<u64, Error> {
        let (h, w) = self.str_header_slots(r)?;
        if h.flags() & flags::HASHED != 0 {
            return w.get(SLOT_HASH as usize).copied().ok_or(Error::BadSlot);
        }
        let data = w.get(SLOT_DATA as usize).copied().unwrap_or(0);
        self.str_hash_fill(r, h, data)
    }

    /// Computes and caches the hash of `str` `r` (header `h`, data slot
    /// `data`), with one lookup to read and one to write.
    fn str_hash_fill(&mut self, r: Ref, h: Header, data: u64) -> Result<u64, Error> {
        let hash = hash_bytes(self.bytes(Value(data).as_byte_ref().ok_or(Error::BadType)?)?);
        let (s, off) = self.words.block_mut(r.raw()).ok_or(Error::BadRef)?;
        s.data
            .write(off, h.with_flags(h.flags() | flags::HASHED).word());
        s.data.write(off + 1 + SLOT_HASH as usize, hash);
        Ok(hash)
    }

    /// The UTF-8 bytes and char count of `str` `r`, with one validation.
    #[inline]
    pub fn str_data(&self, r: Ref) -> Result<(&[u8], u64), Error> {
        let &[d, c, _] = self.str_slots(r)? else {
            return Err(Error::BadType);
        };
        Ok((
            self.bytes(Value(d).as_byte_ref().ok_or(Error::BadType)?)?,
            c,
        ))
    }

    /// Are `str`s `a` and `b` equal? Cached hashes that differ decide
    /// without reading the text. Also returns the number of bytes the test
    /// compared (0 when the lengths or cached hashes decided it), for
    /// callers that charge for it.
    #[inline]
    pub fn str_eq_n(&self, a: Ref, b: Ref) -> Result<(bool, usize), Error> {
        if a == b {
            return Ok((true, 0));
        }
        let (ha, wa) = self.str_header_slots(a)?;
        let (hb, wb) = self.str_header_slots(b)?;
        let (&[da, ca, xa], &[db, cb, xb]) = (wa, wb) else {
            return Err(Error::BadType);
        };
        if ca != cb || (ha.flags() & hb.flags() & flags::HASHED != 0 && xa != xb) {
            return Ok((false, 0));
        }
        let ba = self.bytes(Value(da).as_byte_ref().ok_or(Error::BadType)?)?;
        let bb = self.bytes(Value(db).as_byte_ref().ok_or(Error::BadType)?)?;
        Ok((bytes_eq(ba, bb), ba.len().min(bb.len())))
    }

    /// `str_eq_n`'s answer when it read fewer than 64 bytes of text; `None` when it
    /// read more, for a fast path that leaves charged work to others.
    pub fn str_eq_short(&self, a: Ref, b: Ref) -> Option<bool> {
        let (eq, n) = self.str_eq_n(a, b).ok()?;
        (n < 64).then_some(eq)
    }

    /// Allocates a `str` holding a copy of `bytes`, which must be ASCII
    /// (`Error::BadType` otherwise): no UTF-8 validation or char count.
    pub fn new_str_ascii(&mut self, bytes: &[u8]) -> Result<Ref, Error> {
        if !bytes.is_ascii() {
            return Err(Error::BadType);
        }
        self.new_str_counted(bytes, bytes.len(), None)
    }

    /// Is the hash of `str` `r` cached?
    #[must_use]
    pub fn str_hashed(&self, r: Ref) -> bool {
        self.str_header_slots(r)
            .is_ok_and(|(h, _)| h.flags() & flags::HASHED != 0)
    }

    /// The number of chars (code points) in `str` `r`.
    pub fn str_chars(&self, r: Ref) -> Result<u64, Error> {
        self.str_slots(r)?
            .get(SLOT_CHARS as usize)
            .copied()
            .ok_or(Error::BadSlot)
    }

    /// The byte offset of char `k` of `str` `r` (its length when `k` is at
    /// or past the end) and the bytes walked to find it. A non-ASCII string
    /// is walked from the nearest of its ends and the positions found
    /// lately (up to 8, cleared by every collection), and the result is
    /// remembered.
    pub fn str_offset(&mut self, r: Ref, k: usize) -> Result<(usize, usize), Error> {
        self.str_walk(r, k, false)
    }

    /// The char index of byte offset `at` of `str` `r` (chars that start
    /// before it; its char count when `at` is at or past the end) and the
    /// bytes walked to find it, as [`Heap::str_offset`] finds it.
    pub fn str_index(&mut self, r: Ref, at: usize) -> Result<(usize, usize), Error> {
        self.str_walk(r, at, true)
    }

    /// [`Heap::str_offset`] of char `x`, or [`Heap::str_index`] of byte
    /// offset `x` when `by_byte`.
    #[inline(never)]
    fn str_walk(&mut self, r: Ref, x: usize, by_byte: bool) -> Result<(usize, usize), Error> {
        let (head, data, chars) = self.str_parts(r)?;
        let text = self.bytes(data)?;
        let chars = usize::try_from(chars).map_err(|_| Error::BadType)?;
        // Positions are [char, byte]; `x` counts in `[axis]`, the answer
        // in `[1 - axis]`.
        let axis = usize::from(by_byte);
        let end = [chars, text.len()];
        if head.flags() & flags::ASCII != 0 || x == 0 || x >= end[axis] {
            return Ok((if x >= end[axis] { end[1 - axis] } else { x }, 0));
        }
        // The nearest known position and its entry.
        let mut from = (if x < end[axis] - x { [0, 0] } else { end }, None);
        for (e, &(s, p)) in self.str_pos.at.iter().enumerate() {
            if s == r && p[axis].abs_diff(x) < from.0[axis].abs_diff(x) {
                from = (p, Some(e));
            }
        }
        let ([base, start], entry) = from;
        let pos = if by_byte {
            let k = if x >= start {
                base + count_leads(text.get(start..x).unwrap_or(&[]))
            } else {
                base.saturating_sub(count_leads(text.get(x..start).unwrap_or(&[])))
            };
            [k, x]
        } else if x >= base {
            [x, walk_forward(text, start, x - base)]
        } else {
            [x, walk_back(text, start, base - x)]
        };
        let walked = pos[1].abs_diff(start);
        // Only a char's own offset is a position.
        if text.get(pos[1]).is_some_and(|&b| lead(b)) {
            self.str_pos.remember(entry, walked, r, pos);
        }
        Ok((pos[1 - axis], walked))
    }

    /// The interned `str` equal to `s`, creating and interning one if needed.
    /// Equal live strings give the same `Ref`. The caller must root it across GC.
    pub fn intern(&mut self, s: &str) -> Result<Ref, Error> {
        let hash = hash_bytes(s.as_bytes());
        if let Some(r) = self.find_hashed(s.as_bytes(), hash)? {
            return Ok(r);
        }
        self.reserve_intern()?;
        let r = self.new_str_flagged(s, Some(hash))?;
        self.insert_interned(r, hash)?;
        Ok(r)
    }

    /// Interns the existing `str` `r`: returns the interned string equal to it,
    /// which is `r` itself (now flagged `INTERNED`) if there was none.
    pub fn intern_str(&mut self, r: Ref) -> Result<Ref, Error> {
        if self.header(r)?.flags() & flags::INTERNED != 0 {
            return Ok(r);
        }
        let hash = self.str_hash_mut(r)?;
        let found = self.find_hashed(self.str_bytes(r)?, hash)?;
        if let Some(existing) = found {
            return Ok(existing);
        }
        self.reserve_intern()?;
        self.add_core_flag(r, flags::INTERNED)?;
        self.insert_interned(r, hash)?;
        Ok(r)
    }

    /// The interned `str` equal to `s`, if there is one. Allocates nothing.
    #[must_use]
    pub fn find_interned(&self, s: &str) -> Option<Ref> {
        self.find_hashed(s.as_bytes(), hash_bytes(s.as_bytes()))
            .ok()
            .flatten()
    }

    /// Number of interned strings.
    #[must_use]
    pub fn interned_count(&self) -> u32 {
        self.intern_table
            .and_then(|t| self.get(t, 0).ok())
            .and_then(Value::as_int)
            .and_then(|n| u32::try_from(n).ok())
            .unwrap_or(0)
    }

    fn table_cap(&self, table: Ref) -> Result<u32, Error> {
        Ok(self.header(table)?.len() - 1)
    }

    fn find_hashed(&self, bytes: &[u8], hash: u64) -> Result<Option<Ref>, Error> {
        let Some(table) = self.intern_table else {
            return Ok(None);
        };
        // The table's entries, read with one validation.
        let entries = self.slot_words(table)?.get(1..).ok_or(Error::BadSlot)?;
        let mask = entries.len().wrapping_sub(1);
        #[allow(clippy::cast_possible_truncation, reason = "masked to the table size")]
        let mut i = hash as usize & mask;
        for _ in 0..entries.len() {
            let Some(r) = entries.get(i).and_then(|&w| Value(w).as_obj()) else {
                return Ok(None);
            };
            if self.get_raw(r, SLOT_HASH)? == hash && self.str_bytes(r)? == bytes {
                return Ok(Some(r));
            }
            i = (i + 1) & mask;
        }
        Ok(None)
    }

    /// The intern table's slots, writable: its count, then its entries.
    /// Marks the table's segment dirty, as a value-slot write does.
    fn intern_slots_mut(&mut self, table: Ref) -> Result<&mut [u64], Error> {
        let (s, off) = self.words.block_mut(table.raw()).ok_or(Error::BadRef)?;
        let h = Header::from_word(s.data.read(off).ok_or(Error::BadRef)?);
        if h.type_id() != types::INTERN_TABLE {
            return Err(Error::BadType);
        }
        s.dirty = crate::space::DIRTY_REFS;
        s.data
            .get_mut(off + 1..off + 1 + h.len() as usize)
            .ok_or(Error::BadSlot)
    }

    /// Puts `entry` (an interned string's reference) in `slots` (an intern
    /// table's, after its count) at its hash's first free place; the caller
    /// has made room.
    fn place_interned(slots: &mut [u64], entry: u64, hash: u64) -> Result<(), Error> {
        let mask = slots.len().wrapping_sub(1);
        #[allow(clippy::cast_possible_truncation, reason = "masked to the table size")]
        let mut i = hash as usize & mask;
        for _ in 0..slots.len() {
            let slot = slots.get_mut(i).ok_or(Error::BadSlot)?;
            if Value(*slot).as_obj().is_none() {
                *slot = entry;
                return Ok(());
            }
            i = (i + 1) & mask;
        }
        Err(Error::OutOfMemory)
    }

    /// Makes room for one more entry: creates the table or doubles it.
    fn reserve_intern(&mut self) -> Result<(), Error> {
        if let Some(t) = self.intern_table {
            // The usual case, room already: count and capacity in one read.
            let w = self.slot_words(t)?;
            let count = w
                .first()
                .and_then(|&c| Value(c).as_int())
                .and_then(|n| usize::try_from(n).ok())
                .ok_or(Error::BadSlot)?;
            // Entries are the slots after the count.
            if (count + 1) * 2 < w.len() {
                return Ok(());
            }
        }
        self.grow_intern(self.interned_count() + 1)
    }

    /// Makes room for `n` interned strings in all, so a known batch (the
    /// builtins and the prelude) interns without rehashing on the way.
    ///
    /// # Errors
    ///
    /// `OutOfMemory` if the table does not fit the memory limit.
    pub fn reserve_interned(&mut self, n: u32) -> Result<(), Error> {
        self.grow_intern(n)
    }

    fn grow_intern(&mut self, n: u32) -> Result<(), Error> {
        let old = self.intern_table;
        let old_cap = match old {
            Some(t) => self.table_cap(t)?,
            None => 0,
        };
        let want = n.checked_mul(2).ok_or(Error::OutOfMemory)?;
        if want <= old_cap {
            return Ok(());
        }
        let cap = want
            .checked_next_power_of_two()
            .ok_or(Error::OutOfMemory)?
            .max(old_cap * 2)
            .max(INTERN_MIN_CAP);
        // The old entries with their hashes, in table order (each string's
        // hash is cached: interned strings are flagged `HASHED`).
        let mut moved = Vec::new();
        if let Some(old) = old {
            let entries = self.slot_words(old)?.get(1..).ok_or(Error::BadSlot)?;
            moved
                .try_reserve_exact(
                    entries
                        .iter()
                        .filter(|&&w| Value(w).as_obj().is_some())
                        .count(),
                )
                .map_err(|_| Error::OutOfMemory)?;
            for &w in entries {
                if let Some(r) = Value(w).as_obj() {
                    moved.push((w, self.get_raw(r, SLOT_HASH)?));
                }
            }
        }
        let table = self.alloc_object(types::INTERN_TABLE, 0, cap + 1, 0)?;
        let count = Value::int(i64::try_from(moved.len()).map_err(|_| Error::OutOfMemory)?)
            .ok_or(Error::OutOfMemory)?;
        let slots = self.intern_slots_mut(table)?;
        let (head, entries) = slots.split_first_mut().ok_or(Error::BadSlot)?;
        *head = count.0;
        for (w, hash) in moved {
            Self::place_interned(entries, w, hash)?;
        }
        self.intern_table = Some(table);
        Ok(())
    }

    /// After marking, remove dead strings in place. Backward-shifting a
    /// cluster closes each hole without tombstones or a rehash allocation.
    ///
    /// An entry is dead when its string's mark bit is clear: this runs at
    /// the end of a collection (`Heap::finish_collection`), in the step
    /// that drained the last marks and before any block is swept, so the
    /// bits are final and no entry's address has been reused. A slot holds
    /// an interned string's reference or no object (never stored to, or
    /// emptied here): there are no tombstones, so a lookup's probe ends at
    /// the first slot without an object, and every entry must stay
    /// reachable from its hash's slot over occupied slots only. Removing
    /// an entry therefore moves up each later entry of its cluster that
    /// the hole would cut off, until a slot without an object ends the
    /// cluster.
    ///
    /// The dead entries are taken in ascending slot order, each looked at
    /// again after its removal (the shift may have put another dead entry
    /// in its slot). Slots holding a live entry or none are only read, so
    /// they are found by one scan of the table's words
    /// ([`Heap::next_dead_interned`]) instead of a checked read of each: a
    /// collection that frees no interned string writes only the count.
    /// The count is written even then, as the walk of every slot wrote
    /// it: nothing reads the dirty flag that write sets, since the mark is
    /// idle here and the next one clears every segment's flag as it starts.
    pub(crate) fn prune_intern(&mut self) -> Result<(), Error> {
        let Some(table) = self.intern_table else {
            return Ok(());
        };
        let cap = self.table_cap(table)?;
        let mask = cap - 1;
        let mut count = self.interned_count();
        let mut i = 0;
        while let Some(dead) = self.next_dead_interned(table, i)? {
            i = dead;
            let mut hole = i;
            let mut j = (i + 1) & mask;
            for _ in 0..cap {
                let v = self.get(table, 1 + j)?;
                let Some(r) = v.as_obj() else { break };
                let bucket = u32::try_from(self.get_raw(r, SLOT_HASH)? & u64::from(mask))
                    .map_err(|_| Error::BadRef)?;
                if (hole.wrapping_sub(bucket) & mask) < (j.wrapping_sub(bucket) & mask) {
                    self.write_slot(table, 1 + hole, v.0, true, false)?;
                    hole = j;
                }
                j = (j + 1) & mask;
            }
            self.write_slot(table, 1 + hole, Value::UNDEFINED.0, true, false)?;
            count = count.saturating_sub(1);
        }
        self.write_slot(
            table,
            0,
            Value::int(i64::from(count)).map_or(0, Value::to_bits),
            true,
            false,
        )
    }

    /// The first slot at or after `from` of intern table `table` whose
    /// entry is dead (an object whose mark bit is clear), read from the
    /// table's words with one validation.
    fn next_dead_interned(&self, table: Ref, from: u32) -> Result<Option<u32>, Error> {
        let entries = self
            .slot_words(table)?
            .get(1 + from as usize..)
            .ok_or(Error::BadSlot)?;
        let dead = entries.iter().position(|&w| {
            Value(w).as_obj().is_some_and(|r| {
                let (seg, off) = crate::space::split(r.raw());
                !self
                    .words
                    .marks
                    .get(seg)
                    .is_some_and(|m| crate::space::bit(m, off))
            })
        });
        Ok(dead.and_then(|n| u32::try_from(n).ok()).map(|n| from + n))
    }

    /// Adds `r` to the table; the caller has made room.
    fn insert_interned(&mut self, r: Ref, hash: u64) -> Result<(), Error> {
        let table = self.intern_table.ok_or(Error::BadRef)?;
        let slots = self.intern_slots_mut(table)?;
        let (head, entries) = slots.split_first_mut().ok_or(Error::BadSlot)?;
        let count = Value(*head).as_int().ok_or(Error::BadSlot)? + 1;
        *head = Value::int(count).ok_or(Error::OutOfMemory)?.0;
        Self::place_interned(entries, Value::from_ref(r, 0).0, hash)
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::cast_possible_truncation,
        reason = "tests"
    )]

    use super::*;
    use alloc::{format, string::String, vec::Vec};

    /// The table a walk of every slot leaves: each slot in order, looked
    /// at again after a removal, over a copy of the entries. What
    /// `prune_intern` must leave, slot for slot.
    fn pruned_slot_by_slot(
        mut slots: Vec<u64>,
        hash: &dyn Fn(u64) -> u64,
        live: &dyn Fn(u64) -> bool,
    ) -> Vec<u64> {
        let object = |w: u64| Value(w).as_obj().is_some();
        let mask = slots.len() - 1;
        for i in 0..slots.len() {
            while object(slots[i]) && !live(slots[i]) {
                let mut hole = i;
                let mut j = (i + 1) & mask;
                while object(slots[j]) {
                    let bucket = hash(slots[j]) as usize & mask;
                    if (hole.wrapping_sub(bucket) & mask) < (j.wrapping_sub(bucket) & mask) {
                        slots[hole] = slots[j];
                        hole = j;
                    }
                    j = (j + 1) & mask;
                }
                slots[hole] = Value::UNDEFINED.0;
            }
        }
        slots
    }

    /// Over rounds of interning and collections that keep none, all, or a
    /// pseudo-random part of the strings, in tables of 64 to 1,024 slots
    /// with clusters that wrap past the last slot: the table is, slot for
    /// slot, the one the walk of every slot leaves, its count is the live
    /// strings', every live string is found as itself and no dead one is.
    #[test]
    fn a_prune_leaves_the_table_a_walk_of_every_slot_leaves() {
        let mut heap = Heap::default();
        let mut held: Vec<(String, Ref)> = Vec::new();
        let mut seed = 0x9e37_79b9_7f4a_7c15_u64;
        let mut removed = 0;
        for round in 0..40_u32 {
            // New names: a run that hashes to the table's last slots, so a
            // cluster wraps, then others; some rounds enough to grow it.
            let wrapping = (0..)
                .map(|i| format!("wrap{round}_{i}"))
                .filter(|s| hash_bytes(s.as_bytes()) & 63 >= 60)
                .take(6);
            let plain = (0..if round % 7 == 3 { 150 } else { 12 }).map(|i| format!("n{round}_{i}"));
            for name in wrapping.chain(plain) {
                let r = heap.intern(&name).unwrap();
                held.push((name, r));
            }
            let table = heap.intern_table.unwrap();
            let before = heap.slot_words(table).unwrap()[1..].to_vec();
            let hashes: Vec<(u64, u64)> = held
                .iter()
                .map(|(_, r)| {
                    let hash = heap.get_raw(*r, SLOT_HASH).unwrap();
                    (Value::from_ref(*r, 0).0, hash)
                })
                .collect();
            let keep: Vec<bool> = held
                .iter()
                .map(|_| {
                    seed = seed
                        .wrapping_mul(6_364_136_223_846_793_005)
                        .wrapping_add(1_442_695_040_888_963_407);
                    match round % 5 {
                        0 => true,
                        4 => false,
                        k => (seed >> 33) % 4 < u64::from(k),
                    }
                })
                .collect();
            let hash = |w: u64| hashes.iter().find(|(e, _)| *e == w).unwrap().1;
            let live = |w: u64| {
                let at = hashes.iter().position(|(e, _)| *e == w).unwrap();
                keep[at]
            };
            let want = pruned_slot_by_slot(before.clone(), &hash, &live);
            removed += before.iter().zip(&want).filter(|(b, w)| b != w).count();

            heap.collect(&mut |t| {
                for ((_, r), keep) in held.iter().zip(&keep) {
                    if *keep {
                        t.value(Value::from_ref(*r, types::STR as u8));
                    }
                }
            });

            let after = &heap.slot_words(table).unwrap()[1..];
            assert_eq!(after, want.as_slice(), "round {round}");
            let kept = keep.iter().filter(|k| **k).count();
            assert_eq!(heap.interned_count() as usize, kept, "round {round}");
            for ((name, r), keep) in held.iter().zip(&keep) {
                assert_eq!(heap.find_interned(name), keep.then_some(*r), "{name}");
            }
            let mut keep = keep.iter();
            held.retain(|_| *keep.next().unwrap());
        }
        // The rounds did move entries, not only empty slots.
        assert!(removed > 500, "{removed}");
    }
}
