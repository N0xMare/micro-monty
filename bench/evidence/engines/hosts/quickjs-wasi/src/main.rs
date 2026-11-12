//! quickjs-wasi (pi codemode's engine: quickjs-ng compiled to a WASI reactor,
//! `quickjs-wasi/quickjs.wasm` from npm, `$QUICKJS_WASM`) under wasmtime,
//! driven through its C interface exports the way its own TypeScript wrapper
//! and pi's `vm.ts` drive it: one instance per VM, pi's unmodified prelude
//! (the bridge is a host function `bridge`), the script as the body of
//! `(async (tools, console) => {...})`, results through `settle`, then the
//! job queue drained and `stalled()`, polling the interrupt flag every 50
//! jobs (`JOBS_PER_POLL`). QuickJS's own interrupt handler calls the
//! `host_interrupt` import.
//!
//! wasmtime is configured as crates/mm-wasm-host's default (epoch
//! interruption with a 10 ms ticker, a 4 MiB wasm stack, a precompiled
//! module), so the two wasm engines pay the same runtime costs. A stop is the
//! interrupt flag first; like mm-wasm-host, the epoch deadline traps the
//! instance when the flag has not stopped it within 100 ms (a long native
//! operation), and the instance is then lost.
//!
//! Snapshots are quickjs-wasi's own: the whole linear memory, the stack
//! pointer and the runtime and context pointers, restored into a new
//! instance of the same module (`QuickJS.snapshot` / `QuickJS.restore`).

use common::{
    Answer, Control, Engine, Host, Lang, RunResult, Snap, Stop, ToolDef, Value as Json, json, timed,
};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use wasmtime::{
    Caller, Config, Engine as WEngine, Instance, InstancePre, Linker, Memory, Module, Store,
    StoreLimits, StoreLimitsBuilder, TypedFunc, UpdateDeadline,
};
use wasmtime_wasi::{WasiCtxBuilder, p1::WasiP1Ctx};

const PRELUDE: &str = include_str!("../../../prelude/pi-prelude.js");
const MEMORY_LIMIT: u32 = 256 << 20;
const MAX_STACK: u32 = 512 << 10;
const JOBS_PER_POLL: u32 = 50;
const GRACE: Duration = Duration::from_millis(100);
const JS_EVAL_TYPE_GLOBAL: i32 = 0;

#[derive(Default)]
struct Bridge {
    calls: Vec<(f64, String, String)>,
    out: String,
    done: Option<Result<String, String>>,
}

macro_rules! funcs {
    ($($name:ident: ($($p:ty),*) -> $r:ty;)*) => {
        #[derive(Clone)]
        #[allow(non_snake_case, unused_parens, dead_code)]
        struct F { $($name: TypedFunc<($($p),*), $r>,)* }
        impl F {
            fn get(store: &mut Store<Ctx>, i: &Instance) -> wasmtime::Result<F> {
                Ok(F { $($name: i.get_typed_func(&mut *store, stringify!($name))?,)* })
            }
        }
    };
}

funcs! {
    _initialize: () -> ();
    qjs_init: () -> i32;
    qjs_eval: (i32, i32, i32, i32) -> i32;
    qjs_call: (i32, i32, i32, i32) -> i32;
    qjs_new_string: (i32, i32) -> i32;
    qjs_new_number: (f64) -> i32;
    qjs_get_string_len: (i32, i32) -> i32;
    qjs_free_cstring: (i32) -> ();
    qjs_get_float64: (i32) -> f64;
    qjs_get_bool: (i32) -> i32;
    qjs_is_exception: (i32) -> i32;
    qjs_get_exception: () -> i32;
    qjs_free_value: (i32) -> ();
    qjs_dup_value: (i32) -> i32;
    qjs_get_prop_string: (i32, i32) -> i32;
    qjs_set_prop_string: (i32, i32, i32) -> i32;
    qjs_get_global: () -> i32;
    qjs_get_undefined: () -> i32;
    qjs_get_true: () -> i32;
    qjs_get_false: () -> i32;
    qjs_new_host_function: (i32, i32, i32) -> i32;
    qjs_is_job_pending: () -> i32;
    qjs_execute_pending_job: () -> i32;
    qjs_set_memory_limit: (i32) -> ();
    qjs_set_max_stack_size: (i32) -> ();
    qjs_set_interrupt_handler: (i32) -> ();
    qjs_get_runtime_ptr: () -> i32;
    qjs_get_context_ptr: () -> i32;
    qjs_set_runtime_and_context: (i32, i32) -> ();
    qjs_compute_memory_usage: (i32) -> ();
    wasm_malloc: (i32) -> i32;
    wasm_free: (i32) -> ();
}

struct Ctx {
    wasi: WasiP1Ctx,
    limits: StoreLimits,
    f: Option<F>,
    memory: Option<Memory>,
    /// A 4-byte scratch cell in linear memory (string lengths).
    scratch: i32,
    bridge: Bridge,
    stop: Stop,
    /// Count polls without ever interrupting (unused flag: armed runs).
    armed: bool,
    /// The epoch deadline callback traps once a requested stop is older
    /// than the grace period.
    trapped: Arc<AtomicBool>,
}

struct S {
    store: Store<Ctx>,
    instance: Instance,
    /// `__api` and its `settle`, `run`, `stalled` (JSValue pointers).
    api: Option<[i32; 4]>,
}

struct Qw {
    engine: WEngine,
    pre: InstancePre<Ctx>,
    armed: bool,
    image: Option<Image>,
    ticker: Arc<AtomicBool>,
}

#[derive(Clone)]
struct Image {
    memory: Vec<u8>,
    sp: i32,
    rt: i32,
    ctx: i32,
    api: Option<[i32; 4]>,
}

fn wasm_path() -> PathBuf {
    std::env::var_os("QUICKJS_WASM").map_or_else(|| common::fail("set QUICKJS_WASM"), Into::into)
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// Reads the JS string of the JSValue at `v` (any value: ToString).
fn read_string(
    caller: &mut impl wasmtime::AsContextMut<Data = Ctx>,
    v: i32,
) -> wasmtime::Result<String> {
    let (f, mem, scratch) = {
        let c = caller.as_context_mut();
        let d = c.data();
        (
            d.f.clone()
                .ok_or_else(|| wasmtime::Error::msg("no funcs"))?,
            d.memory.ok_or_else(|| wasmtime::Error::msg("no memory"))?,
            d.scratch,
        )
    };
    let p = f.qjs_get_string_len.call(&mut *caller, (v, scratch))?;
    if p == 0 {
        return Ok(String::new());
    }
    let data = mem.data(&*caller);
    let len = u32::from_le_bytes(data[scratch as usize..scratch as usize + 4].try_into()?) as usize;
    let s = String::from_utf8_lossy(&data[p as usize..p as usize + len]).into_owned();
    f.qjs_free_cstring.call(&mut *caller, p)?;
    Ok(s)
}

fn read_i32(mem: &[u8], at: i32) -> i32 {
    i32::from_le_bytes(
        mem[at as usize..at as usize + 4]
            .try_into()
            .unwrap_or([0; 4]),
    )
}

/// The `host_call` import: `bridge(kind, a, b, c)` and `hsync(x)`.
fn host_call(
    mut c: Caller<'_, Ctx>,
    name: i32,
    name_len: i32,
    _this: i32,
    argc: i32,
    argv: i32,
) -> wasmtime::Result<i32> {
    let f = c
        .data()
        .f
        .clone()
        .ok_or_else(|| wasmtime::Error::msg("no funcs"))?;
    let mem = c
        .data()
        .memory
        .ok_or_else(|| wasmtime::Error::msg("no memory"))?;
    let (fname, args) = {
        let data = mem.data(&c);
        let fname =
            String::from_utf8_lossy(&data[name as usize..(name + name_len) as usize]).into_owned();
        let args: Vec<i32> = (0..argc).map(|i| read_i32(data, argv + 4 * i)).collect();
        (fname, args)
    };
    if fname == "hsync" {
        common::mark();
        return match args.first() {
            Some(&a) => f.qjs_dup_value.call(&mut c, a),
            None => f.qjs_get_undefined.call(&mut c, ()),
        };
    }
    let kind = read_string(&mut c, args[0])?;
    match kind.as_str() {
        "call" | "global" => {
            let id = f.qjs_get_float64.call(&mut c, args[1])?;
            let n = read_string(&mut c, args[2])?;
            let a = read_string(&mut c, args[3])?;
            c.data_mut().bridge.calls.push((id, n, a));
        }
        "output" => {
            let t = read_string(&mut c, args[2])?;
            let b = &mut c.data_mut().bridge;
            b.out.push_str(&t);
            b.out.push('\n');
        }
        "done" => {
            let ok = f.qjs_get_bool.call(&mut c, args[1])? != 0;
            let p = if args.len() > 2 {
                read_string(&mut c, args[2])?
            } else {
                String::new()
            };
            c.data_mut().bridge.done = Some(if ok { Ok(p) } else { Err(p) });
        }
        _ => {}
    }
    f.qjs_get_undefined.call(&mut c, ())
}

impl Qw {
    fn store(&self) -> Store<Ctx> {
        let wasi = WasiCtxBuilder::new().build_p1();
        let limits = StoreLimitsBuilder::new()
            .memory_size(2 * MEMORY_LIMIT as usize)
            .build();
        let trapped = Arc::new(AtomicBool::new(false));
        let mut store = Store::new(
            &self.engine,
            Ctx {
                wasi,
                limits,
                f: None,
                memory: None,
                scratch: 0,
                bridge: Bridge::default(),
                stop: Stop::default(),
                armed: self.armed,
                trapped,
            },
        );
        store.limiter(|c| &mut c.limits);
        store.set_epoch_deadline(1);
        store.epoch_deadline_callback(|c| {
            let at = c.data().stop.at();
            if at != 0 && common::now_ns().saturating_sub(at) > GRACE.as_nanos() as u64 {
                c.data().trapped.store(true, Ordering::SeqCst);
                return Err(wasmtime::Error::msg("interrupted by the epoch deadline"));
            }
            Ok(UpdateDeadline::Continue(1))
        });
        store
    }

    fn instance(&self) -> Result<S, String> {
        let mut store = self.store();
        let instance = self.pre.instantiate(&mut store).map_err(err)?;
        let f = F::get(&mut store, &instance).map_err(err)?;
        store.data_mut().memory = instance.get_memory(&mut store, "memory");
        store.data_mut().f = Some(f);
        Ok(S {
            store,
            instance,
            api: None,
        })
    }

    /// A new VM: `_initialize`, `qjs_init`, limits, interrupt handler, the
    /// `bridge` and `hsync` host functions (pi: `QuickJS.create`).
    fn vm(&self) -> Result<S, String> {
        let mut s = self.instance()?;
        let f = s.store.data().f.clone().ok_or("no funcs")?;
        f._initialize.call(&mut s.store, ()).map_err(err)?;
        if f.qjs_init.call(&mut s.store, ()).map_err(err)? != 0 {
            return Err("qjs_init failed".into());
        }
        Qw::limits(&mut s)?;
        let scratch = f.wasm_malloc.call(&mut s.store, 8).map_err(err)?;
        s.store.data_mut().scratch = scratch;
        let global = f.qjs_get_global.call(&mut s.store, ()).map_err(err)?;
        for name in ["bridge", "hsync"] {
            let n = put_cstr(&mut s, name)?;
            let func = f
                .qjs_new_host_function
                .call(&mut s.store, (n, name.len() as i32, 4))
                .map_err(err)?;
            f.qjs_set_prop_string
                .call(&mut s.store, (global, n, func))
                .map_err(err)?;
            f.qjs_free_value.call(&mut s.store, func).map_err(err)?;
            f.wasm_free.call(&mut s.store, n).map_err(err)?;
        }
        f.qjs_free_value.call(&mut s.store, global).map_err(err)?;
        Ok(s)
    }

    fn limits(s: &mut S) -> Result<(), String> {
        let f = s.store.data().f.clone().ok_or("no funcs")?;
        f.qjs_set_memory_limit
            .call(&mut s.store, MEMORY_LIMIT as i32)
            .map_err(err)?;
        f.qjs_set_max_stack_size
            .call(&mut s.store, MAX_STACK as i32)
            .map_err(err)?;
        let armed = s.store.data().armed;
        f.qjs_set_interrupt_handler
            .call(&mut s.store, i32::from(armed))
            .map_err(err)
    }

    /// Evaluates `src` as a global script: the completion value (owned).
    fn eval(s: &mut S, src: &str, file: &str) -> Result<i32, String> {
        let f = s.store.data().f.clone().ok_or("no funcs")?;
        let code = put_cstr(s, src)?;
        let name = put_cstr(s, file)?;
        let v = f
            .qjs_eval
            .call(
                &mut s.store,
                (code, src.len() as i32, name, JS_EVAL_TYPE_GLOBAL),
            )
            .map_err(err)?;
        f.wasm_free.call(&mut s.store, code).map_err(err)?;
        f.wasm_free.call(&mut s.store, name).map_err(err)?;
        Qw::check(s, v)
    }

    /// `v`, or the pending exception's text if `v` is an exception.
    fn check(s: &mut S, v: i32) -> Result<i32, String> {
        let f = s.store.data().f.clone().ok_or("no funcs")?;
        if f.qjs_is_exception.call(&mut s.store, v).map_err(err)? != 0 {
            f.qjs_free_value.call(&mut s.store, v).map_err(err)?;
            let e = f.qjs_get_exception.call(&mut s.store, ()).map_err(err)?;
            let text = read_string(&mut s.store, e).map_err(err)?;
            f.qjs_free_value.call(&mut s.store, e).map_err(err)?;
            return Err(text);
        }
        Ok(v)
    }

    fn call(s: &mut S, func: i32, this: i32, args: &[i32]) -> Result<(), String> {
        let f = s.store.data().f.clone().ok_or("no funcs")?;
        let argv = f
            .wasm_malloc
            .call(&mut s.store, 4 * args.len().max(1) as i32)
            .map_err(err)?;
        {
            let mem = s.store.data().memory.ok_or("no memory")?;
            let data = mem.data_mut(&mut s.store);
            for (i, a) in args.iter().enumerate() {
                data[argv as usize + 4 * i..argv as usize + 4 * i + 4]
                    .copy_from_slice(&a.to_le_bytes());
            }
        }
        let r = f
            .qjs_call
            .call(&mut s.store, (func, this, args.len() as i32, argv))
            .map_err(err)?;
        f.wasm_free.call(&mut s.store, argv).map_err(err)?;
        let r = Qw::check(s, r)?;
        f.qjs_free_value.call(&mut s.store, r).map_err(err)
    }

    /// pi's `drain`: jobs one at a time, the interrupt flag every 50, then
    /// `stalled()`.
    fn drain(s: &mut S) -> Result<(), String> {
        let f = s.store.data().f.clone().ok_or("no funcs")?;
        let mut jobs = 0u32;
        while f.qjs_is_job_pending.call(&mut s.store, ()).map_err(err)? != 0 {
            if f.qjs_execute_pending_job
                .call(&mut s.store, ())
                .map_err(err)?
                < 0
            {
                return Err("Promise job failed".into());
            }
            jobs += 1;
            if jobs % JOBS_PER_POLL == 0 && s.store.data().stop.requested() {
                return Err("Execution interrupted".into());
            }
        }
        let [api, _, _, stalled] = s.api.ok_or("no prelude")?;
        Qw::call(s, stalled, api, &[])
    }

    fn start(s: &mut S, src: &str) -> Result<(), String> {
        s.store.data_mut().bridge = Bridge::default();
        let fun = Qw::eval(
            s,
            &format!("(async (tools, console) => {{{src}\n}})"),
            "codemode.js",
        )?;
        let [api, _, run, _] = s.api.ok_or("no prelude")?;
        let r = Qw::call(s, run, api, &[fun]);
        let f = s.store.data().f.clone().ok_or("no funcs")?;
        f.qjs_free_value.call(&mut s.store, fun).map_err(err)?;
        r?;
        Qw::drain(s)
    }

    fn pump(s: &mut S, host: &mut dyn Host, stop_after: Option<usize>) -> Result<bool, String> {
        let f = s.store.data().f.clone().ok_or("no funcs")?;
        let mut delivered = 0;
        loop {
            if s.store.data().bridge.done.is_some() {
                return Ok(false);
            }
            if stop_after.is_some_and(|n| delivered >= n) {
                return Ok(true);
            }
            let calls = std::mem::take(&mut s.store.data_mut().bridge.calls);
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
                let idv = f.qjs_new_number.call(&mut s.store, id).map_err(err)?;
                let okv = if ok {
                    f.qjs_get_true.call(&mut s.store, ())
                } else {
                    f.qjs_get_false.call(&mut s.store, ())
                }
                .map_err(err)?;
                let pv = new_string(s, &payload)?;
                let props = match props {
                    Some(p) => new_string(s, &p)?,
                    None => f.qjs_get_undefined.call(&mut s.store, ()).map_err(err)?,
                };
                let [api, settle, _, _] = s.api.ok_or("no prelude")?;
                Qw::call(s, settle, api, &[idv, okv, pv, props])?;
                for v in [idv, okv, pv, props] {
                    f.qjs_free_value.call(&mut s.store, v).map_err(err)?;
                }
                Qw::drain(s)?;
                delivered += 1;
                if stop_after.is_some_and(|n| delivered >= n) {
                    let mut rest: Vec<_> = it.collect();
                    let b = &mut s.store.data_mut().bridge;
                    rest.append(&mut b.calls);
                    b.calls = rest;
                    break;
                }
            }
        }
    }

    fn result(s: &S) -> RunResult {
        let b = &s.store.data().bridge;
        match &b.done {
            Some(Ok(_)) => Ok(b.out.clone()),
            Some(Err(e)) => Err(e.clone()),
            None => Err("not done".into()),
        }
    }

    fn plain(&self, src: &str) -> RunResult {
        let mut s = self.vm()?;
        let v = Qw::eval(
            &mut s,
            "globalThis.console = { log: (...a) => bridge('output', 'text', a.map(String).join(' ')) };",
            "console.js",
        )?;
        free(&mut s, v)?;
        let v = Qw::eval(&mut s, src, "main.js")?;
        free(&mut s, v)?;
        let f = s.store.data().f.clone().ok_or("no funcs")?;
        while f.qjs_is_job_pending.call(&mut s.store, ()).map_err(err)? != 0 {
            f.qjs_execute_pending_job
                .call(&mut s.store, ())
                .map_err(err)?;
        }
        Ok(std::mem::take(&mut s.store.data_mut().bridge.out))
    }

    fn api(s: &mut S) -> Result<[i32; 4], String> {
        let f = s.store.data().f.clone().ok_or("no funcs")?;
        let global = f.qjs_get_global.call(&mut s.store, ()).map_err(err)?;
        let k = put_cstr(s, "__api")?;
        let api = f
            .qjs_get_prop_string
            .call(&mut s.store, (global, k))
            .map_err(err)?;
        f.wasm_free.call(&mut s.store, k).map_err(err)?;
        let mut out = [api, 0, 0, 0];
        for (i, name) in ["settle", "run", "stalled"].iter().enumerate() {
            let k = put_cstr(s, name)?;
            out[i + 1] = f
                .qjs_get_prop_string
                .call(&mut s.store, (api, k))
                .map_err(err)?;
            f.wasm_free.call(&mut s.store, k).map_err(err)?;
        }
        f.qjs_free_value.call(&mut s.store, global).map_err(err)?;
        Ok(out)
    }

    fn image(s: &mut S) -> Result<Image, String> {
        let f = s.store.data().f.clone().ok_or("no funcs")?;
        let mem = s.store.data().memory.ok_or("no memory")?;
        let memory = mem.data(&s.store).to_vec();
        let sp = s
            .instance
            .get_global(&mut s.store, "__stack_pointer")
            .ok_or("no __stack_pointer")?;
        let sp = sp.get(&mut s.store).i32().ok_or("sp")?;
        let rt = f.qjs_get_runtime_ptr.call(&mut s.store, ()).map_err(err)?;
        let ctx = f.qjs_get_context_ptr.call(&mut s.store, ()).map_err(err)?;
        Ok(Image {
            memory,
            sp,
            rt,
            ctx,
            api: s.api,
        })
    }

    /// `QuickJS.restore`: a new instance, memory grown and overwritten, the
    /// pointers set, limits applied again.
    fn restore_image(&self, img: &Image) -> Result<S, String> {
        let mut s = self.instance()?;
        let f = s.store.data().f.clone().ok_or("no funcs")?;
        let mem = s.store.data().memory.ok_or("no memory")?;
        let have = mem.data_size(&s.store);
        if img.memory.len() > have {
            mem.grow(&mut s.store, ((img.memory.len() - have) / 65536) as u64)
                .map_err(err)?;
        }
        mem.data_mut(&mut s.store)[..img.memory.len()].copy_from_slice(&img.memory);
        f.qjs_set_runtime_and_context
            .call(&mut s.store, (img.rt, img.ctx))
            .map_err(err)?;
        let sp = s
            .instance
            .get_global(&mut s.store, "__stack_pointer")
            .ok_or("no __stack_pointer")?;
        sp.set(&mut s.store, wasmtime::Val::I32(img.sp))
            .map_err(err)?;
        Qw::limits(&mut s)?;
        let scratch = f.wasm_malloc.call(&mut s.store, 8).map_err(err)?;
        s.store.data_mut().scratch = scratch;
        s.api = img.api;
        Ok(s)
    }
}

fn put_cstr(s: &mut S, text: &str) -> Result<i32, String> {
    let f = s.store.data().f.clone().ok_or("no funcs")?;
    let p = f
        .wasm_malloc
        .call(&mut s.store, text.len() as i32 + 1)
        .map_err(err)?;
    let mem = s.store.data().memory.ok_or("no memory")?;
    let data = mem.data_mut(&mut s.store);
    data[p as usize..p as usize + text.len()].copy_from_slice(text.as_bytes());
    data[p as usize + text.len()] = 0;
    Ok(p)
}

fn new_string(s: &mut S, text: &str) -> Result<i32, String> {
    let f = s.store.data().f.clone().ok_or("no funcs")?;
    let p = put_cstr(s, text)?;
    let v = f
        .qjs_new_string
        .call(&mut s.store, (p, text.len() as i32))
        .map_err(err)?;
    f.wasm_free.call(&mut s.store, p).map_err(err)?;
    Ok(v)
}

fn free(s: &mut S, v: i32) -> Result<(), String> {
    let f = s.store.data().f.clone().ok_or("no funcs")?;
    f.qjs_free_value.call(&mut s.store, v).map_err(err)
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

impl Engine for Qw {
    type S = S;
    const NAME: &'static str = "quickjs-wasi";
    const LANG: Lang = Lang::Js;

    fn new_runtime() -> Self {
        let mut c = Config::new();
        c.epoch_interruption(true);
        c.max_wasm_stack(4 << 20);
        c.async_stack_size(5 << 20);
        let engine = WEngine::new(&c).unwrap_or_else(|e| common::fail(&e.to_string()));
        let path = wasm_path();
        let bytes = std::fs::read(&path).unwrap_or_else(|e| common::fail(&e.to_string()));
        // Precompiled once per module and wasmtime build, as mm-wasm-host.
        let cache = path.with_extension(format!("{}.cwasm", common::zstd1(&bytes)));
        if !cache.exists() {
            let c = engine
                .precompile_module(&bytes)
                .unwrap_or_else(|e| common::fail(&e.to_string()));
            std::fs::write(&cache, c).unwrap_or_else(|e| common::fail(&e.to_string()));
        }
        // SAFETY: the file is the module this host precompiled above.
        let module = unsafe { Module::deserialize_file(&engine, &cache) }
            .unwrap_or_else(|e| common::fail(&e.to_string()));
        let mut linker: Linker<Ctx> = Linker::new(&engine);
        wasmtime_wasi::p1::add_to_linker_sync(&mut linker, |c: &mut Ctx| &mut c.wasi)
            .unwrap_or_else(|e| common::fail(&e.to_string()));
        linker
            .func_wrap("env", "host_call", host_call)
            .unwrap_or_else(|e| common::fail(&e.to_string()));
        linker
            .func_wrap("env", "host_interrupt", |c: Caller<'_, Ctx>| -> i32 {
                i32::from(c.data().stop.requested())
            })
            .unwrap_or_else(|e| common::fail(&e.to_string()));
        linker
            .func_wrap(
                "env",
                "host_get_timezone_offset",
                |_: Caller<'_, Ctx>, _: i32, _: i32| -> i32 { 0 },
            )
            .unwrap_or_else(|e| common::fail(&e.to_string()));
        linker
            .define_unknown_imports_as_traps(&module)
            .unwrap_or_else(|e| common::fail(&e.to_string()));
        let pre = linker
            .instantiate_pre(&module)
            .unwrap_or_else(|e| common::fail(&e.to_string()));
        let ticker = Arc::new(AtomicBool::new(false));
        let (e2, t2) = (engine.clone(), Arc::clone(&ticker));
        std::thread::spawn(move || {
            while !t2.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(10));
                e2.increment_epoch();
            }
        });
        Qw {
            engine,
            pre,
            armed: true,
            image: None,
            ticker,
        }
    }

    fn new_session(&mut self) -> S {
        self.vm().unwrap_or_else(|e| common::fail(&e))
    }

    fn setup(&mut self, s: &mut S, tools: &[ToolDef]) {
        let src = format!(
            "globalThis.__api = ({PRELUDE})(bridge, {}, '[]', '{{}}');",
            Json::String(tools_json(tools))
        );
        let v = Qw::eval(s, &src, "codemode-prelude.js").unwrap_or_else(|e| common::fail(&e));
        free(s, v).unwrap_or_else(|e| common::fail(&e));
        s.api = Some(Qw::api(s).unwrap_or_else(|e| common::fail(&e)));
    }

    fn prepare_image(&mut self, tools: &[ToolDef]) -> bool {
        let mut s = self.new_session();
        self.setup(&mut s, tools);
        self.image = Qw::image(&mut s).ok();
        self.image.is_some()
    }

    fn from_image(&mut self) -> Option<S> {
        let img = self.image.clone()?;
        self.restore_image(&img).ok()
    }

    fn run(&mut self, s: &mut S, src: &str, host: &mut dyn Host) -> RunResult {
        Qw::start(s, src)?;
        Qw::pump(s, host, None)?;
        Qw::result(s)
    }

    fn run_plain(&mut self, src: &str) -> RunResult {
        let armed = std::mem::replace(&mut self.armed, false);
        let r = self.plain(src);
        self.armed = armed;
        r
    }

    fn run_polled(&mut self, src: &str) -> RunResult {
        self.plain(src)
    }

    fn session_bytes(&mut self, s: &mut S) -> Option<u64> {
        s.store.data().memory.map(|m| m.data_size(&s.store) as u64)
    }

    fn control(&mut self, s: &mut S, src: &str, stop: &Stop, delay: Duration) -> Control {
        let mine = s.store.data().stop.clone();
        let harness = stop.clone();
        mine.request_after(delay, move || {
            harness.0.store(common::now_ns().max(1), Ordering::SeqCst)
        });
        let started = Qw::start(s, src);
        let back_ns = common::now_ns();
        let trapped = s.store.data().trapped.load(Ordering::SeqCst);
        let done = s.store.data().bridge.done.clone();
        let how = match (&started, &done) {
            _ if trapped => "epoch trap (instance lost)".to_owned(),
            (Err(e), _) | (_, Some(Err(e))) if e.contains("interrupted") => {
                "interrupted".to_owned()
            }
            (Err(e), _) => format!("error: {}", e.chars().take(80).collect::<String>()),
            _ => "other".to_owned(),
        };
        if trapped {
            return Control {
                how,
                resumable: false,
                session_usable: false,
                back_ns,
            };
        }
        mine.clear();
        let probe = self.run(
            s,
            common::probe_script(Lang::Js),
            &mut common::Noop::default(),
        );
        Control {
            how,
            resumable: false,
            session_usable: probe.as_deref() == Ok("42\n"),
            back_ns,
        }
    }

    fn run_until(&mut self, s: &mut S, src: &str, host: &mut dyn Host, stop_after: usize) -> bool {
        Qw::start(s, src).is_ok() && matches!(Qw::pump(s, host, Some(stop_after)), Ok(true))
    }

    fn snapshot(&mut self, s: &mut S) -> Option<Snap> {
        let (img, ns) = timed(|| Qw::image(s));
        let img = img.ok()?;
        // The serialized form of `QuickJS.serializeSnapshot`: a 24-byte header
        // and the memory.
        let mut bytes = Vec::with_capacity(24 + img.memory.len());
        bytes.extend_from_slice(b"QJSS\x01\0\0\0");
        for v in [img.memory.len() as i32, img.sp, img.rt, img.ctx] {
            bytes.extend_from_slice(&v.to_le_bytes());
        }
        bytes.extend_from_slice(&img.memory);
        let api = img.api.unwrap_or_default();
        for v in api {
            bytes.extend_from_slice(&v.to_le_bytes());
        }
        let pending = s
            .store
            .data()
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
            kind: "linear memory (QuickJS.snapshot, mid-await)",
            bytes,
            snapshot_ns: ns,
            pending,
        })
    }

    fn restore(&mut self, snap: &Snap) -> Option<S> {
        let b = &snap.bytes;
        let rd = |at: usize| i32::from_le_bytes(b[at..at + 4].try_into().unwrap_or([0; 4]));
        let len = rd(8) as usize;
        let api_at = 24 + len;
        let img = Image {
            memory: b[24..24 + len].to_vec(),
            sp: rd(12),
            rt: rd(16),
            ctx: rd(20),
            api: Some([rd(api_at), rd(api_at + 4), rd(api_at + 8), rd(api_at + 12)]),
        };
        let mut s = self.restore_image(&img).ok()?;
        s.store.data_mut().bridge.calls = snap
            .pending
            .iter()
            .map(|(id, n, a)| (*id as f64, n.clone(), a.to_string()))
            .collect();
        Some(s)
    }

    fn finish(&mut self, s: &mut S, host: &mut dyn Host) -> RunResult {
        Qw::pump(s, host, None)?;
        Qw::result(s)
    }

    fn describe() -> Json {
        json!({"module": wasm_path().display().to_string(), "language": "C (quickjs-ng) in wasm32-wasip1, wasmtime host",
            "memory_limit": MEMORY_LIMIT, "max_stack": MAX_STACK, "jobs_per_poll": JOBS_PER_POLL})
    }
}

impl Drop for Qw {
    fn drop(&mut self) {
        self.ticker.store(true, Ordering::Relaxed);
    }
}

fn main() {
    common::main::<Qw>();
}
