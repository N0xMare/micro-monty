//! djs's wasm module (`djs_wasm.wasm` from `scripts/build-wasm.sh`, then
//! `wasm-opt -Oz`; `$DJS_WASM`) under wasmtime, configured as the
//! quickjs-wasi host (mm-wasm-host's defaults). One instance per VM, as pi's
//! djs adapter does.
//!
//! Only plain programs, a new VM, and pi's prelude setup run here: djs's
//! wasm exports hand host-call arguments out as raw arena values, which only
//! djs's own TypeScript reader decodes, so the codemode protocol is measured
//! on the native build (hosts/djs, the same engine code). The bench programs
//! print through a `console.log` that collects into `__o`, read back with
//! `vm_display`; no host calls.

use common::{Control, Engine, Host, Lang, RunResult, Stop, ToolDef, Value as Json, json};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use wasmtime::{
    Config, Engine as WEngine, Instance, InstancePre, Linker, Memory, Module, Store, TypedFunc,
};

const PRELUDE: &str = include_str!("../../../../prelude/pi-prelude.js");
const DONE: u32 = 0;
const THREW: u32 = 1;
const OUT_OF_FUEL: u32 = 2;
const FUEL_PER_POLL: u32 = 10_000;

struct S {
    store: Store<()>,
    mem: Memory,
    vm: u32,
    f: F,
}

#[derive(Clone)]
struct F {
    vm_new: TypedFunc<u32, u32>,
    djs_staging: TypedFunc<u32, u32>,
    djs_out_ptr: TypedFunc<(), u32>,
    djs_out_len: TypedFunc<(), u32>,
    vm_compile: TypedFunc<(u32, u32, u32, u32), u32>,
    vm_call: TypedFunc<(u32, u32, u32, u32), i32>,
    vm_run: TypedFunc<(u32, u32), u32>,
    vm_result: TypedFunc<u32, u32>,
    vm_display: TypedFunc<(u32, u32), u32>,
    vm_handle_free: TypedFunc<(u32, u32), ()>,
}

struct Dw {
    engine: WEngine,
    pre: InstancePre<()>,
    slice: u32,
    ticker: Arc<AtomicBool>,
    /// The module's custom section `djs_base` (the coded base snapshot).
    coded: Vec<u8>,
    /// The base the first instance decoded, injected into later ones (pi's
    /// `provideBase`/`rememberBase`, js/src/base.ts).
    base: Option<Vec<u8>>,
}

/// The payload of custom section `name` in a wasm module.
fn custom_section(b: &[u8], name: &str) -> Option<Vec<u8>> {
    fn leb(b: &[u8], i: &mut usize) -> usize {
        let (mut r, mut s) = (0usize, 0);
        loop {
            let x = b[*i];
            *i += 1;
            r |= ((x & 0x7f) as usize) << s;
            s += 7;
            if x < 0x80 {
                return r;
            }
        }
    }
    let mut i = 8;
    while i < b.len() {
        let id = b[i];
        i += 1;
        let n = leb(b, &mut i);
        let end = i + n;
        if id == 0 {
            let mut j = i;
            let nl = leb(b, &mut j);
            if &b[j..j + nl] == name.as_bytes() {
                return Some(b[j + nl..end].to_vec());
            }
        }
        i = end;
    }
    None
}

fn path() -> PathBuf {
    std::env::var_os("DJS_WASM").map_or_else(|| common::fail("set DJS_WASM"), Into::into)
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

impl S {
    fn stage(&mut self, text: &str) -> Result<(), String> {
        let p = self
            .f
            .djs_staging
            .call(&mut self.store, text.len() as u32)
            .map_err(err)?;
        self.mem.data_mut(&mut self.store)[p as usize..p as usize + text.len()]
            .copy_from_slice(text.as_bytes());
        Ok(())
    }

    fn out(&mut self) -> Result<String, String> {
        let p = self.f.djs_out_ptr.call(&mut self.store, ()).map_err(err)? as usize;
        let n = self.f.djs_out_len.call(&mut self.store, ()).map_err(err)? as usize;
        Ok(String::from_utf8_lossy(&self.mem.data(&self.store)[p..p + n]).into_owned())
    }

    /// Compiles and runs `src` as a Script; the completion value's text.
    fn eval(&mut self, src: &str, slice: u32) -> Result<String, String> {
        self.stage(src)?;
        let h = self
            .f
            .vm_compile
            .call(&mut self.store, (self.vm, 0, 0, src.len() as u32))
            .map_err(err)?;
        if h == 0 {
            return Err(self.out()?);
        }
        if self
            .f
            .vm_call
            .call(&mut self.store, (self.vm, h, 0, 0))
            .map_err(err)?
            != 0
        {
            return Err("vm_call".into());
        }
        let status = loop {
            let st = self
                .f
                .vm_run
                .call(&mut self.store, (self.vm, slice))
                .map_err(err)?;
            if st != OUT_OF_FUEL {
                break st;
            }
        };
        self.f
            .vm_handle_free
            .call(&mut self.store, (self.vm, h))
            .map_err(err)?;
        let r = self
            .f
            .vm_result
            .call(&mut self.store, self.vm)
            .map_err(err)?;
        self.f
            .vm_display
            .call(&mut self.store, (self.vm, r))
            .map_err(err)?;
        self.f
            .vm_handle_free
            .call(&mut self.store, (self.vm, r))
            .map_err(err)?;
        let text = self.out()?;
        match status {
            DONE => Ok(text),
            THREW => Err(text),
            other => Err(format!("status {other}")),
        }
    }
}

impl Dw {
    fn vm(&mut self) -> Result<S, String> {
        let mut store = Store::new(&self.engine, ());
        store.set_epoch_deadline(u64::MAX / 2);
        let instance: Instance = self.pre.instantiate(&mut store).map_err(err)?;
        macro_rules! get {
            ($($n:ident),*) => { F { $($n: instance.get_typed_func(&mut store, stringify!($n)).map_err(err)?,)* } };
        }
        let f = get!(
            vm_new,
            djs_staging,
            djs_out_ptr,
            djs_out_len,
            vm_compile,
            vm_call,
            vm_run,
            vm_result,
            vm_display,
            vm_handle_free
        );
        let mem = instance
            .get_memory(&mut store, "memory")
            .ok_or("no memory")?;
        // pi's provideBase, then rememberBase (js/src/base.ts), before the first vm_new.
        let u = |n: &str, store: &mut Store<()>| {
            instance.get_typed_func::<u32, u32>(store, n).map_err(err)
        };
        let mut injected = false;
        if let Some(base) = &self.base {
            let at = u("djs_base_inject_ptr", &mut store)?
                .call(&mut store, base.len() as u32)
                .map_err(err)? as usize;
            if at != 0 {
                mem.data_mut(&mut store)[at..at + base.len()].copy_from_slice(base);
                let inject = instance
                    .get_typed_func::<(), u32>(&mut store, "djs_base_inject")
                    .map_err(err)?;
                injected = inject.call(&mut store, ()).map_err(err)? == 1;
            }
        }
        if !injected {
            let at = u("djs_base_coded_ptr", &mut store)?
                .call(&mut store, self.coded.len() as u32)
                .map_err(err)? as usize;
            if at != 0 {
                mem.data_mut(&mut store)[at..at + self.coded.len()].copy_from_slice(&self.coded);
            }
        }
        if self.base.is_none() {
            let ptr = instance
                .get_typed_func::<(), u32>(&mut store, "djs_base_ptr")
                .map_err(err)?
                .call(&mut store, ())
                .map_err(err)? as usize;
            let len = instance
                .get_typed_func::<(), u32>(&mut store, "djs_base_len")
                .map_err(err)?
                .call(&mut store, ())
                .map_err(err)? as usize;
            if ptr == 0 {
                return Err("no djs_base".into());
            }
            self.base = Some(mem.data(&store)[ptr..ptr + len].to_vec());
        }
        let vm = f.vm_new.call(&mut store, 256 << 20).map_err(err)?;
        Ok(S { store, mem, vm, f })
    }

    fn plain(&mut self, src: &str, slice: u32) -> RunResult {
        let mut s = self.vm()?;
        s.eval("globalThis.__o = []; globalThis.console = { log: (...a) => { __o.push(a.map(String).join(' ')); } };", slice)?;
        s.eval(src, slice)?;
        let out = s.eval("__o.join('\\n')", slice)?;
        Ok(if out.is_empty() { out } else { out + "\n" })
    }
}

impl Engine for Dw {
    type S = S;
    const NAME: &'static str = "djs-wasm";
    const LANG: Lang = Lang::Js;

    fn new_runtime() -> Self {
        let mut c = Config::new();
        c.epoch_interruption(true);
        c.max_wasm_stack(4 << 20);
        c.async_stack_size(5 << 20);
        let engine = WEngine::new(&c).unwrap_or_else(|e| common::fail(&e.to_string()));
        let p = path();
        let bytes = std::fs::read(&p).unwrap_or_else(|e| common::fail(&e.to_string()));
        let coded = custom_section(&bytes, "djs_base")
            .unwrap_or_else(|| common::fail("no djs_base section"));
        let cache = p.with_extension(format!("{}.cwasm", common::zstd1(&bytes)));
        if !cache.exists() {
            let c = engine
                .precompile_module(&bytes)
                .unwrap_or_else(|e| common::fail(&e.to_string()));
            std::fs::write(&cache, c).unwrap_or_else(|e| common::fail(&e.to_string()));
        }
        // SAFETY: the file is the module this host precompiled above.
        let module = unsafe { Module::deserialize_file(&engine, &cache) }
            .unwrap_or_else(|e| common::fail(&e.to_string()));
        let mut linker: Linker<()> = Linker::new(&engine);
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
        Dw {
            engine,
            pre,
            slice: FUEL_PER_POLL,
            ticker,
            coded,
            base: None,
        }
    }

    fn new_session(&mut self) -> S {
        self.vm().unwrap_or_else(|e| common::fail(&e))
    }

    /// pi's prelude, with a no-op JS `bridge` (setup never calls it).
    fn setup(&mut self, s: &mut S, tools: &[ToolDef]) {
        let tools = Json::Array(
            tools
                .iter()
                .map(|t| json!({"name": t.name, "jsName": t.name, "description": t.description}))
                .collect(),
        );
        let src = format!(
            "globalThis.__api = ({PRELUDE})(() => undefined, {}, '[]', '{{}}'); 0",
            Json::String(tools.to_string())
        );
        s.eval(&src, self.slice)
            .unwrap_or_else(|e| common::fail(&e));
    }

    fn run(&mut self, s: &mut S, src: &str, _host: &mut dyn Host) -> RunResult {
        // The start-up measurement's first cell only.
        s.eval(
            src.trim_start_matches("console.log(")
                .trim_end_matches(");"),
            self.slice,
        )
        .map(|o| o + "\n")
    }

    fn run_plain(&mut self, src: &str) -> RunResult {
        self.plain(src, u32::MAX)
    }

    fn run_polled(&mut self, src: &str) -> RunResult {
        self.plain(src, self.slice)
    }

    fn session_bytes(&mut self, s: &mut S) -> Option<u64> {
        Some(s.mem.data_size(&s.store) as u64)
    }

    fn control(&mut self, _s: &mut S, _src: &str, _stop: &Stop, _delay: Duration) -> Control {
        common::fail("djs-wasm: control is measured on the native build")
    }

    fn run_until(
        &mut self,
        _s: &mut S,
        _src: &str,
        _host: &mut dyn Host,
        _stop_after: usize,
    ) -> bool {
        false
    }

    fn finish(&mut self, _s: &mut S, _host: &mut dyn Host) -> RunResult {
        Err("unsupported".into())
    }

    fn describe() -> Json {
        json!({"module": path().display().to_string(), "language": "Rust (no_std) in wasm32-unknown-unknown, wasmtime host",
            "fuel_per_poll": FUEL_PER_POLL})
    }
}

impl Drop for Dw {
    fn drop(&mut self) {
        self.ticker.store(true, Ordering::Relaxed);
    }
}

fn main() {
    common::main::<Dw>();
}
