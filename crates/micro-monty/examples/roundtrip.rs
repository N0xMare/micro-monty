//! Comparisons nested to the depth limit on the host's default stack, then
//! one cell parked on fuel, snapshotted, restored into a new session and
//! finished there. `cargo xtask wasm` runs it under wasmtime on
//! `wasm32-wasip1` (prints `roundtrip ok <value>` and the snapshot
//! `build_id`) and on `wasm32-unknown-unknown` (its `main` returns 0),
//! where std has no clock or randomness and the host provides them.

use micro_monty::{Engine, EngineConfig, Limits, ParkReason, Progress, Resume, Session};
use std::process::ExitCode;

// std's allocator without its per-site checks (crates/mm-wasm-alloc,
// docs/user-guide.md); optional for embedders.
#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
#[global_allocator]
static ALLOCATOR: mm_wasm_alloc::Allocator = mm_wasm_alloc::Allocator::new();

const CELL: &str = "\
import random, re, time
assert time.time() > 0 and 0 <= random.random() < 1
# The default endpos (2**63 - 1) clamps on 32-bit targets too.
assert re.findall('a', 'banana') == ['a', 'a', 'a'] and re.match('a', 'a')
total = 0
for i in range(10_000):
    total += i * i
print('computed', total)
total";

/// Comparisons as deep as the depth limit allows, which used to recurse
/// natively and exhaust wasmtime's default stack ("call stack exhausted").
const DEEP: &str = "\
a = {}
a['x'] = a
b = {}
b['x'] = b
try:
    a == b
except RecursionError:
    pass
else:
    raise AssertionError('no RecursionError')
u, v = 'u', 'u'
for i in range(600):
    u = frozenset([u, 2])
    v = frozenset([v, 2])
assert u == v and u <= v and {u: 1} == {v: 1} and not [u] < [v]
'deep ok'";

/// `wasm32-unknown-unknown`: no system clock or OS randomness.
const NO_OS: bool = cfg!(all(target_family = "wasm", target_os = "unknown"));

/// What the host provides where std cannot: a fixed clock and a counter
/// as "entropy" (an embedder would pass real ones).
fn provide(session: &mut Session) {
    session.set_output(Box::new(|text| print!("{text}")));
    if NO_OS {
        session.set_wall_clock(Some(Box::new(|| 1_800_000_000.0)));
        let mut n = 0u8;
        session.set_entropy(Some(Box::new(move |buf: &mut [u8]| {
            for b in buf {
                n = n.wrapping_add(1);
                *b = n;
            }
        })));
        session.set_deadline_with(Box::new(|| 0), u64::MAX);
    }
}

fn main() -> ExitCode {
    let engine = Engine::new(EngineConfig::new().with_limits(Limits::new().with_fuel(Some(500))));
    let Ok(code) = engine.compile(CELL, "roundtrip.py") else {
        return fail("compile");
    };
    let Ok(mut session) = Session::new(&engine) else {
        return fail("session");
    };
    if NO_OS {
        // Without a host clock, reading it raises instead of panicking.
        let Ok(clock) = engine.compile("import time\ntime.time()", "clock.py") else {
            return fail("compile");
        };
        session.set_fuel(None);
        match session.start(&clock) {
            Progress::Error(e) if e.type_name == "RuntimeError" => {}
            _ => return fail("clockless time.time()"),
        }
        session.set_fuel(Some(500));
    }
    provide(&mut session);
    if let Err(step) = deep_comparisons(&engine) {
        return fail(step);
    }
    if session.start(&code)
        != (Progress::Parked {
            reason: ParkReason::Fuel,
        })
    {
        return fail("park on fuel");
    }
    let Ok(snapshot) = session.snapshot() else {
        return fail("snapshot");
    };
    drop(session);
    let Ok(mut restored) = engine.restore(&snapshot) else {
        return fail("restore");
    };
    provide(&mut restored);
    restored.set_fuel(None);
    match restored.resume(Resume::Continue) {
        Ok(Progress::Complete(Some(value))) if value.repr == "333283335000" => {
            let id = micro_monty::build_id()
                .iter()
                .fold(String::new(), |mut id, b| {
                    let _ = std::fmt::Write::write_fmt(&mut id, format_args!("{b:02x}"));
                    id
                });
            println!(
                "roundtrip ok {} ({} snapshot bytes)\nbuild_id {id}",
                value.repr,
                snapshot.len()
            );
            ExitCode::SUCCESS
        }
        other => {
            eprintln!("{other:?}");
            fail("resume")
        }
    }
}

/// Runs [`DEEP`] in a session of its own.
fn deep_comparisons(engine: &Engine) -> Result<(), &'static str> {
    // Source at the parser's nesting limit, the deepest Rust recursion
    // left (docs/user-guide.md, Embedding guarantees): 199 parentheses compile,
    // 199 nested conditional expressions raise `RecursionError`.
    let n = 199;
    let parens = format!("x = {}1{}\n", "(".repeat(n), ")".repeat(n));
    engine.compile(&parens, "parens.py").map_err(|_| "parens")?;
    let ifexp = format!("x = {}1{}\n", "1 if 1 else (".repeat(n), ")".repeat(n));
    if engine.compile(&ifexp, "ifexp.py").is_ok() {
        return Err("ifexp compiled");
    }
    let deep = engine.compile(DEEP, "deep.py").map_err(|_| "compile")?;
    let mut session = Session::new(engine).map_err(|_| "session")?;
    provide(&mut session);
    session.set_fuel(None);
    match session.start(&deep) {
        Progress::Complete(Some(v)) if v.repr == "'deep ok'" => Ok(()),
        other => {
            eprintln!("{other:?}");
            Err("deep comparisons")
        }
    }
}

fn fail(step: &str) -> ExitCode {
    eprintln!("roundtrip failed at {step}");
    ExitCode::FAILURE
}
