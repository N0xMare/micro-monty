//! `cargo xtask wasm-ir`: wasmtime's guest instructions on the bench
//! programs against a base revision.
//!
//! Builds the working tree (or `--head`) and `--base` (default
//! `origin/main`), each revision checked out in a worktree under
//! `target/wasm-ir/`, as shipped: `mm` for
//! `wasm32-wasip1`, precompiled by `wasmtime compile --target` for this
//! architecture's baseline ISA, not this host's CPU, so the code and the
//! counts are the same on every machine. Each program runs once per build
//! under `valgrind --tool=callgrind --dump-instr=yes` as `wasmtime run
//! --profile=perfmap mm run PROGRAM`; the count is the instructions
//! executed at addresses in wasmtime's perf map, the guest's compiled code
//! only (wasmtime's runtime, the host calls and the loader left out). The
//! output must equal the program's line in `bench/expected.txt`.
//!
//! Unlike `wasm-wall`, the count does not depend on load or on the host,
//! so one run decides. It catches what wasm-opt and Cranelift do to the
//! run loop's code (a spill, an inlining flip) at the commit that does
//! it, either way; it does not see layout costs that leave the executed
//! instructions alone. The gate fails when a program's count rises more
//! than 1.0% (`PROGRAM_LIMIT`) or the geometric mean of the ratios more
//! than 0.5% (`GEOMEAN_LIMIT`): unrelated commits move the run loop's
//! register allocation by up to about 1% either way. Fewer instructions
//! always pass, with the table printed. `MM_WASM_IR_ACCEPT` passes an
//! intended rise, whose commit message records the table and the reason.
//! Drift built up from commits each under the limits is caught at a
//! release, against the previous tag (AGENTS.md).

use std::{
    env, fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Mutex, PoisonError},
    thread,
};

use crate::{
    bench::{PROGRAMS, read_expected},
    wasm_wall::{build_mm, checkout, head_id},
    workspace_root,
};

/// The most one program's count may rise against the base's.
const PROGRAM_LIMIT: f64 = 1.010;
/// The most the geometric mean of the programs' ratios may rise.
const GEOMEAN_LIMIT: f64 = 1.005;

pub(crate) fn run(args: &[String]) -> Result<(), String> {
    let mut base = "origin/main".to_owned();
    let mut head: Option<String> = None;
    let mut only: Option<Vec<String>> = None;
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--base" => base.clone_from(args.next().ok_or("--base needs a revision")?),
            "--head" => head = Some(args.next().ok_or("--head needs a revision")?.clone()),
            "--programs" => {
                let list = args.next().ok_or("--programs needs a list")?;
                only = Some(list.split(',').map(str::to_owned).collect());
            }
            other => return Err(format!("wasm-ir: unknown option `{other}`")),
        }
    }
    let programs: Vec<(&str, &str)> = match &only {
        None => PROGRAMS.to_vec(),
        Some(names) => names
            .iter()
            .map(|n| {
                PROGRAMS
                    .iter()
                    .find(|(name, _)| name == n)
                    .copied()
                    .ok_or_else(|| format!("wasm-ir: no bench program `{n}`"))
            })
            .collect::<Result<_, _>>()?,
    };
    if !cfg!(target_os = "linux") {
        return Err("wasm-ir needs valgrind, which runs on Linux only".to_owned());
    }
    let valgrind = crate::wasm::tool("VALGRIND", "valgrind")
        .ok_or("wasm-ir needs valgrind ($VALGRIND, PATH); run .agents/setup")?;
    let wasmtime = crate::wasm::tool("WASMTIME", "wasmtime")
        .ok_or("wasm-ir needs wasmtime ($WASMTIME, ~/.local/bin, PATH); run .agents/setup")?;
    let root = workspace_root();
    let dir = root.join("target/wasm-ir");
    let base_tree = dir.join("base-src");
    let sha = checkout(&root, &base_tree, &base)?;
    let (head_tree, head_name) = match &head {
        Some(rev) => {
            let tree = dir.join("head-src");
            let sha = checkout(&root, &tree, rev)?;
            (tree, format!("{rev} ({})", &sha[..sha.len().min(10)]))
        }
        None => (root.clone(), format!("working tree ({})", head_id(&root)?)),
    };
    println!(
        "wasm-ir: head = {head_name}, base = {base} ({})",
        &sha[..sha.len().min(10)]
    );
    let isa = format!("{}-unknown-linux-gnu", env::consts::ARCH);
    let builds = [
        build_mm(&head_tree, &dir.join("head"), &wasmtime, Some(&isa))?,
        build_mm(&base_tree, &dir.join("base"), &wasmtime, Some(&isa))?,
    ];
    let bench = root.join("bench");
    let expected = read_expected(&bench.join("expected.txt"))?;
    let scratch = dir.join("callgrind");
    fs::create_dir_all(&scratch).map_err(|error| format!("{}: {error}", scratch.display()))?;

    let jobs: Vec<(usize, &str, &str)> = programs
        .iter()
        .flat_map(|&(name, stem)| [(0, name, stem), (1, name, stem)])
        .collect();
    let mut counts = parallel(jobs.len(), |i| {
        let (which, name, stem) = jobs[i];
        let label = format!("{}.{stem}", ["head", "base"][which]);
        count(
            &valgrind,
            &wasmtime,
            &builds[which],
            &bench,
            stem,
            expected.get(name).map_or("", String::as_str),
            &scratch.join(label),
        )
    })
    .into_iter();
    let mut pairs = Vec::new();
    for &(name, _) in &programs {
        let (head, base) = (counts.next(), counts.next());
        let (Some(head), Some(base)) = (head, base) else {
            return Err(format!("{name}: not run"));
        };
        pairs.push((name, head?, base?));
    }
    verdict(&base, &report(&pairs))
}

/// Passes when nothing rose past a limit, or `MM_WASM_IR_ACCEPT` says why
/// it did.
fn verdict(base: &str, failures: &[String]) -> Result<(), String> {
    let accepted = env::var("MM_WASM_IR_ACCEPT").unwrap_or_default();
    if failures.is_empty() {
        println!("\nwasm-ir: within the limits");
        Ok(())
    } else if !accepted.trim().is_empty() {
        println!(
            "\nwasm-ir: past the limits ({}), accepted: {}",
            failures.join("; "),
            accepted.trim()
        );
        Ok(())
    } else {
        Err(format!(
            "wasm-ir: more guest instructions than {base}: {}; if the rise is \
             intended, rerun with MM_WASM_IR_ACCEPT=<reason> and record the \
             table and the reason in the commit message",
            failures.join("; ")
        ))
    }
}

/// `job(0..n)` on at most one thread per CPU (a count does not depend on
/// load); the results in order.
fn parallel<T: Send>(n: usize, job: impl Fn(usize) -> T + Sync) -> Vec<T> {
    let next = Mutex::new(0_usize);
    let done = Mutex::new(Vec::with_capacity(n));
    let workers = thread::available_parallelism()
        .map_or(1, usize::from)
        .min(n);
    thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                loop {
                    let i = {
                        let mut at = next.lock().unwrap_or_else(PoisonError::into_inner);
                        *at += 1;
                        *at - 1
                    };
                    if i >= n {
                        break;
                    }
                    let result = job(i);
                    done.lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .push((i, result));
                }
            });
        }
    });
    let mut done = done.into_inner().unwrap_or_else(PoisonError::into_inner);
    done.sort_by_key(|(i, _)| *i);
    done.into_iter().map(|(_, result)| result).collect()
}

/// Prints the table; returns the limits exceeded.
fn report(pairs: &[(&str, u64, u64)]) -> Vec<String> {
    println!("\nwasmtime guest instructions, millions (base, head), ratio head / base");
    let mut failures = Vec::new();
    let mut product = 1.0_f64;
    for &(name, head, base) in pairs {
        let ratio = ratio(head, base);
        let over = ratio > PROGRAM_LIMIT;
        println!(
            "  {name:<11} {:>10.3} {:>10.3}  {ratio:.4}{}",
            millions(base),
            millions(head),
            if over { "  > limit" } else { "" }
        );
        product *= ratio;
        if over {
            failures.push(format!("{name} {ratio:.4} > {PROGRAM_LIMIT}"));
        }
    }
    let count = i32::try_from(pairs.len()).unwrap_or(i32::MAX).max(1);
    let geomean = product.powf(1.0 / f64::from(count));
    println!("  geomean     {geomean:.4}");
    if geomean > GEOMEAN_LIMIT {
        failures.push(format!("geomean {geomean:.4} > {GEOMEAN_LIMIT}"));
    }
    failures
}

#[allow(clippy::cast_precision_loss, reason = "a ratio of counts")]
fn ratio(head: u64, base: u64) -> f64 {
    head as f64 / base.max(1) as f64
}

#[allow(clippy::cast_precision_loss, reason = "display")]
fn millions(n: u64) -> f64 {
    n as f64 / 1e6
}

/// One `mm run bench/STEM.py` under callgrind; its guest instructions.
/// The callgrind file and wasmtime's perf map are removed once read.
fn count(
    valgrind: &Path,
    wasmtime: &Path,
    mm: &Path,
    bench: &Path,
    stem: &str,
    want: &str,
    prefix: &Path,
) -> Result<u64, String> {
    let output = Command::new(valgrind)
        .args(["--tool=callgrind", "--dump-instr=yes"])
        .arg(format!("--callgrind-out-file={}.%p.cg", prefix.display()))
        .arg(wasmtime)
        .args(["run", "--profile=perfmap", "--allow-precompiled", "--dir"])
        .arg(format!("{}::/b", bench.display()))
        .arg(mm)
        .args(["run", &format!("/b/{stem}.py")])
        .stdin(Stdio::null())
        .output()
        .map_err(|error| format!("cannot run {}: {error}", valgrind.display()))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() || stdout.trim() != want {
        return Err(format!(
            "{stem}.py on {} printed {:?} (want {want:?}): {stderr}",
            mm.display(),
            stdout.trim()
        ));
    }
    // valgrind runs in the process it starts: `==PID==` heads its lines.
    let pid = stderr
        .split("==")
        .nth(1)
        .filter(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
        .ok_or_else(|| format!("{stem}.py: no pid in valgrind's output"))?;
    let cg = PathBuf::from(format!("{}.{pid}.cg", prefix.display()));
    let map = PathBuf::from(format!("/tmp/perf-{pid}.map"));
    let read =
        |p: &Path| fs::read_to_string(p).map_err(|error| format!("{}: {error}", p.display()));
    let counted = match guest(&read(&cg)?, &read(&map)?) {
        Some(n) if n > 0 => Ok(n),
        _ => Err(format!(
            "{stem}.py: no guest instructions in {} for the perf map {}",
            cg.display(),
            map.display()
        )),
    };
    let _ = fs::remove_file(&cg);
    let _ = fs::remove_file(&map);
    counted
}

/// Instructions in `cg` (callgrind, `--dump-instr=yes`, events `Ir`) at
/// addresses inside a function of `map` (a perf map: `0xADDR SIZE NAME`,
/// hex). The shipped module has no name section, so the functions are only
/// indices, which move between builds: only the total compares.
fn guest(cg: &str, map: &str) -> Option<u64> {
    let hex = |s: &str| u64::from_str_radix(s.strip_prefix("0x").unwrap_or(s), 16).ok();
    let mut functions: Vec<(u64, u64)> = map
        .lines()
        .filter_map(|line| {
            let mut f = line.split(' ');
            let start = hex(f.next()?)?;
            Some((start, start.checked_add(hex(f.next()?)?)?))
        })
        .collect();
    functions.sort_unstable();
    let mut total = 0;
    let mut last = [0_u64; 2];
    // The cost line after `calls=` is the call's inclusive cost: skipped.
    let mut call = false;
    for line in cg.lines() {
        if line.starts_with("calls=") {
            call = true;
            continue;
        }
        if !line
            .bytes()
            .next()
            .is_some_and(|c| c.is_ascii_digit() || matches!(c, b'+' | b'-' | b'*'))
        {
            continue;
        }
        let mut f = line.split_ascii_whitespace();
        let (addr, ln, ir) = (f.next()?, f.next()?, f.next()?);
        last = [position(addr, last[0])?, position(ln, last[1])?];
        if core::mem::take(&mut call) {
            continue;
        }
        let at = functions.partition_point(|(start, _)| *start <= last[0]);
        if at.checked_sub(1).is_some_and(|i| last[0] < functions[i].1) {
            total += ir.parse::<u64>().ok()?;
        }
    }
    Some(total)
}

/// A callgrind position: absolute (`0x` hex or decimal), `+N` or `-N`
/// relative to the previous one, or `*`, the same.
fn position(s: &str, last: u64) -> Option<u64> {
    let number = |s: &str| match s.strip_prefix("0x") {
        Some(hex) => u64::from_str_radix(hex, 16).ok(),
        None => s.parse().ok(),
    };
    match s.as_bytes().first()? {
        b'*' => Some(last),
        b'+' => last.checked_add(number(&s[1..])?),
        b'-' => last.checked_sub(number(&s[1..])?),
        _ => number(s),
    }
}
