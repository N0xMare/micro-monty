//! djs (djs-core + djs-prelude) under the shared engine harness.
//!
//! The codemode setup follows djs's own pi adapter (`js/pi/vm.ts`, shape
//! "pi"): pi's unmodified prelude, called with the sync host function
//! `bridge` (id 1), the tools, no globals and an empty store; the script is
//! compiled as `(async (tools, console) => {...})` and passed to the
//! prelude's `run`; each tool result goes back through `settle`, then the VM
//! runs until idle and `stalled()` is called. The VM runs in slices of
//! `FUEL_PER_POLL` (10,000) fuel, the adapter's default, with an interrupt
//! poll between slices; `run_plain` runs unsliced.

use common::{
    Answer, Control, Engine, Host, Lang, RunResult, Snap, Stop, ToolDef, Value as Json, json, timed,
};
use djs_core::compiler::CompileOptions;
use djs_core::heap::Handle;
use djs_core::value::Value;
use djs_core::vm::{Status, Vm, VmOptions};
use std::sync::atomic::Ordering;
use std::time::Duration;

const PRELUDE: &str = include_str!("../../../prelude/pi-prelude.js");
const BRIDGE: u32 = 1;
const HSYNC: u32 = 2;
const FUEL_PER_POLL: u64 = 10_000;
/// pi's heap limit (256 MB).
const MEMORY_LIMIT: usize = 256 << 20;

#[derive(Default)]
struct Bridge {
    calls: Vec<(f64, String, String)>,
    out: String,
    done: Option<Result<String, String>>,
}

struct S {
    vm: Vm,
    bridge: Bridge,
    /// `__api` and its `settle`, `run`, `stalled`, rooted.
    api: Option<[Handle; 4]>,
    stop: Stop,
}

struct Djs {
    /// Fuel per slice; `u64::MAX` runs unsliced.
    slice: u64,
    image: Option<Vec<u8>>,
}

fn opts() -> VmOptions {
    VmOptions {
        memory_limit: MEMORY_LIMIT,
        ..VmOptions::default()
    }
}

fn new_vm() -> Vm {
    let mut vm = djs_prelude::new_vm(opts()).unwrap_or_else(|e| common::fail(&format!("{e:?}")));
    for (id, name) in [(BRIDGE, "bridge"), (HSYNC, "hsync")] {
        let f = vm
            .new_host_function(id, name)
            .unwrap_or_else(|e| common::fail(&format!("{e:?}")));
        vm.define_global(name, f)
            .unwrap_or_else(|e| common::fail(&format!("{e:?}")));
    }
    vm
}

fn string(vm: &Vm, v: Value) -> String {
    match v.as_string() {
        Some(s) => vm.heap().str_to_string(s),
        None => v.as_number().map(|n| n.to_string()).unwrap_or_default(),
    }
}

fn new_str(vm: &mut Vm, s: &str) -> Result<Value, String> {
    vm.heap_mut()
        .alloc_str(s)
        .map(Value::string)
        .map_err(|e| format!("{e:?}"))
}

impl Djs {
    /// Runs the VM until done or idle, answering `bridge` and `hsync`; polls
    /// `stop` between slices (the adapter's `loop`).
    fn exec(&self, s: &mut S) -> Result<Status, String> {
        loop {
            match s.vm.run(self.slice) {
                Status::OutOfFuel => {
                    if s.stop.requested() {
                        return Ok(Status::OutOfFuel);
                    }
                }
                Status::HostCall { call_id, fn_id } => {
                    let info = s.vm.host_call(call_id).ok_or("no host call")?;
                    let args = info.args.clone();
                    let mut reply = Value::UNDEFINED;
                    if fn_id == HSYNC {
                        common::mark();
                        reply = args.first().copied().unwrap_or(Value::UNDEFINED);
                    } else {
                        let arg = |i: usize| args.get(i).copied().unwrap_or(Value::UNDEFINED);
                        let kind = string(&s.vm, arg(0));
                        match kind.as_str() {
                            "call" | "global" => {
                                let id = arg(1).as_number().unwrap_or(0.0);
                                let (n, a) = (string(&s.vm, arg(2)), string(&s.vm, arg(3)));
                                s.bridge.calls.push((id, n, a));
                            }
                            "output" => {
                                let t = string(&s.vm, arg(2));
                                s.bridge.out.push_str(&t);
                                s.bridge.out.push('\n');
                            }
                            "done" => {
                                let ok = arg(1).as_bool().unwrap_or(false);
                                let p = string(&s.vm, arg(2));
                                s.bridge.done = Some(if ok { Ok(p) } else { Err(p) });
                            }
                            _ => {}
                        }
                    }
                    s.vm.resolve_host_call(call_id, Ok(reply))
                        .map_err(|e| format!("{e:?}"))?;
                }
                Status::Threw(v) => {
                    return Err(s.vm.error_stack(v).unwrap_or_else(|| s.vm.display(v)));
                }
                other => return Ok(other),
            }
        }
    }

    fn call(&self, s: &mut S, f: Value, this: Value, args: &[Value]) -> Result<Status, String> {
        s.vm.call(f, this, args).map_err(|e| format!("{e:?}"))?;
        self.exec(s)
    }

    fn eval(&self, s: &mut S, src: &str, name: &str) -> Result<Value, String> {
        let f =
            s.vm.compile(src.as_bytes(), &CompileOptions::script(name))
                .map_err(|e| format!("{e:?}"))?;
        let this = Value::object(s.vm.global());
        match self.call(s, f, this, &[])? {
            Status::Done(v) => Ok(v),
            other => Err(format!("eval: {other:?}")),
        }
    }

    fn api(s: &S, i: usize) -> Result<Value, String> {
        Ok(s.vm
            .heap()
            .get_handle(s.api.as_ref().ok_or("no prelude")?[i]))
    }

    fn stalled(&self, s: &mut S) -> Result<(), String> {
        let (f, this) = (Djs::api(s, 3)?, Djs::api(s, 0)?);
        self.call(s, f, this, &[]).map(|_| ())
    }

    fn start(&self, s: &mut S, src: &str) -> Result<Status, String> {
        s.bridge = Bridge::default();
        let f = self.eval(
            s,
            &format!("(async (tools, console) => {{{src}\n}})"),
            "codemode.js",
        )?;
        let (run, this) = (Djs::api(s, 2)?, Djs::api(s, 0)?);
        let st = self.call(s, run, this, &[f])?;
        if st == Status::OutOfFuel {
            return Ok(st);
        }
        self.stalled(s)?;
        Ok(st)
    }

    fn pump(
        &self,
        s: &mut S,
        host: &mut dyn Host,
        stop_after: Option<usize>,
    ) -> Result<bool, String> {
        let mut delivered = 0;
        loop {
            if s.bridge.done.is_some() {
                return Ok(false);
            }
            if stop_after.is_some_and(|n| delivered >= n) {
                return Ok(true);
            }
            let calls = std::mem::take(&mut s.bridge.calls);
            if calls.is_empty() {
                return Err("waiting with no call".into());
            }
            let mut it = calls.into_iter();
            while let Some((id, name, args)) = it.next() {
                let args: Json = common::serde_json::from_str(&args).unwrap_or(Json::Null);
                let (ok, payload, props) = match host.call(&name, args) {
                    Answer::Ok(v) => (true, v.to_string(), None),
                    Answer::Err(k, m) => {
                        (false, m.to_owned(), Some(json!({"type": k}).to_string()))
                    }
                };
                let payload = new_str(&mut s.vm, &payload)?;
                let props = match props {
                    Some(p) => new_str(&mut s.vm, &p)?,
                    None => Value::UNDEFINED,
                };
                let (settle, this) = (Djs::api(s, 1)?, Djs::api(s, 0)?);
                self.call(
                    s,
                    settle,
                    this,
                    &[Value::number(id), Value::bool(ok), payload, props],
                )?;
                self.stalled(s)?;
                delivered += 1;
                if stop_after.is_some_and(|n| delivered >= n) {
                    let mut rest: Vec<_> = it.collect();
                    rest.append(&mut s.bridge.calls);
                    s.bridge.calls = rest;
                    break;
                }
            }
        }
    }

    fn result(s: &S) -> RunResult {
        match &s.bridge.done {
            Some(Ok(_)) => Ok(s.bridge.out.clone()),
            Some(Err(e)) => Err(e.clone()),
            None => Err("not done".into()),
        }
    }

    fn plain(&self, src: &str) -> RunResult {
        let mut s = S {
            vm: new_vm(),
            bridge: Bridge::default(),
            api: None,
            stop: Stop::default(),
        };
        self.eval(
            &mut s,
            "globalThis.console = { log: (...a) => bridge('output', 'text', a.map(String).join(' ')) };",
            "console.js",
        )?;
        self.eval(&mut s, src, "main.js")?;
        Ok(s.bridge.out)
    }
}

fn tools_json(tools: &[ToolDef]) -> String {
    Json::Array(
        tools
            .iter()
            .map(|t| json!({"name": t.name, "jsName": t.name, "description": t.description}))
            .collect(),
    )
    .to_string()
}

impl Engine for Djs {
    type S = S;
    const NAME: &'static str = "djs";
    const LANG: Lang = Lang::Js;

    fn new_runtime() -> Self {
        Djs {
            slice: FUEL_PER_POLL,
            image: None,
        }
    }

    fn new_session(&mut self) -> S {
        S {
            vm: new_vm(),
            bridge: Bridge::default(),
            api: None,
            stop: Stop::default(),
        }
    }

    fn setup(&mut self, s: &mut S, tools: &[ToolDef]) {
        let src = format!(
            "globalThis.__api = ({PRELUDE})(bridge, {}, '[]', '{{}}');",
            Json::String(tools_json(tools))
        );
        self.eval(s, &src, "codemode-prelude.js")
            .unwrap_or_else(|e| common::fail(&e));
        s.api = Some(handles(&mut s.vm).unwrap_or_else(|| common::fail("prelude handles")));
    }

    fn prepare_image(&mut self, tools: &[ToolDef]) -> bool {
        let mut s = self.new_session();
        self.setup(&mut s, tools);
        self.image = Some(s.vm.image().to_vec());
        true
    }

    fn from_image(&mut self) -> Option<S> {
        let image = self.image.clone()?;
        let mut vm = Vm::from_bytes_trusted(image, opts()).ok()?;
        let api = handles(&mut vm)?;
        Some(S {
            vm,
            bridge: Bridge::default(),
            api: Some(api),
            stop: Stop::default(),
        })
    }

    fn run(&mut self, s: &mut S, src: &str, host: &mut dyn Host) -> RunResult {
        self.start(s, src)?;
        self.pump(s, host, None)?;
        Djs::result(s)
    }

    fn run_plain(&mut self, src: &str) -> RunResult {
        let slice = std::mem::replace(&mut self.slice, u64::MAX);
        let r = self.plain(src);
        self.slice = slice;
        r
    }

    fn run_polled(&mut self, src: &str) -> RunResult {
        self.plain(src)
    }

    fn session_bytes(&mut self, s: &mut S) -> Option<u64> {
        Some(s.vm.bytes().len() as u64)
    }

    fn control(&mut self, s: &mut S, src: &str, stop: &Stop, delay: Duration) -> Control {
        let harness = stop.clone();
        s.stop.request_after(delay, move || {
            harness.0.store(common::now_ns().max(1), Ordering::SeqCst)
        });
        let started = self.start(s, src);
        let back_ns = common::now_ns();
        let how = match &started {
            Ok(Status::OutOfFuel) => "stopped at a slice".to_owned(),
            Ok(other) => format!("{other:?}"),
            Err(e) => format!("error: {}", e.chars().take(80).collect::<String>()),
        };
        // Resumable: the stopped call runs on for another slice.
        let resumable = matches!(started, Ok(Status::OutOfFuel))
            && s.vm.run(FUEL_PER_POLL) == Status::OutOfFuel;
        s.vm.abort();
        s.stop.clear();
        let probe = self.run(
            s,
            common::probe_script(Lang::Js),
            &mut common::Noop::default(),
        );
        Control {
            how,
            resumable,
            session_usable: probe.as_deref() == Ok("42\n"),
            back_ns,
        }
    }

    fn run_until(&mut self, s: &mut S, src: &str, host: &mut dyn Host, stop_after: usize) -> bool {
        self.start(s, src).is_ok() && matches!(self.pump(s, host, Some(stop_after)), Ok(true))
    }

    fn snapshot(&mut self, s: &mut S) -> Option<Snap> {
        let (bytes, ns) = timed(|| s.vm.image().to_vec());
        let pending = s
            .bridge
            .calls
            .iter()
            .map(|(id, n, a)| {
                (
                    *id as u64,
                    n.clone(),
                    common::serde_json::from_str(a).unwrap_or(Json::Null),
                )
            })
            .collect();
        Some(Snap {
            kind: "raw heap image (Vm::image, in-process format)",
            bytes,
            snapshot_ns: ns,
            pending,
        })
    }

    fn restore(&mut self, snap: &Snap) -> Option<S> {
        let mut vm = Vm::from_bytes(snap.bytes.clone(), opts()).ok()?;
        let api = handles(&mut vm)?;
        let calls = snap
            .pending
            .iter()
            .map(|(id, n, a)| (*id as f64, n.clone(), a.to_string()))
            .collect();
        Some(S {
            vm,
            bridge: Bridge {
                calls,
                ..Bridge::default()
            },
            api: Some(api),
            stop: Stop::default(),
        })
    }

    fn finish(&mut self, s: &mut S, host: &mut dyn Host) -> RunResult {
        self.pump(s, host, None)?;
        Djs::result(s)
    }

    fn gc_session(&mut self, limit: u64) -> Option<S> {
        let opts = VmOptions {
            memory_limit: limit as usize,
            ..VmOptions::default()
        };
        Some(S {
            vm: djs_prelude::new_vm(opts).ok()?,
            bridge: Bridge::default(),
            api: None,
            stop: Stop::default(),
        })
    }

    /// Slices of `FUEL_PER_POLL`, the adapter's control points; djs
    /// collects at safepoints inside a slice (`Heap::collect_auto`).
    fn gc_run(&mut self, s: &mut S, src: &str, gaps: &common::Gaps) -> Result<(), String> {
        let f =
            s.vm.compile(src.as_bytes(), &CompileOptions::script("gc.js"))
                .map_err(|e| format!("{e:?}"))?;
        let this = Value::object(s.vm.global());
        s.vm.call(f, this, &[]).map_err(|e| format!("{e:?}"))?;
        loop {
            match s.vm.run(FUEL_PER_POLL) {
                Status::OutOfFuel => gaps.tick(),
                Status::Done(_) => return Ok(()),
                Status::Threw(v) => {
                    return Err(s.vm.error_stack(v).unwrap_or_else(|| s.vm.display(v)));
                }
                other => return Err(format!("{other:?}")),
            }
        }
    }

    /// `Vm::collect`: a full mark-compact collection.
    fn gc_full(&mut self, s: &mut S) -> Result<(), String> {
        s.vm.collect().map_err(|e| format!("{e:?}"))
    }

    fn describe() -> Json {
        json!({"language": "Rust (no_std core)", "fuel_per_poll": FUEL_PER_POLL, "memory_limit": MEMORY_LIMIT})
    }
}

/// `__api` and its `settle`, `run` and `stalled`, rooted in handles.
fn handles(vm: &mut Vm) -> Option<[Handle; 4]> {
    let mut h = |p: &str| {
        let v = vm.global_path(p)?;
        vm.heap_mut().new_handle(v).ok()
    };
    Some([
        h("__api")?,
        h("__api.settle")?,
        h("__api.run")?,
        h("__api.stalled")?,
    ])
}

fn main() {
    common::main::<Djs>();
}
