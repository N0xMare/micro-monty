//! Sessions: cells share globals until `reset`; imports of bundled and host
//! modules; cancellation keeps the session usable.

#![allow(clippy::expect_used, clippy::panic, reason = "tests")]

use micro_monty::{
    Engine, EngineConfig, HintGroup, HostError, HostErrorKind, HostModule, HostParam,
    HostSignature, HostValue, Limits, ParkReason, Progress, Resume, Session, Stream, TextCut,
};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

struct Cells {
    engine: Engine,
    session: Session,
    out: Arc<Mutex<String>>,
}

impl Cells {
    fn new() -> Cells {
        let engine = Engine::new(EngineConfig::default());
        let mut session = Session::new(&engine).expect("session");
        let out = Arc::new(Mutex::new(String::new()));
        let sink = Arc::clone(&out);
        session.set_output_streams(Box::new(move |stream, s: &str| {
            let mut o = sink.lock().expect("output lock");
            if stream == Stream::Stderr {
                o.push_str("[err]");
            }
            o.push_str(s);
        }));
        Cells {
            engine,
            session,
            out,
        }
    }

    fn start(&mut self, src: &str) -> Progress {
        let code = self.engine.compile(src, "cell.py").expect("compiles");
        self.session.start(&code)
    }

    fn value(&mut self, src: &str) -> String {
        match self.start(src) {
            Progress::Complete(Some(c)) => c.repr,
            other => panic!("{other:?}"),
        }
    }

    fn error(&mut self, src: &str) -> (String, String) {
        match self.start(src) {
            Progress::Error(e) => (e.type_name, e.message),
            other => panic!("{other:?}"),
        }
    }

    fn take_out(&self) -> String {
        std::mem::take(&mut *self.out.lock().expect("output lock"))
    }
}

#[test]
fn globals_persist_across_cells_until_reset() {
    let mut c = Cells::new();
    c.session
        .define(&HostModule::untyped("", &["tool"]))
        .expect("defined");
    assert_eq!(c.value("x = 40\ndef f(y):\n    return x + y\nf(2)"), "42");
    assert_eq!(
        c.value("x = 1\nimport sys\n[f(1), sys.__name__]"),
        "[2, 'sys']"
    );
    c.session.reset().expect("reset");
    assert_eq!(
        c.error("f"),
        ("NameError".into(), "name 'f' is not defined".into())
    );
    assert_eq!(
        c.error("sys"),
        ("NameError".into(), "name 'sys' is not defined".into())
    );
    assert_eq!(c.value("__name__"), "'__main__'");
    // Host functions survive a reset.
    let p = c.start("tool(1)");
    assert!(
        matches!(p, Progress::HostCall { ref name, .. } if name == "tool"),
        "{p:?}"
    );
}

#[test]
fn globals_is_a_read_only_view_across_cells() {
    // Decision G2: reads see the session's globals as earlier cells left
    // them; every write raises TypeError instead of silently diverging.
    let mut c = Cells::new();
    assert_eq!(c.value("'answer' in globals()"), "False");
    assert_eq!(
        c.start("answer = 42\ng = globals()"),
        Progress::Complete(None)
    );
    assert_eq!(
        c.value("['answer' in globals(), g['answer'], g.get('nope', 0), 'g' in g]"),
        "[True, 42, 0, True]"
    );
    let ro = "micro-monty: globals() is a read-only view; assign or del the name itself \
              (or use a dict of your own)";
    for write in [
        "globals()['answer'] = 1",
        "del globals()['answer']",
        "globals().update(answer=1)",
        "globals().setdefault('b', 1)",
        "globals().pop('answer')",
        "g |= {'b': 1}",
    ] {
        assert_eq!(c.error(write), ("TypeError".into(), ro.into()), "{write}");
    }
    assert_eq!(c.value("[answer, 'b' in globals()]"), "[42, False]");
}

#[test]
fn transient_cells_do_not_accumulate_source_and_code() {
    let mut c = Cells::new();
    let padding = "#".repeat(128 * 1024);
    let code = c
        .engine
        .compile(&format!("{padding}\npass"), "large.py")
        .expect("code");
    assert_eq!(c.session.start(&code), Progress::Complete(None));
    let initial = c.session.snapshot().expect("snapshot").len();
    for i in 0..64 {
        assert_eq!(c.session.start(&code), Progress::Complete(None));
        // Recompilation and different filenames must not defeat reclamation.
        let other = c
            .engine
            .compile(&format!("{padding}\n# {i}\npass"), &format!("{i}.py"))
            .expect("code");
        assert_eq!(c.session.start(&other), Progress::Complete(None));
    }
    let snapshot = c.session.snapshot().expect("snapshot");
    assert!(
        snapshot.len() < initial + 4096,
        "{} -> {}",
        initial,
        snapshot.len()
    );
    c.session = c.engine.restore(&snapshot).expect("restore");
    c.session.reset().expect("reset");
    assert!(c.session.snapshot().expect("snapshot").len() < initial - padding.len());
}

#[test]
fn discarded_string_literals_do_not_exhaust_a_persistent_session() {
    let engine = Engine::new(EngineConfig::new().with_limits(Limits::new().with_memory(4 << 20)));
    let mut session = Session::new(&engine).expect("session");
    let keep = engine.compile("class C: pass\nkept = C()\nkept.name = 'still live'\ndef read():\n    return kept.name", "keep.py").expect("code");
    assert_eq!(session.start(&keep), Progress::Complete(None));
    let padding = "x".repeat(64 * 1024);
    for i in 0..128 {
        let code = engine
            .compile(
                &format!(
                    "transient = '{padding}{i}'\ndel transient\nassert read() == 'still live'"
                ),
                "literal.py",
            )
            .expect("code");
        assert_eq!(session.start(&code), Progress::Complete(None), "cell {i}");
        if i % 32 == 31 {
            let image = session.snapshot().expect("snapshot");
            session = engine.restore(&image).expect("restore");
        }
    }
    session.reset().expect("reset");
    let check = engine
        .compile("assert len('y' * (1 << 20)) == 1 << 20", "check.py")
        .expect("code");
    assert_eq!(session.start(&check), Progress::Complete(None));
}

#[test]
fn failed_code_load_does_not_retain_partial_source_or_code() {
    let engine = Engine::new(EngineConfig::new().with_limits(Limits::new().with_memory(8 << 20)));
    let mut session = Session::new(&engine).expect("session");
    let fill = engine.compile("keep = []\ntry:\n    while True:\n        keep.append('x' * 1000)\nexcept MemoryError:\n    pass", "fill.py").expect("code");
    assert_eq!(session.start(&fill), Progress::Complete(None));
    let before = session.snapshot().expect("snapshot").len();
    let code = engine
        .compile(
            &format!("#{}\nb'{}'", "p".repeat(64_000), "y".repeat(130_000)),
            "failed.py",
        )
        .expect("code");
    for _ in 0..8 {
        let Progress::Error(e) = session.start(&code) else {
            panic!("expected allocation failure")
        };
        assert_eq!(e.type_name, "MemoryError");
        let after = session.snapshot().expect("snapshot").len();
        assert!(after < before + 4096, "{before} -> {after}");
    }
    let check = engine
        .compile("len(keep) > 1000", "check.py")
        .expect("code");
    let Progress::Complete(Some(value)) = session.start(&check) else {
        panic!("session remains usable")
    };
    assert_eq!(value.repr, "True");
}

#[test]
fn garbage_left_by_a_memory_error_is_collected() {
    let engine = Engine::new(EngineConfig::new().with_limits(Limits::new().with_memory(8 << 20)));
    let mut session = Session::new(&engine).expect("session");
    // `list()` fills the arena with ever larger ints, then fails; nothing
    // refers to them afterwards, but no allocation can succeed to schedule
    // the collection that would free them.
    let src = "\
def bits(num):
    while True:
        yield num
        num <<= 1
try:
    list(bits(1))
except MemoryError:
    pass
ok = 0
for i in range(50):
    try:
        x = [0] * 200_000
        ok += 1
    except MemoryError:
        pass
ok";
    let code = engine.compile(src, "oom.py").expect("code");
    let Progress::Complete(Some(value)) = session.start(&code) else {
        panic!("expected completion")
    };
    assert_eq!(value.repr, "50");
}

/// A cell that ends with the heap at its wall and its data garbage (a fill
/// that raised, then dropped) leaves no safepoint to collect it: every
/// later cell's start, a reset and a host definition were refused for
/// room, a next cell whose load fit failed at its first object, and the
/// session never ran a cell again. python3.14 runs them all.
#[test]
fn cells_after_one_that_filled_memory_run_once_its_garbage_is_collected() {
    let strs = "xs = []\nwhile True:\n    xs.append(str(len(xs)))";
    let chain =
        "node = None\ntry:\n    while True:\n        node = (node,)\nexcept MemoryError:\n    pass";
    let fills: [(u64, &str, &str); 5] = [
        (6, strs, "del xs"),
        (16, strs, "del xs"),
        (6, chain, "node = None"),
        (
            16,
            "def build():\n    xs = []\n    while True:\n        xs.append(str(len(xs)))\nbuild()",
            "1",
        ),
        // The default limit, where the mark stack's doubling is the wall
        // (as after `[{...} for i in range(10**9)]`); the slowest row.
        (256, chain, "node = None"),
    ];
    for (mib, fill, drop) in fills {
        let engine =
            Engine::new(EngineConfig::new().with_limits(Limits::new().with_memory(mib << 20)));
        let start = |session: &mut Session, src: &str| {
            let code = engine.compile(src, "cell.py").expect("compiles");
            match session.start(&code) {
                Progress::Complete(_) => "ok".to_owned(),
                Progress::Error(e) => e.type_name,
                p => panic!("{p:?}"),
            }
        };
        let filled = || {
            let mut session = Session::new(&engine).expect("session");
            start(&mut session, fill);
            assert_eq!(start(&mut session, drop), "ok", "{mib} MiB: {fill}");
            session
        };
        let mut session = filled();
        for next in ["print('next')", "x = 1", "def f(): pass", "import json"] {
            assert_eq!(start(&mut session, next), "ok", "{mib} MiB: {fill}: {next}");
        }
        if mib == 256 {
            continue;
        }
        let mut session = filled();
        let code = session.compile("x = 1", "cell.py").expect("fits");
        assert_eq!(session.start(&code), Progress::Complete(None), "{mib} MiB");
        let mut session = filled();
        session.reset().expect("reset");
        assert_eq!(start(&mut session, "print('next')"), "ok", "{mib} MiB");
        let mut session = filled();
        session
            .define(&HostModule::untyped("tools", &["a", "b"]))
            .expect("defined");
        assert_eq!(start(&mut session, "import tools"), "ok", "{mib} MiB");
    }
    let cell = |engine: &Engine, session: &mut Session, src: &str| {
        let code = engine.compile(src, "cell.py").expect("compiles");
        match session.start(&code) {
            Progress::Complete(_) => "ok".to_owned(),
            Progress::Error(e) => e.type_name,
            p => panic!("{p:?}"),
        }
    };
    let fill = "xs = []\ntry:\n    while True:\n        xs.append(str(len(xs)))\nexcept MemoryError:\n    pass";
    // Code reclamation collects before it lets go of the dead cells' code,
    // whose constants then were garbage beside a flag that said collected.
    let engine = Engine::new(EngineConfig::new().with_limits(Limits::new().with_memory(64 << 20)));
    let mut session = Session::new(&engine).expect("session");
    let consts: Vec<String> = (0..8000).map(|i| format!("\"k{i:05}x\"")).collect();
    let dead = format!("t = [{}]\nt = None\n{fill}", consts.join(", "));
    assert_eq!(cell(&engine, &mut session, &dead), "ok");
    for next in ["y = 'one'", "z = 'two'", "w = 'three'"] {
        assert_eq!(cell(&engine, &mut session, next), "ok", "{next}");
    }
    // That collection, once the fill's code is reclaimed at the wall, also
    // frees `<module>`, which every load needs; loads refused part way
    // follow (a 200 kB literal each). A cell with no other new object
    // (`del xs`) must still load: the refused loads' garbage held the room
    // `<module>` needs while no start collected it.
    let engine = Engine::new(EngineConfig::new().with_limits(Limits::new().with_memory(6 << 20)));
    let literal = "a".repeat(200_000);
    let freed = [
        "del xs",
        "pass",
        "xs = None",
        "del xs",
        "pass",
        "print('after del')",
    ];
    let reused = ["del xs", "y = [str(i) for i in range(1000)]\nprint(len(y))"];
    for (literals, tail) in [(8, &freed[..]), (8, &reused[..]), (4, &reused[..])] {
        let mut session = Session::new(&engine).expect("session");
        assert_eq!(cell(&engine, &mut session, fill), "ok");
        for k in 0..literals {
            cell(&engine, &mut session, &format!("s{k} = \"{literal}\""));
        }
        for next in tail {
            let outcome = cell(&engine, &mut session, next);
            assert_eq!(outcome, "ok", "{literals} literals: {next}");
        }
    }
}

/// A reset over a heap its globals fill with live data, at the mark
/// stack's wall: its new globals were refused, and the collection before
/// the rerun freed nothing, as the old globals still held it all; the
/// reset failed and changed nothing.
/// The fill runs in a function the session keeps, so reclamation frees
/// nothing before the reset.
#[test]
fn a_reset_over_a_heap_full_of_live_globals_resets_the_session() {
    let engine = wall_engine(64);
    let mut session = Session::new(&engine).expect("session");
    let kept = "def fill():\n    xs = []\n    try:\n        while True:\n            xs.append(str(len(xs)))\n    except MemoryError:\n        pass\n    return xs\nxs = fill()";
    assert_eq!(wall_cell(&engine, &mut session, kept), "ok");
    assert_eq!(wall_cell(&engine, &mut session, "y = 1"), "MemoryError");
    session.reset().expect("reset");
    assert_eq!(wall_cell(&engine, &mut session, "xs"), "NameError");
    let check = "assert __name__ == '__main__'\nys = [str(i) for i in range(1000)]";
    assert_eq!(wall_cell(&engine, &mut session, check), "ok");
    // The same at 6 MiB after `__name__` was deleted and its interned
    // name collected: the old globals let go before it is interned again.
    let engine = wall_engine(6);
    let mut session = Session::new(&engine).expect("session");
    session.set_module_dunders(None, None).expect("set");
    assert_eq!(wall_cell(&engine, &mut session, kept), "ok");
    assert_eq!(wall_cell(&engine, &mut session, "y = 1"), "MemoryError");
    session.reset().expect("reset");
    assert_eq!(wall_cell(&engine, &mut session, check), "ok");
}

/// A generator that outlives a `reset` on a typed host function's attribute
/// keeps the dead temporary its yield saved no longer than without the
/// reset: the reset's collections clear it from the generator registry,
/// which the in-place reset leaves whole. Before the registry, 40 MB more
/// at 64 MiB raised `MemoryError` [M]. The same with a snapshot and restore
/// on each side of the reset.
#[test]
fn a_generator_kept_through_the_host_across_a_reset_keeps_no_dead_temporary() {
    let kept = "\
def load():
    return [bytearray(1_000_000) for _ in range(40)]
def gen():
    n = len(load())
    yield n
    yield 7
g = gen()
next(g)
h.g = g";
    let after = "x = [bytearray(1_000_000) for _ in range(40)]\n[len(x), next(h.g)]";
    for restore in [false, true] {
        let engine = wall_engine(64);
        let mut session = Session::new(&engine).expect("session");
        session
            .define(&HostModule {
                name: String::new(),
                functions: vec![HostSignature {
                    name: "h".to_owned(),
                    params: Vec::new(),
                    returns: "None".to_owned(),
                    doc: String::new(),
                    dict_as_kwargs: false,
                }],
                classes: Vec::new(),
                constants: Vec::new(),
                exceptions: Vec::new(),
            })
            .expect("defined");
        let hop = |session: Session| {
            if !restore {
                return session;
            }
            let mut session = session;
            let image = session.snapshot().expect("snapshot");
            engine.restore(&image).expect("restore")
        };
        assert_eq!(wall_cell(&engine, &mut session, kept), "ok");
        let mut session = hop(session);
        session.reset().expect("reset");
        let mut session = hop(session);
        assert_eq!(wall_cell(&engine, &mut session, "g"), "NameError");
        let code = engine.compile(after, "after.py").expect("compiles");
        let Progress::Complete(Some(value)) = session.start(&code) else {
            panic!("the cell after the reset completes (restore: {restore})")
        };
        assert_eq!(value.repr, "[40, 7]");
    }
}

fn wall_cell(engine: &Engine, session: &mut Session, src: &str) -> String {
    let code = engine.compile(src, "cell.py").expect("compiles");
    match session.start(&code) {
        Progress::Complete(_) => "ok".to_owned(),
        Progress::Error(e) => e.type_name,
        p => panic!("{p:?}"),
    }
}

fn wall_engine(mib: u64) -> Engine {
    Engine::new(EngineConfig::new().with_limits(Limits::new().with_memory(mib << 20)))
}

/// A cell that appends `item` to a new `xs` until memory runs out.
fn fill_with(item: &str) -> String {
    format!(
        "xs = []\ntry:\n    while True:\n        xs.append({item})\nexcept MemoryError:\n    pass"
    )
}

/// A `define` before each start at the wall: its bindings are garbage the
/// next start collects, and `del xs`, whose load allocates nothing, runs.
#[test]
fn a_define_before_each_start_at_the_wall_leaves_no_cell_stuck() {
    for (mib, item) in [
        (6, "(len(xs), None)"),
        (3, "(len(xs), None)"),
        (3, "str(len(xs))"),
    ] {
        for builtins in [false, true] {
            let engine = wall_engine(mib);
            let mut session = Session::new(&engine).expect("session");
            if builtins {
                session
                    .define(&HostModule::untyped("", &["h"]))
                    .expect("defined");
            }
            assert_eq!(wall_cell(&engine, &mut session, &fill_with(item)), "ok");
            wall_cell(&engine, &mut session, "n = 1");
            let tools = HostModule::untyped("tools0", &["a", "b"]);
            let _ = session.define(&tools);
            let outcome = wall_cell(&engine, &mut session, "del xs");
            assert_eq!(outcome, "ok", "{mib} MiB {item} builtins={builtins}");
            for _ in 0..3 {
                let _ = session.define(&tools);
                assert_eq!(wall_cell(&engine, &mut session, "del xs"), "NameError");
            }
        }
    }
}

/// A module imported and redefined at the wall while no global holds it:
/// its first definition's 200 kB constant is garbage the next start frees.
#[test]
fn a_module_redefined_at_the_wall_leaves_its_old_constants_to_collect() {
    let engine = wall_engine(64);
    let mut session = Session::new(&engine).expect("session");
    let module = |bytes: usize| {
        let mut m = HostModule::untyped("tools", &["a", "b"]);
        m.constants
            .push(("BIG".to_owned(), HostValue::Str("z".repeat(bytes))));
        m
    };
    session.define(&module(200_000)).expect("defined");
    let first = format!(
        "import tools\nq = len(tools.BIG)\ndel tools\n{}",
        fill_with("str(len(xs))")
    );
    assert_eq!(wall_cell(&engine, &mut session, &first), "ok");
    session.define(&module(1000)).expect("defined");
    for next in [
        "y = 'one'",
        "z = 'two'",
        "w = 'three'",
        "pass",
        "y = 'four'",
    ] {
        assert_eq!(wall_cell(&engine, &mut session, next), "ok", "{next}");
    }
}

/// The same cells with and without a snapshot between them: the code
/// reclaimed at the wall must not depend on a collection request that a
/// snapshot serves.
#[test]
fn cells_at_the_wall_run_the_same_with_snapshots_between_them() {
    let engine = wall_engine(6);
    let literal = format!("s = \"{}\"", "a".repeat(50_000));
    let mut outcomes = Vec::new();
    for mode in 0..3 {
        let mut session = Session::new(&engine).expect("session");
        let mut out = Vec::new();
        let filled = fill_with("(len(xs), None)");
        for src in [
            filled.as_str(),
            &literal,
            "v = 1",
            "w = 2",
            "del xs",
            "print('freed')",
        ] {
            out.push(wall_cell(&engine, &mut session, src));
            if mode > 0 {
                let image = session.snapshot().expect("snapshot");
                if mode == 2 {
                    session = engine.restore(&image).expect("restore");
                }
            }
        }
        outcomes.push(out);
    }
    assert_eq!(outcomes[0], outcomes[1]);
    assert_eq!(outcomes[0], outcomes[2]);
    assert_eq!(outcomes[0][4..], ["ok", "ok"]);
}

/// A session restored after every cell, whose heap ends with no free word
/// slot: a cell that loads without allocating (`del xs`) still runs. The
/// cells marked `#!` are host actions.
#[test]
fn a_session_restored_after_every_cell_runs_del_at_the_wall() {
    let engine = wall_engine(8);
    let a = |n: usize| format!("\"{}\"", "a".repeat(n));
    let names: Vec<String> = (0..8000).map(|i| format!("\"c8_{i:05}x\"")).collect();
    let cells = [
        format!(
            "base = [str(i) for i in range(50000)]\n{}\ndel xs[-1:]",
            fill_with("bytes(1000)")
        ),
        "pass".to_owned(),
        format!("n = {}", a(100)),
        format!(
            "#!abandon\nzs = []\n{}",
            fill_with("str(len(zs))")
                .replace("xs = []\n", "")
                .replace("xs.append", "zs.append")
        ),
        "print(len(xs))".to_owned(),
        format!("#!compile\nw5 = \"{}\"", "c".repeat(100_000)),
        format!("n = {}", a(1000)),
        "print(len(xs))".to_owned(),
        format!("t = [{}]\nt = None", names.join(", ")),
        format!("s = {}", a(400_000)),
        "n = 1".to_owned(),
        "y11 = 'new constant 11'".to_owned(),
        "try:\n    while True:\n        xs.append(str(len(xs)))\nexcept MemoryError:\n    pass"
            .to_owned(),
        "#!compile\nq = 1".to_owned(),
        "#!define".to_owned(),
        "def f15():\n    return 'c15'".to_owned(),
        "#!hint".to_owned(),
        "for _ in range(1):\n    xs.pop()".to_owned(),
        "del xs".to_owned(),
        "del xs".to_owned(),
        "pass".to_owned(),
        "xs = None".to_owned(),
        "del xs".to_owned(),
    ];
    let mut session = Session::new(&engine).expect("session");
    session
        .define(&HostModule::untyped("", &["h", "hf"]))
        .expect("defined");
    let mut out = Vec::new();
    for src in &cells {
        let (first, body) = src.split_once('\n').unwrap_or((src, ""));
        match first {
            "#!define" => {
                let _ = session.define(&HostModule::untyped("tools2", &["a", "b"]));
                continue;
            }
            "#!hint" => {
                let _ = session.set_hint(HintGroup::Network, Some("call_the_tool_16"));
                continue;
            }
            // A cell that does not compile is not run, nor restored after.
            "#!compile" => match session.compile(body, "cell.py") {
                Ok(code) => {
                    session.start(&code);
                }
                Err(_) => continue,
            },
            "#!abandon" => {
                let code = engine.compile(body, "cell.py").expect("compiles");
                session.set_fuel(Some(20_000));
                // At 8 MiB it fills before its fuel runs out.
                session.start(&code);
                session.set_fuel(None);
                continue;
            }
            _ if src.len() > 256 << 10 => continue,
            _ => out.push(wall_cell(&engine, &mut session, src)),
        }
        let image = session.snapshot().expect("snapshot");
        session = engine.restore(&image).expect("restore");
    }
    assert_eq!(out[out.len() - 5..], ["ok", "NameError", "ok", "ok", "ok"]);
}

/// A `reset` that returns `Ok` after a cell's `NameError` found no room
/// frees the session: the shared `MemoryError` raised in its place takes no
/// hint data, which held the old globals alive past the reset. `n += 1`
/// loads once reclamation at the wall freed a slot for `n`; `append` was
/// interned already, the older form.
#[test]
fn a_reset_after_a_name_error_with_no_room_frees_the_globals() {
    let engine = wall_engine(6);
    for second in ["n += 1", "append"] {
        let mut session = Session::new(&engine).expect("session");
        assert_eq!(
            wall_cell(&engine, &mut session, &fill_with("str(len(xs))")),
            "ok"
        );
        let refused = wall_cell(&engine, &mut session, second);
        assert_eq!(refused, "MemoryError", "{second}");
        session.reset().expect("reset");
        for next in ["a = 'after reset'\nprint(a)", "print('second')"] {
            assert_eq!(
                wall_cell(&engine, &mut session, next),
                "ok",
                "{second}: {next}"
            );
        }
    }
}

/// Off the wall, a start refused after its rerun leaves what the rerun
/// allocated; the next start refused for room reruns after collecting it,
/// whether or not a snapshot came between (`Heap::idle` read before the
/// step).
#[test]
fn a_start_refused_off_the_wall_reruns_over_the_last_refusal_garbage() {
    let engine = wall_engine(6);
    let lit = |c: char, n: usize| format!("\"{}\"", c.to_string().repeat(n));
    let cells = [
        fill_with("(len(xs), None)"),
        "for i in range(0, len(xs), 2):\n    xs[i] = None".to_owned(),
        format!("pad = {}", lit('z', 50_000)),
        format!("p = {}\nq = {}", lit('a', 50_000), lit('b', 60_000)),
        format!("r = {}", lit('c', 60_000)),
        format!("r = {}", lit('c', 60_000)),
        format!("r = {}", lit('c', 60_000)),
        "pass".to_owned(),
        format!("r = {}", lit('c', 60_000)),
    ];
    for snapshot in [false, true] {
        let mut session = Session::new(&engine).expect("session");
        let out: Vec<String> = cells
            .iter()
            .map(|src| {
                let outcome = wall_cell(&engine, &mut session, src);
                if snapshot {
                    drop(session.snapshot().expect("snapshot"));
                }
                outcome
            })
            .collect();
        assert_eq!(out[3], "MemoryError");
        for (i, outcome) in out.iter().enumerate().skip(4) {
            assert_eq!(outcome, "ok", "cell {i}, snapshot {snapshot}");
        }
    }
}

/// `set_module_dunders` at the wall releases `__name__` before it
/// allocates: refused, it runs once more after collecting what it
/// released, and what it leaves the next start collects.
#[test]
fn module_dunders_that_release_at_the_wall_leave_no_cell_stuck() {
    let engine = wall_engine(4);
    let mut session = Session::new(&engine).expect("session");
    session
        .define(&HostModule::untyped("", &["h", "hf"]))
        .expect("defined");
    let fill = "d = {}\nxs = []\ntry:\n    while True:\n        d[len(d)] = str(len(d))\nexcept MemoryError:\n    pass";
    for src in [
        fill,
        "print(len(xs))",
        "print(len(xs))",
        "pass",
        "class C4:\n    a4 = 1\no4 = C4()\no4.z4 = 2",
        "xs = None",
    ] {
        wall_cell(&engine, &mut session, src);
    }
    // A cell left at its first park.
    let parked = "def g():\n    zs = []\n    while True:\n        zs.append(str(len(zs)))\ng()";
    let code = engine.compile(parked, "cell.py").expect("compiles");
    session.set_fuel(Some(20_000));
    session.start(&code);
    session.set_fuel(None);
    // It deletes `__name__` before it interns `__file__`, which is refused
    // at first; the rerun collects what it released and fits.
    session.set_module_dunders(None, None).expect("set");
    assert_eq!(
        wall_cell(&engine, &mut session, "y8 = 'new constant 8'"),
        "MemoryError"
    );
    let refill =
        "try:\n    while True:\n        xs.append(bytes(1000))\nexcept MemoryError:\n    pass";
    assert_eq!(wall_cell(&engine, &mut session, refill), "ok");
}

/// An untyped redefinition at the wall of a module no global holds
/// releases its old copy without allocating: the next start collects it.
#[test]
fn an_untyped_redefinition_after_a_collection_leaves_the_next_start_to_collect() {
    let engine = wall_engine(6);
    let mut session = Session::new(&engine).expect("session");
    session
        .define(&HostModule::untyped("", &["h", "hf"]))
        .expect("defined");
    let module = |bytes: usize| {
        let mut m = HostModule::untyped("tools", &["a", "b"]);
        m.constants
            .push(("BIG".to_owned(), HostValue::Str("z".repeat(bytes))));
        m
    };
    session.define(&module(200_000)).expect("defined");
    let first = format!(
        "import tools\nq = len(tools.BIG)\ndel tools\n{}",
        fill_with("str(len(xs))")
    );
    assert_eq!(wall_cell(&engine, &mut session, &first), "ok");
    let y = "y = 'new constant one'";
    assert_eq!(wall_cell(&engine, &mut session, y), "MemoryError");
    session.define(&module(1000)).expect("defined");
    assert_eq!(wall_cell(&engine, &mut session, y), "ok");
    assert_eq!(
        wall_cell(&engine, &mut session, "z = 'new constant two'"),
        "ok"
    );
}

/// A session as the random sessions drive it: the builtins module
/// defined, then `cells` run in turn; `#!dunders NAME` (or bare) sets the
/// module dunders instead. Returns each outcome.
fn wall_session(engine: &Engine, session: &mut Session, cells: &[&str]) -> Vec<String> {
    cells
        .iter()
        .map(|src| match src.strip_prefix("#!dunders") {
            Some(name) => {
                let name = Some(name.trim()).filter(|n| !n.is_empty());
                match session.set_module_dunders(name, None) {
                    Ok(()) => "ok".to_owned(),
                    Err(e) => e.type_name,
                }
            }
            None => wall_cell(engine, session, src),
        })
        .collect()
}

/// `reset` and `set_module_dunders` release before they allocate: refused
/// at a heap idle at its wall, they run once more after the collection
/// that frees what they released, whether or not the host snapshotted
/// first (random sessions 336 at 6 MiB and 314 at 4 MiB).
#[test]
fn reset_and_dunders_refused_at_an_idle_wall_rerun_after_collecting() {
    let fill = fill_with("str(len(xs))");
    let engine = wall_engine(6);
    let mut session = Session::new(&engine).expect("session");
    session
        .define(&HostModule::untyped("", &["h", "hf"]))
        .expect("defined");
    let refill =
        "try:\n    while True:\n        xs.append(str(len(xs)))\nexcept MemoryError:\n    pass";
    let cells = [
        fill.as_str(),
        "y1 = 'new constant 1'",
        "#!dunders mod2",
        "n = 1",
        refill,
    ];
    wall_session(&engine, &mut session, &cells);
    session.reset().expect("reset");
    let engine = wall_engine(4);
    let mut session = Session::new(&engine).expect("session");
    session
        .define(&HostModule::untyped("", &["h", "hf"]))
        .expect("defined");
    let fill = "d = {}\nxs = []\ntry:\n    while True:\n        d[len(d)] = str(len(d))\nexcept MemoryError:\n    pass";
    let b4 = format!("b4 = b\"{}\"", "b".repeat(1000));
    let gc = "import gc\ngc.collect()";
    let cells = [
        fill,
        "xs = None",
        "n = 1",
        "pass",
        &b4,
        gc,
        "n += 1",
        gc,
        "#!dunders",
    ];
    let out = wall_session(&engine, &mut session, &cells);
    assert_eq!(out[8], "ok");
}

/// A typed redefinition collects at the wall, deletes its stale
/// definitions without allocating and can be refused before the module is
/// built: the next start collects what it deleted.
#[test]
fn a_typed_redefinition_refused_at_the_wall_leaves_the_next_start_to_collect() {
    let engine = wall_engine(8);
    let mut session = Session::new(&engine).expect("session");
    session
        .define(&HostModule::untyped("", &["h", "hf"]))
        .expect("defined");
    let typed = HostModule {
        name: "typed".to_owned(),
        functions: (0..8)
            .map(|i| HostSignature {
                name: format!("fn{i}"),
                params: vec![HostParam {
                    name: "key".to_owned(),
                    annotation: "str".to_owned(),
                    ..HostParam::default()
                }],
                returns: "int".to_owned(),
                doc: format!("doc of fn{i}"),
                ..HostSignature::default()
            })
            .collect(),
        ..HostModule::default()
    };
    session.define(&typed).expect("defined");
    assert_eq!(
        wall_cell(&engine, &mut session, &fill_with("str(len(xs))")),
        "ok"
    );
    for i in 0..30 {
        wall_cell(&engine, &mut session, &format!("q{i} = 1"));
    }
    assert!(session.define(&typed).is_err());
    assert_eq!(wall_cell(&engine, &mut session, "r0 = 1"), "ok");
}

/// Each way between cells that allocates or releases leaves the heap
/// not collected, and the next start at the wall collects once; a start
/// refused at its first object, a snapshot and a cancel with no cell
/// leave it collected. The bytes space is full, so a 200 kB `bytes`
/// literal is refused at its first object while the host actions, which
/// allocate words, succeed.
#[test]
fn each_way_between_cells_that_allocates_or_releases_makes_the_next_start_collect() {
    let engine = wall_engine(6);
    let start = |session: &mut Session, src: &str| {
        let before = session.gc_collections();
        let outcome = wall_cell(&engine, session, src);
        (outcome, session.gc_collections() - before)
    };
    let me = |n| ("MemoryError".to_owned(), n);
    let fill = "def fill():\n    xs = []\n    try:\n        while True:\n            xs.append(bytes(1000))\n    except MemoryError:\n        pass\n    return xs\nxs = fill()";
    let mut session = Session::new(&engine).expect("session");
    assert_eq!(start(&mut session, fill).0, "ok");
    let refused = format!("b = b\"{}\"", "b".repeat(200_000));
    assert_eq!(start(&mut session, &refused), me(1));
    assert_eq!(start(&mut session, &refused), me(0));
    let tools = |f: &[&str]| HostModule::untyped("tools", f);
    session.define(&tools(&["a"])).expect("defined");
    assert_eq!(start(&mut session, &refused), me(1));
    session.define(&tools(&["a", "b"])).expect("defined");
    assert_eq!(start(&mut session, &refused), me(1));
    session
        .set_hint(HintGroup::Network, Some("x"))
        .expect("set");
    assert_eq!(start(&mut session, &refused), me(1));
    session.set_hint(HintGroup::Network, None).expect("set");
    assert_eq!(start(&mut session, &refused), me(1));
    session.set_module_dunders(Some("m"), None).expect("set");
    assert_eq!(start(&mut session, &refused), me(1));
    session.set_module_dunders(None, None).expect("set");
    assert_eq!(start(&mut session, &refused), me(1));
    assert_eq!(start(&mut session, &refused), me(0));
    // Refused part way: its first constant is garbage.
    let part = format!("t = 'part way'\n{refused}");
    assert_eq!(start(&mut session, &part), me(0));
    assert_eq!(start(&mut session, &refused), me(1));
    drop(session.snapshot().expect("snapshot"));
    assert_eq!(start(&mut session, &refused), me(0));
    assert!(!session.cancel());
    assert_eq!(start(&mut session, &refused), me(0));
}

/// A cell start at the wall collects once per cell that ran since the last
/// collection: a start refused in its load after its collection ran no
/// cell, so the next does not collect again (a retry of the refused load
/// collected a second time: 188–232 ms a start on a 256 MiB live heap
/// instead of 98–105 [M]), nor does one after `Session::compile` collected,
/// while each start after a cell that ran collects, whether that cell
/// dropped data, failed or did nothing. A restored heap starts as not
/// collected: its first start at the wall collects once, which frees a
/// stopped cell it ends without resuming. The collection at a start also
/// serves the one the last cell's `MemoryError` asked for: the cell's first
/// safepoint does not collect again.
#[test]
fn a_cell_start_at_the_wall_collects_once_per_cell_that_ran() {
    let engine = Engine::new(EngineConfig::new().with_limits(Limits::new().with_memory(6 << 20)));
    let start = |session: &mut Session, src: &str| {
        let code = engine.compile(src, "cell.py").expect("compiles");
        let before = session.gc_collections();
        let outcome = match session.start(&code) {
            Progress::Complete(_) => "ok".to_owned(),
            Progress::Error(e) => e.type_name,
            p => panic!("{p:?}"),
        };
        (outcome, session.gc_collections() - before)
    };
    let me = |n| ("MemoryError".to_owned(), n);
    let ok = |n| ("ok".to_owned(), n);
    let mut session = Session::new(&engine).expect("session");
    // The fill runs in a function the session keeps, so its code stays
    // live: reclamation at the wall frees nothing either.
    let filling = |item: &str| {
        format!(
            "def fill():\n    xs = []\n    try:\n        while True:\n            xs.append({item})\n    except MemoryError:\n        pass\n    return xs\nxs = fill()"
        )
    };
    let fill = &filling("str(len(xs))");
    assert_eq!(start(&mut session, fill).0, "ok");
    // Live data at the wall: the collection frees nothing.
    assert_eq!(start(&mut session, "y = 1"), me(1));
    assert_eq!(start(&mut session, "y = 1"), me(0));
    let before = session.gc_collections();
    assert!(session.compile(&"z = 1\n".repeat(400), "big.py").is_err());
    assert_eq!(session.gc_collections() - before, 1);
    assert_eq!(start(&mut session, "y = 1"), me(0));
    let image = session.snapshot().expect("snapshot");
    let mut session = engine.restore(&image).expect("restore");
    assert_eq!(start(&mut session, "y = 1"), me(1));
    assert_eq!(start(&mut session, "y = 1"), me(0));
    assert_eq!(start(&mut session, "xs = None"), ok(0));
    assert_eq!(start(&mut session, "y = 1"), ok(1));
    assert_eq!(start(&mut session, "y = 1"), ok(0));
    // Cells that load, run and fail at the wall: each next start collects.
    let mut session = Session::new(&engine).expect("session");
    assert_eq!(start(&mut session, fill).0, "ok");
    assert_eq!(start(&mut session, "y = 1"), me(1));
    // Its names are interned already, so it loads at the wall.
    let grow = "while True:\n    xs.append(str(len(xs)))";
    assert_eq!(start(&mut session, grow), me(0));
    assert_eq!(start(&mut session, grow), me(1));
    assert_eq!(start(&mut session, grow), me(1));
    // A fill that raised: the next cell's safepoints do not collect again.
    let mut session = Session::new(&engine).expect("session");
    let raises = "xs = []\nwhile True:\n    xs.append(str(len(xs)))";
    assert_eq!(start(&mut session, raises).0, "MemoryError");
    assert_eq!(start(&mut session, "del xs"), ok(1));
    let looped = "for _ in range(3):\n    pass";
    assert_eq!(start(&mut session, looped), ok(1));
    assert_eq!(start(&mut session, looped), ok(0));
    // A cell stopped at the wall with its data live, restored and ended
    // without a resume: the first start collects once and frees it.
    let stopped = "import tools\ndef build():\n    xs = []\n    try:\n        while True:\n            xs.append(str(len(xs)))\n    except MemoryError:\n        pass\n    tools.a()\nbuild()";
    for cancel in [false, true] {
        let mut session = Session::new(&engine).expect("session");
        session
            .define(&HostModule::untyped("tools", &["a"]))
            .expect("defined");
        let code = engine.compile(stopped, "fill.py").expect("compiles");
        assert!(matches!(session.start(&code), Progress::HostCall { .. }));
        let image = session.snapshot().expect("snapshot");
        let mut session = engine.restore(&image).expect("restore");
        if cancel {
            assert!(session.cancel());
        }
        assert_eq!(start(&mut session, "y = 1"), ok(1), "cancel {cancel}");
        assert_eq!(start(&mut session, "y = 1"), ok(0), "cancel {cancel}");
    }
    // Near the limit but short of the wall (4 MiB of 16 left, a segment
    // is 1 MiB): no start collects.
    let engine = Engine::new(EngineConfig::new().with_limits(Limits::new().with_memory(16 << 20)));
    let mut session = Session::new(&engine).expect("session");
    let fill = engine
        .compile("big = [bytes(4000) for _ in range(3000)]", "fill.py")
        .expect("compiles");
    assert!(matches!(session.start(&fill), Progress::Complete(_)));
    let code = engine.compile("y = 1", "cell.py").expect("compiles");
    let before = session.gc_collections();
    for _ in 0..20 {
        assert!(matches!(session.start(&code), Progress::Complete(_)));
    }
    assert_eq!(session.gc_collections() - before, 0);
}

/// The fill cell may stop and continue on another path than a start: a
/// resume after a park or after a host call's answer, in the same session
/// or in one restored from a snapshot taken there. A restored heap starts as
/// collected, and a fill that only resumed there left it so: the next
/// start neither collected nor retried, and no cell ran again.
#[test]
fn cells_after_a_fill_that_resumed_run_once_its_garbage_is_collected() {
    let engine = Engine::new(EngineConfig::new().with_limits(Limits::new().with_memory(6 << 20)));
    let fill = "import tools\ndef build():\n    try:\n        tools.a()\n    except KeyError:\n        pass\n    xs = []\n    while True:\n        xs.append(str(len(xs)))\nbuild()";
    let answers = [
        Resume::Value(HostValue::None),
        Resume::Error(HostError {
            kind: HostErrorKind::KeyError,
            message: "k".into(),
            data: None,
            class: None,
        }),
    ];
    for answer in answers {
        for restore_at_call in [false, true] {
            for restore_at_park in [false, true] {
                let mut session = Session::new(&engine).expect("session");
                session
                    .define(&HostModule::untyped("tools", &["a"]))
                    .expect("defined");
                let code = engine.compile(fill, "fill.py").expect("compiles");
                let p = session.start(&code);
                assert!(matches!(p, Progress::HostCall { .. }), "{p:?}");
                if restore_at_call {
                    let image = session.snapshot().expect("snapshot");
                    session = engine.restore(&image).expect("restore");
                }
                session.set_fuel(Some(100_000));
                let mut p = session.resume(answer.clone()).expect("resumes");
                let mut parks = 0;
                while let Progress::Parked { .. } = p {
                    parks += 1;
                    if restore_at_park && parks == 1 {
                        let image = session.snapshot().expect("snapshot");
                        session = engine.restore(&image).expect("restore");
                    }
                    session.set_fuel(Some(100_000));
                    p = session.resume(Resume::Continue).expect("resumes");
                }
                let case = format!(
                    "{answer:?}, restored at call {restore_at_call}, at park {restore_at_park}"
                );
                assert!(parks > 0, "{case}");
                assert!(
                    matches!(&p, Progress::Error(e) if e.type_name == "MemoryError"),
                    "{case}: {p:?}"
                );
                session.set_fuel(None);
                for next in ["print('next')", "x = 1"] {
                    let code = engine.compile(next, "cell.py").expect("compiles");
                    let p = session.start(&code);
                    assert!(matches!(p, Progress::Complete(_)), "{case}: {next}: {p:?}");
                }
            }
        }
    }
    // A cell stopped with the heap at its wall and its data still live,
    // ended without being resumed: by the next start, a cancel or a reset,
    // in the same session or in one restored from a snapshot taken there.
    let fill = "import tools\ndef build():\n    xs = []\n    try:\n        while True:\n            xs.append(str(len(xs)))\n    except MemoryError:\n        pass\n    tools.a()\n    while True:\n        pass\nbuild()";
    for end in ["start", "cancel", "reset"] {
        for restore in [false, true] {
            let mut session = Session::new(&engine).expect("session");
            session
                .define(&HostModule::untyped("tools", &["a"]))
                .expect("defined");
            let code = engine.compile(fill, "fill.py").expect("compiles");
            let p = session.start(&code);
            assert!(matches!(p, Progress::HostCall { .. }), "{p:?}");
            session.set_fuel(Some(1_000));
            let p = session
                .resume(Resume::Value(HostValue::None))
                .expect("resumes");
            assert!(matches!(p, Progress::Parked { .. }), "{p:?}");
            if restore {
                let image = session.snapshot().expect("snapshot");
                session = engine.restore(&image).expect("restore");
            }
            session.set_fuel(None);
            match end {
                "cancel" => assert!(session.cancel()),
                "reset" => session.reset().expect("reset"),
                _ => {}
            }
            for next in ["print('next')", "x = 1"] {
                let code = engine.compile(next, "cell.py").expect("compiles");
                let p = session.start(&code);
                assert!(
                    matches!(p, Progress::Complete(_)),
                    "abandoned by {end}, restored {restore}: {next}: {p:?}"
                );
            }
        }
    }
}

#[test]
fn compiling_a_cell_is_charged_to_the_memory_left() {
    let engine = Engine::new(EngineConfig::new().with_limits(Limits::new().with_memory(16 << 20)));
    let mut session = Session::new(&engine).expect("session");
    let run = |session: &mut Session, src: &str| {
        let code = session.compile(src, "cell.py").expect("compiles");
        match session.start(&code) {
            Progress::Complete(v) => v.map(|v| v.repr),
            p => panic!("{src}: {p:?}"),
        }
    };
    // 12 of the 16 MiB in live strings.
    run(
        &mut session,
        "big = ['x' * 4096 + str(i) for i in range(3000)]",
    );
    // 16 KiB of source reserves 8 MiB while it compiles: more than is left.
    let cell = "y = 1\n".repeat(2731) + "y";
    let e = session.compile(&cell, "cell.py").expect_err("no room");
    assert_eq!(
        (e.type_name.as_str(), e.rendered.as_str()),
        ("MemoryError", "MemoryError\n")
    );
    // Outside the session it compiles: only the charge refused it.
    assert!(engine.compile(&cell, "cell.py").is_ok());
    assert_eq!(run(&mut session, "len(big)").as_deref(), Some("3000"));
    // Freed, then collected by the compile that needs the room.
    run(&mut session, "del big");
    assert_eq!(run(&mut session, &cell).as_deref(), Some("1"));
    // The largest cell, the worst kind, fits a fresh session at the default
    // limit: 128 MiB of scratch, not an object (no 64 MiB object cap).
    let engine = Engine::new(EngineConfig::default());
    let mut session = Session::new(&engine).expect("session");
    let largest = "1;".repeat(128 << 10);
    assert!(session.compile(&largest, "cell.py").is_ok());
}

#[test]
fn retained_code_shares_the_memory_budget_and_restore_limit() {
    let engine = Engine::new(EngineConfig::new().with_limits(Limits::new().with_memory(8 << 20)));
    let mut session = Session::new(&engine).expect("session");
    let init = engine.compile("keep = []", "init.py").expect("code");
    assert_eq!(session.start(&init), Progress::Complete(None));
    let source = format!(
        "#{}\ndef f():\n    return 17\nkeep.append(f)",
        "x".repeat(200 * 1024)
    );
    let mut exhausted = false;
    for i in 0..64 {
        // Different source identities, with live functions retaining each unit.
        let code = engine
            .compile(&source, &format!("live-{i}.py"))
            .expect("code");
        match session.start(&code) {
            Progress::Complete(_) => {}
            Progress::Error(e) => {
                assert_eq!(e.type_name, "MemoryError");
                exhausted = true;
                break;
            }
            other => panic!("{other:?}"),
        }
    }
    assert!(
        exhausted,
        "retained source must not escape the memory budget"
    );
    let snapshot = session.snapshot().expect("snapshot");
    let smaller = Engine::new(EngineConfig::new().with_limits(Limits::new().with_memory(4 << 20)));
    assert!(matches!(
        smaller.restore(&snapshot),
        Err(micro_monty::SnapshotError::TooLarge)
    ));
    session = engine
        .restore(&snapshot)
        .expect("restore within original budget");
    // Restoring the reservation must also leave less room for arena objects.
    let allocate = engine
        .compile("'y' * (1 << 20)", "allocate.py")
        .expect("code");
    let Progress::Error(e) = session.start(&allocate) else {
        panic!("code and arena must share the limit after restore")
    };
    assert_eq!(e.type_name, "MemoryError");
    let clear = engine
        .compile(
            "assert all(fn() == 17 for fn in keep)\nkeep.clear()\ndel f",
            "clear.py",
        )
        .expect("code");
    assert_eq!(session.start(&clear), Progress::Complete(None));
    let code = engine.compile(&source, "recovered.py").expect("code");
    assert!(matches!(session.start(&code), Progress::Complete(_)));
    let check = engine
        .compile("[len(keep), keep[0]()]", "check.py")
        .expect("code");
    let Progress::Complete(Some(value)) = session.start(&check) else {
        panic!("retained function remains usable")
    };
    assert_eq!(value.repr, "[1, 17]");
}

#[test]
fn code_at_the_memory_limit_can_restore_in_the_same_budget() {
    let engine =
        Engine::new(EngineConfig::new().with_limits(Limits::new().with_memory(1088 << 10)));
    let mut session = Session::new(&engine).expect("session");
    let init = engine.compile("keep = []", "init.py").expect("code");
    assert_eq!(session.start(&init), Progress::Complete(None));
    let constants = (0..100)
        .map(|i| format!("'constant{i}'"))
        .collect::<Vec<_>>()
        .join(",");
    let code = engine
        .compile(
            &format!("def f():\n    return [{constants}]\nkeep.append(f)"),
            "live.py",
        )
        .expect("code");
    for _ in 0..128 {
        match session.start(&code) {
            Progress::Complete(_) => {}
            Progress::Error(e) => {
                assert_eq!(e.type_name, "MemoryError");
                let snapshot = session.snapshot().expect("snapshot");
                session = engine.restore(&snapshot).expect("same budget restore");
                session.reset().expect("reset releases retained functions");
                assert_eq!(session.start(&init), Progress::Complete(None));
                assert!(matches!(session.start(&code), Progress::Complete(_)));
                return;
            }
            other => panic!("{other:?}"),
        }
    }
    panic!("code buffers must exhaust the budget");
}

/// Garbage not yet collected counts against the memory limit, and natives
/// cannot collect: a loop of results a third of the limit each used to
/// raise `MemoryError` on its third pass. A method of an immutable builtin
/// type that runs out of memory runs again after a collection; one whose
/// result cannot fit still raises, and the session goes on.
#[test]
fn a_native_out_of_memory_runs_again_after_collecting_garbage() {
    let engine = Engine::new(EngineConfig::new().with_limits(Limits::new().with_memory(64 << 20)));
    let mut session = Session::new(&engine).expect("session");
    let mut run = |src: &str| {
        let code = engine.compile(src, "cell.py").expect("compiles");
        match session.start(&code) {
            Progress::Complete(c) => c.map_or_default(|c| c.repr),
            Progress::Error(e) => e.type_name,
            other => panic!("{other:?}"),
        }
    };
    // Each result is about a sixth of the limit; a pass holds the last
    // one and builds the next in a buffer. All three raised before.
    let setup = "xs = ['x' * 64] * 180_000\nbs = [b'x' * 64] * 180_000\nt = 'y' * 10_800_000";
    assert_eq!(run(setup), "");
    assert_eq!(
        run("for i in range(12):\n    s = '-'.join(xs)\nlen(s)"),
        "11699999"
    );
    assert_eq!(
        run("del s\nfor i in range(12):\n    b = b'-'.join(bs)\nlen(b)"),
        "11699999"
    );
    assert_eq!(
        run("del b\nfor i in range(12):\n    u = t.upper()\nlen(u)"),
        "10800000"
    );
    assert_eq!(run("'-'.join(['x' * 64] * 900_000)"), "MemoryError");
    assert_eq!(run("len('-'.join(xs))"), "11699999");
}

#[test]
fn code_reclamation_keeps_closures_generators_and_tracebacks() {
    let mut c = Cells::new();
    assert_eq!(c.value("def obsolete():\n    return 91\nobsolete()"), "91");
    assert_eq!(c.value("def outer(x):\n    def inner(y):\n        return x + y\n    return inner\nf = outer(17)\ndef gen():\n    yield 3\n    yield f(5)\ng = gen()\nnext(g)"), "3");
    assert_eq!(
        c.value("try:\n    1 / 0\nexcept Exception as e:\n    saved_error = e\n42"),
        "42"
    );
    // These bundled and synthesized functions are loaded after user code;
    // their indices must move too when the earlier compilation unit dies.
    assert_eq!(c.value("from collections import namedtuple\nPair = namedtuple('Pair', 'x y')\ndel obsolete\nPair(4, 7).y"), "7");
    for i in 0..16 {
        assert_eq!(c.value(&format!("{i} + 1")), (i + 1).to_string());
    }
    let snapshot = c.session.snapshot().expect("snapshot");
    c.session = c.engine.restore(&snapshot).expect("restore");
    assert_eq!(c.value("[f(2), next(g), outer(40)(2)]"), "[19, 22, 42]");
    assert_eq!(c.value("Pair(8, 9).x"), "8");
    let Progress::Error(e) = c.start("raise saved_error") else {
        panic!("expected saved exception")
    };
    assert!(e.traceback.contains("1 / 0"), "{}", e.traceback);
    assert!(e.traceback.contains("raise saved_error"), "{}", e.traceback);
}

#[test]
fn discarded_generated_methods_are_reclaimed_between_cells_and_restores() {
    let mut c = Cells::new();
    assert_eq!(
        c.value("from collections import namedtuple\nfields = ['f' + str(i) for i in range(32)]\nKept = namedtuple('Kept', fields)\nkept = Kept(*range(32))\nkept.f31"),
        "31"
    );
    assert_eq!(c.start("pass"), Progress::Complete(None));
    let initial = c.session.snapshot().expect("snapshot").len();
    for _ in 0..3 {
        for i in 0..32 {
            assert_eq!(
                c.start(&format!("Transient = namedtuple('Transient', fields[{i}:] + fields[:{i}])\nassert Transient(*range(32))[31] == 31\ndel Transient")),
                Progress::Complete(None)
            );
        }
        assert_eq!(c.start("pass"), Progress::Complete(None));
        let image = c.session.snapshot().expect("snapshot");
        assert!(image.len() < initial + 4096, "{initial} -> {}", image.len());
        c.session = c.engine.restore(&image).expect("restore");
        assert_eq!(c.value("[Kept(*range(32)).f17, kept.f31]"), "[17, 31]");
    }
    c.session.reset().expect("reset");
    assert_eq!(
        c.value("from collections import namedtuple\nPair = namedtuple('Pair', 'left right')\nPair(2, 9).right"),
        "9"
    );
}

#[test]
fn wide_instances_do_not_retain_quadratic_attribute_metadata() {
    let mut c = Cells::new();
    assert_eq!(c.start("class Wide: pass\nprimed = Wide()\nprimed.field0 = 0\nw = Wide()\nw.field0 = 'x' * (1 << 20)\nfor i in range(1, 2000):\n    setattr(w, 'field' + str(i), i)\nw.field0 = 0\nassert sum(getattr(w, 'field' + str(i)) for i in range(2000)) == 1999000"), Progress::Complete(None));
    let image = c.session.snapshot().expect("snapshot");
    assert!(image.len() < 1 << 20, "{} bytes", image.len());
    c.session = c.engine.restore(&image).expect("restore");
    assert_eq!(c.value("w.field31 += 7\nw.field32 += 9\ndel w.field30\nw.field30 = -1\n[w.field31, w.field32, w.field1999, list(vars(w))[-1]]"), "[38, 41, 1999, 'field30']");
}

#[test]
fn transient_instance_shapes_are_reclaimed_without_changing_live_attributes() {
    let mut c = Cells::new();
    assert_eq!(c.start("class C: pass\nfields = ['field' + str(i) for i in range(64)]\njunk = C()\nfor i, name in enumerate(fields):\n    setattr(junk, name, i)\nkept = C()\nkept.field61 = 17\nkept.field7 = 93\ndel junk\ndef read(o):\n    return [o.field61, o.field7]"), Progress::Complete(None));
    assert_eq!(c.start("pass"), Progress::Complete(None));
    let initial = c.session.snapshot().expect("snapshot").len();
    for _ in 0..2 {
        for i in 0..64 {
            assert_eq!(c.value(&format!("temporary = C()\nfor name in (fields[{i}:] + fields[:{i}])[:12]:\n    setattr(temporary, name, 1)\ndel temporary\nread(kept)")), "[17, 93]");
        }
        assert_eq!(c.start("pass"), Progress::Complete(None));
        let image = c.session.snapshot().expect("snapshot");
        assert!(image.len() < initial + 4096, "{initial} -> {}", image.len());
        c.session = c.engine.restore(&image).expect("restore");
        assert_eq!(c.value("kept.field7 += 1\nread(kept)"), "[17, 94]");
        assert_eq!(c.start("kept.field7 = 93"), Progress::Complete(None));
    }
}

#[test]
fn bundled_modules_import() {
    let mut c = Cells::new();
    let src = "\
import asyncio
import sys as system
from asyncio import gather, sleep as nap
print('out', file=system.stdout)
print('err', 1, file=system.stderr)
n = system.stderr.write('raw\\n')
async def one():
    await nap(0)
    return 1
r = await gather(one(), one())
[r, n, asyncio.run(one()), repr(system.stdout)]
";
    assert_eq!(
        c.value(src),
        "[[1, 1], 4, 1, \"<_io.TextIOWrapper name='<stdout>' mode='w' encoding='utf-8'>\"]"
    );
    assert_eq!(c.take_out(), "out\n[err]err 1\n[err]raw\n");
    assert_eq!(
        c.error("import tkinter"),
        (
            "ModuleNotFoundError".into(),
            "No module named 'tkinter'".into()
        )
    );
    assert_eq!(
        c.error("import sys.path"),
        (
            "ModuleNotFoundError".into(),
            "No module named 'sys.path'; 'sys' is not a package".into()
        )
    );
    assert_eq!(
        c.error("from asyncio import nothing"),
        (
            "ImportError".into(),
            "cannot import name 'nothing' from 'asyncio' (unknown location)".into()
        )
    );
    assert_eq!(
        c.error("from . import x"),
        (
            "ImportError".into(),
            "attempted relative import with no known parent package".into()
        )
    );
    assert_eq!(
        c.value("try:\n    import nope\nexcept ImportError as e:\n    r = type(e).__name__\nr"),
        "'ModuleNotFoundError'"
    );
}

/// Runs `cells` in one session under `stress` (`""`, `"full"` or `"inc"`),
/// with nothing, a snapshot or a restore (`between`: 0, 1 or 2) between
/// them; each cell's value and fuel.
fn generator_cells(
    engine: &Engine,
    cells: &[&str],
    stress: &str,
    between: u8,
) -> Vec<(String, u64)> {
    let set = |s: &mut Session| match stress {
        "full" => s.set_gc_stress(true),
        "inc" => s.set_gc_inc_stress(true),
        _ => {}
    };
    let mut session = Session::new(engine).expect("session");
    set(&mut session);
    session.set_fuel_total(Some(1 << 40));
    let mut run = Vec::new();
    for (k, cell) in cells.iter().enumerate() {
        if between > 0 && k > 0 {
            let image = session.snapshot().expect("snapshot");
            if between == 2 {
                session = engine.restore(&image).expect("restore");
                set(&mut session);
            }
        }
        let before = session.fuel_total().expect("total");
        let code = engine.compile(cell, "cell.py").expect("compiles");
        let p = session.start(&code);
        let Progress::Complete(v) = p else {
            panic!("stress {stress:?}, between {between}, cell {k}: {p:?}")
        };
        let repr = v.map_or_else(|| "None".into(), |v| v.repr);
        run.push((repr, before - session.fuel_total().expect("total")));
    }
    run
}

#[test]
fn a_generator_suspended_in_an_earlier_cell_keeps_no_dead_register_at_the_wall() {
    // The generator yields, in cell 0 and again in cell 1, with the list
    // `load()` made dead in a register; under the incremental stress mode a
    // mark is in progress at each yield. Cell 2 fits in 6 MiB only if a
    // collection clears the suspended generator's saved copy, whether or not
    // a snapshot was taken between the cells, with the same fuel either way.
    let engine = Engine::new(EngineConfig::new().with_limits(Limits::new().with_memory(6 << 20)));
    let cells = [
        "def load():\n    return [bytearray(700_000) for _ in range(5)]\ndef gen():\n    n = len(load())\n    yield n\n    yield 0\n    yield 1\ng = gen()\nnext(g)",
        "next(g)",
        "len(load())",
    ];
    for stress in ["", "full", "inc"] {
        let plain = generator_cells(&engine, &cells, stress, 0);
        assert_eq!(plain[2].0, "5", "stress {stress:?}");
        assert_eq!(
            plain,
            generator_cells(&engine, &cells, stress, 2),
            "stress {stress:?}"
        );
    }
}

#[test]
fn a_generator_at_a_code_index_reclamation_reused_gets_its_own_dead_registers() {
    // Cell 2's long comment makes the start reclaim code: `gx` goes and
    // `gy` takes its index, at the same yield pc. The reclamation's
    // collection cleared `g1` and `g2` first; a dead set it remembered
    // under `gx`'s index would clear `g3`'s live `a`, `b` and `c`.
    let engine = Engine::new(EngineConfig::new());
    let pad = format!("#{}\ndel g1, gx\n0", "x".repeat(20_000));
    let cells = [
        "def gx():\n    yield 1\n    return 0\ng1 = gx()\nnext(g1)",
        "def gy(a, b, c):\n    yield 1\n    return (a, b, c)\ng2 = gy(1, 2, 3)\nnext(g2)",
        &pad,
        "import gc\ng3 = gy(4, 5, 6)\nnext(g3)\ngc.collect()\ntry:\n    next(g3)\n    r = 'no stop'\nexcept StopIteration as e:\n    r = e.value\nr",
    ];
    for stress in ["", "full", "inc"] {
        let plain = generator_cells(&engine, &cells, stress, 0);
        assert_eq!(plain[3].0, "(4, 5, 6)", "stress {stress:?}");
        for between in [1, 2] {
            assert_eq!(
                plain,
                generator_cells(&engine, &cells, stress, between),
                "stress {stress:?}"
            );
        }
    }
}

#[test]
fn the_clearing_pass_before_a_mark_lets_no_guest_code_run_until_it_ends() {
    // 40,000 generators each suspended again since the last collection
    // with 200 registers saved make the pass before the next mark run in
    // over a hundred slices. The guest growing `xs` meanwhile would
    // allocate past what the due collection left room for; it waits, and
    // the loop completes as it does without generators. Run it in slices
    // of fuel too: a park between two slices of the pass changes nothing.
    let engine = Engine::new(EngineConfig::new());
    let args = (1..200)
        .map(|i| i.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    let params = (0..200)
        .map(|i| format!("a{i}"))
        .collect::<Vec<_>>()
        .join(", ");
    let src = format!(
        "import gc\nkeep = [(i, i) for i in range(600000)]\ndef use({params}):\n    return 1\ndef filler():\n    n = 0\n    while True:\n        n += use((yield n), {args})\nfillers = [filler() for _ in range(40000)]\nfor f in fillers:\n    next(f)\ngc.collect()\nfor f in fillers:\n    f.send(0)\nbig = bytes(3_000_000)\ni = 0\nwhile i < 6:\n    xs = []\n    xs.extend(big)\n    i += 1\n('ok', i, len(fillers), len(keep))"
    );
    let code = engine.compile(&src, "cell.py").expect("compiles");
    let mut fuel = Vec::new();
    for slice in [None, Some(997_u64)] {
        let mut session = Session::new(&engine).expect("session");
        session.set_fuel_total(Some(1 << 40));
        session.set_fuel(slice);
        let mut p = session.start(&code);
        while let Progress::Parked { .. } = p {
            session.set_fuel(slice);
            p = session.resume(Resume::Continue).expect("resume");
        }
        let Progress::Complete(Some(v)) = p else {
            panic!("slice {slice:?}: {p:?}")
        };
        assert_eq!(v.repr, "('ok', 6, 40000, 600000)", "slice {slice:?}");
        fuel.push((1 << 40) - session.fuel_total().expect("total"));
    }
    assert_eq!(fuel[0], fuel[1]);
}

#[test]
fn a_retry_after_memory_error_clears_a_generator_that_yielded_while_a_pass_was_due() {
    // `first()` yields with its 3,000,000-item list dead in a register
    // while a collection, and the clearing pass before it, are due; the
    // retry of cell 1's allocation waits for a collection that clears it.
    let engine = Engine::new(EngineConfig::new().with_limits(Limits::new().with_memory(128 << 20)));
    let args = (1..200)
        .map(|i| i.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    let setup = format!(
        "import gc\nkeep = [(i, i) for i in range(450000)]\ndef use(*a):\n    return len(a)\ndef first():\n    n = len([[None] * 3_000_000, 1])\n    yield n\n    yield 0\ndef filler():\n    n = 0\n    while True:\n        n += use((yield n), {args})\ng = first()\nfillers = [filler() for _ in range(20000)]\nfor f in fillers:\n    next(f)\ngc.collect()\nfor f in fillers:\n    f.send(0)\na = next(g)"
    );
    let cells = [
        setup.as_str(),
        "m = len([[None] * 8_000_000, 1])\n('ok', a, m, len(keep), len(fillers))",
    ];
    for between in [0, 1, 2] {
        let run = generator_cells(&engine, &cells, "", between);
        assert_eq!(run[1].0, "('ok', 2, 2, 450000, 20000)", "between {between}");
    }
}

#[test]
fn a_cell_ended_between_two_slices_of_the_clearing_pass_leaves_no_part_way_pass() {
    // The collection due while the fillers suspend again clears their
    // 20,000 copies in slices, the guest waiting; under `gc_yield` a park
    // falls between slices. The host cancels cell 0 there, or starts
    // cell 1 over it: `g`, made first and so behind the pass's cursor,
    // then yields with `big` in a dead register. The retry of cell 1's
    // allocation waits for a collection whose pass must clear `g`'s copy.
    let engine = Engine::new(EngineConfig::new().with_limits(Limits::new().with_memory(128 << 20)));
    let args = (1..200)
        .map(|i| i.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    let setup = format!(
        "import gc\nkeep = [(i, i) for i in range(450000)]\ndef use(*a):\n    return len(a)\ndef first():\n    n = len([(yield 0), 1])\n    yield n - 1\n    yield 0\ndef filler():\n    n = 0\n    while True:\n        n += use((yield n), {args})\ng = first()\nnext(g)\nfillers = [filler() for _ in range(20000)]\nfor f in fillers:\n    next(f)\nbig = [None] * 3_000_000\ngc.collect()\nfor f in fillers:\n    f.send(0)"
    );
    let setup = engine.compile(&setup, "cell.py").expect("compiles");
    let next = engine
        .compile(
            "r = g.send(big)\ndel big\nm = len([[None] * 8_000_000, 1])\n('ok', r, m, len(keep), len(fillers))",
            "cell.py",
        )
        .expect("compiles");
    // Cell 0 run through, the fuel total at each park; then the park to
    // stop at: the middle of the last run of three or more parks with no
    // guest code between them, the sliced pass.
    let run = |stop: Option<(usize, bool)>| {
        let mut s = Session::new(&engine).expect("session");
        s.set_fuel_total(Some(1 << 40));
        s.set_gc_yield(true);
        let mut fuels = Vec::new();
        let mut p = s.start(&setup);
        while let Progress::Parked { .. } = p {
            fuels.push(s.fuel_total().expect("total"));
            if let Some((at, cancel)) = stop
                && fuels.len() == at
            {
                if cancel {
                    assert!(s.cancel());
                }
                break;
            }
            p = s.resume(Resume::Continue).expect("resume");
        }
        s.set_gc_yield(false);
        let Progress::Complete(Some(v)) = s.start(&next) else {
            panic!("stop {stop:?}: cell 1 did not complete")
        };
        (fuels, v.repr)
    };
    let (fuels, value) = run(None);
    assert_eq!(value, "('ok', 1, 2, 450000, 20000)");
    let mut held = None;
    let mut i = 0;
    while i < fuels.len() {
        let j = i + fuels[i..].iter().take_while(|&&f| f == fuels[i]).count();
        if j - i >= 3 {
            held = Some(usize::midpoint(i, j) + 1);
        }
        i = j;
    }
    let at = held.expect("a pass in slices");
    for cancel in [true, false] {
        assert_eq!(
            run(Some((at, cancel))).1,
            value,
            "park {at}, cancel {cancel}"
        );
    }
}

#[test]
fn the_generator_registry_is_charged_alike_with_and_without_a_snapshot() {
    // The registry's capacity, which is charged, follows its length alone:
    // 20,000 live generators leave cell 1 the same room whether or not a
    // snapshot or a restore came between, and `bytearray(3_350_000)` fits
    // in each. (Under `inc` the cell may wait for a collection after a
    // `MemoryError` where the snapshot's collection already ran, a retry
    // charge the parent pays too at a larger size; the value is the same.)
    let engine = Engine::new(EngineConfig::new().with_limits(Limits::new().with_memory(6 << 20)));
    let cells = [
        "def g():\n    yield 1\ngs = [g() for _ in range(20000)]\n0",
        "len(bytearray(3_350_000)), len(gs)",
    ];
    for stress in ["", "inc"] {
        let plain = generator_cells(&engine, &cells, stress, 0);
        assert_eq!(plain[1].0, "(3350000, 20000)", "stress {stress:?}");
        for between in [1, 2] {
            let run = generator_cells(&engine, &cells, stress, between);
            if stress.is_empty() {
                assert_eq!(run, plain, "between {between}");
            } else {
                assert_eq!(run[1].0, plain[1].0, "stress {stress:?}, between {between}");
            }
        }
    }
}

#[test]
fn the_generator_registry_gives_back_its_room_after_a_collection() {
    // 30,000 generators made and dropped leave the registry's capacity
    // charged until a collection prunes it and gives the room back; cell 1
    // fits in 6 MiB only then, with or without a snapshot between. (Under
    // the full stress mode each of the 30,000 makes a collection that
    // walks the registry: minutes in a debug build.)
    let engine = Engine::new(EngineConfig::new().with_limits(Limits::new().with_memory(6 << 20)));
    let cells = [
        "def g():\n    yield 1\ngs = [g() for _ in range(30000)]\ndel gs\n0",
        "len(bytearray(3_950_000))",
    ];
    for stress in ["", "inc"] {
        for between in [0, 1, 2] {
            let run = generator_cells(&engine, &cells, stress, between);
            assert_eq!(run[1].0, "3950000", "stress {stress:?}, between {between}");
        }
    }
}

#[test]
fn a_coroutine_suspended_by_send_resumes_in_a_later_cell_and_after_restore() {
    let mut c = Cells::new();
    let src = "\\
class Step:
    def __await__(self):
        return (yield 'step')
async def inner():
    return await Step()
async def outer():
    a = await inner()
    b = await inner()
    return a + b
co = outer()
first = co.send(None)
";
    assert_eq!(c.start(src), Progress::Complete(None));
    assert_eq!(c.value("first, co.send('x')"), "('step', 'step')");
    let image = c.session.snapshot().expect("snapshot");
    let mut restored = c.engine.restore(&image).expect("restore");
    let code = c
        .engine
        .compile(
            "try:\n    co.send('y')\nexcept StopIteration as e:\n    r = e.value\nr",
            "cell.py",
        )
        .expect("code");
    for s in [&mut c.session, &mut restored] {
        let Progress::Complete(Some(v)) = s.start(&code) else {
            panic!("expected completion")
        };
        assert_eq!(v.repr, "'xy'");
    }
}

#[test]
fn warnings_print_to_stderr_or_raise() {
    let mut c = Cells::new();
    assert_eq!(
        c.value("[issubclass(DeprecationWarning, Warning), issubclass(Warning, Exception), EncodingWarning.__mro__[1].__name__]"),
        "[True, True, 'Warning']"
    );
    // A warning names the caller's file and line; the module is
    // `__main__` (so the default filters show DeprecationWarning and hide
    // PendingDeprecationWarning), the "default" action shows a message once
    // per line, and a stacklevel past the stack gives CPython's `<sys>:0`.
    let src = "\
import warnings
for i in range(2):
    warnings.warn('first')
warnings.warn('shown', DeprecationWarning)
warnings.warn('off', stacklevel=3)
warnings.warn('hidden', PendingDeprecationWarning)
warnings.simplefilter('error', RuntimeWarning)
try:
    warnings.warn('raised', RuntimeWarning)
except RuntimeWarning as e:
    print('caught', e)
";
    assert_eq!(c.start(src), Progress::Complete(None));
    assert_eq!(
        c.take_out(),
        "[err]cell.py:3: UserWarning: first\n[err]cell.py:4: DeprecationWarning: shown\n[err]<sys>:0: UserWarning: off\ncaught raised\n"
    );
}

#[test]
fn host_module_attributes_are_host_functions() {
    let mut c = Cells::new();
    c.session
        .define(&HostModule {
            constants: [("LIMIT", HostValue::Int(3))]
                .iter()
                .map(|(k, v)| ((*k).to_owned(), v.clone()))
                .collect(),
            ..HostModule::untyped("tools", &["add"])
        })
        .expect("defined");
    c.session
        .define(&HostModule::untyped("tools.extra", &["echo"]))
        .expect("defined");
    let src = "\
import tools
from tools.extra import echo
r = [tools.LIMIT, tools.add(1, 2), echo('x'), tools.extra is not tools]
r
";
    let mut p = c.start(src);
    let mut names = Vec::new();
    while let Progress::HostCall { name, args, .. } = p {
        names.push(name);
        let answer = args.into_iter().next().unwrap_or(HostValue::None);
        p = c.session.resume(Resume::Value(answer)).expect("resumes");
    }
    assert_eq!(names, ["tools.add", "tools.extra.echo"]);
    let Progress::Complete(Some(v)) = p else {
        panic!("{p:?}")
    };
    assert_eq!(v.repr, "[3, 1, 'x', True]");
    assert_eq!(
        c.error("import tools\ntools.nope"),
        (
            "AttributeError".into(),
            "module 'tools' has no attribute 'nope'".into()
        )
    );
}

#[test]
fn cancelling_a_stopped_cell_keeps_the_session_usable() {
    let mut c = Cells::new();
    c.session
        .define(&HostModule::untyped("", &["tool"]))
        .expect("defined");
    // At a host call.
    let p = c.start("before = 1\ndef f():\n    return tool()\nf()\nafter = 2");
    assert!(matches!(p, Progress::HostCall { .. }), "{p:?}");
    assert!(c.session.cancel());
    assert!(!c.session.cancel(), "nothing left to cancel");
    assert!(c.session.resume(Resume::Value(HostValue::None)).is_err());
    assert_eq!(c.value("before"), "1");
    assert_eq!(
        c.error("after"),
        ("NameError".into(), "name 'after' is not defined".into())
    );
    // Parked inside a task.
    let src = "\
import asyncio
async def spin():
    t = 0
    for i in range(100000):
        t += i
    return t
r = await asyncio.gather(spin(), spin())
";
    c.session.set_fuel(Some(1_000));
    let p = c.start(src);
    assert_eq!(
        p,
        Progress::Parked {
            reason: ParkReason::Fuel
        }
    );
    assert!(c.session.cancel());
    c.session.set_fuel(None);
    // Waiting on host futures.
    let p = c.start("import asyncio\nr = await asyncio.gather(tool(), tool())");
    let Progress::HostCall { .. } = p else {
        panic!("{p:?}")
    };
    let p = c.session.resume(Resume::Future).expect("resumes");
    let p = match p {
        Progress::HostCall { .. } => c.session.resume(Resume::Future).expect("resumes"),
        other => other,
    };
    assert!(matches!(p, Progress::Waiting { .. }), "{p:?}");
    assert!(c.session.cancel());
    assert_eq!(
        c.value("async def g():\n    return before + 1\nawait g()"),
        "2"
    );
}

impl Cells {
    /// The last line of the uncaught exception's report.
    fn report(&mut self, src: &str) -> String {
        match self.start(src) {
            Progress::Error(e) => e.traceback.lines().last().unwrap_or("").to_owned(),
            other => panic!("{other:?}"),
        }
    }
}

#[test]
#[allow(clippy::too_many_lines, reason = "one scenario, step by step")]
fn unsupported_imports_keep_cpython_text_and_add_a_replaceable_hint() {
    let mut c = Cells::new();
    let numpy = "ModuleNotFoundError: No module named 'numpy'. micro-monty runs a Python subset \
                 without third-party packages; use lists, dicts and the math, statistics, csv and \
                 json modules, or a tool";
    assert_eq!(c.report("import numpy.linalg"), numpy);
    // The message itself is CPython's; the hint is only in the report.
    assert_eq!(
        c.value("try:\n    import pandas\nexcept ImportError as e:\n    m = str(e)\nm"),
        "\"No module named 'pandas'\""
    );
    for (src, end) in [
        (
            "import requests",
            "There is no network in this sandbox; call a tool to fetch data",
        ),
        (
            "import urllib.request",
            "There is no network in this sandbox; call a tool to fetch data",
        ),
        (
            "from subprocess import run",
            "run tool calls concurrently with asyncio.gather",
        ),
        (
            "import sqlite3",
            "There is no filesystem in this sandbox; read and write files with a tool",
        ),
        (
            "import dateutil",
            "Use zoneinfo.ZoneInfo, or datetime with timezone.utc or fixed timedelta offsets",
        ),
        ("import pydantic", "Use dataclasses"),
        ("import yaml", "Use json, or tomllib for TOML"),
        (
            "import bs4",
            "Use re and str methods on the text, or a tool",
        ),
        (
            "import asyncio\nasyncio.eager_task_factory",
            "micro-monty's asyncio has ALL_COMPLETED, Barrier, BoundedSemaphore, BrokenBarrierError, Condition, Event, EventLoop, FIRST_COMPLETED, \
             FIRST_EXCEPTION, Future, Handle, LifoQueue, Lock, PriorityQueue, Queue, QueueEmpty, \
             QueueFull, QueueShutDown, Runner, Semaphore, TaskGroup, Timeout, TimerHandle, all_tasks, \
             as_completed, create_task, current_task, ensure_future, gather, get_event_loop, \
             get_running_loop, isfuture, new_event_loop, run, set_event_loop, shield, sleep, timeout, timeout_at, \
             wait and wait_for; pass coroutines to asyncio.gather",
        ),
        // asyncio's streams and threads point at the host.
        (
            "import asyncio\nasyncio.open_connection",
            "There is no network in this sandbox; call a tool to fetch data",
        ),
        (
            "import asyncio\nasyncio.to_thread",
            "run tool calls concurrently with asyncio.gather",
        ),
        (
            "import json\njson.lods",
            "AttributeError: module 'json' has no attribute 'lods'. Did you mean: 'loads'?",
        ),
        (
            "from json import lods",
            "(unknown location). Did you mean: 'loads'?",
        ),
        (
            "open('a.txt')",
            "this sandbox has no ambient filesystem. Use a tool for files, environment variables and commands",
        ),
        (
            "import tkinter",
            "ModuleNotFoundError: No module named 'tkinter'",
        ),
    ] {
        let report = c.report(src);
        assert!(report.ends_with(end), "{src}: {report}");
    }
    // A module this build leaves out says so.
    let env = c.engine.environment();
    if let Some(off) = env
        .lines()
        .find_map(|l| l.strip_prefix("Not enabled in this build: "))
        .and_then(|l| l.trim_end_matches('.').split(", ").next())
    {
        assert_eq!(
            c.report(&format!("import {off}")),
            format!(
                "ModuleNotFoundError: No module named '{off}'. '{off}' is not enabled in this \
                 build of micro-monty"
            )
        );
    }
    // The host replaces or blanks a group; the choice survives reset and
    // snapshots, and `None` restores the default.
    c.session
        .set_hint(
            HintGroup::Network,
            Some("Call the http tool for '{module}'"),
        )
        .expect("set");
    c.session
        .set_hint(HintGroup::Timezone, Some(""))
        .expect("set");
    c.session
        .set_hint(HintGroup::OsHooks, Some("Use fs.read"))
        .expect("set");
    c.session.reset().expect("reset");
    let image = c.session.snapshot().expect("snapshot");
    c.session = c.engine.restore(&image).expect("restore");
    assert_eq!(
        c.report("import httpx"),
        "ModuleNotFoundError: No module named 'httpx'. Call the http tool for 'httpx'"
    );
    assert_eq!(
        c.report("import pytz"),
        "ModuleNotFoundError: No module named 'pytz'"
    );
    assert!(
        c.report("open('x')")
            .ends_with("no ambient filesystem. Use fs.read")
    );
    c.session.set_hint(HintGroup::Network, None).expect("set");
    assert!(
        c.report("import httpx")
            .ends_with("call a tool to fetch data")
    );
}

#[test]
fn the_environment_summary_comes_from_the_build() {
    let engine = Engine::new(
        EngineConfig::new().with_limits(
            Limits::new()
                .with_memory(64 << 20)
                .with_fuel(Some(5_000_000)),
        ),
    );
    let env = engine.environment();
    let modules = env
        .lines()
        .find_map(|l| l.strip_prefix("Modules: "))
        .expect("a module line");
    let listed: Vec<&str> = modules.trim_end_matches('.').split(", ").collect();
    for m in [
        "json",
        "re",
        "math",
        "asyncio",
        "datetime",
        "os.path",
        "collections.abc",
    ] {
        assert!(listed.contains(&m), "{m} missing from {env}");
    }
    assert!(!listed.iter().any(|m| m.starts_with('_')), "{env}");
    // Every module listed imports, and nothing listed as missing does.
    let mut s = Session::new(&engine).expect("session");
    for m in &listed {
        let code = engine
            .compile(&format!("import {m}"), "cell.py")
            .expect("compiles");
        assert!(
            matches!(s.start(&code), Progress::Complete(_)),
            "import {m}"
        );
    }
    assert!(
        env.contains(
            "asyncio: ALL_COMPLETED, Barrier, BoundedSemaphore, BrokenBarrierError, Condition, Event, EventLoop, FIRST_COMPLETED, \
             FIRST_EXCEPTION, Future, Handle, LifoQueue, Lock, PriorityQueue, Queue, QueueEmpty, \
             QueueFull, QueueShutDown, Runner, Semaphore, TaskGroup, Timeout, TimerHandle, all_tasks, \
             as_completed, create_task, current_task, ensure_future, gather, get_event_loop, \
             get_running_loop, isfuture, new_event_loop, run, set_event_loop, shield, sleep, timeout, timeout_at, \
             wait and wait_for."
        ),
        "{env}"
    );
    assert!(
        env.contains("Limits: memory 64 MiB, recursion depth 1000, 5000000 fuel units per cell."),
        "{env}"
    );
    // A session reports the same limits, read back from its VM.
    assert_eq!(s.environment(), env);
    assert!(env.contains("TimeoutError"), "{env}");
}

#[test]
fn head_tail_cuts_keep_both_ends_and_containers_keep_their_shape() {
    let mut c = Cells::new();
    c.session.set_text_limit(120);
    c.session.set_text_cut(TextCut::HeadTail);
    let Progress::Complete(Some(done)) = c.start("'a' * 300 + 'z' * 300") else {
        panic!()
    };
    assert!(done.repr_truncated && done.str_truncated);
    assert!(done.repr.len() <= 120, "{}", done.repr);
    assert!(
        done.repr.starts_with("'aaa") && done.repr.ends_with("zzz'"),
        "{}",
        done.repr
    );
    assert!(done.repr.contains("bytes omitted"), "{}", done.repr);
    let Progress::Complete(Some(done)) =
        c.start("[{'id': i, 'name': 'row%d' % i} for i in range(500)]")
    else {
        panic!()
    };
    assert!(done.repr_truncated);
    assert!(
        done.repr
            .starts_with("[{'id': 0, 'name': 'row0'}, {'id': 1, 'name': 'row1'}"),
        "{}",
        done.repr
    );
    assert!(done.repr.ends_with(" items)]"), "{}", done.repr);
    // The value stays whole for the host.
    let Some(HostValue::List(rows)) = &done.value else {
        panic!("{:?}", done.value)
    };
    assert_eq!(rows.len(), 500);
    // Short text is untouched, and the default cut keeps the head only.
    assert_eq!(c.value("[1, 2]"), "[1, 2]");
    c.session.set_text_cut(TextCut::Head);
    let Progress::Complete(Some(done)) = c.start("'a' * 300 + 'z' * 300") else {
        panic!()
    };
    assert_eq!(done.repr.len(), 120);
    assert!(!done.repr.contains('z'));
    // A class's own `__repr__` is not in the host value: the cut keeps the
    // real text rather than a shape built from the fields.
    c.session.set_text_cut(TextCut::HeadTail);
    let Progress::Complete(Some(done)) = c.start(
        "from collections import namedtuple\n\
         class P(namedtuple('P', 'x y')):\n    def __repr__(self): return 'custom'\n\
         [P(i, i) for i in range(100)]",
    ) else {
        panic!()
    };
    assert!(done.repr.starts_with("[custom, custom"), "{}", done.repr);
    assert!(done.repr.ends_with("custom]"), "{}", done.repr);
}

#[test]
fn an_uncaught_exception_report_stops_at_the_text_limit_and_the_deadline() {
    // Fuzz finding C2 (run_source-timeout-excrender-34f25cd9): the key's
    // `repr` is 2^26 frozensets deep in sharing, and the report used to
    // render it whole, for seconds, outside fuel and the deadline.
    let src = "fs = 'f'\nfor _ in range(26):\n    fs = frozenset([fs, (fs, 1)])\n{0: 1}[fs]\n";
    let bound = Duration::from_millis(if cfg!(debug_assertions) { 10_000 } else { 500 });
    let mut c = Cells::new();
    let t = Instant::now();
    let Progress::Error(e) = c.start(src) else {
        panic!()
    };
    assert!(t.elapsed() < bound, "took {:?}", t.elapsed());
    assert!(e.truncated);
    assert_eq!(e.type_name, "KeyError");
    assert_eq!(e.message.len(), 1 << 20);
    assert!(e.message.starts_with("frozenset({"), "{}", &e.message[..40]);
    assert_eq!(e.traceback.len(), 1 << 20);
    assert!(e.traceback.starts_with(
        "Traceback (most recent call last):\n  File \"cell.py\", line 4, in <module>\n"
    ));
    assert!(e.traceback.contains("\nKeyError: frozenset({"));
    // `TextCut::HeadTail` marks the stopped text's end as unknown.
    c.session.set_text_limit(4096);
    c.session.set_text_cut(TextCut::HeadTail);
    let Progress::Error(e) = c.start(src) else {
        panic!()
    };
    assert!(e.truncated && e.message.len() <= 4096);
    assert!(e.message.ends_with(" ... [more omitted]"), "{}", e.message);
    assert!(e.traceback.ends_with(" ... [more omitted]"));
    // With no text limit the deadline stops it: the memory limit would
    // allow 256 MiB of text.
    c.session.set_text_limit(u64::MAX);
    c.session
        .set_deadline(Some(Instant::now() + Duration::from_millis(300)));
    let t = Instant::now();
    let Progress::Error(e) = c.start(src) else {
        panic!()
    };
    assert!(
        t.elapsed() < bound + Duration::from_millis(300),
        "took {:?}",
        t.elapsed()
    );
    assert!(e.truncated);
    assert!(e.message.starts_with("frozenset({"));
    // An ordinary report is whole, as before, with the deadline past when
    // it is made. (The cell may also reach a regular check, where the
    // past deadline parks it; each resume sets it past again.)
    c.session.set_text_limit(1 << 20);
    c.session.set_deadline(Some(Instant::now()));
    let mut p = c.start("def f():\n    raise ValueError('boom', 2)\nf()\n");
    while p
        == (Progress::Parked {
            reason: ParkReason::Deadline,
        })
    {
        c.session.set_deadline(Some(Instant::now()));
        p = c.session.resume(Resume::Continue).expect("resumes");
    }
    let Progress::Error(e) = p else {
        panic!("{p:?}")
    };
    assert!(!e.truncated);
    assert_eq!(e.message, "('boom', 2)");
    assert_eq!(
        e.traceback,
        "Traceback (most recent call last):\n  File \"cell.py\", line 3, in <module>\n    f()\n    \
         ~^^\n  File \"cell.py\", line 2, in f\n    raise ValueError('boom', 2)\n\
         ValueError: ('boom', 2)\n"
    );
}

#[test]
fn a_missed_module_attribute_charges_fuel_for_its_hint() {
    // The "Did you mean" of a missing module attribute is computed when
    // the error is rendered, charged by the module's names; a `hasattr`
    // miss renders nothing and pays nothing for it.
    let mut c = Cells::new();
    let src =
        "import json, traceback\nfor i in range(20000):\n    setattr(json, 'attr_%d' % i, i)\n";
    assert!(matches!(c.start(src), Progress::Complete(_)));
    // Fuel is per cell: each cell gets the budget set before it.
    let mut used = |src: &str| {
        let before = u64::MAX;
        c.session.set_fuel(Some(before));
        assert!(matches!(c.start(src), Progress::Complete(_)));
        before - c.session.fuel().expect("fuel")
    };
    let missed = used("for i in range(200):\n    hasattr(json, 'lods')\n");
    assert!(missed < 200 * 100, "{missed}");
    let rendered = used(
        "for i in range(200):\n    try:\n        json.lods\n    except AttributeError as e:\n        traceback.format_exception_only(e)\n",
    );
    assert!(rendered > 200 * 20_000 / 8, "{rendered}");
}

#[test]
fn never_awaited_coroutines_are_collected_and_reported_once() {
    // Coroutines that die unstarted are garbage (CPython warns as each
    // dies); the end of the cell still names each once, in creation order,
    // whether or not a collection ran, also across a snapshot.
    let engine = Engine::new(EngineConfig::new().with_limits(Limits::new().with_memory(8 << 20)));
    let mut s = Session::new(&engine).expect("session");
    s.define(&HostModule::untyped("", &["tool"]))
        .expect("defined");
    let out = Arc::new(Mutex::new(String::new()));
    let sink = Arc::clone(&out);
    s.set_output_streams(Box::new(move |_, t: &str| {
        sink.lock().expect("lock").push_str(t);
    }));
    let src = "\
async def f():
    pass
async def g():
    pass
keep = g()
for i in range(200_000):
    f()
async def h():
    pass
h()
x = tool()
for i in range(200_000):
    f()
'done'
";
    let code = engine.compile(src, "cell.py").expect("code");
    let p = s.start(&code);
    assert!(matches!(p, Progress::HostCall { .. }), "{p:?}");
    let image = s.snapshot().expect("snapshot");
    let mut s = engine.restore(&image).expect("restore");
    let sink = Arc::clone(&out);
    s.set_output_streams(Box::new(move |_, t: &str| {
        sink.lock().expect("lock").push_str(t);
    }));
    let p = s.resume(Resume::Value(HostValue::None)).expect("resumes");
    let Progress::Complete(Some(v)) = p else {
        panic!("{p:?}")
    };
    assert_eq!(v.repr, "'done'");
    assert_eq!(
        out.lock().expect("lock").as_str(),
        "<sys>:0: RuntimeWarning: coroutine 'g' was never awaited\n\
         <sys>:0: RuntimeWarning: coroutine 'f' was never awaited\n\
         <sys>:0: RuntimeWarning: coroutine 'h' was never awaited\n"
    );
}

#[test]
fn as_completed_over_many_awaitables_fits_a_small_budget() {
    // Each step of `as_completed` waits on every awaitable left; settled
    // waits must not pile up on the others.
    let engine = Engine::new(EngineConfig::new().with_limits(Limits::new().with_memory(8 << 20)));
    let mut s = Session::new(&engine).expect("session");
    let src = "\
import asyncio
async def f(i):
    await asyncio.sleep(i * 0.001)
    return i
async def main():
    n = 0
    async for t in asyncio.as_completed([f(i) for i in range(1000)]):
        n += await t
    return n
asyncio.run(main())
";
    let code = engine.compile(src, "cell.py").expect("code");
    let p = s.start(&code);
    let Progress::Complete(Some(v)) = p else {
        panic!("{p:?}")
    };
    assert_eq!(v.repr, "499500");
}

#[test]
fn the_schedulers_timers_end_cancelled_with_their_loop() {
    // `call_later`'s timer is a task of the scheduler's own, which
    // `asyncio.run`'s shutdown does not wait for: it ends cancelled with
    // the loop, its coroutine closed, as CPython's closing loop drops the
    // handle (which itself stays uncancelled).
    let mut c = Cells::new();
    let src = "import asyncio\nh = None\nasync def main():\n    global h\n\
               \x20   h = asyncio.get_running_loop().call_later(100, print, 'never')\n\
               \x20   await asyncio.sleep(0)\nasyncio.run(main())\n\
               (h._task.done(), h._task.cancelled(), h.cancelled())";
    assert_eq!(c.value(src), "(True, True, False)");
    let again = "async def f():\n    return await asyncio.sleep(0, 'again')\nasyncio.run(f())";
    assert_eq!(c.value(again), "'again'");
    assert_eq!(c.take_out(), "");
}

#[test]
fn create_task_works_under_top_level_await() {
    // A cell with top-level `await` is its own event loop: `create_task`
    // there must run the task (no "no running event loop").
    let mut c = Cells::new();
    let src = "import asyncio\nasync def f(x):\n    await asyncio.sleep(0)\n    return x * 2\n\
               t = asyncio.create_task(f(21))\nawait t";
    assert_eq!(c.value(src), "42");
    assert_eq!(c.take_out(), "");
}

/// Runs an importing cell in a session of `engine` with a pinned clock and
/// entropy, saves it, and continues the image in a second cell: what a
/// session shows of itself, which must not depend on the module cache.
fn cached_imports(engine: &Engine) -> (String, String, Vec<u8>, String) {
    let pin = |session: &mut Session, out: &Arc<Mutex<String>>| {
        session.set_wall_clock(Some(Box::new(|| 1_773_532_800.0)));
        session.set_entropy(Some(Box::new(|b: &mut [u8]| b.fill(7))));
        let sink = Arc::clone(out);
        session.set_output(Box::new(move |s: &str| {
            sink.lock().expect("output lock").push_str(s);
        }));
    };
    let out = Arc::new(Mutex::new(String::new()));
    let first = match Session::new(engine) {
        Ok(mut session) => {
            pin(&mut session, &out);
            let code = engine
                .compile(
                    "import json, re, collections, datetime, statistics, random, asyncio\n\
                     print(json.dumps({'a': [1, 2]}), re.sub(r'\\d', '#', 'a1b2'),\n\
                           collections.Counter('abca').most_common(1),\n\
                           datetime.date(2026, 1, 2).isoformat(), statistics.mean([1, 2, 4]),\n\
                           random.random(), asyncio.run(asyncio.wait_for(asyncio.sleep(0, 'ok'), 1)))",
                    "imports.py",
                )
                .expect("compiles");
            let progress = format!("{:?}", session.start(&code));
            match session.snapshot() {
                Ok(image) => (progress, image),
                Err(e) => (progress, format!("{e:?}").into_bytes()),
            }
        }
        Err(e) => (format!("{e:?}"), Vec::new()),
    };
    let (progress, image) = first;
    let second = match engine.restore(&image) {
        Ok(mut session) => {
            pin(&mut session, &out);
            let code = engine
                .compile(
                    "import decimal\nprint(statistics.median([3, 1, 2]), decimal.Decimal('1.1') * 3)",
                    "again.py",
                )
                .expect("compiles");
            format!("{:?}", session.start(&code))
        }
        Err(e) => format!("{e:?}"),
    };
    let text = out.lock().expect("output lock").clone();
    (progress, text, image, second)
}

#[test]
fn sessions_on_many_threads_share_decoded_modules_and_behave_as_uncached() {
    let uncached = Engine::with_module_cache(EngineConfig::default(), 0);
    let expected = cached_imports(&uncached);
    assert!(
        expected.1.contains("a#b#") && expected.1.contains("ok"),
        "{expected:?}"
    );
    assert!(expected.3.starts_with("Complete"), "{expected:?}");
    assert_eq!(uncached.module_cache_bytes(), 0);

    let engine = Engine::new(EngineConfig::default());
    let threads: Vec<_> = (0..8)
        .map(|_| {
            let engine = engine.clone();
            std::thread::spawn(move || (0..12).map(|_| cached_imports(&engine)).collect::<Vec<_>>())
        })
        .collect();
    for t in threads {
        for run in t.join().expect("thread") {
            assert_eq!(run, expected);
        }
    }
    let held = engine.module_cache_bytes();
    assert!(
        held > 0 && held <= micro_monty::DEFAULT_MODULE_CACHE,
        "{held}"
    );

    // An image from either engine restores on the other, the same way.
    let image = expected.2.clone();
    assert_eq!(
        engine.restore(&image).expect("restore").snapshot().ok(),
        uncached.restore(&image).expect("restore").snapshot().ok()
    );
}

#[test]
fn prelude_chunks_from_the_cache_are_charged_as_unpacked() {
    // asyncio is built in Rust and its classes come from prelude chunks
    // alone: under every memory limit, a session using it fails or
    // completes the same whether its chunks are unpacked or cached.
    let run = |engine: &Engine| {
        let mut session = Session::new(engine).map_err(|e| e.traceback)?;
        let code = engine
            .compile(
                "import asyncio\nx = asyncio.run(asyncio.wait_for(asyncio.sleep(0), 1))",
                "a.py",
            )
            .expect("compiles");
        let progress = format!("{:?}", session.start(&code));
        Ok::<_, String>((progress, session.snapshot().ok()))
    };
    let mut outcomes = Vec::new();
    for kb in (100..1200).step_by(4) {
        let limits = Limits::new().with_memory(kb << 10);
        let cold = run(&Engine::with_module_cache(
            EngineConfig::new().with_limits(limits),
            0,
        ));
        let warm = Engine::new(EngineConfig::new().with_limits(limits));
        drop(run(&Engine::new(EngineConfig::default())));
        let _ = run(&warm);
        assert!(run(&warm) == cold, "{kb} KB");
        // The mm-wasm guest's engine keeps no chunks, and behaves the same.
        let unshared = Engine::without_prelude_chunk_cache(EngineConfig::new().with_limits(limits));
        assert!(run(&unshared) == cold, "{kb} KB");
        assert_eq!(unshared.module_cache_bytes(), 0);
        outcomes.push(cold.is_ok_and(|r| r.0.starts_with("Complete")));
    }
    assert!(
        outcomes.contains(&true) && outcomes.contains(&false),
        "{outcomes:?}"
    );
}

#[test]
fn prelude_code_in_images_restores_the_same_from_every_cache_state() {
    // An image names prelude units' code by reference: a restore decodes
    // it from the unit's chunk, or takes it from the engine's cache, and
    // is charged the same way, so it fits or is TooLarge under the same
    // limits on a cold, warm or chunk-less engine.
    let mut session = Session::new(&Engine::new(EngineConfig::default())).expect("session");
    let code = Engine::new(EngineConfig::default())
        .compile(
            "import json, asyncio\nx = json.loads(json.dumps({'a': [1, 2]}))\n\
             y = asyncio.run(asyncio.wait_for(asyncio.sleep(0), 1))",
            "p.py",
        )
        .expect("compiles");
    assert_eq!(session.start(&code), Progress::Complete(None));
    let image = session.snapshot().expect("snapshot");
    let restore = |engine: &Engine| {
        engine
            .restore(&image)
            .map(|mut s| s.snapshot().expect("snapshot"))
            .map_err(|e| format!("{e:?}"))
    };
    let mut outcomes = Vec::new();
    for kb in (200..1400).step_by(8) {
        let limits = Limits::new().with_memory(kb << 10);
        let config = || EngineConfig::new().with_limits(limits);
        let cold = restore(&Engine::with_module_cache(config(), 0));
        // Its cache filled by a restore under the default limit.
        let warm = Engine::new(config());
        drop(warm.with_limits(Limits::new()).restore(&image));
        assert!(warm.module_cache_bytes() > 0);
        assert!(restore(&warm) == cold, "{kb} KB");
        let unshared = Engine::without_prelude_chunk_cache(config());
        assert!(restore(&unshared) == cold, "{kb} KB");
        outcomes.push(cold.is_ok());
    }
    assert!(
        outcomes.contains(&true) && outcomes.contains(&false),
        "{outcomes:?}"
    );
}

#[test]
fn the_module_cache_stops_at_its_limit_and_sessions_decode_past_it() {
    let probe = Engine::new(EngineConfig::default());
    let mut session = Session::new(&probe).expect("session");
    let first = probe
        .compile("import collections", "first.py")
        .expect("compiles");
    assert_eq!(session.start(&first), Progress::Complete(None));
    let first_bytes = probe.module_cache_bytes();
    assert!(first_bytes > 0);

    // Room for collections alone: the first session caches it and nothing
    // more.
    let small = Engine::with_module_cache(EngineConfig::default(), first_bytes);
    let expected = cached_imports(&Engine::with_module_cache(EngineConfig::default(), 0));
    let mut session = Session::new(&small).expect("session");
    assert_eq!(session.start(&first), Progress::Complete(None));
    assert_eq!(small.module_cache_bytes(), first_bytes);
    for _ in 0..3 {
        assert_eq!(cached_imports(&small), expected);
        assert_eq!(small.module_cache_bytes(), first_bytes);
    }

    // A session's memory limit is charged the same with a warm cache, so
    // the same tight limits fail the same way.
    let mut outcomes = Vec::new();
    for mb in [1, 2, 3, 4, 6, 8] {
        let limits = Limits::new().with_memory(mb << 20);
        let cold = Engine::with_module_cache(EngineConfig::new().with_limits(limits), 0);
        let warm = Engine::new(EngineConfig::new().with_limits(limits));
        drop(cached_imports(&Engine::new(EngineConfig::default())));
        let _ = cached_imports(&warm);
        let run = cached_imports(&warm);
        assert_eq!(run, cached_imports(&cold), "{mb} MB");
        outcomes.push(run.0.starts_with("Complete"));
    }
    assert!(
        outcomes.contains(&true) && outcomes.contains(&false),
        "{outcomes:?}"
    );
}
