//! `mm`: run Python files on micro-monty.
//!
//! - `mm run FILE` prints the program's output to stdout and an uncaught
//!   exception's traceback to stderr (CPython's format), exiting 1 on error.
//! - `mm exec --json FILE` prints one JSON object describing the run (for the
//!   conformance harness): `stdout`, `result` (`repr`/`str`/`type` of a final
//!   expression statement, or null), `error` (`type`, `message`, `traceback`,
//!   or null) and `exit`. `--recursion-limit N` sets the frame depth limit
//!   (the conformance harness uses Monty's test limit, 50).
//!   `--host-fixtures` provides the host functions, constants and `tools`
//!   module of Monty's datatest fixtures (`conformance/monty-scripts/
//!   test_fixtures.py`) for `call-external` cases: sync calls are answered
//!   in place, `async_call`/`async_fail` with futures resolved when the
//!   cell waits. The host-object fixtures (`make_point`, ...) raise
//!   `NotImplementedError` until classes exist (M4).
//! - Durability (docs/spec.md §13): `mm exec --json --snapshot-every N
//!   --snapshot-dir DIR FILE` runs with N fuel units per slice; when a slice
//!   runs out (and, with `--host-fixtures`, at every host call and every
//!   wait on host futures) it saves the session to `DIR/session.mmsnap` and
//!   the host's side (output so far, the fixture answers owed) to
//!   `DIR/state.json`, prints `{"park": K, "reason": ..., "stdout": ...}`
//!   and exits. `mm resume --json DIR` restores both in a new process and
//!   continues to the next park, or prints the same report `exec --json`
//!   prints for an unparked run, with the fuel the whole chain spent
//!   (`"fuel"`). `--report-fuel` adds the fuel an unparked run spent to its
//!   report the same way, so the two compare (`oracle.py --durability`).
//!   `--park-every N` runs the cell in this process instead, parked every
//!   N fuel units and resumed (with `--host-fixtures`, the fixtures
//!   answered as above), and reports its fuel; `--snapshot-continue` with
//!   it also takes a snapshot at every stop and goes on with the same
//!   session. `--digest` pins the wall clock and makes entropy a counter
//!   (the parked chain saves it with the host's side), and adds the
//!   final-state digest to the report (`"digest"`): the checksum of the
//!   session's image at the end, taken with the fuel and its total
//!   cleared, so the durability gate compares the state each way of
//!   running a cell leaves (docs/spec.md §13).
//! - `mm version` prints the engine version and the snapshot `build_id`
//!   (hex): `mm resume` restores only snapshots written under the same id.
//! - `MM_GC_STRESS=1` collects garbage at every safepoint of every session,
//!   over 1 MiB live after each 1/64 of it allocated
//!   (`Session::set_gc_stress`; `cargo xtask gc-stress`); `MM_GC_STRESS=inc`
//!   keeps an incremental mark in progress at every safepoint, each checked
//!   against a full mark (`cargo xtask gc-stress --incremental`).
//! - `MM_GC_YIELD=1` makes a safepoint that ran a collection step park
//!   with fuel left in a parked chain (`exec --snapshot-every`, `resume`;
//!   `Session::set_gc_yield`, as `drive_async` sets it), for the gates:
//!   `MM_GC_YIELD=1 cargo xtask durability`.
//! - `MM_STEP_MS=N` makes `mm run` a step-time check (docs/spec.md §5,
//!   bounded native steps): it parks at every safepoint past a 1 ms
//!   deadline and resumes, and when a start or resume runs more than N ms
//!   past its deadline, it reports `mm: a native step ran ...` on stderr
//!   and exits with status 3. Output is unchanged otherwise, so
//!   `difffuzz.py` run with it counts a long step as a divergence. The
//!   bound is 50 ms in release on the reference machine; a fuzzing run
//!   leaves room for load, such as `MM_STEP_MS=250`.
//! - Module dunders: `mm run` runs the file as `python FILE` does
//!   (`__name__ == '__main__'`, `__file__` its absolute path). `mm exec`
//!   runs it as the datatest's CPython side does, in a namespace without
//!   `__name__` (code sees `'builtins'`, as with `exec(code, {})`), unless
//!   `--main-module` (the `cpython-main-module` header) sets both.

#![allow(clippy::doc_markdown, reason = "the docs name CPython in prose")]

mod vfs;

use micro_monty::{
    Engine, EngineConfig, HostError, HostErrorKind, HostModule, HostSignature, HostValue, Limits,
    ParkReason, Progress, Resume, Session, SessionSettings, Stream,
};
use std::{
    collections::BTreeMap,
    io::{BufWriter, Read, Write},
    process::ExitCode,
    sync::{
        Arc, Mutex, MutexGuard, PoisonError,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

const USAGE: &str = "usage: mm run FILE
       mm version
       mm exec --json [--recursion-limit N] [--host-fixtures] [--report-fuel]
                      [--main-module] [--digest] [--snapshot-every N --snapshot-dir DIR]
                      [--park-every N [--snapshot-continue]] FILE
       mm resume --json DIR";

/// `MM_GC_STRESS=1`: collect at every safepoint (over 1 MiB live, after
/// each 1/64 of it allocated); `inc`: an incremental
/// mark at every safepoint.
/// `MM_GC_YIELD=1`: in a parked chain (`exec --snapshot-every`, `resume`),
/// a safepoint that ran a collection step parks with fuel left
/// (`Session::set_gc_yield`; not in the image, so set again on every
/// restore). Not for a plain or `--report-fuel` run, which would stop at
/// that park.
fn yielding(mut s: Session) -> Session {
    if std::env::var("MM_GC_YIELD").as_deref() == Ok("1") {
        s.set_gc_yield(true);
    }
    s
}

/// The wall clock `--digest` pins (seconds since the Unix epoch).
const PINNED_CLOCK: f64 = 1_794_000_000.0;

/// `entropy`, with `--digest`: the counter the session's entropy reads,
/// with the wall clock pinned, so the final state does not depend on
/// either; a parked chain carries the counter on.
fn settings(entropy: Option<&Arc<AtomicU64>>) -> SessionSettings {
    let mut s = SessionSettings::default();
    match std::env::var("MM_GC_STRESS").as_deref() {
        Ok("1") => s.gc_stress = true,
        Ok("inc") => s.gc_inc_stress = true,
        _ => {}
    }
    if let Some(counter) = entropy {
        let counter = Arc::clone(counter);
        s.wall_clock = Some(Box::new(|| PINNED_CLOCK));
        s.entropy = Some(Box::new(move |buf: &mut [u8]| {
            for byte in buf {
                let n = counter.fetch_add(1, Ordering::Relaxed);
                *byte = n.wrapping_mul(0x9E37_79B9_7F4A_7C15).to_be_bytes()[0];
            }
        }));
    }
    s
}

/// `exec`'s switches.
#[derive(Default)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent command-line switches"
)]
struct Flags {
    fixtures: bool,
    report_fuel: bool,
    main_module: bool,
    digest: bool,
    /// `--park-every N`.
    park_every: Option<u64>,
    snapshot_continue: bool,
}

impl Flags {
    /// The switches in `refs`, which it removes; `park_every` is left
    /// unset.
    fn take(refs: &mut Vec<&str>) -> Flags {
        let flags = Flags {
            fixtures: refs.contains(&"--host-fixtures"),
            report_fuel: refs.contains(&"--report-fuel"),
            main_module: refs.contains(&"--main-module"),
            digest: refs.contains(&"--digest"),
            park_every: None,
            snapshot_continue: refs.contains(&"--snapshot-continue"),
        };
        refs.retain(|a| {
            !matches!(
                *a,
                "--host-fixtures"
                    | "--report-fuel"
                    | "--main-module"
                    | "--digest"
                    | "--snapshot-continue"
            )
        });
        flags
    }

    fn any(&self) -> bool {
        self.fixtures
            || self.report_fuel
            || self.main_module
            || self.digest
            || self.park_every.is_some()
            || self.snapshot_continue
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let mut flags = Flags::take(&mut refs);
    // `--snapshot-every N --snapshot-dir DIR` and `--park-every N`,
    // anywhere after `exec`.
    let mut durable: (Option<u64>, Option<String>) = (None, None);
    let positive = |option: &str, value: &str| match value.parse() {
        Ok(n) if n > 0 => Some(n),
        _ => {
            eprintln!("mm: {option}: `{value}` is not a positive number");
            None
        }
    };
    let mut k = 0;
    while k + 1 < refs.len() {
        match refs[k] {
            "--snapshot-every" => match positive(refs[k], refs[k + 1]) {
                Some(n) => durable.0 = Some(n),
                None => return ExitCode::from(2),
            },
            "--park-every" => match positive(refs[k], refs[k + 1]) {
                Some(n) => flags.park_every = Some(n),
                None => return ExitCode::from(2),
            },
            "--snapshot-dir" => durable.1 = Some(refs[k + 1].to_owned()),
            _ => {
                k += 1;
                continue;
            }
        }
        refs.drain(k..k + 2);
    }
    let durable = match durable {
        (Some(every), Some(dir)) => Some(Durable {
            every,
            dir: dir.into(),
        }),
        (None, None) => None,
        _ => {
            eprintln!("mm: --snapshot-every and --snapshot-dir go together\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    if (flags.snapshot_continue && flags.park_every.is_none())
        || (flags.park_every.is_some() && durable.is_some())
    {
        eprintln!(
            "mm: --snapshot-continue needs --park-every, which runs in one process \
             (not with --snapshot-every)\n{USAGE}"
        );
        return ExitCode::from(2);
    }
    match refs.as_slice() {
        ["version" | "--version"] if !flags.any() && durable.is_none() => {
            let id = micro_monty::build_id()
                .iter()
                .fold(String::new(), |mut id, b| {
                    let _ = std::fmt::Write::write_fmt(&mut id, format_args!("{b:02x}"));
                    id
                });
            println!("mm {}\nbuild_id {id}", micro_monty::VERSION);
            ExitCode::SUCCESS
        }
        ["run", file] if !flags.any() && durable.is_none() => run(file),
        ["resume", "--json", dir] if !flags.any() && durable.is_none() => resume_json(dir.as_ref()),
        ["exec", "--json", file] => {
            exec_json(file, EngineConfig::default(), &flags, durable.as_ref())
        }
        ["exec", "--json", "--recursion-limit", depth, file] => {
            let Ok(recursion_depth) = depth.parse() else {
                eprintln!("mm: --recursion-limit: `{depth}` is not a number\n{USAGE}");
                return ExitCode::from(2);
            };
            let limits = Limits::new().with_recursion_depth(recursion_depth);
            exec_json(
                file,
                EngineConfig::new().with_limits(limits),
                &flags,
                durable.as_ref(),
            )
        }
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

/// The local time zone `TZ` names, for `Session::set_local_timezone`: a
/// zoneinfo name under `TZDIR` or `/usr/share/zoneinfo`, a path, or a POSIX
/// TZ rule, as libc reads it. Without `TZ`, or when it is empty or names
/// nothing, the zone is UTC: the CLI never reads `/etc/localtime`.
fn local_zone() -> Option<Vec<u8>> {
    let read = |p: &std::path::Path| std::fs::read(p).ok().filter(|b| b.starts_with(b"TZif"));
    let tz = std::env::var_os("TZ")?;
    let tz = tz.to_string_lossy().into_owned();
    let tz = tz.strip_prefix(':').unwrap_or(&tz);
    if tz.is_empty() {
        return None;
    }
    if tz.starts_with('/') {
        return read(tz.as_ref());
    }
    let dir = std::env::var_os("TZDIR").unwrap_or_else(|| "/usr/share/zoneinfo".into());
    if !tz.split('/').any(|part| part == "..")
        && let Some(b) = read(&std::path::Path::new(&dir).join(tz))
    {
        return Some(b);
    }
    // A POSIX rule: TZif v2 data with no transitions and the rule as footer.
    let block = |out: &mut Vec<u8>| {
        out.extend_from_slice(b"TZif2");
        out.extend_from_slice(&[0; 15]);
        for count in [0u32, 0, 0, 0, 1, 4] {
            out.extend_from_slice(&count.to_be_bytes());
        }
        out.extend_from_slice(&[0, 0, 0, 0, 0, 0]);
        out.extend_from_slice(b"UTC\0");
    };
    let mut out = Vec::new();
    block(&mut out);
    block(&mut out);
    out.push(b'\n');
    out.extend_from_slice(tz.as_bytes());
    out.push(b'\n');
    Some(out)
}

/// The zone source `zoneinfo` reads: TZif files under `TZDIR` or
/// `/usr/share/zoneinfo`, by key (keys with `..` or a leading `/` name
/// nothing; the engine validates them first as well).
fn zone_source() -> micro_monty::ZoneSource {
    Box::new(|key: &str| {
        if key.starts_with('/') || key.split('/').any(|part| part == ".." || part.is_empty()) {
            return None;
        }
        let dir = std::env::var_os("TZDIR").unwrap_or_else(|| "/usr/share/zoneinfo".into());
        std::fs::read(std::path::Path::new(&dir).join(key))
            .ok()
            .filter(|b| b.starts_with(b"TZif"))
    })
}

/// `file` as an absolute path (as given if it cannot be resolved).
fn absolute(file: &str) -> String {
    std::fs::canonicalize(file).map_or_else(|_| file.to_owned(), |p| p.display().to_string())
}

fn read_source(file: &str) -> Result<String, String> {
    let bytes = std::fs::read(file).map_err(|e| format!("mm: can't open file '{file}': {e}"))?;
    String::from_utf8(bytes).map_err(|_| format!("mm: '{file}' is not valid UTF-8"))
}

fn run(file: &str) -> ExitCode {
    let source = match read_source(file) {
        Ok(s) => s,
        Err(msg) => {
            eprintln!("{msg}");
            return ExitCode::from(2);
        }
    };
    let engine = Engine::new(EngineConfig::default());
    let shown = display_path(file);
    let code = match engine.compile(&source, &shown) {
        Ok(c) => c,
        Err(e) => {
            eprint!("{}", e.rendered);
            return ExitCode::FAILURE;
        }
    };
    for w in code.warnings() {
        eprint!("{w}");
    }
    let mut session = match Session::new_with(&engine, settings(None)) {
        Ok(s) => s,
        Err(e) => {
            eprint!("{}", e.traceback);
            return ExitCode::FAILURE;
        }
    };
    session.set_last_expr_value(false);
    session.set_local_timezone(local_zone());
    session.set_zone_source(Some(zone_source()));
    if let Err(e) = session.set_module_dunders(Some("__main__"), Some(&absolute(file))) {
        eprint!("{}", e.traceback);
        return ExitCode::FAILURE;
    }
    let out = Arc::new(Mutex::new(BufWriter::new(std::io::stdout())));
    let sink = Arc::clone(&out);
    session.set_output_streams(Box::new(move |stream, s: &str| match stream {
        Stream::Stderr => {
            // Keep the two streams' order as CPython's unbuffered stderr does.
            let _ = lock(&sink).flush();
            let _ = std::io::stderr().write_all(s.as_bytes());
        }
        // Stdout, and any stream a later engine adds.
        _ => {
            let _ = lock(&sink).write_all(s.as_bytes());
        }
    }));
    let progress = match step_ms() {
        Some(limit) => match run_in_steps(&mut session, &code, limit) {
            Ok(p) => p,
            Err(over) => {
                let _ = lock(&out).flush();
                eprintln!(
                    "mm: a native step ran {over:?} past a 1 ms deadline (MM_STEP_MS={})",
                    limit.as_millis()
                );
                return ExitCode::from(3);
            }
        },
        None => session.start(&code),
    };
    let _ = lock(&out).flush();
    match progress {
        Progress::Complete(_) => ExitCode::SUCCESS,
        Progress::Error(e) => {
            eprint!("{}", e.traceback);
            ExitCode::FAILURE
        }
        other => {
            eprintln!("mm: stopped: {other:?}");
            ExitCode::FAILURE
        }
    }
}

/// `MM_STEP_MS`: the longest a start or resume of `mm run` may run past
/// its deadline.
fn step_ms() -> Option<Duration> {
    let ms: u64 = std::env::var("MM_STEP_MS").ok()?.parse().ok()?;
    (ms > 0).then(|| Duration::from_millis(ms))
}

/// Runs `code` with a 1 ms deadline on its start and every resume,
/// resuming each deadline park; the end, or how far past its deadline a
/// start or resume ran when that was more than `limit`.
fn run_in_steps(
    session: &mut Session,
    code: &micro_monty::Code,
    limit: Duration,
) -> Result<Progress, Duration> {
    let tick = Duration::from_millis(1);
    let mut t = Instant::now();
    session.set_deadline(Some(t + tick));
    let mut p = session.start(code);
    loop {
        let over = t.elapsed().saturating_sub(tick);
        if over > limit {
            return Err(over);
        }
        if p != (Progress::Parked {
            reason: ParkReason::Deadline,
        }) {
            session.set_deadline(None);
            return Ok(p);
        }
        t = Instant::now();
        session.set_deadline(Some(t + tick));
        // A park that cannot resume ends the run as `mm: stopped`.
        let Ok(next) = session.resume(Resume::Continue) else {
            return Ok(p);
        };
        p = next;
    }
}

fn exec_json(
    file: &str,
    config: EngineConfig,
    flags: &Flags,
    durable: Option<&Durable>,
) -> ExitCode {
    let source = match read_source(file) {
        Ok(s) => s,
        Err(msg) => {
            eprintln!("{msg}");
            return ExitCode::from(2);
        }
    };
    let engine = Engine::new(config);
    let stdout = Arc::new(Mutex::new(String::new()));
    let mut fuel_used = None;
    let mut digest = None;
    let entropy = Arc::new(AtomicU64::new(0));
    let entropy = flags.digest.then_some(&entropy);
    let (result, error) = match engine.compile(&source, &display_path(file)) {
        Err(e) => (
            serde_json::Value::Null,
            serde_json::json!({
                "type": e.type_name,
                "message": e.message,
                "traceback": e.rendered,
            }),
        ),
        Ok(code) => match Session::new_with(&engine, settings(entropy)) {
            Err(e) => (serde_json::Value::Null, exception_json(&e)),
            Ok(mut session) => {
                session.set_local_timezone(local_zone());
                session.set_zone_source(Some(zone_source()));
                let dunders = if flags.main_module {
                    session.set_module_dunders(Some("__main__"), Some(&absolute(file)))
                } else {
                    session.set_module_dunders(None, None)
                };
                if let Err(e) = dunders {
                    print_report(
                        "",
                        &serde_json::Value::Null,
                        &exception_json(&e),
                        None,
                        None,
                    );
                    return ExitCode::SUCCESS;
                }
                let sink = Arc::clone(&stdout);
                session.set_output(Box::new(move |s: &str| lock(&sink).push_str(s)));
                if flags.fixtures {
                    install_fixtures(&engine, &mut session);
                }
                // The fuel spent, counted exactly, parked or not: the
                // session's total (kept in the image) from `u64::MAX` down.
                let report_fuel = flags.report_fuel || flags.park_every.is_some();
                if durable.is_some() || report_fuel {
                    session.set_fuel_total(Some(u64::MAX));
                }
                if let Some(d) = durable {
                    session = yielding(session);
                    session.set_fuel(Some(d.every));
                    let first = session.start(&code);
                    let host = HostSide {
                        park: 0,
                        every: d.every,
                        fixtures: flags.fixtures,
                        later: BTreeMap::new(),
                        vfs: vfs::Vfs::default(),
                        entropy: entropy.cloned(),
                    };
                    return drive_durable(d, session, first, host, &stdout);
                }
                if flags.park_every.is_some() {
                    session = yielding(session);
                    session.set_fuel(flags.park_every);
                } else if report_fuel {
                    session.set_fuel(Some(u64::MAX));
                }
                let first = session.start(&code);
                let progress = drive(
                    &mut session,
                    first,
                    flags.fixtures,
                    flags.park_every,
                    flags.snapshot_continue,
                );
                if report_fuel {
                    fuel_used = spent(&session);
                }
                if flags.digest {
                    digest = Some(final_digest(&mut session));
                }
                report_parts(progress)
            }
        },
    };
    print_report(
        &lock(&stdout),
        &result,
        &error,
        fuel_used,
        digest.as_deref(),
    );
    ExitCode::SUCCESS
}

/// The final-state digest (`--digest`): the checksum that ends the
/// session's image, taken with the fuel and its total cleared so it covers
/// the state alone, or why there is no image.
fn final_digest(session: &mut Session) -> String {
    session.set_fuel(None);
    session.set_fuel_total(None);
    match session.snapshot() {
        Ok(bytes) => bytes
            .iter()
            .rev()
            .take(8)
            .fold(String::new(), |mut hex, b| {
                let _ = std::fmt::Write::write_fmt(&mut hex, format_args!("{b:02x}"));
                hex
            }),
        Err(e) => format!("no image: {e}"),
    }
}

/// The fuel the session's cells spent, exactly: what `exec_json` set as
/// the total less what is left of it.
fn spent(session: &Session) -> Option<u64> {
    session.fuel_total().map(|left| u64::MAX - left)
}

/// The `result` and `error` of a report for how a run ended.
fn report_parts(progress: Progress) -> (serde_json::Value, serde_json::Value) {
    match progress {
        Progress::Complete(Some(c)) => (
            serde_json::json!({"repr": c.repr, "str": c.str, "type": c.type_name}),
            serde_json::Value::Null,
        ),
        Progress::Complete(None) => (serde_json::Value::Null, serde_json::Value::Null),
        Progress::Error(e) => (serde_json::Value::Null, exception_json(&e)),
        other => (
            serde_json::Value::Null,
            serde_json::json!({
                "type": "Parked",
                "message": format!("{other:?}"),
                "traceback": "",
            }),
        ),
    }
}

/// Prints the report; `fuel` (with `--report-fuel` or `--park-every`, and
/// at the end of a parked chain) is the fuel the run used: the safepoints
/// it ran, plus what natives charged; `digest`, with `--digest`, the
/// final-state digest.
fn print_report(
    stdout: &str,
    result: &serde_json::Value,
    error: &serde_json::Value,
    fuel: Option<u64>,
    digest: Option<&str>,
) {
    let exit = i32::from(!error.is_null());
    let mut doc = serde_json::json!({
        "stdout": stdout,
        "result": result,
        "error": error,
        "exit": exit,
    });
    if let Some(f) = fuel {
        doc["fuel"] = f.into();
    }
    if let Some(d) = digest {
        doc["digest"] = d.into();
    }
    println!("{doc}");
}

/// Where and how often a durable run saves itself.
struct Durable {
    every: u64,
    dir: std::path::PathBuf,
}

/// The host's side of a durable run, saved beside the snapshot: what the
/// engine cannot own (docs/spec.md §9.1).
struct HostSide {
    /// Parks so far.
    park: u64,
    every: u64,
    fixtures: bool,
    /// Results owed to host futures, by call id.
    later: BTreeMap<u64, Result<HostValue, HostError>>,
    /// The fixtures' filesystem.
    vfs: vfs::Vfs,
    /// With `--digest`: the entropy counter (`settings`).
    entropy: Option<Arc<AtomicU64>>,
}

const SNAPSHOT_FILE: &str = "session.mmsnap";
const STATE_FILE: &str = "state.json";

/// wasmtime's WASI refuses one read or write of more than 128 MiB
/// (`ENOMEM`) [M], and an image near the default memory limit is larger,
/// so the image file moves in pieces of this size.
const IMAGE_PIECE: usize = 16 << 20;

fn write_image(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut file = std::fs::File::create(path)?;
    bytes
        .chunks(IMAGE_PIECE)
        .try_for_each(|piece| file.write_all(piece))
}

fn read_image(path: &std::path::Path) -> std::io::Result<Vec<u8>> {
    let mut file = std::fs::File::open(path)?;
    let mut bytes = Vec::new();
    let mut piece = vec![0; IMAGE_PIECE];
    loop {
        match file.read(&mut piece)? {
            0 => return Ok(bytes),
            n => bytes.extend_from_slice(&piece[..n]),
        }
    }
}

/// Runs until the cell ends or stops; at a stop, saves the session and the
/// host side to `d.dir` with the answer the stop needs, prints the park
/// line and exits.
fn drive_durable(
    d: &Durable,
    mut session: Session,
    progress: Progress,
    mut host: HostSide,
    stdout: &Arc<Mutex<String>>,
) -> ExitCode {
    let (answer, reason) = match progress {
        Progress::Parked { .. } => (serde_json::json!("continue"), "fuel"),
        Progress::HostCall {
            call,
            name,
            args,
            kwargs,
            ..
        } if host.fixtures => {
            let base = name.rsplit('.').next().unwrap_or(&name);
            let answer = match host
                .vfs
                .call(&name, &args)
                .map_or_else(|| fixture(base, &args, &kwargs), Answer::Now)
            {
                Answer::Now(Ok(v)) => serde_json::json!({"value": host_json(&v)}),
                Answer::Now(Err(e)) => serde_json::json!({"error": error_json(&e)}),
                Answer::Later(r) => {
                    host.later.insert(call, r);
                    serde_json::json!("future")
                }
            };
            (answer, "host-call")
        }
        Progress::Waiting { calls } if host.fixtures => {
            let batch: Vec<serde_json::Value> = calls
                .iter()
                .filter_map(|c| {
                    host.later
                        .remove(c)
                        .map(|r| serde_json::json!([c, result_json(&r)]))
                })
                .collect();
            if batch.is_empty() {
                let (result, error) = report_parts(progress_error(
                    "the cell waits on calls the fixtures never saw",
                ));
                print_report(&lock(stdout), &result, &error, None, None);
                return ExitCode::SUCCESS;
            }
            (serde_json::json!({"batch": batch}), "waiting")
        }
        other => {
            let (result, error) = report_parts(other);
            let fuel = spent(&session);
            let digest = host.entropy.is_some().then(|| final_digest(&mut session));
            print_report(&lock(stdout), &result, &error, fuel, digest.as_deref());
            return ExitCode::SUCCESS;
        }
    };
    host.park += 1;
    let bytes = match session.snapshot() {
        Ok(b) => b,
        Err(e) => {
            eprintln!("mm: snapshot failed: {e}");
            return ExitCode::FAILURE;
        }
    };
    let later: Vec<serde_json::Value> = host
        .later
        .iter()
        .map(|(c, r)| serde_json::json!([c, result_json(r)]))
        .collect();
    let state = serde_json::json!({
        "park": host.park,
        "every": host.every,
        "fixtures": host.fixtures,
        "stdout": lock(stdout).as_str(),
        "answer": answer,
        "later": later,
        "vfs": host.vfs.to_json(),
        "digest": host.entropy.is_some(),
        "entropy": host.entropy.as_ref().map_or(0, |e| e.load(Ordering::Relaxed)),
    });
    let written = std::fs::create_dir_all(&d.dir)
        .and_then(|()| write_image(&d.dir.join(SNAPSHOT_FILE), &bytes))
        .and_then(|()| std::fs::write(d.dir.join(STATE_FILE), state.to_string()));
    if let Err(e) = written {
        eprintln!("mm: cannot save to {}: {e}", d.dir.display());
        return ExitCode::FAILURE;
    }
    let line = serde_json::json!({
        "park": host.park,
        "reason": reason,
        "bytes": bytes.len(),
        "stdout": lock(stdout).as_str(),
    });
    println!("{line}");
    ExitCode::SUCCESS
}

/// `mm resume --json DIR`: restores what `drive_durable` saved and
/// continues with the answer it recorded.
fn resume_json(dir: &std::path::Path) -> ExitCode {
    let fail = |msg: String| {
        eprintln!("mm: {msg}");
        ExitCode::FAILURE
    };
    let state: serde_json::Value = match std::fs::read_to_string(dir.join(STATE_FILE))
        .map_err(|e| e.to_string())
        .and_then(|t| serde_json::from_str(&t).map_err(|e| e.to_string()))
    {
        Ok(v) => v,
        Err(e) => return fail(format!("{}: {e}", dir.join(STATE_FILE).display())),
    };
    let bytes = match read_image(&dir.join(SNAPSHOT_FILE)) {
        Ok(b) => b,
        Err(e) => return fail(format!("{}: {e}", dir.join(SNAPSHOT_FILE).display())),
    };
    let entropy = (state["digest"] == true)
        .then(|| Arc::new(AtomicU64::new(state["entropy"].as_u64().unwrap_or(0))));
    let engine = Engine::new(EngineConfig::default());
    let mut session = match engine
        .restore_with(&bytes, settings(entropy.as_ref()))
        .map(yielding)
    {
        Ok(s) => s,
        Err(e) => return fail(format!("cannot restore {}: {e}", dir.display())),
    };
    session.set_zone_source(Some(zone_source()));
    let mut later = BTreeMap::new();
    for entry in state["later"].as_array().into_iter().flatten() {
        if let (Some(c), Some(r)) = (entry[0].as_u64(), json_result(&entry[1])) {
            later.insert(c, r);
        }
    }
    let host = HostSide {
        park: state["park"].as_u64().unwrap_or(0),
        every: state["every"].as_u64().unwrap_or(1),
        fixtures: state["fixtures"].as_bool().unwrap_or(false),
        later,
        vfs: vfs::Vfs::from_json(&state["vfs"]),
        entropy,
    };
    let stdout = Arc::new(Mutex::new(
        state["stdout"].as_str().unwrap_or_default().to_owned(),
    ));
    let sink = Arc::clone(&stdout);
    session.set_output(Box::new(move |s: &str| lock(&sink).push_str(s)));
    session.set_fuel(Some(host.every));
    let answer = &state["answer"];
    let resume = if answer == "continue" {
        Resume::Continue
    } else if answer == "future" {
        Resume::Future
    } else if let Some(v) = answer.get("value").and_then(|v| json_host(v, 0)) {
        Resume::Value(v)
    } else if let Some(e) = answer.get("error").and_then(json_error) {
        Resume::Error(e)
    } else if let Some(batch) = answer.get("batch").and_then(serde_json::Value::as_array) {
        Resume::Batch(
            batch
                .iter()
                .filter_map(|e| Some((e[0].as_u64()?, json_result(&e[1])?)))
                .collect(),
        )
    } else {
        return fail(format!(
            "{}: no answer recorded",
            dir.join(STATE_FILE).display()
        ));
    };
    let progress = match session.resume(resume) {
        Ok(p) => p,
        Err(e) => return fail(format!("the restored session refused its answer: {e}")),
    };
    let d = Durable {
        every: host.every,
        dir: dir.to_owned(),
    };
    drive_durable(&d, session, progress, host, &stdout)
}

/// A host value as JSON for `state.json` (tagged, so it reads back exactly).
fn host_json(v: &HostValue) -> serde_json::Value {
    use serde_json::json;
    match v {
        HostValue::Bool(b) => json!(["bool", b]),
        HostValue::Int(i) => json!(["int", i]),
        HostValue::BigInt(s) => json!(["bigint", s]),
        HostValue::Float(f) => json!(["float", f.to_bits()]),
        HostValue::Str(s) => json!(["str", s]),
        HostValue::Bytes(b) => json!(["bytes", b]),
        HostValue::List(items) => json!(["list", items.iter().map(host_json).collect::<Vec<_>>()]),
        HostValue::Tuple(items) => {
            json!(["tuple", items.iter().map(host_json).collect::<Vec<_>>()])
        }
        HostValue::Dict(fields) => json!(["dict", fields_json(fields)]),
        HostValue::Record { name, fields } => json!(["record", name, fields_json(fields)]),
        HostValue::Object { class, fields } => json!(["object", class, fields_json(fields)]),
        // `None`; the CLI converts strictly, so nothing else arrives.
        _ => json!(["none"]),
    }
}

fn fields_json(fields: &[(String, HostValue)]) -> Vec<serde_json::Value> {
    fields
        .iter()
        .map(|(k, v)| serde_json::json!([k, host_json(v)]))
        .collect()
}

/// Reads `host_json`'s output back (at most `MAX_HOST_DEPTH` deep).
fn json_host(v: &serde_json::Value, depth: u32) -> Option<HostValue> {
    if depth > micro_monty::MAX_HOST_DEPTH {
        return None;
    }
    let fields = |f: &serde_json::Value| -> Option<Vec<(String, HostValue)>> {
        f.as_array()?
            .iter()
            .map(|kv| Some((kv[0].as_str()?.to_owned(), json_host(&kv[1], depth + 1)?)))
            .collect()
    };
    Some(match v[0].as_str()? {
        "none" => HostValue::None,
        "bool" => HostValue::Bool(v[1].as_bool()?),
        "int" => HostValue::Int(v[1].as_i64()?),
        "bigint" => HostValue::BigInt(v[1].as_str()?.to_owned()),
        "float" => HostValue::Float(f64::from_bits(v[1].as_u64()?)),
        "str" => HostValue::Str(v[1].as_str()?.to_owned()),
        "bytes" => HostValue::Bytes(
            v[1].as_array()?
                .iter()
                .map(|b| b.as_u64().and_then(|b| u8::try_from(b).ok()))
                .collect::<Option<_>>()?,
        ),
        "list" | "tuple" => {
            let items = v[1]
                .as_array()?
                .iter()
                .map(|x| json_host(x, depth + 1))
                .collect::<Option<_>>()?;
            if v[0] == "tuple" {
                HostValue::Tuple(items)
            } else {
                HostValue::List(items)
            }
        }
        "dict" => HostValue::Dict(fields(&v[1])?),
        "record" => HostValue::Record {
            name: v[1].as_str()?.to_owned(),
            fields: fields(&v[2])?,
        },
        "object" => HostValue::Object {
            class: v[1].as_str()?.to_owned(),
            fields: fields(&v[2])?,
        },
        _ => return None,
    })
}

fn error_json(e: &HostError) -> serde_json::Value {
    serde_json::json!([e.kind.name(), e.message])
}

fn json_error(v: &serde_json::Value) -> Option<HostError> {
    Some(HostError {
        kind: HostErrorKind::from_name(v[0].as_str()?)?,
        message: v[1].as_str()?.to_owned(),
        data: None,
        class: None,
    })
}

fn result_json(r: &Result<HostValue, HostError>) -> serde_json::Value {
    match r {
        Ok(v) => serde_json::json!({"value": host_json(v)}),
        Err(e) => serde_json::json!({"error": error_json(e)}),
    }
}

fn json_result(v: &serde_json::Value) -> Option<Result<HostValue, HostError>> {
    if let Some(x) = v.get("value") {
        return json_host(x, 0).map(Ok);
    }
    v.get("error").and_then(json_error).map(Err)
}

/// The script path as CPython shows it in tracebacks: made absolute
/// (without resolving symlinks).
fn display_path(file: &str) -> String {
    std::path::absolute(file).map_or_else(|_| file.to_owned(), |p| p.to_string_lossy().into_owned())
}

/// Fixture constants (Monty's NameLookup values), defined as globals.
const FIXTURE_CONSTANTS: &str = "CONST_INT = 42\nCONST_STR = 'hello'\nCONST_FLOAT = 3.14\n\
CONST_BOOL = True\nCONST_LIST = [1, 2, 3]\nCONST_NONE = None\n";

/// The host functions every `call-external` case may call.
const FIXTURE_FUNCTIONS: [&str; 11] = [
    "add_ints",
    "concat_strings",
    "return_value",
    "get_list",
    "raise_error",
    "async_call",
    "async_fail",
    "make_point",
    "make_mutable_point",
    "make_user",
    "make_empty",
];

/// The functions of the `tools` host module.
const TOOLS_FUNCTIONS: [&str; 7] = [
    "add_ints",
    "concat_strings",
    "return_value",
    "get_list",
    "raise_error",
    "async_call",
    "async_fail",
];

fn install_fixtures(engine: &Engine, session: &mut Session) {
    let module = |name: &str, functions: &mut dyn Iterator<Item = &str>| HostModule {
        name: name.to_owned(),
        functions: functions.map(HostSignature::any).collect(),
        ..HostModule::default()
    };
    let mut builtins = FIXTURE_FUNCTIONS.into_iter().chain(vfs::FUNCTIONS);
    let _ = session.define(&module("", &mut builtins));
    let _ = session.define(&module("tools", &mut TOOLS_FUNCTIONS.into_iter()));
    let _ = session.define(&HostModule {
        constants: vec![("VERSION".to_owned(), HostValue::Int(7))],
        ..module("tools.nested", &mut ["return_value"].into_iter())
    });
    if let Ok(code) = engine.compile(FIXTURE_CONSTANTS, "<fixtures>") {
        let _ = session.start(&code);
    }
}

/// How a fixture answers.
enum Answer {
    Now(Result<HostValue, HostError>),
    Later(Result<HostValue, HostError>),
}

fn host_error(kind: &str, message: &str) -> HostError {
    HostError {
        kind: HostErrorKind::from_name(kind).unwrap_or(HostErrorKind::RuntimeError),
        message: message.to_owned(),
        data: None,
        class: None,
    }
}

fn type_name(v: &HostValue) -> &'static str {
    match v {
        HostValue::None => "NoneType",
        HostValue::Bool(_) => "bool",
        HostValue::Int(_) | HostValue::BigInt(_) => "int",
        HostValue::Float(_) => "float",
        HostValue::Str(_) => "str",
        HostValue::Bytes(_) => "bytes",
        HostValue::List(_) => "list",
        HostValue::Dict(_) => "dict",
        HostValue::Record { .. } | HostValue::Tuple(_) => "tuple",
        // The CLI defines no host classes and converts strictly.
        _ => "object",
    }
}

/// Runs fixture `name` (as `test_fixtures.py` defines it) on its arguments.
fn fixture(name: &str, args: &[HostValue], kwargs: &[(String, HostValue)]) -> Answer {
    let params: &[&str] = match name {
        "add_ints" | "concat_strings" => &["a", "b"],
        "return_value" | "async_call" => &["x"],
        "raise_error" | "async_fail" => &["exc_type", "message"],
        _ => &[],
    };
    let mut bound: Vec<Option<&HostValue>> = args.iter().map(Some).collect();
    bound.resize(params.len().max(args.len()), None);
    for (k, v) in kwargs {
        let Some(i) = params.iter().position(|p| p == k) else {
            let msg = format!("{name}() got an unexpected keyword argument '{k}'");
            return Answer::Now(Err(host_error("TypeError", &msg)));
        };
        bound[i] = Some(v);
    }
    if bound.len() != params.len() || bound.iter().any(Option::is_none) {
        let msg = format!(
            "{name}() takes {} arguments ({} given)",
            params.len(),
            args.len() + kwargs.len()
        );
        return Answer::Now(Err(host_error("TypeError", &msg)));
    }
    let arg = |i: usize| bound[i].cloned().unwrap_or(HostValue::None);
    let text = |v: &HostValue| match v {
        HostValue::Str(s) => s.clone(),
        _ => String::new(),
    };
    match name {
        "add_ints" => Answer::Now(match (arg(0), arg(1)) {
            (HostValue::Int(a), HostValue::Int(b)) => a.checked_add(b).map_or_else(
                || {
                    Ok(HostValue::BigInt(
                        (i128::from(a) + i128::from(b)).to_string(),
                    ))
                },
                |s| Ok(HostValue::Int(s)),
            ),
            (a, b) => Err(host_error(
                "TypeError",
                &format!(
                    "unsupported operand type(s) for +: '{}' and '{}'",
                    type_name(&a),
                    type_name(&b)
                ),
            )),
        }),
        "concat_strings" => Answer::Now(match (arg(0), arg(1)) {
            (HostValue::Str(a), HostValue::Str(b)) => Ok(HostValue::Str(a + &b)),
            (a, b) => Err(host_error(
                "TypeError",
                &format!(
                    "unsupported operand type(s) for +: '{}' and '{}'",
                    type_name(&a),
                    type_name(&b)
                ),
            )),
        }),
        "return_value" => Answer::Now(Ok(arg(0))),
        "get_list" => Answer::Now(Ok(HostValue::List(vec![
            HostValue::Int(1),
            HostValue::Int(2),
            HostValue::Int(3),
        ]))),
        "raise_error" => Answer::Now(Err(host_error(&text(&arg(0)), &text(&arg(1))))),
        "async_call" => Answer::Later(Ok(arg(0))),
        "async_fail" => Answer::Later(Err(host_error(&text(&arg(0)), &text(&arg(1))))),
        _ => Answer::Now(Err(HostError {
            kind: HostErrorKind::NotImplementedError,
            message: format!(
                "mm's host fixtures do not provide host objects ({name}) until classes land (M4)"
            ),
            data: None,
            class: None,
        })),
    }
}

/// Runs the cell on in this process until it ends or stops for good: with
/// `fixtures`, host calls and waits are answered with the fixtures; with
/// `every` (`--park-every`), every stop gives the cell `every` more fuel,
/// as each process of a parked chain does, and a park resumes; with `snap`
/// (`--snapshot-continue`), every stop first takes a snapshot, which is
/// dropped: the same session goes on.
fn drive(
    session: &mut Session,
    mut progress: Progress,
    fixtures: bool,
    every: Option<u64>,
    snap: bool,
) -> Progress {
    let mut later: BTreeMap<u64, Result<HostValue, HostError>> = BTreeMap::new();
    let mut vfs = vfs::Vfs::default();
    loop {
        let resume = match progress {
            Progress::Parked { .. } if every.is_some() => Resume::Continue,
            Progress::HostCall {
                call,
                name,
                args,
                kwargs,
                ..
            } if fixtures => {
                let base = name.rsplit('.').next().unwrap_or(&name);
                match vfs
                    .call(&name, &args)
                    .map_or_else(|| fixture(base, &args, &kwargs), Answer::Now)
                {
                    Answer::Now(Ok(v)) => Resume::Value(v),
                    Answer::Now(Err(e)) => Resume::Error(e),
                    Answer::Later(r) => {
                        later.insert(call, r);
                        Resume::Future
                    }
                }
            }
            Progress::Waiting { calls } if fixtures => {
                let batch: Vec<_> = calls
                    .iter()
                    .filter_map(|c| later.remove(c).map(|r| (*c, r)))
                    .collect();
                if batch.is_empty() {
                    return progress_error("the cell waits on calls the fixtures never saw");
                }
                Resume::Batch(batch)
            }
            other => return other,
        };
        if snap && let Err(e) = session.snapshot() {
            return progress_error(&format!("snapshot failed: {e}"));
        }
        if every.is_some() {
            session.set_fuel(every);
        }
        progress = match session.resume(resume) {
            Ok(p) => p,
            Err(e) => return progress_error(&e.to_string()),
        };
    }
}

fn progress_error(message: &str) -> Progress {
    Progress::Error(micro_monty::PyException::new(
        "HostFixtureError".to_owned(),
        message.to_owned(),
        format!("HostFixtureError: {message}\n"),
        false,
    ))
}

fn exception_json(e: &micro_monty::PyException) -> serde_json::Value {
    serde_json::json!({
        "type": e.type_name,
        "message": e.message,
        "traceback": e.traceback,
    })
}

/// Locks an output buffer; a panic while it was held cannot leave it
/// half-written in a way that matters, so poisoning is ignored.
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}
