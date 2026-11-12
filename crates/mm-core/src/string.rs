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
/// deterministic across runs, processes and platforms (docs/spec.md §16).
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
        Ok((ba == bb, ba.len().min(bb.len())))
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
        let cap = self.table_cap(table)?;
        let mask = cap - 1;
        #[allow(clippy::cast_possible_truncation, reason = "masked to the table size")]
        let mut i = hash as u32 & mask;
        for _ in 0..cap {
            let v = self.get(table, 1 + i)?;
            let Some(r) = v.as_obj() else {
                return Ok(None);
            };
            if self.get_raw(r, SLOT_HASH)? == hash && self.str_bytes(r)? == bytes {
                return Ok(Some(r));
            }
            i = (i + 1) & mask;
        }
        Ok(None)
    }

    /// Makes room for one more entry: creates the table or doubles it.
    fn reserve_intern(&mut self) -> Result<(), Error> {
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
        let table = self.alloc_object(types::INTERN_TABLE, 0, cap + 1, 0)?;
        self.write_slot(
            table,
            0,
            Value::int(0).map_or(0, Value::to_bits),
            true,
            false,
        )?;
        self.intern_table = Some(table);
        if let Some(old) = old {
            for i in 0..old_cap {
                if let Some(r) = self.get(old, 1 + i)?.as_obj() {
                    let hash = self.get_raw(r, SLOT_HASH)?;
                    self.insert_interned(r, hash)?;
                }
            }
        }
        Ok(())
    }

    /// After marking, remove dead strings in place. Backward-shifting a
    /// cluster closes each hole without tombstones or a rehash allocation.
    pub(crate) fn prune_intern(&mut self) -> Result<(), Error> {
        let Some(table) = self.intern_table else {
            return Ok(());
        };
        let cap = self.table_cap(table)?;
        let mask = cap - 1;
        let mut count = self.interned_count();
        for i in 0..cap {
            // A deletion may move another dead entry into this slot.
            while let Some(r) = self.get(table, 1 + i)?.as_obj() {
                let (seg, off) = crate::space::split(r.raw());
                if self
                    .words
                    .marks
                    .get(seg)
                    .is_some_and(|m| crate::space::bit(m, off))
                {
                    break;
                }
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
        }
        self.write_slot(
            table,
            0,
            Value::int(i64::from(count)).map_or(0, Value::to_bits),
            true,
            false,
        )
    }

    /// Adds `r` to the table; the caller has made room.
    fn insert_interned(&mut self, r: Ref, hash: u64) -> Result<(), Error> {
        let table = self.intern_table.ok_or(Error::BadRef)?;
        let mask = self.table_cap(table)? - 1;
        #[allow(clippy::cast_possible_truncation, reason = "masked to the table size")]
        let mut i = hash as u32 & mask;
        for _ in 0..mask {
            if self.get(table, 1 + i)?.as_obj().is_none() {
                break;
            }
            i = (i + 1) & mask;
        }
        if self.get(table, 1 + i)?.as_obj().is_some() {
            return Err(Error::OutOfMemory);
        }
        self.write_slot(table, 1 + i, Value::from_ref(r, 0).0, true, false)?;
        let count = i64::from(self.interned_count()) + 1;
        let count = Value::int(count).ok_or(Error::OutOfMemory)?;
        self.write_slot(table, 0, count.0, true, false)
    }
}
