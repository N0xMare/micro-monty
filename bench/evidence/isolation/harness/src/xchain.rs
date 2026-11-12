//! `iso-harness xchain`: the mm-wasm host-API scenario
//! (crates/mm-wasm-host/tests/parity.rs) as a snapshot chain, one event
//! per process, each hop on a chosen engine, so a driver can alternate
//! machines and engines between hops.
//!
//! ```text
//! iso-harness xchain step ENGINE DIR   one hop: create or restore DIR/image, advance one event, save
//! iso-harness xchain ref ENGINE        the same scenario in one process, no snapshots
//! ```
//! ENGINE is `native`, `abi` (wasm32-wasip1 guest) or `abi-uu`
//! (wasm32-unknown-unknown guest). Both print the report as one JSON line
//! once the scenario has finished (`step` prints `{"hop": ...}` before
//! that). Session state under test: the host module and its exception
//! class, structured host errors, the call log, a hint, the zones loaded
//! from the zone source, and the fuel left. Host-side settings (text cut,
//! text limit, the zone source itself) are set again on every hop, as a
//! host must.

use std::{
    path::Path,
    sync::{Arc, Mutex},
};

use micro_monty::{
    Engine, EngineConfig, HintGroup, HostError, HostErrorKind, HostException, HostModule,
    HostParam, HostSignature, HostValue, Limits, ParamKind, ParkReason, Progress, Resume, TextCut,
};
use serde_json::{Value, json};

const FUEL: u64 = 2_000;

const LIMITS: Limits = Limits::new()
    .with_memory(64 << 20)
    .with_recursion_depth(200)
    .with_fuel(None);

const CELL: &str = "\
import tools
from zoneinfo import ZoneInfo
from datetime import datetime
r = tools.search({'query': 'q', 'limit': 2})
for i in range(3):
    r = r + tools.search({'query': str(i), 'limit': i})
try:
    tools.fetch('x')
except tools.ToolError as e:
    print('caught', type(e).__name__, e.tool, e.data, isinstance(e, RuntimeError))
paris = ZoneInfo('Europe/Paris')
ny = ZoneInfo('America/New_York')
print(datetime(2026, 7, 1, 12, tzinfo=paris).utcoffset(), datetime(2026, 1, 1, tzinfo=ny).utcoffset())
x = 7 ** 100000
print(x.bit_length())
list(range(300))
";

/// Run after the cell, with no zone source: the zones must come from the image.
const TAIL: &str = "\
from zoneinfo import ZoneInfo
print(len(r), tools.VERSION, paris.key, ZoneInfo('America/New_York').key)
print(datetime(2026, 12, 1, tzinfo=ZoneInfo('Europe/Paris')).utcoffset())
r
";

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

fn zones() -> micro_monty::ZoneSource {
    Box::new(|key: &str| std::fs::read(format!("/usr/share/zoneinfo/{key}")).ok())
}

/// What to do next from `p`: answer `tools.search`, fail `tools.fetch`,
/// refuel, or nothing (the cell has ended).
fn action(p: &Progress) -> Option<&'static str> {
    match p {
        Progress::HostCall { name, .. } if name == "tools.search" => Some("search"),
        Progress::HostCall { .. } => Some("fetch"),
        Progress::Parked {
            reason: ParkReason::Fuel,
        } => Some("fuel"),
        _ => None,
    }
}

/// One engine's session behind the calls the scenario makes.
#[allow(clippy::large_enum_variant, reason = "one per process")]
enum Sess {
    Native(Engine, micro_monty::Session),
    Abi(mm_wasm_host::Session),
}

type Out = Arc<Mutex<String>>;

fn runtime(engine: &str) -> Result<mm_wasm_host::Runtime, String> {
    let target = if engine == "abi" {
        "wasm32-wasip1"
    } else {
        "wasm32-unknown-unknown"
    };
    mm_wasm_host::Runtime::new(
        &crate::abi_guest_path(target),
        mm_wasm_host::RuntimeConfig::default(),
    )
    .map_err(|e| e.to_string())
}

fn options() -> mm_wasm_host::SessionOptions {
    let mut o = mm_wasm_host::SessionOptions::new().with_limits(LIMITS);
    o.text_limit = Some(40);
    o
}

impl Sess {
    /// A new session (`image` None) or a restored one, with the host-side
    /// settings applied.
    fn open(engine: &str, image: Option<&[u8]>, out: &Out) -> Result<Sess, String> {
        let e = |e: &dyn std::fmt::Display| e.to_string();
        let mut s = match engine {
            "native" => {
                let eng = Engine::new(EngineConfig::new().with_limits(LIMITS));
                let mut n = match image {
                    Some(b) => eng.restore(b).map_err(|x| e(&x))?,
                    None => micro_monty::Session::new(&eng).map_err(|x| format!("{x:?}"))?,
                };
                let sink = Arc::clone(out);
                n.set_output(Box::new(move |t| {
                    sink.lock().unwrap_or_else(|p| p.into_inner()).push_str(t)
                }));
                n.set_text_cut(TextCut::HeadTail);
                n.set_text_limit(40);
                Sess::Native(eng, n)
            }
            "abi" | "abi-uu" => {
                // The runtime is leaked: one per process, as a host keeps it.
                let rt: &'static mm_wasm_host::Runtime = Box::leak(Box::new(runtime(engine)?));
                let mut w = match image {
                    Some(b) => rt.restore(b, &options()).map_err(|x| e(&x))?,
                    None => rt.session(&options()).map_err(|x| e(&x))?,
                };
                let sink = Arc::clone(out);
                w.set_output(Box::new(move |_, t| {
                    sink.lock().unwrap_or_else(|p| p.into_inner()).push_str(t);
                }));
                w.set_text_cut(TextCut::HeadTail).map_err(|x| e(&x))?;
                Sess::Abi(w)
            }
            other => return Err(format!("unknown engine {other}")),
        };
        s.set_zones(true);
        if image.is_none() {
            s.setup()?;
        }
        Ok(s)
    }

    fn set_zones(&mut self, on: bool) {
        match self {
            Sess::Native(_, n) => n.set_zone_source(on.then(zones)),
            Sess::Abi(w) => w.set_zone_source(on.then(zones)),
        }
    }

    /// Session state, set once: it must travel in the images.
    fn setup(&mut self) -> Result<(), String> {
        match self {
            Sess::Native(_, n) => {
                n.set_hint(HintGroup::Network, Some("Use the http tool."))
                    .map_err(|x| format!("{x:?}"))?;
                n.define(&module()).map_err(|x| format!("{x:?}"))?;
            }
            Sess::Abi(w) => {
                w.set_hint(HintGroup::Network, Some("Use the http tool."))
                    .map_err(|x| x.to_string())?;
                w.define(&module()).map_err(|x| x.to_string())?;
            }
        }
        Ok(())
    }

    fn set_fuel(&mut self, fuel: u64) -> Result<(), String> {
        match self {
            Sess::Native(_, n) => n.set_fuel(Some(fuel)),
            Sess::Abi(w) => w.set_fuel(Some(fuel)).map_err(|x| x.to_string())?,
        }
        Ok(())
    }

    fn fuel(&mut self) -> Result<Option<u64>, String> {
        match self {
            Sess::Native(_, n) => Ok(n.fuel()),
            Sess::Abi(w) => w.fuel().map_err(|x| x.to_string()),
        }
    }

    fn fuel_wanted(&mut self) -> Result<u64, String> {
        match self {
            Sess::Native(_, n) => Ok(n.fuel_wanted()),
            Sess::Abi(w) => w.fuel_wanted().map_err(|x| x.to_string()),
        }
    }

    fn start(&mut self, src: &str, file: &str) -> Result<Progress, String> {
        match self {
            Sess::Native(eng, n) => {
                Ok(n.start(&eng.compile(src, file).map_err(|x| format!("{x:?}"))?))
            }
            Sess::Abi(w) => w.start(src, file).map_err(|x| x.to_string()),
        }
    }

    fn resume(&mut self, r: &Resume) -> Result<Progress, String> {
        match self {
            Sess::Native(_, n) => n.resume(r.clone()).map_err(|x| format!("{x:?}")),
            Sess::Abi(w) => w.resume(r).map_err(|x| x.to_string()),
        }
    }

    fn snapshot(&mut self) -> Result<Vec<u8>, String> {
        match self {
            Sess::Native(_, n) => n.snapshot().map_err(|x| x.to_string()),
            Sess::Abi(w) => w.snapshot(None).map_err(|x| x.to_string()),
        }
    }

    fn call_log(&mut self) -> Result<String, String> {
        match self {
            Sess::Native(_, n) => Ok(format!("{:?}", n.call_log())),
            Sess::Abi(w) => w
                .call_log()
                .map(|l| format!("{l:?}"))
                .map_err(|x| x.to_string()),
        }
    }

    /// After the cell: the call log, the tail with no zone source, and the
    /// hint on an uncaught unsupported import.
    fn finish(&mut self) -> Result<Value, String> {
        let log = self.call_log()?;
        self.set_zones(false);
        self.set_fuel(10_000_000)?;
        let tail = format!("{:?}", self.start(TAIL, "tail.py")?);
        let hinted = format!("{:?}", self.start("import requests", "h.py")?);
        Ok(json!({"call_log": log, "tail": tail, "hinted": hinted}))
    }
}

/// Takes `action` (from [`action`]) and runs to the next event.
fn advance(s: &mut Sess, action: &str, wanted: &mut Vec<u64>) -> Result<Progress, String> {
    let r = match action {
        "search" => Resume::Value(HostValue::List(vec![HostValue::Int(1)])),
        "fetch" => Resume::Error(HostError {
            kind: HostErrorKind::Exception,
            message: "down".into(),
            data: Some(HostValue::Dict(vec![("code".into(), HostValue::Int(503))])),
            class: Some("tools.ToolError".into()),
        }),
        _ => {
            let w = s.fuel_wanted()?;
            wanted.push(w);
            s.set_fuel(w.max(FUEL))?;
            Resume::Continue
        }
    };
    s.resume(&r)
}

fn report(steps: Vec<String>, wanted: Vec<u64>, out: &Out, end: Value) -> Value {
    let out = out.lock().unwrap_or_else(|p| p.into_inner()).clone();
    json!({"steps": steps, "wanted": wanted, "out": out, "end": end})
}

fn reference(engine: &str) -> Result<Value, String> {
    let out = Out::default();
    let mut s = Sess::open(engine, None, &out)?;
    s.set_fuel(FUEL)?;
    let mut p = s.start(CELL, "cell.py")?;
    let (mut steps, mut wanted) = (vec![format!("{p:?}")], Vec::new());
    while let Some(a) = action(&p) {
        p = advance(&mut s, a, &mut wanted)?;
        steps.push(format!("{p:?}"));
    }
    let end = s.finish()?;
    Ok(report(steps, wanted, &out, end))
}

/// One hop. DIR holds `image` (the session parked at the pending event)
/// and `state.json` (steps so far, fuel wanted, output so far, fuel left,
/// and the pending action).
fn step(engine: &str, dir: &Path) -> Result<Value, String> {
    let io = |e: std::io::Error| e.to_string();
    let js = |e: serde_json::Error| e.to_string();
    let image = dir.join("image");
    let state_path = dir.join("state.json");
    let out = Out::default();
    let (mut s, mut steps, wanted, p) = if image.exists() {
        let state: Value =
            serde_json::from_slice(&std::fs::read(&state_path).map_err(io)?).map_err(js)?;
        *out.lock().unwrap_or_else(|p| p.into_inner()) =
            state["out"].as_str().unwrap_or("").to_owned();
        let mut s = Sess::open(engine, Some(&std::fs::read(&image).map_err(io)?), &out)?;
        if let Some(f) = state["fuel"].as_u64() {
            s.set_fuel(f)?;
        }
        let steps: Vec<String> = serde_json::from_value(state["steps"].clone()).map_err(js)?;
        let mut wanted: Vec<u64> = serde_json::from_value(state["wanted"].clone()).map_err(js)?;
        let a = state["action"].as_str().unwrap_or("").to_owned();
        let p = advance(&mut s, &a, &mut wanted)?;
        (s, steps, wanted, p)
    } else {
        let mut s = Sess::open(engine, None, &out)?;
        s.set_fuel(FUEL)?;
        let p = s.start(CELL, "cell.py")?;
        (s, Vec::new(), Vec::new(), p)
    };
    steps.push(format!("{p:?}"));
    let Some(a) = action(&p) else {
        let end = s.finish()?;
        return Ok(report(steps, wanted, &out, end));
    };
    let fuel = s.fuel()?;
    let bytes = s.snapshot()?;
    std::fs::write(&image, &bytes).map_err(io)?;
    let state = json!({
        "steps": steps,
        "wanted": wanted,
        "out": out.lock().unwrap_or_else(|p| p.into_inner()).clone(),
        "fuel": fuel,
        "action": a,
    });
    std::fs::write(&state_path, state.to_string()).map_err(io)?;
    Ok(json!({"hop": steps.len(), "image_bytes": bytes.len()}))
}

pub fn main(args: &[String]) -> Result<(), String> {
    let v = match (args.first().map(String::as_str), args.get(1), args.get(2)) {
        (Some("ref"), Some(engine), None) => reference(engine)?,
        (Some("step"), Some(engine), Some(dir)) => step(engine, Path::new(dir))?,
        _ => return Err("usage: iso-harness xchain ref ENGINE | step ENGINE DIR".into()),
    };
    println!("{v}");
    Ok(())
}
