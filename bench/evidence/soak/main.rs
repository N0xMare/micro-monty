//! `soak`: many sessions on one shared facade `Engine` (so one decoded-module
//! cache, 1c03cff) across threads, each run checked against the same work
//! on a fresh uncached `Engine` (`Engine::with_module_cache(_, 0)`), with
//! RSS and the cache's bytes logged over time. bench/evidence/soak/README.md.
//!
//! ```text
//! soak WORKLOAD.jsonl OUT_DIR [--minutes M] [--threads T] [--inflight K]
//!      [--cache-cap BYTES] [--limits KIB,..] [--max-fuel F] [--seed S]
//! ```
//!
//! Workload: codemode solutions (tool replies replayed in order), own cases
//! (as `mm run`), and one import per bundled module plus all of them in one
//! cell. Each job picks an item and a memory limit; a limit below the
//! default comes from restoring, on the shared engine, an empty session
//! saved by an engine with that limit (a restored session keeps the smaller
//! limit). It runs in fuel slices, parks, is saved and restored on the
//! shared engine at random parks, host calls and waits, and moves between
//! threads through a shared queue. Its transcript (stdout, stderr, the end,
//! every host call) must equal the reference: the item at the same limit on
//! a new uncached engine, unparked. Differences go to OUT_DIR/diff-*.json;
//! OUT_DIR/log.csv gets one line per interval.
#![forbid(unsafe_code)]

use micro_monty::{
    Engine, EngineConfig, HostError, HostErrorKind, HostModule, HostValue, Limits, Progress,
    Resume, Session, Stream,
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    fmt::Write as _,
    hash::{DefaultHasher, Hash, Hasher},
    io::Write as _,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, PoisonError, RwLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

type Res<T> = Result<T, String>;

/// 2026-03-15T12:00:00Z, the codemode suite's clock, for every session.
const CLOCK: f64 = 1_773_576_000.0;
/// Fuel for unparked runs: never reached.
const ENOUGH_FUEL: u64 = 1 << 60;

#[derive(Debug)]
enum Kind {
    Codemode { tools: Vec<String>, replies: Vec<Value> },
    Own { file: String },
    Import,
}

#[derive(Debug)]
struct Item {
    id: String,
    kind: Kind,
    source: String,
}

impl Item {
    fn cells(&self) -> Vec<(String, String)> {
        match &self.kind {
            Kind::Codemode { tools, .. } => {
                let mut bootstrap = String::from("import _ptc\n");
                for tool in tools {
                    let _ = write!(
                        bootstrap,
                        "async def {tool}(arguments):\n    return await _ptc.{tool}(arguments)\n"
                    );
                }
                vec![
                    (bootstrap, "<tools>".to_owned()),
                    (self.source.clone(), "solution.py".to_owned()),
                ]
            }
            Kind::Own { file } => vec![(self.source.clone(), file.clone())],
            Kind::Import => vec![(self.source.clone(), "<imports>".to_owned())],
        }
    }
}

/// What a run produced.
#[derive(Debug, Default, Clone, PartialEq)]
struct Transcript {
    stdout: String,
    stderr: String,
    end: String,
    calls: Vec<String>,
}

impl Transcript {
    fn hash(&self) -> u64 {
        let mut h = DefaultHasher::new();
        self.hash_into(&mut h);
        h.finish()
    }
    fn hash_into(&self, h: &mut DefaultHasher) {
        self.stdout.hash(h);
        self.stderr.hash(h);
        self.end.hash(h);
        self.calls.hash(h);
    }
    fn to_json(&self) -> Value {
        json!({"stdout": self.stdout, "stderr": self.stderr, "end": self.end, "calls": self.calls})
    }
}

/// Small, fast, good enough: xorshift64*.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: u64) -> u64 {
        if n == 0 { 0 } else { self.next() % n }
    }
    fn chance(&mut self, percent: u64) -> bool {
        self.below(100) < percent
    }
}

fn config(memory: u64) -> EngineConfig {
    EngineConfig::new().with_limits(Limits::new().with_memory(memory))
}

type Output = Arc<Mutex<(String, String)>>;

/// Host-side settings, installed on a new session and again after every
/// restore (a restored session has none of them).
fn attach(s: &mut Session, out: &Output, entropy: &Arc<AtomicU64>) {
    let sink = Arc::clone(out);
    s.set_output_streams(Box::new(move |stream, text| {
        let mut o = sink.lock().unwrap_or_else(PoisonError::into_inner);
        if o.0.len() + o.1.len() < 4 << 20 {
            if stream == Stream::Stderr {
                o.1.push_str(text);
            } else {
                o.0.push_str(text);
            }
        }
    }));
    s.set_wall_clock(Some(Box::new(|| CLOCK)));
    // The counter lives in the job, so a restored session continues it.
    let counter = Arc::clone(entropy);
    s.set_entropy(Some(Box::new(move |buf: &mut [u8]| {
        for b in buf {
            let n = counter.fetch_add(1, Ordering::Relaxed);
            *b = (n.wrapping_mul(37) & 0xff) as u8;
        }
    })));
    s.set_zone_source(Some(Box::new(|key: &str| {
        if key.starts_with('/') || key.split('/').any(|p| p == ".." || p.is_empty()) {
            return None;
        }
        std::fs::read(Path::new("/usr/share/zoneinfo").join(key))
            .ok()
            .filter(|b| b.starts_with(b"TZif"))
    })));
}

/// Settings made once on a new session (kept by snapshots).
fn prepare(s: &mut Session, item: &Item) -> Res<()> {
    s.set_last_expr_value(false);
    match &item.kind {
        Kind::Codemode { tools, .. } => {
            let names: Vec<&str> = tools.iter().map(String::as_str).collect();
            s.define(&HostModule::untyped("_ptc", &names))
                .map_err(|e| e.to_string())?;
        }
        Kind::Own { file } => {
            s.set_module_dunders(Some("__main__"), Some(file))
                .map_err(|e| e.traceback)?;
        }
        Kind::Import => {}
    }
    Ok(())
}

/// How a job continues after each stop.
#[derive(Clone, Copy)]
struct Plan {
    /// Fuel per slice (`None`: unparked).
    slice: Option<u64>,
    /// Percent chance of a save and restore at each park, host call or wait.
    snapshot: u64,
    /// Percent chance of handing the job to the queue at each park.
    migrate: u64,
}

struct Job {
    item: usize,
    level: usize,
    /// The engine the job restores on: the shared one, or an uncached one
    /// for references.
    engine: Engine,
    session: Session,
    out: Output,
    entropy: Arc<AtomicU64>,
    cells: Vec<(String, String)>,
    cell: usize,
    progress: Option<Progress>,
    /// How to continue a job taken from the queue.
    pending: Option<Resume>,
    answers: BTreeMap<u64, Result<HostValue, HostError>>,
    next_reply: usize,
    calls: Vec<String>,
    plan: Plan,
    /// Every save-and-restore decision in order; a reference replays them.
    decisions: Vec<bool>,
    replay: Option<VecDeque<bool>>,
    /// What happened, for a difference report.
    trail: Vec<String>,
    end: Option<String>,
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

fn reply(item: &Item, index: usize, name: &str) -> Result<HostValue, HostError> {
    let err = |kind: &str, message: &str| HostError {
        kind: HostErrorKind::from_name(kind).unwrap_or(HostErrorKind::RuntimeError),
        message: message.to_owned(),
        data: None,
        class: None,
    };
    let Kind::Codemode { replies, .. } = &item.kind else {
        return Err(err("RuntimeError", "no host"));
    };
    let Some(r) = replies.get(index) else {
        return Err(err("RuntimeError", "soak: no recorded reply"));
    };
    let name = name.strip_prefix("_ptc.").unwrap_or(name);
    if r["name"].as_str() != Some(name) {
        return Err(err("RuntimeError", "soak: replay drift"));
    }
    if let Some(v) = r.get("ok") {
        return Ok(decode(v));
    }
    Err(err(
        r["err"][0].as_str().unwrap_or("RuntimeError"),
        r["err"][1].as_str().unwrap_or_default(),
    ))
}

#[derive(Default)]
struct Stats {
    jobs: AtomicU64,
    same: AtomicU64,
    differ: AtomicU64,
    parks: AtomicU64,
    snapshots: AtomicU64,
    snapshot_bytes: AtomicU64,
    snapshot_refused: AtomicU64,
    restore_failed: AtomicU64,
    migrations: AtomicU64,
    memory_errors: AtomicU64,
    /// Differs from the unparked run only by where a MemoryError hit.
    memory_edge: AtomicU64,
    refs: AtomicU64,
    ref_unstable: AtomicU64,
    skipped_items: AtomicU64,
}

struct Shared {
    engine: Engine,
    items: Vec<Item>,
    /// Memory limits, bytes; level 0 is the default.
    levels: Vec<u64>,
    /// Per level above 0: an empty session saved by an engine with that limit.
    images: Vec<Option<Vec<u8>>>,
    /// (item, level) -> reference transcript hash and fuel.
    refs: RwLock<HashMap<(usize, usize), (u64, u64)>>,
    skipped: RwLock<HashSet<usize>>,
    queue: Mutex<VecDeque<Job>>,
    stats: Stats,
    stop: AtomicBool,
    max_fuel: u64,
    out_dir: PathBuf,
    diff_count: AtomicU64,
}

/// How a job's session is made.
#[derive(Clone, Copy, PartialEq)]
enum Make {
    /// On the shared engine: `Session::new` at the default limit, else a
    /// restore of the level's empty-session image.
    Shared,
    /// The same on a new uncached engine with the default limit.
    SameUncached,
    /// `Session::new` on a new uncached engine with the level's limit: the
    /// plain way.
    Plain,
}

fn start_job(sh: &Shared, item: usize, level: usize, make: Make, plan: Plan) -> Res<Job> {
    let engine = match make {
        Make::Shared => sh.engine.clone(),
        Make::SameUncached => Engine::with_module_cache(config(sh.levels[0]), 0),
        Make::Plain => Engine::with_module_cache(config(sh.levels[level]), 0),
    };
    let session = match (&sh.images[level], make) {
        (Some(image), Make::Shared | Make::SameUncached) => {
            engine.restore(image).map_err(|e| e.to_string())?
        }
        _ => Session::new(&engine).map_err(|e| e.traceback)?,
    };
    job_with(sh, &engine, item, level, session, plan)
}

/// A save-and-restore decision: drawn, or replayed by a reference.
fn decide(job: &mut Job, rng: &mut Rng) -> bool {
    let d = match job.replay.as_mut() {
        Some(r) => r.pop_front().unwrap_or(false),
        None => rng.chance(job.plan.snapshot),
    };
    job.decisions.push(d);
    d
}

enum Step {
    Done,
    Yield,
}

fn slice_fuel(job: &Job) -> u64 {
    job.plan.slice.unwrap_or(ENOUGH_FUEL)
}

/// Saves the session and continues it from the bytes on the shared engine.
fn save_restore(sh: &Shared, job: &mut Job, at: &str) {
    match job.session.snapshot() {
        Ok(bytes) => {
            sh.stats.snapshots.fetch_add(1, Ordering::Relaxed);
            sh.stats
                .snapshot_bytes
                .fetch_add(bytes.len() as u64, Ordering::Relaxed);
            match job.engine.restore(&bytes) {
                Ok(mut s) => {
                    attach(&mut s, &job.out, &job.entropy);
                    job.session = s;
                    job.trail.push(format!("snap@{at}:{}", bytes.len()));
                }
                Err(e) => {
                    let n = sh.stats.restore_failed.fetch_add(1, Ordering::Relaxed);
                    job.trail.push(format!("restore-failed@{at}:{e}"));
                    // The image, for `--restore FILE`.
                    if n < 50 {
                        let base = sh.out_dir.join(format!("restore-fail-{n:03}"));
                        let _ = std::fs::write(base.with_extension("snap"), &bytes);
                        let doc = json!({
                            "item": sh.items[job.item].id,
                            "level": job.level,
                            "memory": sh.levels[job.level],
                            "error": e.to_string(),
                            "cached_engine": job.engine.module_cache_bytes() > 0,
                            "trail": job.trail,
                        });
                        let _ = std::fs::write(base.with_extension("json"), doc.to_string());
                        eprintln!("RESTORE FAILED {}: {e} -> {}", sh.items[job.item].id, base.display());
                    }
                }
            }
        }
        Err(e) => {
            sh.stats.snapshot_refused.fetch_add(1, Ordering::Relaxed);
            job.trail.push(format!("snap-refused@{at}:{e}"));
        }
    }
}

/// Runs a job until it ends, or until it hands itself to the queue.
fn drive(sh: &Shared, job: &mut Job, rng: &mut Rng, chaos: bool) -> Step {
    if let Some(resume) = job.pending.take() {
        match job.session.resume(resume) {
            Ok(p) => job.progress = Some(p),
            Err(e) => {
                job.end = Some(format!("resume refused: {e}"));
                return Step::Done;
            }
        }
    }
    loop {
        let progress = match job.progress.take() {
            Some(p) => p,
            None => {
                let (src, name) = job.cells[job.cell].clone();
                let code = match sh.engine.compile(&src, &name) {
                    Ok(c) => c,
                    Err(e) => {
                        job.end = Some(format!("compile: {}", e.rendered));
                        return Step::Done;
                    }
                };
                job.session.set_fuel(Some(slice_fuel(job)));
                job.session.start(&code)
            }
        };
        let resume = match progress {
            Progress::HostCall {
                call,
                name,
                args,
                kwargs,
                ..
            } => {
                job.calls.push(format!("{name}{args:?}{kwargs:?}"));
                let r = reply(&sh.items[job.item], job.next_reply, &name);
                job.next_reply += 1;
                job.answers.insert(call, r);
                if chaos && decide(job, rng) {
                    save_restore(sh, job, &format!("call{call}"));
                }
                Resume::Future
            }
            Progress::Waiting { calls } => {
                let batch: Vec<_> = calls
                    .iter()
                    .filter_map(|c| job.answers.remove(c).map(|r| (*c, r)))
                    .collect();
                if batch.is_empty() {
                    job.end = Some(format!("waiting on unanswered calls {calls:?}"));
                    return Step::Done;
                }
                if chaos && decide(job, rng) {
                    save_restore(sh, job, "wait");
                }
                Resume::Batch(batch)
            }
            Progress::Parked { reason } => {
                if !chaos {
                    job.end = Some(format!("parked unexpectedly: {reason:?}"));
                    return Step::Done;
                }
                sh.stats.parks.fetch_add(1, Ordering::Relaxed);
                if decide(job, rng) {
                    save_restore(sh, job, "park");
                }
                job.session.set_fuel(Some(slice_fuel(job)));
                if job.replay.is_none() && rng.chance(job.plan.migrate) {
                    // Parked, to the queue; whichever thread takes it resumes it.
                    job.pending = Some(Resume::Continue);
                    sh.stats.migrations.fetch_add(1, Ordering::Relaxed);
                    return Step::Yield;
                }
                Resume::Continue
            }
            Progress::Complete(_) => {
                job.cell += 1;
                if job.cell == job.cells.len() {
                    job.end = Some("complete".to_owned());
                    return Step::Done;
                }
                continue;
            }
            Progress::Error(e) => {
                if e.type_name == "MemoryError" {
                    sh.stats.memory_errors.fetch_add(1, Ordering::Relaxed);
                }
                job.end = Some(format!("error: {}", e.traceback));
                return Step::Done;
            }
            other => {
                job.end = Some(format!("unexpected: {other:?}"));
                return Step::Done;
            }
        };
        match job.session.resume(resume) {
            Ok(p) => job.progress = Some(p),
            Err(e) => {
                job.end = Some(format!("resume refused: {e}"));
                return Step::Done;
            }
        }
    }
}

fn transcript(job: &Job) -> Transcript {
    let o = job.out.lock().unwrap_or_else(PoisonError::into_inner);
    Transcript {
        stdout: o.0.clone(),
        stderr: o.1.clone(),
        end: job.end.clone().unwrap_or_default(),
        calls: job.calls.clone(),
    }
}

/// The item at the level on a new uncached engine, unparked: its transcript
/// and the fuel it used.
fn reference(sh: &Shared, item: usize, level: usize) -> Res<(Transcript, u64)> {
    let plan = Plan {
        slice: None,
        snapshot: 0,
        migrate: 0,
    };
    let mut job = start_job(sh, item, level, Make::Plain, plan)?;
    let mut rng = Rng(1);
    drive(sh, &mut job, &mut rng, false);
    let used = ENOUGH_FUEL - job.session.fuel().unwrap_or(ENOUGH_FUEL);
    Ok((transcript(&job), used))
}

/// The reference's hash and fuel, computed on first use.
fn reference_of(sh: &Shared, item: usize, level: usize) -> Res<(u64, u64)> {
    if let Some(r) = sh
        .refs
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .get(&(item, level))
    {
        return Ok(*r);
    }
    let (t, fuel) = reference(sh, item, level)?;
    let r = (t.hash(), fuel);
    sh.stats.refs.fetch_add(1, Ordering::Relaxed);
    sh.refs
        .write()
        .unwrap_or_else(PoisonError::into_inner)
        .insert((item, level), r);
    Ok(r)
}

fn new_job(sh: &Shared, rng: &mut Rng) -> Option<Job> {
    loop {
        if sh.stop.load(Ordering::Relaxed) {
            return None;
        }
        let item = rng.below(sh.items.len() as u64) as usize;
        if sh
            .skipped
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .contains(&item)
        {
            continue;
        }
        // Half the jobs at the default limit.
        let level = if rng.chance(50) {
            0
        } else {
            rng.below(sh.levels.len() as u64) as usize
        };
        if level > 0 && sh.images[level].is_none() {
            continue;
        }
        let fuel = match reference_of(sh, item, level) {
            Ok((_, fuel)) => fuel,
            Err(e) => {
                eprintln!("reference of {} failed: {e}", sh.items[item].id);
                sh.skipped
                    .write()
                    .unwrap_or_else(PoisonError::into_inner)
                    .insert(item);
                continue;
            }
        };
        if fuel > sh.max_fuel {
            if sh
                .skipped
                .write()
                .unwrap_or_else(PoisonError::into_inner)
                .insert(item)
            {
                sh.stats.skipped_items.fetch_add(1, Ordering::Relaxed);
            }
            continue;
        }
        // Parks per run: none, a few, many or about a thousand.
        let parks = [0, 5, 50, 300, 1000][rng.below(5) as usize];
        let slice = (parks > 0).then(|| (fuel / parks).max(1) + rng.below(3));
        let plan = Plan {
            slice,
            snapshot: [0, 10, 30, 100][rng.below(4) as usize],
            migrate: [0, 30, 70][rng.below(3) as usize],
        };
        match start_job(sh, item, level, Make::Shared, plan) {
            Ok(mut job) => {
                job.trail.push(format!(
                    "slice={slice:?} snapshot={} migrate={}",
                    plan.snapshot, plan.migrate
                ));
                return Some(job);
            }
            Err(e) => {
                // A level the shared engine cannot restore into.
                eprintln!("start failed for {} at level {level}: {e}", sh.items[item].id);
                continue;
            }
        }
    }
}

/// The job's schedule (fuel slices, save-and-restore decisions) replayed
/// on a new uncached engine, its session made the same way.
fn same_schedule(sh: &Shared, job: &Job) -> Res<Transcript> {
    let plan = Plan {
        migrate: 0,
        ..job.plan
    };
    let mut r = start_job(sh, job.item, job.level, Make::SameUncached, plan)?;
    r.replay = Some(job.decisions.iter().copied().collect());
    drive(sh, &mut r, &mut Rng(1), true);
    Ok(transcript(&r))
}

fn mentions_memory_error(t: &Transcript) -> bool {
    [&t.stdout, &t.stderr, &t.end]
        .iter()
        .any(|s| s.contains("MemoryError"))
}

fn finish(sh: &Shared, job: &Job, rng: &mut Rng) {
    sh.stats.jobs.fetch_add(1, Ordering::Relaxed);
    let got = transcript(job);
    // 1. The cache and the threads change nothing: the same schedule on
    //    an uncached engine gives the same transcript.
    match same_schedule(sh, job) {
        Ok(same) if same == got => {}
        Ok(same) => {
            sh.stats.differ.fetch_add(1, Ordering::Relaxed);
            report(sh, job, &got, Some(&same), "differs from its schedule on an uncached engine");
            return;
        }
        Err(e) => {
            sh.stats.differ.fetch_add(1, Ordering::Relaxed);
            eprintln!("same-schedule run of {} failed: {e}", sh.items[job.item].id);
            return;
        }
    }
    // 2. Parks and restores change nothing: an unparked run on a new
    //    uncached engine gives the same transcript. Below the default
    //    limit, where a collection happens (every snapshot collects) can
    //    decide a MemoryError at the edge, so those are only counted.
    let want = reference_of(sh, job.item, job.level).map(|r| r.0);
    if rng.chance(2)
        && let Ok((again, _)) = reference(sh, job.item, job.level)
        && Ok(again.hash()) != want
    {
        sh.stats.ref_unstable.fetch_add(1, Ordering::Relaxed);
        report(sh, job, &got, Some(&again), "unparked run not deterministic");
    }
    if want == Ok(got.hash()) {
        sh.stats.same.fetch_add(1, Ordering::Relaxed);
        return;
    }
    let plain = reference(sh, job.item, job.level).ok().map(|r| r.0);
    if job.level > 0
        && (mentions_memory_error(&got) || plain.as_ref().is_some_and(mentions_memory_error))
    {
        sh.stats.memory_edge.fetch_add(1, Ordering::Relaxed);
        return;
    }
    sh.stats.differ.fetch_add(1, Ordering::Relaxed);
    report(sh, job, &got, plain.as_ref(), "differs from an unparked run");
}

fn report(sh: &Shared, job: &Job, got: &Transcript, want: Option<&Transcript>, what: &str) {
    let n = sh.diff_count.fetch_add(1, Ordering::Relaxed);
    if n >= 200 {
        return;
    }
    let item = &sh.items[job.item];
    let doc = json!({
        "what": what,
        "item": item.id,
        "level": job.level,
        "memory": sh.levels[job.level],
        "trail": job.trail,
        "slice": job.plan.slice,
        "decisions": job.decisions,
        "got": got.to_json(),
        "want": want.map(Transcript::to_json),
    });
    let path = sh.out_dir.join(format!("diff-{n:03}.json"));
    let _ = std::fs::write(&path, serde_json::to_string_pretty(&doc).unwrap_or_default());
    eprintln!("DIFF {what}: {} at level {} -> {}", item.id, job.level, path.display());
}

fn worker(sh: &Shared, seed: u64, inflight: usize) {
    let mut rng = Rng(seed | 1);
    while !sh.stop.load(Ordering::Relaxed) {
        let queued = {
            let mut q = sh.queue.lock().unwrap_or_else(PoisonError::into_inner);
            if q.len() >= inflight || (!q.is_empty() && rng.chance(50)) {
                q.pop_front()
            } else {
                None
            }
        };
        let mut job = match queued {
            Some(j) => j,
            None => match new_job(sh, &mut rng) {
                Some(j) => j,
                None => break,
            },
        };
        match drive(sh, &mut job, &mut rng, true) {
            Step::Done => finish(sh, &job, &mut rng),
            Step::Yield => sh
                .queue
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push_back(job),
        }
    }
}

/// One item at one level, single-threaded, in each way a session can be
/// made and run: which of them differ from the reference.
fn probe(sh: &Shared, id: &str, level: usize) -> Res<()> {
    let item = sh
        .items
        .iter()
        .position(|i| i.id == id)
        .ok_or_else(|| format!("no item {id}"))?;
    let memory = sh.levels[level];
    let image = |e: &Engine| -> Res<Session> {
        let maker = Engine::with_module_cache(config(memory), 0);
        let bytes = Session::new(&maker)
            .map_err(|e| e.traceback)?
            .snapshot()
            .map_err(|e| e.to_string())?;
        e.restore(&bytes).map_err(|e| e.to_string())
    };
    let (want, fuel) = reference(sh, item, level)?;
    println!("reference: fuel {fuel}, end {:?}", want.end.lines().last());
    let warm = Engine::with_module_cache(config(memory), micro_monty::DEFAULT_MODULE_CACHE);
    // Warm the cache: the item once, unparked.
    {
        let s = Session::new(&warm).map_err(|e| e.traceback)?;
        let plan = Plan { slice: None, snapshot: 0, migrate: 0 };
        let mut job = job_with(sh, &warm, item, level, s, plan)?;
        drive(sh, &mut job, &mut Rng(1), false);
    }
    let cold = || Engine::with_module_cache(config(memory), 0);
    for (name, slice, snapshot) in [
        ("unparked", None, 0),
        ("snapshots at host stops", None, 100),
        ("parked 7", Some(7), 0),
        ("parked 1000", Some(1000), 0),
        ("parked 7 + snapshots", Some(7), 100),
    ] {
        for way in ["uncached new", "uncached image", "cached new", "cached image"] {
            let engine = if way.starts_with("cached") { warm.clone() } else { cold() };
            let s = if way.ends_with("new") {
                Session::new(&engine).map_err(|e| e.traceback)?
            } else {
                image(&engine)?
            };
            let plan = Plan { slice, snapshot, migrate: 0 };
            let mut job = job_with(sh, &warm, item, level, s, plan)?;
            let chaos = slice.is_some() || snapshot > 0;
            drive(sh, &mut job, &mut Rng(7), chaos);
            let got = transcript(&job);
            println!(
                "{name:22} {way:15} {} calls {:3} end {:?}",
                if got == want { "same  " } else { "DIFFER" },
                got.calls.len(),
                got.end.lines().last()
            );
        }
    }
    Ok(())
}

/// One item at one level under random schedules, one thread: a session
/// on a cold cached engine, on a warm one and on an uncached one, each with
/// the same schedule, must agree.
fn probe_schedules(sh: &Shared, id: &str, level: usize, runs: u64) -> Res<()> {
    let item = sh
        .items
        .iter()
        .position(|i| i.id == id)
        .ok_or_else(|| format!("no item {id}"))?;
    let (_, fuel) = reference(sh, item, level)?;
    let warm = Engine::with_module_cache(config(sh.levels[0]), micro_monty::DEFAULT_MODULE_CACHE);
    let mut rng = Rng(0x1234_5678);
    let mut differ = 0;
    for run in 0..runs {
        let parks = [0, 5, 50, 300][rng.below(4) as usize];
        let plan = Plan {
            slice: (parks > 0).then(|| (fuel / parks).max(1) + rng.below(3)),
            snapshot: [10, 30, 100][rng.below(3) as usize],
            migrate: 0,
        };
        let cold = Engine::with_module_cache(config(sh.levels[0]), micro_monty::DEFAULT_MODULE_CACHE);
        let mut first: Option<(Transcript, Vec<bool>)> = None;
        for (name, engine) in [("cold", cold), ("warm", warm.clone()), ("uncached", Engine::with_module_cache(config(sh.levels[0]), 0))] {
            let session = match &sh.images[level] {
                Some(image) => engine.restore(image).map_err(|e| e.to_string())?,
                None => Session::new(&engine).map_err(|e| e.traceback)?,
            };
            let mut job = job_with(sh, &engine, item, level, session, plan)?;
            if let Some((_, d)) = &first {
                job.replay = Some(d.iter().copied().collect());
            }
            drive(sh, &mut job, &mut rng, true);
            let t = transcript(&job);
            match &first {
                None => first = Some((t, job.decisions.clone())),
                Some((t0, d)) if *t0 != t => {
                    differ += 1;
                    println!(
                        "run {run}: {name} differs from cold; slice {:?} decisions {:?}\n  cold end {:?}\n  {name} end {:?}",
                        plan.slice,
                        d,
                        t0.end.lines().last(),
                        t.end.lines().last()
                    );
                }
                Some(_) => {}
            }
        }
    }
    println!("{differ} differences in {runs} runs");
    Ok(())
}

/// One schedule replayed in a row of sessions, one thread, on one cached
/// engine and on new uncached ones: which runs end differently.
fn probe_replay(sh: &Shared, id: &str, level: usize, slice: Option<u64>, decisions: &[bool]) -> Res<()> {
    let item = sh
        .items
        .iter()
        .position(|i| i.id == id)
        .ok_or_else(|| format!("no item {id}"))?;
    let cached = Engine::with_module_cache(config(sh.levels[0]), micro_monty::DEFAULT_MODULE_CACHE);
    let plan = Plan { slice, snapshot: 0, migrate: 0 };
    for run in 0..4 {
        for (name, engine) in [("cached", cached.clone()), ("uncached", Engine::with_module_cache(config(sh.levels[0]), 0))] {
            let session = match &sh.images[level] {
                Some(image) => engine.restore(image).map_err(|e| e.to_string())?,
                None => Session::new(&engine).map_err(|e| e.traceback)?,
            };
            let mut job = job_with(sh, &engine, item, level, session, plan)?;
            job.replay = Some(decisions.iter().copied().collect());
            drive(sh, &mut job, &mut Rng(1), true);
            let t = transcript(&job);
            println!("run {run} {name:8} cache {:8} calls {} end {:?} trail {:?}", cached.module_cache_bytes(), t.calls.len(), t.end.lines().last(), job.trail);
        }
    }
    Ok(())
}

fn job_with(
    sh: &Shared,
    engine: &Engine,
    item: usize,
    level: usize,
    mut session: Session,
    plan: Plan,
) -> Res<Job> {
    let out: Output = Arc::default();
    let entropy = Arc::new(AtomicU64::new(0));
    attach(&mut session, &out, &entropy);
    prepare(&mut session, &sh.items[item])?;
    Ok(Job {
        item,
        level,
        engine: engine.clone(),
        session,
        out,
        entropy,
        cells: sh.items[item].cells(),
        cell: 0,
        progress: None,
        pending: None,
        answers: BTreeMap::new(),
        next_reply: 0,
        calls: Vec::new(),
        plan,
        decisions: Vec::new(),
        replay: None,
        trail: Vec::new(),
        end: None,
    })
}

fn proc_status(field: &str) -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with(field))
                .and_then(|l| l.split_whitespace().nth(1))
                .and_then(|n| n.parse().ok())
        })
        .unwrap_or(0)
}

/// Every public bundled module, from the engine's environment summary.
fn bundled_modules(engine: &Engine) -> Vec<String> {
    let env = engine.environment();
    env.lines()
        .find_map(|l| l.strip_prefix("Modules: "))
        .map(|l| {
            l.trim_end_matches('.')
                .split(", ")
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn load_items(path: &Path, engine: &Engine) -> Res<Vec<Item>> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut items = Vec::new();
    for line in text.lines() {
        let v: Value = serde_json::from_str(line).map_err(|e| e.to_string())?;
        let id = v["id"].as_str().unwrap_or_default().to_owned();
        let source = v["source"].as_str().unwrap_or_default().to_owned();
        let kind = match v["kind"].as_str() {
            Some("codemode") => Kind::Codemode {
                tools: v["tools"]
                    .as_array()
                    .map(|t| {
                        t.iter()
                            .filter_map(Value::as_str)
                            .map(str::to_owned)
                            .collect()
                    })
                    .unwrap_or_default(),
                replies: v["replies"].as_array().cloned().unwrap_or_default(),
            },
            Some("own") => Kind::Own {
                file: v["file"].as_str().unwrap_or_default().to_owned(),
            },
            other => return Err(format!("unknown kind {other:?}")),
        };
        items.push(Item { id, kind, source });
    }
    let modules = bundled_modules(engine);
    for m in &modules {
        let top = m.split('.').next().unwrap_or(m);
        items.push(Item {
            id: format!("import/{m}"),
            kind: Kind::Import,
            source: format!(
                "import {m}\nnames = sorted(n for n in dir({m}) if not n.startswith('_'))\nprint(len(names), names[:3], names[-3:])\ndel {top}\n"
            ),
        });
    }
    let mut tops: Vec<&str> = modules
        .iter()
        .map(|m| m.split('.').next().unwrap_or(m))
        .collect();
    tops.dedup();
    items.push(Item {
        id: "import/all".to_owned(),
        kind: Kind::Import,
        source: format!(
            "import {}\nimport sys\nprint(len(sys.modules))\ndel {}\n",
            modules.join(", "),
            tops.join(", ")
        ),
    });
    Ok(items)
}

#[allow(clippy::too_many_lines, reason = "options, setup, the monitor loop")]
fn main() -> Res<()> {
    let args: Vec<String> = std::env::args().collect();
    let (Some(workload), Some(out_dir)) = (args.get(1), args.get(2)) else {
        return Err("usage: soak WORKLOAD.jsonl OUT_DIR [options]".to_owned());
    };
    let opt = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let num = |name: &str, default: u64| opt(name).and_then(|v| v.parse().ok()).unwrap_or(default);
    let minutes = num("--minutes", 60);
    let threads = num("--threads", 6) as usize;
    let inflight = num("--inflight", 24) as usize;
    let cap = num("--cache-cap", micro_monty::DEFAULT_MODULE_CACHE);
    let max_fuel = num("--max-fuel", 3_000_000);
    let seed = num("--seed", 0x5eed);
    let interval = num("--interval", 30);
    let default_memory = Limits::default().memory;
    let mut levels = vec![default_memory];
    let kib = opt("--limits").unwrap_or_else(|| "16384,4096,3072,2048,1536,1024".to_owned());
    for k in kib.split(',') {
        levels.push(k.parse::<u64>().map_err(|e| e.to_string())? << 10);
    }
    let out_dir = PathBuf::from(out_dir);
    std::fs::create_dir_all(&out_dir).map_err(|e| e.to_string())?;
    let engine = Engine::with_module_cache(config(default_memory), cap);
    let mut items = load_items(Path::new(workload), &engine)?;
    // `--only A,B`: just the items whose id contains one of these.
    if let Some(only) = opt("--only") {
        let keys: Vec<&str> = only.split(',').collect();
        items.retain(|i| keys.iter().any(|k| i.id.contains(k)));
    }
    // `--probe file:PATH`: a script of one's own, as an own case.
    if let Some(path) = opt("--probe").and_then(|p| p.strip_prefix("file:").map(str::to_owned)) {
        items.push(Item {
            id: format!("file:{path}"),
            kind: Kind::Own { file: path.clone() },
            source: std::fs::read_to_string(&path).map_err(|e| e.to_string())?,
        });
    }
    let images: Vec<Option<Vec<u8>>> = levels
        .iter()
        .enumerate()
        .map(|(i, &memory)| {
            if i == 0 {
                return None;
            }
            let e = Engine::with_module_cache(config(memory), 0);
            match Session::new(&e).map(|mut s| s.snapshot()) {
                Ok(Ok(image)) => Some(image),
                other => {
                    eprintln!("level {memory} B: no empty session ({other:?}); dropped");
                    None
                }
            }
        })
        .collect();
    let sh = Shared {
        engine,
        items,
        levels,
        images,
        refs: RwLock::default(),
        skipped: RwLock::default(),
        queue: Mutex::default(),
        stats: Stats::default(),
        stop: AtomicBool::new(false),
        max_fuel,
        out_dir: out_dir.clone(),
        diff_count: AtomicU64::new(0),
    };
    if let Some(path) = opt("--restore") {
        let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
        for cap in [micro_monty::DEFAULT_MODULE_CACHE, 0] {
            let e = Engine::with_module_cache(config(sh.levels[0]), cap);
            println!("cache cap {cap}: {:?}", e.restore(&bytes).map(|_| "restored"));
            println!("  again on the same engine: {:?}", e.restore(&bytes).map(|_| "restored"));
        }
        println!("shared engine: {:?}", sh.engine.restore(&bytes).map(|_| "restored"));
        return Ok(());
    }
    if let Some(id) = opt("--probe-replay") {
        let decisions: Vec<bool> = opt("--decisions").unwrap_or_default().chars().map(|c| c == '1').collect();
        let slice = opt("--slice").and_then(|v| v.parse().ok());
        return probe_replay(&sh, &id, num("--level", 0) as usize, slice, &decisions);
    }
    if let Some(id) = opt("--probe-schedules") {
        return probe_schedules(&sh, &id, num("--level", 0) as usize, num("--runs", 100));
    }
    if let Some(id) = opt("--probe") {
        return probe(&sh, &id, num("--level", 0) as usize);
    }
    let kinds = |p: &str| sh.items.iter().filter(|i| i.id.starts_with(p)).count();
    eprintln!(
        "soak: {} items ({} own, {} imports, rest codemode), levels {:?} B, cache cap {cap} B, \
         {threads} threads, {inflight} in flight, {minutes} min, build {}",
        sh.items.len(),
        kinds("own/"),
        kinds("import/"),
        sh.levels,
        micro_monty::build_id()
            .iter()
            .take(6)
            .fold(String::new(), |mut s, b| {
                let _ = write!(s, "{b:02x}");
                s
            })
    );
    let log_path = out_dir.join("log.csv");
    let mut log = std::fs::File::create(&log_path).map_err(|e| e.to_string())?;
    let header = "elapsed_s,jobs,same,differ,parks,snapshots,snapshot_mb,snapshot_refused,restore_failed,migrations,memory_errors,memory_edge,refs,ref_unstable,skipped_items,queue,rss_kib,hwm_kib,cache_bytes,cache_cap";
    let _ = writeln!(log, "{header}");
    let started = Instant::now();
    let deadline = started + Duration::from_secs(minutes * 60);
    std::thread::scope(|scope| {
        for t in 0..threads {
            let sh = &sh;
            scope.spawn(move || worker(sh, seed.wrapping_add(t as u64 * 0x9E37_79B9), inflight));
        }
        let mut next = started;
        loop {
            next += Duration::from_secs(interval);
            let now = Instant::now();
            if next > now {
                std::thread::sleep(next - now);
            }
            let s = &sh.stats;
            let g = |a: &AtomicU64| a.load(Ordering::Relaxed);
            let line = format!(
                "{:.0},{},{},{},{},{},{:.1},{},{},{},{},{},{},{},{},{},{},{},{},{}",
                started.elapsed().as_secs_f64(),
                g(&s.jobs),
                g(&s.same),
                g(&s.differ),
                g(&s.parks),
                g(&s.snapshots),
                g(&s.snapshot_bytes) as f64 / 1e6,
                g(&s.snapshot_refused),
                g(&s.restore_failed),
                g(&s.migrations),
                g(&s.memory_errors),
                g(&s.memory_edge),
                g(&s.refs),
                g(&s.ref_unstable),
                g(&s.skipped_items),
                sh.queue.lock().map(|q| q.len()).unwrap_or(0),
                proc_status("VmRSS:"),
                proc_status("VmHWM:"),
                sh.engine.module_cache_bytes(),
                cap,
            );
            let _ = writeln!(log, "{line}");
            let _ = log.flush();
            eprintln!("{line}");
            if Instant::now() >= deadline {
                sh.stop.store(true, Ordering::Relaxed);
                break;
            }
        }
    });
    eprintln!(
        "soak: done; {} jobs, {} differ",
        sh.stats.jobs.load(Ordering::Relaxed),
        sh.stats.differ.load(Ordering::Relaxed)
    );
    if sh.stats.differ.load(Ordering::Relaxed) > 0 || sh.stats.restore_failed.load(Ordering::Relaxed) > 0 {
        return Err("differences found".to_owned());
    }
    Ok(())
}
