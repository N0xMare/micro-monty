//! Generators (docs/spec.md §8.3).
//!
//! A generator owns a copy of its frame: the registers (a `BUF`), the pc to
//! resume at and its handled-exception state. Resuming pushes the frame on
//! the running stack above the caller, which records a continuation for
//! what the yielded (or returned) value means to it: a `for` loop's next
//! item or exit, `next()`'s result or default, `send()`'s result, a `yield
//! from` delegation step. `YIELD` copies the registers back and pops to the
//! caller. No Rust frame ever holds a suspended generator.

use crate::cont::{Cont, Outcome};
use crate::interp::op_of;
use crate::types::{T, ty};
use crate::vm::{HDR, Raised, State, Vm, exc_slot, fn_slot};
use alloc::{format, string::String};
use mm_compile::bytecode::{instr_len, op, operand};
use mm_core::Value;

/// The pcs a frame suspended at `pc` waits at: the suspending instruction
/// (a `throw` raises there) and the one after it (a resume runs there).
pub(crate) fn suspend_pcs(code: &mm_compile::bytecode::Code, pc: u64) -> [u32; 2] {
    // Code that suspended has run, so has its instructions: a restore
    // decodes those of every suspended generator and coroutine.
    debug_assert!(!code.bytecode_deferred(), "a suspended code was decoded");
    let at = u32::try_from(pc).unwrap_or(u32::MAX);
    let len = code
        .bytecode
        .get(at as usize)
        .map_or(1, |&w| instr_len(op_of(w)));
    [at, at.saturating_add(u32::try_from(len).unwrap_or(1))]
}

/// One entry of `Vm::dead_cache`: a code index + 1, a suspended pc and the
/// registers dead where a generator suspended there waits and resumes.
pub(crate) type DeadMemo = (u32, u32, [u64; 4]);
/// Entries of `Vm::dead_cache`.
pub(crate) const DEAD_CACHE: usize = 32;
/// An empty entry of `Vm::dead_cache`: an entry keys its code as index +
/// 1, so an all-zero one matches no code.
const NO_MEMO: DeadMemo = (0, 0, [0; 4]);
/// Work units of a slice of the clearing pass before an incremental mark
/// (`Vm::clear_suspended`): a generator, or a register looked at.
pub(crate) const CLEAR_SLICE: u64 = 1 << 17;

/// The capacity [`Vm::gens`] keeps for `len` entries: `len` rounded up to
/// four significant bits (at least 8), so at most an eighth over. A
/// function of the length alone, so a session that never snapshotted, one
/// that went on after a snapshot and one restored from it are charged the
/// same for the same generators; the steps grow by an eighth or more, so
/// growing to `n` copies `O(n)` entries.
pub(crate) fn gens_room(len: usize) -> usize {
    if len <= 8 {
        return if len == 0 { 0 } else { 8 };
    }
    let shift = len.bit_width() - 4;
    (((len - 1) >> shift) + 1) << shift
}

/// Generator slots.
pub(crate) mod g {
    pub const FUNC: u32 = 0;
    /// The saved registers.
    pub const REGS: u32 = 1;
    /// The exception being handled inside the generator.
    pub const HANDLED: u32 = 2;
    /// The caller's handled exception while the generator runs.
    pub const OUTER: u32 = 3;
    pub const STATE: u32 = 4;
    /// The pc of the yield the generator is suspended at.
    pub const YPC: u32 = 5;
    pub const CODE: u32 = 6;
    /// Set in `YPC` once the copy of a suspended generator has its dead
    /// registers cleared (`Vm::clear_suspended`); every suspension writes
    /// `YPC` whole, which resets it, and every reader masks it
    /// (`tests/ypc_readers.rs` lists them).
    pub const CLEARED: u64 = 1 << 63;
    pub const CREATED: u64 = 0;
    pub const SUSPENDED: u64 = 1;
    pub const RUNNING: u64 = 2;
    pub const DONE: u64 = 3;
}

/// Slots of the awaitable an async generator's `asend()`, `athrow()` and
/// `aclose()` return.
pub(crate) mod ag {
    pub const AGEN: u32 = 0;
    /// The value sent or the exception thrown.
    pub const ARG: u32 = 1;
    pub const MODE: u32 = 2;
    /// Nonzero once awaited.
    pub const USED: u32 = 3;
    pub const SEND: u64 = 0;
    pub const THROW: u64 = 1;
    pub const CLOSE: u64 = 2;
}

/// What resuming or throwing into a generator did.
pub(crate) enum Resumed {
    /// The generator's frame was pushed; continue in it.
    Frame,
    /// The generator's frame was pushed and the pending exception is to be
    /// raised at its pc.
    Fault,
    /// The generator could not run; the caller's continuation gets this.
    Out(Outcome),
}

impl Vm {
    /// A generator for `func` (code `cidx`) whose `nregs` registers were
    /// bound at stack index `top`.
    pub(crate) fn new_gen(
        &mut self,
        func: Value,
        cidx: u32,
        top: usize,
        nregs: usize,
    ) -> Result<Value, Raised> {
        if self.gens.len() == self.gens.capacity() {
            let extra = gens_room(self.gens.len() + 1) - self.gens.len();
            self.reserve_execution((extra * size_of::<Value>()) as u64)?;
            let grown = self.gens.try_reserve_exact(extra);
            self.reserve_execution(0)?;
            grown.map_err(|_| self.err(T::MemoryError, "generator list allocation failed"))?;
        }
        let buf = self.alloc_buf(nregs.max(1))?;
        for i in 0..nregs {
            let v = self.stack[top + i];
            self.set_slot_hot(buf, u32::try_from(i).unwrap_or(0), v);
        }
        // An `async def` with `yield` makes an async generator.
        let kind = if self
            .codes
            .get(cidx as usize)
            .is_some_and(|c| c.code.is_coroutine)
        {
            ty::AGEN
        } else {
            ty::GEN
        };
        let r = {
            let res = self.heap.alloc_init(
                kind,
                0,
                &[
                    func,
                    Value::from_ref(buf, ty::BUF as u8),
                    Value::NONE,
                    Value::NONE,
                ],
                &[g::CREATED, 0, u64::from(cidx)],
            );
            self.hx_hot(res)
        }?;
        let gn = Value::from_ref(r, kind as u8);
        // Room was reserved before the allocation.
        self.gens.push(gn);
        Ok(gn)
    }

    pub(crate) fn gen_raw(&self, gn: Value, i: u32) -> u64 {
        gn.as_obj().map_or(0, |r| self.raw_hot(r, i))
    }

    fn gen_set_raw(&mut self, gn: Value, i: u32, w: u64) {
        if let Some(r) = gn.as_obj() {
            self.set_raw_hot(r, i, w);
        }
    }

    /// Is `v` a generator or an async generator (whose frames run the
    /// same way)?
    pub(crate) fn is_gen(v: Value) -> bool {
        matches!(v.obj_kind().map(u16::from), Some(ty::GEN | ty::AGEN))
    }

    /// The generator's state.
    pub(crate) fn gen_state(&self, gn: Value) -> u64 {
        self.gen_raw(gn, g::STATE)
    }

    /// Pushes the frame of `gn` above `st` (a call from `call_pc` that
    /// returns to `ret_pc` through `cont`), at `pc`.
    fn gen_enter(
        &mut self,
        st: &mut State,
        gn: Value,
        cont: Cont,
        call_pc: usize,
        ret_pc: usize,
    ) -> Result<(), Raised> {
        // One validation of the generator for all its fields.
        let word = |w: Option<&u64>| w.copied().unwrap_or(0);
        let (fields, state, ypc, cidx) = match gn.as_obj().map(|r| self.heap.slot_words(r)) {
            Some(Ok(w)) => (
                [g::FUNC, g::REGS, g::HANDLED]
                    .map(|i| Value::from_bits(word(w.get(i as usize))).unwrap_or(Value::NONE)),
                word(w.get(g::STATE as usize)),
                (word(w.get(g::YPC as usize)) & !g::CLEARED) as usize,
                u32::try_from(word(w.get(g::CODE as usize))).unwrap_or(0),
            ),
            _ => return Err(self.err(T::SystemError, "micro-monty: bad generator")),
        };
        let [func, regs, handled] = fields;
        // A created generator's code may still be left out: its first
        // fetch decodes it (`Rare::Load`). A suspended one's has run.
        let nregs = self.codes.get(cidx as usize).map_or(0, |c| {
            debug_assert!(
                state == g::CREATED || !c.code.bytecode_deferred(),
                "a suspended generator's code was decoded"
            );
            c.code.nregs as usize
        });
        let new_base = st.base + self.codes[st.ci as usize].code.nregs as usize + HDR;
        self.ensure_stack(new_base + nregs + 1)?;
        // The registers in one copy.
        let dst = &mut self.stack[new_base..new_base + nregs];
        if regs
            .as_obj()
            .is_none_or(|b| self.heap.read_values(b, 0, dst).is_err())
        {
            dst.fill(Value::UNDEFINED);
        }
        self.enter_frame(st, func, cidx, new_base, call_pc, ret_pc, cont)?;
        st.coro = gn;
        st.pc = match state {
            g::CREATED => 0,
            _ => {
                ypc + instr_len(
                    self.codes[st.ci as usize]
                        .code
                        .bytecode
                        .get(ypc)
                        .map_or(op::YIELD, |&w| op_of(w)),
                )
            }
        };
        // The generator's handled exception goes on top of the caller's.
        if handled != Value::NONE
            && let Some(r) = gn.as_obj()
        {
            self.set_slot_hot(r, g::OUTER, self.handled);
            self.handled = handled;
        }
        self.gen_set_raw(gn, g::STATE, g::RUNNING);
        Ok(())
    }

    /// [`Vm::gen_enter`] for a new or suspended plain generator with one
    /// validated read of the generator and no code-table clone; the frame
    /// record is the one `enter_frame` pushes. `false` (nothing done) when
    /// the general path must run. Not inlined: inlining it into the
    /// dispatch loop cost other programs up to 10% Ir [M].
    #[inline(never)]
    pub(crate) fn gen_enter_fast(
        &mut self,
        st: &mut State,
        gn: Value,
        cont: Cont,
        call_pc: usize,
        ret_pc: usize,
    ) -> Result<bool, Raised> {
        let Some(r) = gn.as_obj() else {
            return Ok(false);
        };
        let Some(w) = self
            .heap
            .slot_words(r)
            .ok()
            .and_then(|w| w.first_chunk::<{ g::CODE as usize + 1 }>())
        else {
            return Ok(false);
        };
        let state = w[g::STATE as usize];
        if state != g::SUSPENDED && state != g::CREATED {
            return Ok(false);
        }
        let (Some(func), Some(regs), Some(handled)) = (
            Value::from_bits(w[g::FUNC as usize]),
            Value::from_bits(w[g::REGS as usize]),
            Value::from_bits(w[g::HANDLED as usize]),
        ) else {
            return Ok(false);
        };
        let cidx = u32::try_from(w[g::CODE as usize]).unwrap_or(u32::MAX);
        let Some(code) = self.codes.get(cidx as usize) else {
            return Ok(false);
        };
        let nregs = code.code.nregs as usize;
        debug_assert!(
            state == g::CREATED || !code.code.bytecode_deferred(),
            "a suspended generator's code was decoded"
        );
        if !code.code.freevars.is_empty()
            || self.depth as usize + 1 >= self.recursion_limit as usize
            || self.specials.len() == self.specials.capacity()
        {
            return Ok(false);
        }
        let resume_pc = if state == g::CREATED {
            0
        } else {
            // A suspended generator (including one restored from an image)
            // points at YIELD or YIELD_FROM, both one word.
            let at = (w[g::YPC as usize] & !g::CLEARED) as usize;
            debug_assert_eq!(instr_len(op_of(code.code.bytecode[at])), 1);
            at + 1
        };
        let Some(b) = regs.as_obj() else {
            return Ok(false);
        };
        let new_base = st.base + self.codes[st.ci as usize].code.nregs as usize + HDR;
        self.ensure_stack(new_base + nregs + 1)?;
        if self
            .heap
            .read_values(b, 0, &mut self.stack[new_base..new_base + nregs])
            .is_err()
        {
            return Ok(false);
        }
        let cont = self.cont_put(cont)?;
        self.specials.push(crate::vm::Frame {
            code: st.ci,
            func: st.func,
            base: u32::try_from(st.base).unwrap_or(0),
            call_pc: u32::try_from(call_pc).unwrap_or(0),
            ret_pc: u32::try_from(ret_pc).unwrap_or(0),
            coro: st.coro,
            cont,
        });
        self.stack[new_base - 2] = st.func;
        self.stack[new_base - 1] = crate::vm::SPECIAL_LINK;
        self.depth += 1;
        st.base = new_base;
        st.ci = cidx;
        st.func = func;
        st.coro = gn;
        st.pc = resume_pc;
        // The kind byte proved a generator: its slots are in place.
        if handled != Value::NONE {
            self.heap.set_guarded(r, g::OUTER, self.handled);
            self.handled = handled;
        }
        self.heap.set_raw_guarded(r, g::STATE, g::RUNNING);
        Ok(true)
    }

    /// [`Vm::gen_yield`] for a plain generator, with guarded writes (the
    /// kind byte proves the generator's slots).
    #[inline]
    pub(crate) fn gen_yield_fast(&mut self, st: &State, pc: usize) {
        let gn = st.coro;
        let Some(r) = gn.as_obj() else {
            return;
        };
        let nregs = self.codes[st.ci as usize].code.nregs as usize;
        let regs = self.heap.get_guarded(r, g::REGS);
        if let Some(b) = regs.as_obj()
            && let Some(frame) = self.stack.get(st.base..st.base + nregs)
        {
            let _ = self.heap.set_values(b, 0, frame);
        }
        self.heap.set_raw_guarded(r, g::STATE, g::SUSPENDED);
        self.heap.set_raw_guarded(r, g::YPC, pc as u64);
        if self.heap.get_guarded(r, g::HANDLED) != Value::NONE {
            self.own_handler_suspend(r, g::HANDLED);
        }
    }

    fn already_running(&mut self) -> Raised {
        self.err(T::ValueError, "generator already executing")
    }

    /// `gn.send(v)` (`next` when `v` is `None`), finishing through `cont`.
    #[cold]
    pub(crate) fn gen_resume(
        &mut self,
        st: &mut State,
        gn: Value,
        v: Value,
        cont: Cont,
        call_pc: usize,
        ret_pc: usize,
    ) -> Result<Resumed, Raised> {
        match self.gen_state(gn) {
            g::RUNNING => return Err(self.already_running()),
            g::DONE => return Ok(Resumed::Out(Outcome::Stop(Value::NONE))),
            g::CREATED if v != Value::NONE => {
                return Err(self.err(
                    T::TypeError,
                    "can't send non-None value to a just-started generator",
                ));
            }
            _ => {}
        }
        self.gen_enter(st, gn, cont, call_pc, ret_pc)?;
        st.acc = v;
        Ok(Resumed::Frame)
    }

    /// `gn.throw(exc)`, finishing through `cont`.
    #[cold]
    pub(crate) fn gen_throw(
        &mut self,
        st: &mut State,
        gn: Value,
        exc: Value,
        cont: Cont,
        call_pc: usize,
        ret_pc: usize,
    ) -> Result<Resumed, Raised> {
        match self.gen_state(gn) {
            g::RUNNING => return Err(self.already_running()),
            g::CREATED | g::DONE => {
                // Never started (or finished): it is over, and the
                // exception is raised to the caller. Its frame (the bound
                // arguments) goes: nothing reads it again.
                self.gen_over(gn);
                self.exc = exc;
                return Ok(Resumed::Out(Outcome::Raise));
            }
            _ => {}
        }
        self.gen_enter(st, gn, cont, call_pc, ret_pc)?;
        let ypc = (self.gen_raw(gn, g::YPC) & !g::CLEARED) as usize;
        st.pc = ypc;
        let bc = &self.codes[st.ci as usize].code.bytecode;
        if bc.get(ypc).map(|&w| op_of(w)) == Some(op::YIELD_FROM) {
            // Delegate to the sub-iterator; its yields are re-yielded by
            // this `YIELD_FROM`, its return value ends the delegation.
            let r = operand(bc, ypc, 0);
            let send_pc = ypc.saturating_sub(instr_len(op::SEND));
            let exit = u32::try_from(operand(bc, send_pc, 1)).unwrap_or(0);
            let sub = self.stack[st.base + r];
            let dcont = Cont::Delegate {
                exit,
                at: u32::try_from(ypc).unwrap_or(0),
            };
            if sub.is_obj_kind(ty::GEN as u8) {
                let thrown =
                    self.gen_throw(st, sub, exc, dcont, ypc, ypc + instr_len(op::YIELD_FROM));
                // The sub-generator's frame was not entered (the stack or
                // the recursion limit refused it): `st` is this generator's
                // frame, which the error is raised in, at its `yield from`.
                // Returned as an error, it was raised at the `throw()` with
                // this frame still entered and the generator still running.
                let Ok(thrown) = thrown else {
                    return Ok(Resumed::Fault);
                };
                return match thrown {
                    Resumed::Out(out) => {
                        // The sub-generator could not run: finish here.
                        let n = self.finish(st.base, dcont, out);
                        match n {
                            Ok(crate::cont::Next::Jump(t, v)) => {
                                st.pc = t as usize;
                                st.acc = v;
                                Ok(Resumed::Frame)
                            }
                            Ok(_) | Err(Raised) => Ok(Resumed::Fault),
                        }
                    }
                    other => Ok(other),
                };
            }
        }
        self.exc = exc;
        Ok(Resumed::Fault)
    }

    /// Empties the saved copies of the generators and coroutines whose
    /// frames are on a stack (each running frame's and each caller
    /// record's `coro`, of the running stack and every task's): the frame
    /// on the stack replaced the copy, which holds what the last yield or
    /// suspension saved and is read again only after the next one has
    /// written it whole. A generator's copy is cleared of every register,
    /// a coroutine's dropped (its next suspension by `send` allocates one).
    /// Called before a collection step that can take roots from an idle
    /// heap (`Vm::gc`), which costs a walk of the caller records then and
    /// nothing per resume. A mark already running needs none: what such a
    /// copy holds was reachable when the mark began and is marked anyway.
    pub(crate) fn clear_running(&mut self, st: &State) {
        self.clear_copy(st.coro);
        if let Some(c) = self.saved.as_ref().map(|s| s.coro) {
            self.clear_copy(c);
        }
        // By index: the copies are cleared through the heap.
        for i in 0..self.specials.len() {
            self.clear_copy(self.specials[i].coro);
        }
        // A task cleared so and not run since is skipped (`Task::copies_cleared`).
        for t in 0..self.tasks.len() {
            let Some(task) = &self.tasks[t] else {
                continue;
            };
            if task.copies_cleared {
                continue;
            }
            let (c, n) = (
                task.regs().map_or(Value::NONE, |s| s.coro),
                task.specials.len(),
            );
            self.clear_copy(c);
            for i in 0..n {
                let c = self.tasks[t]
                    .as_ref()
                    .map_or(Value::NONE, |task| task.specials[i].coro);
                self.clear_copy(c);
            }
            if let Some(task) = &mut self.tasks[t] {
                task.copies_cleared = true;
            }
        }
    }

    /// Clears the dead saved registers of the suspended generators in
    /// [`Vm::gens`] not cleared since they last yielded: those dead where
    /// each waits and where it resumes (docs/spec.md §8.3), then sets
    /// [`g::CLEARED`] in its `YPC`. Goes on from [`Vm::gens_at`] until
    /// `budget` work units are spent: a generator is one, plus twice its
    /// buffer's registers (at most 256) when it is cleared, which bounds a
    /// dead set's computation (two searches of the live table and two
    /// chains of at most `nregs` links) and the writes. The charge does
    /// not depend on [`Vm::dead_cache`], which is not in an image, so the
    /// step at which the pass ends is the same in a session that went on
    /// after a snapshot and in the one restored from it. Returns `true`
    /// once the pass reached the end (the cursor is back at 0).
    pub(crate) fn clear_suspended(&mut self, mut budget: u64) -> bool {
        // An unbudgeted pass (a full collection's) covers every generator,
        // also those a sliced pass in progress already passed and that
        // have yielded since.
        if budget == u64::MAX {
            self.gens_at = 0;
        }
        while let Some(&gn) = self.gens.get(self.gens_at) {
            if budget == 0 {
                return false;
            }
            self.gens_at += 1;
            budget = budget.saturating_sub(1 + self.clear_saved(gn));
        }
        self.gens_at = 0;
        true
    }

    /// [`Vm::clear_suspended`] for one generator; its work beyond one unit.
    fn clear_saved(&mut self, gn: Value) -> u64 {
        let Some(r) = gn.as_obj() else {
            return 0;
        };
        let Ok(w) = self.heap.slot_words(r) else {
            return 0;
        };
        let raw = |i: u32| w.get(i as usize).copied();
        let (Some(g::SUSPENDED), Some(ci), Some(ypc), Some(buf)) =
            (raw(g::STATE), raw(g::CODE), raw(g::YPC), raw(g::REGS))
        else {
            return 0;
        };
        if ypc & g::CLEARED != 0 {
            return 0;
        }
        let Some(b) = Value::from_bits(buf)
            .filter(|v| v.is_obj_kind(ty::BUF as u8))
            .and_then(Value::as_obj)
        else {
            return 0;
        };
        let (Ok(ci), Ok(pc)) = (u32::try_from(ci), u32::try_from(ypc)) else {
            return 0;
        };
        let slot = (ci.wrapping_mul(0x9E37_79B9) ^ pc) as usize % DEAD_CACHE;
        let key = ci.wrapping_add(1);
        if self.dead_cache.is_empty() {
            self.dead_cache.resize(DEAD_CACHE, NO_MEMO);
        }
        let dead = match self.dead_cache[slot] {
            (c, p, dead) if (c, p) == (key, pc) => dead,
            _ => {
                let Some(code) = self.codes.get(ci as usize) else {
                    return 0;
                };
                let code = &code.code;
                let dead = crate::interp::dead_where(code, &suspend_pcs(code, ypc));
                self.dead_cache[slot] = (key, pc, dead);
                dead
            }
        };
        #[cfg(debug_assertions)]
        let fill = crate::interp::dead_fill(&mut self.dead_cache, &self.codes, ci);
        #[cfg(not(debug_assertions))]
        let fill = crate::interp::DEAD;
        let looked = self.heap.fill_bits(b, dead, fill).unwrap_or(0);
        let _ = self.heap.set_raw(r, g::YPC, ypc | g::CLEARED);
        2 * looked
    }

    /// [`Vm::clear_running`] for one frame's `coro`, if it is a running
    /// generator or coroutine.
    fn clear_copy(&mut self, c: Value) {
        use crate::aio::coro;
        if c.is_obj_kind(ty::CORO as u8) {
            if let Some(r) = c.as_obj()
                && self.raw(r, coro::STATE) & 3 == coro::RUNNING
                && self.slot(r, coro::ARGS) != Value::NONE
            {
                self.set_slot(r, coro::ARGS, Value::NONE);
            }
            return;
        }
        if !Vm::is_gen(c) || self.gen_state(c) != g::RUNNING {
            return;
        }
        let n = self
            .codes
            .get(self.gen_raw(c, g::CODE) as usize)
            .map_or(0, |code| code.code.nregs);
        if let Some(b) = self.vslot(c, g::REGS).as_obj() {
            for reg in 0..n {
                let _ = self.heap.set(b, reg, crate::interp::DEAD);
            }
        }
    }

    /// `YIELD`: saves the running generator frame (suspended at `pc`).
    pub(crate) fn gen_yield(&mut self, st: &State, pc: usize) {
        let gn = st.coro;
        let nregs = self.codes[st.ci as usize].code.nregs as usize;
        let regs = self.vslot(gn, g::REGS);
        if let Some(b) = regs.as_obj()
            && let Some(frame) = self.stack.get(st.base..st.base + nregs)
        {
            let _ = self.heap.set_values(b, 0, frame);
        }
        if let Some(r) = gn.as_obj()
            && let Ok(raw) = self.heap.raw_words_mut(r)
        {
            // Raw slots from `STATE`: the state, then the yield's pc.
            if let [state, ypc, ..] = raw {
                *state = g::SUSPENDED;
                *ypc = pc as u64;
            }
        }
        self.gen_leave(gn);
    }

    /// Restores the caller's handled exception when the generator stops
    /// running.
    fn gen_leave(&mut self, gn: Value) {
        if let Some(r) = gn.as_obj()
            && self.slot(r, g::HANDLED) != Value::NONE
        {
            self.own_handler_suspend(r, g::HANDLED);
        }
    }

    /// The `HANDLED` slot of generator or coroutine `c`; `OUTER` follows it.
    fn handled_slot(c: Value) -> u32 {
        if c.is_obj_kind(ty::CORO as u8) {
            crate::aio::coro::HANDLED
        } else {
            g::HANDLED
        }
    }

    /// `PUSH_EXC` in the frame of generator or coroutine `c`, with `prev`
    /// the handled exception until now and `exc` the one from now on: what
    /// the handler's register keeps. The outermost handler of the frame
    /// found an exception that is its caller's, which another resumption
    /// replaces: it keeps `UNDEFINED`, `OUTER` holds `prev` and `HANDLED`
    /// is set, which marks the frame as handling an exception of its own
    /// while it runs. A handler inside it keeps `prev`, the frame's own.
    #[cold]
    #[inline(never)]
    pub(crate) fn own_handler_enter(&mut self, c: Value, prev: Value, exc: Value) -> Value {
        let at = Self::handled_slot(c);
        let Some(r) = c.as_obj() else {
            return prev;
        };
        if self.slot(r, at) != Value::NONE {
            return prev;
        }
        let _ = self.heap.set_values(r, at, &[exc, prev]);
        Value::UNDEFINED
    }

    /// `POP_EXC` of the outermost handler of generator or coroutine `c`:
    /// the handled exception is the caller's of this resumption again.
    #[cold]
    #[inline(never)]
    pub(crate) fn own_handler_leave(&mut self, c: Value) {
        let at = Self::handled_slot(c);
        if let Some(r) = c.as_obj() {
            self.handled = self.slot(r, at + 1);
            let _ = self.heap.set_values(r, at, &[Value::NONE, Value::NONE]);
        }
    }

    /// The frame of generator or coroutine `r`, inside a handler of its
    /// own, stops running: `HANDLED` (slot `at`) keeps its handled
    /// exception and the caller's is back.
    #[cold]
    #[inline(never)]
    pub(crate) fn own_handler_suspend(&mut self, r: mm_core::Ref, at: u32) {
        let outer = self.slot(r, at + 1);
        // `HANDLED` and `OUTER` are adjacent: one write.
        let _ = self.heap.set_values(r, at, &[self.handled, Value::NONE]);
        self.handled = outer;
    }

    /// The frame of generator or coroutine `r`, suspended inside a handler
    /// of its own, runs again: its handled exception goes on top of this
    /// resumption's caller's.
    pub(crate) fn own_handler_resume(&mut self, r: mm_core::Ref, at: u32, handled: Value) {
        self.set_slot(r, at + 1, self.handled);
        self.handled = handled;
    }

    /// The generator returned: it is done.
    pub(crate) fn gen_done(&mut self, gn: Value) {
        self.gen_leave(gn);
        self.gen_over(gn);
    }

    /// Marks `gn` done and drops its frame and handled exception, without
    /// leaving it: for a generator that is not running, whose `OUTER` is
    /// not the caller's handled exception.
    pub(crate) fn gen_over(&mut self, gn: Value) {
        if let Some(r) = gn.as_obj() {
            self.set_slot_hot(r, g::HANDLED, Value::NONE);
            self.set_slot_hot(r, g::REGS, Value::NONE);
            self.set_raw_hot(r, g::STATE, g::DONE);
        }
    }

    /// An exception left the generator or coroutine: it is done, and a
    /// `StopIteration` (or in an async generator `StopAsyncIteration`)
    /// becomes `RuntimeError` (PEP 479).
    pub(crate) fn gen_raised(&mut self, gn: Value) {
        let kind = gn.obj_kind().map(u16::from);
        if kind == Some(ty::CORO) {
            self.coro_finished(gn);
        } else {
            self.gen_done(gn);
        }
        let e = self.exc;
        let t = if e.is_obj_kind(ty::EXC as u8) {
            self.exc_type(e)
        } else {
            T::object
        };
        let msg = match (kind, t) {
            (Some(ty::GEN), T::StopIteration) => "generator raised StopIteration",
            (Some(ty::CORO), T::StopIteration) => "coroutine raised StopIteration",
            (Some(ty::AGEN), T::StopIteration) => "async generator raised StopIteration",
            (Some(ty::AGEN), T::StopAsyncIteration) => "async generator raised StopAsyncIteration",
            _ => "",
        };
        if !msg.is_empty() {
            let _ = self.err(T::RuntimeError, msg);
            let new = self.exc;
            if let Some(r) = new.as_obj() {
                self.set_slot(r, exc_slot::CAUSE, e);
                self.set_slot(r, exc_slot::CONTEXT, e);
                let fl = self.raw(r, exc_slot::FLAGS) | 1;
                self.set_raw(r, exc_slot::FLAGS, fl);
            }
        }
    }

    /// `repr(gn)`.
    pub(crate) fn gen_repr(&self, gn: Value) -> String {
        let f = self.vslot(gn, g::FUNC);
        let q = self.str_ref(self.vslot(f, fn_slot::QUALNAME));
        let addr = 0x7f00_0000_0000 + (u64::from(gn.as_obj().map_or(0, mm_core::Ref::raw)) << 3);
        let what = if gn.is_obj_kind(ty::AGEN as u8) {
            "async_generator"
        } else {
            "generator"
        };
        format!("<{what} object {q} at 0x{addr:x}>")
    }

    /// A new `asend()`/`athrow()`/`aclose()` awaitable of async generator
    /// `agen`.
    pub(crate) fn new_agen_op(
        &mut self,
        agen: Value,
        mode: u64,
        arg: Value,
    ) -> Result<Value, Raised> {
        let r = {
            let res = self
                .heap
                .alloc_init(ty::AGENOP, 0, &[agen, arg], &[mode, 0]);
            self.hx(res)
        }?;
        Ok(Value::from_ref(r, ty::AGENOP as u8))
    }

    /// Awaiting `asend()`/`athrow()`/`aclose()` awaitable `op`: its value
    /// now, or `None` with a request that resumes (or throws into) the
    /// async generator, whose yield or return [`Cont::ASend`] or
    /// [`Cont::AClose`] then turns into the result.
    pub(crate) fn agen_await(&mut self, op: Value) -> Result<Option<Value>, Raised> {
        let (agen, arg) = (self.vslot_hot(op, ag::AGEN), self.vslot_hot(op, ag::ARG));
        let (mode, used) = match op.as_obj() {
            Some(r) => (self.raw_hot(r, ag::MODE), self.raw_hot(r, ag::USED)),
            None => (ag::SEND, 1),
        };
        if used != 0 {
            let msg = if mode == ag::SEND {
                "cannot reuse already awaited __anext__()/asend()"
            } else {
                "cannot reuse already awaited aclose()/athrow()"
            };
            return Err(self.err(T::RuntimeError, msg));
        }
        if let Some(r) = op.as_obj() {
            self.set_raw_hot(r, ag::USED, 1);
        }
        let state = self.gen_state(agen);
        if state == g::RUNNING {
            let msg = match mode {
                ag::SEND => "anext(): asynchronous generator is already running",
                ag::THROW => "athrow(): asynchronous generator is already running",
                _ => "aclose(): asynchronous generator is already running",
            };
            return Err(self.err(T::RuntimeError, msg));
        }
        self.request = Some(if mode == ag::SEND {
            if state == g::CREATED && arg != Value::NONE {
                return Err(self.err(
                    T::TypeError,
                    "can't send non-None value to a just-started async generator",
                ));
            }
            #[cfg(feature = "asyncio")]
            if state == g::CREATED && self.loop_flags & crate::vm::LOOP_IN_RUN != 0 {
                self.track_asyncgen(agen)?;
            }
            crate::vm::Request::Call {
                func: agen,
                args: crate::cont::Args::of(&[arg]),
                kw: Value::NONE,
                cont: Cont::ASend,
                outer: Cont::None,
            }
        } else {
            if state == g::DONE {
                return Ok(Some(Value::NONE));
            }
            let (exc, cont) = if mode == ag::CLOSE {
                (self.new_exception(T::GeneratorExit, &[])?, Cont::AClose)
            } else {
                (arg, Cont::ASend)
            };
            crate::vm::Request::Throw {
                gn: agen,
                exc,
                cont,
            }
        });
        Ok(None)
    }
}
