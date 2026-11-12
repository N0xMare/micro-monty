//! micro-monty (native facade) under the shared engine harness.
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
    Engine as MmEngine, EngineConfig, HostError, HostErrorKind, HostModule, HostParam,
    HostSignature, HostValue, Limits, ParamKind, ParkReason, Progress, Resume, Session,
    SessionTemplate,
};
use std::sync::{Arc, Mutex};
use std::time::Duration;

struct Mm {
    engine: MmEngine,
    template: Option<SessionTemplate>,
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

fn limits() -> Limits {
    Limits::new().with_fuel(None)
}

impl Mm {
    fn session(&self) -> S {
        let mut session = Session::new(&self.engine).unwrap_or_else(|e| common::fail(&e.message));
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
                    s.session.set_fuel(self.slice);
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
        let code = self
            .engine
            .compile(src, "cell.py")
            .map_err(|e| e.rendered)?;
        if let Ok(mut o) = s.out.lock() {
            o.clear();
        }
        s.session.set_fuel(self.slice);
        Ok(s.session.start(&code))
    }

    fn take_output(s: &S) -> String {
        s.out.lock().map(|o| o.clone()).unwrap_or_default()
    }
}

fn attach(session: &mut Session, out: &Arc<Mutex<String>>) {
    let sink = Arc::clone(out);
    session.set_output(Box::new(move |t| {
        if let Ok(mut o) = sink.lock() {
            o.push_str(t);
        }
    }));
}

fn resume(session: &mut Session, r: Resume) -> Result<Progress, String> {
    session.resume(r).map_err(|e| e.to_string())
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
    const NAME: &'static str = "mm-native";
    const LANG: Lang = Lang::Py;

    fn new_runtime() -> Self {
        Mm {
            engine: MmEngine::new(EngineConfig::new().with_limits(limits())),
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
        // The template carries what a codemode cell imports first;
        // `MM_TEMPLATE_IMPORTS` (comma-separated) overrides the set.
        let set = std::env::var("MM_TEMPLATE_IMPORTS").unwrap_or_else(|_| "asyncio, json".into());
        let code = self
            .engine
            .compile(&format!("import {set}\ndel {set}\n"), "setup.py");
        let code = code.unwrap_or_else(|e| common::fail(&e.rendered));
        let _ = s.session.start(&code);
        self.template = Some(
            SessionTemplate::capture(&self.engine, &mut s.session)
                .unwrap_or_else(|e| common::fail(&e.to_string())),
        );
        true
    }

    fn from_image(&mut self) -> Option<S> {
        let mut session = self.template.as_ref()?.instantiate().ok()?;
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
            Err(e) => common::fail(&e),
        };
        while let Progress::HostCall { ref name, .. } = p {
            if name == "hsync" {
                common::mark();
            }
            p = resume(&mut s.session, Resume::Value(HostValue::Int(0)))
                .unwrap_or_else(|e| common::fail(&e));
        }
        let back_ns = common::now_ns();
        let how = match p {
            Progress::Parked {
                reason: ParkReason::Requested,
            } => "parked",
            Progress::Error(_) => "error",
            _ => "other",
        };
        // Resumable: continue, park again on a second request. The request
        // is made before resuming: a request stays until a park takes it, so
        // a timer firing as the run ends would park the probe below instead.
        s.session.request_park();
        let resumable = matches!(
            s.session.resume(Resume::Continue),
            Ok(Progress::Parked {
                reason: ParkReason::Requested
            })
        );
        s.session.cancel();
        let mut probe = self.run(
            s,
            common::probe_script(Lang::Py),
            &mut common::Noop::default(),
        );
        // A resumed run that ended (an exception) before parking leaves the
        // request for the next cell, which the probe took: run it again.
        if !resumable && probe.as_ref().is_err_and(|e| e.starts_with("parked")) {
            s.session.cancel();
            probe = self.run(
                s,
                common::probe_script(Lang::Py),
                &mut common::Noop::default(),
            );
        }
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
        let (bytes, ns) = timed(|| s.session.snapshot());
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
        let mut session = self.engine.restore(&snap.bytes).ok()?;
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
        let p = s.session.pending().ok_or("not stopped")?;
        self.drive(s, p, host, None)?;
        Ok(Mm::take_output(s))
    }

    fn gc_session(&mut self, limit: u64) -> Option<S> {
        self.engine = MmEngine::new(EngineConfig::new().with_limits(limits().with_memory(limit)));
        Some(self.session())
    }

    fn gc_run(&mut self, s: &mut S, src: &str, gaps: &common::Gaps) -> Result<(), String> {
        let slice = micro_monty::ASYNC_SLICE_FUEL;
        let code = self.engine.compile(src, "gc.py").map_err(|e| e.rendered)?;
        s.session.set_fuel(Some(slice));
        let mut p = s.session.start(&code);
        loop {
            p = match p {
                Progress::Parked {
                    reason: ParkReason::Fuel,
                } => {
                    gaps.tick();
                    s.session.set_fuel(Some(slice));
                    resume(&mut s.session, Resume::Continue)?
                }
                Progress::Complete(_) => return Ok(()),
                Progress::Error(e) => return Err(e.traceback),
                _ => return Err("unexpected progress".into()),
            }
        }
    }

    /// The park handle, as the spec §9.2 measurements: a request every
    /// `every` while none is outstanding, the latency to `Parked`.
    fn gc_requests(&mut self, s: &mut S, src: &str, every: Duration) -> Result<Vec<u64>, String> {
        use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::SeqCst};
        let code = self.engine.compile(src, "gc.py").map_err(|e| e.rendered)?;
        let (asked, done) = (
            Arc::new(AtomicU64::new(0)),
            Arc::new(AtomicBool::new(false)),
        );
        let (a, d, park) = (
            Arc::clone(&asked),
            Arc::clone(&done),
            s.session.park_handle(),
        );
        let asker = std::thread::spawn(move || {
            while !d.load(SeqCst) {
                std::thread::sleep(every);
                if a.load(SeqCst) == 0 {
                    a.store(common::now_ns().max(1), SeqCst);
                    park.request();
                }
            }
        });
        s.session.set_fuel(None);
        let mut p = s.session.start(&code);
        let mut latencies = Vec::new();
        let r = loop {
            p = match p {
                Progress::Parked { .. } => {
                    let at = asked.swap(0, SeqCst);
                    if at != 0 {
                        latencies.push(common::now_ns().saturating_sub(at));
                    }
                    match resume(&mut s.session, Resume::Continue) {
                        Ok(p) => p,
                        Err(e) => break Err(e),
                    }
                }
                Progress::Complete(_) => break Ok(latencies),
                Progress::Error(e) => break Err(e.traceback),
                _ => break Err("unexpected progress".into()),
            }
        };
        done.store(true, SeqCst);
        let _ = asker.join();
        // A request made as the run ended stays until a park takes it.
        if asked.load(SeqCst) != 0 {
            let code = self
                .engine
                .compile("pass\n", "gc.py")
                .map_err(|e| e.rendered)?;
            let mut p = s.session.start(&code);
            while let Progress::Parked { .. } = p {
                p = resume(&mut s.session, Resume::Continue)?;
            }
        }
        r
    }

    fn gc_full(&mut self, s: &mut S) -> Result<(), String> {
        let p = self.start(s, "gc.collect()\n")?;
        self.drive(s, p, &mut common::Noop::default(), None)
            .map(|_| ())
    }

    fn describe() -> Value {
        json!({"build_id": micro_monty::build_id().iter().map(|b| format!("{b:02x}")).collect::<String>(),
            "language": "Rust (no unsafe)", "fuel_slice": micro_monty::ASYNC_SLICE_FUEL})
    }
}

fn main() {
    common::main::<Mm>();
}
