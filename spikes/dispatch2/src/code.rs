//! Bytecode format, the flat code table and an assembler.
//!
//! Instructions are fixed 32-bit words: `op:8 | a:8 | b:8 | c:8`, or
//! `op:8 | a:8 | imm16`, or `op:8 | imm24`. One bounds check fetches a whole
//! instruction. Register operands are physical window indices: window slot 0
//! is the frame's function, slot 1 its link word, Python registers start at
//! slot 2. The assembler takes logical registers and adds the 2.
//!
//! The opcode set and the fused forms follow mm-engine's compiler output for
//! the bench programs (`LDA_GLOBAL_STAR`, `LDA_SMI_SUB`, `LDA_SMI_ADD`,
//! `TEST_LT_JF` as in `mm-compile` `bytecode::fuse`); what changes is the
//! call window (callee, link, then arguments) and the range loop
//! (`FOR_PREP` / `FOR_LOOP`, counters in registers).

use crate::value::Value;

pub mod op {
    pub const LDAR: u8 = 0;
    pub const NOP: u8 = 1;
    pub const STAR: u8 = 2;
    pub const MOV: u8 = 3;
    /// `acc = imm24` (signed).
    pub const LDA_SMI: u8 = 4;
    /// `acc = consts[imm16]`.
    pub const LDA_CONST: u8 = 5;
    pub const LDA_NONE: u8 = 6;
    pub const LDA_TRUE: u8 = 7;
    pub const LDA_FALSE: u8 = 8;
    /// `acc = R[a]`, `UnboundLocalError` if unbound.
    pub const LDA_LOCAL: u8 = 9;
    /// `acc = global` through IC `imm24`.
    pub const LDA_GLOBAL: u8 = 10;
    /// `acc = R[a] = global` through IC `imm16` (engine `LDA_GLOBAL_STAR` + `STAR`).
    pub const LDA_GLOBAL_STAR: u8 = 11;
    /// `global = acc` through IC `imm24`.
    pub const STA_GLOBAL: u8 = 12;
    /// `acc = cell a of the running function`.
    pub const LDA_DEREF: u8 = 13;
    /// `acc = R[a] + acc`.
    pub const ADD: u8 = 14;
    /// `acc = R[a] - acc`.
    pub const SUB: u8 = 15;
    /// `acc = R[a] + imm16` (engine `LDA_SMI_ADD` + `ADD`).
    pub const ADD_SMI: u8 = 16;
    /// `acc = R[a] - imm16` (engine `LDA_SMI_SUB` + `SUB`).
    pub const SUB_SMI: u8 = 17;
    /// `acc = R[a] += acc` (int fast path as `ADD`).
    pub const INPLACE_ADD: u8 = 18;
    /// `acc = R[a] < acc`.
    pub const TEST_LT: u8 = 19;
    /// `acc = R[a] < acc`; jump to `imm16` if false (engine `TEST_LT_JF` + `JUMP_IF_FALSE`).
    pub const TEST_LT_JF: u8 = 20;
    pub const JUMP: u8 = 21;
    pub const JUMP_IF_FALSE: u8 = 22;
    pub const JUMP_IF_TRUE: u8 = 23;
    /// Backward jump to `imm24`; a safepoint.
    pub const JUMP_LOOP: u8 = 24;
    /// `for _ in R[a](R[a+2], ...)` with `b` arguments; next word: exit target.
    /// Builtin `range` with int arguments: `R[a]` = counter, `R[a+1]` = stop,
    /// `acc` = first value. Otherwise `R[a]` = iterator, `R[a+1]` = None.
    pub const FOR_PREP: u8 = 25;
    /// Loop tail: next value into `acc` and jump back to `imm16`, or fall
    /// through when exhausted. A safepoint (one tick per execution, as the
    /// engine's `JUMP_LOOP`).
    pub const FOR_LOOP: u8 = 26;
    /// `acc = R[a](R[a+2], ..., R[a+1+b])`; `R[a+1]` receives the link.
    pub const CALL: u8 = 27;
    pub const RETURN: u8 = 28;
    /// `acc = R[a][acc]`.
    pub const GET_ITEM: u8 = 29;
    /// `R[a][R[b]] = acc`.
    pub const SET_ITEM: u8 = 30;
    // ---- hot with feature `full_hot` (default): the rest of a realistic
    // hot set, so the loop's register allocation sees a fuller match ----
    pub const TEST_NE: u8 = 31;
    pub const TEST_LE: u8 = 32;
    pub const TEST_GT: u8 = 33;
    pub const TEST_GE: u8 = 34;
    pub const TEST_LE_JF: u8 = 35;
    pub const TEST_GT_JF: u8 = 36;
    pub const TEST_GE_JF: u8 = 37;
    /// `acc = acc.<IC imm16>` (instance shape → slot).
    pub const GET_ATTR: u8 = 38;
    /// `R[a].<IC imm16> = acc`.
    pub const SET_ATTR: u8 = 39;
    /// `R[a], R[a+2] = method, acc` for `acc.name(...)` (class shape IC).
    pub const LOAD_METHOD: u8 = 48;
    pub const NEG: u8 = 49;
    pub const NOT: u8 = 50;
    /// `R[a].append(acc)`.
    pub const LIST_APPEND: u8 = 51;
    /// `acc = R[a] is acc`.
    pub const TEST_IS: u8 = 52;
    // ---- cold: executed by `Vm::cold_op`, out of line (MUL, TEST_EQ and
    // STA_DEREF are hot with `full_hot`) ----
    /// `acc = R[a] * acc`.
    pub const MUL: u8 = 40;
    /// `acc = R[a] == acc`.
    pub const TEST_EQ: u8 = 41;
    /// `R[a] = cell(R[a])`.
    pub const MAKE_CELL: u8 = 42;
    /// `acc = function(code imm16, cells R[a..])`.
    pub const MAKE_FUNCTION: u8 = 43;
    /// `raise` an exception of kind `a`.
    pub const RAISE: u8 = 44;
    /// `acc = acc is an exception of kind a`.
    pub const EXC_MATCH: u8 = 45;
    /// `acc = {}`.
    pub const BUILD_DICT: u8 = 46;
    /// `cell a of the running function = acc`.
    pub const STA_DEREF: u8 = 47;
}

/// An exception-table entry: `[start, end)` → `handler` (word pcs).
#[derive(Clone, Copy, Debug)]
pub struct ExcEntry {
    pub start: u32,
    pub end: u32,
    pub handler: u32,
}

/// One entry of the flat code table: the fields the hot loop reads, decoded
/// once (no `Rc`, no indirection beyond the table itself).
#[derive(Debug)]
pub struct CodeEntry {
    pub code: Box<[u32]>,
    pub consts: Box<[Value]>,
    /// Window size: 2 header slots + registers.
    pub win: u32,
    /// Positional parameters (simple signatures only; 0xFF = not simple).
    pub argc: u8,
    /// Named locals after the parameters, set to unbound at entry.
    pub nclear: u8,
    /// Closure cells the function object carries inline.
    pub ncells: u8,
    pub ic_base: u32,
    pub exc: Box<[ExcEntry]>,
}

/// Inline-cache entry (side table, never snapshotted, empty after restore).
#[derive(Clone, Copy, Debug)]
pub struct Ic {
    /// Globals version this entry is valid for; 0 = empty.
    pub version: u64,
    /// Arena address of the global's value word (in the globals dict's
    /// entries buffer), or 0 for a builtin; also the attribute slot index
    /// for attribute ICs.
    pub slot: u32,
    /// The builtin's value when `entries == 0`.
    pub value: Value,
    /// The global name (interned str), fixed at compile time.
    pub name: Value,
}

/// Physical register for logical register `r`.
pub const fn phys(r: u8) -> u32 {
    r as u32 + 2
}

#[derive(Default)]
pub struct Asm {
    pub words: Vec<u32>,
    labels: Vec<Option<u32>>,
    /// (word index, label, kind): 16 = imm16 field, 24 = imm24 field, 32 = whole word.
    fixups: Vec<(usize, usize, u8)>,
    pub ic_names: Vec<Value>,
    pub consts: Vec<Value>,
    pub exc: Vec<(usize, usize, usize)>,
}

pub type Label = usize;

impl Asm {
    pub fn label(&mut self) -> Label {
        self.labels.push(None);
        self.labels.len() - 1
    }
    pub fn bind(&mut self, l: Label) {
        self.labels[l] = Some(self.words.len() as u32);
    }
    fn w(&mut self, op: u8, a: u32, rest: u32) {
        self.words.push(op as u32 | a << 8 | rest << 16);
    }
    fn ic(&mut self, name: Value) -> u32 {
        self.ic_names.push(name);
        (self.ic_names.len() - 1) as u32
    }
    pub fn op0(&mut self, op: u8) {
        self.w(op, 0, 0);
    }
    /// One logical register operand.
    pub fn r(&mut self, op: u8, r: u8) {
        self.w(op, phys(r), 0);
    }
    pub fn lda_smi(&mut self, i: i32) {
        self.words.push(op::LDA_SMI as u32 | (i as u32) << 8);
    }
    pub fn lda_const(&mut self, v: Value) {
        self.consts.push(v);
        let k = (self.consts.len() - 1) as u32;
        self.w(op::LDA_CONST, 0, k);
    }
    pub fn lda_global(&mut self, name: Value) {
        let k = self.ic(name);
        self.words.push(op::LDA_GLOBAL as u32 | k << 8);
    }
    pub fn lda_global_star(&mut self, r: u8, name: Value) {
        let k = self.ic(name);
        self.w(op::LDA_GLOBAL_STAR, phys(r), k);
    }
    pub fn sta_global(&mut self, name: Value) {
        let k = self.ic(name);
        self.words.push(op::STA_GLOBAL as u32 | k << 8);
    }
    /// Cell index operand (not a register).
    pub fn deref(&mut self, op: u8, k: u8) {
        self.w(op, k as u32, 0);
    }
    pub fn r_imm(&mut self, op: u8, r: u8, imm: i16) {
        self.w(op, phys(r), imm as u16 as u32);
    }
    pub fn rr(&mut self, op: u8, a: u8, b: u8) {
        self.w(op, phys(a), phys(b));
    }
    /// `CALL` / `FOR_PREP` style: window register and a count.
    pub fn rn(&mut self, op: u8, r: u8, n: u8) {
        self.w(op, phys(r), n as u32);
    }
    pub fn raw(&mut self, op: u8, a: u32, rest: u32) {
        self.w(op, a, rest);
    }
    pub fn jump24(&mut self, op: u8, l: Label) {
        self.fixups.push((self.words.len(), l, 24));
        self.words.push(op as u32);
    }
    pub fn jump_r16(&mut self, op: u8, r: u8, l: Label) {
        self.fixups.push((self.words.len(), l, 16));
        self.w(op, phys(r), 0);
    }
    /// `FOR_PREP r n` plus its exit word.
    pub fn for_prep(&mut self, r: u8, n: u8, exit: Label) {
        self.rn(op::FOR_PREP, r, n);
        self.fixups.push((self.words.len(), exit, 32));
        self.words.push(0);
    }
    pub fn try_range(&mut self, start: Label, end: Label, handler: Label) {
        self.exc.push((start, end, handler));
    }

    /// Resolves labels and builds the code entry (`ic_base` set by the program).
    pub fn finish(
        mut self,
        _name: &'static str,
        nregs: u8,
        argc: u8,
        nclear: u8,
        ncells: u8,
    ) -> (CodeEntry, Vec<Value>) {
        let at = |labels: &Vec<Option<u32>>, l: usize| labels[l].unwrap_or(0);
        for &(i, l, kind) in &self.fixups {
            let t = at(&self.labels, l);
            match kind {
                16 => self.words[i] = (self.words[i] & 0xFFFF) | t << 16,
                24 => self.words[i] = (self.words[i] & 0xFF) | t << 8,
                _ => self.words[i] = t,
            }
        }
        let exc = self
            .exc
            .iter()
            .map(|&(s, e, h)| ExcEntry {
                start: at(&self.labels, s),
                end: at(&self.labels, e),
                handler: at(&self.labels, h),
            })
            .collect();
        (
            CodeEntry {
                code: self.words.into_boxed_slice(),
                consts: self.consts.into_boxed_slice(),
                win: 2 + nregs as u32,
                argc,
                nclear,
                ncells,
                ic_base: 0,
                exc,
            },
            self.ic_names,
        )
    }
}

/// Builds the table: assigns each code its IC range.
pub fn table(units: Vec<(CodeEntry, Vec<Value>)>) -> (Vec<CodeEntry>, Vec<Ic>) {
    let mut codes = Vec::new();
    let mut ics = Vec::new();
    for (mut c, names) in units {
        c.ic_base = ics.len() as u32;
        for name in names {
            ics.push(Ic {
                version: 0,
                slot: 0,
                value: Value::NONE,
                name,
            });
        }
        codes.push(c);
    }
    (codes, ics)
}
