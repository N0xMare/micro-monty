//! rquickjs under the shared engine harness: crates.io rquickjs (quickjs-ng)
//! by default, dquickts's vendored rquickjs (Bellard QuickJS with heap
//! snapshots) with the `snapshots` feature (hosts/dquickts).
//!
//! The codemode setup is pi's: its unmodified prelude
//! (`prelude/pi-prelude.js`) called with the bridge, the tools, no globals
//! and an empty store, exactly as pi's `vm.ts` (`startVm`) does on
//! quickjs-wasi; the script runs as the body of
//! `(async (tools, console) => {...})`, tool arguments and results cross as
//! JSON text, each result is delivered with `settle` followed by a drain of
//! the job queue and `stalled()`, as `deliver` does. A session is one
//! QuickJS runtime and one context (pi: one wasm instance per VM), with
//! pi's 256 MB heap limit and 512 KiB stack guard.

use common::{
    Answer, Control, Engine, Host, Lang, RunResult, Snap, Stop, ToolDef, Value as Json, json, timed,
};
use rquickjs::function::{Opt, This};
use rquickjs::{CatchResultExt, Context, Ctx, Function, Object, Persistent, Runtime, Value};
use std::cell::RefCell;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

const PRELUDE: &str = include_str!("../../../prelude/pi-prelude.js");
const MEMORY_LIMIT: usize = 256 << 20;
const MAX_STACK: usize = 512 << 10;

/// What the prelude reports through `bridge`.
#[derive(Default)]
struct Bridge {
    /// Calls issued and not yet delivered: pi id, tool name, JSON arguments.
    calls: Vec<(f64, String, String)>,
    out: String,
    done: Option<Result<String, String>>,
}

/// Fields drop in order: the prelude's handle before the context, the
/// context before the runtime.
struct S {
    /// `{settle, run, stalled}` of the prelude, after setup.
    api: Option<Persistent<Object<'static>>>,
    ctx: Context,
    rt: Runtime,
    /// The interrupt handler's poll counter and stop flag.
    polls: Arc<AtomicU64>,
    stop: Stop,
}

struct Qjs {
    /// Count polls without ever interrupting (`run_polled`).
    armed: bool,
    #[cfg(feature = "snapshots")]
    image: Option<Vec<u8>>,
}

fn new_session(armed: bool) -> S {
    let rt = Runtime::new().unwrap_or_else(|e| common::fail(&e.to_string()));
    rt.set_memory_limit(MEMORY_LIMIT);
    rt.set_max_stack_size(MAX_STACK);
    let ctx = Context::full(&rt).unwrap_or_else(|e| common::fail(&e.to_string()));
    let polls = Arc::new(AtomicU64::new(0));
    let stop = Stop::default();
    if armed {
        let (p, st) = (Arc::clone(&polls), stop.clone());
        rt.set_interrupt_handler(Some(Box::new(move || {
            p.fetch_add(1, Ordering::Relaxed);
            st.requested()
        })));
    }
    S {
        api: None,
        ctx,
        rt,
        polls,
        stop,
    }
}

fn text(v: &Value<'_>) -> String {
    if let Some(s) = v.as_string() {
        return s.to_string().unwrap_or_default();
    }
    if let Some(n) = v.as_number() {
        return n.to_string();
    }
    if let Some(b) = v.as_bool() {
        return b.to_string();
    }
    String::new()
}

fn exception(ctx: &Ctx<'_>, e: rquickjs::Error) -> String {
    match e {
        rquickjs::Error::Exception => format!("{:?}", ctx.catch()),
        other => other.to_string(),
    }
}

thread_local! {
    /// The prelude's reports. One session runs at a time per thread, and a
    /// snapshot can only reference registered natives, not closures over a
    /// session's state, so the bridge writes here.
    static BRIDGE: RefCell<Bridge> = RefCell::default();
}

fn bridge<R>(f: impl FnOnce(&mut Bridge) -> R) -> R {
    BRIDGE.with(|b| f(&mut b.borrow_mut()))
}

fn make_bridge<'js>(ctx: Ctx<'js>) -> rquickjs::Result<Function<'js>> {
    Function::new(
        ctx,
        |kind: String, a: Opt<Value<'js>>, bv: Opt<Value<'js>>, c: Opt<Value<'js>>| {
            let (a, bv, c) = (a.0, bv.0, c.0);
            bridge(|st| match kind.as_str() {
                "call" | "global" => {
                    let id = a.as_ref().and_then(Value::as_number).unwrap_or(0.0);
                    let name = bv.as_ref().map(text).unwrap_or_default();
                    let args = c.as_ref().map(text).unwrap_or_default();
                    st.calls.push((id, name, args));
                }
                "output" => {
                    st.out.push_str(&bv.as_ref().map(text).unwrap_or_default());
                    st.out.push('\n');
                }
                "done" => {
                    let ok = a.as_ref().and_then(Value::as_bool).unwrap_or(false);
                    let payload = bv.as_ref().map(text).unwrap_or_default();
                    st.done = Some(if ok { Ok(payload) } else { Err(payload) });
                }
                _ => {}
            })
        },
    )
}

fn make_hsync<'js>(ctx: Ctx<'js>) -> rquickjs::Result<Function<'js>> {
    Function::new(ctx, |x: f64| {
        common::mark();
        x
    })
}

/// Defines the bridge and `hsync` as globals.
fn define_host(s: &S) {
    s.ctx.with(|ctx| {
        let g = ctx.globals();
        #[cfg(not(feature = "snapshots"))]
        let (b, h) = (make_bridge(ctx.clone()), make_hsync(ctx.clone()));
        #[cfg(feature = "snapshots")]
        let (b, h) = {
            let natives = snapshot::host()
                .natives(&ctx)
                .unwrap_or_else(|e| common::fail(&e.to_string()));
            (natives.function("bridge"), natives.function("hsync"))
        };
        let b = b.unwrap_or_else(|e| common::fail(&e.to_string()));
        let h = h.unwrap_or_else(|e| common::fail(&e.to_string()));
        g.set("__bridge", b)
            .unwrap_or_else(|e| common::fail(&e.to_string()));
        g.set("hsync", h)
            .unwrap_or_else(|e| common::fail(&e.to_string()));
    });
}

impl Qjs {
    /// Runs every pending job, then `stalled()`, as pi's `drain`.
    fn drain(s: &S) -> Result<(), String> {
        loop {
            match s.rt.execute_pending_job() {
                Ok(true) => {}
                Ok(false) => break,
                Err(_) => return Err("Promise job failed".into()),
            }
        }
        s.ctx.with(|ctx| -> Result<(), String> {
            let api = s
                .api
                .clone()
                .ok_or("no prelude")?
                .restore(&ctx)
                .map_err(|e| e.to_string())?;
            let stalled: Function = api.get("stalled").map_err(|e| e.to_string())?;
            stalled
                .call::<_, Value>((This(api),))
                .map_err(|e| exception(&ctx, e))?;
            Ok(())
        })
    }

    fn settle(s: &S, id: f64, a: Answer<'_>) -> Result<(), String> {
        s.ctx.with(|ctx| -> Result<(), String> {
            let api = s
                .api
                .clone()
                .ok_or("no prelude")?
                .restore(&ctx)
                .map_err(|e| e.to_string())?;
            let settle: Function = api.get("settle").map_err(|e| e.to_string())?;
            let r = match a {
                Answer::Ok(v) => settle.call::<_, Value>((
                    This(api),
                    id,
                    true,
                    v.to_string(),
                    rquickjs::Undefined,
                )),
                Answer::Err(kind, msg) => settle.call::<_, Value>((
                    This(api),
                    id,
                    false,
                    msg.to_owned(),
                    json!({ "type": kind }).to_string(),
                )),
            };
            r.map_err(|e| exception(&ctx, e))?;
            Ok(())
        })
    }

    /// Delivers results until the script is done, or until `stop_after`
    /// results were delivered and it waits again (true).
    fn pump(s: &S, host: &mut dyn Host, stop_after: Option<usize>) -> Result<bool, String> {
        let mut delivered = 0;
        loop {
            if bridge(|b| b.done.is_some()) {
                return Ok(false);
            }
            if stop_after.is_some_and(|n| delivered >= n) {
                return Ok(true);
            }
            let calls = bridge(|b| std::mem::take(&mut b.calls));
            if calls.is_empty() {
                return Err("waiting with no call (stalled not reported)".into());
            }
            let mut it = calls.into_iter();
            while let Some((id, name, args)) = it.next() {
                let args: Json = serde(&args);
                Qjs::settle(s, id, host.call(&name, args))?;
                Qjs::drain(s)?;
                delivered += 1;
                if stop_after.is_some_and(|n| delivered >= n) {
                    // Keep the rest open, as calls the host has not answered.
                    let rest: Vec<_> = it.collect();
                    bridge(|b| {
                        let new = std::mem::take(&mut b.calls);
                        b.calls = rest;
                        b.calls.extend(new);
                    });
                    break;
                }
            }
        }
    }

    fn start(s: &S, src: &str) -> Result<(), String> {
        bridge(|b| {
            b.out.clear();
            b.done = None;
            b.calls.clear();
        });
        s.ctx.with(|ctx| -> Result<(), String> {
            let api = s
                .api
                .clone()
                .ok_or("no prelude")?
                .restore(&ctx)
                .map_err(|e| e.to_string())?;
            let run: Function = api.get("run").map_err(|e| e.to_string())?;
            let f: Function = ctx
                .eval(format!("(async (tools, console) => {{{src}\n}})"))
                .map_err(|e| exception(&ctx, e))?;
            run.call::<_, Value>((This(api), f))
                .map_err(|e| exception(&ctx, e))?;
            Ok(())
        })?;
        Qjs::drain(s)
    }

    fn result(_s: &S) -> RunResult {
        bridge(|b| match &b.done {
            Some(Ok(_)) => Ok(b.out.clone()),
            Some(Err(e)) => Err(e.clone()),
            None => Err("not done".into()),
        })
    }

    fn plain(&self, src: &str, armed: bool) -> RunResult {
        let s = new_session(armed);
        define_host(&s);
        bridge(|b| b.out.clear());
        s.ctx.with(|ctx| -> Result<(), String> {
            ctx.eval::<(), _>(
                "globalThis.console = { log: (...a) => __bridge('output', 'text', a.map(String).join(' ')) };",
            )
            .map_err(|e| exception(&ctx, e))?;
            ctx.eval::<(), _>(src).catch(&ctx).map_err(|e| e.to_string())?;
            Ok(())
        })?;
        loop {
            match s.rt.execute_pending_job() {
                Ok(true) => {}
                Ok(false) => break,
                Err(_) => return Err("job failed".into()),
            }
        }
        Ok(bridge(|b| b.out.clone()))
    }
}

fn serde(text: &str) -> Json {
    common::serde_json::from_str(text).unwrap_or(Json::Null)
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

impl Engine for Qjs {
    type S = S;
    #[cfg(not(feature = "snapshots"))]
    const NAME: &'static str = "rquickjs";
    #[cfg(feature = "snapshots")]
    const NAME: &'static str = "dquickts";
    const LANG: Lang = Lang::Js;

    fn new_runtime() -> Self {
        Qjs {
            armed: false,
            #[cfg(feature = "snapshots")]
            image: None,
        }
    }

    fn new_session(&mut self) -> S {
        let s = new_session(true);
        define_host(&s);
        s
    }

    fn setup(&mut self, s: &mut S, tools: &[ToolDef]) {
        let api = s
            .ctx
            .with(|ctx| -> Result<Persistent<Object<'static>>, String> {
                let make: Function = ctx.eval(PRELUDE).map_err(|e| exception(&ctx, e))?;
                let bridge: Function = ctx.globals().get("__bridge").map_err(|e| e.to_string())?;
                let api: Object = make
                    .call((bridge, tools_json(tools), "[]", "{}"))
                    .map_err(|e| exception(&ctx, e))?;
                Ok(Persistent::save(&ctx, api))
            });
        s.api = Some(api.unwrap_or_else(|e| common::fail(&e)));
    }

    fn run(&mut self, s: &mut S, src: &str, host: &mut dyn Host) -> RunResult {
        Qjs::start(s, src)?;
        Qjs::pump(s, host, None)?;
        Qjs::result(s)
    }

    fn run_plain(&mut self, src: &str) -> RunResult {
        self.plain(src, self.armed)
    }

    fn run_polled(&mut self, src: &str) -> RunResult {
        self.plain(src, true)
    }

    fn session_bytes(&mut self, s: &mut S) -> Option<u64> {
        Some(s.rt.memory_usage().malloc_size as u64)
    }

    fn control(&mut self, s: &mut S, src: &str, _stop: &Stop, delay: Duration) -> Control {
        let harness = _stop.clone();
        s.stop.request_after(delay, move || {
            harness.0.store(common::now_ns().max(1), Ordering::SeqCst)
        });
        let started = Qjs::start(s, src);
        let back_ns = common::now_ns();
        let done = bridge(|b| b.done.clone());
        let how = match (&started, &done) {
            (Err(e), _) | (_, Some(Err(e))) if e.contains("interrupted") => {
                "interrupted".to_owned()
            }
            (Err(e), _) => format!("error: {}", e.chars().take(80).collect::<String>()),
            (_, Some(Err(e))) => format!("failed: {}", e.chars().take(80).collect::<String>()),
            _ => "other".to_owned(),
        };
        s.stop.clear();
        // The runtime survives if its context still evaluates. pi's prelude
        // is one run per VM (`finished`), so a probe through `run` says
        // whether the interrupted run already reported, not whether the
        // engine instance is lost: noted, not used.
        let raw = s
            .ctx
            .with(|ctx| ctx.eval::<i32, _>("6 * 7").map_err(|e| exception(&ctx, e)));
        let reported = bridge(|b| b.done.is_some()) || started.is_err();
        let how = format!("{how}{}", if reported { "" } else { " (run not reported)" });
        Control {
            how,
            resumable: false,
            session_usable: raw == Ok(42),
            back_ns,
        }
    }

    fn run_until(&mut self, s: &mut S, src: &str, host: &mut dyn Host, stop_after: usize) -> bool {
        Qjs::start(s, src).is_ok() && matches!(Qjs::pump(s, host, Some(stop_after)), Ok(true))
    }

    fn finish(&mut self, s: &mut S, host: &mut dyn Host) -> RunResult {
        Qjs::pump(s, host, None)?;
        Qjs::result(s)
    }

    #[cfg(feature = "snapshots")]
    fn prepare_image(&mut self, tools: &[ToolDef]) -> bool {
        let mut s = self.new_session();
        self.setup(&mut s, tools);
        self.image = snapshot::write(&s).map(|(b, _)| b);
        self.image.is_some()
    }

    #[cfg(feature = "snapshots")]
    fn from_image(&mut self) -> Option<S> {
        snapshot::read(self.image.as_ref()?)
    }

    #[cfg(feature = "snapshots")]
    fn snapshot(&mut self, s: &mut S) -> Option<Snap> {
        let (r, ns) = timed(|| snapshot::write(s));
        let (bytes, pending) = r?;
        Some(Snap {
            kind: "heap snapshot (JS_WriteSnapshot, mid-await)",
            bytes,
            snapshot_ns: ns,
            pending,
        })
    }

    #[cfg(feature = "snapshots")]
    fn restore(&mut self, snap: &Snap) -> Option<S> {
        let s = snapshot::read(&snap.bytes)?;
        bridge(|b| {
            b.calls = snap
                .pending
                .iter()
                .map(|(id, n, a)| (*id as f64, n.clone(), a.to_string()))
                .collect()
        });
        Some(s)
    }

    fn gc_session(&mut self, limit: u64) -> Option<S> {
        let s = new_session(false);
        s.rt.set_memory_limit(limit as usize);
        Some(s)
    }

    /// The interrupt handler is QuickJS's control point: it runs every
    /// 10,000 polls (calls and backward jumps), never inside a collection.
    fn gc_run(&mut self, s: &mut S, src: &str, gaps: &common::Gaps) -> Result<(), String> {
        let g = gaps.clone();
        s.rt.set_interrupt_handler(Some(Box::new(move || {
            g.tick();
            false
        })));
        let r = s
            .ctx
            .with(|ctx| ctx.eval::<(), _>(src).map_err(|e| exception(&ctx, e)));
        s.rt.set_interrupt_handler(None);
        r
    }

    /// `JS_RunGC`: the cycle collector over every GC object.
    fn gc_full(&mut self, s: &mut S) -> Result<(), String> {
        s.rt.run_gc();
        Ok(())
    }

    fn describe() -> Json {
        json!({"language": "C (QuickJS) with Rust bindings", "memory_limit": MEMORY_LIMIT, "max_stack": MAX_STACK})
    }
}

#[cfg(not(feature = "snapshots"))]
#[allow(dead_code)]
fn unused(_: Snap) {
    let _ = timed(|| ());
}

#[cfg(feature = "snapshots")]
mod snapshot {
    include!("snapshot.rs");
}

fn main() {
    common::main::<Qjs>();
}
