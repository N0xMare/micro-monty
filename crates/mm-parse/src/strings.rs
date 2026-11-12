//! String literal bodies: escape decoding for `str`, `bytes` and f-string
//! literal text, as ruff 0.0.14 decodes them (unknown escapes keep their
//! backslash), except that a surrogate escape (`\ud800`) is an error,
//! where ruff makes it U+FFFD: a `str` is UTF-8 (docs/sdk.md).
//!
//! A line break in a body (`\r\n` or a lone `\r`) reads as `\n`, raw
//! bodies included, as CPython's tokenizer translates newlines before it
//! decodes; error positions count the translated body.
//!
//! `\N{...}` needs the Unicode name table (about 1 MB, docs/spec.md §6), so
//! it is reported as unsupported instead of decoded.

use alloc::{format, string::String, vec::Vec};

/// Why a body could not be decoded.
pub(crate) enum Bad {
    /// A malformed escape: CPython's message and the escape's byte range in
    /// the body.
    Escape(String),
    /// A `\N{...}` escape.
    NamedEscape,
    /// A non-ASCII character in a bytes literal.
    NonAsciiBytes,
}

/// Where the decoded text goes.
pub(crate) trait Sink {
    fn text(&mut self, s: &str);
    fn ch(&mut self, c: u32);
}

impl Sink for String {
    fn text(&mut self, s: &str) {
        self.push_str(s);
    }
    fn ch(&mut self, c: u32) {
        self.push(char::from_u32(c).unwrap_or('\u{FFFD}'));
    }
}

impl Sink for Vec<u8> {
    fn text(&mut self, s: &str) {
        self.extend_from_slice(s.as_bytes());
    }
    fn ch(&mut self, c: u32) {
        // Octal escapes past 0o377 wrap, as ruff's `c as u8` does.
        #[allow(clippy::cast_possible_truncation, reason = "wraps on purpose")]
        self.push(c as u8);
    }
}

fn hex(body: &str, i: usize, n: usize) -> Option<u32> {
    let digits = body.get(i..i + n)?;
    if !digits.bytes().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    u32::from_str_radix(digits, 16).ok()
}

/// Decodes a literal body. `bytes`: `\u`, `\U`, `\N` stay as written.
/// `fbraces`: f-string literal text, where `{{` and `}}` are single braces.
#[allow(
    clippy::too_many_lines,
    reason = "one scan over the body with CPython's escapes"
)]
pub(crate) fn decode(
    body: &str,
    bytes: bool,
    raw: bool,
    fbraces: bool,
    out: &mut dyn Sink,
) -> Result<(), Bad> {
    if bytes && !body.is_ascii() {
        return Err(Bad::NonAsciiBytes);
    }
    let b = body.as_bytes();
    let mut i = 0usize;
    let mut run = 0usize;
    // Bytes the translated line breaks dropped so far.
    let mut cut = 0usize;
    while i < b.len() {
        let c = b[i];
        if c == b'\r' {
            out.text(&body[run..i]);
            out.text("\n");
            let lf = usize::from(b.get(i + 1) == Some(&b'\n'));
            i += 1 + lf;
            cut += lf;
            run = i;
            continue;
        }
        if fbraces && (c == b'{' || c == b'}') {
            out.text(&body[run..=i]);
            i += if b.get(i + 1) == Some(&c) { 2 } else { 1 };
            run = i;
            continue;
        }
        if c != b'\\' || raw {
            i += 1;
            continue;
        }
        out.text(&body[run..i]);
        let esc = i;
        let Some(&e) = b.get(i + 1) else {
            // A trailing backslash (only reachable in f-string text).
            out.text("\\");
            i += 1;
            run = i;
            continue;
        };
        i += 2;
        match e {
            b'\n' => {}
            b'\r' => {
                if b.get(i) == Some(&b'\n') {
                    i += 1;
                }
            }
            b'\\' => out.text("\\"),
            b'\'' => out.text("'"),
            b'"' => out.text("\""),
            b'a' => out.text("\x07"),
            b'b' => out.text("\x08"),
            b'f' => out.text("\x0c"),
            b'n' => out.text("\n"),
            b'r' => out.text("\r"),
            b't' => out.text("\t"),
            b'v' => out.text("\x0b"),
            b'{' | b'}' if fbraces && b.get(i) == Some(&e) => {
                // `\{{`: the backslash stays and the brace pair follows.
                out.text("\\");
                i -= 1;
            }
            b'0'..=b'7' => {
                let mut v = u32::from(e - b'0');
                let mut n = 1;
                while n < 3 && matches!(b.get(i), Some(b'0'..=b'7')) {
                    v = v * 8 + u32::from(b[i] - b'0');
                    i += 1;
                    n += 1;
                }
                out.ch(v);
            }
            b'x' => {
                let Some(v) = hex(body, i, 2) else {
                    return Err(truncated(body, esc, i, "\\xXX", bytes, cut));
                };
                i += 2;
                out.ch(v);
            }
            b'u' | b'U' if !bytes => {
                let n = if e == b'u' { 4 } else { 8 };
                let Some(v) = hex(body, i, n) else {
                    let what = if e == b'u' { "\\uXXXX" } else { "\\UXXXXXXXX" };
                    return Err(truncated(body, esc, i, what, false, cut));
                };
                if v > 0x10_FFFF {
                    return Err(Bad::Escape(format!(
                        "(unicode error) 'unicodeescape' codec can't decode bytes in position {}-{}: illegal Unicode character",
                        esc - cut,
                        i + n - 1 - cut
                    )));
                }
                if (0xD800..=0xDFFF).contains(&v) {
                    // A `str` is UTF-8, which has no surrogates; CPython
                    // keeps a lone one.
                    return Err(Bad::Escape(format!(
                        "micro-monty does not support surrogate escapes ({} at position {}): its strings are UTF-8",
                        &body[esc..i + n],
                        esc - cut
                    )));
                }
                i += n;
                out.ch(v);
            }
            b'N' if !bytes => return Err(Bad::NamedEscape),
            _ => {
                // Unknown escape: kept with its backslash.
                let len = body[i - 1..].chars().next().map_or(1, char::len_utf8);
                out.text(&body[esc..i - 1 + len]);
                i = i - 1 + len;
            }
        }
        run = i;
    }
    out.text(&body[run..]);
    Ok(())
}

fn truncated(body: &str, esc: usize, at: usize, what: &str, bytes: bool, cut: usize) -> Bad {
    if bytes {
        return Bad::Escape(format!(
            "(value error) invalid \\x escape at position {}",
            esc - cut
        ));
    }
    let b = body.as_bytes();
    let mut end = at;
    while end < b.len() && b[end].is_ascii_hexdigit() && end < at + 8 {
        end += 1;
    }
    Bad::Escape(format!(
        "(unicode error) 'unicodeescape' codec can't decode bytes in position {}-{}: truncated {what} escape",
        esc - cut,
        end.saturating_sub(1).max(esc + 1) - cut
    ))
}
