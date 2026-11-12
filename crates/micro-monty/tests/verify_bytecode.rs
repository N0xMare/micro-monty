//! The snapshot reader's bytecode verifier accepts everything the compiler
//! produces: the prelude and every conformance case that compiles.

#![allow(clippy::unwrap_used, clippy::expect_used, reason = "test")]

use std::path::Path;

#[test]
fn compiler_output_verifies() {
    let prelude = mm_engine::prelude_programs().expect("prelude decodes");
    assert!(prelude.len() > 100, "prelude units: {}", prelude.len());
    for c in prelude.iter().flat_map(|p| &p.codes) {
        assert!(c.verify(), "prelude code {} rejected", c.qualname);
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../conformance");
    let mut n = 0;
    for dir in ["monty", "own"] {
        for entry in std::fs::read_dir(root.join(dir)).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_none_or(|x| x != "py") {
                continue;
            }
            let Ok(src) = std::fs::read_to_string(&path) else {
                continue;
            };
            let Ok(module) = mm_parse::parse(&src) else {
                continue;
            };
            let opts = mm_compile::Options {
                last_expr_value: true,
            };
            let Ok(program) = mm_compile::compile(&module, opts) else {
                continue;
            };
            for c in &program.codes {
                assert!(
                    c.verify(),
                    "{}: code {} rejected",
                    path.display(),
                    c.qualname
                );
                let mut encoded = Vec::new();
                c.encode_into(&mut encoded);
                let decoded = mm_compile::bytecode::Code::decode_bounded(
                    &mut encoded.as_slice(),
                    &mut c.buffer_bytes(),
                )
                .expect("code decodes within its original buffer budget");
                assert_eq!(&decoded, &**c);
                n += 1;
            }
        }
    }
    assert!(n > 1000, "only {n} code objects checked");
}

#[test]
fn code_decode_budgets_decoded_buffers_not_encoded_bytes() {
    use mm_compile::{
        DecodeError,
        bytecode::{Code, op},
    };

    let code = Code {
        bytecode: vec![u32::from(op::RETURN)],
        names: vec![std::sync::Arc::from(""); 100],
        ..Code::default()
    };
    let mut encoded = Vec::new();
    code.encode_into(&mut encoded);
    // Empty strings have no data buffers, but their vector still owns slots.
    let required = (100 * size_of::<std::sync::Arc<str>>() + 4) as u64;
    assert!((encoded.len() as u64) < required);
    for allowance in [required - 1, required, required + 17] {
        let mut input = encoded.as_slice();
        let mut budget = allowance;
        let decoded = Code::decode_bounded(&mut input, &mut budget);
        if allowance < required {
            assert_eq!(decoded, Err(DecodeError::TooLarge));
            assert_eq!(budget, allowance);
            assert_eq!(input, encoded);
        } else {
            assert_eq!(decoded.expect("fits"), code);
            assert_eq!(budget, allowance - required);
            assert_eq!(input, []);
        }
    }
    let mut budget = required;
    assert_eq!(
        Code::decode_bounded(&mut &encoded[..encoded.len() - 1], &mut budget),
        Err(DecodeError::Invalid)
    );
}

#[test]
fn program_decode_budgets_the_pool_and_each_owned_string_copy() {
    use mm_compile::{
        DecodeError,
        bytecode::{Code, Program, op},
    };

    let program = Program {
        codes: vec![
            Code {
                bytecode: vec![u32::from(op::RETURN)],
                names: vec!["é".into(), "é".into(), "€".into()],
                ..Code::default()
            }
            .into(),
        ],
        ..Program::default()
    };
    let encoded = program.encode();
    // Code vector (a shared pointer to the code, in its own allocation with
    // two counts), one word of bytecode, three owned names (2 + 2 + 3
    // UTF-8 bytes), plus a pool containing "", "é" and "€" (2 + 3 bytes).
    let code = size_of::<std::sync::Arc<Code>>() + size_of::<Code>() + 2 * size_of::<usize>();
    let required = (code + 4 + 6 * size_of::<std::sync::Arc<str>>() + 7 + 5) as u64;
    for allowance in [required - 1, required, required + 17] {
        let mut budget = allowance;
        let decoded = Program::decode_bounded(&encoded, &mut budget);
        if allowance < required {
            assert_eq!(decoded, Err(DecodeError::TooLarge));
            assert_eq!(budget, allowance);
        } else {
            assert_eq!(decoded.expect("fits"), program);
            assert_eq!(budget, allowance - required);
        }
    }
    let mut budget = required;
    assert_eq!(
        Program::decode_bounded(&encoded[..encoded.len() - 1], &mut budget),
        Err(DecodeError::Invalid)
    );
    assert_eq!(budget, required);
}

#[test]
fn code_loading_leaves_room_to_verify_its_snapshot() {
    use mm_compile::bytecode::{Code, Program, op};
    use mm_engine::{Vm, VmConfig};

    let mut vm = Vm::new(VmConfig::default()).expect("vm");
    let used = vm.heap.memory_limit() - vm.heap.available_bytes();
    let mut bytecode = vec![u32::from(op::NOP); 1 << 20];
    *bytecode.last_mut().expect("nonempty") = u32::from(op::RETURN);
    let program = Program {
        codes: vec![
            Code {
                bytecode,
                is_module: true,
                ..Code::default()
            }
            .into(),
        ],
        ..Program::default()
    };
    // Enough for the code (4 MiB of words) and its tables, but not even a
    // one-bit-per-word verifier workspace. Loading must leave that room for
    // a later restore.
    let tight = used + (4 << 20) + (64 << 10);
    vm.heap.set_memory_limit(tight);
    assert!(vm.load(&program, "", "").is_err());
    assert_eq!(vm.exception_parts(vm.exception()).0, "MemoryError");
    vm.heap.set_memory_limit(tight + (128 << 10));
    vm.load(&program, "", "").expect("room for verification");
    // Do not rely on spare capacity: restore must use the reserved workspace.
    let limit = vm.heap.memory_limit() - vm.heap.available_bytes();
    vm.heap.set_memory_limit(limit);
    let mut snapshot = Vec::new();
    vm.write_snapshot(&mut snapshot).expect("snapshot");
    let mut reader = mm_core::Reader::new(&snapshot);
    Vm::read_snapshot(&mut reader, &[], None, &mut |_, _, _, _| None, limit)
        .expect("same-budget restore");
}

#[test]
fn bundled_code_loading_reserves_decoder_workspace_for_restore() {
    use mm_compile::bytecode::{Code, Program, op};
    use mm_engine::{Exit, StdModule, Vm, VmConfig};

    let modules = Box::leak(
        // Words: 16 KiB and 12 KiB of bytecode.
        [("costly", 4 << 10), ("smaller", 3 << 10)]
            .into_iter()
            .map(|(name, n)| {
                let mut bytecode = vec![u32::from(op::NOP); n];
                bytecode[n - 1] = u32::from(op::RETURN);
                let program = Program {
                    codes: vec![
                        Code {
                            bytecode,
                            is_module: true,
                            ..Code::default()
                        }
                        .into(),
                    ],
                    ..Program::default()
                };
                StdModule {
                    name,
                    code: Box::leak(mm_compile::pack::pack(&program.encode()).into_boxed_slice()),
                    packing: None,
                }
            })
            .collect::<Vec<_>>()
            .into_boxed_slice(),
    );
    let import = mm_compile::compile(
        &mm_parse::parse("import costly\nimport smaller").expect("parse"),
        mm_compile::Options::default(),
    )
    .expect("compile");
    let mut vm = Vm::new(VmConfig {
        stdlib: modules,
        ..VmConfig::default()
    })
    .expect("vm");
    let entry = vm.load(&import, "", "").expect("load import");
    let used = vm.heap.memory_limit() - vm.heap.available_bytes();
    // The retained bytecode fits, but its unpacked input and decoded Program
    // must coexist while loading. Import must account for both before copying.
    vm.heap.set_memory_limit(used + (40 << 10));
    assert_eq!(vm.run(entry), Exit::Error);
    assert_eq!(vm.exception_parts(vm.exception()).0, "MemoryError");
    vm.heap.set_memory_limit(used + (8 << 20));
    vm.prepare_cell();
    let entry = vm.load(&import, "", "").expect("retry import");
    assert!(matches!(vm.run(entry), Exit::Complete(_)));
    // Restore multiple modules at an exhausted budget. Their decoder workspace
    // is reusable: reserving the maximum must suffice, not their sum.
    let limit = vm.heap.memory_limit() - vm.heap.available_bytes();
    vm.heap.set_memory_limit(limit);
    let mut snapshot = Vec::new();
    vm.write_snapshot(&mut snapshot).expect("snapshot");
    let mut restored = Vm::read_snapshot(
        &mut mm_core::Reader::new(&snapshot),
        modules,
        None,
        &mut |_, _, _, _| None,
        limit,
    )
    .expect("same-budget restore");
    let mut again = Vec::new();
    restored.write_snapshot(&mut again).expect("snapshot again");
    Vm::read_snapshot(
        &mut mm_core::Reader::new(&again),
        modules,
        None,
        &mut |_, _, _, _| None,
        limit,
    )
    .expect("second restore");
}

#[test]
fn instruction_boundaries_reject_operands_padding_and_malformed_tails() {
    use mm_compile::bytecode::{Code, op};

    let w = u32::from;
    let mut code = Code {
        bytecode: [
            vec![w(op::JUMP) | 16 << 8],
            vec![w(op::NOP); 6],
            // MAKE_FUNCTION u0 b0, then r0 in its second word.
            vec![w(op::MAKE_FUNCTION), 0],
            vec![w(op::NOP); 7],
            vec![w(op::RETURN)],
        ]
        .concat(),
        ..Code::default()
    };
    // The two-word instruction at 7 crosses a bitmap-byte boundary; the
    // last instruction begins a partially filled byte. Neither operand
    // words nor padding bits count.
    let starts = [0, 1, 2, 3, 4, 5, 6, 7, 9, 10, 11, 12, 13, 14, 15, 16];
    for pc in (0u32..24).chain([0xff_ffff]) {
        assert_eq!(code.is_instr_start(pc as usize), starts.contains(&pc));
        code.bytecode[0] = w(op::JUMP) | pc << 8;
        assert_eq!(code.verify(), pc > 0 && starts.contains(&pc), "jump {pc}");
    }
    code.bytecode[0] = w(op::JUMP) | 16 << 8;
    assert!(code.verify());
    // Bits that hold no operand must be zero.
    for (at, bits) in [(1, 1 << 8), (8, 1 << 8), (8, 1 << 31)] {
        code.bytecode[at] |= bits;
        assert!(!code.verify(), "padding {bits:#x} at {at}");
        code.bytecode[at] &= !bits;
    }
    assert!(code.verify());
    for tail in [w(u8::MAX), w(op::MAKE_FUNCTION)] {
        code.bytecode.push(tail);
        assert!(!code.is_instr_start(0), "malformed tail {tail}");
        assert!(!code.verify());
        code.bytecode.pop();
    }
}

#[test]
fn verified_bytecode_rejects_wrong_container_types() {
    use mm_compile::bytecode::{Code, Program, op};
    use mm_engine::{Exit, Vm, VmConfig};

    // The operand verifier cannot infer types. ((1,), (2,)) has two
    // references, but treating them as a set's keys/hashes never finds
    // an empty slot. Treating the second reference as a list's length
    // can corrupt its counts instead. A dict insert into another object
    // read its slots as a dict table that never got an index (libFuzzer
    // vm_bytecode F4: every store then scanned all entries unmetered).
    // The deref ops read or wrote slot 0 of any object (the 0.0.1 soak's
    // vm_bytecode crash: a list's buffer replaced by an int), and a
    // closure that is not a tuple of cells fed any object's slots to the
    // frame's free variables.
    // All must fail before mutation.
    for mutation in [
        &[op::SET_ADD, 2][..],
        &[op::LIST_APPEND, 2],
        &[op::DICT_INSERT, 2, 0],
        &[op::STA_DEREF, 2],
        &[op::DEL_DEREF, 2],
        &[op::LDA_DEREF, 2],
        &[op::MAKE_FUNCTION, 0, 0, 4, 2],
    ] {
        let code = Code {
            bytecode: mm_compile::bytecode::assemble(
                &[
                    &[op::LDA_SMI, 1, 0][..],
                    &[op::STAR, 0],
                    &[op::BUILD_TUPLE, 0, 1, 0],
                    &[op::STAR, 0],
                    &[op::LDA_SMI, 2, 0],
                    &[op::STAR, 1],
                    &[op::BUILD_TUPLE, 1, 1, 0],
                    &[op::STAR, 1],
                    &[op::BUILD_TUPLE, 0, 2, 0],
                    &[op::STAR, 2],
                    &[op::LDA_SMI, 3, 0],
                    mutation,
                    &[op::RETURN],
                ]
                .concat(),
            )
            .expect("assembles")
            .0,
            nregs: 3,
            is_module: true,
            ..Code::default()
        };
        assert!(code.verify());
        let program = Program {
            codes: vec![code.into()],
            entry: 0,
            returns_last_expr: false,
            warnings: vec![],
        };
        let mut vm = Vm::new(VmConfig {
            fuel: Some(100),
            ..VmConfig::default()
        })
        .expect("vm");
        let entry = vm.load(&program, "bad-set.py", "").expect("loads");
        assert!(matches!(vm.run(entry), Exit::Error));
        assert_eq!(vm.exception_parts(vm.exception()).0, "SystemError");
    }
}

#[test]
fn fused_opcodes_verify_only_before_their_partner() {
    use mm_compile::bytecode::{Code, FUSED, op};

    // A fused handler reads its partner's operands from the next
    // instruction, so a restored image must keep the pair together.
    for &(_, second, fused) in &FUSED {
        for (next, ok) in [(second, true), (op::NOP, false)] {
            let instr = |o: u8| {
                let mut words = vec![0u32; mm_compile::bytecode::instr_len(o)];
                words[0] = u32::from(o);
                words
            };
            let mut bytecode = instr(fused);
            let mut partner = instr(next);
            // Forward jump targets of the pair point at the final RETURN.
            let end = u32::try_from(bytecode.len() + partner.len()).unwrap();
            for code_part in [&mut bytecode, &mut partner] {
                let o = u8::try_from(code_part[0]).unwrap();
                let fmt = mm_compile::bytecode::FORMATS[mm_compile::bytecode::unfused(o) as usize];
                if let Some(k) = fmt.find('j') {
                    let f = mm_compile::bytecode::layout_of(o).fields[k];
                    code_part[f.word as usize] |= end << f.shift;
                }
            }
            bytecode.extend(partner);
            bytecode.push(u32::from(op::RETURN));
            let code = Code {
                bytecode,
                // `FOR_RANGE` reads three registers.
                nregs: 3,
                n_ics: 1,
                names: vec!["x".into()],
                is_module: true,
                ..Code::default()
            };
            assert_eq!(code.verify(), ok, "fused {fused} before {next}");
        }
    }
}
