//! A static check of a code object's live table (docs/spec.md §8.3).
//!
//! A collection clears the registers the table calls dead
//! ([`Code::dead_at`]), so the table is right only if no cleared value is
//! ever read. [`Code::check_live`] checks that on the bytecode, for every
//! instruction and every way on from it, whether a test ever runs that
//! path or not.
//!
//! A session calls it only in a build with debug assertions, once for each
//! code whose registers it clears, to clear a refused table's registers to
//! `UNDEFINED` rather than the poison its reads assert they never see
//! (mm-engine `interp.rs`, `dead_fill`); the compiler's tests and fuzz
//! target call it too. Otherwise a restored image's table is not checked (a
//! wrong entry there makes registers read back as `UNDEFINED`, which every
//! instruction handles).

use crate::bytecode::{Code, FORMATS, instr_len, op, opcode, operand, unfused};
use core::fmt;

/// The rule of [`Code::check_live`] a table breaks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rule {
    /// Rule 1: an instruction reads a register dead at it.
    ReadsDead,
    /// Rule 2: a register is live after a step that did not store to it
    /// and began with it dead.
    LiveUnstored,
    /// Rule 3: a frame waiting in an instruction can be cleared of a
    /// register that is live where it goes on.
    ClearedWaiting,
    /// Rule 4: a register of a call's window, which the callee's frame
    /// overlaid, is live after the call.
    WindowAfterCall,
}

/// Where a live table is wrong.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LiveError {
    /// The rule broken.
    pub rule: Rule,
    /// The instruction.
    pub pc: u32,
    /// Where it goes on (`pc` itself for [`Rule::ReadsDead`]).
    pub to: u32,
    /// The register.
    pub reg: u8,
}

impl fmt::Display for LiveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let LiveError { rule, pc, to, reg } = *self;
        match rule {
            Rule::ReadsDead => write!(
                f,
                "rule 1: the instruction at pc {pc} reads register {reg}, dead there"
            ),
            Rule::LiveUnstored => write!(
                f,
                "rule 2: register {reg} is dead at pc {pc} and live at pc {to}, where the \
                 instruction goes on without storing to it"
            ),
            Rule::ClearedWaiting => write!(
                f,
                "rule 3: register {reg} is live at pc {to}, where the instruction at pc {pc} can \
                 go on after a wait in which a collection cleared it"
            ),
            Rule::WindowAfterCall => write!(
                f,
                "rule 4: register {reg} is live at pc {to}, after the call at pc {pc} whose \
                 callee's frame overlaid it"
            ),
        }
    }
}

/// A set of registers, as [`Code::dead_at`]'s.
type Regs = [u64; 4];

/// The lowest register in all of `a` and none of `b`.
fn first(a: &[Regs], b: &[Regs]) -> Option<u8> {
    (0..=u8::MAX).find(|&r| {
        let has = |s: &Regs| s[usize::from(r) / 64] >> (r % 64) & 1 != 0;
        a.iter().all(has) && !b.iter().any(has)
    })
}

/// What an instruction does with registers.
#[derive(Default)]
struct Uses {
    /// The ones it may read.
    read: Regs,
    /// The ones it has stored to whenever it goes on to the next
    /// instruction.
    stored: Regs,
    /// The ones it has stored to whenever it jumps.
    jump_stored: Regs,
    /// One past the highest it names: those above, and a register it keeps
    /// for a callee's frame header or a later instruction. A run of no
    /// registers counts up to where it would start.
    end: usize,
}

impl Uses {
    fn span(set: &mut Regs, end: &mut usize, r: usize, n: usize) {
        *end = (*end).max(r + n);
        for r in (r..r + n).take_while(|&r| r < 256) {
            set[r / 64] |= 1 << (r % 64);
        }
    }

    fn read(&mut self, r: usize, n: usize) {
        Uses::span(&mut self.read, &mut self.end, r, n);
    }

    fn store(&mut self, r: usize, n: usize) {
        Uses::span(&mut self.stored, &mut self.end, r, n);
    }

    fn keep(&mut self, r: usize, n: usize) {
        Uses::span(&mut [0; 4], &mut self.end, r, n);
    }
}

/// The registers the instruction at `pc` reads and stores to. A fused
/// instruction is its first: its partner follows as an instruction of its
/// own. Reads may be more than an execution makes and stores are only
/// those every execution makes, the side on which the check holds.
/// `Code::verify` bounds exactly the registers named here (`tests`).
fn uses(bc: &[u32], pc: usize) -> Uses {
    let mut u = Uses::default();
    let a = |k: usize| operand(bc, pc, k);
    match unfused(opcode(bc, pc)) {
        op::LDAR
        | op::LDA_LOCAL
        | op::LDA_DEREF
        | op::STA_DEREF
        | op::DEL_DEREF
        | op::ADD
        | op::SUB
        | op::MUL
        | op::TEST_EQ
        | op::TEST_NE
        | op::TEST_LT
        | op::TEST_LE
        | op::TEST_GT
        | op::TEST_GE
        | op::TEST_IN
        | op::TEST_NOT_IN
        | op::TEST_IS
        | op::TEST_IS_NOT
        | op::LIST_APPEND
        | op::SET_ADD
        | op::SET_UPDATE
        | op::FORMAT_SPEC
        | op::GET_ITEM
        | op::DEL_ITEM
        | op::SET_ATTR
        | op::POP_EXC
        | op::CHECK_EXC
        | op::EXC_STAR
        | op::WITH_EXIT
        | op::ANEXT
        | op::YIELD_FROM
        | op::FOR_ITER
        | op::SEND
        | op::IMPORT_FROM => u.read(a(0), 1),
        op::MAKE_CELL | op::DEL_LOCAL => {
            u.read(a(0), 1);
            u.store(a(0), 1);
        }
        op::STAR | op::PUSH_EXC | op::WITH_ENTER => u.store(a(0), 1),
        op::MOV => {
            u.read(a(0), 1);
            u.store(a(1), 1);
        }
        op::DICT_INSERT | op::SET_ITEM => {
            u.read(a(0), 1);
            u.read(a(1), 1);
        }
        op::BINOP | op::INPLACE => u.read(a(1), 1),
        op::RAISE if a(0) >= 2 => u.read(a(1), 1),
        op::LIST_EXTEND => {
            u.read(a(0), 1);
            // A call's `*args`: the callee, for the error's text.
            if a(1) == 1 && a(0) >= 1 {
                u.read(a(0) - 1, 1);
            }
        }
        op::DICT_UPDATE => {
            u.read(a(0), 1);
            match a(1) {
                // A call's `**`: the callee, for the error's text.
                1 if a(0) >= 2 => u.read(a(0) - 2, 1),
                2 => u.read(a(2), 1),
                _ => {}
            }
            // The other modes fetch the key register and drop it.
            u.keep(a(2), 1);
        }
        op::LDA_NAME | op::STA_NAME | op::DEL_NAME => u.read(0, 1),
        op::FOR_RANGE => {
            u.read(a(0), 3);
            u.store(a(0), 1);
        }
        op::FOR_PREP => {
            u.read(a(0), 1 + a(1));
            Uses::span(&mut u.jump_stored, &mut u.end, a(0), 3);
            // Room for the generic call's arguments, one register up.
            u.keep(a(0), 4);
        }
        op::BUILD_LIST | op::BUILD_TUPLE | op::BUILD_SET | op::BUILD_STRING => u.read(a(0), a(1)),
        op::BUILD_DICT => u.read(a(0), 2 * a(1)),
        op::BUILD_SLICE | op::CALL_EX => u.read(a(0), 3),
        op::BUILD_CLASS => u.read(a(0), 4),
        op::UNPACK_SEQ => u.store(a(0), a(1)),
        op::UNPACK_EX => u.store(a(0), a(1) + a(2) + 1),
        op::LOAD_METHOD => {
            u.store(a(0), 1);
            u.keep(a(0) + 1, 1);
            u.store(a(0) + 2, 1);
        }
        op::CALL | op::CALL_KW => {
            u.read(a(0), 1);
            u.keep(a(0) + 1, 1);
            u.read(a(0) + 2, a(1));
        }
        op::CALL_METHOD | op::CALL_METHOD_KW => {
            u.read(a(0), 1);
            u.keep(a(0) + 1, 1);
            u.read(a(0) + 2, 1 + a(1));
        }
        op::MAKE_FUNCTION => u.read(a(2), (a(1) & 7).count_ones() as usize),
        op::MATCH => match a(0) {
            3 | 4 => u.read(a(1), 1),
            5 => u.read(a(1), 2),
            _ => {}
        },
        _ => {}
    }
    u
}

/// Whether (unfused) opcode `o` is a call whose callee's frame starts in
/// its window (its header in `R[r]` and `R[r+1]`, its registers from
/// `R[r+2]`). A function, not a `const`: a `const` item would move the
/// snapshot build id, which this module must not (nothing shipped uses it).
fn calls(o: u8) -> bool {
    matches!(
        o,
        op::CALL | op::CALL_METHOD | op::CALL_KW | op::CALL_METHOD_KW
    )
}

/// Whether an instruction with (unfused) opcode `o` can go on to the one
/// after it. `Code::verify` lets exactly the others end a code (`tests`).
fn falls(o: u8) -> bool {
    !matches!(
        o,
        op::RETURN | op::JUMP | op::JUMP_LOOP | op::RAISE | op::RERAISE | op::ASSERT_FAIL
    )
}

/// Whether an instruction with (unfused) opcode `o` can raise: all but
/// the ones that only move a value or jump ahead. Decided from the
/// dispatch loop (`mm-engine/src/interp.rs`, the `NOP`, `LDAR`, `STAR`,
/// `MOV` and `JUMP` arms): each is a register or pc move with no call, no
/// allocation (so no `MemoryError`) and no safepoint check (`JUMP` is not
/// a safepoint, `JUMP_LOOP` and `CHECK` are), so no park, deadline, fuel
/// or collection stops at one, and a host error is raised only at a host
/// call. Their register indices are within `nregs` by `Code::verify`, so
/// nothing panics either. So they cannot reach a handler, and the cleanup
/// range `async with` opens at the `STAR` of its spill needs no change.
fn raises(o: u8) -> bool {
    !matches!(o, op::NOP | op::LDAR | op::STAR | op::MOV | op::JUMP)
}

impl Code {
    /// Checks the live table against the bytecode and returns the number
    /// of instructions checked (0 for code without a table, which keeps
    /// every register), or the first place the table is wrong.
    ///
    /// Write `L(pc)` for the registers below `nregs` that
    /// [`Code::dead_at`] does not call dead at `pc`. Every instruction `I`
    /// at `pc`, reachable or not, is taken to be able to run Python code
    /// and to wait, and, unless it only moves a value or jumps ahead
    /// (`NOP`, `LDAR`, `STAR`, `MOV`, `JUMP`), to raise. Its ways on are
    /// the next instruction `next` (unless it always returns, jumps or
    /// raises), its jump target and, if it can raise, the handler
    /// covering `pc` ([`Code::handler`], the unwinder's lookup), where
    /// the exception arrives in the accumulator and no register is
    /// stored. The rules:
    ///
    /// 1. `I` reads only registers in `L(pc)`.
    /// 2. For each way on `to`, a register in `L(to)` is in `L(pc)` or
    ///    `I` has stored to it whenever it goes that way (on a jump only
    ///    `FOR_PREP` stores, its counters; to a handler nothing does).
    /// 3. A frame waiting in `I` is not cleared of a register live where
    ///    it goes on. A caller's record holds `pc` and `next` (or `pc`
    ///    twice, to run `I` again) and a collection clears the frame of
    ///    the registers dead at both, so for a jump target `to`,
    ///    `L(to)` is within `L(pc)` and `L(next)` together (but for
    ///    `FOR_PREP`, which runs no Python code and reaches its
    ///    safepoint with the jump taken). A frame parked at a host call
    ///    is saved at `next` and cleared of the registers dead there,
    ///    and an error from the host is raised at `pc`, so for its
    ///    handler `to`, `L(to)` is within `L(next)`.
    /// 4. After a call (`CALL`, `CALL_METHOD` and their `_KW` forms), no
    ///    register of its window, from `R[r]` up, is live at `next`: the
    ///    callee's frame overlaid them, so what they hold is the callee's
    ///    until the caller stores to them again. This is what keeps a
    ///    window the compiler reuses (decorators, a `for` over a `range`
    ///    that is not the builtin's) from keeping a callee's leftover.
    ///
    /// With 1 and 2, a register a collection clears at `pc` is stored to
    /// on every path before an instruction reads it, so clearing the
    /// running frame at its pc changes nothing a program sees; 3 gives a
    /// waiting frame the same, and 4 a returned call's window.
    ///
    /// What this does not prove:
    /// - that the engine's instructions read and store what `uses` says
    ///   (its register bounds are tested against [`Code::verify`]'s, not
    ///   which of them are reads);
    /// - that an instruction waiting or parked part-way keeps what it has
    ///   done so far out of registers dead at `pc` and at `next`;
    /// - anything about code without a table (the prelude and bundled
    ///   modules as built), or about a coroutine's frame saved by `send`,
    ///   which is cleared by the same table at its suspended pc and the
    ///   one after (`Vm::await_yield`).
    ///
    /// The unpacks and `LOAD_METHOD` clear their targets themselves before
    /// they hand off to Python code (an iterator's `__next__`, a
    /// descriptor's `__get__`), and store them when it returns, before
    /// they go on: `uses` lists those stores as made whenever they go on,
    /// which holds either way.
    pub fn check_live(&self) -> Result<usize, LiveError> {
        if self.live.is_empty() {
            return Ok(0);
        }
        let bc = &self.bytecode;
        let live = |pc: usize| {
            let dead = self.dead_at(u32::try_from(pc).unwrap_or(u32::MAX));
            let mut live = [0; 4];
            for r in 0..(self.nregs as usize).min(256) {
                live[r / 64] |= (!dead[r / 64] >> (r % 64) & 1) << (r % 64);
            }
            live
        };
        let (mut pc, mut checked) = (0, 0);
        while pc < bc.len() {
            let o = unfused(opcode(bc, pc));
            let u = uses(bc, pc);
            let here = live(pc);
            let fail = |rule: Rule, to: usize, reg: u8| LiveError {
                rule,
                pc: u32::try_from(pc).unwrap_or(u32::MAX),
                to: u32::try_from(to).unwrap_or(u32::MAX),
                reg,
            };
            if let Some(r) = first(&[u.read], &[here]) {
                return Err(fail(Rule::ReadsDead, pc, r));
            }
            let within = |to: &usize| *to < bc.len();
            let next = falls(o).then_some(pc + instr_len(o)).filter(within);
            let jump = FORMATS[o as usize]
                .bytes()
                .position(|c| c == b'j')
                .map(|k| operand(bc, pc, k))
                .filter(within);
            let handler = self
                .handler(u32::try_from(pc).unwrap_or(u32::MAX))
                .filter(|_| raises(o))
                .map(|e| e.handler as usize)
                .filter(within);
            for (to, stored) in [(next, u.stored), (jump, u.jump_stored), (handler, [0; 4])] {
                if let Some(to) = to
                    && let Some(r) = first(&[live(to)], &[here, stored])
                {
                    return Err(fail(Rule::LiveUnstored, to, r));
                }
            }
            // A call's callee takes its frame from the window up: what
            // those registers hold after it returns is the callee's.
            if let (Some(next), true) = (next, calls(o)) {
                let mut window = [0; 4];
                Uses::span(&mut window, &mut 0, operand(bc, pc, 0), 256);
                if let Some(r) = first(&[live(next), window], &[]) {
                    return Err(fail(Rule::WindowAfterCall, next, r));
                }
            }
            if let Some(next) = next {
                let after = live(next);
                if let Some(to) = jump.filter(|_| o != op::FOR_PREP)
                    && let Some(r) = first(&[live(to)], &[here, after])
                {
                    return Err(fail(Rule::ClearedWaiting, to, r));
                }
                if let Some(to) = handler
                    && let Some(r) = first(&[live(to)], &[after])
                {
                    return Err(fail(Rule::ClearedWaiting, to, r));
                }
            }
            checked += 1;
            pc += instr_len(o);
        }
        Ok(checked)
    }
}

#[cfg(test)]
mod tests {
    use super::{falls, uses};
    use crate::bytecode::{
        Code, FORMATS, FUSED, Konst, NAMES, instr_len, layout_of, op, operand_plan,
    };
    use alloc::{vec, vec::Vec};

    /// One instruction of opcode `o` with operands `vals` (a jump goes to
    /// `target`), after a `NOP` and, if `then_return`, before a `RETURN`,
    /// in a code with four of everything an operand can index.
    fn code(o: u8, vals: &[usize], target: usize, then_return: bool) -> Code {
        let mut words = vec![u32::from(o); instr_len(o)];
        words[1..].fill(0);
        let l = layout_of(o);
        for (k, f) in l.fields.iter().take(l.n as usize).enumerate() {
            let v = if operand_plan(o) >> (9 * k) & 7 == 5 {
                target
            } else {
                vals[k]
            };
            words[f.word as usize] |= u32::try_from(v).unwrap_or(0) << f.shift;
        }
        let mut bytecode = vec![u32::from(op::NOP)];
        bytecode.extend(words);
        if then_return {
            bytecode.push(u32::from(op::RETURN));
        }
        Code {
            bytecode,
            consts: vec![Konst::Names(Vec::new()); 4],
            names: vec!["x".into(); 4],
            n_ics: 4,
            is_module: true,
            ..Code::default()
        }
    }

    /// The fewest registers a code needs for [`Code::verify`] to accept
    /// `c`, if any number does.
    fn fewest(mut c: Code) -> Option<usize> {
        (0..64u32)
            .find(|&n| {
                c.nregs = n;
                c.verify()
            })
            .map(|n| n as usize)
    }

    /// Operand values to try, by kind: registers, bytes (modes and
    /// counts), 16-bit indices and counts.
    fn samples(o: u8) -> Vec<Vec<usize>> {
        let mut all = vec![Vec::new()];
        for k in 0..FORMATS[o as usize].len() {
            let vals: &[usize] = match operand_plan(o) >> (9 * k) & 7 {
                1 => &[0, 1, 2, 6],
                2 => &[0, 1, 2, 3, 4, 5, 7],
                3 => &[0, 1, 3],
                _ => &[0],
            };
            all = all
                .iter()
                .flat_map(|done| vals.iter().map(move |&v| [&done[..], &[v]].concat()))
                .collect();
        }
        all
    }

    fn opcodes() -> impl Iterator<Item = u8> {
        (0..=u8::MAX).filter(|&o| !NAMES[o as usize].is_empty() && FUSED.iter().all(|f| f.2 != o))
    }

    #[test]
    fn the_registers_an_instruction_names_are_the_ones_verify_bounds() {
        let mut compared = 0;
        for o in opcodes() {
            let before = compared;
            for vals in samples(o) {
                let jump = 1 + instr_len(o);
                let (c, back) = (code(o, &vals, jump, true), code(o, &vals, 0, true));
                // `JUMP_LOOP` jumps back, the others forward.
                let Some(n) = fewest(c.clone()).or_else(|| fewest(back)) else {
                    continue;
                };
                assert_eq!(
                    uses(&c.bytecode, 1).end,
                    n,
                    "{} {vals:?}",
                    NAMES[o as usize]
                );
                compared += 1;
            }
            assert!(compared > before, "no {} verifies", NAMES[o as usize]);
        }
        assert!(compared > 1000, "{compared}");
    }

    #[test]
    fn the_instructions_that_end_a_code_are_the_ones_that_do_not_fall_through() {
        for o in opcodes() {
            // A forward jump cannot be the last instruction, whatever it
            // does: `JUMP` never falls through.
            if o != op::JUMP_LOOP && FORMATS[o as usize].contains('j') {
                assert_eq!(falls(o), o != op::JUMP, "{}", NAMES[o as usize]);
                continue;
            }
            let ends = samples(o)
                .iter()
                .any(|vals| fewest(code(o, vals, 0, false)).is_some());
            assert_eq!(falls(o), !ends, "{}", NAMES[o as usize]);
        }
    }
}
