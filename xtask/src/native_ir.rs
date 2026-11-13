//! `cargo xtask native-ir`: the bench programs' gated instruction counts on
//! the shipped native `mm` against the base's (AGENTS.md's Ir rule).
//!
//! Counts as `bench/callgrind.py` counts first: callgrind Ir from glibc's
//! `__libc_start_main` onward, without `pthread_getattr_np`. The head is
//! `$MM` (`cargo xtask check` passes the `shipped-mm` stage's binary) or
//! this tree's `mm` built as shipped; the base is the merge base with
//! `--base REV` (default `origin/main`), checked out in a worktree under
//! `target/native-ir/` and built as shipped with this checkout's recipe.
//! The base's counts are cached per commit, keyed by `rustc -vV`, the
//! valgrind version and each program's source, so an unmoved base is
//! neither built nor run again. A drop passes; a program's count rising
//! more than 0.5% fails, unless a commit since the base says why on a
//! `Native-IR-Accepted: <reason>` line, with the table, or
//! `MM_IR_ACCEPT=<reason>` does for an uncommitted change.

use std::{
    collections::BTreeMap,
    env,
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use crate::{
    bench::{PROGRAMS, read_expected},
    wasm_ir::{parallel, source_hash, version},
    wasm_wall::{checkout, git},
    workspace_root,
};

/// The most a program's count may rise.
const LIMIT: f64 = 0.005;
/// The line that accepts a rise in a commit since the base.
const ACCEPTED: &str = "Native-IR-Accepted:";
/// What a count holds, in the cache key: change it when that changes.
const COUNTING: &str = "gated callgrind Ir, shipped native mm, 1";
/// callgrind's collection toggles of the gated count.
const GATED: [&str; 2] = [
    "--toggle-collect=__libc_start_main*",
    "--toggle-collect=pthread_getattr_np*",
];

type Counts = BTreeMap<String, u64>;

pub(crate) fn run(args: &[String]) -> Result<(), String> {
    let base_rev = match args {
        [] => "origin/main",
        [flag, rev] if flag == "--base" => rev.as_str(),
        _ => return Err(crate::USAGE.to_owned()),
    };
    if !cfg!(target_os = "linux") {
        return Err("native-ir needs valgrind, which runs on Linux only".to_owned());
    }
    let valgrind = crate::wasm::tool("VALGRIND", "valgrind")
        .ok_or("native-ir needs valgrind ($VALGRIND, PATH); run .agents/setup")?;
    let root = workspace_root();
    let host = crate::host_target()?;
    let rustc = env::var("RUSTC").unwrap_or_else(|_| "rustc".to_owned());
    let key = format!(
        "{COUNTING}; {}; {}",
        crate::command_output(Path::new(&rustc), &["-vV"])?.replace('\n', "; "),
        version(&valgrind)
    );
    let bench = root.join("bench");
    let expected = read_expected(&bench.join("expected.txt"))?;
    let mut hashes = BTreeMap::new();
    for (name, stem) in PROGRAMS {
        let path = bench.join(format!("{stem}.py"));
        let bytes = fs::read(&path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        hashes.insert(name.to_owned(), source_hash(&bytes));
    }
    let dir = root.join("target/native-ir");
    let head_mm = match env::var_os("MM").filter(|v| !v.is_empty()) {
        Some(mm) => PathBuf::from(mm),
        None => build(&root, &root.join("target"), &host)?,
    };
    let base = git(&root, &["merge-base", base_rev, "HEAD"])?;
    let cache = dir.join("cache").join(format!("{base}.tsv"));
    let mut base_counts = load(&cache, &key, &hashes);
    let mut jobs: Vec<(bool, &str, &str, PathBuf)> = PROGRAMS
        .iter()
        .map(|&(name, stem)| (true, name, stem, head_mm.clone()))
        .collect();
    if base_counts.len() < PROGRAMS.len() {
        println!("native-ir: building the base, {base} (cached afterwards)");
        let tree = dir.join("base-src");
        checkout(&root, &tree, &base)?;
        let mm = build(&tree, &dir.join("base"), &host)?;
        jobs.extend(
            PROGRAMS
                .iter()
                .filter(|(name, _)| !base_counts.contains_key(*name))
                .map(|&(name, stem)| (false, name, stem, mm.clone())),
        );
    } else {
        println!("native-ir: base {base} counts cached");
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
    let mut head = Counts::new();
    for ((is_head, name, _, _), result) in jobs.iter().zip(results) {
        let n = result?;
        if *is_head {
            head.insert((*name).to_owned(), n);
        } else {
            base_counts.insert((*name).to_owned(), n);
        }
    }
    save(&cache, &key, &hashes, &base_counts)?;
    let failures = report(&head, &base_counts, &base);
    verdict(&root, &base, &failures)
}

/// Passes when no program rose past [`LIMIT`], or when a commit since
/// `base` (an [`ACCEPTED`] line) or `MM_IR_ACCEPT` says why.
fn verdict(root: &Path, base: &str, failures: &[String]) -> Result<(), String> {
    let mut accepted = env::var("MM_IR_ACCEPT").unwrap_or_default();
    if accepted.trim().is_empty() {
        let range = format!("{base}..HEAD");
        let grep = format!("--grep=^{ACCEPTED}");
        accepted = git(root, &["log", "--format=%h", &grep, &range])?;
    }
    if failures.is_empty() {
        println!("\nnative-ir: no program rose past +0.5%");
        Ok(())
    } else if !accepted.trim().is_empty() {
        println!(
            "\nnative-ir: past +0.5% ({}), accepted: {}",
            failures.join("; "),
            accepted.split_whitespace().collect::<Vec<_>>().join(", ")
        );
        Ok(())
    } else {
        Err(format!(
            "native-ir: gated Ir rose past +0.5%: {}; if the rise is intended, put the table \
             and a `{ACCEPTED} <reason>` line in the commit message (or rerun with \
             MM_IR_ACCEPT=<reason> before committing)",
            failures.join("; ")
        ))
    }
}

/// Builds `mm` of the tree at `src` as shipped for `host` into `target`.
fn build(src: &Path, target: &Path, host: &str) -> Result<PathBuf, String> {
    let status = crate::shipped_build(host)
        .current_dir(src)
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

/// Prints the table; returns the programs that rose past [`LIMIT`].
fn report(head: &Counts, base: &Counts, sha: &str) -> Vec<String> {
    let short = &sha[..sha.len().min(10)];
    println!("\n| Program | base {short} (M Ir) | head (M Ir) | head / base |");
    println!("|---|---:|---:|---:|");
    let mut failures = Vec::new();
    for (name, _) in PROGRAMS {
        let (h, b) = (head[name], base[name]);
        #[allow(clippy::cast_precision_loss, reason = "a ratio")]
        let ratio = h as f64 / b.max(1) as f64;
        #[allow(clippy::cast_precision_loss, reason = "millions, printed")]
        let (hm, bm) = (h as f64 / 1e6, b as f64 / 1e6);
        println!("| {name} | {bm:.3} | {hm:.3} | {ratio:.4} |");
        if ratio - 1.0 > LIMIT {
            failures.push(format!("{name} {ratio:.4}"));
        }
    }
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
