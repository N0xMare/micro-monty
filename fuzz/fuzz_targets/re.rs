//! Runs `re` on fuzzed patterns and subjects through the facade: compile,
//! search, findall, sub, split and fullmatch, with a fuel budget. A run may
//! raise `re.error` (or another ordinary exception) or park when the fuel
//! runs out (catastrophic patterns), but must never panic, hang or raise
//! `SystemError` (a program the compiler built badly).
//!
//! Input: byte 0 picks the flags and str or bytes; the rest is the pattern
//! and the subject, split at the first `0xFF` byte.
//!
//! ```bash
//! cd fuzz && cargo +nightly fuzz run re
//! ```

#![no_main]

use micro_monty::{Engine, EngineConfig, Limits, Progress, Session};
use std::cell::RefCell;
use std::fmt::Write as _;

const SETUP: &str = "import re
def _r(p, s, f):
    try:
        c = re.compile(p, f)
    except (re.error, OverflowError, ValueError, RecursionError):
        return
    c.search(s)
    c.findall(s)
    c.sub('x' if isinstance(s, str) else b'x', s)
    c.split(s)
    c.fullmatch(s)
    list(c.finditer(s))
";

/// Flags each selector bit adds: IGNORECASE, MULTILINE, DOTALL, VERBOSE,
/// ASCII.
const FLAGS: [u32; 5] = [2, 8, 16, 64, 256];

/// Cells a session runs before a fresh one (each cell's code stays loaded).
const CELLS_PER_SESSION: u32 = 200;

thread_local! {
    static STATE: RefCell<Option<(Engine, Session, u32)>> = const { RefCell::new(None) };
}

/// `data` as a Python literal: `str` (lossy UTF-8) or `bytes`.
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
    let Some((&sel, rest)) = data.split_first() else {
        return;
    };
    if rest.len() > 512 {
        return;
    }
    let (pat, subj) = match rest.iter().position(|&b| b == 0xFF) {
        Some(i) => (&rest[..i], &rest[i + 1..]),
        None => (rest, &[][..]),
    };
    let bytes = sel & 0x80 != 0;
    let mut flags = 0;
    for (i, f) in FLAGS.iter().enumerate() {
        if sel & (1 << i) != 0 {
            flags |= f;
        }
    }
    let source = format!(
        "_r({}, {}, {flags})",
        literal(pat, bytes),
        literal(subj, bytes)
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
        session.set_fuel(Some(300_000));
        match session.start(&code) {
            Progress::Complete(_) | Progress::Parked { .. } => {}
            Progress::Error(e) => {
                assert!(
                    e.type_name != "SystemError" && e.type_name != "NameError",
                    "{source}\n{}",
                    e.traceback
                );
            }
            other => panic!("{source}: {other:?}"),
        }
        session.cancel();
    });
});
