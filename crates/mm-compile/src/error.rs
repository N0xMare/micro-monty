//! Compile errors.

use crate::ast::Pos;
use alloc::string::String;

/// A compile-time error: a `SyntaxError` (including "micro-monty does not
/// support X") or a `RecursionError` for source nested too deeply.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompileError {
    /// The Python exception type name.
    pub kind: &'static str,
    /// The message.
    pub msg: String,
    /// Where (line 0 when unknown).
    pub pos: Pos,
}

impl CompileError {
    /// A `SyntaxError`.
    #[must_use]
    pub fn syntax(msg: String, pos: Pos) -> CompileError {
        CompileError {
            kind: "SyntaxError",
            msg,
            pos,
        }
    }

    /// "micro-monty does not support `what`".
    #[must_use]
    pub fn unsupported(what: &str, pos: Pos) -> CompileError {
        let mut msg = String::from("micro-monty does not support ");
        msg.push_str(what);
        CompileError::syntax(msg, pos)
    }

    /// Source nested too deeply for the compiler's bounded recursion.
    #[must_use]
    pub fn too_deep(pos: Pos) -> CompileError {
        CompileError {
            kind: "RecursionError",
            msg: String::from("maximum recursion depth exceeded during compilation"),
            pos,
        }
    }
}
