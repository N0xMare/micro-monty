//! `bytes`: the type, its constructor and methods, and the codecs shared
//! with `str.encode` (utf-8, ascii, latin-1; errors strict, replace,
//! ignore, backslashreplace, xmlcharrefreplace).
//!
//! ```text
//! BYTES  value slot 0: ByteRef of the data
//! ```
//!
//! Work over long data (searches, splits, joins, replacements, repeats,
//! case maps, codecs, `hex`, `strip`, and `bytearray`'s, `binascii`'s and
//! `hashlib`'s) is a [`Job`]: a step does at most `nstep::STEP_WORK` units
//! and goes on in `_steps` (`nstep::op::BYTES`; docs/spec.md §5). A result
//! is written in place into an arena block of its final size, after a pass
//! that measures it when only the input tells; searches resume through
//! [`seek`], linear and without a table the size of the needle.

use crate::types::{T, ty};
use crate::vm::{CallArgs, Raised, Vm};
use alloc::{format, string::String, vec::Vec};
use mm_core::{ByteRef, Value};

/// Finds `needle` in `hay` (first or last occurrence) in linear time, in
/// one go ([`seek`]).
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn find_bytes(hay: &[u8], needle: &[u8], rev: bool) -> Option<usize> {
    if needle.is_empty() {
        return Some(if rev { hay.len() } else { 0 });
    }
    let mut s = [0; JOB_WORDS - W_S];
    seek_init(&mut s, hay.len());
    let mut work = usize::MAX;
    let Hit::At(p) = seek(hay, needle, rev, &mut s, &mut work) else {
        return None;
    };
    Some(if rev { hay.len() - p - needle.len() } else { p })
}

#[cfg_attr(mm_shipped, optimize(size))]
fn is_space(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c)
}

/// A codec name, normalised as CPython's lookup does.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Codec {
    Utf8,
    /// `utf-8-sig`: a BOM written on encode, skipped on decode.
    Utf8Sig,
    Ascii,
    Latin1,
    /// UTF-16 or UTF-32 (`wide`): `be` for big-endian, `bom` for the bare
    /// name (a BOM written on encode, read on decode).
    Utf16 {
        be: bool,
        bom: bool,
    },
    Utf32 {
        be: bool,
        bom: bool,
    },
}

impl Codec {
    #[cfg_attr(mm_shipped, optimize(size))]
    fn name(self) -> &'static str {
        match self {
            Codec::Utf8 | Codec::Utf8Sig => "utf-8",
            Codec::Ascii => "ascii",
            Codec::Latin1 => "latin-1",
            Codec::Utf16 { be: false, .. } => "utf-16-le",
            Codec::Utf16 { be: true, .. } => "utf-16-be",
            Codec::Utf32 { be: false, .. } => "utf-32-le",
            Codec::Utf32 { be: true, .. } => "utf-32-be",
        }
    }

    /// The codec and BOM length a decode of `data` uses: the bare UTF-16/32
    /// names read a BOM of either order.
    #[cfg_attr(mm_shipped, optimize(size))]
    fn with_bom(self, data: &[u8]) -> (Self, usize) {
        match self {
            Codec::Utf16 { bom: true, .. } if data.starts_with(&[0xFE, 0xFF]) => (
                Codec::Utf16 {
                    be: true,
                    bom: true,
                },
                2,
            ),
            Codec::Utf16 { bom: true, .. } if data.starts_with(&[0xFF, 0xFE]) => (self, 2),
            Codec::Utf32 { bom: true, .. } if data.starts_with(&[0, 0, 0xFE, 0xFF]) => (
                Codec::Utf32 {
                    be: true,
                    bom: true,
                },
                4,
            ),
            Codec::Utf32 { bom: true, .. } if data.starts_with(&[0xFF, 0xFE, 0, 0]) => (self, 4),
            Codec::Utf8Sig => (Codec::Utf8, if data.starts_with(BOM_UTF8) { 3 } else { 0 }),
            c => (c, 0),
        }
    }

    /// Lengths of the next valid prefix and invalid run, with its error reason.
    #[cfg_attr(mm_shipped, optimize(size))]
    fn decode_run(self, data: &[u8]) -> (usize, usize, &'static str) {
        match self {
            Self::Latin1 | Self::Utf16 { .. } | Self::Utf32 { .. } => (data.len(), 0, ""),
            Self::Ascii => match data.iter().position(|&b| b >= 0x80) {
                None => (data.len(), 0, ""),
                Some(k) => (k, 1, "ordinal not in range(128)"),
            },
            Self::Utf8 | Self::Utf8Sig => match core::str::from_utf8(data) {
                Ok(_) => (data.len(), 0, ""),
                Err(e) => {
                    let at = e.valid_up_to();
                    let (bad, reason) = match e.error_len() {
                        Some(n) => (
                            n,
                            if matches!(data[at], 0x80..=0xC1 | 0xF5..=0xFF) {
                                "invalid start byte"
                            } else {
                                "invalid continuation byte"
                            },
                        ),
                        None => (data.len() - at, "unexpected end of data"),
                    };
                    (at, bad, reason)
                }
            },
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Errors {
    Strict,
    SurrogateEscape,
    SurrogatePass,
    Replace,
    Ignore,
    BackslashReplace,
    XmlCharRefReplace,
    NameReplace,
    Unknown,
}

impl Errors {
    // Resolve once, but reject an unknown name only if conversion needs it.
    #[cfg_attr(mm_shipped, optimize(size))]
    fn named(name: &str) -> Self {
        match name {
            "strict" => Self::Strict,
            "surrogateescape" => Self::SurrogateEscape,
            "surrogatepass" => Self::SurrogatePass,
            "replace" => Self::Replace,
            "ignore" => Self::Ignore,
            "backslashreplace" => Self::BackslashReplace,
            "xmlcharrefreplace" => Self::XmlCharRefReplace,
            "namereplace" => Self::NameReplace,
            _ => Self::Unknown,
        }
    }
}

/// `namereplace`'s `\N{NAME}`, or the `backslashreplace` escape of a
/// character without a name.
#[cfg(feature = "unicodedata")]
#[cfg_attr(mm_shipped, optimize(size))]
fn name_escape(ch: char) -> String {
    crate::unicodedata::name_of(u32::from(ch))
        .map_or_else(|| escaped_char(ch), |n| format!("\\N{{{n}}}"))
}

/// The codec `name` names, after CPython's normalization (lower case, runs
/// of punctuation other than `.` as one `_`, `encodings.aliases`).
#[cfg_attr(mm_shipped, optimize(size))]
fn codec(name: &str) -> Option<Codec> {
    // The longest supported alias has 21 bytes; a longer normalized name
    // names none, so normalization needs no input-sized buffer.
    let mut buf = [0u8; 24];
    let mut len = 0;
    let mut punct = false;
    for c in name.chars() {
        if c.is_alphanumeric() || c == '.' {
            if punct && len > 0 {
                *buf.get_mut(len)? = b'_';
                len += 1;
            }
            if c.is_ascii() {
                *buf.get_mut(len)? = (c as u8).to_ascii_lowercase();
                len += 1;
            }
            punct = false;
        } else {
            punct = true;
        }
    }
    let n = &mut buf[..len];
    if let Some(c) = codec_alias(n, false) {
        return Some(c);
    }
    // The second try matches aliases only, not module names (`utf.8`).
    for b in n.iter_mut() {
        if *b == b'.' {
            *b = b'_';
        }
    }
    codec_alias(n, true)
}

/// Normalized names and aliases of each codec, in `CODECS` order, one
/// space-separated group per codec (a table: a `match` costs code size).
const ALIASES: &str = "utf_8 utf8 u8 utf cp65001 utf8_ucs2 utf8_ucs4|\
ascii us_ascii 646 us ansi_x3.4_1968 ansi_x3.4_1986 ansi_x3_4_1968 cp367 csascii ibm367 \
iso646_us iso_646.irv_1991 iso_ir_6|\
latin_1 latin1 iso8859_1 8859 cp819 l1 latin csisolatin1 ibm819 iso8859 iso_8859_1 \
iso_8859_1_1987 iso_ir_100|\
utf_16 utf16 u16|utf_16_le utf_16le unicodelittleunmarked|utf_16_be utf_16be unicodebigunmarked|\
utf_32 utf32 u32|utf_32_le utf_32le|utf_32_be utf_32be|utf_8_sig";

const BOM_UTF8: &[u8] = b"\xef\xbb\xbf";

const CODECS: [Codec; 10] = [
    Codec::Utf8,
    Codec::Ascii,
    Codec::Latin1,
    Codec::Utf16 {
        be: false,
        bom: true,
    },
    Codec::Utf16 {
        be: false,
        bom: false,
    },
    Codec::Utf16 {
        be: true,
        bom: false,
    },
    Codec::Utf32 {
        be: false,
        bom: true,
    },
    Codec::Utf32 {
        be: false,
        bom: false,
    },
    Codec::Utf32 {
        be: true,
        bom: false,
    },
    Codec::Utf8Sig,
];

/// The codec a normalized name or alias names; `aliases_only` skips each
/// group's first entry, the module name.
#[cfg_attr(mm_shipped, optimize(size))]
fn codec_alias(n: &[u8], aliases_only: bool) -> Option<Codec> {
    ALIASES.split('|').zip(CODECS).find_map(|(group, c)| {
        group
            .split(' ')
            .skip(usize::from(aliases_only))
            .any(|a| a.as_bytes() == n)
            .then_some(c)
    })
}

/// The UTF-16 or UTF-32 length of `s` with `c`'s BOM, `None` on overflow.
#[cfg_attr(mm_shipped, optimize(size))]
fn wide_len(s: &str, c: Codec) -> Option<usize> {
    match c {
        Codec::Utf16 { bom, .. } => s
            .chars()
            .map(char::len_utf16)
            .sum::<usize>()
            .checked_add(usize::from(bom))?
            .checked_mul(2),
        Codec::Utf32 { bom, .. } => crate::text::char_len(s)
            .checked_add(usize::from(bom))?
            .checked_mul(4),
        _ => Some(0),
    }
}

/// Writes the UTF-16 or UTF-32 bytes of `s` into `out`, sized by
/// [`wide_len`] (they cannot fail: no lone surrogates).
#[cfg_attr(mm_shipped, optimize(size))]
fn encode_wide(s: &str, c: Codec, out: &mut [u8]) {
    let (be, bom, wide) = match c {
        Codec::Utf16 { be, bom } => (be, bom, false),
        Codec::Utf32 { be, bom } => (be, bom, true),
        _ => return,
    };
    let mut at = 0;
    let mut unit = |u: u32| {
        if wide {
            let b = if be { u.to_be_bytes() } else { u.to_le_bytes() };
            if let Some(dst) = out.get_mut(at..at + 4) {
                dst.copy_from_slice(&b);
            }
            at += 4;
        } else {
            let u = u as u16;
            let b = if be { u.to_be_bytes() } else { u.to_le_bytes() };
            if let Some(dst) = out.get_mut(at..at + 2) {
                dst.copy_from_slice(&b);
            }
            at += 2;
        }
    };
    if bom {
        unit(0xFEFF);
    }
    for ch in s.chars() {
        if wide {
            unit(ch as u32);
        } else {
            let mut buf = [0u16; 2];
            for u in ch.encode_utf16(&mut buf) {
                unit(u32::from(*u));
            }
        }
    }
}

/// The next step of a UTF-16/UTF-32 decode at `i`: `Ok((char, len))` or
/// `Err((bad_len, reason))`.
#[cfg_attr(mm_shipped, optimize(size))]
fn wide_step(data: &[u8], i: usize, c: Codec) -> Result<(char, usize), (usize, &'static str)> {
    let rest = &data[i..];
    match c {
        Codec::Utf32 { be, .. } => {
            if rest.len() < 4 {
                return Err((rest.len(), "truncated data"));
            }
            let b = [rest[0], rest[1], rest[2], rest[3]];
            let u = if be {
                u32::from_be_bytes(b)
            } else {
                u32::from_le_bytes(b)
            };
            if (0xD800..0xE000).contains(&u) {
                return Err((
                    4,
                    "code point in surrogate code point range(0xd800, 0xe000)",
                ));
            }
            char::from_u32(u)
                .map(|ch| (ch, 4))
                .ok_or((4, "code point not in range(0x110000)"))
        }
        Codec::Utf16 { be, .. } => {
            let unit = |k: usize| {
                let b = [rest[k], rest[k + 1]];
                u32::from(if be {
                    u16::from_be_bytes(b)
                } else {
                    u16::from_le_bytes(b)
                })
            };
            if rest.len() < 2 {
                return Err((rest.len(), "truncated data"));
            }
            let u = unit(0);
            if !(0xD800..0xE000).contains(&u) {
                return Ok((char::from_u32(u).unwrap_or('\u{fffd}'), 2));
            }
            if u >= 0xDC00 {
                return Err((2, "illegal encoding"));
            }
            if rest.len() < 4 {
                return Err((rest.len(), "unexpected end of data"));
            }
            let lo = unit(2);
            if !(0xDC00..0xE000).contains(&lo) {
                return Err((2, "illegal UTF-16 surrogate"));
            }
            let cp = 0x10000 + ((u - 0xD800) << 10) + (lo - 0xDC00);
            Ok((char::from_u32(cp).unwrap_or('\u{fffd}'), 4))
        }
        _ => Err((rest.len(), "")),
    }
}

/// `'\xe9'`-style text for a code point in a `UnicodeEncodeError`.
#[cfg_attr(mm_shipped, optimize(size))]
fn escaped_char(c: char) -> String {
    let u = c as u32;
    if u <= 0xff {
        format!("\\x{u:02x}")
    } else if u <= 0xffff {
        format!("\\u{u:04x}")
    } else {
        format!("\\U{u:08x}")
    }
}

impl Vm {
    /// A new `bytes` object holding a copy of `data`.
    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn new_bytes(&mut self, data: &[u8]) -> Result<Value, Raised> {
        if data.len() >= 64 {
            self.burn_result(data.len(), true);
        }
        let b = {
            let res = self.heap.alloc_bytes(data);
            self.hx_o(res)
        }?;
        let r = {
            let res = self
                .heap
                .alloc_init(ty::BYTES, 0, &[Value::from_byte_ref(b)], &[]);
            self.hx_o(res)
        }?;
        Ok(Value::from_ref(r, ty::BYTES as u8))
    }

    /// [`Vm::build_kind`] with a result of `v`'s kind.
    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn build_like(
        &mut self,
        v: Value,
        len: usize,
        parts: &[Part<'_>],
    ) -> Result<Value, Raised> {
        self.build_kind(Vm::is_bytearray(v), len, parts)
    }

    /// A new `bytes` (a `bytearray` when `ba`) of `len` bytes, `parts` in
    /// order, built in its arena block (one copy, at memory speed; no
    /// buffer the size of the result).
    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn build_kind(
        &mut self,
        ba: bool,
        len: usize,
        parts: &[Part<'_>],
    ) -> Result<Value, Raised> {
        let b = self.job_block(len)?;
        let mut o = 0;
        for p in parts {
            let n = match *p {
                Part::Copy(v, a, e) => {
                    let n = e.saturating_sub(a);
                    let ok = e <= self.job_data(v).len()
                        && self
                            .data_block(v)
                            .is_some_and(|src| self.heap.copy_bytes(src, a, b, o, n).is_ok());
                    if !ok && n > 0 {
                        return Err(self.bad_step());
                    }
                    n
                }
                Part::Fill(c, n) => {
                    if c != 0
                        && let Ok(out) = self.heap.bytes_mut(b)
                        && let Some(out) = out.get_mut(o..o + n)
                    {
                        out.fill(c);
                    }
                    n
                }
                Part::Lit(d) => {
                    if let Ok(out) = self.heap.bytes_mut(b)
                        && let Some(out) = out.get_mut(o..o + d.len())
                    {
                        out.copy_from_slice(d);
                    }
                    d.len()
                }
            };
            o += n;
        }
        let res = if ba { R_BYTEARRAY } else { R_BYTES };
        self.job_wrap(&Job::new(0, Value::NONE, 0, 0, res), b, 0)
    }

    /// `a + b` for bytes-likes, of `a`'s kind; out of line, as `binop`
    /// is on the dispatch loop's path.
    #[inline(never)]
    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn bytes_concat(&mut self, a: Value, b: Value) -> Result<Value, Raised> {
        let (la, lb) = (self.bytes_data(a).len(), self.bytes_data(b).len());
        let parts = [Part::Copy(a, 0, la), Part::Copy(b, 0, lb)];
        self.build_like(a, la.saturating_add(lb), &parts)
    }

    /// A new `bytes` of `v`'s kind (a `bytearray` for a `bytearray`).
    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn new_like(&mut self, v: Value, data: &[u8]) -> Result<Value, Raised> {
        self.new_kind(Vm::is_bytearray(v), data)
    }

    /// A new `bytes`, or a `bytearray` when `ba`.
    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn new_kind(&mut self, ba: bool, data: &[u8]) -> Result<Value, Raised> {
        if ba {
            self.new_bytearray(data)
        } else {
            self.new_bytes(data)
        }
    }

    /// Copy a contiguous byte range of `v`'s kind, reusing an unchanged
    /// `bytes` (a `bytearray` is always copied).
    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn bytes_slice(
        &mut self,
        v: Value,
        start: usize,
        end: usize,
    ) -> Result<Value, Raised> {
        if start == 0 && end == self.bytes_data(v).len() && Vm::is_bytes(v) {
            return Ok(v);
        }
        let len = end - start;
        // Split fields usually fit here; no data-dependent native allocation.
        let mut small = [0; 256];
        if len <= small.len() {
            small[..len].copy_from_slice(&self.bytes_data(v)[start..end]);
            return self.new_like(v, &small[..len]);
        }
        self.build_like(v, len, &[Part::Copy(v, start, end)])
    }

    /// Is `v` a `bytes`?
    #[inline]
    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn is_bytes(v: Value) -> bool {
        v.is_obj_kind(ty::BYTES as u8)
    }

    /// Is `v` a `bytes` or a `bytearray`?
    #[inline]
    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn is_bytes_like(v: Value) -> bool {
        Vm::is_bytes(v) || Vm::is_bytearray(v)
    }

    /// The data of a `bytes` or `bytearray` value (empty for anything else).
    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn bytes_data(&self, v: Value) -> &[u8] {
        if Vm::is_bytes(v) {
            return self
                .vslot(v, 0)
                .as_byte_ref()
                .and_then(|b| self.heap.bytes(b).ok())
                .unwrap_or(&[]);
        }
        if Vm::is_bytearray(v) {
            return self.bytearray_data(v);
        }
        &[]
    }

    /// `bytes * k` (a `bytearray` for a `bytearray`).
    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn bytes_repeat(&mut self, v: Value, k: usize) -> Result<Value, Raised> {
        self.repeat_job(v, k, Value::NONE)
    }

    /// `v * k`, or with bytearray `into` its new contents (`*=`).
    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn repeat_job(&mut self, v: Value, k: usize, into: Value) -> Result<Value, Raised> {
        let total = self.bytes_data(v).len().saturating_mul(k);
        if total > (1 << 30) {
            return Err(self.err(T::MemoryError, ""));
        }
        let res = if into != Value::NONE {
            R_INTO
        } else if Vm::is_bytearray(v) {
            R_BYTEARRAY
        } else {
            R_BYTES
        };
        let mut j = self.job_fixed(job::REPEAT, v, 0, 0, res, total)?;
        j.v[3] = into;
        self.job_run(j)
    }

    /// A job of `kind` over `src[i..end]` whose output has `len` bytes:
    /// its block allocated, so it starts with the pass that writes.
    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn job_fixed(
        &mut self,
        kind: u8,
        src: Value,
        i: usize,
        end: usize,
        res: u64,
        len: usize,
    ) -> Result<Job, Raised> {
        let b = self.job_block(len)?;
        let mut j = Job::new(kind, src, i, end, res);
        j.v[2] = Value::from_byte_ref(b);
        j.w[W_PASS] = 1;
        Ok(j)
    }

    /// `x in b` (`not in` with `neg`), a step at a time over long data;
    /// out of line, as the `in` operator is on the dispatch loop's path.
    #[inline(never)]
    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn bytes_in(&mut self, b: Value, item: Value, neg: bool) -> Result<Value, Raised> {
        let n = self.bytes_data(b).len();
        let mut j = Job::new(job::FIND, b, 0, n, 0);
        j.w[W_A] = if neg { found::NOT_IN } else { found::IN };
        if Vm::is_bytes_like(item) {
            if self.bytes_data(item).is_empty() {
                return Ok(Value::from_bool(!neg));
            }
            j.v[1] = item;
        } else if Vm::is_intlike(item) {
            j.w[W_A + 2] = u64::from(self.byte_value(item)?);
        } else {
            let msg = format!(
                "a bytes-like object is required, not '{}'",
                self.type_name(item)
            );
            return Err(self.err(T::TypeError, &msg));
        }
        seek_init(&mut j.w[W_S..], n);
        self.job_run(j)
    }

    /// `item in b`.
    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn bytes_contains(&mut self, b: Value, item: Value) -> Result<bool, Raised> {
        self.burn_bytes(self.bytes_data(b).len());
        if Vm::is_bytes_like(item) {
            let (hay, needle) = (self.bytes_data(b), self.bytes_data(item));
            return Ok(find_bytes(hay, needle, false).is_some());
        }
        if Vm::is_intlike(item) {
            let byte = self.byte_value(item)?;
            return Ok(self.bytes_data(b).contains(&byte));
        }
        let msg = format!(
            "a bytes-like object is required, not '{}'",
            self.type_name(item)
        );
        Err(self.err(T::TypeError, &msg))
    }

    /// An int in `range(0, 256)`.
    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn byte_value(&mut self, v: Value) -> Result<u8, Raised> {
        match self.int_of_o(v).and_then(|i| u8::try_from(i).ok()) {
            Some(b) => Ok(b),
            None => Err(self.err(T::ValueError, "byte must be in range(0, 256)")),
        }
    }

    /// `b[key]`.
    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn bytes_getitem(&mut self, b: Value, key: Value) -> Result<Value, Raised> {
        let n = self.bytes_data(b).len();
        if let Some(i) = self.int_of_o(key) {
            return match Vm::norm_index(i, n) {
                Some(k) => Ok(Value::int(i64::from(self.bytes_data(b)[k])).unwrap_or(Value::NONE)),
                None if Vm::is_bytearray(b) => {
                    Err(self.err(T::IndexError, "bytearray index out of range"))
                }
                None => Err(self.err(T::IndexError, "index out of range")),
            };
        }
        if key.is_obj_kind(ty::SLICE as u8) {
            let (start, _, step, len) = self.slice_indices(key, n)?;
            if step == 1 {
                return self.bytes_slice(b, start as usize, start as usize + len);
            }
            let res = if Vm::is_bytearray(b) {
                R_BYTEARRAY
            } else {
                R_BYTES
            };
            let mut j = self.job_fixed(job::STRIDE, b, 0, 0, res, len)?;
            (j.w[W_A], j.w[W_A + 1]) = (start as u64, step as u64);
            return self.job_run(j);
        }
        if self.uflags(key) & crate::class::f::INDEX != 0 {
            return Err(self.need_py());
        }
        let msg = format!(
            "{} indices must be integers or slices, not {}",
            if Vm::is_bytearray(b) {
                "bytearray"
            } else {
                "byte"
            },
            self.type_name(key)
        );
        Err(self.err(T::TypeError, &msg))
    }

    /// Encodes a string value with `codec`; `errors` is the handler name.
    /// The result is a `bytearray` when `ba`.
    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn encode_str(
        &mut self,
        source: Value,
        enc: Value,
        errors: Value,
        ba: bool,
    ) -> Result<Value, Raised> {
        let res = if ba { R_BYTEARRAY } else { R_BYTES };
        let Some(c) = codec(if Self::is_str(enc) {
            self.str_ref_o(enc)
        } else {
            "utf-8"
        }) else {
            return Err(self.codec_lookup_error(enc, true));
        };
        let limit: u64 = match c {
            Codec::Utf8 => {
                let n = self.str_bytes(source).len();
                let j = self.job_fixed(job::REPEAT, source, 0, 0, res, n)?;
                return self.job_run(j);
            }
            Codec::Utf8Sig => {
                let n = self.str_bytes(source).len().saturating_add(3);
                let parts = [Part::Lit(BOM_UTF8), Part::Copy(source, 0, n - 3)];
                return self.build_kind(ba, n, &parts);
            }
            Codec::Ascii => 0x80,
            Codec::Latin1 => 0x100,
            Codec::Utf16 { .. } | Codec::Utf32 { .. } => 0x11_0000,
        };
        let n = self.str_bytes(source).len();
        if n == 0 {
            // Only a BOM, if any.
            let mut b = [0; 4];
            let len = wide_len("", c).unwrap_or(0).min(4);
            encode_wide("", c, &mut b[..len]);
            return self.new_kind(ba, &b[..len]);
        }
        let mut j = Job::new(job::ENCODE, source, 0, n, res);
        j.w[W_A] = codec_word(c);
        j.w[W_A + 1] = self.errors_word(errors);
        j.w[W_A + 2] = limit;
        j.v[3] = errors;
        self.job_run(j)
    }

    /// The handler `errors` names (`strict` when not a `str`), as a job word.
    #[cfg_attr(mm_shipped, optimize(size))]
    fn errors_word(&self, errors: Value) -> u64 {
        let e = Errors::named(if Self::is_str(errors) {
            self.str_ref_o(errors)
        } else {
            "strict"
        });
        HANDLERS.iter().position(|&h| h == e).unwrap_or(8) as u64
    }

    /// Decodes a bytes value with `codec`; `errors` is the handler name.
    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn decode_bytes(
        &mut self,
        source: Value,
        enc: Value,
        errors: Value,
    ) -> Result<Value, Raised> {
        let Some(c) = codec(if Self::is_str(enc) {
            self.str_ref_o(enc)
        } else {
            "utf-8"
        }) else {
            return Err(self.codec_lookup_error(enc, true));
        };
        let data = self.bytes_data(source);
        let n = data.len();
        let sig = c == Codec::Utf8Sig;
        let (c, skip) = c.with_bom(data);
        let mut j = Job::new(job::DECODE, source, skip, n, R_STR);
        j.w[W_A] = codec_word(c);
        j.w[W_A + 1] = self.errors_word(errors);
        // utf-8-sig reports positions after its BOM, as CPython's.
        j.w[W_A + 2] = if sig { skip as u64 } else { 0 };
        j.v[3] = errors;
        self.job_run(j)
    }

    /// Build a possibly large name-bearing error through the admitted string buffer.
    #[cfg_attr(mm_shipped, optimize(size))]
    fn codec_lookup_error(&mut self, name: Value, encoding: bool) -> Raised {
        let (prefix, suffix) = if encoding {
            ("unknown encoding: ", "")
        } else {
            ("unknown error handler name '", "'")
        };
        let len = self
            .str_bytes(name)
            .len()
            .saturating_add(prefix.len() + suffix.len());
        match self.build_text(len, |vm, out| {
            out.push_str(prefix);
            vm.sbuf_push(out, name);
            out.push_str(suffix);
            Ok(())
        }) {
            Ok(message) => self.err_value(T::LookupError, message),
            Err(raised) => raised,
        }
    }

    /// A `str` argument named `name` of method `func`, or the default sentinel.
    #[cfg_attr(mm_shipped, optimize(size))]
    fn codec_arg(&mut self, v: Value, func: &str, name: &str) -> Result<Value, Raised> {
        if v == Value::UNDEFINED || Vm::is_str(v) {
            return Ok(v);
        }
        let tn = if v == Value::NONE {
            "None"
        } else {
            self.type_name(v)
        };
        let msg = format!("{func}() argument '{name}' must be str, not {tn}");
        Err(self.err(T::TypeError, &msg))
    }

    /// The bytes of a bytes-like argument (`bytes` or `bytearray`).
    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn bytes_like(&self, v: Value) -> Option<&[u8]> {
        if Vm::is_bytes_like(v) {
            Some(self.bytes_data(v))
        } else {
            None
        }
    }

    #[cfg_attr(mm_shipped, optimize(size))]
    fn need_bytes(&mut self, v: Value) -> Result<Value, Raised> {
        if Self::is_bytes_like(v) {
            return Ok(v);
        }
        let msg = format!(
            "a bytes-like object is required, not '{}'",
            self.type_name(v)
        );
        Err(self.err(T::TypeError, &msg))
    }
}

// ----------------------------------------------------------- constructor

/// `bytes(source, encoding, errors)`.
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn bi_bytes(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    construct(vm, &a, false)
}

/// `bytearray(source, encoding, errors)`.
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn bi_bytearray(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    construct(vm, &a, true)
}

/// Both constructors; `ba` makes a `bytearray`.
#[cfg_attr(mm_shipped, optimize(size))]
fn construct(vm: &mut Vm, a: &CallArgs, ba: bool) -> Result<Value, Raised> {
    let a = *a;
    let name = if ba { "bytearray" } else { "bytes" };
    let src = vm.arg_o(&a, 0);
    let (enc, errs) = (vm.arg_o(&a, 1), vm.arg_o(&a, 2));
    if src == Value::UNDEFINED {
        if enc != Value::UNDEFINED {
            return Err(vm.err(T::TypeError, "encoding without a string argument"));
        }
        if errs != Value::UNDEFINED {
            return Err(vm.err(T::TypeError, "errors without a string argument"));
        }
        return vm.new_kind(ba, &[]);
    }
    if Vm::is_str(src) {
        if enc == Value::UNDEFINED {
            return Err(vm.err(T::TypeError, "string argument without an encoding"));
        }
        let e = vm.codec_arg(enc, name, "encoding")?;
        let r = vm.codec_arg(errs, name, "errors")?;
        return vm.encode_str(src, e, r, ba);
    }
    if enc != Value::UNDEFINED {
        return Err(vm.err(T::TypeError, "encoding without a string argument"));
    }
    if errs != Value::UNDEFINED {
        return Err(vm.err(T::TypeError, "errors without a string argument"));
    }
    if Vm::is_bytes(src) && !ba {
        return Ok(src);
    }
    if Vm::is_bytes_like(src) {
        return vm.bytes_copy(src, ba);
    }
    // An int subclass (IntEnum) without its own `__index__`: a count.
    let src = match vm.backing(src) {
        Some(b) if Vm::is_intlike(b) && vm.uflags(src) & crate::class::f::INDEX == 0 => b,
        _ => src,
    };
    if Vm::is_intlike(src) {
        let Some(n) = vm.int_of_o(src) else {
            return Err(vm.index_overflow(T::OverflowError));
        };
        if n < 0 {
            return Err(vm.err(T::ValueError, "negative count"));
        }
        let n = usize::try_from(n).unwrap_or(usize::MAX);
        if n > (1 << 30) || !vm.heap.would_fit(n as u64) {
            return Err(vm.err(T::MemoryError, ""));
        }
        // Zeros: the arena block as it is allocated.
        let b = vm.job_block(n)?;
        let res = if ba { R_BYTEARRAY } else { R_BYTES };
        return vm.job_wrap(&Job::new(job::MAP, src, 0, 0, res), b, 0);
    }
    if src.is_float() {
        let msg = format!("cannot convert 'float' object to {name}");
        return Err(vm.err(T::TypeError, &msg));
    }
    if vm.user_class(src).is_some() {
        return Err(vm.need_py());
    }
    if Vm::ints_in_place(src) {
        let res = if ba { R_BYTEARRAY } else { R_BYTES };
        return vm.ints_job(src, res, Value::NONE);
    }
    if vm.is_iterator(src) {
        // Pulled an item at a time in the prelude (`_pulled`), so a bad
        // one leaves the rest in the iterator, as in CPython.
        return Err(vm.need_py());
    }
    let items = match vm.collect(src) {
        Ok(items) => items,
        Err(e) => {
            if vm.exc_type(vm.exc) == T::TypeError && !vm.slow {
                let msg = format!("cannot convert '{}' object to {name}", vm.type_name(src));
                return Err(vm.err(T::TypeError, &msg));
            }
            return Err(e);
        }
    };
    let (_charge, out) = vm.ints_to_bytes(&items, ba)?;
    vm.new_kind(ba, &out)
}

impl Vm {
    /// The values of `items`, each an int in `range(0, 256)`, admitted as
    /// scratch for as long as the `Vec` lives (the caller copies it into the
    /// arena at once).
    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn ints_to_bytes(
        &mut self,
        items: &[Value],
        ba: bool,
    ) -> Result<(mm_core::Scratch, Vec<u8>), Raised> {
        let vm = self;
        let res = vm.heap.reserve_scratch(items.len());
        let charge = vm.hx_o(res)?;
        let mut out = Vec::new();
        let res = out
            .try_reserve_exact(items.len())
            .map_err(|_| mm_core::Error::OutOfMemory);
        vm.hx_o(res)?;
        for &x in items {
            out.push(vm.int_byte(x, ba, None)?);
        }
        Ok((charge, out))
    }

    /// Item `x` of an iterable of ints making a `bytearray` (`ba`) or
    /// `bytes`, as a byte. An `__index__` object needs Python code: the
    /// prelude's when `name` is `None`, else the error the prelude raises
    /// for `name()` (a later step cannot hand off).
    #[inline(never)]
    #[cfg_attr(mm_shipped, optimize(size))]
    fn int_byte(&mut self, x: Value, ba: bool, name: Option<&str>) -> Result<u8, Raised> {
        // An int subclass (IntEnum) is its value.
        let x = match self.backing(x) {
            Some(b) if Vm::is_intlike(b) => b,
            _ => x,
        };
        if !Vm::is_intlike(x) {
            if self.uflags(x) & crate::class::f::INDEX != 0 {
                let Some(name) = name else {
                    return Err(self.need_py());
                };
                let msg = format!("micro-monty does not support user-defined objects in {name}()");
                return Err(self.err(T::TypeError, &msg));
            }
            let msg = format!(
                "'{}' object cannot be interpreted as an integer",
                self.type_name(x)
            );
            return Err(self.err(T::TypeError, &msg));
        }
        match self.int_of_o(x).and_then(|i| u8::try_from(i).ok()) {
            Some(b) => Ok(b),
            None if ba => Err(self.err(T::ValueError, "byte must be in range(0, 256)")),
            None => Err(self.err(T::ValueError, "bytes must be in range(0, 256)")),
        }
    }

    /// `bytes(src)`, `bytearray(src)` ([`R_BYTES`], [`R_BYTEARRAY`]) or
    /// `into.extend(src)` ([`R_APPEND`]) for a list or tuple of ints
    /// `src`, a [`job::INTS`] job reading the items in place.
    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn ints_job(&mut self, src: Value, res: u64, into: Value) -> Result<Value, Raised> {
        let n = self.ints_len(src);
        if res == R_APPEND {
            let len = self.bytearray_data(into).len();
            let Some(total) = len.checked_add(n) else {
                return Err(self.err(T::MemoryError, ""));
            };
            if self.ba_grows_far(into, total) {
                return self.ba_grow(into, total, crate::bytearray::fin::INTS, [0, 0], src);
            }
            let j = self.ints_append(src, into)?;
            return self.job_run(j);
        }
        let mut j = Job::new(job::INTS, src, 0, self.ints_words(src).len(), res);
        j.v[2] = Value::from_byte_ref(self.job_block(n)?);
        self.job_run(j)
    }

    /// Can [`Vm::ints_job`] read `v` in place: a list, tuple, set or
    /// frozenset?
    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn ints_in_place(v: Value) -> bool {
        v.is_obj_kind(ty::LIST as u8) || v.is_obj_kind(ty::TUPLE as u8) || Vm::is_set(v)
    }

    /// The value words a [`job::INTS`] job reads: a list's or tuple's
    /// items, a set's table (`UNDEFINED` in its empty slots).
    #[cfg_attr(mm_shipped, optimize(size))]
    fn ints_words(&self, v: Value) -> &[u64] {
        match self.set_parts(v) {
            Some((_, k, _, _)) => self.heap.slot_words(k).unwrap_or(&[]),
            None => self.seq_words(v),
        }
    }

    /// The bytes a [`job::INTS`] job over `v` makes.
    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn ints_len(&self, v: Value) -> usize {
        if Vm::is_set(v) {
            self.set_len(v)
        } else {
            self.seq_words(v).len()
        }
    }

    /// `into` grown by the length of list or tuple `src`, and the
    /// [`job::INTS`] job that fills the new bytes.
    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn ints_append(&mut self, src: Value, into: Value) -> Result<Job, Raised> {
        let n = self.ints_len(src);
        let len = self.bytearray_data(into).len();
        let Some(total) = len.checked_add(n) else {
            return Err(self.err(T::MemoryError, ""));
        };
        self.ba_resize(into, total, usize::MAX)?;
        let mut j = Job::new(job::INTS, src, 0, self.ints_words(src).len(), R_APPEND);
        (j.v[3], j.w[W_O], j.w[W_A]) = (into, len as u64, len as u64);
        Ok(j)
    }

    /// A copy of bytes-like `v` as a `bytearray` (`ba`) or `bytes`.
    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn bytes_slice_as(&mut self, v: Value, ba: bool) -> Result<Value, Raised> {
        let n = self.bytes_data(v).len();
        self.build_kind(ba, n, &[Part::Copy(v, 0, n)])
    }

    /// [`Vm::bytes_slice_as`] as a native's result: a copy past a step's
    /// work goes a step at a time (a [`job::REPEAT`] of one).
    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn bytes_copy(&mut self, v: Value, ba: bool) -> Result<Value, Raised> {
        let n = self.bytes_data(v).len();
        if ((n / 64) as u64) < crate::nstep::STEP_WORK {
            return self.bytes_slice_as(v, ba);
        }
        let res = if ba { R_BYTEARRAY } else { R_BYTES };
        let j = self.job_fixed(job::REPEAT, v, 0, 0, res, n)?;
        self.job_run(j)
    }
}

// ---------------------------------------------------------------- methods

#[cfg_attr(mm_shipped, optimize(size))]
fn this_value(vm: &mut Vm, a: &CallArgs, name: &str) -> Result<Value, Raised> {
    let s = vm.arg_o(a, 0);
    if !Vm::is_bytes_like(s) {
        let msg = format!(
            "descriptor '{name}' for 'bytes' objects doesn't apply to a '{}' object",
            vm.type_name(s)
        );
        return Err(vm.err(T::TypeError, &msg));
    }
    Ok(s)
}

#[cfg_attr(mm_shipped, optimize(size))]
fn opt(v: Value) -> Option<Value> {
    if v == Value::UNDEFINED || v == Value::NONE {
        None
    } else {
        Some(v)
    }
}

/// An optional search bound (`d` when absent) as a slice index: an
/// oversized int clamps.
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn search_bound(vm: &mut Vm, v: Value, d: i64) -> Result<i64, Raised> {
    let Some(x) = opt(v) else { return Ok(d) };
    if let Some(i) = vm.int_of_o(x) {
        return Ok(i);
    }
    if Vm::is_heap_int(x) {
        return Ok(if vm.big_of(x).is_some_and(|b| b.neg) {
            i64::MIN / 2
        } else {
            i64::MAX / 2
        });
    }
    if vm.uflags(x) & crate::class::f::INDEX != 0 {
        return Err(vm.need_py());
    }
    // An int subclass (IntEnum) without its own `__index__`.
    if let Some(i) = vm.backing(x).and_then(|b| vm.int_of_o(b)) {
        return Ok(i);
    }
    Err(vm.err(
        T::TypeError,
        "slice indices must be integers or None or have an __index__ method",
    ))
}

/// CPython's `ADJUST_INDICES` for optional `start`/`end` arguments.
#[cfg_attr(mm_shipped, optimize(size))]
fn window(vm: &mut Vm, len: usize, start: Value, end: Value) -> Result<(i64, i64), Raised> {
    let n = i64::try_from(len).unwrap_or(i64::MAX);
    let get = search_bound;
    let mut st = get(vm, start, 0)?;
    let mut en = get(vm, end, n)?;
    if en > n {
        en = n;
    } else if en < 0 {
        en = (en + n).max(0);
    }
    if st < 0 {
        st = (st + n).max(0);
    }
    Ok((st, en))
}

/// The needle of find/count/index: bytes or an int byte.
#[cfg_attr(mm_shipped, optimize(size))]
fn needle(vm: &mut Vm, v: Value) -> Result<Value, Raised> {
    if Vm::is_bytes_like(v) {
        return Ok(v);
    }
    if Vm::is_intlike(v) {
        return Ok(Value::int(i64::from(vm.byte_value(v)?)).unwrap_or(Value::NONE));
    }
    let msg = format!(
        "argument should be integer or bytes-like object, not '{}'",
        vm.type_name(v)
    );
    Err(vm.err(T::TypeError, &msg))
}

/// A `find`-family search (`mode`, a [`found`] word) or `count` of the
/// needle in the window, a step at a time over long data.
#[cfg_attr(mm_shipped, optimize(size))]
fn search(vm: &mut Vm, a: &CallArgs, name: &str, mode: u64, rev: bool) -> Result<Value, Raised> {
    let s = this_value(vm, a, name)?;
    let len = vm.bytes_data(s).len();
    // CPython converts the bounds before checking the needle (`count`
    // after).
    let count = mode == found::COUNT;
    let sub = if count {
        needle(vm, vm.arg_o(a, 1))?
    } else {
        Value::NONE
    };
    let (st, en) = window(vm, len, vm.arg_o(a, 2), vm.arg_o(a, 3))?;
    let sub = if count {
        sub
    } else {
        needle(vm, vm.arg_o(a, 1))?
    };
    let m = vm.bytes_like(sub).map_or(1, <[u8]>::len) as i64;
    let n = len as i64;
    if count && (st > n || en < st) {
        return vm.new_int(0);
    }
    if count && m == 0 {
        return vm.new_int(en - st + 1);
    }
    if st > n || en - st < m {
        return if count {
            vm.new_int(0)
        } else {
            found(vm, None, mode == found::INDEX)
        };
    }
    let (st, en) = (st as usize, en as usize);
    if m == 0 {
        return vm.new_int(if rev { en } else { st } as i64);
    }
    let mut j = Job::new(job::FIND, s, st, en, 0);
    (j.w[W_A], j.w[W_A + 1]) = (mode, u64::from(rev));
    if Vm::is_bytes_like(sub) {
        j.v[1] = sub;
    } else {
        j.w[W_A + 2] = sub.as_int().unwrap_or(0) as u64;
    }
    seek_init(&mut j.w[W_S..], en - st);
    vm.job_run(j)
}

/// A search of str or bytes `src`'s bytes `st..en` for `sub` (of the same
/// type, non-empty) giving `mode` ([`found`]: not `PARTITION`), from the
/// end when `rev`: a step at a time, the needle's factorization kept
/// between steps ([`seek`]). For a str needle too long to read whole in
/// every step (`strmeth`).
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn find_job(
    vm: &mut Vm,
    src: Value,
    sub: Value,
    (st, en): (usize, usize),
    mode: u64,
    rev: bool,
) -> Result<Value, Raised> {
    let mut j = Job::new(job::FIND, src, st, en, 0);
    (j.w[W_A], j.w[W_A + 1]) = (mode, u64::from(rev));
    j.v[1] = sub;
    seek_init(&mut j.w[W_S..], en - st);
    vm.job_run(j)
}

/// A [`job::FIND`] value's bytes: a str's UTF-8, or bytes-like data.
#[cfg_attr(mm_shipped, optimize(size))]
fn find_data(vm: &Vm, v: Value) -> &[u8] {
    if Vm::is_str(v) {
        vm.str_bytes(v)
    } else {
        vm.bytes_data(v)
    }
}

#[cfg_attr(mm_shipped, optimize(size))]
fn found(vm: &mut Vm, r: Option<usize>, raise: bool) -> Result<Value, Raised> {
    match r {
        Some(k) => vm.new_int(k as i64),
        None if raise => Err(vm.err(T::ValueError, "subsection not found")),
        None => Ok(Value::int(-1).unwrap_or(Value::NONE)),
    }
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn find(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    search(vm, &a, "find", found::FIND, false)
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn rfind(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    search(vm, &a, "rfind", found::FIND, true)
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn index(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    search(vm, &a, "index", found::INDEX, false)
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn rindex(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    search(vm, &a, "rindex", found::INDEX, true)
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn count(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    search(vm, &a, "count", found::COUNT, false)
}

#[cfg_attr(mm_shipped, optimize(size))]
fn affix(vm: &mut Vm, a: &CallArgs, name: &str, prefix: bool) -> Result<Value, Raised> {
    let s = this_value(vm, a, name)?;
    let pat = vm.arg_o(a, 1);
    let tuple = pat.is_obj_kind(ty::TUPLE as u8);
    let len = vm.bytes_data(s).len();
    let (st, en) = window(vm, len, vm.arg_o(a, 2), vm.arg_o(a, 3))?;
    // Each pattern compared, as byte work and at least one unit.
    let pats = if tuple { vm.tuple_len(pat) } else { 1 };
    let bytes: usize = (0..pats)
        .filter_map(|i| {
            let p = if tuple {
                vm.vslot_o(pat, i as u32)
            } else {
                pat
            };
            vm.bytes_like(p).map(<[u8]>::len)
        })
        .sum();
    vm.burn(pats as u64 + bytes as u64 / 64);
    let s = vm.bytes_data(s);
    let n = s.len() as i64;
    for i in 0..pats {
        let p = if tuple {
            vm.vslot_o(pat, i as u32)
        } else {
            pat
        };
        let Some(pb) = vm.bytes_like(p) else {
            let msg = format!(
                "{name} first arg must be bytes or a tuple of bytes, not {}",
                vm.type_name(p)
            );
            return Err(vm.err(T::TypeError, &msg));
        };
        let pl = pb.len() as i64;
        if st > n || en - st < pl {
            continue;
        }
        let hit = if prefix {
            s[st as usize..].starts_with(pb)
        } else {
            s[..en as usize].ends_with(pb)
        };
        if hit {
            return Ok(Value::TRUE);
        }
    }
    Ok(Value::FALSE)
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn startswith(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    affix(vm, &a, "startswith", true)
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn endswith(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    affix(vm, &a, "endswith", false)
}

#[cfg_attr(mm_shipped, optimize(size))]
fn maxsplit(vm: &mut Vm, v: Value) -> Result<i64, Raised> {
    match opt(v) {
        None => Ok(-1),
        Some(x) => vm.int_arg(x, "maxsplit"),
    }
}

#[cfg_attr(mm_shipped, optimize(size))]
fn split_impl(vm: &mut Vm, a: &CallArgs, name: &str, rev: bool) -> Result<Value, Raised> {
    let source = this_value(vm, a, name)?;
    let max = maxsplit(vm, vm.arg_o(a, 2))?;
    let len = vm.bytes_data(source).len();
    let mut j = Job::new(job::SPLIT, source, 0, len, 0);
    j.w[W_A + 1] = u64::from(rev);
    j.w[W_A + 2] = if max < 0 { u64::MAX } else { max as u64 };
    if let Some(sep) = opt(vm.arg_o(a, 1)) {
        let sep = vm.need_bytes(sep)?;
        if vm.bytes_data(sep).is_empty() {
            return Err(vm.err(T::ValueError, "empty separator"));
        }
        (j.w[W_A], j.v[1]) = (cut::SEP, sep);
    } else {
        j.w[W_A] = cut::SPACE;
    }
    seek_init(&mut j.w[W_S..], len);
    vm.job_run(j)
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn split(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    split_impl(vm, &a, "split", false)
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn rsplit(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    split_impl(vm, &a, "rsplit", true)
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn join(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let sep = this_value(vm, &a, "join")?;
    let it = vm.arg_o(&a, 1);
    // A long list or tuple of bytes-likes is read in place and joined a
    // step at a time, as a str join is.
    if let Some(n) = vm.seq_items_len(it)
        && (4 * n) as u64 >= crate::strmeth::BIG
    {
        let all = vm
            .seq_words(it)
            .iter()
            .all(|&w| Value::from_bits(w).is_some_and(Vm::is_bytes_like));
        vm.burn(n as u64 / 8);
        if all {
            return crate::strmeth::join_bytes(vm, sep, it);
        }
    }
    let items = match vm.collect(it) {
        Ok(items) => items,
        Err(e) => {
            if vm.slow || vm.exc_type(vm.exc) != T::TypeError {
                return Err(e);
            }
            return Err(vm.err(T::TypeError, "can only join an iterable"));
        }
    };
    let mut len = vm
        .bytes_data(sep)
        .len()
        .saturating_mul(items.len().saturating_sub(1));
    for (i, x) in items.iter().enumerate() {
        if !Vm::is_bytes_like(*x) {
            let msg = format!(
                "sequence item {i}: expected a bytes-like object, {} found",
                vm.type_name(*x)
            );
            return Err(vm.err(T::TypeError, &msg));
        }
        len = len.saturating_add(vm.bytes_data(*x).len());
    }
    if (len / 64 + 4 * items.len()) as u64 >= crate::strmeth::BIG {
        let items = vm.alloc_tuple(&items)?;
        return crate::strmeth::join_bytes(vm, sep, items);
    }
    // Built in place: the result's block, zeros, then each part copied in.
    let result = vm.build_like(sep, len, &[])?;
    let (Some(out), seps) = (vm.data_block(result), vm.data_block(sep)) else {
        return Ok(result);
    };
    let sl = vm.bytes_data(sep).len();
    let mut o = 0;
    for (i, &x) in items.iter().enumerate() {
        if i > 0 && sl > 0 {
            if let Some(sb) = seps {
                let res = vm.heap.copy_bytes(sb, 0, out, o, sl);
                vm.hx_o(res)?;
            }
            o += sl;
        }
        let n = vm.bytes_data(x).len();
        if let Some(xb) = vm.data_block(x)
            && n > 0
        {
            let res = vm.heap.copy_bytes(xb, 0, out, o, n);
            vm.hx_o(res)?;
        }
        o += n;
    }
    vm.burn(len as u64 / 64);
    Ok(result)
}

#[cfg_attr(mm_shipped, optimize(size))]
fn strip_impl(
    vm: &mut Vm,
    a: &CallArgs,
    name: &str,
    left: bool,
    right: bool,
) -> Result<Value, Raised> {
    let source = this_value(vm, a, name)?;
    let set = match opt(vm.arg_o(a, 1)) {
        None => None,
        Some(c) => Some(vm.need_bytes(c)?),
    };
    let mut bits = [0u64; 4];
    let set = set.map_or(&b" \t\n\r\x0b\x0c"[..], |v| vm.bytes_data(v));
    let m = set.len();
    for &c in set {
        bits[usize::from(c >> 6)] |= 1 << (c & 63);
    }
    vm.burn_bytes(m);
    let n = vm.bytes_data(source).len();
    let mut j = Job::new(job::STRIP, source, 0, n, 0);
    j.w[W_A..W_A + 4].copy_from_slice(&bits);
    j.w[W_C] = u64::from(left) | u64::from(right) << 1;
    vm.job_run(j)
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn strip(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    strip_impl(vm, &a, "strip", true, true)
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn lstrip(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    strip_impl(vm, &a, "lstrip", true, false)
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn rstrip(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    strip_impl(vm, &a, "rstrip", false, true)
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn replace(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let source = this_value(vm, &a, "replace")?;
    let old = vm.need_bytes(vm.arg_o(&a, 1))?;
    let new = vm.need_bytes(vm.arg_o(&a, 2))?;
    let count = match opt(vm.arg_o(&a, 3)) {
        None => -1,
        Some(c) => vm.int_arg(c, "count")?,
    };
    let n = vm.bytes_data(source).len();
    let res = if Vm::is_bytearray(source) {
        R_BYTEARRAY
    } else {
        R_BYTES
    };
    let mut j = Job::new(job::REPLACE, source, 0, n, res);
    j.v[1] = old;
    j.v[3] = new;
    j.w[W_A] = if count < 0 { u64::MAX } else { count as u64 };
    seek_init(&mut j.w[W_S..], n);
    vm.job_run(j)
}

#[inline(never)]
#[cfg_attr(mm_shipped, optimize(size))]
fn map_bytes(vm: &mut Vm, a: &CallArgs, name: &str, how: u64) -> Result<Value, Raised> {
    let s = this_value(vm, a, name)?;
    let n = vm.bytes_data(s).len();
    let res = if Vm::is_bytearray(s) {
        R_BYTEARRAY
    } else {
        R_BYTES
    };
    let mut j = vm.job_fixed(job::MAP, s, 0, n, res, n)?;
    j.w[W_A] = how;
    vm.job_run(j)
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn upper(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    map_bytes(vm, &a, "upper", 0)
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn lower(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    map_bytes(vm, &a, "lower", 1)
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn swapcase(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    map_bytes(vm, &a, "swapcase", 2)
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn capitalize(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    map_bytes(vm, &a, "capitalize", 3)
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn title(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    map_bytes(vm, &a, "title", 4)
}

#[cfg_attr(mm_shipped, optimize(size))]
fn pred(vm: &mut Vm, a: &CallArgs, name: &str, p: u64) -> Result<Value, Raised> {
    let s = this_value(vm, a, name)?;
    let n = vm.bytes_data(s).len();
    if n == 0 {
        return Ok(Value::from_bool(p == pred::ASCII));
    }
    let mut j = Job::new(job::PRED, s, 0, n, 0);
    j.w[W_A] = p;
    vm.job_run(j)
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn isascii(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    pred(vm, &a, "isascii", pred::ASCII)
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn isalpha(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    pred(vm, &a, "isalpha", pred::ALPHA)
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn isalnum(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    pred(vm, &a, "isalnum", pred::ALNUM)
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn isdigit(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    pred(vm, &a, "isdigit", pred::DIGIT)
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn isspace(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    pred(vm, &a, "isspace", pred::SPACE)
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn islower(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    pred(vm, &a, "islower", pred::LOWER)
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn isupper(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    pred(vm, &a, "isupper", pred::UPPER)
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn istitle(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    pred(vm, &a, "istitle", pred::TITLE)
}

#[cfg_attr(mm_shipped, optimize(size))]
fn fill_arg(vm: &mut Vm, v: Value, name: &str) -> Result<u8, Raised> {
    if v == Value::UNDEFINED {
        return Ok(b' ');
    }
    match vm.bytes_like(v) {
        Some(b) if b.len() == 1 => Ok(b[0]),
        Some(b) => {
            let msg = format!(
                "{name}(): argument 2 must be a byte string of length 1, not a bytes object of length {}",
                b.len()
            );
            Err(vm.err(T::TypeError, &msg))
        }
        None => {
            let msg = format!(
                "{name}() argument 2 must be a byte string of length 1, not {}",
                vm.type_name(v)
            );
            Err(vm.err(T::TypeError, &msg))
        }
    }
}

#[cfg_attr(mm_shipped, optimize(size))]
fn pad_impl(vm: &mut Vm, a: &CallArgs, name: &str, align: u8) -> Result<Value, Raised> {
    let s = this_value(vm, a, name)?;
    let w = vm.int_arg(vm.arg_o(a, 1), "width")?;
    let fill = fill_arg(vm, vm.arg_o(a, 2), name)?;
    let w = usize::try_from(w.max(0)).unwrap_or(usize::MAX);
    let len = vm.bytes_data(s).len();
    if w <= len {
        return vm.bytes_slice(s, 0, len);
    }
    if w > (1 << 30) || !vm.heap.would_fit(w as u64) {
        return Err(vm.err(T::MemoryError, ""));
    }
    let total = w - len;
    let left = match align {
        b'<' => 0,
        b'>' => total,
        // CPython's centering: the extra fill goes left when the width is odd.
        _ => total / 2 + (total & w & 1),
    };
    let parts = [
        Part::Fill(fill, left),
        Part::Copy(s, 0, len),
        Part::Fill(fill, total - left),
    ];
    vm.build_like(s, w, &parts)
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn center(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    pad_impl(vm, &a, "center", b'^')
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn ljust(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    pad_impl(vm, &a, "ljust", b'<')
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn rjust(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    pad_impl(vm, &a, "rjust", b'>')
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn zfill(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let source = this_value(vm, &a, "zfill")?;
    let w = vm.int_arg(vm.arg_o(&a, 1), "width")?;
    let w = usize::try_from(w.max(0)).unwrap_or(usize::MAX);
    let len = vm.bytes_data(source).len();
    if w <= len {
        return vm.bytes_slice(source, 0, len);
    }
    if w > (1 << 30) || !vm.heap.would_fit(w as u64) {
        return Err(vm.err(T::MemoryError, ""));
    }
    let fill = w - len;
    let sign = match vm.bytes_data(source).first() {
        Some(&c @ (b'+' | b'-')) => [c],
        _ => [0],
    };
    let k = usize::from(sign[0] != 0);
    let parts = [
        Part::Lit(&sign[..k]),
        Part::Fill(b'0', fill),
        Part::Copy(source, k, len),
    ];
    vm.build_like(source, w, &parts)
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn removeprefix(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let source = this_value(vm, &a, "removeprefix")?;
    let p = vm.need_bytes(vm.arg_o(&a, 1))?;
    vm.burn_bytes(vm.bytes_data(p).len());
    let s = vm.bytes_data(source);
    let out = s.strip_prefix(vm.bytes_data(p)).unwrap_or(s);
    vm.bytes_slice(source, s.len() - out.len(), s.len())
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn removesuffix(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let source = this_value(vm, &a, "removesuffix")?;
    let p = vm.need_bytes(vm.arg_o(&a, 1))?;
    vm.burn_bytes(vm.bytes_data(p).len());
    let s = vm.bytes_data(source);
    let out = s.strip_suffix(vm.bytes_data(p)).unwrap_or(s);
    vm.bytes_slice(source, 0, out.len())
}

#[cfg_attr(mm_shipped, optimize(size))]
fn partition_impl(vm: &mut Vm, a: &CallArgs, name: &str, rev: bool) -> Result<Value, Raised> {
    let source = this_value(vm, a, name)?;
    let sep = vm.need_bytes(vm.arg_o(a, 1))?;
    if vm.bytes_data(sep).is_empty() {
        return Err(vm.err(T::ValueError, "empty separator"));
    }
    let len = vm.bytes_data(source).len();
    let mut j = Job::new(job::FIND, source, 0, len, 0);
    (j.w[W_A], j.w[W_A + 1], j.v[1]) = (found::PARTITION, u64::from(rev), sep);
    seek_init(&mut j.w[W_S..], len);
    vm.job_run(j)
}

impl Vm {
    /// `partition`'s (or `rpartition`'s, `rev`) three parts of `source`
    /// around `sep` found at `hit`.
    #[cfg_attr(mm_shipped, optimize(size))]
    fn parts_at(
        &mut self,
        source: Value,
        sep: Value,
        hit: Option<usize>,
        rev: bool,
    ) -> Result<Value, Raised> {
        let len = self.bytes_data(source).len();
        let ba = Vm::is_bytearray(source);
        let parts = if let Some(k) = hit {
            let sep_len = self.bytes_data(sep).len();
            let sep = if Vm::is_bytes(sep) && !ba {
                sep
            } else {
                self.bytes_slice_as(sep, ba)?
            };
            [
                self.bytes_slice(source, 0, k)?,
                sep,
                self.bytes_slice(source, k + sep_len, len)?,
            ]
        } else {
            // A bytearray's three parts are three new objects.
            let whole = self.bytes_slice(source, 0, len)?;
            let empty = self.new_kind(ba, &[])?;
            let other = if ba { self.new_kind(ba, &[])? } else { empty };
            if rev {
                [empty, other, whole]
            } else {
                [whole, empty, other]
            }
        };
        self.alloc_tuple(&parts)
    }
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn partition(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    partition_impl(vm, &a, "partition", false)
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn rpartition(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    partition_impl(vm, &a, "rpartition", true)
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn splitlines(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let source = this_value(vm, &a, "splitlines")?;
    let keep = opt(vm.arg_o(&a, 1)).is_some_and(|v| vm.truthy(v));
    let len = vm.bytes_data(source).len();
    let mut j = Job::new(job::SPLIT, source, 0, len, 0);
    (j.w[W_A], j.w[W_A + 2]) = (cut::LINES, u64::from(keep));
    vm.job_run(j)
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn decode(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let s = this_value(vm, &a, "decode")?;
    let enc = vm.codec_arg(vm.arg_o(&a, 1), "decode", "encoding")?;
    let errs = vm.codec_arg(vm.arg_o(&a, 2), "decode", "errors")?;
    vm.decode_bytes(s, enc, errs)
}

/// `str.encode(encoding, errors)`.
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn str_encode(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let s = vm.arg_o(&a, 0);
    if !Vm::is_str(s) {
        let msg = format!(
            "descriptor 'encode' for 'str' objects doesn't apply to a '{}' object",
            vm.type_name(s)
        );
        return Err(vm.err(T::TypeError, &msg));
    }
    let enc = vm.codec_arg(vm.arg_o(&a, 1), "encode", "encoding")?;
    let errs = vm.codec_arg(vm.arg_o(&a, 2), "encode", "errors")?;
    vm.encode_str(s, enc, errs, false)
}

#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn hex(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let source = this_value(vm, &a, "hex")?;
    let sep = vm.arg_o(&a, 1);
    let per = vm.arg_o(&a, 2);
    let sep: Option<char> = match opt(sep) {
        None => None,
        Some(v) => {
            let pair = if Vm::is_str(v) {
                let mut chars = vm.str_ref_o(v).chars();
                (chars.next(), chars.next())
            } else if let Some(b) = vm.bytes_like(v) {
                (
                    b.first().copied().map(char::from),
                    b.get(1).copied().map(char::from),
                )
            } else {
                let msg = format!("object of type '{}' has no len()", vm.type_name(v));
                return Err(vm.err(T::TypeError, &msg));
            };
            match pair {
                (Some(c), None) if c.is_ascii() => Some(c),
                (Some(_), None) => return Err(vm.err(T::ValueError, "sep must be ASCII.")),
                _ => return Err(vm.err(T::ValueError, "sep must be length 1.")),
            }
        }
    };
    let per: i64 = match opt(per) {
        None => 1,
        Some(v) => {
            if !Vm::is_intlike(v) {
                let msg = format!(
                    "'{}' object cannot be interpreted as an integer",
                    vm.type_name(v)
                );
                return Err(vm.err(T::TypeError, &msg));
            }
            match vm.int_of_o(v).filter(|&i| i32::try_from(i).is_ok()) {
                Some(i) => i,
                None => {
                    return Err(
                        vm.err(T::OverflowError, "Python int too large to convert to C int")
                    );
                }
            }
        }
    };
    let group = usize::try_from(per.unsigned_abs())
        .unwrap_or(usize::MAX)
        .max(1);
    let len = vm.bytes_data(source).len();
    let separators = if sep.is_some() && per != 0 {
        len.saturating_sub(1) / group
    } else {
        0
    };
    let bytes = len.saturating_mul(2).saturating_add(separators);
    let mut j = vm.job_fixed(job::HEX, source, 0, len, R_STR, bytes)?;
    j.w[W_A] = sep.map_or(0, |c| c as u64 + 1);
    j.w[W_A + 1] = per as u64;
    j.w[W_A + 2] = group as u64;
    vm.job_run(j)
}

/// `bytes.fromhex(s)` (a class method: the receiver is the type).
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn fromhex(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    // Called on an instance (`b''.fromhex`), the receiver is not a type.
    let cls = vm.arg_o(&a, 0);
    if Vm::is_type(cls) && cls != vm.type_obj(T::bytes) {
        // A subclass: `cls(bytes.fromhex(s))` in the prelude.
        return Err(vm.need_py());
    }
    let v = vm.arg_o(&a, 1);
    if !Vm::is_str(v) && !Vm::is_bytes_like(v) {
        let msg = format!(
            "fromhex() argument must be str or bytes-like, not {}",
            vm.type_name(v)
        );
        return Err(vm.err(T::TypeError, &msg));
    }
    let n = vm.job_data(v).len();
    let recv = vm.arg_o(&a, 0);
    let ba = Vm::is_bytearray(recv) || vm.type_of_typeobj(recv) == Some(T::bytearray);
    let res = if ba { R_BYTEARRAY } else { R_BYTES };
    // A pass counting the output (and finding any error), one writing it.
    vm.job_run(Job::new(job::FROMHEX, v, 0, n, res))
}

/// `bytes.translate(table, /, delete=b'')` (and `bytearray`'s).
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn translate(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let source = this_value(vm, &a, "translate")?;
    let table = match vm.arg_o(&a, 1) {
        Value::NONE => None,
        t => Some(vm.need_bytes(t)?),
    };
    if table.is_some_and(|t| vm.bytes_data(t).len() != 256) {
        return Err(vm.err(
            T::ValueError,
            "translation table must be 256 characters long",
        ));
    }
    let delete = match vm.arg_o(&a, 2) {
        Value::UNDEFINED => None,
        d => Some(vm.need_bytes(d)?),
    };
    let n = vm.bytes_data(source).len();
    let res = if Vm::is_bytearray(source) {
        R_BYTEARRAY
    } else {
        R_BYTES
    };
    let mut gone = [0u64; 4];
    if let Some(d) = delete {
        vm.burn_bytes(vm.bytes_data(d).len());
        for &c in vm.bytes_data(d) {
            gone[usize::from(c >> 6)] |= 1 << (c & 63);
        }
    }
    let mut j = if gone == [0; 4] {
        if table.is_none() {
            return vm.bytes_slice(source, 0, n);
        }
        vm.job_fixed(job::TRANSLATE, source, 0, n, res, n)?
    } else {
        Job::new(job::TRANSLATE, source, 0, n, res)
    };
    j.v[1] = table.unwrap_or(Value::NONE);
    j.w[W_A..W_A + 4].copy_from_slice(&gone);
    vm.job_run(j)
}

/// `bytes.maketrans(frm, to)` (static in CPython; bound to the type here).
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn maketrans(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let (f, t) = (
        vm.need_bytes(vm.arg_o(&a, 1))?,
        vm.need_bytes(vm.arg_o(&a, 2))?,
    );
    // Both arguments are read.
    vm.burn_bytes(vm.bytes_data(f).len() + vm.bytes_data(t).len());
    let (f, t) = (vm.bytes_data(f), vm.bytes_data(t));
    if f.len() != t.len() {
        return Err(vm.err(T::ValueError, "maketrans arguments must have same length"));
    }
    let mut tab = [0u8; 256];
    for (i, b) in tab.iter_mut().enumerate() {
        *b = i as u8;
    }
    for (&x, &y) in f.iter().zip(t) {
        tab[x as usize] = y;
    }
    vm.new_bytes(&tab)
}

/// `bytes.expandtabs(tabsize=8)`: columns count bytes, as in CPython.
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn expandtabs(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let source = this_value(vm, &a, "expandtabs")?;
    let size = match opt(vm.arg_o(&a, 1)) {
        None => 8,
        Some(v) => {
            let n = if Vm::is_heap_int(vm.unbx(v)) {
                None
            } else {
                Some(vm.int_arg(v, "tabsize")?).filter(|n| i32::try_from(*n).is_ok())
            };
            let Some(n) = n else {
                return Err(vm.err(T::OverflowError, "Python int too large to convert to C int"));
            };
            n
        }
    };
    let n = vm.bytes_data(source).len();
    let res = if Vm::is_bytearray(source) {
        R_BYTEARRAY
    } else {
        R_BYTES
    };
    // One pass for the length, one to write.
    let mut j = Job::new(job::TABS, source, 0, n, res);
    j.w[W_A] = size as u64;
    vm.job_run(j)
}

// ------------------------------------------------- chunked byte operations

/// Progress words of a [`Job`] (its `nstep::STEPS` layout adds the
/// operation and kind words).
pub(crate) const JOB_WORDS: usize = 24;

/// What a [`Job`] does (raw word 1 of its state).
pub(crate) mod job {
    /// Scans to one value ([`crate::vm::Vm::job_scan`]): `binascii`'s
    /// checksums, `hashlib` updates, the `is*` predicates.
    pub const CRC32: u8 = 0;
    pub const HQX: u8 = 1;
    pub const ADLER: u8 = 2;
    pub const HASH: u8 = 3;
    pub const PRED: u8 = 4;
    /// `find` and its family, `count`, `in`, `partition`.
    pub const FIND: u8 = 5;
    /// `split`, `rsplit`, `splitlines`: a pass counting the pieces, one
    /// making them.
    pub const SPLIT: u8 = 6;
    pub const REPLACE: u8 = 7;
    /// `*`, `*=`.
    pub const REPEAT: u8 = 8;
    /// A slice with a step.
    pub const STRIDE: u8 = 9;
    /// Window by window ([`crate::vm::Vm::job_fill`]); variable-length
    /// results take a counting pass first.
    pub const MAP: u8 = 10;
    pub const TRANSLATE: u8 = 11;
    pub const HEX: u8 = 12;
    pub const B64E: u8 = 13;
    pub const B64D: u8 = 14;
    pub const UNHEX: u8 = 15;
    pub const FROMHEX: u8 = 16;
    pub const DECODE: u8 = 17;
    pub const ENCODE: u8 = 18;
    pub const TABS: u8 = 19;
    /// A bytearray's extended slice assigned or deleted, in place.
    pub const SET_STRIDE: u8 = 20;
    pub const DEL_STRIDE: u8 = 21;
    /// `strip` and its family: the ends' bytes in a set, then a slice.
    pub const STRIP: u8 = 22;
    /// A SHAKE digest's output.
    #[cfg(feature = "hashlib")]
    pub const SQUEEZE: u8 = 23;
    /// A job that ran out of memory making many objects
    /// ([`crate::vm::Vm::job_failed`]): its next step raises `MemoryError`,
    /// after the safepoint before it collected what the job had made.
    pub const FAILED: u8 = 24;
    /// A list or tuple of ints as bytes ([`crate::vm::Vm::ints_job`]).
    pub const INTS: u8 = 25;
    /// A bytearray's bytes copied into a larger block, then the operation
    /// that grew it (`crate::bytearray::fin`).
    pub const GROW: u8 = 26;
    /// Output bytes `W_O..W_A` set to byte `W_A + 1`: `int.to_bytes`'s
    /// sign padding.
    pub const PAD: u8 = 27;
}

/// Input position, end and start (where each pass starts).
pub(crate) const W_I: usize = 0;
pub(crate) const W_END: usize = 1;
pub(crate) const W_I0: usize = 2;
/// Output bytes counted (pass 0) or written (pass 1, into the block in
/// value slot 2), and the UTF-8 chars written.
pub(crate) const W_O: usize = 3;
pub(crate) const W_CH: usize = 4;
pub(crate) const W_PASS: usize = 5;
/// What the output becomes ([`R_BYTES`] ...).
pub(crate) const W_RES: usize = 6;
/// The kind's arguments (4 words) and carries (3, reset each pass).
pub(crate) const W_A: usize = 7;
pub(crate) const W_C: usize = 11;
/// A search's words ([`seek`]).
pub(crate) const W_S: usize = 14;

pub(crate) const R_BYTES: u64 = 0;
pub(crate) const R_BYTEARRAY: u64 = 1;
pub(crate) const R_STR: u64 = 2;
/// The new contents of bytearray value slot 3 (`*=`), which is the value.
pub(crate) const R_INTO: u64 = 3;
/// Written into bytearray value slot 3 from byte `W_O` on, which it has
/// grown to hold; `None` is the value. `W_A` is its length before, which
/// an error restores.
pub(crate) const R_APPEND: u64 = 4;

/// `x` as a byte when it is a small int or bool in `range(0, 256)`.
#[cfg_attr(mm_shipped, optimize(size))]
fn small_byte(x: Value) -> Option<u8> {
    match x.as_int() {
        Some(i) => u8::try_from(i).ok(),
        None if x == Value::TRUE => Some(1),
        None if x == Value::FALSE => Some(0),
        None => None,
    }
}

/// A piece of a result [`Vm::build_kind`] makes: bytes `a..e` of a
/// bytes-like or `str` value, a run of one byte, or literal bytes.
pub(crate) enum Part<'a> {
    Copy(Value, usize, usize),
    Fill(u8, usize),
    Lit(&'a [u8]),
}

/// A byte operation whose work grows with its data, done a step at a time
/// (docs/spec.md §5, "Bounded native steps"): its natives run the first
/// step ([`Vm::job_run`]), `_more` the next ones (`nstep::op::BYTES`).
/// Results are built in place, in an arena block of their final size
/// (docs/spec.md §11), never in a Rust buffer the size of the data.
#[derive(Clone, Copy)]
pub(crate) struct Job {
    pub(crate) kind: u8,
    /// Source, operand, output (a byte block or a list), extra.
    pub(crate) v: [Value; 4],
    pub(crate) w: [u64; JOB_WORDS],
}

impl Job {
    /// A job of `kind` over `src[i..end]` making an `res`.
    #[cfg_attr(mm_shipped, optimize(size))]
    #[inline(never)]
    pub(crate) fn new(kind: u8, src: Value, i: usize, end: usize, res: u64) -> Job {
        let mut w = [0; JOB_WORDS];
        w[W_I] = i as u64;
        w[W_I0] = i as u64;
        w[W_END] = end as u64;
        w[W_RES] = res;
        Job {
            kind,
            v: [src, Value::NONE, Value::NONE, Value::NONE],
            w,
        }
    }

    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn at(&self, i: usize) -> usize {
        usize::try_from(self.w[i]).unwrap_or(usize::MAX)
    }
}

/// Search words (from [`W_S`]): the phase (0 a direct scan, 1 and 2 the
/// needle's two maximal suffixes, 3 two-way) and the next candidate.
pub(crate) const S_PH: usize = 0;
pub(crate) const S_POS: usize = 1;
/// Direct scan: the byte comparisons left before two-way takes over;
/// two-way: its memory, `u64::MAX` for a needle without one.
pub(crate) const S_MEM: usize = 2;
/// A maximal suffix's left, right, offset and period; in phase 3 the
/// critical position and period.
const S_L: usize = 3;
const S_R: usize = 4;
const S_K: usize = 5;
const S_P: usize = 6;
/// The first maximal suffix's position and period.
const S_L1: usize = 7;
const S_P1: usize = 8;
/// The needle's bytes as a set of `1 << (b & 63)`.
const S_SET: usize = 9;

/// Starts a search in `n` bytes: the direct scan may compare `2 n`.
#[cfg_attr(mm_shipped, optimize(size))]
fn seek_init(s: &mut [u64], n: usize) {
    s.fill(0);
    if let Some(m) = s.get_mut(S_MEM) {
        *m = (n as u64).saturating_mul(2);
    }
}

/// Byte `i` of `s` in search order (from the end when `rev`).
#[cfg_attr(mm_shipped, optimize(size))]
fn at(s: &[u8], i: usize, rev: bool) -> u8 {
    let k = if rev {
        s.len().wrapping_sub(1).wrapping_sub(i)
    } else {
        i
    };
    s.get(k).copied().unwrap_or(0)
}

/// Bytes `a..b` of `s` in search order, as the slice of `s` they are.
#[cfg_attr(mm_shipped, optimize(size))]
fn span(s: &[u8], a: usize, b: usize, rev: bool) -> &[u8] {
    let n = s.len();
    let r = if rev {
        n.checked_sub(b).zip(n.checked_sub(a))
    } else {
        Some((a, b))
    };
    r.and_then(|(x, y)| s.get(x..y)).unwrap_or(&[])
}

/// The first search-order index where `a` and `b` differ.
#[cfg_attr(mm_shipped, optimize(size))]
fn mismatch(a: &[u8], b: &[u8], rev: bool) -> Option<usize> {
    let n = a.len().min(b.len());
    let (a, b) = (a.get(..n)?, b.get(..n)?);
    let mut i = 0;
    while i < n {
        let k = (n - i).min(32);
        let (x, y) = if rev { (n - i - k, n - i) } else { (i, i + k) };
        if a.get(x..y) != b.get(x..y) {
            return (i..i + k).find(|&t| at(a, t, rev) != at(b, t, rev));
        }
        i += k;
    }
    None
}

/// What a [`seek`] came to.
enum Hit {
    /// A match at this search-order position (the search goes on after
    /// it).
    At(usize),
    /// No more matches.
    Gone,
    /// The work ran out (the search words hold where to go on).
    More,
}

/// The next match of `nd` in `hay` from search position `s[S_POS]` (from
/// the end when `rev`), doing at most `*work` bytes of work. A direct scan, as fast as `memchr` for most needles;
/// once its comparisons pass twice the haystack, two-way
/// (Crochemore-Perrin): linear, and no table the size of the needle.
#[inline(never)]
#[cfg_attr(mm_shipped, optimize(size))]
fn seek(hay: &[u8], nd: &[u8], rev: bool, s: &mut [u64], work: &mut usize) -> Hit {
    if s.len() < JOB_WORDS - W_S {
        return Hit::Gone;
    }
    let (n, m) = (hay.len(), nd.len());
    let mut pos = usize::try_from(s[S_POS]).unwrap_or(usize::MAX);
    let r = loop {
        if m == 0 || pos.checked_add(m).is_none_or(|e| e > n) {
            break Hit::Gone;
        }
        if *work == 0 {
            break Hit::More;
        }
        match s[S_PH] {
            0 => {
                let first = at(nd, 0, rev);
                let len = (n - m + 1 - pos).min(*work);
                let h = span(hay, pos, pos + len, rev);
                let k = if rev {
                    h.iter().rposition(|&c| c == first).map(|k| h.len() - 1 - k)
                } else {
                    h.iter().position(|&c| c == first)
                };
                let Some(k) = k else {
                    *work -= len;
                    pos += len;
                    continue;
                };
                *work -= k + 1;
                pos += k;
                if s[S_MEM] < m as u64 {
                    // Too many comparisons: two-way from here.
                    s[S_PH] = 1;
                    (s[S_L], s[S_R], s[S_K], s[S_P]) = (0, 1, 0, 1);
                    s[S_SET] = 1 << (first & 63);
                    continue;
                }
                s[S_MEM] -= m as u64;
                // A candidate costs about 15 ns [M]: a fuel unit at least.
                *work = work.saturating_sub(m.max(64));
                if span(hay, pos, pos + m, rev) == nd {
                    s[S_POS] = (pos + m) as u64;
                    return Hit::At(pos);
                }
                pos += 1;
            }
            ph @ (1 | 2) => {
                // A maximal suffix of the needle, for `<` (1) or `>` (2).
                let [mut l, mut r, mut k, mut p, l1, p1] = [S_L, S_R, S_K, S_P, S_L1, S_P1]
                    .map(|i| usize::try_from(s[i]).unwrap_or(usize::MAX));
                let mut set = s[S_SET];
                while r.saturating_add(k) < m && *work > 0 {
                    // A needle byte costs about 2.6 ns [M]: four bytes of
                    // work (a 16M needle was 20 ms a step at one).
                    *work = work.saturating_sub(4);
                    let (a, b) = (at(nd, r + k, rev), at(nd, l.saturating_add(k), rev));
                    set |= 1 << (a & 63);
                    if a != b && (a < b) == (ph == 1) {
                        r += k + 1;
                        k = 0;
                        p = r.saturating_sub(l);
                    } else if a == b {
                        if k + 1 == p {
                            r += k + 1;
                            k = 0;
                        } else {
                            k += 1;
                        }
                    } else {
                        l = r;
                        r += 1;
                        k = 0;
                        p = 1;
                    }
                }
                s[S_SET] = set;
                (s[S_L], s[S_R], s[S_K], s[S_P]) = (l as u64, r as u64, k as u64, p as u64);
                if r.saturating_add(k) < m {
                    continue;
                }
                if ph == 1 {
                    (s[S_L1], s[S_P1]) = (l as u64, p as u64);
                    let _ = (l1, p1);
                    s[S_PH] = 2;
                    (s[S_L], s[S_R], s[S_K], s[S_P]) = (0, 1, 0, 1);
                    continue;
                }
                // The critical factorization: the later suffix.
                let (crit, per) = if l1 > l { (l1, p1) } else { (l, p) };
                let periodic = crit.saturating_add(per) <= m
                    && span(nd, 0, crit, rev) == span(nd, per, per + crit, rev);
                *work = work.saturating_sub(crit);
                s[S_L] = crit as u64;
                if periodic {
                    (s[S_P], s[S_MEM]) = (per as u64, 0);
                } else {
                    (s[S_P], s[S_MEM]) = ((crit.max(m - crit.min(m)) + 1) as u64, u64::MAX);
                }
                s[S_PH] = 3;
            }
            _ => {
                let crit = usize::try_from(s[S_L]).unwrap_or(m).min(m);
                let per = usize::try_from(s[S_P]).unwrap_or(m).max(1);
                let long = s[S_MEM] == u64::MAX;
                // Two-way's memory is a prefix of the needle.
                let mem = if long {
                    0
                } else {
                    usize::try_from(s[S_MEM]).unwrap_or(m).min(m)
                };
                // An alignment costs about 20 ns [M]: a fuel unit (64
                // bytes of scanning), so a needle failing late at every
                // alignment parks within a few ms.
                *work = work.saturating_sub(64);
                if s[S_SET] & (1 << (at(hay, pos + m - 1, rev) & 63)) == 0 {
                    pos += m;
                    if !long {
                        s[S_MEM] = 0;
                    }
                    continue;
                }
                let st = if long { crit } else { crit.max(mem).min(m) };
                let right = (span(nd, st, m, rev), span(hay, pos + st, pos + m, rev));
                if let Some(i) = mismatch(right.0, right.1, rev) {
                    *work = work.saturating_sub(i);
                    pos += st + i + 1 - crit;
                    if !long {
                        s[S_MEM] = 0;
                    }
                    continue;
                }
                let lo = if long { 0 } else { mem };
                *work = work.saturating_sub(m - lo.min(st));
                if span(nd, lo, crit, rev) != span(hay, pos + lo, pos + crit, rev) {
                    pos += per;
                    if !long {
                        s[S_MEM] = m.saturating_sub(per) as u64;
                    }
                    continue;
                }
                s[S_POS] = (pos + m) as u64;
                if !long {
                    s[S_MEM] = 0;
                }
                return Hit::At(pos);
            }
        }
    };
    s[S_POS] = pos as u64;
    r
}

/// Output of a window-by-window job: counted (no block) or written into
/// a block of its final size through a stack buffer.
pub(crate) struct Out {
    blk: Option<ByteRef>,
    /// Bytes counted, or written to the block.
    at: usize,
    chars: usize,
    n: usize,
    buf: [u8; 1024],
}

impl Out {
    #[cfg_attr(mm_shipped, optimize(size))]
    fn of(j: &Job) -> Out {
        Out {
            blk: (j.w[W_PASS] != 0).then(|| j.v[2].as_byte_ref()).flatten(),
            at: j.at(W_O),
            chars: j.at(W_CH),
            n: 0,
            buf: [0; 1024],
        }
    }

    #[inline(never)]
    #[cfg_attr(mm_shipped, optimize(size))]
    fn put(&mut self, vm: &mut Vm, s: &[u8]) -> Result<(), Raised> {
        if self.blk.is_none() {
            self.at += s.len();
            return Ok(());
        }
        if self.n + s.len() > self.buf.len() {
            self.flush(vm)?;
        }
        if let Some(d) = self.buf.get_mut(self.n..self.n + s.len()) {
            d.copy_from_slice(s);
            self.n += s.len();
            return Ok(());
        }
        // Longer than the buffer (a long name escape).
        for c in s.chunks(self.buf.len()) {
            self.put(vm, c)?;
        }
        Ok(())
    }

    #[inline]
    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn byte(&mut self, vm: &mut Vm, c: u8) -> Result<(), Raised> {
        if self.blk.is_some() && self.n < self.buf.len() {
            self.buf[self.n] = c;
            self.n += 1;
            return Ok(());
        }
        self.put(vm, &[c])
    }

    /// `n` bytes `c`, at memset speed.
    #[inline(never)]
    #[cfg_attr(mm_shipped, optimize(size))]
    fn fill(&mut self, vm: &mut Vm, c: u8, n: usize) -> Result<(), Raised> {
        let Some(b) = self.blk else {
            self.at += n;
            return Ok(());
        };
        self.flush(vm)?;
        let ok = vm
            .heap
            .bytes_mut(b)
            .ok()
            .and_then(|d| d.get_mut(self.at..self.at.checked_add(n)?))
            .map(|d| d.fill(c))
            .is_some();
        if !ok {
            return Err(vm.bad_step());
        }
        self.at += n;
        if c.is_ascii() {
            self.chars += n;
        }
        Ok(())
    }

    #[inline(never)]
    #[cfg_attr(mm_shipped, optimize(size))]
    fn char(&mut self, vm: &mut Vm, ch: char) -> Result<(), Raised> {
        let mut b = [0; 4];
        self.put(vm, ch.encode_utf8(&mut b).as_bytes())
    }

    #[inline(never)]
    #[cfg_attr(mm_shipped, optimize(size))]
    fn flush(&mut self, vm: &mut Vm) -> Result<(), Raised> {
        let Some(b) = self.blk else { return Ok(()) };
        let data = &self.buf[..self.n];
        self.chars += data.iter().filter(|&&c| c & 0xC0 != 0x80).count();
        let ok = vm
            .heap
            .bytes_mut(b)
            .ok()
            .and_then(|d| d.get_mut(self.at..self.at + data.len()))
            .map(|d| d.copy_from_slice(data))
            .is_some();
        if !ok {
            return Err(vm.bad_step());
        }
        self.at += self.n;
        self.n = 0;
        Ok(())
    }

    #[cfg_attr(mm_shipped, optimize(size))]
    fn save(&mut self, vm: &mut Vm, j: &mut Job) -> Result<(), Raised> {
        self.flush(vm)?;
        j.w[W_O] = self.at as u64;
        j.w[W_CH] = self.chars as u64;
        Ok(())
    }
}

/// Input bytes a window of a [`Vm::job_fill`] job reads, and how far past
/// them it may look (a char or pair started before its end).
const WIN: usize = 512;
const LOOK: usize = 8;
/// The input bytes a step of a [`Vm::job_fill`] job reads at most (about
/// 5 ms of byte by byte work [M]).
const FILL_STEP: usize = 1 << 18;

impl Vm {
    /// Runs `j` for a step: its value if it finished, else a request to go
    /// on in `_steps` (nothing allocated for a job done in one step).
    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn job_run(&mut self, mut j: Job) -> Result<Value, Raised> {
        let start = self.work;
        if let Some(v) = self.job_go(&mut j, start)? {
            return Ok(v);
        }
        self.job_check();
        let mut raw = [0; 1 + JOB_WORDS];
        raw[0] = u64::from(j.kind);
        raw[1..].copy_from_slice(&j.w);
        self.chunk_rest(crate::nstep::op::BYTES, &j.v, &raw)
    }

    /// The next step of the job in `state` (`nstep::op::BYTES`).
    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn job_step(&mut self, state: Value) -> Result<crate::nstep::Step, Raised> {
        use crate::nstep::{Step, op};
        let got = self.nstate_get::<4, { 2 + JOB_WORDS }>(op::BYTES, state);
        let (Some((v, raw)), Some(r)) = (got, state.as_obj()) else {
            return Err(self.bad_step());
        };
        let mut w = [0; JOB_WORDS];
        w.copy_from_slice(&raw[2..]);
        let Ok(kind) = u8::try_from(raw[1]) else {
            return Err(self.bad_step());
        };
        let mut j = Job { kind, v, w };
        let start = self.work;
        if let Some(x) = self.job_go(&mut j, start)? {
            return Ok(Step::Done(x));
        }
        self.job_check();
        for (i, (&new, &old)) in j.v.iter().zip(&v).enumerate() {
            if new != old {
                self.set_slot_o(r, i as u32, new);
            }
        }
        let mut raw = [0; 1 + JOB_WORDS];
        raw[0] = u64::from(j.kind);
        raw[1..].copy_from_slice(&j.w);
        self.nstate_put(state, 1, &raw);
        Ok(Step::More)
    }

    /// A step ended: the next safepoint checks fuel, the deadline and park
    /// requests, as [`Vm::burn`] makes it after one large charge (a job's
    /// come in small ones).
    #[cfg_attr(mm_shipped, optimize(size))]
    fn job_check(&mut self) {
        let left = (self.ticks + self.tick_spare).max(0);
        self.ticks = self.ticks.min(1);
        self.tick_spare = self.tick_spare.min(1);
        self.tick_period -= left - (self.ticks + self.tick_spare).max(0);
    }

    /// Fuel units left in the step that started at `start`.
    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn job_left(&self, start: u64) -> u64 {
        crate::nstep::STEP_WORK.saturating_sub(self.work.wrapping_sub(start))
    }

    /// Runs `j` until it is done (its value) or has done a step's work
    /// since `start` (`None`).
    #[inline(never)]
    #[cfg_attr(mm_shipped, optimize(size))]
    fn job_go(&mut self, j: &mut Job, start: u64) -> Result<Option<Value>, Raised> {
        match j.kind {
            job::CRC32..=job::PRED => self.job_scan(j, start),
            job::FIND => self.job_find(j, start),
            job::SPLIT => self.job_split(j, start),
            job::REPLACE => self.job_replace(j, start),
            job::REPEAT => self.job_repeat(j, start),
            job::STRIDE => self.job_stride(j, start),
            job::MAP..=job::TABS => self.job_fill(j, start),
            job::SET_STRIDE | job::DEL_STRIDE => self.ba_stride_step(j, start),
            job::STRIP => self.job_strip(j, start),
            #[cfg(feature = "hashlib")]
            job::SQUEEZE => self.job_squeeze(j, start),
            job::FAILED => Err(self.err(T::MemoryError, "")),
            job::INTS => self.job_ints(j, start),
            job::GROW => self.ba_grow_step(j, start),
            job::PAD => self.job_pad(j, start),
            _ => Err(self.bad_step()),
        }
    }

    /// The data a job reads: a `str`'s UTF-8 or bytes-like data.
    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn job_data(&self, v: Value) -> &[u8] {
        if Vm::is_str(v) {
            self.str_bytes(v)
        } else {
            self.bytes_data(v)
        }
    }

    /// The byte block of bytes-like `v`.
    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn data_block(&self, v: Value) -> Option<ByteRef> {
        self.vslot_o(v, 0).as_byte_ref()
    }

    /// A new output block of `len` bytes, charged as the result it
    /// becomes; `MemoryError` when it cannot fit.
    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn job_block(&mut self, len: usize) -> Result<ByteRef, Raised> {
        // Refused before it is charged: past the largest object, a block
        // that fits the limit was charged as written (240 MB of
        // `expandtabs`: 3.75M units in one slice) and then refused.
        if len as u64 > mm_core::MAX_OBJECT_BYTES || !self.heap.would_fit(len as u64) {
            return Err(self.err(T::MemoryError, ""));
        }
        if len >= 64 {
            self.burn_result(len, true);
        }
        let res = self.heap.alloc_bytes_zeroed(len);
        self.hx_o(res)
    }

    /// Job `j`'s filled output block `b` as its result (`W_RES`), holding
    /// `chars` UTF-8 chars.
    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn job_wrap(&mut self, j: &Job, b: ByteRef, chars: usize) -> Result<Value, Raised> {
        let len = self.heap.bytes(b).map_or(0, <[u8]>::len);
        let (t, raw) = match j.w[W_RES] {
            R_STR => {
                if crate::vm::shared_char(self.heap.bytes(b).unwrap_or(&[])) {
                    // Interned, as `new_str` makes them.
                    let s =
                        core::str::from_utf8(self.heap.bytes(b).unwrap_or(&[])).unwrap_or_default();
                    let s: String = s.into();
                    return self.intern(&s);
                }
                let res = self.heap.str_from_block(b, chars as u64);
                let r = self.hx_o(res)?;
                return Ok(Value::from_ref(r, ty::STR as u8));
            }
            R_INTO => {
                self.ba_adopt(j.v[3], b, len);
                return Ok(j.v[3]);
            }
            R_BYTEARRAY => (ty::BYTEARRAY, &[len as u64][..]),
            _ => (ty::BYTES, &[][..]),
        };
        let res = self.heap.alloc_init(t, 0, &[Value::from_byte_ref(b)], raw);
        let r = self.hx_o(res)?;
        Ok(Value::from_ref(r, t as u8))
    }
}

/// The `is*` predicates of a [`job::PRED`] job (`W_A`).
pub(crate) mod pred {
    pub const ASCII: u64 = 0;
    pub const ALPHA: u64 = 1;
    pub const ALNUM: u64 = 2;
    pub const DIGIT: u64 = 3;
    pub const SPACE: u64 = 4;
    pub const LOWER: u64 = 5;
    pub const UPPER: u64 = 6;
    pub const TITLE: u64 = 7;
}

/// Feeds `d` to predicate `p` (carry `acc`): its value once `d` decides it.
#[cfg_attr(mm_shipped, optimize(size))]
fn pred_feed(p: u64, acc: &mut u64, d: &[u8]) -> Option<bool> {
    let all = |f: fn(&u8) -> bool| (!d.iter().all(f)).then_some(false);
    match p {
        pred::ASCII => (!d.is_ascii()).then_some(false),
        pred::ALPHA => all(u8::is_ascii_alphabetic),
        pred::ALNUM => all(u8::is_ascii_alphanumeric),
        pred::DIGIT => all(u8::is_ascii_digit),
        pred::SPACE => all(|c| is_space(*c)),
        pred::LOWER | pred::UPPER => {
            type Is = fn(&u8) -> bool;
            let (yes, no): (Is, Is) = if p == pred::LOWER {
                (u8::is_ascii_lowercase, u8::is_ascii_uppercase)
            } else {
                (u8::is_ascii_uppercase, u8::is_ascii_lowercase)
            };
            if d.iter().any(no) {
                return Some(false);
            }
            if d.iter().any(yes) {
                *acc = 1;
            }
            None
        }
        _ => {
            // Bit 0: a cased byte seen; bit 1: the last byte was cased.
            let (mut cased, mut prev) = (*acc & 1 != 0, *acc & 2 != 0);
            for &c in d {
                if c.is_ascii_uppercase() {
                    if prev {
                        return Some(false);
                    }
                    (prev, cased) = (true, true);
                } else if c.is_ascii_lowercase() {
                    if !prev {
                        return Some(false);
                    }
                    (prev, cased) = (true, true);
                } else {
                    prev = false;
                }
            }
            *acc = u64::from(cased) | u64::from(prev) << 1;
            None
        }
    }
}

/// What a [`job::FIND`] job gives (`W_A`); `W_A + 1` is set for a search
/// from the end, `W_A + 2` holds an int needle (value slot 1 `None`).
pub(crate) mod found {
    pub const FIND: u64 = 0;
    pub const INDEX: u64 = 1;
    pub const COUNT: u64 = 2;
    pub const IN: u64 = 3;
    pub const NOT_IN: u64 = 4;
    pub const PARTITION: u64 = 5;
}

/// A [`job::SPLIT`] job's pieces (`W_A`): at a separator (value slot 1),
/// at whitespace, at line ends; `W_A + 1` from the end, `W_A + 2` the most
/// splits (or `keepends`).
pub(crate) mod cut {
    pub const SEP: u64 = 0;
    pub const SPACE: u64 = 1;
    pub const LINES: u64 = 2;
}

/// What [`Vm::split_next`] came to.
enum Piece {
    /// A piece, from and to in search order.
    At(usize, usize),
    /// No more pieces.
    End,
    /// The work ran out.
    More,
}

/// The longest run of `s[a..b]` in search order whose bytes pass `f`.
#[cfg_attr(mm_shipped, optimize(size))]
fn run_len(s: &[u8], a: usize, b: usize, rev: bool, f: impl Fn(u8) -> bool) -> usize {
    let h = span(s, a, b, rev);
    let r = if rev {
        h.iter().rev().position(|&c| !f(c))
    } else {
        h.iter().position(|&c| !f(c))
    };
    r.unwrap_or(h.len())
}

/// How a window of a [`Vm::job_fill`] kind ended.
pub(crate) enum Feed {
    /// The input it took (whole units, at least one).
    Used(usize),
    /// None, but the state went on: part of a wide tab's spaces.
    Again,
    /// The rest of the input is not read.
    #[cfg_attr(
        not(feature = "binascii"),
        expect(dead_code, reason = "made by binascii's decoder")
    )]
    Stop,
    /// The job's value: a `binascii` error message.
    Msg(&'static str),
}

/// The most spaces a wide tab writes at once (16 Ki fuel units): a tab
/// of 60M spaces was one 27 ms step charged 1.87M units, and several past
/// the memory limit were counted in one 7.5M-unit slice [M].
const TAB_SLICE: usize = 1 << 20;

/// Input bytes a [`Vm::job_fill`] kind reads per fuel unit.
#[cfg_attr(mm_shipped, optimize(size))]
fn fill_rate(kind: u8) -> u64 {
    match kind {
        job::TRANSLATE | job::TABS => 1,
        job::B64E | job::B64D | job::FROMHEX => 32,
        _ => 64,
    }
}

const DIGITS: &[u8; 16] = b"0123456789abcdef";

/// A [`Codec`] as a job word, and back.
#[cfg_attr(mm_shipped, optimize(size))]
fn codec_word(c: Codec) -> u64 {
    let (k, be, bom) = match c {
        Codec::Utf8 => (0, false, false),
        Codec::Utf8Sig => (1, false, false),
        Codec::Ascii => (2, false, false),
        Codec::Latin1 => (3, false, false),
        Codec::Utf16 { be, bom } => (4, be, bom),
        Codec::Utf32 { be, bom } => (5, be, bom),
    };
    k | u64::from(be) << 8 | u64::from(bom) << 9
}

#[cfg_attr(mm_shipped, optimize(size))]
fn word_codec(w: u64) -> Codec {
    let (be, bom) = (w & 1 << 8 != 0, w & 1 << 9 != 0);
    match w & 0xff {
        1 => Codec::Utf8Sig,
        2 => Codec::Ascii,
        3 => Codec::Latin1,
        4 => Codec::Utf16 { be, bom },
        5 => Codec::Utf32 { be, bom },
        _ => Codec::Utf8,
    }
}

const HANDLERS: [Errors; 9] = [
    Errors::Strict,
    Errors::SurrogateEscape,
    Errors::SurrogatePass,
    Errors::Replace,
    Errors::Ignore,
    Errors::BackslashReplace,
    Errors::XmlCharRefReplace,
    Errors::NameReplace,
    Errors::Unknown,
];

#[cfg_attr(mm_shipped, optimize(size))]
fn word_errors(w: u64) -> Errors {
    HANDLERS
        .get(usize::try_from(w).unwrap_or(usize::MAX))
        .copied()
        .unwrap_or(Errors::Unknown)
}

impl Vm {
    /// Scans `src[i..end]` to one value ([`job::CRC32`] to [`job::PRED`]),
    /// carrying it in `W_C`.
    #[cfg_attr(mm_shipped, optimize(size))]
    fn job_scan(&mut self, j: &mut Job, start: u64) -> Result<Option<Value>, Raised> {
        let rate: u64 = match j.kind {
            job::HQX => 8,
            job::HASH => 16,
            job::CRC32 | job::ADLER => 32,
            _ => 64,
        };
        let mut acc = j.w[W_C];
        let early = loop {
            let (i, end) = (j.at(W_I), j.at(W_END));
            if end > self.job_data(j.v[0]).len() {
                return Err(self.bad_step());
            }
            let left = usize::try_from(self.job_left(start) * rate).unwrap_or(usize::MAX);
            let n = end.saturating_sub(i).min(left);
            if n == 0 {
                break None;
            }
            let d = self.job_data(j.v[0]).get(i..i + n).unwrap_or(&[]);
            let r = match j.kind {
                job::PRED => pred_feed(j.w[W_A], &mut acc, d),
                #[cfg(feature = "binascii")]
                k @ (job::CRC32 | job::HQX | job::ADLER) => {
                    crate::binascii::crc_feed(k, &mut acc, d);
                    None
                }
                #[cfg(feature = "hashlib")]
                job::HASH => {
                    crate::hashlib::feed(self, j.v[1], j.v[0], i, n)?;
                    None
                }
                _ => return Err(self.bad_step()),
            };
            self.burn(n as u64 / rate);
            j.w[W_I] = (i + n) as u64;
            if r.is_some() {
                break r;
            }
        };
        j.w[W_C] = acc;
        if early.is_none() && j.w[W_I] < j.w[W_END] {
            return Ok(None);
        }
        Ok(Some(match j.kind {
            job::HASH => Value::NONE,
            job::PRED => Value::from_bool(early.unwrap_or(match j.w[W_A] {
                pred::LOWER | pred::UPPER | pred::TITLE => acc & 1 != 0,
                _ => true,
            })),
            #[cfg(feature = "binascii")]
            k @ (job::CRC32 | job::HQX | job::ADLER) => {
                self.new_int(crate::binascii::crc_end(k, acc))?
            }
            _ => Value::NONE,
        }))
    }

    /// A [`job::FIND`] job: searches `src[i0..end]` for value slot 1.
    /// `src` may be a str ([`find_job`]): its UTF-8, a match of whose
    /// needle is a match of chars.
    #[cfg_attr(mm_shipped, optimize(size))]
    fn job_find(&mut self, j: &mut Job, start: u64) -> Result<Option<Value>, Raised> {
        let (mode, rev) = (j.w[W_A], j.w[W_A + 1] != 0);
        let (st, en) = (j.at(W_I0), j.at(W_END));
        let one = [j.w[W_A + 2] as u8];
        let hit = loop {
            let mut work = usize::try_from(self.job_left(start) * 64).unwrap_or(usize::MAX);
            let before = work;
            let Some(hay) = find_data(self, j.v[0]).get(st..en) else {
                return Err(self.bad_step());
            };
            let nd = if j.v[1] == Value::NONE {
                &one[..]
            } else {
                find_data(self, j.v[1])
            };
            let m = nd.len();
            let r = seek(hay, nd, rev, &mut j.w[W_S..], &mut work);
            self.burn((before - work).div_ceil(64) as u64);
            match r {
                Hit::More => return Ok(None),
                Hit::At(_) if mode == found::COUNT => j.w[W_C] = j.w[W_C].saturating_add(1),
                Hit::At(p) => break Some(if rev { en - p - m } else { st + p }),
                Hit::Gone => break None,
            }
        };
        if Vm::is_str(j.v[0]) && mode <= found::INDEX {
            return crate::strmeth::found_char(self, j.v[0], hit, mode == found::INDEX).map(Some);
        }
        Ok(Some(match mode {
            found::COUNT => self.new_int(j.w[W_C] as i64)?,
            found::IN => Value::from_bool(hit.is_some()),
            found::NOT_IN => Value::from_bool(hit.is_none()),
            found::PARTITION => self.parts_at(j.v[0], j.v[1], hit, rev)?,
            _ => found(self, hit, mode == found::INDEX)?,
        }))
    }

    /// The next piece of split job `j`, in search order.
    #[cfg_attr(mm_shipped, optimize(size))]
    fn split_next(&self, j: &mut Job, work: &mut usize) -> Piece {
        let d = self.bytes_data(j.v[0]);
        let n = d.len();
        let (rev, limit) = (j.w[W_A + 1] != 0, j.w[W_A + 2]);
        if j.w[W_C + 1] != 0 {
            return Piece::End;
        }
        let a = j.at(W_I).min(n);
        match j.w[W_A] {
            cut::SEP => {
                if j.w[W_C] < limit {
                    let sep = self.bytes_data(j.v[1]);
                    match seek(d, sep, rev, &mut j.w[W_S..], work) {
                        Hit::More => return Piece::More,
                        Hit::At(p) => {
                            j.w[W_I] = (p + sep.len()) as u64;
                            return Piece::At(a, p.max(a));
                        }
                        Hit::Gone => {}
                    }
                }
                j.w[W_C + 1] = 1;
                Piece::At(a, n)
            }
            cut::SPACE => {
                if j.w[W_C + 2] == 0 {
                    let len = (n - a).min(*work);
                    let k = run_len(d, a, a + len, rev, is_space);
                    *work -= k;
                    j.w[W_I] = (a + k) as u64;
                    if k == len && a + k < n {
                        return Piece::More;
                    }
                    if a + k >= n {
                        j.w[W_C + 1] = 1;
                        return Piece::End;
                    }
                    if j.w[W_C] >= limit {
                        j.w[W_C + 1] = 1;
                        return Piece::At(a + k, n);
                    }
                    j.w[W_C + 2] = 1;
                    j.w[W_S + S_POS] = (a + k) as u64;
                }
                let (a, q) = (j.at(W_I), j.at(W_S + S_POS).min(n));
                let len = (n - q).min(*work);
                let k = run_len(d, q, q + len, rev, |c| !is_space(c));
                *work -= k;
                j.w[W_S + S_POS] = (q + k) as u64;
                if k == len && q + k < n {
                    return Piece::More;
                }
                j.w[W_C + 2] = 0;
                j.w[W_I] = (q + k) as u64;
                Piece::At(a, q + k)
            }
            _ => {
                if a >= n {
                    return Piece::End;
                }
                let q = j.at(W_S + S_POS).clamp(a, n);
                let len = (n - q).min(*work);
                let at = d
                    .get(q..q + len)
                    .and_then(|h| h.iter().position(|&c| c == b'\r' || c == b'\n'));
                *work -= at.map_or(len, |k| k + 1);
                let Some(k) = at else {
                    if q + len < n {
                        j.w[W_S + S_POS] = (q + len) as u64;
                        return Piece::More;
                    }
                    j.w[W_I] = n as u64;
                    return Piece::At(a, n);
                };
                let end = q + k;
                let eol = if d.get(end) == Some(&b'\r') && d.get(end + 1) == Some(&b'\n') {
                    2
                } else {
                    1
                };
                j.w[W_I] = (end + eol) as u64;
                j.w[W_S + S_POS] = (end + eol) as u64;
                Piece::At(a, if limit != 0 { end + eol } else { end })
            }
        }
    }

    /// A [`job::SPLIT`] job: a pass counting the pieces (into `W_O`), then
    /// one making them into a list of that many (value slot 2).
    #[cfg_attr(mm_shipped, optimize(size))]
    fn job_split(&mut self, j: &mut Job, start: u64) -> Result<Option<Value>, Raised> {
        let rev = j.w[W_A + 1] != 0;
        loop {
            if j.w[W_PASS] != 0 && j.v[2] == Value::NONE {
                let count = j.at(W_O);
                match self.list_with_room(count, count) {
                    Ok(l) => j.v[2] = l,
                    Err(_) => return Ok(self.job_failed(j)),
                }
            }
            let mut work = usize::try_from(self.job_left(start) * 64).unwrap_or(usize::MAX);
            let before = work;
            let r = self.split_next(j, &mut work);
            self.burn((before - work).div_ceil(64) as u64);
            match r {
                Piece::More => return Ok(None),
                Piece::End if j.w[W_PASS] == 0 => {
                    let count = j.at(W_C);
                    j.w[W_O] = count as u64;
                    j.w[W_PASS] = 1;
                    j.w[W_I] = 0;
                    j.w[W_C..].fill(0);
                    let n = self.bytes_data(j.v[0]).len();
                    seek_init(&mut j.w[W_S..], n);
                    // Many pieces are made in steps of their own, so the
                    // job's state exists for `job_failed` to drop them from.
                    if count >= 1024 {
                        return Ok(None);
                    }
                }
                Piece::End => return Ok(Some(j.v[2])),
                Piece::At(a, b) => {
                    let k = j.at(W_C);
                    j.w[W_C] += 1;
                    self.burn(1);
                    if j.w[W_PASS] != 0 {
                        let n = self.bytes_data(j.v[0]).len();
                        let (x, y) = if rev { (n - b, n - a) } else { (a, b) };
                        let Ok(piece) = self.bytes_slice(j.v[0], x, y) else {
                            return Ok(self.job_failed(j));
                        };
                        let at = if rev {
                            j.at(W_O).wrapping_sub(k + 1)
                        } else {
                            k
                        };
                        match (self.list_buf(j.v[2]), u32::try_from(at)) {
                            (Some(buf), Ok(at)) if at < self.obj_len(buf) => {
                                self.set_slot_o(buf, at, piece);
                            }
                            _ => return Err(self.bad_step()),
                        }
                    }
                }
            }
        }
    }

    /// `j` ran out of memory making its result: drops what it made and
    /// becomes a [`job::FAILED`] job, whose `MemoryError` comes a step
    /// later, once the safepoint between collected that (a handler then
    /// starts with the memory free, as it would after one failed block).
    #[cfg_attr(mm_shipped, optimize(size))]
    fn job_failed(&mut self, j: &mut Job) -> Option<Value> {
        j.v = [Value::NONE; 4];
        j.kind = job::FAILED;
        self.request_gc();
        None
    }

    /// Copies up to `room` bytes of bytes-like `from[a..b]` into block `out`
    /// at `*o`: how many it copied, charged as byte work.
    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn copy_part(
        &mut self,
        from: Value,
        a: usize,
        b: usize,
        out: ByteRef,
        o: &mut usize,
        room: usize,
    ) -> Result<usize, Raised> {
        let n = b.saturating_sub(a).min(room);
        if n == 0 {
            return Ok(0);
        }
        let ok = b <= self.job_data(from).len()
            && self
                .data_block(from)
                .is_some_and(|src| self.heap.copy_bytes(src, a, out, *o, n).is_ok());
        if !ok {
            return Err(self.bad_step());
        }
        *o += n;
        self.burn(n.div_ceil(64) as u64);
        Ok(n)
    }

    /// Byte work left in this step.
    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn job_room(&self, start: u64) -> usize {
        usize::try_from(self.job_left(start) * 64).unwrap_or(usize::MAX)
    }

    /// A [`job::REPLACE`] job: value slot 1 (`old`) by slot 3 (`new`), at
    /// most `W_A` times; a pass counting the matches, one copying.
    #[cfg_attr(mm_shipped, optimize(size))]
    fn job_replace(&mut self, j: &mut Job, start: u64) -> Result<Option<Value>, Raised> {
        let (src, old, new) = (j.v[0], j.v[1], j.v[3]);
        let n = self.bytes_data(src).len();
        let m = self.bytes_data(old).len();
        if j.w[W_PASS] == 0 {
            if m == 0 {
                j.w[W_C] = j.w[W_A].min(n as u64 + 1);
            }
            while j.w[W_C] < j.w[W_A] && m > 0 {
                let mut work = self.job_room(start);
                let before = work;
                let r = seek(
                    self.bytes_data(src),
                    self.bytes_data(old),
                    false,
                    &mut j.w[W_S..],
                    &mut work,
                );
                self.burn((before - work).div_ceil(64) as u64);
                match r {
                    Hit::More => return Ok(None),
                    Hit::Gone => break,
                    Hit::At(_) => j.w[W_C] += 1,
                }
            }
            let k = j.at(W_C);
            let nl = self.bytes_data(new).len();
            let Some(len) = k
                .checked_mul(nl)
                .and_then(|x| (n - k.saturating_mul(m).min(n)).checked_add(x))
            else {
                return Err(self.err(T::MemoryError, ""));
            };
            let b = self.job_block(len)?;
            j.v[2] = Value::from_byte_ref(b);
            (j.w[W_PASS], j.w[W_A + 1], j.w[W_O]) = (1, k as u64, 0);
            j.w[W_C..].fill(0);
            seek_init(&mut j.w[W_S..], n);
        }
        let Some(out) = j.v[2].as_byte_ref() else {
            return Err(self.bad_step());
        };
        let mut o = j.at(W_O);
        let r = loop {
            let room = self.job_room(start);
            if room == 0 {
                break None;
            }
            let i = j.at(W_I);
            if j.w[W_C] >= j.w[W_A + 1] {
                let c = self.copy_part(src, i, n, out, &mut o, room)?;
                j.w[W_I] += c as u64;
                if i + c >= n {
                    break Some(());
                }
                continue;
            }
            if j.w[W_C + 1] == 0 {
                let p = if m == 0 {
                    j.at(W_C)
                } else {
                    let mut work = room;
                    let r = seek(
                        self.bytes_data(src),
                        self.bytes_data(old),
                        false,
                        &mut j.w[W_S..],
                        &mut work,
                    );
                    self.burn((room - work).div_ceil(64) as u64);
                    match r {
                        Hit::More => continue,
                        Hit::Gone => return Err(self.bad_step()),
                        Hit::At(p) => p,
                    }
                };
                j.w[W_C + 1] = p as u64 + 1;
            }
            let p = j.at(W_C + 1) - 1;
            if i < p {
                let c = self.copy_part(src, i, p, out, &mut o, room)?;
                j.w[W_I] += c as u64;
                continue;
            }
            let (done, nl) = (j.at(W_C + 2), self.bytes_data(new).len());
            if done < nl {
                let c = self.copy_part(new, done, nl, out, &mut o, room)?;
                j.w[W_C + 2] += c as u64;
                continue;
            }
            self.burn(1);
            j.w[W_I] = (p + m) as u64;
            j.w[W_C] += 1;
            (j.w[W_C + 1], j.w[W_C + 2]) = (0, 0);
        };
        j.w[W_O] = o as u64;
        match r {
            None => Ok(None),
            Some(()) => self.job_wrap(j, out, 0).map(Some),
        }
    }

    /// A [`job::STRIP`] job: `W_I..W_END` shrinks from the left while bit
    /// 0 of `W_C` is set, then from the right while bit 1 is, by the bytes
    /// in the set at `W_A`.
    #[cfg_attr(mm_shipped, optimize(size))]
    fn job_strip(&mut self, j: &mut Job, start: u64) -> Result<Option<Value>, Raised> {
        let set: [u64; 4] = [j.w[W_A], j.w[W_A + 1], j.w[W_A + 2], j.w[W_A + 3]];
        let inset = |c: u8| set[usize::from(c >> 6)] >> (c & 63) & 1 != 0;
        let n = self.bytes_data(j.v[0]).len();
        loop {
            let (st, en) = (j.at(W_I), j.at(W_END).min(n));
            let side = j.w[W_C] & 3;
            if side == 0 || st >= en {
                return self.bytes_slice(j.v[0], st.min(en), en).map(Some);
            }
            let room = self.job_room(start);
            if room == 0 {
                return Ok(None);
            }
            let len = (en - st).min(room);
            let d = self.bytes_data(j.v[0]);
            let k = if side & 1 != 0 {
                let k = run_len(d, st, st + len, false, inset);
                j.w[W_I] = (st + k) as u64;
                k
            } else {
                let k = run_len(d, n - en, n - en + len, true, inset);
                j.w[W_END] = (en - k) as u64;
                k
            };
            if k < len {
                // This end is done.
                j.w[W_C] &= if side & 1 != 0 { !1 } else { !2 };
            }
            self.burn((k + 1).min(len).div_ceil(64) as u64);
        }
    }

    /// A [`job::INTS`] job: items `W_I..W_END` of a list, tuple or set
    /// (value slot 0; [`Vm::ints_words`]) as bytes from `W_O` on of the block (value slot 2), or of
    /// the bytearray in value slot 3 ([`R_APPEND`]); a fuel unit an item
    /// or slot, as `collect` charges.
    #[cfg_attr(mm_shipped, optimize(size))]
    fn job_ints(&mut self, j: &mut Job, start: u64) -> Result<Option<Value>, Raised> {
        let res = j.w[W_RES];
        let (ba, name) = match res {
            R_BYTES => (false, "bytes"),
            R_BYTEARRAY => (true, "bytearray"),
            _ => (true, "extend"),
        };
        loop {
            let (i, end) = (j.at(W_I), j.at(W_END));
            let out = if res == R_APPEND {
                self.data_block(j.v[3])
            } else {
                j.v[2].as_byte_ref()
            };
            let Some(out) = out else {
                return Err(self.bad_step());
            };
            if i >= end {
                if res == R_APPEND {
                    return Ok(Some(Value::NONE));
                }
                return self.job_wrap(j, out, 0).map(Some);
            }
            let left = usize::try_from(self.job_left(start)).unwrap_or(usize::MAX);
            if left == 0 {
                return Ok(None);
            }
            let mut buf = [0u8; 1024];
            let n = (end - i).min(buf.len()).min(left);
            // `k` slots read, `m` bytes made (a set's empty slots make none).
            let (mut k, mut m) = (0, 0);
            while k < n {
                let words = self.ints_words(j.v[0]).get(i + k..i + n).unwrap_or(&[]);
                for &w in words {
                    let x = Value::from_bits(w);
                    if x == Some(Value::UNDEFINED) {
                        k += 1;
                        continue;
                    }
                    let Some(c) = x.and_then(small_byte) else {
                        break;
                    };
                    buf[m] = c;
                    (k, m) = (k + 1, m + 1);
                }
                if k < n {
                    // Not a small int: a big int, `__index__` or an error.
                    let x = self.ints_words(j.v[0]).get(i + k).copied();
                    let r = match x.and_then(Value::from_bits) {
                        Some(x) => self.int_byte(x, ba, Some(name)),
                        None => Err(self.bad_step()),
                    };
                    match r {
                        Ok(c) => {
                            buf[m] = c;
                            (k, m) = (k + 1, m + 1);
                        }
                        Err(e) => {
                            if res == R_APPEND {
                                self.ba_resize(j.v[3], j.at(W_A), usize::MAX)?;
                            }
                            return Err(e);
                        }
                    }
                }
            }
            let o = j.at(W_O);
            let ok = self
                .heap
                .bytes_mut(out)
                .ok()
                .and_then(|d| d.get_mut(o..o + m))
                .map(|d| d.copy_from_slice(&buf[..m]))
                .is_some();
            if !ok {
                return Err(self.bad_step());
            }
            self.burn(n as u64);
            (j.w[W_I], j.w[W_O]) = ((i + n) as u64, (o + m) as u64);
        }
    }

    /// A [`job::REPEAT`] job: the block (value slot 2) filled with copies
    /// of the source, doubling what it holds.
    #[cfg_attr(mm_shipped, optimize(size))]
    fn job_repeat(&mut self, j: &mut Job, start: u64) -> Result<Option<Value>, Raised> {
        let Some(out) = j.v[2].as_byte_ref() else {
            return Err(self.bad_step());
        };
        let total = self.heap.bytes(out).map_or(0, <[u8]>::len);
        let mut o = j.at(W_O);
        while o < total {
            let room = self.job_room(start);
            if room == 0 {
                j.w[W_O] = o as u64;
                return Ok(None);
            }
            let len = self.job_data(j.v[0]).len();
            if o < len {
                self.copy_part(j.v[0], o, len, out, &mut o, room)?;
                continue;
            }
            // From the same phase of the period: a step may have ended
            // with `o` not a multiple of it.
            let r = o % len.max(1);
            let n = (total - o).min(o - r).min(room);
            let res = self.heap.copy_bytes(out, r, out, o, n);
            self.hx_o(res)?;
            self.burn(n.div_ceil(64) as u64);
            o += n;
        }
        self.job_wrap(j, out, 0).map(Some)
    }

    /// A [`job::PAD`] job: the output's bytes `W_O..W_A` set to byte
    /// `W_A + 1`, as many a step as its room.
    #[cfg_attr(mm_shipped, optimize(size))]
    fn job_pad(&mut self, j: &mut Job, start: u64) -> Result<Option<Value>, Raised> {
        let Some(out) = j.v[2].as_byte_ref() else {
            return Err(self.bad_step());
        };
        let (end, c) = (j.at(W_A), j.w[W_A + 1] as u8);
        let mut o = j.at(W_O);
        while o < end {
            let room = self.job_room(start);
            if room == 0 {
                j.w[W_O] = o as u64;
                return Ok(None);
            }
            let n = (end - o).min(room);
            let Some(d) = self
                .heap
                .bytes_mut(out)
                .ok()
                .and_then(|d| d.get_mut(o..o + n))
            else {
                return Err(self.bad_step());
            };
            d.fill(c);
            self.burn(n.div_ceil(64) as u64);
            o += n;
        }
        self.job_wrap(j, out, 0).map(Some)
    }

    /// A [`job::STRIDE`] job: byte `start + k * step` (`W_A`, `W_A + 1`)
    /// of the source for each `k` of the block.
    #[cfg_attr(mm_shipped, optimize(size))]
    fn job_stride(&mut self, j: &mut Job, start: u64) -> Result<Option<Value>, Raised> {
        let Some(out) = j.v[2].as_byte_ref() else {
            return Err(self.bad_step());
        };
        let total = self.heap.bytes(out).map_or(0, <[u8]>::len);
        let (first, step) = (j.w[W_A] as i64, j.w[W_A + 1] as i64);
        let mut o = j.at(W_O);
        while o < total {
            // Reads far apart: a fuel unit per 8.
            let left = usize::try_from(self.job_left(start) * 8).unwrap_or(usize::MAX);
            if left == 0 {
                j.w[W_O] = o as u64;
                return Ok(None);
            }
            let mut buf = [0u8; 1024];
            let n = (total - o).min(buf.len()).min(left);
            let d = self.bytes_data(j.v[0]);
            for (t, b) in buf[..n].iter_mut().enumerate() {
                let at = first.wrapping_add(((o + t) as i64).wrapping_mul(step));
                *b = usize::try_from(at)
                    .ok()
                    .and_then(|k| d.get(k))
                    .copied()
                    .unwrap_or(0);
            }
            let ok = self
                .heap
                .bytes_mut(out)
                .ok()
                .and_then(|d| d.get_mut(o..o + n))
                .map(|d| d.copy_from_slice(&buf[..n]))
                .is_some();
            if !ok {
                return Err(self.bad_step());
            }
            self.burn(n.div_ceil(8) as u64);
            o += n;
        }
        self.job_wrap(j, out, 0).map(Some)
    }

    /// A window-by-window job ([`job::MAP`] to [`job::TABS`]): each window
    /// of input, read into a stack buffer, becomes output (counted in pass
    /// 0, written in pass 1).
    #[cfg_attr(mm_shipped, optimize(size))]
    fn job_fill(&mut self, j: &mut Job, start: u64) -> Result<Option<Value>, Raised> {
        let rate = fill_rate(j.kind);
        let mut out = Out::of(j);
        // Byte by byte work: at most `FILL_STEP` input bytes a step.
        let mut first = j.at(W_I);
        loop {
            let (i, end) = (j.at(W_I), j.at(W_END));
            if i >= end {
                if let Some(v) = self.fill_end(j, &mut out)? {
                    return Ok(Some(v));
                }
                if let Some(b) = out.blk {
                    out.flush(self)?;
                    return self.job_wrap(j, b, out.chars).map(Some);
                }
                let b = self.job_block(out.at)?;
                j.v[2] = Value::from_byte_ref(b);
                (j.w[W_PASS], j.w[W_I], j.w[W_O], j.w[W_CH]) = (1, j.w[W_I0], 0, 0);
                j.w[W_C..W_S].fill(0);
                out = Out::of(j);
                first = j.at(W_I);
                continue;
            }
            let left = self.job_left(start);
            let cap = FILL_STEP.saturating_sub(i - first);
            if left == 0 || cap == 0 {
                out.save(self, j)?;
                return Ok(None);
            }
            let k = (end - i)
                .min(WIN)
                .min(cap)
                .min(usize::try_from(left * rate).unwrap_or(usize::MAX));
            let mut inb = [0u8; WIN + LOOK];
            let avail = {
                let d = self.job_data(j.v[0]);
                let Some(s) = d.get(i..end.min(i + k + LOOK)) else {
                    return Err(self.bad_step());
                };
                inb[..s.len()].copy_from_slice(s);
                s.len()
            };
            let last = i + avail >= end;
            let used = match self.fill_feed(j, &inb[..avail], k, last, i, &mut out)? {
                // Every kind takes at least one byte of a window, unless
                // it went on in its state.
                Feed::Used(0) => return Err(self.bad_step()),
                Feed::Used(u) => u,
                Feed::Again => 0,
                Feed::Stop => end - i,
                Feed::Msg(m) => return self.new_str(m).map(Some),
            };
            let i0 = j.at(W_I0);
            let charge = (i + used - i0) as u64 / rate - (i - i0) as u64 / rate;
            self.burn(charge);
            j.w[W_I] = (i + used) as u64;
        }
    }

    /// The end of a pass of a window-by-window job: a value to finish with
    /// instead (a `binascii` error message), or `None`.
    #[cfg_attr(mm_shipped, optimize(size))]
    fn fill_end(&mut self, j: &mut Job, out: &mut Out) -> Result<Option<Value>, Raised> {
        match j.kind {
            job::B64E if j.w[W_A] != 0 => out.byte(self, b'\n')?,
            job::B64D => {
                let (quad, pads) = (j.w[W_C] & 0xff, j.w[W_C] >> 16);
                if quad == 1 {
                    let msg = format!(
                        "Invalid base64-encoded string: number of data characters ({}) cannot be 1 more than a multiple of 4",
                        out.at / 3 * 4 + 1
                    );
                    return self.new_str(&msg).map(Some);
                }
                if quad != 0 && quad + pads < 4 {
                    return self.new_str("Incorrect padding").map(Some);
                }
            }
            job::ENCODE if j.w[W_C + 1] != 0 => return Err(self.encode_err(j)),
            _ => {}
        }
        Ok(None)
    }

    #[cold]
    #[cfg_attr(mm_shipped, optimize(size))]
    fn fromhex_err(&mut self, odd: bool, at: usize) -> Raised {
        if odd {
            return self.err(
                T::ValueError,
                "fromhex() arg must contain an even number of hexadecimal digits",
            );
        }
        let msg = format!("non-hexadecimal number found in fromhex() arg at position {at}");
        self.err(T::ValueError, &msg)
    }

    /// The `UnicodeEncodeError` of an encode job that met chars it cannot
    /// encode: from char `W_C + 1` (less one) to char `W_C`.
    #[cold]
    #[cfg_attr(mm_shipped, optimize(size))]
    fn encode_err(&mut self, j: &Job) -> Raised {
        let (i, end) = (j.at(W_C + 1) - 1, j.at(W_C));
        let c = word_codec(j.w[W_A]);
        let limit = j.w[W_A + 2];
        let what = if end - i == 1 {
            let ch = char::from_u32(j.w[W_C + 2] as u32).unwrap_or('\u{fffd}');
            format!("character '{}' in position {i}", escaped_char(ch))
        } else {
            format!("characters in position {i}-{}", end - 1)
        };
        let msg = format!(
            "'{}' codec can't encode {what}: ordinal not in range({limit})",
            c.name()
        );
        self.err(T::UnicodeEncodeError, &msg)
    }

    /// One window of a window-by-window job: `inb` holds input `i..` (at
    /// most `k` bytes to take and `LOOK` more to look at; `last` when it
    /// reaches the end).
    #[cfg_attr(mm_shipped, optimize(size))]
    fn fill_feed(
        &mut self,
        j: &mut Job,
        inb: &[u8],
        k: usize,
        last: bool,
        i: usize,
        out: &mut Out,
    ) -> Result<Feed, Raised> {
        let k = k.min(inb.len());
        match j.kind {
            job::MAP => {
                let mut t = [0u8; WIN];
                let t = &mut t[..k];
                t.copy_from_slice(&inb[..k]);
                match j.w[W_A] {
                    0 => t.make_ascii_uppercase(),
                    1 => t.make_ascii_lowercase(),
                    2 => {
                        for c in t.iter_mut() {
                            *c = if c.is_ascii_uppercase() {
                                c.to_ascii_lowercase()
                            } else {
                                c.to_ascii_uppercase()
                            };
                        }
                    }
                    3 => {
                        t.make_ascii_lowercase();
                        if i == j.at(W_I0)
                            && let Some(c) = t.first_mut()
                        {
                            c.make_ascii_uppercase();
                        }
                    }
                    _ => {
                        let mut prev = j.w[W_C] != 0;
                        for c in t.iter_mut() {
                            let r = if prev {
                                c.to_ascii_lowercase()
                            } else {
                                c.to_ascii_uppercase()
                            };
                            prev = c.is_ascii_alphabetic();
                            *c = r;
                        }
                        j.w[W_C] = u64::from(prev);
                    }
                }
                out.put(self, t)?;
                Ok(Feed::Used(k))
            }
            job::TRANSLATE => {
                let mut tab = [0u8; 256];
                match self.bytes_data(j.v[1]) {
                    t if t.len() == 256 => tab.copy_from_slice(t),
                    _ => {
                        for (x, b) in tab.iter_mut().enumerate() {
                            *b = x as u8;
                        }
                    }
                }
                for &c in &inb[..k] {
                    if j.w[W_A + usize::from(c >> 6)] & 1 << (c & 63) == 0 {
                        out.byte(self, tab[usize::from(c)])?;
                    }
                }
                Ok(Feed::Used(k))
            }
            job::HEX => {
                let (sep, per, group) = (j.w[W_A], j.w[W_A + 1] as i64, j.at(W_A + 2).max(1));
                let n = j.at(W_END) - j.at(W_I0);
                for (t, &c) in inb[..k].iter().enumerate() {
                    let at = i - j.at(W_I0) + t;
                    if sep != 0 && at > 0 && per != 0 {
                        let edge = if per > 0 {
                            (n - at).is_multiple_of(group)
                        } else {
                            at.is_multiple_of(group)
                        };
                        if edge {
                            out.byte(self, (sep - 1) as u8)?;
                        }
                    }
                    out.byte(self, DIGITS[usize::from(c >> 4)])?;
                    out.byte(self, DIGITS[usize::from(c & 15)])?;
                }
                Ok(Feed::Used(k))
            }
            #[cfg(feature = "binascii")]
            job::B64E => {
                let mut p = 0;
                while p < k {
                    let c = &inb[p..(p + 3).min(inb.len())];
                    if c.len() < 3 && !last {
                        break;
                    }
                    let w = (u32::from(c[0]) << 16)
                        | (u32::from(*c.get(1).unwrap_or(&0)) << 8)
                        | u32::from(*c.get(2).unwrap_or(&0));
                    let mut q = [b'='; 4];
                    for (t, x) in q.iter_mut().enumerate().take(c.len() + 1) {
                        *x = crate::binascii::B64[((w >> (18 - 6 * t)) & 63) as usize];
                    }
                    out.put(self, &q)?;
                    p += c.len();
                }
                Ok(Feed::Used(p))
            }
            #[cfg(feature = "binascii")]
            job::B64D => {
                let strict = j.w[W_A] != 0;
                let mut st = j.w[W_C];
                let r = crate::binascii::b64_feed(
                    self,
                    &inb[..k],
                    i == j.at(W_I0),
                    strict,
                    &mut st,
                    out,
                );
                j.w[W_C] = st;
                r.map(|r| r.unwrap_or(Feed::Used(k)))
            }
            job::UNHEX => {
                let mut p = 0;
                while p < k {
                    let hex = |x: Option<&u8>| x.and_then(|&c| char::from(c).to_digit(16));
                    let (Some(hi), Some(lo)) = (hex(inb.get(p)), hex(inb.get(p + 1))) else {
                        return Ok(Feed::Msg("Non-hexadecimal digit found"));
                    };
                    out.byte(self, (hi * 16 + lo) as u8)?;
                    p += 2;
                }
                Ok(Feed::Used(p))
            }
            job::FROMHEX => self.fromhex_feed(inb, k, last, i, out),
            job::DECODE => self.decode_feed(j, inb, k, last, i, out),
            job::ENCODE => self.encode_feed(j, inb, k, i, out),
            _ => {
                // `expandtabs`: `W_A` the tab size, `W_C` the column.
                let size = j.w[W_A] as i64;
                let mut col = j.w[W_C] as i64;
                for (x, &c) in inb[..k].iter().enumerate() {
                    match c {
                        b'\t' if size > 0 => {
                            let n = size - col % size;
                            let n = usize::try_from(n).unwrap_or(0);
                            // A wide tab's spaces go out [`TAB_SLICE`] at a
                            // time, the column keeping how many went: the
                            // tab is read again until the rest fits.
                            let m = n.min(TAB_SLICE);
                            out.fill(self, b' ', m)?;
                            self.burn(m as u64 / 64);
                            col += m as i64;
                            if m < n {
                                j.w[W_C] = col as u64;
                                return Ok(if x == 0 { Feed::Again } else { Feed::Used(x) });
                            }
                        }
                        b'\t' => {}
                        b'\n' | b'\r' => {
                            out.byte(self, c)?;
                            col = 0;
                        }
                        _ => {
                            out.byte(self, c)?;
                            col += 1;
                        }
                    }
                }
                j.w[W_C] = col as u64;
                Ok(Feed::Used(k))
            }
        }
    }

    /// A window of `bytes.fromhex`. As CPython, a digit pair cut short by
    /// the end is the odd-count error, anything else not a digit the
    /// position error.
    #[cfg_attr(mm_shipped, optimize(size))]
    fn fromhex_feed(
        &mut self,
        inb: &[u8],
        k: usize,
        last: bool,
        i: usize,
        out: &mut Out,
    ) -> Result<Feed, Raised> {
        let hexval = |c: u8| char::from(c).to_digit(16);
        let mut p = 0;
        while p < k {
            let c = inb[p];
            if c.is_ascii_whitespace() {
                p += 1;
                continue;
            }
            let Some(hi) = hexval(c) else {
                return Err(self.fromhex_err(false, i + p));
            };
            let Some(&lo) = inb.get(p + 1) else {
                if last {
                    return Err(self.fromhex_err(true, 0));
                }
                break;
            };
            let Some(lo) = hexval(lo) else {
                return Err(self.fromhex_err(false, i + p + 1));
            };
            out.byte(self, (hi * 16 + lo) as u8)?;
            p += 2;
        }
        Ok(Feed::Used(p))
    }

    /// A window of `bytes.decode`: `W_A` the codec, `W_A + 1` the error
    /// handler (its name in value slot 3), `W_A + 2` where positions in
    /// messages count from.
    #[cfg_attr(mm_shipped, optimize(size))]
    fn decode_feed(
        &mut self,
        j: &mut Job,
        inb: &[u8],
        k: usize,
        last: bool,
        i: usize,
        out: &mut Out,
    ) -> Result<Feed, Raised> {
        let c = word_codec(j.w[W_A]);
        let mut p = 0;
        while p < k {
            let (good, bad, reason) = match c {
                Codec::Latin1 => {
                    for &b in &inb[p..k] {
                        out.char(self, char::from(b))?;
                    }
                    return Ok(Feed::Used(k));
                }
                Codec::Utf16 { .. } | Codec::Utf32 { .. } => match wide_step(inb, p, c) {
                    Ok((ch, n)) => {
                        out.char(self, ch)?;
                        p += n;
                        continue;
                    }
                    Err((n, reason)) => (0, n, reason),
                },
                _ => c.decode_run(&inb[p..]),
            };
            out.put(self, &inb[p..p + good])?;
            p += good;
            if bad == 0 || (!last && p + bad >= inb.len() && reason == "unexpected end of data") {
                break;
            }
            self.decode_bad(j, &inb[p..p + bad], i + p, reason, out)?;
            p += bad;
        }
        Ok(Feed::Used(p))
    }

    /// What decode job `j`'s handler makes of `bad`, at input `at`.
    #[cold]
    #[inline(never)]
    #[cfg_attr(mm_shipped, optimize(size))]
    fn decode_bad(
        &mut self,
        j: &Job,
        bad: &[u8],
        at: usize,
        reason: &str,
        out: &mut Out,
    ) -> Result<(), Raised> {
        match word_errors(j.w[W_A + 1]) {
            Errors::Strict | Errors::SurrogatePass => {
                let c = word_codec(j.w[W_A]);
                let at = at - j.at(W_A + 2);
                let what = if bad.len() == 1 {
                    format!("byte 0x{:02x} in position {at}", bad[0])
                } else {
                    format!("bytes in position {at}-{}", at + bad.len() - 1)
                };
                let msg = format!("'{}' codec can't decode {what}: {reason}", c.name());
                Err(self.err(T::UnicodeDecodeError, &msg))
            }
            Errors::Replace => out.char(self, '\u{fffd}'),
            Errors::Ignore => Ok(()),
            Errors::BackslashReplace => {
                for &b in bad {
                    out.put(
                        self,
                        &[
                            b'\\',
                            b'x',
                            DIGITS[usize::from(b >> 4)],
                            DIGITS[usize::from(b & 15)],
                        ],
                    )?;
                }
                Ok(())
            }
            Errors::XmlCharRefReplace | Errors::NameReplace => Err(self.err(
                T::TypeError,
                "don't know how to handle UnicodeDecodeError in error callback",
            )),
            Errors::SurrogateEscape => Err(self.err(
                T::NotImplementedError,
                "micro-monty does not support lone surrogates (surrogateescape)",
            )),
            Errors::Unknown => Err(self.codec_lookup_error(j.v[3], false)),
        }
    }

    /// A window of `str.encode`: `W_A` the codec, `W_A + 1` the error
    /// handler (its name in value slot 3), `W_A + 2` the code point limit;
    /// `W_C` counts chars, `W_C + 1` (plus one) is where a run of chars the
    /// codec cannot encode started, `W_C + 2` its first.
    #[cfg_attr(mm_shipped, optimize(size))]
    fn encode_feed(
        &mut self,
        j: &mut Job,
        inb: &[u8],
        k: usize,
        i: usize,
        out: &mut Out,
    ) -> Result<Feed, Raised> {
        let c = word_codec(j.w[W_A]);
        let limit = j.w[W_A + 2];
        let mut cut = k;
        while cut < inb.len() && inb[cut] & 0xC0 == 0x80 {
            cut += 1;
        }
        let Ok(s) = core::str::from_utf8(&inb[..cut]) else {
            return Err(self.bad_step());
        };
        let wide = matches!(c, Codec::Utf16 { .. } | Codec::Utf32 { .. });
        if wide
            && i == j.at(W_I0)
            && matches!(
                c,
                Codec::Utf16 { bom: true, .. } | Codec::Utf32 { bom: true, .. }
            )
        {
            self.encode_unit(c, 0xFEFF, out)?;
        }
        for ch in s.chars() {
            let u = u64::from(ch as u32);
            if j.w[W_C + 1] != 0 {
                if u < limit {
                    return Err(self.encode_err(j));
                }
            } else if wide {
                if let Codec::Utf16 { .. } = c {
                    let mut buf = [0u16; 2];
                    for unit in ch.encode_utf16(&mut buf) {
                        self.encode_unit(c, u32::from(*unit), out)?;
                    }
                } else {
                    self.encode_unit(c, ch as u32, out)?;
                }
            } else if u < limit {
                out.byte(self, u as u8)?;
            } else {
                match word_errors(j.w[W_A + 1]) {
                    Errors::Replace => out.byte(self, b'?')?,
                    Errors::Ignore => {}
                    Errors::BackslashReplace => out.put(self, escaped_char(ch).as_bytes())?,
                    Errors::XmlCharRefReplace => {
                        out.put(self, format!("&#{};", ch as u32).as_bytes())?;
                    }
                    #[cfg(feature = "unicodedata")]
                    Errors::NameReplace => out.put(self, name_escape(ch).as_bytes())?,
                    Errors::Strict | Errors::SurrogateEscape | Errors::SurrogatePass => {
                        j.w[W_C + 1] = j.w[W_C] + 1;
                        j.w[W_C + 2] = u;
                    }
                    _ => return Err(self.codec_lookup_error(j.v[3], false)),
                }
            }
            j.w[W_C] += 1;
        }
        Ok(Feed::Used(cut))
    }

    /// One UTF-16 or UTF-32 code unit of codec `c`.
    #[cfg_attr(mm_shipped, optimize(size))]
    fn encode_unit(&mut self, c: Codec, u: u32, out: &mut Out) -> Result<(), Raised> {
        match c {
            Codec::Utf32 { be, .. } => {
                out.put(self, &if be { u.to_be_bytes() } else { u.to_le_bytes() })
            }
            Codec::Utf16 { be, .. } => {
                let u = u as u16;
                out.put(self, &if be { u.to_be_bytes() } else { u.to_le_bytes() })
            }
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
#[allow(clippy::panic, clippy::cast_possible_truncation, reason = "tests")]
mod tests {
    use super::{Hit, JOB_WORDS, W_S, seek, seek_init};
    use alloc::vec::Vec;

    /// Non-overlapping matches, in search order, by brute force.
    fn naive(h: &[u8], n: &[u8], rev: bool) -> Vec<usize> {
        let r: Vec<u8> = h.iter().rev().copied().collect();
        let nr: Vec<u8> = n.iter().rev().copied().collect();
        let (h, n) = if rev { (&r[..], &nr[..]) } else { (h, n) };
        let mut out = Vec::new();
        let mut at = 0;
        while let Some(i) = (at..=h.len().saturating_sub(n.len()))
            .find(|&i| h.len() >= n.len() && &h[i..i + n.len()] == n)
        {
            out.push(i);
            at = i + n.len();
        }
        out
    }

    /// A long search resumes where its work ran out, in every phase: the
    /// direct scan, the needle's factorization and two-way.
    #[test]
    fn seek_resumes_in_every_phase() {
        let mut x: u64 = 12345;
        let mut rnd = |k: u64| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x % k
        };
        for case in 0..3000 {
            let alpha = 1 + rnd(3) as u8;
            let hl = rnd(300) as usize;
            let nl = 1 + rnd(12) as usize;
            let mut h: Vec<u8> = (0..hl)
                .map(|_| b'a' + rnd(u64::from(alpha)) as u8)
                .collect();
            if case % 3 == 0 {
                // Candidates everywhere: two-way takes over.
                h.fill(b'a');
            }
            let nd: Vec<u8> = (0..nl)
                .map(|_| b'a' + rnd(u64::from(alpha)) as u8)
                .collect();
            for rev in [false, true] {
                let want = naive(&h, &nd, rev);
                for step in [1, 2, 3, 7, 64, usize::MAX] {
                    let mut s = [0; JOB_WORDS - W_S];
                    seek_init(&mut s, h.len());
                    let mut got = Vec::new();
                    for calls in 0.. {
                        assert!(calls < 100_000, "no progress: {h:?} {nd:?} step={step}");
                        let mut work = step;
                        match seek(&h, &nd, rev, &mut s, &mut work) {
                            Hit::At(p) => got.push(p),
                            Hit::Gone => break,
                            Hit::More => {}
                        }
                    }
                    assert!(got == want, "{h:?} {nd:?} rev={rev} step={step}");
                }
            }
        }
    }
}
