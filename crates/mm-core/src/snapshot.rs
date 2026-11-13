//! The FULL snapshot form of a heap (docs/spec.md §9.3), and the byte reader,
//! writer and checksum the engine's snapshot code shares.
//!
//! A heap is written right after a collection, so every block with a start
//! bit is live. Per space: the segment table's size, the bump segment, the
//! vacant indices, then each present segment (a normal one with its
//! committed capacity, which a restore commits again) with its live runs (maximal
//! sequences of adjacent live blocks) as raw data; free space is omitted.
//!
//! Reading validates everything before it is used: indices against the
//! table, runs against the segment, every block header inside a run, the
//! memory limit before a segment is allocated and data lengths against the
//! remaining input. It then rebuilds the start bitmaps from the block headers
//! and the free lists, bump state and accounting with the sweep a collection
//! ends with, so the result is the heap the collection left. `str` objects
//! and the intern table are checked too: everything mm-core itself trusts
//! about them (valid UTF-8, the char count, the ASCII flag, the table's
//! shape) holds after a restore.

use crate::heap::{Error, GcStats, Header, Heap, HeapConfig, Ref, flags, types};
use crate::space::{FIRST_UNITS, LARGE_UNITS, SEG_UNITS, Space, Store, header_len};
use crate::value::Value;
use alloc::{boxed::Box, vec::Vec};

/// Appends `v`.
#[inline(never)]
#[cfg_attr(mm_shipped, optimize(size))]
pub fn put_u8(out: &mut dyn FnMut(&[u8]), v: u8) {
    out(&[v]);
}

/// Appends `v`, little-endian.
#[inline(never)]
#[cfg_attr(mm_shipped, optimize(size))]
pub fn put_u32(out: &mut dyn FnMut(&[u8]), v: u32) {
    out(&v.to_le_bytes());
}

/// Appends `v`, little-endian.
#[inline(never)]
#[cfg_attr(mm_shipped, optimize(size))]
pub fn put_u64(out: &mut dyn FnMut(&[u8]), v: u64) {
    out(&v.to_le_bytes());
}

/// Appends a length (as `u32`; longer is cut, which the reader rejects).
#[cfg_attr(mm_shipped, optimize(size))]
pub fn put_len(out: &mut dyn FnMut(&[u8]), n: usize) {
    put_u32(out, u32::try_from(n).unwrap_or(u32::MAX));
}

/// Appends length-prefixed bytes.
#[cfg_attr(mm_shipped, optimize(size))]
pub fn put_bytes(out: &mut dyn FnMut(&[u8]), b: &[u8]) {
    put_len(out, b.len());
    out(b);
}

/// Reads what the `put_*` functions wrote. Every read checks the remaining
/// input; `None` means truncated or malformed.
pub struct Reader<'a> {
    rest: &'a [u8],
}

impl<'a> Reader<'a> {
    /// A reader over `input`.
    #[must_use]
    #[cfg_attr(mm_shipped, optimize(size))]
    pub fn new(input: &'a [u8]) -> Reader<'a> {
        Reader { rest: input }
    }

    /// Bytes not read yet.
    #[must_use]
    #[cfg_attr(mm_shipped, optimize(size))]
    pub fn remaining(&self) -> usize {
        self.rest.len()
    }

    /// The unread input, for decoders that advance it themselves.
    #[cfg_attr(mm_shipped, optimize(size))]
    pub fn rest_mut(&mut self) -> &mut &'a [u8] {
        &mut self.rest
    }

    /// The next `n` bytes.
    #[cfg_attr(mm_shipped, optimize(size))]
    pub fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let (head, rest) = self.rest.split_at_checked(n)?;
        self.rest = rest;
        Some(head)
    }

    /// A byte.
    #[cfg_attr(mm_shipped, optimize(size))]
    pub fn u8(&mut self) -> Option<u8> {
        self.take(1).and_then(|b| b.first().copied())
    }

    /// A `0`/`1` byte.
    #[cfg_attr(mm_shipped, optimize(size))]
    pub fn bool(&mut self) -> Option<bool> {
        match self.u8()? {
            0 => Some(false),
            1 => Some(true),
            _ => None,
        }
    }

    /// A little-endian `u32`.
    #[cfg_attr(mm_shipped, optimize(size))]
    pub fn u32(&mut self) -> Option<u32> {
        self.take(4)
            .and_then(|b| <[u8; 4]>::try_from(b).ok())
            .map(u32::from_le_bytes)
    }

    /// A little-endian `u64`.
    #[cfg_attr(mm_shipped, optimize(size))]
    pub fn u64(&mut self) -> Option<u64> {
        self.take(8)
            .and_then(|b| <[u8; 8]>::try_from(b).ok())
            .map(u64::from_le_bytes)
    }

    /// A value: its bits must be a valid encoding.
    #[cfg_attr(mm_shipped, optimize(size))]
    pub fn value(&mut self) -> Option<Value> {
        Value::from_bits(self.u64()?)
    }

    /// A count of items each at least `min_bytes` long: never more than the
    /// remaining input could hold, so callers may allocate for it.
    #[cfg_attr(mm_shipped, optimize(size))]
    pub fn count(&mut self, min_bytes: usize) -> Option<usize> {
        let n = self.u32()? as usize;
        (n.checked_mul(min_bytes.max(1))? <= self.rest.len()).then_some(n)
    }

    /// Length-prefixed bytes.
    #[cfg_attr(mm_shipped, optimize(size))]
    pub fn bytes(&mut self) -> Option<&'a [u8]> {
        let n = self.u32()? as usize;
        self.take(n)
    }

    /// A length-prefixed UTF-8 string.
    #[cfg_attr(mm_shipped, optimize(size))]
    pub fn str(&mut self) -> Option<&'a str> {
        core::str::from_utf8(self.bytes()?).ok()
    }
}

/// XXH64 (Yann Collet's xxHash, 64-bit variant) of `data`: the snapshot
/// checksum. Not cryptographic; snapshots are integrity-protected by the
/// host (docs/spec.md §9.3).
#[must_use]
#[cfg_attr(mm_shipped, optimize(size))]
pub fn xxh64(data: &[u8], seed: u64) -> u64 {
    const P1: u64 = 0x9E37_79B1_85EB_CA87;
    const P2: u64 = 0xC2B2_AE3D_27D4_EB4F;
    const P3: u64 = 0x1656_67B1_9E37_79F9;
    const P4: u64 = 0x85EB_CA77_C2B2_AE63;
    const P5: u64 = 0x27D4_EB2F_1656_67C5;
    #[cfg_attr(mm_shipped, optimize(size))]
    fn round(acc: u64, lane: u64) -> u64 {
        acc.wrapping_add(lane.wrapping_mul(P2))
            .rotate_left(31)
            .wrapping_mul(P1)
    }
    #[cfg_attr(mm_shipped, optimize(size))]
    fn merge(acc: u64, v: u64) -> u64 {
        (acc ^ round(0, v)).wrapping_mul(P1).wrapping_add(P4)
    }
    #[cfg_attr(mm_shipped, optimize(size))]
    fn word(b: &[u8]) -> u64 {
        <[u8; 8]>::try_from(b).map_or(0, u64::from_le_bytes)
    }
    let mut rest = data;
    let mut h = if data.len() >= 32 {
        let mut v = [
            seed.wrapping_add(P1).wrapping_add(P2),
            seed.wrapping_add(P2),
            seed,
            seed.wrapping_sub(P1),
        ];
        let (stripes, tail) = data.as_chunks::<32>();
        for s in stripes {
            for (k, lane) in v.iter_mut().enumerate() {
                *lane = round(*lane, word(&s[k * 8..k * 8 + 8]));
            }
        }
        rest = tail;
        let mut h = v[0]
            .rotate_left(1)
            .wrapping_add(v[1].rotate_left(7))
            .wrapping_add(v[2].rotate_left(12))
            .wrapping_add(v[3].rotate_left(18));
        for lane in v {
            h = merge(h, lane);
        }
        h
    } else {
        seed.wrapping_add(P5)
    };
    h = h.wrapping_add(data.len() as u64);
    let (words, tail) = rest.as_chunks::<8>();
    for w in words {
        h = (h ^ round(0, u64::from_le_bytes(*w)))
            .rotate_left(27)
            .wrapping_mul(P1)
            .wrapping_add(P4);
    }
    let (halves, tail) = tail.as_chunks::<4>();
    for w in halves {
        h = (h ^ u64::from(u32::from_le_bytes(*w)).wrapping_mul(P1))
            .rotate_left(23)
            .wrapping_mul(P2)
            .wrapping_add(P3);
    }
    for &b in tail {
        h = (h ^ u64::from(b).wrapping_mul(P5))
            .rotate_left(11)
            .wrapping_mul(P1);
    }
    h ^= h >> 33;
    h = h.wrapping_mul(P2);
    h ^= h >> 29;
    h = h.wrapping_mul(P3);
    h ^ (h >> 32)
}

#[cfg_attr(mm_shipped, optimize(size))]
fn bad<T>() -> Result<T, Error> {
    Err(Error::BadSnapshot)
}

impl Heap {
    /// Appends the heap's snapshot form to `out`. Call it right after
    /// [`Heap::collect`], with nothing allocated in between: blocks are
    /// written as live because they have a start bit.
    #[cfg_attr(mm_shipped, optimize(size))]
    pub fn write_snapshot(&self, out: &mut Vec<u8>) {
        self.write_snapshot_to(&mut |b| out.extend_from_slice(b));
    }

    /// Streams [`Heap::write_snapshot`]'s form without allocating scratch
    /// or an output buffer.
    #[cfg_attr(mm_shipped, optimize(size))]
    pub fn write_snapshot_to(&self, out: &mut dyn FnMut(&[u8])) {
        let c = self.config;
        put_u64(out, c.memory_limit);
        put_u32(out, c.gc_ratio_percent);
        put_u64(out, c.gc_min_bytes);
        let s = self.stats;
        for x in [
            s.collections,
            s.live_bytes,
            s.freed_bytes,
            s.total_freed_bytes,
            s.allocated_since_gc,
        ] {
            put_u64(out, x);
        }
        put_u64(out, self.kept_since_gc);
        put_u32(out, self.intern_table.map_or(0, Ref::raw));
        self.words.write_snapshot(out);
        self.bytes.write_snapshot(out);
    }

    /// Reads [`Heap::write_snapshot`]'s output. Segments are committed only
    /// within `max_commit` bytes (and the snapshot's own memory limit), so a
    /// malformed input cannot allocate more than that.
    ///
    /// # Errors
    ///
    /// `BadSnapshot` for anything truncated or inconsistent, `OutOfMemory`
    /// if the segments need more than the limits allow.
    #[cfg_attr(mm_shipped, optimize(size))]
    pub fn read_snapshot(r: &mut Reader<'_>, max_commit: u64) -> Result<Heap, Error> {
        let config = HeapConfig {
            memory_limit: r.u64().ok_or(Error::BadSnapshot)?.min(max_commit),
            gc_ratio_percent: r.u32().ok_or(Error::BadSnapshot)?,
            gc_min_bytes: r.u64().ok_or(Error::BadSnapshot)?,
        };
        let mut st = [0u64; 5];
        for x in &mut st {
            *x = r.u64().ok_or(Error::BadSnapshot)?;
        }
        let kept = r.u64().ok_or(Error::BadSnapshot)?;
        let intern = r.u32().ok_or(Error::BadSnapshot)?;
        let mut heap = Heap::new(config);
        heap.kept_since_gc = kept;
        let mut avail = config.memory_limit;
        heap.words
            .read_snapshot(r, &mut avail, |h| h & 0xFFFF != 0)?;
        heap.bytes
            .read_snapshot(r, &mut avail, |h| h & 0xFFFF == 1)?;
        heap.objects = heap
            .words
            .segs
            .iter()
            .flat_map(|s| s.starts.iter())
            .map(|m| m.count_ones() as usize)
            .sum();
        heap.reserve_marks(heap.objects)?;
        heap.objects_at_gc = heap.objects;
        heap.stats = GcStats {
            collections: st[0],
            live_bytes: st[1],
            freed_bytes: st[2],
            total_freed_bytes: st[3],
            allocated_since_gc: st[4],
            committed_bytes: 0,
        };
        if intern != 0 {
            let t = Ref::from_raw(intern);
            let h = heap.header(t)?;
            let cap = h.len().wrapping_sub(1);
            if h.type_id() != types::INTERN_TABLE
                || h.nvals() != h.len()
                || !cap.is_power_of_two()
                || cap < 2
            {
                return bad();
            }
            heap.intern_table = Some(t);
        }
        heap.check_objects()?;
        Ok(heap)
    }

    /// Checks every word-space object mm-core gives meaning to: each `str`
    /// has its layout, valid UTF-8 data, the right char count and ASCII flag
    /// (and hash, if cached); each intern table entry is an interned `str`.
    #[cfg_attr(mm_shipped, optimize(size))]
    fn check_objects(&self) -> Result<(), Error> {
        let mut bad_str = false;
        let mut tables = Ok(());
        self.for_each_object(&mut |r, h| match h.type_id() {
            types::STR => bad_str |= self.check_str(r, h).is_err(),
            types::INTERN_TABLE => {
                if tables.is_ok() {
                    tables = self.check_intern_table(r, h);
                }
            }
            t => bad_str |= t < types::FIRST_ENGINE,
        });
        if bad_str {
            return bad();
        }
        tables
    }

    #[cfg_attr(mm_shipped, optimize(size))]
    fn check_intern_table(&self, table: Ref, header: Header) -> Result<(), Error> {
        let n = header.len();
        let cap = n.wrapping_sub(1);
        if header.nvals() != n || cap < 2 || !cap.is_power_of_two() {
            return bad();
        }
        let mut count = 0u32;
        for i in 1..n {
            let v = self.get(table, i)?;
            if v == Value::UNDEFINED {
                continue;
            }
            let e = v.as_obj().ok_or(Error::BadSnapshot)?;
            let h = self.header(e)?;
            if h.type_id() != types::STR || h.flags() & flags::INTERNED == 0 {
                return bad();
            }
            count += 1;
        }
        if count > cap / 2 || self.get(table, 0)?.as_int() != Some(i64::from(count)) {
            return bad();
        }
        Ok(())
    }

    #[cfg_attr(mm_shipped, optimize(size))]
    fn check_str(&self, r: Ref, h: Header) -> Result<(), Error> {
        if h.len() != 3 || h.nvals() != 1 {
            return bad();
        }
        let data = self.str_bytes(r).map_err(|_| Error::BadSnapshot)?;
        let text = core::str::from_utf8(data).map_err(|_| Error::BadSnapshot)?;
        let chars = if text.is_ascii() {
            text.len()
        } else {
            text.chars().count()
        };
        let ascii = chars == text.len();
        if self.str_chars(r)? != chars as u64
            || (h.flags() & flags::ASCII != 0) != ascii
            || (h.flags() & flags::INTERNED != 0 && h.flags() & flags::HASHED == 0)
            || (h.flags() & flags::HASHED != 0
                && self.str_hash(r)? != crate::string::hash_bytes(data))
        {
            return bad();
        }
        Ok(())
    }
}

impl<S: Store> Space<S> {
    #[cfg_attr(mm_shipped, optimize(size))]
    fn write_snapshot(&self, out: &mut dyn FnMut(&[u8])) {
        put_len(out, self.segs.len().max(1));
        put_len(out, self.bump);
        put_len(out, self.vacant.len());
        for &v in &self.vacant {
            put_len(out, v);
        }
        let present = self.segs.iter().filter(|s| s.present).count();
        put_len(out, present);
        for (idx, s) in self.segs.iter().enumerate() {
            if !s.present {
                continue;
            }
            put_len(out, idx);
            put_u8(out, u8::from(s.large));
            put_len(out, s.top);
            // Count first, then stream: no vector proportional to heap
            // fragmentation, and the existing binary form is unchanged.
            let mut count = 0;
            live_runs(s, &mut |_, _| count += 1);
            put_len(out, count);
            // A large segment's room past its block (`append_bytes`) is a
            // number, never bytes: restoring reserves it again.
            put_len(out, s.cap());
            live_runs(s, &mut |off, units| {
                put_len(out, off);
                put_len(out, units);
                s.data.append_units(off, units, out);
            });
        }
    }

    /// Reads one space; `valid` accepts a block header word of this space.
    #[cfg_attr(mm_shipped, optimize(size))]
    fn read_snapshot(
        &mut self,
        input: &mut Reader<'_>,
        avail: &mut u64,
        valid: fn(u64) -> bool,
    ) -> Result<(), Error> {
        let nsegs = input.u32().ok_or(Error::BadSnapshot)? as usize;
        if nsegs == 0 || nsegs > crate::space::MAX_SEGS {
            return bad();
        }
        let bump = input.u32().ok_or(Error::BadSnapshot)? as usize;
        let nvacant = input.count(4).ok_or(Error::BadSnapshot)?;
        if nsegs == 1 {
            // The reserved zero index is implicit for an unused space.
            if bump != 0 || nvacant != 0 || input.u32() != Some(0) {
                return bad();
            }
            return Ok(());
        }
        let before = self.metadata_bytes();
        self.reserve_tables(nsegs, *avail)?;
        *avail -= self.metadata_bytes() - before;
        self.segs.resize_with(nsegs, crate::space::Seg::vacant);
        self.marks.resize_with(nsegs, || Box::new([]));
        // Borrow the dirty flag as a visited bit instead of allocating a
        // second index table; every segment ends clean.
        for _ in 0..nvacant {
            let v = input.u32().ok_or(Error::BadSnapshot)? as usize;
            match self.segs.get_mut(v) {
                Some(s) if v != 0 && s.dirty == 0 => s.dirty = 1,
                _ => return bad(),
            }
            self.vacant.push(v);
        }
        let present = input.count(13).ok_or(Error::BadSnapshot)?;
        let mut prev = 0;
        for _ in 0..present {
            let idx = input.u32().ok_or(Error::BadSnapshot)? as usize;
            if idx <= prev || self.segs.get(idx).is_none_or(|s| s.dirty != 0) {
                return bad();
            }
            prev = idx;
            let large = input.bool().ok_or(Error::BadSnapshot)?;
            let top = input.u32().ok_or(Error::BadSnapshot)? as usize;
            let nruns = input.count(8).ok_or(Error::BadSnapshot)?;
            let cap = input.u32().ok_or(Error::BadSnapshot)? as usize;
            if large {
                // One block filling the segment, its data here, in a room
                // that holds it and no more than the largest object.
                let room = cap >= top && !crate::space::over_cap(cap);
                if top < LARGE_UNITS || nruns != 1 || input.remaining() / 8 < top || !room {
                    return bad();
                }
                let fits = self.install_large(idx, top, cap, avail);
                fits.then_some(()).ok_or(Error::OutOfMemory)?;
            } else {
                if !cap.is_power_of_two()
                    || !(FIRST_UNITS..=SEG_UNITS).contains(&cap)
                    || top > cap
                    || (nruns == 0 && idx != bump)
                {
                    return bad();
                }
                if !self.install_normal(idx, top, cap, avail) {
                    return Err(Error::OutOfMemory);
                }
            }
            let seg = self.segs.get_mut(idx).ok_or(Error::BadSnapshot)?;
            seg.dirty = 1;
            let mut end = 0;
            for _ in 0..nruns {
                let off = input.u32().ok_or(Error::BadSnapshot)? as usize;
                let units = input.u32().ok_or(Error::BadSnapshot)? as usize;
                if off < end || units == 0 || off + units > top {
                    return bad();
                }
                let data = input
                    .take(units.checked_mul(8).ok_or(Error::BadSnapshot)?)
                    .ok_or(Error::BadSnapshot)?;
                seg.data.load_units(off, data);
                end = off + units;
                // The blocks must tile the run exactly.
                let mut at = off;
                while at < end {
                    let h = seg.data.read(at).ok_or(Error::BadSnapshot)?;
                    let size = S::units_for(header_len(h));
                    if !valid(h) || at + size > end || (large && (at != 0 || size != top)) {
                        return bad();
                    }
                    crate::space::set_bit(&mut seg.starts, at);
                    seg.used += size;
                    at += size;
                }
            }
            // Free space between the last run and `top` (the sweep below
            // lists it).
            seg.data.fill_to(top);
        }
        if bump != 0 && self.segs.get(bump).is_none_or(|s| !s.present || s.large) {
            return bad();
        }
        for s in self.segs.iter_mut().skip(1) {
            if s.dirty == 0 {
                // Every index is either vacant or present.
                return bad();
            }
            s.dirty = 0;
        }
        self.bump = bump;
        // The collection's sweep, with every block marked, rebuilds the free
        // lists exactly as the snapshotted heap had them.
        for (seg, m) in self.segs.iter().zip(self.marks.iter_mut()) {
            m.copy_from_slice(&seg.starts);
        }
        self.sweep(false);
        Ok(())
    }
}

/// The live runs of a segment: `(unit offset, units)` of each maximal
/// sequence of adjacent blocks with start bits.
#[cfg_attr(mm_shipped, optimize(size))]
fn live_runs<S: Store>(s: &crate::space::Seg<S>, emit: &mut dyn FnMut(usize, usize)) {
    let (mut start, mut len) = (0, 0);
    'runs: for (w, &bits) in s.starts.iter().enumerate() {
        let mut b = bits;
        while b != 0 {
            let off = w * 64 + b.trailing_zeros() as usize;
            b &= b - 1;
            let Some(h) = s.data.read(off) else {
                break 'runs;
            };
            let units = S::units_for(header_len(h));
            if start + len == off {
                len += units;
            } else {
                if len != 0 {
                    emit(start, len);
                }
                (start, len) = (off, units);
            }
        }
    }
    if len != 0 {
        emit(start, len);
    }
}
