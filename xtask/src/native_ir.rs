//! `cargo xtask native-ir`: the bench programs' gated instruction counts on
//! the shipped native `mm` against the base and against each count's
//! floor (AGENTS.md's Ir rule).
//!
//! Counts as `bench/callgrind.py` counts first: callgrind Ir from glibc's
//! `__libc_start_main` onward, without `pthread_getattr_np`, but run from a
//! fixed place with an empty environment ([`count`]), so a count is the
//! same in every run and on every machine of one class. The head is `$MM`
//! (`cargo xtask check` passes the `shipped-mm` stage's binary) or this
//! tree's `mm` built as shipped. The base is the merge base with `--base
//! REV` (default `origin/main`), checked out outside this checkout
//! ([`crate::shipped::refs_dir`]) and built as shipped with its own
//! toolchain and, when its xtask has `shipped-mm`, its own recipe (else
//! this checkout's); its counts are cached per commit, keyed by that
//! recipe and toolchain, the counting method, valgrind, the host and each
//! program's source.
//!
//! The floors are `xtask/ir-floors/native-ir.txt` ([`crate::floors`]): no
//! history is built. A drop passes, and lowers the floor in the same
//! change. Against the base and against the floor, a program rising more
//! than 0.5%, or `startup` (about 0.58M) by more than 300 ([`LIMITS`])
//! instructions, fails unless an accept line covers it
//! ([`crate::accept`]): `Native-IR-Accepted: <programs> <ceiling>
//! <reason>` in a commit since the base passes those programs up to the
//! ceiling and raises their floors to the head's counts. With `--base` at
//! a `v*` tag (`just pretag`) the head is compared with the tag, and the
//! floors are checked against both.

use std::{
    collections::BTreeMap,
    env,
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        OnceLock,
        atomic::{AtomicU32, Ordering},
    },
};

use crate::{
    accept::{self, Failure, Limits},
    bench::{PROGRAMS, read_expected},
    floors, size,
    wasm_ir::{parallel, source_hash, version},
    wasm_wall::{checkout, git},
    workspace_root,
};

/// The limits: a program +0.5%; `startup` 300 instructions, since startup
/// drift is not accepted (owner) and [`count`] makes the count exact, so
/// an intended rise takes a `startup` accept line (size review of A+B).
pub(crate) const LIMITS: Limits = Limits {
    program: 1.005,
    startup: 300,
};
/// The line that accepts a rise ([`crate::accept`]).
pub(crate) const ACCEPTED: &str = "Native-IR-Accepted:";
/// What a count holds, in the cache key: change it when that changes.
const COUNTING: &str = "gated callgrind Ir, shipped native mm, empty environment, 4";
/// callgrind's collection toggles of the gated count.
const GATED: [&str; 2] = [
    "--toggle-collect=__libc_start_main*",
    "--toggle-collect=pthread_getattr_np*",
];
/// The line by which a revision's xtask builds the shipped `mm` itself.
const OWN_MM: &str = "Some(\"shipped-mm\")";

type Counts = BTreeMap<String, u64>;

#[allow(
    clippy::too_many_lines,
    reason = "sides, builds, counts and the verdict in order"
)]
pub(crate) fn run(args: &[String]) -> Result<(), String> {
    let mut base_rev = "origin/main".to_owned();
    let mut write = false;
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--plain" => return plain(),
            "--write-floors" => write = true,
            "--base" => base_rev = args.next().cloned().ok_or("--base needs a revision")?,
            other => return Err(format!("native-ir: unknown option `{other}`")),
        }
    }
    if !cfg!(target_os = "linux") {
        return Err("native-ir needs valgrind, which runs on Linux only".to_owned());
    }
    let valgrind = crate::wasm::tool("VALGRIND", "valgrind")
        .ok_or("native-ir needs valgrind ($VALGRIND, PATH); run .agents/setup")?;
    let root = workspace_root();
    let host = crate::host_target()?;
    let bench = root.join("bench");
    let expected = read_expected(&bench.join("expected.txt"))?;
    let method = format!(
        "{COUNTING}; {}; {}",
        version(&valgrind),
        floors::host(&valgrind)
    );
    let mut hashes = BTreeMap::new();
    let mut keys = BTreeMap::new();
    for (name, stem) in PROGRAMS {
        let path = bench.join(format!("{stem}.py"));
        let bytes = fs::read(&path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        let hash = source_hash(&bytes);
        keys.insert(name.to_owned(), floors::key(&method, &hash));
        hashes.insert(name.to_owned(), hash);
    }
    let dir = root.join("target/native-ir");
    let refs = crate::shipped::refs_dir("native-ir");
    let short = |s: &str| s[..s.len().min(10)].to_owned();
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
        "native-ir: base = {base_rev} ({}){}",
        short(&base),
        if tag {
            ", a tag: the floors are checked against it"
        } else {
            ""
        }
    );
    // (label, commit): the base, then the build of each stale floor.
    let stale = floors::stale(&root, "native-ir", &base, tag, &keys)?;
    let mut sides = vec![("base".to_owned(), base.clone())];
    sides.extend(
        stale
            .iter()
            .map(|(sha, _)| (format!("floor-{}", short(sha)), sha.clone())),
    );
    let head_mm = match env::var_os("MM").filter(|v| !v.is_empty()) {
        Some(mm) => PathBuf::from(mm),
        None => crate::shipped_mm_binary()?,
    };
    let mut counts: Vec<Counts> = vec![Counts::new()];
    let mut jobs: Vec<(usize, &str, &str, PathBuf)> = PROGRAMS
        .iter()
        .map(|&(name, stem)| (0, name, stem, head_mm.clone()))
        .collect();
    let mut cache_keys = Vec::new();
    fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for (i, (label, sha)) in sides.iter().enumerate() {
        let tree = refs.join(if i == 0 { "base-src" } else { "floor-src" });
        checkout(&root, &tree, sha)?;
        let own = git(
            &root,
            &[
                "grep",
                "-q",
                "-F",
                "-e",
                OWN_MM,
                sha,
                "--",
                "xtask/src/main.rs",
            ],
        )
        .is_ok();
        let recipe = size::recipe_key(if own { &tree } else { &root }, &tree)?;
        let key = format!("{method}; {recipe}");
        let cached = load(&dir.join("cache").join(format!("{sha}.tsv")), &key, &hashes);
        if cached.len() < PROGRAMS.len() {
            let how = if own {
                "its own recipe"
            } else {
                "this checkout's recipe"
            };
            println!(
                "native-ir: building the {label}, {} ({how}, its own toolchain)",
                short(sha)
            );
            let built = if own {
                build_itself(&tree)?
            } else {
                build(
                    &tree,
                    &refs.join(if i == 0 { "base" } else { "floor" }),
                    &host,
                )?
            };
            // Sides share a tree and a target directory: each keeps its mm.
            let mm = dir.join(format!("{label}.mm"));
            fs::copy(&built, &mm).map_err(|e| format!("{}: {e}", mm.display()))?;
            jobs.extend(
                PROGRAMS
                    .iter()
                    .filter(|(name, _)| !cached.contains_key(*name))
                    .map(|&(name, stem)| (i + 1, name, stem, mm.clone())),
            );
        }
        counts.push(cached);
        cache_keys.push(key);
    }
    let results = parallel(jobs.len(), |j| {
        let (_, name, stem, ref mm) = jobs[j];
        let want = expected.get(name).map_or("", String::as_str);
        count(&valgrind, mm, &bench, stem, want)
    });
    let _ = fs::remove_dir_all(scratch_root());
    for ((i, name, _, _), result) in jobs.iter().zip(results) {
        counts[*i].insert((*name).to_owned(), result?);
    }
    for ((_, sha), (key, c)) in sides.iter().zip(cache_keys.iter().zip(&counts[1..])) {
        save(
            &dir.join("cache").join(format!("{sha}.tsv")),
            key,
            &hashes,
            c,
        )?;
    }
    let mut failures = report(&counts[0], (&format!("base {}", short(&base)), &counts[1]));
    let recounted = floors::recounted(&stale, &counts[2..]);
    let head_build = floors::build_id(&root, None)?;
    let accepts = accept::for_change(&root, ACCEPTED, &base, "MM_IR_ACCEPT")?;
    let raised = |name: &str, ratio: f64| {
        accepts
            .iter()
            .any(|a| a.names(None, name) && ratio <= a.ceiling())
    };
    let (against, errors) = floors::judge(
        &root,
        "native-ir",
        &floors::Sides {
            base: &base,
            tag,
            head: &counts[0],
            base_counts: &counts[1],
            keys: &keys,
            head_build: &head_build,
            recounted: &recounted,
        },
        &raised,
        write,
    )?;
    failures.extend(report(&counts[0], ("floor", &against)));
    floors::verdict(
        accept::verdict("native-ir", ACCEPTED, "MM_IR_ACCEPT", &failures, &accepts),
        &errors,
    )
}

/// `native-ir --plain`, advisory: this tree's `mm` built as shipped beside
/// a plain `cargo build --release`, as crates.io users build it (owner,
/// 2026-11-13), program by program. It never fails on the counts.
fn plain() -> Result<(), String> {
    let valgrind = crate::wasm::tool("VALGRIND", "valgrind")
        .ok_or("native-ir needs valgrind ($VALGRIND, PATH); run .agents/setup")?;
    let root = workspace_root();
    let bench = root.join("bench");
    let expected = read_expected(&bench.join("expected.txt"))?;
    let shipped = crate::shipped_mm_binary()?;
    let dir = root.join("target/native-ir");
    let target = dir.join("plain");
    let status = Command::new(env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned()))
        .current_dir(&root)
        .args([
            "build",
            "--release",
            "--locked",
            "--quiet",
            "--package",
            "mm-cli",
            "--bin",
            "mm",
        ])
        .arg("--target-dir")
        .arg(&target)
        .env_remove("RUSTFLAGS")
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .status()
        .map_err(|e| format!("cannot run cargo: {e}"))?;
    if !status.success() {
        return Err("plain release build of mm failed".to_owned());
    }
    let plain = target.join("release/mm");
    let results = parallel(PROGRAMS.len() * 2, |j| {
        let (name, stem) = PROGRAMS[j / 2];
        let mm = if j % 2 == 0 { &shipped } else { &plain };
        let want = expected.get(name).map_or("", String::as_str);
        count(&valgrind, mm, &bench, stem, want)
    });
    let _ = fs::remove_dir_all(scratch_root());
    println!("\n| Program | shipped (M Ir) | plain (M Ir) | plain / shipped |");
    println!("|---|---:|---:|---:|");
    for (i, &(name, _)) in PROGRAMS.iter().enumerate() {
        let (s, p) = (results[2 * i].clone()?, results[2 * i + 1].clone()?);
        #[allow(clippy::cast_precision_loss, reason = "a ratio and millions, printed")]
        let (r, sm, pm) = (p as f64 / s.max(1) as f64, s as f64 / 1e6, p as f64 / 1e6);
        println!("| {name} | {sm:.3} | {pm:.3} | {r:.4} |");
    }
    println!("(advisory: the published natives are built as shipped)");
    Ok(())
}

/// The tree at `tree` builds its shipped `mm` with its own xtask
/// (`cargo xtask shipped-mm`, which prints its path) and toolchain.
fn build_itself(tree: &Path) -> Result<PathBuf, String> {
    let output = crate::shipped::tree_cargo(tree)
        .args(["xtask", "shipped-mm"])
        .env_remove("RUSTFLAGS")
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .env_remove("MM")
        .stderr(Stdio::inherit())
        .output()
        .map_err(|e| format!("cannot run cargo: {e}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    match stdout.lines().last() {
        Some(path) if output.status.success() => Ok(PathBuf::from(path.trim())),
        _ => Err(format!(
            "`cargo xtask shipped-mm` failed in {}",
            tree.display()
        )),
    }
}

/// Builds `mm` of the tree at `src` as shipped for `host` into `target`,
/// with this checkout's recipe and the tree's toolchain.
fn build(src: &Path, target: &Path, host: &str) -> Result<PathBuf, String> {
    let status = crate::shipped::shipped_build_in(host, src)
        .arg("--target-dir")
        .arg(target)
        .args(["--quiet", "--package", "mm-cli", "--bin", "mm"])
        .status()
        .map_err(|e| format!("cannot run cargo: {e}"))?;
    if !status.success() {
        return Err(format!(
            "shipped build of mm for {host} in {} failed",
            src.display()
        ));
    }
    Ok(target.join(host).join("release/mm"))
}

/// One program's gated count on `mm`: its output must be `want`. What a
/// process sees moves `startup`'s count [M]: about 22 instructions per
/// environment variable, one per character of the binary's path, and more
/// with the script's path. So every count runs the binary as
/// `/tmp/mm-native-ir-<run>/<job>/mm` (each part as long in every run),
/// on a copy of the program beside it named by its stem, from that
/// directory, with an empty environment, and a count is the same in every
/// run and on every machine with the same valgrind, glibc and CPU class
/// (the 0.0.2 performance review, section 17: 581,149 on startup.py at
/// 71b9c5b5 on two machines).
fn count(valgrind: &Path, mm: &Path, bench: &Path, stem: &str, want: &str) -> Result<u64, String> {
    let dir = scratch_dir()?;
    let output = (|| {
        let local = dir.join("mm");
        fs::hard_link(mm, &local)
            .or_else(|_| fs::copy(mm, &local).map(|_| ()))
            .map_err(|e| format!("cannot place {} at {}: {e}", mm.display(), local.display()))?;
        let script = format!("{stem}.py");
        fs::copy(bench.join(&script), dir.join(&script))
            .map_err(|e| format!("cannot copy {script}: {e}"))?;
        Command::new(valgrind)
            .args(["--tool=callgrind", "--trace-children=yes"])
            .arg(format!("--callgrind-out-file={}/%p.out", dir.display()))
            .args(GATED)
            .arg(&local)
            .arg("run")
            .arg(&script)
            .current_dir(&dir)
            .env_clear()
            .stdin(Stdio::null())
            .output()
            .map_err(|e| format!("cannot run {}: {e}", valgrind.display()))
    })();
    let _ = fs::remove_dir_all(&dir);
    let output = output?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() || stdout.trim() != want {
        return Err(format!(
            "{stem}.py on {} printed {:?} (want {want:?}): {stderr}",
            mm.display(),
            stdout.trim()
        ));
    }
    let total: u64 = stderr
        .lines()
        .filter_map(|l| l.split_once("Collected : "))
        .filter_map(|(_, n)| n.trim().parse::<u64>().ok())
        .sum();
    if total == 0 {
        return Err(format!(
            "{stem}.py: nothing collected from __libc_start_main (not a glibc binary?)"
        ));
    }
    Ok(total)
}

/// A new directory `/tmp/mm-native-ir-XXXXXXXX/NNNNN`, as long for every
/// count: the run's 8 hex digits and a 5-digit sequence number.
fn scratch_dir() -> Result<PathBuf, String> {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let n = NEXT.fetch_add(1, Ordering::Relaxed) % 100_000;
    let dir = scratch_root().join(format!("{n:05}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    Ok(dir)
}

/// This run's `/tmp/mm-native-ir-XXXXXXXX`, removed when the counts are in.
fn scratch_root() -> PathBuf {
    static RUN: OnceLock<String> = OnceLock::new();
    let run = RUN.get_or_init(|| {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.subsec_nanos());
        let seed = format!("{}-{nanos}", std::process::id());
        source_hash(seed.as_bytes())[..8].to_owned()
    });
    PathBuf::from(format!("/tmp/mm-native-ir-{run}"))
}

/// Prints the table against `other` (named `against`); returns the counts
/// past [`LIMITS`].
fn report(head: &Counts, (against, other): (&str, &Counts)) -> Vec<Failure> {
    println!("\n| Program | {against} (M Ir) | head (M Ir) | head / it |");
    println!("|---|---:|---:|---:|");
    let mut failures = Vec::new();
    for &(name, _) in &PROGRAMS {
        let (Some(&h), Some(&b)) = (head.get(name), other.get(name)) else {
            continue;
        };
        #[allow(clippy::cast_precision_loss, reason = "a ratio")]
        let ratio = h as f64 / b.max(1) as f64;
        #[allow(clippy::cast_precision_loss, reason = "millions, printed")]
        let (hm, bm) = (h as f64 / 1e6, b as f64 / 1e6);
        let mut row = format!("| {name} | {bm:.3} | {hm:.3} | {ratio:.4} |");
        if name == "startup" {
            let _ = write!(row, " {:+}", i128::from(h) - i128::from(b));
        }
        println!("{row}");
        if LIMITS.past(name, h, b) {
            failures.push(Failure {
                guest: None,
                name: name.to_owned(),
                ratio,
                text: format!("{name} {ratio:.4} against {against}"),
            });
        }
    }
    println!(
        "(limits: +0.5% a program, startup +{} instructions)",
        LIMITS.startup
    );
    failures
}

/// The cached counts in `path` under `key`, each kept only while its
/// program's source hash is the same.
fn load(path: &Path, key: &str, hashes: &BTreeMap<String, String>) -> Counts {
    let Ok(text) = fs::read_to_string(path) else {
        return Counts::new();
    };
    let mut lines = text.lines();
    if lines.next() != Some(&format!("# {key}")) {
        return Counts::new();
    }
    lines
        .filter_map(|line| {
            let mut f = line.split('\t');
            let name = f.next()?.to_owned();
            let n = f.next()?.parse().ok()?;
            (hashes.get(&name).map(String::as_str) == f.next()).then_some((name, n))
        })
        .collect()
}

fn save(
    path: &Path,
    key: &str,
    hashes: &BTreeMap<String, String>,
    counts: &Counts,
) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let mut text = format!("# {key}\n");
    for (name, n) in counts {
        let hash = hashes.get(name).map_or("", String::as_str);
        let _ = writeln!(text, "{name}\t{n}\t{hash}");
    }
    fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
}
