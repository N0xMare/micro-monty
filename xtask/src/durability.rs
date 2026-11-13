//! `cargo xtask durability`: the durability test (docs/spec.md §13).
//!
//! Every case that passes on micro-monty (all of `conformance/own/` and the
//! corpus cases `conformance/results/micro-monty.jsonl` records as `pass`)
//! runs through `conformance/oracle.py --durability N`: once unparked, and
//! once parked every N fuel units (one unit is one safepoint) and at every
//! host call, each park saved to a FULL snapshot by one `mm` process and
//! continued by a new one. stdout, the result and the exception must be
//! identical. A failure names the case and the first park after which the
//! output diverges.
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
//! wasmtime, once starting native and once starting on wasm (docs/sdk.md,
//! "Cross-target restore"). Both builds have the same build id.
//!
//! `--fuel` also compares the fuel each parked chain spent with the
//! unparked run's (docs/spec.md §5, guarantee 4), prints every case's
//! delta and fails on a case that differs unless `FUEL_ALLOWLIST` names
//! it (one case and the reason per line). Not in `cargo xtask check` until
//! the allowlist holds only the guarantee's documented exceptions.

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
/// Parks for the subset: every unit, at most about 1,000 parks.
const FINE: &str = "1:1000";
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
/// The cases `--fuel` lets differ in fuel: `FILE<TAB>reason` per line;
/// `#` starts a comment.
const FUEL_ALLOWLIST: &str = "conformance/durability-fuel-allow.txt";

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
    /// With `--fuel`: the parked chain's fuel less the unparked run's.
    fuel_delta: i64,
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
    let mut fuel = false;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--filter" => filter = Some(it.next().ok_or("--filter needs a substring")?.clone()),
            "--cross-target" => cross = true,
            "--fuel" => fuel = true,
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
    let root = workspace_root();
    let allowed = if fuel {
        read_allowlist(&root.join(FUEL_ALLOWLIST))?
    } else {
        BTreeMap::new()
    };
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
            jobs.push(Job {
                case: case.clone(),
                spec: fine.clone(),
                first: None,
            });
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
    let policy = match &every {
        _ if cross => format!(
            "at {}, each snapshot restored on the other target (native <-> wasm32-wasip1, \
             both directions)",
            every.as_deref().unwrap_or(COARSE)
        ),
        Some(n) => format!("at {n}"),
        None => format!(
            "at {COARSE} (N:M = every N fuel units, at most ~M parks), and at {fine} for the \
             own cases and one corpus case in {FINE_STRIDE}"
        ),
    };
    println!(
        "durability: {} own + {} corpus cases ({} runs), parked {policy} and at every host \
         call, one mm process per park ({}), {workers} workers",
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
                        job,
                        timeout,
                        fuel,
                    );
                    let took = start.elapsed().as_secs_f64();
                    if let Ok(mut all) = outcomes.lock() {
                        all.push((job_key(job), took, o));
                    }
                }
            });
        }
    });
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
    // A fuel difference on an allowed case passes; its delta still prints.
    let fuel_only = |o: &Outcome| o.status == "fuel" && allowed.contains_key(&o.file);
    let failed: Vec<&Outcome> = outcomes
        .iter()
        .filter(|o| o.status != "pass" && o.status != "skip" && !fuel_only(o))
        .collect();
    let skipped = outcomes.iter().filter(|o| o.status == "skip").count();
    println!(
        "{label}: {} of {} runs identical to an unparked run, {skipped} skipped, {parks} \
         parks (= {parks} snapshots restored in a new process), {:.1} s",
        outcomes.len() - failed.len() - skipped,
        outcomes.len(),
        started.elapsed().as_secs_f64()
    );
    if fuel {
        let moved: Vec<&Outcome> = outcomes.iter().filter(|o| o.fuel_delta != 0).collect();
        println!(
            "{label}: fuel equal to the unparked run in {} of {} runs",
            outcomes.len() - moved.len() - skipped,
            outcomes.len() - skipped
        );
        for o in &moved {
            let why = allowed.get(&o.file).map_or("NOT ALLOWED", String::as_str);
            println!(
                "  fuel {:+} {} (at {}): {}",
                o.fuel_delta, o.file, o.spec, why
            );
        }
        let used: std::collections::BTreeSet<&str> =
            moved.iter().map(|o| o.file.as_str()).collect();
        for (file, _) in allowed.iter().filter(|(f, _)| !used.contains(f.as_str())) {
            if filter.is_none() && every.is_none() {
                println!("  note: {file} is allowed in {FUEL_ALLOWLIST} but its fuel did not move");
            }
        }
    }
    if failed.is_empty() {
        return Ok(());
    }
    for o in &failed {
        println!(
            "  FAIL {} (at {}): {} [{}]",
            o.file, o.spec, o.reason, o.status
        );
    }
    Err(format!("{label}: {} runs failed", failed.len()))
}

/// `FUEL_ALLOWLIST`: reason by case file name.
fn read_allowlist(path: &Path) -> Result<BTreeMap<String, String>, String> {
    let text =
        fs::read_to_string(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let mut allowed = BTreeMap::new();
    for (n, line) in text.lines().enumerate() {
        let line = line.trim_end();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((file, reason)) = line.split_once('\t').filter(|(_, r)| !r.trim().is_empty())
        else {
            return Err(format!(
                "{}:{}: want `FILE<TAB>reason`",
                path.display(),
                n + 1
            ));
        };
        allowed.insert(file.to_owned(), reason.trim().to_owned());
    }
    Ok(allowed)
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

fn run_job(
    python: &Path,
    oracle: &Path,
    mm: &Path,
    alternate: Option<&Alternate>,
    job: &Job,
    timeout: u64,
    fuel: bool,
) -> Outcome {
    let case_timeout = CASE_TIMEOUT.max(Duration::from_secs(timeout * 15));
    let file = job
        .case
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
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
        fuel_delta: 0,
    };
    let mut command = Command::new(python);
    command
        .arg(oracle)
        .args(["--engine", "micro-monty", "--timeout", &timeout.to_string()])
        .arg("--durability")
        .arg(&job.spec)
        .arg("--mm")
        .arg(mm);
    if fuel {
        command.arg("--fuel");
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
        fuel_delta: v["fuel_delta"].as_i64().unwrap_or(0),
        file,
        spec,
    }
}
