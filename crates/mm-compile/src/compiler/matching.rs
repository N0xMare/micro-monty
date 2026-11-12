//! `match` statements (PEP 634), compiled as CPython 3.14 does: each case
//! tests the subject (kept in a register) and jumps to the next case on the
//! first failing sub-pattern. Captured values go to temporaries and are
//! bound only once the whole pattern matched, before the guard. The checks
//! CPython's code generator makes (unreachable patterns, repeated names,
//! duplicate mapping keys) raise the same `SyntaxError`s here.

use super::{Compiler, Label, R, too_many};
use crate::ast::{BinOp, Const, Expr, ExprKind, MatchCase, Pattern, PatternKind, Pos, UnaryOp};
use crate::bytecode::{Konst, op};
use crate::error::CompileError;
use alloc::{
    format,
    string::{String, ToString},
    vec::Vec,
};
use core::fmt::Write as _;

/// `MATCH` instruction kinds (the `b` operand).
pub mod kind {
    /// `acc = acc is a sequence` (`collections.abc.Sequence`, not str/bytes).
    pub const SEQ: u8 = 0;
    /// `acc = acc is a mapping`.
    pub const MAP: u8 = 1;
    /// `acc = len(acc)`.
    pub const LEN: u8 = 2;
    /// `acc = tuple of R[r]'s values for the keys tuple acc, or None`.
    pub const KEYS: u8 = 3;
    /// `acc = dict(R[r])` without the keys in tuple acc.
    pub const REST: u8 = 4;
    /// `acc = tuple of attributes of subject acc for class R[r] (u
    /// positional sub-patterns, keyword names tuple R[r+1]), or None`.
    pub const CLASS: u8 = 5;
}

struct PatCtx {
    /// Names bound so far, in binding order (for duplicate checks).
    stores: Vec<String>,
    /// The register each capture name collects into.
    slots: Vec<(String, u8)>,
    allow_irrefutable: bool,
}

fn syntax(msg: String, pos: Pos) -> CompileError {
    CompileError::syntax(msg, pos)
}

fn is_wildcard(p: &Pattern) -> bool {
    matches!(
        p.kind,
        PatternKind::As(None, None) | PatternKind::Star(None)
    )
}

/// Every name `p` captures (unique, first-appearance order).
fn capture_names(p: &Pattern, out: &mut Vec<String>) {
    let mut work = alloc::vec![p];
    while let Some(p) = work.pop() {
        let mut add = |n: &Option<String>| {
            if let Some(n) = n
                && !out.contains(n)
            {
                out.push(n.clone());
            }
        };
        match &p.kind {
            PatternKind::Value(_) | PatternKind::Singleton(_) => {}
            PatternKind::Star(n) => add(n),
            PatternKind::Sequence(items) | PatternKind::Or(items) => {
                work.extend(items.iter().rev());
            }
            PatternKind::Mapping { patterns, rest, .. } => {
                add(rest);
                work.extend(patterns.iter().rev());
            }
            PatternKind::Class {
                patterns,
                kwd_patterns,
                ..
            } => {
                work.extend(kwd_patterns.iter().rev());
                work.extend(patterns.iter().rev());
            }
            PatternKind::As(inner, n) => {
                add(n);
                if let Some(x) = inner {
                    work.push(x);
                }
            }
        }
    }
}

impl<'a> Compiler<'a> {
    pub(super) fn match_stmt(&mut self, subject: &'a Expr, cases: &'a [MatchCase]) -> R<()> {
        self.u().block_depth += 1;
        self.expr(subject)?;
        let subj = self.temp()?;
        self.emit_r(op::STAR, subj);
        let end = self.label();
        let n = cases.len();
        for (i, c) in cases.iter().enumerate() {
            let m = self.mark();
            let next = self.label();
            let mut names = Vec::new();
            capture_names(&c.pattern, &mut names);
            let mut slots = Vec::with_capacity(names.len());
            for name in names {
                let r = self.temp()?;
                slots.push((name, r));
            }
            let mut pc = PatCtx {
                stores: Vec::new(),
                slots,
                allow_irrefutable: c.guard.is_some() || i + 1 == n,
            };
            self.pattern(&c.pattern, subj, next, &mut pc)?;
            for name in &pc.stores {
                let r = pc
                    .slots
                    .iter()
                    .find(|(n, _)| n == name)
                    .map_or(subj, |s| s.1);
                self.emit_r(op::LDAR, r);
                self.store_name(name)?;
            }
            self.release(m);
            if let Some(g) = &c.guard {
                self.cond_jump(g, false, next)?;
            }
            self.block(&c.body)?;
            if i + 1 < n {
                self.jump(op::JUMP, end);
            }
            self.bind(next);
        }
        self.bind(end);
        self.u().block_depth -= 1;
        Ok(())
    }

    #[inline(never)]
    fn match_op(&mut self, kind: u8, r: u8, u: u16, pos: Pos) {
        self.at_pos(pos);
        self.emit(op::MATCH);
        self.byte(kind);
        self.byte(r);
        self.word(u);
    }

    /// Jumps to `fail` unless the accumulator is true.
    fn fail_unless(&mut self, fail: Label) {
        self.jump(op::JUMP_IF_FALSE, fail);
    }

    /// Fails unless `len(R[subj])` is `n` (or at least `n`).
    #[inline(never)]
    fn len_check(&mut self, subj: u8, n: usize, at_least: bool, fail: Label, pos: Pos) -> R<()> {
        let len = self.temp()?;
        self.emit_r(op::LDAR, subj);
        self.match_op(kind::LEN, 0, 0, pos);
        self.emit_r(op::STAR, len);
        let k = i16::try_from(n).map_err(|_| too_many("patterns this long"))?;
        self.emit(op::LDA_SMI);
        self.word(k.cast_unsigned());
        self.emit_r(if at_least { op::TEST_GE } else { op::TEST_EQ }, len);
        self.fail_unless(fail);
        Ok(())
    }

    /// Stores the accumulator in a new temporary and fails if it is `None`.
    #[inline(never)]
    fn none_check(&mut self, fail: Label) -> R<u8> {
        let vals = self.temp()?;
        self.emit_r(op::STAR, vals);
        self.emit(op::LDA_NONE);
        self.emit_r(op::TEST_IS_NOT, vals);
        self.fail_unless(fail);
        Ok(vals)
    }

    /// Unpacks the tuple in `vals` and matches each item.
    #[inline(never)]
    fn sub_items(
        &mut self,
        vals: u8,
        items: &[&'a Pattern],
        fail: Label,
        pc: &mut PatCtx,
    ) -> R<()> {
        if items.iter().all(|x| is_wildcard(x)) {
            return Ok(());
        }
        let n = items.len();
        let cnt = u16::try_from(n).map_err(|_| too_many("patterns this long"))?;
        let vb = self.temps(n)?;
        self.emit_r(op::LDAR, vals);
        self.emit_ru(op::UNPACK_SEQ, vb, cnt);
        for (i, x) in items.iter().enumerate() {
            self.subpattern(x, vb + u8::try_from(i).unwrap_or(0), fail, pc)?;
        }
        Ok(())
    }

    #[inline(never)]
    fn capture(&mut self, name: &str, value: u8, pos: Pos, pc: &mut PatCtx) -> R<()> {
        if pc.stores.iter().any(|n| n == name) {
            return Err(syntax(
                format!("multiple assignments to name '{name}' in pattern"),
                pos,
            ));
        }
        pc.stores.push(name.to_string());
        let slot = pc
            .slots
            .iter()
            .find(|(n, _)| n == name)
            .map(|s| s.1)
            .ok_or_else(|| too_many("this pattern"))?;
        if slot != value {
            self.emit_rr(op::MOV, value, slot);
        }
        Ok(())
    }

    fn subpattern(&mut self, p: &'a Pattern, subj: u8, fail: Label, pc: &mut PatCtx) -> R<()> {
        let allow = pc.allow_irrefutable;
        pc.allow_irrefutable = true;
        self.pattern(p, subj, fail, pc)?;
        pc.allow_irrefutable = allow;
        Ok(())
    }

    /// Matches `p` against the value in register `subj`; jumps to `fail`
    /// when it does not match.
    fn pattern(&mut self, p: &'a Pattern, subj: u8, fail: Label, pc: &mut PatCtx) -> R<()> {
        self.enter(p.pos)?;
        match &p.kind {
            PatternKind::Value(e) => {
                if !value_ok(e) {
                    return Err(syntax(
                        "patterns may only match literals and attribute lookups".into(),
                        e.pos,
                    ));
                }
                self.expr(e)?;
                self.at_pos(p.pos);
                self.emit_r(op::TEST_EQ, subj);
                self.fail_unless(fail);
            }
            PatternKind::Singleton(c) => {
                self.at_pos(p.pos);
                self.constant(c)?;
                self.emit_r(op::TEST_IS, subj);
                self.fail_unless(fail);
            }
            PatternKind::As(None, name) => {
                if !pc.allow_irrefutable {
                    let msg = match name {
                        Some(n) => {
                            format!("name capture '{n}' makes remaining patterns unreachable")
                        }
                        None => "wildcard makes remaining patterns unreachable".into(),
                    };
                    return Err(syntax(msg, p.pos));
                }
                if let Some(n) = name {
                    self.capture(n, subj, p.pos, pc)?;
                }
            }
            PatternKind::As(Some(inner), name) => {
                self.pattern(inner, subj, fail, pc)?;
                if let Some(n) = name {
                    self.capture(n, subj, p.pos, pc)?;
                }
            }
            PatternKind::Star(name) => {
                if let Some(n) = name {
                    self.capture(n, subj, p.pos, pc)?;
                }
            }
            PatternKind::Or(alts) => self.or_pattern(p, alts, subj, fail, pc)?,
            PatternKind::Sequence(items) => self.seq_pattern(p, items, subj, fail, pc)?,
            PatternKind::Mapping {
                keys,
                patterns,
                rest,
            } => self.map_pattern(p, keys, patterns, rest.as_deref(), subj, fail, pc)?,
            PatternKind::Class {
                cls,
                patterns,
                kwd_attrs,
                kwd_patterns,
            } => self.class_pattern(p, cls, patterns, kwd_attrs, kwd_patterns, subj, fail, pc)?,
        }
        self.depth -= 1;
        Ok(())
    }

    #[inline(never)]
    fn or_pattern(
        &mut self,
        p: &'a Pattern,
        alts: &'a [Pattern],
        subj: u8,
        fail: Label,
        pc: &mut PatCtx,
    ) -> R<()> {
        let ok = self.label();
        let outer = core::mem::take(&mut pc.stores);
        let allow = pc.allow_irrefutable;
        let mut control: Option<Vec<String>> = None;
        let n = alts.len();
        for (i, alt) in alts.iter().enumerate() {
            pc.stores = Vec::new();
            pc.allow_irrefutable = i + 1 == n && allow;
            let last = i + 1 == n;
            let next = if last { fail } else { self.label() };
            self.pattern(alt, subj, next, pc)?;
            match &control {
                None => control = Some(core::mem::take(&mut pc.stores)),
                Some(c) => {
                    if c.len() != pc.stores.len() || pc.stores.iter().any(|s| !c.contains(s)) {
                        return Err(syntax(
                            "alternative patterns bind different names".into(),
                            p.pos,
                        ));
                    }
                }
            }
            if !last {
                self.jump(op::JUMP, ok);
                self.bind(next);
            }
        }
        self.bind(ok);
        pc.allow_irrefutable = allow;
        pc.stores = outer;
        for name in control.unwrap_or_default() {
            if pc.stores.contains(&name) {
                return Err(syntax(
                    format!("multiple assignments to name '{name}' in pattern"),
                    p.pos,
                ));
            }
            pc.stores.push(name);
        }
        Ok(())
    }

    #[inline(never)]
    fn seq_pattern(
        &mut self,
        p: &'a Pattern,
        items: &'a [Pattern],
        subj: u8,
        fail: Label,
        pc: &mut PatCtx,
    ) -> R<()> {
        let n = items.len();
        let mut star = None;
        for (i, x) in items.iter().enumerate() {
            if matches!(x.kind, PatternKind::Star(_)) {
                if star.is_some() {
                    return Err(syntax(
                        "multiple starred names in sequence pattern".into(),
                        p.pos,
                    ));
                }
                star = Some(i);
            }
        }
        let m = self.mark();
        self.at_pos(p.pos);
        self.emit_r(op::LDAR, subj);
        self.match_op(kind::SEQ, 0, 0, p.pos);
        self.fail_unless(fail);
        let min = if star.is_some() { n - 1 } else { n };
        if star.is_none() || min > 0 {
            self.len_check(subj, min, star.is_some(), fail, p.pos)?;
        }
        if items.iter().all(is_wildcard) {
            self.release(m);
            return Ok(());
        }
        let wild_star = star.is_some_and(|i| is_wildcard(&items[i]));
        if wild_star {
            // `[a, *_, b]`: index the items, as CPython does.
            let si = star.unwrap_or(0);
            let r = self.temp()?;
            for (i, x) in items.iter().enumerate() {
                if is_wildcard(x) {
                    continue;
                }
                let idx = if i < si {
                    i64::try_from(i).unwrap_or(0)
                } else {
                    i64::try_from(i).unwrap_or(0) - i64::try_from(n).unwrap_or(0)
                };
                let k = i16::try_from(idx).map_err(|_| too_many("patterns this long"))?;
                self.at_pos(p.pos);
                self.emit(op::LDA_SMI);
                self.word(k.cast_unsigned());
                self.emit_r(op::GET_ITEM, subj);
                self.emit_r(op::STAR, r);
                self.subpattern(x, r, fail, pc)?;
            }
            self.release(m);
            return Ok(());
        }
        let base = self.temps(n)?;
        self.at_pos(p.pos);
        self.emit_r(op::LDAR, subj);
        match star {
            None => {
                let k = u16::try_from(n).map_err(|_| too_many("patterns this long"))?;
                self.emit_ru(op::UNPACK_SEQ, base, k);
            }
            Some(si) => {
                let before = u8::try_from(si).map_err(|_| too_many("patterns this long"))?;
                let after = u8::try_from(n - si - 1).map_err(|_| too_many("patterns this long"))?;
                self.emit(op::UNPACK_EX);
                self.byte(base);
                self.byte(before);
                self.byte(after);
            }
        }
        for (i, x) in items.iter().enumerate() {
            let r = base + u8::try_from(i).unwrap_or(0);
            self.subpattern(x, r, fail, pc)?;
        }
        self.release(m);
        Ok(())
    }

    #[allow(clippy::too_many_arguments, reason = "the pattern's parts")]
    #[inline(never)]
    fn map_pattern(
        &mut self,
        p: &'a Pattern,
        keys: &'a [Expr],
        patterns: &'a [Pattern],
        rest: Option<&'a str>,
        subj: u8,
        fail: Label,
        pc: &mut PatCtx,
    ) -> R<()> {
        // Keys: literals or dotted names; constant keys must be distinct.
        let mut seen: Vec<KeyVal> = Vec::new();
        for k in keys {
            if !value_ok(k) {
                return Err(syntax(
                    "mapping pattern keys may only match literals and attribute lookups".into(),
                    p.pos,
                ));
            }
            if let Some(v) = key_value(k) {
                if seen.iter().any(|s| s.same(&v)) {
                    return Err(syntax(
                        format!("mapping pattern checks duplicate key ({})", v.repr()),
                        p.pos,
                    ));
                }
                seen.push(v);
            }
        }
        let m = self.mark();
        self.at_pos(p.pos);
        self.emit_r(op::LDAR, subj);
        self.match_op(kind::MAP, 0, 0, p.pos);
        self.fail_unless(fail);
        let n = keys.len();
        if n == 0 && rest.is_none() {
            self.release(m);
            return Ok(());
        }
        let keys_r = self.temp()?;
        if n > 0 {
            self.len_check(subj, n, true, fail, p.pos)?;
            let kb = self.temps(n)?;
            for (i, k) in keys.iter().enumerate() {
                self.expr(k)?;
                self.emit_r(op::STAR, kb + u8::try_from(i).unwrap_or(0));
            }
            let cnt = u16::try_from(n).map_err(|_| too_many("patterns this long"))?;
            self.at_pos(p.pos);
            self.emit_ru(op::BUILD_TUPLE, kb, cnt);
            self.emit_r(op::STAR, keys_r);
            self.match_op(kind::KEYS, subj, 0, p.pos);
            let vals = self.none_check(fail)?;
            let items: Vec<&'a Pattern> = patterns.iter().collect();
            self.sub_items(vals, &items, fail, pc)?;
        } else {
            self.at_pos(p.pos);
            self.emit_ru(op::BUILD_TUPLE, keys_r, 0);
            self.emit_r(op::STAR, keys_r);
        }
        if let Some(name) = rest {
            self.emit_r(op::LDAR, keys_r);
            self.match_op(kind::REST, subj, 0, p.pos);
            let r = self.temp()?;
            self.emit_r(op::STAR, r);
            self.capture(name, r, p.pos, pc)?;
        }
        self.release(m);
        Ok(())
    }

    #[allow(clippy::too_many_arguments, reason = "the pattern's parts")]
    #[inline(never)]
    fn class_pattern(
        &mut self,
        p: &'a Pattern,
        cls: &'a Expr,
        patterns: &'a [Pattern],
        kwd_attrs: &'a [String],
        kwd_patterns: &'a [Pattern],
        subj: u8,
        fail: Label,
        pc: &mut PatCtx,
    ) -> R<()> {
        for (i, a) in kwd_attrs.iter().enumerate() {
            if kwd_attrs[..i].contains(a) {
                let pos = kwd_patterns.get(i).map_or(p.pos, |x| x.pos);
                return Err(syntax(
                    format!("attribute name repeated in class pattern: {a}"),
                    pos,
                ));
            }
        }
        let m = self.mark();
        let base = self.temps(2)?;
        self.expr(cls)?;
        self.emit_r(op::STAR, base);
        let k = self.konst(Konst::Names(kwd_attrs.to_vec()))?;
        self.emit_u(op::LDA_CONST, k);
        self.emit_r(op::STAR, base + 1);
        self.emit_r(op::LDAR, subj);
        let nargs = u16::try_from(patterns.len()).map_err(|_| too_many("patterns this long"))?;
        self.match_op(kind::CLASS, base, nargs, p.pos);
        let vals = self.none_check(fail)?;
        let all: Vec<&'a Pattern> = patterns.iter().chain(kwd_patterns).collect();
        self.sub_items(vals, &all, fail, pc)?;
        self.release(m);
        Ok(())
    }
}

/// Is `e` a valid value-pattern or mapping-key expression: a literal
/// (complex sums included) or a dotted name?
fn value_ok(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Const(_) | ExprKind::Attribute(..) => true,
        ExprKind::Unary(UnaryOp::Neg, x) => matches!(x.kind, ExprKind::Const(_)),
        ExprKind::BinOp(l, BinOp::Add | BinOp::Sub, r) => {
            value_ok(l) && matches!(r.kind, ExprKind::Const(Const::Complex(_)))
        }
        _ => false,
    }
}

/// A constant mapping key, for CPython's duplicate check (`==` on the
/// folded constants). Numbers carry their parts and, for ints, the exact
/// value (an `i64` or the literal's digits); `repr` is the key's text.
enum KeyVal {
    Num(f64, f64, Option<i64>, Option<String>, String),
    Text(bool, Vec<u8>),
    None,
}

fn num(re: f64, im: f64, int: Option<i64>, big: Option<String>, repr: String) -> KeyVal {
    KeyVal::Num(re, im, int, big, repr)
}

fn key_value(e: &Expr) -> Option<KeyVal> {
    #[allow(
        clippy::cast_precision_loss,
        reason = "compared as CPython compares int and float"
    )]
    Some(match &e.kind {
        ExprKind::Const(c) => match c {
            Const::None => KeyVal::None,
            Const::Bool(b) => {
                let i = i64::from(*b);
                num(
                    i as f64,
                    0.0,
                    Some(i),
                    None,
                    String::from(if *b { "True" } else { "False" }),
                )
            }
            Const::Int(i) => num(*i as f64, 0.0, Some(*i), None, i.to_string()),
            Const::BigInt(t) => {
                let d: String = t.chars().filter(|&c| c != '_').collect();
                num(
                    d.parse().unwrap_or(f64::INFINITY),
                    0.0,
                    None,
                    Some(d.clone()),
                    d,
                )
            }
            Const::Float(f) => num(*f, 0.0, None, None, float_repr(*f)),
            Const::Complex(im) => num(0.0, *im, None, None, complex_repr(0.0, *im)),
            Const::Str(s) => KeyVal::Text(false, s.as_bytes().to_vec()),
            Const::Bytes(b) => KeyVal::Text(true, b.clone()),
            Const::Ellipsis => return None,
        },
        ExprKind::Unary(UnaryOp::Neg, x) => match key_value(x)? {
            KeyVal::Num(re, im, None, None, _) => num(-re, -im, None, None, complex_repr(-re, -im)),
            KeyVal::Num(re, _, None, Some(d), _) => {
                let d = format!("-{d}");
                num(-re, 0.0, None, Some(d.clone()), d)
            }
            _ => return None,
        },
        ExprKind::BinOp(l, op, r) => {
            let (KeyVal::Num(re, _, _, _, _), Some(KeyVal::Num(_, im, _, _, _))) =
                (key_value(l)?, key_value(r))
            else {
                return None;
            };
            let im = if *op == BinOp::Sub { -im } else { im };
            num(re, im, None, None, complex_repr(re, im))
        }
        _ => return None,
    })
}

impl KeyVal {
    #[allow(
        clippy::float_cmp,
        clippy::cast_possible_truncation,
        reason = "Python's =="
    )]
    fn same(&self, other: &KeyVal) -> bool {
        match (self, other) {
            (KeyVal::Num(r1, i1, n1, b1, _), KeyVal::Num(r2, i2, n2, b2, _)) => {
                match (n1, n2, b1, b2) {
                    (Some(a), Some(b), _, _) => a == b,
                    (_, _, Some(a), Some(b)) => a == b,
                    (Some(_), _, _, Some(_)) | (_, Some(_), Some(_), _) => false,
                    // An int against a float: exactly, as CPython compares.
                    (Some(i), None, _, _) => *i2 == 0.0 && *r2 == *r1 && *r2 as i64 == *i,
                    (None, Some(i), _, _) => *i1 == 0.0 && *r1 == *r2 && *r1 as i64 == *i,
                    _ => r1 == r2 && i1 == i2,
                }
            }
            (KeyVal::Text(x, a), KeyVal::Text(y, b)) => x == y && a == b,
            (KeyVal::None, KeyVal::None) => true,
            _ => false,
        }
    }

    fn repr(&self) -> String {
        match self {
            KeyVal::Num(.., r) => r.clone(),
            KeyVal::Text(bytes, b) => text_repr(*bytes, b),
            KeyVal::None => String::from("None"),
        }
    }
}

/// Python's `repr(float)`: shortest round-trip digits.
fn float_repr(f: f64) -> String {
    if !f.is_finite() {
        return String::from(if f.is_nan() {
            "nan"
        } else if f > 0.0 {
            "inf"
        } else {
            "-inf"
        });
    }
    let s = format!("{f:e}");
    let (m, e) = s.split_once('e').unwrap_or((&s, "0"));
    let e: i32 = e.parse().unwrap_or(0);
    if (-4..16).contains(&e) {
        let mut t = format!("{f}");
        if !t.contains('.') {
            t.push_str(".0");
        }
        t
    } else {
        format!("{m}e{}{:02}", if e < 0 { '-' } else { '+' }, e.abs())
    }
}

/// Python's `repr(complex)`.
fn complex_repr(re: f64, im: f64) -> String {
    let part = |f: f64| {
        let s = float_repr(f);
        s.strip_suffix(".0").map_or(s.clone(), String::from)
    };
    let imag = part(im);
    if re == 0.0 && re.is_sign_positive() {
        return format!("{imag}j");
    }
    let sign = if imag.starts_with('-') { "" } else { "+" };
    format!("({}{sign}{imag}j)", part(re))
}

/// `repr` of a str (`bytes` false) or bytes literal key.
fn text_repr(bytes: bool, b: &[u8]) -> String {
    let q = if b.contains(&b'\'') && !b.contains(&b'"') {
        '"'
    } else {
        '\''
    };
    let mut out = String::from(if bytes { "b" } else { "" });
    out.push(q);
    let text = if bytes {
        b.iter().map(|&c| char::from(c)).collect()
    } else {
        String::from_utf8_lossy(b).into_owned()
    };
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c == q => {
                out.push('\\');
                out.push(c);
            }
            c if (c as u32) < 0x20 || c as u32 == 0x7f || (bytes && c as u32 > 0x7f) => {
                let _ = write!(out, "\\x{:02x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push(q);
    out
}
