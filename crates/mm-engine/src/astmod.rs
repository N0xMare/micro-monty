//! `ast._parse(source, mode, filename, module)`: CPython's `ast` node
//! tree, built from the own parser's tree ([`mm_parse::parse_tree`]) as
//! instances of the node classes `ast.py` binds on its module.
//!
//! The conversion follows the tree's nesting, which the parser bounds at
//! `MAX_NESTING` as it does for the compiler. Columns are UTF-8 byte
//! offsets, as CPython's. One fuel unit per node.

use crate::types::T;
use crate::vm::{CallArgs, Raised, Vm};
use alloc::{format, string::String, vec::Vec};
use mm_compile::{
    CompileError,
    ast::{
        BinOp, BoolOp, ClassDef, CmpOp, Comprehension, Const, Expr, ExprKind, FPart, FuncDef,
        Keyword, Param, Params, Pattern, PatternKind, Pos, Stmt, StmtKind, UnaryOp,
    },
};
use mm_core::Value;
use mm_parse::lines::Lines;

type R = Result<Value, Raised>;

/// The longest source `ast._parse` takes, in UTF-8 bytes: the lexer, the
/// parser and the node building run as one native step, and the worst
/// shape found at this size (a statement every two bytes) takes 29–38 ms
/// [M] (docs/spec.md §5). Longer sources raise `MemoryError`.
const MAX_SOURCE: usize = 32 * 1024;

#[derive(Clone, Copy, PartialEq)]
enum Ctx {
    Load,
    Store,
    Del,
}

struct Build<'a> {
    vm: &'a mut Vm,
    src: &'a str,
    /// [`mm_parse::string_segments`], computed at the first f-string.
    segs: Option<Vec<(u32, u32)>>,
    ns: Value,
    lines: Lines<'a>,
    /// `ast._one`: the shared context and operator instances.
    one: Value,
}

/// `ast._parse(source, mode, filename, module)`.
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn ast_parse(vm: &mut Vm, a: CallArgs) -> R {
    let (src, mode, file, ns) = (
        vm.arg_o(&a, 0),
        vm.arg_o(&a, 1),
        vm.arg_o(&a, 2),
        vm.arg_o(&a, 3),
    );
    let ns = vm.vslot_o(ns, crate::modules::module_slot::DICT);
    if !Vm::is_str(src) || !Vm::is_str(mode) || !ns.is_obj_kind(crate::types::ty::DICT as u8) {
        return Err(vm.err(
            T::TypeError,
            "expected str source and mode, and the ast module",
        ));
    }
    if vm.str_bytes(src).len() > MAX_SOURCE {
        return Err(vm.err(
            T::MemoryError,
            &format!("ast.parse() source larger than {} KiB", MAX_SOURCE / 1024),
        ));
    }
    // The native tree, built before any of it reaches the arena, and its
    // line table and string pieces: up to about 410 bytes per source byte
    // [M] (one-character statements with the statement vector just past a
    // doubling), held for the whole call.
    let res = vm
        .heap
        .reserve_scratch(vm.str_bytes(src).len().saturating_mul(512));
    let _native = vm.hx_o(res)?;
    let text = String::from(vm.str_ref_o(src));
    let mode = String::from(vm.str_ref_o(mode));
    if !matches!(mode.as_str(), "exec" | "eval" | "single") {
        return Err(vm.err(
            T::ValueError,
            "compile() mode must be 'exec', 'eval', 'single' or 'func_type'",
        ));
    }
    let tree = match mm_parse::parse_tree(&text, mode == "eval") {
        Ok(m) => m,
        Err(e) => return Err(syntax_error(vm, &e, &text, file, mode != "eval")),
    };
    let k = vm.intern("_one")?;
    let one = vm.dict_get_str(ns, k).unwrap_or(Value::NONE);
    let mut b = Build {
        vm,
        src: &text,
        segs: None,
        ns,
        lines: Lines::new(&text),
        one,
    };
    let body = &tree.body;
    if mode == "eval" {
        let e = match body.first().map(|s| &s.kind) {
            Some(StmtKind::Expr(e)) => b.expr(e, Ctx::Load)?,
            _ => Value::NONE,
        };
        return b.node("Expression", &[("body", e)], None);
    }
    let body = b.stmts(body)?;
    if mode == "single" {
        return b.node("Interactive", &[("body", body)], None);
    }
    let ti = b.vm.new_list(&[])?;
    b.node("Module", &[("body", body), ("type_ignores", ti)], None)
}

/// The `SyntaxError` (or `IndentationError`, `TabError`) CPython's
/// `compile` raises: `(msg, (filename, lineno, offset, text, end_lineno,
/// end_offset))`, with 1-based character offsets.
#[cfg_attr(mm_shipped, optimize(size))]
fn syntax_error(vm: &mut Vm, e: &CompileError, src: &str, file: Value, exec: bool) -> Raised {
    let t = match e.kind {
        "IndentationError" => T::IndentationError,
        "TabError" => T::TabError,
        "SyntaxError" => T::SyntaxError,
        _ => return vm.err(T::RecursionError, &e.msg),
    };
    let r = (|| {
        let mut p = e.pos;
        let msg = vm.new_str(&e.msg)?;
        let line = |n: u32| src.split_inclusive('\n').nth((n as usize).wrapping_sub(1));
        // A token that ends with its line's newline ends there, not at the
        // start of the next line.
        if p.end_line > p.line && p.end_col == 0 {
            p.end_line -= 1;
            p.end_col = line(p.end_line).map_or(0, |l| u32::try_from(l.len()).unwrap_or(0));
        }
        // The line keeps its newline; exec input gets one at the end, as
        // CPython's file-mode tokenizer adds it, except in errors raised
        // inside a token (a string, a number, an unmatched closer).
        let m = e.msg.as_str();
        let in_token = ["unterminated", "unmatched", "f-string", "leading zeros"]
            .iter()
            .any(|x| m.starts_with(x))
            || (m.starts_with("invalid") && m.ends_with("literal"));
        let text = match line(p.line) {
            Some(l) if p.line > 0 && exec && !l.ends_with('\n') && !in_token => {
                vm.new_str(&format!("{l}\n"))?
            }
            Some(l) if p.line > 0 => vm.new_str(l)?,
            _ => Value::NONE,
        };
        let int = |vm: &mut Vm, v: u32, base: u32| {
            if p.line == 0 {
                Ok(Value::NONE)
            } else {
                vm.new_int(i64::from(v) + i64::from(base))
            }
        };
        // CPython's tokenizer reports these without an end.
        let end_col = match e.msg.as_str() {
            "unexpected indent" | "expected 'except' or 'finally' block" => Value::int(-1),
            m if m.ends_with("was never closed") || e.kind == "TabError" => Value::int(0),
            _ => None,
        };
        // An eval input's error at its end, with no newline, is at offset 0.
        let at_end = !exec
            && !src.ends_with('\n')
            && line(p.line).is_some_and(|l| p.col as usize >= l.len())
            && line(p.line + 1).is_none();
        let end_col = if at_end { Value::int(0) } else { end_col };
        let info = [
            file,
            int(vm, p.line, 0)?,
            int(vm, if at_end { 0 } else { p.col + 1 }, 0)?,
            text,
            int(
                vm,
                if end_col.is_some() {
                    p.line
                } else {
                    p.end_line
                },
                0,
            )?,
            match end_col {
                Some(v) => v,
                None => int(vm, p.end_col, 1)?,
            },
        ];
        let info = vm.alloc_tuple(&info)?;
        vm.new_exception(t, &[msg, info])
    })();
    match r {
        Ok(x) => {
            vm.exc = x;
            Raised
        }
        Err(r) => r,
    }
}

impl Build<'_> {
    /// A node of class `class` (from `ast.py`) with `fields`, and the
    /// position attributes when `pos` is given.
    #[cfg_attr(mm_shipped, optimize(size))]
    fn node(&mut self, class: &str, fields: &[(&str, Value)], pos: Option<Pos>) -> R {
        self.vm.burn(1);
        let k = self.vm.intern(class)?;
        let Some(c) = self.vm.dict_get_str(self.ns, k) else {
            let msg = format!("module 'ast' has no attribute '{class}'");
            return Err(self.vm.err(T::AttributeError, &msg));
        };
        let o = self.vm.alloc_of_class(c, &[])?;
        for &(f, v) in fields {
            let k = self.vm.intern(f)?;
            self.vm.inst_set(o, k, v)?;
        }
        if let Some(p) = pos {
            self.set_pos(o, p)?;
        }
        Ok(o)
    }

    /// Sets node `o`'s position attributes to `p`.
    #[cfg_attr(mm_shipped, optimize(size))]
    fn set_pos(&mut self, o: Value, p: Pos) -> Result<(), Raised> {
        let (c, ec) = (self.col(p.line, p.col), self.col(p.end_line, p.end_col));
        for (f, v) in [
            ("lineno", p.line as usize),
            ("col_offset", c),
            ("end_lineno", p.end_line as usize),
            ("end_col_offset", ec),
        ] {
            let k = self.vm.intern(f)?;
            let v = self.vm.new_int(i64::try_from(v).unwrap_or(0))?;
            self.vm.inst_set(o, k, v)?;
        }
        Ok(())
    }

    /// The UTF-8 byte column of (line, character column).
    #[cfg_attr(mm_shipped, optimize(size))]
    fn col(&self, line: u32, col: u32) -> usize {
        self.lines.offset(line, col) - self.lines.start(line)
    }

    fn str(&mut self, s: &str) -> R {
        self.vm.new_str(s)
    }

    #[cfg_attr(mm_shipped, optimize(size))]
    fn opt_str(&mut self, s: Option<&String>) -> R {
        s.map_or(Ok(Value::NONE), |s| self.vm.new_str(s))
    }

    #[cfg_attr(mm_shipped, optimize(size))]
    fn list(&mut self, items: &[Value]) -> R {
        self.vm.new_list(items)
    }

    #[cfg_attr(mm_shipped, optimize(size))]
    fn strs(&mut self, names: &[String]) -> R {
        let v = names
            .iter()
            .map(|n| self.vm.new_str(n))
            .collect::<Result<Vec<_>, _>>()?;
        self.list(&v)
    }

    #[cfg_attr(mm_shipped, optimize(size))]
    fn stmts(&mut self, body: &[Stmt]) -> R {
        let v = body
            .iter()
            .map(|s| self.stmt(s))
            .collect::<Result<Vec<_>, _>>()?;
        self.list(&v)
    }

    /// An `if` and its `elif` arms, built from the last arm back: an
    /// `elif` chain nests as deeply as it is long.
    #[cfg_attr(mm_shipped, optimize(size))]
    fn if_chain(&mut self, s: &Stmt) -> R {
        let mut arms = Vec::new();
        let mut cur = s;
        while let StmtKind::If(_, _, o) = &cur.kind {
            arms.push(cur);
            match o.as_slice() {
                [
                    n @ Stmt {
                        kind: StmtKind::If(..),
                        ..
                    },
                ] => cur = n,
                _ => break,
            }
        }
        let mut node = Value::NONE;
        while let Some(arm) = arms.pop() {
            let StmtKind::If(t, b, o) = &arm.kind else {
                continue;
            };
            let t = self.expr(t, Ctx::Load)?;
            let b = self.stmts(b)?;
            let o = if node == Value::NONE {
                self.stmts(o)?
            } else {
                self.list(&[node])?
            };
            node = self.node(
                "If",
                &[("test", t), ("body", b), ("orelse", o)],
                Some(arm.pos),
            )?;
        }
        Ok(node)
    }

    #[cfg_attr(mm_shipped, optimize(size))]
    fn exprs(&mut self, es: &[Expr], ctx: Ctx) -> R {
        let v = es
            .iter()
            .map(|e| self.expr(e, ctx))
            .collect::<Result<Vec<_>, _>>()?;
        self.list(&v)
    }

    #[cfg_attr(mm_shipped, optimize(size))]
    fn opt(&mut self, e: Option<&Expr>, ctx: Ctx) -> R {
        e.map_or(Ok(Value::NONE), |e| self.expr(e, ctx))
    }

    #[cfg_attr(mm_shipped, optimize(size))]
    fn ctx(&mut self, c: Ctx) -> R {
        self.op(["Load", "Store", "Del"][c as usize])
    }

    /// The shared instance of context or operator class `name`.
    #[cfg_attr(mm_shipped, optimize(size))]
    fn op(&mut self, name: &str) -> R {
        let k = self.vm.intern(name)?;
        match self.vm.dict_get_str(self.one, k) {
            Some(v) => Ok(v),
            None => self.node(name, &[], None),
        }
    }

    #[allow(clippy::too_many_lines, reason = "one arm per statement kind")]
    #[cfg_attr(mm_shipped, optimize(size))]
    fn stmt(&mut self, s: &Stmt) -> R {
        let p = Some(s.pos);
        let no = Value::NONE;
        match &s.kind {
            StmtKind::Expr(e) => {
                let v = self.expr(e, Ctx::Load)?;
                self.node("Expr", &[("value", v)], p)
            }
            StmtKind::Assign(ts, v) => {
                let ts = self.exprs(ts, Ctx::Store)?;
                let v = self.expr(v, Ctx::Load)?;
                self.node(
                    "Assign",
                    &[("targets", ts), ("value", v), ("type_comment", no)],
                    p,
                )
            }
            StmtKind::AugAssign(t, op, v) => {
                let t = self.expr(t, Ctx::Store)?;
                let op = self.op(binop(*op))?;
                let v = self.expr(v, Ctx::Load)?;
                self.node("AugAssign", &[("target", t), ("op", op), ("value", v)], p)
            }
            StmtKind::AnnAssign(t, v, ann) => {
                let simple =
                    Value::int(i64::from(matches!(t.kind, ExprKind::Name(_)))).unwrap_or(no);
                let t = self.expr(t, Ctx::Store)?;
                let a = self.opt(ann.as_deref(), Ctx::Load)?;
                let v = self.opt(v.as_ref(), Ctx::Load)?;
                self.node(
                    "AnnAssign",
                    &[
                        ("target", t),
                        ("annotation", a),
                        ("value", v),
                        ("simple", simple),
                    ],
                    p,
                )
            }
            StmtKind::If(..) => self.if_chain(s),
            StmtKind::While(t, b, o) => {
                let t = self.expr(t, Ctx::Load)?;
                let b = self.stmts(b)?;
                let o = self.stmts(o)?;
                self.node("While", &[("test", t), ("body", b), ("orelse", o)], p)
            }
            StmtKind::For(t, it, b, o, asy) => {
                let t = self.expr(t, Ctx::Store)?;
                let it = self.expr(it, Ctx::Load)?;
                let b = self.stmts(b)?;
                let o = self.stmts(o)?;
                self.node(
                    if *asy { "AsyncFor" } else { "For" },
                    &[
                        ("target", t),
                        ("iter", it),
                        ("body", b),
                        ("orelse", o),
                        ("type_comment", no),
                    ],
                    p,
                )
            }
            StmtKind::Break => self.node("Break", &[], p),
            StmtKind::Continue => self.node("Continue", &[], p),
            StmtKind::Pass => self.node("Pass", &[], p),
            StmtKind::Return(v) => {
                let v = self.opt(v.as_ref(), Ctx::Load)?;
                self.node("Return", &[("value", v)], p)
            }
            StmtKind::FunctionDef(f) => self.funcdef(f),
            StmtKind::Delete(ts) => {
                let ts = self.exprs(ts, Ctx::Del)?;
                self.node("Delete", &[("targets", ts)], p)
            }
            StmtKind::Global(ns) | StmtKind::Nonlocal(ns) => {
                let name = if matches!(s.kind, StmtKind::Global(_)) {
                    "Global"
                } else {
                    "Nonlocal"
                };
                let ns = self.strs(ns)?;
                self.node(name, &[("names", ns)], p)
            }
            StmtKind::Raise(e, c) => {
                let e = self.opt(e.as_ref(), Ctx::Load)?;
                let c = self.opt(c.as_ref(), Ctx::Load)?;
                self.node("Raise", &[("exc", e), ("cause", c)], p)
            }
            StmtKind::Try {
                body,
                handlers,
                orelse,
                finalbody,
                star,
            } => {
                let b = self.stmts(body)?;
                let mut hs = Vec::with_capacity(handlers.len());
                for h in handlers {
                    let t = self.opt(h.typ.as_ref(), Ctx::Load)?;
                    let n = self.opt_str(h.name.as_ref())?;
                    let hb = self.stmts(&h.body)?;
                    let mut hp = h.pos;
                    if let Some(last) = h.body.last() {
                        hp.end_line = last.pos.end_line;
                        hp.end_col = last.pos.end_col;
                    }
                    hs.push(self.node(
                        "ExceptHandler",
                        &[("type", t), ("name", n), ("body", hb)],
                        Some(hp),
                    )?);
                }
                let hs = self.list(&hs)?;
                let o = self.stmts(orelse)?;
                let f = self.stmts(finalbody)?;
                self.node(
                    if *star { "TryStar" } else { "Try" },
                    &[
                        ("body", b),
                        ("handlers", hs),
                        ("orelse", o),
                        ("finalbody", f),
                    ],
                    p,
                )
            }
            StmtKind::Assert(t, m) => {
                let t = self.expr(t, Ctx::Load)?;
                let m = self.opt(m.as_ref(), Ctx::Load)?;
                self.node("Assert", &[("test", t), ("msg", m)], p)
            }
            StmtKind::Import(names) => {
                let ns = self.aliases(names)?;
                self.node("Import", &[("names", ns)], p)
            }
            StmtKind::ImportFrom(module, names) => {
                let rest = module.trim_start_matches('.');
                let level = (module.len() - rest.len()) as i64;
                let m = if rest.is_empty() { no } else { self.str(rest)? };
                let ns = self.aliases(names)?;
                let level = self.vm.new_int(level)?;
                self.node(
                    "ImportFrom",
                    &[("module", m), ("names", ns), ("level", level)],
                    p,
                )
            }
            StmtKind::ClassDef(c) => self.classdef(c),
            StmtKind::With(items, body, asy) => {
                let mut v = Vec::with_capacity(items.len());
                for it in items {
                    let c = self.expr(&it.ctx, Ctx::Load)?;
                    let t = self.opt(it.target.as_ref(), Ctx::Store)?;
                    v.push(self.node(
                        "withitem",
                        &[("context_expr", c), ("optional_vars", t)],
                        None,
                    )?);
                }
                let items = self.list(&v)?;
                let b = self.stmts(body)?;
                self.node(
                    if *asy { "AsyncWith" } else { "With" },
                    &[("items", items), ("body", b), ("type_comment", no)],
                    p,
                )
            }
            StmtKind::Match(subject, cases) => {
                let sub = self.expr(subject, Ctx::Load)?;
                let mut v = Vec::with_capacity(cases.len());
                for c in cases {
                    let pat = self.pattern(&c.pattern)?;
                    let g = self.opt(c.guard.as_ref(), Ctx::Load)?;
                    let b = self.stmts(&c.body)?;
                    v.push(self.node(
                        "match_case",
                        &[("pattern", pat), ("guard", g), ("body", b)],
                        None,
                    )?);
                }
                let cases = self.list(&v)?;
                self.node("Match", &[("subject", sub), ("cases", cases)], p)
            }
        }
    }

    #[cfg_attr(mm_shipped, optimize(size))]
    fn aliases(&mut self, names: &[mm_compile::ast::Alias]) -> R {
        let mut v = Vec::with_capacity(names.len());
        for a in names {
            let n = self.str(&a.name)?;
            let asn = self.opt_str(a.asname.as_ref())?;
            v.push(self.node("alias", &[("name", n), ("asname", asn)], Some(a.pos))?);
        }
        self.list(&v)
    }

    #[cfg_attr(mm_shipped, optimize(size))]
    fn funcdef(&mut self, f: &FuncDef) -> R {
        let name = self.str(&f.name)?;
        let args = self.arguments(&f.params)?;
        let body = self.stmts(&f.body)?;
        let decos = self.exprs(&f.decorators, Ctx::Load)?;
        let ret = self.opt(f.returns.as_deref(), Ctx::Load)?;
        let tp = self.list(&[])?;
        self.node(
            if f.is_async {
                "AsyncFunctionDef"
            } else {
                "FunctionDef"
            },
            &[
                ("name", name),
                ("args", args),
                ("body", body),
                ("decorator_list", decos),
                ("returns", ret),
                ("type_comment", Value::NONE),
                ("type_params", tp),
            ],
            Some(f.pos),
        )
    }

    #[cfg_attr(mm_shipped, optimize(size))]
    fn classdef(&mut self, c: &ClassDef) -> R {
        let name = self.str(&c.name)?;
        let bases = self.exprs(&c.bases, Ctx::Load)?;
        let kws = self.keywords(&c.keywords)?;
        let body = self.stmts(&c.body)?;
        let decos = self.exprs(&c.decorators, Ctx::Load)?;
        let tp = self.list(&[])?;
        self.node(
            "ClassDef",
            &[
                ("name", name),
                ("bases", bases),
                ("keywords", kws),
                ("body", body),
                ("decorator_list", decos),
                ("type_params", tp),
            ],
            Some(c.span),
        )
    }

    fn arg(&mut self, p: &Param) -> R {
        let n = self.str(&p.name)?;
        let a = self.opt(p.ann.as_deref(), Ctx::Load)?;
        self.node(
            "arg",
            &[("arg", n), ("annotation", a), ("type_comment", Value::NONE)],
            Some(p.pos),
        )
    }

    #[cfg_attr(mm_shipped, optimize(size))]
    fn args(&mut self, ps: &[Param]) -> R {
        let v = ps
            .iter()
            .map(|p| self.arg(p))
            .collect::<Result<Vec<_>, _>>()?;
        self.list(&v)
    }

    fn arguments(&mut self, p: &Params) -> R {
        let posonly = self.args(&p.posonly)?;
        let args = self.args(&p.args)?;
        let star = |i: usize| p.star.as_ref().and_then(|s| s[i].as_ref());
        let vararg = star(0).map_or(Ok(Value::NONE), |a| self.arg(a))?;
        let kwonly = self.args(&p.kwonly)?;
        let mut kwd = Vec::with_capacity(p.kwonly.len());
        for a in &p.kwonly {
            kwd.push(self.opt(a.default.as_ref(), Ctx::Load)?);
        }
        let kwd = self.list(&kwd)?;
        let kwarg = star(1).map_or(Ok(Value::NONE), |a| self.arg(a))?;
        let mut defaults = Vec::new();
        for a in p.posonly.iter().chain(&p.args) {
            if let Some(d) = &a.default {
                defaults.push(self.expr(d, Ctx::Load)?);
            }
        }
        let defaults = self.list(&defaults)?;
        self.node(
            "arguments",
            &[
                ("posonlyargs", posonly),
                ("args", args),
                ("vararg", vararg),
                ("kwonlyargs", kwonly),
                ("kw_defaults", kwd),
                ("kwarg", kwarg),
                ("defaults", defaults),
            ],
            None,
        )
    }

    #[cfg_attr(mm_shipped, optimize(size))]
    fn keywords(&mut self, kws: &[Keyword]) -> R {
        let mut v = Vec::with_capacity(kws.len());
        for k in kws {
            let n = self.opt_str(k.name.as_ref())?;
            let val = self.expr(&k.value, Ctx::Load)?;
            v.push(self.node("keyword", &[("arg", n), ("value", val)], Some(k.pos))?);
        }
        self.list(&v)
    }

    #[cfg_attr(mm_shipped, optimize(size))]
    fn comps(&mut self, gens: &[Comprehension]) -> R {
        let mut v = Vec::with_capacity(gens.len());
        for g in gens {
            let t = self.expr(&g.target, Ctx::Store)?;
            let it = self.expr(&g.iter, Ctx::Load)?;
            let ifs = self.exprs(&g.ifs, Ctx::Load)?;
            let asy = Value::int(i64::from(g.is_async)).unwrap_or(Value::NONE);
            v.push(self.node(
                "comprehension",
                &[("target", t), ("iter", it), ("ifs", ifs), ("is_async", asy)],
                None,
            )?);
        }
        self.list(&v)
    }

    #[cfg_attr(mm_shipped, optimize(size))]
    fn constant(&mut self, c: &Const) -> R {
        Ok(match c {
            Const::None => Value::NONE,
            Const::Bool(b) => Value::from_bool(*b),
            Const::Ellipsis => Value::ELLIPSIS,
            Const::Int(i) => self.vm.new_int(*i)?,
            Const::Float(f) => Value::float(*f),
            Const::Str(s) => self.vm.new_str(s)?,
            Const::BigInt(s) => {
                match crate::intops::parse_int_text(s, 0, crate::intops::MAX_STR_DIGITS) {
                    Ok(b) => self.vm.new_big(&b)?,
                    Err(_) => return Err(self.vm.err(T::SyntaxError, "invalid integer literal")),
                }
            }
            Const::Bytes(b) => self.vm.new_bytes(b)?,
            Const::Complex(im) => self.vm.new_complex(0.0, *im)?,
        })
    }

    /// The `Pos` of byte range `s..e`.
    #[cfg_attr(mm_shipped, optimize(size))]
    fn span(&self, s: usize, e: usize) -> Pos {
        let at = |o: usize| self.lines.loc(u32::try_from(o).unwrap_or(u32::MAX));
        let ((line, col), (end_line, end_col)) = (at(s), at(e));
        Pos {
            line,
            col,
            end_line,
            end_col,
        }
    }

    /// The byte offset of character position `(line, col)`.
    #[cfg_attr(mm_shipped, optimize(size))]
    fn off(&self, line: u32, col: u32) -> usize {
        self.lines.offset(line, col)
    }

    /// The byte range of the literal before `next`: `base`, or, when `next`
    /// is a debug field (`{x=}`), extended over its text up to the `=` and
    /// the spaces after it (starting after the `{` when `base` is empty).
    #[cfg_attr(mm_shipped, optimize(size))]
    fn with_debug(
        &self,
        base: Option<(usize, usize)>,
        next: Option<&FPart>,
    ) -> Option<(usize, usize)> {
        let Some(FPart::Field { expr, pos: fp, .. }) = next else {
            return base;
        };
        let open = self.off(fp.line, fp.col);
        let fend = self.off(fp.end_line, fp.end_col);
        let xend = self.off(expr.pos.end_line, expr.pos.end_col);
        let tail = self.src.get(xend..fend).unwrap_or("");
        let t = tail.trim_start();
        if !t.starts_with('=') {
            return base;
        }
        let eq = xend + tail.len() - t.len() + 1;
        let rest = self.src.get(eq..fend).unwrap_or("");
        let end = eq + rest.len() - rest.trim_start().len();
        Some((base.map_or(open + 1, |b| b.0), end))
    }

    /// A `Constant` literal part over bytes `a..b`.
    #[cfg_attr(mm_shipped, optimize(size))]
    fn lit(&mut self, text: &str, a: usize, b: usize) -> R {
        let p = self.span(a, b.max(a));
        let s = self.str(text)?;
        self.node("Constant", &[("value", s), ("kind", Value::NONE)], Some(p))
    }

    /// An f-string at `pos` as a `JoinedStr`. A literal part spans the
    /// string pieces and f-string text it was joined from (the own lexer's
    /// tokens outside this f-string's fields), as CPython's does.
    #[cfg_attr(mm_shipped, optimize(size))]
    fn fstring(&mut self, parts: &[FPart], pos: Pos) -> R {
        let (s, e) = (
            self.off(pos.line, pos.col),
            self.off(pos.end_line, pos.end_col),
        );
        let fields: Vec<(usize, usize)> = parts
            .iter()
            .filter_map(|p| match p {
                FPart::Field { pos: fp, .. } => {
                    Some((self.off(fp.line, fp.col), self.off(fp.end_line, fp.end_col)))
                }
                FPart::Lit(_) => None,
            })
            .collect();
        // groups[j]: the literal tokens before field j (after the last).
        let mut groups: Vec<Option<(usize, usize)>> = alloc::vec![None; fields.len() + 1];
        let src = self.src;
        let segs = self
            .segs
            .get_or_insert_with(|| mm_parse::string_segments(src));
        let first = segs.partition_point(|&(a, _)| (a as usize) < s);
        for &(a, b) in &segs[first..] {
            let (a, b) = (a as usize, b as usize);
            if a >= e {
                break;
            }
            let j = fields.partition_point(|&(fs, _)| fs < a);
            if b > e || j > 0 && b <= fields[j - 1].1 {
                continue;
            }
            groups[j] = Some(groups[j].map_or((a, b), |(x, _)| (x, b)));
        }
        let mut v = Vec::with_capacity(parts.len());
        let mut j = 0;
        for (i, part) in parts.iter().enumerate() {
            v.push(match part {
                FPart::Lit(text) => {
                    let base = groups.get(j).copied().flatten();
                    let (a, b) = self.with_debug(base, parts.get(i + 1)).unwrap_or((s, s));
                    self.lit(text, a, b)?
                }
                FPart::Field {
                    expr,
                    conv,
                    spec,
                    pos: fp,
                } => {
                    j += 1;
                    self.field(expr, *conv, spec.as_deref(), *fp)?
                }
            });
        }
        let v = self.list(&v)?;
        self.node("JoinedStr", &[("values", v)], Some(pos))
    }

    /// A replacement field as a `FormattedValue`; its format spec is a
    /// `JoinedStr` from the `:` to the closing `}`.
    fn field(&mut self, expr: &Expr, conv: u8, spec: Option<&[FPart]>, fp: Pos) -> R {
        let x = self.expr(expr, Ctx::Load)?;
        let c = Value::int(if conv == 0 { -1 } else { i64::from(conv) }).unwrap_or(Value::NONE);
        let sp = match spec {
            Some(sp) => {
                let end = self.off(fp.end_line, fp.end_col);
                let from = self.off(expr.pos.end_line, expr.pos.end_col);
                let colon = self
                    .src
                    .get(from..end)
                    .and_then(|t| t.find(':'))
                    .map_or(end.saturating_sub(1), |k| from + k);
                let close = end.saturating_sub(1).max(colon);
                let mut at = colon + 1;
                let mut v = Vec::with_capacity(sp.len());
                for (i, part) in sp.iter().enumerate() {
                    v.push(match part {
                        FPart::Lit(text) => {
                            let next = match sp.get(i + 1) {
                                Some(FPart::Field { pos: p, .. }) => self.off(p.line, p.col),
                                _ => close,
                            };
                            let base = (at < next).then_some((at, next));
                            let (a, b) = self.with_debug(base, sp.get(i + 1)).unwrap_or((at, at));
                            self.lit(text, a, b)?
                        }
                        FPart::Field {
                            expr,
                            conv,
                            spec,
                            pos,
                        } => {
                            at = self.off(pos.end_line, pos.end_col);
                            self.field(expr, *conv, spec.as_deref(), *pos)?
                        }
                    });
                }
                let vals = self.list(&v)?;
                let p = self.span(colon, close);
                self.node("JoinedStr", &[("values", vals)], Some(p))?
            }
            None => Value::NONE,
        };
        self.node(
            "FormattedValue",
            &[("value", x), ("conversion", c), ("format_spec", sp)],
            Some(fp),
        )
    }

    /// `e`'s node. A chain (`a + b + ...`, `x.f().g[0]`, `a if c else b if
    /// ...`) nests as deeply as it is long, so it is built in a loop from
    /// its innermost operand out.
    #[cfg_attr(mm_shipped, optimize(size))]
    fn expr<'e>(&mut self, e: &'e Expr, ctx: Ctx) -> R {
        let link = |e: &'e Expr| match &e.kind {
            ExprKind::IfExp(_, _, o) => Some(&**o),
            _ => e.chain_left(),
        };
        let mut spine = Vec::new();
        let mut base = e;
        while let Some(l) = link(base) {
            spine.push(base);
            base = l;
        }
        if spine.is_empty() {
            return self.one(e, ctx, None);
        }
        let mut v = self.one(base, Ctx::Load, None)?;
        while let Some(s) = spine.pop() {
            let c = if spine.is_empty() { ctx } else { Ctx::Load };
            v = self.one(s, c, Some(v))?;
        }
        Ok(v)
    }

    /// One node, with its chain operand (`expr`'s `link`) already built
    /// when `chained` is given.
    #[allow(clippy::too_many_lines, reason = "one arm per expression kind")]
    #[cfg_attr(mm_shipped, optimize(size))]
    fn one(&mut self, e: &Expr, ctx: Ctx, chained: Option<Value>) -> R {
        let p = Some(e.pos);
        let load = Ctx::Load;
        match &e.kind {
            ExprKind::Const(c) => {
                let v = self.constant(c)?;
                self.node("Constant", &[("value", v), ("kind", Value::NONE)], p)
            }
            ExprKind::Name(n) => {
                let n = self.str(n)?;
                let c = self.ctx(ctx)?;
                self.node("Name", &[("id", n), ("ctx", c)], p)
            }
            ExprKind::BinOp(l, op, r) => {
                let l = self.chained(chained, l)?;
                let op = self.op(binop(*op))?;
                let r = self.expr(r, load)?;
                self.node("BinOp", &[("left", l), ("op", op), ("right", r)], p)
            }
            ExprKind::Unary(op, x) => {
                let op = self.op(match op {
                    UnaryOp::Neg => "USub",
                    UnaryOp::Pos => "UAdd",
                    UnaryOp::Not => "Not",
                    UnaryOp::Invert => "Invert",
                })?;
                let x = self.expr(x, load)?;
                self.node("UnaryOp", &[("op", op), ("operand", x)], p)
            }
            ExprKind::BoolOp(op, vs) => {
                let op = self.op(match op {
                    BoolOp::And => "And",
                    BoolOp::Or => "Or",
                })?;
                let vs = self.exprs(vs, load)?;
                self.node("BoolOp", &[("op", op), ("values", vs)], p)
            }
            ExprKind::Compare(l, rest) => {
                let l = self.expr(l, load)?;
                let mut ops = Vec::with_capacity(rest.len());
                let mut cs = Vec::with_capacity(rest.len());
                for (op, c) in rest {
                    ops.push(self.op(cmpop(*op))?);
                    cs.push(self.expr(c, load)?);
                }
                let ops = self.list(&ops)?;
                let cs = self.list(&cs)?;
                self.node(
                    "Compare",
                    &[("left", l), ("ops", ops), ("comparators", cs)],
                    p,
                )
            }
            ExprKind::IfExp(t, b, o) => {
                let t = self.expr(t, load)?;
                let b = self.expr(b, load)?;
                let o = self.chained(chained, o)?;
                self.node("IfExp", &[("test", t), ("body", b), ("orelse", o)], p)
            }
            ExprKind::Lambda(f) => {
                let args = self.arguments(&f.params)?;
                let body = match f.body.first().map(|s| &s.kind) {
                    Some(StmtKind::Return(Some(b))) => self.expr(b, load)?,
                    _ => Value::NONE,
                };
                self.node("Lambda", &[("args", args), ("body", body)], p)
            }
            ExprKind::Call(f, args, kws) => {
                let f = self.chained(chained, f)?;
                let args = match &args[..] {
                    // A bare generator expression, the only argument, spans
                    // the call's parentheses.
                    [
                        g @ Expr {
                            kind: ExprKind::GenExp(..),
                            ..
                        },
                    ] if kws.is_empty() && bare_genexp(g) => {
                        let v = self.expr(g, load)?;
                        let span = Pos {
                            line: e.anchor.left_line,
                            col: e.anchor.left_col,
                            ..e.pos
                        };
                        self.set_pos(v, span)?;
                        self.list(&[v])?
                    }
                    _ => self.exprs(args, load)?,
                };
                let kws = self.keywords(kws)?;
                self.node("Call", &[("func", f), ("args", args), ("keywords", kws)], p)
            }
            ExprKind::Attribute(v, attr) => {
                let v = self.chained(chained, v)?;
                let a = self.str(attr)?;
                let c = self.ctx(ctx)?;
                self.node("Attribute", &[("value", v), ("attr", a), ("ctx", c)], p)
            }
            ExprKind::Subscript(v, s) => {
                let v = self.chained(chained, v)?;
                let s = self.expr(s, load)?;
                let c = self.ctx(ctx)?;
                self.node("Subscript", &[("value", v), ("slice", s), ("ctx", c)], p)
            }
            ExprKind::Slice(a, b, c) => {
                let a = self.opt(a.as_deref(), load)?;
                let b = self.opt(b.as_deref(), load)?;
                let c = self.opt(c.as_deref(), load)?;
                self.node("Slice", &[("lower", a), ("upper", b), ("step", c)], p)
            }
            ExprKind::Starred(v) => {
                let v = self.expr(v, ctx)?;
                let c = self.ctx(ctx)?;
                self.node("Starred", &[("value", v), ("ctx", c)], p)
            }
            ExprKind::List(es) | ExprKind::Tuple(es) => {
                let name = if matches!(e.kind, ExprKind::List(_)) {
                    "List"
                } else {
                    "Tuple"
                };
                let es = self.exprs(es, ctx)?;
                let c = self.ctx(ctx)?;
                self.node(name, &[("elts", es), ("ctx", c)], p)
            }
            ExprKind::Set(es) => {
                let es = self.exprs(es, load)?;
                self.node("Set", &[("elts", es)], p)
            }
            ExprKind::Dict(items) => {
                let mut ks = Vec::with_capacity(items.len());
                let mut vs = Vec::with_capacity(items.len());
                for (k, v) in items {
                    ks.push(self.opt(k.as_ref(), load)?);
                    vs.push(self.expr(v, load)?);
                }
                let ks = self.list(&ks)?;
                let vs = self.list(&vs)?;
                self.node("Dict", &[("keys", ks), ("values", vs)], p)
            }
            ExprKind::ListComp(elt, gens)
            | ExprKind::SetComp(elt, gens)
            | ExprKind::GenExp(elt, gens) => {
                let name = match e.kind {
                    ExprKind::ListComp(..) => "ListComp",
                    ExprKind::SetComp(..) => "SetComp",
                    _ => "GeneratorExp",
                };
                let elt = self.expr(elt, load)?;
                let g = self.comps(gens)?;
                self.node(name, &[("elt", elt), ("generators", g)], p)
            }
            ExprKind::DictComp(k, v, gens) => {
                let k = self.expr(k, load)?;
                let v = self.expr(v, load)?;
                let g = self.comps(gens)?;
                self.node(
                    "DictComp",
                    &[("key", k), ("value", v), ("generators", g)],
                    p,
                )
            }
            ExprKind::FString(parts) => self.fstring(parts, e.pos),
            ExprKind::Named(n, v) => {
                let mut tp = e.pos;
                tp.end_line = tp.line;
                tp.end_col = tp.col + u32::try_from(n.chars().count()).unwrap_or(0);
                let n = self.str(n)?;
                let c = self.ctx(Ctx::Store)?;
                let t = self.node("Name", &[("id", n), ("ctx", c)], Some(tp))?;
                let v = self.expr(v, load)?;
                self.node("NamedExpr", &[("target", t), ("value", v)], p)
            }
            ExprKind::Await(v) => {
                let v = self.expr(v, load)?;
                self.node("Await", &[("value", v)], p)
            }
            ExprKind::Yield(v) => {
                let v = self.opt(v.as_deref(), load)?;
                self.node("Yield", &[("value", v)], p)
            }
            ExprKind::YieldFrom(v) => {
                let v = self.expr(v, load)?;
                self.node("YieldFrom", &[("value", v)], p)
            }
        }
    }

    /// A chain operand: the node `expr` built already, else `e`'s.
    #[cfg_attr(mm_shipped, optimize(size))]
    fn chained(&mut self, chained: Option<Value>, e: &Expr) -> R {
        chained.map_or_else(|| self.expr(e, Ctx::Load), Ok)
    }

    #[cfg_attr(mm_shipped, optimize(size))]
    fn patterns(&mut self, ps: &[Pattern]) -> R {
        let v = ps
            .iter()
            .map(|p| self.pattern(p))
            .collect::<Result<Vec<_>, _>>()?;
        self.list(&v)
    }

    #[cfg_attr(mm_shipped, optimize(size))]
    fn pattern(&mut self, pat: &Pattern) -> R {
        let p = Some(pat.pos);
        match &pat.kind {
            PatternKind::Value(e) => {
                let v = self.expr(e, Ctx::Load)?;
                self.node("MatchValue", &[("value", v)], p)
            }
            PatternKind::Singleton(c) => {
                let v = self.constant(c)?;
                self.node("MatchSingleton", &[("value", v)], p)
            }
            PatternKind::Sequence(ps) => {
                let ps = self.patterns(ps)?;
                self.node("MatchSequence", &[("patterns", ps)], p)
            }
            PatternKind::Star(n) => {
                let n = self.opt_str(n.as_ref())?;
                self.node("MatchStar", &[("name", n)], p)
            }
            PatternKind::Mapping {
                keys,
                patterns,
                rest,
            } => {
                let ks = self.exprs(keys, Ctx::Load)?;
                let ps = self.patterns(patterns)?;
                let r = self.opt_str(rest.as_ref())?;
                self.node(
                    "MatchMapping",
                    &[("keys", ks), ("patterns", ps), ("rest", r)],
                    p,
                )
            }
            PatternKind::Class {
                cls,
                patterns,
                kwd_attrs,
                kwd_patterns,
            } => {
                let c = self.expr(cls, Ctx::Load)?;
                let ps = self.patterns(patterns)?;
                let ka = self.strs(kwd_attrs)?;
                let kp = self.patterns(kwd_patterns)?;
                self.node(
                    "MatchClass",
                    &[
                        ("cls", c),
                        ("patterns", ps),
                        ("kwd_attrs", ka),
                        ("kwd_patterns", kp),
                    ],
                    p,
                )
            }
            PatternKind::As(inner, n) => {
                let i = match inner {
                    Some(i) => self.pattern(i)?,
                    None => Value::NONE,
                };
                let n = self.opt_str(n.as_ref())?;
                self.node("MatchAs", &[("pattern", i), ("name", n)], p)
            }
            PatternKind::Or(ps) => {
                let ps = self.patterns(ps)?;
                self.node("MatchOr", &[("patterns", ps)], p)
            }
        }
    }
}

/// A generator expression without its own parentheses: it starts where its
/// element does or ends where its last clause does (its own parentheses
/// would surround both; `f((x) for x in (y))` is taken as parenthesized).
#[cfg_attr(mm_shipped, optimize(size))]
fn bare_genexp(g: &Expr) -> bool {
    let ExprKind::GenExp(elt, gens) = &g.kind else {
        return false;
    };
    let last = gens.last().map(|c| c.ifs.last().unwrap_or(&c.iter).pos);
    (elt.pos.line, elt.pos.col) == (g.pos.line, g.pos.col)
        || last.is_some_and(|p| (p.end_line, p.end_col) == (g.pos.end_line, g.pos.end_col))
}

#[cfg_attr(mm_shipped, optimize(size))]
fn binop(op: BinOp) -> &'static str {
    [
        "Add", "Sub", "Mult", "Div", "FloorDiv", "Mod", "Pow", "LShift", "RShift", "BitAnd",
        "BitOr", "BitXor", "MatMult",
    ][op as usize]
}

#[cfg_attr(mm_shipped, optimize(size))]
fn cmpop(op: CmpOp) -> &'static str {
    match op {
        CmpOp::Eq => "Eq",
        CmpOp::Ne => "NotEq",
        CmpOp::Lt => "Lt",
        CmpOp::Le => "LtE",
        CmpOp::Gt => "Gt",
        CmpOp::Ge => "GtE",
        CmpOp::In => "In",
        CmpOp::NotIn => "NotIn",
        CmpOp::Is => "Is",
        CmpOp::IsNot => "IsNot",
    }
}
