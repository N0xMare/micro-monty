//! `cargo xtask overflow-wasip1`: every own, corpus, codemode and breadth
//! case run by `mm` for `wasm32-wasip1` built with overflow checks, under
//! wasmtime. The shipped build wraps an integer overflow silently, so a
//! shift or a size computation past 32 bits can give a wrong value on
//! wasm32 that the native build never sees (`2f9b8584`, J1); here it
//! panics. Fails on any panic or wasm trap; what the case prints or
//! raises is not checked (conformance does that). Release gate only.
//!
//! It builds into its own target directory, `target/check/overflow-wasip1`:
//! overflow checks change codegen, so sharing `target/` would rebuild the
//! shipped wasip1 `mm` the `wasm`, `conformance-wasip1` and
//! `durability-cross` stages use.

use std::{
    env, fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use crate::workspace_root;

/// Wall time one case may take under wasmtime.
const CASE_TIMEOUT: Duration = Duration::from_mins(10);

pub(crate) fn run() -> Result<(), String> {
    let started = Instant::now();
    let root = workspace_root();
    let target_dir = root.join("target/check/overflow-wasip1");
    let wasmtime = crate::wasm::tool("WASMTIME", "wasmtime").ok_or(
        "overflow-wasip1 needs wasmtime ($WASMTIME, ~/.local/bin, PATH); run .agents/setup",
    )?;
    let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned());
    let status = Command::new(cargo)
        .args([
            "build",
            "--release",
            "--quiet",
            "--locked",
            "--package",
            "mm-cli",
        ])
        .args(["--target", "wasm32-wasip1"])
        .env("CARGO_PROFILE_RELEASE_OVERFLOW_CHECKS", "true")
        .env("CARGO_TARGET_DIR", &target_dir)
        .current_dir(&root)
        .status()
        .map_err(|e| format!("cannot run cargo: {e}"))?;
    if !status.success() {
        return Err("overflow-checked build of mm for wasm32-wasip1 failed".to_owned());
    }
    let release = target_dir.join("wasm32-wasip1/release");
    let module = release.join("mm.cwasm");
    let status = Command::new(&wasmtime)
        .arg("compile")
        .arg(release.join("mm.wasm"))
        .arg("-o")
        .arg(&module)
        .status()
        .map_err(|e| format!("cannot run {}: {e}", wasmtime.display()))?;
    if !status.success() {
        return Err("wasmtime compile of the overflow-checked mm.wasm failed".to_owned());
    }

    let mut cases = Vec::new();
    for dir in ["own", "monty", "codemode", "breadth"] {
        collect_cases(&root.join("conformance").join(dir), &mut cases)?;
    }
    cases.sort();
    let next = AtomicUsize::new(0);
    let failures = Mutex::new(Vec::new());
    let workers = thread::available_parallelism().map_or(1, usize::from);
    thread::scope(|scope| {
        for w in 0..workers {
            let (cases, next, failures) = (&cases, &next, &failures);
            let (wasmtime, module) = (&wasmtime, &module);
            let stderr_file = target_dir.join(format!("stderr-{w}.txt"));
            scope.spawn(move || {
                while let Some(case) = cases.get(next.fetch_add(1, Ordering::Relaxed)) {
                    if let Some(why) = run_case(wasmtime, module, case, &stderr_file) {
                        let shown = case.strip_prefix(workspace_root()).unwrap_or(case);
                        let mut f = failures
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        f.push(format!("{}: {why}", shown.display()));
                    }
                }
            });
        }
    });
    let mut failures = failures
        .into_inner()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    failures.sort();
    println!(
        "overflow-wasip1: {} cases, {} failed, {:.1} s",
        cases.len(),
        failures.len(),
        started.elapsed().as_secs_f64()
    );
    for f in &failures {
        println!("  FAIL {f}");
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "overflow-wasip1: {} cases panicked or trapped",
            failures.len()
        ))
    }
}

fn collect_cases(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = fs::read_dir(dir).map_err(|e| format!("cannot read {}: {e}", dir.display()))?;
    for entry in entries {
        let path = entry
            .map_err(|e| format!("cannot read {}: {e}", dir.display()))?
            .path();
        if path.is_dir() {
            collect_cases(&path, out)?;
        } else if path.extension().is_some_and(|x| x == "py") {
            out.push(path);
        }
    }
    Ok(())
}

/// Runs `mm run CASE` under wasmtime; why it failed, if it panicked,
/// trapped or ran past `CASE_TIMEOUT`. Its stderr goes to a file, so a
/// long traceback cannot fill a pipe while it runs. (Not "unreachable":
/// Python's own errors can say it.)
fn run_case(wasmtime: &Path, module: &Path, case: &Path, stderr_file: &Path) -> Option<String> {
    let dir = case.parent()?.display().to_string();
    let stderr = match fs::File::create(stderr_file) {
        Ok(f) => f,
        Err(e) => return Some(format!("cannot create {}: {e}", stderr_file.display())),
    };
    let child = Command::new(wasmtime)
        .args([
            "run",
            "--allow-precompiled",
            "--dir",
            &format!("{dir}::{dir}"),
        ])
        .args(["--env", "TZ=UTC"])
        .arg(module)
        .arg("run")
        .arg(case)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(stderr)
        .spawn();
    let mut child = match child {
        Ok(c) => c,
        Err(e) => return Some(format!("cannot run wasmtime: {e}")),
    };
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if started.elapsed() > CASE_TIMEOUT => {
                let _ = child.kill();
                let _ = child.wait();
                return Some(format!("ran past {} s", CASE_TIMEOUT.as_secs()));
            }
            Ok(None) => thread::sleep(Duration::from_millis(5)),
            Err(e) => return Some(format!("cannot wait for wasmtime: {e}")),
        }
    }
    let text = fs::read_to_string(stderr_file).unwrap_or_default();
    text.lines()
        .find(|l| l.contains("panicked") || l.contains("wasm trap"))
        .map(str::to_owned)
}
