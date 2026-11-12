//! Design 2: accumulator + register file (V8 Ignition style). 1-byte opcodes,
//! u8 register/IC operands, u16 jump targets. Binary ops are `acc = R[r] op acc`
//! (left operand in a register, as Ignition). No fused compare-and-jump
//! (`TestLessThan` + `JumpIfFalse`, as Ignition). Calls name the callee and
//! argument registers (`CallUndefinedReceiver1 f a`, `CallProperty1 f recv a`)
//! and copy the arguments into a new frame above the caller's frame, as
//! Ignition copies them; the result is left in the accumulator. Bytecode
//! assumes the one peephole Ignition's register optimizer always applies:
//! no `Ldar r` right after `Star r`.

use crate::access::{ld, rd8, rd16, st};
use crate::heap::names::{APPEND, FIB, INC, K, MAKE, RANGE, X, Y};
use crate::kernels::Kernel;
use crate::rt::{self, Callee};
use crate::value::Value;
use crate::vm::{Asm, Flow, ProgBuilder, Program, Vm, pop_frame, push_frame};

pub mod op {
    pub const LDAR: u8 = 1;
    pub const STAR: u8 = 2;
    pub const LDA_SMI: u8 = 3;
    pub const LDA_CONSTANT: u8 = 4;
    pub const LDA_GLOBAL: u8 = 5;
    pub const ADD: u8 = 6;
    pub const SUB: u8 = 7;
    pub const TEST_LESS_THAN: u8 = 8;
    pub const JUMP_IF_FALSE: u8 = 9;
    pub const JUMP_LOOP: u8 = 10;
    pub const CALL_UNDEFINED_RECEIVER1: u8 = 11;
    pub const CALL_PROPERTY1: u8 = 12;
    pub const RETURN: u8 = 13;
    pub const GET_NAMED_PROPERTY: u8 = 14;
    pub const SET_NAMED_PROPERTY: u8 = 15;
    pub const GET_METHOD: u8 = 16;
    pub const GET_KEYED_PROPERTY: u8 = 17;
    pub const SET_KEYED_PROPERTY: u8 = 18;
    pub const CREATE_EMPTY_LIST: u8 = 19;
    pub const CREATE_EMPTY_DICT: u8 = 20;
    pub const LDA_DEREF: u8 = 21;
    pub const MAKE_CELL: u8 = 22;
    pub const CREATE_CLOSURE: u8 = 23;
    pub const FOR_RANGE: u8 = 24;
}
use op::*;

static LENS: [u8; 256] = {
    let mut l = [0u8; 256];
    l[LDAR as usize] = 2;
    l[STAR as usize] = 2;
    l[LDA_SMI as usize] = 2;
    l[LDA_CONSTANT as usize] = 2;
    l[LDA_GLOBAL as usize] = 2;
    l[ADD as usize] = 2;
    l[SUB as usize] = 2;
    l[TEST_LESS_THAN as usize] = 2;
    l[JUMP_IF_FALSE as usize] = 3;
    l[JUMP_LOOP as usize] = 3;
    l[CALL_UNDEFINED_RECEIVER1 as usize] = 3;
    l[CALL_PROPERTY1 as usize] = 4;
    l[RETURN as usize] = 1;
    l[GET_NAMED_PROPERTY as usize] = 3;
    l[SET_NAMED_PROPERTY as usize] = 3;
    l[GET_METHOD as usize] = 3;
    l[GET_KEYED_PROPERTY as usize] = 2;
    l[SET_KEYED_PROPERTY as usize] = 3;
    l[CREATE_EMPTY_LIST as usize] = 1;
    l[CREATE_EMPTY_DICT as usize] = 1;
    l[LDA_DEREF as usize] = 2;
    l[MAKE_CELL as usize] = 2;
    l[CREATE_CLOSURE as usize] = 3;
    l[FOR_RANGE as usize] = 4;
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
fn ldar(vm: &mut Vm<'_>) -> Flow {
    vm.acc = reg(vm, 1);
    vm.pc += 2;
    Flow::Continue
}
#[inline(always)]
fn star(vm: &mut Vm<'_>) -> Flow {
    set_reg(vm, 1, vm.acc);
    vm.pc += 2;
    Flow::Continue
}
#[inline(always)]
fn lda_smi(vm: &mut Vm<'_>) -> Flow {
    vm.acc = Value::int(rd8(vm.code, vm.pc + 1) as i8 as i64);
    vm.pc += 2;
    Flow::Continue
}
#[inline(always)]
fn lda_constant(vm: &mut Vm<'_>) -> Flow {
    vm.acc = vm.prog.consts[arg(vm, 1)];
    vm.pc += 2;
    Flow::Continue
}
#[inline(always)]
fn lda_global(vm: &mut Vm<'_>) -> Flow {
    vm.acc = tri!(rt::load_global(vm.heap, arg(vm, 1)));
    vm.pc += 2;
    Flow::Continue
}
#[inline(always)]
fn add(vm: &mut Vm<'_>) -> Flow {
    vm.acc = tri!(rt::add(reg(vm, 1), vm.acc));
    vm.pc += 2;
    Flow::Continue
}
#[inline(always)]
fn sub(vm: &mut Vm<'_>) -> Flow {
    vm.acc = tri!(rt::sub(reg(vm, 1), vm.acc));
    vm.pc += 2;
    Flow::Continue
}
#[inline(always)]
fn test_less_than(vm: &mut Vm<'_>) -> Flow {
    vm.acc = tri!(rt::lt(reg(vm, 1), vm.acc));
    vm.pc += 2;
    Flow::Continue
}
#[inline(always)]
fn jump_if_false(vm: &mut Vm<'_>) -> Flow {
    if tri!(rt::truthy(vm.heap, vm.acc)) {
        vm.pc += 3;
    } else {
        vm.pc = rd16(vm.code, vm.pc + 1);
    }
    Flow::Continue
}
#[inline(always)]
fn jump_loop(vm: &mut Vm<'_>) -> Flow {
    rt::tick(vm.heap, vm.pc);
    vm.pc = rd16(vm.code, vm.pc + 1);
    Flow::Continue
}

/// Shared tail of the call bytecodes: `args` are copied to a new frame placed
/// right above the caller's frame.
#[inline(always)]
fn do_call<const N: usize>(vm: &mut Vm<'_>, f: Value, args: [Value; N], ret_pc: usize) -> Flow {
    match rt::callee(vm.heap, f) {
        Callee::Py(ci) => {
            let new_base = vm.base + vm.prog.codes[vm.code_idx as usize].frame_size;
            tri!(push_frame(vm, f, ci, new_base, N, ret_pc));
            for (j, a) in args.into_iter().enumerate() {
                st(vm.stack, new_base + j, a);
            }
        }
        Callee::Native(i) => {
            vm.acc = tri!(rt::call_native(vm.heap, i, &args));
            vm.pc = ret_pc;
        }
        Callee::Bad => return Flow::Err(crate::heap::Fail::Type),
    }
    Flow::Continue
}
/// `CallUndefinedReceiver1 f a0`.
#[inline(always)]
fn call_undefined_receiver1(vm: &mut Vm<'_>) -> Flow {
    let f = reg(vm, 1);
    let a0 = reg(vm, 2);
    let ret_pc = vm.pc + 3;
    do_call(vm, f, [a0], ret_pc)
}
/// `CallProperty1 f receiver a0`.
#[inline(always)]
fn call_property1(vm: &mut Vm<'_>) -> Flow {
    let f = reg(vm, 1);
    let recv = reg(vm, 2);
    let a0 = reg(vm, 3);
    let ret_pc = vm.pc + 4;
    do_call(vm, f, [recv, a0], ret_pc)
}
/// The result stays in the accumulator.
#[inline(always)]
fn ret(vm: &mut Vm<'_>) -> Flow {
    if !pop_frame(vm) {
        vm.ret = vm.acc;
        return Flow::Done;
    }
    Flow::Continue
}
#[inline(always)]
fn get_named_property(vm: &mut Vm<'_>) -> Flow {
    vm.acc = tri!(rt::get_attr(vm.heap, reg(vm, 1), arg(vm, 2)));
    vm.pc += 3;
    Flow::Continue
}
#[inline(always)]
fn set_named_property(vm: &mut Vm<'_>) -> Flow {
    tri!(rt::set_attr(vm.heap, reg(vm, 1), arg(vm, 2), vm.acc));
    vm.pc += 3;
    Flow::Continue
}
#[inline(always)]
fn get_method(vm: &mut Vm<'_>) -> Flow {
    vm.acc = tri!(rt::load_method(vm.heap, reg(vm, 1), arg(vm, 2)));
    vm.pc += 3;
    Flow::Continue
}
/// `acc = R[obj][acc]`.
#[inline(always)]
fn get_keyed_property(vm: &mut Vm<'_>) -> Flow {
    vm.acc = tri!(rt::getitem(vm.heap, reg(vm, 1), vm.acc));
    vm.pc += 2;
    Flow::Continue
}
/// `R[obj][R[key]] = acc`.
#[inline(always)]
fn set_keyed_property(vm: &mut Vm<'_>) -> Flow {
    tri!(rt::setitem(vm.heap, reg(vm, 1), reg(vm, 2), vm.acc));
    vm.pc += 3;
    Flow::Continue
}
#[inline(always)]
fn create_empty_list(vm: &mut Vm<'_>) -> Flow {
    vm.acc = tri!(rt::new_list(vm.heap));
    vm.pc += 1;
    Flow::Continue
}
#[inline(always)]
fn create_empty_dict(vm: &mut Vm<'_>) -> Flow {
    vm.acc = tri!(rt::new_dict(vm.heap));
    vm.pc += 1;
    Flow::Continue
}
#[inline(always)]
fn lda_deref(vm: &mut Vm<'_>) -> Flow {
    vm.acc = tri!(rt::load_deref(vm.heap, vm.func, arg(vm, 1)));
    vm.pc += 2;
    Flow::Continue
}
#[inline(always)]
fn make_cell(vm: &mut Vm<'_>) -> Flow {
    let c = tri!(rt::new_cell(vm.heap, reg(vm, 1)));
    set_reg(vm, 1, c);
    vm.pc += 2;
    Flow::Continue
}
/// `CreateClosure code cell_reg`.
#[inline(always)]
fn create_closure(vm: &mut Vm<'_>) -> Flow {
    let cell = reg(vm, 2);
    vm.acc = tri!(rt::new_function(vm.heap, arg(vm, 1), &[cell]));
    vm.pc += 3;
    Flow::Continue
}
/// `ForRange it_reg exit`: next value into the accumulator.
#[inline(always)]
fn for_range(vm: &mut Vm<'_>) -> Flow {
    match tri!(rt::range_next(vm.heap, reg(vm, 1))) {
        Some(v) => {
            vm.acc = v;
            vm.pc += 4;
        }
        None => vm.pc = rd16(vm.code, vm.pc + 2),
    }
    Flow::Continue
}

dispatchers! {
    LDAR => ldar,
    STAR => star,
    LDA_SMI => lda_smi,
    LDA_CONSTANT => lda_constant,
    LDA_GLOBAL => lda_global,
    ADD => add,
    SUB => sub,
    TEST_LESS_THAN => test_less_than,
    JUMP_IF_FALSE => jump_if_false,
    JUMP_LOOP => jump_loop,
    CALL_UNDEFINED_RECEIVER1 => call_undefined_receiver1,
    CALL_PROPERTY1 => call_property1,
    RETURN => ret,
    GET_NAMED_PROPERTY => get_named_property,
    SET_NAMED_PROPERTY => set_named_property,
    GET_METHOD => get_method,
    GET_KEYED_PROPERTY => get_keyed_property,
    SET_KEYED_PROPERTY => set_keyed_property,
    CREATE_EMPTY_LIST => create_empty_list,
    CREATE_EMPTY_DICT => create_empty_dict,
    LDA_DEREF => lda_deref,
    MAKE_CELL => make_cell,
    CREATE_CLOSURE => create_closure,
    FOR_RANGE => for_range,
}

// ------------------------------------------------------------------- kernels

/// `R[it] = range(n)` using temporaries `t`, `t+1`.
fn range(a: &mut Asm, p: &mut ProgBuilder, n: i64, it: u8, t: u8) {
    a.op(LDA_GLOBAL).u(p.ic(RANGE));
    a.op(STAR).u(t);
    a.op(LDA_CONSTANT).u(p.konst(n));
    a.op(STAR).u(t + 1);
    a.op(CALL_UNDEFINED_RECEIVER1).u(t).u(t + 1);
    a.op(STAR).u(it);
}

pub fn build(k: Kernel) -> Program {
    let mut p = ProgBuilder::default();
    let mut a = Asm::new(&LENS);
    let main = match k {
        Kernel::Fib => {
            // fib(n): n=r0, temporaries r1..r3
            let els = a.label();
            a.op(LDA_SMI).i(2);
            a.op(TEST_LESS_THAN).u(0);
            a.op(JUMP_IF_FALSE).to(els);
            a.op(LDAR).u(0);
            a.op(RETURN);
            a.bind(els);
            a.op(LDA_GLOBAL).u(p.ic(FIB));
            a.op(STAR).u(1);
            a.op(LDA_SMI).i(1);
            a.op(SUB).u(0);
            a.op(STAR).u(2);
            a.op(CALL_UNDEFINED_RECEIVER1).u(1).u(2);
            a.op(STAR).u(3);
            a.op(LDA_GLOBAL).u(p.ic(FIB));
            a.op(STAR).u(1);
            a.op(LDA_SMI).i(2);
            a.op(SUB).u(0);
            a.op(STAR).u(2);
            a.op(CALL_UNDEFINED_RECEIVER1).u(1).u(2);
            a.op(ADD).u(3);
            a.op(RETURN);
            p.add("fib", 1, 1, 4, a);
            let mut a = Asm::new(&LENS);
            a.op(LDA_GLOBAL).u(p.ic(FIB));
            a.op(STAR).u(0);
            a.op(LDA_SMI).i(27);
            a.op(STAR).u(1);
            a.op(CALL_UNDEFINED_RECEIVER1).u(0).u(1);
            a.op(RETURN);
            p.add("main", 0, 0, 2, a)
        }
        Kernel::Loop => {
            // s=r0 i=r1 it=r2 t=r3,r4
            a.op(LDA_SMI).i(0);
            a.op(STAR).u(0);
            range(&mut a, &mut p, 3_000_000, 2, 3);
            let (top, exit) = (a.here(), a.label());
            a.op(FOR_RANGE).u(2).to(exit);
            a.op(STAR).u(1);
            a.op(ADD).u(0);
            a.op(STAR).u(0);
            a.op(JUMP_LOOP).to(top);
            a.bind(exit);
            a.op(LDAR).u(0);
            a.op(RETURN);
            p.add("main", 0, 2, 5, a)
        }
        Kernel::Closure => {
            // add(x): x=r0
            a.op(LDA_DEREF).u(0);
            a.op(ADD).u(0);
            a.op(RETURN);
            let add = p.add("add", 1, 1, 1, a);
            // make(k): k=r0
            let mut a = Asm::new(&LENS);
            a.op(MAKE_CELL).u(0);
            a.op(CREATE_CLOSURE).u(add).u(0);
            a.op(RETURN);
            p.add("make", 1, 1, 1, a);
            // f=r0 s=r1 i=r2 it=r3 t=r4,r5
            let mut a = Asm::new(&LENS);
            a.op(LDA_GLOBAL).u(p.ic(MAKE));
            a.op(STAR).u(4);
            a.op(LDA_SMI).i(3);
            a.op(STAR).u(5);
            a.op(CALL_UNDEFINED_RECEIVER1).u(4).u(5);
            a.op(STAR).u(0);
            a.op(LDA_SMI).i(0);
            a.op(STAR).u(1);
            range(&mut a, &mut p, 1_000_000, 3, 4);
            let (top, exit) = (a.here(), a.label());
            a.op(FOR_RANGE).u(3).to(exit);
            a.op(STAR).u(2);
            a.op(CALL_UNDEFINED_RECEIVER1).u(0).u(1);
            a.op(STAR).u(1);
            a.op(JUMP_LOOP).to(top);
            a.bind(exit);
            a.op(LDAR).u(1);
            a.op(RETURN);
            p.add("main", 0, 3, 6, a)
        }
        Kernel::Attr => {
            // p=r0 s=r1 i=r2 it=r3 t=r4,r5
            a.op(LDA_SMI).i(0);
            a.op(STAR).u(1);
            range(&mut a, &mut p, 1_000_000, 3, 4);
            let (top, exit) = (a.here(), a.label());
            a.op(FOR_RANGE).u(3).to(exit);
            a.op(STAR).u(2);
            a.op(GET_NAMED_PROPERTY).u(0).u(p.ic(X));
            a.op(STAR).u(4);
            a.op(GET_NAMED_PROPERTY).u(0).u(p.ic(Y));
            a.op(ADD).u(4);
            a.op(ADD).u(1);
            a.op(STAR).u(1);
            a.op(LDAR).u(2);
            a.op(SET_NAMED_PROPERTY).u(0).u(p.ic(X));
            a.op(JUMP_LOOP).to(top);
            a.bind(exit);
            a.op(LDAR).u(1);
            a.op(RETURN);
            p.add("main", 1, 3, 6, a)
        }
        Kernel::Method => {
            // inc(self, v): self=r0 v=r1
            a.op(GET_NAMED_PROPERTY).u(0).u(p.ic(K));
            a.op(ADD).u(1);
            a.op(RETURN);
            p.add("inc", 2, 2, 2, a);
            // c=r0 s=r1 i=r2 it=r3 t=r4,r5
            let mut a = Asm::new(&LENS);
            a.op(LDA_SMI).i(0);
            a.op(STAR).u(1);
            range(&mut a, &mut p, 1_000_000, 3, 4);
            let (top, exit) = (a.here(), a.label());
            a.op(FOR_RANGE).u(3).to(exit);
            a.op(STAR).u(2);
            a.op(GET_METHOD).u(0).u(p.ic(INC));
            a.op(STAR).u(4);
            a.op(CALL_PROPERTY1).u(4).u(0).u(1);
            a.op(STAR).u(1);
            a.op(JUMP_LOOP).to(top);
            a.bind(exit);
            a.op(LDAR).u(1);
            a.op(RETURN);
            p.add("main", 1, 3, 6, a)
        }
        Kernel::List => {
            // lst=r0 i=r1 s=r2 it=r3 t=r4,r5
            a.op(CREATE_EMPTY_LIST);
            a.op(STAR).u(0);
            range(&mut a, &mut p, 1_000_000, 3, 4);
            let (top, exit) = (a.here(), a.label());
            a.op(FOR_RANGE).u(3).to(exit);
            a.op(STAR).u(1);
            a.op(GET_METHOD).u(0).u(p.ic(APPEND));
            a.op(STAR).u(4);
            a.op(CALL_PROPERTY1).u(4).u(0).u(1);
            a.op(JUMP_LOOP).to(top);
            a.bind(exit);
            sum_loop(&mut a, &mut p, 1_000_000);
            p.add("main", 0, 3, 6, a)
        }
        Kernel::Dict => {
            // d=r0 i=r1 s=r2 it=r3 t=r4,r5
            a.op(CREATE_EMPTY_DICT);
            a.op(STAR).u(0);
            range(&mut a, &mut p, 200_000, 3, 4);
            let (top, exit) = (a.here(), a.label());
            a.op(FOR_RANGE).u(3).to(exit);
            a.op(STAR).u(1);
            a.op(SET_KEYED_PROPERTY).u(0).u(1);
            a.op(JUMP_LOOP).to(top);
            a.bind(exit);
            sum_loop(&mut a, &mut p, 200_000);
            p.add("main", 0, 3, 6, a)
        }
    };
    p.finish(main)
}

/// `s = 0; for i in range(n): s += c[i]; return s` with c=r0 i=r1 s=r2 it=r3.
fn sum_loop(a: &mut Asm, p: &mut ProgBuilder, n: i64) {
    a.op(LDA_SMI).i(0);
    a.op(STAR).u(2);
    range(a, p, n, 3, 4);
    let (top, exit) = (a.here(), a.label());
    a.op(FOR_RANGE).u(3).to(exit);
    a.op(STAR).u(1);
    a.op(GET_KEYED_PROPERTY).u(0);
    a.op(ADD).u(2);
    a.op(STAR).u(2);
    a.op(JUMP_LOOP).to(top);
    a.bind(exit);
    a.op(LDAR).u(2);
    a.op(RETURN);
}
