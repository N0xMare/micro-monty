//! Segmented word arena and VM heap state shared by every variant
//! (docs/spec.md §7.2, §8.3, §8.6).

use crate::access::{seg_ld, seg_st};
use crate::value::{Value, kind};

/// log2 of the segment size in words (2^17 words = 1 MiB).
pub const SEG_BITS: u32 = 17;
pub const SEG_WORDS: u64 = 1 << SEG_BITS;
const WORD_MASK: u32 = (1 << SEG_BITS) - 1;
/// A Ref is a u32 `seg << 17 | word`, so at most 2^15 segments.
const MAX_SEGS: u64 = 1 << (32 - SEG_BITS);

/// Fuel quantum: ticks (backward jumps + calls) between safepoint polls.
pub const FUEL: i64 = 10_000;

/// Errors (a real engine raises Python exceptions; the spike just stops).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Fail {
    Type,
    Attr,
    Key,
    Index,
    Name,
    Arity,
    Recursion,
    Memory,
    BadOp,
}

/// Interned-name id (attribute and global names).
pub type Name = u8;

/// Name ids used by the kernels.
pub mod names {
    use super::Name;
    pub const X: Name = 1;
    pub const Y: Name = 2;
    pub const K: Name = 3;
    pub const INC: Name = 4;
    pub const APPEND: Name = 5;
    pub const FIB: Name = 6;
    pub const MAKE: Name = 7;
    pub const RANGE: Name = 8;
}

/// One inline-cache entry. `key` is the guard (dict version, shape id or class
/// version), `val` the cached result (global slot, inline slot or function).
/// ICs live outside the arena in a side table and start empty (spec §8.1).
#[derive(Clone, Copy, Debug)]
pub struct Ic {
    pub key: u64,
    pub val: u64,
    pub name: Name,
}

/// Guard value of an empty IC entry; no version/shape ever equals it.
pub const IC_EMPTY: u64 = u64::MAX;

/// Saved caller state, pushed on a Python→Python call.
#[derive(Clone, Copy, Debug)]
pub struct Rec {
    pub pc: u32,
    pub base: u32,
    pub code: u32,
    pub func: Value,
}

/// Segmented word arena: `Vec<Box<[u64]>>`, bump allocation, no GC.
pub struct Arena {
    segs: Vec<Box<[u64]>>,
    top: u64,
    limit: u64,
}

fn new_seg() -> Box<[u64]> {
    vec![0u64; SEG_WORDS as usize].into_boxed_slice()
}

impl Arena {
    pub fn new() -> Arena {
        // Address 0 is reserved so a zero word is never a valid object.
        Arena {
            segs: vec![new_seg()],
            top: 1,
            limit: SEG_WORDS,
        }
    }

    /// Forget every object but keep the segments (stands in for a full GC
    /// between benchmark runs).
    pub fn reset(&mut self) {
        self.top = 1;
        self.limit = SEG_WORDS;
    }

    pub fn segments(&self) -> usize {
        self.segs.len()
    }

    #[inline(always)]
    pub fn get(&self, a: u32) -> u64 {
        seg_ld(
            &self.segs,
            (a >> SEG_BITS) as usize,
            (a & WORD_MASK) as usize,
        )
    }

    #[inline(always)]
    pub fn set(&mut self, a: u32, v: u64) {
        seg_st(
            &mut self.segs,
            (a >> SEG_BITS) as usize,
            (a & WORD_MASK) as usize,
            v,
        )
    }

    /// Allocate `n` uninitialised words (callers initialise every word they read).
    #[inline]
    pub fn alloc(&mut self, n: usize) -> Result<u32, Fail> {
        let n = n as u64;
        if self.top + n <= self.limit {
            let a = self.top;
            self.top += n;
            Ok(a as u32)
        } else {
            self.alloc_slow(n)
        }
    }

    /// Start at the next segment boundary; an object larger than a segment
    /// gets a run of consecutive segments.
    #[inline(never)]
    fn alloc_slow(&mut self, n: u64) -> Result<u32, Fail> {
        let seg = self.top.div_ceil(SEG_WORDS);
        let nsegs = n.div_ceil(SEG_WORDS).max(1);
        if seg + nsegs > MAX_SEGS {
            return Err(Fail::Memory);
        }
        while (self.segs.len() as u64) < seg + nsegs {
            self.segs.push(new_seg());
        }
        let a = seg * SEG_WORDS;
        self.top = a + n;
        self.limit = (seg + nsegs) * SEG_WORDS;
        Ok(a as u32)
    }
}

/// Header word: kind in the low byte, aux (shape id, cell count, ...) above.
#[inline(always)]
pub fn header(k: u8, aux: u32) -> u64 {
    k as u64 | (aux as u64) << 8
}

/// Version tag of the builtin `list` type (user classes start at 100).
pub const LIST_TYPE_VERSION: u64 = 1;

/// Everything the handlers mutate except the frame stack.
pub struct Heap {
    pub arena: Arena,
    pub globals: Vec<Value>,
    pub global_names: Vec<Name>,
    /// Simulated globals-dict version; never changes in the spike, but is a
    /// real field read on every `LOAD_GLOBAL`.
    pub globals_version: u64,
    pub ics: Vec<Ic>,
    /// Control stack: one `Rec` per active Python→Python call.
    pub ctrl: Vec<Rec>,
    /// Shape id → attribute names in slot order.
    pub shapes: Vec<Vec<Name>>,
    pub fuel: i64,
    pub refills: u64,
    pub safepoint_pc: usize,
    /// Dispatched-instruction counter (only incremented with `--features count`).
    pub count: u64,
    next_class_version: u64,
}

impl Heap {
    pub fn new() -> Heap {
        Heap {
            arena: Arena::new(),
            globals: Vec::new(),
            global_names: Vec::new(),
            globals_version: 1,
            ics: Vec::new(),
            ctrl: Vec::with_capacity(1 << 12),
            shapes: Vec::new(),
            fuel: FUEL,
            refills: 0,
            safepoint_pc: 0,
            count: 0,
            next_class_version: 100,
        }
    }

    /// Fresh heap state for one run of `ic_names`' program.
    pub fn reset(&mut self, ic_names: &[Name]) {
        self.arena.reset();
        self.globals.clear();
        self.global_names.clear();
        self.ics.clear();
        self.ics.extend(ic_names.iter().map(|&name| Ic {
            key: IC_EMPTY,
            val: 0,
            name,
        }));
        self.ctrl.clear();
        self.shapes.clear();
        self.fuel = FUEL;
        self.refills = 0;
        self.safepoint_pc = 0;
        self.count = 0;
        self.next_class_version = 100;
    }

    /// Total safepoint ticks so far (backward jumps + calls).
    pub fn ticks(&self) -> u64 {
        self.refills * FUEL as u64 + (FUEL - self.fuel) as u64
    }

    pub fn define_global(&mut self, name: Name, v: Value) {
        self.global_names.push(name);
        self.globals.push(v);
    }

    pub fn add_shape(&mut self, names: Vec<Name>) -> u32 {
        self.shapes.push(names);
        (self.shapes.len() - 1) as u32
    }

    /// Class: `[hdr, version, n, name0, fn0, name1, fn1, ...]`.
    pub fn new_class(&mut self, methods: &[(Name, Value)]) -> Result<Value, Fail> {
        let a = self.arena.alloc(3 + 2 * methods.len())?;
        self.arena.set(a, header(kind::CLASS, 0));
        self.arena.set(a + 1, self.next_class_version);
        self.next_class_version += 1;
        self.arena.set(a + 2, methods.len() as u64);
        for (j, &(name, f)) in methods.iter().enumerate() {
            self.arena.set(a + 3 + 2 * j as u32, name as u64);
            self.arena.set(a + 4 + 2 * j as u32, f.0);
        }
        Ok(Value::obj(kind::CLASS, a))
    }

    /// Instance: `[hdr(shape), class, slot0, slot1, ...]`.
    pub fn new_instance(&mut self, cls: Value, shape: u32, slots: &[Value]) -> Result<Value, Fail> {
        let a = self.arena.alloc(2 + slots.len())?;
        self.arena.set(a, header(kind::INST, shape));
        self.arena.set(a + 1, cls.0);
        for (j, v) in slots.iter().enumerate() {
            self.arena.set(a + 2 + j as u32, v.0);
        }
        Ok(Value::obj(kind::INST, a))
    }
}
