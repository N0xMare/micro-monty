//! `cargo xtask native-ir`: the bench programs' gated instruction counts on
//! the shipped native `mm` against a base and an anchor (AGENTS.md's Ir
//! rule).
//!
//! Counts as `bench/callgrind.py` counts first: callgrind Ir from glibc's
//! `__libc_start_main` onward, without `pthread_getattr_np`. The head is
//! `$MM` (`cargo xtask check` passes the `shipped-mm` stage's binary) or
//! this tree's `mm` built as shipped. The base is the merge base with
//! `--base REV` (default `origin/main`); the anchor is the last `v*` tag
//! the base descends from (`--anchor REV` names another), with every
//! first-parent commit after it up to the base that changed build inputs.
//! Each is checked out outside this checkout ([`crate::shipped::refs_dir`])
//! and built as shipped with its own toolchain and, when its xtask has
//! `shipped-mm`, its own recipe (else this checkout's); its counts are
//! cached per commit, keyed by that recipe and toolchain, the valgrind
//! version and each program's source.
//!
//! A drop passes. Against the base and against the anchor (each program's
//! least count from the anchor to the base), a program rising more than
//! 0.5%, or `startup` (about 0.58M) by more than [`STARTUP_LIMIT`]
//! instructions, fails unless an accept line covers it
//! ([`crate::accept`]): `Native-IR-Accepted: <programs> <ceiling>
//! <reason>` in a commit since the base passes those programs up to the
//! ceiling, and from the anchor to the base re-anchors them. A count the
//! base is already past against the anchor is inherited while the head
//! stays at the base's level (within [`NOISE`]): printed, not this
//! change's failure, and `--base` at the tag (`just pretag`) fails on it.
//! A head that adds to it fails against the anchor.

use std::{
    collections::BTreeMap,
    env,
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use crate::{
    accept::{self, Failure},
    bench::{PROGRAMS, read_expected},
    size,
    wasm_ir::{changed_since, parallel, source_hash, version},
    wasm_wall::{checkout, git},
    workspace_root,
};

/// The most a program's count may rise.
const LIMIT: f64 = 1.005;
/// The most `startup`'s count may rise, in instructions: 0.5% of its
/// 0.58M at 0.0.1, fixed, so a startup gain (lane I's 0.9478) does not
/// tighten it and the gate does not loosen as startup grows.
const STARTUP_LIMIT: u64 = 2_900;
/// How far a count the base is already past against the anchor may rise
/// over the base and still be the base's: the measurement's noise, as a
/// ratio for a program (two builds of one engine differ by at most 0.001%
/// [M]) and in instructions for `startup` (+585 [M], head and base built
/// in different directories).
const NOISE: (f64, u64) = (1.0005, 1_000);
/// The line that accepts a rise ([`crate::accept`]).
const ACCEPTED: &str = "Native-IR-Accepted:";
/// What a count holds, in the cache key: change it when that changes.
const COUNTING: &str = "gated callgrind Ir, shipped native mm, 2";
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
    let (mut base_rev, mut anchor_rev) = ("origin/main".to_owned(), None::<String>);
    if args.first().is_some_and(|a| a == "--plain") {
        return plain();
    }
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        let value = args
            .next()
            .cloned()
            .ok_or_else(|| format!("{arg} needs a revision"));
        match arg.as_str() {
            "--base" => base_rev = value?,
            "--anchor" => anchor_rev = Some(value?),
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
    let mut hashes = BTreeMap::new();
    for (name, stem) in PROGRAMS {
        let path = bench.join(format!("{stem}.py"));
        let bytes = fs::read(&path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        hashes.insert(name.to_owned(), source_hash(&bytes));
    }
    let dir = root.join("target/native-ir");
    let refs = crate::shipped::refs_dir("native-ir");
    let short = |s: &str| s[..s.len().min(10)].to_owned();
    let base = git(&root, &["merge-base", &base_rev, "HEAD"])?;
    let anchor = match &anchor_rev {
        Some(rev) => (
            rev.clone(),
            git(
                &root,
                &["rev-parse", "--verify", &format!("{rev}^{{commit}}")],
            )?,
        ),
        None => accept::tag_anchor(&root, &base, "native-ir")?,
    };
    // (label, tree, commit): the base, then the anchor and its history.
    let mut sides = vec![("base".to_owned(), "base", base.clone())];
    if anchor.1 != base {
        sides.push(("anchor".to_owned(), "anchor", anchor.1.clone()));
        for c in changed_since(&root, &anchor.1, &base)? {
            sides.push((format!("since-{}", short(&c)), "anchor", c));
        }
    }
    println!("native-ir: base = {base_rev} ({})", short(&base));
    if sides.len() > 1 {
        println!(
            "native-ir: anchor = {} ({}), {} commits since that changed build inputs",
            anchor.0,
            short(&anchor.1),
            sides.len() - 2
        );
    }
    let head_mm = match env::var_os("MM").filter(|v| !v.is_empty()) {
        Some(mm) => PathBuf::from(mm),
        None => crate::shipped_mm_binary()?,
    };
    let mut counts: Vec<Counts> = Vec::new();
    let mut jobs: Vec<(usize, &str, &str, PathBuf)> = PROGRAMS
        .iter()
        .map(|&(name, stem)| (0, name, stem, head_mm.clone()))
        .collect();
    counts.push(Counts::new());
    let mut keys = Vec::new();
    fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for (i, (label, tree_name, sha)) in sides.iter().enumerate() {
        let tree = refs.join(format!("{tree_name}-src"));
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
        let recipe = if own {
            checkout(&root, &tree, sha)?;
            size::recipe_key(&tree, &tree)?
        } else {
            checkout(&root, &tree, sha)?;
            size::recipe_key(&root, &tree)?
        };
        let key = format!("{COUNTING}; {recipe}; {}", version(&valgrind));
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
                build(&tree, &refs.join(tree_name), &host)?
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
        keys.push(key);
    }
    let scratch = dir.join("callgrind");
    let _ = fs::remove_dir_all(&scratch);
    fs::create_dir_all(&scratch).map_err(|e| format!("{}: {e}", scratch.display()))?;
    let results = parallel(jobs.len(), |j| {
        let (_, name, stem, ref mm) = jobs[j];
        let want = expected.get(name).map_or("", String::as_str);
        count(
            &valgrind,
            mm,
            &bench,
            stem,
            want,
            &scratch.join(format!("{j}.{stem}")),
        )
    });
    let _ = fs::remove_dir_all(&scratch);
    for ((i, name, _, _), result) in jobs.iter().zip(results) {
        counts[*i].insert((*name).to_owned(), result?);
    }
    for ((_, _, sha), (key, c)) in sides.iter().zip(keys.iter().zip(&counts[1..])) {
        save(
            &dir.join("cache").join(format!("{sha}.tsv")),
            key,
            &hashes,
            c,
        )?;
    }
    let mut failures = report(&counts[0], (&format!("base {}", short(&base)), &counts[1]));
    if sides.len() > 1 {
        let history: Vec<(&str, &Counts)> = sides[1..]
            .iter()
            .map(|(_, _, s)| s.as_str())
            .zip(&counts[2..])
            .chain([(base.as_str(), &counts[1])])
            .collect();
        let range = format!("{}..{base}", anchor.1);
        let accepts = accept::committed(&root, ACCEPTED, &range)?;
        let least = accept::least_since(&root, &history, &accepts, |k| (None, k.to_owned()))?;
        let name = format!("anchor {} to the base", anchor.0);
        for f in report(&counts[0], (&name, &least)) {
            // A count the base is already past is the base's, not this
            // change's, while the head does not add to it: `check` reports
            // it, and `--base` at the tag (just pretag) fails on it until
            // a line or a gain clears it.
            let (h, b, a) = (
                counts[0].get(&f.name).copied().unwrap_or_default(),
                counts[1].get(&f.name).copied().unwrap_or_default(),
                least.get(&f.name).copied().unwrap_or_default(),
            );
            if inherited(&f.name, h, b, a) {
                println!(
                    "native-ir: {}: inherited, the base is past it already (base {b}, anchor \
                     {a}); `native-ir --base {}` fails on it",
                    f.text, anchor.0
                );
            } else if past(&f.name, b, a) {
                failures.push(Failure {
                    text: format!(
                        "{} (the base is past it already, and the head adds {:+} to it)",
                        f.text,
                        i128::from(h) - i128::from(b)
                    ),
                    ..f
                });
            } else {
                failures.push(f);
            }
        }
    }
    let accepts = accept::for_change(&root, ACCEPTED, &base, "MM_IR_ACCEPT")?;
    accept::verdict("native-ir", ACCEPTED, "MM_IR_ACCEPT", &failures, &accepts)
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
    let scratch = dir.join("callgrind-plain");
    let _ = fs::remove_dir_all(&scratch);
    fs::create_dir_all(&scratch).map_err(|e| format!("{}: {e}", scratch.display()))?;
    let results = parallel(PROGRAMS.len() * 2, |j| {
        let (name, stem) = PROGRAMS[j / 2];
        let mm = if j % 2 == 0 { &shipped } else { &plain };
        let want = expected.get(name).map_or("", String::as_str);
        count(
            &valgrind,
            mm,
            &bench,
            stem,
            want,
            &scratch.join(format!("{j}.{stem}")),
        )
    });
    let _ = fs::remove_dir_all(&scratch);
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

/// `mm run bench/STEM.py`'s gated count; its output must be `want`.
fn count(
    valgrind: &Path,
    mm: &Path,
    bench: &Path,
    stem: &str,
    want: &str,
    prefix: &Path,
) -> Result<u64, String> {
    let output = Command::new(valgrind)
        .args(["--tool=callgrind", "--trace-children=yes"])
        .arg(format!("--callgrind-out-file={}.%p.out", prefix.display()))
        .args(GATED)
        .arg(mm)
        .arg("run")
        .arg(bench.join(format!("{stem}.py")))
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("cannot run {}: {e}", valgrind.display()))?;
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

/// Prints the table against `other` (named `against`); returns the counts
/// past [`LIMIT`] or [`STARTUP_LIMIT`].
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
        if past(name, h, b) {
            failures.push(Failure {
                guest: None,
                name: name.to_owned(),
                ratio,
                text: format!("{name} {ratio:.4} against {against}"),
            });
        }
    }
    println!("(limits: +0.5% a program, startup +{STARTUP_LIMIT} instructions)");
    failures
}

/// Whether `name`'s count past the anchor's (`anchor`) is the base's
/// excess, which the head (`head`) does not add to beyond [`NOISE`].
fn inherited(name: &str, head: u64, base: u64, anchor: u64) -> bool {
    #[allow(clippy::cast_precision_loss, reason = "a ratio")]
    let stays = if name == "startup" {
        head <= base + NOISE.1
    } else {
        head as f64 <= base as f64 * NOISE.0
    };
    past(name, base, anchor) && stays
}

/// Whether `head` is past the limit against `other` for `name`.
fn past(name: &str, head: u64, other: u64) -> bool {
    if name == "startup" {
        head > other + STARTUP_LIMIT
    } else {
        #[allow(clippy::cast_precision_loss, reason = "a ratio")]
        let ratio = head as f64 / other.max(1) as f64;
        ratio > LIMIT
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_inherited_excess_cannot_grow() {
        // The base's startup is +4,464 over the anchor: a head at the
        // base's level is the base's excess, one at +7,000 adds +2,536.
        let a = 581_004;
        assert!(inherited("startup", a + 4_464 + 585, a + 4_464, a));
        assert!(!inherited("startup", a + 7_000, a + 4_464, a));
        // dict at 1.0076 over the anchor: +0.03% over the base is noise,
        // +0.2% adds to it.
        let (a, b) = (1_160_658_603, 1_169_461_072);
        assert!(inherited("dict", b + b / 3_000, b, a));
        assert!(!inherited("dict", b + b / 500, b, a));
        // A base within the limit is no excess to inherit.
        assert!(!inherited("fib", 1_000, 1_000, 999));
    }
}
