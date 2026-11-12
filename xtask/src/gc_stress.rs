//! `cargo xtask gc-stress`: the GC rooting test. With `MM_GC_STRESS=1`,
//! `mm` collects garbage at every safepoint (`Session::set_gc_stress`), so
//! a value no root reaches is reclaimed at the first chance instead of
//! surviving until the next allocation-driven collection. Over 1 MiB live
//! it collects at the first safepoint after each 1/64 of the live bytes
//! is allocated: collecting a few MiB at every one of millions of
//! safepoints took own cases 425, 427, 463 and 466 hours each, and 249 of
//! the 260 own cases never pass 1 MiB [M].
//!
//! - every `conformance/own` case: `mm run` under stress must print exactly
//!   what it prints without;
//! - the corpus: `conformance --engine micro-monty --check` under stress
//!   (mm timeout 600 s), so every case keeps its committed outcome;
//! - durability on the own cases under stress (mm timeout 3,600 s): every
//!   park snapshotted and continued in a new process, collecting at every
//!   safepoint of each.
//!
//! `--incremental` runs the same phases with `MM_GC_STRESS=inc`: an
//! incremental mark in progress at every safepoint, a few objects per step,
//! each checked against a full mark from the same roots when it ends
//! (`Session::set_gc_inc_stress`), so a write the segment flags miss aborts.

use std::{
    env,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
    time::Instant,
};

use crate::{micro_monty_binary, workspace_root};

pub(crate) fn run(args: &[String]) -> Result<(), String> {
    let stress = match args {
        [] => "1",
        [flag] if flag == "--incremental" => "inc",
        _ => return Err("usage: cargo xtask gc-stress [--incremental]".to_owned()),
    };
    let mm = micro_monty_binary()?;
    let started = Instant::now();
    own_cases(&mm, stress)?;
    println!(
        "gc-stress: own cases identical under stress ({:.0} s)",
        started.elapsed().as_secs_f64()
    );
    let xtask = env::current_exe().map_err(|error| format!("cannot find xtask: {error}"))?;
    for task in [
        &[
            "conformance",
            "--engine",
            "micro-monty",
            "--check",
            "--timeout",
            "600",
        ][..],
        // Per `mm` process; the unparked reference run of a case is one
        // process. Under stress the deep comparisons of own cases 310, 324
        // and 331 collect at every safepoint and take 449 s, 517 s and
        // 1,598 s unparked [M]; 3,600 s leaves headroom, so every case
        // runs and none is excluded.
        &[
            "durability",
            "--filter",
            "conformance/own/",
            "--timeout",
            "3600",
        ][..],
    ] {
        let phase = Instant::now();
        println!("gc-stress: MM_GC_STRESS={stress} xtask {}", task.join(" "));
        let status = Command::new(&xtask)
            .args(task)
            .env("MM_GC_STRESS", stress)
            .env("MM", &mm)
            .status()
            .map_err(|error| format!("cannot run xtask: {error}"))?;
        if !status.success() {
            return Err(format!("under GC stress: xtask {} failed", task.join(" ")));
        }
        println!(
            "gc-stress: {} passed ({:.0} s)",
            task[0],
            phase.elapsed().as_secs_f64()
        );
    }
    println!(
        "gc-stress: every phase passed in {:.0} s",
        started.elapsed().as_secs_f64()
    );
    Ok(())
}

/// Runs every own case with and without stress; their outputs must match.
fn own_cases(mm: &Path, stress: &str) -> Result<(), String> {
    let dir = workspace_root().join("conformance/own");
    let mut cases: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map_err(|error| format!("cannot read {}: {error}", dir.display()))?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|x| x == "py"))
        .collect();
    cases.sort();
    let next = AtomicUsize::new(0);
    let failures = Mutex::new(Vec::new());
    let workers = thread::available_parallelism().map_or(1, usize::from);
    thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                while let Some(case) = cases.get(next.fetch_add(1, Ordering::Relaxed)) {
                    let plain = run_mm(mm, case, None);
                    let stressed = run_mm(mm, case, Some(stress));
                    if plain != stressed
                        && let Ok(mut failures) = failures.lock()
                    {
                        failures.push(format!(
                            "{}:\n-- without stress\n{}\n-- with stress\n{}",
                            case.display(),
                            plain,
                            stressed
                        ));
                    }
                }
            });
        }
    });
    let failures = failures.into_inner().unwrap_or_default();
    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "{} own cases differ under GC stress:\n{}",
            failures.len(),
            failures.join("\n")
        ))
    }
}

/// `mm run case`: exit status, stdout and stderr as one text.
fn run_mm(mm: &Path, case: &Path, stress: Option<&str>) -> String {
    let mut command = Command::new(mm);
    command.arg("run").arg(case);
    match stress {
        Some(stress) => command.env("MM_GC_STRESS", stress),
        None => command.env_remove("MM_GC_STRESS"),
    };
    match command.output() {
        Ok(Output {
            status,
            stdout,
            stderr,
        }) => format!(
            "{status}\n{}{}",
            String::from_utf8_lossy(&stdout),
            String::from_utf8_lossy(&stderr)
        ),
        Err(error) => format!("cannot run mm: {error}"),
    }
}
