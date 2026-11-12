//! micro-monty as a wasm module speaking `mm-wasm` (`mm_wasm_abi`,
//! docs/research/wasm-embedding.md). The same source builds for
//! `wasm32-unknown-unknown` and `wasm32-wasip1`: the guest reads no WASI
//! clock, randomness or files, only its `mm_host` imports.
//!
//! Variable-size data goes through two guest-owned buffers: the host sizes
//! the input with `mm_input(len)` and writes it, then calls an export; the
//! export leaves its payload in the output, which `mm_output()` locates.
//! Both are `Vec<u8>`s read and written with safe Rust. The only `unsafe`
//! syntax is the import block ([`host`]) and the exports'
//! `#[unsafe(no_mangle)]` ([`exports`]), the exception docs/spec.md §5
//! records for this crate.
//!
//! An instance can hold several sessions (`u32` ids). The reference host
//! gives each session an instance of its own, as hosts/js does unless told
//! to share (`sessionsPerInstance`): a trap loses the instance, so sessions
//! that share one share its faults and its memory.
//!
//! On other targets the imports are stubs, so the workspace's tests and
//! clippy cover the crate; only a wasm build is a usable guest.

use std::cell::RefCell;

use micro_monty::{
    Engine, EngineConfig, ParkReason, Progress, Session, SessionSettings, Stream, ZoneSource,
};
use mm_wasm_abi::{ARMED, Bad, Reader, SessionOptions, get_options, put_progress, put_str, status};

// std's allocator without its per-site checks (crates/mm-wasm-alloc,
// docs/sdk.md); optional for embedders.
#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
#[global_allocator]
static ALLOCATOR: mm_wasm_alloc::Allocator = mm_wasm_alloc::Allocator::new();

/// The `mm_host` imports (`mm_wasm_abi::import`).
#[cfg(target_family = "wasm")]
#[allow(unsafe_code, reason = "the import block; docs/spec.md §5")]
mod host {
    #[link(wasm_import_module = "mm_host")]
    unsafe extern "C" {
        /// The host's monotonic clock in nanoseconds (deadlines).
        pub safe fn now_ns(session: u32) -> i64;
        /// Whether a park of `session` is requested; `clear` takes it.
        pub safe fn park(session: u32, clear: u32) -> u32;
        /// Seconds since the Unix epoch (`time.time()`).
        pub safe fn wall_clock(session: u32) -> f64;
        /// Fills `len` bytes at `ptr` with randomness.
        pub safe fn entropy(session: u32, ptr: *mut u8, len: usize);
        /// Output in production order; `stream` 0 stdout, 1 stderr.
        pub safe fn output(session: u32, stream: u32, ptr: *const u8, len: usize);
        /// Zone `key`'s `TZif` data: its length (written when it fits in
        /// `cap`), or -1.
        pub safe fn zone(
            session: u32,
            key: *const u8,
            key_len: usize,
            buf: *mut u8,
            cap: usize,
        ) -> i64;
    }

    // The bundle guest's one extra import (docs/research/wasm-embedding.md).
    #[cfg(feature = "host-bundle")]
    #[link(wasm_import_module = "mm_host")]
    unsafe extern "C" {
        /// Copies `len` bytes from `offset` of bundle `region` to `ptr`:
        /// 1 if it could.
        pub safe fn bundle_read(region: u32, offset: u32, ptr: *mut u8, len: usize) -> u32;
    }
}

/// Stubs, so the crate builds (and is checked) on other targets.
#[cfg(not(target_family = "wasm"))]
mod host {
    pub fn now_ns(_: u32) -> i64 {
        0
    }
    pub fn park(_: u32, _: u32) -> u32 {
        0
    }
    pub fn wall_clock(_: u32) -> f64 {
        0.0
    }
    pub fn entropy(_: u32, _: *mut u8, _: usize) {}
    pub fn output(_: u32, _: u32, _: *const u8, _: usize) {}
    pub fn zone(_: u32, _: *const u8, _: usize, _: *mut u8, _: usize) -> i64 {
        -1
    }
}

struct Slot {
    session: Session,
    /// The host's deadline (`mm_set_deadline`), kept across park requests.
    deadline: Option<u64>,
}

thread_local! {
    static SESSIONS: RefCell<Vec<Option<Slot>>> = const { RefCell::new(Vec::new()) };
    static INPUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
    static OUTPUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

/// A guest address as the wasm `i32` the host sees (addresses are 32-bit
/// on wasm32; on other targets the value is meaningless).
#[allow(
    clippy::cast_possible_truncation,
    reason = "wasm32 addresses fit in u32"
)]
fn address(p: *const u8) -> u32 {
    p as usize as u32
}

fn input<T>(f: impl FnOnce(&[u8]) -> T) -> T {
    INPUT.with_borrow(|i| f(i))
}

/// Decodes the input with `f`.
fn decode<T>(f: impl FnOnce(&mut Reader<'_>) -> Result<T, Bad>) -> Result<T, Bad> {
    input(|i| f(&mut Reader::new(i)))
}

/// Capacity the input and output buffers keep between calls. A larger
/// one (a snapshot image, a restored one, a long value) is freed once it
/// has been read, so the heap can reuse its memory: linear memory never
/// shrinks, and a kept image would add to every later peak.
const KEEP: usize = 64 << 10;

/// `buf`, emptied, keeping at most [`KEEP`] (or `want`) bytes of capacity.
fn recycle(buf: &mut Vec<u8>, want: usize) {
    if buf.capacity() > KEEP.max(want) {
        *buf = Vec::new();
    } else {
        buf.clear();
    }
}

/// Replaces the output with what `f` writes.
fn output(f: impl FnOnce(&mut Vec<u8>)) {
    OUTPUT.with_borrow_mut(|o| {
        recycle(o, 0);
        f(o);
    });
}

fn bad(what: &str) -> u32 {
    output(|o| put_str(o, what));
    status::BAD_REQUEST
}

fn with_slot(s: u32, f: impl FnOnce(&mut Slot) -> u32) -> u32 {
    SESSIONS.with_borrow_mut(
        |all| match all.get_mut(s as usize).and_then(Option::as_mut) {
            Some(slot) => f(slot),
            None => bad("no such session"),
        },
    )
}

fn py_status(result: Result<(), micro_monty::PyException>) -> u32 {
    match result {
        Ok(()) => status::OK,
        Err(e) => {
            output(|o| mm_wasm_abi::put_exception(o, &e));
            status::PY_ERROR
        }
    }
}

/// One engine per session: it shares decoded modules within the session,
/// but not unpacked prelude chunks, which would stay in the instance's
/// memory for its whole life (docs/HANDOFF.md, scope decision 48). Its
/// images store their host-module code, since each restore lands on a new
/// instance's engine, which would compile it again (scope decision 60).
fn engine(o: &SessionOptions) -> Engine {
    // The bundle guest: the bytecode is in the module's `mm.prelude` and
    // `mm.stdlib` custom sections, which wasm never copies into linear
    // memory; the host serves them.
    #[cfg(all(feature = "host-bundle", target_family = "wasm"))]
    micro_monty::set_bundle_reader(|region, offset, out| {
        host::bundle_read(region, offset, out.as_mut_ptr(), out.len()) == 1
    });
    Engine::without_prelude_chunk_cache(EngineConfig::new().with_limits(o.limits))
        .with_host_code_in_snapshots(true)
}

/// Zone `key` from the host: asks with a 4 KiB buffer (most `TZif` files
/// are smaller), then once more at the length the host reported.
fn zone(id: u32, key: &str) -> Option<Vec<u8>> {
    let mut buf = vec![0u8; 4096];
    let ask = |buf: &mut Vec<u8>| {
        let n = host::zone(id, key.as_ptr(), key.len(), buf.as_mut_ptr(), buf.len());
        usize::try_from(n).ok()
    };
    let n = ask(&mut buf)?;
    if n > buf.len() {
        buf.resize(n, 0);
        if ask(&mut buf)? != n {
            return None;
        }
    }
    buf.truncate(n);
    Some(buf)
}

/// The host-side settings of session `id`, new or restored.
fn settings(id: u32, o: &SessionOptions) -> SessionSettings {
    let mut s = SessionSettings::default();
    s.text_cut = o.text_cut;
    s.int_bits_text = o.int_bits_text;
    s.gc_stress = o.gc_stress;
    s.output = Some(Box::new(move |stream, text| {
        let stream = u32::from(matches!(stream, Stream::Stderr));
        host::output(id, stream, text.as_ptr(), text.len());
    }));
    s.wall_clock = o
        .wall_clock
        .then(|| Box::new(move || host::wall_clock(id)) as micro_monty::WallClock);
    s.entropy = o.entropy.then(|| {
        Box::new(move |buf: &mut [u8]| host::entropy(id, buf.as_mut_ptr(), buf.len()))
            as micro_monty::Entropy
    });
    s.zone_source = o
        .zone_source
        .then(|| Box::new(move |key: &str| zone(id, key)) as ZoneSource);
    s
}

/// Arms the deadline clock before running: the engine reads it at each
/// safepoint check, before its park flag, so a park request the host
/// answers through `mm_host.park` becomes the session's own request there
/// and parks the cell as `Requested`, without a trap (owner decision: the
/// poll stays here, with no facade hook). The clock then reads past any
/// deadline, which also cuts a long report short.
fn arm(id: u32, slot: &mut Slot) {
    let park = slot.session.park_handle();
    let deadline = slot.deadline;
    slot.session.set_deadline_with(
        Box::new(move || {
            if host::park(id, 0) != 0 {
                park.request();
                u64::MAX
            } else if deadline.is_some() {
                host::now_ns(id).max(0).cast_unsigned()
            } else {
                0
            }
        }),
        deadline.unwrap_or(ARMED),
    );
}

/// Reports `progress`. The host's park request stays pending until the
/// park it asked for is reported; a deadline is one-shot.
fn report(id: u32, slot: &mut Slot, progress: &Progress) -> u32 {
    match progress {
        Progress::Parked {
            reason: ParkReason::Requested,
        } => {
            host::park(id, 1);
        }
        Progress::Parked {
            reason: ParkReason::Deadline,
        } => slot.deadline = None,
        _ => {}
    }
    let mut code = 0;
    output(|o| code = put_progress(o, progress));
    code
}

fn push(slot: Slot) -> u32 {
    SESSIONS.with_borrow_mut(|all| {
        all.push(Some(slot));
        u32::try_from(all.len() - 1).unwrap_or(u32::MAX)
    })
}

fn next_id() -> u32 {
    SESSIONS.with_borrow(|all| u32::try_from(all.len()).unwrap_or(u32::MAX))
}

/// The exports (`mm_wasm_abi::export` documents each).
#[allow(
    unsafe_code,
    reason = "`#[unsafe(no_mangle)]` on the exports; docs/spec.md §5"
)]
pub mod exports {
    use micro_monty::{Progress, Session, TextCut};
    use mm_wasm_abi::{
        ABI_VERSION, Reader, event, get_module, get_resume, hint_group, phase_code, put_call_log,
        put_compile_error, put_define_error, put_exception, put_function_refs, put_modules,
        put_progress, put_resume_error, put_snapshot_error, put_str, put_stubs, status,
        text_cut_from_code,
    };

    use super::{
        INPUT, OUTPUT, SESSIONS, Slot, address, arm, bad, decode, engine, get_options, input,
        next_id, output, push, py_status, recycle, report, settings, with_slot,
    };

    /// The interface version.
    #[unsafe(no_mangle)]
    pub extern "C" fn mm_abi_version() -> u32 {
        ABI_VERSION
    }

    /// The snapshot build id (32 bytes) in the output.
    #[unsafe(no_mangle)]
    pub extern "C" fn mm_build_id() -> u32 {
        output(|o| o.extend_from_slice(micro_monty::build_id()));
        status::OK
    }

    /// Sizes the input buffer to `len` bytes; returns its address.
    #[unsafe(no_mangle)]
    pub extern "C" fn mm_input(len: u32) -> u32 {
        INPUT.with_borrow_mut(|i| {
            recycle(i, len as usize);
            i.resize(len as usize, 0);
            address(i.as_ptr())
        })
    }

    /// The output buffer: `address << 32 | length`.
    #[unsafe(no_mangle)]
    pub extern "C" fn mm_output() -> u64 {
        OUTPUT.with_borrow(|o| (u64::from(address(o.as_ptr())) << 32) | o.len() as u64)
    }

    /// A new session from the options in the input.
    #[unsafe(no_mangle)]
    pub extern "C" fn mm_session_new() -> i32 {
        let Ok(o) = decode(get_options) else {
            bad("session options");
            return -2;
        };
        let engine = engine(&o);
        match Session::new_with(&engine, settings(next_id(), &o)) {
            Ok(mut session) => {
                session.set_host_args(o.host_args);
                session.set_last_expr_value(o.last_expr_value);
                if let Some(t) = o.text_limit {
                    session.set_text_limit(t);
                }
                push(Slot {
                    session,
                    deadline: None,
                })
                .cast_signed()
            }
            Err(e) => {
                output(|out| put_exception(out, &e));
                -1
            }
        }
    }

    /// A session restored from the input: the options record, then the
    /// image.
    #[unsafe(no_mangle)]
    pub extern "C" fn mm_session_restore() -> i32 {
        let restored = input(|i| {
            let mut r = Reader::new(i);
            let o = get_options(&mut r)?;
            let engine = engine(&o);
            Ok::<_, mm_wasm_abi::Bad>(engine.restore_with(r.rest(), settings(next_id(), &o)))
        });
        // The image is in the session now.
        INPUT.with_borrow_mut(|i| recycle(i, 0));
        match restored {
            Err(_) => {
                bad("restore options");
                -2
            }
            Ok(Ok(session)) => push(Slot {
                session,
                deadline: None,
            })
            .cast_signed(),
            Ok(Err(e)) => {
                output(|out| put_snapshot_error(out, &e));
                -1
            }
        }
    }

    /// Drops session `s` and its heap.
    #[unsafe(no_mangle)]
    pub extern "C" fn mm_session_drop(s: u32) -> u32 {
        SESSIONS.with_borrow_mut(|all| match all.get_mut(s as usize) {
            Some(slot @ Some(_)) => {
                *slot = None;
                status::OK
            }
            _ => bad("no such session"),
        })
    }

    /// Defines the `HostModule` in the input.
    #[unsafe(no_mangle)]
    pub extern "C" fn mm_define(s: u32) -> u32 {
        let Ok(module) = decode(get_module) else {
            return bad("host module");
        };
        with_slot(s, |slot| match slot.session.define(&module) {
            Ok(refs) => {
                output(|o| put_function_refs(o, &refs));
                status::OK
            }
            Err(e) => {
                output(|o| put_define_error(o, &e));
                status::DEFINE_ERROR
            }
        })
    }

    /// Compiles the input (filename, source) and starts it.
    #[unsafe(no_mangle)]
    pub extern "C" fn mm_start(s: u32) -> u32 {
        let Ok((filename, source)) = decode(|r| Ok((r.string()?, r.string()?))) else {
            return bad("filename and source");
        };
        // Charged to the session's memory limit (`Session::compile`).
        with_slot(s, |slot| match slot.session.compile(&source, &filename) {
            Ok(code) => {
                arm(s, slot);
                let progress = slot.session.start(&code);
                report(s, slot, &progress)
            }
            Err(e) => {
                output(|o| put_compile_error(o, &e));
                event::COMPILE_ERROR
            }
        })
    }

    /// Answers the stop with the `Resume` in the input.
    #[unsafe(no_mangle)]
    pub extern "C" fn mm_resume(s: u32) -> u32 {
        let Ok(resume) = decode(get_resume) else {
            return bad("resume");
        };
        with_slot(s, |slot| {
            arm(s, slot);
            match slot.session.resume(resume) {
                Ok(progress) => report(s, slot, &progress),
                Err(e) => {
                    output(|o| put_resume_error(o, &e));
                    event::RESUME_ERROR
                }
            }
        })
    }

    /// `Session::cancel`: 1 if a cell was stopped, else 0.
    #[unsafe(no_mangle)]
    pub extern "C" fn mm_cancel(s: u32) -> u32 {
        with_slot(s, |slot| u32::from(slot.session.cancel()))
    }

    /// `Session::reset`.
    #[unsafe(no_mangle)]
    pub extern "C" fn mm_reset(s: u32) -> u32 {
        with_slot(s, |slot| py_status(slot.session.reset()))
    }

    /// The session's image in the output; `max_bytes` `u64::MAX` is the
    /// default cap (the memory limit).
    #[unsafe(no_mangle)]
    pub extern "C" fn mm_snapshot(s: u32, max_bytes: u64) -> u32 {
        with_slot(s, |slot| {
            let result = if max_bytes == u64::MAX {
                slot.session.snapshot()
            } else {
                slot.session.snapshot_with_limit(max_bytes)
            };
            match result {
                Ok(image) => {
                    // Freed by the next call that writes output.
                    OUTPUT.with_borrow_mut(|o| *o = image);
                    status::OK
                }
                Err(e) => {
                    output(|o| put_snapshot_error(o, &e));
                    status::SNAPSHOT_ERROR
                }
            }
        })
    }

    /// `Session::set_fuel`; `u64::MAX` is unlimited.
    #[unsafe(no_mangle)]
    pub extern "C" fn mm_set_fuel(s: u32, fuel: u64) -> u32 {
        with_slot(s, |slot| {
            slot.session.set_fuel(Some(fuel).filter(|f| *f != u64::MAX));
            status::OK
        })
    }

    /// `Session::set_fuel_total`; `u64::MAX` is no cap.
    #[unsafe(no_mangle)]
    pub extern "C" fn mm_set_fuel_total(s: u32, total: u64) -> u32 {
        with_slot(s, |slot| {
            slot.session
                .set_fuel_total(Some(total).filter(|t| *t != u64::MAX));
            status::OK
        })
    }

    /// `Session::fuel_total`; `u64::MAX` is no cap (or no such session).
    #[unsafe(no_mangle)]
    pub extern "C" fn mm_fuel_total(s: u32) -> u64 {
        query(s, |session| session.fuel_total().unwrap_or(u64::MAX))
    }

    /// `Session::phase` (`mm_wasm_abi::phase`).
    #[unsafe(no_mangle)]
    pub extern "C" fn mm_phase(s: u32) -> u32 {
        with_slot(s, |slot| phase_code(slot.session.phase()))
    }

    /// `Session::pending`, as the event that reported the stop; `COMPLETE`
    /// without a completion between cells.
    #[unsafe(no_mangle)]
    pub extern "C" fn mm_pending(s: u32) -> u32 {
        with_slot(s, |slot| {
            let p = slot.session.pending().unwrap_or(Progress::Complete(None));
            let mut code = 0;
            output(|o| code = put_progress(o, &p));
            code
        })
    }

    fn query(s: u32, f: impl FnOnce(&Session) -> u64) -> u64 {
        SESSIONS.with_borrow(|all| {
            all.get(s as usize)
                .and_then(Option::as_ref)
                .map_or(u64::MAX, |slot| f(&slot.session))
        })
    }

    /// `Session::fuel`; `u64::MAX` is unlimited (or no such session).
    #[unsafe(no_mangle)]
    pub extern "C" fn mm_fuel(s: u32) -> u64 {
        query(s, |session| session.fuel().unwrap_or(u64::MAX))
    }

    /// `Session::fuel_wanted` (`u64::MAX`: no such session).
    #[unsafe(no_mangle)]
    pub extern "C" fn mm_fuel_wanted(s: u32) -> u64 {
        query(s, Session::fuel_wanted)
    }

    /// A deadline on `mm_host.now_ns` (`u64::MAX` clears it). One-shot,
    /// as `Session::set_deadline`.
    #[unsafe(no_mangle)]
    pub extern "C" fn mm_set_deadline(s: u32, ns: u64) -> u32 {
        with_slot(s, |slot| {
            slot.deadline = Some(ns).filter(|d| *d != u64::MAX);
            status::OK
        })
    }

    /// `Session::set_hint` from the input: the group's key, then an
    /// optional text.
    #[unsafe(no_mangle)]
    pub extern "C" fn mm_set_hint(s: u32) -> u32 {
        let Ok((key, text)) = decode(|r| Ok((r.string()?, r.opt_string()?))) else {
            return bad("hint");
        };
        let Some(group) = hint_group(&key) else {
            return bad("no such hint group");
        };
        with_slot(s, |slot| {
            py_status(slot.session.set_hint(group, text.as_deref()))
        })
    }

    /// `Session::set_text_cut` (`mm_wasm_abi::text_cut`).
    #[unsafe(no_mangle)]
    pub extern "C" fn mm_set_text_cut(s: u32, cut: u32) -> u32 {
        let Ok(cut): Result<TextCut, _> = text_cut_from_code(cut) else {
            return bad("text cut");
        };
        with_slot(s, |slot| {
            slot.session.set_text_cut(cut);
            status::OK
        })
    }

    /// `Session::set_local_timezone` from the input (optional `TZif` bytes).
    #[unsafe(no_mangle)]
    pub extern "C" fn mm_set_local_timezone(s: u32) -> u32 {
        let Ok(tzif) = decode(|r| Ok(r.opt_bytes()?.map(<[u8]>::to_vec))) else {
            return bad("time zone");
        };
        with_slot(s, |slot| {
            slot.session.set_local_timezone(tzif);
            status::OK
        })
    }

    /// `Session::call_log` in the output.
    #[unsafe(no_mangle)]
    pub extern "C" fn mm_call_log(s: u32) -> u32 {
        with_slot(s, |slot| {
            let log = slot.session.call_log();
            output(|o| put_call_log(o, &log));
            status::OK
        })
    }

    /// `Engine::environment` for the session's engine, in the output.
    #[unsafe(no_mangle)]
    pub extern "C" fn mm_environment(s: u32) -> u32 {
        with_slot(s, |slot| {
            let text = slot.session.environment();
            output(|o| put_str(o, &text));
            status::OK
        })
    }

    /// `Session::stubs` in the output.
    #[unsafe(no_mangle)]
    pub extern "C" fn mm_stubs(s: u32) -> u32 {
        with_slot(s, |slot| {
            let stubs = slot.session.stubs();
            output(|o| put_stubs(o, &stubs));
            status::OK
        })
    }

    /// `Session::host_modules` in the output.
    #[unsafe(no_mangle)]
    pub extern "C" fn mm_host_modules(s: u32) -> u32 {
        with_slot(s, |slot| {
            let modules = slot.session.host_modules();
            output(|o| put_modules(o, &modules));
            status::OK
        })
    }

    /// `Session::set_module_dunders` from the input: optional `__name__`,
    /// then optional `__file__`.
    #[unsafe(no_mangle)]
    pub extern "C" fn mm_set_module_dunders(s: u32) -> u32 {
        let Ok((name, file)) = decode(|r| Ok((r.opt_string()?, r.opt_string()?))) else {
            return bad("module dunders");
        };
        with_slot(s, |slot| {
            py_status(
                slot.session
                    .set_module_dunders(name.as_deref(), file.as_deref()),
            )
        })
    }

    /// `Session::set_int_bits_text`.
    #[unsafe(no_mangle)]
    pub extern "C" fn mm_set_int_bits_text(s: u32, on: u32) -> u32 {
        with_slot(s, |slot| {
            slot.session.set_int_bits_text(on != 0);
            status::OK
        })
    }

    /// `Session::set_gc_stress`.
    #[unsafe(no_mangle)]
    pub extern "C" fn mm_set_gc_stress(s: u32, on: u32) -> u32 {
        with_slot(s, |slot| {
            slot.session.set_gc_stress(on != 0);
            status::OK
        })
    }

    /// `Session::set_gc_yield`.
    #[unsafe(no_mangle)]
    pub extern "C" fn mm_set_gc_yield(s: u32, on: u32) -> u32 {
        with_slot(s, |slot| {
            slot.session.set_gc_yield(on != 0);
            status::OK
        })
    }

    /// Fault injection: the next `start` (0), `resume` (1), `snapshot` (2)
    /// or `restore` (3) panics inside the engine.
    #[cfg(feature = "test-hooks")]
    #[unsafe(no_mangle)]
    pub extern "C" fn mm_test_panic(op: u32) -> u32 {
        let op = match op {
            0 => "start",
            1 => "resume",
            2 => "snapshot",
            _ => "restore",
        };
        micro_monty::test_hooks::panic_next(op);
        status::OK
    }
}
