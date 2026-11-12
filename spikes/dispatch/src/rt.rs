//! Operation semantics shared by every variant: generic arithmetic and
//! comparison with inline SmallInt fast paths, truthiness, inline caches,
//! object operations, natives and the safepoint. Handlers call these; only
//! operand access and dispatch differ between variants.

use crate::heap::{Fail, Heap, LIST_TYPE_VERSION, header, names};
use crate::value::{Value, kind};

// ---------------------------------------------------------------- safepoint

/// Called at backward jumps and calls (spec §8.6).
#[inline(always)]
pub fn tick(heap: &mut Heap, pc: usize) {
    heap.fuel -= 1;
    if heap.fuel <= 0 {
        safepoint(heap, pc);
    }
}

/// The cold path: a real engine would write back pc/sp and poll for
/// deadline/GC/park here. The spike records the pc and refills.
#[cold]
#[inline(never)]
fn safepoint(heap: &mut Heap, pc: usize) {
    heap.refills += 1;
    heap.safepoint_pc = pc;
    heap.fuel = crate::heap::FUEL;
}

// --------------------------------------------------------------- arithmetic

#[inline(always)]
pub fn add(a: Value, b: Value) -> Result<Value, Fail> {
    if a.is_int() & b.is_int() {
        let r = a.int_val() + b.int_val();
        if Value::fits_int(r) {
            return Ok(Value::int(r));
        }
    }
    add_slow(a, b)
}

#[inline(always)]
pub fn sub(a: Value, b: Value) -> Result<Value, Fail> {
    if a.is_int() & b.is_int() {
        let r = a.int_val() - b.int_val();
        if Value::fits_int(r) {
            return Ok(Value::int(r));
        }
    }
    sub_slow(a, b)
}

/// Generic `<` producing a bool `Value`.
#[inline(always)]
pub fn lt(a: Value, b: Value) -> Result<Value, Fail> {
    if a.is_int() & b.is_int() {
        return Ok(Value::from_bool(a.int_val() < b.int_val()));
    }
    lt_slow(a, b).map(Value::from_bool)
}

/// `<` feeding a conditional jump directly (fused compare-and-jump).
#[inline(always)]
pub fn lt_bool(a: Value, b: Value) -> Result<bool, Fail> {
    if a.is_int() & b.is_int() {
        return Ok(a.int_val() < b.int_val());
    }
    lt_slow(a, b)
}

/// Truthiness for conditional jumps.
#[inline(always)]
pub fn truthy(heap: &Heap, v: Value) -> Result<bool, Fail> {
    if v == Value::TRUE {
        Ok(true)
    } else if v == Value::FALSE {
        Ok(false)
    } else {
        truthy_slow(heap, v)
    }
}

fn as_f64(v: Value) -> Option<f64> {
    if v.is_int() {
        Some(v.int_val() as f64)
    } else if v.is_float() {
        Some(v.float_val())
    } else {
        None
    }
}

/// Floats, and SmallInt overflow. A real engine promotes to a heap bigint;
/// the spike produces a float (no kernel overflows i48).
#[cold]
#[inline(never)]
fn add_slow(a: Value, b: Value) -> Result<Value, Fail> {
    match (as_f64(a), as_f64(b)) {
        (Some(x), Some(y)) => Ok(Value::float(x + y)),
        _ => Err(Fail::Type),
    }
}

#[cold]
#[inline(never)]
fn sub_slow(a: Value, b: Value) -> Result<Value, Fail> {
    match (as_f64(a), as_f64(b)) {
        (Some(x), Some(y)) => Ok(Value::float(x - y)),
        _ => Err(Fail::Type),
    }
}

#[cold]
#[inline(never)]
fn lt_slow(a: Value, b: Value) -> Result<bool, Fail> {
    match (as_f64(a), as_f64(b)) {
        (Some(x), Some(y)) => Ok(x < y),
        _ => Err(Fail::Type),
    }
}

#[cold]
#[inline(never)]
fn truthy_slow(heap: &Heap, v: Value) -> Result<bool, Fail> {
    if v.is_int() {
        Ok(v.int_val() != 0)
    } else if v.is_float() {
        Ok(v.float_val() != 0.0)
    } else if v == Value::NONE {
        Ok(false)
    } else if v.is_obj(kind::LIST) || v.is_obj(kind::DICT) {
        Ok(heap.arena.get(v.addr() + 1) != 0)
    } else {
        Ok(true)
    }
}

// ------------------------------------------------------------------ globals

/// `LOAD_GLOBAL`: slot by index, guarded by the globals-dict version.
#[inline(always)]
pub fn load_global(heap: &mut Heap, ic: usize) -> Result<Value, Fail> {
    let e = heap.ics[ic];
    if e.key == heap.globals_version {
        return Ok(heap.globals[e.val as usize]);
    }
    load_global_slow(heap, ic)
}

#[cold]
#[inline(never)]
fn load_global_slow(heap: &mut Heap, ic: usize) -> Result<Value, Fail> {
    let name = heap.ics[ic].name;
    let slot = heap
        .global_names
        .iter()
        .position(|&n| n == name)
        .ok_or(Fail::Name)?;
    let e = &mut heap.ics[ic];
    e.key = heap.globals_version;
    e.val = slot as u64;
    Ok(heap.globals[slot])
}

// --------------------------------------------------------------- attributes

/// Instance: `[hdr(kind, shape), class, slot0, ...]`; IC: shape id → slot.
#[inline(always)]
pub fn get_attr(heap: &mut Heap, obj: Value, ic: usize) -> Result<Value, Fail> {
    if obj.is_obj(kind::INST) {
        let a = obj.addr();
        let shape = heap.arena.get(a) >> 8;
        let e = heap.ics[ic];
        if e.key == shape {
            return Ok(Value(heap.arena.get(a + 2 + e.val as u32)));
        }
    }
    attr_slow(heap, obj, ic, None)
}

#[inline(always)]
pub fn set_attr(heap: &mut Heap, obj: Value, ic: usize, v: Value) -> Result<(), Fail> {
    if obj.is_obj(kind::INST) {
        let a = obj.addr();
        let shape = heap.arena.get(a) >> 8;
        let e = heap.ics[ic];
        if e.key == shape {
            heap.arena.set(a + 2 + e.val as u32, v.0);
            return Ok(());
        }
    }
    attr_slow(heap, obj, ic, Some(v)).map(|_| ())
}

/// Shape lookup and IC fill. No shape transitions: the spike's instances are
/// built with all their attributes.
#[cold]
#[inline(never)]
fn attr_slow(heap: &mut Heap, obj: Value, ic: usize, store: Option<Value>) -> Result<Value, Fail> {
    if !obj.is_obj(kind::INST) {
        return Err(Fail::Attr);
    }
    let a = obj.addr();
    let shape = heap.arena.get(a) >> 8;
    let name = heap.ics[ic].name;
    let slot = heap
        .shapes
        .get(shape as usize)
        .and_then(|s| s.iter().position(|&n| n == name))
        .ok_or(Fail::Attr)?;
    let e = &mut heap.ics[ic];
    e.key = shape;
    e.val = slot as u64;
    match store {
        Some(v) => {
            heap.arena.set(a + 2 + slot as u32, v.0);
            Ok(Value::NONE)
        }
        None => Ok(Value(heap.arena.get(a + 2 + slot as u32))),
    }
}

/// `LOAD_METHOD`: unbound function for `obj.name(...)`; IC: class version → function.
#[inline(always)]
pub fn load_method(heap: &mut Heap, obj: Value, ic: usize) -> Result<Value, Fail> {
    let version = if obj.is_obj(kind::INST) {
        let cls = heap.arena.get(obj.addr() + 1) as u32;
        heap.arena.get(cls + 1)
    } else if obj.is_obj(kind::LIST) {
        LIST_TYPE_VERSION
    } else {
        return Err(Fail::Attr);
    };
    let e = heap.ics[ic];
    if e.key == version {
        return Ok(Value(e.val));
    }
    load_method_slow(heap, obj, ic, version)
}

#[cold]
#[inline(never)]
fn load_method_slow(heap: &mut Heap, obj: Value, ic: usize, version: u64) -> Result<Value, Fail> {
    let name = heap.ics[ic].name;
    let f = if obj.is_obj(kind::LIST) {
        if name != names::APPEND {
            return Err(Fail::Attr);
        }
        Value::native(NATIVE_APPEND)
    } else {
        let cls = heap.arena.get(obj.addr() + 1) as u32;
        let n = heap.arena.get(cls + 2) as u32;
        let mut found = None;
        for j in 0..n {
            if heap.arena.get(cls + 3 + 2 * j) == name as u64 {
                found = Some(Value(heap.arena.get(cls + 4 + 2 * j)));
                break;
            }
        }
        found.ok_or(Fail::Attr)?
    };
    let e = &mut heap.ics[ic];
    e.key = version;
    e.val = f.0;
    Ok(f)
}

// ---------------------------------------------------- functions and closures

pub enum Callee {
    Py(usize),
    Native(u32),
    Bad,
}

/// Function: `[hdr(kind, ncells), code index, cell0, ...]`.
#[inline(always)]
pub fn callee(heap: &Heap, f: Value) -> Callee {
    if f.is_obj(kind::FUNC) {
        Callee::Py(heap.arena.get(f.addr() + 1) as usize)
    } else if f.is_native() {
        Callee::Native(f.addr())
    } else {
        Callee::Bad
    }
}

pub fn new_function(heap: &mut Heap, code: usize, cells: &[Value]) -> Result<Value, Fail> {
    let a = heap.arena.alloc(2 + cells.len())?;
    heap.arena.set(a, header(kind::FUNC, cells.len() as u32));
    heap.arena.set(a + 1, code as u64);
    for (j, c) in cells.iter().enumerate() {
        heap.arena.set(a + 2 + j as u32, c.0);
    }
    Ok(Value::obj(kind::FUNC, a))
}

/// Cell: `[hdr, value]`.
pub fn new_cell(heap: &mut Heap, v: Value) -> Result<Value, Fail> {
    let a = heap.arena.alloc(2)?;
    heap.arena.set(a, header(kind::CELL, 0));
    heap.arena.set(a + 1, v.0);
    Ok(Value::obj(kind::CELL, a))
}

/// Free variable `i` of the running function, through its cell.
#[inline(always)]
pub fn load_deref(heap: &Heap, func: Value, i: usize) -> Result<Value, Fail> {
    if func.is_obj(kind::FUNC) {
        let cell = heap.arena.get(func.addr() + 2 + i as u32) as u32;
        let v = Value(heap.arena.get(cell + 1));
        if v != Value::UNDEF {
            return Ok(v);
        }
    }
    Err(Fail::Name)
}

// ------------------------------------------------------------------ natives

pub type Native = fn(&mut Heap, &[Value]) -> Result<Value, Fail>;
pub const NATIVE_RANGE: u32 = 0;
pub const NATIVE_APPEND: u32 = 1;
static NATIVES: [Native; 2] = [native_range, native_append];

#[inline(always)]
pub fn call_native(heap: &mut Heap, i: u32, args: &[Value]) -> Result<Value, Fail> {
    match NATIVES.get(i as usize) {
        Some(f) => f(heap, args),
        None => Err(Fail::Type),
    }
}

/// `range(stop)` returns its iterator directly: `[hdr, cur, stop]`.
fn native_range(heap: &mut Heap, args: &[Value]) -> Result<Value, Fail> {
    match args {
        [stop] if stop.is_int() => {
            let a = heap.arena.alloc(3)?;
            heap.arena.set(a, header(kind::RANGE, 0));
            heap.arena.set(a + 1, 0);
            heap.arena.set(a + 2, stop.int_val() as u64);
            Ok(Value::obj(kind::RANGE, a))
        }
        _ => Err(Fail::Type),
    }
}

fn native_append(heap: &mut Heap, args: &[Value]) -> Result<Value, Fail> {
    match args {
        [lst, v] if lst.is_obj(kind::LIST) => {
            list_append(heap, *lst, *v)?;
            Ok(Value::NONE)
        }
        _ => Err(Fail::Type),
    }
}

/// Range-specialised `FOR_ITER`: next value, or `None` when exhausted.
#[inline(always)]
pub fn range_next(heap: &mut Heap, it: Value) -> Result<Option<Value>, Fail> {
    if it.is_obj(kind::RANGE) {
        let a = it.addr();
        let cur = heap.arena.get(a + 1) as i64;
        if cur < heap.arena.get(a + 2) as i64 {
            heap.arena.set(a + 1, (cur + 1) as u64);
            return Ok(Some(Value::int(cur)));
        }
        return Ok(None);
    }
    Err(Fail::Type)
}

// -------------------------------------------------------------------- lists

/// List: `[hdr, len, cap, buffer]`; buffer: `[hdr, e0, e1, ...]`.
pub fn new_list(heap: &mut Heap) -> Result<Value, Fail> {
    let a = heap.arena.alloc(4)?;
    heap.arena.set(a, header(kind::LIST, 0));
    heap.arena.set(a + 1, 0);
    heap.arena.set(a + 2, 0);
    heap.arena.set(a + 3, 0);
    Ok(Value::obj(kind::LIST, a))
}

/// Append with growth by doubling (old buffers are garbage; there is no GC).
#[inline(never)]
pub fn list_append(heap: &mut Heap, lst: Value, v: Value) -> Result<(), Fail> {
    let a = lst.addr();
    let len = heap.arena.get(a + 1);
    let cap = heap.arena.get(a + 2);
    let mut buf = heap.arena.get(a + 3) as u32;
    if len == cap {
        let new_cap = (cap * 2).max(4);
        let nb = heap.arena.alloc(1 + new_cap as usize)?;
        heap.arena.set(nb, header(kind::BUF, 0));
        for j in 0..len as u32 {
            let e = heap.arena.get(buf + 1 + j);
            heap.arena.set(nb + 1 + j, e);
        }
        heap.arena.set(a + 2, new_cap);
        heap.arena.set(a + 3, nb as u64);
        buf = nb;
    }
    heap.arena.set(buf + 1 + len as u32, v.0);
    heap.arena.set(a + 1, len + 1);
    Ok(())
}

// -------------------------------------------------------------------- dicts

/// Dict: `[hdr, len, cap, table]`; table: `[hdr, k0, v0, k1, v1, ...]`,
/// open addressing with linear probing, SmallInt keys only, empty key = UNDEF.
pub fn new_dict(heap: &mut Heap) -> Result<Value, Fail> {
    let cap = 8;
    let t = new_table(heap, cap)?;
    let a = heap.arena.alloc(4)?;
    heap.arena.set(a, header(kind::DICT, 0));
    heap.arena.set(a + 1, 0);
    heap.arena.set(a + 2, cap);
    heap.arena.set(a + 3, t as u64);
    Ok(Value::obj(kind::DICT, a))
}

fn new_table(heap: &mut Heap, cap: u64) -> Result<u32, Fail> {
    let t = heap.arena.alloc(1 + 2 * cap as usize)?;
    heap.arena.set(t, header(kind::BUF, 0));
    for j in 0..cap as u32 {
        heap.arena.set(t + 1 + 2 * j, Value::UNDEF.0);
    }
    Ok(t)
}

/// Fx-style multiplicative hash (rustc-hash 2 constant and finish rotation).
#[inline(always)]
fn fx(k: i64) -> u64 {
    (k as u64)
        .wrapping_mul(0xf135_7aea_2e62_a9c5)
        .rotate_left(26)
}

/// Address of `key`'s pair in table `t`, or of the empty pair where it would go.
#[inline(always)]
fn probe(heap: &Heap, t: u32, cap: u64, key: Value) -> (u32, bool) {
    let mask = cap - 1;
    let mut i = fx(key.int_val()) & mask;
    loop {
        let p = t + 1 + 2 * i as u32;
        let k = heap.arena.get(p);
        if k == key.0 {
            return (p, true);
        }
        if k == Value::UNDEF.0 {
            return (p, false);
        }
        i = (i + 1) & mask;
    }
}

#[inline(never)]
pub fn dict_get(heap: &mut Heap, d: Value, key: Value) -> Result<Value, Fail> {
    if !key.is_int() {
        return Err(Fail::Type);
    }
    let a = d.addr();
    let (p, found) = probe(
        heap,
        heap.arena.get(a + 3) as u32,
        heap.arena.get(a + 2),
        key,
    );
    if found {
        Ok(Value(heap.arena.get(p + 1)))
    } else {
        Err(Fail::Key)
    }
}

#[inline(never)]
pub fn dict_set(heap: &mut Heap, d: Value, key: Value, v: Value) -> Result<(), Fail> {
    if !key.is_int() {
        return Err(Fail::Type);
    }
    let a = d.addr();
    let len = heap.arena.get(a + 1);
    let mut cap = heap.arena.get(a + 2);
    let mut t = heap.arena.get(a + 3) as u32;
    let (p, found) = probe(heap, t, cap, key);
    if found {
        heap.arena.set(p + 1, v.0);
        return Ok(());
    }
    let mut p = p;
    if (len + 1) * 3 > cap * 2 {
        // Resize at 2/3 load: double and reinsert.
        let new_cap = cap * 2;
        let nt = new_table(heap, new_cap)?;
        for j in 0..cap as u32 {
            let k = Value(heap.arena.get(t + 1 + 2 * j));
            if k != Value::UNDEF {
                let (q, _) = probe(heap, nt, new_cap, k);
                heap.arena.set(q, k.0);
                let val = heap.arena.get(t + 2 + 2 * j);
                heap.arena.set(q + 1, val);
            }
        }
        cap = new_cap;
        t = nt;
        heap.arena.set(a + 2, cap);
        heap.arena.set(a + 3, t as u64);
        p = probe(heap, t, cap, key).0;
    }
    heap.arena.set(p, key.0);
    heap.arena.set(p + 1, v.0);
    heap.arena.set(a + 1, len + 1);
    Ok(())
}

// --------------------------------------------------------------- subscripts

#[inline(always)]
pub fn getitem(heap: &mut Heap, obj: Value, key: Value) -> Result<Value, Fail> {
    if obj.is_obj(kind::LIST) & key.is_int() {
        let a = obj.addr();
        let i = key.int_val();
        if (i as u64) < heap.arena.get(a + 1) {
            let buf = heap.arena.get(a + 3) as u32;
            return Ok(Value(heap.arena.get(buf + 1 + i as u32)));
        }
    } else if obj.is_obj(kind::DICT) {
        return dict_get(heap, obj, key);
    }
    getitem_slow(heap, obj, key)
}

/// Negative indices and errors.
#[cold]
#[inline(never)]
fn getitem_slow(heap: &mut Heap, obj: Value, key: Value) -> Result<Value, Fail> {
    if obj.is_obj(kind::LIST) && key.is_int() {
        let a = obj.addr();
        let len = heap.arena.get(a + 1) as i64;
        let i = key.int_val() + len;
        if (0..len).contains(&i) {
            let buf = heap.arena.get(a + 3) as u32;
            return Ok(Value(heap.arena.get(buf + 1 + i as u32)));
        }
        return Err(Fail::Index);
    }
    Err(Fail::Type)
}

#[inline(always)]
pub fn setitem(heap: &mut Heap, obj: Value, key: Value, v: Value) -> Result<(), Fail> {
    if obj.is_obj(kind::DICT) {
        return dict_set(heap, obj, key, v);
    } else if obj.is_obj(kind::LIST) & key.is_int() {
        let a = obj.addr();
        let i = key.int_val();
        if (i as u64) < heap.arena.get(a + 1) {
            let buf = heap.arena.get(a + 3) as u32;
            heap.arena.set(buf + 1 + i as u32, v.0);
            return Ok(());
        }
    }
    setitem_slow()
}

#[cold]
#[inline(never)]
fn setitem_slow() -> Result<(), Fail> {
    Err(Fail::Type)
}
