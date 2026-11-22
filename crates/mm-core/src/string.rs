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
//! roots, preserving probe chains without allocating another table: a
//! cluster that holds a dead string is laid out again in its own slots,
//! its survivors sorted by bucket, and a doubled table takes the entries
//! sorted too, with no probe. Entry by entry, both take time quadratic in
//! a cluster's length, in one step.

use crate::heap::{ByteRef, Error, Header, Heap, Ref, flags, types};
use crate::space::Store;
use crate::value::Value;
use alloc::vec::Vec;

const SLOT_DATA: u32 = 0;
const SLOT_CHARS: u32 = 1;
pub(crate) const SLOT_HASH: u32 = 2;
const INTERN_MIN_CAP: u32 = 64;

/// The most slots an interned string lies past its hash's slot, so a
/// lookup reads at most this many and one more, whatever names a guest
/// picks by `hash()` (docs/spec.md §5). An insert with no free slot that
/// near doubles the table first. Names with random hashes come nowhere
/// near it: the longest walk in 300 half-full tables of 65,536 slots was
/// 44 \[M\].
pub const INTERN_WALK_CAP: usize = 128;

/// A reset replaces an intern table of at least this many times the
/// slots of the one its entries get then: one of
/// [`INTERN_SHRINK_ROOM`] slots an entry (`Heap::shrink_interned`). At
/// 16, the largest table shrinks with at most 65,536 entries, a sort of
/// 26 ms; at 4, with 262,144, 69 ms \[M, shipped\].
const INTERN_SHRINK_FROM: usize = 16;

/// Slots an entry in the table a reset shrinks to, before rounding up to
/// a power of two: a quarter full or less, so names interned next do not
/// double it at once.
const INTERN_SHRINK_ROOM: usize = 4;

/// What a lookup of a string in the intern table found.
enum Probe {
    /// The interned string equal to it.
    Found(Ref),
    /// None, and the free slot (an index into the entries) where it goes,
    /// in a table that stays at most half full with it.
    Free(usize),
    /// None, and no slot to take as the table is: none free within
    /// [`INTERN_WALK_CAP`] of its hash's, a table half full, or no table.
    NoRoom,
}

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
        let probe = match self.probe_interned(s.as_bytes(), hash)? {
            Probe::Found(r) => return Ok(r),
            probe => probe,
        };
        // Room before the string exists: a refused table leaves nothing.
        let slot = self.reserve_intern(hash, &probe)?;
        let r = self.new_str_flagged(s, Some(hash))?;
        self.insert_interned(r, slot, hash)?;
        Ok(r)
    }

    /// Interns the existing `str` `r`: returns the interned string equal to it,
    /// which is `r` itself (now flagged `INTERNED`) if there was none.
    pub fn intern_str(&mut self, r: Ref) -> Result<Ref, Error> {
        if self.header(r)?.flags() & flags::INTERNED != 0 {
            return Ok(r);
        }
        let hash = self.str_hash_mut(r)?;
        let probe = match self.probe_interned(self.str_bytes(r)?, hash)? {
            Probe::Found(existing) => return Ok(existing),
            probe => probe,
        };
        // Room before the flag: a refused table leaves `r` as it was.
        let slot = self.reserve_intern(hash, &probe)?;
        self.add_core_flag(r, flags::INTERNED)?;
        self.insert_interned(r, slot, hash)?;
        Ok(r)
    }

    /// The interned `str` equal to `s`, if there is one. Allocates nothing.
    #[must_use]
    pub fn find_interned(&self, s: &str) -> Option<Ref> {
        match self.probe_interned(s.as_bytes(), hash_bytes(s.as_bytes())) {
            Ok(Probe::Found(r)) => Some(r),
            _ => None,
        }
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

    /// Looks for the interned string of `bytes` from its hash's slot on,
    /// over at most [`INTERN_WALK_CAP`] slots after it: no entry lies
    /// further (inserts, the prune and a doubling keep it so, and a
    /// checked restore refuses a table that does not).
    fn probe_interned(&self, bytes: &[u8], hash: u64) -> Result<Probe, Error> {
        let Some(table) = self.intern_table else {
            return Ok(Probe::NoRoom);
        };
        // The table's count and entries, read with one validation.
        let (count, entries) = self
            .slot_words(table)?
            .split_first()
            .ok_or(Error::BadSlot)?;
        let mask = entries.len().wrapping_sub(1);
        #[allow(clippy::cast_possible_truncation, reason = "masked to the table size")]
        let mut i = hash as usize & mask;
        let tag = hash_tag(hash);
        for _ in 0..entries.len().min(INTERN_WALK_CAP + 1) {
            let entry = Value(entries.get(i).copied().unwrap_or(Value::UNDEFINED.0));
            let Some(r) = entry.as_obj() else {
                // Free to take while the table stays under half full.
                let room = Value(*count)
                    .as_int()
                    .and_then(|n| usize::try_from(n).ok())
                    .is_some_and(|n| (n + 1) * 2 <= entries.len());
                return Ok(if room { Probe::Free(i) } else { Probe::NoRoom });
            };
            // An entry of another hash is passed by its word alone.
            if entry.obj_kind() == Some(tag)
                && self.get_raw(r, SLOT_HASH)? == hash
                && self.str_bytes(r)? == bytes
            {
                return Ok(Probe::Found(r));
            }
            i = (i + 1) & mask;
        }
        Ok(Probe::NoRoom)
    }

    /// The first free slot within [`INTERN_WALK_CAP`] of `hash`'s.
    fn free_interned(&self, table: Ref, hash: u64) -> Result<Option<usize>, Error> {
        let entries = self.slot_words(table)?.get(1..).ok_or(Error::BadSlot)?;
        let mask = entries.len().wrapping_sub(1);
        #[allow(clippy::cast_possible_truncation, reason = "masked to the table size")]
        let bucket = hash as usize;
        Ok((0..entries.len().min(INTERN_WALK_CAP + 1))
            .map(|k| (bucket + k) & mask)
            .find(|&i| entries.get(i).is_some_and(|&w| Value(w).as_obj().is_none())))
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

    /// Makes room for one more entry, of hash `hash`, which `probe` did
    /// not find: the free slot it goes to, the probe's own when it found
    /// one in a table with room.
    #[inline]
    fn reserve_intern(&mut self, hash: u64, probe: &Probe) -> Result<usize, Error> {
        match *probe {
            Probe::Free(slot) => Ok(slot),
            _ => self.make_intern_room(hash),
        }
    }

    /// [`Heap::reserve_intern`] with no slot to take as it is: creates
    /// the table, doubles it when it is half full, and doubles it again
    /// for as long as no slot within [`INTERN_WALK_CAP`] of the hash's is
    /// free: names crowded in one place whatever the table's size end in
    /// `OutOfMemory`, at the largest table an object may be.
    #[cold]
    #[inline(never)]
    fn make_intern_room(&mut self, hash: u64) -> Result<usize, Error> {
        // (Nothing, for a table with room that is only crowded there.)
        self.grow_intern(self.interned_count() + 1)?;
        let table = self.intern_table.ok_or(Error::BadRef)?;
        self.uncrowd_interned(table, hash)
    }

    /// A free slot for `hash` in `table` or, doubling until there is
    /// one, in the table that replaces it.
    fn uncrowd_interned(&mut self, mut table: Ref, hash: u64) -> Result<usize, Error> {
        loop {
            if let Some(slot) = self.free_interned(table, hash)? {
                return Ok(slot);
            }
            let shift = self.crowd_shift(table, hash)?;
            table = self.grow_interned(table, shift)?;
        }
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
        let want = n.checked_mul(2).ok_or(Error::OutOfMemory)?;
        let Some(mut old) = self.intern_table else {
            let cap = want
                .checked_next_power_of_two()
                .ok_or(Error::OutOfMemory)?
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
            return Ok(());
        };
        // A table with entries only ever doubles, as often as it takes
        // (more than once only for a batch reserved late).
        while want > self.table_cap(old)? {
            old = self.grow_interned(old, 1)?;
        }
        Ok(())
    }

    /// How many times intern table `table` must double for a string of
    /// hash `hash`, which finds no slot within [`INTERN_WALK_CAP`] of its
    /// own, to find one: the fewest after which at most that many of the
    /// entries near its slot stay near it (those whose hashes agree with
    /// it in the bits the larger table reads too), or as many as the
    /// largest table allows. The table then grows once, not a doubling at
    /// a time: names crowded on one place of every table would make it
    /// double up to the largest in one step (53 ms from 2,048 slots [M]).
    fn crowd_shift(&self, table: Ref, hash: u64) -> Result<u32, Error> {
        let entries = self.slot_words(table)?.get(1..).ok_or(Error::BadSlot)?;
        let cap = entries.len();
        let mut most = 0;
        while !crate::space::over_cap((cap << (most + 1)) + 2) {
            most += 1;
        }
        if most == 0 || cap == 0 {
            // As the allocation of a larger table would refuse it.
            return Err(self.refuse(cap as u64 * 16 + 8));
        }
        #[allow(clippy::cast_possible_truncation, reason = "masked to the table size")]
        let bucket = hash as usize;
        // An entry that can lie in the string's reach in a larger table
        // has its bucket within the cap of the string's there, and so
        // here: it lies at most two caps after it, or one before.
        let near = (3 * INTERN_WALK_CAP + 1).min(cap);
        let first = bucket.wrapping_sub(INTERN_WALK_CAP);
        for shift in 1..most {
            let mask = (cap << shift) - 1;
            let mut stay = 0;
            for k in 0..near {
                let Some(r) = entries
                    .get(first.wrapping_add(k) & (cap - 1))
                    .and_then(|&w| Value(w).as_obj())
                else {
                    continue;
                };
                #[allow(clippy::cast_possible_truncation, reason = "masked to the table size")]
                let other = self.get_raw(r, SLOT_HASH)? as usize;
                let apart = other.wrapping_sub(bucket).wrapping_add(INTERN_WALK_CAP) & mask;
                stay += usize::from(apart <= 2 * INTERN_WALK_CAP);
            }
            if stay <= INTERN_WALK_CAP {
                return Ok(shift);
            }
        }
        Ok(most)
    }

    /// Replaces intern table `old` by one of `1 << shift` times its slots
    /// (twice, for a table half full), with no probe; the new table. The
    /// entries are taken in table order from the slot after a free one,
    /// the origin (so no cluster is cut). Each has a key, how many
    /// buckets after the origin its bucket lies in the new table: its
    /// low bits say the same of the old table, and the bits above them
    /// which of `1 << shift` runs the entry joins, an old table apart
    /// from the next. A cluster's buckets lie within its own slots, so
    /// with each cluster's entries sorted by key, every run's entries
    /// come in bucket order over the whole pass: an entry goes to its
    /// bucket or right after the one before it in its run, and no run
    /// reaches the next (a table is at most half full). Probing for each
    /// entry instead takes time quadratic in a cluster's length, in one
    /// step (docs/spec.md §5).
    fn grow_interned(&mut self, old: Ref, shift: u32) -> Result<Ref, Error> {
        let entries = self.slot_words(old)?.get(1..).ok_or(Error::BadSlot)?;
        let old_cap = entries.len();
        // Refused before any work, as the allocation would refuse it: a
        // crowded insert at the largest table costs no pass over it.
        if crate::space::over_cap((old_cap << shift) + 2) {
            return Err(self.refuse((old_cap as u64) << shift << 3));
        }
        let mask = old_cap.wrapping_sub(1);
        let occupied = |w: &u64| Value(*w).as_obj().is_some();
        let mut words = Vec::new();
        let mut next = Vec::new();
        if words
            .try_reserve_exact(self.interned_count() as usize)
            .is_err()
            || next.try_reserve_exact(1 << shift).is_err()
        {
            return Err(Error::OutOfMemory);
        }
        let origin = entries
            .iter()
            .position(|w| !occupied(w))
            .map_or(0, |free| (free + 1) & mask);
        let (wrapped, first) = entries.split_at_checked(origin).ok_or(Error::BadSlot)?;
        let new_mask = (old_cap << shift).wrapping_sub(1);
        // One pass: each entry's word, kept in order within its cluster
        // as it is added (most clusters of names with any hashes are a
        // few entries); a long cluster out of order is sorted at its end.
        let (mut from, mut unsorted) = (0, false);
        for part in [first, wrapped] {
            for &w in part {
                let Some(r) = Value(w).as_obj() else {
                    if unsorted {
                        let n = words.len() - from;
                        sort_words(&mut words, from, n, usize::MAX);
                        unsorted = false;
                    }
                    from = words.len();
                    continue;
                };
                // (Only a forged table holds more entries than its count.)
                if words.len() == words.capacity() {
                    return Err(Error::BadSlot);
                }
                // An entry is a live string with its hash (the prune and
                // a checked restore keep it so): read by that guard.
                #[allow(clippy::cast_possible_truncation, reason = "masked to the table size")]
                let key =
                    (self.get_raw_guarded(r, SLOT_HASH) as usize).wrapping_sub(origin) & new_mask;
                let word = (key as u64) << 40 | w & ((1 << 40) - 1);
                let mut at = words.len();
                words.push(word);
                if unsorted || words.len() - from > SORT_IN_PLACE {
                    unsorted |= at > from && words.get(at - 1).is_some_and(|&last| last > word);
                    continue;
                }
                while at > from && words.get(at - 1).is_some_and(|&last| last > word) {
                    words.swap(at - 1, at);
                    at -= 1;
                }
            }
        }
        if unsorted {
            let n = words.len() - from;
            sort_words(&mut words, from, n, usize::MAX);
        }
        let cap = u32::try_from(old_cap << shift).map_err(|_| Error::OutOfMemory)?;
        let table = self.alloc_object(types::INTERN_TABLE, 0, cap + 1, 0)?;
        let count = Value::int(i64::try_from(words.len()).map_err(|_| Error::OutOfMemory)?)
            .ok_or(Error::OutOfMemory)?;
        let slots = self.intern_slots_mut(table)?;
        let (head, entries) = slots.split_first_mut().ok_or(Error::BadSlot)?;
        *head = count.0;
        let mask = entries.len().wrapping_sub(1);
        let old_bits = old_cap.trailing_zeros();
        // Each run's next free slot, not yet wrapped.
        next.extend((0..1_usize << shift).map(|run| origin + run * old_cap));
        let mut spill = 0;
        for word in words {
            #[allow(clippy::cast_possible_truncation, reason = "22 bits")]
            let bucket = origin + (word >> 40) as usize;
            let mut at = bucket;
            if let Some(free) = next.get_mut((bucket - origin) >> old_bits) {
                at = bucket.max(*free);
                *free = at + 1;
            }
            // Taken only in a forged table, whose entries are not where
            // probes look: the next free slot then, of one pass over the
            // table for all of them (`spill` never goes back).
            if entries.get(at & mask).is_some_and(occupied) {
                while entries.get(spill).is_some_and(occupied) {
                    spill += 1;
                }
                at = spill;
            }
            *entries.get_mut(at & mask).ok_or(Error::BadSlot)? = entry_of(word);
        }
        self.intern_table = Some(table);
        Ok(table)
    }

    /// Replaces an intern table far larger than its entries need by a
    /// small one; was it replaced? For a reset, right after a collection
    /// (every entry is live), which collects again to free the old table.
    /// Names crowded on one place grow the table to the largest, 32 MiB
    /// of the arena, which every collection then reads and every image
    /// holds, and no prune gives it back: a guest could otherwise grow
    /// and shrink it in a loop, a step each (docs/spec.md §7.3).
    ///
    /// The new table has `INTERN_SHRINK_ROOM` slots an entry or more
    /// and is laid out sorted by bucket, so every entry lies at the
    /// earliest slot any layout gives it. The old table stays, and this
    /// is `false`, when it is under `INTERN_SHRINK_FROM` times that
    /// size, when an entry would lie past [`INTERN_WALK_CAP`] in the new
    /// one, and when there is no memory for the pass or the new table.
    pub fn shrink_interned(&mut self) -> bool {
        let shrunk = self.shrunk_interned().unwrap_or(false);
        // A refusal here is no request of the guest's.
        self.take_refused();
        shrunk
    }

    fn shrunk_interned(&mut self) -> Result<bool, Error> {
        let Some(old) = self.intern_table else {
            return Ok(false);
        };
        let entries = self.slot_words(old)?.get(1..).ok_or(Error::BadSlot)?;
        let count = self.interned_count() as usize;
        let cap = (count * INTERN_SHRINK_ROOM)
            .next_power_of_two()
            .max(INTERN_MIN_CAP as usize);
        let mut words = Vec::new();
        if cap * INTERN_SHRINK_FROM > entries.len() || words.try_reserve_exact(count).is_err() {
            return Ok(false);
        }
        // Each entry as its bucket in the new table over its kind byte
        // and reference, as a growth takes them.
        for &w in entries {
            let Some(r) = Value(w).as_obj() else {
                continue;
            };
            // (Only a forged table holds more entries than its count.)
            if words.len() == words.capacity() {
                return Ok(false);
            }
            #[allow(clippy::cast_possible_truncation, reason = "masked to the table size")]
            let bucket = self.get_raw_guarded(r, SLOT_HASH) as usize & (cap - 1);
            words.push((bucket as u64) << 40 | w & ((1 << 40) - 1));
        }
        let n = words.len();
        sort_words(&mut words, 0, n, usize::MAX);
        // The entries that pass the table's end go to its start, ahead of
        // the ones whose buckets are there. How many they are is known
        // after one pass from an empty start, and a second pass from
        // behind that many gives the same number: a table under full has
        // a free slot before its end, from which both passes agree.
        let (lead, _) = lay_out_sorted(&words, cap, 0, &mut []);
        let (_, far) = lay_out_sorted(&words, cap, lead, &mut []);
        if far > INTERN_WALK_CAP {
            return Ok(false);
        }
        let slots = u32::try_from(cap + 1).map_err(|_| Error::OutOfMemory)?;
        let table = self.alloc_object(types::INTERN_TABLE, 0, slots, 0)?;
        let count = Value::int(i64::try_from(n).map_err(|_| Error::OutOfMemory)?)
            .ok_or(Error::OutOfMemory)?;
        let slots = self.intern_slots_mut(table)?;
        let (head, entries) = slots.split_first_mut().ok_or(Error::BadSlot)?;
        *head = count.0;
        lay_out_sorted(&words, cap, lead, entries);
        self.intern_table = Some(table);
        Ok(true)
    }

    /// After marking, remove dead strings in place, with no tombstones and
    /// no allocation: each cluster that holds one is laid out again whole
    /// (`lay_out_cluster`). Removing them one by one, each hole closed by
    /// shifting the rest of its cluster back, took time quadratic in a
    /// cluster of dead strings, in one step (docs/spec.md §5).
    ///
    /// An entry is dead when its string's mark bit is clear: this runs at
    /// the end of a collection (`Heap::finish_collection`), in the step
    /// that drained the last marks and before any block is swept, so the
    /// bits are final and no entry's address has been reused. A slot holds
    /// an interned string's reference or no object (never stored to, or
    /// emptied here): there are no tombstones, so a lookup's probe ends at
    /// the first slot without an object, and every entry must stay
    /// reachable from its hash's slot over occupied slots only.
    ///
    /// The clusters are taken in ascending slot order. Slots holding a
    /// live entry or none are only read, so a cluster's first dead entry
    /// is found by one scan of the table's words
    /// ([`Heap::next_dead_interned`]) instead of a checked read of each: a
    /// collection that frees no interned string writes only the count.
    /// The count is written even then: nothing reads the dirty flag that
    /// write sets, since the mark is idle here and the next one clears
    /// every segment's flag as it starts.
    pub(crate) fn prune_intern(&mut self) -> Result<(), Error> {
        let Some(table) = self.intern_table else {
            return Ok(());
        };
        let mut count = self.interned_count();
        let mut from = 0;
        while let Some(dead) = self.next_dead_interned(table, from)? {
            let (base, len, after) = self.cluster_of(table, dead)?;
            count = count.saturating_sub(self.lay_out_cluster(table, base, len)?);
            from = after;
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
    /// table's words with one validation. `from` may lie past the table:
    /// a cluster that wraps ends after its last slot.
    fn next_dead_interned(&self, table: Ref, from: usize) -> Result<Option<usize>, Error> {
        let entries = self.slot_words(table)?.get(1 + from..).unwrap_or(&[]);
        let dead = entries
            .iter()
            .position(|&w| Value(w).as_obj().is_some_and(|r| !self.str_marked(r)));
        Ok(dead.map(|n| from + n))
    }

    fn str_marked(&self, r: Ref) -> bool {
        let (seg, off) = crate::space::split(r.raw());
        self.words
            .marks
            .get(seg)
            .is_some_and(|m| crate::space::bit(m, off))
    }

    /// The cluster of slot `at`: its first slot, its length, and the
    /// first slot index after `at` that is past it. A cluster runs back to
    /// the slot after a free one and on to the next free one, around the
    /// table's end if it lies there. (In a table with no free slot, which
    /// no restore admits, it would be every slot once.)
    fn cluster_of(&self, table: Ref, at: usize) -> Result<(usize, usize, usize), Error> {
        let entries = self.slot_words(table)?.get(1..).ok_or(Error::BadSlot)?;
        let cap = entries.len();
        let mask = cap.wrapping_sub(1);
        let occupied = |i: usize| {
            entries
                .get(i & mask)
                .is_some_and(|&w| Value(w).as_obj().is_some())
        };
        let mut back = 0;
        while back + 1 < cap && occupied(at.wrapping_sub(back + 1)) {
            back += 1;
        }
        let mut ahead = 1;
        while back + ahead < cap && occupied(at + ahead) {
            ahead += 1;
        }
        Ok((at.wrapping_sub(back) & mask, back + ahead, at + ahead + 1))
    }

    /// Lays out again the cluster of `len` entries from slot `base` on
    /// (around the table's end, if it lies there) without its dead
    /// strings; how many those were. The survivors, sorted by bucket, go
    /// each to its bucket or right after the one before it, the earliest
    /// slot a probe for it reaches: a table every lookup reads as before,
    /// in O(len log len) whatever the buckets.
    fn lay_out_cluster(&mut self, table: Ref, base: usize, len: usize) -> Result<u32, Error> {
        let mask = (self.table_cap(table)? as usize).wrapping_sub(1);
        #[allow(clippy::cast_possible_truncation, reason = "masked to the table size")]
        let slot = |i: usize| 1 + ((base + i) & mask) as u32;
        // The survivors move to the cluster's end, each as its bucket's
        // distance from `base` over its entry's kind byte and reference.
        let mut first = len;
        for i in (0..len).rev() {
            let entry = self.get(table, slot(i))?;
            let Some(r) = entry.as_obj() else {
                continue;
            };
            if !self.str_marked(r) {
                continue;
            }
            // A forged table's entry may have no hash to read.
            let hash = self.get_raw(r, SLOT_HASH).unwrap_or(0);
            #[allow(clippy::cast_possible_truncation, reason = "masked to the table size")]
            let distance = (hash as usize).wrapping_sub(base) & mask;
            first -= 1;
            let kind = u64::from(entry.obj_kind().unwrap_or(0));
            let word = (distance as u64) << 40 | kind << 32 | u64::from(r.raw());
            self.write_slot(table, slot(first), word, true, false)?;
        }
        let slots = self.intern_slots_mut(table)?;
        let entries = slots.get_mut(1..).ok_or(Error::BadSlot)?;
        for i in 0..first {
            if let Some(e) = entries.get_mut((base + i) & mask) {
                *e = Value::UNDEFINED.0;
            }
        }
        sort_words(entries, (base + first) & mask, len - first, mask);
        let mut next = 0;
        for i in first..len {
            let Some(e) = entries.get_mut((base + i) & mask) else {
                break;
            };
            let word = core::mem::replace(e, Value::UNDEFINED.0);
            // Never past the slot it came from: a forged table's bucket
            // may lie outside its cluster.
            #[allow(clippy::cast_possible_truncation, reason = "a distance in the table")]
            let at = next.max((word >> 40) as usize).min(i);
            if let Some(e) = entries.get_mut((base + at) & mask) {
                *e = entry_of(word);
            }
            next = at + 1;
        }
        u32::try_from(first).map_err(|_| Error::BadSlot)
    }

    /// Adds `r`, of hash `hash`, to the table at entry `slot`, the free
    /// one `reserve_intern` gave.
    fn insert_interned(&mut self, r: Ref, slot: usize, hash: u64) -> Result<(), Error> {
        let table = self.intern_table.ok_or(Error::BadRef)?;
        let slots = self.intern_slots_mut(table)?;
        let (head, entries) = slots.split_first_mut().ok_or(Error::BadSlot)?;
        let count = Value(*head).as_int().ok_or(Error::BadSlot)? + 1;
        *head = Value::int(count).ok_or(Error::OutOfMemory)?.0;
        *entries.get_mut(slot).ok_or(Error::BadSlot)? = Value::from_ref(r, hash_tag(hash)).0;
        Ok(())
    }
}

/// The kind byte of the intern table entry of a string of hash `hash`:
/// the hash's top byte, which no table size takes for the slot. A lookup
/// reads a string only where it matches (one entry in 256 of another
/// hash), so its walk costs a word a slot, wherever a guest has put the
/// strings (docs/spec.md §5).
#[must_use]
pub fn hash_tag(hash: u64) -> u8 {
    hash.to_be_bytes()[0]
}

/// The intern table entry a sort word stands for: its low half is the
/// entry's reference and the byte above it the entry's kind byte.
fn entry_of(word: u64) -> u64 {
    Value::from_ref(low_ref(word), word.to_le_bytes()[4]).0
}

/// The string of a word that holds a reference in its low half.
#[allow(
    clippy::cast_possible_truncation,
    reason = "the low half is the reference"
)]
fn low_ref(word: u64) -> Ref {
    Ref::from_raw(word as u32)
}

/// Lays out words sorted by bucket (the bits from 40 up) in a table of
/// `cap` slots: each at its bucket or right after the one before it,
/// the first no earlier than slot `lead`, around the table's end. How
/// many pass the end, and the most slots one lies past its bucket. The
/// entries are written to `into`, the table's slots, unless it is empty.
/// (One copy for its three passes: a reset is no hot path.)
#[inline(never)]
fn lay_out_sorted(words: &[u64], cap: usize, lead: usize, into: &mut [u64]) -> (usize, usize) {
    let (mut next, mut far) = (lead, 0);
    for &word in words {
        #[allow(clippy::cast_possible_truncation, reason = "22 bits")]
        let bucket = (word >> 40) as usize;
        let at = bucket.max(next);
        next = at + 1;
        far = far.max(at - bucket);
        if let Some(entry) = into.get_mut(at & (cap - 1)) {
            *entry = entry_of(word);
        }
    }
    (next.saturating_sub(cap), far)
}

/// The longest cluster a doubling keeps in order entry by entry as it
/// reads it; a longer one is sorted whole.
const SORT_IN_PLACE: usize = 16;

/// Sorts the `n` words of `e` from index `first` on, ascending; indexes
/// are taken under `wrap`, so a run may pass a table's end. In place and
/// O(n log n) for any input: a heapsort, past the few words of an
/// ordinary cluster, which an insertion sort orders faster.
fn sort_words(e: &mut [u64], first: usize, n: usize, wrap: usize) {
    let at = |i: usize| (first + i) & wrap;
    let word = |e: &[u64], i: usize| e.get(at(i)).copied().unwrap_or(0);
    if n <= 16 {
        for i in 1..n {
            let mut j = i;
            while j > 0 && word(e, j - 1) > word(e, j) {
                e.swap(at(j - 1), at(j));
                j -= 1;
            }
        }
        return;
    }
    let sift = |e: &mut [u64], mut root: usize, end: usize| {
        loop {
            let mut child = 2 * root + 1;
            if child >= end {
                break;
            }
            if child + 1 < end && word(e, child) < word(e, child + 1) {
                child += 1;
            }
            if word(e, root) >= word(e, child) {
                break;
            }
            e.swap(at(root), at(child));
            root = child;
        }
    };
    for i in (0..n / 2).rev() {
        sift(e, i, n);
    }
    for end in (1..n).rev() {
        e.swap(at(0), at(end));
        sift(e, 0, end);
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

    /// The table `prune_intern` must leave, slot for slot, over a copy of
    /// the entries: each cluster (a run of occupied slots, around the
    /// table's end if it lies there) that holds a dead entry has its live
    /// ones in bucket order, then kind byte and reference order, each at its
    /// bucket or
    /// right after the one before; every other slot is as it was.
    fn pruned_sorted(
        mut slots: Vec<u64>,
        hash: &dyn Fn(u64) -> u64,
        live: &dyn Fn(u64) -> bool,
    ) -> Vec<u64> {
        let object = |w: u64| Value(w).as_obj().is_some();
        let cap = slots.len();
        let mask = cap - 1;
        // From the slot after a free one, so no cluster is cut.
        let start = slots.iter().position(|&w| !object(w)).unwrap() + 1;
        let mut i = 0;
        while i < cap {
            let base = (start + i) & mask;
            let len = (0..cap)
                .position(|k| !object(slots[(base + k) & mask]))
                .unwrap();
            let cluster: Vec<u64> = (0..len).map(|k| slots[(base + k) & mask]).collect();
            i += len + 1;
            if cluster.iter().all(|&w| live(w)) {
                continue;
            }
            let mut kept: Vec<(usize, u64, u64)> = cluster
                .iter()
                .filter(|&&w| live(w))
                .map(|&w| {
                    (
                        (hash(w) as usize).wrapping_sub(base) & mask,
                        w & 0xff_ffff_ffff,
                        w,
                    )
                })
                .collect();
            kept.sort_unstable();
            for k in 0..len {
                slots[(base + k) & mask] = Value::UNDEFINED.0;
            }
            let mut next = 0;
            for (distance, _, w) in kept {
                let at = next.max(distance);
                slots[(base + at) & mask] = w;
                next = at + 1;
            }
        }
        slots
    }

    /// Over rounds of interning and collections that keep none, all, or a
    /// pseudo-random part of the strings, in tables of 64 to 1,024 slots
    /// with clusters that wrap past the last slot: the table is, slot for
    /// slot, the sorted layout of each cluster that lost a string, its count is the live
    /// strings', every live string is found as itself and no dead one is.
    #[test]
    fn a_prune_leaves_each_cluster_that_lost_a_string_sorted() {
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
                    (Value::from_ref(*r, hash_tag(hash)).0, hash)
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
            let want = pruned_sorted(before.clone(), &hash, &live);
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
