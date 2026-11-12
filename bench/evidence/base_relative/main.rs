//! `br`: a host-side prototype of base-relative snapshots
//! (docs/research/base-relative-snapshots.md). No engine change: it takes the
//! unchanged engine's FULL image (`Session::snapshot`), compares its heap
//! segments block by block with the build's fresh-session image (the
//! "base", `Session::new` then `snapshot`, kept once per build by the host)
//! and writes a base id plus the changed blocks. `apply` rebuilds the exact
//! FULL bytes, which `Engine::restore` then reads as usual.
//!
//! ```text
//! br base OUT [CODE]                write this build's base image (after CODE: a warm base)
//! br program FILE --base B --every N [--swap] [--measure all|first] [--dump DIR]
//! br codemode --base B [--swap] [--measure all] [--dump DIR]   (mm-codemode's protocol)
//! br reject --base B --wrong W --image FULL     wrong-base rejection
//! ```
//!
//! Metrics go to `--metrics PATH` or `$BR_METRICS` (JSON lines, appended); `--swap` replaces the
//! session at every park with one restored from base + delta, as a
//! durability run does with FULL images.
#![forbid(unsafe_code)]

use micro_monty::{
    Engine, EngineConfig, HostError, HostErrorKind, HostModule, HostValue, Limits, Progress,
    Resume, Session, Stream,
};
use mm_core::xxh64;

mod delta;
use delta::{Base, PREFIX, Res, SEGMENT, Seg, apply, base_id, encode, parse};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fmt::Write as _,
    io::{BufRead, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, PoisonError},
    time::Instant,
};

// ---------------------------------------------------------------------------
// Measurement.

const BLOCKS: [(u32, &str); 5] = [
    (64, "b64"),
    (256, "b256"),
    (1024, "b1k"),
    (4096, "b4k"),
    (SEGMENT, "seg"),
];

fn best<T>(n: usize, mut f: impl FnMut() -> T) -> (T, u128) {
    let mut out = None;
    let mut ns = u128::MAX;
    for _ in 0..n {
        let start = Instant::now();
        let v = f();
        ns = ns.min(start.elapsed().as_nanos());
        out = Some(v);
    }
    (out.unwrap_or_else(|| f()), ns)
}

struct Ctx {
    engine: Engine,
    base: Base,
    base_bytes: Vec<u8>,
    name: String,
    dump: Option<PathBuf>,
    metrics: Option<std::fs::File>,
    /// Which parks to measure: `all`, `first` or `none`.
    measure: String,
    parks: u64,
}

impl Ctx {
    /// Measures the park (when asked) and returns the session to continue
    /// with: restored from base + delta under `--swap`, else the same one.
    fn park(&mut self, session: &mut Session, swap: bool) -> Res<Option<Session>> {
        self.parks += 1;
        let measure = self.measure == "all" || (self.measure == "first" && self.parks == 1);
        if !measure && !swap {
            return Ok(None);
        }
        let (full, snap_ns) = if measure {
            best(3, || session.snapshot())
        } else {
            best(1, || session.snapshot())
        };
        let full = full.map_err(|e| format!("snapshot: {e}"))?;
        let (delta, _) = best(1, || encode(&full, &self.base, 256));
        let delta = delta?;
        let rebuilt = apply(&delta, &self.base, true)?;
        if rebuilt != full {
            return Err(format!(
                "{}: park {}: rebuilt image differs",
                self.name, self.parks
            ));
        }
        if measure {
            self.record(&full, snap_ns)?;
        }
        if swap {
            let s = self
                .engine
                .restore(&rebuilt)
                .map_err(|e| format!("restore: {e}"))?;
            return Ok(Some(s));
        }
        Ok(None)
    }

    fn record(&mut self, full: &[u8], snap_ns: u128) -> Res<()> {
        let t = parse(full)?;
        let base = &self.base;
        let (mut live, mut same_bytes, mut segs, mut segs_in_base, mut segs_same) = (0, 0, 0, 0, 0);
        let mut per_space = Vec::new();
        for (si, s) in t.spaces.iter().enumerate() {
            let (mut sl, mut ss) = (0usize, 0usize);
            for g in &s.segs {
                segs += 1;
                sl += g.live_bytes();
                let Some(b) = base.seg(si, g.idx) else {
                    continue;
                };
                segs_in_base += 1;
                for &(off, units) in &g.runs {
                    for u in off as usize..(off + units) as usize {
                        let a = u * 8;
                        if u < b.top as usize
                            && b.is_live(u)
                            && b.dense[a..a + 8] == g.dense[a..a + 8]
                        {
                            ss += 8;
                        }
                    }
                }
                if g.runs == b.runs && g.dense == b.dense {
                    segs_same += 1;
                }
            }
            per_space.push(json!({"live": sl, "same": ss, "segs": s.segs.len()}));
            live += sl;
            same_bytes += ss;
        }
        let base_live: usize = base
            .img
            .spaces
            .iter()
            .flat_map(|s| &s.segs)
            .map(Seg::live_bytes)
            .sum();
        let mut sizes = serde_json::Map::new();
        let mut timing = serde_json::Map::new();
        for (block, label) in BLOCKS {
            let (delta, enc_ns) = best(5, || encode(full, base, block));
            let delta = delta?;
            let (rebuilt, app_ns) = best(5, || apply(&delta, base, true));
            if rebuilt? != full {
                return Err(format!("{}: {label}: rebuilt image differs", self.name));
            }
            sizes.insert(label.into(), json!(delta.len()));
            timing.insert(
                label.into(),
                json!({"encode_ns": enc_ns, "apply_ns": app_ns}),
            );
            if let Some(dir) = &self.dump {
                std::fs::write(dir.join(format!("{}.{label}", self.parks)), &delta)
                    .map_err(|e| e.to_string())?;
            }
        }
        if let Some(dir) = &self.dump {
            std::fs::write(dir.join(format!("{}.full", self.parks)), full)
                .map_err(|e| e.to_string())?;
        }
        let (restored, restore_ns) = best(3, || self.engine.restore(full).map(drop));
        restored.map_err(|e| format!("restore: {e}"))?;
        let row = json!({
            "image": self.name, "park": self.parks, "full_bytes": full.len(),
            "prefix_bytes": PREFIX + 8, "vm_bytes": t.vm.len(), "heap_live_bytes": live,
            "heap_same_bytes": same_bytes, "base_live_bytes": base_live,
            "base_bytes": self.base_bytes.len(),
            "segments": segs, "segments_in_base": segs_in_base, "segments_identical": segs_same,
            "base_segments": base.img.spaces.iter().map(|s| s.segs.len()).sum::<usize>(),
            "spaces": per_space, "delta_bytes": sizes, "delta_ns": timing,
            "snapshot_ns": snap_ns, "restore_ns": restore_ns,
        });
        if let Some(f) = &mut self.metrics {
            writeln!(f, "{row}").map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Drivers.

fn arg(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1).cloned())
}

fn ctx(args: &[String], engine: Engine, name: String) -> Res<Ctx> {
    let base_path = arg(args, "--base").ok_or("--base is required")?;
    let base_bytes = std::fs::read(&base_path).map_err(|e| format!("{base_path}: {e}"))?;
    let path = arg(args, "--metrics")
        .map(Into::into)
        .or_else(|| std::env::var_os("BR_METRICS"));
    let metrics = match path {
        Some(p) => Some(
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(p)
                .map_err(|e| e.to_string())?,
        ),
        None => None,
    };
    Ok(Ctx {
        engine,
        base: Base::new(&base_bytes)?,
        base_bytes,
        name,
        dump: arg(args, "--dump").map(PathBuf::from),
        metrics,
        measure: arg(args, "--measure").unwrap_or_else(|| "none".into()),
        parks: 0,
    })
}

/// The fresh-session image; with `code`, after running it (a "warm" base).
fn fresh_image(code: Option<&String>) -> Res<Vec<u8>> {
    let engine = Engine::new(EngineConfig::default());
    let mut s = Session::new(&engine).map_err(|e| e.traceback)?;
    if let Some(code) = code {
        let code = engine.compile(code, "<base>").map_err(|e| e.rendered)?;
        if let Progress::Error(e) = s.start(&code) {
            return Err(e.traceback);
        }
    }
    s.snapshot().map_err(|e| e.to_string())
}

/// Runs FILE parked every N fuel units; prints the output and the result.
fn program(args: &[String]) -> Res<()> {
    let file = args.get(2).ok_or("program FILE")?;
    let every: u64 = arg(args, "--every")
        .and_then(|n| n.parse().ok())
        .ok_or("--every N")?;
    let swap = args.iter().any(|a| a == "--swap");
    let source = std::fs::read_to_string(file).map_err(|e| format!("{file}: {e}"))?;
    let name = arg(args, "--name").unwrap_or_else(|| file.clone());
    let mut c = ctx(args, Engine::new(EngineConfig::default()), name)?;
    let code = c.engine.compile(&source, file).map_err(|e| e.rendered)?;
    let out = Arc::new(Mutex::new(String::new()));
    let attach = |s: &mut Session| {
        let sink = Arc::clone(&out);
        s.set_output(Box::new(move |t: &str| {
            sink.lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push_str(t);
        }));
        s.set_fuel(Some(every));
    };
    let mut session = Session::new(&c.engine).map_err(|e| e.traceback)?;
    session
        .set_module_dunders(None, None)
        .map_err(|e| e.traceback)?;
    attach(&mut session);
    let mut p = session.start(&code);
    let result = loop {
        match p {
            Progress::Parked { .. } => {
                if let Some(s) = c.park(&mut session, swap)? {
                    session = s;
                }
                attach(&mut session);
                p = session
                    .resume(Resume::Continue)
                    .map_err(|e| e.to_string())?;
            }
            Progress::Complete(v) => break json!(v.map(|v| v.repr)),
            Progress::Error(e) => break json!({"error": e.traceback}),
            other => break json!({"stopped": format!("{other:?}")}),
        }
    };
    let stdout = out.lock().unwrap_or_else(PoisonError::into_inner).clone();
    println!(
        "{}",
        json!({"stdout": stdout, "result": result, "parks": c.parks})
    );
    Ok(())
}

/// A delta written against `--base`, applied to `--wrong`: refused by the
/// base id; with the id forced equal, refused by the FULL checksum; with
/// both checks gone (the trailer recomputed over what was rebuilt), what
/// `Engine::restore` makes of it.
fn reject(args: &[String]) -> Res<()> {
    let read = |f: &str| {
        let p = arg(args, f).ok_or(f.to_owned())?;
        std::fs::read(&p).map_err(|e| format!("{p}: {e}"))
    };
    let (base, wrong, full) = (read("--base")?, read("--wrong")?, read("--image")?);
    let engine = Engine::new(EngineConfig::default());
    let delta = encode(&full, &Base::new(&base)?, 256)?;
    let right = apply(&delta, &Base::new(&base)?, true).and_then(|f| {
        engine
            .restore(&f)
            .map(drop)
            .map_err(|e| format!("restore: {e}"))
    });
    let by_id = apply(&delta, &Base::new(&wrong)?, true).map(|f| f.len());
    let mut forced = Base::new(&wrong)?;
    forced.id = base_id(&base);
    let by_checksum = apply(&delta, &forced, true).map(|f| f.len());
    let by_restore = apply(&delta, &forced, false).and_then(|f| {
        engine
            .restore(&f)
            .map(drop)
            .map_err(|e| format!("restore: {e}"))
    });
    let unchecked = apply(&delta, &forced, false).and_then(|mut f| {
        let n = f.len() - 8;
        let sum = xxh64(&f[..n], 0);
        f[n..].copy_from_slice(&sum.to_le_bytes());
        let same = f == full;
        let restored = engine
            .restore(&f)
            .map(drop)
            .map_err(|e| format!("restore: {e}"));
        Ok(format!(
            "rebuilt == original: {same}; restore: {restored:?}"
        ))
    });
    println!(
        "{}",
        json!({
            "right_base": format!("{right:?}"),
            "wrong_base": format!("{by_id:?}"),
            "wrong_base_id_forced": format!("{by_checksum:?}"),
            "wrong_base_id_forced_no_rebuild_check": format!("{by_restore:?}"),
            "wrong_base_no_checks_at_all": format!("{unchecked:?}"),
        })
    );
    Ok(())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let result = match args.get(1).map(String::as_str) {
        Some("base") => args
            .get(2)
            .ok_or_else(|| "base OUT".to_owned())
            .and_then(|out| {
                let img = fresh_image(args.get(3))?;
                std::fs::write(out, &img).map_err(|e| e.to_string())?;
                println!("{} bytes, xxh64 {:016x}", img.len(), xxh64(&img, 0));
                Ok(())
            }),
        Some("program") => program(&args),
        Some("codemode") => codemode(&args),
        Some("reject") => reject(&args),
        _ => Err("usage: see the crate docs".into()),
    };
    if let Err(e) = result {
        eprintln!("br: {e}");
        std::process::exit(1);
    }
}

// ---------------------------------------------------------------------------
// Codemode: conformance/codemode/runner's protocol and session setup, with a
// park at every host call and wait. The value codec is copied from
// conformance/codemode/runner/src/main.rs (evidence code; the runner is not
// changed).

type Output = Arc<Mutex<(String, String)>>;

fn attach_codemode(session: &mut Session, output: &Output, clock: f64, counter: &Arc<Mutex<u8>>) {
    let sink = Arc::clone(output);
    session.set_output_streams(Box::new(move |stream, text| {
        let mut streams = sink.lock().unwrap_or_else(PoisonError::into_inner);
        let target = if stream == Stream::Stderr {
            &mut streams.1
        } else {
            &mut streams.0
        };
        if target.len() < 1 << 20 {
            target.push_str(text);
        }
    }));
    session.set_wall_clock(Some(Box::new(move || clock)));
    let counter = Arc::clone(counter);
    session.set_entropy(Some(Box::new(move |buf: &mut [u8]| {
        let mut c = counter.lock().unwrap_or_else(PoisonError::into_inner);
        for b in buf {
            *c = c.wrapping_add(37);
            *b = *c;
        }
    })));
    session.set_zone_source(Some(Box::new(|key: &str| {
        if key.starts_with('/') || key.split('/').any(|p| p == ".." || p.is_empty()) {
            return None;
        }
        std::fs::read(Path::new("/usr/share/zoneinfo").join(key))
            .ok()
            .filter(|b| b.starts_with(b"TZif"))
    })));
}

fn codemode(args: &[String]) -> Res<()> {
    let swap = args.iter().any(|a| a == "--swap");
    let stdin = std::io::stdin();
    let mut lines = stdin.lock().lines();
    let mut out = std::io::stdout().lock();
    let init: Value = lines
        .next()
        .and_then(Result::ok)
        .and_then(|l| serde_json::from_str(&l).ok())
        .ok_or("expected the init line")?;
    let name = arg(args, "--name").unwrap_or_else(|| "codemode".into());
    let fuel = init["fuel"].as_u64().unwrap_or(1_000_000_000);
    let engine = Engine::new(EngineConfig::new().with_limits(Limits::new().with_fuel(Some(fuel))));
    let mut c = ctx(args, engine, name)?;
    let output: Output = Arc::new(Mutex::new((String::new(), String::new())));
    let clock = init["clock"].as_f64().unwrap_or(0.0);
    let counter = Arc::new(Mutex::new(0u8));
    let tools: Vec<&str> = init["tools"]
        .as_array()
        .map(|t| t.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    let error = |t: &str, m: &str, tb: &str| json!({"type": t, "message": m, "traceback": tb});
    let mut session = Session::new(&c.engine).map_err(|e| e.traceback)?;
    attach_codemode(&mut session, &output, clock, &counter);
    session.set_last_expr_value(false);
    session
        .define(&HostModule::untyped("_ptc", &tools))
        .map_err(|e| e.traceback)?;
    let mut bootstrap = String::from("import _ptc\n");
    for tool in &tools {
        let _ = write!(
            bootstrap,
            "async def {tool}(arguments):\n    return await _ptc.{tool}(arguments)\n"
        );
    }
    let boot = c
        .engine
        .compile(&bootstrap, "<tools>")
        .map_err(|e| e.rendered)?;
    if let Progress::Error(e) = session.start(&boot) {
        return Err(e.traceback);
    }
    let source = init["source"].as_str().unwrap_or_default();
    let (mut calls, mut batch, mut max_waiting) = (0u64, 0u64, 0usize);
    let mut answers: BTreeMap<u64, Result<HostValue, HostError>> = BTreeMap::new();
    let err = match c.engine.compile(source, "solution.py") {
        Err(e) => error(&e.type_name, &e.message, &e.rendered),
        Ok(code) => {
            let mut progress = session.start(&code);
            loop {
                let stop = matches!(
                    progress,
                    Progress::HostCall { .. } | Progress::Waiting { .. }
                );
                if stop && let Some(s) = c.park(&mut session, swap)? {
                    session = s;
                    attach_codemode(&mut session, &output, clock, &counter);
                }
                let resume = match progress {
                    Progress::HostCall {
                        call,
                        name,
                        args,
                        kwargs,
                        ..
                    } => {
                        calls += 1;
                        let name = name.strip_prefix("_ptc.").unwrap_or(&name);
                        let request = json!({"call": call, "name": name,
                            "args": args.iter().map(encode_value).collect::<Vec<_>>(),
                            "kwargs": pairs(&kwargs), "batch": batch});
                        let _ = writeln!(out, "{request}");
                        let _ = out.flush();
                        let reply: Value = lines
                            .next()
                            .and_then(Result::ok)
                            .and_then(|l| serde_json::from_str(&l).ok())
                            .ok_or("the harness stopped answering")?;
                        answers.insert(call, answer(&reply));
                        Resume::Future
                    }
                    Progress::Waiting { calls: waiting } => {
                        max_waiting = max_waiting.max(waiting.len());
                        batch += 1;
                        let b: Vec<_> = waiting
                            .iter()
                            .filter_map(|c| answers.remove(c).map(|r| (*c, r)))
                            .collect();
                        if b.is_empty() {
                            break error("RunnerError", "waiting on unanswered calls", "");
                        }
                        Resume::Batch(b)
                    }
                    Progress::Parked { reason } => {
                        break error("FuelExhausted", &format!("parked: {reason:?}"), "");
                    }
                    Progress::Complete(_) => break Value::Null,
                    Progress::Error(e) => break error(&e.type_name, &e.message, &e.traceback),
                    other => break error("RunnerError", &format!("{other:?}"), ""),
                };
                progress = match session.resume(resume) {
                    Ok(p) => p,
                    Err(e) => break error("RunnerError", &e.to_string(), ""),
                };
            }
        }
    };
    let streams = output.lock().unwrap_or_else(PoisonError::into_inner);
    let done = json!({"done": {"stdout": streams.0, "stderr": streams.1, "error": err,
        "calls": calls, "max_waiting": max_waiting, "parks": c.parks}});
    let _ = writeln!(out, "{done}");
    let _ = out.flush();
    Ok(())
}

fn pairs(items: &[(String, HostValue)]) -> Vec<Value> {
    items
        .iter()
        .map(|(k, v)| json!([k, encode_value(v)]))
        .collect()
}

fn encode_value(value: &HostValue) -> Value {
    match value {
        HostValue::None => Value::Null,
        HostValue::Bool(b) => json!(b),
        HostValue::Int(i) => json!(i),
        HostValue::BigInt(digits) => json!({"$i": digits}),
        HostValue::Float(f) => json!({"$f": format!("{f:?}")}),
        HostValue::Str(s) => json!(s),
        HostValue::Bytes(b) => json!({"$b": b.iter().fold(String::new(), |mut hex, x| {
            let _ = write!(hex, "{x:02x}");
            hex
        })}),
        HostValue::List(items) => Value::Array(items.iter().map(encode_value).collect()),
        HostValue::Tuple(items) => {
            json!({"$t": items.iter().map(encode_value).collect::<Vec<_>>()})
        }
        HostValue::Dict(items) => json!({"$d": pairs(items)}),
        HostValue::Record { name, fields } => json!({"$r": name, "fields": pairs(fields)}),
        HostValue::Object { class, fields } => json!({"$r": class, "fields": pairs(fields)}),
        HostValue::Unconverted(piece) => json!({"$u": format!("{piece:?}")}),
    }
}

fn decode(value: &Value) -> HostValue {
    match value {
        Value::Null => HostValue::None,
        Value::Bool(b) => HostValue::Bool(*b),
        Value::Number(n) => n.as_i64().map_or_else(
            || HostValue::Float(n.as_f64().unwrap_or(f64::NAN)),
            HostValue::Int,
        ),
        Value::String(s) => HostValue::Str(s.clone()),
        Value::Array(items) => HostValue::List(items.iter().map(decode).collect()),
        Value::Object(map) => {
            if let Some(Value::String(f)) = map.get("$f") {
                HostValue::Float(f.parse().unwrap_or(f64::NAN))
            } else if let Some(Value::String(digits)) = map.get("$i") {
                HostValue::BigInt(digits.clone())
            } else if let Some(Value::Array(items)) = map.get("$t") {
                HostValue::Tuple(items.iter().map(decode).collect())
            } else if let Some(Value::String(hex)) = map.get("$b") {
                HostValue::Bytes(
                    (0..hex.len() / 2)
                        .filter_map(|i| u8::from_str_radix(hex.get(2 * i..2 * i + 2)?, 16).ok())
                        .collect(),
                )
            } else if let Some(Value::Array(items)) = map.get("$d") {
                HostValue::Dict(
                    items
                        .iter()
                        .filter_map(|pair| {
                            Some((pair.get(0)?.as_str()?.to_owned(), decode(pair.get(1)?)))
                        })
                        .collect(),
                )
            } else {
                HostValue::None
            }
        }
    }
}

fn answer(reply: &Value) -> Result<HostValue, HostError> {
    if let Some(value) = reply.get("ok") {
        return Ok(decode(value));
    }
    let kind = reply["err"][0].as_str().unwrap_or("RuntimeError");
    Err(HostError {
        kind: HostErrorKind::from_name(kind).unwrap_or(HostErrorKind::RuntimeError),
        message: reply["err"][1].as_str().unwrap_or_default().to_owned(),
        data: None,
        class: None,
    })
}
