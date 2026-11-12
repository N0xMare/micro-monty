//! The phase-1 front end, kept only as a differential oracle
//! (`--features ruff-oracle`): `ruff_python_parser` plus the lowering the
//! engine was built against. [`crate::parse`] must agree with it on every
//! corpus file (`tests/ruff_parity.rs`).

#![allow(clippy::doc_markdown, reason = "the docs name CPython in prose")]
#![allow(
    clippy::missing_errors_doc,
    reason = "every fallible function returns the one `CompileError`"
)]

use mm_compile::ast::{
    Alias, Anchor, BinOp, BoolOp, ClassDef, CmpOp, Comprehension, Const, Expr, ExprKind, FPart,
    FuncDef, Handler, Keyword, MatchCase, Module, Param, Params, Pattern, PatternKind, Pos, Stmt,
    StmtKind, UnaryOp, WithItem,
};
use mm_compile::{CompileError, MAX_NESTING};
use ruff_python_ast as ra;
use ruff_python_ast::InterpolatedStringElement as IElem;
use ruff_text_size::Ranged;

/// Parses `source` with ruff and lowers it (the phase-1 path).
pub fn parse(source: &str) -> Result<Module, CompileError> {
    crate::prescan::check(source)?;
    // Line breaks as `\n`, as CPython's tokenizer reads them: ruff keeps
    // `\r\n` and a lone `\r` in string values. Lines and columns do not
    // move (a column counts from its line's start).
    let source = &source.replace("\r\n", "\n").replace('\r', "\n");
    let parsed = ruff_python_parser::parse_module(source).map_err(|e| {
        let lx = Lines::new(source);
        let pos = lx.pos(e.location.start().to_usize(), e.location.end().to_usize());
        CompileError::syntax(e.error.to_string(), pos)
    })?;
    let module = parsed.into_syntax();
    let mut lower = Lower {
        src: source.as_bytes(),
        lines: Lines::new(source),
        depth: 0,
        in_class: false,
        annotations: 0,
    };
    let body = lower.body(&module.body)?;
    Ok(Module { body })
}

/// Line starts (after each `\n`, `\r\n` or lone `\r`, as CPython counts
/// them), for byte offset to (line, character column) conversion.
struct Lines<'s> {
    src: &'s str,
    starts: Vec<usize>,
}

impl<'s> Lines<'s> {
    fn new(src: &'s str) -> Lines<'s> {
        let mut starts = vec![0];
        let b = src.as_bytes();
        for (i, &c) in b.iter().enumerate() {
            if c == b'\n' || (c == b'\r' && b.get(i + 1) != Some(&b'\n')) {
                starts.push(i + 1);
            }
        }
        Lines { src, starts }
    }

    fn loc(&self, off: usize) -> (u32, u32) {
        let line = self.starts.partition_point(|&s| s <= off).max(1);
        let start = self.starts[line - 1];
        let off = off.min(self.src.len());
        let text = self.src.get(start..off).unwrap_or("");
        let col = text.chars().count();
        (
            u32::try_from(line).unwrap_or(u32::MAX),
            u32::try_from(col).unwrap_or(u32::MAX),
        )
    }

    fn pos(&self, start: usize, end: usize) -> Pos {
        let (line, col) = self.loc(start);
        let (end_line, end_col) = self.loc(end);
        Pos {
            line,
            col,
            end_line,
            end_col,
        }
    }
}

struct Lower<'s> {
    src: &'s [u8],
    lines: Lines<'s>,
    depth: u32,
    /// Lowering a class body (not a function nested in one).
    in_class: bool,
    /// Class-body annotations stored in the current class so far.
    annotations: u32,
}

type R<T> = Result<T, CompileError>;

fn unsupported(what: &str, pos: Pos) -> CompileError {
    CompileError::unsupported(what, pos)
}

/// `__future__` features (all no-ops in micro-monty).
const FUTURE_FEATURES: [&str; 10] = [
    "nested_scopes",
    "generators",
    "division",
    "absolute_import",
    "with_statement",
    "print_function",
    "unicode_literals",
    "barry_as_FLUFL",
    "generator_stop",
    "annotations",
];

fn alias(a: &ra::Alias) -> Alias {
    Alias {
        name: a.name.id.as_str().to_string(),
        asname: a.asname.as_ref().map(|n| n.id.as_str().to_string()),
        pos: Pos::default(),
    }
}

fn binop(op: ra::Operator) -> BinOp {
    use ra::Operator as O;
    match op {
        O::Add => BinOp::Add,
        O::Sub => BinOp::Sub,
        O::Mult => BinOp::Mul,
        O::MatMult => BinOp::MatMul,
        O::Div => BinOp::Div,
        O::Mod => BinOp::Mod,
        O::Pow => BinOp::Pow,
        O::LShift => BinOp::LShift,
        O::RShift => BinOp::RShift,
        O::BitOr => BinOp::BitOr,
        O::BitXor => BinOp::BitXor,
        O::BitAnd => BinOp::BitAnd,
        O::FloorDiv => BinOp::FloorDiv,
    }
}

fn cmpop(op: ra::CmpOp) -> CmpOp {
    use ra::CmpOp as C;
    match op {
        C::Eq => CmpOp::Eq,
        C::NotEq => CmpOp::Ne,
        C::Lt => CmpOp::Lt,
        C::LtE => CmpOp::Le,
        C::Gt => CmpOp::Gt,
        C::GtE => CmpOp::Ge,
        C::Is => CmpOp::Is,
        C::IsNot => CmpOp::IsNot,
        C::In => CmpOp::In,
        C::NotIn => CmpOp::NotIn,
    }
}

impl Lower<'_> {
    fn pos<T: Ranged>(&self, node: &T) -> Pos {
        self.lines
            .pos(node.start().to_usize(), node.end().to_usize())
    }

    fn enter(&mut self, pos: Pos) -> R<()> {
        self.depth += 1;
        if self.depth > MAX_NESTING {
            return Err(CompileError::too_deep(pos));
        }
        Ok(())
    }

    // ------------------------------------------------------- anchors

    /// Moves past the end of a line, a comment or a line continuation.
    fn next_line(&self, mut i: usize) -> usize {
        while i < self.src.len() && !matches!(self.src[i], b'\n' | b'\r') {
            i += 1;
        }
        i + 1 + usize::from(self.src.get(i..i + 2) == Some(b"\r\n"))
    }

    /// CPython's `increment_until`: the first offset from `i` whose byte
    /// satisfies `stop`, skipping comments and continuations.
    fn scan_until(&self, mut i: usize, stop: impl Fn(u8) -> bool) -> Option<usize> {
        while i < self.src.len() {
            let ch = self.src[i];
            if ch == b'\\' || ch == b'#' {
                i = self.next_line(i);
            } else if stop(ch) {
                return Some(i);
            } else {
                i += 1;
            }
        }
        None
    }

    fn binop_anchor(&self, left: &ra::Expr, right: &ra::Expr) -> Anchor {
        let Some(op) = self.scan_until(left.end().to_usize(), |c| {
            !c.is_ascii_whitespace() && c != b')'
        }) else {
            return Anchor::default();
        };
        let mut end = op + 1;
        let right_start = right.start().to_usize();
        if let Some(&ch) = self.src.get(end)
            && end < right_start
            && ch != b'\n'
            && ch != b'\r'
            && !ch.is_ascii_whitespace()
            && ch != b'\\'
            && ch != b'#'
        {
            end += 1;
        }
        let (l1, c1) = self.lines.loc(op);
        let (l2, c2) = self.lines.loc(end);
        Anchor {
            kind: 1,
            left_line: l1,
            left_col: c1,
            right_line: l2,
            right_col: c2,
        }
    }

    fn bracket_anchor(&self, value_end: usize, open: u8, expr_end: usize) -> Anchor {
        let Some(at) = self.scan_until(value_end, |c| c == open) else {
            return Anchor::default();
        };
        let (l1, c1) = self.lines.loc(at);
        let (l2, c2) = self.lines.loc(expr_end);
        Anchor {
            kind: 2,
            left_line: l1,
            left_col: c1,
            right_line: l2,
            right_col: c2,
        }
    }

    /// Is the statement at `start` the first thing on its line?
    fn starts_line(&self, s: usize) -> bool {
        let mut i = s;
        while i > 0 && !matches!(self.src[i - 1], b'\n' | b'\r') {
            i -= 1;
            if !matches!(self.src[i], b' ' | b'\t' | b'\x0c') {
                return false;
            }
        }
        true
    }

    // ---------------------------------------------------- statements

    fn body(&mut self, stmts: &[ra::Stmt]) -> R<Vec<Stmt>> {
        let mut out = Vec::with_capacity(stmts.len());
        for s in stmts {
            out.push(self.stmt(s)?);
            if self.in_class
                && let ra::Stmt::AnnAssign(a) = s
                && a.simple
                && let ra::Expr::Name(n) = a.target.as_ref()
            {
                // `__annotations__['x'] = 'int'` (see `crate::parser`): the
                // annotation lowered and unparsed, or its source text when
                // micro-monty rejects something in it.
                let depth = self.depth;
                let lowered = self.expr(&a.annotation);
                self.depth = depth;
                let src = core::str::from_utf8(self.src).unwrap_or_default();
                let text = match lowered {
                    Ok(e) => crate::unparse::annotation(&e, src),
                    Err(_) => crate::unparse::slice(src, self.pos(&*a.annotation)).to_string(),
                };
                out.push(crate::parser::annotation_store(
                    n.id.as_str(),
                    text,
                    self.pos(s),
                ));
                self.annotations += 1;
            }
        }
        Ok(out)
    }

    #[allow(clippy::too_many_lines, reason = "one arm per statement kind")]
    fn stmt(&mut self, s: &ra::Stmt) -> R<Stmt> {
        let pos = self.pos(s);
        self.enter(pos)?;
        let kind = match s {
            ra::Stmt::Expr(e) => StmtKind::Expr(self.expr(&e.value)?),
            ra::Stmt::Assign(a) => {
                let mut value = self.expr(&a.value)?;
                if a.targets.len() == 1
                    && a.targets[0].is_name_expr()
                    && matches!(value.kind, ExprKind::Call(..))
                    && value.pos.line == pos.line
                    && self.starts_line(s.start().to_usize())
                {
                    value.no_carets = true;
                }
                let mut targets = Vec::with_capacity(a.targets.len());
                for t in &a.targets {
                    targets.push(self.expr(t)?);
                }
                StmtKind::Assign(targets, value)
            }
            ra::Stmt::AugAssign(a) => {
                StmtKind::AugAssign(self.expr(&a.target)?, binop(a.op), self.expr(&a.value)?)
            }
            ra::Stmt::AnnAssign(a) => {
                let value = match &a.value {
                    Some(v) => Some(self.expr(v)?),
                    None => None,
                };
                StmtKind::AnnAssign(self.expr(&a.target)?, value, None)
            }
            ra::Stmt::If(i) => {
                let test = self.expr(&i.test)?;
                let body = self.body(&i.body)?;
                let mut orelse = Vec::new();
                // elif chains nest from the last clause outwards.
                for clause in i.elif_else_clauses.iter().rev() {
                    let cbody = self.body(&clause.body)?;
                    match &clause.test {
                        None => orelse = cbody,
                        Some(t) => {
                            let cpos = self.pos(clause);
                            let test = self.expr(t)?;
                            orelse = vec![Stmt {
                                kind: StmtKind::If(test, cbody, orelse),
                                pos: cpos,
                            }];
                        }
                    }
                }
                StmtKind::If(test, body, orelse)
            }
            ra::Stmt::While(w) => StmtKind::While(
                self.expr(&w.test)?,
                self.body(&w.body)?,
                self.body(&w.orelse)?,
            ),
            ra::Stmt::For(f) => StmtKind::For(
                self.expr(&f.target)?,
                self.expr(&f.iter)?,
                self.body(&f.body)?,
                self.body(&f.orelse)?,
                f.is_async,
            ),
            ra::Stmt::Break(_) => StmtKind::Break,
            ra::Stmt::Continue(_) => StmtKind::Continue,
            ra::Stmt::Pass(_) => StmtKind::Pass,
            ra::Stmt::Return(r) => match &r.value {
                Some(v) => {
                    let mut value = self.expr(v)?;
                    if let ExprKind::Call(func, ..) = &value.kind
                        && matches!(func.kind, ExprKind::Name(_))
                        && value.pos.line == pos.line
                        && self.starts_line(s.start().to_usize())
                    {
                        value.no_carets = true;
                    }
                    StmtKind::Return(Some(value))
                }
                None => StmtKind::Return(None),
            },
            ra::Stmt::FunctionDef(f) => {
                if f.type_params.is_some() {
                    return Err(unsupported("type parameters", pos));
                }
                let mut decorators = Vec::new();
                for d in &f.decorator_list {
                    decorators.push(self.expr(&d.expression)?);
                }
                let params = self.params(&f.parameters)?;
                let in_class = core::mem::replace(&mut self.in_class, false);
                let body = self.body(&f.body);
                self.in_class = in_class;
                let body = body?;
                let mut fpos = pos;
                fpos.end_line = fpos.line;
                fpos.end_col = fpos.col;
                StmtKind::FunctionDef(Box::new(FuncDef {
                    name: f.name.id.as_str().to_string(),
                    params,
                    body,
                    decorators,
                    pos: fpos,
                    is_async: f.is_async,
                    returns: None,
                }))
            }
            ra::Stmt::Delete(d) => {
                let mut targets = Vec::new();
                for t in &d.targets {
                    targets.push(self.expr(t)?);
                }
                StmtKind::Delete(targets)
            }
            ra::Stmt::Global(g) => {
                StmtKind::Global(g.names.iter().map(|n| n.id.as_str().to_string()).collect())
            }
            ra::Stmt::Nonlocal(g) => {
                StmtKind::Nonlocal(g.names.iter().map(|n| n.id.as_str().to_string()).collect())
            }
            ra::Stmt::Raise(r) => {
                let exc = match &r.exc {
                    Some(e) => Some(self.expr(e)?),
                    None => None,
                };
                let cause = match &r.cause {
                    Some(c) => Some(self.expr(c)?),
                    None => None,
                };
                StmtKind::Raise(exc, cause)
            }
            ra::Stmt::Try(t) => {
                let body = self.body(&t.body)?;
                let mut handlers = Vec::new();
                for h in &t.handlers {
                    let ra::ExceptHandler::ExceptHandler(h) = h;
                    let typ = match &h.type_ {
                        Some(e) => Some(self.expr(e)?),
                        None => None,
                    };
                    let mut hpos = self.pos(h);
                    if let Some(t) = &typ {
                        hpos.end_line = t.pos.end_line;
                        hpos.end_col = t.pos.end_col;
                    } else {
                        hpos.end_line = hpos.line;
                        hpos.end_col = hpos.col + 6;
                    }
                    handlers.push(Handler {
                        typ,
                        name: h.name.as_ref().map(|n| n.id.as_str().to_string()),
                        body: self.body(&h.body)?,
                        pos: hpos,
                    });
                }
                StmtKind::Try {
                    body,
                    handlers,
                    orelse: self.body(&t.orelse)?,
                    finalbody: self.body(&t.finalbody)?,
                    star: t.is_star,
                }
            }
            ra::Stmt::Assert(a) => {
                let msg = match &a.msg {
                    Some(m) => Some(self.expr(m)?),
                    None => None,
                };
                StmtKind::Assert(self.expr(&a.test)?, msg)
            }
            ra::Stmt::ClassDef(c) => {
                if c.type_params.is_some() {
                    return Err(unsupported("type parameters", pos));
                }
                let mut decorators = Vec::new();
                for d in &c.decorator_list {
                    decorators.push(self.expr(&d.expression)?);
                }
                let mut bases = Vec::new();
                let mut keywords = Vec::new();
                if let Some(args) = &c.arguments {
                    bases = self.exprs(&args.args)?;
                    for k in &args.keywords {
                        let name = k.arg.as_ref().map(|a| a.id.as_str().to_string());
                        keywords.push(Keyword {
                            name,
                            value: self.expr(&k.value)?,
                            pos: self.pos(k),
                        });
                    }
                }
                for s in &c.body {
                    if let Some(what) = class_body_unsupported(s) {
                        return Err(unsupported(what, self.pos(s)));
                    }
                }
                let in_class = core::mem::replace(&mut self.in_class, true);
                let outer = core::mem::replace(&mut self.annotations, 0);
                let body = self.body(&c.body);
                self.in_class = in_class;
                let annotated = core::mem::replace(&mut self.annotations, outer) != 0;
                let mut body = body?;
                if annotated {
                    crate::parser::annotations_dict(&mut body, pos);
                }
                // The statement without its decorators: from the `class`
                // keyword (tracebacks show it for the class body call).
                let name_start = c.name.start().to_usize();
                let head = self.src.get(..name_start).unwrap_or_default();
                let kw = head.trim_ascii_end().len().saturating_sub(5);
                let span = self.lines.pos(kw, s.end().to_usize());
                let mut cpos = span;
                cpos.end_line = cpos.line;
                cpos.end_col = cpos.col;
                StmtKind::ClassDef(Box::new(ClassDef {
                    name: c.name.id.as_str().to_string(),
                    bases,
                    keywords,
                    body,
                    decorators,
                    pos: cpos,
                    span,
                }))
            }
            ra::Stmt::Import(i) => {
                if i.is_lazy {
                    return Err(unsupported("lazy imports", pos));
                }
                StmtKind::Import(i.names.iter().map(alias).collect())
            }
            ra::Stmt::ImportFrom(i) => {
                if i.is_lazy {
                    return Err(unsupported("lazy imports", pos));
                }
                let module = i.module.as_ref().map_or("", |m| m.id.as_str());
                if i.level == 0 && module == "__future__" {
                    // Compiler directives: the known features are no-ops.
                    for a in &i.names {
                        let feature = a.name.id.as_str();
                        if !FUTURE_FEATURES.contains(&feature) {
                            return Err(CompileError::syntax(
                                format!("future feature {feature} is not defined"),
                                self.pos(a),
                            ));
                        }
                    }
                    StmtKind::Pass
                } else {
                    // A relative import keeps its dots; it fails at run time
                    // as in a script, which has no parent package.
                    let level = usize::try_from(i.level).unwrap_or(0);
                    let module = format!("{}{module}", ".".repeat(level));
                    StmtKind::ImportFrom(module, i.names.iter().map(alias).collect())
                }
            }
            ra::Stmt::With(w) => {
                let mut items = Vec::new();
                for it in &w.items {
                    let target = match &it.optional_vars {
                        Some(t) => Some(self.expr(t)?),
                        None => None,
                    };
                    items.push(WithItem {
                        ctx: self.expr(&it.context_expr)?,
                        target,
                    });
                }
                StmtKind::With(items, self.body(&w.body)?, w.is_async)
            }
            ra::Stmt::Match(m) => {
                let subject = self.expr(&m.subject)?;
                let mut cases = Vec::with_capacity(m.cases.len());
                for c in &m.cases {
                    let pattern = self.pattern(&c.pattern)?;
                    let guard = match &c.guard {
                        Some(g) => Some(self.expr(g)?),
                        None => None,
                    };
                    cases.push(MatchCase {
                        pattern,
                        guard,
                        body: self.body(&c.body)?,
                    });
                }
                StmtKind::Match(subject, cases)
            }
            ra::Stmt::TypeAlias(_) => return Err(unsupported("type aliases", pos)),
            ra::Stmt::IpyEscapeCommand(_) => {
                return Err(unsupported("IPython escape commands", pos));
            }
        };
        self.depth -= 1;
        Ok(Stmt { kind, pos })
    }

    fn pattern(&mut self, p: &ra::Pattern) -> R<Pattern> {
        let pos = self.pos(p);
        self.enter(pos)?;
        let name = |n: &ra::Identifier| n.id.as_str().to_string();
        let kind = match p {
            ra::Pattern::MatchValue(v) => PatternKind::Value(self.expr(&v.value)?),
            ra::Pattern::MatchSingleton(v) => PatternKind::Singleton(match v.value {
                ra::Singleton::None => Const::None,
                ra::Singleton::True => Const::Bool(true),
                ra::Singleton::False => Const::Bool(false),
            }),
            ra::Pattern::MatchSequence(v) => {
                let mut items = Vec::with_capacity(v.patterns.len());
                for x in &v.patterns {
                    items.push(self.pattern(x)?);
                }
                PatternKind::Sequence(items)
            }
            ra::Pattern::MatchStar(v) => PatternKind::Star(v.name.as_ref().map(name)),
            ra::Pattern::MatchMapping(v) => {
                let mut keys = Vec::with_capacity(v.keys.len());
                let mut patterns = Vec::with_capacity(v.patterns.len());
                for (k, x) in v.keys.iter().zip(&v.patterns) {
                    keys.push(self.expr(k)?);
                    patterns.push(self.pattern(x)?);
                }
                PatternKind::Mapping {
                    keys,
                    patterns,
                    rest: v.rest.as_ref().map(name),
                }
            }
            ra::Pattern::MatchClass(v) => {
                let cls = self.expr(&v.cls)?;
                let mut patterns = Vec::new();
                for x in &v.arguments.patterns {
                    patterns.push(self.pattern(x)?);
                }
                let mut kwd_attrs = Vec::new();
                let mut kwd_patterns = Vec::new();
                for k in &v.arguments.keywords {
                    kwd_attrs.push(name(&k.attr));
                    kwd_patterns.push(self.pattern(&k.pattern)?);
                }
                PatternKind::Class {
                    cls,
                    patterns,
                    kwd_attrs,
                    kwd_patterns,
                }
            }
            ra::Pattern::MatchAs(v) => {
                let inner = match &v.pattern {
                    Some(x) => Some(Box::new(self.pattern(x)?)),
                    None => None,
                };
                PatternKind::As(inner, v.name.as_ref().map(name))
            }
            ra::Pattern::MatchOr(v) => {
                let mut items = Vec::with_capacity(v.patterns.len());
                for x in &v.patterns {
                    items.push(self.pattern(x)?);
                }
                PatternKind::Or(items)
            }
        };
        self.depth -= 1;
        Ok(Pattern { kind, pos })
    }

    fn params(&mut self, p: &ra::Parameters) -> R<Params> {
        let mut out = Params::default();
        let conv = |this: &mut Self, list: &[ra::ParameterWithDefault]| -> R<Vec<Param>> {
            let mut v = Vec::new();
            for a in list {
                let default = match &a.default {
                    Some(d) => Some(this.expr(d)?),
                    None => None,
                };
                v.push(Param {
                    name: a.parameter.name.id.as_str().to_string(),
                    default,
                    ann: None,
                    pos: Pos::default(),
                });
            }
            Ok(v)
        };
        out.posonly = conv(self, &p.posonlyargs)?;
        out.args = conv(self, &p.args)?;
        out.kwonly = conv(self, &p.kwonlyargs)?;
        out.vararg = p.vararg.as_ref().map(|v| v.name.id.as_str().to_string());
        out.kwarg = p.kwarg.as_ref().map(|v| v.name.id.as_str().to_string());
        Ok(out)
    }

    // --------------------------------------------------- expressions

    /// A chain operand: the node `expr` lowered already, else `e`'s.
    fn link(&mut self, chained: Option<Expr>, e: &ra::Expr) -> R<Expr> {
        chained.map_or_else(|| self.expr(e), Ok)
    }

    fn exprs(&mut self, list: &[ra::Expr]) -> R<Vec<Expr>> {
        let mut out = Vec::with_capacity(list.len());
        for e in list {
            out.push(self.expr(e)?);
        }
        Ok(out)
    }

    fn opt(&mut self, e: Option<&ra::Expr>) -> R<Option<Box<Expr>>> {
        match e {
            Some(e) => Ok(Some(Box::new(self.expr(e)?))),
            None => Ok(None),
        }
    }

    fn comps(&mut self, gens: &[ra::Comprehension]) -> R<Vec<Comprehension>> {
        let mut out = Vec::new();
        for g in gens {
            out.push(Comprehension {
                target: self.expr(&g.target)?,
                iter: self.expr(&g.iter)?,
                ifs: self.exprs(&g.ifs)?,
                is_async: g.is_async,
            });
        }
        Ok(out)
    }

    fn felems(&mut self, elems: &[IElem], out: &mut Vec<FPart>) -> R<()> {
        for el in elems {
            match el {
                IElem::Literal(l) => push_lit(out, &l.value),
                IElem::Interpolation(i) => {
                    let mut conv = i.conversion.to_byte().unwrap_or(0);
                    if let Some(d) = &i.debug_text {
                        push_lit(out, d.leading());
                        push_lit(out, d.expression());
                        push_lit(out, d.trailing());
                        if conv == 0 && i.format_spec.is_none() {
                            conv = b'r';
                        }
                    }
                    let spec = match &i.format_spec {
                        Some(s) => {
                            let mut parts = Vec::new();
                            self.felems(&s.elements, &mut parts)?;
                            Some(parts)
                        }
                        None => None,
                    };
                    out.push(FPart::Field {
                        expr: Box::new(self.expr(&i.expression)?),
                        conv,
                        spec,
                        pos: self.pos(i),
                    });
                }
            }
        }
        Ok(())
    }

    /// `e` lowered. A chain (`a + b + ...`, `x.f().g[0]`, `a if c else b if
    /// ...`) is one level of nesting however long, as `crate::walk` counts
    /// it, and is lowered in a loop from its innermost step out.
    fn expr(&mut self, e: &ra::Expr) -> R<Expr> {
        let mut spine = vec![e];
        let mut cur = e;
        while let Some(next) = chain_next(cur) {
            spine.push(next);
            cur = next;
        }
        let mut v = None;
        while let Some(step) = spine.pop() {
            v = Some(self.one(step, v)?);
        }
        v.ok_or_else(|| unsupported("an empty chain", self.pos(e)))
    }

    /// One node, with its chain operand (`chain_next`) already lowered when
    /// `chained` is given.
    #[allow(clippy::too_many_lines, reason = "one arm per expression kind")]
    fn one(&mut self, e: &ra::Expr, mut chained: Option<Expr>) -> R<Expr> {
        let pos = self.pos(e);
        self.enter(pos)?;
        let mut anchor = Anchor::default();
        let kind = match e {
            ra::Expr::BoolOp(b) => ExprKind::BoolOp(
                if b.op == ra::BoolOp::And {
                    BoolOp::And
                } else {
                    BoolOp::Or
                },
                self.exprs(&b.values)?,
            ),
            ra::Expr::Named(n) => {
                let ra::Expr::Name(name) = n.target.as_ref() else {
                    return Err(CompileError::syntax(
                        "cannot use assignment expressions with this target".into(),
                        pos,
                    ));
                };
                ExprKind::Named(name.id.as_str().to_string(), Box::new(self.expr(&n.value)?))
            }
            ra::Expr::BinOp(b) => {
                anchor = self.binop_anchor(&b.left, &b.right);
                ExprKind::BinOp(
                    Box::new(self.link(chained.take(), &b.left)?),
                    binop(b.op),
                    Box::new(self.expr(&b.right)?),
                )
            }
            ra::Expr::UnaryOp(u) => {
                let op = match u.op {
                    ra::UnaryOp::Invert => UnaryOp::Invert,
                    ra::UnaryOp::Not => UnaryOp::Not,
                    ra::UnaryOp::UAdd => UnaryOp::Pos,
                    ra::UnaryOp::USub => UnaryOp::Neg,
                };
                // Fold negative literals so `-5` is a constant, as CPython does.
                let inner = self.expr(&u.operand)?;
                match (op, &inner.kind) {
                    (UnaryOp::Neg, ExprKind::Const(Const::Int(n))) if *n != i64::MIN => {
                        ExprKind::Const(Const::Int(-n))
                    }
                    (UnaryOp::Neg, ExprKind::Const(Const::Float(f))) => {
                        ExprKind::Const(Const::Float(-f))
                    }
                    _ => ExprKind::Unary(op, Box::new(inner)),
                }
            }
            ra::Expr::Lambda(l) => {
                let params = match &l.parameters {
                    Some(p) => self.params(p)?,
                    None => Params::default(),
                };
                let body = self.expr(&l.body)?;
                let bpos = body.pos;
                ExprKind::Lambda(Box::new(FuncDef {
                    name: "<lambda>".into(),
                    params,
                    body: vec![Stmt {
                        kind: StmtKind::Return(Some(body)),
                        pos: bpos,
                    }],
                    decorators: Vec::new(),
                    pos,
                    is_async: false,
                    returns: None,
                }))
            }
            ra::Expr::If(i) => ExprKind::IfExp(
                Box::new(self.expr(&i.test)?),
                Box::new(self.expr(&i.body)?),
                Box::new(self.link(chained.take(), &i.orelse)?),
            ),
            ra::Expr::Dict(d) => {
                let mut items = Vec::new();
                for it in &d.items {
                    let k = match &it.key {
                        Some(k) => Some(self.expr(k)?),
                        None => None,
                    };
                    items.push((k, self.expr(&it.value)?));
                }
                ExprKind::Dict(items)
            }
            ra::Expr::Set(s) => ExprKind::Set(self.exprs(&s.elts)?),
            ra::Expr::ListComp(c) => {
                ExprKind::ListComp(Box::new(self.expr(&c.elt)?), self.comps(&c.generators)?)
            }
            ra::Expr::SetComp(c) => {
                ExprKind::SetComp(Box::new(self.expr(&c.elt)?), self.comps(&c.generators)?)
            }
            ra::Expr::DictComp(c) => {
                let Some(key) = &c.key else {
                    return Err(unsupported("dict comprehension unpacking", pos));
                };
                ExprKind::DictComp(
                    Box::new(self.expr(key)?),
                    Box::new(self.expr(&c.value)?),
                    self.comps(&c.generators)?,
                )
            }
            ra::Expr::Generator(g) => {
                ExprKind::GenExp(Box::new(self.expr(&g.elt)?), self.comps(&g.generators)?)
            }
            ra::Expr::Await(a) => ExprKind::Await(Box::new(self.expr(&a.value)?)),
            ra::Expr::Yield(y) => ExprKind::Yield(self.opt(y.value.as_deref())?),
            ra::Expr::YieldFrom(y) => ExprKind::YieldFrom(Box::new(self.expr(&y.value)?)),
            ra::Expr::Compare(c) => {
                let mut rest = Vec::new();
                let first = self.expr(&c.operands[0])?;
                for (op, x) in c.ops.iter().zip(c.operands.iter().skip(1)) {
                    rest.push((cmpop(*op), self.expr(x)?));
                }
                ExprKind::Compare(Box::new(first), rest)
            }
            ra::Expr::Call(c) => {
                anchor = self.bracket_anchor(c.func.end().to_usize(), b'(', c.end().to_usize());
                let func = self.link(chained.take(), &c.func)?;
                let args = self.exprs(&c.arguments.args)?;
                let mut kws = Vec::new();
                for k in &c.arguments.keywords {
                    kws.push(Keyword {
                        name: k.arg.as_ref().map(|a| a.id.as_str().to_string()),
                        value: self.expr(&k.value)?,
                        pos: self.pos(k),
                    });
                }
                ExprKind::Call(Box::new(func), args, kws)
            }
            ra::Expr::FString(f) => {
                let mut parts = Vec::new();
                for part in &f.value {
                    match part {
                        ra::FStringPartRef::Literal(l) => push_lit(&mut parts, &l.value),
                        ra::FStringPartRef::FString(fs) => self.felems(&fs.elements, &mut parts)?,
                    }
                }
                if parts.is_empty() {
                    parts.push(FPart::Lit(String::new()));
                }
                ExprKind::FString(parts)
            }
            ra::Expr::TString(_) => return Err(unsupported("t-strings", pos)),
            ra::Expr::StringLiteral(s) => ExprKind::Const(Const::Str(s.value.to_str().to_string())),
            ra::Expr::BytesLiteral(b) => {
                let mut data = Vec::new();
                for part in &b.value {
                    data.extend_from_slice(&part.value);
                }
                ExprKind::Const(Const::Bytes(data))
            }
            ra::Expr::NumberLiteral(n) => match &n.value {
                ra::Number::Int(i) => match i.as_i64() {
                    Some(v) => ExprKind::Const(Const::Int(v)),
                    None => ExprKind::Const(Const::BigInt(i.to_string())),
                },
                ra::Number::Float(f) => ExprKind::Const(Const::Float(*f)),
                ra::Number::Complex { imag, .. } => ExprKind::Const(Const::Complex(*imag)),
            },
            ra::Expr::BooleanLiteral(b) => ExprKind::Const(Const::Bool(b.value)),
            ra::Expr::NoneLiteral(_) => ExprKind::Const(Const::None),
            ra::Expr::EllipsisLiteral(_) => ExprKind::Const(Const::Ellipsis),
            ra::Expr::Attribute(a) => ExprKind::Attribute(
                Box::new(self.link(chained.take(), &a.value)?),
                a.attr.id.as_str().to_string(),
            ),
            ra::Expr::Subscript(s) => {
                anchor = self.bracket_anchor(s.value.end().to_usize(), b'[', s.end().to_usize());
                ExprKind::Subscript(
                    Box::new(self.link(chained.take(), &s.value)?),
                    Box::new(self.expr(&s.slice)?),
                )
            }
            ra::Expr::Starred(s) => ExprKind::Starred(Box::new(self.expr(&s.value)?)),
            ra::Expr::Name(n) => ExprKind::Name(n.id.as_str().to_string()),
            ra::Expr::List(l) => ExprKind::List(self.exprs(&l.elts)?),
            ra::Expr::Tuple(t) => ExprKind::Tuple(self.exprs(&t.elts)?),
            ra::Expr::Slice(s) => ExprKind::Slice(
                self.opt(s.lower.as_deref())?,
                self.opt(s.upper.as_deref())?,
                self.opt(s.step.as_deref())?,
            ),
            ra::Expr::IpyEscapeCommand(_) => {
                return Err(unsupported("IPython escape commands", pos));
            }
        };
        self.depth -= 1;
        Ok(Expr {
            kind,
            pos,
            anchor,
            no_carets: false,
        })
    }
}

/// The step a chain continues in below `e`: a step's first operand when
/// that is a step too (`a + b` of `a + b + c`, `x.f` of `x.f()`), or a
/// conditional expression's `else` when that is one.
fn chain_next(e: &ra::Expr) -> Option<&ra::Expr> {
    let step = |x: &ra::Expr| {
        matches!(
            x,
            ra::Expr::BinOp(_)
                | ra::Expr::Attribute(_)
                | ra::Expr::Subscript(_)
                | ra::Expr::Call(_)
        )
    };
    let next = match e {
        ra::Expr::BinOp(b) => &*b.left,
        ra::Expr::Attribute(a) => &*a.value,
        ra::Expr::Subscript(s) => &*s.value,
        ra::Expr::Call(c) => &*c.func,
        ra::Expr::If(i) => return matches!(&*i.orelse, ra::Expr::If(_)).then_some(&*i.orelse),
        _ => return None,
    };
    step(next).then_some(next)
}

/// What a class-body statement uses that micro-monty rejects (`__slots__`,
/// `__del__`, `__getattribute__`), if anything.
fn class_body_unsupported(s: &ra::Stmt) -> Option<&'static str> {
    let is_slots = |t: &ra::Expr| matches!(t, ra::Expr::Name(n) if n.id.as_str() == "__slots__");
    match s {
        ra::Stmt::Assign(a) if a.targets.iter().any(is_slots) => Some("__slots__"),
        ra::Stmt::AnnAssign(a) if is_slots(&a.target) => Some("__slots__"),
        ra::Stmt::FunctionDef(f) if f.name.id.as_str() == "__del__" => Some("__del__"),
        ra::Stmt::FunctionDef(f) if f.name.id.as_str() == "__getattribute__" => {
            Some("__getattribute__")
        }
        _ => None,
    }
}

fn push_lit(out: &mut Vec<FPart>, s: &str) {
    if s.is_empty() {
        return;
    }
    if let Some(FPart::Lit(prev)) = out.last_mut() {
        prev.push_str(s);
    } else {
        out.push(FPart::Lit(s.to_string()));
    }
}
