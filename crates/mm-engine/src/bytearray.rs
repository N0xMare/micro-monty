//! `bytearray`: the mutable byte type (docs/spec.md §7.4). Its read-only
//! methods are `bytes`' (`bytes.rs`), found through the method table's
//! fallback; this file holds the representation and the mutators.
//!
//! ```text
//! BYTEARRAY  value slot 0: ByteRef of the capacity block
//!            raw slot 1:   the length (at most the block's length)
//! ```

use crate::bytes::{Job, R_APPEND, W_A, W_END, W_I, job};
use crate::types::{T, ty};
use crate::vm::{CallArgs, Raised, Vm};
use alloc::format;
use mm_core::{ByteRef, Value};

/// Capacities above this many bytes shrink when the length falls under a
/// quarter of them.
const SHRINK_ABOVE: usize = 64;

/// The operation a [`job::GROW`] job does once the bytearray holds its new
/// block (raw word `W_A`; its arguments are `W_A + 1`, `W_A + 2` and value
/// slot 0).
pub(crate) mod fin {
    /// `append(x)`, `insert(at, x)`.
    pub const APPEND: u64 = 0;
    pub const INSERT: u64 = 1;
    /// `+= src`, `extend(src)`, `[lo:hi] = src`.
    pub const IADD: u64 = 2;
    pub const EXTEND: u64 = 3;
    pub const SPLICE: u64 = 4;
    /// `extend(list_or_tuple)`: becomes the `job::INTS` job.
    pub const INTS: u64 = 5;
}

/// The block a bytearray of capacity `cap` grows into to hold `n` bytes:
/// CPython's over-allocation for appends, at most to the largest block.
#[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
fn grown_block(cap: usize, n: usize) -> usize {
    if n <= cap.saturating_add(cap / 8) {
        let max = usize::try_from(mm_core::MAX_OBJECT_BYTES).unwrap_or(usize::MAX);
        n.saturating_add(n / 8 + if n < 9 { 3 } else { 6 })
            .min(max.max(n))
    } else {
        n
    }
}

impl Vm {
    /// Is `v` a `bytearray`?
    #[inline]
    #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
    pub(crate) fn is_bytearray(v: Value) -> bool {
        v.is_obj_kind(ty::BYTEARRAY as u8)
    }

    /// A new `bytearray` holding a copy of `data`, with no spare capacity.
    #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
    pub(crate) fn new_bytearray(&mut self, data: &[u8]) -> Result<Value, Raised> {
        if data.len() >= 64 {
            self.burn_result(data.len(), true);
        }
        let b = {
            let res = self.heap.alloc_bytes(data);
            self.hx_o(res)
        }?;
        let r = {
            let res = self.heap.alloc_init(
                ty::BYTEARRAY,
                0,
                &[Value::from_byte_ref(b)],
                &[data.len() as u64],
            );
            self.hx_o(res)
        }?;
        Ok(Value::from_ref(r, ty::BYTEARRAY as u8))
    }

    /// The capacity block of bytearray `v`.
    #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
    fn ba_block(&self, v: Value) -> Option<ByteRef> {
        self.vslot_o(v, 0).as_byte_ref()
    }

    /// The live bytes of bytearray `v`. A length past the block (only in a
    /// corrupt snapshot) reads as the whole block.
    #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
    pub(crate) fn bytearray_data(&self, v: Value) -> &[u8] {
        let Some(r) = v.as_obj() else { return &[] };
        let n = usize::try_from(self.raw_o(r, 1)).unwrap_or(usize::MAX);
        let data = self
            .slot(r, 0)
            .as_byte_ref()
            .and_then(|b| self.heap.bytes(b).ok())
            .unwrap_or(&[]);
        data.get(..n).unwrap_or(data)
    }

    /// Sets `v`'s length to `n`, keeping its first `min(len, n)` bytes;
    /// bytes past the old length are zero. A growth with `at` below the
    /// length opens its gap there instead: the bytes from `at` on move to
    /// the end (into a new block directly), and the caller fills the gap.
    /// Growth that does not fit raises `MemoryError` before anything changes.
    #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
    pub(crate) fn ba_resize(&mut self, v: Value, n: usize, at: usize) -> Result<(), Raised> {
        let (Some(r), Some(b)) = (v.as_obj(), self.ba_block(v)) else {
            return Ok(());
        };
        let old = self.bytearray_data(v).len();
        // A growth's gap: the bytes from `at` on move to the end.
        let (at, gap) = (at.min(old), n.saturating_sub(old));
        let cap = self.heap.bytes(b).map_or(0, <[u8]>::len);
        let shrink = cap > SHRINK_ABOVE && n < cap / 4;
        if n <= cap && !shrink {
            if n > old
                && let Ok(d) = self.heap.bytes_mut(b)
                && let Some(t) = d.get_mut(old..n)
            {
                t.fill(0);
            }
            if at < old {
                let res = self.heap.copy_bytes(b, at, b, at + gap, old - at);
                self.hx_o(res)?;
            }
            self.set_raw_o(r, 1, n as u64);
            return Ok(());
        }
        let alloc = if shrink { n } else { grown_block(cap, n) };
        if alloc >= 64 {
            self.burn_result(alloc, true);
        }
        let nb = match self.heap.alloc_bytes_zeroed(alloc) {
            Ok(nb) => nb,
            // A failed shrink keeps the larger block.
            Err(_) if shrink => {
                self.set_raw_o(r, 1, n as u64);
                return Ok(());
            }
            Err(e) => return self.hx_o(Err(e)),
        };
        let res = self.heap.copy_bytes(b, 0, nb, 0, at.min(n));
        self.hx_o(res)?;
        if at < old.min(n) {
            let res = self.heap.copy_bytes(b, at, nb, at + gap, old - at);
            self.hx_o(res)?;
        }
        self.set_slot_o(r, 0, Value::from_byte_ref(nb));
        self.set_raw_o(r, 1, n as u64);
        Ok(())
    }

    /// Does growing bytearray `v` to `need` bytes move more than a step's
    /// copy into a new block?
    #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
    pub(crate) fn ba_grows_far(&self, v: Value, need: usize) -> bool {
        let cap = self
            .ba_block(v)
            .and_then(|b| self.heap.bytes(b).ok())
            .map_or(0, <[u8]>::len);
        need > cap && self.bytearray_data(v).len() as u64 >= crate::nstep::STEP_WORK * 64
    }

    /// Grows bytearray `v` (which [`Vm::ba_grows_far`] to `need` bytes) in
    /// a [`job::GROW`] job: its bytes copied into the new block a step at a
    /// time, then the operation `f` ([`fin`]; arguments `w`, `operand`)
    /// done in it.
    #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
    pub(crate) fn ba_grow(
        &mut self,
        v: Value,
        need: usize,
        f: u64,
        w: [u64; 2],
        operand: Value,
    ) -> Result<Value, Raised> {
        let cap = self
            .ba_block(v)
            .and_then(|b| self.heap.bytes(b).ok())
            .map_or(0, <[u8]>::len);
        let nb = self.job_block(grown_block(cap, need))?;
        let old = self.bytearray_data(v).len();
        let mut j = Job::new(job::GROW, operand, 0, old, 0);
        (j.v[2], j.v[3]) = (Value::from_byte_ref(nb), v);
        (j.w[W_A], j.w[W_A + 1], j.w[W_A + 2]) = (f, w[0], w[1]);
        self.job_run(j)
    }

    /// A [`job::GROW`] job: bytes `W_I..W_END` of the bytearray (value slot
    /// 3) into the block in slot 2, which it then holds, and its operation.
    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn ba_grow_step(
        &mut self,
        j: &mut Job,
        start: u64,
    ) -> Result<Option<Value>, Raised> {
        let (v, op) = (j.v[3], j.v[0]);
        let (Some(r), Some(nb)) = (v.as_obj(), j.v[2].as_byte_ref()) else {
            return Err(self.bad_step());
        };
        let (old, a, b) = (j.at(W_END), j.at(W_A + 1), j.at(W_A + 2));
        let mut o = j.at(W_I);
        while o < old {
            let room = self.job_room(start);
            if room == 0 {
                j.w[W_I] = o as u64;
                return Ok(None);
            }
            self.copy_part(v, o, old, nb, &mut o, room)?;
        }
        self.set_slot_o(r, 0, j.v[2]);
        match j.w[W_A] {
            fin::APPEND | fin::INSERT => {
                let at = if j.w[W_A] == fin::APPEND { old } else { a };
                self.ba_resize(v, old + 1, at)?;
                self.burn(old as u64 / 64);
                self.ba_put(v, at, if j.w[W_A] == fin::APPEND { a } else { b } as u8);
                Ok(Some(Value::NONE))
            }
            fin::IADD => self.bytearray_iadd(v, op).map(Some),
            fin::EXTEND => self.bytearray_iadd(v, op).map(|_| Some(Value::NONE)),
            fin::SPLICE => self
                .ba_splice(v, a, b, Some(op))
                .map(|()| Some(Value::NONE)),
            fin::INTS => {
                // The list's bytes are filled from the next step on.
                *j = self.ints_append(op, v)?;
                Ok(None)
            }
            _ => Err(self.bad_step()),
        }
    }

    /// Replaces `v[lo:hi]` with the bytes of `src` (a `bytes` or another
    /// `bytearray`; `None` deletes). `src` must not be `v` itself.
    #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
    fn ba_splice(
        &mut self,
        v: Value,
        lo: usize,
        hi: usize,
        src: Option<Value>,
    ) -> Result<(), Raised> {
        let old = self.bytearray_data(v).len();
        let hi = hi.clamp(lo, old);
        let n = src.map_or(0, |s| self.bytes_data(s).len());
        let gone = hi - lo;
        self.burn(old as u64 / 64);
        if n > gone {
            let Some(len) = old.checked_add(n - gone) else {
                return Err(self.err(T::MemoryError, ""));
            };
            self.ba_resize(v, len, hi)?;
        } else if n < gone {
            if let Some(b) = self.ba_block(v) {
                let res = self.heap.copy_bytes(b, hi, b, lo + n, old - hi);
                self.hx_o(res)?;
            }
            self.ba_resize(v, old - (gone - n), usize::MAX)?;
        }
        if let (Some(s), Some(b)) = (
            src.and_then(|s| self.vslot_o(s, 0).as_byte_ref()),
            self.ba_block(v),
        ) && n > 0
        {
            let res = self.heap.copy_bytes(s, 0, b, lo, n);
            self.hx_o(res)?;
        }
        Ok(())
    }

    /// Stores byte `x` at index `k` (in range) of bytearray `v`.
    #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
    fn ba_put(&mut self, v: Value, k: usize, x: u8) {
        if let Some(b) = self.ba_block(v)
            && let Ok(d) = self.heap.bytes_mut(b)
            && let Some(slot) = d.get_mut(k)
        {
            *slot = x;
        }
    }

    /// A byte argument as CPython's `_getbytevalue` reads it.
    #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
    fn byte_arg(&mut self, v: Value) -> Result<u8, Raised> {
        if !Vm::is_intlike(v) {
            if self.uflags(v) & crate::class::f::INDEX != 0 {
                return Err(self.need_py());
            }
            let msg = format!(
                "'{}' object cannot be interpreted as an integer",
                self.type_name(v)
            );
            return Err(self.err(T::TypeError, &msg));
        }
        self.byte_value(v)
    }

    /// The bytes-like source of a slice assignment to `v`: `src` itself,
    /// a copy when it is `v`, or a new `bytes` of an iterable of ints.
    #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
    fn ba_source(&mut self, v: Value, src: Value) -> Result<Value, Raised> {
        if src == v {
            return self.bytes_slice_as(v, false);
        }
        if Vm::is_bytes_like(src) {
            return Ok(src);
        }
        if Vm::is_intlike(src) || src.is_float() || Vm::is_str(src) || Vm::is_complex(src) {
            return Err(self.err(
                T::TypeError,
                "can assign only bytes, buffers, or iterables of ints in range(0, 256)",
            ));
        }
        // A long list or tuple becomes bytes in the prelude, a step at a
        // time (`_ba_items`).
        if self.user_class(src).is_some()
            || self.is_iterator(src)
            || Vm::ints_in_place(src) && self.ints_len(src) > crate::nstep::STEP_WORK as usize
        {
            return Err(self.need_py());
        }
        let items = match self.collect(src) {
            Ok(items) => items,
            Err(e) => {
                if self.exc_type(self.exc) == T::TypeError && !self.slow {
                    let msg = format!(
                        "cannot convert '{}' object to bytearray",
                        self.type_name(src)
                    );
                    return Err(self.err(T::TypeError, &msg));
                }
                return Err(e);
            }
        };
        let (_charge, out) = self.ints_to_bytes(&items, true)?;
        self.new_bytes(&out)
    }

    /// `v[key] = x` for bytearray `v`.
    #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
    pub(crate) fn bytearray_setitem(
        &mut self,
        v: Value,
        key: Value,
        x: Value,
    ) -> Result<(), Raised> {
        let n = self.bytearray_data(v).len();
        if let Some(i) = self.int_of_o(key) {
            let byte = self.byte_arg(x)?;
            let Some(k) = Vm::norm_index(i, n) else {
                return Err(self.err(T::IndexError, "bytearray index out of range"));
            };
            self.ba_put(v, k, byte);
            return Ok(());
        }
        if Vm::is_heap_int(key) {
            return Err(self.index_overflow(T::IndexError));
        }
        if !key.is_obj_kind(ty::SLICE as u8) {
            return Err(self.ba_bad_index(key));
        }
        let (start, _, step, len) = self.slice_indices(key, n)?;
        let src = self.ba_source(v, x)?;
        let need = self.bytes_data(src).len();
        let lo = usize::try_from(start).unwrap_or(0).min(n);
        if step == 1 {
            let need = (n - len).saturating_add(need);
            if self.ba_grows_far(v, need) {
                let w = [lo as u64, (lo + len) as u64];
                return self.ba_grow(v, need, fin::SPLICE, w, src).map(|_| ());
            }
            return self.ba_splice(v, lo, lo + len, Some(src));
        }
        if need == 0 {
            // CPython deletes an extended slice assigned empty bytes.
            return self.ba_del_extended(v, start, step, len);
        }
        if need != len {
            let msg =
                format!("attempt to assign bytes of size {need} to extended slice of size {len}");
            return Err(self.err(T::ValueError, &msg));
        }
        let mut j = Job::new(job::SET_STRIDE, src, 0, len, 0);
        j.v[3] = v;
        (j.w[W_A], j.w[W_A + 1]) = (start as u64, step as u64);
        self.job_run(j).map(|_| ())
    }

    /// `del v[key]` for bytearray `v`.
    #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
    pub(crate) fn bytearray_delitem(&mut self, v: Value, key: Value) -> Result<(), Raised> {
        let n = self.bytearray_data(v).len();
        if let Some(i) = self.int_of_o(key) {
            let Some(k) = Vm::norm_index(i, n) else {
                return Err(self.err(T::IndexError, "bytearray index out of range"));
            };
            return self.ba_splice(v, k, k + 1, None);
        }
        if Vm::is_heap_int(key) {
            return Err(self.index_overflow(T::IndexError));
        }
        if !key.is_obj_kind(ty::SLICE as u8) {
            return Err(self.ba_bad_index(key));
        }
        let (start, _, step, len) = self.slice_indices(key, n)?;
        if step == 1 {
            let lo = usize::try_from(start).unwrap_or(0).min(n);
            return self.ba_splice(v, lo, lo + len, None);
        }
        self.ba_del_extended(v, start, step, len)
    }

    /// Deletes the `len` items `start`, `start + step`, ... (`step != 1`).
    #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
    fn ba_del_extended(
        &mut self,
        v: Value,
        start: i64,
        step: i64,
        len: usize,
    ) -> Result<(), Raised> {
        if len == 0 {
            return Ok(());
        }
        let n = self.bytearray_data(v).len();
        let span = i64::try_from(len - 1).unwrap_or(0).saturating_mul(step);
        let (first, step) = if step < 0 {
            (start.saturating_add(span), step.unsigned_abs())
        } else {
            (start, step.unsigned_abs())
        };
        let first = usize::try_from(first).unwrap_or(0);
        let mut j = Job::new(job::DEL_STRIDE, v, first, n, 0);
        j.v[3] = v;
        (j.w[W_A], j.w[W_A + 1], j.w[W_A + 2]) = (first as u64, step, len as u64);
        // The write position.
        j.w[W_A + 3] = first as u64;
        self.job_run(j).map(|_| ())
    }

    /// A step of a bytearray's extended slice assigned
    /// ([`job::SET_STRIDE`]: byte `k` of the source to `start + k * step`)
    /// or deleted ([`job::DEL_STRIDE`]: the bytes kept moved down, a read
    /// and a write position).
    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn ba_stride_step(
        &mut self,
        j: &mut Job,
        start: u64,
    ) -> Result<Option<Value>, Raised> {
        let v = j.v[3];
        let Some(b) = self.ba_block(v).filter(|_| Vm::is_bytearray(v)) else {
            return Err(self.bad_step());
        };
        let set = j.kind == job::SET_STRIDE;
        loop {
            let (i, end) = (j.w[W_I] as usize, j.w[W_END] as usize);
            if i >= end {
                break;
            }
            let left = crate::nstep::STEP_WORK.saturating_sub(self.work.wrapping_sub(start));
            let k = (end - i)
                .min(1024)
                .min(usize::try_from(left * 64).unwrap_or(usize::MAX));
            if k == 0 {
                return Ok(None);
            }
            let mut buf = [0u8; 1024];
            if set {
                let Some(src) = self.bytes_data(j.v[0]).get(i..i + k) else {
                    return Err(self.bad_step());
                };
                buf[..k].copy_from_slice(src);
            }
            let (first, step) = (j.w[W_A] as i64, j.w[W_A + 1] as i64);
            let n = self.bytearray_data(v).len();
            let Ok(d) = self.heap.bytes_mut(b) else {
                return Err(self.bad_step());
            };
            let m = n.min(d.len());
            let d = &mut d[..m];
            if set {
                for (t, &c) in buf[..k].iter().enumerate() {
                    let at = first.wrapping_add(((i + t) as i64).wrapping_mul(step));
                    if let Some(x) = usize::try_from(at).ok().and_then(|a| d.get_mut(a)) {
                        *x = c;
                    }
                }
            } else {
                let (first, step, len) = (first as usize, step as usize, j.w[W_A + 2] as usize);
                let mut w = j.w[W_A + 3] as usize;
                for r in i..(i + k).min(d.len()) {
                    let off = r - first;
                    if off % step == 0 && off / step < len {
                        continue;
                    }
                    d[w] = d[r];
                    w += 1;
                }
                j.w[W_A + 3] = w as u64;
            }
            self.burn(k.div_ceil(64) as u64);
            j.w[W_I] = (i + k) as u64;
        }
        if !set {
            let n = self.bytearray_data(v).len();
            self.ba_resize(v, n.saturating_sub(j.w[W_A + 2] as usize), usize::MAX)?;
        }
        Ok(Some(Value::NONE))
    }

    /// Bytearray `v` takes byte block `b` (filled, `len` bytes) as its
    /// contents.
    #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
    pub(crate) fn ba_adopt(&mut self, v: Value, b: ByteRef, len: usize) {
        if let Some(r) = v.as_obj().filter(|_| Vm::is_bytearray(v)) {
            self.set_slot_o(r, 0, Value::from_byte_ref(b));
            self.set_raw_o(r, 1, len as u64);
        }
    }

    #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
    fn ba_bad_index(&mut self, key: Value) -> Raised {
        if self.uflags(key) & crate::class::f::INDEX != 0 {
            return self.need_py();
        }
        let msg = format!(
            "bytearray indices must be integers or slices, not {}",
            self.type_name(key)
        );
        self.err(T::TypeError, &msg)
    }

    /// `a += b` for bytearray `a` and bytes-like `b`.
    #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
    pub(crate) fn bytearray_iadd(&mut self, a: Value, b: Value) -> Result<Value, Raised> {
        self.ba_add(a, b, fin::IADD)
    }

    /// `a += b` ([`fin::IADD`], `a` the value) or `a.extend(b)`
    /// ([`fin::EXTEND`], `None`).
    #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
    fn ba_add(&mut self, a: Value, b: Value, f: u64) -> Result<Value, Raised> {
        if a == b && !self.bytearray_data(a).is_empty() {
            // CPython exports `b`'s buffer while resizing `a`.
            return Err(self.err(
                T::BufferError,
                "Existing exports of data: object cannot be re-sized",
            ));
        }
        let n = self.bytearray_data(a).len();
        let need = n.saturating_add(self.bytes_data(b).len());
        if self.ba_grows_far(a, need) {
            return self.ba_grow(a, need, f, [0, 0], b);
        }
        self.ba_splice(a, n, n, Some(b))?;
        Ok(if f == fin::IADD { a } else { Value::NONE })
    }

    /// `a *= k` for bytearray `a`.
    #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
    pub(crate) fn bytearray_imul(&mut self, a: Value, k: usize) -> Result<Value, Raised> {
        self.repeat_job(a, k, a)
    }
}

// ---------------------------------------------------------------- methods

#[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
fn this(vm: &mut Vm, a: &CallArgs, name: &str) -> Result<Value, Raised> {
    let s = vm.arg_o(a, 0);
    if !Vm::is_bytearray(s) {
        let msg = format!(
            "descriptor '{name}' for 'bytearray' objects doesn't apply to a '{}' object",
            vm.type_name(s)
        );
        return Err(vm.err(T::TypeError, &msg));
    }
    Ok(s)
}

/// `bytearray.append(item)`.
#[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
pub(crate) fn append(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let s = this(vm, &a, "append")?;
    let x = vm.byte_arg(vm.arg_o(&a, 1))?;
    let n = vm.bytearray_data(s).len();
    if vm.ba_grows_far(s, n + 1) {
        return vm.ba_grow(s, n + 1, fin::APPEND, [u64::from(x), 0], Value::NONE);
    }
    vm.ba_resize(s, n + 1, usize::MAX)?;
    vm.ba_put(s, n, x);
    Ok(Value::NONE)
}

/// `bytearray.extend(iterable_of_ints)`.
#[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
pub(crate) fn extend(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let s = this(vm, &a, "extend")?;
    let it = vm.arg_o(&a, 1);
    if Vm::is_bytes_like(it) {
        let src = if it == s {
            vm.bytes_slice_as(s, false)?
        } else {
            it
        };
        return vm.ba_add(s, src, fin::EXTEND);
    }
    if Vm::is_str(it) {
        return Err(vm.err(T::TypeError, "expected iterable of integers; got: 'str'"));
    }
    if Vm::ints_in_place(it) {
        return vm.ints_job(it, R_APPEND, s);
    }
    if vm.is_iterator(it) {
        return Err(vm.need_py());
    }
    let items = match vm.collect(it) {
        Ok(items) => items,
        Err(e) => {
            if !vm.slow && vm.exc_type(vm.exc) == T::TypeError && {
                let e = vm.exc;
                vm.exc_str(e).is_ok_and(|m| m.ends_with("is not iterable"))
            } {
                let msg = format!("can't extend bytearray with {}", vm.type_name(it));
                return Err(vm.err(T::TypeError, &msg));
            }
            return Err(e);
        }
    };
    let (_charge, out) = vm.ints_to_bytes(&items, true)?;
    let n = vm.bytearray_data(s).len();
    let Some(len) = n.checked_add(out.len()) else {
        return Err(vm.err(T::MemoryError, ""));
    };
    if vm.ba_grows_far(s, len) {
        let src = vm.new_bytes(&out)?;
        return vm.ba_grow(s, len, fin::EXTEND, [0, 0], src);
    }
    vm.ba_resize(s, len, usize::MAX)?;
    if let Some(b) = vm.ba_block(s)
        && let Ok(d) = vm.heap.bytes_mut(b)
        && let Some(t) = d.get_mut(n..len)
    {
        t.copy_from_slice(&out);
    }
    Ok(Value::NONE)
}

/// `bytearray.insert(index, item)`.
#[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
pub(crate) fn insert(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let s = this(vm, &a, "insert")?;
    let i = vm.int_arg(vm.arg_o(&a, 1), "index")?;
    let x = vm.byte_arg(vm.arg_o(&a, 2))?;
    let n = vm.bytearray_data(s).len();
    let len = i64::try_from(n).unwrap_or(i64::MAX);
    let at = if i < 0 { (i + len).max(0) } else { i.min(len) };
    let at = usize::try_from(at).unwrap_or(0);
    if vm.ba_grows_far(s, n + 1) {
        return vm.ba_grow(
            s,
            n + 1,
            fin::INSERT,
            [at as u64, u64::from(x)],
            Value::NONE,
        );
    }
    vm.ba_resize(s, n + 1, at)?;
    vm.burn(n as u64 / 64);
    vm.ba_put(s, at, x);
    Ok(Value::NONE)
}

/// `bytearray.pop(index=-1)`.
#[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
pub(crate) fn pop(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let s = this(vm, &a, "pop")?;
    let i = match vm.arg_o(&a, 1) {
        Value::UNDEFINED => -1,
        v => vm.int_arg(v, "index")?,
    };
    let n = vm.bytearray_data(s).len();
    if n == 0 {
        return Err(vm.err(T::IndexError, "pop from empty bytearray"));
    }
    let Some(k) = Vm::norm_index(i, n) else {
        return Err(vm.err(T::IndexError, "pop index out of range"));
    };
    let x = vm.bytearray_data(s)[k];
    vm.ba_splice(s, k, k + 1, None)?;
    Ok(Value::int(i64::from(x)).unwrap_or(Value::NONE))
}

/// `bytearray.remove(value)`.
#[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
pub(crate) fn remove(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let s = this(vm, &a, "remove")?;
    let x = vm.byte_arg(vm.arg_o(&a, 1))?;
    let Some(k) = vm.bytearray_data(s).iter().position(|&c| c == x) else {
        return Err(vm.err(T::ValueError, "value not found in bytearray"));
    };
    vm.ba_splice(s, k, k + 1, None)?;
    Ok(Value::NONE)
}

/// `bytearray.clear()`.
#[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
pub(crate) fn clear(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let s = this(vm, &a, "clear")?;
    vm.ba_resize(s, 0, usize::MAX)?;
    Ok(Value::NONE)
}

/// `bytearray.reverse()`.
#[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
pub(crate) fn reverse(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let s = this(vm, &a, "reverse")?;
    let n = vm.bytearray_data(s).len();
    vm.burn(n as u64 / 64);
    if let Some(b) = vm.ba_block(s)
        && let Ok(d) = vm.heap.bytes_mut(b)
        && let Some(t) = d.get_mut(..n)
    {
        t.reverse();
    }
    Ok(Value::NONE)
}

/// `bytearray.copy()`.
#[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
pub(crate) fn copy(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let s = this(vm, &a, "copy")?;
    vm.bytes_copy(s, true)
}
