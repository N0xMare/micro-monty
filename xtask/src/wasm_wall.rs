//! `cargo xtask wasm-wall`: wasm wall time of the bench programs against a
//! base revision, on wasmtime and on V8.
//!
//! Builds the working tree and `--base` (default `origin/main`, checked out
//! in a worktree outside this checkout, `shipped::refs_dir`, so it reads
//! none of this checkout's `.cargo/`) as shipped, each into its own target
//! directory: `mm` for `wasm32-wasip1`, precompiled for
//! wasmtime, and the mm-wasm guest for `wasm32-unknown-unknown`. Then,
//! in `--rounds` rounds, the two builds alternate first:
//!
//! - wasmtime: `mm run PROGRAM` per program, timed from spawn to exit
//!   (startup included), pinned to CPU 2, `REPS` times, the fastest kept;
//! - V8: `hosts/js/bench/wall.ts` under `node --no-liftoff` (TurboFan
//!   only, so steady state from the first run), each build's guest driven
//!   by its own tree's `hosts/js`, one warm-up and `REPS`
//!   timed runs per program in fresh sessions, the fastest kept, pinned to
//!   CPUs 2-5. `startup` is left out there: it runs in about 0.1 ms;
//! - `v8-bundle`: the same with the head's bundle guest
//!   (`mm_wasm_guest_bundle.wasm`), against the base's bundle guest, or its
//!   default guest when the base has none.
//!
//! Every run's output must equal its line in `bench/expected.txt`. Per
//! program and engine, the gate takes the median over rounds of the ratio
//! head / base in the same round; it fails when a program's median ratio
//! exceeds `PROGRAM_LIMIT` with its lower quartile above 1, or an engine's
//! geometric mean of the medians exceeds
//! `GEOMEAN_LIMIT` (calibrated in AGENTS.md: the same build against
//! itself). Faster never fails. Run it alone on an idle machine.
//!
//! Every run writes its rounds to `target/wasm-wall/rounds/` (one line per
//! engine, program and round: head ms, base ms), headed by the base commit
//! and the head (its commit, plus a hash of `git diff HEAD` when the tree is
//! dirty). `--pool FILE...` judges the rounds of several such runs together
//! with the same limits, without building or timing anything; the files
//! must share their base and head.

use std::{
    collections::BTreeMap,
    env,
    fmt::Write as _,
    fs,
    io::Write as _,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::Instant,
};

use serde_json::Value;

use crate::{
    bench::{PROGRAMS, read_expected},
    workspace_root,
};

/// Default rounds.
const ROUNDS: usize = 11;
/// Timed runs per program, build and round; the round keeps the fastest.
const REPS: usize = 3;
/// The highest median ratio head / base one program may have, unless its
/// lower quartile is at most 1 (a quarter of the rounds or more not slower:
/// noise, not a regression).
const PROGRAM_LIMIT: f64 = 1.05;
/// The highest geometric mean of an engine's median ratios.
const GEOMEAN_LIMIT: f64 = 1.03;

/// One build's two modules.
struct Build {
    /// The source tree it was built from, whose `hosts/js` drives its guest
    /// (a host speaks one `ABI_VERSION`, which a base may not share).
    src: PathBuf,
    /// `mm` for wasm32-wasip1, precompiled for wasmtime.
    mm: PathBuf,
    /// The mm-wasm guest, as shipped.
    guest: PathBuf,
    /// The bundle guest, when the tree has one (`host-bundle`).
    bundle: Option<PathBuf>,
}

/// Per engine and program, one `[head ms, base ms]` pair per round.
type Samples = BTreeMap<&'static str, BTreeMap<&'static str, Vec<[f64; 2]>>>;

pub(crate) fn run(args: &[String]) -> Result<(), String> {
    let mut base = "origin/main".to_owned();
    let mut rounds = ROUNDS;
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--pool" => return pool(&args.map(PathBuf::from).collect::<Vec<_>>()),
            "--base" => base.clone_from(args.next().ok_or("--base needs a revision")?),
            "--rounds" => {
                rounds = args
                    .next()
                    .and_then(|n| n.parse().ok())
                    .filter(|&n| n > 0)
                    .ok_or("--rounds needs a positive number")?;
            }
            other => return Err(format!("wasm-wall: unknown option `{other}`")),
        }
    }
    let root = workspace_root();
    let wasmtime = crate::wasm::tool("WASMTIME", "wasmtime")
        .ok_or("wasm-wall needs wasmtime ($WASMTIME, ~/.local/bin, PATH); run .agents/setup")?;
    let dir = root.join("target/wasm-wall");
    // The base outside this checkout, so it reads none of its `.cargo/`.
    let refs = crate::shipped::refs_dir("wasm-wall");
    let tree = refs.join("base-src");
    let sha = checkout(&root, &tree, &base)?;
    println!(
        "wasm-wall: head = working tree, base = {base} ({})",
        &sha[..sha.len().min(10)]
    );
    let builds = [
        build(&root, &dir.join("head"), &wasmtime)?,
        build(&tree, &refs.join("base"), &wasmtime)?,
    ];
    let samples = measure(&root, &wasmtime, &builds, rounds)?;
    let header = format!("# base {sha}\n# head {}\n", head_id(&root)?);
    let saved = save(&dir.join("rounds"), &header, &samples)?;
    println!("wasm-wall: rounds written to {}", saved.display());
    let failures = report(&samples);
    if failures.is_empty() {
        println!("\nwasm-wall: within the limits ({rounds} rounds)");
        Ok(())
    } else {
        Err(format!(
            "wasm-wall: slower than {base}: {}",
            failures.join("; ")
        ))
    }
}

/// `rev` checked out (detached, forced) in the worktree `tree`; its full
/// commit.
pub(crate) fn checkout(root: &Path, tree: &Path, rev: &str) -> Result<String, String> {
    let sha = git(
        root,
        &["rev-parse", "--verify", &format!("{rev}^{{commit}}")],
    )?;
    if tree.join(".git").exists() {
        git(tree, &["checkout", "--quiet", "--detach", "--force", &sha])?;
    } else {
        let path = tree.to_string_lossy().into_owned();
        git(
            root,
            &[
                "worktree", "add", "--quiet", "--detach", "--force", &path, &sha,
            ],
        )?;
    }
    Ok(sha)
}

/// `rounds` rounds of every program on both engines, the builds alternating first.
fn measure(
    root: &Path,
    wasmtime: &Path,
    builds: &[Build; 2],
    rounds: usize,
) -> Result<Samples, String> {
    let node = env::var_os("NODE").map_or_else(|| PathBuf::from("node"), PathBuf::from);
    let bench = root.join("bench");
    let expected = read_expected(&bench.join("expected.txt"))?;
    let pinnable = thread::available_parallelism().map_or(1, usize::from) >= 6
        && Command::new("taskset")
            .args(["-c", "0", "true"])
            .status()
            .is_ok_and(|status| status.success());
    let pin = |cpus: &str| -> Vec<String> {
        if pinnable {
            vec!["taskset".to_owned(), "-c".to_owned(), cpus.to_owned()]
        } else {
            Vec::new()
        }
    };
    let (pin_wasi, pin_v8) = (pin("2"), pin("2-5"));
    let mut samples = Samples::new();
    let started = Instant::now();
    for round in 0..rounds {
        let order: [usize; 2] = if round % 2 == 0 { [0, 1] } else { [1, 0] };
        for (name, stem) in PROGRAMS {
            let want = expected
                .get(name)
                .ok_or_else(|| format!("bench/expected.txt has no line for {name}"))?;
            let mut pair = [0.0; 2];
            for which in order {
                pair[which] = f64::INFINITY;
                for _ in 0..REPS {
                    let ms = wasi(&pin_wasi, wasmtime, &builds[which].mm, &bench, stem, want)?;
                    pair[which] = pair[which].min(ms);
                }
            }
            samples
                .entry("wasmtime")
                .or_default()
                .entry(name)
                .or_default()
                .push(pair);
        }
        for engine in ["v8", "v8-bundle"] {
            let bundled = engine == "v8-bundle";
            if bundled && builds[0].bundle.is_none() {
                continue;
            }
            let guest = |b: &Build| -> PathBuf {
                match &b.bundle {
                    Some(g) if bundled => g.clone(),
                    _ => b.guest.clone(),
                }
            };
            let mut times: [BTreeMap<String, f64>; 2] = Default::default();
            for which in order {
                times[which] = v8(
                    &pin_v8,
                    &node,
                    &builds[which].src,
                    &guest(&builds[which]),
                    &bench,
                    &expected,
                )?;
            }
            for (name, stem) in &PROGRAMS[1..] {
                let key = bench
                    .join(format!("{stem}.py"))
                    .to_string_lossy()
                    .into_owned();
                let at = |m: &BTreeMap<String, f64>| {
                    m.get(&key)
                        .copied()
                        .ok_or(format!("V8 run has no time for {name}"))
                };
                let pair = [at(&times[0])?, at(&times[1])?];
                samples
                    .entry(engine)
                    .or_default()
                    .entry(name)
                    .or_default()
                    .push(pair);
            }
        }
        eprintln!(
            "wasm-wall: round {} of {rounds} ({:.0} s)",
            round + 1,
            started.elapsed().as_secs_f64()
        );
    }
    Ok(samples)
}

/// Prints the table per engine; returns the limits exceeded.
fn report(samples: &Samples) -> Vec<String> {
    let mut failures = Vec::new();
    for (engine, programs) in samples {
        println!(
            "\n{engine}: median ms (base, head), median ratio head / base [interquartile range]"
        );
        let mut product = 1.0_f64;
        let mut count = 0_i32;
        for (name, _) in PROGRAMS {
            let Some(pairs) = programs.get(name) else {
                continue;
            };
            let ratios: Vec<f64> = pairs.iter().map(|[head, base]| head / base).collect();
            let (low, ratio) = (quartile(&ratios, 1), quartile(&ratios, 2));
            let column = |i: usize| pairs.iter().map(|pair| pair[i]).collect::<Vec<f64>>();
            println!(
                "  {name:<11} {:>8.1} {:>8.1}  {ratio:.3} [{:.3}-{:.3}]",
                quartile(&column(1), 2),
                quartile(&column(0), 2),
                low,
                quartile(&ratios, 3)
            );
            product *= ratio;
            count += 1;
            if ratio > PROGRAM_LIMIT && low > 1.0 {
                failures.push(format!("{engine} {name} {ratio:.3} > {PROGRAM_LIMIT}"));
            }
        }
        let geomean = product.powf(1.0 / f64::from(count.max(1)));
        println!("  geomean     {geomean:.3}");
        if geomean > GEOMEAN_LIMIT {
            failures.push(format!("{engine} geomean {geomean:.3} > {GEOMEAN_LIMIT}"));
        }
    }
    failures
}

/// The engines `measure` times, as `Samples` keys.
const ENGINES: [&str; 3] = ["wasmtime", "v8", "v8-bundle"];

/// The head's identity for a rounds file: its commit, plus a hash of
/// `git diff HEAD` when the tree has uncommitted changes.
pub(crate) fn head_id(root: &Path) -> Result<String, String> {
    let head = git(root, &["rev-parse", "HEAD"])?;
    if git(root, &["status", "--porcelain", "--untracked-files=no"])?.is_empty() {
        return Ok(head);
    }
    let diff = Command::new("git")
        .args(["diff", "HEAD"])
        .current_dir(root)
        .output()
        .map_err(|error| format!("cannot run git: {error}"))?;
    let mut hash = Command::new("git")
        .args(["hash-object", "--stdin"])
        .current_dir(root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|error| format!("cannot run git: {error}"))?;
    hash.stdin
        .take()
        .ok_or("git hash-object has no stdin")?
        .write_all(&diff.stdout)
        .map_err(|error| format!("git hash-object: {error}"))?;
    let out = hash
        .wait_with_output()
        .map_err(|error| format!("git hash-object: {error}"))?;
    let id = String::from_utf8_lossy(&out.stdout);
    Ok(format!("{head}+{}", id.trim().get(..12).unwrap_or("")))
}

/// Writes `samples` under `dir` as a new rounds file; its path.
fn save(dir: &Path, header: &str, samples: &Samples) -> Result<PathBuf, String> {
    fs::create_dir_all(dir).map_err(|error| format!("{}: {error}", dir.display()))?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let path = dir.join(format!("{stamp}.tsv"));
    let mut text = header.to_owned();
    for (engine, programs) in samples {
        for (name, pairs) in programs {
            for [head, base] in pairs {
                let _ = writeln!(text, "{engine}\t{name}\t{head}\t{base}");
            }
        }
    }
    fs::write(&path, text).map_err(|error| format!("{}: {error}", path.display()))?;
    Ok(path)
}

/// `--pool`: the rounds of several runs' files judged together.
fn pool(files: &[PathBuf]) -> Result<(), String> {
    if files.is_empty() {
        return Err("--pool needs rounds files (target/wasm-wall/rounds/*.tsv)".to_owned());
    }
    let mut header: Option<String> = None;
    let mut samples = Samples::new();
    let mut seen = Vec::new();
    for file in files {
        let real =
            fs::canonicalize(file).map_err(|error| format!("{}: {error}", file.display()))?;
        if seen.contains(&real) {
            return Err(format!("{}: given twice", file.display()));
        }
        seen.push(real);
        let text =
            fs::read_to_string(file).map_err(|error| format!("{}: {error}", file.display()))?;
        let head = text
            .lines()
            .take_while(|l| l.starts_with('#'))
            .collect::<Vec<_>>()
            .join("\n");
        match &header {
            None => header = Some(head),
            Some(first) if *first != head => {
                return Err(format!(
                    "{}: another base or head than {}",
                    file.display(),
                    files[0].display()
                ));
            }
            Some(_) => {}
        }
        for line in text
            .lines()
            .filter(|l| !l.starts_with('#') && !l.is_empty())
        {
            let bad = || format!("{}: bad line {line:?}", file.display());
            let mut f = line.split('\t');
            let (Some(engine), Some(name), Some(head), Some(base), None) =
                (f.next(), f.next(), f.next(), f.next(), f.next())
            else {
                return Err(bad());
            };
            let engine = ENGINES.iter().find(|e| **e == engine).ok_or_else(bad)?;
            let name = PROGRAMS.iter().find(|(n, _)| *n == name).ok_or_else(bad)?.0;
            let pair = [
                head.parse().map_err(|_| bad())?,
                base.parse().map_err(|_| bad())?,
            ];
            samples
                .entry(engine)
                .or_default()
                .entry(name)
                .or_default()
                .push(pair);
        }
    }
    let rounds = samples
        .get("wasmtime")
        .and_then(|p| p.values().next())
        .map_or(0, Vec::len);
    println!(
        "wasm-wall: {} runs pooled, {rounds} rounds\n{}",
        files.len(),
        header.unwrap_or_default()
    );
    let failures = report(&samples);
    if failures.is_empty() {
        println!("\nwasm-wall: within the limits ({rounds} rounds pooled)");
        Ok(())
    } else {
        Err(format!(
            "wasm-wall: pooled, slower: {}",
            failures.join("; ")
        ))
    }
}

/// Builds `src`'s wasip1 `mm` and guest as shipped into `target`.
fn build(src: &Path, target: &Path, wasmtime: &Path) -> Result<Build, String> {
    let cwasm = build_mm(src, target, wasmtime, None)?;
    let target_dir = target.to_string_lossy().into_owned();
    let status = crate::shipped_build("wasm32-unknown-unknown")
        .current_dir(src)
        .args([
            "--quiet",
            "--package",
            "mm-wasm-guest",
            "--target-dir",
            &target_dir,
        ])
        .status()
        .map_err(|error| format!("cannot run cargo: {error}"))?;
    if !status.success() {
        return Err(format!(
            "shipped build of mm-wasm-guest for wasm32-unknown-unknown in {} failed",
            src.display()
        ));
    }
    let guest =
        crate::wasm::optimize(&target.join("wasm32-unknown-unknown/release/mm_wasm_guest.wasm"))?;
    let manifest = src.join("crates/mm-wasm-guest/Cargo.toml");
    let bundle = if fs::read_to_string(&manifest).is_ok_and(|m| m.contains("host-bundle")) {
        Some(crate::wasm::bundle_guest_in(
            src,
            &target.join("bundle"),
            "wasm32-unknown-unknown",
        )?)
    } else {
        None
    };
    Ok(Build {
        src: src.to_path_buf(),
        mm: cwasm,
        guest,
        bundle,
    })
}

/// `src`'s wasip1 `mm` built as shipped into `target` and precompiled for
/// wasmtime: for this host's CPU, or for `isa` (a target triple, so for its
/// baseline features); the `.cwasm` path.
pub(crate) fn build_mm(
    src: &Path,
    target: &Path,
    wasmtime: &Path,
    isa: Option<&str>,
) -> Result<PathBuf, String> {
    let target_dir = target.to_string_lossy().into_owned();
    let status = crate::shipped_build("wasm32-wasip1")
        .current_dir(src)
        .args([
            "--quiet",
            "--package",
            "mm-cli",
            "--target-dir",
            &target_dir,
        ])
        .status()
        .map_err(|error| format!("cannot run cargo: {error}"))?;
    if !status.success() {
        return Err(format!(
            "shipped build of mm-cli for wasm32-wasip1 in {} failed",
            src.display()
        ));
    }
    let mm = crate::wasm::optimize(&target.join("wasm32-wasip1/release/mm.wasm"))?;
    let cwasm = mm.with_extension("cwasm");
    let mut command = Command::new(wasmtime);
    command.arg("compile");
    if let Some(isa) = isa {
        command.args(["--target", isa]);
    }
    let status = command
        .arg(&mm)
        .arg("-o")
        .arg(&cwasm)
        .status()
        .map_err(|error| format!("cannot run {}: {error}", wasmtime.display()))?;
    if !status.success() {
        return Err(format!("wasmtime compile of {} failed", mm.display()));
    }
    Ok(cwasm)
}

/// One `mm run bench/STEM.py` under wasmtime; its wall time in ms.
fn wasi(
    pin: &[String],
    wasmtime: &Path,
    mm: &Path,
    bench: &Path,
    stem: &str,
    want: &str,
) -> Result<f64, String> {
    let mut command = command(pin, wasmtime);
    command
        .args(["run", "--allow-precompiled", "--dir"])
        .arg(format!("{}::/b", bench.display()))
        .arg(mm)
        .args(["run", &format!("/b/{stem}.py")])
        .stdin(Stdio::null());
    let started = Instant::now();
    let output = command
        .output()
        .map_err(|error| format!("cannot run {}: {error}", wasmtime.display()))?;
    let ms = started.elapsed().as_secs_f64() * 1000.0;
    let stdout = String::from_utf8_lossy(&output.stdout);
    if !output.status.success() || stdout.trim() != want {
        return Err(format!(
            "{stem}.py on {} printed {:?} (want {want:?}): {}",
            mm.display(),
            stdout.trim(),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(ms)
}

/// Every program but `startup` on `guest` under V8, driven by the
/// `hosts/js` of `src`, the tree that built it; ms per program path.
fn v8(
    pin: &[String],
    node: &Path,
    src: &Path,
    guest: &Path,
    bench: &Path,
    expected: &BTreeMap<String, String>,
) -> Result<BTreeMap<String, f64>, String> {
    let mut command = command(pin, node);
    command
        .args(["--no-liftoff", "bench/wall.ts"])
        .arg(guest)
        .arg(REPS.to_string());
    for (name, stem) in &PROGRAMS[1..] {
        let want = expected.get(*name).map_or("", String::as_str);
        command.arg(format!(
            "{}={want}",
            bench.join(format!("{stem}.py")).display()
        ));
    }
    let output = command
        .current_dir(src.join("hosts/js"))
        .stdin(Stdio::null())
        .output()
        .map_err(|error| format!("cannot run {}: {error}", node.display()))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: Option<BTreeMap<String, f64>> = serde_json::from_str::<Value>(stdout.trim())
        .ok()
        .and_then(|v| {
            v.as_object().map(|o| {
                o.iter()
                    .filter_map(|(k, v)| Some((k.clone(), v.as_f64()?)))
                    .collect()
            })
        });
    match parsed {
        Some(times) if output.status.success() => Ok(times),
        _ => Err(format!(
            "bench/wall.ts on {} failed: {}",
            guest.display(),
            String::from_utf8_lossy(&output.stderr)
        )),
    }
}

fn command(pin: &[String], program: &Path) -> Command {
    match pin {
        [taskset, rest @ ..] => {
            let mut command = Command::new(taskset);
            command.args(rest).arg(program);
            command
        }
        [] => Command::new(program),
    }
}

/// Quartile `k` (1, 2 = median, 3) of `xs`, linearly interpolated.
fn quartile(xs: &[f64], k: usize) -> f64 {
    let mut xs = xs.to_vec();
    xs.sort_by(f64::total_cmp);
    let at = xs.len().saturating_sub(1) * k;
    let (lo, hi) = (at / 4, at.div_ceil(4));
    let frac = u8::try_from(at % 4).map_or(0.0, f64::from) / 4.0;
    match (xs.get(lo), xs.get(hi)) {
        (Some(a), Some(b)) => a + (b - a) * frac,
        _ => f64::NAN,
    }
}

pub(crate) fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .map_err(|error| format!("cannot run git: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}
