//! Deeply nested data through the natives that walk it (docs/spec.md §5,
//! §13): `repr`, `str`, `==`, `<`, `hash`, `copy`, `json.dumps`, `sorted`,
//! `in`, f-strings and dict/set keys on lists, dicts, tuples, frozensets
//! and strings nested up to ~60,000 deep, with shared (at most 2^8
//! paths) and cyclic parts.
//! Every walk must finish or raise (`RecursionError`, `MemoryError`,
//! `TypeError`, `ValueError`), never overflow the Rust stack or panic.
//!
//! Input: byte 0 picks the operations; each further byte adds a run of
//! one container kind (low 3 bits) whose length is its high 5 bits × 61
//! plus 1.
//!
//! ```bash
//! cd fuzz && cargo +nightly fuzz run nested_data
//! ```

#![no_main]

use micro_monty::{Engine, EngineConfig, Limits, Progress, Session};
use std::cell::RefCell;
use std::fmt::Write as _;

const SETUP: &str = "import copy, json
def _build(spec):
    x = 'leaf'
    shared = 0
    for b in spec:
        k = b & 7
        for _ in range((b >> 3) * 61 + 1):
            if k == 0:
                x = [x]
            elif k == 1:
                x = {'k': x}
            elif k == 2:
                x = (x,)
            elif k == 3:
                # Shared, but not deeper than 2 ** 8 paths: comparing
                # two such DAGs is exponential, in CPython too, and a
                # native walk cannot stop when its fuel runs out; at
                # 2 ** 12 a single repr is already ~65 MB.
                shared += 1
                x = [x, 1, x] if shared <= 8 else [x, 1]
            elif k == 4:
                x = {1: x, 's': 'v'}
            elif k == 5:
                try:
                    x = frozenset([x, 2])
                except TypeError:
                    x = (2, x)
            elif k == 6:
                x = 'a' + str(len(spec)) if isinstance(x, str) else (x, 'a')
            else:
                c = [x]
                c.append(c)
                x = c
    return x
_OPS = [
    lambda x, y: repr(x),
    lambda x, y: str(y),
    lambda x, y: x == y,
    lambda x, y: x < y,
    lambda x, y: hash(x),
    lambda x, y: copy.deepcopy(x),
    lambda x, y: copy.copy(x),
    lambda x, y: json.dumps(x),
    lambda x, y: sorted([x, y, x]),
    lambda x, y: x in [1, y],
    lambda x, y: f'{x!r:>5}',
    lambda x, y: {x: 1},
    lambda x, y: {x, 1},
    lambda x, y: [x] == [y] and (x,) != (y,),
    lambda x, y: print(x),
    lambda x, y: max([y, x]),
]
def _n(spec, ops):
    x = _build(spec)
    y = _build(spec)
    for i in ops:
        try:
            _OPS[i % len(_OPS)](x, y)
        except (RecursionError, MemoryError, TypeError, ValueError):
            pass
";

thread_local! {
    static STATE: RefCell<Option<(Engine, Session, u32)>> = const { RefCell::new(None) };
}

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    let Some((&sel, spec)) = data.split_first() else {
        return;
    };
    if spec.is_empty() || spec.len() > 32 {
        return;
    }
    let mut source = String::from("_n(b'");
    for b in spec {
        let _ = write!(source, "\\x{b:02x}");
    }
    // Two operations from the selector byte, plus `repr` of both.
    let _ = write!(source, "', [{}, {}, 0])", sel & 15, sel >> 4);
    STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        if state.as_ref().is_some_and(|s| s.2 >= 100) {
            *state = None;
        }
        let (engine, session, cells) = state.get_or_insert_with(|| {
            let engine =
                Engine::new(EngineConfig::new().with_limits(Limits::new().with_memory(256 << 20)));
            let mut session = Session::new(&engine).expect("session");
            session.set_output(Box::new(|_| {}));
            let setup = engine.compile(SETUP, "<setup>").expect("setup compiles");
            match session.start(&setup) {
                Progress::Complete(_) => {}
                other => panic!("setup: {other:?}"),
            }
            (engine, session, 0)
        });
        *cells += 1;
        let Ok(code) = engine.compile(&source, "<fuzz>") else {
            return;
        };
        session.set_fuel(Some(5_000_000));
        match session.start(&code) {
            Progress::Complete(_) | Progress::Parked { .. } => {}
            Progress::Error(e) => {
                assert!(
                    !matches!(e.type_name.as_str(), "SystemError" | "NameError"),
                    "{source}\n{}",
                    e.traceback
                );
            }
            other => panic!("{source}: {other:?}"),
        }
        session.cancel();
    });
});
