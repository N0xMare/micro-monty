//! The snapshot reader's consistency checks, each shown refusing an image
//! whose one field was made inconsistent before the image was written: a
//! real session is run to a stop, the field is corrupted in the decoded
//! state, and the writer encodes it as it encodes any state. Unlike byte
//! edits at fixed offsets, these keep finding their field when the
//! compiler, the prelude or the heap layout changes.

#![allow(clippy::expect_used, clippy::panic, reason = "tests")]

use super::*;
use crate::vm::{Exit, GSLOT_PRELUDE, StdModule, VmConfig};
use alloc::boxed::Box;

/// One bundled module, `bundled`, of a few codes.
fn bundled() -> &'static [StdModule] {
    let src = "def f():\n    return 1\ndef g():\n    return 2\nx = f() + g()\n";
    let program = mm_compile::compile(
        &mm_parse::parse(src).expect("parses"),
        mm_compile::Options::default(),
    )
    .expect("compiles");
    Box::leak(Box::new([StdModule {
        name: "bundled",
        code: Box::leak(mm_compile::pack::pack(&program.encode()).into_boxed_slice()),
        packing: None,
    }]))
}

/// A session that ran `src` in fuel slices of one unit until `stop` held
/// at a park (or, with `stop` never true, until it completed).
fn session(src: &str, stop: impl Fn(&Vm) -> bool) -> Vm {
    session_in(src, 1, stop)
}

/// [`session`] in fuel slices of `slice` units.
fn session_in(src: &str, slice: u64, stop: impl Fn(&Vm) -> bool) -> Vm {
    let module = mm_parse::parse(src).expect("parses");
    let prog = mm_compile::compile(
        &module,
        mm_compile::Options {
            last_expr_value: false,
        },
    )
    .expect("compiles");
    let mut vm = Vm::new(VmConfig {
        stdlib: bundled(),
        ..VmConfig::default()
    })
    .expect("vm");
    let entry = vm.load(&prog, "crafted.py", src).expect("loads");
    vm.set_fuel(Some(slice));
    let mut exit = vm.run(entry);
    loop {
        match exit {
            Exit::Parked(_) if stop(&vm) => return vm,
            Exit::Parked(_) => {
                vm.set_fuel(Some(slice));
                exit = vm.resume();
            }
            Exit::Complete(_) => return vm,
            other => panic!(
                "{src:?}: {other:?} {:?}",
                vm.exception_parts(vm.exception())
            ),
        }
    }
}

fn restore(vm: &mut Vm) -> Res<Vm> {
    let mut image = Vec::new();
    vm.write_snapshot(&mut image).expect("at a safepoint");
    let mut no_host_code =
        |_: &str, _: &HostValue, _: &[String], _: &[u8; 16]| -> Option<Arc<crate::HostProgram>> {
            None
        };
    Vm::read_snapshot(
        &mut Reader::new(&image),
        vm.stdlib,
        None,
        &mut no_host_code,
        1 << 40,
    )
}

/// `src`'s session at the first park where `stop` holds restores as it is,
/// and is refused as `what` once `corrupt` has changed it.
fn refuses(src: &str, stop: impl Fn(&Vm) -> bool, what: &'static str, corrupt: impl Fn(&mut Vm)) {
    let mut vm = session(src, &stop);
    assert!(restore(&mut vm).is_ok(), "the session itself restores");
    let mut vm = session(src, &stop);
    corrupt(&mut vm);
    assert_eq!(restore(&mut vm).err(), Some(SnapError::Corrupt(what)));
}

/// Parked inside a user `__next__` under star-unpacking: the module
/// frame holds a continuation.
const UNPACKING: &str = "\
class It:
    def __init__(self):
        self.i = 0
    def __iter__(self):
        return self
    def __next__(self):
        self.i += 1
        if self.i > 3:
            raise StopIteration
        return self.i
a, *b = It()
";

fn holds_cont(vm: &Vm) -> bool {
    vm.conts.iter().any(Option::is_some)
}

fn set_cont(vm: &mut Vm, c: Cont) {
    let slot = vm
        .conts
        .iter_mut()
        .find(|c| c.is_some())
        .expect("a live continuation");
    *slot = Some(c);
}

#[test]
fn a_continuation_must_land_inside_its_frame() {
    let big = 1 << 20;
    for (c, what) in [
        // A register past the frame's.
        (Cont::StoreReg(u8::MAX), "frame"),
        (Cont::Unpack { r: 0, n: 1_000 }, "frame"),
        // A jump target that is not an instruction of the frame's code.
        (
            Cont::Truth {
                mode: 0,
                target: big,
                len: false,
                orig: Value::NONE,
            },
            "frame",
        ),
        (Cont::ForIter(big), "frame"),
        (Cont::Send(big), "frame"),
        (Cont::Delegate { exit: 0, at: big }, "frame"),
        (Cont::Delegate { exit: big, at: 0 }, "frame"),
        (
            Cont::CallIter {
                it: Value::NONE,
                exit: big,
            },
            "frame",
        ),
        // A native step the engine does not have.
        (
            Cont::Native {
                op: u8::try_from(crate::nstep::STEPS.len()).expect("few steps"),
                state: Value::NONE,
            },
            "continuation",
        ),
    ] {
        refuses(UNPACKING, holds_cont, what, |vm| set_cont(vm, c));
    }
}

#[test]
fn the_stopped_frame_must_fit_its_stack() {
    refuses(UNPACKING, holds_cont, "state", |vm| {
        let base = vm.saved.as_ref().expect("stopped").base;
        vm.stack.truncate(base + 1);
    });
}

#[test]
fn a_suspended_generator_resumes_at_a_yield() {
    let src = "\
def g():
    yield 1
    yield 2
x = g()
next(x)
while True:
    pass
";
    let suspended = |vm: &Vm| {
        let mut any = false;
        vm.heap.for_each_object(&mut |r, h| {
            any |= h.type_id() == ty::GEN
                && vm.heap.get_raw(r, crate::generator::g::STATE)
                    == Ok(crate::generator::g::SUSPENDED);
        });
        any
    };
    refuses(src, suspended, "generator", |vm| {
        let mut gens = Vec::new();
        vm.heap.for_each_object(&mut |r, h| {
            if h.type_id() == ty::GEN {
                gens.push(r);
            }
        });
        for r in gens {
            // Instruction 0 starts the code but is not a yield.
            vm.heap
                .set_raw(r, crate::generator::g::YPC, 0)
                .expect("slot");
        }
    });
}

#[test]
fn a_suspended_coroutine_resumes_at_its_await() {
    let src = "\
class Aw:
    def __await__(self):
        yield 1
        return 2
async def co():
    return await Aw()
c = co()
c.send(None)
while True:
    pass
";
    let suspended = |vm: &Vm| {
        let mut any = false;
        vm.heap.for_each_object(&mut |r, h| {
            any |= h.type_id() == ty::CORO
                && vm.heap.get_raw(r, crate::aio::coro::STATE).map(|s| s & 3)
                    == Ok(crate::aio::coro::SUSPENDED);
        });
        any
    };
    refuses(src, suspended, "coroutine", |vm| {
        let mut coros = Vec::new();
        vm.heap.for_each_object(&mut |r, h| {
            if h.type_id() == ty::CORO {
                coros.push(r);
            }
        });
        for r in coros {
            vm.heap.set_raw(r, crate::aio::coro::PC, 0).expect("slot");
        }
    });
}

#[test]
fn a_reference_names_an_object_of_its_kind() {
    refuses(
        "xs = ['a' * 40, ('b',)]\n",
        |_| false,
        "references",
        |vm| {
            // A tuple's first slot that refers to an object, retagged as an
            // object of another kind.
            let mut found = None;
            vm.heap.for_each_object(&mut |r, h| {
                if found.is_some() || h.type_id() != ty::TUPLE {
                    return;
                }
                let Ok(w) = vm.heap.slot_words(r) else {
                    return;
                };
                let n = (h.nvals() as usize).min(w.len());
                found = w[..n].iter().enumerate().find_map(|(i, &b)| {
                    let v = Value::from_bits(b)?;
                    let (o, kind) = (v.as_obj()?, v.obj_kind()?);
                    let other = if kind == ty::TUPLE as u8 {
                        ty::LIST
                    } else {
                        ty::TUPLE
                    };
                    Some((r, i, Value::from_ref(o, other as u8)))
                });
            });
            let (r, i, v) = found.expect("a reference");
            vm.heap
                .set(r, u32::try_from(i).expect("slot"), v)
                .expect("slot");
        },
    );
}

#[test]
fn a_shape_transition_leads_to_a_later_shape() {
    refuses(
        "class A:\n    pass\na = A()\na.x = 1\na.y = 2\n",
        |_| false,
        "shapes",
        |vm| {
            let (i, s) = vm
                .shapes
                .iter_mut()
                .enumerate()
                .find(|(_, s)| !s.trans.is_empty())
                .expect("a transition");
            s.trans[0].1 = u32::try_from(i).expect("index");
        },
    );
}

#[test]
fn the_tick_registers_stay_in_range() {
    refuses(
        "x = 1\n",
        |_| false,
        "registers",
        |vm| {
            vm.tick_period = 0;
            vm.ticks = 0;
        },
    );
}

/// The user's module code, made uniquely owned to be changed.
fn user_code(vm: &mut Vm) -> &mut CodeObj {
    let i = vm
        .codes
        .iter()
        .rposition(|c| !c.prelude)
        .expect("user code");
    let codes = Arc::get_mut(&mut vm.codes).expect("uniquely owned");
    Arc::get_mut(&mut codes[i]).expect("uniquely owned")
}

#[test]
fn stored_code_is_verified_and_runs_in_its_own_globals() {
    refuses(
        "x = 1\n",
        |_| false,
        "code verification",
        |vm| {
            let code = Arc::get_mut(&mut user_code(vm).code).expect("uniquely owned");
            // An unknown opcode.
            code.bytecode[0] = u32::MAX;
        },
    );
    refuses(
        "x = 1\n",
        |_| false,
        "code",
        |vm| {
            user_code(vm).gslot = GSLOT_PRELUDE;
        },
    );
}

#[test]
fn a_dict_index_reaches_every_entry() {
    refuses(
        "d = {'a': 1, 'b': 2}\n",
        |_| false,
        "dict index",
        |vm| {
            // Empty every index slot of the dicts with entries: lookups would
            // no longer find them.
            let mut indexes = Vec::new();
            vm.heap.for_each_object(&mut |r, h| {
                if h.type_id() != ty::DICT {
                    return;
                }
                let index = vm
                    .heap
                    .get(r, 1)
                    .ok()
                    .and_then(Value::as_obj)
                    .filter(|_| vm.heap.get(r, 0).is_ok_and(|e| e.as_obj().is_some()));
                if let (Some(i), Ok(w)) = (
                    index,
                    index.map_or(Err(mm_core::Error::BadRef), |i| vm.heap.raw_words(i)),
                ) {
                    indexes.push((i, w.len() / 2));
                }
            });
            for (i, cap) in indexes {
                for k in 0..cap {
                    vm.heap
                        .set_raw(i, u32::try_from(k).expect("slot"), 0)
                        .expect("slot");
                }
            }
        },
    );
}

#[test]
fn a_restored_class_size_hint_cannot_size_an_instance() {
    // Honest hints never exceed the inline slots; a restored one may say
    // anything, and instances of the class stay small.
    let mut vm = session("class A:\n    pass\na = A()\n", |_| false);
    let mut classes = Vec::new();
    vm.heap.for_each_object(&mut |r, h| {
        if h.type_id() == ty::INST
            && let Some(c) = vm
                .heap
                .get(r, crate::class::inst::CLASS)
                .ok()
                .and_then(Value::as_obj)
            && vm
                .heap
                .object_words(c)
                .is_ok_and(|(h, _)| h.type_id() == ty::CLASS)
        {
            classes.push(c);
        }
    });
    assert_ne!(classes.len(), 0);
    for c in classes {
        vm.heap
            .set_raw(c, crate::class::cls::HINT, 1 << 33)
            .expect("slot");
    }
    let mut vm = restore(&mut vm).expect("restores");
    let src = "b = [A() for _ in range(3)]\n";
    let module = mm_parse::parse(src).expect("parses");
    let prog = mm_compile::compile(
        &module,
        mm_compile::Options {
            last_expr_value: false,
        },
    )
    .expect("compiles");
    let entry = vm.load(&prog, "after.py", src).expect("loads");
    vm.set_fuel(None);
    let exit = vm.run(entry);
    assert!(matches!(exit, Exit::Complete(_)), "{exit:?}");
    vm.heap.for_each_object(&mut |_, h| {
        if h.type_id() == ty::INST {
            assert!(u64::from(h.nvals()) <= u64::from(crate::class::inst::FIRST) + 32);
        }
    });
}

#[test]
fn a_bundled_module_enters_inside_its_codes() {
    fn range(vm: &mut Vm) -> &mut (u32, u32, u32) {
        vm.std_codes
            .iter_mut()
            .flatten()
            .next()
            .expect("the module was loaded")
    }
    let src = "import bundled\n";
    // Before its first code and at its end.
    refuses(
        src,
        |_| false,
        "bundled modules",
        |vm| {
            let (first, _, entry) = range(vm);
            *entry = first.checked_sub(1).expect("prelude codes come first");
        },
    );
    refuses(
        src,
        |_| false,
        "bundled modules",
        |vm| {
            let (_, end, entry) = range(vm);
            *entry = *end;
        },
    );
}

#[cfg(feature = "asyncio")]
#[test]
fn a_task_index_names_a_task() {
    let src = "\
import asyncio
async def spin():
    i = 0
    while True:
        i += 1
        await asyncio.sleep(0)
async def main():
    await asyncio.gather(spin(), spin())
asyncio.run(main())
";
    let running = |vm: &Vm| vm.tasks.iter().flatten().count() >= 2 && !vm.blocked.is_empty();
    // The current task, then a blocked one, each the only bad index.
    refuses(src, running, "tasks", |vm| vm.cur = vm.tasks.len());
    refuses(src, running, "tasks", |vm| vm.blocked[0] = vm.tasks.len());
}

/// Parked inside chunked op `op` with its state's word `i` (value slots,
/// then raw words) set to `x`, `src`'s restored session raises the bad
/// continuation `SystemError` on its next step instead of parking for ever
/// without progress.
fn a_forged_step_raises(src: &str, op: u8, i: u32, x: u64) {
    a_forged_state_raises(src, op, |_, _| true, i, x);
}

/// [`a_forged_step_raises`], parked once a state of `op` passes `at`.
fn a_forged_state_raises(
    src: &str,
    op: u8,
    at: impl Fn(&Vm, mm_core::Ref) -> bool,
    i: u32,
    x: u64,
) {
    let (name, msg) = resume_forged(src, op, at, i, x).expect("raises");
    assert_eq!(name, "SystemError", "{msg}");
}

/// `src` parked once a state of `op` passes `at`, its word `i` (value
/// slots, then raw words) set to `x`, restored and resumed: the exception
/// it raises (`"refused"` if the restore refuses the image), or `None`
/// when it completes. It never panics, overflow checks on (the debug and
/// fuzz builds) or off.
fn resume_forged(
    src: &str,
    op: u8,
    at: impl Fn(&Vm, mm_core::Ref) -> bool,
    i: u32,
    x: u64,
) -> Option<(String, String)> {
    let find = |vm: &Vm| {
        let mut found = None;
        vm.heap.for_each_object(&mut |r, h| {
            if found.is_none()
                && h.type_id() == ty::NSTATE
                && vm.heap.get_raw(r, h.nvals()) == Ok(u64::from(op))
                && at(vm, r)
            {
                found = Some(r);
            }
        });
        found
    };
    let mut vm = session(src, |vm| find(vm).is_some());
    let r = find(&vm).unwrap_or_else(|| panic!("{src:?}: never parked inside op {op}"));
    let vals = vm.heap.header(r).expect("header").nvals();
    let set = match Value::from_bits(x) {
        Some(v) if i < vals => vm.heap.set(r, i, v),
        _ => vm.heap.set_raw(r, i, x),
    };
    set.expect("a word of the state");
    restore_and_finish(&mut vm)
}

/// `vm` snapshotted, restored and resumed: as [`resume_forged`].
fn restore_and_finish(vm: &mut Vm) -> Option<(String, String)> {
    // An image the restore check refuses (`check_steps`).
    let Ok(mut vm) = restore(vm) else {
        return Some(("refused".into(), String::new()));
    };
    for _ in 0..64 {
        vm.set_fuel(Some(1 << 20));
        match vm.resume() {
            Exit::Parked(_) => {}
            Exit::Complete(_) => return None,
            Exit::Error => {
                let exc = vm.exception();
                let (name, msg) = vm.exception_parts(exc);
                return Some((name, msg));
            }
            other => panic!("{other:?}"),
        }
    }
    panic!("still parked without an error");
}

#[test]
fn a_forged_search_or_digit_word_never_panics() {
    use crate::bytes::{S_MEM, S_PH, W_S, job};
    use crate::nstep::op;
    // A long needle's two-way search (phase 3) with a memory past the
    // needle (`pos + mem`).
    let src = "s = 'é' * 3_000_000\nn = 'é' * 300_000\nx = s.find(n + 'z')\n";
    let find = |vm: &Vm, r| {
        let n = vm.heap.header(r).expect("header").nvals();
        let phase = n + 2 + u32::try_from(W_S + S_PH).expect("small");
        raw(vm, r, n + 1) == u64::from(job::FIND) && raw(vm, r, phase) == 3
    };
    let vals = crate::nstep::STEPS[op::BYTES as usize].1;
    let mem = vals + 2 + u32::try_from(W_S + S_MEM).expect("small");
    resume_forged(src, op::BYTES, find, mem, u64::MAX - 1);
    // A power-of-two digit text with separators whose int bytes read
    // (`done`) went back: more bits than digits left (`acc |= b << held`).
    let digits = |vm: &Vm, r| {
        let n = vm.heap.header(r).expect("header").nvals();
        raw(vm, r, n + 1) == crate::builtins::DIGITS
    };
    let src = "t = format(1 << 6_000_000, '_b')\n";
    resume_forged(src, op::NUM_TEXT, digits, 2 + 3, 3);
}

#[test]
fn a_forged_pending_bulk_table_raises() {
    // A set grows past `SPLIT` slots: its new table comes at the next
    // step's start (new keys `NONE`, `j` its size). A size whose bytes
    // overflow, sizes too small for the set (a full table probed for
    // ever), then sizes that are not a power of two.
    let src = "s = set(range(3_000_000))\n";
    let pending = |vm: &Vm, r| vm.heap.get(r, 6) == Ok(Value::NONE);
    let bulk = crate::nstep::op::BULK;
    for j in [1 << 62, 1, 7, 16, 131_073, 3 << 22] {
        a_forged_state_raises(src, bulk, pending, 8 + 4, j);
    }
    // A table being rebuilt (new keys a buffer) whose keys are forged
    // away: `j`, the rebuild's progress, read as the size to grow to,
    // either fits the set or is refused; the set never hangs.
    let rebuilding = |vm: &Vm, r| {
        vm.heap
            .get(r, 6)
            .is_ok_and(|k| k.is_obj_kind(ty::BUF as u8))
    };
    resume_forged(src, bulk, rebuilding, 6, Value::NONE.to_bits());
    // A dict's pending size is how many items it holds next: one at or
    // below its length reserves nothing, so it only has to be a count.
    let src = "d = dict.fromkeys(range(3_000_000))\ne = dict(d)\n";
    for j in [0, 1, 7, 1 << 62] {
        resume_forged(src, bulk, pending, 8 + 4, j);
    }
}

#[test]
fn a_live_state_forged_finished_raises() {
    use crate::nstep::{STEPS, op};
    // A finished state is marked `DONE`, which restore does not check and
    // no step runs: one forged on a live state of any chunked op raises
    // the bad continuation `SystemError` at its next step, before any of
    // its words are read (`MATH_FOLD`, json's and unicodedata's ops need
    // their modules: `tests/snapshots.rs`).
    let mut cases: Vec<(u8, &str)> = STEPPED.to_vec();
    cases.extend([
        (op::SEQ_IN, "a = [0] * 5_000_000\nx = 1 in a\n"),
        (op::BULK, "s = set(range(3_000_000))\n"),
        (op::STR_SCAN, "S = 'ab' * 9_000_000\nx = S.count('ab')\n"),
        (op::NUM_TEXT, "x = int('f' * 12_000_000, 16)\n"),
        (op::NUM_TEXT, "t = bin(1 << 5_000_000)\n"),
        (
            op::STR_PCT,
            "x = ('%s,' * 1_000_000) % tuple(range(1_000_000))\n",
        ),
        (op::FOLD, "a = [1] * 5_000_000\nx = sum(iter(a))\n"),
        (
            op::DV_IN,
            "d = dict.fromkeys(range(1_000_000), 0)\nx = 1 in d.values()\n",
        ),
    ]);
    for (o, src) in cases {
        let done = u64::from(op::DONE);
        a_forged_state_raises(src, o, |_, _| true, STEPS[o as usize].1, done);
    }
    // A sort's state keeps no operation word (its first raw word is the
    // order, which `DONE` would only reverse): a list to put its items
    // back in that is not one, with that word forged past the sort's
    // codes so restore checks none of it, raises at the sort's step.
    let src = "a = list(range(300_000, 0, -1))\na.sort(key=abs)\n";
    let mut vm = session(src, |vm| step_state(vm, op::SORT_KEYS).is_some());
    let r = step_state(&vm, op::SORT_KEYS).expect("a sort");
    vm.heap.set(r, 3, Value::TRUE).expect("slot");
    vm.heap.set_raw(r, 4, u64::from(op::DONE)).expect("word");
    let mut vm = restore(&mut vm).expect("restores");
    let raised = loop {
        vm.set_fuel(Some(1 << 20));
        match vm.resume() {
            Exit::Parked(_) => {}
            Exit::Error => break vm.exception_parts(vm.exception()).0,
            other => panic!("{other:?}"),
        }
    };
    assert_eq!(raised, "SystemError");
}

#[test]
fn a_sort_puts_its_items_back_into_a_list() {
    // `list.sort`'s list (value slot 3) gets the sorted buffer and length
    // written into it: anything but a list or `None` is refused.
    let src = "a = list(range(300_000, 0, -1))\na.sort(key=abs)\n";
    let sorting = |vm: &Vm| step_state(vm, crate::nstep::op::SORT_KEYS).is_some();
    refuses(src, sorting, "native step", |vm| {
        let r = step_state(vm, crate::nstep::op::SORT_KEYS).expect("state");
        vm.heap.set(r, 3, Value::TRUE).expect("slot");
    });
}

#[test]
fn a_forged_str_scan_raises_instead_of_parking_for_ever() {
    let count = "S = 'ab' * 9_000_000\nx = S.count('ab')\n";
    let find = "S = 'a' * 20_000_000\nx = S.find('ab')\n";
    let scan = crate::nstep::op::STR_SCAN;
    // The window's end past the text, then the text not a str.
    a_forged_step_raises(count, scan, 6, 18_000_001);
    a_forged_step_raises(count, scan, 0, Value::NONE.to_bits());
    a_forged_step_raises(find, scan, 6, 20_000_001);
    a_forged_step_raises(find, scan, 0, Value::NONE.to_bits());
}

#[test]
fn a_forged_number_text_raises_instead_of_parking_for_ever() {
    let src = "x = int('f' * 12_000_000, 16)\n";
    a_forged_step_raises(src, crate::nstep::op::NUM_TEXT, 0, Value::NONE.to_bits());
}

/// The objects of type `kind`.
fn objects(vm: &Vm, kind: u16) -> Vec<mm_core::Ref> {
    let mut out = Vec::new();
    vm.heap.for_each_object(&mut |r, h| {
        if h.type_id() == kind {
            out.push(r);
        }
    });
    out
}

fn raw(vm: &Vm, r: mm_core::Ref, i: u32) -> u64 {
    vm.heap.get_raw(r, i).expect("slot")
}

/// A function whose code is a generator's (or with `coroutine`, a
/// coroutine's) other than code `not`.
fn other_function(vm: &Vm, coroutine: bool, not: u64) -> Value {
    let f = objects(vm, ty::FUNC)
        .into_iter()
        .find(|&f| {
            let ci = raw(vm, f, crate::vm::fn_slot::CODE);
            let code = &vm.codes[usize::try_from(ci).expect("index")].code;
            ci != not
                && if coroutine {
                    code.is_coroutine
                } else {
                    code.is_generator
                }
        })
        .expect("another function");
    Value::from_ref(f, ty::FUNC as u8)
}

fn gen_in(vm: &Vm, state: u64) -> Option<mm_core::Ref> {
    use crate::generator::g;
    objects(vm, ty::GEN)
        .into_iter()
        .find(|&r| raw(vm, r, g::STATE) == state)
}

#[test]
fn a_generator_runs_its_functions_code_in_every_state() {
    use crate::generator::g;
    // `other` has `g`'s shape: its first yield is where `g`'s is.
    let src = "\
def g():
    yield 1
    yield 2
def other():
    yield 3
    yield 4
a = g()
b = g()
next(b)
c = g()
list(c)
other()
while True:
    pass
";
    let all = |vm: &Vm| {
        [g::CREATED, g::SUSPENDED, g::DONE]
            .iter()
            .all(|&s| gen_in(vm, s).is_some())
    };
    for state in [g::CREATED, g::SUSPENDED, g::DONE] {
        refuses(src, all, "generator", |vm| {
            let r = gen_in(vm, state).expect("a generator");
            let mine = raw(vm, r, g::CODE);
            let f = other_function(vm, false, mine);
            let ci = raw(vm, f.as_obj().expect("ref"), crate::vm::fn_slot::CODE);
            vm.heap.set_raw(r, g::CODE, ci).expect("slot");
        });
    }
}

/// The fuzz pre-run's crash (`snapshot_restore-crash-gen-code-*`): a
/// running generator whose code is not its frame's saved its frame's pc
/// at the next yield and resumed it in the other code.
#[test]
fn a_running_generators_frame_runs_its_code() {
    use crate::generator::g;
    let src = "\
def g():
    i = 0
    while True:
        i += 1
        yield i
def other():
    yield 0
other()
for x in g():
    if x > 1000:
        break
";
    let running = |vm: &Vm| gen_in(vm, g::RUNNING).is_some();
    // Its code, then its state.
    refuses(src, running, "frame owner", |vm| {
        let r = gen_in(vm, g::RUNNING).expect("running");
        let f = other_function(vm, false, raw(vm, r, g::CODE));
        let ci = raw(vm, f.as_obj().expect("ref"), crate::vm::fn_slot::CODE);
        vm.heap.set_raw(r, g::CODE, ci).expect("slot");
    });
    refuses(src, running, "frame owner", |vm| {
        let r = gen_in(vm, g::RUNNING).expect("running");
        vm.heap.set_raw(r, g::STATE, g::DONE).expect("slot");
    });
}

#[test]
fn a_running_coroutines_frame_runs_its_functions_code() {
    use crate::aio::coro;
    let src = "\
class Aw:
    def __await__(self):
        yield 1
        return 2
async def co():
    i = 0
    while i < 1000:
        i += 1
    return await Aw()
async def other():
    return 0
c = co()
c.send(None)
";
    let running = |vm: &Vm| {
        objects(vm, ty::CORO)
            .iter()
            .any(|&r| raw(vm, r, coro::STATE) & 3 == coro::RUNNING)
    };
    refuses(src, running, "frame owner", |vm| {
        let r = objects(vm, ty::CORO)[0];
        let mine = vm.heap.get(r, coro::FUNC).expect("slot");
        let ci = raw(vm, mine.as_obj().expect("ref"), crate::vm::fn_slot::CODE);
        let f = other_function(vm, true, ci);
        vm.heap.set(r, coro::FUNC, f).expect("slot");
    });
}

#[test]
fn a_continuation_is_held_once_and_freed_once() {
    // Two frame records hold continuations: the module's (`Unpack`) and
    // `__next__`'s (`Check`, of what `__eq__` returns).
    let src = "\
class E:
    def __eq__(self, o):
        i = 0
        while i < 1000:
            i += 1
        return True
class It:
    def __init__(self):
        self.i = 0
    def __iter__(self):
        return self
    def __next__(self):
        self.i += 1
        if self.i > 3:
            raise StopIteration
        return E() == E()
a, *b = It()
";
    let two = |vm: &Vm| vm.specials.iter().filter(|f| f.cont != 0).count() == 2;
    // A record given the other's `Check`, which is valid in any frame.
    refuses(src, two, "frame", |vm| {
        let check = vm
            .specials
            .iter()
            .map(|f| f.cont)
            .find(|&c| c != 0 && matches!(vm.conts[c as usize - 1], Some(Cont::Check(_))))
            .expect("the result check");
        for f in vm.specials.iter_mut().filter(|f| f.cont != 0) {
            f.cont = check;
        }
    });
    // The free list naming a held slot, or an empty slot twice.
    refuses(src, two, "continuations", |vm| {
        let held = vm.specials.iter().find(|f| f.cont != 0).expect("held").cont;
        vm.cont_free.push(held - 1);
    });
    refuses(src, two, "continuations", |vm| {
        let n = u32::try_from(vm.conts.len()).expect("few");
        vm.conts.push(None);
        vm.cont_free.extend([n, n]);
    });
}

#[test]
fn a_stored_yield_from_follows_its_send() {
    use crate::generator::g;
    let src = "\
def sub():
    yield 1
def g():
    yield from sub()
x = g()
next(x)
while True:
    pass
";
    let suspended = |vm: &Vm| gen_in(vm, g::SUSPENDED).is_some();
    refuses(src, suspended, "code verification", |vm| {
        let codes = Arc::get_mut(&mut vm.codes).expect("uniquely owned");
        let c = codes.iter_mut().filter(|c| !c.prelude).find_map(|c| {
            let bc = &c.code.bytecode;
            let mut pc = 0;
            while pc < bc.len() {
                let o = mm_compile::bytecode::opcode(bc, pc);
                if o == op::SEND {
                    return Some((c, pc));
                }
                pc += mm_compile::bytecode::instr_len(o);
            }
            None
        });
        let (c, pc) = c.expect("a SEND");
        let code = Arc::get_mut(&mut Arc::get_mut(c).expect("uniquely owned").code)
            .expect("uniquely owned");
        // A `MAKE_FUNCTION` of the same length that verifies, whose second
        // operand (`throw()`'s exit) is past the code.
        code.bytecode[pc] = u32::from(op::MAKE_FUNCTION) | (0xF8 << 24);
        code.bytecode[pc + 1] = 0;
    });
}

/// The `NSTATE` of chunked native `op` in `vm`, if one is live.
fn step_state(vm: &Vm, op: u8) -> Option<mm_core::Ref> {
    let mut found = None;
    vm.heap.for_each_object(&mut |r, h| {
        if h.type_id() == ty::NSTATE
            && let Ok((h, w)) = vm.heap.object_words(r)
            && w.get(h.nvals() as usize) == Some(&u64::from(op))
            && crate::nstep::STEPS[op as usize].2 as usize + h.nvals() as usize == w.len()
        {
            found = Some(r);
        }
    });
    found
}

/// Sets raw word `i` (after the operation word: `1` is the first) of the
/// state of `op`.
fn set_step_word(vm: &mut Vm, op: u8, i: u32, x: u64) {
    let r = step_state(vm, op).expect("a state");
    let n = vm.heap.header(r).expect("header").nvals();
    vm.heap.set_raw(r, n + i, x).expect("slot");
}

fn step_word(vm: &Vm, op: u8, i: u32) -> u64 {
    let r = step_state(vm, op).expect("a state");
    let n = vm.heap.header(r).expect("header").nvals();
    raw(vm, r, n + i)
}

/// Programs that run chunked natives a step at a time, and the operation
/// each parks in.
const STEPPED: &[(u8, &str)] = {
    use crate::nstep::op;
    &[
        (
            op::SEQ_BUILD,
            "a = list(range(1_200_000))\nb = a + a\nc = a[::3]\ndel a[::2]\n",
        ),
        (
            op::SEQ_SCAN,
            "a = [1] * 3_000_000\nn = a.count(1)\ni = a.index(2) if 2 in a else 0\n",
        ),
        (
            op::STR_FILL,
            "s = 'é' * 4_300_000\nt = format(s, '>9000000')\nu = 'ab' * 4_300_000\n",
        ),
        (op::STR_JOIN, "t = ','.join(['é' * 40] * 140_000)\n"),
        (
            op::STR_REPLACE,
            "s = 'aé' * 3_000_000\nt = s.replace('a', 'xyz')\nu = s[:600_000].replace('', '-')\n",
        ),
        (
            op::STR_SPLIT,
            "s = ('abcdefgh' * 8 + ', é ') * 70_000\nx = s.split(',')\nx = s.rsplit(',')\nx = s.split()\n\
             x = s.rsplit()\nx = None\nx = (('é' * 50 + '\\n') * 90_000).splitlines()\nx = None\n\
             x = ('a' * 9_000_000 + ',b').split(',')\nx = ('b,' + 'a' * 9_000_000).rsplit(',')\n",
        ),
        (
            op::STR_MAP,
            "s = 'é' * 8_000_000\nt = s[::2]\nt = s[::-3]\nt = ('ab' * 8_000_000)[::2]\n\
             t = s[:2_000_000].upper()\nt = ('aΣ b' * 600_000).lower()\nt = repr(s[:3_000_000])\n",
        ),
        (
            op::BYTES,
            "b = b'ab' * 5_000_000\nx = (b'aaaaaaa b' * 1_000_000).split()\nx = None\n\
             x = (b'a' * 9_000_000 + b' b ').split()\nx = None\n\
             ba = bytearray(9_000_000)\ndel ba[::2]\nba = bytearray(9_000_000)\nba[5:6] = bytes(9_000_000)\n",
        ),
    ]
};

#[test]
fn every_parked_step_restores() {
    use crate::bytes::job;
    let mut kinds = Vec::new();
    for &(op, src) in STEPPED {
        let module = mm_parse::parse(src).expect("parses");
        let prog = mm_compile::compile(&module, mm_compile::Options::default()).expect("compiles");
        let mut vm = Vm::new(VmConfig {
            stdlib: bundled(),
            ..VmConfig::default()
        })
        .expect("vm");
        let entry = vm.load(&prog, "steps.py", src).expect("loads");
        vm.set_fuel(Some(1));
        let mut exit = vm.run(entry);
        let (mut parked_in, mut last) = (0, None);
        loop {
            match exit {
                Exit::Parked(_) => {
                    // Every phase's first park, then every fourth.
                    let phase = step_state(&vm, op)
                        .map(|_| (step_word(&vm, op, 1), step_word(&vm, op, 2) == 0));
                    if phase.is_some() {
                        parked_in += 1;
                        if (phase != last || parked_in % 32 == 0)
                            && let Err(e) = restore(&mut vm)
                        {
                            panic!("op {op}, park {parked_in}: {e:?}");
                        }
                        if op == crate::nstep::op::BYTES {
                            kinds.extend((0..=u8::MAX).filter(|&k| job_state(&vm, k).is_some()));
                        }
                    }
                    last = phase;
                    vm.set_fuel(Some(1));
                    exit = vm.resume();
                }
                Exit::Complete(_) => break,
                other => panic!("op {op}: {other:?} {:?}", vm.exc),
            }
        }
        assert!(
            parked_in >= 2,
            "op {op} parked in its steps {parked_in} times"
        );
    }
    for k in [job::REPEAT, job::SPLIT, job::DEL_STRIDE, job::GROW] {
        assert!(kinds.contains(&k), "job {k}: {kinds:?}");
    }
}

/// Runs `src` in fuel slices of `fuel`, restoring the session at every
/// park where `at` holds: each must restore. How many it restored.
fn restores_where(src: &str, fuel: u64, at: impl Fn(&Vm, &mut bool) -> bool) -> usize {
    let module = mm_parse::parse(src).expect("parses");
    let prog = mm_compile::compile(&module, mm_compile::Options::default()).expect("compiles");
    let mut vm = Vm::new(VmConfig {
        stdlib: bundled(),
        ..VmConfig::default()
    })
    .expect("vm");
    let entry = vm.load(&prog, "steps.py", src).expect("loads");
    let (mut n, mut seen) = (0, false);
    vm.set_fuel(Some(fuel));
    let mut exit = vm.run(entry);
    loop {
        match exit {
            Exit::Parked(_) => {
                if at(&vm, &mut seen) {
                    n += 1;
                    if let Err(e) = restore(&mut vm) {
                        panic!("{src:?} fuel {fuel}, park {n}: {e:?}");
                    }
                }
                vm.set_fuel(Some(fuel));
                exit = vm.resume();
            }
            Exit::Complete(_) => return n,
            other => panic!("{src:?}: {other:?} {:?}", vm.exc),
        }
    }
}

#[test]
fn a_finished_step_restores_at_the_park_after_it() {
    // A chunked native's last step finishes in place (a lowering's pending
    // `σ` written as `ς`, a set table filled) without saving its words;
    // its state stays live in `_steps` until the step's fuel is charged,
    // where the session may park. Every park while a lowering runs, its
    // last step starting at many offsets, with or without a `σ` pending.
    let mut parks = 0;
    for f in ["lower", "swapcase", "title", "capitalize"] {
        for pad in 0..24 {
            let src = alloc::format!("s = 'x' * {pad} + 'ΟΔΟΣ ΑΘΗΝΑΣ, ' * 30000\nt = s.{f}()\n");
            parks += restores_where(&src, 20_000, |vm, _| {
                let mut any = false;
                vm.heap
                    .for_each_object(&mut |_, h| any |= h.type_id() == ty::NSTATE);
                any
            });
        }
    }
    assert!(parks > 96, "{parks} parks");
    // Every chunked op's state, at its first park after it finished (no
    // longer a live state of its op), in slices of 1 and a prime.
    for &(op, src) in STEPPED {
        for fuel in [1, 7_919] {
            let done = restores_where(src, fuel, |vm, live| {
                let now = step_state(vm, op).is_some();
                core::mem::replace(live, now) && !now
            });
            assert!(done >= 1, "op {op} fuel {fuel}: {done} finishes");
        }
    }
}

/// `src` parked inside chunked native `op` (`STEPPED`) is refused once
/// `corrupt` has changed its state.
fn refuses_step(op: u8, corrupt: impl Fn(&mut Vm)) {
    let src = STEPPED.iter().find(|s| s.0 == op).expect("a program").1;
    refuses(
        src,
        |vm| step_state(vm, op).is_some(),
        "native step",
        corrupt,
    );
}

#[test]
fn a_parked_step_keeps_its_progress_in_range() {
    use crate::nstep::op;
    // Lists: the next item of a segment within it (`pos += 1` overflows);
    // matches counted among the items passed (`c += 1`).
    refuses_step(op::SEQ_BUILD, |vm| {
        let seg = step_word(vm, op::SEQ_BUILD, 2);
        let count = step_word(
            vm,
            op::SEQ_BUILD,
            8 + 5 * u32::try_from(seg).expect("seg") + 3,
        );
        set_step_word(vm, op::SEQ_BUILD, 3, count.max(1) + 1);
    });
    refuses_step(op::SEQ_SCAN, |vm| {
        let i = step_word(vm, op::SEQ_SCAN, 2);
        set_step_word(vm, op::SEQ_SCAN, 4, i + 1);
    });
    // Str builds: a part's bytes done were written (`o - p`), a char
    // takes a byte (`chars` adds up), a join's items counted are its own.
    refuses_step(op::STR_FILL, |vm| {
        let out = step_word(vm, op::STR_FILL, 3);
        set_step_word(vm, op::STR_FILL, 2, out + 1);
    });
    refuses_step(op::STR_FILL, |vm| {
        let out = step_word(vm, op::STR_FILL, 3);
        set_step_word(vm, op::STR_FILL, 4, out + 1);
    });
    refuses_step(op::STR_JOIN, |vm| {
        set_step_word(vm, op::STR_JOIN, 1, u64::MAX / 2);
    });
    // Replace: more matches counted than the text holds (`len - pl * n`).
    refuses_step(op::STR_REPLACE, |vm| {
        if step_word(vm, op::STR_REPLACE, 1) == 0 {
            set_step_word(vm, op::STR_REPLACE, 4, 1 << 40);
        } else {
            set_step_word(vm, op::STR_REPLACE, 6, 1 << 40);
        }
    });
    // All the matches, more than the text holds (the result's chars
    // `len + (new - old) * all` overflow).
    let building = |vm: &Vm| {
        step_state(vm, op::STR_REPLACE).is_some() && step_word(vm, op::STR_REPLACE, 1) != 0
    };
    let src = STEPPED
        .iter()
        .find(|s| s.0 == op::STR_REPLACE)
        .expect("replace")
        .1;
    refuses(src, building, "native step", |vm| {
        set_step_word(vm, op::STR_REPLACE, 6, u64::MAX);
    });
    // A split from the right at whitespace, searching past the text
    // (`h[b..p]` out of range).
    let src = STEPPED
        .iter()
        .find(|s| s.0 == op::STR_SPLIT)
        .expect("split")
        .1;
    let rspace = |vm: &Vm| {
        step_state(vm, op::STR_SPLIT).is_some()
            && step_word(vm, op::STR_SPLIT, 4) == crate::strmeth::sm::RSPACE
            && step_word(vm, op::STR_SPLIT, 6) == 0
    };
    refuses(src, rspace, "native step", |vm| {
        set_step_word(vm, op::STR_SPLIT, 2, 1 << 40);
    });
    // An extended slice whose step passes more chars than the text has:
    // its step walks them before it charges fuel.
    refuses_step(op::STR_MAP, |vm| {
        let r = step_state(vm, op::STR_MAP).expect("state");
        let arg = vm.heap.get(r, 1).expect("slot").as_obj().expect("tuple");
        let big = Value::int(1 << 40).expect("small int");
        vm.heap.set(arg, 0, big).expect("slot");
    });
}

#[test]
fn a_parked_str_build_holds_the_chars_it_counts() {
    use crate::nstep::op;
    // A str build's block becomes a `str` with the chars counted so far
    // and those still to come: a count that is not its block's, or bytes
    // that are not UTF-8, would make a `str` restore refuses.
    let built = |o: u8| {
        move |vm: &Vm| {
            step_state(vm, o).is_some() && step_word(vm, o, 1) != 0 && step_word(vm, o, 3) >= 4
        }
    };
    let fill = STEPPED
        .iter()
        .find(|s| s.0 == op::STR_FILL)
        .expect("fill")
        .1;
    let started =
        |vm: &Vm| step_state(vm, op::STR_FILL).is_some() && step_word(vm, op::STR_FILL, 3) >= 4;
    refuses(fill, started, "native step", |vm| {
        let chars = step_word(vm, op::STR_FILL, 4);
        set_step_word(vm, op::STR_FILL, 4, chars - 1);
    });
    let spoil = |o: u8| {
        move |vm: &mut Vm| {
            let r = step_state(vm, o).expect("state");
            let out = vm
                .heap
                .get(r, 3)
                .expect("slot")
                .as_byte_ref()
                .expect("block");
            vm.heap.bytes_mut(out).expect("bytes")[0] = 0xFF;
        }
    };
    refuses(fill, started, "native step", spoil(op::STR_FILL));
    let replace = STEPPED
        .iter()
        .find(|s| s.0 == op::STR_REPLACE)
        .expect("replace")
        .1;
    refuses(
        replace,
        built(op::STR_REPLACE),
        "native step",
        spoil(op::STR_REPLACE),
    );
}

#[test]
fn a_map_part_way_through_a_chars_replacement_restores() {
    use crate::nstep::op;
    // `expandtabs` and `translate` keep how much of a wide tab's spaces or
    // a long replacement they wrote in `part` (word 7), as a lowering
    // keeps a pending `σ` there.
    let pending = |vm: &Vm| {
        step_state(vm, op::STR_MAP).is_some()
            && step_word(vm, op::STR_MAP, 1) != 0
            && step_word(vm, op::STR_MAP, 7) != 0
    };
    for src in [
        "t = ('é\\t' * 3).expandtabs(3_000_000)\n",
        "t = ('b' * 600_000 + 'a').translate({97: 'é' * 4_000_000})\n",
    ] {
        let mut vm = session(src, pending);
        assert!(pending(&vm), "{src}");
        assert!(restore(&mut vm).is_ok(), "{src}");
    }
    // A power-of-two digit text becomes a `str` of its block: ASCII only.
    let digits = |vm: &Vm| {
        step_state(vm, op::NUM_TEXT)
            .is_some_and(|_| step_word(vm, op::NUM_TEXT, 1) == crate::builtins::DIGITS)
    };
    refuses("t = bin(1 << 5_000_000)\n", digits, "native step", |vm| {
        let r = step_state(vm, op::NUM_TEXT).expect("state");
        let b = vm
            .heap
            .get(r, 1)
            .expect("slot")
            .as_byte_ref()
            .expect("block");
        let d = vm.heap.bytes_mut(b).expect("bytes");
        let last = d.len() - 1;
        d[last] = 0xFF;
    });
}

/// The live byte job of kind `k` (a finished one may not be collected
/// yet), and raw word `i` of it (as [`set_step_word`] counts).
fn job_state(vm: &Vm, k: u8) -> Option<(mm_core::Ref, u32)> {
    use crate::nstep::op;
    let mut found = None;
    vm.heap.for_each_object(&mut |r, h| {
        if h.type_id() == ty::NSTATE
            && let Ok((h, w)) = vm.heap.object_words(r)
            && w.get(h.nvals() as usize..h.nvals() as usize + 2)
                == Some(&[u64::from(op::BYTES), u64::from(k)])
        {
            found = Some((r, h.nvals()));
        }
    });
    found
}

#[test]
fn a_parked_byte_job_keeps_its_progress_in_range() {
    use crate::bytearray::fin;
    use crate::bytes::{S_POS, W_A, W_C, W_I, W_PASS, W_S, cut, job};
    use crate::nstep::op;
    // Job word `i` of the job of kind `k` (after the operation and kind
    // words).
    let word = |vm: &Vm, k: u8, i: usize| {
        let (r, n) = job_state(vm, k).expect("a job");
        raw(vm, r, n + 2 + u32::try_from(i).expect("small"))
    };
    let set = |vm: &mut Vm, k: u8, i: usize, x: u64| {
        let (r, n) = job_state(vm, k).expect("a job");
        vm.heap
            .set_raw(r, n + 2 + u32::try_from(i).expect("small"), x)
            .expect("slot");
    };
    let src = STEPPED.iter().find(|s| s.0 == op::BYTES).expect("bytes").1;
    let running = |k: u8| move |vm: &Vm| job_state(vm, k).is_some();
    // `b * k` with nothing to copy: the doubling never advanced.
    refuses(src, running(job::REPEAT), "native step", |vm| {
        let (r, _) = job_state(vm, job::REPEAT).expect("a job");
        vm.heap.set(r, 0, Value::NONE).expect("slot");
        set(vm, job::REPEAT, crate::bytes::W_O, 0);
    });
    // A stride of 0 (`off % step`), and a write position past the read.
    refuses(src, running(job::DEL_STRIDE), "native step", |vm| {
        set(vm, job::DEL_STRIDE, W_A + 1, 0);
    });
    refuses(src, running(job::DEL_STRIDE), "native step", |vm| {
        let i = word(vm, job::DEL_STRIDE, W_I);
        set(vm, job::DEL_STRIDE, W_A + 3, i + 1);
    });
    // A splice's `lo` past the bytearray (`clamp(lo, old)` asserts).
    let splice = |vm: &Vm| running(job::GROW)(vm) && word(vm, job::GROW, W_A) == fin::SPLICE;
    refuses(src, splice, "native step", |vm| {
        set(vm, job::GROW, W_A + 1, 1 << 40);
    });
    // Whitespace split, making pieces: one that ends before it starts
    // (`bytes_slice` out of range).
    let making = |vm: &Vm| {
        running(job::SPLIT)(vm)
            && word(vm, job::SPLIT, W_A) == cut::SPACE
            && word(vm, job::SPLIT, W_PASS) != 0
    };
    refuses(src, making, "native step", |vm| {
        set(vm, job::SPLIT, W_C + 2, 1);
        set(vm, job::SPLIT, W_S + S_POS, 0);
        set(vm, job::SPLIT, W_I, u64::MAX - 10);
    });
}

#[test]
fn a_linear_dict_uses_no_more_entries_than_it_holds() {
    refuses(
        "d = {'a': 1, 'b': 2}\n",
        |_| false,
        "dict index",
        |vm| {
            for r in objects(vm, ty::DICT) {
                let (h, w) = vm.heap.object_words(r).expect("words");
                let is_linear =
                    Value::from_bits(w[1]).is_none_or(|v| !v.is_obj_kind(ty::RAW as u8));
                if is_linear && w.len() > h.nvals() as usize {
                    let last = u32::try_from(w.len() - 1).expect("small");
                    let c = w[w.len() - 1];
                    vm.heap
                        .set_raw(
                            r,
                            last,
                            (c & u64::from(u32::MAX)) | (u64::from(u32::MAX) << 32),
                        )
                        .expect("slot");
                }
            }
        },
    );
}

#[test]
fn a_rebuild_restarted_or_given_full_tables_is_refused() {
    // BULK part way through rebuilding a set's or a dict's table into new
    // tables (value slots 6 and 7, raw words `[op, prog, pc, i, j, n,
    // res]` after them, `j` old slots or entries moved). Restarted from
    // `j` 0, the rebuild put its keys in again (`len(s)` 2,000,000 and
    // `len(list(s))` 3,048,576 [M]); given the set's own tables, or a
    // list's full buffer, its probes for a free slot found none, a step
    // that never ended [M].
    let bulk = crate::nstep::op::BULK;
    let word = |vm: &Vm, r, i: u32| raw(vm, r, 8 + i);
    let rebuilding = |vm: &Vm, r| {
        vm.heap
            .get(r, 7)
            .is_ok_and(|h| h.is_obj_kind(ty::RAW as u8))
            && word(vm, r, 4) > 1
    };
    let forged = |src: &str, edit: &dyn Fn(&mut Vm, mm_core::Ref)| {
        let mut vm = session(src, |vm| {
            step_state(vm, bulk).is_some_and(|r| rebuilding(vm, r))
        });
        let r = step_state(&vm, bulk).expect("parked rebuilding");
        edit(&mut vm, r);
        restore(&mut vm).is_err()
    };
    let set = "s = set(range(3_000_000))\n";
    let dict = "d = dict.fromkeys(range(1_000_000))\n";
    for src in [set, dict] {
        // Unforged, it restores (and `resume_forged` finishes it).
        assert!(!forged(src, &|_, _| {}), "{src}");
        // Restarted, or with fewer entries moved than it put in.
        // (A set's `n` is 0 here; a dict's counts what it moved.)
        let n0 = if src == dict { (5, 0) } else { (4, 0) };
        for (i, x) in [(4, 0), (4, 1), (5, 1), n0] {
            let edit = |vm: &mut Vm, r| vm.heap.set_raw(r, 8 + i, x).expect("word");
            assert!(forged(src, &edit), "{src} word {i} = {x}");
        }
    }
    // A set's: as if its hashes were still being written.
    let touching = |vm: &mut Vm, r| {
        let len = vm.heap.get(r, 7).ok().and_then(Value::as_obj);
        let len = len.map_or(0, |h| vm.heap.header(h).expect("hashes").len());
        vm.heap.set_raw(r, 8 + 4, 0).expect("j");
        vm.heap.set_raw(r, 8 + 5, u64::from(len)).expect("n");
    };
    assert!(forged(set, &touching));
    // Its own key table as the new one, then a list's full buffer.
    let own = |vm: &mut Vm, r| {
        let s = objects(vm, ty::SET).into_iter().max_by_key(|&s| {
            let k = vm.heap.get(s, 0).ok().and_then(Value::as_obj);
            k.map_or(0, |k| vm.heap.header(k).expect("keys").len())
        });
        let keys = vm.heap.get(s.expect("the set"), 0).expect("keys");
        vm.heap.set(r, 6, keys).expect("slot");
    };
    assert!(forged(set, &own));
    let src = "xs = [0] * 4_194_304\ns = set(range(3_000_000))\n";
    let full = |vm: &mut Vm, r| {
        let l = objects(vm, ty::LIST).into_iter().find(|&l| {
            let b = vm.heap.get(l, 0).ok().and_then(Value::as_obj);
            b.is_some_and(|b| vm.heap.header(b).expect("buf").len() >= 4_194_304)
        });
        let buf = vm.heap.get(l.expect("the list"), 0).expect("buf");
        vm.heap.set(r, 6, buf).expect("slot");
    };
    assert!(forged(src, &full));
}

#[cfg(feature = "json")]
#[test]
fn a_json_objects_growth_restarted_is_refused() {
    // json's growth of a big object (`JSON_SCAN`, value slots `sv, ctxs,
    // aux, ne, ni`, raw words after the operation word, `GAT` 13 and `GN`
    // 14): `n` forged to 0 gave 606,784 entries of 1,000,000 [M, reviewer].
    let src = "import json\nt = str(list(range(300_000)))[1:-1].replace(', ', '\": 1, \"')\nd = json.loads('{\"' + t + '\": 1}')\nassert len(d) == 300_000\n";
    let scan = crate::nstep::op::JSON_SCAN;
    let growing = |vm: &Vm, r| {
        vm.heap
            .get(r, 4)
            .is_ok_and(|h| h.is_obj_kind(ty::RAW as u8))
            && raw(vm, r, 6 + 14) > 1
    };
    let find = |vm: &Vm| step_state(vm, scan).filter(|&r| growing(vm, r));
    for (i, x) in [
        (6 + 13, 0),
        (6 + 14, 0),
        (6 + 14, 1),
        (6 + 14, u64::MAX - 1),
    ] {
        let mut vm = session_in(src, 1 << 12, |vm| find(vm).is_some());
        let r = find(&vm).expect("parked growing an object");
        vm.heap.set_raw(r, i, x).expect("a word");
        let got = restore_and_finish(&mut vm);
        assert!(
            got.as_ref().is_some_and(|(name, _)| name == "refused"),
            "{i} {x}: {got:?}"
        );
    }
    // Unforged, it restores and finishes.
    let mut vm = session_in(src, 1 << 12, |vm| find(vm).is_some());
    assert_eq!(restore_and_finish(&mut vm), None);
}

#[test]
fn a_forged_merge_position_raises() {
    // Items of the state to set, from its items.
    type Edit = dyn Fn(&[u64]) -> Vec<(u32, Value)>;
    fn int(v: u64) -> Option<i64> {
        Value::from_bits(v).and_then(Value::as_int)
    }
    fn small(x: i64) -> Value {
        Value::int(x).expect("small")
    }
    // A long sort's state, a list `[a, b, pass, i, l, r, k, reverse,
    // kind]` (`Vm::sort_more`), parked in a merge pass part way through a
    // run pair. A write position other than what the reads took, reads
    // outside their runs, or `b` as `a` completed with an unsorted list
    // [M, reviewer].
    let src = "xs = [(i * 7919) % 300_007 for i in range(300_000)]\nys = sorted(xs)\n";
    let merging = |vm: &Vm| {
        objects(vm, ty::LIST).into_iter().find(|&l| {
            let w = vm.seq_words(Value::from_ref(l, ty::LIST as u8));
            w.len() == 9
                && Value::from_bits(w[0]).is_some_and(|a| a.is_obj_kind(ty::LIST as u8))
                && int(w[2]).is_some_and(|p| (10..62).contains(&p))
                && int(w[6]) > int(w[3])
        })
    };
    let forged = |edit: &Edit| {
        let mut vm = session(src, |vm| merging(vm).is_some());
        let st = Value::from_ref(merging(&vm).expect("a merge pass"), ty::LIST as u8);
        let w = vm.seq_words(st).to_vec();
        let buf = vm.list_buf(st).expect("its items");
        for (i, v) in edit(&w) {
            vm.heap.set_values(buf, i, &[v]).expect("an item");
        }
        restore_and_finish(&mut vm)
    };
    // Unforged, it finishes.
    assert_eq!(forged(&|_| Vec::new()), None);
    let edits: [(&str, &Edit); 5] = [
        ("k 0", &|_| vec![(6, small(0))]),
        ("l = r = i", &|w| {
            let i = Value::from_bits(w[3]).expect("i");
            vec![(4, i), (5, i)]
        }),
        ("k + 1", &|w| vec![(6, small(int(w[6]).expect("k") + 1))]),
        ("b = a", &|w| vec![(1, Value::from_bits(w[0]).expect("a"))]),
        ("i off a pair", &|w| {
            vec![(3, small(int(w[3]).expect("i") + 1))]
        }),
    ];
    for (what, edit) in edits {
        let got = forged(edit);
        assert!(
            got.as_ref().is_some_and(|(name, _)| name == "SystemError"),
            "{what}: {got:?}"
        );
    }
}
