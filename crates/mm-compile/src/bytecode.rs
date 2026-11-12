//! Accumulator-register bytecode (docs/spec.md §8.1) and code objects.
//!
//! One implicit accumulator plus frame registers. Binary operators are
//! `acc = R[r] op acc`. Instructions are one or two 32-bit words: an
//! opcode in bits 0..8 of the first, then operands in format order ([`layout`]):
//! `r` a register (8 bits), `b` a byte, `u` a 16-bit index, `i` a 16-bit
//! signed immediate and `j` a 24-bit absolute jump target (a word pc). Bits
//! that hold no operand are zero ([`Code::verify`] checks it).
//!
//! The compiler emits words directly. [`assemble`] turns a byte stream
//! (opcode, then `r`/`b` 1, `u`/`i` 2 and `j` 4 little-endian bytes; `j` a
//! byte pc) into words, for code built by hand.

use alloc::{string::String, sync::Arc, vec::Vec};

macro_rules! opcodes {
    ($($name:ident = $n:literal, $fmt:literal, $doc:literal;)*) => {
        /// Opcode values.
        pub mod op {
            $(#[doc = $doc] pub const $name: u8 = $n;)*
        }
        /// Operand formats by opcode (empty for unused opcodes).
        pub static FORMATS: [&str; 256] = {
            let mut t = [""; 256];
            $(t[$n] = $fmt;)*
            t
        };
        /// Operand layouts by opcode, packed (see [`layout_of`]).
        static PACKED: [u32; 256] = {
            let mut t = [0u32; 256];
            $(t[$n] = pack(layout($fmt));)*
            t
        };
        /// By opcode, the bytes its words carry: the first word's (the
        /// opcode and operands) in bits 0..4, the second's in bits 4..8.
        static SPLIT: [u8; 256] = {
            let mut t = [1u8; 256];
            $(t[$n] = split(layout($fmt));)*
            t
        };
        /// Operand kinds and places by opcode (see [`plan`]).
        static PLANS: [u32; 256] = {
            let mut t = [0u32; 256];
            $(t[$n] = plan($fmt);)*
            t
        };
        /// One bit per opcode: set if its instructions are two words long.
        static TWO_WORDS: [u32; 8] = {
            let mut t = [0u32; 8];
            $(if layout($fmt).words == 2 { t[$n / 32] |= 1 << ($n % 32); })*
            t
        };
        /// Opcode names, for disassembly.
        pub static NAMES: [&str; 256] = {
            let mut t = [""; 256];
            $(t[$n] = stringify!($name);)*
            t
        };
        /// A hash of this table (each opcode's number, name and operand
        /// format) for the snapshot build id (`micro_monty::build_id`,
        /// docs/spec.md §9.3): bytecode in an image means what this table
        /// says. An opcode whose operands keep their format but change
        /// meaning needs `SNAPSHOT_EPOCH` bumped instead.
        pub const TABLE_HASH: u64 = {
            let mut h = 0xcbf2_9ce4_8422_2325;
            $(
                h = fnv(h, &[$n]);
                h = fnv(h, stringify!($name).as_bytes());
                h = fnv(h, &[0]);
                h = fnv(h, $fmt.as_bytes());
                h = fnv(h, &[0]);
            )*
            h
        };
    };
}

/// FNV-1a over `bytes`, continuing from `h` ([`TABLE_HASH`]).
#[allow(clippy::cast_lossless, reason = "u64::from is not const")]
const fn fnv(mut h: u64, bytes: &[u8]) -> u64 {
    let mut i = 0;
    while i < bytes.len() {
        h = (h ^ bytes[i] as u64).wrapping_mul(0x0100_0000_01b3);
        i += 1;
    }
    h
}

opcodes! {
    NOP = 0, "", "Does nothing.";
    LDAR = 1, "r", "`acc = R[r]`.";
    STAR = 2, "r", "`R[r] = acc`.";
    MOV = 3, "rr", "`R[b] = R[a]`.";
    LDA_LOCAL = 4, "r", "`acc = R[r]`, raising `UnboundLocalError` if unbound.";
    LDA_SMI = 5, "i", "`acc = imm`.";
    LDA_CONST = 6, "u", "`acc = consts[k]`.";
    LDA_NONE = 7, "", "`acc = None`.";
    LDA_TRUE = 8, "", "`acc = True`.";
    LDA_FALSE = 9, "", "`acc = False`.";
    LDA_GLOBAL = 10, "uu", "`acc = globals[names[n]]` or the builtin; IC.";
    STA_GLOBAL = 11, "uu", "`globals[names[n]] = acc`; IC.";
    DEL_GLOBAL = 12, "u", "`del globals[names[n]]`.";
    LDA_DEREF = 13, "r", "`acc = R[r].cell_contents`.";
    STA_DEREF = 14, "r", "`R[r].cell_contents = acc`.";
    DEL_DEREF = 15, "r", "Empties the cell in `R[r]`.";
    MAKE_CELL = 16, "r", "`R[r] = cell(R[r])`.";
    DEL_LOCAL = 17, "r", "Unbinds local `R[r]` (error if unbound).";
    ADD = 18, "r", "`acc = R[r] + acc`.";
    SUB = 19, "r", "`acc = R[r] - acc`.";
    MUL = 20, "r", "`acc = R[r] * acc`.";
    BINOP = 21, "br", "`acc = R[r] <op> acc` (operator `b`).";
    INPLACE = 22, "br", "`acc = R[r] <op>= acc` (operator `b`).";
    NEG = 23, "", "`acc = -acc`.";
    POS = 24, "", "`acc = +acc`.";
    NOT = 25, "", "`acc = not acc`.";
    INVERT = 26, "", "`acc = ~acc`.";
    TEST_EQ = 27, "r", "`acc = R[r] == acc`.";
    TEST_NE = 28, "r", "`acc = R[r] != acc`.";
    TEST_LT = 29, "r", "`acc = R[r] < acc`.";
    TEST_LE = 30, "r", "`acc = R[r] <= acc`.";
    TEST_GT = 31, "r", "`acc = R[r] > acc`.";
    TEST_GE = 32, "r", "`acc = R[r] >= acc`.";
    TEST_IN = 33, "r", "`acc = R[r] in acc`.";
    TEST_NOT_IN = 34, "r", "`acc = R[r] not in acc`.";
    TEST_IS = 35, "r", "`acc = R[r] is acc`.";
    TEST_IS_NOT = 36, "r", "`acc = R[r] is not acc`.";
    JUMP = 37, "j", "Forward jump.";
    JUMP_LOOP = 38, "j", "Backward jump; a safepoint.";
    JUMP_IF_TRUE = 39, "j", "Jump if `acc` is truthy (acc unchanged).";
    JUMP_IF_FALSE = 40, "j", "Jump if `acc` is falsy (acc unchanged).";
    GET_ITER = 41, "", "`acc = iter(acc)`.";
    FOR_ITER = 42, "rj", "`acc = next(R[r])`, or jump when exhausted.";
    BUILD_LIST = 43, "ru", "`acc = [R[r], ..., R[r+n-1]]`.";
    BUILD_TUPLE = 44, "ru", "`acc = (R[r], ..., R[r+n-1])`.";
    BUILD_SET = 45, "ru", "`acc = {R[r], ..., R[r+n-1]}`.";
    BUILD_DICT = 46, "ru", "`acc = {R[r]: R[r+1], ...}` (n pairs).";
    BUILD_SLICE = 47, "r", "`acc = slice(R[r], R[r+1], R[r+2])`.";
    BUILD_STRING = 48, "ru", "`acc = ''.join(R[r..r+n])` (all str).";
    LIST_APPEND = 49, "r", "`R[r].append(acc)`.";
    LIST_EXTEND = 50, "rb", "`R[r].extend(acc)`; b = 1 for a call's `*args` (callee in `R[r-1]`).";
    SET_ADD = 51, "r", "`R[r].add(acc)`.";
    SET_UPDATE = 52, "r", "`R[r].update(acc)`.";
    DICT_INSERT = 53, "rr", "`R[a][R[b]] = acc`.";
    DICT_UPDATE = 54, "rbr", "Mode b: 0 `R[a].update(acc)`; 1 the same rejecting duplicate keys (call `**`); 2 `R[a][R[c]] = acc` rejecting a duplicate.";
    LIST_TO_TUPLE = 55, "", "`acc = tuple(acc)` for a list.";
    UNPACK_SEQ = 56, "ru", "Unpacks `acc` into exactly `n` registers from `r`.";
    UNPACK_EX = 57, "rbb", "Unpacks `acc` into `before`, a starred list and `after`.";
    FORMAT = 58, "b", "`acc = format(conv(acc))` (f-string field).";
    FORMAT_SPEC = 59, "rb", "`acc = format(conv(R[r]), acc)`.";
    GET_ATTR = 60, "uu", "`acc = acc.name`; IC.";
    SET_ATTR = 61, "ruu", "`R[r].name = acc`; IC.";
    DEL_ATTR = 62, "u", "`del acc.name`.";
    LOAD_METHOD = 63, "ruu", "`R[r], R[r+2] = method, self` (self UNDEFINED for a plain function) for `acc.name(...)`; `R[r+1]` is reserved; IC.";
    GET_ITEM = 64, "r", "`acc = R[r][acc]`.";
    SET_ITEM = 65, "rr", "`R[a][R[b]] = acc`.";
    DEL_ITEM = 66, "r", "`del R[r][acc]`.";
    CALL = 67, "rbu", "`acc = R[r](R[r+2], ..., R[r+1+n])`; `R[r+1]` is reserved (the compiler never reads or writes it) and every register from `R[r+2+n]` up is dead, so the callee's frame may start in place; IC.";
    CALL_METHOD = 68, "rbu", "Call after `LOAD_METHOD`: `R[r]` with self `R[r+2]`, args `R[r+3..r+3+n]`; `R[r+1]` reserved and registers above dead as for `CALL`; IC.";
    CALL_KW = 69, "rbu", "`CALL` whose last arguments are keywords named by const `k`.";
    CALL_METHOD_KW = 70, "rbu", "`CALL_METHOD` with keywords named by const `k`.";
    CALL_EX = 71, "r", "`acc = R[r](*R[r+1], **R[r+2])`: `R[r+1]` a tuple or list, `R[r+2]` a dict or None.";
    MAKE_FUNCTION = 72, "ubr", "`acc = function(code u)`; flags b: 1 defaults, 2 kwdefaults, 4 closure, from `R[r]`.";
    RETURN = 73, "", "Returns `acc`.";
    RAISE = 74, "br", "b = 0: re-raise the handled exception; 1: raise acc; 2: raise `R[r]` from acc.";
    RERAISE = 75, "", "Re-raises `acc` unchanged (no traceback entry, no context).";
    PUSH_EXC = 76, "r", "`R[r] = handled; handled = acc` on entry to a handler.";
    POP_EXC = 77, "r", "`handled = R[r]` on leaving a handler.";
    CHECK_EXC = 78, "r", "`acc = R[r] matches the exception type or tuple in acc`.";
    ASSERT_FAIL = 79, "b", "Raises `AssertionError` (with message `acc` if b = 1).";
    AWAIT = 80, "", "`acc = await acc`: runs a coroutine inline or suspends the task until the awaitable settles.";
    IMPORT_NAME = 81, "ub", "`acc = import names[u]`; b = 1 yields the top-level package of a dotted name.";
    IMPORT_FROM = 82, "ru", "`acc = R[r].names[u]` for `from module import name` (`*`: copies the public names into the globals).";
    LDA_NAME = 83, "u", "Class body: `acc = ns[names[n]]`, else the global or builtin (ns in `R[0]`).";
    STA_NAME = 84, "u", "Class body: `ns[names[n]] = acc`.";
    DEL_NAME = 85, "u", "Class body: `del ns[names[n]]`.";
    BUILD_CLASS = 86, "r", "`acc = class(name R[r], bases R[r+1], ns R[r+2], keywords R[r+3])`; acc in: the `__class__` cell or None.";
    YIELD = 87, "", "Yields `acc`; resumes with the sent value in `acc`.";
    SEND = 88, "rj", "`acc = R[r].send(acc)` (`next` for None); when it stops, `acc` = its value and jump.";
    YIELD_FROM = 89, "r", "`YIELD` while delegating to `R[r]` (`throw` and `close` go to it).";
    GET_YIELD_FROM_ITER = 90, "", "`acc = iter(acc)` for `yield from` (generators stay as they are).";
    WITH_ENTER = 91, "rb", "`R[r]` = bound `__exit__` of `acc`; `acc = acc.__enter__()`. b = 1 (`async with`): `__aexit__` and the awaitable `acc.__aenter__()`.";
    WITH_EXIT = 92, "rb", "b = 0: `R[r](None, None, None)`; b = 1: `R[r](type(acc), acc, tb)`, re-raising `acc` unless the result is true. b = 2 and 3 (`async with`): the same calls, `acc` = the awaitable they return.";
    MATCH = 93, "bru", "Pattern matching step `b` (see `compiler::match_kind`) on `acc` and `R[r]`.";
    // Fused pairs (`fuse`): the first instruction's opcode byte rewritten,
    // the second left in place. The fused handler runs both when it can,
    // otherwise exactly the first, so the second stays reachable as is.
    LDA_GLOBAL_STAR = 94, "uu", "`LDA_GLOBAL` then the `STAR` that follows.";
    LDA_SMI_ADD = 95, "i", "`LDA_SMI` then the `ADD` that follows.";
    LDA_SMI_SUB = 96, "i", "`LDA_SMI` then the `SUB` that follows.";
    TEST_LT_JF = 97, "r", "`TEST_LT` then the `JUMP_IF_FALSE` that follows.";
    TEST_LE_JF = 98, "r", "`TEST_LE` then the `JUMP_IF_FALSE` that follows.";
    TEST_GT_JF = 99, "r", "`TEST_GT` then the `JUMP_IF_FALSE` that follows.";
    TEST_GE_JF = 100, "r", "`TEST_GE` then the `JUMP_IF_FALSE` that follows.";
    FOR_ITER_STAR = 101, "rj", "`FOR_ITER` then the `STAR` that follows.";
    FOR_PREP = 102, "rbj", "`range(R[r+1..r+1+b])` with `R[r]` the builtin `range` and int arguments: `R[r..r+3]` = start, stop, step; jump (a safepoint, as the call it replaces). Otherwise falls through to the generic call (`MOV`s shifting the arguments up to `R[r+2..]`, then `CALL r b`).";
    FOR_RANGE = 103, "rj", "With an int in `R[r+1]`: `acc = R[r]`, `R[r] += R[r+2]` while before the stop, else jump. Otherwise `FOR_ITER r`.";
    FOR_RANGE_STAR = 104, "rj", "`FOR_RANGE` then the `STAR` that follows.";
    GET_AITER = 105, "", "`acc = aiter(acc)` for `async for`.";
    ANEXT = 106, "r", "`acc = R[r].__anext__()` (an awaitable) for `async for`.";
    END_ASYNC_FOR = 107, "", "An `async for` handler: falls through when `acc` is a `StopAsyncIteration`, else re-raises it.";
    CHECK = 109, "", "A safepoint check that charges nothing: parks if fuel, the deadline, a request or a collection is due. The compiler puts one where straight-line code has run `CHECK_EVERY` instructions since the last safepoint (C4).";
    EXC_STAR = 108, "rb", "`except*` over the state list `R[r]` (`[orig, rest, raised...]`): b = 0: `acc` = the part of the rest matching the type in `acc`, or None (the prelude's `_star_match`); b = 1: `acc` = what the statement re-raises, or None (`_star_prep`).";
}

/// Fused opcodes: `(first, second, fused)`.
pub const FUSED: [(u8, u8, u8); 9] = [
    (op::LDA_GLOBAL, op::STAR, op::LDA_GLOBAL_STAR),
    (op::LDA_SMI, op::ADD, op::LDA_SMI_ADD),
    (op::LDA_SMI, op::SUB, op::LDA_SMI_SUB),
    (op::TEST_LT, op::JUMP_IF_FALSE, op::TEST_LT_JF),
    (op::TEST_LE, op::JUMP_IF_FALSE, op::TEST_LE_JF),
    (op::TEST_GT, op::JUMP_IF_FALSE, op::TEST_GT_JF),
    (op::TEST_GE, op::JUMP_IF_FALSE, op::TEST_GE_JF),
    (op::FOR_ITER, op::STAR, op::FOR_ITER_STAR),
    (op::FOR_RANGE, op::STAR, op::FOR_RANGE_STAR),
];

/// The instruction a fused opcode starts with (`op` itself otherwise).
#[must_use]
pub fn unfused(o: u8) -> u8 {
    FUSED.iter().find(|f| f.2 == o).map_or(o, |f| f.0)
}

/// Rewrites the first opcode of each fusable pair of adjacent instructions.
pub fn fuse(bc: &mut [u32]) {
    let mut pc = 0;
    while pc < bc.len() {
        let o = opcode(bc, pc);
        let len = instr_len(o);
        if pc + len < bc.len()
            && let Some(f) = FUSED
                .iter()
                .find(|f| f.0 == o && f.1 == opcode(bc, pc + len))
        {
            bc[pc] = bc[pc] & !0xff | u32::from(f.2);
        }
        pc += len;
    }
}

/// Words a code may have: every word pc, and the length, fits a `j`
/// operand (24 bits).
pub const MAX_WORDS: usize = 1 << 24;

/// Byte length of an instruction with operand format `fmt` in the
/// compiler's byte stream: the opcode, then `r`/`b` 1, `u`/`i` 2 and `j` 4
/// little-endian bytes.
const fn byte_len(fmt: &str) -> usize {
    let b = fmt.as_bytes();
    let (mut n, mut i) = (1, 0);
    while i < b.len() {
        n += match b[i] {
            b'r' | b'b' => 1,
            b'u' | b'i' => 2,
            _ => 4,
        };
        i += 1;
    }
    n
}

/// Assembles a byte stream (see `byte_len`; `j` operands are byte pcs) into words per [`layout`], with
/// every `j` operand rewritten to the target's word pc. Also returns the
/// map from byte pc to word pc: `bytes.len() + 1` entries, the last the
/// word length, `u32::MAX` inside an instruction. `None` if an opcode is
/// unknown, the last instruction is cut short, a jump target is not an
/// instruction start (or the end), or the code needs [`MAX_WORDS`] words
/// or more. Does not fuse (see [`fuse`]).
#[must_use]
pub fn assemble(bytes: &[u8]) -> Option<(Vec<u32>, Vec<u32>)> {
    let mut map = alloc::vec![u32::MAX; bytes.len() + 1];
    let (mut pc, mut words) = (0, 0usize);
    while let Some(&o) = bytes.get(pc) {
        let len = byte_len(FORMATS[o as usize]);
        if NAMES[o as usize].is_empty() || pc + len > bytes.len() {
            return None;
        }
        map[pc] = u32::try_from(words).ok()?;
        words += instr_len(o);
        pc += len;
    }
    if words >= MAX_WORDS {
        return None;
    }
    map[pc] = u32::try_from(words).ok()?;
    let mut out = alloc::vec![0u32; words];
    let mut pc = 0;
    while let Some(&o) = bytes.get(pc) {
        let at = map[pc] as usize;
        let l = &layout_of(o);
        out[at] = u32::from(o);
        let mut k = pc + 1;
        for (i, c) in FORMATS[o as usize].bytes().enumerate() {
            let b = |d: usize| u32::from(bytes.get(k + d).copied().unwrap_or(0));
            let (v, n) = match c {
                b'r' | b'b' => (b(0), 1),
                b'u' | b'i' => (b(0) | b(1) << 8, 2),
                _ => {
                    let target = b(0) | b(1) << 8 | b(2) << 16 | b(3) << 24;
                    let w = *map.get(target as usize)?;
                    if w == u32::MAX {
                        return None;
                    }
                    (w, 4)
                }
            };
            let f = l.fields[i];
            out[at + f.word as usize] |= v << f.shift;
            k += n;
        }
        pc += byte_len(FORMATS[o as usize]);
    }
    Some((out, map))
}

/// The bits of word `w` of an instruction with layout `l` that hold the
/// opcode or an operand.
fn used_bits(l: &Layout, w: usize) -> u32 {
    let mut m = if w == 0 { 0xff } else { 0 };
    for f in l.fields.iter().take(l.n as usize) {
        if f.word as usize == w {
            m |= u32::try_from(((1u64 << f.width) - 1) << f.shift).unwrap_or(u32::MAX);
        }
    }
    m
}

/// A constant.
#[derive(Clone, Debug, PartialEq)]
pub enum Konst {
    /// `None`.
    None,
    /// A bool.
    Bool(bool),
    /// `...`.
    Ellipsis,
    /// An int.
    Int(i64),
    /// A float.
    Float(f64),
    /// A str.
    Str(String),
    /// A tuple of strs (keyword names of a call).
    Names(Vec<String>),
    /// An int past `i64`, as literal text.
    BigInt(String),
    /// A bytes literal.
    Bytes(Vec<u8>),
    /// A complex number (real, imaginary).
    Complex(f64, f64),
}

/// One exception-table entry: a handler for `start..end`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExcEntry {
    /// First covered pc.
    pub start: u32,
    /// First pc past the range.
    pub end: u32,
    /// Handler pc; the exception arrives in the accumulator.
    pub handler: u32,
    /// Nesting depth of the handler (1 = outermost).
    pub depth: u32,
}

/// The source position of the instructions from `pc` up to the next entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PosEntry {
    /// First pc this entry covers.
    pub pc: u32,
    /// The range.
    pub pos: crate::ast::Pos,
    /// Caret anchors.
    pub anchor: crate::ast::Anchor,
    /// No carets for this instruction (CPython's full-line call rule).
    pub no_carets: bool,
}

/// The variable a register holds over `start..end`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegName {
    /// First pc.
    pub start: u32,
    /// First pc past the range.
    pub end: u32,
    /// The register.
    pub reg: u8,
    /// The variable's name.
    pub name: String,
}

/// Parameter layout of a code object. Parameters occupy the first registers:
/// positional (`argcount`, of which `posonly` positional-only), keyword-only,
/// then `*args` and `**kwargs` if present.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct ParamSpec {
    /// Positional parameters, positional-only included.
    pub argcount: u32,
    /// Positional-only parameters.
    pub posonly: u32,
    /// Keyword-only parameters.
    pub kwonly: u32,
    /// Has `*args`.
    pub varargs: bool,
    /// Has `**kwargs`.
    pub varkw: bool,
}

impl ParamSpec {
    /// Registers the parameters take.
    #[must_use]
    pub fn total(&self) -> u32 {
        self.argcount + self.kwonly + u32::from(self.varargs) + u32::from(self.varkw)
    }

    /// Plain positional only: no defaults needed, no `*`/`**`/keyword-only.
    #[must_use]
    pub fn is_simple(&self) -> bool {
        self.kwonly == 0 && !self.varargs && !self.varkw
    }
}

/// An immutable compiled function or module body.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Code {
    /// `co_name`.
    pub name: Arc<str>,
    /// `co_qualname`.
    pub qualname: Arc<str>,
    /// Line of the `def`.
    pub first_line: u32,
    /// The instructions: 32-bit words (see [`layout`]); pcs are word
    /// indices.
    pub bytecode: Vec<u32>,
    /// Constants.
    pub consts: Vec<Konst>,
    /// Global and attribute names.
    pub names: Vec<Arc<str>>,
    /// Register count of a frame.
    pub nregs: u32,
    /// Names of the variable registers (`nvars` = its length).
    pub varnames: Vec<Arc<str>>,
    /// Parameters.
    pub params: ParamSpec,
    /// Registers that receive the closure's cells at entry, in closure order.
    pub freevars: Vec<u8>,
    /// Names of those free variables.
    pub freenames: Vec<Arc<str>>,
    /// Exception table, sorted by `start`, non-overlapping.
    pub exc_table: Vec<ExcEntry>,
    /// Position table, sorted by `pc`.
    pub positions: Vec<PosEntry>,
    /// Names of registers that hold different variables over different pc
    /// ranges (inlined comprehension variables), for error messages.
    pub reg_names: Vec<RegName>,
    /// Inline-cache slots.
    pub n_ics: u32,
    /// Is this a module body (globals are its variables)?
    pub is_module: bool,
    /// `async def`: calling the function makes a coroutine object.
    pub is_coroutine: bool,
    /// Contains `yield`: calling the function makes a generator object.
    pub is_generator: bool,
    /// The function's docstring.
    pub doc: Option<String>,
}

impl Code {
    /// Bytes owned by this code's buffers, excluding the inline `Code` and
    /// allocator metadata. Also an upper bound for the buffers of a clone.
    #[must_use]
    pub fn buffer_bytes(&self) -> u64 {
        let strings = |v: &Vec<Arc<str>>| {
            (v.capacity() * size_of::<Arc<str>>()) as u64
                + v.iter().map(|s| s.len() as u64).sum::<u64>()
        };
        let mut bytes = (self.name.len()
            + self.qualname.len()
            + self.bytecode.capacity() * size_of::<u32>()
            + self.freevars.capacity()
            + self.consts.capacity() * size_of::<Konst>()
            + self.exc_table.capacity() * size_of::<ExcEntry>()
            + self.positions.capacity() * size_of::<PosEntry>()
            + self.reg_names.capacity() * size_of::<RegName>()) as u64;
        bytes += strings(&self.names) + strings(&self.varnames) + strings(&self.freenames);
        bytes += self
            .reg_names
            .iter()
            .map(|r| r.name.capacity() as u64)
            .sum::<u64>();
        bytes += self.doc.as_ref().map_or(0, |s| s.capacity() as u64);
        for k in &self.consts {
            bytes += match k {
                Konst::Str(s) | Konst::BigInt(s) => s.capacity() as u64,
                Konst::Bytes(b) => b.capacity() as u64,
                Konst::Names(ns) => {
                    (ns.capacity() * size_of::<String>()) as u64
                        + ns.iter().map(|s| s.capacity() as u64).sum::<u64>()
                }
                _ => 0,
            };
        }
        bytes
    }

    /// The position entry covering `pc`.
    #[must_use]
    pub fn position(&self, pc: u32) -> Option<&PosEntry> {
        let i = self.positions.partition_point(|p| p.pc <= pc);
        i.checked_sub(1).and_then(|i| self.positions.get(i))
    }

    /// The name of the variable in register `reg` at `pc`.
    #[must_use]
    pub fn reg_name(&self, reg: u8, pc: u32) -> Option<&str> {
        self.reg_names
            .iter()
            .rev()
            .find(|n| n.reg == reg && n.start <= pc && pc < n.end)
            .map(|n| n.name.as_str())
            .or_else(|| self.varnames.get(usize::from(reg)).map(|s| &**s))
    }

    /// The handler covering `pc`.
    #[must_use]
    pub fn handler(&self, pc: u32) -> Option<&ExcEntry> {
        let i = self.exc_table.partition_point(|e| e.start <= pc);
        i.checked_sub(1)
            .and_then(|i| self.exc_table.get(i))
            .filter(|e| pc < e.end)
    }
}

/// The output of compiling a module: code objects that refer to each other by
/// index (`MAKE_FUNCTION` operands index this list).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Program {
    /// Every code object; `codes[entry]` is the module body. Shared, so
    /// sessions loading a program (the prelude, a compiled cell) reference
    /// its code instead of copying it.
    pub codes: Vec<Arc<Code>>,
    /// The module body.
    pub entry: u32,
    /// The module body returns the value of a final expression statement.
    pub returns_last_expr: bool,
    /// Compile-time `SyntaxWarning`s, in source order.
    pub warnings: Vec<(crate::ast::Pos, String)>,
}

impl Code {
    /// Maximum temporary buffer capacity used by [`Self::verify`], excluding
    /// allocator metadata. Instruction-boundary queries allocate nothing.
    #[must_use]
    pub fn verify_scratch_bytes(&self) -> usize {
        self.bytecode.len().div_ceil(8)
    }

    /// Checks that `self` is safe to run although it was not compiled in
    /// this process (a restored snapshot): every instruction is known and
    /// complete, control cannot fall off the end, jump and handler targets
    /// are instruction starts, only `JUMP_LOOP` (a safepoint) jumps back and
    /// handlers follow the code they cover (so fuel bounds every loop), and
    /// every register, constant, name and inline-cache operand is in range
    /// for the operation that reads it.
    #[must_use]
    pub fn verify(&self) -> bool {
        let bc = &self.bytecode;
        let n = self.nregs as usize;
        let Some(starts) = self.instr_starts() else {
            return false;
        };
        let at = |pc: usize| starts.get(pc / 8).is_some_and(|b| b & (1 << (pc % 8)) != 0);
        let mut pc = 0;
        let mut last = op::NOP;
        while pc < bc.len() {
            let raw = opcode(bc, pc);
            let l = &layout_of(raw);
            // Bits that hold no operand are zero, so a handler may read the
            // last field of a word with a shift alone.
            if (0..l.words as usize)
                .any(|w| bc.get(pc + w).is_none_or(|&x| x & !used_bits(l, w) != 0))
            {
                return false;
            }
            // A fused opcode checks as its first instruction, followed by
            // its partner (which is checked as an instruction of its own).
            let o = unfused(raw);
            let partner = FUSED
                .iter()
                .find(|f| f.2 == raw)
                .is_none_or(|f| opcode(bc, pc + instr_len(raw)) == f.1);
            if !partner {
                return false;
            }
            let a = |k: usize| operand(bc, pc, k);
            let reg = |r: usize| r < n;
            let span = |r: usize, k: usize| r + k <= n;
            let konst = |k: usize| k < self.consts.len();
            let name = |k: usize| k < self.names.len();
            let ic = |k: usize| k < self.n_ics as usize;
            let kwnames = |k: usize, argc: usize| matches!(self.consts.get(k), Some(Konst::Names(ns)) if ns.len() <= argc);
            let ok = match o {
                op::LDAR
                | op::STAR
                | op::LDA_LOCAL
                | op::LDA_DEREF
                | op::STA_DEREF
                | op::DEL_DEREF
                | op::MAKE_CELL
                | op::DEL_LOCAL
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
                | op::LIST_EXTEND
                | op::SET_ADD
                | op::SET_UPDATE
                | op::FORMAT_SPEC
                | op::GET_ITEM
                | op::DEL_ITEM
                | op::PUSH_EXC
                | op::POP_EXC
                | op::CHECK_EXC
                | op::EXC_STAR
                | op::WITH_ENTER
                | op::WITH_EXIT
                | op::ANEXT
                | op::YIELD_FROM => reg(a(0)),
                op::MOV | op::DICT_INSERT | op::SET_ITEM => reg(a(0)) && reg(a(1)),
                op::BINOP | op::INPLACE => reg(a(1)),
                op::LDA_CONST => konst(a(0)),
                op::LDA_GLOBAL | op::STA_GLOBAL | op::GET_ATTR => name(a(0)) && ic(a(1)),
                op::DEL_GLOBAL | op::DEL_ATTR | op::IMPORT_NAME => name(a(0)),
                op::LDA_NAME | op::STA_NAME | op::DEL_NAME => name(a(0)) && n >= 1,
                // Only `JUMP_LOOP`, a safepoint, may jump back: a loop
                // without one could not be stopped by fuel.
                op::JUMP_LOOP => at(a(0)) && a(0) <= pc,
                op::JUMP | op::JUMP_IF_TRUE | op::JUMP_IF_FALSE => at(a(0)) && a(0) > pc,
                op::FOR_ITER | op::SEND => reg(a(0)) && at(a(1)) && a(1) > pc,
                op::FOR_RANGE => span(a(0), 3) && at(a(1)) && a(1) > pc,
                op::FOR_PREP => span(a(0), 4) && (1..=3).contains(&a(1)) && at(a(2)) && a(2) > pc,
                op::BUILD_LIST
                | op::BUILD_TUPLE
                | op::BUILD_SET
                | op::BUILD_STRING
                | op::UNPACK_SEQ => span(a(0), a(1)),
                op::BUILD_DICT => span(a(0), 2 * a(1)),
                op::BUILD_SLICE | op::CALL_EX => span(a(0), 3),
                op::BUILD_CLASS => span(a(0), 4),
                op::DICT_UPDATE => reg(a(0)) && reg(a(2)),
                op::UNPACK_EX => span(a(0), a(1) + a(2) + 1),
                op::SET_ATTR => reg(a(0)) && name(a(1)) && ic(a(2)),
                op::LOAD_METHOD => span(a(0), 3) && name(a(1)) && ic(a(2)),
                op::IMPORT_FROM => reg(a(0)) && name(a(1)),
                op::CALL => span(a(0), 2 + a(1)) && ic(a(2)),
                op::CALL_METHOD => span(a(0), 3 + a(1)) && ic(a(2)),
                op::CALL_KW => span(a(0), 2 + a(1)) && kwnames(a(2), a(1)),
                op::CALL_METHOD_KW => span(a(0), 3 + a(1)) && kwnames(a(2), a(1)),
                op::MAKE_FUNCTION => span(a(2), (a(1) & 7).count_ones() as usize),
                op::RAISE => a(0) < 2 || reg(a(1)),
                op::MATCH => match a(0) {
                    0..=2 => true,
                    3 | 4 => reg(a(1)),
                    5 => span(a(1), 2),
                    _ => false,
                },
                _ => true,
            };
            if !ok {
                return false;
            }
            last = o;
            pc += instr_len(o);
        }
        let p = &self.params;
        matches!(
            last,
            op::RETURN | op::JUMP | op::JUMP_LOOP | op::RAISE | op::RERAISE | op::ASSERT_FAIL
        ) && p.total() as usize <= n
            && p.total() as usize <= self.varnames.len()
            && p.posonly <= p.argcount
            && self.freevars.iter().all(|&r| usize::from(r) < n)
            && self
                .exc_table
                .iter()
                .all(|e| e.start <= e.end && e.end <= e.handler && at(e.handler as usize))
            && self.exc_table.is_sorted_by_key(|e| e.start)
    }

    /// Whether word `pc` starts an instruction (instructions are complete).
    #[must_use]
    pub fn is_instr_start(&self, pc: usize) -> bool {
        let bc = &self.bytecode;
        if pc >= bc.len() {
            return false;
        }
        let mut at = 0;
        let mut found = false;
        while at < bc.len() {
            let o = opcode(bc, at);
            if NAMES[o as usize].is_empty() || at + instr_len(o) > bc.len() {
                return false;
            }
            found |= at == pc;
            at += instr_len(o);
        }
        found
    }

    /// One bit per word: does an instruction start there? `None` if an
    /// opcode is unknown or the last instruction is cut short.
    fn instr_starts(&self) -> Option<Vec<u8>> {
        let bc = &self.bytecode;
        let mut starts = alloc::vec![0; self.verify_scratch_bytes()];
        let mut pc = 0;
        while pc < bc.len() {
            let o = opcode(bc, pc);
            if NAMES[o as usize].is_empty() || pc + instr_len(o) > bc.len() {
                return None;
            }
            starts[pc / 8] |= 1 << (pc % 8);
            pc += instr_len(o);
        }
        Some(starts)
    }
}

/// Length in words of the instruction with opcode `op` (1 if unknown).
#[must_use]
pub fn instr_len(op: u8) -> usize {
    1 + (TWO_WORDS[(op / 32) as usize] >> (op % 32) & 1) as usize
}

/// Opcode `op`'s operands as [`plan`] lists them.
pub(crate) fn operand_plan(op: u8) -> u32 {
    PLANS[op as usize]
}

/// Format `fmt`'s operands, nine bits each, the first in the low bits:
/// the kind (1 `r`, 2 `b`, 3 `u`, 4 `i`, 5 `j`), then the operand's bit
/// offset in the instruction's two words (the second word from bit 32).
const fn plan(fmt: &str) -> u32 {
    let b = fmt.as_bytes();
    let l = layout(fmt);
    let mut p = 0;
    let mut i = 0;
    while i < b.len() {
        let kind = match b[i] {
            b'r' => 1,
            b'b' => 2,
            b'u' => 3,
            b'i' => 4,
            _ => 5,
        };
        let f = l.fields[i];
        p |= (kind | (f.word as u32 * 32 + f.shift as u32) << 3) << (9 * i);
        i += 1;
    }
    p
}

/// The operand layout of opcode `op` (see [`layout`]).
#[must_use]
#[inline]
#[allow(
    clippy::cast_possible_truncation,
    reason = "each packed field is one byte"
)]
pub fn layout_of(op: u8) -> Layout {
    let p = PACKED[op as usize];
    let mut l = layout("");
    while l.n < 4 {
        let b = (p >> (8 * l.n)) as u8;
        if b == 0 {
            break;
        }
        let word = b >> 2 & 1;
        l.fields[l.n as usize] = Field {
            word,
            shift: b >> 3,
            width: (b & 3) * 8,
        };
        l.words = l.words.max(word + 1);
        l.n += 1;
    }
    l
}

/// The bytes the words of an instruction with opcode `op` carry, in the
/// first word (counting the opcode) and the second. Operands fill each word
/// from its low used bit up, so each word is those bytes little endian.
#[must_use]
#[inline]
pub fn word_bytes(op: u8) -> (usize, usize) {
    let s = SPLIT[op as usize];
    (usize::from(s & 15), usize::from(s >> 4))
}

#[allow(
    clippy::cast_possible_truncation,
    reason = "an instruction is at most eight bytes"
)]
const fn split(l: Layout) -> u8 {
    let mut n = [1u8, 0];
    let mut i = 0;
    while i < l.n as usize {
        n[l.fields[i].word as usize] += l.fields[i].width / 8;
        i += 1;
    }
    n[0] | n[1] << 4
}

/// Layout `l` as one byte per field: width in bytes (bits 0..2, nonzero),
/// word (bit 2), shift (bits 3..8).
const fn pack(l: Layout) -> u32 {
    let mut p = 0;
    let mut i = 0;
    while i < l.n as usize {
        let f = l.fields[i];
        p |= ((f.width / 8) as u32 | (f.word as u32) << 2 | (f.shift as u32) << 3) << (8 * i);
        i += 1;
    }
    p
}

/// One operand's place: word `word` of the instruction, bits
/// `shift..shift + width`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Field {
    /// Word offset from the instruction's first word.
    pub word: u8,
    /// Bit offset in that word.
    pub shift: u8,
    /// Width in bits.
    pub width: u8,
}

/// Where an instruction's operands are, and its length in words.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Layout {
    /// The operands, in format order (unused entries are zero).
    pub fields: [Field; 4],
    /// Operand count.
    pub n: u8,
    /// Instruction length in words.
    pub words: u8,
}

/// The word layout of operand format `fmt`. The opcode is bits 0..8 of the
/// first word. Operands follow in order from bit 8, each in the current word
/// if it fits and otherwise at bit 0 of the next word. Widths: `r` and `b`
/// 8 bits, `u` and `i` 16, `j` (a word pc) 24. So `r`/`b` fields of the
/// first word are bits 8, 16, 24 (A, B, C), and a lone `u`, `i` or `j` is
/// bits 8.. of the first word.
#[must_use]
#[allow(
    clippy::cast_possible_truncation,
    reason = "a format has at most four operands"
)]
pub const fn layout(fmt: &str) -> Layout {
    let b = fmt.as_bytes();
    let zero = Field {
        word: 0,
        shift: 0,
        width: 0,
    };
    let mut fields = [zero; 4];
    let (mut word, mut shift, mut i) = (0u8, 8u8, 0);
    while i < b.len() {
        let width = match b[i] {
            b'r' | b'b' => 8,
            b'u' | b'i' => 16,
            _ => 24,
        };
        if shift + width > 32 {
            word += 1;
            shift = 0;
        }
        fields[i] = Field { word, shift, width };
        shift += width;
        i += 1;
    }
    Layout {
        fields,
        n: b.len() as u8,
        words: word + 1,
    }
}

/// Operand `k` of the instruction at word `pc` (0 if absent or cut short).
/// `i` operands are returned as their 16 raw bits.
#[must_use]
#[inline(never)]
pub fn operand(bc: &[u32], pc: usize, k: usize) -> usize {
    let Some(&w0) = bc.get(pc) else { return 0 };
    let l = &layout_of((w0 & 0xff) as u8);
    if k >= l.n as usize {
        return 0;
    }
    let f = l.fields[k];
    bc.get(pc + f.word as usize)
        .map_or(0, |&w| ((w >> f.shift) & ((1u32 << f.width) - 1)) as usize)
}

/// The opcode of the instruction at word `pc` (`NOP` past the end).
#[must_use]
pub fn opcode(bc: &[u32], pc: usize) -> u8 {
    bc.get(pc).map_or(op::NOP, |&w| (w & 0xff) as u8)
}

/// A readable listing of `code` (for tests and debugging): word pcs and
/// decoded operands (`j` as a word pc, `i` signed).
#[must_use]
pub fn disassemble(code: &Code) -> String {
    use core::fmt::Write;
    let mut out = String::new();
    let b = &code.bytecode;
    let mut pc = 0;
    while pc < b.len() {
        let op = opcode(b, pc);
        let _ = write!(out, "{pc:5} {}", NAMES[op as usize]);
        for (k, c) in FORMATS[op as usize].bytes().enumerate() {
            let v = operand(b, pc, k);
            if c == b'i' {
                let _ = write!(out, " {}", u16::try_from(v).unwrap_or(0).cast_signed());
            } else {
                let _ = write!(out, " {v}");
            }
        }
        out.push('\n');
        pc += instr_len(op);
    }
    out
}
