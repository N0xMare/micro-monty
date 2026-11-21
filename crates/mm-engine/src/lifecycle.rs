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
use crate::vm::{CodeObj, Frame, Ic, Raised, Ready, Source, TICKS, Task, Vm, exc_slot, fn_slot};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Reference {
    Code,
    Traceback,
    Shape,
}

impl CodeObj {
    /// Admission upper bound: clone buffers, materialized constants/names,
    /// Arc allocation, new code/cache entries and restore verifier workspace.
    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn load_bytes(code: &Code) -> u64 {
        code.buffer_bytes()
            + code.verify_scratch_bytes() as u64
            + (size_of::<Self>() + 3 * size_of::<usize>()) as u64
            + (code.consts.len() + code.names.len()) as u64 * size_of::<Value>() as u64
            + u64::from(code.n_ics) * size_of::<Ic>() as u64
    }
}

/// What a session keeps counting across [`Vm::reset`] that a host or a
/// cell can observe: the next host call id, the tasks numbered (`Task-N`)
/// and the safepoint phase, where regular checks fall
/// (`Vm::reset_ticks`). A session that restarts from a state captured
/// elsewhere ([`Vm::adopt`]) continues these, not the capture's.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counters {
    calls: u64,
    tasks: u64,
    phase: i32,
}

impl Counters {
    /// These counters moved on as they did from `from` to `to`.
    #[must_use]
    #[cfg_attr(mm_shipped, optimize(size))]
    pub fn advanced(self, from: Counters, to: Counters) -> Counters {
        Counters {
            calls: self.calls.wrapping_add(to.calls.wrapping_sub(from.calls)),
            tasks: self.tasks.wrapping_add(to.tasks.wrapping_sub(from.tasks)),
            phase: (self.phase + to.phase - from.phase).rem_euclid(TICKS),
        }
    }
}

impl Vm {
    /// [`Counters`] now (between cells).
    #[must_use]
    #[cfg_attr(mm_shipped, optimize(size))]
    pub fn counters(&self) -> Counters {
        let used = i32::try_from(self.ticks_used()).unwrap_or(0);
        Counters {
            calls: self.next_call,
            tasks: self.task_counter,
            phase: (self.sp_phase + used) % TICKS,
        }
    }

    /// This VM, read from an image, takes the place of `old` between
    /// cells: it keeps what a reset of `old` would have kept and an image
    /// does not hold or holds as of its capture. Moved over: the host
    /// side (output, clocks, deadline, entropy, zone source, park flag,
    /// module cache, GC testing modes and incremental settings), the fuel
    /// set for the next cell,
    /// the settings (host arguments, hint overrides) and `old`'s host call
    /// names when this image's are a prefix of them. `counters` replace
    /// this image's, and the call log's ids move with them. The recursion
    /// limit is the image's (what a reset and the setup leave), within
    /// `old`'s ceiling.
    ///
    /// Fallible steps come first: on an error `old` is untouched.
    ///
    /// # Errors
    ///
    /// A `MemoryError` if `old`'s host call names or a hint override do not
    /// fit.
    #[cfg_attr(mm_shipped, optimize(size))]
    pub fn adopt(&mut self, old: &mut Vm, counters: Counters) -> Result<(), Raised> {
        if old.host_ids.len() > self.host_ids.len() {
            self.host_ids.clone_from(&old.host_ids);
            self.reserve_code(0)?;
        }
        for group in crate::HintGroup::ALL {
            let text = old.hint_override(group);
            if text != self.hint_override(group) {
                self.set_hint(group, text.as_deref())?;
            }
        }
        let shift = counters.calls.wrapping_sub(self.next_call);
        for e in &mut self.call_log {
            e.0 = e.0.wrapping_add(shift);
        }
        self.next_call = counters.calls;
        self.task_counter = counters.tasks;
        // The image holds its own ceiling; a template's engine may have
        // allowed more than the session's.
        self.recursion_ceiling = old.recursion_ceiling;
        self.recursion_limit = self.recursion_limit.min(old.recursion_ceiling);
        self.fuel = old.fuel;
        self.fuel_overdraft = old.fuel_overdraft;
        self.gc_off = old.gc_off;
        self.gc_yield = old.gc_yield;
        self.heap.copy_incremental(&old.heap);
        if old.gc_stress {
            self.set_gc_stress(true);
        }
        if old.gc_inc_stress {
            self.set_gc_inc_stress(true);
        }
        // Phase `counters.phase` with no safepoint run since, and a period
        // that ends by the next multiple of `TICKS` from there.
        self.ticks = self.tick_period;
        self.tick_spare = 0;
        self.sp_phase = counters.phase;
        self.reset_ticks();
        self.output = old.output.take();
        self.clock = old.clock.take();
        self.deadline = old.deadline;
        self.wall_clock = old.wall_clock.take();
        self.entropy = old.entropy.take();
        self.zone_source = old.zone_source.take();
        self.park = Arc::clone(&old.park);
        self.module_cache = old.module_cache.take();
        self.host_code_in_snapshots = old.host_code_in_snapshots;
        self.known_modules = old.known_modules;
        self.host_args = old.host_args;
        Ok(())
    }

    /// Whether a session in `old`'s place may take this image's state
    /// ([`Vm::adopt`]): the same host modules (name, functions, constants,
    /// spec, in order), host call names one a prefix of the other, so no
    /// function id changes, and the same local time zone, which modules
    /// the setup imported have read.
    #[must_use]
    #[cfg_attr(mm_shipped, optimize(size))]
    pub fn adoptable(&self, old: &Vm) -> bool {
        let n = self.host_ids.len().min(old.host_ids.len());
        self.same_host_modules(old)
            && self.host_ids[..n] == old.host_ids[..n]
            && self.local_zone == old.local_zone
    }

    /// Exact-sized tables that never grow after construction, plus the park
    /// flag's shared allocation (including its counters and alignment).
    #[cfg_attr(mm_shipped, optimize(size))]
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

    #[cfg_attr(mm_shipped, optimize(size))]
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
    #[cfg_attr(mm_shipped, optimize(size))]
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
                + self.fresh.capacity() * size_of::<Value>()
                + self.gens.capacity() * size_of::<Value>()) as u64
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
        self.hx_hot(res)
    }

    /// The code charge (the session's code table, bundled workspace and
    /// loaded programs) and how many codes a restore left without their
    /// instructions; for tests.
    #[doc(hidden)]
    #[must_use]
    pub fn code_state(&self) -> (u64, usize) {
        let deferred = self
            .codes
            .iter()
            .filter(|c| c.code.bytecode_deferred())
            .count();
        (self.code_bytes(), deferred)
    }

    /// Decodes the instructions of restored bundled code `ci` before it
    /// first runs or is read ([`crate::vm::load_bytecode_in`]); nothing for
    /// any other code. It burns no fuel and charges nothing (the restore
    /// charged the full decode), so a cell runs the same whether its
    /// session was restored or not.
    pub(crate) fn load_bytecode(&mut self, ci: u32) -> Result<(), Raised> {
        if self
            .codes
            .get(ci as usize)
            .is_some_and(|c| c.code.bytecode_deferred())
        {
            return self.decode_bytecode(ci);
        }
        Ok(())
    }

    /// [`Vm::load_bytecode`] for the frame at `pc` of code `ci`, whose
    /// fetch in the dispatch loop's copy of the table found the
    /// placeholder ([`mm_compile::bytecode::DEFERRED`]); `SystemError` if
    /// the code holds opcode 255 there once decoded, which no compiled code
    /// does. Out of line, so the dispatch loop's registers stay as they are.
    #[cold]
    #[inline(never)]
    pub(crate) fn load_frame_code(&mut self, ci: u32, pc: usize) -> Result<(), Raised> {
        use mm_compile::bytecode::DEFERRED;
        self.load_bytecode(ci)?;
        if self
            .codes
            .get(ci as usize)
            .and_then(|c| c.code.bytecode.get(pc))
            .is_none_or(|&w| w & 0xff == DEFERRED & 0xff)
        {
            return Err(self.err(crate::types::T::SystemError, "bad opcode 255 (length 1)"));
        }
        Ok(())
    }

    #[cold]
    #[inline(never)]
    fn decode_bytecode(&mut self, ci: u32) -> Result<(), Raised> {
        let cache = self.module_cache.clone();
        crate::vm::load_bytecode_in(
            &mut Arc::make_mut(&mut self.codes)[..],
            ci as usize,
            &self.sources,
            self.stdlib,
            cache.as_deref(),
            &mut self.unpackers,
        )
        .map_err(|_| {
            self.err(
                crate::types::T::SystemError,
                "micro-monty: a bundled module is corrupt",
            )
        })
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
        self.hx_hot(res)?;
        self.code_charge = bytes;
        Ok(())
    }

    /// The code loaded so far is the session's setup: [`Vm::prepare_cell`]'s
    /// reclamation trigger counts only what is loaded from here on. A setup
    /// (host definitions, a bootstrap cell's functions) stays reachable, so
    /// counted, it brought the first reclamation's collection forward to
    /// the first few cells, which freed nothing. It stays charged to the
    /// memory limit, and the next reclamation frees what a cell dropped of
    /// it. Between cells.
    pub fn setup_done(&mut self) {
        self.code_kept = self.code_charge;
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
        //
        // A cell that ends with the heap at its wall leaves what it or a
        // later cell drops (`del xs` after a fill) to no safepoint: none
        // runs before the next cell's first object, its load's included,
        // and every later cell was refused for room. Collected here in
        // full and uncharged, as a snapshot's collection is, so fuel does
        // not depend on whether the host snapshotted between cells. With
        // live data at the wall each start pays it once
        // (`Heap::idle`). That collection is reclamation's: the triggers
        // above read counts a snapshot's collection resets, so a plain
        // session dropped the dead code before the next load where one
        // snapshotted between the cells did not, and a cell's outcome at
        // the wall depended on the snapshot. It is the start's one
        // collection; the constants of the code it drops are collected by
        // `Vm::between_cells`' rerun if the start is refused.
        let debt = self.code_charge.saturating_sub(self.code_kept);
        let wall = !self.gc_off && !self.heap.idle() && self.heap.at_wall();
        if wall
            || !self.gc_off
                && (debt > (self.heap.stats().live_bytes / 8).max(16 << 10)
                    || debt > self.heap.available_bytes() / 4
                    || (self.heap.should_collect() && !self.heap.incremental()))
        {
            let collected = self.reclaim_code();
            self.code_kept = self.code_charge;
            if wall && !collected {
                self.collect_now();
            }
        }
    }

    /// Returns whether it collected.
    #[cold]
    pub(crate) fn reclaim_code(&mut self) -> bool {
        if !self.tasks.is_empty() || self.saved.is_some() || self.request.is_some() {
            return false;
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
            return false;
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
                if let Some(i) = vm.slot(r, i).as_int()
                    && let Some(s) = vm.shapes.get_mut(i as usize)
                {
                    s.reclaim_index = 0;
                }
                return;
            }
            let ci = if kind == Reference::Traceback {
                tracebacks = true;
                vm.slot(r, i).as_int().map(|n| (n >> 28) as u64)
            } else {
                Some(vm.raw(r, i))
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
            return true;
        }
        // The collection above ran before the dead code's constants are let
        // go below: a refused load collects them (`Heap::idle_collected`).
        self.heap.idle_collected = false;
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
                        && let Some(old) = vm.slot(r, i).as_int()
                        && let Some(s) = vm.shapes.get(old as usize)
                    {
                        let shape = Value::int(i64::from(s.reclaim_index)).unwrap_or(Value::NONE);
                        vm.set_slot(r, i, shape);
                    }
                } else if !codes_moved {
                    // Shape-only reclamation does not touch code references.
                } else if kind == Reference::Traceback {
                    // Shared traceback lists must be remapped only once. Invert
                    // nonnegative packed entries until the following pass; no
                    // allocation, GC, callback or snapshot can observe this tag.
                    if let Some(n) = vm.slot(r, i).as_int()
                        && n >= 0
                        && let Some(c) = vm.codes.get((n >> 28) as usize)
                        && let Some(v) =
                            Value::int(!((i64::from(c.reclaim_index) << 28) | (n & 0x0fff_ffff)))
                    {
                        vm.set_slot(r, i, v);
                    }
                } else if let Some(c) = vm.codes.get(vm.raw(r, i) as usize) {
                    vm.set_raw(r, i, u64::from(c.reclaim_index));
                }
            });
        }
        if codes_moved && tracebacks {
            self.visit_references(|vm, r, i, kind| {
                if kind == Reference::Traceback
                    && let Some(n) = vm.slot(r, i).as_int()
                    && n < 0
                    && let Some(v) = Value::int(!n)
                {
                    vm.set_slot(r, i, v);
                }
            });
        }
        if shapes_changed {
            self.compact_shapes();
        }
        if !codes_changed {
            let _ = self.reserve_code(0);
            return true;
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
        // The codes are renumbered: a dead set the collection above (or an
        // earlier one) cached under an old index would be another code's.
        // Nothing between that collection and here collects.
        self.dead_cache.clear();
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
        true
    }

    /// Visits code and shape references in one heap scan without copying
    /// addresses. The visitor may rewrite slots and metadata, not allocate
    /// or collect. Keep the visitor inline in this cold, data-proportional loop.
    #[cfg_attr(mm_shipped, optimize(size))]
    fn visit_references(&mut self, mut visit: impl FnMut(&mut Vm, Ref, u32, Reference)) {
        self.heap.finish_sweep();
        let mut cursor = ObjectCursor::default();
        while let Some((r, h)) = self.heap.next_object(&mut cursor) {
            match h.type_id() {
                ty::FUNC => visit(self, r, fn_slot::CODE, Reference::Code),
                ty::GEN | ty::AGEN => visit(self, r, g::CODE, Reference::Code),
                ty::INST => visit(self, r, inst::SHAPE, Reference::Shape),
                ty::EXC => {
                    let tb = self.slot(r, exc_slot::TRACEBACK);
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
    #[cfg_attr(mm_shipped, optimize(size))]
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
    #[cfg_attr(mm_shipped, optimize(size))]
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
