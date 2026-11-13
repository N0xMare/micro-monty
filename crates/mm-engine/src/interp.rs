//! The dispatch loop: one `match` over opcodes with pc, frame base and the
//! accumulator in locals. Python-to-Python calls push a frame record and
//! continue in the same loop; exceptions unwind through the side table, then
//! frames. Safepoints (backward jumps and calls) charge fuel and run the GC.

use crate::aio::Awaited;
use crate::builtins;
use crate::class::Dn;
use crate::class::{cls, f, inst};
use crate::cont::{Args, Cont, Next, Outcome};
use crate::generator::Resumed;
use crate::host::{HostCall, HostError, HostValue};
use crate::scratch::NativeValues;
use crate::text::FormatSpec;
use crate::types::{T, ty};
use crate::vm::{
    CALL_LEN, CodeObj, Exit, Frame, HDR, Ic, ParkReason, Raised, Ready, Request, SPECIAL_LINK,
    State, Vm, exc_slot, fn_slot, link, unlink,
};
use alloc::{format, string::String, sync::Arc, vec::Vec};
use core::sync::atomic::Ordering;
use mm_compile::ast::BinOp;
use mm_compile::bytecode::{instr_len, op, operand};
use mm_core::Value;

/// Why the dispatch loop returned.
enum Flow {
    /// The task's bottom frame returned.
    Return(Value),
    /// A safepoint parked.
    Park(ParkReason),
    /// A host function was called; `st.pc` is past the call.
    Host(HostCall),
    /// An `AWAIT` at `st.pc` waits on this awaitable.
    Block(Value),
}

/// What [`Vm::dispatch_rare`] asks the dispatch loop to do next (`st`
/// holds the loop registers).
enum Rare {
    /// Run the next instruction of the same frame.
    Next,
    /// The frame changed: reload it.
    Frames,
    /// Leave the loop with this flow.
    Exit(Flow),
}

/// How a request (a builtin or an operation returned `UNDEFINED`)
/// continued.
pub(crate) enum Landed {
    /// Continue with `st` (a frame was pushed, or the operation finished
    /// and `st.pc`/`st.acc` say where to go on).
    Go,
    /// Stop for this host call.
    Host(HostCall),
    /// Re-raise the pending exception unchanged at `st.pc`.
    Reraise,
    /// Raise the pending exception at `st.pc`.
    Fault,
    /// The `AWAIT` at `st.pc` waits on this awaitable.
    Block(Value),
}

/// How `invoke` started a call.
pub(crate) enum Invoked {
    Frame,
    Fault,
    Request,
    Value(Value),
    Out(Outcome),
}

impl From<Resumed> for Invoked {
    fn from(r: Resumed) -> Invoked {
        match r {
            Resumed::Frame => Invoked::Frame,
            Resumed::Fault => Invoked::Fault,
            Resumed::Out(o) => Invoked::Out(o),
        }
    }
}

/// The records of the `depth` frames below the running one (whose base is
/// `base`), innermost first, decoded from their headers; `None` if a header
/// is malformed or the special records do not match the headers.
pub(crate) fn walk_frames(
    stack: &[Value],
    specials: &[Frame],
    codes: &[Arc<CodeObj>],
    mut base: usize,
    depth: u32,
) -> Option<Vec<Frame>> {
    let mut out = Vec::new();
    let mut sp = specials.len();
    for _ in 0..depth {
        let f = match header(stack, codes, base)? {
            Header::Plain(f) => f,
            Header::Special => {
                sp = sp.checked_sub(1)?;
                specials[sp]
            }
        };
        if f.base as usize >= base {
            return None;
        }
        base = f.base as usize;
        out.push(f);
    }
    (sp == 0).then_some(out)
}

/// A called frame's header.
enum Header {
    /// A plain frame: its caller's record.
    Plain(Frame),
    /// A special frame: its caller's record is on the specials stack.
    Special,
}

/// The header of the frame at `base`, or `None` if it holds none.
fn header(stack: &[Value], codes: &[Arc<CodeObj>], base: usize) -> Option<Header> {
    let func = *stack.get(base.checked_sub(2)?)?;
    let lk = *stack.get(base - 1)?;
    let (ret_pc, ci, off) = unlink(lk);
    if !lk.is_int() {
        return None;
    }
    if off == 0 {
        return Some(Header::Special);
    }
    codes.get(ci as usize)?;
    Some(Header::Plain(Frame {
        code: ci,
        func,
        base: u32::try_from(base.checked_sub(off)?).ok()?,
        call_pc: u32::try_from(ret_pc.checked_sub(CALL_LEN)?).ok()?,
        ret_pc: u32::try_from(ret_pc).ok()?,
        coro: Value::NONE,
        cont: 0,
    }))
}

/// The length in words of an instruction whose byte-stream length was
/// `n` (the dispatch loop's operation macros take byte lengths).
macro_rules! words {
    ($n:expr) => {
        if $n <= 4 { 1 } else { 2 }
    };
}

/// The opcode of instruction word `w`.
#[inline]
pub(crate) fn op_of(w: u32) -> u8 {
    w.to_le_bytes()[0]
}

/// How unwinding ended.
enum Unwound {
    /// A handler (or a continuation) took the exception; continue with `st`.
    Handled,
    /// No frame handles it.
    Uncaught,
    /// A continuation stopped for a host call.
    Host(HostCall),
}

/// An exception left the instruction at `st.pc`.
struct Fault {
    reraise: bool,
}

impl Vm {
    /// Runs the module body `ci` (from [`Vm::load`]) until it completes,
    /// raises or parks.
    pub fn run(&mut self, ci: u32) -> Exit {
        let Some(code) = self.codes.get(ci as usize).cloned() else {
            return Exit::Error;
        };
        if self.begin_cell().is_err() {
            return Exit::Error;
        }
        // Top-level `await` runs the cell as the task of a running loop.
        let bc = &code.code.bytecode;
        let mut pc = 0;
        while pc < bc.len() {
            let o = op_of(bc[pc]);
            if o == op::AWAIT {
                self.loop_flags |= crate::vm::LOOP_TOP_AWAIT;
                break;
            }
            pc += instr_len(o);
        }
        let n = code.code.nregs as usize + 16;
        if self.ensure_stack(n).is_err() {
            self.end_cell();
            return Exit::Error;
        }
        for v in &mut self.stack[..n] {
            *v = Value::UNDEFINED;
        }
        let st = State {
            pc: 0,
            base: 0,
            ci,
            acc: Value::NONE,
            func: Value::NONE,
            coro: Value::NONE,
        };
        self.execute(st, false)
    }

    /// The saved registers of a stopped run, or a pending `SystemError`.
    fn take_saved(&mut self) -> Option<State> {
        let st = self.saved.take();
        if st.is_none() {
            let _ = self.err(T::SystemError, "micro-monty: nothing to resume");
        }
        st
    }

    /// Continues after a park.
    pub fn resume(&mut self) -> Exit {
        match self.take_saved() {
            Some(st) => {
                // A collection step the park went ahead of runs first, and
                // a park can follow it before any code runs.
                if self.gc_owed
                    && let Some(r) = self.safepoint(&st)
                {
                    self.saved = Some(st);
                    return Exit::Parked(r);
                }
                // Only an operator that parked before starting may now
                // run; after any other park the next one still checks.
                self.afford = self.fuel_wanted != 0;
                self.execute(st, false)
            }
            None => Exit::Error,
        }
    }

    /// Answers the pending host call with a value, returned in place.
    pub fn resume_value(&mut self, v: &HostValue) -> Exit {
        let Some(mut st) = self.take_saved() else {
            return Exit::Error;
        };
        let call = self.host_call.take();
        if let Some((id, _)) = call {
            self.settle_call(id, crate::host::CallStatus::Returned);
        }
        if let Ok(x) = self.py_value(v) {
            st.acc = x;
            return self.execute(st, false);
        }
        if let Some((_, pc)) = call {
            st.pc = pc as usize;
        }
        self.execute(st, true)
    }

    /// Answers the pending host call with an error, raised at the call.
    pub fn resume_error(&mut self, e: &HostError) -> Exit {
        let Some(mut st) = self.take_saved() else {
            return Exit::Error;
        };
        let call = self.host_call.take();
        if let Some((_, pc)) = call {
            st.pc = pc as usize;
        }
        if let Ok(x) = self.host_exception(e, call.map_or(0, |(id, _)| id)) {
            self.exc = x;
        }
        self.execute(st, true)
    }

    /// Answers the pending host call with a future for it: the call returns
    /// an awaitable that a later [`Vm::resume_batch`] settles.
    pub fn resume_future(&mut self) -> Exit {
        let Some(mut st) = self.take_saved() else {
            return Exit::Error;
        };
        let call = self.host_call.take();
        let id = call.map_or(0, |(id, _)| id);
        if let Ok(f) = self.new_future(id) {
            st.acc = f;
            return self.execute(st, false);
        }
        if let Some((_, pc)) = call {
            st.pc = pc as usize;
        }
        self.execute(st, true)
    }

    /// Ends the running or stopped cell without running any more of it
    /// (no `finally` blocks run). Globals keep what the cell assigned.
    /// Returns whether a cell was active. A pending park request stays:
    /// only the park it causes takes it.
    pub fn cancel(&mut self) -> bool {
        let active = !self.tasks.is_empty();
        self.end_cell();
        active
    }

    /// Runs `st` (raising the pending exception at `st.pc` first if
    /// `fault`) and then the other tasks, until the cell completes, raises
    /// out of the main task or stops for the host.
    pub(crate) fn execute(&mut self, mut st: State, mut fault: bool) -> Exit {
        self.fuel_wanted = 0;
        loop {
            let step = if fault {
                fault = false;
                Err(Fault { reraise: false })
            } else {
                self.dispatch(&mut st)
            };
            match step {
                Ok(Flow::Return(v)) => {
                    if self.cur == 0 {
                        match self.post.take() {
                            Some(crate::vm::Post::Complete(orig)) => {
                                let r = self.vslot(v, 0);
                                let s = self.vslot(v, 1);
                                if Vm::is_str(r) && Vm::is_str(s) {
                                    self.completion_text = Some((r, s));
                                }
                                self.finish_cell();
                                return Exit::Complete(orig);
                            }
                            Some(crate::vm::Post::Render(e)) => {
                                self.exc = e;
                                self.finish_cell();
                                return Exit::Error;
                            }
                            None => {}
                        }
                        if self.want_text
                            && self.text_needs_py(v)
                            && let Ok(f) = self.prelude_fn("_post_complete")
                            && let Ok(s) = self.bottom_call(f, &[v])
                        {
                            self.post = Some(crate::vm::Post::Complete(v));
                            st = s;
                            continue;
                        }
                        self.finish_cell();
                        return Exit::Complete(v);
                    }
                    self.finish_task(true, v);
                }
                Ok(Flow::Park(r)) => {
                    self.saved = Some(st);
                    return Exit::Parked(r);
                }
                Ok(Flow::Host(call)) => {
                    self.saved = Some(st);
                    return Exit::HostCall(call);
                }
                Ok(Flow::Block(a)) => self.block_current(st, a),
                Err(Fault { reraise })
                    if core::mem::take(&mut self.oom_wait)
                        || (!reraise
                            && self.exc == self.memory_error
                            && self.oom_retry_fault(&st)) =>
                {
                    // A builtin ran out of memory that garbage may hold
                    // (`Vm::oom_retry`): `st.pc` is still the `CALL`, which
                    // runs again once a collection has ended, a step per
                    // safepoint. Parked before then, it runs again once the
                    // collection has ended all the same (`Vm::resume` runs
                    // the owed steps first, and a snapshot's collection is
                    // that collection), so that run is the rerun too: its
                    // mark goes on through the resume and into an image.
                    while self.gc_waiting() {
                        if let Some(r) = self.safepoint(&st) {
                            self.oom_rerun = self.op_stamp();
                            self.saved = Some(st);
                            return Exit::Parked(r);
                        }
                    }
                    self.oom_rerun = self.op_stamp();
                    continue;
                }
                Err(Fault { .. }) if core::mem::take(&mut self.check_before) => {
                    if core::mem::take(&mut self.long_build) {
                        // `BUILD_STRING`'s parts pass a step's work
                        // (`Vm::reserve_build`): `st.pc` is still the
                        // instruction, which joins them a step at a time.
                        fault = self.build_string_steps(&mut st);
                        continue;
                    }
                    // An operator stopped before starting (`stop_before`):
                    // `st.pc` is still the instruction.
                    if let Some(r) = self.safepoint(&st) {
                        self.saved = Some(st);
                        return Exit::Parked(r);
                    }
                    continue;
                }
                Err(Fault { reraise }) => {
                    if self.fuel_wanted != 0 {
                        // An operator parked before starting (`fuel_short`):
                        // `st.pc` is still the instruction. Charge the
                        // safepoints run since the last check, as one does.
                        self.charge_ticks();
                        self.saved = Some(st);
                        return Exit::Parked(ParkReason::Fuel);
                    }
                    match self.unwind(&mut st, reraise) {
                        Unwound::Handled => continue,
                        Unwound::Host(call) => {
                            self.saved = Some(st);
                            return Exit::HostCall(call);
                        }
                        Unwound::Uncaught => {}
                    }
                    if self.cur == 0 {
                        match self.post.take() {
                            Some(crate::vm::Post::Render(e)) => {
                                // `str()` failed while reporting: report
                                // the original exception anyway.
                                self.exc = e;
                            }
                            Some(crate::vm::Post::Complete(_)) => {}
                            None => {
                                let e = self.exc;
                                if self.exc_needs_py(e)
                                    && let Ok(f) = self.prelude_fn("_post_render")
                                    && let Ok(s) = self.bottom_call(f, &[e])
                                {
                                    self.post = Some(crate::vm::Post::Render(e));
                                    self.handled = Value::NONE;
                                    st = s;
                                    continue;
                                }
                                self.exc = e;
                            }
                        }
                        self.finish_cell();
                        return Exit::Error;
                    }
                    let e = self.exc;
                    self.finish_task(false, e);
                }
            }
            match self.next_task() {
                crate::aio::Next::Run(s) => st = s,
                crate::aio::Next::Raise(s) => {
                    st = s;
                    fault = true;
                }
                crate::aio::Next::Wait(ids) => return self.wait_on(ids),
            }
        }
    }

    /// Drops what a `collect` that stopped part way kept (`Vm::partial`),
    /// out of line: `unwind` is inlined into the dispatch loop.
    #[cold]
    #[inline(never)]
    fn drop_partial(&mut self) {
        self.partial = None;
    }

    /// Finds a handler for the pending exception (or a continuation that
    /// takes it).
    fn unwind(&mut self, st: &mut State, mut reraise: bool) -> Unwound {
        self.request = None;
        self.slow = false;
        if self.partial.is_some() {
            self.drop_partial();
        }
        if self.exc == self.memory_error {
            // A full arena may be mostly garbage that no successful
            // allocation would ever schedule a collection for.
            self.request_gc();
        }
        'raise: loop {
            if !reraise {
                let exc = self.exc;
                self.attach_context(exc);
                self.add_tb(exc, st.ci, u32::try_from(st.pc).unwrap_or(0));
            }
            reraise = false;
            loop {
                if let Some(h) = self.codes[st.ci as usize]
                    .code
                    .handler(u32::try_from(st.pc).unwrap_or(0))
                {
                    st.pc = h.handler as usize;
                    st.acc = self.exc;
                    return Unwound::Handled;
                }
                if st.coro != Value::NONE {
                    self.gen_raised(st.coro);
                    st.coro = Value::NONE;
                }
                let Some(f) = self.pop_frame(st) else {
                    return Unwound::Uncaught;
                };
                let fc = self.cont_take(f.cont);
                if fc != Cont::None {
                    let r = self
                        .finish(st.base, fc, Outcome::Raise)
                        .and_then(|n| match n {
                            Next::Pass => Ok(None),
                            n => self
                                .drive(st, n, f.call_pc as usize, f.ret_pc as usize, Vec::new())
                                .map(Some),
                        });
                    match r {
                        Ok(None) => {}
                        Ok(Some(Landed::Go)) => return Unwound::Handled,
                        Ok(Some(Landed::Host(call))) => return Unwound::Host(call),
                        Ok(Some(Landed::Reraise)) => {
                            reraise = true;
                            continue 'raise;
                        }
                        Ok(Some(Landed::Fault)) | Err(Raised) => continue 'raise,
                        Ok(Some(Landed::Block(_))) => {
                            let _ = self.err(T::SystemError, "micro-monty: await while unwinding");
                            continue 'raise;
                        }
                    }
                }
                let exc = self.exc;
                self.add_tb(exc, f.code, f.call_pc);
            }
        }
    }

    /// A frame state that calls `func(*args)` as the bottom frame of the
    /// running task (post-mortem prelude work).
    pub(crate) fn bottom_call(&mut self, func: Value, args: &[Value]) -> Result<State, Raised> {
        let cidx = u32::try_from(self.vslot_raw(func, fn_slot::CODE)).unwrap_or(0);
        let Some(code) = self.codes.get(cidx as usize).cloned() else {
            return Err(self.err(T::SystemError, "bad code index"));
        };
        let nregs = code.code.nregs as usize;
        self.ensure_stack(nregs + 64)?;
        for v in &mut self.stack[..nregs] {
            *v = Value::UNDEFINED;
        }
        self.bind_args(func, &code, args, &[], 0, None)?;
        Ok(State {
            pc: 0,
            base: 0,
            ci: cidx,
            acc: Value::NONE,
            func,
            coro: Value::NONE,
        })
    }

    // --------------------------------------------------------- safepoints

    /// The cold half of a safepoint: fuel, deadline and GC.
    #[cold]
    #[inline(never)]
    fn safepoint(&mut self, st: &State) -> Option<ParkReason> {
        self.charge_ticks();
        self.afford = false;
        // What an earlier instruction's builtin consumed is not kept alive
        // for the GC (`Vm::fresh_consumed`).
        if !self.consumed.is_empty() {
            self.fresh_consumed();
        }
        let stress = self.gc_stress && self.stress_due();
        let request = self.gc_pending || stress;
        let due = request
            || (!self.gc_off
                && (self.gc_inc_stress
                    || self.heap.marking()
                    || self.heap.sweeping()
                    || self.heap.should_collect()));
        // A park that is due goes first, so the step of native work behind
        // it and a collection step never share one gap: the collection step
        // is owed and runs first when the session resumes (`Vm::resume`),
        // with a park check after it. It is owed once, never twice: with
        // fuel slices of at most `TICKS` units every check ends in a fuel
        // park, and a collection that waited for none would never run.
        if due
            && !self.gc_owed
            && !stress
            && !self.gc_inc_stress
            && let Some(r) = self.take_park()
        {
            self.gc_owed = true;
            return Some(r);
        }
        let owed = core::mem::take(&mut self.gc_owed);
        // Collection steps charge no fuel: what they do depends on when
        // collections fall due, which differs between targets and after a
        // restore. With `gc_yield` under a fuel limit, a fuel slice ends
        // after a step that marked or collected instead (stress testing,
        // which collects at every check, aside), so a host that regains
        // control only between slices waits for at most one collection
        // step plus a slice.
        let yields = self.gc_yield
            && self.fuel.is_some()
            && (request || due)
            && !self.gc_stress
            && !self.gc_inc_stress;
        let before = yields.then(|| (self.heap.marking(), self.heap.stats().collections));
        if request {
            // A collection asked for in steps (`gc_wait`) runs its next
            // step; any other request collects in full.
            let step = self.gc_pending && self.gc_wait != 0;
            self.gc_requested_done();
            self.gc(st, step);
        } else if due {
            // One step of a collection (incremental over 32 MiB live,
            // or a slice of the last one's lazy sweep); a park can follow.
            self.gc(st, true);
        }
        let mut marked = before.is_some_and(|(marking, collections)| {
            marking || self.heap.marking() || self.heap.stats().collections != collections
        });
        // A mark the guest must wait for (`Heap::holding`: too little room
        // left to go on allocating, or after a mark that did not converge),
        // and a collection it asked for (`Vm::gc_held`: `gc.collect`, a
        // retry after `MemoryError`), go on a step at a time with no guest
        // code between steps, a park check between them; a park leaves its
        // next step owed.
        while self.heap.holding() || self.gc_held() {
            if let Some(r) = self.take_park() {
                self.gc_owed = true;
                return Some(r);
            }
            if marked {
                self.gc_owed = true;
                return Some(ParkReason::Fuel);
            }
            self.gc(st, true);
            marked = yields;
        }
        // An owed step ran at a resume: its slice goes on.
        match self.take_park() {
            None if marked && !owed => Some(ParkReason::Fuel),
            r => r,
        }
    }

    /// A park that is due: requested, past the deadline, or out of fuel.
    fn take_park(&mut self) -> Option<ParkReason> {
        // The clock is read first: a host may request the park from it
        // (the mm-wasm guest polls its host there), and a request wins.
        let late =
            matches!((self.deadline, self.clock.as_ref()), (Some(d), Some(clock)) if clock() >= d);
        if self.park.swap(false, Ordering::SeqCst) {
            return Some(ParkReason::Requested);
        }
        if late {
            self.deadline = None;
            return Some(ParkReason::Deadline);
        }
        if self.fuel == Some(0) {
            return Some(ParkReason::Fuel);
        }
        None
    }

    pub(crate) fn collect_garbage(&mut self, st: &State) {
        self.gc(st, false);
    }

    /// A full collection, or one step of an incremental one (`step`).
    fn gc(&mut self, st: &State, step: bool) {
        let top = (st.base + self.codes[st.ci as usize].code.nregs as usize).min(self.stack.len());
        self.prune_fresh();
        let fresh = &mut self.fresh;
        let mut roots = |t: &mut mm_core::Tracer<'_>| {
            t.values(&self.stack[..top]);
            t.values(&[
                st.acc,
                st.func,
                st.coro,
                self.globals,
                self.builtins,
                self.handled,
                self.exc,
                self.memory_error,
                self.prelude_globals,
                self.missing,
                self.modules,
                self.empty_tuple,
            ]);
            if let Some(s) = &self.saved {
                t.values(&[s.acc, s.func, s.coro]);
            }
            match &self.request {
                #[cfg(feature = "asyncio")]
                Some(Request::Run(c)) => t.value(*c),
                Some(Request::Call {
                    func,
                    args,
                    kw,
                    cont,
                    outer,
                }) => {
                    t.values(&[*func, *kw]);
                    t.values(args);
                    cont.trace(t);
                    outer.trace(t);
                }
                Some(Request::Throw { gn, exc, cont }) => {
                    t.values(&[*gn, *exc]);
                    cont.trace(t);
                }
                _ => {}
            }
            t.values(&self.std_globals);
            t.values(&self.dunders);
            for c in self.conts.iter().flatten() {
                c.trace(t);
            }
            for (k, items) in &self.consumed {
                t.value(*k);
                t.values(items);
            }
            if let Some((k, it, items)) = &self.partial {
                t.values(&[*k, *it]);
                t.values(items);
            }
            match self.post {
                Some(crate::vm::Post::Complete(v) | crate::vm::Post::Render(v)) => t.value(v),
                None => {}
            }
            if let Some((r, s)) = self.completion_text {
                t.values(&[r, s]);
            }
            t.values(&self.gathers);
            t.value(self.task_names);
            t.values(&[self.context, self.escape]);
            t.values(&self.unretrieved);
            for r in &self.ready {
                match *r {
                    Ready::Gather(g) => t.value(g),
                    Ready::Call(f, a) => t.values(&[f, a]),
                    Ready::Task(_) => {}
                }
            }
            for &(_, f) in &self.futures {
                t.value(f);
            }
            // Suspended tasks: their live registers and frames.
            for task in self.tasks.iter().flatten() {
                t.values(&[task.handle, task.handled, task.awaiting, task.context]);
                if let Some(s) = &task.st {
                    t.values(&[s.acc, s.func, s.coro]);
                    let top = (s.base + self.codes[s.ci as usize].code.nregs as usize)
                        .min(task.stack.len());
                    t.values(&task.stack[..top]);
                }
                for f in &task.specials {
                    t.values(&[f.func, f.coro]);
                }
            }
            t.values(&self.types);
            for f in &self.specials {
                t.value(f.func);
                t.value(f.coro);
            }
            for c in self.codes.iter() {
                t.values(&c.consts);
                t.values(&c.names);
            }
            for shape in &self.shapes {
                t.values(&shape.names);
                for &(name, _) in &shape.trans {
                    t.value(name);
                }
            }
        };
        let mut weak = |t: &mut mm_core::Tracer<'_>| Vm::sweep_fresh(fresh, t);
        if step {
            if self.heap.collect_step(&mut roots, &mut weak).is_none() {
                return;
            }
        } else {
            self.heap.collect_weak(&mut roots, &mut weak);
        }
        // Registers above a frame's top are dead (a callee's frame starts
        // at its caller's argument window, below the caller's dead
        // temporaries) and were not traced: clear them, or a later frame
        // whose registers cover them would trace and save freed objects.
        self.stack[top..].fill(Value::UNDEFINED);
        for task in self.tasks.iter_mut().flatten() {
            let top = task.st.as_ref().map_or(0, |s| {
                (s.base + self.codes[s.ci as usize].code.nregs as usize).min(task.stack.len())
            });
            task.stack[top..].fill(Value::UNDEFINED);
        }
        self.ics.fill(Ic::Empty);
    }

    /// Forces a collection now (tests and the facade's explicit GC).
    pub fn collect_now(&mut self) {
        if self.codes.is_empty() {
            return;
        }
        let st = State {
            pc: 0,
            base: 0,
            ci: 0,
            acc: Value::NONE,
            func: Value::NONE,
            coro: Value::NONE,
        };
        self.end_cell();
        // A collection between cells (a compile that did not fit, code
        // reclamation) is not one a cell's operations ran: the retry after
        // `MemoryError` still counts what came before it, as after a
        // snapshot's (`Vm::prepare_snapshot`), so a cell's outcome does not
        // depend on whether the host snapshotted between cells.
        let since = self.heap.allocated_since_own_gc();
        self.collect_garbage(&st);
        self.heap.keep_allocated_since_gc(since);
    }

    // ------------------------------------------------------------ calls

    /// Pushes a frame for Python function `func` (code `cidx`) whose
    /// registers the caller has filled from `new_base`.
    /// Would calling code `cidx` exceed the recursion limit? Prelude and
    /// continuation frames do not count (CPython runs that code in C),
    /// but all frames together stay under ten times the limit.
    #[cold]
    fn too_deep(&self, st: &State, cidx: u32) -> bool {
        let limit = self.recursion_limit as usize;
        if self.depth as usize + 1 >= 10 * limit {
            return true;
        }
        let user = |ci: u32| self.codes.get(ci as usize).is_some_and(|c| !c.prelude);
        // A prelude frame adds no user frame, so only the cap above applies;
        // counting would walk every frame per call of a deep prelude descent.
        if !user(cidx) {
            return false;
        }
        let n = self.user_frames(st.base) + usize::from(user(st.ci)) + 1;
        n > limit
    }

    /// User (not prelude) frames below the running one, whose base is
    /// `base`.
    pub(crate) fn user_frames(&self, base: usize) -> usize {
        let frames = walk_frames(&self.stack, &self.specials, &self.codes, base, self.depth);
        let user = |ci: u32| self.codes.get(ci as usize).is_some_and(|c| !c.prelude);
        frames.map_or(0, |fs| fs.iter().filter(|f| user(f.code)).count())
    }

    /// Pops the frame below the running one into `st` (its pc at the call);
    /// `None` at the bottom frame.
    pub(crate) fn pop_frame(&mut self, st: &mut State) -> Option<Frame> {
        if self.depth == 0 {
            return None;
        }
        let f = match header(&self.stack, &self.codes, st.base)? {
            Header::Plain(f) => f,
            Header::Special => self.specials.pop()?,
        };
        self.depth -= 1;
        self.codes.get(f.code as usize)?;
        st.ci = f.code;
        st.func = f.func;
        st.coro = f.coro;
        st.base = f.base as usize;
        st.pc = f.call_pc as usize;
        Some(f)
    }

    #[allow(clippy::too_many_arguments, reason = "the full call-site description")]
    pub(crate) fn enter_frame(
        &mut self,
        st: &mut State,
        func: Value,
        cidx: u32,
        new_base: usize,
        call_pc: usize,
        ret_pc: usize,
        cont: Cont,
    ) -> Result<(), Raised> {
        if self.depth as usize + 1 >= self.recursion_limit as usize && self.too_deep(st, cidx) {
            return Err(self.err(T::RecursionError, "maximum recursion depth exceeded"));
        }
        let Some(code) = self.codes.get(cidx as usize).cloned() else {
            return Err(self.err(T::SystemError, "bad code index"));
        };
        // The callee's header may overlay the caller's call window.
        if new_base < st.base + HDR
            || new_base > st.base + self.codes[st.ci as usize].code.nregs as usize + HDR
            || new_base > self.stack.len()
        {
            return Err(self.err(T::SystemError, "micro-monty: frame overlaps its caller"));
        }
        // `matches!` tests the tag only; the derived `==` compares fields.
        let plain = matches!(cont, Cont::None)
            && st.coro == Value::NONE
            && ret_pc == call_pc + CALL_LEN
            && self.codes[st.ci as usize]
                .code
                .bytecode
                .get(call_pc)
                .map(|&o| instr_len(op_of(o)))
                == Some(CALL_LEN);
        let lk = match link(ret_pc, st.ci, new_base - st.base) {
            Some(lk) if plain => lk,
            _ => {
                if self.specials.len() == self.specials.capacity() {
                    self.grow_specials()?;
                }
                let cont = self.cont_put(cont)?;
                self.specials.push(Frame {
                    code: st.ci,
                    func: st.func,
                    base: u32::try_from(st.base).unwrap_or(0),
                    call_pc: u32::try_from(call_pc).unwrap_or(0),
                    ret_pc: u32::try_from(ret_pc).unwrap_or(0),
                    coro: st.coro,
                    cont,
                });
                SPECIAL_LINK
            }
        };
        self.stack[new_base - 2] = st.func;
        self.stack[new_base - 1] = lk;
        self.depth += 1;
        if !code.code.freevars.is_empty() {
            let closure = self.vslot(func, fn_slot::CLOSURE);
            for (k, &r) in code.code.freevars.iter().enumerate() {
                let cell = self.vslot(closure, u32::try_from(k).unwrap_or(0));
                self.stack[new_base + r as usize] = cell;
            }
        }
        st.base = new_base;
        st.ci = cidx;
        st.func = func;
        st.coro = Value::NONE;
        st.pc = 0;
        Ok(())
    }

    pub(crate) fn ensure_stack(&mut self, n: usize) -> Result<(), Raised> {
        if self.stack.len() < n {
            self.grow_stack(n)?;
        }
        Ok(())
    }

    #[cold]
    fn grow_stack(&mut self, n: usize) -> Result<(), Raised> {
        if n > (1 << 24) {
            return Err(self.err(T::MemoryError, "stack overflow"));
        }
        let new = n.max(self.stack.len() * 2);
        self.reserve_execution(
            new.saturating_sub(self.stack.capacity()) as u64 * size_of::<Value>() as u64,
        )?;
        let result = self.stack.try_reserve_exact(new - self.stack.len());
        self.reserve_execution(0)?;
        result.map_err(|_| self.err(T::MemoryError, "stack allocation failed"))?;
        self.stack.resize(new, Value::UNDEFINED);
        Ok(())
    }

    #[cold]
    fn grow_specials(&mut self) -> Result<(), Raised> {
        let extra = self.specials.len().max(4);
        self.reserve_execution(extra as u64 * size_of::<Frame>() as u64)?;
        let result = self.specials.try_reserve_exact(extra);
        self.reserve_execution(0)?;
        result.map_err(|_| self.err(T::MemoryError, "frame allocation failed"))
    }

    /// Calls `callee` with `npos` positional values from stack index `args`
    /// followed by the values of the keywords named in `kwnames`. Returns the
    /// result for builtins, or `None` after pushing a Python frame.
    #[allow(clippy::too_many_arguments, reason = "the full call-site description")]
    fn call_value(
        &mut self,
        st: &mut State,
        callee: Value,
        args: usize,
        npos: usize,
        kwnames: Value,
        call_pc: usize,
        ret_pc: usize,
        ic: Option<usize>,
        cont: Cont,
    ) -> Result<Option<Value>, Raised> {
        self.frame_base = st.base;
        let top = st.base + self.codes[st.ci as usize].code.nregs as usize + HDR;
        let nkw = if kwnames == Value::NONE {
            0
        } else {
            self.tuple_len(kwnames)
        };
        if let Some(idx) = callee.as_native() {
            self.ensure_stack(top + npos + nkw + 64)?;
            // Only the dispatch loop's `CALL` instructions reach here: the
            // builtin may park before it starts (`Vm::native_short`).
            self.direct_call = true;
            let v = self.call_native(
                idx,
                u32::try_from(args).unwrap_or(0),
                u32::try_from(npos).unwrap_or(0),
                kwnames,
                u32::try_from(top).unwrap_or(0),
            )?;
            return Ok(Some(v));
        }
        if callee.is_obj_kind(ty::FUNC as u8) {
            let cidx = u32::try_from(self.vslot_raw(callee, fn_slot::CODE)).unwrap_or(0);
            let Some(code) = self.codes.get(cidx as usize).cloned() else {
                return Err(self.err(T::SystemError, "bad code index"));
            };
            let nregs = code.code.nregs as usize;
            self.ensure_stack(top + nregs + 1)?;
            let p = &code.code.params;
            let argc = p.argcount as usize;
            let simple = nkw == 0 && p.is_simple();
            if simple && npos == argc {
                for i in 0..npos {
                    self.stack[top + i] = self.stack[args + i];
                }
                if let Some(slot) = ic
                    && !code.code.is_coroutine
                    && !code.code.is_generator
                {
                    self.ics[slot] = Ic::Call {
                        func: callee,
                        code: cidx,
                    };
                }
            } else if simple
                && npos < argc
                && let d = self.vslot(callee, fn_slot::DEFAULTS)
                && d.is_obj_kind(ty::TUPLE as u8)
                && let Some(k) = self.tuple_len(d).checked_sub(argc - npos)
            {
                // Positional arguments only, the rest from the defaults:
                // the general binder's result without its bookkeeping.
                for i in 0..npos {
                    self.stack[top + i] = self.stack[args + i];
                }
                for i in npos..argc {
                    let j = u32::try_from(k + i - npos).unwrap_or(u32::MAX);
                    self.stack[top + i] = self.vslot(d, j);
                }
            } else {
                let pos: Vec<Value> = self.stack[args..args + npos].to_vec();
                let kws: Vec<(Value, Value)> = if nkw == 0 {
                    Vec::new()
                } else {
                    let names = self.tuple_items(kwnames);
                    names
                        .into_iter()
                        .enumerate()
                        .map(|(k, n)| (n, self.stack[args + npos + k]))
                        .collect()
                };
                self.bind_args(callee, &code, &pos, &kws, top, None)?;
            }
            let total = code.code.params.total() as usize;
            for v in &mut self.stack[top + total.min(nregs)..top + nregs] {
                *v = Value::UNDEFINED;
            }
            if code.code.is_generator {
                return Ok(Some(self.new_gen(callee, cidx, top, nregs)?));
            }
            if code.code.is_coroutine {
                return Ok(Some(self.new_coro(callee, top, nregs)?));
            }
            self.enter_frame(st, callee, cidx, top, call_pc, ret_pc, cont)?;
            return Ok(None);
        }
        if let Some(kind) = callee.obj_kind().map(u16::from)
            && matches!(kind, ty::METHOD | ty::CLASS | ty::INST | ty::STATICMETHOD)
        {
            let pos: Vec<Value> = self.stack[args..args + npos].to_vec();
            let kws: Vec<(Value, Value)> = if nkw == 0 {
                Vec::new()
            } else {
                self.tuple_items(kwnames)
                    .into_iter()
                    .enumerate()
                    .map(|(k, n)| (n, self.stack[args + npos + k]))
                    .collect()
            };
            return match kind {
                ty::METHOD => {
                    let mut all = Vec::with_capacity(pos.len() + 1);
                    all.push(self.vslot(callee, 0));
                    all.extend(pos);
                    let f = self.vslot(callee, 1);
                    self.call_vec(st, f, &all, &kws, call_pc, ret_pc, cont)
                }
                ty::STATICMETHOD => {
                    let f = self.vslot(callee, 0);
                    self.call_vec(st, f, &pos, &kws, call_pc, ret_pc, cont)
                }
                ty::CLASS => self.call_class(st, callee, &pos, &kws, call_pc, ret_pc, cont),
                _ => {
                    let Some(m) = self.dunder(callee, Dn::Call) else {
                        let msg = format!("'{}' object is not callable", self.type_name(callee));
                        return Err(self.err(T::TypeError, &msg));
                    };
                    let (f, all) = if m.is_obj_kind(ty::FUNC as u8) {
                        let mut all = Vec::with_capacity(pos.len() + 1);
                        all.push(callee);
                        all.extend(pos);
                        (m, all)
                    } else {
                        let c = self.class_of(callee);
                        (self.bind_attr(m, callee, c)?, pos)
                    };
                    self.call_vec(st, f, &all, &kws, call_pc, ret_pc, cont)
                }
            };
        }
        if callee.is_obj_kind(ty::HOSTFN as u8) {
            let call = self.host_call(callee, args, npos, kwnames)?;
            self.request = Some(Request::Host(call));
            return Ok(Some(Value::UNDEFINED));
        }
        if callee.is_obj_kind(ty::BOUND as u8) {
            let recv = self.vslot(callee, 0);
            let f = self.vslot(callee, 1);
            let n = npos + nkw;
            self.ensure_stack(top + n + 1 + 64)?;
            for i in (0..n).rev() {
                self.stack[top + 1 + i] = self.stack[args + i];
            }
            self.stack[top] = recv;
            let fi = f.as_native().unwrap_or(0);
            let v = self.call_native(
                fi,
                u32::try_from(top).unwrap_or(0),
                u32::try_from(npos + 1).unwrap_or(0),
                kwnames,
                u32::try_from(top + n + 1).unwrap_or(0),
            )?;
            return Ok(Some(v));
        }
        if let Some(t) = self.type_of_typeobj(callee) {
            if t.is_exception() {
                if nkw > 0 {
                    return self.exc_with_kwargs(t, args, npos, kwnames).map(Some);
                }
                let items: Vec<Value> = self.stack[args..args + npos].to_vec();
                return Ok(Some(self.new_exception(t, &items)?));
            }
            if let Some(c) = self.ctor_of(t) {
                self.ensure_stack(top + npos + nkw + 64)?;
                let v = self.call_native(
                    c,
                    u32::try_from(args).unwrap_or(0),
                    u32::try_from(npos).unwrap_or(0),
                    kwnames,
                    u32::try_from(top).unwrap_or(0),
                )?;
                return Ok(Some(v));
            }
            // The singletons' types give their one instance.
            let one = match t {
                T::NoneType => Some(Value::NONE),
                T::ellipsis => Some(Value::ELLIPSIS),
                T::NotImplementedType => Some(Value::NOT_IMPLEMENTED),
                _ => None,
            };
            if let Some(v) = one {
                if npos + nkw == 0 {
                    return Ok(Some(v));
                }
                // CPython names `type(...)` `ellipsis` but `EllipsisType` here.
                let name = if t == T::ellipsis {
                    "EllipsisType"
                } else {
                    t.name()
                };
                let msg = format!("{name} takes no arguments");
                return Err(self.err(T::TypeError, &msg));
            }
            return Err(self.err(
                T::TypeError,
                &format!("cannot create '{}' instances", t.name()),
            ));
        }
        let msg = format!("'{}' object is not callable", self.type_name(callee));
        Err(self.err(T::TypeError, &msg))
    }

    /// Calls `f(*pos, **kws)` from `st` (the arguments go above its
    /// registers), finishing a pushed frame through `cont`. Like
    /// `call_value`: a builtin's result is returned for the caller to
    /// finish.
    #[allow(clippy::too_many_arguments, reason = "the full call-site description")]
    pub(crate) fn call_vec(
        &mut self,
        st: &mut State,
        f: Value,
        pos: &[Value],
        kws: &[(Value, Value)],
        call_pc: usize,
        ret_pc: usize,
        cont: Cont,
    ) -> Result<Option<Value>, Raised> {
        if let Some(kind) = f.obj_kind().map(u16::from)
            && matches!(kind, ty::METHOD | ty::CLASS | ty::INST | ty::STATICMETHOD)
        {
            // Indirect callables can form cycles without entering a Python
            // function (for example A.__init__ = A). Cross a bytecode frame
            // before following the next link, so limits and parking apply.
            let args = self.alloc_tuple(pos)?;
            let kw = if kws.is_empty() {
                Value::NONE
            } else {
                let d = self.new_dict()?;
                for &(k, v) in kws {
                    self.dict_set(d, k, v)?;
                }
                d
            };
            let invoke = self.prelude_fn("_invoke")?;
            // This re-entry is bounded: invoke is a plain Python function.
            return self.call_vec(st, invoke, &[f, args, kw], &[], call_pc, ret_pc, cont);
        }
        if kws.is_empty() && self.push_func(st, f, pos, call_pc, ret_pc, cont)? {
            return Ok(None);
        }
        let top = st.base + self.codes[st.ci as usize].code.nregs as usize + HDR;
        let n = pos.len() + kws.len();
        self.ensure_stack(top + n + 64)?;
        self.stack[top..top + pos.len()].copy_from_slice(pos);
        for (k, (_, v)) in kws.iter().enumerate() {
            self.stack[top + pos.len() + k] = *v;
        }
        let kwnames = if kws.is_empty() {
            Value::NONE
        } else {
            let names: Vec<Value> = kws.iter().map(|(k, _)| *k).collect();
            self.alloc_tuple(&names)?
        };
        self.call_value_at(
            st,
            f,
            top,
            pos.len(),
            kwnames,
            call_pc,
            ret_pc,
            top + n + 1,
            cont,
        )
    }

    /// Pushes the frame of plain Python function `f` called with exactly
    /// its positional parameters, the arguments written straight into the
    /// new frame's registers; `false` (nothing done) for any other call.
    fn push_func(
        &mut self,
        st: &mut State,
        f: Value,
        pos: &[Value],
        call_pc: usize,
        ret_pc: usize,
        cont: Cont,
    ) -> Result<bool, Raised> {
        if !f.is_obj_kind(ty::FUNC as u8) {
            return Ok(false);
        }
        let cidx = u32::try_from(self.vslot_raw(f, fn_slot::CODE)).unwrap_or(0);
        let Some(code) = self.codes.get(cidx as usize) else {
            return Ok(false);
        };
        let c = &code.code;
        if !c.params.is_simple()
            || c.params.argcount as usize != pos.len()
            || c.is_generator
            || c.is_coroutine
        {
            return Ok(false);
        }
        let nregs = c.nregs as usize;
        let top = st.base + self.codes[st.ci as usize].code.nregs as usize + HDR;
        self.ensure_stack(top + nregs + 1)?;
        self.stack[top..top + pos.len()].copy_from_slice(pos);
        for v in &mut self.stack[top + pos.len().min(nregs)..top + nregs] {
            *v = Value::UNDEFINED;
        }
        self.enter_frame(st, f, cidx, top, call_pc, ret_pc, cont)?;
        Ok(true)
    }

    /// The `Ic::New` entry for calling class `c` with `argc` positional
    /// arguments, when `call_class` would only allocate an instance and run
    /// a plain Python `__init__` (metaclass `type`, no `__new__`, no
    /// builtin or exception base, simple parameters).
    fn new_ic(&mut self, c: Value, argc: usize) -> Option<Ic> {
        let cr = c.as_obj().filter(|_| Vm::is_class(c))?;
        // A metaclass could gain `__call__` without changing `c`'s version.
        if self.meta_of(c).is_some() {
            return None;
        }
        let base = self.class_base_t(c);
        if self.raw(cr, cls::FLAGS) & f::NEW != 0
            || base == T::type_
            || base.is_exception()
            || Vm::is_bx_type(base)
        {
            return None;
        }
        let k = self.dn(Dn::Init).ok()?;
        let init = self.class_lookup(c, k)?;
        if !init.is_obj_kind(ty::FUNC as u8) {
            return None;
        }
        let code = u32::try_from(self.vslot_raw(init, fn_slot::CODE)).ok()?;
        let k = &self.codes.get(code as usize)?.code;
        if !k.params.is_simple()
            || k.params.argcount as usize != argc + 1
            || k.is_generator
            || k.is_coroutine
        {
            return None;
        }
        Some(Ic::New {
            version: self.raw(cr, cls::VERSION),
            init,
            code,
        })
    }

    /// `cls(...)` (arguments at stack index `args`) through call site
    /// `slot`'s `Ic::New`, filling it on a miss: allocates the instance and
    /// pushes `__init__` with `Cont::Init`. `false` (nothing done) when the
    /// class does not qualify. Out of line: the dispatch loop's register
    /// allocation is sensitive to its size.
    #[allow(clippy::too_many_arguments, reason = "the full call-site description")]
    #[inline(never)]
    fn call_new(
        &mut self,
        st: &mut State,
        callee: Value,
        args: usize,
        argc: usize,
        slot: usize,
        call_pc: usize,
        ret_pc: usize,
    ) -> Result<bool, Raised> {
        let hit = match self.ics[slot] {
            Ic::New {
                version,
                init,
                code,
            } if callee
                .as_obj()
                .is_some_and(|cr| self.heap.get_raw_guarded(cr, cls::VERSION) == version) =>
            {
                Some((init, code))
            }
            _ => match self.new_ic(callee, argc) {
                Some(ic @ Ic::New { init, code, .. }) => {
                    self.ics[slot] = ic;
                    Some((init, code))
                }
                _ => None,
            },
        };
        let Some((init, cidx)) = hit else {
            return Ok(false);
        };
        let inst = self.alloc_of_class(callee, &[])?;
        let top = st.base + self.codes[st.ci as usize].code.nregs as usize + HDR;
        let nregs = self
            .codes
            .get(cidx as usize)
            .map_or(0, |c| c.code.nregs as usize)
            .max(argc + 1);
        self.ensure_stack(top + nregs + 1)?;
        self.stack.copy_within(args..args + argc, top + 1);
        self.stack[top] = inst;
        for v in &mut self.stack[top + argc + 1..top + nregs] {
            *v = Value::UNDEFINED;
        }
        self.enter_frame(st, init, cidx, top, call_pc, ret_pc, Cont::Init(inst))?;
        Ok(true)
    }

    /// Calls user class `cls`: `__new__` and `__init__` (CPython's
    /// `type.__call__`).
    #[allow(clippy::too_many_arguments, reason = "the full call-site description")]
    #[cold]
    fn call_class(
        &mut self,
        st: &mut State,
        cls: Value,
        pos: &[Value],
        kws: &[(Value, Value)],
        call_pc: usize,
        ret_pc: usize,
        cont: Cont,
    ) -> Result<Option<Value>, Raised> {
        if self.uflags(cls) & f::CALL != 0
            && let Some(call) = self.user_method(cls, Dn::Call)
        {
            // The metaclass's `__call__`.
            let mut all = Vec::with_capacity(pos.len() + 1);
            all.push(cls);
            all.extend_from_slice(pos);
            return self.call_vec(st, call, &all, kws, call_pc, ret_pc, cont);
        }
        let fl = cls.as_obj().map_or(0, |r| self.raw(r, cls::FLAGS));
        // A metaclass without `__new__` makes classes with `type.__new__`.
        let meta = self.class_base_t(cls) == T::type_;
        let (func, all, inner) = if fl & f::NEW != 0 || meta {
            let k = self.dn(Dn::New)?;
            let m = self.class_lookup(cls, k).unwrap_or(Value::NONE);
            let m = if m.is_obj_kind(ty::STATICMETHOD as u8) {
                self.vslot(m, 0)
            } else {
                m
            };
            let mut all = Vec::with_capacity(pos.len() + 1);
            all.push(cls);
            all.extend_from_slice(pos);
            let args = self.alloc_tuple(pos)?;
            let kw = if kws.is_empty() {
                Value::NONE
            } else {
                let d = self.new_dict()?;
                for &(k, v) in kws {
                    self.dict_set(d, k, v)?;
                }
                d
            };
            (m, all, Cont::NewInit { cls, args, kw })
        } else if Vm::is_bx_type(self.class_base_t(cls)) {
            // A subclass of a builtin type: the prelude builds the value
            // and runs `__init__`.
            let args = self.alloc_tuple(pos)?;
            let kw = if kws.is_empty() {
                Value::NONE
            } else {
                let d = self.new_dict()?;
                for &(k, v) in kws {
                    self.dict_set(d, k, v)?;
                }
                d
            };
            let f = self.prelude_fn("_bx_call")?;
            return self.call_vec(st, f, &[cls, args, kw], &[], call_pc, ret_pc, cont);
        } else {
            let inst = self.alloc_of_class(cls, pos)?;
            let Some(init) = self.user_method(inst, Dn::Init) else {
                if !kws.is_empty() && self.class_base_t(cls).is_exception() {
                    let name = String::from(self.class_name(cls));
                    self.exc_init_kwargs(inst, &name, kws)?;
                    return Ok(Some(inst));
                }
                if (!pos.is_empty() || !kws.is_empty()) && !self.class_base_t(cls).is_exception() {
                    let msg = format!("{}() takes no arguments", self.class_name(cls));
                    return Err(self.err(T::TypeError, &msg));
                }
                return Ok(Some(inst));
            };
            if init.is_obj_kind(ty::FUNC as u8) {
                let mut all = Vec::with_capacity(pos.len() + 1);
                all.push(inst);
                all.extend_from_slice(pos);
                (init, all, Cont::Init(inst))
            } else {
                // Any other callable binds as an attribute does (a builtin
                // function gets only the arguments).
                let b = self.bind_attr(init, inst, cls)?;
                if b == Value::UNDEFINED {
                    self.request = None;
                    return Err(self.err(
                        T::TypeError,
                        "micro-monty does not support properties as special methods",
                    ));
                }
                (b, pos.to_vec(), Cont::Init(inst))
            }
        };
        let depth = self.depth;
        let (cp, rp) = self.push_tramps(st, &[cont], call_pc, ret_pc)?;
        match self.call_vec(st, func, &all, kws, cp, rp, inner) {
            Ok(None) => Ok(None),
            Ok(Some(v)) => {
                self.pop_tramps(st, depth);
                if v == Value::UNDEFINED {
                    self.request = None;
                    return Err(self.err(
                        T::TypeError,
                        "micro-monty does not support builtin __init__ or __new__ here",
                    ));
                }
                match self.finish(st.base, inner, Outcome::Value(v))? {
                    Next::Acc(x) => Ok(Some(x)),
                    // A builtin `__new__` (a metaclass's `type.__new__`),
                    // then a Python `__init__`.
                    Next::Invoke(f, a, kw, c) => {
                        let kws = if kw == Value::NONE {
                            Vec::new()
                        } else {
                            self.dict_items_vec(kw)
                        };
                        let (cp, rp) = self.push_tramps(st, &[cont], call_pc, ret_pc)?;
                        self.call_vec(st, f, &a, &kws, cp, rp, c)
                    }
                    _ => Err(self.err(
                        T::TypeError,
                        "micro-monty does not support this __new__ result",
                    )),
                }
            }
            Err(e) => {
                self.pop_tramps(st, depth);
                Err(e)
            }
        }
    }

    /// Pushes continuation frames (the one-instruction trampoline) for
    /// `conts`, outermost first; returns the call and return pcs a callee
    /// above them uses.
    #[cold]
    pub(crate) fn push_tramps(
        &mut self,
        st: &mut State,
        conts: &[Cont],
        call_pc: usize,
        ret_pc: usize,
    ) -> Result<(usize, usize), Raised> {
        let mut pcs = (call_pc, ret_pc);
        for &c in conts {
            if c == Cont::None {
                continue;
            }
            let top = st.base + self.codes[st.ci as usize].code.nregs as usize + HDR;
            self.ensure_stack(top + 1)?;
            self.enter_frame(st, Value::NONE, self.tramp, top, pcs.0, pcs.1, c)?;
            pcs = (0, 0);
        }
        Ok(pcs)
    }

    /// Where the dispatch loop goes after an operation landed (`r`):
    /// `None` continues at `st` (after a safepoint); otherwise the loop
    /// returns this. `pc`, `base` and `acc` are the loop's values before
    /// the operation. Out of line to keep the loop small.
    #[cold]
    #[inline(never)]
    fn landing(
        &mut self,
        st: &mut State,
        r: Result<Landed, Raised>,
        pc: usize,
        base: usize,
        acc: Value,
    ) -> Option<Result<Flow, Fault>> {
        match r {
            Ok(Landed::Go) => {
                self.ticks -= 1;
                if self.ticks <= 0
                    && let Some(p) = self.safepoint(st)
                {
                    return Some(Ok(Flow::Park(p)));
                }
                None
            }
            Ok(Landed::Host(call)) => Some(Ok(Flow::Host(call))),
            Ok(Landed::Reraise) => {
                st.acc = acc;
                Some(Err(Fault { reraise: true }))
            }
            Ok(Landed::Fault) => Some(Err(Fault { reraise: false })),
            Ok(Landed::Block(a)) => Some(Ok(Flow::Block(a))),
            Err(Raised) => {
                st.pc = pc;
                st.base = base;
                st.acc = acc;
                Some(Err(Fault { reraise: false }))
            }
        }
    }

    /// The truth of `st.acc` for a conditional jump at `st.pc` that is not
    /// a bool or `None`; `None` when a user `__bool__`/`__len__` was
    /// requested (the jump finishes when it returns).
    #[inline(never)]
    fn jump_truth(&mut self, st: &State, opcode: u8) -> Result<Option<bool>, Raised> {
        let acc = st.acc;
        if let Some(i) = acc.as_int() {
            return Ok(Some(i != 0));
        }
        if self.uflags(acc) & (f::BOOL | f::LEN) != 0 || acc == Value::NOT_IMPLEMENTED {
            let bc = &self.codes[st.ci as usize].code.bytecode;
            let pc = st.pc;
            let target = u32::try_from(operand(bc, pc, 0)).unwrap_or(0);
            self.op_truth(acc, Vm::truth_mode(opcode), target)?;
            return Ok(None);
        }
        Ok(Some(self.truthy(acc)))
    }

    /// The slow end of an operation in the dispatch loop: it left a request
    /// (`failed` false), or failed and needs Python code, which its prelude
    /// twin `twin(*args)` runs (or failed for good). Out of line to keep the
    /// loop small.
    #[cold]
    #[inline(never)]
    #[allow(
        clippy::too_many_arguments,
        reason = "the operation's full description"
    )]
    fn op_slow(
        &mut self,
        st: &mut State,
        failed: bool,
        len: usize,
        cont: Cont,
        twin: &str,
        args: &[Value],
        discard: bool,
    ) -> Option<Result<Flow, Fault>> {
        let pc = st.pc;
        let r = if failed && !self.slow {
            Err(Raised)
        } else if failed {
            self.slow = false;
            let c = if discard { Cont::Discard } else { Cont::None };
            self.prelude_fn(twin)
                .and_then(|f| self.slow_call(f, args.to_vec(), Value::NONE))
                .map(|(f, args, _)| self.request_call(f, args, c))
                .and_then(|_| self.land(st, pc, pc + len, cont))
        } else {
            self.land(st, pc, pc + len, cont)
        };
        let (base, acc) = (st.base, st.acc);
        self.landing(st, r, pc, base, acc)
    }

    /// `BUILD_STRING` at `st.pc` over parts too long for one step: the
    /// stepped `''.join` of them, landing at the next instruction. True
    /// when it raised at `st.pc`.
    #[cold]
    #[inline(never)]
    fn build_string_steps(&mut self, st: &mut State) -> bool {
        let pc = st.pc;
        let w = self.codes[st.ci as usize].code.bytecode[pc];
        let r = st.base + ((w >> 8) & 0xff) as usize;
        let n = (w >> 16) as usize;
        let parts = self.stack[r..r + n].to_vec();
        let res = self.alloc_tuple(&parts).and_then(|t| {
            let sep = self.intern("")?;
            crate::strmeth::join_big(self, sep, t)
        });
        let landed = match res {
            Ok(v) if v != Value::UNDEFINED => {
                st.acc = v;
                st.pc = pc + 1;
                return false;
            }
            // `_steps`' frame, or a `RecursionError` for it.
            Ok(_) => self.land(st, pc, pc + 1, Cont::None),
            Err(Raised) => Err(Raised),
        };
        match landed {
            Ok(Landed::Go) => return false,
            Ok(_) => {
                self.err(T::SystemError, "micro-monty: a stepped join did not land");
            }
            Err(Raised) => {}
        }
        st.pc = pc;
        true
    }

    /// Pops frames above `depth` without running them (continuation frames
    /// a call did not need); `st` becomes the frame below them.
    pub(crate) fn pop_tramps(&mut self, st: &mut State, depth: u32) {
        while self.depth > depth {
            let Some(f) = self.pop_frame(st) else { break };
            self.cont_take(f.cont);
        }
    }

    /// Acts on the request a builtin or an operation left (it returned
    /// `UNDEFINED`) for the instruction at `call_pc`; `op_cont` finishes the
    /// operation itself.
    #[cold]
    pub(crate) fn land(
        &mut self,
        st: &mut State,
        call_pc: usize,
        ret_pc: usize,
        op_cont: Cont,
    ) -> Result<Landed, Raised> {
        match self.request.take() {
            Some(Request::Host(call)) => {
                if op_cont != Cont::None {
                    let (cp, rp) = self.push_tramps(st, &[op_cont], call_pc, ret_pc)?;
                    self.host_call = Some((call.id, u32::try_from(cp).unwrap_or(0)));
                    st.pc = rp;
                    return Ok(Landed::Host(call));
                }
                self.host_call = Some((call.id, u32::try_from(call_pc).unwrap_or(0)));
                st.pc = ret_pc;
                Ok(Landed::Host(call))
            }
            #[cfg(feature = "asyncio")]
            Some(Request::Run(c)) => {
                if self.in_event_loop(st) {
                    return Err(self.err(
                        T::RuntimeError,
                        "asyncio.run() cannot be called from a running event loop",
                    ));
                }
                self.start_coro_frame(st, c, call_pc, ret_pc, Cont::None)?;
                self.mark_run_root(c)?;
                Ok(Landed::Go)
            }
            Some(Request::Call {
                func,
                args,
                kw,
                cont,
                outer,
            }) => {
                let pending: Vec<Cont> = [op_cont, outer]
                    .into_iter()
                    .filter(|c| *c != Cont::None)
                    .collect();
                self.drive(
                    st,
                    Next::Invoke(func, args, kw, cont),
                    call_pc,
                    ret_pc,
                    pending,
                )
            }
            Some(Request::Throw { gn, exc, cont }) => {
                let pending: Vec<Cont> =
                    [op_cont].into_iter().filter(|c| *c != Cont::None).collect();
                self.drive(st, Next::Throw(gn, exc, cont), call_pc, ret_pc, pending)
            }
            None => Err(self.err(T::SystemError, "micro-monty: a builtin returned no value")),
        }
    }

    /// Carries out `next` for the instruction at `call_pc` of `st`, with
    /// `pending` continuations (outermost first) still to finish it.
    #[cold]
    pub(crate) fn drive(
        &mut self,
        st: &mut State,
        mut next: Next,
        call_pc: usize,
        ret_pc: usize,
        mut pending: Vec<Cont>,
    ) -> Result<Landed, Raised> {
        // Builtins that hand off to builtins, counted like frames.
        let mut chained = 0;
        loop {
            next = match next {
                Next::Acc(v) => match pending.pop() {
                    None => {
                        st.acc = v;
                        st.pc = ret_pc;
                        return Ok(Landed::Go);
                    }
                    Some(c) => self.finish(st.base, c, Outcome::Value(v))?,
                },
                Next::Jump(t, v) => {
                    if pending.is_empty() {
                        st.acc = v;
                        st.pc = t as usize;
                        return Ok(Landed::Go);
                    }
                    Next::Acc(v)
                }
                Next::Reraise(e) => {
                    self.exc = e;
                    st.pc = call_pc;
                    return Ok(Landed::Reraise);
                }
                Next::Pass => match pending.pop() {
                    None => return Err(Raised),
                    Some(c) => self.finish(st.base, c, Outcome::Raise)?,
                },
                Next::AwaitYield(y, w) => {
                    if !pending.is_empty() {
                        return Err(self.err(T::SystemError, "micro-monty: bad await step"));
                    }
                    return self.await_yield(st, y, w, call_pc);
                }
                Next::Invoke(..) | Next::Throw(..) => {
                    let depth = self.depth;
                    let (cp, rp) = self.push_tramps(st, &pending, call_pc, ret_pc)?;
                    let (r, cont) = match next {
                        Next::Throw(gn, exc, cont) => (
                            self.gen_throw(st, gn, exc, cont, cp, rp).map(Invoked::from),
                            cont,
                        ),
                        Next::Invoke(f, args, kw, cont) => {
                            let pushed = if kw == Value::NONE {
                                self.push_func(st, f, &args, cp, rp, cont)
                            } else {
                                Ok(false)
                            };
                            let r = match pushed {
                                Ok(true) => Ok(Invoked::Frame),
                                Ok(false) => self.invoke(st, f, &args, kw, cont, cp, rp),
                                Err(e) => Err(e),
                            };
                            (r, cont)
                        }
                        _ => (Ok(Invoked::Frame), Cont::None),
                    };
                    match r {
                        Ok(Invoked::Frame) => return Ok(Landed::Go),
                        Ok(Invoked::Fault) => return Ok(Landed::Fault),
                        Ok(Invoked::Request) if !pending.is_empty() || cont != Cont::None => {
                            if matches!(self.request, Some(Request::Host(_)))
                                && let Some(Request::Host(call)) = self.request.take()
                            {
                                // A host function as a dunder: continuation
                                // frames carry the rest.
                                let (cp2, rp2) = self.push_tramps(st, &[cont], cp, rp)?;
                                self.host_call = Some((call.id, u32::try_from(cp2).unwrap_or(0)));
                                st.pc = rp2;
                                return Ok(Landed::Host(call));
                            }
                            self.pop_tramps(st, depth);
                            pending.push(cont);
                            self.request_next(&mut pending, &mut chained)?
                        }
                        Ok(Invoked::Request) => {
                            if matches!(self.request, Some(Request::Host(_)))
                                && let Some(Request::Host(call)) = self.request.take()
                            {
                                self.host_call = Some((call.id, u32::try_from(cp).unwrap_or(0)));
                                st.pc = rp;
                                return Ok(Landed::Host(call));
                            }
                            self.pop_tramps(st, depth);
                            self.request_next(&mut pending, &mut chained)?
                        }
                        Ok(Invoked::Value(v)) => {
                            self.pop_tramps(st, depth);
                            self.finish(st.base, cont, Outcome::Value(v))?
                        }
                        Ok(Invoked::Out(out)) => {
                            self.pop_tramps(st, depth);
                            self.finish(st.base, cont, out)?
                        }
                        Err(Raised) => {
                            self.pop_tramps(st, depth);
                            self.request = None;
                            self.finish(st.base, cont, Outcome::Raise)?
                        }
                    }
                }
            };
        }
    }

    /// The call a builtin left as its request, as the next step. A chain of
    /// builtins requesting builtins pushes no frame of its own, so it is
    /// bounded here (`object.__str__` as `__repr__` chains without end).
    #[cold]
    fn request_next(
        &mut self,
        pending: &mut Vec<Cont>,
        chained: &mut usize,
    ) -> Result<Next, Raised> {
        *chained += 1;
        // Each hop re-pushes the pending continuations as frames, so a
        // longer chain costs quadratic time; CPython counted C recursion
        // against the same limit.
        if *chained >= self.recursion_limit as usize {
            self.request = None;
            let _ = self.err(
                T::RecursionError,
                "maximum recursion depth exceeded while calling a Python object",
            );
            return Ok(Next::Pass);
        }
        match self.request.take() {
            Some(Request::Call {
                func,
                args,
                kw,
                cont,
                outer,
            }) => {
                if outer != Cont::None {
                    pending.push(outer);
                }
                Ok(Next::Invoke(func, args, kw, cont))
            }
            Some(Request::Throw { gn, exc, cont }) => Ok(Next::Throw(gn, exc, cont)),
            _ => Err(self.err(
                T::SystemError,
                "micro-monty: unsupported request in a continuation",
            )),
        }
    }

    /// Calls (a generator: sends to) `f` from `st`, a call from `call_pc`
    /// returning to `ret_pc` through `cont`.
    #[allow(clippy::too_many_arguments, reason = "the full call-site description")]
    #[cold]
    fn invoke(
        &mut self,
        st: &mut State,
        f: Value,
        args: &[Value],
        kw: Value,
        cont: Cont,
        call_pc: usize,
        ret_pc: usize,
    ) -> Result<Invoked, Raised> {
        if Vm::is_gen(f) {
            let v = args.first().copied().unwrap_or(Value::NONE);
            return self
                .gen_resume(st, f, v, cont, call_pc, ret_pc)
                .map(Invoked::from);
        }
        if f.is_obj_kind(ty::CORO as u8) {
            // `coroutine.send(None)`: its frame runs as an `await` would.
            self.start_coro_frame(st, f, call_pc, ret_pc, cont)?;
            return Ok(Invoked::Frame);
        }
        let kws: Vec<(Value, Value)> = if kw.is_obj_kind(ty::DICT as u8) {
            self.dict_items_vec(kw)
        } else {
            Vec::new()
        };
        match self.call_vec(st, f, args, &kws, call_pc, ret_pc, cont)? {
            None => Ok(Invoked::Frame),
            Some(v) if v == Value::UNDEFINED => Ok(Invoked::Request),
            Some(v) => Ok(Invoked::Value(v)),
        }
    }

    /// A deref op or a closure given something other than a cell, which
    /// only bytecode the compiler never emits can do.
    #[cold]
    #[inline(never)]
    fn not_a_cell(&mut self) -> Raised {
        self.err(T::SystemError, "micro-monty: not a cell")
    }

    /// Whether `v` is a tuple of cells, as `MAKE_FUNCTION` takes a closure.
    #[inline(never)]
    fn is_closure(&self, v: Value) -> bool {
        v.is_obj_kind(ty::TUPLE as u8)
            && (0..self.tuple_len(v)).all(|i| {
                let i = u32::try_from(i).unwrap_or(u32::MAX);
                self.vslot(v, i).is_obj_kind(ty::CELL as u8)
            })
    }

    fn vslot_raw(&self, v: Value, i: u32) -> u64 {
        v.as_obj().map_or(0, |r| self.raw(r, i))
    }

    fn func_qualname(&self, func: Value) -> String {
        String::from(self.str_ref(self.vslot(func, fn_slot::QUALNAME)))
    }

    /// The general argument binder for Python functions: positional,
    /// keywords, defaults, `*args`, `**kwargs`, keyword-only (CPython's
    /// rules and messages). Writes the parameters from `base`. `rest`, when
    /// given, is `*args` already built from the arguments past `pos`.
    #[allow(
        clippy::too_many_lines,
        reason = "CPython's binding algorithm in one place"
    )]
    fn bind_args(
        &mut self,
        func: Value,
        code: &Arc<CodeObj>,
        pos: &[Value],
        kws: &[(Value, Value)],
        base: usize,
        rest: Option<Value>,
    ) -> Result<(), Raised> {
        let p = code.code.params.clone();
        let argc = p.argcount as usize;
        let nkwonly = p.kwonly as usize;
        let total = p.total() as usize;
        for v in &mut self.stack[base..base + total] {
            *v = Value::UNDEFINED;
        }
        let name = self.func_qualname(func);
        let names: Vec<String> = code
            .code
            .varnames
            .iter()
            .take(argc + nkwonly)
            .map(|s| String::from(&**s))
            .collect();
        // Positional.
        let n = pos.len().min(argc);
        self.stack[base..base + n].copy_from_slice(&pos[..n]);
        let vararg_slot = argc + nkwonly;
        let kwarg_slot = vararg_slot + usize::from(p.varargs);
        let mut overflow: Option<String> = None;
        if p.varargs {
            let extra = if pos.len() > argc {
                &pos[argc..]
            } else {
                &[][..]
            };
            self.stack[base + vararg_slot] = match rest {
                Some(t) => t,
                None => self.alloc_tuple(extra)?,
            };
        } else if pos.len() > argc {
            let defaults = self.vslot(func, fn_slot::DEFAULTS);
            let ndef = if defaults.is_obj_kind(ty::TUPLE as u8) {
                self.tuple_len(defaults)
            } else {
                0
            };
            let kw_given = kws
                .iter()
                .filter(|(k, _)| {
                    let k = self.str_ref(*k);
                    names[argc..].iter().any(|n| n == k)
                })
                .count();
            let plural = |n: usize| if n == 1 { "" } else { "s" };
            let takes = if ndef > 0 {
                format!(
                    "from {} to {argc} positional arguments",
                    argc as i64 - ndef as i64
                )
            } else {
                format!("{argc} positional argument{}", plural(argc))
            };
            let given = if kw_given > 0 {
                format!(
                    "{} positional argument{} (and {kw_given} keyword-only argument{}) were given",
                    pos.len(),
                    plural(pos.len()),
                    plural(kw_given)
                )
            } else {
                format!(
                    "{} {} given",
                    pos.len(),
                    if pos.len() == 1 { "was" } else { "were" }
                )
            };
            // Reported after the keywords, as CPython does.
            overflow = Some(format!("{name}() takes {takes} but {given}"));
        }
        let kwdict = if p.varkw {
            let d = self.new_dict()?;
            self.stack[base + kwarg_slot] = d;
            Some(d)
        } else {
            None
        };
        // Keywords.
        let mut posonly_kw: Vec<String> = Vec::new();
        for &(k, v) in kws {
            let key = String::from(self.str_ref(k));
            let found = names.iter().position(|n| *n == key);
            match found {
                Some(i) if i >= p.posonly as usize => {
                    if self.stack[base + i] != Value::UNDEFINED {
                        return Err(self.err(
                            T::TypeError,
                            &format!("{name}() got multiple values for argument '{key}'"),
                        ));
                    }
                    self.stack[base + i] = v;
                }
                Some(_) if kwdict.is_none() => posonly_kw.push(key),
                _ => {
                    if let Some(d) = kwdict {
                        if self.dict_get(d, k)?.is_some() {
                            return Err(self.err(
                                T::TypeError,
                                &format!(
                                    "{name}() got multiple values for keyword argument '{key}'"
                                ),
                            ));
                        }
                        self.dict_set(d, k, v)?;
                    } else {
                        let cands: Vec<&str> = names.iter().map(String::as_str).collect();
                        let mut msg =
                            format!("{name}() got an unexpected keyword argument '{key}'");
                        if let Some(s) = crate::exc::suggest(&cands, &key) {
                            msg.push_str(". Did you mean '");
                            msg.push_str(s);
                            msg.push_str("'?");
                        }
                        return Err(self.err(T::TypeError, &msg));
                    }
                }
            }
        }
        if !posonly_kw.is_empty() {
            let list = posonly_kw
                .iter()
                .map(|n| format!("'{n}'"))
                .collect::<Vec<_>>()
                .join(", ");
            return Err(self.err(
                T::TypeError,
                &format!("{name}() got some positional-only arguments passed as keyword arguments: {list}"),
            ));
        }
        if let Some(msg) = overflow {
            return Err(self.err(T::TypeError, &msg));
        }
        // Defaults for positional parameters.
        let defaults = self.vslot(func, fn_slot::DEFAULTS);
        let dvals = if defaults.is_obj_kind(ty::TUPLE as u8) {
            self.tuple_items(defaults)
        } else {
            Vec::new()
        };
        // __defaults__ may contain more entries than positional parameters.
        let dvals = &dvals[dvals.len().saturating_sub(argc)..];
        let first_default = argc - dvals.len();
        let mut missing: Vec<String> = Vec::new();
        for i in 0..argc {
            if self.stack[base + i] == Value::UNDEFINED {
                if i >= first_default {
                    self.stack[base + i] = dvals[i - first_default];
                } else {
                    missing.push(names[i].clone());
                }
            }
        }
        if !missing.is_empty() {
            return Err(self.missing(&name, &missing, "positional"));
        }
        // Keyword-only defaults.
        let kwdefaults = self.vslot(func, fn_slot::KWDEFAULTS);
        for (i, pname) in names.iter().enumerate().take(argc + nkwonly).skip(argc) {
            if self.stack[base + i] == Value::UNDEFINED {
                let mut filled = false;
                if kwdefaults.is_obj_kind(ty::DICT as u8) {
                    let k = self.intern(pname)?;
                    if let Some(v) = self.dict_get(kwdefaults, k)? {
                        self.stack[base + i] = v;
                        filled = true;
                    }
                }
                if !filled {
                    missing.push(pname.clone());
                }
            }
        }
        if !missing.is_empty() {
            return Err(self.missing(&name, &missing, "keyword-only"));
        }
        Ok(())
    }

    fn missing(&mut self, name: &str, missing: &[String], kind: &str) -> Raised {
        let quoted: Vec<String> = missing.iter().map(|m| format!("'{m}'")).collect();
        let list = match quoted.len() {
            1 => quoted[0].clone(),
            2 => format!("{} and {}", quoted[0], quoted[1]),
            n => format!("{}, and {}", quoted[..n - 1].join(", "), quoted[n - 1]),
        };
        let n = missing.len();
        self.err(
            T::TypeError,
            &format!(
                "{name}() missing {n} required {kind} argument{}: {list}",
                if n == 1 { "" } else { "s" }
            ),
        )
    }

    fn make_function(
        &mut self,
        st: &State,
        child: u32,
        flags: u8,
        r: usize,
    ) -> Result<Value, Raised> {
        let cidx = self.codes[st.ci as usize].module_base + child;
        let Some(code) = self.codes.get(cidx as usize).cloned() else {
            return Err(self.err(T::SystemError, "bad code index"));
        };
        let mut k = r;
        let mut take = |bit: u8, vm: &Vm| {
            if flags & bit != 0 {
                let v = vm.stack[k];
                k += 1;
                v
            } else {
                Value::NONE
            }
        };
        let defaults = take(1, self);
        let kwdefaults = take(2, self);
        let closure = take(4, self);
        drop(code);
        self.new_function(cidx, defaults, kwdefaults, closure)
    }

    /// A function of code `cidx`, in the globals its code runs in.
    pub(crate) fn new_function(
        &mut self,
        cidx: u32,
        defaults: Value,
        kwdefaults: Value,
        closure: Value,
    ) -> Result<Value, Raised> {
        // A frame's free variables are read from the closure unchecked, and
        // the deref ops take only cells: a closure is a tuple of cells.
        if closure != Value::NONE && !self.is_closure(closure) {
            return Err(self.not_a_cell());
        }
        let Some(code) = self.codes.get(cidx as usize).cloned() else {
            return Err(self.err(T::SystemError, "bad code index"));
        };
        let name = self.intern(&code.code.name)?;
        let qual = self.intern(&code.code.qualname)?;
        let g = self.globals_of(&code);
        let f = {
            let res = self.heap.alloc_init(
                ty::FUNC,
                0,
                &[name, qual, defaults, kwdefaults, closure, g, Value::NONE],
                &[u64::from(cidx)],
            );
            self.hx(res)
        }?;
        Ok(Value::from_ref(f, ty::FUNC as u8))
    }

    // --------------------------------------------------------- helpers

    fn load_global_slow(&mut self, st: &State, n: usize, ic: usize) -> Result<Value, Raised> {
        let name = self.codes[st.ci as usize].names[n];
        // Engine code has namespaces of its own.
        let g = self.globals_of(&self.codes[st.ci as usize]);
        if let Some(e) = self.dict_find_entry(g, name)? {
            let entries = self.dict_entries(g);
            let v = self
                .dict_entry(g, e as usize)
                .map_or(Value::UNDEFINED, |kv| kv.1);
            if let Some(entries) = entries {
                self.ics[ic] = Ic::Global {
                    version: self.gver,
                    entries,
                    idx: e,
                };
            }
            return Ok(v);
        }
        // Prelude code: then the prelude's units, before builtins.
        if self.codes[st.ci as usize].gslot == crate::vm::GSLOT_PRELUDE
            && let Some(v) = self.prelude_unit(name)?
        {
            return Ok(v);
        }
        if let Some(v) = self.builtin_get(name)? {
            self.ics[ic] = Ic::Builtin {
                version: self.gver.wrapping_add(self.bver),
                value: v,
            };
            return Ok(v);
        }
        let s = String::from(self.str_ref(name));
        let e = self.err(T::NameError, &format!("name '{s}' is not defined"));
        let locals = self.hint_locals(st, name);
        self.set_name_hint(name, &locals);
        Err(e)
    }

    /// A builtin exception type called with keywords (`NameError(name=…)`),
    /// out of the plain call's way.
    #[cold]
    #[inline(never)]
    fn exc_with_kwargs(
        &mut self,
        t: T,
        args: usize,
        npos: usize,
        kwnames: Value,
    ) -> Result<Value, Raised> {
        let items: Vec<Value> = self.stack[args..args + npos].to_vec();
        let e = self.new_exception(t, &items)?;
        let kws: Vec<(Value, Value)> = self
            .tuple_items(kwnames)
            .into_iter()
            .enumerate()
            .map(|(k, n)| (n, self.stack[args + npos + k]))
            .collect();
        self.exc_init_kwargs(e, t.name(), &kws)?;
        Ok(e)
    }

    /// `raise e`: a `NameError` keeps the raising frame's names for its
    /// suggestion, as CPython's traceback reads them from the last frame.
    /// Every `raise` asks, so it tests one flag until guest code has built
    /// a `NameError` (the interpreter's own carry their names already).
    #[inline]
    fn note_raise(&mut self, st: &State, e: Value) -> Result<(), Raised> {
        if self.name_error_built {
            self.note_raise_slow(st, e)?;
        }
        Ok(())
    }

    #[cold]
    #[inline(never)]
    fn note_raise_slow(&mut self, st: &State, e: Value) -> Result<(), Raised> {
        if !e.is_obj_kind(ty::EXC as u8) || !self.wants_raise_names(e) {
            return Ok(());
        }
        let name = self.exc_name_attr(e, "name").unwrap_or(Value::NONE);
        let locals = self.hint_locals(st, name);
        self.note_raised_name_error(e, self.globals, &locals)
    }

    /// The frame's bound local names for a `NameError`'s hint, or
    /// [`crate::exc::SELF_HAS_NAME`] when its `self` has attribute `name`
    /// (CPython then suggests `self.name`).
    fn hint_locals(&self, st: &State, name: Value) -> Vec<String> {
        let code = &self.codes[st.ci as usize].code;
        if !code.is_module
            && Vm::is_str(name)
            && let Some(i) = code.varnames.iter().position(|n| &**n == "self")
        {
            let mut v = self
                .stack
                .get(st.base + i)
                .copied()
                .unwrap_or(Value::UNDEFINED);
            if v.is_obj_kind(ty::CELL as u8) {
                v = self.vslot(v, 0);
            }
            if let Some(c) = self.user_class(v)
                && (self.inst_get(v, name).is_some() || self.class_lookup(c, name).is_some())
            {
                return alloc::vec![String::from(crate::exc::SELF_HAS_NAME)];
            }
        }
        self.bound_locals(st)
    }

    fn bound_locals(&self, st: &State) -> Vec<String> {
        let mut out = Vec::new();
        if self.codes[st.ci as usize].code.is_module {
            return out;
        }
        for (i, n) in self.codes[st.ci as usize].code.varnames.iter().enumerate() {
            let v = self
                .stack
                .get(st.base + i)
                .copied()
                .unwrap_or(Value::UNDEFINED);
            let bound = if v.is_obj_kind(ty::CELL as u8) {
                self.vslot(v, 0) != Value::UNDEFINED
            } else {
                v != Value::UNDEFINED
            };
            if bound {
                out.push(String::from(&**n));
            }
        }
        out
    }

    fn unbound_local(&mut self, st: &State, r: usize) -> Raised {
        let pc = u32::try_from(st.pc).unwrap_or(0);
        let name = u8::try_from(r)
            .ok()
            .and_then(|r8| self.codes[st.ci as usize].code.reg_name(r8, pc))
            .map_or_default(String::from);
        let is_free = self.codes[st.ci as usize]
            .code
            .freevars
            .iter()
            .any(|&f| f as usize == r);
        if is_free {
            self.err(
                T::NameError,
                &format!("cannot access free variable '{name}' where it is not associated with a value in enclosing scope"),
            )
        } else {
            self.err(
                T::UnboundLocalError,
                &format!(
                    "cannot access local variable '{name}' where it is not associated with a value"
                ),
            )
        }
    }

    /// The exception to raise for `raise v`.
    pub(crate) fn raisable(&mut self, v: Value) -> Result<Value, Raised> {
        if v.is_obj_kind(ty::EXC as u8) {
            return Ok(v);
        }
        if let Some(t) = self.type_of_typeobj(v)
            && t.is_exception()
        {
            return self.new_exception(t, &[]);
        }
        if Vm::is_class(v) && self.class_base_t(v).is_exception() {
            return self.alloc_of_class(v, &[]);
        }
        Err(self.err(T::TypeError, "exceptions must derive from BaseException"))
    }

    fn exc_matches(&mut self, exc: Value, typ: Value) -> Result<bool, Raised> {
        let user = |vm: &Vm, t: Value| Vm::is_class(t) && vm.class_base_t(t).is_exception();
        let tuple_has_class =
            typ.is_obj_kind(ty::TUPLE as u8) && self.tuple_items(typ).into_iter().any(Vm::is_class);
        if Vm::is_class(typ) || self.user_class(exc).is_some() || tuple_has_class {
            let items = if typ.is_obj_kind(ty::TUPLE as u8) {
                self.tuple_items(typ)
            } else {
                alloc::vec![typ]
            };
            let mut hit = false;
            for t in items {
                let ok = user(self, t) || self.type_of_typeobj(t).is_some_and(T::is_exception);
                if !ok {
                    return Err(self.err(
                        T::TypeError,
                        "catching classes that do not inherit from BaseException is not allowed",
                    ));
                }
                hit |= self.subclass_of(self.class_of(exc), t);
            }
            return Ok(hit);
        }
        let et = self.exc_type(exc);
        if let Some(t) = self.type_of_typeobj(typ) {
            if !t.is_exception() {
                return Err(self.err(
                    T::TypeError,
                    "catching classes that do not inherit from BaseException is not allowed",
                ));
            }
            return Ok(et.is_subclass(t));
        }
        if typ.is_obj_kind(ty::TUPLE as u8) {
            // Every item must be an exception class; nested tuples are not
            // descended into (CPython's `check_except_type_valid`).
            let mut hit = false;
            for t in self.tuple_items(typ) {
                match self.type_of_typeobj(t) {
                    Some(t) if t.is_exception() => hit |= et.is_subclass(t),
                    _ => {
                        return Err(self.err(
                            T::TypeError,
                            "catching classes that do not inherit from BaseException is not allowed",
                        ));
                    }
                }
            }
            return Ok(hit);
        }
        Err(self.err(
            T::TypeError,
            "catching classes that do not inherit from BaseException is not allowed",
        ))
    }

    /// An f-string field's conversion, for `FORMAT` and `FORMAT_SPEC`
    /// only (they then format a value that is not a str, which can be
    /// long work: C4's `stop_before`).
    fn convert(&mut self, v: Value, conv: u8) -> Result<Value, Raised> {
        if !Vm::is_str(v) && self.stop_before() {
            return Err(Raised);
        }
        match conv {
            b'r' => {
                let s = self.repr(v)?;
                self.new_str(&s)
            }
            b'a' => {
                let s = self.repr(v)?;
                let ascii = crate::percent::ascii_escape(self, s)?;
                self.new_str(&ascii)
            }
            b's' => {
                if Vm::is_str(v) {
                    return Ok(v);
                }
                let s = self.stringify(v)?;
                self.new_str(&s)
            }
            _ => Ok(v),
        }
    }

    fn dict_update_op(
        &mut self,
        st: &State,
        d: Value,
        mode: u8,
        key: Value,
        src: Value,
        callee_reg: usize,
    ) -> Result<(), Raised> {
        if mode == 2 {
            if self.dict_get(d, key)?.is_some() {
                let f = self.callee_name(self.stack[callee_reg]);
                let k = String::from(self.str_ref(key));
                return Err(self.err(
                    T::TypeError,
                    &format!("{f} got multiple values for keyword argument '{k}'"),
                ));
            }
            return self.dict_set(d, key, src);
        }
        if !src.is_obj_kind(ty::DICT as u8) {
            let msg = if mode == 1 {
                let f = self.callee_name(self.stack[callee_reg]);
                format!(
                    "{f} argument after ** must be a mapping, not {}",
                    self.type_name(src)
                )
            } else {
                format!("'{}' object is not a mapping", self.type_name(src))
            };
            return Err(self.err(T::TypeError, &msg));
        }
        if mode == 0 {
            // `d.update(src)`, in steps when long.
            use crate::set::{act, phase, res};
            return self
                .bulk(&[d, src], phase(act::DICT, 0, 1, 0), res::NONE, true)
                .map(|_| ());
        }
        for (k, v) in self.dict_items_charged(src) {
            if mode == 1 {
                if !Vm::is_str(k) {
                    return Err(self.err(T::TypeError, "keywords must be strings"));
                }
                if self.dict_get(d, k)?.is_some() {
                    let f = self.callee_name(self.stack[callee_reg]);
                    let ks = String::from(self.str_ref(k));
                    return Err(self.err(
                        T::TypeError,
                        &format!("{f} got multiple values for keyword argument '{ks}'"),
                    ));
                }
            }
            self.dict_set(d, k, v)?;
        }
        let _ = st;
        Ok(())
    }

    /// CPython's `_PyObject_FunctionStr`: `module.qualname()` unless the
    /// module is `builtins`.
    fn callee_name(&self, f: Value) -> String {
        if f.is_obj_kind(ty::FUNC as u8) {
            let g = self.vslot(f, fn_slot::GLOBALS);
            let m = self
                .find_interned("__name__")
                .filter(|_| g.is_obj_kind(ty::DICT as u8))
                .and_then(|k| self.dict_get_str(g, k))
                .filter(|m| Vm::is_str(*m) && self.str_ref(*m) != "builtins");
            return match m {
                Some(m) => format!("{}.{}()", self.str_ref(m), self.func_qualname(f)),
                None => format!("{}()", self.func_qualname(f)),
            };
        }
        if let Some(n) = f.as_native().and_then(builtins::native) {
            return match n.owner {
                Some(t) => format!("{}.{}()", t.name(), n.name()),
                None => format!("{}()", n.name()),
            };
        }
        if let Some(t) = self.type_of_typeobj(f) {
            return format!("{}()", t.name());
        }
        String::from("function")
    }

    /// A class-body name that is not in the namespace: the global or the
    /// builtin.
    fn load_name_slow(&mut self, st: &State, name: Value, _slot: usize) -> Result<Value, Raised> {
        let g = self.globals_of(&self.codes[st.ci as usize]);
        if let Some(v) = self.dict_get(g, name)? {
            return Ok(v);
        }
        if let Some(v) = self.builtin_get(name)? {
            return Ok(v);
        }
        let s = String::from(self.str_ref(name));
        let e = self.err(T::NameError, &format!("name '{s}' is not defined"));
        let locals = self.hint_locals(st, name);
        self.set_name_hint(name, &locals);
        Err(e)
    }

    /// Instance `o` and its slots, validated once, if its shape is `shape`
    /// and its class's version is `version`.
    #[inline]
    fn inst_guard(&self, o: Value, version: u64, shape: u32) -> Option<(mm_core::Ref, &[u64])> {
        if !o.is_obj_kind(ty::INST as u8) {
            return None;
        }
        let r = o.as_obj()?;
        let w = self.heap.slot_words(r).ok()?;
        if *w.get(inst::SHAPE as usize)? != Value::int(i64::from(shape))?.to_bits() {
            return None;
        }
        // The kind byte proves a class, which the instance keeps alive.
        let c = Value::from_bits(*w.get(inst::CLASS as usize)?)?;
        let cr = c.as_obj().filter(|_| c.is_obj_kind(ty::CLASS as u8))?;
        (self.heap.get_raw_guarded(cr, cls::VERSION) == version).then_some((r, w))
    }

    /// `GET_ATTR` through its inline cache.
    #[inline]
    fn attr_ic_get(&self, o: Value, version: u64, shape: u32, idx: u32) -> Option<Value> {
        let (_, w) = self.inst_guard(o, version, shape)?;
        let v = Value::from_bits(*w.get(idx as usize)?)?;
        (v != Value::UNDEFINED).then_some(v)
    }

    /// Fills `GET_ATTR`'s cache after `o.name` read an inline attribute.
    fn attr_ic_fill(&mut self, o: Value, name: Value, slot: usize) {
        let c = self.vslot(o, inst::CLASS);
        if self
            .class_lookup(c, name)
            .is_some_and(|d| d.is_obj_kind(ty::PROPERTY as u8) || self.uflags(d) & f::DESCR != 0)
        {
            return;
        }
        if let Some((s, k)) = self.inst_inline_slot(o, name)
            && let Some(cr) = c.as_obj()
        {
            self.ics[slot] = Ic::Attr {
                version: self.raw(cr, cls::VERSION),
                shape: s,
                idx: k,
            };
        }
    }

    /// `SET_ATTR` through its inline cache; `false` if it does not apply.
    #[inline]
    fn attr_ic_set(
        &mut self,
        o: Value,
        version: u64,
        shape: u32,
        to: u32,
        idx: u32,
        v: Value,
    ) -> bool {
        let Some((r, len)) = self
            .inst_guard(o, version, shape)
            .map(|(r, w)| (r, w.len()))
        else {
            return false;
        };
        if idx as usize >= len {
            return false;
        }
        // The guard proved `r` an instance (all value slots) of `len` slots.
        self.heap.set_guarded(r, idx, v);
        if to != shape {
            self.heap.set_guarded(
                r,
                inst::SHAPE,
                Value::int(i64::from(to)).unwrap_or(Value::NONE),
            );
        }
        true
    }

    /// Does `LOAD_METHOD`'s cache apply to `o`?
    #[inline]
    fn method_ic_ok(&self, o: Value, version: u64, shape: u32) -> bool {
        self.inst_guard(o, version, shape).is_some()
    }

    /// `o.name` for a method call when it is a plain function of the class
    /// that the instance does not shadow (no bound method is made); fills
    /// the cache.
    fn plain_method(&mut self, o: Value, name: Value, slot: usize) -> Option<Value> {
        let c = self.class_of(o);
        let d = self.class_lookup(c, name)?;
        if !d.is_obj_kind(ty::FUNC as u8) || self.inst_get(o, name).is_some() {
            return None;
        }
        if let Some(s) = self.shape_of(o)
            && o.is_obj_kind(ty::INST as u8)
            && let Some(cr) = c.as_obj()
        {
            self.ics[slot] = Ic::UMethod {
                version: self.raw(cr, cls::VERSION),
                shape: s,
                func: d,
            };
        }
        Some(d)
    }

    /// One step of an `iter(callable, sentinel)` iterator: a call request
    /// (nothing when it is exhausted).
    pub(crate) fn calliter_next(&mut self, it: Value, exit: u32) -> Result<Value, Raised> {
        if self.vslot(it, 1) == Value::UNDEFINED {
            return Ok(Value::NONE);
        }
        let f = self.vslot(it, 0);
        self.next_request(Next::Invoke(
            f,
            Args::of(&[]),
            Value::NONE,
            Cont::CallIter { it, exit },
        ))
    }

    // --------------------------------------------------------- dispatch

    #[allow(
        clippy::too_many_lines,
        reason = "the dispatch loop: one arm per opcode"
    )]
    #[allow(clippy::cognitive_complexity, reason = "the dispatch loop")]
    #[allow(
        clippy::needless_continue,
        reason = "`goto_frame!` continues the loop from any arm"
    )]
    fn dispatch(&mut self, st: &mut State) -> Result<Flow, Fault> {
        let mut pc = st.pc;
        let mut base = st.base;
        let mut acc = st.acc;

        macro_rules! fail {
            ($reraise:expr) => {{
                st.pc = pc;
                st.base = base;
                st.acc = acc;
                return Err(Fault { reraise: $reraise });
            }};
        }
        macro_rules! tri {
            ($e:expr) => {
                match $e {
                    Ok(v) => v,
                    Err(Raised) => fail!(false),
                }
            };
        }

        let mut codes = self.codes.clone();
        'frames: loop {
            if st.ci as usize >= codes.len() {
                // Code loaded during this run (exec, eval, import).
                codes = self.codes.clone();
            }
            let Some(mut code) = codes.get(st.ci as usize) else {
                fail!(false);
            };
            let mut bc: &[u32] = &code.code.bytecode;
            let mut icb = code.ic_base as usize;
            // Switches the loop to the code of frame `st.ci` in place
            // (restarting the frame loop if the table copy lacks it).
            macro_rules! goto_frame {
                () => {{
                    match codes.get(st.ci as usize) {
                        Some(c) => {
                            code = c;
                            bc = &c.code.bytecode;
                            icb = c.ic_base as usize;
                            continue;
                        }
                        None => continue 'frames,
                    }
                }};
            }
            macro_rules! reg {
                ($r:expr) => {
                    self.stack[base + $r]
                };
            }
            // The instruction word at `pc`, fetched by the loop below.
            let mut w: u32;
            // Operands of the instruction word `w` at `pc` (see
            // `mm_compile::bytecode::layout`); `x` is its second word.
            macro_rules! wa {
                () => {
                    ((w >> 8) & 0xff) as usize
                };
            }
            macro_rules! wb {
                () => {
                    ((w >> 16) & 0xff) as usize
                };
            }
            macro_rules! wc {
                () => {
                    (w >> 24) as usize
                };
            }
            macro_rules! wu1 {
                () => {
                    ((w >> 8) & 0xffff) as usize
                };
            }
            macro_rules! wu2 {
                () => {
                    (w >> 16) as usize
                };
            }
            macro_rules! wj {
                () => {
                    (w >> 8) as usize
                };
            }
            macro_rules! wx {
                () => {
                    bc[pc + 1] as usize
                };
            }
            // The register and jump operands of the one-word instruction a
            // fused opcode moved `pc` to.
            macro_rules! na {
                () => {
                    ((bc[pc] >> 8) & 0xff) as usize
                };
            }
            macro_rules! nj {
                () => {
                    (bc[pc] >> 8) as usize
                };
            }
            // Writes the loop registers back, for helpers that inspect `st`.
            macro_rules! sync {
                () => {
                    st.pc = pc;
                    st.base = base;
                    st.acc = acc;
                };
            }
            macro_rules! tick {
                () => {
                    self.ticks -= 1;
                    if self.ticks <= 0 {
                        sync!();
                        if let Some(r) = self.safepoint(st) {
                            return Ok(Flow::Park(r));
                        }
                    }
                };
            }
            // Enters a simple Python callee whose arguments are already in
            // place: `enter_frame` inlined for the common case (no depth
            // check, no frame-table growth, no continuation).
            macro_rules! push_frame {
                ($callee:expr, $cidx:expr, $new_base:expr, $ret_pc:expr) => {{
                    let depth = self.depth + 1;
                    debug_assert_eq!($ret_pc, pc + CALL_LEN);
                    debug_assert_eq!(instr_len(op_of(bc[pc])), CALL_LEN);
                    if (depth as usize) < self.recursion_limit as usize
                        && st.coro == Value::NONE
                        && let Some(lk) = link($ret_pc, st.ci, $new_base - base)
                        && let Some(callee_code) = codes.get($cidx as usize)
                    {
                        if !callee_code.code.freevars.is_empty() {
                            // The call cache proved `$callee` is this live
                            // function, and `MAKE_FUNCTION` gave it a
                            // closure tuple with one cell per free variable.
                            let closure = $callee.as_obj().map_or(Value::UNDEFINED, |f| {
                                self.heap.get_guarded(f, fn_slot::CLOSURE)
                            });
                            if let Some(c) = closure.as_obj() {
                                for (k, &r) in callee_code.code.freevars.iter().enumerate() {
                                    self.stack[$new_base + r as usize] =
                                        self.heap.get_guarded(c, k as u32);
                                }
                            }
                        }
                        if let Some(h) = self.stack.get_mut($new_base - HDR..$new_base) {
                            h[0] = st.func;
                            h[1] = lk;
                        }
                        self.depth = depth;
                        st.ci = $cidx;
                        st.func = $callee;
                        st.coro = Value::NONE;
                        st.base = $new_base;
                        st.pc = 0;
                    } else {
                        sync!();
                        tri!(self.enter_frame(
                            st,
                            $callee,
                            $cidx,
                            $new_base,
                            pc,
                            $ret_pc,
                            Cont::None
                        ));
                    }
                }};
            }
            // One `FOR_RANGE` step for instruction word `$fw` at `pc` with
            // int counters (current, stop, step in `R[r..r+3]`): `false`
            // (nothing done) for any other instruction or counters.
            macro_rules! range_step {
                ($fw:expr) => {{
                    let fw: u32 = $fw;
                    let fop = op_of(fw);
                    let r = ((fw >> 8) & 0xff) as usize;
                    let mut stepped = false;
                    if (fop == op::FOR_RANGE || fop == op::FOR_RANGE_STAR)
                        && let Some([c, e, d]) = self.stack.get_mut(base + r..base + r + 3)
                        && let (Some(cur), Some(stop), Some(step)) =
                            (c.as_int(), e.as_int(), d.as_int())
                    {
                        stepped = true;
                        if (step > 0 && cur < stop) || (step < 0 && cur > stop) {
                            acc = *c;
                            // Past the small-int range only beyond the stop.
                            *c = Value::int(cur + step).unwrap_or(*e);
                            pc += 2;
                            if fop == op::FOR_RANGE_STAR {
                                reg!(na!()) = acc;
                                pc += 1;
                            }
                        } else {
                            pc = bc.get(pc + 1).map_or(0, |&x| x as usize);
                        }
                    }
                    stepped
                }};
            }
            // After an operation left a request: run it (`st` says where to
            // go on).
            macro_rules! landed {
                ($r:expr) => {{
                    let r = $r;
                    landed!(@exit self.landing(st, r, pc, base, acc))
                }};
                (@exit $x:expr) => {{
                    match $x {
                        None => {
                            pc = st.pc;
                            base = st.base;
                            acc = st.acc;
                            continue 'frames;
                        }
                        Some(exit) => return exit,
                    }
                }};
            }
            // An operation's result: a value; or `UNDEFINED` with a
            // request, finished by `$cont`; or "needs Python code", handed
            // to the prelude twin `$twin` (name, arguments).
            macro_rules! op_result {
                ($e:expr, $len:expr, $cont:expr, $twin:expr) => {{
                    match $e {
                        Ok(v) if v != Value::UNDEFINED => {
                            acc = v;
                            pc += words!($len);
                        }
                        r => {
                            sync!();
                            let (name, args): (&str, &[Value]) = $twin;
                            landed!(@exit self.op_slow(st, r.is_err(), words!($len), $cont, name, args, false));
                        }
                    }
                }};
            }
            // An operation without a result (stores and deletions).
            macro_rules! op_unit {
                ($e:expr, $len:expr, $twin:expr) => {{
                    match $e {
                        Ok(()) if self.request.is_none() => pc += words!($len),
                        r => {
                            sync!();
                            let (name, args): (&str, &[Value]) = $twin;
                            landed!(@exit self.op_slow(
                                st,
                                r.is_err(),
                                words!($len),
                                Cont::None,
                                name,
                                args,
                                true
                            ));
                        }
                    }
                }};
            }
            // A generator resumed or thrown into: its frame runs next, or
            // `$cont` finishes the instruction.
            macro_rules! resumed {
                ($r:expr, $ret:expr, $cont:expr) => {{
                    match $r {
                        Ok(Resumed::Frame) => {
                            pc = st.pc;
                            base = st.base;
                            acc = st.acc;
                            tick!();
                            continue 'frames;
                        }
                        Ok(Resumed::Fault) => {
                            pc = st.pc;
                            base = st.base;
                            acc = st.acc;
                            fail!(false);
                        }
                        Ok(Resumed::Out(o)) => {
                            let (at, ret) = (pc, $ret);
                            let r = match self.finish(base, $cont, o) {
                                Ok(n) => self.drive(st, n, at, ret, Vec::new()),
                                Err(e) => Err(e),
                            };
                            landed!(r);
                        }
                        Err(Raised) => fail!(false),
                    }
                }};
            }
            let smi = |i: usize| Value::int(i as i64).unwrap_or(Value::NONE);
            loop {
                w = bc[pc];
                let opcode = op_of(w);
                match opcode {
                    op::NOP => pc += 1,
                    op::LDAR => {
                        acc = reg!(wa!());
                        pc += 1;
                    }
                    op::STAR => {
                        reg!(wa!()) = acc;
                        pc += 1;
                    }
                    op::MOV => {
                        let v = reg!(wa!());
                        reg!(wb!()) = v;
                        pc += 1;
                    }
                    op::LDA_LOCAL => {
                        let r = wa!();
                        let v = reg!(r);
                        if v == Value::UNDEFINED {
                            sync!();
                            let e = self.unbound_local(st, r);
                            let _ = e;
                            fail!(false);
                        }
                        acc = v;
                        pc += 1;
                    }
                    op::LDA_SMI | op::LDA_SMI_ADD | op::LDA_SMI_SUB => {
                        let raw = i64::from(u16::try_from(wu1!()).unwrap_or(0).cast_signed());
                        acc = Value::int(raw).unwrap_or(Value::NONE);
                        pc += 1;
                        // `R[r] + imm` / `R[r] - imm` for a small int in the
                        // following `ADD r` / `SUB r`; anything else runs it.
                        if opcode != op::LDA_SMI
                            && let Some(x) = reg!(na!()).as_int()
                            && let Some(v) = Value::int(if opcode == op::LDA_SMI_ADD {
                                x + raw
                            } else {
                                x - raw
                            })
                        {
                            acc = v;
                            pc += 1;
                        }
                    }
                    op::LDA_CONST => {
                        acc = code.consts[wu1!()];
                        pc += 1;
                    }
                    op::LDA_NONE => {
                        acc = Value::NONE;
                        pc += 1;
                    }
                    op::LDA_TRUE => {
                        acc = Value::TRUE;
                        pc += 1;
                    }
                    op::LDA_FALSE => {
                        acc = Value::FALSE;
                        pc += 1;
                    }
                    op::LDA_GLOBAL | op::LDA_GLOBAL_STAR => {
                        let slot = icb + (wx!() & 0xffff);
                        // A deleted global's entry reads `UNDEFINED`.
                        let hit = match self.ics[slot] {
                            Ic::Global {
                                version,
                                entries,
                                idx,
                            } if version == self.gver => {
                                self.heap.get_guarded(entries, 2 * idx + 1)
                            }
                            Ic::Builtin { version, value }
                                if version == self.gver.wrapping_add(self.bver) =>
                            {
                                value
                            }
                            _ => Value::UNDEFINED,
                        };
                        acc = if hit == Value::UNDEFINED {
                            sync!();
                            tri!(self.load_global_slow(st, wu1!(), slot))
                        } else {
                            hit
                        };
                        pc += 2;
                        if opcode == op::LDA_GLOBAL_STAR {
                            reg!(na!()) = acc;
                            pc += 1;
                        }
                    }
                    op::STA_GLOBAL => {
                        let slot = icb + (wx!() & 0xffff);
                        // A deleted global's entry has an `UNDEFINED` key.
                        let hit = match self.ics[slot] {
                            Ic::Global {
                                version,
                                entries,
                                idx,
                            } if version == self.gver => {
                                self.heap.set_guarded_live(entries, 2 * idx, acc)
                            }
                            _ => false,
                        };
                        if !hit {
                            let name = code.names[wu1!()];
                            let g = self.globals_of(code);
                            tri!(self.dict_set(g, name, acc));
                            if let Some(e) = tri!(self.dict_find_entry(g, name))
                                && let Some(entries) = self.dict_entries(g)
                            {
                                self.ics[slot] = Ic::Global {
                                    version: self.gver,
                                    entries,
                                    idx: e,
                                };
                            }
                        }
                        pc += 2;
                    }
                    // A deref of anything but a cell (only bytecode the
                    // compiler never emits) raises in `dispatch_rare`.
                    op::LDA_DEREF if reg!(wa!()).is_obj_kind(ty::CELL as u8) => {
                        let r = wa!();
                        let c = reg!(r);
                        // A cell value's kind byte proves its one slot.
                        let v = match c.as_obj() {
                            Some(cr) => self.heap.get_guarded(cr, 0),
                            None => Value::UNDEFINED,
                        };
                        if v == Value::UNDEFINED {
                            sync!();
                            let _ = self.unbound_local(st, r);
                            fail!(false);
                        }
                        acc = v;
                        pc += 1;
                    }
                    op::STA_DEREF if reg!(wa!()).is_obj_kind(ty::CELL as u8) => {
                        if let Some(c) = reg!(wa!()).as_obj() {
                            self.set_slot(c, 0, acc);
                        }
                        pc += 1;
                    }
                    op::MAKE_CELL => {
                        let r = wa!();
                        let c = tri!(self.new_cell(reg!(r)));
                        reg!(r) = c;
                        pc += 1;
                    }
                    op::ADD => {
                        let a = reg!(wa!());
                        if let (Some(x), Some(y)) = (a.as_int(), acc.as_int())
                            && let Some(v) = Value::int(x + y)
                        {
                            acc = v;
                        } else if let (Some(x), Some(y)) = (a.as_float(), acc.as_float()) {
                            acc = Value::float(x + y);
                        } else {
                            op_result!(
                                self.op_binop(BinOp::Add, a, acc, false),
                                2,
                                Cont::None,
                                (
                                    "_op_binop",
                                    &[a, acc, smi(BinOp::Add as usize), Value::FALSE]
                                )
                            );
                            continue;
                        }
                        pc += 1;
                    }
                    op::SUB => {
                        let a = reg!(wa!());
                        if let (Some(x), Some(y)) = (a.as_int(), acc.as_int())
                            && let Some(v) = Value::int(x - y)
                        {
                            acc = v;
                        } else if let (Some(x), Some(y)) = (a.as_float(), acc.as_float()) {
                            acc = Value::float(x - y);
                        } else {
                            op_result!(
                                self.op_binop(BinOp::Sub, a, acc, false),
                                2,
                                Cont::None,
                                (
                                    "_op_binop",
                                    &[a, acc, smi(BinOp::Sub as usize), Value::FALSE]
                                )
                            );
                            continue;
                        }
                        pc += 1;
                    }
                    op::MUL => {
                        let a = reg!(wa!());
                        if let (Some(x), Some(y)) = (a.as_int(), acc.as_int())
                            && let Some(v) = x.checked_mul(y).and_then(Value::int)
                        {
                            acc = v;
                        } else if let (Some(x), Some(y)) = (a.as_float(), acc.as_float()) {
                            acc = Value::float(x * y);
                        } else {
                            op_result!(
                                self.op_binop(BinOp::Mul, a, acc, false),
                                2,
                                Cont::None,
                                (
                                    "_op_binop",
                                    &[a, acc, smi(BinOp::Mul as usize), Value::FALSE]
                                )
                            );
                            continue;
                        }
                        pc += 1;
                    }
                    op::BINOP | op::INPLACE => {
                        let bop = BinOp::ALL.get(wa!()).copied().unwrap_or(BinOp::Add);
                        let a = reg!(wb!());
                        // Small-int `+ - * % //` without leaving the loop (a
                        // positive divisor floors as Euclid's division does).
                        if let (Some(x), Some(y)) = (a.as_int(), acc.as_int())
                            && let Some(v) = match bop {
                                BinOp::Add => x.checked_add(y),
                                BinOp::Sub => x.checked_sub(y),
                                BinOp::Mul => x.checked_mul(y),
                                BinOp::Mod if y > 0 => Some(x.rem_euclid(y)),
                                BinOp::FloorDiv if y > 0 => Some(x.div_euclid(y)),
                                _ => None,
                            }
                            .and_then(Value::int)
                        {
                            acc = v;
                            pc += 1;
                            continue;
                        }
                        let inplace = opcode == op::INPLACE;
                        op_result!(
                            self.op_binop(bop, a, acc, inplace),
                            3,
                            Cont::None,
                            (
                                "_op_binop",
                                &[a, acc, smi(bop as usize), Value::from_bool(inplace)]
                            )
                        );
                    }
                    op::NEG | op::POS | op::INVERT => {
                        op_result!(
                            self.op_unary(opcode, acc),
                            1,
                            Cont::None,
                            ("_op_unary", &[acc, smi(opcode as usize)])
                        );
                    }
                    op::NOT => {
                        if self.uflags(acc) & (f::BOOL | f::LEN) != 0
                            || acc == Value::NOT_IMPLEMENTED
                        {
                            sync!();
                            if let Err(Raised) = self.op_truth(acc, crate::cont::truth::NOT, 0) {
                                fail!(false);
                            }
                            landed!(self.land(st, pc, pc + 1, Cont::None));
                        }
                        acc = Value::from_bool(!self.truthy(acc));
                        pc += 1;
                    }
                    op::TEST_LT
                    | op::TEST_LE
                    | op::TEST_GT
                    | op::TEST_GE
                    | op::TEST_LT_JF
                    | op::TEST_LE_JF
                    | op::TEST_GT_JF
                    | op::TEST_GE_JF => {
                        let cmp = match opcode {
                            op::TEST_LT_JF => op::TEST_LT,
                            op::TEST_LE_JF => op::TEST_LE,
                            op::TEST_GT_JF => op::TEST_GT,
                            op::TEST_GE_JF => op::TEST_GE,
                            o => o,
                        };
                        let a = reg!(wa!());
                        if let (Some(x), Some(y)) = (a.as_int(), acc.as_int()) {
                            let b = match cmp {
                                op::TEST_LT => x < y,
                                op::TEST_LE => x <= y,
                                op::TEST_GT => x > y,
                                _ => x >= y,
                            };
                            acc = Value::from_bool(b);
                            pc += 1;
                            // The following `JUMP_IF_FALSE`, on a known bool.
                            if cmp != opcode {
                                pc = if b { pc + 1 } else { nj!() };
                            }
                        } else {
                            op_result!(
                                self.op_compare(cmp, a, acc),
                                2,
                                Cont::None,
                                ("_op_cmp", &[a, acc, smi(cmp as usize)])
                            );
                        }
                    }
                    op::TEST_EQ | op::TEST_NE => {
                        let a = reg!(wa!());
                        if let (Some(x), Some(y)) = (a.as_int(), acc.as_int()) {
                            acc = Value::from_bool((x == y) == (opcode == op::TEST_EQ));
                            pc += 1;
                        } else if Vm::is_str(a)
                            && Vm::is_str(acc)
                            && let Some(eq) = self.str_eq_short(a, acc)
                        {
                            // `str == str` without the comparison walk, unless
                            // it reads long texts (the walk charges those).
                            acc = Value::from_bool(eq == (opcode == op::TEST_EQ));
                            pc += 1;
                        } else {
                            let twin = if opcode == op::TEST_EQ {
                                "_op_eq"
                            } else {
                                "_op_ne"
                            };
                            op_result!(
                                self.op_compare(opcode, a, acc),
                                2,
                                Cont::None,
                                (twin, &[a, acc])
                            );
                        }
                    }
                    op::TEST_IN | op::TEST_NOT_IN | op::TEST_IS | op::TEST_IS_NOT => {
                        let a = reg!(wa!());
                        // Identity needs no operation.
                        if opcode == op::TEST_IS || opcode == op::TEST_IS_NOT {
                            acc = Value::from_bool((a == acc) == (opcode == op::TEST_IS));
                            pc += 1;
                            continue;
                        }
                        let twin = if opcode == op::TEST_NOT_IN {
                            "_op_not_contains"
                        } else {
                            "_op_contains"
                        };
                        op_result!(
                            self.op_compare(opcode, a, acc),
                            2,
                            Cont::None,
                            (twin, &[acc, a])
                        );
                    }
                    op::JUMP => pc = wj!(),
                    op::JUMP_LOOP => {
                        pc = wj!();
                        tick!();
                        // Most loops jump back to a `FOR_RANGE`: step it here.
                        if let Some(&fw) = bc.get(pc)
                            && range_step!(fw)
                        {
                            continue;
                        }
                    }
                    op::JUMP_IF_FALSE => {
                        if acc == Value::TRUE {
                            pc += 1;
                        } else if acc == Value::FALSE || acc == Value::NONE {
                            pc = wj!();
                        } else if let Some(i) = acc.as_int() {
                            if i != 0 {
                                pc += 1;
                            } else {
                                pc = wj!();
                            }
                        } else {
                            sync!();
                            match self.jump_truth(st, op::JUMP_IF_FALSE) {
                                Ok(Some(true)) => pc += 1,
                                Ok(Some(false)) => pc = wj!(),
                                Ok(None) => landed!(self.land(st, pc, pc + 1, Cont::None)),
                                Err(Raised) => fail!(false),
                            }
                        }
                    }
                    op::JUMP_IF_TRUE => {
                        if acc == Value::TRUE {
                            pc = wj!();
                        } else if acc == Value::FALSE || acc == Value::NONE {
                            pc += 1;
                        } else if let Some(i) = acc.as_int() {
                            if i != 0 {
                                pc = wj!();
                            } else {
                                pc += 1;
                            }
                        } else {
                            sync!();
                            match self.jump_truth(st, op::JUMP_IF_TRUE) {
                                Ok(Some(true)) => pc = wj!(),
                                Ok(Some(false)) => pc += 1,
                                Ok(None) => landed!(self.land(st, pc, pc + 1, Cont::None)),
                                Err(Raised) => fail!(false),
                            }
                        }
                    }
                    op::GET_ITER => {
                        op_result!(self.op_get_iter(acc), 1, Cont::None, ("_op_iter", &[acc]));
                    }
                    op::FOR_PREP => {
                        let r = wa!();
                        let n = wb!();
                        let arg = |k: usize| self.stack[base + r + 1 + k.min(n - 1)].as_int();
                        let counters = match (arg(0), arg(1), arg(2)) {
                            (Some(a), _, _) if n == 1 => Some((0, a, 1)),
                            (Some(a), Some(b), _) if n == 2 => Some((a, b, 1)),
                            (Some(a), Some(b), Some(c)) if c != 0 => Some((a, b, c)),
                            _ => None,
                        };
                        let is_range = reg!(r) == self.types[T::range as usize];
                        if is_range
                            && let Some((a, b, c)) = counters
                            && let (Some(a), Some(b), Some(c)) =
                                (Value::int(a), Value::int(b), Value::int(c))
                        {
                            reg!(r) = a;
                            reg!(r + 1) = b;
                            reg!(r + 2) = c;
                            pc = wx!();
                            // A safepoint, as the call it replaces.
                            tick!();
                        } else {
                            pc += 2;
                        }
                    }
                    op::FOR_ITER | op::FOR_ITER_STAR | op::FOR_RANGE | op::FOR_RANGE_STAR => {
                        // The next item, and the following `STAR` if fused.
                        macro_rules! next_item {
                            () => {{
                                pc += 2;
                                if opcode == op::FOR_ITER_STAR || opcode == op::FOR_RANGE_STAR {
                                    reg!(na!()) = acc;
                                    pc += 1;
                                }
                            }};
                        }
                        if opcode >= op::FOR_RANGE && range_step!(w) {
                            continue;
                        }
                        let r = wa!();
                        let it = reg!(r);
                        // A range iterator steps with one validated access.
                        let range_next = if it.is_obj_kind(ty::ITER as u8)
                            && let Some(o) = it.as_obj()
                            && let Ok([kind, cur, left, step, ..]) = self.heap.raw_words_mut(o)
                            && *kind == crate::ops::it::RANGE
                        {
                            if *left != 0 {
                                let c = cur.cast_signed();
                                // Wraps only past the last item, never read.
                                *cur = cur.wrapping_add(*step);
                                *left -= 1;
                                Some(Some(c))
                            } else {
                                Some(None)
                            }
                        } else {
                            None
                        };
                        if let Some(next) = range_next {
                            if let Some(c) = next {
                                acc = match Value::int(c) {
                                    Some(v) => v,
                                    None => tri!(self.new_int(c)),
                                };
                                next_item!();
                            } else {
                                pc = wx!();
                            }
                        } else if Vm::is_native_iter(it) {
                            match tri!(self.iter_next(it)) {
                                Some(v) => {
                                    acc = v;
                                    next_item!();
                                }
                                None => pc = wx!(),
                            }
                        } else {
                            let exit = u32::try_from(wx!()).unwrap_or(0);
                            let cont = Cont::ForIter(exit);
                            sync!();
                            if it.is_obj_kind(ty::GEN as u8)
                                && tri!(self.gen_enter_fast(st, it, cont, pc, pc + 2))
                            {
                                pc = st.pc;
                                base = st.base;
                                acc = Value::NONE;
                                tick!();
                                continue 'frames;
                            }
                            if it.is_obj_kind(ty::GEN as u8) {
                                resumed!(
                                    self.gen_resume(st, it, Value::NONE, cont, pc, pc + 2),
                                    pc + 2,
                                    cont
                                );
                            }
                            let r = if it.is_obj_kind(ty::CALLITER as u8) {
                                self.calliter_next(it, exit)
                            } else {
                                self.op_user_next(it, cont)
                            };
                            if let Err(Raised) = r {
                                fail!(false);
                            }
                            if self.request.is_none() {
                                pc = exit as usize;
                                continue;
                            }
                            landed!(self.land(st, pc, pc + 2, Cont::None));
                        }
                    }
                    op::BUILD_LIST | op::BUILD_TUPLE | op::BUILD_SET => {
                        let r = base + wa!();
                        let n = wu2!();
                        // Short displays copy their items on the stack.
                        let mut small = [Value::NONE; 16];
                        let heap_items;
                        let items: &[Value] = if let Some(s) = small.get_mut(..n) {
                            s.copy_from_slice(&self.stack[r..r + n]);
                            s
                        } else {
                            heap_items = self.stack[r..r + n].to_vec();
                            &heap_items
                        };
                        match opcode {
                            op::BUILD_LIST => acc = tri!(self.new_list(items)),
                            op::BUILD_TUPLE => acc = tri!(self.alloc_tuple(items)),
                            _ => {
                                let r = self.set_from(items);
                                if r.is_err() && self.slow {
                                    let l = tri!(self.new_list(items));
                                    op_result!(r, 4, Cont::None, ("_op_build_set", &[l]));
                                    continue;
                                }
                                acc = tri!(r);
                            }
                        }
                        pc += 1;
                    }
                    op::BUILD_DICT => {
                        let r = base + wa!();
                        let n = wu2!();
                        let d = tri!(if n == 0 {
                            self.new_dict()
                        } else {
                            self.new_dict_for(n)
                        });
                        let mut res = Ok(d);
                        for k in 0..n {
                            let (key, v) = (self.stack[r + 2 * k], self.stack[r + 2 * k + 1]);
                            if let Err(e) = self.dict_set(d, key, v) {
                                res = Err(e);
                                break;
                            }
                        }
                        if res.is_err() && self.slow {
                            let items: Vec<Value> = self.stack[r..r + 2 * n].to_vec();
                            let l = tri!(self.new_list(&items));
                            op_result!(res, 4, Cont::None, ("_op_build_dict", &[l]));
                            continue;
                        }
                        acc = tri!(res);
                        pc += 1;
                    }
                    op::BUILD_STRING => {
                        let r = base + wa!();
                        let n = wu2!();
                        let bytes = self.stack[r..r + n].iter().fold(0usize, |len, &v| {
                            len.saturating_add(self.str_bytes(v).len())
                        });
                        acc = tri!(self.build_str(bytes, |vm, buf| {
                            for k in 0..n {
                                vm.sbuf_push(buf, vm.stack[r + k]);
                            }
                            Ok(())
                        }));
                        pc += 1;
                    }
                    op::LIST_APPEND => {
                        let l = reg!(wa!());
                        tri!(self.list_push(l, acc));
                        pc += 1;
                    }
                    op::DICT_INSERT => {
                        let d = reg!(wa!());
                        let k = reg!(wb!());
                        op_unit!(
                            self.dict_set_top(d, k, acc),
                            3,
                            ("_op_setitem", &[d, k, acc])
                        );
                    }
                    op::UNPACK_SEQ => {
                        let r = base + wa!();
                        let n = wu2!();
                        match self.unpack(acc, n) {
                            Ok(items) => {
                                self.stack[r..r + n].copy_from_slice(&items);
                                pc += 1;
                            }
                            Err(e) => {
                                let cont = Cont::Unpack {
                                    r: u8::try_from(wa!()).unwrap_or(0),
                                    n: u16::try_from(n).unwrap_or(0),
                                };
                                op_result!(
                                    Err::<Value, Raised>(e),
                                    4,
                                    cont,
                                    ("_op_unpack", &[acc, smi(n)])
                                );
                            }
                        }
                    }
                    op::FORMAT => {
                        let conv = u8::try_from(wa!()).unwrap_or(u8::MAX);
                        let r = self.convert(acc, conv).and_then(|v| {
                            if Vm::is_str(v) {
                                Ok(v)
                            } else if let Some(i) = v.as_int() {
                                self.int_str(i)
                            } else {
                                let s = self.format(v, FormatSpec::Text(""))?;
                                self.new_str(&s)
                            }
                        });
                        let empty = tri!(self.intern(""));
                        op_result!(
                            r,
                            2,
                            Cont::None,
                            ("_op_fmt", &[acc, smi(conv as usize), empty])
                        );
                    }
                    op::GET_ATTR => {
                        let slot = icb + (wx!() & 0xffff);
                        if let Ic::Attr {
                            version,
                            shape,
                            idx,
                        } = self.ics[slot]
                            && let Some(v) = self.attr_ic_get(acc, version, shape, idx)
                        {
                            acc = v;
                            pc += 2;
                            continue;
                        }
                        let name = code.names[wu1!()];
                        let obj = acc;
                        let r = self.getattr(obj, name);
                        if let Ok(v) = r
                            && v != Value::UNDEFINED
                            && obj.is_obj_kind(ty::INST as u8)
                        {
                            self.attr_ic_fill(obj, name, slot);
                        }
                        op_result!(r, 5, Cont::None, ("_unreachable", &[]));
                    }
                    op::SET_ATTR => {
                        let o = reg!(wa!());
                        let slot = icb + (wx!() & 0xffff);
                        if let Ic::SetAttr {
                            version,
                            shape,
                            to,
                            idx,
                        } = self.ics[slot]
                            && self.attr_ic_set(o, version, shape, to, idx, acc)
                        {
                            pc += 2;
                            continue;
                        }
                        let name = code.names[wu2!()];
                        if o.is_obj_kind(ty::INST as u8) {
                            match self.inst_setattr(o, name, acc) {
                                Ok(Some((s0, s1, k))) => {
                                    let c = self.vslot(o, inst::CLASS);
                                    if let Some(cr) = c.as_obj() {
                                        self.ics[slot] = Ic::SetAttr {
                                            version: self.raw(cr, cls::VERSION),
                                            shape: s0,
                                            to: s1,
                                            idx: k,
                                        };
                                    }
                                    pc += 2;
                                }
                                r => op_unit!(r.map(|_| ()), 6, ("_unreachable", &[])),
                            }
                        } else {
                            op_unit!(self.setattr(o, name, acc), 6, ("_unreachable", &[]));
                        }
                    }
                    op::LOAD_METHOD => {
                        let r = base + wa!();
                        let slot = icb + (wx!() & 0xffff);
                        if let Ic::UMethod {
                            version,
                            shape,
                            func,
                        } = self.ics[slot]
                            && self.method_ic_ok(acc, version, shape)
                        {
                            self.stack[r] = func;
                            self.stack[r + 2] = acc;
                            pc += 2;
                            continue;
                        }
                        if self.user_class(acc).is_some() {
                            let name = code.names[wu2!()];
                            if let Some(func) = self.plain_method(acc, name, slot) {
                                self.stack[r] = func;
                                self.stack[r + 2] = acc;
                                pc += 2;
                                continue;
                            }
                            self.stack[r + 2] = Value::UNDEFINED;
                            match self.getattr(acc, name) {
                                Ok(v) if v != Value::UNDEFINED => {
                                    self.stack[r] = v;
                                    pc += 2;
                                }
                                Ok(_) => {
                                    sync!();
                                    let cont = Cont::StoreReg(u8::try_from(wa!()).unwrap_or(0));
                                    landed!(self.land(st, pc, pc + 2, cont));
                                }
                                Err(Raised) => fail!(false),
                            }
                            continue;
                        }
                        let tid = self.type_of(acc) as u8;
                        if let Ic::Method { tid: t, value } = self.ics[slot]
                            && t == tid
                        {
                            self.stack[r] = value;
                            self.stack[r + 2] = acc;
                        } else {
                            let name = code.names[wu2!()];
                            let owner = self.method_owner(acc);
                            let found = if self.type_of(acc) == T::type_ {
                                None
                            } else {
                                builtins::find_method(owner, self.str_ref(name))
                            };
                            if let Some(f) = found {
                                let value = Value::native(f);
                                self.ics[slot] = Ic::Method { tid, value };
                                self.stack[r] = value;
                                self.stack[r + 2] = acc;
                            } else {
                                self.stack[r + 2] = Value::UNDEFINED;
                                let v = tri!(self.getattr(acc, name));
                                if v == Value::UNDEFINED {
                                    sync!();
                                    let cont = Cont::StoreReg(u8::try_from(wa!()).unwrap_or(0));
                                    landed!(self.land(st, pc, pc + 2, cont));
                                }
                                self.stack[r] = v;
                            }
                        }
                        pc += 2;
                    }
                    op::GET_ITEM => {
                        let o = reg!(wa!());
                        if o.is_obj_kind(ty::LIST as u8)
                            && let Some(i) = acc.as_int()
                            && let Ok(k) = u64::try_from(i)
                            && let Some((buf, n)) = self.list_parts(o)
                            && k < n
                        {
                            acc = self.slot(buf, u32::try_from(k).unwrap_or(u32::MAX));
                        } else {
                            op_result!(
                                self.op_getitem(o, acc),
                                2,
                                Cont::None,
                                ("_op_getitem", &[o, acc])
                            );
                            continue;
                        }
                        pc += 1;
                    }
                    op::SET_ITEM => {
                        let o = reg!(wa!());
                        let k = reg!(wb!());
                        op_unit!(self.op_setitem(o, k, acc), 3, ("_op_setitem", &[o, k, acc]));
                    }
                    op::CALL => {
                        let f = wa!();
                        let argc = wb!();
                        let slot = icb + (wx!() & 0xffff);
                        let callee = reg!(f);
                        let ret_pc = pc + 2;
                        if let Ic::Call { func, code: cidx } = self.ics[slot]
                            && func == callee
                            && let Some(callee_code) = codes.get(cidx as usize)
                        {
                            // Fast path: a known Python function with simple
                            // parameters and the right argument count. The
                            // arguments in `R[f+2..]` become its first
                            // registers; `R[f]` and `R[f+1]` its header.
                            let new_base = base + f + HDR;
                            let nregs = callee_code.code.nregs as usize;
                            if self.stack.len() < new_base + nregs + 1 {
                                tri!(self.ensure_stack(new_base + nregs + 1));
                            }
                            for v in &mut self.stack[new_base + argc..new_base + nregs] {
                                *v = Value::UNDEFINED;
                            }
                            push_frame!(callee, cidx, new_base, ret_pc);
                            pc = 0;
                            base = new_base;
                            tick!();
                            goto_frame!();
                        }
                        sync!();
                        // `cls(...)` through `Ic::New`: the instance, then `__init__`.
                        if callee.is_obj_kind(ty::CLASS as u8)
                            && tri!(self.call_new(
                                st,
                                callee,
                                base + f + HDR,
                                argc,
                                slot,
                                pc,
                                ret_pc
                            ))
                        {
                            pc = 0;
                            base = st.base;
                            tick!();
                            continue 'frames;
                        }
                        match tri!(self.call_value(
                            st,
                            callee,
                            base + f + HDR,
                            argc,
                            Value::NONE,
                            pc,
                            ret_pc,
                            Some(slot),
                            Cont::None
                        )) {
                            Some(v) if v != Value::UNDEFINED => {
                                acc = v;
                                pc = ret_pc;
                                tick!();
                            }
                            Some(_) => landed!(self.land(st, pc, ret_pc, Cont::None)),
                            None => {
                                pc = 0;
                                base = st.base;
                                tick!();
                                continue 'frames;
                            }
                        }
                    }
                    op::CALL_METHOD | op::CALL_KW | op::CALL_METHOD_KW => {
                        let f = wa!();
                        let argc = wb!();
                        let k = wx!() & 0xffff;
                        let ret_pc = pc + 2;
                        let kwnames = if opcode == op::CALL_METHOD {
                            Value::NONE
                        } else {
                            code.consts[k]
                        };
                        let nkw = if kwnames == Value::NONE {
                            0
                        } else {
                            self.tuple_len(kwnames)
                        };
                        let callee = reg!(f);
                        // Arguments from `R[f+2]` (`R[f+1]` is kept for the
                        // callee's link); a method's `self`, or `UNDEFINED`
                        // for a plain function, is `R[f+2]`.
                        let (args, npos) = if opcode == op::CALL_KW {
                            (base + f + HDR, argc - nkw)
                        } else if reg!(f + 2) == Value::UNDEFINED {
                            (base + f + 3, argc - nkw)
                        } else {
                            (base + f + 2, argc - nkw + 1)
                        };
                        if opcode == op::CALL_METHOD
                            && let Ic::Call { func, code: cidx } = self.ics[icb + k]
                            && func == callee
                            && let Some(callee_code) = codes.get(cidx as usize)
                            && callee_code.code.params.argcount as usize == npos
                        {
                            // Fast path: a method call to a known Python
                            // function with simple parameters, its arguments
                            // (`self` first) in place.
                            let new_base = args;
                            let nregs = callee_code.code.nregs as usize;
                            if self.stack.len() < new_base + nregs + 1 {
                                tri!(self.ensure_stack(new_base + nregs + 1));
                            }
                            for v in &mut self.stack[new_base + npos..new_base + nregs] {
                                *v = Value::UNDEFINED;
                            }
                            push_frame!(callee, cidx, new_base, ret_pc);
                            pc = 0;
                            base = new_base;
                            tick!();
                            goto_frame!();
                        }
                        if nkw == 0
                            && let Some(idx) = callee.as_native()
                        {
                            let top = base + code.code.nregs as usize;
                            if self.stack.len() < top + 64 {
                                tri!(self.ensure_stack(top + 64));
                            }
                            self.frame_base = base;
                            self.direct_call = true;
                            acc = tri!(self.call_native(
                                idx,
                                u32::try_from(args).unwrap_or(0),
                                u32::try_from(npos).unwrap_or(0),
                                Value::NONE,
                                u32::try_from(top).unwrap_or(0),
                            ));
                            if acc == Value::UNDEFINED {
                                sync!();
                                landed!(self.land(st, pc, ret_pc, Cont::None));
                            }
                            pc = ret_pc;
                            tick!();
                            continue;
                        }
                        let ic = if opcode == op::CALL_METHOD {
                            Some(icb + k)
                        } else {
                            None
                        };
                        sync!();
                        match tri!(self.call_value(
                            st,
                            callee,
                            args,
                            npos,
                            kwnames,
                            pc,
                            ret_pc,
                            ic,
                            Cont::None
                        )) {
                            Some(v) if v != Value::UNDEFINED => {
                                acc = v;
                                pc = ret_pc;
                                tick!();
                            }
                            Some(_) => landed!(self.land(st, pc, ret_pc, Cont::None)),
                            None => {
                                pc = 0;
                                base = st.base;
                                tick!();
                                continue 'frames;
                            }
                        }
                    }
                    op::MAKE_FUNCTION => {
                        let child = u32::try_from(wu1!()).unwrap_or(0);
                        let flags = u8::try_from(wc!()).unwrap_or(u8::MAX);
                        let r = base + (wx!() & 0xff);
                        acc = tri!(self.make_function(st, child, flags, r));
                        pc += 2;
                    }
                    // A plain return to a Python caller; everything else
                    // (generators, coroutines, continuations, the bottom
                    // frame) is in `dispatch_rare`.
                    op::RETURN
                        if st.coro == Value::NONE
                            && let Some(&[func, lk]) =
                                self.stack.get(base.wrapping_sub(HDR)..base)
                            && let (ret_pc, ci, off) = unlink(lk)
                            && off != 0
                            && (ci as usize) < codes.len() =>
                    {
                        st.ci = ci;
                        st.func = func;
                        base -= off;
                        pc = ret_pc;
                        self.depth -= 1;
                        goto_frame!();
                    }
                    _ => {
                        sync!();
                        let r = self.dispatch_rare(st, opcode);
                        pc = st.pc;
                        base = st.base;
                        acc = st.acc;
                        match r {
                            Ok(Rare::Next) => {}
                            Ok(Rare::Frames) => continue 'frames,
                            Ok(Rare::Exit(f)) => return Ok(f),
                            Err(f) => return Err(f),
                        }
                    }
                }
            }
        }
    }

    /// The rarely executed opcodes, kept out of [`Self::dispatch`] so its
    /// hot arms keep their registers. Runs one instruction at `st.pc`.
    #[allow(
        clippy::too_many_lines,
        reason = "the cold half of the dispatch loop: one arm per opcode"
    )]
    #[inline(never)]
    fn dispatch_rare(&mut self, st: &mut State, opcode: u8) -> Result<Rare, Fault> {
        let mut pc = st.pc;
        let mut base = st.base;
        let mut acc = st.acc;

        macro_rules! fail {
            ($reraise:expr) => {{
                st.pc = pc;
                st.base = base;
                st.acc = acc;
                return Err(Fault { reraise: $reraise });
            }};
        }
        macro_rules! tri {
            ($e:expr) => {
                match $e {
                    Ok(v) => v,
                    Err(Raised) => fail!(false),
                }
            };
        }
        let code = self.codes[st.ci as usize].clone();
        let bc: &[u32] = &code.code.bytecode;
        // Operand `k` of the instruction at `pc`, in format order.
        macro_rules! opd {
            ($k:expr) => {
                operand(bc, pc, $k)
            };
        }
        macro_rules! ilen {
            () => {
                instr_len(op_of(bc[pc]))
            };
        }
        macro_rules! reg {
            ($r:expr) => {
                self.stack[base + $r]
            };
        }
        // Writes the loop registers back, for helpers that inspect `st`.
        macro_rules! sync {
            () => {
                st.pc = pc;
                st.base = base;
                st.acc = acc;
            };
        }
        macro_rules! tick {
            () => {
                self.ticks -= 1;
                if self.ticks <= 0 {
                    sync!();
                    if let Some(r) = self.safepoint(st) {
                        return Ok(Rare::Exit(Flow::Park(r)));
                    }
                }
            };
        }
        // After an operation left a request: run it (`st` says where to
        // go on).
        macro_rules! landed {
            ($r:expr) => {{
                let r = $r;
                landed!(@exit self.landing(st, r, pc, base, acc))
            }};
            (@exit $x:expr) => {{
                match $x {
                    None => {
                        pc = st.pc;
                        base = st.base;
                        acc = st.acc;
                        {
                            sync!();
                            return Ok(Rare::Frames);
                        }
                    }
                    Some(exit) => return exit.map(Rare::Exit),
                }
            }};
        }
        // An operation's result: a value; or `UNDEFINED` with a
        // request, finished by `$cont`; or "needs Python code", handed
        // to the prelude twin `$twin` (name, arguments).
        macro_rules! op_result {
            ($e:expr, $len:expr, $cont:expr, $twin:expr) => {{
                match $e {
                    Ok(v) if v != Value::UNDEFINED => {
                        acc = v;
                        pc += words!($len);
                    }
                    r => {
                        sync!();
                        let (name, args): (&str, &[Value]) = $twin;
                        landed!(@exit self.op_slow(st, r.is_err(), words!($len), $cont, name, args, false));
                    }
                }
            }};
        }
        // An operation without a result (stores and deletions).
        macro_rules! op_unit {
            ($e:expr, $len:expr, $twin:expr) => {{
                match $e {
                    Ok(()) if self.request.is_none() => pc += words!($len),
                    r => {
                        sync!();
                        let (name, args): (&str, &[Value]) = $twin;
                        landed!(@exit self.op_slow(st, r.is_err(), words!($len), Cont::None, name, args, true));
                    }
                }
            }};
        }
        // A generator resumed or thrown into: its frame runs next, or
        // `$cont` finishes the instruction.
        macro_rules! resumed {
            ($r:expr, $ret:expr, $cont:expr) => {{
                match $r {
                    Ok(Resumed::Frame) => {
                        pc = st.pc;
                        base = st.base;
                        acc = st.acc;
                        tick!();
                        {
                            sync!();
                            return Ok(Rare::Frames);
                        }
                    }
                    Ok(Resumed::Fault) => {
                        pc = st.pc;
                        base = st.base;
                        acc = st.acc;
                        fail!(false);
                    }
                    Ok(Resumed::Out(o)) => {
                        let (at, ret) = (pc, $ret);
                        let r = match self.finish(base, $cont, o) {
                            Ok(n) => self.drive(st, n, at, ret, Vec::new()),
                            Err(e) => Err(e),
                        };
                        landed!(r);
                    }
                    Err(Raised) => fail!(false),
                }
            }};
        }
        let smi = |i: usize| Value::int(i as i64).unwrap_or(Value::NONE);
        match opcode {
            op::RETURN => {
                let mut stop = false;
                if st.coro != Value::NONE {
                    if Vm::is_gen(st.coro) {
                        self.gen_done(st.coro);
                        stop = true;
                    } else {
                        self.coro_finished(st.coro);
                    }
                }
                if self.depth == 0 {
                    st.pc = pc;
                    st.acc = acc;
                    return Ok(Rare::Exit(Flow::Return(acc)));
                }
                let Some(fr) = self.pop_frame(st) else {
                    fail!(false);
                };
                base = st.base;
                if fr.cont == 0 && !stop {
                    pc = fr.ret_pc as usize;
                    {
                        sync!();
                        return Ok(Rare::Frames);
                    }
                }
                let fc = self.cont_take(fr.cont);
                pc = fr.call_pc as usize;
                sync!();
                let out = if stop {
                    Outcome::Stop(acc)
                } else {
                    Outcome::Value(acc)
                };
                let (cp, rp) = (fr.call_pc as usize, fr.ret_pc as usize);
                let r = self
                    .finish(base, fc, out)
                    .and_then(|n| self.drive(st, n, cp, rp, Vec::new()));
                landed!(r);
            }
            op::DEL_GLOBAL => {
                let name = code.names[opd!(0)];
                let g = self.globals_of(&code);
                if tri!(self.dict_del(g, name)).is_none() {
                    let s = String::from(self.str_ref(name));
                    let e = self.err(T::NameError, &format!("name '{s}' is not defined"));
                    let _ = e;
                    fail!(false);
                }
                pc += ilen!();
            }
            op::DEL_DEREF => {
                let r = opd!(0);
                let cell = reg!(r);
                let Some(c) = cell.as_obj().filter(|_| cell.is_obj_kind(ty::CELL as u8)) else {
                    sync!();
                    let _ = self.not_a_cell();
                    fail!(false);
                };
                if self.slot(c, 0) == Value::UNDEFINED {
                    sync!();
                    let _ = self.unbound_local(st, r);
                    fail!(false);
                }
                self.set_slot(c, 0, Value::UNDEFINED);
                pc += ilen!();
            }
            op::DEL_LOCAL => {
                let r = opd!(0);
                if reg!(r) == Value::UNDEFINED {
                    sync!();
                    let _ = self.unbound_local(st, r);
                    fail!(false);
                }
                reg!(r) = Value::UNDEFINED;
                pc += ilen!();
            }
            op::BUILD_SLICE => {
                let r = base + opd!(0);
                let parts = [self.stack[r], self.stack[r + 1], self.stack[r + 2]];
                let s = tri!({
                    let res = self.heap.alloc_init(ty::SLICE, 0, &parts, &[]);
                    self.hx(res)
                });
                acc = Value::from_ref(s, ty::SLICE as u8);
                pc += ilen!();
            }
            op::LIST_EXTEND | op::SET_UPDATE => {
                let target = reg!(opd!(0));
                let in_call = opcode == op::LIST_EXTEND && opd!(1) == 1;
                self.consumed.clear();
                let collected = if opcode == op::SET_UPDATE {
                    self.set_update(target, acc)
                        .map(|()| NativeValues::default())
                } else {
                    self.extend_items(target, acc)
                };
                if collected.is_ok() && self.request.is_some() {
                    // A long `{*x}` or `[*xs]` goes on in steps
                    // (`nstep::op::BULK`, `SEQ_BUILD`).
                    self.consumed.clear();
                    landed!(@exit self.op_slow(st, false, words!(0), Cont::None, "", &[], true));
                }
                if collected.is_err() && self.slow {
                    let twin = if opcode == op::SET_UPDATE {
                        "_op_set_update"
                    } else {
                        "_op_list_extend"
                    };
                    let r = collected.map(|_| ());
                    let src = tri!(self.unconsume(acc));
                    self.consumed.clear();
                    op_unit!(r, 0, (twin, &[target, src, Value::from_bool(in_call)]));
                    {
                        sync!();
                        return Ok(Rare::Next);
                    }
                }
                let Ok(items) = collected else {
                    // `{*x}` keeps the plain "not iterable" message.
                    if opcode == op::LIST_EXTEND && self.exc_type(self.exc) == T::TypeError {
                        let what = if in_call {
                            let f = reg!(opd!(0).saturating_sub(1));
                            format!("{} argument", self.callee_name(f))
                        } else {
                            String::from("Value")
                        };
                        let msg = format!(
                            "{what} after * must be an iterable, not {}",
                            self.type_name(acc)
                        );
                        let _ = self.err(T::TypeError, &msg);
                    }
                    fail!(false);
                };
                tri!(self.list_extend_values(target, &items));
                pc += ilen!();
            }
            op::SET_ADD => {
                let s = reg!(opd!(0));
                op_unit!(self.set_add(s, acc), 2, ("_op_set_add", &[s, acc]));
            }
            op::DICT_UPDATE => {
                let dr = base + opd!(0);
                let mode = u8::try_from(opd!(1)).unwrap_or(u8::MAX);
                let key = reg!(opd!(2));
                let d = self.stack[dr];
                if mode != 2 && !acc.is_obj_kind(ty::DICT as u8) {
                    // Any other mapping (`keys()` and `__getitem__`) runs
                    // through the twin, as in CPython.
                    let f = if mode == 1 {
                        self.callee_name(self.stack[dr.saturating_sub(2)])
                    } else {
                        String::new()
                    };
                    let f = tri!(self.new_str(&f));
                    let r = Err(self.need_py());
                    op_unit!(
                        r,
                        3,
                        ("_op_dict_update", &[d, acc, Value::from_bool(mode == 1), f])
                    );
                    {
                        sync!();
                        return Ok(Rare::Next);
                    }
                }
                sync!();
                let r = self.dict_update_op(st, d, mode, key, acc, dr.saturating_sub(2));
                if r.is_ok() && self.request.is_some() {
                    // A long `{**m}` goes on in steps (`nstep::op::BULK`).
                    landed!(@exit self.op_slow(st, false, words!(3), Cont::None, "", &[], true));
                }
                tri!(r);
                pc += ilen!();
            }
            op::LIST_TO_TUPLE => {
                acc = tri!(self.list_to_tuple(acc));
                if self.request.is_some() {
                    // A long list goes on in steps (`nstep::op::SEQ_BUILD`).
                    landed!(@exit self.op_slow(st, false, words!(0), Cont::None, "", &[], true));
                }
                pc += ilen!();
            }
            op::UNPACK_EX => {
                let r = base + opd!(0);
                let before = opd!(1);
                let after = opd!(2);
                if tri!(self.unpack_ex_slots(acc, r, before, after)) {
                    pc += ilen!();
                } else {
                    let items = {
                        let c = self.collect(acc);
                        if c.is_err() && self.slow {
                            let cont = Cont::Unpack {
                                r: u8::try_from(opd!(0)).unwrap_or(0),
                                n: u16::try_from(before + after + 1).unwrap_or(0),
                            };
                            op_result!(
                                c.map(|_| Value::NONE),
                                4,
                                cont,
                                ("_op_unpack_ex", &[acc, smi(before), smi(after)])
                            );
                            {
                                sync!();
                                return Ok(Rare::Next);
                            }
                        }
                        if let Ok(i) = c {
                            i
                        } else {
                            if self.exc_type(self.exc) == T::TypeError {
                                let msg = format!(
                                    "cannot unpack non-iterable {} object",
                                    self.type_name(acc)
                                );
                                let _ = self.err(T::TypeError, &msg);
                            }
                            fail!(false);
                        }
                    };
                    if items.len() < before + after {
                        let msg = format!(
                            "not enough values to unpack (expected at least {}, got {})",
                            before + after,
                            items.len()
                        );
                        let _ = self.err(T::ValueError, &msg);
                        fail!(false);
                    }
                    let mid = tri!(self.new_list(&items[before..items.len() - after]));
                    self.stack[r..r + before].copy_from_slice(&items[..before]);
                    self.stack[r + before] = mid;
                    self.stack[r + before + 1..r + before + 1 + after]
                        .copy_from_slice(&items[items.len() - after..]);
                    pc += ilen!();
                }
            }
            op::FORMAT_SPEC => {
                let v = reg!(opd!(0));
                let conv = u8::try_from(opd!(1)).unwrap_or(u8::MAX);
                let spec_v = acc;
                let r = self
                    .convert(v, conv)
                    .and_then(|v| self.format_str(v, FormatSpec::Value(spec_v)));
                op_result!(
                    r,
                    3,
                    Cont::None,
                    ("_op_fmt", &[v, smi(conv as usize), spec_v])
                );
            }
            op::MATCH => {
                let k = u8::try_from(opd!(0)).unwrap_or(u8::MAX);
                let r = opd!(1);
                let n = opd!(2);
                let a = if k >= 3 { reg!(r) } else { Value::NONE };
                let b = if k == 5 { reg!(r + 1) } else { Value::NONE };
                op_result!(
                    self.match_step(k, acc, a, b, n),
                    5,
                    Cont::None,
                    ("_op_match", &[smi(k as usize), acc, a, b, smi(n)])
                );
            }
            op::DEL_ATTR => {
                let name = code.names[opd!(0)];
                op_unit!(self.delattr(acc, name), 3, ("_unreachable", &[]));
            }
            op::DEL_ITEM => {
                let o = reg!(opd!(0));
                op_unit!(self.op_delitem(o, acc), 2, ("_op_delitem", &[o, acc]));
            }
            op::CALL_EX if self.is_hypot_star(base + opd!(0)) => {
                let t = reg!(opd!(0) + 1);
                op_result!(self.hypot_of(t), 1, Cont::None, ("_unreachable", &[]));
            }
            op::CALL_EX => {
                let ret_pc = pc + ilen!();
                sync!();
                let res = self.call_ex(st, base + opd!(0), pc, ret_pc);
                match tri!(res) {
                    Some(v) if v != Value::UNDEFINED => {
                        acc = v;
                        pc = ret_pc;
                        tick!();
                    }
                    Some(_) => landed!(self.land(st, pc, ret_pc, Cont::None)),
                    None => {
                        pc = 0;
                        base = st.base;
                        tick!();
                        {
                            sync!();
                            return Ok(Rare::Frames);
                        }
                    }
                }
            }
            op::YIELD | op::YIELD_FROM => {
                if !Vm::is_gen(st.coro) {
                    let _ = self.err(T::SystemError, "micro-monty: yield outside a generator");
                    fail!(false);
                }
                sync!();
                if st.coro.is_obj_kind(ty::GEN as u8) {
                    self.gen_yield_fast(st, pc);
                } else {
                    self.gen_yield(st, pc);
                }
                let Some(fr) = self.pop_frame(st) else {
                    fail!(false);
                };
                base = st.base;
                pc = fr.call_pc as usize;
                sync!();
                let (cp, rp) = (fr.call_pc as usize, fr.ret_pc as usize);
                let fc = self.cont_take(fr.cont);
                // A value yielded to `for`, `next()` or `send()` lands
                // directly, as `finish` + `drive` would land it.
                if matches!(
                    fc,
                    Cont::ForIter(_) | Cont::Send(_) | Cont::Next(_) | Cont::GenSend | Cont::None
                ) {
                    pc = rp;
                    tick!();
                    sync!();
                    return Ok(Rare::Frames);
                }
                let r = self
                    .finish(base, fc, Outcome::Value(acc))
                    .and_then(|n| self.drive(st, n, cp, rp, Vec::new()));
                landed!(r);
            }
            op::SEND => {
                let it = reg!(opd!(0));
                let exit = u32::try_from(opd!(1)).unwrap_or(0);
                let cont = Cont::Send(exit);
                sync!();
                if it.is_obj_kind(ty::GEN as u8) {
                    resumed!(
                        self.gen_resume(st, it, acc, cont, pc, pc + ilen!()),
                        pc + ilen!(),
                        cont
                    );
                }
                if Vm::is_native_iter(it) && acc == Value::NONE {
                    if let Some(v) = tri!(self.iter_next(it)) {
                        acc = v;
                        pc += ilen!();
                    } else {
                        acc = Value::NONE;
                        pc = exit as usize;
                    }
                    {
                        sync!();
                        return Ok(Rare::Next);
                    }
                }
                let r = if acc == Value::NONE {
                    if it.is_obj_kind(ty::CALLITER as u8) {
                        self.calliter_next(it, exit)
                    } else {
                        self.op_user_next(it, cont)
                    }
                } else {
                    let k = tri!(self.intern("send"));
                    self.getattr(it, k).and_then(|m| {
                        if m == Value::UNDEFINED {
                            self.request = None;
                            return Err(self.err(T::TypeError, "micro-monty: unsupported send"));
                        }
                        self.next_request(Next::Invoke(m, Args::of(&[acc]), Value::NONE, cont))
                    })
                };
                if let Err(Raised) = r {
                    fail!(false);
                }
                if self.request.is_none() {
                    acc = Value::NONE;
                    pc = exit as usize;
                    {
                        sync!();
                        return Ok(Rare::Next);
                    }
                }
                landed!(self.land(st, pc, pc + ilen!(), Cont::None));
            }
            op::GET_YIELD_FROM_ITER => {
                if acc.is_obj_kind(ty::GEN as u8) {
                    pc += ilen!();
                } else if acc.is_obj_kind(ty::CORO as u8) {
                    // Only a `types.coroutine` generator delegates to one.
                    if !(st.coro.is_obj_kind(ty::GEN as u8) && self.iterable_coroutine(st.coro)) {
                        let _ = self.err(
                            T::TypeError,
                            "cannot 'yield from' a coroutine object in a non-coroutine generator",
                        );
                        fail!(false);
                    }
                    let c = acc;
                    op_result!(
                        self.prelude_request("_coro_wrapper", alloc::vec![c], Cont::None),
                        1,
                        Cont::None,
                        ("", &[])
                    );
                } else {
                    op_result!(self.op_get_iter(acc), 1, Cont::None, ("_op_iter", &[acc]));
                }
            }
            op::LDA_NAME => {
                let name = code.names[opd!(0)];
                let ns = reg!(0);
                if let Some(v) = self.dict_get_str(ns, name) {
                    acc = v;
                } else {
                    sync!();
                    let slot = usize::MAX;
                    acc = tri!(self.load_name_slow(st, name, slot));
                }
                pc += ilen!();
            }
            op::STA_NAME => {
                let name = code.names[opd!(0)];
                let ns = reg!(0);
                tri!(self.dict_set(ns, name, acc));
                pc += ilen!();
            }
            op::DEL_NAME => {
                let name = code.names[opd!(0)];
                let ns = reg!(0);
                if tri!(self.dict_del(ns, name)).is_none() {
                    let s = String::from(self.str_ref(name));
                    let _ = self.err(T::NameError, &format!("name '{s}' is not defined"));
                    fail!(false);
                }
                pc += ilen!();
            }
            op::BUILD_CLASS => {
                let r = base + opd!(0);
                let (name, bases, ns, kw) = (
                    self.stack[r],
                    self.stack[r + 1],
                    self.stack[r + 2],
                    self.stack[r + 3],
                );
                op_result!(
                    self.build_class(name, bases, ns, kw, acc, Value::NONE),
                    2,
                    Cont::None,
                    ("_unreachable", &[])
                );
            }
            op::WITH_ENTER => {
                let r = opd!(0);
                if let Ok((exit, res)) = self.op_with_enter(acc, opd!(1) == 1) {
                    reg!(r) = exit;
                    op_result!(
                        Ok::<Value, Raised>(res),
                        2,
                        Cont::None,
                        ("_unreachable", &[])
                    );
                } else {
                    fail!(false)
                }
            }
            op::WITH_EXIT => {
                let exit = reg!(opd!(0));
                let exc = (opd!(1) & 1 == 1).then_some(acc);
                op_result!(
                    self.op_with_exit(exit, exc, opd!(1) >= 2),
                    3,
                    Cont::None,
                    ("_unreachable", &[])
                );
            }
            op::RAISE => {
                let kind = opd!(0);
                match kind {
                    0 => {
                        if self.handled == Value::NONE {
                            let _ = self.err(T::RuntimeError, "No active exception to reraise");
                            fail!(false);
                        }
                        self.exc = self.handled;
                        fail!(true);
                    }
                    1 => {
                        if Vm::is_class(acc) && self.class_base_t(acc).is_exception() {
                            // `raise C`: instantiate C (its `__init__` runs).
                            let n = Next::Invoke(acc, Args::of(&[]), Value::NONE, Cont::Raise);
                            tri!(self.next_request(n));
                            sync!();
                            landed!(self.land(st, pc, pc + ilen!(), Cont::None));
                        }
                        let e = tri!(self.raisable(acc));
                        tri!(self.note_raise(st, e));
                        self.exc = e;
                        fail!(false);
                    }
                    _ => {
                        let e = tri!(self.raisable(reg!(opd!(1))));
                        tri!(self.note_raise(st, e));
                        let cause = if acc == Value::NONE {
                            Value::NONE
                        } else if acc.is_obj_kind(ty::EXC as u8)
                            || self.type_of_typeobj(acc).is_some_and(T::is_exception)
                        {
                            tri!(self.raisable(acc))
                        } else {
                            let _ = self.err(
                                T::TypeError,
                                "exception causes must derive from BaseException",
                            );
                            fail!(false);
                        };
                        if let Some(r) = e.as_obj() {
                            self.set_slot(r, exc_slot::CAUSE, cause);
                            let fl = self.raw(r, exc_slot::FLAGS) | 1;
                            self.set_raw(r, exc_slot::FLAGS, fl);
                        }
                        self.exc = e;
                        fail!(false);
                    }
                }
            }
            op::RERAISE => {
                self.exc = acc;
                fail!(true);
            }
            op::PUSH_EXC => {
                reg!(opd!(0)) = self.handled;
                self.handled = acc;
                pc += ilen!();
            }
            op::POP_EXC => {
                self.handled = reg!(opd!(0));
                pc += ilen!();
            }
            op::CHECK_EXC => {
                let e = reg!(opd!(0));
                acc = Value::from_bool(tri!(self.exc_matches(e, acc)));
                pc += ilen!();
            }
            op::AWAIT => {
                sync!();
                match tri!(self.await_step(st, acc, pc)) {
                    Awaited::Value(v) => {
                        acc = v;
                        pc += ilen!();
                    }
                    Awaited::Frame => {
                        pc = st.pc;
                        base = st.base;
                        acc = st.acc;
                        tick!();
                        {
                            sync!();
                            return Ok(Rare::Frames);
                        }
                    }
                    Awaited::Block => return Ok(Rare::Exit(Flow::Block(acc))),
                    Awaited::Request => landed!(self.land(st, pc, pc + ilen!(), Cont::None)),
                    Awaited::Retry(w) => acc = w,
                    Awaited::RequestRetry => landed!(self.land(st, pc, pc, Cont::None)),
                }
            }
            op::GET_AITER => {
                op_result!(self.op_get_aiter(acc), 1, Cont::None, ("_unreachable", &[]));
            }
            op::ANEXT => {
                let it = reg!(opd!(0));
                op_result!(self.op_anext(it), 1, Cont::None, ("_unreachable", &[]));
            }
            op::EXC_STAR => {
                let state = reg!(opd!(0));
                let call = if opd!(1) == 0 {
                    ("_star_match", alloc::vec![state, acc])
                } else {
                    ("_star_prep", alloc::vec![state])
                };
                op_result!(
                    self.prelude_request(call.0, call.1, Cont::None),
                    3,
                    Cont::None,
                    ("", &[])
                );
            }
            // A safepoint in straight-line code (C4): after long native
            // work `Vm::burn` left `ticks` at most 1. Charges no tick.
            op::CHECK => {
                pc += ilen!();
                if self.ticks <= 1 {
                    sync!();
                    if let Some(r) = self.safepoint(st) {
                        return Ok(Rare::Exit(Flow::Park(r)));
                    }
                }
            }
            op::END_ASYNC_FOR => {
                if !self.instance_of(acc, self.type_obj(T::StopAsyncIteration)) {
                    self.exc = acc;
                    fail!(true);
                }
                acc = Value::NONE;
                pc += ilen!();
            }
            op::IMPORT_NAME => {
                let name = code.names[opd!(0)];
                op_result!(self.import(name, opd!(1) != 0), 4, Cont::None, ("", &[]));
            }
            op::IMPORT_FROM => {
                let m = reg!(opd!(0));
                let name = code.names[opd!(1)];
                let g = self.globals_of(&code);
                op_result!(self.import_from(m, name, g), 4, Cont::None, ("", &[]));
            }
            op::ASSERT_FAIL => {
                let args: Vec<Value> = if opd!(0) == 1 {
                    alloc::vec![acc]
                } else {
                    Vec::new()
                };
                let e = tri!(self.new_exception(T::AssertionError, &args));
                self.exc = e;
                fail!(false);
            }

            // `dispatch` runs these only on a cell.
            op::LDA_DEREF | op::STA_DEREF => {
                let _ = self.not_a_cell();
                fail!(false);
            }
            _ => {
                let _ = self.err(
                    T::SystemError,
                    &format!("bad opcode {opcode} (length {})", instr_len(opcode)),
                );
                fail!(false);
            }
        }
        sync!();
        Ok(Rare::Next)
    }

    /// `CALL_EX` of `R[f]` with the positional arguments in `R[f+1]` (a
    /// tuple or list) and the keywords in `R[f+2]` (a dict or None). A
    /// function's `*args` is copied from that sequence slot to slot; other
    /// callees take the arguments laid out above this frame.
    #[inline(never)]
    fn call_ex(
        &mut self,
        st: &mut State,
        f: usize,
        call_pc: usize,
        ret_pc: usize,
    ) -> Result<Option<Value>, Raised> {
        let [callee, args, kw] = [self.stack[f], self.stack[f + 1], self.stack[f + 2]];
        let Some((sr, npos)) = self.seq_slots(args) else {
            return Err(self.err(T::SystemError, "CALL_EX arguments are not a sequence"));
        };
        let (names, vals): (Vec<Value>, Vec<Value>) = if kw.is_obj_kind(ty::DICT as u8) {
            self.dict_items_vec(kw).into_iter().unzip()
        } else {
            (Vec::new(), Vec::new())
        };
        let nregs_here = self.codes[st.ci as usize].code.nregs as usize;
        if callee.is_obj_kind(ty::FUNC as u8) {
            let cidx = u32::try_from(self.vslot_raw(callee, fn_slot::CODE)).unwrap_or(0);
            let Some(code) = self.codes.get(cidx as usize).cloned() else {
                return Err(self.err(T::SystemError, "bad code index"));
            };
            let top = st.base + nregs_here + HDR;
            let nregs = code.code.nregs as usize;
            self.ensure_stack(top + nregs + 1)?;
            let argc = code.code.params.argcount as usize;
            let (pos, rest) = if code.code.params.varargs && npos > argc {
                let rest = self.tuple_from_slots(sr, argc, npos - argc)?;
                (self.values_of(sr, argc), Some(rest))
            } else {
                (self.values_of(sr, npos), None)
            };
            let kws: Vec<(Value, Value)> = names.into_iter().zip(vals).collect();
            self.bind_args(callee, &code, &pos, &kws, top, rest)?;
            let total = code.code.params.total() as usize;
            for v in &mut self.stack[top + total.min(nregs)..top + nregs] {
                *v = Value::UNDEFINED;
            }
            if code.code.is_generator {
                return Ok(Some(self.new_gen(callee, cidx, top, nregs)?));
            }
            if code.code.is_coroutine {
                return Ok(Some(self.new_coro(callee, top, nregs)?));
            }
            self.enter_frame(st, callee, cidx, top, call_pc, ret_pc, Cont::None)?;
            return Ok(None);
        }
        let kwnames = if names.is_empty() {
            Value::NONE
        } else {
            self.alloc_tuple(&names)?
        };
        // Arguments go above this frame; the callee's frame starts after
        // them.
        let scratch = st.base + nregs_here;
        let n = npos + vals.len();
        self.ensure_stack(scratch + n + 64)?;
        // A copy's unit per 8 items, as the tuple a function gets.
        self.burn(npos as u64 / 8);
        Self::read_slots(&self.heap, sr, 0, &mut self.stack[scratch..scratch + npos]);
        self.stack[scratch + npos..scratch + n].copy_from_slice(&vals);
        self.call_value_at(
            st,
            callee,
            scratch,
            npos,
            kwnames,
            call_pc,
            ret_pc,
            scratch + n + 1,
            Cont::None,
        )
    }

    #[allow(clippy::too_many_arguments, reason = "the full call-site description")]
    fn call_value_at(
        &mut self,
        st: &mut State,
        callee: Value,
        args: usize,
        npos: usize,
        kwnames: Value,
        call_pc: usize,
        ret_pc: usize,
        free: usize,
        cont: Cont,
    ) -> Result<Option<Value>, Raised> {
        self.frame_base = st.base;
        let nkw = if kwnames == Value::NONE {
            0
        } else {
            self.tuple_len(kwnames)
        };
        self.ensure_stack(free + npos + nkw + 64)?;
        let native = if let Some(i) = callee.as_native() {
            Some((i, false))
        } else if let Some(t) = self.type_of_typeobj(callee)
            && !t.is_exception()
        {
            self.ctor_of(t).map(|c| (c, false))
        } else if callee.is_obj_kind(ty::BOUND as u8) {
            callee_bound(self, callee).map(|i| (i, true))
        } else {
            None
        };
        match native {
            Some((idx, false)) => {
                let v = self.call_native(
                    idx,
                    u32::try_from(args).unwrap_or(0),
                    u32::try_from(npos).unwrap_or(0),
                    kwnames,
                    u32::try_from(free).unwrap_or(0),
                )?;
                Ok(Some(v))
            }
            Some((idx, true)) => {
                let recv = self.vslot(callee, 0);
                let total = npos + nkw;
                for i in (0..total).rev() {
                    self.stack[free + 1 + i] = self.stack[args + i];
                }
                self.stack[free] = recv;
                let v = self.call_native(
                    idx,
                    u32::try_from(free).unwrap_or(0),
                    u32::try_from(npos + 1).unwrap_or(0),
                    kwnames,
                    u32::try_from(free + total + 1).unwrap_or(0),
                )?;
                Ok(Some(v))
            }
            None => self.call_value(st, callee, args, npos, kwnames, call_pc, ret_pc, None, cont),
        }
    }
}

fn callee_bound(vm: &Vm, callee: Value) -> Option<u32> {
    vm.vslot(callee, 1).as_native()
}
