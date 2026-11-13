//! `re`'s matcher: a backtracking machine over the program `re.py`
//! compiles (docs/spec.md §12 `re`). The pattern parser (a port of
//! CPython's `re._parser`) and the compiler are Python; this file runs
//! compiled programs over `str` (UTF-8, positions at char boundaries) or
//! `bytes` subjects.
//!
//! The machine follows CPython's `sre` semantics (`Modules/_sre/sre_lib.h`,
//! PSF license; see `THIRD_PARTY.md`): `REPEAT`/`MAX_UNTIL`/`MIN_UNTIL`
//! with their empty-iteration rule, single-width `REPEAT_ONE`, atomic
//! groups, lookaround and `must_advance`. It differs in how it backtracks:
//! one explicit stack of choice points and undo records (no Rust
//! recursion), so a run can stop after any step, save itself into the
//! arena and continue in a later call. `re.py` loops over `_exec` calls;
//! each loop iteration is a safepoint, so a catastrophic pattern burns
//! fuel and parks instead of hanging the host.

use crate::types::{T, ty};
use crate::vm::{CallArgs, Raised, Vm};
use alloc::{format, vec, vec::Vec};
use core::cmp::Ordering;
use mm_core::{Heap, Scratch, Value};

/// First word of every program.
const MAGIC: u32 = 0x5245_4D31;
/// Header words: magic, groups (with group 0), repeat registers, 1 for a
/// `str` pattern (0 for `bytes`).
const HDR: usize = 4;
/// An unset register.
const NONE: u32 = u32::MAX;
/// `max` of an unbounded repeat.
const INF: u32 = u32::MAX;
/// Steps per `_exec` call before the run saves itself and returns.
const SLICE: u64 = 1 << 17;
/// Steps per fuel unit.
const STEPS_PER_FUEL: u64 = 32;

/// Backtrack-stack words a call may push without a charge.
const STACK_FREE: usize = 1 << 16;

/// Bytes charged per backtrack-stack word: the word, and its saved word
/// and bytes when the call ends a slice.
const STACK_WORD_COST: u64 = 12;

/// Span words of matches a call collects before it ends its slice.
const FOUND_WORDS: usize = 1 << 16;

/// Opcodes (the compiler in `re.py` emits these numbers).
mod op {
    pub const FAIL: u32 = 0;
    pub const MATCH: u32 = 1;
    /// `CHAR c`
    pub const CHAR: u32 = 2;
    pub const NOT_CHAR: u32 = 3;
    /// `CHAR_I c mode`: case-insensitive (mode 1 ASCII, 2 Unicode).
    pub const CHAR_I: u32 = 4;
    pub const NOT_CHAR_I: u32 = 5;
    pub const ANY: u32 = 6;
    pub const ANY_ALL: u32 = 7;
    /// `SET mode skip items… 0`: mode 0 exact, 1 ASCII fold, 2 Unicode fold.
    pub const SET: u32 = 8;
    /// `AT kind`
    pub const AT: u32 = 9;
    /// `JMP target`
    pub const JMP: u32 = 10;
    /// `SPLIT a b`: try `a`, then `b`.
    pub const SPLIT: u32 = 11;
    /// `SAVE slot`
    pub const SAVE: u32 = 12;
    /// `REP1 kind min max next item…`: a single-width item repeated.
    pub const REP1: u32 = 13;
    /// `REP_INIT r until`
    pub const REP_INIT: u32 = 14;
    /// `REP_UNTIL r min max lazy body`
    pub const REP_UNTIL: u32 = 15;
    /// `ATOMIC`
    pub const ATOMIC: u32 = 16;
    /// `LOOK kind width next`: kind bit 0 negative, bit 1 behind.
    pub const LOOK: u32 = 17;
    /// `END`: ends an atomic group or lookaround.
    pub const END: u32 = 18;
    /// `GROUPREF g mode`
    pub const GROUPREF: u32 = 19;
    /// `GROUP_EXISTS g no`
    pub const GROUP_EXISTS: u32 = 20;
}

/// Set items.
mod set {
    pub const END: u32 = 0;
    pub const LIT: u32 = 1;
    pub const RANGE: u32 = 2;
    pub const CAT: u32 = 3;
    pub const NEGATE: u32 = 4;
}

/// Backtrack frame tags (the last word of each frame).
mod bt {
    /// `[pc, pos]`
    pub const ALT: u32 = 1;
    /// `[slot, old]`
    pub const UNDO: u32 = 2;
    /// `[next, min, count, pos, lit]`: a greedy single-width repeat.
    pub const REPG: u32 = 3;
    /// `[next, item, max, count, pos]`: a lazy single-width repeat.
    pub const REPL: u32 = 4;
    /// `[kind, pos, next]`: an atomic group (kind 4) or lookaround.
    pub const BARRIER: u32 = 5;
    /// `[r, count, pos, body, max]`: a lazy repeat's next iteration.
    pub const LAZY: u32 = 6;
}

const ATOMIC_KIND: u32 = 4;

/// Words of a frame with tag `t`, the tag included.
#[cfg_attr(mm_shipped, optimize(size))]
fn frame_len(t: u32) -> usize {
    match t {
        bt::ALT | bt::UNDO => 3,
        bt::BARRIER => 4,
        bt::REPG | bt::REPL | bt::LAZY => 6,
        _ => 1,
    }
}

// ------------------------------------------------------------ characters

/// Python's `str.lower()` of one code point, simple (one-char) mapping.
#[cfg_attr(mm_shipped, optimize(size))]
fn lower_uni(c: u32) -> u32 {
    if c < 0x80 {
        return u32::from((c as u8).to_ascii_lowercase());
    }
    match char::from_u32(c) {
        Some(ch) => ch.to_lowercase().next().map_or(c, u32::from),
        None => c,
    }
}

/// The simple uppercase mapping (unchanged when the full one expands).
#[cfg_attr(mm_shipped, optimize(size))]
fn upper_uni(c: u32) -> u32 {
    if c < 0x80 {
        return u32::from((c as u8).to_ascii_uppercase());
    }
    match char::from_u32(c) {
        Some(ch) => {
            let mut it = ch.to_uppercase();
            match (it.next(), it.next()) {
                (Some(u), None) => u32::from(u),
                _ => c,
            }
        }
        None => c,
    }
}

#[cfg_attr(mm_shipped, optimize(size))]
fn lower_ascii(c: u32) -> u32 {
    if (u32::from(b'A')..=u32::from(b'Z')).contains(&c) {
        c + 32
    } else {
        c
    }
}

#[cfg_attr(mm_shipped, optimize(size))]
fn upper_ascii(c: u32) -> u32 {
    if (u32::from(b'a')..=u32::from(b'z')).contains(&c) {
        c - 32
    } else {
        c
    }
}

#[cfg_attr(mm_shipped, optimize(size))]
fn is_space_uni(c: u32) -> bool {
    (0x1C..=0x1F).contains(&c) || char::from_u32(c).is_some_and(char::is_whitespace)
}

#[cfg_attr(mm_shipped, optimize(size))]
fn is_word_uni(c: u32) -> bool {
    c == u32::from(b'_') || char::from_u32(c).is_some_and(char::is_alphanumeric)
}

#[cfg_attr(mm_shipped, optimize(size))]
fn is_word_ascii(c: u32) -> bool {
    c < 0x80 && ((c as u8).is_ascii_alphanumeric() || c == u32::from(b'_'))
}

/// Category `k` (the low 3 bits: digit, not digit, space, not space, word,
/// not word; bit 3: Unicode).
#[cfg_attr(mm_shipped, optimize(size))]
fn category(k: u32, c: u32) -> bool {
    let uni = k & 8 != 0;
    let r = match (k & 7) >> 1 {
        0 => {
            if uni {
                crate::text::is_decimal(c)
            } else {
                (0x30..=0x39).contains(&c)
            }
        }
        1 => {
            if uni {
                is_space_uni(c)
            } else {
                matches!(c, 9..=13 | 32)
            }
        }
        _ => {
            if uni {
                is_word_uni(c)
            } else {
                is_word_ascii(c)
            }
        }
    };
    r != (k & 1 != 0)
}

// ------------------------------------------------------------- subjects

/// A subject: UTF-8 text or bytes, cut at `end` (the byte offset of
/// `endpos`).
struct Subj<'a> {
    b: &'a [u8],
    text: bool,
    end: usize,
}

impl Subj<'_> {
    /// The code point at `pos` (< `end`) and the offset after it.
    #[inline]
    #[cfg_attr(mm_shipped, optimize(size))]
    fn next(&self, pos: usize) -> Option<(u32, usize)> {
        if pos >= self.end {
            return None;
        }
        let b0 = *self.b.get(pos)?;
        if !self.text || b0 < 0x80 {
            return Some((u32::from(b0), pos + 1));
        }
        let n = if b0 >= 0xF0 {
            4
        } else if b0 >= 0xE0 {
            3
        } else {
            2
        };
        let s = self.b.get(pos..pos + n)?;
        let mut c = u32::from(b0) & (0x7F >> n);
        for &x in &s[1..] {
            c = (c << 6) | (u32::from(x) & 0x3F);
        }
        Some((c, pos + n))
    }

    /// The code point before `pos` (> 0) and its offset.
    #[inline]
    #[cfg_attr(mm_shipped, optimize(size))]
    fn prev(&self, pos: usize) -> Option<(u32, usize)> {
        if pos == 0 {
            return None;
        }
        let mut p = pos - 1;
        if self.text {
            while p > 0 && self.b.get(p).is_some_and(|&x| x & 0xC0 == 0x80) {
                p -= 1;
            }
        }
        let save = self.end;
        let s = Subj {
            b: self.b,
            text: self.text,
            end: save.max(pos),
        };
        s.next(p).map(|(c, _)| (c, p))
    }
}

// ------------------------------------------------------------- the machine

/// Why a run stopped.
enum Stop {
    /// Finished (no more matches wanted or possible).
    Done,
    /// The step slice ran out; the state is saved.
    Sliced,
    /// The backtrack stack outgrew the memory limit.
    Memory,
    /// A malformed program.
    Bad,
}

/// A run: the scan over the subject and the attempt in progress.
struct Run {
    /// 0 match, 1 search, 2 fullmatch.
    mode: u32,
    /// Matches still wanted (`u32::MAX`: all).
    limit: u32,
    /// Where the current search started (`must_advance` applies there).
    search: usize,
    must_advance: bool,
    /// The attempt's start, and whether one is in progress.
    start: usize,
    active: bool,
    pc: usize,
    pos: usize,
    regs: Vec<u32>,
    stack: Vec<u32>,
    /// Stack length past which the next memory check happens.
    check_at: usize,
    /// Groups (with group 0).
    ngroups: usize,
    /// The end of the last match (segment output).
    last_end: usize,
}

/// One match: `lastindex` and the group spans as byte offsets.
struct Found {
    lastindex: u32,
    spans: Vec<u32>,
}

impl Run {
    #[cfg_attr(mm_shipped, optimize(size))]
    fn lastindex_slot(&self) -> usize {
        2 * self.ngroups
    }

    #[inline]
    #[cfg_attr(mm_shipped, optimize(size))]
    fn set_reg(&mut self, slot: usize, v: u32) {
        let Some(old) = self.regs.get(slot).copied() else {
            return;
        };
        if old == v {
            return;
        }
        if !self.stack.is_empty() {
            self.stack
                .extend_from_slice(&[u32::try_from(slot).unwrap_or(0), old, bt::UNDO]);
        }
        self.regs[slot] = v;
    }

    #[cfg_attr(mm_shipped, optimize(size))]
    fn push(&mut self, words: &[u32]) {
        self.stack.extend_from_slice(words);
    }

    /// Starts an attempt at `at`.
    #[cfg_attr(mm_shipped, optimize(size))]
    fn begin(&mut self, at: usize) {
        self.start = at;
        self.pos = at;
        self.pc = HDR;
        self.active = true;
        self.stack.clear();
        self.regs.fill(NONE);
    }

    /// Pops frames until one resumes the attempt: `true` if it did,
    /// `false` if the attempt failed.
    #[cfg_attr(mm_shipped, optimize(size))]
    fn backtrack(&mut self, code: &[u32], s: &Subj<'_>, steps: &mut u64) -> bool {
        loop {
            *steps += 1;
            let Some(&tag) = self.stack.last() else {
                return false;
            };
            let n = frame_len(tag);
            let Some(base) = self.stack.len().checked_sub(n) else {
                self.stack.clear();
                return false;
            };
            let f: [u32; 6] = {
                let mut f = [0u32; 6];
                for (d, &w) in f.iter_mut().zip(&self.stack[base..]) {
                    *d = w;
                }
                f
            };
            self.stack.truncate(base);
            match tag {
                bt::ALT => {
                    self.pc = f[0] as usize;
                    self.pos = f[1] as usize;
                    return true;
                }
                bt::UNDO => {
                    if let Some(r) = self.regs.get_mut(f[0] as usize) {
                        *r = f[1];
                    }
                }
                bt::REPG => {
                    let (next, min, mut count, mut pos, lit) =
                        (f[0], f[1], f[2], f[3] as usize, f[4]);
                    if count <= min {
                        continue;
                    }
                    while let Some((_, p)) = s.prev(pos) {
                        pos = p;
                        count -= 1;
                        *steps += 1;
                        if lit == NONE || count <= min || s.next(pos).is_some_and(|(c, _)| c == lit)
                        {
                            break;
                        }
                    }
                    if lit != NONE && s.next(pos).is_none_or(|(c, _)| c != lit) {
                        continue;
                    }
                    if count > min {
                        self.push(&[next, min, count, pos as u32, lit, bt::REPG]);
                    }
                    self.pc = next as usize;
                    self.pos = pos;
                    return true;
                }
                bt::REPL => {
                    let (next, item, max, count, pos) = (f[0], f[1], f[2], f[3], f[4] as usize);
                    if count >= max {
                        continue;
                    }
                    let Some((c, p)) = s.next(pos) else { continue };
                    if !one(code, item as usize, c, steps) {
                        continue;
                    }
                    self.push(&[next, item, max, count + 1, p as u32, bt::REPL]);
                    self.pc = next as usize;
                    self.pos = p;
                    return true;
                }
                bt::BARRIER => {
                    // The atomic group or lookaround's body failed.
                    let (kind, pos, next) = (f[0], f[1] as usize, f[2]);
                    if kind != ATOMIC_KIND && kind & 1 != 0 {
                        self.pc = next as usize;
                        self.pos = pos;
                        return true;
                    }
                }
                bt::LAZY => {
                    let (r, count, pos, body, max) =
                        (f[0] as usize, f[1], f[2] as usize, f[3], f[4]);
                    let last = self.regs.get(self.ngroups * 2 + 2 + 2 * r).copied();
                    if (max != INF && count >= max) || last == Some(pos as u32) {
                        continue;
                    }
                    let base = self.ngroups * 2 + 1 + 2 * r;
                    self.set_reg(base, count);
                    self.set_reg(base + 1, pos as u32);
                    self.pc = body as usize;
                    self.pos = pos;
                    return true;
                }
                _ => {
                    self.stack.clear();
                    return false;
                }
            }
        }
    }

    /// Cuts the stack back to the innermost barrier, keeping the undo
    /// records above it (their registers stay set); returns the barrier's
    /// `(kind, pos, next)`.
    #[cfg_attr(mm_shipped, optimize(size))]
    fn cut(&mut self) -> Option<(u32, usize, u32)> {
        let mut i = self.stack.len();
        let mut keep: Vec<u32> = Vec::new();
        while i > 0 {
            let tag = self.stack[i - 1];
            let n = frame_len(tag);
            let base = i.checked_sub(n)?;
            if tag == bt::BARRIER {
                let (kind, pos, next) = (
                    self.stack[base],
                    self.stack[base + 1] as usize,
                    self.stack[base + 2],
                );
                self.stack.truncate(base);
                // Undo records go back in their original order.
                let mut j = keep.len();
                while j >= 3 {
                    self.stack.extend_from_slice(&keep[j - 3..j]);
                    j -= 3;
                }
                return Some((kind, pos, next));
            }
            if tag == bt::UNDO {
                keep.extend_from_slice(&self.stack[base..i]);
            }
            i = base;
        }
        None
    }

    /// Unwinds to (and drops) the innermost barrier, applying undo records.
    #[cfg_attr(mm_shipped, optimize(size))]
    fn unwind(&mut self) -> Option<(u32, usize, u32)> {
        loop {
            let &tag = self.stack.last()?;
            let base = self.stack.len().checked_sub(frame_len(tag))?;
            let f = [
                self.stack[base],
                self.stack.get(base + 1).copied().unwrap_or(0),
                self.stack.get(base + 2).copied().unwrap_or(0),
            ];
            self.stack.truncate(base);
            if tag == bt::UNDO {
                if let Some(r) = self.regs.get_mut(f[0] as usize) {
                    *r = f[1];
                }
            } else if tag == bt::BARRIER {
                return Some((f[0], f[1] as usize, f[2]));
            }
        }
    }

    /// Runs the attempt until it matches (`Some(true)`), fails
    /// (`Some(false)`) or the slice ends (`None`).
    #[inline(never)]
    #[cfg_attr(mm_shipped, optimize(size))]
    fn attempt(
        &mut self,
        code: &[u32],
        s: &Subj<'_>,
        steps: &mut u64,
        budget: u64,
    ) -> Result<Option<bool>, Stop> {
        macro_rules! fail {
            () => {{
                if self.backtrack(code, s, steps) {
                    continue;
                }
                return Ok(Some(false));
            }};
        }
        let w = |i: usize| code.get(i).copied().ok_or(Stop::Bad);
        loop {
            *steps += 1;
            if *steps >= budget {
                return Ok(None);
            }
            if self.stack.len() >= self.check_at {
                return Err(Stop::Memory);
            }
            let pc = self.pc;
            match w(pc)? {
                op::FAIL => fail!(),
                op::MATCH => {
                    let empty = self.pos == self.start;
                    if (self.mode == 2 && self.pos != s.end)
                        || (self.must_advance && empty && self.start == self.search)
                    {
                        fail!();
                    }
                    self.regs[0] = self.start as u32;
                    self.regs[1] = self.pos as u32;
                    return Ok(Some(true));
                }
                op::CHAR
                | op::NOT_CHAR
                | op::CHAR_I
                | op::NOT_CHAR_I
                | op::ANY
                | op::ANY_ALL
                | op::SET => {
                    let Some((c, p)) = s.next(self.pos) else {
                        fail!()
                    };
                    if !one(code, pc, c, steps) {
                        fail!();
                    }
                    self.pos = p;
                    self.pc = pc + width(code, pc).ok_or(Stop::Bad)?;
                }
                op::AT => {
                    if !at(w(pc + 1)?, s, self.pos) {
                        fail!();
                    }
                    self.pc = pc + 2;
                }
                op::JMP => self.pc = w(pc + 1)? as usize,
                op::SPLIT => {
                    let (a, b) = (w(pc + 1)?, w(pc + 2)?);
                    self.push(&[b, self.pos as u32, bt::ALT]);
                    self.pc = a as usize;
                }
                op::SAVE => {
                    let slot = w(pc + 1)? as usize;
                    if slot >= 2 * self.ngroups {
                        return Err(Stop::Bad);
                    }
                    self.set_reg(slot, self.pos as u32);
                    if slot & 1 == 1 && slot > 1 {
                        let li = self.lastindex_slot();
                        self.set_reg(li, (slot / 2) as u32);
                    }
                    self.pc = pc + 2;
                }
                op::REP1 => {
                    let (kind, min, max, next) = (w(pc + 1)?, w(pc + 2)?, w(pc + 3)?, w(pc + 4)?);
                    let item = pc + 5;
                    let mut count = 0u32;
                    let mut pos = self.pos;
                    // The mandatory part, then (greedy, possessive) as many
                    // more as match.
                    let want = if kind == 1 { min } else { max };
                    let it = Item::of(code, item);
                    while count < want {
                        let Some((c, p)) = s.next(pos) else { break };
                        if !it.test(code, item, c, steps) {
                            break;
                        }
                        pos = p;
                        count += 1;
                        *steps += 1;
                    }
                    if count < min {
                        fail!();
                    }
                    match kind {
                        0 => {
                            // A literal right after lets backtracking skip
                            // positions it cannot match at.
                            let lit = if w(next as usize).ok() == Some(op::CHAR) {
                                w(next as usize + 1)?
                            } else {
                                NONE
                            };
                            if count > min {
                                self.push(&[next, min, count, pos as u32, lit, bt::REPG]);
                            }
                            if lit != NONE && s.next(pos).is_none_or(|(c, _)| c != lit) {
                                fail!();
                            }
                        }
                        1 if count < max => {
                            self.push(&[next, item as u32, max, count, pos as u32, bt::REPL]);
                        }
                        _ => {}
                    }
                    self.pos = pos;
                    self.pc = next as usize;
                }
                op::REP_INIT => {
                    let (r, until) = (w(pc + 1)? as usize, w(pc + 2)?);
                    let base = self.ngroups * 2 + 1 + 2 * r;
                    if base + 1 >= self.regs.len() {
                        return Err(Stop::Bad);
                    }
                    self.set_reg(base, NONE);
                    self.set_reg(base + 1, NONE);
                    self.pc = until as usize;
                }
                op::REP_UNTIL => {
                    let (r, min, max, lazy, body) = (
                        w(pc + 1)? as usize,
                        w(pc + 2)?,
                        w(pc + 3)?,
                        w(pc + 4)?,
                        w(pc + 5)?,
                    );
                    let base = self.ngroups * 2 + 1 + 2 * r;
                    let (Some(&cnt), Some(&last)) = (self.regs.get(base), self.regs.get(base + 1))
                    else {
                        return Err(Stop::Bad);
                    };
                    let count = cnt.wrapping_add(1);
                    let next = pc + 6;
                    if count < min {
                        self.set_reg(base, count);
                        self.pc = body as usize;
                        continue;
                    }
                    if lazy == 0 {
                        if (max == INF || count < max) && self.pos as u32 != last {
                            self.push(&[next as u32, self.pos as u32, bt::ALT]);
                            self.set_reg(base, count);
                            self.set_reg(base + 1, self.pos as u32);
                            self.pc = body as usize;
                        } else {
                            self.pc = next;
                        }
                    } else {
                        self.push(&[r as u32, count, self.pos as u32, body, max, bt::LAZY]);
                        self.pc = next;
                    }
                }
                op::ATOMIC => {
                    self.push(&[ATOMIC_KIND, self.pos as u32, 0, bt::BARRIER]);
                    self.pc = pc + 1;
                }
                op::LOOK => {
                    let (kind, wid, next) = (w(pc + 1)?, w(pc + 2)?, w(pc + 3)?);
                    let mut from = self.pos;
                    let mut ok = true;
                    if kind & 2 != 0 {
                        for _ in 0..wid {
                            *steps += 1;
                            let Some((_, p)) = s.prev(from) else {
                                ok = false;
                                break;
                            };
                            from = p;
                        }
                    }
                    if !ok {
                        if kind & 1 != 0 {
                            self.pc = next as usize;
                            continue;
                        }
                        fail!();
                    }
                    self.push(&[kind, self.pos as u32, next, bt::BARRIER]);
                    self.pos = from;
                    self.pc = pc + 4;
                }
                op::END => {
                    let (kind, pos, next) = self.peek_barrier().ok_or(Stop::Bad)?;
                    if kind == ATOMIC_KIND {
                        self.cut();
                        self.pc = pc + 1;
                    } else if kind & 1 == 0 {
                        self.cut();
                        self.pos = pos;
                        self.pc = next as usize;
                    } else {
                        // A negative lookaround's body matched: it fails.
                        self.unwind();
                        fail!();
                    }
                }
                op::GROUPREF => {
                    let (g, mode) = (w(pc + 1)? as usize, w(pc + 2)?);
                    let (Some(&a), Some(&b)) = (self.regs.get(2 * g), self.regs.get(2 * g + 1))
                    else {
                        return Err(Stop::Bad);
                    };
                    if a == NONE || b == NONE || a > b {
                        fail!();
                    }
                    let mut p = self.pos;
                    let mut q = a as usize;
                    let mut ok = true;
                    let full = Subj {
                        b: s.b,
                        text: s.text,
                        end: s.b.len(),
                    };
                    while q < b as usize {
                        *steps += 1;
                        let (Some((x, q2)), Some((y, p2))) = (full.next(q), s.next(p)) else {
                            ok = false;
                            break;
                        };
                        let same = match mode {
                            0 => x == y,
                            1 => lower_ascii(x) == lower_ascii(y),
                            _ => lower_uni(x) == lower_uni(y),
                        };
                        if !same {
                            ok = false;
                            break;
                        }
                        q = q2;
                        p = p2;
                    }
                    if !ok {
                        fail!();
                    }
                    self.pos = p;
                    self.pc = pc + 3;
                }
                op::GROUP_EXISTS => {
                    let (g, no) = (w(pc + 1)? as usize, w(pc + 2)?);
                    let set = self.regs.get(2 * g).is_some_and(|&x| x != NONE)
                        && self.regs.get(2 * g + 1).is_some_and(|&x| x != NONE);
                    self.pc = if set { pc + 3 } else { no as usize };
                }
                _ => return Err(Stop::Bad),
            }
        }
    }

    /// The innermost barrier's `(kind, pos, next)`, without changing the
    /// stack.
    #[cfg_attr(mm_shipped, optimize(size))]
    fn peek_barrier(&self) -> Option<(u32, usize, u32)> {
        let mut i = self.stack.len();
        while i > 0 {
            let tag = self.stack[i - 1];
            let base = i.checked_sub(frame_len(tag))?;
            if tag == bt::BARRIER {
                return Some((
                    self.stack[base],
                    self.stack[base + 1] as usize,
                    self.stack[base + 2],
                ));
            }
            i = base;
        }
        None
    }

    /// Whether this run, restored from saved words, is one the program
    /// with instruction starts `starts` could have left over subject `b`:
    /// pcs on instructions, positions on char boundaries of `b`, group
    /// registers holding positions, whole frames. A guest can pass `_exec`
    /// any saved state.
    #[cfg_attr(mm_shipped, optimize(size))]
    fn fits(&self, starts: &[u64], b: &[u8], text: bool) -> bool {
        let at = |p: u32| {
            let p = p as usize;
            p == b.len() || b.get(p).is_some_and(|&x| !text || x & 0xC0 != 0x80)
        };
        let pc = |p: u32| is_start(starts, p);
        let last = self.lastindex_slot();
        let reg = |slot: u32, v: u32| match (slot as usize).cmp(&last) {
            Ordering::Less => v == NONE || at(v),
            Ordering::Equal => v == NONE || (v as usize) < self.ngroups,
            Ordering::Greater => (slot as usize) < self.regs.len(),
        };
        let nrep = (self.regs.len() - last - 1) / 2;
        let spots = [self.search, self.start, self.pos, self.last_end];
        if spots.iter().any(|&p| !at(p as u32)) || self.active && !pc(self.pc as u32) {
            return false;
        }
        if !(0..).zip(&self.regs).all(|(slot, &v)| reg(slot, v)) {
            return false;
        }
        let mut i = self.stack.len();
        while let Some(&tag) = self.stack.get(i.wrapping_sub(1)) {
            let Some(base) = i.checked_sub(frame_len(tag)) else {
                return false;
            };
            let ok = match (tag, self.stack.get(base..i - 1).unwrap_or(&[])) {
                (bt::ALT, &[to, pos]) => pc(to) && at(pos),
                (bt::UNDO, &[slot, old]) => reg(slot, old),
                (bt::REPG, &[next, _, _, pos, _]) => pc(next) && at(pos),
                (bt::REPL, &[next, item, _, _, pos]) => pc(next) && pc(item) && at(pos),
                (bt::BARRIER, &[kind, pos, next]) => at(pos) && (kind == ATOMIC_KIND || pc(next)),
                (bt::LAZY, &[r, _, pos, body, _]) => (r as usize) < nrep && at(pos) && pc(body),
                _ => false,
            };
            if !ok {
                return false;
            }
            i = base;
        }
        true
    }

    /// Saves the run as words.
    #[cfg_attr(mm_shipped, optimize(size))]
    fn save(&self) -> Vec<u32> {
        let mut out = vec![
            self.search as u32,
            u32::from(self.must_advance),
            self.start as u32,
            u32::from(self.active),
            self.pc as u32,
            self.pos as u32,
            self.limit,
            self.regs.len() as u32,
            self.last_end as u32,
        ];
        out.extend_from_slice(&self.regs);
        out.extend_from_slice(&self.stack);
        out
    }
}

/// Does the single-width instruction at `pc` match `c`?
#[cfg_attr(mm_shipped, optimize(size))]
fn one(code: &[u32], pc: usize, c: u32, steps: &mut u64) -> bool {
    let w = |i: usize| code.get(i).copied().unwrap_or(NONE);
    match w(pc) {
        op::CHAR => c == w(pc + 1),
        op::NOT_CHAR => c != w(pc + 1),
        op::CHAR_I | op::NOT_CHAR_I => {
            let lit = w(pc + 1);
            let eq = if w(pc + 2) == 1 {
                lower_ascii(c) == lower_ascii(lit)
            } else {
                c == lit || lower_uni(c) == lower_uni(lit) || upper_uni(c) == upper_uni(lit)
            };
            eq == (w(pc) == op::CHAR_I)
        }
        op::ANY => c != u32::from(b'\n'),
        op::ANY_ALL => true,
        op::SET => {
            // A set is read item by item: a step per 16 words of it.
            *steps += u64::from(w(pc + 2)) / 16;
            in_set(code, pc + 3, w(pc + 1), c)
        }
        _ => false,
    }
}

/// A single-width item, classified once for a repeat's loop: the common
/// forms test a char without decoding the instruction again.
#[derive(Clone, Copy)]
enum Item {
    Char(u32),
    NotChar(u32),
    Any,
    AnyAll,
    /// A category (`\d`, `\s`, `\w`, …), negated when `true`.
    Cat(u32, bool),
    /// Anything else: [`one`].
    Other,
}

impl Item {
    #[cfg_attr(mm_shipped, optimize(size))]
    fn of(code: &[u32], pc: usize) -> Item {
        let w = |i: usize| code.get(i).copied().unwrap_or(NONE);
        match w(pc) {
            op::CHAR => Item::Char(w(pc + 1)),
            op::NOT_CHAR => Item::NotChar(w(pc + 1)),
            op::ANY => Item::Any,
            op::ANY_ALL => Item::AnyAll,
            // An exact set of one literal or one category, maybe negated.
            op::SET if w(pc + 1) == 0 => {
                let neg = w(pc + 3) == set::NEGATE;
                let i = pc + 3 + usize::from(neg);
                match (w(i), w(i + 2)) {
                    (set::LIT, set::END) if neg => Item::NotChar(w(i + 1)),
                    (set::LIT, set::END) => Item::Char(w(i + 1)),
                    (set::CAT, set::END) => Item::Cat(w(i + 1), neg),
                    _ => Item::Other,
                }
            }
            _ => Item::Other,
        }
    }

    #[inline]
    #[cfg_attr(mm_shipped, optimize(size))]
    fn test(self, code: &[u32], pc: usize, c: u32, steps: &mut u64) -> bool {
        match self {
            Item::Char(l) => c == l,
            Item::NotChar(l) => c != l,
            Item::Any => c != u32::from(b'\n'),
            Item::AnyAll => true,
            Item::Cat(k, neg) => category(k, c) != neg,
            Item::Other => one(code, pc, c, steps),
        }
    }
}

/// The single-width items one of which starts every match (their `pc`s),
/// from following `SAVE`, `JMP` and `SPLIT` from the start; `None` when a
/// path meets anything else first, or there are more than 8.
#[cfg_attr(mm_shipped, optimize(size))]
fn first_items(code: &[u32]) -> Option<Vec<usize>> {
    let w = |i: usize| code.get(i).map(|&x| x as usize);
    let (mut out, mut todo, mut visits) = (Vec::new(), vec![HDR], 0);
    while let Some(mut pc) = todo.pop() {
        loop {
            visits += 1;
            if visits > 32 || out.len() > 8 {
                return None;
            }
            match u32::try_from(w(pc)?).ok()? {
                op::SAVE => pc += 2,
                op::JMP => pc = w(pc + 1)?,
                op::SPLIT => {
                    todo.push(w(pc + 2)?);
                    pc = w(pc + 1)?;
                }
                op::REP1 if w(pc + 2)? >= 1 => {
                    width(code, pc + 5)?;
                    out.push(pc + 5);
                    break;
                }
                _ => {
                    width(code, pc)?;
                    out.push(pc);
                    break;
                }
            }
        }
    }
    Some(out)
}

/// Words of single-width instruction `pc`.
#[cfg_attr(mm_shipped, optimize(size))]
fn width(code: &[u32], pc: usize) -> Option<usize> {
    match code.get(pc)? {
        &op::CHAR | &op::NOT_CHAR => Some(2),
        &op::CHAR_I | &op::NOT_CHAR_I => Some(3),
        &op::ANY | &op::ANY_ALL => Some(1),
        &op::SET => code.get(pc + 2).map(|&n| n as usize),
        _ => None,
    }
}

/// The instruction starts of program `code` (a bit per word), or `None`
/// unless it is laid out as `re.py`'s compiler lays one out: the header
/// with group 0, each instruction known and whole, a set's items ending
/// in `END` where its length says, a repeat's item a single-width
/// instruction, registers and groups in range, and every jump on an
/// instruction. `_exec` and `_match` take whatever `bytes` a guest passes
/// them, so a program is checked before it runs; the matcher then never
/// meets a bad register or a set it does not charge for.
#[cfg_attr(mm_shipped, optimize(size))]
fn check_program(code: &[u32]) -> Option<Vec<u64>> {
    let w = |i: usize| code.get(i).copied();
    let (ngroups, nrep) = (w(1)? as usize, w(2)? as usize);
    if w(0)? != MAGIC || ngroups == 0 || w(3)? > 1 {
        return None;
    }
    // Instruction starts, and the words jumps go to.
    let mut starts = vec![0u64; code.len().div_ceil(64)];
    let mut targets = starts.clone();
    let mut jump = |t: u32| {
        let t = t as usize;
        targets.get_mut(t / 64).map(|b| *b |= 1 << (t % 64))
    };
    let mut pc = HDR;
    while pc < code.len() {
        *starts.get_mut(pc / 64)? |= 1 << (pc % 64);
        let len = match w(pc)? {
            op::FAIL | op::MATCH | op::ANY | op::ANY_ALL | op::ATOMIC | op::END => 1,
            op::CHAR | op::NOT_CHAR => 2,
            op::CHAR_I | op::NOT_CHAR_I => 3,
            op::SET => {
                let len = w(pc + 2)? as usize;
                let end = pc.checked_add(len)?.checked_sub(1)?;
                if w(pc + 1)? > 2 || len < 4 || w(end)? != set::END {
                    return None;
                }
                let mut i = pc + 3;
                while i < end {
                    i += match w(i)? {
                        set::NEGATE => 1,
                        set::LIT | set::CAT => 2,
                        set::RANGE => 3,
                        _ => return None,
                    };
                }
                if i != end {
                    return None;
                }
                len
            }
            op::AT if w(pc + 1)? <= 8 => 2,
            op::JMP => {
                jump(w(pc + 1)?)?;
                2
            }
            op::SPLIT => {
                jump(w(pc + 1)?)?;
                jump(w(pc + 2)?)?;
                3
            }
            op::SAVE if (w(pc + 1)? as usize) < 2 * ngroups => 2,
            op::REP1 => {
                // The item follows; `next` is the word after it.
                let after = width(code, pc + 5).map(|n| pc + 5 + n);
                if w(pc + 1)? > 2 || after != Some(w(pc + 4)? as usize) {
                    return None;
                }
                5
            }
            op::REP_INIT if (w(pc + 1)? as usize) < nrep => {
                jump(w(pc + 2)?)?;
                3
            }
            op::REP_UNTIL if (w(pc + 1)? as usize) < nrep => {
                jump(w(pc + 5)?)?;
                6
            }
            op::LOOK => {
                jump(w(pc + 3)?)?;
                4
            }
            op::GROUPREF if (w(pc + 1)? as usize) < ngroups && w(pc + 2)? <= 2 => 3,
            op::GROUP_EXISTS if (w(pc + 1)? as usize) < ngroups => {
                jump(w(pc + 2)?)?;
                3
            }
            _ => return None,
        };
        pc = pc.checked_add(len).filter(|&p| p <= code.len())?;
    }
    let landed = starts.iter().zip(&targets).all(|(s, t)| t & !s == 0);
    landed.then_some(starts)
}

/// Does an instruction start at `pc`?
#[cfg_attr(mm_shipped, optimize(size))]
fn is_start(starts: &[u64], pc: u32) -> bool {
    starts
        .get(pc as usize / 64)
        .is_some_and(|b| b >> (pc % 64) & 1 == 1)
}

/// Is `c` in the set whose items start at `i`?
#[cfg_attr(mm_shipped, optimize(size))]
fn in_set(code: &[u32], mut i: usize, mode: u32, c: u32) -> bool {
    let w = |i: usize| code.get(i).copied().unwrap_or(set::END);
    let mut negate = false;
    // Fold modes test the character and its other cases; Unicode also the
    // cases of those (Kelvin sign `K` -> `k` -> `K`, `ſ` -> `S` -> `s`).
    let (lo, up) = match mode {
        1 => (lower_ascii(c), upper_ascii(c)),
        2 => (lower_uni(c), upper_uni(c)),
        _ => (c, c),
    };
    let alts = if mode == 0 {
        [c; 5]
    } else {
        [c, lo, up, upper_uni(lo), lower_uni(up)]
    };
    let cands: &[u32] = match mode {
        0 => &alts[..1],
        1 => &alts[..3],
        _ => &alts,
    };
    loop {
        match w(i) {
            set::END => return negate,
            set::NEGATE => {
                negate = true;
                i += 1;
            }
            set::LIT => {
                let l = w(i + 1);
                if cands.contains(&l) || mode == 2 && lower_uni(l) == lo {
                    return !negate;
                }
                i += 2;
            }
            set::RANGE => {
                let (a, b) = (w(i + 1), w(i + 2));
                if cands.iter().any(|&x| a <= x && x <= b) {
                    return !negate;
                }
                i += 3;
            }
            set::CAT => {
                if category(w(i + 1), c) {
                    return !negate;
                }
                i += 2;
            }
            _ => return false,
        }
    }
}

/// Does assertion `kind` hold at `pos`?
#[cfg_attr(mm_shipped, optimize(size))]
fn at(kind: u32, s: &Subj<'_>, pos: usize) -> bool {
    let nl = u32::from(b'\n');
    let word = |k: u32| -> bool {
        // Since 3.14 `\B` matches the empty string; `\b` does not.
        if s.end == 0 {
            return k == 6 || k == 8;
        }
        let f = if k >= 7 { is_word_uni } else { is_word_ascii };
        let before = s.prev(pos).is_some_and(|(c, _)| f(c));
        let after = s.next(pos).is_some_and(|(c, _)| f(c));
        (before != after) == (k == 5 || k == 7)
    };
    match kind {
        0 => pos == 0,
        1 => pos == 0 || s.prev(pos).is_some_and(|(c, _)| c == nl),
        2 => pos == s.end || (s.next(pos).is_some_and(|(c, p)| c == nl && p == s.end)),
        3 => pos == s.end || s.next(pos).is_some_and(|(c, _)| c == nl),
        4 => pos == s.end,
        5..=8 => word(kind),
        _ => false,
    }
}

// ------------------------------------------------------------- natives

/// Char indices of byte offsets in a `str`, from a moving anchor.
struct Chars<'a> {
    b: &'a [u8],
    ascii: bool,
    anchor: (usize, usize),
}

impl Chars<'_> {
    /// The char index of byte offset `b`.
    #[cfg_attr(mm_shipped, optimize(size))]
    fn index(&mut self, b: usize) -> usize {
        if self.ascii {
            return b;
        }
        let (ab, ac) = self.anchor;
        let cont = |x: &u8| x & 0xC0 != 0x80;
        // Saturating: a guest can pass `_exec` an anchor whose char index
        // is not its byte offset's.
        let c = if b >= ab {
            ac.saturating_add(
                self.b
                    .get(ab..b)
                    .map_or(0, |s| s.iter().filter(|x| cont(x)).count()),
            )
        } else {
            ac.saturating_sub(
                self.b
                    .get(b..ab)
                    .map_or(0, |s| s.iter().filter(|x| cont(x)).count()),
            )
        };
        self.anchor = (b, c);
        c
    }
}

/// What one slice of a program found.
struct Slice {
    /// Each match's `lastindex` (-1 for none) and spans as char offsets
    /// (-1 when unset), for output kind 0.
    out: Vec<Vec<i64>>,
    found: Vec<Found>,
    /// Segments between matches (output kind 2), as byte offsets.
    segs: Vec<(usize, usize)>,
    /// The run's state when the slice ran out before it finished.
    saved: Option<Vec<u32>>,
    /// A byte offset and its char index, to start the next slice from.
    hint: (usize, usize),
    /// Groups, with group 0.
    ngroups: usize,
}

/// Runs program `code_v` over `subj_v` from char `span.0` to `span.1` for
/// one step slice. `mode_arg`: the mode in its low 2 bits, the output kind
/// above; `saved`, `must_advance` and `hint` continue an earlier slice.
#[allow(clippy::too_many_arguments, reason = "`_exec`'s arguments")]
#[inline(never)]
#[cfg_attr(mm_shipped, optimize(size))]
fn exec_slice(
    vm: &mut Vm,
    code_v: Value,
    subj_v: Value,
    span: (i64, i64),
    mode_arg: u32,
    limit: i64,
    saved: Value,
    must_advance: bool,
    hint: (Option<i64>, Option<i64>),
) -> Result<Slice, Raised> {
    let text = Vm::is_str(subj_v);
    if !Vm::is_bytes(code_v) {
        return Err(vm.err(T::TypeError, "re: bad _exec arguments"));
    }
    if !text && !Vm::is_bytes(subj_v) {
        return Err(vm.err(T::TypeError, "re: bad _exec subject"));
    }
    let (mode, out_kind) = (mode_arg & 3, mode_arg >> 2);
    // Decoding the program and the saved state is byte work, each call.
    let saved_len = vm.bytes_like(saved).map_or(0, <[u8]>::len);
    vm.burn_bytes(vm.bytes_data(code_v).len() + saved_len);
    let code: Vec<u32> = vm
        .bytes_data(code_v)
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| u32::from_le_bytes(*c))
        .collect();
    let starts = (code.len() > HDR).then(|| check_program(&code)).flatten();
    let Some(starts) = starts else {
        return Err(vm.err(T::SystemError, "re: corrupt program"));
    };
    let ngroups = code[1] as usize;
    let nrep = code[2] as usize;
    let nregs = 2 * ngroups + 1 + 2 * nrep;
    let saved_words: Option<Vec<u32>> = if Vm::is_bytes(saved) {
        Some(
            vm.bytes_data(saved)
                .as_chunks::<4>()
                .0
                .iter()
                .map(|c| u32::from_le_bytes(*c))
                .collect(),
        )
    } else {
        None
    };
    let blen = if text {
        vm.str_bytes(subj_v).len()
    } else {
        vm.bytes_data(subj_v).len()
    };
    if blen >= (u32::MAX - 1) as usize || nregs > 1 << 20 {
        return Err(vm.err(T::OverflowError, "re: the string is too long"));
    }
    let ascii = !text || vm.str_ascii(subj_v);
    let nchars = if text { vm.str_len(subj_v) } else { blen };
    let mut steps = 0u64;
    let clamp = |v: i64| usize::try_from(v.max(0)).unwrap_or(usize::MAX).min(nchars);
    let (pos_c, endpos_c) = (clamp(span.0), clamp(span.1));
    // Byte offsets from the nearest known position (`Vm::str_offset`), so
    // `p.match(s, pos)` along a string does not walk from its start.
    let (start, end) = if text {
        let start = vm.str_offset(subj_v, pos_c);
        (start, vm.str_offset(subj_v, endpos_c).max(start))
    } else {
        (pos_c, endpos_c.max(pos_c))
    };
    let (out, found, segs, stop, saved, hint) = {
        let subj = if text {
            vm.str_bytes(subj_v)
        } else {
            vm.bytes_data(subj_v)
        };
        let mut chars = Chars {
            b: subj,
            ascii,
            anchor: (start, pos_c),
        };
        if let (Some(hb), Some(hc)) = hint
            && let (Ok(hb), Ok(hc)) = (usize::try_from(hb), usize::try_from(hc))
            && hb <= subj.len()
            && hc <= nchars
            && (!text || hb == subj.len() || subj[hb] & 0xC0 != 0x80)
        {
            chars.anchor = (hb, hc);
        }
        let s = Subj { b: subj, text, end };
        let mut run = Run {
            mode,
            limit: u32::try_from(limit)
                .ok()
                .filter(|&l| l > 0)
                .unwrap_or(u32::MAX),
            search: start,
            must_advance,
            start,
            active: false,
            pc: HDR,
            pos: start,
            regs: vec![NONE; nregs],
            // Room for a few frames: most attempts never reallocate.
            stack: Vec::with_capacity(64),
            check_at: 0,
            ngroups,
            last_end: 0,
        };
        if let Some(sw) = &saved_words {
            if sw.len() < 9 || sw[7] as usize != nregs || sw.len() < 9 + nregs {
                return Err(vm.err(T::SystemError, "re: corrupt saved state"));
            }
            run.last_end = (sw[8] as usize).min(subj.len());
            run.search = (sw[0] as usize).min(subj.len());
            run.must_advance = sw[1] != 0;
            run.start = (sw[2] as usize).min(subj.len());
            run.active = sw[3] != 0;
            run.pc = sw[4] as usize;
            run.pos = (sw[5] as usize).min(subj.len());
            run.limit = sw[6];
            run.regs.copy_from_slice(&sw[9..9 + nregs]);
            run.stack.extend_from_slice(&sw[9 + nregs..]);
            if !run.fits(&starts, subj, text) {
                return Err(vm.err(T::SystemError, "re: corrupt saved state"));
            }
        }
        run.check_at = (run.stack.len() * 2).max(STACK_FREE);
        // The stack past `STACK_FREE` words is charged three times: itself,
        // and at a slice its saved words and their bytes.
        let Ok(mut charge) = vm.heap.reserve_scratch(0) else {
            return Err(vm.err(T::MemoryError, ""));
        };
        if run.check_at > STACK_FREE {
            run.check_at = stack_charge(&mut charge, &vm.heap, run.check_at, run.stack.len());
        }
        let mut found: Vec<Found> = Vec::new();
        // Matches wait here, outside the arena, until the call returns:
        // past `FOUND_WORDS` the call ends its slice, so they move to the
        // arena's result list before more are found.
        let mut found_words = 0usize;
        let mut segs: Vec<(usize, usize)> = Vec::new();
        // A slice grows with the subject only so far that one stays well
        // under 50 ms at the default limits (`subj.len() / 8` made 49 ms
        // slices over 60M chars [M]; tests/steps.rs). It never follows the
        // fuel left: where slices end, and so what their steps and `_more`
        // safepoints charge, must not depend on where the host's fuel
        // slices end (docs/spec.md §5, guarantee 4); a slice past the fuel
        // is overdraft, as any native step's is.
        let budget = SLICE
            .max(run.stack.len() as u64 * 4)
            .max(subj.len() as u64 / 64);
        // The items a match can start with let a search skip ahead: to a
        // literal with `find_char`, else char by char.
        let firsts: Vec<(Item, usize)> = if mode == 1 {
            first_items(&code)
                .unwrap_or_default()
                .into_iter()
                .map(|pc| (Item::of(&code, pc), pc))
                .collect()
        } else {
            Vec::new()
        };
        let first_lit = match firsts[..] {
            [(Item::Char(c), _)] => Some(c),
            _ => None,
        };
        // Steps a char costs the skip: a test per item, three for one
        // that `one` decodes (a set, a case-folded literal).
        let weight: u64 = firsts
            .iter()
            .map(|(it, _)| if matches!(it, Item::Other) { 3 } else { 1 })
            .sum();
        let stop = loop {
            if !run.active {
                if run.limit == 0 || run.search > end {
                    break Stop::Done;
                }
                let mut at = run.search;
                if run.mode == 1
                    && let Some(l) = first_lit
                {
                    // The skip is byte work: one step per 2 bytes, so
                    // one fuel unit per 64 (docs/spec.md §5).
                    let p = find_char(&s, at, l);
                    steps += (p.unwrap_or(s.end).saturating_sub(at) / 2) as u64;
                    match p {
                        Some(p) => at = p,
                        None => break Stop::Done,
                    }
                    if at != run.search {
                        run.must_advance = false;
                        run.search = at;
                    }
                } else if run.mode == 1 && !firsts.is_empty() {
                    // `weight` steps a char, up to the slice's budget.
                    while let Some((c, p)) = s.next(at) {
                        if firsts
                            .iter()
                            .any(|&(it, pc)| it.test(&code, pc, c, &mut steps))
                        {
                            break;
                        }
                        steps += weight;
                        at = p;
                        if steps >= budget {
                            break;
                        }
                    }
                    if at != run.search {
                        run.must_advance = false;
                        run.search = at;
                    }
                    // A match takes at least one char.
                    if s.next(at).is_none() {
                        break Stop::Done;
                    }
                    if steps >= budget {
                        break Stop::Sliced;
                    }
                }
                run.begin(at);
                // Clearing the group registers: a step per 16.
                steps += (run.regs.len() / 16) as u64;
            }
            match run.attempt(&code, &s, &mut steps, budget) {
                Ok(None) => break Stop::Sliced,
                Ok(Some(true)) => {
                    run.active = false;
                    let li = run.regs[run.lastindex_slot()];
                    found.push(Found {
                        lastindex: li,
                        spans: run.regs[..2 * ngroups].to_vec(),
                    });
                    found_words += 2 * ngroups + 8;
                    run.limit = run.limit.saturating_sub(1);
                    let (st0, en0) = (run.regs[0] as usize, run.regs[1] as usize);
                    if out_kind == 2 {
                        segs.push((run.last_end, st0));
                        run.last_end = en0;
                    }
                    run.must_advance = st0 == en0;
                    run.search = en0;
                    if run.mode != 1 {
                        break Stop::Done;
                    }
                    if found_words >= FOUND_WORDS {
                        break Stop::Sliced;
                    }
                }
                Ok(Some(false)) => {
                    run.active = false;
                    if run.mode != 1 {
                        break Stop::Done;
                    }
                    // The next start: one char on.
                    match s.next(run.start) {
                        Some((_, p)) => {
                            run.search = p;
                            run.must_advance = false;
                        }
                        None => break Stop::Done,
                    }
                }
                Err(Stop::Memory) => {
                    let len = run.stack.len();
                    run.check_at =
                        stack_charge(&mut charge, &vm.heap, run.check_at.saturating_mul(2), len);
                    if len >= run.check_at
                        || run.stack.try_reserve_exact(run.check_at - len).is_err()
                    {
                        break Stop::Memory;
                    }
                    continue;
                }
                Err(e) => break e,
            }
            if steps >= budget {
                break Stop::Sliced;
            }
        };
        if out_kind == 2 && matches!(stop, Stop::Done) {
            segs.push((run.last_end, subj.len()));
        }
        let saved_out = if matches!(stop, Stop::Sliced) {
            Some(run.save())
        } else {
            None
        };
        // Spans as char offsets.
        let mut out: Vec<Vec<i64>> = Vec::new();
        if out_kind == 0 || out_kind == 3 {
            for f in &found {
                let mut v = Vec::with_capacity(f.spans.len() + 1);
                v.push(if f.lastindex == NONE {
                    -1
                } else {
                    i64::from(f.lastindex)
                });
                for &b in &f.spans {
                    v.push(if b == NONE {
                        -1
                    } else {
                        i64::try_from(chars.index(b as usize)).unwrap_or(-1)
                    });
                }
                out.push(v);
            }
        }
        (out, found, segs, stop, saved_out, chars.anchor)
    };
    vm.burn(steps / STEPS_PER_FUEL);
    match stop {
        Stop::Bad => Err(vm.err(T::SystemError, "re: corrupt program")),
        Stop::Memory => Err(vm.err(T::MemoryError, "")),
        _ => Ok(Slice {
            out,
            found,
            segs,
            saved,
            hint,
            ngroups,
        }),
    }
}

/// `_exec(code, string, pos, endpos, mode, limit, st)`: runs a compiled
/// program. `st` is `[saved, results, must_advance, hint_char,
/// hint_byte]`; each match appends `(lastindex, start0, end0, start1,
/// …)` (char offsets, -1 when unset) to `results`. Returns `True` when
/// done, `False` when it stopped after its step slice (call again with the
/// same arguments).
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn re_exec(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let (code_v, st) = (vm.arg_o(&a, 0), vm.arg_o(&a, 6));
    let subj_v = vm.unbx(vm.arg_o(&a, 1));
    let pos_c = vm.int_arg(vm.arg_o(&a, 2), "")?;
    let endpos_c = vm.int_arg(vm.arg_o(&a, 3), "")?;
    let mode_arg = u32::try_from(vm.int_arg(vm.arg_o(&a, 4), "")?).unwrap_or(1);
    let out_kind = mode_arg >> 2;
    let limit = vm.int_arg(vm.arg_o(&a, 5), "")?;
    let want = if out_kind == 3 { 8 } else { 5 };
    if !st.is_obj_kind(ty::LIST as u8) || vm.list_len(st) < want {
        return Err(vm.err(T::TypeError, "re: bad _exec arguments"));
    }
    let must_advance = vm.list_get(st, 2) == Value::TRUE;
    let hint = (
        vm.int_of_o(vm.list_get(st, 4)),
        vm.int_of_o(vm.list_get(st, 3)),
    );
    let saved = vm.list_get(st, 0);
    let Slice {
        out,
        found,
        segs,
        saved: saved_out,
        hint: hint_out,
        ngroups,
    } = exec_slice(
        vm,
        code_v,
        subj_v,
        (pos_c, endpos_c),
        mode_arg,
        limit,
        saved,
        must_advance,
        hint,
    )?;
    let text = Vm::is_str(subj_v);
    let results = vm.list_get(st, 1);
    if out_kind == 3 {
        // `re.Match` objects, of the pattern and positions `st` adds.
        let p = vm.list_get(st, 5);
        let (pos, endpos) = (vm.list_get(st, 6), vm.list_get(st, 7));
        let (pos, endpos) = (vm.int_arg(pos, "")?, vm.int_arg(endpos, "")?);
        let span = clamp_span(vm, subj_v, pos, endpos);
        let string = vm.arg_o(&a, 1);
        for (f, c) in found.iter().zip(&out) {
            let m = build_match(vm, p, string, span, f, c)?;
            vm.list_push(results, m)?;
        }
    }
    for f in out.into_iter().filter(|_| out_kind == 0) {
        let mut items = Vec::with_capacity(f.len());
        for (k, x) in f.into_iter().enumerate() {
            items.push(if k == 0 && x < 0 {
                Value::NONE
            } else {
                vm.new_int(x)?
            });
        }
        let t = vm.alloc_tuple(&items)?;
        vm.list_push(results, t)?;
    }
    if out_kind == 1 {
        // findall's items: the match, its one group, or a tuple of groups.
        for f in &found {
            let piece = |vm: &mut Vm, g: usize| {
                let (a, b) = (f.spans[2 * g] as usize, f.spans[2 * g + 1] as usize);
                if f.spans[2 * g] == NONE {
                    piece(vm, subj_v, text, 0, 0)
                } else {
                    piece(vm, subj_v, text, a, b)
                }
            };
            let item = match ngroups {
                1 => piece(vm, 0)?,
                2 => piece(vm, 1)?,
                _ => {
                    let mut items = Vec::with_capacity(ngroups - 1);
                    for g in 1..ngroups {
                        items.push(piece(vm, g)?);
                    }
                    vm.alloc_tuple(&items)?
                }
            };
            vm.list_push(results, item)?;
        }
    }
    for (a, b) in segs {
        let v = piece(vm, subj_v, text, a, b)?;
        vm.list_push(results, v)?;
    }
    let saved = match &saved_out {
        Some(words) => {
            let mut bytes = Vec::with_capacity(words.len() * 4);
            for x in words {
                bytes.extend_from_slice(&x.to_le_bytes());
            }
            vm.new_bytes(&bytes)?
        }
        None => Value::NONE,
    };
    let (hb, hc) = (
        vm.new_int(hint_out.0 as i64)?,
        vm.new_int(hint_out.1 as i64)?,
    );
    if let Some(buf) = vm.list_buf(st) {
        for (i, v) in [saved, results, Value::FALSE, hc, hb]
            .into_iter()
            .enumerate()
        {
            vm.set_slot_o(buf, i as u32, v);
        }
    }
    Ok(Value::from_bool(saved_out.is_none()))
}

/// Charges a backtrack stack of up to `want` words (its 4 bytes a word, and
/// twice that for a slice's saved state), or as many as the memory limit
/// still has room for; returns the words charged, at least `len`, the
/// stack's current length (a stack that cannot grow fails its next check).
#[cfg_attr(mm_shipped, optimize(size))]
fn stack_charge(charge: &mut Scratch, heap: &Heap, want: usize, len: usize) -> usize {
    let room = usize::try_from(
        (charge.bytes() as u64).saturating_add(heap.available_bytes()) / STACK_WORD_COST,
    )
    .unwrap_or(usize::MAX);
    let words = want.min(room).max(len);
    let bytes = words.saturating_mul(STACK_WORD_COST as usize);
    if charge.resize(heap, bytes).is_err() {
        return len;
    }
    words
}

/// Bytes `a..b` of the subject, as a `str` or `bytes`.
#[cfg_attr(mm_shipped, optimize(size))]
fn piece(vm: &mut Vm, subj: Value, text: bool, a: usize, b: usize) -> Result<Value, Raised> {
    if text {
        vm.build_text(b.saturating_sub(a), |vm, buf| {
            if let Some(r) = subj.as_obj() {
                buf.push_slice(&vm.heap, r, a, b)?;
            }
            Ok(())
        })
    } else {
        let data = vm.bytes_data(subj).get(a..b).unwrap_or(&[]).to_vec();
        vm.new_bytes(&data)
    }
}

/// The first offset at or after `from` (and before `s.end`) where `c`
/// starts.
#[cfg_attr(mm_shipped, optimize(size))]
fn find_char(s: &Subj<'_>, from: usize, c: u32) -> Option<usize> {
    let hay = s.b.get(from..s.end)?;
    if !s.text || c < 0x80 {
        let b = u8::try_from(c).ok()?;
        return hay.iter().position(|&x| x == b).map(|i| from + i);
    }
    let ch = char::from_u32(c)?;
    let mut buf = [0u8; 4];
    let enc = ch.encode_utf8(&mut buf).as_bytes();
    hay.windows(enc.len())
        .position(|w| w == enc)
        .map(|i| from + i)
}

/// `_pack(words)`: a list of ints as a program (`bytes`, little-endian
/// `u32`s).
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn re_pack(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let Some(items) = vm.seq_items(vm.arg_o(&a, 0)) else {
        return Err(vm.err(T::TypeError, "re: _pack needs a list"));
    };
    let mut out = Vec::with_capacity(items.len() * 4);
    for v in items {
        let x = vm.int_of_o(v).and_then(|i| u32::try_from(i).ok());
        let Some(x) = x else {
            return Err(vm.err(T::OverflowError, "re: program word out of range"));
        };
        out.extend_from_slice(&x.to_le_bytes());
    }
    // `new_bytes` charges the copy.
    vm.new_bytes(&out)
}

/// `_fold(c, unicode)`: `(lower, upper)` of code point `c`, as the
/// matcher folds it.
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn re_fold(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let c = vm.int_arg(vm.arg_o(&a, 0), "")?;
    let uni = vm.arg_o(&a, 1) == Value::TRUE;
    let Ok(c) = u32::try_from(c) else {
        return Err(vm.err(T::ValueError, &format!("re: bad code point {c}")));
    };
    let (l, u) = if uni {
        (lower_uni(c), upper_uni(c))
    } else {
        (lower_ascii(c), upper_ascii(c))
    };
    let (l, u) = (vm.new_int(i64::from(l))?, vm.new_int(i64::from(u))?);
    vm.alloc_tuple(&[l, u])
}

// ------------------------------------------------------------- re.Match

/// `re.Match`: value slots the pattern and the subject; raw words the
/// clamped `pos` and `endpos`, `lastindex` (-1 for `None`), the group
/// count (with group 0), then each group's start and end as char offsets
/// and as byte offsets (-1 when unset).
/// Raw words are numbered after the value slots.
mod mt {
    pub const RE: u32 = 0;
    pub const STRING: u32 = 1;
    pub const POS: u32 = 2;
    pub const ENDPOS: u32 = 3;
    pub const LAST: u32 = 4;
    pub const NGROUPS: u32 = 5;
    pub const SPANS: u32 = 6;
    /// Raw words before the spans.
    pub const HEAD: usize = 4;
}

/// The subject's length in chars.
#[cfg_attr(mm_shipped, optimize(size))]
fn subj_len(vm: &Vm, subj: Value) -> usize {
    if Vm::is_str(subj) {
        vm.str_len(subj)
    } else {
        vm.bytes_data(subj).len()
    }
}

/// `pos` and `endpos` as a `Match` keeps them: clamped to the subject.
fn clamp_span(vm: &Vm, subj: Value, pos: i64, endpos: i64) -> [u64; 2] {
    let n = i64::try_from(subj_len(vm, subj)).unwrap_or(i64::MAX);
    [
        pos.clamp(0, n).cast_unsigned(),
        endpos.clamp(0, n).cast_unsigned(),
    ]
}

/// `_match(code, pattern, string, pos, endpos, mode)`: `Pattern.match`
/// (mode 0), `search` (1) and `fullmatch` (2) of the pattern compiled to
/// `code`, a `Match` or `None`. A subject that is not of the pattern's own
/// type, positions that are not ints, and a run longer than one step slice
/// continue in `re.py`'s `_match_py` (passed the slice's saved state),
/// whose `_exec` loop has a safepoint per slice.
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn re_match(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let (code, p, subj) = (vm.arg_o(&a, 0), vm.arg_o(&a, 1), vm.arg_o(&a, 2));
    let (pos, endpos) = (vm.int_of_o(vm.arg_o(&a, 3)), vm.int_of_o(vm.arg_o(&a, 4)));
    let mode = vm.arg_o(&a, 5).as_int().and_then(|m| u32::try_from(m).ok());
    // The header's pattern kind.
    let text = Vm::is_bytes(code).then(|| vm.bytes_data(code).get(12..16) == Some(&[1, 0, 0, 0]));
    let own = match text {
        Some(true) => Vm::is_str(subj),
        Some(false) => Vm::is_bytes(subj),
        None => false,
    };
    let (Some(pos), Some(endpos), Some(mode @ 0..=2), true) = (pos, endpos, mode, own) else {
        return match_py(vm, &a, Value::NONE);
    };
    let s = exec_slice(
        vm,
        code,
        subj,
        (pos, endpos),
        mode,
        1,
        Value::NONE,
        false,
        (None, None),
    )?;
    if let Some(words) = &s.saved {
        // `_exec`'s state list, to go on from here.
        let saved = words_bytes(vm, words)?;
        let results = vm.new_list(&[])?;
        let (hb, hc) = (s.hint.0 as i64, s.hint.1 as i64);
        let (hb, hc) = (vm.new_int(hb)?, vm.new_int(hc)?);
        let (pv, ev) = (vm.arg_o(&a, 3), vm.arg_o(&a, 4));
        let st = vm.new_list(&[saved, results, Value::FALSE, hc, hb, p, pv, ev])?;
        return match_py(vm, &a, st);
    }
    match (s.found.first(), s.out.first()) {
        (Some(f), Some(c)) => {
            let span = clamp_span(vm, subj, pos, endpos);
            build_match(vm, p, subj, span, f, c)
        }
        _ => Ok(Value::NONE),
    }
}

/// A `re.Match` of pattern `p` over `subj`: `span` its clamped `pos` and
/// `endpos`, `f` the match's byte spans, `c` its `lastindex` and char
/// spans (as `Slice::out`).
fn build_match(
    vm: &mut Vm,
    p: Value,
    subj: Value,
    span: [u64; 2],
    f: &Found,
    c: &[i64],
) -> Result<Value, Raised> {
    let ngroups = f.spans.len() / 2;
    let mut raw = Vec::with_capacity(mt::HEAD + 4 * ngroups);
    raw.extend_from_slice(&span);
    raw.push(c.first().copied().unwrap_or(-1).cast_unsigned());
    raw.push(ngroups as u64);
    for g in 0..ngroups {
        let byte = |k: usize| match f.spans.get(k) {
            Some(&b) if b != NONE => u64::from(b),
            _ => u64::MAX,
        };
        let ch = |k: usize| c.get(k + 1).copied().unwrap_or(-1).cast_unsigned();
        raw.extend_from_slice(&[ch(2 * g), ch(2 * g + 1), byte(2 * g), byte(2 * g + 1)]);
    }
    // Charged as a tuple of its words would be.
    vm.burn(raw.len() as u64 / 8);
    let r = {
        let res = vm.heap.alloc_init(ty::MATCH, 0, &[p, subj], &raw);
        vm.hx_o(res)
    }?;
    Ok(Value::from_ref(r, ty::MATCH as u8))
}

/// Continues `_match`'s call in `re.py`'s `_match_py`, with the state of
/// the slice it ran (`st`, or `None` to start over).
#[cfg_attr(mm_shipped, optimize(size))]
fn match_py(vm: &mut Vm, a: &CallArgs, st: Value) -> Result<Value, Raised> {
    let Some(f) = vm.std_global("re", "_match_py")? else {
        return Err(vm.err(T::SystemError, "re: _match_py is missing"));
    };
    let mut args: Vec<Value> = (0..6).map(|i| vm.arg_o(a, i)).collect();
    args.push(st);
    Ok(vm.request_call(f, args, crate::cont::Cont::None))
}

/// Words as little-endian `bytes`.
#[cfg_attr(mm_shipped, optimize(size))]
fn words_bytes(vm: &mut Vm, words: &[u32]) -> Result<Value, Raised> {
    let mut bytes = Vec::with_capacity(words.len() * 4);
    for x in words {
        bytes.extend_from_slice(&x.to_le_bytes());
    }
    vm.new_bytes(&bytes)
}

/// Group `g`'s `[char start, char end, byte start, byte end]` (-1 unset).
fn mspan(vm: &Vm, m: Value, g: usize) -> [i64; 4] {
    let Some(r) = m.as_obj() else {
        return [-1; 4];
    };
    let at = mt::SPANS as usize + 4 * g;
    core::array::from_fn(|k| {
        let w = u32::try_from(at + k).unwrap_or(u32::MAX);
        vm.raw_o(r, w).cast_signed()
    })
}

#[cfg_attr(mm_shipped, optimize(size))]
fn mword(vm: &Vm, m: Value, i: u32) -> i64 {
    m.as_obj().map_or(-1, |r| vm.raw_o(r, i).cast_signed())
}

/// The receiver, a `re.Match`.
#[inline(never)]
#[cfg_attr(mm_shipped, optimize(size))]
fn check_match(vm: &mut Vm, a: &CallArgs) -> Result<Value, Raised> {
    let m = vm.arg_o(a, 0);
    if m.is_obj_kind(ty::MATCH as u8) {
        return Ok(m);
    }
    let msg = format!(
        "descriptor of 're.Match' object doesn't apply to a '{}' object",
        vm.type_name(m)
    );
    Err(vm.err(T::TypeError, &msg))
}

/// The pattern's `groupindex` dict, if it has one.
#[cfg_attr(mm_shipped, optimize(size))]
fn groupindex(vm: &mut Vm, m: Value) -> Result<Option<Value>, Raised> {
    let p = vm.vslot_o(m, mt::RE);
    let k = vm.intern("groupindex")?;
    let gi = if p.is_obj_kind(ty::INST as u8) {
        vm.inst_get(p, k)
    } else {
        None
    };
    Ok(gi.filter(|d| d.is_obj_kind(ty::DICT as u8)))
}

/// CPython's `match_getindex`: group `g`'s number. An object that may
/// define `__index__` continues in the method's Python twin.
#[cfg_attr(mm_shipped, optimize(size))]
fn group_index(vm: &mut Vm, m: Value, g: Value) -> Result<usize, Raised> {
    let n = mword(vm, m, mt::NGROUPS);
    let i = if let Some(i) = g.as_int() {
        i
    } else if g == Value::TRUE || g == Value::FALSE {
        i64::from(g == Value::TRUE)
    } else if Vm::is_str(g) {
        match groupindex(vm, m)? {
            Some(d) => vm.dict_get_str(d, g).and_then(Value::as_int).unwrap_or(-1),
            None => -1,
        }
    } else if g.is_obj_kind(ty::INST as u8) {
        return Err(vm.need_py());
    } else {
        -1
    };
    if (0..n).contains(&i) {
        return usize::try_from(i).map_err(|_| vm.err(T::IndexError, "no such group"));
    }
    Err(vm.err(T::IndexError, "no such group"))
}

/// Group `g`'s text, or `default` when it did not take part.
#[cfg_attr(mm_shipped, optimize(size))]
fn group_text(vm: &mut Vm, m: Value, g: usize, default: Value) -> Result<Value, Raised> {
    let [_, _, bs, be] = mspan(vm, m, g);
    let (Ok(bs), Ok(be)) = (usize::try_from(bs), usize::try_from(be)) else {
        return Ok(default);
    };
    let subj = vm.unbx(vm.vslot_o(m, mt::STRING));
    piece(vm, subj, Vm::is_str(subj), bs, be)
}

/// `Match.group(*groups)`.
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn match_group(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let m = check_match(vm, &a)?;
    if a.n == 1 {
        return group_text(vm, m, 0, Value::NONE);
    }
    let mut out = Vec::with_capacity(a.n as usize - 1);
    for i in 1..a.n {
        let g = group_index(vm, m, vm.arg_o(&a, i))?;
        out.push(group_text(vm, m, g, Value::NONE)?);
    }
    if let [v] = out[..] {
        return Ok(v);
    }
    vm.alloc_tuple(&out)
}

/// `Match.groups(default=None)`.
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn match_groups(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let m = check_match(vm, &a)?;
    let default = match vm.arg_o(&a, 1) {
        Value::UNDEFINED => Value::NONE,
        d => d,
    };
    let n = usize::try_from(mword(vm, m, mt::NGROUPS)).unwrap_or(0);
    let mut out = Vec::with_capacity(n);
    for g in 1..n {
        out.push(group_text(vm, m, g, default)?);
    }
    vm.alloc_tuple(&out)
}

/// `Match.groupdict(default=None)`.
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn match_groupdict(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let m = check_match(vm, &a)?;
    let default = match vm.arg_o(&a, 1) {
        Value::UNDEFINED => Value::NONE,
        d => d,
    };
    let out = vm.new_dict()?;
    let items = match groupindex(vm, m)? {
        Some(d) => vm.dict_items_vec(d),
        None => Vec::new(),
    };
    for (name, i) in items {
        let g = group_index(vm, m, i)?;
        let v = group_text(vm, m, g, default)?;
        vm.dict_set(out, name, v)?;
    }
    Ok(out)
}

/// `start` (0), `end` (1) or `span` (2) of the group the call names (0
/// when none).
#[inline(never)]
#[cfg_attr(mm_shipped, optimize(size))]
fn match_pos(vm: &mut Vm, a: &CallArgs, which: u8) -> Result<Value, Raised> {
    let m = check_match(vm, a)?;
    let g = match vm.arg_o(a, 1) {
        Value::UNDEFINED => 0,
        g => group_index(vm, m, g)?,
    };
    let s = mspan(vm, m, g);
    match which {
        0 => vm.new_int(s[0]),
        1 => vm.new_int(s[1]),
        _ => span_tuple(vm, s),
    }
}

/// `Match.start(group=0)`.
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn match_start(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    match_pos(vm, &a, 0)
}

/// `Match.end(group=0)`.
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn match_end(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    match_pos(vm, &a, 1)
}

/// `Match.span(group=0)`.
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn match_span(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    match_pos(vm, &a, 2)
}

/// `(start, end)` as a tuple.
fn span_tuple(vm: &mut Vm, s: [i64; 4]) -> Result<Value, Raised> {
    let (a, b) = (vm.new_int(s[0])?, vm.new_int(s[1])?);
    vm.alloc_tuple(&[a, b])
}

/// `Match.__getitem__(g)`.
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn match_getitem(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let m = check_match(vm, &a)?;
    match_item(vm, m, vm.arg_o(&a, 1))
}

/// `m[g]`.
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn match_item(vm: &mut Vm, m: Value, g: Value) -> Result<Value, Raised> {
    let g = group_index(vm, m, g)?;
    group_text(vm, m, g, Value::NONE)
}

/// `Match.__copy__()` and `__deepcopy__(memo)`: the match itself, as in
/// CPython.
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn match_copy(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    check_match(vm, &a)
}

/// `Match.expand(template)`: the template compiler is Python.
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn match_expand(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    check_match(vm, &a)?;
    Err(vm.need_py())
}

/// A `re.Match`'s data attributes.
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn match_attr(vm: &mut Vm, m: Value, name: &str) -> Result<Option<Value>, Raised> {
    let word = match name {
        "re" => return Ok(Some(vm.vslot_o(m, mt::RE))),
        "string" => return Ok(Some(vm.vslot_o(m, mt::STRING))),
        "pos" => mt::POS,
        "endpos" => mt::ENDPOS,
        "lastindex" => mt::LAST,
        "lastgroup" => {
            let last = mword(vm, m, mt::LAST);
            let items = match groupindex(vm, m)? {
                Some(d) if last >= 0 => vm.dict_items_vec(d),
                _ => Vec::new(),
            };
            let name = items.into_iter().find(|(_, i)| i.as_int() == Some(last));
            return Ok(Some(name.map_or(Value::NONE, |(k, _)| k)));
        }
        "regs" => {
            let n = usize::try_from(mword(vm, m, mt::NGROUPS)).unwrap_or(0);
            let mut out = Vec::with_capacity(n);
            for g in 0..n {
                let s = mspan(vm, m, g);
                out.push(span_tuple(vm, s)?);
            }
            return vm.alloc_tuple(&out).map(Some);
        }
        _ => return Ok(None),
    };
    // `pos` and `endpos` are never negative; `lastindex` is -1 for `None`.
    match mword(vm, m, word) {
        -1 => Ok(Some(Value::NONE)),
        i => vm.new_int(i).map(Some),
    }
}

/// `repr(m)`'s span and group 0's text.
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn match_repr_parts(vm: &mut Vm, m: Value) -> Result<(i64, i64, Value), Raised> {
    let s = mspan(vm, m, 0);
    let g = group_text(vm, m, 0, Value::NONE)?;
    Ok((s[0], s[1], g))
}
