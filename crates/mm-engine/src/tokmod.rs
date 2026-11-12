//! `tokenize._tokens(source)`: CPython's `tokenize` token stream over the
//! own lexer ([`mm_parse::tokens`]), as `(type, string, (row, col), (row,
//! col))` tuples with character columns, and the error that ended it as
//! `(kind, msg, lineno, offset)` or None. One fuel unit per token.

use crate::types::T;
use crate::vm::{CallArgs, Raised, Vm};
use alloc::{format, string::String, vec::Vec};
use mm_core::Value;
use mm_parse::lines::Lines;

/// The longest source `tokenize._tokens` takes, in UTF-8 bytes: the
/// lexer and the token tuples are one native step, and the worst shape at
/// this size (a token a byte) takes about 22 ms [M] (docs/spec.md §5).
/// Longer sources raise `MemoryError`.
const MAX_SOURCE: usize = 64 * 1024;

/// `tokenize._tokens(source)`.
pub(crate) fn tokens(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let src = vm.arg_o(&a, 0);
    if !Vm::is_str(src) {
        return Err(vm.err(T::TypeError, "expected str source"));
    }
    if vm.str_bytes(src).len() > MAX_SOURCE {
        return Err(vm.err(
            T::MemoryError,
            &format!("tokenize source larger than {} KiB", MAX_SOURCE / 1024),
        ));
    }
    // The copy, the lexer's and this stream's tokens, the line table and
    // the token values, before any of it reaches the arena: up to about 56
    // bytes per source byte [M] (one-character comment lines).
    let res = vm
        .heap
        .reserve_scratch(vm.str_bytes(src).len().saturating_mul(64));
    let _native = vm.hx_o(res)?;
    let text = String::from(vm.str_ref_o(src));
    let (toks, err) = mm_parse::tokens(&text);
    let lines = Lines::new(&text);
    let n = text.len();
    // CPython's DEDENT and ENDMARKER after a last line without a newline
    // sit at the start of the line after it.
    let open_end = n > 0 && !text.ends_with('\n');
    let after = lines.loc(u32::try_from(n).unwrap_or(u32::MAX)).0 + 1;
    let mut items = Vec::with_capacity(toks.len());
    for (k, s, e) in toks {
        vm.burn(1);
        let (mut sl, mut sc) = lines.loc(s);
        // A NEWLINE or NL ends on its line.
        let (mut el, mut ec) = if e > s && matches!(k, 4 | 66) {
            let (l, c) = lines.loc(e - 1);
            (l, c + 1)
        } else {
            lines.loc(e)
        };
        // The implicit NEWLINE or NL at the end is one column wide.
        if s == e && matches!(k, 4 | 66) {
            ec = sc + 1;
        }
        if open_end && s as usize >= n && matches!(k, 0 | 6) {
            (sl, sc, el, ec) = (after, 0, after, 0);
        }
        let string = vm.new_str(text.get(s as usize..e as usize).unwrap_or(""))?;
        let start = pair(vm, sl, sc)?;
        let end = pair(vm, el, ec)?;
        items.push(vm.alloc_tuple(&[
            Value::int(i64::from(k)).unwrap_or(Value::NONE),
            string,
            start,
            end,
        ])?);
    }
    let list = vm.new_list(&items)?;
    let err = match err {
        Some(e) => {
            let kind = vm.new_str(e.kind)?;
            let msg = vm.new_str(&e.msg)?;
            let line = pair(vm, e.pos.line, e.pos.col + 1)?;
            vm.alloc_tuple(&[kind, msg, line])?
        }
        None => Value::NONE,
    };
    vm.alloc_tuple(&[list, err])
}

fn pair(vm: &mut Vm, a: u32, b: u32) -> Result<Value, Raised> {
    let a = vm.new_int(i64::from(a))?;
    let b = vm.new_int(i64::from(b))?;
    vm.alloc_tuple(&[a, b])
}
