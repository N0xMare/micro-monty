//! Coroutines, tasks and the scheduler (docs/spec.md §8.5).
//!
//! A coroutine object holds its function and bound arguments until it
//! starts. Awaiting it runs its frame inline on the awaiting task's stack, as
//! a call; `asyncio.gather` wraps each coroutine in a task, which has a stack
//! of its own. Suspending a task (an `await` on something unsettled) saves its
//! registers and swaps its stack out, so no Rust frame ever holds a
//! suspended coroutine. Awaitables (tasks, host futures, gathers, sleeps)
//! share one layout; a blocked task re-runs its `AWAIT` when the awaitable
//! settles. Settling queues what waits on the awaitable the way asyncio's
//! done callbacks do (a task's wakeup; a gather's check, which then wakes
//! the gather's waiters), so tasks interleave as on CPython. When nothing
//! can run and some task waits on host futures, the run stops with
//! `Exit::Waiting`.

use crate::types::{T, ty};
use crate::vm::{
    Exit, HDR, HOLE, NOT_BLOCKED, ParkReason, Raised, Ready, State, Task, Vm, fn_slot,
};
use alloc::{format, string::String, vec::Vec};
use mm_core::{Ref, Value};

/// One awaitable whose children a cancel walks ([`Vm::cancel_walk`]): a
/// task (one child, what it waits on), a gather (its children tuple) or,
/// at the bottom, the cancel's root (`node` `NONE`, its child the root).
/// A frame's children are as deep in the tree as the frames under it.
#[derive(Clone, Copy)]
struct Frame {
    node: Value,
    /// A task's waiter, a gather's children, the root.
    src: Value,
    /// Its next child.
    next: usize,
    /// Second pass: it took the cancel (a gather: a child did).
    took: bool,
}

impl Frame {
    fn root(root: Value) -> Self {
        Frame {
            node: Value::NONE,
            src: root,
            next: 0,
            took: false,
        }
    }
}

/// The slots of a [`Frame`] in a cancel's state (`nstep::op::CANCEL`):
/// node, next, took. Its `src` is read from the tree again.
const FRAME_WORDS: usize = 3;

/// The work a cancel's step does at most, in visits of an awaitable that
/// settles nothing costly (about 100 ns each: a millisecond and a half a
/// step [M]). A visit costs more by what its settle scans
/// ([`Vm::settle_work`]), so one visit can pass it.
const CANCEL_STEP: u64 = 1 << 14;

/// The task turns and gather notifications between the scheduler's own
/// checks (`Vm::sched_due`): a task that starts, awaits and blocks with
/// no call or loop passes no safepoint, so 200,000 of them were one step
/// of 66.9 ms [M]; a check every 256 is a step of well under 1 ms.
const SCHED_TURNS: u32 = 256;
const SCHED_BURNED: u64 = 1 << 14;

/// A cancel in progress ([`Vm::cancel_walk`]).
struct CancelWalk {
    root: Value,
    msg: Value,
    /// The second pass.
    act: bool,
    frames: Vec<Frame>,
    /// The callbacks its settles queue into room it reserves: found by the
    /// first pass, still to queue in the second.
    callbacks: usize,
    /// The most frames the first pass held.
    deepest: usize,
    /// The work the first pass found for the second ([`CANCEL_STEP`]'s
    /// unit), at least.
    rest: u64,
}

impl CancelWalk {
    /// The slots its frames need in a state, now and at their deepest.
    fn room(&self) -> usize {
        self.deepest
            .max(self.frames.len())
            .saturating_mul(FRAME_WORDS)
    }
}

/// Where a cancel's walk stopped.
enum Walked {
    /// The second pass ended: did the root take the cancel.
    Done(bool),
    /// The first pass ended.
    Counted,
    /// Its step's work is spent.
    Spent,
}

/// Who made a cancel in steps and goes on when it ends: raw word 5 of its
/// state.
pub(crate) mod cancel_for {
    /// The program's `cancel()`: `_steps` runs the steps and gives the
    /// call its value.
    pub const CALL: u64 = 0;
    /// The scheduler, for a task that had a cancel to raise when it
    /// blocked (`Vm::block_current`): the state's `owner` is that task.
    pub const AWAIT: u64 = 1;
    /// The scheduler, for the main task when a `KeyboardInterrupt` or
    /// `SystemExit` leaves the loop (`Vm::finish_task`).
    pub const ESCAPE: u64 = 2;
}

/// How [`Vm::cancel_begin`] left a cancel.
enum Begun {
    /// Done: did anything take it.
    Done(bool),
    /// In steps: its state.
    State(Value),
}

/// Coroutine object slots and states.
pub(crate) mod coro {
    pub const FUNC: u32 = 0;
    /// The bound arguments, then the registers while suspended.
    pub const ARGS: u32 = 1;
    /// While suspended: what its `AWAIT` waits on (a delegation buffer or
    /// the suspended coroutine it awaits).
    pub const AWAITED: u32 = 2;
    /// While suspended inside another coroutine's `await`: that coroutine.
    pub const AWAITER: u32 = 3;
    /// The value `send()` passes on (or the exception `throw()` raises).
    pub const SENT: u32 = 4;
    /// The handled exception inside it while suspended by `send()`.
    pub const HANDLED: u32 = 5;
    /// The caller's handled exception while `send()` runs it.
    pub const OUTER: u32 = 6;
    pub const STATE: u32 = 7;
    /// While suspended: the pc of its `AWAIT`.
    pub const PC: u32 = 8;
    /// How [`SENT`] is delivered: [`SEND`] or [`THROW`].
    pub const MODE: u32 = 9;
    pub const CREATED: u64 = 0;
    pub const RUNNING: u64 = 1;
    pub const DONE: u64 = 2;
    /// Suspended in an `await` whose iterator yielded to `send()`'s caller.
    pub const SUSPENDED: u64 = 3;
    pub const SEND: u64 = 0;
    pub const THROW: u64 = 1;
    /// State bit: the coroutine is `asyncio.run`'s; other tasks end with it.
    pub const RUN_ROOT: u64 = 4;
    /// State bit, with [`RUN_ROOT`]: a `run_until_complete()`, not an
    /// `asyncio.run` (its loop is not closed when it ends).
    pub const RUN_OPEN: u64 = 16;
    /// State bit, with [`RUN_OPEN`]: the run awaits a future made before
    /// it, so its own task takes no number.
    pub const RUN_ANON: u64 = 32;
    /// The bits a coroutine keeps while it runs and suspends.
    pub const RUN_BITS: u64 = RUN_ROOT | RUN_OPEN | RUN_ANON;
    /// State bit: a task (`create_task`, `gather`) runs it, so it is not
    /// "never awaited" even if the task never runs.
    pub const SCHEDULED: u64 = 8;
}

/// The layout every awaitable shares: a result (value or exception), one
/// kind-specific value, a state, the settle order and a kind-specific word.
pub(crate) mod aw {
    /// The value or the exception. A pending task keeps here the message
    /// of a cancel that must reach its next step; a cancelled awaitable its
    /// cancel message (`_cancel_message`), or, with [`HELD`], the
    /// `CancelledError` that ended a task until it is first retrieved.
    pub const RESULT: u32 = 0;
    pub const EXTRA: u32 = 1;
    /// What waits on this awaitable, in registration order (CPython's done
    /// callbacks): `None`, one entry, or a BUF with a small-int length in
    /// slot zero followed by its entries and then spare slots. An entry is a
    /// gather it is a child of, a task slot (small int) blocked on it, or a
    /// callable from `add_done_callback`.
    pub const WAITERS: u32 = 2;
    pub const STATE: u32 = 3;
    pub const SEQ: u32 = 4;
    pub const AUX: u32 = 5;
    /// A gather's two counts: its distinct children still pending, in
    /// the low 32 bits ([`LEFT_MASK`], a term of `Vm::callback_queue`),
    /// and above [`UNRUN_SHIFT`] its children's notifications that have
    /// not run (CPython's `nfuts - nfinished`). It settles in the
    /// notification of its last child, or of its first failed one. A
    /// children tuple holds far fewer than 2^32 (`MAX_OBJECT_BYTES`).
    pub const LEFT: u32 = 6;
    pub const LEFT_MASK: u64 = 0xFFFF_FFFF;
    pub const UNRUN_SHIFT: u32 = 32;
    /// The same word of a task: its cancel requests (`cancelling()`, the
    /// low [`COUNT`] bits), [`INTERNAL`], and its `Task-N` number above.
    pub const CANCELS: u32 = 6;
    pub const COUNT: u64 = 0x7FFF_FFFF;
    /// A task of the scheduler's own (a timer), which `asyncio.run` and
    /// `all_tasks()` leave out.
    #[cfg(feature = "asyncio")]
    pub const INTERNAL: u64 = 1 << 31;
    #[cfg(feature = "asyncio")]
    pub const NUMBER_SHIFT: u32 = 32;
    /// A gather's flags: [`CANCEL_REQUESTED`].
    pub const FAILS: u32 = 7;
    /// The same word of a pending host future: something awaited or
    /// gathered it (nonzero).
    pub const AWAITED: u32 = 7;
    /// The same word of a pending task: a cancel waits for its next step
    /// (`_must_cancel`, nonzero); of a cancelled awaitable, [`HELD`] or 0.
    pub const MUST_CANCEL: u32 = 7;
    /// A cancel reached a child (`_cancel_requested`).
    pub const CANCEL_REQUESTED: u64 = 2;
    /// Set only inside [`super::Vm::pending_ids`], on the gathers its walk
    /// has reached: never in an image, nor seen by anything else.
    pub const VISITED: u64 = 4;
    /// Above the flags of a gather the scheduler tracks: its place in
    /// `Vm::gathers`, counted from one (`Vm::tracked`). Kept while the
    /// session runs only: an image is written without it and a restore
    /// sets it from the list, so no image holds it.
    pub const PLACE_SHIFT: u32 = 8;
    /// [`RESULT`] of a cancelled task holds the error that ended it.
    pub const HELD: u64 = 2;
    /// The same word of a failed task or loop future: its exception was
    /// never retrieved (`_log_traceback`).
    pub const LOG_TB: u64 = 4;
    pub const PENDING: u64 = 0;
    pub const DONE: u64 = 1;
    pub const FAILED: u64 = 2;
    pub const CANCELLED: u64 = 3;
    /// A sleep's states: created, armed (waiting for its wake time), spent,
    /// or [`CANCELLED`] while armed.
    pub const ARMED: u64 = 1;
    pub const SPENT: u64 = 2;
}

/// What an `AWAIT` did.
pub(crate) enum Awaited {
    /// The value is ready.
    Value(Value),
    /// A coroutine frame was pushed.
    Frame,
    /// The task must wait.
    Block,
    /// Run the `AWAIT` again with this accumulator.
    Retry(Value),
    /// The awaitable left a request whose result the `AWAIT` runs on.
    RequestRetry,
    /// The awaitable left a request (an async generator step).
    Request,
}

/// What the scheduler picked.
pub(crate) enum Next {
    Run(State),
    /// Run this task by raising the pending exception at its pc.
    Raise(State),
    Wait(Vec<u64>),
    /// Park while the scheduler waits for a collection (`LOOP_START_WAIT`).
    Park(ParkReason, State),
}

/// The state of a host future, for checking a batch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FutureStatus {
    /// Awaiting its result.
    Pending,
    /// Already resolved.
    Settled,
    /// Cancelled by the cell (`Task.cancel()`, or `asyncio.run` ending):
    /// its call is abandoned and an answer to it changes nothing.
    Cancelled,
    /// No such future in the running cell.
    Unknown,
}

const REUSED: &str = "cannot reuse already awaited coroutine";

/// A gather's `AUX` for `_aio_first`: it settles with its first child done.
const FIRST: u64 = 2;

/// A future's `AUX` when the loop settles it (`asyncio.Future`), not a host.
#[cfg(feature = "asyncio")]
const LOOP_FUTURE: u64 = u64::MAX;

/// A future's `AUX` when the scheduler settles it once the loop stops
/// (`_aio_stop`); it cannot be cancelled.
const STOP_FUTURE: u64 = u64::MAX - 1;

/// The function attribute `types.coroutine` sets on a generator function
/// (CPython sets `CO_ITERABLE_COROUTINE` on its code).
pub(crate) const ITERABLE_COROUTINE: &str = "__mm_iterable_coroutine__";

impl Vm {
    // ------------------------------------------------------- objects

    /// A coroutine object for `func` whose `nregs` registers were bound at
    /// stack index `top`.
    pub(crate) fn new_coro(
        &mut self,
        func: Value,
        top: usize,
        nregs: usize,
    ) -> Result<Value, Raised> {
        let buf = self.alloc_buf(nregs.max(1))?;
        for i in 0..nregs {
            let v = self.stack[top + i];
            self.set_slot_hot(buf, u32::try_from(i).unwrap_or(0), v);
        }
        let r = {
            let res = self.heap.alloc_init(
                ty::CORO,
                0,
                &[
                    func,
                    Value::from_ref(buf, ty::BUF as u8),
                    Value::NONE,
                    Value::NONE,
                    Value::NONE,
                    Value::NONE,
                    Value::NONE,
                ],
                &[coro::CREATED, 0, coro::SEND],
            );
            self.hx_hot(res)
        }?;
        let c = Value::from_ref(r, ty::CORO as u8);
        self.note_fresh(c)?;
        Ok(c)
    }

    /// Remembers coroutine `c` of the cell's own code until it starts, so
    /// the end of the cell can report it as never awaited
    /// ([`Vm::warn_unawaited`]). `fresh` holds `c, name` pairs (its
    /// `__qualname__` when created, as CPython's `cr_qualname`), and a bare
    /// name once `c` died unstarted: the collector does not keep `c` alive
    /// ([`Vm::sweep_fresh`]). Started or scheduled ones are dropped when the
    /// list would grow.
    #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
    fn note_fresh(&mut self, c: Value) -> Result<(), Raised> {
        let f = self.vslot_hot(c, coro::FUNC);
        // Only the cell's own: the prelude's (`anext()`) and the bundled
        // modules' (probes for the coroutine type) are the engine's.
        let ci = self.vslot_raw_of(f, fn_slot::CODE);
        if usize::try_from(ci)
            .ok()
            .and_then(|i| self.codes.get(i))
            .is_none_or(|c| c.gslot != crate::vm::GSLOT_USER)
        {
            return Ok(());
        }
        let q = self.vslot_hot(f, fn_slot::QUALNAME);
        if !Vm::is_str(q) {
            return Ok(());
        }
        if self.fresh.capacity() - self.fresh.len() < 2 {
            self.prune_fresh();
        }
        if self.fresh.capacity() - self.fresh.len() < 2 {
            let extra = self.fresh.len().max(4);
            self.reserve_execution((extra * size_of::<Value>()) as u64)?;
            let grown = self.fresh.try_reserve_exact(extra);
            self.reserve_execution(0)?;
            grown.map_err(|_| self.err(T::MemoryError, "coroutine list allocation failed"))?;
        }
        self.fresh.extend_from_slice(&[c, q]);
        Ok(())
    }

    /// Has coroutine `c` neither started nor been wrapped in a task?
    fn unawaited(&self, c: Value) -> bool {
        self.raw_of_hot(c, coro::STATE) & (3 | coro::SCHEDULED) == coro::CREATED
    }

    /// Drops the coroutines of `fresh` that started or were scheduled, with
    /// their names; bare names stay.
    pub(crate) fn prune_fresh(&mut self) {
        let mut fresh = core::mem::take(&mut self.fresh);
        let (mut w, mut i) = (0, 0);
        while i < fresh.len() {
            let v = fresh[i];
            if !v.is_obj_kind(ty::CORO as u8) {
                fresh[w] = v;
                w += 1;
                i += 1;
                continue;
            }
            if let Some(&name) = fresh.get(i + 1)
                && self.unawaited(v)
            {
                fresh[w] = v;
                fresh[w + 1] = name;
                w += 2;
            }
            i += 2;
        }
        fresh.truncate(w);
        self.fresh = fresh;
    }

    /// The collector's weak pass over `fresh` ([`Vm::note_fresh`]), once
    /// everything else is marked: a coroutine nothing else keeps alive
    /// leaves its name behind, each name object once (the first), and the
    /// names are kept alive.
    #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
    pub(crate) fn sweep_fresh(fresh: &mut Vec<Value>, t: &mut mm_core::Tracer<'_>) {
        let (mut w, mut i) = (0, 0);
        let mut bare = 0;
        while i < fresh.len() {
            let v = fresh[i];
            if !v.is_obj_kind(ty::CORO as u8) {
                fresh[w] = v;
                (w, i, bare) = (w + 1, i + 1, bare + 1);
                continue;
            }
            let Some(&name) = fresh.get(i + 1) else { break };
            if t.marked(v) {
                fresh[w] = v;
                fresh[w + 1] = name;
                w += 2;
            } else {
                fresh[w] = name;
                (w, bare) = (w + 1, bare + 1);
            }
            i += 2;
        }
        fresh.truncate(w);
        // Bare names: one entry per name object, the first.
        let mut names: Vec<(u64, usize)> = Vec::new();
        if names.try_reserve_exact(bare).is_ok() {
            let mut i = 0;
            while i < fresh.len() {
                if fresh[i].is_obj_kind(ty::CORO as u8) {
                    i += 2;
                } else {
                    names.push((fresh[i].to_bits(), i));
                    i += 1;
                }
            }
            crate::ops::heap_sort_by(&mut names, |a, b| a < b);
            let mut dup = false;
            for k in 1..names.len() {
                if names[k].0 == names[k - 1].0 {
                    fresh[names[k].1] = Value::UNDEFINED;
                    dup = true;
                }
            }
            if dup {
                fresh.retain(|&v| v != Value::UNDEFINED);
            }
        }
        for &v in fresh.iter() {
            if !v.is_obj_kind(ty::CORO as u8) {
                t.value(v);
            }
        }
    }

    /// Writes CPython's `RuntimeWarning` for each coroutine the ending
    /// cell created and never started, and each host future it never
    /// awaited, to stderr: `<sys>:0: RuntimeWarning: coroutine 'f' was
    /// never awaited`, once per name, in creation order. A scan at the end
    /// of the cell, not a finalizer, so it does not depend on GC timing
    /// (docs/spec.md §1): coroutines collected unstarted left their names.
    #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
    pub(crate) fn warn_unawaited(&mut self) {
        // (creation order, name): name values first, deduplicated before
        // any text is compared or copied.
        let mut found: Vec<(u64, Value)> = Vec::new();
        let mut i = 0;
        while i < self.fresh.len() {
            let v = self.fresh[i];
            if !v.is_obj_kind(ty::CORO as u8) {
                found.push((i as u64, v));
                i += 1;
                continue;
            }
            if let Some(&name) = self.fresh.get(i + 1)
                && self.unawaited(v)
            {
                found.push((i as u64, name));
            }
            i += 2;
        }
        let base = self.fresh.len() as u64;
        let mut hosts: Vec<(u64, String)> = Vec::new();
        for &(id, f) in &self.futures {
            if self.raw_of_hot(f, aw::STATE) != aw::PENDING || self.raw_of_hot(f, aw::AWAITED) != 0
            {
                continue;
            }
            let name = self
                .call_log
                .binary_search_by_key(&id, |e| e.0)
                .ok()
                .and_then(|i| self.host_ids.get(self.call_log[i].1 as usize))
                .map(|n| self.host_bare_name(n));
            if let Some(n) = name {
                hosts.push((base + id, n));
            }
        }
        found.retain(|&(_, q)| Vm::is_str(q));
        // The first of each name value, then the first of each text.
        crate::ops::heap_sort_by(&mut found, |a, b| {
            (a.1.to_bits(), a.0) < (b.1.to_bits(), b.0)
        });
        found.dedup_by_key(|e| e.1.to_bits());
        let mut named: Vec<(u64, &str)> = found
            .iter()
            .map(|&(k, q)| (k, self.str_ref_hot(q)))
            .collect();
        named.extend(hosts.iter().map(|(k, n)| (*k, n.as_str())));
        crate::ops::heap_sort_by(&mut named, |a, b| (a.1, a.0) < (b.1, b.0));
        named.dedup_by_key(|e| e.1);
        crate::ops::heap_sort_by(&mut named, |a, b| a.0 < b.0);
        let mut text = String::new();
        for (_, n) in &named {
            text.push_str("<sys>:0: RuntimeWarning: coroutine '");
            text.push_str(n);
            text.push_str("' was never awaited\n");
        }
        if !text.is_empty() {
            self.write_out(crate::vm::Stream::Stderr, &text);
        }
    }

    /// A new awaitable of kind `kind`.
    pub(crate) fn new_awaitable(
        &mut self,
        kind: u16,
        result: Value,
        extra: Value,
        state: u64,
        aux: u64,
    ) -> Result<Value, Raised> {
        let r = {
            let res = self.heap.alloc_init(
                kind,
                0,
                &[result, extra, Value::NONE],
                &[state, 0, aux, 0, 0],
            );
            self.hx_hot(res)
        }?;
        Ok(Value::from_ref(r, kind as u8))
    }

    /// [`Vm::raw_of_hot`], out of line (`tests/hot_accessors.rs`).
    #[inline(never)]
    fn raw_of(&self, v: Value, i: u32) -> u64 {
        self.raw_of_hot(v, i)
    }

    fn raw_of_hot(&self, v: Value, i: u32) -> u64 {
        v.as_obj().map_or(0, |r| self.raw_hot(r, i))
    }

    /// [`Vm::set_raw_of_hot`], out of line (`tests/hot_accessors.rs`).
    #[inline(never)]
    fn set_raw_of(&mut self, v: Value, i: u32, w: u64) {
        self.set_raw_of_hot(v, i, w);
    }

    fn set_raw_of_hot(&mut self, v: Value, i: u32, w: u64) {
        if let Some(r) = v.as_obj() {
            self.set_raw_hot(r, i, w);
        }
    }

    /// Settles awaitable `v` with a result or an exception and queues what
    /// waits on it: tasks blocked on it, then gathers it is a child of.
    pub(crate) fn settle(&mut self, v: Value, ok: bool, result: Value) {
        self.settle_as(v, if ok { aw::DONE } else { aw::FAILED }, result);
    }

    /// Settles `v` cancelled with message `msg`; `exc` is the
    /// `CancelledError` that ended a task (its first retrieval raises it),
    /// or `None`.
    #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
    pub(crate) fn settle_cancelled(&mut self, v: Value, msg: Value, exc: Value) {
        let held = exc.is_obj_kind(ty::EXC as u8);
        self.set_raw_of(v, aw::MUST_CANCEL, if held { aw::HELD } else { 0 });
        self.settle_as(v, aw::CANCELLED, if held { exc } else { msg });
    }

    /// The cancel message of cancelled `a` (`_cancel_message`).
    fn cancel_msg(&self, a: Value) -> Value {
        if self.raw_of(a, aw::MUST_CANCEL) == aw::HELD {
            Value::NONE
        } else {
            self.vslot(a, aw::RESULT)
        }
    }

    fn settle_as(&mut self, v: Value, state: u64, result: Value) {
        self.seq += 1;
        let was_pending = self.raw_of_hot(v, aw::STATE) == aw::PENDING;
        if let Some(r) = v.as_obj() {
            self.set_slot_hot(r, aw::RESULT, result);
            self.set_raw_hot(r, aw::STATE, state);
            self.set_raw_hot(r, aw::SEQ, self.seq);
        }
        // Tasks, futures and gathers list their waiters in order; tasks
        // blocked on anything else are found in `blocked`.
        if !matches!(
            v.obj_kind().map(u16::from),
            Some(ty::TASK | ty::FUTURE | ty::GATHER)
        ) {
            // Over a list no more than half holes: a cancel's budget
            // counts this scan by the tasks blocked (`Vm::settle_work`),
            // and wakes alone never pack the list.
            if self.blocked_holes > self.blocked.len() - self.blocked_holes {
                self.pack_blocked();
            }
            for k in 0..self.blocked.len() {
                let t = self.blocked[k];
                if matches!(self.tasks.get(t), Some(Some(task)) if task.awaiting == v)
                    && self.unblock(t)
                {
                    self.ready.push_back(Ready::Task(t));
                }
            }
        }
        // Gathers keep their own counts, and each notification names its
        // child, so their callbacks need not rescan every child.
        let waiters = self.vslot_hot(v, aw::WAITERS);
        if let Some(r) = v.as_obj() {
            self.set_slot_hot(r, aw::WAITERS, Value::NONE);
        }
        // Whether the callbacks' room is in `cb_slots`: not a loop future's
        // (reserved by its settle), nor an earlier cell's task or host
        // future's (released with that cell; a cancel reserves it). A gather
        // settles only in its own cell: an earlier cell's `FIRST` waiter
        // future holds task wakeups only.
        let held = !self.pays_at_settle(v) && (v.is_obj_kind(ty::GATHER as u8) || self.live(v));
        // Slot numbers can be reused. Only the latest registration of a
        // task blocked on v is its wakeup; an older one might still be in
        // the buffer. Claim that wakeup last-to-first, then put the new
        // queue entries back in registration order.
        let start = self.ready.len();
        for k in (0..self.waiter_count(waiters)).rev() {
            let g = self.waiter_at(waiters, k);
            if let Some(t) = g.as_int().and_then(|t| usize::try_from(t).ok()) {
                // A task's wakeup.
                if matches!(self.tasks.get(t), Some(Some(task)) if task.awaiting == v)
                    && self.unblock(t)
                {
                    self.ready.push_back(Ready::Task(t));
                }
                continue;
            }
            if !g.is_obj_kind(ty::GATHER as u8) {
                // `add_done_callback`: called soon with the awaitable.
                if held {
                    self.cb_slots = self.cb_slots.saturating_sub(1);
                }
                self.ready.push_back(Ready::Call(g, v));
                self.callback_queue += 1;
                continue;
            }
            // Gathers dropped with their `asyncio.run` no longer listen.
            // A child notifies once: at the settle that ends its pending.
            if !was_pending || !self.tracked(g) {
                continue;
            }
            let left = self.raw_of_hot(g, aw::LEFT);
            // The notification it waited for is now in the queue.
            if left & aw::LEFT_MASK != 0 {
                self.set_raw_of_hot(g, aw::LEFT, left - 1);
                self.callback_queue -= 1;
            }
            self.ready.push_back(Ready::Gather(g, v));
            self.callback_queue += 1;
        }
        let end = self.ready.len();
        for i in 0..(end - start) / 2 {
            self.ready.swap(start + i, end - 1 - i);
        }
    }

    /// Marks a coroutine frame finished (returned or raised out).
    pub(crate) fn coro_finished(&mut self, c: Value) {
        let state = self.raw_of_hot(c, coro::STATE);
        self.set_raw_of_hot(c, coro::STATE, coro::DONE);
        if let Some(r) = c.as_obj() {
            let _ = self.heap.set_values(
                r,
                coro::ARGS,
                &[Value::NONE, Value::NONE, Value::NONE, Value::NONE],
            );
        }
        if state & coro::RUN_ROOT != 0 {
            // `asyncio.run` ends its loop: tasks it left behind go too.
            self.drop_other_tasks();
            self.loop_flags &= !crate::vm::LOOP_IN_RUN;
            self.run_handled = Value::NONE;
            // CPython's `Runner.close()` runs `shutdown_asyncgens()` and
            // `shutdown_default_executor()` as two more numbered tasks.
            if state & coro::RUN_OPEN == 0 {
                self.task_counter += 2;
            }
        }
    }

    /// Coroutine `c`, run by `send()`, `throw()` or `close()`, returned or
    /// raised out of that frame: its caller's handled exception is back,
    /// as a generator's return restores it (`Vm::gen_leave`), and the
    /// coroutine keeps none.
    pub(crate) fn coro_left(&mut self, c: Value) {
        if let Some(r) = c.as_obj()
            && self.slot(r, coro::HANDLED) != Value::NONE
        {
            self.handled = self.slot(r, coro::OUTER);
            let _ = self
                .heap
                .set_values(r, coro::HANDLED, &[Value::NONE, Value::NONE]);
        }
    }

    /// The state of coroutine `c` (`asyncio.sleep()` objects count as
    /// coroutines once awaited).
    #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
    pub(crate) fn coro_state(&self, c: Value) -> u64 {
        if c.is_obj_kind(ty::CORO as u8) {
            self.raw_of(c, coro::STATE) & 3
        } else if self.raw_of(c, aw::STATE) == aw::PENDING {
            coro::CREATED
        } else {
            coro::DONE
        }
    }

    /// `send()` and `throw()` need a coroutine that has not started, or
    /// one that a `send()` suspended.
    #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
    pub(crate) fn coro_unstarted(&mut self, c: Value) -> Result<(), Raised> {
        match self.coro_state(c) {
            coro::CREATED => Ok(()),
            coro::SUSPENDED if self.vslot(c, coro::AWAITER) == Value::NONE => Ok(()),
            coro::SUSPENDED => Err(self.err(T::RuntimeError, "coroutine is being awaited already")),
            coro::RUNNING => Err(self.err(T::ValueError, "coroutine already executing")),
            _ => Err(self.err(T::RuntimeError, REUSED)),
        }
    }

    /// `close()` of a coroutine that is not running: it is done.
    #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
    pub(crate) fn coro_close(&mut self, c: Value) {
        if let Some(r) = c.as_obj()
            && c.is_obj_kind(ty::CORO as u8)
        {
            self.set_raw(r, coro::STATE, coro::DONE);
            self.set_slot(r, coro::ARGS, Value::NONE);
        }
    }

    /// What `send()`/`throw()` deliver to suspended coroutine `c` when it
    /// resumes.
    pub(crate) fn coro_deliver(&mut self, c: Value, v: Value, mode: u64) {
        if let Some(r) = c.as_obj() {
            self.set_slot(r, coro::SENT, v);
            self.set_raw(r, coro::MODE, mode);
        }
    }

    /// Takes what coroutine `c` (or a plain frame, `None`) was sent.
    #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
    fn coro_take_sent(&mut self, c: Value) -> (Value, u64) {
        let Some(r) = c.as_obj().filter(|_| c.is_obj_kind(ty::CORO as u8)) else {
            return (Value::NONE, coro::SEND);
        };
        let v = self.slot(r, coro::SENT);
        let mode = self.raw(r, coro::MODE);
        self.set_slot(r, coro::SENT, Value::NONE);
        self.set_raw(r, coro::MODE, coro::SEND);
        (v, mode)
    }

    /// Pushes the frame of coroutine `c` above the running frame, as a call
    /// from `call_pc` returning to `ret_pc` through `cont` (`Cont::None` for
    /// an `await`, `Cont::CoroSend` for `send()`).
    pub(crate) fn start_coro_frame(
        &mut self,
        st: &mut State,
        c: Value,
        call_pc: usize,
        ret_pc: usize,
        cont: crate::cont::Cont,
    ) -> Result<(), Raised> {
        let state = self.raw_of_hot(c, coro::STATE) & 3;
        let resume = state == coro::SUSPENDED
            && if cont == crate::cont::Cont::CoroSend {
                self.vslot_hot(c, coro::AWAITER) == Value::NONE
            } else {
                self.vslot_hot(c, coro::AWAITER) == st.coro
            };
        if state != coro::CREATED && !resume {
            let msg = if state == coro::RUNNING || state == coro::SUSPENDED {
                "coroutine is being awaited already"
            } else {
                REUSED
            };
            return Err(self.err(T::RuntimeError, msg));
        }
        if resume && cont != crate::cont::Cont::CoroSend {
            // A re-run `AWAIT` passes on what its own coroutine was sent.
            let (v, mode) = self.coro_take_sent(st.coro);
            self.coro_deliver(c, v, mode);
        }
        let func = self.vslot_hot(c, coro::FUNC);
        let cidx = u32::try_from(self.vslot_raw_of(func, fn_slot::CODE)).unwrap_or(0);
        let Some(code) = self.codes.get(cidx as usize).cloned() else {
            return Err(self.err(T::SystemError, "bad code index"));
        };
        let nregs = code.code.nregs as usize;
        let new_base = st.base + self.codes[st.ci as usize].code.nregs as usize + HDR;
        self.ensure_stack(new_base + nregs + 1)?;
        self.load_coro_args(c, new_base, nregs);
        self.enter_frame(st, func, cidx, new_base, call_pc, ret_pc, cont)?;
        let keep = self.raw_of_hot(c, coro::STATE) & coro::RUN_BITS;
        self.set_raw_of_hot(c, coro::STATE, coro::RUNNING | keep);
        st.coro = c;
        if resume && let Some(r) = c.as_obj() {
            // Back at its `AWAIT`, which steps what it waits on.
            st.pc = usize::try_from(self.raw_hot(r, coro::PC)).unwrap_or(0);
            st.acc = self.slot_hot(r, coro::AWAITED);
            let _ = self
                .heap
                .set_values(r, coro::AWAITED, &[Value::NONE, Value::NONE]);
            let handled = self.slot_hot(r, coro::HANDLED);
            if handled != Value::NONE {
                self.own_handler_resume(r, coro::HANDLED, handled);
            }
        }
        Ok(())
    }

    /// The iterator an `__await__()` (or a `types.coroutine` generator)
    /// gave, which an `AWAIT` steps until it returns.
    #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
    pub(crate) fn new_delegation(&mut self, it: Value) -> Result<Value, Raised> {
        let b = self.alloc_buf(1)?;
        self.set_slot_hot(b, 0, it);
        Ok(Value::from_ref(b, ty::BUF as u8))
    }

    /// The delegation's iterator yielded `y` to the `AWAIT` at `pc` of
    /// `st`, which steps delegation `w`. Under `send()`, every coroutine
    /// from `st` to the one `send()` runs saves its frame (as a generator
    /// does at a `yield`) and `send()` returns `y`. In a task (`asyncio`),
    /// `None` yields to the event loop and any other value is an error.
    pub(crate) fn await_yield(
        &mut self,
        st: &mut State,
        y: Value,
        w: Value,
        pc: usize,
    ) -> Result<crate::interp::Landed, Raised> {
        use crate::cont::Cont;
        use crate::interp::Landed;
        let frames = crate::interp::walk_frames(
            &self.stack,
            &self.specials,
            &self.codes,
            st.base,
            self.depth,
        )
        .unwrap_or_default();
        self.burn(frames.len() as u64);
        // The frames up to the one `send()` entered, when it drives this.
        let mut levels = 0;
        let mut driven = false;
        let mut c = st.coro;
        for f in &frames {
            if !c.is_obj_kind(ty::CORO as u8) {
                break;
            }
            levels += 1;
            let fc = f
                .cont
                .checked_sub(1)
                .and_then(|i| self.conts.get(i as usize).copied().flatten())
                .unwrap_or(Cont::None);
            if fc == Cont::CoroSend {
                driven = true;
                break;
            }
            let awaits = self.codes.get(f.code as usize).is_some_and(|code| {
                code.code
                    .bytecode
                    .get(f.call_pc as usize)
                    .is_some_and(|&w| crate::interp::op_of(w) == mm_compile::bytecode::op::AWAIT)
            });
            if fc != Cont::None || !awaits {
                break;
            }
            c = f.coro;
        }
        st.pc = pc;
        st.acc = w;
        if !driven {
            if y == Value::NONE {
                return Ok(Landed::Block(w));
            }
            self.prelude_request("_bad_yield", alloc::vec![y], Cont::None)?;
            return self.land(st, pc, pc + 1, Cont::None);
        }
        // Every coroutine up to the one `send()` runs gets the buffer its
        // suspension copies its registers to before a frame is left (one
        // has none before its first suspension, and a collection drops it,
        // `Vm::clear_running`): a buffer refused after the first frame had
        // gone raised in a frame `st` no longer was (`Vm::landing`).
        // Refused here, nothing has changed.
        let mut level = (st.coro, st.ci);
        for f in &frames[..levels] {
            let nregs = self.codes[level.1 as usize].code.nregs as usize;
            let buf = self.vslot_hot(level.0, coro::ARGS);
            let len = buf
                .as_obj()
                .filter(|_| buf.is_obj_kind(ty::BUF as u8))
                .and_then(|b| self.heap.header(b).ok())
                .map_or(0, |h| h.nvals() as usize);
            if len < nregs
                && let Some(r) = level.0.as_obj()
            {
                let b = self.alloc_buf(nregs.max(1))?;
                self.set_slot_hot(r, coro::ARGS, Value::from_ref(b, ty::BUF as u8));
            }
            level = (f.coro, f.code);
        }
        let mut awaited = w;
        let mut at = pc;
        loop {
            let c = st.coro;
            // The saved copy is cleared now, not by a collection: this path
            // (a coroutine driven by `send`) runs once per suspension.
            let code = &self.codes[st.ci as usize].code;
            let nregs = code.nregs as usize;
            let pcs = crate::generator::suspend_pcs(code, at as u64);
            crate::interp::clear_frame(&mut self.stack, code, st.base, st.base + nregs, &pcs);
            // Reserved above.
            let buf = self.vslot_hot(c, coro::ARGS);
            let Some(r) = c.as_obj() else {
                return Err(self.err(T::SystemError, "micro-monty: bad coroutine"));
            };
            if let (Some(b), Some(regs)) = (buf.as_obj(), self.stack.get(st.base..st.base + nregs))
            {
                let regs: Vec<Value> = regs.to_vec();
                let _ = self.heap.set_values(b, 0, &regs);
            }
            let _ = self.heap.set_values(r, coro::ARGS, &[buf, awaited]);
            let keep = self.raw_hot(r, coro::STATE) & coro::RUN_BITS;
            self.set_raw_hot(r, coro::STATE, coro::SUSPENDED | keep);
            self.set_raw_hot(r, coro::PC, at as u64);
            let Some(fr) = self.pop_frame(st) else {
                return Err(self.err(T::SystemError, "micro-monty: lost send() frame"));
            };
            // Each level inside a handler of its own keeps its handled
            // exception, so below it the one handled is its awaiter's.
            if self.slot_hot(r, coro::HANDLED) != Value::NONE {
                self.own_handler_suspend(r, coro::HANDLED);
            }
            if self.cont_take(fr.cont) == Cont::CoroSend {
                st.acc = y;
                st.pc = fr.ret_pc as usize;
                return Ok(Landed::Go);
            }
            self.set_slot_hot(r, coro::AWAITER, st.coro);
            awaited = c;
            at = fr.call_pc as usize;
        }
    }

    pub(crate) fn vslot_raw_of(&self, v: Value, i: u32) -> u64 {
        v.as_obj().map_or(0, |r| self.raw_hot(r, i))
    }

    /// Copies bound arguments to `base`; keep the original until frame
    /// admission succeeds, so an allocation failure does not consume them.
    fn load_coro_args(&mut self, c: Value, base: usize, nregs: usize) {
        let buf = self.vslot_hot(c, coro::ARGS);
        for i in 0..nregs {
            self.stack[base + i] = self.vslot_hot(buf, u32::try_from(i).unwrap_or(0));
        }
    }

    // ---------------------------------------------------------- await

    /// One `AWAIT` of `aw` at `pc`.
    #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
    pub(crate) fn await_step(
        &mut self,
        st: &mut State,
        a: Value,
        pc: usize,
    ) -> Result<Awaited, Raised> {
        let Some(kind) = a.obj_kind().map(u16::from) else {
            return Err(self.not_awaitable(a));
        };
        match kind {
            ty::CORO => {
                self.start_coro_frame(st, a, pc, pc + 1, crate::cont::Cont::None)?;
                Ok(Awaited::Frame)
            }
            // A delegation: step its iterator (send, or throw).
            ty::BUF => {
                let it = self.vslot_hot(a, 0);
                let (v, mode) = self.coro_take_sent(st.coro);
                let name = if mode == coro::THROW {
                    "_await_throw"
                } else {
                    "_await_step"
                };
                self.prelude_request(name, alloc::vec![it, v], crate::cont::Cont::AwaitStep(a))?;
                Ok(Awaited::Request)
            }
            ty::GEN if self.iterable_coroutine(a) => {
                let w = self.new_delegation(a)?;
                Ok(Awaited::Retry(w))
            }
            ty::AGENOP => Ok(match self.agen_await(a)? {
                Some(v) => Awaited::Value(v),
                None => Awaited::Request,
            }),
            ty::TASK | ty::FUTURE | ty::GATHER => {
                if kind == ty::FUTURE {
                    self.set_raw_of_hot(a, aw::AWAITED, 1);
                }
                match self.raw_of_hot(a, aw::STATE) {
                    // A task that awaits itself yields to the loop, which
                    // raises in it at its next step (`Vm::block_current`).
                    aw::PENDING if self.awaits_itself(a) => Ok(Awaited::Block),
                    aw::PENDING => {
                        // The task's wakeup, in line with the other done
                        // callbacks.
                        let me =
                            Value::int(i64::try_from(self.cur).unwrap_or(0)).unwrap_or(Value::NONE);
                        self.add_waiter(a, me)?;
                        Ok(Awaited::Block)
                    }
                    aw::DONE => Ok(Awaited::Value(self.vslot_hot(a, aw::RESULT))),
                    aw::CANCELLED => {
                        self.exc = self.cancelled_error(a)?;
                        Err(Raised)
                    }
                    _ => {
                        self.retrieved(a);
                        self.exc = self.vslot_hot(a, aw::RESULT);
                        Err(Raised)
                    }
                }
            }
            ty::SLEEP => match self.raw_of_hot(a, aw::STATE) {
                aw::PENDING => {
                    self.arm_sleep(a);
                    Ok(Awaited::Block)
                }
                aw::ARMED if self.sleep_due(a) => {
                    self.set_raw_of_hot(a, aw::STATE, aw::SPENT);
                    Ok(Awaited::Value(self.vslot_hot(a, aw::RESULT)))
                }
                aw::ARMED => Ok(Awaited::Block),
                aw::CANCELLED => {
                    self.exc = self.cancelled_error(a)?;
                    self.set_raw_of_hot(a, aw::STATE, aw::SPENT);
                    Err(Raised)
                }
                _ => Err(self.err(T::RuntimeError, REUSED)),
            },
            _ => {
                let key = self.intern_hot("__await__")?;
                let cls = self.class_of(a);
                match self.class_lookup(cls, key) {
                    Some(m) => {
                        let n = self.call_dunder(
                            m,
                            a,
                            &[],
                            crate::cont::Cont::Check(crate::cont::check::AWAIT),
                        )?;
                        self.next_request(n)?;
                        Ok(Awaited::RequestRetry)
                    }
                    _ => Err(self.not_awaitable(a)),
                }
            }
        }
    }

    /// Was generator `gn` made by a function `types.coroutine` marked?
    #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
    pub(crate) fn iterable_coroutine(&mut self, gn: Value) -> bool {
        let f = self.vslot(gn, crate::generator::g::FUNC);
        let d = self.vslot(f, fn_slot::DICT);
        if !d.is_obj_kind(ty::DICT as u8) {
            return false;
        }
        let Ok(k) = self.intern(ITERABLE_COROUTINE) else {
            return false;
        };
        matches!(self.dict_get(d, k), Ok(Some(v)) if v == Value::TRUE)
    }

    #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
    fn not_awaitable(&mut self, a: Value) -> Raised {
        let msg = format!("'{}' object can't be awaited", self.type_name(a));
        self.err(T::TypeError, &msg)
    }

    fn sleep_due(&self, a: Value) -> bool {
        self.vclock >= f64::from_bits(self.raw_of_hot(a, aw::AUX))
    }

    // ------------------------------------------------------ scheduler

    /// Pending child notifications plus callbacks already queued. Duplicate
    /// child references overestimate pending notifications, never undercount.
    #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
    fn callback_slots(&self) -> usize {
        debug_assert_eq!(
            self.callback_queue,
            self.count_callback_queue(),
            "callback_queue out of step"
        );
        self.callback_queue.saturating_add(self.cb_slots)
    }

    /// The run queue's room beyond what a reservation needs for the tasks
    /// and the callbacks that hold room (`callback_slots`): what callbacks
    /// registered on pending loop futures grew it by, less what their
    /// settles have taken. Part of the image, and reserved again by the
    /// restore, so a restored session has the room the running one had.
    pub(crate) fn queue_spare(&self) -> usize {
        self.ready
            .capacity()
            .saturating_sub(self.tasks.capacity().saturating_add(self.callback_slots()))
    }

    /// [`Vm::callback_queue`] counted: the queue's entries that are not a
    /// task's step, and the notifications each tracked gather still waits
    /// for (one per distinct pending child, `LEFT`'s low count).
    pub(crate) fn count_callback_queue(&self) -> usize {
        let queued = self
            .ready
            .iter()
            .filter(|r| !matches!(r, Ready::Task(_)))
            .count();
        self.gathers.iter().fold(queued, |n, &g| {
            n.saturating_add((self.raw_of(g, aw::LEFT) & aw::LEFT_MASK) as usize)
        })
    }

    /// Takes tracked gather `g` out of `gathers`, with its notifications
    /// from [`Vm::callback_queue`].
    fn untrack_gather(&mut self, g: Value) {
        if !self.tracked(g) {
            return;
        }
        let flags = self.raw_of(g, aw::FAILS);
        let k = (flags >> aw::PLACE_SHIFT) as usize - 1;
        // A hole in its place: no other gather moves (`Vm::track_gather`
        // packs the list).
        self.gathers[k] = Value::UNDEFINED;
        self.gathers_holes += 1;
        self.set_raw_of(g, aw::FAILS, flags & ((1 << aw::PLACE_SHIFT) - 1));
        let left = (self.raw_of(g, aw::LEFT) & aw::LEFT_MASK) as usize;
        self.callback_queue = self.callback_queue.saturating_sub(left);
    }

    /// Whether the scheduler tracks gather `g`: its place names the entry
    /// of `gathers` that is `g`. A place alone is not trusted, so a word
    /// that names another entry (a forged image's) reads as untracked.
    pub(crate) fn tracked(&self, g: Value) -> bool {
        let k = (self.raw_of_hot(g, aw::FAILS) >> aw::PLACE_SHIFT) as usize;
        k != 0 && self.gathers.get(k - 1) == Some(&g)
    }

    /// Writes the place of each tracked gather into it (`on`) or takes
    /// the places out, as an image holds the gathers.
    pub(crate) fn place_gathers(&mut self, on: bool) {
        for k in 0..self.gathers.len() {
            let g = self.gathers[k];
            if !g.is_obj_kind(ty::GATHER as u8) {
                continue;
            }
            let flags = self.raw_of_hot(g, aw::FAILS) & ((1 << aw::PLACE_SHIFT) - 1);
            let place = if on {
                (k as u64 + 1) << aw::PLACE_SHIFT
            } else {
                0
            };
            self.set_raw_of_hot(g, aw::FAILS, flags | place);
        }
    }

    /// Takes the holes out of `gathers`, in order, and gives each gather
    /// its new place.
    pub(crate) fn pack_gathers(&mut self) {
        if self.gathers_holes != 0 {
            self.gathers.retain(|&g| g != Value::UNDEFINED);
            self.gathers_holes = 0;
            self.place_gathers(true);
        }
    }

    /// Tracks gather `g`, last, in room `Vm::gathers_room` reserved.
    #[cfg(feature = "asyncio")]
    fn track_gather(&mut self, g: Value) {
        let place = (self.gathers.len() as u64 + 1) << aw::PLACE_SHIFT;
        self.gathers.push(g);
        let flags = self.raw_of(g, aw::FAILS);
        self.set_raw_of(g, aw::FAILS, flags | place);
    }

    /// The capacity `gathers` needs for one more: the list is packed
    /// first when it is full or its holes outnumber its gathers, so it
    /// grows exactly when its gathers fill it.
    #[cfg(feature = "asyncio")]
    fn gathers_room(&mut self) -> usize {
        let len = self.gathers.len();
        if len == self.gathers.capacity() || self.gathers_holes > len - self.gathers_holes {
            self.pack_gathers();
        }
        if self.gathers.len() == self.gathers.capacity() {
            self.gathers.len().max(2) * 2
        } else {
            self.gathers.capacity()
        }
    }

    /// Stops tracking every gather (the cell or its `asyncio.run` ended).
    fn drop_gathers(&mut self) {
        self.place_gathers(false);
        self.gathers.clear();
        self.gathers_holes = 0;
    }

    /// Reserve all possible wakeups before registering work. Settling and
    /// unwinding can then queue callbacks without a fallible allocation.
    /// Restore uses exact capacities so a same-budget image still fits.
    #[cold]
    pub(crate) fn reserve_scheduler(
        &mut self,
        tasks: usize,
        gathers: usize,
        callbacks: usize,
    ) -> Result<(), Raised> {
        let tasks = tasks.max(self.tasks.capacity());
        let gathers = gathers.max(self.gathers.capacity());
        let blocked = tasks.max(self.blocked.capacity());
        let ready = tasks
            .saturating_add(self.callback_slots())
            .saturating_add(callbacks)
            .max(self.ready.capacity());
        // The queue's term saturates: a cancel may ask for more callbacks
        // than an address space holds (a count a first pass summed, or a
        // forged one), which the reservation refuses.
        let queue =
            ((ready - self.ready.capacity()) as u64).saturating_mul(size_of::<Ready>() as u64);
        let extra = ((tasks - self.tasks.capacity()) as u64 * size_of::<Option<Task>>() as u64
            + (gathers - self.gathers.capacity()) as u64 * size_of::<Value>() as u64
            + (blocked - self.blocked.capacity()) as u64 * size_of::<usize>() as u64)
            .saturating_add(queue);
        if extra == 0 {
            return Ok(());
        }
        self.reserve_execution(extra)?;
        let result = (|| {
            self.gathers
                .try_reserve_exact(gathers - self.gathers.len())?;
            self.blocked
                .try_reserve_exact(blocked - self.blocked.len())?;
            self.ready.try_reserve_exact(ready - self.ready.len())?;
            // Grow the task table last: its spare slots promise that the
            // corresponding blocked and ready slots already exist.
            self.tasks.try_reserve_exact(tasks - self.tasks.len())
        })();
        self.reserve_execution(0)?;
        result.map_err(|_| self.err(T::MemoryError, "scheduler allocation failed"))
    }

    /// Sets up the main task for a new cell.
    pub(crate) fn begin_cell(&mut self) -> Result<(), Raised> {
        self.end_cell();
        self.reserve_scheduler(1, 0, 0)?;
        self.tasks.push(Some(Task::new(Value::NONE, Value::NONE)));
        self.cur = 0;
        Ok(())
    }

    /// The prelude global that lists the async generators first iterated
    /// inside `asyncio.run` (CPython's `loop._asyncgens`, which its
    /// `shutdown_asyncgens()` closes): a list, or `None`. In the prelude's
    /// globals, so it is in an image with no field of its own.
    #[cfg(feature = "asyncio")]
    const ASYNCGENS: &'static str = "_aio_asyncgens";

    /// An async generator takes its first step inside `asyncio.run`: the
    /// run closes it when it ends, if it has not finished by then. There
    /// is no finalizer to close one that became garbage earlier, as
    /// CPython's does, so the list holds it until the run ends; each time
    /// its length reaches a power of two, the finished ones leave it.
    #[cfg(feature = "asyncio")]
    #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
    pub(crate) fn track_asyncgen(&mut self, agen: Value) -> Result<(), Raised> {
        let key = self.intern_hot(Self::ASYNCGENS)?;
        let list = match self.dict_lookup_str(self.prelude_globals, key) {
            Some(l) if l.is_obj_kind(ty::LIST as u8) => l,
            _ => {
                let l = self.new_list(&[])?;
                self.dict_set(self.prelude_globals, key, l)?;
                l
            }
        };
        self.asyncgens_listed = true;
        let n = self.list_len_hot(list);
        if n >= 32 && n.is_power_of_two() {
            self.burn(n as u64);
            let live = self.unfinished_asyncgens(list);
            self.list_set_items(list, &live)?;
        }
        self.list_push(list, agen)
    }

    /// The generators of `list` the run still has to close: not finished,
    /// and not suspended at a yield no handler covers. Closing one of those
    /// runs none of its code (no `finally`, `except` or `async with` is
    /// open), so the run does not hold it, or what its frame holds, to its
    /// end.
    #[cfg(feature = "asyncio")]
    fn unfinished_asyncgens(&self, list: Value) -> Vec<Value> {
        use crate::generator::g;
        let mut live = self.list_items(list);
        live.retain(|&gn| match self.gen_state(gn) {
            g::DONE => false,
            // Suspended at a yield no handler covers: a close would run
            // none of its code, so the run need not hold it.
            g::SUSPENDED => {
                let ypc = self.gen_raw(gn, g::YPC) & !g::CLEARED;
                let code = usize::try_from(self.gen_raw(gn, g::CODE))
                    .ok()
                    .and_then(|ci| self.codes.get(ci));
                match (code, u32::try_from(ypc)) {
                    (Some(c), Ok(pc)) => c.code.handler(pc).is_some(),
                    _ => true,
                }
            }
            _ => true,
        });
        live
    }

    /// `_mm.aio_asyncgens()`: the tracked async generators that have not
    /// finished, as a list the run then closes, or `None` for none; the
    /// run tracks none after it.
    #[cfg(feature = "asyncio")]
    #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
    pub(crate) fn take_asyncgens(&mut self) -> Result<Value, Raised> {
        let Some(key) = self.find_interned(Self::ASYNCGENS) else {
            return Ok(Value::NONE);
        };
        let Some(list) = self
            .dict_lookup_str(self.prelude_globals, key)
            .filter(|l| l.is_obj_kind(ty::LIST as u8))
        else {
            return Ok(Value::NONE);
        };
        self.dict_set(self.prelude_globals, key, Value::NONE)?;
        self.burn(self.list_len(list) as u64);
        let live = self.unfinished_asyncgens(list);
        if live.is_empty() {
            return Ok(Value::NONE);
        }
        self.list_set_items(list, &live)?;
        Ok(list)
    }

    /// Is anything in the run queue (with every task done: callbacks)?
    #[cfg(feature = "asyncio")]
    pub(crate) fn callbacks_queued(&self) -> bool {
        !self.ready.is_empty()
    }

    /// Ends the cell that completed or raised: reports what it never
    /// awaited, then drops its frames, tasks and futures.
    pub(crate) fn finish_cell(&mut self) {
        self.warn_unawaited();
        self.end_cell();
    }

    /// Forgets the containers a `repr` was rendering when its cell was
    /// ended where it stood: no `finally` of the prelude's walk ran, so
    /// they stayed in its `_REPR_IDS`, held for good and shown as `[...]`
    /// by every later `repr`. A cell that completes or raises unwinds the
    /// walk and leaves none. Emptied in place, allocating nothing.
    pub(crate) fn drop_repr_marks(&mut self) {
        let marks = self
            .find_interned("_REPR_IDS")
            .and_then(|k| self.dict_get_str(self.prelude_globals, k));
        if let Some(d) = marks {
            self.dict_empty(d);
            // Released without allocating (`Heap::idle`).
            self.heap.idle_collected = false;
        }
    }

    /// Drops every frame, task and future of the running cell; host calls
    /// it left unanswered are abandoned.
    pub(crate) fn end_cell(&mut self) {
        self.check_parked_bytes();
        self.abandon_calls();
        // No run of a later cell closes an async generator of this one: not
        // those of a run the cell did not finish, of a loop it left open
        // (`run_until_complete()`, a `Runner` not closed), or first stepped
        // while a run's end was closing the others.
        if self.asyncgens_listed {
            self.asyncgens_listed = false;
            #[cfg(feature = "asyncio")]
            if let Some(key) = self.find_interned(Self::ASYNCGENS) {
                let _ = self.dict_set(self.prelude_globals, key, Value::NONE);
            }
        }
        if self.cur != 0
            && let Some(Some(t)) = self.tasks.first()
        {
            self.context = t.context;
        }
        self.fresh.clear();
        self.fresh.shrink_to_fit();
        // Bundled-module decoders serve one cell's import chain.
        self.unpackers = None;
        self.afford = false;
        self.conts.clear();
        self.cont_free.clear();
        self.tasks.clear();
        self.parked_bytes = 0;
        self.cur = 0;
        self.ready.clear();
        self.blocked.clear();
        self.blocked_holes = 0;
        self.drop_gathers();
        self.callback_queue = 0;
        self.iter_left = 0;
        self.iter_open = false;
        self.stop_wait = 0;
        self.gc_wait = 0;
        // A clearing pass the ended cell left part way (it parked between
        // two slices and was cancelled or replaced) starts over: the next
        // cell's code runs before the collection the pass was for, and a
        // generator behind the cursor may yield meanwhile.
        self.gens_at = 0;
        self.oom_wait = false;
        self.oom_rerun = u64::MAX;
        self.cb_slots = 0;
        self.task_names = Value::NONE;
        self.escape = Value::NONE;
        self.cancelling = Value::NONE;
        self.run_handled = Value::NONE;
        self.unretrieved.clear();
        self.unretrieved.shrink_to_fit();
        self.loop_flags = 0;
        self.futures.clear();
        self.consumed.clear();
        self.depth = 0;
        self.specials.clear();
        self.saved = None;
        self.request = None;
        self.host_call = None;
        self.handled = Value::NONE;
        self.vclock = 0.0;
        self.stack.truncate(256);
        self.stack.shrink_to_fit();
        self.specials.shrink_to_fit();
        self.conts.shrink_to_fit();
        self.cont_free.shrink_to_fit();
        self.tasks.shrink_to_fit();
        self.ready.shrink_to_fit();
        self.blocked.shrink_to_fit();
        self.gathers.shrink_to_fit();
        self.futures.shrink_to_fit();
        self.consumed.shrink_to_fit();
        let exc = self.exc;
        if self.reserve_execution(0).is_err() {
            self.heap.take_refused();
            self.exc = exc;
        }
    }

    /// Ends every task but the main one, which is running (when the
    /// coroutine `asyncio.run` started returns or raises). The program's
    /// tasks are done by then; the scheduler's own (timers: `call_later`'s,
    /// `wait_for`'s, `as_completed`'s) end cancelled and their coroutines
    /// closed, as CPython's closing loop drops its scheduled handles.
    #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
    fn drop_other_tasks(&mut self) {
        self.check_parked_bytes();
        for k in 1..self.tasks.len() {
            let Some(Some(t)) = self.tasks.get(k) else {
                continue;
            };
            let h = t.handle;
            if !h.is_obj_kind(ty::TASK as u8) || self.raw_of(h, aw::STATE) != aw::PENDING {
                continue;
            }
            let c = self.vslot(h, aw::EXTRA);
            if c.is_obj_kind(ty::CORO as u8) {
                self.coro_finished(c);
            }
            self.settle_cancelled(h, Value::NONE, Value::NONE);
        }
        for t in self.tasks.iter_mut().skip(1) {
            *t = None;
        }
        self.task_hint = self.task_hint.min(1);
        self.parked_bytes = self
            .tasks
            .first()
            .and_then(Option::as_ref)
            .map_or(0, Task::buffer_bytes);
        self.ready.clear();
        self.blocked.clear();
        self.blocked_holes = 0;
        self.drop_gathers();
        self.callback_queue = 0;
        self.iter_left = 0;
        self.iter_open = false;
        self.stop_wait = 0;
        let exc = self.exc;
        if self.reserve_execution(0).is_err() {
            self.heap.take_refused();
            self.exc = exc;
        }
    }

    /// Saves the running task, blocked on `a` (queued again at once if `a`
    /// is a sleep already due, as `sleep(0)` yields). A cancel requested
    /// while it ran goes to `a` now, as CPython's `__step` does.
    #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
    pub(crate) fn block_current(&mut self, st: State, a: Value) {
        let i = self.cur;
        let stack = core::mem::take(&mut self.stack);
        let depth = core::mem::take(&mut self.depth);
        let specials = core::mem::take(&mut self.specials);
        let handled = self.handled;
        let context = self.context;
        if let Some(Some(t)) = self.tasks.get_mut(i) {
            self.parked_bytes = self.parked_bytes.saturating_sub(t.buffer_bytes());
            t.context = context;
            t.stack = stack;
            t.depth = depth;
            t.specials = specials;
            self.parked_bytes = self.parked_bytes.saturating_add(t.buffer_bytes());
            t.st = st;
            t.saved = true;
            t.handled = handled;
            t.awaiting = a;
            t.cleared = false;
            t.copies_cleared = false;
        }
        // A delegation whose iterator yielded `None` yields to the loop,
        // and so does a task that awaits itself: its next step raises
        // (CPython's `__step` schedules itself with the `RuntimeError`), or
        // raises the `CancelledError` of a cancel that came first.
        if (a.is_obj_kind(ty::SLEEP as u8) && self.sleep_due(a))
            || a.is_obj_kind(ty::BUF as u8)
            || self.awaits_itself(a)
        {
            self.ready.push_back(Ready::Task(i));
            return;
        }
        self.block(i);
        let h = self
            .tasks
            .get(i)
            .and_then(Option::as_ref)
            .map_or(Value::NONE, |t| t.handle);
        if self.must_cancel(h) {
            let msg = self.vslot_hot(h, aw::RESULT);
            self.cancel_sched(a, msg, cancel_for::AWAIT, h);
        }
    }

    /// Moves blocked task `i` to the run queue (whose room its blocked slot
    /// holds): a cancel it must raise could not go to what it waits on.
    fn wake_blocked(&mut self, i: usize) {
        if self.unblock(i) {
            self.ready.push_back(Ready::Task(i));
        }
    }

    /// Lists task `i` as blocked, last. The list is packed first when it
    /// is full or its holes outnumber its tasks, in order: a pass over it
    /// for at least as many wakes as it holds tasks, or for a list as
    /// full as the task table (one a restore sized exactly).
    fn block(&mut self, i: usize) {
        let len = self.blocked.len();
        if self.blocked_holes != 0
            && (len == self.blocked.capacity() || self.blocked_holes > len - self.blocked_holes)
        {
            self.pack_blocked();
        }
        // Every entry is now a task or the list has room: it does not
        // grow past what the task table reserved (`Vm::reserve_scheduler`).
        debug_assert!(self.blocked.len() < self.blocked.capacity());
        let at = u32::try_from(self.blocked.len()).unwrap_or(NOT_BLOCKED);
        self.blocked.push(i);
        if let Some(Some(task)) = self.tasks.get_mut(i) {
            task.at = at;
        }
    }

    /// Takes the holes out of the blocked list, in order, and gives each
    /// task its new place.
    fn pack_blocked(&mut self) {
        self.blocked.retain(|&t| t != HOLE);
        self.blocked_holes = 0;
        for k in 0..self.blocked.len() {
            let t = self.blocked[k];
            if let Some(Some(task)) = self.tasks.get_mut(t) {
                task.at = u32::try_from(k).unwrap_or(NOT_BLOCKED);
            }
        }
    }

    /// Takes task `i` out of the blocked list, leaving a hole in its
    /// place: whether it was in it.
    fn unblock(&mut self, i: usize) -> bool {
        let Some(Some(task)) = self.tasks.get_mut(i) else {
            return false;
        };
        let at = core::mem::replace(&mut task.at, NOT_BLOCKED) as usize;
        match self.blocked.get_mut(at) {
            Some(b) if *b == i => {
                *b = HOLE;
                self.blocked_holes += 1;
                true
            }
            _ => false,
        }
    }

    /// Is `a` the running task's own handle?
    fn awaits_itself(&self, a: Value) -> bool {
        a.is_obj_kind(ty::TASK as u8)
            && matches!(self.tasks.get(self.cur), Some(Some(t)) if t.handle == a)
    }

    /// CPython's `RuntimeError` for task `h` awaiting itself, with the
    /// task's repr as micro-monty prints it (no `coro=`); a name longer
    /// than 200 bytes is cut, as the text is built outside the arena.
    fn await_self_error(&mut self, h: Value) -> Value {
        #[cfg(feature = "asyncio")]
        let (state, name) = {
            let state = if self.cancelling(h) == 0 {
                "pending"
            } else {
                "cancelling"
            };
            let name = self.task_name(h).map_or_else(
                |Raised| String::new(),
                |n| {
                    let s = self.str_ref(n);
                    let mut end = s.len().min(200);
                    while !s.is_char_boundary(end) {
                        end -= 1;
                    }
                    crate::text::str_repr(&s[..end])
                },
            );
            (state, name)
        };
        #[cfg(not(feature = "asyncio"))]
        let (state, name) = ("pending", String::new());
        let _ = h;
        let _ = self.err(
            T::RuntimeError,
            &format!("Task cannot await on itself: <Task {state} name={name}>"),
        );
        self.exc
    }

    /// Does task handle `h` (or `None`) have a cancel waiting for its next
    /// step?
    fn must_cancel(&self, h: Value) -> bool {
        h.is_obj_kind(ty::TASK as u8)
            && self.raw_of_hot(h, aw::STATE) == aw::PENDING
            && self.raw_of_hot(h, aw::MUST_CANCEL) != 0
    }

    /// The running child task ended: settles its `TASK` and frees it. A
    /// task that returned with a cancel pending ends cancelled with its
    /// message; one a `CancelledError` ended keeps it for the first
    /// retrieval (CPython's `__step`).
    pub(crate) fn finish_task(&mut self, ok: bool, result: Value) {
        let i = self.cur;
        // A `KeyboardInterrupt` or `SystemExit` leaves the loop: the run
        // cancels its main task and raises it once shut down.
        let escapes = !ok
            && i != 0
            && self.loop_flags & crate::vm::LOOP_IN_RUN != 0
            && self.escape == Value::NONE
            && (self.instance_of(result, self.type_obj(T::KeyboardInterrupt))
                || self.instance_of(result, self.type_obj(T::SystemExit)));
        if let Some(Some(t)) = self.tasks.get(i)
            && t.handle.is_obj_kind(ty::TASK as u8)
        {
            let h = t.handle;
            let log = !ok && !escapes && !self.gathered(h);
            self.end_task(h, ok, result);
            if log && self.raw_of_hot(h, aw::STATE) == aw::FAILED {
                self.note_unretrieved(h);
            }
        }
        if escapes {
            self.escape = result;
            if let Some(Some(t)) = self.tasks.first()
                && t.handle.is_obj_kind(ty::TASK as u8)
            {
                let h = t.handle;
                self.cancel_sched(h, Value::NONE, cancel_for::ESCAPE, Value::NONE);
            }
        }
        if let Some(slot) = self.tasks.get_mut(i) {
            if let Some(t) = slot {
                self.parked_bytes = self.parked_bytes.saturating_sub(t.buffer_bytes());
            }
            *slot = None;
            self.task_hint = self.task_hint.min(i);
        }
        self.depth = 0;
        self.specials.clear();
    }

    /// Is `a` a child of a `gather` (which retrieves its exception)?
    #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
    fn gathered(&self, a: Value) -> bool {
        let w = self.vslot(a, aw::WAITERS);
        (0..self.waiter_count(w))
            .map(|i| self.waiter_at(w, i))
            .any(|g| g.is_obj_kind(ty::GATHER as u8) && self.raw_of(g, aw::AUX) != FIRST)
    }

    /// Failed `a` reports \"exception was never retrieved\" at the end of
    /// `asyncio.run` unless something retrieves it first.
    #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
    pub(crate) fn note_unretrieved(&mut self, a: Value) {
        if self.loop_flags & crate::vm::LOOP_IN_RUN == 0 {
            return;
        }
        if self.unretrieved.len() >= 64 && self.unretrieved.len().is_power_of_two() {
            let keep: Vec<Value> = self
                .unretrieved
                .iter()
                .copied()
                .filter(|&f| self.raw_of(f, aw::MUST_CANCEL) == aw::LOG_TB)
                .collect();
            self.unretrieved = keep;
        }
        if self.unretrieved.try_reserve(1).is_ok() {
            self.set_raw_of(a, aw::MUST_CANCEL, aw::LOG_TB);
            self.unretrieved.push(a);
        }
    }

    /// `a`'s exception was retrieved.
    #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
    pub(crate) fn retrieved(&mut self, a: Value) {
        if self.raw_of(a, aw::STATE) == aw::FAILED {
            self.set_raw_of(a, aw::MUST_CANCEL, 0);
        }
    }

    /// The failed tasks and futures whose exception nobody retrieved, in
    /// the order they failed (the list empties).
    #[cfg(feature = "asyncio")]
    #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
    pub(crate) fn take_unretrieved(&mut self) -> Vec<Value> {
        let mut all = core::mem::take(&mut self.unretrieved);
        all.retain(|&f| self.raw_of(f, aw::MUST_CANCEL) == aw::LOG_TB);
        for &f in &all {
            self.set_raw_of(f, aw::MUST_CANCEL, 0);
        }
        all
    }

    /// Settles task handle `h` as its coroutine ended (see
    /// [`Vm::finish_task`]).
    pub(crate) fn end_task(&mut self, h: Value, ok: bool, result: Value) {
        let must = self.must_cancel(h);
        self.set_raw_of_hot(h, aw::MUST_CANCEL, 0);
        if ok && must {
            let msg = self.vslot_hot(h, aw::RESULT);
            self.settle_cancelled(h, msg, Value::NONE);
        } else if !ok && self.instance_of(result, self.type_obj(T::CancelledError)) {
            self.settle_cancelled(h, Value::NONE, result);
        } else {
            self.settle(h, ok, result);
        }
    }

    /// A new `CancelledError(msg)`, or `CancelledError()` for `None`.
    #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
    pub(crate) fn new_cancelled(&mut self, msg: Value) -> Result<Value, Raised> {
        let args = if msg == Value::NONE {
            self.alloc_tuple(&[])?
        } else {
            self.alloc_tuple(&[msg])?
        };
        let tv = self.types[T::CancelledError as usize];
        self.alloc_exc(tv, args)
    }

    /// What retrieving cancelled `a` raises (`_make_cancelled_error`): the
    /// `CancelledError` that ended a task the first time, then new ones
    /// with the message.
    #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
    pub(crate) fn cancelled_error(&mut self, a: Value) -> Result<Value, Raised> {
        if self.raw_of(a, aw::MUST_CANCEL) == aw::HELD {
            let e = self.vslot(a, aw::RESULT);
            self.set_raw_of(a, aw::MUST_CANCEL, 0);
            if let Some(r) = a.as_obj() {
                self.set_slot(r, aw::RESULT, Value::NONE);
            }
            return Ok(e);
        }
        let msg = self.vslot(a, aw::RESULT);
        self.new_cancelled(msg)
    }

    /// A task for coroutine (or `asyncio.sleep`) `c`, queued to run.
    #[cfg(feature = "asyncio")]
    pub(crate) fn spawn(&mut self, c: Value) -> Result<Value, Raised> {
        self.spawn_named(c, Value::NONE)
    }

    /// A new task handle for slot `i` running `c`, named `name` (`None`
    /// for the next `Task-N`).
    #[cfg(feature = "asyncio")]
    fn new_task_handle(&mut self, c: Value, i: usize, name: Value) -> Result<Value, Raised> {
        // As CPython's `_task_name_counter`, only unnamed tasks take a
        // number.
        let word = if name == Value::FALSE {
            aw::INTERNAL
        } else if Vm::is_str(name) {
            0
        } else {
            self.task_counter += 1;
            (self.task_counter & 0xFFFF_FFFF) << aw::NUMBER_SHIFT
        };
        let r = {
            let res = self.heap.alloc_init(
                ty::TASK,
                0,
                &[Value::NONE, c, Value::NONE],
                &[aw::PENDING, 0, i as u64, word, 0],
            );
            self.hx_hot(res)
        }?;
        let h = Value::from_ref(r, ty::TASK as u8);
        if Vm::is_str(name) {
            self.set_task_name(h, name)?;
        }
        Ok(h)
    }

    /// [`Vm::spawn`] with a name: `None` for `Task-N`, `False` for the
    /// scheduler's own tasks, which `asyncio.run` does not wait for.
    #[cfg(feature = "asyncio")]
    pub(crate) fn spawn_named(&mut self, c: Value, name: Value) -> Result<Value, Raised> {
        // Slots below `task_hint` are taken; the lowest free one is reused.
        let from = self.task_hint.min(self.tasks.len());
        let i = self.tasks[from..]
            .iter()
            .position(Option::is_none)
            .map_or(self.tasks.len(), |k| from + k);
        self.task_hint = i + 1;
        if i == self.tasks.capacity() {
            self.check_parked_bytes();
            self.reserve_scheduler(i.max(2) * 2, self.gathers.len(), 0)?;
        }
        // A task's `AUX` is its index, for `cancel()`.
        let h = self.new_task_handle(c, i, name)?;
        if c.is_obj_kind(ty::CORO as u8) {
            let s = self.raw_of_hot(c, coro::STATE);
            self.set_raw_of_hot(c, coro::STATE, s | coro::SCHEDULED);
        }
        // A task runs in a copy of its creator's context.
        let context = self.context_copy()?;
        let task = Task::new(h, context);
        if i == self.tasks.len() {
            self.tasks.push(Some(task));
        } else {
            self.tasks[i] = Some(task);
        }
        self.ready.push_back(Ready::Task(i));
        Ok(h)
    }

    fn arm_sleep(&mut self, a: Value) {
        let delay = self.vslot_hot(a, aw::EXTRA).as_float().unwrap_or(0.0);
        let wake = self.vclock + delay.max(0.0);
        self.set_raw_of_hot(a, aw::AUX, wake.to_bits());
        self.set_raw_of_hot(a, aw::STATE, aw::ARMED);
    }

    /// Makes task `i` the running one and returns its registers, starting
    /// its coroutine if it never ran; `None` if a sleep's task took its
    /// step instead (it waits again, or completed).
    fn activate(&mut self, i: usize) -> Result<Option<State>, Raised> {
        self.cur = i;
        let Some(Some(t)) = self.tasks.get_mut(i) else {
            return Err(self.err(T::SystemError, "bad task"));
        };
        self.parked_bytes = self.parked_bytes.saturating_sub(t.buffer_bytes());
        self.stack = core::mem::take(&mut t.stack);
        self.depth = core::mem::take(&mut t.depth);
        self.specials = core::mem::take(&mut t.specials);
        self.handled = t.handled;
        self.context = t.context;
        t.awaiting = Value::NONE;
        let handle = t.handle;
        let saved = core::mem::take(&mut t.saved).then(|| t.st.clone());
        // Task buffers move without a new charge; replacing the previous
        // task's completed buffers releases their reservation. This only
        // lowers the charge, and the task's registers are already taken,
        // so a failure (a charge an earlier growth left past the limit) is
        // left to the next reservation, which raises `MemoryError` in a
        // task, instead of losing this one.
        let exc = self.exc;
        if self.reserve_execution(0).is_err() {
            self.heap.take_refused();
            self.exc = exc;
        }
        if let Some(st) = saved {
            return Ok(Some(st));
        }
        // A first step runs from the loop, under the run's caller.
        self.handled = self.run_handled;
        let c = self.vslot_hot(handle, aw::EXTRA);
        if c.is_obj_kind(ty::SLEEP as u8) {
            // A sleep's task steps as the `asyncio.sleep()` coroutine does:
            // the first arms it and yields (to the loop for a delay of 0),
            // the next one ends the task.
            let s = self.raw_of_hot(c, aw::STATE);
            if s == aw::PENDING {
                self.arm_sleep(c);
                if let Some(Some(t)) = self.tasks.get_mut(i) {
                    t.awaiting = c;
                }
                if self.sleep_due(c) {
                    self.ready.push_back(Ready::Task(i));
                } else {
                    self.block(i);
                }
                return Ok(None);
            }
            if s == aw::ARMED && self.sleep_due(c) {
                self.set_raw_of_hot(c, aw::STATE, aw::SPENT);
                let r = self.vslot_hot(c, aw::RESULT);
                self.finish_task(true, r);
                return Ok(None);
            }
            if s == aw::CANCELLED {
                let e = self.cancelled_error(c)?;
                self.set_raw_of_hot(c, aw::STATE, aw::SPENT);
                self.finish_task(false, e);
                return Ok(None);
            }
            return Err(self.err(T::RuntimeError, REUSED));
        }
        if self.raw_of_hot(c, coro::STATE) & 3 != coro::CREATED {
            return Err(self.err(T::RuntimeError, REUSED));
        }
        let func = self.vslot_hot(c, coro::FUNC);
        let cidx = u32::try_from(self.vslot_raw_of(func, fn_slot::CODE)).unwrap_or(0);
        let Some(code) = self.codes.get(cidx as usize).cloned() else {
            return Err(self.err(T::SystemError, "bad code index"));
        };
        let nregs = code.code.nregs as usize;
        self.ensure_stack(nregs + 64)?;
        self.load_coro_args(c, 0, nregs);
        if !code.code.freevars.is_empty() {
            let closure = self.vslot_hot(func, fn_slot::CLOSURE);
            for (k, &r) in code.code.freevars.iter().enumerate() {
                self.stack[r as usize] = self.vslot_hot(closure, u32::try_from(k).unwrap_or(0));
            }
        }
        self.set_raw_of_hot(c, coro::STATE, coro::RUNNING);
        if let Some(r) = c.as_obj() {
            self.set_slot_hot(r, coro::ARGS, Value::NONE);
        }
        Ok(Some(State {
            pc: 0,
            base: 0,
            ci: cidx,
            acc: Value::NONE,
            func,
            coro: c,
        }))
    }

    /// A gather's done callback for settled `child` (CPython's
    /// `_done_callback`): settles the gather if its children allow.
    fn check_gather(&mut self, g: Value, child: Value) {
        // The gather's state, kind and counts in one read of its words,
        // which proves `LEFT` a raw word of it for the write: this runs
        // for each child's notification. An object without them (a forged
        // entry's) has nothing to settle.
        let Some(r) = g.as_obj() else {
            return;
        };
        let Ok((h, w)) = self.heap.object_words(r) else {
            return;
        };
        let raw = |i: u32| w.get(i as usize).copied().filter(|_| i >= h.nvals());
        let (Some(state), Some(aux), Some(left)) = (raw(aw::STATE), raw(aw::AUX), raw(aw::LEFT))
        else {
            return;
        };
        if state != aw::PENDING {
            return;
        }
        // This notification has run (`nfinished += 1`).
        let unrun = (left >> aw::UNRUN_SHIFT).saturating_sub(1);
        let left = unrun << aw::UNRUN_SHIFT | left & aw::LEFT_MASK;
        self.heap.set_raw_guarded(r, aw::LEFT, left);
        if aux == FIRST {
            if let Some(c) = self.first_done(g) {
                self.untrack_gather(g);
                self.settle(g, true, c);
            }
            return;
        }
        let keep_errors = aux != 0;
        let failed = matches!(
            self.raw_of_hot(child, aw::STATE),
            aw::FAILED | aw::CANCELLED
        );
        if !keep_errors && failed {
            self.untrack_gather(g);
            let e = self.child_error(child);
            self.settle(g, false, e);
            return;
        }
        if unrun != 0 {
            return;
        }
        // The gather settles now: one pass over its children.
        let children = self.tuple_items(self.vslot_hot(g, aw::EXTRA));
        self.untrack_gather(g);
        if self.raw_of_hot(g, aw::FAILS) & aw::CANCEL_REQUESTED != 0 {
            // `gather(..., return_exceptions=True)` cancelled still raises:
            // CPython's `_make_cancelled_error()` of the last child, which
            // for one that did not end cancelled is a new error with the
            // message of its last cancel (kept at its settle).
            let last = children.last().copied().unwrap_or(Value::NONE);
            let e = if self.raw_of_hot(last, aw::STATE) == aw::CANCELLED {
                self.child_error(last)
            } else {
                let msg = self.vslot_hot(g, aw::RESULT);
                self.new_cancelled(msg).unwrap_or(self.exc)
            };
            self.settle(g, false, e);
            return;
        }
        let mut results: Vec<Value> = Vec::with_capacity(children.len());
        for &c in &children {
            let v = if self.raw_of_hot(c, aw::STATE) == aw::CANCELLED {
                // CPython reports `CancelledError('')` without a message.
                let msg = match self.cancel_msg(c) {
                    Value::NONE => self.new_str(""),
                    m => Ok(m),
                };
                let Ok(e) = msg.and_then(|m| self.new_cancelled(m)) else {
                    let e = self.exc;
                    self.settle(g, false, e);
                    return;
                };
                e
            } else {
                self.vslot_hot(c, aw::RESULT)
            };
            results.push(v);
        }
        if let Ok(l) = self.new_list(&results) {
            self.settle(g, true, l);
        } else {
            // Raised later, in the task that awaits it: its refusal is
            // not this instruction's to retry.
            self.heap.take_refused();
            let e = self.exc;
            self.settle(g, false, e);
        }
    }

    /// The exception a gather takes from failed or cancelled child `c`.
    #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
    fn child_error(&mut self, c: Value) -> Value {
        self.retrieved(c);
        if self.raw_of(c, aw::STATE) == aw::CANCELLED {
            self.cancelled_error(c).unwrap_or(self.exc)
        } else {
            self.vslot(c, aw::RESULT)
        }
    }

    /// The next task to run, or the host futures everything waits on.
    pub(crate) fn next_task(&mut self) -> Next {
        loop {
            // A cancel the scheduler began ends before anything else runs.
            if self.cancelling != Value::NONE
                && let Some(n) = self.cancel_resume()
            {
                return n;
            }
            // Loop iterations, as CPython's `_run_once` runs the callbacks
            // ready when it starts: `asyncio.run` stops its loop at the end
            // of one, and the main task resumes then (`_aio_stop`).
            if self.iter_left == 0 {
                if self.iter_open {
                    self.iter_open = false;
                    if self.stop_wait > 0 {
                        self.stop_wait -= 1;
                        if self.stop_wait == 0 || self.ready.is_empty() {
                            self.stop_wait = 0;
                            if let Some(st) = self.loop_stopped() {
                                return Next::Run(st);
                            }
                        }
                    }
                }
                if !self.ready.is_empty() {
                    self.iter_left = self.ready.len();
                    self.iter_open = true;
                }
            }
            if let Some(entry) = self.ready.pop_front() {
                self.iter_left = self.iter_left.saturating_sub(1);
                if !matches!(entry, Ready::Task(_)) {
                    self.callback_queue -= 1;
                }
                // Whether this entry is the retry after a start that waited
                // for a collection (`Vm::start_failed`).
                let retry = self.loop_flags & crate::vm::LOOP_START_WAIT != 0;
                self.loop_flags &= !crate::vm::LOOP_START_WAIT;
                let i = match entry {
                    Ready::Task(i) => i,
                    Ready::Gather(g, c) => {
                        self.check_gather(g, c);
                        if self.sched_due() {
                            let st = self.idle_state();
                            if let Some(r) = self.safepoint(&st) {
                                self.loop_flags |= crate::vm::LOOP_SKIP_PARK;
                                return Next::Park(r, st);
                            }
                        }
                        continue;
                    }
                    Ready::Call(..) | Ready::Handle(_) => {
                        match self.start_callback(entry) {
                            // The start is the tick `_aio_callback`'s
                            // `CALL_EX` charged: a callback with no call or
                            // loop in its body passes a safepoint too, so a
                            // queue of them is no single step.
                            Ok(Some(st)) => {
                                self.ticks -= 1;
                                if self.ticks <= 0
                                    && let Some(r) = self.safepoint(&st)
                                {
                                    return Next::Park(r, st);
                                }
                                return Next::Run(st);
                            }
                            // A cancelled handle runs nothing, but its start
                            // charges the same tick, at the trampoline's
                            // safepoint (`Vm::idle_state`), as on 0.0.2,
                            // where `Handle._run` returned: a queue of them
                            // is no single step either.
                            Ok(None) => {
                                self.ticks -= 1;
                                if self.ticks <= 0 {
                                    let st = self.idle_state();
                                    if let Some(r) = self.safepoint(&st) {
                                        self.loop_flags |= crate::vm::LOOP_SKIP_PARK;
                                        return Next::Park(r, st);
                                    }
                                }
                            }
                            Err(Raised) => {
                                if let Some(n) = self.start_failed(entry, retry) {
                                    return n;
                                }
                            }
                        }
                        continue;
                    }
                };
                let Some(Some(t)) = self.tasks.get(i) else {
                    continue;
                };
                let (h, waited, started) = (t.handle, t.awaiting, t.saved);
                // A task settled before it ran: free its slot.
                if i != 0 && !started && self.raw_of_hot(h, aw::STATE) != aw::PENDING {
                    self.tasks[i] = None;
                    self.task_hint = self.task_hint.min(i);
                    continue;
                }
                // `cancel()` asked for a `CancelledError` at this step.
                let must = self.must_cancel(h);
                if must {
                    self.set_raw_of_hot(h, aw::MUST_CANCEL, 0);
                    // Woken by a cancelled awaitable (or a gather that one
                    // failed): its own error goes in.
                    let s = self.raw_of_hot(waited, aw::STATE);
                    if s == aw::CANCELLED
                        || (s == aw::FAILED
                            && self.instance_of(
                                self.vslot_hot(waited, aw::RESULT),
                                self.type_obj(T::CancelledError),
                            ))
                    {
                        if let Ok(Some(st)) = self.activate(i) {
                            return Next::Run(st);
                        }
                        let e = self.exc;
                        self.finish_task(false, e);
                        continue;
                    }
                }
                let exc = if must {
                    let msg = self.vslot_hot(h, aw::RESULT);
                    self.new_cancelled(msg).unwrap_or(self.exc)
                } else if waited == h && h != Value::NONE {
                    self.await_self_error(h)
                } else {
                    Value::NONE
                };
                let must = exc != Value::NONE;
                if must && !started && i != 0 {
                    // Thrown into a coroutine that never started, which
                    // closes it unrun.
                    self.cur = i;
                    let c = self.vslot_hot(h, aw::EXTRA);
                    if c.is_obj_kind(ty::SLEEP as u8) {
                        self.set_raw_of_hot(c, aw::STATE, aw::SPENT);
                    } else {
                        self.coro_close(c);
                    }
                    self.finish_task(false, exc);
                    continue;
                }
                match self.activate(i) {
                    Ok(Some(st)) if must => {
                        // Thrown into the `__await__` iterator it steps.
                        if st.acc.is_obj_kind(ty::BUF as u8) && st.coro.is_obj_kind(ty::CORO as u8)
                        {
                            self.coro_deliver(st.coro, exc, coro::THROW);
                            return Next::Run(st);
                        }
                        self.exc = exc;
                        return Next::Raise(st);
                    }
                    Ok(Some(st)) => {
                        if self.sched_due()
                            && let Some(r) = self.safepoint(&st)
                        {
                            return Next::Park(r, st);
                        }
                        return Next::Run(st);
                    }
                    Ok(None) => continue,
                    // A first start with no room for its stack: as a
                    // callback's start (the task is still unstarted).
                    Err(Raised)
                        if i != 0 && !started && self.exc_type(self.exc) == T::MemoryError =>
                    {
                        if let Some(n) = self.start_failed(entry, retry) {
                            return n;
                        }
                        continue;
                    }
                    Err(Raised) => {
                        let e = self.exc;
                        self.finish_task(false, e);
                        continue;
                    }
                }
            }
            if self.stop_wait > 0 {
                // Nothing left to run: the loop stops now.
                self.stop_wait = 0;
                if let Some(st) = self.loop_stopped() {
                    return Next::Run(st);
                }
            }
            // Sleepers: advance virtual time to the earliest wake-up and
            // queue those due then, earliest first. Every sleep and
            // deadline waits for pending host calls first: host calls take
            // no virtual time, so a loop polling with `sleep(t)` gets its
            // answer, and a sleep cannot outrun a call made before it.
            let mut sleepers: Vec<(f64, usize)> = Vec::new();
            let mut hosts: Option<bool> = None;
            for (k, &t) in self.blocked.iter().enumerate() {
                if let Some(Some(task)) = self.tasks.get(t) {
                    let a = task.awaiting;
                    if a.is_obj_kind(ty::SLEEP as u8) && self.raw_of_hot(a, aw::STATE) == aw::ARMED
                    {
                        if *hosts.get_or_insert_with(|| !self.host_ids(true).is_empty()) {
                            continue;
                        }
                        sleepers.push((f64::from_bits(self.raw_of_hot(a, aw::AUX)), k));
                    }
                }
            }
            if let Some(w) = sleepers.iter().map(|s| s.0).reduce(f64::min) {
                self.vclock = self.vclock.max(w);
                sleepers.retain(|s| s.0 <= self.vclock);
                crate::ops::heap_sort_by(&mut sleepers, |a, b| {
                    a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)).is_lt()
                });
                let due: Vec<usize> = sleepers.iter().map(|s| self.blocked[s.1]).collect();
                for &t in &due {
                    self.unblock(t);
                }
                self.ready.extend(due.into_iter().map(Ready::Task));
                continue;
            }
            // A sleeper held back for host calls may be polling any of them
            // (`f.done()`), so then every unsettled one is asked for.
            let ids = if hosts == Some(true) {
                self.host_ids(true)
            } else {
                self.pending_ids()
            };
            if !ids.is_empty() {
                return Next::Wait(ids);
            }
            // Nothing can ever settle: raise in the main task.
            if self.unblock(0)
                && let Ok(Some(st)) = self.activate(0)
            {
                let _ = self.err(
                    T::RuntimeError,
                    "every task is waiting and no host call is pending",
                );
                return Next::Raise(st);
            }
            return Next::Wait(Vec::new());
        }
    }

    /// Ids of the unsettled host futures something waits for, in ascending
    /// order: those of [`Vm::host_ids`], and those a blocked task waits
    /// on, directly or through gathers. Each gather is walked once: it is
    /// marked [`aw::VISITED`] while the walk lasts (a list of the ones
    /// seen, searched for each, took 227 ms for a gather of 40,000 [M]).
    #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
    fn pending_ids(&mut self) -> Vec<u64> {
        let mut ids = self.host_ids(false);
        let mut work: Vec<Value> = self
            .blocked
            .iter()
            .filter_map(|&t| match self.tasks.get(t) {
                Some(Some(task)) => Some(task.awaiting),
                _ => None,
            })
            .collect();
        let mut visited: Vec<Value> = Vec::new();
        while let Some(a) = work.pop() {
            if self.raw_of(a, aw::STATE) != aw::PENDING {
                continue;
            }
            if a.is_obj_kind(ty::FUTURE as u8) {
                let id = self.raw_of(a, aw::AUX);
                if id < STOP_FUTURE && self.host_pending(id) {
                    ids.push(id);
                }
            } else if a.is_obj_kind(ty::GATHER as u8) {
                let flags = self.raw_of(a, aw::FAILS);
                if flags & aw::VISITED != 0 {
                    continue;
                }
                self.set_raw_of(a, aw::FAILS, flags | aw::VISITED);
                visited.push(a);
                work.extend(self.tuple_items(self.vslot(a, aw::EXTRA)));
            }
        }
        for g in visited {
            let flags = self.raw_of(g, aw::FAILS);
            self.set_raw_of(g, aw::FAILS, flags & !aw::VISITED);
        }
        crate::ops::heap_sort_by(&mut ids, |a, b| a < b);
        ids.dedup();
        ids
    }

    /// Is host future `id` in the cell's table and unsettled?
    fn host_pending(&self, id: u64) -> bool {
        matches!(self.futures.binary_search_by_key(&id, |&(id, _)| id),
            Ok(i) if self.raw_of(self.futures[i].1, aw::STATE) == aw::PENDING)
    }

    /// Ids of the cell's unsettled host futures, in ascending order: with
    /// `all`, every one, awaited or not (what a sleep waits for: its
    /// answer may be what a polling loop looks for); else those a done
    /// callback waits on (`shield`, `wait`, `add_done_callback`) or that
    /// list a waiter. Only futures in the cell's table count: cell end
    /// clears `futures`, and the host cannot answer a call of an earlier
    /// cell.
    fn host_ids(&self, all: bool) -> Vec<u64> {
        self.futures
            .iter()
            .filter(|&&(id, f)| {
                self.host_pending(id) && (all || self.vslot(f, aw::WAITERS) != Value::NONE)
            })
            .map(|&(id, _)| id)
            .collect()
    }

    // ------------------------------------------------- host futures

    /// The status of host future `id`.
    #[must_use]
    #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
    pub fn future_status(&self, id: u64) -> FutureStatus {
        match self.futures.binary_search_by_key(&id, |&(id, _)| id) {
            Err(_) => FutureStatus::Unknown,
            Ok(i) => match self.raw_of(self.futures[i].1, aw::STATE) {
                aw::PENDING => FutureStatus::Pending,
                aw::CANCELLED => FutureStatus::Cancelled,
                _ => FutureStatus::Settled,
            },
        }
    }

    /// Delivers host results to pending futures (unknown or settled ids are
    /// ignored; check [`Vm::future_status`] first) and runs until the next
    /// event.
    #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
    pub fn resume_batch(
        &mut self,
        results: &[(u64, Result<crate::HostValue, crate::HostError>)],
    ) -> Exit {
        for (id, r) in results {
            let Ok(i) = self.futures.binary_search_by_key(id, |&(id, _)| id) else {
                continue;
            };
            let f = self.futures[i].1;
            if self.raw_of(f, aw::STATE) != aw::PENDING {
                continue;
            }
            if r.is_ok() {
                self.settle_call(*id, crate::host::CallStatus::Returned);
            }
            let (ok, v) = match r {
                Ok(h) => match self.py_value(h) {
                    Ok(v) => (true, v),
                    Err(Raised) => (false, self.exc),
                },
                Err(e) => match self.host_exception(e, *id) {
                    Ok(x) => (false, x),
                    Err(Raised) => (false, self.exc),
                },
            };
            self.settle(f, ok, v);
        }
        self.run_next()
    }

    /// Picks the next task and runs it.
    #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
    pub(crate) fn run_next(&mut self) -> Exit {
        match self.next_task() {
            Next::Run(st) => self.execute(st, false),
            Next::Raise(st) => self.execute(st, true),
            Next::Wait(ids) => self.wait_on(ids),
            Next::Park(r, st) => {
                self.saved = Some(st);
                Exit::Parked(r)
            }
        }
    }

    /// `Exit::Waiting`, unless nothing is pending (the main task is gone,
    /// which would be an engine bug): then the cell fails.
    #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
    pub(crate) fn wait_on(&mut self, ids: Vec<u64>) -> Exit {
        if ids.is_empty() {
            self.end_cell();
            let _ = self.err(T::SystemError, "micro-monty: no task can run");
            return Exit::Error;
        }
        Exit::Waiting(ids)
    }

    // ------------------------------------------------------- natives

    /// `asyncio.gather(*aws, return_exceptions=False)`.
    #[cfg(feature = "asyncio")]
    pub(crate) fn gather(&mut self, items: &[Value], keep_errors: bool) -> Result<Value, Raised> {
        for (i, &a) in items.iter().enumerate() {
            if !matches!(
                a.obj_kind().map(u16::from),
                Some(ty::CORO | ty::TASK | ty::FUTURE | ty::GATHER | ty::SLEEP)
            ) {
                // CPython has already wrapped the items before this one in
                // tasks: they are not reported as never awaited.
                self.burn(i as u64);
                for &c in &items[..i] {
                    if c.is_obj_kind(ty::CORO as u8) {
                        let s = self.raw_of_hot(c, coro::STATE);
                        self.set_raw_of_hot(c, coro::STATE, s | coro::SCHEDULED);
                    }
                }
                return Err(self.err(
                    T::TypeError,
                    "An asyncio.Future, a coroutine or an awaitable is required",
                ));
            }
        }
        for &a in items {
            if a.is_obj_kind(ty::FUTURE as u8) {
                // Gathered: not reported as never awaited.
                self.set_raw_of_hot(a, aw::AWAITED, 1);
            }
        }
        // `first[i]`: the first position holding the same object as `i`.
        let mut order: Vec<usize> = (0..items.len()).collect();
        crate::ops::heap_sort_by(&mut order, |&x, &y| {
            (items[x].to_bits(), x) < (items[y].to_bits(), y)
        });
        let mut first: Vec<usize> = (0..items.len()).collect();
        for w in order.windows(2) {
            if items[w[0]] == items[w[1]] {
                first[w[1]] = first[w[0]];
            }
        }
        let mut children: Vec<Value> = Vec::with_capacity(items.len());
        let mut distinct: Vec<Value> = Vec::new();
        for (i, &a) in items.iter().enumerate() {
            if first[i] != i {
                children.push(children[first[i]]);
                continue;
            }
            let c = if a.is_obj_kind(ty::CORO as u8) || a.is_obj_kind(ty::SLEEP as u8) {
                self.spawn(a)?
            } else {
                a
            };
            distinct.push(c);
            children.push(c);
        }
        let t = self.alloc_tuple_hot(&children)?;
        let g = self.new_awaitable(
            ty::GATHER,
            Value::NONE,
            t,
            aw::PENDING,
            u64::from(keep_errors),
        )?;
        let mut left = 0;
        let mut failed = None;
        for &c in &distinct {
            match self.raw_of_hot(c, aw::STATE) {
                aw::PENDING => left += 1,
                aw::FAILED | aw::CANCELLED if failed.is_none() => failed = Some(c),
                _ => {}
            }
            self.add_waiter(c, g)?;
        }
        // CPython 3.14's `gather` runs the done callbacks of the children
        // already done before it returns, in argument order: the first of
        // them that failed, or every child done, settles it at once. Its
        // pending children's later callbacks find it untracked. So only
        // the pending children's notifications are left to run.
        self.set_raw_of_hot(g, aw::LEFT, left << aw::UNRUN_SHIFT | left);
        if let Some(c) = failed.filter(|_| !keep_errors) {
            let e = self.child_error(c);
            self.settle(g, false, e);
            return Ok(g);
        }
        if left == 0 {
            // The last done child's callback, which runs with every child
            // done: no failure is its to take here.
            self.set_raw_of_hot(g, aw::LEFT, 1 << aw::UNRUN_SHIFT);
            self.check_gather(g, Value::NONE);
            return Ok(g);
        }
        let capacity = self.gathers_room();
        self.reserve_scheduler(self.tasks.len(), capacity, children.len())?;
        self.track_gather(g);
        self.callback_queue += usize::try_from(left).unwrap_or(usize::MAX);
        Ok(g)
    }

    fn waiter_count(&self, w: Value) -> u32 {
        if w.is_obj_kind(ty::BUF as u8) {
            self.vslot(w, 0)
                .as_int()
                .and_then(|n| u32::try_from(n).ok())
                .unwrap_or(0)
        } else {
            u32::from(w != Value::NONE)
        }
    }

    fn waiter_at(&self, w: Value, i: u32) -> Value {
        if w.is_obj_kind(ty::BUF as u8) {
            self.vslot(w, i + 1)
        } else {
            w
        }
    }

    /// Lists `g` (a gather, a task slot or a callback) among what waits on
    /// awaitable `c`.
    #[inline(never)]
    fn add_waiter(&mut self, c: Value, g: Value) -> Result<(), Raised> {
        let w = self.vslot_hot(c, aw::WAITERS);
        let Some(owner) = c.as_obj() else {
            return Ok(());
        };
        if w == Value::NONE {
            self.set_slot_hot(owner, aw::WAITERS, g);
            return Ok(());
        }
        let n = self.waiter_count(w);
        if w.is_obj_kind(ty::BUF as u8)
            && let Some(b) = w.as_obj()
            && self.heap.set(b, n + 1, g).is_ok()
        {
            self.set_slot_hot(b, 0, Value::int(i64::from(n + 1)).unwrap_or(Value::NONE));
            return Ok(());
        }
        // Settled gathers listen no more (each `as_completed` step leaves
        // one on every awaitable left), nor tasks no longer blocked on `c`:
        // drop them.
        self.burn(u64::from(n));
        let live = |vm: &Vm, x: Value| {
            if x.is_obj_kind(ty::GATHER as u8) {
                return vm.raw_of(x, aw::STATE) == aw::PENDING;
            }
            match x.as_int().and_then(|t| usize::try_from(t).ok()) {
                Some(t) => {
                    matches!(vm.tasks.get(t), Some(Some(task)) if task.awaiting == c)
                        || (t == vm.cur && x != g)
                }
                None => true,
            }
        };
        let kept = (0..n).filter(|&i| live(self, self.waiter_at(w, i))).count() as u32;
        if kept == 0 {
            self.set_slot(owner, aw::WAITERS, g);
        } else {
            let max = (mm_core::MAX_OBJECT_BYTES / 8) as u32 - 1;
            let cap = (kept * 2).min(max).max(kept + 1);
            let b = self.alloc_buf(cap as usize + 1)?;
            if kept == n
                && w.is_obj_kind(ty::BUF as u8)
                && let Some(src) = w.as_obj()
            {
                self.copy_slots(b, 1, src, 1, n as usize);
            } else {
                let mut at = 1;
                for i in 0..n {
                    let x = self.waiter_at(w, i);
                    if live(self, x) {
                        self.set_slot(b, at, x);
                        at += 1;
                    }
                }
            }
            self.set_slot(b, kept + 1, g);
            self.set_slot(b, 0, Value::int(i64::from(kept + 1)).unwrap_or(Value::NONE));
            self.set_slot(owner, aw::WAITERS, Value::from_ref(b, ty::BUF as u8));
        }
        Ok(())
    }

    /// `fut.add_done_callback(fn)`: `fn(fut)` runs soon after `fut` is
    /// done, or soon if it is done already.
    #[cfg(feature = "asyncio")]
    #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
    pub(crate) fn add_done_callback(&mut self, fut: Value, f: Value) -> Result<(), Raised> {
        let pending = self.raw_of(fut, aw::STATE) == aw::PENDING;
        if pending && self.pays_at_settle(fut) {
            // The settle reserves this callback's room; the queue grows by
            // one past its spare room now if it can, while the heap has not
            // yet committed the segments the program goes on to fill, which
            // a collection does not give back. Without it, 3.5% fewer
            // callbacks could be registered and run at 16 MiB [M]. The
            // spare room is in the image (`Vm::queue_spare`), so a restored
            // queue grows as the running one would.
            let exc = self.exc;
            let want = self.queue_spare().saturating_add(1);
            if self
                .reserve_scheduler(self.tasks.len(), self.gathers.len(), want)
                .is_err()
            {
                self.heap.take_refused();
                self.exc = exc;
            }
            self.add_waiter(fut, f)?;
            return Ok(());
        }
        self.reserve_scheduler(self.tasks.len(), self.gathers.len(), 1)?;
        if !pending {
            self.ready.push_back(Ready::Call(f, fut));
            self.callback_queue += 1;
            return Ok(());
        }
        self.add_waiter(fut, f)?;
        self.cb_slots += 1;
        Ok(())
    }

    /// `fut.remove_done_callback(fn)`: drops every callback equal to `fn`
    /// (the same object, or a bound method of the same function and
    /// object) and returns how many.
    #[cfg(feature = "asyncio")]
    #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
    pub(crate) fn remove_done_callback(&mut self, fut: Value, f: Value) -> usize {
        let w = self.vslot(fut, aw::WAITERS);
        let len = self.waiter_count(w);
        self.burn(u64::from(len));
        let same = |vm: &Vm, x: Value| {
            if x == f {
                return true;
            }
            let k = x.obj_kind().map(u16::from);
            matches!(k, Some(ty::METHOD | ty::BOUND))
                && f.obj_kind().map(u16::from) == k
                && vm.vslot(x, 0) == vm.vslot(f, 0)
                && vm.vslot(x, 1) == vm.vslot(f, 1)
        };
        let callable = |x: Value| x.as_int().is_none() && !x.is_obj_kind(ty::GATHER as u8);
        let mut kept = 0;
        for i in 0..len {
            let x = self.waiter_at(w, i);
            if callable(x) && same(self, x) {
                continue;
            }
            if kept != i
                && let Some(b) = w.as_obj()
            {
                self.set_slot(b, kept + 1, x);
            }
            kept += 1;
        }
        let n = (len - kept) as usize;
        if n == 0 {
            return 0;
        }
        let w = match kept {
            0 => Value::NONE,
            1 => self.waiter_at(w, 0),
            _ => {
                if let Some(b) = w.as_obj() {
                    for i in kept..len {
                        self.set_slot(b, i + 1, Value::NONE);
                    }
                    self.set_slot(b, 0, Value::int(i64::from(kept)).unwrap_or(Value::NONE));
                }
                w
            }
        };
        if let Some(r) = fut.as_obj() {
            self.set_slot(r, aw::WAITERS, w);
        }
        // Only room this cell reserved goes back: a loop future holds none,
        // and an earlier cell's awaitable lost its room with that cell.
        if !self.pays_at_settle(fut) && self.live(fut) {
            self.cb_slots = self.cb_slots.saturating_sub(n);
        }
        n
    }

    /// `loop.call_soon(...)`: `asyncio.Handle` `h` runs at its turn in the
    /// run queue.
    #[cfg(feature = "asyncio")]
    #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
    pub(crate) fn call_soon(&mut self, h: Value) -> Result<(), Raised> {
        self.reserve_scheduler(self.tasks.len(), self.gathers.len(), 1)?;
        self.ready.push_back(Ready::Handle(h));
        self.callback_queue += 1;
        Ok(())
    }

    /// A timer's handle `h` is due: it runs next, in this iteration, at the
    /// place of the internal task that waited for it (`_aio_later`), as
    /// CPython moves a due timer to the ready queue.
    #[cfg(feature = "asyncio")]
    #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
    pub(crate) fn fire_timer(&mut self, h: Value) -> Result<(), Raised> {
        // The timer's task calls this as it ends, and a running task holds
        // a queue entry it does not use (the queue's room covers every task
        // slot): the handle takes it, so a due timer always fires. Only a
        // direct call of `_mm.aio_fire` can find the queue full.
        if self.ready.len() >= self.ready.capacity() {
            return Err(self.err(T::MemoryError, "scheduler allocation failed"));
        }
        self.ready.push_front(Ready::Handle(h));
        self.callback_queue += 1;
        self.iter_left += 1;
        Ok(())
    }

    /// Starts queue entry `entry`, a done callback `f(fut)` or a handle's
    /// `_callback(*_args)`, as a callback of the loop: in a task slot of its
    /// own with no handle, through the prelude's `_aio_callback` (which
    /// reports what it raises). `Ok(None)`: a cancelled handle, which runs
    /// nothing.
    ///
    /// The start is everything up to the callback running: its arguments,
    /// its context, its slot, `_aio_callback`'s frame and, for a Python
    /// function (or a method of one), the function's frame entered from
    /// `_aio_callback`'s `CALL_EX`, as that instruction would. So nothing
    /// that can fail for room runs as guest code before the callback: a
    /// failure is the start's, and the entry stays queued
    /// (`Vm::start_failed`). A native callback, or another callable, is
    /// called by that `CALL_EX` when the slot runs: the call is its run.
    #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
    fn start_callback(&mut self, entry: Ready) -> Result<Option<State>, Raised> {
        // What can fail comes first, so a failure leaves no slot taken.
        let (f, args, h, context) = match entry {
            Ready::Call(f, fut) => (
                f,
                self.alloc_tuple(&[fut])?,
                Value::NONE,
                self.context_copy()?,
            ),
            Ready::Handle(h) => {
                let get = |vm: &mut Vm, name: &str| -> Result<Value, Raised> {
                    let k = vm.intern(name)?;
                    Ok(vm.inst_get(h, k).unwrap_or(Value::UNDEFINED))
                };
                match get(self, "_cancelled")? {
                    Value::FALSE => {}
                    // Deleted: the callback is `getattr(h, '_cancelled')`,
                    // so its `AttributeError` is reported, as when
                    // `Handle._run` read it (with a handle of its own: `h`'s
                    // repr reads `_cancelled` too).
                    Value::UNDEFINED => {
                        let k = self.intern("getattr")?;
                        let g = self.dict_get(self.builtins, k)?.unwrap_or(Value::NONE);
                        let name = self.intern("_cancelled")?;
                        let args = self.alloc_tuple(&[h, name])?;
                        let context = self.context_copy()?;
                        return self
                            .start_in_slot(g, args, Value::NONE, context, g, args)
                            .map(Some);
                    }
                    _ => return Ok(None),
                }
                // As CPython's `Handle._run`: in the handle's own context.
                let none = |v: Value| {
                    if v == Value::UNDEFINED {
                        Value::NONE
                    } else {
                        v
                    }
                };
                let f = none(get(self, "_callback")?);
                let args = none(get(self, "_args")?);
                let context = none(get(self, "_ctx")?);
                // A context the program replaced with a non-dict: the call
                // is `_mm.ctx_swap(context)`, whose `TypeError` is reported
                // as the callback's, as `Handle._run`'s swap raised it.
                if context != Value::NONE && !context.is_obj_kind(ty::DICT as u8) {
                    let k = self.intern("_mm")?;
                    let m = self.import(k, false)?;
                    let k = self.intern("ctx_swap")?;
                    let swap = self.module_attr(m, k)?.unwrap_or(Value::NONE);
                    let call_args = self.alloc_tuple(&[context])?;
                    return self
                        .start_in_slot(f, args, h, Value::NONE, swap, call_args)
                        .map(Some);
                }
                (f, args, h, context)
            }
            Ready::Task(_) | Ready::Gather(..) => return Ok(None),
        };
        // A method of a Python function: its function, called with the
        // receiver first.
        let (callee, call_args) = if f.is_obj_kind(ty::METHOD as u8)
            && self.vslot(f, 1).is_obj_kind(ty::FUNC as u8)
            && let Some((r, n)) = self.seq_slots(args)
        {
            let mut items = Vec::new();
            if items.try_reserve_exact(n + 1).is_err() {
                return Err(self.err(T::MemoryError, "callback arguments allocation failed"));
            }
            items.push(self.vslot(f, 0));
            items.extend(self.values_of(r, n));
            (self.vslot(f, 1), self.alloc_tuple(&items)?)
        } else {
            (f, args)
        };
        self.start_in_slot(f, args, h, context, callee, call_args)
            .map(Some)
    }

    /// [`Vm::start_callback`]'s callback `f(*args)` of handle `h` (or
    /// `None`) started in a task slot of its own, in `context`, with
    /// `callee(*call_args)` the call `_aio_callback` makes.
    #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
    fn start_in_slot(
        &mut self,
        f: Value,
        args: Value,
        h: Value,
        context: Value,
        callee: Value,
        call_args: Value,
    ) -> Result<State, Raised> {
        let func = self.prelude_fn("_aio_callback")?;
        let from = self.task_hint.min(self.tasks.len());
        let i = self.tasks[from..]
            .iter()
            .position(Option::is_none)
            .map_or(self.tasks.len(), |k| from + k);
        if i == self.tasks.capacity() {
            self.check_parked_bytes();
            self.reserve_scheduler(i.max(2) * 2, self.gathers.len(), 0)?;
        }
        let task = Task::new(Value::NONE, Value::NONE);
        if i == self.tasks.len() {
            self.tasks.push(Some(task));
        } else {
            self.tasks[i] = Some(task);
        }
        self.task_hint = i + 1;
        self.cur = i;
        self.depth = 0;
        self.specials.clear();
        self.handled = self.run_handled;
        self.context = context;
        let st = self.enter_callback(func, [f, args, h], callee, call_args);
        if st.is_err() {
            self.tasks[i] = None;
            self.task_hint = self.task_hint.min(i);
        }
        st
    }

    /// `_aio_callback(f, args, h)`'s frame at its `CALL_EX` of `f(*args)`,
    /// with `callee(*call_args)` in the call's registers (what the
    /// instructions before it compute); a Python function's frame entered
    /// above it ([`Vm::start_callback`]).
    #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
    fn enter_callback(
        &mut self,
        func: Value,
        params: [Value; 3],
        callee: Value,
        call_args: Value,
    ) -> Result<State, Raised> {
        use mm_compile::bytecode::{instr_len, op, operand};
        let mut st = self.bottom_call(func, &params)?;
        // Its instructions are read here, not in the dispatch loop: a
        // restore may have left them out (`Vm::load_bytecode`).
        self.load_bytecode(st.ci)?;
        // Arguments that are not a list or tuple (a handle's `_args` the
        // program replaced): `_aio_callback` runs from its start, and its
        // `f(*args)` raises the `TypeError`, reported as the callback's.
        if self.seq_slots(call_args).is_none() {
            return Ok(st);
        }
        let bc = &self.codes[st.ci as usize].code.bytecode;
        let mut pc = 0;
        while let Some(&w) = bc.get(pc) {
            if crate::interp::op_of(w) == op::CALL_EX {
                break;
            }
            pc += instr_len(crate::interp::op_of(w));
        }
        if pc >= bc.len() {
            return Err(self.err(T::SystemError, "micro-monty: _aio_callback has no call"));
        }
        let r = st.base + operand(bc, pc, 0);
        self.stack[r..r + 3].copy_from_slice(&[callee, call_args, Value::NONE]);
        st.pc = pc;
        if callee.is_obj_kind(ty::FUNC as u8) {
            let ret_pc = pc + instr_len(op::CALL_EX);
            let mut at = st.clone();
            match self.call_ex(&mut at, r, pc, ret_pc) {
                Ok(None) => return Ok(at),
                // A generator or coroutine function returns its object at
                // once.
                Ok(Some(v)) => {
                    st.acc = v;
                    st.pc = ret_pc;
                }
                // No room: the start's failure.
                Err(Raised) if self.exc_type(self.exc) == T::MemoryError => return Err(Raised),
                // A call that does not bind (`TypeError`) or recurses too
                // deep is the callback's own error: the `CALL_EX` raises it
                // again when the slot runs, in `_aio_callback`'s `try`,
                // which reports it.
                Err(Raised) => self.exc = Value::NONE,
            }
        }
        Ok(st)
    }

    /// `asyncio.sleep(delay, result=None)`.
    #[cfg(feature = "asyncio")]
    pub(crate) fn sleep(&mut self, delay: Value, result: Value) -> Result<Value, Raised> {
        let d = match (self.int_of(delay), delay.as_float()) {
            (Some(i), _) => i as f64,
            (None, Some(f)) => f,
            _ => {
                return Err(self.err(
                    T::TypeError,
                    &format!(
                        "'<=' not supported between instances of '{}' and 'int'",
                        self.type_name(delay)
                    ),
                ));
            }
        };
        if d.is_nan() {
            return Err(self.err(T::ValueError, "Invalid delay: NaN (not a number)"));
        }
        let result = if result == Value::UNDEFINED {
            Value::NONE
        } else {
            result
        };
        self.new_awaitable(ty::SLEEP, result, Value::float(d), aw::PENDING, 0)
    }

    /// Is `asyncio.run` refused here (inside a coroutine or a task)?
    #[cfg(feature = "asyncio")]
    #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
    pub(crate) fn in_event_loop(&self, st: &State) -> bool {
        let is_coro = |c: Value| c.is_obj_kind(ty::CORO as u8) || c.is_obj_kind(ty::AGEN as u8);
        self.cur != 0 || is_coro(st.coro) || self.specials.iter().any(|f| is_coro(f.coro))
    }

    /// Starts `asyncio.run`'s coroutine `c` from `st` (a call from
    /// `call_pc` returning to `ret_pc`): its frame runs as the first step
    /// of its loop's first iteration. What can fail (its task's handle,
    /// the frame's entry) comes first and changes nothing when it does,
    /// so an error is raised in the caller's frame, which `st` still is
    /// (`Vm::landing`): the handle was allocated after the entry, and one
    /// refused left `st` in the coroutine's frame with the caller's pc
    /// and registers, whose handlers then ran on them [M].
    #[cfg(feature = "asyncio")]
    #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
    pub(crate) fn start_run(
        &mut self,
        st: &mut State,
        c: Value,
        call_pc: usize,
        ret_pc: usize,
    ) -> Result<(), Raised> {
        // Its task is `Task-1` of a first run, as CPython numbers them.
        let counter = self.task_counter;
        let started = self
            .new_task_handle(Value::NONE, 0, Value::NONE)
            .and_then(|h| {
                self.start_coro_frame(st, c, call_pc, ret_pc, crate::cont::Cont::None)
                    .map(|()| h)
            });
        let handle = match started {
            Ok(h) => h,
            Err(e) => {
                self.task_counter = counter;
                return Err(e);
            }
        };
        let s = self.raw_of_hot(c, coro::STATE);
        self.set_raw_of_hot(c, coro::STATE, s | coro::RUN_ROOT);
        self.iter_left = 0;
        self.iter_open = true;
        self.stop_wait = 0;
        self.loop_flags |= crate::vm::LOOP_IN_RUN;
        // The run's caller's handler stays open beneath the loop.
        self.run_handled = self.handled;
        if let Some(Some(t)) = self.tasks.get_mut(0) {
            t.handle = handle;
        }
        if s & coro::RUN_ANON != 0 {
            self.task_counter = self.task_counter.saturating_sub(1);
        }
        Ok(())
    }

    /// Marks `asyncio.run`'s coroutine `c` as a `run_until_complete()`.
    /// (`anon`: its task takes no number).
    #[cfg(feature = "asyncio")]
    #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
    pub(crate) fn mark_run_open(&mut self, c: Value, anon: bool) {
        let s = self.raw_of(c, coro::STATE) | coro::RUN_OPEN;
        let s = if anon { s | coro::RUN_ANON } else { s };
        self.set_raw_of(c, coro::STATE, s);
    }

    /// `_aio_first(aws)`: an awaitable settled with the first of `aws`
    /// (tasks or futures) to finish, at once if one already has.
    #[cfg(feature = "asyncio")]
    pub(crate) fn first_of(&mut self, items: &[Value]) -> Result<Value, Raised> {
        // `_mm` is importable: check every item before reading its slots.
        if !items
            .iter()
            .all(|c| matches!(c.obj_kind().map(u16::from), Some(ty::TASK | ty::FUTURE)))
        {
            return Err(self.err(T::TypeError, "a task or future is required"));
        }
        let t = self.alloc_tuple(items)?;
        let g = self.new_awaitable(ty::GATHER, Value::NONE, t, aw::PENDING, FIRST)?;
        if let Some(c) = self.first_done(g) {
            self.settle(g, true, c);
            return Ok(g);
        }
        for &c in items {
            self.add_waiter(c, g)?;
        }
        let capacity = self.gathers_room();
        self.reserve_scheduler(self.tasks.len(), capacity, items.len())?;
        // Every item is pending and holds one waiter entry per place it
        // has in `items`: what the gather waits for (`callback_queue`),
        // and the notifications it has to run.
        let n = items.len() as u64;
        self.set_raw_of(g, aw::LEFT, n << aw::UNRUN_SHIFT | n);
        self.track_gather(g);
        self.callback_queue += items.len();
        Ok(g)
    }

    /// The finished child of gather `g` that settled first.
    #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
    fn first_done(&self, g: Value) -> Option<Value> {
        let children = self.tuple_items(self.vslot(g, aw::EXTRA));
        children
            .into_iter()
            .filter(|&c| self.raw_of(c, aw::STATE) != aw::PENDING)
            .min_by_key(|&c| self.raw_of(c, aw::SEQ))
    }

    /// `_aio_deadline(delay)`: a sleep that waits for pending host calls
    /// before virtual time reaches it.
    #[cfg(feature = "asyncio")]
    #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
    pub(crate) fn deadline(&mut self, delay: Value) -> Result<Value, Raised> {
        let s = self.sleep(delay, Value::UNDEFINED)?;
        self.set_raw_of(s, aw::LEFT, 1);
        Ok(s)
    }

    /// `asyncio.Future()`: a future the loop settles (`_aio_set`).
    #[cfg(feature = "asyncio")]
    #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
    pub(crate) fn loop_future(&mut self) -> Result<Value, Raised> {
        self.new_awaitable(
            ty::FUTURE,
            Value::NONE,
            Value::NONE,
            aw::PENDING,
            LOOP_FUTURE,
        )
    }

    /// Is `v` a future the loop settles (`asyncio.Future()`)?
    #[cfg(feature = "asyncio")]
    pub(crate) fn is_loop_future(&self, v: Value) -> bool {
        v.is_obj_kind(ty::FUTURE as u8) && self.raw_of(v, aw::AUX) == LOOP_FUTURE
    }

    /// Settles a pending loop future; `false` if it was already done, and
    /// `MemoryError`, with nothing changed, if its callbacks have no room.
    #[cfg(feature = "asyncio")]
    #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
    pub(crate) fn settle_pending(&mut self, a: Value, ok: bool, v: Value) -> Result<bool, Raised> {
        if self.raw_of(a, aw::STATE) != aw::PENDING {
            return Ok(false);
        }
        let k = self.done_callbacks(a);
        if k != 0 {
            self.reserve_scheduler(self.tasks.len(), self.gathers.len(), k)?;
        }
        self.settle(a, ok, v);
        Ok(true)
    }

    /// Whether `a`'s done callbacks get their run-queue room when it
    /// settles rather than when they are registered: a loop future, which
    /// only the program settles (`set_result`, `cancel`), so its settle can
    /// raise `MemoryError`, and which can outlive its cell. Tasks, gathers
    /// and host futures settle where nothing may fail (a task's end, a
    /// host's answer), so their callbacks' room is reserved as they register
    /// and held in `cb_slots` until the cell ends.
    fn pays_at_settle(&self, a: Value) -> bool {
        a.is_obj_kind(ty::FUTURE as u8) && self.raw_of(a, aw::AUX) >= STOP_FUTURE
    }

    /// Whether awaitable `a` belongs to the running cell: a task in the task
    /// table, a gather the scheduler tracks, a host future whose call is
    /// pending in this cell (call ids are never reused in a session). One
    /// from an earlier cell holds no run-queue room: `end_cell` released
    /// it with `cb_slots`.
    fn live(&self, a: Value) -> bool {
        match a.obj_kind().map(u16::from) {
            Some(ty::TASK) => self.task_index(a).is_some(),
            Some(ty::GATHER) => self.tracked(a),
            Some(ty::FUTURE) => {
                let id = self.raw_of(a, aw::AUX);
                matches!(self.futures.binary_search_by_key(&id, |&(id, _)| id),
                    Ok(i) if self.futures[i].1 == a)
            }
            _ => false,
        }
    }

    /// How many done callbacks wait on `a` (its waiters that are neither a
    /// task's wakeup nor a gather), counted in place.
    #[cfg(feature = "asyncio")]
    fn done_callbacks(&self, a: Value) -> usize {
        let w = self.vslot(a, aw::WAITERS);
        let callable = |x: Value| x.as_int().is_none() && !x.is_obj_kind(ty::GATHER as u8);
        (0..self.waiter_count(w))
            .filter(|&i| callable(self.waiter_at(w, i)))
            .count()
    }

    /// The pending awaitable state as `asyncio.Future` reports it: 0
    /// pending, 1 done, 2 failed.
    #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
    pub(crate) fn aw_state(&self, a: Value) -> u64 {
        self.raw_of(a, aw::STATE)
    }

    /// An awaitable's kind-specific value (a task's coroutine).
    #[cfg(feature = "asyncio")]
    pub(crate) fn aw_extra(&self, a: Value) -> Value {
        self.vslot(a, aw::EXTRA)
    }

    /// The result or exception of a settled awaitable.
    #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
    pub(crate) fn aw_result(&self, a: Value) -> Value {
        self.vslot(a, aw::RESULT)
    }

    /// `x.cancel(msg)` for a task, future or gather `root`, as CPython's:
    /// a pending task passes it to what it waits on (a task, a gather's
    /// children, a future, a sleep) and, when that cannot take it, raises
    /// `CancelledError(msg)` at its next step; a future or sleep settles
    /// cancelled and wakes what waits on it; a gather cancels its children.
    ///
    /// Two passes over the tree ([`Vm::cancel_walk`]): the first changes
    /// nothing, counting the done callbacks the settles will queue whose
    /// room is not held (a loop future's, an earlier cell's); one
    /// reservation pays for them all, and only then the second acts. So a
    /// cancel that raises (`MemoryError`, `RecursionError`) has changed
    /// nothing, and its fuel is linear in the tree.
    ///
    /// Both passes in this step when they fit one ([`CANCEL_STEP`]):
    /// whether anything took the cancel. Else a state (`nstep::op::CANCEL`)
    /// for the steps that go on ([`Vm::cancel_step`]), the walk's frames,
    /// as deep as the tree, in it; nothing has changed yet. `kind`
    /// ([`cancel_for`]) and `owner` say who goes on when it ends.
    fn cancel_begin(
        &mut self,
        root: Value,
        msg: Value,
        kind: u64,
        owner: Value,
    ) -> Result<Begun, Raised> {
        use crate::nstep::op;
        let mut w = self.cancel_new(root, msg)?;
        let mut budget = CANCEL_STEP;
        if let Walked::Counted = self.cancel_walk(&mut w, &mut budget)? {
            let short = w.rest <= budget;
            self.cancel_turn(&mut w)?;
            if short {
                // The second pass fits what the step has left, by the
                // first's count: it needs no state.
                budget = u64::MAX;
                let done = self.cancel_walk(&mut w, &mut budget)?;
                return Ok(Begun::Done(matches!(done, Walked::Done(true))));
            }
        }
        let buf = self.alloc_buf(w.room())?;
        let frames = Value::from_ref(buf, ty::BUF as u8);
        let raw = [u64::from(op::CANCEL), 0, 0, 0, 0, kind];
        let state = self.nstate(op::CANCEL, &[root, msg, frames, owner], &raw)?;
        self.cancel_save(state, buf, &w);
        Ok(Begun::State(state))
    }

    /// `cancel(msg)` as the program calls it: its value, or the rest of a
    /// long one handed to `_steps`, a step a call.
    pub(crate) fn cancel_start(&mut self, root: Value, msg: Value) -> Result<Value, Raised> {
        match self.cancel_begin(root, msg, cancel_for::CALL, Value::NONE)? {
            Begun::Done(took) => Ok(Value::from_bool(took)),
            Begun::State(state) => {
                self.prelude_request("_steps", alloc::vec![state], crate::cont::Cont::None)
            }
        }
    }

    /// A cancel the scheduler makes, between tasks (`kind` not
    /// [`cancel_for::CALL`]): a long one goes on a step at a time before
    /// the next queue entry runs ([`Vm::cancel_resume`]), and what follows
    /// it ([`Vm::cancel_end`]) waits for its end. No safepoint comes
    /// between this first step and the next, so the longest gap is two
    /// steps.
    fn cancel_sched(&mut self, root: Value, msg: Value, kind: u64, owner: Value) {
        let exc = self.exc;
        let took = match self.cancel_begin(root, msg, kind, owner) {
            Ok(Begun::Done(took)) => Some(took),
            Ok(Begun::State(state)) => {
                self.cancelling = state;
                return;
            }
            Err(Raised) => None,
        };
        self.cancel_end(kind, root, owner, took, exc);
    }

    /// The scheduler's cancel ended: `took`, or `None` when it raised with
    /// nothing changed. `exc` is the pending exception as it began.
    fn cancel_end(&mut self, kind: u64, root: Value, owner: Value, took: Option<bool>, exc: Value) {
        match (kind, took) {
            // What the task waits on took the cancel it had to raise.
            (cancel_for::AWAIT, Some(true)) => self.set_raw_of(owner, aw::MUST_CANCEL, 0),
            // No room to cancel what it waits on: it raises
            // `CancelledError` at its `await` at once instead.
            (cancel_for::AWAIT, None) => {
                self.heap.take_refused();
                if let Some(i) = self.task_index(owner) {
                    self.wake_blocked(i);
                }
            }
            // No room: the main task raises `CancelledError` at its
            // `await` at once, and the run raises the escape after.
            (cancel_for::ESCAPE, None) => {
                self.heap.take_refused();
                self.set_must_cancel(root, Value::NONE);
                self.wake_blocked(0);
            }
            _ => {}
        }
        self.exc = exc;
    }

    /// The scheduler's cancel in progress ([`Vm::cancelling`]) goes on, a
    /// safepoint after each step with the trampoline's state (every task
    /// is suspended): `None` once it has ended, to go on scheduling, or a
    /// park. The safepoints run no tick, so they charge nothing.
    fn cancel_resume(&mut self) -> Option<Next> {
        use crate::nstep::Step;
        let state = self.cancelling;
        let exc = self.exc;
        let took = loop {
            match self.cancel_step(state, true) {
                Ok(Step::More) => {
                    let st = self.idle_state();
                    if let Some(r) = self.safepoint(&st) {
                        self.loop_flags |= crate::vm::LOOP_SKIP_PARK;
                        return Some(Next::Park(r, st));
                    }
                }
                Ok(Step::Done(v)) => break Some(v == Value::TRUE),
                Err(Raised) => break None,
            }
        };
        self.cancelling = Value::NONE;
        if took.is_none() && self.exc_type(self.exc) == T::SystemError {
            // Not a state a cancel wrote (a forged image): the main task
            // raises it at its `await`.
            let e = self.exc;
            return self.raise_at_main(e, false);
        }
        if let Some(([root, _, _, owner], raw)) =
            self.nstate_get::<4, 6>(crate::nstep::op::CANCEL, state)
        {
            self.cancel_end(raw[5], root, owner, took, exc);
        }
        None
    }

    /// The next step of a cancel in `state` ([`Vm::cancel_begin`]), the
    /// scheduler's (`sched`) or a call's: its value once the second pass
    /// ends. A step of the first pass that raises has changed nothing, its
    /// state included (a call's runs again after a collection,
    /// `Vm::oom_retry`); a step of the second does not raise on a state a
    /// cancel wrote. Every step forces the check that follows it: the
    /// second pass charges nothing (the first charged each visit, as a
    /// cancel in one step does), and a first-pass step of a few costly
    /// visits charges less than the 64 units that force one (`Vm::burn`),
    /// so a deadline or a park request would wait for the tick period to
    /// end, hundreds of steps on.
    pub(crate) fn cancel_step(
        &mut self,
        state: Value,
        sched: bool,
    ) -> Result<crate::nstep::Step, Raised> {
        use crate::nstep::Step;
        let Some(mut w) = self.cancel_load(state, sched) else {
            return Err(self.bad_step());
        };
        if w.act && w.callbacks != 0 {
            // The room the second pass's settles queue into: a restore
            // gave the run queue what it held (`Vm::queue_spare`), so
            // this reserves nothing on a state a cancel wrote.
            self.reserve_scheduler(self.tasks.len(), self.gathers.len(), w.callbacks)?;
        }
        let mut budget = CANCEL_STEP;
        let walked = self.cancel_walk(&mut w, &mut budget);
        self.force_check();
        match walked? {
            Walked::Done(took) => return Ok(Step::Done(Value::from_bool(took))),
            Walked::Counted | Walked::Spent => {}
        }
        // Room for the frames first: as the first pass ends, for the
        // deepest the second will hold, so no later step's state can fail
        // to fit.
        let held = self
            .vslot(state, 2)
            .as_obj()
            .filter(|&b| self.obj_len(b) as usize >= w.room());
        let buf = match held {
            Some(b) => b,
            None => self.alloc_buf(w.room())?,
        };
        if !w.act && w.frames.is_empty() {
            self.cancel_turn(&mut w)?;
        }
        self.cancel_save(state, buf, &w);
        Ok(Step::More)
    }

    /// A cancel of `root` before its first pass.
    fn cancel_new(&mut self, root: Value, msg: Value) -> Result<CancelWalk, Raised> {
        let mut frames = Vec::new();
        if frames.try_reserve(1).is_err() {
            return Err(self.err(T::MemoryError, "cancel allocation failed"));
        }
        frames.push(Frame::root(root));
        Ok(CancelWalk {
            root,
            msg,
            act: false,
            frames,
            callbacks: 0,
            deepest: 1,
            rest: 0,
        })
    }

    /// The first pass has ended: room for the callbacks the second will
    /// queue and for its frames, then the second pass from the root.
    fn cancel_turn(&mut self, w: &mut CancelWalk) -> Result<(), Raised> {
        if w.frames.try_reserve(w.deepest).is_err() {
            return Err(self.err(T::MemoryError, "cancel allocation failed"));
        }
        if w.callbacks != 0 {
            self.reserve_scheduler(self.tasks.len(), self.gathers.len(), w.callbacks)?;
        }
        w.act = true;
        w.frames.push(Frame::root(w.root));
        Ok(())
    }

    /// Walks `w` on until `budget` is spent, depth first, children in
    /// order: CPython's call order, an awaitable reached twice visited
    /// twice. The first pass changes nothing: it charges a fuel unit a
    /// visit, counts what the settles will queue and checks the depth. The
    /// second acts on each awaitable before its children, and an awaitable
    /// reports to its parent once its children have reported to it: a
    /// gather took the cancel if a child did; a task whose waiter did not
    /// raises at its next step. Nothing else runs between the passes or
    /// their steps, and the second pass's settles change no task's waiter
    /// and no pending gather's children, so both walk the same tree.
    fn cancel_walk(&mut self, w: &mut CancelWalk, budget: &mut u64) -> Result<Walked, Raised> {
        let limit = self.recursion_limit as usize;
        while let Some(&f) = w.frames.last() {
            let Some(c) = self.cancel_child(&f, f.next) else {
                w.frames.pop();
                if !w.act {
                    continue;
                }
                if f.took && f.node.is_obj_kind(ty::GATHER as u8) {
                    let n = self.raw_of(f.node, aw::FAILS);
                    self.set_raw_of(f.node, aw::FAILS, n | aw::CANCEL_REQUESTED);
                }
                match w.frames.last_mut() {
                    Some(p) => self.cancel_report(p, f.took, w.msg),
                    None => return Ok(Walked::Done(f.took)),
                }
                continue;
            };
            if *budget == 0 {
                return Ok(Walked::Spent);
            }
            let pending = self.raw_of(c, aw::STATE) == aw::PENDING;
            let below = if pending { self.cancel_below(c) } else { None };
            let (scan, wake, k) = self.settle_work(c, pending);
            let took = if w.act {
                // As deep as the first pass went, which checked the depth
                // and whose count the state's room is: deeper is a state
                // no cancel wrote. Checked before the visit acts.
                if below.is_some() && w.frames.len() >= w.deepest {
                    return Err(self.bad_step());
                }
                *budget = budget.saturating_sub(scan.saturating_add(wake));
                if pending && self.pays_room(c) {
                    // Its callbacks queue into the room the first pass
                    // counted; a state that counted less is not a cancel's.
                    if k > w.callbacks {
                        return Err(self.bad_step());
                    }
                    w.callbacks -= k;
                }
                self.cancel_act(c, pending, below.is_some(), w.msg)
            } else {
                self.burn(1);
                if w.frames.len() - 1 > limit {
                    return Err(self.err(T::RecursionError, "maximum recursion depth exceeded"));
                }
                // A future reached twice counts twice: room to spare.
                if pending && self.pays_room(c) {
                    w.callbacks = w.callbacks.saturating_add(k);
                }
                *budget = budget.saturating_sub(scan);
                w.rest = w.rest.saturating_add(scan.saturating_add(wake));
                false
            };
            if let Some(top) = w.frames.last_mut() {
                top.next += 1;
            }
            match below {
                Some(src) => {
                    if w.frames.try_reserve(1).is_err() {
                        return Err(self.err(T::MemoryError, "cancel allocation failed"));
                    }
                    w.frames.push(Frame {
                        node: c,
                        src,
                        next: 0,
                        took,
                    });
                    w.deepest = w.deepest.max(w.frames.len());
                }
                None if w.act => {
                    if let Some(p) = w.frames.last_mut() {
                        self.cancel_report(p, took, w.msg);
                    }
                }
                None => {}
            }
        }
        Ok(Walked::Counted)
    }

    /// Child `i` of frame `f`, if it has one.
    fn cancel_child(&self, f: &Frame, i: usize) -> Option<Value> {
        if f.node.is_obj_kind(ty::GATHER as u8) {
            (i < self.tuple_len(f.src)).then(|| self.vslot(f.src, u32::try_from(i).unwrap_or(0)))
        } else {
            (i == 0).then_some(f.src)
        }
    }

    /// What pending awaitable `c` passes a cancel to: a task what it waits
    /// on now (`_fut_waiter`: nothing while it runs or before it starts), a
    /// gather (not `as_completed`'s or `wait_for`'s waiter) its children.
    fn cancel_below(&self, c: Value) -> Option<Value> {
        match c.obj_kind().map(u16::from) {
            Some(ty::TASK) => {
                let w = self
                    .task_index(c)
                    .and_then(|i| self.tasks.get(i))
                    .and_then(Option::as_ref)
                    .map_or(Value::NONE, |t| t.awaiting);
                // One that awaited itself waits on nothing: its step is
                // queued, and takes the cancel.
                (w != c
                    && matches!(
                        w.obj_kind().map(u16::from),
                        Some(ty::TASK | ty::FUTURE | ty::GATHER | ty::SLEEP)
                    ))
                .then_some(w)
            }
            Some(ty::GATHER) if self.raw_of(c, aw::AUX) != FIRST => Some(self.vslot(c, aw::EXTRA)),
            _ => None,
        }
    }

    /// Whether a cancel settling future `c` queues its done callbacks into
    /// room the cancel reserves (a loop future's, an earlier cell's).
    fn pays_room(&self, c: Value) -> bool {
        c.is_obj_kind(ty::FUTURE as u8)
            && self.raw_of(c, aw::AUX) != STOP_FUTURE
            && (self.pays_at_settle(c) || !self.live(c))
    }

    /// The work of a cancel's visit of `c` (`pending` as it is reached), in
    /// [`CANCEL_STEP`]'s unit: what either pass reads (one, and the
    /// waiters of an awaitable the second pass settles, 8 a unit), and
    /// what the settle then scans besides (`Vm::settle_as`). A sleep's
    /// looks at every blocked task (5 ns each [M]); a future's or a waiter
    /// gather's takes each task it wakes out of the blocked list (under
    /// 1 ns a task blocked [M]) and looks each gather that waits on it up
    /// among the tracked ones, where a waiter gather is looked up too.
    fn settle_work(&self, c: Value, pending: bool) -> (u64, u64, usize) {
        // The tasks blocked, not the list's holes: an image holds none, so
        // a restored cancel must charge as the one that went on did.
        let blocked = (self.blocked.len() - self.blocked_holes) as u64;
        let gathers = (self.gathers.len() - self.gathers_holes) as u64;
        match c.obj_kind().map(u16::from) {
            Some(ty::SLEEP) if self.raw_of(c, aw::STATE) == aw::ARMED => (1, blocked / 16, 0),
            Some(kind @ (ty::FUTURE | ty::GATHER))
                if pending && (kind == ty::FUTURE || self.raw_of(c, aw::AUX) == FIRST) =>
            {
                let w = self.vslot(c, aw::WAITERS);
                let n = self.waiter_count(w) as usize;
                let (mut wakes, mut listens) = (0, u64::from(kind == ty::GATHER));
                for i in 0..n as u32 {
                    let x = self.waiter_at(w, i);
                    wakes += u64::from(x.is_int());
                    listens += u64::from(x.is_obj_kind(ty::GATHER as u8));
                }
                // Its done callbacks too (`Vm::done_callbacks`: the
                // waiters that neither wake a task nor are a gather), from
                // the same read: a second one doubled a first pass's time
                // on a future with many waiters reached many times [M].
                let waiting = listens - u64::from(kind == ty::GATHER);
                let callbacks = n - usize::try_from(wakes + waiting).unwrap_or(n);
                let wake = wakes
                    .saturating_mul(blocked / 128)
                    .saturating_add(listens.saturating_mul(gathers / 16));
                (1 + n as u64 / 8, wake, callbacks)
            }
            // A task that must raise the cancel itself hands its message
            // to the gathers that wait on it (`Vm::set_must_cancel`), one
            // read of its waiters.
            Some(ty::TASK) if pending => {
                let w = self.vslot(c, aw::WAITERS);
                (1 + u64::from(self.waiter_count(w)) / 8, 0, 0)
            }
            _ => (1, 0, 0),
        }
    }

    /// The second pass's act on `c` (`pending` as it was reached;
    /// `passes_on`: a task whose waiter is in the tree): whether it took
    /// the cancel.
    fn cancel_act(&mut self, c: Value, pending: bool, passes_on: bool, msg: Value) -> bool {
        match c.obj_kind().map(u16::from) {
            Some(ty::TASK) if pending => {
                let n = self.raw_of(c, aw::CANCELS);
                if n & aw::COUNT != aw::COUNT {
                    self.set_raw_of(c, aw::CANCELS, n + 1);
                }
                // A task whose waiter is not in the tree raises at its
                // next step.
                if !passes_on {
                    self.set_must_cancel(c, msg);
                }
                true
            }
            Some(ty::GATHER) if pending && self.raw_of(c, aw::AUX) == FIRST => {
                // `as_completed`'s and `wait_for`'s waiter future.
                self.untrack_gather(c);
                self.settle_cancelled(c, msg, Value::NONE);
                true
            }
            Some(ty::FUTURE) if pending && self.raw_of(c, aw::AUX) != STOP_FUTURE => {
                let id = self.raw_of(c, aw::AUX);
                self.settle_cancelled(c, msg, Value::NONE);
                if id < STOP_FUTURE {
                    // The host call it stands for is abandoned.
                    self.settle_call(id, crate::host::CallStatus::Abandoned);
                }
                true
            }
            Some(ty::SLEEP) if self.raw_of(c, aw::STATE) == aw::ARMED && !self.sleep_due(c) => {
                self.settle_cancelled(c, msg, Value::NONE);
                true
            }
            _ => false,
        }
    }

    /// A child that took the cancel (`took`) or did not reports to its
    /// parent's frame `p`.
    fn cancel_report(&mut self, p: &mut Frame, took: bool, msg: Value) {
        if p.node.is_obj_kind(ty::TASK as u8) {
            if !took {
                self.set_must_cancel(p.node, msg);
            }
        } else {
            p.took |= took;
        }
    }

    /// Writes `w` to its state: the frames into `buf`, which has their
    /// room, then the words.
    fn cancel_save(&mut self, state: Value, buf: Ref, w: &CancelWalk) {
        for (k, f) in (0u32..).step_by(FRAME_WORDS).zip(&w.frames) {
            let next = i64::try_from(f.next).ok().and_then(Value::int);
            let words = [
                f.node,
                next.unwrap_or(Value::NONE),
                Value::from_bool(f.took),
            ];
            let _ = self.heap.set_values(buf, k, &words);
        }
        if let Some(r) = state.as_obj() {
            self.set_slot(r, 2, Value::from_ref(buf, ty::BUF as u8));
        }
        let raw = [
            u64::from(w.act),
            w.callbacks as u64,
            w.deepest as u64,
            w.frames.len() as u64,
        ];
        self.nstate_put(state, 1, &raw);
    }

    /// The cancel in `state` (the scheduler's if `sched`), each frame checked against the tree as a
    /// step left it: the root's first, then each the pending task or
    /// gather the frame under it visited last, its children read from it.
    /// `None` for anything a cancel did not write (a forged or stale
    /// image).
    fn cancel_load(&self, state: Value, sched: bool) -> Option<CancelWalk> {
        let ([root, msg, frames, owner], [_, act, callbacks, deepest, n, kind]) =
            self.nstate_get::<4, 6>(crate::nstep::op::CANCEL, state)?;
        // Who goes on after it: the scheduler's has a kind of its own, and
        // only a task that awaits has an owner, that task.
        let owned = if kind == cancel_for::AWAIT {
            owner.is_obj_kind(ty::TASK as u8)
        } else {
            owner == Value::NONE
        };
        if kind > cancel_for::ESCAPE || (kind != cancel_for::CALL) != sched || !owned {
            return None;
        }
        let kind = root.obj_kind().map(u16::from);
        if act > 1
            || !matches!(kind, Some(ty::TASK | ty::FUTURE | ty::GATHER | ty::SLEEP))
            || !frames.is_obj_kind(ty::BUF as u8)
        {
            return None;
        }
        let n = usize::try_from(n).ok().filter(|&n| n != 0)?;
        let words = self.heap.slot_words(frames.as_obj()?).ok()?;
        let words = words.get(..n.checked_mul(FRAME_WORDS)?)?;
        let mut w = CancelWalk {
            root,
            msg,
            act: act != 0,
            frames: Vec::new(),
            // A first pass may count more callbacks than any queue has
            // room for (a future with many, reached many times): the
            // reservation refuses them with `MemoryError`, as a cancel in
            // one step's does.
            callbacks: usize::try_from(callbacks).unwrap_or(usize::MAX),
            // A depth past any heap's objects is no cancel's, and sizes
            // no reservation.
            deepest: u32::try_from(deepest).ok()? as usize,
            rest: 0,
        };
        w.frames.try_reserve(n.max(w.deepest.min(1 << 16))).ok()?;
        for f in words.as_chunks::<FRAME_WORDS>().0 {
            let node = Value::from_bits(f[0])?;
            let next = usize::try_from(Value::from_bits(f[1])?.as_int()?).ok()?;
            let took = Value::from_bits(f[2])?;
            let src = match w.frames.last() {
                None if node == Value::NONE => root,
                None => return None,
                Some(p) => {
                    let visited = p.next.checked_sub(1)?;
                    if self.cancel_child(p, visited) != Some(node)
                        || self.raw_of(node, aw::STATE) != aw::PENDING
                    {
                        return None;
                    }
                    self.cancel_below(node)?
                }
            };
            let len = if node.is_obj_kind(ty::GATHER as u8) {
                self.tuple_len(src)
            } else {
                1
            };
            if next > len || ![Value::TRUE, Value::FALSE].contains(&took) {
                return None;
            }
            w.frames.push(Frame {
                node,
                src,
                next,
                took: took == Value::TRUE,
            });
        }
        Some(w)
    }

    /// Task `h` raises `CancelledError(msg)` at its next step.
    #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
    fn set_must_cancel(&mut self, h: Value, msg: Value) {
        self.set_raw_of(h, aw::MUST_CANCEL, 1);
        if let Some(r) = h.as_obj() {
            self.set_slot(r, aw::RESULT, msg);
        }
        // The message stays with the task when it ends some other way
        // than cancelled (`Task._cancel_message`), and its result takes
        // the slot: a pending gather whose last child it is keeps the
        // message in its own `RESULT`, for the error it raises if it was
        // cancelled ([`Vm::check_gather`]).
        let w = self.vslot(h, aw::WAITERS);
        for k in 0..self.waiter_count(w) {
            let g = self.waiter_at(w, k);
            if g.is_obj_kind(ty::GATHER as u8)
                && self.gather_last_child(g) == h
                && let Some(r) = g.as_obj()
            {
                self.set_slot(r, aw::RESULT, msg);
            }
        }
    }

    /// The last listed child of gather `g` (`None` for a `FIRST` waiter
    /// future, or one with no children).
    fn gather_last_child(&self, g: Value) -> Value {
        if self.raw_of(g, aw::AUX) == FIRST {
            return Value::NONE;
        }
        let children = self.vslot(g, aw::EXTRA);
        match u32::try_from(self.tuple_len(children)) {
            Ok(n) if n != 0 => self.vslot(children, n - 1),
            _ => Value::NONE,
        }
    }

    /// `Task.cancelling()`.
    #[cfg(feature = "asyncio")]
    #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
    pub(crate) fn cancelling(&self, h: Value) -> u64 {
        self.raw_of(h, aw::CANCELS) & aw::COUNT
    }

    /// `Task.uncancel()`: one cancel request fewer; none left drops a
    /// pending one.
    #[cfg(feature = "asyncio")]
    #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
    pub(crate) fn uncancel(&mut self, h: Value) -> u64 {
        let w = self.raw_of(h, aw::CANCELS);
        let n = w & aw::COUNT;
        if n > 0 {
            self.set_raw_of(h, aw::CANCELS, w - 1);
            // A done task's word is its held `CancelledError` or its
            // unretrieved mark, not a cancel to drop.
            if n == 1 && self.raw_of(h, aw::STATE) == aw::PENDING {
                self.set_raw_of(h, aw::MUST_CANCEL, 0);
            }
        }
        n.saturating_sub(1)
    }

    /// The running task's handle (`asyncio.current_task()`); the main
    /// task gets one when first asked.
    #[cfg(feature = "asyncio")]
    pub(crate) fn current_task(&mut self) -> Result<Value, Raised> {
        let i = self.cur;
        let h = self
            .tasks
            .get(i)
            .and_then(Option::as_ref)
            .map_or(Value::NONE, |t| t.handle);
        if h != Value::NONE || i != 0 {
            // A callback has no task.
            return Ok(h);
        }
        let h = self.new_task_handle(Value::NONE, i, Value::NONE)?;
        if let Some(Some(t)) = self.tasks.get_mut(i) {
            t.handle = h;
        }
        Ok(h)
    }

    /// The main coroutine of `asyncio.run` ended: its task settles (a
    /// pending cancel makes a return a `CancelledError`, raised), and the
    /// next `asyncio.run` gets a new one.
    #[cfg(feature = "asyncio")]
    #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
    pub(crate) fn main_ended(&mut self, ok: bool, v: Value) -> Result<Value, Raised> {
        let h = match self.tasks.first_mut() {
            Some(Some(t)) if self.cur == 0 => core::mem::replace(&mut t.handle, Value::NONE),
            _ => Value::NONE,
        };
        if h == Value::NONE {
            return Ok(v);
        }
        self.end_task(h, ok, v);
        match self.raw_of(h, aw::STATE) {
            aw::CANCELLED if ok => {
                self.exc = self.cancelled_error(h)?;
                Err(Raised)
            }
            _ => Ok(v),
        }
    }

    /// The pending tasks other than the running main task, by slot.
    #[cfg(feature = "asyncio")]
    #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
    pub(crate) fn other_tasks(&self) -> Vec<Value> {
        self.all_tasks(1)
    }

    /// The program's pending tasks from slot `from` on (`asyncio.all_tasks`
    /// with 0), not the scheduler's own or callbacks.
    #[cfg(feature = "asyncio")]
    #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
    pub(crate) fn all_tasks(&self, from: usize) -> Vec<Value> {
        self.tasks
            .iter()
            .skip(from)
            .flatten()
            .map(|t| t.handle)
            .filter(|&h| {
                h.is_obj_kind(ty::TASK as u8)
                    && self.raw_of(h, aw::STATE) == aw::PENDING
                    && self.raw_of(h, aw::CANCELS) & aw::INTERNAL == 0
            })
            .collect()
    }

    /// `Task.get_name()`: a name the program set, else `Task-N`.
    #[cfg(feature = "asyncio")]
    #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
    pub(crate) fn task_name(&mut self, h: Value) -> Result<Value, Raised> {
        if self.task_names != Value::NONE
            && let Some(n) = self.dict_get(self.task_names, h)?
        {
            return Ok(n);
        }
        let n = self.raw_of(h, aw::CANCELS) >> aw::NUMBER_SHIFT;
        self.new_str(&format!("Task-{n}"))
    }

    /// `Task.set_name(name)` (with `str(name)` already taken). Names live
    /// in a dict of the cell, which keeps their tasks alive until it ends.
    #[cfg(feature = "asyncio")]
    #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
    pub(crate) fn set_task_name(&mut self, h: Value, name: Value) -> Result<(), Raised> {
        if self.task_names == Value::NONE {
            self.task_names = self.new_dict()?;
        }
        let d = self.task_names;
        self.dict_set(d, h, name)
    }

    /// A copy of the running context for a new task (`None` stays `None`).
    pub(crate) fn context_copy(&mut self) -> Result<Value, Raised> {
        if self.context.is_obj_kind(ty::DICT as u8) {
            self.dict_copy(self.context)
        } else {
            Ok(Value::NONE)
        }
    }

    /// The running context's dict, made on first use.
    #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
    pub(crate) fn context_dict(&mut self) -> Result<Value, Raised> {
        if !self.context.is_obj_kind(ty::DICT as u8) {
            self.context = self.new_dict()?;
        }
        Ok(self.context)
    }

    /// Task `h`'s context (before it first runs): `d`.
    #[cfg(feature = "asyncio")]
    #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
    pub(crate) fn set_task_context(&mut self, h: Value, d: Value) {
        let i = usize::try_from(self.raw_of(h, aw::AUX)).unwrap_or(usize::MAX);
        if let Some(Some(t)) = self.tasks.get_mut(i)
            && t.handle == h
        {
            t.context = d;
        }
    }

    /// The virtual clock (`loop.time()`).
    #[cfg(feature = "asyncio")]
    #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
    pub(crate) fn loop_time(&self) -> f64 {
        self.vclock
    }

    /// Is a loop running here (`get_running_loop()` succeeds)?
    #[cfg(feature = "asyncio")]
    #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
    pub(crate) fn loop_running(&self) -> bool {
        let is_coro = |c: Value| c.is_obj_kind(ty::CORO as u8) || c.is_obj_kind(ty::AGEN as u8);
        self.cur != 0 || self.loop_flags != 0 || self.specials.iter().any(|f| is_coro(f.coro))
    }

    /// `_aio_stop(n)`: a future the main task awaits until the loop has
    /// run `n` more iterations to their end, or has nothing left to run
    /// (CPython's `run_until_complete` stopping after the callback that
    /// ends it).
    #[cfg(feature = "asyncio")]
    #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
    pub(crate) fn stop_future(&mut self, n: u32) -> Result<Value, Raised> {
        if self.cur != 0 {
            return Err(self.err(T::RuntimeError, "_aio_stop() outside the main task"));
        }
        let f = self.new_awaitable(
            ty::FUTURE,
            Value::NONE,
            Value::NONE,
            aw::PENDING,
            STOP_FUTURE,
        )?;
        self.stop_wait = n.max(1);
        Ok(f)
    }

    /// The loop stopped: the main task, blocked on its stop future, runs
    /// next, outside any iteration.
    #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
    fn loop_stopped(&mut self) -> Option<State> {
        let a = self.tasks.first().and_then(Option::as_ref)?.awaiting;
        if !(a.is_obj_kind(ty::FUTURE as u8)
            && self.raw_of(a, aw::AUX) == STOP_FUTURE
            && self.raw_of(a, aw::STATE) == aw::PENDING)
        {
            return None;
        }
        if !self.unblock(0) {
            return None;
        }
        self.set_raw_of(a, aw::STATE, aw::DONE);
        self.activate(0).ok().flatten()
    }

    /// Queue entry `entry` could not start: no room for its task slot,
    /// stack or context (`MemoryError` pending). It goes back first in its
    /// iteration, so no accepted callback or task is dropped. The first time,
    /// the scheduler waits for a full collection, as an instruction that
    /// ran out of memory does (`Vm::oom_retry`), held at a safepoint with
    /// no guest code (`LOOP_START_WAIT`, in the image), and the start is
    /// tried once more; `None` then. A second failure (`waited`) raises the
    /// error in the main task at its `await` ([`Vm::raise_at_main`]).
    fn start_failed(&mut self, entry: Ready, waited: bool) -> Option<Next> {
        if !matches!(entry, Ready::Task(_)) {
            self.callback_queue += 1;
        }
        self.ready.push_front(entry);
        self.iter_left += 1;
        if !waited && self.exc_type(self.exc) == T::MemoryError && self.room_may_help() {
            self.exc = Value::NONE;
            self.loop_flags |= crate::vm::LOOP_START_WAIT;
            self.collect_for_retry();
            return self.hold_for_start().map(|(r, st)| Next::Park(r, st));
        }
        self.heap.take_refused();
        let e = self.exc;
        self.raise_at_main(e, true)
    }

    /// The running task ended with the pending exception. A callback's slot
    /// (no handle) has no one to retrieve it: what leaves `_aio_callback`
    /// is what it could not report (`MemoryError` in the report itself) or
    /// does not catch (`SystemExit`, `KeyboardInterrupt`), and it goes to
    /// the main task's `await` ([`Vm::raise_at_main`]) unless it ended an
    /// `asyncio.run` already (`Vm::escape`). `None`: go on scheduling.
    pub(crate) fn task_failed(&mut self) -> Option<Next> {
        let e = self.exc;
        // Anything else leaving `_aio_callback` came from its report (an
        // exception handler or `sys.stderr` that raises), which CPython's
        // `call_exception_handler` swallows: dropped, as there.
        let callback = self.cur != 0
            && self
                .tasks
                .get(self.cur)
                .and_then(Option::as_ref)
                .is_some_and(|t| t.handle == Value::NONE)
            && [T::MemoryError, T::SystemExit, T::KeyboardInterrupt]
                .into_iter()
                .any(|t| self.instance_of(e, self.type_obj(t)));
        // The scheduler's own tasks (a timer's wait, `_aio_later`) fail
        // only for room, or when their timer is cancelled.
        #[cfg(not(feature = "asyncio"))]
        let internal = false;
        #[cfg(feature = "asyncio")]
        let internal = !callback
            && self.cur != 0
            && self
                .tasks
                .get(self.cur)
                .and_then(Option::as_ref)
                .is_some_and(|t| {
                    t.handle.is_obj_kind(ty::TASK as u8)
                        && self.raw_of(t.handle, aw::CANCELS) & aw::INTERNAL != 0
                })
            && !self.instance_of(e, self.type_obj(T::CancelledError));
        self.finish_task(false, e);
        if !(callback || internal) || self.escape == e {
            return None;
        }
        self.heap.take_refused();
        self.raise_at_main(e, false)
    }

    /// Runs the collection a failed start waits for, a step per safepoint
    /// with no guest code between steps; a park leaves the rest owed. The
    /// state the safepoints see is the trampoline's, with no registers:
    /// every task is suspended, so their saved states are the roots.
    pub(crate) fn hold_for_start(&mut self) -> Option<(ParkReason, State)> {
        let st = self.idle_state();
        while self.gc_waiting() {
            if let Some(r) = self.safepoint(&st) {
                return Some((r, st));
            }
        }
        None
    }

    /// Whether the scheduler makes a check before what it runs next: every
    /// `SCHED_TURNS` task turns and gather notifications, or sooner after
    /// native work past `SCHED_BURNED` or with the fuel spent. A task can
    /// start and block without passing an instruction's safepoint. This
    /// check charges only ticks already run.
    fn sched_due(&mut self) -> bool {
        self.sched_turns += 1;
        if self.sched_turns < SCHED_TURNS && self.burned <= SCHED_BURNED && self.fuel != Some(0) {
            return false;
        }
        self.sched_turns = 0;
        true
    }

    /// The registers of the scheduler between tasks: the trampoline's, with
    /// no frame of a task's.
    fn idle_state(&mut self) -> State {
        self.cur = 0;
        // What the last task left in the VM stack is dead.
        self.stack.fill(Value::UNDEFINED);
        State {
            pc: 0,
            base: 0,
            ci: self.tramp,
            acc: Value::NONE,
            func: Value::NONE,
            coro: Value::NONE,
        }
    }

    /// A parked session resumes the collection a failed start waits for
    /// ([`Vm::start_failed`]), then the scheduler tries the start again;
    /// after a skipped handle's park (`LOOP_SKIP_PARK`) no collection is
    /// held, and the scheduler goes on.
    pub(crate) fn resume_start_wait(&mut self) -> Exit {
        self.loop_flags &= !crate::vm::LOOP_SKIP_PARK;
        if let Some((r, st)) = self.hold_for_start() {
            self.saved = Some(st);
            return Exit::Parked(r);
        }
        self.run_next()
    }

    /// Error `e` came from the loop itself (a queue entry that cannot start,
    /// an exception leaving a callback): it is raised in the main task at
    /// its `await`, where the guest can catch it. In `asyncio.run` after
    /// the run's coroutine has ended (its teardown), nothing awaits for the
    /// guest: the run raises it when it ends (`Vm::escape`) and the entry
    /// waiting first (`requeued`) goes, as the run ends with the error.
    /// `None`: go on scheduling.
    fn raise_at_main(&mut self, e: Value, requeued: bool) -> Option<Next> {
        let teardown = self.loop_flags & crate::vm::LOOP_IN_RUN != 0
            && self
                .tasks
                .first()
                .and_then(Option::as_ref)
                .is_some_and(|t| t.handle == Value::NONE);
        if teardown {
            if self.escape == Value::NONE {
                self.escape = e;
            }
            if requeued {
                if self
                    .ready
                    .pop_front()
                    .is_some_and(|r| !matches!(r, Ready::Task(_)))
                {
                    self.callback_queue -= 1;
                }
                self.iter_left = self.iter_left.saturating_sub(1);
            }
            return None;
        }
        let Some(st) = self.wake_main() else {
            // Every task but the running one is in `ready` or `blocked`.
            debug_assert!(false, "the loop failed with no main task");
            return Some(Next::Wait(Vec::new()));
        };
        self.exc = e;
        Some(Next::Raise(st))
    }

    /// Takes the main task out of the run queue or the blocked list and
    /// activates it, to raise the pending error at its `await` (a step that
    /// cannot go on without room). `None` if it is in neither: it runs, or
    /// has ended, so no task could have been waiting on the scheduler.
    #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
    fn wake_main(&mut self) -> Option<State> {
        if !self.unblock(0) {
            let k = self
                .ready
                .iter()
                .position(|r| matches!(r, Ready::Task(0)))?;
            self.burn(k as u64);
            self.ready.remove(k);
            if k < self.iter_left {
                self.iter_left -= 1;
            }
        }
        let exc = self.exc;
        let st = self.activate(0).ok().flatten();
        self.exc = exc;
        st
    }

    /// The index of the task running task handle `h`.
    #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
    pub(crate) fn task_index(&self, h: Value) -> Option<usize> {
        if !h.is_obj_kind(ty::TASK as u8) {
            return None;
        }
        let i = usize::try_from(self.raw_of_hot(h, aw::AUX)).ok()?;
        matches!(self.tasks.get(i), Some(Some(t)) if t.handle == h).then_some(i)
    }

    /// A pending host future for call `id`.
    #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
    pub(crate) fn new_future(&mut self, id: u64) -> Result<Value, Raised> {
        if self.futures.len() == self.futures.capacity() {
            let extra = self.futures.len().max(4);
            self.reserve_execution(extra as u64 * size_of::<(u64, Value)>() as u64)?;
            let result = self.futures.try_reserve_exact(extra);
            self.reserve_execution(0)?;
            result.map_err(|_| self.err(T::MemoryError, "future allocation failed"))?;
        }
        let f = self.new_awaitable(ty::FUTURE, Value::NONE, Value::NONE, aw::PENDING, id)?;
        match self.futures.binary_search_by_key(&id, |&(id, _)| id) {
            Ok(i) => self.futures[i].1 = f,
            Err(i) => self.futures.insert(i, (id, f)),
        }
        Ok(f)
    }
}
