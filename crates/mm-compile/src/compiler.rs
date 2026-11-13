//! AST to accumulator bytecode.
//!
//! Expressions leave their value in the accumulator. Locals live in the first
//! registers of a frame; temporaries are allocated stack-like above them.
//! `try`/`finally` compile to side-table handlers, with `finally` bodies
//! inlined on the normal, `return`, `break` and `continue` paths (CPython
//! 3.11+). Comprehensions are inlined (PEP 709); generator expressions are
//! generator functions. A class body is a function whose one parameter is
//! the namespace dict its names live in.

use crate::ast::{
    Anchor, BinOp, BoolOp, ClassDef, CmpOp, Comprehension, Const, Expr, ExprKind, FPart, FuncDef,
    Handler, Keyword, Module, Pos, Stmt, StmtKind, UnaryOp, WithItem,
};
use crate::bytecode::{Code, ExcEntry, Konst, ParamSpec, PosEntry, Program, RegName, op};
use crate::error::CompileError;
use crate::symtable::{self, Binding, ScopeId, ScopeKind, SymTable};
use alloc::{
    boxed::Box,
    collections::BTreeMap,
    format,
    string::{String, ToString},
    sync::Arc,
    vec,
    vec::Vec,
};

mod matching;

pub use matching::kind as match_kind;

/// Deepest expression or statement nesting the compiler accepts; deeper source
/// is a `RecursionError` at compile time. Keeps every recursive pass (lowering,
/// symbol table, code generation, `Drop`) well inside a 2 MiB stack.
pub const MAX_NESTING: u32 = 200;

/// Displays with more items than this are built item by item.
const LARGE_DISPLAY: usize = 32;

/// Compile options.
#[derive(Clone, Copy, Debug, Default)]
pub struct Options {
    /// Make the module body return the value of a final expression statement.
    pub last_expr_value: bool,
}

/// Compiles a module.
pub fn compile(module: &Module, opts: Options) -> Result<Program, CompileError> {
    let st = symtable::build(module)?;
    let mut c = Compiler {
        st: &st,
        codes: Vec::new(),
        units: Vec::new(),
        depth: 0,
    };
    c.codes.push(None);
    c.begin_unit(0, "<module>", "<module>", 1, true)?;
    let mut returns_last = false;
    let n = module.body.len();
    for (i, s) in module.body.iter().enumerate() {
        if opts.last_expr_value
            && i + 1 == n
            && let StmtKind::Expr(e) = &s.kind
        {
            c.expr(e)?;
            c.at_pos(s.pos);
            c.emit(op::RETURN);
            returns_last = true;
        } else {
            c.stmt(s)?;
        }
    }
    if !returns_last {
        c.emit(op::LDA_NONE);
        c.emit(op::RETURN);
    }
    let code = c.end_unit()?;
    c.codes[0] = Some(code);
    let mut codes = Vec::with_capacity(c.codes.len());
    for code in c.codes {
        codes.push(Arc::new(code.unwrap_or_default()));
    }
    let mut warnings = Vec::new();
    finally_warnings(&module.body, false, false, 0, &mut warnings);
    Ok(Program {
        codes,
        entry: 0,
        returns_last_expr: returns_last,
        warnings,
    })
}

/// CPython 3.14's PEP 765 warnings: `return` in a `finally` block, and
/// `break`/`continue` that would leave one. Walks in source order; nesting
/// is bounded by [`MAX_NESTING`] (deeper statements were rejected anyway).
fn finally_warnings(
    body: &[Stmt],
    in_fin: bool,
    loop_in_fin: bool,
    depth: u32,
    out: &mut Vec<(Pos, String)>,
) {
    if depth > MAX_NESTING {
        return;
    }
    let d = depth + 1;
    for s in body {
        match &s.kind {
            StmtKind::Return(_) if in_fin => {
                out.push((s.pos, String::from("'return' in a 'finally' block")));
            }
            StmtKind::Break if in_fin && !loop_in_fin => {
                out.push((s.pos, String::from("'break' in a 'finally' block")));
            }
            StmtKind::Continue if in_fin && !loop_in_fin => {
                out.push((s.pos, String::from("'continue' in a 'finally' block")));
            }
            StmtKind::If(_, b, e) => {
                finally_warnings(b, in_fin, loop_in_fin, d, out);
                // An `elif` chain at this level, not nested.
                let mut e = e;
                while let [
                    Stmt {
                        kind: StmtKind::If(_, b, e2),
                        ..
                    },
                ] = e.as_slice()
                {
                    finally_warnings(b, in_fin, loop_in_fin, d, out);
                    e = e2;
                }
                finally_warnings(e, in_fin, loop_in_fin, d, out);
            }
            StmtKind::While(_, b, e) | StmtKind::For(_, _, b, e, _) => {
                finally_warnings(b, in_fin, in_fin, d, out);
                finally_warnings(e, in_fin, loop_in_fin, d, out);
            }
            StmtKind::Try {
                body,
                handlers,
                orelse,
                finalbody,
                ..
            } => {
                finally_warnings(body, in_fin, loop_in_fin, d, out);
                for h in handlers {
                    finally_warnings(&h.body, in_fin, loop_in_fin, d, out);
                }
                finally_warnings(orelse, in_fin, loop_in_fin, d, out);
                finally_warnings(finalbody, true, false, d, out);
            }
            StmtKind::FunctionDef(f) => finally_warnings(&f.body, false, false, d, out),
            StmtKind::ClassDef(c) => finally_warnings(&c.body, false, false, d, out),
            StmtKind::With(_, b, _) => finally_warnings(b, in_fin, loop_in_fin, d, out),
            StmtKind::Match(_, cases) => {
                for c in cases {
                    finally_warnings(&c.body, in_fin, loop_in_fin, d, out);
                }
            }
            _ => {}
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Label(usize);

enum FBlock<'a> {
    Loop {
        brk: Label,
        cont: Label,
    },
    Finally {
        body: &'a [Stmt],
        handlers: usize,
    },
    Handler {
        old: u8,
        handlers: usize,
        name: Option<&'a str>,
        /// An `except*` handler: no `break`, `continue` or `return` leaves it.
        star: bool,
    },
    With {
        exit: u8,
        handlers: usize,
        ctx: &'a Expr,
        is_async: bool,
    },
}

/// A comprehension clause's loop: head and exit labels, the clause, and
/// for `async for` its `StopAsyncIteration` handler.
type CompLoop<'a> = (Label, Label, &'a Comprehension, Option<Label>);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Access {
    Local(u8),
    Deref(u8),
    Global,
    /// A class-body name: the namespace dict, then globals and builtins.
    Name,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Loc {
    pos: Pos,
    anchor: Anchor,
    no_carets: bool,
}

struct Unit<'a> {
    scope: ScopeId,
    is_class: bool,
    code: Code,
    /// The layout of the instruction being emitted, its first word and
    /// how many of its operands are written.
    lay: crate::bytecode::Layout,
    at: usize,
    nops: usize,
    /// Registers of locals, one map per scope: the unit's own first, then
    /// the comprehensions inlined into it. Keyed by `String` alone, the maps
    /// share `BTreeMap<String, usize>`'s code with the symbol tables and
    /// the encoder.
    regs: Vec<(ScopeId, BTreeMap<String, usize>)>,
    next_reg: u32,
    max_reg: u32,
    labels: Vec<Option<u32>>,
    /// Jump operands to patch: word, bit shift, label.
    fixups: Vec<(usize, u8, usize)>,
    handlers: Vec<Label>,
    seg_start: u32,
    raw_exc: Vec<(u32, u32, Label, u32)>,
    fblocks: Vec<FBlock<'a>>,
    comps: Vec<ScopeId>,
    bound: Vec<u8>,
    block_depth: u32,
    str_consts: BTreeMap<String, usize>,
    int_consts: BTreeMap<i64, u16>,
    name_idx: BTreeMap<String, usize>,
    last_loc: Option<Loc>,
    cur: Loc,
    /// Instructions any path may have run since its last safepoint, at
    /// the emission point; `label_runs` holds the most of the forward
    /// jumps to each label (see [`CHECK_EVERY`]).
    run: u32,
    label_runs: Vec<u32>,
    carried: u32,
}

impl Unit<'_> {
    /// The register of local `name` of `scope`.
    fn reg(&self, scope: ScopeId, name: &str) -> Option<u8> {
        let regs = match self.regs.first() {
            Some((s, regs)) if *s == scope => regs,
            _ => &self.regs.iter().rev().find(|(s, _)| *s == scope)?.1,
        };
        u8::try_from(*regs.get(name)?).ok()
    }

    fn set_reg(&mut self, scope: ScopeId, name: &str, r: u8) {
        let i = self.regs.iter().rposition(|(s, _)| *s == scope);
        let i = i.unwrap_or_else(|| {
            self.regs.push((scope, BTreeMap::new()));
            self.regs.len() - 1
        });
        self.regs[i].1.insert(name.into(), usize::from(r));
    }
}

/// Straight-line instructions that may do native work in proportion to
/// their data (not moves, constants, branches or calls) a path runs at
/// most before a `CHECK`: each can be a long native operation (`s == t` over 10 MB strings), and
/// without a backward jump, call or return nothing else checks fuel, the
/// deadline or a park request (finding C4, docs/spec.md §8.6). A loop
/// head restarts the count (its back edge is a safepoint), so a path runs
/// fewer than `2 * CHECK_EVERY` such instructions between two checks, and
/// a loop body shorter than that runs none. A handler starts with the
/// most its block reached, as a raise can come from anywhere in it.
pub const CHECK_EVERY: u32 = 32;

struct Compiler<'a> {
    st: &'a SymTable,
    codes: Vec<Option<Code>>,
    units: Vec<Unit<'a>>,
    depth: u32,
}

const NO_LOC: Loc = Loc {
    pos: Pos {
        line: 0,
        col: 0,
        end_line: 0,
        end_col: 0,
    },
    anchor: Anchor {
        kind: 0,
        left_line: 0,
        left_col: 0,
        right_line: 0,
        right_col: 0,
    },
    no_carets: false,
};

type R<T> = Result<T, CompileError>;

fn too_many(what: &str) -> CompileError {
    CompileError::unsupported(what, Pos::default())
}

impl<'a> Compiler<'a> {
    // ----------------------------------------------------------- units

    fn u(&mut self) -> &mut Unit<'a> {
        let n = self.units.len() - 1;
        &mut self.units[n]
    }

    fn ur(&self) -> &Unit<'a> {
        &self.units[self.units.len() - 1]
    }

    fn begin_unit(
        &mut self,
        scope: ScopeId,
        name: &str,
        qualname: &str,
        first_line: u32,
        is_module: bool,
    ) -> R<()> {
        let sc = &self.st.scopes[scope];
        let is_class = sc.kind == ScopeKind::Class;
        let mut unit = Unit {
            scope,
            is_class,
            code: Code {
                name: name.into(),
                qualname: qualname.into(),
                first_line,
                is_module,
                ..Code::default()
            },
            lay: crate::bytecode::layout(""),
            at: 0,
            nops: 0,
            regs: Vec::new(),
            next_reg: 0,
            max_reg: 0,
            labels: Vec::new(),
            fixups: Vec::new(),
            handlers: Vec::new(),
            seg_start: 0,
            raw_exc: Vec::new(),
            fblocks: Vec::new(),
            comps: Vec::new(),
            bound: Vec::new(),
            block_depth: 0,
            str_consts: BTreeMap::new(),
            int_consts: BTreeMap::new(),
            name_idx: BTreeMap::new(),
            last_loc: None,
            cur: NO_LOC,
            run: 0,
            label_runs: Vec::new(),
            carried: 0,
        };
        if !is_module {
            // Variables: parameters first, then locals, cells and free variables.
            // A class body keeps its own names in the namespace dict: only
            // the namespace parameter, cells and free variables get registers.
            let regs_for: &[Binding] = if is_class {
                &[Binding::Cell, Binding::Free]
            } else {
                &[Binding::Local, Binding::Cell, Binding::Free]
            };
            for (i, n) in sc.names.iter().enumerate() {
                let b = sc.bindings[i];
                if regs_for.contains(&b) || i < sc.nparams {
                    let r = u8::try_from(unit.code.varnames.len())
                        .ok()
                        .filter(|&r| r < 250)
                        .ok_or_else(|| too_many("functions with more than 250 local variables"))?;
                    unit.set_reg(scope, n, r);
                    unit.code.varnames.push(n.as_str().into());
                    if b == Binding::Free {
                        unit.code.freevars.push(r);
                        unit.code.freenames.push(n.as_str().into());
                    }
                    if i < sc.nparams && !sc.is_deleted(n) {
                        unit.bound.push(r);
                    }
                }
            }
            for n in &sc.class_free {
                let r = u8::try_from(unit.code.varnames.len())
                    .ok()
                    .filter(|&r| r < 250)
                    .ok_or_else(|| too_many("functions with more than 250 local variables"))?;
                unit.set_reg(scope, n, r);
                unit.code.varnames.push(n.as_str().into());
                unit.code.freevars.push(r);
                unit.code.freenames.push(n.as_str().into());
            }
            let nvars = u32::try_from(unit.code.varnames.len()).unwrap_or(250);
            unit.next_reg = nvars;
            unit.max_reg = nvars;
        }
        self.units.push(unit);
        if !is_module {
            // Cells for captured locals (parameters keep their value).
            let cells: Vec<u8> = sc
                .names
                .iter()
                .enumerate()
                .filter(|(i, _)| sc.bindings[*i] == Binding::Cell)
                .filter_map(|(_, n)| self.ur().reg(scope, n))
                .collect();
            for r in cells {
                self.emit_r(op::MAKE_CELL, r);
            }
        }
        Ok(())
    }

    fn end_unit(&mut self) -> R<Code> {
        self.set_handlers();
        let Some(mut u) = self.units.pop() else {
            return Err(too_many("an empty compiler stack"));
        };
        if u.code.bytecode.len() >= crate::bytecode::MAX_WORDS {
            return Err(too_many("functions this large"));
        }
        for &(at, shift, l) in &u.fixups {
            u.code.bytecode[at] |= u.labels[l].unwrap_or(0) << shift;
        }
        for &(start, end, l, depth) in &u.raw_exc {
            if start < end {
                u.code.exc_table.push(ExcEntry {
                    start,
                    end,
                    handler: u.labels[l.0].unwrap_or(0),
                    depth,
                });
            }
        }
        u.code.nregs = u.max_reg;
        crate::bytecode::fuse(&mut u.code.bytecode);
        Ok(u.code)
    }

    // ------------------------------------------------------- emission

    fn pc(&self) -> u32 {
        u32::try_from(self.ur().code.bytecode.len()).unwrap_or(u32::MAX)
    }

    fn at(&mut self, e: &Expr) {
        self.u().cur = Loc {
            pos: e.pos,
            anchor: e.anchor,
            no_carets: e.no_carets,
        };
    }

    fn at_pos(&mut self, pos: Pos) {
        self.u().cur = Loc { pos, ..NO_LOC };
    }

    #[inline(never)]
    fn emit(&mut self, opcode: u8) {
        // `SEND` and its `YIELD_FROM` stay adjacent (the VM finds one from
        // the other).
        if self.ur().run >= CHECK_EVERY && opcode != op::YIELD_FROM {
            self.u().run = 0;
            self.emit_one(op::CHECK);
        }
        self.emit_one(opcode);
        let u = self.u();
        if u.run > 0 {
            // A raise from here reaches the active handlers with this count
            // (counting this instruction).
            for &h in &u.handlers {
                u.label_runs[h.0] = u.label_runs[h.0].max(u.run + 1);
            }
        }
        let after = match opcode {
            // A call ticks once it returns, a safepoint.
            op::CALL | op::CALL_METHOD | op::CALL_KW | op::CALL_METHOD_KW | op::CALL_EX => 0,
            // Register, constant and name moves, identity tests and
            // branches do no work in proportion to data.
            op::NOP
            | op::LDAR
            | op::STAR
            | op::MOV
            | op::LDA_LOCAL
            | op::LDA_SMI
            | op::LDA_CONST
            | op::LDA_NONE
            | op::LDA_TRUE
            | op::LDA_FALSE
            | op::LDA_GLOBAL
            | op::STA_GLOBAL
            | op::LDA_DEREF
            | op::STA_DEREF
            | op::MAKE_CELL
            | op::DEL_LOCAL
            | op::NOT
            | op::TEST_IS
            | op::TEST_IS_NOT
            | op::JUMP
            | op::JUMP_IF_TRUE
            | op::JUMP_IF_FALSE
            | op::LDA_NAME
            | op::STA_NAME
            | op::PUSH_EXC
            | op::POP_EXC => u.run,
            _ => u.run + 1,
        };
        // What a jump operand of this instruction carries to its label.
        u.carried = after;
        u.run = match opcode {
            // Nothing falls through; a label after it says what reaches it.
            op::RETURN | op::JUMP | op::JUMP_LOOP | op::RAISE | op::RERAISE | op::ASSERT_FAIL => 0,
            _ => after,
        };
    }

    fn emit_one(&mut self, opcode: u8) {
        let pc = self.pc();
        let u = self.u();
        if u.last_loc != Some(u.cur) {
            u.code.positions.push(PosEntry {
                pc,
                pos: u.cur.pos,
                anchor: u.cur.anchor,
                no_carets: u.cur.no_carets,
            });
            u.last_loc = Some(u.cur);
        }
        u.lay = crate::bytecode::layout_of(opcode);
        u.at = u.code.bytecode.len();
        u.nops = 0;
        u.code.bytecode.push(u32::from(opcode));
        if u.lay.words == 2 {
            u.code.bytecode.push(0);
        }
    }

    /// Writes the next operand of the instruction being emitted; returns
    /// its word and bit shift.
    fn put(&mut self, v: u32) -> (usize, u8) {
        let u = self.u();
        let f = u.lay.fields[u.nops];
        u.nops += 1;
        let at = u.at + f.word as usize;
        u.code.bytecode[at] |= v << f.shift;
        (at, f.shift)
    }

    fn byte(&mut self, b: u8) {
        self.put(u32::from(b));
    }

    fn word(&mut self, w: u16) {
        self.put(u32::from(w));
    }

    /// A jump operand to label `l`, patched in `end_unit`.
    fn target(&mut self, l: Label) {
        let (at, shift) = self.put(0);
        let u = self.u();
        u.fixups.push((at, shift, l.0));
        // A forward jump carries its path's count to the label; a backward
        // one is `JUMP_LOOP`, a safepoint.
        if u.labels[l.0].is_none() {
            u.label_runs[l.0] = u.label_runs[l.0].max(u.carried);
        }
    }

    #[inline(never)]
    fn emit_r(&mut self, opcode: u8, r: u8) {
        self.emit(opcode);
        self.byte(r);
    }

    #[inline(never)]
    fn emit_rr(&mut self, opcode: u8, a: u8, b: u8) {
        self.emit(opcode);
        self.byte(a);
        self.byte(b);
    }

    #[inline(never)]
    fn emit_u(&mut self, opcode: u8, k: u16) {
        self.emit(opcode);
        self.word(k);
    }

    #[inline(never)]
    fn emit_ru(&mut self, opcode: u8, r: u8, k: u16) {
        self.emit(opcode);
        self.byte(r);
        self.word(k);
    }

    #[inline(never)]
    fn emit_uu(&mut self, opcode: u8, a: u16, b: u16) {
        self.emit(opcode);
        self.word(a);
        self.word(b);
    }

    #[inline(never)]
    fn emit_rb(&mut self, opcode: u8, r: u8, b: u8) {
        self.emit(opcode);
        self.byte(r);
        self.byte(b);
    }

    #[inline(never)]
    fn emit_rbu(&mut self, opcode: u8, r: u8, b: u8, k: u16) {
        self.emit(opcode);
        self.byte(r);
        self.byte(b);
        self.word(k);
    }

    #[inline(never)]
    fn label(&mut self) -> Label {
        let u = self.u();
        u.labels.push(None);
        u.label_runs.push(0);
        Label(u.labels.len() - 1)
    }

    fn bind(&mut self, l: Label) {
        let pc = self.pc();
        let u = self.u();
        u.labels[l.0] = Some(pc);
        u.run = u.run.max(u.label_runs[l.0]);
    }

    /// Binds a loop head: its back edge is a safepoint, so the count
    /// restarts (see [`CHECK_EVERY`]).
    fn bind_top(&mut self, l: Label) {
        self.bind(l);
        self.u().run = 0;
    }

    #[inline(never)]
    fn jump(&mut self, opcode: u8, l: Label) {
        self.emit(opcode);
        self.target(l);
    }

    #[inline(never)]
    fn jump_r(&mut self, opcode: u8, r: u8, l: Label) {
        self.emit(opcode);
        self.byte(r);
        self.target(l);
    }

    #[inline(never)]
    fn ic(&mut self) -> R<u16> {
        let u = self.u();
        let k = u16::try_from(u.code.n_ics).map_err(|_| too_many("functions this large"))?;
        u.code.n_ics += 1;
        Ok(k)
    }

    #[inline(never)]
    fn konst(&mut self, k: Konst) -> R<u16> {
        let u = self.u();
        match &k {
            Konst::Str(s) => {
                if let Some(&i) = u.str_consts.get(s) {
                    return Ok(u16::try_from(i).unwrap_or(u16::MAX));
                }
            }
            Konst::Int(n) => {
                if let Some(&i) = u.int_consts.get(n) {
                    return Ok(i);
                }
            }
            _ => {}
        }
        let i = u16::try_from(u.code.consts.len())
            .map_err(|_| too_many("functions with more than 65535 constants"))?;
        match &k {
            Konst::Str(s) => {
                u.str_consts.insert(s.clone(), usize::from(i));
            }
            Konst::Int(n) => {
                u.int_consts.insert(*n, i);
            }
            _ => {}
        }
        u.code.consts.push(k);
        Ok(i)
    }

    /// Index of name `n` (mangled if private, as every name the bytecode
    /// names: globals, class namespace names, attributes and imports).
    #[inline(never)]
    fn name(&mut self, n: &str) -> R<u16> {
        let n = &*symtable::mangle(&self.st.scopes[self.ur().scope].private, n);
        let u = self.u();
        if let Some(&i) = u.name_idx.get(n) {
            return Ok(u16::try_from(i).unwrap_or(u16::MAX));
        }
        let i = u16::try_from(u.code.names.len())
            .map_err(|_| too_many("functions with more than 65535 names"))?;
        u.name_idx.insert(n.to_string(), usize::from(i));
        u.code.names.push(n.into());
        Ok(i)
    }

    // ------------------------------------------------------ registers

    #[inline(never)]
    fn temp(&mut self) -> R<u8> {
        let u = self.u();
        let r = u8::try_from(u.next_reg)
            .ok()
            .filter(|&r| r < 255)
            .ok_or_else(|| too_many("expressions needing more than 255 registers"))?;
        u.next_reg += 1;
        u.max_reg = u.max_reg.max(u.next_reg);
        Ok(r)
    }

    fn temps(&mut self, n: usize) -> R<u8> {
        let base = self.temp()?;
        for _ in 1..n {
            self.temp()?;
        }
        Ok(base)
    }

    fn mark(&self) -> u32 {
        self.ur().next_reg
    }

    fn release(&mut self, m: u32) {
        self.u().next_reg = m;
    }

    // ----------------------------------------------- exception ranges

    /// Closes the current exception-table segment; a new one starts here
    /// with the current innermost handler.
    fn set_handlers(&mut self) {
        let pc = self.pc();
        let u = self.u();
        if let Some(&top) = u.handlers.last()
            && u.seg_start < pc
        {
            let depth = u32::try_from(u.handlers.len()).unwrap_or(u32::MAX);
            u.raw_exc.push((u.seg_start, pc, top, depth));
        }
        u.seg_start = pc;
    }

    fn push_handler(&mut self, l: Label) {
        self.set_handlers();
        self.u().handlers.push(l);
    }

    fn pop_handler(&mut self) {
        self.set_handlers();
        self.u().handlers.pop();
    }

    // ------------------------------------------------------- names

    fn resolve(&self, name: &str) -> Access {
        let u = self.ur();
        let name = &*symtable::mangle(&self.st.scopes[u.scope].private, name);
        let mut depth = u.comps.len();
        let mut sid = u.comps.last().copied().unwrap_or(u.scope);
        loop {
            let sc = &self.st.scopes[sid];
            let reg = u.reg(sid, name);
            let class = sc.kind == ScopeKind::Class;
            match sc.binding(name) {
                Some(Binding::Local) if class && sc.position(name) >= Some(sc.nparams) => {
                    return Access::Name;
                }
                Some(Binding::Local) => {
                    return reg.map_or(Access::Global, Access::Local);
                }
                Some(Binding::Cell) => return reg.map_or(Access::Global, Access::Deref),
                Some(Binding::Free) => {
                    if sc.kind == ScopeKind::Comprehension && depth > 0 {
                        depth -= 1;
                        sid = if depth > 0 {
                            u.comps[depth - 1]
                        } else {
                            u.scope
                        };
                        continue;
                    }
                    return reg.map_or(Access::Global, Access::Deref);
                }
                Some(Binding::GlobalImplicit) | None if class => return Access::Name,
                _ => return Access::Global,
            }
        }
    }

    fn is_bound(&self, r: u8) -> bool {
        self.ur().bound.contains(&r)
    }

    fn load_name(&mut self, name: &str) -> R<()> {
        match self.resolve(name) {
            Access::Local(r) => {
                if self.is_bound(r) {
                    self.emit_r(op::LDAR, r);
                } else {
                    self.emit_r(op::LDA_LOCAL, r);
                }
            }
            Access::Deref(r) => self.emit_r(op::LDA_DEREF, r),
            Access::Global => {
                let n = self.name(name)?;
                let ic = self.ic()?;
                self.emit_uu(op::LDA_GLOBAL, n, ic);
            }
            Access::Name => {
                let n = self.name(name)?;
                self.emit_u(op::LDA_NAME, n);
            }
        }
        Ok(())
    }

    fn store_name(&mut self, name: &str) -> R<()> {
        match self.resolve(name) {
            Access::Local(r) => {
                self.emit_r(op::STAR, r);
                let sid = self.ur().comps.last().copied().unwrap_or(self.ur().scope);
                let deleted = self.st.scopes[sid].is_deleted(name);
                let top = self.ur().block_depth == 0 && self.ur().comps.is_empty();
                if top && !deleted && !self.is_bound(r) {
                    self.u().bound.push(r);
                }
            }
            Access::Deref(r) => self.emit_r(op::STA_DEREF, r),
            Access::Global => {
                let n = self.name(name)?;
                let ic = self.ic()?;
                self.emit_uu(op::STA_GLOBAL, n, ic);
            }
            Access::Name => {
                let n = self.name(name)?;
                self.emit_u(op::STA_NAME, n);
            }
        }
        Ok(())
    }

    fn delete_name(&mut self, name: &str) -> R<()> {
        match self.resolve(name) {
            Access::Local(r) => self.emit_r(op::DEL_LOCAL, r),
            Access::Deref(r) => self.emit_r(op::DEL_DEREF, r),
            Access::Global => {
                let n = self.name(name)?;
                self.emit_u(op::DEL_GLOBAL, n);
            }
            Access::Name => {
                let n = self.name(name)?;
                self.emit_u(op::DEL_NAME, n);
            }
        }
        Ok(())
    }

    // ---------------------------------------------------- statements

    fn enter(&mut self, pos: Pos) -> R<()> {
        self.depth += 1;
        if self.depth > MAX_NESTING {
            return Err(CompileError::too_deep(pos));
        }
        Ok(())
    }

    fn block(&mut self, body: &'a [Stmt]) -> R<()> {
        self.u().block_depth += 1;
        for s in body {
            self.stmt(s)?;
        }
        self.u().block_depth -= 1;
        Ok(())
    }

    fn stmt(&mut self, s: &'a Stmt) -> R<()> {
        self.enter(s.pos)?;
        let m = self.mark();
        self.stmt_inner(s)?;
        self.release(m);
        self.depth -= 1;
        Ok(())
    }

    fn stmt_inner(&mut self, s: &'a Stmt) -> R<()> {
        match &s.kind {
            StmtKind::Expr(e) => self.expr(e),
            StmtKind::Assign(targets, value) => {
                self.expr(value)?;
                if targets.len() == 1 {
                    return self.store(&targets[0]);
                }
                let t = self.temp()?;
                self.emit_r(op::STAR, t);
                for (i, target) in targets.iter().enumerate() {
                    if i > 0 {
                        self.emit_r(op::LDAR, t);
                    }
                    self.store(target)?;
                }
                Ok(())
            }
            StmtKind::AugAssign(target, bop, value) => self.aug_assign(s.pos, target, *bop, value),
            StmtKind::AnnAssign(target, value, _) => {
                if let Some(v) = value {
                    self.expr(v)?;
                    self.store(target)?;
                }
                Ok(())
            }
            StmtKind::If(..) => {
                // An `elif` chain in a loop: each `elif` is the one
                // statement of the `else` before it, not a nested level.
                let end = self.label();
                let mut cur = s;
                let mut elifs = 0;
                while let StmtKind::If(test, body, orelse) = &cur.kind {
                    let els = self.label();
                    self.cond_jump(test, false, els)?;
                    self.block(body)?;
                    if orelse.is_empty() {
                        self.bind(els);
                        break;
                    }
                    self.jump(op::JUMP, end);
                    self.bind(els);
                    let [
                        next @ Stmt {
                            kind: StmtKind::If(..),
                            ..
                        },
                    ] = orelse.as_slice()
                    else {
                        self.block(orelse)?;
                        break;
                    };
                    self.u().block_depth += 1;
                    elifs += 1;
                    cur = next;
                }
                self.u().block_depth -= elifs;
                self.bind(end);
                Ok(())
            }
            StmtKind::While(test, body, orelse) => {
                let (top, els, brk) = (self.label(), self.label(), self.label());
                self.bind_top(top);
                self.cond_jump(test, false, els)?;
                self.u().fblocks.push(FBlock::Loop { brk, cont: top });
                self.block(body)?;
                self.u().fblocks.pop();
                self.at_pos(s.pos);
                self.jump(op::JUMP_LOOP, top);
                self.bind(els);
                self.block(orelse)?;
                self.bind(brk);
                Ok(())
            }
            StmtKind::For(target, iter, body, orelse, is_async) => {
                let (top, els, brk) = (self.label(), self.label(), self.label());
                let mut end_async = None;
                if *is_async {
                    self.async_ok("'async for' outside async function", s.pos)?;
                    self.expr(iter)?;
                    self.at(iter);
                    self.emit(op::GET_AITER);
                    let it = self.temp()?;
                    self.emit_r(op::STAR, it);
                    end_async = Some(self.anext_step(it, top, s.pos));
                } else if let Some((f, args)) = range_call(iter) {
                    // `for _ in range(...)`: counters in registers when
                    // `range` is the builtin and the arguments are ints,
                    // otherwise the generic call and iterator below. The
                    // range call has its arguments from `it + 1`; the
                    // generic `CALL` wants them from `it + 2` (`it + 1` is
                    // its reserved slot), so the fallback shifts them up.
                    let it = self.temps(5)?;
                    self.expr(f)?;
                    self.emit_r(op::STAR, it);
                    self.call_args(it + 1, args, &[])?;
                    let n = u8::try_from(args.len()).unwrap_or(1);
                    self.at(iter);
                    self.emit(op::FOR_PREP);
                    self.byte(it);
                    self.byte(n);
                    self.target(top);
                    let ic = self.ic()?;
                    self.at(iter);
                    for k in (1..=n).rev() {
                        self.emit_rr(op::MOV, it + k, it + k + 1);
                    }
                    self.emit_rbu(op::CALL, it, n, ic);
                    self.emit(op::GET_ITER);
                    self.emit_r(op::STAR, it);
                    self.emit(op::LDA_NONE);
                    self.emit_r(op::STAR, it + 1);
                    self.bind_top(top);
                    self.at(iter);
                    self.jump_r(op::FOR_RANGE, it, els);
                } else {
                    self.expr(iter)?;
                    self.at(iter);
                    self.emit(op::GET_ITER);
                    let it = self.temp()?;
                    self.emit_r(op::STAR, it);
                    self.bind_top(top);
                    self.at(iter);
                    self.jump_r(op::FOR_ITER, it, els);
                }
                let saved = self.ur().bound.len();
                self.u().block_depth += 1;
                self.store(target)?;
                self.bind_targets(target);
                self.u().block_depth -= 1;
                self.u().fblocks.push(FBlock::Loop { brk, cont: top });
                self.block(body)?;
                self.u().fblocks.pop();
                self.u().bound.truncate(saved);
                self.at(iter);
                self.jump(op::JUMP_LOOP, top);
                if let Some(h) = end_async {
                    self.bind(h);
                    self.emit(op::END_ASYNC_FOR);
                }
                self.bind(els);
                self.block(orelse)?;
                self.bind(brk);
                Ok(())
            }
            StmtKind::Break | StmtKind::Continue => {
                let is_break = matches!(s.kind, StmtKind::Break);
                let Some(i) = self
                    .ur()
                    .fblocks
                    .iter()
                    .rposition(|b| matches!(b, FBlock::Loop { .. }))
                else {
                    let msg = if is_break {
                        "'break' outside loop"
                    } else {
                        "'continue' not properly in loop"
                    };
                    return Err(CompileError::syntax(msg.to_string(), s.pos));
                };
                self.star_exit(i + 1, s.pos)?;
                self.at_pos(s.pos);
                self.unwind(i + 1, s.pos)?;
                if let FBlock::Loop { brk, cont } = self.ur().fblocks[i] {
                    self.at_pos(s.pos);
                    if is_break {
                        self.jump(op::JUMP, brk);
                    } else {
                        self.jump(op::JUMP_LOOP, cont);
                    }
                }
                Ok(())
            }
            StmtKind::Pass | StmtKind::Global(_) | StmtKind::Nonlocal(_) => Ok(()),
            StmtKind::Return(value) => {
                if self.ur().code.is_module || self.ur().is_class {
                    return Err(CompileError::syntax(
                        "'return' outside function".to_string(),
                        s.pos,
                    ));
                }
                if value.is_some() && self.ur().code.is_coroutine && self.ur().code.is_generator {
                    return Err(CompileError::syntax(
                        "'return' with value in async generator".to_string(),
                        s.pos,
                    ));
                }
                // CPython reports it at a literal value (whose load is the
                // only instruction), else at the statement; a negative
                // number is a folded `-x`, not a literal.
                let at = match value.as_ref().map(|v| (v, &v.kind)) {
                    Some((_, ExprKind::Const(Const::Int(i)))) if *i < 0 => s.pos,
                    Some((_, ExprKind::Const(Const::Float(f)))) if f.is_sign_negative() => s.pos,
                    Some((v, ExprKind::Const(_))) => v.pos,
                    _ => s.pos,
                };
                self.star_exit(0, at)?;
                if let Some(v) = value {
                    self.expr(v)?;
                } else {
                    self.at_pos(s.pos);
                    self.emit(op::LDA_NONE);
                }
                if self
                    .ur()
                    .fblocks
                    .iter()
                    .any(|b| !matches!(b, FBlock::Loop { .. }))
                {
                    let t = self.temp()?;
                    self.emit_r(op::STAR, t);
                    self.unwind(0, s.pos)?;
                    self.emit_r(op::LDAR, t);
                }
                self.at_pos(s.pos);
                self.emit(op::RETURN);
                Ok(())
            }
            StmtKind::FunctionDef(f) => {
                let m = self.mark();
                let mut decos = Vec::new();
                for d in &f.decorators {
                    self.expr(d)?;
                    let r = self.temp()?;
                    self.emit_r(op::STAR, r);
                    decos.push((r, d));
                }
                self.make_function(f, &f.name)?;
                // One call frame serves every decorator.
                let base = if decos.is_empty() { 0 } else { self.temps(3)? };
                for (r, d) in decos.into_iter().rev() {
                    self.emit_r(op::STAR, base + 2);
                    self.emit_rr(op::MOV, r, base);
                    self.at(d);
                    let ic = self.ic()?;
                    self.emit_rbu(op::CALL, base, 1, ic);
                }
                self.release(m);
                self.store_name(&f.name)
            }
            StmtKind::Delete(targets) => {
                for t in targets {
                    self.delete(t)?;
                }
                Ok(())
            }
            StmtKind::Raise(exc, cause) => {
                match (exc, cause) {
                    (None, _) => {
                        self.at_pos(s.pos);
                        self.emit_rb(op::RAISE, 0, 0);
                    }
                    (Some(e), None) => {
                        self.expr(e)?;
                        self.at_pos(s.pos);
                        self.emit_rb(op::RAISE, 1, 0);
                    }
                    (Some(e), Some(c)) => {
                        self.expr(e)?;
                        let t = self.temp()?;
                        self.emit_r(op::STAR, t);
                        self.expr(c)?;
                        self.at_pos(s.pos);
                        self.emit_rb(op::RAISE, 2, t);
                    }
                }
                Ok(())
            }
            StmtKind::Try {
                body,
                handlers,
                orelse,
                finalbody,
                star,
            } => self.try_stmt(body, handlers, orelse, finalbody, *star),
            StmtKind::Assert(test, msg) => {
                let ok = self.label();
                self.cond_jump(test, true, ok)?;
                let has_msg = if let Some(m) = msg {
                    self.expr(m)?;
                    1
                } else {
                    0
                };
                // CPython attributes the failure to the test expression.
                self.at(test);
                self.emit(op::ASSERT_FAIL);
                self.byte(has_msg);
                self.bind(ok);
                Ok(())
            }
            StmtKind::Import(names) => {
                for a in names {
                    let n = self.name(&a.name)?;
                    let top = a.asname.is_none() && a.name.contains('.');
                    self.at_pos(s.pos);
                    self.emit(op::IMPORT_NAME);
                    self.word(n);
                    self.byte(u8::from(top));
                    let bound = a
                        .asname
                        .as_deref()
                        .unwrap_or_else(|| a.name.split('.').next().unwrap_or(&a.name));
                    self.store_name(bound)?;
                }
                Ok(())
            }
            StmtKind::ClassDef(c) => self.class_def(c),
            StmtKind::With(items, body, is_async) => {
                if *is_async {
                    self.async_ok("'async with' outside async function", s.pos)?;
                }
                self.with_stmt(items, body, s.pos, *is_async)
            }
            StmtKind::Match(subject, cases) => self.match_stmt(subject, cases),
            StmtKind::ImportFrom(module, names) => {
                let n = self.name(module)?;
                self.at_pos(s.pos);
                self.emit(op::IMPORT_NAME);
                self.word(n);
                self.byte(0);
                let m = self.temp()?;
                self.emit_r(op::STAR, m);
                for a in names {
                    let k = self.name(&a.name)?;
                    self.at_pos(s.pos);
                    self.emit_ru(op::IMPORT_FROM, m, k);
                    if a.name != "*" {
                        self.store_name(a.asname.as_deref().unwrap_or(&a.name))?;
                    }
                }
                Ok(())
            }
        }
    }

    /// Marks loop-target registers as bound for the loop body.
    fn bind_targets(&mut self, target: &Expr) {
        match &target.kind {
            ExprKind::Name(n) => {
                if let Access::Local(r) = self.resolve(n) {
                    let sid = self.ur().comps.last().copied().unwrap_or(self.ur().scope);
                    if !self.st.scopes[sid].is_deleted(n) {
                        self.u().bound.push(r);
                    }
                }
            }
            ExprKind::Tuple(items) | ExprKind::List(items) => {
                for i in items {
                    self.bind_targets(i);
                }
            }
            ExprKind::Starred(inner) => self.bind_targets(inner),
            _ => {}
        }
    }

    /// Emits the exit actions of every fblock above index `keep`, innermost
    /// first: inlined `finally` bodies and handler exits.
    fn unwind(&mut self, keep: usize, pos: Pos) -> R<()> {
        let n = self.ur().fblocks.len();
        for i in (keep..n).rev() {
            let action = match &self.ur().fblocks[i] {
                FBlock::Loop { .. } => None,
                FBlock::With {
                    exit,
                    handlers,
                    ctx,
                    is_async,
                } => Some((None, *handlers, None, Some((*exit, *ctx, *is_async)))),
                FBlock::Finally { body, handlers } => Some((Some(*body), *handlers, None, None)),
                FBlock::Handler {
                    old,
                    handlers,
                    name,
                    ..
                } => Some((None, *handlers, Some((*old, *name)), None)),
            };
            let Some((body, handlers, exit, with_exit)) = action else {
                continue;
            };
            // Emit with the fblocks and handlers as they were outside.
            let saved_blocks: Vec<FBlock<'a>> = self.u().fblocks.drain(i..).collect();
            self.set_handlers();
            let saved_handlers: Vec<Label> = self.u().handlers.drain(handlers..).collect();
            if let Some((r, ctx, is_async)) = with_exit {
                self.at(ctx);
                self.emit_rb(op::WITH_EXIT, r, if is_async { 2 } else { 0 });
                if is_async {
                    self.emit(op::AWAIT);
                }
            }
            if let Some((old, name)) = exit {
                self.at_pos(pos);
                self.emit_r(op::POP_EXC, old);
                if let Some(name) = name {
                    self.emit(op::LDA_NONE);
                    self.store_name(name)?;
                    self.delete_name(name)?;
                }
            }
            if let Some(body) = body {
                self.block(body)?;
            }
            self.set_handlers();
            self.u().handlers.extend(saved_handlers);
            self.u().fblocks.extend(saved_blocks);
        }
        Ok(())
    }

    /// `break`, `continue` and `return` may not leave an `except*` handler
    /// (the fblocks above `keep`).
    fn star_exit(&self, keep: usize, pos: Pos) -> R<()> {
        if self.ur().fblocks[keep..]
            .iter()
            .any(|b| matches!(b, FBlock::Handler { star: true, .. }))
        {
            return Err(CompileError::syntax(
                "'break', 'continue' and 'return' cannot appear in an except* block".to_string(),
                pos,
            ));
        }
        Ok(())
    }

    fn try_stmt(
        &mut self,
        body: &'a [Stmt],
        handlers: &'a [Handler],
        orelse: &'a [Stmt],
        finalbody: &'a [Stmt],
        star: bool,
    ) -> R<()> {
        if finalbody.is_empty() {
            return self.try_except(body, handlers, orelse, star);
        }
        let fin = self.label();
        let end = self.label();
        let depth = self.ur().handlers.len();
        self.push_handler(fin);
        self.u().fblocks.push(FBlock::Finally {
            body: finalbody,
            handlers: depth,
        });
        if handlers.is_empty() {
            self.block(body)?;
        } else {
            self.try_except(body, handlers, orelse, star)?;
        }
        self.u().fblocks.pop();
        self.pop_handler();
        self.block(finalbody)?;
        self.jump(op::JUMP, end);
        // Exceptional path: run the body with the exception handled, re-raise.
        self.bind(fin);
        let m = self.mark();
        let old = self.temp()?;
        let exc = self.temp()?;
        let cleanup = self.label();
        self.emit_r(op::PUSH_EXC, old);
        self.emit_r(op::STAR, exc);
        self.push_handler(cleanup);
        self.u().fblocks.push(FBlock::Handler {
            old,
            handlers: depth,
            name: None,
            star: false,
        });
        self.block(finalbody)?;
        self.u().fblocks.pop();
        self.pop_handler();
        self.emit_r(op::POP_EXC, old);
        self.emit_r(op::LDAR, exc);
        self.emit(op::RERAISE);
        self.bind(cleanup);
        self.emit_r(op::POP_EXC, old);
        self.emit(op::RERAISE);
        self.release(m);
        self.bind(end);
        Ok(())
    }

    fn try_except(
        &mut self,
        body: &'a [Stmt],
        handlers: &'a [Handler],
        orelse: &'a [Stmt],
        star: bool,
    ) -> R<()> {
        if star {
            return self.try_star(body, handlers, orelse);
        }
        let h = self.label();
        let end = self.label();
        let depth = self.ur().handlers.len();
        self.push_handler(h);
        self.block(body)?;
        self.pop_handler();
        self.block(orelse)?;
        self.jump(op::JUMP, end);

        self.bind(h);
        let m = self.mark();
        let old = self.temp()?;
        let exc = self.temp()?;
        let cleanup = self.label();
        if let Some(first) = handlers.first() {
            self.at_pos(first.pos);
        }
        self.emit_r(op::PUSH_EXC, old);
        self.emit_r(op::STAR, exc);
        self.push_handler(cleanup);
        for (i, hd) in handlers.iter().enumerate() {
            let next = self.label();
            match &hd.typ {
                Some(t) => {
                    self.expr(t)?;
                    self.at_pos(hd.pos);
                    self.emit_r(op::CHECK_EXC, exc);
                    self.jump(op::JUMP_IF_FALSE, next);
                }
                None => {
                    if i + 1 != handlers.len() {
                        return Err(CompileError::syntax(
                            "default 'except:' must be last".to_string(),
                            hd.pos,
                        ));
                    }
                }
            }
            // A named handler unbinds its name on an exceptional exit too.
            let unbind = hd.name.as_ref().map(|_| self.label());
            if let Some(n) = &hd.name {
                self.emit_r(op::LDAR, exc);
                self.store_name(n)?;
            }
            if let Some(l) = unbind {
                self.push_handler(l);
            }
            self.u().fblocks.push(FBlock::Handler {
                old,
                handlers: depth,
                name: hd.name.as_deref(),
                star: false,
            });
            self.block(&hd.body)?;
            self.u().fblocks.pop();
            if unbind.is_some() {
                self.pop_handler();
            }
            self.at_pos(hd.pos);
            self.emit_r(op::POP_EXC, old);
            if let Some(n) = &hd.name {
                self.emit(op::LDA_NONE);
                self.store_name(n)?;
                self.delete_name(n)?;
            }
            self.jump(op::JUMP, end);
            if let (Some(l), Some(n)) = (unbind, &hd.name) {
                self.bind(l);
                let raised = self.temp()?;
                self.emit_r(op::STAR, raised);
                self.emit(op::LDA_NONE);
                self.store_name(n)?;
                self.delete_name(n)?;
                self.emit_r(op::LDAR, raised);
                self.jump(op::JUMP, cleanup);
            }
            self.bind(next);
        }
        self.emit_r(op::LDAR, exc);
        self.emit(op::RERAISE);
        self.pop_handler();
        self.bind(cleanup);
        self.emit_r(op::POP_EXC, old);
        self.emit(op::RERAISE);
        self.release(m);
        self.bind(end);
        Ok(())
    }

    /// `try ... except*` as CPython 3.14 compiles it: a state list
    /// `[orig, rest, raised...]`; each clause takes the matching part of
    /// the rest (`EXC_STAR` 0, the prelude's `_star_match`) and runs with
    /// it as the handled exception, collecting what it raises; at the end
    /// `EXC_STAR` 1 (`_star_prep`) gives what to re-raise, as is.
    fn try_star(&mut self, body: &'a [Stmt], handlers: &'a [Handler], orelse: &'a [Stmt]) -> R<()> {
        let h = self.label();
        let end = self.label();
        let depth = self.ur().handlers.len();
        self.push_handler(h);
        self.block(body)?;
        self.pop_handler();
        self.block(orelse)?;
        self.jump(op::JUMP, end);

        self.bind(h);
        let m = self.mark();
        let old = self.temp()?;
        let state = self.temp()?;
        let spare = self.temp()?;
        let cleanup = self.label();
        if let Some(first) = handlers.first() {
            self.at_pos(first.pos);
        }
        self.emit_r(op::PUSH_EXC, old);
        self.emit_r(op::STAR, spare);
        self.push_handler(cleanup);
        // `[orig, rest]`, the rest being all of it at first.
        self.emit_ru(op::BUILD_LIST, state, 0);
        self.emit_r(op::STAR, state);
        self.emit_r(op::LDAR, spare);
        self.emit_r(op::LIST_APPEND, state);
        self.emit_r(op::LIST_APPEND, state);
        for hd in handlers {
            let next = self.label();
            // CPython attributes the match to the whole clause.
            let mut span = hd.pos;
            if let Some(last) = hd.body.last() {
                span.end_line = last.pos.end_line;
                span.end_col = last.pos.end_col;
            }
            if let Some(t) = &hd.typ {
                self.expr(t)?;
            }
            self.at_pos(span);
            self.emit_rb(op::EXC_STAR, state, 0);
            // `None` is no match (a group may be falsy).
            self.emit_r(op::STAR, spare);
            self.emit(op::LDA_NONE);
            self.emit_r(op::TEST_IS, spare);
            self.jump(op::JUMP_IF_TRUE, next);
            self.emit_r(op::LDAR, spare);
            // The match is the handled exception from here on.
            self.emit_r(op::PUSH_EXC, spare);
            let unbind = self.label();
            if let Some(n) = &hd.name {
                self.store_name(n)?;
            }
            self.push_handler(unbind);
            self.u().fblocks.push(FBlock::Handler {
                old,
                handlers: depth,
                name: hd.name.as_deref(),
                star: true,
            });
            self.block(&hd.body)?;
            self.u().fblocks.pop();
            self.pop_handler();
            if let Some(n) = &hd.name {
                self.emit(op::LDA_NONE);
                self.store_name(n)?;
                self.delete_name(n)?;
            }
            self.jump(op::JUMP, next);
            // What the clause raises joins the state list.
            self.bind(unbind);
            if let Some(n) = &hd.name {
                self.emit_r(op::STAR, spare);
                self.emit(op::LDA_NONE);
                self.store_name(n)?;
                self.delete_name(n)?;
                self.emit_r(op::LDAR, spare);
            }
            self.emit_r(op::LIST_APPEND, state);
            self.bind(next);
        }
        self.emit_rb(op::EXC_STAR, state, 1);
        let quiet = self.label();
        self.emit_r(op::STAR, spare);
        self.emit(op::LDA_NONE);
        self.emit_r(op::TEST_IS, spare);
        self.jump(op::JUMP_IF_TRUE, quiet);
        self.emit_r(op::LDAR, spare);
        self.emit(op::RERAISE);
        self.bind(quiet);
        self.pop_handler();
        self.emit_r(op::POP_EXC, old);
        self.jump(op::JUMP, end);
        self.bind(cleanup);
        self.emit_r(op::POP_EXC, old);
        self.emit(op::RERAISE);
        self.release(m);
        self.bind(end);
        Ok(())
    }

    /// `with a as x, b: body`: one context manager at a time, outermost
    /// first. The exit function sits in a register; the body's handler
    /// calls it with the exception and re-raises unless it returns true.
    /// `async with` awaits what `__aenter__` and `__aexit__` return, and
    /// tests the awaited result of the exceptional `__aexit__` here.
    fn with_stmt(
        &mut self,
        items: &'a [WithItem],
        body: &'a [Stmt],
        pos: Pos,
        is_async: bool,
    ) -> R<()> {
        let Some((first, rest)) = items.split_first() else {
            return self.block(body);
        };
        self.enter(pos)?;
        let m = self.mark();
        self.expr(&first.ctx)?;
        let exit = self.temp()?;
        self.at(&first.ctx);
        self.emit_rb(op::WITH_ENTER, exit, u8::from(is_async));
        if is_async {
            self.emit(op::AWAIT);
        }
        let h = self.label();
        let end = self.label();
        let depth = self.ur().handlers.len();
        self.push_handler(h);
        if let Some(t) = &first.target {
            self.store(t)?;
        }
        self.u().fblocks.push(FBlock::With {
            exit,
            handlers: depth,
            ctx: &first.ctx,
            is_async,
        });
        self.u().block_depth += 1;
        if rest.is_empty() {
            self.block(body)?;
        } else {
            self.with_stmt(rest, body, pos, is_async)?;
        }
        self.u().block_depth -= 1;
        self.u().fblocks.pop();
        self.pop_handler();
        // CPython reports `__exit__` at the context expression: without
        // the call anchor on a normal exit, with it on an exceptional one.
        self.at_pos(first.ctx.pos);
        let b = if is_async { 2 } else { 0 };
        self.emit_rb(op::WITH_EXIT, exit, b);
        if is_async {
            self.emit(op::AWAIT);
        }
        self.jump(op::JUMP, end);
        // Exceptional path: the exception is in acc.
        self.bind(h);
        let old = self.temp()?;
        let cleanup = self.label();
        self.at(&first.ctx);
        self.emit_r(op::PUSH_EXC, old);
        self.push_handler(cleanup);
        if is_async {
            let e = self.temp()?;
            let suppress = self.label();
            self.emit_r(op::STAR, e);
            self.emit_rb(op::WITH_EXIT, exit, 3);
            self.emit(op::AWAIT);
            self.jump(op::JUMP_IF_TRUE, suppress);
            self.emit_r(op::LDAR, e);
            self.emit(op::RERAISE);
            self.bind(suppress);
        } else {
            self.emit_rb(op::WITH_EXIT, exit, 1);
        }
        self.pop_handler();
        self.emit_r(op::POP_EXC, old);
        self.jump(op::JUMP, end);
        self.bind(cleanup);
        self.emit_r(op::POP_EXC, old);
        self.emit(op::RERAISE);
        self.release(m);
        self.bind(end);
        self.depth -= 1;
        Ok(())
    }

    /// A `class` statement: decorators, the name, bases, keywords and a
    /// namespace dict; the body runs as a function over the namespace; then
    /// `BUILD_CLASS`, the decorators and the binding.
    fn class_def(&mut self, c: &'a ClassDef) -> R<()> {
        let Some(scope) = self.st.scope_of(symtable::class_key(c)) else {
            return Err(too_many("this class (no scope)"));
        };
        let m = self.mark();
        let mut decos = Vec::new();
        for d in &c.decorators {
            self.expr(d)?;
            let r = self.temp()?;
            self.emit_r(op::STAR, r);
            decos.push((r, d));
        }
        let base = self.temps(4)?;
        self.at_pos(c.pos);
        let k = self.konst(Konst::Str(c.name.clone()))?;
        self.emit_u(op::LDA_CONST, k);
        self.emit_r(op::STAR, base);
        // Bases.
        if c.bases
            .iter()
            .any(|b| matches!(b.kind, ExprKind::Starred(_)))
        {
            let l = base + 1;
            self.emit_ru(op::BUILD_LIST, l, 0);
            self.emit_r(op::STAR, l);
            for b in &c.bases {
                if let ExprKind::Starred(inner) = &b.kind {
                    self.expr(inner)?;
                    self.at(b);
                    self.emit_rb(op::LIST_EXTEND, l, 0);
                } else {
                    self.expr(b)?;
                    self.emit_r(op::LIST_APPEND, l);
                }
            }
            self.emit_r(op::LDAR, l);
            self.emit(op::LIST_TO_TUPLE);
            self.emit_r(op::STAR, l);
        } else {
            let refs: Vec<&'a Expr> = c.bases.iter().collect();
            let t = self.build_seq(&refs, op::BUILD_TUPLE)?;
            self.emit_rr(op::MOV, t, base + 1);
            self.release(u32::from(base) + 4);
        }
        // Keywords (for `__init_subclass__`).
        let kw = base + 3;
        if c.keywords.is_empty() {
            self.emit(op::LDA_NONE);
            self.emit_r(op::STAR, kw);
        } else {
            self.emit_ru(op::BUILD_DICT, kw, 0);
            self.emit_r(op::STAR, kw);
            for k in &c.keywords {
                let mm = self.mark();
                if let Some(n) = &k.name {
                    let kr = self.temp()?;
                    let ck = self.konst(Konst::Str(n.clone()))?;
                    self.emit_u(op::LDA_CONST, ck);
                    self.emit_r(op::STAR, kr);
                    self.expr(&k.value)?;
                    self.emit_rr(op::DICT_INSERT, kw, kr);
                } else {
                    self.expr(&k.value)?;
                    self.at(&k.value);
                    self.emit_rb(op::DICT_UPDATE, kw, 0);
                    self.byte(0);
                }
                self.release(mm);
            }
        }
        // The namespace: `__module__` and `__qualname__`.
        let qual = self.qualname(&c.name);
        let mm = self.mark();
        let t = self.temps(4)?;
        self.at_pos(c.pos);
        let k1 = self.konst(Konst::Str("__module__".to_string()))?;
        self.emit_u(op::LDA_CONST, k1);
        self.emit_r(op::STAR, t);
        let n = self.name("__name__")?;
        let ic = self.ic()?;
        self.emit_uu(op::LDA_GLOBAL, n, ic);
        self.emit_r(op::STAR, t + 1);
        let k2 = self.konst(Konst::Str("__qualname__".to_string()))?;
        self.emit_u(op::LDA_CONST, k2);
        self.emit_r(op::STAR, t + 2);
        let k3 = self.konst(Konst::Str(qual))?;
        self.emit_u(op::LDA_CONST, k3);
        self.emit_r(op::STAR, t + 3);
        self.emit_ru(op::BUILD_DICT, t, 2);
        self.emit_r(op::STAR, base + 2);
        self.release(mm);
        // Run the body over the namespace.
        let idx = self.class_body(c, scope)?;
        let freenames = self.codes[idx]
            .as_ref()
            .map_or_default(|c| c.freenames.clone());
        let f = self.temps(3)?;
        // The closure tuple's temp is dead after MAKE_FUNCTION: release it
        // so the call window below stays the top of the temps.
        let mf = self.mark();
        let mut flags = 0;
        let parts = if freenames.is_empty() {
            f
        } else {
            flags = 4;
            self.closure(&freenames)?
        };
        let ci = u16::try_from(idx).map_err(|_| too_many("more than 65535 functions"))?;
        self.at_pos(c.pos);
        self.emit(op::MAKE_FUNCTION);
        self.word(ci);
        self.byte(flags);
        self.byte(parts);
        self.release(mf);
        self.emit_r(op::STAR, f);
        self.emit_rr(op::MOV, base + 2, f + 2);
        let ic = self.ic()?;
        self.at_pos(c.span);
        self.emit_rbu(op::CALL, f, 1, ic);
        self.emit_r(op::BUILD_CLASS, base);
        let b = if decos.is_empty() { 0 } else { self.temps(3)? };
        for (r, d) in decos.into_iter().rev() {
            self.emit_r(op::STAR, b + 2);
            self.emit_rr(op::MOV, r, b);
            self.at(d);
            let ic = self.ic()?;
            self.emit_rbu(op::CALL, b, 1, ic);
        }
        self.release(m);
        self.store_name(&c.name)
    }

    /// The code of a class body; returns its index.
    fn class_body(&mut self, c: &'a ClassDef, scope: ScopeId) -> R<usize> {
        let idx = self.codes.len();
        self.codes.push(None);
        let qual = self.qualname(&c.name);
        self.begin_unit(scope, &c.name, &qual, c.pos.line, false)?;
        self.u().code.params = ParamSpec {
            argcount: 1,
            ..ParamSpec::default()
        };
        if let Some(first) = c.body.first()
            && let StmtKind::Expr(e) = &first.kind
            && let ExprKind::Const(Const::Str(doc)) = &e.kind
        {
            self.at_pos(first.pos);
            let k = self.konst(Konst::Str(doc.clone()))?;
            self.emit_u(op::LDA_CONST, k);
            let n = self.name("__doc__")?;
            self.emit_u(op::STA_NAME, n);
        }
        for s in &c.body {
            self.stmt(s)?;
        }
        let end = c.body.last().map_or(c.pos, |s| s.pos);
        self.at_pos(end);
        let sc = &self.st.scopes[scope];
        let cell = (sc.binding("__class__") == Some(Binding::Cell))
            .then(|| self.ur().reg(scope, "__class__"))
            .flatten();
        match cell {
            Some(r) => self.emit_r(op::LDAR, r),
            None => self.emit(op::LDA_NONE),
        }
        self.emit(op::RETURN);
        let code = self.end_unit()?;
        self.codes[idx] = Some(code);
        Ok(idx)
    }

    fn aug_assign(&mut self, pos: Pos, target: &'a Expr, bop: BinOp, value: &'a Expr) -> R<()> {
        match &target.kind {
            ExprKind::Name(n) => {
                self.at(target);
                self.load_name(n)?;
                let t = self.temp()?;
                self.emit_r(op::STAR, t);
                self.expr(value)?;
                self.at_pos(pos);
                self.emit_rb(op::INPLACE, bop as u8, t);
                self.store_name(n)
            }
            ExprKind::Attribute(obj, attr) => {
                let o = self.operand(obj, None)?;
                let n = self.name(attr)?;
                let ic = self.ic()?;
                self.emit_r(op::LDAR, o);
                self.at(target);
                self.emit_uu(op::GET_ATTR, n, ic);
                let t = self.temp()?;
                self.emit_r(op::STAR, t);
                self.expr(value)?;
                self.at_pos(pos);
                self.emit_rb(op::INPLACE, bop as u8, t);
                self.at(target);
                let ic = self.ic()?;
                self.emit_ru(op::SET_ATTR, o, n);
                self.word(ic);
                Ok(())
            }
            ExprKind::Subscript(obj, idx) => {
                let o = self.operand(obj, None)?;
                let i = self.temp()?;
                self.subscript_index(idx)?;
                self.emit_r(op::STAR, i);
                self.at(target);
                self.emit_r(op::GET_ITEM, o);
                let t = self.temp()?;
                self.emit_r(op::STAR, t);
                self.expr(value)?;
                self.at_pos(pos);
                self.emit_rb(op::INPLACE, bop as u8, t);
                self.at(target);
                self.emit_rr(op::SET_ITEM, o, i);
                Ok(())
            }
            _ => Err(CompileError::syntax(
                "'expression' is an illegal expression for augmented assignment".to_string(),
                target.pos,
            )),
        }
    }

    /// Stores the accumulator into `target`.
    fn store(&mut self, target: &'a Expr) -> R<()> {
        match &target.kind {
            ExprKind::Name(n) => self.store_name(n),
            ExprKind::Attribute(obj, attr) => {
                let m = self.mark();
                let v = self.temp()?;
                self.emit_r(op::STAR, v);
                let o = self.operand(obj, None)?;
                self.emit_r(op::LDAR, v);
                let n = self.name(attr)?;
                self.at(target);
                let ic = self.ic()?;
                self.emit_ru(op::SET_ATTR, o, n);
                self.word(ic);
                self.release(m);
                Ok(())
            }
            ExprKind::Subscript(obj, idx) => {
                let m = self.mark();
                let v = self.temp()?;
                self.emit_r(op::STAR, v);
                let o = self.operand(obj, None)?;
                let i = self.temp()?;
                self.subscript_index(idx)?;
                self.emit_r(op::STAR, i);
                self.emit_r(op::LDAR, v);
                self.at(target);
                self.emit_rr(op::SET_ITEM, o, i);
                self.release(m);
                Ok(())
            }
            ExprKind::Tuple(items) | ExprKind::List(items) => {
                let m = self.mark();
                let star = items
                    .iter()
                    .position(|e| matches!(e.kind, ExprKind::Starred(_)));
                let n = items.len();
                let base = self.temps(n)?;
                self.at(target);
                match star {
                    None => {
                        let k = u16::try_from(n).map_err(|_| too_many("this many targets"))?;
                        self.emit_ru(op::UNPACK_SEQ, base, k);
                    }
                    Some(si) => {
                        if items
                            .iter()
                            .filter(|e| matches!(e.kind, ExprKind::Starred(_)))
                            .count()
                            > 1
                        {
                            return Err(CompileError::syntax(
                                "multiple starred expressions in assignment".to_string(),
                                target.pos,
                            ));
                        }
                        let before = u8::try_from(si).map_err(|_| too_many("this many targets"))?;
                        let after =
                            u8::try_from(n - si - 1).map_err(|_| too_many("this many targets"))?;
                        self.emit(op::UNPACK_EX);
                        self.byte(base);
                        self.byte(before);
                        self.byte(after);
                    }
                }
                for (k, item) in items.iter().enumerate() {
                    let r = base + u8::try_from(k).unwrap_or(0);
                    self.emit_r(op::LDAR, r);
                    let inner = match &item.kind {
                        ExprKind::Starred(inner) => inner,
                        _ => item,
                    };
                    self.store(inner)?;
                }
                self.release(m);
                Ok(())
            }
            ExprKind::Starred(_) => Err(CompileError::syntax(
                "starred assignment target must be in a list or tuple".to_string(),
                target.pos,
            )),
            _ => Err(CompileError::syntax(
                "cannot assign to expression".to_string(),
                target.pos,
            )),
        }
    }

    fn delete(&mut self, target: &'a Expr) -> R<()> {
        match &target.kind {
            ExprKind::Name(n) => {
                self.at(target);
                self.delete_name(n)
            }
            ExprKind::Attribute(obj, attr) => {
                self.expr(obj)?;
                let n = self.name(attr)?;
                self.at(target);
                self.emit_u(op::DEL_ATTR, n);
                Ok(())
            }
            ExprKind::Subscript(obj, idx) => {
                let m = self.mark();
                let o = self.operand(obj, None)?;
                self.subscript_index(idx)?;
                self.at(target);
                self.emit_r(op::DEL_ITEM, o);
                self.release(m);
                Ok(())
            }
            ExprKind::Tuple(items) | ExprKind::List(items) => {
                for i in items {
                    self.delete(i)?;
                }
                Ok(())
            }
            _ => Err(CompileError::syntax(
                "cannot delete expression".to_string(),
                target.pos,
            )),
        }
    }

    // --------------------------------------------------- functions

    fn qualname(&self, name: &str) -> String {
        let u = self.ur();
        if u.code.is_module {
            name.to_string()
        } else if u.is_class {
            format!("{}.{name}", u.code.qualname)
        } else {
            format!("{}.<locals>.{name}", u.code.qualname)
        }
    }

    /// Builds defaults and closure, then `MAKE_FUNCTION`; result in acc.
    fn make_function(&mut self, f: &'a FuncDef, name: &str) -> R<()> {
        let Some(scope) = self.st.scope_of(symtable::func_key(f)) else {
            return Err(too_many("this function (no scope)"));
        };
        let m = self.mark();
        let defaults: Vec<&'a Expr> = f
            .params
            .posonly
            .iter()
            .chain(&f.params.args)
            .filter_map(|p| p.default.as_ref())
            .collect();
        let kwdefaults: Vec<(&'a str, &'a Expr)> = f
            .params
            .kwonly
            .iter()
            .filter_map(|p| p.default.as_ref().map(|d| (p.name.as_str(), d)))
            .collect();
        // Keyed by the parameter's name as bound: mangled, like CPython's.
        let private = self.st.scopes[self.ur().scope].private.clone();
        let mut parts = Vec::new();
        let mut flags = 0u8;
        if !defaults.is_empty() {
            let r = self.build_seq(&defaults, op::BUILD_TUPLE)?;
            parts.push(r);
            flags |= 1;
        }
        if !kwdefaults.is_empty() {
            let mm = self.mark();
            let n = kwdefaults.len();
            let base = self.temps(2 * n)?;
            for (k, (pname, d)) in kwdefaults.iter().enumerate() {
                let kr = base + u8::try_from(2 * k).unwrap_or(0);
                let c = self.konst(Konst::Str(symtable::mangle(&private, pname).into_owned()))?;
                self.emit_u(op::LDA_CONST, c);
                self.emit_r(op::STAR, kr);
                self.expr(d)?;
                self.emit_r(op::STAR, kr + 1);
            }
            let k = u16::try_from(n).map_err(|_| too_many("this many defaults"))?;
            self.emit_ru(op::BUILD_DICT, base, k);
            self.release(mm);
            let r = self.temp()?;
            self.emit_r(op::STAR, r);
            parts.push(r);
            flags |= 2;
        }
        let code_idx = self.function_body(f, scope, name)?;
        let freenames = self.codes[code_idx]
            .as_ref()
            .map_or_default(|c| c.freenames.clone());
        if !freenames.is_empty() {
            let r = self.closure(&freenames)?;
            parts.push(r);
            flags |= 4;
        }
        // Move the parts to consecutive registers.
        let base = self.temps(parts.len().max(1))?;
        for (k, r) in parts.iter().enumerate() {
            self.emit_rr(op::MOV, *r, base + u8::try_from(k).unwrap_or(0));
        }
        let idx = u16::try_from(code_idx).map_err(|_| too_many("more than 65535 functions"))?;
        self.at_pos(f.pos);
        self.emit(op::MAKE_FUNCTION);
        self.word(idx);
        self.byte(flags);
        self.byte(base);
        self.release(m);
        Ok(())
    }

    /// The closure tuple for a child with these free variables, in a temp.
    fn closure(&mut self, freenames: &[Arc<str>]) -> R<u8> {
        let mm = self.mark();
        let n = freenames.len();
        let base = self.temps(n)?;
        for (k, fname) in freenames.iter().enumerate() {
            let u = self.ur();
            if u.is_class
                && let Some(r) = u.reg(u.scope, fname)
            {
                self.emit_rr(op::MOV, r, base + u8::try_from(k).unwrap_or(0));
                continue;
            }
            match self.resolve(fname) {
                Access::Deref(r) | Access::Local(r) => {
                    self.emit_rr(op::MOV, r, base + u8::try_from(k).unwrap_or(0));
                }
                Access::Global | Access::Name => {
                    return Err(too_many("this closure (unresolved free variable)"));
                }
            }
        }
        let k = u16::try_from(n).map_err(|_| too_many("this many free variables"))?;
        self.emit_ru(op::BUILD_TUPLE, base, k);
        self.release(mm);
        let r = self.temp()?;
        self.emit_r(op::STAR, r);
        Ok(r)
    }

    fn function_body(&mut self, f: &'a FuncDef, scope: ScopeId, name: &str) -> R<usize> {
        let idx = self.codes.len();
        self.codes.push(None);
        let qual = self.qualname(name);
        self.begin_unit(scope, name, &qual, f.pos.line, false)?;
        self.u().code.is_coroutine = f.is_async;
        self.u().code.is_generator = self.st.scopes[scope].generator;
        if let Some(first) = f.body.first()
            && let StmtKind::Expr(e) = &first.kind
            && let ExprKind::Const(Const::Str(doc)) = &e.kind
        {
            self.u().code.doc = Some(doc.clone());
        }
        let p = &f.params;
        self.u().code.params = ParamSpec {
            argcount: u32::try_from(p.posonly.len() + p.args.len()).unwrap_or(0),
            posonly: u32::try_from(p.posonly.len()).unwrap_or(0),
            kwonly: u32::try_from(p.kwonly.len()).unwrap_or(0),
            varargs: p.vararg.is_some(),
            varkw: p.kwarg.is_some(),
        };
        for s in &f.body {
            self.stmt(s)?;
        }
        if !matches!(f.body.last().map(|s| &s.kind), Some(StmtKind::Return(_))) {
            let end = f.body.last().map_or(f.pos, |s| s.pos);
            self.at_pos(end);
            self.emit(op::LDA_NONE);
            self.emit(op::RETURN);
        }
        let code = self.end_unit()?;
        self.codes[idx] = Some(code);
        Ok(idx)
    }

    // --------------------------------------------------- expressions

    /// A register holding `e`: a bound local's own register, or a temp.
    /// `clobber` is evaluated after the operand is read; if it could rebind
    /// the local (a walrus), the local is copied first.
    fn operand(&mut self, e: &'a Expr, clobber: Option<&Expr>) -> R<u8> {
        if let ExprKind::Name(n) = &e.kind
            && let Access::Local(r) = self.resolve(n)
            && self.is_bound(r)
            && !clobber.is_some_and(has_walrus)
        {
            return Ok(r);
        }
        self.expr(e)?;
        let t = self.temp()?;
        self.emit_r(op::STAR, t);
        Ok(t)
    }

    /// Evaluates each expression into consecutive temps and builds a
    /// sequence with `opcode`; the result goes to a fresh temp.
    fn build_seq(&mut self, items: &[&'a Expr], opcode: u8) -> R<u8> {
        let m = self.mark();
        let n = items.len();
        let base = self.temps(n.max(1))?;
        for (k, e) in items.iter().enumerate() {
            self.expr(e)?;
            self.emit_r(op::STAR, base + u8::try_from(k).unwrap_or(0));
        }
        let k = u16::try_from(n).map_err(|_| too_many("displays this large"))?;
        self.emit_ru(opcode, base, k);
        self.release(m);
        let r = self.temp()?;
        self.emit_r(op::STAR, r);
        Ok(r)
    }

    fn expr(&mut self, e: &'a Expr) -> R<()> {
        self.enter(e.pos)?;
        let m = self.mark();
        if chain_operand(e).and_then(chain_operand).is_some() {
            self.chain(e)?;
        } else {
            self.expr_inner(e)?;
        }
        self.release(m);
        self.depth -= 1;
        Ok(())
    }

    /// A left-nested chain (`a + b + c`, `x.f(1).g[0]`): its innermost
    /// step as usual, then each step above it on the value before, in a
    /// loop. However long, it is one level of nesting and recurses only
    /// into the other operands.
    fn chain(&mut self, e: &'a Expr) -> R<()> {
        let mut steps = vec![e];
        let mut cur = e;
        while let Some(c) = chain_operand(cur).filter(|c| chain_operand(c).is_some()) {
            steps.push(c);
            cur = c;
        }
        steps.pop();
        let m = self.mark();
        self.expr_inner(cur)?;
        self.release(m);
        for s in steps.into_iter().rev() {
            let m = self.mark();
            self.chain_step(s)?;
            self.release(m);
        }
        Ok(())
    }

    /// Step `e` of a chain with its first operand's value in the
    /// accumulator.
    fn chain_step(&mut self, e: &'a Expr) -> R<()> {
        match &e.kind {
            ExprKind::BinOp(_, bop, b) => {
                let t = self.temp()?;
                self.emit_r(op::STAR, t);
                self.expr(b)?;
                self.at(e);
                self.binop(*bop, t);
            }
            ExprKind::Attribute(_, attr) => {
                let n = self.name(attr)?;
                let ic = self.ic()?;
                self.at(e);
                self.emit_uu(op::GET_ATTR, n, ic);
            }
            ExprKind::Subscript(_, idx) => {
                let t = self.temp()?;
                self.emit_r(op::STAR, t);
                self.subscript_index(idx)?;
                self.at(e);
                self.emit_r(op::GET_ITEM, t);
            }
            ExprKind::Call(func, args, kws) => self.call_tail(e, func, args, kws)?,
            _ => return Err(too_many("this chain (no step)")),
        }
        Ok(())
    }

    fn binop(&mut self, bop: BinOp, ra: u8) {
        match bop {
            BinOp::Add => self.emit_r(op::ADD, ra),
            BinOp::Sub => self.emit_r(op::SUB, ra),
            BinOp::Mul => self.emit_r(op::MUL, ra),
            _ => self.emit_rb(op::BINOP, bop as u8, ra),
        }
    }

    #[allow(clippy::too_many_lines, reason = "one arm per expression kind")]
    fn expr_inner(&mut self, e: &'a Expr) -> R<()> {
        match &e.kind {
            ExprKind::Const(c) => {
                self.at(e);
                self.constant(c)
            }
            ExprKind::Name(n) => {
                self.at(e);
                self.load_name(n)
            }
            ExprKind::BinOp(a, bop, b) => {
                let ra = self.operand(a, Some(b))?;
                self.expr(b)?;
                self.at(e);
                self.binop(*bop, ra);
                Ok(())
            }
            ExprKind::Unary(uop, a) => {
                self.expr(a)?;
                self.at(e);
                self.emit(match uop {
                    UnaryOp::Neg => op::NEG,
                    UnaryOp::Pos => op::POS,
                    UnaryOp::Not => op::NOT,
                    UnaryOp::Invert => op::INVERT,
                });
                Ok(())
            }
            ExprKind::BoolOp(bop, values) => {
                let end = self.label();
                let jump = if *bop == BoolOp::And {
                    op::JUMP_IF_FALSE
                } else {
                    op::JUMP_IF_TRUE
                };
                for (i, v) in values.iter().enumerate() {
                    self.expr(v)?;
                    if i + 1 < values.len() {
                        self.jump(jump, end);
                    }
                }
                self.bind(end);
                Ok(())
            }
            ExprKind::Compare(first, rest) => self.compare(e, first, rest),
            ExprKind::IfExp(..) => {
                // `a if x else b if y else c` in a loop: a conditional in
                // an `else` is part of the chain, not a nested level.
                let end = self.label();
                let mut cur = e;
                while let ExprKind::IfExp(test, body, orelse) = &cur.kind {
                    let els = self.label();
                    self.cond_jump(test, false, els)?;
                    self.expr(body)?;
                    self.jump(op::JUMP, end);
                    self.bind(els);
                    cur = orelse;
                }
                self.expr(cur)?;
                self.bind(end);
                Ok(())
            }
            ExprKind::Lambda(f) => self.make_function(f, "<lambda>"),
            ExprKind::Call(func, args, kws) => self.call(e, func, args, kws),
            ExprKind::Attribute(obj, attr) => {
                self.expr(obj)?;
                let n = self.name(attr)?;
                let ic = self.ic()?;
                self.at(e);
                self.emit_uu(op::GET_ATTR, n, ic);
                Ok(())
            }
            ExprKind::Subscript(obj, idx) => {
                let o = self.operand(obj, Some(idx))?;
                self.subscript_index(idx)?;
                self.at(e);
                self.emit_r(op::GET_ITEM, o);
                Ok(())
            }
            ExprKind::Slice(..) => Err(CompileError::syntax("invalid slice".to_string(), e.pos)),
            ExprKind::Starred(_) => Err(CompileError::syntax(
                "can't use starred expression here".to_string(),
                e.pos,
            )),
            ExprKind::List(items) => self.display(e, items, op::BUILD_LIST),
            ExprKind::Tuple(items) => self.display(e, items, op::BUILD_TUPLE),
            ExprKind::Set(items) => self.display(e, items, op::BUILD_SET),
            ExprKind::Dict(items) => self.dict_display(e, items),
            ExprKind::ListComp(elt, gens) => self.comprehension(e, gens, CompKind::List(elt)),
            ExprKind::SetComp(elt, gens) => self.comprehension(e, gens, CompKind::Set(elt)),
            ExprKind::DictComp(k, v, gens) => self.comprehension(e, gens, CompKind::Dict(k, v)),
            ExprKind::GenExp(elt, gens) => self.genexp(e, elt, gens),
            ExprKind::FString(parts) => self.fstring(e, parts),
            ExprKind::Named(n, v) => {
                self.expr(v)?;
                self.at(e);
                self.store_name(n)
            }
            ExprKind::Yield(v) => {
                match v {
                    Some(v) => self.expr(v)?,
                    None => self.emit(op::LDA_NONE),
                }
                self.at(e);
                self.emit(op::YIELD);
                Ok(())
            }
            ExprKind::YieldFrom(v) => {
                if self.ur().code.is_coroutine {
                    return Err(CompileError::syntax(
                        "'yield from' inside async function".to_string(),
                        e.pos,
                    ));
                }
                self.expr(v)?;
                self.at(e);
                self.emit(op::GET_YIELD_FROM_ITER);
                let it = self.temp()?;
                self.emit_r(op::STAR, it);
                self.emit(op::LDA_NONE);
                let (top, exit) = (self.label(), self.label());
                self.bind_top(top);
                self.jump_r(op::SEND, it, exit);
                self.emit_r(op::YIELD_FROM, it);
                self.jump(op::JUMP_LOOP, top);
                self.bind(exit);
                Ok(())
            }
            ExprKind::Await(v) => {
                // Top-level `await` is allowed in module code (cells).
                let u = self.ur();
                if !u.code.is_module && !u.code.is_coroutine {
                    return Err(CompileError::syntax(
                        "'await' outside async function".to_string(),
                        e.pos,
                    ));
                }
                self.expr(v)?;
                self.at(e);
                self.emit(op::AWAIT);
                Ok(())
            }
        }
    }

    fn constant(&mut self, c: &Const) -> R<()> {
        match c {
            Const::None => self.emit(op::LDA_NONE),
            Const::Bool(true) => self.emit(op::LDA_TRUE),
            Const::Bool(false) => self.emit(op::LDA_FALSE),
            Const::Int(n) if i16::try_from(*n).is_ok() => {
                self.emit(op::LDA_SMI);
                self.word(i16::try_from(*n).unwrap_or(0).cast_unsigned());
            }
            _ => {
                let k = self.konst(match c {
                    Const::Ellipsis => Konst::Ellipsis,
                    Const::Int(n) => Konst::Int(*n),
                    Const::Float(f) => Konst::Float(*f),
                    Const::Str(s) => Konst::Str(s.clone()),
                    Const::None => Konst::None,
                    Const::Bool(b) => Konst::Bool(*b),
                    Const::BigInt(s) => Konst::BigInt(s.clone()),
                    Const::Bytes(b) => Konst::Bytes(b.clone()),
                    Const::Complex(f) => Konst::Complex(0.0, *f),
                })?;
                self.emit_u(op::LDA_CONST, k);
            }
        }
        Ok(())
    }

    /// Evaluates a subscript index (or slice) into the accumulator.
    fn subscript_index(&mut self, idx: &'a Expr) -> R<()> {
        if let ExprKind::Slice(lo, hi, step) = &idx.kind {
            let m = self.mark();
            let base = self.temps(3)?;
            for (k, part) in [lo, hi, step].into_iter().enumerate() {
                match part {
                    Some(p) => self.expr(p)?,
                    None => self.emit(op::LDA_NONE),
                }
                self.emit_r(op::STAR, base + u8::try_from(k).unwrap_or(0));
            }
            self.at(idx);
            self.emit_r(op::BUILD_SLICE, base);
            self.release(m);
            Ok(())
        } else if let ExprKind::Tuple(items) = &idx.kind
            && items.iter().any(|x| matches!(x.kind, ExprKind::Slice(..)))
        {
            // `a[1:2, ::3]`: a tuple whose items may be slices.
            let m = self.mark();
            let base = self.temps(items.len())?;
            for (k, x) in items.iter().enumerate() {
                self.subscript_index(x)?;
                self.emit_r(op::STAR, base + u8::try_from(k).unwrap_or(0));
            }
            let k = u16::try_from(items.len()).map_err(|_| too_many("subscripts this large"))?;
            self.at(idx);
            self.emit_ru(op::BUILD_TUPLE, base, k);
            self.release(m);
            Ok(())
        } else {
            self.expr(idx)
        }
    }

    fn compare(&mut self, e: &'a Expr, first: &'a Expr, rest: &'a [(CmpOp, Expr)]) -> R<()> {
        let end = self.label();
        let mut left = self.operand(first, rest.first().map(|(_, x)| x))?;
        for (i, (cop, right)) in rest.iter().enumerate() {
            self.expr(right)?;
            let last = i + 1 == rest.len();
            let mut saved = None;
            if !last {
                let t = self.temp()?;
                self.emit_r(op::STAR, t);
                saved = Some(t);
            }
            self.at(e);
            self.emit_r(cmp_opcode(*cop), left);
            if let Some(t) = saved {
                self.jump(op::JUMP_IF_FALSE, end);
                left = t;
            }
        }
        self.bind(end);
        Ok(())
    }

    /// Jumps to `target` when `test` is `when`; falls through otherwise.
    fn cond_jump(&mut self, test: &'a Expr, when: bool, target: Label) -> R<()> {
        match &test.kind {
            ExprKind::Unary(UnaryOp::Not, inner) => self.cond_jump(inner, !when, target),
            ExprKind::BoolOp(bop, values) if values.len() > 1 => {
                self.enter(test.pos)?;
                let is_and = *bop == BoolOp::And;
                // and+false / or+true: any short-circuit operand jumps.
                if is_and == when {
                    let skip = self.label();
                    let n = values.len();
                    for (i, v) in values.iter().enumerate() {
                        if i + 1 < n {
                            self.cond_jump(v, !when, skip)?;
                        } else {
                            self.cond_jump(v, when, target)?;
                        }
                    }
                    self.bind(skip);
                } else {
                    for v in values {
                        self.cond_jump(v, when, target)?;
                    }
                }
                self.depth -= 1;
                Ok(())
            }
            _ => {
                self.expr(test)?;
                self.jump(
                    if when {
                        op::JUMP_IF_TRUE
                    } else {
                        op::JUMP_IF_FALSE
                    },
                    target,
                );
                Ok(())
            }
        }
    }

    fn display(&mut self, e: &'a Expr, items: &'a [Expr], build: u8) -> R<()> {
        let has_star = items.iter().any(|x| matches!(x.kind, ExprKind::Starred(_)));
        // Large displays are built incrementally so they need few registers.
        if !has_star && items.len() <= LARGE_DISPLAY {
            let refs: Vec<&'a Expr> = items.iter().collect();
            let m = self.mark();
            let n = refs.len();
            let base = self.temps(n.max(1))?;
            for (k, x) in refs.iter().enumerate() {
                self.expr(x)?;
                self.emit_r(op::STAR, base + u8::try_from(k).unwrap_or(0));
            }
            let k = u16::try_from(n).map_err(|_| too_many("displays this large"))?;
            self.at(e);
            self.emit_ru(build, base, k);
            self.release(m);
            return Ok(());
        }
        let (start, add, extend) = if build == op::BUILD_SET {
            (op::BUILD_SET, op::SET_ADD, op::SET_UPDATE)
        } else {
            (op::BUILD_LIST, op::LIST_APPEND, op::LIST_EXTEND)
        };
        let acc = self.temp()?;
        self.at(e);
        self.emit_ru(start, acc, 0);
        self.emit_r(op::STAR, acc);
        for x in items {
            if let ExprKind::Starred(inner) = &x.kind {
                self.expr(inner)?;
                self.at(x);
                self.emit_r(extend, acc);
                if extend == op::LIST_EXTEND {
                    self.byte(0);
                }
            } else {
                self.expr(x)?;
                self.emit_r(add, acc);
            }
        }
        self.emit_r(op::LDAR, acc);
        if build == op::BUILD_TUPLE {
            self.emit(op::LIST_TO_TUPLE);
        }
        Ok(())
    }

    fn dict_display(&mut self, e: &'a Expr, items: &'a [(Option<Expr>, Expr)]) -> R<()> {
        let has_spread = items.iter().any(|(k, _)| k.is_none());
        if !has_spread && items.len() <= LARGE_DISPLAY / 2 {
            let m = self.mark();
            let n = items.len();
            let base = self.temps((2 * n).max(1))?;
            for (k, (key, value)) in items.iter().enumerate() {
                let kr =
                    base + u8::try_from(2 * k).map_err(|_| too_many("dict displays this large"))?;
                if let Some(key) = key {
                    self.expr(key)?;
                }
                self.emit_r(op::STAR, kr);
                self.expr(value)?;
                self.emit_r(op::STAR, kr + 1);
            }
            let k = u16::try_from(n).map_err(|_| too_many("dict displays this large"))?;
            self.at(e);
            self.emit_ru(op::BUILD_DICT, base, k);
            self.release(m);
            return Ok(());
        }
        let d = self.temp()?;
        self.at(e);
        self.emit_ru(op::BUILD_DICT, d, 0);
        self.emit_r(op::STAR, d);
        for (key, value) in items {
            if let Some(key) = key {
                let m = self.mark();
                let kr = self.temp()?;
                self.expr(key)?;
                self.emit_r(op::STAR, kr);
                self.expr(value)?;
                self.emit_rr(op::DICT_INSERT, d, kr);
                self.release(m);
            } else {
                self.expr(value)?;
                self.at(value);
                self.emit_rb(op::DICT_UPDATE, d, 0);
                self.byte(0);
            }
        }
        self.emit_r(op::LDAR, d);
        Ok(())
    }

    /// Zero-argument `super()` inside a function with the `__class__` cell
    /// becomes `super(__class__, <first argument>)`.
    fn zero_arg_super(&mut self, e: &'a Expr, func: &'a Expr) -> R<bool> {
        let u = self.ur();
        if u.is_class || u.code.is_module || u.code.params.total() == 0 {
            return Ok(false);
        }
        if !matches!(self.resolve("__class__"), Access::Deref(_))
            || !matches!(self.resolve("super"), Access::Global)
        {
            return Ok(false);
        }
        let Some(first) = self.ur().code.varnames.first().cloned() else {
            return Ok(false);
        };
        let base = self.temps(4)?;
        self.expr(func)?;
        self.emit_r(op::STAR, base);
        self.load_name("__class__")?;
        self.emit_r(op::STAR, base + 2);
        self.load_name(&first)?;
        self.emit_r(op::STAR, base + 3);
        let ic = self.ic()?;
        self.at(e);
        self.emit_rbu(op::CALL, base, 2, ic);
        Ok(true)
    }

    /// Zero-argument `dir()` in a function or class body passes its scope
    /// as the hidden keyword `<scope>`: the function's variable names, one
    /// per register (the builtin reads which are bound from its caller's
    /// registers), or the class namespace. At module level `dir()` lists
    /// the globals. An inlined comprehension's own variables are left out.
    fn zero_arg_dir(&mut self, e: &'a Expr, func: &'a Expr) -> R<bool> {
        if self.ur().code.is_module || !matches!(self.resolve("dir"), Access::Global | Access::Name)
        {
            return Ok(false);
        }
        let base = self.temps(3)?;
        self.expr(func)?;
        self.emit_r(op::STAR, base);
        if self.ur().is_class {
            self.emit_r(op::LDAR, 0);
        } else {
            let names = self
                .ur()
                .code
                .varnames
                .iter()
                .map(|n| String::from(&**n))
                .collect();
            let k = self.konst(Konst::Names(names))?;
            self.emit_u(op::LDA_CONST, k);
        }
        self.emit_r(op::STAR, base + 2);
        let k = self.konst(Konst::Names(vec![String::from("<scope>")]))?;
        self.at(e);
        self.emit_rbu(op::CALL_KW, base, 1, k);
        Ok(true)
    }

    fn call(&mut self, e: &'a Expr, func: &'a Expr, args: &'a [Expr], kws: &'a [Keyword]) -> R<()> {
        if args.is_empty()
            && kws.is_empty()
            && matches!(&func.kind, ExprKind::Name(n) if n == "super")
            && self.zero_arg_super(e, func)?
        {
            return Ok(());
        }
        if args.is_empty()
            && kws.is_empty()
            && matches!(&func.kind, ExprKind::Name(n) if n == "dir")
            && self.zero_arg_dir(e, func)?
        {
            return Ok(());
        }
        self.expr(method_object(func, args, kws).unwrap_or(func))?;
        self.call_tail(e, func, args, kws)
    }

    /// Call `e` with its callee, or a method call's object, in the
    /// accumulator.
    fn call_tail(
        &mut self,
        e: &'a Expr,
        func: &'a Expr,
        args: &'a [Expr],
        kws: &'a [Keyword],
    ) -> R<()> {
        if star_call(args, kws) {
            return self.call_ex(e, args, kws);
        }
        let nargs = args.len() + kws.len();
        let argc = u8::try_from(nargs).map_err(|_| too_many("calls with this many arguments"))?;
        let kwnames = if kws.is_empty() {
            None
        } else {
            let names: Vec<String> = kws
                .iter()
                .map(|k| k.name.clone().unwrap_or_default())
                .collect();
            for (i, n) in names.iter().enumerate() {
                if names[..i].contains(n) {
                    return Err(CompileError::syntax(
                        format!("keyword argument repeated: {n}"),
                        kws[i].pos,
                    ));
                }
            }
            Some(self.konst(Konst::Names(names))?)
        };
        if let ExprKind::Attribute(_, attr) = &func.kind {
            let base = self.temps(nargs + 3)?;
            let n = self.name(attr)?;
            let ic = self.ic()?;
            self.at(func);
            self.emit(op::LOAD_METHOD);
            self.byte(base);
            self.word(n);
            self.word(ic);
            self.call_args(base + 3, args, kws)?;
            let ic = self.ic()?;
            self.at(e);
            match kwnames {
                None => self.emit_rbu(op::CALL_METHOD, base, argc, ic),
                Some(k) => self.emit_rbu(op::CALL_METHOD_KW, base, argc, k),
            }
        } else {
            let base = self.temps(nargs + 2)?;
            self.emit_r(op::STAR, base);
            self.call_args(base + 2, args, kws)?;
            let ic = self.ic()?;
            self.at(e);
            match kwnames {
                None => self.emit_rbu(op::CALL, base, argc, ic),
                Some(k) => self.emit_rbu(op::CALL_KW, base, argc, k),
            }
        }
        Ok(())
    }

    fn call_args(&mut self, base: u8, args: &'a [Expr], kws: &'a [Keyword]) -> R<()> {
        let values = args.iter().chain(kws.iter().map(|k| &k.value));
        for (k, a) in values.enumerate() {
            self.expr(a)?;
            self.emit_r(op::STAR, base + u8::try_from(k).unwrap_or(0));
        }
        Ok(())
    }

    /// A call with `*args`, `**kwargs` or past 200 arguments, its callee
    /// in the accumulator.
    fn call_ex(&mut self, e: &'a Expr, args: &'a [Expr], kws: &'a [Keyword]) -> R<()> {
        let base = self.temps(3)?;
        self.emit_r(op::STAR, base);
        // Positional arguments as a tuple, or as a list that `CALL_EX`
        // copies into the callee's `*args`, with no tuple in between.
        let has_star = args.iter().any(|a| matches!(a.kind, ExprKind::Starred(_)));
        if has_star || args.len() > 200 {
            let l = base + 1;
            self.emit_ru(op::BUILD_LIST, l, 0);
            self.emit_r(op::STAR, l);
            for a in args {
                if let ExprKind::Starred(inner) = &a.kind {
                    self.expr(inner)?;
                    self.at(a);
                    self.emit_rb(op::LIST_EXTEND, l, 1);
                } else {
                    self.expr(a)?;
                    self.emit_r(op::LIST_APPEND, l);
                }
            }
        } else {
            let refs: Vec<&'a Expr> = args.iter().collect();
            let t = self.build_seq(&refs, op::BUILD_TUPLE)?;
            self.emit_rr(op::MOV, t, base + 1);
            self.release(u32::from(base) + 3);
        }
        // Keywords as a dict.
        let d = base + 2;
        if kws.is_empty() {
            self.emit(op::LDA_NONE);
            self.emit_r(op::STAR, d);
        } else {
            self.emit_ru(op::BUILD_DICT, d, 0);
            self.emit_r(op::STAR, d);
            for k in kws {
                if let Some(n) = &k.name {
                    let m = self.mark();
                    let kr = self.temp()?;
                    let c = self.konst(Konst::Str(n.clone()))?;
                    self.emit_u(op::LDA_CONST, c);
                    self.emit_r(op::STAR, kr);
                    self.expr(&k.value)?;
                    self.emit_rb(op::DICT_UPDATE, d, 2);
                    // DICT_UPDATE mode 2: insert R[kr] = acc, rejecting duplicates.
                    self.byte(kr);
                    self.release(m);
                } else {
                    self.expr(&k.value)?;
                    self.at(&k.value);
                    self.emit_rb(op::DICT_UPDATE, d, 1);
                    self.byte(0);
                }
            }
        }
        self.at(e);
        self.emit_r(op::CALL_EX, base);
        Ok(())
    }

    fn fstring(&mut self, e: &'a Expr, parts: &'a [FPart]) -> R<()> {
        if let [FPart::Lit(s)] = parts {
            self.at(e);
            let k = self.konst(Konst::Str(s.clone()))?;
            self.emit_u(op::LDA_CONST, k);
            return Ok(());
        }
        if parts.len() <= FSTRING_GROUP {
            return self.fstring_parts(e, parts);
        }
        // Long f-strings in groups, each joined to the text so far, so
        // they need few registers.
        let m = self.mark();
        let text = self.temp()?;
        for (g, group) in parts.chunks(FSTRING_GROUP).enumerate() {
            if g > 0 {
                self.emit_r(op::STAR, text);
            }
            self.fstring_parts(e, group)?;
            if g > 0 {
                let t = self.temp()?;
                self.emit_r(op::STAR, t);
                self.at(e);
                self.emit_ru(op::BUILD_STRING, text, 2);
                self.release(u32::from(t));
            }
        }
        self.release(m);
        Ok(())
    }

    /// The parts of an f-string, each in a register, then joined.
    fn fstring_parts(&mut self, e: &'a Expr, parts: &'a [FPart]) -> R<()> {
        let m = self.mark();
        let n = parts.len();
        // A lone field is its formatted str, joined to nothing (CPython's
        // `FORMAT_SIMPLE` or `FORMAT_WITH_SPEC` alone): no copy of a long one.
        let lone = matches!(parts, [FPart::Field { .. }]);
        let base = self.temps(n.max(1))?;
        for (k, p) in parts.iter().enumerate() {
            let r = base + u8::try_from(k).map_err(|_| too_many("f-strings this large"))?;
            match p {
                FPart::Lit(s) => {
                    let c = self.konst(Konst::Str(s.clone()))?;
                    self.emit_u(op::LDA_CONST, c);
                }
                FPart::Field {
                    expr,
                    conv,
                    spec,
                    pos,
                } => {
                    self.expr(expr)?;
                    // A lone field without a conversion is whatever
                    // `__format__` returns, a str subclass included: no
                    // conversion byte 1, which `_op_fmt` keeps unconverted.
                    let conv = if lone && *conv == 0 { 1 } else { *conv };
                    match spec {
                        None => {
                            self.at_pos(*pos);
                            self.emit(op::FORMAT);
                            self.byte(conv);
                        }
                        Some(spec) => {
                            let v = self.temp()?;
                            self.emit_r(op::STAR, v);
                            self.fstring(e, spec)?;
                            self.at_pos(*pos);
                            self.emit_rb(op::FORMAT_SPEC, v, conv);
                        }
                    }
                }
            }
            if !lone {
                self.emit_r(op::STAR, r);
            }
        }
        if !lone {
            let k = u16::try_from(n).map_err(|_| too_many("f-strings this large"))?;
            self.at(e);
            self.emit_ru(op::BUILD_STRING, base, k);
        }
        self.release(m);
        Ok(())
    }

    // ------------------------------------------------ comprehensions

    fn comprehension(
        &mut self,
        e: &'a Expr,
        gens: &'a [Comprehension],
        kind: CompKind<'a>,
    ) -> R<()> {
        let Some(scope) = self.st.scope_of(symtable::expr_key(e)) else {
            return Err(too_many("this comprehension (no scope)"));
        };
        let Some(first) = gens.first() else {
            return Err(CompileError::syntax(
                "empty comprehension".to_string(),
                e.pos,
            ));
        };
        let result = self.temp()?;
        self.expr(&first.iter)?;
        self.at(&first.iter);
        self.emit(if first.is_async {
            op::GET_AITER
        } else {
            op::GET_ITER
        });
        let it0 = self.temp()?;
        self.emit_r(op::STAR, it0);
        self.at(e);
        let build = match kind {
            CompKind::List(_) => op::BUILD_LIST,
            CompKind::Set(_) => op::BUILD_SET,
            CompKind::Dict(..) => op::BUILD_DICT,
        };
        self.emit_ru(build, result, 0);
        self.emit_r(op::STAR, result);
        self.comp_loops(scope, gens, it0, result, kind, e.pos)?;
        self.emit_r(op::LDAR, result);
        Ok(())
    }

    /// Enters comprehension scope `scope` (registers for its variables), runs
    /// the loops and adds each element to `result`, then leaves the scope.
    fn comp_loops(
        &mut self,
        scope: ScopeId,
        gens: &'a [Comprehension],
        it0: u8,
        result: u8,
        kind: CompKind<'a>,
        pos: Pos,
    ) -> R<()> {
        let sc = &self.st.scopes[scope];
        let mut vars = Vec::new();
        let saved_bound = self.ur().bound.len();
        for (i, n) in sc.names.iter().enumerate() {
            if matches!(sc.bindings[i], Binding::Local | Binding::Cell) {
                let r = self.temp()?;
                self.u().set_reg(scope, n, r);
                vars.push((r, n.clone(), sc.bindings[i] == Binding::Cell));
            }
        }
        // The registers are reused temporaries: start them unbound (cells
        // fresh and empty) so a read before assignment raises.
        for (r, _, cell) in &vars {
            self.emit(op::LDA_NONE);
            self.emit_r(op::STAR, *r);
            self.emit_r(op::DEL_LOCAL, *r);
            if *cell {
                self.emit_r(op::MAKE_CELL, *r);
            }
        }
        let start = self.pc();
        self.u().comps.push(scope);
        self.u().block_depth += 1;
        let mut tops = Vec::new();
        for (k, g) in gens.iter().enumerate() {
            tops.push(self.comp_head(k, g, it0, pos)?);
        }
        match kind {
            CompKind::List(elt) => {
                self.expr(elt)?;
                self.emit_r(op::LIST_APPEND, result);
            }
            CompKind::Set(elt) => {
                self.expr(elt)?;
                self.emit_r(op::SET_ADD, result);
            }
            CompKind::Dict(k, v) => {
                let m = self.mark();
                let kr = self.temp()?;
                self.expr(k)?;
                self.emit_r(op::STAR, kr);
                self.expr(v)?;
                self.emit_rr(op::DICT_INSERT, result, kr);
                self.release(m);
            }
        }
        self.comp_tails(tops);
        let end = self.pc();
        for (reg, name, _) in vars {
            self.u().code.reg_names.push(RegName {
                start,
                end,
                reg,
                name,
            });
        }
        self.u().block_depth -= 1;
        self.u().comps.pop();
        self.u().bound.truncate(saved_bound);
        Ok(())
    }

    /// One `[async] for ... in ... if ...` clause of a comprehension, `k`
    /// in order (the first one's iterator is ready in `it0`): the loop
    /// head, the target and the filters.
    fn comp_head(&mut self, k: usize, g: &'a Comprehension, it0: u8, pos: Pos) -> R<CompLoop<'a>> {
        let it = if k == 0 {
            it0
        } else {
            self.expr(&g.iter)?;
            self.at(&g.iter);
            self.emit(if g.is_async {
                op::GET_AITER
            } else {
                op::GET_ITER
            });
            let t = self.temp()?;
            self.emit_r(op::STAR, t);
            t
        };
        let (top, end) = (self.label(), self.label());
        let handler = if g.is_async {
            self.async_ok(
                "asynchronous comprehension outside of an asynchronous function",
                pos,
            )?;
            Some(self.anext_step(it, top, pos))
        } else {
            self.bind_top(top);
            self.at(&g.iter);
            self.jump_r(op::FOR_ITER, it, end);
            None
        };
        self.store(&g.target)?;
        self.bind_targets(&g.target);
        for cond in &g.ifs {
            let skip = self.label();
            self.cond_jump(cond, true, skip)?;
            self.at(cond);
            self.jump(op::JUMP_LOOP, top);
            self.bind(skip);
        }
        Ok((top, end, g, handler))
    }

    /// The loop ends of [`Self::comp_head`]'s clauses, innermost first.
    fn comp_tails(&mut self, tops: Vec<CompLoop<'a>>) {
        for (top, end, g, handler) in tops.into_iter().rev() {
            self.at(&g.iter);
            self.jump(op::JUMP_LOOP, top);
            if let Some(h) = handler {
                self.bind(h);
                self.emit(op::END_ASYNC_FOR);
            }
            self.bind(end);
        }
    }

    /// An `async for` step at loop head `top` over the async iterator in
    /// `it`: `acc = await it.__anext__()`, under a handler (returned) that
    /// ends the loop on `StopAsyncIteration`.
    fn anext_step(&mut self, it: u8, top: Label, pos: Pos) -> Label {
        let h = self.label();
        self.bind_top(top);
        self.push_handler(h);
        self.at_pos(pos);
        self.emit_r(op::ANEXT, it);
        self.emit(op::AWAIT);
        self.pop_handler();
        h
    }

    /// `async for`, `async with` and async comprehensions need an async
    /// function (or module code, as top-level `await` in cells).
    fn async_ok(&self, msg: &str, pos: Pos) -> R<()> {
        let u = self.ur();
        if u.code.is_module || u.code.is_coroutine {
            Ok(())
        } else {
            Err(CompileError::syntax(msg.to_string(), pos))
        }
    }

    fn genexp(&mut self, e: &'a Expr, elt: &'a Expr, gens: &'a [Comprehension]) -> R<()> {
        let Some(scope) = self.st.scope_of(symtable::expr_key(e)) else {
            return Err(too_many("this generator expression (no scope)"));
        };
        let pos = e.pos;
        let Some(first) = gens.first() else {
            return Err(CompileError::syntax("empty generator".to_string(), e.pos));
        };
        let base = self.temps(3)?;
        // The function: `.0` is its iterator parameter.
        let idx = self.codes.len();
        self.codes.push(None);
        let qual = self.qualname("<genexpr>");
        self.begin_unit(scope, "<genexpr>", &qual, e.pos.line, false)?;
        self.u().code.params = ParamSpec {
            argcount: 1,
            ..ParamSpec::default()
        };
        let it0 = self.ur().reg(scope, ".0").unwrap_or(0);
        self.u().code.is_generator = true;
        self.u().code.is_coroutine = self.st.scopes[scope].coroutine;
        // The generator's own variables are its locals; run the loops directly.
        self.u().block_depth += 1;
        let mut tops = Vec::new();
        for (k, g) in gens.iter().enumerate() {
            tops.push(self.comp_head(k, g, it0, pos)?);
        }
        self.expr(elt)?;
        self.at(elt);
        self.emit(op::YIELD);
        self.comp_tails(tops);
        self.u().block_depth -= 1;
        self.at(e);
        self.emit(op::LDA_NONE);
        self.emit(op::RETURN);
        let code = self.end_unit()?;
        let freenames = code.freenames.clone();
        self.codes[idx] = Some(code);

        // The call site: iter(first.iter), then call the new function with it.
        self.expr(&first.iter)?;
        self.at(&first.iter);
        self.emit(if first.is_async {
            op::GET_AITER
        } else {
            op::GET_ITER
        });
        self.emit_r(op::STAR, base + 2);
        // As in `class_def`: the closure temp dies at MAKE_FUNCTION.
        let mf = self.mark();
        let mut flags = 0;
        let parts = if freenames.is_empty() {
            base
        } else {
            flags = 4;
            self.closure(&freenames)?
        };
        let k = u16::try_from(idx).map_err(|_| too_many("more than 65535 functions"))?;
        self.at(e);
        self.emit(op::MAKE_FUNCTION);
        self.word(k);
        self.byte(flags);
        self.byte(parts);
        self.release(mf);
        self.emit_r(op::STAR, base);
        let ic = self.ic()?;
        self.emit_rbu(op::CALL, base, 1, ic);
        Ok(())
    }
}

#[derive(Clone, Copy)]
enum CompKind<'a> {
    List(&'a Expr),
    Set(&'a Expr),
    Dict(&'a Expr, &'a Expr),
}

fn cmp_opcode(c: CmpOp) -> u8 {
    match c {
        CmpOp::Eq => op::TEST_EQ,
        CmpOp::Ne => op::TEST_NE,
        CmpOp::Lt => op::TEST_LT,
        CmpOp::Le => op::TEST_LE,
        CmpOp::Gt => op::TEST_GT,
        CmpOp::Ge => op::TEST_GE,
        CmpOp::In => op::TEST_IN,
        CmpOp::NotIn => op::TEST_NOT_IN,
        CmpOp::Is => op::TEST_IS,
        CmpOp::IsNot => op::TEST_IS_NOT,
    }
}

/// Could evaluating `e` rebind a local (does it contain a walrus)?
/// How many parts of an f-string are joined at once (`Compiler::fstring`).
const FSTRING_GROUP: usize = 64;

/// Whether a call passes `*args`, `**kwargs` or more than 200 arguments
/// (built as a tuple and a dict, `call_ex`).
fn star_call(args: &[Expr], kws: &[Keyword]) -> bool {
    args.iter().any(|a| matches!(a.kind, ExprKind::Starred(_)))
        || kws.iter().any(|k| k.name.is_none())
        || args.len() + kws.len() > 200
}

/// The object of a method call `obj.m(...)` (`LOAD_METHOD`), if `func` and
/// the arguments make one.
fn method_object<'e>(func: &'e Expr, args: &[Expr], kws: &[Keyword]) -> Option<&'e Expr> {
    match &func.kind {
        ExprKind::Attribute(obj, _) if !star_call(args, kws) => Some(obj),
        _ => None,
    }
}

/// The operand a chain step evaluates first: `a` of `a + b`, `a.x` and
/// `a[i]`, a call's callee or a method call's object.
fn chain_operand(e: &Expr) -> Option<&Expr> {
    match &e.kind {
        ExprKind::BinOp(a, _, _) | ExprKind::Attribute(a, _) | ExprKind::Subscript(a, _) => Some(a),
        ExprKind::Call(func, args, kws) => Some(method_object(func, args, kws).unwrap_or(func)),
        _ => None,
    }
}

fn has_walrus(e: &Expr) -> bool {
    let mut stack: Vec<&Expr> = vec![e];
    while let Some(x) = stack.pop() {
        match &x.kind {
            // Comprehension walruses bind in the enclosing scope.
            ExprKind::Named(..)
            | ExprKind::ListComp(..)
            | ExprKind::SetComp(..)
            | ExprKind::DictComp(..)
            | ExprKind::GenExp(..) => return true,
            ExprKind::Const(_) | ExprKind::Name(_) | ExprKind::Lambda(_) => {}
            ExprKind::BinOp(a, _, b) | ExprKind::Subscript(a, b) => {
                stack.push(a);
                stack.push(b);
            }
            ExprKind::Unary(_, a)
            | ExprKind::Attribute(a, _)
            | ExprKind::Starred(a)
            | ExprKind::Await(a)
            | ExprKind::YieldFrom(a) => {
                stack.push(a);
            }
            ExprKind::Yield(a) => {
                if let Some(a) = a {
                    stack.push(a);
                }
            }
            ExprKind::BoolOp(_, v) | ExprKind::List(v) | ExprKind::Tuple(v) | ExprKind::Set(v) => {
                stack.extend(v.iter());
            }
            ExprKind::Compare(a, rest) => {
                stack.push(a);
                stack.extend(rest.iter().map(|(_, x)| x));
            }
            ExprKind::IfExp(a, b, c) => {
                stack.push(a);
                stack.push(b);
                stack.push(c);
            }
            ExprKind::Call(f, args, kws) => {
                stack.push(f);
                stack.extend(args.iter());
                stack.extend(kws.iter().map(|k| &k.value));
            }
            ExprKind::Slice(a, b, c) => {
                stack.extend([a, b, c].into_iter().flatten().map(Box::as_ref));
            }
            ExprKind::Dict(items) => {
                for (k, v) in items {
                    if let Some(k) = k {
                        stack.push(k);
                    }
                    stack.push(v);
                }
            }
            ExprKind::FString(parts) => {
                let mut ps: Vec<&FPart> = parts.iter().collect();
                while let Some(p) = ps.pop() {
                    if let FPart::Field { expr, spec, .. } = p {
                        stack.push(expr);
                        if let Some(s) = spec {
                            ps.extend(s.iter());
                        }
                    }
                }
            }
        }
    }
    false
}

/// `range(...)` with one to three plain positional arguments, as the iterable
/// of a `for` statement: its callee and arguments.
fn range_call(e: &Expr) -> Option<(&Expr, &[Expr])> {
    match &e.kind {
        ExprKind::Call(f, args, kws)
            if kws.is_empty()
                && (1..=3).contains(&args.len())
                && matches!(&f.kind, ExprKind::Name(n) if n == "range")
                && !args.iter().any(|a| matches!(a.kind, ExprKind::Starred(_))) =>
        {
            Some((f, args))
        }
        _ => None,
    }
}
