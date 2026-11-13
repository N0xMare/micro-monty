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
use crate::vm::{Exit, HDR, Raised, Ready, State, Task, Vm, fn_slot};
use alloc::{format, string::String, vec::Vec};
use mm_core::Value;

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
    /// callbacks): `None`, one entry, or a tuple of them. An entry is a
    /// gather it is a child of, a task slot (small int) blocked on it, or a
    /// callable from `add_done_callback`.
    pub const WAITERS: u32 = 2;
    pub const STATE: u32 = 3;
    pub const SEQ: u32 = 4;
    pub const AUX: u32 = 5;
    /// A gather's distinct children still pending.
    pub const LEFT: u32 = 6;
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
    /// A gather's flags: [`FAILED_CHILD`], [`CANCEL_REQUESTED`].
    pub const FAILS: u32 = 7;
    /// The same word of a pending host future: something awaited or
    /// gathered it (nonzero).
    pub const AWAITED: u32 = 7;
    /// The same word of a pending task: a cancel waits for its next step
    /// (`_must_cancel`, nonzero); of a cancelled awaitable, [`HELD`] or 0.
    pub const MUST_CANCEL: u32 = 7;
    pub const FAILED_CHILD: u64 = 1;
    /// A cancel reached a child (`_cancel_requested`).
    pub const CANCEL_REQUESTED: u64 = 2;
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
            self.set_slot(buf, u32::try_from(i).unwrap_or(0), v);
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
            self.hx(res)
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
    fn note_fresh(&mut self, c: Value) -> Result<(), Raised> {
        let f = self.vslot(c, coro::FUNC);
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
        let q = self.vslot(f, fn_slot::QUALNAME);
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
        self.raw_of(c, coro::STATE) & (3 | coro::SCHEDULED) == coro::CREATED
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
            if self.raw_of(f, aw::STATE) != aw::PENDING || self.raw_of(f, aw::AWAITED) != 0 {
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
        let mut named: Vec<(u64, &str)> =
            found.iter().map(|&(k, q)| (k, self.str_ref(q))).collect();
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
            self.hx(res)
        }?;
        Ok(Value::from_ref(r, kind as u8))
    }

    fn raw_of(&self, v: Value, i: u32) -> u64 {
        v.as_obj().map_or(0, |r| self.raw(r, i))
    }

    fn set_raw_of(&mut self, v: Value, i: u32, w: u64) {
        if let Some(r) = v.as_obj() {
            self.set_raw(r, i, w);
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
        let ok = state == aw::DONE;
        let was_pending = self.raw_of(v, aw::STATE) == aw::PENDING;
        if let Some(r) = v.as_obj() {
            self.set_slot(r, aw::RESULT, result);
            self.set_raw(r, aw::STATE, state);
            self.set_raw(r, aw::SEQ, self.seq);
        }
        // Tasks, futures and gathers list their waiters in order; tasks
        // blocked on anything else are found in `blocked`.
        if !matches!(
            v.obj_kind().map(u16::from),
            Some(ty::TASK | ty::FUTURE | ty::GATHER)
        ) {
            let mut k = 0;
            while k < self.blocked.len() {
                let t = self.blocked[k];
                if matches!(self.tasks.get(t), Some(Some(task)) if task.awaiting == v) {
                    self.blocked.remove(k);
                    self.ready.push_back(Ready::Task(t));
                } else {
                    k += 1;
                }
            }
        }
        // Gathers keep their own pending count and failure flag, so their
        // callbacks need not rescan every child.
        let waiters = self.vslot(v, aw::WAITERS);
        if let Some(r) = v.as_obj() {
            self.set_slot(r, aw::WAITERS, Value::NONE);
        }
        let single = !waiters.is_obj_kind(ty::TUPLE as u8);
        let n = if waiters == Value::NONE {
            0
        } else if single {
            1
        } else {
            self.tuple_len(waiters)
        };
        for k in 0..n {
            let g = if single {
                waiters
            } else {
                self.vslot(waiters, u32::try_from(k).unwrap_or(0))
            };
            if let Some(t) = g.as_int().and_then(|t| usize::try_from(t).ok()) {
                // A task's wakeup.
                if matches!(self.tasks.get(t), Some(Some(task)) if task.awaiting == v)
                    && let Some(j) = self.blocked.iter().position(|&b| b == t)
                {
                    self.blocked.remove(j);
                    self.ready.push_back(Ready::Task(t));
                }
                continue;
            }
            if !g.is_obj_kind(ty::GATHER as u8) {
                // `add_done_callback`: called soon with the awaitable.
                self.cb_slots = self.cb_slots.saturating_sub(1);
                self.ready.push_back(Ready::Call(g, v));
                continue;
            }
            // Gathers dropped with their `asyncio.run` no longer listen.
            if !self.gathers.contains(&g) {
                continue;
            }
            if was_pending {
                let left = self.raw_of(g, aw::LEFT);
                self.set_raw_of(g, aw::LEFT, left.saturating_sub(1));
                if !ok {
                    let f = self.raw_of(g, aw::FAILS);
                    self.set_raw_of(g, aw::FAILS, f | aw::FAILED_CHILD);
                }
            }
            self.ready.push_back(Ready::Gather(g));
        }
    }

    /// Marks a coroutine frame finished (returned or raised out).
    pub(crate) fn coro_finished(&mut self, c: Value) {
        let state = self.raw_of(c, coro::STATE);
        self.set_raw_of(c, coro::STATE, coro::DONE);
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
            // CPython's `Runner.close()` runs `shutdown_asyncgens()` and
            // `shutdown_default_executor()` as two more numbered tasks.
            if state & coro::RUN_OPEN == 0 {
                self.task_counter += 2;
            }
        }
    }

    /// The state of coroutine `c` (`asyncio.sleep()` objects count as
    /// coroutines once awaited).
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
        let state = self.raw_of(c, coro::STATE) & 3;
        let resume = state == coro::SUSPENDED
            && if cont == crate::cont::Cont::CoroSend {
                self.vslot(c, coro::AWAITER) == Value::NONE
            } else {
                self.vslot(c, coro::AWAITER) == st.coro
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
        let func = self.vslot(c, coro::FUNC);
        let cidx = u32::try_from(self.vslot_raw_of(func, fn_slot::CODE)).unwrap_or(0);
        let Some(code) = self.codes.get(cidx as usize).cloned() else {
            return Err(self.err(T::SystemError, "bad code index"));
        };
        let nregs = code.code.nregs as usize;
        let new_base = st.base + self.codes[st.ci as usize].code.nregs as usize + HDR;
        self.ensure_stack(new_base + nregs + 1)?;
        self.load_coro_args(c, new_base, nregs);
        self.enter_frame(st, func, cidx, new_base, call_pc, ret_pc, cont)?;
        let keep = self.raw_of(c, coro::STATE) & coro::RUN_BITS;
        self.set_raw_of(c, coro::STATE, coro::RUNNING | keep);
        st.coro = c;
        if let Some(r) = c.as_obj() {
            if resume {
                // Back at its `AWAIT`, which steps what it waits on.
                st.pc = usize::try_from(self.raw(r, coro::PC)).unwrap_or(0);
                st.acc = self.slot(r, coro::AWAITED);
                let _ = self
                    .heap
                    .set_values(r, coro::AWAITED, &[Value::NONE, Value::NONE]);
                if cont == crate::cont::Cont::CoroSend {
                    let (handled, outer) = (self.slot(r, coro::HANDLED), self.handled);
                    let _ = self
                        .heap
                        .set_values(r, coro::HANDLED, &[Value::NONE, outer]);
                    if handled != Value::NONE {
                        self.handled = handled;
                    }
                }
            } else if cont == crate::cont::Cont::CoroSend {
                self.set_slot(r, coro::OUTER, self.handled);
            }
        }
        Ok(())
    }

    /// The iterator an `__await__()` (or a `types.coroutine` generator)
    /// gave, which an `AWAIT` steps until it returns.
    pub(crate) fn new_delegation(&mut self, it: Value) -> Result<Value, Raised> {
        let b = self.alloc_buf(1)?;
        self.set_slot(b, 0, it);
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
        let mut driven = false;
        let mut c = st.coro;
        for f in &frames {
            if !c.is_obj_kind(ty::CORO as u8) {
                break;
            }
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
        let mut awaited = w;
        let mut at = pc;
        loop {
            let c = st.coro;
            let nregs = self.codes[st.ci as usize].code.nregs as usize;
            let mut buf = self.vslot(c, coro::ARGS);
            let len = buf
                .as_obj()
                .filter(|_| buf.is_obj_kind(ty::BUF as u8))
                .and_then(|b| self.heap.header(b).ok())
                .map_or(0, |h| h.nvals() as usize);
            if len < nregs {
                let b = self.alloc_buf(nregs.max(1))?;
                buf = Value::from_ref(b, ty::BUF as u8);
            }
            let Some(r) = c.as_obj() else {
                return Err(self.err(T::SystemError, "micro-monty: bad coroutine"));
            };
            if let (Some(b), Some(regs)) = (buf.as_obj(), self.stack.get(st.base..st.base + nregs))
            {
                let regs: Vec<Value> = regs.to_vec();
                let _ = self.heap.set_values(b, 0, &regs);
            }
            let _ = self.heap.set_values(r, coro::ARGS, &[buf, awaited]);
            let keep = self.raw(r, coro::STATE) & coro::RUN_BITS;
            self.set_raw(r, coro::STATE, coro::SUSPENDED | keep);
            self.set_raw(r, coro::PC, at as u64);
            let Some(fr) = self.pop_frame(st) else {
                return Err(self.err(T::SystemError, "micro-monty: lost send() frame"));
            };
            if self.cont_take(fr.cont) == Cont::CoroSend {
                let outer = self.slot(r, coro::OUTER);
                let inner = self.handled;
                let _ = self
                    .heap
                    .set_values(r, coro::HANDLED, &[inner, Value::NONE]);
                self.handled = outer;
                st.acc = y;
                st.pc = fr.ret_pc as usize;
                return Ok(Landed::Go);
            }
            self.set_slot(r, coro::AWAITER, st.coro);
            awaited = c;
            at = fr.call_pc as usize;
        }
    }

    pub(crate) fn vslot_raw_of(&self, v: Value, i: u32) -> u64 {
        v.as_obj().map_or(0, |r| self.raw(r, i))
    }

    /// Copies bound arguments to `base`; keep the original until frame
    /// admission succeeds, so an allocation failure does not consume them.
    fn load_coro_args(&mut self, c: Value, base: usize, nregs: usize) {
        let buf = self.vslot(c, coro::ARGS);
        for i in 0..nregs {
            self.stack[base + i] = self.vslot(buf, u32::try_from(i).unwrap_or(0));
        }
    }

    // ---------------------------------------------------------- await

    /// One `AWAIT` of `aw` at `pc`.
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
                let it = self.vslot(a, 0);
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
                    self.set_raw_of(a, aw::AWAITED, 1);
                }
                match self.raw_of(a, aw::STATE) {
                    aw::PENDING => {
                        // The task's wakeup, in line with the other done
                        // callbacks.
                        let me =
                            Value::int(i64::try_from(self.cur).unwrap_or(0)).unwrap_or(Value::NONE);
                        self.add_waiter(a, me)?;
                        Ok(Awaited::Block)
                    }
                    aw::DONE => Ok(Awaited::Value(self.vslot(a, aw::RESULT))),
                    aw::CANCELLED => {
                        self.exc = self.cancelled_error(a)?;
                        Err(Raised)
                    }
                    _ => {
                        self.retrieved(a);
                        self.exc = self.vslot(a, aw::RESULT);
                        Err(Raised)
                    }
                }
            }
            ty::SLEEP => match self.raw_of(a, aw::STATE) {
                aw::PENDING => {
                    self.arm_sleep(a);
                    Ok(Awaited::Block)
                }
                aw::ARMED if self.sleep_due(a) => {
                    self.set_raw_of(a, aw::STATE, aw::SPENT);
                    Ok(Awaited::Value(self.vslot(a, aw::RESULT)))
                }
                aw::ARMED => Ok(Awaited::Block),
                aw::CANCELLED => {
                    self.exc = self.cancelled_error(a)?;
                    self.set_raw_of(a, aw::STATE, aw::SPENT);
                    Err(Raised)
                }
                _ => Err(self.err(T::RuntimeError, REUSED)),
            },
            _ => {
                let key = self.intern("__await__")?;
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

    fn not_awaitable(&mut self, a: Value) -> Raised {
        let msg = format!("'{}' object can't be awaited", self.type_name(a));
        self.err(T::TypeError, &msg)
    }

    fn sleep_due(&self, a: Value) -> bool {
        self.vclock >= f64::from_bits(self.raw_of(a, aw::AUX))
    }

    // ------------------------------------------------------ scheduler

    /// Pending child notifications plus callbacks already queued. Duplicate
    /// child references overestimate pending notifications, never undercount.
    fn callback_slots(&self) -> usize {
        let queued = self
            .ready
            .iter()
            .filter(|r| matches!(r, Ready::Gather(_) | Ready::Call(..)))
            .count()
            .saturating_add(self.cb_slots);
        self.gathers.iter().fold(queued, |n, &g| {
            let children = self.vslot(g, aw::EXTRA);
            n.saturating_add(
                (0..self.tuple_len(children))
                    .filter(|&k| {
                        let c = self.vslot(children, u32::try_from(k).unwrap_or(0));
                        self.raw_of(c, aw::STATE) == aw::PENDING
                    })
                    .count(),
            )
        })
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
        let extra = (tasks - self.tasks.capacity()) as u64 * size_of::<Option<Task>>() as u64
            + (gathers - self.gathers.capacity()) as u64 * size_of::<Value>() as u64
            + (blocked - self.blocked.capacity()) as u64 * size_of::<usize>() as u64
            + (ready - self.ready.capacity()) as u64 * size_of::<Ready>() as u64;
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
        self.tasks.push(Some(Task {
            handle: Value::NONE,
            stack: Vec::new(),
            depth: 0,
            specials: Vec::new(),
            st: None,
            handled: Value::NONE,
            awaiting: Value::NONE,
            context: Value::NONE,
        }));
        self.cur = 0;
        Ok(())
    }

    /// Ends the cell that completed or raised: reports what it never
    /// awaited, then drops its frames, tasks and futures.
    pub(crate) fn finish_cell(&mut self) {
        self.warn_unawaited();
        self.end_cell();
    }

    /// Drops every frame, task and future of the running cell; host calls
    /// it left unanswered are abandoned.
    pub(crate) fn end_cell(&mut self) {
        self.check_parked_bytes();
        self.abandon_calls();
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
        self.gathers.clear();
        self.iter_left = 0;
        self.iter_open = false;
        self.stop_wait = 0;
        self.gc_wait = 0;
        self.oom_rerun = u64::MAX;
        self.cb_slots = 0;
        self.task_names = Value::NONE;
        self.escape = Value::NONE;
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
        let _ = self.reserve_execution(0);
    }

    /// Ends every task but the main one, which is running (when the
    /// coroutine `asyncio.run` started returns or raises). The program's
    /// tasks are done by then; the scheduler's own (timers: `call_later`'s,
    /// `wait_for`'s, `as_completed`'s) end cancelled and their coroutines
    /// closed, as CPython's closing loop drops its scheduled handles.
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
        self.gathers.clear();
        self.iter_left = 0;
        self.iter_open = false;
        self.stop_wait = 0;
        let _ = self.reserve_execution(0);
    }

    /// Saves the running task, blocked on `a` (queued again at once if `a`
    /// is a sleep already due, as `sleep(0)` yields). A cancel requested
    /// while it ran goes to `a` now, as CPython's `__step` does.
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
            t.st = Some(st);
            t.handled = handled;
            t.awaiting = a;
        }
        // A delegation whose iterator yielded `None` yields to the loop.
        if (a.is_obj_kind(ty::SLEEP as u8) && self.sleep_due(a)) || a.is_obj_kind(ty::BUF as u8) {
            self.ready.push_back(Ready::Task(i));
            return;
        }
        self.blocked.push(i);
        let h = self
            .tasks
            .get(i)
            .and_then(Option::as_ref)
            .map_or(Value::NONE, |t| t.handle);
        if self.must_cancel(h) {
            let msg = self.vslot(h, aw::RESULT);
            if matches!(self.cancel_tree(a, msg), Ok(true)) {
                self.set_raw_of(h, aw::MUST_CANCEL, 0);
            }
        }
    }

    /// Does task handle `h` (or `None`) have a cancel waiting for its next
    /// step?
    fn must_cancel(&self, h: Value) -> bool {
        h.is_obj_kind(ty::TASK as u8)
            && self.raw_of(h, aw::STATE) == aw::PENDING
            && self.raw_of(h, aw::MUST_CANCEL) != 0
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
            if log && self.raw_of(h, aw::STATE) == aw::FAILED {
                self.note_unretrieved(h);
            }
        }
        if escapes {
            self.escape = result;
            if let Some(Some(t)) = self.tasks.first()
                && t.handle.is_obj_kind(ty::TASK as u8)
            {
                let h = t.handle;
                let saved = self.exc;
                let _ = self.cancel_tree(h, Value::NONE);
                self.exc = saved;
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
    fn gathered(&self, a: Value) -> bool {
        let w = self.vslot(a, aw::WAITERS);
        let all = if w.is_obj_kind(ty::TUPLE as u8) {
            self.tuple_items(w)
        } else {
            alloc::vec![w]
        };
        all.into_iter()
            .any(|g| g.is_obj_kind(ty::GATHER as u8) && self.raw_of(g, aw::AUX) != FIRST)
    }

    /// Failed `a` reports \"exception was never retrieved\" at the end of
    /// `asyncio.run` unless something retrieves it first.
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
    pub(crate) fn retrieved(&mut self, a: Value) {
        if self.raw_of(a, aw::STATE) == aw::FAILED {
            self.set_raw_of(a, aw::MUST_CANCEL, 0);
        }
    }

    /// The failed tasks and futures whose exception nobody retrieved, in
    /// the order they failed (the list empties).
    #[cfg(feature = "asyncio")]
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
        self.set_raw_of(h, aw::MUST_CANCEL, 0);
        if ok && must {
            let msg = self.vslot(h, aw::RESULT);
            self.settle_cancelled(h, msg, Value::NONE);
        } else if !ok && self.instance_of(result, self.type_obj(T::CancelledError)) {
            self.settle_cancelled(h, Value::NONE, result);
        } else {
            self.settle(h, ok, result);
        }
    }

    /// A new `CancelledError(msg)`, or `CancelledError()` for `None`.
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
            self.hx(res)
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
            let s = self.raw_of(c, coro::STATE);
            self.set_raw_of(c, coro::STATE, s | coro::SCHEDULED);
        }
        // A task runs in a copy of its creator's context.
        let context = self.context_copy()?;
        let task = Task {
            handle: h,
            stack: Vec::new(),
            depth: 0,
            specials: Vec::new(),
            st: None,
            handled: Value::NONE,
            awaiting: Value::NONE,
            context,
        };
        if i == self.tasks.len() {
            self.tasks.push(Some(task));
        } else {
            self.tasks[i] = Some(task);
        }
        self.ready.push_back(Ready::Task(i));
        Ok(h)
    }

    fn arm_sleep(&mut self, a: Value) {
        let delay = self.vslot(a, aw::EXTRA).as_float().unwrap_or(0.0);
        let wake = self.vclock + delay.max(0.0);
        self.set_raw_of(a, aw::AUX, wake.to_bits());
        self.set_raw_of(a, aw::STATE, aw::ARMED);
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
        let saved = t.st.take();
        // Task buffers move without a new charge; replacing the previous
        // task's completed buffers releases their reservation.
        self.reserve_execution(0)?;
        if let Some(st) = saved {
            return Ok(Some(st));
        }
        let c = self.vslot(handle, aw::EXTRA);
        if c.is_obj_kind(ty::SLEEP as u8) {
            // A sleep's task steps as the `asyncio.sleep()` coroutine does:
            // the first arms it and yields (to the loop for a delay of 0),
            // the next one ends the task.
            let s = self.raw_of(c, aw::STATE);
            if s == aw::PENDING {
                self.arm_sleep(c);
                if let Some(Some(t)) = self.tasks.get_mut(i) {
                    t.awaiting = c;
                }
                if self.sleep_due(c) {
                    self.ready.push_back(Ready::Task(i));
                } else {
                    self.blocked.push(i);
                }
                return Ok(None);
            }
            if s == aw::ARMED && self.sleep_due(c) {
                self.set_raw_of(c, aw::STATE, aw::SPENT);
                let r = self.vslot(c, aw::RESULT);
                self.finish_task(true, r);
                return Ok(None);
            }
            if s == aw::CANCELLED {
                let e = self.cancelled_error(c)?;
                self.set_raw_of(c, aw::STATE, aw::SPENT);
                self.finish_task(false, e);
                return Ok(None);
            }
            return Err(self.err(T::RuntimeError, REUSED));
        }
        if self.raw_of(c, coro::STATE) & 3 != coro::CREATED {
            return Err(self.err(T::RuntimeError, REUSED));
        }
        let func = self.vslot(c, coro::FUNC);
        let cidx = u32::try_from(self.vslot_raw_of(func, fn_slot::CODE)).unwrap_or(0);
        let Some(code) = self.codes.get(cidx as usize).cloned() else {
            return Err(self.err(T::SystemError, "bad code index"));
        };
        let nregs = code.code.nregs as usize;
        self.ensure_stack(nregs + 64)?;
        self.load_coro_args(c, 0, nregs);
        if !code.code.freevars.is_empty() {
            let closure = self.vslot(func, fn_slot::CLOSURE);
            for (k, &r) in code.code.freevars.iter().enumerate() {
                self.stack[r as usize] = self.vslot(closure, u32::try_from(k).unwrap_or(0));
            }
        }
        self.set_raw_of(c, coro::STATE, coro::RUNNING);
        if let Some(r) = c.as_obj() {
            self.set_slot(r, coro::ARGS, Value::NONE);
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

    /// A gather's done callback: settles it if its children allow.
    fn check_gather(&mut self, g: Value) {
        if self.raw_of(g, aw::STATE) != aw::PENDING {
            return;
        }
        if self.raw_of(g, aw::AUX) == FIRST {
            if let Some(c) = self.first_done(g) {
                self.gathers.retain(|&x| x != g);
                self.settle(g, true, c);
            }
            return;
        }
        let keep_errors = self.raw_of(g, aw::AUX) != 0;
        let flags = self.raw_of(g, aw::FAILS);
        let fails = !keep_errors && flags & aw::FAILED_CHILD != 0;
        if !fails && self.raw_of(g, aw::LEFT) != 0 {
            return;
        }
        // The gather settles now: one pass over its children.
        let children = self.tuple_items(self.vslot(g, aw::EXTRA));
        if fails {
            let mut first: Option<(u64, Value)> = None;
            for &c in &children {
                if matches!(self.raw_of(c, aw::STATE), aw::FAILED | aw::CANCELLED) {
                    let s = self.raw_of(c, aw::SEQ);
                    if first.is_none_or(|(f, _)| s < f) {
                        first = Some((s, c));
                    }
                }
            }
            if let Some((_, c)) = first {
                self.gathers.retain(|&x| x != g);
                let e = self.child_error(c);
                self.settle(g, false, e);
                return;
            }
        }
        self.gathers.retain(|&x| x != g);
        if flags & aw::CANCEL_REQUESTED != 0 {
            // `gather(..., return_exceptions=True)` cancelled still raises:
            // CPython's `_make_cancelled_error()` of the last child.
            let last = children.last().copied().unwrap_or(Value::NONE);
            let e = if self.raw_of(last, aw::STATE) == aw::CANCELLED {
                self.child_error(last)
            } else {
                self.new_cancelled(Value::NONE).unwrap_or(self.exc)
            };
            self.settle(g, false, e);
            return;
        }
        let mut results: Vec<Value> = Vec::with_capacity(children.len());
        for &c in &children {
            let v = if self.raw_of(c, aw::STATE) == aw::CANCELLED {
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
                self.vslot(c, aw::RESULT)
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
                let i = match entry {
                    Ready::Task(i) => i,
                    Ready::Gather(g) => {
                        self.check_gather(g);
                        continue;
                    }
                    Ready::Call(f, arg) => {
                        if let Ok(st) = self.start_callback(f, arg) {
                            return Next::Run(st);
                        }
                        // No room to run it: CPython would report the
                        // callback's failure and go on.
                        continue;
                    }
                };
                let Some(Some(t)) = self.tasks.get(i) else {
                    continue;
                };
                let (h, waited, started) = (t.handle, t.awaiting, t.st.is_some());
                // A task settled before it ran: free its slot.
                if i != 0 && !started && self.raw_of(h, aw::STATE) != aw::PENDING {
                    self.tasks[i] = None;
                    self.task_hint = self.task_hint.min(i);
                    continue;
                }
                // `cancel()` asked for a `CancelledError` at this step.
                let must = self.must_cancel(h);
                if must {
                    self.set_raw_of(h, aw::MUST_CANCEL, 0);
                    // Woken by a cancelled awaitable (or a gather that one
                    // failed): its own error goes in.
                    let s = self.raw_of(waited, aw::STATE);
                    if s == aw::CANCELLED
                        || (s == aw::FAILED
                            && self.instance_of(
                                self.vslot(waited, aw::RESULT),
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
                    let msg = self.vslot(h, aw::RESULT);
                    self.new_cancelled(msg).unwrap_or(self.exc)
                } else {
                    Value::NONE
                };
                if must && !started && i != 0 {
                    // Thrown into a coroutine that never started, which
                    // closes it unrun.
                    self.cur = i;
                    let c = self.vslot(h, aw::EXTRA);
                    if c.is_obj_kind(ty::SLEEP as u8) {
                        self.set_raw_of(c, aw::STATE, aw::SPENT);
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
                    Ok(Some(st)) => return Next::Run(st),
                    Ok(None) => continue,
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
                    if a.is_obj_kind(ty::SLEEP as u8) && self.raw_of(a, aw::STATE) == aw::ARMED {
                        if *hosts.get_or_insert_with(|| !self.pending_ids(true).is_empty()) {
                            continue;
                        }
                        sleepers.push((f64::from_bits(self.raw_of(a, aw::AUX)), k));
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
                self.blocked.retain(|t| !due.contains(t));
                self.ready.extend(due.into_iter().map(Ready::Task));
                continue;
            }
            // A sleeper held back for host calls may be polling any of them
            // (`f.done()`), so then every unsettled one is asked for.
            let ids = self.pending_ids(hosts == Some(true));
            if !ids.is_empty() {
                return Next::Wait(ids);
            }
            // Nothing can ever settle: raise in the main task.
            if let Some(k) = self.blocked.iter().position(|&t| t == 0) {
                self.blocked.remove(k);
                if let Ok(Some(st)) = self.activate(0) {
                    let _ = self.err(
                        T::RuntimeError,
                        "every task is waiting and no host call is pending",
                    );
                    return Next::Raise(st);
                }
            }
            return Next::Wait(Vec::new());
        }
    }

    /// Ids of the cell's unsettled host futures, in ascending order: with
    /// `all`, every one, awaited or not (what a sleep waits for: its
    /// answer may be what a polling loop looks for); else those a blocked
    /// task waits on, directly or through gathers, or that a done callback
    /// waits on (`shield`, `wait`, `add_done_callback`). Only futures in
    /// the cell's table count: cell end clears `futures`, and the host
    /// cannot answer a call of an earlier cell.
    fn pending_ids(&self, all: bool) -> Vec<u64> {
        let pending = |id: u64| {
            matches!(self.futures.binary_search_by_key(&id, |&(id, _)| id),
                Ok(i) if self.raw_of(self.futures[i].1, aw::STATE) == aw::PENDING)
        };
        let mut ids: Vec<u64> = self
            .futures
            .iter()
            .filter(|&&(id, f)| pending(id) && (all || self.vslot(f, aw::WAITERS) != Value::NONE))
            .map(|&(id, _)| id)
            .collect();
        if all {
            return ids;
        }
        let mut work: Vec<Value> = self
            .blocked
            .iter()
            .filter_map(|&t| match self.tasks.get(t) {
                Some(Some(task)) => Some(task.awaiting),
                _ => None,
            })
            .collect();
        let mut seen: Vec<u64> = Vec::new();
        while let Some(a) = work.pop() {
            if seen.contains(&a.to_bits()) {
                continue;
            }
            seen.push(a.to_bits());
            if self.raw_of(a, aw::STATE) != aw::PENDING {
                continue;
            }
            if a.is_obj_kind(ty::FUTURE as u8) {
                let id = self.raw_of(a, aw::AUX);
                if id < STOP_FUTURE && pending(id) {
                    ids.push(id);
                }
            } else if a.is_obj_kind(ty::GATHER as u8) {
                work.extend(self.tuple_items(self.vslot(a, aw::EXTRA)));
            }
        }
        crate::ops::heap_sort_by(&mut ids, |a, b| a < b);
        ids.dedup();
        ids
    }

    // ------------------------------------------------- host futures

    /// The status of host future `id`.
    #[must_use]
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
    pub(crate) fn run_next(&mut self) -> Exit {
        match self.next_task() {
            Next::Run(st) => self.execute(st, false),
            Next::Raise(st) => self.execute(st, true),
            Next::Wait(ids) => self.wait_on(ids),
        }
    }

    /// `Exit::Waiting`, unless nothing is pending (the main task is gone,
    /// which would be an engine bug): then the cell fails.
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
                        let s = self.raw_of(c, coro::STATE);
                        self.set_raw_of(c, coro::STATE, s | coro::SCHEDULED);
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
                self.set_raw_of(a, aw::AWAITED, 1);
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
        let t = self.alloc_tuple(&children)?;
        let g = self.new_awaitable(
            ty::GATHER,
            Value::NONE,
            t,
            aw::PENDING,
            u64::from(keep_errors),
        )?;
        let mut left = 0;
        let mut fails = 0;
        for &c in &distinct {
            match self.raw_of(c, aw::STATE) {
                aw::PENDING => left += 1,
                aw::FAILED => fails = 1,
                _ => {}
            }
            self.add_waiter(c, g)?;
        }
        self.set_raw_of(g, aw::LEFT, left);
        self.set_raw_of(g, aw::FAILS, fails);
        if children.is_empty() {
            let l = self.new_list(&[])?;
            self.settle(g, true, l);
        } else {
            let capacity = if self.gathers.len() == self.gathers.capacity() {
                self.gathers.len().max(2) * 2
            } else {
                self.gathers.capacity()
            };
            self.reserve_scheduler(self.tasks.len(), capacity, children.len())?;
            self.gathers.push(g);
            // Done callbacks added to settled children run soon.
            for &c in &children {
                if self.raw_of(c, aw::STATE) != aw::PENDING {
                    self.ready.push_back(Ready::Gather(g));
                }
            }
        }
        Ok(g)
    }

    /// Lists `g` (a gather, a task slot or a callback) among what waits on
    /// awaitable `c`.
    #[inline(never)]
    fn add_waiter(&mut self, c: Value, g: Value) -> Result<(), Raised> {
        let w = self.vslot(c, aw::WAITERS);
        let mut all = if w == Value::NONE {
            Vec::new()
        } else if !w.is_obj_kind(ty::TUPLE as u8) {
            alloc::vec![w]
        } else {
            self.tuple_items(w)
        };
        // Settled gathers listen no more (each `as_completed` step leaves
        // one on every awaitable left), nor tasks no longer blocked on `c`:
        // drop them.
        self.burn(all.len() as u64);
        all.retain(|&x| {
            if x.is_obj_kind(ty::GATHER as u8) {
                return self.raw_of(x, aw::STATE) == aw::PENDING;
            }
            match x.as_int().and_then(|t| usize::try_from(t).ok()) {
                Some(t) => {
                    matches!(self.tasks.get(t), Some(Some(task)) if task.awaiting == c)
                        || (t == self.cur && x != g)
                }
                None => true,
            }
        });
        let w = if all.is_empty() {
            g
        } else {
            all.push(g);
            self.alloc_tuple(&all)?
        };
        if let Some(r) = c.as_obj() {
            self.set_slot(r, aw::WAITERS, w);
        }
        Ok(())
    }

    /// `fut.add_done_callback(fn)`: `fn(fut)` runs soon after `fut` is
    /// done, or soon if it is done already.
    #[cfg(feature = "asyncio")]
    pub(crate) fn add_done_callback(&mut self, fut: Value, f: Value) -> Result<(), Raised> {
        self.reserve_scheduler(self.tasks.len(), self.gathers.len(), 1)?;
        if self.raw_of(fut, aw::STATE) != aw::PENDING {
            self.ready.push_back(Ready::Call(f, fut));
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
    pub(crate) fn remove_done_callback(&mut self, fut: Value, f: Value) -> Result<usize, Raised> {
        let w = self.vslot(fut, aw::WAITERS);
        let all = if w == Value::NONE {
            Vec::new()
        } else if !w.is_obj_kind(ty::TUPLE as u8) {
            alloc::vec![w]
        } else {
            self.tuple_items(w)
        };
        self.burn(all.len() as u64);
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
        let keep: Vec<Value> = all
            .iter()
            .copied()
            .filter(|&x| !(callable(x) && same(self, x)))
            .collect();
        let n = all.len() - keep.len();
        if n == 0 {
            return Ok(0);
        }
        let w = match keep.len() {
            0 => Value::NONE,
            1 => keep[0],
            _ => self.alloc_tuple(&keep)?,
        };
        if let Some(r) = fut.as_obj() {
            self.set_slot(r, aw::WAITERS, w);
        }
        self.cb_slots = self.cb_slots.saturating_sub(n);
        Ok(n)
    }

    /// `loop.call_soon(fn)`: `fn()` runs at its turn in the run queue.
    #[cfg(feature = "asyncio")]
    pub(crate) fn call_soon(&mut self, f: Value) -> Result<(), Raised> {
        self.reserve_scheduler(self.tasks.len(), self.gathers.len(), 1)?;
        self.ready.push_back(Ready::Call(f, Value::UNDEFINED));
        Ok(())
    }

    /// Runs `f(arg)` (or `f()`) as a callback of the loop: in a task slot
    /// of its own with no handle, through the prelude's `_aio_callback`
    /// (which reports what it raises).
    fn start_callback(&mut self, f: Value, arg: Value) -> Result<State, Raised> {
        let from = self.task_hint.min(self.tasks.len());
        let i = self.tasks[from..]
            .iter()
            .position(Option::is_none)
            .map_or(self.tasks.len(), |k| from + k);
        if i == self.tasks.capacity() {
            self.check_parked_bytes();
            self.reserve_scheduler(i.max(2) * 2, self.gathers.len(), 0)?;
        }
        let task = Task {
            handle: Value::NONE,
            stack: Vec::new(),
            depth: 0,
            specials: Vec::new(),
            st: None,
            handled: Value::NONE,
            awaiting: Value::NONE,
            context: Value::NONE,
        };
        if i == self.tasks.len() {
            self.tasks.push(Some(task));
        } else {
            self.tasks[i] = Some(task);
        }
        self.task_hint = i + 1;
        self.cur = i;
        self.depth = 0;
        self.specials.clear();
        self.handled = Value::NONE;
        self.context = self.context_copy()?;
        let func = self.prelude_fn("_aio_callback")?;
        let st = if arg == Value::UNDEFINED {
            self.bottom_call(func, &[f])
        } else {
            self.bottom_call(func, &[f, arg])
        };
        if st.is_err() {
            self.tasks[i] = None;
            self.task_hint = self.task_hint.min(i);
        }
        st
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
    pub(crate) fn in_event_loop(&self, st: &State) -> bool {
        let is_coro = |c: Value| c.is_obj_kind(ty::CORO as u8) || c.is_obj_kind(ty::AGEN as u8);
        self.cur != 0 || is_coro(st.coro) || self.specials.iter().any(|f| is_coro(f.coro))
    }

    /// Marks `c` as `asyncio.run`'s coroutine, which runs as the first
    /// step of its loop's first iteration.
    #[cfg(feature = "asyncio")]
    pub(crate) fn mark_run_root(&mut self, c: Value) -> Result<(), Raised> {
        let s = self.raw_of(c, coro::STATE);
        self.set_raw_of(c, coro::STATE, s | coro::RUN_ROOT);
        self.iter_left = 0;
        self.iter_open = true;
        self.stop_wait = 0;
        self.loop_flags |= crate::vm::LOOP_IN_RUN;
        // Its task is `Task-1` of a first run, as CPython numbers them.
        if let Some(Some(t)) = self.tasks.get_mut(0) {
            t.handle = Value::NONE;
        }
        self.current_task()?;
        if s & coro::RUN_ANON != 0 {
            self.task_counter = self.task_counter.saturating_sub(1);
        }
        Ok(())
    }

    /// Marks `asyncio.run`'s coroutine `c` as a `run_until_complete()`.
    /// (`anon`: its task takes no number).
    #[cfg(feature = "asyncio")]
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
        let capacity = if self.gathers.len() == self.gathers.capacity() {
            self.gathers.len().max(2) * 2
        } else {
            self.gathers.capacity()
        };
        self.reserve_scheduler(self.tasks.len(), capacity, items.len())?;
        self.gathers.push(g);
        Ok(g)
    }

    /// The finished child of gather `g` that settled first.
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
    pub(crate) fn deadline(&mut self, delay: Value) -> Result<Value, Raised> {
        let s = self.sleep(delay, Value::UNDEFINED)?;
        self.set_raw_of(s, aw::LEFT, 1);
        Ok(s)
    }

    /// `asyncio.Future()`: a future the loop settles (`_aio_set`).
    #[cfg(feature = "asyncio")]
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

    /// Settles a pending awaitable; `false` if it was already done.
    #[cfg(feature = "asyncio")]
    pub(crate) fn settle_pending(&mut self, a: Value, ok: bool, v: Value) -> bool {
        if self.raw_of(a, aw::STATE) != aw::PENDING {
            return false;
        }
        self.settle(a, ok, v);
        true
    }

    /// The pending awaitable state as `asyncio.Future` reports it: 0
    /// pending, 1 done, 2 failed.
    pub(crate) fn aw_state(&self, a: Value) -> u64 {
        self.raw_of(a, aw::STATE)
    }

    /// An awaitable's kind-specific value (a task's coroutine).
    #[cfg(feature = "asyncio")]
    pub(crate) fn aw_extra(&self, a: Value) -> Value {
        self.vslot(a, aw::EXTRA)
    }

    /// The result or exception of a settled awaitable.
    pub(crate) fn aw_result(&self, a: Value) -> Value {
        self.vslot(a, aw::RESULT)
    }

    /// `x.cancel(msg)` for a task, future or gather `root`, as CPython's:
    /// a pending task passes it to what it waits on (a task, a gather's
    /// children, a future, a sleep) and, when that cannot take it, raises
    /// `CancelledError(msg)` at its next step; a future or sleep settles
    /// cancelled and wakes what waits on it; a gather cancels its children.
    /// Iterative: the awaitables reached, in CPython's call order with
    /// their parents, act first; then the results flow back to them.
    /// `false` if nothing took the cancel.
    pub(crate) fn cancel_tree(&mut self, root: Value, msg: Value) -> Result<bool, Raised> {
        // (awaitable, parent index, depth, cancelled)
        let mut nodes: Vec<(Value, usize, u32, bool)> = Vec::new();
        let mut work: Vec<usize> = Vec::new();
        let grow = |nodes: &mut Vec<(Value, usize, u32, bool)>, work: &mut Vec<usize>, n: usize| {
            nodes.try_reserve(n).is_ok() && work.try_reserve(n).is_ok()
        };
        if !grow(&mut nodes, &mut work, 1) {
            return Err(self.err(T::MemoryError, "cancel allocation failed"));
        }
        nodes.push((root, usize::MAX, 0, false));
        work.push(0);
        let limit = self.recursion_limit;
        while let Some(k) = work.pop() {
            let (v, _, depth, _) = nodes[k];
            self.burn(1);
            if depth > limit {
                return Err(self.err(T::RecursionError, "maximum recursion depth exceeded"));
            }
            let pending = self.raw_of(v, aw::STATE) == aw::PENDING;
            let took = match v.obj_kind().map(u16::from) {
                Some(ty::TASK) if pending => {
                    let n = self.raw_of(v, aw::CANCELS);
                    if n & aw::COUNT != aw::COUNT {
                        self.set_raw_of(v, aw::CANCELS, n + 1);
                    }
                    // What it waits on now (`_fut_waiter`): nothing while it
                    // runs or before it starts.
                    let w = self
                        .task_index(v)
                        .and_then(|i| self.tasks.get(i))
                        .and_then(Option::as_ref)
                        .map_or(Value::NONE, |t| t.awaiting);
                    if matches!(
                        w.obj_kind().map(u16::from),
                        Some(ty::TASK | ty::FUTURE | ty::GATHER | ty::SLEEP)
                    ) {
                        if !grow(&mut nodes, &mut work, 1) {
                            return Err(self.err(T::MemoryError, "cancel allocation failed"));
                        }
                        work.push(nodes.len());
                        nodes.push((w, k, depth + 1, false));
                    } else {
                        self.set_must_cancel(v, msg);
                    }
                    true
                }
                Some(ty::GATHER) if pending && self.raw_of(v, aw::AUX) == FIRST => {
                    // `as_completed`'s and `wait_for`'s waiter future.
                    self.gathers.retain(|&x| x != v);
                    self.settle_cancelled(v, msg, Value::NONE);
                    true
                }
                Some(ty::GATHER) if pending => {
                    let children = self.tuple_items(self.vslot(v, aw::EXTRA));
                    if !grow(&mut nodes, &mut work, children.len()) {
                        return Err(self.err(T::MemoryError, "cancel allocation failed"));
                    }
                    // Pushed last first, so they act in order.
                    let base = nodes.len();
                    for &c in &children {
                        nodes.push((c, k, depth + 1, false));
                    }
                    work.extend((base..nodes.len()).rev());
                    false
                }
                Some(ty::FUTURE) if pending && self.raw_of(v, aw::AUX) != STOP_FUTURE => {
                    let id = self.raw_of(v, aw::AUX);
                    self.settle_cancelled(v, msg, Value::NONE);
                    if id < STOP_FUTURE {
                        // The host call it stands for is abandoned.
                        self.settle_call(id, crate::host::CallStatus::Abandoned);
                    }
                    true
                }
                Some(ty::SLEEP) if self.raw_of(v, aw::STATE) == aw::ARMED && !self.sleep_due(v) => {
                    self.settle_cancelled(v, msg, Value::NONE);
                    true
                }
                _ => false,
            };
            nodes[k].3 = took;
        }
        // Children (later nodes) report to their parents: a gather took the
        // cancel if a child did; a task whose waiter did not raises at its
        // next step.
        for k in (1..nodes.len()).rev() {
            let (_, p, _, took) = nodes[k];
            let pv = nodes[p].0;
            if pv.is_obj_kind(ty::GATHER as u8) {
                nodes[p].3 |= took;
            } else if !took {
                self.set_must_cancel(pv, msg);
            }
        }
        for &(v, _, _, took) in &nodes {
            if took && v.is_obj_kind(ty::GATHER as u8) && self.raw_of(v, aw::AUX) != FIRST {
                let f = self.raw_of(v, aw::FAILS);
                self.set_raw_of(v, aw::FAILS, f | aw::CANCEL_REQUESTED);
            }
        }
        Ok(nodes[0].3)
    }

    /// Task `h` raises `CancelledError(msg)` at its next step.
    fn set_must_cancel(&mut self, h: Value, msg: Value) {
        self.set_raw_of(h, aw::MUST_CANCEL, 1);
        if let Some(r) = h.as_obj() {
            self.set_slot(r, aw::RESULT, msg);
        }
    }

    /// `Task.cancelling()`.
    #[cfg(feature = "asyncio")]
    pub(crate) fn cancelling(&self, h: Value) -> u64 {
        self.raw_of(h, aw::CANCELS) & aw::COUNT
    }

    /// `Task.uncancel()`: one cancel request fewer; none left drops a
    /// pending one.
    #[cfg(feature = "asyncio")]
    pub(crate) fn uncancel(&mut self, h: Value) -> u64 {
        let w = self.raw_of(h, aw::CANCELS);
        let n = w & aw::COUNT;
        if n > 0 {
            self.set_raw_of(h, aw::CANCELS, w - 1);
            if n == 1 {
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
    pub(crate) fn other_tasks(&self) -> Vec<Value> {
        self.all_tasks(1)
    }

    /// The program's pending tasks from slot `from` on (`asyncio.all_tasks`
    /// with 0), not the scheduler's own or callbacks.
    #[cfg(feature = "asyncio")]
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
    pub(crate) fn context_dict(&mut self) -> Result<Value, Raised> {
        if !self.context.is_obj_kind(ty::DICT as u8) {
            self.context = self.new_dict()?;
        }
        Ok(self.context)
    }

    /// Task `h`'s context (before it first runs): `d`.
    #[cfg(feature = "asyncio")]
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
    pub(crate) fn loop_time(&self) -> f64 {
        self.vclock
    }

    /// Is a loop running here (`get_running_loop()` succeeds)?
    #[cfg(feature = "asyncio")]
    pub(crate) fn loop_running(&self) -> bool {
        let is_coro = |c: Value| c.is_obj_kind(ty::CORO as u8) || c.is_obj_kind(ty::AGEN as u8);
        self.cur != 0 || self.loop_flags != 0 || self.specials.iter().any(|f| is_coro(f.coro))
    }

    /// `_aio_stop(n)`: a future the main task awaits until the loop has
    /// run `n` more iterations to their end, or has nothing left to run
    /// (CPython's `run_until_complete` stopping after the callback that
    /// ends it).
    #[cfg(feature = "asyncio")]
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
    fn loop_stopped(&mut self) -> Option<State> {
        let a = self.tasks.first().and_then(Option::as_ref)?.awaiting;
        if !(a.is_obj_kind(ty::FUTURE as u8)
            && self.raw_of(a, aw::AUX) == STOP_FUTURE
            && self.raw_of(a, aw::STATE) == aw::PENDING)
        {
            return None;
        }
        let k = self.blocked.iter().position(|&t| t == 0)?;
        self.blocked.remove(k);
        self.set_raw_of(a, aw::STATE, aw::DONE);
        self.activate(0).ok().flatten()
    }

    /// The index of the task running task handle `h`.
    pub(crate) fn task_index(&self, h: Value) -> Option<usize> {
        if !h.is_obj_kind(ty::TASK as u8) {
            return None;
        }
        let i = usize::try_from(self.raw_of(h, aw::AUX)).ok()?;
        matches!(self.tasks.get(i), Some(Some(t)) if t.handle == h).then_some(i)
    }

    /// A pending host future for call `id`.
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
