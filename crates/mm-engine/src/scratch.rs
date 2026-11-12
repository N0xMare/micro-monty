//! Owned native buffers keep their memory reservation through nested allocations.

use crate::vm::{Raised, Vm};
use alloc::{string::String, vec::Vec};
use core::ops::{Deref, DerefMut};
use mm_core::{Error, Heap, Ref, Scratch, Value};

pub(crate) struct NativeVec<T> {
    // Drop storage before releasing its reservation.
    items: Vec<T>,
    charge: Option<Scratch>,
}

pub(crate) type NativeValues = NativeVec<Value>;

impl<T> Default for NativeVec<T> {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            charge: None,
        }
    }
}

/// The capacity a native buffer of capacity `old` grows to for `needed`
/// bytes: double (at least `min`), or, when the budget cannot pay for that,
/// all of what is left (at least `needed`). Growing to exactly `needed`
/// there instead made every later write reallocate and copy the whole
/// buffer, quadratic near the memory limit; this way the buffer grows once
/// more and the growth after that fails.
pub(crate) fn grown(old: usize, needed: usize, min: usize, available: u64) -> usize {
    let doubled = needed.max(old.saturating_mul(2)).max(min);
    if (doubled - old) as u64 <= available {
        doubled
    } else {
        needed.max(old.saturating_add(usize::try_from(available).unwrap_or(usize::MAX)))
    }
}

impl NativeVec<u8> {
    fn reserve_bytes(&mut self, heap: &mut Heap, needed: usize) -> Result<(), Error> {
        let old = self.items.capacity();
        if needed <= old {
            return Ok(());
        }
        self.reserve(heap, grown(old, needed, 0, heap.available_bytes()))
    }

    pub(crate) fn extend_from_slice(&mut self, heap: &mut Heap, bytes: &[u8]) -> Result<(), Error> {
        self.reserve_bytes(heap, self.items.len().saturating_add(bytes.len()))?;
        self.items.extend_from_slice(bytes);
        Ok(())
    }

    pub(crate) fn resize(&mut self, heap: &mut Heap, len: usize, value: u8) -> Result<(), Error> {
        self.reserve_bytes(heap, len)?;
        self.items.resize(len, value);
        Ok(())
    }
}

impl NativeVec<Value> {
    pub(crate) fn copy(heap: &mut Heap, items: &[Value]) -> Result<Self, Error> {
        let mut out = Self::default();
        out.reserve(heap, items.len())?;
        out.items.extend_from_slice(items);
        Ok(out)
    }

    pub(crate) fn from_heap(heap: &mut Heap, r: Ref, n: usize) -> Result<Self, Error> {
        let n = n.min(heap.slot_words(r)?.len());
        let mut out = Self::default();
        out.reserve(heap, n)?;
        out.items.extend(
            heap.slot_words(r)?[..n]
                .iter()
                .map(|&w| Value::from_bits(w).unwrap_or(Value::UNDEFINED)),
        );
        Ok(out)
    }

    pub(crate) fn try_clone(&self, heap: &mut Heap) -> Result<Self, Error> {
        Self::copy(heap, &self.items)
    }
}

impl<T> NativeVec<T> {
    pub(crate) fn push(&mut self, heap: &mut Heap, value: T) -> Result<(), Error> {
        self.push_shared(heap, value)
    }

    /// [`NativeVec::push_shared`] with the room-left case inlined, for hot
    /// loops (one push per piece of a split).
    #[inline]
    pub(crate) fn push_hot(&mut self, heap: &Heap, value: T) -> Result<(), Error> {
        if self.items.len() < self.items.capacity() {
            self.items.push(value);
            return Ok(());
        }
        self.push_shared(heap, value)
    }

    /// [`NativeVec::push`] with shared heap access, for natives that borrow
    /// the VM (comparison walks).
    pub(crate) fn push_shared(&mut self, heap: &Heap, value: T) -> Result<(), Error> {
        if self.items.len() == self.items.capacity() {
            let capacity = self
                .items
                .capacity()
                .checked_mul(2)
                .ok_or(Error::OutOfMemory)?;
            self.reserve_shared(heap, capacity.max(4))?;
        }
        self.items.push(value);
        Ok(())
    }

    pub(crate) fn pop(&mut self) -> Option<T> {
        self.items.pop()
    }

    /// Drops the items from `len` on, keeping the capacity (and its charge).
    pub(crate) fn truncate(&mut self, len: usize) {
        self.items.truncate(len);
    }

    pub(crate) fn reserve(&mut self, heap: &mut Heap, capacity: usize) -> Result<(), Error> {
        self.reserve_shared(heap, capacity)
    }

    fn reserve_shared(&mut self, heap: &Heap, capacity: usize) -> Result<(), Error> {
        if capacity <= self.items.capacity() {
            return Ok(());
        }
        let bytes = capacity
            .checked_mul(size_of::<T>())
            .ok_or(Error::OutOfMemory)?;
        match &mut self.charge {
            Some(charge) => charge.resize(heap, bytes)?,
            None => self.charge = Some(heap.reserve_scratch(bytes)?),
        }
        if self
            .items
            .try_reserve_exact(capacity - self.items.len())
            .is_err()
        {
            if let Some(charge) = &mut self.charge {
                let _ = charge.resize(heap, self.items.capacity() * size_of::<T>());
            }
            return Err(Error::OutOfMemory);
        }
        Ok(())
    }
}

impl<T> Deref for NativeVec<T> {
    type Target = [T];

    fn deref(&self) -> &[T] {
        &self.items
    }
}

impl<T> DerefMut for NativeVec<T> {
    fn deref_mut(&mut self) -> &mut [T] {
        &mut self.items
    }
}

impl<'a, T> IntoIterator for &'a NativeVec<T> {
    type Item = &'a T;
    type IntoIter = core::slice::Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.items.iter()
    }
}

/// Native text stays admitted until its consumer has finished with it.
#[derive(Default)]
pub(crate) struct NativeText {
    // Release storage before its reservation. Direct writes must reserve first.
    pub(crate) text: String,
    charge: Option<Scratch>,
}

impl NativeText {
    /// Split the buffer from its reservation for writers borrowing arena text.
    /// Reborrowing the heap for each write leaves traversal free to mutate it.
    pub(crate) fn parts(&mut self, heap: &mut Heap) -> Result<(&mut String, &mut Scratch), Error> {
        let charge = match self.charge.take() {
            Some(charge) => charge,
            None => heap.reserve_scratch(self.text.capacity())?,
        };
        Ok((&mut self.text, self.charge.insert(charge)))
    }

    pub(crate) fn reserve(&mut self, vm: &mut Vm, additional: usize) -> Result<(), Raised> {
        let needed = self.text.len().saturating_add(additional);
        if needed <= self.text.capacity() {
            return Ok(());
        }
        self.grow(vm, needed)
    }

    #[inline(never)]
    fn grow(&mut self, vm: &mut Vm, needed: usize) -> Result<(), Raised> {
        let old = self.text.capacity();
        let capacity = grown(old, needed, 8, vm.heap.available_bytes());
        let res = match &mut self.charge {
            Some(charge) => charge.resize(&vm.heap, capacity),
            None => vm.heap.reserve_scratch(capacity).map(|charge| {
                self.charge = Some(charge);
            }),
        };
        vm.hx_o(res)?;
        let res = self
            .text
            .try_reserve_exact(capacity - self.text.len())
            .map_err(|_| Error::OutOfMemory);
        if res.is_err()
            && let Some(charge) = &mut self.charge
        {
            let _ = charge.resize(&vm.heap, old);
        }
        vm.hx_o(res)
    }

    pub(crate) fn push(&mut self, vm: &mut Vm, s: &str) -> Result<(), Raised> {
        let n = report_len(vm, self.text.len(), s);
        if n < s.len() {
            vm.report_cut = true;
        }
        self.reserve(vm, n)?;
        self.text.push_str(&s[..n]);
        Ok(())
    }

    pub(crate) fn push_value(&mut self, vm: &mut Vm, v: Value) -> Result<(), Raised> {
        // Size raw storage first so growing does not validate all of UTF-8 twice.
        let n = report_len(vm, self.text.len(), vm.str_ref_o(v));
        if n < vm.str_bytes(v).len() {
            vm.report_cut = true;
        }
        self.reserve(vm, n)?;
        self.text.push_str(&vm.str_ref_o(v)[..n]);
        Ok(())
    }
}

/// How many bytes of `s` a text of `len` bytes takes: all of them, except
/// while a host report renders ([`Vm::begin_report`]), where the text stops
/// one byte past the report's cap, and the caller marks the report cut. An
/// uncaught exception's message or `SyntaxError` text is then copied only
/// as far as the host sees it, not whole.
pub(crate) fn report_len(vm: &Vm, len: usize, s: &str) -> usize {
    if vm.report_cap == usize::MAX {
        return s.len();
    }
    let mut n = vm.report_cap.saturating_sub(len).saturating_add(1);
    if n >= s.len() {
        return s.len();
    }
    while !s.is_char_boundary(n) {
        n -= 1;
    }
    n
}

impl Deref for NativeText {
    type Target = str;

    fn deref(&self) -> &str {
        &self.text
    }
}

impl core::fmt::Display for NativeText {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        self.text.fmt(f)
    }
}

/// Admitted object identities, compared by bits without invoking user code.
/// `UNDEFINED` marks empty slots. Half-full probing and backward-shift deletion
/// reuse capacity without tombstones or allocations when removing identities.
#[derive(Default)]
pub(crate) struct NativeIds {
    ids: NativeValues,
    len: usize,
}

impl NativeIds {
    pub(crate) fn len(&self) -> usize {
        self.len
    }

    fn bucket(id: Value, mask: usize) -> usize {
        usize::try_from(mm_core::hash_bytes(&id.to_bits().to_le_bytes()) & mask as u64).unwrap_or(0)
    }

    fn slot(ids: &[Value], id: Value) -> usize {
        let mask = ids.len() - 1;
        let mut slot = Self::bucket(id, mask);
        while ids[slot] != Value::UNDEFINED && ids[slot] != id {
            slot = (slot + 1) & mask;
        }
        slot
    }

    pub(crate) fn insert(&mut self, heap: &mut Heap, id: Value) -> Result<bool, Error> {
        // An existing identity needs no growth, even if the budget is full.
        let mut slot = 0;
        if !self.ids.is_empty() {
            slot = Self::slot(&self.ids, id);
            if self.ids[slot] == id {
                return Ok(false);
            }
        }
        if (self.len + 1) * 2 > self.ids.len() {
            let capacity = (self.ids.len() * 2).max(8);
            let mut ids = NativeValues::default();
            ids.reserve(heap, capacity)?;
            for _ in 0..capacity {
                ids.push(heap, Value::UNDEFINED)?;
            }
            for &id in &self.ids {
                if id != Value::UNDEFINED {
                    let slot = Self::slot(&ids, id);
                    ids[slot] = id;
                }
            }
            self.ids = ids;
            slot = Self::slot(&self.ids, id);
        }
        self.ids[slot] = id;
        self.len += 1;
        Ok(true)
    }

    /// Remove a previously inserted identity.
    pub(crate) fn remove(&mut self, id: Value) {
        let mask = self.ids.len() - 1;
        let mut hole = Self::slot(&self.ids, id);
        let mut next = (hole + 1) & mask;
        while self.ids[next] != Value::UNDEFINED {
            let bucket = Self::bucket(self.ids[next], mask);
            if (hole.wrapping_sub(bucket) & mask) < (next.wrapping_sub(bucket) & mask) {
                self.ids[hole] = self.ids[next];
                hole = next;
            }
            next = (next + 1) & mask;
        }
        self.ids[hole] = Value::UNDEFINED;
        self.len -= 1;
    }
}
