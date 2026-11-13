//! Compile errors.

use crate::ast::Pos;
use alloc::boxed::Box;
use alloc::string::String;
use core::ops::{Deref, DerefMut};

/// The fields of a [`CompileError`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ErrorData {
    /// The Python exception type name.
    pub kind: &'static str,
    /// The message.
    pub msg: String,
    /// Where (line 0 when unknown).
    pub pos: Pos,
}

/// A compile-time error: a `SyntaxError` (including "micro-monty does not
/// support X") or a `RecursionError` for source nested too deeply. Boxed,
/// so a `Result<T, CompileError>` costs one word on the success path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompileError(Box<ErrorData>);

impl From<ErrorData> for CompileError {
    #[cold]
    #[inline(never)]
    fn from(data: ErrorData) -> CompileError {
        CompileError(Box::new(data))
    }
}

impl Deref for CompileError {
    type Target = ErrorData;
    fn deref(&self) -> &ErrorData {
        &self.0
    }
}

impl DerefMut for CompileError {
    fn deref_mut(&mut self) -> &mut ErrorData {
        &mut self.0
    }
}

impl CompileError {
    /// A `SyntaxError`.
    #[must_use]
    pub fn syntax(msg: String, pos: Pos) -> CompileError {
        CompileError::from(ErrorData {
            kind: "SyntaxError",
            msg,
            pos,
        })
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
        CompileError::from(ErrorData {
            kind: "RecursionError",
            msg: String::from("maximum recursion depth exceeded during compilation"),
            pos,
        })
    }
}
