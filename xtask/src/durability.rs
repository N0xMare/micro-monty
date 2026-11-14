//! `cargo xtask durability`: the durability test (docs/spec.md §13).
//!
//! Every case that passes on micro-monty (all of `conformance/own/` and the
//! corpus cases `conformance/results/micro-monty.jsonl` records as `pass`)
//! runs through `conformance/oracle.py --durability N`: once unparked, and
//! in four modes, with the wall clock pinned and counter entropy: unparked;
//! parked every N fuel units (one unit is one safepoint) in one process
//! (`parks`); the same with a snapshot at every stop and the session going
//! on (`snapshot`); and parked every N units and at every host call, each
//! park saved to a FULL snapshot by one `mm` process and continued by a new
//! one (`restore`). Each parked mode's report (stdout, the result and the
//! exception), fuel and final-state digest (the checksum of the session's
//! image at the end) are compared with the unparked run's (docs/spec.md §5.1,
//! clause 4), and `restore`'s with `snapshot`'s (`restore-vs-snapshot`: a
//! restored session must go on as the one that took the same snapshots,
//! whatever a snapshot still changes). A difference fails unless `RESULTS`
//! records it for that case and mode. A failure names the case, the mode
//! and the columns, and for `restore` the first park after which the output
//! diverges.
//!
//! Policy (`N:M`: park every N fuel units, or every ceil(fuel / M) units
//! for a case whose unparked run uses more than N * M, so no case parks
//! much more than M times; the oracle measures the fuel with
//! `mm exec --report-fuel`):
//! - every case at `COARSE` (5:50);
//! - every own case and every `FINE_STRIDE`-th corpus case in name order
//!   (a fixed subset) also at `FINE` (1:1000): a small case parks at every
//!   call and loop iteration. `--fine N:M` sets another cap for the subset
//!   (`cargo xtask check`'s landing tier runs 1:100; `--release-gate`, 1:1000).
//!
//! `--cross-target` instead runs the own cases at `COARSE` with every
//! snapshot restored on the other target: the parked chain alternates
//! between the native `mm` and `mm` built for `wasm32-wasip1` under
//! wasmtime, once starting native and once starting on wasm (docs/user-guide.md,
//! "Cross-target restore"); both builds have the same build id. That chain
//! is mode `cross`, and the wasm-first job also runs the unparked and the
//! in-process modes on wasm (`wasm`, `wasm-parks`, `wasm-snapshot`), each
//! compared with the native unparked run.
//!
//! `--record` only shrinks `RESULTS`: it rewrites it without the entries no
//! run differed in. It runs the subset at both gates' policies (`1:100`
//! and `1:1000`, beside `5:50`), takes no other policy or filter, and
//! writes only when every job finished and differed only where `RESULTS`
//! records. An entry
//! is added only by hand, in the commit that makes the case differ, with
//! its cause and closing step on the line and the reason in the commit
//! message (the file's header). `--report-only` compares the
//! reports alone, as the gate did before fuel and digests (`gc-stress`,
//! whose collections at every safepoint move both).

use std::{
    collections::{BTreeMap, HashMap},
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use serde_json::Value;

use crate::{micro_monty_binary, python, workspace_root};

/// Parks for every case: every 5 units, at most about 50 parks.
const COARSE: &str = "5:50";
/// Parks for the subset: every unit, at most about 1,000 parks (the
/// release gate's policy).
const FINE: &str = "1:1000";
/// The landing tier's policy for the subset (`cargo xtask check`).
pub(crate) const LANDING_FINE: &str = "1:100";
/// Every this many corpus cases (in name order) is in the subset.
const FINE_STRIDE: usize = 3;
/// CPU seconds one `mm` process may take (the oracle's `--timeout`;
/// `--timeout` overrides it; `conformance/oracle.py`'s `MM_TIMEOUT`).
const MM_TIMEOUT_SECS: u64 = 20;
/// Wall-clock limit for one case's whole chain (at least 15 × `--timeout`):
/// a backstop, since every `mm` process in it has its CPU limit. The
/// longest chain, own case 466 at 1:1000, takes 141 s alone and 219–223 s
/// on a loaded orb [M], so 300 s failed `check` on a busy machine. On
/// 4b32aa2 it takes 196 s alone, mostly in the kernel (757 restores in new
/// processes), and 774–879 s while the rest of the parallel `check` runs
/// on 16 cores [M], so 15 minutes left too little room.
const CASE_TIMEOUT: Duration = Duration::from_mins(30);
/// The differences the gate allows: `FILE<TAB>MODE<TAB>COLUMNS<TAB>reason`
/// per line, `COLUMNS` a comma-separated subset of `report`, `fuel` and
/// `digest`; `#` starts a comment.
const RESULTS: &str = "conformance/durability-results.txt";
/// The most one job holds in its scratch directory at once: the case and
/// one image, which the chain rewrites in place and the default 256 MiB
/// memory limit bounds (267 MB at most, own case 534 at 1:1000 [M]).
const SCRATCH_PER_JOB: u64 = 320 << 20;
/// The tmpfs the jobs' scratch goes to when it has room.
const TMPFS: &str = "/dev/shm";
/// A run's scratch directory, under [`TMPFS`] or the system's temporary
/// directory: this, then the pid of the xtask that made it.
const SCRATCH_PREFIX: &str = "mm-durability-";
/// The parked modes, and the columns each is compared in.
/// `restore-vs-snapshot` compares the restore chain with the session that
/// took the same snapshots and went on, not with the unparked run.
const NATIVE_MODES: [&str; 4] = ["parks", "snapshot", "restore", "restore-vs-snapshot"];
const CROSS_MODES: [&str; 4] = ["cross", "wasm", "wasm-parks", "wasm-snapshot"];
const COLUMNS: [&str; 3] = ["report", "fuel", "digest"];

/// A recorded difference: the columns and why.
struct Recorded {
    columns: Vec<String>,
    reason: String,
}

struct Job {
    case: PathBuf,
    spec: String,
    /// Cross-target: which target writes the first snapshot.
    first: Option<&'static str>,
}

/// The cross-target run's second engine: wasmtime and the precompiled
/// wasm32-wasip1 `mm`.
struct Alternate {
    wasmtime: PathBuf,
    module: PathBuf,
}

struct Outcome {
    file: String,
    spec: String,
    status: String,
    parks: u64,
    reason: String,
    /// Mode -> the columns in which it differed from the unparked run.
    diffs: BTreeMap<String, Vec<String>>,
}

#[allow(
    clippy::too_many_lines,
    reason = "options, case lists, workers and the report in order"
)]
pub(crate) fn run(args: &[String]) -> Result<(), String> {
    let mut filter: Option<String> = None;
    let mut every: Option<String> = None;
    let mut fine = FINE.to_owned();
    let mut timeout = MM_TIMEOUT_SECS;
    let mut cross = false;
    let mut record = false;
    let mut report_only = false;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--filter" => filter = Some(it.next().ok_or("--filter needs a substring")?.clone()),
            "--cross-target" => cross = true,
            "--record" => record = true,
            "--report-only" => report_only = true,
            "--every" => every = Some(it.next().ok_or("--every needs N or N:M")?.clone()),
            "--fine" => fine.clone_from(it.next().ok_or("--fine needs N:M")?),
            "--timeout" => {
                let value = it.next().ok_or("--timeout needs seconds")?;
                timeout = value
                    .parse()
                    .map_err(|_| format!("--timeout: `{value}` is not a number of seconds"))?;
            }
            other => return Err(format!("durability: unknown option `{other}`")),
        }
    }
    if record && (filter.is_some() || every.is_some() || fine != FINE || report_only) {
        return Err(
            "--record rewrites the results of a whole run at the default policy: \
             no --filter, --every, --fine or --report-only"
                .to_owned(),
        );
    }
    let root = workspace_root();
    let recorded = read_results(&root.join(RESULTS))?;
    let mm = micro_monty_binary()?;
    let python = python();
    let oracle = root.join("conformance/oracle.py");
    let alternate = if cross {
        Some(crate::wasm::wasip1_mm().map(|(wasmtime, module)| Alternate { wasmtime, module })?)
    } else {
        None
    };

    let mut own: Vec<PathBuf> = fs::read_dir(root.join("conformance/own"))
        .map_err(|e| format!("cannot read conformance/own: {e}"))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "py"))
        .collect();
    own.sort();
    let results = root.join("conformance/results/micro-monty.jsonl");
    let text = fs::read_to_string(&results)
        .map_err(|e| format!("cannot read {}: {e}", results.display()))?;
    // The cross-target run covers the own cases only.
    let mut corpus: Vec<PathBuf> = if cross { "" } else { &text }
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .filter(|v| v["status"] == "pass")
        .filter_map(|v| {
            v["file"]
                .as_str()
                .map(|f| root.join("conformance/monty").join(f))
        })
        .collect();
    corpus.sort();
    let keep = |p: &PathBuf| {
        filter
            .as_deref()
            .is_none_or(|f| p.to_string_lossy().contains(f))
    };
    let mut jobs = Vec::new();
    for (k, case) in own.iter().chain(&corpus).enumerate() {
        if !keep(case) {
            continue;
        }
        let is_own = k < own.len();
        if cross {
            for first in ["native", "wasm"] {
                jobs.push(Job {
                    case: case.clone(),
                    spec: every.clone().unwrap_or_else(|| COARSE.to_owned()),
                    first: Some(first),
                });
            }
            continue;
        }
        if let Some(n) = &every {
            jobs.push(Job {
                case: case.clone(),
                spec: n.clone(),
                first: None,
            });
            continue;
        }
        jobs.push(Job {
            case: case.clone(),
            spec: COARSE.to_owned(),
            first: None,
        });
        if is_own || (k - own.len()).is_multiple_of(FINE_STRIDE) {
            // `--record` runs the subset at both gates' policies: which park
            // differs depends on the policy (463's `parks` digest differs at
            // 1:1000 only), so an entry goes only when no gate would see it.
            let specs = if record {
                &[LANDING_FINE, FINE][..]
            } else {
                &[fine.as_str()][..]
            };
            for spec in specs {
                jobs.push(Job {
                    case: case.clone(),
                    spec: (*spec).to_owned(),
                    first: None,
                });
            }
        }
    }
    if jobs.is_empty() {
        return Err("no case matches the filter".to_owned());
    }
    // Longest first, by the times the last run recorded, so no long chain
    // starts last; without a record, fine-grained runs first (they take
    // longest), then by name. The order changes no run's result.
    let times_file = root.join(if cross {
        "target/durability/times-cross.tsv"
    } else {
        "target/durability/times.tsv"
    });
    let known = read_times(&times_file);
    let last_time = |job: &Job| known.get(&job_key(job)).copied().unwrap_or(f64::INFINITY);
    jobs.sort_by(|a, b| {
        last_time(b)
            .total_cmp(&last_time(a))
            .then_with(|| (a.spec != fine, &a.case).cmp(&(b.spec != fine, &b.case)))
    });
    let workers = thread::available_parallelism().map_or(1, usize::from);
    // A run killed before its end leaves its scratch behind, on the
    // tmpfs as memory no one can reclaim: remove those first, so the
    // room check below sees what is really free.
    if Path::new("/proc/self").is_dir() {
        let alive = |pid: u32| Path::new(&format!("/proc/{pid}")).exists();
        for (dir, bytes) in sweep(&[Path::new(TMPFS), &std::env::temp_dir()], alive) {
            println!(
                "durability: removed {} ({} MiB), left by a run that is gone",
                dir.display(),
                bytes >> 20
            );
        }
    }
    let (scratch, place) = scratch_dir(workers.min(jobs.len()));
    let policy = match &every {
        _ if cross => format!(
            "at {}, each snapshot restored on the other target (native <-> wasm32-wasip1, \
             both directions)",
            every.as_deref().unwrap_or(COARSE)
        ),
        Some(n) => format!("at {n}"),
        None => format!(
            "at {COARSE} (N:M = every N fuel units, at most ~M parks), and at {} for the \
             own cases and one corpus case in {FINE_STRIDE}",
            if record {
                format!("{LANDING_FINE} and {FINE}")
            } else {
                fine.clone()
            }
        ),
    };
    println!(
        "durability: {} own + {} corpus cases ({} runs), parked {policy} and at every host \
         call, one mm process per park ({}), {workers} workers, scratch {place}",
        own.iter().filter(|p| keep(p)).count(),
        corpus.iter().filter(|p| keep(p)).count(),
        jobs.len(),
        mm.display()
    );
    let started = Instant::now();
    let next = AtomicUsize::new(0);
    let outcomes = Mutex::new(Vec::new());
    thread::scope(|scope| {
        for _ in 0..workers.min(jobs.len()) {
            scope.spawn(|| {
                while let Some(job) = jobs.get(next.fetch_add(1, Ordering::Relaxed)) {
                    let start = Instant::now();
                    let o = run_job(
                        &python,
                        &oracle,
                        &mm,
                        alternate.as_ref(),
                        scratch.as_deref(),
                        job,
                        timeout,
                    );
                    let took = start.elapsed().as_secs_f64();
                    if let Ok(mut all) = outcomes.lock() {
                        all.push((job_key(job), took, o));
                    }
                }
            });
        }
    });
    if let Some(dir) = &scratch {
        // Empty unless a job was killed before its oracle cleaned up.
        let _ = fs::remove_dir_all(dir);
    }
    let timed = outcomes
        .into_inner()
        .map_err(|_| "a worker thread panicked".to_owned())?;
    write_times(&times_file, &timed);
    let mut outcomes: Vec<Outcome> = timed.into_iter().map(|(_, _, o)| o).collect();
    outcomes.sort_by(|a, b| (&a.file, &a.spec).cmp(&(&b.file, &b.spec)));
    let label = if cross {
        "durability (cross-target)"
    } else {
        "durability"
    };
    let parks: u64 = outcomes.iter().map(|o| o.parks).sum();
    let modes: &[&str] = if cross { &CROSS_MODES } else { &NATIVE_MODES };
    // Every (case, mode, column) that differed, and the runs that failed:
    // crashed, timed out, or differed where `RESULTS` records nothing.
    let mut seen: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
    let mut failed: Vec<String> = Vec::new();
    for o in &outcomes {
        match o.status.as_str() {
            "pass" | "skip" => continue,
            "differs" => {}
            _ => {
                failed.push(format!(
                    "{} (at {}): {} [{}]",
                    o.file, o.spec, o.reason, o.status
                ));
                continue;
            }
        }
        let mut unrecorded = Vec::new();
        for (mode, columns) in &o.diffs {
            let allowed = recorded.get(&(o.file.clone(), mode.clone()));
            for column in columns {
                if report_only && column != "report" {
                    continue;
                }
                let entry = seen.entry((o.file.clone(), mode.clone())).or_default();
                if !entry.contains(column) {
                    entry.push(column.clone());
                }
                if allowed.is_none_or(|r| !r.columns.contains(column)) {
                    unrecorded.push(format!("{mode} {column}"));
                }
            }
        }
        if !unrecorded.is_empty() {
            failed.push(format!(
                "{} (at {}): {} not in {RESULTS}; {}",
                o.file,
                o.spec,
                unrecorded.join(", "),
                o.reason
            ));
        }
    }
    let skipped = outcomes.iter().filter(|o| o.status == "skip").count();
    let ran = outcomes.len() - skipped;
    println!(
        "{label}: {} of {ran} runs equal to an unparked run in every mode, {skipped} skipped, \
         {parks} parks (= {parks} snapshots restored in a new process), {:.1} s",
        outcomes.iter().filter(|o| o.status == "pass").count(),
        started.elapsed().as_secs_f64()
    );
    let columns: &[&str] = if report_only { &COLUMNS[..1] } else { &COLUMNS };
    for mode in modes {
        let runs = outcomes
            .iter()
            .filter(|o| o.status == "pass" || o.diffs.contains_key(*mode) || o.status == "differs");
        let runs: Vec<&Outcome> = runs.filter(|o| o.status != "skip").collect();
        let runs: Vec<&Outcome> = if *mode == "wasm" || mode.starts_with("wasm-") {
            runs.into_iter()
                .filter(|o| o.spec.ends_with("wasm first"))
                .collect()
        } else {
            runs
        };
        if runs.is_empty() {
            continue;
        }
        let mut line = format!("  {mode:<19}");
        for column in columns {
            let equal = runs
                .iter()
                .filter(|o| {
                    o.diffs
                        .get(*mode)
                        .is_none_or(|d| !d.iter().any(|c| c == column))
                })
                .count();
            let _ = write!(line, " {column} {equal}/{}", runs.len());
        }
        println!("{line}");
    }
    // Recorded differences no run showed: a whole run at the default
    // policy shrinks the file with `--record`.
    let stale: Vec<(String, String, String)> = recorded
        .iter()
        .filter(|((_, mode), _)| modes.contains(&mode.as_str()))
        .flat_map(|((file, mode), r)| {
            r.columns.iter().filter_map(|c| {
                let shown = seen
                    .get(&(file.clone(), mode.clone()))
                    .is_some_and(|s| s.contains(c));
                (!shown).then(|| (file.clone(), mode.clone(), c.clone()))
            })
        })
        .collect();
    if filter.is_none() && every.is_none() && !report_only {
        for (file, mode, column) in &stale {
            println!(
                "  note: {file} {mode} {column} is recorded in {RESULTS} but no run differed in it"
            );
        }
    }
    // A run that crashed, timed out or differed unrecorded did not show
    // what its case differs in, so its entries would look stale: write
    // nothing then.
    if record && !failed.is_empty() {
        println!(
            "{label}: --record left {RESULTS} as it was: a run removes entries only when every \
             job finished and differed only where it records"
        );
    } else if record {
        write_results(&root.join(RESULTS), &recorded, &seen, modes)?;
        println!(
            "{label}: rewrote {RESULTS} without the {} stale entries",
            stale.len()
        );
    }
    if failed.is_empty() {
        return Ok(());
    }
    for f in &failed {
        println!("  FAIL {f}");
    }
    println!(
        "  A difference that is a known cause (the causes in {RESULTS}'s header) is \
         added there by hand, one line per case and mode with its cause and closing step, \
         and named with its reason in the commit message; any other difference is a bug. \
         --record only removes entries."
    );
    Err(format!("{label}: {} runs failed", failed.len()))
}

/// `RESULTS`: the recorded columns and reason by case and mode.
fn read_results(path: &Path) -> Result<BTreeMap<(String, String), Recorded>, String> {
    let text =
        fs::read_to_string(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let mut recorded = BTreeMap::new();
    for (n, line) in text.lines().enumerate() {
        let line = line.trim_end();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let bad = || {
            format!(
                "{}:{}: want `FILE<TAB>MODE<TAB>COLUMNS<TAB>reason`",
                path.display(),
                n + 1
            )
        };
        let mut parts = line.splitn(4, '\t');
        let (Some(file), Some(mode), Some(columns), Some(reason)) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return Err(bad());
        };
        let columns: Vec<String> = columns.split(',').map(str::to_owned).collect();
        let known = NATIVE_MODES.contains(&mode) || CROSS_MODES.contains(&mode);
        if !known
            || reason.trim().is_empty()
            || !columns.iter().all(|c| COLUMNS.contains(&c.as_str()))
        {
            return Err(bad());
        }
        let key = (file.to_owned(), mode.to_owned());
        if recorded.contains_key(&key) {
            return Err(format!(
                "{}:{}: {file} {mode} is recorded twice",
                path.display(),
                n + 1
            ));
        }
        recorded.insert(
            key,
            Recorded {
                columns,
                reason: reason.trim().to_owned(),
            },
        );
    }
    Ok(recorded)
}

/// Rewrites `RESULTS` keeping its comments and, for this run's `modes`,
/// only the columns a run differed in; it never adds an entry.
fn write_results(
    path: &Path,
    recorded: &BTreeMap<(String, String), Recorded>,
    seen: &BTreeMap<(String, String), Vec<String>>,
    modes: &[&str],
) -> Result<(), String> {
    let text =
        fs::read_to_string(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let mut out = String::new();
    for line in text.lines() {
        let mut parts = line.splitn(3, '\t');
        let key = match (parts.next(), parts.next()) {
            (Some(file), Some(mode)) if !line.starts_with('#') => {
                (file.to_owned(), mode.to_owned())
            }
            _ => {
                let _ = writeln!(out, "{line}");
                continue;
            }
        };
        let Some(r) = recorded.get(&key) else {
            continue;
        };
        let columns: Vec<&String> = if modes.contains(&key.1.as_str()) {
            let shown = seen.get(&key);
            r.columns
                .iter()
                .filter(|c| shown.is_some_and(|s| s.contains(c)))
                .collect()
        } else {
            r.columns.iter().collect()
        };
        if !columns.is_empty() {
            let columns: Vec<&str> = columns.iter().map(|c| c.as_str()).collect();
            let _ = writeln!(
                out,
                "{}\t{}\t{}\t{}",
                key.0,
                key.1,
                columns.join(","),
                r.reason
            );
        }
    }
    fs::write(path, out).map_err(|e| format!("cannot write {}: {e}", path.display()))
}

/// A job's key in the times file: its case and parking policy.
fn job_key(job: &Job) -> String {
    format!(
        "{}\t{}\t{}",
        job.case.display(),
        job.spec,
        job.first.unwrap_or("-")
    )
}

/// The seconds each job took on the last run, by [`job_key`]; empty when
/// there is no record.
fn read_times(path: &Path) -> HashMap<String, f64> {
    let text = fs::read_to_string(path).unwrap_or_default();
    text.lines()
        .filter_map(|line| {
            let (key, secs) = line.rsplit_once('\t')?;
            Some((key.to_owned(), secs.parse().ok()?))
        })
        .collect()
}

/// Records each job's seconds for the next run's order, beside the jobs a
/// filtered run left out; best effort.
fn write_times(path: &Path, timed: &[(String, f64, Outcome)]) {
    let mut all: BTreeMap<String, f64> = read_times(path).into_iter().collect();
    all.extend(timed.iter().map(|(key, secs, _)| (key.clone(), *secs)));
    let mut text = String::new();
    for (key, secs) in all {
        let _ = writeln!(text, "{key}\t{secs:.2}");
    }
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    let _ = fs::write(path, text);
}

/// Removes the scratch directories in `bases` whose run is not `alive` (by
/// the pid in its name), and returns each with the bytes it held. A
/// directory of a live run, or any other entry, stays.
fn sweep(bases: &[&Path], alive: impl Fn(u32) -> bool) -> Vec<(PathBuf, u64)> {
    let mut removed = Vec::new();
    for base in bases {
        let Ok(entries) = fs::read_dir(base) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            let name = entry.file_name();
            let Some(pid) = name
                .to_str()
                .and_then(|n| n.strip_prefix(SCRATCH_PREFIX))
                .and_then(|p| p.parse::<u32>().ok())
            else {
                continue;
            };
            let dir = entry.path();
            if alive(pid) || !dir.is_dir() {
                continue;
            }
            let bytes = size_of(&dir);
            if fs::remove_dir_all(&dir).is_ok() {
                removed.push((dir, bytes));
            }
        }
    }
    removed
}

/// The bytes of the files under `dir`, without following links.
fn size_of(dir: &Path) -> u64 {
    let mut total = 0;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in fs::read_dir(&d)
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
        {
            match entry.file_type() {
                Ok(t) if t.is_dir() => stack.push(entry.path()),
                Ok(t) if t.is_file() => total += entry.metadata().map_or(0, |m| m.len()),
                _ => {}
            }
        }
    }
    total
}

/// Where the jobs' scratch directories go, and how the header says it: a
/// directory on [`TMPFS`] when it has room for `jobs` jobs'
/// [`SCRATCH_PER_JOB`] and the memory twice that (a tmpfs page is memory
/// no one can reclaim; the jobs' own heaps need the rest), so the images a
/// fine chain writes and reads (about 33 GB each way for own cases 466
/// and 534 at 1:1000 [M]) never reach the disk; else one in the system's
/// temporary directory. Either is named for this run ([`SCRATCH_PREFIX`]),
/// so [`sweep`] finds it if the run is killed.
fn scratch_dir(jobs: usize) -> (Option<PathBuf>, String) {
    let need = SCRATCH_PER_JOB * jobs as u64;
    let mib = |b: u64| format!("{} MiB", b >> 20);
    // `df` prints the bytes available on the tmpfs; MemAvailable (KiB)
    // whether the memory behind it has them too.
    let tmpfs = Command::new("df")
        .args(["-B1", "--output=avail", TMPFS])
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .nth(1)?
                .trim()
                .parse::<u64>()
                .ok()
        });
    let memory = fs::read_to_string("/proc/meminfo").ok().and_then(|m| {
        let line = m.lines().find(|l| l.starts_with("MemAvailable:"))?;
        Some(line.split_whitespace().nth(1)?.parse::<u64>().ok()? * 1024)
    });
    let own = format!("{SCRATCH_PREFIX}{}", std::process::id());
    let disk = |why: String| {
        let dir = std::env::temp_dir().join(&own);
        match fs::create_dir_all(&dir) {
            Ok(()) => (
                Some(dir.clone()),
                format!("on disk, {} ({why})", dir.display()),
            ),
            Err(e) => (
                None,
                format!("on disk ({why}; cannot create {}: {e})", dir.display()),
            ),
        }
    };
    let (Some(free), Some(memory)) = (tmpfs, memory) else {
        return disk(if !Path::new(TMPFS).is_dir() {
            format!("no {TMPFS}")
        } else if tmpfs.is_none() {
            format!("`df` cannot tell {TMPFS}'s free space")
        } else {
            "no MemAvailable in /proc/meminfo".to_owned()
        });
    };
    if free < need || memory < 2 * need {
        return disk(format!(
            "{TMPFS} has {} free and the memory {}; {jobs} jobs need {} there and {} of memory",
            mib(free),
            mib(memory),
            mib(need),
            mib(2 * need)
        ));
    }
    let dir = Path::new(TMPFS).join(&own);
    match fs::create_dir_all(&dir) {
        Ok(()) => {
            let shown = format!(
                "on {} ({} free, {} needed)",
                dir.display(),
                mib(free),
                mib(need)
            );
            (Some(dir), shown)
        }
        Err(e) => disk(format!("cannot create {}: {e}", dir.display())),
    }
}

fn run_job(
    python: &Path,
    oracle: &Path,
    mm: &Path,
    alternate: Option<&Alternate>,
    scratch: Option<&Path>,
    job: &Job,
    timeout: u64,
) -> Outcome {
    let case_timeout = CASE_TIMEOUT.max(Duration::from_secs(timeout * 15));
    let file = job
        .case
        .file_name()
        .map_or_default(|n| n.to_string_lossy().into_owned());
    let spec = match job.first {
        Some(first) => format!("{}, {first} first", job.spec),
        None => job.spec.clone(),
    };
    let fail = |status: &str, reason: String| Outcome {
        file: file.clone(),
        spec: spec.clone(),
        status: status.to_owned(),
        parks: 0,
        reason,
        diffs: BTreeMap::new(),
    };
    let mut command = Command::new(python);
    command
        .arg(oracle)
        .args(["--engine", "micro-monty", "--timeout", &timeout.to_string()])
        .arg("--durability")
        .arg(&job.spec)
        .arg("--mm")
        .arg(mm);
    if let Some(dir) = scratch {
        // The oracle's scratch directories (`tempfile.mkdtemp`).
        command.env("TMPDIR", dir);
    }
    if let (Some(alt), Some(first)) = (alternate, job.first) {
        command
            .arg("--alternate")
            .arg(&alt.module)
            .arg("--wasmtime")
            .arg(&alt.wasmtime)
            .args(["--alternate-first", first]);
    }
    let child = command
        .arg(&job.case)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    let mut child = match child {
        Ok(c) => c,
        Err(e) => return fail("error", format!("cannot start the oracle: {e}")),
    };
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if started.elapsed() > case_timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return fail("timeout", format!("over {} s", case_timeout.as_secs()));
            }
            Ok(None) => thread::sleep(Duration::from_millis(5)),
            Err(e) => return fail("error", format!("cannot wait for the oracle: {e}")),
        }
    }
    let output = match child.wait_with_output() {
        Ok(o) => o,
        Err(e) => return fail("error", format!("cannot read the oracle: {e}")),
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    let Some(v) = stdout
        .lines()
        .rev()
        .find_map(|l| serde_json::from_str::<Value>(l).ok())
    else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let tail = stderr.lines().last().unwrap_or("").to_owned();
        return fail("error", format!("oracle printed no result: {tail}"));
    };
    Outcome {
        status: v["status"].as_str().unwrap_or("error").to_owned(),
        parks: v["parks"].as_u64().unwrap_or(0),
        reason: v["reason"].as_str().unwrap_or("").to_owned(),
        diffs: v["diffs"].as_object().map_or_else(BTreeMap::new, |d| {
            d.iter()
                .map(|(mode, columns)| {
                    let columns = columns.as_array().map_or_else(Vec::new, |c| {
                        c.iter()
                            .filter_map(|c| c.as_str().map(str::to_owned))
                            .collect()
                    });
                    (mode.clone(), columns)
                })
                .collect()
        }),
        file,
        spec,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, reason = "tests fail on a broken fixture")]

    use super::*;

    #[test]
    fn a_sweep_removes_only_the_scratch_of_runs_that_are_gone() {
        let base = std::env::temp_dir().join(format!("sweep-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let make = |name: &str, bytes: usize| {
            let dir = base.join(name);
            fs::create_dir_all(dir.join("tmpabc")).unwrap();
            fs::write(dir.join("tmpabc/session.mmsnap"), vec![0_u8; bytes]).unwrap();
            dir
        };
        let gone = make("mm-durability-41", 3_000);
        let live = make("mm-durability-42", 10);
        let other = make("mm-durability-x", 10);
        let unrelated = make("tmp-41", 10);
        let removed = sweep(&[&base, &base.join("missing")], |pid| pid == 42);
        assert_eq!(removed, vec![(gone.clone(), 3_000)]);
        assert!(!gone.exists());
        assert!(live.exists() && other.exists() && unrelated.exists());
        // Nothing is left to remove the second time.
        assert_eq!(sweep(&[&base], |pid| pid == 42), Vec::new());
        fs::remove_dir_all(&base).unwrap();
    }
}
