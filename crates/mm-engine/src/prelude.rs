//! The prelude's lazy units (docs/spec.md §12). `build.rs` splits
//! `prelude.py` into units, packed in chunks: top-level functions, classes
//! of methods and literals, instances of those classes and aliases. A
//! session loads a unit on the first lookup of its name and builds its
//! value in Rust; no Python runs. A session starts with only the prelude's
//! natives, `_MISSING` and `__name__` bound.

use crate::types::T;
use crate::types::ty;
use crate::vm::{GSLOT_PRELUDE, ModuleCache, PreludeChunk, Raised, Vm};
use alloc::{sync::Arc, vec::Vec};
use mm_compile::bytecode::Program;
use mm_compile::section::COUNT;
use mm_compile::{DecodeError, SharedStrings};
use mm_core::Value;

/// Unit kinds.
const FUNC: u16 = 0;
const CLASS: u16 = 1;
const INST: u16 = 2;
const ALIAS: u16 = 3;
const VALUE: u16 = 4;

include!(concat!(env!("OUT_DIR"), "/prelude_units.rs"));

/// Prelude functions that are builtins: put in the builtins on their first
/// lookup.
pub(crate) const AS_BUILTINS: [&str; 8] = [
    "__import__",
    "map",
    "filter",
    "open",
    "help",
    "globals",
    "BaseExceptionGroup",
    "ExceptionGroup",
];

/// How many chunks the prelude's units are packed in.
pub const PRELUDE_CHUNKS: usize = CHUNK_END.len();

/// Parts per chunk: the nine program sections of its units, then their
/// records.
pub(crate) const PARTS: usize = COUNT + 1;

/// The part holding a chunk's unit records.
const RECORDS: usize = PARTS - 1;

/// The packed prelude's hash, for the build fingerprint: the same with
/// and without `host-bundle`.
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn fingerprint() -> u64 {
    PRELUDE_HASH
}

/// The parts of chunk `chunk`, unpacked within `budget`; with
/// `host-bundle`, its packed bytes come from `cache`'s bundle.
#[cfg_attr(mm_shipped, optimize(size))]
fn unpack_parts(
    chunk: usize,
    cache: Option<&dyn ModuleCache>,
    budget: &mut u64,
) -> Result<[Vec<u8>; PARTS], DecodeError> {
    let end = *CHUNK_END.get(chunk).ok_or(DecodeError::Invalid)? as usize;
    let start = match chunk {
        0 => HEADER,
        c => CHUNK_END[c - 1] as usize,
    };
    // With `host-bundle`, the host's bundle region 0: the chunks after
    // the headers. Default builds have no such path at all.
    #[cfg(feature = "host-bundle")]
    let fetched = {
        let mut b = alloc::vec![0; end.saturating_sub(start)];
        let at = u32::try_from(start - HEADER).map_err(|_| DecodeError::Invalid)?;
        if !cache.is_some_and(|c| c.bundle(0, at, &mut b)) {
            return Err(DecodeError::Invalid);
        }
        b
    };
    #[cfg(feature = "host-bundle")]
    let packed = fetched.as_slice();
    #[cfg(not(feature = "host-bundle"))]
    let packed = {
        let _ = cache;
        CHUNKS.get(start..end).ok_or(DecodeError::Invalid)?
    };
    let mut r = Rd(packed);
    let mut lens = [0usize; PARTS];
    for len in &mut lens[..PARTS - 1] {
        *len = r.len().ok_or(DecodeError::Invalid)?;
    }
    let mut parts: [Vec<u8>; PARTS] = Default::default();
    for (k, len) in lens.into_iter().enumerate() {
        // The last part is the rest.
        let len = if k == PARTS - 1 { r.0.len() } else { len };
        let part = r.take(len).ok_or(DecodeError::Invalid)?;
        let start = if k == 0 { 0 } else { HEADER_END[k - 1] };
        let header = CHUNKS
            .get(start..HEADER_END[k])
            .ok_or(DecodeError::Invalid)?;
        parts[k] = mm_compile::pack::unpack_shared_bounded(header, part, budget)?;
        // One part's decoders at a time: all but the last give their
        // workspace back, so the chunk peaks at its parts plus one.
        if k < PARTS - 1 {
            *budget = budget.saturating_add(mm_compile::pack::workspace());
        }
    }
    Ok(parts)
}

/// The parts of `c`.
#[cfg_attr(mm_shipped, optimize(size))]
fn parts_of(c: &PreludeChunk) -> [&[u8]; PARTS] {
    core::array::from_fn(|k| c.parts[k].as_slice())
}

/// The program sections and the rest of the record of the unit whose
/// record is at `at`, `len` long, in `parts`' records.
#[cfg_attr(mm_shipped, optimize(size))]
fn locate<'a>(
    parts: &[&'a [u8]; PARTS],
    at: usize,
    len: usize,
) -> Option<([&'a [u8]; COUNT], Rd<'a>)> {
    let records = parts[RECORDS];
    // The units before it in the chunk: where its sections start.
    let mut offsets = [0usize; COUNT];
    let mut r = Rd(records.get(..at)?);
    while !r.0.is_empty() {
        let n = r.len()?;
        let mut rec = Rd(r.take(n)?);
        for o in &mut offsets {
            *o = o.checked_add(rec.len()?)?;
        }
    }
    let mut r = Rd(records.get(at..at.checked_add(len)?)?);
    let _ = r.len()?;
    let mut sections: [&[u8]; COUNT] = [&[]; COUNT];
    for (k, s) in sections.iter_mut().enumerate() {
        let n = r.len()?;
        *s = parts[k].get(offsets[k]..offsets[k].checked_add(n)?)?;
    }
    Some((sections, r))
}

/// Every function and class unit's program (for tests); `None` if the
/// build's encoding is corrupt.
#[must_use]
#[cfg_attr(mm_shipped, optimize(size))]
pub fn prelude_programs() -> Option<Vec<Program>> {
    let mut out = Vec::new();
    for c in 0..CHUNK_END.len() {
        let owned = unpack_parts(c, None, &mut { u64::MAX }).ok()?;
        let parts: [&[u8]; PARTS] = core::array::from_fn(|k| owned[k].as_slice());
        for u in UNITS
            .iter()
            .filter(|u| matches!(u[4], FUNC | CLASS) && usize::from(u[1]) == c)
        {
            let (sections, _) = locate(&parts, usize::from(u[2]), usize::from(u[3]))?;
            let shared = SharedStrings::new(TEXT, ENDS);
            out.push(Program::decode_sections(&sections, shared, &mut { u64::MAX }).ok()?);
        }
    }
    Some(out)
}

/// The name of unit `i`.
#[cfg_attr(mm_shipped, optimize(size))]
fn name_of(i: usize) -> &'static str {
    let start = if i == 0 { 0 } else { UNITS[i - 1][0] as usize };
    NAMES.get(start..UNITS[i][0] as usize).unwrap_or("")
}

/// The unit named `name`.
#[cfg_attr(mm_shipped, optimize(size))]
fn unit(name: &str) -> Option<&'static [u16; 5]> {
    let (mut lo, mut hi) = (0, UNITS.len());
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        match name_of(mid).cmp(name) {
            core::cmp::Ordering::Less => lo = mid + 1,
            core::cmp::Ordering::Greater => hi = mid,
            core::cmp::Ordering::Equal => return Some(&UNITS[mid]),
        }
    }
    None
}

/// Whether the prelude has a unit named `name`.
#[cfg(feature = "asyncio")]
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn has_unit(name: &str) -> bool {
    unit(name).is_some()
}

/// A unit record being read (`build.rs`'s `record`); `None` if it is
/// malformed.
struct Rd<'a>(&'a [u8]);

impl<'a> Rd<'a> {
    #[cfg_attr(mm_shipped, optimize(size))]
    fn byte(&mut self) -> Option<u8> {
        let (&b, rest) = self.0.split_first()?;
        self.0 = rest;
        Some(b)
    }

    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let (b, rest) = self.0.split_at_checked(n)?;
        self.0 = rest;
        Some(b)
    }

    #[inline(never)]
    #[cfg_attr(mm_shipped, optimize(size))]
    fn uleb(&mut self) -> Option<u64> {
        let mut v = 0u64;
        for shift in (0..64).step_by(7) {
            let b = self.byte()?;
            v |= u64::from(b & 0x7f) << shift;
            if b & 0x80 == 0 {
                return Some(v);
            }
        }
        None
    }

    fn len(&mut self) -> Option<usize> {
        usize::try_from(self.uleb()?).ok()
    }

    fn str(&mut self) -> Option<&'a str> {
        let n = self.len()?;
        core::str::from_utf8(self.take(n)?).ok()
    }
}

impl Vm {
    /// Binds what the prelude's globals start with: its natives, `_MISSING`
    /// and `__name__`.
    pub(crate) fn init_prelude(&mut self) -> Result<(), Raised> {
        let ns = self.prelude_globals;
        let k = self.intern("_MISSING")?;
        let m = self.missing;
        self.dict_set(ns, k, m)?;
        for (i, n) in crate::builtins::NATIVES.iter().enumerate() {
            if n.module() == Some("_prelude") {
                let k = self.intern(n.name())?;
                let f = Value::native(u32::try_from(i).unwrap_or(0));
                self.dict_set(ns, k, f)?;
            }
        }
        let k = self.intern("__name__")?;
        let v = self.intern(PRELUDE_NAME)?;
        self.dict_set(ns, k, v)
    }

    /// Prelude global `name` (interned): bound, or built from its unit.
    pub(crate) fn prelude_get(&mut self, name: Value) -> Result<Option<Value>, Raised> {
        match self.dict_get_str(self.prelude_globals, name) {
            Some(v) => Ok(Some(v)),
            None => self.prelude_unit(name),
        }
    }

    /// Builtin `name` (interned), putting a prelude function that is a
    /// builtin there on its first lookup.
    pub(crate) fn builtin_get(&mut self, name: Value) -> Result<Option<Value>, Raised> {
        if let Some(v) = self.dict_get(self.builtins, name)? {
            return Ok(Some(v));
        }
        if !AS_BUILTINS.contains(&self.str_ref_o(name)) {
            return Ok(None);
        }
        let Some(f) = self.prelude_get(name)? else {
            return Ok(None);
        };
        let b = self.builtins;
        self.dict_set(b, name, f)?;
        Ok(Some(f))
    }

    /// Builds the unit named `name` (interned, not yet bound) and binds it
    /// in the prelude globals; `None` if no unit has that name.
    pub(crate) fn prelude_unit(&mut self, name: Value) -> Result<Option<Value>, Raised> {
        let Some(&[_, chunk, at, len, kind]) = unit(self.str_ref_o(name)) else {
            // The asyncio types its prelude part names (docs/spec.md §8.5).
            let t = match self.str_ref_o(name) {
                "CancelledError" => T::CancelledError,
                "InvalidStateError" => T::InvalidStateError,
                "Task" => T::Task,
                "_AioFuture" => T::Future,
                "coroutine" => T::coroutine,
                _ => return Ok(None),
            };
            let v = self.types[t as usize];
            let g = self.prelude_globals;
            self.dict_set(g, name, v)?;
            return Ok(Some(v));
        };
        let v = if kind == ALIAS {
            let other = self.intern(name_of(chunk as usize))?;
            match self.prelude_get(other)? {
                Some(v) => v,
                None => return Ok(None),
            }
        } else {
            let c = self.unpack_chunk(chunk)?;
            let built = match locate(&parts_of(&c), at.into(), len.into()) {
                Some((sections, mut r)) => self.build_unit(name, kind, &sections, &mut r)?,
                None => None,
            };
            match built {
                Some(v) => v,
                None => return Err(self.err(T::SystemError, "micro-monty: the prelude is corrupt")),
            }
        };
        let g = self.prelude_globals;
        self.dict_set(g, name, v)?;
        Ok(Some(v))
    }

    /// Prelude globals `names`, as `prelude_get` of each in turn returns
    /// them and in the same order, but unpacking each chunk once: the
    /// records of the units still to build are copied out chunk by chunk,
    /// within the same bounded workspace, then built in `names`' order.
    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn prelude_get_all(&mut self, names: &[&str]) -> Result<Vec<Option<Value>>, Raised> {
        let mut keys = Vec::with_capacity(names.len());
        let mut todo = Vec::new();
        for (i, n) in names.iter().enumerate() {
            let k = self.intern(n)?;
            keys.push(k);
            if self.dict_get_str(self.prelude_globals, k).is_none()
                && let Some(&[_, chunk, at, len, kind]) = unit(n)
                && kind != ALIAS
            {
                todo.push((chunk, at, len, i));
            }
        }
        // By chunk (an insertion sort: a few dozen names, and no new sort
        // instantiation in the binary).
        for i in 1..todo.len() {
            let mut k = i;
            while k > 0 && todo[k - 1] > todo[k] {
                todo.swap(k - 1, k);
                k -= 1;
            }
        }
        // Each unit copied out as its sections, then its record, with
        // where each part ends.
        let mut records: Vec<Option<(Vec<u8>, [usize; PARTS])>> = alloc::vec![None; names.len()];
        let mut copied = 0u64;
        let mut held: Option<(u16, Arc<PreludeChunk>)> = None;
        for &(chunk, at, len, i) in &todo {
            let c = match held.take() {
                Some((n, c)) if n == chunk => c,
                _ => self.unpack_chunk_with(chunk, copied)?,
            };
            let parts = parts_of(&c);
            let (at, len) = (usize::from(at), usize::from(len));
            if let Some((sections, _)) = locate(&parts, at, len)
                && let Some(record) = parts[RECORDS].get(at..at + len)
            {
                let mut flat = Vec::new();
                let mut ends = [0; PARTS];
                for (s, end) in sections.iter().chain([&record]).zip(&mut ends) {
                    flat.extend_from_slice(s);
                    *end = flat.len();
                }
                copied += flat.len() as u64;
                records[i] = Some((flat, ends));
            }
            held = Some((chunk, c));
        }
        drop(held);
        let mut out = Vec::with_capacity(names.len());
        for (i, &k) in keys.iter().enumerate() {
            if let Some(v) = self.dict_get_str(self.prelude_globals, k) {
                out.push(Some(v));
                continue;
            }
            let Some(rec) = records[i].take() else {
                out.push(self.prelude_unit(k)?);
                continue;
            };
            let kind = unit(names[i]).map_or(VALUE, |u| u[4]);
            let (flat, ends) = &rec;
            let parts: [&[u8]; PARTS] = core::array::from_fn(|k| {
                let start = if k == 0 { 0 } else { ends[k - 1] };
                flat.get(start..ends[k]).unwrap_or(&[])
            });
            let built = match locate(&parts, 0, parts[RECORDS].len()) {
                Some((sections, mut r)) => self.build_unit(k, kind, &sections, &mut r)?,
                None => None,
            };
            let Some(v) = built else {
                return Err(self.err(T::SystemError, "micro-monty: the prelude is corrupt"));
            };
            let g = self.prelude_globals;
            self.dict_set(g, k, v)?;
            out.push(Some(v));
        }
        Ok(out)
    }

    /// Chunk `chunk`, unpacked within the same bounded workspace as a
    /// bundled module's decode.
    #[cfg_attr(mm_shipped, optimize(size))]
    fn unpack_chunk(&mut self, chunk: u16) -> Result<Arc<PreludeChunk>, Raised> {
        self.unpack_chunk_with(chunk, 0)
    }

    /// [`Self::unpack_chunk`], with `extra` more bytes of workspace held
    /// beside it. A chunk from the engine's cache is charged the workspace
    /// its unpacking took, so the session behaves the same either way.
    #[cfg_attr(mm_shipped, optimize(size))]
    fn unpack_chunk_with(&mut self, chunk: u16, extra: u64) -> Result<Arc<PreludeChunk>, Raised> {
        let allowance = self.heap.available_bytes().saturating_add(self.std_scratch);
        let Some(mut remaining) = allowance.checked_sub(extra) else {
            return Err(self.err(T::MemoryError, ""));
        };
        let cache = self.module_cache.clone();
        let c = if let Some(c) = cache.as_deref().and_then(|c| c.prelude_chunk(chunk.into())) {
            let Some(left) = remaining.checked_sub(c.workspace) else {
                return Err(self.err(T::MemoryError, ""));
            };
            remaining = left;
            c
        } else {
            let before = remaining;
            let parts = unpack_parts(chunk.into(), cache.as_deref(), &mut remaining)
                .map_err(|e| self.decode_error(e))?;
            let c = Arc::new(PreludeChunk {
                parts,
                workspace: before - remaining,
            });
            if let Some(cache) = cache {
                cache.offer_prelude_chunk(chunk.into(), &c);
            }
            c
        };
        self.std_scratch = self.std_scratch.max(allowance - remaining);
        Ok(c)
    }

    #[cfg_attr(mm_shipped, optimize(size))]
    fn decode_error(&mut self, e: DecodeError) -> Raised {
        match e {
            DecodeError::TooLarge => self.err(T::MemoryError, ""),
            DecodeError::Invalid => self.err(T::SystemError, "micro-monty: the prelude is corrupt"),
        }
    }

    /// The value of the unit record `r` of kind `kind`; `None` if the record
    /// is malformed.
    #[cfg_attr(mm_shipped, optimize(size))]
    fn build_unit(
        &mut self,
        name: Value,
        kind: u16,
        sections: &[&[u8]; COUNT],
        r: &mut Rd<'_>,
    ) -> Result<Option<Value>, Raised> {
        let first = if sections.iter().all(|s| s.is_empty()) {
            0
        } else {
            let allowance = self.heap.available_bytes().saturating_add(self.std_scratch);
            let mut remaining = allowance;
            let shared = SharedStrings::new(TEXT, ENDS);
            let prog = Program::decode_sections(sections, shared, &mut remaining)
                .map_err(|e| self.decode_error(e))?;
            let old_scratch = self.std_scratch;
            self.std_scratch = old_scratch.max(allowance - remaining);
            let first = u32::try_from(self.codes.len()).unwrap_or(u32::MAX);
            if let Err(e) = self.load_in(&prog, "<prelude>", "", GSLOT_PRELUDE) {
                self.std_scratch = old_scratch;
                self.reserve_code(0)?;
                return Err(e);
            }
            first
        };
        match kind {
            FUNC => self.build_unit_func(first, r),
            CLASS => self.build_unit_class(name, first, r),
            INST => {
                let Some(class) = r.str() else {
                    return Ok(None);
                };
                let c = self.intern(class)?;
                let Some(c) = self.prelude_get(c)? else {
                    return Ok(None);
                };
                let o = self.alloc_of_class(c, &[])?;
                if !self.set_attrs(o, r)? {
                    return Ok(None);
                }
                Ok(Some(o))
            }
            VALUE => self.read_literal(r),
            _ => Ok(None),
        }
    }

    /// A function of code `first` + its record's index, with its defaults.
    #[cfg_attr(mm_shipped, optimize(size))]
    fn build_unit_func(&mut self, first: u32, r: &mut Rd<'_>) -> Result<Option<Value>, Raised> {
        let Some(ci) = r.len().and_then(|i| u32::try_from(i).ok()) else {
            return Ok(None);
        };
        let Some(n) = r.len() else { return Ok(None) };
        let defaults = if n == 0 {
            Value::NONE
        } else {
            let mut items = Vec::with_capacity(n.min(64));
            for _ in 0..n {
                let Some(v) = self.read_literal(r)? else {
                    return Ok(None);
                };
                items.push(v);
            }
            self.alloc_tuple(&items)?
        };
        let Some(n) = r.len() else { return Ok(None) };
        let kwdefaults = if n == 0 {
            Value::NONE
        } else {
            let m = self.new_dict()?;
            for _ in 0..n {
                let Some(k) = r.str() else { return Ok(None) };
                let k = self.intern(k)?;
                let Some(v) = self.read_literal(r)? else {
                    return Ok(None);
                };
                self.dict_set(m, k, v)?;
            }
            m
        };
        self.new_function(first + ci, defaults, kwdefaults, Value::NONE)
            .map(Some)
    }

    /// The class its body would have built: `new_class` over the same
    /// namespace (`__module__`, `__qualname__`, then the body's entries),
    /// then the attributes the prelude sets on it.
    #[cfg_attr(mm_shipped, optimize(size))]
    fn build_unit_class(
        &mut self,
        name: Value,
        first: u32,
        r: &mut Rd<'_>,
    ) -> Result<Option<Value>, Raised> {
        let Some(n) = r.len() else { return Ok(None) };
        let mut bases = Vec::with_capacity(n.min(8));
        for _ in 0..n {
            let Some(b) = r.str() else { return Ok(None) };
            let k = self.intern(b)?;
            let v = match self.prelude_get(k)? {
                Some(v) => Some(v),
                None => self.builtin_get(k)?,
            };
            let Some(v) = v else { return Ok(None) };
            bases.push(v);
        }
        let Some(module) = r.str() else {
            return Ok(None);
        };
        let ns = self.new_dict()?;
        let k = self.intern("__module__")?;
        let m = self.intern(module)?;
        self.dict_set(ns, k, m)?;
        let k = self.intern("__qualname__")?;
        self.dict_set(ns, k, name)?;
        let Some(n) = r.len() else { return Ok(None) };
        for _ in 0..n {
            let Some(k) = r.str() else { return Ok(None) };
            let k = self.intern(k)?;
            let v = match r.byte() {
                Some(tag @ (0 | 1)) => {
                    let Some(i) = r.len().and_then(|i| u32::try_from(i).ok()) else {
                        return Ok(None);
                    };
                    let f = self.new_function(first + i, Value::NONE, Value::NONE, Value::NONE)?;
                    if tag == 0 {
                        f
                    } else {
                        // `property(f)`, as the decorator makes it.
                        let dk = self.intern("__doc__")?;
                        let doc = self.getattr(f, dk)?;
                        let p = {
                            let res = self.heap.alloc_init(
                                ty::PROPERTY,
                                0,
                                &[f, Value::NONE, Value::NONE, doc],
                                &[],
                            );
                            self.hx_o(res)
                        }?;
                        Value::from_ref(p, ty::PROPERTY as u8)
                    }
                }
                Some(2) => match self.read_literal(r)? {
                    Some(v) => v,
                    None => return Ok(None),
                },
                _ => return Ok(None),
            };
            self.dict_set(ns, k, v)?;
        }
        let c = self.new_class(name, &bases, ns)?;
        if !self.set_attrs(c, r)? {
            return Ok(None);
        }
        Ok(Some(c))
    }

    /// Sets the counted `(name, literal)` attributes of record `r` on `o`;
    /// `false` if the record is malformed.
    #[cfg_attr(mm_shipped, optimize(size))]
    fn set_attrs(&mut self, o: Value, r: &mut Rd<'_>) -> Result<bool, Raised> {
        let Some(n) = r.len() else { return Ok(false) };
        for _ in 0..n {
            let Some(k) = r.str() else { return Ok(false) };
            let k = self.intern(k)?;
            let Some(v) = self.read_literal(r)? else {
                return Ok(false);
            };
            self.setattr(o, k, v)?;
        }
        Ok(true)
    }

    /// A literal: a scalar, or a tuple (tag 6), dict (7) or list (8) of
    /// scalars.
    #[cfg_attr(mm_shipped, optimize(size))]
    fn read_literal(&mut self, r: &mut Rd<'_>) -> Result<Option<Value>, Raised> {
        let tag = r.0.first().copied();
        if !matches!(tag, Some(6..=8)) {
            return self.read_scalar(r);
        }
        r.byte();
        let Some(n) = r.len() else { return Ok(None) };
        let per = if tag == Some(7) { 2 } else { 1 };
        let mut items = Vec::with_capacity(n.min(64) * per);
        for _ in 0..n * per {
            let Some(v) = self.read_scalar(r)? else {
                return Ok(None);
            };
            items.push(v);
        }
        Ok(Some(match tag {
            Some(6) => self.alloc_tuple(&items)?,
            Some(8) => self.new_list(&items)?,
            _ => {
                let d = self.new_dict()?;
                for &[k, v] in items.as_chunks::<2>().0 {
                    self.dict_set(d, k, v)?;
                }
                d
            }
        }))
    }

    /// A scalar: tag 0 `None`, 1 `False`, 2 `True`, 3 int (zigzag), 4 float,
    /// 5 str, 9 a prelude or builtin name's value, 10 a builtin type.
    #[cfg_attr(mm_shipped, optimize(size))]
    fn read_scalar(&mut self, r: &mut Rd<'_>) -> Result<Option<Value>, Raised> {
        Ok(Some(match r.byte() {
            Some(0) => Value::NONE,
            Some(t @ (1 | 2)) => Value::from_bool(t == 2),
            Some(3) => {
                let Some(z) = r.uleb() else { return Ok(None) };
                #[allow(clippy::cast_possible_wrap, reason = "zigzag decoding")]
                let i = (z >> 1) as i64 ^ -((z & 1) as i64);
                self.new_int(i)?
            }
            Some(4) => {
                let Some(b) = r.take(8).and_then(|b| <[u8; 8]>::try_from(b).ok()) else {
                    return Ok(None);
                };
                Value::float(f64::from_le_bytes(b))
            }
            Some(5) => match r.str() {
                Some(s) => self.intern(s)?,
                None => return Ok(None),
            },
            Some(9) => {
                let Some(s) = r.str() else { return Ok(None) };
                let k = self.intern(s)?;
                match self.prelude_get(k)? {
                    Some(v) => v,
                    None => match self.builtin_get(k)? {
                        Some(v) => v,
                        None => return Ok(None),
                    },
                }
            }
            Some(10) => {
                let Some(s) = r.str() else { return Ok(None) };
                match crate::types::ALL.iter().find(|t| t.name() == s) {
                    Some(&t) => self.type_obj(t),
                    None => return Ok(None),
                }
            }
            _ => return Ok(None),
        }))
    }
}
