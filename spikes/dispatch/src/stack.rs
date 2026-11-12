//! Design 1: stack machine (CPython / QuickJS style). 1-byte opcodes, u8
//! operands, u16 jump targets. Superinstructions as planned in spec §8.1:
//! `LOAD_FAST_LOAD_FAST`, fused `COMPARE_LT_JUMP_IF_FALSE`; method calls are
//! `LOAD_METHOD` + `CALL`. Calls put `[callee, args...]` on the operand stack
//! and the callee frame's locals overlay the arguments (no copy).

use crate::access::{ld, rd8, rd16, sl, st};
use crate::heap::names::{APPEND, FIB, INC, K, MAKE, RANGE, X, Y};
use crate::kernels::Kernel;
use crate::rt::{self, Callee};
use crate::value::Value;
use crate::vm::{Asm, Flow, ProgBuilder, Program, Vm, pop_frame, push_frame};

pub mod op {
    pub const LOAD_FAST: u8 = 1;
    pub const STORE_FAST: u8 = 2;
    pub const LOAD_FAST_LOAD_FAST: u8 = 3;
    pub const LOAD_SMALL: u8 = 4;
    pub const LOAD_CONST: u8 = 5;
    pub const POP_TOP: u8 = 6;
    pub const LOAD_GLOBAL: u8 = 7;
    pub const BINARY_ADD: u8 = 8;
    pub const BINARY_SUB: u8 = 9;
    pub const COMPARE_LT_JUMP_IF_FALSE: u8 = 10;
    pub const JUMP_BACKWARD: u8 = 11;
    pub const CALL: u8 = 12;
    pub const RETURN_VALUE: u8 = 13;
    pub const LOAD_ATTR: u8 = 14;
    pub const STORE_ATTR: u8 = 15;
    pub const LOAD_METHOD: u8 = 16;
    pub const BINARY_SUBSCR: u8 = 17;
    pub const STORE_SUBSCR: u8 = 18;
    pub const BUILD_LIST: u8 = 19;
    pub const BUILD_MAP: u8 = 20;
    pub const LOAD_DEREF: u8 = 21;
    pub const MAKE_CELL: u8 = 22;
    pub const MAKE_CLOSURE: u8 = 23;
    pub const FOR_ITER_RANGE: u8 = 24;
}
use op::*;

/// Instruction lengths in bytes.
static LENS: [u8; 256] = {
    let mut l = [0u8; 256];
    l[LOAD_FAST as usize] = 2;
    l[STORE_FAST as usize] = 2;
    l[LOAD_FAST_LOAD_FAST as usize] = 3;
    l[LOAD_SMALL as usize] = 2;
    l[LOAD_CONST as usize] = 2;
    l[POP_TOP as usize] = 1;
    l[LOAD_GLOBAL as usize] = 2;
    l[BINARY_ADD as usize] = 1;
    l[BINARY_SUB as usize] = 1;
    l[COMPARE_LT_JUMP_IF_FALSE as usize] = 3;
    l[JUMP_BACKWARD as usize] = 3;
    l[CALL as usize] = 2;
    l[RETURN_VALUE as usize] = 1;
    l[LOAD_ATTR as usize] = 2;
    l[STORE_ATTR as usize] = 2;
    l[LOAD_METHOD as usize] = 2;
    l[BINARY_SUBSCR as usize] = 1;
    l[STORE_SUBSCR as usize] = 1;
    l[BUILD_LIST as usize] = 1;
    l[BUILD_MAP as usize] = 1;
    l[LOAD_DEREF as usize] = 2;
    l[MAKE_CELL as usize] = 2;
    l[MAKE_CLOSURE as usize] = 3;
    l[FOR_ITER_RANGE as usize] = 3;
    l
};

// ------------------------------------------------------------ operand access

#[inline(always)]
fn arg(vm: &Vm<'_>, k: usize) -> usize {
    rd8(vm.code, vm.pc + k) as usize
}
#[inline(always)]
fn local(vm: &Vm<'_>, i: usize) -> Value {
    ld(vm.stack, vm.base + i)
}
#[inline(always)]
fn push(vm: &mut Vm<'_>, v: Value) {
    st(vm.stack, vm.sp, v);
    vm.sp += 1;
}
/// `peek(vm, 1)` is the top of stack.
#[inline(always)]
fn peek(vm: &Vm<'_>, k: usize) -> Value {
    ld(vm.stack, vm.sp - k)
}

// ------------------------------------------------------------------ handlers

#[inline(always)]
fn load_fast(vm: &mut Vm<'_>) -> Flow {
    let v = local(vm, arg(vm, 1));
    push(vm, v);
    vm.pc += 2;
    Flow::Continue
}
#[inline(always)]
fn store_fast(vm: &mut Vm<'_>) -> Flow {
    let v = peek(vm, 1);
    vm.sp -= 1;
    st(vm.stack, vm.base + arg(vm, 1), v);
    vm.pc += 2;
    Flow::Continue
}
#[inline(always)]
fn load_fast_load_fast(vm: &mut Vm<'_>) -> Flow {
    let a = local(vm, arg(vm, 1));
    let b = local(vm, arg(vm, 2));
    push(vm, a);
    push(vm, b);
    vm.pc += 3;
    Flow::Continue
}
#[inline(always)]
fn load_small(vm: &mut Vm<'_>) -> Flow {
    let v = Value::int(rd8(vm.code, vm.pc + 1) as i8 as i64);
    push(vm, v);
    vm.pc += 2;
    Flow::Continue
}
#[inline(always)]
fn load_const(vm: &mut Vm<'_>) -> Flow {
    let v = vm.prog.consts[arg(vm, 1)];
    push(vm, v);
    vm.pc += 2;
    Flow::Continue
}
#[inline(always)]
fn pop_top(vm: &mut Vm<'_>) -> Flow {
    vm.sp -= 1;
    vm.pc += 1;
    Flow::Continue
}
#[inline(always)]
fn load_global(vm: &mut Vm<'_>) -> Flow {
    let v = tri!(rt::load_global(vm.heap, arg(vm, 1)));
    push(vm, v);
    vm.pc += 2;
    Flow::Continue
}
#[inline(always)]
fn binary_add(vm: &mut Vm<'_>) -> Flow {
    let r = tri!(rt::add(peek(vm, 2), peek(vm, 1)));
    st(vm.stack, vm.sp - 2, r);
    vm.sp -= 1;
    vm.pc += 1;
    Flow::Continue
}
#[inline(always)]
fn binary_sub(vm: &mut Vm<'_>) -> Flow {
    let r = tri!(rt::sub(peek(vm, 2), peek(vm, 1)));
    st(vm.stack, vm.sp - 2, r);
    vm.sp -= 1;
    vm.pc += 1;
    Flow::Continue
}
#[inline(always)]
fn compare_lt_jump_if_false(vm: &mut Vm<'_>) -> Flow {
    let t = tri!(rt::lt_bool(peek(vm, 2), peek(vm, 1)));
    vm.sp -= 2;
    if t {
        vm.pc += 3;
    } else {
        vm.pc = rd16(vm.code, vm.pc + 1);
    }
    Flow::Continue
}
#[inline(always)]
fn jump_backward(vm: &mut Vm<'_>) -> Flow {
    rt::tick(vm.heap, vm.pc);
    vm.pc = rd16(vm.code, vm.pc + 1);
    Flow::Continue
}
/// `CALL n`: stack is `[callee, a0 .. a(n-1)]`.
#[inline(always)]
fn call(vm: &mut Vm<'_>) -> Flow {
    let n = arg(vm, 1);
    let fpos = vm.sp - n - 1;
    let f = ld(vm.stack, fpos);
    let ret_pc = vm.pc + 2;
    match rt::callee(vm.heap, f) {
        Callee::Py(ci) => {
            let code = tri!(push_frame(vm, f, ci, fpos + 1, n, ret_pc));
            vm.sp = fpos + 1 + code.nlocals;
        }
        Callee::Native(i) => {
            let r = tri!(rt::call_native(vm.heap, i, sl(vm.stack, fpos + 1, n)));
            st(vm.stack, fpos, r);
            vm.sp = fpos + 1;
            vm.pc = ret_pc;
        }
        Callee::Bad => return Flow::Err(crate::heap::Fail::Type),
    }
    Flow::Continue
}
/// The result replaces the callee slot just below the returning frame.
#[inline(always)]
fn return_value(vm: &mut Vm<'_>) -> Flow {
    let v = peek(vm, 1);
    let callee_base = vm.base;
    if !pop_frame(vm) {
        vm.ret = v;
        return Flow::Done;
    }
    st(vm.stack, callee_base - 1, v);
    vm.sp = callee_base;
    Flow::Continue
}
#[inline(always)]
fn load_attr(vm: &mut Vm<'_>) -> Flow {
    let v = tri!(rt::get_attr(vm.heap, peek(vm, 1), arg(vm, 1)));
    st(vm.stack, vm.sp - 1, v);
    vm.pc += 2;
    Flow::Continue
}
/// `STORE_ATTR`: TOS is the object, TOS1 the value.
#[inline(always)]
fn store_attr(vm: &mut Vm<'_>) -> Flow {
    tri!(rt::set_attr(vm.heap, peek(vm, 1), arg(vm, 1), peek(vm, 2)));
    vm.sp -= 2;
    vm.pc += 2;
    Flow::Continue
}
/// `LOAD_METHOD`: replaces `obj` with `[function, obj]`.
#[inline(always)]
fn load_method(vm: &mut Vm<'_>) -> Flow {
    let obj = peek(vm, 1);
    let f = tri!(rt::load_method(vm.heap, obj, arg(vm, 1)));
    st(vm.stack, vm.sp - 1, f);
    push(vm, obj);
    vm.pc += 2;
    Flow::Continue
}
#[inline(always)]
fn binary_subscr(vm: &mut Vm<'_>) -> Flow {
    let v = tri!(rt::getitem(vm.heap, peek(vm, 2), peek(vm, 1)));
    st(vm.stack, vm.sp - 2, v);
    vm.sp -= 1;
    vm.pc += 1;
    Flow::Continue
}
/// `STORE_SUBSCR`: TOS key, TOS1 container, TOS2 value.
#[inline(always)]
fn store_subscr(vm: &mut Vm<'_>) -> Flow {
    tri!(rt::setitem(vm.heap, peek(vm, 2), peek(vm, 1), peek(vm, 3)));
    vm.sp -= 3;
    vm.pc += 1;
    Flow::Continue
}
#[inline(always)]
fn build_list(vm: &mut Vm<'_>) -> Flow {
    let v = tri!(rt::new_list(vm.heap));
    push(vm, v);
    vm.pc += 1;
    Flow::Continue
}
#[inline(always)]
fn build_map(vm: &mut Vm<'_>) -> Flow {
    let v = tri!(rt::new_dict(vm.heap));
    push(vm, v);
    vm.pc += 1;
    Flow::Continue
}
#[inline(always)]
fn load_deref(vm: &mut Vm<'_>) -> Flow {
    let v = tri!(rt::load_deref(vm.heap, vm.func, arg(vm, 1)));
    push(vm, v);
    vm.pc += 2;
    Flow::Continue
}
#[inline(always)]
fn make_cell(vm: &mut Vm<'_>) -> Flow {
    let l = arg(vm, 1);
    let c = tri!(rt::new_cell(vm.heap, local(vm, l)));
    st(vm.stack, vm.base + l, c);
    vm.pc += 2;
    Flow::Continue
}
/// `MAKE_CLOSURE code cell_local`.
#[inline(always)]
fn make_closure(vm: &mut Vm<'_>) -> Flow {
    let cell = local(vm, arg(vm, 2));
    let f = tri!(rt::new_function(vm.heap, arg(vm, 1), &[cell]));
    push(vm, f);
    vm.pc += 3;
    Flow::Continue
}
/// TOS is the range iterator; exhausted → pop it and jump.
#[inline(always)]
fn for_iter_range(vm: &mut Vm<'_>) -> Flow {
    match tri!(rt::range_next(vm.heap, peek(vm, 1))) {
        Some(v) => {
            push(vm, v);
            vm.pc += 3;
        }
        None => {
            vm.sp -= 1;
            vm.pc = rd16(vm.code, vm.pc + 1);
        }
    }
    Flow::Continue
}

dispatchers! {
    LOAD_FAST => load_fast,
    STORE_FAST => store_fast,
    LOAD_FAST_LOAD_FAST => load_fast_load_fast,
    LOAD_SMALL => load_small,
    LOAD_CONST => load_const,
    POP_TOP => pop_top,
    LOAD_GLOBAL => load_global,
    BINARY_ADD => binary_add,
    BINARY_SUB => binary_sub,
    COMPARE_LT_JUMP_IF_FALSE => compare_lt_jump_if_false,
    JUMP_BACKWARD => jump_backward,
    CALL => call,
    RETURN_VALUE => return_value,
    LOAD_ATTR => load_attr,
    STORE_ATTR => store_attr,
    LOAD_METHOD => load_method,
    BINARY_SUBSCR => binary_subscr,
    STORE_SUBSCR => store_subscr,
    BUILD_LIST => build_list,
    BUILD_MAP => build_map,
    LOAD_DEREF => load_deref,
    MAKE_CELL => make_cell,
    MAKE_CLOSURE => make_closure,
    FOR_ITER_RANGE => for_iter_range,
}

// ------------------------------------------------------------------- kernels

/// Operand-stack headroom on top of the locals.
const STACK_HEADROOM: usize = 8;

/// `range(n)` iterator onto the stack.
fn range(a: &mut Asm, p: &mut ProgBuilder, n: i64) {
    a.op(LOAD_GLOBAL).u(p.ic(RANGE));
    a.op(LOAD_CONST).u(p.konst(n));
    a.op(CALL).u(1);
}

pub fn build(k: Kernel) -> Program {
    let mut p = ProgBuilder::default();
    let mut a = Asm::new(&LENS);
    let main = match k {
        Kernel::Fib => {
            // def fib(n): return n if n < 2 else fib(n-1) + fib(n-2)
            let els = a.label();
            a.op(LOAD_FAST).u(0);
            a.op(LOAD_SMALL).i(2);
            a.op(COMPARE_LT_JUMP_IF_FALSE).to(els);
            a.op(LOAD_FAST).u(0);
            a.op(RETURN_VALUE);
            a.bind(els);
            a.op(LOAD_GLOBAL).u(p.ic(FIB));
            a.op(LOAD_FAST).u(0);
            a.op(LOAD_SMALL).i(1);
            a.op(BINARY_SUB);
            a.op(CALL).u(1);
            a.op(LOAD_GLOBAL).u(p.ic(FIB));
            a.op(LOAD_FAST).u(0);
            a.op(LOAD_SMALL).i(2);
            a.op(BINARY_SUB);
            a.op(CALL).u(1);
            a.op(BINARY_ADD);
            a.op(RETURN_VALUE);
            p.add("fib", 1, 1, 1 + STACK_HEADROOM, a);
            // fib(27)
            let mut a = Asm::new(&LENS);
            a.op(LOAD_GLOBAL).u(p.ic(FIB));
            a.op(LOAD_SMALL).i(27);
            a.op(CALL).u(1);
            a.op(RETURN_VALUE);
            p.add("main", 0, 0, STACK_HEADROOM, a)
        }
        Kernel::Loop => {
            // s = 0; for i in range(3_000_000): s += i      locals: s=0 i=1
            a.op(LOAD_SMALL).i(0);
            a.op(STORE_FAST).u(0);
            range(&mut a, &mut p, 3_000_000);
            let (top, exit) = (a.here(), a.label());
            a.op(FOR_ITER_RANGE).to(exit);
            a.op(STORE_FAST).u(1);
            a.op(LOAD_FAST_LOAD_FAST).u(0).u(1);
            a.op(BINARY_ADD);
            a.op(STORE_FAST).u(0);
            a.op(JUMP_BACKWARD).to(top);
            a.bind(exit);
            a.op(LOAD_FAST).u(0);
            a.op(RETURN_VALUE);
            p.add("main", 0, 2, 2 + STACK_HEADROOM, a)
        }
        Kernel::Closure => {
            // def add(x): return x + k
            a.op(LOAD_FAST).u(0);
            a.op(LOAD_DEREF).u(0);
            a.op(BINARY_ADD);
            a.op(RETURN_VALUE);
            let add = p.add("add", 1, 1, 1 + STACK_HEADROOM, a);
            // def make(k): return add       (k is a cell)
            let mut a = Asm::new(&LENS);
            a.op(MAKE_CELL).u(0);
            a.op(MAKE_CLOSURE).u(add).u(0);
            a.op(RETURN_VALUE);
            p.add("make", 1, 1, 1 + STACK_HEADROOM, a);
            // f = make(3); s = 0; for i in range(1_000_000): s = f(s)   f=0 s=1 i=2
            let mut a = Asm::new(&LENS);
            a.op(LOAD_GLOBAL).u(p.ic(MAKE));
            a.op(LOAD_SMALL).i(3);
            a.op(CALL).u(1);
            a.op(STORE_FAST).u(0);
            a.op(LOAD_SMALL).i(0);
            a.op(STORE_FAST).u(1);
            range(&mut a, &mut p, 1_000_000);
            let (top, exit) = (a.here(), a.label());
            a.op(FOR_ITER_RANGE).to(exit);
            a.op(STORE_FAST).u(2);
            a.op(LOAD_FAST_LOAD_FAST).u(0).u(1);
            a.op(CALL).u(1);
            a.op(STORE_FAST).u(1);
            a.op(JUMP_BACKWARD).to(top);
            a.bind(exit);
            a.op(LOAD_FAST).u(1);
            a.op(RETURN_VALUE);
            p.add("main", 0, 3, 3 + STACK_HEADROOM, a)
        }
        Kernel::Attr => {
            // (p) s = 0; for i in range(1_000_000): s += p.x + p.y; p.x = i    p=0 s=1 i=2
            a.op(LOAD_SMALL).i(0);
            a.op(STORE_FAST).u(1);
            range(&mut a, &mut p, 1_000_000);
            let (top, exit) = (a.here(), a.label());
            a.op(FOR_ITER_RANGE).to(exit);
            a.op(STORE_FAST).u(2);
            a.op(LOAD_FAST_LOAD_FAST).u(1).u(0);
            a.op(LOAD_ATTR).u(p.ic(X));
            a.op(LOAD_FAST).u(0);
            a.op(LOAD_ATTR).u(p.ic(Y));
            a.op(BINARY_ADD);
            a.op(BINARY_ADD);
            a.op(STORE_FAST).u(1);
            a.op(LOAD_FAST_LOAD_FAST).u(2).u(0);
            a.op(STORE_ATTR).u(p.ic(X));
            a.op(JUMP_BACKWARD).to(top);
            a.bind(exit);
            a.op(LOAD_FAST).u(1);
            a.op(RETURN_VALUE);
            p.add("main", 1, 3, 3 + STACK_HEADROOM, a)
        }
        Kernel::Method => {
            // def inc(self, v): return v + self.k
            a.op(LOAD_FAST_LOAD_FAST).u(1).u(0);
            a.op(LOAD_ATTR).u(p.ic(K));
            a.op(BINARY_ADD);
            a.op(RETURN_VALUE);
            p.add("inc", 2, 2, 2 + STACK_HEADROOM, a);
            // (c) s = 0; for i in range(1_000_000): s = c.inc(s)    c=0 s=1 i=2
            let mut a = Asm::new(&LENS);
            a.op(LOAD_SMALL).i(0);
            a.op(STORE_FAST).u(1);
            range(&mut a, &mut p, 1_000_000);
            let (top, exit) = (a.here(), a.label());
            a.op(FOR_ITER_RANGE).to(exit);
            a.op(STORE_FAST).u(2);
            a.op(LOAD_FAST).u(0);
            a.op(LOAD_METHOD).u(p.ic(INC));
            a.op(LOAD_FAST).u(1);
            a.op(CALL).u(2);
            a.op(STORE_FAST).u(1);
            a.op(JUMP_BACKWARD).to(top);
            a.bind(exit);
            a.op(LOAD_FAST).u(1);
            a.op(RETURN_VALUE);
            p.add("main", 1, 3, 3 + STACK_HEADROOM, a)
        }
        Kernel::List => {
            // lst = []; for i in range(1M): lst.append(i)
            // s = 0; for i in range(1M): s += lst[i]          lst=0 i=1 s=2
            a.op(BUILD_LIST);
            a.op(STORE_FAST).u(0);
            range(&mut a, &mut p, 1_000_000);
            let (top, exit) = (a.here(), a.label());
            a.op(FOR_ITER_RANGE).to(exit);
            a.op(STORE_FAST).u(1);
            a.op(LOAD_FAST).u(0);
            a.op(LOAD_METHOD).u(p.ic(APPEND));
            a.op(LOAD_FAST).u(1);
            a.op(CALL).u(2);
            a.op(POP_TOP);
            a.op(JUMP_BACKWARD).to(top);
            a.bind(exit);
            sum_loop(&mut a, &mut p, 1_000_000);
            p.add("main", 0, 3, 3 + STACK_HEADROOM, a)
        }
        Kernel::Dict => {
            // d = {}; for i in range(200k): d[i] = i
            // s = 0; for i in range(200k): s += d[i]          d=0 i=1 s=2
            a.op(BUILD_MAP);
            a.op(STORE_FAST).u(0);
            range(&mut a, &mut p, 200_000);
            let (top, exit) = (a.here(), a.label());
            a.op(FOR_ITER_RANGE).to(exit);
            a.op(STORE_FAST).u(1);
            a.op(LOAD_FAST).u(1);
            a.op(LOAD_FAST_LOAD_FAST).u(0).u(1);
            a.op(STORE_SUBSCR);
            a.op(JUMP_BACKWARD).to(top);
            a.bind(exit);
            sum_loop(&mut a, &mut p, 200_000);
            p.add("main", 0, 3, 3 + STACK_HEADROOM, a)
        }
    };
    p.finish(main)
}

/// `s = 0; for i in range(n): s += c[i]; return s` with c=0 i=1 s=2.
fn sum_loop(a: &mut Asm, p: &mut ProgBuilder, n: i64) {
    a.op(LOAD_SMALL).i(0);
    a.op(STORE_FAST).u(2);
    range(a, p, n);
    let (top, exit) = (a.here(), a.label());
    a.op(FOR_ITER_RANGE).to(exit);
    a.op(STORE_FAST).u(1);
    a.op(LOAD_FAST_LOAD_FAST).u(2).u(0);
    a.op(LOAD_FAST).u(1);
    a.op(BINARY_SUBSCR);
    a.op(BINARY_ADD);
    a.op(STORE_FAST).u(2);
    a.op(JUMP_BACKWARD).to(top);
    a.bind(exit);
    a.op(LOAD_FAST).u(2);
    a.op(RETURN_VALUE);
}
