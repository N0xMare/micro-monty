//! A deterministic interpreter that reads bytes as a sequence of heap
//! operations and checks the heap against a model as it goes. Shared by
//! `tests/gc.rs` (driven by a PRNG) and the cargo-fuzz target
//! `fuzz/fuzz_targets/arena_ops.rs` (driven by libFuzzer).
//!
//! Invariants checked:
//! - every object, block and string reachable from the roots
//!   keeps its exact header and contents across any number of collections;
//! - a collection frees exactly the unreachable model items: their references
//!   become `BadRef` and the live byte count matches the model;
//! - stale, forged and mid-object references are errors, never panics, and
//!   never resolve to something that is not a live object;
//! - slot-kind and bounds errors, read-only mm-core objects, flag writes,
//!   `would_fit`, and interning identity.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::too_many_lines,
    reason = "test interpreter: casts of fuzz input are intended"
)]

use mm_core::{
    ByteRef, Error, Heap, HeapConfig, Ref, Special, StrBuf, Value, flags, hash_bytes, types,
};
use std::collections::HashMap;

/// Memory limits for interpreted runs (the first input byte picks one): small
/// enough that the out-of-memory paths run.
const LIMITS: [u64; 4] = [1 << 20, 3 << 20, 6 << 20, 16 << 20];
const MAX_ROOTS: usize = 48;
const MAX_ITEMS: usize = 3000;
const BIG: usize = 1000;
const SPECIALS: [Special; 6] = [
    Special::None,
    Special::False,
    Special::True,
    Special::NotImplemented,
    Special::Ellipsis,
    Special::Undefined,
];
const PIECES: [&str; 6] = ["a", "b", "\u{e9}", "\u{20ac}", "\u{1d11e}", "xy"];

/// What a run did.
#[derive(Clone, Copy, Debug, Default)]
pub struct Summary {
    /// Operations executed.
    pub ops: u64,
    /// Collections run.
    pub collections: u64,
    /// Allocations refused with `OutOfMemory`.
    pub oom: u64,
    /// Most bytes committed at any point.
    pub max_committed: u64,
}

struct Input<'a> {
    data: &'a [u8],
    pos: usize,
}

impl Input<'_> {
    fn done(&self) -> bool {
        self.pos >= self.data.len()
    }
    fn u8(&mut self) -> u8 {
        let b = self.data.get(self.pos).copied().unwrap_or(0);
        self.pos += 1;
        b
    }
    fn u16(&mut self) -> u16 {
        u16::from(self.u8()) | u16::from(self.u8()) << 8
    }
    fn u32(&mut self) -> u32 {
        u32::from(self.u16()) | u32::from(self.u16()) << 16
    }
    fn u64(&mut self) -> u64 {
        u64::from(self.u32()) | u64::from(self.u32()) << 32
    }
    fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            usize::from(self.u16()) % n
        }
    }
}

struct Obj {
    r: Ref,
    type_id: u16,
    flags: u8,
    vals: Vec<Value>,
    raws: Vec<u64>,
}

struct Blk {
    b: ByteRef,
    data: Vec<u8>,
}

struct Str {
    r: Ref,
    s: String,
    interned: bool,
}

struct Model {
    heap: Heap,
    objs: Vec<Obj>,
    blks: Vec<Blk>,
    strs: Vec<Str>,
    obj_at: HashMap<u32, usize>,
    blk_at: HashMap<u32, usize>,
    str_at: HashMap<u32, usize>,
    interned: HashMap<String, Ref>,
    roots: Vec<Value>,
    /// An allocation may have failed halfway (a `str` whose object failed
    /// after its data block), leaving an unmodelled block until the next GC.
    orphans: bool,
    limit: u64,
    sum: Summary,
}

fn block_bytes(len: usize) -> u64 {
    (1 + len.div_ceil(8) as u64) * 8
}

/// Runs the operations encoded in `data`; panics on any invariant violation.
pub fn run(data: &[u8]) -> Summary {
    let limit = LIMITS[usize::from(data.first().copied().unwrap_or(0) % 4)];
    let mut m = Model {
        heap: Heap::new(HeapConfig {
            memory_limit: limit,
            ..HeapConfig::default()
        }),
        objs: Vec::new(),
        blks: Vec::new(),
        strs: Vec::new(),
        obj_at: HashMap::new(),
        blk_at: HashMap::new(),
        str_at: HashMap::new(),
        interned: HashMap::new(),
        roots: Vec::new(),
        orphans: false,
        limit,
        sum: Summary::default(),
    };
    let mut inp = Input { data, pos: 0 };
    while !inp.done() {
        m.step(&mut inp);
        m.sum.ops += 1;
        m.sum.max_committed = m.sum.max_committed.max(m.heap.committed_bytes());
        assert!(
            m.heap.committed_bytes() <= limit,
            "committed past the limit"
        );
    }
    // Drop every root: the weak intern table must retain no strings.
    m.roots.clear();
    m.collect();
    assert!(m.objs.is_empty() && m.blks.is_empty());
    assert!(m.strs.is_empty() && m.interned.is_empty());
    m.sum
}

impl Model {
    fn step(&mut self, inp: &mut Input<'_>) {
        if self.objs.len() + self.blks.len() + self.strs.len() > MAX_ITEMS {
            self.collect();
            if self.objs.len() + self.blks.len() + self.strs.len() > MAX_ITEMS / 2 {
                self.roots.truncate(self.roots.len() / 2);
            }
        }
        match inp.u8() % 20 {
            0 | 1 => self.op_alloc(inp),
            2 | 3 => self.op_alloc_init(inp),
            4 => self.op_alloc_big(inp),
            5 | 6 => self.op_set(inp),
            7 => self.op_set_raw(inp),
            8 => self.op_bytes(inp),
            9 => self.op_str(inp),
            10 => self.op_intern(inp),
            11 => self.op_intern_str(inp),
            12 => {
                if self.roots.len() < MAX_ROOTS {
                    let v = self.pick_value(inp);
                    self.roots.push(v);
                }
            }
            13 => {
                if !self.roots.is_empty() {
                    let i = inp.below(self.roots.len());
                    self.roots.swap_remove(i);
                }
            }
            14 => {
                if inp.u8().is_multiple_of(4) {
                    self.collect();
                }
            }
            15 => self.op_probe(inp),
            16 => self.op_bad_slots(inp),
            17 => self.op_set_flags(inp),
            18 => self.op_preflight(inp),
            _ => {
                if self.heap.should_collect() {
                    self.collect();
                }
            }
        }
    }

    fn oom(&mut self, e: Error) {
        assert_eq!(e, Error::OutOfMemory, "unexpected error");
        self.sum.oom += 1;
    }

    fn fresh(&self, raw: u32) {
        assert!(
            !self.obj_at.contains_key(&raw) && !self.str_at.contains_key(&raw),
            "allocation returned a live reference"
        );
    }

    fn push_obj(&mut self, o: Obj) {
        self.fresh(o.r.raw());
        self.check_obj(&o);
        self.obj_at.insert(o.r.raw(), self.objs.len());
        self.objs.push(o);
    }

    fn push_str(&mut self, s: Str) {
        self.fresh(s.r.raw());
        self.check_str(&s);
        self.str_at.insert(s.r.raw(), self.strs.len());
        self.strs.push(s);
    }

    fn push_blk(&mut self, b: Blk) {
        assert!(!self.blk_at.contains_key(&b.b.raw()));
        self.check_blk(&b);
        self.blk_at.insert(b.b.raw(), self.blks.len());
        self.blks.push(b);
    }

    fn engine_type(inp: &mut Input<'_>) -> (u16, u8) {
        (
            types::FIRST_ENGINE + u16::from(inp.u8() % 4),
            inp.u8() & !flags::CORE,
        )
    }

    fn op_alloc(&mut self, inp: &mut Input<'_>) {
        let nvals = inp.below(9);
        let nraw = inp.below(5);
        let (type_id, fl) = Self::engine_type(inp);
        match self.heap.alloc(type_id, fl, nvals as u32, nraw as u32) {
            Ok(r) => self.push_obj(Obj {
                r,
                type_id,
                flags: fl,
                vals: vec![Value::UNDEFINED; nvals],
                raws: vec![0; nraw],
            }),
            Err(e) => self.oom(e),
        }
    }

    fn op_alloc_init(&mut self, inp: &mut Input<'_>) {
        let nvals = inp.below(9);
        let nraw = inp.below(5);
        let vals: Vec<Value> = (0..nvals).map(|_| self.pick_value(inp)).collect();
        let raws: Vec<u64> = (0..nraw).map(|_| inp.u64()).collect();
        let (type_id, fl) = Self::engine_type(inp);
        match self.heap.alloc_init(type_id, fl, &vals, &raws) {
            Ok(r) => self.push_obj(Obj {
                r,
                type_id,
                flags: fl,
                vals,
                raws,
            }),
            Err(e) => self.oom(e),
        }
    }

    /// Medium and dedicated-segment objects, at most two big ones alive.
    fn op_alloc_big(&mut self, inp: &mut Input<'_>) {
        let big = self
            .objs
            .iter()
            .filter(|o| o.vals.len() + o.raws.len() > BIG)
            .count();
        if big >= 2 {
            return;
        }
        let (nvals, nraw) = match inp.u8() % 3 {
            0 => (64 + inp.below(2000), 0),
            1 => (17_000 + inp.below(8000), 0),
            _ => (inp.below(255), inp.below(20_000)),
        };
        let (type_id, fl) = Self::engine_type(inp);
        match self.heap.alloc(type_id, fl, nvals as u32, nraw as u32) {
            Ok(r) => self.push_obj(Obj {
                r,
                type_id,
                flags: fl,
                vals: vec![Value::UNDEFINED; nvals],
                raws: vec![0; nraw],
            }),
            Err(e) => self.oom(e),
        }
    }

    fn op_set(&mut self, inp: &mut Input<'_>) {
        if self.objs.is_empty() {
            return;
        }
        let k = inp.below(self.objs.len());
        let v = self.pick_value(inp);
        let o = &mut self.objs[k];
        if o.vals.is_empty() {
            return;
        }
        let i = inp.below(o.vals.len());
        let mode = inp.u8() % 3;
        if mode == 2 {
            // In place: up to four slots from another, refused past the
            // values.
            let (from, len) = (inp.below(o.vals.len()), inp.below(5));
            let res = self
                .heap
                .move_values(o.r, from as u32, i as u32, len as u32);
            if from.max(i) + len <= o.vals.len() {
                res.unwrap();
                o.vals.copy_within(from..from + len, i);
            } else {
                assert_eq!(res, Err(Error::BadSlot));
                return;
            }
            assert_eq!(self.heap.get(o.r, i as u32), Ok(o.vals[i]));
            return;
        }
        if mode == 0 {
            self.heap.set(o.r, i as u32, v).unwrap();
            o.vals[i] = v;
        } else {
            // Batched: up to three slots from `i`, refused past the values.
            let vs = vec![v; 1 + inp.below(3)];
            let res = self.heap.set_values(o.r, i as u32, &vs);
            if i + vs.len() <= o.vals.len() {
                res.unwrap();
                o.vals[i..i + vs.len()].copy_from_slice(&vs);
            } else {
                assert_eq!(res, Err(Error::BadSlot));
                return;
            }
        }
        assert_eq!(self.heap.get(o.r, i as u32), Ok(v));
    }

    fn op_set_raw(&mut self, inp: &mut Input<'_>) {
        if self.objs.is_empty() {
            return;
        }
        let k = inp.below(self.objs.len());
        let w = inp.u64();
        let o = &mut self.objs[k];
        if o.raws.is_empty() {
            return;
        }
        let i = inp.below(o.raws.len());
        let slot = (o.vals.len() + i) as u32;
        if inp.u8().is_multiple_of(2) {
            self.heap.set_raw(o.r, slot, w).unwrap();
        } else {
            let raws = self.heap.raw_words_mut(o.r).unwrap();
            assert_eq!(raws.len(), o.raws.len());
            raws[i] = w;
        }
        o.raws[i] = w;
        assert_eq!(self.heap.get_raw(o.r, slot), Ok(w));
    }

    fn op_bytes(&mut self, inp: &mut Input<'_>) {
        let big = self.blks.iter().filter(|b| b.data.len() > BIG).count();
        let len = if inp.u8().is_multiple_of(32) && big < 2 {
            130_000 + inp.below(60_000)
        } else {
            inp.below(300)
        };
        let seed = inp.u8();
        let zeroed = inp.u8().is_multiple_of(4);
        let data: Vec<u8> = if zeroed {
            vec![0; len]
        } else {
            (0..len)
                .map(|i| seed.wrapping_add((i as u8).wrapping_mul(31)))
                .collect()
        };
        let res = if zeroed {
            self.heap.alloc_bytes_zeroed(len)
        } else {
            self.heap.alloc_bytes(&data)
        };
        match res {
            Ok(b) => {
                if !data.is_empty() && inp.u8().is_multiple_of(2) {
                    // Mutate through the barrier.
                    let i = inp.below(data.len());
                    let x = inp.u8();
                    self.heap.bytes_mut(b).unwrap()[i] = x;
                    let mut data = data;
                    data[i] = x;
                    self.push_blk(Blk { b, data });
                } else {
                    self.push_blk(Blk { b, data });
                }
            }
            Err(e) => self.oom(e),
        }
    }

    fn gen_string(inp: &mut Input<'_>) -> String {
        let n = inp.below(7);
        (0..n).map(|_| PIECES[inp.below(PIECES.len())]).collect()
    }

    fn op_str(&mut self, inp: &mut Input<'_>) {
        let mut s = Self::gen_string(inp);
        let res = match inp.u8() % 4 {
            // `a + b` of two model strings.
            0 if !self.strs.is_empty() => {
                let a = &self.strs[inp.below(self.strs.len())];
                let b = &self.strs[inp.below(self.strs.len())];
                let res = self.heap.str_concat(a.r, b.r);
                if a.s.is_empty() || b.s.is_empty() {
                    // The other operand itself comes back.
                    let same = if b.s.is_empty() { a.r } else { b.r };
                    assert_eq!(res, Ok(same));
                    return;
                }
                s = format!("{}{}", a.s, b.s);
                res
            }
            // Built in a `StrBuf` from pieces, a model string and a slice.
            1 if !self.strs.is_empty() => {
                let other = &self.strs[inp.below(self.strs.len())];
                let cut = inp.below(other.s.len() + 1);
                let mut buf = StrBuf::new();
                buf.push_str(&s);
                buf.push_ref(&self.heap, other.r).unwrap();
                let sliced = buf.push_slice(&self.heap, other.r, 0, cut);
                let mut want = format!("{s}{}", other.s);
                if other.s.is_char_boundary(cut) {
                    sliced.unwrap();
                    want.push_str(&other.s[..cut]);
                } else {
                    assert_eq!(sliced, Err(Error::BadSlot));
                }
                buf.push('\u{e9}');
                want.push('\u{e9}');
                assert_eq!(buf.as_bytes(), want.as_bytes());
                assert_eq!(buf.chars(), want.chars().count());
                s = want;
                self.heap.new_str_from(&buf)
            }
            _ => self.heap.new_str(&s),
        };
        match res {
            Ok(r) => {
                // Hashes are computed lazily; caching one changes nothing
                // else about the string.
                assert!(!self.heap.str_hashed(r));
                if inp.u8().is_multiple_of(2) {
                    assert_eq!(self.heap.str_hash_mut(r), Ok(hash_bytes(s.as_bytes())));
                    assert!(self.heap.str_hashed(r));
                }
                self.push_str(Str {
                    r,
                    s,
                    interned: false,
                });
            }
            Err(e) => {
                self.orphans = true;
                self.oom(e);
            }
        }
    }

    fn op_intern(&mut self, inp: &mut Input<'_>) {
        let s = Self::gen_string(inp);
        match self.heap.intern(&s) {
            Ok(r) => {
                if let Some(&old) = self.interned.get(&s) {
                    assert_eq!(r, old, "intern of an equal string gave a new ref");
                } else {
                    assert!(self.interned.values().all(|&x| x != r));
                    self.interned.insert(s.clone(), r);
                    self.push_str(Str {
                        r,
                        s: s.clone(),
                        interned: true,
                    });
                }
                assert_eq!(self.heap.find_interned(&s), Some(r));
            }
            Err(e) => {
                self.orphans = true;
                self.oom(e);
            }
        }
        assert_eq!(self.heap.interned_count() as usize, self.interned.len());
    }

    fn op_intern_str(&mut self, inp: &mut Input<'_>) {
        if self.strs.is_empty() {
            return;
        }
        let k = inp.below(self.strs.len());
        let r = self.strs[k].r;
        let s = self.strs[k].s.clone();
        match self.heap.intern_str(r) {
            Ok(got) => {
                if let Some(&old) = self.interned.get(&s) {
                    assert_eq!(got, old);
                } else {
                    assert_eq!(got, r);
                    self.interned.insert(s, r);
                    self.strs[k].interned = true;
                }
                let h = self.heap.header(got).unwrap();
                assert_ne!(h.flags() & flags::INTERNED, 0);
            }
            Err(e) => {
                self.orphans = true;
                self.oom(e);
            }
        }
    }

    /// Forged, stale and mid-object references.
    fn op_probe(&mut self, inp: &mut Input<'_>) {
        let raw = match inp.u8() % 3 {
            0 if !self.objs.is_empty() => {
                let base = self.objs[inp.below(self.objs.len())].r.raw();
                base.wrapping_add(u32::from(inp.u8() % 8))
            }
            1 if !self.blks.is_empty() => {
                let base = self.blks[inp.below(self.blks.len())].b.raw();
                let b = ByteRef::from_raw(base.wrapping_add(u32::from(inp.u8() % 8)));
                self.probe_bytes(b);
                return;
            }
            _ => inp.u32(),
        };
        let r = Ref::from_raw(raw);
        let modelled = self.obj_at.contains_key(&raw) || self.str_at.contains_key(&raw);
        match self.heap.header(r) {
            Ok(h) => assert!(
                modelled || h.type_id() == types::INTERN_TABLE || self.orphans,
                "forged ref resolved to an object"
            ),
            Err(e) => {
                assert!(!modelled, "live object not found");
                assert_eq!(e, Error::BadRef);
                assert_eq!(self.heap.get(r, 0), Err(Error::BadRef));
                assert_eq!(self.heap.get_raw(r, 0), Err(Error::BadRef));
                assert_eq!(self.heap.set(r, 0, Value::NONE), Err(Error::BadRef));
                assert_eq!(self.heap.set_raw(r, 0, 1), Err(Error::BadRef));
                assert!(self.heap.as_str(r).is_err());
            }
        }
        self.probe_bytes(ByteRef::from_raw(inp.u32()));
    }

    fn probe_bytes(&mut self, b: ByteRef) {
        let modelled = self.blk_at.contains_key(&b.raw());
        let str_data = |s: &Str| self.heap.get(s.r, 0).ok().and_then(Value::as_byte_ref);
        match self.heap.bytes(b) {
            Ok(_) => assert!(
                modelled || self.orphans || self.strs.iter().any(|s| str_data(s) == Some(b)),
                "forged byte ref resolved to a block"
            ),
            Err(e) => {
                assert!(!modelled);
                assert_eq!(e, Error::BadRef);
                assert!(self.heap.bytes_mut(b).is_err());
            }
        }
    }

    fn op_bad_slots(&mut self, inp: &mut Input<'_>) {
        if !self.strs.is_empty() {
            let r = self.strs[inp.below(self.strs.len())].r;
            assert_eq!(self.heap.set(r, 0, Value::NONE), Err(Error::BadType));
            assert_eq!(self.heap.set_raw(r, 1, 0), Err(Error::BadType));
            assert_eq!(self.heap.set_flags(r, flags::FROZEN), Err(Error::BadType));
        }
        if self.objs.is_empty() {
            return;
        }
        let o = &self.objs[inp.below(self.objs.len())];
        let (nv, len) = (o.vals.len() as u32, (o.vals.len() + o.raws.len()) as u32);
        let extra = u32::from(inp.u8()) % 4;
        if o.raws.is_empty() {
            assert_eq!(self.heap.get(o.r, len + extra), Err(Error::BadSlot));
        } else {
            assert_eq!(self.heap.get(o.r, nv + extra), Err(Error::BadSlot));
            assert_eq!(
                self.heap.set(o.r, nv + extra, Value::NONE),
                Err(Error::BadSlot)
            );
        }
        assert_eq!(self.heap.get_raw(o.r, len + extra), Err(Error::BadSlot));
        assert_eq!(self.heap.set_raw(o.r, len + extra, 0), Err(Error::BadSlot));
        if nv > 0 {
            assert_eq!(self.heap.get_raw(o.r, extra % nv), Err(Error::BadSlot));
            assert_eq!(self.heap.set_raw(o.r, extra % nv, 0), Err(Error::BadSlot));
        }
        assert_eq!(
            self.heap.set_flags(o.r, flags::INTERNED),
            Err(Error::BadType)
        );
        assert_eq!(self.heap.alloc(types::STR, 0, 1, 0), Err(Error::BadType));
        assert_eq!(
            self.heap.alloc(types::FIRST_ENGINE, flags::ASCII, 1, 0),
            Err(Error::BadType)
        );
        assert_eq!(
            self.heap.alloc(types::FIRST_ENGINE, 0, 255, 1),
            Err(Error::BadType)
        );
    }

    fn op_set_flags(&mut self, inp: &mut Input<'_>) {
        if self.objs.is_empty() {
            return;
        }
        let k = inp.below(self.objs.len());
        let fl = inp.u8() & !flags::CORE;
        let r = self.objs[k].r;
        self.heap.set_flags(r, fl).unwrap();
        self.objs[k].flags = fl;
        assert_eq!(self.heap.header(r).unwrap().flags(), fl);
    }

    /// `would_fit` false must mean the allocation fails.
    fn op_preflight(&mut self, inp: &mut Input<'_>) {
        let bytes = inp.below(1 << 15) * 64;
        assert!(!self.heap.would_fit(self.limit * 2));
        if !self.heap.would_fit(bytes as u64) {
            assert_eq!(
                self.heap.alloc_bytes_zeroed(bytes),
                Err(Error::OutOfMemory),
                "would_fit said no but the allocation succeeded"
            );
            self.sum.oom += 1;
        }
    }

    fn pick_value(&self, inp: &mut Input<'_>) -> Value {
        let v = match inp.u8() % 10 {
            0 => Value::int((inp.u64() as i64) >> 16),
            1 => Some(Value::float(f64::from_bits(inp.u64()))),
            2 => Some(Value::special(SPECIALS[inp.below(6)])),
            3 => Some(Value::native(inp.u32())),
            4..=6 if !self.objs.is_empty() => {
                let o = &self.objs[inp.below(self.objs.len())];
                Some(Value::from_ref(o.r, inp.u8()))
            }
            7 | 8 if !self.strs.is_empty() => {
                let s = &self.strs[inp.below(self.strs.len())];
                Some(Value::from_ref(s.r, inp.u8()))
            }
            9 if !self.blks.is_empty() => Some(Value::from_byte_ref(
                self.blks[inp.below(self.blks.len())].b,
            )),
            _ => None,
        };
        v.unwrap_or(Value::NONE)
    }

    fn check_obj(&self, o: &Obj) {
        let words = self.heap.slot_words(o.r).unwrap();
        assert_eq!(words.len(), o.vals.len() + o.raws.len());
        assert_eq!(self.heap.raw_words(o.r).unwrap(), &o.raws[..]);
        let h = self.heap.header(o.r).unwrap();
        assert_eq!(h.type_id(), o.type_id);
        assert_eq!(h.flags(), o.flags);
        assert_eq!(h.nvals() as usize, o.vals.len());
        assert_eq!(h.len() as usize, o.vals.len() + o.raws.len());
        for (i, v) in o.vals.iter().enumerate() {
            assert_eq!(self.heap.get(o.r, i as u32), Ok(*v), "value slot changed");
        }
        for (i, w) in o.raws.iter().enumerate() {
            let slot = (o.vals.len() + i) as u32;
            assert_eq!(self.heap.get_raw(o.r, slot), Ok(*w), "raw slot changed");
        }
    }

    fn check_blk(&self, b: &Blk) {
        assert_eq!(self.heap.bytes(b.b).unwrap(), &b.data[..], "block changed");
    }

    fn check_str(&self, s: &Str) {
        assert_eq!(self.heap.as_str(s.r).unwrap(), s.s);
        assert_eq!(self.heap.str_hash(s.r), Ok(hash_bytes(s.s.as_bytes())));
        assert_eq!(self.heap.str_chars(s.r), Ok(s.s.chars().count() as u64));
        let h = self.heap.header(s.r).unwrap();
        assert_eq!(h.type_id(), types::STR);
        assert_eq!(h.flags() & flags::ASCII != 0, s.s.is_ascii());
        assert_eq!(h.flags() & flags::INTERNED != 0, s.interned);
    }

    /// Collects, then checks that exactly the model's reachable items survive
    /// unchanged and everything else is gone.
    fn collect(&mut self) {
        let roots = self.roots.clone();
        let before = self.heap.stats().collections;
        let stats = self.heap.collect(&mut |t| t.values(&roots));
        assert_eq!(stats.collections, before + 1);
        self.sum.collections += 1;
        self.orphans = false;

        // Reachability in the model.
        let mut live_obj = vec![false; self.objs.len()];
        let mut live_blk = vec![false; self.blks.len()];
        let mut live_str = vec![false; self.strs.len()];
        let mut work: Vec<Value> = roots;
        while let Some(v) = work.pop() {
            if let Some(r) = v.as_obj() {
                if let Some(&i) = self.obj_at.get(&r.raw()) {
                    if !live_obj[i] {
                        live_obj[i] = true;
                        work.extend_from_slice(&self.objs[i].vals);
                    }
                } else if let Some(&i) = self.str_at.get(&r.raw()) {
                    live_str[i] = true;
                }
            } else if let Some(b) = v.as_byte_ref()
                && let Some(&i) = self.blk_at.get(&b.raw())
            {
                live_blk[i] = true;
            }
        }

        // Dead items are gone.
        let mut objs = Vec::new();
        for (o, live) in std::mem::take(&mut self.objs).into_iter().zip(live_obj) {
            if live {
                objs.push(o);
            } else {
                assert_eq!(
                    self.heap.header(o.r),
                    Err(Error::BadRef),
                    "dead object kept"
                );
            }
        }
        let mut blks = Vec::new();
        for (b, live) in std::mem::take(&mut self.blks).into_iter().zip(live_blk) {
            if live {
                blks.push(b);
            } else {
                assert_eq!(self.heap.bytes(b.b), Err(Error::BadRef), "dead block kept");
            }
        }
        let mut strs = Vec::new();
        for (s, live) in std::mem::take(&mut self.strs).into_iter().zip(live_str) {
            if live {
                strs.push(s);
            } else {
                assert_eq!(self.heap.header(s.r), Err(Error::BadRef), "dead str kept");
                if s.interned {
                    assert_eq!(
                        self.heap.find_interned(&s.s),
                        None,
                        "dead intern entry kept"
                    );
                    self.interned.remove(&s.s);
                }
            }
        }
        self.objs = objs;
        self.blks = blks;
        self.strs = strs;
        self.obj_at = self
            .objs
            .iter()
            .enumerate()
            .map(|(i, o)| (o.r.raw(), i))
            .collect();
        self.blk_at = self
            .blks
            .iter()
            .enumerate()
            .map(|(i, b)| (b.b.raw(), i))
            .collect();
        self.str_at = self
            .strs
            .iter()
            .enumerate()
            .map(|(i, s)| (s.r.raw(), i))
            .collect();

        // Live items are unchanged, and the live byte count is theirs plus at
        // most one intern table (slot 0 + a power-of-two array >= 64).
        let mut model_bytes = 0;
        for o in &self.objs {
            self.check_obj(o);
            model_bytes += (1 + o.vals.len() + o.raws.len()) as u64 * 8;
        }
        for b in &self.blks {
            self.check_blk(b);
            model_bytes += block_bytes(b.data.len());
        }
        for s in &self.strs {
            self.check_str(s);
            model_bytes += 4 * 8 + block_bytes(s.s.len());
        }
        let extra = stats
            .live_bytes
            .checked_sub(model_bytes)
            .expect("live bytes below the model's");
        let table_cap = (extra / 8).wrapping_sub(2);
        assert!(
            extra == 0 || (table_cap >= 64 && table_cap.is_power_of_two()),
            "live bytes {} vs model {model_bytes}",
            stats.live_bytes
        );
        assert_eq!(self.heap.interned_count() as usize, self.interned.len());
    }
}
