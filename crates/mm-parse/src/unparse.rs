//! Annotation stringization: an expression back to source text the way
//! CPython's `Python/ast_unparse.c` writes it (what `from __future__ import
//! annotations` stores in `__annotations__`). Class-body annotations are
//! stored this way (docs/spec.md §4: annotations are never evaluated).
//!
//! It works on the engine AST, so the own parser and the `ruff-oracle`
//! lowering stringize identically. Kinds it does not rewrite (floats,
//! lambdas, comprehensions, ...) are copied from the source. The recursion
//! follows the expression's nesting and its chains (`a.b.c`, `a + b + c`);
//! past `MAX_DEPTH` levels of both the annotation is copied from the source
//! too.

use alloc::string::String;
use core::fmt::Write as _;
use mm_compile::ast::{BinOp, BoolOp, CmpOp, Const, Expr, ExprKind, FPart, Pos, UnaryOp};

use crate::parser::{MARK, MARK_CLASS, MARK_ENTER};

// CPython's precedence levels (`ast_unparse.c`).
const PR_TUPLE: u8 = 0;
const PR_TEST: u8 = 1;
const PR_OR: u8 = 2;
const PR_AND: u8 = 3;
const PR_NOT: u8 = 4;
const PR_CMP: u8 = 5;
const PR_EXPR: u8 = 6;
const PR_BOR: u8 = 6;
const PR_BXOR: u8 = 7;
const PR_BAND: u8 = 8;
const PR_SHIFT: u8 = 9;
const PR_ARITH: u8 = 10;
const PR_TERM: u8 = 11;
const PR_FACTOR: u8 = 12;
const PR_POWER: u8 = 13;
const PR_ATOM: u8 = 15;

/// The deepest expression stringized; the parser bounds nesting at 200
/// levels but not the length of a chain.
const MAX_DEPTH: u32 = 400;

/// The annotation `e` as a string; `whole` is the annotation's own
/// position and `src` the module source. An annotation holding a construct
/// micro-monty rejects (a parser marker) is copied from the source whole,
/// as the `ruff-oracle` lowering does when lowering it fails.
pub(crate) fn annotation(e: &Expr, src: &str) -> String {
    let mut out = String::new();
    let mut u = Unparser {
        src,
        out: &mut out,
        bad: false,
        depth: 0,
    };
    u.expr(e, PR_TEST);
    if u.bad {
        return String::from(slice(src, e.pos));
    }
    out
}

/// The source text at `pos` (lines from 1, character columns).
pub(crate) fn slice(src: &str, pos: Pos) -> &str {
    let start = offset(src, pos.line, pos.col);
    let end = offset(src, pos.end_line, pos.end_col).max(start);
    src.get(start..end).unwrap_or_default()
}

fn offset(src: &str, line: u32, col: u32) -> usize {
    let mut start = 0;
    for _ in 1..line {
        match src.get(start..).and_then(|t| t.find('\n')) {
            Some(i) => start += i + 1,
            None => return src.len(),
        }
    }
    src.get(start..)
        .unwrap_or_default()
        .char_indices()
        .nth(col as usize)
        .map_or(src.len(), |(i, _)| start + i)
}

struct Unparser<'a> {
    src: &'a str,
    out: &'a mut String,
    /// A parser marker was reached, or `MAX_DEPTH`.
    bad: bool,
    depth: u32,
}

impl Unparser<'_> {
    fn open(&mut self, cond: bool) {
        if cond {
            self.out.push('(');
        }
    }

    fn close(&mut self, cond: bool) {
        if cond {
            self.out.push(')');
        }
    }

    fn list(&mut self, items: &[Expr], level: u8) {
        for (i, e) in items.iter().enumerate() {
            if i > 0 {
                self.out.push_str(", ");
            }
            self.expr(e, level);
        }
    }

    fn verbatim(&mut self, e: &Expr) {
        let text = slice(self.src, e.pos);
        self.out.push_str(text);
    }

    fn expr(&mut self, e: &Expr, level: u8) {
        if self.depth == MAX_DEPTH {
            self.bad = true;
        }
        if self.bad {
            return;
        }
        self.depth += 1;
        self.node(e, level);
        self.depth -= 1;
    }

    #[allow(clippy::too_many_lines, reason = "one arm per expression kind")]
    fn node(&mut self, e: &Expr, level: u8) {
        match &e.kind {
            ExprKind::Name(n) => self.out.push_str(n),
            ExprKind::Attribute(v, attr) => {
                self.expr(v, PR_ATOM);
                self.out.push('.');
                self.out.push_str(attr);
            }
            ExprKind::Subscript(v, index) => {
                self.expr(v, PR_ATOM);
                self.out.push('[');
                match &index.kind {
                    ExprKind::Tuple(elts) if !elts.is_empty() => {
                        self.list(elts, PR_TEST);
                        if elts.len() == 1 {
                            self.out.push(',');
                        }
                    }
                    _ => self.expr(index, PR_TUPLE),
                }
                self.out.push(']');
            }
            ExprKind::Slice(lower, upper, step) => {
                if let Some(l) = lower {
                    self.expr(l, PR_TEST);
                }
                self.out.push(':');
                if let Some(u) = upper {
                    self.expr(u, PR_TEST);
                }
                if let Some(st) = step {
                    self.out.push(':');
                    self.expr(st, PR_TEST);
                }
            }
            ExprKind::Tuple(elts) => {
                if elts.is_empty() {
                    self.out.push_str("()");
                    return;
                }
                let paren = level > PR_TUPLE;
                self.open(paren);
                self.list(elts, PR_TEST);
                if elts.len() == 1 {
                    self.out.push(',');
                }
                self.close(paren);
            }
            ExprKind::List(elts) => {
                self.out.push('[');
                self.list(elts, PR_TEST);
                self.out.push(']');
            }
            ExprKind::Set(elts) => {
                self.out.push('{');
                self.list(elts, PR_TEST);
                self.out.push('}');
            }
            ExprKind::Dict(items) => {
                self.out.push('{');
                for (i, (k, v)) in items.iter().enumerate() {
                    if i > 0 {
                        self.out.push_str(", ");
                    }
                    if let Some(k) = k {
                        self.expr(k, PR_TEST);
                        self.out.push_str(": ");
                        self.expr(v, PR_TEST);
                    } else {
                        self.out.push_str("**");
                        self.expr(v, PR_EXPR);
                    }
                }
                self.out.push('}');
            }
            ExprKind::Starred(v) => {
                self.out.push('*');
                self.expr(v, PR_EXPR);
            }
            ExprKind::Call(f, args, keywords) => {
                self.expr(f, PR_ATOM);
                self.out.push('(');
                let mut first = true;
                for a in args {
                    if !first {
                        self.out.push_str(", ");
                    }
                    first = false;
                    self.expr(a, PR_TEST);
                }
                for k in keywords {
                    if !first {
                        self.out.push_str(", ");
                    }
                    first = false;
                    match &k.name {
                        Some(name) => {
                            self.out.push_str(name);
                            self.out.push('=');
                        }
                        None => self.out.push_str("**"),
                    }
                    self.expr(&k.value, PR_TEST);
                }
                self.out.push(')');
            }
            ExprKind::BinOp(a, op, b) => {
                let (pr, right_assoc) = match op {
                    BinOp::Add | BinOp::Sub => (PR_ARITH, false),
                    BinOp::Mul | BinOp::MatMul | BinOp::Div | BinOp::Mod | BinOp::FloorDiv => {
                        (PR_TERM, false)
                    }
                    BinOp::LShift | BinOp::RShift => (PR_SHIFT, false),
                    BinOp::BitOr => (PR_BOR, false),
                    BinOp::BitXor => (PR_BXOR, false),
                    BinOp::BitAnd => (PR_BAND, false),
                    BinOp::Pow => (PR_POWER, true),
                };
                let paren = level > pr;
                self.open(paren);
                self.expr(a, if right_assoc { pr + 1 } else { pr });
                self.out.push(' ');
                self.out.push_str(op.symbol());
                self.out.push(' ');
                self.expr(b, if right_assoc { pr } else { pr + 1 });
                self.close(paren);
            }
            ExprKind::Unary(op, v) => {
                let (pr, sym) = match op {
                    UnaryOp::Not => (PR_NOT, "not "),
                    UnaryOp::Invert => (PR_FACTOR, "~"),
                    UnaryOp::Pos => (PR_FACTOR, "+"),
                    UnaryOp::Neg => (PR_FACTOR, "-"),
                };
                let paren = level > pr;
                self.open(paren);
                self.out.push_str(sym);
                self.expr(v, pr);
                self.close(paren);
            }
            ExprKind::BoolOp(op, values) => {
                let (pr, sym) = match op {
                    BoolOp::And => (PR_AND, " and "),
                    BoolOp::Or => (PR_OR, " or "),
                };
                let paren = level > pr;
                self.open(paren);
                for (i, v) in values.iter().enumerate() {
                    if i > 0 {
                        self.out.push_str(sym);
                    }
                    self.expr(v, pr + 1);
                }
                self.close(paren);
            }
            ExprKind::Compare(left, rest) => {
                let paren = level > PR_CMP;
                self.open(paren);
                self.expr(left, PR_CMP + 1);
                for (op, v) in rest {
                    self.out.push(' ');
                    self.out.push_str(cmp_symbol(*op));
                    self.out.push(' ');
                    self.expr(v, PR_CMP + 1);
                }
                self.close(paren);
            }
            ExprKind::IfExp(test, body, orelse) => {
                let paren = level > PR_TEST;
                self.open(paren);
                self.expr(body, PR_TEST + 1);
                self.out.push_str(" if ");
                self.expr(test, PR_TEST + 1);
                self.out.push_str(" else ");
                self.expr(orelse, PR_TEST);
                self.close(paren);
            }
            ExprKind::FString(parts) => {
                let mut body = String::new();
                self.fparts(parts, &mut body);
                self.out.push('f');
                str_repr(&body, self.out);
            }
            ExprKind::Const(c) => self.constant(e, c, level),
            _ => self.verbatim(e),
        }
    }

    fn constant(&mut self, e: &Expr, c: &Const, level: u8) {
        match c {
            Const::None if matches!(e.anchor.kind, MARK | MARK_ENTER | MARK_CLASS) => {
                self.bad = true;
            }
            Const::None => self.out.push_str("None"),
            Const::Bool(b) => self.out.push_str(if *b { "True" } else { "False" }),
            Const::Ellipsis => self.out.push_str("..."),
            Const::Int(i) => {
                // A folded `-n` is a unary minus to CPython.
                let paren = *i < 0 && level > PR_FACTOR;
                self.open(paren);
                let _ = write!(self.out, "{i}");
                self.close(paren);
            }
            Const::Str(s) => {
                let first = slice(self.src, e.pos).bytes().next();
                if matches!(first, Some(b'u' | b'U')) {
                    self.out.push('u');
                }
                str_repr(s, self.out);
            }
            Const::Bytes(b) => bytes_repr(b, self.out),
            Const::Float(_) | Const::BigInt(_) | Const::Complex(_) => self.verbatim(e),
        }
    }

    /// An f-string body (`build_fstring_body`): literals with braces
    /// doubled, fields as `{expr!c:spec}`.
    fn fparts(&mut self, parts: &[FPart], body: &mut String) {
        for p in parts {
            match p {
                FPart::Lit(s) => escape_braces(s, body),
                FPart::Field {
                    expr, conv, spec, ..
                } => {
                    let mut inner = String::new();
                    let mut u = Unparser {
                        src: self.src,
                        out: &mut inner,
                        bad: false,
                        depth: self.depth,
                    };
                    u.expr(expr, PR_TEST + 1);
                    self.bad |= u.bad;
                    body.push('{');
                    if inner.starts_with('{') {
                        body.push(' ');
                    }
                    body.push_str(&inner);
                    if *conv != 0 {
                        body.push('!');
                        body.push(char::from(*conv));
                    }
                    if let Some(spec) = spec {
                        body.push(':');
                        self.fparts(spec, body);
                    }
                    body.push('}');
                }
            }
        }
    }
}

fn cmp_symbol(op: CmpOp) -> &'static str {
    match op {
        CmpOp::Eq => "==",
        CmpOp::Ne => "!=",
        CmpOp::Lt => "<",
        CmpOp::Le => "<=",
        CmpOp::Gt => ">",
        CmpOp::Ge => ">=",
        CmpOp::In => "in",
        CmpOp::NotIn => "not in",
        CmpOp::Is => "is",
        CmpOp::IsNot => "is not",
    }
}

fn escape_braces(s: &str, out: &mut String) {
    for c in s.chars() {
        match c {
            '{' => out.push_str("{{"),
            '}' => out.push_str("}}"),
            _ => out.push(c),
        }
    }
}

/// Python's `repr(str)`.
fn str_repr(s: &str, out: &mut String) {
    let quote = if s.contains('\'') && !s.contains('"') {
        '"'
    } else {
        '\''
    };
    out.push(quote);
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c == quote => {
                out.push('\\');
                out.push(c);
            }
            c if (c as u32) < 0x20 || (0x7f..0xa0).contains(&(c as u32)) => {
                let _ = write!(out, "\\x{:02x}", c as u32);
            }
            c if c.is_control() || c.is_whitespace() && c != ' ' => {
                let n = c as u32;
                if n < 0x100 {
                    let _ = write!(out, "\\x{n:02x}");
                } else if n < 0x10000 {
                    let _ = write!(out, "\\u{n:04x}");
                } else {
                    let _ = write!(out, "\\U{n:08x}");
                }
            }
            c => out.push(c),
        }
    }
    out.push(quote);
}

/// Python's `repr(bytes)`.
fn bytes_repr(b: &[u8], out: &mut String) {
    let quote = if b.contains(&b'\'') && !b.contains(&b'"') {
        b'"'
    } else {
        b'\''
    };
    out.push('b');
    out.push(char::from(quote));
    for &c in b {
        match c {
            b'\\' => out.push_str("\\\\"),
            b'\n' => out.push_str("\\n"),
            b'\r' => out.push_str("\\r"),
            b'\t' => out.push_str("\\t"),
            c if c == quote => {
                out.push('\\');
                out.push(char::from(c));
            }
            0x20..0x7f => out.push(char::from(c)),
            c => {
                let _ = write!(out, "\\x{c:02x}");
            }
        }
    }
    out.push(char::from(quote));
}
