//! Native comparison walks: `==`, ordering, and the key probes and subset
//! tests inside them, over builtin data (docs/spec.md §5).
//!
//! One loop runs a walk over an explicit stack of frames: a container
//! comparison is a frame that hands its items to child frames, and a
//! lookup inside it (a dict entry matched in the other dict, a set element
//! found in the other set) is a frame too. Nesting costs memory charged to
//! the arena limit (the first [`INLINE`] frames sit in the loop's own
//! frame), not Rust stack, so any nesting the depth limit admits runs on
//! a small host thread.
//!
//! The walk visits what the recursive walk it replaced visited, in the
//! same order and at the same nesting: each visit spends one unit of the
//! caller's `left` budget, nesting past [`MAX_NEST`] is
//! [`OrdErr::Deep`] (`RecursionError`), and a pair only Python can decide
//! (a user `__eq__`, or a spent budget) is [`OrdErr::Py`], so fuel charges
//! and handoffs to the prelude twins are unchanged; identical sequence
//! items, which identity decides, spend one unit per [`SAME`]. A walk whose stack
//! cannot grow within the memory limit is [`OrdErr::Py`] as well: the
//! twin goes on in bytecode, where frame admission raises `MemoryError`.

use crate::class::f;
use crate::dict::{DELETED, Geo, Hard, islot, num_eq};
use crate::ops::{OrdErr, cmp_int_float};
use crate::scratch::NativeVec;
use crate::set::{LINEAR_PROBES, PERTURB_SHIFT};
use crate::types::{T, ty};
use crate::vm::Vm;
use core::cmp::Ordering;
use mm_core::{Heap, Ref, Value};

/// Deepest nesting a walk follows.
const MAX_NEST: u32 = 1000;

/// Frames kept in the walk loop's own frame before the stack spills to
/// admitted scratch memory.
const INLINE: usize = 8;

/// Identical items a sequence walk passes over (identity decides them,
/// as `PyObject_RichCompareBool` does) cost one visit per block of this
/// many, so comparing long runs of shared items spends fuel, and runs out
/// of budget, in proportion to the items scanned.
const SAME: usize = 64;

/// Passes identical items from `i` (whose pair is identical) in sequence
/// words `xa`, `xb`: whole blocks of [`SAME`] identical pairs at one visit
/// of `left` each, then the identical pairs before the next differing one
/// free. `Ok` is the next index to look at; `Err` the index where the
/// budget ran out.
#[inline]
pub(crate) fn pass_same(xa: &[u64], xb: &[u64], i: u32, left: &mut u32) -> Result<u32, u32> {
    let at = |j: usize| u32::try_from(j).unwrap_or(u32::MAX);
    let mut j = i as usize;
    while let (Some(ca), Some(cb)) = (xa.get(j..j + SAME), xb.get(j..j + SAME)) {
        if ca != cb {
            let p = ca.iter().zip(cb).position(|(x, y)| x != y).unwrap_or(0);
            return Ok(at((j + p).max(i as usize + 1)));
        }
        let Some(rest) = left.checked_sub(1) else {
            return Err(at(j));
        };
        *left = rest;
        j += SAME;
    }
    Ok(at(j.max(i as usize + 1)))
}

impl Vm {
    /// Spends the extra walk visits for comparing `n` bytes of two strs,
    /// two bytes-likes or two big ints: one per 64, so long texts spend fuel and budget as
    /// byte work does (docs/spec.md §5). The comparison is decided even when
    /// that is more than the budget left; the excess is charged with the
    /// walk's visits (`walked_of`).
    #[inline]
    pub(crate) fn spend_text(&self, left: &mut u32, n: usize) {
        if n >= 64 {
            let units = n as u64 / 64;
            let over = units.saturating_sub(u64::from(*left));
            if over > 0 {
                self.text_over
                    .set(self.text_over.get().saturating_add(over));
            }
            *left = left.saturating_sub(u32::try_from(units).unwrap_or(u32::MAX));
        }
    }
}

/// The bytes an equality test of `x` and `y` compares (none when the
/// lengths differ).
#[inline]
fn eq_bytes(x: &[u8], y: &[u8]) -> usize {
    if x.len() == y.len() { x.len() } else { 0 }
}

/// What a finished frame (or a leaf) hands its parent.
#[derive(Clone, Copy)]
enum Res {
    Bool(bool),
    Ord(Option<Ordering>),
    /// A lookup: the value of the dict entry found, or the set element.
    Found(Option<Value>),
}

type Out = Result<Res, OrdErr>;

/// A comparison or lookup decided at once, or the frame that decides it.
enum Start {
    Leaf(Out),
    Push(Frame),
}

/// What a child frame is computing for its parent.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Wait {
    No,
    Lookup,
    Eq,
}

/// A comparison or lookup in progress. Each holds where it is, so the
/// loop can run a child and then resume it with the child's result.
#[derive(Clone, Copy)]
enum Frame {
    /// `==` of two lists or two tuples of equal length (or the three parts
    /// of two slices) from item `i` on.
    Seq {
        a: Value,
        b: Value,
        i: u32,
        depth: u32,
        slice: bool,
    },
    /// `==` of two dicts of equal length: `a`'s entries from `e` on looked
    /// up in `b`, then `v` (entry `e`'s value) compared with what was found.
    Dict {
        a: Value,
        b: Value,
        e: u32,
        depth: u32,
        v: Value,
        wait: Wait,
    },
    /// `==` of two keys views (or two items views): `da`'s entries from `e`
    /// on looked up in `db`.
    Views {
        da: Value,
        db: Value,
        e: u32,
        depth: u32,
        items: bool,
        v: Value,
        wait: Wait,
    },
    /// `==` of a keys or items view of `d` and set `st`: the set's
    /// elements from slot `i` on looked up in `d`.
    ViewSet {
        st: Value,
        d: Value,
        i: u32,
        depth: u32,
        items: bool,
        want: Value,
        wait: Wait,
    },
    /// `a <= b` for two sets: `a`'s elements from slot `i` on found in `b`.
    Subset {
        a: Value,
        b: Value,
        i: u32,
        depth: u32,
    },
    /// Key `k` (hash `h`) in an indexed dict table: probe slot `i`, `n`
    /// probe steps left; `e` is the entry whose key a child compares.
    Probe {
        entries: Ref,
        index: Ref,
        k: Value,
        h: u64,
        depth: u32,
        i: u32,
        n: u32,
        perturb: u64,
        e: u32,
        deferred: bool,
        wait: bool,
    },
    /// Key `k` (hash `h`) in a dict table without an index, entry `e` on.
    Scan {
        entries: Ref,
        used: u32,
        k: Value,
        h: u64,
        depth: u32,
        e: u32,
        deferred: bool,
        wait: bool,
    },
    /// Key `k` without a native hash in dict `d`: each entry compared.
    Linear {
        d: Value,
        k: Value,
        e: u32,
        depth: u32,
        wait: bool,
    },
    /// Element `key` (hash `hash`) in set table `keys`/`hashes`: probe run
    /// `i`, position `j` in it.
    SetFind {
        keys: Ref,
        hashes: Ref,
        mask: u32,
        key: Value,
        hash: u64,
        depth: u32,
        i: u32,
        j: u32,
        perturb: u64,
        deferred: bool,
        wait: bool,
    },
    /// Order of two lists or two tuples from item `i` on: `wait` 1 while a
    /// child orders item `i`, 2 while it tests the pair for `==`.
    OrdSeq {
        a: Value,
        b: Value,
        i: u32,
        depth: u32,
        wait: u8,
    },
    /// Order of two sets: `a <= b` (stage 1), then `b <= a` (stage 2).
    OrdSet {
        a: Value,
        b: Value,
        depth: u32,
        stage: u8,
        sub: bool,
    },
    /// Order of builtin-subclass instances `a`, `b` as their values `x`,
    /// `y`; unorderable values name the instances' types.
    OrdUnbox {
        a: Value,
        b: Value,
        x: Value,
        y: Value,
        depth: u32,
        started: bool,
    },
}

/// What resuming a frame did.
enum Step {
    Done(Out),
    Push(Frame),
}

/// The walk's frames: [`INLINE`] in place, the rest in admitted scratch.
struct Stack {
    inline: [Option<Frame>; INLINE],
    n: usize,
    spill: NativeVec<Frame>,
}

impl Stack {
    fn push(&mut self, heap: &Heap, f: Frame) -> bool {
        if let Some(slot) = self.inline.get_mut(self.n) {
            *slot = Some(f);
        } else if self.spill.push_shared(heap, f).is_err() {
            return false;
        }
        self.n += 1;
        true
    }

    fn pop(&mut self) -> Option<Frame> {
        self.n = self.n.checked_sub(1)?;
        match self.inline.get_mut(self.n) {
            Some(slot) => slot.take(),
            None => self.spill.pop(),
        }
    }
}

impl Vm {
    /// Equality without allocation in the arena, at nesting `depth`, with
    /// `left` visits of the walk's budget.
    pub(crate) fn eq_hard(
        &self,
        a: Value,
        b: Value,
        depth: u32,
        left: &mut u32,
    ) -> Result<bool, Hard> {
        let out = match self.start_eq(a, b, depth, left) {
            Start::Leaf(o) => o,
            Start::Push(Frame::Seq {
                a,
                b,
                depth,
                slice: false,
                ..
            }) if depth < MAX_NEST => match self.eq_flat(a, b, left) {
                Ok(r) => return Ok(r),
                Err(i) => {
                    let slice = false;
                    self.walk(
                        Frame::Seq {
                            a,
                            b,
                            i,
                            depth,
                            slice,
                        },
                        left,
                    )
                }
            },
            Start::Push(f) => self.walk(f, left),
        };
        match out {
            Ok(Res::Bool(r)) => Ok(r),
            Ok(_) => Ok(false),
            Err(OrdErr::Deep) => Err(Hard::Deep),
            Err(_) => Err(Hard::Py),
        }
    }

    /// `==` of two lists or two tuples of equal length whose differing
    /// items are ints or strs, as a `Seq` frame decides it, visit for
    /// visit. `Err` is the first item it leaves to that frame.
    fn eq_flat(&self, a: Value, b: Value, left: &mut u32) -> Result<bool, u32> {
        let (xa, xb) = (self.seq_words(a), self.seq_words(b));
        let mut i = 0;
        while let (Some(&x), Some(&y)) = (xa.get(i as usize), xb.get(i as usize)) {
            if x == y {
                i = pass_same(xa, xb, i, left)?;
                continue;
            }
            let (Some(x), Some(y)) = (Value::from_bits(x), Value::from_bits(y)) else {
                return Err(i);
            };
            let (r, n) = if let (Some(p), Some(q)) = (x.as_int(), y.as_int()) {
                (p == q, 0)
            } else if Vm::is_str(x) && Vm::is_str(y) {
                self.str_eq_n(x, y)
            } else {
                return Err(i);
            };
            let Some(rest) = left.checked_sub(1) else {
                return Err(i);
            };
            *left = rest;
            self.spend_text(left, n);
            if !r {
                return Ok(false);
            }
            i += 1;
        }
        Ok(true)
    }

    /// The order of two lists or two tuples whose first differing items
    /// are ints or strs, as an `OrdSeq` frame decides it, visit for visit.
    /// `Err` is the first item it leaves to that frame.
    fn ord_flat(&self, a: Value, b: Value, left: &mut u32) -> Result<Option<Ordering>, u32> {
        let (xa, xb) = (self.seq_words(a), self.seq_words(b));
        let mut i = 0;
        while let (Some(&x), Some(&y)) = (xa.get(i as usize), xb.get(i as usize)) {
            if x == y {
                i = pass_same(xa, xb, i, left)?;
                continue;
            }
            let (Some(x), Some(y)) = (Value::from_bits(x), Value::from_bits(y)) else {
                return Err(i);
            };
            if let (Some(p), Some(q)) = (x.as_int(), y.as_int()) {
                return Ok(Some(p.cmp(&q)));
            }
            if !(Vm::is_str(x) && Vm::is_str(y)) {
                return Err(i);
            }
            let Some(rest) = left.checked_sub(1) else {
                return Err(i);
            };
            let (p, q) = (self.str_bytes(x), self.str_bytes(y));
            *left = rest;
            self.spend_text(left, p.len().min(q.len()));
            let o = p.cmp(q);
            if o != Ordering::Equal {
                return Ok(Some(o));
            }
            i += 1;
        }
        Ok(Some(xa.len().cmp(&xb.len())))
    }

    /// Ordering without allocation in the arena, at nesting `depth`, with
    /// `left` visits of the walk's budget; `Err` names the unorderable pair.
    pub(crate) fn order_pure(
        &self,
        a: Value,
        b: Value,
        depth: u32,
        left: &mut u32,
    ) -> Result<Option<Ordering>, OrdErr> {
        let out = match self.start_ord(a, b, depth, left) {
            Start::Leaf(o) => o,
            Start::Push(Frame::OrdSeq { a, b, depth, .. }) if depth < MAX_NEST => {
                match self.ord_flat(a, b, left) {
                    Ok(o) => return Ok(o),
                    Err(i) => {
                        let wait = 0;
                        self.walk(
                            Frame::OrdSeq {
                                a,
                                b,
                                i,
                                depth,
                                wait,
                            },
                            left,
                        )
                    }
                }
            }
            Start::Push(f) => self.walk(f, left),
        };
        match out {
            Ok(Res::Ord(o)) => Ok(o),
            Ok(_) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// `a <= b` for two sets, inside a walk at `depth` with `left` visits.
    pub(crate) fn set_issubset(
        &self,
        a: Value,
        b: Value,
        depth: u32,
        left: &mut u32,
    ) -> Result<bool, Hard> {
        let out = match self.start_subset(a, b, depth) {
            Start::Leaf(o) => o,
            Start::Push(f) => self.walk(f, left),
        };
        match out {
            Ok(Res::Bool(r)) => Ok(r),
            Ok(_) => Ok(false),
            Err(OrdErr::Deep) => Err(Hard::Deep),
            Err(_) => Err(Hard::Py),
        }
    }

    /// Runs frame `first` and everything it starts to the end. Most walks
    /// (flat sequences, keys that compare at once) finish in the first
    /// step without a child; only one that starts a child builds a stack.
    fn walk(&self, mut first: Frame, left: &mut u32) -> Out {
        // The sequence frames' steps are called directly: comparing flat
        // tuples and lists (sort keys) is the hot case.
        let step = match first {
            Frame::OrdSeq { .. } => self.ord_seq_step(&mut first, None, left),
            Frame::Seq { .. } => self.seq_step(&mut first, None, left),
            _ => self.walk_step(&mut first, None, left),
        };
        match step {
            Step::Done(out) => out,
            Step::Push(child) => self.walk_deep(first, child, left),
        }
    }

    /// [`Vm::walk`] once `parent` has started `child`.
    #[inline(never)]
    fn walk_deep(&self, parent: Frame, child: Frame, left: &mut u32) -> Out {
        let mut stack = Stack {
            inline: [None; INLINE],
            n: 0,
            spill: NativeVec::default(),
        };
        if !stack.push(&self.heap, parent) {
            return Err(OrdErr::Py);
        }
        let mut top = child;
        let mut input = None;
        loop {
            match self.walk_step(&mut top, input.take(), left) {
                Step::Push(child) => {
                    if !stack.push(&self.heap, top) {
                        return Err(OrdErr::Py);
                    }
                    top = child;
                }
                Step::Done(out) => match stack.pop() {
                    Some(parent) => {
                        top = parent;
                        input = Some(out);
                    }
                    None => return out,
                },
            }
        }
    }

    /// `a == b` at nesting `depth`: decided, or the frame that decides it.
    fn start_eq(&self, mut a: Value, mut b: Value, mut depth: u32, left: &mut u32) -> Start {
        use Start::{Leaf, Push};
        loop {
            if a == b {
                if let Some((re, im)) = self.complex_of(a) {
                    return Leaf(Ok(Res::Bool(!re.is_nan() && !im.is_nan())));
                }
                return Leaf(Ok(Res::Bool(a.as_float().is_none_or(|f| !f.is_nan()))));
            }
            let Some(rest) = left.checked_sub(1) else {
                return Leaf(Err(OrdErr::Py));
            };
            *left = rest;
            if depth > MAX_NEST {
                return Leaf(Err(OrdErr::Deep));
            }
            if (self.uflags(a) | self.uflags(b)) & (f::EQ | f::NE) != 0 {
                return Leaf(Err(OrdErr::Py));
            }
            let (x, y) = (self.unbx(a), self.unbx(b));
            if x != a || y != b {
                (a, b, depth) = (x, y, depth.saturating_add(1));
                continue;
            }
            let leaf = |r: bool| Leaf(Ok(Res::Bool(r)));
            if Vm::is_str(a) {
                if !Vm::is_str(b) {
                    return leaf(false);
                }
                let (eq, n) = self.str_eq_n(a, b);
                self.spend_text(left, n);
                return leaf(eq);
            }
            if Vm::is_bytes_like(a) || Vm::is_bytes_like(b) {
                if !(Vm::is_bytes_like(a) && Vm::is_bytes_like(b)) {
                    return leaf(false);
                }
                let (p, q) = (self.bytes_data(a), self.bytes_data(b));
                self.spend_text(left, eq_bytes(p, q));
                return leaf(p == q);
            }
            if let (Some(x), Some(y)) = (self.int_of_o(a), self.int_of_o(b)) {
                return leaf(x == y);
            }
            if (Vm::is_heap_int(a) || Vm::is_heap_int(b))
                && let Some((o, n)) = self.num_cmp(a, b)
            {
                self.spend_text(left, n);
                return leaf(o == Some(Ordering::Equal));
            }
            if let Some(x) = self.num_f64(a)
                && let Some(y) = self.num_f64(b)
            {
                return leaf(num_eq(self, a, b, x, y));
            }
            if let Some((re, im)) = self.complex_of(a).or_else(|| self.complex_of(b)) {
                let other = if Vm::is_complex(a) { b } else { a };
                if let Some((r2, i2)) = self.complex_of(other) {
                    return leaf(re == r2 && im == i2);
                }
                if im != 0.0 || !Vm::is_number(other) {
                    return leaf(false);
                }
                (a, b, depth) = (Value::float(re), other, depth.saturating_add(1));
                continue;
            }
            let (ta, tb) = (self.type_of(a), self.type_of(b));
            return match (ta, tb) {
                (T::tuple, T::tuple) | (T::list, T::list) => {
                    if self.seq_words(a).len() != self.seq_words(b).len() {
                        return leaf(false);
                    }
                    let (i, slice) = (0, false);
                    Push(Frame::Seq {
                        a,
                        b,
                        i,
                        depth,
                        slice,
                    })
                }
                (T::dict, T::dict) => {
                    if self.dict_len(a) != self.dict_len(b) {
                        return leaf(false);
                    }
                    let (e, v, wait) = (0, Value::UNDEFINED, Wait::No);
                    Push(Frame::Dict {
                        a,
                        b,
                        e,
                        depth,
                        v,
                        wait,
                    })
                }
                (T::set | T::frozenset, T::set | T::frozenset) => {
                    if self.set_len(a) != self.set_len(b) {
                        return leaf(false);
                    }
                    self.start_subset(a, b, depth)
                }
                (T::range, T::range) => leaf(self.range_eq(a, b)),
                (T::slice, T::slice) => {
                    let (i, slice) = (0, true);
                    Push(Frame::Seq {
                        a,
                        b,
                        i,
                        depth,
                        slice,
                    })
                }
                (T::dict_keys, T::dict_keys) | (T::dict_items, T::dict_items) => {
                    let (da, db) = (self.vslot_o(a, 0), self.vslot_o(b, 0));
                    if self.dict_len(da) != self.dict_len(db) {
                        return leaf(false);
                    }
                    let items = ta == T::dict_items;
                    let (e, v, wait) = (0, Value::UNDEFINED, Wait::No);
                    Push(Frame::Views {
                        da,
                        db,
                        e,
                        depth,
                        items,
                        v,
                        wait,
                    })
                }
                (T::dict_keys, T::dict_items) | (T::dict_items, T::dict_keys) => {
                    // Set-like, as CPython's `all_contained_in` decides:
                    // a keys view's keys found as pairs in the items view,
                    // or the other way round. Rare; the prelude does it.
                    if self.dict_len(self.vslot_o(a, 0)) != self.dict_len(self.vslot_o(b, 0)) {
                        return leaf(false);
                    }
                    Leaf(Err(OrdErr::Py))
                }
                (T::dict_keys | T::dict_items, T::set | T::frozenset)
                | (T::set | T::frozenset, T::dict_keys | T::dict_items) => {
                    let (kv, st) = if Vm::is_set(a) { (b, a) } else { (a, b) };
                    let items = self.type_of(kv) == T::dict_items;
                    let d = self.vslot_o(kv, 0);
                    if self.dict_len(d) != self.set_len(st) {
                        return leaf(false);
                    }
                    let (i, want, wait) = (0, Value::UNDEFINED, Wait::No);
                    Push(Frame::ViewSet {
                        st,
                        d,
                        i,
                        depth,
                        items,
                        want,
                        wait,
                    })
                }
                (T::method, T::method) => leaf(
                    self.vslot_o(a, 0) == self.vslot_o(b, 0)
                        && self.vslot_o(a, 1) == self.vslot_o(b, 1),
                ),
                // Builtin methods (`xs.append`): the same receiver and method.
                (T::builtin, T::builtin)
                    if a.is_obj_kind(ty::BOUND as u8) && b.is_obj_kind(ty::BOUND as u8) =>
                {
                    leaf(
                        self.vslot_o(a, 0) == self.vslot_o(b, 0)
                            && self.vslot_o(a, 1) == self.vslot_o(b, 1),
                    )
                }
                _ => leaf(false),
            };
        }
    }

    /// Key equality once the hashes match: two `str`s compare at once.
    fn start_key_eq(&self, key: Value, k: Value, depth: u32, left: &mut u32) -> Start {
        if Vm::is_str(key)
            && Vm::is_str(k)
            && let (Some(a), Some(b)) = (key.as_obj(), k.as_obj())
        {
            // One visit of the walk's budget, as `start_eq` charges.
            let Some(rest) = left.checked_sub(1) else {
                return Start::Leaf(Err(OrdErr::Py));
            };
            let (eq, n) = self.heap.str_eq_n(a, b).unwrap_or((false, 0));
            *left = rest;
            self.spend_text(left, n);
            return Start::Leaf(Ok(Res::Bool(eq)));
        }
        self.start_eq(key, k, depth, left)
    }

    /// Order of `a` and `b` at nesting `depth`.
    fn start_ord(&self, a: Value, b: Value, depth: u32, left: &mut u32) -> Start {
        use Start::{Leaf, Push};
        let ord = |o: Option<Ordering>| Leaf(Ok(Res::Ord(o)));
        let Some(rest) = left.checked_sub(1) else {
            return Leaf(Err(OrdErr::Py));
        };
        *left = rest;
        if let (Some(x), Some(y)) = (a.as_int(), b.as_int()) {
            return ord(Some(x.cmp(&y)));
        }
        // Two tuples or two lists (never user objects): items in place.
        if let (Some(ka), Some(kb)) = (a.obj_kind(), b.obj_kind())
            && ka == kb
            && (u16::from(ka) == ty::TUPLE || u16::from(ka) == ty::LIST)
        {
            if depth > MAX_NEST {
                return Leaf(Err(OrdErr::Deep));
            }
            return Push(Frame::OrdSeq {
                a,
                b,
                i: 0,
                depth,
                wait: 0,
            });
        }
        if let (Some(x), Some(y)) = (self.int_of_o(a), self.int_of_o(b)) {
            return ord(Some(x.cmp(&y)));
        }
        if (Vm::is_heap_int(a) || Vm::is_heap_int(b))
            && let Some((o, n)) = self.num_cmp(a, b)
        {
            self.spend_text(left, n);
            return ord(o);
        }
        if let (Some(x), Some(y)) = (self.num_f64(a), self.num_f64(b)) {
            // Exact comparison of an int with a float.
            if let (Some(i), None) = (self.int_of_o(a), self.int_of_o(b)) {
                return ord(cmp_int_float(i, y));
            }
            if let (None, Some(j)) = (self.int_of_o(a), self.int_of_o(b)) {
                return ord(cmp_int_float(j, x).map(Ordering::reverse));
            }
            return ord(x.partial_cmp(&y));
        }
        if (self.uflags(a) | self.uflags(b)) & f::ORDER != 0 {
            return Leaf(Err(OrdErr::Py));
        }
        if depth > MAX_NEST {
            return Leaf(Err(OrdErr::Deep));
        }
        let (x, y) = (self.unbx(a), self.unbx(b));
        if x != a || y != b {
            // Builtin-type subclasses order as their values; errors name
            // the instances' types.
            let started = false;
            return Push(Frame::OrdUnbox {
                a,
                b,
                x,
                y,
                depth,
                started,
            });
        }
        match (self.type_of(a), self.type_of(b)) {
            (T::str, T::str) => {
                let (p, q) = (self.str_bytes(a), self.str_bytes(b));
                self.spend_text(left, p.len().min(q.len()));
                ord(Some(p.cmp(q)))
            }
            (T::bytes | T::bytearray, T::bytes | T::bytearray) => {
                let (p, q) = (self.bytes_data(a), self.bytes_data(b));
                self.spend_text(left, p.len().min(q.len()));
                ord(Some(p.cmp(q)))
            }
            (T::list, T::list) | (T::tuple, T::tuple) => Push(Frame::OrdSeq {
                a,
                b,
                i: 0,
                depth,
                wait: 0,
            }),
            (T::set | T::frozenset, T::set | T::frozenset) => {
                let (stage, sub) = (0, false);
                Push(Frame::OrdSet {
                    a,
                    b,
                    depth,
                    stage,
                    sub,
                })
            }
            _ => Leaf(Err(OrdErr::Types(a, b))),
        }
    }

    /// `a <= b` for two sets at nesting `depth`.
    fn start_subset(&self, a: Value, b: Value, depth: u32) -> Start {
        if self.set_len(a) > self.set_len(b) {
            return Start::Leaf(Ok(Res::Bool(false)));
        }
        Start::Push(Frame::Subset { a, b, i: 0, depth })
    }

    /// Element `key` (hash `hash`) in set `s`, inside a walk at `depth`.
    fn start_set_find(&self, s: Value, key: Value, hash: u64, depth: u32) -> Start {
        let Some((_, keys, hashes, size)) = self.set_parts(s) else {
            return Start::Leaf(Ok(Res::Found(None)));
        };
        let mask = u32::try_from(size).unwrap_or(u32::MAX).wrapping_sub(1);
        #[allow(clippy::cast_possible_truncation, reason = "masked")]
        let i = hash as u32 & mask;
        Start::Push(Frame::SetFind {
            keys,
            hashes,
            mask,
            key,
            hash,
            depth,
            i,
            j: 0,
            perturb: hash,
            deferred: false,
            wait: false,
        })
    }

    /// Key `k` (hash `h`) in dict `d`, inside a walk at `depth`.
    fn start_probe(&self, d: Ref, k: Value, h: u64, depth: u32) -> Start {
        let Some(dv) = self.dv(d) else {
            return Start::Leaf(Ok(Res::Found(None)));
        };
        let Some(index) = dv.index else {
            let (entries, used, e) = (dv.entries, dv.used, 0);
            let (deferred, wait) = (false, false);
            return Start::Push(Frame::Scan {
                entries,
                used,
                k,
                h,
                depth,
                e,
                deferred,
                wait,
            });
        };
        let words = self.heap.raw_words(index).unwrap_or(&[]);
        let cap = u32::try_from(words.len() / 2).unwrap_or(0);
        if cap == 0 {
            return Start::Leaf(Ok(Res::Found(None)));
        }
        let g = Geo::new(cap);
        Start::Push(Frame::Probe {
            entries: dv.entries,
            index,
            k,
            h,
            depth,
            i: g.start(h),
            n: g.steps().saturating_add(1),
            perturb: h,
            e: 0,
            deferred: false,
            wait: false,
        })
    }

    /// Key `k` in dict `d` for a view comparison, at `depth`: found
    /// through its native hash, else by comparing every key.
    fn start_lookup(&self, d: Value, k: Value, depth: u32) -> Start {
        match self.key_hash_pure(k) {
            Some(h) => match d.as_obj() {
                Some(r) => self.start_probe(r, k, h, depth),
                None => Start::Leaf(Ok(Res::Found(None))),
            },
            None => Start::Push(Frame::Linear {
                d,
                k,
                e: 0,
                depth,
                wait: false,
            }),
        }
    }

    /// One step of a `OrdSeq` frame (see [`Vm::walk_step`]).
    #[cfg_attr(mm_shipped, optimize(size))]
    fn ord_seq_step(&self, fr: &mut Frame, input: Option<Out>, left: &mut u32) -> Step {
        use Step::{Done, Push};
        let Frame::OrdSeq {
            a,
            b,
            i,
            depth,
            wait,
        } = fr
        else {
            return Done(Err(OrdErr::Py));
        };
        let (xa, xb) = (self.seq_words(*a), self.seq_words(*b));
        let item = |w: &[u64], at: u32| {
            w.get(at as usize)
                .and_then(|&w| Value::from_bits(w))
                .unwrap_or(Value::UNDEFINED)
        };
        let (x, y) = (item(xa, *i), item(xb, *i));
        let child = depth.saturating_add(1);
        // An unorderable pair may still be equal (`{}`, `{}`).
        let equal = |left: &mut u32| self.start_eq(x, y, child, left);
        match (*wait, input) {
            (_, None) => {}
            (1, Some(Ok(Res::Ord(Some(Ordering::Equal))))) | (2, Some(Ok(Res::Bool(true)))) => {
                *i += 1;
            }
            (1, Some(Err(OrdErr::Types(p, q)))) if p == x && q == y => match equal(left) {
                Start::Leaf(Ok(Res::Bool(true))) => *i += 1,
                Start::Leaf(Ok(_)) => return Done(Err(OrdErr::Types(p, q))),
                Start::Leaf(r) => return Done(r),
                Start::Push(c) => {
                    *wait = 2;
                    return Push(c);
                }
            },
            (2, Some(Ok(_))) => return Done(Err(OrdErr::Types(x, y))),
            (_, Some(r)) => return Done(r),
        }
        let n = xa.len().min(xb.len());
        while (*i as usize) < n {
            let (x, y) = (item(xa, *i), item(xb, *i));
            // Identity first, as `PyObject_RichCompareBool` does
            // (so a shared NaN compares equal).
            if x == y {
                match pass_same(xa, xb, *i, left) {
                    Ok(j) => *i = j,
                    Err(j) => {
                        *i = j;
                        return Done(Err(OrdErr::Py));
                    }
                }
                continue;
            }
            if let (Some(p), Some(q)) = (x.as_int(), y.as_int()) {
                return Done(Ok(Res::Ord(Some(p.cmp(&q)))));
            }
            // CPython orders the first pair that is not `==`.
            // For builtin data an orderable pair is `==`
            // exactly when it orders `Equal`, so one walk does
            // both (`==` and then `<` would walk each
            // differing level twice).
            if (self.uflags(x) | self.uflags(y)) & (f::EQ | f::NE) != 0 {
                return Done(Err(OrdErr::Py));
            }
            match self.start_ord(x, y, child, left) {
                Start::Leaf(Ok(Res::Ord(Some(Ordering::Equal)))) => {}
                Start::Leaf(Err(OrdErr::Types(p, q))) if p == x && q == y => {
                    match self.start_eq(x, y, child, left) {
                        Start::Leaf(Ok(Res::Bool(true))) => {}
                        Start::Leaf(Ok(_)) => return Done(Err(OrdErr::Types(p, q))),
                        Start::Leaf(r) => return Done(r),
                        Start::Push(c) => {
                            *wait = 2;
                            return Push(c);
                        }
                    }
                }
                Start::Leaf(r) => return Done(r),
                Start::Push(c) => {
                    *wait = 1;
                    return Push(c);
                }
            }
            *i += 1;
        }
        Done(Ok(Res::Ord(Some(xa.len().cmp(&xb.len())))))
    }

    /// One step of a `Seq` frame (see [`Vm::walk_step`]).
    #[cfg_attr(mm_shipped, optimize(size))]
    fn seq_step(&self, fr: &mut Frame, input: Option<Out>, left: &mut u32) -> Step {
        use Step::{Done, Push};
        let Frame::Seq {
            a,
            b,
            i,
            depth,
            slice,
        } = fr
        else {
            return Done(Err(OrdErr::Py));
        };
        if let Some(r) = input {
            if !matches!(r, Ok(Res::Bool(true))) {
                return Done(r);
            }
            *i += 1;
        }
        let (xa, xb): (&[u64], &[u64]) = if *slice {
            (&[], &[])
        } else {
            (self.seq_words(*a), self.seq_words(*b))
        };
        let n = if *slice { 3 } else { xa.len().min(xb.len()) };
        while (*i as usize) < n {
            let (x, y) = if *slice {
                (self.vslot_o(*a, *i), self.vslot_o(*b, *i))
            } else {
                let at = *i as usize;
                let v = |w: &[u64]| {
                    w.get(at)
                        .and_then(|&w| Value::from_bits(w))
                        .unwrap_or(Value::UNDEFINED)
                };
                (v(xa), v(xb))
            };
            // Identity first, as `PyObject_RichCompareBool` does.
            if x == y {
                match pass_same(xa, xb, *i, left) {
                    Ok(j) => *i = j,
                    Err(j) => {
                        *i = j;
                        return Done(Err(OrdErr::Py));
                    }
                }
                continue;
            }
            match self.start_eq(x, y, depth.saturating_add(1), left) {
                Start::Leaf(Ok(Res::Bool(true))) => {}
                Start::Leaf(r) => return Done(r),
                Start::Push(c) => return Push(c),
            }
            *i += 1;
        }
        Done(Ok(Res::Bool(true)))
    }
    /// Advances frame `fr`, given the result of the child it last started.
    #[allow(clippy::too_many_lines, reason = "one arm per frame kind")]
    #[cfg_attr(mm_shipped, optimize(size))]
    fn walk_step(&self, fr: &mut Frame, input: Option<Out>, left: &mut u32) -> Step {
        use Step::{Done, Push};
        let yes = Ok(Res::Bool(true));
        let no = Ok(Res::Bool(false));
        let found = |v: Option<Value>| Done(Ok(Res::Found(v)));
        match fr {
            Frame::Seq { .. } => self.seq_step(fr, input, left),
            Frame::Dict {
                a,
                b,
                e,
                depth,
                v,
                wait,
            } => {
                match (input, *wait) {
                    (None, _) => {}
                    (Some(Ok(Res::Found(Some(w)))), Wait::Lookup) => {
                        if w != *v {
                            match self.start_eq(*v, w, depth.saturating_add(1), left) {
                                Start::Leaf(Ok(Res::Bool(true))) => {}
                                Start::Leaf(r) => return Done(r),
                                Start::Push(c) => {
                                    *wait = Wait::Eq;
                                    return Push(c);
                                }
                            }
                        }
                        *e += 1;
                    }
                    (Some(Ok(Res::Found(None))), _) => return Done(no),
                    (Some(Ok(Res::Bool(true))), Wait::Eq) => *e += 1,
                    (Some(r), _) => return Done(r),
                }
                let (Some(da), Some(rb)) = (a.as_obj().and_then(|r| self.dv(r)), b.as_obj()) else {
                    return Done(no);
                };
                while *e < da.used {
                    let k = self.slot_o(da.entries, 2 * *e);
                    if k != Value::UNDEFINED {
                        // One visit per entry, found by identity or not.
                        let Some(rest) = left.checked_sub(1) else {
                            return Done(Err(OrdErr::Py));
                        };
                        *left = rest;
                        *v = self.slot_o(da.entries, 2 * *e + 1);
                        let h = match da.index {
                            Some(index) => {
                                let words = self.heap.raw_words(index).unwrap_or(&[]);
                                words.get(words.len() / 2 + *e as usize).copied()
                            }
                            None => self.key_hash_pure(k),
                        };
                        let Some(h) = h else {
                            return Done(Err(OrdErr::Py));
                        };
                        match self.start_probe(rb, k, h, *depth) {
                            Start::Leaf(Ok(Res::Found(Some(w)))) => {
                                if w != *v {
                                    match self.start_eq(*v, w, depth.saturating_add(1), left) {
                                        Start::Leaf(Ok(Res::Bool(true))) => {}
                                        Start::Leaf(r) => return Done(r),
                                        Start::Push(c) => {
                                            *wait = Wait::Eq;
                                            return Push(c);
                                        }
                                    }
                                }
                            }
                            Start::Leaf(Ok(_)) => return Done(no),
                            Start::Leaf(r) => return Done(r),
                            Start::Push(c) => {
                                *wait = Wait::Lookup;
                                return Push(c);
                            }
                        }
                    }
                    *e += 1;
                }
                Done(yes)
            }
            Frame::Views {
                da,
                db,
                e,
                depth,
                items,
                v,
                wait,
            } => {
                match (input, *wait) {
                    (None, _) => {}
                    (Some(Ok(Res::Found(Some(w)))), Wait::Lookup) => {
                        if *items && w != *v {
                            match self.start_eq(*v, w, depth.saturating_add(1), left) {
                                Start::Leaf(Ok(Res::Bool(true))) => {}
                                Start::Leaf(r) => return Done(r),
                                Start::Push(c) => {
                                    *wait = Wait::Eq;
                                    return Push(c);
                                }
                            }
                        }
                        *e += 1;
                    }
                    (Some(Ok(Res::Found(None))), _) => return Done(no),
                    (Some(Ok(Res::Bool(true))), Wait::Eq) => *e += 1,
                    (Some(r), _) => return Done(r),
                }
                let Some(dv) = da.as_obj().and_then(|r| self.dv(r)) else {
                    return Done(yes);
                };
                while *e < dv.used {
                    let k = self.slot_o(dv.entries, 2 * *e);
                    if k != Value::UNDEFINED {
                        // One visit per entry, found by identity or not.
                        let Some(rest) = left.checked_sub(1) else {
                            return Done(Err(OrdErr::Py));
                        };
                        *left = rest;
                        *v = self.slot_o(dv.entries, 2 * *e + 1);
                        match self.start_lookup(*db, k, *depth) {
                            Start::Leaf(Ok(Res::Found(Some(w)))) => {
                                if *items && w != *v {
                                    match self.start_eq(*v, w, depth.saturating_add(1), left) {
                                        Start::Leaf(Ok(Res::Bool(true))) => {}
                                        Start::Leaf(r) => return Done(r),
                                        Start::Push(c) => {
                                            *wait = Wait::Eq;
                                            return Push(c);
                                        }
                                    }
                                }
                            }
                            Start::Leaf(Ok(_)) => return Done(no),
                            Start::Leaf(r) => return Done(r),
                            Start::Push(c) => {
                                *wait = Wait::Lookup;
                                return Push(c);
                            }
                        }
                    }
                    *e += 1;
                }
                Done(yes)
            }
            Frame::ViewSet {
                st,
                d,
                i,
                depth,
                items,
                want,
                wait,
            } => {
                match (input, *wait) {
                    (None, _) => {}
                    (Some(Ok(Res::Found(Some(v)))), Wait::Lookup) => {
                        if *items && v != *want {
                            match self.start_eq(v, *want, depth.saturating_add(1), left) {
                                Start::Leaf(Ok(Res::Bool(true))) => {}
                                Start::Leaf(r) => return Done(r),
                                Start::Push(c) => {
                                    *wait = Wait::Eq;
                                    return Push(c);
                                }
                            }
                        }
                        *i += 1;
                    }
                    (Some(Ok(Res::Found(None))), _) => return Done(no),
                    (Some(Ok(Res::Bool(true))), Wait::Eq) => *i += 1,
                    (Some(r), _) => return Done(r),
                }
                let Some((_, keys, _, size)) = self.set_parts(*st) else {
                    return Done(yes);
                };
                while (*i as usize) < size {
                    let k = self.slot_o(keys, *i);
                    if k != Value::UNDEFINED {
                        // One visit per entry, found by identity or not.
                        let Some(rest) = left.checked_sub(1) else {
                            return Done(Err(OrdErr::Py));
                        };
                        *left = rest;
                        // An items view holds `(key, value)` pairs.
                        let key = if !*items {
                            k
                        } else if self.type_of(k) == T::tuple && self.tuple_len(k) == 2 {
                            *want = self.vslot_o(k, 1);
                            self.vslot_o(k, 0)
                        } else {
                            return Done(no);
                        };
                        match self.start_lookup(*d, key, *depth) {
                            Start::Leaf(Ok(Res::Found(Some(v)))) => {
                                if *items && v != *want {
                                    match self.start_eq(v, *want, depth.saturating_add(1), left) {
                                        Start::Leaf(Ok(Res::Bool(true))) => {}
                                        Start::Leaf(r) => return Done(r),
                                        Start::Push(c) => {
                                            *wait = Wait::Eq;
                                            return Push(c);
                                        }
                                    }
                                }
                            }
                            Start::Leaf(Ok(_)) => return Done(no),
                            Start::Leaf(r) => return Done(r),
                            Start::Push(c) => {
                                *wait = Wait::Lookup;
                                return Push(c);
                            }
                        }
                    }
                    *i += 1;
                }
                Done(yes)
            }
            Frame::Subset { a, b, i, depth } => {
                match input {
                    None => {}
                    Some(Ok(Res::Found(Some(_)))) => *i += 1,
                    Some(Ok(_)) => return Done(no),
                    Some(r) => return Done(r),
                }
                let Some((_, keys, hashes, size)) = self.set_parts(*a) else {
                    return Done(yes);
                };
                while (*i as usize) < size {
                    let k = self.slot_o(keys, *i);
                    if k != Value::UNDEFINED {
                        // One visit per entry, found by identity or not.
                        let Some(rest) = left.checked_sub(1) else {
                            return Done(Err(OrdErr::Py));
                        };
                        *left = rest;
                        let h = self.raw_o(hashes, *i);
                        match self.start_set_find(*b, k, h, *depth) {
                            Start::Leaf(Ok(Res::Found(Some(_)))) => {}
                            Start::Leaf(Ok(_)) => return Done(no),
                            Start::Leaf(r) => return Done(r),
                            Start::Push(c) => return Push(c),
                        }
                    }
                    *i += 1;
                }
                Done(yes)
            }
            Frame::Probe {
                entries,
                index,
                k,
                h,
                depth,
                i,
                n,
                perturb,
                e,
                deferred,
                wait,
            } => {
                let words = self.heap.raw_words(*index).unwrap_or(&[]);
                let kvs = self.heap.slot_words(*entries).unwrap_or(&[]);
                let cap = u32::try_from(words.len() / 2).unwrap_or(0);
                let g = Geo::new(cap);
                let val = |e: u32| {
                    kvs.get(2 * e as usize + 1)
                        .and_then(|&w| Value::from_bits(w))
                        .unwrap_or(Value::UNDEFINED)
                };
                if *wait {
                    *wait = false;
                    match input {
                        Some(Ok(Res::Bool(true))) => return found(Some(val(*e))),
                        Some(Ok(_)) | None => {}
                        Some(Err(OrdErr::Py)) => *deferred = true,
                        Some(r) => return Done(r),
                    }
                    *i = g.next(*i, perturb);
                    *n -= 1;
                }
                let tag = g.tag(*h);
                while *n > 0 {
                    let w = islot(words, *i);
                    if w == 0 {
                        break;
                    }
                    if w != DELETED && w & !g.emask == tag {
                        *e = (w & g.emask).wrapping_sub(1);
                        let kb = kvs.get(2 * *e as usize).copied().unwrap_or(0);
                        if kb == k.to_bits() {
                            return found(Some(val(*e)));
                        }
                        if words.get((cap + *e) as usize) == Some(h) {
                            let key = Value::from_bits(kb).unwrap_or(Value::UNDEFINED);
                            match self.start_key_eq(key, *k, depth.saturating_add(1), left) {
                                Start::Leaf(Ok(Res::Bool(true))) => return found(Some(val(*e))),
                                Start::Leaf(Ok(_)) => {}
                                Start::Leaf(Err(OrdErr::Py)) => *deferred = true,
                                Start::Leaf(r) => return Done(r),
                                Start::Push(c) => {
                                    *wait = true;
                                    return Push(c);
                                }
                            }
                        }
                    }
                    *i = g.next(*i, perturb);
                    *n -= 1;
                }
                if *deferred {
                    return Done(Err(OrdErr::Py));
                }
                found(None)
            }
            Frame::Scan {
                entries,
                used,
                k,
                h,
                depth,
                e,
                deferred,
                wait,
            } => {
                let kvs = self.heap.slot_words(*entries).unwrap_or(&[]);
                let n = (*used as usize).min(kvs.len() / 2);
                let val = |e: u32| {
                    kvs.get(2 * e as usize + 1)
                        .and_then(|&w| Value::from_bits(w))
                        .unwrap_or(Value::UNDEFINED)
                };
                if *wait {
                    *wait = false;
                    match input {
                        Some(Ok(Res::Bool(true))) => return found(Some(val(*e))),
                        Some(Ok(_)) | None => {}
                        Some(Err(OrdErr::Py)) => *deferred = true,
                        Some(r) => return Done(r),
                    }
                    *e += 1;
                }
                while (*e as usize) < n {
                    let kb = kvs.get(2 * *e as usize).copied().unwrap_or(0);
                    if kb == k.to_bits() {
                        return found(Some(val(*e)));
                    }
                    if let Some(key) = Value::from_bits(kb)
                        && key != Value::UNDEFINED
                        && self.key_hash_pure(key) == Some(*h)
                    {
                        match self.start_key_eq(key, *k, depth.saturating_add(1), left) {
                            Start::Leaf(Ok(Res::Bool(true))) => return found(Some(val(*e))),
                            Start::Leaf(Ok(_)) => {}
                            Start::Leaf(Err(OrdErr::Py)) => *deferred = true,
                            Start::Leaf(r) => return Done(r),
                            Start::Push(c) => {
                                *wait = true;
                                return Push(c);
                            }
                        }
                    }
                    *e += 1;
                }
                if *deferred {
                    return Done(Err(OrdErr::Py));
                }
                found(None)
            }
            Frame::Linear {
                d,
                k,
                e,
                depth,
                wait,
            } => {
                let Some(dv) = d.as_obj().and_then(|r| self.dv(r)) else {
                    return found(None);
                };
                if *wait {
                    *wait = false;
                    match input {
                        Some(Ok(Res::Bool(true))) => {
                            return found(Some(self.slot_o(dv.entries, 2 * *e + 1)));
                        }
                        Some(Ok(_)) | None => {}
                        Some(r) => return Done(r),
                    }
                    *e += 1;
                }
                while *e < dv.used {
                    let k2 = self.slot_o(dv.entries, 2 * *e);
                    if k2 != Value::UNDEFINED {
                        match self.start_eq(*k, k2, depth.saturating_add(1), left) {
                            Start::Leaf(Ok(Res::Bool(true))) => {
                                return found(Some(self.slot_o(dv.entries, 2 * *e + 1)));
                            }
                            Start::Leaf(Ok(_)) => {}
                            Start::Leaf(r) => return Done(r),
                            Start::Push(c) => {
                                *wait = true;
                                return Push(c);
                            }
                        }
                    }
                    *e += 1;
                }
                found(None)
            }
            Frame::SetFind {
                keys,
                hashes,
                mask,
                key,
                hash,
                depth,
                i,
                j,
                perturb,
                deferred,
                wait,
            } => {
                if *wait {
                    *wait = false;
                    match input {
                        Some(Ok(Res::Bool(true))) => return found(Some(*key)),
                        Some(Ok(_)) | None => {}
                        Some(Err(OrdErr::Py)) => *deferred = true,
                        Some(r) => return Done(r),
                    }
                    *j += 1;
                }
                loop {
                    let probes = if *i as usize + LINEAR_PROBES <= *mask as usize {
                        LINEAR_PROBES as u32
                    } else {
                        0
                    };
                    while *j <= probes {
                        let idx = i.wrapping_add(*j);
                        let ek = self.slot_o(*keys, idx);
                        let eh = self.raw_o(*hashes, idx);
                        if ek == Value::UNDEFINED && eh == 0 {
                            if *deferred {
                                return Done(Err(OrdErr::Py));
                            }
                            return found(None);
                        }
                        if eh == *hash && ek != Value::UNDEFINED {
                            if ek == *key {
                                return found(Some(ek));
                            }
                            match self.start_eq(ek, *key, depth.saturating_add(1), left) {
                                Start::Leaf(Ok(Res::Bool(true))) => return found(Some(ek)),
                                Start::Leaf(Ok(_)) => {}
                                Start::Leaf(Err(OrdErr::Py)) => *deferred = true,
                                Start::Leaf(r) => return Done(r),
                                Start::Push(c) => {
                                    *wait = true;
                                    return Push(c);
                                }
                            }
                        }
                        *j += 1;
                    }
                    *j = 0;
                    *perturb >>= PERTURB_SHIFT;
                    #[allow(clippy::cast_possible_truncation, reason = "masked")]
                    {
                        *i = (i
                            .wrapping_mul(5)
                            .wrapping_add(1)
                            .wrapping_add(*perturb as u32))
                            & *mask;
                    }
                }
            }
            Frame::OrdSeq { .. } => self.ord_seq_step(fr, input, left),
            Frame::OrdSet {
                a,
                b,
                depth,
                stage,
                sub,
            } => {
                match (*stage, input) {
                    (_, None) => {}
                    (1 | 2, Some(Ok(Res::Bool(r)))) => {
                        if *stage == 1 {
                            *sub = r;
                        } else {
                            return Done(Ok(Res::Ord(set_order(*sub, r))));
                        }
                    }
                    (_, Some(r)) => return Done(r),
                }
                if *stage == 0 {
                    *stage = 1;
                    match self.start_subset(*a, *b, *depth) {
                        Start::Leaf(Ok(Res::Bool(r))) => *sub = r,
                        Start::Leaf(r) => return Done(r),
                        Start::Push(c) => return Push(c),
                    }
                }
                *stage = 2;
                match self.start_subset(*b, *a, *depth) {
                    Start::Leaf(Ok(Res::Bool(r))) => Done(Ok(Res::Ord(set_order(*sub, r)))),
                    Start::Leaf(r) => Done(r),
                    Start::Push(c) => Push(c),
                }
            }
            Frame::OrdUnbox {
                a,
                b,
                x,
                y,
                depth,
                started,
            } => {
                let r = if *started {
                    input.unwrap_or(Err(OrdErr::Py))
                } else {
                    *started = true;
                    match self.start_ord(*x, *y, depth.saturating_add(1), left) {
                        Start::Leaf(r) => r,
                        Start::Push(c) => return Push(c),
                    }
                };
                Done(match r {
                    Err(OrdErr::Types(p, q)) if p == *x && q == *y => Err(OrdErr::Types(*a, *b)),
                    r => r,
                })
            }
        }
    }
}

/// The order of two sets from `a <= b` (`sub`) and `b <= a` (`sup`).
fn set_order(sub: bool, sup: bool) -> Option<Ordering> {
    match (sub, sup) {
        (true, true) => Some(Ordering::Equal),
        (true, false) => Some(Ordering::Less),
        (false, true) => Some(Ordering::Greater),
        _ => None,
    }
}
