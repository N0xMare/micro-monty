//! The heap: a word space for objects and a byte space for their byte data
//! (docs/spec.md §7.2), the memory limit (§11) and the write barrier (§9.3).
//!
//! # Objects
//!
//! An object is a header word followed by `len` slots. The first `nvals`
//! slots hold `Value`s, which the GC traces; the rest are raw words it never
//! looks at. Values that are not references (a `SmallInt` shape id, say) may
//! sit in value slots too.
//!
//! ```text
//! header  bits 0..16   type id (0 = free block; 1..16 reserved for mm-core)
//!         bits 16..24  flags (INTERNED and ASCII are mm-core's, the rest the
//!                      engine's)
//!         bits 24..32  value-slot count: 0..=254, or 255 for "all slots"
//!         bits 32..64  slot count `len`
//! ```
//!
//! The mark bit is not in the header: marks live in a side bitmap per segment.
//!
//! # Byte blocks
//!
//! A byte block is a header granule (bits 0..16 = 1, bits 32..64 = byte
//! length) followed by the data, padded with zeros to 8 bytes. An object owns
//! a block by storing `Value::from_byte_ref` in one of its value slots; the GC
//! marks every block reachable that way and frees the others.
//!
//! # Safety net
//!
//! Every reference is checked against the object-start bitmap, every slot index
//! against the header, and headers are written only by mm-core. A stale or
//! forged reference is an `Error::BadRef`, never a panic, and cannot corrupt
//! the heap's structure.

use crate::space::{self, LARGE_UNITS, Space, Store, header_len};
use crate::value::Value;
use alloc::{sync::Arc, vec::Vec};
use core::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};

/// Bytes in a full normal segment of either space. Each space's first
/// segment starts at 32 KiB and grows to this; dedicated segments for large
/// objects are sized to fit.
pub const SEGMENT_BYTES: u64 = (space::SEG_UNITS as u64) * 8;

/// Objects of at least this many bytes, header included, get a dedicated
/// segment of their own.
pub const LARGE_OBJECT_BYTES: u64 = (LARGE_UNITS as u64) * 8;

/// The largest object, header excluded: a `str`'s or `bytes`' data, a
/// list's or tuple's slots, a dict's or set's tables. Allocating a bigger
/// one fails with [`Error::OutOfMemory`] (`MemoryError`) whatever the
/// memory limit, so one operation on one object stays a bounded amount of
/// work (docs/spec.md §5, "Bounded native steps").
pub const MAX_OBJECT_BYTES: u64 = 64 << 20;

/// A non-borrowing word-space traversal, used with [`Heap::next_object`].
/// Start a new traversal with `default`; slot writes are allowed between
/// steps, but allocation and collection are not. The cursor owns no storage.
#[derive(Default)]
pub struct ObjectCursor {
    segment: usize,
    word: usize,
    pending: u64,
}

/// A reference to a word-space object: `segment << 17 | word`. Any `u32` can be
/// made into a `Ref`; heap methods reject the ones that are not live objects.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Ref(u32);

impl Ref {
    /// The reference with this raw encoding.
    #[must_use]
    pub const fn from_raw(raw: u32) -> Ref {
        Ref(raw)
    }
    /// The raw encoding.
    #[must_use]
    pub const fn raw(self) -> u32 {
        self.0
    }
    /// Index of the segment the object starts in.
    #[must_use]
    pub const fn segment(self) -> u32 {
        self.0 >> space::SEG_BITS
    }
}

/// A reference to a byte-space block: `segment << 17 | granule`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ByteRef(u32);

impl ByteRef {
    /// The reference with this raw encoding.
    #[must_use]
    pub const fn from_raw(raw: u32) -> ByteRef {
        ByteRef(raw)
    }
    /// The raw encoding.
    #[must_use]
    pub const fn raw(self) -> u32 {
        self.0
    }
    /// Index of the segment the block lives in.
    #[must_use]
    pub const fn segment(self) -> u32 {
        self.0 >> space::SEG_BITS
    }
}

/// Every heap error.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Error {
    /// The memory limit or the 32-bit address space is exhausted. The engine
    /// raises `MemoryError`.
    OutOfMemory,
    /// Not a live object or block.
    BadRef,
    /// Slot index out of range, or a value slot used as raw or vice versa.
    BadSlot,
    /// A reserved type id or flag, an object of the wrong type, an mm-core
    /// object written through an engine setter, or an unrepresentable layout.
    BadType,
    /// A snapshot is truncated or inconsistent.
    BadSnapshot,
}

/// Object type ids mm-core reserves. Engine types start at `FIRST_ENGINE`.
pub mod types {
    /// `str`: value slot 0 is the UTF-8 data's `ByteRef`; raw slot 1 the char
    /// count; raw slot 2 the hash (`hash_bytes` of the data) once the
    /// `HASHED` flag is set.
    pub const STR: u16 = 1;
    /// The intern table: slot 0 the entry count as a `SmallInt`, then a
    /// power-of-two array of entries (`UNDEFINED` or a `str` ref).
    pub const INTERN_TABLE: u16 = 2;
    /// The first type id `Heap::alloc` accepts.
    pub const FIRST_ENGINE: u16 = 16;
}

/// Header flag bits.
pub mod flags {
    /// The string is in the intern table (set by mm-core).
    pub const INTERNED: u8 = 1;
    /// The string is pure ASCII (set by mm-core).
    pub const ASCII: u8 = 2;
    /// Immutable (engine-defined).
    pub const FROZEN: u8 = 4;
    /// Has an instance dict (engine-defined).
    pub const HAS_DICT: u8 = 8;
    /// The string's hash slot is filled (set by mm-core).
    pub const HASHED: u8 = 16;
    /// Bits only mm-core sets; `Heap::alloc` and `Heap::set_flags` refuse them.
    pub const CORE: u8 = INTERNED | ASCII | HASHED;
}

/// An object header.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Header(u64);

const ALL_VALUES: u8 = 255;

impl Header {
    pub(crate) fn new(type_id: u16, flags: u8, nvals: u8, len: u32) -> Header {
        Header(
            u64::from(type_id)
                | u64::from(flags) << 16
                | u64::from(nvals) << 24
                | u64::from(len) << 32,
        )
    }
    pub(crate) fn from_word(w: u64) -> Header {
        Header(w)
    }
    /// The type id.
    #[must_use]
    pub fn type_id(self) -> u16 {
        (self.0 & 0xFFFF) as u16
    }
    /// The flag bits.
    #[must_use]
    pub fn flags(self) -> u8 {
        (self.0 >> 16 & 0xFF) as u8
    }
    /// Number of slots.
    #[must_use]
    pub fn len(self) -> u32 {
        header_len(self.0)
    }
    /// Does the object have no slots?
    #[must_use]
    pub fn is_empty(self) -> bool {
        self.len() == 0
    }
    /// Number of leading value slots.
    #[must_use]
    pub fn nvals(self) -> u32 {
        match (self.0 >> 24 & 0xFF) as u8 {
            ALL_VALUES => self.len(),
            n => u32::from(n),
        }
    }
    pub(crate) fn with_flags(self, flags: u8) -> Header {
        Header(self.0 & !(0xFF << 16) | u64::from(flags) << 16)
    }
    pub(crate) fn word(self) -> u64 {
        self.0
    }
}

/// Encodings an image holds that no declared table states, for the
/// snapshot build id (`micro_monty::build_id`, docs/spec.md §9.3): header
/// words, the segment size and string hashes (stored in `str` objects and
/// in dict and set tables).
#[must_use]
pub fn encoding_samples() -> [u64; 5] {
    [
        Header::new(types::STR, flags::ASCII | flags::HASHED, 1, 3).word(),
        Header::new(0x1234, 0x5a, ALL_VALUES, 0x0102_0304).word(),
        SEGMENT_BYTES,
        crate::hash_bytes(b""),
        crate::hash_bytes("micro-monty \u{e9}\u{1f600}".as_bytes()),
    ]
}

/// Heap settings.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct HeapConfig {
    /// Budget for arena data/metadata, the mark stack and external reservations.
    pub memory_limit: u64,
    /// `should_collect` fires once the bytes allocated since the last
    /// collection exceed this percentage of the bytes live after it.
    pub gc_ratio_percent: u32,
    /// Live-byte floor for that rule, so a tiny heap is not collected after
    /// every few allocations.
    pub gc_min_bytes: u64,
}

impl Default for HeapConfig {
    /// 256 MiB limit (the facade's default), k = 2, 1 MiB floor.
    fn default() -> Self {
        HeapConfig {
            memory_limit: 256 << 20,
            gc_ratio_percent: 200,
            gc_min_bytes: 1 << 20,
        }
    }
}

/// Collection statistics.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct GcStats {
    /// Collections run so far.
    pub collections: u64,
    /// Bytes in live objects and blocks after the last collection (headers
    /// included).
    pub live_bytes: u64,
    /// Bytes the last collection freed.
    pub freed_bytes: u64,
    /// Bytes freed by all collections.
    pub total_freed_bytes: u64,
    /// Bytes allocated since the last collection.
    pub allocated_since_gc: u64,
    /// Bytes committed now (both spaces, bitmaps included).
    pub committed_bytes: u64,
}

/// A session's heap. See the module docs for the layout.
pub struct Heap {
    pub(crate) words: Space<Vec<u64>>,
    pub(crate) bytes: Space<Vec<u8>>,
    pub(crate) config: HeapConfig,
    pub(crate) stats: GcStats,
    /// What collections the embedder forced (a snapshot's) took from
    /// `allocated_since_gc` since the engine's own last one
    /// ([`Heap::allocated_since_own_gc`]). In the image.
    pub(crate) kept_since_gc: u64,
    pub(crate) intern_table: Option<Ref>,
    pub(crate) mark_stack: Vec<u32>,
    /// Allocated word objects: an upper bound on the next mark stack.
    pub(crate) objects: usize,
    external_bytes: u64,
    /// Scratch charges, shared with their reservations.
    scratch: Arc<AtomicUsize>,
    /// A reservation was made: the counter's own size counts from then on.
    scratch_live: AtomicBool,
    /// The bytes of the last request refused for room ([`Heap::take_refused`];
    /// not snapshotted).
    refused: AtomicU64,
    /// Positions found in non-ASCII strings (not snapshotted).
    pub(crate) str_pos: crate::string::StrPos,
    /// The incremental mark in progress, if any (not snapshotted).
    pub(crate) inc: crate::gc::Inc,
}

/// An owned reservation for native scratch storage. Keep it with the buffer;
/// dropping it releases the charge without borrowing the heap. It does not
/// allocate the buffer or root any values stored there.
pub struct Scratch {
    counter: Arc<AtomicUsize>,
    bytes: usize,
}

impl Scratch {
    /// Bytes reserved.
    #[must_use]
    pub fn bytes(&self) -> usize {
        self.bytes
    }

    /// Change this reservation before growing a buffer, or after shrinking it.
    /// Failure preserves the charge. A different heap is `BadRef`.
    #[inline(never)]
    pub fn resize(&mut self, heap: &Heap, bytes: usize) -> Result<(), Error> {
        if !Arc::ptr_eq(&heap.scratch, &self.counter) {
            return Err(Error::BadRef);
        }
        if bytes > self.bytes {
            let extra = bytes - self.bytes;
            // Shared heap access fixes all non-scratch charges. Competing
            // reservations may grow or drop, so test and charge atomically.
            let ceiling = heap.scratch_budget();
            self.counter
                .try_update(Ordering::Relaxed, Ordering::Relaxed, |used| {
                    used.checked_add(extra).filter(|&n| n as u64 <= ceiling)
                })
                .map_err(|_| heap.refuse(extra as u64))?;
        } else if bytes < self.bytes {
            self.counter
                .fetch_sub(self.bytes - bytes, Ordering::Relaxed);
        }
        self.bytes = bytes;
        Ok(())
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        self.counter.fetch_sub(self.bytes, Ordering::Relaxed);
    }
}

impl Default for Heap {
    fn default() -> Self {
        Heap::new(HeapConfig::default())
    }
}

impl Heap {
    /// An empty heap. Commits nothing until the first allocation.
    #[must_use]
    pub fn new(config: HeapConfig) -> Heap {
        Heap {
            words: Space::new(),
            bytes: Space::new(),
            config,
            stats: GcStats::default(),
            kept_since_gc: 0,
            intern_table: None,
            mark_stack: Vec::new(),
            objects: 0,
            external_bytes: 0,
            scratch: Arc::new(AtomicUsize::new(0)),
            scratch_live: AtomicBool::new(false),
            refused: AtomicU64::new(0),
            str_pos: crate::string::StrPos::default(),
            inc: crate::gc::Inc::new(),
        }
    }

    // --- objects -------------------------------------------------------

    /// Allocates an object of engine type `type_id` with `nvals` value slots
    /// (all `Value::UNDEFINED`) followed by `nraw` raw slots (all 0).
    ///
    /// An object with raw slots can have at most 254 value slots.
    pub fn alloc(&mut self, type_id: u16, flags: u8, nvals: u32, nraw: u32) -> Result<Ref, Error> {
        check_engine(type_id, flags)?;
        self.alloc_object(type_id, flags, nvals, nraw)
    }

    /// Allocates an object of engine type `type_id` with these slots.
    pub fn alloc_init(
        &mut self,
        type_id: u16,
        flags: u8,
        values: &[Value],
        raws: &[u64],
    ) -> Result<Ref, Error> {
        check_engine(type_id, flags)?;
        let nvals = u32::try_from(values.len()).map_err(|_| Error::OutOfMemory)?;
        let nraw = u32::try_from(raws.len()).map_err(|_| Error::OutOfMemory)?;
        let (a, slots) = self.alloc_words(type_id, flags, nvals, nraw)?;
        if let Some((v, w)) = slots.split_at_mut_checked(values.len()) {
            for (slot, value) in v.iter_mut().zip(values) {
                *slot = value.0;
            }
            for (slot, raw) in w.iter_mut().zip(raws) {
                *slot = *raw;
            }
        }
        Ok(Ref(a))
    }

    /// Allocates any object, mm-core types included, with initialised slots.
    pub(crate) fn alloc_object(
        &mut self,
        type_id: u16,
        flags: u8,
        nvals: u32,
        nraw: u32,
    ) -> Result<Ref, Error> {
        let (a, slots) = self.alloc_words(type_id, flags, nvals, nraw)?;
        let fresh = Self::fresh_zeroed(slots.len() + 1);
        if let Some((v, w)) = slots.split_at_mut_checked(nvals as usize) {
            v.fill(Value::UNDEFINED.0);
            // A large block's raw slots are already 0 (`alloc_unfilled`).
            if !fresh {
                w.fill(0);
            }
        }
        Ok(Ref(a))
    }

    /// Allocates an object of engine type `type_id` with `nvals` value
    /// slots and no raw slots, for a caller that writes every slot before
    /// reading it, such as a list buffer or a tuple built in steps. A
    /// large block is not filled: it comes from a fresh zeroed segment, so
    /// its unwritten slots read as `0.0`, and its pages fault in as the
    /// caller writes them, a step at a time, not all in the allocating
    /// step (docs/spec.md §5, bounded native steps). A small one holds
    /// `UNDEFINED`, as from [`Heap::alloc`].
    pub fn alloc_unfilled(&mut self, type_id: u16, flags: u8, nvals: u32) -> Result<Ref, Error> {
        check_engine(type_id, flags)?;
        let (a, slots) = self.alloc_words(type_id, flags, nvals, 0)?;
        if !Self::fresh_zeroed(slots.len() + 1) {
            slots.fill(Value::UNDEFINED.0);
        }
        Ok(Ref(a))
    }

    /// Allocates an object's block and writes its header; returns its
    /// address and its slots, which hold stale words until the caller writes
    /// every one of them.
    pub(crate) fn alloc_words(
        &mut self,
        type_id: u16,
        flags: u8,
        nvals: u32,
        nraw: u32,
    ) -> Result<(u32, &mut [u64]), Error> {
        let len = nvals.checked_add(nraw).ok_or(Error::OutOfMemory)?;
        let code = if nraw == 0 {
            ALL_VALUES
        } else {
            u8::try_from(nvals)
                .ok()
                .filter(|&n| n < ALL_VALUES)
                .ok_or(Error::BadType)?
        };
        let units = usize::try_from(u64::from(len) + 1).map_err(|_| Error::OutOfMemory)?;
        let header = Header::new(type_id, flags, code, len);
        if self.objects == self.mark_stack.capacity() {
            self.reserve_marks(self.objects.saturating_mul(2).max(64))?;
        }
        let a = if let Some(a) = self.words.place_fast(units) {
            a
        } else {
            let avail = self.available_bytes();
            self.words
                .place_slow(units, avail)
                .map_err(|e| self.refuse_if(e, units as u64 * 8))?
        };
        let (data, off) = self.words.start(a, units, header.0)?;
        self.objects += 1;
        self.stats.allocated_since_gc += units as u64 * 8;
        let slots = data.get_mut(off + 1..off + units).ok_or(Error::BadSlot)?;
        Ok((a, slots))
    }

    /// The header of object `r`.
    pub fn header(&self, r: Ref) -> Result<Header, Error> {
        let (s, off) = self.words.block(r.0).ok_or(Error::BadRef)?;
        s.data.read(off).map(Header).ok_or(Error::BadRef)
    }

    /// Value slot `i` of object `r`.
    #[inline]
    pub fn get(&self, r: Ref, i: u32) -> Result<Value, Error> {
        let (s, off) = self.words.block(r.0).ok_or(Error::BadRef)?;
        let h = Header(s.data.read(off).ok_or(Error::BadRef)?);
        if i >= h.nvals() {
            return Err(Error::BadSlot);
        }
        s.data
            .read(off + 1 + i as usize)
            .map(Value)
            .ok_or(Error::BadSlot)
    }

    /// Raw slot `i` of object `r` (`i` counts from the first slot, so raw
    /// slots start at `nvals`).
    #[inline]
    pub fn get_raw(&self, r: Ref, i: u32) -> Result<u64, Error> {
        let (s, off) = self.words.block(r.0).ok_or(Error::BadRef)?;
        let h = Header(s.data.read(off).ok_or(Error::BadRef)?);
        if i < h.nvals() || i >= h.len() {
            return Err(Error::BadSlot);
        }
        s.data.read(off + 1 + i as usize).ok_or(Error::BadSlot)
    }

    /// Value slot `i` of `r` for a caller whose own guard (an inline
    /// cache's version check) already proves that `r` is a live object with
    /// value slot `i`. Only the segment bound is checked: were the guard
    /// wrong, this would read another arena word, never outside the arena.
    #[inline]
    pub fn get_guarded(&self, r: Ref, i: u32) -> Value {
        let (seg, off) = space::split(r.0);
        self.words
            .segs
            .get(seg)
            .and_then(|s| s.data.read(off + 1 + i as usize))
            .map_or(Value::UNDEFINED, Value)
    }

    /// Raw slot `i` of `r` (counted as in [`Heap::get_raw`]) under the
    /// [`Heap::get_guarded`] contract; 0 past the segment.
    #[inline]
    #[must_use]
    pub fn get_raw_guarded(&self, r: Ref, i: u32) -> u64 {
        let (seg, off) = space::split(r.0);
        self.words
            .segs
            .get(seg)
            .and_then(|s| s.data.read(off + 1 + i as usize))
            .unwrap_or(0)
    }

    /// Sets value slot `i` of `r` (write barrier) under the same guard
    /// contract as [`Heap::get_guarded`].
    #[inline]
    pub fn set_guarded(&mut self, r: Ref, i: u32, v: Value) {
        let (seg, off) = space::split(r.0);
        if let Some(s) = self.words.segs.get_mut(seg)
            && let Some(slot) = s.data.get_mut(off + 1 + i as usize)
        {
            *slot = v.0;
            s.dirty = space::DIRTY_REFS;
        }
    }

    /// Sets raw slot `i` of `r` (counted as in [`Heap::get_raw`]; write
    /// barrier) under the [`Heap::get_guarded`] contract. Raw slots are
    /// never traced, so a wrong guard cannot forge a reference.
    #[inline]
    pub fn set_raw_guarded(&mut self, r: Ref, i: u32, w: u64) {
        let (seg, off) = space::split(r.0);
        if let Some(s) = self.words.segs.get_mut(seg)
            && let Some(slot) = s.data.get_mut(off + 1 + i as usize)
        {
            *slot = w;
        }
    }

    /// Sets value slot `i + 1` of `r` to `v` (write barrier) if slot `i` is
    /// not `UNDEFINED` (a live dict entry's key), with one access for both,
    /// under the [`Heap::get_guarded`] contract. Whether it wrote.
    #[inline]
    pub fn set_guarded_live(&mut self, r: Ref, i: u32, v: Value) -> bool {
        let (seg, off) = space::split(r.0);
        let at = off + 1 + i as usize;
        if let Some(s) = self.words.segs.get_mut(seg)
            && let Some([k, slot]) = s.data.get_mut(at..at + 2)
            && *k != Value::UNDEFINED.0
        {
            *slot = v.0;
            s.dirty = space::DIRTY_REFS;
            return true;
        }
        false
    }

    /// Every slot of object `r` as words (value slots as `Value` bits), for
    /// reading several with one validation.
    pub fn slot_words(&self, r: Ref) -> Result<&[u64], Error> {
        let (s, off) = self.words.block(r.0).ok_or(Error::BadRef)?;
        let h = Header(s.data.read(off).ok_or(Error::BadRef)?);
        s.data
            .get(off + 1..off + 1 + h.len() as usize)
            .ok_or(Error::BadSlot)
    }

    /// The header and every slot of object `r` as words, validated once.
    #[inline]
    pub fn object_words(&self, r: Ref) -> Result<(Header, &[u64]), Error> {
        let (s, off) = self.words.block(r.0).ok_or(Error::BadRef)?;
        let h = Header(s.data.read(off).ok_or(Error::BadRef)?);
        let w = s
            .data
            .get(off + 1..off + 1 + h.len() as usize)
            .ok_or(Error::BadSlot)?;
        Ok((h, w))
    }

    /// Copies value slots `i..i + out.len()` of object `r` into `out`,
    /// validated once.
    pub fn read_values(&self, r: Ref, i: u32, out: &mut [Value]) -> Result<(), Error> {
        let (s, off) = self.words.block(r.0).ok_or(Error::BadRef)?;
        let h = Header(s.data.read(off).ok_or(Error::BadRef)?);
        let end = (i as usize).checked_add(out.len()).ok_or(Error::BadSlot)?;
        if end > h.nvals() as usize {
            return Err(Error::BadSlot);
        }
        let src = s
            .data
            .get(off + 1 + i as usize..off + 1 + end)
            .ok_or(Error::BadSlot)?;
        for (o, &w) in out.iter_mut().zip(src) {
            *o = Value(w);
        }
        Ok(())
    }

    /// Sets value slots `i..i + vs.len()` of engine object `r` (write
    /// barrier), validated once.
    pub fn set_values(&mut self, r: Ref, i: u32, vs: &[Value]) -> Result<(), Error> {
        let (s, off) = self.words.block_mut(r.0).ok_or(Error::BadRef)?;
        let h = Header(s.data.read(off).ok_or(Error::BadRef)?);
        if h.type_id() < types::FIRST_ENGINE {
            return Err(Error::BadType);
        }
        let end = (i as usize).checked_add(vs.len()).ok_or(Error::BadSlot)?;
        if end > h.nvals() as usize {
            return Err(Error::BadSlot);
        }
        let slots = s
            .data
            .get_mut(off + 1 + i as usize..off + 1 + end)
            .ok_or(Error::BadSlot)?;
        for (slot, v) in slots.iter_mut().zip(vs) {
            *slot = v.0;
        }
        s.dirty = space::DIRTY_REFS;
        Ok(())
    }

    /// Sets value slots `from..to` of engine object `r` to `UNDEFINED`,
    /// validated once: a table's keys marked empty a slice at a time. No
    /// write barrier: the slots then reach nothing, so a mark need not
    /// rescan the segment for them.
    pub fn clear_values(&mut self, r: Ref, from: u32, to: u32) -> Result<(), Error> {
        let (s, off) = self.words.block_mut(r.0).ok_or(Error::BadRef)?;
        let h = Header(s.data.read(off).ok_or(Error::BadRef)?);
        if h.type_id() < types::FIRST_ENGINE {
            return Err(Error::BadType);
        }
        if from > to || to > h.nvals() {
            return Err(Error::BadSlot);
        }
        let slots = s
            .data
            .get_mut(off + 1 + from as usize..off + 1 + to as usize)
            .ok_or(Error::BadSlot)?;
        slots.fill(Value::UNDEFINED.0);
        Ok(())
    }

    /// Moves value slots `from..from + n` of engine object `r` to `to..to +
    /// n` in place (overlap allowed, as `memmove`), validated once (write
    /// barrier): a list's items shifting for `insert` or `pop`.
    pub fn move_values(&mut self, r: Ref, from: u32, to: u32, n: u32) -> Result<(), Error> {
        let (s, off) = self.words.block_mut(r.0).ok_or(Error::BadRef)?;
        let h = Header(s.data.read(off).ok_or(Error::BadRef)?);
        if h.type_id() < types::FIRST_ENGINE {
            return Err(Error::BadType);
        }
        if from
            .max(to)
            .checked_add(n)
            .is_none_or(|end| end > h.nvals())
        {
            return Err(Error::BadSlot);
        }
        let base = off + 1;
        let slots = s
            .data
            .get_mut(base..base + h.nvals() as usize)
            .ok_or(Error::BadSlot)?;
        let (from, to, n) = (from as usize, to as usize, n as usize);
        if to < from && from - to < 8 {
            // A short move down (`pop(0)`, `del l[0]`) goes as memmoves of
            // at most 2 KiB, below glibc's `rep movsb` threshold. On
            // GitHub's runners one long move ran 16 times slower than the
            // same move up [M], likely an overlapping `rep movsb` this
            // close, which glibc avoids only on Intel [E]. Ascending chunks
            // only overwrite slots already read.
            for at in (0..n).step_by(256) {
                let len = (n - at).min(256);
                slots.copy_within(from + at..from + at + len, to + at);
            }
        } else {
            slots.copy_within(from..from + n, to);
        }
        s.dirty = space::DIRTY_REFS;
        Ok(())
    }

    /// The raw slots of object `r` (slots `nvals..len`) as one slice,
    /// validated once, for loops over many of them.
    pub fn raw_words(&self, r: Ref) -> Result<&[u64], Error> {
        let (s, off) = self.words.block(r.0).ok_or(Error::BadRef)?;
        let h = Header(s.data.read(off).ok_or(Error::BadRef)?);
        s.data
            .get(off + 1 + h.nvals() as usize..off + 1 + h.len() as usize)
            .ok_or(Error::BadSlot)
    }

    /// Mutable form of [`Heap::raw_words`] for engine objects (write
    /// barrier: the segment is marked dirty). Raw slots are never traced, so
    /// no write through it can forge a reference.
    pub fn raw_words_mut(&mut self, r: Ref) -> Result<&mut [u64], Error> {
        let (s, off) = self.words.block_mut(r.0).ok_or(Error::BadRef)?;
        let h = Header(s.data.read(off).ok_or(Error::BadRef)?);
        if h.type_id() < types::FIRST_ENGINE {
            return Err(Error::BadType);
        }
        s.data
            .get_mut(off + 1 + h.nvals() as usize..off + 1 + h.len() as usize)
            .ok_or(Error::BadSlot)
    }

    /// Sets value slot `i` of engine object `r` (write barrier).
    #[inline]
    pub fn set(&mut self, r: Ref, i: u32, v: Value) -> Result<(), Error> {
        self.write_slot(r, i, v.0, true, true)
    }

    /// Sets raw slot `i` of engine object `r` (write barrier).
    #[inline]
    pub fn set_raw(&mut self, r: Ref, i: u32, w: u64) -> Result<(), Error> {
        self.write_slot(r, i, w, false, true)
    }

    /// Sets the engine flag bits of engine object `r`; mm-core's bits
    /// (`flags::CORE`) are kept.
    pub fn set_flags(&mut self, r: Ref, new: u8) -> Result<(), Error> {
        if new & flags::CORE != 0 {
            return Err(Error::BadType);
        }
        let (s, off) = self.words.block_mut(r.0).ok_or(Error::BadRef)?;
        let h = Header(s.data.read(off).ok_or(Error::BadRef)?);
        if h.type_id() < types::FIRST_ENGINE {
            return Err(Error::BadType);
        }
        s.data
            .write(off, h.with_flags(h.flags() & flags::CORE | new).0);
        Ok(())
    }

    /// Sets a core flag bit on any object (mm-core internal).
    pub(crate) fn add_core_flag(&mut self, r: Ref, flag: u8) -> Result<(), Error> {
        let (s, off) = self.words.block_mut(r.0).ok_or(Error::BadRef)?;
        let h = Header(s.data.read(off).ok_or(Error::BadRef)?);
        s.data.write(off, h.with_flags(h.flags() | flag).0);
        Ok(())
    }

    /// The one slot writer: checks the slot kind and, for engine setters, that
    /// the object is not an mm-core object; marks the segment dirty.
    #[inline]
    pub(crate) fn write_slot(
        &mut self,
        r: Ref,
        i: u32,
        word: u64,
        value: bool,
        engine: bool,
    ) -> Result<(), Error> {
        let (s, off) = self.words.block_mut(r.0).ok_or(Error::BadRef)?;
        let header = Header(s.data.read(off).ok_or(Error::BadRef)?);
        if engine && header.type_id() < types::FIRST_ENGINE {
            return Err(Error::BadType);
        }
        let ok = if value {
            i < header.nvals()
        } else {
            i >= header.nvals() && i < header.len()
        };
        let slot = s.data.get_mut(off + 1 + i as usize).filter(|_| ok);
        let slot = slot.ok_or(Error::BadSlot)?;
        *slot = word;
        if value {
            s.dirty = space::DIRTY_REFS;
        }
        Ok(())
    }

    /// Calls `f` with every word-space object and its header, in address
    /// order (for checks over a restored heap).
    pub fn for_each_object(&self, f: &mut dyn FnMut(Ref, Header)) {
        for (idx, s) in self.words.segs.iter().enumerate() {
            for (w, &bits) in s.starts.iter().enumerate() {
                let mut b = bits;
                while b != 0 {
                    let off = w * 64 + b.trailing_zeros() as usize;
                    b &= b - 1;
                    if let Some(h) = s.data.read(off) {
                        f(Ref(space::addr_of(idx, off)), Header(h));
                    }
                }
            }
        }
    }

    /// The next word-space object in address order. Start with a default
    /// cursor. Slot writes between calls are allowed; do not
    /// allocate or collect during a traversal. This lets callers rewrite
    /// objects without first collecting their references in a native buffer.
    #[must_use]
    #[inline]
    pub fn next_object(&self, cursor: &mut ObjectCursor) -> Option<(Ref, Header)> {
        while let Some(s) = self.words.segs.get(cursor.segment) {
            if cursor.pending != 0 {
                let off = (cursor.word - 1) * 64 + cursor.pending.trailing_zeros() as usize;
                cursor.pending &= cursor.pending - 1;
                if let Some(h) = s.data.read(off) {
                    return Some((Ref(space::addr_of(cursor.segment, off)), Header(h)));
                }
            } else if let Some(&bits) = s.starts.get(cursor.word) {
                cursor.pending = bits;
                cursor.word += 1;
            } else {
                cursor.segment += 1;
                cursor.word = 0;
            }
        }
        None
    }

    // --- byte blocks ---------------------------------------------------

    /// Allocates a byte block holding a copy of `data`.
    pub fn alloc_bytes(&mut self, data: &[u8]) -> Result<ByteRef, Error> {
        self.alloc_block(data.len(), Some(data))
    }

    /// Allocates a zero-filled byte block of `len` bytes.
    pub fn alloc_bytes_zeroed(&mut self, len: usize) -> Result<ByteRef, Error> {
        self.alloc_block(len, None)
    }

    /// Allocates a block of `len` bytes holding `data` (zeros if `None`),
    /// padding zeroed.
    fn alloc_block(&mut self, len: usize, data: Option<&[u8]>) -> Result<ByteRef, Error> {
        let len32 = u32::try_from(len).map_err(|_| Error::OutOfMemory)?;
        let units = <Vec<u8> as Store>::units_for(len32);
        let a = if let Some(a) = self.bytes.place_fast(units) {
            a
        } else {
            let avail = self.available_bytes();
            self.bytes
                .place_slow(units, avail)
                .map_err(|e| self.refuse_if(e, units as u64 * 8))?
        };
        let (store, off) = self.bytes.start(a, units, 1 | u64::from(len32) << 32)?;
        self.stats.allocated_since_gc += units as u64 * 8;
        if let Some(body) = store.get_mut((off + 1) * 8..(off + units) * 8) {
            match data {
                Some(src) if src.len() == len => {
                    // Zero the last unit (the padding) as one word, then
                    // copy: no separate fill of the 0..7 padding bytes.
                    let n = body.len();
                    if let Some(last) = body.get_mut(n.saturating_sub(8)..) {
                        last.fill(0);
                    }
                    if let Some(d) = body.get_mut(..len) {
                        d.copy_from_slice(src);
                    }
                }
                _ if Self::fresh_zeroed(units) => {}
                _ => body.fill(0),
            }
        }
        Ok(ByteRef(a))
    }

    /// Whether a new byte block of `units` units is already all zeros: a
    /// block of `LARGE_UNITS` or more is always placed in a dedicated
    /// segment of its own, freshly allocated zeroed (`Space::new_large`;
    /// never a reused one), so filling it again would only fault in every
    /// page at once (docs/spec.md §5, bounded native steps). Reuse this
    /// check, not the size test, wherever a block's zero fill is skipped.
    #[inline]
    fn fresh_zeroed(units: usize) -> bool {
        units >= LARGE_UNITS
    }

    /// Appends `data` to byte block `b`. A block with a dedicated segment
    /// grows in place, its room set to `reserve` bytes (at least what it
    /// then holds; less gives the rest back); any other is copied into a
    /// new block (`b` is left to the collector), given that room once it
    /// is large (no data and a small block: `b` as it is). Growth is admitted against the memory limit before it is
    /// written, and nothing is filled first: a text written over several
    /// native steps grows this way.
    pub fn append_bytes(
        &mut self,
        b: ByteRef,
        data: &[u8],
        reserve: usize,
    ) -> Result<ByteRef, Error> {
        let len = self.bytes(b)?.len();
        let large = |n: usize| {
            u32::try_from(n).map_or(true, |n| <Vec<u8> as Store>::units_for(n) >= LARGE_UNITS)
        };
        let (b, data) = if large(len) {
            (b, data)
        } else if data.is_empty() {
            return Ok(b);
        } else {
            let end = len.checked_add(data.len()).ok_or(Error::OutOfMemory)?;
            let n = self.alloc_bytes_zeroed(end)?;
            self.copy_bytes(b, 0, n, 0, len)?;
            if let Some(d) = self.bytes_mut(n)?.get_mut(len..) {
                d.copy_from_slice(data);
            }
            if !large(end) {
                return Ok(n);
            }
            (n, &[][..])
        };
        let avail = self.available_bytes();
        self.bytes.append_large(b.0, data, reserve, avail)?;
        self.stats.allocated_since_gc += data.len() as u64;
        Ok(b)
    }

    /// The data of byte block `b`.
    pub fn bytes(&self, b: ByteRef) -> Result<&[u8], Error> {
        let (s, off) = self.bytes.block(b.0).ok_or(Error::BadRef)?;
        let len = header_len(s.data.read(off).ok_or(Error::BadRef)?) as usize;
        let start = (off + 1) * 8;
        s.data.get(start..start + len).ok_or(Error::BadRef)
    }

    /// Mutable data of byte block `b`. Handing out the slice is the write
    /// barrier: the segment is marked dirty.
    pub fn bytes_mut(&mut self, b: ByteRef) -> Result<&mut [u8], Error> {
        let (s, off) = self.bytes.block_mut(b.0).ok_or(Error::BadRef)?;
        let len = header_len(s.data.read(off).ok_or(Error::BadRef)?) as usize;
        let start = (off + 1) * 8;
        s.data.get_mut(start..start + len).ok_or(Error::BadRef)
    }

    /// Copies `len` bytes of block `src` from offset `from` into block
    /// `dst` at offset `to`; the blocks may be the same and the ranges may
    /// overlap. Like [`Heap::bytes_mut`], it marks `dst`'s segment dirty.
    /// Both ranges are checked before anything is written.
    pub fn copy_bytes(
        &mut self,
        src: ByteRef,
        from: usize,
        dst: ByteRef,
        to: usize,
        len: usize,
    ) -> Result<(), Error> {
        let at = |space: &Space<Vec<u8>>, b: ByteRef, off: usize| {
            let (s, unit) = space.block(b.0)?;
            let n = header_len(s.data.read(unit)?) as usize;
            (off.checked_add(len)? <= n).then(|| (unit + 1) * 8 + off)
        };
        let a = at(&self.bytes, src, from).ok_or(Error::BadRef)?;
        let b = at(&self.bytes, dst, to).ok_or(Error::BadRef)?;
        let (si, di) = (src.segment() as usize, dst.segment() as usize);
        let segs = &mut self.bytes.segs;
        if si == di {
            let s = segs.get_mut(di).ok_or(Error::BadRef)?;
            if s.data.len() < a.max(b) + len {
                return Err(Error::BadRef);
            }
            s.data.copy_within(a..a + len, b);
            return Ok(());
        }
        let (lo, hi) = segs.split_at_mut(si.max(di));
        let (s, d) = if si < di {
            (lo.get(si), hi.first_mut())
        } else {
            (hi.first(), lo.get_mut(di))
        };
        let (Some(s), Some(d)) = (s, d) else {
            return Err(Error::BadRef);
        };
        let (Some(from), Some(into)) = (s.data.get(a..a + len), d.data.get_mut(b..b + len)) else {
            return Err(Error::BadRef);
        };
        into.copy_from_slice(from);
        Ok(())
    }

    // --- memory limit --------------------------------------------------

    /// Bytes committed by both spaces, segment bitmaps included.
    #[must_use]
    pub fn committed_bytes(&self) -> u64 {
        self.words.committed + self.bytes.committed
    }

    /// The memory limit in bytes.
    #[must_use]
    pub fn memory_limit(&self) -> u64 {
        self.config.memory_limit
    }

    /// Changes the memory limit without freeing existing allocations.
    /// Growth that needs additional budget fails until enough is released.
    pub fn set_memory_limit(&mut self, bytes: u64) {
        self.config.memory_limit = bytes;
    }

    /// Reserves part of the same limit for allocations owned by the engine
    /// outside the arena. A failed increase leaves the reservation unchanged.
    /// This is not serialized: the engine rebuilds it when restoring its data.
    pub fn set_external_bytes(&mut self, bytes: u64) -> Result<(), Error> {
        if bytes > self.external_bytes && bytes - self.external_bytes > self.available_bytes() {
            return Err(self.refuse(bytes - self.external_bytes));
        }
        self.external_bytes = bytes;
        Ok(())
    }

    /// Reserves native buffer capacity independently of retained engine tables.
    /// The shared counter counts too, from the first reservation on. Shared
    /// heap access suffices, so reservations may be made and grow while
    /// borrowing the heap; owners may release them even after the heap dies.
    /// Scratch is not serialized; snapshot callers own any live reservations.
    #[inline(never)]
    pub fn reserve_scratch(&self, bytes: usize) -> Result<Scratch, Error> {
        if !self.scratch_live.load(Ordering::Relaxed) {
            let needed = (bytes as u64).saturating_add((3 * size_of::<usize>()) as u64);
            if needed > self.available_bytes() {
                return Err(self.refuse(needed));
            }
            self.scratch_live.store(true, Ordering::Relaxed);
        }
        let counter = Arc::clone(&self.scratch);
        let mut reservation = Scratch { counter, bytes: 0 };
        reservation.resize(self, bytes)?;
        Ok(reservation)
    }

    /// [`Heap::refuse`] if a placement failed for room.
    #[cold]
    #[inline(never)]
    fn refuse_if(&self, e: Error, bytes: u64) -> Error {
        if e == Error::OutOfMemory {
            self.refuse(bytes);
        }
        e
    }

    /// Records a request of `bytes` refused for room: the last one is what
    /// [`Heap::take_refused`] gives.
    #[cold]
    #[inline(never)]
    fn refuse(&self, bytes: u64) -> Error {
        self.refused.store(bytes.max(1), Ordering::Relaxed);
        Error::OutOfMemory
    }

    /// Whether a request was refused for room since the last
    /// [`Heap::take_refused`], without taking it.
    #[must_use]
    pub fn refusal_pending(&self) -> bool {
        self.refused.load(Ordering::Relaxed) != 0
    }

    /// The bytes of the last request refused for room since the last call
    /// (0: none), so a caller can tell a refusal a collection might help
    /// (a request no larger than the limit) from one it cannot (larger, or
    /// a cap refused before asking the heap).
    pub fn take_refused(&self) -> u64 {
        self.refused.swap(0, Ordering::Relaxed)
    }

    /// Unreserved budget after arena, collector and external allocations.
    /// Excludes reusable free blocks inside already committed segments.
    #[must_use]
    #[inline(never)]
    pub fn available_bytes(&self) -> u64 {
        self.scratch_budget()
            .saturating_sub(self.scratch.load(Ordering::Relaxed) as u64)
    }

    /// The ceiling for all scratch owners together, excluding their charges.
    fn scratch_budget(&self) -> u64 {
        self.config
            .memory_limit
            .saturating_sub(self.committed_bytes())
            .saturating_sub(self.external_bytes)
            .saturating_sub(self.mark_stack.capacity() as u64 * 4)
            .saturating_sub(self.words.metadata_bytes())
            .saturating_sub(self.bytes.metadata_bytes())
            .saturating_sub(if self.scratch_live.load(Ordering::Relaxed) {
                (3 * size_of::<usize>()) as u64
            } else {
                0
            })
    }

    /// Reserve before admitting objects, not when GC needs to reclaim them.
    pub(crate) fn reserve_marks(&mut self, capacity: usize) -> Result<(), Error> {
        if capacity == 0 {
            return Ok(());
        }
        // Use the same capacity classes after restore. Exact-sized restored
        // stacks would double on their first new object, past the original cap.
        let capacity = capacity
            .checked_next_power_of_two()
            .ok_or(Error::OutOfMemory)?;
        let extra = capacity.saturating_sub(self.mark_stack.capacity()) as u64 * 4;
        if extra > self.available_bytes() {
            return Err(self.refuse(extra));
        }
        self.mark_stack
            .try_reserve_exact(capacity)
            .map_err(|_| Error::OutOfMemory)
    }

    /// Preflight for an allocation of `bytes` (object slots or block data,
    /// header excluded). `false` means it would certainly fail with
    /// `OutOfMemory`. `true` may still fail if free space is too fragmented
    /// or admitting another object needs more collector/segment metadata.
    #[must_use]
    pub fn would_fit(&self, bytes: u64) -> bool {
        let fits = self.fits(bytes);
        if !fits {
            self.refuse(bytes);
        }
        fits
    }

    fn fits(&self, bytes: u64) -> bool {
        let Some(total) = bytes
            .div_ceil(8)
            .checked_add(1)
            .and_then(|u| u.checked_mul(8))
        else {
            return false;
        };
        if total >= LARGE_OBJECT_BYTES {
            return total
                .checked_add(16)
                .is_some_and(|total| total <= self.available_bytes());
        }
        let units = usize::try_from(total / 8).unwrap_or(usize::MAX);
        let fresh = self
            .words
            .fresh_cost(units)
            .min(self.bytes.fresh_cost(units));
        self.available_bytes() >= fresh
            || total <= self.words.free_bytes().max(self.bytes.free_bytes())
    }

    // --- lazy sweep ----------------------------------------------------

    /// Does the last collection's lazy sweep have segments left?
    #[must_use]
    pub fn sweeping(&self) -> bool {
        !self.words.pending.is_empty() || !self.bytes.pending.is_empty()
    }

    /// Sweeps up to `k` pending segments of each space.
    pub(crate) fn sweep_step(&mut self, k: usize) {
        self.words.sweep_some(k);
        self.bytes.sweep_some(k);
        self.take_lazily_freed();
    }

    /// Lazy sweeps freed blocks the last collection counted live.
    #[cold]
    fn take_lazily_freed(&mut self) {
        let units = core::mem::take(&mut self.words.lazily_freed)
            + core::mem::take(&mut self.bytes.lazily_freed);
        self.stats.live_bytes = self.stats.live_bytes.saturating_sub(units * 8);
    }

    /// Sweeps what the last collection left to allocation, so that every
    /// block with a start bit is live: before walking objects or writing a
    /// snapshot.
    pub fn finish_sweep(&mut self) {
        if !self.sweeping() {
            return;
        }
        self.words.finish_sweep();
        self.bytes.finish_sweep();
        self.take_lazily_freed();
    }
}

fn check_engine(type_id: u16, flags: u8) -> Result<(), Error> {
    if type_id < types::FIRST_ENGINE || flags & flags::CORE != 0 {
        Err(Error::BadType)
    } else {
        Ok(())
    }
}
