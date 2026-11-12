//! Host modules (typed and untyped functions, classes, constants), their
//! ids and stubs, and session templates.

#![allow(clippy::expect_used, clippy::panic, reason = "tests")]

use micro_monty::{
    DefineError, Engine, EngineConfig, HostArgs, HostClass, HostFunctionRef, HostModule, HostParam,
    HostSignature, HostValue, ParamKind, Progress, Resume, Session, SessionTemplate, SnapshotError,
};
use std::{
    fmt::Write as _,
    sync::{Arc, Mutex},
};

fn param(name: &str, annotation: &str, default: Option<HostValue>, kind: ParamKind) -> HostParam {
    HostParam {
        name: name.to_owned(),
        annotation: annotation.to_owned(),
        default,
        kind,
    }
}

/// `read(path: str, n: float = 1.5, *, limit: int | None = None) -> str`.
fn read_sig(name: &str) -> HostSignature {
    HostSignature {
        name: name.to_owned(),
        params: vec![
            param("path", "str", None, ParamKind::PositionalOrKeyword),
            param(
                "n",
                "float",
                Some(HostValue::Float(1.5)),
                ParamKind::PositionalOrKeyword,
            ),
            param(
                "limit",
                "int | None",
                Some(HostValue::None),
                ParamKind::KeywordOnly,
            ),
        ],
        returns: "str".to_owned(),
        doc: "Read a file.".to_owned(),
        dict_as_kwargs: false,
    }
}

/// Module `name` with these functions and classes.
fn module(name: &str, functions: Vec<HostSignature>, classes: Vec<HostClass>) -> HostModule {
    HostModule {
        name: name.to_owned(),
        functions,
        classes,
        constants: Vec::new(),
        exceptions: Vec::new(),
    }
}

fn session(engine: &Engine, out: &Arc<Mutex<String>>) -> Session {
    let mut s = Session::new(engine).expect("session");
    let sink = Arc::clone(out);
    s.set_output(Box::new(move |t: &str| {
        sink.lock().expect("output lock").push_str(t);
    }));
    s
}

/// Runs `src`, answering every host call with `answer(name, args)`;
/// returns the calls seen and the final progress.
fn run(
    engine: &Engine,
    s: &mut Session,
    src: &str,
    answer: &dyn Fn(&str, &[HostValue]) -> HostValue,
) -> (Vec<(String, Vec<HostValue>, usize)>, Progress) {
    let code = engine.compile(src, "cell.py").expect("compiles");
    let mut calls = Vec::new();
    let mut p = s.start(&code);
    while let Progress::HostCall {
        name, args, kwargs, ..
    } = p
    {
        let v = answer(&name, &args);
        calls.push((name, args, kwargs.len()));
        p = s.resume(Resume::Value(v)).expect("resumes");
    }
    (calls, p)
}

fn str_(s: &str) -> HostValue {
    HostValue::Str(s.to_owned())
}

#[test]
fn typed_functions_bind_check_and_pass_parameters_in_order() {
    let engine = Engine::new(EngineConfig::default());
    let out = Arc::new(Mutex::new(String::new()));
    let mut s = session(&engine, &out);
    s.define(&module("", vec![read_sig("read")], vec![]))
        .expect("defined");
    let src = "\
r = [read('a'), read(limit=3, path='b'), read('c', 2), read('d', True, limit=None)]
for call in [lambda: read(), lambda: read(1), lambda: read('x', 'y'),
             lambda: read('x', limit='3'), lambda: read('x', 1, 2),
             lambda: read('x', size=1), lambda: read('x', limit=True)]:
    try:
        call()
        print('ok')
    except TypeError as e:
        print('TypeError', e)
print(read.__name__, read.__doc__)
r
";
    let (calls, p) = run(&engine, &mut s, src, &|_, args| match &args[0] {
        HostValue::Str(p) => HostValue::Str(p.to_uppercase()),
        _ => HostValue::None,
    });
    let args: Vec<Vec<HostValue>> = calls.iter().map(|c| c.1.clone()).collect();
    assert_eq!(
        args,
        [
            vec![str_("a"), HostValue::Float(1.5), HostValue::None],
            vec![str_("b"), HostValue::Float(1.5), HostValue::Int(3)],
            vec![str_("c"), HostValue::Int(2), HostValue::None],
            vec![str_("d"), HostValue::Bool(true), HostValue::None],
            // `limit=True` passes: bool is an int.
            vec![str_("x"), HostValue::Float(1.5), HostValue::Bool(true)],
        ]
    );
    assert!(calls.iter().all(|c| c.0 == "read" && c.2 == 0), "{calls:?}");
    assert!(
        matches!(&p, Progress::Complete(Some(c)) if c.repr == "['A', 'B', 'C', 'D']"),
        "{p:?}"
    );
    // CPython's messages for a `def read(path, n=1.5, *, limit=None)`.
    assert_eq!(
        out.lock().expect("output lock").as_str(),
        "\
TypeError read() missing 1 required positional argument: 'path'
TypeError read() argument 'path' must be str, not int
TypeError read() argument 'n' must be float, not str
TypeError read() argument 'limit' must be int | None, not str
TypeError read() takes from 1 to 2 positional arguments but 3 were given
TypeError read() got an unexpected keyword argument 'size'
ok
read Read a file.
"
    );
}

#[test]
fn a_type_error_points_at_the_caller_and_definitions_survive_reset_and_restore() {
    let engine = Engine::new(EngineConfig::default());
    let out = Arc::new(Mutex::new(String::new()));
    let mut s = session(&engine, &out);
    s.define(&module(
        "fs",
        vec![read_sig("read"), HostSignature::any("stat")],
        vec![],
    ))
    .expect("defined");
    let bad = "import fs\n\ndef f():\n    return fs.read(7)\n\nf()\n";
    let check = |s: &mut Session| {
        let (calls, p) = run(&engine, s, bad, &|_, _| HostValue::None);
        assert_eq!(calls, []);
        let Progress::Error(e) = p else {
            panic!("{p:?}")
        };
        assert_eq!(
            e.traceback,
            "Traceback (most recent call last):\n  File \"cell.py\", line 6, in <module>\n    f()\n    \
             ~^^\n  File \"cell.py\", line 4, in f\n    return fs.read(7)\n           \
             ~~~~~~~^^^\nTypeError: read() argument 'path' must be str, not int\n"
        );
        let (calls, _) = run(
            &engine,
            s,
            "import fs\nfs.read('p')\nfs.stat('q')",
            &|_, _| HostValue::None,
        );
        let names: Vec<&str> = calls.iter().map(|c| c.0.as_str()).collect();
        assert_eq!(names, ["fs.read", "fs.stat"]);
    };
    check(&mut s);
    s.reset().expect("reset");
    check(&mut s);
    let image = s.snapshot().expect("snapshot");
    let mut restored = engine.restore(&image).expect("restores");
    check(&mut restored);
}

fn entry_class() -> HostClass {
    HostClass {
        name: "Entry".to_owned(),
        fields: vec![
            ("path".to_owned(), "str".to_owned()),
            ("size".to_owned(), "int".to_owned()),
        ],
        methods: vec![HostSignature {
            name: "read".to_owned(),
            params: vec![param(
                "limit",
                "int",
                Some(HostValue::Int(10)),
                ParamKind::PositionalOrKeyword,
            )],
            returns: "Entry".to_owned(),
            doc: String::new(),
            dict_as_kwargs: false,
        }],
        doc: "A directory entry.".to_owned(),
    }
}

fn entry(path: &str, size: i64) -> HostValue {
    HostValue::Object {
        class: "files.Entry".to_owned(),
        fields: vec![
            ("path".to_owned(), str_(path)),
            ("size".to_owned(), HostValue::Int(size)),
        ],
    }
}

/// A stop's host call is kept in the session (`Session::pending`) and in
/// its snapshot with every kind of argument intact: a host-class object, a
/// named tuple, and each placeholder lenient arguments leave in place (one
/// call each, as all but `Opaque` stop the conversion).
#[test]
fn a_pending_call_keeps_every_kind_of_argument_across_a_restore() {
    let engine = Engine::new(EngineConfig::default());
    let out = Arc::new(Mutex::new(String::new()));
    let mut s = session(&engine, &out);
    s.define(&module(
        "files",
        vec![HostSignature::any("fetch")],
        vec![entry_class()],
    ))
    .expect("defined");
    s.set_host_args(HostArgs {
        max_bytes: 200,
        max_depth: 20,
        lenient: true,
    });
    let src = "\
import collections
from files import Entry, fetch
P = collections.namedtuple('P', 'x y')
deep = 1
for _ in range(30):
    deep = [deep]
fetch(Entry('a.txt', 3), P(1, 'b'), {1, 2}, {1: 'a'})
fetch(deep)
fetch('x' * 300)
";
    let mut kinds = String::new();
    let mut p = s.start(&engine.compile(src, "pending.py").expect("compiles"));
    while let Progress::HostCall { args, .. } = &p {
        write!(kinds, "{args:?}").expect("string");
        assert_eq!(s.pending().as_ref(), Some(&p));
        let restored = engine
            .restore(&s.snapshot().expect("snapshot"))
            .expect("restores");
        assert_eq!(restored.pending().as_ref(), Some(&p));
        p = s.resume(Resume::Value(HostValue::None)).expect("resumes");
    }
    assert!(matches!(p, Progress::Complete(_)), "{p:?}");
    for kind in [
        "Object",
        "Record",
        "Opaque",
        "KeyNotStr",
        "TooDeep",
        "TooLarge",
    ] {
        assert!(kinds.contains(kind), "{kind} in {kinds}");
    }
}

#[test]
fn host_classes_cross_the_boundary_as_objects() {
    let engine = Engine::new(EngineConfig::default());
    let out = Arc::new(Mutex::new(String::new()));
    let mut s = session(&engine, &out);
    let size_of = HostSignature {
        name: "size_of".to_owned(),
        params: vec![param("e", "Entry", None, ParamKind::PositionalOrKeyword)],
        returns: "int".to_owned(),
        doc: String::new(),
        dict_as_kwargs: false,
    };
    s.define(&module(
        "files",
        vec![HostSignature::any("listdir"), size_of],
        vec![entry_class()],
    ))
    .expect("defined");
    let src = "\
from files import Entry, listdir, size_of
e = Entry('a.txt', 3)
print(e, e.path, e.size, e == Entry('a.txt', 3), hash(e) == hash(Entry('a.txt', 3)))
print(type(e).__name__, type(e).__module__, Entry.__doc__, isinstance(e, tuple))
try:
    e.size = 4
except AttributeError as x:
    print('AttributeError', x)
match e:
    case Entry(path, size):
        print('matched', path, size)
first = listdir('/')[0]
print(type(first) is Entry, first.size, first)
nxt = first.read()
print(nxt.path, nxt.read(limit=2).size)
try:
    size_of('not an entry')
except TypeError as x:
    print('TypeError', x)
size_of(e)
e
";
    let (calls, p) = run(&engine, &mut s, src, &|name, args| match name {
        "files.listdir" => HostValue::List(vec![entry("b.txt", 5)]),
        "files.Entry.read" => match &args[1] {
            HostValue::Int(n) => entry("next.txt", *n),
            _ => HostValue::None,
        },
        "files.size_of" => HostValue::Int(3),
        _ => HostValue::None,
    });
    assert_eq!(
        calls
            .iter()
            .map(|c| (c.0.as_str(), c.1.clone()))
            .collect::<Vec<_>>(),
        [
            ("files.listdir", vec![str_("/")]),
            (
                "files.Entry.read",
                vec![entry("b.txt", 5), HostValue::Int(10)]
            ),
            (
                "files.Entry.read",
                vec![entry("next.txt", 10), HostValue::Int(2)]
            ),
            ("files.size_of", vec![entry("a.txt", 3)]),
        ]
    );
    let Progress::Complete(Some(c)) = &p else {
        panic!("{p:?}")
    };
    assert_eq!(c.value, Some(entry("a.txt", 3)));
    assert_eq!(
        out.lock().expect("output lock").as_str(),
        "\
Entry(path='a.txt', size=3) a.txt 3 True True
Entry files A directory entry. True
AttributeError cannot assign to field 'size'
matched a.txt 3
True 5 Entry(path='b.txt', size=5)
next.txt 2
TypeError size_of() argument 'e' must be Entry, not str
"
    );
}

#[test]
fn objects_of_unknown_classes_or_with_wrong_fields_raise() {
    let engine = Engine::new(EngineConfig::default());
    let out = Arc::new(Mutex::new(String::new()));
    let mut s = session(&engine, &out);
    s.define(&module("files", vec![], vec![entry_class()]))
        .expect("module");
    s.define(&module("", vec![HostSignature::any("get")], vec![]))
        .expect("defined");
    let answers = [
        HostValue::Object {
            class: "files.Nope".to_owned(),
            fields: vec![],
        },
        HostValue::Object {
            class: "files.Entry".to_owned(),
            fields: vec![("path".to_owned(), str_("a"))],
        },
        // Fields must come in the class's order.
        HostValue::Object {
            class: "files.Entry".to_owned(),
            fields: vec![
                ("size".to_owned(), HostValue::Int(1)),
                ("path".to_owned(), str_("p")),
            ],
        },
        entry("p", 1),
    ];
    for (answer, want) in answers.iter().zip([
        "TypeError: unknown host class\n",
        "TypeError: host object fields differ from its class's, in order\n",
        "TypeError: host object fields differ from its class's, in order\n",
        "",
    ]) {
        let code = engine.compile("print(get())", "cell.py").expect("compiles");
        let p = s.start(&code);
        assert!(matches!(p, Progress::HostCall { .. }), "{p:?}");
        match s.resume(Resume::Value(answer.clone())).expect("resumes") {
            Progress::Error(e) => assert!(e.traceback.ends_with(want), "{}", e.traceback),
            Progress::Complete(_) => assert_eq!(want, ""),
            other => panic!("{other:?}"),
        }
    }
    assert_eq!(
        out.lock().expect("output lock").as_str(),
        "Entry(path='p', size=1)\n"
    );
}

#[test]
fn definitions_are_checked_before_anything_runs() {
    let engine = Engine::new(EngineConfig::default());
    let mut s = Session::new(&engine).expect("session");
    let mut bad = read_sig("read");
    bad.params[0].name = "import".to_owned();
    for (sig, msg) in [
        (read_sig("m.read"), "invalid function name \"m.read\""),
        (read_sig("bad name"), "invalid function name \"bad name\""),
        (read_sig("_host_x"), "invalid function name \"_host_x\""),
        (
            HostSignature {
                params: vec![
                    param(
                        "a",
                        "",
                        Some(HostValue::None),
                        ParamKind::PositionalOrKeyword,
                    ),
                    param("b", "", None, ParamKind::PositionalOrKeyword),
                ],
                ..read_sig("f")
            },
            "a parameter without a default follows one with a default",
        ),
        (
            HostSignature {
                params: vec![param("a", "", None, ParamKind::VarPositional)],
                ..read_sig("f")
            },
            "f: *args and **kwargs are only supported as the untyped form (*args, **kwargs)",
        ),
    ] {
        let e = s
            .define(&module("", vec![sig], vec![]))
            .expect_err("refused");
        assert_eq!(e, DefineError::Invalid(msg.to_owned()));
    }
    let twice = HostModule {
        constants: vec![("read".to_owned(), HostValue::None)],
        ..module("m", vec![read_sig("read")], vec![])
    };
    assert_eq!(
        s.define(&twice).expect_err("refused").to_string(),
        "\"read\" is defined twice"
    );
    assert_eq!(
        s.define(&module("a-b", vec![], vec![]))
            .expect_err("refused")
            .to_string(),
        "invalid module name \"a-b\""
    );
    // A keyword fails to compile: a SyntaxError, and nothing is defined.
    let e = s
        .define(&module("", vec![bad], vec![]))
        .expect_err("refused");
    assert!(
        matches!(&e, DefineError::Invalid(m) if m.contains("SyntaxError")),
        "{e:?}"
    );
    assert_eq!(s.host_modules(), []);
    // Not while a cell is stopped.
    s.define(&module("", vec![HostSignature::any("wait")], vec![]))
        .expect("defined");
    let code = engine.compile("wait()", "cell.py").expect("compiles");
    assert!(matches!(s.start(&code), Progress::HostCall { .. }));
    let e = s
        .define(&module("", vec![read_sig("read")], vec![]))
        .expect_err("refused");
    assert_eq!(e, DefineError::Busy);
    assert!(e.to_string().contains("while a cell is stopped"), "{e}");
}

#[test]
fn stubs_render_every_host_definition() {
    let engine = Engine::new(EngineConfig::default());
    let mut s = Session::new(&engine).expect("session");
    let builtins = module(
        "",
        vec![
            HostSignature::any("plain"),
            read_sig("read"),
            HostSignature::any("os.getenv"),
        ],
        vec![],
    );
    s.define(&builtins).expect("defined");
    let files = HostModule {
        constants: vec![("ROOT".to_owned(), str_("/"))],
        ..module(
            "files",
            vec![HostSignature::any("listdir")],
            vec![entry_class()],
        )
    };
    s.define(&files).expect("module");
    // The definitions come back as given, for stubs or serialization.
    assert_eq!(s.host_modules(), [builtins, files]);
    let stubs = s.stubs();
    assert_eq!(
        stubs,
        [
            (
                String::new(),
                "from typing import Any\n\n\
                 def plain(*args: Any, **kwargs: Any) -> Any: ...\n\
                 def read(path: str, n: float = ..., *, limit: int | None = ...) -> str:\n    \
                 \"\"\"Read a file.\"\"\"\n"
                    .to_owned()
            ),
            (
                "files".to_owned(),
                "from typing import Any\n\nROOT: str\n\
                 class Entry:\n    \"\"\"A directory entry.\"\"\"\n    path: str\n    size: int\n    \
                 def __init__(self, path: str, size: int) -> None: ...\n    \
                 def read(self, limit: int = ...) -> Entry: ...\n\
                 def listdir(*args: Any, **kwargs: Any) -> Any: ...\n"
                    .to_owned()
            ),
        ]
    );
}

#[test]
fn templates_start_independent_sessions_in_the_saved_state() {
    let engine = Engine::new(EngineConfig::default());
    let mut s = Session::new(&engine).expect("session");
    s.define(&module("", vec![read_sig("read")], vec![]))
        .expect("typed");
    let boot = engine
        .compile("def helper(p):\n    return read(p)\nstate = []", "boot.py")
        .expect("compiles");
    assert!(matches!(s.start(&boot), Progress::Complete(_)));
    let template = SessionTemplate::capture(&engine, &mut s).expect("captured");
    let template = SessionTemplate::from_image(&engine, template.image().to_vec()).expect("loads");
    let mut first = template.instantiate().expect("first");
    let mut second = template.instantiate().expect("second");
    let (calls, p) = run(
        &engine,
        &mut first,
        "state.append(1)\nhelper('x')",
        &|_, _| str_("X"),
    );
    assert_eq!(calls.len(), 1);
    assert!(
        matches!(&p, Progress::Complete(Some(c)) if c.repr == "'X'"),
        "{p:?}"
    );
    let (_, p) = run(&engine, &mut second, "state", &|_, _| HostValue::None);
    assert!(
        matches!(&p, Progress::Complete(Some(c)) if c.repr == "[]"),
        "{p:?}"
    );
    // Not with a stopped cell.
    let code = engine.compile("read('p')", "cell.py").expect("compiles");
    assert!(matches!(s.start(&code), Progress::HostCall { .. }));
    assert_eq!(
        SessionTemplate::capture(&engine, &mut s).expect_err("busy"),
        SnapshotError::Busy
    );
}

#[test]
fn defining_a_module_replaces_it_whole() {
    let engine = Engine::new(EngineConfig::default());
    let out = Arc::new(Mutex::new(String::new()));
    let mut s = session(&engine, &out);
    let first = HostModule {
        constants: vec![("K".to_owned(), HostValue::Int(1))],
        ..module(
            "files",
            vec![HostSignature::any("old"), read_sig("read")],
            vec![entry_class()],
        )
    };
    s.define(&first).expect("first");
    s.define(&module("", vec![HostSignature::any("gone")], vec![]))
        .expect("builtins");
    let probe = "\
import files
print([n for n in ['Entry', 'K', 'old', 'read'] if hasattr(files, n)])
try:
    gone
except NameError as e:
    print('NameError', e)
";
    let (_, p) = run(&engine, &mut s, probe, &|_, _| HostValue::None);
    assert!(matches!(p, Progress::Complete(_)), "{p:?}");
    // The second definition has no `old`, `Entry` or `K`, and `read` is
    // untyped now; the builtins lose `gone`.
    s.define(&module("files", vec![HostSignature::any("read")], vec![]))
        .expect("second");
    s.define(&module("", vec![], vec![])).expect("builtins");
    let (calls, p) = run(
        &engine,
        &mut s,
        &format!("{probe}files.read(1, x=2)"),
        &|_, _| HostValue::None,
    );
    assert!(matches!(p, Progress::Complete(_)), "{p:?}");
    assert_eq!(
        calls,
        [("files.read".to_owned(), vec![HostValue::Int(1)], 1)]
    );
    assert_eq!(
        out.lock().expect("output lock").as_str(),
        "\
['Entry', 'K', 'old', 'read']
['read']
NameError name 'gone' is not defined
"
    );
}

#[test]
fn host_calls_carry_ids_that_survive_redefinition_snapshots_and_templates() {
    let engine = Engine::new(EngineConfig::default());
    let mut s = Session::new(&engine).expect("session");
    let files = module(
        "files",
        vec![HostSignature::any("listdir"), read_sig("read")],
        vec![entry_class()],
    );
    let pairs = |refs: Vec<HostFunctionRef>| -> Vec<(String, u32)> {
        refs.into_iter().map(|f| (f.name, f.id)).collect()
    };
    let ids = pairs(s.define(&files).expect("files"));
    let names: Vec<&str> = ids.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, ["files.listdir", "files.read", "files.Entry.read"]);
    let builtins = pairs(
        s.define(&module("", vec![HostSignature::any("get")], vec![]))
            .expect("builtins"),
    );
    let mut all = ids.clone();
    all.extend(builtins);
    let distinct: std::collections::BTreeSet<u32> = all.iter().map(|(_, id)| *id).collect();
    assert_eq!(distinct.len(), all.len(), "{all:?}");
    for (name, id) in &all {
        assert_eq!(s.host_function_id(name), Some(*id));
        assert_eq!(s.host_function_name(*id), Some(name.as_str()));
    }
    // Redefining keeps every id; a new function gets a new one.
    let mut more = files.clone();
    more.functions.insert(0, HostSignature::any("stat"));
    let again = pairs(s.define(&more).expect("again"));
    assert_eq!(again[1..], ids[..]);
    assert!(!distinct.contains(&again[0].1));
    let expected: Vec<(String, u32)> = all.clone();

    // Every call reports its function's id, in this session and after a
    // snapshot or a template, mid-cell too.
    let src = "\
import files
get()
files.listdir('/')
e = files.Entry('a', 1)
e.read()
files.read('p')
";
    let code = engine.compile(src, "cell.py").expect("compiles");
    let id_of = |n: &str| expected.iter().find(|(m, _)| m == n).map(|(_, id)| *id);
    let drive = |s: &mut Session, mut p: Progress, seen: &mut Vec<(String, u32)>| {
        while let Progress::HostCall { name, function, .. } = &p {
            seen.push((name.clone(), *function));
            let answer = if name == "files.Entry.read" {
                entry("b", 2)
            } else {
                HostValue::None
            };
            p = s.resume(Resume::Value(answer)).expect("resumes");
        }
        assert!(matches!(p, Progress::Complete(_)), "{p:?}");
    };
    let mut seen = Vec::new();
    let p = s.start(&code);
    // Park at the first call, continue in a restored copy.
    let image = s.snapshot().expect("snapshot");
    drive(&mut s, p, &mut seen);
    let mut restored = engine.restore(&image).expect("restores");
    let mut seen_restored = Vec::new();
    let first = restored
        .resume(Resume::Value(HostValue::None))
        .expect("resumes");
    seen_restored.push(("get".to_owned(), id_of("get").expect("id")));
    drive(&mut restored, first, &mut seen_restored);
    let template = SessionTemplate::capture(&engine, &mut restored).expect("template");
    let mut fresh = template.instantiate().expect("instance");
    let mut seen_template = Vec::new();
    let p = fresh.start(&code);
    drive(&mut fresh, p, &mut seen_template);
    assert_eq!(fresh.host_function_id("files.read"), id_of("files.read"));
    for run in [&seen, &seen_restored, &seen_template] {
        let want: Vec<(String, u32)> = ["get", "files.listdir", "files.Entry.read", "files.read"]
            .iter()
            .map(|n| ((*n).to_owned(), id_of(n).expect("id")))
            .collect();
        assert_eq!(*run, want);
    }
}

#[test]
fn dict_as_kwargs_binds_one_dict_as_keywords_and_checks_as_usual() {
    let engine = Engine::new(EngineConfig::default());
    let out = Arc::new(Mutex::new(String::new()));
    let mut s = session(&engine, &out);
    let flagged = HostSignature {
        dict_as_kwargs: true,
        ..read_sig("read")
    };
    let entry = HostClass {
        name: "Entry".into(),
        fields: vec![("path".into(), "str".into())],
        methods: vec![HostSignature {
            dict_as_kwargs: true,
            ..read_sig("read")
        }],
        doc: String::new(),
    };
    s.define(&module("fs", vec![flagged, read_sig("plain")], vec![entry]))
        .expect("defined");
    let src = "\
import fs
r = [fs.read({'path': 'a', 'limit': 3}), fs.read('b'), fs.Entry('e').read({'path': 'd', 'n': 2})]
for call in [lambda: fs.read({'path': 1}), lambda: fs.read({'nope': 1}),
             lambda: fs.plain({'path': 'x'}),
             # With other arguments, a dict is the first parameter itself.
             lambda: fs.read({'path': 'x'}, limit=1)]:
    try:
        call()
        print('ok')
    except TypeError as e:
        print('TypeError', e)
print(fs.read.__name__, fs.read.__qualname__, fs.read.__module__, fs.read.__doc__)
r
";
    let (calls, p) = run(&engine, &mut s, src, &|_, _| HostValue::None);
    let args: Vec<Vec<HostValue>> = calls.iter().map(|c| c.1.clone()).collect();
    let entry_obj = HostValue::Object {
        class: "fs.Entry".into(),
        fields: vec![("path".into(), str_("e"))],
    };
    assert_eq!(
        args,
        [
            vec![str_("a"), HostValue::Float(1.5), HostValue::Int(3)],
            vec![str_("b"), HostValue::Float(1.5), HostValue::None],
            vec![entry_obj, str_("d"), HostValue::Int(2), HostValue::None],
        ]
    );
    assert!(matches!(p, Progress::Complete(Some(_))), "{p:?}");
    assert_eq!(
        out.lock().expect("output").as_str(),
        "TypeError read() argument 'path' must be str, not int\n\
         TypeError read() got an unexpected keyword argument 'nope'\n\
         TypeError plain() argument 'path' must be str, not dict\n\
         TypeError read() argument 'path' must be str, not dict\n\
         read read fs Read a file.\n"
    );
}

#[test]
fn host_functions_have_doc_name_module_dir_and_help() {
    let engine = Engine::new(EngineConfig::default());
    let out = Arc::new(Mutex::new(String::new()));
    let mut s = session(&engine, &out);
    let lookup = HostSignature {
        doc: "Look a key up.".into(),
        ..HostSignature::any("lookup")
    };
    s.define(&HostModule {
        constants: vec![("ROOT".into(), str_("/"))],
        ..module("fs", vec![read_sig("read"), lookup], vec![])
    })
    .expect("defined");
    s.define(&HostModule::untyped("", &["ping"]))
        .expect("defined");
    let src = "\
import fs
for f in (fs.read, fs.lookup, ping):
    print(f.__name__, f.__module__, f.__doc__)
print([n for n in dir(fs) if not n.startswith('_')])
help(fs.read)
help(fs.lookup)
help(fs)
";
    let (calls, p) = run(&engine, &mut s, src, &|_, _| HostValue::None);
    assert_eq!(calls.len(), 0);
    assert!(matches!(p, Progress::Complete(_)), "{p:?}");
    assert_eq!(
        out.lock().expect("output").as_str(),
        "read fs Read a file.\n\
         lookup fs Look a key up.\n\
         ping builtins None\n\
         ['ROOT', 'lookup', 'read']\n\
         def read(path: str, n: float = ..., *, limit: int | None = ...) -> str:\n    \"\"\"Read a file.\"\"\"\n\
         def lookup(*args: Any, **kwargs: Any) -> Any:\n    \"\"\"Look a key up.\"\"\"\n\
         from typing import Any\n\nROOT: str\n\
         def read(path: str, n: float = ..., *, limit: int | None = ...) -> str:\n    \"\"\"Read a file.\"\"\"\n\
         def lookup(*args: Any, **kwargs: Any) -> Any:\n    \"\"\"Look a key up.\"\"\"\n"
    );
}

#[test]
fn a_module_named_like_a_bundled_one_is_refused() {
    let engine = Engine::new(EngineConfig::default());
    let mut s = Session::new(&engine).expect("session");
    for name in [
        "json",
        "json.decoder",
        "math",
        "_mm",
        "builtins",
        "collections",
    ] {
        let e = s
            .define(&HostModule::untyped(name, &["f"]))
            .expect_err(name);
        assert!(
            matches!(&e, DefineError::Invalid(m) if m.contains("micro-monty has a module of that name")),
            "{name}: {e:?}"
        );
    }
    assert_eq!(s.host_modules(), []);
    // A name under a bundled package that is not one of its modules, or
    // that only starts like one, is the host's.
    for name in ["json.tools", "jsonish", "tools.json"] {
        s.define(&HostModule::untyped(name, &["f"])).expect(name);
    }
}

#[test]
fn modules_with_many_typed_functions_methods_and_exceptions_define() {
    let engine = Engine::new(EngineConfig::default());
    let out = Arc::new(Mutex::new(String::new()));
    let mut s = session(&engine, &out);
    let functions: Vec<HostSignature> = (0..600).map(|i| read_sig(&format!("f{i}"))).collect();
    let class = HostClass {
        name: "Big".to_owned(),
        fields: vec![("x".to_owned(), "int".to_owned())],
        methods: (0..300).map(|i| read_sig(&format!("m{i}"))).collect(),
        ..HostClass::default()
    };
    let module = HostModule {
        exceptions: (0..300)
            .map(|i| micro_monty::HostException {
                name: format!("E{i}"),
                ..micro_monty::HostException::default()
            })
            .collect(),
        ..module("big", functions, vec![class])
    };
    let refs = s.define(&module).expect("defined");
    assert_eq!(refs.len(), 900);
    let code = engine
        .compile(
            "import big\n\
             try:\n    big.f599(1)\nexcept TypeError as e:\n    print(e)\n\
             try:\n    big.Big(1).m299(2)\nexcept TypeError as e:\n    print(e)\n\
             print(issubclass(big.E299, Exception), big.E299.__module__)",
            "big.py",
        )
        .expect("compiles");
    assert!(matches!(s.start(&code), Progress::Complete(_)));
    assert_eq!(
        out.lock().expect("output").as_str(),
        "f599() argument 'path' must be str, not int\n\
         Big.m299() argument 'path' must be str, not int\n\
         True big\n"
    );
}

#[test]
fn engines_from_with_limits_share_the_module_cache() {
    let engine = Engine::new(EngineConfig::default());
    let small = engine.with_limits(engine.limits().with_memory(8 << 20));
    assert_eq!(small.limits().memory, 8 << 20);
    let mut s = Session::new(&small).expect("session");
    let code = small.compile("import bisect", "b.py").expect("compiles");
    assert!(matches!(s.start(&code), Progress::Complete(_)));
    assert!(engine.module_cache_bytes() > 0);
    assert_eq!(engine.module_cache_bytes(), small.module_cache_bytes());
}

/// `files` (`read`, `Entry`) and `tools.check(e: files.Entry)`, whose
/// check refers to another module's class, defined on `engine`.
fn checked_tools(engine: &Engine, out: &Arc<Mutex<String>>) -> Session {
    let check = HostSignature {
        name: "check".to_owned(),
        params: vec![param(
            "e",
            "files.Entry",
            None,
            ParamKind::PositionalOrKeyword,
        )],
        ..HostSignature::default()
    };
    let mut s = session(engine, out);
    s.define(&module(
        "files",
        vec![read_sig("read")],
        vec![entry_class()],
    ))
    .expect("files");
    s.define(&module("tools", vec![check], Vec::new()))
        .expect("tools");
    s
}

/// Runs `src` in `s` (output to `out`, every call answered 7); returns
/// the output and the number of host calls.
fn checked_run(
    engine: &Engine,
    s: &mut Session,
    out: &Arc<Mutex<String>>,
    src: &str,
) -> (String, usize) {
    out.lock().expect("output lock").clear();
    let sink = Arc::clone(out);
    s.set_output(Box::new(move |t: &str| {
        sink.lock().expect("output lock").push_str(t);
    }));
    let (calls, p) = run(engine, s, src, &|_, _| HostValue::Int(7));
    assert!(matches!(p, Progress::Complete(_)), "{p:?}");
    let text = out.lock().expect("output lock").clone();
    (text, calls.len())
}

const CHECKED: &str = "\
import tools, files
try:
    tools.check('x')
except TypeError as x:
    print(x)
tools.check(files.Entry('a', 1))
";

#[test]
fn host_code_is_compiled_once_per_engine_and_images_refer_to_it_by_key() {
    let engine = Engine::new(EngineConfig::default());
    let out = Arc::new(Mutex::new(String::new()));
    let cached = engine.module_cache_bytes();
    let mut s = checked_tools(&engine, &out);
    let after_first = engine.module_cache_bytes();
    assert!(after_first > cached);
    drop(checked_tools(&engine, &out));
    assert_eq!(
        engine.module_cache_bytes(),
        after_first,
        "a hit compiles nothing"
    );
    let expected = (
        "check() argument 'e' must be files.Entry, not str\n".to_owned(),
        1,
    );
    assert_eq!(checked_run(&engine, &mut s, &out, CHECKED), expected);

    // Images carry no generated source or code; any Engine of the build
    // restores them, building the code again when its cache lacks it.
    let image = s.snapshot().expect("snapshot");
    assert!(!image.windows(15).any(|w| w == b"def _host_make("));
    let cold = Engine::new(EngineConfig::default());
    let before = cold.module_cache_bytes();
    for e in [
        &engine,
        &cold,
        &Engine::with_module_cache(EngineConfig::default(), 0),
    ] {
        let mut r = e.restore(&image).expect("restores");
        assert_eq!(checked_run(e, &mut r, &out, CHECKED), expected);
    }
    assert_eq!(cold.module_cache_bytes() - before, after_first - cached);

    // Parked at a typed function's host call, inside the generated code:
    // the call returns through it after a restore on another Engine.
    let call = engine
        .compile(
            "import tools, files\nprint(tools.check(files.Entry('a', 1)) + 1)\n",
            "p.py",
        )
        .expect("compiles");
    assert!(matches!(s.start(&call), Progress::HostCall { .. }));
    let parked = s.snapshot().expect("snapshot");
    for e in [
        &Engine::new(EngineConfig::default()),
        &Engine::with_module_cache(EngineConfig::default(), 0),
    ] {
        let mut r = e.restore(&parked).expect("restores");
        out.lock().expect("output lock").clear();
        let sink = Arc::clone(&out);
        r.set_output(Box::new(move |t: &str| {
            sink.lock().expect("output lock").push_str(t);
        }));
        let p = r.resume(Resume::Value(HostValue::Int(7))).expect("resumes");
        assert!(matches!(p, Progress::Complete(_)), "{p:?}");
        assert_eq!(*out.lock().expect("output lock"), "8\n");
    }
    let p = s.resume(Resume::Value(HostValue::Int(7))).expect("resumes");
    assert!(matches!(p, Progress::Complete(_)), "{p:?}");

    // An earlier definition's code a cell still holds is stored instead.
    let old = engine
        .compile("from tools import check as old", "c.py")
        .expect("compiles");
    assert!(matches!(s.start(&old), Progress::Complete(_)));
    s.define(&module(
        "tools",
        vec![HostSignature::any("check")],
        Vec::new(),
    ))
    .expect("redefined");
    let image = s.snapshot().expect("snapshot");
    let fresh = Engine::new(EngineConfig::default());
    let mut r = fresh.restore(&image).expect("restores");
    let src = "try:\n    old(1)\nexcept TypeError as x:\n    print(x)\n";
    assert_eq!(
        checked_run(&fresh, &mut r, &out, src).0,
        "check() argument 'e' must be files.Entry, not int\n"
    );
}

#[test]
fn images_store_host_code_on_request_and_never_bind_to_other_code() {
    let out = Arc::new(Mutex::new(String::new()));
    let engine = Engine::new(EngineConfig::default());
    let keyed = checked_tools(&engine, &out).snapshot().expect("snapshot");

    // Code the Engine does not share (on request, or with no cache) is
    // stored, and restores on a new Engine without compiling it again.
    for storing in [
        engine.with_host_code_in_snapshots(true),
        Engine::with_module_cache(EngineConfig::default(), 0),
    ] {
        let mut s = checked_tools(&storing, &out);
        let image = s.snapshot().expect("snapshot");
        assert!(image.len() > keyed.len());
        let cold = Engine::new(EngineConfig::default());
        let before = cold.module_cache_bytes();
        let mut r = cold.restore(&image).expect("restores");
        assert_eq!(cold.module_cache_bytes(), before);
        assert_eq!(
            checked_run(&cold, &mut r, &out, CHECKED),
            (
                "check() argument 'e' must be files.Entry, not str\n".to_owned(),
                1
            )
        );
    }

    // A definition that differs only where the generated code does not (a
    // docstring) shares the code; a restore checks that the image's
    // definition still builds it, and takes it.
    let cached = engine.module_cache_bytes();
    let mut s = session(&engine, &out);
    let mut read = read_sig("read");
    read.doc = "Reads a file.".to_owned();
    s.define(&module("files", vec![read], vec![entry_class()]))
        .expect("files");
    let image = s.snapshot().expect("snapshot");
    assert_eq!(engine.module_cache_bytes(), cached, "the same code");
    let mut r = engine.restore(&image).expect("restores");
    let doc = "import files\nprint(files.read.__doc__)\n";
    assert_eq!(
        checked_run(&engine, &mut r, &out, doc),
        ("Reads a file.\n".to_owned(), 0)
    );
    assert_eq!(engine.module_cache_bytes(), cached);

    // A spec that no longer generates the image's code, under the image's
    // key, is refused: on an Engine that builds the code again, and on one
    // whose cache holds the key (for another definition).
    let mut image = keyed;
    let at = image
        .windows(9)
        .position(|w| w == b"Read a fi")
        .expect("the spec is in the image");
    image[at..at + 4].copy_from_slice(b"READ");
    // `read`'s parameter `path: str`, as the spec encodes it.
    let param = b"\x04\0\0\0path\x05\x03\0\0\0str";
    let at = image
        .windows(param.len())
        .position(|w| w == param)
        .expect("a parameter in the spec")
        + 4;
    image[at..at + 4].copy_from_slice(b"pxth");
    let n = image.len() - 8;
    let sum = mm_core::xxh64(&image[..n], 0).to_le_bytes();
    image[n..].copy_from_slice(&sum);
    for e in [
        &engine,
        &Engine::new(EngineConfig::default()),
        &Engine::with_module_cache(EngineConfig::default(), 0),
    ] {
        match e.restore(&image) {
            Err(SnapshotError::Corrupt { what }) => assert_eq!(what, "host code"),
            other => panic!("{:?}", other.err()),
        }
    }
}
