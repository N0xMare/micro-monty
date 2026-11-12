//! A compact binary form of a [`Program`], so that Python compiled at build
//! time (the engine's prelude) loads without parsing at startup.
//!
//! Integers are LEB128, strings and vectors are length-prefixed. A whole
//! [`Program`] stores each distinct string once, in a pool ahead of the
//! code objects, which refer to it by index. The format is private to one
//! build: the encoder and decoder ship together.
//!
//! The bundled stdlib uses a sectioned form ([`Program::encode_sections`]):
//! the same fields, each written to the [`section`] of its kind, so that a
//! packer with one set of codes per section (`crate::pack`) models each kind
//! on its own. Instructions are split into opcodes and operands by operand
//! kind; jump targets are relative and a 16-bit operand equal to the next
//! unused inline-cache slot is written as 0. Strings may also come from a
//! table shared by every bundled module.

use crate::ast::{Anchor, Pos};
use crate::bytecode::{
    Code, ExcEntry, FORMATS, Konst, ParamSpec, PosEntry, Program, RegName, instr_len, layout_of,
    opcode, operand_plan, word_bytes,
};
use alloc::{collections::BTreeMap, string::String, sync::Arc, vec::Vec};

/// The sections of [`Program::encode_sections`], by kind of field.
pub mod section {
    /// The program's own strings (those not in the shared table).
    pub const POOL: usize = 0;
    /// String references: indices into the shared table, then the pool.
    pub const INDEX: usize = 1;
    /// Everything else: counts, flags, constants, parameters, tables.
    pub const META: usize = 2;
    /// Opcodes.
    pub const OP: usize = 3;
    /// `r` operands (registers).
    pub const REG: usize = 4;
    /// `b` operands.
    pub const BYTE: usize = 5;
    /// `u` operands, predicted (see the module docs), up to 254; 255
    /// for larger ones.
    pub const U: usize = 6;
    /// `i` operands, zigzag LEB128, and `u` operands past 254, less 255,
    /// LEB128.
    pub const I: usize = 7;
    /// `j` operands, relative to the instruction, zigzag LEB128.
    pub const JUMP: usize = 8;
    /// How many there are.
    pub const COUNT: usize = 9;
}
use section::COUNT as SECTIONS;

/// How [`Out`] writes strings: inline, or as indices into a pool.
trait Strings {
    /// The index to write for `s`, or `None` to write it inline.
    fn index(&mut self, s: &str) -> Option<usize>;
    /// Whether instructions are written split by section.
    fn sectioned(&self) -> bool {
        false
    }
}

/// Strings written inline (single code objects, for snapshots).
struct Inline;

impl Strings for Inline {
    fn index(&mut self, _: &str) -> Option<usize> {
        None
    }
}

/// Strings pooled in order of first use (whole programs, at build time),
/// after any shared ones; `.1` for the sectioned form.
#[derive(Default)]
struct Pool(BTreeMap<String, usize>, bool);

impl Strings for Pool {
    fn index(&mut self, s: &str) -> Option<usize> {
        let next = self.0.len();
        Some(*self.0.entry(s.into()).or_insert(next))
    }

    fn sectioned(&self) -> bool {
        self.1
    }
}

/// A writer: the sink takes each piece with the section it belongs to
/// (`.2`, [`section::META`] unless a field sets another), which only the
/// sectioned form keeps apart.
struct Out<'a, P>(&'a mut dyn FnMut(usize, &[u8]), P, usize);

impl<P: Strings> Out<'_, P> {
    fn put(&mut self, b: &[u8]) {
        (self.0)(self.2, b);
    }

    fn byte(&mut self, b: u8) {
        self.put(&[b]);
    }

    /// Runs `f` writing to section `k`.
    fn to(&mut self, k: usize, f: impl FnOnce(&mut Self)) {
        let saved = core::mem::replace(&mut self.2, k);
        f(self);
        self.2 = saved;
    }

    fn u64(&mut self, mut v: u64) {
        loop {
            let b = (v & 0x7f) as u8;
            v >>= 7;
            if v == 0 {
                self.byte(b);
                return;
            }
            self.byte(b | 0x80);
        }
    }

    fn u32(&mut self, v: u32) {
        self.u64(u64::from(v));
    }

    fn len(&mut self, n: usize) {
        self.u64(n as u64);
    }

    fn bool(&mut self, b: bool) {
        self.byte(u8::from(b));
    }

    fn bytes(&mut self, b: &[u8]) {
        self.len(b.len());
        self.put(b);
    }

    /// Instruction words as their count, then each instruction packed: its
    /// opcode byte, then each operand in its width (1, 2 or 3 bytes, little
    /// endian), which is each word's low [`word_bytes`] bytes. Other bits
    /// are not kept; verified code has them zero.
    fn words(&mut self, w: &[u32]) {
        self.len(w.len());
        if self.1.sectioned() {
            self.split_words(w);
            return;
        }
        let mut pc = 0;
        while let Some(&w0) = w.get(pc) {
            let (n0, n1) = word_bytes(opcode(w, pc));
            self.put(&w0.to_le_bytes()[..n0]);
            if n1 > 0 {
                let w1 = w.get(pc + 1).copied().unwrap_or(0);
                self.put(&w1.to_le_bytes()[..n1]);
            }
            pc += 1 + usize::from(n1 > 0);
        }
    }

    /// The sectioned form of [`Out::words`]: each opcode to
    /// [`section::OP`], each operand to the section of its kind.
    #[allow(
        clippy::cast_possible_truncation,
        reason = "operands are at most 24 bits; bytes are taken on purpose"
    )]
    fn split_words(&mut self, w: &[u32]) {
        let mut next_ic = 0;
        let mut pc = 0;
        while pc < w.len() {
            let op = opcode(w, pc);
            self.to(section::OP, |o| o.byte(op));
            let l = layout_of(op);
            for (k, kind) in FORMATS[op as usize].bytes().enumerate() {
                let f = l.fields[k];
                let v = w.get(pc + f.word as usize).copied().unwrap_or(0) >> f.shift
                    & ((1 << f.width) - 1);
                match kind {
                    b'r' => self.to(section::REG, |o| o.byte(v as u8)),
                    b'b' => self.to(section::BYTE, |o| o.byte(v as u8)),
                    b'u' => {
                        let x = if v == next_ic {
                            next_ic += 1;
                            0
                        } else {
                            v + 1
                        };
                        // Past 254, 255 and the rest in I.
                        self.to(section::U, |o| o.byte(x.min(255) as u8));
                        if x >= 255 {
                            self.to(section::I, |o| o.u32(x - 255));
                        }
                    }
                    b'i' => {
                        // Zigzag, as the 16-bit immediate is signed.
                        let i = (v as u16).cast_signed();
                        self.to(section::I, |o| {
                            o.u32(u32::from(((i << 1) ^ (i >> 15)).cast_unsigned()));
                        });
                    }
                    _ => {
                        let d = i64::from(v) - i64::try_from(pc).unwrap_or(0);
                        self.to(section::JUMP, |o| {
                            o.u64(((d << 1) ^ (d >> 63)).cast_unsigned());
                        });
                    }
                }
            }
            pc += instr_len(op);
        }
    }

    fn str(&mut self, s: &str) {
        match self.1.index(s) {
            Some(i) => self.to(section::INDEX, |o| o.len(i)),
            None => self.bytes(s.as_bytes()),
        }
    }

    fn strs<S: AsRef<str>>(&mut self, v: &[S]) {
        self.len(v.len());
        for s in v {
            self.str(s.as_ref());
        }
    }

    fn pos(&mut self, p: Pos) {
        for x in [p.line, p.col, p.end_line, p.end_col] {
            self.u32(x);
        }
    }

    fn code(&mut self, c: &Code) {
        self.str(&c.name);
        self.str(&c.qualname);
        let p = &c.params;
        let extras = !(c.freevars.is_empty()
            && c.freenames.is_empty()
            && c.exc_table.is_empty()
            && c.positions.is_empty()
            && c.reg_names.is_empty());
        let flags = [
            c.is_module,
            c.is_coroutine,
            c.is_generator,
            c.doc.is_some(),
            p.varargs,
            p.varkw,
            extras,
        ];
        self.byte(
            flags
                .iter()
                .enumerate()
                .fold(0, |acc, (i, &f)| acc | u8::from(f) << i),
        );
        self.u32(c.first_line);
        self.words(&c.bytecode);
        self.len(c.consts.len());
        for k in &c.consts {
            match k {
                Konst::None => self.byte(0),
                Konst::Bool(b) => {
                    self.byte(1);
                    self.bool(*b);
                }
                Konst::Ellipsis => self.byte(2),
                Konst::Int(i) => {
                    self.byte(3);
                    self.u64(i.cast_unsigned());
                }
                Konst::Float(f) => {
                    self.byte(4);
                    self.u64(f.to_bits());
                }
                Konst::Str(s) => {
                    self.byte(5);
                    self.str(s);
                }
                Konst::Names(v) => {
                    self.byte(6);
                    self.strs(v);
                }
                Konst::BigInt(s) => {
                    self.byte(7);
                    self.str(s);
                }
                Konst::Bytes(b) => {
                    self.byte(8);
                    self.bytes(b);
                }
                Konst::Complex(re, im) => {
                    self.byte(9);
                    self.u64(re.to_bits());
                    self.u64(im.to_bits());
                }
            }
        }
        self.strs(&c.names);
        self.u32(c.nregs);
        self.strs(&c.varnames);
        self.u32(p.argcount);
        self.u32(p.posonly);
        self.u32(p.kwonly);
        if extras {
            self.extras(c);
        }
        self.u32(c.n_ics);
        if let Some(d) = &c.doc {
            self.str(d);
        }
    }

    /// The tables engine code has none of.
    fn extras(&mut self, c: &Code) {
        self.bytes(&c.freevars);
        self.strs(&c.freenames);
        self.len(c.exc_table.len());
        for e in &c.exc_table {
            for x in [e.start, e.end, e.handler, e.depth] {
                self.u32(x);
            }
        }
        self.len(c.positions.len());
        for e in &c.positions {
            self.u32(e.pc);
            self.pos(e.pos);
            let a = e.anchor;
            self.byte(a.kind);
            for x in [a.left_line, a.left_col, a.right_line, a.right_col] {
                self.u32(x);
            }
            self.bool(e.no_carets);
        }
        self.len(c.reg_names.len());
        for r in &c.reg_names {
            self.u32(r.start);
            self.u32(r.end);
            self.byte(r.reg);
            self.str(&r.name);
        }
    }
}

/// Why a bounded code decode failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodeError {
    /// Truncated or malformed bytecode encoding.
    Invalid,
    /// The decoded buffers exceed the supplied allocation budget.
    TooLarge,
}

/// A reader. Everything but string references and instructions comes
/// from `meta`; in the sectioned form string references come from `index`
/// and instructions from `code` (sections [`section::OP`] to
/// [`section::JUMP`]), otherwise from `meta` too. `pool` holds the strings
/// read so far, `budget` what may still be allocated (`None` once
/// exceeded).
struct In<'a> {
    meta: &'a [u8],
    index: &'a [u8],
    code: [&'a [u8]; SECTIONS - section::OP],
    pool: Vec<Arc<str>>,
    budget: Option<u64>,
    split: bool,
    /// The shared strings (index `i < shared.len()`), each made an
    /// `Arc` on its first use (`memo`); `pool` holds the rest.
    shared: SharedStrings<'a>,
    memo: Vec<Option<Arc<str>>>,
}

#[inline]
fn byte(s: &mut &[u8]) -> Option<u8> {
    let (&b, rest) = s.split_first()?;
    *s = rest;
    Some(b)
}

#[inline]
fn leb(s: &mut &[u8]) -> Option<u64> {
    let b = byte(s)?;
    if b & 0x80 == 0 {
        return Some(u64::from(b));
    }
    let mut v = u64::from(b & 0x7f);
    for shift in (7..64).step_by(7) {
        let b = byte(s)?;
        v |= u64::from(b & 0x7f) << shift;
        if b & 0x80 == 0 {
            return Some(v);
        }
    }
    None
}

/// A table of strings that bundled programs share
/// ([`Program::encode_sections`]): the strings one after another, and
/// where each ends, as little-endian `u16`s. Kept as it is in the binary;
/// a decode makes an `Arc` only of the strings its program uses.
#[derive(Clone, Copy, Debug, Default)]
pub struct SharedStrings<'a> {
    text: &'a str,
    ends: &'a [u8],
}

impl<'a> SharedStrings<'a> {
    /// The table [`SharedStrings::build`] wrote.
    #[must_use]
    pub const fn new(text: &'a str, ends: &'a [u8]) -> SharedStrings<'a> {
        SharedStrings { text, ends }
    }

    /// `strings` as a table's text and ends (at build time); `None` if the
    /// text is longer than 65,535 bytes.
    #[must_use]
    pub fn build(strings: &[&str]) -> Option<(String, Vec<u8>)> {
        let mut text = String::new();
        let mut ends = Vec::new();
        for s in strings {
            text.push_str(s);
            ends.extend_from_slice(&u16::try_from(text.len()).ok()?.to_le_bytes());
        }
        Some((text, ends))
    }

    /// How many strings there are.
    #[must_use]
    pub fn len(&self) -> usize {
        self.ends.len() / 2
    }

    /// Whether there are none.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.ends.len() < 2
    }

    fn end(&self, i: usize) -> Option<usize> {
        let b = self.ends.get(2 * i..2 * i + 2)?;
        Some(usize::from(u16::from_le_bytes([b[0], b[1]])))
    }

    fn get(&self, i: usize) -> Option<&'a str> {
        let start = if i == 0 { 0 } else { self.end(i - 1)? };
        self.text.get(start..self.end(i)?)
    }
}

impl<'a> In<'a> {
    fn new(bytes: &'a [u8], budget: u64) -> In<'a> {
        In {
            meta: bytes,
            index: &[],
            code: [&[]; SECTIONS - section::OP],
            pool: Vec::new(),
            budget: Some(budget),
            split: false,
            shared: SharedStrings::default(),
            memo: Vec::new(),
        }
    }

    fn charge(&mut self, bytes: u64) -> Option<()> {
        self.budget = self.budget?.checked_sub(bytes);
        self.budget.map(|_| ())
    }

    fn byte(&mut self) -> Option<u8> {
        byte(&mut self.meta)
    }

    fn u64(&mut self) -> Option<u64> {
        leb(&mut self.meta)
    }

    fn u32(&mut self) -> Option<u32> {
        u32::try_from(self.u64()?).ok()
    }

    fn len(&mut self) -> Option<usize> {
        let n = usize::try_from(self.u64()?).ok()?;
        // A length never exceeds the bytes left (each item takes at least
        // one; in the sectioned form, perhaps in another section).
        let mut left = self.meta.len();
        if self.split {
            left += self.index.len() + self.code.iter().map(|s| s.len()).sum::<usize>();
        }
        (n <= left).then_some(n)
    }

    fn bool(&mut self) -> Option<bool> {
        match self.byte()? {
            0 => Some(false),
            1 => Some(true),
            _ => None,
        }
    }

    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let (b, rest) = self.meta.split_at_checked(n)?;
        self.meta = rest;
        Some(b)
    }

    fn bytes(&mut self) -> Option<Vec<u8>> {
        let n = self.len()?;
        self.charge(n as u64)?;
        self.take(n).map(<[u8]>::to_vec)
    }

    fn words(&mut self) -> Option<Vec<u32>> {
        if self.split {
            let n = usize::try_from(self.u64()?).ok()?;
            // At most two words per opcode.
            if n > 2 * self.code[0].len() {
                return None;
            }
            self.charge(n as u64 * 4)?;
            return self.split_words(n);
        }
        let n = self.len()?;
        self.charge(n as u64 * 4)?;
        let mut out = Vec::with_capacity(n);
        while out.len() < n {
            let s = self.meta;
            let (n0, n1) = word_bytes(*s.first()?);
            let x = match s.first_chunk::<8>() {
                Some(c) => u64::from_le_bytes(*c) & (u64::MAX >> (64 - 8 * (n0 + n1))),
                None => s
                    .get(..n0 + n1)?
                    .iter()
                    .rev()
                    .fold(0, |x, &b| x << 8 | u64::from(b)),
            };
            self.meta = s.get(n0 + n1..)?;
            #[allow(clippy::cast_possible_truncation, reason = "each word is 32 bits")]
            {
                out.push((x & ((1 << (8 * n0)) - 1)) as u32);
                if n1 > 0 {
                    out.push((x >> (8 * n0)) as u32);
                }
            }
        }
        (out.len() == n).then_some(out)
    }

    /// [`Out::split_words`]'s `n` words back.
    #[allow(
        clippy::cast_possible_truncation,
        reason = "pcs and operands are checked to fit"
    )]
    fn split_words(&mut self, n: usize) -> Option<Vec<u32>> {
        let [mut ops, mut reg, mut bytes, mut u, mut i, mut jump] = self.code;
        let mut out = alloc::vec![0u32; n];
        let mut next_ic = 0u32;
        let mut pc = 0;
        while pc < n {
            let op = byte(&mut ops)?;
            let mut plan = operand_plan(op);
            // Both words, the second in the high half.
            let mut w = u64::from(op);
            while plan != 0 {
                // `r` and `b` take 8 bits and `i` 16, so only `u` and `j`
                // can overflow their fields (16 and 24 bits).
                let v = match plan & 7 {
                    1 => u32::from(byte(&mut reg)?),
                    2 => u32::from(byte(&mut bytes)?),
                    3 => {
                        let mut x = u32::from(byte(&mut u)?);
                        if x == 255 {
                            x = u32::try_from(leb(&mut i)?).ok()?.checked_add(255)?;
                        }
                        let x = if x == 0 {
                            next_ic += 1;
                            next_ic - 1
                        } else {
                            x - 1
                        };
                        if x >> 16 != 0 {
                            return None;
                        }
                        x
                    }
                    4 => {
                        let z = u32::try_from(leb(&mut i)?).ok()?;
                        if z >> 16 != 0 {
                            return None;
                        }
                        (z >> 1 ^ (z & 1).wrapping_neg()) & 0xffff
                    }
                    _ => {
                        let z = leb(&mut jump)?;
                        let d = (z >> 1).cast_signed() ^ -((z & 1).cast_signed());
                        let t = u32::try_from(i64::try_from(pc).ok()?.checked_add(d)?).ok()?;
                        if t >> 24 != 0 {
                            return None;
                        }
                        t
                    }
                };
                w |= u64::from(v) << (plan >> 3 & 63);
                plan >>= 9;
            }
            *out.get_mut(pc)? = w as u32;
            if instr_len(op) == 2 {
                *out.get_mut(pc + 1)? = (w >> 32) as u32;
                pc += 2;
            } else {
                pc += 1;
            }
        }
        self.code = [ops, reg, bytes, u, i, jump];
        (pc == n).then_some(out)
    }

    /// A pool string, shared with every code object naming it (its bytes
    /// are still charged per use, as an owned copy would be).
    fn shared(&mut self) -> Option<Arc<str>> {
        if !self.split && self.pool.is_empty() {
            // Inline (a single code object), or the pool itself.
            let n = self.len()?;
            self.charge(n as u64)?;
            return core::str::from_utf8(self.take(n)?).ok().map(Arc::from);
        }
        let i = usize::try_from(leb(if self.split {
            &mut self.index
        } else {
            &mut self.meta
        })?)
        .ok()?;
        if let Some(slot) = self.memo.get(i) {
            if let Some(s) = slot {
                let s = s.clone();
                self.charge(s.len() as u64)?;
                return Some(s);
            }
            let s: Arc<str> = Arc::from(self.shared.get(i)?);
            // The memo's copy, then this use.
            self.charge(2 * s.len() as u64)?;
            self.memo[i] = Some(s.clone());
            return Some(s);
        }
        let i = i - self.memo.len();
        self.charge(self.pool.get(i)?.len() as u64)?;
        self.pool.get(i).cloned()
    }

    /// The pool itself, appended to `pool`: one allocation per string,
    /// straight from the input.
    fn read_pool(&mut self) -> Option<()> {
        let n = self.len()?;
        self.charge((n as u64).checked_mul(size_of::<Arc<str>>() as u64)?)?;
        self.pool.reserve_exact(n);
        for _ in 0..n {
            let n = self.len()?;
            self.charge(n as u64)?;
            let s = core::str::from_utf8(self.take(n)?).ok()?;
            self.pool.push(Arc::from(s));
        }
        Some(())
    }

    fn shareds(&mut self) -> Option<Vec<Arc<str>>> {
        self.list(Self::shared)
    }

    fn str(&mut self) -> Option<String> {
        self.shared().map(|s| String::from(&*s))
    }

    fn strs(&mut self) -> Option<Vec<String>> {
        self.list(Self::str)
    }

    // Exact capacities keep a decoded code no larger than the loaded clone
    // that produced it, including when only one or two elements are present.
    fn list<T>(&mut self, read: fn(&mut Self) -> Option<T>) -> Option<Vec<T>> {
        let n = self.len()?;
        self.charge((n as u64).checked_mul(size_of::<T>() as u64)?)?;
        let mut values = Vec::with_capacity(n);
        for _ in 0..n {
            values.push(read(self)?);
        }
        Some(values)
    }

    fn pos(&mut self) -> Option<Pos> {
        Some(Pos {
            line: self.u32()?,
            col: self.u32()?,
            end_line: self.u32()?,
            end_col: self.u32()?,
        })
    }

    fn konst(&mut self) -> Option<Konst> {
        Some(match self.byte()? {
            0 => Konst::None,
            1 => Konst::Bool(self.bool()?),
            2 => Konst::Ellipsis,
            3 => Konst::Int(self.u64()?.cast_signed()),
            4 => Konst::Float(f64::from_bits(self.u64()?)),
            5 => Konst::Str(self.str()?),
            6 => Konst::Names(self.strs()?),
            7 => Konst::BigInt(self.str()?),
            8 => Konst::Bytes(self.bytes()?),
            9 => Konst::Complex(f64::from_bits(self.u64()?), f64::from_bits(self.u64()?)),
            _ => return None,
        })
    }

    fn code(&mut self) -> Option<Code> {
        let name = self.shared()?;
        let qualname = self.shared()?;
        let flags = self.byte()?;
        if flags >> 7 != 0 {
            return None;
        }
        let flag = |i: u8| flags >> i & 1 != 0;
        let first_line = self.u32()?;
        let bytecode = self.words()?;
        let consts = self.list(Self::konst)?;
        let names = self.shareds()?;
        let nregs = self.u32()?;
        let varnames = self.shareds()?;
        let params = ParamSpec {
            argcount: self.u32()?,
            posonly: self.u32()?,
            kwonly: self.u32()?,
            varargs: flag(4),
            varkw: flag(5),
        };
        let mut code = Code {
            name,
            qualname,
            first_line,
            bytecode,
            consts,
            names,
            nregs,
            varnames,
            params,
            freevars: Vec::new(),
            freenames: Vec::new(),
            exc_table: Vec::new(),
            positions: Vec::new(),
            reg_names: Vec::new(),
            n_ics: 0,
            is_module: flag(0),
            is_coroutine: flag(1),
            is_generator: flag(2),
            doc: None,
        };
        if flag(6) {
            self.extras(&mut code)?;
        }
        code.n_ics = self.u32()?;
        if flag(3) {
            code.doc = Some(self.str()?);
        }
        Some(code)
    }

    fn extras(&mut self, c: &mut Code) -> Option<()> {
        c.freevars = self.bytes()?;
        c.freenames = self.shareds()?;
        c.exc_table = self.list(|r| {
            Some(ExcEntry {
                start: r.u32()?,
                end: r.u32()?,
                handler: r.u32()?,
                depth: r.u32()?,
            })
        })?;
        c.positions = self.list(|r| {
            Some(PosEntry {
                pc: r.u32()?,
                pos: r.pos()?,
                anchor: Anchor {
                    kind: r.byte()?,
                    left_line: r.u32()?,
                    left_col: r.u32()?,
                    right_line: r.u32()?,
                    right_col: r.u32()?,
                },
                no_carets: r.bool()?,
            })
        })?;
        c.reg_names = self.list(|r| {
            Some(RegName {
                start: r.u32()?,
                end: r.u32()?,
                reg: r.byte()?,
                name: r.str()?,
            })
        })?;
        Some(())
    }
}

impl Code {
    /// Appends the binary form of one code object ([`Code::decode`] reads
    /// it), for snapshots.
    pub fn encode_into(&self, out: &mut Vec<u8>) {
        self.encode_to(&mut |b| out.extend_from_slice(b));
    }

    /// Streams the same binary form without allocating an output buffer.
    pub fn encode_to(&self, out: &mut dyn FnMut(&[u8])) {
        Out(&mut |_, b| out(b), Inline, section::META).code(self);
    }

    /// Reads one code object [`Code::encode_into`] wrote from the front of
    /// `bytes`, advancing it; `None` if it is malformed. The result is
    /// unverified (see [`Code::verify`]).
    pub fn decode(bytes: &mut &[u8]) -> Option<Code> {
        let mut budget = u64::MAX;
        Self::decode_bounded(bytes, &mut budget).ok()
    }

    /// Like [`Code::decode`], but checks every buffer allocation against
    /// `budget` before allocating. On success deducts the buffers' bytes;
    /// on failure leaves both arguments unchanged. Excludes the inline
    /// `Code`, allocator overhead and [`Code::verify`]'s scratch space.
    pub fn decode_bounded(bytes: &mut &[u8], budget: &mut u64) -> Result<Code, DecodeError> {
        let mut r = In::new(bytes, *budget);
        let c = r.code().ok_or(if r.budget.is_none() {
            DecodeError::TooLarge
        } else {
            DecodeError::Invalid
        })?;
        *bytes = r.meta;
        *budget = r.budget.unwrap_or(0);
        Ok(c)
    }
}

impl Program {
    /// The binary form [`Program::decode`] reads (warnings are dropped).
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        for part in self.encode_parts(&[], false) {
            out.extend_from_slice(&part);
        }
        out
    }

    /// The sectioned form [`Program::decode_sections`] reads: one byte
    /// string per [`section`]. Strings in `shared` are written as their
    /// index there, and the others are pooled after them.
    #[must_use]
    pub fn encode_sections(&self, shared: &[&str]) -> [Vec<u8>; SECTIONS] {
        self.encode_parts(shared, true)
    }

    /// Every distinct string the encoded forms refer to.
    #[must_use]
    pub fn strings(&self) -> Vec<String> {
        let mut body = Out(&mut |_, _| {}, Pool::default(), section::META);
        for c in &self.codes {
            body.code(c);
        }
        body.1.0.into_keys().collect()
    }

    /// The pool, then the code objects that refer to it. Unsectioned, every
    /// part but [`section::POOL`] is empty and the pool comes first.
    fn encode_parts(&self, shared: &[&str], split: bool) -> [Vec<u8>; SECTIONS] {
        let mut parts: [Vec<u8>; SECTIONS] = Default::default();
        let mut pool = Pool(BTreeMap::new(), split);
        for (i, s) in shared.iter().enumerate() {
            pool.0.insert(String::from(*s), i);
        }
        let mut body = Vec::new();
        let mut write = |k: usize, b: &[u8]| {
            if split {
                parts[k].extend_from_slice(b);
            } else {
                body.extend_from_slice(b);
            }
        };
        let mut out = Out(&mut write, pool, section::META);
        out.len(self.codes.len());
        for c in &self.codes {
            out.code(c);
        }
        out.u32(self.entry);
        out.bool(self.returns_last_expr);
        // Pooled strings in order of first use, after the shared ones.
        let mut own: Vec<(usize, String)> = core::mem::take(&mut out.1.0)
            .into_iter()
            .filter(|&(_, i)| i >= shared.len())
            .map(|(s, i)| (i, s))
            .collect();
        own.sort_unstable();
        let mut head = Vec::new();
        let mut write = |_: usize, b: &[u8]| head.extend_from_slice(b);
        let mut out = Out(&mut write, Inline, section::POOL);
        out.len(own.len());
        for (_, s) in &own {
            out.str(s);
        }
        head.extend_from_slice(&body);
        let mut pool = core::mem::take(&mut parts[section::POOL]);
        head.append(&mut pool);
        parts[section::POOL] = head;
        parts
    }

    /// Reads [`Program::encode`]'s output; `None` if it is malformed.
    #[must_use]
    pub fn decode(bytes: &[u8]) -> Option<Program> {
        let mut budget = u64::MAX;
        Self::decode_bounded(bytes, &mut budget).ok()
    }

    /// Like [`Program::decode`], admitting every allocation before copying.
    /// On success deducts decoded buffers and the temporary string pool from
    /// `budget`; on failure leaves it unchanged. Excludes the borrowed input,
    /// allocator overhead and code verification workspace.
    pub fn decode_bounded(bytes: &[u8], budget: &mut u64) -> Result<Program, DecodeError> {
        let mut r = In::new(bytes, *budget);
        let result = (|| {
            r.pool = r.shareds()?;
            r.program()
        })();
        r.finish(result, budget)
    }

    /// Reads [`Program::encode_sections`]'s output, with `shared` the
    /// table it was encoded against. Admits allocations as
    /// [`Program::decode_bounded`] does, with a slot for every shared
    /// string and a copy of each one used.
    pub fn decode_sections(
        sections: &[&[u8]; SECTIONS],
        shared: SharedStrings<'_>,
        budget: &mut u64,
    ) -> Result<Program, DecodeError> {
        let mut r = In::new(sections[section::POOL], *budget);
        r.index = sections[section::INDEX];
        r.code = core::array::from_fn(|k| sections[section::OP + k]);
        r.split = true;
        r.shared = shared;
        let result = (|| {
            let n = shared.len();
            r.charge((n as u64).checked_mul(size_of::<Option<Arc<str>>>() as u64)?)?;
            r.memo.resize(n, None);
            r.read_pool()?;
            if !r.meta.is_empty() {
                return None;
            }
            r.meta = sections[section::META];
            let program = r.program()?;
            (r.index.is_empty() && r.code.iter().all(|s| s.is_empty())).then_some(program)
        })();
        r.finish(result, budget)
    }
}

impl In<'_> {
    /// The code objects, entry and flag of a program, after its pool.
    fn program(&mut self) -> Option<Program> {
        let codes = self.list(|r| {
            // The shared allocation each code object now lives in.
            r.charge(size_of::<Code>() as u64 + 2 * size_of::<usize>() as u64)?;
            In::code(r).map(Arc::new)
        })?;
        let entry = self.u32()?;
        let returns_last_expr = self.bool()?;
        (self.meta.is_empty() && (entry as usize) < codes.len()).then_some(Program {
            codes,
            entry,
            returns_last_expr,
            warnings: Vec::new(),
        })
    }

    /// `result`, deducting what was admitted from `budget` on success.
    fn finish<T>(&self, result: Option<T>, budget: &mut u64) -> Result<T, DecodeError> {
        let value = result.ok_or(if self.budget.is_none() {
            DecodeError::TooLarge
        } else {
            DecodeError::Invalid
        })?;
        *budget = self.budget.unwrap_or(0);
        Ok(value)
    }
}
