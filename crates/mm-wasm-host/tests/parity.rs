//! The mm-wasm surface against the native facade: scenarios that touch
//! structured host errors and exception classes, `dict_as_kwargs`, the
//! call log, hints, text cut, the zone source, `fuel_wanted`, environment
//! and stubs, the fuel total, the phase, `pending` and the typed errors run
//! on a native session and on the guest, and the transcripts must match.
//! Snapshots cross both ways, and a restore takes its host-side settings
//! from the options record as a native one takes `SessionSettings`.
//!
//! Needs a guest: `MM_WASM_GUEST`, else the root workspace's release
//! build for wasm32-unknown-unknown
//! (`cargo build --release -p mm-wasm-guest --target wasm32-unknown-unknown`).
#![allow(clippy::unwrap_used, clippy::expect_used, reason = "tests")]

use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use micro_monty::{
    DefineError, Engine, EngineConfig, HintGroup, HostError, HostErrorKind, HostException,
    HostModule, HostParam, HostSignature, HostValue, Limits, ParamKind, ParkReason, Phase,
    Progress, Resume, ResumeError, SessionSettings, SnapshotError, TextCut,
};
use mm_wasm_host::{Error, Runtime, RuntimeConfig, SessionOptions};

fn guest() -> PathBuf {
    std::env::var_os("MM_WASM_GUEST").map_or_else(
        || {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../target/wasm32-unknown-unknown/release/mm_wasm_guest.wasm")
        },
        PathBuf::from,
    )
}

fn module() -> HostModule {
    let param = |name: &str, annotation: &str| HostParam {
        name: name.into(),
        annotation: annotation.into(),
        default: None,
        kind: ParamKind::PositionalOrKeyword,
    };
    HostModule {
        name: "tools".into(),
        functions: vec![
            HostSignature {
                name: "search".into(),
                params: vec![param("query", "str"), param("limit", "int")],
                returns: "list".into(),
                doc: "Search.".into(),
                dict_as_kwargs: true,
            },
            HostSignature::any("fetch"),
        ],
        classes: Vec::new(),
        constants: vec![("VERSION".into(), HostValue::Int(3))],
        exceptions: vec![HostException {
            name: "ToolError".into(),
            base: HostErrorKind::RuntimeError,
            doc: "A tool failed.".into(),
        }],
    }
}

const CELL: &str = "\
import tools
from zoneinfo import ZoneInfo
from datetime import datetime
r = tools.search({'query': 'q', 'limit': 2})
try:
    tools.fetch('x')
except tools.ToolError as e:
    print('caught', type(e).__name__, e.tool, e.data, isinstance(e, RuntimeError))
paris = ZoneInfo('Europe/Paris')
print(datetime(2026, 7, 1, 12, tzinfo=paris).utcoffset())
x = 7 ** 100000
list(range(300))
";

/// Everything a run reports, in order.
fn transcript(steps: &mut Vec<String>, p: &Progress) -> Option<Resume> {
    steps.push(format!("{p:?}"));
    match p {
        Progress::HostCall { name, .. } if name == "tools.search" => {
            Some(Resume::Value(HostValue::List(vec![HostValue::Int(1)])))
        }
        Progress::HostCall { .. } => Some(Resume::Error(HostError {
            kind: HostErrorKind::Exception,
            message: "down".into(),
            data: Some(HostValue::Dict(vec![("code".into(), HostValue::Int(503))])),
            class: Some("tools.ToolError".into()),
        })),
        _ => None,
    }
}

fn zones() -> micro_monty::ZoneSource {
    Box::new(|key: &str| std::fs::read(format!("/usr/share/zoneinfo/{key}")).ok())
}

const LIMITS: Limits = Limits::new()
    .with_memory(64 << 20)
    .with_recursion_depth(200)
    .with_fuel(None);

#[test]
fn the_guest_matches_the_native_facade() {
    // Native.
    let engine = Engine::new(EngineConfig::new().with_limits(LIMITS));
    let native_out = Arc::new(Mutex::new(String::new()));
    let mut n = micro_monty::Session::new(&engine).unwrap();
    let sink = Arc::clone(&native_out);
    n.set_output(Box::new(move |t| sink.lock().unwrap().push_str(t)));
    n.set_zone_source(Some(zones()));
    n.set_hint(HintGroup::Network, Some("Use the http tool."))
        .unwrap();
    n.set_text_cut(TextCut::HeadTail);
    n.set_text_limit(40);
    n.define(&module()).unwrap();
    n.set_fuel(Some(5_000));
    let mut native = Vec::new();
    let mut p = n.start(&engine.compile(CELL, "cell.py").unwrap());
    let mut native_wanted = Vec::new();
    loop {
        if let Some(r) = transcript(&mut native, &p) {
            p = n.resume(r).unwrap();
        } else if let Progress::Parked {
            reason: ParkReason::Fuel,
        } = p
        {
            let wanted = n.fuel_wanted();
            native_wanted.push(wanted);
            n.set_fuel(Some(wanted.max(5_000)));
            p = n.resume(Resume::Continue).unwrap();
        } else {
            break;
        }
    }
    let native_log = n.call_log();
    let native_stubs = n.stubs();
    let native_env = n.environment();
    assert_eq!(native_env, engine.environment());
    let image = n.snapshot().unwrap();

    // The guest.
    let rt = Runtime::new(&guest(), RuntimeConfig::default()).unwrap();
    let mut options = SessionOptions::new().with_limits(LIMITS);
    options.text_limit = Some(40);
    let mut w = rt.session(&options).unwrap();
    let wasm_out = Arc::new(Mutex::new(String::new()));
    let sink = Arc::clone(&wasm_out);
    w.set_output(Box::new(move |_, t| sink.lock().unwrap().push_str(t)));
    w.set_zone_source(Some(zones()));
    w.set_hint(HintGroup::Network, Some("Use the http tool."))
        .unwrap();
    w.set_text_cut(TextCut::HeadTail).unwrap();
    w.define(&module()).unwrap();
    w.set_fuel(Some(5_000)).unwrap();
    let mut wasm = Vec::new();
    let mut p = w.start(CELL, "cell.py").unwrap();
    let mut wasm_wanted = Vec::new();
    loop {
        if let Some(r) = transcript(&mut wasm, &p) {
            p = w.resume(&r).unwrap();
        } else if let Progress::Parked {
            reason: ParkReason::Fuel,
        } = p
        {
            let wanted = w.fuel_wanted().unwrap();
            wasm_wanted.push(wanted);
            w.set_fuel(Some(wanted.max(5_000))).unwrap();
            p = w.resume(&Resume::Continue).unwrap();
        } else {
            break;
        }
    }

    assert_eq!(wasm, native);
    assert_eq!(*wasm_out.lock().unwrap(), *native_out.lock().unwrap());
    assert_eq!(wasm_wanted, native_wanted);
    assert_eq!(w.call_log().unwrap(), native_log);
    assert_eq!(w.stubs().unwrap(), native_stubs);
    assert_eq!(w.environment().unwrap(), native_env);
    // Absolute checks, so a shared mistake does not pass.
    let out = native_out.lock().unwrap().clone();
    assert!(
        out.contains("caught ToolError tools.fetch {'code': 503} True"),
        "{out}"
    );
    assert!(out.contains("2:00:00\n"), "{out}");
    assert!(
        native_wanted.iter().any(|w| *w > 5_000),
        "the big int parked before starting: {native_wanted:?}"
    );
    assert_eq!(native_log.len(), 2);
    assert!(
        native_stubs
            .iter()
            .any(|(m, t)| m == "tools" && t.contains("def search"))
    );
    let last = native.last().unwrap();
    assert!(
        last.contains("repr_truncated: true") && last.contains("bytes omitted"),
        "{last}"
    );

    // The hint reaches the uncaught exception's report, which the 40-byte
    // text limit cuts as it cuts a completion (HANDOFF decision 61): the
    // traceback keeps the hint's last bytes.
    let image_before = w.snapshot(None).unwrap();
    let hinted = n.start(&engine.compile("import requests", "h.py").unwrap());
    assert_eq!(
        format!("{:?}", w.start("import requests", "h.py").unwrap()),
        format!("{hinted:?}")
    );
    assert!(
        matches!(&hinted, Progress::Error(e) if e.truncated
            && e.message == "No module named 'requests'"
            && e.traceback.ends_with("ool.\n")),
        "{hinted:?}"
    );

    // The images cross both ways and resume the same, with the host-side
    // settings the restoring host gives: the options record's on the
    // guest, `SessionSettings` natively.
    let wasm_image = image_before;
    let mut head_tail = options;
    head_tail.text_cut = TextCut::HeadTail;
    let mut from_native = rt.restore(&image, &head_tail).unwrap();
    let mut settings = SessionSettings::default();
    settings.text_cut = TextCut::HeadTail;
    let mut from_wasm = engine.restore_with(&wasm_image, settings).unwrap();
    assert_eq!(from_native.call_log().unwrap(), from_wasm.call_log());
    let tail = "print(len(r), tools.VERSION, paris.key)\nr";
    let a = from_native.start(tail, "tail.py").unwrap();
    let b = from_wasm.start(&engine.compile(tail, "tail.py").unwrap());
    assert_eq!(format!("{a:?}"), format!("{b:?}"));
    let long = "list(range(300))";
    let a = from_native.start(long, "long.py").unwrap();
    let b = from_wasm.start(&engine.compile(long, "long.py").unwrap());
    assert_eq!(format!("{a:?}"), format!("{b:?}"));
    // Cut at both ends, as the image's 40-byte text limit and the
    // restoring host's text cut say.
    assert!(format!("{a:?}").contains("bytes omitted"), "{a:?}");
}

/// `host_modules`, `set_module_dunders`, `set_int_bits_text` and GC stress,
/// on a native session and on the guest, before and after a restore.
#[test]
fn host_modules_dunders_int_bits_and_gc_stress_match_the_native_facade() {
    const DUNDERS: &str = "\
import tools
def f(): pass
class C: pass
print(__name__, __file__, f.__module__, repr(C).split()[1])
";
    const BIG: &str = "[2 ** 20000, {'k': -(3 ** 20000)}]";
    // Garbage at every safepoint, with a cycle the collector must keep.
    const GARBAGE: &str = "\
a = []; a.append(a)
for i in range(200):
    b = [i, (i, str(i)), {'x': [i]}]
print(len(a[0][0]), b[2]['x'][0])
";
    let engine = Engine::new(EngineConfig::new().with_limits(LIMITS));
    let native_out = Arc::new(Mutex::new(String::new()));
    let mut n = micro_monty::Session::new(&engine).unwrap();
    let sink = Arc::clone(&native_out);
    n.set_output(Box::new(move |t| sink.lock().unwrap().push_str(t)));
    n.define(&module()).unwrap();
    n.define(&HostModule::untyped("", &["lookup"])).unwrap();
    n.set_module_dunders(Some("job"), Some("/work/job.py"))
        .unwrap();
    n.set_int_bits_text(true);
    n.set_gc_stress(true);
    let mut native = Vec::new();
    for src in [DUNDERS, BIG, GARBAGE] {
        native.push(format!(
            "{:?}",
            n.start(&engine.compile(src, "c.py").unwrap())
        ));
    }

    let rt = Runtime::new(&guest(), RuntimeConfig::default()).unwrap();
    let options = SessionOptions::new().with_limits(LIMITS);
    let mut w = rt.session(&options).unwrap();
    let wasm_out = Arc::new(Mutex::new(String::new()));
    let sink = Arc::clone(&wasm_out);
    w.set_output(Box::new(move |_, t| sink.lock().unwrap().push_str(t)));
    w.define(&module()).unwrap();
    w.define(&HostModule::untyped("", &["lookup"])).unwrap();
    w.set_module_dunders(Some("job"), Some("/work/job.py"))
        .unwrap();
    w.set_int_bits_text(true).unwrap();
    w.set_gc_stress(true).unwrap();
    let mut wasm = Vec::new();
    for src in [DUNDERS, BIG, GARBAGE] {
        wasm.push(format!("{:?}", w.start(src, "c.py").unwrap()));
    }

    assert_eq!(wasm, native);
    assert_eq!(*wasm_out.lock().unwrap(), *native_out.lock().unwrap());
    let modules = n.host_modules();
    assert_eq!(w.host_modules().unwrap(), modules);
    // Absolute checks, so a shared mistake does not pass.
    assert_eq!(
        modules.iter().map(|m| m.name.as_str()).collect::<Vec<_>>(),
        ["", "tools"]
    );
    assert_eq!(modules[1].exceptions, module().exceptions);
    assert!(modules[1].functions[0].dict_as_kwargs);
    let out = native_out.lock().unwrap().clone();
    assert!(out.starts_with("job /work/job.py job 'job.C'>\n"), "{out}");
    assert!(out.ends_with("1 199\n"), "{out}");
    assert!(native[1].contains("<int with 20001 bits>"), "{}", native[1]);

    // Definitions survive the images; the host-side settings are the
    // restoring host's, which here leave `int_bits_text` off.
    let wasm_image = w.snapshot(None).unwrap();
    let native_image = n.snapshot().unwrap();
    let mut from_wasm = engine.restore(&wasm_image).unwrap();
    let mut from_native = rt.restore(&native_image, &options).unwrap();
    assert_eq!(from_native.host_modules().unwrap(), modules);
    assert_eq!(from_wasm.host_modules(), modules);
    let a = from_native.start(BIG, "c.py").unwrap();
    let b = from_wasm.start(&engine.compile(BIG, "c.py").unwrap());
    assert_eq!(format!("{a:?}"), format!("{b:?}"));
    assert!(
        matches!(&a, Progress::Error(e) if e.type_name == "ValueError"),
        "{a:?}"
    );
    // Given at the restore: the options record on the guest,
    // `SessionSettings` natively.
    let mut on = options;
    on.int_bits_text = true;
    let mut settings = SessionSettings::default();
    settings.int_bits_text = true;
    let mut from_native = rt.restore(&native_image, &on).unwrap();
    let mut from_wasm = engine.restore_with(&wasm_image, settings).unwrap();
    let a = from_native.start(BIG, "c.py").unwrap();
    let b = from_wasm.start(&engine.compile(BIG, "c.py").unwrap());
    assert_eq!(format!("{a:?}"), format!("{b:?}"));
    assert_eq!(format!("{a:?}"), native[1]);
    // Or set afterwards, through the export.
    let mut from_native = rt.restore(&native_image, &options).unwrap();
    from_native.set_int_bits_text(true).unwrap();
    assert_eq!(
        format!("{:?}", from_native.start(BIG, "c.py").unwrap()),
        native[1]
    );
}

/// The fuel total and the environment that reports it, the phase, the
/// typed `DefineError`, and the details of `ResumeError` and
/// `SnapshotError`, natively and on the guest.
#[test]
fn fuel_total_phase_and_typed_errors_match_the_native_facade() {
    const LOOP: &str = "t = 0\nfor i in range(5000):\n    t += i\nt";
    let engine = Engine::new(EngineConfig::new().with_limits(LIMITS));
    let mut n = micro_monty::Session::new(&engine).unwrap();
    let rt = Runtime::new(&guest(), RuntimeConfig::default()).unwrap();
    let options = SessionOptions::new().with_limits(LIMITS);
    let mut w = rt.session(&options).unwrap();

    n.set_fuel_total(Some(3_000));
    w.set_fuel_total(Some(3_000)).unwrap();
    assert_eq!(w.environment().unwrap(), n.environment());
    assert!(
        n.environment()
            .contains("3000 fuel units left for the session")
    );
    assert_eq!(w.phase().unwrap(), Phase::Idle);
    let a = n.start(&engine.compile(LOOP, "l.py").unwrap());
    let b = w.start(LOOP, "l.py").unwrap();
    assert_eq!(a, b);
    assert_eq!(
        a,
        Progress::Parked {
            reason: ParkReason::Fuel
        }
    );
    assert_eq!(n.phase(), Phase::Parked);
    assert_eq!(w.phase().unwrap(), Phase::Parked);
    assert_eq!(n.fuel_total(), Some(0));
    assert_eq!(w.fuel_total().unwrap(), Some(0));
    let refused = Some(ResumeError::Unexpected {
        phase: Phase::Parked,
    });
    assert_eq!(n.resume(Resume::Future).err(), refused);
    assert_eq!(w.resume(&Resume::Future).err(), refused.map(Error::Resume));
    // The total travels in the image, both ways.
    let from_native = rt.restore(&n.snapshot().unwrap(), &options);
    assert_eq!(from_native.unwrap().fuel_total().unwrap(), Some(0));
    let from_wasm = engine.restore(&w.snapshot(None).unwrap()).unwrap();
    assert_eq!(from_wasm.fuel_total(), Some(0));
    assert!(n.cancel());
    assert!(w.cancel().unwrap());

    // A refused definition is the same typed error on both.
    for module in [
        HostModule::untyped("json", &["f"]),
        HostModule::untyped("a-b", &["f"]),
    ] {
        let a = n.define(&module).unwrap_err();
        assert!(matches!(a, DefineError::Invalid(_)), "{a:?}");
        assert_eq!(w.define(&module).unwrap_err(), Error::Define(a));
    }

    // Another format's image: the details survive the boundary.
    let mut image = w.snapshot(None).unwrap();
    image[8..10].copy_from_slice(&3u16.to_le_bytes());
    let expected = SnapshotError::Incompatible {
        format: 3,
        build_id: *micro_monty::build_id(),
    };
    assert_eq!(engine.restore(&image).err(), Some(expected.clone()));
    assert_eq!(
        rt.restore(&image, &options).err(),
        Some(Error::Snapshot(expected))
    );

    // A report cut at the text limit, and its flag, cross the boundary.
    const CUT: &str =
        "fs = 'f'\nfor _ in range(22):\n    fs = frozenset([fs, (fs, 1)])\n{0: 1}[fs]\n";
    let mut n = micro_monty::Session::new(&engine).unwrap();
    n.set_text_limit(4096);
    let mut wide = options;
    wide.text_limit = Some(4096);
    let mut w = rt.session(&wide).unwrap();
    let a = n.start(&engine.compile(CUT, "c.py").unwrap());
    assert_eq!(w.start(CUT, "c.py").unwrap(), a);
    assert!(
        matches!(&a, Progress::Error(e) if e.truncated && e.message.len() == 4096),
        "{a:?}"
    );
    let a = n.start(&engine.compile("1 / 0", "c.py").unwrap());
    assert_eq!(w.start("1 / 0", "c.py").unwrap(), a);
    assert!(matches!(&a, Progress::Error(e) if !e.truncated), "{a:?}");
}

/// `pending` re-reports the stop as the native facade does, on the session
/// that stopped and on one restored from either side's image.
/// `set_gc_yield` ends 10,000-unit fuel slices after collection steps, as
/// fuel parks with fuel left, natively and on the guest. Collections fall
/// due at different points on the two (32- and 64-bit sizes), so the
/// yields differ; the fuel charged and the result do not, with or without
/// it.
#[test]
fn gc_yield_parks_leave_fuel_totals_and_results_unchanged_on_both_targets() {
    const GARBAGE: &str = "\
keep = [[i] for i in range(50_000)]
t = 0
for i in range(100_000):
    b = [i, (i, str(i)), {'x': [i]}]
    t += len(b)
t + len(keep)
";
    const TOTAL: u64 = 1 << 40;
    let engine = Engine::new(EngineConfig::new().with_limits(LIMITS));
    let code = engine.compile(GARBAGE, "g.py").unwrap();
    let native = |on: bool| {
        let mut n = micro_monty::Session::new(&engine).unwrap();
        n.set_gc_yield(on);
        n.set_fuel_total(Some(TOTAL));
        n.set_fuel(Some(10_000));
        let (mut p, mut yields) = (n.start(&code), 0);
        while let Progress::Parked {
            reason: ParkReason::Fuel,
        } = p
        {
            yields += u32::from(n.fuel() != Some(0));
            n.set_fuel(Some(10_000));
            p = n.resume(Resume::Continue).unwrap();
        }
        (format!("{p:?}"), TOTAL - n.fuel_total().unwrap(), yields)
    };
    let rt = Runtime::new(&guest(), RuntimeConfig::default()).unwrap();
    let wasm = |on: bool| {
        let mut w = rt
            .session(&SessionOptions::new().with_limits(LIMITS))
            .unwrap();
        assert!(w.set_gc_yield(on).unwrap());
        w.set_fuel_total(Some(TOTAL)).unwrap();
        w.set_fuel(Some(10_000)).unwrap();
        let (mut p, mut yields) = (w.start(GARBAGE, "g.py").unwrap(), 0);
        while let Progress::Parked {
            reason: ParkReason::Fuel,
        } = p
        {
            yields += u32::from(w.fuel().unwrap() != Some(0));
            w.set_fuel(Some(10_000)).unwrap();
            p = w.resume(&Resume::Continue).unwrap();
        }
        (
            format!("{p:?}"),
            TOTAL - w.fuel_total().unwrap().unwrap(),
            yields,
        )
    };
    let runs = [native(false), native(true), wasm(false), wasm(true)];
    assert!(runs[0].0.contains("Int(350000)"), "{:?}", runs[0]);
    for run in &runs {
        assert_eq!((&run.0, run.1), (&runs[0].0, runs[0].1), "{runs:?}");
    }
    assert_eq!((runs[0].2, runs[2].2), (0, 0), "{runs:?}");
    assert!(runs[1].2 > 0 && runs[3].2 > 0, "{runs:?}");
}

#[test]
fn pending_matches_the_native_facade_across_restores() {
    const CELL: &str = "\
import asyncio
x = fetch(1, 'two', [2.5, None, (b'\\x00',)], k={'a': 10 ** 30})
async def main():
    return await asyncio.gather(slow(1), slow(2))
asyncio.run(main())
";
    let engine = Engine::new(EngineConfig::new().with_limits(LIMITS));
    // Pooled with two slots, so each restore reuses the slot the last freed.
    let config = RuntimeConfig {
        pool_instances: Some(2),
        ..RuntimeConfig::default()
    };
    let rt = Runtime::new(&guest(), config).unwrap();
    let options = SessionOptions::new().with_limits(LIMITS);
    let functions = HostModule::untyped("", &["fetch", "slow"]);
    let mut n = micro_monty::Session::new(&engine).unwrap();
    let mut w = rt.session(&options).unwrap();
    n.define(&functions).unwrap();
    w.define(&functions).unwrap();
    assert_eq!(w.pending().unwrap(), None);
    let mut a = n.start(&engine.compile(CELL, "p.py").unwrap());
    assert_eq!(w.start(CELL, "p.py").unwrap(), a);
    let mut stops = 0;
    while !matches!(a, Progress::Complete(_)) {
        let stop = n.pending();
        assert_eq!(stop.as_ref(), Some(&a));
        assert_eq!(w.pending().unwrap(), stop);
        let mut from_native = rt.restore(&n.snapshot().unwrap(), &options).unwrap();
        assert_eq!(from_native.pending().unwrap(), stop);
        let from_wasm = engine.restore(&w.snapshot(None).unwrap()).unwrap();
        assert_eq!(from_wasm.pending(), stop);
        stops += 1;
        let answer = match &a {
            Progress::HostCall { name, .. } if name == "fetch" => Resume::Value(HostValue::None),
            Progress::HostCall { .. } => Resume::Future,
            Progress::Waiting { calls } => {
                Resume::Batch(vec![(calls[0], Ok(HostValue::Int(calls[0].cast_signed())))])
            }
            _ => break,
        };
        a = n.resume(answer.clone()).unwrap();
        assert_eq!(w.resume(&answer).unwrap(), a);
    }
    assert!(matches!(a, Progress::Complete(_)), "{a:?}");
    assert_eq!((n.pending(), w.pending().unwrap()), (None, None));
    // fetch, two slow calls, two waits.
    assert_eq!(stops, 5);
}

/// A park the host requests is `Requested` wherever the stop is read: the
/// progress, `pending`, and its image restored on either side. Reporting
/// it takes the request, so the cell then runs to the end.
#[test]
fn a_requested_park_stays_requested_in_pending_and_in_its_image() {
    let engine = Engine::new(EngineConfig::new().with_limits(LIMITS));
    let rt = Runtime::new(&guest(), RuntimeConfig::default()).unwrap();
    let options = SessionOptions::new().with_limits(LIMITS);
    let mut w = rt.session(&options).unwrap();
    let parked = Progress::Parked {
        reason: ParkReason::Requested,
    };
    w.park_handle().request();
    let p = w.start("for i in range(10**5): pass\n7", "<p>").unwrap();
    assert_eq!(p, parked);
    assert_eq!(w.pending().unwrap().as_ref(), Some(&parked));
    let image = w.snapshot(None).unwrap();
    assert_eq!(engine.restore(&image).unwrap().pending().as_ref(), Some(&parked));
    let mut r = rt.restore(&image, &options).unwrap();
    assert_eq!(r.pending().unwrap().as_ref(), Some(&parked));
    let p = w.resume(&Resume::Continue).unwrap();
    assert!(matches!(p, Progress::Complete(Some(ref v)) if v.repr == "7"), "{p:?}");
}

/// A park request no running cell consumed (the cell completed, or waits at
/// a host call) must not cost the instance once the grace period passes:
/// the grace counts from the start of the call that should park.
#[test]
fn a_park_request_between_calls_parks_the_next_call_instead_of_trapping() {
    let rt = Runtime::new(&guest(), RuntimeConfig::default()).expect("guest");
    let mut s = rt.session(&SessionOptions::default()).expect("session");
    let p = s.start("1", "<a>").expect("starts");
    assert!(matches!(p, Progress::Complete(_)), "{p:?}");
    s.park_handle().request();
    std::thread::sleep(std::time::Duration::from_millis(250));
    assert_eq!(s.fuel_wanted().expect("no trap"), 0);
    s.define(&HostModule {
        name: "t".into(),
        functions: vec![HostSignature::any("f")],
        classes: Vec::new(),
        constants: Vec::new(),
        exceptions: Vec::new(),
    })
    .expect("defines");
    let p = s
        .start("import t\nt.f()\nfor i in range(10**6): pass\n2", "<b>")
        .expect("starts");
    // The stale request parks the cell at its first safepoint, or the cell
    // reaches the host call; either way, no trap.
    let p = match p {
        Progress::Parked { .. } => s.resume(&Resume::Continue).expect("resumes"),
        p => p,
    };
    assert!(matches!(p, Progress::HostCall { .. }), "{p:?}");
    // A slow tool; meanwhile the host asks the cell to park.
    s.park_handle().request();
    std::thread::sleep(std::time::Duration::from_millis(250));
    let p = s.resume(&Resume::Value(HostValue::None)).expect("no trap");
    assert!(
        matches!(
            p,
            Progress::Parked {
                reason: ParkReason::Requested
            }
        ),
        "{p:?}"
    );
}

/// A collection cannot park, and over a nearly full default heap (2M
/// `[i, str(i)]`, about 200 MiB live) it takes 85–120 ms on the guest [M]:
/// longer than a fixed 100 ms grace, which trapped this run 3 times in 3.
/// The grace grows with the memory limit, so park requests landing in
/// those collections wait for them instead of losing the instance.
#[test]
fn a_park_request_during_a_long_collection_does_not_trap() {
    let rt = Runtime::new(&guest(), RuntimeConfig::default()).expect("guest");
    let mut s = rt.session(&SessionOptions::default()).expect("session");
    let fill = "big = [[i, str(i)] for i in range(2_000_000)]";
    assert!(matches!(s.start(fill, "<f>"), Ok(Progress::Complete(_))));
    let handle = s.park_handle();
    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let asking = Arc::clone(&stop);
    let asker = std::thread::spawn(move || {
        while !asking.load(std::sync::atomic::Ordering::Relaxed) {
            std::thread::sleep(std::time::Duration::from_millis(3));
            handle.request();
        }
    });
    let mut parks = 0;
    let mut p = s.start("for i in range(1_500_000):\n    y = [i]", "<l>");
    while let Ok(Progress::Parked { .. }) = p {
        parks += 1;
        p = s.resume(&Resume::Continue);
    }
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    asker.join().unwrap();
    assert!(matches!(p, Ok(Progress::Complete(_))), "{p:?}");
    assert!(parks > 0);
}

/// A snapshot cannot park: it collects in full first, about 1.4 ms per MiB
/// of image [M], past the default grace for an image over about 210 MiB. A
/// park request pending during it (the cell had parked for fuel when a
/// supervisor asked it to stop) waits for the next call instead of costing
/// the instance and its image. With no grace, this trapped every time.
#[test]
fn a_park_request_pending_during_a_snapshot_does_not_trap() {
    let config = RuntimeConfig {
        park_grace: std::time::Duration::ZERO,
        park_grace_per_mib: std::time::Duration::ZERO,
        ..RuntimeConfig::default()
    };
    let rt = Runtime::new(&guest(), config).expect("guest");
    let mut s = rt.session(&SessionOptions::default()).expect("session");
    let fill = "big = [[i, i + 1] for i in range(600_000)]";
    assert!(matches!(s.start(fill, "<f>"), Ok(Progress::Complete(_))));
    s.park_handle().request();
    let image = s.snapshot(None).expect("no trap");
    let mut r = rt
        .restore(&image, &SessionOptions::default())
        .expect("restores");
    let p = r.start("len(big)", "<n>").expect("starts");
    assert!(
        matches!(p, Progress::Complete(Some(ref v)) if v.repr == "600000"),
        "{p:?}"
    );
}

/// The precompiled cache is keyed on the module's content, not its mtime: a
/// guest replaced by different bytes with an older mtime (`cp -p`) is
/// compiled anew instead of loading the stale native code.
#[test]
fn the_precompiled_cache_follows_the_module_content_not_its_mtime() {
    let dir = std::env::temp_dir().join(format!("mmw-cache-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let wasm = dir.join("guest.wasm");
    std::fs::copy(guest(), &wasm).unwrap();
    let caches = || {
        let mut v: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .filter(|n| n.ends_with(".cwasm"))
            .collect();
        v.sort();
        v
    };
    let config = RuntimeConfig {
        precompiled: true,
        ..RuntimeConfig::default()
    };
    drop(Runtime::new(&wasm, config.clone()).expect("guest"));
    let first = caches();
    assert_eq!(first.len(), 1, "{first:?}");
    // Still valid: a custom section ("mmxx", empty payload) at the end.
    let mut bytes = std::fs::read(&wasm).unwrap();
    bytes.extend_from_slice(&[0, 5, 4, b'm', b'm', b'x', b'x']);
    std::fs::write(&wasm, &bytes).unwrap();
    let cache_time = std::fs::metadata(dir.join(&first[0]))
        .unwrap()
        .modified()
        .unwrap();
    let older = cache_time - std::time::Duration::from_secs(3600);
    std::fs::File::options()
        .write(true)
        .open(&wasm)
        .unwrap()
        .set_modified(older)
        .unwrap();
    let rt = Runtime::new(&wasm, config).expect("replaced guest");
    let mut s = rt.session(&SessionOptions::default()).expect("session");
    let p = s.start("1", "<a>").expect("starts");
    assert!(matches!(p, Progress::Complete(_)), "{p:?}");
    let second = caches();
    std::fs::remove_dir_all(&dir).unwrap();
    assert_eq!(
        second.len(),
        2,
        "a new cache for the new content: {second:?}"
    );
}
