//! micro-monty's shipped mm-wasm guest under wasmtime (crates/mm-wasm-host,
//! its default `RuntimeConfig`: epoch interruption, 100 ms park grace,
//! precompiled module cache), under the shared engine harness. Everything
//! else is the native host's (hosts/mm).
//!
//! Codemode setup: one typed host module, the builtins module, with each
//! tool as a typed function whose parameters are keyword arguments with
//! `None` defaults and `dict_as_kwargs`, so `await tool({...})` and
//! `await tool(k=v)` both work; plus the untyped sync `hsync`. A tool call
//! answers `Resume::Future`, and the answers go back in one `Resume::Batch`
//! when the cell waits (as plak and the codemode runner do).

use common::{
    Answer, Control, Engine, Host, Lang, RunResult, Snap, Stop, ToolDef, Value, json, timed,
};
use micro_monty::{
    HostError, HostErrorKind, HostModule, HostParam, HostSignature, HostValue, Limits, ParamKind,
    ParkReason, Progress, Resume,
};
use mm_wasm_host::{Runtime, RuntimeConfig, Session, SessionOptions};
use std::sync::{Arc, Mutex};
use std::time::Duration;

struct Mm {
    rt: Runtime,
    template: Option<Vec<u8>>,
    /// Fuel slice for `run_polled`, else unlimited.
    slice: Option<u64>,
    /// Typed tools' parameter names (the last setup), to rebuild the
    /// argument dict.
    params: Vec<(String, Vec<String>)>,
}

struct S {
    session: Session,
    out: Arc<Mutex<String>>,
    /// Open calls: id, name, arguments, answer.
    pending: Vec<(u64, String, Value, Result<HostValue, HostError>)>,
}

fn to_host(v: &Value) -> HostValue {
    match v {
        Value::Null => HostValue::None,
        Value::Bool(b) => HostValue::Bool(*b),
        Value::Number(n) => match n.as_i64() {
            Some(i) => HostValue::Int(i),
            None => HostValue::Float(n.as_f64().unwrap_or(f64::NAN)),
        },
        Value::String(s) => HostValue::Str(s.clone()),
        Value::Array(a) => HostValue::List(a.iter().map(to_host).collect()),
        Value::Object(m) => {
            HostValue::Dict(m.iter().map(|(k, v)| (k.clone(), to_host(v))).collect())
        }
    }
}

fn to_json(v: &HostValue) -> Value {
    match v {
        HostValue::None => Value::Null,
        HostValue::Bool(b) => json!(b),
        HostValue::Int(i) => json!(i),
        HostValue::Float(f) => json!(f),
        HostValue::Str(s) => json!(s),
        HostValue::List(a) | HostValue::Tuple(a) => Value::Array(a.iter().map(to_json).collect()),
        HostValue::Dict(d) => {
            Value::Object(d.iter().map(|(k, v)| (k.clone(), to_json(v))).collect())
        }
        other => json!(format!("{other:?}")),
    }
}

fn answer(a: Answer<'_>) -> Result<HostValue, HostError> {
    match a {
        Answer::Ok(v) => Ok(to_host(v)),
        Answer::Err(kind, msg) => Err(HostError::new(
            HostErrorKind::from_name(kind).unwrap_or(HostErrorKind::RuntimeError),
            msg,
        )),
    }
}

fn options() -> SessionOptions {
    SessionOptions::new().with_limits(Limits::new().with_fuel(None))
}

fn guest() -> std::path::PathBuf {
    std::env::var_os("MM_WASM_GUEST").map_or_else(|| common::fail("set MM_WASM_GUEST"), Into::into)
}

impl Mm {
    fn session(&self) -> S {
        let mut session = self
            .rt
            .session(&options())
            .unwrap_or_else(|e| common::fail(&e.to_string()));
        let out = Arc::new(Mutex::new(String::new()));
        attach(&mut session, &out);
        S {
            session,
            out,
            pending: Vec::new(),
        }
    }

    /// Drives `progress` to the end, or until `stop(answered)` holds at a wait.
    fn drive(
        &self,
        s: &mut S,
        mut progress: Progress,
        host: &mut dyn Host,
        stop_after: Option<usize>,
    ) -> Result<Option<()>, String> {
        let mut answered = 0usize;
        loop {
            progress = match progress {
                Progress::HostCall {
                    call,
                    name,
                    args,
                    kwargs,
                    ..
                } => {
                    if name == "hsync" {
                        common::mark();
                        let v = args.into_iter().next().unwrap_or(HostValue::None);
                        resume(&mut s.session, Resume::Value(v))?
                    } else {
                        let args = tool_args(&self.params, &name, &args, &kwargs);
                        let a = answer(host.call(&name, args.clone()));
                        s.pending.push((call, name, args, a));
                        resume(&mut s.session, Resume::Future)?
                    }
                }
                Progress::Waiting { calls } => {
                    if stop_after.is_some_and(|n| answered >= n) {
                        return Ok(Some(()));
                    }
                    let mut batch = Vec::new();
                    s.pending.retain(|(id, _, _, a)| {
                        if calls.contains(id) {
                            batch.push((*id, a.clone()));
                            false
                        } else {
                            true
                        }
                    });
                    if batch.is_empty() {
                        return Err(format!("waiting on unanswered calls {calls:?}"));
                    }
                    answered += batch.len();
                    resume(&mut s.session, Resume::Batch(batch))?
                }
                Progress::Parked {
                    reason: ParkReason::Fuel,
                } => {
                    s.session.set_fuel(self.slice).map_err(|e| e.to_string())?;
                    resume(&mut s.session, Resume::Continue)?
                }
                Progress::Parked { reason } => return Err(format!("parked: {reason:?}")),
                Progress::Complete(_) => return Ok(None),
                Progress::Error(e) => return Err(e.traceback),
                _ => return Err("unexpected progress".into()),
            }
        }
    }

    fn start(&self, s: &mut S, src: &str) -> Result<Progress, String> {
        if let Ok(mut o) = s.out.lock() {
            o.clear();
        }
        s.session.set_fuel(self.slice).map_err(|e| e.to_string())?;
        s.session.start(src, "cell.py").map_err(|e| e.to_string())
    }

    fn take_output(s: &S) -> String {
        s.out.lock().map(|o| o.clone()).unwrap_or_default()
    }
}

fn attach(session: &mut Session, out: &Arc<Mutex<String>>) {
    let sink = Arc::clone(out);
    session.set_output(Box::new(move |_, t| {
        if let Ok(mut o) = sink.lock() {
            o.push_str(t);
        }
    }));
}

fn resume(session: &mut Session, r: Resume) -> Result<Progress, String> {
    session.resume(&r).map_err(|e| e.to_string())
}

/// The argument dict as the tool receives it: typed parameters arrive
/// positionally with defaults filled, so `None` ones are dropped.
fn tool_args(
    params: &[(String, Vec<String>)],
    name: &str,
    args: &[HostValue],
    kwargs: &[(String, HostValue)],
) -> Value {
    let mut m = common::serde_json::Map::new();
    if let Some((_, names)) = params.iter().find(|(n, _)| n == name) {
        for (p, v) in names.iter().zip(args) {
            if *v != HostValue::None {
                m.insert(p.clone(), to_json(v));
            }
        }
    } else {
        // Untyped: one dict.
        if let Some(Value::Object(d)) = args.first().map(to_json) {
            m = d;
        }
        for (k, v) in kwargs {
            m.insert(k.clone(), to_json(v));
        }
    }
    Value::Object(m)
}

fn module(tools: &[ToolDef]) -> HostModule {
    let mut functions: Vec<HostSignature> = tools
        .iter()
        .map(|t| HostSignature {
            name: t.name.clone(),
            params: t
                .params
                .iter()
                .map(|p| HostParam {
                    name: p.clone(),
                    annotation: String::new(),
                    default: Some(HostValue::None),
                    kind: ParamKind::KeywordOnly,
                })
                .collect(),
            returns: String::new(),
            doc: t.description.clone(),
            dict_as_kwargs: true,
        })
        .collect();
    functions.push(HostSignature::any("hsync"));
    HostModule {
        name: String::new(),
        functions,
        ..HostModule::default()
    }
}

impl Engine for Mm {
    type S = S;
    const NAME: &'static str = "mm-wasm";
    const LANG: Lang = Lang::Py;

    fn new_runtime() -> Self {
        let rt = Runtime::new(&guest(), RuntimeConfig::default())
            .unwrap_or_else(|e| common::fail(&e.to_string()));
        Mm {
            rt,
            template: None,
            slice: None,
            params: Vec::new(),
        }
    }

    fn new_session(&mut self) -> S {
        self.session()
    }

    fn setup(&mut self, s: &mut S, tools: &[ToolDef]) {
        s.session
            .define(&module(tools))
            .unwrap_or_else(|e| common::fail(&e.to_string()));
        self.params = tools
            .iter()
            .map(|t| (t.name.clone(), t.params.clone()))
            .collect();
    }

    fn prepare_image(&mut self, tools: &[ToolDef]) -> bool {
        let mut s = self.session();
        self.setup(&mut s, tools);
        // The template carries what a codemode cell imports first.
        let _ = s
            .session
            .start("import asyncio, json\ndel asyncio, json\n", "setup.py");
        self.template = Some(
            s.session
                .snapshot(None)
                .unwrap_or_else(|e| common::fail(&e.to_string())),
        );
        true
    }

    fn from_image(&mut self) -> Option<S> {
        let mut session = self.rt.restore(self.template.as_ref()?, &options()).ok()?;
        let out = Arc::new(Mutex::new(String::new()));
        attach(&mut session, &out);
        Some(S {
            session,
            out,
            pending: Vec::new(),
        })
    }

    fn run(&mut self, s: &mut S, src: &str, host: &mut dyn Host) -> RunResult {
        let p = self.start(s, src)?;
        self.drive(s, p, host, None)?;
        Ok(Mm::take_output(s))
    }

    fn run_plain(&mut self, src: &str) -> RunResult {
        let mut s = self.session();
        self.run(&mut s, src, &mut common::Noop::default())
    }

    fn run_polled(&mut self, src: &str) -> RunResult {
        self.slice = Some(micro_monty::ASYNC_SLICE_FUEL);
        let r = self.run_plain(src);
        self.slice = None;
        r
    }

    fn control(&mut self, s: &mut S, src: &str, stop: &Stop, delay: Duration) -> Control {
        let park = s.session.park_handle();
        stop.request_after(delay, move || park.request());
        let mut p = match self.start(s, src) {
            Ok(p) => p,
            Err(e) => {
                let back_ns = common::now_ns();
                let dead = s.session.is_dead();
                return Control {
                    how: format!("trap: {}", e.chars().take(60).collect::<String>()),
                    resumable: false,
                    session_usable: !dead,
                    back_ns,
                };
            }
        };
        while let Progress::HostCall { ref name, .. } = p {
            if name == "hsync" {
                common::mark();
            }
            p = match resume(&mut s.session, Resume::Value(HostValue::Int(0))) {
                Ok(p) => p,
                Err(e) => {
                    let back_ns = common::now_ns();
                    return Control {
                        how: format!("trap: {}", e.chars().take(60).collect::<String>()),
                        resumable: false,
                        session_usable: !s.session.is_dead(),
                        back_ns,
                    };
                }
            };
        }
        let back_ns = common::now_ns();
        let how = match &p {
            Progress::Parked {
                reason: ParkReason::Requested,
            } => "parked".to_owned(),
            Progress::Error(e) => format!("error: {}", e.type_name),
            _ => "other".to_owned(),
        };
        // Resumable: continue, park again on a second request.
        let park = s.session.park_handle();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(20));
            park.request();
        });
        let resumable = matches!(
            s.session.resume(&Resume::Continue),
            Ok(Progress::Parked {
                reason: ParkReason::Requested
            })
        );
        let _ = s.session.cancel();
        if s.session.is_dead() {
            // The first stop's outcome stands; the resume test's second stop
            // (20 ms into the next slice) outlived the 100 ms grace.
            return Control {
                how: format!("{how}; on resume, a second stop hit the epoch trap (instance lost)"),
                resumable: false,
                session_usable: false,
                back_ns,
            };
        }
        let probe = self.run(
            s,
            common::probe_script(Lang::Py),
            &mut common::Noop::default(),
        );
        Control {
            how: how.into(),
            resumable,
            session_usable: probe.as_deref() == Ok("42\n"),
            back_ns,
        }
    }

    fn run_until(&mut self, s: &mut S, src: &str, host: &mut dyn Host, stop_after: usize) -> bool {
        match self.start(s, src) {
            Ok(p) => matches!(self.drive(s, p, host, Some(stop_after)), Ok(Some(()))),
            Err(_) => false,
        }
    }

    fn snapshot(&mut self, s: &mut S) -> Option<Snap> {
        let (bytes, ns) = timed(|| s.session.snapshot(None));
        let pending = s
            .pending
            .iter()
            .map(|(id, n, a, _)| (*id, n.clone(), a.clone()))
            .collect();
        Some(Snap {
            kind: "full image (mid-await)",
            bytes: bytes.ok()?,
            snapshot_ns: ns,
            pending,
        })
    }

    fn restore(&mut self, snap: &Snap) -> Option<S> {
        let mut session = self.rt.restore(&snap.bytes, &options()).ok()?;
        let out = Arc::new(Mutex::new(String::new()));
        attach(&mut session, &out);
        Some(S {
            session,
            out,
            pending: snap
                .pending
                .iter()
                .map(|(id, n, a)| (*id, n.clone(), a.clone(), Ok(HostValue::None)))
                .collect(),
        })
    }

    fn finish(&mut self, s: &mut S, host: &mut dyn Host) -> RunResult {
        // The host answers the open calls again from its own record.
        for p in &mut s.pending {
            p.3 = answer(host.call(&p.1, p.2.clone()));
        }
        let p = s
            .session
            .pending()
            .map_err(|e| e.to_string())?
            .ok_or("not stopped")?;
        self.drive(s, p, host, None)?;
        Ok(Mm::take_output(s))
    }

    fn session_bytes(&mut self, s: &mut S) -> Option<u64> {
        Some(s.session.memory_bytes())
    }

    fn describe() -> Value {
        json!({"guest": guest().display().to_string(), "language": "Rust (no unsafe) in wasm32, wasmtime host",
            "fuel_slice": micro_monty::ASYNC_SLICE_FUEL})
    }
}

fn main() {
    common::main::<Mm>();
}
