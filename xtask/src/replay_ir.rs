//! `cargo xtask replay-ir`: the recorded real sessions'
//! (`conformance/codemode/real`) instruction counts on the replay worker,
//! head against base, session by session (AGENTS.md's Ir rule). In the
//! release gate.
//!
//! The bench programs reach a small part of the engine: they ran 20 of
//! the 261 functions one size change marked (size review of A+B). This
//! workload is the 160 sessions agents ran: this checkout's `gate.py
//! --check --worker W` replays each with `W`, a script that runs the
//! worker under callgrind, counting everything from glibc's
//! `start_thread` on (the worker serves on a thread of its own; the main
//! thread's startup reads the environment and paths), without
//! `pthread_getattr_np`. Each replay's callgrind dump gives three numbers
//! ([`Count`]): the total; the judged count, the total without the
//! inclusive Ir of glibc's allocator ([`ALLOCATOR`]); and the allocator's
//! calls. The allocator's cost follows the heap's layout, not the work: a
//! worker with one more 24-byte allocation read a session's total up to
//! 1.0062 and down to 0.9933 against the same worker without it (one more
//! `malloc_consolidate`), past the session limit, where the judged count
//! moved within 0.05% [M]. The judged count sees an added allocation only
//! in the worker's own instructions around the call, 17.5 of its 157 Ir
//! (11%) [M], so the calls are judged too: they follow the work, not the
//! layout (same-work workers differ by at most 0.031% of a session's
//! calls [M]). Neither sees work glibc does in proportion to a call's
//! size: the copy inside `realloc` and the zeroing inside `calloc` (5.4%
//! of `memcpy`'s Ir and 5.0% of `memset`'s, up to 4.6% of a session [M]).
//! A session `gate.py` finds diverged from the recording counts nothing,
//! whether it killed the worker or the worker ended, exited or aborted
//! first, or exited with an error after its last cell (it names each such
//! session), and is not compared; one that diverged on the head and not
//! on the base fails, and no accept line covers it.
//!
//! A count repeats only if the worker's heap is laid out the same way in
//! every run: which reallocations copy, and the engine's probes keyed by
//! addresses (`Vm::intern_shared`), follow the layout. Two things moved
//! it, and the replay pins both. The worker's reader thread parses a frame
//! while its serve thread runs, so the frames' timing decided how the two
//! interleaved (one binary read 14.129M and 14.234M Ir on a session, as
//! the load around it changed [M]): `gate.py --lockstep` sends a frame
//! only once every thread of the worker sleeps in two looks with no thread
//! run between them (one look let a frame out early in 45 of 320 replays
//! of a session [M]). And std's `HashMap` keys came from `getrandom`, so a
//! map's drop order differed per process: the worker preloads a fixed
//! `getrandom` ([`FIXED_RANDOM`]).
//!
//! Both workers are `mm-cell-worker`'s `replay_worker` built with this
//! checkout's shipped recipe and `--cfg mm_size_wasm`, which turns on the
//! `optimize(size)` marks that are otherwise on for wasm only, so the
//! sessions cover them too (the 0.0.2 performance review, section 17);
//! the base's is built in its own tree ([`crate::shipped::refs_dir`]) with
//! its own toolchain. Counts are cached per worker binary, keyed by
//! valgrind, the host and the workload (the traces and `gate.py`): a head
//! whose worker is the base's byte for byte is counted once.
//!
//! What still moves a count is a race inside the worker: its serve thread,
//! woken by the reader, may meet the channel's lock still held (about 800
//! Ir a time, a few times a session; one binary's judged count differs by
//! up to 0.106% on a session between runs, the highest of five [M]). One
//! session holds 63.1% of all the counted instructions. So the gate judges
//! each session's ratio, head over base, and the median of the ratios, not
//! the total: a session past +0.5% ([`SESSION_LIMIT`]) or the median
//! past +0.3% ([`MEDIAN_LIMIT`]) fails unless a `Native-IR-Accepted:
//! sessions <ceiling> <reason>` line covers it, and so does a session
//! whose allocator calls rise past +0.5% ([`CALLS_LIMIT`]) unless a line
//! naming `sessions-calls` ([`CALLS_NAME`]) does.
//!
//! Each session has two floors ([`crate::floors`], in
//! `xtask/ir-floors/replay-ir.txt`): `sessions/<id>`, its count without
//! the allocator, and `sessions-calls/<id>`, its allocator calls, judged
//! against the head by the same limits as the base, so rises that land on
//! the landing tier, where this gate does not run, cannot add up between
//! two release-gate runs. The total is not floored: it follows the heap's
//! layout. A drop passes and leaves the file alone; a count read on one
//! worker moves by the race above, so `--write-floors` lowers a floor only
//! for a drop past [`band`]. An accepted median past its
//! limit against the floors raises the floor of every session that rose,
//! as an accepted geomean does in wasm-ir. Every session shares one key
//! (the counting, valgrind, the host, the workload and the fixed
//! `getrandom`): a change to any of them stales all of them, and the gate
//! then replays the floors' build too.

use std::{
    collections::BTreeMap,
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
    process::Command,
    thread,
};

use crate::{
    accept::{self, Accept, Failure},
    floors,
    native_ir::ACCEPTED,
    wasm_ir::{source_hash, version},
    wasm_wall::{checkout, git},
    workspace_root,
};

/// The gate's floor file, `xtask/ir-floors/replay-ir.txt`.
const GATE: &str = "replay-ir";
/// The workload's name in the verdict and in accept lines.
const NAME: &str = "sessions";
/// The most one session's count may rise, head over base.
const SESSION_LIMIT: f64 = 1.005;
/// The most the median of the sessions' ratios may rise.
const MEDIAN_LIMIT: f64 = 1.003;
/// The cfg that turns on the wasm-only `optimize(size)` marks natively.
const WASM_MARKS: &str = "--cfg mm_size_wasm";
/// What a count holds, in the cache key: change it when that changes.
const COUNTING: &str = "callgrind from start_thread, all counted: total, total without the \
                        allocator's entry points, allocator calls; replay_worker, wasm marks on, \
                        lockstep, fixed getrandom, 5";
/// glibc's allocator entry points. Their inclusive Ir is left out of the
/// judged count; the calls of the first [`CALLED`] are a session's
/// allocator calls. `free` is one of them: with it two workers that do the
/// same work differ by at most 0.031% of a session's calls, without it by
/// 0.057% [M].
const ALLOCATOR: [&str; 6] = [
    "malloc",
    "free",
    "realloc",
    "calloc",
    "posix_memalign",
    "__libc_thread_freeres",
];
/// How many of [`ALLOCATOR`] are the calls a session's allocations count.
const CALLED: usize = 5;
/// How many of [`ALLOCATOR`] every worker that ran calls, so a dump
/// without one of them is an error.
const REQUIRED: usize = 2;
/// The most one session's allocator calls may rise, head over base;
/// fewer calls always pass.
const CALLS_LIMIT: f64 = 1.005;
/// The allocator calls' name in the verdict and in accept lines: a line
/// accepts the number it names, so one written for a rise in calls leaves
/// the instructions judged.
const CALLS_NAME: &str = "sessions-calls";
/// A `getrandom` for the worker's `LD_PRELOAD`: fixed bytes, so std's
/// `HashMap` keys, the only randomness the replay worker draws itself (it
/// pins the cells' entropy), are the same in every run. With random keys a
/// map's drop order (mm-cell-worker's bindings, at each `Reset`) and so the
/// heap's layout differed run to run, by up to 0.13% of a session [M].
const FIXED_RANDOM: &str = "#include <stddef.h>
#include <sys/types.h>
ssize_t getrandom(void *buf, size_t len, unsigned int flags) {
    unsigned char *p = buf;
    (void)flags;
    for (size_t i = 0; i < len; i++) p[i] = (unsigned char)(i * 37 + 11);
    return (ssize_t)len;
}
";
/// The files that make the workload.
const WORKLOAD: [&str; 2] = [
    "conformance/codemode/real/traces.tar.xz",
    "conformance/codemode/real/gate.py",
];

/// What one session read, from callgrind's dump of its worker.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Count {
    /// The judged count: `total` without the inclusive Ir of [`ALLOCATOR`].
    ir: u64,
    /// Every instruction from `start_thread` on.
    total: u64,
    /// Calls of the first [`CALLED`] of [`ALLOCATOR`].
    calls: u64,
}

/// Each session's count, by its id with `/` as `.`.
type Counts = BTreeMap<String, Count>;

#[allow(
    clippy::too_many_lines,
    reason = "sides, builds, replays and the verdict in order"
)]
pub(crate) fn run(args: &[String]) -> Result<(), String> {
    let mut base_rev = "origin/main".to_owned();
    let mut write = false;
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--write-floors" => write = true,
            "--base" => base_rev = args.next().cloned().ok_or("--base needs a revision")?,
            other => return Err(format!("replay-ir: unknown option `{other}`")),
        }
    }
    if !cfg!(target_os = "linux") {
        return Err("replay-ir needs valgrind, which runs on Linux only".to_owned());
    }
    let valgrind = crate::wasm::tool("VALGRIND", "valgrind")
        .and_then(|v| absolute(&v))
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
        "{COUNTING}; {}; {}; workload {}; getrandom {}",
        version(&valgrind),
        floors::host(&valgrind),
        source_hash(&workload),
        source_hash(FIXED_RANDOM.as_bytes())
    );
    let base = git(&root, &["merge-base", &base_rev, "HEAD"])?;
    let tag = git(
        &root,
        &[
            "describe",
            "--exact-match",
            "--tags",
            "--match",
            "v*",
            &base,
        ],
    )
    .is_ok();
    println!(
        "replay-ir: base = {base_rev} ({}){}",
        short(&base),
        if tag {
            ", a tag: the floors are checked against it"
        } else {
            ""
        }
    );
    // Every session's floors share one key: the workload is one archive
    // and one driver. The names are the floor file's until the head has
    // counted (a stale floor is one the file holds).
    let key = floors::key(&method, "all sessions");
    let in_file = if tag {
        floors::here(&root, GATE)?
    } else {
        floors::at(&root, &base, GATE)?
    };
    let mut keys: BTreeMap<String, String> =
        in_file.keys().map(|n| (n.clone(), key.clone())).collect();
    let stale = floors::stale(&root, GATE, &base, tag, write, &keys)?;
    let base_tree = refs.join("base-src");
    checkout(&root, &base_tree, &base)?;
    // This run's own: another gate in the checkout uses another. A run
    // that stopped on an error left its dumps for a look; once its pid is
    // gone they go.
    let scratches = dir.join("callgrind");
    for entry in fs::read_dir(&scratches).into_iter().flatten().flatten() {
        let gone = entry
            .file_name()
            .to_str()
            .and_then(|n| n.parse::<u32>().ok())
            .is_some_and(|pid| !Path::new(&format!("/proc/{pid}")).exists());
        if gone {
            let _ = fs::remove_dir_all(entry.path());
        }
    }
    let scratch = scratches.join(std::process::id().to_string());
    let _ = fs::remove_dir_all(&scratch);
    let preload = fixed_random(&scratch)?;
    // (label, tree, target): the head, the base, then the build of each
    // stale floor, checked out beside the base.
    let mut trees = vec![
        (
            "head".to_owned(),
            root.clone(),
            root.join("target/replay-ir/build"),
        ),
        ("base".to_owned(), base_tree, refs.join("base")),
    ];
    for (i, (sha, _)) in stale.iter().enumerate() {
        let tree = refs.join(format!("floor-{i}-src"));
        checkout(&root, &tree, sha)?;
        trees.push((
            format!("floor {}", short(sha)),
            tree,
            refs.join(format!("floor-{i}")),
        ));
    }
    let mut workers = Vec::new();
    for (label, tree, target) in &trees {
        println!("replay-ir: building the {label}'s worker");
        let worker = build(tree, target, &host)?;
        let bytes = fs::read(&worker).map_err(|e| format!("{}: {e}", worker.display()))?;
        workers.push((source_hash(&bytes), worker));
    }
    // A count belongs to the worker binary: one the base shares with the
    // head, or one counted before, is not counted again.
    if workers[0].0 == workers[1].0 {
        println!("replay-ir: the head's worker is the base's, counted once");
    }
    let cache = |hash: &str| dir.join("cache").join(format!("{hash}.tsv"));
    let mut counts: BTreeMap<String, Counts> = BTreeMap::new();
    let mut todo: Vec<&(String, PathBuf)> = Vec::new();
    for side in &workers {
        if counts.contains_key(&side.0) || todo.iter().any(|t| t.0 == side.0) {
            continue;
        }
        match load(&cache(&side.0), &method) {
            Some(c) => {
                println!("replay-ir: {} is counted (cache)", side.1.display());
                counts.insert(side.0.clone(), c);
            }
            None => todo.push(side),
        }
    }
    // The replays run at once, sharing the cores: each ends with its
    // longest session alone (about 8.5 minutes under callgrind [M]).
    let jobs = thread::available_parallelism()
        .map_or(1, usize::from)
        .div_ceil(todo.len().max(1));
    for (_, worker) in &todo {
        canary(&valgrind, &preload, worker, &scratch)?;
    }
    let results: Vec<(&String, Result<Counts, String>)> = thread::scope(|scope| {
        let running: Vec<_> = todo
            .iter()
            .enumerate()
            .map(|(i, (hash, worker))| {
                let out = scratch.join(format!("{i:02}"));
                let (root, valgrind, preload) = (&root, &valgrind, &preload);
                let job = scope.spawn(move || replay(root, valgrind, preload, worker, jobs, &out));
                (hash, job)
            })
            .collect();
        running
            .into_iter()
            .map(|(hash, job)| {
                let result = job
                    .join()
                    .unwrap_or_else(|_| Err("a replay thread failed".to_owned()));
                (hash, result)
            })
            .collect()
    });
    let _ = fs::remove_dir_all(&scratch);
    for (hash, result) in results {
        let c = result?;
        save(&cache(hash), &method, &c)?;
        counts.insert(hash.clone(), c);
    }
    let sides = workers
        .iter()
        .map(|(hash, _)| counts.get(hash).map(named))
        .collect::<Option<Vec<floors::Counts>>>()
        .ok_or("replay-ir: a side has no counts")?;
    let (head, base_counts) = (&counts[&workers[0].0], &counts[&workers[1].0]);
    for name in sides[0].keys() {
        keys.insert(name.clone(), key.clone());
    }
    let accepts = accept::for_change(&root, ACCEPTED, &base, "MM_IR_ACCEPT")?;
    // A line naming `sessions` or `sessions-calls` covers each session's
    // floor of that number.
    let accepted = |name: &str, r: f64| {
        let number = name.split_once('/').map_or(name, |(n, _)| n);
        accepts
            .iter()
            .any(|a| a.names(None, number) && r <= a.ceiling())
    };
    let floor_sides = floors::Sides {
        base: &base,
        tag,
        head: &sides[0],
        base_counts: &sides[1],
        keys: &keys,
        head_build: &floors::build_id(&root, None)?,
        recounted: &floors::recounted(&stale, &sides[2..]),
    };
    let past = |name: &str, h: u64, f: u64| {
        if name.starts_with(CALLS_NAME) {
            calls_rose(h, f)
        } else {
            ratio(h, f) > SESSION_LIMIT
        }
    };
    // An accepted median past its limit against the floors raises every
    // session that rose, as wasm-ir's geomean does, so the next landing
    // does not fail on it again.
    let (at_floor, _) = floors::judge(
        &root,
        GATE,
        &floor_sides,
        &floors::Rules {
            accepted: &accepted,
            past: &past,
            swept: &|_| false,
            band,
        },
        false,
    )?;
    let prefix = format!("{NAME}/");
    let m = median(
        sides[0]
            .iter()
            .filter(|(name, _)| name.starts_with(&prefix))
            .filter_map(|(name, &h)| Some(ratio(h, *at_floor.get(name)?)))
            .collect(),
    );
    let swept = m > MEDIAN_LIMIT && accepted(NAME, m);
    let (against, errors) = floors::judge(
        &root,
        GATE,
        &floor_sides,
        &floors::Rules {
            accepted: &accepted,
            past: &past,
            swept: &|name| swept && name.starts_with(&prefix),
            band,
        },
        write,
    )?;
    let (mut failures, mut uncounted) =
        report(head, (&format!("base {}", short(&base)), base_counts), true);
    let (more, gone) = report(head, ("floor", &unnamed(&against)), false);
    failures.extend(more);
    for name in gone {
        if !uncounted.contains(&name) {
            uncounted.push(name);
        }
    }
    floors::verdict(judge(failures, &uncounted, &accepts), &errors)
}

/// A session's floors by name ([`crate::floors`]): `sessions/<id>`, the
/// count without the allocator, and `sessions-calls/<id>`, the allocator's
/// calls, of each session that ran to its end. The total is not floored:
/// it follows the heap's layout.
fn named(counts: &Counts) -> floors::Counts {
    let mut out = floors::Counts::new();
    for (id, c) in counts.iter().filter(|(_, c)| c.total > 0) {
        out.insert(format!("{NAME}/{id}"), c.ir);
        out.insert(format!("{CALLS_NAME}/{id}"), c.calls);
    }
    out
}

/// The floors as [`Counts`], for [`report`]: a session with both of its
/// floors, its total standing in for the count it is not floored on.
fn unnamed(floors: &floors::Counts) -> Counts {
    let mut out = Counts::new();
    for (name, &ir) in floors {
        let Some(id) = name.strip_prefix(&format!("{NAME}/")) else {
            continue;
        };
        if let Some(&calls) = floors.get(&format!("{CALLS_NAME}/{id}")) {
            out.insert(
                id.to_owned(),
                Count {
                    ir,
                    total: ir.max(1),
                    calls,
                },
            );
        }
    }
    out
}

/// How far a session's count of one worker reads below a floor `f` of it
/// and is still that count ([`floors::Rules::band`]): a race inside the
/// worker moves one binary's count without allocator by up to 0.106% of a
/// session between runs [M], so 0.15% for it; 0.05% for the allocator's
/// calls [E].
fn band(name: &str, f: u64) -> u64 {
    if name.starts_with(CALLS_NAME) {
        f / 2_000
    } else {
        f * 3 / 2_000
    }
}

/// The verdict on `failures` under `accepts`, one number at a time, so the
/// line a failure suggests names one number with its own ceiling; and a
/// failure for each session in `uncounted`, which no line covers.
fn judge(failures: Vec<Failure>, uncounted: &[String], accepts: &[Accept]) -> Result<(), String> {
    let (ir, calls): (Vec<Failure>, Vec<Failure>) =
        failures.into_iter().partition(|f| f.name == NAME);
    let verdict =
        |f: &[Failure]| accept::verdict("replay-ir", ACCEPTED, "MM_IR_ACCEPT", f, accepts);
    let mut errors: Vec<String> = if ir.is_empty() && calls.is_empty() {
        verdict(&[]).err().into_iter().collect()
    } else {
        [ir, calls]
            .iter()
            .filter(|f| !f.is_empty())
            .filter_map(|f| verdict(f).err())
            .collect()
    };
    if !uncounted.is_empty() {
        errors.push(format!(
            "replay-ir: diverged on the head and not on the base, so not counted: {} (see the \
             real-replay stage)",
            uncounted.join(", ")
        ));
    }
    if errors.is_empty() {
        return Ok(());
    }
    Err(errors.join("\n"))
}

/// `tool` as an absolute path, looked up on `PATH` if it is a bare name:
/// `gate.py` runs `worker.sh` with a `PATH` of its own, and the valgrind
/// that counts must be the one whose version is in the cache key.
fn absolute(tool: &Path) -> Option<PathBuf> {
    if tool.components().count() > 1 {
        return std::path::absolute(tool).ok();
    }
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|dir| dir.join(tool))
        .find(|path| path.is_file())
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

/// [`FIXED_RANDOM`] built with the C compiler (`$CC`, else `cc`) into
/// `dir`: the library the replays preload.
fn fixed_random(dir: &Path) -> Result<PathBuf, String> {
    fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let src = dir.join("fixed_random.c");
    let lib = dir.join("fixed_random.so");
    fs::write(&src, FIXED_RANDOM).map_err(|e| format!("{}: {e}", src.display()))?;
    let cc = std::env::var_os("CC").unwrap_or_else(|| "cc".into());
    let status = Command::new(&cc)
        .args(["-shared", "-fPIC", "-O2", "-o"])
        .arg(&lib)
        .arg(&src)
        .status()
        .map_err(|e| format!("replay-ir needs a C compiler ({}): {e}", cc.display()))?;
    if !status.success() {
        return Err(format!("{} failed on {}", cc.display(), src.display()));
    }
    Ok(lib)
}

/// Every session replayed by this checkout's `gate.py --lockstep` on
/// `worker` under callgrind with [`fixed_random`]'s `preload`, `jobs` at a
/// time, in `out`: each session's count. `gate.py`'s
/// verdict is the real-replay stage's: an older build may differ from the
/// recorded results, and its count is still what this workload costs it.
fn replay(
    root: &Path,
    valgrind: &Path,
    preload: &Path,
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
            "#!/bin/sh\nn=$(printf %s \"$MM_REPLAY_SESSION\" | tr / .)\nexport LD_PRELOAD='{}'\n\
             exec '{}' --tool=callgrind --log-file=\"{o}/$n.%p.log\" \
             --callgrind-out-file=\"{o}/$n.%p.cg\" \
             --toggle-collect=start_thread '--toggle-collect=pthread_getattr_np*' '{}'\n",
            preload.display(),
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
        .arg("--lockstep")
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
    // A session that diverged counts nothing, whether gate.py killed its
    // worker or the worker ended, exited or aborted first: what it ran is
    // not the session.
    let diverged: Vec<String> = stdout
        .lines()
        .filter_map(|l| l.strip_prefix("diverged "))
        .map(|sid| sid.replace('/', "."))
        .collect();
    for session in &diverged {
        println!(
            "replay-ir: {session} diverged on {}: counted as nothing",
            worker.display()
        );
    }
    // One log per replay; its dump, `<session>.<pid>.cg`, is written when
    // the worker exits and removed once read.
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
        if diverged.contains(&session) {
            counts.entry(session).or_default();
            continue;
        }
        let count = counts.entry(session).or_default();
        let log = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        if !log.contains("Collected : ") {
            return Err(format!(
                "{} left no totals, and gate.py did not find its session diverged",
                path.display()
            ));
        }
        let cg = out.join(format!("{stem}.cg"));
        let text = fs::read_to_string(&cg).map_err(|e| format!("{}: {e}", cg.display()))?;
        let c = dump(&text).map_err(|e| format!("{}: {e}", cg.display()))?;
        let _ = fs::remove_file(&cg);
        count.ir += c.ir;
        count.total += c.total;
        count.calls += c.calls;
    }
    if counts.values().all(|c| c.total == 0) {
        return Err(format!(
            "replay-ir: nothing collected on {}",
            worker.display()
        ));
    }
    Ok(counts)
}

/// Runs `worker` with an empty stdin under callgrind collecting from
/// `start_thread` on, and fails unless that collects something: without
/// glibc's debug symbols callgrind never sees the (local) `start_thread`,
/// and every session would read nothing.
fn canary(valgrind: &Path, preload: &Path, worker: &Path, dir: &Path) -> Result<(), String> {
    let log = dir.join("canary.log");
    let status = Command::new(valgrind)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("LD_PRELOAD", preload)
        .arg("--tool=callgrind")
        .arg(format!("--log-file={}", log.display()))
        .arg("--callgrind-out-file=/dev/null")
        .arg("--toggle-collect=start_thread")
        .arg(worker)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map_err(|e| format!("cannot run {}: {e}", valgrind.display()))?;
    let text = fs::read_to_string(&log).map_err(|e| format!("{}: {e}", log.display()))?;
    let n: u64 = text
        .lines()
        .filter_map(|l| l.split_once("Collected : "))
        .filter_map(|(_, n)| n.trim().parse::<u64>().ok())
        .sum();
    // The worker exits with an error on an empty stdin (no configure
    // frame); only what was collected before that matters.
    if n == 0 {
        return Err(format!(
            "replay-ir: {} under callgrind collected nothing from glibc's start_thread \
             ({status}): valgrind needs glibc's debug symbols (libc6-dbg), see {}",
            worker.display(),
            log.display()
        ));
    }
    Ok(())
}

/// One callgrind dump's three numbers ([`Count`]). The calls into each of
/// [`ALLOCATOR`] in glibc carry their inclusive Ir. A dump that ran
/// without glibc's `malloc` or `free` (an
/// allocator linked in, or interposed without forwarding to glibc's) and
/// a dump without its totals are errors, never zero; the other entry
/// points, called once a session or not at all, count 0 when missing.
fn dump(text: &str) -> Result<Count, String> {
    // Names are compressed: `(id) name` the first time, `(id)` after.
    fn name<'a>(table: &mut BTreeMap<&'a str, &'a str>, spec: &'a str) -> &'a str {
        let Some((id, rest)) = spec.split_once(')') else {
            return spec.trim();
        };
        let rest = rest.trim();
        if rest.is_empty() {
            return table.get(id).copied().unwrap_or_default();
        }
        table.insert(id, rest);
        rest
    }
    let (mut objects, mut functions) = (BTreeMap::new(), BTreeMap::new());
    let glibc = |object: &str| {
        Path::new(object)
            .file_name()
            .is_some_and(|f| f.to_string_lossy().starts_with("libc.so"))
    };
    let (mut ob, mut cob, mut cfn): (&str, Option<&str>, Option<&str>) = ("", None, None);
    let mut call: Option<u64> = None;
    let mut total = None;
    let (mut inclusive, mut calls, mut seen) = ([0u64; 6], [0u64; 6], [false; 6]);
    for line in text.lines() {
        // The line after `calls=` is the call's inclusive cost.
        if let Some(n) = call.take() {
            let cost = line
                .split_whitespace()
                .last()
                .and_then(|c| c.parse::<u64>().ok())
                .ok_or_else(|| format!("a call without its cost: `{line}`"))?;
            let object = cob.take().unwrap_or(ob);
            if let Some(i) = ALLOCATOR.iter().position(|f| Some(*f) == cfn)
                && glibc(object)
            {
                inclusive[i] += cost;
                calls[i] += n;
                seen[i] = true;
            }
            cfn = None;
        } else if let Some(spec) = line.strip_prefix("ob=") {
            ob = name(&mut objects, spec);
        } else if let Some(spec) = line.strip_prefix("cob=") {
            cob = Some(name(&mut objects, spec));
        } else if let Some(spec) = line.strip_prefix("cfn=") {
            cfn = Some(name(&mut functions, spec));
        } else if let Some(spec) = line.strip_prefix("fn=") {
            name(&mut functions, spec);
            cob = None;
        } else if let Some(n) = line.strip_prefix("calls=") {
            let n = n.split_whitespace().next().and_then(|n| n.parse().ok());
            call = Some(n.ok_or_else(|| format!("a malformed `{line}`"))?);
        } else if let Some(n) = line.strip_prefix("totals:") {
            total = Some(
                n.trim()
                    .parse::<u64>()
                    .map_err(|_| format!("a malformed `{line}`"))?,
            );
        }
    }
    let total = total.ok_or("a callgrind dump without its totals")?;
    if total > 0
        && let Some(i) = seen[..REQUIRED].iter().position(|s| !s)
    {
        return Err(format!(
            "glibc's `{}` is not in the dump: an allocator other than glibc's cannot be \
             left out of the count",
            ALLOCATOR[i]
        ));
    }
    let ir = total
        .checked_sub(inclusive.iter().sum())
        .ok_or("the allocator's inclusive Ir exceeds the total")?;
    Ok(Count {
        ir,
        total,
        calls: calls[..CALLED].iter().sum(),
    })
}

#[allow(clippy::cast_precision_loss, reason = "a ratio of counts")]
fn ratio(head: u64, other: u64) -> f64 {
    head as f64 / other.max(1) as f64
}

#[allow(clippy::cast_precision_loss, reason = "millions, printed")]
fn millions(n: u64) -> f64 {
    n as f64 / 1e6
}

/// One session's two judged numbers, head against base.
struct Row<'a> {
    name: &'a String,
    head: Count,
    base: Count,
    ratio: f64,
}

/// Prints the sessions against `other` (named `against`): per session the
/// total, the judged count and the allocator calls, the median ratio of
/// the judged count and the sessions that moved most; returns the sessions
/// and the median past their limits. Without `totals` (`other` the
/// floors, which hold no total) the total's ratios are not printed.
#[allow(
    clippy::too_many_lines,
    reason = "the table, the sessions that moved and the verdict on two numbers"
)]
fn report(
    head: &Counts,
    (against, other): (&str, &Counts),
    totals: bool,
) -> (Vec<Failure>, Vec<String>) {
    let total = |h: u64, o: u64| {
        if totals {
            format!("{:.4}", ratio(h, o))
        } else {
            "-".to_owned()
        }
    };
    // Sessions counted on both sides; one that diverged on a side counts
    // nothing there.
    let mut rows: Vec<Row<'_>> = head
        .iter()
        .filter_map(|(name, &h)| {
            let b = *other.get(name)?;
            (h.total > 0 && b.total > 0).then(|| Row {
                name,
                head: h,
                base: b,
                ratio: ratio(h.ir, b.ir),
            })
        })
        .collect();
    // (session, counted on the head, counted on the base)
    let names: std::collections::BTreeSet<&String> = head.keys().chain(other.keys()).collect();
    let unmatched: Vec<(&String, bool, bool)> = names
        .into_iter()
        .map(|k| {
            let counted = |c: &Counts| c.get(k).is_some_and(|c| c.total > 0);
            (k, counted(head), counted(other))
        })
        .filter(|(_, h, b)| !(*h && *b))
        .collect();
    rows.sort_by(|x, y| x.ratio.total_cmp(&y.ratio));
    let median = median(rows.iter().map(|r| r.ratio).collect());
    let sum = |f: fn(&Row<'_>) -> u64| rows.iter().map(f).sum::<u64>();
    let (hi, bi) = (sum(|r| r.head.ir), sum(|r| r.base.ir));
    let (ht, bt) = (sum(|r| r.head.total), sum(|r| r.base.total));
    let (hc, bc) = (sum(|r| r.head.calls), sum(|r| r.base.calls));
    println!(
        "\n| Sessions | {against}: Ir without the allocator (M) | head | head / it \
         | total Ir, head / it | allocator calls, head / it |"
    );
    println!("|---|---:|---:|---:|---:|---:|");
    println!(
        "| total of {} | {:.3} | {:.3} | {:.4} | {} | {:.4} |",
        rows.len(),
        millions(bi),
        millions(hi),
        ratio(hi, bi),
        total(ht, bt),
        ratio(hc, bc)
    );
    println!(
        "| median ratio | | | {median:.4} | | |\n(limits on Ir without the allocator: a session \
         +{:.1}%, the median +{:.1}%; on allocator calls: a session +{:.1}%)",
        (SESSION_LIMIT - 1.0) * 100.0,
        (MEDIAN_LIMIT - 1.0) * 100.0,
        (CALLS_LIMIT - 1.0) * 100.0
    );
    let mut text = String::from(
        "sessions that moved most, head / base: Ir without the allocator; total Ir; allocator calls",
    );
    for r in rows.iter().rev().take(5) {
        let _ = write!(
            text,
            "\n  {}: {:.3} -> {:.3} M Ir ({:.4}); total {}; calls {} -> {}",
            r.name,
            millions(r.base.ir),
            millions(r.head.ir),
            r.ratio,
            total(r.head.total, r.base.total),
            r.base.calls,
            r.head.calls
        );
    }
    println!("{text}");
    let mut by_calls: Vec<&Row<'_>> = rows.iter().collect();
    by_calls.sort_by_key(|r| core::cmp::Reverse(r.head.calls.saturating_sub(r.base.calls)));
    let rose = by_calls
        .first()
        .is_some_and(|r| r.head.calls > r.base.calls);
    let mut text = String::from(if rose {
        "sessions whose allocator calls rose most:"
    } else {
        "no session's allocator calls rose"
    });
    for r in by_calls
        .iter()
        .take(5)
        .filter(|r| r.head.calls > r.base.calls)
    {
        let _ = write!(
            text,
            "\n  {}: {} -> {} calls ({:.4})",
            r.name,
            r.base.calls,
            r.head.calls,
            ratio(r.head.calls, r.base.calls)
        );
    }
    println!("{text}");
    let mut uncounted = Vec::new();
    for &(name, h, b) in &unmatched {
        let side = match (h, b) {
            (true, false) => "on the head only",
            (false, true) => "on the base only",
            _ => "on neither side",
        };
        println!("replay-ir: {name} is not compared: it diverged, counted {side}");
        // Counted on the base and not on the head: no ceiling covers a
        // session the head did not finish.
        if b && !h {
            uncounted.push(name.clone());
        }
    }
    let mut failures = Vec::new();
    failures.extend(
        rows.iter()
            .filter(|r| r.ratio > SESSION_LIMIT)
            .map(|r| Failure {
                guest: None,
                name: NAME.to_owned(),
                ratio: r.ratio,
                text: format!(
                    "session {}: Ir without the allocator {:.4} against {against}",
                    r.name, r.ratio
                ),
            }),
    );
    if median > MEDIAN_LIMIT {
        failures.push(Failure {
            guest: None,
            name: NAME.to_owned(),
            ratio: median,
            text: format!(
                "the sessions' median ratio of Ir without the allocator {median:.4} against \
                 {against}"
            ),
        });
    }
    failures.extend(
        rows.iter()
            .filter(|r| calls_rose(r.head.calls, r.base.calls))
            .map(|r| {
                let q = ratio(r.head.calls, r.base.calls);
                Failure {
                    guest: None,
                    name: CALLS_NAME.to_owned(),
                    ratio: q,
                    text: format!(
                        "session {}: allocator calls {} -> {} ({q:.4}) against {against}",
                        r.name, r.base.calls, r.head.calls
                    ),
                }
            }),
    );
    (failures, uncounted)
}

/// The median of `ratios`, 1 of none.
fn median(mut ratios: Vec<f64>) -> f64 {
    ratios.sort_by(f64::total_cmp);
    match ratios.len() {
        0 => 1.0,
        n if n % 2 == 1 => ratios[n / 2],
        n => f64::midpoint(ratios[n / 2 - 1], ratios[n / 2]),
    }
}

/// Whether `head` allocator calls are past [`CALLS_LIMIT`] over `base`'s.
/// A compared session ran on both sides and so called the allocator; a
/// `base` of 0 would make any call on the head a failure, never a pass.
fn calls_rose(head: u64, base: u64) -> bool {
    head > base && (base == 0 || ratio(head, base) > CALLS_LIMIT)
}

/// The cached counts in `path` under `key`: every line a session and its
/// three numbers, closed by a line with their number, or none.
fn load(path: &Path, key: &str) -> Option<Counts> {
    let text = fs::read_to_string(path).ok()?;
    let mut lines = text.lines();
    if lines.next() != Some(&format!("# {key}")) {
        return None;
    }
    let mut counts = Counts::new();
    for line in lines {
        if let Some(n) = line.strip_prefix("# sessions ") {
            return (n.parse() == Ok(counts.len()) && !counts.is_empty()).then_some(counts);
        }
        let mut fields = line.split('\t');
        let name = fields.next()?;
        let mut n = || fields.next()?.parse::<u64>().ok();
        let count = Count {
            ir: n()?,
            total: n()?,
            calls: n()?,
        };
        if fields.next().is_some() || count.ir > count.total {
            return None;
        }
        counts.insert(name.to_owned(), count);
    }
    None
}

fn save(path: &Path, key: &str, counts: &Counts) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let mut text = format!("# {key}\n");
    for (k, c) in counts {
        let _ = writeln!(text, "{k}\t{}\t{}\t{}", c.ir, c.total, c.calls);
    }
    let _ = writeln!(text, "# sessions {}", counts.len());
    // Through a new name and a rename: a run stopped while writing leaves
    // no entry that reads as complete.
    let new = path.with_extension("tsv.new");
    fs::write(&new, text).map_err(|e| format!("{}: {e}", new.display()))?;
    fs::rename(&new, path).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, reason = "tests fail on a broken fixture")]

    use super::{
        CALLS_NAME, Count, Counts, NAME, band, calls_rose, dump, judge, named, report, unnamed,
    };
    use crate::{
        accept::{self, Failure},
        native_ir::ACCEPTED,
        wasm_wall::git,
    };

    /// A dump in callgrind's compressed form: `malloc` named once and
    /// referred to by id after, an allocator call from the worker's object
    /// counted only when its callee is glibc's.
    const DUMP: &str = "events: Ir
ob=(1) /w/replay_worker
fn=(1) main
0 100
cob=(2) /usr/lib/x86_64-linux-gnu/libc.so.6
cfn=(2) malloc
calls=3 0
0 40
cob=(2)
cfn=(2)
calls=2 0
0 20
cob=(2)
cfn=(3) free
calls=4 0
0 30
cob=(2)
cfn=(4) realloc
calls=1 0
0 10
cob=(2)
cfn=(5) calloc
calls=1 0
0 5
cob=(2)
cfn=(6) posix_memalign
calls=1 0
0 5
cob=(2)
cfn=(7) __libc_thread_freeres
calls=1 0
0 7
cob=(3) /w/other.so
cfn=(2)
calls=9 0
0 900
totals: 1117
";

    #[test]
    fn a_dump_reads_its_total_the_count_without_the_allocator_and_its_calls() {
        assert_eq!(
            dump(DUMP),
            Ok(Count {
                ir: 1117 - (40 + 20 + 30 + 10 + 5 + 5 + 7),
                total: 1117,
                calls: 3 + 2 + 4 + 1 + 1 + 1,
            })
        );
    }

    #[test]
    fn a_dump_missing_its_totals_malloc_or_free_is_an_error() {
        assert!(dump(&DUMP.replace("totals: 1117\n", "")).is_err());
        let interposed = DUMP.replace("cfn=(3) free", "cfn=(3) je_free");
        assert!(dump(&interposed).is_err_and(|e| e.contains("`free`")));
    }

    #[test]
    fn entry_points_called_once_a_session_may_be_missing() {
        // A worker that never calls posix_memalign, and one that exits
        // without its thread's cleanup.
        let text = DUMP
            .replace("cfn=(6) posix_memalign", "cfn=(6) other")
            .replace("cfn=(7) __libc_thread_freeres", "cfn=(7) other2");
        assert_eq!(
            dump(&text),
            Ok(Count {
                ir: 1117 - (40 + 20 + 30 + 10 + 5),
                total: 1117,
                calls: 3 + 2 + 4 + 1 + 1,
            })
        );
    }

    fn counts(rows: &[(&str, u64, u64)]) -> Counts {
        rows.iter()
            .map(|&(name, ir, calls)| {
                let count = Count {
                    ir,
                    total: ir + 100,
                    calls,
                };
                (name.to_owned(), count)
            })
            .collect()
    }

    /// Each finished session has two floors, which come back as one count
    /// to judge; a session that diverged has none. The band takes a
    /// session's measured race (0.106% [M]) and not a rise of 0.2%.
    #[test]
    fn a_session_s_floors_are_its_two_numbers() {
        let c = |ir, total, calls| Count { ir, total, calls };
        let head = Counts::from([
            ("a.x".to_owned(), c(1_000_000, 1_200_000, 900)),
            ("b.y".to_owned(), c(0, 0, 0)),
        ]);
        let floors = named(&head);
        assert_eq!(floors.len(), 2, "{floors:?}");
        assert_eq!(floors[&format!("{NAME}/a.x")], 1_000_000);
        assert_eq!(floors[&format!("{CALLS_NAME}/a.x")], 900);
        assert_eq!(unnamed(&floors)["a.x"].calls, 900);
        assert_eq!(unnamed(&floors)["a.x"].ir, 1_000_000);
        let f = 1_000_000;
        let ir = band(&format!("{NAME}/a.x"), f);
        assert!((1_060..2_000).contains(&ir), "{ir}");
        assert!(band(&format!("{CALLS_NAME}/a.x"), f) < ir);
    }

    #[test]
    fn a_session_the_head_did_not_finish_is_named_and_not_a_ratio() {
        let base = counts(&[("a", 1000, 100), ("b", 1000, 100)]);
        // gate.py found b diverged on the head: it counts nothing there.
        let mut head = counts(&[("a", 1000, 100)]);
        head.insert("b".to_owned(), Count::default());
        let (failures, uncounted) = report(&head, ("base", &base), true);
        assert!(failures.is_empty());
        assert_eq!(uncounted, ["b"]);
        // The other way round, the base's divergence is only reported.
        let (failures, uncounted) = report(&base, ("base", &head), true);
        assert!(failures.is_empty() && uncounted.is_empty());
    }

    #[test]
    fn instructions_and_calls_fail_under_their_own_names() {
        let base = counts(&[("a", 1000, 1000), ("b", 1000, 1000), ("c", 1000, 1000)]);
        let head = counts(&[("a", 1010, 1000), ("b", 1000, 1010), ("c", 1000, 1000)]);
        let (failures, _) = report(&head, ("base", &base), true);
        let names: Vec<(&str, bool)> = failures
            .iter()
            .map(|f| (f.name.as_str(), f.text.contains("session a")))
            .collect();
        assert_eq!(names, [(NAME, true), (CALLS_NAME, false)]);
    }

    #[test]
    fn allocator_calls_fail_past_half_a_percent_and_never_on_fewer() {
        assert!(!calls_rose(100_500, 100_000));
        assert!(calls_rose(100_501, 100_000));
        // No floor: a small session fails on a few calls.
        assert!(calls_rose(202, 200));
        assert!(!calls_rose(50, 100_000));
        assert!(calls_rose(1, 0));
        assert!(!calls_rose(0, 0));
    }

    #[test]
    fn each_number_takes_its_own_accept_line_and_suggests_its_own() {
        let failure = |name: &str, ratio: f64| Failure {
            guest: None,
            name: name.to_owned(),
            ratio,
            text: format!("{name} {ratio}"),
        };
        let none = judge(
            vec![failure(NAME, 1.0151), failure(CALLS_NAME, 1.2994)],
            &[],
            &[],
        )
        .unwrap_err();
        assert!(
            none.contains(&format!("`{ACCEPTED} sessions 1.016 <reason>`")),
            "{none}"
        );
        assert!(
            none.contains(&format!("`{ACCEPTED} sessions-calls 1.300 <reason>`")),
            "{none}"
        );
        // Two lines in one commit are both read, each for the number it names.
        let dir = std::env::temp_dir().join(format!("replay-ir-accept-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        git(&dir, &["init", "--quiet"]).unwrap();
        let commit = |message: &str| {
            let args = [
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t",
                "-c",
                "core.hooksPath=/dev/null",
                "commit",
                "--quiet",
                "--allow-empty",
                "-m",
                message,
            ];
            git(&dir, &args).unwrap();
        };
        commit("base");
        let base = git(&dir, &["rev-parse", "HEAD"]).unwrap();
        commit(&format!(
            "Two boxes a call\n\n{ACCEPTED} sessions 1.016 the boxes' own code\n\
             {ACCEPTED} sessions-calls 1.300 two boxes a call"
        ));
        let accepts = accept::for_change(&dir, ACCEPTED, base.trim(), "MM_REPLAY_IR_TEST").unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(
            judge(
                vec![failure(NAME, 1.0151), failure(CALLS_NAME, 1.2994)],
                &[],
                &accepts
            )
            .is_ok()
        );
        // The calls' ceiling does not cover the instructions.
        assert!(judge(vec![failure(NAME, 1.0201)], &[], &accepts).is_err());
        assert!(judge(vec![], &["s".to_owned()], &accepts).is_err());
    }
}
