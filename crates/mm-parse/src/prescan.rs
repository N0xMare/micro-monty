//! A linear pre-scan that bounds nesting before ruff builds its AST.
//!
//! ruff parses deeply nested source fine but drops its AST recursively, and
//! its lowering recurses too. The scan follows CPython's tokenizer limits
//! (`too many nested parentheses` past 200 open brackets, `too many levels of
//! indentation` past 100 blocks) and estimates expression depth from
//! operator chains, rejecting with the compiler's `RecursionError` only what
//! is far beyond what CPython compiles (it runs out of stack between 12,000
//! and 50,000 `+` terms). The own front end walks chains in loops and counts
//! real nesting itself.

use alloc::{vec, vec::Vec};
use mm_compile::{CompileError, ast::Pos};

/// CPython's `MAXLEVEL`; the lexer enforces it too.
pub(crate) const MAX_BRACKETS: u32 = 200;
/// CPython's `MAXINDENT`.
const MAX_INDENT: usize = 100;
/// Upper bound on the estimated expression depth (chain operators and
/// brackets); the parser's own nesting limit applies after.
const MAX_CHAIN: u32 = 100_000;

#[derive(Clone, Copy)]
enum Mode {
    Str {
        q: u8,
        triple: bool,
        raw: bool,
        f: bool,
    },
    FExpr {
        base: u32,
    },
    FSpec {
        base: u32,
    },
}

fn at(line: u32, col: u32) -> Pos {
    Pos {
        line,
        col,
        end_line: line,
        end_col: col + 1,
    }
}

fn is_prefix(word: &[u8]) -> Option<(bool, bool)> {
    let mut raw = false;
    let mut f = false;
    if word.is_empty() || word.len() > 2 {
        return None;
    }
    for &c in word {
        match c.to_ascii_lowercase() {
            b'r' => raw = true,
            b'f' | b't' => f = true,
            b'b' | b'u' => {}
            _ => return None,
        }
    }
    Some((raw, f))
}

/// Whether `b[i]` ends a line: a `\n`, or a `\r` not before one (the `\r`
/// of `\r\n` does not; its `\n` does).
fn ends_line(b: &[u8], i: usize) -> bool {
    b[i] == b'\n' || (b[i] == b'\r' && b.get(i + 1) != Some(&b'\n'))
}

/// The length of the line break at `b[i]`, if any: 2 for `\r\n`.
fn line_break(b: &[u8], i: usize) -> usize {
    match b.get(i) {
        Some(b'\n') => 1,
        Some(b'\r') => 1 + usize::from(b.get(i + 1) == Some(&b'\n')),
        _ => 0,
    }
}

/// Checks bracket, indentation and expression-chain nesting.
#[allow(clippy::too_many_lines, reason = "one tokenizer state machine")]
pub fn check(src: &str) -> Result<(), CompileError> {
    let b = src.as_bytes();
    let mut modes: Vec<Mode> = Vec::new();
    let mut depth: u32 = 0;
    let mut chain: u32 = 0;
    let mut saved: Vec<u32> = Vec::new();
    let mut saved_sum: u32 = 0;
    let mut indents: Vec<usize> = vec![0];
    let mut line: u32 = 1;
    let mut line_start = 0usize;
    let mut at_line_start = true;
    let mut i = 0usize;
    let col = |i: usize, ls: usize| u32::try_from(i - ls).unwrap_or(u32::MAX);
    while i < b.len() {
        let c = b[i];
        match modes.last().copied() {
            Some(Mode::Str { q, triple, raw, f }) => {
                if ends_line(b, i) {
                    line += 1;
                    line_start = i + 1;
                }
                if c == b'\\' && !raw {
                    let n = line_break(b, i + 1);
                    if n > 0 {
                        line += 1;
                        line_start = i + 1 + n;
                    }
                    i += 1 + n.max(1);
                    continue;
                }
                if f && c == b'{' {
                    if b.get(i + 1) == Some(&b'{') {
                        i += 2;
                        continue;
                    }
                    if depth >= MAX_BRACKETS {
                        return Err(CompileError::syntax(
                            "too many nested parentheses".into(),
                            at(line, col(i, line_start)),
                        ));
                    }
                    modes.push(Mode::FExpr { base: depth });
                    depth += 1;
                    saved.push(chain);
                    saved_sum += chain;
                    chain = 0;
                    i += 1;
                    continue;
                }
                if f && c == b'}' && b.get(i + 1) == Some(&b'}') {
                    i += 2;
                    continue;
                }
                if c == q {
                    if !triple {
                        modes.pop();
                        i += 1;
                        continue;
                    }
                    if b.get(i + 1) == Some(&q) && b.get(i + 2) == Some(&q) {
                        modes.pop();
                        i += 3;
                        continue;
                    }
                }
                if (c == b'\n' || c == b'\r') && !triple {
                    // An unterminated string: ruff reports it.
                    modes.pop();
                }
                i += 1;
                continue;
            }
            Some(Mode::FSpec { base }) => {
                if c == b'{' {
                    modes.push(Mode::FExpr { base: depth });
                    depth += 1;
                    saved.push(chain);
                    saved_sum += chain;
                    chain = 0;
                    i += 1;
                    continue;
                }
                if c == b'}' {
                    modes.pop();
                    depth = base;
                    chain = saved.pop().map_or(0, |s| {
                        saved_sum -= s;
                        s + 1
                    });
                }
                if ends_line(b, i) {
                    line += 1;
                    line_start = i + 1;
                }
                i += 1;
                continue;
            }
            _ => {}
        }
        // Code, or an f-string replacement field.
        let in_field = matches!(modes.last(), Some(Mode::FExpr { .. }));
        if at_line_start && !in_field && depth == 0 {
            // Indentation of a logical line.
            let mut w = 0usize;
            let mut j = i;
            while j < b.len() && (b[j] == b' ' || b[j] == b'\t' || b[j] == b'\x0c') {
                w += if b[j] == b'\t' { 8 - w % 8 } else { 1 };
                j += 1;
            }
            at_line_start = false;
            let blank = j >= b.len() || matches!(b[j], b'\n' | b'\r' | b'#');
            if !blank {
                let top = indents.last().copied().unwrap_or(0);
                if w > top {
                    indents.push(w);
                    if indents.len() > MAX_INDENT {
                        return Err(CompileError::from(mm_compile::ErrorData {
                            kind: "IndentationError",
                            msg: "too many levels of indentation".into(),
                            pos: at(line, 0),
                        }));
                    }
                } else {
                    while indents.len() > 1 && indents.last().copied().unwrap_or(0) > w {
                        indents.pop();
                    }
                }
            }
            i = j;
            continue;
        }
        match c {
            b'\n' | b'\r' => {
                let n = line_break(b, i);
                line += 1;
                line_start = i + n;
                if depth == 0 {
                    at_line_start = true;
                    chain = 0;
                }
                i += n;
            }
            b'#' => {
                while i < b.len() && !matches!(b[i], b'\n' | b'\r') {
                    i += 1;
                }
            }
            b'\\' => {
                let n = line_break(b, i + 1);
                if n > 0 {
                    line += 1;
                    line_start = i + 1 + n;
                }
                i += 1 + n;
            }
            b'\'' | b'"' => {
                let triple = b.get(i + 1) == Some(&c) && b.get(i + 2) == Some(&c);
                modes.push(Mode::Str {
                    q: c,
                    triple,
                    raw: false,
                    f: false,
                });
                i += if triple { 3 } else { 1 };
            }
            b'(' | b'[' | b'{' => {
                if depth >= MAX_BRACKETS {
                    return Err(CompileError::syntax(
                        "too many nested parentheses".into(),
                        at(line, col(i, line_start)),
                    ));
                }
                depth += 1;
                saved.push(chain);
                saved_sum += chain;
                chain = 0;
                i += 1;
            }
            b')' | b']' | b'}' => {
                if let Some(Mode::FExpr { base }) = modes.last().copied()
                    && depth == base + 1
                    && c == b'}'
                {
                    modes.pop();
                }
                depth = depth.saturating_sub(1);
                chain = saved.pop().map_or(0, |s| {
                    saved_sum -= s;
                    s + 1
                });
                i += 1;
            }
            b':' => {
                if let Some(Mode::FExpr { base }) = modes.last().copied()
                    && depth == base + 1
                    && b.get(i + 1) != Some(&b'=')
                {
                    modes.pop();
                    modes.push(Mode::FSpec { base });
                }
                i += 1;
            }
            b',' | b';' => {
                chain = 0;
                i += 1;
            }
            b'=' => {
                let prev = if i > 0 { b[i - 1] } else { b' ' };
                if b.get(i + 1) == Some(&b'=') || b"=!<>:+-*/%&|^@".contains(&prev) {
                    chain += 1;
                } else {
                    chain = 0;
                }
                i += 1;
            }
            b'+' | b'-' | b'*' | b'/' | b'%' | b'@' | b'&' | b'|' | b'^' | b'~' | b'<' | b'>'
            | b'.' | b'!' => {
                chain += 1;
                i += 1;
            }
            _ if c.is_ascii_alphabetic() || c == b'_' || c >= 0x80 => {
                let start = i;
                while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_' || b[i] >= 0x80)
                {
                    i += 1;
                }
                let word = &b[start..i];
                if let Some(&q) = b.get(i)
                    && (q == b'\'' || q == b'"')
                    && let Some((raw, f)) = is_prefix(word)
                {
                    let triple = b.get(i + 1) == Some(&q) && b.get(i + 2) == Some(&q);
                    modes.push(Mode::Str { q, triple, raw, f });
                    i += if triple { 3 } else { 1 };
                    continue;
                }
                if matches!(
                    word,
                    b"not"
                        | b"and"
                        | b"or"
                        | b"in"
                        | b"is"
                        | b"if"
                        | b"else"
                        | b"lambda"
                        | b"await"
                        | b"yield"
                ) {
                    chain += 1;
                }
            }
            _ => i += 1,
        }
        if saved_sum + depth + chain > MAX_CHAIN {
            return Err(CompileError::too_deep(at(
                line,
                col(i.min(b.len()), line_start),
            )));
        }
    }
    Ok(())
}
