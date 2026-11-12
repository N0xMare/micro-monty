//! Precise, non-moving mark-sweep (docs/spec.md §9.2), run only when the
//! engine calls `Heap::collect` at a safepoint.
//!
//! Marking is iterative: an explicit stack of word addresses, each object
//! pushed at most once (it is marked when pushed), so deep graphs cannot
//! overflow the Rust stack. Before admitting each object, the heap reserves
//! room for its `u32` stack entry within the memory budget. Collection never
//! needs to grow the stack; excess capacity is released afterward. Marks live
//! in the side bitmaps, so marking writes no object memory. The intern table
//! is weak: only strings reachable from ordinary roots survive collection.
//! [`Heap::collect_weak`] lets the engine keep weak tables of its own.
//!
//! Over [`MIN_LIVE`] live bytes a due collection is incremental
//! ([`Heap::collect_step`], docs/spec.md §9.2): marking runs a slice per
//! safepoint, so a park can follow any slice, with no write barrier beyond
//! the setters' segment flags. The mutator runs between slices; its writes
//! to value slots set [`crate::space::DIRTY_REFS`] on their segment, and
//! the marked objects of such segments are rescanned (mostly-parallel
//! marking, Boehm, Demers and Shenker 1991) until a pass finds few, then
//! a remark of the roots and the remaining dirty segments ends the mark in
//! one step. Objects allocated during a mark are unmarked; anything that
//! reaches one is a root or a written slot, so the remark finds it. A mark
//! with too little room left to let the mutator go on allocating, or one
//! that does not converge (writes spread over the heap), is held
//! ([`Heap::holding`]): it goes on a slice per safepoint with no mutator
//! code between slices, so a park can still follow any slice. The sweep
//! that follows is lazy (`Space::pending`).

use crate::heap::{GcStats, Header, Heap, Ref};
use crate::space::{Seg, Space, Store, split};
use crate::value::{HI_BYTES, HI_REF, Value};
use alloc::{boxed::Box, vec::Vec};

type Words = Space<Vec<u64>>;
type Bytes = Space<Vec<u8>>;

/// Hands roots to a collection. Everything reachable from a root survives.
pub struct Tracer<'a> {
    words: &'a mut Words,
    bytes: &'a mut Bytes,
    stack: &'a mut Vec<u32>,
}

impl Tracer<'_> {
    /// A root value. Non-references and invalid references are ignored.
    pub fn value(&mut self, v: Value) {
        mark_value(
            &self.words.segs,
            &mut self.words.marks,
            &self.bytes.segs,
            &mut self.bytes.marks,
            self.stack,
            v.0,
        );
    }

    /// Is `v` marked so far (it survives)? Non-references are.
    #[must_use]
    pub fn marked(&self, v: Value) -> bool {
        #[allow(
            clippy::cast_possible_truncation,
            reason = "low 32 bits are the reference"
        )]
        let (seg, off) = split(v.0 as u32);
        let marked =
            |marks: &[Box<[u64]>]| marks.get(seg).is_some_and(|m| crate::space::bit(m, off));
        match v.0 >> 48 {
            HI_REF => marked(&self.words.marks),
            HI_BYTES => marked(&self.bytes.marks),
            _ => true,
        }
    }

    /// Several root values.
    pub fn values(&mut self, vs: &[Value]) {
        for &v in vs {
            self.value(v);
        }
    }

    /// Keeps object `r` alive but traces only its first `n` value slots, as
    /// for a frame stack whose slots above the top are dead. If `r` is also
    /// reachable as an ordinary value, whichever is seen first wins.
    pub fn object_prefix(&mut self, r: Ref, n: u32) {
        if !try_mark(&self.words.segs, &mut self.words.marks, r.raw()) {
            return;
        }
        let (seg, off) = split(r.raw());
        let Some(s) = self.words.segs.get(seg) else {
            return;
        };
        let Some(header) = s.data.read(off) else {
            return;
        };
        let n = Header::nvals_of(header).min(n) as usize;
        if let Some(slots) = s.data.get(off + 1..off + 1 + n) {
            for &w in slots {
                mark_value(
                    &self.words.segs,
                    &mut self.words.marks,
                    &self.bytes.segs,
                    &mut self.bytes.marks,
                    self.stack,
                    w,
                );
            }
        }
    }
}

/// Sets the mark bit of the block starting at `a`; `false` if it is not a
/// block start or was already marked.
#[inline]
fn try_mark<S>(segs: &[Seg<S>], marks: &mut [Box<[u64]>], a: u32) -> bool {
    let (seg, off) = split(a);
    let Some(s) = segs.get(seg) else {
        return false;
    };
    if !crate::space::bit(&s.starts, off) {
        return false;
    }
    let Some(w) = marks.get_mut(seg).and_then(|m| m.get_mut(off >> 6)) else {
        return false;
    };
    let bit = 1 << (off & 63);
    if *w & bit != 0 {
        return false;
    }
    *w |= bit;
    true
}

#[allow(
    clippy::inline_always,
    reason = "wasm32 keeps the incremental mark loop as calls otherwise, 2x slower [M]"
)]
#[inline(always)]
fn mark_value(
    wsegs: &[Seg<Vec<u64>>],
    wmarks: &mut [Box<[u64]>],
    bsegs: &[Seg<Vec<u8>>],
    bmarks: &mut [Box<[u64]>],
    stack: &mut Vec<u32>,
    w: u64,
) {
    #[allow(
        clippy::cast_possible_truncation,
        reason = "low 32 bits are the reference"
    )]
    let a = w as u32;
    match w >> 48 {
        HI_REF => {
            if try_mark(wsegs, wmarks, a) {
                stack.push(a);
            }
        }
        HI_BYTES => {
            try_mark(bsegs, bmarks, a);
        }
        _ => {}
    }
}

/// Scans marked objects until the stack is empty.
fn drain(words: &mut Words, bytes: &mut Bytes, stack: &mut Vec<u32>) {
    while let Some(a) = stack.pop() {
        let (seg, off) = split(a);
        let Some(s) = words.segs.get(seg) else {
            continue;
        };
        let Some(header) = s.data.read(off) else {
            continue;
        };
        let n = Header::nvals_of(header) as usize;
        let Some(slots) = s.data.get(off + 1..off + 1 + n) else {
            continue;
        };
        for &w in slots {
            mark_value(
                &words.segs,
                &mut words.marks,
                &bytes.segs,
                &mut bytes.marks,
                stack,
                w,
            );
        }
    }
}

/// Like [`drain`], but stops once `budget` units of work (an object and
/// its value slots) are done. Returns the budget left: 0 if it stopped
/// early (the stack may still hold work).
#[allow(
    clippy::inline_always,
    reason = "wasm32 keeps the incremental mark loop as calls otherwise, 2x slower [M]"
)]
#[inline(always)]
fn drain_some(
    words: &mut Words,
    bytes: &mut Bytes,
    stack: &mut Vec<u32>,
    partial: &mut Vec<(u32, u32)>,
    mut budget: u64,
    chunk: usize,
) -> u64 {
    while budget > 0 {
        let (a, from) = if let Some(p) = partial.pop() {
            p
        } else if let Some(a) = stack.pop() {
            (a, 0)
        } else {
            return budget;
        };
        budget =
            budget.saturating_sub(1 + scan(words, bytes, stack, partial, a, from as usize, chunk));
    }
    0
}

/// Value slots one incremental scan covers: a longer object (a list's
/// item array) is scanned in chunks, the rest kept in `partial` (64 under
/// incremental stress).
const CHUNK: usize = 1 << 15;

/// Marks the value slots of object `a` from slot `from`, at most
/// [`CHUNK`] of them, leaving the rest in `partial`; the number scanned.
#[allow(
    clippy::inline_always,
    reason = "wasm32 keeps the incremental mark loop as calls otherwise, 2x slower [M]"
)]
#[inline(always)]
fn scan(
    words: &mut Words,
    bytes: &mut Bytes,
    stack: &mut Vec<u32>,
    partial: &mut Vec<(u32, u32)>,
    a: u32,
    from: usize,
    chunk: usize,
) -> u64 {
    let (seg, off) = split(a);
    let Some(s) = words.segs.get(seg) else {
        return 0;
    };
    let Some(header) = s.data.read(off) else {
        return 0;
    };
    let n = Header::nvals_of(header) as usize;
    if from >= n {
        return 0;
    }
    let mut end = n;
    #[allow(
        clippy::cast_possible_truncation,
        reason = "slot counts are below 2^32"
    )]
    if n - from > chunk && partial.try_reserve(1).is_ok() {
        end = from + chunk;
        partial.push((a, end as u32));
    }
    let n = end - from;
    let Some(slots) = s.data.get(off + 1 + from..off + 1 + end) else {
        return 0;
    };
    for &w in slots {
        mark_value(
            &words.segs,
            &mut words.marks,
            &bytes.segs,
            &mut bytes.marks,
            stack,
            w,
        );
    }
    n as u64
}

/// Rescans every marked object of word segment `idx` (written since it was
/// scanned); the work done.
fn rescan(
    words: &mut Words,
    bytes: &mut Bytes,
    stack: &mut Vec<u32>,
    partial: &mut Vec<(u32, u32)>,
    idx: usize,
    chunk: usize,
) -> u64 {
    let nwords = words.marks.get(idx).map_or(0, |m| m.len());
    let mut work = 0;
    for w in 0..nwords {
        let (Some(s), Some(&m)) = (
            words.segs.get(idx),
            words.marks.get(idx).and_then(|m| m.get(w)),
        ) else {
            break;
        };
        let mut bits = m & s.starts.get(w).copied().unwrap_or(0);
        while bits != 0 {
            let off = w * 64 + bits.trailing_zeros() as usize;
            bits &= bits - 1;
            #[allow(
                clippy::cast_possible_truncation,
                reason = "segment indices are below 2^15"
            )]
            let a = ((idx as u32) << crate::space::SEG_BITS) | off as u32;
            work += 1 + scan(words, bytes, stack, partial, a, 0, chunk);
        }
    }
    work
}

/// Clears a segment's [`crate::space::DIRTY_REFS`]; whether it was set.
fn take_refs(dirty: &mut u8) -> bool {
    let was = *dirty & crate::space::DIRTY_REFS != 0;
    *dirty &= !crate::space::DIRTY_REFS;
    was
}

/// The state of an incremental mark (see the module docs). Mark bits and
/// the stack persist between slices; nothing is freed until the remark.
/// None of it is snapshotted: a snapshot abandons the mark for a full
/// collection, and a restored heap starts with none.
#[derive(Default)]
pub(crate) struct Inc {
    /// Whether a mark is in progress, and whether it is held.
    mark: Mark,
    /// The next word segment the current rescan pass looks at.
    cursor: usize,
    /// Rescan passes done.
    passes: u32,
    /// Dirty segments the current pass found, and the last pass.
    found: u32,
    prev: u32,
    /// Work units of this mark before its first rescan, and of its rescans.
    first: u64,
    rescanned: u64,
    /// `allocated_since_gc` when this mark began, and at its last step.
    began: u64,
    paced: u64,
    /// A collection was asked for (`Heap::force_collection`): one is due
    /// until the next mark starts or a full collection runs.
    forced: bool,
    /// Due collections to hold from their start before letting the
    /// mutator run during marks again, and how many the next mark that
    /// does not converge sets.
    backoff: u32,
    penalty: u32,
    /// Work units per slice; 0 disables incremental marking.
    pub(crate) slice: u64,
    /// Live bytes below which a collection runs at once.
    pub(crate) min_live: u64,
    /// Check every remark against a full mark (stress testing).
    pub(crate) verify: bool,
    /// Objects being scanned in chunks, with the next slot of each.
    partial: Vec<(u32, u32)>,
}

/// The state of the incremental mark.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Mark {
    /// No mark is in progress.
    #[default]
    Idle,
    /// A mark is in progress; the guest runs between its steps.
    Running,
    /// The guest waits for this mark to end ([`Heap::holding`]): it had too
    /// little room left to let the guest go on allocating, or marks were
    /// not converging. It goes on a slice per step with no guest code
    /// between steps, so nothing is written meanwhile.
    Held,
}

impl Inc {
    /// Slices of [`SLICE`] work units, for heaps of [`MIN_LIVE`] live or
    /// more.
    pub(crate) fn new() -> Inc {
        Inc {
            slice: SLICE,
            min_live: MIN_LIVE,
            ..Inc::default()
        }
    }
}

/// Work units (an object, or a value slot) per incremental step: 1.5–5 ms
/// native, up to 16 ms on wasm [M].
const SLICE: u64 = 1 << 18;
/// Live bytes from which a due collection is incremental. Below it a
/// collection runs in full and is due once live and allocated bytes reach
/// twice it, so a full collection marks at most about 64 MiB: 30 ms native
/// at worst for a heap growing through it (48 MiB: up to 51 ms) [M]. No
/// bench program gets there.
pub(crate) const MIN_LIVE: u64 = 32 << 20;
/// A mark whose rescans exceed this share of its first mark (percent) is
/// not converging (writes spread over an old heap; allocation, appends and
/// in-order writes rescan at most 2% [M]): it is held from then on, so
/// such a loop costs about what full collections cost instead of 2x, in
/// slices.
const GIVE_UP_PERCENT: u64 = 25;
/// Due collections held from their start after a first mark that did not
/// converge.
const FIRST_BACKOFF: u32 = 4;
/// Most due collections held after consecutive marks that did not converge.
const MAX_BACKOFF: u32 = 16;

/// A rescan pass that found at most this many dirty segments ends the mark:
/// the remark rescans what was dirtied since, in one step.
const REMARK_SEGS: u32 = 8;
/// Passes after which the mark ends whatever the passes found.
const MAX_PASSES: u32 = 8;
/// Pending segments of each space one step sweeps (1 MiB each, about
/// 0.1 ms [M]).
const SWEEP_SLICE: usize = 8;

impl Heap {
    /// Is an incremental mark in progress?
    #[must_use]
    pub fn marking(&self) -> bool {
        self.inc.mark != Mark::Idle
    }

    /// Must the guest wait for the mark in progress? Its next step is due
    /// at once, before any guest code runs; a park may come between steps.
    #[must_use]
    pub fn holding(&self) -> bool {
        self.inc.mark == Mark::Held
    }

    /// `slice` work units (an object plus its value slots) per incremental
    /// step, 0 to collect at once; heaps with fewer than `min_live` live
    /// bytes are collected at once (tests and stress testing).
    pub fn set_incremental(&mut self, slice: u64, min_live: u64) {
        self.inc.slice = slice;
        self.inc.min_live = min_live;
    }

    /// Does a collection due now run incrementally (or is one in
    /// progress)?
    #[must_use]
    pub fn incremental(&self) -> bool {
        self.inc.mark != Mark::Idle || (self.inc.slice != 0 && self.live_now() >= self.inc.min_live)
    }

    /// Makes a collection due that starts after this call: the next one,
    /// or the one after a mark in progress (whose marks predate the call).
    /// Over 32 MiB live it marks incrementally, a step per safepoint.
    pub fn force_collection(&mut self) {
        self.inc.forced = true;
    }

    /// Stress testing: checks every remark against a full mark from the
    /// same roots; a reachable object the incremental mark missed aborts.
    pub fn set_incremental_verify(&mut self, on: bool) {
        self.inc.verify = on;
    }

    /// Every object a full mark from `roots` reaches must be marked.
    fn verify_mark(&mut self, roots: &mut dyn FnMut(&mut Tracer<'_>)) {
        let saved = self.words.marks.clone();
        let saved_bytes = self.bytes.marks.clone();
        for m in self
            .words
            .marks
            .iter_mut()
            .chain(self.bytes.marks.iter_mut())
        {
            m.fill(0);
        }
        self.mark_roots(roots, true);
        let mut stack = core::mem::take(&mut self.mark_stack);
        drain(&mut self.words, &mut self.bytes, &mut stack);
        self.mark_stack = stack;
        for (full, inc) in self
            .words
            .marks
            .iter()
            .zip(&saved)
            .chain(self.bytes.marks.iter().zip(&saved_bytes))
        {
            assert!(
                !full.iter().zip(inc.iter()).any(|(f, i)| f & !i != 0),
                "incremental mark missed a reachable object"
            );
        }
        self.words.marks = saved;
        self.bytes.marks = saved_bytes;
    }

    /// Room to allocate without a collection: uncommitted budget plus
    /// listed free blocks.
    fn mark_room(&self) -> u64 {
        self.available_bytes()
            .saturating_add(self.words.free_bytes())
            .saturating_add(self.bytes.free_bytes())
    }

    /// Drops a mark in progress (a full collection follows).
    fn abandon_mark(&mut self) {
        if self.inc.mark == Mark::Idle {
            return;
        }
        self.inc.mark = Mark::Idle;
        self.mark_stack.clear();
        self.inc.partial.clear();
        for m in self
            .words
            .marks
            .iter_mut()
            .chain(self.bytes.marks.iter_mut())
        {
            m.fill(0);
        }
    }

    /// One step of a collection, at a safepoint where one is due, one is in
    /// progress or the last one's sweep has segments left. The first step
    /// marks the roots; later ones mark up to a slice of work, then rescan
    /// segments dirtied since, with the roots, until a pass finds few; the
    /// last one remarks the roots and the dirty segments and sweeps. A
    /// small heap is collected in full in this step. A mark with little
    /// room left, one after marks that did not converge, and one that does
    /// not converge are held ([`Heap::holding`]). Returns the statistics
    /// once a collection completed.
    #[allow(
        clippy::too_many_lines,
        reason = "one state machine; its phases share the stack and budget"
    )]
    pub fn collect_step(
        &mut self,
        roots: &mut dyn FnMut(&mut Tracer<'_>),
        weak: &mut dyn FnMut(&mut Tracer<'_>),
    ) -> Option<GcStats> {
        let limit = self.config.memory_limit;
        if self.inc.mark == Mark::Idle {
            // Called for a lazy sweep with no collection due: sweep a slice.
            if self.sweeping() && !self.should_collect() && !self.inc.verify {
                self.sweep_step(SWEEP_SLICE);
                return None;
            }
            if self.inc.slice == 0 || self.live_now() < self.inc.min_live {
                return Some(self.collect_weak(roots, weak));
            }
            // Nothing pending may be swept while marks are being set: the
            // last collection's sweep goes on first, a slice at a time.
            if self.sweeping() {
                self.sweep_step(SWEEP_SLICE);
                return None;
            }
            if !self.should_collect() && !self.inc.verify {
                return None;
            }
            // Nothing is freed until the remark: with little room left to
            // allocate meanwhile, and after marks that did not converge
            // (the guest's writes outpaced them), the guest waits for this
            // mark, which goes on a slice per step.
            let held = self.mark_room() < limit / 64
                || (!self.inc.verify && self.mark_room() < self.stats.allocated_since_gc / 2)
                || (self.inc.backoff > 0 && !self.inc.verify);
            self.inc.backoff = self.inc.backoff.saturating_sub(1);
            for s in &mut self.words.segs {
                s.dirty &= !crate::space::DIRTY_REFS;
            }
            self.inc.mark = if held { Mark::Held } else { Mark::Running };
            self.inc.cursor = 1;
            self.inc.passes = 0;
            self.inc.found = 0;
            self.inc.prev = u32::MAX;
            self.inc.first = 0;
            self.inc.rescanned = 0;
            self.inc.began = self.stats.allocated_since_gc;
            self.inc.paced = self.inc.began;
            self.inc.forced = false;
            self.mark_roots(roots, true);
            return None;
        }
        // Pacing: the mark holds its garbage until the remark, and natives
        // cannot collect, so once the room left is no more than what was
        // allocated since the mark began, it finishes in this step.
        // Otherwise a step marks a slice plus a unit per word allocated
        // since the last one, so the mark keeps up with natives that
        // allocate a lot in one step.
        let now = self.stats.allocated_since_gc;
        let during = now - self.inc.began;
        if self.mark_room() < limit / 128 || (!self.inc.verify && self.mark_room() <= during) {
            self.inc.mark = Mark::Held;
        }
        let mut budget = if self.inc.mark == Mark::Held || self.inc.verify {
            self.inc.slice
        } else {
            self.inc.slice + (now - self.inc.paced) / 8
        };
        self.inc.paced = now;
        // A held mark never gives up: nothing writes while it goes on, and
        // a full collection would mark everything again, in one step.
        let held = self.inc.mark == Mark::Held;
        let chunk = if self.inc.verify { 64 } else { CHUNK };
        let mut stack = core::mem::take(&mut self.mark_stack);
        let mut partial = core::mem::take(&mut self.inc.partial);
        let done = loop {
            let before = budget;
            budget = drain_some(
                &mut self.words,
                &mut self.bytes,
                &mut stack,
                &mut partial,
                budget,
                chunk,
            );
            if self.inc.passes == 0 && self.inc.found == 0 {
                self.inc.first += before - budget;
            } else {
                self.inc.rescanned += before - budget;
            }
            if budget == 0 {
                break Some(false);
            }
            let idx = self.inc.cursor;
            if let Some(s) = self.words.segs.get_mut(idx) {
                self.inc.cursor += 1;
                if take_refs(&mut s.dirty) {
                    self.inc.found += 1;
                    let work = rescan(
                        &mut self.words,
                        &mut self.bytes,
                        &mut stack,
                        &mut partial,
                        idx,
                        chunk,
                    );
                    budget = budget.saturating_sub(work);
                    self.inc.rescanned += work;
                    if self.inc.rescanned * 100 > self.inc.first * GIVE_UP_PERCENT
                        && !self.inc.verify
                        && !held
                    {
                        break None;
                    }
                }
                continue;
            }
            // A pass that did not halve the dirty segments will not
            // converge (writes spread over the heap). Stress testing checks
            // every mark it can, so it gives up on none.
            if self.inc.passes > 0
                && self.inc.found > self.inc.prev / 2
                && !self.inc.verify
                && !held
            {
                break None;
            }
            // A held mark ends after a pass that found nothing dirty, so its
            // remark only marks the roots again.
            let ends = if held {
                self.inc.found == 0
            } else {
                self.inc.found <= REMARK_SEGS || self.inc.passes >= MAX_PASSES
            };
            if ends {
                break Some(true);
            }
            self.inc.passes += 1;
            self.inc.prev = self.inc.found;
            self.inc.found = 0;
            self.inc.cursor = 1;
            self.mark_stack = stack;
            self.mark_roots(roots, false);
            stack = core::mem::take(&mut self.mark_stack);
        };
        self.mark_stack = stack;
        self.inc.partial = partial;
        let Some(done) = done else {
            // Not converging: the guest waits for this mark from now on,
            // and for the next few from their start, doubling while marks
            // keep failing.
            self.inc.backoff = self.inc.penalty.max(FIRST_BACKOFF);
            self.inc.penalty = (self.inc.backoff * 2).min(MAX_BACKOFF);
            self.inc.mark = Mark::Held;
            return None;
        };
        if !done {
            return None;
        }
        self.inc.penalty = 0;
        // Remark: the roots, the segments dirtied since their last rescan,
        // and everything they reach, in this step.
        let chunk = if self.inc.verify { 64 } else { CHUNK };
        self.mark_roots(roots, true);
        let mut stack = core::mem::take(&mut self.mark_stack);
        let mut partial = core::mem::take(&mut self.inc.partial);
        for idx in 1..self.words.segs.len() {
            if self
                .words
                .segs
                .get_mut(idx)
                .is_some_and(|s| take_refs(&mut s.dirty))
            {
                rescan(
                    &mut self.words,
                    &mut self.bytes,
                    &mut stack,
                    &mut partial,
                    idx,
                    chunk,
                );
            }
        }
        drain_some(
            &mut self.words,
            &mut self.bytes,
            &mut stack,
            &mut partial,
            u64::MAX,
            chunk,
        );
        self.mark_stack = stack;
        self.inc.partial = partial;
        self.inc.mark = Mark::Idle;
        if self.inc.verify {
            self.verify_mark(roots);
        }
        Some(self.finish_collection(weak))
    }

    /// Collects garbage. `roots` reports every root through the `Tracer`; the
    /// intern table itself is retained, but its strings need ordinary roots.
    /// Returns the updated statistics.
    pub fn collect(&mut self, roots: &mut dyn FnMut(&mut Tracer<'_>)) -> GcStats {
        self.collect_weak(roots, &mut |_| {})
    }

    /// [`Heap::collect`], and once everything reachable from `roots` is
    /// marked, `weak` runs: for tables whose entries must not keep their
    /// objects alive. It asks [`Tracer::marked`] which survive, must drop
    /// the dead ones, and may mark more values.
    pub fn collect_weak(
        &mut self,
        roots: &mut dyn FnMut(&mut Tracer<'_>),
        weak: &mut dyn FnMut(&mut Tracer<'_>),
    ) -> GcStats {
        self.abandon_mark();
        self.inc.forced = false;
        self.words.clear_pending_marks();
        self.bytes.clear_pending_marks();
        self.mark_roots(roots, true);
        let mut stack = core::mem::take(&mut self.mark_stack);
        drain(&mut self.words, &mut self.bytes, &mut stack);
        self.mark_stack = stack;
        self.finish_collection(weak)
    }

    /// Marks the roots (and the intern table itself, if `table`) and pushes
    /// what they newly mark.
    fn mark_roots(&mut self, roots: &mut dyn FnMut(&mut Tracer<'_>), table: bool) {
        let mut stack = core::mem::take(&mut self.mark_stack);
        {
            let mut t = Tracer {
                words: &mut self.words,
                bytes: &mut self.bytes,
                stack: &mut stack,
            };
            if table && let Some(table) = self.intern_table {
                t.object_prefix(table, 0);
            }
            roots(&mut t);
        }
        self.mark_stack = stack;
    }

    /// The end of every collection, once everything strongly reachable is
    /// marked: the weak pass, then the sweep.
    fn finish_collection(&mut self, weak: &mut dyn FnMut(&mut Tracer<'_>)) -> GcStats {
        // Sweep may reuse the address of a string with a position.
        self.str_pos.clear();
        let mut stack = core::mem::take(&mut self.mark_stack);
        {
            let mut t = Tracer {
                words: &mut self.words,
                bytes: &mut self.bytes,
                stack: &mut stack,
            };
            weak(&mut t);
        }
        drain(&mut self.words, &mut self.bytes, &mut stack);
        // Remove weak entries before their addresses can be reused by sweep.
        let _ = self.prune_intern();
        self.objects = self
            .words
            .marks
            .iter()
            .flatten()
            .map(|m| m.count_ones() as usize)
            .sum();
        stack.clear();
        let keep = self
            .objects
            .max(1024)
            .checked_next_power_of_two()
            .unwrap_or(stack.capacity());
        if stack.capacity() > keep {
            stack.shrink_to(keep);
        }
        self.mark_stack = stack;

        // Sweep lazily only where marks are incremental: a sweep must not
        // fill one step there, and smaller heaps sweep as before.
        let lazy = self.inc.slice != 0 && self.live_now() >= self.inc.min_live;
        let w = self.words.sweep(lazy);
        let b = self.bytes.sweep(lazy);
        self.words.lazily_freed = 0;
        self.bytes.lazily_freed = 0;
        let freed = (w.freed_units + b.freed_units) * 8;
        self.stats.collections += 1;
        self.stats.live_bytes = (w.live_units + b.live_units) * 8;
        self.stats.freed_bytes = freed;
        self.stats.total_freed_bytes += freed;
        self.stats.allocated_since_gc = 0;
        self.kept_since_gc = 0;
        self.stats()
    }

    /// Should the engine collect at its next safepoint? True once the bytes
    /// allocated since the last collection exceed `gc_ratio_percent` of the
    /// live bytes after it (floored at `gc_min_bytes`), or under memory
    /// pressure: at least 1/64 of the limit allocated since the last
    /// collection, and less than 1/8 of the limit left uncommitted with at
    /// least half as much allocated since then, or a space with less than
    /// 1/8 of the limit free that could not commit its next segment (up to
    /// 1 MiB, more than 1/8 of a limit below 8 MiB). The half keeps a nearly
    /// full heap, whose garbage is reused from the free lists, from being
    /// collected every 1/64 of the limit, each time for a full mark of the
    /// live heap, while half of the uncommitted budget stays for natives,
    /// which cannot collect.
    #[must_use]
    pub fn should_collect(&self) -> bool {
        if self.inc.forced {
            return true;
        }
        let since = u128::from(self.stats.allocated_since_gc);
        let base = u128::from(self.live_now().max(self.config.gc_min_bytes));
        if since * 100 > base * u128::from(self.config.gc_ratio_percent) {
            return true;
        }
        let limit = self.config.memory_limit;
        if self.stats.allocated_since_gc < limit / 64 {
            return false;
        }
        // Under `min_live` a collection runs in full: one is due before the
        // heap could reach twice that, not three times (`gc_ratio_percent`).
        let live = self.live_now();
        if self.inc.slice != 0
            && live < self.inc.min_live
            && live.saturating_add(self.stats.allocated_since_gc)
                >= self.inc.min_live.saturating_mul(2)
        {
            return true;
        }
        // Over it a collection marks incrementally and holds its garbage
        // until the remark, so one is due while the room left is still at
        // least the bytes allocated since the last one.
        if self.inc.slice != 0
            && live >= self.inc.min_live
            && self.mark_room() <= self.stats.allocated_since_gc
        {
            return true;
        }
        let available = self.available_bytes();
        (available < limit / 8 && self.stats.allocated_since_gc >= available / 2)
            || (available < self.words.fresh_cost(1) && self.words.free_bytes() < limit / 8)
            || (available < self.bytes.fresh_cost(1) && self.bytes.free_bytes() < limit / 8)
    }

    /// Counts object `r`, which the caller has just unlinked, as allocated
    /// since the last collection. A list buffer replaced at the end of a
    /// rebuild in steps was still in use if a collection ran during the
    /// rebuild, so without this the garbage it leaves would not show in
    /// [`GcStats::allocated_since_gc`].
    pub fn count_dropped(&mut self, r: Ref) {
        if let Ok(h) = self.header(r) {
            self.stats.allocated_since_gc += (u64::from(h.len()) + 1) * 8;
        }
    }

    /// After a collection the embedder forced (a snapshot's): `bytes`
    /// ([`Heap::allocated_since_own_gc`] before it) still count for the
    /// engine's rules that ask what was allocated since its own last
    /// collection, so they do not depend on whether, or where, the
    /// embedder took one. The trigger (`allocated_since_gc`,
    /// [`Heap::should_collect`]) does not see them: that collection freed
    /// what they left (keeping them there made every safepoint after a
    /// restore collect, and a yielding collection park again at once).
    pub fn keep_allocated_since_gc(&mut self, bytes: u64) {
        self.kept_since_gc = bytes;
    }

    /// Bytes allocated since the engine's own last collection, counting
    /// those before a snapshot's ([`Heap::keep_allocated_since_gc`]).
    #[must_use]
    pub fn allocated_since_own_gc(&self) -> u64 {
        self.stats
            .allocated_since_gc
            .saturating_add(self.kept_since_gc)
    }

    /// Current statistics.
    #[must_use]
    pub fn stats(&self) -> GcStats {
        GcStats {
            committed_bytes: self.committed_bytes(),
            live_bytes: self.live_now(),
            ..self.stats
        }
    }

    /// Live bytes after the last collection, less what lazy sweeps have
    /// since found dead in segments it counted live.
    pub(crate) fn live_now(&self) -> u64 {
        let freed = self.words.lazily_freed + self.bytes.lazily_freed;
        self.stats.live_bytes.saturating_sub(freed * 8)
    }
}

impl Header {
    /// Value-slot count of a raw header word.
    #[inline]
    pub(crate) fn nvals_of(h: u64) -> u32 {
        Header::from_word(h).nvals()
    }
}
