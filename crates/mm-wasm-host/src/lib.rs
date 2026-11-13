//! The reference wasmtime host for the `mm-wasm` guest
//! (`crates/mm-wasm-guest`, docs/spec.md §10.3), with an API
//! close to `micro_monty::Session`: one [`Session`] owns one wasmtime store
//! and instance, so a trap loses that session only (one session per
//! instance, owner decision 2026-11-05). It is synchronous; an async host
//! is a separate piece of work.
//!
//! Stopping a running cell from another thread ([`ParkHandle::request`])
//! does not trap: the guest polls `mm_host.park` at every engine safepoint
//! check (every 256 safepoints) and parks there, as the native facade's
//! park flag does. The epoch deadline callback is the last resort: when a
//! requested park has not happened within the grace period (by default
//! 100 ms plus 0.75 ms per MiB of the session's memory limit: the cell is
//! inside one long native operation or collection), or a
//! call outran [`RuntimeConfig::call_budget`] (off by default, an opt-in
//! backstop), it interrupts the instance, which traps. A snapshot cannot
//! park (it collects in full first, about 1.4 ms per MiB of image [M], past
//! the grace for an image over about 210 MiB at the default limit), so a
//! request pending during [`Session::snapshot`] waits for the next call
//! instead; only `call_budget` bounds a snapshot. A restore runs before the
//! session's [`ParkHandle`] exists.
//!
//! A host that bounds CPU runs cells in fuel slices: on
//! `Parked { reason: Fuel }` it checks [`Session::fuel_wanted`] before
//! granting more, and stops the cell instead when the next native
//! operation would need more than the cell may still spend. That is how a
//! long native operation is refused before it starts; in native mode it is
//! the only defence, and here it keeps the grace interrupt rare.

use std::{
    hash::{BuildHasher, Hasher, RandomState},
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub use micro_monty::{
    CompileError, DefineError, HintGroup, HostCallRecord, HostFunctionRef, HostModule, HostValue,
    Limits, ParkReason, Phase, Progress, PyException, Resume, ResumeError, SnapshotError, Stream,
    TextCut, ZoneSource, image_build_id,
};
pub use mm_wasm_abi::SessionOptions;
use mm_wasm_abi::{
    ABI_VERSION, Reader, event, export, get_call_log, get_compile_error, get_define_error,
    get_exception, get_function_refs, get_modules, get_progress, get_resume_error,
    get_snapshot_error, get_stubs, import, phase_from_code, put_module, put_opt_bytes, put_opt_str,
    put_options, put_resume, put_str, status, text_cut_code,
};
use sha2::{Digest, Sha256};
use wasmtime::{
    Caller, Config, Engine, InstanceAllocationStrategy, InstancePre, Linker, Memory, Module,
    PoolingAllocationConfig, Store, StoreLimits, StoreLimitsBuilder, TypedFunc, UpdateDeadline,
};
use wasmtime_wasi::{WasiCtxBuilder, p1::WasiP1Ctx};

/// Why a call failed.
#[derive(Clone, Debug, PartialEq)]
pub enum Error {
    /// The instance trapped (stack exhaustion, an engine panic, a failed
    /// allocation at the store limit, an epoch interrupt). The session is
    /// gone; restore its last snapshot into a new one. From
    /// `Runtime::session` or `Runtime::restore`, an instance that could
    /// not be created (a full pool, `RuntimeConfig::pool_instances`) is a
    /// `Trap` too: no session existed yet, nothing is lost, and the call can
    /// be retried once a session is dropped.
    Trap(String),
    /// `start`: the source did not compile.
    Compile(CompileError),
    /// `resume`: the answer was refused; the session is unchanged.
    Resume(ResumeError),
    /// `reset`, `set_hint`, `set_module_dunders`, a new session.
    Py(PyException),
    /// `define`: the module was refused; the previous definition stays.
    Define(DefineError),
    /// `snapshot`, `restore`.
    Snapshot(SnapshotError),
    /// The module or a message broke the interface.
    Abi(String),
    /// `restore`: the image was written by a build this runtime has not
    /// loaded ([`Runtime::load`] its guest module to drain it). Under the
    /// upgrade policy (docs/user-guide.md, "Upgrading: drain old builds"), a cell
    /// no loaded build can resume is treated like a crash.
    UnknownBuild(BuildId),
}

/// A snapshot build id: what `mm_build_id` returns and what an image
/// carries in its bytes 10..42 (after the 8-byte magic and the 2-byte
/// format version).
pub type BuildId = [u8; 32];

fn hex(id: &BuildId) -> String {
    id.iter().map(|b| format!("{b:02x}")).collect()
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Trap(m) => write!(f, "instance trapped: {m}"),
            Error::Compile(e) => f.write_str(&e.rendered),
            Error::Resume(e) => write!(f, "{e}"),
            Error::Py(e) => write!(f, "{}: {}", e.type_name, e.message),
            Error::Define(e) => write!(f, "{e}"),
            Error::Snapshot(e) => write!(f, "{e}"),
            Error::Abi(m) => write!(f, "mm-wasm interface: {m}"),
            Error::UnknownBuild(id) => write!(
                f,
                "the snapshot was written by build {}, which this runtime has not loaded",
                hex(id)
            ),
        }
    }
}

impl std::error::Error for Error {}

/// A byte count as this platform's `usize`: past the address space is no
/// limit.
fn clamp(n: u64) -> usize {
    usize::try_from(n).unwrap_or(usize::MAX)
}

fn abi(e: impl std::fmt::Display) -> Error {
    Error::Abi(e.to_string())
}

/// How the runtime configures wasmtime.
#[derive(Clone, Debug)]
pub struct RuntimeConfig {
    /// `Config::max_wasm_stack` (wasmtime's default, 512 KiB, is too small
    /// for some native recursion).
    pub max_wasm_stack: u64,
    /// The store's linear-memory ceiling: about 3 × the guest session's
    /// `Limits::memory` + 16 MiB (the memory bound, docs/spec.md §11; a
    /// guest past it aborts instead of raising `MemoryError`). The default,
    /// 784 MiB, fits the default 256 MiB session limit.
    pub memory_limit: u64,
    /// The epoch ticker's period.
    pub epoch_tick: Duration,
    /// How long a requested park may take before the instance is
    /// interrupted (trap), plus [`RuntimeConfig::park_grace_per_mib`] for
    /// each MiB of the session's `Limits::memory`. Default 100 ms. Not
    /// applied during [`Session::snapshot`], which cannot park.
    pub park_grace: Duration,
    /// Grace added per MiB of the session's memory limit, because a
    /// garbage collection cannot park and its pause grows with the live
    /// heap: about 0.55 ms per live MiB of small containers on the guest
    /// [M]. Default 0.75 ms: 292 ms at the default 256 MiB limit.
    pub park_grace_per_mib: Duration,
    /// Longest single export call before the instance is interrupted;
    /// `None` (the default) is unlimited. An opt-in backstop: fuel slices
    /// and park requests stop cells without losing the instance.
    pub call_budget: Option<Duration>,
    /// Load `<module stem>.<key>.mmw.cwasm` instead of compiling the module
    /// at start; it is written on first use. The key is a SHA-256 of the
    /// module's bytes, wasmtime's version and configuration, so a changed
    /// module gets a new cache. The module's directory must be writable
    /// only by the host: a cache file is loaded as trusted native code.
    pub precompiled: bool,
    /// Fill each new instance's shadow stack with a pattern before its
    /// first call, so [`Session::stack_high_water`] can report the deepest
    /// stack use (tests; off by default). Needs a guest that exports
    /// `__stack_pointer`, as the shipped guest does.
    pub paint_stack: bool,
    /// Instances in wasmtime's pooling allocator, which keeps a slot's
    /// linear memory mapped after its session ends so the next session or
    /// restore reuses it, instead of mapping and faulting in fresh pages
    /// per instance. For hosts that swap many sessions per process
    /// (snapshot, then restore into a new instance): over 10 minutes at 64
    /// sessions, a 128-slot pool took 15 ms per swap instead of 144, at a
    /// peak RSS of 1.15 GB instead of 0.80 [M]. Size it near the most
    /// instances alive at once, counting a restore's new instance before
    /// the old one is dropped (two per swapping session): more than that
    /// fail to start or restore, and a much larger pool keeps more slots
    /// resident and costs more (1000 slots there: 37 ms per swap, 1.6 GB).
    /// Each slot reserves 4 GiB of address space, not memory. With every
    /// slot in use, `Runtime::session` and `Runtime::restore` fail with
    /// `Error::Trap` before any guest code runs: the image and every live
    /// session are unchanged, and the call can be retried once a session is
    /// dropped. Default `None`: wasmtime's on-demand allocator.
    pub pool_instances: Option<u32>,
    /// Bytes at the start of each pooled linear memory reset with `memset`
    /// and kept resident when its session ends; the rest is returned to the
    /// kernel. Without it a pooled slot's pages are faulted in again as on
    /// demand, at the same cost. Every slot used keeps this much resident,
    /// touched or not. Default 4 MiB; over a minute at 64 sessions, 2 MiB
    /// took 8.1 ms per swap and 8 MiB 4.2 ms at a peak RSS of 1.5 GB [M].
    pub pool_keep_resident: u64,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        RuntimeConfig {
            max_wasm_stack: 4 << 20,
            memory_limit: 784 << 20,
            epoch_tick: Duration::from_millis(10),
            park_grace: Duration::from_millis(100),
            park_grace_per_mib: Duration::from_micros(750),
            call_budget: None,
            precompiled: true,
            paint_stack: false,
            pool_instances: None,
            pool_keep_resident: 4 << 20,
        }
    }
}

/// Shared between a session and its [`ParkHandle`]s.
#[derive(Debug)]
struct ParkState {
    base: Instant,
    /// Nanoseconds since `base` (at least 1) when a park was requested, or 0.
    requested: AtomicU64,
}

impl ParkState {
    fn now(&self) -> u64 {
        u64::try_from(self.base.elapsed().as_nanos())
            .unwrap_or(u64::MAX)
            .max(1)
    }
}

/// Parks the session's running cell at its next safepoint, from any thread.
#[derive(Clone, Debug)]
pub struct ParkHandle(Arc<ParkState>);

impl ParkHandle {
    /// Requests the park (`ParkReason::Requested`); if the cell has not
    /// parked after the grace period, the instance is interrupted.
    pub fn request(&self) {
        let _ =
            self.0
                .requested
                .compare_exchange(0, self.0.now(), Ordering::SeqCst, Ordering::SeqCst);
    }
}

/// Output in production order.
pub type Output = Box<dyn FnMut(Stream, &str) + Send>;

/// A store's host state.
pub struct Ctx {
    wasi: WasiP1Ctx,
    limits: StoreLimits,
    park: Arc<ParkState>,
    /// When the current export call started, in `park.base` nanoseconds.
    call_started: u64,
    /// False during a call that cannot park (a snapshot): a park request
    /// does not interrupt it.
    parkable: bool,
    output: Option<Output>,
    zones: Option<ZoneSource>,
    /// The last zone the source answered, so the guest's second call (for
    /// data larger than its buffer) gets the same bytes.
    zone_cache: Option<(String, Option<Vec<u8>>)>,
    rng: (RandomState, u64),
    /// The bundle guest's custom sections, served to
    /// `mm_host.bundle_read` (empty for the default guest).
    bundle: Bundle,
}

/// The bundle guest's regions: its `mm.prelude` and `mm.stdlib` custom
/// sections, read once per loaded module; empty for the default guest,
/// which does not import `mm_host.bundle_read`.
type Bundle = Arc<[Vec<u8>; 2]>;

/// The bundle regions of the module at `wasm`. Only the custom sections it
/// names are read; the walk seeks past every other section.
fn bundle_of(wasm: &Path) -> std::io::Result<Bundle> {
    use std::io::{Read, Seek, SeekFrom};
    fn leb(f: &mut impl Read) -> std::io::Result<Option<u64>> {
        let (mut v, mut shift) = (0u64, 0);
        loop {
            let mut b = [0u8];
            if f.read(&mut b)? == 0 {
                return Ok(None);
            }
            v |= u64::from(b[0] & 0x7f) << shift;
            if b[0] & 0x80 == 0 {
                return Ok(Some(v));
            }
            shift += 7;
            if shift > 63 {
                return Ok(None);
            }
        }
    }
    let mut f = std::io::BufReader::new(std::fs::File::open(wasm)?);
    let mut out: [Vec<u8>; 2] = Default::default();
    f.seek(SeekFrom::Start(8))?;
    loop {
        let mut id = [0u8];
        if f.read(&mut id)? == 0 {
            break;
        }
        let Some(size) = leb(&mut f)? else { break };
        let start = f.stream_position()?;
        if id[0] == 0
            && let Some(n) = leb(&mut f)?
            && n <= 64
        {
            let mut name = vec![0u8; usize::try_from(n).unwrap_or(0)];
            f.read_exact(&mut name)?;
            let region = [mm_wasm_abi::section::PRELUDE, mm_wasm_abi::section::STDLIB]
                .iter()
                .position(|s| s.as_bytes() == name.as_slice());
            if let Some(r) = region {
                let body = size.saturating_sub(f.stream_position()? - start);
                out[r] = vec![0u8; usize::try_from(body).unwrap_or(0)];
                f.read_exact(&mut out[r])?;
            }
        }
        f.seek(SeekFrom::Start(start.saturating_add(size)))?;
    }
    Ok(Arc::new(out))
}

/// The byte [`RuntimeConfig::paint_stack`] fills the shadow stack with.
const STACK_PAINT: u8 = 0xa5;

/// `len` bytes of guest memory at `ptr`, if in bounds.
fn guest(data: &[u8], ptr: u32, len: u32) -> Option<&[u8]> {
    data.get(ptr as usize..(ptr as usize).checked_add(len as usize)?)
}

/// One loaded guest module.
struct Guest {
    pre: InstancePre<Ctx>,
    build_id: BuildId,
    bundle: Bundle,
}

/// The wasmtime engine and the loaded guest modules, keyed by build id;
/// shared by all sessions. New sessions run on the current (last loaded)
/// module; [`Runtime::restore`] picks the module whose build wrote the
/// image, so a host that upgrades keeps the old module loaded until the
/// old build's parked snapshots have drained (docs/user-guide.md, "Upgrading:
/// drain old builds").
pub struct Runtime {
    engine: Engine,
    /// Loaded guests, the current one last.
    guests: Vec<Guest>,
    config: RuntimeConfig,
    ticker_stop: Arc<AtomicBool>,
    /// The last [`Runtime::load`]'s module load time (compile, or
    /// deserialize when precompiled).
    pub load_time: Duration,
}

impl Drop for Runtime {
    fn drop(&mut self) {
        self.ticker_stop.store(true, Ordering::Relaxed);
    }
}

fn linker(engine: &Engine) -> Result<Linker<Ctx>, Error> {
    let mut l: Linker<Ctx> = Linker::new(engine);
    // wasm32-wasip1 guests import WASI for std's start-up. A default context
    // grants no files, environment or stdio (it still has clocks and
    // randomness, which the guest replaces with `mm_host` imports).
    wasmtime_wasi::p1::add_to_linker_sync(&mut l, |c: &mut Ctx| &mut c.wasi).map_err(abi)?;
    let m = mm_wasm_abi::IMPORT_MODULE;
    l.func_wrap(m, import::NOW_NS, |c: Caller<'_, Ctx>, _s: u32| -> i64 {
        i64::try_from(c.data().park.base.elapsed().as_nanos()).unwrap_or(i64::MAX - 2)
    })
    .map_err(abi)?;
    l.func_wrap(
        m,
        import::PARK,
        |c: Caller<'_, Ctx>, _s: u32, clear: u32| -> u32 {
            let r = &c.data().park.requested;
            let v = if clear != 0 {
                r.swap(0, Ordering::SeqCst)
            } else {
                r.load(Ordering::SeqCst)
            };
            u32::from(v != 0)
        },
    )
    .map_err(abi)?;
    l.func_wrap(
        m,
        import::WALL_CLOCK,
        |_: Caller<'_, Ctx>, _s: u32| -> f64 {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0.0, |d| d.as_secs_f64())
        },
    )
    .map_err(abi)?;
    l.func_wrap(
        m,
        import::ENTROPY,
        |mut c: Caller<'_, Ctx>, _s: u32, ptr: u32, len: u32| {
            let Some(mem) = c.get_export("memory").and_then(|e| e.into_memory()) else {
                return;
            };
            let (data, ctx) = mem.data_and_store_mut(&mut c);
            let Some(buf) = data.get_mut(ptr as usize..(ptr as usize).saturating_add(len as usize))
            else {
                return;
            };
            for chunk in buf.chunks_mut(8) {
                ctx.rng.1 += 1;
                let mut h = ctx.rng.0.build_hasher();
                h.write_u64(ctx.rng.1);
                chunk.copy_from_slice(&h.finish().to_le_bytes()[..chunk.len()]);
            }
        },
    )
    .map_err(abi)?;
    l.func_wrap(
        m,
        import::BUNDLE_READ,
        |mut c: Caller<'_, Ctx>, region: u32, offset: u32, ptr: u32, len: u32| -> u32 {
            let Some(mem) = c.get_export("memory").and_then(|e| e.into_memory()) else {
                return 0;
            };
            let (data, ctx) = mem.data_and_store_mut(&mut c);
            let src = ctx.bundle.get(region as usize).and_then(|r| {
                r.get(offset as usize..(offset as usize).checked_add(len as usize)?)
            });
            let dst = data.get_mut(ptr as usize..(ptr as usize).saturating_add(len as usize));
            match (src, dst) {
                (Some(s), Some(d)) => {
                    d.copy_from_slice(s);
                    1
                }
                _ => 0,
            }
        },
    )
    .map_err(abi)?;
    l.func_wrap(
        m,
        import::OUTPUT,
        |mut c: Caller<'_, Ctx>, _s: u32, stream: u32, ptr: u32, len: u32| {
            let Some(mem) = c.get_export("memory").and_then(|e| e.into_memory()) else {
                return;
            };
            let (data, ctx) = mem.data_and_store_mut(&mut c);
            let text = data
                .get(ptr as usize..(ptr as usize).saturating_add(len as usize))
                .and_then(|b| std::str::from_utf8(b).ok());
            if let (Some(text), Some(out)) = (text, ctx.output.as_mut()) {
                out(
                    if stream == 1 {
                        Stream::Stderr
                    } else {
                        Stream::Stdout
                    },
                    text,
                );
            }
        },
    )
    .map_err(abi)?;
    l.func_wrap(
        m,
        import::ZONE,
        |mut c: Caller<'_, Ctx>, _s: u32, key: u32, key_len: u32, buf: u32, cap: u32| -> i64 {
            let Some(mem) = c.get_export("memory").and_then(|e| e.into_memory()) else {
                return -1;
            };
            let (data, ctx) = mem.data_and_store_mut(&mut c);
            let Some(key) = guest(data, key, key_len).and_then(|k| std::str::from_utf8(k).ok())
            else {
                return -1;
            };
            let cached = ctx.zone_cache.as_ref().filter(|(k, _)| k == key);
            let tzif = match cached {
                Some((_, tzif)) => tzif.clone(),
                None => {
                    let tzif = ctx.zones.as_mut().and_then(|source| source(key));
                    ctx.zone_cache = Some((key.to_owned(), tzif.clone()));
                    tzif
                }
            };
            let Some(tzif) = tzif else {
                return -1;
            };
            if tzif.len() <= cap as usize {
                let Some(out) = data.get_mut(buf as usize..buf as usize + tzif.len()) else {
                    return -1;
                };
                out.copy_from_slice(&tzif);
            }
            i64::try_from(tzif.len()).unwrap_or(-1)
        },
    )
    .map_err(abi)?;
    Ok(l)
}

impl Runtime {
    /// A runtime with the guest module at `wasm` (either target) as its
    /// current module.
    pub fn new(wasm: &Path, config: RuntimeConfig) -> Result<Runtime, Error> {
        let mut c = Config::new();
        c.epoch_interruption(true);
        let stack = clamp(config.max_wasm_stack);
        c.max_wasm_stack(stack);
        // Validated against max_wasm_stack even though calls are sync.
        c.async_stack_size(stack.saturating_add(1 << 20));
        if let Some(n) = config.pool_instances {
            let mut p = PoolingAllocationConfig::new();
            let keep = clamp(config.pool_keep_resident);
            p.total_core_instances(n)
                .total_memories(n)
                .total_tables(n)
                .max_memory_size(clamp(config.memory_limit.min(4 << 30)))
                .linear_memory_keep_resident(keep)
                .table_keep_resident(keep);
            c.allocation_strategy(InstanceAllocationStrategy::Pooling(p));
        }
        let engine = Engine::new(&c).map_err(abi)?;
        let ticker_stop = Arc::new(AtomicBool::new(false));
        {
            let (engine, stop, tick) =
                (engine.clone(), Arc::clone(&ticker_stop), config.epoch_tick);
            std::thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    std::thread::sleep(tick);
                    engine.increment_epoch();
                }
            });
        }
        let mut runtime = Runtime {
            engine,
            guests: Vec::new(),
            config,
            ticker_stop,
            load_time: Duration::ZERO,
        };
        runtime.load(wasm)?;
        Ok(runtime)
    }

    /// Loads the guest module at `wasm` (each module path has its own
    /// precompiled `.cwasm`) and makes it current: new sessions run on it.
    /// Modules loaded earlier stay loaded for [`Runtime::restore`]. Returns
    /// its build id; loading a build that is already loaded replaces that
    /// module.
    pub fn load(&mut self, wasm: &Path) -> Result<BuildId, Error> {
        let (pre, load_time) = self.compile(wasm)?;
        // The bundle guest's bytecode, read once; a bundle guest without
        // both sections is refused here, not at its first import.
        let needs_bundle = pre
            .module()
            .imports()
            .any(|i| i.module() == mm_wasm_abi::IMPORT_MODULE && i.name() == import::BUNDLE_READ);
        let bundle = if needs_bundle {
            let b = bundle_of(wasm).map_err(abi)?;
            if b.iter().any(Vec::is_empty) {
                return Err(abi(format!(
                    "the module imports {}.{} but lacks its {} or {} section",
                    mm_wasm_abi::IMPORT_MODULE,
                    import::BUNDLE_READ,
                    mm_wasm_abi::section::PRELUDE,
                    mm_wasm_abi::section::STDLIB
                )));
            }
            b
        } else {
            Bundle::default()
        };
        // No session: only the build id is read.
        let id = self.instance(&pre, &bundle, 0)?.build_id()?;
        let build_id: BuildId = id
            .try_into()
            .map_err(|_| abi("mm_build_id is not 32 bytes"))?;
        self.guests.retain(|g| g.build_id != build_id);
        self.guests.push(Guest {
            pre,
            build_id,
            bundle,
        });
        self.load_time = load_time;
        Ok(build_id)
    }

    /// Unloads build `id` once its snapshots have drained. The current
    /// build stays (load another first); returns whether `id` was unloaded.
    pub fn unload(&mut self, id: &BuildId) -> bool {
        if self.current_build() == *id {
            return false;
        }
        let before = self.guests.len();
        self.guests.retain(|g| g.build_id != *id);
        self.guests.len() < before
    }

    /// The build new sessions run on.
    #[must_use]
    pub fn current_build(&self) -> BuildId {
        self.guests.last().map_or([0; 32], |g| g.build_id)
    }

    /// Every loaded build, the current one last.
    #[must_use]
    pub fn builds(&self) -> Vec<BuildId> {
        self.guests.iter().map(|g| g.build_id).collect()
    }

    fn current(&self) -> Result<&Guest, Error> {
        self.guests.last().ok_or_else(|| abi("no module loaded"))
    }

    fn compile(&self, wasm: &Path) -> Result<(InstancePre<Ctx>, Duration), Error> {
        let engine = &self.engine;
        let start = Instant::now();
        let module = if self.config.precompiled {
            // The cache is keyed on content: SHA-256 over the module's
            // bytes and `precompile_compatibility_hash` (target, compiler
            // flags, tunables, features and wasmtime's version). The key
            // names the file. The module is hashed in chunks and read whole
            // only to compile it: a 2.4 MB read on every load stayed
            // resident under allocators that keep freed memory (macOS's),
            // about +2.4 MiB per runtime [M].
            struct Sha(Sha256);
            impl std::hash::Hasher for Sha {
                fn write(&mut self, b: &[u8]) {
                    self.0.update(b);
                }
                fn finish(&self) -> u64 {
                    0
                }
            }
            let mut sha = Sha(Sha256::new());
            {
                use std::io::Read;
                let mut file = std::fs::File::open(wasm).map_err(abi)?;
                let len = file.metadata().map_err(abi)?.len();
                sha.0.update(len.to_le_bytes());
                let (mut buf, mut read) = (vec![0u8; 64 << 10], 0u64);
                loop {
                    let n = file.read(&mut buf).map_err(abi)?;
                    if n == 0 {
                        break;
                    }
                    sha.0.update(&buf[..n]);
                    read += n as u64;
                }
                if read != len {
                    return Err(abi("the module changed while it was hashed"));
                }
            }
            std::hash::Hash::hash(&engine.precompile_compatibility_hash(), &mut sha);
            let key: [u8; 32] = sha.0.finalize().into();
            let hex: String = key.iter().map(|b| format!("{b:02x}")).collect();
            let cwasm = wasm.with_extension(format!("{hex}.mmw.cwasm"));
            // Loaded from the file, not from bytes: wasmtime then maps the
            // file and builds each instance's linear memory copy-on-write
            // from it. From bytes, macOS has no copy-on-write source (no
            // memfd), so every instance copied the 600 KB data segment
            // eagerly (+470 KiB idle RSS per instance [M]).
            // SAFETY: trust model: the cache directory (the module's) must be
            // writable only by this host, so every `.cwasm` there is what
            // `precompile_module` wrote, and files are replaced only by
            // rename, never modified in place (the mapping stays valid). The
            // key in the name (module content, wasmtime version and
            // configuration) guards against a stale or mismatched cache, not
            // a hostile writer: native code from a writer to that directory
            // is not verified and is trusted. wasmtime checks its own
            // version and configuration again in the file.
            #[allow(unsafe_code, reason = "loads the precompiled module this host wrote")]
            let load = |p: &Path| unsafe { Module::deserialize_file(engine, p) };
            let start = Instant::now();
            // A missing or unreadable cache is compiled again.
            let m = match load(&cwasm) {
                Ok(m) => m,
                Err(_) => {
                    let bytes = std::fs::read(wasm).map_err(abi)?;
                    let compiled = engine.precompile_module(&bytes).map_err(abi)?;
                    // Written beside it and renamed, so a concurrent loader
                    // never reads a partial file.
                    static TMP: AtomicU64 = AtomicU64::new(0);
                    let n = TMP.fetch_add(1, Ordering::Relaxed);
                    let tmp = cwasm.with_extension(format!("{}.{n}.tmp", std::process::id()));
                    std::fs::write(&tmp, &compiled).map_err(abi)?;
                    std::fs::rename(&tmp, &cwasm).map_err(abi)?;
                    load(&cwasm).map_err(abi)?
                }
            };
            (m, start.elapsed())
        } else {
            (
                Module::from_file(engine, wasm).map_err(abi)?,
                start.elapsed(),
            )
        };
        let (module, load_time) = module;
        for name in export::REQUIRED {
            if module.get_export(name).is_none() {
                return Err(abi(format!(
                    "the module lacks export {name}: not an mm-wasm v{ABI_VERSION} guest"
                )));
            }
        }
        let pre = linker(engine)?.instantiate_pre(&module).map_err(abi)?;
        Ok((pre, load_time))
    }

    /// A new session in a new instance of the current module.
    pub fn session(&self, options: &SessionOptions) -> Result<Session, Error> {
        let g = self.current()?;
        let mut s = self.instance(&g.pre, &g.bundle, options.limits.memory)?;
        let mut input = Vec::new();
        put_options(&mut input, options);
        s.write(&input)?;
        let id = s.call(|s| s.f.session_new.call(&mut s.store, ()))?;
        s.adopt(id, false)
    }

    /// A session restored from `image` in a new instance of the module
    /// whose build wrote it ([`image_build_id`]); [`Error::UnknownBuild`]
    /// when that build is not loaded. The limits and host-side settings of
    /// `options` apply; the text limit, the last expression's value and
    /// host-call conversion come from the image.
    pub fn restore(&self, image: &[u8], options: &SessionOptions) -> Result<Session, Error> {
        let guest = match image_build_id(image) {
            Some(id) => self
                .guests
                .iter()
                .find(|g| g.build_id == id)
                .ok_or(Error::UnknownBuild(id))?,
            // Not an image: the current module reports it as corrupt.
            None => self.current()?,
        };
        let mut s = self.instance(&guest.pre, &guest.bundle, options.limits.memory)?;
        let mut input = Vec::with_capacity(64 + image.len());
        put_options(&mut input, options);
        input.extend_from_slice(image);
        s.write(&input)?;
        let id = s.call(|s| s.f.session_restore.call(&mut s.store, ()))?;
        s.adopt(id, true)
    }

    /// A new instance for a session whose memory limit is `memory` bytes.
    fn instance(
        &self,
        pre: &InstancePre<Ctx>,
        bundle: &Bundle,
        memory: u64,
    ) -> Result<Session, Error> {
        let park = Arc::new(ParkState {
            base: Instant::now(),
            requested: AtomicU64::new(0),
        });
        let ctx = Ctx {
            wasi: WasiCtxBuilder::new().build_p1(),
            limits: StoreLimitsBuilder::new()
                .memory_size(clamp(self.config.memory_limit))
                .instances(1)
                .build(),
            park: Arc::clone(&park),
            call_started: 0,
            parkable: true,
            output: None,
            zones: None,
            zone_cache: None,
            rng: (RandomState::new(), 0),
            bundle: Arc::clone(bundle),
        };
        let mut store = Store::new(&self.engine, ctx);
        store.limiter(|c| &mut c.limits);
        store.set_epoch_deadline(1);
        let per_mib = self.config.park_grace_per_mib.as_nanos();
        let grace = self
            .config
            .park_grace
            .as_nanos()
            .saturating_add(per_mib.saturating_mul(u128::from(memory >> 20)));
        let grace = u64::try_from(grace).unwrap_or(u64::MAX);
        let budget = self
            .config
            .call_budget
            .map(|b| u64::try_from(b.as_nanos()).unwrap_or(u64::MAX));
        // Checked every tick: trap only for a park that did not happen
        // within the grace period, or a call past its budget.
        store.epoch_deadline_callback(move |state| {
            let c = state.data();
            let now = c.park.now();
            let asked = c.park.requested.load(Ordering::SeqCst);
            if (c.parkable && asked != 0 && now.saturating_sub(asked.max(c.call_started)) >= grace)
                || budget.is_some_and(|b| now.saturating_sub(c.call_started) >= b)
            {
                Ok(UpdateDeadline::Interrupt)
            } else {
                Ok(UpdateDeadline::Continue(1))
            }
        });
        let instance = pre
            .instantiate(&mut store)
            .map_err(|e| Error::Trap(format!("{e:#}")))?;
        // The stack grows down from its top toward address 0.
        let stack_top = instance
            .get_global(&mut store, "__stack_pointer")
            .and_then(|g| g.get(&mut store).i32())
            .and_then(|top| usize::try_from(top).ok())
            .filter(|_| self.config.paint_stack);
        if let (Some(top), Some(m)) = (stack_top, instance.get_memory(&mut store, "memory"))
            && let Some(stack) = m.data_mut(&mut store).get_mut(..top)
        {
            stack.fill(STACK_PAINT);
        }
        if let Ok(init) = instance.get_typed_func::<(), ()>(&mut store, "_initialize") {
            init.call(&mut store, ())
                .map_err(|e| Error::Trap(format!("{e:#}")))?;
        }
        macro_rules! f {
            ($name:expr) => {
                instance
                    .get_typed_func(&mut store, $name)
                    .map_err(|e| abi(format!("{}: {e}", $name)))?
            };
        }
        let version: TypedFunc<(), u32> = f!(export::ABI_VERSION);
        let got = version
            .call(&mut store, ())
            .map_err(|e| Error::Trap(format!("{e:#}")))?;
        if got != ABI_VERSION {
            return Err(abi(format!(
                "guest speaks mm-wasm {got}, this host {ABI_VERSION}"
            )));
        }
        let f = Funcs {
            input: f!(export::INPUT),
            output: f!(export::OUTPUT),
            build_id: f!(export::BUILD_ID),
            session_new: f!(export::SESSION_NEW),
            session_restore: f!(export::SESSION_RESTORE),
            define: f!(export::DEFINE),
            start: f!(export::START),
            resume: f!(export::RESUME),
            cancel: f!(export::CANCEL),
            reset: f!(export::RESET),
            snapshot: f!(export::SNAPSHOT),
            set_fuel: f!(export::SET_FUEL),
            fuel: f!(export::FUEL),
            fuel_wanted: f!(export::FUEL_WANTED),
            set_fuel_total: f!(export::SET_FUEL_TOTAL),
            fuel_total: f!(export::FUEL_TOTAL),
            phase: f!(export::PHASE),
            set_deadline: f!(export::SET_DEADLINE),
            set_hint: f!(export::SET_HINT),
            set_text_cut: f!(export::SET_TEXT_CUT),
            set_local_timezone: f!(export::SET_LOCAL_TIMEZONE),
            call_log: f!(export::CALL_LOG),
            environment: f!(export::ENVIRONMENT),
            stubs: f!(export::STUBS),
            host_modules: f!(export::HOST_MODULES),
            set_module_dunders: f!(export::SET_MODULE_DUNDERS),
            set_int_bits_text: f!(export::SET_INT_BITS_TEXT),
            set_gc_stress: f!(export::SET_GC_STRESS),
            pending: f!(export::PENDING),
            set_gc_yield: instance
                .get_typed_func(&mut store, export::SET_GC_YIELD)
                .ok(),
            test_panic: instance.get_typed_func(&mut store, export::TEST_PANIC).ok(),
        };
        let memory = instance
            .get_memory(&mut store, "memory")
            .ok_or_else(|| abi("no memory export"))?;
        Ok(Session {
            store,
            f,
            memory,
            id: 0,
            dead: None,
            park: ParkHandle(park),
            stack_top,
        })
    }
}

struct Funcs {
    input: TypedFunc<u32, u32>,
    output: TypedFunc<(), u64>,
    build_id: TypedFunc<(), u32>,
    session_new: TypedFunc<(), i32>,
    session_restore: TypedFunc<(), i32>,
    define: TypedFunc<u32, u32>,
    start: TypedFunc<u32, u32>,
    resume: TypedFunc<u32, u32>,
    cancel: TypedFunc<u32, u32>,
    reset: TypedFunc<u32, u32>,
    snapshot: TypedFunc<(u32, u64), u32>,
    set_fuel: TypedFunc<(u32, u64), u32>,
    fuel: TypedFunc<u32, u64>,
    fuel_wanted: TypedFunc<u32, u64>,
    set_fuel_total: TypedFunc<(u32, u64), u32>,
    fuel_total: TypedFunc<u32, u64>,
    phase: TypedFunc<u32, u32>,
    set_deadline: TypedFunc<(u32, u64), u32>,
    set_hint: TypedFunc<u32, u32>,
    set_text_cut: TypedFunc<(u32, u32), u32>,
    set_local_timezone: TypedFunc<u32, u32>,
    call_log: TypedFunc<u32, u32>,
    environment: TypedFunc<u32, u32>,
    stubs: TypedFunc<u32, u32>,
    host_modules: TypedFunc<u32, u32>,
    set_module_dunders: TypedFunc<u32, u32>,
    set_int_bits_text: TypedFunc<(u32, u32), u32>,
    set_gc_stress: TypedFunc<(u32, u32), u32>,
    pending: TypedFunc<u32, u32>,
    set_gc_yield: Option<TypedFunc<(u32, u32), u32>>,
    test_panic: Option<TypedFunc<u32, u32>>,
}

/// One micro-monty session in its own wasm instance.
pub struct Session {
    store: Store<Ctx>,
    f: Funcs,
    memory: Memory,
    id: u32,
    dead: Option<String>,
    park: ParkHandle,
    /// The shadow stack's top, when it was painted.
    stack_top: Option<usize>,
}

impl Session {
    fn call<T>(&mut self, f: impl FnOnce(&mut Session) -> wasmtime::Result<T>) -> Result<T, Error> {
        if let Some(d) = &self.dead {
            return Err(Error::Trap(d.clone()));
        }
        let now = self.store.data().park.now();
        self.store.data_mut().call_started = now;
        f(self).map_err(|e| {
            let message = match e.downcast_ref::<wasmtime::Trap>() {
                Some(trap) => trap.to_string(),
                None => format!("{e:#}"),
            };
            self.dead = Some(message.clone());
            Error::Trap(message)
        })
    }

    fn write(&mut self, bytes: &[u8]) -> Result<(), Error> {
        let len = u32::try_from(bytes.len()).map_err(abi)?;
        let at = self.call(|s| s.f.input.call(&mut s.store, len))?;
        self.memory
            .write(&mut self.store, at as usize, bytes)
            .map_err(abi)
    }

    fn read(&mut self) -> Result<Vec<u8>, Error> {
        let packed = self.call(|s| s.f.output.call(&mut s.store, ()))?;
        let (at, len) = ((packed >> 32) as usize, (packed & 0xffff_ffff) as usize);
        self.memory
            .data(&self.store)
            .get(at..at.saturating_add(len))
            .map(<[u8]>::to_vec)
            .ok_or_else(|| abi("output out of bounds"))
    }

    fn adopt(mut self, id: i32, restore: bool) -> Result<Session, Error> {
        if id >= 0 {
            self.id = id.cast_unsigned();
            return Ok(self);
        }
        let out = self.read()?;
        let r = &mut Reader::new(&out);
        Err(match (id, restore) {
            (-1, false) => Error::Py(get_exception(r).map_err(|e| abi(e.0))?),
            (-1, true) => Error::Snapshot(get_snapshot_error(r).map_err(|e| abi(e.0))?),
            _ => Error::Abi(r.string().unwrap_or_default()),
        })
    }

    fn failure(&mut self, code: u32) -> Error {
        let out = match self.read() {
            Ok(out) => out,
            Err(e) => return e,
        };
        let r = &mut Reader::new(&out);
        let decoded = match code {
            status::PY_ERROR => get_exception(r).map(Error::Py),
            status::SNAPSHOT_ERROR => get_snapshot_error(r).map(Error::Snapshot),
            status::DEFINE_ERROR => get_define_error(r).map(Error::Define),
            _ => r.string().map(Error::Abi),
        };
        decoded.unwrap_or_else(|e| abi(e.0))
    }

    fn progress(&mut self, code: u32) -> Result<Progress, Error> {
        let out = self.read()?;
        let r = &mut Reader::new(&out);
        match code {
            event::COMPILE_ERROR => {
                Err(Error::Compile(get_compile_error(r).map_err(|e| abi(e.0))?))
            }
            event::RESUME_ERROR => Err(Error::Resume(get_resume_error(r).map_err(|e| abi(e.0))?)),
            event::BAD_REQUEST => Err(Error::Abi(r.string().unwrap_or_default())),
            _ => get_progress(code, r).map_err(|e| abi(e.0)),
        }
    }

    /// Whether a trap ended this session.
    #[must_use]
    pub fn is_dead(&self) -> bool {
        self.dead.is_some()
    }

    /// The guest's snapshot build id.
    pub fn build_id(&mut self) -> Result<Vec<u8>, Error> {
        self.call(|s| s.f.build_id.call(&mut s.store, ()))?;
        self.read()
    }

    /// Where `print` and `sys.stdout`/`stderr` writes go. Host-side, so
    /// set it again after a restore.
    pub fn set_output(&mut self, out: Output) {
        self.store.data_mut().output = Some(out);
    }

    /// `Session::define`.
    pub fn define(&mut self, module: &HostModule) -> Result<Vec<HostFunctionRef>, Error> {
        let mut input = Vec::new();
        put_module(&mut input, module);
        self.write(&input)?;
        let id = self.id;
        match self.call(|s| s.f.define.call(&mut s.store, id))? {
            status::OK => {
                let out = self.read()?;
                get_function_refs(&mut Reader::new(&out)).map_err(|e| abi(e.0))
            }
            code => Err(self.failure(code)),
        }
    }

    /// Compiles `source` in the guest and starts it (compile + `start`).
    pub fn start(&mut self, source: &str, filename: &str) -> Result<Progress, Error> {
        let mut input = Vec::with_capacity(source.len() + filename.len() + 8);
        put_str(&mut input, filename);
        put_str(&mut input, source);
        self.write(&input)?;
        let id = self.id;
        let code = self.call(|s| s.f.start.call(&mut s.store, id))?;
        self.progress(code)
    }

    /// `Session::resume`.
    pub fn resume(&mut self, resume: &Resume) -> Result<Progress, Error> {
        let mut input = Vec::new();
        put_resume(&mut input, resume);
        self.write(&input)?;
        let id = self.id;
        let code = self.call(|s| s.f.resume.call(&mut s.store, id))?;
        self.progress(code)
    }

    /// `Session::cancel`.
    pub fn cancel(&mut self) -> Result<bool, Error> {
        let id = self.id;
        Ok(self.call(|s| s.f.cancel.call(&mut s.store, id))? != 0)
    }

    /// `Session::reset`.
    pub fn reset(&mut self) -> Result<(), Error> {
        let id = self.id;
        match self.call(|s| s.f.reset.call(&mut s.store, id))? {
            status::OK => Ok(()),
            code => Err(self.failure(code)),
        }
    }

    /// `Session::snapshot` (`max_bytes` `None`: capped at the memory limit).
    pub fn snapshot(&mut self, max_bytes: Option<u64>) -> Result<Vec<u8>, Error> {
        let (id, max) = (self.id, max_bytes.unwrap_or(u64::MAX));
        // A snapshot cannot park: a request waits for the next call.
        self.store.data_mut().parkable = false;
        let r = self.call(|s| s.f.snapshot.call(&mut s.store, (id, max)));
        self.store.data_mut().parkable = true;
        match r? {
            status::OK => self.read(),
            code => Err(self.failure(code)),
        }
    }

    /// `Session::set_fuel`.
    pub fn set_fuel(&mut self, fuel: Option<u64>) -> Result<(), Error> {
        let (id, f) = (self.id, fuel.unwrap_or(u64::MAX));
        self.call(|s| s.f.set_fuel.call(&mut s.store, (id, f)))?;
        Ok(())
    }

    /// `Session::fuel`.
    pub fn fuel(&mut self) -> Result<Option<u64>, Error> {
        let id = self.id;
        Ok(Some(self.call(|s| s.f.fuel.call(&mut s.store, id))?).filter(|f| *f != u64::MAX))
    }

    /// `Session::fuel_wanted`: after `Parked { reason: Fuel }`, the fuel
    /// the next native operation will charge when that is more than was
    /// left (0 after an ordinary fuel park). Check it before granting
    /// another fuel slice.
    pub fn fuel_wanted(&mut self) -> Result<u64, Error> {
        let id = self.id;
        self.call(|s| s.f.fuel_wanted.call(&mut s.store, id))
    }

    /// `Session::set_fuel_total`.
    pub fn set_fuel_total(&mut self, total: Option<u64>) -> Result<(), Error> {
        let (id, t) = (self.id, total.unwrap_or(u64::MAX));
        self.call(|s| s.f.set_fuel_total.call(&mut s.store, (id, t)))?;
        Ok(())
    }

    /// `Session::fuel_total`.
    pub fn fuel_total(&mut self) -> Result<Option<u64>, Error> {
        let id = self.id;
        Ok(Some(self.call(|s| s.f.fuel_total.call(&mut s.store, id))?).filter(|t| *t != u64::MAX))
    }

    /// `Session::pending`: the stop the session is at, as it was reported
    /// (a restored session's too).
    pub fn pending(&mut self) -> Result<Option<Progress>, Error> {
        let id = self.id;
        let code = self.call(|s| s.f.pending.call(&mut s.store, id))?;
        match self.progress(code)? {
            Progress::Complete(None) => Ok(None),
            p => Ok(Some(p)),
        }
    }

    /// `Session::phase`.
    pub fn phase(&mut self) -> Result<Phase, Error> {
        let id = self.id;
        let code = self.call(|s| s.f.phase.call(&mut s.store, id))?;
        phase_from_code(code).map_err(|e| abi(e.0))
    }

    /// `Session::set_hint`.
    pub fn set_hint(&mut self, group: HintGroup, text: Option<&str>) -> Result<(), Error> {
        let mut input = Vec::new();
        put_str(&mut input, group.key());
        put_opt_str(&mut input, text);
        self.write(&input)?;
        let id = self.id;
        match self.call(|s| s.f.set_hint.call(&mut s.store, id))? {
            status::OK => Ok(()),
            code => Err(self.failure(code)),
        }
    }

    /// `Session::set_text_cut`.
    pub fn set_text_cut(&mut self, cut: TextCut) -> Result<(), Error> {
        let (id, code) = (self.id, text_cut_code(cut));
        self.call(|s| s.f.set_text_cut.call(&mut s.store, (id, code)))?;
        Ok(())
    }

    /// `Session::set_local_timezone` (session state: snapshots carry it).
    pub fn set_local_timezone(&mut self, tzif: Option<&[u8]>) -> Result<(), Error> {
        let mut input = Vec::new();
        put_opt_bytes(&mut input, tzif);
        self.write(&input)?;
        let id = self.id;
        self.call(|s| s.f.set_local_timezone.call(&mut s.store, id))?;
        Ok(())
    }

    /// `Session::set_zone_source`. Host-side, so set it again after a
    /// restore; it answers only when [`SessionOptions::zone_source`] was on.
    pub fn set_zone_source(&mut self, source: Option<ZoneSource>) {
        let ctx = self.store.data_mut();
        ctx.zones = source;
        ctx.zone_cache = None;
    }

    /// `Session::call_log`.
    pub fn call_log(&mut self) -> Result<Vec<HostCallRecord>, Error> {
        let id = self.id;
        self.call(|s| s.f.call_log.call(&mut s.store, id))?;
        let out = self.read()?;
        get_call_log(&mut Reader::new(&out)).map_err(|e| abi(e.0))
    }

    /// `Session::environment`: this session's limits and fuel total.
    pub fn environment(&mut self) -> Result<String, Error> {
        let id = self.id;
        self.call(|s| s.f.environment.call(&mut s.store, id))?;
        let out = self.read()?;
        Reader::new(&out).string().map_err(|e| abi(e.0))
    }

    /// `Session::stubs`.
    pub fn stubs(&mut self) -> Result<Vec<(String, String)>, Error> {
        let id = self.id;
        self.call(|s| s.f.stubs.call(&mut s.store, id))?;
        let out = self.read()?;
        get_stubs(&mut Reader::new(&out)).map_err(|e| abi(e.0))
    }

    /// `Session::host_modules`.
    pub fn host_modules(&mut self) -> Result<Vec<HostModule>, Error> {
        let id = self.id;
        self.call(|s| s.f.host_modules.call(&mut s.store, id))?;
        let out = self.read()?;
        get_modules(&mut Reader::new(&out)).map_err(|e| abi(e.0))
    }

    /// `Session::set_module_dunders`.
    pub fn set_module_dunders(
        &mut self,
        name: Option<&str>,
        file: Option<&str>,
    ) -> Result<(), Error> {
        let mut input = Vec::new();
        put_opt_str(&mut input, name);
        put_opt_str(&mut input, file);
        self.write(&input)?;
        let id = self.id;
        match self.call(|s| s.f.set_module_dunders.call(&mut s.store, id))? {
            status::OK => Ok(()),
            code => Err(self.failure(code)),
        }
    }

    /// `Session::set_int_bits_text`.
    pub fn set_int_bits_text(&mut self, on: bool) -> Result<(), Error> {
        let (id, on) = (self.id, u32::from(on));
        self.call(|s| s.f.set_int_bits_text.call(&mut s.store, (id, on)))?;
        Ok(())
    }

    /// `Session::set_gc_stress` (engine testing; very slow).
    pub fn set_gc_stress(&mut self, on: bool) -> Result<(), Error> {
        let (id, on) = (self.id, u32::from(on));
        self.call(|s| s.f.set_gc_stress.call(&mut s.store, (id, on)))?;
        Ok(())
    }

    /// `Session::set_gc_yield`, when the guest has the optional export;
    /// whether it has it.
    pub fn set_gc_yield(&mut self, on: bool) -> Result<bool, Error> {
        let Some(f) = self.f.set_gc_yield.clone() else {
            return Ok(false);
        };
        let (id, on) = (self.id, u32::from(on));
        self.call(|s| f.call(&mut s.store, (id, on)))?;
        Ok(true)
    }

    /// Parks with `Deadline` at the first safepoint `after` from now
    /// (`None` clears it). One-shot, as `Session::set_deadline`.
    pub fn set_deadline(&mut self, after: Option<Duration>) -> Result<(), Error> {
        let at = after.map_or(u64::MAX, |d| {
            let now = self.store.data().park.base.elapsed() + d;
            u64::try_from(now.as_nanos()).unwrap_or(u64::MAX - 1)
        });
        let id = self.id;
        self.call(|s| s.f.set_deadline.call(&mut s.store, (id, at)))?;
        Ok(())
    }

    /// A handle that parks the running cell from another thread.
    #[must_use]
    pub fn park_handle(&self) -> ParkHandle {
        self.park.clone()
    }

    /// The deepest shadow-stack use since the instance started and the
    /// stack's size, in bytes, with [`RuntimeConfig::paint_stack`]: the
    /// stack below the lowest byte that no longer holds the paint. `None`
    /// without it.
    #[must_use]
    pub fn stack_high_water(&self) -> Option<(u64, u64)> {
        let top = self.stack_top?;
        let stack = self.memory.data(&self.store).get(..top)?;
        let untouched = stack.iter().take_while(|&&b| b == STACK_PAINT).count();
        Some((top.checked_sub(untouched)? as u64, top as u64))
    }

    /// Linear memory in bytes.
    #[must_use]
    pub fn memory_bytes(&self) -> u64 {
        u64::try_from(self.memory.data_size(&self.store)).unwrap_or(u64::MAX)
    }

    /// Fault injection, when the guest was built with `test-hooks`.
    pub fn test_panic(&mut self, op: u32) -> Result<(), Error> {
        let Some(f) = self.f.test_panic.clone() else {
            return Err(abi("guest built without test-hooks"));
        };
        self.call(|s| f.call(&mut s.store, op))?;
        Ok(())
    }
}
