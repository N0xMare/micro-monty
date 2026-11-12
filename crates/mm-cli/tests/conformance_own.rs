//! Differential test: every `conformance/own/*.py` runs through `mm run` and
//! through CPython 3.14 (`$PYTHON`, else `~/.local/bin/python3.14`, else
//! `python3.14` on `PATH`); stdout, stderr and exit status must be identical
//! after replacing the case's path with `<file>`. Both run with `TZ=UTC`.
//! On macOS the cases in [`ORACLE_DIFFERS`] are not compared: CPython's
//! output there comes from the host C library.
//!
//! `MM_CASE=<substring>` runs only matching cases. `MM=<path>` runs another
//! `mm` binary instead of this package's: `cargo xtask check` runs the
//! cases on `mm` built as shipped (docs/spec.md §6), whose codegen
//! (`-Cpanic=immediate-abort`, `#[optimize(size)]` under `mm_shipped`)
//! no other native gate runs.

#![allow(clippy::expect_used, clippy::panic, reason = "test harness")]
#![allow(clippy::doc_markdown, reason = "the docs name CPython in prose")]

use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

fn mm() -> PathBuf {
    std::env::var_os("MM")
        .filter(|p| !p.is_empty())
        .map_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_mm")), PathBuf::from)
}

fn python() -> PathBuf {
    if let Some(p) = std::env::var_os("PYTHON") {
        return PathBuf::from(p);
    }
    if let Some(home) = std::env::var_os("HOME") {
        let p = Path::new(&home).join(".local/bin/python3.14");
        if p.exists() {
            return p;
        }
    }
    PathBuf::from("python3.14")
}

/// Cases whose CPython output depends on the host's C library, so CPython
/// on this host is not the oracle for them. mm's output is the same on
/// every host (no host libm or libc text); it matches CPython on Linux,
/// where these cases are compared.
#[cfg(target_os = "macos")]
const ORACLE_DIFFERS: &[(&str, &str)] = &[
    (
        "105_bigint.py",
        "macOS strerror(ERANGE) is 'Result too large'",
    ),
    (
        "52_builtin_error_messages.py",
        "macOS strerror(ERANGE) is 'Result too large'",
    ),
    (
        "107_math.py",
        "Apple libm differs from glibc in the last bit",
    ),
    (
        "202_complex_cmath.py",
        "Apple libm differs from glibc in the last bit",
    ),
    (
        "308_time_struct.py",
        "macOS gmtime names the zone UTC, glibc GMT",
    ),
    (
        "315_logging.py",
        "macOS gmtime names the zone UTC, glibc GMT",
    ),
    (
        "425_dunder_import.py",
        "CPython's sys.platform is darwin; mm's is linux on every host",
    ),
    (
        "442_float_special_values.py",
        "Apple libm differs from glibc in the last bit",
    ),
    (
        "456_numeric_edges_and_attribute_hints.py",
        "Apple libm differs from glibc in the last bit",
    ),
    (
        "458_number_specials_and_reordering.py",
        "Apple libm differs from glibc in the last bit",
    ),
    (
        "459_float_pow_negative_int_extremes.py",
        "macOS strerror(ERANGE) is 'Result too large'",
    ),
];
#[cfg(not(target_os = "macos"))]
const ORACLE_DIFFERS: &[(&str, &str)] = &[];

/// CPython, with the main thread's stack at its hard limit on macOS: the
/// 8 MiB default (64 MiB is the hard limit) is too small for CPython
/// 3.14's own recursion guard on the deep cases (324), where Linux passes.
fn oracle(py: &Path) -> Command {
    if cfg!(target_os = "macos") {
        let mut c = Command::new("/bin/sh");
        c.args(["-c", "ulimit -s hard && exec \"$0\" \"$@\""])
            .arg(py);
        c
    } else {
        Command::new(py)
    }
}

/// `f` on every item, spread over every core; the results in item order.
fn on_every_core<T: Sync, R: Send>(items: &[T], f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let next = AtomicUsize::new(0);
    let results: Vec<Mutex<Option<R>>> = items.iter().map(|_| Mutex::new(None)).collect();
    let workers = std::thread::available_parallelism().map_or(1, usize::from);
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(item) = items.get(i) else { break };
                    let result = f(item);
                    *results[i].lock().expect("result lock") = Some(result);
                }
            });
        }
    });
    results
        .into_iter()
        .map(|r| {
            r.into_inner()
                .expect("result lock")
                .expect("every item ran")
        })
        .collect()
}

struct Run {
    stdout: String,
    stderr: String,
    status: Option<i32>,
}

fn run(cmd: &mut Command, case: &Path) -> Run {
    // UTC on every host, as conformance/oracle.py sets it for the corpus.
    let out = cmd.env("TZ", "UTC").output().expect("failed to start");
    let path = case.to_string_lossy();
    let norm = |b: &[u8]| String::from_utf8_lossy(b).replace(path.as_ref(), "<file>");
    Run {
        stdout: norm(&out.stdout),
        stderr: norm(&out.stderr),
        status: out.status.code(),
    }
}

#[test]
fn own_cases_match_cpython() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../conformance/own");
    let root = root.canonicalize().expect("conformance/own exists");
    let filter = std::env::var("MM_CASE").unwrap_or_default();
    let mut cases: Vec<PathBuf> = std::fs::read_dir(&root)
        .expect("readable")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "py"))
        .filter(|p| p.to_string_lossy().contains(&filter))
        .collect();
    cases.sort();
    assert!(
        filter.is_empty() || !cases.is_empty(),
        "no case matches MM_CASE"
    );
    let py = python();
    let mut failures = Vec::new();
    // Both interpreters on every core, compared in name order.
    let runs = on_every_core(&cases, |case| {
        let mine = run(Command::new(mm()).arg("run").arg(case), case);
        if ORACLE_DIFFERS.iter().any(|(name, _)| case.ends_with(name)) {
            return (mine, None);
        }
        let theirs = run(
            oracle(&py)
                .arg("-X")
                .arg("utf8")
                .arg(case)
                .env("PYTHONHASHSEED", "0")
                .env_remove("PYTHONSTARTUP"),
            case,
        );
        (mine, Some(theirs))
    });
    for (case, (mine, theirs)) in cases.iter().zip(runs) {
        let Some(theirs) = theirs else {
            if let Some((_, why)) = ORACLE_DIFFERS.iter().find(|(name, _)| case.ends_with(name)) {
                eprintln!("{}: not compared on this host ({why})", case.display());
            }
            continue;
        };
        if mine.stdout != theirs.stdout
            || mine.stderr != theirs.stderr
            || mine.status != theirs.status
        {
            failures.push(format!(
                "== {}\n-- mm (exit {:?})\n{}{}-- python (exit {:?})\n{}{}",
                case.file_name()
                    .map(|n| n.to_string_lossy())
                    .unwrap_or_default(),
                mine.status,
                mine.stdout,
                mine.stderr,
                theirs.status,
                theirs.stdout,
                theirs.stderr
            ));
        }
    }
    eprintln!(
        "conformance/own: {}/{} cases match CPython",
        cases.len() - failures.len(),
        cases.len()
    );
    assert!(
        failures.is_empty(),
        "{} mismatches:\n{}",
        failures.len(),
        failures.join("\n")
    );
    if filter.is_empty() {
        assert!(cases.len() >= 40, "expected at least 40 cases");
    }
}
