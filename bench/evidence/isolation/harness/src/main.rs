//! Isolation-mode harness (docs/research/isolation-modes.md).
//!
//! ```text
//! iso-harness perf [SECTION...]          performance, JSON lines on stdout
//! iso-harness fault MODE CASE [K=V...]   one fault scenario, JSON verdict
//! iso-harness cold MODE                  internal: first session in a new process
//! iso-harness xchain ref|step ...         the host-API scenario as a snapshot chain (xchain.rs)
//! ```
//! `run.sh` builds every variant and runs the lot.

mod backend;
mod drive;
mod faults;
mod perf;
#[cfg(feature = "wasm")]
mod xchain;

use std::path::PathBuf;

use backend::{Backend, ChildConfig, ChildWorker, Native};

/// The repository root.
pub fn root() -> PathBuf {
    std::env::var_os("ISO_ROOT").map_or_else(
        || PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../..")),
        PathBuf::from,
    )
}

fn target() -> PathBuf {
    root().join("target/isolation")
}

/// The child worker binary (`ISO_WORKER`).
pub fn worker_path() -> PathBuf {
    std::env::var_os("ISO_WORKER")
        .map_or_else(|| target().join("release/iso-worker"), PathBuf::from)
}

/// The guest module (`ISO_GUEST`).
pub fn guest_path() -> PathBuf {
    std::env::var_os("ISO_GUEST").map_or_else(
        || target().join("wasm32-wasip1/release/iso_guest.wasm"),
        PathBuf::from,
    )
}

/// The mm-wasm v1 guest for `target` (`ISO_ABI_WASI`, `ISO_ABI_UU`).
#[cfg(feature = "wasm")]
pub fn abi_guest_path(target: &str) -> PathBuf {
    let var = if target == "wasm32-wasip1" {
        "ISO_ABI_WASI"
    } else {
        "ISO_ABI_UU"
    };
    std::env::var_os(var).map_or_else(
        || target_dir_for(target).join("mm_wasm_guest.wasm"),
        PathBuf::from,
    )
}

#[cfg(feature = "wasm")]
fn target_dir_for(triple: &str) -> PathBuf {
    target().join(triple).join("release")
}

/// The modes by name: `native`, `child`, `wasm` (recommended config) and
/// wasm variants `wasm-plain` (no epoch), `wasm-fuel`, `wasm-pool`,
/// `wasm-compile` (not precompiled).
pub enum Mode {
    Native,
    Child(ChildConfig),
    #[cfg(feature = "wasm")]
    Wasm(std::sync::Arc<backend::WasmHost>),
    /// The mm-wasm v1 interface through the reference host: `abi`
    /// (wasm32-wasip1 guest) and `abi-uu` (wasm32-unknown-unknown).
    #[cfg(feature = "wasm")]
    Abi(std::sync::Arc<mm_wasm_host::Runtime>),
}

impl Mode {
    pub fn parse(name: &str) -> Result<Mode, String> {
        Mode::parse_with(name, &[])
    }

    /// `opts`: `stack_kib=N` (child), `max_wasm_stack_kib=N`,
    /// `memory_limit_mib=N`, `epoch_ticks=N` (wasm), `prlimit=ARG` (child).
    pub fn parse_with(name: &str, opts: &[(String, String)]) -> Result<Mode, String> {
        let opt = |k: &str| {
            opts.iter()
                .find(|(key, _)| key == k)
                .map(|(_, v)| v.clone())
        };
        let num = |k: &str| opt(k).and_then(|v| v.parse::<usize>().ok());
        match name {
            "native" => Ok(Mode::Native),
            "child" => Ok(Mode::Child(ChildConfig {
                worker: worker_path(),
                stack_kib: num("stack_kib"),
                prlimit: opts
                    .iter()
                    .filter(|(k, _)| k == "prlimit")
                    .map(|(_, v)| v.clone())
                    .collect(),
            })),
            #[cfg(feature = "wasm")]
            "abi" | "abi-uu" => {
                let target = if name == "abi" {
                    "wasm32-wasip1"
                } else {
                    "wasm32-unknown-unknown"
                };
                let mut config = mm_wasm_host::RuntimeConfig::default();
                if let Some(kib) = num("max_wasm_stack_kib") {
                    config.max_wasm_stack = (kib as u64) << 10;
                }
                if let Some(mib) = num("memory_limit_mib") {
                    config.memory_limit = (mib as u64) << 20;
                }
                if let Some(ms) = num("park_grace_ms") {
                    config.park_grace = std::time::Duration::from_millis(ms as u64);
                }
                if let Some(ms) = num("call_budget_ms") {
                    config.call_budget = Some(std::time::Duration::from_millis(ms as u64));
                }
                let runtime = mm_wasm_host::Runtime::new(&abi_guest_path(target), config)
                    .map_err(|e| e.to_string())?;
                Ok(Mode::Abi(std::sync::Arc::new(runtime)))
            }
            #[cfg(feature = "wasm")]
            wasm if wasm.starts_with("wasm") => {
                let mut config = backend::WasmConfig::default();
                match wasm {
                    "wasm" => {}
                    "wasm-plain" => config.epoch = false,
                    "wasm-fuel" => {
                        config.epoch = false;
                        config.fuel = Some(u64::MAX / 4);
                    }
                    "wasm-pool" => config.pooling = true,
                    "wasm-compile" => config.precompiled = false,
                    "wasm-default" => {
                        // wasmtime's defaults: no interruption, 512 KiB stack.
                        config.epoch = false;
                        config.max_wasm_stack = 512 << 10;
                    }
                    other => return Err(format!("unknown mode {other}")),
                }
                if let Some(kib) = num("max_wasm_stack_kib") {
                    config.max_wasm_stack = kib << 10;
                }
                if let Some(mib) = num("memory_limit_mib") {
                    config.memory_limit = mib << 20;
                }
                if let Some(ticks) = num("epoch_ticks") {
                    config.epoch_deadline_ticks = ticks as u64;
                }
                if let Some(fuel) = num("wasm_fuel") {
                    config.fuel = Some(fuel as u64);
                }
                Ok(Mode::Wasm(std::sync::Arc::new(backend::WasmHost::new(
                    &guest_path(),
                    config,
                )?)))
            }
            other => Err(format!("unknown mode {other}")),
        }
    }

    /// A new, empty backend (one session's container).
    pub fn backend(&self) -> Result<Box<dyn Backend>, String> {
        Ok(match self {
            Mode::Native => Box::new(Native::new()),
            Mode::Child(config) => Box::new(ChildWorker::spawn(config)?),
            #[cfg(feature = "wasm")]
            Mode::Wasm(host) => Box::new(host.instantiate()?),
            #[cfg(feature = "wasm")]
            Mode::Abi(rt) => Box::new(backend::Abi::new(std::sync::Arc::clone(rt))),
        })
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("perf") => perf::main(&args[1..]),
        Some("cold") => perf::cold(args.get(1).map_or("native", String::as_str)),
        Some("fault") => faults::main(&args[1..]),
        #[cfg(feature = "wasm")]
        Some("xchain") => xchain::main(&args[1..]),
        _ => Err(
            "usage: iso-harness perf [SECTION...] | fault MODE CASE [K=V...] | cold MODE".into(),
        ),
    };
    if let Err(e) = result {
        eprintln!("iso-harness: {e}");
        std::process::exit(2);
    }
}
