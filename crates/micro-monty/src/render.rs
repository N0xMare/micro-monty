//! Bounded text for hosts: head-and-tail cuts ([`head_tail`]) and the
//! container-aware cut of a completion `repr` ([`TextCut::HeadTail`]),
//! which keeps a value's shape and marks what it leaves out.

use mm_engine::{HostValue, Unconverted};
use std::fmt::Write as _;

/// How [`crate::Session::set_text_limit`] cuts a completion's text.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum TextCut {
    /// Keep the beginning (the default).
    #[default]
    Head,
    /// Keep the beginning and the end around a marker
    /// (`... [N bytes omitted] ...`). A completion value that converts to
    /// a container keeps its shape instead: each list, tuple and dict
    /// shows its first items and `...(+N items)`, a long string
    /// `...(+N chars)`.
    HeadTail,
}

/// Nesting shown before the rest of a value is a marker; keeps the
/// renderer's Rust recursion small (docs/spec.md §5).
const MAX_SHOWN_DEPTH: u32 = 32;

/// Room kept for a container's marker and closing bracket.
const RESERVE: usize = 24;

/// `text` cut to at most `max_bytes` bytes keeping its beginning and end
/// around `... [N bytes omitted] ...`, at character boundaries, and
/// whether it was cut. For output text too: hosts can apply it to what
/// their output callback collected. A `max_bytes` past the platform's
/// address space keeps the whole text.
#[must_use]
pub fn head_tail(text: &str, max_bytes: u64) -> (String, bool) {
    cut_head_tail(text, usize::try_from(max_bytes).unwrap_or(usize::MAX))
}

/// [`head_tail`] with the limit in this platform's `usize`.
pub(crate) fn cut_head_tail(text: &str, max_bytes: usize) -> (String, bool) {
    if text.len() <= max_bytes {
        return (text.to_owned(), false);
    }
    // The marker's own length depends on the count; size it for the
    // worst case, the whole text.
    let probe = marker(text.len());
    if max_bytes <= probe.len() + 2 {
        return (floor(text, max_bytes).to_owned(), true);
    }
    let keep = max_bytes - probe.len();
    let head = floor(text, keep.div_ceil(2));
    let mut start = text.len() - (keep - head.len());
    while !text.is_char_boundary(start) {
        start += 1;
    }
    let tail = &text[start..];
    let omitted = text.len() - head.len() - tail.len();
    (format!("{head}{}{tail}", marker(omitted)), true)
}

/// `text`, the first part of a longer text whose rendering stopped, cut to
/// at most `max_bytes` bytes keeping its beginning and marking the rest
/// (` ... [more omitted]`: its length is unknown).
pub(crate) fn head_more(text: &str, max_bytes: usize) -> String {
    const MORE: &str = " ... [more omitted]";
    format!(
        "{}{MORE}",
        floor(text, max_bytes.saturating_sub(MORE.len()))
    )
}

fn marker(omitted: usize) -> String {
    format!(" ... [{omitted} bytes omitted] ... ")
}

/// The longest prefix of `s` of at most `n` bytes.
pub(crate) fn floor(s: &str, n: usize) -> &str {
    let mut end = n.min(s.len());
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// A Python-style `repr` of `v` in about `limit` bytes that keeps its
/// containers' shape, or `None` when `v` is not a container (then a
/// head-and-tail cut of the text serves).
pub(crate) fn structured(v: &HostValue, limit: usize) -> Option<String> {
    let container = matches!(
        v,
        HostValue::List(_)
            | HostValue::Tuple(_)
            | HostValue::Dict(_)
            | HostValue::Record { .. }
            | HostValue::Object { .. }
    );
    if !container || limit < 2 * RESERVE {
        return None;
    }
    let mut out = String::new();
    render(v, &mut out, limit, 0);
    Some(out)
}

/// Appends `v` to `out` while `out` stays within about `limit` bytes.
fn render(v: &HostValue, out: &mut String, limit: usize, depth: u32) {
    match v {
        HostValue::None => out.push_str("None"),
        HostValue::Bool(b) => out.push_str(if *b { "True" } else { "False" }),
        HostValue::Int(i) => {
            let _ = write!(out, "{i}");
        }
        HostValue::BigInt(s) => text(out, s, limit, "digits", str::to_owned),
        HostValue::Float(f) => out.push_str(&mm_engine::text::float_repr(*f)),
        HostValue::Str(s) => text(out, s, limit, "chars", mm_engine::text::str_repr),
        HostValue::Bytes(b) => bytes(out, b, limit),
        HostValue::List(items) => seq(out, items, ("[", "]"), false, limit, depth),
        HostValue::Tuple(items) => seq(out, items, ("(", ")"), items.len() == 1, limit, depth),
        HostValue::Dict(items) => fields(out, items, None, limit, depth),
        HostValue::Record { name, fields: f }
        | HostValue::Object {
            class: name,
            fields: f,
        } => {
            let shown = name.rsplit('.').next().unwrap_or(name);
            fields(out, f, Some(shown), limit, depth);
        }
        HostValue::Unconverted(Unconverted::Opaque { repr, .. }) => {
            text(out, repr, limit, "chars", str::to_owned);
        }
        // Unconverted values, and kinds a later engine adds.
        _ => out.push_str("..."),
    }
}

/// A string-like leaf: whole, or the prefix that fits and `...(+N what)`.
fn text(out: &mut String, s: &str, limit: usize, what: &str, show: fn(&str) -> String) {
    let room = limit.saturating_sub(out.len());
    let whole = s.len() <= room;
    let mut prefix = floor(
        s,
        if whole {
            s.len()
        } else {
            room.saturating_sub(RESERVE)
        },
    );
    let mut shown = show(prefix);
    // Escapes can make the shown text longer than its source.
    while !whole && shown.len() > room.saturating_sub(RESERVE) && !prefix.is_empty() {
        prefix = floor(prefix, prefix.len() / 2);
        shown = show(prefix);
    }
    out.push_str(&shown);
    if prefix.len() < s.len() {
        let _ = write!(out, "...(+{} {what})", s[prefix.len()..].chars().count());
    }
}

/// `bytes.__repr__` of `b`, or of the prefix that fits.
fn bytes(out: &mut String, b: &[u8], limit: usize) {
    let room = limit.saturating_sub(out.len()).saturating_sub(RESERVE);
    let quote = if b.contains(&b'\'') && !b.contains(&b'"') {
        '"'
    } else {
        '\''
    };
    let mut shown = String::from("b");
    shown.push(quote);
    let mut n = 0;
    for &c in b {
        if shown.len() + 5 > room && n < b.len() {
            break;
        }
        match c {
            b'\\' => shown.push_str("\\\\"),
            b'\t' => shown.push_str("\\t"),
            b'\n' => shown.push_str("\\n"),
            b'\r' => shown.push_str("\\r"),
            c if char::from(c) == quote => {
                shown.push('\\');
                shown.push(quote);
            }
            0x20..=0x7e => shown.push(char::from(c)),
            c => {
                let _ = write!(shown, "\\x{c:02x}");
            }
        }
        n += 1;
    }
    shown.push(quote);
    out.push_str(&shown);
    if n < b.len() {
        let _ = write!(out, "...(+{} bytes)", b.len() - n);
    }
}

/// A list or tuple.
fn seq(
    out: &mut String,
    items: &[HostValue],
    (open, close): (&str, &str),
    one: bool,
    limit: usize,
    depth: u32,
) {
    out.push_str(open);
    let shown = each(out, items.len(), limit, depth, |out, i, limit| {
        render(&items[i], out, limit, depth + 1);
    });
    if one && shown == 1 {
        out.push(',');
    }
    omitted(out, items.len() - shown, shown);
    out.push_str(close);
}

/// A dict, or `Name(field=value, ...)` for a record or host object.
fn fields(
    out: &mut String,
    items: &[(String, HostValue)],
    name: Option<&str>,
    limit: usize,
    depth: u32,
) {
    match name {
        Some(n) => {
            text(out, n, limit, "chars", str::to_owned);
            out.push('(');
        }
        None => out.push('{'),
    }
    let shown = each(out, items.len(), limit, depth, |out, i, limit| {
        let (k, v) = &items[i];
        // Keys are cut like any string, so one long key stays bounded.
        if name.is_some() {
            text(out, k, limit, "chars", str::to_owned);
            out.push('=');
        } else {
            text(out, k, limit, "chars", mm_engine::text::str_repr);
            out.push_str(": ");
        }
        render(v, out, limit, depth + 1);
    });
    omitted(out, items.len() - shown, shown);
    out.push(if name.is_some() { ')' } else { '}' });
}

/// Renders items `0..n` separated by `, ` while they fit; returns how many
/// were shown. An item that does not fit whole is still shown cut if it
/// is the first, so the shape below it stays visible.
fn each(
    out: &mut String,
    n: usize,
    limit: usize,
    depth: u32,
    mut item: impl FnMut(&mut String, usize, usize),
) -> usize {
    if depth >= MAX_SHOWN_DEPTH {
        return 0;
    }
    let inner = limit.saturating_sub(RESERVE);
    for i in 0..n {
        let mark = out.len();
        if i > 0 {
            out.push_str(", ");
        }
        item(out, i, inner);
        if out.len() > inner || (i > 0 && out[mark..].contains("...(+")) {
            if i == 0 {
                return 1;
            }
            out.truncate(mark);
            return i;
        }
    }
    n
}

/// The marker for `left` items not shown after `shown` ones.
fn omitted(out: &mut String, left: usize, shown: usize) {
    if left > 0 {
        if shown > 0 {
            out.push_str(", ");
        }
        let _ = write!(
            out,
            "...(+{left} {})",
            if left == 1 { "item" } else { "items" }
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn head_tail_keeps_both_ends_within_the_limit() {
        let text = format!("{}{}", "a".repeat(500), "z".repeat(500));
        let (cut, truncated) = head_tail(&text, 100);
        assert!(truncated);
        assert!(cut.len() <= 100, "{}", cut.len());
        assert!(cut.starts_with("aaa") && cut.ends_with("zzz"), "{cut}");
        assert!(cut.contains("bytes omitted"), "{cut}");
        // The omitted count is exact.
        let n: usize = cut
            .split("... [")
            .nth(1)
            .and_then(|r| r.split(' ').next())
            .and_then(|n| n.parse().ok())
            .unwrap_or(0);
        assert_eq!(n, 1000 - (cut.len() - marker(n).len()));
        assert_eq!(head_tail("short", 100), ("short".to_owned(), false));
        // Multibyte text cuts at character boundaries.
        let (cut, _) = head_tail(&"é".repeat(400), 101);
        assert!(cut.len() <= 101);
    }

    #[test]
    fn structured_keeps_shape_and_counts_what_it_leaves_out() {
        let rows = HostValue::List(
            (0..1000)
                .map(|i| HostValue::Dict(vec![("id".into(), HostValue::Int(i))]))
                .collect(),
        );
        let text = structured(&rows, 200).unwrap_or_default();
        assert!(text.starts_with("[{'id': 0}, {'id': 1}"), "{text}");
        assert!(text.ends_with(" items)]"), "{text}");
        let left: usize = text
            .rsplit("...(+")
            .next()
            .and_then(|r| r.split(' ').next())
            .and_then(|n| n.parse().ok())
            .unwrap_or(0);
        let shown = text.matches("{'id'").count();
        assert_eq!(shown + left, 1000, "{text}");
        assert!(text.len() <= 200 + RESERVE, "{}", text.len());
        // A long string inside is cut with its own marker.
        let one = HostValue::Tuple(vec![HostValue::Str("x".repeat(5000))]);
        let text = structured(&one, 100).unwrap_or_default();
        assert!(
            text.starts_with("('xxx") && text.ends_with("chars),)"),
            "{text}"
        );
        assert!(structured(&HostValue::Str("x".repeat(5000)), 100).is_none());
        // So is a long dict key, alone or nested.
        let key = HostValue::Dict(vec![("k".repeat(100_000), HostValue::Int(1))]);
        for v in [key.clone(), HostValue::List(vec![key])] {
            let text = structured(&v, 120).unwrap_or_default();
            assert!(text.len() <= 120 + RESERVE, "{}", text.len());
            assert!(text.contains("chars)"), "{text}");
        }
    }
}
