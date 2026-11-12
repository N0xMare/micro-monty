//! The engine AST: what every parser lowers into and the compiler consumes.
//!
//! Plain Rust data (`Box`/`Vec`), not the Python heap. Positions are 1-based
//! lines and 0-based character columns, as CPython reports them after
//! converting its byte offsets.

use alloc::{boxed::Box, string::String, vec::Vec};

/// A source range.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Pos {
    /// First line (1-based).
    pub line: u32,
    /// First column (0-based, in characters).
    pub col: u32,
    /// Last line.
    pub end_line: u32,
    /// Column just past the end.
    pub end_col: u32,
}

/// Where CPython's traceback puts the `^` part of a `~~^~~` underline: for a
/// binary operator, the operator; for a call or subscript, from the opening
/// bracket to the end. Absolute lines and character columns.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Anchor {
    /// 0 = none, 1 = binary operator (`~` outside, `^` inside), 2 = bracket.
    pub kind: u8,
    /// Line where the `^` part starts.
    pub left_line: u32,
    /// Column where the `^` part starts.
    pub left_col: u32,
    /// Line where the `^` part ends.
    pub right_line: u32,
    /// Column just past the `^` part.
    pub right_col: u32,
}

/// An expression with its position.
#[derive(Clone, Debug, PartialEq)]
pub struct Expr {
    /// What it is.
    pub kind: ExprKind,
    /// Where it is.
    pub pos: Pos,
    /// Caret anchors for calls, subscripts and binary operators.
    pub anchor: Anchor,
    /// CPython shows no carets for `return f(x)` / `x = f(x)` when the call is
    /// the whole statement line; set by the parser on such calls.
    pub no_carets: bool,
}

impl Expr {
    /// An expression without anchors.
    #[must_use]
    pub fn new(kind: ExprKind, pos: Pos) -> Expr {
        Expr {
            kind,
            pos,
            anchor: Anchor::default(),
            no_carets: false,
        }
    }
}

impl Expr {
    /// The operand evaluated first in a left-nested chain: `a` of `a + b`,
    /// `a.x` and `a[i]`, and a call's callee. A chain (`a + b + c`,
    /// `x.f(1).g[0]`) nests as deeply as it is long, so every walk follows
    /// it in a loop and counts it as one level of nesting.
    #[must_use]
    pub fn chain_left(&self) -> Option<&Expr> {
        match &self.kind {
            ExprKind::BinOp(a, _, _)
            | ExprKind::Attribute(a, _)
            | ExprKind::Subscript(a, _)
            | ExprKind::Call(a, _, _) => Some(a),
            _ => None,
        }
    }
}

/// Chains (`a + b + ...`, `x.f().g()...`, `a if c else b if ...`) and
/// unary prefixes are dropped in a loop, not by recursion.
impl Drop for Expr {
    fn drop(&mut self) {
        let mut next = self.kind.take_chain();
        while let Some(mut e) = next {
            next = e.kind.take_chain();
        }
    }
}

impl ExprKind {
    /// Empties a chain node, dropping its other parts, and returns the
    /// operand that continues the chain.
    fn take_chain(&mut self) -> Option<Box<Expr>> {
        if !matches!(
            self,
            ExprKind::BinOp(..)
                | ExprKind::Attribute(..)
                | ExprKind::Subscript(..)
                | ExprKind::Call(..)
                | ExprKind::IfExp(..)
                | ExprKind::Unary(..)
        ) {
            return None;
        }
        match core::mem::replace(self, ExprKind::Const(Const::None)) {
            ExprKind::BinOp(a, _, _)
            | ExprKind::Attribute(a, _)
            | ExprKind::Subscript(a, _)
            | ExprKind::Call(a, _, _)
            | ExprKind::Unary(_, a)
            | ExprKind::IfExp(_, _, a) => Some(a),
            _ => None,
        }
    }
}

/// A literal.
#[derive(Clone, Debug, PartialEq)]
pub enum Const {
    /// `None`.
    None,
    /// `True` / `False`.
    Bool(bool),
    /// `...`.
    Ellipsis,
    /// An integer literal that fits `i64`.
    Int(i64),
    /// A float literal.
    Float(f64),
    /// A string literal (implicit concatenation already applied).
    Str(String),
    /// An integer literal past `i64`, as its source text (decimal or with a
    /// base prefix, underscores allowed).
    BigInt(String),
    /// A bytes literal (implicit concatenation already applied).
    Bytes(Vec<u8>),
    /// An imaginary literal (`2j`): its imaginary part.
    Complex(f64),
}

/// Binary operators.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum BinOp {
    /// `+`
    Add = 0,
    /// `-`
    Sub = 1,
    /// `*`
    Mul = 2,
    /// `/`
    Div = 3,
    /// `//`
    FloorDiv = 4,
    /// `%`
    Mod = 5,
    /// `**`
    Pow = 6,
    /// `<<`
    LShift = 7,
    /// `>>`
    RShift = 8,
    /// `&`
    BitAnd = 9,
    /// `|`
    BitOr = 10,
    /// `^`
    BitXor = 11,
    /// `@`
    MatMul = 12,
}

impl BinOp {
    /// Every operator, indexed by its discriminant.
    pub const ALL: [BinOp; 13] = [
        BinOp::Add,
        BinOp::Sub,
        BinOp::Mul,
        BinOp::Div,
        BinOp::FloorDiv,
        BinOp::Mod,
        BinOp::Pow,
        BinOp::LShift,
        BinOp::RShift,
        BinOp::BitAnd,
        BinOp::BitOr,
        BinOp::BitXor,
        BinOp::MatMul,
    ];

    /// The operator's source text.
    #[must_use]
    pub fn symbol(self) -> &'static str {
        match self {
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
            BinOp::FloorDiv => "//",
            BinOp::Mod => "%",
            BinOp::Pow => "**",
            BinOp::LShift => "<<",
            BinOp::RShift => ">>",
            BinOp::BitAnd => "&",
            BinOp::BitOr => "|",
            BinOp::BitXor => "^",
            BinOp::MatMul => "@",
        }
    }
}

/// Unary operators.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnaryOp {
    /// `-x`
    Neg,
    /// `+x`
    Pos,
    /// `not x`
    Not,
    /// `~x`
    Invert,
}

/// `and` / `or`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoolOp {
    /// `and`
    And,
    /// `or`
    Or,
}

/// Comparison operators.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CmpOp {
    /// `==`
    Eq,
    /// `!=`
    Ne,
    /// `<`
    Lt,
    /// `<=`
    Le,
    /// `>`
    Gt,
    /// `>=`
    Ge,
    /// `in`
    In,
    /// `not in`
    NotIn,
    /// `is`
    Is,
    /// `is not`
    IsNot,
}

/// A keyword argument in a call; `name` is `None` for `**mapping`.
#[derive(Clone, Debug, PartialEq)]
pub struct Keyword {
    /// The keyword, or `None` for `**`.
    pub name: Option<String>,
    /// The value.
    pub value: Expr,
    /// The whole `name=value` (or `**value`).
    pub pos: Pos,
}

/// One `for ... in ... if ...` clause of a comprehension.
#[derive(Clone, Debug, PartialEq)]
pub struct Comprehension {
    /// The loop target.
    pub target: Expr,
    /// The iterable.
    pub iter: Expr,
    /// The `if` filters.
    pub ifs: Vec<Expr>,
    /// `async for`.
    pub is_async: bool,
}

/// A part of an f-string.
#[derive(Clone, Debug, PartialEq)]
pub enum FPart {
    /// Literal text.
    Lit(String),
    /// A replacement field.
    Field {
        /// The expression.
        expr: Box<Expr>,
        /// `b's'`, `b'r'`, `b'a'` or 0.
        conv: u8,
        /// The format spec, itself an f-string.
        spec: Option<Vec<FPart>>,
        /// The whole field, braces included (where formatting errors point).
        pos: Pos,
    },
}

/// Expression kinds.
#[derive(Clone, Debug, PartialEq)]
pub enum ExprKind {
    /// A literal.
    Const(Const),
    /// A name.
    Name(String),
    /// `a op b`.
    BinOp(Box<Expr>, BinOp, Box<Expr>),
    /// `op a`.
    Unary(UnaryOp, Box<Expr>),
    /// `a and b and ...`.
    BoolOp(BoolOp, Vec<Expr>),
    /// `a < b <= c`.
    Compare(Box<Expr>, Vec<(CmpOp, Expr)>),
    /// `body if test else orelse`.
    IfExp(Box<Expr>, Box<Expr>, Box<Expr>),
    /// `lambda params: body` (the body is one `return` statement).
    Lambda(Box<FuncDef>),
    /// A call.
    Call(Box<Expr>, Vec<Expr>, Vec<Keyword>),
    /// `value.attr`.
    Attribute(Box<Expr>, String),
    /// `value[index]`.
    Subscript(Box<Expr>, Box<Expr>),
    /// `lower:upper:step` inside a subscript.
    Slice(Option<Box<Expr>>, Option<Box<Expr>>, Option<Box<Expr>>),
    /// `*value` in a display, call or target.
    Starred(Box<Expr>),
    /// `[...]`.
    List(Vec<Expr>),
    /// `(...)`.
    Tuple(Vec<Expr>),
    /// `{a, b}`.
    Set(Vec<Expr>),
    /// `{k: v, **m}`; a `None` key is `**value`.
    Dict(Vec<(Option<Expr>, Expr)>),
    /// `[elt for ...]`.
    ListComp(Box<Expr>, Vec<Comprehension>),
    /// `{elt for ...}`.
    SetComp(Box<Expr>, Vec<Comprehension>),
    /// `{k: v for ...}`.
    DictComp(Box<Expr>, Box<Expr>, Vec<Comprehension>),
    /// `(elt for ...)`.
    GenExp(Box<Expr>, Vec<Comprehension>),
    /// An f-string.
    FString(Vec<FPart>),
    /// `name := value`.
    Named(String, Box<Expr>),
    /// `await value`.
    Await(Box<Expr>),
    /// `yield [value]`.
    Yield(Option<Box<Expr>>),
    /// `yield from value`.
    YieldFrom(Box<Expr>),
}

/// A parameter with an optional default.
#[derive(Clone, Debug, PartialEq)]
pub struct Param {
    /// Its name.
    pub name: String,
    /// Its default value.
    pub default: Option<Expr>,
    /// Its annotation (only from `mm_parse::parse_tree`, for `ast`).
    pub ann: Option<Box<Expr>>,
    /// Its position, name and annotation (only from `mm_parse::parse_tree`).
    pub pos: Pos,
}

/// A function's parameter list.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Params {
    /// Positional-only parameters.
    pub posonly: Vec<Param>,
    /// Positional-or-keyword parameters.
    pub args: Vec<Param>,
    /// `*args`.
    pub vararg: Option<String>,
    /// Keyword-only parameters.
    pub kwonly: Vec<Param>,
    /// `**kwargs`.
    pub kwarg: Option<String>,
    /// `*args` and `**kwargs` with their annotations and positions (only
    /// from `mm_parse::parse_tree`, for `ast`).
    pub star: Option<Box<[Option<Param>; 2]>>,
}

/// A `def` or `lambda`.
#[derive(Clone, Debug, PartialEq)]
pub struct FuncDef {
    /// The name (`<lambda>` for lambdas).
    pub name: String,
    /// Parameters.
    pub params: Params,
    /// Body statements.
    pub body: Vec<Stmt>,
    /// Decorators, outermost first.
    pub decorators: Vec<Expr>,
    /// Position of the whole definition (for a `def`, its first line).
    pub pos: Pos,
    /// `async def`: calling it makes a coroutine.
    pub is_async: bool,
    /// The return annotation (only from `mm_parse::parse_tree`, for `ast`).
    pub returns: Option<Box<Expr>>,
}

/// An `except` clause.
#[derive(Clone, Debug, PartialEq)]
pub struct Handler {
    /// The exception type expression, or `None` for a bare `except:`.
    pub typ: Option<Expr>,
    /// The `as` name.
    pub name: Option<String>,
    /// The handler body.
    pub body: Vec<Stmt>,
    /// Position of the `except` clause.
    pub pos: Pos,
}

/// A statement with its position.
#[derive(Clone, Debug, PartialEq)]
pub struct Stmt {
    /// What it is.
    pub kind: StmtKind,
    /// Where it is.
    pub pos: Pos,
}

/// An `elif` chain is dropped in a loop, not by recursion.
impl Drop for Stmt {
    fn drop(&mut self) {
        if let StmtKind::If(_, _, orelse) = &mut self.kind
            && let [
                Stmt {
                    kind: StmtKind::If(..),
                    ..
                },
            ] = orelse.as_slice()
        {
            let mut work = core::mem::take(orelse);
            while let Some(mut s) = work.pop() {
                if let StmtKind::If(_, _, o) = &mut s.kind {
                    work.append(o);
                }
            }
        }
    }
}

/// Statement kinds.
#[derive(Clone, Debug, PartialEq)]
pub enum StmtKind {
    /// An expression statement.
    Expr(Expr),
    /// `t1 = t2 = value`.
    Assign(Vec<Expr>, Expr),
    /// `target op= value`.
    AugAssign(Expr, BinOp, Expr),
    /// `target: annotation [= value]` (the annotation is never evaluated,
    /// and kept only by `mm_parse::parse_tree`, for `ast`).
    AnnAssign(Expr, Option<Expr>, Option<Box<Expr>>),
    /// `if test: body else: orelse`.
    If(Expr, Vec<Stmt>, Vec<Stmt>),
    /// `while test: body else: orelse`.
    While(Expr, Vec<Stmt>, Vec<Stmt>),
    /// `[async] for target in iter: body else: orelse`.
    For(Expr, Expr, Vec<Stmt>, Vec<Stmt>, bool),
    /// `break`.
    Break,
    /// `continue`.
    Continue,
    /// `pass`.
    Pass,
    /// `return [value]`.
    Return(Option<Expr>),
    /// `def`.
    FunctionDef(Box<FuncDef>),
    /// `del targets`.
    Delete(Vec<Expr>),
    /// `global names`.
    Global(Vec<String>),
    /// `nonlocal names`.
    Nonlocal(Vec<String>),
    /// `raise [exc [from cause]]`.
    Raise(Option<Expr>, Option<Expr>),
    /// `try` with handlers, `else` and `finally`.
    Try {
        /// The protected body.
        body: Vec<Stmt>,
        /// `except` clauses.
        handlers: Vec<Handler>,
        /// `else` body.
        orelse: Vec<Stmt>,
        /// `finally` body.
        finalbody: Vec<Stmt>,
        /// `except*` handlers (exception groups).
        star: bool,
    },
    /// `assert test[, msg]`.
    Assert(Expr, Option<Expr>),
    /// `import a.b as c, d`.
    Import(Vec<Alias>),
    /// `from module import a, b as c` (absolute, no `*`).
    ImportFrom(String, Vec<Alias>),
    /// `class`.
    ClassDef(Box<ClassDef>),
    /// `[async] with a as x, b: body`.
    With(Vec<WithItem>, Vec<Stmt>, bool),
    /// `match subject: case ...`.
    Match(Expr, Vec<MatchCase>),
}

/// One `case pattern [if guard]: body` of a `match` statement.
#[derive(Clone, Debug, PartialEq)]
pub struct MatchCase {
    /// The pattern.
    pub pattern: Pattern,
    /// The guard.
    pub guard: Option<Expr>,
    /// The body.
    pub body: Vec<Stmt>,
}

/// A pattern with its position (PEP 634).
#[derive(Clone, Debug, PartialEq)]
pub struct Pattern {
    /// What it is.
    pub kind: PatternKind,
    /// Where it is.
    pub pos: Pos,
}

/// Pattern kinds, as CPython's `ast.pattern`.
#[derive(Clone, Debug, PartialEq)]
pub enum PatternKind {
    /// A literal or dotted name, compared with `==`.
    Value(Expr),
    /// `None`, `True` or `False`, compared with `is`.
    Singleton(Const),
    /// `[p, *rest, q]` or `(p, q)` or `p, q`.
    Sequence(Vec<Pattern>),
    /// `*name` inside a sequence pattern (`None` for `*_`).
    Star(Option<String>),
    /// `{key: p, **rest}`.
    Mapping {
        /// Literal or dotted-name keys.
        keys: Vec<Expr>,
        /// One pattern per key.
        patterns: Vec<Pattern>,
        /// The `**rest` name.
        rest: Option<String>,
    },
    /// `Cls(p, name=p)`.
    Class {
        /// The class (a name or dotted name).
        cls: Expr,
        /// Positional sub-patterns.
        patterns: Vec<Pattern>,
        /// Keyword names.
        kwd_attrs: Vec<String>,
        /// Keyword sub-patterns.
        kwd_patterns: Vec<Pattern>,
    },
    /// `p as name`, a capture `name` (no pattern) or the wildcard `_`
    /// (neither).
    As(Option<Box<Pattern>>, Option<String>),
    /// `p | q | ...`.
    Or(Vec<Pattern>),
}

/// One `ctx [as target]` of a `with` statement.
#[derive(Clone, Debug, PartialEq)]
pub struct WithItem {
    /// The context manager expression.
    pub ctx: Expr,
    /// The `as` target.
    pub target: Option<Expr>,
}

/// A `class` statement.
#[derive(Clone, Debug, PartialEq)]
pub struct ClassDef {
    /// The class name.
    pub name: String,
    /// Base class expressions (may contain `*args`).
    pub bases: Vec<Expr>,
    /// Class keywords (passed to `__init_subclass__`).
    pub keywords: Vec<Keyword>,
    /// The body.
    pub body: Vec<Stmt>,
    /// Decorators, outermost first.
    pub decorators: Vec<Expr>,
    /// Position of the `class` keyword.
    pub pos: Pos,
    /// From the `class` keyword to the end of the body.
    pub span: Pos,
}

/// One name of an `import` statement.
#[derive(Clone, Debug, PartialEq)]
pub struct Alias {
    /// The (dotted, for `import`) name.
    pub name: String,
    /// The `as` name.
    pub asname: Option<String>,
    /// Its position (only from `mm_parse::parse_tree`, for `ast`).
    pub pos: Pos,
}

/// A parsed module.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Module {
    /// Top-level statements.
    pub body: Vec<Stmt>,
}
