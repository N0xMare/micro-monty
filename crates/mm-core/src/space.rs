//! One arena space: a table of fixed-size segments that never move, with
//! side bitmaps, size-class free lists and a sweep. The word space and the
//! byte space are two instances (`Store` for `Vec<u64>` and `Vec<u8>`).
//!
//! Addresses are `u32` = `segment << 17 | unit`, a unit being 8 bytes (a word,
//! or a granule of the byte space). Segment 0 is never used, so address 0 is
//! never valid.
//!
//! A normal segment spans up to 2^17 units (1 MiB) of address space and
//! commits `cap` of them. A space's first normal segment starts at
//! `FIRST_UNITS` (32 KiB) and, as the bump segment, grows by reallocation,
//! doubling up to `SEG_UNITS`; later ones are committed whole. Growth moves
//! the backing store, never an address, so a small heap commits little and
//! no `Ref` changes. Its blocks tile `[0, top)`, `top <= cap`; every
//! block starts with a header unit, so the segment can be walked. Header bits
//! 0..16 are a type id (0 = free block) and bits 32..64 a length from which
//! `Store::units_for` derives the block size. Two side bitmaps, one bit per
//! unit, mark object starts (set on allocation, cleared on free, so a stale or
//! forged address is rejected instead of being read as a header) and GC marks.
//!
//! A block of `LARGE_UNITS` or more gets a dedicated segment of exactly its
//! size, so its data stays one contiguous slice (`&str` over a 100 MB string
//! needs that) and its memory is returned as soon as it dies.
//!
//! Free blocks of 2..=`SMALL_MAX` units sit on exact-size lists, bigger ones
//! on power-of-two bins searched first-fit; the link is unit 1 of the block.
//! A one-unit free block is a hole, reclaimed by coalescing at the next sweep.
//! A small block (up to `SMALL_MAX` units) is placed from its exact list,
//! the smallest bigger small block (split), the free run (a medium free
//! block carved front to back; when it is too short, the next listed one),
//! then by bumping. While there is no run no medium free block is listed:
//! listing one makes it the run (`Space::arm_run`). A medium block is bumped
//! first, then placed in a medium free block (its rest listed again), then
//! carved from the run; it takes its room (`medium_room`), the rest of which
//! is a hole, except when bumped in a segment that is not full-size. Then the bump segment grows, then a fresh segment comes. Small
//! blocks fill freed space before the frontier, so a long-lived small object
//! is not left after each block of a growing series (`rows = rows + [row]`):
//! there it split the space the series freed into blocks each just too
//! short for the next, and the heap committed a block per round until
//! `MemoryError` with under 2 MB live. Sweep rebuilds every list from
//! scratch, coalescing adjacent free blocks, makes the first medium one the
//! run and releases segments with nothing live.

use alloc::{boxed::Box, vec, vec::Vec};

use crate::heap::Error;

/// `Seg::dirty`: a value slot of an object was written since an
/// incremental mark last rescanned the segment; only value setters set it.
pub(crate) const DIRTY_REFS: u8 = 2;

pub(crate) const SEG_BITS: u32 = 17;
/// Units in a normal segment.
pub(crate) const SEG_UNITS: usize = 1 << SEG_BITS;
const OFF_MASK: u32 = (1 << SEG_BITS) - 1;
/// Segment indices fit the 15 high bits of an address.
pub(crate) const MAX_SEGS: usize = 1 << (32 - SEG_BITS);
/// Largest block size with an exact-size free list.
const SMALL_MAX: usize = 63;
/// Blocks of at least this many units get a dedicated segment (128 KiB).
pub(crate) const LARGE_UNITS: usize = SEG_UNITS / 8;
/// Medium bins: bin `b` holds free blocks of `2^(b+6) .. 2^(b+7)` units.
const MED_BINS: usize = 12;
/// Fewest units a normal segment has (32 KiB): the bytes space's first
/// segment ([`Store::FIRST`]).
pub(crate) const FIRST_UNITS: usize = 1 << 12;
/// Committed bytes of a full normal segment: data plus both bitmaps.
pub(crate) const SEG_COST: u64 = normal_cost(SEG_UNITS);

/// Committed bytes of a normal segment of `cap` units (a multiple of 64):
/// data plus both bitmaps.
pub(crate) const fn normal_cost(cap: usize) -> u64 {
    (cap as u64 + 2 * (cap as u64 / 64)) * 8
}
/// Bitmap bytes of a dedicated segment (one word each).
const LARGE_OVERHEAD: u64 = 16;
const NIL: u32 = 0;

/// Segment storage: units of 8 bytes, read and written as `u64` headers and
/// links. Storage holds at least the units below the segment's `top`;
/// nothing above `top` is read before it is written. Segments made while
/// running are zeroed at their capacity (fresh memory from the allocator
/// costs nothing to zero); a restored segment reserves its capacity and
/// initializes only up to `top`, so the image's units are written once and
/// only the gaps between them are zeroed.
pub(crate) trait Store {
    /// `units` zero units.
    fn zeroed(units: usize) -> Self;
    /// Empty storage with room for `units` units.
    fn reserve(units: usize) -> Self;
    fn empty() -> Self;
    /// Units the space's first normal segment starts with: what a fresh
    /// session's startup heap needs, so `Session::new` does not grow it.
    const FIRST: usize;
    /// Makes room for `units` units, keeping its contents (one
    /// reallocation). Nothing is initialized: the bump allocator fills in
    /// 4 KiB steps as it passes the initialized units, as on restore, so a
    /// growth neither zeroes nor later copies what is never allocated.
    fn grow(&mut self, units: usize);
    /// Zero-fills the storage up to `units` units if it holds fewer.
    fn fill_to(&mut self, units: usize);
    /// Units initialized.
    fn units(&self) -> usize;
    fn read(&self, unit: usize) -> Option<u64>;
    fn write(&mut self, unit: usize, w: u64);
    /// Units of a block whose header length field is `len`, header included.
    fn units_for(len: u32) -> usize;
    /// Header length field of a free block of `units` units.
    fn free_len(units: usize) -> u32;
    /// Appends units `off..off + n` to `out` as little-endian bytes.
    fn append_units(&self, off: usize, n: usize, out: &mut dyn FnMut(&[u8]));
    /// Copies little-endian bytes (whole units) in at unit `off`, zero-filling
    /// up to `off` first if the storage holds fewer units.
    fn load_units(&mut self, off: usize, bytes: &[u8]);
}

impl Store for Vec<u64> {
    // 64 KiB: a session's startup objects take about 32 KiB (4,019 units
    // after the prelude's comparison twins [M]); a 32 KiB start grew, and
    // copied, once in every `Session::new`.
    const FIRST: usize = FIRST_UNITS * 2;
    fn zeroed(units: usize) -> Self {
        vec![0; units]
    }
    fn reserve(units: usize) -> Self {
        Vec::with_capacity(units)
    }
    fn empty() -> Self {
        Vec::new()
    }
    fn grow(&mut self, units: usize) {
        self.reserve_exact(units.saturating_sub(self.len()));
    }
    fn fill_to(&mut self, units: usize) {
        if self.len() < units {
            self.resize(units, 0);
        }
    }
    #[inline]
    fn units(&self) -> usize {
        self.len()
    }
    #[inline]
    fn read(&self, unit: usize) -> Option<u64> {
        self.get(unit).copied()
    }
    #[inline]
    fn write(&mut self, unit: usize, w: u64) {
        if let Some(slot) = self.get_mut(unit) {
            *slot = w;
        }
    }
    #[inline]
    fn units_for(len: u32) -> usize {
        len as usize + 1
    }
    #[inline]
    fn free_len(units: usize) -> u32 {
        u32::try_from(units - 1).unwrap_or(0)
    }
    fn append_units(&self, off: usize, n: usize, out: &mut dyn FnMut(&[u8])) {
        if let Some(words) = self.get(off..off + n) {
            // A fixed-size endian-conversion buffer avoids one sink call per
            // word without restoring scratch proportional to the live heap.
            let mut bytes = [0u8; 1024];
            for chunk in words.chunks(bytes.len() / 8) {
                for (w, b) in chunk.iter().zip(bytes.as_chunks_mut::<8>().0) {
                    b.copy_from_slice(&w.to_le_bytes());
                }
                out(&bytes[..chunk.len() * 8]);
            }
        }
    }
    fn load_units(&mut self, off: usize, bytes: &[u8]) {
        let (chunks, _) = bytes.as_chunks::<8>();
        if self.len() <= off {
            self.fill_to(off);
            self.extend(chunks.iter().map(|c| u64::from_le_bytes(*c)));
            return;
        }
        self.fill_to(off + chunks.len());
        if let Some(words) = self.get_mut(off..off + chunks.len()) {
            for (w, c) in words.iter_mut().zip(chunks) {
                *w = u64::from_le_bytes(*c);
            }
        }
    }
}

impl Store for Vec<u8> {
    const FIRST: usize = FIRST_UNITS;
    fn zeroed(units: usize) -> Self {
        vec![0; units.saturating_mul(8)]
    }
    fn reserve(units: usize) -> Self {
        Vec::with_capacity(units.saturating_mul(8))
    }
    fn empty() -> Self {
        Vec::new()
    }
    fn grow(&mut self, units: usize) {
        self.reserve_exact(units.saturating_mul(8).saturating_sub(self.len()));
    }
    fn fill_to(&mut self, units: usize) {
        if self.len() < units * 8 {
            self.resize(units * 8, 0);
        }
    }
    #[inline]
    fn units(&self) -> usize {
        self.len() / 8
    }
    #[inline]
    fn read(&self, unit: usize) -> Option<u64> {
        let bytes = self.get(unit * 8..unit * 8 + 8)?;
        <[u8; 8]>::try_from(bytes).ok().map(u64::from_le_bytes)
    }
    #[inline]
    fn write(&mut self, unit: usize, w: u64) {
        if let Some(bytes) = self.get_mut(unit * 8..unit * 8 + 8) {
            bytes.copy_from_slice(&w.to_le_bytes());
        }
    }
    #[inline]
    fn units_for(len: u32) -> usize {
        1 + (len as usize).div_ceil(8)
    }
    #[inline]
    fn free_len(units: usize) -> u32 {
        u32::try_from((units - 1) * 8).unwrap_or(0)
    }
    fn append_units(&self, off: usize, n: usize, out: &mut dyn FnMut(&[u8])) {
        if let Some(b) = self.get(off * 8..(off + n) * 8) {
            out(b);
        }
    }
    fn load_units(&mut self, off: usize, bytes: &[u8]) {
        if self.len() <= off * 8 {
            self.fill_to(off);
            self.extend_from_slice(bytes);
            return;
        }
        self.fill_to((off * 8 + bytes.len()).div_ceil(8));
        if let Some(b) = self.get_mut(off * 8..off * 8 + bytes.len()) {
            b.copy_from_slice(bytes);
        }
    }
}

impl Space<Vec<u8>> {
    /// Appends `data` to the byte block at `a`, alone in its dedicated
    /// segment, after setting the segment's room to `reserve` bytes of
    /// data (at least what it then holds; less gives the rest back). The
    /// room is reserved, not filled, and counts against the limit: growth
    /// is admitted against `avail` first. A snapshot keeps only the block.
    pub(crate) fn append_large(
        &mut self,
        a: u32,
        data: &[u8],
        reserve: usize,
        avail: u64,
    ) -> Result<(), Error> {
        let (idx, off) = split(a);
        let s = self
            .segs
            .get_mut(idx)
            .filter(|s| s.present && s.large && off == 0)
            .ok_or(Error::BadRef)?;
        let len = header_len(s.data.read(0).ok_or(Error::BadRef)?) as usize;
        let end = len
            .checked_add(data.len())
            .and_then(|e| u32::try_from(e).ok())
            .ok_or(Error::OutOfMemory)?;
        let units = <Vec<u8> as Store>::units_for(end);
        let room = u32::try_from(reserve).unwrap_or(u32::MAX);
        let cap = <Vec<u8> as Store>::units_for(room).max(units);
        // Growing past the buffer's capacity may move it: the copy holds
        // the old buffer and the new one at once (dlmalloc on wasm, glibc
        // below its mmap threshold), so both must fit.
        let held = if cap * 8 > s.data.capacity() {
            s.data.capacity() as u64
        } else {
            0
        };
        let (old, new) = (s.cost(), cap as u64 * 8 + LARGE_OVERHEAD);
        if over_cap(cap) || (new + held > old && new + held - old > avail) {
            return Err(Error::OutOfMemory);
        }
        s.data.truncate(8 + len);
        if cap * 8 > s.data.capacity() {
            s.data.reserve_exact(cap * 8 - s.data.len());
        } else {
            s.data.shrink_to(cap * 8);
        }
        s.data.extend_from_slice(data);
        s.data.resize(units * 8, 0);
        s.data.write(0, 1 | u64::from(end) << 32);
        s.cap = u32::try_from(cap).unwrap_or(0);
        (s.top, s.used) = (units, units);
        self.committed = self.committed - old + new;
        Ok(())
    }
}

/// The room a medium block of `n` units is placed in: `n` rounded up to
/// one of 64 sizes per doubling (at most 1.6% more). A block that dies
/// leaves its room, so a slightly bigger block fits where it was: a
/// temporary beside a result a few units longer (`prefix + str(i)`,
/// `base[:] + [i]`) once left holes each just too short for the next
/// result, which needed twice the memory.
fn medium_room(n: usize) -> usize {
    let g = 1 << (n.ilog2() - 6);
    ((n + g - 1) & !(g - 1)).min(LARGE_UNITS - 1)
}

/// Is a block of `n` units, header included, over `MAX_OBJECT_BYTES`?
pub(crate) fn over_cap(n: usize) -> bool {
    (n as u64).saturating_sub(1) > crate::heap::MAX_OBJECT_BYTES / 8
}

/// Extends `b` to `len` zero-filled elements with one exact reallocation.
fn grow_slice<T: Copy + Default>(b: &mut Box<[T]>, len: usize) {
    let mut v = core::mem::take(b).into_vec();
    v.reserve_exact(len.saturating_sub(v.len()));
    v.resize(len, T::default());
    *b = v.into_boxed_slice();
}

/// Initializes a restored or grown segment's storage past its new `top`,
/// in 4 KiB steps.
#[cold]
#[inline(never)]
fn fill_restored<S: Store>(s: &mut Seg<S>) {
    s.data.fill_to(s.top.next_multiple_of(512).min(s.cap()));
}

/// Splits an address into segment index and unit offset.
#[inline]
pub(crate) fn split(a: u32) -> (usize, usize) {
    ((a >> SEG_BITS) as usize, (a & OFF_MASK) as usize)
}

#[inline]
#[allow(
    clippy::cast_possible_truncation,
    reason = "idx < MAX_SEGS and off < SEG_UNITS, so the address fits 32 bits"
)]
pub(crate) fn addr_of(idx: usize, off: usize) -> u32 {
    ((idx << SEG_BITS) | off) as u32
}

/// The length field of a header.
#[inline]
pub(crate) fn header_len(h: u64) -> u32 {
    (h >> 32) as u32
}

#[inline]
pub(crate) fn bit(map: &[u64], off: usize) -> bool {
    map.get(off >> 6).is_some_and(|w| w >> (off & 63) & 1 != 0)
}

#[inline]
pub(crate) fn set_bit(map: &mut [u64], off: usize) {
    if let Some(w) = map.get_mut(off >> 6) {
        *w |= 1 << (off & 63);
    }
}

#[inline]
fn clear_bit(map: &mut [u64], off: usize) {
    if let Some(w) = map.get_mut(off >> 6) {
        *w &= !(1 << (off & 63));
    }
}

/// One segment and its metadata. A vacant entry has empty storage, so every
/// access to it fails its bounds check.
pub(crate) struct Seg<S> {
    pub(crate) data: S,
    /// Object-start bitmap.
    pub(crate) starts: Box<[u64]>,
    /// Normal: end of the tiled blocks. Dedicated: block size.
    pub(crate) top: usize,
    /// Units in allocated blocks.
    pub(crate) used: usize,
    pub(crate) present: bool,
    pub(crate) large: bool,
    /// The write barrier's record: [`DIRTY_REFS`] once a value slot was
    /// written since the collector last rescanned the segment.
    pub(crate) dirty: u8,
    /// Units reserved; `data` holds at least `top` of them. A `u32` (and
    /// the cost derived from it) keeps `Seg` at 64 bytes, so indexing the
    /// segment table is a shift on every heap access.
    cap: u32,
}

impl<S: Store> Seg<S> {
    /// Committed units. A normal segment's bitmaps have a bit for each; it
    /// is a power of two from `FIRST_UNITS` to `SEG_UNITS`.
    #[inline]
    pub(crate) fn cap(&self) -> usize {
        self.cap as usize
    }

    /// Bytes counted against the memory limit.
    fn cost(&self) -> u64 {
        match (self.present, self.large) {
            (false, _) => 0,
            (true, false) => normal_cost(self.cap()),
            (true, true) => u64::from(self.cap) * 8 + LARGE_OVERHEAD,
        }
    }

    pub(crate) fn vacant() -> Self {
        Seg {
            data: S::empty(),
            starts: Box::new([]),
            top: 0,
            used: 0,
            present: false,
            large: false,
            dirty: 0,
            cap: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Sweep {
    pub(crate) live_units: u64,
    pub(crate) freed_units: u64,
}

/// Free-list heads.
struct Lists {
    small: [u32; SMALL_MAX + 1],
    /// Bit `n` set iff `small[n]` is non-empty.
    mask: u64,
    medium: [u32; MED_BINS],
    /// Units on the lists (holes excluded).
    free_units: u64,
}

fn med_bin(units: usize) -> usize {
    (units.ilog2() as usize).saturating_sub(6).min(MED_BINS - 1)
}

impl Lists {
    fn new() -> Self {
        Lists {
            small: [NIL; SMALL_MAX + 1],
            mask: 0,
            medium: [NIL; MED_BINS],
            free_units: 0,
        }
    }

    /// Turns `[off, off + units)` of segment `idx` into a free block and lists
    /// it. Free-block contents are not snapshot-visible, so this does not set
    /// the dirty flag.
    fn file<S: Store>(&mut self, s: &mut Seg<S>, idx: usize, off: usize, units: usize) {
        s.data.write(off, u64::from(S::free_len(units)) << 32);
        if units < 2 {
            return;
        }
        let a = addr_of(idx, off);
        let head = if units <= SMALL_MAX {
            self.mask |= 1 << units;
            &mut self.small[units]
        } else {
            &mut self.medium[med_bin(units)]
        };
        s.data.write(off + 1, u64::from(*head));
        *head = a;
        self.free_units += units as u64;
    }

    fn pop_small<S: Store>(&mut self, segs: &[Seg<S>], n: usize) -> Option<u32> {
        let a = *self.small.get(n)?;
        if a == NIL {
            return None;
        }
        let (i, o) = split(a);
        let next = link(segs.get(i)?.data.read(o + 1)?);
        self.small[n] = next;
        if next == NIL {
            self.mask &= !(1 << n);
        }
        self.free_units -= n as u64;
        Some(a)
    }

    /// Takes the smallest listed small block bigger than `n < SMALL_MAX`
    /// units; returns its address and size.
    fn take_small_above<S: Store>(&mut self, segs: &[Seg<S>], n: usize) -> Option<(u32, usize)> {
        let above = self.mask & (u64::MAX << (n + 1));
        if above == 0 {
            return None;
        }
        let class = above.trailing_zeros() as usize;
        Some((self.pop_small(segs, class)?, class))
    }

    /// Takes a medium free block of at least `n` units off the lists;
    /// returns its address and size. The caller uses or lists the rest.
    fn take_fit<S: Store>(&mut self, segs: &mut [Seg<S>], n: usize) -> Option<(u32, usize)> {
        let first = if n < 64 { 0 } else { med_bin(n) };
        for b in first..MED_BINS {
            let mut prev = NIL;
            let mut cur = self.medium[b];
            while cur != NIL {
                let (seg, off) = split(cur);
                let block_seg = segs.get(seg)?;
                let m = S::units_for(header_len(block_seg.data.read(off)?));
                let next = link(block_seg.data.read(off + 1)?);
                if m >= n {
                    if prev == NIL {
                        self.medium[b] = next;
                    } else {
                        let (prev_seg, prev_off) = split(prev);
                        segs.get_mut(prev_seg)?
                            .data
                            .write(prev_off + 1, u64::from(next));
                    }
                    self.free_units -= m as u64;
                    return Some((cur, m));
                }
                prev = cur;
                cur = next;
            }
        }
        None
    }
}

#[inline]
fn link(w: u64) -> u32 {
    u32::try_from(w).unwrap_or(NIL)
}

/// A segmented space.
pub(crate) struct Space<S> {
    pub(crate) segs: Vec<Seg<S>>,
    /// GC mark bitmaps, parallel to `segs` (kept apart so marking can write
    /// them while reading segment data).
    pub(crate) marks: Vec<Box<[u64]>>,
    pub(crate) vacant: Vec<usize>,
    /// Segment being bump-allocated into (0: none).
    pub(crate) bump: usize,
    /// A free run being allocated from front to back, `(segment, next
    /// unit, end)`; segment 0: none. Its unallocated part always carries a
    /// free-block header, so the segment stays walkable.
    run: (usize, usize, usize),
    lists: Lists,
    /// Bytes of present segments, bitmaps included.
    pub(crate) committed: u64,
    /// Lazy sweep: partly live segments whose dead blocks the last
    /// collection left for allocation (or a safepoint step) to sweep; their
    /// marks are kept until then.
    pub(crate) pending: Vec<usize>,
    /// Units the lazy sweeps freed since the heap last took the count.
    pub(crate) lazily_freed: u64,
}

impl<S: Store> Space<S> {
    pub(crate) fn new() -> Self {
        Space {
            segs: Vec::new(),
            marks: Vec::new(),
            vacant: Vec::new(),
            bump: 0,
            run: (0, 0, 0),
            lists: Lists::new(),
            committed: 0,
            pending: Vec::new(),
            lazily_freed: 0,
        }
    }

    pub(crate) fn metadata_bytes(&self) -> u64 {
        (self.segs.capacity() * size_of::<Seg<S>>()
            + self.marks.capacity() * size_of::<Box<[u64]>>()
            + self.vacant.capacity() * size_of::<usize>()) as u64
    }

    /// Reserve all tables together, including every index GC could release.
    pub(crate) fn reserve_tables(&mut self, len: usize, avail: u64) -> Result<(), Error> {
        let capacity = len.checked_next_power_of_two().ok_or(Error::OutOfMemory)?;
        let extra = (capacity.saturating_sub(self.segs.capacity()) * size_of::<Seg<S>>()
            + capacity.saturating_sub(self.marks.capacity()) * size_of::<Box<[u64]>>()
            + capacity.saturating_sub(self.vacant.capacity()) * size_of::<usize>())
            as u64;
        if extra > avail {
            return Err(Error::OutOfMemory);
        }
        self.segs
            .try_reserve_exact(capacity - self.segs.len())
            .and_then(|()| self.marks.try_reserve_exact(capacity - self.marks.len()))
            .and_then(|()| self.vacant.try_reserve_exact(capacity - self.vacant.len()))
            .and_then(|()| {
                self.pending
                    .try_reserve_exact(capacity - self.pending.len())
            })
            .map_err(|_| Error::OutOfMemory)
    }

    /// Places a block of `n >= 1` units in memory already committed (a free
    /// list, the free run or the bump segment), or `None` if it needs a new
    /// segment ([`Space::place_slow`]), in the order the module docs give.
    /// Needs no budget, so the caller computes the budget only on the slow
    /// path. Only `start` writes the header. The common cases (exact list,
    /// run, bump) are inline; taking a bigger listed block is out of line.
    #[allow(
        clippy::inline_always,
        reason = "the lazy sweep's slow path otherwise un-inlines it: str +1.4% Ir [M]"
    )]
    #[inline(always)]
    pub(crate) fn place_fast(&mut self, n: usize) -> Option<u32> {
        if n <= SMALL_MAX {
            if let Some(a) = self.lists.pop_small(&self.segs, n) {
                return Some(a);
            }
            // Best fit among small blocks before carving the run.
            if n < SMALL_MAX
                && self.lists.mask & (u64::MAX << (n + 1)) != 0
                && let Some(a) = self.split_small(n)
            {
                return Some(a);
            }
            if let Some(a) = self.carve_run(n) {
                return Some(a);
            }
            // Without a run no medium block is listed (`arm_run`), so only
            // a run too short for `n` can be replaced from freed medium space.
            if self.run.0 == 0 {
                return self.bump(n);
            }
        }
        self.place_rest(n)
    }

    /// Places what `place_fast` does not inline: a medium block
    /// ([`Space::place_medium`]), or a small block the run is too short
    /// for ([`Space::next_run`]), bumped if no medium free block is listed.
    /// One cold call keeps the inlined small path's registers: with two
    /// calls there wasm32 str rose 0.7%, without `cold` native str 0.2% [M].
    #[cold]
    #[inline(never)]
    fn place_rest(&mut self, n: usize) -> Option<u32> {
        if n > SMALL_MAX {
            return self.place_medium(n);
        }
        self.next_run(n).or_else(|| self.bump(n))
    }

    /// The smallest listed small block bigger than `n` units, its rest
    /// listed again.
    #[inline(never)]
    fn split_small(&mut self, n: usize) -> Option<u32> {
        let (a, m) = self.lists.take_small_above(&self.segs, n)?;
        let (seg, off) = split(a);
        if let Some(s) = self.segs.get_mut(seg) {
            self.lists.file(s, seg, off + n, m - n);
        }
        Some(a)
    }

    /// Replaces a run too short for a small block of `n` units: its rest
    /// goes back on the lists and a medium free block, if one is listed,
    /// becomes the run.
    #[cold]
    #[inline(never)]
    fn next_run(&mut self, n: usize) -> Option<u32> {
        self.retire_run();
        self.arm_run();
        self.carve_run(n)
    }

    /// Keeps the invariant that, while there is no run, no medium free block
    /// is listed: the first one listed becomes the run. Called wherever
    /// medium blocks are listed (sweeps, a retired bump tail, a medium
    /// block's rest), so small blocks fill freed space before the frontier
    /// and the hot path tests only the run. Its choice follows from the
    /// lists alone, which a restore's sweep rebuilds.
    fn arm_run(&mut self) {
        if self.run.0 != 0 {
            return;
        }
        if let Some((a, m)) = self.lists.take_fit(&mut self.segs, 1) {
            let (seg, off) = split(a);
            self.run = (seg, off, off + m);
        }
    }

    /// Places a medium block of `n` units: bump, then a medium free block
    /// whose rest is listed again, then the run. Out of line: medium blocks
    /// are rarer and their callers fill them, and inlined it slowed the
    /// small path.
    #[inline(never)]
    fn place_medium(&mut self, n: usize) -> Option<u32> {
        if n >= LARGE_UNITS {
            return None;
        }
        if let Some(a) = self.bump_medium(n) {
            return Some(a);
        }
        let Some((a, m)) = self.lists.take_fit(&mut self.segs, n) else {
            // The run is freed medium space too, held for small blocks.
            return self.carve_run(n);
        };
        let room = medium_room(n).min(m);
        self.pad(a, n, room);
        let (seg, off) = split(a);
        if m > room
            && let Some(s) = self.segs.get_mut(seg)
        {
            self.lists.file(s, seg, off + room, m - room);
            self.arm_run();
        }
        Some(a)
    }

    /// Bumps a medium block of `n` units with its room (`medium_room`), or
    /// without it if only the block fits or the bump segment is not
    /// full-size. A small segment is filled 4 KiB at a time as the bump
    /// reaches it (`fill_restored`), and the series that need rooms outgrow
    /// it, so a room there would only fill more: at startup one more 4 KiB
    /// fill, +0.58% Ir [M]. The test reads `cap`, which the image holds.
    fn bump_medium(&mut self, n: usize) -> Option<u32> {
        let room = medium_room(n);
        if self
            .segs
            .get(self.bump)
            .is_some_and(|s| s.cap() == SEG_UNITS)
            && let Some(a) = self.bump(room)
        {
            self.pad(a, n, room);
            return Some(a);
        }
        self.bump(n)
    }

    /// Makes units `n..room` after the block placed at `a` a hole: free,
    /// not listed, so the block's room stays free while it lives and is
    /// reused with it once it dies (sweep coalesces them).
    fn pad(&mut self, a: u32, n: usize, room: usize) {
        let (seg, off) = split(a);
        if room > n
            && let Some(s) = self.segs.get_mut(seg)
        {
            s.data
                .write(off + n, u64::from(S::free_len(room - n)) << 32);
        }
    }

    /// Places a block of `n` units that `place_fast` could not: in a new
    /// dedicated segment, the grown bump segment or a fresh bump segment.
    /// `avail` is how many more bytes may be committed.
    #[cold]
    pub(crate) fn place_slow(&mut self, n: usize, avail: u64) -> Result<u32, Error> {
        if n >= LARGE_UNITS {
            return self.new_large(n, avail);
        }
        if !self.pending.is_empty()
            && let Some(a) = self.place_pending(n, false)
        {
            return Ok(a);
        }
        let fresh = if let Some(cap) = self.grow_target(n) {
            self.grow_bump(cap, avail)
        } else {
            self.new_bump(n, avail)
        };
        if fresh.is_err()
            && let Some(a) = self.place_pending(n, true)
        {
            return Ok(a);
        }
        fresh?;
        if n > SMALL_MAX {
            self.bump_medium(n)
        } else {
            self.bump(n)
        }
        .ok_or(Error::OutOfMemory)
    }

    /// The capacity the bump segment grows to so that `n` more units fit,
    /// if it is not full-size and they fit `SEG_UNITS`: at least double.
    fn grow_target(&self, n: usize) -> Option<usize> {
        let s = self.segs.get(self.bump).filter(|s| s.present)?;
        let need = s.top + n;
        (s.cap() < SEG_UNITS && need <= SEG_UNITS)
            .then(|| need.next_power_of_two().max(s.cap() * 2).min(SEG_UNITS))
    }

    /// Capacity of a space's first normal segment, sized to fit `n` units.
    fn first_cap(n: usize) -> usize {
        n.next_power_of_two().clamp(S::FIRST, SEG_UNITS)
    }

    /// Bytes to commit before `n` units can be bump-allocated in fresh
    /// memory: growing the bump segment, a first small segment or a full one.
    pub(crate) fn fresh_cost(&self, n: usize) -> u64 {
        if let Some(cap) = self.grow_target(n) {
            let s = self.segs.get(self.bump).map_or(0, Seg::cost);
            normal_cost(cap) - s
        } else if self.bump == 0 {
            normal_cost(Self::first_cap(n))
        } else {
            SEG_COST
        }
    }

    /// Grows the bump segment to `cap` units (one copy of its data and
    /// bitmaps); addresses are unchanged.
    fn grow_bump(&mut self, cap: usize, avail: u64) -> Result<(), Error> {
        let (Some(s), Some(m)) = (self.segs.get_mut(self.bump), self.marks.get_mut(self.bump))
        else {
            return Err(Error::OutOfMemory);
        };
        let (cost, old) = (normal_cost(cap), s.cost());
        if cost - old > avail {
            return Err(Error::OutOfMemory);
        }
        s.data.grow(cap);
        s.cap = u32::try_from(cap).unwrap_or(0);
        grow_slice(&mut s.starts, cap / 64);
        grow_slice(m, cap / 64);
        self.committed += cost - old;
        // The capacity is snapshot-visible.
        Ok(())
    }

    /// Makes the `n` units placed at `a` an allocated block with header
    /// `header`. Only the header is written. Returns the segment's storage
    /// and the unit offset so the caller can fill the block without looking
    /// it up again.
    #[inline]
    pub(crate) fn start(
        &mut self,
        a: u32,
        n: usize,
        header: u64,
    ) -> Result<(&mut S, usize), Error> {
        let (idx, off) = split(a);
        let s = self.segs.get_mut(idx).ok_or(Error::OutOfMemory)?;
        set_bit(&mut s.starts, off);
        s.data.write(off, header);
        s.used += n;
        Ok((&mut s.data, off))
    }

    /// The next `n` units of the run, if it has them.
    fn carve_run(&mut self, n: usize) -> Option<u32> {
        let (seg, cur, end) = self.run;
        if seg == 0 || cur + n > end {
            return None;
        }
        self.run.1 = cur + n;
        self.mark_run_rest();
        Some(addr_of(seg, cur))
    }

    /// Writes the free-block header of the run's unallocated part.
    fn mark_run_rest(&mut self) {
        let (seg, cur, end) = self.run;
        if cur < end
            && let Some(s) = self.segs.get_mut(seg)
        {
            s.data.write(cur, u64::from(S::free_len(end - cur)) << 32);
        }
    }

    /// Lists the run's unallocated part and drops the run.
    fn retire_run(&mut self) {
        let (seg, cur, end) = core::mem::take(&mut self.run);
        if seg != 0
            && cur < end
            && let Some(s) = self.segs.get_mut(seg)
        {
            self.lists.file(s, seg, cur, end - cur);
        }
    }

    fn bump(&mut self, n: usize) -> Option<u32> {
        let s = self.segs.get_mut(self.bump)?;
        if !s.present || s.top + n > s.cap() {
            return None;
        }
        let a = addr_of(self.bump, s.top);
        s.top += n;
        if s.data.units() < s.top {
            fill_restored(s);
        }
        Some(a)
    }

    fn take_index(&mut self, avail: u64) -> Result<usize, Error> {
        if let Some(i) = self.vacant.pop() {
            return Ok(i);
        }
        if self.segs.len() >= MAX_SEGS {
            return Err(Error::OutOfMemory);
        }
        self.reserve_tables(self.segs.len().max(1) + 1, avail)?;
        if self.segs.is_empty() {
            // Index zero is reserved, but an unused space needs no allocation.
            self.segs.push(Seg::vacant());
            self.marks.push(Box::new([]));
        }
        self.segs.push(Seg::vacant());
        self.marks.push(Box::new([]));
        Ok(self.segs.len() - 1)
    }

    /// Retires the bump segment (its tail becomes a free block) and starts a
    /// fresh one that fits `n` units: small if it is the space's first.
    fn new_bump(&mut self, n: usize, avail: u64) -> Result<(), Error> {
        let cap = if self.bump == 0 {
            Self::first_cap(n)
        } else {
            SEG_UNITS
        };
        let cost = normal_cost(cap);
        if cost > avail {
            return Err(Error::OutOfMemory);
        }
        let idx = self.take_index(avail - cost)?;
        if let Some(s) = self.segs.get_mut(self.bump)
            && s.present
            && s.top < s.cap()
        {
            let (top, end) = (s.top, s.cap());
            s.data.fill_to(end);
            self.lists.file(s, self.bump, top, end - top);
            s.top = end;
            self.arm_run();
        }
        // A small first segment is initialized as the bump allocator
        // reaches it (`fill_restored`); a full one comes zeroed from the
        // system allocator, which maps fresh pages.
        let data = if cap < SEG_UNITS {
            S::reserve(cap)
        } else {
            S::zeroed(cap)
        };
        self.install(idx, data, cap, cap / 64, 0, false);
        self.bump = idx;
        Ok(())
    }

    fn new_large(&mut self, n: usize, avail: u64) -> Result<u32, Error> {
        // Every block over `MAX_OBJECT_BYTES` is large, so this and
        // `append_large` are the places the cap is checked (the header is
        // the one unit more).
        if over_cap(n) {
            return Err(Error::OutOfMemory);
        }
        let cost = (n as u64)
            .checked_mul(8)
            .and_then(|b| b.checked_add(LARGE_OVERHEAD))
            .ok_or(Error::OutOfMemory)?;
        if cost > avail {
            return Err(Error::OutOfMemory);
        }
        let idx = self.take_index(avail - cost)?;
        self.install(idx, S::zeroed(n), n, 1, n, true);
        Ok(addr_of(idx, 0))
    }

    fn install(&mut self, idx: usize, data: S, cap: usize, bitmap: usize, top: usize, large: bool) {
        if let (Some(s), Some(m)) = (self.segs.get_mut(idx), self.marks.get_mut(idx)) {
            *s = Seg {
                data,
                starts: vec![0; bitmap].into_boxed_slice(),
                top,
                used: 0,
                present: true,
                large,
                dirty: 0,
                cap: u32::try_from(cap).unwrap_or(0),
            };
            *m = vec![0; bitmap].into_boxed_slice();
            self.committed += s.cost();
        }
    }

    /// Installs an empty normal segment of `cap` units at `idx` (restoring a
    /// snapshot) if its cost fits `avail`, which it reduces.
    pub(crate) fn install_normal(
        &mut self,
        idx: usize,
        top: usize,
        cap: usize,
        avail: &mut u64,
    ) -> bool {
        let cost = normal_cost(cap);
        if cost > *avail {
            return false;
        }
        *avail -= cost;
        // The snapshot's units are loaded into the reserved storage; the
        // reader zero-fills only the gaps up to `top`.
        self.install(idx, S::reserve(cap), cap, cap / 64, top, false);
        true
    }

    /// Installs a dedicated segment of `n` units with room for `cap` at
    /// `idx` (restoring a snapshot) if its cost fits `avail`, which it
    /// reduces. The room is reserved, not filled.
    pub(crate) fn install_large(
        &mut self,
        idx: usize,
        n: usize,
        cap: usize,
        avail: &mut u64,
    ) -> bool {
        let Some(cost) = (cap as u64)
            .checked_mul(8)
            .and_then(|b| b.checked_add(LARGE_OVERHEAD))
            .filter(|&c| c <= *avail)
        else {
            return false;
        };
        *avail -= cost;
        self.install(idx, S::reserve(cap), cap, 1, n, true);
        true
    }

    fn release(&mut self, idx: usize) {
        if let (Some(s), Some(m)) = (self.segs.get_mut(idx), self.marks.get_mut(idx)) {
            self.committed -= s.cost();
            *s = Seg::vacant();
            // A released segment is a change a snapshot delta must record.
            *m = Box::new([]);
            self.vacant.push(idx);
        }
    }

    /// The segment and unit offset of the block starting at `a`, if one does.
    #[inline]
    pub(crate) fn block(&self, a: u32) -> Option<(&Seg<S>, usize)> {
        let (i, o) = split(a);
        let s = self.segs.get(i)?;
        bit(&s.starts, o).then_some((s, o))
    }

    /// Mutable form of `block`.
    #[inline]
    pub(crate) fn block_mut(&mut self, a: u32) -> Option<(&mut Seg<S>, usize)> {
        let (i, o) = split(a);
        let s = self.segs.get_mut(i)?;
        bit(&s.starts, o).then_some((s, o))
    }

    /// Bytes reusable without committing a segment: listed free blocks plus
    /// the bump segment's tail.
    pub(crate) fn free_bytes(&self) -> u64 {
        let tail = self
            .segs
            .get(self.bump)
            .filter(|s| s.present)
            .map_or(0, |s| s.cap() - s.top);
        let run = self.run.2 - self.run.1;
        (self.lists.free_units + (tail + run) as u64) * 8
    }

    /// Frees every allocated block whose mark bit is clear, coalesces free
    /// space, rebuilds the free lists, releases empty segments and clears all
    /// marks.
    pub(crate) fn sweep(&mut self, lazy: bool) -> Sweep {
        // The run's rest has a free-block header; the walk below lists it.
        self.run = (0, 0, 0);
        self.lists = Lists::new();
        self.pending.clear();
        // A small space sweeps at once: its sweep is short, and listing all
        // of its free blocks keeps allocation as it was.
        let lazy = lazy && self.committed > 16 << 20;
        let mut out = Sweep::default();
        for idx in 1..self.segs.len() {
            let (Some(s), Some(m)) = (self.segs.get_mut(idx), self.marks.get_mut(idx)) else {
                continue;
            };
            if !s.present {
                continue;
            }
            let any_live = s.starts.iter().zip(m.iter()).any(|(a, b)| a & b != 0);
            if any_live {
                // A normal segment tiled by allocated blocks only (no free
                // block, no hole: `used == top`) with every block marked has
                // nothing to free or list; skip the header walk.
                let all_live =
                    s.used == s.top && s.starts.iter().zip(m.iter()).all(|(a, b)| a & !b == 0);
                if !s.large && !all_live {
                    if lazy && idx != self.bump {
                        // Counted live until swept (`lazily_freed`).
                        out.live_units += s.used as u64;
                        self.pending.push(idx);
                        continue;
                    }
                    s.used = walk(&mut self.lists, s, m, idx, idx == self.bump, &mut out);
                }
                out.live_units += s.used as u64;
                m.fill(0);
                continue;
            }
            out.freed_units += s.used as u64;
            if idx == self.bump {
                s.top = 0;
                s.used = 0;
                s.starts.fill(0);
                m.fill(0);
            } else {
                self.release(idx);
            }
        }
        self.arm_run();
        out
    }

    /// Lazy sweep: reuses what the last collection found dead before more
    /// memory is committed, one segment at a time.
    #[cold]
    #[inline(never)]
    fn place_pending(&mut self, n: usize, all: bool) -> Option<u32> {
        // A few segments per allocation, so that one allocation does not
        // sweep a densely live heap; all of them before memory runs out.
        let mut left = if all { usize::MAX } else { 4 };
        while left > 0
            && let Some(idx) = self.pending.pop()
        {
            self.sweep_pending(idx);
            if let Some(a) = self.place_fast(n) {
                return Some(a);
            }
            left -= 1;
        }
        None
    }

    /// Sweeps up to `k` pending segments (lazy sweep, at a safepoint).
    pub(crate) fn sweep_some(&mut self, k: usize) {
        for _ in 0..k {
            let Some(idx) = self.pending.pop() else {
                break;
            };
            self.sweep_pending(idx);
        }
    }

    /// Sweeps pending segment `idx` (lazy sweep) and clears its marks.
    fn sweep_pending(&mut self, idx: usize) {
        let (Some(s), Some(m)) = (self.segs.get_mut(idx), self.marks.get_mut(idx)) else {
            return;
        };
        let mut out = Sweep::default();
        s.used = walk(&mut self.lists, s, m, idx, false, &mut out);
        m.fill(0);
        self.lazily_freed += out.freed_units;
        self.arm_run();
    }

    /// Sweeps every pending segment.
    pub(crate) fn finish_sweep(&mut self) {
        while let Some(idx) = self.pending.pop() {
            self.sweep_pending(idx);
        }
    }

    /// Before a collection marks: pending segments keep their dead blocks
    /// (the new marks find them dead again), not their old marks.
    pub(crate) fn clear_pending_marks(&mut self) {
        for &idx in &self.pending {
            if let Some(m) = self.marks.get_mut(idx) {
                m.fill(0);
            }
        }
    }
}

/// Sweeps one normal segment: frees unmarked objects, coalesces free runs and
/// lists them (or gives the bump segment's last run back to its bump region).
/// Returns the live units.
///
/// Only live objects' headers are read: the blocks tile the segment, so
/// everything between the end of one live object and the start of the next
/// (dead objects and old free blocks alike) is one free run, and a dead
/// object's start bit is cleared word by word from the bitmaps.
fn walk<S: Store>(
    lists: &mut Lists,
    s: &mut Seg<S>,
    marks: &[u64],
    idx: usize,
    is_bump: bool,
    out: &mut Sweep,
) -> usize {
    let top = s.top;
    let used = s.used;
    // `[free, next live object)` is free space.
    let mut free = 0;
    let mut live = 0;
    'words: for w in 0..top.div_ceil(64) {
        let (Some(st), Some(&mk)) = (s.starts.get(w).copied(), marks.get(w)) else {
            break;
        };
        if st & !mk != 0
            && let Some(x) = s.starts.get_mut(w)
        {
            *x = st & mk;
        }
        let mut bits = st & mk;
        while bits != 0 {
            let off = w * 64 + bits.trailing_zeros() as usize;
            bits &= bits - 1;
            let units = s.data.read(off).map_or(0, |h| S::units_for(header_len(h)));
            if off < free || units == 0 || off + units > top {
                // Unreachable while headers are only written by mm-core;
                // treat a malformed tail as free rather than trusting it.
                for o in off..top {
                    clear_bit(&mut s.starts, o);
                }
                break 'words;
            }
            if off > free {
                lists.file(s, idx, free, off - free);
            }
            live += units;
            free = off + units;
        }
    }
    out.freed_units += used.saturating_sub(live) as u64;
    if free < top {
        if is_bump {
            s.top = free;
        } else {
            lists.file(s, idx, free, top - free);
        }
    }
    live
}
