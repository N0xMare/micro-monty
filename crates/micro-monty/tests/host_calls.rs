//! Host calls: synchronous answers in place from any frame, host errors as
//! Python exceptions, futures awaited one by one and through
//! `asyncio.gather`, refused resumes, and bounded (strict or lenient)
//! argument conversion.

#![allow(clippy::expect_used, clippy::panic, reason = "tests")]

use micro_monty::{
    CallStatus, DefineError, Engine, EngineConfig, HostArgs, HostCallRecord, HostError,
    HostErrorKind, HostException, HostModule, HostValue, Limits, MAX_HOST_DEPTH, Phase, Progress,
    Resume, ResumeError, Session, SnapshotError, Unconverted,
};
use std::sync::{Arc, Mutex};

fn session(out: &Arc<Mutex<String>>) -> (Engine, Session) {
    let engine = Engine::new(EngineConfig::default());
    let mut session = Session::new(&engine).expect("session");
    let sink = Arc::clone(out);
    session.set_output(Box::new(move |s: &str| {
        sink.lock().expect("output lock").push_str(s);
    }));
    (engine, session)
}

fn start(engine: &Engine, session: &mut Session, src: &str) -> Progress {
    let code = engine.compile(src, "test.py").expect("compiles");
    session.start(&code)
}

fn int(v: &HostValue) -> i64 {
    match v {
        HostValue::Int(i) => *i,
        other => panic!("expected an int, got {other:?}"),
    }
}

#[test]
fn retained_host_modules_share_the_budget_and_failed_replacements_preserve_them() {
    let engine = Engine::new(EngineConfig::new().with_limits(Limits::new().with_memory(4 << 20)));
    let mut session = Session::new(&engine).expect("session");
    session
        .define(&HostModule {
            constants: [("K", HostValue::Int(7))]
                .iter()
                .map(|(k, v)| ((*k).to_owned(), v.clone()))
                .collect(),
            ..HostModule::untyped("config", &[])
        })
        .expect("defines");
    let error = session
        .define(&HostModule {
            constants: [("K", HostValue::Bytes(vec![0; 4 << 20]))]
                .iter()
                .map(|(k, v)| ((*k).to_owned(), v.clone()))
                .collect(),
            ..HostModule::untyped("config", &[])
        })
        .expect_err("retained host data must fit before it is cloned");
    assert!(
        matches!(&error, DefineError::Raised(e) if e.type_name == "MemoryError"),
        "{error:?}"
    );
    assert!(matches!(
        start(&engine, &mut session, "import config\nassert config.K == 7"),
        Progress::Complete(_)
    ));

    let mut registered = 0;
    for index in 0..128 {
        match session.define(&HostModule {
            constants: [("data", HostValue::Bytes(vec![17; 1 << 16]))]
                .iter()
                .map(|(k, v)| ((*k).to_owned(), v.clone()))
                .collect(),
            ..HostModule::untyped(&format!("module{index}"), &[])
        }) {
            Ok(_) => registered += 1,
            Err(error) => {
                assert!(
                    matches!(&error, DefineError::Raised(e) if e.type_name == "MemoryError"),
                    "{error:?}"
                );
                break;
            }
        }
    }
    assert!(registered > 0 && registered < 128);
    // A final deep definition must either fail admission or leave enough
    // workspace to decode it after all the preceding modules at the same cap.
    session
        .define(&HostModule::untyped("module0", &[]))
        .expect("release payload");
    let mut nested = HostValue::None;
    for _ in 0..1000 {
        nested = HostValue::List(vec![nested]);
    }
    if let Err(error) = session.define(&HostModule {
        constants: [("data", nested)]
            .iter()
            .map(|(k, v)| ((*k).to_owned(), v.clone()))
            .collect(),
        ..HostModule::untyped("deep", &[])
    }) {
        assert!(
            matches!(&error, DefineError::Raised(e) if e.type_name == "MemoryError"),
            "{error:?}"
        );
    }
    let image = session.snapshot().expect("snapshot at exhaustion");
    let smaller = Engine::new(EngineConfig::new().with_limits(Limits::new().with_memory(3 << 20)));
    assert_eq!(smaller.restore(&image).err(), Some(SnapshotError::TooLarge));
    session = engine.restore(&image).expect("same-budget restore");
    // Replacement releases the old retained payload, including after restore.
    session
        .define(&HostModule::untyped("module1", &[]))
        .expect("replace");
    session.reset().expect("reset");
    assert!(matches!(
        start(&engine, &mut session, "import config\nassert config.K == 7"),
        Progress::Complete(_)
    ));
}

#[test]
fn host_module_nesting_is_bounded_before_cloning_and_survives_restore() {
    let engine = Engine::new(EngineConfig::default());
    let mut session = Session::new(&engine).expect("session");
    let mut value = HostValue::List(Vec::new());
    for depth in 0..999 {
        value = match depth % 3 {
            0 => HostValue::List(vec![value]),
            1 => HostValue::Dict(vec![("key".into(), value)]),
            _ => HostValue::Record {
                name: "Row".into(),
                fields: vec![("field".into(), value)],
            },
        };
    }
    let mut constants = [("data", value)];
    session
        .define(&HostModule {
            constants: constants
                .iter()
                .map(|(k, v)| ((*k).to_owned(), v.clone()))
                .collect(),
            ..HostModule::untyped("deep", &[])
        })
        .expect("at ceiling");
    let image = session.snapshot().expect("snapshot");
    session = engine.restore(&image).expect("restore at ceiling");
    let value = std::mem::replace(&mut constants[0].1, HostValue::None);
    constants[0].1 = HostValue::List(vec![value]);
    let error = session
        .define(&HostModule {
            constants: constants
                .iter()
                .map(|(k, v)| ((*k).to_owned(), v.clone()))
                .collect(),
            ..HostModule::untyped("deep", &[])
        })
        .expect_err("over ceiling");
    assert!(
        matches!(&error, DefineError::Invalid(m) if m.contains("deeper than 1000")),
        "{error:?}"
    );
    session
        .define(&HostModule::untyped("deep", &[]))
        .expect("release deep value");
    assert!(matches!(
        start(&engine, &mut session, "import deep"),
        Progress::Complete(_)
    ));
}

#[test]
fn sync_host_calls_return_in_place_from_nested_frames() {
    let out = Arc::new(Mutex::new(String::new()));
    let (engine, mut s) = session(&out);
    s.define(&HostModule::untyped("", &["double"]))
        .expect("defined");
    let src = "\
def outer():
    def inner(n):
        total = 0
        for i in range(n):
            total += double(i)
        return total
    squares = [double(x) for x in range(3)]
    lazy = sum(double(x) for x in [10, 20])
    return inner(4), squares, lazy
print(outer())
";
    let mut p = start(&engine, &mut s, src);
    let mut ids = Vec::new();
    while let Progress::HostCall {
        call,
        name,
        args,
        kwargs,
        ..
    } = p
    {
        assert_eq!(name, "double");
        assert_eq!(kwargs.len(), 0);
        ids.push(call);
        let x = int(&args[0]);
        p = s
            .resume(Resume::Value(HostValue::Int(2 * x)))
            .expect("resumes");
    }
    assert!(matches!(p, Progress::Complete(_)), "{p:?}");
    assert_eq!(
        out.lock().expect("output lock").as_str(),
        "(12, [0, 2, 4], 60)\n"
    );
    assert_eq!(ids.len(), 9);
    assert!(ids.windows(2).all(|w| w[0] < w[1]), "{ids:?}");
}

#[test]
fn host_errors_raise_catchable_exceptions_with_tracebacks() {
    let out = Arc::new(Mutex::new(String::new()));
    let (engine, mut s) = session(&out);
    s.define(&HostModule::untyped("", &["fetch"]))
        .expect("defined");
    let src = "\
def get(key):
    return fetch(key)
try:
    get('k')
except KeyError as e:
    print('caught', repr(e))
get('v')
";
    let p = start(&engine, &mut s, src);
    let Progress::HostCall { args, .. } = p else {
        panic!("{p:?}")
    };
    assert_eq!(args, [HostValue::Str("k".into())]);
    let p = s
        .resume(Resume::Error(HostError {
            kind: HostErrorKind::KeyError,
            message: "k".into(),
            data: None,
            class: None,
        }))
        .expect("resumes");
    assert!(matches!(p, Progress::HostCall { .. }), "{p:?}");
    let p = s
        .resume(Resume::Error(HostError {
            kind: HostErrorKind::ValueError,
            message: "bad value".into(),
            data: None,
            class: None,
        }))
        .expect("resumes");
    let Progress::Error(e) = p else {
        panic!("{p:?}")
    };
    assert_eq!(
        out.lock().expect("output lock").as_str(),
        "caught KeyError('k')\n"
    );
    assert_eq!(e.type_name, "ValueError");
    assert_eq!(e.message, "bad value");
    assert_eq!(
        e.traceback,
        "Traceback (most recent call last):\n  File \"test.py\", line 7, in <module>\n    \
         get('v')\n    ~~~^^^^^\n  File \"test.py\", line 2, in get\n    return fetch(key)\n\
         ValueError: bad value\n"
    );
}

#[test]
fn unconvertible_arguments_raise_before_any_round_trip() {
    let out = Arc::new(Mutex::new(String::new()));
    let (engine, mut s) = session(&out);
    s.define(&HostModule::untyped("", &["send"]))
        .expect("defined");
    let src = "\
for bad in ({1, 2}, {1: 'x'}, print):
    try:
        send(bad)
    except TypeError as e:
        print(e)
";
    let p = start(&engine, &mut s, src);
    assert_eq!(p, Progress::Complete(None));
    assert_eq!(
        out.lock().expect("output lock").as_str(),
        "send() argument: a 'set' object cannot be passed to the host\n\
         send() argument: dict keys passed to the host must be str, not int\n\
         send() argument: a 'builtin_function_or_method' object cannot be passed to the host\n"
    );
}

#[test]
fn host_values_convert_both_ways() {
    let out = Arc::new(Mutex::new(String::new()));
    let (engine, mut s) = session(&out);
    s.define(&HostModule::untyped("", &["echo"]))
        .expect("defined");
    let p = start(
        &engine,
        &mut s,
        "r = echo([None, True, -3, 1.5, 'é', (1, 2)], k={'a': [1]})\n\
         print(r, r['rec'].y, r['rec'] == (1, 2), r['rec']._asdict(), type(r['tup']).__name__)\nr",
    );
    let Progress::HostCall { args, kwargs, .. } = p else {
        panic!("{p:?}")
    };
    assert_eq!(
        args,
        [HostValue::List(vec![
            HostValue::None,
            HostValue::Bool(true),
            HostValue::Int(-3),
            HostValue::Float(1.5),
            HostValue::Str("é".into()),
            HostValue::Tuple(vec![HostValue::Int(1), HostValue::Int(2)]),
        ])]
    );
    assert_eq!(
        kwargs,
        [(
            "k".to_owned(),
            HostValue::Dict(vec![(
                "a".to_owned(),
                HostValue::List(vec![HostValue::Int(1)])
            )])
        )]
    );
    let tup = HostValue::Tuple(vec![HostValue::Int(3), HostValue::List(vec![])]);
    let answer = HostValue::Dict(vec![
        ("big".into(), HostValue::BigInt("123".into())),
        ("tup".into(), tup.clone()),
        (
            "rec".into(),
            HostValue::Record {
                name: "Point".into(),
                fields: vec![
                    ("x".into(), HostValue::Int(1)),
                    ("y".into(), HostValue::Int(2)),
                ],
            },
        ),
    ]);
    let p = s.resume(Resume::Value(answer)).expect("resumes");
    // A record arrives as a named tuple and leaves as a record again; a
    // tuple stays a tuple.
    assert_eq!(
        out.lock().expect("output lock").as_str(),
        "{'big': 123, 'tup': (3, []), 'rec': Point(x=1, y=2)} 2 True {'x': 1, 'y': 2} tuple\n"
    );
    let Progress::Complete(Some(c)) = p else {
        panic!("{p:?}")
    };
    assert_eq!(
        c.value,
        Some(HostValue::Dict(vec![
            ("big".into(), HostValue::Int(123)),
            ("tup".into(), tup),
            (
                "rec".into(),
                HostValue::Record {
                    name: "Point".into(),
                    fields: vec![
                        ("x".into(), HostValue::Int(1)),
                        ("y".into(), HostValue::Int(2)),
                    ],
                }
            ),
        ]))
    );
    // So does a `collections.namedtuple` instance.
    let p = start(
        &engine,
        &mut s,
        "import collections\ncollections.namedtuple('P', 'a b')(1, [2])",
    );
    let Progress::Complete(Some(c)) = p else {
        panic!("{p:?}")
    };
    assert_eq!(
        c.value,
        Some(HostValue::Record {
            name: "P".into(),
            fields: vec![
                ("a".into(), HostValue::Int(1)),
                ("b".into(), HostValue::List(vec![HostValue::Int(2)])),
            ],
        })
    );
    // A completion that does not convert keeps only its repr.
    let p = start(&engine, &mut s, "print");
    let Progress::Complete(Some(c)) = p else {
        panic!("{p:?}")
    };
    assert_eq!(
        (c.repr.as_str(), c.value),
        ("<built-in function print>", None)
    );
}

#[test]
fn bytes_and_big_ints_convert_both_ways() {
    let out = Arc::new(Mutex::new(String::new()));
    let (engine, mut s) = session(&out);
    s.define(&HostModule::untyped("", &["echo"]))
        .expect("defined");
    let p = start(
        &engine,
        &mut s,
        "r = echo([b'a\\x00', 2**64, -2**80, 2**62])\nprint(r)\nr",
    );
    let Progress::HostCall { args, .. } = p else {
        panic!("{p:?}")
    };
    assert_eq!(
        args,
        [HostValue::List(vec![
            HostValue::Bytes(vec![b'a', 0]),
            HostValue::BigInt("18446744073709551616".into()),
            HostValue::BigInt("-1208925819614629174706176".into()),
            HostValue::Int(1 << 62),
        ])]
    );
    let answer = HostValue::List(vec![
        HostValue::Bytes(b"\xffz".to_vec()),
        HostValue::BigInt("-340282366920938463463374607431768211456".into()),
        HostValue::BigInt("7".into()),
    ]);
    let p = s.resume(Resume::Value(answer)).expect("resumes");
    assert_eq!(
        out.lock().expect("output lock").as_str(),
        "[b'\\xffz', -340282366920938463463374607431768211456, 7]\n"
    );
    let Progress::Complete(Some(c)) = p else {
        panic!("{p:?}")
    };
    assert_eq!(
        c.value,
        Some(HostValue::List(vec![
            HostValue::Bytes(b"\xffz".to_vec()),
            HostValue::BigInt("-340282366920938463463374607431768211456".into()),
            HostValue::Int(7),
        ]))
    );
}

#[test]
fn host_results_nested_past_the_depth_limit_raise_in_every_container() {
    // A result converts while its innermost value is at most
    // `MAX_HOST_DEPTH` levels down, whichever containers hold it; one
    // level more raises at the call instead of recursing on.
    let wraps: [fn(HostValue) -> HostValue; 4] = [
        |v| HostValue::List(vec![v]),
        |v| HostValue::Tuple(vec![v]),
        |v| HostValue::Dict(vec![("k".into(), v)]),
        |v| HostValue::Record {
            name: "R".into(),
            fields: vec![("k".into(), v)],
        },
    ];
    let src = "try:\n    f()\n    print('ok')\nexcept RecursionError:\n    print('deep')\n";
    let answer = |r: HostValue| {
        let out = Arc::new(Mutex::new(String::new()));
        let (engine, mut s) = session(&out);
        s.define(&HostModule::untyped("", &["f"])).expect("defined");
        let p = start(&engine, &mut s, src);
        assert!(matches!(p, Progress::HostCall { .. }), "{p:?}");
        let p = s.resume(Resume::Value(r)).expect("resumes");
        assert!(matches!(p, Progress::Complete(_)), "{p:?}");
        out.lock().expect("output lock").clone()
    };
    for wrap in wraps {
        let nest = |levels| (0..levels).fold(HostValue::Int(1), |v, _| wrap(v));
        assert_eq!(answer(nest(MAX_HOST_DEPTH)), "ok\n");
        assert_eq!(answer(nest(MAX_HOST_DEPTH + 1)), "deep\n");
    }
}

/// Answers every host call with a future and returns the next event.
fn futures_until_wait(s: &mut Session, mut p: Progress, calls: &mut Vec<(u64, i64)>) -> Progress {
    while let Progress::HostCall { call, args, .. } = p {
        calls.push((call, int(&args[0])));
        p = s.resume(Resume::Future).expect("resumes");
    }
    p
}

#[test]
fn awaited_host_futures_in_sequence() {
    let out = Arc::new(Mutex::new(String::new()));
    let (engine, mut s) = session(&out);
    // The py-ptc shape: async wrappers over private host callbacks.
    s.define(&HostModule::untyped("_ptc", &["fetch"]))
        .expect("defined");
    let src = "\
from _ptc import fetch as _fetch
async def tool(arguments):
    return await _fetch(arguments)
a = await tool(1)
b = await tool(2)
c = await _fetch(3)
[a, b, c]
";
    let mut p = start(&engine, &mut s, src);
    let mut seen = Vec::new();
    for k in 1..=3 {
        let Progress::HostCall {
            call, name, args, ..
        } = p
        else {
            panic!("{p:?}")
        };
        assert_eq!((name.as_str(), int(&args[0])), ("_ptc.fetch", k));
        seen.push(call);
        p = s.resume(Resume::Future).expect("resumes");
        assert_eq!(
            p,
            Progress::Waiting { calls: vec![call] },
            "one future at a time"
        );
        p = s
            .resume(Resume::Batch(vec![(call, Ok(HostValue::Int(k * 100)))]))
            .expect("resumes");
    }
    let Progress::Complete(Some(c)) = p else {
        panic!("{p:?}")
    };
    assert_eq!(c.repr, "[100, 200, 300]");
}

#[test]
fn gather_of_host_futures_waits_once_and_accepts_partial_batches() {
    let out = Arc::new(Mutex::new(String::new()));
    let (engine, mut s) = session(&out);
    s.define(&HostModule::untyped("", &["fetch"]))
        .expect("defined");
    let src = "\
import asyncio
async def twice(x):
    return 2 * await fetch(x)
r = await asyncio.gather(fetch(1), twice(2), fetch(3))
print(r)
r
";
    let mut calls = Vec::new();
    let p = start(&engine, &mut s, src);
    let p = futures_until_wait(&mut s, p, &mut calls);
    let ids: Vec<u64> = calls.iter().map(|c| c.0).collect();
    assert_eq!(calls.iter().map(|c| c.1).collect::<Vec<_>>(), [1, 3, 2]);
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    assert_eq!(p, Progress::Waiting { calls: sorted });
    // Out of order and partial: the last call alone first.
    let by_arg = |a: i64| calls.iter().find(|c| c.1 == a).map(|c| c.0).expect("call");
    let p = s
        .resume(Resume::Batch(vec![(by_arg(2), Ok(HostValue::Int(20)))]))
        .expect("resumes");
    let mut rest = vec![by_arg(1), by_arg(3)];
    rest.sort_unstable();
    assert_eq!(p, Progress::Waiting { calls: rest });
    let p = s
        .resume(Resume::Batch(vec![
            (by_arg(3), Ok(HostValue::Int(30))),
            (by_arg(1), Ok(HostValue::Int(10))),
        ]))
        .expect("resumes");
    let Progress::Complete(Some(c)) = p else {
        panic!("{p:?}")
    };
    assert_eq!(c.repr, "[10, 40, 30]");
    assert_eq!(out.lock().expect("output lock").as_str(), "[10, 40, 30]\n");
}

#[test]
fn an_error_in_one_gathered_call_propagates() {
    let out = Arc::new(Mutex::new(String::new()));
    let (engine, mut s) = session(&out);
    s.define(&HostModule::untyped("", &["fetch"]))
        .expect("defined");
    let src = "\
import asyncio
try:
    await asyncio.gather(fetch(1), fetch(2), fetch(3))
except ValueError as e:
    print('caught', e)
";
    let mut calls = Vec::new();
    let p = start(&engine, &mut s, src);
    let p = futures_until_wait(&mut s, p, &mut calls);
    assert!(
        matches!(&p, Progress::Waiting { calls } if calls.len() == 3),
        "{p:?}"
    );
    let p = s
        .resume(Resume::Batch(vec![
            (calls[0].0, Ok(HostValue::Int(1))),
            (
                calls[1].0,
                Err(HostError {
                    kind: HostErrorKind::ValueError,
                    message: "second failed".into(),
                    data: None,
                    class: None,
                }),
            ),
        ]))
        .expect("resumes");
    assert_eq!(p, Progress::Complete(None));
    assert_eq!(
        out.lock().expect("output lock").as_str(),
        "caught second failed\n"
    );
}

#[test]
fn bad_resumes_are_refused_without_changing_the_cell() {
    let out = Arc::new(Mutex::new(String::new()));
    let (engine, mut s) = session(&out);
    s.define(&HostModule::untyped("", &["fetch"]))
        .expect("defined");
    assert_eq!(
        s.resume(Resume::Continue),
        Err(ResumeError::Unexpected { phase: Phase::Idle })
    );
    let src = "import asyncio\nr = await asyncio.gather(fetch(1), fetch(2))\nr";
    let mut calls = Vec::new();
    let p = start(&engine, &mut s, src);
    let p = futures_until_wait(&mut s, p, &mut calls);
    let (first, second) = (calls[0].0, calls[1].0);
    assert!(matches!(p, Progress::Waiting { .. }));
    assert!(matches!(
        s.resume(Resume::Value(HostValue::None)),
        Err(ResumeError::Unexpected { .. })
    ));
    assert_eq!(
        s.resume(Resume::Batch(vec![(999, Ok(HostValue::None))])),
        Err(ResumeError::UnknownCall { call: 999 })
    );
    assert_eq!(
        s.resume(Resume::Batch(vec![
            (first, Ok(HostValue::None)),
            (first, Ok(HostValue::None))
        ])),
        Err(ResumeError::AlreadySettled { call: first })
    );
    let p = s
        .resume(Resume::Batch(vec![(first, Ok(HostValue::Int(1)))]))
        .expect("resumes");
    assert_eq!(
        p,
        Progress::Waiting {
            calls: vec![second]
        }
    );
    assert_eq!(
        s.resume(Resume::Batch(vec![(first, Ok(HostValue::Int(1)))])),
        Err(ResumeError::AlreadySettled { call: first })
    );
    let p = s
        .resume(Resume::Batch(vec![(second, Ok(HostValue::Int(2)))]))
        .expect("resumes");
    let Progress::Complete(Some(c)) = p else {
        panic!("{p:?}")
    };
    assert_eq!(c.repr, "[1, 2]");
}

/// A graph of 2**40 shared leaves, a few hundred bytes of heap.
const SHARED: &str = "a = [1]\nfor _ in range(40):\n    a = [a, a]\n";

#[test]
fn argument_conversion_is_bounded_in_bytes_even_for_shared_graphs() {
    let out = Arc::new(Mutex::new(String::new()));
    let (engine, mut s) = session(&out);
    s.define(&HostModule::untyped("", &["send"]))
        .expect("defined");
    s.set_host_args(HostArgs {
        max_bytes: 1000,
        ..HostArgs::default()
    });
    let src = format!(
        "{SHARED}for bad in (a, 'x' * 999):\n    try:\n        send(bad)\n    except ValueError as e:\n        print(e)\nsend('x' * 998)"
    );
    let p = start(&engine, &mut s, &src);
    assert_eq!(
        out.lock().expect("output lock").as_str(),
        "send() argument: the value is too large to pass to the host (more than 1000 bytes)\n\
         send() argument: the value is too large to pass to the host (more than 1000 bytes)\n"
    );
    let Progress::HostCall { args, .. } = p else {
        panic!("{p:?}")
    };
    assert_eq!(args, [HostValue::Str("x".repeat(998))]);
}

#[test]
fn lenient_conversion_reports_what_does_not_convert_in_place() {
    let out = Arc::new(Mutex::new(String::new()));
    let (engine, mut s) = session(&out);
    s.define(&HostModule::untyped("", &["send"]))
        .expect("defined");
    s.set_host_args(HostArgs {
        max_bytes: 100,
        max_depth: 3,
        lenient: true,
    });
    let mut calls = Vec::new();
    let src = "\
send([{1, 2}, len], k=None, j=1)
send({'a': 1, 2: 'b', 'c': 3})
send([[[[1]]]], 'after')
send(['x' * 10, 'y' * 100, 'z'])
";
    let mut p = start(&engine, &mut s, src);
    while let Progress::HostCall { args, kwargs, .. } = p {
        calls.push((args, kwargs));
        p = s.resume(Resume::Value(HostValue::None)).expect("resumes");
    }
    assert!(matches!(p, Progress::Complete(_)), "{p:?}");
    let opaque = |type_name: &str, repr: &str| {
        HostValue::Unconverted(Unconverted::Opaque {
            type_name: type_name.into(),
            repr: repr.into(),
        })
    };
    assert_eq!(
        calls,
        [
            (
                vec![HostValue::List(vec![
                    opaque("set", "{1, 2}"),
                    opaque("builtin_function_or_method", "<built-in function len>"),
                ])],
                vec![
                    ("k".to_owned(), HostValue::None),
                    ("j".to_owned(), HostValue::Int(1))
                ]
            ),
            (
                vec![HostValue::Dict(vec![
                    ("a".into(), HostValue::Int(1)),
                    (
                        String::new(),
                        HostValue::Unconverted(Unconverted::KeyNotStr)
                    ),
                ])],
                vec![]
            ),
            // After a stop the remaining arguments are placeholders.
            (
                vec![
                    HostValue::List(vec![HostValue::List(vec![HostValue::List(vec![
                        HostValue::List(vec![HostValue::Unconverted(Unconverted::TooDeep)])
                    ])])]),
                    HostValue::None,
                ],
                vec![]
            ),
            (
                vec![HostValue::List(vec![
                    HostValue::Str("x".repeat(10)),
                    HostValue::Unconverted(Unconverted::TooLarge {
                        type_name: "str".into()
                    }),
                ])],
                vec![]
            ),
        ]
    );
    // An unconverted value is never accepted back.
    let p = start(
        &engine,
        &mut s,
        "try:\n    send(0)\nexcept TypeError as e:\n    print(e)",
    );
    assert!(matches!(p, Progress::HostCall { .. }));
    let p = s
        .resume(Resume::Value(HostValue::Unconverted(
            Unconverted::KeyNotStr,
        )))
        .expect("resumes");
    assert_eq!(p, Progress::Complete(None));
    assert_eq!(
        out.lock().expect("output lock").as_str(),
        "an unconverted host value cannot be returned to Python\n"
    );
}

#[test]
fn completion_text_is_capped_and_huge_reprs_raise_memory_error() {
    let out = Arc::new(Mutex::new(String::new()));
    let (engine, mut s) = session(&out);
    s.set_text_limit(10);
    // The completion value converts within the same byte budget as
    // arguments; a small one keeps this shared graph cheap to refuse.
    s.set_host_args(HostArgs {
        max_bytes: 1000,
        ..HostArgs::default()
    });
    let p = start(&engine, &mut s, &format!("{SHARED}a"));
    let Progress::Complete(Some(c)) = p else {
        panic!("{p:?}")
    };
    assert_eq!((c.repr.as_str(), c.repr_truncated), ("[[[[[[[[[[", true));
    assert_eq!((c.str.as_str(), c.str_truncated), ("[[[[[[[[[[", true));
    assert_eq!(c.value, None);
    let p = start(&engine, &mut s, "'é' * 6");
    let Progress::Complete(Some(c)) = p else {
        panic!("{p:?}")
    };
    assert_eq!((c.str.as_str(), c.str_truncated), ("ééééé", true));
    assert_eq!((c.repr.as_str(), c.repr_truncated), ("'éééé", true));
    let p = start(&engine, &mut s, "'short'");
    let Progress::Complete(Some(c)) = p else {
        panic!("{p:?}")
    };
    assert_eq!(
        (c.str.as_str(), c.str_truncated, c.repr_truncated),
        ("short", false, false)
    );
    // Native leaves must not retain a full-sized rendering buffer either.
    for (source, repr, str) in [
        ("'é' * 131072", "'éééé", "ééééé"),
        ("b'\\x00' * 131072", "b'\\x00\\x00", "b'\\x00\\x00"),
        ("['€' * 65536]", "['€€", "['€€"),
    ] {
        let Progress::Complete(Some(c)) = start(&engine, &mut s, source) else {
            panic!("native completion: {source}")
        };
        assert_eq!((c.repr.as_str(), c.str.as_str()), (repr, str));
        assert!(c.repr_truncated && c.str_truncated);
        assert!(c.repr.capacity() <= 10 && c.str.capacity() <= 10);
    }
    let code = engine
        .compile(
            "class Text:\n    def __repr__(self):\n        return 'é' * 131072\n    def __str__(self):\n        return '€' * 65536\nText()",
            "text.py",
        )
        .expect("code");
    for (limit, repr, str) in [(10, "ééééé", "€€€"), (0, "", ""), (5, "éé", "€")] {
        s.set_text_limit(limit);
        let Progress::Complete(Some(c)) = s.start(&code) else {
            panic!("user-rendered completion")
        };
        assert_eq!((c.repr.as_str(), c.str.as_str()), (repr, str));
        assert!(c.repr_truncated && c.str_truncated);
        let cap = usize::try_from(limit).expect("small");
        assert!(c.repr.capacity() <= cap && c.str.capacity() <= cap);
    }
    // In Python, a repr longer than the memory limit raises.
    let small = Engine::new(EngineConfig::new().with_limits(Limits::new().with_memory(8 << 20)));
    let mut s = Session::new(&small).expect("session");
    let p = start(&small, &mut s, &format!("{SHARED}r = repr(a)"));
    let Progress::Error(e) = p else {
        panic!("{p:?}")
    };
    assert_eq!(e.type_name, "MemoryError");
}

#[test]
fn completion_preflight_releases_its_budget_before_rendering() {
    // The preflight fallback renders full text before the host takes its
    // prefix. Its native output now counts too, including during arena copy.
    for (memory, count, completes) in [
        (1984 << 10, 20000, false),
        (2368 << 10, 20000, true),
        (4032 << 10, 50000, false),
        (8 << 20, 50000, true),
    ] {
        let engine =
            Engine::new(EngineConfig::new().with_limits(Limits::new().with_memory(memory)));
        let mut s = Session::new(&engine).expect("session");
        s.set_text_limit(16);
        s.set_host_args(HostArgs {
            max_bytes: 0,
            ..HostArgs::default()
        });
        assert_eq!(
            start(
                &engine,
                &mut s,
                &format!("data = [] + [[i] for i in range({count})]")
            ),
            Progress::Complete(None)
        );
        s.set_fuel(Some(100));
        let mut p = start(&engine, &mut s, "data");
        if matches!(p, Progress::Parked { .. }) {
            let image = s.snapshot().expect("parked snapshot");
            drop(s);
            s = engine.restore(&image).expect("parked restore");
        }
        while matches!(p, Progress::Parked { .. }) {
            s.set_fuel(Some(1_000_000));
            p = s.resume(Resume::Continue).expect("resume rendering");
        }
        if completes {
            let Progress::Complete(Some(c)) = p else {
                panic!("completion under {memory} bytes: {p:?}")
            };
            assert_eq!(c.repr, "[[0], [1], [2], ");
            assert_eq!(c.str, c.repr);
            assert!(c.repr_truncated && c.str_truncated);
            assert_eq!(c.value, None);
        } else {
            let Progress::Error(error) = p else {
                panic!("unbounded preflight under {memory} bytes: {p:?}")
            };
            assert_eq!(error.type_name, "MemoryError");
        }
        let image = s.snapshot().expect("completed snapshot");
        drop(s);
        let mut s = engine.restore(&image).expect("completed restore");
        s.set_fuel(None);
        assert_eq!(
            start(
                &engine,
                &mut s,
                "data = None\nassert repr([8, 3]) == '[8, 3]'"
            ),
            Progress::Complete(None)
        );
    }
}

#[test]
fn host_errors_can_raise_name_error() {
    let out = Arc::new(Mutex::new(String::new()));
    let (engine, mut s) = session(&out);
    s.define(&HostModule::untyped("", &["tool"]))
        .expect("defined");
    let p = start(
        &engine,
        &mut s,
        "try:\n    tool()\nexcept NameError as e:\n    print(e)",
    );
    assert!(matches!(p, Progress::HostCall { .. }));
    let p = s
        .resume(Resume::Error(HostError {
            kind: HostErrorKind::NameError,
            message: "name 'tool' is not defined".into(),
            data: None,
            class: None,
        }))
        .expect("resumes");
    assert_eq!(p, Progress::Complete(None));
    assert_eq!(
        out.lock().expect("output lock").as_str(),
        "name 'tool' is not defined\n"
    );
}

#[test]
fn tuple_constants_survive_a_snapshot() {
    let out = Arc::new(Mutex::new(String::new()));
    let (engine, mut s) = session(&out);
    let pair = HostValue::Tuple(vec![HostValue::Int(1), HostValue::Tuple(vec![])]);
    s.define(&HostModule {
        constants: [("PAIR", pair.clone())]
            .iter()
            .map(|(k, v)| ((*k).to_owned(), v.clone()))
            .collect(),
        ..HostModule::untyped("config", &[])
    })
    .expect("defined");
    let mut restored = engine
        .restore(&s.snapshot().expect("snapshot"))
        .expect("restores");
    let p = start(&engine, &mut restored, "import config\nconfig.PAIR");
    let Progress::Complete(Some(c)) = p else {
        panic!("{p:?}")
    };
    assert_eq!((c.repr.as_str(), c.value), ("(1, ())", Some(pair)));
}

#[test]
fn host_value_conversion_is_charged_to_the_session_memory_limit() {
    // A host value tree takes about five times the arena's bytes per small
    // int; it used to be built outside `Limits::memory` (a 64 MiB session
    // drove a host to 487 MiB RSS through its completion value).
    let engine = Engine::new(EngineConfig::new().with_limits(Limits::new().with_memory(16 << 20)));
    let mut s = Session::new(&engine).expect("session");
    let out = Arc::new(Mutex::new(String::new()));
    let sink = Arc::clone(&out);
    s.set_output(Box::new(move |t: &str| {
        sink.lock().expect("output lock").push_str(t);
    }));
    s.define(&HostModule::untyped("", &["send"]))
        .expect("defined");
    s.set_host_args(HostArgs {
        max_bytes: u64::MAX,
        ..HostArgs::default()
    });
    let src = "\
big = list(range(1_000_000))
try:
    send(big)
except MemoryError:
    print('MemoryError')
send(big[:100_000])
";
    let p = start(&engine, &mut s, src);
    assert_eq!(out.lock().expect("output lock").as_str(), "MemoryError\n");
    let Progress::HostCall { args, .. } = p else {
        panic!("{p:?}")
    };
    assert!(matches!(&args[..], [HostValue::List(l)] if l.len() == 100_000));
    let p = s.resume(Resume::Value(HostValue::None)).expect("resumes");
    assert!(matches!(p, Progress::Complete(_)), "{p:?}");
    // A completion value too big to convert has no host form; a smaller
    // one still converts once the reservation is gone.
    for (src, converts) in [("big", false), ("big[:100_000]", true), ("big", false)] {
        let Progress::Complete(Some(c)) = start(&engine, &mut s, src) else {
            panic!("{src}")
        };
        assert_eq!(c.value.is_some(), converts, "{src}");
    }
}

#[test]
fn deepest_host_arguments_convert_on_a_small_host_stack() {
    // Conversion recursed once per level (a frame of about 900 bytes),
    // so the 1000-level ceiling needed close to 1 MiB of Rust stack.
    let out = std::thread::Builder::new()
        .stack_size(256 << 10)
        .spawn(|| {
            let out = Arc::new(Mutex::new(String::new()));
            let (engine, mut s) = session(&out);
            s.define(&HostModule::untyped("", &["send"]))
                .expect("defined");
            s.set_host_args(HostArgs {
                max_depth: 1000,
                ..HostArgs::default()
            });
            let src = "x = 1\nfor i in range(600):\n    x = [{'k': (x,)}] if i % 3 == 0 else [x]\nsend(x)";
            let Progress::HostCall { args, .. } = start(&engine, &mut s, src) else {
                panic!("expected a host call")
            };
            let mut depth = 0;
            let mut h = &args[0];
            loop {
                h = match h {
                    HostValue::List(l) | HostValue::Tuple(l) => &l[0],
                    HostValue::Dict(d) => &d[0].1,
                    _ => break,
                };
                depth += 1;
            }
            // Dropping the tree recurses too; dismantle it first.
            let mut stack = args;
            while let Some(h) = stack.pop() {
                match h {
                    HostValue::List(l) | HostValue::Tuple(l) => stack.extend(l),
                    HostValue::Dict(d) => stack.extend(d.into_iter().map(|(_, v)| v)),
                    _ => {}
                }
            }
            depth
        })
        .expect("spawns")
        .join()
        .expect("joins");
    assert_eq!(out, 1000);
}

/// Answers the host call `p` stops at with `answer`, checking its name.
fn answer(s: &mut Session, p: &Progress, name: &str, answer: Resume) -> Progress {
    let Progress::HostCall { name: n, .. } = p else {
        panic!("{p:?}")
    };
    assert_eq!(n, name);
    s.resume(answer).expect("resumes")
}

#[test]
#[allow(clippy::too_many_lines, reason = "one scenario, step by step")]
fn host_errors_carry_a_payload_and_a_host_exception_class_across_snapshots() {
    let out = Arc::new(Mutex::new(String::new()));
    let (engine, mut s) = session(&out);
    s.define(&HostModule {
        exceptions: vec![HostException {
            name: "ToolError".into(),
            base: HostErrorKind::RuntimeError,
            doc: "A tool failed.".into(),
        }],
        ..HostModule::untyped("tools", &["fetch"])
    })
    .expect("defines");
    let src = "\
from tools import ToolError, fetch
print(issubclass(ToolError, RuntimeError), ToolError.__module__, ToolError.__doc__)
mine = ToolError('built here')
print(mine.tool, mine.data)
try:
    fetch('a')
except ToolError as e:
    caught = e
    print('ToolError', e, e.tool, e.data['status'], isinstance(e, RuntimeError))
try:
    fetch('b')
except ValueError as e:
    print('ValueError', e, e.tool, e.data)
fetch('c')
";
    let p = start(&engine, &mut s, src);
    let error = |kind, message: &str, class: Option<&str>, data: Option<HostValue>| HostError {
        kind,
        message: message.into(),
        data,
        class: class.map(Into::into),
    };
    let status = HostValue::Dict(vec![("status".into(), HostValue::Int(404))]);
    // Snapshot at the call, restore, then raise the host-class error.
    let Progress::HostCall { .. } = &p else {
        panic!("{p:?}")
    };
    let image = s.snapshot().expect("snapshot");
    let mut s = engine.restore(&image).expect("restore");
    let sink = Arc::clone(&out);
    s.set_output(Box::new(move |t: &str| {
        sink.lock().expect("lock").push_str(t);
    }));
    let p = answer(
        &mut s,
        &p,
        "tools.fetch",
        Resume::Error(error(
            HostErrorKind::Exception,
            "not found",
            Some("tools.ToolError"),
            Some(status),
        )),
    );
    let p = answer(
        &mut s,
        &p,
        "tools.fetch",
        Resume::Error(error(
            HostErrorKind::ValueError,
            "bad",
            None,
            Some(HostValue::List(vec![HostValue::Int(1)])),
        )),
    );
    let p = answer(
        &mut s,
        &p,
        "tools.fetch",
        Resume::Error(error(
            HostErrorKind::Exception,
            "late",
            Some("tools.ToolError"),
            None,
        )),
    );
    let Progress::Error(e) = p else {
        panic!("{p:?}")
    };
    assert_eq!(e.type_name, "tools.ToolError");
    assert!(
        e.traceback.ends_with("tools.ToolError: late\n"),
        "{}",
        e.traceback
    );
    assert_eq!(
        out.lock().expect("lock").as_str(),
        "True tools A tool failed.\n\
         None None\n\
         ToolError not found tools.fetch 404 True\n\
         ValueError bad tools.fetch [1]\n"
    );
    // The caught exception survives a snapshot between cells.
    let image = s.snapshot().expect("snapshot");
    let mut s = engine.restore(&image).expect("restore");
    let p = start(
        &engine,
        &mut s,
        "(type(caught).__name__, caught.tool, caught.data, str(caught))",
    );
    let Progress::Complete(Some(c)) = p else {
        panic!("{p:?}")
    };
    assert_eq!(
        c.repr,
        "('ToolError', 'tools.fetch', {'status': 404}, 'not found')"
    );
    // An unknown class is the host's mistake: a TypeError in the cell.
    let p = start(&engine, &mut s, "from tools import fetch\nfetch()");
    let p = answer(
        &mut s,
        &p,
        "tools.fetch",
        Resume::Error(error(
            HostErrorKind::Exception,
            "x",
            Some("tools.Nope"),
            None,
        )),
    );
    let Progress::Error(e) = p else {
        panic!("{p:?}")
    };
    assert_eq!(
        (e.type_name.as_str(), e.message.as_str()),
        ("TypeError", "unknown host exception class 'tools.Nope'")
    );
}

#[test]
#[allow(clippy::too_many_lines, reason = "one scenario, step by step")]
fn the_call_log_reports_each_call_and_what_became_of_it() {
    let out = Arc::new(Mutex::new(String::new()));
    let (engine, mut s) = session(&out);
    let ids = s
        .define(&HostModule::untyped("", &["get", "slow"]))
        .expect("defines");
    let src = "\
import asyncio
get(1)
try:
    get(2)
except KeyError:
    pass
pending = slow(3)
async def main():
    return await asyncio.gather(slow(4), slow(5))
asyncio.run(main())
";
    let p = start(&engine, &mut s, src);
    let p = answer(&mut s, &p, "get", Resume::Value(HostValue::None));
    let p = answer(
        &mut s,
        &p,
        "get",
        Resume::Error(HostError::new(HostErrorKind::KeyError, "2")),
    );
    let p = answer(&mut s, &p, "slow", Resume::Future);
    let p = answer(&mut s, &p, "slow", Resume::Future);
    let p = answer(&mut s, &p, "slow", Resume::Future);
    let Progress::Waiting { calls } = p else {
        panic!("{p:?}")
    };
    assert_eq!(calls, [4, 5]);
    let record = |call, function: u32, status| {
        HostCallRecord::new(
            call,
            function,
            if function == ids[0].id { "get" } else { "slow" }.into(),
            status,
        )
    };
    let (get, slow) = (ids[0].id, ids[1].id);
    assert_eq!(
        s.call_log(),
        [
            record(1, get, CallStatus::Returned),
            record(2, get, CallStatus::Raised),
            record(3, slow, CallStatus::Pending),
            record(4, slow, CallStatus::Pending),
            record(5, slow, CallStatus::Pending),
        ]
    );
    // The log rides in snapshots.
    let image = s.snapshot().expect("snapshot");
    let mut s = engine.restore(&image).expect("restore");
    let sink = Arc::clone(&out);
    s.set_output_streams(Box::new(move |_, t: &str| {
        sink.lock().expect("lock").push_str(t);
    }));
    let p = s
        .resume(Resume::Batch(vec![
            (4, Ok(HostValue::Int(4))),
            (
                5,
                Err(HostError::new(
                    HostErrorKind::TimeoutError,
                    "slow timed out",
                )),
            ),
        ]))
        .expect("resumes");
    let Progress::Error(e) = p else {
        panic!("{p:?}")
    };
    assert_eq!(e.type_name, "TimeoutError");
    // The cell failed: the unawaited future is abandoned, and named.
    assert_eq!(
        s.call_log(),
        [
            record(1, get, CallStatus::Returned),
            record(2, get, CallStatus::Raised),
            record(3, slow, CallStatus::Abandoned),
            record(4, slow, CallStatus::Returned),
            record(5, slow, CallStatus::Raised),
        ]
    );
    assert_eq!(
        out.lock().expect("lock").as_str(),
        "<sys>:0: RuntimeWarning: coroutine 'slow' was never awaited\n"
    );
    // The next cell starts a new log.
    assert!(matches!(start(&engine, &mut s, "1"), Progress::Complete(_)));
    assert_eq!(s.call_log(), []);
}

#[test]
fn semaphore_batches_and_wait_for_wait_on_host_futures_across_restores() {
    let out = Arc::new(Mutex::new(String::new()));
    let (engine, mut s) = session(&out);
    s.define(&HostModule::untyped("", &["fetch"]))
        .expect("defined");
    // Two calls at a time; a wait_for deadline counts virtual time only, so
    // it waits for host results however long the host takes.
    let src = "\
import asyncio
sem = asyncio.Semaphore(2)
async def one(x):
    async with sem:
        return await asyncio.wait_for(fetch(x), timeout=1)
r = []
for c in asyncio.as_completed([one(i) for i in range(5)]):
    r.append(await c)
r
";
    let mut p = start(&engine, &mut s, src);
    let mut batches = Vec::new();
    while !matches!(p, Progress::Complete(_)) {
        let mut calls = Vec::new();
        p = futures_until_wait(&mut s, p, &mut calls);
        let Progress::Waiting { calls: ids } = &p else {
            panic!("{p:?}")
        };
        assert_eq!(ids.len(), calls.len(), "{p:?}");
        batches.push(calls.iter().map(|c| c.1).collect::<Vec<_>>());
        // Every batch goes through a snapshot, answered last call first.
        s = engine
            .restore(&s.snapshot().expect("snapshot"))
            .expect("restores");
        let answers = calls
            .iter()
            .rev()
            .map(|&(id, x)| (id, Ok(HostValue::Int(x * 10))))
            .collect();
        p = s.resume(Resume::Batch(answers)).expect("resumes");
    }
    assert_eq!(batches, [vec![0, 1], vec![2, 3], vec![4]]);
    let Progress::Complete(Some(c)) = p else {
        panic!("{p:?}")
    };
    assert_eq!(c.repr, "[10, 0, 30, 20, 40]");
}

fn restored(engine: &Engine, s: &mut Session, out: &Arc<Mutex<String>>) -> Session {
    let mut s = engine
        .restore(&s.snapshot().expect("snapshot"))
        .expect("restores");
    let sink = Arc::clone(out);
    s.set_output(Box::new(move |t: &str| {
        sink.lock().expect("output lock").push_str(t);
    }));
    s
}

#[test]
fn cancelling_a_task_abandons_the_host_call_it_awaits() {
    let out = Arc::new(Mutex::new(String::new()));
    let (engine, mut s) = session(&out);
    let ids = s
        .define(&HostModule::untyped("", &["fetch"]))
        .expect("defined");
    let src = "\
import asyncio
async def worker(i):
    try:
        return await fetch(i)
    finally:
        print('finally', i)
async def main():
    t = asyncio.create_task(worker(1))
    u = asyncio.create_task(worker(2))
    await asyncio.sleep(0)
    print(t.cancel('stop'))
    try:
        await t
    except asyncio.CancelledError as e:
        print('t', repr(e), t.cancelled())
    return await u
asyncio.run(main())
";
    let mut calls = Vec::new();
    let p = start(&engine, &mut s, src);
    let p = futures_until_wait(&mut s, p, &mut calls);
    assert_eq!(calls, [(1, 1), (2, 2)]);
    // Only the call left pending is waited on; the cancelled one is
    // abandoned at once.
    let Progress::Waiting { calls: waiting } = &p else {
        panic!("{p:?}")
    };
    assert_eq!(waiting, &[2]);
    let record = |call, status| HostCallRecord::new(call, ids[0].id, "fetch".into(), status);
    assert_eq!(
        s.call_log(),
        [
            record(1, CallStatus::Abandoned),
            record(2, CallStatus::Pending)
        ]
    );
    let mut s = restored(&engine, &mut s, &out);
    assert_eq!(s.call_log()[0].status, CallStatus::Abandoned);
    // A host that ran the cancelled call anyway may still answer it.
    let p = s
        .resume(Resume::Batch(vec![
            (1, Ok(HostValue::Int(10))),
            (2, Ok(HostValue::Int(20))),
        ]))
        .expect("resumes");
    assert!(matches!(p, Progress::Complete(_)), "{p:?}");
    assert_eq!(
        out.lock().expect("lock").as_str(),
        "True\nfinally 1\nt CancelledError('stop') True\nfinally 2\n"
    );
    assert_eq!(
        s.call_log(),
        [
            record(1, CallStatus::Abandoned),
            record(2, CallStatus::Returned)
        ]
    );
}

#[test]
fn asyncio_run_cancels_tasks_waiting_on_the_host_and_waits_for_their_cleanup() {
    let out = Arc::new(Mutex::new(String::new()));
    let (engine, mut s) = session(&out);
    s.define(&HostModule::untyped("", &["fetch"]))
        .expect("defined");
    // The task left when main returns is cancelled at its host call; its
    // cleanup makes another host call, which the cell waits for (through a
    // snapshot) before asyncio.run returns.
    let src = "\
import asyncio
async def bg():
    try:
        await fetch(1)
    except asyncio.CancelledError as e:
        print('cleanup', repr(e), await fetch(2))
        raise
    finally:
        print('bg finally')
async def main():
    asyncio.create_task(bg())
    await asyncio.sleep(0)
    return 'main'
print(asyncio.run(main()))
";
    let mut calls = Vec::new();
    let p = start(&engine, &mut s, src);
    let p = futures_until_wait(&mut s, p, &mut calls);
    assert_eq!(calls, [(1, 1), (2, 2)]);
    let Progress::Waiting { calls: waiting } = &p else {
        panic!("{p:?}")
    };
    assert_eq!(waiting, &[2]);
    let statuses = |s: &Session| s.call_log().iter().map(|r| r.status).collect::<Vec<_>>();
    assert_eq!(statuses(&s), [CallStatus::Abandoned, CallStatus::Pending]);
    let mut s = restored(&engine, &mut s, &out);
    let p = s
        .resume(Resume::Batch(vec![(2, Ok(HostValue::Str("done".into())))]))
        .expect("resumes");
    assert!(matches!(p, Progress::Complete(_)), "{p:?}");
    assert_eq!(
        out.lock().expect("lock").as_str(),
        "cleanup CancelledError() done\nbg finally\nmain\n"
    );
    assert_eq!(statuses(&s), [CallStatus::Abandoned, CallStatus::Returned]);
}

#[test]
fn host_calls_under_lock_queue_taskgroup_and_wait_survive_restores_and_abandon_on_cancel() {
    let out = Arc::new(Mutex::new(String::new()));
    let (engine, mut s) = session(&out);
    s.define(&HostModule::untyped("", &["fetch"]))
        .expect("defined");
    // Every wait goes through a snapshot: a task holds the lock over a host
    // call while a sibling waits for the lock and a producer waits on a
    // full queue; a failing sibling tears the TaskGroup down, which
    // abandons the held call; then asyncio.wait returns on the first
    // answer and the loser is cancelled.
    let src = "\
import asyncio
lock = asyncio.Lock()
q = asyncio.Queue(1)
async def locked(i):
    async with lock:
        return await fetch(i)
async def failing():
    raise ValueError(await fetch(99))
async def call(i):
    return await fetch(i)
async def main():
    await q.put('x')
    async def producer():
        await q.put('y')
        return 'put'
    p = asyncio.create_task(producer())
    try:
        async with asyncio.TaskGroup() as tg:
            tg.create_task(locked(1))
            tg.create_task(locked(2))
            tg.create_task(failing())
    except ExceptionGroup as eg:
        print('eg', eg.exceptions)
    print(lock.locked(), q.get_nowait(), await p, q.get_nowait())
    done, pending = await asyncio.wait(
        [asyncio.create_task(call(3)), asyncio.create_task(call(4))],
        return_when=asyncio.FIRST_COMPLETED)
    for t in pending:
        t.cancel()
    print(len(done), len(pending), [t.result() for t in done])
asyncio.run(main())
";
    let statuses = |s: &Session| s.call_log().iter().map(|r| r.status).collect::<Vec<_>>();
    let mut calls = Vec::new();
    let p = start(&engine, &mut s, src);
    let p = futures_until_wait(&mut s, p, &mut calls);
    assert_eq!(calls, [(1, 1), (2, 99)]);
    assert!(
        matches!(&p, Progress::Waiting { calls } if calls == &[1, 2]),
        "{p:?}"
    );
    let mut s = restored(&engine, &mut s, &out);
    let p = s
        .resume(Resume::Batch(vec![(2, Ok(HostValue::Str("bad".into())))]))
        .expect("resumes");
    calls.clear();
    let p = futures_until_wait(&mut s, p, &mut calls);
    assert_eq!(calls, [(3, 3), (4, 4)]);
    assert!(
        matches!(&p, Progress::Waiting { calls } if calls == &[3, 4]),
        "{p:?}"
    );
    assert_eq!(
        statuses(&s),
        [
            CallStatus::Abandoned,
            CallStatus::Returned,
            CallStatus::Pending,
            CallStatus::Pending
        ]
    );
    let mut s = restored(&engine, &mut s, &out);
    let p = s
        .resume(Resume::Batch(vec![(4, Ok(HostValue::Int(40)))]))
        .expect("resumes");
    assert!(matches!(p, Progress::Complete(_)), "{p:?}");
    assert_eq!(
        out.lock().expect("lock").as_str(),
        "eg (ValueError('bad'),)\nFalse x put y\n1 1 [40]\n"
    );
    assert_eq!(
        statuses(&s),
        [
            CallStatus::Abandoned,
            CallStatus::Returned,
            CallStatus::Abandoned,
            CallStatus::Returned
        ]
    );
}

#[test]
fn host_calls_in_runner_barrier_and_except_star_survive_restores() {
    let out = Arc::new(Mutex::new(String::new()));
    let (engine, mut s) = session(&out);
    s.define(&HostModule::untyped("", &["fetch"]))
        .expect("defined");
    // Each wait goes through a snapshot: two Runner.run() calls, the first
    // with parties waiting at a Barrier and a task cancelled over its host
    // call, the second awaiting a host call inside an except* handler.
    let src = "\
import asyncio
b = asyncio.Barrier(2)
async def party(i):
    r = await fetch(i)
    print('party', i, b.n_waiting)
    await b.wait()
    return r
async def bg():
    try:
        await fetch(3)
    finally:
        print('bg cancelled')
async def main1():
    t = asyncio.create_task(bg())
    r = await asyncio.gather(party(1), party(2))
    t.cancel()
    await asyncio.sleep(0)
    return r
async def main2():
    try:
        try:
            raise ExceptionGroup('g', [ValueError(1), KeyError(2)])
        except* ValueError:
            print('v', await fetch(4))
    except* KeyError as e:
        print('k', repr(e))
with asyncio.Runner() as r:
    print(r.run(main1()))
    r.run(main2())
";
    let statuses = |s: &Session| s.call_log().iter().map(|r| r.status).collect::<Vec<_>>();
    let mut calls = Vec::new();
    let p = start(&engine, &mut s, src);
    let p = futures_until_wait(&mut s, p, &mut calls);
    assert_eq!(calls, [(1, 3), (2, 1), (3, 2)]);
    assert!(
        matches!(&p, Progress::Waiting { calls } if calls == &[1, 2, 3]),
        "{p:?}"
    );
    let mut s = restored(&engine, &mut s, &out);
    let p = s
        .resume(Resume::Batch(vec![(3, Ok(HostValue::Str("b".into())))]))
        .expect("resumes");
    let p = futures_until_wait(&mut s, p, &mut calls);
    assert!(
        matches!(&p, Progress::Waiting { calls } if calls == &[1, 2]),
        "{p:?}"
    );
    let mut s = restored(&engine, &mut s, &out);
    let p = s
        .resume(Resume::Batch(vec![(2, Ok(HostValue::Str("a".into())))]))
        .expect("resumes");
    calls.clear();
    let p = futures_until_wait(&mut s, p, &mut calls);
    assert_eq!(calls, [(4, 4)]);
    assert!(
        matches!(&p, Progress::Waiting { calls } if calls == &[4]),
        "{p:?}"
    );
    assert_eq!(
        statuses(&s),
        [
            CallStatus::Abandoned,
            CallStatus::Returned,
            CallStatus::Returned,
            CallStatus::Pending
        ]
    );
    let mut s = restored(&engine, &mut s, &out);
    let p = s
        .resume(Resume::Batch(vec![(4, Ok(HostValue::Str("d".into())))]))
        .expect("resumes");
    assert!(matches!(p, Progress::Complete(_)), "{p:?}");
    assert_eq!(
        out.lock().expect("lock").as_str(),
        "party 2 0\nparty 1 1\nbg cancelled\n['a', 'b']\nv d\nk ExceptionGroup('g', [KeyError(2)])\n"
    );
}

#[test]
fn the_byte_budget_is_compact_json_size_to_the_byte() {
    // `HostArgs::max_bytes` bounds a call's arguments by (a lower bound of)
    // their compact JSON size; for these it is the exact size of the JSON
    // shown, so the call converts at that budget and stops one byte under.
    let cases = [
        // {"ab":[1,"xyz",null],"cd":true}
        ("{'ab': [1, 'xyz', None], 'cd': True}", 31),
        // [1,22,333]
        ("[1, 22, 333]", 10),
        // ["a","b"]
        ("('a', 'b')", 9),
        // {"xa":1,"yb":2}: a named tuple goes as a record of its fields.
        ("collections.namedtuple('P', 'xa yb')(1, 2)", 15),
        // "{1, 2}": what does not convert goes as its repr.
        ("{1, 2}", 8),
    ];
    for (arg, json) in cases {
        let converts = |max_bytes: u64| {
            let out = Arc::new(Mutex::new(String::new()));
            let (engine, mut s) = session(&out);
            s.define(&HostModule::untyped("", &["send"]))
                .expect("defined");
            s.set_host_args(HostArgs {
                max_bytes,
                lenient: true,
                ..HostArgs::default()
            });
            let p = start(
                &engine,
                &mut s,
                &format!("import collections\nsend({arg})\n"),
            );
            let Progress::HostCall { args, .. } = p else {
                panic!("{arg}: {p:?}")
            };
            !format!("{args:?}").contains("TooLarge")
        };
        assert!(converts(json), "{arg} at {json} bytes");
        assert!(!converts(json - 1), "{arg} at {} bytes", json - 1);
    }
}

#[test]
fn host_args_depth_is_capped_and_completions_convert_strictly() {
    // A `max_depth` past the ceiling (1000) is capped there: a value 1001
    // levels deep stops at the ceiling even when the host asks for more.
    let out = Arc::new(Mutex::new(String::new()));
    let (engine, mut s) = session(&out);
    s.define(&HostModule::untyped("", &["send"]))
        .expect("defined");
    s.set_host_args(HostArgs {
        max_depth: u32::MAX,
        lenient: true,
        ..HostArgs::default()
    });
    let p = start(
        &engine,
        &mut s,
        "x = 1\nfor _ in range(1001):\n    x = [x]\nsend(x)\n",
    );
    let Progress::HostCall { args, .. } = p else {
        panic!("{p:?}")
    };
    assert!(format!("{args:?}").contains("TooDeep"));
    // Lenient arguments leave a cell's completion value strict: one that
    // does not convert keeps only its repr.
    let p = s.resume(Resume::Value(HostValue::None)).expect("resumes");
    assert!(matches!(p, Progress::Complete(_)), "{p:?}");
    let p = start(&engine, &mut s, "{1, 2}");
    let Progress::Complete(Some(c)) = p else {
        panic!("{p:?}")
    };
    assert_eq!((c.repr.as_str(), c.value), ("{1, 2}", None));
}
