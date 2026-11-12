//! `soak-wasm`: the soak (../main.rs) on the mm-wasm guest through
//! `crates/mm-wasm-host`: one runtime, many sessions across threads, each
//! in its own instance (so each has its own decoded-module cache), parked in
//! fuel slices, saved and restored into new instances, moved between
//! threads, and checked against an unparked run in a new instance. Logs the
//! host process's RSS. bench/evidence/soak/README.md.
//!
//! ```text
//! soak-wasm GUEST.wasm WORKLOAD.jsonl OUT_DIR [--minutes M] [--threads T]
//!           [--inflight K] [--limits KIB,..] [--max-fuel F] [--seed S]
//! ```
//!
//! The reference host's wall clock and entropy are real, so sessions run
//! without them (`time.time()` and `os.urandom` raise) to stay
//! deterministic.
#![forbid(unsafe_code)]

use mm_wasm_host::{
    HostModule, HostValue, Limits, Progress, Resume, Runtime, RuntimeConfig, Session,
    SessionOptions, Stream,
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
use micro_monty::{HostError, HostErrorKind};

const ENOUGH_FUEL: u64 = 1 << 60;

enum Kind {
    Codemode { tools: Vec<String>, replies: Vec<Value> },
    Own { file: String },
    Import,
}

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
        self.stdout.hash(&mut h);
        self.stderr.hash(&mut h);
        self.end.hash(&mut h);
        self.calls.hash(&mut h);
        h.finish()
    }
    fn to_json(&self) -> Value {
        json!({"stdout": self.stdout, "stderr": self.stderr, "end": self.end, "calls": self.calls})
    }
    fn mentions_memory_error(&self) -> bool {
        [&self.stdout, &self.stderr, &self.end]
            .iter()
            .any(|s| s.contains("MemoryError"))
    }
}

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

type Output = Arc<Mutex<(String, String)>>;

fn options(memory: u64) -> SessionOptions {
    let mut o = SessionOptions::new().with_limits(Limits::new().with_memory(memory));
    o.last_expr_value = false;
    o.wall_clock = false;
    o.entropy = false;
    o
}

fn attach(s: &mut Session, out: &Output) {
    let sink = Arc::clone(out);
    s.set_output(Box::new(move |stream, text| {
        let mut o = sink.lock().unwrap_or_else(PoisonError::into_inner);
        if o.0.len() + o.1.len() < 4 << 20 {
            if stream == Stream::Stderr {
                o.1.push_str(text);
            } else {
                o.0.push_str(text);
            }
        }
    }));
    s.set_zone_source(Some(Box::new(|key: &str| {
        if key.starts_with('/') || key.split('/').any(|p| p == ".." || p.is_empty()) {
            return None;
        }
        std::fs::read(Path::new("/usr/share/zoneinfo").join(key))
            .ok()
            .filter(|b| b.starts_with(b"TZif"))
    })));
}

fn prepare(s: &mut Session, item: &Item) -> Res<()> {
    match &item.kind {
        Kind::Codemode { tools, .. } => {
            let names: Vec<&str> = tools.iter().map(String::as_str).collect();
            s.define(&HostModule::untyped("_ptc", &names))
                .map_err(|e| format!("{e:?}"))?;
        }
        Kind::Own { file } => {
            s.set_module_dunders(Some("__main__"), Some(file))
                .map_err(|e| format!("{e:?}"))?;
        }
        Kind::Import => {}
    }
    Ok(())
}

#[derive(Clone, Copy)]
struct Plan {
    slice: Option<u64>,
    snapshot: u64,
    migrate: u64,
}

struct Job {
    item: usize,
    level: usize,
    session: Session,
    out: Output,
    cells: Vec<(String, String)>,
    cell: usize,
    progress: Option<Progress>,
    pending: Option<Resume>,
    answers: BTreeMap<u64, Result<HostValue, HostError>>,
    next_reply: usize,
    calls: Vec<String>,
    plan: Plan,
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
    memory_edge: AtomicU64,
    parks: AtomicU64,
    snapshots: AtomicU64,
    snapshot_bytes: AtomicU64,
    snapshot_refused: AtomicU64,
    restore_failed: AtomicU64,
    traps: AtomicU64,
    migrations: AtomicU64,
    refs: AtomicU64,
}

struct Shared {
    rt: Runtime,
    items: Vec<Item>,
    levels: Vec<u64>,
    refs: RwLock<HashMap<(usize, usize), (u64, u64)>>,
    skipped: RwLock<HashSet<usize>>,
    queue: Mutex<VecDeque<Job>>,
    stats: Stats,
    stop: AtomicBool,
    max_fuel: u64,
    out_dir: PathBuf,
    diff_count: AtomicU64,
}

fn start_job(sh: &Shared, item: usize, level: usize, plan: Plan) -> Res<Job> {
    let mut session = sh
        .rt
        .session(&options(sh.levels[level]))
        .map_err(|e| format!("{e:?}"))?;
    let out: Output = Arc::default();
    attach(&mut session, &out);
    prepare(&mut session, &sh.items[item])?;
    Ok(Job {
        item,
        level,
        session,
        out,
        cells: sh.items[item].cells(),
        cell: 0,
        progress: None,
        pending: None,
        answers: BTreeMap::new(),
        next_reply: 0,
        calls: Vec::new(),
        plan,
        trail: Vec::new(),
        end: None,
    })
}

enum Step {
    Done,
    Yield,
}

fn save_restore(sh: &Shared, job: &mut Job, at: &str) {
    match job.session.snapshot(None) {
        Ok(bytes) => {
            sh.stats.snapshots.fetch_add(1, Ordering::Relaxed);
            sh.stats
                .snapshot_bytes
                .fetch_add(bytes.len() as u64, Ordering::Relaxed);
            match sh.rt.restore(&bytes, &options(sh.levels[job.level])) {
                Ok(mut s) => {
                    attach(&mut s, &job.out);
                    job.session = s;
                    job.trail.push(format!("snap@{at}:{}", bytes.len()));
                }
                Err(e) => {
                    sh.stats.restore_failed.fetch_add(1, Ordering::Relaxed);
                    job.trail.push(format!("restore-failed@{at}:{e:?}"));
                }
            }
        }
        Err(e) => {
            sh.stats.snapshot_refused.fetch_add(1, Ordering::Relaxed);
            job.trail.push(format!("snap-refused@{at}:{e:?}"));
        }
    }
}

fn fail(job: &mut Job, what: &str, e: &mm_wasm_host::Error, sh: &Shared) -> Step {
    if matches!(e, mm_wasm_host::Error::Trap(_)) {
        sh.stats.traps.fetch_add(1, Ordering::Relaxed);
    }
    job.end = Some(format!("{what}: {e:?}"));
    Step::Done
}

fn drive(sh: &Shared, job: &mut Job, rng: &mut Rng, chaos: bool) -> Step {
    let fuel = job.plan.slice.unwrap_or(ENOUGH_FUEL);
    if let Some(resume) = job.pending.take() {
        match job.session.resume(&resume) {
            Ok(p) => job.progress = Some(p),
            Err(e) => return fail(job, "resume", &e, sh),
        }
    }
    loop {
        let progress = match job.progress.take() {
            Some(p) => p,
            None => {
                let (src, name) = job.cells[job.cell].clone();
                if let Err(e) = job.session.set_fuel(Some(fuel)) {
                    return fail(job, "set_fuel", &e, sh);
                }
                match job.session.start(&src, &name) {
                    Ok(p) => p,
                    Err(mm_wasm_host::Error::Compile(e)) => {
                        job.end = Some(format!("compile: {}", e.rendered));
                        return Step::Done;
                    }
                    Err(e) => return fail(job, "start", &e, sh),
                }
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
                if chaos && rng.chance(job.plan.snapshot) {
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
                if chaos && rng.chance(job.plan.snapshot) {
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
                if rng.chance(job.plan.snapshot) {
                    save_restore(sh, job, "park");
                }
                if let Err(e) = job.session.set_fuel(Some(fuel)) {
                    return fail(job, "set_fuel", &e, sh);
                }
                if rng.chance(job.plan.migrate) {
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
                job.end = Some(format!("error: {}", e.traceback));
                return Step::Done;
            }
            other => {
                job.end = Some(format!("unexpected: {other:?}"));
                return Step::Done;
            }
        };
        match job.session.resume(&resume) {
            Ok(p) => job.progress = Some(p),
            Err(e) => return fail(job, "resume", &e, sh),
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

fn reference(sh: &Shared, item: usize, level: usize) -> Res<(Transcript, u64)> {
    let plan = Plan {
        slice: None,
        snapshot: 0,
        migrate: 0,
    };
    let mut job = start_job(sh, item, level, plan)?;
    drive(sh, &mut job, &mut Rng(1), false);
    let left = job.session.fuel().ok().flatten().unwrap_or(ENOUGH_FUEL);
    Ok((transcript(&job), ENOUGH_FUEL - left))
}

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
        let level = if rng.chance(50) {
            0
        } else {
            rng.below(sh.levels.len() as u64) as usize
        };
        let fuel = match reference_of(sh, item, level) {
            Ok((_, fuel)) if fuel <= sh.max_fuel => fuel,
            other => {
                if let Err(e) = other {
                    eprintln!("reference of {} failed: {e}", sh.items[item].id);
                }
                sh.skipped
                    .write()
                    .unwrap_or_else(PoisonError::into_inner)
                    .insert(item);
                continue;
            }
        };
        let parks = [0, 5, 50, 300][rng.below(4) as usize];
        let slice = (parks > 0).then(|| (fuel / parks).max(1) + rng.below(3));
        let plan = Plan {
            slice,
            snapshot: [0, 10, 30, 100][rng.below(4) as usize],
            migrate: [0, 30, 70][rng.below(3) as usize],
        };
        match start_job(sh, item, level, plan) {
            Ok(mut job) => {
                job.trail.push(format!(
                    "slice={slice:?} snapshot={} migrate={}",
                    plan.snapshot, plan.migrate
                ));
                return Some(job);
            }
            Err(e) => {
                eprintln!("start failed for {} at level {level}: {e}", sh.items[item].id);
            }
        }
    }
}

fn finish(sh: &Shared, job: &Job) {
    sh.stats.jobs.fetch_add(1, Ordering::Relaxed);
    let got = transcript(job);
    let want = reference_of(sh, job.item, job.level).map(|r| r.0);
    if want == Ok(got.hash()) {
        sh.stats.same.fetch_add(1, Ordering::Relaxed);
        return;
    }
    let plain = reference(sh, job.item, job.level).ok().map(|r| r.0);
    if job.level > 0
        && (got.mentions_memory_error() || plain.as_ref().is_some_and(Transcript::mentions_memory_error))
    {
        sh.stats.memory_edge.fetch_add(1, Ordering::Relaxed);
        return;
    }
    sh.stats.differ.fetch_add(1, Ordering::Relaxed);
    let n = sh.diff_count.fetch_add(1, Ordering::Relaxed);
    if n >= 200 {
        return;
    }
    let doc = json!({
        "what": "differs from an unparked run in a new instance",
        "item": sh.items[job.item].id,
        "level": job.level,
        "memory": sh.levels[job.level],
        "trail": job.trail,
        "got": got.to_json(),
        "want": plain.as_ref().map(Transcript::to_json),
    });
    let path = sh.out_dir.join(format!("diff-{n:03}.json"));
    let _ = std::fs::write(&path, serde_json::to_string_pretty(&doc).unwrap_or_default());
    eprintln!(
        "DIFF {} at level {} -> {}",
        sh.items[job.item].id,
        job.level,
        path.display()
    );
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
            Step::Done => finish(sh, &job),
            Step::Yield => {
                sh.queue
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .push_back(job);
            }
        }
    }
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

fn load_items(path: &Path, rt: &Runtime) -> Res<Vec<Item>> {
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
    let env = rt
        .session(&SessionOptions::default())
        .and_then(|mut s| s.environment())
        .map_err(|e| format!("{e:?}"))?;
    let modules: Vec<String> = env
        .lines()
        .find_map(|l| l.strip_prefix("Modules: "))
        .map(|l| {
            l.trim_end_matches('.')
                .split(", ")
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
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
    let (Some(guest), Some(workload), Some(out_dir)) = (args.get(1), args.get(2), args.get(3))
    else {
        return Err("usage: soak-wasm GUEST.wasm WORKLOAD.jsonl OUT_DIR [options]".to_owned());
    };
    let opt = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let num = |name: &str, default: u64| opt(name).and_then(|v| v.parse().ok()).unwrap_or(default);
    let minutes = num("--minutes", 60);
    let threads = num("--threads", 4) as usize;
    let inflight = num("--inflight", 16) as usize;
    let max_fuel = num("--max-fuel", 3_000_000);
    let seed = num("--seed", 0x5eed);
    let interval = num("--interval", 30);
    let mut levels = vec![Limits::default().memory];
    let kib = opt("--limits").unwrap_or_else(|| "16384,4096,3072,2048,1536".to_owned());
    for k in kib.split(',') {
        levels.push(k.parse::<u64>().map_err(|e| e.to_string())? << 10);
    }
    let out_dir = PathBuf::from(out_dir);
    std::fs::create_dir_all(&out_dir).map_err(|e| e.to_string())?;
    let rt = Runtime::new(Path::new(guest), RuntimeConfig::default()).map_err(|e| format!("{e:?}"))?;
    let mut items = load_items(Path::new(workload), &rt)?;
    if let Some(only) = opt("--only") {
        let keys: Vec<&str> = only.split(',').collect();
        items.retain(|i| keys.iter().any(|k| i.id.contains(k)));
    }
    let sh = Shared {
        rt,
        items,
        levels,
        refs: RwLock::default(),
        skipped: RwLock::default(),
        queue: Mutex::default(),
        stats: Stats::default(),
        stop: AtomicBool::new(false),
        max_fuel,
        out_dir: out_dir.clone(),
        diff_count: AtomicU64::new(0),
    };
    eprintln!(
        "soak-wasm: {guest}, {} items, levels {:?} B, {threads} threads, {inflight} in flight, {minutes} min",
        sh.items.len(),
        sh.levels
    );
    let mut log = std::fs::File::create(out_dir.join("log.csv")).map_err(|e| e.to_string())?;
    let _ = writeln!(
        log,
        "elapsed_s,jobs,same,differ,memory_edge,parks,snapshots,snapshot_mb,snapshot_refused,restore_failed,traps,migrations,refs,queue,queued_guest_mb,rss_kib,hwm_kib"
    );
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
            let (queued, guest_mb) = sh
                .queue
                .lock()
                .map(|q| {
                    (
                        q.len(),
                        q.iter().map(|j| j.session.memory_bytes()).sum::<u64>() as f64 / 1e6,
                    )
                })
                .unwrap_or((0, 0.0));
            let line = format!(
                "{:.0},{},{},{},{},{},{},{:.1},{},{},{},{},{},{},{:.1},{},{}",
                started.elapsed().as_secs_f64(),
                g(&s.jobs),
                g(&s.same),
                g(&s.differ),
                g(&s.memory_edge),
                g(&s.parks),
                g(&s.snapshots),
                g(&s.snapshot_bytes) as f64 / 1e6,
                g(&s.snapshot_refused),
                g(&s.restore_failed),
                g(&s.traps),
                g(&s.migrations),
                g(&s.refs),
                queued,
                guest_mb,
                proc_status("VmRSS:"),
                proc_status("VmHWM:"),
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
    let differ = sh.stats.differ.load(Ordering::Relaxed);
    eprintln!(
        "soak-wasm: done; {} jobs, {differ} differ",
        sh.stats.jobs.load(Ordering::Relaxed)
    );
    if differ > 0 || sh.stats.restore_failed.load(Ordering::Relaxed) > 0 {
        return Err("differences found".to_owned());
    }
    Ok(())
}
