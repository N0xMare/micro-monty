//! The execution core: one dispatch loop, frames in the value stack.
//!
//! Frame layout. A frame is a window of the value stack starting at its
//! header base `hb`:
//!
//! ```text
//! stack[hb + 0]   the running function (None for a module body)
//! stack[hb + 1]   link: SmallInt ret_pc:24 | caller code:16 | hb - caller hb:8
//! stack[hb + 2..] registers (parameters first)
//! ```
//!
//! A call `CALL a n` uses the caller's registers `a` (callee), `a+1` (link
//! slot) and `a+2..` (arguments); the callee's window starts at caller
//! `hb + a`, so the callee value becomes its slot 0 and the arguments its
//! first registers without a copy. There is no separate frame record.
//!
//! All execution state is plain data: `Vm::st` (pc, hb, code index, depth,
//! accumulator, tick budget), the value stack and the arena. A park is
//! "write the loop's locals to `st` and return"; `run` continues from `st`.

use std::rc::Rc;

use crate::code::{CodeEntry, Ic, op};
use crate::heap::{self, CELL_VAL, Exc, FN_CELLS, FN_CODE, Heap, exc};
use crate::value::{Value, kind};

pub const NATIVE_RANGE: u32 = 0;
pub const NATIVE_LEN: u32 = 1;

/// Default recursion limit (Python's), frames.
pub const RECURSION_LIMIT: u32 = 1000;
/// Ticks between safepoint polls when fuel is unlimited.
pub const QUANTUM: i64 = 10_000;
/// Value-stack words a VM starts with (it grows on demand).
const STACK_INIT: usize = 4096;
/// Value-stack words a VM may use at most.
const STACK_MAX: usize = 1 << 22;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct State {
    pub pc: u32,
    pub hb: u32,
    pub ci: u32,
    pub depth: u32,
    pub acc: Value,
    pub ticks: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Park {
    Fuel,
    Requested,
}

#[derive(Debug)]
pub enum Flow {
    Done(Value),
    Park(Park),
    /// An uncaught exception.
    Error(Value),
}

pub struct Vm {
    pub stack: Vec<Value>,
    pub heap: Heap,
    pub ics: Vec<Ic>,
    pub codes: Rc<[CodeEntry]>,
    pub st: State,
    /// Remaining fuel (ticks), or unlimited.
    pub fuel: Option<u64>,
    /// Ticks granted at the last reset (`st.ticks` counts down from it).
    pub period: i64,
    pub ticks_used: u64,
    pub limit: u32,
    pub park_requested: bool,
    pub gc_stress: bool,
    /// Builtin names (interned str) → values.
    pub builtins: Vec<(Value, Value)>,
    #[cfg(feature = "count")]
    pub count: u64,
    #[cfg(feature = "perturb_field")]
    pub perturb: [u64; 3],
}

/// The 256-register window at `hb`: u8 operands index it without checks.
#[inline(always)]
fn win(stack: &mut [Value], hb: usize) -> Option<&mut [Value; 256]> {
    stack.get_mut(hb..hb.wrapping_add(256))?.try_into().ok()
}

#[inline(always)]
fn both_int(a: Value, b: Value) -> bool {
    a.is_int() & b.is_int()
}

/// A cached global, or `None` on a miss.
#[inline(always)]
fn ic_load(heap: &Heap, ics: &[Ic], i: usize) -> Option<Value> {
    let ic = ics.get(i)?;
    if ic.version != heap.gver {
        return None;
    }
    if ic.slot == 0 {
        return Some(ic.value);
    }
    Some(heap.val(ic.slot))
}

impl Vm {
    pub fn new(
        codes: Vec<CodeEntry>,
        ics: Vec<Ic>,
        heap: Heap,
        builtins: Vec<(Value, Value)>,
    ) -> Vm {
        Vm {
            stack: vec![Value::UNDEF; STACK_INIT],
            heap,
            ics,
            codes: codes.into(),
            st: State {
                pc: 0,
                hb: 0,
                ci: 0,
                depth: 0,
                acc: Value::NONE,
                ticks: 0,
            },
            fuel: None,
            period: 0,
            ticks_used: 0,
            limit: RECURSION_LIMIT,
            park_requested: false,
            gc_stress: false,
            builtins,
            #[cfg(feature = "count")]
            count: 0,
            #[cfg(feature = "perturb_field")]
            perturb: [0; 3],
        }
    }

    /// Enters code `ci` as a module body (depth 1, frame at 0).
    pub fn start(&mut self, ci: u32) {
        self.stack[0] = Value::NONE;
        self.stack[1] = Value::link(0, 0, 0);
        self.st = State {
            pc: 0,
            hb: 0,
            ci,
            depth: 1,
            acc: Value::NONE,
            ticks: 0,
        };
        self.reset_ticks();
    }

    /// Grants `fuel` more ticks (None: unlimited) to a parked VM.
    pub fn refuel(&mut self, fuel: Option<u64>) {
        self.fuel = fuel;
        self.reset_ticks();
    }

    fn reset_ticks(&mut self) {
        self.period = match self.fuel {
            Some(f) => (f.min(QUANTUM as u64)) as i64,
            None => QUANTUM,
        };
        self.st.ticks = self.period;
    }

    /// Charges the ticks used since the last reset.
    fn charge(&mut self) {
        let used = (self.period - self.st.ticks.max(0)) as u64;
        self.ticks_used += used;
        if let Some(f) = &mut self.fuel {
            *f = f.saturating_sub(used);
        }
    }

    /// Top of the live value stack (the running frame's window end).
    pub fn live_top(&self) -> usize {
        let win = self
            .codes
            .get(self.st.ci as usize)
            .map_or(2, |c| c.win as usize);
        (self.st.hb as usize + win).min(self.stack.len())
    }

    /// The cold half of a safepoint: fuel, park requests, GC.
    #[cold]
    #[inline(never)]
    fn safepoint(&mut self) -> Option<Park> {
        #[cfg(feature = "perturb_field")]
        {
            self.perturb[1] += 1;
        }
        self.charge();
        self.reset_ticks();
        if self.park_requested {
            self.park_requested = false;
            return Some(Park::Requested);
        }
        if self.fuel == Some(0) {
            return Some(Park::Fuel);
        }
        if self.heap.gc_pending || self.gc_stress {
            self.collect();
        }
        None
    }

    pub fn collect(&mut self) {
        let top = self.live_top();
        let extra = [self.st.acc, self.heap.globals];
        self.heap.collect(&self.stack[..top], &extra);
    }

    #[cold]
    #[inline(never)]
    fn fault(&mut self) -> Flow {
        let e = self.heap.exc(exc::SYSTEM);
        Flow::Error(e.0)
    }

    #[cold]
    #[inline(never)]
    fn grow_stack(&mut self, need: usize) -> Result<(), Exc> {
        if need > STACK_MAX {
            return Err(self.heap.exc(exc::MEMORY));
        }
        let n = need
            .next_power_of_two()
            .max(self.stack.len() * 2)
            .min(STACK_MAX);
        self.stack.resize(n, Value::UNDEF);
        Ok(())
    }

    /// Unwinds to a handler (state in `st`, `acc` = exception), or returns
    /// the uncaught error.
    #[cold]
    #[inline(never)]
    fn unwind(&mut self, codes: &[CodeEntry], x: Value) -> Result<(), Flow> {
        let mut s = self.st;
        loop {
            let Some(c) = codes.get(s.ci as usize) else {
                return Err(self.fault());
            };
            if let Some(h) = c.exc.iter().find(|e| e.start <= s.pc && s.pc < e.end) {
                s.pc = h.handler;
                s.acc = x;
                self.st = s;
                return Ok(());
            }
            if s.depth <= 1 {
                s.depth = 0;
                self.st = s;
                return Err(Flow::Error(x));
            }
            let link = self
                .stack
                .get(s.hb as usize + 1)
                .copied()
                .unwrap_or(Value::NONE);
            s.hb = s.hb.wrapping_sub(link.link_off() as u32);
            s.ci = link.link_code() as u32;
            s.pc = (link.link_pc() as u32).wrapping_sub(1);
            s.depth -= 1;
        }
    }

    /// Runs until the module body returns, a park or an uncaught exception.
    pub fn run(&mut self) -> Flow {
        let codes = self.codes.clone();
        let s = self.st;
        let mut pc = s.pc as usize;
        let mut hb = s.hb as usize;
        let mut ci = s.ci as usize;
        let mut acc = s.acc;
        let (mut code, mut ic_base): (&[u32], usize) = match codes.get(ci) {
            Some(e) => (&e.code, e.ic_base as usize),
            None => return self.fault(),
        };
        let mut regs = match win(&mut self.stack, hb) {
            Some(r) => r,
            None => return self.fault(),
        };

        macro_rules! sync {
            () => {
                self.st.pc = pc as u32;
                self.st.hb = hb as u32;
                self.st.ci = ci as u32;
                self.st.acc = acc;
            };
        }
        macro_rules! reload {
            () => {
                let s = self.st;
                pc = s.pc as usize;
                hb = s.hb as usize;
                ci = s.ci as usize;
                acc = s.acc;
                match codes.get(ci) {
                    Some(e) => {
                        code = &e.code;
                        ic_base = e.ic_base as usize;
                    }
                    None => return self.fault(),
                }
                regs = match win(&mut self.stack, hb) {
                    Some(r) => r,
                    None => return self.fault(),
                };
            };
        }
        // Raise at the current instruction (pc not yet advanced).
        macro_rules! raise {
            ($x:expr) => {{
                let x: Exc = $x;
                sync!();
                match self.unwind(&codes, x.0) {
                    Ok(()) => {
                        reload!();
                        continue;
                    }
                    Err(f) => return f,
                }
            }};
        }
        macro_rules! tick {
            () => {
                self.st.ticks -= 1;
                if self.st.ticks <= 0 || self.heap.gc_pending {
                    sync!();
                    if let Some(p) = self.safepoint() {
                        return Flow::Park(p);
                    }
                    reload!();
                }
            };
        }
        macro_rules! int_op {
            ($l:expr, $r:expr, $f:ident, $opc:expr) => {{
                let (l, r): (Value, Value) = ($l, $r);
                if both_int(l, r) {
                    let s = l.int_val().$f(r.int_val());
                    if Value::fits_int(s) {
                        acc = Value::int(s);
                        pc += 1;
                        continue;
                    }
                }
                match arith_slow(&mut self.heap, $opc, l, r) {
                    Ok(v) => {
                        acc = v;
                        pc += 1;
                    }
                    Err(x) => raise!(x),
                }
            }};
        }

        macro_rules! cmp_val {
            ($l:expr, $cmp:tt, $w:expr) => {{
                let l: Value = $l;
                if both_int(l, acc) {
                    l.int_val() $cmp acc.int_val()
                } else {
                    match compare_slow(&mut self.heap, $w as u8, l, acc) {
                        Ok(b) => b,
                        Err(x) => raise!(x),
                    }
                }
            }};
        }

        loop {
            let w = match code.get(pc) {
                Some(&w) => w,
                None => return self.fault(),
            };
            #[cfg(feature = "count")]
            {
                self.count += 1;
            }
            let a = (w >> 8) as u8 as usize;
            macro_rules! a1 {
                () => {
                    (a as u8).wrapping_add(1) as usize
                };
            }
            match w as u8 {
                #[cfg(feature = "perturb_order")]
                op::RETURN => {
                    if self.st.depth <= 1 {
                        sync!();
                        return self.finish(acc);
                    }
                    let link = regs[1];
                    if link.link_special() {
                        // Generator / coroutine / continuation frames: the
                        // engine finishes them out of line; not in the spike.
                        sync!();
                        return self.fault();
                    }
                    let nci = link.link_code();
                    match codes.get(nci) {
                        Some(e) => {
                            code = &e.code;
                            ic_base = e.ic_base as usize;
                        }
                        None => return self.fault(),
                    }
                    hb = hb.wrapping_sub(link.link_off());
                    ci = nci;
                    pc = link.link_pc();
                    self.st.depth -= 1;
                    regs = match win(&mut self.stack, hb) {
                        Some(r) => r,
                        None => return self.fault(),
                    };
                }
                op::LDAR => {
                    acc = regs[a];
                    pc += 1;
                }
                op::STAR => {
                    regs[a] = acc;
                    pc += 1;
                }
                op::MOV => {
                    regs[(w >> 16) as u8 as usize] = regs[a];
                    pc += 1;
                }
                op::LDA_SMI => {
                    acc = Value::int(((w as i32) >> 8) as i64);
                    pc += 1;
                }
                op::LDA_CONST => {
                    match codes.get(ci).and_then(|e| e.consts.get((w >> 16) as usize)) {
                        Some(&v) => acc = v,
                        None => return self.fault(),
                    }
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
                op::LDA_LOCAL => {
                    let v = regs[a];
                    if v == Value::UNDEF {
                        raise!(self.heap.exc(exc::UNBOUND_LOCAL));
                    }
                    acc = v;
                    pc += 1;
                }
                op::LDA_GLOBAL => {
                    let i = ic_base + (w >> 8) as usize;
                    match ic_load(&self.heap, &self.ics, i) {
                        Some(v) => acc = v,
                        None => {
                            match global_load_slow(&mut self.heap, &mut self.ics, &self.builtins, i)
                            {
                                Ok(v) => acc = v,
                                Err(x) => raise!(x),
                            }
                        }
                    }
                    pc += 1;
                }
                op::LDA_GLOBAL_STAR => {
                    let i = ic_base + (w >> 16) as usize;
                    match ic_load(&self.heap, &self.ics, i) {
                        Some(v) => acc = v,
                        None => {
                            match global_load_slow(&mut self.heap, &mut self.ics, &self.builtins, i)
                            {
                                Ok(v) => acc = v,
                                Err(x) => raise!(x),
                            }
                        }
                    }
                    regs[a] = acc;
                    pc += 1;
                }
                op::STA_GLOBAL => {
                    let i = ic_base + (w >> 8) as usize;
                    match self.ics.get(i) {
                        Some(ic) if ic.version == self.heap.gver && ic.slot != 0 => {
                            self.heap.put(ic.slot, acc);
                        }
                        _ => {
                            if let Err(x) = global_store_slow(&mut self.heap, &mut self.ics, i, acc)
                            {
                                raise!(x);
                            }
                        }
                    }
                    pc += 1;
                }
                op::LDA_DEREF => {
                    let f = regs[0];
                    let c = self.heap.val(f.addr().wrapping_add(FN_CELLS + a as u32));
                    let v = self.heap.val(c.addr().wrapping_add(CELL_VAL));
                    if !(f.is_obj(kind::FUNC) & c.is_obj(kind::CELL)) || v == Value::UNDEF {
                        raise!(self.heap.exc(exc::NAME));
                    }
                    acc = v;
                    pc += 1;
                }
                op::ADD => int_op!(regs[a], acc, wrapping_add, op::ADD),
                op::INPLACE_ADD => int_op!(regs[a], acc, wrapping_add, op::ADD),
                op::SUB => int_op!(regs[a], acc, wrapping_sub, op::SUB),
                op::ADD_SMI => int_op!(
                    regs[a],
                    Value::int((w >> 16) as u16 as i16 as i64),
                    wrapping_add,
                    op::ADD
                ),
                op::SUB_SMI => int_op!(
                    regs[a],
                    Value::int((w >> 16) as u16 as i16 as i64),
                    wrapping_sub,
                    op::SUB
                ),
                op::TEST_LT => {
                    let l = regs[a];
                    let lt = if both_int(l, acc) {
                        l.int_val() < acc.int_val()
                    } else {
                        match compare_slow(&mut self.heap, w as u8, l, acc) {
                            Ok(b) => b,
                            Err(x) => raise!(x),
                        }
                    };
                    acc = Value::from_bool(lt);
                    pc += 1;
                }
                op::TEST_LT_JF => {
                    let l = regs[a];
                    let lt = if both_int(l, acc) {
                        l.int_val() < acc.int_val()
                    } else {
                        match compare_slow(&mut self.heap, w as u8, l, acc) {
                            Ok(b) => b,
                            Err(x) => raise!(x),
                        }
                    };
                    acc = Value::from_bool(lt);
                    pc = if lt { pc + 1 } else { (w >> 16) as usize };
                }
                op::JUMP => pc = (w >> 8) as usize,
                op::JUMP_IF_FALSE | op::JUMP_IF_TRUE => {
                    let t = if acc == Value::TRUE {
                        true
                    } else if acc == Value::FALSE || acc == Value::NONE {
                        false
                    } else if acc.is_int() {
                        acc.int_val() != 0
                    } else {
                        match truthy_slow(&self.heap, acc) {
                            Ok(b) => b,
                            Err(()) => raise!(self.heap.exc(exc::TYPE)),
                        }
                    };
                    pc = if t == (w as u8 == op::JUMP_IF_TRUE) {
                        (w >> 8) as usize
                    } else {
                        pc + 1
                    };
                }
                op::JUMP_LOOP => {
                    pc = (w >> 8) as usize;
                    tick!();
                }
                op::FOR_PREP => {
                    let n = (w >> 16) as u8;
                    let a2 = (a as u8).wrapping_add(2) as usize;
                    let a3 = (a as u8).wrapping_add(3) as usize;
                    let (start, stop) = if n == 1 {
                        (Value::int(0), regs[a2])
                    } else {
                        (regs[a2], regs[a3])
                    };
                    let exit = code.get(pc + 1).copied().unwrap_or(u32::MAX) as usize;
                    if regs[a] == Value::native(NATIVE_RANGE)
                        && (n == 1 || n == 2)
                        && both_int(start, stop)
                    {
                        if start.int_val() < stop.int_val() {
                            regs[a] = start;
                            regs[a1!()] = stop;
                            acc = start;
                            pc += 2;
                        } else {
                            pc = exit;
                        }
                    } else {
                        // Not a plain int range: a real iterator object.
                        let it = match regs.get(a2..a2 + n as usize) {
                            Some(args) => for_prep_slow(&mut self.heap, regs[a], args),
                            None => Err(self.heap.exc(exc::SYSTEM)),
                        };
                        let it = match it {
                            Ok(it) => it,
                            Err(x) => raise!(x),
                        };
                        regs[a] = it;
                        regs[a1!()] = Value::NONE;
                        match iter_next(&mut self.heap, it) {
                            Ok(Some(v)) => {
                                acc = v;
                                pc += 2;
                            }
                            Ok(None) => pc = exit,
                            Err(x) => raise!(x),
                        }
                    }
                }
                op::FOR_LOOP => {
                    let stop = regs[a1!()];
                    if stop.is_int() {
                        let i = regs[a].int_val() + 1;
                        if i < stop.int_val() {
                            let v = Value::int(i);
                            regs[a] = v;
                            acc = v;
                            pc = (w >> 16) as usize;
                        } else {
                            pc += 1;
                        }
                    } else {
                        match iter_next(&mut self.heap, regs[a]) {
                            Ok(Some(v)) => {
                                acc = v;
                                pc = (w >> 16) as usize;
                            }
                            Ok(None) => pc += 1,
                            Err(x) => raise!(x),
                        }
                    }
                    tick!();
                }
                op::CALL => {
                    let n = (w >> 16) as u8;
                    let f = regs[a];
                    if f.is_obj(kind::FUNC) {
                        let cidx = self.heap.val(f.addr().wrapping_add(FN_CODE)).int_val() as usize;
                        if let Some(e) = codes.get(cidx)
                            && e.argc == n
                            && self.st.depth < self.limit
                        {
                            regs[a1!()] = Value::link(pc + 1, ci, a);
                            let nhb = hb + a;
                            regs = match win(&mut self.stack, nhb) {
                                Some(r) => r,
                                None => {
                                    // Grow, then run this CALL again.
                                    sync!();
                                    if let Err(x) = self.grow_stack(nhb + 256) {
                                        raise!(x);
                                    }
                                    reload!();
                                    continue;
                                }
                            };
                            if e.nclear != 0 {
                                for k in 0..e.nclear {
                                    regs[(n.wrapping_add(2).wrapping_add(k)) as usize] =
                                        Value::UNDEF;
                                }
                            }
                            hb = nhb;
                            ci = cidx;
                            code = &e.code;
                            ic_base = e.ic_base as usize;
                            pc = 0;
                            self.st.depth += 1;
                            tick!();
                            continue;
                        }
                    }
                    let a2 = (a as u8).wrapping_add(2) as usize;
                    let r = match regs.get(a2..a2 + n as usize) {
                        Some(args) => {
                            call_slow(&mut self.heap, &codes, f, args, self.st.depth, self.limit)
                        }
                        None => Err(self.heap.exc(exc::SYSTEM)),
                    };
                    match r {
                        Ok(v) => {
                            acc = v;
                            pc += 1;
                        }
                        Err(x) => raise!(x),
                    }
                }
                #[cfg(not(feature = "perturb_order"))]
                op::RETURN => {
                    if self.st.depth <= 1 {
                        sync!();
                        return self.finish(acc);
                    }
                    let link = regs[1];
                    if link.link_special() {
                        // Generator / coroutine / continuation frames: the
                        // engine finishes them out of line; not in the spike.
                        sync!();
                        return self.fault();
                    }
                    let nci = link.link_code();
                    match codes.get(nci) {
                        Some(e) => {
                            code = &e.code;
                            ic_base = e.ic_base as usize;
                        }
                        None => return self.fault(),
                    }
                    hb = hb.wrapping_sub(link.link_off());
                    ci = nci;
                    pc = link.link_pc();
                    self.st.depth -= 1;
                    regs = match win(&mut self.stack, hb) {
                        Some(r) => r,
                        None => return self.fault(),
                    };
                }
                #[cfg(feature = "full_hot")]
                op::TEST_NE => {
                    let b = cmp_val!(regs[a], !=, w);
                    acc = Value::from_bool(b);
                    pc += 1;
                }
                #[cfg(feature = "full_hot")]
                op::TEST_EQ => {
                    let b = cmp_val!(regs[a], ==, w);
                    acc = Value::from_bool(b);
                    pc += 1;
                }
                #[cfg(feature = "full_hot")]
                op::TEST_LE => {
                    let b = cmp_val!(regs[a], <=, w);
                    acc = Value::from_bool(b);
                    pc += 1;
                }
                #[cfg(feature = "full_hot")]
                op::TEST_GT => {
                    let b = cmp_val!(regs[a], >, w);
                    acc = Value::from_bool(b);
                    pc += 1;
                }
                #[cfg(feature = "full_hot")]
                op::TEST_GE => {
                    let b = cmp_val!(regs[a], >=, w);
                    acc = Value::from_bool(b);
                    pc += 1;
                }
                #[cfg(feature = "full_hot")]
                op::TEST_LE_JF => {
                    let b = cmp_val!(regs[a], <=, w);
                    acc = Value::from_bool(b);
                    pc = if b { pc + 1 } else { (w >> 16) as usize };
                }
                #[cfg(feature = "full_hot")]
                op::TEST_GT_JF => {
                    let b = cmp_val!(regs[a], >, w);
                    acc = Value::from_bool(b);
                    pc = if b { pc + 1 } else { (w >> 16) as usize };
                }
                #[cfg(feature = "full_hot")]
                op::TEST_GE_JF => {
                    let b = cmp_val!(regs[a], >=, w);
                    acc = Value::from_bool(b);
                    pc = if b { pc + 1 } else { (w >> 16) as usize };
                }
                #[cfg(feature = "full_hot")]
                op::TEST_IS => {
                    acc = Value::from_bool(regs[a] == acc);
                    pc += 1;
                }
                #[cfg(feature = "full_hot")]
                op::MUL => {
                    let l = regs[a];
                    if both_int(l, acc)
                        && let Some(p) = l.int_val().checked_mul(acc.int_val())
                        && Value::fits_int(p)
                    {
                        acc = Value::int(p);
                        pc += 1;
                        continue;
                    }
                    match arith_slow(&mut self.heap, op::MUL, l, acc) {
                        Ok(v) => {
                            acc = v;
                            pc += 1;
                        }
                        Err(x) => raise!(x),
                    }
                }
                #[cfg(feature = "full_hot")]
                op::NEG => {
                    if acc.is_int() && acc.int_val() != -(1 << 47) {
                        acc = Value::int(-acc.int_val());
                    } else if acc.is_float() {
                        acc = Value::float(-acc.float_val());
                    } else {
                        raise!(self.heap.exc(exc::TYPE));
                    }
                    pc += 1;
                }
                #[cfg(feature = "full_hot")]
                op::NOT => {
                    let t = if acc == Value::TRUE {
                        true
                    } else if acc == Value::FALSE || acc == Value::NONE {
                        false
                    } else if acc.is_int() {
                        acc.int_val() != 0
                    } else {
                        match truthy_slow(&self.heap, acc) {
                            Ok(b) => b,
                            Err(()) => raise!(self.heap.exc(exc::TYPE)),
                        }
                    };
                    acc = Value::from_bool(!t);
                    pc += 1;
                }
                #[cfg(feature = "full_hot")]
                op::GET_ATTR => {
                    let o = acc;
                    let i = ic_base + (w >> 16) as usize;
                    if o.is_obj(kind::INST)
                        && let Some(ic) = self.ics.get(i)
                        && ic.version == self.heap.val(o.addr() + 1).0
                    {
                        acc = self.heap.val(o.addr().wrapping_add(2 + ic.slot));
                        pc += 1;
                    } else {
                        raise!(attr_slow(&mut self.heap));
                    }
                }
                #[cfg(feature = "full_hot")]
                op::SET_ATTR => {
                    let o = regs[a];
                    let i = ic_base + (w >> 16) as usize;
                    if o.is_obj(kind::INST)
                        && let Some(ic) = self.ics.get(i)
                        && ic.version == self.heap.val(o.addr() + 1).0
                    {
                        let at = o.addr().wrapping_add(2 + ic.slot);
                        self.heap.put(at, acc);
                        pc += 1;
                    } else {
                        raise!(attr_slow(&mut self.heap));
                    }
                }
                #[cfg(feature = "full_hot")]
                op::LOAD_METHOD => {
                    let o = acc;
                    let i = ic_base + (w >> 16) as usize;
                    if o.is_obj(kind::INST)
                        && let Some(ic) = self.ics.get(i)
                        && ic.version == self.heap.val(o.addr() + 1).0
                    {
                        regs[a] = ic.value;
                        regs[(a as u8).wrapping_add(2) as usize] = o;
                        pc += 1;
                    } else {
                        raise!(attr_slow(&mut self.heap));
                    }
                }
                #[cfg(feature = "full_hot")]
                op::LIST_APPEND => match list_append(&mut self.heap, regs[a], acc) {
                    Ok(()) => pc += 1,
                    Err(x) => raise!(x),
                },
                #[cfg(feature = "full_hot")]
                op::STA_DEREF => {
                    let f = regs[0];
                    let c = self.heap.val(f.addr().wrapping_add(FN_CELLS + a as u32));
                    if !(f.is_obj(kind::FUNC) & c.is_obj(kind::CELL)) {
                        raise!(self.heap.exc(exc::SYSTEM));
                    }
                    self.heap.put(c.addr() + CELL_VAL, acc);
                    pc += 1;
                }
                op::GET_ITEM => {
                    let o = regs[a];
                    #[cfg(feature = "full_hot")]
                    if o.is_obj(kind::LIST) && acc.is_int() {
                        // list[int]: inline, as the engine's fast path.
                        let len = self.heap.val(o.addr() + 1).int_val();
                        let mut i = acc.int_val();
                        if i < 0 {
                            i += len;
                        }
                        if 0 <= i && i < len {
                            let buf = self.heap.val(o.addr() + 2).addr();
                            acc = self.heap.val(buf.wrapping_add(1 + i as u32));
                            pc += 1;
                            continue;
                        }
                    }
                    match getitem(&mut self.heap, o, acc) {
                        Ok(v) => {
                            acc = v;
                            pc += 1;
                        }
                        Err(x) => raise!(x),
                    }
                }
                op::SET_ITEM => {
                    let k = regs[(w >> 16) as u8 as usize];
                    match setitem(&mut self.heap, regs[a], k, acc) {
                        Ok(()) => pc += 1,
                        Err(x) => raise!(x),
                    }
                }
                #[cfg(feature = "perturb_arm")]
                op::NOP => {
                    // An extra hot arm the kernels never execute.
                    acc = Value::int(acc.int_val().wrapping_mul(3) ^ regs[a].int_val());
                    pc += 1;
                }
                _ => {
                    sync!();
                    match self.cold_op(w) {
                        Ok(()) => {
                            reload!();
                        }
                        Err(x) => raise!(x),
                    }
                }
            }
        }
    }

    /// The module body returned.
    fn finish(&mut self, acc: Value) -> Flow {
        self.charge();
        self.st.ticks = 0;
        self.period = 0;
        self.st.depth = 0;
        Flow::Done(acc)
    }

    /// Executes one rare instruction at `st.pc`; on success `st` says where
    /// to continue. Not a dispatch loop: one instruction, then back.
    #[cold]
    #[inline(never)]
    fn cold_op(&mut self, w: u32) -> Result<(), Exc> {
        let a = (w >> 8) as u8 as usize;
        let hb = self.st.hb as usize;
        let r = |vm: &Vm, i: usize| vm.stack.get(hb + i).copied().unwrap_or(Value::UNDEF);
        let acc = self.st.acc;
        let next = self.st.pc + 1;
        let out = match w as u8 {
            op::NOP => acc,
            op::MUL => {
                let (l, x) = (r(self, a), acc);
                match both_int(l, x)
                    .then(|| l.int_val().checked_mul(x.int_val()))
                    .flatten()
                {
                    Some(p) if Value::fits_int(p) => Value::int(p),
                    _ => arith_slow(&mut self.heap, op::MUL, l, x)?,
                }
            }
            op::TEST_EQ => Value::from_bool(r(self, a) == acc),
            op::MAKE_CELL => {
                let c = self.heap.cell_new(r(self, a))?;
                if let Some(s) = self.stack.get_mut(hb + a) {
                    *s = c;
                }
                acc
            }
            op::MAKE_FUNCTION => {
                let cidx = (w >> 16) as usize;
                let ncells = self.codes.get(cidx).map_or(0, |c| c.ncells as usize);
                let cells: Vec<Value> = (0..ncells).map(|k| r(self, a + k)).collect();
                self.heap.func_new(cidx as u32, &cells)?
            }
            op::BUILD_DICT => self.heap.dict_new()?,
            op::STA_DEREF => {
                let f = r(self, 0);
                let c = self.heap.val(f.addr().wrapping_add(FN_CELLS + a as u32));
                if !c.is_obj(kind::CELL) {
                    return Err(self.heap.exc(exc::SYSTEM));
                }
                self.heap.put(c.addr() + CELL_VAL, acc);
                acc
            }
            op::RAISE => {
                if acc.is_obj(kind::EXC) && a == 0 {
                    return Err(Exc(acc));
                }
                return Err(self.heap.exc(a as i64));
            }
            op::EXC_MATCH => Value::from_bool(self.heap.exc_kind(acc) == a as i64),
            #[cfg(feature = "perturb_cold")]
            60..=69 => {
                // More rare opcodes, as a full engine has.
                let x = r(self, a);
                let k = (w as u8 - 60) as i64;
                if x.is_int() {
                    Value::int(x.int_val().rotate_left(k as u32) & 0xFFFF)
                } else {
                    acc
                }
            }
            _ => return Err(self.heap.exc(exc::SYSTEM)),
        };
        self.st.acc = out;
        self.st.pc = next;
        Ok(())
    }
}

// ------------------------------------------------------------ slow paths
//
// All out of line; they take the heap (and side tables), never the value
// stack, so the loop keeps its register window across them.

#[cold]
#[inline(never)]
fn arith_slow(heap: &mut Heap, opc: u8, l: Value, r: Value) -> Result<Value, Exc> {
    let f = |v: Value| -> Option<f64> {
        if v.is_float() {
            Some(v.float_val())
        } else if v.is_int() {
            Some(v.int_val() as f64)
        } else {
            None
        }
    };
    if both_int(l, r) {
        // i48 overflow: the engine promotes to a heap bigint here.
        return Err(heap.exc(exc::OVERFLOW));
    }
    match (f(l), f(r)) {
        (Some(x), Some(y)) => Ok(Value::float(match opc {
            op::ADD => x + y,
            op::SUB => x - y,
            #[cfg(feature = "perturb_slow")]
            op::MUL if y == 0.0 => return Err(heap.exc(exc::VALUE)),
            _ => x * y,
        })),
        _ => Err(heap.exc(exc::TYPE)),
    }
}

#[cold]
#[inline(never)]
fn compare_slow(heap: &mut Heap, opc: u8, l: Value, r: Value) -> Result<bool, Exc> {
    let f = |v: Value| {
        if v.is_float() {
            Some(v.float_val())
        } else if v.is_int() {
            Some(v.int_val() as f64)
        } else {
            None
        }
    };
    match (f(l), f(r)) {
        (Some(x), Some(y)) => Ok(match opc {
            op::TEST_LE | op::TEST_LE_JF => x <= y,
            op::TEST_GT | op::TEST_GT_JF => x > y,
            op::TEST_GE | op::TEST_GE_JF => x >= y,
            op::TEST_EQ => x == y,
            op::TEST_NE => x != y,
            _ => x < y,
        }),
        _ if opc == op::TEST_EQ => Ok(l == r),
        _ if opc == op::TEST_NE => Ok(l != r),
        _ => Err(heap.exc(exc::TYPE)),
    }
}

/// Attribute or method IC miss (the engine looks up the MRO and fills the
/// entry; the spike's kernels have no instances).
#[cold]
#[inline(never)]
fn attr_slow(heap: &mut Heap) -> Exc {
    heap.exc(exc::UNSUPPORTED)
}

#[inline(never)]
fn list_append(heap: &mut Heap, l: Value, _v: Value) -> Result<(), Exc> {
    if !l.is_obj(kind::LIST) {
        return Err(heap.exc(exc::TYPE));
    }
    Err(heap.exc(exc::UNSUPPORTED))
}

#[cold]
#[inline(never)]
fn truthy_slow(heap: &Heap, v: Value) -> Result<bool, ()> {
    if v.is_float() {
        return Ok(v.float_val() != 0.0);
    }
    if v.is_obj(kind::DICT) {
        return Ok(heap.val(v.addr() + heap::D_USED).int_val() != 0);
    }
    if v.is_ref() || v.is_native() {
        return Ok(true);
    }
    Err(())
}

/// IC miss: look the name up in the globals dict, then the builtins, and
/// fill the entry.
#[cold]
#[inline(never)]
fn global_load_slow(
    heap: &mut Heap,
    ics: &mut [Ic],
    builtins: &[(Value, Value)],
    i: usize,
) -> Result<Value, Exc> {
    let Some(&ic) = ics.get(i) else {
        return Err(heap.exc(exc::SYSTEM));
    };
    let g = heap.globals;
    if let Ok(Some((ent, idx))) = heap.dict_lookup(g, ic.name) {
        ics[i] = Ic {
            version: heap.gver,
            slot: ent + 3 * idx + 3,
            value: Value::NONE,
            name: ic.name,
        };
        return Ok(heap.val(ent + 3 * idx + 3));
    }
    if let Some(&(_, v)) = builtins.iter().find(|(n, _)| *n == ic.name) {
        ics[i] = Ic {
            version: heap.gver,
            slot: 0,
            value: v,
            name: ic.name,
        };
        return Ok(v);
    }
    Err(heap.exc(exc::NAME))
}

#[cold]
#[inline(never)]
fn global_store_slow(heap: &mut Heap, ics: &mut [Ic], i: usize, v: Value) -> Result<(), Exc> {
    let Some(&ic) = ics.get(i) else {
        return Err(heap.exc(exc::SYSTEM));
    };
    let g = heap.globals;
    if heap.dict_set(g, ic.name, v)? {
        heap.gver += 1;
    }
    if let Ok(Some((ent, idx))) = heap.dict_lookup(g, ic.name) {
        ics[i] = Ic {
            version: heap.gver,
            slot: ent + 3 * idx + 3,
            value: Value::NONE,
            name: ic.name,
        };
    }
    Ok(())
}

/// A call the fast path does not take: natives, arity errors, recursion.
#[cold]
#[inline(never)]
fn call_slow(
    heap: &mut Heap,
    codes: &[CodeEntry],
    f: Value,
    args: &[Value],
    depth: u32,
    limit: u32,
) -> Result<Value, Exc> {
    if f.is_native() {
        return match (f.native_idx(), args) {
            (NATIVE_LEN, [d]) if d.is_obj(kind::DICT) => Ok(heap.val(d.addr() + heap::D_USED)),
            (NATIVE_RANGE, _) => for_prep_slow(heap, f, args),
            _ => Err(heap.exc(exc::TYPE)),
        };
    }
    if f.is_obj(kind::FUNC) {
        if depth >= limit {
            return Err(heap.exc(exc::RECURSION));
        }
        let cidx = heap.val(f.addr() + FN_CODE).int_val() as usize;
        return match codes.get(cidx) {
            // Defaults, keywords, *args: the engine binds them here and
            // then enters the frame; the spike leaves them out.
            Some(c) if c.argc == 0xFF => Err(heap.exc(exc::UNSUPPORTED)),
            _ => Err(heap.exc(exc::TYPE)),
        };
    }
    Err(heap.exc(exc::TYPE))
}

/// `iter(callee(*args))` when `FOR_PREP` cannot keep counters in registers.
#[cold]
#[inline(never)]
fn for_prep_slow(heap: &mut Heap, f: Value, args: &[Value]) -> Result<Value, Exc> {
    if f != Value::native(NATIVE_RANGE) {
        // A user callable or a rebound `range`: the engine calls it with a
        // continuation that runs `GET_ITER`; the spike leaves it out.
        return Err(heap.exc(exc::UNSUPPORTED));
    }
    let (start, stop, step) = match args {
        [s] => (Value::int(0), *s, Value::int(1)),
        [a, b] => (*a, *b, Value::int(1)),
        [a, b, c] => (*a, *b, *c),
        _ => return Err(heap.exc(exc::TYPE)),
    };
    if !(start.is_int() && stop.is_int() && step.is_int()) {
        return Err(heap.exc(exc::TYPE));
    }
    if step.int_val() == 0 {
        return Err(heap.exc(exc::VALUE));
    }
    let a = heap.alloc(kind::RANGE, 4)?;
    heap.put(a + heap::RI_CUR, start);
    heap.put(a + heap::RI_STOP, stop);
    heap.put(a + heap::RI_STEP, step);
    Ok(Value::obj(kind::RANGE, a))
}

#[inline(never)]
fn iter_next(heap: &mut Heap, it: Value) -> Result<Option<Value>, Exc> {
    if !it.is_obj(kind::RANGE) {
        return Err(heap.exc(exc::TYPE));
    }
    let a = it.addr();
    let (cur, stop, step) = (
        heap.val(a + heap::RI_CUR).int_val(),
        heap.val(a + heap::RI_STOP).int_val(),
        heap.val(a + heap::RI_STEP).int_val(),
    );
    if (step > 0 && cur < stop) || (step < 0 && cur > stop) {
        heap.put(a + heap::RI_CUR, Value::int(cur + step));
        Ok(Some(Value::int(cur)))
    } else {
        Ok(None)
    }
}

#[inline(never)]
fn getitem(heap: &mut Heap, obj: Value, key: Value) -> Result<Value, Exc> {
    if !obj.is_obj(kind::DICT) {
        return Err(heap.exc(exc::TYPE));
    }
    match heap.dict_lookup(obj, key) {
        Ok(Some((ent, idx))) => Ok(heap.val(ent + 3 * idx + 3)),
        Ok(None) => Err(heap.exc(exc::KEY)),
        Err(()) => Err(heap.exc(exc::TYPE)),
    }
}

#[inline(never)]
fn setitem(heap: &mut Heap, obj: Value, key: Value, v: Value) -> Result<(), Exc> {
    if !obj.is_obj(kind::DICT) {
        return Err(heap.exc(exc::TYPE));
    }
    heap.dict_set(obj, key, v).map(|_| ())
}
