//! What every engine host shares: the command line, the mocked tools (replay
//! tables recorded by `record.py`), the scripts of each measurement in both
//! languages, clocks, RSS and the JSON records the hosts print.
//!
//! An engine host implements [`Engine`] and calls [`main`]. Every command
//! prints JSON lines on stdout; `run.py` collects them.

use std::collections::{HashMap, VecDeque};
use std::fmt::Write as _;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub use serde_json::{self, Value, json};

/// The language an engine runs.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Js,
    Py,
}

/// A tool as a codemode setup declares it.
#[derive(Clone, Debug)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub params: Vec<String>,
}

/// What a mocked tool answers.
pub enum Answer<'a> {
    Ok(&'a Value),
    /// A failure: the exception type the Python mock raised, and its message.
    Err(&'a str, &'a str),
}

/// The tool side of a codemode run.
pub trait Host {
    fn call(&mut self, name: &str, args: Value) -> Answer<'_>;
}

/// One task's recording: the tools and every call with its reply.
pub struct Fixture {
    pub task: String,
    pub tools: Vec<ToolDef>,
    replies: Vec<(String, Value, Result<Value, (String, String)>)>,
}

impl Fixture {
    pub fn load(path: &str) -> Fixture {
        let text = std::fs::read_to_string(path).unwrap_or_else(|e| fail(&format!("{path}: {e}")));
        let v: Value =
            serde_json::from_str(&text).unwrap_or_else(|e| fail(&format!("{path}: {e}")));
        let tools = v["tools"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|t| ToolDef {
                name: t["name"].as_str().unwrap_or_default().to_owned(),
                description: t["description"].as_str().unwrap_or_default().to_owned(),
                params: t["params"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|p| p.as_str().map(str::to_owned))
                    .collect(),
            })
            .collect();
        let replies = v["calls"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|c| {
                let reply = match c.get("err") {
                    Some(e) => Err((
                        e[0].as_str().unwrap_or_default().to_owned(),
                        e[1].as_str().unwrap_or_default().to_owned(),
                    )),
                    None => Ok(c["ok"].clone()),
                };
                (
                    c["name"].as_str().unwrap_or_default().to_owned(),
                    c["args"].clone(),
                    reply,
                )
            })
            .collect();
        Fixture {
            task: v["task"].as_str().unwrap_or_default().to_owned(),
            tools,
            replies,
        }
    }
}

/// Answers a run's calls from a [`Fixture`], keyed by tool name and canonical
/// arguments; repeated identical calls get the recorded replies in order
/// (the retry tools). `submit_answer` always answers "ok" (the checker judges
/// its arguments afterwards, with the task's float tolerance).
pub struct Replay<'a> {
    fixture: &'a Fixture,
    table: HashMap<String, VecDeque<usize>>,
    pub calls: Vec<(String, Value)>,
    pub misses: Vec<String>,
}

static OK: Value = Value::Null;

impl<'a> Replay<'a> {
    pub fn new(fixture: &'a Fixture) -> Replay<'a> {
        let mut table: HashMap<String, VecDeque<usize>> = HashMap::new();
        for (i, (name, args, _)) in fixture.replies.iter().enumerate() {
            table.entry(key(name, args)).or_default().push_back(i);
        }
        Replay {
            fixture,
            table,
            calls: Vec::new(),
            misses: Vec::new(),
        }
    }
}

impl Host for Replay<'_> {
    fn call(&mut self, name: &str, args: Value) -> Answer<'_> {
        let k = key(name, &args);
        self.calls.push((name.to_owned(), args));
        match self.table.get_mut(&k).and_then(VecDeque::pop_front) {
            Some(i) => match &self.fixture.replies[i].2 {
                Ok(v) => Answer::Ok(v),
                Err((t, m)) => Answer::Err(t, m),
            },
            None if name == "submit_answer" => Answer::Ok(submit_ok()),
            None => {
                self.misses.push(k);
                Answer::Err("RuntimeError", "no recorded reply for this call")
            }
        }
    }
}

fn submit_ok() -> &'static Value {
    static V: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
    let _ = &OK;
    V.get_or_init(|| Value::String("ok".into()))
}

/// The trivial mocked tool of the host-call measurements: every call answers
/// the same small object.
pub struct Noop {
    pub calls: u64,
    reply: Value,
}

impl Default for Noop {
    fn default() -> Self {
        Noop {
            calls: 0,
            reply: json!({"ok": true}),
        }
    }
}

impl Host for Noop {
    fn call(&mut self, _name: &str, _args: Value) -> Answer<'_> {
        self.calls += 1;
        Answer::Ok(&self.reply)
    }
}

/// `name` and the arguments as canonical JSON: keys sorted, integral floats
/// written as integers (Python's `1.0` and JavaScript's `1` are one key).
pub fn key(name: &str, args: &Value) -> String {
    let mut s = String::with_capacity(64);
    s.push_str(name);
    s.push(' ');
    canon(args, &mut s);
    s
}

fn canon(v: &Value, out: &mut String) {
    match v {
        Value::Object(m) => {
            let mut keys: Vec<&String> = m.keys().collect();
            keys.sort();
            out.push('{');
            for (i, k) in keys.into_iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&Value::String(k.clone()).to_string());
                out.push(':');
                canon(&m[k], out);
            }
            out.push('}');
        }
        Value::Array(a) => {
            out.push('[');
            for (i, x) in a.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                canon(x, out);
            }
            out.push(']');
        }
        Value::Number(n) => match n.as_f64() {
            Some(f) if n.is_f64() && f.fract() == 0.0 && f.abs() < 9.0e15 => {
                let _ = write!(out, "{}", f as i64);
            }
            _ => {
                let _ = write!(out, "{n}");
            }
        },
        other => out.push_str(&other.to_string()),
    }
}

/// The tools of the host-call measurements.
pub fn noop_tools() -> Vec<ToolDef> {
    vec![ToolDef {
        name: "noop".into(),
        description: "Does nothing.".into(),
        params: vec!["i".into()],
    }]
}

/// The outcome of one codemode run: printed output, or the error.
pub type RunResult = Result<String, String>;

/// A stop request from another thread: the time it was made, in ns since
/// [`epoch`], or 0.
#[derive(Clone, Default)]
pub struct Stop(pub Arc<AtomicU64>);

impl Stop {
    pub fn requested(&self) -> bool {
        self.0.load(Ordering::Relaxed) != 0
    }
    pub fn at(&self) -> u64 {
        self.0.load(Ordering::SeqCst)
    }
    pub fn clear(&self) {
        self.0.store(0, Ordering::SeqCst);
    }
    /// Requests a stop from a new thread `after` the script's mark (its
    /// `hsync` call just before the operation under test); the requester
    /// also calls `extra` (an engine's own park or interrupt handle).
    pub fn request_after(&self, after: Duration, extra: impl FnOnce() + Send + 'static) {
        let flag = Arc::clone(&self.0);
        std::thread::spawn(move || {
            while MARK.load(Ordering::SeqCst) == 0 {
                std::thread::sleep(Duration::from_micros(200));
            }
            std::thread::sleep(after);
            flag.store(now_ns().max(1), Ordering::SeqCst);
            extra();
        });
    }
}

static MARK: AtomicU64 = AtomicU64::new(0);

/// Set while `gcpause` records a timestamp at every `hsync`.
static RECORD: AtomicBool = AtomicBool::new(false);
static TICKS: Mutex<Vec<u64>> = Mutex::new(Vec::new());

/// Called by every engine's `hsync`: marks the start of a control target's
/// operation, and records the time for `gcpause`.
pub fn mark() {
    if RECORD.load(Ordering::Relaxed) {
        let t = now_ns();
        TICKS.lock().unwrap_or_else(std::sync::PoisonError::into_inner).push(t);
    }
    let _ = MARK.compare_exchange(0, now_ns().max(1), Ordering::SeqCst, Ordering::SeqCst);
}

/// The `gcpause` script: builds a live set of `live` one-item lists, then
/// allocates `garbage` more one-item lists (each a self-cycle if `cyclic`),
/// then drops the live set (`live = None`: Monty has no `del`). `hsync` every 1,000 allocations gives the host a
/// timestamp, so the longest gap is the longest stall: a collection, or a
/// refcount cascade, included.
pub fn gcpause_script(live: u64, garbage: u64, cyclic: bool) -> String {
    let make = if cyclic { "x = [j]\n        x.append(x)" } else { "x = [j]" };
    format!(
        "live = []\nfor i in range({live}):\n    live.append([i])\n    if i % 1000 == 999:\n        hsync(1)\nhsync(2)\n\
         for r in range({rounds}):\n    for j in range(1000):\n        {make}\n    hsync(3)\n\
         live = None\nx = None\nhsync(4)\nprint('ok')\n",
        rounds = garbage / 1000
    )
}

/// Median, 99th percentile and maximum of the gaps between `ticks`, in ms.
fn gap_stats(ticks: &[u64]) -> Value {
    let mut g: Vec<u64> = ticks.windows(2).map(|w| w[1] - w[0]).collect();
    if g.is_empty() {
        return Value::Null;
    }
    g.sort_unstable();
    let ms = |ns: u64| ns as f64 / 1e6;
    let at = |q: f64| g[((g.len() - 1) as f64 * q).round() as usize];
    json!({"n": g.len(), "median_ms": ms(at(0.5)), "p99_ms": ms(at(0.99)), "max_ms": ms(g[g.len() - 1]),
        "over_1ms": g.iter().filter(|&&x| x > 1_000_000).count(),
        "over_10ms": g.iter().filter(|&&x| x > 10_000_000).count()})
}

/// How long after the mark the stop is requested, per control target.
pub fn control_delay(kind: &str) -> Duration {
    Duration::from_millis(match kind {
        "string" => 10,
        "loop" => 50,
        _ => 100,
    })
}

/// What a control measurement reports.
pub struct Control {
    /// How control came back: "parked", "interrupted", "finished", "trap", ...
    pub how: String,
    /// The stopped run can continue where it stopped.
    pub resumable: bool,
    /// The session (or instance) still runs a new script afterwards.
    pub session_usable: bool,
    /// [`now_ns`] when control came back (before any follow-up checks).
    pub back_ns: u64,
}

/// Nanoseconds since the first call in this process.
pub fn now_ns() -> u64 {
    static BASE: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    let base = BASE.get_or_init(Instant::now);
    u64::try_from(base.elapsed().as_nanos()).unwrap_or(u64::MAX)
}

#[repr(C)]
#[derive(Default)]
struct Mallinfo2 {
    arena: usize,
    ordblks: usize,
    smblks: usize,
    hblks: usize,
    hblkhd: usize,
    usmblks: usize,
    fsmblks: usize,
    uordblks: usize,
    fordblks: usize,
    keepcost: usize,
}

unsafe extern "C" {
    fn mallinfo2() -> Mallinfo2;
}

/// Bytes allocated through malloc and not freed (glibc `mallinfo2`: in-use
/// arena chunks plus mmapped blocks). Rust's default allocator and the C
/// engines both allocate through malloc, so it counts every engine alike.
pub fn heap_in_use() -> u64 {
    // SAFETY: mallinfo2 only reads the allocator's statistics.
    let m = unsafe { mallinfo2() };
    (m.uordblks + m.hblkhd) as u64
}

/// The process's resident set size and its peak, in bytes.
pub fn rss() -> (u64, u64) {
    let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    let field = |name: &str| {
        status
            .lines()
            .find_map(|l| l.strip_prefix(name))
            .and_then(|r| r.split_whitespace().next())
            .and_then(|n| n.parse::<u64>().ok())
            .map_or(0, |kb| kb * 1024)
    };
    (field("VmRSS:"), field("VmHWM:"))
}

pub fn zstd1(bytes: &[u8]) -> usize {
    zstd::bulk::compress(bytes, 1).map_or(0, |c| c.len())
}

pub fn median(v: &mut [f64]) -> f64 {
    if v.is_empty() {
        return f64::NAN;
    }
    v.sort_by(f64::total_cmp);
    let n = v.len();
    if n % 2 == 1 {
        v[n / 2]
    } else {
        (v[n / 2 - 1] + v[n / 2]) / 2.0
    }
}

pub fn fail(msg: &str) -> ! {
    eprintln!("error: {msg}");
    std::process::exit(1)
}

pub fn emit(v: Value) {
    println!("{v}");
}

/// Times `f` in ns.
pub fn timed<R>(f: impl FnOnce() -> R) -> (R, f64) {
    let t = Instant::now();
    let r = f();
    (r, t.elapsed().as_nanos() as f64)
}

// ---------------------------------------------------------------------------
// Scripts

/// The sync host-call loop: `hsync` is a host function returning its argument;
/// `local` swaps in a function of the language itself (the baseline).
pub fn sync_script(lang: Lang, n: u64, local: bool) -> String {
    match (lang, local) {
        (Lang::Js, false) => {
            format!("let s = 0; for (let i = 0; i < {n}; i++) s += hsync(i); console.log(s);")
        }
        (Lang::Js, true) => format!(
            "function f(x) {{ return x; }} let s = 0; for (let i = 0; i < {n}; i++) s += f(i); console.log(s);"
        ),
        (Lang::Py, false) => format!("s = 0\nfor i in range({n}):\n    s += hsync(i)\nprint(s)\n"),
        (Lang::Py, true) => {
            format!(
                "def f(x):\n    return x\ns = 0\nfor i in range({n}):\n    s += f(i)\nprint(s)\n"
            )
        }
    }
}

/// `n` sequential awaited calls of the `noop` tool, or of a local async
/// function (the baseline).
pub fn await_script(lang: Lang, n: u64, local: bool) -> String {
    match (lang, local) {
        (Lang::Js, false) => format!("for (let i = 0; i < {n}; i++) await tools.noop({{ i }});"),
        (Lang::Js, true) => {
            format!("const f = async (a) => a; for (let i = 0; i < {n}; i++) await f({{ i }});")
        }
        (Lang::Py, false) => format!("for i in range({n}):\n    await noop({{'i': i}})\n"),
        (Lang::Py, true) => {
            format!(
                "async def f(a):\n    return a\nfor i in range({n}):\n    await f({{'i': i}})\n"
            )
        }
    }
}

/// `rounds` fan-outs of 100 awaited calls: `Promise.all` / `asyncio.gather`.
pub fn fanout_script(lang: Lang, rounds: u64, local: bool) -> String {
    match (lang, local) {
        (Lang::Js, false) => format!(
            "for (let r = 0; r < {rounds}; r++) await Promise.all(Array.from({{ length: 100 }}, (_, i) => tools.noop({{ i }})));"
        ),
        (Lang::Js, true) => format!(
            "const f = async (a) => a; for (let r = 0; r < {rounds}; r++) await Promise.all(Array.from({{ length: 100 }}, (_, i) => f({{ i }})));"
        ),
        (Lang::Py, false) => format!(
            "import asyncio\nfor r in range({rounds}):\n    await asyncio.gather(*[noop({{'i': i}}) for i in range(100)])\n"
        ),
        (Lang::Py, true) => format!(
            "import asyncio\nasync def f(a):\n    return a\nfor r in range({rounds}):\n    await asyncio.gather(*[f({{'i': i}}) for i in range(100)])\n"
        ),
    }
}

/// The control targets: a busy loop, and two long native operations (a sort
/// of 1M strings without a key or comparator, and joining a 1M-item list of
/// strings into one ~64 MB string, repeated) after a short warm-up.
pub fn control_script(lang: Lang, kind: &str) -> String {
    // `catch`/`catch-all`: a busy loop inside `try` whose handler
    // (`except Exception` / bare `except`) restarts it, as model-written
    // retry code does: does the stop survive the script's own handler?
    match (lang, kind) {
        (Lang::Py, "catch" | "catch-all") => {
            let handler = if kind == "catch" {
                "except Exception:"
            } else {
                "except:"
            };
            return format!(
                "n = 0\nhsync(0)\nwhile True:\n    try:\n        while True:\n            n += 1\n    {handler}\n        pass\n"
            );
        }
        (Lang::Js, "catch" | "catch-all") => {
            return "let n = 0; hsync(0); while (true) { try { while (true) n++; } catch (e) {} }"
                .into();
        }
        _ => {}
    }
    let (setup, op) = match (lang, kind.trim_end_matches("-loop")) {
        // One big-int multiply of two 1M-bit numbers.
        (Lang::Py, "bigint") => ("x = (1 << 1000000) - 1", "x * x"),
        (Lang::Js, "bigint") => ("const x = (1n << 1000000n) - 1n;", "x * x;"),
        (Lang::Js, "loop") => ("let n = 0;", "n++;"),
        (Lang::Py, "loop") => ("n = 0", "n += 1"),
        (Lang::Js, "sort") => (
            "const xs = []; for (let i = 0; i < 1000000; i++) xs.push(String((i * 7919) % 1000003));",
            "xs.slice().sort();",
        ),
        (Lang::Py, "sort") => (
            "xs = [str((i * 7919) % 1000003) for i in range(1000000)]",
            "sorted(xs)",
        ),
        (Lang::Js, "string") => (
            "const part = 'x'.repeat(64); const xs = []; for (let i = 0; i < 1000000; i++) xs.push(part);",
            "xs.join('-');",
        ),
        (Lang::Py, "string") => ("part = 'x' * 64\nxs = [part] * 1000000", "'-'.join(xs)"),
        _ => fail(&format!("unknown control kind {kind}")),
    };
    // `kind`: one operation, then a busy loop; `kind-loop`: the operation
    // over and over.
    let repeat = kind.ends_with("-loop") || kind == "loop";
    match (lang, repeat) {
        (Lang::Js, true) => format!("{setup} hsync(0); while (true) {{ {op} }}"),
        (Lang::Js, false) => format!("{setup} let n = 0; hsync(0); {op} while (true) {{ n++; }}"),
        (Lang::Py, true) => format!("{setup}\nhsync(0)\nwhile True:\n    {op}\n"),
        (Lang::Py, false) => format!("{setup}\nn = 0\nhsync(0)\n{op}\nwhile True:\n    n += 1\n"),
    }
}

/// A trivial script that a usable session finishes.
pub fn probe_script(lang: Lang) -> &'static str {
    match lang {
        Lang::Js => "console.log(6 * 7);",
        Lang::Py => "print(6 * 7)\n",
    }
}

/// The trivial first cell after a setup (start-up measurement).
pub fn first_cell(lang: Lang) -> &'static str {
    match lang {
        Lang::Js => "console.log(1);",
        Lang::Py => "print(1)\n",
    }
}

/// The `gc` command's scripts: `build` makes `n` live pairs `[i, str(i)]`
/// in `big`; the workloads run `n` iterations beside them: `garbage` (a
/// one-item list per iteration, dropped by the next), `cycles` (the same
/// list made to contain itself: garbage only a tracing pass frees, the
/// QuickJS cycle collector), `writes` (in-order writes of new strings into
/// the old pairs) and `appends` (new pairs kept: live data grows).
pub fn gc_script(lang: Lang, kind: &str, n: u64) -> String {
    let body = match (lang, kind) {
        (Lang::Js, "build") => {
            return format!(
                "var big = []; for (let i = 0; i < {n}; i++) big.push([i, String(i)]); var keep = []; var n = big.length; var y;"
            );
        }
        (Lang::Py, "build") => {
            return format!(
                "import gc\nbig = []\nfor i in range({n}):\n    big.append([i, str(i)])\nkeep = []\nn = len(big)\n"
            );
        }
        (Lang::Js, "garbage") => "y = [i];",
        (Lang::Py, "garbage") => "y = [i]",
        (Lang::Js, "cycles") => "y = [i]; y.push(y);",
        (Lang::Py, "cycles") => "y = [i]\n    y.append(y)",
        (Lang::Js, "writes") => "big[i % n][1] = String(i);",
        (Lang::Py, "writes") => "big[i % n][1] = str(i)",
        (Lang::Js, "appends") => "keep.push([i, String(i)]);",
        (Lang::Py, "appends") => "keep.append([i, str(i)])",
        _ => fail(&format!("unknown gc workload {kind}")),
    };
    match lang {
        Lang::Js => format!("for (let i = 0; i < {n}; i++) {{ {body} }}"),
        Lang::Py => format!("for i in range({n}):\n    {body}\n"),
    }
}

/// The gaps between an engine's control points during a `gc` workload: the
/// time from one point where the host could stop the run (a fuel park, an
/// interrupt poll, a slice end) to the next, in ns. A collection inside the
/// run lengthens the gap it falls in.
#[derive(Clone, Default)]
pub struct Gaps(Arc<std::sync::Mutex<(u64, Vec<u64>)>>);

impl Gaps {
    /// Starts a new log: the first gap counts from now.
    pub fn reset(&self) {
        if let Ok(mut g) = self.0.lock() {
            *g = (now_ns(), Vec::new());
        }
    }
    /// A control point.
    pub fn tick(&self) {
        let t = now_ns();
        if let Ok(mut g) = self.0.lock() {
            let last = std::mem::replace(&mut g.0, t);
            g.1.push(t.saturating_sub(last));
        }
    }
    /// The gaps so far, plus the one from the last point to now.
    pub fn finish(&self) -> Vec<u64> {
        self.tick();
        self.0.lock().map(|g| g.1.clone()).unwrap_or_default()
    }
}

// ---------------------------------------------------------------------------
// The engine interface and the command line

/// Mid-await durability: what [`Engine::snapshot`] reports.
pub struct Snap {
    pub kind: &'static str,
    pub bytes: Vec<u8>,
    pub snapshot_ns: f64,
    /// Host-side state a host keeps beside the image: the open calls (the
    /// engine's call id, tool name, arguments).
    pub pending: Vec<(u64, String, Value)>,
}

pub trait Engine: Sized {
    type S;
    const NAME: &'static str;
    const LANG: Lang;

    /// A new runtime: what every session of the process shares (engine,
    /// compiled module, runtime object).
    fn new_runtime() -> Self;
    /// A new empty session.
    fn new_session(&mut self) -> Self::S;
    /// The codemode setup: pi's prelude with these tools for the JS engines,
    /// the typed tool module and its globals for mm. Also defines the sync
    /// host function `hsync`.
    fn setup(&mut self, s: &mut Self::S, tools: &[ToolDef]);
    /// Prepares sessions from a saved image of a codemode setup; false if the
    /// engine has none.
    fn prepare_image(&mut self, _tools: &[ToolDef]) -> bool {
        false
    }
    /// A session from the image [`Engine::prepare_image`] saved.
    fn from_image(&mut self) -> Option<Self::S> {
        None
    }
    /// Runs a codemode script in a set-up session, answering tool calls with
    /// `host`.
    fn run(&mut self, s: &mut Self::S, src: &str, host: &mut dyn Host) -> RunResult;
    /// Runs a plain program (the bench programs) in a new session: output.
    fn run_plain(&mut self, src: &str) -> RunResult;
    /// Bytes the engine itself reports for a session, if it does.
    fn session_bytes(&mut self, _s: &mut Self::S) -> Option<u64> {
        None
    }
    /// Runs `src` (a plain script in a set-up session) until `stop` asks, then
    /// reports how control came back.
    fn control(&mut self, s: &mut Self::S, src: &str, stop: &Stop, delay: Duration) -> Control;
    /// The cost of being stoppable: runs `src` with the engine's stop
    /// mechanism armed (fuel slices, interrupt handler) but never triggered.
    fn run_polled(&mut self, src: &str) -> RunResult;
    /// Snapshot of a session stopped mid-await (after `answered` replies of
    /// `fixture`): returns the image; `restore_and_finish` then resumes it.
    fn snapshot(&mut self, _s: &mut Self::S) -> Option<Snap> {
        None
    }
    /// Restores `snap` into a new session (timed by the caller).
    fn restore(&mut self, _snap: &Snap) -> Option<Self::S> {
        None
    }
    /// Starts a codemode script and answers replies until `stop_after` calls
    /// were answered and the script waits again; true if it is waiting.
    fn run_until(
        &mut self,
        s: &mut Self::S,
        src: &str,
        host: &mut dyn Host,
        stop_after: usize,
    ) -> bool;
    /// Continues a restored (or stopped) run to its end.
    fn finish(&mut self, s: &mut Self::S, host: &mut dyn Host) -> RunResult;
    /// The `gc` command's session: plain scripts, a heap that may grow to
    /// `limit` bytes; `None` if the host does not measure GC.
    fn gc_session(&mut self, _limit: u64) -> Option<Self::S> {
        None
    }
    /// Runs a plain script in a [`Engine::gc_session`] in the engine's
    /// control slices (10k fuel units or interrupt polls), calling
    /// `gaps.tick()` at each slice end.
    fn gc_run(&mut self, _s: &mut Self::S, _src: &str, _gaps: &Gaps) -> Result<(), String> {
        Err("no gc_run".into())
    }
    /// Runs a plain script in a [`Engine::gc_session`] with no fuel while
    /// another thread requests a stop every `every` (when none is
    /// outstanding): the latencies from each request to the stop, in ns.
    /// For engines that stop between any two instructions (mm's park
    /// handle), finer than their slices.
    fn gc_requests(
        &mut self,
        _s: &mut Self::S,
        _src: &str,
        _every: Duration,
    ) -> Result<Vec<u64>, String> {
        Err("no gc_requests".into())
    }
    /// A full collection now, as the script or embedder asks for one.
    fn gc_full(&mut self, _s: &mut Self::S) -> Result<(), String> {
        Err("no gc_full".into())
    }
    /// Free-form facts about the build (versions, features).
    fn describe() -> Value {
        Value::Null
    }
}

/// Max, 99th percentile and median of `gaps`, in ns.
fn slice_gap_stats(mut gaps: Vec<u64>) -> Value {
    let max_at = (0..gaps.len()).max_by_key(|&i| gaps[i]);
    gaps.sort_unstable();
    let at = |q: f64| {
        gaps.get(((gaps.len() as f64 - 1.0) * q).round() as usize)
            .copied()
    };
    json!({"n": gaps.len(), "max_ns": gaps.last(), "max_at": max_at, "p99_ns": at(0.99), "p50_ns": at(0.5)})
}

fn arg(args: &[String], i: usize) -> &str {
    args.get(i).map_or("", String::as_str)
}

fn num(args: &[String], i: usize, default: u64) -> u64 {
    args.get(i).and_then(|a| a.parse().ok()).unwrap_or(default)
}

/// The command line every host shares:
///
/// - `describe`
/// - `bench FILE REPS`: the program REPS times, a new session each, after one
///   warm-up; output checked by the caller.
/// - `startup REPS`
/// - `mem empty|setup N`
/// - `hostcall sync|await|fanout N REPS`
/// - `control loop|sort|string|sort-loop|string-loop|catch|catch-all|bigint`
/// - `hostile FILE`: one hostile script, then a probe in the same session
/// - `isolation`: whether one session's patches reach another
/// - `gcpause LIVE GARBAGE [cyclic]`: the longest stalls while a live set of
///   LIVE small lists grows, beside GARBAGE more, and when it is dropped
/// - `polling FILE REPS`
/// - `snapshot FIXTURE SCRIPT AFTER REPS`
/// - `workload FIXTURE SCRIPT REPS [CALLS_OUT]`
/// - `profile FIXTURE SCRIPT REPS [image]`: `workload`'s runs through one
///   out-of-line function for callgrind, optionally from the template
/// - `gc ITEMS [LIMIT_MIB] [requests]`: full collections and the gaps
///   between control points beside ITEMS live pairs (`gc_script`, [`Gaps`]),
///   or the latencies of stop requests
pub fn main<E: Engine>() {
    let args: Vec<String> = std::env::args().collect();
    let _ = now_ns();
    let base = json!({"engine": E::NAME});
    let with = |mut v: Value| {
        if let (Value::Object(m), Value::Object(b)) = (&mut v, &base) {
            for (k, x) in b {
                m.insert(k.clone(), x.clone());
            }
        }
        emit(v);
    };
    match arg(&args, 1) {
        "describe" => with(json!({"cmd": "describe", "describe": E::describe()})),
        "bench" => {
            let src =
                std::fs::read_to_string(arg(&args, 2)).unwrap_or_else(|e| fail(&e.to_string()));
            let reps = num(&args, 3, 5);
            let mut e = E::new_runtime();
            let out = e.run_plain(&src).unwrap_or_else(|e| fail(&e));
            let mut ns = Vec::new();
            for _ in 0..reps {
                let (r, t) = timed(|| e.run_plain(&src));
                if r.as_deref() != Ok(out.as_str()) {
                    fail("output changed between repetitions");
                }
                ns.push(t);
            }
            with(json!({"cmd": "bench", "file": arg(&args, 2), "output": out, "ns": ns}));
        }
        "polling" => {
            let src =
                std::fs::read_to_string(arg(&args, 2)).unwrap_or_else(|e| fail(&e.to_string()));
            let reps = num(&args, 3, 5);
            let mut e = E::new_runtime();
            let out = e.run_plain(&src).unwrap_or_else(|e| fail(&e));
            let polled = e.run_polled(&src).unwrap_or_else(|e| fail(&e));
            if polled != out {
                fail("polled run printed something else");
            }
            let (mut plain, mut armed) = (Vec::new(), Vec::new());
            for _ in 0..reps {
                plain.push(timed(|| e.run_plain(&src)).1);
                armed.push(timed(|| e.run_polled(&src)).1);
            }
            with(
                json!({"cmd": "polling", "file": arg(&args, 2), "plain_ns": plain, "polled_ns": armed}),
            );
        }
        "startup" => {
            let reps = num(&args, 2, 20);
            let tools = Fixture::load(arg(&args, 3)).tools;
            let (mut e, runtime_ns) = timed(E::new_runtime);
            let mut rec = json!({"cmd": "startup", "runtime_ns": runtime_ns});
            let (mut session, mut setup, mut first, mut image) = (vec![], vec![], vec![], vec![]);
            let mut image_first = vec![];
            // One untimed warm-up of each step.
            let mut s = e.new_session();
            e.setup(&mut s, &tools);
            drop(s);
            let has_image = e.prepare_image(&tools);
            for _ in 0..reps {
                let (mut s, t) = timed(|| e.new_session());
                session.push(t);
                let ((), t) = timed(|| e.setup(&mut s, &tools));
                setup.push(t);
                let (r, t) = timed(|| e.run(&mut s, first_cell(E::LANG), &mut Noop::default()));
                r.unwrap_or_else(|e| fail(&e));
                first.push(t);
                drop(s);
                if has_image {
                    let (s, t) = timed(|| e.from_image());
                    let mut s = s.unwrap_or_else(|| fail("from_image"));
                    image.push(t);
                    let (r, t) = timed(|| e.run(&mut s, first_cell(E::LANG), &mut Noop::default()));
                    r.unwrap_or_else(|e| fail(&e));
                    image_first.push(t);
                }
            }
            rec["session_ns"] = json!(session);
            rec["setup_ns"] = json!(setup);
            rec["first_cell_ns"] = json!(first);
            if has_image {
                rec["image_ns"] = json!(image);
                rec["image_first_cell_ns"] = json!(image_first);
            }
            with(rec);
        }
        "mem" => {
            let mode = arg(&args, 2).to_owned();
            let n = num(&args, 3, 1);
            let tools = Fixture::load(arg(&args, 4)).tools;
            let mut e = E::new_runtime();
            // Warm the runtime (caches, lazily built tables) with one session.
            {
                let mut s = e.new_session();
                if mode == "setup" {
                    e.setup(&mut s, &tools);
                    e.run(&mut s, first_cell(E::LANG), &mut Noop::default())
                        .unwrap_or_else(|e| fail(&e));
                }
            }
            let (before, _) = rss();
            let heap_before = heap_in_use();
            let mut sessions = Vec::new();
            let mut reported = 0u64;
            for _ in 0..n {
                let mut s = e.new_session();
                if mode == "setup" {
                    e.setup(&mut s, &tools);
                    e.run(&mut s, first_cell(E::LANG), &mut Noop::default())
                        .unwrap_or_else(|e| fail(&e));
                }
                reported += e.session_bytes(&mut s).unwrap_or(0);
                sessions.push(s);
            }
            let (after, peak) = rss();
            let heap_after = heap_in_use();
            with(
                json!({"cmd": "mem", "mode": mode, "n": n, "rss_before": before, "rss_after": after,
                "heap_before": heap_before, "heap_after": heap_after,
                "rss_peak": peak, "engine_bytes": reported}),
            );
            drop(sessions);
        }
        "hostcall" => {
            let kind = arg(&args, 2).to_owned();
            let n = num(&args, 3, 10_000);
            let reps = num(&args, 4, 5);
            let mut e = E::new_runtime();
            let (mut tool_ns, mut local_ns) = (Vec::new(), Vec::new());
            let script = |local| match kind.as_str() {
                "sync" => sync_script(E::LANG, n, local),
                "await" => await_script(E::LANG, n, local),
                "fanout" => fanout_script(E::LANG, n, local),
                _ => fail("hostcall kind"),
            };
            let (with_tool, without) = (script(false), script(true));
            for i in 0..=reps {
                for (src, out) in [(&with_tool, &mut tool_ns), (&without, &mut local_ns)] {
                    let mut s = e.new_session();
                    e.setup(&mut s, &noop_tools());
                    let mut host = Noop::default();
                    let (r, t) = timed(|| e.run(&mut s, src, &mut host));
                    r.unwrap_or_else(|e| fail(&e));
                    if i > 0 {
                        out.push(t);
                    }
                }
            }
            with(
                json!({"cmd": "hostcall", "kind": kind, "n": n, "tool_ns": tool_ns, "local_ns": local_ns}),
            );
        }
        "control" => {
            let kind = arg(&args, 2).to_owned();
            let mut e = E::new_runtime();
            let mut s = e.new_session();
            e.setup(&mut s, &noop_tools());
            let stop = Stop::default();
            let src = control_script(E::LANG, &kind);
            // A target an engine never stops: report it and end the process.
            let limit = std::env::var("CONTROL_LIMIT_S")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(20);
            let (k, st) = (kind.clone(), stop.clone());
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_secs(limit));
                emit(
                    json!({"cmd": "control", "engine": E::NAME, "kind": k, "how": "not regained",
                    "limit_s": limit, "requested": st.requested()}),
                );
                std::process::exit(0);
            });
            let c = e.control(&mut s, &src, &stop, control_delay(&kind));
            let back = c.back_ns;
            let asked = stop.at();
            with(
                json!({"cmd": "control", "kind": kind, "how": c.how, "resumable": c.resumable,
                "session_usable": c.session_usable,
                "latency_ns": if asked == 0 { Value::Null } else { json!(back.saturating_sub(asked)) }}),
            );
        }
        "snapshot" => {
            let fixture = Fixture::load(arg(&args, 2));
            let src =
                std::fs::read_to_string(arg(&args, 3)).unwrap_or_else(|e| fail(&e.to_string()));
            let after = num(&args, 4, 0) as usize;
            let reps = num(&args, 5, 10);
            let mut e = E::new_runtime();
            let mut s = e.new_session();
            e.setup(&mut s, &fixture.tools);
            let mut host = Replay::new(&fixture);
            if !e.run_until(&mut s, &src, &mut host, after) {
                fail("the script did not wait");
            }
            let Some(snap) = e.snapshot(&mut s) else {
                with(json!({"cmd": "snapshot", "supported": false}));
                return;
            };
            let mut snap_ns = vec![snap.snapshot_ns];
            for _ in 1..reps {
                if let Some(again) = e.snapshot(&mut s) {
                    snap_ns.push(again.snapshot_ns);
                }
            }
            drop(s);
            let mut restore_ns = Vec::new();
            let mut finished = None;
            for i in 0..reps {
                let (r, t) = timed(|| e.restore(&snap));
                let mut r = r.unwrap_or_else(|| fail("restore failed"));
                restore_ns.push(t);
                if i == 0 {
                    // Finish the first restore with the same replies: the run
                    // must make the remaining calls and succeed.
                    let mut rest = Replay::new(&fixture);
                    // Hosts that ask the tool when a call is issued (mm) list
                    // the open calls last; the others ask on delivery.
                    let (n, k) = (host.calls.len(), snap.pending.len());
                    let open_last = n >= k
                        && host.calls[n - k..]
                            .iter()
                            .zip(&snap.pending)
                            .all(|((a, x), (_, b, y))| key(a, x) == key(b, y));
                    let answered = if open_last { n - k } else { n };
                    for (name, args) in &host.calls[..answered] {
                        let _ = rest.call(name, args.clone());
                    }
                    let before = rest.calls.len();
                    let out = e.finish(&mut r, &mut rest);
                    finished = Some(json!({"ok": out.is_ok(), "error": out.err(),
                        "calls_after_restore": rest.calls.len() - before, "misses": rest.misses}));
                }
            }
            with(
                json!({"cmd": "snapshot", "supported": true, "kind": snap.kind, "task": fixture.task,
                "answered": after, "raw_bytes": snap.bytes.len(), "zstd1_bytes": zstd1(&snap.bytes),
                "snapshot_ns": snap_ns, "restore_ns": restore_ns, "finished": finished}),
            );
        }
        "workload" => {
            let fixture = Fixture::load(arg(&args, 2));
            let src =
                std::fs::read_to_string(arg(&args, 3)).unwrap_or_else(|e| fail(&e.to_string()));
            let reps = num(&args, 4, 5);
            let mut e = E::new_runtime();
            let (mut ns, mut setup_ns, mut run_ns) = (Vec::new(), Vec::new(), Vec::new());
            let mut checked = None;
            for i in 0..=reps {
                let mut host = Replay::new(&fixture);
                let (mut s, t_setup) = timed(|| {
                    let mut s = e.new_session();
                    e.setup(&mut s, &fixture.tools);
                    s
                });
                let (r, t_run) = timed(|| e.run(&mut s, &src, &mut host));
                drop(s);
                if i == 0 {
                    checked = Some(json!({"error": r.as_ref().err(), "misses": host.misses,
                        "calls": host.calls.iter().map(|(n, a)| json!([n, a])).collect::<Vec<_>>()}));
                } else {
                    ns.push(t_setup + t_run);
                    setup_ns.push(t_setup);
                    run_ns.push(t_run);
                }
            }
            with(
                json!({"cmd": "workload", "task": fixture.task, "ns": ns, "setup_ns": setup_ns,
                "run_ns": run_ns, "check": checked}),
            );
        }
        "profile" => {
            // As `workload`, but each script run goes through
            // `profiled_run`, the one function callgrind collects
            // (`--toggle-collect=*profiled_run*`). With `image`, sessions
            // come from the engine's template instead of new + setup.
            let fixture = Fixture::load(arg(&args, 2));
            let src =
                std::fs::read_to_string(arg(&args, 3)).unwrap_or_else(|e| fail(&e.to_string()));
            let reps = num(&args, 4, 5);
            let image = arg(&args, 5) == "image";
            let mut e = E::new_runtime();
            if image && !e.prepare_image(&fixture.tools) {
                fail("no image");
            }
            let (mut setup_ns, mut run_ns) = (Vec::new(), Vec::new());
            for i in 0..=reps {
                let mut host = Replay::new(&fixture);
                let (s, t_setup) = timed(|| {
                    if image {
                        e.from_image().unwrap_or_else(|| fail("from_image"))
                    } else {
                        let mut s = e.new_session();
                        e.setup(&mut s, &fixture.tools);
                        s
                    }
                });
                let mut s = s;
                // The warm-up run (it fills the module cache) is not collected.
                let (r, t_run) = if i == 0 {
                    timed(|| e.run(&mut s, &src, &mut host))
                } else {
                    timed(|| profiled_run(&mut e, &mut s, &src, &mut host))
                };
                if let Err(err) = r {
                    fail(&err);
                }
                drop(s);
                if i > 0 {
                    setup_ns.push(t_setup);
                    run_ns.push(t_run);
                }
            }
            with(
                json!({"cmd": "profile", "task": fixture.task, "image": image,
                "setup_ns": setup_ns, "run_ns": run_ns}),
            );
        }
        "gc" => {
            // `gc ITEMS [LIMIT_MIB] [requests]`: ITEMS live pairs, then the
            // full collection's time and each workload's gaps between
            // control points beside them (`gc_script`); with `requests`,
            // the latencies of stop requests made every 1 ms instead
            // (`Engine::gc_requests`).
            let items = num(&args, 2, 550_000);
            let limit = num(&args, 3, 1024) << 20;
            let requests = arg(&args, 4) == "requests";
            let mut e = E::new_runtime();
            let heap0 = heap_in_use();
            let Some(mut s) = e.gc_session(limit) else {
                with(json!({"cmd": "gc", "supported": false}));
                return;
            };
            let gaps = Gaps::default();
            gaps.reset();
            let (r, build_ns) =
                timed(|| e.gc_run(&mut s, &gc_script(E::LANG, "build", items), &gaps));
            r.unwrap_or_else(|e| fail(&e));
            let mut full_ns = Vec::new();
            for _ in 0..5 {
                let (r, t) = timed(|| e.gc_full(&mut s));
                r.unwrap_or_else(|e| fail(&e));
                full_ns.push(t);
            }
            let live = heap_in_use().saturating_sub(heap0);
            let engine_bytes = e.session_bytes(&mut s);
            let mut workloads = Vec::new();
            // `appends` last: it grows the live data.
            for (kind, n) in [
                ("garbage", 4_000_000),
                ("cycles", 2_000_000),
                ("writes", 4_000_000),
                ("appends", 500_000),
            ] {
                gaps.reset();
                let src = gc_script(E::LANG, kind, n);
                let (r, t) = timed(|| {
                    if requests {
                        e.gc_requests(&mut s, &src, Duration::from_millis(1))
                    } else {
                        e.gc_run(&mut s, &src, &gaps).map(|()| gaps.finish())
                    }
                });
                let mut w =
                    slice_gap_stats(r.as_ref().map_or_else(|_| gaps.finish(), Clone::clone));
                if let Err(err) = r {
                    // Out of memory, say: reported, and the next workload runs.
                    let last = err
                        .lines()
                        .rev()
                        .find(|l| !l.trim().is_empty())
                        .unwrap_or_default();
                    w["error"] = json!(last.chars().take(160).collect::<String>());
                }
                w["kind"] = json!(kind);
                w["iterations"] = json!(n);
                w["wall_ns"] = json!(t);
                workloads.push(w);
            }
            with(
                json!({"cmd": "gc", "supported": true, "items": items, "limit": limit,
                "mode": if requests { "requests" } else { "slices" },
                "build_ns": build_ns, "live_heap": live, "engine_bytes": engine_bytes,
                "full_ns": full_ns, "workloads": workloads, "rss_peak": rss().1}),
            );
        }
        "hostile" => {
            // One hostile script in a set-up session, then a probe in the
            // same session: does the engine report an error, and does the
            // session still work? A crash ends the process (run.py records it).
            let file = arg(&args, 2).to_owned();
            let src = std::fs::read_to_string(&file).unwrap_or_else(|e| fail(&e.to_string()));
            let mut e = E::new_runtime();
            let mut s = e.new_session();
            e.setup(&mut s, &noop_tools());
            let (r, t) = timed(|| e.run(&mut s, &src, &mut Noop::default()));
            let probe = e.run(&mut s, probe_script(E::LANG), &mut Noop::default());
            let (_, peak) = rss();
            let error = r.as_ref().err().map(|m| {
                let last = m
                    .lines()
                    .rev()
                    .find(|l| !l.trim().is_empty())
                    .unwrap_or_default();
                last.chars().take(160).collect::<String>()
            });
            let out = r
                .as_ref()
                .ok()
                .map(|o| o.chars().take(160).collect::<String>());
            with(
                json!({"cmd": "hostile", "file": file, "ok": r.is_ok(), "error": error, "output": out,
                "ms": t / 1e6, "probe_ok": probe.as_deref() == Ok("42\n"), "rss_peak": peak}),
            );
        }
        "isolation" => {
            // Session A patches shared-looking state (a builtin, a native
            // module, a Python module); session B, created before A ran,
            // then checks it.
            let mut e = E::new_runtime();
            let mut a = e.new_session();
            e.setup(&mut a, &noop_tools());
            let mut b = e.new_session();
            e.setup(&mut b, &noop_tools());
            let (patch, check, want) = match E::LANG {
                Lang::Py => (
                    "def _try(f):\n    try:\n        f()\n    except Exception:\n        pass\ntry:\n    import builtins\n    _try(lambda: setattr(builtins, 'len', lambda x: -1))\nexcept ImportError:\n    pass\nimport json\n_try(lambda: setattr(json, 'dumps', lambda *a: 'patched'))\ntry:\n    import asyncio\n    _try(lambda: setattr(asyncio, 'gather', None))\nexcept ImportError:\n    pass\n",
                    "import json\nr = [len([1, 2]), json.dumps(1)]\ntry:\n    import asyncio\n    r.append(asyncio.gather is not None)\nexcept ImportError:\n    r.append(True)\nprint(r)\n",
                    "[2, '1', True]\n",
                ),
                Lang::Js => (
                    "JSON.stringify = () => 'patched'; Array.prototype.push = null;",
                    "console.log([1, 2].length, JSON.stringify(1));",
                    "2 1\n",
                ),
            };
            let patched = e.run(&mut a, patch, &mut Noop::default());
            let seen = e.run(&mut b, check, &mut Noop::default());
            let own = e.run(&mut a, check, &mut Noop::default());
            let show = |r: &RunResult| match r {
                Ok(o) => o.trim_end().to_owned(),
                Err(e) => format!("error: {}", e.lines().last().unwrap_or_default()),
            };
            with(
                json!({"cmd": "isolation", "patch": patched.err(), "other_session": show(&seen),
                "same_session": show(&own), "isolated": seen.as_deref() == Ok(want)}),
            );
        }
        "gcpause" => {
            // GC pauses: the longest gaps between `hsync` timestamps while a
            // live set grows, while garbage is made beside it, and across
            // dropping it (gcpause_script).
            if E::LANG != Lang::Py {
                fail("gcpause: Python engines only");
            }
            let live = num(&args, 2, 0);
            let garbage = num(&args, 3, 3_000_000);
            let cyclic = arg(&args, 4) == "cyclic";
            let src = gcpause_script(live, garbage, cyclic);
            let mut e = E::new_runtime();
            let mut s = e.new_session();
            e.setup(&mut s, &noop_tools());
            RECORD.store(true, Ordering::SeqCst);
            let (r, t) = timed(|| e.run(&mut s, &src, &mut Noop::default()));
            RECORD.store(false, Ordering::SeqCst);
            let (_, peak) = rss();
            let ticks = std::mem::take(&mut *TICKS.lock().unwrap_or_else(std::sync::PoisonError::into_inner));
            let b = usize::try_from(live / 1000).unwrap_or(0);
            let rounds = usize::try_from(garbage / 1000).unwrap_or(0);
            if let Err(err) = r {
                let last = err.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or_default().to_owned();
                with(json!({"cmd": "gcpause", "live": live, "garbage": garbage, "cyclic": cyclic,
                    "error": last, "ticks": ticks.len(), "rss_peak": peak}));
                return;
            }
            if ticks.len() != b + rounds + 2 {
                fail(&format!("gcpause: {} ticks, expected {}", ticks.len(), b + rounds + 2));
            }
            with(json!({"cmd": "gcpause", "live": live, "garbage": garbage, "cyclic": cyclic,
                "build": gap_stats(&ticks[..=b]), "steady": gap_stats(&ticks[b..=b + rounds]),
                "drop_ms": (ticks[b + rounds + 1] - ticks[b + rounds]) as f64 / 1e6,
                "total_ms": t / 1e6, "rss_peak": peak}));
        }
        other => fail(&format!("unknown command {other:?}")),
    }
}

/// One script run, kept out of line so callgrind can collect it alone.
#[inline(never)]
fn profiled_run<E: Engine>(e: &mut E, s: &mut E::S, src: &str, host: &mut dyn Host) -> RunResult {
    std::hint::black_box(e.run(s, src, host))
}
