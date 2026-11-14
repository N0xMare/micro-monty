//! `cargo xtask replay-ir`: the recorded real sessions'
//! (`conformance/codemode/real`) instruction counts on the replay worker,
//! head against base, session by session (AGENTS.md's Ir rule). In the
//! release gate.
//!
//! The bench programs reach a small part of the engine: they ran 20 of
//! the 261 functions one size change marked (size review of A+B). This
//! workload is the 160 sessions agents ran: this checkout's `gate.py
//! --check --worker W` replays each with `W`, a script that runs the
//! worker under callgrind. A session's count is its worker's Ir from
//! glibc's `start_thread` on, without `pthread_getattr_np`: the worker
//! serves on a thread of its own, and the main thread's startup reads the
//! environment and paths. A session whose worker `gate.py` kills (it
//! diverged from the recording) counts nothing on that side, and the
//! real-replay stage fails on it.
//!
//! Both workers are `mm-cell-worker`'s `replay_worker` built with this
//! checkout's shipped recipe and `--cfg mm_size_wasm`, which turns on the
//! `optimize(size)` marks that are otherwise on for wasm only, so the
//! sessions cover them too (the 0.0.2 performance review, section 17);
//! the base's is built in its own tree ([`crate::shipped::refs_dir`]) with
//! its own toolchain. Counts are cached per commit, keyed by the recipe,
//! the toolchain, valgrind, the host and the workload (the traces and
//! `gate.py`).
//!
//! The counts are not exact: one binary's sessions differ by up to 0.1%
//! between runs [M, the review]; one session holds 56% of all the
//! instructions. So the gate judges each session's ratio, head over base,
//! and the median of the ratios, not the total: a session past +0.5%
//! ([`SESSION_LIMIT`]) or the median past +0.3% ([`MEDIAN_LIMIT`]) fails
//! unless a `Native-IR-Accepted: sessions <ceiling> <reason>` line covers
//! it. There are no floors and no anchor: the sessions are measured on the
//! base and the head only.

use std::{
    collections::BTreeMap,
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
    process::Command,
    thread,
};

use crate::{
    accept::{self, Failure},
    native_ir::ACCEPTED,
    size,
    wasm_ir::{source_hash, version},
    wasm_wall::{checkout, git},
    workspace_root,
};

/// The workload's name in the verdict and in accept lines.
const NAME: &str = "sessions";
/// The most one session's count may rise, head over base.
const SESSION_LIMIT: f64 = 1.005;
/// The most the median of the sessions' ratios may rise.
const MEDIAN_LIMIT: f64 = 1.003;
/// The cfg that turns on the wasm-only `optimize(size)` marks natively.
const WASM_MARKS: &str = "--cfg mm_size_wasm";
/// What a count holds, in the cache key: change it when that changes.
const COUNTING: &str = "callgrind Ir from start_thread, replay_worker, wasm marks on, 2";
/// The files that make the workload.
const WORKLOAD: [&str; 2] = [
    "conformance/codemode/real/traces.tar.xz",
    "conformance/codemode/real/gate.py",
];

/// Each session's count, by its id with `/` as `.`.
type Counts = BTreeMap<String, u64>;

#[allow(
    clippy::too_many_lines,
    reason = "sides, builds, replays and the verdict in order"
)]
pub(crate) fn run(args: &[String]) -> Result<(), String> {
    let mut base_rev = "origin/main".to_owned();
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--base" => base_rev = args.next().cloned().ok_or("--base needs a revision")?,
            other => return Err(format!("replay-ir: unknown option `{other}`")),
        }
    }
    if !cfg!(target_os = "linux") {
        return Err("replay-ir needs valgrind, which runs on Linux only".to_owned());
    }
    let valgrind = crate::wasm::tool("VALGRIND", "valgrind")
        .ok_or("replay-ir needs valgrind ($VALGRIND, PATH); run .agents/setup")?;
    let root = workspace_root();
    let host = crate::host_target()?;
    let dir = root.join("target/replay-ir");
    let refs = crate::shipped::refs_dir("replay-ir");
    let short = |s: &str| s[..s.len().min(10)].to_owned();
    let mut workload = Vec::new();
    for file in WORKLOAD {
        workload.extend(fs::read(root.join(file)).map_err(|e| format!("{file}: {e}"))?);
    }
    let method = format!(
        "{COUNTING}; {}; {}; workload {}",
        version(&valgrind),
        crate::floors::host(&valgrind),
        source_hash(&workload)
    );
    let base = git(&root, &["merge-base", &base_rev, "HEAD"])?;
    println!("replay-ir: base = {base_rev} ({})", short(&base));
    let base_tree = refs.join("base-src");
    checkout(&root, &base_tree, &base)?;
    let clean = git(&root, &["status", "--porcelain", "--untracked-files=no"])?.is_empty();
    let head_sha = git(&root, &["rev-parse", "HEAD"])?;
    // (label, tree, commit, cache key); a head with uncommitted changes is
    // not its commit.
    let sides = [
        (
            "head",
            root.clone(),
            clean.then_some(head_sha),
            format!("{method}; {}", size::recipe_key(&root, &root)?),
        ),
        (
            "base",
            base_tree.clone(),
            Some(base.clone()),
            format!("{method}; {}", size::recipe_key(&root, &base_tree)?),
        ),
    ];
    let cache = |sha: &str| dir.join("cache").join(format!("{sha}.tsv"));
    let mut counts: Vec<Option<Counts>> = sides
        .iter()
        .map(|(_, _, sha, key)| sha.as_ref().and_then(|s| load(&cache(s), key)))
        .collect();
    let mut workers = Vec::new();
    for (i, (label, tree, _, _)) in sides.iter().enumerate() {
        if counts[i].is_some() {
            println!("replay-ir: the {label}'s counts are cached");
            continue;
        }
        let target = if i == 0 {
            root.join("target/replay-ir/build")
        } else {
            refs.join("base")
        };
        println!("replay-ir: building the {label}'s worker");
        workers.push((i, build(tree, &target, &host)?));
    }
    // The replays run at once, sharing the cores: each ends with its
    // longest session alone (about 8.5 minutes under callgrind [M]).
    let jobs = thread::available_parallelism()
        .map_or(1, usize::from)
        .div_ceil(workers.len().max(1));
    let scratch = dir.join("callgrind");
    let _ = fs::remove_dir_all(&scratch);
    let results: Vec<(usize, Result<Counts, String>)> = thread::scope(|scope| {
        let running: Vec<_> = workers
            .iter()
            .map(|(i, worker)| {
                let out = scratch.join(format!("{i:02}"));
                let (root, valgrind) = (&root, &valgrind);
                let job = scope.spawn(move || replay(root, valgrind, worker, jobs, &out));
                (*i, job)
            })
            .collect();
        running
            .into_iter()
            .map(|(i, job)| {
                let result = job
                    .join()
                    .unwrap_or_else(|_| Err("a replay thread failed".to_owned()));
                (i, result)
            })
            .collect()
    });
    let _ = fs::remove_dir_all(&scratch);
    for (i, result) in results {
        let c = result?;
        if let (Some(sha), key) = (&sides[i].2, &sides[i].3) {
            save(&cache(sha), key, &c)?;
        }
        counts[i] = Some(c);
    }
    let [Some(head), Some(base_counts)] = &counts[..] else {
        return Err("replay-ir: a side has no counts".to_owned());
    };
    let failures = report(head, (&format!("base {}", short(&base)), base_counts));
    let accepts = accept::for_change(&root, ACCEPTED, &base, "MM_IR_ACCEPT")?;
    accept::verdict("replay-ir", ACCEPTED, "MM_IR_ACCEPT", &failures, &accepts)
}

/// Builds `replay_worker` of the tree at `src` for `host` into `target`,
/// with this checkout's shipped recipe plus [`WASM_MARKS`] and the tree's
/// toolchain.
fn build(src: &Path, target: &Path, host: &str) -> Result<PathBuf, String> {
    let status = crate::shipped::shipped_build_in(host, src)
        .env(
            "RUSTFLAGS",
            format!("{} {WASM_MARKS}", crate::shipped::SHIPPED_RUSTFLAGS),
        )
        .arg("--target-dir")
        .arg(target)
        .args([
            "--quiet",
            "--package",
            "mm-cell-worker",
            "--example",
            "replay_worker",
        ])
        .status()
        .map_err(|e| format!("cannot run cargo: {e}"))?;
    if !status.success() {
        return Err(format!(
            "shipped build of replay_worker for {host} in {} failed",
            src.display()
        ));
    }
    Ok(target.join(host).join("release/examples/replay_worker"))
}

/// Every session replayed by this checkout's `gate.py` on `worker` under
/// callgrind, `jobs` at a time, in `out`: each session's count. `gate.py`'s
/// verdict is the real-replay stage's: an older build may differ from the
/// recorded results, and its count is still what this workload costs it.
fn replay(
    root: &Path,
    valgrind: &Path,
    worker: &Path,
    jobs: usize,
    out: &Path,
) -> Result<Counts, String> {
    fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    let local = out.join("replay_worker");
    fs::hard_link(worker, &local)
        .or_else(|_| fs::copy(worker, &local).map(|_| ()))
        .map_err(|e| format!("cannot place {}: {e}", worker.display()))?;
    // gate.py names the session in MM_REPLAY_SESSION.
    let script = out.join("worker.sh");
    let o = out.display();
    fs::write(
        &script,
        format!(
            "#!/bin/sh\nn=$(printf %s \"$MM_REPLAY_SESSION\" | tr / .)\nexec '{}' --tool=callgrind \
             --log-file=\"{o}/$n.%p.log\" --callgrind-out-file=/dev/null \
             --toggle-collect=start_thread '--toggle-collect=pthread_getattr_np*' '{}'\n",
            valgrind.display(),
            local.display()
        ),
    )
    .map_err(|e| format!("{}: {e}", script.display()))?;
    let status = Command::new("chmod")
        .arg("+x")
        .arg(&script)
        .status()
        .map_err(|e| format!("cannot run chmod: {e}"))?;
    if !status.success() {
        return Err(format!("cannot make {} executable", script.display()));
    }
    let output = Command::new(crate::python())
        .current_dir(root)
        .arg("conformance/codemode/real/gate.py")
        .arg("--check")
        .arg("--worker")
        .arg(&script)
        .arg("--jobs")
        .arg(jobs.to_string())
        .output()
        .map_err(|e| format!("cannot run gate.py: {e}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let summary = stdout
        .lines()
        .find(|l| l.contains(" cells in ") && l.contains(" sessions, "))
        .ok_or_else(|| {
            format!(
                "gate.py on {} printed no summary: {}",
                worker.display(),
                String::from_utf8_lossy(&output.stderr)
            )
        })?;
    println!("replay-ir: {}: {summary}", worker.display());
    let mut counts = Counts::new();
    for entry in fs::read_dir(out).map_err(|e| format!("{}: {e}", out.display()))? {
        let path = entry.map_err(|e| e.to_string())?.path();
        let name = path
            .file_name()
            .map_or_default(|n| n.to_string_lossy().into_owned());
        let Some(stem) = name.strip_suffix(".log") else {
            continue;
        };
        let session = stem.rsplit_once('.').map_or(stem, |(s, _)| s).to_owned();
        let text = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let n: u64 = text
            .lines()
            .filter_map(|l| l.split_once("Collected : "))
            .filter_map(|(_, n)| n.trim().parse::<u64>().ok())
            .sum();
        *counts.entry(session).or_default() += n;
    }
    if counts.values().sum::<u64>() == 0 {
        return Err(format!(
            "replay-ir: nothing collected on {}",
            worker.display()
        ));
    }
    Ok(counts)
}

#[allow(clippy::cast_precision_loss, reason = "a ratio of counts")]
fn ratio(head: u64, other: u64) -> f64 {
    head as f64 / other.max(1) as f64
}

#[allow(clippy::cast_precision_loss, reason = "millions, printed")]
fn millions(n: u64) -> f64 {
    n as f64 / 1e6
}

/// Prints the sessions against `other` (named `against`): the median
/// ratio, the total and the sessions that moved most; returns the
/// sessions and the median past their limits.
fn report(head: &Counts, (against, other): (&str, &Counts)) -> Vec<Failure> {
    // Sessions counted on both sides; one a side's worker was killed in
    // counts nothing there and is only reported.
    let mut ratios: Vec<(f64, &String, u64, u64)> = head
        .iter()
        .filter_map(|(k, &h)| other.get(k).map(|&b| (ratio(h, b), k, h, b)))
        .filter(|&(_, _, h, b)| h > 0 && b > 0)
        .collect();
    let lost: Vec<&String> = head
        .iter()
        .filter(|(k, h)| (**h == 0) != (other.get(*k).copied().unwrap_or(0) == 0))
        .map(|(k, _)| k)
        .collect();
    ratios.sort_by(|x, y| x.0.total_cmp(&y.0));
    let median = match ratios.len() {
        0 => 1.0,
        n if n % 2 == 1 => ratios[n / 2].0,
        n => f64::midpoint(ratios[n / 2 - 1].0, ratios[n / 2].0),
    };
    let (h, b) = (
        ratios.iter().map(|r| r.2).sum::<u64>(),
        ratios.iter().map(|r| r.3).sum::<u64>(),
    );
    println!("\n| Sessions | {against} (M Ir) | head (M Ir) | head / it |");
    println!("|---|---:|---:|---:|");
    println!(
        "| total of {} | {:.3} | {:.3} | {:.4} |",
        ratios.len(),
        millions(b),
        millions(h),
        ratio(h, b)
    );
    println!(
        "| median ratio | | | {median:.4} |\n(limits: a session +{:.1}%, the median +{:.1}%)",
        (SESSION_LIMIT - 1.0) * 100.0,
        (MEDIAN_LIMIT - 1.0) * 100.0
    );
    let mut text = String::from("sessions that moved most, head / base:");
    for (r, name, h, b) in ratios.iter().rev().take(5) {
        let _ = write!(
            text,
            "\n  {name}: {:.3} -> {:.3} M Ir ({r:.4})",
            millions(*b),
            millions(*h)
        );
    }
    println!("{text}");
    for name in lost {
        println!("replay-ir: {name} counted on one side only (a killed worker; see real-replay)");
    }
    let mut failures: Vec<Failure> = ratios
        .iter()
        .filter(|r| r.0 > SESSION_LIMIT)
        .map(|&(r, name, _, _)| Failure {
            guest: None,
            name: NAME.to_owned(),
            ratio: r,
            text: format!("session {name} {r:.4} against {against}"),
        })
        .collect();
    if median > MEDIAN_LIMIT {
        failures.push(Failure {
            guest: None,
            name: NAME.to_owned(),
            ratio: median,
            text: format!("the sessions' median ratio {median:.4} against {against}"),
        });
    }
    failures
}

/// The cached counts in `path` under `key`.
fn load(path: &Path, key: &str) -> Option<Counts> {
    let text = fs::read_to_string(path).ok()?;
    let mut lines = text.lines();
    if lines.next() != Some(&format!("# {key}")) {
        return None;
    }
    let counts: Counts = lines
        .filter_map(|line| {
            let (k, n) = line.split_once('\t')?;
            Some((k.to_owned(), n.parse().ok()?))
        })
        .collect();
    (!counts.is_empty()).then_some(counts)
}

fn save(path: &Path, key: &str, counts: &Counts) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let mut text = format!("# {key}\n");
    for (k, n) in counts {
        let _ = writeln!(text, "{k}\t{n}");
    }
    fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
}
