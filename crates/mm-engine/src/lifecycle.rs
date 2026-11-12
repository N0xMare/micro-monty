//! Reclaim loaded programs between cells, when no executing frame can hold
//! a code-table index. Functions, generators and saved exceptions keep their
//! entire compilation unit alive, including nested functions not yet made.

use alloc::sync::Arc;
use core::mem::size_of;
use mm_compile::bytecode::Code;
use mm_core::{ObjectCursor, Ref, Value};

use crate::class::{Shape, inst};
use crate::generator::g;
use crate::host::HostModule;
use crate::types::ty;
use crate::vm::{CodeObj, Frame, Ic, Raised, Ready, Source, Task, Vm, exc_slot, fn_slot};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Reference {
    Code,
    Traceback,
    Shape,
}

impl CodeObj {
    /// Admission upper bound: clone buffers, materialized constants/names,
    /// Arc allocation, new code/cache entries and restore verifier workspace.
    pub(crate) fn load_bytes(code: &Code) -> u64 {
        code.buffer_bytes()
            + code.verify_scratch_bytes() as u64
            + (size_of::<Self>() + 3 * size_of::<usize>()) as u64
            + (code.consts.len() + code.names.len()) as u64 * size_of::<Value>() as u64
            + u64::from(code.n_ics) * size_of::<Ic>() as u64
    }
}

impl Vm {
    /// Exact-sized tables that never grow after construction, plus the park
    /// flag's shared allocation (including its counters and alignment).
    pub(crate) fn fixed_bytes(modules: usize) -> u64 {
        (crate::types::NTYPES * (size_of::<Value>() + size_of::<u32>())
            + modules * (size_of::<Option<(u32, u32, u32)>>() + size_of::<Value>())
            + 3 * size_of::<usize>()) as u64
    }

    /// The walk of `codes` that `code_bytes` adds: `from` (`code_sums`)
    /// extended over the codes appended since.
    fn code_sums(&self, from: (usize, u64, u64)) -> (usize, u64, u64) {
        let (mut n, mut sum, mut verify) = from;
        if n > self.codes.len() {
            (n, sum, verify) = (0, 0, 0);
        }
        for c in &self.codes[n..] {
            verify = verify.max(c.code.verify_scratch_bytes() as u64);
            sum += c.code.buffer_bytes()
                + (size_of::<CodeObj>()
                    + 2 * size_of::<usize>()
                    + (c.consts.capacity() + c.names.capacity()) * size_of::<Value>())
                    as u64;
        }
        n = self.codes.len();
        (n, sum, verify)
    }

    pub(crate) fn code_bytes(&self) -> u64 {
        let (_, sum, verify) = self.code_sums(self.code_sums);
        (self.codes.capacity() * size_of::<Arc<CodeObj>>()
            + self.sources.capacity() * size_of::<Source>()
            + self.ics.capacity() * size_of::<Ic>()
            + self.synth.capacity() * size_of::<u32>()
            + self.shapes.capacity() * size_of::<Shape>()
            + self.host_modules.capacity() * size_of::<Arc<HostModule>>()) as u64
            + Self::fixed_bytes(self.stdlib.len())
            + self.std_scratch
            + self.host_modules.iter().map(|m| m.bytes).sum::<u64>()
            + (self.host_ids.capacity() * size_of::<alloc::string::String>()) as u64
            + self.host_ids.iter().map(|n| n.len() as u64).sum::<u64>()
            + self
                .host_modules
                .iter()
                .map(|m| m.scratch)
                .max()
                .unwrap_or(0)
            + self
                .shapes
                .iter()
                .map(|s| {
                    (s.names.capacity() * size_of::<Value>()
                        + s.trans.capacity() * size_of::<(Value, u32)>()) as u64
                })
                .sum::<u64>()
            + self
                .sources
                .iter()
                .map(|s| (s.name.capacity() + s.text.capacity()) as u64)
                .sum::<u64>()
            // Restore verifies one code at a time. Keep its maximum workspace
            // available even when other session storage exhausts the budget.
            + verify
            + sum
    }

    /// Checks that `parked_bytes` is in step with the task table. It walks
    /// every task, so it runs only where that is O(1) amortized: when the
    /// table doubles, when tasks are dropped, and when a snapshot is taken.
    pub(crate) fn check_parked_bytes(&self) {
        debug_assert_eq!(
            self.parked_bytes,
            self.tasks
                .iter()
                .flatten()
                .map(Task::buffer_bytes)
                .sum::<u64>(),
            "parked task bytes out of step"
        );
    }

    pub(crate) fn execution_bytes(&self) -> u64 {
        (self.stack.capacity() * size_of::<Value>() + self.specials.capacity() * size_of::<Frame>())
            as u64
            + self.sbuf.capacity() as u64
            + self.parked_bytes
            + (self.conts.capacity() * size_of::<Option<crate::cont::Cont>>()
                + self.cont_free.capacity() * size_of::<u32>()
                + self.tasks.capacity() * size_of::<Option<Task>>()
                + self.ready.capacity() * size_of::<Ready>()
                + self.blocked.capacity() * size_of::<usize>()
                + self.gathers.capacity() * size_of::<Value>()
                + self.consumed.capacity() * size_of::<(Value, crate::scratch::NativeValues)>()
                + self.futures.capacity() * size_of::<(u64, Value)>()
                + self.call_log.capacity() * size_of::<crate::host::CallEntry>()
                + self.fresh.capacity() * size_of::<Value>()) as u64
    }

    // One shared copy: inlined, it would add hundreds of bytes to each of
    // the many growth paths that call it.
    #[inline(never)]
    pub(crate) fn reserve_execution(&mut self, additional: u64) -> Result<(), Raised> {
        let res = self.heap.set_external_bytes(
            self.code_charge
                .saturating_add(self.execution_bytes())
                .saturating_add(additional),
        );
        self.hx_o(res)
    }

    /// Reserve before loading; reconcile unused clone/table capacity afterward.
    /// Cache this cold scan so stack growth never walks every loaded code.
    pub(crate) fn reserve_code(&mut self, additional: u64) -> Result<(), Raised> {
        let bytes = self.code_bytes().saturating_add(additional);
        self.code_sums = self.code_sums(self.code_sums);
        // The full walk is O(loaded codes): only where their count reaches a
        // power of two, so loading N codes stays O(N) with debug assertions
        // on (the `snapshot_restore` fuzz target took 80 s under ASan with a
        // walk per load).
        debug_assert!(
            !self.codes.len().is_power_of_two() || self.code_sums == self.code_sums((0, 0, 0)),
            "code_sums out of step"
        );
        let res = self
            .heap
            .set_external_bytes(bytes.saturating_add(self.execution_bytes()));
        self.hx_o(res)?;
        self.code_charge = bytes;
        Ok(())
    }

    /// Cancels the previous cell and releases its transient execution state
    /// and, once enough has accumulated, unreachable loaded programs.
    /// Previously returned code indices may change; call this before loading
    /// the next program, never before resuming.
    pub fn prepare_cell(&mut self) {
        self.cancel();
        self.exc = Value::NONE;
        self.post = None;
        self.completion_text = None;
        // Reclamation is a full collection plus a heap scan, so it waits
        // until the code and shapes loaded since the last one reach an
        // eighth of the live heap (at least 16 KiB) or a quarter of the
        // memory left, or the heap would be collected at a safepoint
        // anyway: a cell then pays in proportion to what it loaded and
        // allocated, not to the data the session keeps, and dead code
        // (also in images) stays under that bound. Under `gc_off` no
        // collection starts by itself, this one included.
        let debt = self.code_charge.saturating_sub(self.code_kept);
        if !self.gc_off
            && (debt > (self.heap.stats().live_bytes / 8).max(16 << 10)
                || debt > self.heap.available_bytes() / 4
                || (self.heap.should_collect() && !self.heap.incremental()))
        {
            self.reclaim_code();
            self.code_kept = self.code_charge;
        }
    }

    #[cold]
    pub(crate) fn reclaim_code(&mut self) {
        if !self.tasks.is_empty() || self.saved.is_some() || self.request.is_some() {
            return;
        }
        // Synthesized functions are hidden from tracebacks like the prelude,
        // but only live functions keep their code. Bundled source and the
        // continuation trampoline remain available for future cells.
        for (i, c) in Arc::make_mut(&mut self.codes).iter_mut().enumerate() {
            if let Some(c) = unique(c) {
                c.reclaim_live = i == self.tramp as usize || (c.prelude && c.source != u32::MAX);
            }
        }
        if self.codes.iter().all(|c| c.reclaim_live) && self.shapes.len() <= 1 {
            return;
        }
        self.stack.fill(Value::UNDEFINED);
        self.consumed.clear();
        self.partial = None;
        self.collect_now();
        for s in &mut self.shapes {
            s.reclaim_index = u32::MAX;
        }
        self.shapes[0].reclaim_index = 0;

        // Constants cannot contain functions, generators or exceptions, so
        // rooting historical constants for this GC cannot retain dead code.
        // Their heap storage is reclaimed by a subsequent collection.
        let mut tracebacks = false;
        self.visit_references(|vm, r, i, kind| {
            if kind == Reference::Shape {
                if let Some(i) = vm.slot_o(r, i).as_int()
                    && let Some(s) = vm.shapes.get_mut(i as usize)
                {
                    s.reclaim_index = 0;
                }
                return;
            }
            let ci = if kind == Reference::Traceback {
                tracebacks = true;
                vm.slot_o(r, i).as_int().map(|n| (n >> 28) as u64)
            } else {
                Some(vm.raw_o(r, i))
            }
            .and_then(|n| usize::try_from(n).ok());
            if let Some(base) = ci.and_then(|ci| vm.codes.get(ci).map(|c| c.module_base))
                && let Some(c) = Arc::make_mut(&mut vm.codes)
                    .get_mut(base as usize)
                    .and_then(unique)
            {
                c.reclaim_live = true;
            }
        });
        for i in 0..self.codes.len() {
            let live = self
                .codes
                .get(self.codes[i].module_base as usize)
                .is_some_and(|c| c.reclaim_live);
            if let Some(c) = unique(&mut Arc::make_mut(&mut self.codes)[i]) {
                c.reclaim_live |= live;
            }
        }

        // Workspace belongs to the already admitted records, so reclamation
        // does not need additional budget when the session is full. Prefix
        // counts also map exclusive ends of prelude/stdlib ranges.
        let mut count = 0u32;
        for c in Arc::make_mut(&mut self.codes).iter_mut().filter_map(unique) {
            c.reclaim_index = count;
            count += u32::from(c.reclaim_live);
        }
        let codes_changed = count as usize != self.codes.len();
        let shapes_changed = self.map_shapes();
        if !codes_changed && !shapes_changed {
            return;
        }
        // Dropping only trailing records does not change any live reference.
        // This is the usual case for transient cells; avoid a second heap scan.
        let codes_moved = self
            .codes
            .iter()
            .enumerate()
            .any(|(i, c)| c.reclaim_live && c.reclaim_index as usize != i);
        let shapes_moved = self
            .shapes
            .iter()
            .enumerate()
            .any(|(i, s)| s.reclaim_index != u32::MAX && s.reclaim_index as usize != i);
        if codes_moved || shapes_moved {
            self.visit_references(|vm, r, i, kind| {
                if kind == Reference::Shape {
                    if shapes_moved
                        && let Some(old) = vm.slot_o(r, i).as_int()
                        && let Some(s) = vm.shapes.get(old as usize)
                    {
                        let shape = Value::int(i64::from(s.reclaim_index)).unwrap_or(Value::NONE);
                        vm.set_slot_o(r, i, shape);
                    }
                } else if !codes_moved {
                    // Shape-only reclamation does not touch code references.
                } else if kind == Reference::Traceback {
                    // Shared traceback lists must be remapped only once. Invert
                    // nonnegative packed entries until the following pass; no
                    // allocation, GC, callback or snapshot can observe this tag.
                    if let Some(n) = vm.slot_o(r, i).as_int()
                        && n >= 0
                        && let Some(c) = vm.codes.get((n >> 28) as usize)
                        && let Some(v) =
                            Value::int(!((i64::from(c.reclaim_index) << 28) | (n & 0x0fff_ffff)))
                    {
                        vm.set_slot_o(r, i, v);
                    }
                } else if let Some(c) = vm.codes.get(vm.raw_o(r, i) as usize) {
                    vm.set_raw_o(r, i, u64::from(c.reclaim_index));
                }
            });
        }
        if codes_moved && tracebacks {
            self.visit_references(|vm, r, i, kind| {
                if kind == Reference::Traceback
                    && let Some(n) = vm.slot_o(r, i).as_int()
                    && n < 0
                    && let Some(v) = Value::int(!n)
                {
                    vm.set_slot_o(r, i, v);
                }
            });
        }
        if shapes_changed {
            self.compact_shapes();
        }
        if !codes_changed {
            let _ = self.reserve_code(0);
            return;
        }
        let remap = |ci: u32| {
            self.codes
                .get(ci as usize)
                .map_or(count, |c| c.reclaim_index)
        };
        self.tramp = remap(self.tramp);
        for (first, end, entry) in self.std_codes.iter_mut().flatten() {
            *first = remap(*first);
            *end = remap(*end);
            *entry = remap(*entry);
        }
        self.synth
            .retain(|&ci| self.codes[ci as usize].reclaim_live);
        for ci in &mut self.synth {
            *ci = remap(*ci);
        }

        for s in &mut self.sources {
            s.reclaim_index = u32::MAX;
        }
        for c in self.codes.iter().filter(|c| c.reclaim_live) {
            if let Some(s) = self.sources.get_mut(c.source as usize) {
                s.reclaim_index = 0;
            }
        }
        let mut next = 0;
        for s in &mut self.sources {
            if s.reclaim_index != u32::MAX {
                s.reclaim_index = next;
                next += 1;
            }
        }
        // Read mappings before compacting either table. No frames exist
        // between cells: the table uniquely owns its codes.
        for i in 0..self.codes.len() {
            let c = &self.codes[i];
            let base = self.codes[c.module_base as usize].reclaim_index;
            let source = self
                .sources
                .get(c.source as usize)
                .map_or(u32::MAX, |s| s.reclaim_index);
            if let Some(c) = unique(&mut Arc::make_mut(&mut self.codes)[i]) {
                c.module_base = base;
                c.source = source;
            }
        }
        self.code_sums = (0, 0, 0);
        Arc::make_mut(&mut self.codes).retain(|c| c.reclaim_live);
        self.sources.retain(|s| s.reclaim_index != u32::MAX);
        self.ics.clear();
        for c in Arc::make_mut(&mut self.codes).iter_mut().filter_map(unique) {
            c.ic_base = u32::try_from(self.ics.len()).unwrap_or(u32::MAX);
            self.ics
                .extend(core::iter::repeat_n(Ic::Empty, c.code.n_ics as usize));
        }
        Arc::make_mut(&mut self.codes).shrink_to_fit();
        self.sources.shrink_to_fit();
        self.ics.shrink_to_fit();
        self.synth.shrink_to_fit();
        // Reclamation only decreases the reservation.
        let _ = self.reserve_code(0);
    }

    /// Visits code and shape references in one heap scan without copying
    /// addresses. The visitor may rewrite slots and metadata, not allocate
    /// or collect. Keep the visitor inline in this cold, data-proportional loop.
    fn visit_references(&mut self, mut visit: impl FnMut(&mut Vm, Ref, u32, Reference)) {
        self.heap.finish_sweep();
        let mut cursor = ObjectCursor::default();
        while let Some((r, h)) = self.heap.next_object(&mut cursor) {
            match h.type_id() {
                ty::FUNC => visit(self, r, fn_slot::CODE, Reference::Code),
                ty::GEN | ty::AGEN => visit(self, r, g::CODE, Reference::Code),
                ty::INST => visit(self, r, inst::SHAPE, Reference::Shape),
                ty::EXC => {
                    let tb = self.slot_o(r, exc_slot::TRACEBACK);
                    if let Some((buf, n)) = self.list_parts(tb) {
                        for i in 0..u32::try_from(n).unwrap_or(0) {
                            visit(self, buf, i, Reference::Traceback);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// Add transition ancestors to the marked instance shapes, then number
    /// survivors. Only between cells: no inline cache retains an old id.
    fn map_shapes(&mut self) -> bool {
        // Transitions always point to newer shapes, including after compaction.
        for i in (0..self.shapes.len()).rev() {
            if self.shapes[i]
                .trans
                .iter()
                .any(|&(_, to)| self.shapes[to as usize].reclaim_index != u32::MAX)
            {
                self.shapes[i].reclaim_index = 0;
            }
        }
        let mut count = 0;
        for s in &mut self.shapes {
            if s.reclaim_index != u32::MAX {
                s.reclaim_index = count;
                count += 1;
            }
        }
        count as usize != self.shapes.len()
    }

    /// Instances have been remapped. Compact their shape table in place.
    fn compact_shapes(&mut self) {
        // Rewrite transitions before retaining shapes, while old indices
        // still address the complete mapping.
        for i in 0..self.shapes.len() {
            if self.shapes[i].reclaim_index != u32::MAX {
                for j in 0..self.shapes[i].trans.len() {
                    let to = self.shapes[i].trans[j].1 as usize;
                    self.shapes[i].trans[j].1 = self.shapes[to].reclaim_index;
                }
            }
        }
        self.shapes.retain_mut(|s| {
            let keep = s.reclaim_index != u32::MAX;
            if keep {
                s.trans.retain(|&(_, to)| to != u32::MAX);
                s.trans.shrink_to_fit();
            }
            keep
        });
        self.shapes.shrink_to_fit();
    }
}

/// The code record to update in place: between cells the table owns its
/// codes uniquely. Outlined: `Arc::get_mut`'s uniqueness check is several
/// instructions, and reclamation calls it from five places.
#[inline(never)]
fn unique(code: &mut Arc<CodeObj>) -> Option<&mut CodeObj> {
    Arc::get_mut(code)
}
