//! Kernel programs, hand-assembled as mm-engine's compiler emits them
//! (`docs/spikes/dispatch2.md` lists the instruction sequences), with the
//! new call window and range loop.

use crate::code::{self, Asm, op};
use crate::heap::{Heap, exc};
use crate::value::Value;
use crate::vm::{NATIVE_LEN, NATIVE_RANGE, Vm};

pub struct Kernel {
    pub name: &'static str,
    /// The line the bench program prints.
    pub expected: &'static str,
    pub build: fn(&mut Names) -> Vec<Unit>,
    /// Formats the result as the bench program prints it.
    pub show: fn(&Vm, Value) -> String,
}

pub type Unit = (code::CodeEntry, Vec<Value>);

/// Interned names, created on the heap before execution (deterministic, so
/// a rebuilt program has the same addresses as the snapshotted one).
pub struct Names<'h> {
    heap: &'h mut Heap,
    names: Vec<(&'static str, Value)>,
}

impl Names<'_> {
    pub fn get(&mut self, s: &'static str) -> Value {
        if let Some(&(_, v)) = self.names.iter().find(|(n, _)| *n == s) {
            return v;
        }
        let v = self.heap.str_new(s);
        self.names.push((s, v));
        v
    }
}

pub fn build(k: &Kernel) -> Vm {
    let mut heap = Heap::new();
    let (units, range, len) = {
        let mut n = Names {
            heap: &mut heap,
            names: Vec::new(),
        };
        let range = n.get("range");
        let len = n.get("len");
        ((k.build)(&mut n), range, len)
    };
    let (codes, ics) = code::table(units);
    let builtins = vec![
        (range, Value::native(NATIVE_RANGE)),
        (len, Value::native(NATIVE_LEN)),
    ];
    let mut vm = Vm::new(codes, ics, heap, builtins);
    vm.start(0);
    vm
}

fn int(_: &Vm, v: Value) -> String {
    if v.is_int() {
        v.int_val().to_string()
    } else {
        format!("{v:?}")
    }
}

/// `print(len(d), total)` for dictint: `n` is a global set by the program.
fn dict_show(vm: &Vm, v: Value) -> String {
    let n = vm
        .heap
        .dict_lookup(vm.heap.globals, find_name(vm, "n"))
        .ok()
        .flatten();
    let n = n.map_or(-1, |(e, i)| vm.heap.val(e + 3 * i + 3).int_val());
    format!("{n} {}", int(vm, v))
}

fn find_name(vm: &Vm, s: &str) -> Value {
    // Names are interned; find the str with these bytes among IC names.
    for ic in &vm.ics {
        let a = ic.name.addr();
        let len = vm.heap.val(a + crate::heap::STR_LEN).int_val() as usize;
        if len == s.len() {
            let w = vm.heap.arena.get(a + crate::heap::STR_BYTES);
            let mut b = [0u8; 8];
            b[..len.min(8)].copy_from_slice(&s.as_bytes()[..len.min(8)]);
            if w == u64::from_le_bytes(b) {
                return ic.name;
            }
        }
    }
    Value::NONE
}

// fib.py
//   fib: LDA_SMI 2; TEST_LT_JF 0 (+JIF); LDAR 0; RETURN;
//        LDA_GLOBAL_STAR fib (+STAR); LDA_SMI_SUB 1 (+SUB 0); STAR; CALL; STAR;
//        LDA_GLOBAL_STAR fib (+STAR); LDA_SMI_SUB 2 (+SUB 0); STAR; CALL; ADD 1; RETURN
fn fib(n: &mut Names) -> Vec<Unit> {
    let fibn = n.get("fib");
    let mut m = Asm::default();
    m.raw(op::MAKE_FUNCTION, 0, 1);
    m.sta_global(fibn);
    m.lda_global_star(0, fibn);
    m.lda_smi(27);
    m.r(op::STAR, 2);
    m.rn(op::CALL, 0, 1);
    m.op0(op::RETURN);

    // r0 n, r1 fib(n-1), r2..r4 call window
    let mut f = Asm::default();
    let l1 = f.label();
    f.lda_smi(2);
    f.jump_r16(op::TEST_LT_JF, 0, l1);
    f.r(op::LDAR, 0);
    f.op0(op::RETURN);
    f.bind(l1);
    f.lda_global_star(2, fibn);
    f.r_imm(op::SUB_SMI, 0, 1);
    f.r(op::STAR, 4);
    f.rn(op::CALL, 2, 1);
    f.r(op::STAR, 1);
    f.lda_global_star(2, fibn);
    f.r_imm(op::SUB_SMI, 0, 2);
    f.r(op::STAR, 4);
    f.rn(op::CALL, 2, 1);
    f.r(op::ADD, 1);
    f.op0(op::RETURN);
    vec![
        m.finish("<module>", 3, 0, 0, 0),
        f.finish("fib", 5, 1, 0, 0),
    ]
}

/// `for <target> in range(<n>)` header: callee r, stop const, FOR_PREP.
fn range_head(a: &mut Asm, names: &mut Names, r: u8, stop: i64, exit: code::Label) {
    let range = names.get("range");
    a.lda_global_star(r, range);
    a.lda_const(Value::int(stop));
    a.r(op::STAR, r + 2);
    a.for_prep(r, 1, exit);
}

// loop.py: module level, `total` and `i` are globals.
//   per iteration (engine): FOR_ITER; STA_GLOBAL i; LDA_GLOBAL_STAR total (+STAR);
//   LDA_GLOBAL i; INPLACE +; STA_GLOBAL total; JUMP_LOOP
fn loop_(n: &mut Names) -> Vec<Unit> {
    let (total, i) = (n.get("total"), n.get("i"));
    let mut m = Asm::default();
    let (body, exit) = (m.label(), m.label());
    m.lda_smi(0);
    m.sta_global(total);
    range_head(&mut m, n, 0, 3_000_000, exit);
    m.bind(body);
    m.sta_global(i);
    m.lda_global_star(3, total);
    m.lda_global(i);
    m.r(op::INPLACE_ADD, 3);
    m.sta_global(total);
    m.jump_r16(op::FOR_LOOP, 0, body);
    m.bind(exit);
    m.lda_global(total);
    m.op0(op::RETURN);
    vec![m.finish("<module>", 4, 0, 0, 0)]
}

// closure.py
//   per iteration (engine): FOR_ITER; STA_GLOBAL i; LDA_GLOBAL_STAR add (+STAR);
//   LDA_GLOBAL_STAR total (+STAR); CALL; STA_GLOBAL total; JUMP_LOOP
//   add: LDA_DEREF; ADD 0; RETURN
fn closure(n: &mut Names) -> Vec<Unit> {
    let (make_adder, add, total, i) = (
        n.get("make_adder"),
        n.get("add"),
        n.get("total"),
        n.get("i"),
    );
    let mut m = Asm::default();
    let (body, exit) = (m.label(), m.label());
    m.raw(op::MAKE_FUNCTION, 0, 1);
    m.sta_global(make_adder);
    m.lda_global_star(0, make_adder);
    m.lda_smi(3);
    m.r(op::STAR, 2);
    m.rn(op::CALL, 0, 1);
    m.sta_global(add);
    m.lda_smi(0);
    m.sta_global(total);
    range_head(&mut m, n, 0, 1_000_000, exit);
    m.bind(body);
    m.sta_global(i);
    m.lda_global_star(3, add);
    m.lda_global_star(5, total);
    m.rn(op::CALL, 3, 1);
    m.sta_global(total);
    m.jump_r16(op::FOR_LOOP, 0, body);
    m.bind(exit);
    m.lda_global(total);
    m.op0(op::RETURN);

    // make_adder(n): cell n, return a function over it.
    let mut mk = Asm::default();
    mk.r(op::MAKE_CELL, 0);
    mk.raw(op::MAKE_FUNCTION, code::phys(0), 2);
    mk.op0(op::RETURN);

    let mut ad = Asm::default();
    ad.deref(op::LDA_DEREF, 0);
    ad.r(op::ADD, 0);
    ad.op0(op::RETURN);
    vec![
        m.finish("<module>", 6, 0, 0, 0),
        mk.finish("make_adder", 1, 1, 0, 0),
        ad.finish("add", 1, 1, 0, 1),
    ]
}

// callfn.py: 1M calls of a global function from a function's loop.
//   main per iteration (engine): FOR_ITER_STAR (+STAR); LDA_GLOBAL_STAR inc (+STAR);
//   LDAR 0; STAR; CALL; STAR 0; JUMP_LOOP     inc: LDA_SMI_ADD 1 (+ADD 0); RETURN
fn callfn(n: &mut Names) -> Vec<Unit> {
    let (inc, main) = (n.get("inc"), n.get("main"));
    let mut m = Asm::default();
    m.raw(op::MAKE_FUNCTION, 0, 1);
    m.sta_global(inc);
    m.raw(op::MAKE_FUNCTION, 0, 2);
    m.sta_global(main);
    m.lda_global_star(0, main);
    m.rn(op::CALL, 0, 0);
    m.op0(op::RETURN);

    let mut f = Asm::default();
    f.r_imm(op::ADD_SMI, 0, 1);
    f.op0(op::RETURN);

    // r0 total, r1 i, r2..r4 range, r5..r7 call window
    let mut mn = Asm::default();
    let (body, exit) = (mn.label(), mn.label());
    mn.lda_smi(0);
    mn.r(op::STAR, 0);
    range_head(&mut mn, n, 2, 1_000_000, exit);
    mn.bind(body);
    mn.r(op::STAR, 1);
    mn.lda_global_star(5, inc);
    mn.r(op::LDAR, 0);
    mn.r(op::STAR, 7);
    mn.rn(op::CALL, 5, 1);
    mn.r(op::STAR, 0);
    mn.jump_r16(op::FOR_LOOP, 2, body);
    mn.bind(exit);
    mn.r(op::LDAR, 0);
    mn.op0(op::RETURN);
    vec![
        m.finish("<module>", 3, 0, 0, 0),
        f.finish("inc", 1, 1, 0, 0),
        mn.finish("main", 8, 0, 2, 0),
    ]
}

// dictint.py: 200k int keys inserted, then looked up; module level.
//   insert (engine): FOR_ITER; STA_GLOBAL i; LDA_GLOBAL_STAR i (+STAR);
//   LDA_GLOBAL_STAR d (+STAR); LDA_GLOBAL_STAR i (+STAR); LDAR; SET_ITEM; JUMP_LOOP
//   lookup: FOR_ITER; STA_GLOBAL i; LDA_GLOBAL_STAR total (+STAR);
//   LDA_GLOBAL_STAR d (+STAR); LDA_GLOBAL i; GET_ITEM; INPLACE +; STA_GLOBAL total; JUMP_LOOP
fn dictint(n: &mut Names) -> Vec<Unit> {
    let (d, i, total, len, nn) = (
        n.get("d"),
        n.get("i"),
        n.get("total"),
        n.get("len"),
        n.get("n"),
    );
    let mut m = Asm::default();
    let (b1, e1, b2, e2) = (m.label(), m.label(), m.label(), m.label());
    m.op0(op::BUILD_DICT);
    m.sta_global(d);
    range_head(&mut m, n, 0, 200_000, e1);
    m.bind(b1);
    m.sta_global(i);
    m.lda_global_star(5, i);
    m.lda_global_star(3, d);
    m.lda_global_star(4, i);
    m.r(op::LDAR, 5);
    m.rr(op::SET_ITEM, 3, 4);
    m.jump_r16(op::FOR_LOOP, 0, b1);
    m.bind(e1);
    m.lda_smi(0);
    m.sta_global(total);
    range_head(&mut m, n, 0, 200_000, e2);
    m.bind(b2);
    m.sta_global(i);
    m.lda_global_star(3, total);
    m.lda_global_star(4, d);
    m.lda_global(i);
    m.r(op::GET_ITEM, 4);
    m.r(op::INPLACE_ADD, 3);
    m.sta_global(total);
    m.jump_r16(op::FOR_LOOP, 0, b2);
    m.bind(e2);
    m.lda_global_star(0, len);
    m.lda_global_star(2, d);
    m.rn(op::CALL, 0, 1);
    m.sta_global(nn);
    m.lda_global(total);
    m.op0(op::RETURN);
    vec![m.finish("<module>", 6, 0, 0, 0)]
}

// loopfn.py: callfn's loop with the call replaced by its body.
//   main per iteration (engine): FOR_ITER_STAR (+STAR); LDA_SMI_ADD 1 (+ADD 0); STAR 0; JUMP_LOOP
fn loopfn(n: &mut Names) -> Vec<Unit> {
    let main = n.get("main");
    let mut m = Asm::default();
    m.raw(op::MAKE_FUNCTION, 0, 1);
    m.sta_global(main);
    m.lda_global_star(0, main);
    m.rn(op::CALL, 0, 0);
    m.op0(op::RETURN);
    let mut mn = Asm::default();
    let (body, exit) = (mn.label(), mn.label());
    mn.lda_smi(0);
    mn.r(op::STAR, 0);
    range_head(&mut mn, n, 2, 1_000_000, exit);
    mn.bind(body);
    mn.r(op::STAR, 1);
    mn.r_imm(op::ADD_SMI, 0, 1);
    mn.r(op::STAR, 0);
    mn.jump_r16(op::FOR_LOOP, 2, body);
    mn.bind(exit);
    mn.r(op::LDAR, 0);
    mn.op0(op::RETURN);
    vec![
        m.finish("<module>", 3, 0, 0, 0),
        mn.finish("main", 5, 0, 2, 0),
    ]
}

fn empty(_: &mut Names) -> Vec<Unit> {
    let mut m = Asm::default();
    m.op0(op::LDA_NONE);
    m.op0(op::RETURN);
    vec![m.finish("<module>", 0, 0, 0, 0)]
}

pub const KERNELS: &[Kernel] = &[
    Kernel {
        name: "fib",
        expected: "196418",
        build: fib,
        show: int,
    },
    Kernel {
        name: "loop",
        expected: "4499998500000",
        build: loop_,
        show: int,
    },
    Kernel {
        name: "closure",
        expected: "3000000",
        build: closure,
        show: int,
    },
    Kernel {
        name: "callfn",
        expected: "1000000",
        build: callfn,
        show: int,
    },
    Kernel {
        name: "dictint",
        expected: "200000 19999900000",
        build: dictint,
        show: dict_show,
    },
    Kernel {
        name: "loopfn",
        expected: "1000000",
        build: loopfn,
        show: int,
    },
    Kernel {
        name: "empty",
        expected: "None",
        build: empty,
        show: |_, v| {
            if v == Value::NONE {
                "None".into()
            } else {
                format!("{v:?}")
            }
        },
    },
];

// ------------------------------------------------- semantic test programs

/// `def f(n): return f(n + 1)` inside `try: ... except RecursionError: -1`.
pub fn t_recursion(n: &mut Names) -> Vec<Unit> {
    let f = n.get("f");
    let mut m = Asm::default();
    let (s, e, h, re) = (m.label(), m.label(), m.label(), m.label());
    m.raw(op::MAKE_FUNCTION, 0, 1);
    m.sta_global(f);
    m.bind(s);
    m.lda_global_star(0, f);
    m.lda_smi(0);
    m.r(op::STAR, 2);
    m.rn(op::CALL, 0, 1);
    m.op0(op::RETURN);
    m.bind(e);
    m.bind(h);
    m.raw(op::EXC_MATCH, exc::RECURSION as u32, 0);
    m.jump24(op::JUMP_IF_FALSE, re);
    m.lda_smi(-1);
    m.op0(op::RETURN);
    m.bind(re);
    m.raw(op::RAISE, 0, 0);
    m.try_range(s, e, h);

    let mut g = Asm::default();
    g.lda_global_star(1, f);
    g.r_imm(op::ADD_SMI, 0, 1);
    g.r(op::STAR, 3);
    g.rn(op::CALL, 1, 1);
    g.op0(op::RETURN);
    vec![m.finish("<module>", 3, 0, 0, 0), g.finish("f", 4, 1, 0, 0)]
}

/// `sum(range(0, 10, 1))` through the generic (iterator object) path,
/// and `range(5, 5)` (empty) through the fast path.
pub fn t_generic_range(n: &mut Names) -> Vec<Unit> {
    let (range, total) = (n.get("range"), n.get("total"));
    let mut m = Asm::default();
    let (b, x, b2, x2) = (m.label(), m.label(), m.label(), m.label());
    m.lda_smi(0);
    m.sta_global(total);
    m.lda_global_star(0, range);
    m.lda_smi(0);
    m.r(op::STAR, 2);
    m.lda_smi(10);
    m.r(op::STAR, 3);
    m.lda_smi(1);
    m.r(op::STAR, 4);
    m.for_prep(0, 3, x);
    m.bind(b);
    m.r(op::STAR, 5);
    m.lda_global_star(6, total);
    m.r(op::LDAR, 5);
    m.r(op::ADD, 6);
    m.sta_global(total);
    m.jump_r16(op::FOR_LOOP, 0, b);
    m.bind(x);
    m.lda_global_star(0, range);
    m.lda_smi(5);
    m.r(op::STAR, 2);
    m.r(op::STAR, 3);
    m.for_prep(0, 2, x2);
    m.bind(b2);
    m.lda_smi(1000);
    m.sta_global(total);
    m.jump_r16(op::FOR_LOOP, 0, b2);
    m.bind(x2);
    m.lda_global(total);
    m.op0(op::RETURN);
    vec![m.finish("<module>", 7, 0, 0, 0)]
}

/// An uncaught error from a slow path deep in the stack: i48 overflow
/// (`2**46 + 2**46 + ...`), raised three frames down.
pub fn t_uncaught(n: &mut Names) -> Vec<Unit> {
    let f = n.get("f");
    let mut m = Asm::default();
    m.raw(op::MAKE_FUNCTION, 0, 1);
    m.sta_global(f);
    m.lda_global_star(0, f);
    m.lda_const(Value::int(3));
    m.r(op::STAR, 2);
    m.rn(op::CALL, 0, 1);
    m.op0(op::RETURN);
    // f(k): if k < 1: return BIG + BIG; return f(k - 1)
    let mut g = Asm::default();
    let l = g.label();
    g.lda_smi(1);
    g.jump_r16(op::TEST_LT_JF, 0, l);
    g.lda_const(Value::int(1 << 46));
    g.r(op::STAR, 1);
    g.r(op::ADD, 1);
    g.op0(op::RETURN);
    g.bind(l);
    g.lda_global_star(2, f);
    g.r_imm(op::SUB_SMI, 0, 1);
    g.r(op::STAR, 4);
    g.rn(op::CALL, 2, 1);
    g.op0(op::RETURN);
    vec![m.finish("<module>", 3, 0, 0, 0), g.finish("f", 5, 1, 0, 0)]
}
