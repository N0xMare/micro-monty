//! Reports the error the old ruff lowering reported first, in its order.
//!
//! The lowering ran after a complete parse and walked the tree in a fixed
//! order, entering every statement and expression (counting depth against
//! [`MAX_NESTING`]) and rejecting unsupported constructs as it reached them.
//! The parser leaves marker nodes for those constructs (see
//! [`crate::parser::MARK`]); this walk visits the finished tree in the
//! lowering's order and returns the first marker's error or the first node
//! past the depth limit. It recurses at most `MAX_NESTING + 2` deep:
//! chains (`a + b + c`, `x.f().g`, `a if c else b if d else e`) and
//! `elif`s are followed in loops and count as one level, as in the
//! lowering, the symbol table and the compiler.

use alloc::{string::String, vec::Vec};
use mm_compile::{
    CompileError, MAX_NESTING,
    ast::{
        Comprehension, Const, Expr, ExprKind, FPart, Params, Pattern, PatternKind, Pos, Stmt,
        StmtKind,
    },
};

use crate::parser::{MARK, MARK_CLASS, MARK_ENTER};

pub(crate) struct Walk<'a> {
    pub deferred: &'a mut [CompileError],
    /// Sorted (line, col) of `If` statements that came from `elif`.
    pub elifs: &'a [(u32, u32)],
    /// Folded constants (sorted by position) and the positions of the
    /// unary nodes and literal folded into each, innermost first.
    pub folds: &'a [(Pos, Vec<Pos>)],
    pub depth: u32,
}

type R = Result<(), CompileError>;

fn marker(e: &Expr) -> Option<(u8, usize)> {
    match (&e.kind, e.anchor.kind) {
        (ExprKind::Const(Const::None), MARK_ENTER | MARK | MARK_CLASS) => {
            Some((e.anchor.kind, e.anchor.left_line as usize))
        }
        _ => None,
    }
}

impl Walk<'_> {
    fn take(&mut self, idx: usize) -> CompileError {
        self.deferred.get_mut(idx).map_or_else(
            || CompileError::syntax("invalid syntax".into(), Pos::default()),
            |e| core::mem::replace(e, CompileError::syntax(String::new(), Pos::default())),
        )
    }

    fn enter(&mut self, pos: Pos) -> R {
        self.depth += 1;
        if self.depth > MAX_NESTING {
            return Err(CompileError::too_deep(pos));
        }
        Ok(())
    }

    pub(crate) fn body(&mut self, stmts: &[Stmt]) -> R {
        for s in stmts {
            self.stmt(s)?;
        }
        Ok(())
    }

    fn is_elif(&self, s: &Stmt) -> bool {
        self.elifs.binary_search(&(s.pos.line, s.pos.col)).is_ok()
    }

    #[allow(clippy::too_many_lines, reason = "one arm per statement kind")]
    fn stmt(&mut self, s: &Stmt) -> R {
        self.enter(s.pos)?;
        match &s.kind {
            StmtKind::Expr(e) => {
                if let Some((MARK, idx)) = marker(e) {
                    return Err(self.take(idx));
                }
                self.expr(e)?;
            }
            StmtKind::Assign(targets, value) => {
                self.expr(value)?;
                for t in targets {
                    self.expr(t)?;
                }
            }
            StmtKind::AugAssign(t, _, v) => {
                self.expr(t)?;
                self.expr(v)?;
            }
            StmtKind::AnnAssign(t, v, _) => {
                if let Some(v) = v {
                    self.expr(v)?;
                }
                self.expr(t)?;
            }
            StmtKind::If(test, body, orelse) => {
                self.expr(test)?;
                self.body(body)?;
                self.if_tail(orelse)?;
            }
            StmtKind::While(test, body, orelse) => {
                self.expr(test)?;
                self.body(body)?;
                self.body(orelse)?;
            }
            StmtKind::For(t, iter, body, orelse, _) => {
                self.expr(t)?;
                self.expr(iter)?;
                self.body(body)?;
                self.body(orelse)?;
            }
            StmtKind::Break
            | StmtKind::Continue
            | StmtKind::Pass
            | StmtKind::Global(_)
            | StmtKind::Nonlocal(_)
            | StmtKind::Import(_)
            | StmtKind::ImportFrom(..) => {}
            StmtKind::Return(v) => {
                if let Some(v) = v {
                    self.expr(v)?;
                }
            }
            StmtKind::FunctionDef(f) => {
                for d in &f.decorators {
                    self.expr(d)?;
                }
                self.params(&f.params)?;
                self.body(&f.body)?;
            }
            StmtKind::Delete(targets) => {
                for t in targets {
                    self.expr(t)?;
                }
            }
            StmtKind::Raise(exc, cause) => {
                if let Some(e) = exc {
                    self.expr(e)?;
                }
                if let Some(c) = cause {
                    self.expr(c)?;
                }
            }
            StmtKind::Try {
                body,
                handlers,
                orelse,
                finalbody,
                ..
            } => {
                self.body(body)?;
                for h in handlers {
                    if let Some(t) = &h.typ {
                        self.expr(t)?;
                    }
                    self.body(&h.body)?;
                }
                self.body(orelse)?;
                self.body(finalbody)?;
            }
            StmtKind::Assert(test, msg) => {
                if let Some(m) = msg {
                    self.expr(m)?;
                }
                self.expr(test)?;
            }
            StmtKind::ClassDef(c) => {
                for d in &c.decorators {
                    self.expr(d)?;
                }
                for b in &c.bases {
                    self.expr(b)?;
                }
                for k in &c.keywords {
                    if let Some((MARK, idx)) = marker(&k.value) {
                        return Err(self.take(idx));
                    }
                    self.expr(&k.value)?;
                }
                if let Some(Stmt {
                    kind: StmtKind::Expr(e),
                    ..
                }) = c.body.first()
                    && let Some((MARK_CLASS, idx)) = marker(e)
                {
                    return Err(self.take(idx));
                }
                self.body(&c.body)?;
            }
            StmtKind::With(items, body, _) => {
                for it in items {
                    if let Some(t) = &it.target {
                        self.expr(t)?;
                    }
                    self.expr(&it.ctx)?;
                }
                self.body(body)?;
            }
            StmtKind::Match(subject, cases) => {
                self.expr(subject)?;
                for c in cases {
                    self.pattern(&c.pattern)?;
                    if let Some(g) = &c.guard {
                        self.expr(g)?;
                    }
                    self.body(&c.body)?;
                }
            }
        }
        self.depth -= 1;
        Ok(())
    }

    fn pattern(&mut self, p: &Pattern) -> R {
        self.enter(p.pos)?;
        match &p.kind {
            PatternKind::Value(e) => self.expr(e)?,
            PatternKind::Singleton(_) | PatternKind::Star(_) | PatternKind::As(None, _) => {}
            PatternKind::Sequence(items) | PatternKind::Or(items) => {
                for x in items {
                    self.pattern(x)?;
                }
            }
            PatternKind::Mapping { keys, patterns, .. } => {
                for (k, x) in keys.iter().zip(patterns) {
                    self.expr(k)?;
                    self.pattern(x)?;
                }
            }
            PatternKind::Class {
                cls,
                patterns,
                kwd_patterns,
                ..
            } => {
                self.expr(cls)?;
                for x in patterns.iter().chain(kwd_patterns) {
                    self.pattern(x)?;
                }
            }
            PatternKind::As(Some(x), _) => self.pattern(x)?,
        }
        self.depth -= 1;
        Ok(())
    }

    /// The lowering built `elif` chains from the last clause outwards, at
    /// the `if` statement's depth: each clause's body, then its test.
    fn if_tail(&mut self, orelse: &[Stmt]) -> R {
        // Collect the chain of elif clauses (iteratively).
        let mut chain: Vec<&Stmt> = Vec::new();
        let mut cur = orelse;
        let mut tail: &[Stmt] = &[];
        loop {
            match cur {
                [s] if self.is_elif(s) => {
                    chain.push(s);
                    if let StmtKind::If(_, _, o) = &s.kind {
                        cur = o;
                    } else {
                        break;
                    }
                }
                _ => {
                    tail = cur;
                    break;
                }
            }
        }
        self.body(tail)?;
        for s in chain.iter().rev() {
            if let StmtKind::If(test, body, _) = &s.kind {
                self.body(body)?;
                self.expr(test)?;
            }
        }
        Ok(())
    }

    fn params(&mut self, p: &Params) -> R {
        for a in p.posonly.iter().chain(&p.args).chain(&p.kwonly) {
            if let Some(d) = &a.default {
                self.expr(d)?;
            }
        }
        Ok(())
    }

    fn comps(&mut self, gens: &[Comprehension]) -> R {
        for g in gens {
            if let Some((MARK, idx)) = marker(&g.target) {
                return Err(self.take(idx));
            }
            self.expr(&g.target)?;
            self.expr(&g.iter)?;
            for i in &g.ifs {
                self.expr(i)?;
            }
        }
        Ok(())
    }

    fn fparts(&mut self, parts: &[FPart]) -> R {
        for p in parts {
            if let FPart::Field { expr, spec, .. } = p {
                if let Some(spec) = spec {
                    self.fparts(spec)?;
                }
                self.expr(expr)?;
            }
        }
        Ok(())
    }

    /// The operands of chain step `e` after its first ([`Expr::chain_left`]).
    fn chain_rest(&mut self, e: &Expr) -> R {
        match &e.kind {
            ExprKind::BinOp(_, _, r) | ExprKind::Subscript(_, r) => self.expr(r),
            ExprKind::Call(_, args, kws) => {
                for a in args {
                    self.expr(a)?;
                }
                for k in kws {
                    self.expr(&k.value)?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn opt(&mut self, e: Option<&Expr>) -> R {
        match e {
            Some(e) => self.expr(e),
            None => Ok(()),
        }
    }

    #[allow(clippy::too_many_lines, reason = "one arm per expression kind")]
    fn expr(&mut self, e: &Expr) -> R {
        self.enter(e.pos)?;
        if let Some((_, idx)) = marker(e) {
            return Err(self.take(idx));
        }
        // A left-nested chain (`a + b + c`, `x.f(1).g`) is one level: its
        // innermost operand, then each step's other operands, in a loop.
        if e.chain_left().and_then(Expr::chain_left).is_some() {
            let mut steps = alloc::vec![e];
            let mut cur = e;
            while let Some(c) = cur.chain_left() {
                steps.push(c);
                cur = c;
            }
            steps.pop();
            self.expr(cur)?;
            for s in steps.into_iter().rev() {
                self.chain_rest(s)?;
            }
            self.depth -= 1;
            return Ok(());
        }
        match &e.kind {
            ExprKind::Const(_) => {
                // A folded `-literal`: the lowering entered every node.
                let key = (e.pos.line, e.pos.col, e.pos.end_line, e.pos.end_col);
                if let Ok(i) = self
                    .folds
                    .binary_search_by_key(&key, |f| (f.0.line, f.0.col, f.0.end_line, f.0.end_col))
                {
                    let chain = &self.folds[i].1;
                    for p in chain.iter().rev() {
                        self.enter(*p)?;
                    }
                    self.depth -= u32::try_from(chain.len()).unwrap_or(0);
                }
            }
            ExprKind::Name(_) => {}
            ExprKind::BoolOp(_, values) => {
                for v in values {
                    self.expr(v)?;
                }
            }
            ExprKind::Named(_, v)
            | ExprKind::Unary(_, v)
            | ExprKind::Await(v)
            | ExprKind::YieldFrom(v)
            | ExprKind::Starred(v) => self.expr(v)?,
            ExprKind::BinOp(l, ..)
            | ExprKind::Subscript(l, _)
            | ExprKind::Attribute(l, _)
            | ExprKind::Call(l, ..) => {
                self.expr(l)?;
                self.chain_rest(e)?;
            }
            ExprKind::Lambda(f) => {
                self.params(&f.params)?;
                if let Some(Stmt {
                    kind: StmtKind::Return(Some(body)),
                    ..
                }) = f.body.first()
                {
                    self.expr(body)?;
                }
            }
            ExprKind::IfExp(..) => {
                // A conditional in an `else` is the same level.
                let mut cur = e;
                while let ExprKind::IfExp(test, body, orelse) = &cur.kind {
                    self.expr(test)?;
                    self.expr(body)?;
                    cur = orelse;
                }
                self.expr(cur)?;
            }
            ExprKind::Dict(items) => {
                for (k, v) in items {
                    if let Some(k) = k {
                        self.expr(k)?;
                    }
                    self.expr(v)?;
                }
            }
            ExprKind::Set(elts) | ExprKind::List(elts) | ExprKind::Tuple(elts) => {
                for x in elts {
                    self.expr(x)?;
                }
            }
            ExprKind::ListComp(elt, gens)
            | ExprKind::SetComp(elt, gens)
            | ExprKind::GenExp(elt, gens) => {
                self.expr(elt)?;
                self.comps(gens)?;
            }
            ExprKind::DictComp(k, v, gens) => {
                self.expr(k)?;
                self.expr(v)?;
                self.comps(gens)?;
            }
            ExprKind::Yield(v) => self.opt(v.as_deref())?,
            ExprKind::Compare(first, rest) => {
                self.expr(first)?;
                for (_, x) in rest {
                    self.expr(x)?;
                }
            }
            ExprKind::FString(parts) => self.fparts(parts)?,
            ExprKind::Slice(a, b, c) => {
                self.opt(a.as_deref())?;
                self.opt(b.as_deref())?;
                self.opt(c.as_deref())?;
            }
        }
        self.depth -= 1;
        Ok(())
    }
}
