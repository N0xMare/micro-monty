//! Shared VM scaffolding: programs, the assembler, VM registers, frame
//! push/pop, and the macro that builds both dispatch mechanisms (`match` and
//! function-pointer table) from one list of handlers per design.

use crate::access::st;
use crate::heap::{Fail, Heap, Name, Rec};
use crate::rt;
use crate::value::Value;

/// One code object.
pub struct Code {
    pub name: &'static str,
    pub bytes: Vec<u8>,
    pub nparams: usize,
    /// Named locals (params first); the rest of the frame is temporaries.
    pub nlocals: usize,
    /// Words this frame may use above its base (locals + registers/operand stack).
    pub frame_size: usize,
    /// Static instruction count (from the assembler).
    pub ninstr: usize,
}

/// A kernel's code objects plus the program-wide constant pool and IC table.
pub struct Program {
    pub codes: Vec<Code>,
    pub consts: Vec<Value>,
    pub ic_names: Vec<Name>,
    pub entry: usize,
}

impl Program {
    pub fn code(&self, name: &str) -> usize {
        self.codes
            .iter()
            .position(|c| c.name == name)
            .expect("no such code object")
    }
    pub fn bytecode_bytes(&self) -> usize {
        self.codes.iter().map(|c| c.bytes.len()).sum()
    }
    pub fn static_instructions(&self) -> usize {
        self.codes.iter().map(|c| c.ninstr).sum()
    }
}

#[derive(Default)]
pub struct ProgBuilder {
    codes: Vec<Code>,
    consts: Vec<Value>,
    ic_names: Vec<Name>,
}

impl ProgBuilder {
    /// A fresh inline-cache entry for one instruction site.
    pub fn ic(&mut self, name: Name) -> u8 {
        self.ic_names.push(name);
        u8::try_from(self.ic_names.len() - 1).expect("too many ICs")
    }
    pub fn konst(&mut self, i: i64) -> u8 {
        self.consts.push(Value::int(i));
        u8::try_from(self.consts.len() - 1).expect("too many constants")
    }
    pub fn add(
        &mut self,
        name: &'static str,
        nparams: usize,
        nlocals: usize,
        frame_size: usize,
        asm: Asm,
    ) -> u8 {
        let (bytes, ninstr) = asm.finish();
        self.codes.push(Code {
            name,
            bytes,
            nparams,
            nlocals,
            frame_size,
            ninstr,
        });
        u8::try_from(self.codes.len() - 1).expect("too many codes")
    }
    pub fn finish(self, entry: u8) -> Program {
        Program {
            codes: self.codes,
            consts: self.consts,
            ic_names: self.ic_names,
            entry: entry as usize,
        }
    }
}

#[derive(Clone, Copy)]
pub struct Label(usize);

/// Tiny assembler shared by the three designs: 1-byte opcodes, u8 operands,
/// u16 little-endian absolute jump targets. `lens` is the design's opcode →
/// instruction-length table; every instruction is checked against it.
pub struct Asm {
    bytes: Vec<u8>,
    lens: &'static [u8; 256],
    cur: Option<(usize, u8)>,
    labels: Vec<Option<usize>>,
    fixups: Vec<(usize, usize)>,
    ninstr: usize,
}

impl Asm {
    pub fn new(lens: &'static [u8; 256]) -> Asm {
        Asm {
            bytes: Vec::new(),
            lens,
            cur: None,
            labels: Vec::new(),
            fixups: Vec::new(),
            ninstr: 0,
        }
    }
    fn check(&self) {
        if let Some((start, op)) = self.cur {
            let len = self.lens[op as usize] as usize;
            assert!(len != 0, "opcode {op} has no length");
            assert_eq!(
                self.bytes.len() - start,
                len,
                "operand bytes of opcode {op}"
            );
        }
    }
    pub fn op(&mut self, op: u8) -> &mut Asm {
        self.check();
        self.cur = Some((self.bytes.len(), op));
        self.bytes.push(op);
        self.ninstr += 1;
        self
    }
    /// u8 operand (local, register, constant, IC, code index, count).
    pub fn u(&mut self, x: u8) -> &mut Asm {
        self.bytes.push(x);
        self
    }
    /// i8 immediate.
    pub fn i(&mut self, x: i8) -> &mut Asm {
        self.bytes.push(x as u8);
        self
    }
    /// u16 jump target.
    pub fn to(&mut self, l: Label) -> &mut Asm {
        self.fixups.push((self.bytes.len(), l.0));
        self.bytes.extend([0, 0]);
        self
    }
    pub fn label(&mut self) -> Label {
        self.labels.push(None);
        Label(self.labels.len() - 1)
    }
    pub fn bind(&mut self, l: Label) {
        assert!(self.labels[l.0].is_none(), "label bound twice");
        self.labels[l.0] = Some(self.bytes.len());
    }
    /// A label bound here.
    pub fn here(&mut self) -> Label {
        let l = self.label();
        self.bind(l);
        l
    }
    fn finish(mut self) -> (Vec<u8>, usize) {
        self.check();
        for &(pos, l) in &self.fixups {
            let t = self.labels[l].expect("unbound label");
            let t = u16::try_from(t).expect("jump target out of range");
            self.bytes[pos..pos + 2].copy_from_slice(&t.to_le_bytes());
        }
        (self.bytes, self.ninstr)
    }
}

/// VM registers. In `match` dispatch this struct is a local of the dispatch
/// function (LLVM keeps the fields in machine registers); in function-table
/// dispatch it is passed by `&mut` to every handler and lives in memory.
pub struct Vm<'a> {
    pub prog: &'a Program,
    pub heap: &'a mut Heap,
    /// Contiguous frame stack (stands in for an arena segment).
    pub stack: &'a mut [Value],
    pub code: &'a [u8],
    pub code_idx: u32,
    /// The running function object (closure cells).
    pub func: Value,
    pub pc: usize,
    pub base: usize,
    /// Operand-stack pointer (stack design only).
    pub sp: usize,
    /// Accumulator (accumulator design only).
    pub acc: Value,
    /// Result of the outermost return.
    pub ret: Value,
}

impl<'a> Vm<'a> {
    /// Entry frame for `prog.entry` at base 1 with `args` as its parameters.
    pub fn new(
        prog: &'a Program,
        heap: &'a mut Heap,
        stack: &'a mut [Value],
        args: &[Value],
    ) -> Result<Vm<'a>, Fail> {
        let code = &prog.codes[prog.entry];
        let base = 1;
        if args.len() != code.nparams {
            return Err(Fail::Arity);
        }
        if base + code.frame_size > stack.len() {
            return Err(Fail::Recursion);
        }
        for j in 0..code.nlocals {
            stack[base + j] = args.get(j).copied().unwrap_or(Value::UNDEF);
        }
        Ok(Vm {
            prog,
            heap,
            stack,
            code: &code.bytes,
            code_idx: prog.entry as u32,
            func: Value::NONE,
            pc: 0,
            base,
            sp: base + code.nlocals,
            acc: Value::UNDEF,
            ret: Value::NONE,
        })
    }
}

/// What a handler tells the dispatch loop.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Flow {
    Continue,
    Done,
    Err(Fail),
}

/// Function-table handler.
pub type Handler = fn(&mut Vm<'_>) -> Flow;

pub fn bad_op(_: &mut Vm<'_>) -> Flow {
    Flow::Err(Fail::BadOp)
}

/// Python→Python call: new frame at `new_base` (arguments already there or
/// written by the caller right after), same dispatch loop, no Rust recursion.
/// Shared by all designs; only where the arguments live differs.
#[inline(always)]
pub fn push_frame<'a>(
    vm: &mut Vm<'a>,
    func: Value,
    ci: usize,
    new_base: usize,
    nargs: usize,
    ret_pc: usize,
) -> Result<&'a Code, Fail> {
    let prog = vm.prog;
    let code = &prog.codes[ci];
    if nargs != code.nparams {
        return Err(Fail::Arity);
    }
    if new_base + code.frame_size > vm.stack.len() {
        return Err(Fail::Recursion);
    }
    for j in nargs..code.nlocals {
        st(vm.stack, new_base + j, Value::UNDEF);
    }
    vm.heap.ctrl.push(Rec {
        pc: ret_pc as u32,
        base: vm.base as u32,
        code: vm.code_idx,
        func: vm.func,
    });
    rt::tick(vm.heap, ret_pc);
    vm.code = &code.bytes;
    vm.code_idx = ci as u32;
    vm.func = func;
    vm.base = new_base;
    vm.pc = 0;
    Ok(code)
}

/// Return to the caller; `false` if this was the outermost frame.
#[inline(always)]
pub fn pop_frame(vm: &mut Vm<'_>) -> bool {
    match vm.heap.ctrl.pop() {
        None => false,
        Some(r) => {
            let prog = vm.prog;
            vm.pc = r.pc as usize;
            vm.base = r.base as usize;
            vm.code_idx = r.code;
            vm.code = &prog.codes[r.code as usize].bytes;
            vm.func = r.func;
            true
        }
    }
}

/// `?` for handlers: turn an `Err` into `Flow::Err`.
macro_rules! tri {
    ($e:expr) => {
        match $e {
            Ok(v) => v,
            Err(e) => return $crate::vm::Flow::Err(e),
        }
    };
}

/// Build `run_match` and `run_fntable` for a design from one list of
/// `OPCODE => handler`. Handlers are `#[inline(always)] fn(&mut Vm) -> Flow`:
/// the match loop inlines them into one function; the table stores them as
/// out-of-line functions. Expects a module `op` with the opcode constants.
macro_rules! dispatchers {
    ($($op:ident => $h:ident),* $(,)?) => {
        /// `loop { match op { ... } }` with the VM registers in locals.
        #[inline(never)]
        pub fn run_match(
            vm0: $crate::vm::Vm<'_>,
        ) -> Result<$crate::value::Value, $crate::heap::Fail> {
            // Rebuild the registers as a fresh local: the by-value argument
            // lives in caller memory, which LLVM would not promote to registers.
            let mut vm = $crate::vm::Vm {
                prog: vm0.prog,
                heap: vm0.heap,
                stack: vm0.stack,
                code: vm0.code,
                code_idx: vm0.code_idx,
                func: vm0.func,
                pc: vm0.pc,
                base: vm0.base,
                sp: vm0.sp,
                acc: vm0.acc,
                ret: vm0.ret,
            };
            loop {
                #[cfg(feature = "count")]
                {
                    vm.heap.count += 1;
                }
                let flow = match $crate::access::rd8(vm.code, vm.pc) {
                    $(op::$op => $h(&mut vm),)*
                    _ => $crate::vm::Flow::Err($crate::heap::Fail::BadOp),
                };
                match flow {
                    $crate::vm::Flow::Continue => {}
                    $crate::vm::Flow::Done => return Ok(vm.ret),
                    $crate::vm::Flow::Err(e) => return Err(e),
                }
            }
        }

        /// Opcode → handler; unused opcodes map to `bad_op`.
        static TABLE: [$crate::vm::Handler; 256] = {
            let mut t = [$crate::vm::bad_op as $crate::vm::Handler; 256];
            $(t[op::$op as usize] = $h;)*
            t
        };

        /// Loop calling `TABLE[op](&mut vm)`.
        #[inline(never)]
        pub fn run_fntable(
            mut vm: $crate::vm::Vm<'_>,
        ) -> Result<$crate::value::Value, $crate::heap::Fail> {
            loop {
                #[cfg(feature = "count")]
                {
                    vm.heap.count += 1;
                }
                let h = TABLE[$crate::access::rd8(vm.code, vm.pc) as usize];
                match h(&mut vm) {
                    $crate::vm::Flow::Continue => {}
                    $crate::vm::Flow::Done => return Ok(vm.ret),
                    $crate::vm::Flow::Err(e) => return Err(e),
                }
            }
        }
    };
}
