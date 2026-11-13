//! `dict`: a compact, insertion-ordered hash table in the arena, and the
//! Python hash and key equality it uses.
//!
//! A dict has one of three layouts; the first value slot tells them apart
//! (a `BUF` is never a key):
//!
//! ```text
//! inline   values [k0, v0, .., k(n-1), v(n-1)]            raws [counts]
//! linear   values [entries: BUF of 2*8, NONE, unused..]   raws [counts]
//! indexed  values [entries: BUF of 2*cap, index: RAW, unused..]
//!                                                         raws [counts]
//! counts   len | used << 32
//! index    words 0..cap: 2*cap index slots of 32 bits, two per word (slot i
//!          in word i/2, bits 32*(i%2)..); 0 = empty, u32::MAX = deleted,
//!          else a tag of hash bits | (entry + 1) in the low EBITS bits
//!          (EBITS = log2(cap) + 1);
//!          words cap..2*cap: the full hash of each entry (resizes never
//!          call `__hash__` again); word 2*cap: the version (bumped on every
//!          insert or delete; globals inline caches check it)
//! ```
//! Small dicts (at most `LINEAR_MAX` entries, every key an int, float, bool
//! or str) have no index: lookups scan the entries, comparing the probe hash
//! with each key's hash, which these types compute natively and without
//! side effects (a str caches its own), so `__eq__` runs exactly when an
//! indexed table would run it. A dict literal stores up to `LINEAR_MAX`
//! entries inline in the dict object; a fuller dict, or any other key, moves
//! the entries out (inline slots are cleared, the object never moves).
//! In an indexed table `cap` is a power of two (at least 8) and the index is
//! at most half full. Deleted entries keep their slot with key `UNDEFINED`
//! until a resize compacts the table, so iteration order is insertion order.
//! Globals dicts are indexed from the start (`new_dict_indexed`).

use crate::class::Dn;
use crate::types::{T, ty};
use crate::vm::{Raised, Vm};
use alloc::vec::Vec;
use mm_core::{Ref, Value};

pub(crate) const DELETED: u32 = u32::MAX;
/// Most entries a dict holds without an index.
const LINEAR_MAX: usize = 8;
/// Inline entry room of a new empty dict.
const EMPTY_ROOM: usize = 4;
const MODULUS: u64 = (1 << 61) - 1;
/// Why a comparison could not be decided natively.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Hard {
    /// Nesting is too deep (`RecursionError`).
    Deep,
    /// Python code must decide: a user `__eq__` must run, or the walk spent
    /// its [`WALK`] budget.
    Py,
}

/// Pairs one native comparison walk (`==`, ordering, a key probe) visits
/// before it hands off to the prelude, which goes on one level at a time
/// in bytecode, where fuel and parking apply (docs/spec.md §5). A
/// constant, so a parked and an unparked run take the same path.
pub(crate) const WALK: u32 = 1 << 14;

/// The budget of a native walk a prelude twin starts at nesting `depth`
/// of a comparison it took over: halved per level, down to one visit.
/// A walk that runs out hands that level's items to the twin, which
/// descends one level and retries them with the next budget, so a deep
/// comparison spends about 2 × [`WALK`] plus one visit per level on
/// re-walking what the twin then does in bytecode, instead of a full
/// [`WALK`] at every level. A higher floor costs that floor at every level
/// of a chain longer than it (deep frozensets under shared data ran 2–25×
/// slower with floors of 64–512 [M]).
pub(crate) fn walk_budget(depth: u32) -> u32 {
    (WALK >> depth.min(31)).max(1)
}

/// Deepest tuple nesting `hash` and key equality follow.
pub(crate) const MAX_DEPTH: u32 = 200;

/// CPython's hash of an `i64`.
#[must_use]
pub fn hash_int(i: i64) -> i64 {
    let m = MODULUS.cast_signed();
    let h = if i >= 0 {
        i % m
    } else {
        -((-(i128::from(i)) % i128::from(m)) as i64)
    };
    if h == -1 { -2 } else { h }
}

/// The Python hash bits of a `str` whose `hash_bytes` is `h` (`-1` is
/// reserved, as in CPython).
#[inline]
fn str_key_hash(h: u64) -> u64 {
    if h == u64::MAX {
        (-2i64).cast_unsigned()
    } else {
        h
    }
}

/// `frexp` for finite non-zero `x`: `x = m * 2^e`, `0.5 <= |m| < 1`.
fn frexp(x: f64) -> (f64, i32) {
    let bits = x.to_bits();
    let exp = ((bits >> 52) & 0x7FF).cast_signed() as i32;
    if exp == 0 {
        // Subnormal: scale up first.
        let (m, e) = frexp(x * f64::from_bits(0x4350_0000_0000_0000));
        return (m, e - 54);
    }
    let m = f64::from_bits((bits & !(0x7FF << 52)) | (1022 << 52));
    (m, exp - 1022)
}

/// CPython's hash of a float.
#[must_use]
pub fn hash_float(v: f64) -> i64 {
    if v.is_nan() {
        return 0;
    }
    if v.is_infinite() {
        return if v > 0.0 { 314_159 } else { -314_159 };
    }
    if v == 0.0 {
        return 0;
    }
    let (mut m, mut e) = frexp(v);
    let mut sign: i64 = 1;
    if m < 0.0 {
        sign = -1;
        m = -m;
    }
    let mut x: u64 = 0;
    while m != 0.0 {
        x = ((x << 28) & MODULUS) | x >> (61 - 28);
        m *= 268_435_456.0;
        e -= 28;
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "0 <= m < 2^28"
        )]
        let y = m as u64;
        #[allow(clippy::cast_precision_loss, reason = "y < 2^28")]
        {
            m -= y as f64;
        }
        x += y;
        if x >= MODULUS {
            x -= MODULUS;
        }
    }
    let e = if e >= 0 {
        e % 61
    } else {
        61 - 1 - ((-1 - e) % 61)
    };
    let e = e.cast_unsigned();
    x = ((x << e) & MODULUS) | x >> (61 - e);
    let h = x.cast_signed().wrapping_mul(sign);
    if h == -1 { -2 } else { h }
}

/// Index geometry of a dict with room for `cap` entries: `2 * cap` slots.
#[derive(Clone, Copy)]
pub(crate) struct Geo {
    cap: u32,
    mask: u32,
    /// The entry bits of a slot; the others hold the tag.
    pub(crate) emask: u32,
}

impl Geo {
    pub(crate) fn new(cap: u32) -> Geo {
        let cap = cap.max(1);
        let ebits = cap.trailing_zeros() + 1;
        #[allow(clippy::cast_possible_truncation, reason = "ebits <= 32")]
        let emask = ((1u64 << ebits) - 1) as u32;
        Geo {
            cap,
            mask: 2 * cap - 1,
            emask,
        }
    }

    /// The first slot of `h`'s probe sequence.
    #[allow(clippy::cast_possible_truncation, reason = "the two halves of h")]
    pub(crate) fn start(self, h: u64) -> u32 {
        (h as u32 ^ (h >> 32) as u32) & self.mask
    }

    /// The slot after `i` on a probe sequence, CPython's `5i + 1 + perturb`
    /// (with `perturb` starting at the hash): consecutive hashes such as
    /// small ints still get their own slots, but a probe that passes one
    /// leaves the run at once instead of walking it to its end.
    #[allow(clippy::cast_possible_truncation, reason = "masked to the table")]
    pub(crate) fn next(self, i: u32, perturb: &mut u64) -> u32 {
        *perturb >>= 5;
        i.wrapping_mul(5)
            .wrapping_add(1)
            .wrapping_add(*perturb as u32)
            & self.mask
    }

    /// Probe steps that visit every slot: `perturb` is zero after at most
    /// 13 steps, and from then on `5i + 1` cycles through all of them.
    pub(crate) fn steps(self) -> u32 {
        self.mask.saturating_add(14)
    }

    /// The tag bits of `h`.
    #[allow(clippy::cast_possible_truncation, reason = "high half of h")]
    pub(crate) fn tag(self, h: u64) -> u32 {
        (h >> 32) as u32 & !self.emask
    }

    /// Slot word for entry `e` of hash `h`.
    fn word(self, h: u64, e: u32) -> u32 {
        self.tag(h) | (e + 1)
    }
}

/// Whether every indexed dict in `heap` has a consistent index (a restored
/// image is checked once): a power-of-two table, counts within it, and
/// slots that each name a different entry of at most `used` whose hash
/// they are tagged with, every live entry found on its own probe sequence.
/// Lookups rely on all of it (an entry of 0 would underflow).
pub(crate) fn check_indexes(heap: &mm_core::Heap) -> bool {
    let mut ok = true;
    let mut seen: Vec<bool> = Vec::new();
    heap.for_each_object(&mut |r, h| {
        if !ok || h.type_id() != ty::DICT {
            return;
        }
        let Ok((_, w)) = heap.object_words(r) else {
            ok = false;
            return;
        };
        let vals = &w[..(h.nvals() as usize).min(w.len())];
        let (Some(&c), Some(entries), Some(index)) = (
            w.last(),
            vals.first().and_then(|&b| ref_of_kind(b, ty::BUF)),
            vals.get(1).and_then(|&b| ref_of_kind(b, ty::RAW)),
        ) else {
            return;
        };
        #[allow(clippy::cast_possible_truncation, reason = "the two halves")]
        let (len, used) = (c as u32 as usize, (c >> 32) as usize);
        let (Ok(words), Ok(kvs)) = (heap.raw_words(index), heap.slot_words(entries)) else {
            ok = false;
            return;
        };
        let cap = words.len() / 2;
        let Ok(cap32) = u32::try_from(cap) else {
            ok = false;
            return;
        };
        if !cap.is_power_of_two()
            || words.len() != 2 * cap + 1
            || used > cap
            || kvs.len() < 2 * used
        {
            ok = false;
            return;
        }
        let g = Geo::new(cap32);
        let hash = |e: usize| words[cap + e];
        seen.clear();
        seen.resize(used, false);
        for i in 0..2 * cap32 {
            let w = islot(words, i);
            if w == 0 || w == DELETED {
                continue;
            }
            let e = (w & g.emask) as usize;
            if e == 0 || e > used || seen[e - 1] || w & !g.emask != g.tag(hash(e - 1)) {
                ok = false;
                return;
            }
            seen[e - 1] = true;
        }
        let undefined = Value::UNDEFINED.to_bits();
        let mut live = 0;
        for e in 0..used {
            if kvs[2 * e] == undefined {
                continue;
            }
            live += 1;
            // The probe a lookup of this key takes reaches its slot.
            let h = hash(e);
            let mut i = g.start(h);
            let mut perturb = h;
            let mut found = false;
            for _ in 0..=g.steps() {
                let w = islot(words, i);
                if w == 0 {
                    break;
                }
                if w != DELETED && (w & g.emask) as usize == e + 1 {
                    found = true;
                    break;
                }
                i = g.next(i, &mut perturb);
            }
            if !found {
                ok = false;
                return;
            }
        }
        ok = live == len;
    });
    ok
}

/// Index slot `i`.
#[inline]
pub(crate) fn islot(words: &[u64], i: u32) -> u32 {
    #[allow(clippy::cast_possible_truncation, reason = "one 32-bit half")]
    words
        .get((i / 2) as usize)
        .map_or(0, |w| (w >> ((i & 1) * 32)) as u32)
}

/// Sets index slot `i`.
#[inline]
fn set_islot(words: &mut [u64], i: u32, v: u32) {
    if let Some(w) = words.get_mut((i / 2) as usize) {
        let sh = (i & 1) * 32;
        *w = (*w & !(0xFFFF_FFFF << sh)) | u64::from(v) << sh;
    }
}

/// The first empty or deleted slot on `h`'s probe sequence and the slots
/// probed past its first; `None` if there is none (a forged image's full
/// table).
fn free_in(words: &[u64], g: Geo, h: u64) -> Option<(u32, u64)> {
    let mut i = g.start(h);
    let mut perturb = h;
    for n in 0..=g.steps() {
        let w = islot(words, i);
        if w == 0 || w == DELETED {
            return Some((i, u64::from(n)));
        }
        i = g.next(i, &mut perturb);
    }
    None
}

/// A dict's storage and counters, read with one validation.
#[derive(Clone, Copy)]
pub(crate) struct Dv {
    /// The object whose value slots hold the (key, value) pairs from slot 0:
    /// the dict itself (inline) or its entries `BUF`.
    pub(crate) entries: Ref,
    /// The index of an indexed table.
    pub(crate) index: Option<Ref>,
    /// Entry room of an inline table, 0 when the entries are out of line.
    inline: u32,
    len: u32,
    pub(crate) used: u32,
    /// The dict is some code's globals (`GLOBALS_FLAG`).
    globals: bool,
}

/// The `Ref` in `bits` if they are a reference of kind `kind`.
#[inline]
fn ref_of_kind(bits: u64, kind: u16) -> Option<Ref> {
    #[allow(clippy::cast_possible_truncation, reason = "the low 32 bits")]
    let r = Ref::from_raw(bits as u32);
    #[allow(clippy::cast_possible_truncation, reason = "engine kinds fit a byte")]
    let v = Value::from_ref(r, kind as u8);
    (v.to_bits() == bits).then_some(r)
}

/// Can `k` live in a table without an index? Its hash must be computable
/// natively, without side effects ([`Vm::key_hash_pure`]).
fn linear_key(k: Value) -> bool {
    k.as_int().is_some()
        || k.as_float().is_some()
        || Vm::is_str(k)
        || k == Value::TRUE
        || k == Value::FALSE
}

fn counts(len: u32, used: u32) -> u64 {
    u64::from(len) | u64::from(used) << 32
}

/// Entries capacity of a fresh table for `need` entries: a power of two with
/// room to spare, so a compacted table is not resized again right away.
/// Small tables grow fourfold (fewer rebuilds), big ones double (a smaller
/// index stays in cache).
fn cap_for(need: usize) -> usize {
    let room = if need < 1 << 12 {
        need * 2
    } else {
        need + need / 2
    };
    let cap = room.max(8).next_power_of_two();
    // At most the largest table under the object cap while `need` fits it
    // (its index, `2 * cap + 1` words, is the bigger object).
    if need <= MAX_CAP {
        cap.min(MAX_CAP)
    } else {
        cap
    }
}

/// `add_entry`'s `top` for a caller that cannot hand a request on.
const NOT_TOP: u64 = u64::MAX;

/// The largest dict table under the object cap.
pub(crate) const MAX_CAP: usize = mm_core::MAX_OBJECT_BYTES as usize / 32;

impl Vm {
    /// A new empty dict.
    pub(crate) fn new_dict(&mut self) -> Result<Value, Raised> {
        self.new_dict_for(EMPTY_ROOM)
    }

    /// A new empty dict with room for `n` entries: inline up to
    /// `LINEAR_MAX`, else an indexed table of the next power of two.
    pub(crate) fn new_dict_for(&mut self, n: usize) -> Result<Value, Raised> {
        if n > LINEAR_MAX {
            return self.new_dict_indexed(n);
        }
        let empty = [Value::UNDEFINED; 2 * LINEAR_MAX];
        let r = {
            let res = self
                .heap
                .alloc_init(ty::DICT, 0, &empty[..2 * n.max(1)], &[0]);
            self.hx(res)
        }?;
        Ok(Value::from_ref(r, ty::DICT as u8))
    }

    /// A new empty indexed dict of `cap` entries. Globals dicts are indexed:
    /// their inline caches keep an entries buffer and check the version.
    pub(crate) fn new_dict_indexed(&mut self, cap: usize) -> Result<Value, Raised> {
        let cap = cap.max(8).next_power_of_two();
        let (entries, index) = self.dict_tables(cap)?;
        let r = {
            let res = self.heap.alloc_init(
                ty::DICT,
                0,
                &[
                    Value::from_ref(entries, ty::BUF as u8),
                    Value::from_ref(index, ty::RAW as u8),
                ],
                &[0],
            );
            self.hx(res)
        }?;
        Ok(Value::from_ref(r, ty::DICT as u8))
    }

    /// Empty entries and index tables for `cap` (a power of two) entries.
    fn dict_tables(&mut self, cap: usize) -> Result<(Ref, Ref), Raised> {
        Ok((self.dict_entries_table(cap)?, self.dict_index_table(cap)?))
    }

    /// An empty entries table for `cap` entries.
    pub(crate) fn dict_entries_table(&mut self, cap: usize) -> Result<Ref, Raised> {
        // Charged as the copy or rehash that fills it (docs/spec.md §5).
        if cap > 8 {
            self.burn_result(cap * 16, false);
        }
        self.alloc_buf(2 * cap)
    }

    /// An empty index table for `cap` entries.
    pub(crate) fn dict_index_table(&mut self, cap: usize) -> Result<Ref, Raised> {
        if cap > 8 {
            self.burn_result(cap * 8, false);
        }
        let n = u32::try_from(2 * cap + 1).map_err(|_| self.err(T::MemoryError, ""))?;
        let res = self.heap.alloc(ty::RAW, 0, 0, n);
        self.hx(res)
    }

    /// The storage of dict `r`.
    #[inline]
    pub(crate) fn dv(&self, r: Ref) -> Option<Dv> {
        let (hdr, w) = self.heap.object_words(r).ok()?;
        let (&c, vals) = w.split_last()?;
        // Verified but mutated bytecode can hand a dict op another object
        // (libFuzzer vm_bytecode F4: `DICT_INSERT` into a list, whose one
        // value slot read as a linear table that never got an index, so
        // every store scanned all entries). Every dict has room for an
        // index ref, so a linear table holds at most `LINEAR_MAX` entries.
        if hdr.type_id() != ty::DICT || vals.len() < 2 {
            return None;
        }
        #[allow(clippy::cast_possible_truncation, reason = "the two halves")]
        let (len, used) = (c as u32, (c >> 32) as u32);
        let globals = hdr.flags() & crate::vm::GLOBALS_FLAG != 0;
        if let Some(entries) = ref_of_kind(*vals.first()?, ty::BUF) {
            let index = vals.get(1).and_then(|&b| ref_of_kind(b, ty::RAW));
            Some(Dv {
                entries,
                index,
                inline: 0,
                len,
                used,
                globals,
            })
        } else {
            Some(Dv {
                entries: r,
                index: None,
                inline: u32::try_from(vals.len() / 2).ok()?,
                len,
                used,
                globals,
            })
        }
    }

    /// Writes the counters of dict `r`.
    fn set_counts(&mut self, r: Ref, len: u32, used: u32) {
        if let Ok(raws) = self.heap.raw_words_mut(r)
            && let Some(c) = raws.last_mut()
        {
            *c = counts(len, used);
        }
    }

    /// Entry room of a table without an index.
    fn linear_room(&self, dv: &Dv) -> u32 {
        if dv.inline > 0 {
            dv.inline
        } else {
            self.obj_len(dv.entries) / 2
        }
    }

    /// Number of live entries of a dict.
    #[inline(never)]
    pub(crate) fn dict_len(&self, d: Value) -> usize {
        d.as_obj()
            .and_then(|r| self.dv(r))
            .map_or(0, |v| v.len as usize)
    }

    /// Entries used (live and deleted), the bound for iteration.
    #[inline(never)]
    pub(crate) fn dict_used(&self, d: Value) -> usize {
        d.as_obj()
            .and_then(|r| self.dv(r))
            .map_or(0, |v| v.used as usize)
    }

    /// The entries buffer of an indexed dict (for globals inline caches).
    pub(crate) fn dict_entries(&self, d: Value) -> Option<Ref> {
        let dv = self.dv(d.as_obj()?)?;
        dv.index.map(|_| dv.entries)
    }

    /// Entry `i`: `(key, value)`, or `None` if deleted or out of range.
    #[inline(never)]
    pub(crate) fn dict_entry(&self, d: Value, i: usize) -> Option<(Value, Value)> {
        let e = self.dv(d.as_obj()?)?.entries;
        let i = u32::try_from(i).ok()?;
        let k = self.heap.get(e, 2 * i).ok()?;
        if k == Value::UNDEFINED {
            return None;
        }
        Some((k, self.slot(e, 2 * i + 1)))
    }

    /// Entry storage and its used slot bound, including deleted pairs.
    /// Valid only while no mutation or collection can change the storage.
    pub(crate) fn dict_items_buf(&self, d: Value) -> Option<(Ref, u32)> {
        let dv = self.dv(d.as_obj()?)?;
        Some((dv.entries, dv.used.min(self.obj_len(dv.entries) / 2) * 2))
    }

    /// Number of pairs `dict_items_vec` returns, counted without copying.
    pub(crate) fn dict_live_len(&self, d: Value) -> usize {
        let Some(dv) = d.as_obj().and_then(|r| self.dv(r)) else {
            return 0;
        };
        let w = self.heap.slot_words(dv.entries).unwrap_or(&[]);
        let w = w.get(..2 * dv.used as usize).unwrap_or(w);
        w.as_chunks::<2>()
            .0
            .iter()
            .filter(|[k, _]| Value::from_bits(*k).unwrap_or(Value::UNDEFINED) != Value::UNDEFINED)
            .count()
    }

    /// [`Vm::dict_items_vec`] for a caller that stores every pair, charged
    /// one unit per pair (docs/spec.md §5).
    pub(crate) fn dict_items_charged(&mut self, d: Value) -> Vec<(Value, Value)> {
        let items = self.dict_items_vec(d);
        if items.len() >= 8 {
            self.burn(items.len() as u64);
        }
        items
    }

    /// Live `(key, value)` pairs in order.
    pub(crate) fn dict_items_vec(&self, d: Value) -> Vec<(Value, Value)> {
        let Some(dv) = d.as_obj().and_then(|r| self.dv(r)) else {
            return Vec::new();
        };
        let kvs = self.values_of(dv.entries, 2 * dv.used as usize);
        // (A restored snapshot's counts are not trusted.)
        let mut out = Vec::with_capacity((dv.len as usize).min(kvs.len() / 2));
        for &[k, v] in kvs.as_chunks::<2>().0 {
            if k != Value::UNDEFINED {
                out.push((k, v));
            }
        }
        out
    }

    /// After a change to the keys of dict `d`: if it is some code's
    /// globals, the global inline caches are stale.
    #[inline]
    fn globals_changed(&mut self, d: Value, r: Ref) {
        if d == self.globals
            || self
                .heap
                .header(r)
                .is_ok_and(|h| h.flags() & crate::vm::GLOBALS_FLAG != 0)
        {
            self.gver = self.gver.wrapping_add(1);
        }
    }

    /// Bumps the version of an indexed dict (tables without an index have
    /// none: they never hold globals).
    fn bump_version(&mut self, d: Value, r: Ref) {
        self.bump_own_version(r);
        self.globals_changed(d, r);
    }

    /// [`Vm::bump_version`] for a deletion, which leaves the entries table
    /// in place: global inline caches stay valid, as their fast paths
    /// treat a deleted entry (`UNDEFINED`) as a miss.
    fn bump_own_version(&mut self, r: Ref) {
        let Some(index) = self.dv(r).and_then(|v| v.index) else {
            return;
        };
        if let Ok(words) = self.heap.raw_words_mut(index)
            && let Some(x) = words.last_mut()
        {
            *x = x.wrapping_add(1);
        }
    }

    // ------------------------------------------------------------ hash

    /// Python's `hash(v)`; `TypeError` for unhashable values. Hashing
    /// charges one unit per tuple item and per 64 bytes of bytes (and of a
    /// str whose hash is not cached yet), and
    /// visits at most [`WALK`] tuples: past that (`t = (t, t)` nested 60
    /// deep is 2^60 visits) it hands off to the prelude twin, which hashes
    /// each distinct tuple once, as CPython's cached tuple hashes do, and
    /// parks (docs/spec.md §5).
    pub(crate) fn hash(&mut self, v: Value) -> Result<i64, Raised> {
        let mut left = WALK;
        self.hash_depth(v, 0, &mut left)
    }

    fn hash_depth(&mut self, v: Value, depth: u32, left: &mut u32) -> Result<i64, Raised> {
        if let Some(i) = v.as_int() {
            return Ok(hash_int(i));
        }
        if let Some(f) = v.as_float() {
            return Ok(hash_float(f));
        }
        if Vm::is_str(v) {
            let r = v.as_obj();
            if r.is_some_and(|r| !self.heap.str_hashed(r)) {
                // A first hash reads the whole text: byte work.
                self.burn_bytes(self.str_bytes(v).len());
            }
            let h = r
                .and_then(|r| self.heap.str_hash_mut(r).ok())
                .unwrap_or(0)
                .cast_signed();
            return Ok(if h == -1 { -2 } else { h });
        }
        if let Some(i) = self.int_of(v) {
            return Ok(hash_int(i));
        }
        if Vm::is_heap_int(v) {
            // Read in place, a unit per 32 bytes; in a tuple, also that
            // many visits of the walk's budget.
            let (neg, mag) = self.heap_int_mag(v);
            let units = mag.len().div_ceil(4) as u64 / 8;
            *left = left.saturating_sub(u32::try_from(units).unwrap_or(u32::MAX));
            let h = crate::bigint::hash_le(neg, mag);
            self.burn(units);
            return Ok(h);
        }
        if let Some((re, im)) = self.complex_of(v) {
            return Ok(crate::complex::hash(re, im));
        }
        if self.type_of(v) == T::frozenset {
            self.burn(self.set_len(v) as u64);
            return Ok(self.frozenset_hash(v));
        }
        if Vm::is_bytes(v) {
            self.burn_bytes(self.bytes_data(v).len());
            let h = mm_core::hash_bytes(self.bytes_data(v)).cast_signed();
            return Ok(if h == -1 { -2 } else { h });
        }
        if v == Value::NONE {
            // CPython 3.12+ hashes `None` to a constant.
            return Ok(0xFCA8_6420);
        }
        let t = self.type_of(v);
        // A slice of slices or a method of methods nests as deep as a
        // tuple may.
        let deep = |vm: &mut Vm| vm.err(T::RecursionError, "maximum recursion depth exceeded");
        if matches!(t, T::range | T::slice) {
            if depth > MAX_DEPTH {
                return Err(deep(self));
            }
            // CPython hashes a range as `(len, start, step)` with `None`
            // for what does not matter, and a slice as its three parts
            // (xxHash lanes without the length term).
            let parts = if t == T::range {
                let n = self.range_len(v);
                let (start, _, step) = self.range_parts(v);
                let n = self.new_int(i64::try_from(n).unwrap_or(i64::MAX))?;
                let start = if self.range_len(v) == 0 {
                    Value::NONE
                } else {
                    self.new_int(start)?
                };
                let step = if self.range_len(v) < 2 {
                    Value::NONE
                } else {
                    self.new_int(step)?
                };
                [n, start, step]
            } else {
                [self.vslot(v, 0), self.vslot(v, 1), self.vslot(v, 2)]
            };
            let mut acc = XX_SEED;
            for p in parts {
                acc = xx_lane(acc, self.hash_depth(p, depth + 1, left)?);
            }
            return Ok(xx_finish(acc, (t == T::range).then_some(3)));
        }
        if v.is_obj_kind(ty::TUPLE as u8) {
            if depth > MAX_DEPTH {
                return Err(self.err(
                    T::RecursionError,
                    "maximum recursion depth exceeded while hashing a tuple",
                ));
            }
            // The budget counts the items of every tuple walked, so one
            // call hashes at most `WALK`; a longer tuple is hashed a slice
            // per call by the prelude's `_thash` (`pl_hash_tuple`).
            let n = self.tuple_len(v);
            let k = u32::try_from(n).unwrap_or(u32::MAX).saturating_add(1);
            if *left < k {
                return Err(self.need_py());
            }
            *left -= k;
            if n >= 8 || depth > 0 {
                self.burn(n as u64);
            }
            let mut acc = XX_SEED;
            for i in 0..n {
                if *left == 0 {
                    // Big ints spent the budget.
                    return Err(self.need_py());
                }
                let x = self.vslot(v, u32::try_from(i).unwrap_or(0));
                acc = xx_lane(acc, self.hash_depth(x, depth + 1, left)?);
            }
            return Ok(xx_finish(acc, Some(n as u64)));
        }
        if let Some(c) = self.user_class(v)
            && self.uflags(v) & crate::class::f::HASH != 0
        {
            let k = self.dn(Dn::Hash)?;
            if self.class_lookup(c, k) == Some(Value::NONE) {
                let msg = alloc::format!("unhashable type: '{}'", self.type_name(v));
                return Err(self.err(T::TypeError, &msg));
            }
            return Err(self.need_py());
        }
        if let Some(b) = self.backing(v) {
            return self.hash_depth(b, depth + 1, left);
        }
        if matches!(
            t,
            T::list
                | T::dict
                | T::set
                | T::dict_keys
                | T::dict_values
                | T::dict_items
                | T::bytearray
        ) {
            return Err(self.err(
                T::TypeError,
                &alloc::format!("unhashable type: '{}'", t.name()),
            ));
        }
        let id_hash = |v: Value| {
            let bits = v.to_bits();
            ((bits >> 4) ^ (bits << 20)).cast_signed() >> 2
        };
        if matches!(v.obj_kind().map(u16::from), Some(ty::METHOD | ty::BOUND)) {
            // CPython's `meth_hash`: the receiver's identity with the
            // function's hash, so each `obj.f` made hashes alike.
            if depth > MAX_DEPTH {
                return Err(deep(self));
            }
            let f = self.hash_depth(self.vslot(v, 1), depth + 1, left)?;
            let h = id_hash(self.vslot(v, 0)) ^ f;
            return Ok(if h == -1 { -2 } else { h });
        }
        Ok(id_hash(v))
    }

    // ------------------------------------------------------ equality

    /// `a == b` for builtin values; containers compare element-wise. User
    /// `__eq__` methods need Python code ([`Vm::need_py`]).
    pub(crate) fn eq_values(&mut self, a: Value, b: Value) -> Result<bool, Raised> {
        let mut left = WALK;
        let r = self.eq_hard(a, b, 0, &mut left);
        self.walked(left);
        match r {
            Ok(r) => Ok(r),
            Err(Hard::Py) => Err(self.need_py()),
            Err(Hard::Deep) => Err(self.err(
                T::RecursionError,
                "maximum recursion depth exceeded in comparison",
            )),
        }
    }

    /// `a == b` as `PyObject_RichCompareBool` decides it for containers:
    /// identity first (so a shared NaN is found), then [`Vm::eq_values`].
    pub(crate) fn eq_elem(&mut self, a: Value, b: Value) -> Result<bool, Raised> {
        if a == b {
            return Ok(true);
        }
        self.eq_values(a, b)
    }

    /// Charges a walk's visits after its first, one fuel unit each.
    pub(crate) fn walked(&mut self, left: u32) {
        self.walked_of(WALK, left);
    }

    /// [`Vm::walked`] for a walk that started with `budget` visits.
    pub(crate) fn walked_of(&mut self, budget: u32, left: u32) {
        let used = u64::from(budget.saturating_sub(left)) + self.text_over.take();
        if used > 1 {
            self.burn(used - 1);
        }
    }

    /// Matches dict `a`'s entries from entry `e` on against dict `b`
    /// (`b[k] == v` for each key `k` and value `v`, looked up with the
    /// stored hash), inside a walk at `depth` with `left` visits. Returns
    /// the entry it stopped at and `Ok(true)` if all matched, `Ok(false)`
    /// if that entry does not, `Err` if only Python can decide it.
    pub(crate) fn dict_match_from(
        &self,
        a: Value,
        b: Value,
        mut e: u32,
        depth: u32,
        left: &mut u32,
    ) -> (u32, Result<bool, Hard>) {
        let (Some(da), Some(rb)) = (a.as_obj().and_then(|r| self.dv(r)), b.as_obj()) else {
            return (e, Ok(false));
        };
        while e < da.used {
            let k = self.slot(da.entries, 2 * e);
            if k != Value::UNDEFINED {
                // One visit per entry, found by identity or not; the twin
                // takes the entry the budget ran out at.
                let Some(rest) = left.checked_sub(1) else {
                    return (e, Err(Hard::Py));
                };
                *left = rest;
                let v = self.slot(da.entries, 2 * e + 1);
                let h = match da.index {
                    Some(index) => {
                        let words = self.heap.raw_words(index).unwrap_or(&[]);
                        words.get(words.len() / 2 + e as usize).copied()
                    }
                    None => self.key_hash_pure(k),
                };
                let Some(h) = h else {
                    return (e, Err(Hard::Py));
                };
                let r = match self.probe_with(rb, k, h, depth, left, &mut |_, _| {}) {
                    Ok((_, Some(f))) => match self.dict_entry_at(b, f) {
                        Some((_, w)) if v == w => Ok(true),
                        Some((_, w)) => self.eq_hard(v, w, depth + 1, left),
                        None => Ok(false),
                    },
                    Ok((_, None)) => Ok(false),
                    Err(h) => Err(h),
                };
                if r != Ok(true) {
                    return (e, r);
                }
            }
            e += 1;
        }
        (e, Ok(true))
    }

    /// `str == str` for the dispatch loop's fast path: `None` when the test
    /// would read 64 bytes or more (`Heap::str_eq_short`), which the
    /// comparison walk then decides and charges.
    #[inline]
    pub(crate) fn str_eq_short(&self, a: Value, b: Value) -> Option<bool> {
        if a == b {
            return Some(true);
        }
        let (Some(ra), Some(rb)) = (a.as_obj(), b.as_obj()) else {
            return Some(false);
        };
        self.heap.str_eq_short(ra, rb)
    }

    /// Are `str`s `a` and `b` equal, and how many bytes did the test
    /// compare (`Heap::str_eq_n`)?
    pub(crate) fn str_eq_n(&self, a: Value, b: Value) -> (bool, usize) {
        let (Some(ra), Some(rb)) = (a.as_obj(), b.as_obj()) else {
            return (false, 0);
        };
        self.heap.str_eq_n(ra, rb).unwrap_or((false, 0))
    }

    /// The value of an int, bool or float as `f64`.
    pub(crate) fn num_f64(&self, v: Value) -> Option<f64> {
        if let Some(f) = v.as_float() {
            return Some(f);
        }
        #[allow(clippy::cast_precision_loss, reason = "Python int to float conversion")]
        self.int_of(v).map(|i| i as f64)
    }

    // ------------------------------------------------------- lookup

    /// Hash of a dict key as stored (Python hash bits): `str` and int
    /// keys natively, anything else through [`Vm::hash_as`].
    #[inline]
    pub(crate) fn key_hash(&mut self, k: Value) -> Result<u64, Raised> {
        if Vm::is_str(k)
            && let Some(r) = k.as_obj()
            && let Ok(h) = self.heap.str_hash_mut(r)
        {
            return Ok(str_key_hash(h));
        }
        if let Some(i) = k.as_int() {
            return Ok(hash_int(i).cast_unsigned());
        }
        self.hash_as(k, "dict key")
    }

    /// `hash(k)` for a container key; CPython 3.14 names the container in
    /// the error: "cannot use 'list' as a dict key (unhashable type: 'list')".
    pub(crate) fn hash_as(&mut self, k: Value, what: &str) -> Result<u64, Raised> {
        if let Ok(h) = self.hash(k) {
            Ok(h.cast_unsigned())
        } else {
            let exc = self.exc;
            if self.exc_type(exc) != T::TypeError {
                return Err(Raised);
            }
            let inner = self.exc_str(exc).unwrap_or_default();
            let msg = alloc::format!("cannot use '{}' as a {what} ({inner})", self.type_name(k));
            Err(self.err(T::TypeError, &msg))
        }
    }

    /// Hash without raising or running Python code: `Some` for every
    /// [`linear_key`], equal to what `key_hash` gives.
    #[inline]
    pub(crate) fn key_hash_pure(&self, k: Value) -> Option<u64> {
        if Vm::is_str(k) {
            return Some(str_key_hash(self.heap.str_hash(k.as_obj()?).ok()?));
        }
        if let Some(i) = k.as_int() {
            return Some(hash_int(i).cast_unsigned());
        }
        if let Some(f) = k.as_float() {
            return Some(hash_float(f).cast_unsigned());
        }
        if k == Value::TRUE || k == Value::FALSE {
            return Some(hash_int(i64::from(k == Value::TRUE)).cast_unsigned());
        }
        None
    }

    /// Finds `k`: `(index slot, entry index or None)`; `Err` if a key
    /// comparison needs Python code.
    fn probe(&self, d: Ref, k: Value, h: u64) -> Result<(u32, Option<u32>), Hard> {
        let mut left = WALK;
        self.probe_with(d, k, h, 0, &mut left, &mut |_, _| {})
    }

    /// `probe` inside a walk at `depth` with `left` visits, reporting every
    /// entry whose hash matches and whose key compares only in Python to
    /// `undecided` (then going on as if unequal).
    fn probe_with(
        &self,
        d: Ref,
        k: Value,
        h: u64,
        depth: u32,
        left: &mut u32,
        undecided: &mut dyn FnMut(u32, Value),
    ) -> Result<(u32, Option<u32>), Hard> {
        match self.dv(d) {
            Some(dv) => self.probe_dv(dv, k, h, depth, left, undecided),
            None => Ok((0, None)),
        }
    }

    /// `probe_with` on storage already read.
    #[inline]
    fn probe_dv(
        &self,
        dv: Dv,
        k: Value,
        h: u64,
        depth: u32,
        left: &mut u32,
        undecided: &mut dyn FnMut(u32, Value),
    ) -> Result<(u32, Option<u32>), Hard> {
        self.probe_val(dv, k, h, depth, left, undecided)
            .map(|(i, e, _)| (i, e))
    }

    /// `probe_dv`, also returning the value of the entry found (the bits
    /// read with the key, so a lookup needs no further access).
    fn probe_val(
        &self,
        dv: Dv,
        k: Value,
        h: u64,
        depth: u32,
        left: &mut u32,
        undecided: &mut dyn FnMut(u32, Value),
    ) -> Result<(u32, Option<u32>, u64), Hard> {
        let Some(index) = dv.index else {
            return self.scan(dv, k, h, depth, left, undecided);
        };
        let words = self.heap.raw_words(index).unwrap_or(&[]);
        let kvs = self.heap.slot_words(dv.entries).unwrap_or(&[]);
        let cap = u32::try_from(words.len() / 2).unwrap_or(0);
        if cap == 0 {
            return Ok((0, None, 0));
        }
        let g = Geo::new(cap);
        let mut deferred = false;
        let mut i = g.start(h);
        let tag = g.tag(h);
        let mut first_free = None;
        let val = |e: u32| kvs.get(2 * e as usize + 1).copied().unwrap_or(0);
        let mut perturb = h;
        for _ in 0..=g.steps() {
            let w = islot(words, i);
            if w == 0 {
                if deferred {
                    return Err(Hard::Py);
                }
                return Ok((first_free.unwrap_or(i), None, 0));
            }
            if w == DELETED {
                if first_free.is_none() {
                    first_free = Some(i);
                }
            } else if w & !g.emask == tag {
                let e = (w & g.emask) - 1;
                let kb = kvs.get(2 * e as usize).copied().unwrap_or(0);
                if kb == k.to_bits() {
                    return Ok((i, Some(e), val(e)));
                }
                if words.get((cap + e) as usize) == Some(&h) {
                    let key = Value::from_bits(kb).unwrap_or(Value::UNDEFINED);
                    match self.key_eq(key, k, depth + 1, left) {
                        Ok(true) => return Ok((i, Some(e), val(e))),
                        Ok(false) => {}
                        Err(Hard::Deep) => return Err(Hard::Deep),
                        Err(Hard::Py) => {
                            deferred = true;
                            undecided(e, key);
                        }
                    }
                }
            }
            i = g.next(i, &mut perturb);
        }
        if deferred {
            return Err(Hard::Py);
        }
        Ok((first_free.unwrap_or(0), None, 0))
    }

    /// `probe_with` for a table without an index: a scan of the entries,
    /// with each key's hash computed natively (every key is a
    /// [`linear_key`]). The slot it returns is the entry index.
    fn scan(
        &self,
        dv: Dv,
        k: Value,
        h: u64,
        depth: u32,
        left: &mut u32,
        undecided: &mut dyn FnMut(u32, Value),
    ) -> Result<(u32, Option<u32>, u64), Hard> {
        let kvs = self.heap.slot_words(dv.entries).unwrap_or(&[]);
        let mut deferred = false;
        for (e, &[kb, vb]) in (0..dv.used).zip(kvs.as_chunks::<2>().0) {
            if kb == k.to_bits() {
                return Ok((e, Some(e), vb));
            }
            let Some(key) = Value::from_bits(kb) else {
                continue;
            };
            if key == Value::UNDEFINED || self.key_hash_pure(key) != Some(h) {
                continue;
            }
            match self.key_eq(key, k, depth + 1, left) {
                Ok(true) => return Ok((e, Some(e), vb)),
                Ok(false) => {}
                Err(Hard::Deep) => return Err(Hard::Deep),
                Err(Hard::Py) => {
                    deferred = true;
                    undecided(e, key);
                }
            }
        }
        if deferred {
            return Err(Hard::Py);
        }
        Ok((dv.used, None, 0))
    }

    /// Key equality once the hashes match: two `str`s compare natively.
    #[inline(never)]
    fn key_eq(&self, key: Value, k: Value, depth: u32, left: &mut u32) -> Result<bool, Hard> {
        if Vm::is_str(key)
            && Vm::is_str(k)
            && let (Some(a), Some(b)) = (key.as_obj(), k.as_obj())
        {
            // One visit of the walk's budget, as `eq_hard` charges.
            let Some(rest) = left.checked_sub(1) else {
                return Err(Hard::Py);
            };
            let (eq, n) = self.heap.str_eq_n(a, b).unwrap_or((false, 0));
            *left = rest;
            self.spend_text(left, n);
            return Ok(eq);
        }
        self.eq_hard(key, k, depth, left)
    }

    /// Maps a probe failure to an exception.
    fn hard(&mut self, h: Hard) -> Raised {
        match h {
            Hard::Py => self.need_py(),
            Hard::Deep => self.err(
                T::RecursionError,
                "maximum recursion depth exceeded in comparison",
            ),
        }
    }

    /// For the prelude's lookups of keys with Python `__hash__`/`__eq__`:
    /// the entry of `k` (hash `h`) if native comparison finds it, else the
    /// `[(entry, key), ...]` whose keys Python must compare, else -1.
    pub(crate) fn dict_probe_py(&mut self, d: Value, k: Value, h: u64) -> Result<Value, Raised> {
        let Some(r) = d.as_obj().filter(|_| d.is_obj_kind(ty::DICT as u8)) else {
            return Err(self.err(T::TypeError, "expected a dict"));
        };
        let mut cands: Vec<(u32, Value)> = Vec::new();
        let mut left = WALK;
        let found = self.probe_with(r, k, h, 0, &mut left, &mut |e, key| cands.push((e, key)));
        self.walked(left);
        match found {
            Ok((_, Some(e))) => self.new_int(i64::from(e)),
            Ok((_, None)) => Ok(Value::int(-1).unwrap_or(Value::NONE)),
            Err(Hard::Deep) => Err(self.hard(Hard::Deep)),
            Err(Hard::Py) => {
                let mut items = Vec::with_capacity(cands.len());
                for (e, key) in cands {
                    let ev = self.new_int(i64::from(e))?;
                    items.push(self.alloc_tuple(&[ev, key])?);
                }
                self.new_list(&items)
            }
        }
    }

    /// Inserts `k` (hash `h`, known absent) without comparing keys.
    pub(crate) fn dict_insert_new(
        &mut self,
        d: Value,
        k: Value,
        h: u64,
        v: Value,
    ) -> Result<(), Raised> {
        let Some(r) = d.as_obj() else { return Ok(()) };
        let Some(dv) = self.dv(r) else { return Ok(()) };
        self.add_entry(d, r, dv, k, h, v, None, NOT_TOP)
    }

    /// Can `dv` take one more entry with key `k` as it is?
    fn has_room(&self, dv: &Dv, k: Value) -> bool {
        match dv.index {
            Some(i) => (dv.used as usize) < self.heap.raw_words(i).map_or(0, |w| w.len() / 2),
            None => linear_key(k) && dv.used < self.linear_room(dv),
        }
    }

    /// Adds key `k` (hash `h`, known absent) of dict `r` (storage `dv`),
    /// growing or indexing the table first if needed; `slot` is the free
    /// index slot a probe found, if one did.
    /// Unless `top` is [`NOT_TOP`] (the caller can hand a request on,
    /// whose value is then register `top` of [`Vm::bulk_grow_top`]), a
    /// table of
    /// [`crate::set::GROW_STEPS`] live entries or more grows in steps
    /// instead.
    #[allow(clippy::too_many_arguments, reason = "the probe's results")]
    #[inline(never)]
    fn add_entry(
        &mut self,
        d: Value,
        r: Ref,
        mut dv: Dv,
        k: Value,
        h: u64,
        v: Value,
        mut slot: Option<u32>,
        top: u64,
    ) -> Result<(), Raised> {
        // An indexed table checks its room in the same access that writes
        // the index.
        if let Some(index) = dv.index
            && self.dict_put(d, r, dv, index, slot, k, h, v)
        {
            return Ok(());
        }
        if dv.index.is_some() || !self.has_room(&dv, k) {
            if top != NOT_TOP
                && dv.len as usize >= crate::set::GROW_STEPS
                && self.grow_in_steps(cap_for(dv.len as usize + 1) * 32)
            {
                return self.bulk_grow_top(d, k, v, top);
            }
            // A rebuild past a step's work goes on in steps.
            let budget = if top == NOT_TOP {
                u64::MAX
            } else {
                u64::from(dv.used).saturating_add(crate::nstep::STEP_WORK)
            };
            if let Some((e, i, at, n)) =
                self.dict_resize(d, r, dv.len as usize + 1, !linear_key(k), budget)?
            {
                let src = self.alloc_tuple(&[k])?;
                let tables = (
                    Value::from_ref(e, ty::BUF as u8),
                    Value::from_ref(i, ty::RAW as u8),
                    at,
                    n,
                );
                let prog = crate::set::phase(crate::set::act::KEYS, 0, 1, 2);
                return self.bulk_with(&[d, src, v], prog, top, tables).map(|_| ());
            }
            match self.dv(r) {
                Some(n) if self.has_room(&n, k) => dv = n,
                _ => return Err(self.err(T::SystemError, "malformed dict")),
            }
            slot = None;
        }
        match dv.index {
            None => {
                let _ = self.heap.set_values(dv.entries, 2 * dv.used, &[k, v]);
                self.set_counts(r, dv.len.saturating_add(1), dv.used + 1);
            }
            Some(index) => {
                self.dict_put(d, r, dv, index, slot, k, h, v);
            }
        }
        Ok(())
    }

    /// Writes a new entry of an indexed table at index slot `slot` (the
    /// first free one on `h`'s probe sequence if `None`); `false`, writing
    /// nothing, if the table is full.
    #[allow(clippy::too_many_arguments, reason = "one hot call site each")]
    fn dict_put(
        &mut self,
        d: Value,
        r: Ref,
        dv: Dv,
        index: Ref,
        slot: Option<u32>,
        k: Value,
        h: u64,
        v: Value,
    ) -> bool {
        let e = dv.used;
        let Ok(words) = self.heap.raw_words_mut(index) else {
            return false;
        };
        let g = Geo::new(u32::try_from(words.len() / 2).unwrap_or(0));
        if e >= g.cap || words.len() / 2 == 0 {
            return false;
        }
        let slot = slot.unwrap_or_else(|| free_in(words, g, h).map_or(0, |(i, _)| i));
        set_islot(words, slot, g.word(h, e));
        if let Some(w) = words.get_mut((g.cap + e) as usize) {
            *w = h;
        }
        if let Some(x) = words.last_mut() {
            *x = x.wrapping_add(1);
        }
        let _ = self.heap.set_values(dv.entries, 2 * e, &[k, v]);
        self.set_counts(r, dv.len.saturating_add(1), e + 1);
        if dv.globals || d == self.globals {
            // A new global may shadow a builtin; the entries table stays.
            self.bver = self.bver.wrapping_add(1);
        }
        true
    }

    /// The value of entry `e` (`UNDEFINED` if deleted).
    pub(crate) fn dict_value_at(&self, d: Value, e: u32) -> Value {
        match d.as_obj().and_then(|r| self.dv(r)) {
            Some(dv) if self.slot(dv.entries, 2 * e) != Value::UNDEFINED => {
                self.slot(dv.entries, 2 * e + 1)
            }
            _ => Value::UNDEFINED,
        }
    }

    /// The key and value of entry `e` below [`Vm::dict_used`] (the key
    /// `UNDEFINED` if deleted).
    pub(crate) fn dict_entry_at(&self, d: Value, e: u32) -> Option<(Value, Value)> {
        let dv = d.as_obj().and_then(|r| self.dv(r))?;
        (e < dv.used).then(|| {
            (
                self.slot(dv.entries, 2 * e),
                self.slot(dv.entries, 2 * e + 1),
            )
        })
    }

    /// Replaces the value of live entry `e`.
    pub(crate) fn dict_set_at(&mut self, d: Value, e: u32, v: Value) {
        if let Some(dv) = d.as_obj().and_then(|r| self.dv(r))
            && self.slot(dv.entries, 2 * e) != Value::UNDEFINED
        {
            self.set_slot(dv.entries, 2 * e + 1, v);
        }
    }

    /// Deletes live entry `e`.
    /// Removes the last live entry by position (no hashing), and the
    /// deleted entries after it, as CPython's `popitem` drops them from
    /// `dk_nentries`: repeated calls are O(1) each, not a rescan of the
    /// deleted tail (charged per entry passed, docs/spec.md §5).
    pub(crate) fn dict_pop_last(&mut self, d: Value) -> Option<(Value, Value)> {
        let used = self.dict_used(d);
        let (e, kv) = (0..used)
            .rev()
            .find_map(|i| self.dict_entry(d, i).map(|kv| (i, kv)))?;
        self.burn(((used - e) / 8) as u64);
        let e = u32::try_from(e).unwrap_or(0);
        self.dict_del_at(d, e);
        if let Some(r) = d.as_obj()
            && let Some(dv) = self.dv(r)
        {
            self.set_counts(r, dv.len, e);
        }
        Some(kv)
    }

    pub(crate) fn dict_del_at(&mut self, d: Value, e: u32) {
        let Some(r) = d.as_obj() else { return };
        let Some(dv) = self.dv(r) else { return };
        let entries = dv.entries;
        if self.slot(entries, 2 * e) == Value::UNDEFINED {
            return;
        }
        if let Some(index) = dv.index
            && let Ok(words) = self.heap.raw_words_mut(index)
        {
            let g = Geo::new(u32::try_from(words.len() / 2).unwrap_or(0));
            let h = words.get((g.cap + e) as usize).copied().unwrap_or(0);
            let mut i = g.start(h);
            let mut perturb = h;
            for _ in 0..=g.steps() {
                let w = islot(words, i);
                if w == 0 {
                    break;
                }
                if w != DELETED && w & g.emask == e + 1 {
                    set_islot(words, i, DELETED);
                    break;
                }
                i = g.next(i, &mut perturb);
            }
        }
        let _ = self
            .heap
            .set_values(entries, 2 * e, &[Value::UNDEFINED, Value::UNDEFINED]);
        self.set_counts(r, dv.len.saturating_sub(1), dv.used);
        self.bump_own_version(r);
    }

    /// `d[k]` if present.
    pub(crate) fn dict_get(&mut self, d: Value, k: Value) -> Result<Option<Value>, Raised> {
        let h = self.key_hash(k)?;
        let Some(dv) = d.as_obj().and_then(|r| self.dv(r)) else {
            return Ok(None);
        };
        let mut left = WALK;
        let found = self.probe_val(dv, k, h, 0, &mut left, &mut |_, _| {});
        self.walked(left);
        let (_, e, vb) = found.map_err(|x| self.hard(x))?;
        Ok(e.map(|_| Value::from_bits(vb).unwrap_or(Value::UNDEFINED)))
    }

    /// A lookup of a str (or int) key that never raises: `None` if absent
    /// or if deciding needs Python code.
    pub(crate) fn dict_lookup_str(&self, d: Value, k: Value) -> Option<Value> {
        let h = self.key_hash_pure(k)?;
        let dv = self.dv(d.as_obj()?)?;
        let mut left = WALK;
        let (_, e, vb) = self
            .probe_val(dv, k, h, 0, &mut left, &mut |_, _| {})
            .ok()?;
        e?;
        Value::from_bits(vb)
    }

    /// The entry index of `k`, for inline caches.
    pub(crate) fn dict_find_entry(&mut self, d: Value, k: Value) -> Result<Option<u32>, Raised> {
        let h = self.key_hash(k)?;
        let Some(r) = d.as_obj() else { return Ok(None) };
        Ok(self.probe(r, k, h).map_err(|x| self.hard(x))?.1)
    }

    /// `d[k] = v`. Out of line: a wrapper of `dict_store`, it was inlined
    /// at `Vm::dispatch`'s three calls and cost the loop bench 3.8% Ir [M].
    #[inline(never)]
    pub(crate) fn dict_set(&mut self, d: Value, k: Value, v: Value) -> Result<(), Raised> {
        let h = self.key_hash(k)?;
        self.dict_store(d, k, h, v, NOT_TOP)
    }

    /// `d[k] = v` where the caller can hand a request on (a store
    /// opcode, `dict.__setitem__`): a big table grows in steps
    /// ([`Vm::bulk_grow_top`]; the request's value is `None`). Out of
    /// line, as `dict_set` is.
    #[inline(never)]
    pub(crate) fn dict_set_top(&mut self, d: Value, k: Value, v: Value) -> Result<(), Raised> {
        let h = self.key_hash(k)?;
        self.dict_store(d, k, h, v, crate::set::res::NONE)
    }

    /// `d.setdefault(k, v)` for `k` (hash `h`) known absent:
    /// [`Vm::dict_set_top`], but the request's value is `v` (register 2
    /// of [`Vm::bulk_grow_top`]).
    pub(crate) fn dict_setdefault_new(
        &mut self,
        d: Value,
        k: Value,
        h: u64,
        v: Value,
    ) -> Result<(), Raised> {
        let Some(r) = d.as_obj() else { return Ok(()) };
        let Some(dv) = self.dv(r) else { return Ok(()) };
        self.add_entry(d, r, dv, k, h, v, None, 2)
    }

    /// The body of `dict_set` and `dict_set_top`, in each.
    #[allow(
        clippy::inline_always,
        reason = "a call more per insert cost json and startup 0.7% Ir [M]"
    )]
    #[inline(always)]
    fn dict_store(&mut self, d: Value, k: Value, h: u64, v: Value, top: u64) -> Result<(), Raised> {
        let Some(r) = d.as_obj() else { return Ok(()) };
        let Some(dv) = self.dv(r) else {
            return Err(self.err(T::SystemError, "invalid dict table"));
        };
        let mut left = WALK;
        let found = self.probe_dv(dv, k, h, 0, &mut left, &mut |_, _| {});
        self.walked(left);
        let (slot, found) = found.map_err(|x| self.hard(x))?;
        if let Some(e) = found {
            self.set_slot(dv.entries, 2 * e + 1, v);
            return Ok(());
        }
        self.add_entry(d, r, dv, k, h, v, Some(slot), top)
    }

    /// Makes room for at least `need` entries: a full inline table moves out
    /// to a linear one as it is; anything else (an `index`-needing key, a
    /// full linear or indexed table, `need` over `LINEAR_MAX`) is rebuilt
    /// as an indexed table, compacting deletions, the probes past each
    /// entry's first slot charged. A rebuild whose entries and probes pass
    /// `budget` stops there: its new tables, entries read and entries
    /// moved, for the caller to go on with in steps.
    #[allow(clippy::type_complexity, reason = "a rebuild's state")]
    fn dict_resize(
        &mut self,
        d: Value,
        r: Ref,
        need: usize,
        index: bool,
        budget: u64,
    ) -> Result<Option<(Ref, Ref, u64, u64)>, Raised> {
        let Some(o) = self.dv(r) else { return Ok(None) };
        // No more entries than the old storage holds (a restored snapshot's
        // counts are not trusted).
        let held = self.heap.slot_words(o.entries).map_or(0, |w| w.len() / 2);
        let used = (o.used as usize).min(held);
        let inline = (o.inline as usize).min(LINEAR_MAX);
        let empty = [Value::UNDEFINED; 2 * LINEAR_MAX];
        if !index && inline > 0 && used < LINEAR_MAX && need <= LINEAR_MAX {
            // A full inline table moves out to a linear one as it is,
            // deleted entries included, so entry positions (and iterators
            // over them) stay put, as in a CPython table with room left.
            let entries = self.alloc_buf(2 * LINEAR_MAX)?;
            let mut kvs = empty;
            let mut live = 0;
            {
                let okv = self.heap.slot_words(r).unwrap_or(&[]);
                for (slot, &b) in kvs.iter_mut().zip(okv).take(2 * used) {
                    *slot = Value::from_bits(b).unwrap_or(Value::UNDEFINED);
                }
            }
            for &[k, _] in kvs.as_chunks::<2>().0.iter().take(used) {
                live += u32::from(k != Value::UNDEFINED);
            }
            let _ = self.heap.set_values(entries, 0, &kvs[..2 * used]);
            let _ = self.heap.set_values(r, 0, &empty[..2 * inline]);
            self.set_slot(r, 0, Value::from_ref(entries, ty::BUF as u8));
            self.set_slot(r, 1, Value::NONE);
            let used = u32::try_from(used).unwrap_or(0);
            self.set_counts(r, live, used);
            return Ok(None);
        }
        // Anything else becomes a compacted indexed table.
        let (entries, index) = self.dict_tables(cap_for(need))?;
        let Some((at, n, probes)) = self.dict_move(r, entries, index, 0, usize::MAX, 0, budget)
        else {
            return Err(self.err(T::SystemError, "malformed dict"));
        };
        self.burn(probes);
        if at < used {
            let tables = (entries, index, at as u64, u64::from(n));
            return Ok(Some(tables));
        }
        self.dict_install(d, r, entries, index, n);
        Ok(None)
    }

    /// Moves the live entries `at..to` of dict `r`'s table, in order, to
    /// the new indexed tables `entries` and `index` from entry `n` on,
    /// with their hashes (stored in an index, else computed: keys of a
    /// table without one all hash natively), until the entries read and
    /// the probes past the entries' first slots, less one per entry,
    /// reach `budget` (entries with equal hashes each probe past all
    /// those placed before them). Where it stopped, the entries the new
    /// table then holds and those probes, the work to charge on top of
    /// the entries; `None` if an entry found no free slot (a forged
    /// image's index). A rebuild a slice per step reads the old table,
    /// still installed, again each step.
    #[allow(clippy::too_many_arguments, reason = "a rebuild's slice")]
    pub(crate) fn dict_move(
        &mut self,
        r: Ref,
        entries: Ref,
        index: Ref,
        mut at: usize,
        to: usize,
        mut n: u32,
        budget: u64,
    ) -> Option<(usize, u32, u64)> {
        /// Entries moved per batch, through stack buffers (no heap copies).
        const BATCH: usize = 128;
        let Some(o) = self.dv(r) else {
            return Some((to, n, 0));
        };
        // No more entries than the old storage holds (a restored snapshot's
        // counts are not trusted).
        let held = self.heap.slot_words(o.entries).map_or(0, |w| w.len() / 2);
        let used = (o.used as usize).min(held).min(to);
        let mut kvs = [Value::UNDEFINED; 2 * BATCH];
        let mut hs = [0u64; BATCH];
        let g = Geo::new(self.heap.raw_words(index).map_or(0, |w| w.len() / 2) as u32);
        let (from, mut probes, mut keys) = (at, 0u64, 0u64);
        while at < used {
            // Gather the next batch of live entries with their hashes.
            let (mut k, start) = (0, at);
            {
                let okv = self.heap.slot_words(o.entries).unwrap_or(&[]);
                let ow = o
                    .index
                    .and_then(|i| self.heap.raw_words(i).ok())
                    .unwrap_or(&[]);
                let ohs = ow.get(ow.len() / 2..).unwrap_or(&[]);
                while at < used && k < BATCH {
                    let key = okv.get(2 * at).and_then(|&b| Value::from_bits(b));
                    if let Some(key) = key.filter(|&x| x != Value::UNDEFINED) {
                        let v = okv.get(2 * at + 1).and_then(|&b| Value::from_bits(b));
                        kvs[2 * k] = key;
                        kvs[2 * k + 1] = v.unwrap_or(Value::UNDEFINED);
                        hs[k] = if o.index.is_some() {
                            ohs.get(at).copied().unwrap_or(0)
                        } else {
                            self.key_hash_pure(key).unwrap_or(0)
                        };
                        k += 1;
                    }
                    at += 1;
                }
            }
            let (mut placed, mut stop) = (k, false);
            if let Ok(words) = self.heap.raw_words_mut(index) {
                for (i, (e, &h)) in (n..).zip(&hs[..k]).enumerate() {
                    let (slot, p) = free_in(words, g, h)?;
                    set_islot(words, slot, g.word(h, e));
                    if let Some(w) = words.get_mut((g.cap + e) as usize) {
                        *w = h;
                    }
                    (probes, keys) = (probes + p, keys + 1);
                    // Entries read are bounded by `to`; only probes past
                    // one an entry end a slice early.
                    if probes > keys && (at - from) as u64 + probes - keys >= budget {
                        (placed, stop) = (i + 1, true);
                        break;
                    }
                }
            }
            let _ = self.heap.set_values(entries, 2 * n, &kvs[..2 * placed]);
            n += u32::try_from(placed).unwrap_or(0);
            if stop {
                // Just past the last entry placed.
                let okv = self.heap.slot_words(o.entries).unwrap_or(&[]);
                let live = |&j: &usize| {
                    let key = okv.get(2 * j).and_then(|&b| Value::from_bits(b));
                    key.is_some_and(|x| x != Value::UNDEFINED)
                };
                let last = (start..at).filter(live).nth(placed - 1).unwrap_or(at);
                return Some((last + 1, n, probes - keys));
            }
        }
        Some((at.max(used), n, probes.saturating_sub(keys)))
    }

    /// Dict `r` takes the tables [`Vm::dict_move`] filled with `n`
    /// entries.
    pub(crate) fn dict_install(&mut self, d: Value, r: Ref, entries: Ref, index: Ref, n: u32) {
        let Some(o) = self.dv(r) else { return };
        // The version goes on (globals inline caches compare it).
        let version = o
            .index
            .and_then(|i| self.heap.raw_words(i).ok()?.last().copied())
            .unwrap_or(0);
        if let Ok(words) = self.heap.raw_words_mut(index)
            && let Some(x) = words.last_mut()
        {
            *x = version;
        }
        let inline = (o.inline as usize).min(LINEAR_MAX);
        if inline > 0 {
            // The entries moved out: drop the inline copies.
            let empty = [Value::UNDEFINED; 2 * LINEAR_MAX];
            let _ = self.heap.set_values(r, 0, &empty[..2 * inline]);
        }
        self.set_slot(r, 0, Value::from_ref(entries, ty::BUF as u8));
        self.set_slot(r, 1, Value::from_ref(index, ty::RAW as u8));
        self.set_counts(r, n, n);
        self.bump_version(d, r);
    }

    /// Makes room in dict `d` for `need` live entries, as CPython's
    /// `dict_merge` presizes: a small table resizes at once; for a big
    /// one, the capacity of the new tables the caller allocates and fills
    /// a slice per step ([`Vm::dict_move`], then [`Vm::dict_install`]).
    pub(crate) fn dict_reserve(&mut self, d: Value, need: usize) -> Result<Option<usize>, Raised> {
        let Some(r) = d.as_obj() else { return Ok(None) };
        let Some(dv) = self.dv(r) else {
            return Ok(None);
        };
        let more = need.saturating_sub(dv.len as usize);
        let room = match dv.index {
            Some(index) => self.heap.raw_words(index).map_or(0, |w| w.len() / 2),
            None if need > LINEAR_MAX => 0,
            None => return Ok(None),
        };
        if more == 0 || (dv.used as usize).saturating_add(more) <= room {
            return Ok(None);
        }
        if dv.used >= 1 << 12 || need >= 1 << 12 {
            return Ok(Some(cap_for(need)));
        }
        self.dict_resize(d, r, need, true, u64::MAX)?;
        Ok(None)
    }

    /// The key and value of pair `x`, element `i` of a `dict(...)` or
    /// `update` source: a list or tuple natively, an iterator or user
    /// object in the prelude twin.
    pub(crate) fn dict_pair(&mut self, x: Value, i: usize) -> Result<(Value, Value), Raised> {
        let w = self.seq_words(x);
        let (pair, n) = if let [k, v] = *w {
            let val = |b: u64| Value::from_bits(b).unwrap_or(Value::NONE);
            ([val(k), val(v)], 2)
        } else if x.is_obj_kind(ty::LIST as u8) || x.is_obj_kind(ty::TUPLE as u8) {
            ([Value::NONE; 2], w.len())
        } else {
            if self.is_iterator(x) || self.user_class(x).is_some() {
                // A one-shot pair: the twin must see it whole.
                return Err(self.need_py());
            }
            let items = match self.collect(x) {
                Ok(p) => p,
                Err(_) if !self.slow && self.exc_type(self.exc) == T::TypeError => {
                    return Err(self.err(
                        T::TypeError,
                        &alloc::format!(
                            "cannot convert dictionary update sequence element #{i} to a sequence"
                        ),
                    ));
                }
                Err(e) => return Err(e),
            };
            match *items {
                [k, v] => ([k, v], 2),
                _ => ([Value::NONE; 2], items.len()),
            }
        };
        if n != 2 {
            return Err(self.err(
                T::ValueError,
                &alloc::format!(
                    "dictionary update sequence element #{i} has length {n}; 2 is required"
                ),
            ));
        }
        Ok((pair[0], pair[1]))
    }

    /// `d[k] = v` for `k` of hash `h`, unless `k` is new and the table,
    /// past a few thousand entries, must grow first: then nothing is done
    /// and the value is `false` ([`Vm::dict_reserve`]).
    pub(crate) fn dict_set_room(
        &mut self,
        d: Value,
        k: Value,
        h: u64,
        v: Value,
    ) -> Result<bool, Raised> {
        let Some(r) = d.as_obj() else { return Ok(true) };
        let Some(dv) = self.dv(r) else {
            return Err(self.err(T::SystemError, "invalid dict table"));
        };
        let mut left = WALK;
        let found = self.probe_dv(dv, k, h, 0, &mut left, &mut |_, _| {});
        self.walked(left);
        let (slot, found) = found.map_err(|x| self.hard(x))?;
        if let Some(e) = found {
            self.set_slot(dv.entries, 2 * e + 1, v);
            return Ok(true);
        }
        if dv.index.is_some() && dv.used >= 1 << 12 && !self.has_room(&dv, k) {
            return Ok(false);
        }
        self.add_entry(d, r, dv, k, h, v, Some(slot), NOT_TOP)?;
        Ok(true)
    }

    /// Is `k` (hash `h`) a key of dict `d`?
    pub(crate) fn dict_has_hashed(&mut self, d: Value, k: Value, h: u64) -> Result<bool, Raised> {
        let Some(r) = d.as_obj() else {
            return Ok(false);
        };
        let found = self.probe(r, k, h);
        Ok(found.map_err(|x| self.hard(x))?.1.is_some())
    }

    /// The hash of entry `e` of dict `d` as its table stores it (computed
    /// for a table without an index, whose keys all hash natively).
    pub(crate) fn dict_hash_at(&self, d: Value, e: usize) -> Option<u64> {
        let dv = self.dv(d.as_obj()?)?;
        match dv.index {
            Some(i) => {
                let w = self.heap.raw_words(i).ok()?;
                w.get(w.len() / 2 + e).copied()
            }
            None => self.key_hash_pure(self.slot(dv.entries, 2 * u32::try_from(e).ok()?)),
        }
    }

    /// `del d[k]`; returns the removed value.
    pub(crate) fn dict_del(&mut self, d: Value, k: Value) -> Result<Option<Value>, Raised> {
        let h = self.key_hash(k)?;
        let Some(r) = d.as_obj() else { return Ok(None) };
        let (slot, found) = self.probe(r, k, h).map_err(|x| self.hard(x))?;
        let Some(e) = found else { return Ok(None) };
        let Some(dv) = self.dv(r) else {
            return Ok(None);
        };
        let v = self.slot(dv.entries, 2 * e + 1);
        let _ = self
            .heap
            .set_values(dv.entries, 2 * e, &[Value::UNDEFINED, Value::UNDEFINED]);
        if let Some(index) = dv.index
            && let Ok(words) = self.heap.raw_words_mut(index)
        {
            set_islot(words, slot, DELETED);
        }
        self.set_counts(r, dv.len.saturating_sub(1), dv.used);
        self.bump_own_version(r);
        Ok(Some(v))
    }

    /// Removes every entry. An indexed dict stays indexed (and keeps
    /// counting versions); another clears its entries in place.
    pub(crate) fn dict_clear(&mut self, d: Value) -> Result<(), Raised> {
        let Some(r) = d.as_obj() else { return Ok(()) };
        let Some(dv) = self.dv(r) else { return Ok(()) };
        if let Some(old) = dv.index {
            let version = self
                .heap
                .raw_words(old)
                .ok()
                .and_then(|w| w.last().copied())
                .unwrap_or(0);
            let (entries, index) = self.dict_tables(8)?;
            if let Ok(words) = self.heap.raw_words_mut(index)
                && let Some(x) = words.last_mut()
            {
                *x = version;
            }
            self.set_slot(r, 0, Value::from_ref(entries, ty::BUF as u8));
            self.set_slot(r, 1, Value::from_ref(index, ty::RAW as u8));
            self.set_counts(r, 0, 0);
            self.bump_version(d, r);
        } else {
            let empty = [Value::UNDEFINED; 2 * LINEAR_MAX];
            let n = (2 * dv.used as usize).min(empty.len());
            let _ = self.heap.set_values(dv.entries, 0, &empty[..n]);
            self.set_counts(r, 0, 0);
        }
        Ok(())
    }

    /// The dict that `new_dict_for(n)` followed by `dict_set` of each pair
    /// of `pairs` (`[k0, v0, k1, v1, ..]`) in order makes. Up to
    /// `LINEAR_MAX` `str` keys are deduplicated on the stack, comparing
    /// hashes before text, and the inline table is written in one
    /// allocation.
    #[cfg(feature = "json")]
    pub(crate) fn dict_from_pairs(&mut self, pairs: &[Value]) -> Result<Value, Raised> {
        let (pairs, _) = pairs.as_chunks::<2>();
        let n = pairs.len();
        if n > LINEAR_MAX || !pairs.iter().all(|&[k, _]| Vm::is_str(k)) {
            let d = self.new_dict_for(n)?;
            for &[k, v] in pairs {
                self.dict_set(d, k, v)?;
            }
            return Ok(d);
        }
        let mut kvs = [Value::UNDEFINED; 2 * LINEAR_MAX];
        let mut hs = [0u64; LINEAR_MAX];
        let mut used = 0;
        for &[k, v] in pairs {
            let h = self.key_hash(k)?;
            let mut at = None;
            let mut left = WALK;
            for (j, &hj) in hs.iter().enumerate().take(used) {
                let key = kvs[2 * j];
                if hj == h
                    && (key == k
                        || self
                            .key_eq(key, k, 1, &mut left)
                            .map_err(|x| self.hard(x))?)
                {
                    at = Some(j);
                    break;
                }
            }
            let j = at.unwrap_or(used);
            if at.is_none() {
                kvs[2 * j] = k;
                hs[j] = h;
                used += 1;
            }
            kvs[2 * j + 1] = v;
        }
        let used32 = u32::try_from(used).unwrap_or(0);
        let r = {
            let res =
                self.heap
                    .alloc_init(ty::DICT, 0, &kvs[..2 * n.max(1)], &[counts(used32, used32)]);
            self.hx(res)
        }?;
        Ok(Value::from_ref(r, ty::DICT as u8))
    }

    /// A shallow copy.
    pub(crate) fn dict_copy(&mut self, d: Value) -> Result<Value, Raised> {
        let n = self.new_dict_for(self.dict_len(d))?;
        for (k, v) in self.dict_items_charged(d) {
            self.dict_set(n, k, v)?;
        }
        Ok(n)
    }
}

pub(crate) fn num_eq(vm: &Vm, a: Value, b: Value, x: f64, y: f64) -> bool {
    // Exact int/float comparison: an int equals a float only if the float is
    // integral and equal to it exactly.
    match (vm.int_of(a), vm.int_of(b)) {
        (Some(i), None) => int_eq_float(i, y),
        (None, Some(j)) => int_eq_float(j, x),
        _ => x == y,
    }
}

/// Exact `i == f`.
pub(crate) fn int_eq_float(i: i64, f: f64) -> bool {
    if crate::ops::fract(f) != 0.0 || !f.is_finite() {
        return false;
    }
    #[allow(clippy::cast_precision_loss, reason = "range check")]
    if f < -9.223_372_036_854_776e18 || f >= 9.223_372_036_854_776e18 {
        return false;
    }
    #[allow(clippy::cast_possible_truncation, reason = "integral and in range")]
    let j = f as i64;
    i == j
}

/// CPython's xxHash-based tuple hash over already-hashed lanes; `with_len`
/// adds the length term (tuples, ranges), slices leave it out.
const XX_P5: u64 = 2_870_177_450_012_600_261;

/// The accumulator of [`xx_lane`] before the first lane.
pub(crate) const XX_SEED: u64 = XX_P5;

/// One lane of CPython's tuple hash (xxHash) folded into `acc`.
pub(crate) fn xx_lane(acc: u64, lane: i64) -> u64 {
    const P1: u64 = 11_400_714_785_074_694_791;
    const P2: u64 = 14_029_467_366_897_019_727;
    acc.wrapping_add(lane.cast_unsigned().wrapping_mul(P2))
        .rotate_left(31)
        .wrapping_mul(P1)
}

/// The hash from lanes folded into `acc`, with the length term for
/// `Some(len)` lanes.
pub(crate) fn xx_finish(mut acc: u64, len: Option<u64>) -> i64 {
    if let Some(n) = len {
        acc = acc.wrapping_add(n ^ (XX_P5 ^ 0x0035_D373));
    }
    if acc == u64::MAX {
        return 1_546_275_796;
    }
    acc.cast_signed()
}
