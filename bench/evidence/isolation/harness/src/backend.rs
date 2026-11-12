//! The three isolation modes behind one request/response trait.

use std::{
    io::{BufReader, Read, Write},
    path::PathBuf,
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    sync::{Arc, Mutex},
};

use iso_service::{Request, Response, Service, decode, encode};

/// One session in one mode. `Err` means the session's container died: the
/// wasm instance trapped or the worker process exited.
pub trait Backend: Send {
    fn call(&mut self, request: &Request, raw: &[u8]) -> Result<(Response, Vec<u8>), String>;
    /// A way to stop the running cell from another thread: a park request
    /// (native, child) or an epoch interrupt (wasm).
    fn interrupter(&self) -> Interrupter;
    /// The worker's pid (child mode).
    fn pid(&self) -> Option<u32> {
        None
    }
    /// SIGKILL for the worker (child mode).
    fn killer(&self) -> Option<Interrupter> {
        None
    }
    /// Linear memory in KiB (wasm modes).
    fn linear_kib(&self) -> Option<u64> {
        None
    }
}

/// Stops a running cell from another thread.
pub type Interrupter = Arc<dyn Fn() + Send + Sync>;

// ---------------------------------------------------------------- native

/// Native in-process: the service called directly, on the caller's thread.
pub struct Native {
    pub service: Service,
}

impl Native {
    pub fn new() -> Native {
        Native {
            service: Service::new(),
        }
    }
}

impl Backend for Native {
    fn call(&mut self, request: &Request, raw: &[u8]) -> Result<(Response, Vec<u8>), String> {
        Ok(self.service.handle(request, raw))
    }

    fn interrupter(&self) -> Interrupter {
        let slot = self.service.park_slot();
        Arc::new(move || {
            if let Ok(slot) = slot.lock() {
                if let Some(handle) = slot.as_ref() {
                    handle.request();
                }
            }
        })
    }
}

// ---------------------------------------------------------------- child

/// How to start a child worker.
#[derive(Clone, Debug)]
pub struct ChildConfig {
    pub worker: PathBuf,
    /// The session thread's stack (`ISO_STACK_KIB`); `None`: 16 MiB.
    pub stack_kib: Option<usize>,
    /// `prlimit` arguments, e.g. `--as=268435456`.
    pub prlimit: Vec<String>,
}

/// Child process: frames over the worker's stdin/stdout.
pub struct ChildWorker {
    child: Arc<Mutex<Child>>,
    pid: u32,
    stdin: Arc<Mutex<ChildStdin>>,
    stdout: BufReader<ChildStdout>,
}

impl ChildWorker {
    pub fn spawn(config: &ChildConfig) -> Result<ChildWorker, String> {
        let mut command = if config.prlimit.is_empty() {
            Command::new(&config.worker)
        } else if cfg!(target_os = "macos") {
            let mut c = Command::new(&config.worker);
            #[cfg(target_os = "macos")]
            macos::apply_rlimits(&mut c, &config.prlimit);
            c
        } else {
            let mut c = Command::new("prlimit");
            c.args(&config.prlimit).arg("--").arg(&config.worker);
            c
        };
        if let Some(kib) = config.stack_kib {
            command.env("ISO_STACK_KIB", kib.to_string());
        }
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("spawn {}: {e}", config.worker.display()))?;
        let stdin = child.stdin.take().ok_or("no stdin")?;
        let stdout = child.stdout.take().ok_or("no stdout")?;
        Ok(ChildWorker {
            pid: child.id(),
            child: Arc::new(Mutex::new(child)),
            stdin: Arc::new(Mutex::new(stdin)),
            stdout: BufReader::new(stdout),
        })
    }

    pub fn pid(&self) -> u32 {
        self.pid
    }

    /// SIGKILLs the worker, from any thread.
    pub fn killer(&self) -> Interrupter {
        let child = Arc::clone(&self.child);
        Arc::new(move || {
            if let Ok(mut c) = child.lock() {
                let _ = c.kill();
            }
        })
    }

    fn died(&mut self) -> String {
        let status = self
            .child
            .lock()
            .map_err(|_| "poisoned".to_owned())
            .and_then(|mut c| c.wait().map_err(|e| e.to_string()));
        match status {
            Ok(status) => format!("worker exited: {status}"),
            Err(e) => format!("worker lost: {e}"),
        }
    }
}

fn write_frame(stdin: &Mutex<ChildStdin>, message: &[u8]) -> std::io::Result<()> {
    let mut stdin = stdin
        .lock()
        .map_err(|_| std::io::Error::other("poisoned"))?;
    let len = u32::try_from(message.len())
        .unwrap_or(u32::MAX)
        .to_le_bytes();
    stdin.write_all(&len)?;
    stdin.write_all(message)?;
    stdin.flush()
}

impl Backend for ChildWorker {
    fn call(&mut self, request: &Request, raw: &[u8]) -> Result<(Response, Vec<u8>), String> {
        if write_frame(&self.stdin, &encode(request, raw)).is_err() {
            return Err(self.died());
        }
        let mut len = [0u8; 4];
        if self.stdout.read_exact(&mut len).is_err() {
            return Err(self.died());
        }
        let mut body = vec![0u8; u32::from_le_bytes(len) as usize];
        if self.stdout.read_exact(&mut body).is_err() {
            return Err(self.died());
        }
        let (response, raw) = decode::<Response>(&body)?;
        Ok((response, raw.to_vec()))
    }

    fn pid(&self) -> Option<u32> {
        Some(self.pid)
    }

    fn killer(&self) -> Option<Interrupter> {
        Some(ChildWorker::killer(self))
    }

    fn interrupter(&self) -> Interrupter {
        let stdin = Arc::clone(&self.stdin);
        let park = encode(&Request::Park, &[]);
        Arc::new(move || {
            let _ = write_frame(&stdin, &park);
        })
    }
}

impl Drop for ChildWorker {
    fn drop(&mut self) {
        if let Ok(mut c) = self.child.lock() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

/// `VmRSS` and `Pss` of a process, in KiB.
#[cfg(not(target_os = "macos"))]
pub fn rss_pss_kib(pid: u32) -> (u64, u64) {
    let field = |text: &str, name: &str| -> u64 {
        text.lines()
            .find_map(|l| l.strip_prefix(name))
            .and_then(|v| v.split_whitespace().next())
            .and_then(|v| v.parse().ok())
            .unwrap_or(0)
    };
    let status = std::fs::read_to_string(format!("/proc/{pid}/status")).unwrap_or_default();
    let rollup = std::fs::read_to_string(format!("/proc/{pid}/smaps_rollup")).unwrap_or_default();
    (field(&status, "VmRSS:"), field(&rollup, "Pss:"))
}

/// This process's `VmRSS` and `VmHWM`, in KiB.
#[cfg(not(target_os = "macos"))]
pub fn self_rss_hwm_kib() -> (u64, u64) {
    let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    let field = |name: &str| -> u64 {
        status
            .lines()
            .find_map(|l| l.strip_prefix(name))
            .and_then(|v| v.split_whitespace().next())
            .and_then(|v| v.parse().ok())
            .unwrap_or(0)
    };
    (field("VmRSS:"), field("VmHWM:"))
}

/// macOS has no PSS. The second value is the process's phys_footprint
/// (private dirty plus compressed memory, what Activity Monitor shows as
/// "Memory"), which like PSS leaves out clean shared library pages.
#[cfg(target_os = "macos")]
pub fn rss_pss_kib(pid: u32) -> (u64, u64) {
    macos::rusage(pid).map_or((0, 0), |(rss, footprint)| (rss >> 10, footprint >> 10))
}

/// This process's resident size and peak resident size (`ru_maxrss`), in KiB.
#[cfg(target_os = "macos")]
pub fn self_rss_hwm_kib() -> (u64, u64) {
    (
        macos::rusage(std::process::id()).map_or(0, |(rss, _)| rss >> 10),
        macos::max_rss_bytes() >> 10,
    )
}

#[cfg(target_os = "macos")]
#[allow(unsafe_code, reason = "proc_pid_rusage, getrusage and setrlimit FFI")]
pub mod macos {
    /// `(ri_resident_size, ri_phys_footprint)` in bytes.
    pub fn rusage(pid: u32) -> Option<(u64, u64)> {
        let pid = i32::try_from(pid).ok()?;
        // SAFETY: the zeroed POD struct is a valid output buffer for the
        // RUSAGE_INFO_V2 flavor, which writes exactly one rusage_info_v2.
        let mut info: libc::rusage_info_v2 = unsafe { std::mem::zeroed() };
        let buffer = (&raw mut info).cast::<libc::rusage_info_t>();
        let ok = unsafe { libc::proc_pid_rusage(pid, libc::RUSAGE_INFO_V2, buffer) } == 0;
        ok.then_some((info.ri_resident_size, info.ri_phys_footprint))
    }

    /// Peak resident size of this process (macOS reports bytes).
    pub fn max_rss_bytes() -> u64 {
        // SAFETY: getrusage writes one rusage into the zeroed struct.
        let mut ru: libc::rusage = unsafe { std::mem::zeroed() };
        if unsafe { libc::getrusage(libc::RUSAGE_SELF, &raw mut ru) } == 0 {
            u64::try_from(ru.ru_maxrss).unwrap_or(0)
        } else {
            0
        }
    }

    /// Applies `prlimit`-style options (`--as=BYTES`, `--cpu=SECONDS`) in
    /// the child before exec; macOS has no `prlimit`. A limit the kernel
    /// refuses fails the spawn with its errno.
    pub fn apply_rlimits(command: &mut std::process::Command, options: &[String]) {
        use std::os::unix::process::CommandExt;
        let limits: Vec<(libc::c_int, u64)> = options
            .iter()
            .filter_map(|o| {
                let (k, v) = o.strip_prefix("--")?.split_once('=')?;
                let resource = match k {
                    "as" => libc::RLIMIT_AS,
                    "cpu" => libc::RLIMIT_CPU,
                    _ => return None,
                };
                Some((resource, v.parse().ok()?))
            })
            .collect();
        // SAFETY: the closure only calls setrlimit, which is
        // async-signal-safe, between fork and exec.
        unsafe {
            command.pre_exec(move || {
                for &(resource, value) in &limits {
                    let limit = libc::rlimit {
                        rlim_cur: value,
                        rlim_max: value,
                    };
                    if libc::setrlimit(resource, &raw const limit) != 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                }
                Ok(())
            });
        }
    }
}

// ---------------------------------------------------------------- wasm

#[cfg(feature = "wasm")]
pub use wasm::{WasmConfig, WasmHost};

#[cfg(feature = "wasm")]
mod wasm {
    use std::{
        sync::{
            Arc,
            atomic::{AtomicBool, AtomicU64, Ordering},
        },
        time::{Duration, Instant},
    };

    use iso_service::{Request, Response, decode, encode};
    use wasmtime::{
        Config, Engine, InstanceAllocationStrategy, InstancePre, Linker, Memory, Module,
        PoolingAllocationConfig, Store, StoreLimits, StoreLimitsBuilder, TypedFunc, UpdateDeadline,
    };
    use wasmtime_wasi::{WasiCtxBuilder, p1::WasiP1Ctx};

    use std::path::Path;

    use super::{Backend, Interrupter};

    /// The wasmtime configuration under test.
    #[derive(Clone, Debug)]
    pub struct WasmConfig {
        /// Epoch interruption, with a ticker every `EPOCH_TICK`.
        pub epoch: bool,
        /// Ticks (`EPOCH_TICK`) one call may run before it traps (with `epoch`).
        pub epoch_deadline_ticks: u64,
        /// Wasmtime fuel per call (`consume_fuel`); `None`: off.
        pub fuel: Option<u64>,
        pub pooling: bool,
        /// Instances the pool holds (with `pooling`).
        pub pool_size: u32,
        /// `max_wasm_stack` in bytes (wasmtime's default is 512 KiB).
        pub max_wasm_stack: usize,
        /// The store's linear-memory ceiling (`StoreLimits::memory_size`).
        pub memory_limit: usize,
        /// Load a precompiled `.cwasm` (`Module::deserialize_file`) instead
        /// of compiling the `.wasm`.
        pub precompiled: bool,
    }

    impl Default for WasmConfig {
        /// The recommended configuration (docs/research/isolation-modes.md).
        fn default() -> Self {
            WasmConfig {
                epoch: true,
                epoch_deadline_ticks: 6000,
                fuel: None,
                pooling: false,
                pool_size: 1100,
                max_wasm_stack: 4 << 20,
                memory_limit: 512 << 20,
                precompiled: true,
            }
        }
    }

    /// One epoch tick.
    pub const EPOCH_TICK: Duration = Duration::from_millis(10);

    pub struct Ctx {
        wasi: WasiP1Ctx,
        limits: StoreLimits,
    }

    /// The shared part: engine, linked module, ticker.
    pub struct WasmHost {
        pub engine: Engine,
        pre: InstancePre<Ctx>,
        pub config: WasmConfig,
        /// Module load time (compile or deserialize).
        pub load: Duration,
        ticker_stop: Arc<AtomicBool>,
    }

    impl Drop for WasmHost {
        fn drop(&mut self) {
            self.ticker_stop.store(true, Ordering::Relaxed);
        }
    }

    impl WasmHost {
        /// `wasm` is the guest module; its precompiled form is `wasm` with
        /// the extension `cwasm` for this configuration (written if missing).
        pub fn new(wasm: &Path, config: WasmConfig) -> Result<WasmHost, String> {
            let mut c = Config::new();
            c.epoch_interruption(config.epoch);
            c.consume_fuel(config.fuel.is_some());
            c.max_wasm_stack(config.max_wasm_stack);
            // Validated against max_wasm_stack even though calls are sync.
            c.async_stack_size(config.max_wasm_stack + (1 << 20));
            if config.pooling {
                let mut pool = PoolingAllocationConfig::new();
                pool.total_core_instances(config.pool_size)
                    .total_memories(config.pool_size)
                    .total_tables(config.pool_size)
                    .max_memory_size(config.memory_limit);
                c.allocation_strategy(InstanceAllocationStrategy::Pooling(pool));
            }
            let engine = Engine::new(&c).map_err(|e| format!("{e:#}"))?;
            let start = Instant::now();
            let module = if config.precompiled {
                let tag = format!(
                    "{}{}{}",
                    if config.epoch { "e" } else { "" },
                    if config.fuel.is_some() { "f" } else { "" },
                    if config.pooling { "p" } else { "" }
                );
                let cwasm = wasm.with_extension(format!("{tag}.cwasm"));
                if !cwasm.exists() {
                    let bytes = std::fs::read(wasm).map_err(|e| e.to_string())?;
                    let compiled = engine
                        .precompile_module(&bytes)
                        .map_err(|e| format!("{e:#}"))?;
                    std::fs::write(&cwasm, compiled).map_err(|e| e.to_string())?;
                }
                let start = Instant::now();
                // SAFETY: wasmtime requires the file to be a module this
                // wasmtime version precompiled with a compatible config,
                // which is what the lines above wrote; this is the harness,
                // not an engine crate.
                #[allow(unsafe_code)]
                let m = unsafe { Module::deserialize_file(&engine, &cwasm) }
                    .map_err(|e| format!("{e:#}"))?;
                let load = start.elapsed();
                (m, load)
            } else {
                let m = Module::from_file(&engine, wasm).map_err(|e| format!("{e:#}"))?;
                (m, start.elapsed())
            };
            let (module, load) = module;
            let mut linker: Linker<Ctx> = Linker::new(&engine);
            wasmtime_wasi::p1::add_to_linker_sync(&mut linker, |c: &mut Ctx| &mut c.wasi)
                .map_err(|e| format!("{e:#}"))?;
            let pre = linker
                .instantiate_pre(&module)
                .map_err(|e| format!("{e:#}"))?;
            let ticker_stop = Arc::new(AtomicBool::new(false));
            if config.epoch {
                let engine = engine.clone();
                let stop = Arc::clone(&ticker_stop);
                std::thread::spawn(move || {
                    while !stop.load(Ordering::Relaxed) {
                        std::thread::sleep(EPOCH_TICK);
                        engine.increment_epoch();
                    }
                });
            }
            Ok(WasmHost {
                engine,
                pre,
                config,
                load,
                ticker_stop,
            })
        }

        /// A new instance: one store, one session's worth of linear memory.
        pub fn instantiate(&self) -> Result<Wasm, String> {
            let ctx = Ctx {
                wasi: WasiCtxBuilder::new().build_p1(),
                limits: StoreLimitsBuilder::new()
                    .memory_size(self.config.memory_limit)
                    .instances(1)
                    .build(),
            };
            let mut store = Store::new(&self.engine, ctx);
            store.limiter(|c| &mut c.limits);
            if let Some(fuel) = self.config.fuel {
                store.set_fuel(fuel).map_err(|e| format!("{e:#}"))?;
            }
            let interrupt = Arc::new(AtomicBool::new(false));
            let ticks = Arc::new(AtomicU64::new(0));
            if self.config.epoch {
                // Checked every tick: an interrupt or a call past its budget
                // traps; otherwise the call runs on for another tick.
                store.set_epoch_deadline(1);
                let (flag, used, budget) = (
                    Arc::clone(&interrupt),
                    Arc::clone(&ticks),
                    self.config.epoch_deadline_ticks,
                );
                store.epoch_deadline_callback(move |_| {
                    if flag.swap(false, Ordering::Relaxed)
                        || used.fetch_add(1, Ordering::Relaxed) >= budget
                    {
                        Ok(UpdateDeadline::Interrupt)
                    } else {
                        Ok(UpdateDeadline::Continue(1))
                    }
                });
            }
            let instance = self
                .pre
                .instantiate(&mut store)
                .map_err(|e| format!("{e:#}"))?;
            if let Ok(init) = instance.get_typed_func::<(), ()>(&mut store, "_initialize") {
                init.call(&mut store, ()).map_err(|e| format!("{e:#}"))?;
            }
            let mm_in = instance
                .get_typed_func::<u32, u32>(&mut store, "mm_in")
                .map_err(|e| format!("{e:#}"))?;
            let mm_call = instance
                .get_typed_func::<u32, u64>(&mut store, "mm_call")
                .map_err(|e| format!("{e:#}"))?;
            let memory = instance
                .get_memory(&mut store, "memory")
                .ok_or("no memory export")?;
            Ok(Wasm {
                store,
                mm_in,
                mm_call,
                memory,
                engine: self.engine.clone(),
                config: self.config.clone(),
                dead: None,
                interrupt,
                ticks,
            })
        }
    }

    /// One wasm instance holding one session.
    pub struct Wasm {
        store: Store<Ctx>,
        mm_in: TypedFunc<u32, u32>,
        mm_call: TypedFunc<u32, u64>,
        memory: Memory,
        engine: Engine,
        config: WasmConfig,
        pub dead: Option<String>,
        interrupt: Arc<AtomicBool>,
        ticks: Arc<AtomicU64>,
    }

    impl Wasm {
        /// Linear memory size in bytes.
        pub fn memory_bytes(&self) -> usize {
            self.memory.data_size(&self.store)
        }

        fn kill(&mut self, e: &wasmtime::Error) -> String {
            let message = match e.downcast_ref::<wasmtime::Trap>() {
                Some(trap) => format!("trap: {trap}"),
                None => format!("{e:#}"),
            };
            let message = message.lines().next().unwrap_or_default().to_owned();
            self.dead = Some(message.clone());
            message
        }
    }

    impl Backend for Wasm {
        fn linear_kib(&self) -> Option<u64> {
            Some((self.memory_bytes() / 1024) as u64)
        }

        fn call(&mut self, request: &Request, raw: &[u8]) -> Result<(Response, Vec<u8>), String> {
            if let Some(dead) = &self.dead {
                return Err(format!("instance dead: {dead}"));
            }
            if let Some(fuel) = self.config.fuel {
                self.store.set_fuel(fuel).map_err(|e| format!("{e:#}"))?;
            }
            self.ticks.store(0, Ordering::Relaxed);
            self.interrupt.store(false, Ordering::Relaxed);
            let message = encode(request, raw);
            let len = u32::try_from(message.len()).map_err(|e| e.to_string())?;
            let at = match self.mm_in.call(&mut self.store, len) {
                Ok(at) => at,
                Err(e) => return Err(self.kill(&e)),
            };
            self.memory
                .write(&mut self.store, at as usize, &message)
                .map_err(|e| e.to_string())?;
            let packed = match self.mm_call.call(&mut self.store, len) {
                Ok(p) => p,
                Err(e) => return Err(self.kill(&e)),
            };
            let (at, len) = ((packed >> 32) as usize, (packed & 0xffff_ffff) as usize);
            let data = self.memory.data(&self.store);
            let body = data.get(at..at + len).ok_or("response out of bounds")?;
            let (response, raw) = decode::<Response>(body)?;
            Ok((response, raw.to_vec()))
        }

        /// An epoch interrupt: the next deadline check traps. A running
        /// call cannot be parked from outside (the store is borrowed by the
        /// call), so this is the only asynchronous stop wasmtime offers a
        /// synchronous embedding.
        fn interrupter(&self) -> Interrupter {
            let engine = self.engine.clone();
            let flag = Arc::clone(&self.interrupt);
            Arc::new(move || {
                flag.store(true, Ordering::Relaxed);
                // The deadline is always the next tick; crossing it now
                // runs the callback, which sees the flag.
                engine.increment_epoch();
            })
        }
    }
}

// ---------------------------------------------------------------- mm-wasm

#[cfg(feature = "wasm")]
pub use abi::Abi;

#[cfg(feature = "wasm")]
mod abi {
    use std::sync::{Arc, Mutex};

    use iso_service::{OUTPUT_CAP, Request, Response, to_json, to_resume};
    use micro_monty::{HostModule, Limits, ParkReason, Progress};
    use mm_wasm_host::{Error, Runtime, Session, SessionOptions};

    use super::{Backend, Interrupter};

    /// The supported interface (`wasm/`): one instance per session through
    /// the reference host. Its async stop is a park request (no trap).
    pub struct Abi {
        runtime: Arc<Runtime>,
        session: Option<Session>,
        output: Arc<Mutex<String>>,
        park: Arc<Mutex<Option<mm_wasm_host::ParkHandle>>>,
    }

    impl Abi {
        pub fn new(runtime: Arc<Runtime>) -> Abi {
            Abi {
                runtime,
                session: None,
                output: Arc::new(Mutex::new(String::new())),
                park: Arc::new(Mutex::new(None)),
            }
        }

        /// Linear memory of the current session's instance.
        pub fn memory_bytes(&self) -> usize {
            self.session.as_ref().map_or(0, |s| usize::try_from(s.memory_bytes()).unwrap_or(usize::MAX))
        }

        fn options(memory: u64) -> SessionOptions {
            let mut o = SessionOptions::new().with_limits(Limits::new().with_memory(memory));
            o.text_limit = Some(OUTPUT_CAP as u64);
            o
        }

        fn install(&mut self, mut s: Session) {
            let sink = Arc::clone(&self.output);
            s.set_output(Box::new(move |_, text| {
                if let Ok(mut out) = sink.lock() {
                    if out.len() + text.len() <= OUTPUT_CAP {
                        out.push_str(text);
                    }
                }
            }));
            if let Ok(mut p) = self.park.lock() {
                *p = Some(s.park_handle());
            }
            self.session = Some(s);
        }

        fn take_output(&self) -> String {
            self.output
                .lock()
                .map(|mut o| std::mem::take(&mut *o))
                .unwrap_or_default()
        }

        fn respond(&self, p: Progress) -> Response {
            match p {
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

    type Answer = Result<(Response, Vec<u8>), String>;

    /// A trap loses the container (Err); every other error is a response.
    fn lift<T>(r: Result<T, Error>, ok: impl FnOnce(T) -> (Response, Vec<u8>)) -> Answer {
        match r {
            Ok(v) => Ok(ok(v)),
            Err(Error::Trap(m)) => Err(format!("trap: {m}")),
            Err(e) => Ok((
                Response::Failed {
                    message: e.to_string(),
                },
                Vec::new(),
            )),
        }
    }

    fn done() -> (Response, Vec<u8>) {
        (Response::Ok, Vec::new())
    }

    impl Backend for Abi {
        fn linear_kib(&self) -> Option<u64> {
            Some((self.memory_bytes() / 1024) as u64)
        }

        fn call(&mut self, request: &Request, raw: &[u8]) -> Answer {
            let no_session = || {
                Ok((
                    Response::Failed {
                        message: "no session".into(),
                    },
                    Vec::new(),
                ))
            };
            match request {
                Request::NewSession { memory, define } => {
                    self.session = None;
                    let mut s = match self.runtime.session(&Abi::options(*memory)) {
                        Ok(s) => s,
                        Err(e) => return lift::<()>(Err(e), |()| done()),
                    };
                    let names: Vec<&str> = define.iter().map(String::as_str).collect();
                    let r = s.define(&HostModule::untyped("", &names));
                    self.install(s);
                    lift(r, |_| done())
                }
                Request::Restore { memory } => {
                    self.session = None;
                    match self.runtime.restore(raw, &Abi::options(*memory)) {
                        Ok(s) => {
                            self.install(s);
                            Ok(done())
                        }
                        Err(e) => lift::<()>(Err(e), |()| done()),
                    }
                }
                Request::Drop => {
                    self.session = None;
                    Ok(done())
                }
                Request::Park => {
                    (self.interrupter())();
                    Ok(done())
                }
                Request::Hello => Ok((
                    Response::Failed {
                        message: "unsupported".into(),
                    },
                    Vec::new(),
                )),
                _ => {
                    let Some(mut s) = self.session.take() else {
                        return no_session();
                    };
                    let r = match request {
                        Request::Run { source, fuel } => {
                            let started = match fuel {
                                Some(_) => s.set_fuel(*fuel),
                                None => Ok(()),
                            }
                            .and_then(|()| s.start(source, "cell.py"));
                            match started {
                                Err(Error::Compile(e)) => Ok((
                                    Response::Failed {
                                        message: e.rendered,
                                    },
                                    Vec::new(),
                                )),
                                r => lift(r, |p| (self.respond(p), Vec::new())),
                            }
                        }
                        Request::Resume { resume, fuel } => {
                            let r = match fuel {
                                Some(_) => s.set_fuel(*fuel),
                                None => Ok(()),
                            }
                            .and_then(|()| s.resume(&to_resume(resume)));
                            lift(r, |p| (self.respond(p), Vec::new()))
                        }
                        Request::Snapshot => lift(s.snapshot(None), |b| {
                            (Response::Snapshot { bytes: b.len() }, b)
                        }),
                        Request::Cancel => lift(s.cancel(), |_| done()),
                        Request::Inject { op } => {
                            let op = match op.as_str() {
                                "start" => 0,
                                "resume" => 1,
                                "snapshot" => 2,
                                _ => 3,
                            };
                            lift(s.test_panic(op), |()| done())
                        }
                        _ => unreachable!("handled above"),
                    };
                    self.session = Some(s);
                    r
                }
            }
        }

        fn interrupter(&self) -> Interrupter {
            let park = Arc::clone(&self.park);
            Arc::new(move || {
                if let Ok(p) = park.lock() {
                    if let Some(h) = p.as_ref() {
                        h.request();
                    }
                }
            })
        }
    }
}
