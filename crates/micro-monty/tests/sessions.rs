//! Sessions: cells share globals until `reset`; imports of bundled and host
//! modules; cancellation keeps the session usable.

#![allow(clippy::expect_used, clippy::panic, reason = "tests")]

use micro_monty::{
    Engine, EngineConfig, HintGroup, HostModule, HostValue, Limits, ParkReason, Progress, Resume,
    Session, Stream, TextCut,
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
            "import email.utils",
            "micro-monty does not bundle 'email'; use re and str methods on the text, or a tool",
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
    // An ordinary report is whole, as before.
    c.session.set_text_limit(1 << 20);
    c.session.set_deadline(Some(Instant::now()));
    let Progress::Error(e) = c.start("def f():\n    raise ValueError('boom', 2)\nf()\n") else {
        panic!()
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
fn mm_aio_natives_refuse_objects_of_other_kinds() {
    // `_mm` is importable by the guest: its asyncio natives must not
    // settle or read the slots of objects that are not loop futures.
    let mut c = Cells::new();
    let src = "\
import _mm, asyncio
def gen():
    yield 1
lst, tup, g, sl = [1, 2, 3], (1, 2), gen(), asyncio.sleep(0, 'slept')
r = []
for x in (lst, tup, g, sl):
    try:
        _mm.aio_set(x, 5)
    except TypeError as e:
        r.append(str(e))
async def main():
    spent = asyncio.sleep(0)
    await spent
    for x in ([[1]], [(1,)], [spent]):
        try:
            await _mm.aio_first(x)
        except TypeError as e:
            r.append(str(e))
    return await sl
fut = _mm.aio_future()
_mm.aio_set(fut, 7)
[r, lst, tup, list(g), asyncio.run(main()), fut.result()]
";
    let set = "'an asyncio.Future is required'";
    let first = "'a task or future is required'";
    assert_eq!(
        c.value(src),
        format!(
            "[[{set}, {set}, {set}, {set}, {first}, {first}, {first}], [1, 2, 3], (1, 2), [1], 'slept', 7]"
        )
    );
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
