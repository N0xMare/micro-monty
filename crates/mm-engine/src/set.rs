//! `set`: CPython's open-addressing table (linear probes plus perturbation),
//! reproduced so iteration order matches CPython for the same hashes.
//!
//! ```text
//! set   values [keys: BUF of size, hashes: RAW of size]   raws [fill, used,
//!       frozen, finger]
//! slot  empty = (UNDEFINED, hash 0); dummy = (UNDEFINED, hash -1)
//! ```

use crate::dict::{Hard, WALK};
use crate::types::{T, ty};
use crate::vm::{Raised, Vm};
use alloc::vec::Vec;
use mm_core::{Ref, Value};

const FILL: u32 = 2;
const USED: u32 = 3;
/// Nonzero for a `frozenset` (same table, never mutated once built).
const FROZEN: u32 = 4;
/// Where `pop` looks first: past the slot it last took (CPython's
/// `finger`), so a `pop()` loop is linear, not quadratic.
const FINGER: u32 = 5;
pub(crate) const LINEAR_PROBES: usize = 9;
pub(crate) const PERTURB_SHIFT: u32 = 5;
const DUMMY: u64 = u64::MAX;

/// What a phase of a chunked set or dict operation does with each item of
/// its source ([`Vm::bulk`]); the numbers are the prelude's (`_bulk`).
pub(crate) mod act {
    /// `out[k] = v` for each entry of a dict.
    pub const DICT: u64 = 0;
    /// `out[k] = v` for each `(k, v)` pair of a sequence.
    pub const PAIRS: u64 = 1;
    /// `out[x] = other` for each item (`dict.fromkeys`).
    pub const KEYS: u64 = 2;
    /// `out.add(x)`.
    pub const ADD: u64 = 3;
    /// `out.discard(x)`.
    pub const DISCARD: u64 = 4;
    /// `x` discarded from `out` if there, else added.
    pub const TOGGLE: u64 = 5;
    /// `out.add(x)` if `x in other`.
    pub const AND: u64 = 6;
    /// `out.add(x)` if `x not in other`.
    pub const SUB: u64 = 7;
    /// The operation is `False` if `x not in other`.
    pub const SUBSET: u64 = 8;
    /// The operation is `False` if `x in other`.
    pub const DISJOINT: u64 = 9;
    /// `out` takes the table of set `src` (CPython's `set_swap_bodies`).
    pub const MOVE: u64 = 10;
    /// Flag: a dict source gives `(key, value)` tuples (an items view).
    /// It is read as a view: an `ADD` does not presize for it.
    pub const ITEMS: u64 = 1 << 13;
    /// Flag: a dict source is a keys view, read a key at a time as
    /// CPython's `set_update_iterable` does: an `ADD` does not presize.
    pub const VIEW: u64 = 1 << 14;
}

/// A bulk operation's value when its phases are done: a register (0 to
/// 5), or one of these.
pub(crate) mod res {
    pub const NONE: u64 = 6;
    pub const TRUE: u64 = 7;
}

/// A bulk phase: action `a` on register `out`, over the items of register
/// `src`, probing (or taking the value in) register `other`. A program is
/// up to four, 16 bits each, the first lowest.
pub(crate) const fn phase(a: u64, out: u64, src: u64, other: u64) -> u64 {
    1 << 15 | a | out << 4 | src << 7 | other << 10
}

/// Phase bits of [`Bulk::pc`] (the phase is bits 0-2): the phase began;
/// a set copy into an empty table goes slot for slot or by clean inserts
/// (CPython's `set_merge`).
const BEGUN: u64 = 1 << 3;
const SLOTS: u64 = 1 << 4;
const CLEAN: u64 = 1 << 5;

/// The largest set table under the object cap (a word per slot in its
/// key buffer and in its hashes, the header not counted).
const MAX_SIZE: usize = mm_core::MAX_OBJECT_BYTES as usize / 8;

/// Set tables at least this big get their key buffer and their hashes
/// in two steps: first touching 64 MiB takes about 30 ms [M]. A dict's
/// new tables (entries, then index) always come in two.
const SPLIT: usize = 1 << 22;

/// Old table slots or entries a rebuild moves per slice.
const REBUILD_SLICE: u64 = 1 << 12;

// A bulk step allocates a new table only in its first quarter
// (`bulk_phases`): below 4 units that quarter is empty and a table that
// must grow waits for ever (112 own cases hung at 1 to 3 [M, reviewer]).
const _: () = assert!(crate::nstep::STEP_WORK >= 4);

/// Raw words of builtin iterators, as [`Vm::iter_pos`] records them: up
/// to 8 in the first half; in the second, value slot 1 of each
/// `enumerate` whose count is past `i64` (index `u32::MAX`, the slot's
/// bits), which reading an item replaces.
type IterPos = [Option<(Ref, u32, u64)>; 16];

/// The work units of an items view's item: a step of 2M `(key, value)`
/// tuples built at one unit each took 40-55 ms against 7 ms for keys [M].
const TUPLE_ITEM: u64 = 6;

/// Entries of a set or dict past which a bulk item costs two units, not
/// one: its probe of a table that size misses the caches, so a step of
/// `STEP_WORK` inserts into a 1M-entry dict took 17-22 ms on an orb with
/// 64 ns random loads and 44-46 ms on one with 144 ns [M].
const WIDE: usize = 1 << 16;

/// Live entries from which a full dict or set grows in steps when one
/// item goes in ([`Vm::bulk_grow_top`]). Below, the one-step growth takes
/// at most 5.5-6.5 ms (at 131,072 entries, the slowest orb measured), and
/// stepping it costs about 40 Ir an entry more: 0.7% of the dict bench,
/// whose dict grows past 65,536 [M]. At 1,048,576 entries one step took
/// 42 ms, and a set's growth to 8M slots 90-101 ms [M].
pub(crate) const GROW_STEPS: usize = 1 << 18;

impl Vm {
    /// Whether a growth to new tables of `bytes` may go in steps: only
    /// while a 32nd of the memory limit is left beside them. Between two
    /// steps a collection can start an incremental mark with the old
    /// tables still in use, which holds all garbage until its remark;
    /// with little room that fails an allocation the one-step growth
    /// survives, as its collection comes after the tables are swapped and
    /// runs at once. `copy.deepcopy` of 600k lists beside 1M old ones
    /// (memo 2^19 to 2^20 entries: 38 MiB free, 32 MiB of tables) raised
    /// `MemoryError` at 256 MiB, and needs 242.5 MiB in one step [M]; a
    /// set's growth to 8M slots leaves 15 MiB [M] and goes in steps.
    pub(crate) fn grow_in_steps(&self, bytes: usize) -> bool {
        let left = self.heap.available_bytes().saturating_sub(bytes as u64);
        left >= self.heap.memory_limit() / 32
    }
}

/// A chunked set or dict operation between two steps (`NSTATE` op
/// `BULK`: these value slots, then `op, prog, pc, i, j, n, res`).
pub(crate) struct Bulk {
    /// The registers: sets, dicts, sources, a value.
    r: [Value; 6],
    /// The new tables of the current phase's `out` while it is rebuilt
    /// a slice per step (`UNDEFINED` otherwise); the old ones stay
    /// installed until the rebuild is done.
    nk: Value,
    nv: Value,
    prog: u64,
    /// The phase (bits 0-1) and [`BEGUN`], [`SLOTS`], [`CLEAN`].
    pc: u64,
    /// The position in the phase's source.
    i: u64,
    /// A rebuild's position in the old table, and entries it moved.
    j: u64,
    n: u64,
    res: u64,
    /// The step can still allocate a new table (it has done little work
    /// yet, or it has no limit); else growth waits for the next step.
    /// Not saved: each step sets it.
    room: bool,
    /// The operation goes on in steps (`top`): a new table that does not
    /// fit waits for a collection and the next step ([`Vm::oom_defer`]).
    /// Not saved.
    step: bool,
}

/// What a step of a bulk operation came to.
pub(crate) enum Flow {
    Done(Value),
    More,
    /// An item only Python code can decide (the prelude's `_bulk` takes it).
    Py,
}

/// What one item of a bulk phase came to.
enum Item {
    Next,
    /// Not taken: `out`'s table must grow first.
    Wait,
    Done(Value),
    /// The phase is done before its source is (an intersection holding
    /// all of the set it probes, as CPython's `set_intersection` stops).
    End,
}

impl Vm {
    /// A new empty set.
    pub(crate) fn new_set(&mut self) -> Result<Value, Raised> {
        let (k, h) = self.set_table(8)?;
        let r = {
            let res = self.heap.alloc_init(ty::SET, 0, &[k, h], &[0, 0, 0, 0]);
            self.hx_o(res)
        }?;
        Ok(Value::from_ref(r, ty::SET as u8))
    }

    fn set_table(&mut self, size: usize) -> Result<(Value, Value), Raised> {
        Ok((self.set_keys(size)?, self.set_hashes(size)?))
    }

    /// A table's key buffer of `size` slots, all empty.
    fn set_keys(&mut self, size: usize) -> Result<Value, Raised> {
        // Charged as the copy or rehash that fills it (docs/spec.md §5).
        if size > 8 {
            self.burn_result(size * 8, false);
        }
        Ok(Value::from_ref(self.alloc_buf(size)?, ty::BUF as u8))
    }

    /// A table's hashes of `size` slots, all 0.
    fn set_hashes(&mut self, size: usize) -> Result<Value, Raised> {
        if size > 8 {
            self.burn_result(size * 8, false);
        }
        let n = u32::try_from(size).map_err(|_| self.err(T::MemoryError, ""))?;
        let res = self.heap.alloc(ty::RAW, 0, 0, n);
        Ok(Value::from_ref(self.hx_o(res)?, ty::RAW as u8))
    }

    pub(crate) fn set_parts(&self, s: Value) -> Option<(Ref, Ref, Ref, usize)> {
        // Verified bytecode checks operand ranges, not their runtime types.
        // A tuple's references must not be interpreted as a hash table.
        if !Self::is_set(s) {
            return None;
        }
        let r = s.as_obj()?;
        let k = self.slot_o(r, 0).as_obj()?;
        let h = self.slot_o(r, 1).as_obj()?;
        Some((r, k, h, self.obj_len(k) as usize))
    }

    /// Number of elements.
    pub(crate) fn set_len(&self, s: Value) -> usize {
        s.as_obj()
            .map_or(0, |r| usize::try_from(self.raw_o(r, USED)).unwrap_or(0))
    }

    /// [`Vm::set_items`] for a caller that works on every element, charged
    /// one unit per element (docs/spec.md §5).
    pub(crate) fn set_items_charged(&mut self, s: Value) -> Vec<Value> {
        let items = self.set_items(s);
        if items.len() >= 8 {
            self.burn(items.len() as u64);
        }
        items
    }

    /// Elements in table order.
    pub(crate) fn set_items(&self, s: Value) -> Vec<Value> {
        let mut out = Vec::new();
        if let Some((_, k, _, size)) = self.set_parts(s) {
            for i in 0..size {
                let v = self.slot_o(k, u32::try_from(i).unwrap_or(0));
                if v != Value::UNDEFINED {
                    out.push(v);
                }
            }
        }
        out
    }

    pub(crate) fn set_items_hashed(&self, s: Value) -> Vec<(Value, u64)> {
        let mut out = Vec::new();
        if let Some((_, k, h, size)) = self.set_parts(s) {
            for i in 0..size {
                let i = u32::try_from(i).unwrap_or(0);
                let v = self.slot_o(k, i);
                if v != Value::UNDEFINED {
                    out.push((v, self.raw_o(h, i)));
                }
            }
        }
        out
    }

    /// The first element at slot `i` or after (for iterators) and its
    /// slot, or `None` and the table's size: one scan of the slots.
    pub(crate) fn set_next_at(&self, s: Value, i: usize) -> (usize, Option<Value>) {
        let Some((_, k, _, size)) = self.set_parts(s) else {
            return (i, None);
        };
        let keys = self.heap.slot_words(k).unwrap_or(&[]);
        let empty = Value::UNDEFINED.to_bits();
        match keys
            .get(i..size)
            .and_then(|w| w.iter().position(|&w| w != empty))
        {
            Some(j) => (i + j, keys.get(i + j).and_then(|&w| Value::from_bits(w))),
            None => (size.max(i), None),
        }
    }

    /// Finds `key` inside a walk at `depth` with `left` visits: the slot
    /// index if present; `Err` if a comparison needs Python code.
    fn set_find(
        &self,
        s: Value,
        key: Value,
        hash: u64,
        depth: u32,
        left: &mut u32,
    ) -> Result<Option<usize>, Hard> {
        self.set_find_with(s, key, hash, depth, left, &mut |_, _| {})
    }

    /// `set_find`, reporting entries whose keys only Python can compare.
    fn set_find_with(
        &self,
        s: Value,
        key: Value,
        hash: u64,
        depth: u32,
        left: &mut u32,
        undecided: &mut dyn FnMut(usize, Value),
    ) -> Result<Option<usize>, Hard> {
        let Some((_, k, h, size)) = self.set_parts(s) else {
            return Ok(None);
        };
        let mut deferred = false;
        let mask = size - 1;
        #[allow(clippy::cast_possible_truncation, reason = "masked")]
        let mut i = hash as usize & mask;
        let mut perturb = hash;
        loop {
            let probes = if i + LINEAR_PROBES <= mask {
                LINEAR_PROBES
            } else {
                0
            };
            for j in 0..=probes {
                let idx = u32::try_from(i + j).unwrap_or(0);
                let ek = self.slot_o(k, idx);
                let eh = self.raw_o(h, idx);
                if ek == Value::UNDEFINED && eh == 0 {
                    return if deferred { Err(Hard::Py) } else { Ok(None) };
                }
                if eh == hash && ek != Value::UNDEFINED {
                    if ek == key {
                        return Ok(Some(i + j));
                    }
                    match self.eq_hard(ek, key, depth + 1, left) {
                        Ok(true) => return Ok(Some(i + j)),
                        Ok(false) => {}
                        Err(Hard::Deep) => return Err(Hard::Deep),
                        Err(Hard::Py) => {
                            deferred = true;
                            undecided(i + j, ek);
                        }
                    }
                }
            }
            perturb >>= PERTURB_SHIFT;
            #[allow(clippy::cast_possible_truncation, reason = "masked")]
            {
                i = (i
                    .wrapping_mul(5)
                    .wrapping_add(1)
                    .wrapping_add(perturb as usize))
                    & mask;
            }
        }
    }

    /// [`Vm::set_find`] for a walk's twin, one visit charged.
    pub(crate) fn set_find_at(
        &self,
        s: Value,
        key: Value,
        hash: u64,
        depth: u32,
        left: &mut u32,
    ) -> Result<Option<usize>, Hard> {
        *left = left.saturating_sub(1);
        self.set_find(s, key, hash, depth, left)
    }

    /// The first element of set `s` at slot `i` or after, with its stored
    /// hash; `i` becomes its slot.
    pub(crate) fn set_entry_from(&self, s: Value, i: &mut usize) -> Option<(Value, u64)> {
        let (_, k, h, size) = self.set_parts(s)?;
        let keys = self.heap.slot_words(k).ok()?;
        let empty = Value::UNDEFINED.to_bits();
        let at = *i + keys.get(*i..size)?.iter().position(|&w| w != empty)?;
        *i = at;
        let x = Value::from_bits(*keys.get(at)?)?;
        Some((x, self.raw_o(h, u32::try_from(at).ok()?)))
    }

    fn set_hard(&mut self, h: Hard) -> Raised {
        match h {
            Hard::Py => self.need_py(),
            Hard::Deep => self.err(
                T::RecursionError,
                "maximum recursion depth exceeded in comparison",
            ),
        }
    }

    fn set_find_r(&mut self, s: Value, key: Value, hash: u64) -> Result<Option<usize>, Raised> {
        let mut left = WALK;
        let r = self.set_find(s, key, hash, 0, &mut left);
        self.walked(left);
        r.map_err(|h| self.set_hard(h))
    }

    /// `key in s`.
    pub(crate) fn set_contains(&mut self, s: Value, key: Value) -> Result<bool, Raised> {
        let h = self.set_key_hash(key)?;
        Ok(self.set_find_r(s, key, h)?.is_some())
    }

    /// The hash a lookup of `key` uses: a (mutable) `set` looks for the equal
    /// `frozenset`, as CPython's `set_contains`/`set_discard` do.
    fn set_key_hash(&mut self, key: Value) -> Result<u64, Raised> {
        // A subclass instance without its own `__hash__` is as unhashable
        // as the set it extends.
        let b = self.unbx(key);
        if b.is_obj_kind(ty::SET as u8)
            && !self.set_frozen(b)
            && (b == key || self.uflags(key) & crate::class::f::HASH == 0)
        {
            return Ok(self.frozenset_hash(b).cast_unsigned());
        }
        self.hash_as(key, "set element")
    }

    /// For the prelude: the slot of `key` (hash `h`) if native comparison
    /// finds it, else `[(slot, key), ...]` for Python to compare, else -1.
    /// `depth` is the set's nesting in a comparison walk (see
    /// [`crate::dict::walk_budget`]).
    pub(crate) fn set_probe_py(
        &mut self,
        s: Value,
        key: Value,
        h: u64,
        depth: u32,
    ) -> Result<Value, Raised> {
        let mut cands: Vec<(usize, Value)> = Vec::new();
        let budget = crate::dict::walk_budget(depth);
        let mut left = budget;
        let found = self.set_find_with(s, key, h, depth, &mut left, &mut |i, k| {
            cands.push((i, k));
        });
        self.walked_of(budget, left);
        match found {
            Ok(Some(i)) => self.new_int(i64::try_from(i).unwrap_or(0)),
            Ok(None) => Ok(Value::int(-1).unwrap_or(Value::NONE)),
            Err(Hard::Deep) => Err(self.set_hard(Hard::Deep)),
            Err(Hard::Py) => {
                let mut items = Vec::with_capacity(cands.len());
                for (i, k) in cands {
                    let iv = self.new_int(i64::try_from(i).unwrap_or(0))?;
                    items.push(self.alloc_tuple(&[iv, k])?);
                }
                self.new_list(&items)
            }
        }
    }

    /// Adds `key` (hash `h`, known absent) without comparing.
    pub(crate) fn set_add_new(&mut self, s: Value, key: Value, hash: u64) -> Result<(), Raised> {
        self.set_add_core(s, key, hash, true, false)
    }

    /// `s.clear()`: a new empty table, as CPython's `set_clear_internal`
    /// (the old one is not walked).
    pub(crate) fn set_clear(&mut self, s: Value) -> Result<(), Raised> {
        let (k, h) = self.set_table(8)?;
        if let Some((r, ..)) = self.set_parts(s) {
            self.set_slot_o(r, 0, k);
            self.set_slot_o(r, 1, h);
            self.set_raw_o(r, FILL, 0);
            self.set_raw_o(r, USED, 0);
        }
        Ok(())
    }

    /// Removes the element in slot `i`.
    pub(crate) fn set_remove_at(&mut self, s: Value, i: usize) {
        if let Some((r, k, h, size)) = self.set_parts(s)
            && i < size
        {
            let idx = u32::try_from(i).unwrap_or(0);
            if self.slot_o(k, idx) == Value::UNDEFINED {
                return;
            }
            self.set_slot_o(k, idx, Value::UNDEFINED);
            self.set_raw_o(h, idx, DUMMY);
            let used = self.raw_o(r, USED).saturating_sub(1);
            self.set_raw_o(r, USED, used);
        }
    }

    /// `s.add(key)` where the caller can hand a request on (`SET_ADD`,
    /// `set.add`): a big table grows in steps ([`Vm::bulk_grow_top`]).
    /// This, `set_add_hashed` and `set_add_inner`'s signature are main's
    /// text: `SET_ADD` reaching the stepping path through any other shape
    /// moved wasmtime's loop bench, which runs no `SET_ADD`, 13-16% [M].
    pub(crate) fn set_add(&mut self, s: Value, key: Value) -> Result<(), Raised> {
        let h = self.hash_as(key, "set element")?;
        self.set_add_hashed(s, key, h)
    }

    /// `s.add(key)` for a native that cannot hand a request on: grows in
    /// one step.
    pub(crate) fn set_add_plain(&mut self, s: Value, key: Value) -> Result<(), Raised> {
        let h = self.hash_as(key, "set element")?;
        self.set_add_core(s, key, h, false, false)
    }

    fn set_add_hashed(&mut self, s: Value, key: Value, hash: u64) -> Result<(), Raised> {
        self.set_add_inner(s, key, hash, false)
    }

    // Out of line: inlined into `Vm::dispatch` (SET_ADD), it cost the loop
    // and fib benches 4-10% Ir in register pressure [M].
    #[inline(never)]
    fn set_add_inner(
        &mut self,
        s: Value,
        key: Value,
        hash: u64,
        absent: bool,
    ) -> Result<(), Raised> {
        self.set_add_core(s, key, hash, absent, true)
    }

    /// Adds `key`; `top`: the caller can hand a request on, so a big
    /// table grows in steps.
    #[inline(never)]
    fn set_add_core(
        &mut self,
        s: Value,
        key: Value,
        hash: u64,
        absent: bool,
        top: bool,
    ) -> Result<(), Raised> {
        let Some((idx, minused)) = self.set_put(s, key, hash, absent)? else {
            return Ok(());
        };
        if top
            && self.set_len(s) > GROW_STEPS
            && self.grow_in_steps(Self::set_grow_size(minused, self.set_len(s)) * 16)
        {
            // Out again, and added back by the steps that grow the table.
            self.set_unput(s, idx);
            return self.bulk_grow_top(s, key, Value::NONE, res::NONE);
        }
        if let Err(error) = self.set_resize(s, minused) {
            // Resize allocates both replacement tables before publishing
            // either. Roll back this insertion on failure: catching repeated
            // MemoryErrors must not fill every slot and make probing hang.
            self.set_unput(s, idx);
            return Err(error);
        }
        Ok(())
    }

    /// Adds `key` (hash `hash`) to `s` unless present (`absent`: known
    /// not to be), without resizing: the slot it took and the `minused`
    /// to resize for when the table is now too full.
    #[inline]
    fn set_put(
        &mut self,
        s: Value,
        key: Value,
        hash: u64,
        absent: bool,
    ) -> Result<Option<(usize, usize)>, Raised> {
        let Some((r, k, h, size)) = self.set_parts(s) else {
            return Err(self.err(T::SystemError, "invalid set table"));
        };
        let mask = size - 1;
        #[allow(clippy::cast_possible_truncation, reason = "masked")]
        let mut i = hash as usize & mask;
        let mut perturb = hash;
        let mut freeslot = None;
        let slot = loop {
            let probes = if i + LINEAR_PROBES <= mask {
                LINEAR_PROBES
            } else {
                0
            };
            let mut found = None;
            for j in 0..=probes {
                let idx = u32::try_from(i + j).unwrap_or(0);
                let ek = self.slot_o(k, idx);
                let eh = self.raw_o(h, idx);
                if ek == Value::UNDEFINED && eh == 0 {
                    found = Some(i + j);
                    break;
                }
                if !absent && ek != Value::UNDEFINED && eh == hash {
                    if ek == key {
                        return Ok(None);
                    }
                    let mut left = WALK;
                    let r = self.eq_hard(ek, key, 1, &mut left);
                    self.walked(left);
                    match r {
                        Ok(true) => return Ok(None),
                        Ok(false) => {}
                        Err(e) => return Err(self.set_hard(e)),
                    }
                }
                // The last dummy probed, as CPython's `set_add_entry`
                // keeps it.
                if eh == DUMMY {
                    freeslot = Some(i + j);
                }
            }
            if let Some(f) = found {
                break f;
            }
            perturb >>= PERTURB_SHIFT;
            #[allow(clippy::cast_possible_truncation, reason = "masked")]
            {
                i = (i
                    .wrapping_mul(5)
                    .wrapping_add(1)
                    .wrapping_add(perturb as usize))
                    & mask;
            }
        };
        let used = self.raw_o(r, USED) + 1;
        self.set_raw_o(r, USED, used);
        if let Some(f) = freeslot {
            let f = u32::try_from(f).unwrap_or(0);
            self.set_slot_o(k, f, key);
            self.set_raw_o(h, f, hash);
            return Ok(None);
        }
        let idx = u32::try_from(slot).unwrap_or(0);
        self.set_slot_o(k, idx, key);
        self.set_raw_o(h, idx, hash);
        let fill = self.raw_o(r, FILL) + 1;
        self.set_raw_o(r, FILL, fill);
        if fill * 5 < (mask as u64) * 3 {
            return Ok(None);
        }
        let minused = if used > 50_000 { used * 2 } else { used * 4 };
        Ok(Some((slot, usize::try_from(minused).unwrap_or(usize::MAX))))
    }

    /// Takes back the insertion [`Vm::set_put`] made into empty slot `i`.
    fn set_unput(&mut self, s: Value, i: usize) {
        if let Some((r, k, h, _)) = self.set_parts(s) {
            let idx = u32::try_from(i).unwrap_or(0);
            self.set_slot_o(k, idx, Value::UNDEFINED);
            self.set_raw_o(h, idx, 0);
            let (fill, used) = (self.raw_o(r, FILL), self.raw_o(r, USED));
            self.set_raw_o(r, FILL, fill.saturating_sub(1));
            self.set_raw_o(r, USED, used.saturating_sub(1));
        }
    }

    /// The table size CPython's `set_table_resize` picks for `minused`.
    fn set_size_for(minused: usize) -> usize {
        let mut size = 8usize;
        while size <= minused {
            size <<= 1;
        }
        size
    }

    /// The size a set of `used` elements grows to for `minused`: CPython's,
    /// or the largest table under the object cap while they fit it (past
    /// the cap CPython's tables are bigger, so the order differs anyway).
    fn set_grow_size(minused: usize, used: usize) -> usize {
        let size = Self::set_size_for(minused);
        if size > MAX_SIZE && used * 5 < (MAX_SIZE - 1) * 3 {
            MAX_SIZE
        } else {
            size
        }
    }

    fn set_resize(&mut self, s: Value, minused: usize) -> Result<(), Raised> {
        let size = Self::set_grow_size(minused, self.set_len(s));
        let (kv, hv) = self.set_table(size)?;
        let Some((r, ok, oh, osize)) = self.set_parts(s) else {
            return Ok(());
        };
        let (Some(k), Some(h)) = (kv.as_obj(), hv.as_obj()) else {
            return Ok(());
        };
        // Old slots in order, read in place (no copy of the table).
        self.set_rehash(ok, oh, k, h, 0, osize);
        let n = self.raw_o(r, USED);
        self.set_slot_o(r, 0, kv);
        self.set_slot_o(r, 1, hv);
        self.set_raw_o(r, FILL, n);
        self.set_raw_o(r, USED, n);
        Ok(())
    }

    /// Inserts the live entries of old table slots `from..to` (keys `ok`,
    /// hashes `oh`) into the clean table `k`, `h`, in slot order.
    fn set_rehash(&mut self, ok: Ref, oh: Ref, k: Ref, h: Ref, from: usize, to: usize) {
        let mask = self.obj_len(k) as usize - 1;
        for i in from..to {
            let i = u32::try_from(i).unwrap_or(u32::MAX);
            let key = self.slot_o(ok, i);
            if key != Value::UNDEFINED {
                let hash = self.raw_o(oh, i);
                self.insert_clean(k, h, mask, key, hash);
            }
        }
    }

    fn insert_clean(&mut self, k: Ref, h: Ref, mask: usize, key: Value, hash: u64) {
        #[allow(clippy::cast_possible_truncation, reason = "masked")]
        let mut i = hash as usize & mask;
        let mut perturb = hash;
        // Once `perturb` is spent the probes visit every slot, so a table
        // with room has a free one within these; a full one (a forged
        // image's) drops the key.
        let mut left = mask.saturating_add(64);
        loop {
            if self.slot_o(k, u32::try_from(i).unwrap_or(0)) == Value::UNDEFINED {
                break;
            }
            if left == 0 {
                return;
            }
            left -= 1;
            let mut hit = None;
            if i + LINEAR_PROBES <= mask {
                for j in 1..=LINEAR_PROBES {
                    if self.slot_o(k, u32::try_from(i + j).unwrap_or(0)) == Value::UNDEFINED {
                        hit = Some(i + j);
                        break;
                    }
                }
            }
            if let Some(x) = hit {
                i = x;
                break;
            }
            perturb >>= PERTURB_SHIFT;
            #[allow(clippy::cast_possible_truncation, reason = "masked")]
            {
                i = (i
                    .wrapping_mul(5)
                    .wrapping_add(1)
                    .wrapping_add(perturb as usize))
                    & mask;
            }
        }
        let idx = u32::try_from(i).unwrap_or(0);
        self.set_slot_o(k, idx, key);
        self.set_raw_o(h, idx, hash);
    }

    /// `s.discard(key)`; whether it was present.
    pub(crate) fn set_discard(&mut self, s: Value, key: Value) -> Result<bool, Raised> {
        let hash = self.set_key_hash(key)?;
        let Some(i) = self.set_find_r(s, key, hash)? else {
            return Ok(false);
        };
        if let Some((r, k, h, _)) = self.set_parts(s) {
            let idx = u32::try_from(i).unwrap_or(0);
            self.set_slot_o(k, idx, Value::UNDEFINED);
            self.set_raw_o(h, idx, DUMMY);
            let used = self.raw_o(r, USED).saturating_sub(1);
            self.set_raw_o(r, USED, used);
        }
        Ok(true)
    }

    /// A new empty set, a frozenset if `like` is one (set operators return
    /// the left operand's type).
    pub(crate) fn new_set_like(&mut self, like: Value) -> Result<Value, Raised> {
        let out = self.new_set()?;
        self.set_freeze_like(out, like);
        Ok(out)
    }

    /// Is set `s` a `frozenset`?
    pub(crate) fn set_frozen(&self, s: Value) -> bool {
        s.as_obj().is_some_and(|r| self.raw_o(r, FROZEN) != 0)
    }

    /// Makes `out` a frozenset if `like` is one (set operators return the
    /// left operand's type).
    pub(crate) fn set_freeze_like(&mut self, out: Value, like: Value) {
        if self.set_frozen(like) {
            self.set_freeze(out);
        }
    }

    /// Makes set `s` a frozenset.
    pub(crate) fn set_freeze(&mut self, s: Value) {
        if let Some(r) = s.as_obj() {
            self.set_raw_o(r, FROZEN, 1);
        }
    }

    /// CPython's `frozenset_hash`.
    pub(crate) fn frozenset_hash(&self, s: Value) -> i64 {
        let shuffle = |h: u64| ((h ^ 0x055b_4db3) ^ (h << 16)).wrapping_mul(3_644_798_167);
        let mut h: u64 = 0;
        // Read in place: the table is not copied.
        if let Some((_, k, hs, _)) = self.set_parts(s) {
            let keys = self.heap.slot_words(k).unwrap_or(&[]);
            let hashes = self.heap.raw_words(hs).unwrap_or(&[]);
            for (&key, &kh) in keys.iter().zip(hashes) {
                if key != Value::UNDEFINED.to_bits() {
                    h ^= shuffle(kh);
                }
            }
        }
        h ^= (self.set_len(s) as u64 + 1).wrapping_mul(1_927_868_237);
        h ^= (h >> 11) ^ (h >> 25);
        h = h.wrapping_mul(69_069).wrapping_add(907_133_923);
        if h == u64::MAX {
            590_923_713
        } else {
            h.cast_signed()
        }
    }

    /// `set(iterable)` contents added to `s` (`{*x}`), in steps: a request
    /// is left when long.
    pub(crate) fn set_update(&mut self, s: Value, other: Value) -> Result<(), Raised> {
        let (src, items) = self.view_src(other)?;
        self.bulk(&[s, src], phase(act::ADD | items, 0, 1, 0), res::NONE, true)
            .map(|_| ())
    }

    /// A source a bulk operation reads in place (a list, tuple, set, dict,
    /// range, or a builtin iterator that runs no Python code, an item at a
    /// time as CPython reads it), or a list of the items of any other
    /// iterable.
    pub(crate) fn bulk_src(&mut self, v: Value) -> Result<Value, Raised> {
        if matches!(
            v.obj_kind().map(u16::from),
            Some(ty::LIST | ty::TUPLE | ty::SET | ty::DICT | ty::RANGE)
        ) || self.iter_pos(v, &mut [None; 16], &mut 0, 0)
        {
            return Ok(v);
        }
        let items = self.collect(v)?;
        self.new_list(&items)
    }

    /// `a <op> b` for two sets (`&`, `|`, `-`, `^`; in place for a set
    /// `a`, which a frozenset never is), in steps when `top`.
    pub(crate) fn set_binop(
        &mut self,
        op: mm_compile::ast::BinOp,
        a: Value,
        b: Value,
        inplace: bool,
        top: bool,
    ) -> Result<Value, Raised> {
        use mm_compile::ast::BinOp;
        let inplace = inplace && !self.set_frozen(a);
        let (la, lb) = (self.set_len(a), self.set_len(b));
        if inplace {
            // CPython's `set_ior`, `set_iand`, `set_isub`, `set_ixor`.
            let prog = match op {
                BinOp::BitOr => phase(act::ADD, 0, 1, 0),
                BinOp::Sub => phase(act::DISCARD, 0, 1, 0),
                BinOp::BitXor => phase(act::TOGGLE, 0, 1, 0),
                _ => {
                    let tmp = self.new_set()?;
                    let (small, big) = if lb > la { (a, b) } else { (b, a) };
                    let regs = [a, small, big, tmp];
                    let prog = phase(act::AND, 3, 1, 2) | phase(act::MOVE, 0, 3, 0) << 16;
                    return self.bulk(&regs, prog, 0, top);
                }
            };
            return self.bulk(&[a, b], prog, 0, top);
        }
        let out = self.new_set_like(a)?;
        let regs = [out, a, b];
        let prog = match op {
            // CPython's `set_or` and `set_union`: a copy, which `a` itself
            // as the other operand leaves as it is.
            BinOp::BitOr if a == b => phase(act::ADD, 0, 1, 0),
            BinOp::BitOr => phase(act::ADD, 0, 1, 0) | phase(act::ADD, 0, 2, 0) << 16,
            BinOp::BitAnd if lb > la => phase(act::AND, 0, 1, 2),
            BinOp::BitAnd => phase(act::AND, 0, 2, 1),
            // CPython's `set_copy_and_difference` when `b` is much smaller.
            BinOp::Sub if la >> 2 > lb => {
                phase(act::ADD, 0, 1, 0) | phase(act::DISCARD, 0, 2, 0) << 16
            }
            BinOp::Sub => phase(act::SUB, 0, 1, 2),
            // CPython's `set_symmetric_difference`: a copy of `b`, then
            // each element of `a` toggled in.
            _ => phase(act::ADD, 0, 2, 0) | phase(act::TOGGLE, 0, 1, 0) << 16,
        };
        self.bulk(&regs, prog, 0, top)
    }

    /// `a <= b`, raising.
    pub(crate) fn set_issubset_r(&mut self, a: Value, b: Value) -> Result<bool, Raised> {
        let mut left = WALK;
        let r = self.set_issubset(a, b, 0, &mut left);
        self.walked(left);
        r.map_err(|h| self.set_hard(h))
    }

    /// The set of `items`, as a `{...}` display builds it.
    pub(crate) fn set_from(&mut self, items: &[Value]) -> Result<Value, Raised> {
        let s = self.new_set()?;
        for x in items {
            self.set_add_plain(s, *x)?;
        }
        Ok(s)
    }

    /// Requires `v` to be a set for a binary set operator.
    pub(crate) fn is_set(v: Value) -> bool {
        v.is_obj_kind(ty::SET as u8)
    }

    /// `s.pop()`: the first element in table order, charging the slots
    /// scanned to find it (it used to copy the whole table).
    pub(crate) fn set_pop(&mut self, s: Value) -> Result<Value, Raised> {
        let Some((r, k, h, size)) = self.set_parts(s).filter(|_| self.set_len(s) > 0) else {
            return Err(self.err(T::KeyError, "pop from an empty set"));
        };
        // CPython's `set_pop_impl`: from the finger on, wrapping.
        let from = usize::try_from(self.raw_o(r, FINGER)).unwrap_or(0) & size.wrapping_sub(1);
        let words = self.heap.slot_words(k).unwrap_or(&[]);
        let empty = Value::UNDEFINED.to_bits();
        let live = |w: &[u64]| w.iter().position(|&w| w != empty);
        let (i, scanned) = if let Some(j) = live(words.get(from..).unwrap_or(&[])) {
            (Some(from + j), j)
        } else {
            let j = live(words.get(..from).unwrap_or(&[]));
            (j, size - from + j.unwrap_or(from))
        };
        let key = i
            .and_then(|i| words.get(i))
            .and_then(|&w| Value::from_bits(w));
        self.burn(scanned as u64 / 8);
        let (Some(i), Some(key)) = (i, key) else {
            return Err(self.err(T::KeyError, "pop from an empty set"));
        };
        let idx = u32::try_from(i).unwrap_or(0);
        self.set_slot_o(k, idx, Value::UNDEFINED);
        self.set_raw_o(h, idx, DUMMY);
        let used = self.raw_o(r, USED).saturating_sub(1);
        self.set_raw_o(r, USED, used);
        self.set_raw_o(r, FINGER, i as u64 + 1);
        Ok(key)
    }
}

impl Vm {
    /// `out[x] = v` (a dict) or `out.add(x)` (a set), `x` absent and
    /// `out`'s table full, by a caller that can hand a request on: the
    /// table grows a slice per step as `update` grows it, then `x` goes
    /// in; the value is register `res` (`res::NONE`, or 2 for `v`). Keys
    /// hashed in Python never come here: their stores run in the prelude
    /// twins, whose growth is one step.
    pub(crate) fn bulk_grow_top(
        &mut self,
        out: Value,
        x: Value,
        v: Value,
        res: u64,
    ) -> Result<(), Raised> {
        let src = self.alloc_tuple(&[x])?;
        let prog = if Self::is_set(out) {
            phase(act::ADD, 0, 1, 0)
        } else {
            phase(act::KEYS, 0, 1, 2)
        };
        self.bulk(&[out, src, v], prog, res, true).map(|_| ())
    }

    /// Runs bulk program `prog` ([`phase`]) on registers `regs`; its value
    /// is register or [`res`] value `res`. With `top` (the caller can hand
    /// its value to a request) it stops after [`crate::nstep::STEP_WORK`]
    /// and goes on in `_steps` (`nstep::op::BULK`), and an item that needs
    /// Python code hands the rest to the prelude's `_bulk`; without, it
    /// runs to the end and such an item raises for the caller's twin.
    pub(crate) fn bulk(
        &mut self,
        regs: &[Value],
        prog: u64,
        res: u64,
        top: bool,
    ) -> Result<Value, Raised> {
        let mut r = [Value::NONE; 6];
        for (o, &v) in r.iter_mut().zip(regs) {
            *o = v;
        }
        let mut p = prog;
        while p & 0xFFFF != 0 {
            let s = ((p >> 7) & 7) as usize;
            if let Some(&it) = r.get(s).filter(|&&v| Self::one_shot(v)) {
                if top {
                    // Read in place: a call that runs again after a
                    // collection would miss the items taken.
                    self.oom_rerun = self.op_stamp();
                } else {
                    // The caller's twin runs the call again from the
                    // start: it gets the items (`Vm::unconsume`).
                    let items = self.collect(it)?;
                    r[s] = self.new_list(&items)?;
                }
            }
            p >>= 16;
        }
        let mut b = Bulk {
            r,
            nk: Value::UNDEFINED,
            nv: Value::UNDEFINED,
            prog,
            pc: 0,
            i: 0,
            j: 0,
            n: 0,
            res,
            room: true,
            step: top,
        };
        match self.bulk_run(&mut b, top)? {
            Flow::Done(v) => Ok(v),
            Flow::Py if top => self.bulk_py(&b),
            Flow::Py => Err(self.need_py()),
            Flow::More => {
                let [a, c, d, e, f, g] = b.r;
                let vals = [a, c, d, e, f, g, b.nk, b.nv];
                let raw = [b.prog, b.pc, b.i, b.j, b.n, b.res];
                self.chunk_rest(crate::nstep::op::BULK, &vals, &raw)
            }
        }
    }

    /// One step of a bulk operation ([`Vm::bulk`]).
    pub(crate) fn bulk_step(&mut self, state: Value) -> Result<crate::nstep::Step, Raised> {
        use crate::nstep::{Step, op};
        let Some((v, [_, prog, pc, i, j, n, res])) = self.nstate_get::<8, 7>(op::BULK, state)
        else {
            return Err(self.bad_step());
        };
        let [a, c, d, e, f, g, nk, nv] = v;
        let mut b = Bulk {
            r: [a, c, d, e, f, g],
            nk,
            nv,
            prog,
            pc,
            i,
            j,
            n,
            res,
            room: true,
            step: true,
        };
        if !self.bulk_tables_ok(&b) {
            return Err(self.bad_step());
        }
        match self.bulk_run(&mut b, true)? {
            Flow::Done(v) => Ok(Step::Done(v)),
            Flow::Py => Ok(Step::Done(self.bulk_py(&b)?)),
            Flow::More => {
                if let Some(r) = state.as_obj() {
                    self.set_slot_o(r, 6, b.nk);
                    self.set_slot_o(r, 7, b.nv);
                }
                self.nstate_put(state, 2, &[b.pc, b.i, b.j, b.n]);
                Ok(Step::More)
            }
        }
    }

    /// Whether the new tables of a rebuild that a bulk state read from an
    /// image holds fit the table they rebuild: a set's keys and hashes of
    /// one power-of-two size with room for its elements, a dict's entries
    /// and index for at least its entries, and progress inside the old
    /// table. Images are trusted but checked (docs/spec.md §9.3): another
    /// object here made the rehash probe a full table for ever.
    fn bulk_tables_ok(&self, b: &Bulk) -> bool {
        if b.nk == Value::UNDEFINED {
            return true;
        }
        let p = b.prog.checked_shr(16 * (b.pc & 7) as u32).unwrap_or(0);
        let out =
            b.r.get(((p >> 4) & 7) as usize)
                .copied()
                .unwrap_or(Value::NONE);
        if b.nk == Value::NONE {
            // The new table comes next (`bulk_grow`): `j` is its size, a
            // set's a power of two from 8 with room for its elements,
            // small enough that what it allocates and charges is counted
            // without overflow.
            return b.j <= 1 << 40
                && (!Self::is_set(out)
                    || (b.j.is_power_of_two() && b.j >= 8 && b.j > self.set_len(out) as u64));
        }
        let words = |v: Value, t: u16| {
            let r = v.as_obj()?;
            let hdr = self.heap.header(r).ok()?;
            (hdr.type_id() == t).then(|| hdr.len() as usize)
        };
        let Some(nk) = words(b.nk, ty::BUF) else {
            return false;
        };
        let nv = if b.nv == Value::NONE {
            None
        } else {
            match words(b.nv, ty::RAW) {
                Some(n) => Some(n),
                None => return false,
            }
        };
        if let Some((_, _, _, size)) = self.set_parts(out) {
            return nk.is_power_of_two()
                && nk >= 8
                && nk > self.set_len(out)
                && match nv {
                    // Its new keys are still being marked empty
                    // (`set_keys_fill`): `j` counts the new table's slots.
                    None => b.j <= nk as u64,
                    Some(n) => n == nk && b.j <= size as u64,
                };
        }
        let cap = nk / 2;
        out.is_obj_kind(ty::DICT as u8)
            && cap.is_power_of_two()
            && cap >= 8
            && cap >= self.dict_len(out)
            && nv.is_none_or(|n| n == 2 * cap + 1)
            && b.j <= self.dict_used(out) as u64
            && b.n <= b.j
    }

    /// The rest of bulk operation `b` in the prelude's `_bulk`, from the
    /// item that needs Python code.
    fn bulk_py(&mut self, b: &Bulk) -> Result<Value, Raised> {
        let regs = self.alloc_tuple(&b.r)?;
        let mut args = alloc::vec![regs];
        for w in [b.prog, b.pc & 7, b.i, b.res] {
            args.push(self.new_int(w.cast_signed())?);
        }
        let f = self.prelude_fn("_bulk")?;
        Ok(self.request_call(f, args, crate::cont::Cont::None))
    }

    /// Steps of `b` until it is done, or, with `limit`, until a step's
    /// work is spent.
    fn bulk_run(&mut self, b: &mut Bulk, limit: bool) -> Result<Flow, Raised> {
        let start = self.work;
        let mut units = 0u64;
        let r = self.bulk_phases(b, limit, start, &mut units);
        self.burn(units);
        match r {
            Err(Raised) if self.slow => {
                self.slow = false;
                Ok(Flow::Py)
            }
            r => r,
        }
    }

    #[allow(clippy::too_many_lines, reason = "one loop over the actions")]
    fn bulk_phases(
        &mut self,
        b: &mut Bulk,
        limit: bool,
        start: u64,
        units: &mut u64,
    ) -> Result<Flow, Raised> {
        let reg = |b: &Bulk, i: u64| b.r.get((i & 7) as usize).copied().unwrap_or(Value::NONE);
        loop {
            let spent = *units + self.work.wrapping_sub(start);
            if limit && spent >= crate::nstep::STEP_WORK {
                return Ok(Flow::More);
            }
            let p = b.prog.checked_shr(16 * (b.pc & 7) as u32).unwrap_or(0) & 0xFFFF;
            if p == 0 {
                return Ok(Flow::Done(match b.res {
                    res::NONE => Value::NONE,
                    res::TRUE => Value::TRUE,
                    x => reg(b, x),
                }));
            }
            let (a, out, src, other) = (p & 15, reg(b, p >> 4), reg(b, p >> 7), reg(b, p >> 10));
            let left = if limit {
                crate::nstep::STEP_WORK.saturating_sub(spent)
            } else {
                u64::MAX
            };
            // A new table is allocated in a step's first quarter: zeroing
            // 128 MB takes most of a step.
            b.room = !limit || spent < crate::nstep::STEP_WORK / 4;
            if b.nk == Value::NONE || b.nv == Value::NONE {
                // `out`'s new table comes at the start of a step, after
                // the collection one that did not fit waits for.
                if !b.room || self.oom_deferred() {
                    return Ok(Flow::More);
                }
                if b.nk == Value::NONE {
                    self.bulk_grow(b, out, usize::try_from(b.j).unwrap_or(usize::MAX))?;
                } else {
                    let size = b.nk.as_obj().map_or(8, |k| self.obj_len(k) as usize);
                    let nv = if Self::is_set(out) {
                        // A big table's keys come unfilled
                        // (`set_rebuild_start`): marked empty a slice per
                        // step (`j` slots so far), then its hashes.
                        if b.j < size as u64 {
                            *units += self.set_keys_fill(b.nk, &mut b.j, left);
                            continue;
                        }
                        self.set_hashes(size).inspect(|_| b.j = 0)
                    } else {
                        self.dict_index_table(size / 2)
                            .map(|t| Value::from_ref(t, ty::RAW as u8))
                    };
                    match nv {
                        Ok(nv) => {
                            b.nv = nv;
                            self.oom_defer_at = 0;
                        }
                        Err(Raised) if b.step && self.oom_defer() => return Ok(Flow::More),
                        Err(e) => return Err(e),
                    }
                }
                continue;
            }
            if b.nk != Value::UNDEFINED {
                // `out`'s table is rebuilt a slice per step; the items wait.
                *units += self.bulk_rebuild(b, out, left.min(REBUILD_SLICE));
                continue;
            }
            if b.pc & BEGUN == 0 {
                b.pc |= BEGUN;
                b.i = 0;
                let view = p & (act::ITEMS | act::VIEW) != 0;
                if let Some(v) = self.bulk_begin(b, a, view, out, src, other)? {
                    return Ok(Flow::Done(v));
                }
                continue;
            }
            if b.pc & SLOTS != 0 {
                *units += self.bulk_slots(b, out, src, left.saturating_mul(8));
                continue;
            }
            if src.is_obj_kind(ty::LIST as u8) || src.is_obj_kind(ty::TUPLE as u8) {
                // A list or tuple: a batch of items read at once.
                let mut buf = [Value::UNDEFINED; 64];
                let n = {
                    let w = self.seq_words(src);
                    let w = w.get(b.i as usize..).unwrap_or(&[]);
                    for (o, &x) in buf.iter_mut().zip(w) {
                        *o = Value::from_bits(x).unwrap_or(Value::NONE);
                    }
                    w.len().min(buf.len())
                };
                if n == 0 {
                    self.bulk_phase_done(b, a, out)?;
                    continue;
                }
                let unit = self.bulk_unit(out, other);
                for &x in &buf[..n] {
                    b.room = !limit
                        || *units + self.work.wrapping_sub(start) < crate::nstep::STEP_WORK / 4;
                    *units += unit;
                    match self.bulk_act(b, a, out, other, b.i as usize, x, Value::NONE, None)? {
                        Item::Next => b.i += 1,
                        Item::Wait => break,
                        Item::Done(v) => return Ok(Flow::Done(v)),
                        Item::End => {
                            b.pc = (b.pc & 7) + 1;
                            break;
                        }
                    }
                    // A rebuild the item started goes first; a spent step
                    // stops.
                    if b.nk != Value::UNDEFINED
                        || limit
                            && *units + self.work.wrapping_sub(start) >= crate::nstep::STEP_WORK
                    {
                        break;
                    }
                }
                continue;
            }
            let items = p & act::ITEMS != 0;
            // An iterator is read as CPython reads it, an item at a time:
            // one that fails stays taken, one that waits for the new table
            // or needs Python code is read again.
            let mut pos = [None; 16];
            let one_shot = Self::one_shot(src) && self.iter_pos(src, &mut pos, &mut 0, 0);
            let Some((at, x, v, h)) = self.bulk_item(src, b.i, items)? else {
                self.bulk_phase_done(b, a, out)?;
                continue;
            };
            let tuple =
                items || src.is_obj_kind(ty::ZIP as u8) || src.is_obj_kind(ty::ENUMERATE as u8);
            *units += self.bulk_unit(out, other) + if tuple { TUPLE_ITEM - 1 } else { 0 };
            let r = self.bulk_act(b, a, out, other, at, x, v, h);
            if one_shot && (matches!(r, Ok(Item::Wait)) || r.is_err() && self.slow) {
                for &(o, i, w) in pos.iter().flatten() {
                    match (i, Value::from_bits(w)) {
                        (u32::MAX, Some(v)) => self.set_slot_o(o, 1, v),
                        _ => self.set_raw_o(o, i, w),
                    }
                }
            }
            match r? {
                Item::Next => b.i = at as u64 + 1,
                Item::Wait => {}
                Item::Done(v) => return Ok(Flow::Done(v)),
                Item::End => b.pc = (b.pc & 7) + 1,
            }
        }
    }

    /// The work units of a bulk item whose table is `out` or `other`
    /// (a set, a dict or neither): one, or two past [`WIDE`] entries.
    fn bulk_unit(&self, out: Value, other: Value) -> u64 {
        let len = |v: Value| {
            if Self::is_set(v) {
                self.set_len(v)
            } else {
                self.dict_len(v)
            }
        };
        1 + u64::from(len(out).max(len(other)) >= WIDE)
    }

    /// Phase `a` of `b` has read all its source: the next one begins, but
    /// a `DISCARD` first rebuilds a table more than a quarter dummies, as
    /// CPython's `set_difference_update_internal` ends (once it is
    /// rebuilt, its source is still read, and the phase ends here again).
    fn bulk_phase_done(&mut self, b: &mut Bulk, a: u64, out: Value) -> Result<(), Raised> {
        if a == act::DISCARD
            && let Some(minused) = self.set_dummies_minused(out)
        {
            let new = Self::set_grow_size(minused, self.set_len(out));
            return self.bulk_grow(b, out, new);
        }
        b.pc = (b.pc & 7) + 1;
        Ok(())
    }

    /// The `minused` CPython's `set_difference_update_internal` rebuilds
    /// set `s` for at its end, if more than a quarter of its table is
    /// dummies.
    fn set_dummies_minused(&self, s: Value) -> Option<usize> {
        let (r, _, _, size) = self.set_parts(s)?;
        let (fill, used) = (self.raw_o(r, FILL), self.raw_o(r, USED));
        if fill.saturating_sub(used) <= (size as u64).saturating_sub(1) / 4 {
            return None;
        }
        let used = usize::try_from(used).unwrap_or(usize::MAX);
        Some(if used > 50_000 { used * 2 } else { used * 4 })
    }

    /// Set `out` takes the table of set `src`, a temporary (CPython's
    /// `set_swap_bodies`; `out` keeps its finger).
    pub(crate) fn set_move(&mut self, out: Value, src: Value) {
        if let (Some(o), Some(s)) = (out.as_obj(), src.as_obj())
            && Self::is_set(out)
            && Self::is_set(src)
        {
            for i in 0..2 {
                let v = self.slot_o(s, i);
                self.set_slot_o(o, i, v);
            }
            for i in [FILL, USED] {
                let w = self.raw_o(s, i);
                self.set_raw_o(o, i, w);
            }
        }
    }

    /// The end of a `difference_update` in the prelude twins (`_bulk`,
    /// `_set_difference_update`): [`Vm::set_dummies_minused`] at once.
    pub(crate) fn set_settle(&mut self, s: Value) -> Result<(), Raised> {
        match self.set_dummies_minused(s) {
            Some(minused) => self.set_resize(s, minused),
            None => Ok(()),
        }
    }

    /// The entries set or dict `src` gives a merge.
    fn merge_len(&self, src: Value) -> u64 {
        if Self::is_set(src) {
            self.set_len(src) as u64
        } else {
            self.dict_len(src) as u64
        }
    }

    /// The table size, and the `minused` behind it, that `set_merge`
    /// presizes set `out` to before taking in `n` entries of a set or a
    /// dict, if the table changes.
    fn merge_size(&self, out: Value, n: u64) -> Option<(usize, usize)> {
        let (r, _, _, size) = self.set_parts(out)?;
        let (fill, used) = (self.raw_o(r, FILL), self.raw_o(r, USED));
        if (fill + n) * 5 < (size as u64 - 1) * 3 {
            return None;
        }
        // Presized at most to the cap: past it, only growth the items
        // need (they may overlap) can fail.
        let minused = usize::try_from((used + n) * 2).unwrap_or(usize::MAX);
        let new = Self::set_size_for(minused).min(MAX_SIZE);
        (new > size || fill > used).then_some((new, minused))
    }

    /// Set `s` presized, at once, as a merge of set or dict `src` presizes
    /// it (a keys view as its dict): for the prelude twins of a bulk `ADD`
    /// phase and of `dictviews_to_set`.
    pub(crate) fn set_presize(&mut self, s: Value, src: Value) -> Result<(), Raised> {
        // A keys view as its dict (`dictviews_to_set`).
        let src = if src.is_obj_kind(ty::VIEW as u8) && self.type_of(src) == T::dict_keys {
            self.vslot(src, 0)
        } else {
            src
        };
        let n = self.merge_len(src);
        match self.merge_size(s, n).filter(|_| n != 0 && s != src) {
            Some((_, minused)) => self.set_resize(s, minused),
            None => Ok(()),
        }
    }

    /// A phase begins: presizing `out` as CPython does (not for a dict
    /// read as a `view`), or an action done whole (`MOVE`), or a value
    /// already decided.
    #[inline(never)]
    fn bulk_begin(
        &mut self,
        b: &mut Bulk,
        a: u64,
        view: bool,
        out: Value,
        src: Value,
        other: Value,
    ) -> Result<Option<Value>, Raised> {
        let set_src = Self::is_set(src) || src.is_obj_kind(ty::DICT as u8);
        match a {
            act::MOVE => {
                self.set_move(out, src);
                b.pc = (b.pc & 7) + 1;
            }
            act::SUBSET if Self::is_set(src) && Self::is_set(other) => {
                if self.set_len(src) > self.set_len(other) {
                    return Ok(Some(Value::FALSE));
                }
            }
            act::ADD if set_src && src != out && !view => {
                // `set_merge` / `set_update_internal` with a dict.
                let n = self.merge_len(src);
                if n == 0 {
                    b.pc = (b.pc & 7) + 1;
                    return Ok(None);
                }
                if let Some((new, _)) = self.merge_size(out, n) {
                    self.bulk_grow(b, out, new)?;
                }
                self.bulk_mode(b, out, src);
            }
            act::DICT | act::KEYS if set_src && out.is_obj_kind(ty::DICT as u8) => {
                if a == act::DICT && (src == out || self.dict_len(src) == 0) {
                    b.pc = (b.pc & 7) + 1;
                    return Ok(None);
                }
                let n = if Self::is_set(src) {
                    self.set_len(src)
                } else {
                    self.dict_len(src)
                };
                let need = (self.dict_len(out) + n).min(crate::dict::MAX_CAP);
                self.bulk_grow(b, out, need)?;
            }
            _ => {}
        }
        Ok(None)
    }

    /// Whether set `out`'s table takes another element without growing.
    fn set_room(&self, out: Value) -> bool {
        self.set_parts(out).is_some_and(|(r, _, _, size)| {
            self.raw_o(r, FILL).saturating_add(1).saturating_mul(5)
                < (size as u64).saturating_sub(1).saturating_mul(3)
        })
    }

    /// Picks how an `ADD` phase from set `src` fills `out`, once `out` has
    /// its size: slot for slot into an empty table of the same size
    /// without dummies, clean inserts into another empty table.
    fn bulk_mode(&self, b: &mut Bulk, out: Value, src: Value) {
        if b.nk != Value::UNDEFINED || !Self::is_set(src) || b.i != 0 {
            return;
        }
        let (Some((r, _, _, size)), Some((o, _, _, osize))) =
            (self.set_parts(out), self.set_parts(src))
        else {
            return;
        };
        if self.raw_o(r, FILL) == 0 {
            b.pc |= if size == osize && self.raw_o(o, FILL) == self.raw_o(o, USED) {
                SLOTS
            } else {
                CLEAN
            };
        }
    }

    /// A slice of the rebuild of `out`'s table: at most `left` old slots
    /// (a set) or entries (a dict) moved; installs the new table once all
    /// are. The work done.
    #[inline(never)]
    fn bulk_rebuild(&mut self, b: &mut Bulk, out: Value, left: u64) -> u64 {
        let (Some(nk), Some(nv)) = (b.nk.as_obj(), b.nv.as_obj()) else {
            b.nk = Value::UNDEFINED;
            return 1;
        };
        let to = b.j.saturating_add(left.max(1));
        let Some(r) = out.as_obj() else { return 1 };
        if Self::is_set(out) {
            let Some((_, ok, oh, osize)) = self.set_parts(out) else {
                return 1;
            };
            let to = usize::try_from(to).unwrap_or(usize::MAX).min(osize);
            self.set_rehash(ok, oh, nk, nv, b.j as usize, to);
            let done = to as u64 - b.j;
            b.j = to as u64;
            if to == osize {
                let n = self.raw_o(r, USED);
                self.set_slot_o(r, 0, b.nk);
                self.set_slot_o(r, 1, b.nv);
                self.set_raw_o(r, FILL, n);
                b.nk = Value::UNDEFINED;
                b.nv = Value::UNDEFINED;
                let src = Self::bulk_src_reg(b);
                self.bulk_mode(b, out, src);
            }
            return done.max(1);
        }
        let (done, installed) =
            self.dict_grow_slice(out, b.nk, b.nv, &mut b.j, &mut b.n, left.max(1));
        if installed {
            b.nk = Value::UNDEFINED;
            b.nv = Value::UNDEFINED;
        }
        done.max(1)
    }

    /// A slice of dict `d`'s growth, for a chunked native that inserts
    /// into a dict it builds and cannot hand the growth to BULK (json's
    /// decoder). The contract, step by step:
    ///
    /// 1. When an insert finds the table full (`dict_set_room` gives
    ///    `false`), `dict_reserve(d, len + 1)` gives the new capacity
    ///    `cap`. Allocate `dict_entries_table(cap)` and, in the next step
    ///    (each is 16 x `cap` bytes, 32 MiB at 2^21),
    ///    `dict_index_table(cap)`, as BUF and RAW values. Keep `d` and both
    ///    tables in the state's value slots, so a collection or a snapshot
    ///    between steps keeps them.
    /// 2. Each step calls this with the tables and two raw words of the
    ///    state, `at` (old entries read) and `n` (entries moved), both 0
    ///    at first, and `left`, the step's work units still to spend: it
    ///    moves up to `left` old entries and gives the units it did and
    ///    whether it installed the new tables (then drop them from the
    ///    state).
    /// 3. Until then the old table stays installed and in use: nothing may
    ///    write to `d`, not even a value of an existing key, or a moved
    ///    entry is lost. Then insert the pending key as usual
    ///    (`dict_set_room` or `dict_set`).
    ///
    /// The state's layout (`nstep::STEPS`) grows by the two values and
    /// two words; the snapshot build id moves with it.
    pub(crate) fn dict_grow_slice(
        &mut self,
        d: Value,
        entries: Value,
        index: Value,
        at: &mut u64,
        n: &mut u64,
        left: u64,
    ) -> (u64, bool) {
        let (Some(r), Some(nk), Some(nv)) = (d.as_obj(), entries.as_obj(), index.as_obj()) else {
            return (1, true);
        };
        let used = self.dict_used(d) as u64;
        let to = usize::try_from(at.saturating_add(left).min(used)).unwrap_or(usize::MAX);
        let moved = u32::try_from(*n).unwrap_or(u32::MAX);
        let (end, moved) = self.dict_move(r, nk, nv, *at as usize, to, moved);
        let done = end as u64 - *at;
        (*at, *n) = (end as u64, u64::from(moved));
        if end as u64 >= used {
            self.dict_install(d, r, nk, nv, moved);
            return (done, true);
        }
        (done, false)
    }

    /// The source register of `b`'s current phase.
    /// Whether `v` is a builtin iterator a bulk source may read in place.
    pub(crate) fn one_shot(v: Value) -> bool {
        matches!(
            v.obj_kind().map(u16::from),
            Some(ty::ITER | ty::ZIP | ty::ENUMERATE | ty::REVERSED)
        )
    }

    /// Records in `pos` (from `*n` on) the position of builtin iterator
    /// `it` and of the ones it reads (a `zip`'s, an `enumerate`'s), so an
    /// item can be read again: `false` if one of them can run Python code,
    /// or they nest deeper or hold more words than `pos` takes.
    fn iter_pos(&self, it: Value, pos: &mut IterPos, n: &mut usize, depth: u32) -> bool {
        let Some(r) = it.as_obj().filter(|_| depth < 4) else {
            return false;
        };
        let (words, inner): (&[u32], _) = match it.obj_kind().map(u16::from) {
            Some(ty::ITER) => (&[2, 3, 4], Value::UNDEFINED),
            Some(ty::REVERSED) => (&[1], Value::UNDEFINED),
            Some(ty::ENUMERATE) => {
                if self.raw_o(r, 2) == i64::MAX.cast_unsigned() {
                    // Its count is in slot 1 (`Vm::enum_big`). Each
                    // enumerate here also takes a word below, so the
                    // second half has room.
                    let free = pos
                        .get_mut(8..)
                        .and_then(|h| h.iter_mut().find(|p| p.is_none()));
                    let Some(p) = free else {
                        return false;
                    };
                    *p = Some((r, u32::MAX, self.slot_o(r, 1).to_bits()));
                }
                (&[2], self.slot_o(r, 0))
            }
            Some(ty::ZIP) => (&[], self.vslot_o(it, 0)),
            _ => return false,
        };
        for &i in words {
            let Some(p) = pos.get_mut(..8).and_then(|h| h.get_mut(*n)) else {
                return false;
            };
            *p = Some((r, i, self.raw_o(r, i)));
            *n += 1;
        }
        if inner == Value::UNDEFINED {
            return true;
        }
        if !inner.is_obj_kind(ty::TUPLE as u8) {
            return self.iter_pos(inner, pos, n, depth + 1);
        }
        let ws = self.seq_words(inner);
        ws.len() <= 8
            && ws
                .iter()
                .all(|&w| Value::from_bits(w).is_some_and(|v| self.iter_pos(v, pos, n, depth + 1)))
    }

    fn bulk_src_reg(b: &Bulk) -> Value {
        let p = b.prog.checked_shr(16 * (b.pc & 7) as u32).unwrap_or(0);
        b.r.get(((p >> 7) & 7) as usize)
            .copied()
            .unwrap_or(Value::NONE)
    }

    /// New tables of `size` slots for set `out` of `b`, filled a slice per
    /// step ([`Vm::bulk_rebuild`]). A big table's keys come unfilled and
    /// its hashes later (`nv` is `NONE`), so neither faults in its pages
    /// in one step: the keys are marked empty a slice per step
    /// ([`Vm::set_keys_fill`]), then the hashes come, zero as allocated.
    fn set_rebuild_start(&mut self, b: &mut Bulk, size: usize) -> Result<(), Raised> {
        let (k, h) = if size >= SPLIT {
            self.burn_result(size * 8, false);
            (
                Value::from_ref(self.alloc_items(size)?, ty::BUF as u8),
                Value::NONE,
            )
        } else {
            self.set_table(size)?
        };
        (b.nk, b.nv, b.j) = (k, h, 0);
        Ok(())
    }

    /// Marks slots `j..` of new key table `k` empty, at most `left` units'
    /// worth (a unit per 8 slots); the units done.
    fn set_keys_fill(&mut self, k: Value, j: &mut u64, left: u64) -> u64 {
        let Some(k) = k.as_obj() else { return 1 };
        let to = j
            .saturating_add(left.max(1).saturating_mul(8))
            .min(u64::from(self.obj_len(k)));
        let _ = self.heap.clear_values(k, *j as u32, to as u32);
        let done = (to - *j).div_ceil(8);
        *j = to;
        done.max(1)
    }

    /// A slice of a slot-for-slot copy of set `src` into `out` (same size,
    /// `out` empty): at most `left` slots. The work done (a unit per 8).
    #[inline(never)]
    fn bulk_slots(&mut self, b: &mut Bulk, out: Value, src: Value, left: u64) -> u64 {
        let (Some((r, k, h, size)), Some((o, ok, oh, _))) =
            (self.set_parts(out), self.set_parts(src))
        else {
            b.pc = (b.pc & 7) + 1;
            return 1;
        };
        let to = usize::try_from(b.i.saturating_add(left.max(256)))
            .unwrap_or(usize::MAX)
            .min(size);
        let mut at = b.i as usize;
        let (mut tmp, mut hs) = ([Value::UNDEFINED; 256], [0u64; 256]);
        while at < to {
            let n = (to - at).min(256);
            {
                let keys = self.heap.slot_words(ok).unwrap_or(&[]);
                let hashes = self.heap.raw_words(oh).unwrap_or(&[]);
                for (t, &w) in tmp[..n].iter_mut().zip(keys.get(at..).unwrap_or(&[])) {
                    *t = Value::from_bits(w).unwrap_or(Value::UNDEFINED);
                }
                for (t, &w) in hs[..n].iter_mut().zip(hashes.get(at..).unwrap_or(&[])) {
                    *t = w;
                }
            }
            let _ = self.heap.set_values(k, at as u32, &tmp[..n]);
            if let Some(d) = self
                .heap
                .raw_words_mut(h)
                .ok()
                .and_then(|w| w.get_mut(at..at + n))
            {
                d.copy_from_slice(&hs[..n]);
            }
            at += n;
        }
        let done = (at as u64 - b.i) / 8 + 1;
        b.i = at as u64;
        if at >= size {
            let (fill, used) = (self.raw_o(o, FILL), self.raw_o(o, USED));
            self.set_raw_o(r, FILL, fill);
            self.set_raw_o(r, USED, used);
            b.pc = (b.pc & 7) + 1;
        }
        done
    }

    /// Item `i` on of bulk source `src`: its position, the item, the value
    /// of a dict entry, the hash a set or dict stores; `None` past the end.
    /// A dict gives `(key, value)` tuples with `items`.
    #[allow(clippy::type_complexity, reason = "one item")]
    pub(crate) fn bulk_item(
        &mut self,
        src: Value,
        i: u64,
        items: bool,
    ) -> Result<Option<(usize, Value, Value, Option<u64>)>, Raised> {
        let i = usize::try_from(i).unwrap_or(usize::MAX);
        if let Some((_, k, h, size)) = self.set_parts(src) {
            let keys = self.heap.slot_words(k).unwrap_or(&[]);
            let empty = Value::UNDEFINED.to_bits();
            let Some(at) = keys
                .get(i..size)
                .and_then(|w| w.iter().position(|&w| w != empty))
                .map(|j| i + j)
            else {
                return Ok(None);
            };
            let x = keys.get(at).and_then(|&w| Value::from_bits(w));
            let hash = self.raw_o(h, u32::try_from(at).unwrap_or(u32::MAX));
            return Ok(x.map(|x| (at, x, Value::NONE, Some(hash))));
        }
        if src.is_obj_kind(ty::DICT as u8) {
            let used = self.dict_used(src);
            let mut at = i;
            while at < used {
                if let Some((k, v)) = self.dict_entry(src, at) {
                    if items {
                        let t = self.alloc_tuple(&[k, v])?;
                        return Ok(Some((at, t, v, None)));
                    }
                    return Ok(Some((at, k, v, self.dict_hash_at(src, at))));
                }
                at += 1;
            }
            return Ok(None);
        }
        if src.is_obj_kind(ty::RANGE as u8) {
            // Item `i` computed, not read: a range is never copied.
            if i as u64 >= self.range_len(src) {
                return Ok(None);
            }
            let (start, _, step) = self.range_parts(src);
            let x = (i as i64).wrapping_mul(step).wrapping_add(start);
            return Ok(Some((i, self.new_int(x)?, Value::NONE, None)));
        }
        if Self::one_shot(src) {
            // The next item, whatever `i`.
            return Ok(self.iter_next(src)?.map(|x| (i, x, Value::NONE, None)));
        }
        let x = self
            .seq_words(src)
            .get(i)
            .and_then(|&w| Value::from_bits(w));
        Ok(x.map(|x| (i, x, Value::NONE, None)))
    }

    /// Is `x` (hash `h`) in `other`, a set or a dict's keys?
    fn bulk_has(&mut self, other: Value, x: Value, h: u64) -> Result<bool, Raised> {
        if Self::is_set(other) {
            return Ok(self.set_find_r(other, x, h)?.is_some());
        }
        self.dict_has_hashed(other, x, h)
    }

    /// Adds `x` (hash `h`) to set `out`; when the table is then too full,
    /// its rebuild starts.
    fn bulk_add(
        &mut self,
        b: &mut Bulk,
        out: Value,
        x: Value,
        h: u64,
        absent: bool,
    ) -> Result<(), Raised> {
        let Some((slot, minused)) = self.set_put(out, x, h, absent)? else {
            return Ok(());
        };
        let size = Self::set_grow_size(minused, self.set_len(out));
        // A big table, or one this step has no room for, comes at the start
        // of the next. `x` stays where it is meanwhile and moves with the
        // others in slot order, as in CPython's `set_add_entry`, which
        // keeps it when the resize fails too: taken out and added again
        // after the rebuild, it would come last (`list(set(l))` differed
        // from CPython's past 3/5 of a big table).
        if !b.room || size >= SPLIT {
            (b.nk, b.j) = (Value::NONE, size as u64);
            return Ok(());
        }
        let r = self.bulk_grow(b, out, size);
        if r.is_err() {
            self.set_unput(out, slot);
        }
        r
    }

    /// Starts growing `out`'s table to `n` slots (a set) or for `n`
    /// entries (a dict): now if the step has room, else at the start of
    /// the next (`nk` is `NONE` and `j` is `n` until then).
    fn bulk_grow(&mut self, b: &mut Bulk, out: Value, n: usize) -> Result<(), Raised> {
        if !b.room {
            (b.nk, b.j) = (Value::NONE, n as u64);
            return Ok(());
        }
        b.nk = Value::UNDEFINED;
        let r = if Self::is_set(out) {
            self.set_rebuild_start(b, n)
        } else {
            self.dict_reserve(out, n).and_then(|cap| {
                if let Some(cap) = cap {
                    // The index comes next (`nv` is `NONE`).
                    let e = self.dict_entries_table(cap)?;
                    (b.nk, b.nv, b.j, b.n) = (Value::from_ref(e, ty::BUF as u8), Value::NONE, 0, 0);
                }
                Ok(())
            })
        };
        // A table that does not fit comes at the start of a later step,
        // after a collection (`Vm::oom_defer`).
        if r.is_err() && b.step && self.oom_defer() {
            (b.nk, b.j) = (Value::NONE, n as u64);
            return Ok(());
        }
        if r.is_ok() {
            self.oom_defer_at = 0;
        }
        r
    }

    /// Action `a` of a phase on item `x` at `at` (dict value `v`, stored
    /// hash `h`).
    #[allow(clippy::too_many_arguments, reason = "the item and the phase")]
    #[inline(never)]
    fn bulk_act(
        &mut self,
        b: &mut Bulk,
        a: u64,
        out: Value,
        other: Value,
        at: usize,
        x: Value,
        v: Value,
        h: Option<u64>,
    ) -> Result<Item, Raised> {
        let set_hash = |vm: &mut Vm, x: Value| match h {
            Some(h) => Ok(h),
            None => vm.hash_as(x, "set element"),
        };
        match a {
            // Clean inserts while the presized table has room: one a forged
            // image made smaller fills (each insert then probes all of it),
            // so past that the items go in as any others, the table growing.
            act::ADD if b.pc & CLEAN != 0 && self.set_room(out) => {
                if let Some((r, k, hs, size)) = self.set_parts(out) {
                    self.insert_clean(k, hs, size - 1, x, h.unwrap_or(0));
                    for i in [FILL, USED] {
                        let n = self.raw_o(r, i) + 1;
                        self.set_raw_o(r, i, n);
                    }
                }
            }
            act::ADD => {
                let h = set_hash(self, x)?;
                self.bulk_add(b, out, x, h, false)?;
            }
            act::DISCARD => {
                // As CPython's `set_discard_key`: a set element is
                // unhashable here, unlike `set.discard`'s argument.
                let h = set_hash(self, x)?;
                if let Some(i) = self.set_find_r(out, x, h)? {
                    self.set_remove_at(out, i);
                }
            }
            act::TOGGLE => {
                let h = set_hash(self, x)?;
                match self.set_find_r(out, x, h)? {
                    Some(i) => self.set_remove_at(out, i),
                    None => {
                        self.bulk_add(b, out, x, h, true)?;
                    }
                }
            }
            act::AND | act::SUB | act::SUBSET | act::DISJOINT => {
                let h = if a == act::AND && Self::is_set(other) {
                    // `set_intersection` hashes as `hash()` does.
                    match h {
                        Some(h) => h,
                        None => self.hash(x)?.cast_unsigned(),
                    }
                } else if Self::is_set(other) {
                    set_hash(self, x)?
                } else {
                    match h {
                        Some(h) => h,
                        None => self.key_hash(x)?,
                    }
                };
                let has = self.bulk_has(other, x, h)?;
                match a {
                    act::AND | act::SUB if has == (a == act::AND) => {
                        self.bulk_add(b, out, x, h, false)?;
                        // Holding all of `other`, it reads no further, but
                        // not before a rebuild that add started has run:
                        // the table's order is CPython's only after it.
                        if a == act::AND
                            && Self::is_set(other)
                            && self.set_len(out) >= self.set_len(other)
                            && b.nk == Value::UNDEFINED
                        {
                            return Ok(Item::End);
                        }
                    }
                    act::SUBSET if !has => return Ok(Item::Done(Value::FALSE)),
                    act::DISJOINT if has => return Ok(Item::Done(Value::FALSE)),
                    _ => {}
                }
            }
            _ => {
                let (k, v, h) = match a {
                    act::DICT => (x, v, h),
                    act::KEYS => (x, other, h),
                    _ => {
                        let (k, v) = self.dict_pair(x, at)?;
                        (k, v, None)
                    }
                };
                let h = match h {
                    Some(h) => h,
                    None => self.key_hash(k)?,
                };
                if !self.dict_set_room(out, k, h, v)? {
                    let need = self.dict_len(out) + 1;
                    self.bulk_grow(b, out, need)?;
                    if b.nk != Value::UNDEFINED {
                        return Ok(Item::Wait);
                    }
                    self.dict_set(out, k, v)?;
                }
            }
        }
        Ok(Item::Next)
    }
}
