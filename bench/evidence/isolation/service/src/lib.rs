//! One micro-monty session behind a request/response API.
//!
//! The same [`Service`] runs in all three isolation modes, so they differ
//! only in transport: called directly (native in-process), through linear
//! memory (wasmtime in-process, `guest`), or as length-prefixed frames over
//! pipes (child process, `worker`). A message is `u32 LE json_len`, the JSON
//! request or response, then raw bytes (snapshot images); the child worker
//! puts a `u32 LE` length in front of that.

use std::sync::{Arc, Mutex};

use micro_monty::{
    Engine, EngineConfig, HostError, HostErrorKind, HostModule, HostValue, Limits, ParkHandle,
    ParkReason, Progress, Resume, Session,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value as Json};

/// Output kept per cell; the rest is dropped (plak caps a cell at 1 MiB).
pub const OUTPUT_CAP: usize = 1 << 20;

/// What the host asks.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "request", rename_all = "snake_case")]
pub enum Request {
    /// Engine version and build id.
    Hello,
    /// Replaces the session with a new one: memory limit and the untyped
    /// builtins (host functions) to define.
    NewSession { memory: u64, define: Vec<String> },
    /// Compiles and starts a cell; `fuel` (if set) before starting.
    Run { source: String, fuel: Option<u64> },
    /// Answers the current stop; `fuel` (if set) before resuming.
    Resume {
        resume: ResumeMsg,
        fuel: Option<u64>,
    },
    /// FULL image of the session (raw bytes follow the response).
    Snapshot,
    /// Replaces the session with the image in the raw bytes.
    Restore { memory: u64 },
    /// Ends a stopped cell, keeping the session.
    Cancel,
    /// Makes the next facade operation `op` panic (`test_hooks`).
    Inject { op: String },
    /// Drops the session (and the service's memory with it).
    Drop,
    /// Requests a park; the child worker handles it on its reader thread.
    Park,
}

/// The answer to a stop.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ResumeMsg {
    Continue,
    Value { value: Json },
    Error { message: String },
    Future,
    Batch { results: Vec<(u64, Json)> },
}

/// What the service answers.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "ev", rename_all = "snake_case")]
pub enum Response {
    Hello {
        version: String,
        build_id: String,
    },
    Ok,
    HostCall {
        call: u64,
        name: String,
        args: Vec<Json>,
    },
    Waiting {
        calls: Vec<u64>,
    },
    Parked {
        reason: String,
    },
    Complete {
        repr: Option<String>,
        output: String,
    },
    Error {
        type_name: String,
        message: String,
        output: String,
    },
    Snapshot {
        bytes: usize,
    },
    Failed {
        message: String,
    },
}

/// One session and its output buffer.
pub struct Service {
    engine: Engine,
    session: Option<Session>,
    output: Arc<Mutex<String>>,
    park: Arc<Mutex<Option<ParkHandle>>>,
}

impl Default for Service {
    fn default() -> Self {
        Service::new()
    }
}

impl Service {
    /// A service without a session.
    #[must_use]
    pub fn new() -> Service {
        Service {
            engine: Engine::new(EngineConfig::default()),
            session: None,
            output: Arc::new(Mutex::new(String::new())),
            park: Arc::new(Mutex::new(None)),
        }
    }

    /// The current session's park handle, for a thread other than the one
    /// running the session (the worker's reader, the native host).
    #[must_use]
    pub fn park_slot(&self) -> Arc<Mutex<Option<ParkHandle>>> {
        Arc::clone(&self.park)
    }

    /// The session, for the native host's direct-facade measurements.
    pub fn session(&mut self) -> Option<&mut Session> {
        self.session.as_mut()
    }

    fn install(&mut self, mut session: Session) {
        // As mm-cell-worker: completion text capped near 1 MiB.
        session.set_text_limit(OUTPUT_CAP as u64);
        let sink = Arc::clone(&self.output);
        session.set_output(Box::new(move |text: &str| {
            if let Ok(mut out) = sink.lock() {
                if out.len() + text.len() <= OUTPUT_CAP {
                    out.push_str(text);
                }
            }
        }));
        if let Ok(mut park) = self.park.lock() {
            *park = Some(session.park_handle());
        }
        self.session = Some(session);
    }

    fn take_output(&self) -> String {
        self.output
            .lock()
            .map(|mut o| std::mem::take(&mut *o))
            .unwrap_or_default()
    }

    /// Handles one request; the second value is the raw bytes that follow
    /// the response (a snapshot image).
    pub fn handle(&mut self, request: &Request, raw: &[u8]) -> (Response, Vec<u8>) {
        let failed = |message: String| (Response::Failed { message }, Vec::new());
        match request {
            Request::Hello => (
                Response::Hello {
                    version: micro_monty::VERSION.to_owned(),
                    build_id: hex(micro_monty::build_id()),
                },
                Vec::new(),
            ),
            Request::NewSession { memory, define } => {
                self.session = None;
                self.engine = engine(*memory);
                match Session::new(&self.engine) {
                    Ok(mut session) => {
                        let names: Vec<&str> = define.iter().map(String::as_str).collect();
                        if let Err(e) = session.define(&HostModule::untyped("", &names)) {
                            return failed(format!("{e:?}"));
                        }
                        self.install(session);
                        (Response::Ok, Vec::new())
                    }
                    Err(e) => failed(e.message),
                }
            }
            Request::Run { source, fuel } => {
                let Some(session) = self.session.as_mut() else {
                    return failed("no session".into());
                };
                let code = match self.engine.compile(source, "cell.py") {
                    Ok(code) => code,
                    Err(e) => return failed(e.rendered),
                };
                if fuel.is_some() {
                    session.set_fuel(*fuel);
                }
                let progress = session.start(&code);
                (self.respond(progress), Vec::new())
            }
            Request::Resume { resume, fuel } => {
                let Some(session) = self.session.as_mut() else {
                    return failed("no session".into());
                };
                if fuel.is_some() {
                    session.set_fuel(*fuel);
                }
                match session.resume(to_resume(resume)) {
                    Ok(progress) => (self.respond(progress), Vec::new()),
                    Err(e) => failed(e.to_string()),
                }
            }
            Request::Snapshot => {
                let Some(session) = self.session.as_mut() else {
                    return failed("no session".into());
                };
                match session.snapshot() {
                    Ok(bytes) => (Response::Snapshot { bytes: bytes.len() }, bytes),
                    Err(e) => failed(e.to_string()),
                }
            }
            Request::Restore { memory } => {
                self.session = None;
                self.engine = engine(*memory);
                match self.engine.restore(raw) {
                    Ok(session) => {
                        self.install(session);
                        (Response::Ok, Vec::new())
                    }
                    Err(e) => failed(e.to_string()),
                }
            }
            Request::Cancel => match self.session.as_mut() {
                Some(session) => {
                    session.cancel();
                    (Response::Ok, Vec::new())
                }
                None => failed("no session".into()),
            },
            Request::Inject { op } => {
                let op: &'static str = match op.as_str() {
                    "start" => "start",
                    "resume" => "resume",
                    "snapshot" => "snapshot",
                    "restore" => "restore",
                    _ => return failed(format!("unknown op {op}")),
                };
                micro_monty::test_hooks::panic_next(op);
                (Response::Ok, Vec::new())
            }
            Request::Drop => {
                self.session = None;
                (Response::Ok, Vec::new())
            }
            Request::Park => {
                if let Ok(park) = self.park.lock() {
                    if let Some(handle) = park.as_ref() {
                        handle.request();
                    }
                }
                (Response::Ok, Vec::new())
            }
        }
    }

    fn respond(&self, progress: Progress) -> Response {
        match progress {
            Progress::Complete(c) => Response::Complete {
                repr: c.map(|c| c.repr),
                output: self.take_output(),
            },
            Progress::HostCall {
                call, name, args, ..
            } => Response::HostCall {
                call,
                name,
                args: args.iter().map(to_json).collect(),
            },
            Progress::Waiting { calls } => Response::Waiting { calls },
            Progress::Parked { reason } => Response::Parked {
                reason: match reason {
                    ParkReason::Fuel => "fuel",
                    ParkReason::Requested => "requested",
                    ParkReason::Deadline => "deadline",
                    _ => "other",
                }
                .into(),
            },
            Progress::Error(e) => Response::Error {
                type_name: e.type_name,
                message: e.message,
                output: self.take_output(),
            },
            other => Response::Failed {
                message: format!("unexpected progress {other:?}"),
            },
        }
    }
}

fn engine(memory: u64) -> Engine {
    Engine::new(EngineConfig::new().with_limits(Limits::new().with_memory(memory)))
}

/// The facade answer for a message.
#[must_use]
pub fn to_resume(resume: &ResumeMsg) -> Resume {
    match resume {
        ResumeMsg::Continue => Resume::Continue,
        ResumeMsg::Value { value } => Resume::Value(from_json(value)),
        ResumeMsg::Error { message } => Resume::Error(HostError::new(
            HostErrorKind::ValueError,
            message.clone(),
        )),
        ResumeMsg::Future => Resume::Future,
        ResumeMsg::Batch { results } => Resume::Batch(
            results
                .iter()
                .map(|(call, value)| (*call, Ok(from_json(value))))
                .collect(),
        ),
    }
}

/// A host value as JSON: tuples as arrays, records and objects as objects.
#[must_use]
pub fn to_json(value: &HostValue) -> Json {
    match value {
        HostValue::None => Json::Null,
        HostValue::Bool(b) => Json::Bool(*b),
        HostValue::Int(i) => Json::from(*i),
        HostValue::BigInt(s) | HostValue::Str(s) => Json::String(s.clone()),
        HostValue::Float(f) => serde_json::Number::from_f64(*f).map_or(Json::Null, Json::Number),
        HostValue::Bytes(b) => Json::Array(b.iter().map(|&x| Json::from(x)).collect()),
        HostValue::List(items) | HostValue::Tuple(items) => {
            Json::Array(items.iter().map(to_json).collect())
        }
        HostValue::Dict(fields)
        | HostValue::Record { fields, .. }
        | HostValue::Object { fields, .. } => Json::Object(
            fields
                .iter()
                .map(|(k, v)| (k.clone(), to_json(v)))
                .collect::<Map<_, _>>(),
        ),
        HostValue::Unconverted(u) => Json::String(format!("{u:?}")),
        other => Json::String(format!("{other:?}")),
    }
}

/// JSON as a host value.
#[must_use]
pub fn from_json(value: &Json) -> HostValue {
    match value {
        Json::Null => HostValue::None,
        Json::Bool(b) => HostValue::Bool(*b),
        Json::Number(n) => n
            .as_i64()
            .map(HostValue::Int)
            .unwrap_or_else(|| HostValue::Float(n.as_f64().unwrap_or(f64::NAN))),
        Json::String(s) => HostValue::Str(s.clone()),
        Json::Array(items) => HostValue::List(items.iter().map(from_json).collect()),
        Json::Object(fields) => HostValue::Dict(
            fields
                .iter()
                .map(|(k, v)| (k.clone(), from_json(v)))
                .collect(),
        ),
    }
}

/// Lowercase hex.
#[must_use]
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut s, b| {
        s.push_str(&format!("{b:02x}"));
        s
    })
}

/// `u32 LE json_len`, JSON, raw.
#[must_use]
pub fn encode<T: Serialize>(message: &T, raw: &[u8]) -> Vec<u8> {
    let json = serde_json::to_vec(message).unwrap_or_default();
    let mut out = Vec::with_capacity(4 + json.len() + raw.len());
    out.extend_from_slice(&u32::try_from(json.len()).unwrap_or(u32::MAX).to_le_bytes());
    out.extend_from_slice(&json);
    out.extend_from_slice(raw);
    out
}

/// The inverse of [`encode`].
///
/// # Errors
/// A short message or invalid JSON.
pub fn decode<T: for<'de> Deserialize<'de>>(message: &[u8]) -> Result<(T, &[u8]), String> {
    let (len, rest) = message.split_first_chunk::<4>().ok_or("short message")?;
    let len = u32::from_le_bytes(*len) as usize;
    if rest.len() < len {
        return Err("truncated message".into());
    }
    let (json, raw) = rest.split_at(len);
    let value = serde_json::from_slice(json).map_err(|e| e.to_string())?;
    Ok((value, raw))
}
