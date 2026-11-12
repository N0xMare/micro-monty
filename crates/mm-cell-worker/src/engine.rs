//! The engine behind the worker loop: one session per worker, driven
//! through [`CellSession`]. [`NativeEngine`] links micro-monty; a host can
//! supply another implementation (plak's in-process backend wraps
//! `mm-wasm-host`'s session in it), so this crate stays free of wasmtime.

use std::{
    sync::{Arc, Mutex},
    time::Instant,
};

use micro_monty::{Engine, EngineConfig, HostArgs, HostModule, Limits, Progress, Resume, Session};

use crate::lock;

/// Requests a park of the running cell, from any thread.
pub type Park = Arc<dyn Fn() + Send + Sync>;

/// How the worker sets up each session it asks a [`CellEngine`] for.
#[derive(Clone, Copy, Debug)]
pub struct SessionSettings {
    /// The engine limits `Configure` agreed on.
    pub limits: Limits,
    /// The bound on a cell's rendered final value (`Session::set_text_limit`).
    pub text_limit: u64,
    /// How host-call arguments are converted (`Session::set_host_args`).
    pub host_args: HostArgs,
}

/// Creates the worker's session once `Configure` has been checked. Every
/// session renders a final int past 4300 digits as `<int with N bits>`
/// (`Session::set_int_bits_text`), as plak's py-ptc does.
pub trait CellEngine {
    /// A new session.
    ///
    /// # Errors
    /// The engine did not start; the worker is lost with this message.
    fn session(&self, settings: &SessionSettings) -> Result<Box<dyn CellSession>, String>;
}

/// One engine session, as the worker loop drives it.
pub trait CellSession {
    /// Defines a host module.
    ///
    /// # Errors
    /// The Python traceback.
    fn define(&mut self, module: &HostModule) -> Result<(), String>;
    /// Compiles and starts `source`.
    ///
    /// # Errors
    /// The outer error loses the worker (an engine trap); the inner one is
    /// the rendered `SyntaxError`, which fails only the cell.
    fn start(&mut self, source: &str, filename: &str) -> Result<Result<Progress, String>, String>;
    /// Resumes the session with `answer`.
    ///
    /// # Errors
    /// The engine refused the answer; the worker is lost.
    fn resume(&mut self, answer: Resume) -> Result<Progress, String>;
    /// Ends the running cell where it stands.
    fn cancel(&mut self);
    /// Starts a fresh generation of globals.
    ///
    /// # Errors
    /// The reset failed; the worker is lost.
    fn reset(&mut self) -> Result<(), String>;
    /// The deadline at which a running cell parks.
    fn set_deadline(&mut self, at: Option<Instant>);
    /// Where prints go: stdout and stderr, merged in production order.
    fn set_output(&mut self, out: Box<dyn FnMut(&str) + Send>);
    /// A handle that requests a park of the running cell.
    fn park(&self) -> Park;
}

/// micro-monty linked into this process. Sessions of one `NativeEngine`
/// share one module cache, whatever their limits ([`Engine::with_limits`]).
#[derive(Default)]
pub struct NativeEngine {
    engine: Mutex<Option<(Limits, Engine)>>,
    setup: Option<SessionSetup>,
    /// [`NativeEngine::with_host_code_in_snapshots`].
    host_code_in_snapshots: bool,
}

/// Adjusts each new session after the worker's own settings.
type SessionSetup = Box<dyn Fn(&mut Session) + Send + Sync>;

impl NativeEngine {
    /// A `NativeEngine` that passes every new session to `setup` before
    /// its first cell, for what plak leaves to the engine's defaults: a
    /// pinned wall clock and entropy for a replay, a time zone source.
    #[must_use]
    pub fn with_session_setup(setup: impl Fn(&mut Session) + Send + Sync + 'static) -> Self {
        Self {
            engine: Mutex::default(),
            setup: Some(Box::new(setup)),
            host_code_in_snapshots: false,
        }
    }

    /// This `NativeEngine`, whose sessions' snapshots store the code
    /// generated for typed host functions instead of referring to the
    /// Engine's copy by key (`Engine::with_host_code_in_snapshots`). For a
    /// process that holds one session, as the `mm-cell-worker` binary
    /// does: a keyed image restored in a new process compiles the code
    /// again. An in-process host with many sessions keeps the default
    /// (`false`).
    #[must_use]
    pub fn with_host_code_in_snapshots(mut self, on: bool) -> Self {
        self.host_code_in_snapshots = on;
        self
    }
}

impl CellEngine for NativeEngine {
    fn session(&self, settings: &SessionSettings) -> Result<Box<dyn CellSession>, String> {
        let engine = {
            let mut shared = lock(&self.engine);
            match &*shared {
                Some((limits, engine)) if *limits == settings.limits => engine.clone(),
                // Other limits: the same module cache.
                Some((_, engine)) => {
                    let engine = engine.with_limits(settings.limits);
                    *shared = Some((settings.limits, engine.clone()));
                    engine
                }
                None => {
                    let engine = Engine::new(EngineConfig::new().with_limits(settings.limits))
                        .with_host_code_in_snapshots(self.host_code_in_snapshots);
                    *shared = Some((settings.limits, engine.clone()));
                    engine
                }
            }
        };
        let mut session = Session::new(&engine).map_err(|error| error.message)?;
        session.set_host_args(settings.host_args);
        session.set_text_limit(settings.text_limit);
        session.set_int_bits_text(true);
        if let Some(setup) = &self.setup {
            setup(&mut session);
        }
        Ok(Box::new(Native { session }))
    }
}

struct Native {
    session: Session,
}

impl CellSession for Native {
    fn define(&mut self, module: &HostModule) -> Result<(), String> {
        self.session
            .define(module)
            .map(drop)
            .map_err(|error| error.to_string())
    }

    fn start(&mut self, source: &str, filename: &str) -> Result<Result<Progress, String>, String> {
        // Charged to the session: a cell too big for the memory left is a
        // MemoryError, not process memory past the limit.
        Ok(match self.session.compile(source, filename) {
            Ok(code) => Ok(self.session.start(&code)),
            Err(error) => Err(error.rendered),
        })
    }

    fn resume(&mut self, answer: Resume) -> Result<Progress, String> {
        self.session
            .resume(answer)
            .map_err(|error| format!("the engine refused an answer: {error}"))
    }

    fn cancel(&mut self) {
        self.session.cancel();
    }

    fn reset(&mut self) -> Result<(), String> {
        self.session.reset().map_err(|error| error.message)
    }

    fn set_deadline(&mut self, at: Option<Instant>) {
        self.session.set_deadline(at);
    }

    fn set_output(&mut self, mut out: Box<dyn FnMut(&str) + Send>) {
        self.session
            .set_output_streams(Box::new(move |_, text: &str| out(text)));
    }

    fn park(&self) -> Park {
        let handle = self.session.park_handle();
        Arc::new(move || handle.request())
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, reason = "tests")]

    use micro_monty::{
        Engine, EngineConfig, HostArgs, HostModule, HostParam, HostSignature, Limits, Session,
    };

    use super::{CellEngine, NativeEngine, SessionSettings};
    use crate::lock;

    /// An image of a session of `native`'s engine (its limits `memory`)
    /// with one typed host function defined.
    fn typed_image(native: &NativeEngine, memory: u64) -> Vec<u8> {
        let settings = SessionSettings {
            limits: Limits::new().with_memory(memory),
            text_limit: 1 << 20,
            host_args: HostArgs::default(),
        };
        native.session(&settings).expect("session");
        let engine = lock(&native.engine).as_ref().expect("engine").1.clone();
        let mut s = Session::new(&engine).expect("session");
        s.define(&HostModule {
            name: "tools".into(),
            functions: vec![HostSignature {
                name: "read".into(),
                params: vec![HostParam {
                    name: "path".into(),
                    annotation: "str".into(),
                    ..HostParam::default()
                }],
                ..HostSignature::default()
            }],
            ..HostModule::default()
        })
        .expect("defines");
        s.snapshot().expect("snapshot")
    }

    /// With host code stored (the binary's setting), an image restores on
    /// a new Engine without compiling the code again, also from an engine
    /// made for other limits; by default it is keyed and compiles.
    #[test]
    fn host_code_in_snapshots_reaches_every_engine_it_makes() {
        let stored = NativeEngine::default().with_host_code_in_snapshots(true);
        let keyed = NativeEngine::default();
        for (native, compiles) in [(&stored, false), (&keyed, true)] {
            for memory in [64 << 20, 32 << 20] {
                let image = typed_image(native, memory);
                let fresh = Engine::new(EngineConfig::default());
                let before = fresh.module_cache_bytes();
                fresh.restore(&image).expect("restores");
                assert_eq!(fresh.module_cache_bytes() > before, compiles, "{memory}");
            }
        }
    }
}
