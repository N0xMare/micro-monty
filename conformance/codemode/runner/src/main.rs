//! `mm-codemode`: runs one codemode solution on micro-monty for
//! `conformance/codemode/run.py`, which owns the mocked tools.
//!
//! The protocol is one JSON object per line on stdin and stdout:
//!
//! 1. The harness sends `{"source", "tools", "fuel", "clock"}`.
//! 2. The runner defines module `_ptc` with one untyped host function per
//!    tool and runs a bootstrap cell that binds each tool as a global
//!    `async def name(arguments): return await _ptc.name(arguments)`, the
//!    programmatic-tool-calling convention (and plak's): calling a tool
//!    makes an inert coroutine.
//! 3. It runs the solution. Each host call is sent as `{"call", "name",
//!    "args", "kwargs", "batch"}` (`batch` counts the `Waiting`s before
//!    it, so the calls one `Waiting` collects share it) and answered with `{"ok": value}` or `{"err":
//!    [type, message]}`; the runner resumes it with `Resume::Future` and
//!    delivers the answers when the cell waits (`Resume::Batch`), so
//!    `asyncio.gather` over tools yields one batch.
//! 4. It ends with `{"done": {"stdout", "stderr", "error", "calls",
//!    "max_waiting", "timing"}}` (`timing` for `timing.py`).
//!
//! Values use plain JSON for `None`, `bool`, `str`, `list` and `i64`; the
//! rest is tagged: `{"$f": repr}` floats, `{"$i": digits}` big ints,
//! `{"$t": [...]}` tuples, `{"$d": [[key, value], ...]}` dicts (order
//! kept), `{"$b": hex}` bytes, `{"$r": name, "fields": [...]}` records and
//! host objects, `{"$u": text}` unconverted pieces.
//!
//! With `MM_CODEMODE_TEMPLATE` naming a snapshot image (a session template
//! of this build, docs/user-guide.md "Session templates"), setup restores it
//! instead of `Session::new`, then sets the fuel budget and the host-side
//! settings again. The template must not have imported `random`, whose
//! state every instance would share.
//! `MM_CODEMODE_WARM` runs one discarded setup before the timed one.

use micro_monty::{
    Engine, EngineConfig, HostError, HostErrorKind, HostModule, HostValue, Limits, Progress,
    Resume, Session, Stream,
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fmt::Write as _,
    io::{BufRead, Write},
    process::ExitCode,
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicU64, Ordering},
    },
    time::Instant,
};

/// Nanoseconds spent in setup (session, tools, bootstrap cell), in compiling
/// the solution, and in `Session::start`/`resume` (the engine running the
/// cell, host-value conversion included), for `timing.py`.
static SETUP_NS: AtomicU64 = AtomicU64::new(0);
static COMPILE_NS: AtomicU64 = AtomicU64::new(0);
static ENGINE_NS: AtomicU64 = AtomicU64::new(0);
/// The part of `ENGINE_NS` spent in `Session::start` (up to the first host
/// call or the end).
static START_NS: AtomicU64 = AtomicU64::new(0);

/// Runs `f`, adding its duration to each of `counters`.
fn timed<R>(counters: &[&AtomicU64], f: impl FnOnce() -> R) -> R {
    let t = Instant::now();
    let r = f();
    let ns = u64::try_from(t.elapsed().as_nanos()).unwrap_or(u64::MAX);
    for c in counters {
        c.fetch_add(ns, Ordering::Relaxed);
    }
    r
}

/// Output kept per stream; the rest is dropped and marked.
const OUTPUT_CAP: usize = 1 << 20;

/// Stdout and stderr as the cell wrote them.
type Output = Arc<Mutex<(String, String)>>;

type Lines<'a> = std::io::Lines<std::io::StdinLock<'a>>;

fn main() -> ExitCode {
    let stdin = std::io::stdin();
    let mut lines = stdin.lock().lines();
    let mut out = std::io::stdout().lock();
    let Some(init) = lines
        .next()
        .and_then(Result::ok)
        .and_then(|line| serde_json::from_str::<Value>(&line).ok())
    else {
        eprintln!("mm-codemode: expected the JSON init line on stdin");
        return ExitCode::from(2);
    };
    let output = Arc::new(Mutex::new((String::new(), String::new())));
    let Some((error, calls, max_waiting)) = run(&init, &output, &mut lines, &mut out) else {
        eprintln!("mm-codemode: the harness stopped answering");
        return ExitCode::from(2);
    };
    let streams = output.lock().unwrap_or_else(PoisonError::into_inner);
    let done = json!({"done": {
        "stdout": streams.0, "stderr": streams.1, "error": error, "calls": calls,
        "max_waiting": max_waiting,
        "timing": {
            "setup_ns": SETUP_NS.load(Ordering::Relaxed),
            "compile_ns": COMPILE_NS.load(Ordering::Relaxed),
            "engine_ns": ENGINE_NS.load(Ordering::Relaxed),
            "start_ns": START_NS.load(Ordering::Relaxed),
        },
    }});
    let _ = writeln!(out, "{done}");
    let _ = out.flush();
    ExitCode::SUCCESS
}

/// The session with the tools bound, or the error that stopped setup.
fn setup(
    engine: &Engine,
    init: &Value,
    output: &Output,
    template: Option<&[u8]>,
    fuel: u64,
) -> Result<Session, Value> {
    let tools: Vec<&str> = init["tools"]
        .as_array()
        .map(|t| t.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    let clock = init["clock"].as_f64().unwrap_or(0.0);
    let mut session = match template {
        None => {
            Session::new(engine).map_err(|e| error_json(&e.type_name, &e.message, &e.traceback))?
        }
        Some(image) => {
            let mut s = engine
                .restore(image)
                .map_err(|e| error_json("RunnerError", &format!("template: {e}"), ""))?;
            // The image keeps the fuel left when it was taken.
            s.set_fuel(Some(fuel));
            s
        }
    };
    let sink = Arc::clone(output);
    session.set_output_streams(Box::new(move |stream, text| {
        let mut streams = sink.lock().unwrap_or_else(PoisonError::into_inner);
        let target = if stream == Stream::Stderr {
            &mut streams.1
        } else {
            &mut streams.0
        };
        if target.len() < OUTPUT_CAP {
            target.push_str(text);
        }
    }));
    session.set_wall_clock(Some(Box::new(move || clock)));
    let mut counter = 0u8;
    session.set_entropy(Some(Box::new(move |buf: &mut [u8]| {
        for b in buf {
            counter = counter.wrapping_add(37);
            *b = counter;
        }
    })));
    // Z1: the host serves zones from the system's tz database, as plak would.
    session.set_zone_source(Some(Box::new(|key: &str| {
        if key.starts_with('/') || key.split('/').any(|p| p == ".." || p.is_empty()) {
            return None;
        }
        std::fs::read(std::path::Path::new("/usr/share/zoneinfo").join(key))
            .ok()
            .filter(|b| b.starts_with(b"TZif"))
    })));
    session.set_last_expr_value(false);
    session
        .define(&HostModule::untyped("_ptc", &tools))
        .map_err(|e| error_json("DefineError", &e.to_string(), &format!("{e}\n")))?;
    let mut bootstrap = String::from("import _ptc\n");
    for tool in &tools {
        let _ = write!(
            bootstrap,
            "async def {tool}(arguments):\n    return await _ptc.{tool}(arguments)\n"
        );
    }
    let code = engine
        .compile(&bootstrap, "<tools>")
        .map_err(|e| error_json(&e.type_name, &e.message, &e.rendered))?;
    if let Progress::Error(e) = session.start(&code) {
        return Err(error_json(&e.type_name, &e.message, &e.traceback));
    }
    Ok(session)
}

/// Runs the solution: its error (or null), the number of host calls and the
/// most host futures one wait stopped for; `None` if the harness stopped
/// answering.
fn run(
    init: &Value,
    output: &Output,
    lines: &mut Lines<'_>,
    out: &mut dyn Write,
) -> Option<(Value, u64, usize)> {
    let fuel = init["fuel"].as_u64().unwrap_or(1_000_000_000);
    let engine = Engine::new(EngineConfig::new().with_limits(Limits::new().with_fuel(Some(fuel))));
    let template = std::env::var_os("MM_CODEMODE_TEMPLATE").and_then(|p| std::fs::read(p).ok());
    // `MM_CODEMODE_WARM`: one discarded setup first, so the timed one runs
    // as in a long-lived host rather than in a new process.
    if std::env::var_os("MM_CODEMODE_WARM").is_some() {
        drop(setup(&engine, init, output, template.as_deref(), fuel));
    }
    let mut session = match timed(&[&SETUP_NS], || {
        setup(&engine, init, output, template.as_deref(), fuel)
    }) {
        Ok(session) => session,
        Err(error) => return Some((error, 0, 0)),
    };
    let source = init["source"].as_str().unwrap_or_default();
    let code = match timed(&[&COMPILE_NS], || engine.compile(source, "solution.py")) {
        Ok(code) => code,
        Err(e) => return Some((error_json(&e.type_name, &e.message, &e.rendered), 0, 0)),
    };
    let mut max_waiting = 0;
    let mut answers: BTreeMap<u64, Result<HostValue, HostError>> = BTreeMap::new();
    let mut calls = 0;
    let mut batch = 0u64;
    let mut progress = timed(&[&ENGINE_NS, &START_NS], || session.start(&code));
    loop {
        let resume = match progress {
            Progress::HostCall {
                call,
                name,
                args,
                kwargs,
                ..
            } => {
                calls += 1;
                let name = name.strip_prefix("_ptc.").unwrap_or(&name);
                let request = json!({
                    "call": call,
                    "name": name,
                    "args": args.iter().map(encode).collect::<Vec<_>>(),
                    "kwargs": pairs(&kwargs),
                    "batch": batch,
                });
                let _ = writeln!(out, "{request}");
                let _ = out.flush();
                let reply = lines
                    .next()?
                    .ok()
                    .and_then(|line| serde_json::from_str::<Value>(&line).ok())?;
                answers.insert(call, answer(&reply));
                Resume::Future
            }
            Progress::Waiting { calls: waiting } => {
                max_waiting = max_waiting.max(waiting.len());
                batch += 1;
                let batch: Vec<_> = waiting
                    .iter()
                    .filter_map(|c| answers.remove(c).map(|r| (*c, r)))
                    .collect();
                if batch.is_empty() {
                    let message = format!("waiting on unanswered calls {waiting:?}");
                    return Some((error_json("RunnerError", &message, ""), calls, max_waiting));
                }
                Resume::Batch(batch)
            }
            Progress::Parked { reason } => {
                let message = format!("parked: {reason:?} (fuel budget {fuel})");
                return Some((
                    error_json("FuelExhausted", &message, ""),
                    calls,
                    max_waiting,
                ));
            }
            Progress::Complete(_) => return Some((Value::Null, calls, max_waiting)),
            Progress::Error(e) => {
                return Some((
                    error_json(&e.type_name, &e.message, &e.traceback),
                    calls,
                    max_waiting,
                ));
            }
            other => {
                let message = format!("unexpected progress {other:?}");
                return Some((error_json("RunnerError", &message, ""), calls, max_waiting));
            }
        };
        progress = match timed(&[&ENGINE_NS], || session.resume(resume)) {
            Ok(p) => p,
            Err(e) => {
                return Some((
                    error_json("RunnerError", &e.to_string(), ""),
                    calls,
                    max_waiting,
                ));
            }
        };
    }
}

fn error_json(type_name: &str, message: &str, traceback: &str) -> Value {
    json!({"type": type_name, "message": message, "traceback": traceback})
}

fn pairs(items: &[(String, HostValue)]) -> Vec<Value> {
    items.iter().map(|(k, v)| json!([k, encode(v)])).collect()
}

/// A guest value in the tagged JSON form.
fn encode(value: &HostValue) -> Value {
    match value {
        HostValue::None => Value::Null,
        HostValue::Bool(b) => json!(b),
        HostValue::Int(i) => json!(i),
        HostValue::BigInt(digits) => json!({"$i": digits}),
        HostValue::Float(f) => json!({"$f": format!("{f:?}")}),
        HostValue::Str(s) => json!(s),
        HostValue::Bytes(b) => json!({"$b": b.iter().fold(String::new(), |mut hex, x| {
            let _ = write!(hex, "{x:02x}");
            hex
        })}),
        HostValue::List(items) => Value::Array(items.iter().map(encode).collect()),
        HostValue::Tuple(items) => json!({"$t": items.iter().map(encode).collect::<Vec<_>>()}),
        HostValue::Dict(items) => json!({"$d": pairs(items)}),
        HostValue::Record { name, fields } => json!({"$r": name, "fields": pairs(fields)}),
        HostValue::Object { class, fields } => json!({"$r": class, "fields": pairs(fields)}),
        other => json!({"$u": format!("{other:?}")}),
    }
}

/// A harness value from the tagged JSON form.
fn decode(value: &Value) -> HostValue {
    match value {
        Value::Null => HostValue::None,
        Value::Bool(b) => HostValue::Bool(*b),
        Value::Number(n) => n.as_i64().map_or_else(
            || HostValue::Float(n.as_f64().unwrap_or(f64::NAN)),
            HostValue::Int,
        ),
        Value::String(s) => HostValue::Str(s.clone()),
        Value::Array(items) => HostValue::List(items.iter().map(decode).collect()),
        Value::Object(map) => {
            if let Some(Value::String(f)) = map.get("$f") {
                HostValue::Float(f.parse().unwrap_or(f64::NAN))
            } else if let Some(Value::String(digits)) = map.get("$i") {
                HostValue::BigInt(digits.clone())
            } else if let Some(Value::Array(items)) = map.get("$t") {
                HostValue::Tuple(items.iter().map(decode).collect())
            } else if let Some(Value::String(hex)) = map.get("$b") {
                HostValue::Bytes(
                    (0..hex.len() / 2)
                        .filter_map(|i| u8::from_str_radix(hex.get(2 * i..2 * i + 2)?, 16).ok())
                        .collect(),
                )
            } else if let Some(Value::Array(items)) = map.get("$d") {
                HostValue::Dict(
                    items
                        .iter()
                        .filter_map(|pair| {
                            let key = pair.get(0)?.as_str()?.to_owned();
                            Some((key, decode(pair.get(1)?)))
                        })
                        .collect(),
                )
            } else {
                HostValue::None
            }
        }
    }
}

/// The harness's answer to one call.
fn answer(reply: &Value) -> Result<HostValue, HostError> {
    if let Some(value) = reply.get("ok") {
        return Ok(decode(value));
    }
    let kind = reply["err"][0].as_str().unwrap_or("RuntimeError");
    Err(HostError {
        kind: HostErrorKind::from_name(kind).unwrap_or(HostErrorKind::RuntimeError),
        message: reply["err"][1].as_str().unwrap_or_default().to_owned(),
        data: None,
        class: None,
    })
}
