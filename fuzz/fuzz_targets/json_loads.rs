//! `json.loads` on fuzzed text (docs/spec.md §13) through the facade, with
//! a fuel budget: a document either fails with `ValueError` (or
//! `RecursionError`/`MemoryError`) or loads, and then `json.dumps` of it
//! must be stable under another round trip, in every `indent`/`sort_keys`
//! mode. Nothing may panic, hang or raise `SystemError`.
//!
//! Input: byte 0 picks `str` or `bytes` input and the dump options; the
//! rest is the document.
//!
//! ```bash
//! cd fuzz && cargo +nightly fuzz run json_loads
//! ```

#![no_main]

use micro_monty::{Engine, EngineConfig, Limits, Progress, Session};
use std::cell::RefCell;
use std::fmt::Write as _;

const SETUP: &str = "import json
def _j(s, indent, sort_keys):
    try:
        v = json.loads(s)
    except (ValueError, RecursionError, MemoryError):
        return
    try:
        t = json.dumps(v, indent=indent, sort_keys=sort_keys)
    except (RecursionError, MemoryError):
        return
    except TypeError:
        if not sort_keys:
            raise
        return
    u = json.dumps(json.loads(t), indent=indent, sort_keys=sort_keys)
    assert t == u, (t, u)
    json.dumps(v, separators=(',', ':'), ensure_ascii=False)
";

/// Cells a session runs before a fresh one.
const CELLS_PER_SESSION: u32 = 200;

thread_local! {
    static STATE: RefCell<Option<(Engine, Session, u32)>> = const { RefCell::new(None) };
}

fn literal(data: &[u8], bytes: bool) -> String {
    let mut out = String::new();
    if bytes {
        out.push_str("b'");
        for b in data {
            let _ = write!(out, "\\x{b:02x}");
        }
    } else {
        out.push('\'');
        for c in String::from_utf8_lossy(data).chars() {
            let _ = write!(out, "\\U{:08x}", u32::from(c));
        }
    }
    out.push('\'');
    out
}

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    let Some((&sel, doc)) = data.split_first() else {
        return;
    };
    if doc.len() > 4096 {
        return;
    }
    let indent = ["None", "0", "2", "'\\t'"][usize::from(sel & 3)];
    let sort_keys = if sel & 4 != 0 { "True" } else { "False" };
    let source = format!(
        "_j({}, {indent}, {sort_keys})",
        literal(doc, sel & 0x80 != 0)
    );
    STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        if state.as_ref().is_some_and(|s| s.2 >= CELLS_PER_SESSION) {
            *state = None;
        }
        let (engine, session, cells) = state.get_or_insert_with(|| {
            let engine =
                Engine::new(EngineConfig::new().with_limits(Limits::new().with_memory(64 << 20)));
            let mut session = Session::new(&engine).expect("session");
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
        session.set_fuel(Some(2_000_000));
        match session.start(&code) {
            Progress::Complete(_) | Progress::Parked { .. } => {}
            Progress::Error(e) => {
                assert!(
                    !matches!(
                        e.type_name.as_str(),
                        "SystemError" | "NameError" | "AssertionError"
                    ),
                    "{source}\n{}",
                    e.traceback
                );
            }
            other => panic!("{source}: {other:?}"),
        }
        session.cancel();
    });
});
