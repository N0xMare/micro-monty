//! The tokenizer: source bytes to a token vector, with CPython's
//! indentation, bracket and PEP 701 f-string rules.
//!
//! Iterative throughout; nesting state lives in vectors. The first error
//! ends the stream with a [`T::Error`] token and is kept in
//! [`Lexed::err`], so the parser reports it only if it gets that far.

use alloc::{format, string::String, vec::Vec};
use mm_compile::{CompileError, ast::Pos};

use crate::lines::Lines;

/// Token kinds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum T {
    Name,
    Int,
    Float,
    Complex,
    /// A complete (non-f) string literal, prefix and quotes included.
    Str,
    /// `f"` / `t"` (prefix and opening quotes).
    FStart,
    /// Literal text inside an f-string.
    FMiddle,
    /// The closing quotes of an f-string.
    FEnd,
    Newline,
    Indent,
    Dedent,
    End,
    Error,
    // Brackets.
    LPar,
    RPar,
    LSqb,
    RSqb,
    LBrace,
    RBrace,
    // Operators and delimiters.
    Colon,
    Comma,
    Semi,
    Plus,
    Minus,
    Star,
    Slash,
    Vbar,
    Amper,
    Less,
    Greater,
    Equal,
    Dot,
    Percent,
    EqEqual,
    NotEqual,
    LessEqual,
    GreaterEqual,
    Tilde,
    Circumflex,
    LeftShift,
    RightShift,
    DoubleStar,
    DoubleSlash,
    At,
    Rarrow,
    Ellipsis,
    ColonEqual,
    Exclamation,
    PlusEqual,
    MinusEqual,
    StarEqual,
    SlashEqual,
    PercentEqual,
    AmperEqual,
    VbarEqual,
    CircumflexEqual,
    LeftShiftEqual,
    RightShiftEqual,
    DoubleStarEqual,
    DoubleSlashEqual,
    AtEqual,
    /// `$`, `?` or a backtick: an operator token (CPython's tokenizer makes
    /// any other printable character an `OP`) that no grammar rule accepts,
    /// so only the parser rejects it, where it reads it.
    Unknown,
    // Keywords.
    False,
    None,
    True,
    And,
    As,
    Assert,
    Async,
    Await,
    Break,
    Class,
    Continue,
    Def,
    Del,
    Elif,
    Else,
    Except,
    Finally,
    For,
    From,
    Global,
    If,
    Import,
    In,
    Is,
    Lambda,
    Nonlocal,
    Not,
    Or,
    Pass,
    Raise,
    Return,
    Try,
    While,
    With,
    Yield,
}

/// String flags (`Tok::f`).
pub(crate) const F_RAW: u8 = 1;
pub(crate) const F_BYTES: u8 = 2;
pub(crate) const F_F: u8 = 4;
pub(crate) const F_T: u8 = 8;
pub(crate) const F_TRIPLE: u8 = 16;
pub(crate) const F_DOUBLE: u8 = 32;
/// On a `Name`: contains non-ASCII characters.
pub(crate) const F_NONASCII: u8 = 64;

/// A token: kind, byte range and flags.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Tok {
    pub k: T,
    pub s: u32,
    pub e: u32,
    pub f: u8,
}

/// The token stream.
pub(crate) struct Lexed {
    pub toks: Vec<Tok>,
    pub err: Option<CompileError>,
    /// How CPython's full-source scan after a parser error treats `err`
    /// (`pegen_errors.c`): see [`Late`].
    pub late: Late,
}

/// A tokenizer error the parser never fetched, as CPython's scan of the
/// rest of the source after a parser error sees it.
pub(crate) enum Late {
    /// Raised at once by the tokenizer (`syntaxerror()`): it replaces the
    /// parser's error. Also any error inside an f-string, where mm's
    /// replacement-field errors do not follow CPython's error pass.
    Raised,
    /// A deferred `done` code (`E_LINECONT`, `E_EOF`): the parser's error
    /// stands, unless a bracket is open, opened on a line before the last
    /// token the parser read; then that bracket "was never closed" (this
    /// error).
    Deferred(Option<CompileError>),
}

fn keyword(w: &[u8]) -> Option<T> {
    Some(match w {
        b"False" => T::False,
        b"None" => T::None,
        b"True" => T::True,
        b"and" => T::And,
        b"as" => T::As,
        b"assert" => T::Assert,
        b"async" => T::Async,
        b"await" => T::Await,
        b"break" => T::Break,
        b"class" => T::Class,
        b"continue" => T::Continue,
        b"def" => T::Def,
        b"del" => T::Del,
        b"elif" => T::Elif,
        b"else" => T::Else,
        b"except" => T::Except,
        b"finally" => T::Finally,
        b"for" => T::For,
        b"from" => T::From,
        b"global" => T::Global,
        b"if" => T::If,
        b"import" => T::Import,
        b"in" => T::In,
        b"is" => T::Is,
        b"lambda" => T::Lambda,
        b"nonlocal" => T::Nonlocal,
        b"not" => T::Not,
        b"or" => T::Or,
        b"pass" => T::Pass,
        b"raise" => T::Raise,
        b"return" => T::Return,
        b"try" => T::Try,
        b"while" => T::While,
        b"with" => T::With,
        b"yield" => T::Yield,
        _ => return None,
    })
}

/// String prefix flags for an identifier directly followed by a quote.
fn prefix(w: &[u8]) -> Option<u8> {
    let mut f = 0u8;
    if w.is_empty() || w.len() > 2 {
        return None;
    }
    for &c in w {
        let bit = match c.to_ascii_lowercase() {
            b'r' => F_RAW,
            b'b' => F_BYTES,
            b'f' => F_F,
            b't' => F_T,
            b'u' if w.len() == 1 => 0,
            _ => return None,
        };
        if f & bit != 0 {
            return None;
        }
        f |= bit;
    }
    // b, f and t exclude each other.
    if (f & (F_BYTES | F_F | F_T)).count_ones() > 1 {
        return None;
    }
    Some(f)
}

/// NFKC for the compatibility characters likely in identifiers: fullwidth
/// ASCII, mathematical Latin letters and digits, Latin ligatures, `ſ`, `ª`,
/// `º`, `µ`. Full NFKC needs about 70 KB of tables micro-monty does not
/// carry (docs/spec.md §6); names with other characters NFKC would change
/// are rejected by [`needs_nfkc`] rather than silently left unnormalized.
pub(crate) fn nfkc_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        let u = c as u32;
        match u {
            0xFF10..=0xFF19 | 0xFF21..=0xFF3A | 0xFF41..=0xFF5A | 0xFF3F => {
                out.push(char::from_u32(u - 0xFF00 + 0x20).unwrap_or(c));
            }
            0x1D400..=0x1D6A3 => {
                let k = (u - 0x1D400) % 52;
                let ch = if k < 26 { 0x41 + k } else { 0x61 + k - 26 };
                out.push(char::from_u32(ch).unwrap_or(c));
            }
            0x1D7CE..=0x1D7FF => out.push(char::from_u32(0x30 + (u - 0x1D7CE) % 10).unwrap_or(c)),
            0xFB00 => out.push_str("ff"),
            0xFB01 => out.push_str("fi"),
            0xFB02 => out.push_str("fl"),
            0xFB03 => out.push_str("ffi"),
            0xFB04 => out.push_str("ffl"),
            0xFB05 | 0xFB06 => out.push_str("st"),
            0x017F => out.push('s'),
            0x00AA => out.push('a'),
            0x00BA => out.push('o'),
            0x00B5 => out.push('\u{03BC}'),
            _ => out.push(c),
        }
    }
    out
}

/// Is `c` a character NFKC changes that [`nfkc_name`] does not map
/// (compatibility and singleton decompositions, by block, plus combining
/// marks and conjoining jamo that compose with what precedes them)?
pub(crate) fn needs_nfkc(c: char) -> bool {
    let u = c as u32;
    let mapped = matches!(
        u,
        0xFF10..=0xFF19
            | 0xFF21..=0xFF3A
            | 0xFF41..=0xFF5A
            | 0xFF3F
            | 0x1D400..=0x1D6A3
            | 0x1D7CE..=0x1D7FF
            | 0xFB00..=0xFB06
            | 0x017F
            | 0x00AA
            | 0x00BA
            | 0x00B5
    );
    !mapped
        && matches!(
            u,
            0x0132..=0x0133
                | 0x013F..=0x0140
                | 0x0149
                | 0x01C4..=0x01CC
                | 0x01F1..=0x01F3
                | 0x02B0..=0x02B8
                | 0x02E0..=0x02E4
                | 0x0300..=0x036F
                | 0x037A
                | 0x03D0..=0x03D6
                | 0x03F0..=0x03F2
                | 0x03F4..=0x03F5
                | 0x03F9
                | 0x0587
                | 0x0675..=0x0678
                | 0x0E33
                | 0x0EB3
                | 0x0EDC..=0x0EDD
                | 0x0F77
                | 0x0F79
                | 0x10FC
                | 0x1100..=0x11FF
                | 0x1D2C..=0x1D6A
                | 0x1D78
                | 0x1D9B..=0x1DBF
                | 0x1E9A..=0x1E9B
                | 0x1F71
                | 0x1F73
                | 0x1F75
                | 0x1F77
                | 0x1F79
                | 0x1F7B
                | 0x1F7D
                | 0x1FBB
                | 0x1FBE
                | 0x1FC9
                | 0x1FCB
                | 0x1FD3
                | 0x1FDB
                | 0x1FE3
                | 0x1FEB
                | 0x1FF9
                | 0x1FFB
                | 0x2071
                | 0x207F
                | 0x2090..=0x209C
                | 0x2102..=0x2149
                | 0x2160..=0x2188
                | 0x2C7C..=0x2C7D
                | 0x2D6F
                | 0x2E9F
                | 0x2EF3
                | 0x2F00..=0x2FD5
                | 0x3038..=0x303A
                | 0x309B..=0x309C
                | 0x309F
                | 0x30FF
                | 0x3131..=0x318E
                | 0xA69C..=0xA69D
                | 0xA770
                | 0xA7F2..=0xA7F4
                | 0xA7F8..=0xA7F9
                | 0xAB5C..=0xAB5F
                | 0xAB69
                | 0xF900..=0xFAD9
                | 0xFB13..=0xFB17
                | 0xFB1D..=0xFDFF
                | 0xFE70..=0xFEFC
                | 0xFF00..=0xFFEF
                | 0x1D6A4..=0x1D7CD
                | 0x1E030..=0x1E06D
                | 0x1EE00..=0x1EEFF
                | 0x2F800..=0x2FA1D
        )
}

/// Is `c` (non-ASCII) a character an identifier may start with?
pub(crate) fn id_start(c: char) -> bool {
    c.is_alphabetic() || matches!(c, '\u{2118}' | '\u{212E}' | '\u{309B}' | '\u{309C}')
}

/// Is `c` (non-ASCII) a character an identifier may continue with?
/// `XID_Continue` approximated without tables: letters and numbers, the
/// `Other_ID_Continue` characters, and the combining marks of the blocks
/// that use them (diacritics, Hebrew and Arabic points, Indic and Southeast
/// Asian vowel signs and viramas).
pub(crate) fn id_continue(c: char) -> bool {
    id_start(c)
        || c.is_numeric()
        || matches!(
            c,
            '\u{00B7}' | '\u{0387}' | '\u{19DA}' | '\u{200C}' | '\u{200D}'
        )
        || (!c.is_whitespace()
            && matches!(
                c as u32,
                0x0300..=0x036F
                    | 0x0483..=0x0487
                    | 0x0591..=0x05C7
                    | 0x0610..=0x061A
                    | 0x064B..=0x065F
                    | 0x0670
                    | 0x06D6..=0x06ED
                    | 0x0900..=0x0963
                    | 0x0981..=0x0DFF
                    | 0x0E31..=0x0E4E
                    | 0x0EB1..=0x0ECD
                    | 0x0F18..=0x0F19
                    | 0x0F35..=0x0F39
                    | 0x0F71..=0x0F84
                    | 0x102B..=0x103E
                    | 0x1056..=0x109D
                    | 0x17B4..=0x17D3
                    | 0x1AB0..=0x1AFF
                    | 0x1DC0..=0x1DFF
                    | 0x20D0..=0x20F0
                    | 0xFE00..=0xFE0F
                    | 0xFE20..=0xFE2F
            ))
}

/// CPython's `str.isprintable` for characters the tokenizer rejects.
fn printable(c: char) -> bool {
    !(c.is_control()
        || c.is_whitespace()
        || matches!(c as u32, 0xAD | 0x200B..=0x200F | 0x202A..=0x202E | 0x2060..=0x206F | 0xFEFF | 0xFFF9..=0xFFFB)
        || (0xE000..=0xF8FF).contains(&(c as u32))
        || (0xF0000..).contains(&(c as u32)))
}

/// An open f-string.
struct FStr {
    quote: u8,
    triple: bool,
    raw: bool,
    t: bool,
    /// Bracket depth when it opened.
    base: usize,
    /// Format specs currently open inside it.
    specs: usize,
    start: usize,
}

struct Lx<'a, 's> {
    src: &'s str,
    b: &'s [u8],
    lines: &'a Lines<'s>,
    i: usize,
    toks: Vec<Tok>,
    /// Indentation stack: (columns with tabs to 8, columns with tabs to 1).
    indents: Vec<(u32, u32)>,
    /// Open brackets: (byte, offset).
    brackets: Vec<(u8, usize)>,
    fstrings: Vec<FStr>,
    at_bol: bool,
}

type LR = Result<(), CompileError>;

fn at(lines: &Lines<'_>, start: usize, end: usize) -> Pos {
    lines.pos(
        u32::try_from(start).unwrap_or(u32::MAX),
        u32::try_from(end).unwrap_or(u32::MAX),
    )
}

/// Tokenizes `src` (a leading BOM is skipped).
pub(crate) fn lex(src: &str, lines: &Lines<'_>) -> Lexed {
    let mut lx = Lx {
        src,
        b: src.as_bytes(),
        lines,
        i: 0,
        toks: Vec::with_capacity(src.len() / 4 + 8),
        indents: alloc::vec![(0, 0)],
        brackets: Vec::new(),
        fstrings: Vec::new(),
        at_bol: true,
    };
    if src.starts_with('\u{FEFF}') {
        lx.i = 3;
    }
    let err = lx.run().err();
    let late =
        match &err {
            _ if !lx.fstrings.is_empty() => Late::Raised,
            // `E_DEDENT` and `E_TABSPACE` are deferred too, but come at a
            // line's start, where the parser always fetches them.
            Some(e)
                if e.msg.ends_with("was never closed")
                    || matches!(
                        &*e.msg,
                        "unexpected character after line continuation character"
                            | "unexpected EOF while parsing"
                    ) =>
            {
                Late::Deferred(lx.brackets.last().map(|&(c, off)| {
                    lx.syntax(format!("'{}' was never closed", c as char), off, off)
                }))
            }
            _ => Late::Raised,
        };
    if err.is_some() {
        let p = u32::try_from(lx.i.min(src.len())).unwrap_or(u32::MAX);
        lx.toks.push(Tok {
            k: T::Error,
            s: p,
            e: p,
            f: 0,
        });
    }
    Lexed {
        toks: lx.toks,
        err,
        late,
    }
}

impl Lx<'_, '_> {
    fn push(&mut self, k: T, s: usize, e: usize, f: u8) {
        self.toks.push(Tok {
            k,
            s: u32::try_from(s).unwrap_or(u32::MAX),
            e: u32::try_from(e).unwrap_or(u32::MAX),
            f,
        });
    }

    fn peek(&self, n: usize) -> u8 {
        self.b.get(self.i + n).copied().unwrap_or(0)
    }

    fn err(&self, kind: &'static str, msg: String, start: usize, end: usize) -> CompileError {
        CompileError::from(mm_compile::ErrorData {
            kind,
            msg,
            pos: at(self.lines, start, end),
        })
    }

    fn syntax(&self, msg: String, start: usize, end: usize) -> CompileError {
        self.err("SyntaxError", msg, start, end)
    }

    /// Opens a bracket (or a replacement field) at `at`. At most CPython's
    /// `MAXLEVEL` are open at once: this, not the pre-scan, is what bounds
    /// the parser's recursion through brackets, so a pre-scan that reads the
    /// source differently (a comment ended by a lone `\r`, `\'` in a raw
    /// string, `\{` in an f-string) cannot let deeper nesting through.
    fn open(&mut self, c: u8, at: usize) -> LR {
        if self.brackets.len() >= crate::prescan::MAX_BRACKETS as usize {
            return Err(self.syntax("too many nested parentheses".into(), at, at + 1));
        }
        self.brackets.push((c, at));
        Ok(())
    }

    fn line_no(&self, off: usize) -> usize {
        self.lines.line_of(u32::try_from(off).unwrap_or(u32::MAX))
    }

    fn run(&mut self) -> LR {
        loop {
            if let Some(fs) = self.fstrings.last() {
                let depth = self.brackets.len();
                if depth == fs.base {
                    self.fmiddle(false)?;
                    continue;
                }
                if fs.specs > 0 && depth == fs.base + fs.specs {
                    self.fmiddle(true)?;
                    continue;
                }
            }
            if self.at_bol && self.brackets.is_empty() {
                self.at_bol = false;
                if self.indentation()? {
                    continue;
                }
            }
            // Whitespace and line continuations.
            loop {
                match self.peek(0) {
                    b' ' | b'\t' | b'\x0c' => self.i += 1,
                    b'\\' => self.continuation()?,
                    _ => break,
                }
            }
            if self.i >= self.b.len() {
                return self.end();
            }
            let c = self.b[self.i];
            let s = self.i;
            match c {
                b'\n' | b'\r' => {
                    self.i += if c == b'\r' && self.peek(1) == b'\n' {
                        2
                    } else {
                        1
                    };
                    if self.brackets.is_empty() {
                        let last = self.toks.last().map(|t| t.k);
                        if !matches!(last, None | Some(T::Newline | T::Indent | T::Dedent)) {
                            self.push(T::Newline, s, self.i, 0);
                        }
                        self.at_bol = true;
                    }
                }
                b'#' => {
                    while self.i < self.b.len() && !matches!(self.b[self.i], b'\n' | b'\r') {
                        self.i += 1;
                    }
                }
                b'0'..=b'9' => self.number()?,
                b'.' if self.peek(1).is_ascii_digit() => self.number()?,
                b'\'' | b'"' => self.string(s, 0)?,
                _ if c.is_ascii_alphabetic() || c == b'_' || c >= 0x80 => self.name()?,
                _ => self.operator()?,
            }
        }
    }

    /// A `\` outside a string: must end the line.
    fn continuation(&mut self) -> LR {
        let s = self.i;
        self.i += 1;
        match self.peek(0) {
            _ if self.i >= self.b.len() => {
                return Err(self.syntax("unexpected EOF while parsing".into(), self.i, self.i));
            }
            b'\n' => self.i += 1,
            b'\r' => {
                self.i += if self.peek(1) == b'\n' { 2 } else { 1 };
            }
            _ => {
                return Err(self.syntax(
                    "unexpected character after line continuation character".into(),
                    s + 1,
                    s + 1,
                ));
            }
        }
        if self.i >= self.b.len() {
            return Err(self.syntax("unexpected EOF while parsing".into(), self.i, self.i));
        }
        Ok(())
    }

    /// Handles a logical line's indentation; `true` if the line was blank.
    #[allow(clippy::comparison_chain, reason = "CPython's three indentation cases")]
    fn indentation(&mut self) -> Result<bool, CompileError> {
        let line_start = self.i;
        let mut col = 0u32;
        let mut alt = 0u32;
        loop {
            match self.peek(0) {
                b' ' => {
                    col += 1;
                    alt += 1;
                }
                b'\t' => {
                    col = (col / 8 + 1) * 8;
                    alt += 1;
                }
                b'\x0c' => {
                    col = 0;
                    alt = 0;
                }
                b'\\' if self.peek(1) == b'\n' || self.peek(1) == b'\r' => {
                    self.continuation()?;
                    continue;
                }
                _ => break,
            }
            self.i += 1;
        }
        if self.i >= self.b.len() || matches!(self.b[self.i], b'#' | b'\n' | b'\r') {
            // A blank line or a comment: no indentation change.
            return Ok(self.i < self.b.len());
        }
        let (top, top_alt) = self.indents.last().copied().unwrap_or((0, 0));
        let s = self.i;
        if col == top {
            if alt != top_alt {
                return Err(self.tab_error(s));
            }
        } else if col > top {
            if alt <= top_alt {
                return Err(self.tab_error(s));
            }
            self.indents.push((col, alt));
            self.push(T::Indent, line_start, s, 0);
        } else {
            while self.indents.len() > 1 && self.indents.last().is_some_and(|l| l.0 > col) {
                self.indents.pop();
                self.push(T::Dedent, s, s, 0);
            }
            let (top, top_alt) = self.indents.last().copied().unwrap_or((0, 0));
            if col != top {
                let line = self.line_no(s);
                let len = self.lines.text(line).chars().count();
                let p = self.lines.offset(
                    u32::try_from(line).unwrap_or(u32::MAX),
                    u32::try_from(len).unwrap_or(u32::MAX),
                );
                return Err(self.err(
                    "IndentationError",
                    "unindent does not match any outer indentation level".into(),
                    p,
                    p,
                ));
            }
            if alt != top_alt {
                return Err(self.tab_error(s));
            }
        }
        Ok(false)
    }

    fn tab_error(&self, s: usize) -> CompileError {
        let line = self.line_no(s);
        let p = self
            .lines
            .offset(u32::try_from(line).unwrap_or(u32::MAX), 0);
        self.err(
            "TabError",
            "inconsistent use of tabs and spaces in indentation".into(),
            p,
            p,
        )
    }

    fn end(&mut self) -> LR {
        if let Some(fs) = self.fstrings.last() {
            let kind = if fs.t { "t-string" } else { "f-string" };
            let s = fs.start;
            // Inside a replacement field.
            return Err(self.syntax(format!("{kind}: expecting '}}'"), s, s));
        }
        if let Some(&(c, off)) = self.brackets.last() {
            return Err(self.syntax(format!("'{}' was never closed", c as char), off, off));
        }
        let n = self.b.len();
        let last = self.toks.last().map(|t| t.k);
        if !matches!(last, None | Some(T::Newline | T::Indent | T::Dedent)) {
            self.push(T::Newline, n, n, 0);
        }
        while self.indents.len() > 1 {
            self.indents.pop();
            self.push(T::Dedent, n, n, 0);
        }
        self.push(T::End, n, n, 0);
        Ok(())
    }

    fn name(&mut self) -> LR {
        let s = self.i;
        let mut nonascii = false;
        while self.i < self.b.len() {
            let c = self.b[self.i];
            if c.is_ascii_alphanumeric() || c == b'_' {
                self.i += 1;
            } else if c >= 0x80 {
                let ch = self.src[self.i..].chars().next().unwrap_or('\0');
                let ok = if self.i == s {
                    id_start(ch)
                } else {
                    id_continue(ch)
                };
                if !ok {
                    break;
                }
                nonascii = true;
                self.i += ch.len_utf8();
            } else {
                break;
            }
        }
        if self.i == s {
            // A non-ASCII character that cannot start a name.
            let ch = self.src[s..].chars().next().unwrap_or('\0');
            let msg = if printable(ch) {
                format!("invalid character '{ch}' (U+{:04X})", ch as u32)
            } else {
                format!("invalid non-printable character U+{:04X}", ch as u32)
            };
            return Err(self.syntax(msg, s, s));
        }
        let w = &self.b[s..self.i];
        let after_bang = self.toks.last().is_some_and(|t| t.k == T::Exclamation)
            && self.fstrings.last().is_some();
        if matches!(self.b.get(self.i), Some(b'\'' | b'"'))
            && !after_bang
            && let Some(f) = prefix(w)
        {
            return self.string(s, f);
        }
        if nonascii && let Some(c) = self.src[s..self.i].chars().find(|&c| needs_nfkc(c)) {
            return Err(self.syntax(
                format!(
                    "micro-monty does not support identifiers that need NFKC normalization ('{c}', U+{:04X})",
                    c as u32
                ),
                s,
                self.i,
            ));
        }
        if let Some(k) = keyword(w) {
            self.push(k, s, self.i, 0);
        } else {
            self.push(T::Name, s, self.i, if nonascii { F_NONASCII } else { 0 });
        }
        // A non-ASCII character that cannot continue the name.
        if self.i < self.b.len() && self.b[self.i] >= 0x80 {
            let ch = self.src[self.i..].chars().next().unwrap_or('\0');
            if !id_continue(ch) && !ch.is_whitespace() {
                let msg = if printable(ch) {
                    format!("invalid character '{ch}' (U+{:04X})", ch as u32)
                } else {
                    format!("invalid non-printable character U+{:04X}", ch as u32)
                };
                return Err(self.syntax(msg, self.i, self.i));
            }
        }
        Ok(())
    }

    /// Digits of `radix` with single `_` separators; returns whether any
    /// digit was seen.
    fn digits(&mut self, radix: u32) -> Result<bool, CompileError> {
        let mut any = false;
        loop {
            let c = self.peek(0);
            if (c as char).is_digit(radix) {
                any = true;
                self.i += 1;
            } else if c == b'_' && any {
                if (self.peek(1) as char).is_digit(radix) {
                    self.i += 1;
                } else {
                    return Err(self.bad_number(radix));
                }
            } else {
                return Ok(any);
            }
        }
    }

    fn bad_number(&self, radix: u32) -> CompileError {
        let what = match radix {
            16 => "hexadecimal",
            8 => "octal",
            2 => "binary",
            _ => "decimal",
        };
        let p = self.i;
        self.syntax(format!("invalid {what} literal"), p, p)
    }

    fn number(&mut self) -> LR {
        let s = self.i;
        let first = self.b[s];
        let lower = self.peek(1).to_ascii_lowercase();
        if first == b'0' && matches!(lower, b'x' | b'o' | b'b') {
            let radix = match lower {
                b'x' => 16,
                b'o' => 8,
                _ => 2,
            };
            self.i += 2;
            if self.peek(0) == b'_' {
                self.i += 1;
            }
            let any = self.digits(radix)?;
            let c = self.peek(0);
            if c.is_ascii_digit() && radix < 10 {
                let what = if radix == 8 { "octal" } else { "binary" };
                return Err(self.syntax(
                    format!("invalid digit '{}' in {what} literal", c as char),
                    self.i,
                    self.i,
                ));
            }
            if !any {
                self.i -= 1;
                return Err(self.bad_number(radix));
            }
            self.after_number(radix)?;
            self.push(T::Int, s, self.i, 0);
            return Ok(());
        }
        let mut float = false;
        if first == b'.' {
            float = true;
            self.i += 1;
            self.digits(10)?;
        } else {
            self.digits(10)?;
            if self.peek(0) == b'.' {
                float = true;
                self.i += 1;
                if self.peek(0) == b'_' {
                    return Err(self.bad_number(10));
                }
                self.digits(10)?;
            }
        }
        if matches!(self.peek(0), b'e' | b'E') {
            let save = self.i;
            self.i += 1;
            if matches!(self.peek(0), b'+' | b'-') {
                self.i += 1;
            }
            if self.peek(0).is_ascii_digit() {
                self.digits(10)?;
                float = true;
            } else {
                self.i = save;
                if self.after_number(10).is_err() {
                    let p = save - 1;
                    return Err(self.syntax("invalid decimal literal".into(), p, p));
                }
            }
        }
        if matches!(self.peek(0), b'j' | b'J') {
            self.i += 1;
            if self.ident_follows() {
                let p = self.i - 1;
                return Err(self.syntax("invalid imaginary literal".into(), p, p));
            }
            self.push(T::Complex, s, self.i, 0);
            return Ok(());
        }
        if !float && first == b'0' {
            let text = &self.b[s..self.i];
            if text.iter().any(|&c| c != b'0' && c != b'_') {
                return Err(self.syntax(
                    "leading zeros in decimal integer literals are not permitted; use an 0o prefix for octal integers".into(),
                    s,
                    s + 1,
                ));
            }
        }
        self.after_number(10)?;
        self.push(if float { T::Float } else { T::Int }, s, self.i, 0);
        Ok(())
    }

    /// An identifier character straight after a number is an error, except
    /// for the keywords CPython still accepts there (`1if`, `1or`, ...).
    fn ident_follows(&self) -> bool {
        let c = self.peek(0);
        c.is_ascii_alphanumeric() || c == b'_' || c >= 0x80
    }

    fn after_number(&mut self, radix: u32) -> LR {
        if !self.ident_follows() {
            return Ok(());
        }
        let rest = &self.b[self.i..];
        for kw in [
            &b"and"[..],
            b"else",
            b"for",
            b"if",
            b"in",
            b"is",
            b"not",
            b"or",
        ] {
            if rest.starts_with(kw) {
                let after = rest.get(kw.len()).copied().unwrap_or(b' ');
                if !(after.is_ascii_alphanumeric() || after == b'_' || after >= 0x80) {
                    return Ok(());
                }
            }
        }
        if self.peek(0) == b'_' || self.peek(0).is_ascii_digit() {
            return Err(self.bad_number(radix));
        }
        let what = match radix {
            16 => "hexadecimal",
            8 => "octal",
            2 => "binary",
            _ => "decimal",
        };
        let p = self.i - 1;
        Err(self.syntax(format!("invalid {what} literal"), p, p))
    }

    /// A string literal starting at `s` (prefix included); the cursor is at
    /// the opening quote.
    fn string(&mut self, s: usize, mut f: u8) -> LR {
        let q = self.b[self.i];
        let triple = self.peek(1) == q && self.peek(2) == q;
        if q == b'"' {
            f |= F_DOUBLE;
        }
        if triple {
            f |= F_TRIPLE;
        }
        self.i += if triple { 3 } else { 1 };
        if f & (F_F | F_T) != 0 {
            self.push(T::FStart, s, self.i, f);
            self.fstrings.push(FStr {
                quote: q,
                triple,
                raw: f & F_RAW != 0,
                t: f & F_T != 0,
                base: self.brackets.len(),
                specs: 0,
                start: s,
            });
            return Ok(());
        }
        loop {
            if self.i >= self.b.len() {
                return Err(self.unterminated_in_field(s, triple));
            }
            let c = self.b[self.i];
            if c == b'\\' {
                self.i += if self.peek(1) == b'\r' && self.peek(2) == b'\n' {
                    3
                } else {
                    2
                };
                continue;
            }
            if c == q && (!triple || (self.peek(1) == q && self.peek(2) == q)) {
                self.i += if triple { 3 } else { 1 };
                break;
            }
            if !triple && (c == b'\n' || c == b'\r') {
                return Err(self.unterminated_in_field(s, triple));
            }
            self.i += 1;
        }
        self.i = self.i.min(self.b.len());
        self.push(T::Str, s, self.i, f);
        Ok(())
    }

    fn unterminated(&self, s: usize, triple: bool, kind: &str) -> CompileError {
        let at = if triple || self.i >= self.b.len() {
            self.b.len().saturating_sub(usize::from(
                self.b.ends_with(b"\n") || self.b.ends_with(b"\r"),
            ))
        } else {
            self.i
        };
        let detected = self.line_no(at);
        let what = if triple { "triple-quoted " } else { "" };
        let noun = if kind.is_empty() { "string" } else { kind };
        self.syntax(
            format!("unterminated {what}{noun} literal (detected at line {detected})"),
            s,
            s,
        )
    }

    /// An unterminated string; inside a replacement field, a quote that
    /// matches the f-string's own is CPython's "expecting '}'".
    fn unterminated_in_field(&self, s: usize, triple: bool) -> CompileError {
        if let Some(fs) = self.fstrings.last()
            && self.brackets.len() > fs.base
            && self.b.get(s) == Some(&fs.quote)
            && (fs.triple || !triple)
        {
            let kind = if fs.t { "t-string" } else { "f-string" };
            return self.syntax(format!("{kind}: expecting '}}'"), s, s);
        }
        self.unterminated(s, triple, "")
    }

    /// Literal text of the innermost f-string, up to a replacement field,
    /// the end of a format spec or the closing quote.
    #[allow(clippy::too_many_lines, reason = "one f-string literal state machine")]
    fn fmiddle(&mut self, spec: bool) -> LR {
        let Some(fs) = self.fstrings.last() else {
            return Ok(());
        };
        let (q, triple, raw, t, start) = (fs.quote, fs.triple, fs.raw, fs.t, fs.start);
        let kind = if t { "t-string" } else { "f-string" };
        let s = self.i;
        loop {
            if self.i >= self.b.len() {
                if spec {
                    return Err(self.syntax(format!("{kind}: expecting '}}'"), start, start));
                }
                return Err(self.unterminated(start, triple, kind));
            }
            let c = self.b[self.i];
            if c == q && (!triple || (self.peek(1) == q && self.peek(2) == q)) {
                if spec {
                    return Err(self.syntax(
                        format!("{kind}: expecting '}}', or format specs"),
                        self.i,
                        self.i,
                    ));
                }
                if self.i > s {
                    self.push(T::FMiddle, s, self.i, 0);
                }
                let e = self.i + if triple { 3 } else { 1 };
                self.push(T::FEnd, self.i, e, 0);
                self.i = e;
                self.fstrings.pop();
                return Ok(());
            }
            match c {
                b'\n' | b'\r' if !triple => {
                    if spec {
                        return Err(self.syntax(
                            format!("{kind}: newlines are not allowed in format specifiers for single quoted {kind}s"),
                            self.i,
                            self.i,
                        ));
                    }
                    return Err(self.unterminated(start, triple, kind));
                }
                b'\\' => {
                    if !raw && self.peek(1) == b'N' && self.peek(2) == b'{' {
                        // `\N{name}`: its braces are not a replacement field.
                        self.i += 3;
                        while self.i < self.b.len()
                            && !matches!(self.b[self.i], b'}' | b'\n' | b'\r')
                            && self.b[self.i] != q
                        {
                            self.i += 1;
                        }
                        if self.peek(0) == b'}' {
                            self.i += 1;
                        }
                    } else if matches!(self.peek(1), b'{' | b'}') {
                        self.i += 1;
                    } else {
                        self.i += if self.peek(1) == b'\r' && self.peek(2) == b'\n' {
                            3
                        } else {
                            2
                        };
                    }
                }
                b'{' => {
                    if !spec && self.peek(1) == b'{' {
                        self.i += 2;
                        continue;
                    }
                    if self.i > s {
                        self.push(T::FMiddle, s, self.i, 0);
                    }
                    self.open(b'{', self.i)?;
                    self.push(T::LBrace, self.i, self.i + 1, 0);
                    self.i += 1;
                    return Ok(());
                }
                b'}' => {
                    if !spec {
                        if self.peek(1) == b'}' {
                            self.i += 2;
                            continue;
                        }
                        return Err(self.syntax(
                            format!("{kind}: single '}}' is not allowed"),
                            self.i,
                            self.i,
                        ));
                    }
                    if self.i > s {
                        self.push(T::FMiddle, s, self.i, 0);
                    }
                    // Flagged: it closes a format spec (for `tokenize`).
                    self.push(T::RBrace, self.i, self.i + 1, 1);
                    self.brackets.pop();
                    if let Some(fs) = self.fstrings.last_mut() {
                        fs.specs -= 1;
                    }
                    self.i += 1;
                    return Ok(());
                }
                _ => self.i += 1,
            }
        }
    }

    /// In a replacement field directly (no other bracket open)?
    fn in_field(&self) -> bool {
        self.fstrings
            .last()
            .is_some_and(|fs| self.brackets.len() == fs.base + fs.specs + 1)
    }

    #[allow(clippy::too_many_lines, reason = "one arm per operator")]
    fn operator(&mut self) -> LR {
        let s = self.i;
        let c = self.b[s];
        let c1 = self.peek(1);
        let c2 = self.peek(2);
        let (k, n) = match c {
            b'(' | b'[' | b'{' => {
                self.open(c, s)?;
                let k = match c {
                    b'(' => T::LPar,
                    b'[' => T::LSqb,
                    _ => T::LBrace,
                };
                (k, 1)
            }
            b')' | b']' | b'}' => {
                let open = match c {
                    b')' => b'(',
                    b']' => b'[',
                    _ => b'{',
                };
                let Some(&(top, off)) = self.brackets.last() else {
                    return Err(self.syntax(format!("unmatched '{}'", c as char), s, s));
                };
                if let Some(fs) = self.fstrings.last()
                    && self.brackets.len() == fs.base + fs.specs + 1
                    && c != b'}'
                {
                    let kind = if fs.t { "t-string" } else { "f-string" };
                    return Err(self.syntax(format!("{kind}: unmatched '{}'", c as char), s, s));
                }
                if top != open {
                    let (l1, _) = self.lines.loc(u32::try_from(off).unwrap_or(0));
                    let (l2, _) = self.lines.loc(u32::try_from(s).unwrap_or(0));
                    let on = if l1 == l2 {
                        String::new()
                    } else {
                        format!(" on line {l1}")
                    };
                    return Err(self.syntax(
                        format!(
                            "closing parenthesis '{}' does not match opening parenthesis '{}'{on}",
                            c as char, top as char
                        ),
                        s,
                        s,
                    ));
                }
                self.brackets.pop();
                let k = match c {
                    b')' => T::RPar,
                    b']' => T::RSqb,
                    _ => T::RBrace,
                };
                (k, 1)
            }
            b':' if self.in_field() => {
                if let Some(fs) = self.fstrings.last_mut() {
                    fs.specs += 1;
                }
                (T::Colon, 1)
            }
            b':' if c1 == b'=' => (T::ColonEqual, 2),
            b':' => (T::Colon, 1),
            b',' => (T::Comma, 1),
            b';' => (T::Semi, 1),
            b'+' if c1 == b'=' => (T::PlusEqual, 2),
            b'+' => (T::Plus, 1),
            b'-' if c1 == b'=' => (T::MinusEqual, 2),
            b'-' if c1 == b'>' => (T::Rarrow, 2),
            b'-' => (T::Minus, 1),
            b'*' if c1 == b'*' && c2 == b'=' => (T::DoubleStarEqual, 3),
            b'*' if c1 == b'*' => (T::DoubleStar, 2),
            b'*' if c1 == b'=' => (T::StarEqual, 2),
            b'*' => (T::Star, 1),
            b'/' if c1 == b'/' && c2 == b'=' => (T::DoubleSlashEqual, 3),
            b'/' if c1 == b'/' => (T::DoubleSlash, 2),
            b'/' if c1 == b'=' => (T::SlashEqual, 2),
            b'/' => (T::Slash, 1),
            b'|' if c1 == b'=' => (T::VbarEqual, 2),
            b'|' => (T::Vbar, 1),
            b'&' if c1 == b'=' => (T::AmperEqual, 2),
            b'&' => (T::Amper, 1),
            b'<' if c1 == b'<' && c2 == b'=' => (T::LeftShiftEqual, 3),
            b'<' if c1 == b'<' => (T::LeftShift, 2),
            b'<' if c1 == b'=' => (T::LessEqual, 2),
            b'<' => (T::Less, 1),
            b'>' if c1 == b'>' && c2 == b'=' => (T::RightShiftEqual, 3),
            b'>' if c1 == b'>' => (T::RightShift, 2),
            b'>' if c1 == b'=' => (T::GreaterEqual, 2),
            b'>' => (T::Greater, 1),
            b'=' if c1 == b'=' => (T::EqEqual, 2),
            b'=' => (T::Equal, 1),
            b'.' if c1 == b'.' && c2 == b'.' => (T::Ellipsis, 3),
            b'.' => (T::Dot, 1),
            b'%' if c1 == b'=' => (T::PercentEqual, 2),
            b'%' => (T::Percent, 1),
            b'~' => (T::Tilde, 1),
            b'^' if c1 == b'=' => (T::CircumflexEqual, 2),
            b'^' => (T::Circumflex, 1),
            b'@' if c1 == b'=' => (T::AtEqual, 2),
            b'@' => (T::At, 1),
            b'!' if c1 == b'=' => (T::NotEqual, 2),
            b'!' => (T::Exclamation, 1),
            _ if c.is_ascii_graphic() => (T::Unknown, 1),
            _ => {
                let msg = format!("invalid non-printable character U+{:04X}", u32::from(c));
                return Err(self.syntax(msg, s, s));
            }
        };
        self.i += n;
        self.push(k, s, self.i, 0);
        Ok(())
    }
}
