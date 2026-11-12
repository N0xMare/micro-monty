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
        let buf = self.alloc_buf(nregs.max(1))?;
        for i in 0..nregs {
            let v = self.stack[top + i];
            self.set_slot(buf, u32::try_from(i).unwrap_or(0), v);
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
            self.hx(res)
        }?;
        Ok(Value::from_ref(r, kind as u8))
    }

    fn gen_raw(&self, gn: Value, i: u32) -> u64 {
        gn.as_obj().map_or(0, |r| self.raw(r, i))
    }

    fn gen_set_raw(&mut self, gn: Value, i: u32, w: u64) {
        if let Some(r) = gn.as_obj() {
            self.set_raw(r, i, w);
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
                word(w.get(g::YPC as usize)) as usize,
                u32::try_from(word(w.get(g::CODE as usize))).unwrap_or(0),
            ),
            _ => return Err(self.err(T::SystemError, "micro-monty: bad generator")),
        };
        let [func, regs, handled] = fields;
        let nregs = self
            .codes
            .get(cidx as usize)
            .map_or(0, |c| c.code.nregs as usize);
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
        let outer = self.handled;
        if let Some(r) = gn.as_obj() {
            self.set_slot(r, g::OUTER, outer);
            if handled != Value::NONE {
                self.handled = handled;
            }
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
        let Ok(w) = self.heap.slot_words(r) else {
            return Ok(false);
        };
        let field = |i: u32| w.get(i as usize).copied();
        let (Some(fw), Some(rw), Some(hw), Some(state), Some(ypc), Some(cw)) = (
            field(g::FUNC),
            field(g::REGS),
            field(g::HANDLED),
            field(g::STATE),
            field(g::YPC),
            field(g::CODE),
        ) else {
            return Ok(false);
        };
        if state != g::SUSPENDED && state != g::CREATED {
            return Ok(false);
        }
        let (Some(func), Some(regs), Some(handled)) = (
            Value::from_bits(fw),
            Value::from_bits(rw),
            Value::from_bits(hw),
        ) else {
            return Ok(false);
        };
        let cidx = u32::try_from(cw).unwrap_or(u32::MAX);
        let Some(code) = self.codes.get(cidx as usize) else {
            return Ok(false);
        };
        let nregs = code.code.nregs as usize;
        if !code.code.freevars.is_empty()
            || self.depth as usize + 1 >= self.recursion_limit as usize
            || self.specials.len() == self.specials.capacity()
        {
            return Ok(false);
        }
        let resume_pc = if state == g::CREATED {
            0
        } else {
            ypc as usize
                + instr_len(
                    code.code
                        .bytecode
                        .get(ypc as usize)
                        .map_or(op::YIELD, |&x| op_of(x)),
                )
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
        self.heap.set_guarded(r, g::OUTER, self.handled);
        if handled != Value::NONE {
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
        let outer = self.heap.get_guarded(r, g::OUTER);
        let mine = if self.handled == outer {
            Value::NONE
        } else {
            self.handled
        };
        self.heap.set_guarded(r, g::HANDLED, mine);
        self.heap.set_guarded(r, g::OUTER, Value::NONE);
        self.handled = outer;
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
                // exception is raised to the caller.
                self.gen_set_raw(gn, g::STATE, g::DONE);
                self.exc = exc;
                return Ok(Resumed::Out(Outcome::Raise));
            }
            _ => {}
        }
        self.gen_enter(st, gn, cont, call_pc, ret_pc)?;
        let ypc = self.gen_raw(gn, g::YPC) as usize;
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
                return match self.gen_throw(
                    st,
                    sub,
                    exc,
                    dcont,
                    ypc,
                    ypc + instr_len(op::YIELD_FROM),
                )? {
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
        if let Some(r) = gn.as_obj() {
            let outer = self.slot(r, g::OUTER);
            let mine = if self.handled == outer {
                Value::NONE
            } else {
                self.handled
            };
            // `HANDLED` and `OUTER` are adjacent: one write.
            let _ = self.heap.set_values(r, g::HANDLED, &[mine, Value::NONE]);
            self.handled = outer;
        }
    }

    /// The generator returned: it is done.
    pub(crate) fn gen_done(&mut self, gn: Value) {
        self.gen_leave(gn);
        if let Some(r) = gn.as_obj() {
            self.set_slot(r, g::HANDLED, Value::NONE);
            self.set_slot(r, g::REGS, Value::NONE);
            self.set_raw(r, g::STATE, g::DONE);
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
        let (agen, arg) = (self.vslot(op, ag::AGEN), self.vslot(op, ag::ARG));
        let (mode, used) = match op.as_obj() {
            Some(r) => (self.raw(r, ag::MODE), self.raw(r, ag::USED)),
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
            self.set_raw(r, ag::USED, 1);
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
