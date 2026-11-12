//! Word bytecode: compiled code verifies, and jump, handler, position and
//! register-name pcs are word pcs that land on instruction starts.

#![allow(clippy::expect_used, clippy::panic, reason = "tests")]

use mm_compile::bytecode::{FORMATS, NAMES, assemble, disassemble, op};
use mm_compile::{Options, compile};

const SRC: &str = "\
def f(n, xs):
    total = 0
    i = 0
    while i < n:
        if i % 3 == 0:
            total += 1
        elif i % 3 == 1:
            total -= 2
        else:
            total = total * 2
        i += 1
    for k in range(1, n, 2):
        total += k
    try:
        total = total // xs[0]
    except (ZeroDivisionError, IndexError) as e:
        total = -1
    finally:
        total += 0
    return [y * 2 for y in xs if y] + [total]

print(f(10, [1, 2, 3]))
";

#[test]
fn compiled_words_verify_and_jumps_land_on_instruction_starts() {
    let module = mm_parse::parse(SRC).expect("parses");
    let program = compile(&module, Options::default()).expect("compiles");
    let mut jumps = 0;
    let mut fused = 0;
    let mut seen_for_prep = false;
    for code in &program.codes {
        assert!(
            code.verify(),
            "{} does not verify:\n{}",
            code.name,
            disassemble(code)
        );
        let len = code.bytecode.len();
        for line in disassemble(code).lines() {
            let mut parts = line.split_whitespace();
            let pc: usize = parts.next().expect("pc").parse().expect("number");
            let name = parts.next().expect("name");
            assert!(code.is_instr_start(pc), "{line}");
            let o = NAMES.iter().position(|n| *n == name).expect("known opcode");
            let operands: Vec<i64> = parts.map(|p| p.parse().expect("operand")).collect();
            let fmt = FORMATS[o];
            assert_eq!(operands.len(), fmt.len(), "{line}");
            for (c, v) in fmt.bytes().zip(&operands) {
                if c == b'j' {
                    let target = usize::try_from(*v).expect("target");
                    assert!(code.is_instr_start(target), "{line}: target {target}");
                    jumps += 1;
                }
            }
            fused += usize::from(NAMES[o].ends_with("_JF") || NAMES[o].ends_with("_STAR"));
            seen_for_prep |= o == usize::from(op::FOR_PREP);
        }
        for e in &code.exc_table {
            assert!(code.is_instr_start(e.start as usize), "{e:?}");
            assert!(e.end as usize <= len && e.start < e.end, "{e:?}");
            assert!(code.is_instr_start(e.handler as usize), "{e:?}");
        }
        for p in &code.positions {
            assert!(code.is_instr_start(p.pc as usize), "{p:?}");
        }
        for r in &code.reg_names {
            assert!(code.is_instr_start(r.start as usize), "{r:?}");
            assert!(
                r.end as usize == len || code.is_instr_start(r.end as usize),
                "{r:?}"
            );
        }
        let mut bytes = Vec::new();
        code.encode_into(&mut bytes);
        let mut rest = bytes.as_slice();
        let decoded = mm_compile::bytecode::Code::decode(&mut rest).expect("decodes");
        assert_eq!(rest, &[] as &[u8]);
        assert_eq!(&decoded, &**code);
    }
    assert!(jumps >= 8, "{jumps} jumps");
    assert!(fused >= 1, "no fused pair");
    assert!(seen_for_prep, "no FOR_PREP");
    assert!(program.codes.iter().any(|c| !c.exc_table.is_empty()));
}

#[test]
fn assemble_maps_byte_pcs_and_rejects_bad_targets() {
    // 0: LDA_SMI 7 (3 bytes) / 3: JUMP_IF_FALSE -> 15 / 8: CALL r1 b0 u2
    // 13: NOP / 14: NOP / 15: FOR_PREP r0 b1 -> 22 / 22: RETURN.
    let bytes = [
        op::LDA_SMI,
        7,
        0,
        op::JUMP_IF_FALSE,
        15,
        0,
        0,
        0,
        op::CALL,
        1,
        0,
        2,
        0,
        op::NOP,
        op::NOP,
        op::FOR_PREP,
        0,
        1,
        22,
        0,
        0,
        0,
        op::RETURN,
    ];
    let (words, map) = assemble(&bytes).expect("assembles");
    assert_eq!(map.len(), bytes.len() + 1);
    // LDA_SMI 1 word, JUMP_IF_FALSE 1, CALL 2 (r, b, then u in the next
    // word), NOP 1, NOP 1, FOR_PREP 2 (j in the next word), RETURN 1.
    let starts = [(0, 0), (3, 1), (8, 2), (13, 4), (14, 5), (15, 6), (22, 8)];
    for (b, w) in starts {
        assert_eq!(map[b], w, "byte {b}");
    }
    assert_eq!(map[bytes.len()], 9);
    assert_eq!(map[1], u32::MAX);
    assert_eq!(
        words,
        [
            u32::from(op::LDA_SMI) | 7 << 8,
            u32::from(op::JUMP_IF_FALSE) | 6 << 8,
            u32::from(op::CALL) | 1 << 8,
            2,
            u32::from(op::NOP),
            u32::from(op::NOP),
            u32::from(op::FOR_PREP) | 1 << 16,
            8,
            u32::from(op::RETURN),
        ]
    );
    // A jump into the middle of an instruction, an unknown opcode and a cut
    // short instruction do not assemble.
    let mut bad = bytes;
    bad[4] = 9;
    assert!(assemble(&bad).is_none());
    assert!(assemble(&[255]).is_none());
    assert!(assemble(&[op::LDA_SMI, 1]).is_none());
}

#[test]
fn call_windows_leave_a_reserved_slot_before_the_arguments() {
    let src = "def f(g, o, a):\n    g(a, 2)\n    o.m(a)\n    g(x=a)\n";
    let module = mm_parse::parse(src).expect("parses");
    let program = compile(&module, Options::default()).expect("compiles");
    let f = program.codes.iter().find(|c| &*c.name == "f").expect("f");
    assert!(f.verify());
    let listing = disassemble(f);
    let ops: Vec<Vec<&str>> = listing
        .lines()
        .map(|l| l.split_whitespace().skip(1).collect())
        .collect();
    // Writes before each call: the callee to `R[r]` (method and self to
    // `R[r]` and `R[r+2]`), arguments from `R[r+2]` (`R[r+3]`), never
    // `R[r+1]`.
    let mut calls = 0;
    for (i, o) in ops.iter().enumerate() {
        let (first_arg, reserved) = match o[0] {
            "CALL" | "CALL_KW" => (2, 1),
            "CALL_METHOD" | "CALL_METHOD_KW" => (3, 1),
            _ => continue,
        };
        calls += 1;
        let r: usize = o[1].parse().expect("r");
        let n: usize = o[2].parse().expect("n");
        let stars: Vec<usize> = ops[..i]
            .iter()
            .rev()
            .take_while(|p| !p[0].starts_with("CALL"))
            .filter(|p| p[0] == "STAR")
            .map(|p| p[1].parse().expect("reg"))
            .collect();
        for k in 0..n {
            assert!(stars.contains(&(r + first_arg + k)), "{listing}");
        }
        assert!(!stars.contains(&(r + reserved)), "{listing}");
        assert!(stars.iter().all(|&s| s < r + first_arg + n), "{listing}");
    }
    assert_eq!(calls, 3, "{listing}");
    assert!(listing.contains("LOAD_METHOD"), "{listing}");
}
