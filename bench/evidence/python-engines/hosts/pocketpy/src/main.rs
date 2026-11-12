//! PocketPy 2.2.0 under the shared engine harness
//! (bench/evidence/engines/hosts/common). A session is one PocketPy VM
//! (`py_switchvm`; at most 16 per process, reset with `py_resetvm` on reuse).
//! PocketPy has no memory limit and no `async`/`await`: tools are plain
//! functions around the native `_hc(name, json)`, answered synchronously,
//! with arguments and results as JSON text. A stop fires PocketPy's watchdog
//! (a `TimeoutError` at the VM's next bytecode step), the only stop it has.

#![allow(unsafe_code)]

use common::{Answer, Control, Engine, Host, Lang, RunResult, Stop, ToolDef, Value, json};
use std::cell::RefCell;
use std::ffi::c_char;
use std::time::Duration;

type CallFn = extern "C" fn(*const c_char, usize, *const c_char, usize, *mut usize, *mut i32) -> *const c_char;
type SyncFn = extern "C" fn(i64) -> i64;
type OutFn = extern "C" fn(*const c_char, usize);

unsafe extern "C" {
    fn host_pk_init(call: CallFn, sync: SyncFn, out: OutFn);
    fn host_pk_new_session() -> i32;
    fn host_pk_drop_session(i: i32);
    fn host_pk_exec(i: i32, src: *const c_char, err: *mut *const c_char) -> i32;
    fn host_pk_watchdog(i: i32, ms: i64);
    fn host_pk_interrupt();
}

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

struct Pk;

struct S {
    vm: i32,
}

impl Drop for S {
    fn drop(&mut self) {
        // SAFETY: marks the VM for reuse (reset on its next use).
        unsafe { host_pk_drop_session(self.vm) }
    }
}

fn exec(s: &S, src: &str, host: Option<&mut dyn Host>) -> RunResult {
    OUT.with(|o| o.borrow_mut().clear());
    // SAFETY: the pointer is cleared before `host` goes out of scope.
    let ptr = host.map(|h| unsafe {
        std::mem::transmute::<*mut (dyn Host + '_), *mut (dyn Host + 'static)>(h as *mut dyn Host)
    });
    HOST.with(|h| *h.borrow_mut() = ptr);
    let c = std::ffi::CString::new(src).unwrap_or_default();
    let mut err = std::ptr::null();
    // SAFETY: a NUL-terminated source; `err` stays valid until the next exec.
    let rc = unsafe { host_pk_exec(s.vm, c.as_ptr(), &mut err) };
    HOST.with(|h| *h.borrow_mut() = None);
    if rc != 0 {
        // SAFETY: shim.c returns a NUL-terminated string.
        return Err(unsafe { std::ffi::CStr::from_ptr(err) }.to_string_lossy().into_owned());
    }
    Ok(OUT.with(|o| o.borrow().clone()))
}

/// Tools are plain functions: PocketPy has no `async`/`await`.
fn prelude(tools: &[ToolDef]) -> String {
    let mut p = String::from(
        "import json as _json\nimport builtins as _b\n\
         class FileNotFoundError(Exception):\n    pass\n\
         class ConnectionError(Exception):\n    pass\n\
         class LookupError(Exception):\n    pass\n\
         def _exc(kind):\n\
         \x20   g = globals()\n\
         \x20   if kind in g:\n\
         \x20       return g[kind]\n\
         \x20   return getattr(_b, kind, RuntimeError)\n\
         def _mk(name):\n\
         \x20   def tool(arguments=None, **kw):\n\
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

impl Engine for Pk {
    type S = S;
    const NAME: &'static str = "pocketpy";
    const LANG: Lang = Lang::Py;

    fn new_runtime() -> Self {
        // SAFETY: once per process.
        unsafe { host_pk_init(on_call, on_sync, on_out) };
        Pk
    }

    fn new_session(&mut self) -> S {
        // SAFETY: after host_pk_init.
        let vm = unsafe { host_pk_new_session() };
        if vm < 0 {
            common::fail("PocketPy allows at most 16 VMs per process");
        }
        S { vm }
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

    fn control(&mut self, s: &mut S, src: &str, stop: &Stop, delay: Duration) -> Control {
        // SAFETY: see shim.c host_pk_interrupt.
        stop.request_after(delay, || unsafe { host_pk_interrupt() });
        let r = exec(s, src, None);
        let back_ns = common::now_ns();
        let how = match &r {
            Err(e) if e.contains("watchdog") => "interrupted".to_owned(),
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
        let s = self.new_session();
        // SAFETY: arms the watchdog far in the future, then disarms it.
        unsafe { host_pk_watchdog(s.vm, 1 << 40) };
        let r = exec(&s, src, None);
        unsafe { host_pk_watchdog(s.vm, -1) };
        r
    }

    fn run_until(&mut self, _s: &mut S, _src: &str, _host: &mut dyn Host, _n: usize) -> bool {
        false
    }

    fn finish(&mut self, _s: &mut S, _host: &mut dyn Host) -> RunResult {
        Err("no resumable runs".into())
    }

    fn describe() -> Value {
        json!({"engine": "PocketPy 2.2.0, PK_ENABLE_OS=0, PK_ENABLE_WATCHDOG=1, -O2"})
    }
}

fn main() {
    common::main::<Pk>();
}
