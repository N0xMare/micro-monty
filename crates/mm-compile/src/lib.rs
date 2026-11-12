//! The micro-monty compiler.
//!
//! An internal crate of [micro-monty](https://crates.io/crates/micro-monty),
//! published only because the facade depends on it: its API has no semver
//! guarantees and changes with every release. Depend on `micro-monty`.
//!
//! Owns the engine AST that every parser lowers into, symbol tables (local,
//! global, nonlocal, cell, free), bytecode emission, the side exception table,
//! line tables and inline-cache slot allocation. It depends on no heap: its
//! output is plain data that `mm-engine` loads into a session.
//!
//! Unsupported Python is rejected here, at compile time, with
//! "micro-monty does not support X" and a hint.
//!
//! See `docs/spec.md` §4 and §8.1.

#![no_std]
#![allow(clippy::doc_markdown, reason = "the docs name CPython in prose")]
#![allow(
    clippy::missing_errors_doc,
    reason = "every fallible function returns the one `CompileError`"
)]
#![allow(
    clippy::too_many_lines,
    reason = "statement and expression compilation read best as one match each"
)]
#![allow(
    clippy::many_single_char_names,
    clippy::similar_names,
    reason = "register operands are conventionally r, k, d, l, m"
)]
#![allow(
    clippy::struct_excessive_bools,
    reason = "symbol flags mirror CPython's DEF_* bits"
)]

/// A hash of what an image's meaning rests on in this crate's sources (the
/// code codec, and every `const` and `#[repr]` item; comments and blank
/// space left out), part of the snapshot build id
/// (`micro_monty::build_id`, docs/spec.md §9.3; see the build script).
pub const LAYOUT_HASH: u64 = include!(concat!(env!("OUT_DIR"), "/layout_hash.rs"));

extern crate alloc;

pub mod ast;
pub mod bytecode;
mod compiler;
mod encode;
mod error;
pub mod pack;
pub mod symtable;

pub use compiler::{MAX_NESTING, Options, compile, match_kind};
pub use encode::{DecodeError, SharedStrings, section};
pub use error::CompileError;

/// The lines of source `text` as positions number them: each `\n`,
/// `\r\n` or lone `\r` ends one (CPython translates line endings before
/// tokenizing). Like `str::split`, a final line break leaves an empty
/// last line.
pub fn source_lines(text: &str) -> impl Iterator<Item = &str> {
    let mut rest = Some(text);
    core::iter::from_fn(move || {
        let t = rest?;
        let Some(i) = t.find(['\n', '\r']) else {
            rest = None;
            return Some(t);
        };
        let n = if t[i..].starts_with("\r\n") { 2 } else { 1 };
        rest = t.get(i + n..);
        t.get(..i)
    })
}
