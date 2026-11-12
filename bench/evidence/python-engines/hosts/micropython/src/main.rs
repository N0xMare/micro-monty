//! MicroPython 1.29.0 (unix port, `sandbox` variant: build.sh) under the
//! shared engine harness (bench/evidence/engines/hosts/common).
//!
//! MicroPython has one interpreter per process (`mp_state_ctx` is global), so
//! a session is a fresh globals dict in it (shim.c). The heap is one GC
//! arena of 256 MiB, mm's default memory limit; past it MicroPython raises
//! `MemoryError`. The C stack check allows 1 MiB.
//!
//! Codemode setup: each tool is `async def name(arguments=None, **kw)` around
//! the native `_hc(name, json)`, which asks the host synchronously: the VM
//! cannot hand control back with a call outstanding, so `asyncio.gather`
//! over tools makes its calls one after another. Arguments and results
//! cross as JSON text (pi's bridge does the same). `hsync` is a native
//! function. `asyncio.sleep` is virtual (it yields once and returns), as
//! mm's and the codemode shims' are. A stop is `mp_sched_keyboard_interrupt` from another thread: a
//! `KeyboardInterrupt` at the VM's next check, which ends the run.

#![allow(unsafe_code)]

use common::{Answer, Control, Engine, Host, Lang, RunResult, Stop, ToolDef, Value, json};
use std::cell::RefCell;
use std::ffi::c_char;
use std::time::Duration;

type CallFn = extern "C" fn(*const c_char, usize, *const c_char, usize, *mut usize, *mut i32) -> *const c_char;
type SyncFn = extern "C" fn(i64) -> i64;
type OutFn = extern "C" fn(*const c_char, usize);

unsafe extern "C" {
    fn mpy_init(heap: usize, stack: usize, call: CallFn, sync: SyncFn, out: OutFn);
    fn mpy_new_session() -> i64;
    fn mpy_drop_session(id: i64);
    fn mpy_exec(id: i64, src: *const c_char, len: usize, err: *mut *const c_char, err_len: *mut usize) -> i32;
    fn mpy_interrupt();
    fn mpy_heap_used() -> usize;
}

const HEAP: usize = 256 << 20;
const STACK: usize = 1 << 20;

thread_local! {
    static OUT: RefCell<String> = const { RefCell::new(String::new()) };
    static HOST: RefCell<Option<*mut (dyn Host + 'static)>> = const { RefCell::new(None) };
    static REPLY: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

extern "C" fn on_out(s: *const c_char, len: usize) {
    // SAFETY: shim.c passes a valid buffer of `len` bytes.
    let b = unsafe { std::slice::from_raw_parts(s.cast::<u8>(), len) };
    OUT.with(|o| o.borrow_mut().push_str(&String::from_utf8_lossy(b)));
}

extern "C" fn on_sync(x: i64) -> i64 {
    common::mark();
    x
}

extern "C" fn on_call(
    name: *const c_char,
    nl: usize,
    args: *const c_char,
    al: usize,
    out_len: *mut usize,
    is_err: *mut i32,
) -> *const c_char {
    // SAFETY: shim.c passes valid buffers and out-pointers.
    let (name, args) = unsafe {
        (
            String::from_utf8_lossy(std::slice::from_raw_parts(name.cast::<u8>(), nl)).into_owned(),
            std::slice::from_raw_parts(args.cast::<u8>(), al),
        )
    };
    let args: Value = common::serde_json::from_slice(args).unwrap_or(Value::Null);
    let (bytes, err) = HOST.with(|h| {
        let Some(host) = *h.borrow() else {
            return (b"RuntimeError\0no host".to_vec(), true);
        };
        // SAFETY: `run` sets the pointer for the duration of the script.
        match unsafe { (*host).call(&name, args) } {
            Answer::Ok(v) => (v.to_string().into_bytes(), false),
            Answer::Err(t, m) => (format!("{t}\0{m}").into_bytes(), true),
        }
    });
    REPLY.with(|r| {
        let mut r = r.borrow_mut();
        *r = bytes;
        r.push(0);
        // SAFETY: out-pointers from shim.c.
        unsafe {
            *out_len = r.len() - 1;
            *is_err = i32::from(err);
        }
        r.as_ptr().cast()
    })
}

struct Mpy;

struct S {
    id: i64,
}

impl Drop for S {
    fn drop(&mut self) {
        // SAFETY: drops the session dict from the GC root.
        unsafe { mpy_drop_session(self.id) }
    }
}

/// A cell with top-level `await` runs as a coroutine (MicroPython compiles
/// none at module level). The codemode scripts arrive already wrapped with
/// their globals (run.py); this covers the harness's own one-liners.
fn wrap_await(src: &str) -> String {
    if !src.lines().any(|l| l.starts_with("await ") || l.contains(" await ")) || src.contains("async def __codemode_main") {
        return src.to_owned();
    }
    let body: String = src.lines().map(|l| format!("    {l}\n")).collect();
    format!("import asyncio as __a\nasync def __cell():\n{body}    pass\n__a.run(__cell())\n")
}

fn exec(s: &S, src: &str, host: Option<&mut dyn Host>) -> RunResult {
    OUT.with(|o| o.borrow_mut().clear());
    // SAFETY: the pointer is cleared before `host` goes out of scope.
    let ptr = host.map(|h| unsafe {
        std::mem::transmute::<*mut (dyn Host + '_), *mut (dyn Host + 'static)>(h as *mut dyn Host)
    });
    HOST.with(|h| *h.borrow_mut() = ptr);
    let src = wrap_await(src);
    let (mut err, mut err_len) = (std::ptr::null(), 0usize);
    // SAFETY: valid buffers; `err` stays valid until the next exec.
    let rc = unsafe { mpy_exec(s.id, src.as_ptr().cast(), src.len(), &mut err, &mut err_len) };
    HOST.with(|h| *h.borrow_mut() = None);
    if rc != 0 {
        // SAFETY: shim.c returns `err_len` bytes.
        let e = unsafe { std::slice::from_raw_parts(err.cast::<u8>(), err_len) };
        return Err(String::from_utf8_lossy(e).into_owned());
    }
    Ok(OUT.with(|o| o.borrow().clone()))
}

fn prelude(tools: &[ToolDef]) -> String {
    let mut p = String::from(
        "import json as _json\nimport builtins as _b\nimport asyncio as _aio\n\
         if not hasattr(_aio, '_real_sleep'):\n\
         \x20   _aio._real_sleep = _aio.sleep\n\
         \x20   _aio.sleep = lambda t, r=None: _aio._real_sleep(0)\n\
         for _n in ('TimeoutError', 'PermissionError', 'FileNotFoundError', 'ConnectionError'):\n\
         \x20   if not hasattr(_b, _n):\n\
         \x20       globals()[_n] = type(_n, (OSError,), {})\n\
         def _exc(kind):\n\
         \x20   return globals().get(kind) or getattr(_b, kind, RuntimeError)\n\
         def _mk(name):\n\
         \x20   async def tool(arguments=None, **kw):\n\
         \x20       if kw:\n\
         \x20           a = dict(arguments or {})\n\
         \x20           a.update(kw)\n\
         \x20           arguments = a\n\
         \x20       r = _hc(name, _json.dumps({} if arguments is None else arguments))\n\
         \x20       if r[0]:\n\
         \x20           return _json.loads(r[1])\n\
         \x20       raise _exc(r[1])(r[2])\n\
         \x20   return tool\n",
    );
    for t in tools {
        p.push_str(&format!("{0} = _mk('{0}')\n", t.name));
    }
    p
}

impl Engine for Mpy {
    type S = S;
    const NAME: &'static str = "micropython";
    const LANG: Lang = Lang::Py;

    fn new_runtime() -> Self {
        // SAFETY: once per process, before any other shim call.
        unsafe { mpy_init(HEAP, STACK, on_call, on_sync, on_out) };
        Mpy
    }

    fn new_session(&mut self) -> S {
        // SAFETY: after mpy_init.
        S { id: unsafe { mpy_new_session() } }
    }

    fn setup(&mut self, s: &mut S, tools: &[ToolDef]) {
        exec(s, &prelude(tools), None).unwrap_or_else(|e| common::fail(&e));
    }

    fn run(&mut self, s: &mut S, src: &str, host: &mut dyn Host) -> RunResult {
        exec(s, src, Some(host))
    }

    fn run_plain(&mut self, src: &str) -> RunResult {
        let s = self.new_session();
        exec(&s, src, None)
    }

    fn session_bytes(&mut self, _s: &mut S) -> Option<u64> {
        None
    }

    fn control(&mut self, s: &mut S, src: &str, stop: &Stop, delay: Duration) -> Control {
        // SAFETY: mp_sched_keyboard_interrupt is MicroPython's async-safe stop.
        stop.request_after(delay, || unsafe { mpy_interrupt() });
        let r = exec(s, src, None);
        let back_ns = common::now_ns();
        let how = match &r {
            Err(e) if e.contains("KeyboardInterrupt") => "interrupted".to_owned(),
            Err(e) => format!("error: {}", e.lines().last().unwrap_or_default()),
            Ok(_) => "finished".to_owned(),
        };
        let probe = exec(s, common::probe_script(Lang::Py), None);
        Control {
            how,
            resumable: false,
            session_usable: probe.as_deref() == Ok("42\n"),
            back_ns,
        }
    }

    fn run_polled(&mut self, src: &str) -> RunResult {
        // The pending-exception check is always compiled in and always polled.
        self.run_plain(src)
    }

    fn run_until(&mut self, _s: &mut S, _src: &str, _host: &mut dyn Host, _n: usize) -> bool {
        false
    }

    fn finish(&mut self, _s: &mut S, _host: &mut dyn Host) -> RunResult {
        Err("no resumable runs".into())
    }

    fn describe() -> Value {
        json!({"engine": "MicroPython 1.29.0 unix port, sandbox variant", "heap": HEAP, "stack": STACK})
    }
}

fn main() {
    // The GC heap in use (after a collection), for `mem`: printed on request.
    if std::env::args().nth(1).as_deref() == Some("heap") {
        let mut e = Mpy::new_runtime();
        let n: usize = std::env::args().nth(2).and_then(|a| a.parse().ok()).unwrap_or(1);
        let fixture = common::Fixture::load(&std::env::args().nth(3).unwrap_or_default());
        // SAFETY: after mpy_init.
        let base = unsafe { mpy_heap_used() };
        let mut keep = Vec::new();
        for _ in 0..n {
            let mut s = e.new_session();
            e.setup(&mut s, &fixture.tools);
            e.run(&mut s, common::first_cell(Lang::Py), &mut common::Noop::default())
                .unwrap_or_else(|e| common::fail(&e));
            keep.push(s);
        }
        // SAFETY: as above.
        let used = unsafe { mpy_heap_used() };
        common::emit(json!({"engine": "micropython", "cmd": "heap", "n": n, "base": base, "used": used}));
        return;
    }
    common::main::<Mpy>();
}
