//! `datetime`'s parsers, natively (docs/spec.md §12 names this exception
//! to the Rust-versus-Python rule): `fromisoformat` follows CPython
//! 3.14.7's `Modules/_datetimemodule.c` (PSF license, see `THIRD_PARTY.md`)
//! line by line, reading the UTF-8 bytes as C does, with a NUL past the
//! end; `datetime/__init__.py` raises its errors and builds the objects.
//! `strptime` is a fast path for the common directives: it returns `None`
//! for anything else and `datetime/_strptime.py` (CPython's Python
//! `_strptime`) takes over, so it must give exactly what that would.

use crate::vm::{CallArgs, Raised, Vm};
use alloc::vec::Vec;
use mm_core::Value;

#[inline(never)]
fn int(v: i64) -> Value {
    Value::int(v).unwrap_or(Value::NONE)
}

#[inline(never)]
fn digits(b: &[u8]) -> Option<i64> {
    if b.is_empty() || b.len() > 12 || !b.iter().all(u8::is_ascii_digit) {
        return None;
    }
    Some(b.iter().fold(0, |n, &c| n * 10 + i64::from(c - b'0')))
}

fn is_leap(y: i64) -> bool {
    y % 4 == 0 && (y % 100 != 0 || y % 400 == 0)
}

fn days_in_month(y: i64, m: i64) -> i64 {
    match m {
        2 if is_leap(y) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// Days before each month in a common year.
const BEFORE: [i64; 13] = [0, 0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];

/// `ymd_to_ord`: days since 0001-01-01, plus one.
#[inline(never)]
fn ordinal(y: i64, m: i64, d: i64) -> i64 {
    let p = y - 1;
    let leap = i64::from(m > 2 && is_leap(y));
    p * 365 + p / 4 - p / 100 + p / 400 + BEFORE[m.clamp(0, 12) as usize] + leap + d
}

/// `ord_to_ymd`, for `n >= 1`.
fn from_ordinal(n: i64) -> (i64, i64, i64) {
    // A year has at most 366 days, so the guess is at most a year early.
    let mut y = (n - 1) * 400 / 146_097 + 1;
    while ordinal(y + 1, 1, 1) <= n {
        y += 1;
    }
    let mut left = n - ordinal(y, 1, 1);
    let mut m = 1;
    while left >= days_in_month(y, m) {
        left -= days_in_month(y, m);
        m += 1;
    }
    (y, m, left + 1)
}

fn valid_date(y: i64, m: i64, d: i64) -> bool {
    (1..=9999).contains(&y) && (1..=12).contains(&m) && d >= 1 && d <= days_in_month(y, m)
}

// ------------------------------------------------------- fromisoformat

/// The byte at `i`, NUL past the end (C reads the terminator).
fn at(b: &[u8], i: usize) -> u8 {
    b.get(i).copied().unwrap_or(0)
}

#[inline(never)]
fn parse_digits(b: &[u8], p: &mut usize, n: usize) -> Option<i64> {
    let mut v = 0;
    for _ in 0..n {
        let d = at(b, *p).wrapping_sub(b'0');
        *p += 1;
        if d > 9 {
            return None;
        }
        v = v * 10 + i64::from(d);
    }
    Some(v)
}

fn iso_to_ymd(y: i64, w: i64, d: i64) -> Option<(i64, i64, i64)> {
    if !(1..=9999).contains(&y) {
        return None;
    }
    let first = ordinal(y, 1, 1);
    let first_weekday = (first + 6) % 7;
    if !(1..53).contains(&w)
        && !(w == 53 && (first_weekday == 3 || (first_weekday == 2 && is_leap(y))))
    {
        return None;
    }
    if !(1..8).contains(&d) {
        return None;
    }
    let monday = first - first_weekday + if first_weekday > 3 { 7 } else { 0 };
    Some(from_ordinal(monday + (w - 1) * 7 + d - 1))
}

/// `parse_isoformat_date` on `b`, `len` its date part.
fn parse_isoformat_date(b: &[u8], len: usize) -> Option<(i64, i64, i64)> {
    let mut p = 0;
    let year = parse_digits(b, &mut p, 4)?;
    let sep = at(b, p) == b'-';
    p += usize::from(sep);
    if at(b, p) == b'W' {
        p += 1;
        let week = parse_digits(b, &mut p, 2)?;
        let mut day = 1;
        if p < len {
            if sep {
                p += 1;
                if at(b, p - 1) != b'-' {
                    return None;
                }
            }
            day = parse_digits(b, &mut p, 1)?;
        }
        return iso_to_ymd(year, week, day);
    }
    let month = parse_digits(b, &mut p, 2)?;
    if sep {
        p += 1;
        if at(b, p - 1) != b'-' {
            return None;
        }
    }
    Some((year, month, parse_digits(b, &mut p, 2)?))
}

/// `parse_hh_mm_ss_ff` from `p` to `end`: `Err` for its negative codes,
/// else whether something other than the end follows, and the fields.
fn parse_hh_mm_ss_ff(b: &[u8], mut p: usize, end: usize) -> Result<(bool, [i64; 4]), ()> {
    let mut v = [0; 4];
    let mut sep = true;
    for i in 0..3 {
        v[i] = parse_digits(b, &mut p, 2).ok_or(())?;
        let c = at(b, p);
        p += 1;
        if i == 0 {
            sep = c == b':';
        }
        if c == b'.' || c == b',' {
            if i < 2 || p >= end {
                return Err(());
            }
            break;
        } else if p >= end {
            return Ok((c != 0, v));
        } else if sep && c == b':' {
            if i == 2 {
                return Err(());
            }
        } else if !sep {
            p -= 1;
        } else {
            return Err(());
        }
    }
    let n = (end - p).min(6);
    let mut us = parse_digits(b, &mut p, n).ok_or(())?;
    for _ in n..6 {
        us *= 10;
    }
    v[3] = us;
    while at(b, p).is_ascii_digit() {
        p += 1;
    }
    Ok((at(b, p) != 0, v))
}

/// `parse_isoformat_time` on `b[start..]`: the fields and the UTC offset
/// in microseconds (`None` without one).
fn parse_isoformat_time(b: &[u8], start: usize) -> Option<([i64; 4], Option<i64>)> {
    let end = b.len();
    let mut tz = start;
    loop {
        if matches!(at(b, tz), b'Z' | b'+' | b'-') {
            break;
        }
        tz += 1;
        if tz >= end {
            break;
        }
    }
    let (more, v) = parse_hh_mm_ss_ff(b, start, tz).ok()?;
    if tz == end {
        return (!more).then_some((v, None));
    }
    if at(b, tz) == b'Z' {
        return (at(b, tz + 1) == 0).then_some((v, Some(0)));
    }
    let (more, o) = parse_hh_mm_ss_ff(b, tz + 1, end).ok()?;
    let off = (o[0] * 3600 + o[1] * 60 + o[2]) * 1_000_000 + o[3];
    (!more).then_some((v, Some(if at(b, tz) == b'-' { -off } else { off })))
}

/// `_find_isoformat_datetime_separator` (`None` for its -1).
fn find_separator(b: &[u8]) -> Option<usize> {
    let len = b.len();
    if len == 7 {
        return Some(7);
    }
    if b[4] == b'-' {
        if b[5] != b'W' {
            return Some(10);
        }
        if len > 8 && b[8] == b'-' {
            if len == 9 {
                return None;
            }
            return Some(if len > 10 && b[10].is_ascii_digit() {
                8
            } else {
                10
            });
        }
        return Some(8);
    }
    if b[4] != b'W' {
        return Some(8);
    }
    let idx = 7 + b[7..].iter().take_while(|c| c.is_ascii_digit()).count();
    Some(if idx < 9 {
        idx
    } else if idx % 2 == 0 {
        7
    } else {
        8
    })
}

/// `datetime_fromisoformat`'s parse of a string of at least 7 characters
/// (`datetime.fromisoformat` checks): `(y, m, d, h, mi, s, us)` and the
/// offset.
fn iso_datetime(b: &[u8]) -> Option<([i64; 7], Option<i64>)> {
    if b.len() < 7 {
        return None;
    }
    let sep = find_separator(b)?;
    let (y, m, d) = parse_isoformat_date(b, sep)?;
    let (t, off) = if b.len() > sep {
        // Skip the separator, one UTF-8 character.
        let skip = match b[sep] {
            c if c & 0x80 == 0 => 1,
            c if c & 0xf0 == 0xe0 => 3,
            c if c & 0xf0 == 0xf0 => 4,
            _ => 2,
        };
        parse_isoformat_time(b, sep + skip)?
    } else {
        ([0; 4], None)
    };
    Some(([y, m, d, t[0], t[1], t[2], t[3]], off))
}

/// `_mm.dt_iso(s, kind)`: what `date` (kind 0), `time` (1) or `datetime`
/// (2) `.fromisoformat(s)` parses `s` to, before it checks the fields:
/// `0` for an invalid string, `1` for the hour-24 error (`time`); else
/// `(y, m, d, ok)`, `(h, mi, s, us, off, ok)` or `(y, m, d, h, mi, s, us,
/// off, ok)`, `off` the UTC offset in microseconds or `None`, `ok` 1 when
/// every field is in range, 0 when the constructor must check them and 2
/// for the hour-24 error after the zone (`datetime`).
pub(crate) fn dt_iso(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let (s, kind) = (vm.arg_o(&a, 0), vm.int_of_o(vm.arg_o(&a, 1)).unwrap_or(-1));
    if !Vm::is_str(s) {
        return Ok(int(0));
    }
    let n = vm.str_ref_o(s).len();
    vm.burn(1 + n as u64 / 8);
    let b = vm.str_ref_o(s).as_bytes();
    let time_ok = |t: &[i64]| t[0] <= 23 && t[1] <= 59 && t[2] <= 59;
    // The fields, then the offset (`None` for naive) and `ok`.
    let mut f = [0i64; 7];
    let (len, off, ok) = match kind {
        0 => {
            let Some((y, m, d)) = matches!(n, 7 | 8 | 10)
                .then(|| parse_isoformat_date(b, n))
                .flatten()
            else {
                return Ok(int(0));
            };
            f[..3].copy_from_slice(&[y, m, d]);
            (3, None, i64::from(valid_date(y, m, d)))
        }
        1 => {
            let start = usize::from(at(b, 0) == b'T');
            let Some((t, off)) = parse_isoformat_time(b, start) else {
                return Ok(int(0));
            };
            f[..4].copy_from_slice(&t);
            if f[0] == 24 {
                if f[1..4].iter().any(|&x| x != 0) {
                    return Ok(int(1));
                }
                f[0] = 0;
            }
            (4, Some(off), i64::from(time_ok(&f)))
        }
        _ => {
            let Some((g, off)) = iso_datetime(b) else {
                return Ok(int(0));
            };
            f = g;
            let mut ok = i64::from(valid_date(f[0], f[1], f[2]) && time_ok(&f[3..]));
            // Hour 24 of a valid month and day is the next midnight.
            let midnight =
                f[3] == 24 && (1..=12).contains(&f[1]) && f[2] <= days_in_month(f[0], f[1]);
            if midnight && f[4..].iter().any(|&x| x != 0) {
                ok = 2;
            } else if midnight {
                let dim = days_in_month(f[0], f[1]);
                f[3] = 0;
                f[2] += 1;
                if f[2] > dim {
                    f[2] = 1;
                    f[1] += 1;
                    if f[1] > 12 {
                        f[1] = 1;
                        f[0] += 1;
                    }
                }
                ok = i64::from(valid_date(f[0], f[1], f[2]));
            }
            (7, Some(off), ok)
        }
    };
    let mut out = [Value::NONE; 9];
    for (o, &v) in out.iter_mut().zip(&f[..len]) {
        *o = int(v);
    }
    let mut len = len;
    if let Some(off) = off {
        out[len] = off.map_or(Value::NONE, int);
        len += 1;
    }
    out[len] = int(ok);
    vm.alloc_tuple(&out[..=len])
}

// ------------------------------------------------------------- strptime

/// One piece of a format: `\s+`, a literal (matched ignoring case, as
/// `_strptime` compiles with `IGNORECASE`) or a directive.
#[derive(Clone, Copy)]
enum Tok {
    Ws,
    Lit(u8),
    Dir(u8),
}

type Alt = &'static [(u8, u8)];

const D: (u8, u8) = (b'0', b'9');
const SP: (u8, u8) = (b' ', b' ');

/// `_MAPPING`'s alternatives for the numeric directives, in order.
fn alts(d: u8) -> &'static [Alt] {
    match d {
        b'd' => &[
            &[(b'3', b'3'), (b'0', b'1')],
            &[(b'1', b'2'), D],
            &[(b'0', b'0'), (b'1', b'9')],
            &[(b'1', b'9')],
            &[SP, (b'1', b'9')],
        ],
        b'H' => &[
            &[(b'2', b'2'), (b'0', b'3')],
            &[(b'0', b'1'), D],
            &[D],
            &[SP, D],
        ],
        b'I' => &[
            &[(b'1', b'1'), (b'0', b'2')],
            &[(b'0', b'0'), (b'1', b'9')],
            &[(b'1', b'9')],
            &[SP, (b'1', b'9')],
        ],
        b'j' => &[
            &[(b'3', b'3'), (b'6', b'6'), (b'0', b'6')],
            &[(b'3', b'3'), (b'0', b'5'), D],
            &[(b'1', b'2'), D, D],
            &[(b'0', b'0'), (b'1', b'9'), D],
            &[(b'0', b'0'), (b'0', b'0'), (b'1', b'9')],
            &[(b'1', b'9'), D],
            &[(b'0', b'0'), (b'1', b'9')],
            &[(b'1', b'9')],
        ],
        b'm' => &[
            &[(b'1', b'1'), (b'0', b'2')],
            &[(b'0', b'0'), (b'1', b'9')],
            &[(b'1', b'9')],
        ],
        b'M' => &[&[(b'0', b'5'), D], &[D]],
        b'S' => &[&[(b'6', b'6'), (b'0', b'1')], &[(b'0', b'5'), D], &[D]],
        b'y' => &[&[D, D]],
        b'Y' => &[&[D, D, D, D]],
        _ => &[],
    }
}

const F_WEEKDAY: [&str; 7] = [
    "monday",
    "tuesday",
    "wednesday",
    "thursday",
    "friday",
    "saturday",
    "sunday",
];
const A_WEEKDAY: [&str; 7] = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"];
const F_MONTH: [&str; 12] = [
    "january",
    "february",
    "march",
    "april",
    "may",
    "june",
    "july",
    "august",
    "september",
    "october",
    "november",
    "december",
];
const A_MONTH: [&str; 12] = [
    "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
];
const AM_PM: [&str; 2] = ["am", "pm"];

fn names(d: u8) -> &'static [&'static str] {
    match d {
        b'A' => &F_WEEKDAY,
        b'a' => &A_WEEKDAY,
        b'B' => &F_MONTH,
        b'b' => &A_MONTH,
        _ => &AM_PM,
    }
}

fn is_ws(c: u8) -> bool {
    // `str.isspace` on ASCII, as the regex's `\s` matches.
    matches!(c, b' ' | b'\t'..=b'\r' | 0x1c..=0x1f)
}

fn has(s: &[u8], i: usize, lo: u8, hi: u8) -> bool {
    s.get(i).is_some_and(|&x| (lo..=hi).contains(&x))
}

/// The end of `%z`'s `k`-th way to match at `p`, in the order the regex
/// `[+-]\d\d:?[0-5]\d(:?[0-5]\d(\.\d{1,6})?)?|(?-i:Z)` tries them.
#[inline(never)]
fn z_end(s: &[u8], p: usize, k: usize) -> Step {
    let mut ends = [0usize; 32];
    let mut n = 0;
    if s.get(p) == Some(&b'Z') {
        ends[0] = p + 1;
        n = 1;
    } else if matches!(s.get(p), Some(b'+' | b'-'))
        && has(s, p + 1, b'0', b'9')
        && has(s, p + 2, b'0', b'9')
    {
        for colon in [true, false] {
            let q = p + 3 + usize::from(colon);
            if (colon && s.get(p + 3) != Some(&b':'))
                || !has(s, q, b'0', b'5')
                || !has(s, q + 1, b'0', b'9')
            {
                continue;
            }
            let q = q + 2;
            for colon in [true, false] {
                let r = q + usize::from(colon);
                if (colon && s.get(q) != Some(&b':'))
                    || !has(s, r, b'0', b'5')
                    || !has(s, r + 1, b'0', b'9')
                {
                    continue;
                }
                let r = r + 2;
                if s.get(r) == Some(&b'.') {
                    let run = s[r + 1..]
                        .iter()
                        .take(6)
                        .take_while(|x| x.is_ascii_digit())
                        .count();
                    for len in (1..=run).rev() {
                        ends[n] = r + 1 + len;
                        n += 1;
                    }
                }
                ends[n] = r;
                n += 1;
            }
            ends[n] = q;
            n += 1;
        }
    }
    if k < n { Step::Hit(ends[k]) } else { Step::Out }
}

/// One alternative of a piece: it matches up to `Hit`'s end, it does not
/// match, or there are no more.
enum Step {
    Hit(usize),
    Miss,
    Out,
}

fn step(hit: bool, end: usize) -> Step {
    if hit { Step::Hit(end) } else { Step::Miss }
}

/// `t`'s `k`-th alternative at `p`. A whitespace run counts a step per
/// 16 bytes it scans toward [`MAX_STEPS`] (each alternative rescans it).
#[inline(never)]
fn alt_end(t: Tok, s: &[u8], p: usize, k: usize, steps: &mut u32) -> Step {
    match t {
        Tok::Lit(_) if k > 0 => Step::Out,
        Tok::Lit(l) => step(s.get(p).is_some_and(|x| x.eq_ignore_ascii_case(&l)), p + 1),
        Tok::Ws => {
            let run = s[p.min(s.len())..]
                .iter()
                .take_while(|&&x| is_ws(x))
                .count();
            *steps = steps.saturating_add(u32::try_from(run / 16).unwrap_or(u32::MAX));
            if k < run {
                Step::Hit(p + run - k)
            } else {
                Step::Out
            }
        }
        Tok::Dir(b'f') => {
            let run = s[p.min(s.len())..]
                .iter()
                .take(6)
                .take_while(|x| x.is_ascii_digit())
                .count();
            if k < run {
                Step::Hit(p + run - k)
            } else {
                Step::Out
            }
        }
        Tok::Dir(b'z') => z_end(s, p, k),
        Tok::Dir(d @ (b'A' | b'a' | b'B' | b'b' | b'p')) => {
            // `_seq_to_re` sorts by length, longest first; at most one
            // name matches at a place, so the order does not matter.
            let Some(&w) = names(d).get(k) else {
                return Step::Out;
            };
            let hit = s
                .get(p..p + w.len())
                .is_some_and(|x| x.eq_ignore_ascii_case(w.as_bytes()));
            step(hit, p + w.len())
        }
        Tok::Dir(d) => {
            let Some(&alt) = alts(d).get(k) else {
                return Step::Out;
            };
            let hit = alt
                .iter()
                .enumerate()
                .all(|(i, &(lo, hi))| has(s, p + i, lo, hi));
            step(hit, p + alt.len())
        }
    }
}

/// The format's pieces, or `None` for anything the Python path must
/// handle: non-ASCII, a directive outside the set here, flags or widths,
/// a directive given twice (a regex error) or `%d` without a year (a
/// warning).
#[inline(never)]
fn tokens(f: &[u8]) -> Option<Vec<Tok>> {
    // Each token takes a matching step: a format with more than
    // `MAX_STEPS` goes to the Python path whatever the input.
    let mut out = Vec::with_capacity(f.len().min(MAX_STEPS as usize));
    let mut seen = 0u64;
    let mut i = 0;
    while i < f.len() {
        if out.len() >= MAX_STEPS as usize {
            return None;
        }
        let ch = f[i];
        if is_ws(ch) {
            while i < f.len() && is_ws(f[i]) {
                i += 1;
            }
            out.push(Tok::Ws);
            continue;
        }
        i += 1;
        if ch != b'%' {
            out.push(Tok::Lit(ch));
            continue;
        }
        let d = *f.get(i)?;
        i += 1;
        if d == b'%' {
            out.push(Tok::Lit(b'%'));
            continue;
        }
        if !b"YmdHMSfzybBaApIj".contains(&d) {
            return None;
        }
        let bit = 1u64 << (d - b'A');
        if seen & bit != 0 {
            return None;
        }
        seen |= bit;
        out.push(Tok::Dir(d));
    }
    let bit = |d: u8| seen & (1u64 << (d - b'A')) != 0;
    if bit(b'd') && !bit(b'Y') && !bit(b'y') {
        return None;
    }
    Some(out)
}

/// Most matching steps before handing a format to the regex engine.
const MAX_STEPS: u32 = 4096;

#[inline(never)]
fn num(s: &[u8]) -> i64 {
    s.iter()
        .filter(|x| x.is_ascii_digit())
        .fold(0, |n, &c| n * 10 + i64::from(c - b'0'))
}

fn index_of(d: u8, s: &[u8]) -> i64 {
    names(d)
        .iter()
        .position(|w| w.as_bytes().eq_ignore_ascii_case(s))
        .map_or(0, |i| i as i64)
}

/// `_strptime`'s results: `(year, month, day, hour, minute, second,
/// weekday, julian, -1, None, gmtoff)`, `fraction`, `gmtoff_fraction`.
#[inline(never)]
fn strptime(s: &[u8], f: &[u8], steps: &mut u32) -> Option<[Option<i64>; 13]> {
    let toks = tokens(f)?;
    // The `re.match`: the first way through in the regex's order.
    let mut stack: Vec<(usize, usize, usize)> = Vec::with_capacity(toks.len());
    let (mut pos, mut k) = (0, 0);
    while stack.len() < toks.len() {
        *steps += 1;
        if *steps > MAX_STEPS {
            return None;
        }
        let t = toks[stack.len()];
        match alt_end(t, s, pos, k, steps) {
            Step::Hit(end) => {
                stack.push((pos, k + 1, end));
                (pos, k) = (end, 0);
            }
            Step::Miss => k += 1,
            Step::Out => {
                let (p, next, _) = stack.pop()?;
                (pos, k) = (p, next);
            }
        }
    }
    // Unconverted data remains: the Python path raises.
    if pos != s.len() {
        return None;
    }
    let (mut year, mut month, mut day) = (None, 1, 1);
    let (mut hour, mut minute, mut second, mut fraction) = (0, 0, 0, 0);
    let (mut gmtoff, mut gmtoff_fraction) = (None, 0);
    let (mut weekday, mut julian) = (None, None);
    let ampm = toks
        .iter()
        .zip(&stack)
        .find(|(t, _)| matches!(t, Tok::Dir(b'p')))
        .map(|(_, &(a, _, b))| &s[a..b]);
    for (&t, &(a, _, b)) in toks.iter().zip(&stack) {
        let Tok::Dir(d) = t else { continue };
        let v = &s[a..b];
        match d {
            b'y' => year = Some(num(v) + if num(v) <= 68 { 2000 } else { 1900 }),
            b'Y' => year = Some(num(v)),
            b'm' => month = num(v),
            b'B' | b'b' => month = index_of(d, v) + 1,
            b'd' => day = num(v),
            b'H' => hour = num(v),
            b'I' => {
                hour = num(v);
                match ampm {
                    Some(p) if p.eq_ignore_ascii_case(b"pm") => {
                        if hour != 12 {
                            hour += 12;
                        }
                    }
                    _ if hour == 12 => hour = 0,
                    _ => {}
                }
            }
            b'M' => minute = num(v),
            b'S' => second = num(v),
            b'f' => fraction = num(v) * 10i64.pow(6 - v.len() as u32),
            b'A' | b'a' => weekday = Some(index_of(d, v)),
            b'j' => julian = Some(num(v)),
            b'z' if v == b"Z" => gmtoff = Some(0),
            b'z' => {
                // At most `+HH:MM:SS.ffffff`, without the colons.
                let mut buf = [0u8; 16];
                let mut n = 0;
                for (i, &ch) in v.iter().enumerate() {
                    let colon = (i == 3 && v[3] == b':') || (i == 6 && v[3] == b':' && v.len() > 6);
                    if colon && ch != b':' {
                        return None;
                    }
                    if !colon {
                        buf[n] = ch;
                        n += 1;
                    }
                }
                let z = &buf[..n];
                // `int(z[5:7] or 0)` and the fraction after `z[7]`.
                let secs = match z.get(5..z.len().min(7)) {
                    Some(x) if !x.is_empty() => digits(x)?,
                    _ => 0,
                };
                let rest = z.get(8..).unwrap_or(&[]);
                if z.len() > 7 && (z[7] != b'.' || rest.is_empty() || digits(rest).is_none()) {
                    return None;
                }
                let mut frac = if rest.is_empty() { 0 } else { digits(rest)? };
                for _ in rest.len()..6 {
                    frac *= 10;
                }
                let mut off = num(&z[1..3]) * 3600 + num(&z[3..5]) * 60 + secs;
                if z[0] == b'-' {
                    off = -off;
                    frac = -frac;
                }
                gmtoff = Some(off);
                gmtoff_fraction = frac;
            }
            _ => {}
        }
    }
    let mut year = year.unwrap_or(1900);
    if !valid_date(year, 1, 1) {
        return None;
    }
    let n = match julian {
        None if valid_date(year, month, day) => ordinal(year, month, day),
        None => return None,
        Some(j) => {
            let n = j - 1 + ordinal(year, 1, 1);
            if !(1..=3_652_059).contains(&n) {
                return None;
            }
            (year, month, day) = from_ordinal(n);
            n
        }
    };
    let julian = julian.unwrap_or(n - ordinal(year, 1, 1) + 1);
    let weekday = weekday.unwrap_or((n + 6) % 7);
    let f = [year, month, day, hour, minute, second, weekday, julian, -1];
    let mut out = [None; 13];
    for (o, x) in out.iter_mut().zip(f) {
        *o = Some(x);
    }
    out[10] = gmtoff;
    out[11] = Some(fraction);
    out[12] = Some(gmtoff_fraction);
    Some(out)
}

/// `_mm.dt_strptime(data, format)`: `_strptime`'s result, `((year, month,
/// day, hour, minute, second, weekday, julian, -1, None, gmtoff),
/// fraction, gmtoff_fraction)`, or `None` if this does not parse it.
pub(crate) fn dt_strptime(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let (s, f) = (vm.arg_o(&a, 0), vm.arg_o(&a, 1));
    if !Vm::is_str(s) || !Vm::is_str(f) {
        return Ok(Value::NONE);
    }
    let len = vm.str_ref_o(s).len() + vm.str_ref_o(f).len();
    let mut steps = 0;
    let r = {
        let (s, f) = (vm.str_ref_o(s).as_bytes(), vm.str_ref_o(f).as_bytes());
        if s.is_ascii() && f.is_ascii() {
            strptime(s, f, &mut steps)
        } else {
            None
        }
    };
    vm.burn(1 + len as u64 / 8 + u64::from(steps) / 8);
    let Some(r) = r else {
        return Ok(Value::NONE);
    };
    let mut v = [Value::NONE; 13];
    for (o, x) in v.iter_mut().zip(r) {
        *o = x.map_or(Value::NONE, int);
    }
    let tt = vm.alloc_tuple(&v[..11])?;
    vm.alloc_tuple(&[tt, v[11], v[12]])
}

// ------------------------------------------------------- ordinal math

/// `_mm.dt_ord(y, m, d)`: `date(y, m, d).toordinal()` for a valid date,
/// else `None` (the Python caller then takes its own path).
#[allow(clippy::unnecessary_wraps, reason = "the one builtin signature")]
pub(crate) fn dt_ord(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let f = |i: u32| vm.int_of_o(vm.arg_o(&a, i));
    Ok(match (f(0), f(1), f(2)) {
        (Some(y), Some(m), Some(d)) if valid_date(y, m, d) => int(ordinal(y, m, d)),
        _ => Value::NONE,
    })
}

/// `_mm.dt_ymd(n)`: `date.fromordinal(n)`'s `(year, month, day)` for
/// `1 <= n <= date.max.toordinal()`, else `None`.
pub(crate) fn dt_ymd(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    match vm.int_of_o(vm.arg_o(&a, 0)) {
        Some(n) if (1..=3_652_059).contains(&n) => {
            let (y, m, d) = from_ordinal(n);
            vm.alloc_tuple(&[int(y), int(m), int(d)])
        }
        _ => Ok(Value::NONE),
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::strptime;
    use alloc::string::String;
    use std::time::{Duration, Instant};

    /// Each alternative once rescanned a whitespace run: a run of 2M
    /// spaces before a failing `%Y` must cost about what 2,000 do. The
    /// facade's parking test held this bound through `_mm`, which is now
    /// the bundled modules' own.
    #[test]
    fn a_long_whitespace_run_is_scanned_once() {
        let time = |n: usize| {
            let mut s = String::from(" ").repeat(n);
            s.push('x');
            let t = Instant::now();
            assert!(strptime(s.as_bytes(), b" %Y", &mut 0).is_none());
            t.elapsed()
        };
        let (mut slow, mut control) = (time(2_000_000), time(2_000));
        for _ in 0..2 {
            if slow < control * 4 + Duration::from_millis(500) {
                break;
            }
            slow = slow.min(time(2_000_000));
            control = control.min(time(2_000));
        }
        assert!(
            slow < control * 4 + Duration::from_millis(500),
            "{slow:?} against {control:?}"
        );
    }
}
