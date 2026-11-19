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

use crate::gc::{MAX_BACKOFF, Mark};
use crate::heap::{Error, GcStats, Header, Heap, HeapConfig, Ref, flags, types};
use crate::space::{
    DIRTY_REFS, FIRST_UNITS, LARGE_UNITS, MED_CLASSES, SEG_UNITS, SMALL_MAX, Space, Store, addr_of,
    header_len, link, med_class, split,
};
use crate::string::POS_WAYS;
use crate::value::{HI_BYTES, HI_REF, Value};
use alloc::{boxed::Box, vec::Vec};

/// Where an image goes: counted, to size its buffer first, or appended to
/// that buffer. A concrete writer, so a fixed-size field is stored inline
/// rather than copied by a `memcpy` call through a closure.
pub enum Sink<'a> {
    /// Counts the bytes that would be written.
    Count(usize),
    /// Appends them.
    Buf(&'a mut Vec<u8>),
}

impl Sink<'_> {
    /// Appends `b`.
    #[inline]
    pub fn put(&mut self, b: &[u8]) {
        match self {
            Sink::Count(n) => *n = n.saturating_add(b.len()),
            Sink::Buf(v) => v.extend_from_slice(b),
        }
    }
}

/// Sorts `v` ascending (heapsort: far smaller than the standard library's
/// sorts, and the codec sorts only what a check or a write lists). Not
/// stable.
fn heap_sort<X: Ord>(v: &mut [X]) {
    let sift = |v: &mut [X], mut root: usize, end: usize| loop {
        let mut child = 2 * root + 1;
        if child >= end {
            break;
        }
        if child + 1 < end && v[child] < v[child + 1] {
            child += 1;
        }
        if v[root] >= v[child] {
            break;
        }
        v.swap(root, child);
        root = child;
    };
    let n = v.len();
    for i in (0..n / 2).rev() {
        sift(v, i, n);
    }
    for end in (1..n).rev() {
        v.swap(0, end);
        sift(v, 0, end);
    }
}

/// Appends `v`.
#[inline(never)]
pub fn put_u8(out: &mut Sink<'_>, v: u8) {
    out.put(&[v]);
}

/// Appends `v`, little-endian.
#[inline(never)]
pub fn put_u32(out: &mut Sink<'_>, v: u32) {
    out.put(&v.to_le_bytes());
}

/// Appends `v`, little-endian.
#[inline(never)]
pub fn put_u64(out: &mut Sink<'_>, v: u64) {
    out.put(&v.to_le_bytes());
}

/// Appends a length (as `u32`; longer is cut, which the reader rejects).
#[inline(never)]
pub fn put_len(out: &mut Sink<'_>, n: usize) {
    put_u32(out, u32::try_from(n).unwrap_or(u32::MAX));
}

/// Appends a length-prefixed byte string.
#[cfg_attr(mm_shipped, optimize(size))]
pub fn put_bytes(out: &mut Sink<'_>, b: &[u8]) {
    put_len(out, b.len());
    out.put(b);
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

    /// `out.len()` little-endian `u32`s.
    pub fn u32s(&mut self, out: &mut [u32]) -> Option<()> {
        let b = self.take(out.len() * 4)?;
        for (x, c) in out.iter_mut().zip(b.as_chunks::<4>().0) {
            *x = u32::from_le_bytes(*c);
        }
        Some(())
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
    /// A restored value may not refer to a block that the next lazy sweep
    /// will free, even though that block still has an allocation header.
    #[must_use]
    pub fn snapshot_ref_live(&self, v: Value) -> bool {
        if let Some(r) = v.as_obj() {
            self.words.live_start(r.raw())
        } else {
            v.as_byte_ref()
                .is_none_or(|b| self.bytes.live_start(b.raw()))
        }
    }

    /// Appends the heap's snapshot form to `out`: the heap exactly as it
    /// is, garbage, free blocks and the collector's state included, so the
    /// restored heap is the one that goes on (docs/spec.md §9.3).
    #[cfg_attr(mm_shipped, optimize(size))]
    pub fn write_snapshot(&self, out: &mut Vec<u8>) {
        self.write_snapshot_to(&mut Sink::Buf(out));
    }

    /// Streams [`Heap::write_snapshot`]'s form without allocating scratch
    /// or an output buffer.
    #[cfg_attr(mm_shipped, optimize(size))]
    pub fn write_snapshot_to(&self, out: &mut Sink<'_>) {
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
        // The collector's state: no collection comes before an image, so a
        // mark in progress goes on after a restore where it stopped.
        put_len(out, self.objects);
        put_len(out, self.objects_at_gc);
        put_len(out, self.mark_stack.capacity());
        put_len(out, self.mark_stack.len());
        for &a in &self.mark_stack {
            put_u32(out, a);
        }
        let i = &self.inc;
        put_u8(out, i.mark as u8);
        put_len(out, i.cursor);
        for x in [i.passes, i.found, i.prev, i.backoff, i.penalty] {
            put_u32(out, x);
        }
        for x in [i.first, i.rescanned, i.began, i.paced] {
            put_u64(out, x);
        }
        put_u8(out, u8::from(i.forced));
        put_len(out, i.partial.len());
        for &(a, from) in &i.partial {
            put_u32(out, a);
            put_u32(out, from);
        }
        // The positions found in non-ASCII strs: walks are charged, so a
        // restored session must find what the one that goes on finds.
        put_len(out, self.str_pos.at.len());
        for &(r, [c, b]) in &self.str_pos.at {
            put_u32(out, r.raw());
            put_len(out, c);
            put_len(out, b);
        }
        put_len(out, self.str_pos.next);
        put_u8(out, u8::from(self.idle_collected));
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
        Self::read_snapshot_as(r, max_commit, true)
    }

    /// [`Heap::read_snapshot`] of an image this process already read with
    /// it (a session template's): the same heap, admitted the same way,
    /// without the walk that checks every `str` and intern table. Only for
    /// bytes that passed the checked read and have not changed since.
    ///
    /// # Errors
    ///
    /// As [`Heap::read_snapshot`], apart from what the skipped walk finds.
    #[doc(hidden)]
    #[cfg_attr(mm_shipped, optimize(size))]
    pub fn read_snapshot_trusted(r: &mut Reader<'_>, max_commit: u64) -> Result<Heap, Error> {
        Self::read_snapshot_as(r, max_commit, false)
    }

    #[cfg_attr(mm_shipped, optimize(size))]
    fn read_snapshot_as(r: &mut Reader<'_>, max_commit: u64, check: bool) -> Result<Heap, Error> {
        let e = Error::BadSnapshot;
        let config = HeapConfig {
            memory_limit: r.u64().ok_or(e)?.min(max_commit),
            gc_ratio_percent: r.u32().ok_or(e)?,
            gc_min_bytes: r.u64().ok_or(e)?,
        };
        let mut st = [0u64; 5];
        for x in &mut st {
            *x = r.u64().ok_or(e)?;
        }
        let kept = r.u64().ok_or(e)?;
        let intern = r.u32().ok_or(e)?;
        let mut heap = Heap::new(config);
        heap.kept_since_gc = kept;
        let mut avail = config.memory_limit;
        heap.words
            .read_snapshot(r, &mut avail, |h| h & 0xFFFF != 0)?;
        heap.bytes
            .read_snapshot(r, &mut avail, |h| h & 0xFFFF == 1)?;
        heap.objects = r.u32().ok_or(e)? as usize;
        heap.objects_at_gc = r.u32().ok_or(e)? as usize;
        heap.read_mark_state(r, avail)?;
        let ns = r.count(12).ok_or(e)?;
        if ns > POS_WAYS {
            return bad();
        }
        for _ in 0..ns {
            let text = Ref::from_raw(r.u32().ok_or(e)?);
            let chars = r.u32().ok_or(e)? as usize;
            let bytes = r.u32().ok_or(e)? as usize;
            heap.str_pos.at.push((text, [chars, bytes]));
        }
        heap.str_pos.next = r.u32().ok_or(e)? as usize;
        if heap.str_pos.next >= POS_WAYS {
            return bad();
        }
        heap.idle_collected = r.bool().ok_or(e)?;
        heap.stats = GcStats {
            collections: st[0],
            live_bytes: st[1],
            freed_bytes: st[2],
            total_freed_bytes: st[3],
            allocated_since_gc: st[4],
            committed_bytes: 0,
        };
        if heap.marking()
            && !(heap.inc.began..=heap.stats.allocated_since_gc).contains(&heap.inc.paced)
        {
            return bad();
        }
        if intern != 0 {
            let t = Ref::from_raw(intern);
            if !heap.words.live_start(intern) {
                return bad();
            }
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
        if check {
            heap.check_objects()?;
            heap.check_str_pos()?;
            heap.check_mark()?;
        }
        Ok(heap)
    }

    /// Reads the collector's mark state: the mark stack and its reserved
    /// capacity, the incremental mark and its partly scanned objects, each
    /// checked against the marks the segments hold.
    #[cfg_attr(mm_shipped, optimize(size))]
    fn read_mark_state(&mut self, r: &mut Reader<'_>, avail: u64) -> Result<(), Error> {
        let e = Error::BadSnapshot;
        let cap = r.u32().ok_or(e)? as usize;
        let n = r.count(4).ok_or(e)?;
        // The mark stack holds each marked object once, and every object
        // allocated has a slot reserved (`Heap::alloc_words`).
        if self.objects != self.words.object_count()
            || self.objects_at_gc > self.objects
            || cap < self.objects
            || n > cap
        {
            return bad();
        }
        if cap as u64 * 4 > avail {
            return Err(Error::OutOfMemory);
        }
        self.mark_stack
            .try_reserve_exact(cap)
            .map_err(|_| Error::OutOfMemory)?;
        for _ in 0..n {
            let a = r.u32().ok_or(e)?;
            if !self.words.marked_start(a) {
                return bad();
            }
            self.mark_stack.push(a);
        }
        let inc = &mut self.inc;
        inc.mark = match r.u8().ok_or(e)? {
            0 => Mark::Idle,
            1 => Mark::Running,
            2 => Mark::Held,
            _ => return bad(),
        };
        inc.cursor = r.u32().ok_or(e)? as usize;
        for x in [
            &mut inc.passes,
            &mut inc.found,
            &mut inc.prev,
            &mut inc.backoff,
            &mut inc.penalty,
        ] {
            *x = r.u32().ok_or(e)?;
        }
        for x in [
            &mut inc.first,
            &mut inc.rescanned,
            &mut inc.began,
            &mut inc.paced,
        ] {
            *x = r.u64().ok_or(e)?;
        }
        inc.forced = r.bool().ok_or(e)?;
        let np = r.count(8).ok_or(e)?;
        inc.partial
            .try_reserve_exact(np)
            .map_err(|_| Error::OutOfMemory)?;
        for _ in 0..np {
            let a = r.u32().ok_or(e)?;
            let from = r.u32().ok_or(e)?;
            if !self.words.marked_start(a) {
                return bad();
            }
            self.inc.partial.push((a, from));
        }
        // A mark holds marks in no segment the last sweep left pending (a
        // mark starts once that sweep is done); between marks, only those
        // segments keep marks, the last collection's.
        let idle = self.inc.mark == Mark::Idle;
        if self.inc.cursor > self.words.segs.len()
            || self.inc.backoff > MAX_BACKOFF
            || self.inc.penalty > MAX_BACKOFF
            || (idle && (n != 0 || np != 0))
            || !self.words.marks_consistent(idle)
            || !self.bytes.marks_consistent(idle)
        {
            return bad();
        }
        Ok(())
    }

    /// Each remembered position is a char's own offset in a live,
    /// non-ASCII `str`, with the chars before it counted right.
    #[cfg_attr(mm_shipped, optimize(size))]
    fn check_str_pos(&self) -> Result<(), Error> {
        for &(s, [c, b]) in &self.str_pos.at {
            if !self.words.live_start(s.raw()) {
                return bad();
            }
            let h = self.header(s)?;
            if h.type_id() != types::STR || h.flags() & flags::ASCII != 0 {
                return bad();
            }
            let data = self.str_bytes(s)?;
            let ok = data.get(b).is_some_and(|&x| x & 0xC0 != 0x80)
                && data
                    .get(..b)
                    .is_some_and(|d| crate::string::count_leads(d) == c);
            if !ok {
                return bad();
            }
        }
        Ok(())
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
            t => {
                bad_str |= t < types::FIRST_ENGINE
                    || !self.slot_words(r).is_ok_and(|slots| {
                        slots.iter().take(h.nvals() as usize).all(|&w| {
                            Value(w)
                                .as_byte_ref()
                                .is_none_or(|b| self.bytes.live_start(b.raw()))
                        })
                    });
            }
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
        // One pass over the table's words, from the slot after a free one
        // (half the slots are), so `run` is the occupied slots since the
        // last free one. Every entry is an interned `str` where a lookup
        // finds it: at most `INTERN_WALK_CAP` slots after its hash's,
        // with no free slot between, and with its hash's byte as its
        // kind byte (`hash_tag`).
        let entries = self.slot_words(table)?.get(1..).ok_or(Error::BadSnapshot)?;
        let mask = entries.len().wrapping_sub(1);
        let origin = entries
            .iter()
            .position(|&w| w == Value::UNDEFINED.0)
            .map_or(0, |i| i + 1);
        let (mut count, mut run) = (0u32, 0usize);
        for k in 0..entries.len() {
            let i = (origin + k) & mask;
            let entry = Value(entries.get(i).copied().unwrap_or(0));
            if entry == Value::UNDEFINED {
                run = 0;
                continue;
            }
            // (A valid encoding: no bits between the kind byte and the tag.)
            let e = Value::from_bits(entry.0)
                .and_then(Value::as_obj)
                .ok_or(Error::BadSnapshot)?;
            if !self.words.live_start(e.raw()) {
                return bad();
            }
            let h = self.header(e)?;
            // (`check_str` refuses an interned `str` without its hash.)
            let interned = flags::INTERNED | flags::HASHED;
            if h.type_id() != types::STR || h.flags() & interned != interned {
                return bad();
            }
            let hash = self.get_raw(e, crate::string::SLOT_HASH)?;
            #[allow(clippy::cast_possible_truncation, reason = "masked to the table size")]
            let walk = i.wrapping_sub(hash as usize) & mask;
            if walk > run.min(crate::string::INTERN_WALK_CAP)
                || entry.obj_kind() != Some(crate::string::hash_tag(hash))
            {
                return bad();
            }
            count += 1;
            run += 1;
        }
        if count > cap / 2 || self.get(table, 0)?.as_int() != Some(i64::from(count)) {
            return bad();
        }
        Ok(())
    }

    #[cfg_attr(mm_shipped, optimize(size))]
    fn check_str(&self, r: Ref, h: Header) -> Result<(), Error> {
        if h.len() != 3 || h.nvals() != 1 || !self.snapshot_ref_live(self.get(r, 0)?) {
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

    /// During a mark, every marked object of a clean segment that is not
    /// waiting on the mark stack (or, for its slots not scanned yet, in the
    /// partial list) holds no reference to an unmarked block: the remark
    /// rescans only the roots and the dirty segments, so a restored mark
    /// that broke this would free a reachable object.
    #[cfg_attr(mm_shipped, optimize(size))]
    fn check_mark(&self) -> Result<(), Error> {
        if self.inc.mark == Mark::Idle {
            return Ok(());
        }
        let mut grey = self.mark_stack.clone();
        heap_sort(&mut grey);
        let mut partial = self.inc.partial.clone();
        heap_sort(&mut partial);
        let unmarked = |w: u64| {
            #[allow(
                clippy::cast_possible_truncation,
                reason = "low 32 bits are the reference"
            )]
            let a = w as u32;
            match w >> 48 {
                HI_REF => self.words.unmarked_start(a),
                HI_BYTES => self.bytes.unmarked_start(a),
                _ => false,
            }
        };
        for (idx, s) in self.words.segs.iter().enumerate() {
            if !s.present || s.dirty & DIRTY_REFS != 0 {
                continue;
            }
            let marks = self.words.marks.get(idx).map_or(&[][..], |m| &m[..]);
            for (w, (&st, &mk)) in s.starts.iter().zip(marks).enumerate() {
                let mut bits = st & mk;
                while bits != 0 {
                    let off = w * 64 + bits.trailing_zeros() as usize;
                    bits &= bits - 1;
                    let a = addr_of(idx, off);
                    // The intern table is weak: its own mark traces none
                    // of its strings.
                    if grey.binary_search(&a).is_ok() || self.intern_table == Some(Ref::from_raw(a))
                    {
                        continue;
                    }
                    let h = s.data.read(off).ok_or(Error::BadSnapshot)?;
                    let mut n = Header::nvals_of(h) as usize;
                    let at = partial.partition_point(|&(p, _)| p < a);
                    if let Some(&(p, from)) = partial.get(at)
                        && p == a
                    {
                        n = n.min(from as usize);
                    }
                    let slots = s.data.get(off + 1..off + 1 + n).ok_or(Error::BadSnapshot)?;
                    if slots.iter().any(|&v| unmarked(v)) {
                        return bad();
                    }
                }
            }
        }
        Ok(())
    }
}

impl<S: Store> Space<S> {
    /// One space as it is: per present segment its blocks (`[0, top)`,
    /// free ones and garbage included), its dirty flag and its marks if it
    /// has any; then the free run, the free-list heads and the segments the
    /// lazy sweep has not reached. A restore derives the start bitmaps,
    /// `used`, the list masks and the listed units from the blocks.
    #[cfg_attr(mm_shipped, optimize(size))]
    fn write_snapshot(&self, out: &mut Sink<'_>) {
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
            put_u8(out, s.dirty);
            put_len(out, s.top);
            // A large segment's room past its block (`append_bytes`) is a
            // number, never bytes: restoring reserves it again.
            put_len(out, s.cap());
            s.data.append_units(0, s.top, out);
            let marks = self.marks.get(idx).map_or(&[][..], |m| &m[..]);
            let marked = marks.iter().any(|&m| m != 0);
            put_u8(out, u8::from(marked));
            if marked {
                for &m in marks {
                    put_u64(out, m);
                }
            }
        }
        let (seg, cur, end) = self.run;
        put_len(out, seg);
        put_len(out, cur);
        put_len(out, end);
        for &h in &self.lists.small {
            put_u32(out, h);
        }
        put_u8(out, u8::from(!self.lists.medium.is_empty()));
        for &h in &self.lists.medium {
            put_u32(out, h);
        }
        put_len(out, self.pending.len());
        for &p in &self.pending {
            put_len(out, p);
        }
        put_u64(out, self.lazily_freed);
    }

    /// Reads one space; `valid` accepts a block header word of this space.
    /// Everything the allocator and the collector trust is checked against
    /// the blocks: they tile each segment, every listed block and the run's
    /// rest is a free block of its list's size, listed once, outside the
    /// segments the lazy sweep has not reached, and marks sit on starts.
    #[cfg_attr(mm_shipped, optimize(size))]
    #[allow(
        clippy::too_many_lines,
        reason = "one reader; its checks share the tables"
    )]
    fn read_snapshot(
        &mut self,
        input: &mut Reader<'_>,
        avail: &mut u64,
        valid: fn(u64) -> bool,
    ) -> Result<(), Error> {
        let e = Error::BadSnapshot;
        let nsegs = input.u32().ok_or(e)? as usize;
        if nsegs == 0 || nsegs > crate::space::MAX_SEGS {
            return bad();
        }
        let bump = input.u32().ok_or(e)? as usize;
        let nvacant = input.count(4).ok_or(e)?;
        if nsegs > 1 {
            let before = self.metadata_bytes();
            self.reserve_tables(nsegs, *avail)?;
            *avail -= self.metadata_bytes() - before;
            self.segs.resize_with(nsegs, crate::space::Seg::vacant);
            self.marks.resize_with(nsegs, || Box::new([]));
        }
        // Each index is vacant or present, once.
        let mut seen = alloc::vec![0u64; nsegs.div_ceil(64)];
        let see = |seen: &mut [u64], i: usize| {
            if i == 0 || i >= nsegs || crate::space::bit(seen, i) {
                return false;
            }
            crate::space::set_bit(seen, i);
            true
        };
        for _ in 0..nvacant {
            let v = input.u32().ok_or(e)? as usize;
            if !see(&mut seen, v) {
                return bad();
            }
            self.vacant.push(v);
        }
        // Free blocks, by address (ascending): what the lists may name.
        let mut free = Vec::new();
        let present = input.count(15).ok_or(e)?;
        if present + nvacant + 1 != nsegs && nsegs > 1 || (nsegs == 1 && present != 0) {
            return bad();
        }
        let mut prev = 0;
        for _ in 0..present {
            let idx = input.u32().ok_or(e)? as usize;
            if idx <= prev || !see(&mut seen, idx) {
                return bad();
            }
            prev = idx;
            let large = input.bool().ok_or(e)?;
            let dirty = input.u8().ok_or(e)?;
            let top = input.u32().ok_or(e)? as usize;
            let cap = input.u32().ok_or(e)? as usize;
            if dirty & !DIRTY_REFS != 0 || input.remaining() / 8 < top {
                return bad();
            }
            if large {
                // One block filling the segment, in a room that holds it and
                // no more than the largest object.
                let room = cap >= top && !crate::space::over_cap(cap);
                if top < LARGE_UNITS || !room {
                    return bad();
                }
                let fits = self.install_large(idx, top, cap, avail);
                fits.then_some(()).ok_or(Error::OutOfMemory)?;
            } else {
                if !cap.is_power_of_two() || !(FIRST_UNITS..=SEG_UNITS).contains(&cap) || top > cap
                {
                    return bad();
                }
                if !self.install_normal(idx, top, cap, avail) {
                    return Err(Error::OutOfMemory);
                }
            }
            let seg = self.segs.get_mut(idx).ok_or(e)?;
            seg.data.load_units(0, input.take(top * 8).ok_or(e)?);
            seg.data.fill_to(top);
            // The blocks tile `[0, top)`: an allocated block has a header
            // of this space and a start bit, a free one type 0.
            let mut at = 0;
            while at < top {
                let h = seg.data.read(at).ok_or(e)?;
                let size = S::units_for(header_len(h));
                if size > top - at || (large && (at != 0 || size != top)) {
                    return bad();
                }
                // A free block's low half is zero (type 0, no flags); any
                // other type-0 header fails `valid`.
                if h.trailing_zeros() >= 32 {
                    if large {
                        return bad();
                    }
                    free.try_reserve(1).map_err(|_| Error::OutOfMemory)?;
                    free.push(addr_of(idx, at));
                } else {
                    if !valid(h) {
                        return bad();
                    }
                    crate::space::set_bit(&mut seg.starts, at);
                    seg.used += size;
                }
                at += size;
            }
            seg.dirty = dirty;
            if input.bool().ok_or(e)? {
                let m = self.marks.get_mut(idx).ok_or(e)?;
                for (w, &st) in m.iter_mut().zip(seg.starts.iter()) {
                    *w = input.u64().ok_or(e)?;
                    if *w & !st != 0 {
                        return bad();
                    }
                }
            }
        }
        let normal = |s: &crate::space::Seg<S>| s.present && !s.large;
        if bump != 0 && self.segs.get(bump).is_none_or(|s| !normal(s)) {
            return bad();
        }
        self.bump = bump;
        let run = (
            input.u32().ok_or(e)? as usize,
            input.u32().ok_or(e)? as usize,
            input.u32().ok_or(e)? as usize,
        );
        let mut small = [0u32; SMALL_MAX + 1];
        input.u32s(&mut small).ok_or(e)?;
        let medium = if input.bool().ok_or(e)? {
            let mut m = Vec::new();
            m.try_reserve_exact(MED_CLASSES)
                .map_err(|_| Error::OutOfMemory)?;
            m.resize(MED_CLASSES, 0);
            input.u32s(&mut m).ok_or(e)?;
            m
        } else {
            Vec::new()
        };
        let npending = input.count(4).ok_or(e)?;
        // Unswept segments: each once, partly live, not the bump segment
        // (a lazy sweep leaves that one swept).
        seen.fill(0);
        for _ in 0..npending {
            let p = input.u32().ok_or(e)? as usize;
            if p == bump || self.segs.get(p).is_none_or(|s| !normal(s)) || !see(&mut seen, p) {
                return bad();
            }
            self.segs[p].unswept = true;
            self.pending.push(p);
        }
        self.lazily_freed = input.u64().ok_or(e)?;
        if self.lazily_freed > self.committed / 8 {
            return bad();
        }
        // `seen` now holds the pending segments.
        let unswept = |a: u32| crate::space::bit(&seen, split(a).0);
        // A free block of `units` at `a` taken once: by the run, or a list.
        let mut listed = alloc::vec![0u64; free.len().div_ceil(64)];
        let mut take = |segs: &[crate::space::Seg<S>], a: u32| -> Option<usize> {
            let i = free.binary_search(&a).ok()?;
            if crate::space::bit(&listed, i) || unswept(a) {
                return None;
            }
            crate::space::set_bit(&mut listed, i);
            let (s, o) = split(a);
            Some(S::units_for(header_len(segs.get(s)?.data.read(o)?)))
        };
        let (rs, rc, re) = run;
        if rs == 0 {
            if rc != 0 || re != 0 {
                return bad();
            }
        } else {
            let top = self.segs.get(rs).filter(|s| normal(s)).map(|s| s.top);
            if top.is_none_or(|t| rc > re || re > t)
                || (rc < re && take(&self.segs, addr_of(rs, rc)) != Some(re - rc))
            {
                return bad();
            }
        }
        self.run = run;
        let mut units = 0u64;
        for (n, &head) in small.iter().enumerate() {
            let mut cur = head;
            while cur != crate::space::NIL {
                if n < 2 || take(&self.segs, cur) != Some(n) {
                    return bad();
                }
                units += n as u64;
                let (s, o) = split(cur);
                cur = link(self.segs.get(s).and_then(|s| s.data.read(o + 1)).ok_or(e)?);
            }
            if head != crate::space::NIL {
                self.lists.mask |= 1 << n;
            }
        }
        for (c, &head) in medium.iter().enumerate() {
            let mut cur = head;
            while cur != crate::space::NIL {
                let m = take(&self.segs, cur).ok_or(e)?;
                if m <= SMALL_MAX || med_class(m) != c {
                    return bad();
                }
                units += m as u64;
                let (s, o) = split(cur);
                cur = link(self.segs.get(s).and_then(|s| s.data.read(o + 1)).ok_or(e)?);
            }
            if head != crate::space::NIL
                && let Some(bits) = self.lists.medium_mask.get_mut(c / 64)
            {
                *bits |= 1 << (c % 64);
            }
        }
        self.lists.small = small;
        self.lists.medium = medium;
        self.lists.free_units = units;
        Ok(())
    }

    /// Whether `a` starts a block of this space.
    fn starts_block(&self, a: u32) -> bool {
        self.block(a).is_some()
    }

    fn live_start(&self, a: u32) -> bool {
        let (i, o) = split(a);
        self.block(a).is_some_and(|(s, _)| {
            !s.unswept || self.marks.get(i).is_some_and(|m| crate::space::bit(m, o))
        })
    }

    /// Whether `a` starts a block whose mark bit is set.
    pub(crate) fn marked_start(&self, a: u32) -> bool {
        let (i, o) = split(a);
        self.starts_block(a) && self.marks.get(i).is_some_and(|m| crate::space::bit(m, o))
    }

    /// Whether `a` starts a block whose mark bit is clear.
    fn unmarked_start(&self, a: u32) -> bool {
        self.starts_block(a) && !self.marked_start(a)
    }

    /// Blocks with a start bit, those the lazy sweep has not freed yet
    /// (unmarked, in a pending segment) aside: [`Heap::objects`] in the
    /// word space.
    fn object_count(&self) -> usize {
        let mut n = 0;
        for (idx, s) in self.segs.iter().enumerate() {
            let marks = self.marks.get(idx).map_or(&[][..], |m| &m[..]);
            for (w, &st) in s.starts.iter().enumerate() {
                let mk = marks.get(w).copied().unwrap_or(0);
                n += (if s.unswept { st & mk } else { st }).count_ones() as usize;
            }
        }
        n
    }

    /// Marks outside the segments the lazy sweep has not reached are a
    /// mark's: none while no mark runs (`idle`), and no segment is pending
    /// while one does (a mark starts once the last sweep is done).
    fn marks_consistent(&self, idle: bool) -> bool {
        if !idle {
            return self.pending.is_empty();
        }
        self.marks
            .iter()
            .zip(&self.segs)
            .all(|(m, s)| s.unswept || m.iter().all(|&w| w == 0))
    }
}
