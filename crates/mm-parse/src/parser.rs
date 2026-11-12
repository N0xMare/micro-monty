//! Recursive descent over the token vector, straight to the engine AST.
//!
//! Node ranges follow ruff 0.0.14 (what the engine was built against):
//! tight first-token to last-token spans, parentheses excluded except for
//! tuples and parenthesized generator expressions, decorated definitions
//! starting at their first decorator. Recursion is depth-counted; the
//! pre-scan has already bounded bracket, indentation and operator nesting.
//!
//! What the old lowering rejected after a successful parse ("micro-monty
//! does not support ...", unknown `__future__` features) is not raised
//! here: the construct becomes a marker node (see [`crate::walk`]) so the
//! error is reported in the lowering's order, after any syntax error.

use alloc::{
    boxed::Box,
    format,
    string::{String, ToString},
    vec,
    vec::Vec,
};
use mm_compile::{
    CompileError, MAX_NESTING,
    ast::{
        Alias, Anchor, BinOp, BoolOp, ClassDef, CmpOp, Comprehension, Const, Expr, ExprKind, FPart,
        FuncDef, Handler, Keyword, MatchCase, Param, Params, Pattern, PatternKind, Pos, Stmt,
        StmtKind, UnaryOp, WithItem,
    },
};

use crate::lexer::{F_BYTES, F_NONASCII, F_RAW, F_T, Late, T, Tok, nfkc_name};
use crate::lines::Lines;
use crate::strings::{self, Bad};

/// Marker tags in `Anchor::kind` (see [`crate::walk`]).
pub(crate) const MARK_ENTER: u8 = 200;
/// A marker reached without entering a node.
pub(crate) const MARK: u8 = 201;
/// A class-body check (runs before the body is lowered).
pub(crate) const MARK_CLASS: u8 = 202;

/// Precedence levels for [`Parser::pratt`].
const P_OR: u8 = 1;
const P_AND: u8 = 2;
const P_NOT: u8 = 3;
const P_CMP: u8 = 4;
const P_BITOR: u8 = 5;

/// Does `k` (followed by `next`) start a comparison operator?
fn cmp_start(k: T, next: T) -> bool {
    matches!(
        k,
        T::EqEqual
            | T::NotEqual
            | T::Less
            | T::LessEqual
            | T::Greater
            | T::GreaterEqual
            | T::In
            | T::Is
    ) || (k == T::Not && next == T::In)
}

type R<T> = Result<T, CompileError>;

const FORGOT_COMMA: &str = "invalid syntax. Perhaps you forgot a comma?";

pub(crate) struct Parser<'s, 'a> {
    src: &'s str,
    b: &'s [u8],
    lines: &'a Lines<'s>,
    toks: Vec<Tok>,
    lex_err: Option<CompileError>,
    /// How a parser error weighs against `lex_err` (see [`Late`]).
    late: Late,
    /// The farthest token looked at (the current token is always looked
    /// at, so the farthest position `bump` reached): CPython's tokenizer
    /// runs on demand, so its error comes first only if the parser fetched
    /// that far.
    seen: usize,
    /// A parser error CPython's error pass also stops at, without reading
    /// on: a comma hint (`invalid_expression` reads the second expression
    /// and one token more, as `forgot_comma` does) unless a plain string
    /// is next to the gap, and a generic error at an operator no
    /// expression starts with (`f(x=^a)`).
    stops: Option<Pos>,
    i: usize,
    prev_end: u32,
    depth: u32,
    /// Errors behind marker nodes.
    pub deferred: Vec<CompileError>,
    /// (line, col) of `If` statements that came from `elif`.
    pub elifs: Vec<(u32, u32)>,
    /// Folded `-literal` constants: the constant's position and those of
    /// the nodes folded into it, innermost first.
    pub folds: Vec<(Pos, Vec<Pos>)>,
    /// Parsing a class body (not a function nested in one).
    in_class: bool,
    /// Class-body annotations stored in the current class so far.
    annotations: u32,
    /// The `__annotations__['x'] = '...'` statement that follows the
    /// annotated assignment just parsed.
    pending: Option<Stmt>,
    /// Building the tree `ast.parse` returns ([`crate::parse_tree`]):
    /// annotations, parameter and alias positions kept; no `-literal`
    /// folding, no class-body `__annotations__` statements, `__future__`
    /// imports and classes micro-monty would reject kept as written.
    pub tree: bool,
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

/// CPython's name for an expression in "cannot assign to ..." messages.
fn expr_name(e: &Expr) -> &'static str {
    match &e.kind {
        ExprKind::Attribute(..) => "attribute",
        ExprKind::Subscript(..) => "subscript",
        ExprKind::Starred(_) => "starred",
        ExprKind::Name(_) => "name",
        ExprKind::List(_) => "list",
        ExprKind::Tuple(_) => "tuple",
        ExprKind::Lambda(_) => "lambda",
        ExprKind::Call(..) => "function call",
        ExprKind::BoolOp(..) | ExprKind::BinOp(..) | ExprKind::Unary(..) => "expression",
        ExprKind::GenExp(..) => "generator expression",
        ExprKind::Yield(_) | ExprKind::YieldFrom(_) => "yield expression",
        ExprKind::Await(_) => "await expression",
        ExprKind::ListComp(..) => "list comprehension",
        ExprKind::SetComp(..) => "set comprehension",
        ExprKind::DictComp(..) => "dict comprehension",
        ExprKind::Dict(_) => "dict literal",
        ExprKind::Set(_) => "set display",
        ExprKind::FString(_) => "f-string expression",
        ExprKind::Const(c) => match c {
            Const::None => "None",
            Const::Bool(true) => "True",
            Const::Bool(false) => "False",
            Const::Ellipsis => "ellipsis",
            Const::Int(n) if *n < 0 => "expression",
            Const::Float(f) if f.is_sign_negative() => "expression",
            _ => "literal",
        },
        ExprKind::Compare(..) => "comparison",
        ExprKind::IfExp(..) => "conditional expression",
        ExprKind::Named(..) => "named expression",
        ExprKind::Slice(..) => "slice",
    }
}

/// Is `e` an expression CPython parses as `bitwise_or` (for the "Maybe you
/// meant '=='" hint)?
fn is_bitwise_or(e: &Expr) -> bool {
    !matches!(
        e.kind,
        ExprKind::BoolOp(..)
            | ExprKind::Compare(..)
            | ExprKind::IfExp(..)
            | ExprKind::Lambda(_)
            | ExprKind::Named(..)
            | ExprKind::Starred(_)
            | ExprKind::Yield(_)
            | ExprKind::YieldFrom(_)
    ) && !matches!(e.kind, ExprKind::Unary(UnaryOp::Not, _))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Target {
    /// Assignment, `for`, `with ... as`, comprehension targets.
    Store,
    Del,
}

fn binop_of(k: T) -> Option<(BinOp, u8)> {
    Some(match k {
        T::Vbar => (BinOp::BitOr, 5),
        T::Circumflex => (BinOp::BitXor, 6),
        T::Amper => (BinOp::BitAnd, 7),
        T::LeftShift => (BinOp::LShift, 8),
        T::RightShift => (BinOp::RShift, 8),
        T::Plus => (BinOp::Add, 9),
        T::Minus => (BinOp::Sub, 9),
        T::Star => (BinOp::Mul, 10),
        T::Slash => (BinOp::Div, 10),
        T::DoubleSlash => (BinOp::FloorDiv, 10),
        T::Percent => (BinOp::Mod, 10),
        T::At => (BinOp::MatMul, 10),
        _ => return None,
    })
}

fn augop_of(k: T) -> Option<BinOp> {
    Some(match k {
        T::PlusEqual => BinOp::Add,
        T::MinusEqual => BinOp::Sub,
        T::StarEqual => BinOp::Mul,
        T::SlashEqual => BinOp::Div,
        T::DoubleSlashEqual => BinOp::FloorDiv,
        T::PercentEqual => BinOp::Mod,
        T::DoubleStarEqual => BinOp::Pow,
        T::LeftShiftEqual => BinOp::LShift,
        T::RightShiftEqual => BinOp::RShift,
        T::AmperEqual => BinOp::BitAnd,
        T::VbarEqual => BinOp::BitOr,
        T::CircumflexEqual => BinOp::BitXor,
        T::AtEqual => BinOp::MatMul,
        _ => return None,
    })
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

impl<'s, 'a> Parser<'s, 'a> {
    pub(crate) fn new(
        src: &'s str,
        lines: &'a Lines<'s>,
        toks: Vec<Tok>,
        lex_err: Option<CompileError>,
        late: Late,
    ) -> Parser<'s, 'a> {
        Parser {
            src,
            b: src.as_bytes(),
            lines,
            toks,
            lex_err,
            late,
            seen: 0,
            stops: None,
            i: 0,
            prev_end: 0,
            depth: 0,
            deferred: Vec::new(),
            elifs: Vec::new(),
            folds: Vec::new(),
            in_class: false,
            annotations: 0,
            pending: None,
            tree: false,
        }
    }

    // ------------------------------------------------------------ tokens

    fn tok(&self) -> Tok {
        self.toks[self.i.min(self.toks.len() - 1)]
    }

    fn cur(&self) -> T {
        self.tok().k
    }

    fn peek(&self, n: usize) -> Tok {
        self.toks[(self.i + n).min(self.toks.len() - 1)]
    }

    #[inline(never)]
    fn bump(&mut self) -> Tok {
        let t = self.tok();
        if !matches!(t.k, T::Newline | T::Dedent | T::Semi | T::End | T::Error) {
            self.prev_end = t.e;
        }
        if self.i < self.toks.len() - 1 {
            self.i += 1;
            self.seen = self.seen.max(self.i);
        }
        t
    }

    #[inline(never)]
    fn eat(&mut self, k: T) -> bool {
        if self.cur() == k {
            self.bump();
            true
        } else {
            false
        }
    }

    fn text(&self, t: Tok) -> &'s str {
        self.src.get(t.s as usize..t.e as usize).unwrap_or("")
    }

    fn is_name(&self, t: Tok, w: &str) -> bool {
        t.k == T::Name && self.text(t) == w
    }

    fn start(&self) -> u32 {
        self.tok().s
    }

    /// From byte `start` to the end of the last consumed token.
    #[inline(never)]
    fn span(&self, start: u32) -> Pos {
        self.lines.pos(start, self.prev_end.max(start))
    }

    /// `p` when building the `ast` tree, else the default (the compiler
    /// needs no such position).
    fn tree_pos(&self, p: Pos) -> Pos {
        if self.tree { p } else { Pos::default() }
    }

    /// A parameter named `name` from byte `start` to the last token,
    /// with its annotation kept only when building the `ast` tree.
    fn param(&self, name: String, ann: Option<Expr>, start: u32, default: Option<Expr>) -> Param {
        Param {
            name,
            default,
            ann: ann.filter(|_| self.tree).map(Box::new),
            pos: self.tree_pos(self.span(start)),
        }
    }

    fn byte_start(&self, p: Pos) -> usize {
        self.lines.offset(p.line, p.col)
    }

    fn byte_end(&self, p: Pos) -> usize {
        self.lines.offset(p.end_line, p.end_col)
    }

    /// One level of nesting: statements, `expression`s and `**` operands.
    /// Never more levels than the engine AST will have (parenthesized groups
    /// give theirs back), so passing the compiler's limit means the old
    /// lowering would have raised `RecursionError` too; this bounds the
    /// parser's recursion (brackets add at most 200 more, see `Lx::open`).
    fn enter(&mut self) -> R<()> {
        self.depth += 1;
        if self.depth > MAX_NESTING {
            return Err(self.too_deep());
        }
        Ok(())
    }

    #[inline(never)]
    fn too_deep(&self) -> CompileError {
        let t = self.tok();
        CompileError::too_deep(self.lines.pos(t.s, t.e))
    }

    fn leave(&mut self) {
        self.depth -= 1;
    }

    // ------------------------------------------------------------ errors

    #[inline(never)]
    fn err_at(&self, msg: impl Into<String>, s: u32, e: u32) -> CompileError {
        let mut pos = self.lines.pos(s, e);
        if pos.end_line == pos.line && pos.end_col <= pos.col {
            pos.end_col = pos.col + 1;
        }
        CompileError::syntax(msg.into(), pos)
    }

    fn err_pos(msg: impl Into<String>, pos: Pos) -> CompileError {
        CompileError::syntax(msg.into(), pos)
    }

    /// "invalid syntax" at the current token (or the tokenizer's error).
    #[inline(never)]
    fn fail(&mut self) -> CompileError {
        let t = self.tok();
        match t.k {
            T::Error => self
                .lex_err
                .take()
                .unwrap_or_else(|| self.err_at("invalid syntax", t.s, t.e)),
            T::Indent => CompileError {
                kind: "IndentationError",
                msg: "unexpected indent".into(),
                pos: {
                    let (line, col) = self.lines.loc(t.e);
                    Pos {
                        line,
                        col: col.saturating_sub(1),
                        end_line: line,
                        end_col: col,
                    }
                },
            },
            T::Newline | T::End if t.s == t.e => {
                let p = self.lines.pos(t.s, t.s);
                CompileError::syntax(
                    "invalid syntax".into(),
                    Pos {
                        end_col: p.col + 1,
                        ..p
                    },
                )
            }
            _ => {
                let e = self.err_at("invalid syntax", t.s, t.e);
                if (T::LPar as u8..=T::AtEqual as u8).contains(&(t.k as u8))
                    && !self.at_expr_start()
                {
                    self.stops = Some(e.pos);
                }
                e
            }
        }
    }

    /// A parser error against the tokenizer's (`pegen_errors.c`). A
    /// tokenizer error the parser fetched came first. Otherwise CPython
    /// tokenizes the rest of the source: an unclosed bracket replaces the
    /// parser error when that is on a later line than the bracket; an error
    /// the tokenizer raises at once replaces it; a deferred one (a stray
    /// `\`) leaves it standing, as in `f(x=a b \c)`'s "Perhaps you forgot
    /// a comma?". mm applies that only to the errors in `stops`.
    pub(crate) fn tokenizer_first(&mut self, e: CompileError) -> CompileError {
        if e.kind == "IndentationError" && e.msg == "unexpected indent" {
            return e;
        }
        let Some(l) = self.lex_err.take() else {
            return e;
        };
        if l.msg.ends_with("was never closed") {
            return if e.pos.end_line > l.pos.line { l } else { e };
        }
        // CPython reads past mm's other errors, so the tokenizer's wins.
        if self.seen + 1 >= self.toks.len() || self.stops != Some(e.pos) {
            return l;
        }
        // The bracket test is against the line of the last token fetched.
        let last = self.toks[self.seen];
        let line = self.lines.pos(last.s, last.s).line;
        match core::mem::replace(&mut self.late, Late::Raised) {
            Late::Raised => l,
            Late::Deferred(Some(b)) if line > b.pos.line => b,
            Late::Deferred(_) => e,
        }
    }

    #[inline(never)]
    fn expect(&mut self, k: T) -> R<Tok> {
        if self.cur() == k {
            Ok(self.bump())
        } else {
            Err(self.fail())
        }
    }

    /// A block's `:`. CPython says "expected ':'" where the grammar forces
    /// the colon (`def`, `for`, `try`, `else`, `finally`) or when the line
    /// ends without one; otherwise "invalid syntax".
    fn expect_colon(&mut self, forced: bool) -> R<()> {
        if self.eat(T::Colon) {
            return Ok(());
        }
        let t = self.tok();
        if t.k == T::Error {
            return Err(self.fail());
        }
        if t.k == T::Newline || forced {
            return Err(self.err_at("expected ':'", t.s, t.e.max(t.s + 1)));
        }
        Err(self.fail())
    }

    fn defer(&mut self, err: CompileError, pos: Pos, tag: u8) -> Expr {
        let idx = u32::try_from(self.deferred.len()).unwrap_or(u32::MAX);
        self.deferred.push(err);
        Expr {
            kind: ExprKind::Const(Const::None),
            pos,
            anchor: Anchor {
                kind: tag,
                left_line: idx,
                left_col: 0,
                right_line: 0,
                right_col: 0,
            },
            no_carets: false,
        }
    }

    fn unsupported(&mut self, what: &str, pos: Pos, tag: u8) -> Expr {
        self.defer(CompileError::unsupported(what, pos), pos, tag)
    }

    // -------------------------------------------------------- statements

    pub(crate) fn module(&mut self) -> R<Vec<Stmt>> {
        let mut body = Vec::new();
        while self.cur() != T::End {
            self.statement(&mut body)?;
        }
        Ok(body)
    }

    /// CPython's `eval` input (`expressions NEWLINE* ENDMARKER`), as one
    /// expression statement.
    pub(crate) fn eval_input(&mut self) -> R<Vec<Stmt>> {
        let start = self.start();
        if self.cur() == T::Indent {
            return Err(self.fail());
        }
        let e = self.star_expressions()?;
        while self.eat(T::Newline) {}
        if self.cur() != T::End {
            return Err(self.fail());
        }
        Ok(alloc::vec![Stmt {
            kind: StmtKind::Expr(e),
            pos: self.span(start),
        }])
    }

    fn statement(&mut self, out: &mut Vec<Stmt>) -> R<()> {
        self.enter()?;
        let t = self.tok();
        match t.k {
            T::If => self.if_stmt(out)?,
            T::While => self.while_stmt(out)?,
            T::For => self.for_stmt(out, t.s, false)?,
            T::Try => self.try_stmt(out)?,
            T::With => self.with_stmt(out, t.s, false)?,
            T::Def => self.def_stmt(out, t.s, Vec::new(), false)?,
            T::Class => self.class_stmt(out, t.s, Vec::new())?,
            T::At => self.decorated(out)?,
            T::Async => {
                self.bump();
                match self.cur() {
                    T::Def => self.def_stmt(out, t.s, Vec::new(), true)?,
                    T::For => self.for_stmt(out, t.s, true)?,
                    T::With => self.with_stmt(out, t.s, true)?,
                    _ => return Err(self.fail()),
                }
            }
            T::Name if self.is_name(t, "match") && self.match_ahead() => self.match_stmt(out)?,
            T::Indent | T::Error => return Err(self.fail()),
            _ => self.simple_stmts(out)?,
        }
        self.leave();
        Ok(())
    }

    /// The body after a compound statement's `:`.
    fn block(&mut self, what: &str, line: u32) -> R<Vec<Stmt>> {
        let mut body = Vec::new();
        if self.eat(T::Newline) {
            if self.cur() != T::Indent {
                let t = self.tok();
                if t.k == T::Error {
                    return Err(self.fail());
                }
                let mut pos = self.lines.pos(t.s, t.e);
                if t.k == T::End {
                    pos = self.lines.pos(self.prev_end, self.prev_end);
                }
                return Err(CompileError {
                    kind: "IndentationError",
                    msg: format!("expected an indented block after {what} on line {line}"),
                    pos,
                });
            }
            self.bump();
            loop {
                self.statement(&mut body)?;
                if self.eat(T::Dedent) || self.cur() == T::End {
                    break;
                }
            }
        } else {
            self.simple_stmts(&mut body)?;
        }
        Ok(body)
    }

    fn line_of(&self, off: u32) -> u32 {
        self.lines.loc(off).0
    }

    fn simple_stmts(&mut self, out: &mut Vec<Stmt>) -> R<()> {
        loop {
            let s = self.simple_stmt()?;
            out.push(s);
            if let Some(a) = self.pending.take() {
                out.push(a);
            }
            if self.eat(T::Semi) {
                if self.eat(T::Newline) {
                    return Ok(());
                }
                continue;
            }
            if self.eat(T::Newline) {
                return Ok(());
            }
            if let Some(Stmt {
                kind: StmtKind::Expr(e),
                pos,
            }) = out.last()
                && let ExprKind::Name(n) = &e.kind
                && (n == "print" || n == "exec")
                && self.at_expr_start()
            {
                let (n, pos) = (n.clone(), *pos);
                let save = self.i;
                let r = self.star_expressions();
                self.i = save;
                if let Ok(arg) = r {
                    return Err(Self::err_pos(
                        format!("Missing parentheses in call to '{n}'. Did you mean {n}(...)?"),
                        Pos {
                            end_line: arg.pos.end_line,
                            end_col: arg.pos.end_col,
                            ..pos
                        },
                    ));
                }
            }
            return Err(self.fail());
        }
    }

    fn names(&mut self) -> R<Vec<String>> {
        let mut names = Vec::new();
        loop {
            names.push(self.name()?);
            if !self.eat(T::Comma) {
                return Ok(names);
            }
        }
    }

    fn name(&mut self) -> R<String> {
        let t = self.tok();
        if t.k != T::Name {
            return Err(self.fail());
        }
        self.bump();
        Ok(self.name_text(t))
    }

    fn name_text(&self, t: Tok) -> String {
        let s = self.text(t);
        if t.f & F_NONASCII != 0 {
            nfkc_name(s)
        } else {
            s.to_string()
        }
    }

    #[allow(clippy::too_many_lines, reason = "one arm per simple statement")]
    fn simple_stmt(&mut self) -> R<Stmt> {
        let t = self.tok();
        let start = t.s;
        let kind = match t.k {
            T::Pass => {
                self.bump();
                StmtKind::Pass
            }
            T::Break => {
                self.bump();
                StmtKind::Break
            }
            T::Continue => {
                self.bump();
                StmtKind::Continue
            }
            T::Return => {
                self.bump();
                if self.at_expr_start() || self.cur() == T::Star {
                    let mut value = self.star_expressions()?;
                    let pos = self.span(start);
                    if let ExprKind::Call(func, ..) = &value.kind
                        && matches!(func.kind, ExprKind::Name(_))
                        && value.pos.line == pos.line
                        && self.starts_line(start as usize)
                    {
                        value.no_carets = true;
                    }
                    StmtKind::Return(Some(value))
                } else {
                    StmtKind::Return(None)
                }
            }
            T::Raise => {
                self.bump();
                if self.at_expr_start() {
                    let exc = self.expression()?;
                    let cause = if self.eat(T::From) {
                        Some(self.expression()?)
                    } else {
                        None
                    };
                    StmtKind::Raise(Some(exc), cause)
                } else {
                    StmtKind::Raise(None, None)
                }
            }
            T::Global | T::Nonlocal => {
                self.bump();
                let names = self.names()?;
                if t.k == T::Global {
                    StmtKind::Global(names)
                } else {
                    StmtKind::Nonlocal(names)
                }
            }
            T::Del => {
                self.bump();
                let mut targets = Vec::new();
                loop {
                    let e = if self.cur() == T::Star {
                        self.starred(false)?
                    } else {
                        self.expression()?
                    };
                    Self::check_target(&e, Target::Del)?;
                    targets.push(e);
                    if !self.eat(T::Comma) || !self.at_expr_start() && self.cur() != T::Star {
                        break;
                    }
                }
                StmtKind::Delete(targets)
            }
            T::Assert => {
                self.bump();
                let test = self.expression()?;
                let msg = if self.eat(T::Comma) {
                    Some(self.expression()?)
                } else {
                    None
                };
                StmtKind::Assert(test, msg)
            }
            T::Import => return self.import_stmt(start, false),
            T::From => return self.import_from_stmt(start, false),
            T::Name if self.is_name(t, "lazy") && matches!(self.peek(1).k, T::Import | T::From) => {
                self.bump();
                return if self.cur() == T::Import {
                    self.import_stmt(start, true)
                } else {
                    self.import_from_stmt(start, true)
                };
            }
            T::Name
                if self.is_name(t, "type")
                    && self.peek(1).k == T::Name
                    && matches!(self.peek(2).k, T::Equal | T::LSqb) =>
            {
                self.bump();
                self.bump();
                if self.cur() == T::LSqb {
                    self.type_params()?;
                }
                self.expect(T::Equal)?;
                self.expression()?;
                let pos = self.span(start);
                let m = self.unsupported("type aliases", pos, MARK);
                return Ok(Stmt {
                    kind: StmtKind::Expr(m),
                    pos,
                });
            }
            _ => return self.expr_stmt(start),
        };
        Ok(Stmt {
            kind,
            pos: self.span(start),
        })
    }

    /// Is the statement at byte `s` the first thing on its line?
    fn starts_line(&self, s: usize) -> bool {
        let mut i = s;
        while i > 0 && !matches!(self.b[i - 1], b'\n' | b'\r') {
            i -= 1;
            if !matches!(self.b[i], b' ' | b'\t' | b'\x0c') {
                return false;
            }
        }
        true
    }

    #[allow(
        clippy::too_many_lines,
        reason = "one grammar rule with its CPython errors"
    )]
    fn expr_stmt(&mut self, start: u32) -> R<Stmt> {
        let first = if self.cur() == T::Yield {
            self.yield_expr()?
        } else {
            self.star_expressions()?
        };
        let k = self.cur();
        if k == T::Equal {
            let mut parts = vec![first];
            while self.eat(T::Equal) {
                let v = if self.cur() == T::Yield {
                    self.yield_expr()?
                } else {
                    self.star_expressions()?
                };
                parts.push(v);
            }
            let Some(mut value) = parts.pop() else {
                return Err(self.fail());
            };
            for (n, t) in parts.iter().enumerate() {
                if matches!(t.kind, ExprKind::Yield(_) | ExprKind::YieldFrom(_)) {
                    return Err(Self::err_pos(
                        "assignment to yield expression not possible",
                        t.pos,
                    ));
                }
                if let Some(bad) = invalid_target(t, Target::Store) {
                    let here = parts.len() == 1
                        && n == 0
                        && !matches!(
                            t.kind,
                            ExprKind::List(_)
                                | ExprKind::Tuple(_)
                                | ExprKind::GenExp(..)
                                | ExprKind::Const(Const::Bool(_) | Const::None)
                        )
                        && is_bitwise_or(t)
                        && is_bitwise_or(&value);
                    let msg = if here {
                        format!(
                            "cannot assign to {} here. Maybe you meant '==' instead of '='?",
                            expr_name(bad)
                        )
                    } else {
                        format!("cannot assign to {}", expr_name(bad))
                    };
                    return Err(Self::err_pos(msg, bad.pos));
                }
            }
            let pos = self.span(start);
            if parts.len() == 1
                && matches!(parts[0].kind, ExprKind::Name(_))
                && matches!(value.kind, ExprKind::Call(..))
                && value.pos.line == pos.line
                && self.starts_line(start as usize)
            {
                value.no_carets = true;
            }
            return Ok(Stmt {
                kind: StmtKind::Assign(parts, value),
                pos,
            });
        }
        if let Some(op) = augop_of(k) {
            if !matches!(
                first.kind,
                ExprKind::Name(_) | ExprKind::Attribute(..) | ExprKind::Subscript(..)
            ) {
                return Err(Self::err_pos(
                    format!(
                        "'{}' is an illegal expression for augmented assignment",
                        expr_name(&first)
                    ),
                    first.pos,
                ));
            }
            self.bump();
            let value = if self.cur() == T::Yield {
                self.yield_expr()?
            } else {
                self.star_expressions()?
            };
            return Ok(Stmt {
                kind: StmtKind::AugAssign(first, op, value),
                pos: self.span(start),
            });
        }
        if k == T::Colon {
            match &first.kind {
                ExprKind::Name(_) | ExprKind::Attribute(..) | ExprKind::Subscript(..) => {}
                ExprKind::Tuple(_) | ExprKind::List(_) => {
                    return Err(Self::err_pos(
                        format!(
                            "only single target (not {}) can be annotated",
                            expr_name(&first)
                        ),
                        first.pos,
                    ));
                }
                _ => return Err(Self::err_pos("illegal target for annotation", first.pos)),
            }
            self.bump();
            let ann = self.expression()?;
            let value = if self.eat(T::Equal) {
                Some(if self.cur() == T::Yield {
                    self.yield_expr()?
                } else {
                    self.star_expressions()?
                })
            } else {
                None
            };
            let pos = self.span(start);
            if self.in_class
                && !self.tree
                && let ExprKind::Name(n) = &first.kind
                && self.b.get(start as usize) != Some(&b'(')
            {
                // `__annotations__['x'] = 'int'`, stringized as under
                // `from __future__ import annotations`.
                let text = crate::unparse::annotation(&ann, self.src);
                self.pending = Some(annotation_store(n, text, pos));
                self.annotations += 1;
            }
            let ann = self.tree.then(|| Box::new(ann));
            return Ok(Stmt {
                kind: StmtKind::AnnAssign(first, value, ann),
                pos,
            });
        }
        Ok(Stmt {
            kind: StmtKind::Expr(first),
            pos: self.span(start),
        })
    }

    fn check_target(e: &Expr, how: Target) -> R<()> {
        if let Some(bad) = invalid_target(e, how) {
            let msg = match how {
                Target::Store => format!("cannot assign to {}", expr_name(bad)),
                Target::Del => format!("cannot delete {}", expr_name(bad)),
            };
            return Err(Self::err_pos(msg, bad.pos));
        }
        Ok(())
    }

    fn dotted(&mut self) -> R<String> {
        let mut name = self.name()?;
        while self.eat(T::Dot) {
            name.push('.');
            name.push_str(&self.name()?);
        }
        Ok(name)
    }

    fn dotted_tail(&mut self) -> R<()> {
        while self.eat(T::Dot) {
            self.name()?;
        }
        Ok(())
    }

    fn import_stmt(&mut self, start: u32, lazy: bool) -> R<Stmt> {
        self.bump();
        if self.cur() != T::Name {
            let t = self.tok();
            if t.k == T::Newline || t.k == T::End {
                let s = self.prev_end;
                return Err(self.err_at("Expected one or more names after 'import'", s, s));
            }
            return Err(self.fail());
        }
        let mut names = Vec::new();
        loop {
            let s = self.start();
            let name = self.dotted()?;
            let asname = if self.eat(T::As) {
                let s = self.start();
                let n = self.name()?;
                if self.cur() == T::Dot {
                    self.dotted_tail()?;
                    return Err(self.err_at(
                        "cannot use attribute as import target",
                        s,
                        self.prev_end,
                    ));
                }
                Some(n)
            } else {
                None
            };
            let pos = self.tree_pos(self.span(s));
            names.push(Alias { name, asname, pos });
            if !self.eat(T::Comma) {
                break;
            }
        }
        let pos = self.span(start);
        let kind = if lazy {
            StmtKind::Expr(self.unsupported("lazy imports", pos, MARK))
        } else {
            StmtKind::Import(names)
        };
        Ok(Stmt { kind, pos })
    }

    fn import_from_stmt(&mut self, start: u32, lazy: bool) -> R<Stmt> {
        self.bump();
        let mut level = 0usize;
        loop {
            match self.cur() {
                T::Dot => level += 1,
                T::Ellipsis => level += 3,
                _ => break,
            }
            self.bump();
        }
        let module = if self.cur() == T::Name {
            Some(self.dotted()?)
        } else {
            if level == 0 {
                return Err(self.fail());
            }
            None
        };
        self.expect(T::Import)?;
        // (alias, its range)
        let mut names: Vec<(Alias, Pos)> = Vec::new();
        if self.cur() == T::Star {
            let t = self.bump();
            let pos = self.lines.pos(t.s, t.e);
            names.push((
                Alias {
                    name: "*".into(),
                    asname: None,
                    pos: self.tree_pos(pos),
                },
                pos,
            ));
        } else {
            let paren = self.eat(T::LPar);
            if !paren && matches!(self.cur(), T::Newline | T::End) {
                let t = self.tok();
                return Err(self.err_at("Expected one or more names after 'import'", t.s, t.s));
            }
            loop {
                let s = self.start();
                let name = self.name()?;
                let asname = if self.eat(T::As) {
                    Some(self.name()?)
                } else {
                    None
                };
                let pos = self.span(s);
                names.push((
                    Alias {
                        name,
                        asname,
                        pos: self.tree_pos(pos),
                    },
                    pos,
                ));
                if !self.eat(T::Comma) {
                    break;
                }
                if paren && self.cur() == T::RPar {
                    break;
                }
                if !paren && self.cur() != T::Name {
                    let t = self.peek(0);
                    if matches!(t.k, T::Newline | T::Semi | T::End) {
                        return Err(self.err_at(
                            "trailing comma not allowed without surrounding parentheses",
                            t.s,
                            t.e,
                        ));
                    }
                }
            }
            if paren {
                self.expect(T::RPar)?;
            }
        }
        let pos = self.span(start);
        let module = module.unwrap_or_default();
        let kind = if lazy {
            StmtKind::Expr(self.unsupported("lazy imports", pos, MARK))
        } else if level == 0 && module == "__future__" && !self.tree {
            let mut kind = StmtKind::Pass;
            for (a, apos) in &names {
                if !FUTURE_FEATURES.contains(&a.name.as_str()) {
                    let err = CompileError::syntax(
                        format!("future feature {} is not defined", a.name),
                        *apos,
                    );
                    kind = StmtKind::Expr(self.defer(err, pos, MARK));
                    break;
                }
            }
            kind
        } else {
            let module = format!("{}{module}", ".".repeat(level));
            StmtKind::ImportFrom(module, names.into_iter().map(|(a, _)| a).collect())
        };
        Ok(Stmt { kind, pos })
    }

    fn if_stmt(&mut self, out: &mut Vec<Stmt>) -> R<()> {
        let start = self.bump().s;
        let line = self.line_of(start);
        let test = self.condition()?;
        self.expect_colon(false)?;
        let body = self.block("'if' statement", line)?;
        // (start of the clause, test, body, clause range end)
        let mut clauses: Vec<(u32, Option<Expr>, Vec<Stmt>, u32)> = Vec::new();
        loop {
            match self.cur() {
                T::Elif => {
                    let s = self.bump().s;
                    let line = self.line_of(s);
                    let t = self.condition()?;
                    self.expect_colon(false)?;
                    let b = self.block("'elif' statement", line)?;
                    clauses.push((s, Some(t), b, self.prev_end));
                }
                T::Else => {
                    let s = self.bump().s;
                    let line = self.line_of(s);
                    self.expect_colon(true)?;
                    let b = self.block("'else' statement", line)?;
                    clauses.push((s, None, b, self.prev_end));
                    break;
                }
                _ => break,
            }
        }
        let pos = self.span(start);
        let mut orelse = Vec::new();
        for (s, test, body, end) in clauses.into_iter().rev() {
            match test {
                None => orelse = body,
                Some(t) => {
                    // In the `ast` tree an `elif` ends where the whole
                    // chain does, as in CPython.
                    let end = if self.tree { self.prev_end } else { end };
                    let cpos = self.lines.pos(s, end);
                    self.elifs.push((cpos.line, cpos.col));
                    orelse = vec![Stmt {
                        kind: StmtKind::If(t, body, orelse),
                        pos: cpos,
                    }];
                }
            }
        }
        out.push(Stmt {
            kind: StmtKind::If(test, body, orelse),
            pos,
        });
        Ok(())
    }

    fn else_block(&mut self) -> R<Vec<Stmt>> {
        if self.cur() == T::Else {
            let s = self.bump().s;
            let line = self.line_of(s);
            self.expect_colon(true)?;
            return self.block("'else' statement", line);
        }
        Ok(Vec::new())
    }

    fn while_stmt(&mut self, out: &mut Vec<Stmt>) -> R<()> {
        let start = self.bump().s;
        let line = self.line_of(start);
        let test = self.condition()?;
        self.expect_colon(false)?;
        let body = self.block("'while' statement", line)?;
        let orelse = self.else_block()?;
        out.push(Stmt {
            kind: StmtKind::While(test, body, orelse),
            pos: self.span(start),
        });
        Ok(())
    }

    /// A comma list of `'*'? bitwise_or` targets (`for` and comprehension
    /// targets, which must not swallow `in`).
    fn target_list(&mut self) -> R<Expr> {
        let s = self.tok();
        let first = self.star_or_bitwise()?;
        if self.cur() != T::Comma {
            Self::check_target(&first, Target::Store)?;
            return Ok(first);
        }
        let mut elts = vec![first];
        while self.eat(T::Comma) {
            if self.cur() == T::In || !(self.at_expr_start() || self.cur() == T::Star) {
                break;
            }
            elts.push(self.star_or_bitwise()?);
        }
        let e = Expr::new(ExprKind::Tuple(elts), self.span(s.s));
        Self::check_target(&e, Target::Store)?;
        Ok(e)
    }

    fn star_or_bitwise(&mut self) -> R<Expr> {
        if self.cur() == T::Star {
            let s = self.bump().s;
            let v = self.binary()?;
            return Ok(Expr::new(ExprKind::Starred(Box::new(v)), self.span(s)));
        }
        self.binary()
    }

    fn for_stmt(&mut self, out: &mut Vec<Stmt>, start: u32, is_async: bool) -> R<()> {
        let kw = self.bump().s;
        let line = self.line_of(kw);
        let target = self.target_list()?;
        self.expect(T::In)?;
        let iter = self.star_expressions()?;
        self.expect_colon(true)?;
        let body = self.block("'for' statement", line)?;
        let orelse = self.else_block()?;
        let pos = self.span(start);
        let kind = StmtKind::For(target, iter, body, orelse, is_async);
        out.push(Stmt { kind, pos });
        Ok(())
    }

    #[allow(clippy::too_many_lines, reason = "try, handlers, else and finally")]
    fn try_stmt(&mut self, out: &mut Vec<Stmt>) -> R<()> {
        let start = self.bump().s;
        let line = self.line_of(start);
        self.expect_colon(true)?;
        let body = self.block("'try' statement", line)?;
        let mut handlers = Vec::new();
        let mut star: Option<bool> = None;
        while self.cur() == T::Except {
            let hs = self.bump().s;
            let hline = self.line_of(hs);
            let is_star = self.eat(T::Star);
            if let Some(prev) = star
                && prev != is_star
            {
                return Err(self.err_at(
                    "cannot have both 'except' and 'except*' on the same 'try'",
                    hs,
                    hs + 6,
                ));
            }
            star = Some(is_star);
            let mut typ = None;
            let mut name = None;
            if self.cur() != T::Colon {
                let ts = self.tok();
                let first = self.expression()?;
                if self.cur() == T::Comma {
                    let mut elts = vec![first];
                    while self.eat(T::Comma) {
                        if !self.at_expr_start() {
                            break;
                        }
                        elts.push(self.expression()?);
                    }
                    let e = Expr::new(ExprKind::Tuple(elts), self.span(ts.s));
                    if self.cur() == T::As {
                        return Err(Self::err_pos(
                            "multiple exception types must be parenthesized",
                            e.pos,
                        ));
                    }
                    typ = Some(e);
                } else {
                    typ = Some(first);
                }
                if self.eat(T::As) {
                    name = Some(self.name()?);
                }
            } else if is_star {
                let t = self.tok();
                return Err(self.err_at("expected one or more exception types", t.s, t.e));
            }
            self.expect_colon(false)?;
            let what = if is_star {
                "'except*' statement"
            } else {
                "'except' statement"
            };
            let hbody = self.block(what, hline)?;
            let mut hpos = self.span(hs);
            if let Some(t) = &typ {
                hpos.end_line = t.pos.end_line;
                hpos.end_col = t.pos.end_col;
            } else {
                hpos.end_line = hpos.line;
                hpos.end_col = hpos.col + 6;
            }
            handlers.push(Handler {
                typ,
                name,
                body: hbody,
                pos: hpos,
            });
        }
        let orelse = if handlers.is_empty() {
            Vec::new()
        } else {
            self.else_block()?
        };
        let mut finalbody = Vec::new();
        let has_finally = self.cur() == T::Finally;
        if has_finally {
            let s = self.bump().s;
            let fline = self.line_of(s);
            self.expect_colon(true)?;
            finalbody = self.block("'finally' statement", fline)?;
        }
        if handlers.is_empty() && !has_finally {
            let t = self.tok();
            if t.k == T::Error {
                return Err(self.fail());
            }
            let (s, e) = if t.k == T::End {
                (self.prev_end, self.prev_end)
            } else {
                (t.s, t.e)
            };
            return Err(self.err_at("expected 'except' or 'finally' block", s, e));
        }
        let pos = self.span(start);
        let kind = StmtKind::Try {
            body,
            handlers,
            orelse,
            finalbody,
            star: star == Some(true),
        };
        out.push(Stmt { kind, pos });
        Ok(())
    }

    fn with_item(&mut self) -> R<WithItem> {
        let ctx = self.expression()?;
        let target = if self.eat(T::As) {
            let t = self.star_target()?;
            Some(t)
        } else {
            None
        };
        Ok(WithItem { ctx, target })
    }

    /// One `with ... as` / `except ... as` style star target.
    fn star_target(&mut self) -> R<Expr> {
        let e = self.star_or_bitwise()?;
        Self::check_target(&e, Target::Store)?;
        Ok(e)
    }

    fn with_stmt(&mut self, out: &mut Vec<Stmt>, start: u32, is_async: bool) -> R<()> {
        let kw = self.bump().s;
        let line = self.line_of(kw);
        let mut items = Vec::new();
        let mut done = false;
        if self.cur() == T::LPar && self.paren_then_colon() {
            // `with (a as b, c):` -- or a parenthesized expression.
            let save = (
                self.i,
                self.prev_end,
                self.deferred.len(),
                self.elifs.len(),
                self.folds.len(),
                self.depth,
            );
            self.bump();
            let attempt = (|| -> R<Vec<WithItem>> {
                let mut items = Vec::new();
                loop {
                    items.push(self.with_item()?);
                    if !self.eat(T::Comma) || self.cur() == T::RPar {
                        break;
                    }
                }
                self.expect(T::RPar)?;
                if self.cur() != T::Colon {
                    return Err(self.fail());
                }
                Ok(items)
            })();
            if let Ok(v) = attempt {
                items = v;
                done = true;
            } else {
                self.i = save.0;
                self.prev_end = save.1;
                self.deferred.truncate(save.2);
                self.elifs.truncate(save.3);
                self.folds.truncate(save.4);
                self.depth = save.5;
            }
        }
        if !done {
            loop {
                items.push(self.with_item()?);
                if !self.eat(T::Comma) {
                    break;
                }
            }
        }
        self.expect_colon(false)?;
        let body = self.block("'with' statement", line)?;
        let pos = self.span(start);
        let kind = StmtKind::With(items, body, is_async);
        out.push(Stmt { kind, pos });
        Ok(())
    }

    /// At `(`: is the matching `)` followed by `:`?
    fn paren_then_colon(&self) -> bool {
        let mut depth = 0usize;
        let mut j = self.i;
        while j < self.toks.len() {
            match self.toks[j].k {
                T::LPar | T::LSqb | T::LBrace => depth += 1,
                T::RPar | T::RSqb | T::RBrace => {
                    depth -= 1;
                    if depth == 0 {
                        return self.toks.get(j + 1).is_some_and(|t| t.k == T::Colon);
                    }
                }
                T::Newline | T::End | T::Error => return false,
                _ => {}
            }
            j += 1;
        }
        false
    }

    fn decorated(&mut self, out: &mut Vec<Stmt>) -> R<()> {
        let start = self.start();
        let mut decorators = Vec::new();
        while self.eat(T::At) {
            decorators.push(self.named_expression()?);
            self.expect(T::Newline)?;
        }
        match self.cur() {
            T::Def => self.def_stmt(out, start, decorators, false),
            T::Class => self.class_stmt(out, start, decorators),
            T::Async if self.peek(1).k == T::Def => {
                self.bump();
                self.def_stmt(out, start, decorators, true)
            }
            _ => Err(self.fail()),
        }
    }

    fn def_stmt(
        &mut self,
        out: &mut Vec<Stmt>,
        start: u32,
        decorators: Vec<Expr>,
        is_async: bool,
    ) -> R<()> {
        // `async def` starts at `async` (the token before).
        let head = if is_async {
            self.toks[self.i.saturating_sub(1)].s
        } else {
            self.start()
        };
        let kw = self.bump().s;
        let line = self.line_of(kw);
        let name = self.name()?;
        let generic = self.cur() == T::LSqb;
        if generic {
            self.type_params()?;
        }
        self.expect(T::LPar)?;
        let params = self.params(T::RPar, true)?;
        self.expect(T::RPar)?;
        let mut returns = None;
        if self.eat(T::Rarrow) {
            let r = self.expression()?;
            returns = self.tree.then(|| Box::new(r));
        }
        self.expect_colon(true)?;
        let in_class = core::mem::replace(&mut self.in_class, false);
        let body = self.block("function definition", line);
        self.in_class = in_class;
        let body = body?;
        let pos = self.span(start);
        if generic {
            let m = self.unsupported("type parameters", pos, MARK);
            out.push(Stmt {
                kind: StmtKind::Expr(m),
                pos,
            });
            return Ok(());
        }
        let mut fpos = pos;
        fpos.end_line = fpos.line;
        fpos.end_col = fpos.col;
        if self.tree {
            // CPython's FunctionDef starts at `def` (or `async`).
            fpos = self.span(head);
        }
        out.push(Stmt {
            kind: StmtKind::FunctionDef(Box::new(FuncDef {
                name,
                params,
                body,
                decorators,
                pos: fpos,
                is_async,
                returns,
            })),
            pos,
        });
        Ok(())
    }

    /// `[T, *Ts, **P, U: bound = default]` (parsed, then rejected).
    fn type_params(&mut self) -> R<()> {
        self.expect(T::LSqb)?;
        loop {
            if self.eat(T::Star) || self.eat(T::DoubleStar) {
                self.name()?;
            } else {
                self.name()?;
                if self.eat(T::Colon) {
                    self.expression()?;
                }
            }
            if self.eat(T::Equal) {
                if self.cur() == T::Star {
                    self.starred(false)?;
                } else {
                    self.expression()?;
                }
            }
            if !self.eat(T::Comma) || self.cur() == T::RSqb {
                break;
            }
        }
        self.expect(T::RSqb)?;
        Ok(())
    }

    /// A parameter list up to `close` (`)` for `def`, `:` for `lambda`).
    #[allow(clippy::too_many_lines, reason = "CPython's parameter rules")]
    fn params(&mut self, close: T, annotations: bool) -> R<Params> {
        let mut p = Params::default();
        let mut seen_default = false;
        let mut star = false;
        let mut slash = false;
        let mut bare_star: Option<Tok> = None;
        // `*args` and `**kwargs` as parameters, for the `ast` tree.
        let mut stars: [Option<Param>; 2] = [None, None];
        while self.cur() != close {
            let t = self.tok();
            if p.kwarg.is_some() {
                return Err(self.err_at("arguments cannot follow var-keyword argument", t.s, t.e));
            }
            match t.k {
                T::Slash => {
                    if slash {
                        return Err(self.err_at("/ may appear only once", t.s, t.e));
                    }
                    if star {
                        return Err(self.err_at("/ must be ahead of *", t.s, t.e));
                    }
                    if p.args.is_empty() {
                        return Err(self.err_at("at least one argument must precede /", t.s, t.e));
                    }
                    self.bump();
                    slash = true;
                    p.posonly = core::mem::take(&mut p.args);
                }
                T::Star => {
                    if star {
                        return Err(self.err_at("* argument may appear only once", t.s, t.e));
                    }
                    self.bump();
                    star = true;
                    if self.cur() == T::Name {
                        let ns = self.start();
                        let name = self.name()?;
                        let mut ann = None;
                        if annotations && self.eat(T::Colon) {
                            ann = Some(if self.cur() == T::Star {
                                self.starred(false)?
                            } else {
                                self.expression()?
                            });
                        }
                        if self.tree {
                            stars[0] = Some(self.param(name.clone(), ann, ns, None));
                        }
                        p.vararg = Some(name);
                        if self.cur() == T::Equal {
                            let e = self.tok();
                            return Err(self.err_at(
                                "var-positional argument cannot have default value",
                                e.s,
                                e.e,
                            ));
                        }
                    } else {
                        bare_star = Some(t);
                    }
                }
                T::DoubleStar => {
                    self.bump();
                    let ns = self.start();
                    let name = self.name()?;
                    let mut ann = None;
                    if annotations && self.eat(T::Colon) {
                        ann = Some(self.expression()?);
                    }
                    if self.tree {
                        stars[1] = Some(self.param(name.clone(), ann, ns, None));
                    }
                    p.kwarg = Some(name);
                    if self.cur() == T::Equal {
                        let e = self.tok();
                        return Err(self.err_at(
                            "var-keyword argument cannot have default value",
                            e.s,
                            e.e,
                        ));
                    }
                }
                T::Name => {
                    let ns = self.start();
                    let name = self.name()?;
                    let mut ann = None;
                    if annotations && self.eat(T::Colon) {
                        ann = Some(self.expression()?);
                    }
                    // The parameter ends before its default.
                    let ppos = self.tree_pos(self.span(ns));
                    let ann = ann.filter(|_| self.tree).map(Box::new);
                    let default = if self.eat(T::Equal) {
                        Some(self.expression()?)
                    } else {
                        None
                    };
                    if star {
                        bare_star = None;
                        p.kwonly.push(Param {
                            name,
                            default,
                            ann,
                            pos: ppos,
                        });
                    } else {
                        if default.is_some() {
                            seen_default = true;
                        } else if seen_default {
                            return Err(self.err_at(
                                "parameter without a default follows parameter with a default",
                                t.s,
                                t.e,
                            ));
                        }
                        p.args.push(Param {
                            name,
                            default,
                            ann,
                            pos: ppos,
                        });
                    }
                }
                _ => return Err(self.fail()),
            }
            if !self.eat(T::Comma) {
                break;
            }
        }
        if let Some(t) = bare_star {
            return Err(self.err_at("named arguments must follow bare *", t.s, t.e));
        }
        if stars.iter().any(Option::is_some) {
            p.star = Some(Box::new(stars));
        }
        Ok(p)
    }

    fn class_stmt(&mut self, out: &mut Vec<Stmt>, start: u32, decorators: Vec<Expr>) -> R<()> {
        let kw = self.bump().s;
        let line = self.line_of(kw);
        let name_tok = self.tok();
        let name = self.name()?;
        let generic = self.cur() == T::LSqb;
        if generic {
            self.type_params()?;
        }
        let mut bases = Vec::new();
        let mut keywords = Vec::new();
        if self.eat(T::LPar) {
            let (a, k) = self.arguments(false)?;
            bases = a;
            keywords = k;
            self.expect(T::RPar)?;
        }
        self.expect_colon(false)?;
        let in_class = core::mem::replace(&mut self.in_class, true);
        let outer = core::mem::replace(&mut self.annotations, 0);
        let body = self.block("class definition", line);
        self.in_class = in_class;
        let annotated = core::mem::replace(&mut self.annotations, outer) != 0;
        let mut body = body?;
        let pos = self.span(start);
        if annotated {
            annotations_dict(&mut body, pos);
        }
        if generic {
            let m = self.unsupported("type parameters", pos, MARK);
            out.push(Stmt {
                kind: StmtKind::Expr(m),
                pos,
            });
            return Ok(());
        }
        if !self.tree
            && let Some((what, spos)) = body.iter().find_map(class_body_unsupported)
        {
            let m = self.unsupported(what, spos, MARK_CLASS);
            body.insert(
                0,
                Stmt {
                    kind: StmtKind::Expr(m),
                    pos: spos,
                },
            );
        }
        // The statement without its decorators: from the `class` keyword.
        let head = self.b.get(..name_tok.s as usize).unwrap_or_default();
        let kwpos = head.trim_ascii_end().len().saturating_sub(5);
        let span = self
            .lines
            .pos(u32::try_from(kwpos).unwrap_or(0), self.prev_end);
        let mut cpos = span;
        cpos.end_line = cpos.line;
        cpos.end_col = cpos.col;
        out.push(Stmt {
            kind: StmtKind::ClassDef(Box::new(ClassDef {
                name,
                bases,
                keywords,
                body,
                decorators,
                pos: cpos,
                span,
            })),
            pos,
        });
        Ok(())
    }

    // ------------------------------------------------------------- match

    /// At a `match` name: does a match statement follow (`match <subject>:`
    /// then a newline)?
    fn match_ahead(&self) -> bool {
        let next = self.peek(1).k;
        if matches!(
            next,
            T::Equal
                | T::Colon
                | T::Dot
                | T::Comma
                | T::RPar
                | T::Newline
                | T::End
                | T::Semi
                | T::In
                | T::Is
                | T::And
                | T::Or
                | T::If
                | T::Else
                | T::For
                | T::Slash
                | T::Percent
                | T::DoubleSlash
                | T::Vbar
                | T::Amper
                | T::Circumflex
                | T::Less
                | T::Greater
                | T::EqEqual
                | T::NotEqual
                | T::LessEqual
                | T::GreaterEqual
                | T::LeftShift
                | T::RightShift
                | T::DoubleStar
                | T::At
        ) || augop_of(next).is_some()
        {
            return false;
        }
        // The logical line must end with `:`.
        let mut depth = 0usize;
        let mut j = self.i + 1;
        let mut last = T::Name;
        while j < self.toks.len() {
            let k = self.toks[j].k;
            match k {
                T::LPar | T::LSqb | T::LBrace => depth += 1,
                T::RPar | T::RSqb | T::RBrace => depth = depth.saturating_sub(1),
                T::Newline | T::End | T::Error if depth == 0 => return last == T::Colon,
                T::End | T::Error => return false,
                _ => {}
            }
            last = k;
            j += 1;
        }
        false
    }

    fn match_stmt(&mut self, out: &mut Vec<Stmt>) -> R<()> {
        let start = self.bump().s;
        let line = self.line_of(start);
        // Subject: star_named_expression ',' ... | named_expression.
        let ss = self.start();
        let first = if self.cur() == T::Star {
            self.starred(false)?
        } else {
            self.named_expression()?
        };
        let subject = if self.cur() == T::Comma {
            let mut elts = vec![first];
            while self.eat(T::Comma) {
                if self.cur() == T::Colon {
                    break;
                }
                elts.push(self.star_named_expression()?);
            }
            Expr::new(ExprKind::Tuple(elts), self.span(ss))
        } else {
            first
        };
        self.expect_colon(false)?;
        if !self.eat(T::Newline) {
            return Err(self.fail());
        }
        if self.cur() != T::Indent {
            let t = self.tok();
            if t.k == T::Error {
                return Err(self.fail());
            }
            return Err(CompileError {
                kind: "IndentationError",
                msg: format!("expected an indented block after 'match' statement on line {line}"),
                pos: self.lines.pos(t.s, t.e),
            });
        }
        self.bump();
        let mut cases = Vec::new();
        loop {
            let t = self.tok();
            if !self.is_name(t, "case") {
                return Err(self.fail());
            }
            self.bump();
            let cline = self.line_of(t.s);
            let pattern = self.patterns()?;
            let guard = if self.eat(T::If) {
                Some(self.named_expression()?)
            } else {
                None
            };
            self.expect_colon(false)?;
            let body = self.block("'case' statement", cline)?;
            cases.push(MatchCase {
                pattern,
                guard,
                body,
            });
            if self.eat(T::Dedent) || self.cur() == T::End {
                break;
            }
        }
        let pos = self.span(start);
        out.push(Stmt {
            kind: StmtKind::Match(subject, cases),
            pos,
        });
        Ok(())
    }

    /// `patterns`: an open sequence or one pattern.
    fn patterns(&mut self) -> R<Pattern> {
        let s = self.start();
        let first = self.maybe_star_pattern()?;
        if self.cur() != T::Comma {
            if matches!(first.kind, PatternKind::Star(_)) {
                return Err(self.fail());
            }
            return Ok(first);
        }
        let mut items = vec![first];
        while self.eat(T::Comma) {
            if matches!(self.cur(), T::Colon | T::If) {
                break;
            }
            items.push(self.maybe_star_pattern()?);
        }
        Ok(Pattern {
            kind: PatternKind::Sequence(items),
            pos: self.span(s),
        })
    }

    fn maybe_star_pattern(&mut self) -> R<Pattern> {
        if self.cur() == T::Star {
            let s = self.bump().s;
            let name = self.name()?;
            let name = (name != "_").then_some(name);
            return Ok(Pattern {
                kind: PatternKind::Star(name),
                pos: self.span(s),
            });
        }
        self.pattern()
    }

    /// `or_pattern ['as' NAME]`. (Nesting needs brackets, which the
    /// pre-scan bounds; the final walk counts the depth.)
    fn pattern(&mut self) -> R<Pattern> {
        let s = self.start();
        let mut alts = vec![self.closed_pattern()?];
        while self.eat(T::Vbar) {
            alts.push(self.closed_pattern()?);
        }
        let mut p = if alts.len() == 1 {
            alts.pop().ok_or_else(|| self.fail())?
        } else {
            Pattern {
                kind: PatternKind::Or(alts),
                pos: self.span(s),
            }
        };
        if self.eat(T::As) {
            let t = self.tok();
            let name = self.name()?;
            if name == "_" {
                return Err(self.err_at("cannot use '_' as a target", t.s, t.e));
            }
            p = Pattern {
                kind: PatternKind::As(Some(Box::new(p)), Some(name)),
                pos: self.span(s),
            };
        }
        Ok(p)
    }

    /// A literal pattern's value: `[-]number [(+|-) imaginary]`, built the
    /// way an expression would be (folding a negated literal).
    fn number_pattern(&mut self) -> R<Expr> {
        let s = self.start();
        let neg = self.cur() == T::Minus;
        let at = usize::from(neg);
        if !matches!(self.peek(at).k, T::Int | T::Float | T::Complex)
            || matches!(
                self.peek(at + 1).k,
                T::LPar | T::LSqb | T::Dot | T::DoubleStar
            )
        {
            return Err(self.fail());
        }
        let left = if neg { self.unary()? } else { self.atom()? };
        let op = match self.cur() {
            T::Plus => BinOp::Add,
            T::Minus => BinOp::Sub,
            _ => return Ok(left),
        };
        self.bump();
        if self.cur() != T::Complex {
            return Err(self.fail());
        }
        let right = self.atom()?;
        Ok(self.binop(s, left, op, right))
    }

    /// A name or dotted name.
    fn name_or_attr(&mut self) -> R<Expr> {
        let t = self.tok();
        let name = self.name()?;
        let mut e = Expr::new(ExprKind::Name(name), self.lines.pos(t.s, t.e));
        while self.eat(T::Dot) {
            let attr = self.name()?;
            e = Expr::new(ExprKind::Attribute(Box::new(e), attr), self.span(t.s));
        }
        Ok(e)
    }

    #[allow(clippy::too_many_lines, reason = "one arm per pattern kind")]
    fn closed_pattern(&mut self) -> R<Pattern> {
        let t = self.tok();
        let kind = match t.k {
            T::Minus | T::Int | T::Float | T::Complex => PatternKind::Value(self.number_pattern()?),
            T::Str | T::FStart => PatternKind::Value(self.strings()?),
            T::None | T::True | T::False => {
                self.bump();
                PatternKind::Singleton(match t.k {
                    T::None => Const::None,
                    k => Const::Bool(k == T::True),
                })
            }
            T::Name => {
                let e = self.name_or_attr()?;
                if self.eat(T::LPar) {
                    let mut patterns = Vec::new();
                    let mut kwd_attrs = Vec::new();
                    let mut kwd_patterns = Vec::new();
                    while self.cur() != T::RPar {
                        if self.cur() == T::Name && self.peek(1).k == T::Equal {
                            kwd_attrs.push(self.name()?);
                            self.bump();
                            kwd_patterns.push(self.pattern()?);
                        } else {
                            let p = self.pattern()?;
                            if !kwd_attrs.is_empty() {
                                return Err(Self::err_pos(
                                    "positional patterns follow keyword patterns",
                                    p.pos,
                                ));
                            }
                            patterns.push(p);
                        }
                        if !self.eat(T::Comma) {
                            break;
                        }
                    }
                    self.expect(T::RPar)?;
                    PatternKind::Class {
                        cls: e,
                        patterns,
                        kwd_attrs,
                        kwd_patterns,
                    }
                } else {
                    match &e.kind {
                        ExprKind::Name(n) if n == "_" => PatternKind::As(None, None),
                        ExprKind::Name(n) => PatternKind::As(None, Some(n.clone())),
                        _ => PatternKind::Value(e),
                    }
                }
            }
            T::LPar | T::LSqb => {
                let close = if t.k == T::LPar { T::RPar } else { T::RSqb };
                self.bump();
                let mut items = Vec::new();
                let mut comma = false;
                while self.cur() != close {
                    items.push(self.maybe_star_pattern()?);
                    if !self.eat(T::Comma) {
                        break;
                    }
                    comma = true;
                }
                self.expect(close)?;
                if close == T::RPar && items.len() == 1 && !comma {
                    // A group pattern: the pattern itself.
                    let p = items.pop().ok_or_else(|| self.fail())?;
                    if matches!(p.kind, PatternKind::Star(_)) {
                        return Err(self.fail());
                    }
                    return Ok(p);
                }
                PatternKind::Sequence(items)
            }
            T::LBrace => {
                self.bump();
                let mut keys = Vec::new();
                let mut patterns = Vec::new();
                let mut rest = None;
                while self.cur() != T::RBrace {
                    if rest.is_some() {
                        return Err(self.fail());
                    }
                    if self.eat(T::DoubleStar) {
                        rest = Some(self.name()?);
                    } else {
                        // Key: a literal or a dotted value.
                        let k = self.tok();
                        let key = match k.k {
                            T::Minus | T::Int | T::Float | T::Complex => self.number_pattern()?,
                            T::Str | T::FStart => self.strings()?,
                            T::None | T::True | T::False => {
                                self.bump();
                                let c = match k.k {
                                    T::None => Const::None,
                                    kk => Const::Bool(kk == T::True),
                                };
                                Expr::new(ExprKind::Const(c), self.lines.pos(k.s, k.e))
                            }
                            T::Name => {
                                let e = self.name_or_attr()?;
                                if !matches!(e.kind, ExprKind::Attribute(..)) {
                                    return Err(self.fail());
                                }
                                e
                            }
                            _ => return Err(self.fail()),
                        };
                        self.expect(T::Colon)?;
                        keys.push(key);
                        patterns.push(self.pattern()?);
                    }
                    if !self.eat(T::Comma) {
                        break;
                    }
                }
                self.expect(T::RBrace)?;
                PatternKind::Mapping {
                    keys,
                    patterns,
                    rest,
                }
            }
            _ => return Err(self.fail()),
        };
        Ok(Pattern {
            kind,
            pos: self.span(t.s),
        })
    }

    // -------------------------------------------------------- expressions

    /// Can the current token start an expression?
    fn at_expr_start(&self) -> bool {
        matches!(
            self.cur(),
            T::Name
                | T::Int
                | T::Float
                | T::Complex
                | T::Str
                | T::FStart
                | T::LPar
                | T::LSqb
                | T::LBrace
                | T::Minus
                | T::Plus
                | T::Tilde
                | T::Not
                | T::Await
                | T::Lambda
                | T::None
                | T::True
                | T::False
                | T::Ellipsis
                | T::Yield
        )
    }

    /// `if`/`elif`/`while` conditions: a named expression, with CPython's
    /// hint for `=`.
    #[inline(never)]
    fn condition(&mut self) -> R<Expr> {
        let e = self.named_expression()?;
        if self.cur() == T::Equal {
            self.bump();
            let v = self.binary()?;
            let pos = Pos {
                end_line: v.pos.end_line,
                end_col: v.pos.end_col,
                ..e.pos
            };
            if matches!(e.kind, ExprKind::Name(_)) {
                return Err(Self::err_pos(
                    "invalid syntax. Maybe you meant '==' or ':=' instead of '='?",
                    pos,
                ));
            }
            if let Some(bad) = invalid_target(&e, Target::Store) {
                return Err(Self::err_pos(
                    format!(
                        "cannot assign to {} here. Maybe you meant '==' instead of '='?",
                        expr_name(bad)
                    ),
                    bad.pos,
                ));
            }
            return Err(Self::err_pos(
                "invalid syntax. Maybe you meant '==' or ':=' instead of '='?",
                pos,
            ));
        }
        Ok(e)
    }

    /// `star_expressions`: one or a tuple of (starred) expressions.
    fn star_expressions(&mut self) -> R<Expr> {
        let s = self.tok();
        let first = self.star_expression()?;
        if self.cur() != T::Comma {
            return Ok(first);
        }
        let mut elts = vec![first];
        while self.eat(T::Comma) {
            if !(self.at_expr_start() || self.cur() == T::Star) || self.cur() == T::Yield {
                break;
            }
            elts.push(self.star_expression()?);
        }
        Ok(Expr::new(ExprKind::Tuple(elts), self.span(s.s)))
    }

    fn star_expression(&mut self) -> R<Expr> {
        if self.cur() == T::Star {
            return self.starred(true);
        }
        self.expression()
    }

    /// `'*' bitwise_or` (`expression` after the star when `wide`).
    fn starred(&mut self, wide: bool) -> R<Expr> {
        let s = self.bump().s;
        if !self.at_expr_start() || matches!(self.cur(), T::Yield | T::Lambda) {
            let t = self.tok();
            if t.k != T::Error {
                return Err(self.err_at("Invalid star expression", t.s, t.e));
            }
        }
        let v = if wide {
            self.binary()?
        } else {
            self.expression()?
        };
        Ok(Expr::new(ExprKind::Starred(Box::new(v)), self.span(s)))
    }

    fn star_named_expression(&mut self) -> R<Expr> {
        if self.cur() == T::Star {
            return self.starred(true);
        }
        self.named_expression()
    }

    fn named_expression(&mut self) -> R<Expr> {
        let s = self.start();
        let e = self.expression()?;
        if self.cur() == T::ColonEqual {
            return self.walrus(s, &e);
        }
        Ok(e)
    }

    #[inline(never)]
    fn walrus(&mut self, s: u32, e: &Expr) -> R<Expr> {
        let ExprKind::Name(name) = &e.kind else {
            return Err(Self::err_pos(
                format!("cannot use assignment expressions with {}", expr_name(e)),
                e.pos,
            ));
        };
        let name = name.clone();
        self.bump();
        let v = self.expression()?;
        Ok(Expr::new(ExprKind::Named(name, Box::new(v)), self.span(s)))
    }

    /// `expression`: a lambda, a conditional expression or a disjunction.
    /// Counts one level of nesting (see [`Parser::enter`]).
    fn expression(&mut self) -> R<Expr> {
        self.enter()?;
        let r = if self.cur() == T::Lambda {
            self.lambda()
        } else {
            let s = self.start();
            match self.pratt(P_OR) {
                Ok(body) if self.cur() == T::If => self.if_exp(s, body),
                r => r,
            }
        };
        self.leave();
        r
    }

    /// `body if test else orelse`. A chain (`a if x else b if y else c`)
    /// is parsed in a loop, not by recursion: each conditional in an
    /// `else` adds no nesting level (see [`crate::walk`]).
    #[inline(never)]
    fn if_exp(&mut self, s: u32, body: Expr) -> R<Expr> {
        // (start, body, test) of each conditional, outermost first.
        let mut chain: Vec<(u32, Expr, Expr)> = Vec::new();
        let (mut s, mut body) = (s, body);
        let orelse = loop {
            self.bump();
            let test = self.pratt(P_OR)?;
            if !self.eat(T::Else) {
                if self.cur() == T::Error {
                    return Err(self.fail());
                }
                let pos = Pos {
                    end_line: test.pos.end_line,
                    end_col: test.pos.end_col,
                    ..body.pos
                };
                return Err(Self::err_pos("expected 'else' after 'if' expression", pos));
            }
            if !self.at_expr_start() || self.cur() == T::Yield {
                if matches!(
                    self.cur(),
                    T::Pass
                        | T::Break
                        | T::Continue
                        | T::Return
                        | T::Raise
                        | T::Del
                        | T::Import
                        | T::Newline
                        | T::End
                ) {
                    let t = self.tok();
                    return Err(self.err_at(
                        "expected expression after 'else', but statement is given",
                        t.s,
                        t.e,
                    ));
                }
                return Err(self.fail());
            }
            chain.push((s, body, test));
            if self.cur() == T::Lambda {
                break self.expression()?;
            }
            let next = self.start();
            self.enter()?;
            let e = self.pratt(P_OR);
            self.leave();
            let e = e?;
            if self.cur() == T::If {
                (s, body) = (next, e);
                continue;
            }
            break e;
        };
        // Every conditional of the chain ends where its last `else` does.
        let mut acc = orelse;
        while let Some((s, body, test)) = chain.pop() {
            let pos = self.span(s);
            acc = Expr::new(
                ExprKind::IfExp(Box::new(test), Box::new(body), Box::new(acc)),
                pos,
            );
        }
        Ok(acc)
    }

    #[inline(never)]
    fn lambda(&mut self) -> R<Expr> {
        let s = self.bump().s;
        let params = self.params(T::Colon, false)?;
        self.expect(T::Colon)?;
        if self.cur() == T::Yield {
            return Err(self.fail());
        }
        let body = self.expression()?;
        let pos = self.span(s);
        let bpos = body.pos;
        Ok(Expr::new(
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
            })),
            pos,
        ))
    }

    /// `disjunction` (CPython's `or_test`).
    fn disjunction(&mut self) -> R<Expr> {
        self.pratt(P_OR)
    }

    /// `bitwise_or`: binary operators from `|` up.
    fn binary(&mut self) -> R<Expr> {
        self.pratt(P_BITOR)
    }

    /// Operator precedence climbing from `or` (1) to the multiplicative
    /// operators (10): `or` and `and` collect their operands into one
    /// `BoolOp`, comparisons chain into one `Compare`, `not` is a prefix at
    /// level 3, binary operators associate left. Every node spans from the
    /// first token of its leftmost operand.
    fn pratt(&mut self, min: u8) -> R<Expr> {
        let s = self.start();
        let mut left = if min <= P_NOT && self.cur() == T::Not {
            self.not_expr()?
        } else {
            self.factor()?
        };
        loop {
            let k = self.cur();
            if let Some((op, prec)) = binop_of(k) {
                if prec < min {
                    break;
                }
                self.bump();
                let right = self.pratt(prec + 1)?;
                left = self.binop(s, left, op, right);
            } else if min <= P_CMP && cmp_start(k, self.peek(1).k) {
                left = self.compare(s, left)?;
            } else if min <= P_AND && k == T::And {
                left = self.boolop(s, left, T::And, P_NOT)?;
            } else if min <= P_OR && k == T::Or {
                left = self.boolop(s, left, T::Or, P_AND)?;
            } else {
                break;
            }
        }
        Ok(left)
    }

    #[inline(never)]
    fn boolop(&mut self, s: u32, first: Expr, k: T, next: u8) -> R<Expr> {
        let mut values = vec![first];
        while self.eat(k) {
            values.push(self.pratt(next)?);
        }
        let op = if k == T::And { BoolOp::And } else { BoolOp::Or };
        Ok(Expr::new(ExprKind::BoolOp(op, values), self.span(s)))
    }

    #[inline(never)]
    fn compare(&mut self, s: u32, first: Expr) -> R<Expr> {
        let mut rest = Vec::new();
        loop {
            let op = match self.cur() {
                T::EqEqual => CmpOp::Eq,
                T::NotEqual => CmpOp::Ne,
                T::Less => CmpOp::Lt,
                T::LessEqual => CmpOp::Le,
                T::Greater => CmpOp::Gt,
                T::GreaterEqual => CmpOp::Ge,
                T::In => CmpOp::In,
                T::Not if self.peek(1).k == T::In => {
                    self.bump();
                    CmpOp::NotIn
                }
                T::Is => {
                    if self.peek(1).k == T::Not {
                        self.bump();
                        CmpOp::IsNot
                    } else {
                        CmpOp::Is
                    }
                }
                _ => break,
            };
            self.bump();
            rest.push((op, self.pratt(P_BITOR)?));
        }
        Ok(Expr::new(
            ExprKind::Compare(Box::new(first), rest),
            self.span(s),
        ))
    }

    /// `not` prefixes (iterative), then a comparison.
    #[inline(never)]
    fn not_expr(&mut self) -> R<Expr> {
        let mut nots = Vec::new();
        while self.cur() == T::Not {
            nots.push(self.bump().s);
        }
        let mut e = self.pratt(P_CMP)?;
        while let Some(s) = nots.pop() {
            e = Expr::new(ExprKind::Unary(UnaryOp::Not, Box::new(e)), self.span(s));
        }
        Ok(e)
    }

    #[inline(never)]
    fn binop(&self, start: u32, left: Expr, op: BinOp, right: Expr) -> Expr {
        let anchor = self.binop_anchor(self.byte_end(left.pos), self.byte_start(right.pos));
        let pos = self.span(start);
        Expr {
            kind: ExprKind::BinOp(Box::new(left), op, Box::new(right)),
            pos,
            anchor,
            no_carets: false,
        }
    }

    /// `factor`: unary `+`, `-`, `~`, then `power`.
    fn factor(&mut self) -> R<Expr> {
        if matches!(self.cur(), T::Minus | T::Plus | T::Tilde) {
            self.unary()
        } else {
            self.power()
        }
    }

    /// Unary prefixes (iterative). `-` on a numeric constant folds, as the
    /// old lowering did; each folded level is recorded in `folds` because
    /// the lowering entered it (it counts against the nesting limit).
    #[inline(never)]
    fn unary(&mut self) -> R<Expr> {
        let mut ops: Vec<(UnaryOp, u32)> = Vec::new();
        loop {
            let op = match self.cur() {
                T::Minus => UnaryOp::Neg,
                T::Plus => UnaryOp::Pos,
                T::Tilde => UnaryOp::Invert,
                _ => break,
            };
            ops.push((op, self.bump().s));
        }
        let mut e = self.power()?;
        // Positions of the nodes folded into `e`, innermost first.
        let mut chain: Vec<Pos> = Vec::new();
        while let Some((op, s)) = ops.pop() {
            let pos = self.span(s);
            let folded = match (op, &e.kind) {
                _ if self.tree => None,
                (UnaryOp::Neg, ExprKind::Const(Const::Int(n))) if *n != i64::MIN => {
                    Some(Const::Int(-n))
                }
                (UnaryOp::Neg, ExprKind::Const(Const::Float(f))) => Some(Const::Float(-f)),
                _ => None,
            };
            e = if let Some(c) = folded {
                if chain.is_empty()
                    && let Some(i) = self.folds.iter().rposition(|f| f.0 == e.pos)
                {
                    chain = self.folds.swap_remove(i).1;
                }
                chain.push(e.pos);
                Expr::new(ExprKind::Const(c), pos)
            } else {
                if !chain.is_empty() {
                    self.folds.push((e.pos, core::mem::take(&mut chain)));
                }
                Expr::new(ExprKind::Unary(op, Box::new(e)), pos)
            };
        }
        if !chain.is_empty() {
            self.folds.push((e.pos, chain));
        }
        Ok(e)
    }

    fn power(&mut self) -> R<Expr> {
        let start = self.start();
        let base = if self.cur() == T::Await {
            let s = self.bump().s;
            let v = self.primary()?;
            Expr::new(ExprKind::Await(Box::new(v)), self.span(s))
        } else {
            self.primary()?
        };
        if self.cur() != T::DoubleStar {
            return Ok(base);
        }
        self.bump();
        self.enter()?;
        let exp = self.factor();
        self.leave();
        Ok(self.binop(start, base, BinOp::Pow, exp?))
    }

    fn primary(&mut self) -> R<Expr> {
        let s = self.start();
        let mut e = self.atom()?;
        loop {
            match self.cur() {
                T::Dot => {
                    self.bump();
                    let t = self.tok();
                    if t.k != T::Name {
                        return Err(self.fail());
                    }
                    self.bump();
                    let name = self.name_text(t);
                    let pos = self.span(s);
                    e = Expr::new(ExprKind::Attribute(Box::new(e), name), pos);
                }
                T::LPar => {
                    let fend = self.byte_end(e.pos);
                    self.bump();
                    let (args, kws) = self.arguments(true)?;
                    self.expect(T::RPar)?;
                    let pos = self.span(s);
                    let anchor = self.bracket_anchor(fend, b'(', self.prev_end as usize);
                    e = Expr {
                        kind: ExprKind::Call(Box::new(e), args, kws),
                        pos,
                        anchor,
                        no_carets: false,
                    };
                }
                T::LSqb => {
                    let vend = self.byte_end(e.pos);
                    self.bump();
                    let index = self.slices()?;
                    self.expect(T::RSqb)?;
                    let pos = self.span(s);
                    let anchor = self.bracket_anchor(vend, b'[', self.prev_end as usize);
                    e = Expr {
                        kind: ExprKind::Subscript(Box::new(e), Box::new(index)),
                        pos,
                        anchor,
                        no_carets: false,
                    };
                }
                _ => return Ok(e),
            }
        }
    }

    /// Call (or class) arguments up to `)`.
    #[allow(
        clippy::too_many_lines,
        reason = "one grammar rule with its CPython errors"
    )]
    #[inline(never)]
    fn arguments(&mut self, call: bool) -> R<(Vec<Expr>, Vec<Keyword>)> {
        let mut args = Vec::new();
        let mut kws: Vec<Keyword> = Vec::new();
        let mut seen_kw = false;
        let mut seen_kw_unpack = false;
        while self.cur() != T::RPar {
            let t = self.tok();
            // Where a "forgot a comma" range starts: the value's start.
            let mut from = t.s;
            if t.k == T::DoubleStar {
                self.bump();
                from = self.start();
                let v = self.expression()?;
                kws.push(Keyword {
                    name: None,
                    pos: self.span(t.s),
                    value: v,
                });
                seen_kw_unpack = true;
            } else if t.k == T::Name && self.peek(1).k == T::Equal {
                self.bump();
                self.bump();
                if !self.at_expr_start() || self.cur() == T::Yield {
                    if matches!(self.cur(), T::RPar | T::Comma) {
                        return Err(self.err_at(
                            "expected argument value expression",
                            t.s,
                            self.prev_end,
                        ));
                    }
                    return Err(self.fail());
                }
                from = self.start();
                let v = self.expression()?;
                kws.push(Keyword {
                    name: Some(self.name_text(t)),
                    pos: self.span(t.s),
                    value: v,
                });
                seen_kw = true;
            } else {
                let starred = t.k == T::Star;
                if starred {
                    from = self.peek(1).s;
                }
                let e = if starred {
                    self.starred(false)?
                } else {
                    self.named_expression()?
                };
                if matches!(self.cur(), T::For | T::Async) && !starred {
                    if !call {
                        return Err(self.fail());
                    }
                    let g = self.genexp(e, t.s, false)?;
                    let sole = args.is_empty() && kws.is_empty() && self.cur() == T::RPar;
                    if !sole {
                        return Err(Self::err_pos(
                            "Generator expression must be parenthesized",
                            g.pos,
                        ));
                    }
                    args.push(g);
                } else if self.cur() == T::Equal {
                    return Err(Self::err_pos(
                        "expression cannot contain assignment, perhaps you meant \"==\"?",
                        Pos {
                            end_line: e.pos.end_line,
                            end_col: e.pos.end_col + 1,
                            ..e.pos
                        },
                    ));
                } else {
                    let after = self.tok();
                    if starred && seen_kw_unpack {
                        let comma = self.toks[self.i.saturating_sub(1)];
                        let comma = self.toks[..self.i]
                            .iter()
                            .rev()
                            .find(|c| c.k == T::Comma && c.e <= t.s)
                            .copied()
                            .unwrap_or(comma);
                        return Err(self.err_at(
                            "iterable argument unpacking follows keyword argument unpacking",
                            comma.s,
                            after.s,
                        ));
                    }
                    if !starred && seen_kw_unpack {
                        return Err(self.err_at(
                            "positional argument follows keyword argument unpacking",
                            after.s,
                            after.e,
                        ));
                    }
                    if !starred && seen_kw {
                        return Err(self.err_at(
                            "positional argument follows keyword argument",
                            after.s,
                            after.e,
                        ));
                    }
                    args.push(e);
                }
            }
            if !self.eat(T::Comma) {
                if self.at_expr_start() && self.cur() != T::Yield {
                    return Err(self.forgot_comma(from));
                }
                break;
            }
        }
        Ok((args, kws))
    }

    /// Two expressions side by side inside brackets: CPython's range runs
    /// from the start of the first (byte `prev`) to the end of the second.
    #[inline(never)]
    fn forgot_comma(&mut self, prev: u32) -> CompileError {
        let save = (self.i, self.prev_end);
        let next = self.disjunction();
        let (end, end_i) = (self.prev_end, self.i);
        (self.i, self.prev_end) = save;
        match next {
            Ok(_) => {
                let e = self.err_at(FORGOT_COMMA, prev, end);
                // CPython's string rules read on past `'s' x` and `x 's'`,
                // and its `expression_without_invalid` on past `b if`.
                let next = self.toks[end_i].k;
                if self.cur() != T::Str
                    && self.toks[self.i.saturating_sub(1)].k != T::Str
                    && next != T::If
                {
                    self.stops = Some(e.pos);
                }
                e
            }
            Err(e) => e,
        }
    }

    /// The index of a subscript: slices, expressions, starred items.
    #[inline(never)]
    fn slices(&mut self) -> R<Expr> {
        let s = self.tok();
        let first = self.slice()?;
        if self.cur() != T::Comma {
            if matches!(first.kind, ExprKind::Starred(_)) {
                return Ok(Expr::new(ExprKind::Tuple(vec![first]), self.span(s.s)));
            }
            return Ok(first);
        }
        let mut elts = vec![first];
        while self.eat(T::Comma) {
            if self.cur() == T::RSqb {
                break;
            }
            elts.push(self.slice()?);
        }
        Ok(Expr::new(ExprKind::Tuple(elts), self.span(s.s)))
    }

    fn slice(&mut self) -> R<Expr> {
        let s = self.start();
        if self.cur() == T::Star {
            return self.starred(true);
        }
        let lower = if self.cur() == T::Colon {
            None
        } else {
            let e = self.named_expression()?;
            if self.cur() != T::Colon {
                return Ok(e);
            }
            Some(Box::new(e))
        };
        self.expect(T::Colon)?;
        let upper = if matches!(self.cur(), T::Colon | T::Comma | T::RSqb) {
            None
        } else {
            Some(Box::new(self.expression()?))
        };
        let step = if self.eat(T::Colon) {
            if matches!(self.cur(), T::Comma | T::RSqb) {
                None
            } else {
                Some(Box::new(self.expression()?))
            }
        } else {
            None
        };
        Ok(Expr::new(ExprKind::Slice(lower, upper, step), self.span(s)))
    }

    #[allow(clippy::too_many_lines, reason = "one arm per atom")]
    fn atom(&mut self) -> R<Expr> {
        let t = self.tok();
        let c = |k: Const, p: &Self| Expr::new(ExprKind::Const(k), p.lines.pos(t.s, t.e));
        match t.k {
            T::Name => {
                self.bump();
                Ok(Expr::new(
                    ExprKind::Name(self.name_text(t)),
                    self.lines.pos(t.s, t.e),
                ))
            }
            T::None => {
                self.bump();
                Ok(c(Const::None, self))
            }
            T::True => {
                self.bump();
                Ok(c(Const::Bool(true), self))
            }
            T::False => {
                self.bump();
                Ok(c(Const::Bool(false), self))
            }
            T::Ellipsis => {
                self.bump();
                Ok(c(Const::Ellipsis, self))
            }
            T::Int => {
                self.bump();
                Ok(c(int_const(self.text(t)), self))
            }
            T::Float | T::Complex => {
                self.bump();
                let text: String = self
                    .text(t)
                    .chars()
                    .filter(|&ch| !matches!(ch, '_' | 'j' | 'J'))
                    .collect();
                let v = text.parse::<f64>().unwrap_or(f64::INFINITY);
                Ok(c(
                    if t.k == T::Float {
                        Const::Float(v)
                    } else {
                        Const::Complex(v)
                    },
                    self,
                ))
            }
            T::Str | T::FStart => self.strings(),
            T::LPar => self.paren(),
            T::LSqb => self.list(),
            T::LBrace => self.brace(),
            _ => Err(self.fail()),
        }
    }

    #[inline(never)]
    fn list(&mut self) -> R<Expr> {
        let t = self.tok();
        self.bump();
        if self.eat(T::RSqb) {
            return Ok(Expr::new(ExprKind::List(Vec::new()), self.span(t.s)));
        }
        let fs = self.start();
        let first = self.star_named_expression()?;
        if matches!(self.cur(), T::For | T::Async) {
            let gens = self.comprehensions()?;
            self.expect(T::RSqb)?;
            return Ok(Expr::new(
                ExprKind::ListComp(Box::new(first), gens),
                self.span(t.s),
            ));
        }
        let elts = self.elements(first, fs, T::RSqb)?;
        Ok(Expr::new(ExprKind::List(elts), self.span(t.s)))
    }

    /// The rest of a display after its first element, through `close`.
    #[inline(never)]
    fn elements(&mut self, first: Expr, first_start: u32, close: T) -> R<Vec<Expr>> {
        let mut elts = vec![first];
        let mut prev = first_start;
        loop {
            if self.eat(close) {
                return Ok(elts);
            }
            if !self.eat(T::Comma) {
                if self.at_expr_start() && self.cur() != T::Yield {
                    return Err(self.forgot_comma(prev));
                }
                return Err(self.fail());
            }
            if self.eat(close) {
                return Ok(elts);
            }
            prev = self.start();
            elts.push(self.star_named_expression()?);
        }
    }

    #[inline(never)]
    /// A parenthesized group adds no AST level: it gives its nesting level
    /// back while its contents are parsed.
    fn paren(&mut self) -> R<Expr> {
        let d = self.depth;
        self.depth = d.saturating_sub(1);
        let r = self.paren_inner();
        self.depth = d;
        r
    }

    fn paren_inner(&mut self) -> R<Expr> {
        let open = self.bump();
        if self.eat(T::RPar) {
            return Ok(Expr::new(ExprKind::Tuple(Vec::new()), self.span(open.s)));
        }
        if self.cur() == T::Yield {
            let y = self.yield_expr()?;
            self.expect(T::RPar)?;
            return Ok(y);
        }
        let fs = self.start();
        let first = self.star_named_expression()?;
        if matches!(self.cur(), T::For | T::Async) {
            let g = self.genexp(first, open.s, true)?;
            return Ok(g);
        }
        if self.eat(T::RPar) {
            if matches!(first.kind, ExprKind::Starred(_)) {
                return Err(Self::err_pos(
                    "cannot use starred expression here",
                    first.pos,
                ));
            }
            return Ok(first);
        }
        let elts = self.elements(first, fs, T::RPar)?;
        Ok(Expr::new(ExprKind::Tuple(elts), self.span(open.s)))
    }

    /// A generator expression after its element; `open` is the `(` of a
    /// parenthesized one (its range then includes the parentheses).
    #[inline(never)]
    fn genexp(&mut self, elt: Expr, start: u32, paren: bool) -> R<Expr> {
        let gens = self.comprehensions()?;
        if paren {
            self.expect(T::RPar)?;
        }
        let pos = self.span(start);
        Ok(Expr::new(ExprKind::GenExp(Box::new(elt), gens), pos))
    }

    #[inline(never)]
    fn comprehensions(&mut self) -> R<Vec<Comprehension>> {
        let mut gens = Vec::new();
        while matches!(self.cur(), T::For | T::Async) {
            let is_async = self.eat(T::Async);
            self.expect(T::For)?;
            let target = self.target_list()?;
            self.expect(T::In)?;
            let iter = self.disjunction()?;
            let mut ifs = Vec::new();
            while self.eat(T::If) {
                ifs.push(self.disjunction()?);
            }
            gens.push(Comprehension {
                target,
                iter,
                ifs,
                is_async,
            });
        }
        Ok(gens)
    }

    fn brace(&mut self) -> R<Expr> {
        let open = self.bump();
        self.brace_inner(open)
    }

    #[inline(never)]
    fn brace_inner(&mut self, open: Tok) -> R<Expr> {
        if self.eat(T::RBrace) {
            return Ok(Expr::new(ExprKind::Dict(Vec::new()), self.span(open.s)));
        }
        // A dict starting with `**m`.
        if self.cur() == T::DoubleStar {
            self.bump();
            let v = self.binary()?;
            if matches!(self.cur(), T::For | T::Async) {
                self.comprehensions()?;
                self.expect(T::RBrace)?;
                let pos = self.span(open.s);
                return Ok(self.unsupported("dict comprehension unpacking", pos, MARK_ENTER));
            }
            return self.dict_rest(open, vec![(None, v)]);
        }
        let fs = self.start();
        // A starred or unparenthesized `:=` element makes this a set.
        let (first, set_only) = if self.cur() == T::Star {
            (self.starred(true)?, true)
        } else {
            let e = self.expression()?;
            if self.cur() == T::ColonEqual {
                (self.walrus(fs, &e)?, true)
            } else {
                (e, false)
            }
        };
        if self.cur() == T::Colon && set_only {
            return Err(self.fail());
        }
        if self.cur() == T::Colon {
            let colon = self.bump();
            if matches!(self.cur(), T::RBrace | T::Comma) {
                return Err(self.err_at(
                    "expression expected after dictionary key and ':'",
                    colon.s,
                    colon.e,
                ));
            }
            let v = self.expression()?;
            if matches!(self.cur(), T::For | T::Async) {
                let gens = self.comprehensions()?;
                self.expect(T::RBrace)?;
                return Ok(Expr::new(
                    ExprKind::DictComp(Box::new(first), Box::new(v), gens),
                    self.span(open.s),
                ));
            }
            return self.dict_rest(open, vec![(Some(first), v)]);
        }
        if matches!(self.cur(), T::For | T::Async) {
            let gens = self.comprehensions()?;
            self.expect(T::RBrace)?;
            return Ok(Expr::new(
                ExprKind::SetComp(Box::new(first), gens),
                self.span(open.s),
            ));
        }
        let elts = self.elements(first, fs, T::RBrace)?;
        Ok(Expr::new(ExprKind::Set(elts), self.span(open.s)))
    }

    #[inline(never)]
    fn dict_rest(&mut self, open: Tok, mut items: Vec<(Option<Expr>, Expr)>) -> R<Expr> {
        loop {
            if self.eat(T::RBrace) {
                break;
            }
            if !self.eat(T::Comma) {
                return Err(self.fail());
            }
            if self.eat(T::RBrace) {
                break;
            }
            if self.eat(T::DoubleStar) {
                let v = self.binary()?;
                items.push((None, v));
                continue;
            }
            let k = self.expression()?;
            if self.cur() == T::Colon && matches!(self.peek(1).k, T::RBrace | T::Comma) {
                let colon = self.tok();
                return Err(self.err_at(
                    "expression expected after dictionary key and ':'",
                    colon.s,
                    colon.e,
                ));
            }
            if !self.eat(T::Colon) {
                if matches!(self.cur(), T::Comma | T::RBrace) {
                    let t = self.tok();
                    return Err(self.err_at(
                        "':' expected after dictionary key",
                        u32::try_from(self.byte_start(k.pos)).unwrap_or(t.s),
                        t.s,
                    ));
                }
                return Err(self.fail());
            }
            let v = self.expression()?;
            items.push((Some(k), v));
        }
        Ok(Expr::new(ExprKind::Dict(items), self.span(open.s)))
    }

    #[inline(never)]
    fn yield_expr(&mut self) -> R<Expr> {
        let s = self.bump().s;
        if self.eat(T::From) {
            let v = self.expression()?;
            return Ok(Expr::new(ExprKind::YieldFrom(Box::new(v)), self.span(s)));
        }
        if self.at_expr_start() && self.cur() != T::Yield || self.cur() == T::Star {
            let v = self.star_expressions()?;
            return Ok(Expr::new(ExprKind::Yield(Some(Box::new(v))), self.span(s)));
        }
        Ok(Expr::new(ExprKind::Yield(None), self.span(s)))
    }

    // ------------------------------------------------------------ strings

    /// Adjacent string literals: a `str`, `bytes` or f-string constant.
    #[allow(clippy::too_many_lines, reason = "the concatenation rules")]
    #[inline(never)]
    fn strings(&mut self) -> R<Expr> {
        let start = self.start();
        let mut text = String::new();
        let mut data: Vec<u8> = Vec::new();
        let mut parts: Vec<FPart> = Vec::new();
        let (mut bytes, mut plain, mut f, mut t) = (false, false, false, false);
        let mut named_escape = false;
        let mut mixed = false;
        while matches!(self.cur(), T::Str | T::FStart) {
            let tok = self.tok();
            if tok.k == T::Str {
                self.bump();
                let is_bytes = tok.f & F_BYTES != 0;
                if is_bytes {
                    bytes = true;
                } else {
                    plain = true;
                }
                if t && (bytes || plain) {
                    return Err(self.mix_error(start, true));
                }
                if bytes && (plain || f) {
                    mixed = true;
                }
                let raw = tok.f & F_RAW != 0;
                let q = if tok.f & crate::lexer::F_TRIPLE != 0 {
                    3
                } else {
                    1
                };
                let body_start = tok.s as usize + prefix_len(self.text(tok)) + q;
                let body = self.src.get(body_start..tok.e as usize - q).unwrap_or("");
                let r = if is_bytes {
                    strings::decode(body, true, raw, false, &mut data)
                } else if f || t {
                    let mut s = String::new();
                    let r = strings::decode(body, false, raw, false, &mut s);
                    push_lit(&mut parts, &s);
                    r
                } else {
                    strings::decode(body, false, raw, false, &mut text)
                };
                if let Err(bad) = r {
                    match bad {
                        Bad::NamedEscape => named_escape = true,
                        _ => return Err(self.bad_string(bad, tok)),
                    }
                }
            } else {
                if tok.f & F_T != 0 {
                    t = true;
                } else {
                    f = true;
                }
                if t && (bytes || plain || f) {
                    return Err(self.mix_error(start, true));
                }
                if bytes {
                    mixed = true;
                }
                let s = core::mem::take(&mut text);
                push_lit(&mut parts, &s);
                if self.fstring(&mut parts)? {
                    named_escape = true;
                }
            }
        }
        let pos = self.span(start);
        if mixed {
            return Err(self.mix_error(self.prev_end, false));
        }
        if named_escape {
            return Ok(self.unsupported("named Unicode escapes (\\N{...})", pos, MARK_ENTER));
        }
        if t {
            return Ok(self.unsupported("t-strings", pos, MARK_ENTER));
        }
        if bytes {
            return Ok(Expr::new(ExprKind::Const(Const::Bytes(data)), pos));
        }
        if f {
            if !text.is_empty() {
                push_lit(&mut parts, &text);
            }
            if parts.is_empty() {
                parts.push(FPart::Lit(String::new()));
            }
            return Ok(Expr::new(ExprKind::FString(parts), pos));
        }
        Ok(Expr::new(ExprKind::Const(Const::Str(text)), pos))
    }

    #[inline(never)]
    fn mix_error(&self, start: u32, t: bool) -> CompileError {
        let msg = if t {
            "cannot mix t-string literals with string or bytes literals"
        } else {
            "cannot mix bytes and nonbytes literals"
        };
        self.err_at(msg, start, start)
    }

    #[inline(never)]
    fn bad_string(&self, bad: Bad, tok: Tok) -> CompileError {
        let msg = match bad {
            Bad::Escape(m) => m,
            Bad::NonAsciiBytes => "bytes can only contain ASCII literal characters".into(),
            Bad::NamedEscape => String::new(),
        };
        self.err_at(msg, tok.s, tok.e)
    }

    /// One f-string (or t-string) from its start token; literal text and
    /// fields go to `out`. Returns whether a `\N{...}` escape was seen.
    #[inline(never)]
    fn fstring(&mut self, out: &mut Vec<FPart>) -> R<bool> {
        let open = self.bump();
        let raw = open.f & F_RAW != 0;
        let kind = if open.f & F_T != 0 {
            "t-string"
        } else {
            "f-string"
        };
        let mut named = false;
        loop {
            let t = self.tok();
            match t.k {
                T::FMiddle => {
                    self.bump();
                    named |= self.fmiddle(t, raw, out)?;
                }
                T::LBrace => named |= self.field(out, raw, kind, 0)?,
                T::FEnd => {
                    self.bump();
                    break;
                }
                _ => return Err(self.fail()),
            }
        }
        Ok(named)
    }

    #[inline(never)]
    fn fmiddle(&self, t: Tok, raw: bool, out: &mut Vec<FPart>) -> R<bool> {
        let mut s = String::new();
        match strings::decode(self.text(t), false, raw, true, &mut s) {
            Ok(()) => {
                push_lit(out, &s);
                Ok(false)
            }
            Err(Bad::NamedEscape) => Ok(true),
            Err(bad) => Err(self.bad_string(bad, t)),
        }
    }

    /// A replacement field `{expr[=][!c][:spec]}`.
    #[allow(
        clippy::too_many_lines,
        reason = "one grammar rule with its CPython errors"
    )]
    #[inline(never)]
    fn field(&mut self, out: &mut Vec<FPart>, raw: bool, kind: &str, nest: u32) -> R<bool> {
        let lb = self.bump();
        let mut named = false;
        if self.cur() == T::RBrace {
            let t = self.tok();
            return Err(self.err_at(
                format!("{kind}: valid expression required before '}}'"),
                t.s,
                t.e,
            ));
        }
        if self.cur() == T::Lambda {
            let s = self.start();
            let save = self.i;
            // Only to find where the parameters end.
            let _ = self.params(T::Colon, false);
            let e = self.tok().s;
            self.i = save;
            return Err(self.err_at(
                format!("{kind}: lambda expressions are not allowed without parentheses"),
                s,
                e,
            ));
        }
        let expr = if self.cur() == T::Yield {
            self.yield_expr()?
        } else {
            self.star_expressions()?
        };
        let mut conv = 0u8;
        let mut debug = false;
        if self.cur() == T::Equal {
            self.bump();
            let upto = self.tok().s as usize;
            // Raw: only the line breaks translate, as CPython's do.
            let mut text = String::new();
            let _ = strings::decode(
                self.src.get(lb.e as usize..upto).unwrap_or(""),
                false,
                true,
                false,
                &mut text,
            );
            push_lit(out, &text);
            debug = true;
        }
        if self.cur() == T::Exclamation {
            let ex = self.bump();
            let t = self.tok();
            if t.k == T::Name && t.s == ex.e {
                self.bump();
                conv = match self.text(t) {
                    "s" => b's',
                    "r" => b'r',
                    "a" => b'a',
                    other => {
                        return Err(self.err_at(
                            format!(
                                "{kind}: invalid conversion character '{other}': expected 's', 'r', or 'a'"
                            ),
                            t.s,
                            t.e,
                        ));
                    }
                };
            } else if matches!(t.k, T::RBrace | T::Colon) {
                return Err(self.err_at(format!("{kind}: missing conversion character"), t.s, t.e));
            } else {
                return Err(self.err_at(
                    format!("{kind}: conversion type must come right after the exclamation mark"),
                    ex.s,
                    t.e,
                ));
            }
        }
        let spec = if self.eat(T::Colon) {
            let mut parts = Vec::new();
            loop {
                let t = self.tok();
                match t.k {
                    T::FMiddle => {
                        self.bump();
                        named |= self.fmiddle(t, raw, &mut parts)?;
                    }
                    T::LBrace => {
                        if nest >= 2 {
                            let p = self.toks[self.i - 1].s;
                            return Err(self.err_at(
                                format!("{kind}: expressions nested too deeply"),
                                p,
                                p,
                            ));
                        }
                        named |= self.field(&mut parts, raw, kind, nest + 1)?;
                    }
                    _ => break,
                }
            }
            Some(parts)
        } else {
            None
        };
        if self.cur() != T::RBrace {
            let t = self.tok();
            if t.k == T::Error {
                return Err(self.fail());
            }
            let msg = if conv != 0 && spec.is_none() {
                format!("{kind}: expecting ':' or '}}'")
            } else if debug || conv != 0 || spec.is_some() {
                format!("{kind}: expecting '}}'")
            } else {
                format!("{kind}: expecting '=', or '!', or ':', or '}}'")
            };
            return Err(self.err_at(msg, t.s, t.e));
        }
        let rb = self.bump();
        if debug && conv == 0 && spec.is_none() {
            conv = b'r';
        }
        out.push(FPart::Field {
            expr: Box::new(expr),
            conv,
            spec,
            pos: self.lines.pos(lb.s, rb.e),
        });
        Ok(named)
    }

    // ------------------------------------------------------------ anchors

    /// Moves past the end of a line, a comment or a line continuation.
    fn next_line(&self, mut i: usize) -> usize {
        while i < self.b.len() && !matches!(self.b[i], b'\n' | b'\r') {
            i += 1;
        }
        i + 1 + usize::from(self.b.get(i..i + 2) == Some(b"\r\n"))
    }

    /// CPython's `increment_until`: the first offset from `i` whose byte
    /// satisfies `stop`, skipping comments and continuations.
    fn scan_until(&self, mut i: usize, stop: impl Fn(u8) -> bool) -> Option<usize> {
        while i < self.b.len() {
            let ch = self.b[i];
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

    #[inline(never)]
    fn binop_anchor(&self, left_end: usize, right_start: usize) -> Anchor {
        let Some(op) = self.scan_until(left_end, |c| !c.is_ascii_whitespace() && c != b')') else {
            return Anchor::default();
        };
        let mut end = op + 1;
        if let Some(&ch) = self.b.get(end)
            && end < right_start
            && ch != b'\n'
            && ch != b'\r'
            && !ch.is_ascii_whitespace()
            && ch != b'\\'
            && ch != b'#'
        {
            end += 1;
        }
        let (l1, c1) = self.lines.loc(u32::try_from(op).unwrap_or(u32::MAX));
        let (l2, c2) = self.lines.loc(u32::try_from(end).unwrap_or(u32::MAX));
        Anchor {
            kind: 1,
            left_line: l1,
            left_col: c1,
            right_line: l2,
            right_col: c2,
        }
    }

    #[inline(never)]
    fn bracket_anchor(&self, value_end: usize, open: u8, expr_end: usize) -> Anchor {
        let Some(at) = self.scan_until(value_end, |c| c == open) else {
            return Anchor::default();
        };
        let (l1, c1) = self.lines.loc(u32::try_from(at).unwrap_or(u32::MAX));
        let (l2, c2) = self.lines.loc(u32::try_from(expr_end).unwrap_or(u32::MAX));
        Anchor {
            kind: 2,
            left_line: l1,
            left_col: c1,
            right_line: l2,
            right_col: c2,
        }
    }
}

/// The prefix length of a string token's text.
fn prefix_len(text: &str) -> usize {
    text.bytes()
        .take_while(|&c| c != b'\'' && c != b'"')
        .count()
}

/// An integer literal: `i64` when it fits, else its text as ruff kept it
/// (decimal without underscores; other bases as written).
fn int_const(text: &str) -> Const {
    let b = text.as_bytes();
    let radix = if b.len() > 1 && b[0] == b'0' {
        match b[1].to_ascii_lowercase() {
            b'x' => 16,
            b'o' => 8,
            b'b' => 2,
            _ => 10,
        }
    } else {
        10
    };
    let digits: String = if radix == 10 {
        text.chars().filter(|&c| c != '_').collect()
    } else {
        text[2..].chars().filter(|&c| c != '_').collect()
    };
    match u64::from_str_radix(&digits, radix) {
        Ok(v) => match i64::try_from(v) {
            Ok(v) => Const::Int(v),
            Err(_) => Const::BigInt(v.to_string()),
        },
        Err(_) if radix == 10 => Const::BigInt(digits),
        Err(_) => Const::BigInt(text.to_string()),
    }
}

/// The first sub-expression that cannot be a target, if any.
fn invalid_target(e: &Expr, how: Target) -> Option<&Expr> {
    match &e.kind {
        ExprKind::Name(_) | ExprKind::Attribute(..) | ExprKind::Subscript(..) => None,
        ExprKind::Tuple(elts) | ExprKind::List(elts) => {
            elts.iter().find_map(|x| invalid_target(x, how))
        }
        ExprKind::Starred(v) if how == Target::Store => match &v.kind {
            ExprKind::Starred(_) => Some(e),
            _ => invalid_target(v, how),
        },
        _ => Some(e),
    }
}

/// `__annotations__[name] = text` at `pos`.
pub(crate) fn annotation_store(name: &str, text: String, pos: Pos) -> Stmt {
    let target = ExprKind::Subscript(
        Box::new(Expr::new(ExprKind::Name("__annotations__".into()), pos)),
        Box::new(Expr::new(ExprKind::Const(Const::Str(name.into())), pos)),
    );
    Stmt {
        kind: StmtKind::Assign(
            vec![Expr::new(target, pos)],
            Expr::new(ExprKind::Const(Const::Str(text)), pos),
        ),
        pos,
    }
}

/// Starts an annotated class body with `__annotations__ = {}` (after any
/// docstring); `pos` is the class statement's.
pub(crate) fn annotations_dict(body: &mut Vec<Stmt>, pos: Pos) {
    let at = usize::from(matches!(
        body.first().map(|s| &s.kind),
        Some(StmtKind::Expr(Expr {
            kind: ExprKind::Const(Const::Str(_)),
            ..
        }))
    ));
    body.insert(
        at,
        Stmt {
            kind: StmtKind::Assign(
                vec![Expr::new(ExprKind::Name("__annotations__".into()), pos)],
                Expr::new(ExprKind::Dict(Vec::new()), pos),
            ),
            pos,
        },
    );
}

/// What a class-body statement uses that micro-monty rejects (`__slots__`,
/// `__del__`, `__getattribute__`), if anything.
fn class_body_unsupported(s: &Stmt) -> Option<(&'static str, Pos)> {
    let is_slots = |t: &Expr| matches!(&t.kind, ExprKind::Name(n) if n == "__slots__");
    match &s.kind {
        StmtKind::Assign(targets, _) if targets.iter().any(is_slots) => Some(("__slots__", s.pos)),
        StmtKind::AnnAssign(t, _, _) if is_slots(t) => Some(("__slots__", s.pos)),
        StmtKind::FunctionDef(f) if f.name == "__del__" => Some(("__del__", s.pos)),
        StmtKind::FunctionDef(f) if f.name == "__getattribute__" => {
            Some(("__getattribute__", s.pos))
        }
        _ => None,
    }
}
