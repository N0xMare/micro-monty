//! The encoding round-trips what the facade defines and refuses, never
//! panics on, every truncation of its input. The end-to-end check against
//! a real guest is `crates/mm-wasm-host/tests/parity.rs`.
#![allow(clippy::unwrap_used, reason = "tests")]

use micro_monty::{
    CallStatus, CompileError, Completion, DefineError, HintGroup, HostArgs, HostCallRecord,
    HostError, HostErrorKind, HostException, HostFunctionRef, HostModule, HostParam, HostSignature,
    HostValue, Limits, ParamKind, ParkReason, Phase, Progress, PyException, Resume, ResumeError,
    SnapshotError, TextCut, Unconverted,
};
use mm_wasm_abi::Bad;
use mm_wasm_abi::{
    MAX_DEPTH, RESERVED, Reader, SessionOptions, event, get_call_log, get_compile_error,
    get_define_error, get_function_refs, get_module, get_modules, get_options, get_progress,
    get_resume, get_resume_error, get_snapshot_error, get_stubs, get_value, hint_group, option,
    put_call_log, put_compile_error, put_define_error, put_function_refs, put_module, put_modules,
    put_opt_bytes, put_options, put_progress, put_resume, put_resume_error, put_snapshot_error,
    put_stubs, put_value, text_cut, text_cut_code, text_cut_from_code,
};

fn module() -> HostModule {
    HostModule {
        name: "tools".into(),
        functions: vec![HostSignature {
            name: "search".into(),
            params: [
                ("q", ParamKind::PositionalOrKeyword),
                ("rest", ParamKind::VarPositional),
                ("limit", ParamKind::KeywordOnly),
                ("extra", ParamKind::VarKeyword),
            ]
            .into_iter()
            .map(|(name, kind)| HostParam {
                name: name.into(),
                annotation: "int".into(),
                default: (kind == ParamKind::KeywordOnly).then_some(HostValue::Int(-3)),
                kind,
            })
            .collect(),
            returns: "list".into(),
            doc: "Search.".into(),
            dict_as_kwargs: true,
        }],
        classes: Vec::new(),
        constants: vec![("PI".into(), HostValue::Float(3.5))],
        exceptions: vec![HostException {
            name: "ToolError".into(),
            base: HostErrorKind::TimeoutError,
            doc: "d".into(),
        }],
    }
}

fn resume() -> Resume {
    Resume::Batch(vec![
        (
            7,
            Ok(HostValue::Tuple(vec![HostValue::BigInt("-1".repeat(30))])),
        ),
        (
            9,
            Err(HostError {
                kind: HostErrorKind::KeyError,
                message: "m".into(),
                data: Some(HostValue::Dict(vec![("k".into(), HostValue::None)])),
                class: Some("tools.ToolError".into()),
            }),
        ),
    ])
}

fn log() -> Vec<HostCallRecord> {
    [
        CallStatus::Abandoned,
        CallStatus::Returned,
        CallStatus::Pending,
        CallStatus::Raised,
    ]
    .into_iter()
    .zip(1..)
    .map(|(status, call)| HostCallRecord::new(call, 2, "tools.search".into(), status))
    .collect()
}

#[test]
fn round_trips_and_refuses_truncation() {
    let (mut m, mut r, mut l) = (Vec::new(), Vec::new(), Vec::new());
    put_module(&mut m, &module());
    put_resume(&mut r, &resume());
    put_call_log(&mut l, &log());
    assert_eq!(get_module(&mut Reader::new(&m)).unwrap(), module());
    assert_eq!(get_resume(&mut Reader::new(&r)).unwrap(), resume());
    assert_eq!(get_call_log(&mut Reader::new(&l)).unwrap(), log());
    for n in 0..m.len() {
        assert!(
            get_module(&mut Reader::new(&m[..n])).is_err(),
            "module[..{n}]"
        );
    }
    for n in 0..r.len() {
        assert!(
            get_resume(&mut Reader::new(&r[..n])).is_err(),
            "resume[..{n}]"
        );
    }
    let modules = vec![HostModule::untyped("", &["a"]), module()];
    let mut ms = Vec::new();
    put_modules(&mut ms, &modules);
    assert_eq!(get_modules(&mut Reader::new(&ms)).unwrap(), modules);
    for n in 0..ms.len() {
        assert!(
            get_modules(&mut Reader::new(&ms[..n])).is_err(),
            "modules[..{n}]"
        );
    }
    for n in 0..l.len() {
        assert!(
            get_call_log(&mut Reader::new(&l[..n])).is_err(),
            "log[..{n}]"
        );
    }
}

/// Each error variant and park reason round-trips with its details, and
/// each decoder refuses [`RESERVED`] and the first tag past its own
/// rather than guess a variant.
/// Round-trips each of `items`, and checks that `get` refuses `next_tag`
/// and [`RESERVED`].
fn check<T: PartialEq + std::fmt::Debug>(
    items: &[T],
    put: fn(&mut Vec<u8>, &T),
    get: fn(&mut Reader<'_>) -> Result<T, Bad>,
    next_tag: u8,
) {
    for item in items {
        let mut out = Vec::new();
        put(&mut out, item);
        assert_eq!(&get(&mut Reader::new(&out)).unwrap(), item);
    }
    for tag in [next_tag, RESERVED] {
        assert!(
            get(&mut Reader::new(&[tag, 0, 0, 0, 0])).is_err(),
            "tag {tag}"
        );
    }
}

#[test]
fn every_variant_round_trips_and_unknown_tags_are_refused() {
    let mut build_id = [0u8; 32];
    build_id[0] = 0xab;
    build_id[31] = 7;
    check(
        &[
            SnapshotError::Incompatible {
                format: 3,
                build_id,
            },
            SnapshotError::Corrupt {
                what: "the heap".into(),
            },
            SnapshotError::TooLarge,
            SnapshotError::Busy,
            SnapshotError::Poisoned,
            SnapshotError::Internal {
                message: "boom".into(),
            },
        ],
        put_snapshot_error,
        get_snapshot_error,
        6,
    );
    check(
        &[
            ResumeError::Unexpected { phase: Phase::Idle },
            ResumeError::Unexpected {
                phase: Phase::HostCall,
            },
            ResumeError::Unexpected {
                phase: Phase::Waiting,
            },
            ResumeError::Unexpected {
                phase: Phase::Parked,
            },
            ResumeError::UnknownCall { call: 9 },
            ResumeError::AlreadySettled { call: u64::MAX },
        ],
        put_resume_error,
        get_resume_error,
        3,
    );
    check(
        &[
            DefineError::Invalid("bad name".into()),
            DefineError::Busy,
            DefineError::Raised(PyException::new(
                "MemoryError".into(),
                "m".into(),
                "MemoryError: m\n".into(),
                false,
            )),
            DefineError::Raised(PyException::new(
                "KeyError".into(),
                "frozenset({".into(),
                "KeyError: frozenset({".into(),
                true,
            )),
            DefineError::Poisoned("panic".into()),
        ],
        put_define_error,
        get_define_error,
        4,
    );
    check(
        &[
            Resume::Continue,
            Resume::Value(HostValue::None),
            Resume::Error(HostError::new(HostErrorKind::ValueError, "v")),
            Resume::Future,
            resume(),
        ],
        put_resume,
        get_resume,
        5,
    );
    // An unknown phase inside `Unexpected`.
    assert!(get_resume_error(&mut Reader::new(&[0, 4])).is_err());
    for reason in [
        ParkReason::Fuel,
        ParkReason::Requested,
        ParkReason::Deadline,
    ] {
        let parked = Progress::Parked { reason };
        let mut out = Vec::new();
        assert_eq!(put_progress(&mut out, &parked), event::PARKED);
        assert_eq!(
            get_progress(event::PARKED, &mut Reader::new(&out)).unwrap(),
            parked
        );
    }
    for tag in [3, RESERVED] {
        assert!(get_progress(event::PARKED, &mut Reader::new(&[tag])).is_err());
    }
}

/// What the engine sends at a host call, encoded and decoded: `src` calls
/// `t.f(*a, **k)` with host argument nesting 1000.
fn host_call_round_trip(src: &str, lenient: bool) -> Result<Progress, Bad> {
    use micro_monty::{Engine, EngineConfig, HostArgs, Progress, Session};
    let engine = Engine::new(EngineConfig::default());
    let mut s = Session::new(&engine).unwrap();
    s.set_host_args(HostArgs {
        max_depth: 1000,
        lenient,
        ..HostArgs::default()
    });
    s.define(&HostModule::untyped("t", &["f"])).unwrap();
    let progress = s.start(&engine.compile(src, "<t>").unwrap());
    assert!(
        matches!(progress, Progress::HostCall { .. }),
        "{progress:?}"
    );
    let mut out = Vec::new();
    let code = put_progress(&mut out, &progress);
    get_progress(code, &mut Reader::new(&out))
}

#[test]
fn whatever_the_engine_sends_at_its_depth_limit_decodes() {
    let deep = "x = None\nfor _ in range(DEPTH):\n    x = [x]\nimport t\n";
    // A keyword argument as deep as a positional one.
    let src = format!("{}t.f(k=x)\n", deep.replace("DEPTH", "999"));
    assert!(host_call_round_trip(&src, false).is_ok());
    let src = format!("{}t.f(x)\n", deep.replace("DEPTH", "999"));
    assert!(host_call_round_trip(&src, false).is_ok());
    // Lenient conversion marks the rest `TooDeep`, one level below.
    let src = format!("{}t.f(x)\n", deep.replace("DEPTH", "1200"));
    assert!(host_call_round_trip(&src, true).is_ok());
}

#[test]
fn nested_counts_do_not_multiply_preallocation() {
    // 1000 nested lists, each claiming as many items as there are bytes
    // left: every count passes its check, so preallocating each in full
    // would ask for about 1000 × 1 MiB × size_of::<HostValue>().
    let total = 1 << 20;
    let mut input = Vec::new();
    for _ in 0..1000 {
        let left = u32::try_from(total - input.len() - 5).unwrap();
        input.push(8);
        input.extend_from_slice(&left.to_le_bytes());
    }
    input.resize(total, 0xff);
    assert!(get_value(&mut Reader::new(&input)).is_err());
}

/// A value with every `HostValue` variant, at least once each.
fn every_value() -> HostValue {
    let fields = |v: HostValue| vec![("a".to_owned(), v), ("b".to_owned(), HostValue::None)];
    HostValue::List(vec![
        HostValue::None,
        HostValue::Bool(false),
        HostValue::Bool(true),
        HostValue::Int(-7),
        HostValue::BigInt("-123456789012345678901234567890".into()),
        HostValue::Float(-2.5),
        HostValue::Str("é".into()),
        HostValue::Bytes(vec![0, 255]),
        HostValue::Tuple(vec![HostValue::Str(String::new())]),
        HostValue::Dict(fields(HostValue::Bool(true))),
        HostValue::Record {
            name: "Point".into(),
            fields: fields(HostValue::Int(1)),
        },
        HostValue::Object {
            class: "tools.Job".into(),
            fields: fields(HostValue::Bytes(Vec::new())),
        },
        HostValue::Unconverted(Unconverted::Opaque {
            type_name: "set".into(),
            repr: "{1}".into(),
        }),
        HostValue::Unconverted(Unconverted::KeyNotStr),
        HostValue::Unconverted(Unconverted::TooDeep),
        HostValue::Unconverted(Unconverted::TooLarge {
            type_name: "str".into(),
        }),
    ])
}

fn value_round_trip(v: &HostValue) -> Result<HostValue, Bad> {
    let mut out = Vec::new();
    put_value(&mut out, v);
    let mut r = Reader::new(&out);
    let got = get_value(&mut r)?;
    assert_eq!(r.rest(), b"", "{v:?} left input");
    Ok(got)
}

#[test]
fn every_value_and_progress_event_round_trips() {
    let v = every_value();
    assert_eq!(value_round_trip(&v).unwrap(), v);
    let call = Progress::HostCall {
        call: 3,
        function: 2,
        name: "tools.search".into(),
        args: vec![v.clone(), HostValue::Int(1)],
        kwargs: vec![("k".into(), HostValue::Str("v".into()))],
    };
    let progress = [
        (Progress::Complete(None), event::COMPLETE),
        (
            Progress::Complete(Some(Completion::new(
                "'x'".into(),
                "x".into(),
                "str".into(),
                Some(v),
                true,
                false,
            ))),
            event::COMPLETE,
        ),
        (call, event::HOST_CALL),
        (Progress::Waiting { calls: vec![4, 9] }, event::WAITING),
        (
            Progress::Error(PyException::new(
                "ValueError".into(),
                "bad".into(),
                "ValueError: bad\n".into(),
                true,
            )),
            event::ERROR,
        ),
    ];
    for (p, code) in progress {
        let mut out = Vec::new();
        assert_eq!(put_progress(&mut out, &p), code, "{p:?}");
        assert_eq!(get_progress(code, &mut Reader::new(&out)).unwrap(), p);
    }
}

/// An options record of `entries` (tag, payload), as a host writes one.
fn record(entries: &[(u8, &[u8])]) -> Vec<u8> {
    let mut e = Vec::new();
    for (tag, payload) in entries {
        e.push(*tag);
        e.extend_from_slice(&u32::try_from(payload.len()).unwrap().to_le_bytes());
        e.extend_from_slice(payload);
    }
    let mut out = u32::try_from(e.len()).unwrap().to_le_bytes().to_vec();
    out.extend_from_slice(&e);
    out
}

fn options(bytes: &[u8]) -> Result<SessionOptions, Bad> {
    get_options(&mut Reader::new(bytes))
}

#[test]
fn the_options_record_carries_what_differs_from_the_defaults() {
    let mut all = SessionOptions::new().with_limits(
        Limits::new()
            .with_memory(5 << 20)
            .with_recursion_depth(77)
            .with_fuel(Some(12_345)),
    );
    all.text_limit = Some(0);
    all.last_expr_value = false;
    all.host_args = HostArgs {
        max_bytes: 999,
        max_depth: 12,
        lenient: true,
    };
    all.wall_clock = false;
    all.entropy = false;
    all.zone_source = false;
    all.text_cut = TextCut::HeadTail;
    all.int_bits_text = true;
    all.gc_stress = true;
    let mut out = Vec::new();
    put_options(&mut out, &all);
    assert_eq!(options(&out), Ok(all));
    for n in 0..out.len() {
        assert!(options(&out[..n]).is_err(), "options[..{n}]");
    }
    // Defaults are left out: an empty record is the defaults.
    let mut empty = Vec::new();
    put_options(&mut empty, &SessionOptions::default());
    assert_eq!(empty, [0, 0, 0, 0]);
    assert_eq!(options(&empty), Ok(SessionOptions::new()));
    let mut one = SessionOptions::new();
    one.gc_stress = true;
    let mut out = Vec::new();
    put_options(&mut out, &one);
    assert_eq!(out, record(&[(option::GC_STRESS, &[1])]));

    // The record's length bounds it: what follows (a restore's image) is
    // not read as entries, even when it would decode as one.
    let mut input = record(&[(option::RECURSION_DEPTH, &9u32.to_le_bytes())]);
    let image = record(&[(option::MEMORY, &7u64.to_le_bytes())]);
    input.extend_from_slice(&image);
    let mut r = Reader::new(&input);
    let o = get_options(&mut r).unwrap();
    assert_eq!(
        (o.limits.recursion_depth, o.limits.memory),
        (9, SessionOptions::new().limits.memory)
    );
    assert_eq!(r.rest(), &image[..]);

    // Any order; u64::MAX fuel is unlimited.
    let o = options(&record(&[
        (option::GC_STRESS, &[1]),
        (option::FUEL, &u64::MAX.to_le_bytes()),
        (option::MEMORY, &(3u64 << 20).to_le_bytes()),
    ]))
    .unwrap();
    assert_eq!(
        (o.limits.memory, o.limits.fuel, o.gc_stress),
        (3 << 20, None, true)
    );
}

#[test]
fn the_options_record_skips_only_ignorable_unknown_tags() {
    let depth = 9u32.to_le_bytes();
    let around = |tag: u8, payload: &[u8]| {
        options(&record(&[
            (option::RECURSION_DEPTH, &depth),
            (tag, payload),
            (option::INT_BITS_TEXT, &[1]),
        ]))
    };
    for tag in [option::IGNORABLE, 0xa5, RESERVED] {
        let o = around(tag, b"anything, of any length").unwrap();
        assert_eq!(
            (o.limits.recursion_depth, o.int_bits_text),
            (9, true),
            "tag {tag:#x}"
        );
    }
    for tag in [0x00, option::GC_STRESS + 1, option::IGNORABLE - 1] {
        assert_eq!(around(tag, &[]), Err(Bad("unknown option")), "tag {tag:#x}");
    }
    // A repeated tag is refused rather than one of its values taken.
    assert!(
        options(&record(&[
            (option::MEMORY, &(1u64 << 20).to_le_bytes()),
            (option::MEMORY, &(2u64 << 20).to_le_bytes()),
        ]))
        .is_err()
    );
    // A payload must be exactly its tag's encoding.
    for (tag, payload) in [
        (option::MEMORY, &[0u8; 7][..]),
        (option::MEMORY, &[0u8; 9][..]),
        (option::RECURSION_DEPTH, &[0u8; 8][..]),
        (option::WALL_CLOCK, &[][..]),
        (option::WALL_CLOCK, &[2][..]),
        (option::ENTROPY, &[1, 0][..]),
        (option::TEXT_CUT, &[2][..]),
    ] {
        assert!(
            options(&record(&[(tag, payload)])).is_err(),
            "{tag:#x} {payload:?}"
        );
    }
}

#[test]
fn session_payloads_and_codes_round_trip() {
    let mut out = Vec::new();
    for b in [Some(&b"tz"[..]), None, Some(&[][..])] {
        put_opt_bytes(&mut out, b);
    }
    let mut r = Reader::new(&out);
    for b in [Some(&b"tz"[..]), None, Some(&[][..])] {
        assert_eq!(r.opt_bytes().unwrap(), b);
        if b.is_none() {
            // What follows, as the guest's restore takes the image.
            assert_eq!(r.rest(), &out[out.len() - 5..]);
        }
    }
    assert_eq!(r.rest(), b"");

    let refs = vec![
        HostFunctionRef::new("search", 0),
        HostFunctionRef::new("t.f", 7),
    ];
    let mut out = Vec::new();
    put_function_refs(&mut out, &refs);
    assert_eq!(get_function_refs(&mut Reader::new(&out)).unwrap(), refs);
    let stubs = vec![
        (String::new(), "def f(x): ...\n".to_owned()),
        ("tools".to_owned(), String::new()),
    ];
    let mut out = Vec::new();
    put_stubs(&mut out, &stubs);
    assert_eq!(get_stubs(&mut Reader::new(&out)).unwrap(), stubs);
    let e = CompileError::new("SyntaxError".into(), "m".into(), 3, 9, "r".into());
    let mut out = Vec::new();
    put_compile_error(&mut out, &e);
    assert_eq!(get_compile_error(&mut Reader::new(&out)).unwrap(), e);

    for (cut, code) in [
        (TextCut::Head, text_cut::HEAD),
        (TextCut::HeadTail, text_cut::HEAD_TAIL),
    ] {
        assert_eq!(text_cut_code(cut), code);
        assert_eq!(text_cut_from_code(code), Ok(cut));
    }
    assert!(text_cut_from_code(2).is_err());
    for g in HintGroup::ALL {
        assert_eq!(hint_group(g.key()), Some(g));
    }
    assert_eq!(hint_group("no such group"), None);
}

#[test]
fn values_nested_past_the_depth_limit_are_refused_in_every_container() {
    // The engine's lenient conversion puts `TooDeep` one level below
    // `MAX_DEPTH`, so a leaf at `MAX_DEPTH + 1` decodes and one deeper is
    // refused, whichever containers carry it.
    let nest = |levels: u32, wrap: fn(HostValue) -> HostValue| {
        (0..levels).fold(HostValue::None, |v, _| wrap(v))
    };
    let wraps: [fn(HostValue) -> HostValue; 4] = [
        |v| HostValue::List(vec![v]),
        |v| HostValue::Tuple(vec![v]),
        |v| HostValue::Dict(vec![("k".into(), v)]),
        |v| HostValue::Record {
            name: "R".into(),
            fields: vec![("k".into(), v)],
        },
    ];
    // Encoding, decoding, comparing and dropping recurse: unoptimized,
    // 1000 levels need more than a test thread's stack.
    std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(move || {
            for wrap in wraps {
                let ok = nest(MAX_DEPTH + 1, wrap);
                assert_eq!(value_round_trip(&ok).unwrap(), ok);
                assert!(value_round_trip(&nest(MAX_DEPTH + 2, wrap)).is_err());
            }
        })
        .unwrap()
        .join()
        .unwrap();
}
