//! Python source to the `mm-compile` engine AST.
//!
//! An internal crate of [micro-monty](https://crates.io/crates/micro-monty),
//! published only because the facade depends on it: its API has no semver
//! guarantees and changes with every release. Depend on `micro-monty`.
//!
//! micro-monty's own `no_std` front end (docs/spec.md §6): a linear
//! pre-scan (`prescan`) bounds bracket, indentation and operator nesting
//! the way CPython's tokenizer does; an indentation-aware tokenizer
//! (`lexer`) produces a token vector (PEP 701 f-strings included); a
//! depth-counted recursive-descent parser (`parser`) builds the engine AST
//! directly. Constructs micro-monty rejects become marker nodes that a final
//! walk (`walk`) reports in the order the phase-1 ruff lowering did, so
//! errors are unchanged.
//!
//! The ruff-based phase-1 path survives behind the `ruff-oracle` feature,
//! only for the differential test against this parser.

#![cfg_attr(not(feature = "ruff-oracle"), no_std)]
#![allow(clippy::doc_markdown, reason = "the docs name CPython in prose")]
#![allow(
    clippy::many_single_char_names,
    reason = "byte scanners use b, c, i, j, w"
)]
#![allow(
    clippy::missing_errors_doc,
    reason = "every fallible function returns the one `CompileError`"
)]

extern crate alloc;

mod lexer;
pub mod lines;
mod parser;
mod prescan;
mod strings;
mod unparse;
mod walk;

#[cfg(feature = "ruff-oracle")]
pub mod ruff;

use alloc::vec::Vec;
use mm_compile::{CompileError, ast::Module};

/// The message of a source holding a null byte (CPython 3.14 reading a
/// file; `compile()` of a string says "source code string ...").
pub const NULL_BYTES: &str = "source code cannot contain null bytes";

/// Parses `source` into the engine AST.
pub fn parse(source: &str) -> Result<Module, CompileError> {
    parse_with(source, 0)
}

/// Parses `source` into the tree the `ast` module shows: the engine AST
/// with annotations and the positions of parameters and import names, and
/// without the compiler's rewrites (`-literal` folding, class-body
/// `__annotations__`, dropped `__future__` imports, rejected class bodies).
/// With `eval`, `source` is CPython's `eval` input: one expression list,
/// returned as the module's one expression statement.
pub fn parse_tree(source: &str, eval: bool) -> Result<Module, CompileError> {
    parse_with(source, if eval { 2 } else { 1 })
}

/// The byte ranges of `source`'s plain string pieces (prefix and quotes
/// included) and f-string literal text, in source order: what the `ast`
/// module positions an f-string's `Constant` parts from. Empty for source
/// that does not tokenize.
#[must_use]
pub fn string_segments(source: &str) -> Vec<(u32, u32)> {
    let lines = lines::Lines::new(source);
    lexer::lex(source, &lines)
        .toks
        .iter()
        .filter(|t| matches!(t.k, lexer::T::Str | lexer::T::FMiddle))
        .map(|t| (t.s, t.e))
        .collect()
}

/// CPython's `tokenize` stream over the lexer: `(token type, start, end)`
/// byte ranges, with the comments and non-logical newlines (`NL`) the lexer
/// skips, escaped braces splitting f-string text as CPython splits it, and
/// the error that ended the stream. A zero-width `NEWLINE` or `NL` at the
/// end of the source is CPython's empty one.
#[must_use]
pub fn tokens(source: &str) -> (Vec<(u8, u32, u32)>, Option<CompileError>) {
    use lexer::T;
    const NEWLINE: u8 = 4;
    const DEDENT: u8 = 6;
    let lines = lines::Lines::new(source);
    let lexed = lexer::lex(source, &lines);
    let b = source.as_bytes();
    let n = b.len();
    let off = |i: usize| u32::try_from(i).unwrap_or(u32::MAX);
    let mut out: Vec<(u8, u32, u32)> = Vec::with_capacity(lexed.toks.len());
    // Whether each open f-string is a t-string.
    let mut tstr: Vec<bool> = Vec::new();
    let mut at = 0usize;
    for t in &lexed.toks {
        at = gap(b, at, t.s as usize, &mut out);
        // A last line without a newline that the lexer ends no logical
        // line on (a comment, an open bracket) gets CPython's empty NL.
        if at >= n
            && n > 0
            && !matches!(b[n - 1], b'\n' | b'\r' | b'\\')
            && t.k != T::Newline
            && out
                .last()
                .is_some_and(|l| !matches!(l.0, NEWLINE | NL | DEDENT))
        {
            out.push((NL, t.s, t.s));
        }
        let t_string = tstr.last().copied().unwrap_or(false);
        let k = match t.k {
            T::Error => break,
            T::End => 0,
            T::Name => 1,
            T::Int | T::Float | T::Complex => 2,
            T::Str => 3,
            T::Newline => NEWLINE,
            T::Indent => 5,
            T::Dedent => DEDENT,
            T::FStart => {
                tstr.push(t.f & lexer::F_T != 0);
                if t.f & lexer::F_T != 0 { 62 } else { 59 }
            }
            T::FMiddle => {
                let k = if t_string { 63 } else { 60 };
                // `{{` and `}}` end a piece after their first brace.
                let (mut s, e) = (t.s as usize, t.e as usize);
                let mut j = s;
                while j + 1 < e {
                    if matches!(b[j], b'{' | b'}') && b[j + 1] == b[j] {
                        out.push((k, off(s), off(j + 1)));
                        j += 2;
                        s = j;
                    } else {
                        j += 1;
                    }
                }
                if s < e {
                    out.push((k, off(s), off(e)));
                }
                at = e;
                continue;
            }
            T::FEnd => {
                tstr.pop();
                if t_string { 64 } else { 61 }
            }
            // CPython ends a format spec with its last, maybe empty, piece.
            T::RBrace if t.f == 1 && out.last().is_none_or(|l| !matches!(l.0, 60 | 63)) => {
                out.push((if t_string { 63 } else { 60 }, t.s, t.s));
                55
            }
            // Keywords are names.
            k if k as u8 >= T::False as u8 => 1,
            _ => 55,
        };
        out.push((k, t.s, t.e));
        at = at.max(t.e as usize);
    }
    // CPython reports a dedent to no outer level before its DEDENTs.
    if lexed
        .err
        .as_ref()
        .is_some_and(|e| e.kind == "IndentationError")
    {
        while out.last().is_some_and(|l| l.0 == DEDENT) {
            out.pop();
        }
    }
    (out, lexed.err)
}

/// `tokenize`'s `COMMENT` and `NL` tokens.
const COMMENT: u8 = 65;
const NL: u8 = 66;

/// Scans the bytes from `at` up to the next lexer token at `end` (blanks,
/// continuations, comments and non-logical newlines) into `out`; returns
/// where it stopped.
fn gap(b: &[u8], mut at: usize, end: usize, out: &mut Vec<(u8, u32, u32)>) -> usize {
    let n = b.len();
    let off = |i: usize| u32::try_from(i).unwrap_or(u32::MAX);
    while at < end {
        let s = at;
        match b[at] {
            b' ' | b'\t' | b'\x0c' => at += 1,
            b'\\' => {
                at += 1 + usize::from(b.get(at + 1) == Some(&b'\r'));
                at += usize::from(b.get(at) == Some(&b'\n'));
            }
            b'#' => {
                while at < n && !matches!(b[at], b'\n' | b'\r') {
                    at += 1;
                }
                out.push((COMMENT, off(s), off(at)));
            }
            b'\n' | b'\r' => {
                at += if b[at] == b'\r' && b.get(at + 1) == Some(&b'\n') {
                    2
                } else {
                    1
                };
                out.push((NL, off(s), off(at)));
            }
            _ => break,
        }
    }
    at
}

/// `mode`: 0 the compiler's tree, 1 the `ast` tree, 2 the `ast` tree of
/// an `eval` input.
fn parse_with(source: &str, mode: u8) -> Result<Module, CompileError> {
    let lines = lines::Lines::new(source);
    // CPython's file tokenizer fails on reading the line that holds a null
    // byte: a tokenizer error on an earlier line comes first; a later one,
    // and every parser error (its error pass reads to the end), after.
    let null = source.find('\0').map(|at| {
        let line =
            u32::try_from(lines.line_of(u32::try_from(at).unwrap_or(u32::MAX))).unwrap_or(u32::MAX);
        let pos = mm_compile::ast::Pos {
            line,
            col: 0,
            end_line: line,
            end_col: 0,
        };
        CompileError::syntax(NULL_BYTES.into(), pos)
    });
    let earlier = |e: CompileError| match &null {
        Some(n) if e.pos.line == 0 || e.pos.line >= n.pos.line => n.clone(),
        _ => e,
    };
    prescan::check(source).map_err(earlier)?;
    let lexed = lexer::lex(source, &lines);
    if let Some(n) = null
        && lexed
            .err
            .as_ref()
            .is_none_or(|e| e.pos.line == 0 || e.pos.line >= n.pos.line)
    {
        return Err(n);
    }
    let mut p = parser::Parser::new(source, &lines, lexed.toks, lexed.err, lexed.late);
    p.tree = mode > 0;
    let body = if mode == 2 {
        p.eval_input()
    } else {
        p.module()
    };
    let body = body.map_err(|e| p.tokenizer_first(e))?;
    let mut elifs = core::mem::take(&mut p.elifs);
    heap_sort(&mut elifs, |e| (e.0, e.1, 0, 0));
    let mut folds = core::mem::take(&mut p.folds);
    heap_sort(&mut folds, |f| {
        (f.0.line, f.0.col, f.0.end_line, f.0.end_col)
    });
    let mut deferred = core::mem::take(&mut p.deferred);
    let mut w = walk::Walk {
        deferred: &mut deferred,
        elifs: &elifs,
        folds: &folds,
        depth: 0,
    };
    w.body(&body)?;
    Ok(Module { body })
}

/// Sorts `v` by `key` (heapsort: `O(n log n)` and much smaller than the
/// standard library's sorts, which matters for the size budget).
fn heap_sort<X>(v: &mut [X], key: impl Fn(&X) -> (u32, u32, u32, u32)) {
    let sift = |v: &mut [X], mut root: usize, end: usize| loop {
        let mut child = 2 * root + 1;
        if child >= end {
            break;
        }
        if child + 1 < end && key(&v[child]) < key(&v[child + 1]) {
            child += 1;
        }
        if key(&v[root]) >= key(&v[child]) {
            break;
        }
        v.swap(root, child);
        root = child;
    };
    let n = v.len();
    for i in (0..n / 2).rev() {
        sift(v, i, n);
    }
    for end in (1..n).rev() {
        v.swap(0, end);
        sift(v, 0, end);
    }
}
