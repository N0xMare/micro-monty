//! Design 3: three-address register machine (Lua 5 style). 1-byte opcodes,
//! u8 register/IC operands, u16 jump targets (variable length, not Lua's fixed
//! 32-bit words, so that all three designs share one encoding scheme).
//! `CALL a n`: callee in R[a], arguments in R[a+1..=a+n], the callee frame
//! overlays the arguments (no copy, as Lua) and the result lands in R[a].
//! `SELF a b ic`: R[a+1] = R[b]; R[a] = method (Lua's OP_SELF).

use crate::access::{ld, rd8, rd16, sl, st};
use crate::heap::names::{APPEND, FIB, INC, K, MAKE, RANGE, X, Y};
use crate::kernels::Kernel;
use crate::rt::{self, Callee};
use crate::value::Value;
use crate::vm::{Asm, Flow, ProgBuilder, Program, Vm, pop_frame, push_frame};

pub mod op {
    pub const MOVE: u8 = 1;
    pub const LOADI: u8 = 2;
    pub const LOADK: u8 = 3;
    pub const GETGLOBAL: u8 = 4;
    pub const ADD: u8 = 5;
    pub const SUB: u8 = 6;
    pub const LT_JMPF: u8 = 7;
    pub const JMPBACK: u8 = 8;
    pub const CALL: u8 = 9;
    pub const RETURN: u8 = 10;
    pub const GETATTR: u8 = 11;
    pub const SETATTR: u8 = 12;
    pub const SELF: u8 = 13;
    pub const GETITEM: u8 = 14;
    pub const SETITEM: u8 = 15;
    pub const NEWLIST: u8 = 16;
    pub const NEWDICT: u8 = 17;
    pub const GETUPVAL: u8 = 18;
    pub const MAKECELL: u8 = 19;
    pub const CLOSURE: u8 = 20;
    pub const FORRANGE: u8 = 21;
}
use op::*;

static LENS: [u8; 256] = {
    let mut l = [0u8; 256];
    l[MOVE as usize] = 3;
    l[LOADI as usize] = 3;
    l[LOADK as usize] = 3;
    l[GETGLOBAL as usize] = 3;
    l[ADD as usize] = 4;
    l[SUB as usize] = 4;
    l[LT_JMPF as usize] = 5;
    l[JMPBACK as usize] = 3;
    l[CALL as usize] = 3;
    l[RETURN as usize] = 2;
    l[GETATTR as usize] = 4;
    l[SETATTR as usize] = 4;
    l[SELF as usize] = 4;
    l[GETITEM as usize] = 4;
    l[SETITEM as usize] = 4;
    l[NEWLIST as usize] = 2;
    l[NEWDICT as usize] = 2;
    l[GETUPVAL as usize] = 3;
    l[MAKECELL as usize] = 2;
    l[CLOSURE as usize] = 4;
    l[FORRANGE as usize] = 5;
    l
};

// ------------------------------------------------------------ operand access

#[inline(always)]
fn arg(vm: &Vm<'_>, k: usize) -> usize {
    rd8(vm.code, vm.pc + k) as usize
}
/// Register named by operand `k`.
#[inline(always)]
fn reg(vm: &Vm<'_>, k: usize) -> Value {
    ld(vm.stack, vm.base + arg(vm, k))
}
#[inline(always)]
fn set_reg(vm: &mut Vm<'_>, k: usize, v: Value) {
    let i = vm.base + arg(vm, k);
    st(vm.stack, i, v);
}

// ------------------------------------------------------------------ handlers

#[inline(always)]
fn mov(vm: &mut Vm<'_>) -> Flow {
    let v = reg(vm, 2);
    set_reg(vm, 1, v);
    vm.pc += 3;
    Flow::Continue
}
#[inline(always)]
fn loadi(vm: &mut Vm<'_>) -> Flow {
    let v = Value::int(rd8(vm.code, vm.pc + 2) as i8 as i64);
    set_reg(vm, 1, v);
    vm.pc += 3;
    Flow::Continue
}
#[inline(always)]
fn loadk(vm: &mut Vm<'_>) -> Flow {
    let v = vm.prog.consts[arg(vm, 2)];
    set_reg(vm, 1, v);
    vm.pc += 3;
    Flow::Continue
}
#[inline(always)]
fn getglobal(vm: &mut Vm<'_>) -> Flow {
    let v = tri!(rt::load_global(vm.heap, arg(vm, 2)));
    set_reg(vm, 1, v);
    vm.pc += 3;
    Flow::Continue
}
#[inline(always)]
fn add(vm: &mut Vm<'_>) -> Flow {
    let v = tri!(rt::add(reg(vm, 2), reg(vm, 3)));
    set_reg(vm, 1, v);
    vm.pc += 4;
    Flow::Continue
}
#[inline(always)]
fn sub(vm: &mut Vm<'_>) -> Flow {
    let v = tri!(rt::sub(reg(vm, 2), reg(vm, 3)));
    set_reg(vm, 1, v);
    vm.pc += 4;
    Flow::Continue
}
/// `LT_JMPF a b target`: jump unless R[a] < R[b].
#[inline(always)]
fn lt_jmpf(vm: &mut Vm<'_>) -> Flow {
    if tri!(rt::lt_bool(reg(vm, 1), reg(vm, 2))) {
        vm.pc += 5;
    } else {
        vm.pc = rd16(vm.code, vm.pc + 3);
    }
    Flow::Continue
}
#[inline(always)]
fn jmpback(vm: &mut Vm<'_>) -> Flow {
    rt::tick(vm.heap, vm.pc);
    vm.pc = rd16(vm.code, vm.pc + 1);
    Flow::Continue
}
/// `CALL a n`.
#[inline(always)]
fn call(vm: &mut Vm<'_>) -> Flow {
    let fpos = vm.base + arg(vm, 1);
    let n = arg(vm, 2);
    let f = ld(vm.stack, fpos);
    let ret_pc = vm.pc + 3;
    match rt::callee(vm.heap, f) {
        Callee::Py(ci) => {
            tri!(push_frame(vm, f, ci, fpos + 1, n, ret_pc));
        }
        Callee::Native(i) => {
            let r = tri!(rt::call_native(vm.heap, i, sl(vm.stack, fpos + 1, n)));
            st(vm.stack, fpos, r);
            vm.pc = ret_pc;
        }
        Callee::Bad => return Flow::Err(crate::heap::Fail::Type),
    }
    Flow::Continue
}
/// `RETURN a`: the result goes to the callee slot just below the frame.
#[inline(always)]
fn ret(vm: &mut Vm<'_>) -> Flow {
    let v = reg(vm, 1);
    let callee_base = vm.base;
    if !pop_frame(vm) {
        vm.ret = v;
        return Flow::Done;
    }
    st(vm.stack, callee_base - 1, v);
    Flow::Continue
}
/// `GETATTR a b ic`: R[a] = R[b].name.
#[inline(always)]
fn getattr(vm: &mut Vm<'_>) -> Flow {
    let v = tri!(rt::get_attr(vm.heap, reg(vm, 2), arg(vm, 3)));
    set_reg(vm, 1, v);
    vm.pc += 4;
    Flow::Continue
}
/// `SETATTR a ic c`: R[a].name = R[c].
#[inline(always)]
fn setattr(vm: &mut Vm<'_>) -> Flow {
    tri!(rt::set_attr(vm.heap, reg(vm, 1), arg(vm, 2), reg(vm, 3)));
    vm.pc += 4;
    Flow::Continue
}
/// `SELF a b ic`.
#[inline(always)]
fn self_(vm: &mut Vm<'_>) -> Flow {
    let obj = reg(vm, 2);
    let f = tri!(rt::load_method(vm.heap, obj, arg(vm, 3)));
    let a = vm.base + arg(vm, 1);
    st(vm.stack, a + 1, obj);
    st(vm.stack, a, f);
    vm.pc += 4;
    Flow::Continue
}
/// `GETITEM a b c`: R[a] = R[b][R[c]].
#[inline(always)]
fn getitem(vm: &mut Vm<'_>) -> Flow {
    let v = tri!(rt::getitem(vm.heap, reg(vm, 2), reg(vm, 3)));
    set_reg(vm, 1, v);
    vm.pc += 4;
    Flow::Continue
}
/// `SETITEM a b c`: R[a][R[b]] = R[c].
#[inline(always)]
fn setitem(vm: &mut Vm<'_>) -> Flow {
    tri!(rt::setitem(vm.heap, reg(vm, 1), reg(vm, 2), reg(vm, 3)));
    vm.pc += 4;
    Flow::Continue
}
#[inline(always)]
fn newlist(vm: &mut Vm<'_>) -> Flow {
    let v = tri!(rt::new_list(vm.heap));
    set_reg(vm, 1, v);
    vm.pc += 2;
    Flow::Continue
}
#[inline(always)]
fn newdict(vm: &mut Vm<'_>) -> Flow {
    let v = tri!(rt::new_dict(vm.heap));
    set_reg(vm, 1, v);
    vm.pc += 2;
    Flow::Continue
}
/// `GETUPVAL a i`.
#[inline(always)]
fn getupval(vm: &mut Vm<'_>) -> Flow {
    let v = tri!(rt::load_deref(vm.heap, vm.func, arg(vm, 2)));
    set_reg(vm, 1, v);
    vm.pc += 3;
    Flow::Continue
}
#[inline(always)]
fn makecell(vm: &mut Vm<'_>) -> Flow {
    let c = tri!(rt::new_cell(vm.heap, reg(vm, 1)));
    set_reg(vm, 1, c);
    vm.pc += 2;
    Flow::Continue
}
/// `CLOSURE a code cell_reg`.
#[inline(always)]
fn closure(vm: &mut Vm<'_>) -> Flow {
    let cell = reg(vm, 3);
    let f = tri!(rt::new_function(vm.heap, arg(vm, 2), &[cell]));
    set_reg(vm, 1, f);
    vm.pc += 4;
    Flow::Continue
}
/// `FORRANGE it dst exit`.
#[inline(always)]
fn forrange(vm: &mut Vm<'_>) -> Flow {
    match tri!(rt::range_next(vm.heap, reg(vm, 1))) {
        Some(v) => {
            set_reg(vm, 2, v);
            vm.pc += 5;
        }
        None => vm.pc = rd16(vm.code, vm.pc + 3),
    }
    Flow::Continue
}

dispatchers! {
    MOVE => mov,
    LOADI => loadi,
    LOADK => loadk,
    GETGLOBAL => getglobal,
    ADD => add,
    SUB => sub,
    LT_JMPF => lt_jmpf,
    JMPBACK => jmpback,
    CALL => call,
    RETURN => ret,
    GETATTR => getattr,
    SETATTR => setattr,
    SELF => self_,
    GETITEM => getitem,
    SETITEM => setitem,
    NEWLIST => newlist,
    NEWDICT => newdict,
    GETUPVAL => getupval,
    MAKECELL => makecell,
    CLOSURE => closure,
    FORRANGE => forrange,
}

// ------------------------------------------------------------------- kernels

/// `R[it] = range(n)`, calling in place (R[it+1] is the argument).
fn range(a: &mut Asm, p: &mut ProgBuilder, n: i64, it: u8) {
    a.op(GETGLOBAL).u(it).u(p.ic(RANGE));
    a.op(LOADK).u(it + 1).u(p.konst(n));
    a.op(CALL).u(it).u(1);
}

pub fn build(k: Kernel) -> Program {
    let mut p = ProgBuilder::default();
    let mut a = Asm::new(&LENS);
    let main = match k {
        Kernel::Fib => {
            // fib(n): n=r0, temporaries r1..r4
            let els = a.label();
            a.op(LOADI).u(1).i(2);
            a.op(LT_JMPF).u(0).u(1).to(els);
            a.op(RETURN).u(0);
            a.bind(els);
            a.op(GETGLOBAL).u(1).u(p.ic(FIB));
            a.op(LOADI).u(3).i(1);
            a.op(SUB).u(2).u(0).u(3);
            a.op(CALL).u(1).u(1);
            a.op(GETGLOBAL).u(2).u(p.ic(FIB));
            a.op(LOADI).u(4).i(2);
            a.op(SUB).u(3).u(0).u(4);
            a.op(CALL).u(2).u(1);
            a.op(ADD).u(1).u(1).u(2);
            a.op(RETURN).u(1);
            p.add("fib", 1, 1, 5, a);
            let mut a = Asm::new(&LENS);
            a.op(GETGLOBAL).u(0).u(p.ic(FIB));
            a.op(LOADI).u(1).i(27);
            a.op(CALL).u(0).u(1);
            a.op(RETURN).u(0);
            p.add("main", 0, 0, 2, a)
        }
        Kernel::Loop => {
            // s=r0 i=r1 it=r2 (r3 = range argument)
            a.op(LOADI).u(0).i(0);
            range(&mut a, &mut p, 3_000_000, 2);
            let (top, exit) = (a.here(), a.label());
            a.op(FORRANGE).u(2).u(1).to(exit);
            a.op(ADD).u(0).u(0).u(1);
            a.op(JMPBACK).to(top);
            a.bind(exit);
            a.op(RETURN).u(0);
            p.add("main", 0, 2, 4, a)
        }
        Kernel::Closure => {
            // add(x): x=r0
            a.op(GETUPVAL).u(1).u(0);
            a.op(ADD).u(1).u(0).u(1);
            a.op(RETURN).u(1);
            let add = p.add("add", 1, 1, 2, a);
            // make(k): k=r0
            let mut a = Asm::new(&LENS);
            a.op(MAKECELL).u(0);
            a.op(CLOSURE).u(1).u(add).u(0);
            a.op(RETURN).u(1);
            p.add("make", 1, 1, 2, a);
            // f=r0 s=r1 i=r2 it=r3, call window r4,r5
            let mut a = Asm::new(&LENS);
            a.op(GETGLOBAL).u(0).u(p.ic(MAKE));
            a.op(LOADI).u(1).i(3);
            a.op(CALL).u(0).u(1);
            a.op(LOADI).u(1).i(0);
            range(&mut a, &mut p, 1_000_000, 3);
            let (top, exit) = (a.here(), a.label());
            a.op(FORRANGE).u(3).u(2).to(exit);
            a.op(MOVE).u(4).u(0);
            a.op(MOVE).u(5).u(1);
            a.op(CALL).u(4).u(1);
            a.op(MOVE).u(1).u(4);
            a.op(JMPBACK).to(top);
            a.bind(exit);
            a.op(RETURN).u(1);
            p.add("main", 0, 3, 6, a)
        }
        Kernel::Attr => {
            // p=r0 s=r1 i=r2 it=r3 t=r4,r5
            a.op(LOADI).u(1).i(0);
            range(&mut a, &mut p, 1_000_000, 3);
            let (top, exit) = (a.here(), a.label());
            a.op(FORRANGE).u(3).u(2).to(exit);
            a.op(GETATTR).u(4).u(0).u(p.ic(X));
            a.op(GETATTR).u(5).u(0).u(p.ic(Y));
            a.op(ADD).u(4).u(4).u(5);
            a.op(ADD).u(1).u(1).u(4);
            a.op(SETATTR).u(0).u(p.ic(X)).u(2);
            a.op(JMPBACK).to(top);
            a.bind(exit);
            a.op(RETURN).u(1);
            p.add("main", 1, 3, 6, a)
        }
        Kernel::Method => {
            // inc(self, v): self=r0 v=r1
            a.op(GETATTR).u(2).u(0).u(p.ic(K));
            a.op(ADD).u(2).u(1).u(2);
            a.op(RETURN).u(2);
            p.add("inc", 2, 2, 3, a);
            // c=r0 s=r1 i=r2 it=r3, call window r4..r6
            let mut a = Asm::new(&LENS);
            a.op(LOADI).u(1).i(0);
            range(&mut a, &mut p, 1_000_000, 3);
            let (top, exit) = (a.here(), a.label());
            a.op(FORRANGE).u(3).u(2).to(exit);
            a.op(SELF).u(4).u(0).u(p.ic(INC));
            a.op(MOVE).u(6).u(1);
            a.op(CALL).u(4).u(2);
            a.op(MOVE).u(1).u(4);
            a.op(JMPBACK).to(top);
            a.bind(exit);
            a.op(RETURN).u(1);
            p.add("main", 1, 3, 7, a)
        }
        Kernel::List => {
            // lst=r0 i=r1 s=r2 it=r3, call window r4..r6
            a.op(NEWLIST).u(0);
            range(&mut a, &mut p, 1_000_000, 3);
            let (top, exit) = (a.here(), a.label());
            a.op(FORRANGE).u(3).u(1).to(exit);
            a.op(SELF).u(4).u(0).u(p.ic(APPEND));
            a.op(MOVE).u(6).u(1);
            a.op(CALL).u(4).u(2);
            a.op(JMPBACK).to(top);
            a.bind(exit);
            sum_loop(&mut a, &mut p, 1_000_000);
            p.add("main", 0, 3, 7, a)
        }
        Kernel::Dict => {
            // d=r0 i=r1 s=r2 it=r3
            a.op(NEWDICT).u(0);
            range(&mut a, &mut p, 200_000, 3);
            let (top, exit) = (a.here(), a.label());
            a.op(FORRANGE).u(3).u(1).to(exit);
            a.op(SETITEM).u(0).u(1).u(1);
            a.op(JMPBACK).to(top);
            a.bind(exit);
            sum_loop(&mut a, &mut p, 200_000);
            p.add("main", 0, 3, 7, a)
        }
    };
    p.finish(main)
}

/// `s = 0; for i in range(n): s += c[i]; return s` with c=r0 i=r1 s=r2 it=r3 t=r4.
fn sum_loop(a: &mut Asm, p: &mut ProgBuilder, n: i64) {
    a.op(LOADI).u(2).i(0);
    range(a, p, n, 3);
    let (top, exit) = (a.here(), a.label());
    a.op(FORRANGE).u(3).u(1).to(exit);
    a.op(GETITEM).u(4).u(0).u(1);
    a.op(ADD).u(2).u(2).u(4);
    a.op(JMPBACK).to(top);
    a.bind(exit);
    a.op(RETURN).u(2);
}
