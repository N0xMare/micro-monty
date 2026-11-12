//! Segmented word arena, objects, dicts, exceptions and a precise mark phase.
//!
//! Every word after an object's header is a `Value` (counts are SmallInts),
//! except `str` payload bytes, so the tracer needs only the kind and length.

use crate::value::{Value, kind};

pub const SEG_BITS: u32 = 17;
pub const SEG_WORDS: u64 = 1 << SEG_BITS;
const WORD_MASK: u32 = (1 << SEG_BITS) - 1;
const MAX_SEGS: u64 = 1 << (32 - SEG_BITS);

/// Exception kinds (the spike's exception objects carry a kind and detail).
pub mod exc {
    pub const RECURSION: i64 = 1;
    pub const TYPE: i64 = 2;
    pub const NAME: i64 = 3;
    pub const KEY: i64 = 4;
    pub const OVERFLOW: i64 = 5;
    pub const MEMORY: i64 = 6;
    pub const UNBOUND_LOCAL: i64 = 7;
    pub const VALUE: i64 = 8;
    /// Spike-only: a feature the spike leaves out (the engine has it).
    pub const UNSUPPORTED: i64 = 9;
    pub const SYSTEM: i64 = 10;
}

/// A raised exception (an `EXC` object), returned by slow paths.
#[derive(Clone, Copy, Debug)]
pub struct Exc(pub Value);

/// Header word: kind in the low byte, length in words (header included) above.
#[inline(always)]
pub fn header(k: u8, len: u32) -> u64 {
    k as u64 | (len as u64) << 8
}

// Object layouts (word offsets).
pub const FN_CODE: u32 = 1; // SmallInt code index
pub const FN_CELLS: u32 = 2; // inline cells follow
pub const CELL_VAL: u32 = 1;
pub const D_USED: u32 = 1;
pub const D_N: u32 = 2;
pub const D_MASK: u32 = 3;
pub const D_INDEX: u32 = 4;
pub const D_ENTRIES: u32 = 5;
pub const STR_HASH: u32 = 1;
pub const STR_LEN: u32 = 2;
pub const STR_BYTES: u32 = 3;
pub const RI_CUR: u32 = 1;
pub const RI_STOP: u32 = 2;
pub const RI_STEP: u32 = 3;
pub const EXC_KIND: u32 = 1;

pub struct Arena {
    pub segs: Vec<Box<[u64]>>,
    pub top: u64,
    pub limit: u64,
}

fn new_seg() -> Box<[u64]> {
    vec![0u64; SEG_WORDS as usize].into_boxed_slice()
}

impl Arena {
    fn new() -> Arena {
        Arena {
            segs: vec![new_seg()],
            top: 1,
            limit: SEG_WORDS,
        }
    }

    /// Checked read; an out-of-range address reads 0 (not a valid value or
    /// header), never host memory, never a panic.
    #[inline(always)]
    pub fn get(&self, a: u32) -> u64 {
        match self.segs.get((a >> SEG_BITS) as usize) {
            Some(s) => s.get((a & WORD_MASK) as usize).copied().unwrap_or(0),
            None => 0,
        }
    }

    /// Checked write (the write barrier would go here); out of range is ignored.
    #[inline(always)]
    pub fn set(&mut self, a: u32, v: u64) {
        if let Some(s) = self.segs.get_mut((a >> SEG_BITS) as usize)
            && let Some(w) = s.get_mut((a & WORD_MASK) as usize)
        {
            *w = v;
        }
    }

    fn alloc(&mut self, n: u64) -> Option<u32> {
        if self.top + n <= self.limit {
            let a = self.top;
            self.top += n;
            return Some(a as u32);
        }
        let seg = self.top.div_ceil(SEG_WORDS);
        let nsegs = n.div_ceil(SEG_WORDS).max(1);
        if seg + nsegs > MAX_SEGS {
            return None;
        }
        while (self.segs.len() as u64) < seg + nsegs {
            self.segs.push(new_seg());
        }
        let a = seg * SEG_WORDS;
        self.top = a + n;
        self.limit = (seg + nsegs) * SEG_WORDS;
        Some(a as u32)
    }
}

pub struct Heap {
    pub arena: Arena,
    /// Globals version: bumped whenever a globals dict's keys or entries
    /// buffer change (as mm-engine's `gver`). IC entries compare against it.
    pub gver: u64,
    pub globals: Value,
    /// Set by allocation past the threshold; checked at every safepoint.
    pub gc_pending: bool,
    pub allocated: u64,
    pub threshold: u64,
    pub collections: u64,
    pub last_marked: u64,
}

impl Heap {
    pub fn new() -> Heap {
        let mut h = Heap {
            arena: Arena::new(),
            gver: 1,
            globals: Value::NONE,
            gc_pending: false,
            allocated: 0,
            threshold: 1 << 20,
            collections: 0,
            last_marked: 0,
        };
        h.globals = h.dict_new().unwrap_or(Value::NONE);
        h
    }

    pub fn alloc(&mut self, k: u8, len: u32) -> Result<u32, Exc> {
        let Some(a) = self.arena.alloc(len as u64) else {
            return Err(Exc(Value::NONE));
        };
        self.arena.set(a, header(k, len));
        self.allocated += len as u64;
        if self.allocated > self.threshold {
            self.gc_pending = true;
        }
        Ok(a)
    }

    #[inline(always)]
    pub fn val(&self, a: u32) -> Value {
        Value(self.arena.get(a))
    }
    #[inline(always)]
    pub fn put(&mut self, a: u32, v: Value) {
        self.arena.set(a, v.0)
    }

    pub fn exc(&mut self, k: i64) -> Exc {
        match self.alloc(kind::EXC, 2) {
            Ok(a) => {
                self.put(a + EXC_KIND, Value::int(k));
                Exc(Value::obj(kind::EXC, a))
            }
            // Out of memory while raising: a preallocated-style bare kind.
            Err(_) => Exc(Value::int(k)),
        }
    }

    pub fn exc_kind(&self, e: Value) -> i64 {
        if e.is_obj(kind::EXC) {
            self.val(e.addr() + EXC_KIND).int_val()
        } else {
            e.int_val()
        }
    }

    pub fn str_new(&mut self, s: &str) -> Value {
        let b = s.as_bytes();
        let nw = b.len().div_ceil(8) as u32;
        let Ok(a) = self.alloc(kind::STR, STR_BYTES + nw) else {
            return Value::NONE;
        };
        // FNV-1a, kept within SmallInt range.
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for &c in b {
            h = (h ^ c as u64).wrapping_mul(0x100_0000_01b3);
        }
        self.put(a + STR_HASH, Value::int((h >> 17) as i64));
        self.put(a + STR_LEN, Value::int(b.len() as i64));
        for (i, ch) in b.chunks(8).enumerate() {
            let mut w = [0u8; 8];
            w[..ch.len()].copy_from_slice(ch);
            self.arena
                .set(a + STR_BYTES + i as u32, u64::from_le_bytes(w));
        }
        Value::obj(kind::STR, a)
    }

    pub fn func_new(&mut self, code: u32, cells: &[Value]) -> Result<Value, Exc> {
        let a = self.alloc(kind::FUNC, FN_CELLS + cells.len() as u32)?;
        self.put(a + FN_CODE, Value::int(code as i64));
        for (i, &c) in cells.iter().enumerate() {
            self.put(a + FN_CELLS + i as u32, c);
        }
        Ok(Value::obj(kind::FUNC, a))
    }

    pub fn cell_new(&mut self, v: Value) -> Result<Value, Exc> {
        let a = self.alloc(kind::CELL, 2)?;
        self.put(a + CELL_VAL, v);
        Ok(Value::obj(kind::CELL, a))
    }

    // ------------------------------------------------------------ dicts

    fn buf(&mut self, n: u32, fill: Value) -> Result<u32, Exc> {
        let a = self.alloc(kind::BUF, 1 + n)?;
        for i in 0..n {
            self.put(a + 1 + i, fill);
        }
        Ok(a)
    }

    pub fn dict_new(&mut self) -> Result<Value, Exc> {
        let d = self.alloc(kind::DICT, 6)?;
        let idx = self.buf(8, Value::UNDEF)?;
        let ent = self.buf(3 * 5, Value::UNDEF)?;
        self.put(d + D_USED, Value::int(0));
        self.put(d + D_N, Value::int(0));
        self.put(d + D_MASK, Value::int(7));
        self.put(d + D_INDEX, Value::obj(kind::BUF, idx));
        self.put(d + D_ENTRIES, Value::obj(kind::BUF, ent));
        Ok(Value::obj(kind::DICT, d))
    }

    /// 47-bit hash, so it round-trips through a stored SmallInt.
    fn hash(&self, k: Value) -> Result<u64, ()> {
        let h = if k.is_int() {
            k.int_val() as u64
        } else if k.is_obj(kind::STR) {
            self.val(k.addr() + STR_HASH).int_val() as u64
        } else {
            return Err(());
        };
        Ok(h & ((1 << 47) - 1))
    }

    fn key_eq(&self, a: Value, b: Value) -> bool {
        if a == b {
            return true;
        }
        if !(a.is_obj(kind::STR) && b.is_obj(kind::STR)) {
            return false;
        }
        let (x, y) = (a.addr(), b.addr());
        let n = self.val(x + STR_LEN);
        if n != self.val(y + STR_LEN) || self.val(x + STR_HASH) != self.val(y + STR_HASH) {
            return false;
        }
        let nw = (n.int_val() as u32).div_ceil(8);
        (0..nw).all(|i| self.arena.get(x + STR_BYTES + i) == self.arena.get(y + STR_BYTES + i))
    }

    /// Entry index of `key`, and the index slot where it is or would go.
    fn dict_find(&self, d: u32, key: Value, h: u64) -> (Option<u32>, u32) {
        let mask = self.val(d + D_MASK).int_val() as u64;
        let idx = self.val(d + D_INDEX).addr();
        let ent = self.val(d + D_ENTRIES).addr();
        let mut i = h & mask;
        let mut perturb = h;
        loop {
            let s = self.val(idx + 1 + i as u32);
            if s == Value::UNDEF {
                return (None, i as u32);
            }
            let e = s.int_val() as u32;
            let base = ent + 1 + 3 * e;
            if self.val(base).int_val() as u64 == h && self.key_eq(self.val(base + 1), key) {
                return (Some(e), i as u32);
            }
            perturb >>= 5;
            i = (i * 5 + 1 + perturb) & mask;
        }
    }

    /// `(entries buffer, entry index)` of `key`.
    pub fn dict_lookup(&self, d: Value, key: Value) -> Result<Option<(u32, u32)>, ()> {
        let h = self.hash(key)?;
        let (e, _) = self.dict_find(d.addr(), key, h);
        Ok(e.map(|e| (self.val(d.addr() + D_ENTRIES).addr(), e)))
    }

    /// Sets `d[key] = v`; returns true if the keys or the entries buffer
    /// changed (a globals dict must bump `gver`).
    pub fn dict_set(&mut self, dv: Value, key: Value, v: Value) -> Result<bool, Exc> {
        let Ok(h) = self.hash(key) else {
            return Err(self.exc(exc::TYPE));
        };
        let d = dv.addr();
        let (e, slot) = self.dict_find(d, key, h);
        if let Some(e) = e {
            let ent = self.val(d + D_ENTRIES).addr();
            self.put(ent + 1 + 3 * e + 2, v);
            return Ok(false);
        }
        let n = self.val(d + D_N).int_val() as u32;
        let mask = self.val(d + D_MASK).int_val() as u32;
        let cap = (mask + 1) * 2 / 3;
        if n >= cap {
            self.dict_resize(d, (mask + 1) * 2)?;
            return self.dict_set(dv, key, v).map(|_| true);
        }
        let ent = self.val(d + D_ENTRIES).addr();
        let idx = self.val(d + D_INDEX).addr();
        let base = ent + 1 + 3 * n;
        self.put(base, Value::int(h as i64));
        self.put(base + 1, key);
        self.put(base + 2, v);
        self.put(idx + 1 + slot, Value::int(n as i64));
        self.put(d + D_N, Value::int(n as i64 + 1));
        let used = self.val(d + D_USED).int_val();
        self.put(d + D_USED, Value::int(used + 1));
        Ok(true)
    }

    fn dict_resize(&mut self, d: u32, slots: u32) -> Result<(), Exc> {
        let n = self.val(d + D_N).int_val() as u32;
        let old = self.val(d + D_ENTRIES).addr();
        let idx = self.buf(slots, Value::UNDEF)?;
        let ent = self.buf(3 * (slots * 2 / 3), Value::UNDEF)?;
        let mask = slots - 1;
        for e in 0..n {
            let b = old + 1 + 3 * e;
            let h = self.val(b).int_val() as u64;
            let mut i = h & mask as u64;
            let mut perturb = h;
            while self.val(idx + 1 + i as u32) != Value::UNDEF {
                perturb >>= 5;
                i = (i * 5 + 1 + perturb) & mask as u64;
            }
            self.put(idx + 1 + i as u32, Value::int(e as i64));
            for k in 0..3 {
                let v = self.val(b + k);
                self.put(ent + 1 + 3 * e + k, v);
            }
        }
        self.put(d + D_MASK, Value::int(mask as i64));
        self.put(d + D_INDEX, Value::obj(kind::BUF, idx));
        self.put(d + D_ENTRIES, Value::obj(kind::BUF, ent));
        Ok(())
    }

    // ---------------------------------------------------------------- GC

    /// Precise mark from `roots` with a side bitmap and an explicit work
    /// list (no recursion). Non-moving; the spike's bump arena does not
    /// sweep, so a collection only proves the roots are walkable and sets
    /// the next threshold.
    pub fn collect(&mut self, roots: &[Value], extra: &[Value]) {
        let words = self.arena.segs.len() << SEG_BITS;
        let mut marks = vec![0u64; words.div_ceil(64)];
        let mut work: Vec<u32> = Vec::new();
        let mut live: u64 = 0;
        let push = |v: Value, work: &mut Vec<u32>| {
            if v.is_ref() {
                work.push(v.addr());
            }
        };
        for &v in roots.iter().chain(extra) {
            push(v, &mut work);
        }
        while let Some(a) = work.pop() {
            let (w, b) = ((a as usize) / 64, a % 64);
            let Some(m) = marks.get_mut(w) else { continue };
            if *m & (1 << b) != 0 {
                continue;
            }
            *m |= 1 << b;
            let h = self.arena.get(a);
            let (k, len) = (h as u8, (h >> 8) as u32);
            live += len as u64;
            if k == kind::STR {
                continue;
            }
            for i in 1..len {
                push(self.val(a + i), &mut work);
            }
        }
        self.collections += 1;
        self.last_marked = live;
        self.allocated = 0;
        self.threshold = (live * 2).max(1 << 20);
        self.gc_pending = false;
    }
}
